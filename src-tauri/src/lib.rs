//! ClaudeGate —— Claude 环境控制面板。
//!
//! 命令层。业务逻辑全在各自模块里，这里只做参数搬运与状态持有。

pub mod accounts;
pub mod error;
pub mod gate;
pub mod install;
pub mod probe;
pub mod progress;
pub mod relay;
pub mod sysenv;

use error::Result;
use std::sync::Arc;
use tauri::Manager;

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
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    let (tx, rx) = tokio::sync::watch::channel(false);
    *state.watchdog_stop.lock().unwrap() = Some(tx);
    let gs = state.gate.clone();
    tauri::async_runtime::spawn(async move {
        gate::run_watchdog(mode, gs, rx).await;
        let _ = app;
    });
    Ok(())
}

#[tauri::command]
fn watchdog_stop(state: tauri::State<'_, AppState>) {
    if let Some(tx) = state.watchdog_stop.lock().unwrap().take() {
        let _ = tx.send(true);
    }
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
async fn probe_dns() -> Result<probe::dns::DnsReport> {
    probe::dns::check().await
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
        "codex": install::detect::codex(),
        "browsers": install::detect::browsers(),
    })
}

#[tauri::command]
fn installers_list(app: tauri::AppHandle) -> Result<install::download::Lockfile> {
    let dir = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    install::download::load_lockfile(&dir)
}

#[tauri::command]
async fn installer_fetch(inst: install::download::Installer) -> Result<std::path::PathBuf> {
    install::download::fetch_verified(&inst).await
}

// ------------------------------------------------------------------ 账户

#[tauri::command]
fn accounts_list() -> serde_json::Value {
    serde_json::json!({
        "slots": accounts::slots(),
        "caveat": accounts::EXPIRY_CAVEAT,
    })
}

/// 只由界面上的手动点击触发。**不要**从任何自动路径调用它。
#[tauri::command]
fn accounts_switch(label: String) -> Result<()> {
    accounts::switch(&label)
}

// ------------------------------------------------------------------ 中转站

#[tauri::command]
fn relay_current() -> Option<relay::Provider> {
    relay::current_provider()
}

#[tauri::command]
fn relay_apply(provider: relay::Provider) -> Result<()> {
    relay::apply_provider(&provider)
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            gate_status,
            gate_lock_all,
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
            installers_list,
            installer_fetch,
            accounts_list,
            accounts_switch,
            relay_current,
            relay_apply,
            tz_current,
            tz_apply,
            tz_restore,
            progress_load,
            progress_set,
        ])
        .setup(|app| {
            // 启动时先把锁重建一遍。等价于现有实现的 -Mode Check：
            // 宁可多锁一次，也不要因为上次异常退出而敞着。
            let _ = gate::lock_all();
            let _ = app;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("启动 ClaudeGate 失败");
}
