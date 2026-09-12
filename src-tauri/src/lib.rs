//! QB Gate —— Claude 环境控制面板。
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
pub mod profile;
pub mod relay;
pub mod settings;
pub mod snapshot;
pub mod sysenv;
pub mod tray;
pub mod update;

use error::{GateError, Result};
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
        needs_reopen: state.gate.needs_reopen.lock().unwrap().clone(),
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

/// 「重新放行」：验一次出口 IP，过了就把门重新打开。
///
/// 托盘菜单和总览横幅都走这条。它跟 `gate_open` 的唯一区别是**不需要调用方
/// 说出 holder** —— 沿用上一次的那个，因为使用者点它的场景永远是
/// 「门被面板自己关上了，我要把它开回来」。
///
/// 它**不启动任何进程**，只恢复租约。
#[tauri::command]
async fn gate_reopen(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String> {
    let detail = reopen_and_rewatch(&state, Some(app.clone())).await?;
    tray::refresh(&app);
    Ok(detail)
}

/// 重新放行 + 把看门狗接回去。命令与托盘菜单共用同一份。
///
/// 「接回看门狗」不是可选步骤：门开着却没有东西核对出口 IP，
/// 正是 §7.12 那条教训要防的场面。租约记录里带着档位，按原样接回去 ——
/// 接错档等于偷偷改了安全口径。
pub(crate) async fn reopen_and_rewatch(
    state: &AppState,
    app: Option<tauri::AppHandle>,
) -> Result<String> {
    let detail = gate::reopen_now(&state.gate).await?;
    let mode = state.gate.lease.lock().unwrap().mode;
    if let Some(m) = mode {
        start_watchdog(m, state, app);
    }
    Ok(detail)
}

#[tauri::command]
fn gate_clean_stale() -> Vec<(std::path::PathBuf, bool)> {
    gate::clean_stale_copies()
}

#[tauri::command]
fn allowlist_read() -> Result<Vec<String>> {
    gate::allowlist::read()
}

/// 手改白名单。
///
/// **这里不做逐条国家查询。** 想查一个任意 IP 的国家得一条一条去问远端
/// （`ipinfo.io/<ip>/json`），离线或者接口限流时就意味着「你改不了自己的白名单」——
/// 那正是硬约束 4 要防的「被自己的工具关在门外」。
///
/// 放过这一关不留洞：国家层是**判定时**生效的，不是插入时。手工塞进来的脏 IP
/// 照样过不了 `gate::judge` —— 看门狗每轮、hook 每次请求都会重新看一遍当前国家。
/// 它唯一的效果是把报错从「加不进去」推迟到「用的时候被拦」。
#[tauri::command]
fn allowlist_write(entries: Vec<String>) -> Result<()> {
    gate::allowlist::write(&entries)
}

/// 把当前出口 IP 加进白名单 —— **国家不合格就一个字都不写**。
///
/// 这一关堵的是「先把脏 IP 塞进白名单，再回头抱怨门禁没用」。
/// 这条路上我们手里正好有一轮完整的多源探测，查国家是免费的，
/// 所以这里可以严，也必须严。
#[tauri::command]
async fn allowlist_add_current() -> Result<Vec<String>> {
    let reading = probe::ip::reading().await;
    let ip = gate::judge::may_add(&reading, &settings::country_allowlist()).map_err(|j| {
        match j {
            gate::judge::Judgement::IpUnknown => GateError::IpUnknown,
            other => GateError::GateRejected(format!("{}，拒绝加入白名单", other.reason())),
        }
    })?;

    let mut all = gate::allowlist::read().unwrap_or_default();
    if !all.contains(&ip) {
        all.push(ip);
        gate::allowlist::write(&all)?;
    }
    Ok(all)
}

// ------------------------------------------------------------ 会话内门禁

#[tauri::command]
fn hook_status() -> gate::hook::HookStatus {
    gate::hook::status()
}

#[tauri::command]
fn hook_install() -> Result<gate::hook::HookStatus> {
    gate::hook::install()
}

#[tauri::command]
fn hook_uninstall() -> Result<gate::hook::HookStatus> {
    gate::hook::uninstall()
}

// -------------------------------------------------------------- 版本库

/// 托管目录的版本库里有哪几版可以退回去。
#[tauri::command]
fn managed_history(which: install::managed::App) -> Vec<install::versions::VersionEntry> {
    let root = settings::managed_apps_dir();
    let dir = install::managed::app_dir(&root, which);
    let current = install::managed::record_of(&root, which).map(|r| r.version);
    install::versions::history(&dir, which, current.as_deref())
}

/// 回滚到版本库里的某一版。
///
/// **会先把当前这份收进版本库再换** —— 反过来的话中间那一刻当前版本已经没了，
/// 换不上去就两头空。换完要重新上锁（调用方负责，跟安装走同一条路）。
#[tauri::command]
async fn managed_rollback(which: install::managed::App, version: String) -> Result<String> {
    let root = settings::managed_apps_dir();
    let dir = install::managed::app_dir(&root, which);
    let current = install::managed::record_of(&root, which);
    let msg = install::versions::rollback(&dir, which, &version, current.as_ref())?;
    // 换上来的那份此刻是从版本库拷过来的，ACL 跟着源文件走 —— 必须重锁一遍，
    // 否则回滚等于顺手把门禁摘了。
    let n = gate::lock_all().unwrap_or(0);
    Ok(format!("{msg}，已重新上锁 {n} 个副本"))
}

/// 国家白名单的两个起手式。**面板不替你选**，只是省得手打。
#[tauri::command]
fn country_presets() -> Vec<(String, Vec<String>)> {
    vec![
        (
            "只留美国".into(),
            gate::judge::PRESET_US.iter().map(|s| s.to_string()).collect(),
        ),
        (
            "常用支持地区".into(),
            gate::judge::PRESET_COMMON
                .iter()
                .map(|s| s.to_string())
                .collect(),
        ),
    ]
}

#[tauri::command]
fn watchdog_start(
    app: tauri::AppHandle,
    mode: gate::watchdog::WatchMode,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    start_watchdog(mode, &state, Some(app));
    Ok(())
}

/// 起看门狗并把停止句柄挂到 AppState 上。
///
/// 每次都换一个新的 watch channel：旧的那个 sender 被 take 走之后，
/// 上一轮的循环下次醒来会自己退出。
fn start_watchdog(mode: gate::watchdog::WatchMode, state: &AppState, app: Option<tauri::AppHandle>) {
    let (tx, rx) = tokio::sync::watch::channel(false);
    if let Some(old) = state.watchdog_stop.lock().unwrap().replace(tx) {
        let _ = old.send(true);
    }
    let gs = state.gate.clone();
    tauri::async_runtime::spawn(async move {
        gate::run_watchdog(mode, gs, rx, app).await;
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
        launch::LaunchTarget::Codex => events::TASK_LAUNCH_CODEX,
    };
    let gated = target.gated();
    let rep = Reporter::new(app.clone(), task, 4);
    rep.phase(1, if gated { "验证出口 IP 与门禁" } else { "定位可执行文件" });
    let r = match launch::launch(target, &state.gate).await {
        Ok(r) => r,
        Err(e) => {
            rep.fail(e.to_string());
            return Err(e);
        }
    };
    if gated {
        rep.phase(2, "出口 IP 已通过，执行锁已临时放行");
    } else {
        rep.phase(2, "该目标当前不归 IP 门禁管，未验证出口 IP");
    }
    rep.phase(3, format!("启动 {}", target.label()));
    // 不归门禁管就别挂看门狗 —— 看门狗的动作是收回租约并上锁，
    // 而这次根本没有租约，挂上去只会去动 Claude 那边的锁。
    if gated {
        start_watchdog(target.watch_mode(), &state, Some(app));
        rep.done("启动完成，看门狗已挂载");
    } else {
        rep.done("启动完成");
    }
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
        // 本机全部 Claude Code 副本：哪份拿来启动、哪份锁得上、哪份锁不上。
        "claudeCodeInstalls": install::detect::claude_code_installs(),
        "claudeDesktop": install::detect::claude_desktop().await,
        "codex": install::detect::codex().await,
        "browsers": install::detect::browsers(),
    })
}

/// winget 在不在、两个包查不查得到、各自什么版本。
#[tauri::command]
async fn install_probe() -> install::winget::InstallProbe {
    install::winget::probe().await
}

/// 装一个目标。
///
/// * Claude Code / Codex（v0.9.0）：**面板托管安装** —— 从官方源下载、核对官方给的 SHA-256
///   与数字签名、放进托管目录（`install::managed`）。位置由面板说了算，面板再也不用猜。
/// * Claude 桌面端：winget 装官方安装器。它的位置被官方安装器写死，面板接管不了，
///   装完从注册表把实际位置记下来。
#[tauri::command]
async fn install_run(
    app: tauri::AppHandle,
    target: install::winget::InstallTarget,
    state: tauri::State<'_, AppState>,
) -> Result<install::winget::InstallResult> {
    use install::winget::InstallTarget as T;
    let managed_app = match target {
        T::ClaudeCode => Some(install::managed::App::ClaudeCode),
        T::Codex => Some(install::managed::App::Codex),
        T::ClaudeDesktop => None,
    };
    match managed_app {
        Some(a) => managed_install(app, a, "latest", &state).await,
        None => {
            let rep = Reporter::new(app, events::TASK_INSTALL, install::winget::TOTAL);
            // `winget::install` 自己会解锁、自己会重锁（那六步的第 1 步和第 6 步），
            // 所以这里只观察、不重复解锁。守卫在这里只干一件事：
            // **重锁之后把使用者原来拿着的租约还回去**。
            let guard = gate::Maintenance::observing(&state.gate);
            let r = install::winget::install(target, &rep).await;
            let done = guard.finish(&state.gate).await;
            if let Err(e) = &r {
                rep.fail(format!("{e} {}", done.detail));
            }
            r
        }
    }
}

/// 托管安装 / 升级。安装器只管文件，重锁与还租约在这里的维护窗口收尾时做。
async fn managed_install(
    app: tauri::AppHandle,
    which: install::managed::App,
    channel: &str,
    state: &AppState,
) -> Result<install::winget::InstallResult> {
    use install::winget::{InstallResult, InstallTarget, Method};
    let rep = Reporter::new(app, events::TASK_INSTALL, install::managed::TOTAL);
    let guard = gate::Maintenance::observing(&state.gate);
    let r = install::managed::install(which, channel, &rep).await;
    let done = guard.finish(&state.gate).await;
    let target = match which {
        install::managed::App::ClaudeCode => InstallTarget::ClaudeCode,
        install::managed::App::Codex => InstallTarget::Codex,
    };
    match r {
        Ok(o) => {
            let detail = format!(
                "{} {} 已装进托管目录 {}。{}",
                which.label(),
                o.version,
                o.path.display(),
                done.detail
            );
            gate::log::write(&detail);
            rep.done(detail.clone());
            Ok(InstallResult {
                target,
                ok: true,
                method: Method::Managed,
                relocked: done.relocked,
                signature_ok: Some(true),
                detail,
                log: o.log,
            })
        }
        Err(e) => {
            let msg = format!("{e} {}", done.detail);
            gate::log::write(&format!("托管安装 {} 失败：{e}", which.label()));
            rep.fail(msg.clone());
            Err(GateError::Other(msg))
        }
    }
}

/// 托管目录的现状：根目录在哪、Claude Code 与 Codex 装没装、什么版本。
#[tauri::command]
fn managed_status() -> install::managed::Status {
    install::managed::status()
}

/// **当场实测**一个目录能不能当托管根目录（建得出、写得进、锁得上也解得开）。
#[tauri::command]
fn managed_probe_dir(path: String) -> install::managed::Probe {
    install::managed::probe_dir(std::path::Path::new(path.trim()))
}

#[derive(serde::Serialize)]
struct MigrateReport {
    from: std::path::PathBuf,
    to: std::path::PathBuf,
    moved: Vec<String>,
    /// 为了搬走正在运行的 Claude Code，先关掉了几个 Claude 进程。
    closed: usize,
}

/// 换托管根目录。已经装了东西就**一键迁移**过去；什么都没装就只是记下新位置。
///
/// 顺序：实测新目录 → （托管的 Claude Code 正在跑就先关掉全部 Claude）→ 旧目录里还有
/// 别的程序在跑就拒绝 → 搬文件（失败整体回滚）→ 写设置（写不进去就搬回去）→
/// 维护窗口收尾时按新位置重新上锁。写设置必须在重锁之前 ——
/// `lock_all` 从设置里读根目录，顺序反了会锁到旧位置上去。
#[tauri::command]
async fn managed_set_dir(
    app: tauri::AppHandle,
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<MigrateReport> {
    use install::managed;
    let new = std::path::PathBuf::from(path.trim());
    let old = managed::root();
    let mut report = MigrateReport { from: old.clone(), to: new.clone(), moved: Vec::new(), closed: 0 };
    if install::inventory::same_path(&old, &new) {
        return Ok(report);
    }
    if managed::is_inside(&new, &old) || managed::is_inside(&old, &new) {
        return Err(GateError::Other(
            "新目录不能在旧目录里面，旧目录也不能在新目录里面 —— 搬的时候会把自己套进去。".into(),
        ));
    }
    let probe = managed::probe_dir(&new);
    if !probe.ok {
        return Err(GateError::Other(probe.reason.unwrap_or_else(|| "这个目录不能用".into())));
    }

    let store = |dir: &std::path::Path| -> Result<()> {
        let mut s = settings::load();
        s.managed_apps_dir = (!install::inventory::same_path(dir, &settings::default_managed_dir()))
            .then(|| dir.to_path_buf());
        settings::save(&s)
    };

    let has_files = managed::App::ALL.iter().any(|a| managed::exe_in(&old, *a).is_file());
    if !has_files {
        store(&new)?;
        gate::log::write(&format!("托管目录改为 {}（原来没装东西，不用搬）", new.display()));
        return Ok(report);
    }

    let guard = gate::Maintenance::observing(&state.gate);
    // 托管的 claude.exe 正被会话占着的话搬不动（跨盘时删不掉源）。只在确实有进程
    // 从旧目录里跑着时才清场 —— 换个目录不该顺手把没关系的对话全关了。
    let running_here = killswitch::preview()
        .await
        .map(|p| p.targets.iter().any(|t| t.path.as_deref().is_some_and(|x| managed::is_inside(std::path::Path::new(x), &old))))
        .unwrap_or(false);
    if running_here {
        match killswitch::execute().await {
            Ok(k) => report.closed = k.killed.len(),
            Err(e) => {
                guard.finish(&state.gate).await;
                return Err(e);
            }
        }
    }
    // 再看一遍旧目录里还有没有程序在跑（比如 Codex —— 一键关闭不认它）。正在运行的 exe
    // 搬不动，搬一半比不搬危险得多，所以有就一个文件都不动，列出来让使用者自己关。
    // 刚收掉的进程要一小会儿才从进程表里消失，清过场的话多看几轮。
    let mut busy = Vec::new();
    for round in 0..if running_here { 6 } else { 1 } {
        if round > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
        busy = match managed::running_from(&old).await {
            Ok(b) => b,
            Err(e) => {
                guard.finish(&state.gate).await;
                return Err(e);
            }
        };
        if busy.is_empty() {
            break;
        }
    }
    if !busy.is_empty() {
        guard.finish(&state.gate).await;
        return Err(GateError::Other(format!(
            "还有程序正从旧目录运行，搬不动：{}。先把它关掉再迁移（面板不按进程名杀进程）。什么都没动。",
            busy.iter().map(|(pid, p)| format!("PID {pid} {p}")).collect::<Vec<_>>().join("；")
        )));
    }
    let moved = match managed::migrate_files(&old, &new) {
        // 设置写不进去就把文件搬回去 —— 不然文件在新目录、设置指着旧目录，重锁锁不到它们。
        Ok(notes) => match store(&new) {
            Ok(()) => Ok(notes),
            Err(e) => Err(GateError::Other(format!(
                "托管目录的设置没写进去（{e}），{}",
                match managed::migrate_files(&new, &old) {
                    Ok(_) => "文件已经搬回原处，托管目录没有改。".to_string(),
                    Err(b) => format!("往回搬也失败了：{b}"),
                }
            ))),
        },
        Err(e) => Err(e),
    };
    if moved.is_err() {
        let n = managed::lock_strays(&new);
        if n > 0 {
            gate::log::write(&format!("托管目录迁移没成，给留在 {} 的 {n} 个文件加上了执行锁", new.display()));
        }
    }
    let done = guard.finish(&state.gate).await;
    report.moved = moved?;
    gate::log::write(&format!(
        "托管目录从 {} 迁到 {}：{}。{}",
        old.display(),
        new.display(),
        report.moved.join("；"),
        done.detail
    ));
    tray::refresh(&app);
    Ok(report)
}

/// 面板没装的、多余的 Claude Code / Codex 副本，以及准备怎么清。只看不动。
#[tauri::command]
async fn managed_externals() -> Vec<install::managed::External> {
    install::managed::externals().await
}

/// **彻底清除**一个软件的外部副本（使用者确认过清单之后）。
///
/// 先关掉全部 Claude（正在跑的删不掉），清完在维护窗口收尾时重锁 —— 清单变短了，锁也跟着变。
/// 不清的话什么都不用做：inventory 照样把它们全锁上。
#[tauri::command]
async fn managed_cleanup(
    which: install::managed::App,
    state: tauri::State<'_, AppState>,
) -> Result<install::managed::CleanupReport> {
    let guard = gate::Maintenance::observing(&state.gate);
    if which == install::managed::App::ClaudeCode {
        if let Err(e) = killswitch::execute().await {
            guard.finish(&state.gate).await;
            return Err(e);
        }
    }
    let r = install::managed::cleanup(which).await;
    guard.finish(&state.gate).await;
    if let Ok(rep) = &r {
        for d in &rep.done {
            gate::log::write(&format!("清除外部副本：{d}"));
        }
        for f in &rep.failed {
            gate::log::write(&format!("清除外部副本失败：{f}"));
        }
    }
    r
}

// --------------------------------------------------- Claude 痕迹与 Chrome

/// 这台机器以前装过 / 登录过 Claude 吗。
///
/// 只读检测，不改任何东西。**Chrome 正在跑时它的资料文件被占着扫不了** ——
/// 那种情况下 `chrome_scanned` 是 false，界面要说「先关掉 Chrome 再检测」，
/// 不能显示成「没找到痕迹」。
#[tauri::command]
async fn claude_traces() -> install::chrome::TraceReport {
    install::chrome::claude_traces().await
}

/// 卸掉 Chrome、删干净用户资料、再装回来。没装过就只装。
///
/// ⚠ **这个命令会毁掉数据**：书签、密码、扩展、全部站点数据一起没，不可恢复。
/// 界面负责在调用之前弹那一次确认框 —— 使用者要求确认之后全程自动、
/// 中途不再问，所以那一次确认必须把代价说全。
///
/// 只碰 Chrome。Edge、Firefox 一概不动。
#[tauri::command]
async fn chrome_reinstall(app: tauri::AppHandle) -> Result<String> {
    let rep = Reporter::new(app, events::TASK_CHROME, install::chrome::REINSTALL_TOTAL);
    let r = install::chrome::reinstall(&rep).await;
    match &r {
        Ok(d) => rep.done(d.clone()),
        Err(e) => rep.fail(e.to_string()),
    }
    r
}

// ------------------------------------------------------------------ 账户

#[tauri::command]
fn accounts_list() -> serde_json::Value {
    let roots = accounts::AccountRoots::current();
    // 把酒馆桥接那边重复的一份凭证合进来（幂等，已经是一份时几乎不花时间）。
    // 只把真的做了的事写日志 —— 这个命令会被轮询，失败项下次还会再试，不能刷屏。
    let sync = accounts::sync_bridge(&roots);
    for d in &sync.done {
        gate::log::write(&format!("账户合并：{d}"));
    }
    let desktop = accounts::desktop_state(&roots);
    serde_json::json!({
        "slots": accounts::slots_in(&roots),
        "caveat": accounts::EXPIRY_CAVEAT,
        "planCaveat": accounts::PLAN_CAVEAT,
        "sync": sync,
        "desktop": desktop,
        "bridgePresent": roots.bridge.is_some(),
    })
}

/// 新建一个空槽位。只由界面点击触发。
#[tauri::command]
fn accounts_create(app: tauri::AppHandle, label: String) -> Result<accounts::CreateOutcome> {
    let out = accounts::create_slot(&accounts::AccountRoots::current(), label.trim())?;
    for n in &out.notes {
        gate::log::write(n);
    }
    gate::log::write(&format!(
        "新建账户槽位 {}{}",
        out.label,
        if out.activated { "（原来没有激活槽位，它直接成了当前的）" } else { "" }
    ));
    tray::refresh(&app);
    Ok(out)
}

#[derive(serde::Serialize)]
struct SwitchReport {
    /// 切换前清场（关闭全部 Claude）的报告。
    closed: killswitch::KillReport,
    /// 换了哪几处指向：Claude Code / 酒馆桥接 / 桌面端。
    switched: Vec<String>,
    notes: Vec<String>,
}

/// 换账户之前的清场：关掉全部 Claude，收完重锁（维护窗口守着租约）。
///
/// **失败就返回错误，调用方必须停下、不许接着换指向** —— `accounts::switch_in` 从 v0.9.0 起
/// 不再自己查桌面端在不在跑，它信的就是「调用方已经清过场」。清场没成还硬切，
/// 就是在一个跑着的桌面端脚下换它的资料目录。对话框、托盘、档案 / 快照回滚都走这一个。
///
/// `wait_desktop`：taskkill 返回时进程未必已经退干净，桌面端要跟着换资料目录的话
/// 等它真的退了再动（最多约 3 秒）。
pub(crate) async fn clear_for_switch(state: &AppState, wait_desktop: bool) -> Result<killswitch::KillReport> {
    let guard = gate::Maintenance::observing(&state.gate);
    let closed = killswitch::execute().await;
    guard.finish(&state.gate).await;
    let closed = closed?;
    if wait_desktop {
        for _ in 0..8 {
            if killswitch::desktop_processes().map(|v| v.is_empty()).unwrap_or(true) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
    }
    Ok(closed)
}

/// 切换账户。**只由界面上的手动点击触发，不要从任何自动路径调用它。**
///
/// v0.9.0 的语义：**先清场，再切，不自动启动。**
///
///   1. `killswitch::execute()` 关掉全部正在跑的 Claude —— 桌面端、所有 Claude Code
///      会话、酒馆桥接。清场之后没有任何进程还连着旧账户，也就不存在
///      「在跑着的桌面端脚下换它的资料目录」这种会把两个账户搅在一起的中间态。
///   2. `accounts::switch_with` 把三处指向都换到目标槽位。
///   3. 结束。**不起任何进程** —— 用户回总览自己点要启动的东西。
///
/// `desktop` 决定桌面端跟不跟着切（`Follow` / `Keep`）。收进程会顺手重锁，
/// `Maintenance::observing` 守着让重锁不等于把门关死。
#[tauri::command]
async fn accounts_switch(
    app: tauri::AppHandle,
    label: String,
    desktop: bool,
    state: tauri::State<'_, AppState>,
) -> Result<SwitchReport> {
    let mode = if desktop {
        accounts::DesktopMode::Follow
    } else {
        accounts::DesktopMode::Keep
    };

    // ---- 1. 清场：关掉全部 Claude（桌面端也在内）。失败就到此为止，槽位一点没动。
    let closed = clear_for_switch(&state, matches!(mode, accounts::DesktopMode::Follow)).await?;

    // ---- 2. 换指向。
    let out = accounts::switch_with(&label, mode)?;
    // 托盘菜单是静态对象，不重建就会显示上一次的账户。
    tray::refresh(&app);
    Ok(SwitchReport {
        closed,
        switched: out.switched,
        notes: out.notes,
    })
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
fn relay_activate(app: tauri::AppHandle, target: relay::RelayTarget, id: String) -> Result<()> {
    relay::activate(target, &id)?;
    tray::refresh(&app);
    Ok(())
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

/// 查这家中转站的**真实后端**是 Anthropic、Bedrock 还是 Vertex。
///
/// 同 `relay_fetch_models`：会把 Key 发到用户填的地址上，**只在用户点了才跑**。
/// 它要真发一次 `/v1/messages`（max_tokens=1），所以也会**消耗一点点额度**——
/// 界面上得说清楚，别让人以为是免费的只读查询。
#[tauri::command]
async fn relay_detect_backend(
    base_url: String,
    id: Option<String>,
    model: Option<String>,
) -> Result<relay::backend::BackendReport> {
    let key = key_of(id.as_deref());
    relay::backend::detect(&base_url, key.as_deref(), model.as_deref()).await
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

// ------------------------------------------------------------------ 设置

#[tauri::command]
fn settings_load() -> settings::Settings {
    settings::load()
}

/// 存设置。
///
/// 打开 `codex_under_gate` 之后**立刻按新的目标清单重新上锁一次**，
/// 否则「我打开了开关」和「codex 真的被锁上」之间会隔着一次重启，
/// 中间那段时间界面说管了、实际没管。关掉时同理要把 Codex 上的锁摘掉。
#[tauri::command]
fn settings_save(mut next: settings::Settings) -> Result<settings::Settings> {
    let before = settings::load();
    // 托管目录只能经 `managed_set_dir` 改（它会先实测、再把已装的搬过去）。
    // 这里原样保留旧值：前端改个开关不该顺手把它改掉 —— 文件还在旧目录，面板就又找不到了。
    next.managed_apps_dir = before.managed_apps_dir.clone();
    // 国家码规整成两位大写再落盘：使用者手打个 `us` 或者多敲个空格，
    // 判定那边就永远匹配不上，整层会**悄悄失效** —— 比报错难查得多。
    next.country_allowlist = settings::normalize_countries(next.country_allowlist);
    // 会话内门禁只能经 hook_install / hook_uninstall 开关 —— 它们要写脚本、
    // 要改槽位的 settings.json，还要拦「白名单为空」。让前端在这里直接翻这个
    // 布尔值，就会出现「设置里写着开，实际一个 hook 都没装」的假象。
    next.hook_enabled = before.hook_enabled;
    settings::save(&next)?;

    if before.codex_under_gate != next.codex_under_gate {
        if next.codex_under_gate {
            // 只有门本来就关着的时候才顺手把 Codex 也锁上。租约期内
            // （用户正开着 Claude 在用）不该因为改了个设置就把门关上 ——
            // 等这次租约收回时 lock_all 自然会带上 Codex。
            //
            // 判断时要**把 Codex 自己排除掉**：它刚进清单，当然还没锁，
            // 算进去的话这个条件永远不成立。
            let gate_closed = gate::collect_targets()
                .iter()
                .filter(|t| t.kind != gate::targets::TargetKind::CodexCli)
                .all(|t| t.locked);
            if gate_closed {
                let _ = gate::lock_all();
            }
            gate::log::write("设置：Codex 已纳入 IP 门禁");
        } else {
            // 关掉开关时 Codex 上可能还挂着 Deny ACE。这时它已经不在目标
            // 清单里了，lock_all / unlock_all 再也不会碰它 —— 不单独摘掉的话，
            // 用户关了开关 codex 仍然跑不起来，而面板显示一切正常。
            let n = gate::unlock_paths(&gate::targets::codex_lockable());
            gate::log::write(&format!("设置：Codex 已移出 IP 门禁，摘除 {n} 处执行锁"));
        }
    }
    Ok(settings::load())
}

// ------------------------------------------------------------------ 快照

#[tauri::command]
fn snapshot_list() -> Vec<snapshot::SnapshotEntry> {
    snapshot::list()
}

#[tauri::command]
fn snapshot_create(note: String) -> Result<snapshot::SnapshotEntry> {
    snapshot::create(&note)
}

/// 账户要变之前先清场：关掉全部 Claude（v0.9.0「切换账户 = 先清场」）。
///
/// 只在**真的要换账户**时才清 —— `target` 跟当前激活的槽位一样（或者根本没指定账户）
/// 就什么都不关：恢复一份不涉及换号的快照，不该顺手把人正在聊的对话全关了。
/// 返回关掉了几个；没清场是 `None`。
async fn clear_before_account_change(state: &AppState, target: Option<&str>) -> Result<Option<usize>> {
    let Some(target) = target else {
        return Ok(None);
    };
    let current = accounts::active_label(&accounts::AccountRoots::current());
    if current.as_deref() == Some(target) {
        return Ok(None);
    }
    // 档案 / 快照回滚走 `DesktopMode::Auto`，桌面端可能跟着换资料目录，所以等它退干净。
    let r = clear_for_switch(state, true).await?;
    gate::log::write(&format!(
        "要切到账户 {target}，先清场：关掉 {} 个 Claude 进程",
        r.killed.len()
    ));
    Ok(Some(r.killed.len()))
}

/// 回滚。**恢复之前会先把现状再存一份** —— 回滚本身也是个能出错的操作。
///
/// 快照记着当时的账户；要换号的话先清场（见 [`clear_before_account_change`]）。
#[tauri::command]
async fn snapshot_restore(
    app: tauri::AppHandle,
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String> {
    let target = snapshot::account_of(&id);
    let cleared = clear_before_account_change(&state, target.as_deref()).await?;
    let mut detail = snapshot::restore(&id)?;
    if let Some(n) = cleared {
        detail.push_str(&format!(" 换账户之前关掉了 {n} 个 Claude 进程，要用哪个请回总览自己点。"));
    }
    tray::refresh(&app);
    Ok(detail)
}

#[tauri::command]
fn snapshot_remove(id: String) -> Result<()> {
    snapshot::remove(&id)
}

#[tauri::command]
fn snapshot_dir(id: String) -> Result<String> {
    Ok(snapshot::dir_of(&id)?.display().to_string())
}

// ------------------------------------------------------------------ 档案

#[tauri::command]
fn profile_list() -> profile::ProfileStore {
    profile::load()
}

#[tauri::command]
fn profile_save(item: profile::Profile) -> Result<String> {
    profile::upsert(item)
}

#[tauri::command]
fn profile_remove(id: String) -> Result<()> {
    profile::remove(&id)
}

/// 按当前状态生成一个档案草稿（不落盘，交给界面确认后再存）。
#[tauri::command]
fn profile_capture(name: String) -> profile::Profile {
    profile::capture(&name)
}

/// 应用一个档案。**只由界面点击触发，不要加任何自动调用点** ——
/// 它会切账户，加了就变成自动轮换账户，直接踩政策线。
///
/// 档案要换号的话先清场（见 [`clear_before_account_change`]），跟切换对话框同一个语义。
#[tauri::command]
async fn profile_apply(
    app: tauri::AppHandle,
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<profile::ApplyReport> {
    let target = profile::account_of(&id);
    let cleared = clear_before_account_change(&state, target.as_deref()).await?;
    let mut r = profile::apply(&id)?;
    if let Some(n) = cleared {
        r.applied.insert(0, format!("换账户之前关掉了 {n} 个 Claude 进程"));
    }
    tray::refresh(&app);
    Ok(r)
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
    state: tauri::State<'_, AppState>,
) -> Result<String> {
    // v0.9.0：面板托管的那份在，就升它 —— 从官方源下新版、核对、替换，旧的留底。
    // 不在才走下面原来那条（官方安装脚本升 `~\.local\bin` 那份）。
    if install::managed::is_installed(install::managed::App::ClaudeCode) {
        let plan = install::upgrade::plan(channel).await;
        if !force
            && matches!(
                plan.action,
                install::upgrade::Action::UpToDate | install::upgrade::Action::WouldDowngrade
            )
        {
            return Ok(plan.detail);
        }
        let r = managed_install(app, install::managed::App::ClaudeCode, channel.as_str(), &state).await?;
        return Ok(r.detail);
    }

    // 七段：比版本 / 清残留 / 下脚本 / 跑安装器 / 按哈希核对 / 必要时自己补完 /
    // 重锁并恢复租约。第 5、6 两段是 v0.7.0 新增的，见 upgrade.rs 文件头。
    let rep = Reporter::new(app, events::TASK_UPGRADE, 7);
    let r = install::upgrade::execute(channel, force, &state.gate, &rep).await;
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
///
/// **回 `Result`。** 扫描失败得让用户看见 —— 老版本这里回的是裸
/// `KillReport`，枚举一出错就退化成一份空报告，界面照样显示
/// 「发现 0 个可关闭进程」，跟真的没有进程长得一模一样。
#[tauri::command]
async fn killswitch_preview(app: tauri::AppHandle) -> Result<killswitch::KillReport> {
    let rep = Reporter::new(app, events::TASK_KILL_PREVIEW, 2);
    rep.phase(1, "扫描 Claude 相关进程并核对证据");
    match killswitch::preview().await {
        Ok(r) => {
            rep.done(format!("扫描完成，发现 {} 个可关闭进程", r.targets.len()));
            Ok(r)
        }
        Err(e) => {
            rep.fail(e.to_string());
            Err(e)
        }
    }
}

#[tauri::command]
async fn killswitch_execute(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<killswitch::KillReport> {
    let rep = Reporter::new(app, events::TASK_KILL_EXECUTE, 3);
    rep.phase(1, "扫描并核对待关闭进程");

    // 只观察不解锁：一键关闭不需要写 claude.exe，没理由为它开门。
    // 守卫在这里的作用只有一个 —— 收完进程重锁之后，
    // **把使用者原来拿着的租约还给他**。
    //
    // 2026-09-10 07:30:42 的实机日志就是反例：使用者点了一次一键关闭，
    // 门被关死，之后 Claude 桌面端每开一个新会话都报
    // `Claude Code couldn't start`，而他完全不会把两件事联系起来。
    let guard = gate::Maintenance::observing(&state.gate);
    let r = killswitch::execute().await;
    let done = guard.finish(&state.gate).await;
    match &r {
        Ok(v) => {
            rep.phase(2, format!("已关闭 {} 个进程，正在重新上锁", v.killed.len()));
            rep.done(format!("关闭完成。{}", done.detail));
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

    let rep = Reporter::new(app.clone(), events::TASK_TAVERN_START, 5);
    let url = match plugins::sillytavern::start(&rep).await {
        Ok(u) => u,
        Err(e) => {
            // 起失败就把租约还回去，不能让门开着。
            let _ = gate::release_lease(&state.gate);
            rep.fail(e.to_string());
            return Err(e);
        }
    };

    // 桥接跑起来之后由 QB Gate 的看门狗接管，不再起 PowerShell 的 WatchBridge。
    let (tx, rx) = tokio::sync::watch::channel(false);
    *state.watchdog_stop.lock().unwrap() = Some(tx);
    let gs = state.gate.clone();
    tauri::async_runtime::spawn(async move {
        gate::run_watchdog(gate::watchdog::WatchMode::Cli, gs, rx, Some(app)).await;
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
        // **关窗口 = 收进托盘，不是退出。**
        //
        // 退出会重锁全部副本（见下面 RunEvent::Exit 那段），而重锁挡住的是
        // **下一次启动**。Claude 桌面端的 Code 页每开一个新会话就要拉起一次
        // `%APPDATA%\Claude\claude-code\<版本>\claude.exe` —— 那个路径正在
        // `targets::lockable()` 里。于是实机现象是：正在聊的那个会话好好的，
        // 一开新会话就报「Claude Code couldn't start」，而使用者只是顺手把
        // 面板窗口关了，根本不知道这两件事有关系。
        //
        // 收进托盘之后看门狗还在跑、租约还在，门是有人看着的开 ——
        // 这跟「没人看着还敞着」是两码事，安全模型没有被放松。
        // 真要退出走托盘菜单的「退出 QB Gate」，那条路照旧重锁。
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 托盘没建起来就照旧退出 —— 藏起一个叫不回来的窗口更糟。
                if window.label() == "main" && tray::available() {
                    api.prevent_close();
                    let _ = window.hide();
                    gate::log::write("面板收进托盘（门禁与看门狗继续运行）");
                }
            }
        })
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            gate_status,
            gate_lock_all,
            gate_unlock_all,
            gate_reopen,
            gate_open,
            gate_release,
            gate_clean_stale,
            allowlist_read,
            allowlist_write,
            allowlist_add_current,
            country_presets,
            managed_history,
            managed_rollback,
            hook_status,
            hook_install,
            hook_uninstall,
            watchdog_start,
            watchdog_stop,
            probe_ip,
            probe_purity,
            probe_dns,
            purity_criteria,
            detect_software,
            install_probe,
            install_run,
            claude_traces,
            chrome_reinstall,
            launch_claude,
            accounts_list,
            accounts_create,
            accounts_switch,
            managed_status,
            managed_probe_dir,
            managed_set_dir,
            managed_externals,
            managed_cleanup,
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
            relay_detect_backend,
            settings_load,
            settings_save,
            snapshot_list,
            snapshot_create,
            snapshot_restore,
            snapshot_remove,
            snapshot_dir,
            profile_list,
            profile_save,
            profile_remove,
            profile_capture,
            profile_apply,
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

                    // 关好门之后，再看上次退出时门是不是开着的。
                    //
                    // **顺序不能反。** 「先别关门等我查完 IP」会在查询的那几秒里
                    // 留一扇没人看着的门，而面板刚启动时恰恰最可能是上次崩溃 /
                    // 断电留下的场面。所以是「宁可多锁一次，再决定要不要开」。
                    //
                    // 不做这件事的代价，是使用者反复报的那个现象：面板一重启，
                    // 租约随进程没了，桌面端 Code 页从此开不了新会话
                    // （`Claude Code couldn't start`），而且**没有任何东西会去救** ——
                    // `watchdog::decide` 要求 `lease_held` 才会走 `ReclaimLease`。
                    let handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        use tauri::Manager;
                        let st = handle.state::<AppState>();
                        if let Some(mode) = gate::try_restore_lease(&st.gate).await {
                            start_watchdog(mode, &st, Some(handle.clone()));
                            tray::refresh(&handle);
                        }
                    });
                }
                _ => {
                    gate::log::write("白名单为空，启动时不上锁（首次运行请先添加当前 IP）");
                }
            }
            // 托盘：不打开主窗口也能看状态、切账户、切中转站。
            // 建不起来不该让整个程序起不来 —— 有些精简版 Windows 没有
            // 通知区域，那时候面板本身仍然完全可用。
            if let Err(e) = tray::init(app.handle()) {
                gate::log::write(&format!("托盘建立失败（不影响面板使用）：{e}"));
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("启动 QB Gate 失败")
        .run(|_app, event| {
            // **面板退出时把门关上。**
            //
            // 没有这一步的话：用户在租约期内关掉面板，claude.exe 上的
            // Deny ACE 就一直摘着，看门狗也随进程没了 —— 门开着，
            // 而且没有任何东西在看。这跟「跑起来之后出口 IP 悄悄变了」
            // 是同一类风险，只是触发方式变成了「关掉了面板」。
            //
            // 启动时那段注释写的是「宁可多锁一次，也不要因为上次异常退出
            // 而敞着」——那是在给这个洞打补丁。现在正常退出这条路自己堵上了，
            // 启动时那道保险仍然留着（应对崩溃 / 断电）。
            //
            // 已经在跑的进程不受影响（Windows 不会因为加了 Deny ACE 就杀掉
            // 已加载的映像），挡住的是**下一次启动**。
            if matches!(event, tauri::RunEvent::Exit) {
                match gate::lock_all() {
                    Ok(n) => gate::log::write(&format!("面板退出，已重新上锁 {n} 个可执行文件")),
                    Err(e) => gate::log::write(&format!("面板退出时重锁失败：{e}")),
                }
            }
        });
}
