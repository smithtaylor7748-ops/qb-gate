//! 门禁的跨域编排。**L3 编排层：它可以向下调领域模块，领域模块不许回头调它。**
//!
//! # 为什么这些东西不能留在 `gate` 里
//!
//! `stop_managed` 和 `run_watchdog` 名义上属于门禁，实际上要同时碰
//! `killswitch`（杀进程）、`plugins`（停酒馆）、`sessions`（收会话）、
//! `tray`（刷菜单）、`operations`（互斥与变更广播）、`settings`（自动重整开关）。
//!
//! 它们留在 `gate` 里，就让 `gate` 同时成了「所有人都依赖的底层」和
//! 「反过来调所有人的编排器」—— 体检实测 15 对循环依赖里有 9 对缠着它，
//! 入度 25 是全项目最高。搬出来之后 `gate` 只剩判定与执行，
//! `gate ↔ killswitch`、`gate ↔ tray` 两对环直接消失。
//!
//! # 判定留在 `gate`，不许搬过来
//!
//! [`crate::gate::watchdog::decide`] 是纯函数，它决定每一轮该做什么；
//! 这里只负责**执行** `decide` 给出的结论。
//!
//! 这条链断过一次：早先 `run_watchdog` 自己写了一套「不通过就收」的逻辑，
//! `decide` 退化成只有单测在调的死代码 —— 而且**毫无症状**，因为两档
//! `unknown_grace()` 都是 `None`，自己写的那套算出来的结果跟 `decide` 一样，
//! 20 条单测照样全过。代价是下一个人照着 CLAUDE.md 改完 `unknown_grace()`、
//! 跑通全部测试、以为宽限期回来了，而实际一秒都没加上。
//!
//! 所以 `decide` / `Tick` / `StopReason` 的可见性是**故意收窄**的。
//! 本次从 `pub(super)` 放宽到 `pub(crate)` —— 仅仅因为调用方搬到了这个模块，
//! **守卫机制没有变**：`pub(crate)` 同样受 dead_code 检查，谁再把判定搬回
//! 调用方自己写一遍，`cargo clippy --lib -- -D warnings` 照样直接编译失败。
//! 别再往上放宽到 `pub`。

use std::time::Duration;

use crate::error::{GateError, Result};
use crate::gate::{
    collect_targets, judge, judge_now, lease_grant, lease_release, lock_all, release_holder,
    targets, unlock_all, watchdog, GateState,
};
use watchdog::WatchMode;

/// 通知界面刷新。
///
/// **编排层不认识托盘。** 这里原来直接调 `crate::tray::refresh` ——
/// L3 调 L4，方向反了，架构测试当场报出 `tray ↔ usecase` 这对新环。
/// 现在由调用方（`lib.rs` 的 `start_watchdog`，那是接口层）把刷新动作
/// 作为回调传进来。看门狗只管「该刷了」，不管刷的是托盘还是别的什么。
pub type RefreshUi = std::sync::Arc<dyn Fn() + Send + Sync>;

pub async fn stop_managed(
    state: &GateState,
    all_sessions: bool,
) -> Result<crate::killswitch::KillReport> {
    let mut errors = Vec::new();
    let relocked = match lock_all() {
        Ok(n) => n,
        Err(e) => {
            errors.push(e.to_string());
            0
        }
    };
    if let Err(e) = crate::plugins::sillytavern::stop().await {
        errors.push(e.to_string());
    }
    // 酒馆的 GPT 桥接活在面板进程里，它起的 `codex exec` 子进程跟酒馆一样归门禁收 ——
    // 出口 IP 不合格时留着它，等于留一条能继续对官方发请求的路。
    if let Err(e) = crate::gpt_bridge::stop() {
        errors.push(e.to_string());
    }
    // Gemini 桥接同理（0.26.0）：它起的 Gemini CLI 子进程带着 Google 登录往外发请求。
    if let Err(e) = crate::gemini_bridge::stop() {
        errors.push(e.to_string());
    }
    if let Err(e) = crate::sessions::stop_matching(|s| all_sessions || s.gated) {
        errors.push(e.to_string());
    }
    // 门禁驱动的关停（`all_sessions == false`）只收官方那一边：中转会话不归
    // 门禁管（见 `LaunchTarget::gated`），按 PID 把它们一起杀掉等于绕过了
    // 那条判断。使用者手点「一键关闭」时才是真的全收。
    let mut report = if all_sessions {
        crate::killswitch::execute().await
    } else {
        crate::killswitch::execute_official().await
    };
    // `execute_official` 不做第二次重锁（它只收官方进程，不动中转；反重力被移出门禁时也不动它），
    // 所以这一档要把函数开头那次上锁的数量报出去 —— 否则 `gate_lock_all`
    // 会如实地返回一个 0，而锁其实已经加上了。
    if let (false, Ok(r)) = (all_sessions, &mut report) {
        r.relocked = relocked;
    }
    match &report {
        Ok(r) => {
            for (pid, e) in &r.failed {
                errors.push(format!("PID {pid}: {e}"));
            }
        }
        Err(e) => errors.push(e.to_string()),
    }
    lease_release(state);
    if errors.is_empty() {
        *state.pending_stop.lock().unwrap() = None;
        report
    } else {
        *state.pending_stop.lock().unwrap() = Some(all_sessions);
        let detail = format!("部分关停未完成，将继续核验并重试：{}", errors.join("；"));
        state.mark_needs_reopen(detail.clone());
        Err(GateError::Other(detail))
    }
}

/// 门验过了：解锁，重整待命时顺手把租约接回来。
///
/// `Tick::Nothing`（重整待命那一支）与 `Tick::ReclaimLease`（E3）共用这一段 ——
/// 两者的处置本来就是同一件事，差别只在触发原因。
fn reopen_gate(state: &GateState, rearm: &mut bool, mode: WatchMode) {
    match unlock_all() {
        Ok(_) => {
            if *rearm {
                lease_grant(state, "manual", targets::lockable(), Some(mode));
                crate::audit::write("出口 IP 重新验过，租约已自动接回");
            }
            state.clear_needs_reopen();
            *rearm = false;
        }
        Err(e) => state.mark_needs_reopen(e.to_string()),
    }
}

/// A single supervisor serializes side effects; network probes cannot undo newer decisions.
///
/// # 每一轮的处置由 `watchdog::decide` 给出，这里只负责执行
///
/// 这条链曾经断过：`run_watchdog` 自己写了一套「不通过就收」的逻辑，
/// `decide` 退化成只有单测在调的死代码。当时看不出问题 —— 两档的
/// `unknown_grace()` 都是 `None`，自己写的那套跟 `decide` 算出来的结果一样。
///
/// 但 CLAUDE.md 里写着「把宽限期加回来只要改 `unknown_grace()` 一个函数，
/// `decide` 里的分支还在」。链断着的时候那句话是假的：照做、跑测试、
/// 20 条全过，宽限期一秒都加不上，而症状要等到某次网络抖动误杀了
/// 正在用的 Claude 才会露出来。**判定必须留在链路上，测试才算数。**
///
/// `fallback_mode` 只在没有租约（重整待命）时用；有租约就以租约里记的那一档
/// 为准 —— 多个持有者时 Desktop 压过 Cli，见 `lease::Lease::grant_with_mode`。
/// 接错档等于偷偷改了安全口径。
pub async fn run_watchdog(
    fallback_mode: watchdog::WatchMode,
    state: std::sync::Arc<GateState>,
    mut stop: tokio::sync::watch::Receiver<bool>,
    events: Option<std::sync::Arc<dyn crate::sink::EventSink>>,
    refresh_ui: RefreshUi,
) {
    use std::sync::atomic::Ordering;
    *state.watchdog_running.lock().unwrap() = true;
    let mut rearm = false;
    // 连续「查不到」是从哪一刻开始的。`decide` 拿它跟 `unknown_grace()` 比。
    // 判出任何别的结论都要清零，否则网络恢复之后旧的计时还会接着算。
    let mut unknown_since: Option<std::time::Instant> = None;
    loop {
        if *stop.borrow_and_update() {
            break;
        }
        let mut cleanup_blocked = false;
        {
            let Ok(_guard) = crate::operations::exclusive().await else {
                break;
            };
            // Recovery retries only previously unverified records, never recreates active jobs.
            if let Err(e) = crate::sessions::recover() {
                crate::audit::write(&format!("会话恢复：{e}"));
            }
            match crate::sessions::refresh() {
                Ok(true) => {
                    if let Some(a) = &events {
                        crate::operations::changed(a.as_ref(), "session", "");
                    }
                }
                Err(e) => crate::audit::write(&format!("会话检查：{e}")),
                _ => {}
            }
            // 退出的会话一律还租约。不能只还 `s.gated` 的那些 ——
            // 中转会话不归关停策略管，但它起的时候照样拿了租约，
            // 漏掉就等于它一退出，门就永久留在开着的状态。
            for session in crate::sessions::list()
                .into_iter()
                .filter(|s| s.state != "running")
            {
                if state
                    .lease
                    .lock()
                    .unwrap()
                    .holders
                    .contains_key(&session.id)
                {
                    if let Err(e) = release_holder(&state, &session.id) {
                        crate::audit::write(&e.to_string());
                    }
                    refresh_ui();
                }
            }
            let pending = *state.pending_stop.lock().unwrap();
            if let Some(all) = pending {
                cleanup_blocked = true;
                match stop_managed(&state, all).await {
                    Ok(_) => {
                        rearm = !state.manual_paused.load(Ordering::SeqCst)
                            && crate::settings::gate_auto_rearm();
                        state.mark_needs_reopen("关停已完成，等待重新验证门禁");
                    }
                    Err(e) => crate::audit::write(&e.to_string()),
                }
                if let Some(a) = &events {
                    crate::operations::changed(a.as_ref(), "session", "");
                }
                refresh_ui();
            }
        }
        // 本轮按哪一档判：租约里记的那一档说了算，没租约才退回启动时给的。
        let mode = state.lease.lock().unwrap().mode.unwrap_or(fallback_mode);
        let interval = mode.interval();
        if !cleanup_blocked
            && !state.manual_paused.load(Ordering::SeqCst)
            && (state.lease.lock().unwrap().is_held() || rearm)
        {
            let generation = state.generation.load(Ordering::SeqCst);
            let verdict = judge_now().await;
            let Ok(_guard) = crate::operations::exclusive().await else {
                break;
            };
            if !*stop.borrow()
                && state.generation.load(Ordering::SeqCst) == generation
                && !state.manual_paused.load(Ordering::SeqCst)
            {
                // 「查不到」要连续累计；其余任何结论都把计时清零。
                let unknown_for = if matches!(
                    verdict,
                    judge::Judgement::IpUnknown | judge::Judgement::CountryUnknown { .. }
                ) {
                    unknown_since
                        .get_or_insert_with(std::time::Instant::now)
                        .elapsed()
                } else {
                    unknown_since = None;
                    Duration::ZERO
                };
                let lease_held = state.lease.lock().unwrap().is_held();
                // E3：只要还有**任何**一个存在的目标被重新锁上就要回来。
                // 用 any 而不是 all —— 自动更新只会写回其中一个文件。
                let targets_locked = collect_targets().iter().any(|t| t.exists && t.locked);
                match watchdog::decide(&verdict, mode, lease_held, targets_locked, unknown_for) {
                    // 门开着、锁也没被人加回来。正在重整待命就把租约接回去。
                    watchdog::Tick::Nothing => {
                        if rearm && crate::settings::gate_auto_rearm() {
                            reopen_gate(&state, &mut rearm, mode);
                        } else if lease_held {
                            // 门确实开着才撤横幅。自动放行关着、正等使用者手动
                            // 「重新放行」的时候，横幅正是提示他去点的那个东西。
                            state.clear_needs_reopen();
                        }
                    }
                    // E3：IP 还合法，锁却被编辑器 / 安装 / 自动更新加了回来。
                    watchdog::Tick::ReclaimLease => reopen_gate(&state, &mut rearm, mode),
                    // 查不到，但这一档还留着宽限：上锁、留进程，等网络自己回来。
                    // ⚠ 两档 `unknown_grace()` 都是 `None` 时走不到这里。
                    watchdog::Tick::LockKeepProcess => {
                        let detail = format!(
                            "{}；已上锁并保留会话，等待网络恢复{}",
                            verdict.reason(),
                            lock_all()
                                .err()
                                .map(|e| format!("；上锁失败：{e}"))
                                .unwrap_or_default()
                        );
                        state.mark_needs_reopen(detail.clone());
                        crate::audit::write(&detail);
                    }
                    watchdog::Tick::StopEverything(reason) => {
                        let result = stop_managed(&state, false).await;
                        rearm = result.is_ok() && crate::settings::gate_auto_rearm();
                        unknown_since = None;
                        // 两句都要写：`reason()` 说为什么不通过（四种分得开），
                        // `policy()` 说为什么现在就收。见 `StopReason::policy`。
                        let detail = format!(
                            "{}（{}）；{}",
                            verdict.reason(),
                            reason.policy(),
                            result
                                .err()
                                .map(|e| e.to_string())
                                .unwrap_or_else(|| "受门禁管理的会话已停止".into())
                        );
                        state.mark_needs_reopen(detail.clone());
                        crate::audit::write(&detail);
                    }
                }
                if let Some(a) = &events {
                    crate::operations::changed(a.as_ref(), "gate", "");
                }
                refresh_ui();
            }
        } else if state.manual_paused.load(Ordering::SeqCst) {
            rearm = false;
        }
        tokio::select! {_=tokio::time::sleep(interval)=>{},changed=stop.changed()=>{if changed.is_err(){break;}}}
    }
    *state.watchdog_running.lock().unwrap() = false;
}
