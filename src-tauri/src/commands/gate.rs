//! IP 门禁相关的命令。
//!
//! 状态、上锁解锁、白名单、会话内 hook、看门狗、一键关闭。

use crate::sink::ProgressSink;
use crate::{
    audit,
    error::{GateError, Result},
    events::{self, Reporter},
    gate, killswitch, operations, probe, settings, tray, usecase, AppState,
};

// ------------------------------------------------------------------ 门禁

#[tauri::command]
pub async fn gate_status(state: tauri::State<'_, AppState>) -> Result<gate::GateStatus> {
    let allow = gate::allowlist::read()?;
    let cached = gate::cached_verdict();
    let current_ip = cached.as_ref().and_then(|v| v.ip.clone());
    let paused = state
        .gate
        .manual_paused
        .load(std::sync::atomic::Ordering::SeqCst);
    let targets = gate::collect_targets();
    Ok(gate::GateStatus {
        ip_allowed: !paused
            && cached
                .as_ref()
                .is_some_and(|v| v.allowed && gate::verdict_fresh(v)),
        current_ip,
        allowlist: allow,
        all_locked: !targets.is_empty() && targets.iter().all(|t| t.locked),
        targets,
        lease: state.gate.lease.lock().unwrap().clone(),
        watchdog_running: *state.gate.watchdog_running.lock().unwrap(),
        stale_copies: gate::targets::stale_copies(),
        recent_log: audit::tail(20),
        needs_reopen: state.gate.needs_reopen.lock().unwrap().clone(),
    })
}

#[tauri::command]
pub async fn gate_lock_all(state: tauri::State<'_, AppState>) -> Result<usize> {
    let paused = gate::pause(&state.gate);
    let _guard = operations::exclusive().await?;
    let report = gate::finish_pause(
        paused,
        usecase::gate_ops::stop_managed(&state.gate, false).await,
    )?;
    Ok(report.relocked)
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
pub async fn gate_unlock_all(state: tauri::State<'_, AppState>) -> Result<usize> {
    let _guard = operations::exclusive_unchecked().await;
    let paused = gate::pause(&state.gate);
    let n = gate::finish_pause(paused, gate::unlock_all())?;
    audit::write("应急解锁：已摘掉全部执行锁（未验证 IP）");
    Ok(n)
}

#[tauri::command]
pub async fn gate_open(holder: String, state: tauri::State<'_, AppState>) -> Result<()> {
    let _guard = operations::exclusive().await?;
    gate::open_authorized(&holder, &state.gate).await
}

#[tauri::command]
pub async fn gate_release(state: tauri::State<'_, AppState>) -> Result<()> {
    let paused = gate::pause(&state.gate);
    let _guard = operations::exclusive().await?;
    gate::finish_pause(
        paused,
        usecase::gate_ops::stop_managed(&state.gate, false).await,
    )?;
    Ok(())
}

/// 「重新放行」：验一次出口 IP，过了就把门重新打开。
///
/// 托盘菜单和总览横幅都走这条。它跟 `gate_open` 的唯一区别是**不需要调用方
/// 说出 holder** —— 沿用上一次的那个，因为使用者点它的场景永远是
/// 「门被面板自己关上了，我要把它开回来」。
///
/// 它**不启动任何进程**，只恢复租约。
#[tauri::command]
pub async fn gate_reopen(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String> {
    let _guard = operations::exclusive().await?;
    let detail = crate::app::reopen_and_rewatch(&state, Some(app.clone())).await?;
    tray::refresh(&app);
    Ok(detail)
}

#[tauri::command]
pub fn gate_clean_stale() -> Vec<(std::path::PathBuf, bool)> {
    gate::clean_stale_copies()
}

#[tauri::command]
pub fn allowlist_read() -> Result<Vec<String>> {
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
pub async fn allowlist_write(entries: Vec<String>) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    gate::allowlist::write(&entries)
}

/// 把当前出口 IP 加进白名单 —— **国家不合格就一个字都不写**。
///
/// 这一关堵的是「先把脏 IP 塞进白名单，再回头抱怨门禁没用」。
/// 这条路上我们手里正好有一轮完整的多源探测，查国家是免费的，
/// 所以这里可以严，也必须严。
#[tauri::command]
pub async fn allowlist_add_current() -> Result<Vec<String>> {
    let _guard = operations::exclusive().await?;
    let reading = probe::ip::reading().await;
    let ip =
        gate::judge::may_add(&reading, &settings::country_allowlist()).map_err(|j| match j {
            gate::judge::Judgement::IpUnknown => GateError::IpUnknown,
            other => GateError::GateRejected(format!("{}，拒绝加入白名单", other.reason())),
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
pub fn hook_status() -> gate::hook::HookStatus {
    usecase::hook_ops::status()
}

#[tauri::command]
pub async fn hook_install() -> Result<gate::hook::HookStatus> {
    let _guard = operations::exclusive_soon().await?;
    usecase::hook_ops::install()
}

#[tauri::command]
pub async fn hook_uninstall() -> Result<gate::hook::HookStatus> {
    let _guard = operations::exclusive_soon().await?;
    usecase::hook_ops::uninstall()
}

/// 国家白名单的两个起手式。**面板不替你选**，只是省得手打。
#[tauri::command]
pub fn country_presets() -> Vec<(String, Vec<String>)> {
    vec![
        (
            "只留美国".into(),
            gate::judge::PRESET_US
                .iter()
                .map(|s| s.to_string())
                .collect(),
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
pub fn watchdog_start(
    app: tauri::AppHandle,
    mode: gate::watchdog::WatchMode,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    crate::app::start_watchdog(mode, &state, Some(app));
    Ok(())
}

#[tauri::command]
pub fn watchdog_stop(state: tauri::State<'_, AppState>) -> Result<()> {
    // Keep one supervisor alive; pause policy execution without creating a replacement race.
    gate::pause(&state.gate)
}

// ------------------------------------------------------------ 一键关闭

/// 只看不动，把会被收的进程列给用户确认。
///
/// **回 `Result`。** 扫描失败得让用户看见 —— 老版本这里回的是裸
/// `KillReport`，枚举一出错就退化成一份空报告，界面照样显示
/// 「发现 0 个可关闭进程」，跟真的没有进程长得一模一样。
#[tauri::command]
pub async fn killswitch_preview(app: tauri::AppHandle) -> Result<killswitch::KillReport> {
    let rep = Reporter::new(app, events::TASK_KILL_PREVIEW, 2);
    rep.phase(1, "扫描 Claude 相关进程并核对证据");
    match killswitch::preview().await {
        Ok(r) => {
            rep.done(&format!("扫描完成，发现 {} 个可关闭进程", r.targets.len()));
            Ok(r)
        }
        Err(e) => {
            rep.fail(&e.to_string());
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn killswitch_execute(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<killswitch::KillReport> {
    let paused = gate::pause(&state.gate);
    let _guard = operations::exclusive().await?;
    let rep = Reporter::new(app.clone(), events::TASK_KILL_EXECUTE, 3);
    rep.phase(1, "核验并停止全部受管会话");
    let result = gate::finish_pause(
        paused,
        usecase::gate_ops::stop_managed(&state.gate, true).await,
    );
    match &result {
        Ok(_) => rep.done("受管会话已停止，门禁保持关闭；再次启动需重新验证"),
        Err(e) => rep.fail(&e.to_string()),
    }
    operations::changed(&events::ui(&app), "session", "");
    tray::refresh(&app);
    result
}
