//! 设置、时区、上手进度、启动，以及中转站预设。

use crate::{
    accounts, audit, domain,
    error::{GateError, Result},
    events, gate, launch, operations, progress, relay, sessions, settings, sysenv, usecase,
    workspace, AppState,
};

/// 验 IP → 解锁 → **真的把进程拉起来** → 挂看门狗。
///
/// `gate_open` 只解锁不启动，所以旧界面上那两个「启动 …」按钮其实一个进程
/// 都没起过。这条链任何一步失败都会把租约还回去并重新上锁 ——
/// 门禁不过就一个进程都不起。
#[tauri::command]
pub async fn launch_claude(
    app: tauri::AppHandle,
    target: launch::LaunchTarget,
    state: tauri::State<'_, AppState>,
) -> Result<launch::LaunchResult> {
    let _guard = operations::exclusive().await?;
    let client = match target {
        launch::LaunchTarget::ClaudeCode => domain::Client::ClaudeCode,
        launch::LaunchTarget::ClaudeDesktop => domain::Client::ClaudeDesktop,
        launch::LaunchTarget::Codex => domain::Client::Codex,
    };
    let slot = if client == domain::Client::Codex {
        None
    } else {
        accounts::active_label(&accounts::AccountRoots::current())
    };
    let mut task = operations::Run::start(
        events::sink(&app),
        "启动官方会话",
        slot.as_deref().unwrap_or("default"),
    )?;
    let session = task.finish(
        workspace::launch(
            client,
            domain::IdentityKind::Official,
            slot.as_deref().unwrap_or(""),
            None,
            &state.gate,
        )
        .await,
    )?;
    crate::app::start_watchdog(target.watch_mode(), &state, Some(app.clone()));
    operations::changed(&events::ui(&app), "session", &session.id);
    Ok(launch::LaunchResult {
        target,
        path: launch::resolve(target)?.display().to_string(),
        pid: Some(session.pid),
        watchdog: target.watch_mode(),
        detail: session.detail,
        slot,
    })
}

// ------------------------------------------------------------------ 中转站

#[tauri::command]
pub fn relay_presets(target: relay::RelayTarget) -> Vec<relay::presets::Preset> {
    relay::presets::for_target(target)
}

// ------------------------------------------------------------------ 设置

#[tauri::command]
pub fn settings_load() -> Result<settings::Settings> {
    settings::load_checked()
}

/// 存设置。
///
/// 打开 `codex_under_gate` 之后**立刻按新的目标清单重新上锁一次**，
/// 否则「我打开了开关」和「codex 真的被锁上」之间会隔着一次重启，
/// 中间那段时间界面说管了、实际没管。关掉时同理要把 Codex 上的锁摘掉。
#[tauri::command]
pub async fn settings_save(mut next: settings::Settings) -> Result<settings::Settings> {
    let _guard = operations::exclusive_soon().await?;
    let before = settings::load_checked()?;
    if before.codex_under_gate != next.codex_under_gate
        && sessions::list()
            .iter()
            .any(|s| s.context.client == domain::Client::Codex && s.state == "running")
    {
        return Err(GateError::Other(
            "请先停止正在运行的 Codex 会话，再调整门禁范围".into(),
        ));
    }
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
    usecase::settings_ops::save(&next)?;

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
            audit::write("设置：Codex 已纳入 IP 门禁");
        } else {
            // 关掉开关时 Codex 上可能还挂着 Deny ACE。这时它已经不在目标
            // 清单里了，lock_all / unlock_all 再也不会碰它 —— 不单独摘掉的话，
            // 用户关了开关 codex 仍然跑不起来，而面板显示一切正常。
            let n = gate::unlock_paths(&gate::targets::codex_lockable());
            audit::write(&format!("设置：Codex 已移出 IP 门禁，摘除 {n} 处执行锁"));
        }
    }
    Ok(settings::load())
}

// ------------------------------------------------------------------ 时区

#[tauri::command]
pub fn tz_current() -> Result<String> {
    sysenv::current_windows_tz()
}

#[tauri::command]
pub fn tz_apply(
    iana: String,
    restore_on_exit: bool,
    state: tauri::State<'_, AppState>,
) -> Result<sysenv::TzState> {
    let st = sysenv::apply_timezone(&iana, restore_on_exit)?;
    *state.tz.lock().unwrap() = Some(st.clone());
    Ok(st)
}

#[tauri::command]
pub fn tz_restore(state: tauri::State<'_, AppState>) -> Result<()> {
    let st = state.tz.lock().unwrap().clone();
    match st {
        Some(st) => sysenv::restore_timezone(&st),
        None => Ok(()),
    }
}

// ------------------------------------------------------------------ 进度

#[tauri::command]
pub fn progress_load() -> progress::Progress {
    progress::load()
}

#[tauri::command]
pub fn progress_set(
    id: String,
    state: progress::StepState,
    risk: progress::Risk,
    detail: String,
) -> progress::Progress {
    progress::set(&id, state, risk, detail)
}
