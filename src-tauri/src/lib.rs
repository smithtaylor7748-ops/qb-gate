//! ClaudeGate —— Claude 环境控制面板。
//!
//! 命令层。业务逻辑全在各自模块里，这里只做参数搬运与状态持有。

pub mod accounts;
pub mod error;
pub mod events;
pub mod gate;
pub mod install;
pub mod legacy;
pub mod killswitch;
pub mod launch;
pub mod plugins;
pub mod probe;
pub mod progress;
pub mod process;
pub mod relay;
pub mod sysenv;
pub mod update;

use error::Result;
use events::Reporter;
use std::sync::Arc;

pub struct AppState {
    pub gate: Arc<gate::GateState>,
    pub watchdog_stop: std::sync::Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    pub tz: std::sync::Mutex<Option<sysenv::TzState>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            gate: Arc::new(gate::GateState::default()),
            watchdog_stop: std::sync::Mutex::new(None),
            tz: std::sync::Mutex::new(None),
        }
    }
}

// ------------------------------------------------------------------ 门禁

#[tauri::command]
async fn gate_status(state: tauri::State<'_, AppState>) -> Result<gate::GateStatus> {
    let allow = gate::allowlist::read().unwrap_or_default();
    let current_ip = probe::ip::public_ip().await.ok();
    let targets = gate::collect_targets();
    Ok(gate::GateStatus {
        ip_allowed: current_ip
            .as_deref()
            .is_some_and(|ip| gate::allowlist::contains(&allow, ip)),
        all_locked: !targets.is_empty() && targets.iter().all(|t| t.locked),
        lease: state.gate.lease.lock().unwrap().clone(),
        watchdog_running: *state.gate.watchdog_running.lock().unwrap(),
        stale_copies: gate::targets::stale_copies(),
        recent_log: gate::log::tail(20),
        current_ip,
        allowlist: allow,
        targets,
    })
}

#[tauri::command]
fn gate_lock_all() -> Result<usize> {
    gate::lock_all()
}

/// 应急解锁：无条件摘掉所有 Deny ACE，**不验 IP**。
///
/// 这是故意留的逃生口。门禁的正常入口 `gate_open` 要求出口 IP 在白名单里，
/// 可是「白名单是空的」「查不到公网 IP」「填错了 IP」这几种情况都会让它
/// 永远过不了 —— 那时候 claude.exe 是锁着的，用户就被自己的工具关在门外。
///
/// 安全上不吃亏：能点这个按钮的人本来就能改白名单文件、也能自己改 ACL。
/// 门禁防的是「跑起来之后出口 IP 悄悄变了」，不是防本机管理员。
#[tauri::command]
fn gate_unlock_all() -> Result<usize> {
    let n = gate::unlock_all()?;
    gate::log::write("应急解锁：已摘掉全部执行锁（未验证 IP）");
    Ok(n)
}

#[tauri::command]
async fn gate_open(holder: String, state: tauri::State<'_, AppState>) -> Result<()> {
    gate::open_authorized(&holder, &state.gate).await
}

#[tauri::command]
fn gate_release(state: tauri::State<'_, AppState>) -> Result<()> {
    gate::release_lease(&state.gate)
}

#[tauri::command]
fn gate_clean_stale() -> Vec<(std::path::PathBuf, bool)> {
    gate::clean_stale_copies()
}

#[tauri::command]
fn allowlist_read() -> Result<Vec<String>> {
    gate::allowlist::read()
}

#[tauri::command]
fn allowlist_write(entries: Vec<String>) -> Result<()> {
    gate::allowlist::write(&entries)
}

#[tauri::command]
async fn allowlist_add_current() -> Result<Vec<String>> {
    let ip = probe::ip::public_ip().await?;
    let mut all = gate::allowlist::read().unwrap_or_default();
    if !all.contains(&ip) {
        all.push(ip);
        gate::allowlist::write(&all)?;
    }
    Ok(all)
}

#[tauri::command]
fn watchdog_start(
    mode: gate::watchdog::WatchMode,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    start_watchdog(mode, state);
    Ok(())
}

/// 起看门狗并把停止句柄挂到 AppState 上。
///
/// 每次都换一个新的 watch channel：旧的那个 sender 被 take 走之后，
/// 上一轮的循环下次醒来会自己退出。
fn start_watchdog(mode: gate::watchdog::WatchMode, state: tauri::State<'_, AppState>) {
    let (tx, rx) = tokio::sync::watch::channel(false);
    if let Some(old) = state.watchdog_stop.lock().unwrap().replace(tx) {
        let _ = old.send(true);
    }
    let gs = state.gate.clone();
    tauri::async_runtime::spawn(async move {
        gate::run_watchdog(mode, gs, rx).await;
    });
}

#[tauri::command]
fn watchdog_stop(state: tauri::State<'_, AppState>) {
    if let Some(tx) = state.watchdog_stop.lock().unwrap().take() {
        let _ = tx.send(true);
    }
}

/// 验 IP → 解锁 → **真的把进程拉起来** → 挂看门狗。
///
/// `gate_open` 只解锁不启动，所以旧界面上那两个「启动 …」按钮其实一个进程
/// 都没起过。这条链任何一步失败都会把租约还回去并重新上锁 ——
/// 门禁不过就一个进程都不起。
#[tauri::command]
async fn launch_claude(
    app: tauri::AppHandle,
    target: launch::LaunchTarget,
    state: tauri::State<'_, AppState>,
) -> Result<launch::LaunchResult> {
    let task = match target {
        launch::LaunchTarget::ClaudeCode => events::TASK_LAUNCH_CODE,
        launch::LaunchTarget::ClaudeDesktop => events::TASK_LAUNCH_DESKTOP,
    };
    let rep = Reporter::new(app, task, 4);
    rep.phase(1, "验证出口 IP 与门禁");
    let r = match launch::launch(target, &state.gate).await {
        Ok(r) => r,
        Err(e) => {
            rep.fail(e.to_string());
            return Err(e);
        }
    };
    rep.phase(2, "出口 IP 已通过，执行锁已临时放行");
    rep.phase(3, "启动 Claude 进程");
    start_watchdog(target.watch_mode(), state);
    rep.done("启动完成，看门狗已挂载");
    Ok(r)
}

// ------------------------------------------------------------------ 探测

#[tauri::command]
async fn probe_ip() -> Result<probe::ip::IpInfo> {
    probe::ip::ip_info().await
}

#[tauri::command]
async fn probe_purity() -> Result<probe::verdict::PanelVerdict> {
    let info = probe::ip::ip_info().await?;
    Ok(probe::verdict::evaluate(&info))
}

#[tauri::command]
async fn probe_dns(app: tauri::AppHandle) -> Result<probe::dns::DnsReport> {
    // 10 个探针域名各报一次，界面上那条进度条才动得起来。
    let rep = Reporter::new(app, events::TASK_DNS_PROBE, 10);
    let r = probe::dns::check(&rep).await;
    match &r {
        Ok(_) => rep.done("检测完成"),
        Err(e) => rep.fail(e.to_string()),
    }
    r
}

/// 权威站点地址与通过标准。前端原样展示，不要在前端硬编码第二份。
#[tauri::command]
fn purity_criteria() -> serde_json::Value {
    use probe::verdict as v;
    serde_json::json!({
        "ipqs": { "url": v::IPQS_URL, "criteria": v::IPQS_CRITERIA },
        "ippure": { "url": v::IPPURE_URL, "criteria": v::IPPURE_CRITERIA },
        "optional": [
            { "name": "Scamalytics", "url": v::SCAMALYTICS_URL },
            { "name": "IPData", "url": v::IPDATA_URL }
        ],
        "maxFraudScore": v::MAX_FRAUD_SCORE,
        "iproyal": v::IPROYAL_AFF
    })
}

// ------------------------------------------------------------------ 环境

#[tauri::command]
async fn detect_software() -> serde_json::Value {
    serde_json::json!({
        "claudeCode": install::detect::claude_code().await,
        "claudeDesktop": install::detect::claude_desktop(),
        "codex": install::detect::codex().await,
        "browsers": install::detect::browsers(),
    })
}

/// winget 在不在、两个包查不查得到、各自什么版本。
#[tauri::command]
async fn install_probe() -> install::winget::InstallProbe {
    install::winget::probe().await
}

/// 装一个目标：解锁 → winget（失败回退官方脚本）→ 重新枚举 → 验签 → 重新上锁。
#[tauri::command]
async fn install_run(
    app: tauri::AppHandle,
    target: install::winget::InstallTarget,
) -> Result<install::winget::InstallResult> {
    let rep = Reporter::new(app, events::TASK_INSTALL, install::winget::TOTAL);
    let r = install::winget::install(target, &rep).await;
    if let Err(e) = &r {
        rep.fail(e.to_string());
    }
    r
}

// ------------------------------------------------------------------ 账户

#[tauri::command]
fn accounts_list() -> serde_json::Value {
    let migration = accounts::migrate_legacy().ok();
    serde_json::json!({
        "slots": accounts::slots(),
        "caveat": accounts::EXPIRY_CAVEAT,
        "migration": migration,
    })
}

/// 只由界面上的手动点击触发。**不要**从任何自动路径调用它。
#[tauri::command]
fn accounts_switch(label: String) -> Result<()> {
    accounts::switch(&label)
}

// ------------------------------------------------------------------ 中转站

/// 整个中转站目录。
///
/// 回的是 `ProviderView`，那个结构**装不下 API Key** —— 不是靠
/// `skip_serializing` 记得加，是结构上就没有能放 Key 的字段。
#[tauri::command]
fn relay_list() -> Vec<relay::ProviderView> {
    relay::store::load().all_views()
}

/// 新增或更新一条。`api_key` 留空表示**保留原有的那把**，不是清空。
#[tauri::command]
fn relay_save(provider: relay::ProviderInput) -> Result<String> {
    let mut s = relay::store::load();
    let id = s.upsert(provider)?;
    relay::store::save(&s)?;
    Ok(id)
}

#[tauri::command]
fn relay_delete(id: String) -> Result<()> {
    let mut s = relay::store::load();
    s.remove(&id);
    relay::store::save(&s)
}

#[tauri::command]
fn relay_duplicate(id: String) -> Result<Option<String>> {
    let mut s = relay::store::load();
    let new = s.duplicate(&id);
    relay::store::save(&s)?;
    Ok(new)
}

/// 启用某一条：写进目标工具的配置，并记住它。
#[tauri::command]
fn relay_activate(target: relay::RelayTarget, id: String) -> Result<()> {
    relay::activate(target, &id)
}

#[tauri::command]
fn relay_reorder(target: relay::RelayTarget, ids: Vec<String>) -> Result<()> {
    let mut s = relay::store::load();
    s.reorder(target, &ids);
    relay::store::save(&s)
}

/// 从目标工具的 live 配置里读回当前值，用于「从当前配置导入」。
#[tauri::command]
fn relay_import_live(target: relay::RelayTarget) -> Option<relay::ProviderMeta> {
    relay::import_live(target)
}

/// 每个 target 的 live 配置各返回一条。**不要退回成返回单条** ——
/// 旧版读到 Claude 就提前 return，Codex 那边的配置在界面上永远看不见。
#[tauri::command]
fn relay_current() -> Vec<relay::ProviderMeta> {
    relay::current_providers()
}

#[tauri::command]
fn relay_presets(target: relay::RelayTarget) -> Vec<relay::presets::Preset> {
    relay::presets::for_target(target)
}

/// 拉模型列表。会把 Key 发到用户填的地址上，**只在用户点了才跑**。
#[tauri::command]
async fn relay_fetch_models(
    base_url: String,
    id: Option<String>,
) -> Result<relay::probe::ModelList> {
    let key = key_of(id.as_deref());
    relay::probe::fetch_models(&base_url, key.as_deref()).await
}

/// 测端点延迟。同上，只在用户点了才跑。
#[tauri::command]
async fn relay_test_latency(base_url: String, id: Option<String>) -> relay::probe::LatencyResult {
    let key = key_of(id.as_deref());
    relay::probe::measure(&base_url, key.as_deref()).await
}

/// 取某条记录的明文 Key，**只在后端流转**，绝不作为命令返回值。
fn key_of(id: Option<&str>) -> Option<String> {
    let id = id?;
    relay::store::load().get(id)?.plain_key()
}

// ------------------------------------------------------------------ 时区

#[tauri::command]
fn tz_current() -> Result<String> {
    sysenv::current_windows_tz()
}

#[tauri::command]
fn tz_apply(
    iana: String,
    restore_on_exit: bool,
    state: tauri::State<'_, AppState>,
) -> Result<sysenv::TzState> {
    let st = sysenv::apply_timezone(&iana, restore_on_exit)?;
    *state.tz.lock().unwrap() = Some(st.clone());
    Ok(st)
}

#[tauri::command]
fn tz_restore(state: tauri::State<'_, AppState>) -> Result<()> {
    let st = state.tz.lock().unwrap().clone();
    match st {
        Some(st) => sysenv::restore_timezone(&st),
        None => Ok(()),
    }
}

// ------------------------------------------------------------------ 进度

#[tauri::command]
fn progress_load() -> progress::Progress {
    progress::load()
}

#[tauri::command]
fn progress_set(
    id: String,
    state: progress::StepState,
    risk: progress::Risk,
    detail: String,
) -> progress::Progress {
    progress::set(&id, state, risk, detail)
}

// ------------------------------------------------------------ 升级

#[tauri::command]
async fn upgrade_plan(channel: install::upgrade::Channel) -> install::upgrade::UpgradePlan {
    install::upgrade::plan(channel).await
}

#[tauri::command]
async fn upgrade_execute(
    app: tauri::AppHandle,
    channel: install::upgrade::Channel,
    force: bool,
) -> Result<String> {
    let rep = Reporter::new(app, events::TASK_UPGRADE, 5);
    let r = install::upgrade::execute(channel, force, &rep).await;
    if let Err(e) = &r {
        rep.fail(e.to_string());
    }
    r
}

// ------------------------------------------------------------ 应用更新

/// 返回更新配置状态。仓库未创建前保持安全的未配置状态，启动时调用也不会联网。
#[tauri::command]
fn update_status() -> update::UpdateStatus {
    update::status()
}

// ------------------------------------------------------------ 一键关闭

/// 只看不动，把会被收的进程列给用户确认。
#[tauri::command]
async fn killswitch_preview(app: tauri::AppHandle) -> killswitch::KillReport {
    let rep = Reporter::new(app, events::TASK_KILL_PREVIEW, 2);
    rep.phase(1, "扫描 Claude 相关进程并核对证据");
    let r = killswitch::preview().await;
    rep.done(format!("扫描完成，发现 {} 个可关闭进程", r.targets.len()));
    r
}

#[tauri::command]
async fn killswitch_execute(app: tauri::AppHandle) -> Result<killswitch::KillReport> {
    let rep = Reporter::new(app, events::TASK_KILL_EXECUTE, 3);
    rep.phase(1, "扫描并核对待关闭进程");
    let r = killswitch::execute().await;
    match &r {
        Ok(v) => {
            rep.phase(2, format!("已关闭 {} 个进程，正在重新上锁", v.killed.len()));
            rep.done("关闭完成，执行锁已恢复");
        }
        Err(e) => rep.fail(e.to_string()),
    }
    r
}

// ------------------------------------------------------------ 插件

#[tauri::command]
fn plugin_list() -> Vec<plugins::PluginStatus> {
    vec![plugins::sillytavern::status()]
}

#[tauri::command]
fn plugin_catalog_status() -> plugins::OfficialCatalogStatus {
    plugins::official_catalog_status()
}

/// 启动酒馆：先过 IP 门禁，再拉起桥接与酒馆，最后挂上看门狗。
///
/// 门禁不过就一个进程都不起 —— 这是接管启动链之后仍要守住的那条线。
#[tauri::command]
async fn plugin_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String> {
    gate::open_authorized("sillytavern", &state.gate).await?;

    let rep = Reporter::new(app, events::TASK_TAVERN_START, 5);
    let url = match plugins::sillytavern::start(&rep).await {
        Ok(u) => u,
        Err(e) => {
            // 起失败就把租约还回去，不能让门开着。
            let _ = gate::release_lease(&state.gate);
            rep.fail(e.to_string());
            return Err(e);
        }
    };

    // 桥接跑起来之后由 ClaudeGate 的看门狗接管，不再起 PowerShell 的 WatchBridge。
    let (tx, rx) = tokio::sync::watch::channel(false);
    *state.watchdog_stop.lock().unwrap() = Some(tx);
    let gs = state.gate.clone();
    tauri::async_runtime::spawn(async move {
        gate::run_watchdog(gate::watchdog::WatchMode::Cli, gs, rx).await;
    });

    Ok(url)
}

#[tauri::command]
async fn plugin_stop(state: tauri::State<'_, AppState>) -> Result<Vec<String>> {
    let done = plugins::sillytavern::stop().await?;
    if let Some(tx) = state.watchdog_stop.lock().unwrap().take() {
        let _ = tx.send(true);
    }
    gate::release_lease(&state.gate)?;
    Ok(done)
}

#[tauri::command]
fn tavern_config() -> plugins::sillytavern::TavernConfig {
    plugins::sillytavern::load_config()
}

#[tauri::command]
fn tavern_config_save(cfg: plugins::sillytavern::TavernConfig) -> Result<()> {
    plugins::sillytavern::save_config(&cfg)
}

// ------------------------------------------------------------ 酒馆资产

#[tauri::command]
fn tavern_assets() -> Vec<plugins::tavern_assets::CategoryListing> {
    plugins::tavern_assets::list_all()
}

#[tauri::command]
fn tavern_backup() -> Result<plugins::tavern_assets::BackupEntry> {
    plugins::tavern_assets::backup()
}

#[tauri::command]
fn tavern_backups() -> Vec<plugins::tavern_assets::BackupEntry> {
    plugins::tavern_assets::list_backups()
}

#[tauri::command]
fn tavern_restore(backup_id: String) -> Result<String> {
    plugins::tavern_assets::restore(&backup_id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            gate_status,
            gate_lock_all,
            gate_unlock_all,
            gate_open,
            gate_release,
            gate_clean_stale,
            allowlist_read,
            allowlist_write,
            allowlist_add_current,
            watchdog_start,
            watchdog_stop,
            probe_ip,
            probe_purity,
            probe_dns,
            purity_criteria,
            detect_software,
            install_probe,
            install_run,
            launch_claude,
            accounts_list,
            accounts_switch,
            relay_list,
            relay_save,
            relay_delete,
            relay_duplicate,
            relay_activate,
            relay_reorder,
            relay_import_live,
            relay_current,
            relay_presets,
            relay_fetch_models,
            relay_test_latency,
            tz_current,
            tz_apply,
            tz_restore,
            progress_load,
            progress_set,
            upgrade_plan,
            upgrade_execute,
            update_status,
            killswitch_preview,
            killswitch_execute,
            plugin_list,
            plugin_catalog_status,
            plugin_start,
            plugin_stop,
            tavern_config,
            tavern_config_save,
            tavern_assets,
            tavern_backup,
            tavern_backups,
            tavern_restore,
        ])
        .setup(|app| {
            let _ = legacy::disable_known_launchers();
            // 启动时重建锁，等价于现有实现的 -Mode Check：
            // 宁可多锁一次，也不要因为上次异常退出而敞着。
            //
            // **但白名单为空时绝不上锁。** 那种情况通常是首次运行：
            // 锁上之后 open_authorized 一定过不了（IP 不可能在空名单里），
            // 用户就被自己的工具关在门外了。空名单 = 还没配置好，
            // 这时候什么都不做才是对的。
            match gate::allowlist::read() {
                Ok(list) if !list.is_empty() => {
                    let _ = gate::lock_all();
                }
                _ => {
                    gate::log::write("白名单为空，启动时不上锁（首次运行请先添加当前 IP）");
                }
            }
            let _ = app;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("启动 ClaudeGate 失败");
}
