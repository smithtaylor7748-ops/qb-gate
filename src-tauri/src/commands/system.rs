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
        launch::LaunchTarget::Antigravity => domain::Client::Antigravity,
        launch::LaunchTarget::AntigravityIde => domain::Client::AntigravityIde,
    };
    // Codex 与反重力各有各的登录，跟 Claude 槽位无关。
    let slot = if client == domain::Client::Codex || client.is_antigravity() {
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
/// 把 GPT 纳回门禁之后**立刻按新的目标清单重新上锁一次**，
/// 否则「我改了开关」和「codex 真的被锁上」之间会隔着一次重启，
/// 中间那段时间界面说管了、实际没管。移出时同理要把 Codex 上的锁摘掉。
/// （字段是反义的 `codex_outside_gate`，见 `settings.rs`；这里比的是它变没变。）
#[tauri::command]
pub async fn settings_save(mut next: settings::Settings) -> Result<settings::Settings> {
    let _guard = operations::exclusive_soon().await?;
    let before = settings::load_checked()?;
    if before.codex_outside_gate != next.codex_outside_gate
        && sessions::list()
            .iter()
            .any(|s| s.context.client == domain::Client::Codex && s.state == "running")
    {
        return Err(GateError::Other(
            "请先关闭正在运行的 GPT 桌面端，再调整门禁范围".into(),
        ));
    }
    // 反重力同理（0.26.0）：跑着的时候改门禁范围，租约与锁就对不上了。
    if before.antigravity_outside_gate != next.antigravity_outside_gate
        && sessions::list()
            .iter()
            .any(|s| s.context.client.is_antigravity() && s.state == "running")
    {
        return Err(GateError::Other(
            "请先关闭正在运行的反重力，再调整门禁范围".into(),
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
    // 「跳过这个版本」只经 `update_skip` 改（0.25.3）。设置页拿着打开页面时读的那份旧设置
    // 去存别的开关，不该顺手把使用者刚在更新弹窗里点的「跳过」冲掉。
    next.update_skipped_version = before.update_skipped_version.clone();
    // GPT 界面语言只经 `codex_locale_set` 改（2026-09-25）：那条命令同时改各槽位的 config.toml，
    // 这里跟着设置页那份旧快照翻这个布尔值，就成了「设置里写着中文、槽位里一个字没改」。
    next.gpt_ui_zh = before.gpt_ui_zh;
    usecase::settings_ops::save(&next)?;

    if before.codex_outside_gate != next.codex_outside_gate {
        if !next.codex_outside_gate {
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
            audit::write("设置：GPT（Codex）已纳回 IP 门禁");
        } else {
            // 移出时 Codex 上可能还挂着 Deny ACE。这时它已经不在目标
            // 清单里了，lock_all / unlock_all 再也不会碰它 —— 不单独摘掉的话，
            // 用户移出了 codex 仍然跑不起来，而面板显示一切正常。
            let n = gate::unlock_paths(&gate::targets::codex_lockable());
            audit::write(&format!(
                "设置：GPT（Codex）已移出 IP 门禁，摘除 {n} 处执行锁"
            ));
        }
    }
    // 反重力那个开关同一套处理（0.26.0）：纳回时门关着就顺手锁上，移出时把残留的 Deny 摘掉 ——
    // 不摘的话使用者移出了反重力仍然起不来，而面板显示一切正常。
    if before.antigravity_outside_gate != next.antigravity_outside_gate {
        if !next.antigravity_outside_gate {
            let gate_closed = gate::collect_targets()
                .iter()
                .filter(|t| t.kind != gate::targets::TargetKind::Antigravity)
                .all(|t| t.locked);
            if gate_closed {
                let _ = gate::lock_all();
            }
            audit::write("设置：反重力（Hub + IDE）已纳回 IP 门禁");
        } else {
            let roots = crate::install::inventory::Roots::current();
            let n = gate::unlock_paths(&gate::targets::antigravity_lockable(&roots));
            audit::write(&format!(
                "设置：反重力（Hub + IDE）已移出 IP 门禁，摘除 {n} 处执行锁"
            ));
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
