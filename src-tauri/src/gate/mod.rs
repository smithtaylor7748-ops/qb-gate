//! IP 锁总装。
//!
//! 对应现有 ClaudeIpGate.ps1 的八个 Mode：
//!   Check / Preflight / Install / Edit / OpenBridge / OpenCode / OpenDesktop / Watch*
//! 在这里收敛成几个命令 + 一个看门狗任务。
//!
//! # v0.7.0：门关上之后要有东西把它打开
//!
//! 使用者反复报「用着用着 Claude 桌面端就不行了」，报错是
//! `Claude Code couldn't start`。根因不是宽限期太短，是**关上之后没有任何
//! 东西会去开**。三条路径各自都能把门关死：
//!
//!   1. 面板重启 —— 租约原来只在内存里，重启即失效，`setup()` 接着 `lock_all()`；
//!   2. 内部重锁 —— 升级 / 一键关闭 / 装包 / 回滚快照都无条件 `lock_all()`，
//!      哪怕使用者手上正拿着租约；
//!   3. 看门狗收摊 —— `StopEverything` 之后直接 `break`，从此无人看守。
//!
//! 对应三个补丁：租约落盘（`lease.rs`）、[`Maintenance`] 维护窗口、
//! 看门狗的**重整待命**。三者守的是同一条不变量：
//!
//!   **门开着 ⇔ 出口 IP 已核实且在白名单里。**
//!
//! 注意它们都只恢复「租约」，**不启动任何进程**，也不读限流状态 ——
//! README 合规边界那四条一条都没碰。

pub mod allowlist;
pub mod lease;
pub mod log;
pub mod targets;
pub mod watchdog;

#[cfg(windows)]
pub mod acl;

use crate::error::{GateError, Result};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use watchdog::WatchMode;

/// 重整待命时多久看一次。
///
/// 比正常巡检慢得多是刻意的：这时候门已经关上、进程也收了，
/// 没有任何东西在冒险，只是在等网络回来。20 秒一轮去打第三方接口，
/// 网络真断了的时候等于自己给自己刷限流。
const REARM_INTERVAL: Duration = Duration::from_secs(60);

/// 运行期状态目录。与现有实现保持同一个位置，方便老用户平滑迁移。
pub fn state_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ClaudeIpGate")
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GateStatus {
    pub current_ip: Option<String>,
    pub allowlist: Vec<String>,
    pub ip_allowed: bool,
    pub targets: Vec<targets::Target>,
    pub all_locked: bool,
    pub lease: lease::Lease,
    pub watchdog_running: bool,
    pub stale_copies: Vec<PathBuf>,
    pub recent_log: Vec<String>,
    /// 门是在使用者没要求的情况下关上的，而且还没能自己开回来。
    ///
    /// `Some(理由)` 时界面要挂常驻横幅 —— 这是唯一一个使用者**必须知道**
    /// 却完全看不见的状态：面板收在托盘里，`ip-gate.log` 等于没写，
    /// 而症状要等他下次在 Claude 桌面端开新会话才出现，
    /// 那时候他根本不会把两件事联系起来。
    pub needs_reopen: Option<String>,
}

#[derive(Default)]
pub struct GateState {
    pub lease: Mutex<lease::Lease>,
    pub watchdog_running: Mutex<bool>,
    /// 见 [`GateStatus::needs_reopen`]。
    pub needs_reopen: Mutex<Option<String>>,
}

impl GateState {
    /// 标记「门被关上了，而且没开回来」。
    fn mark_needs_reopen(&self, why: impl Into<String>) {
        *self.needs_reopen.lock().unwrap() = Some(why.into());
    }

    /// 门重新开了 —— 把标记撤掉。
    fn clear_needs_reopen(&self) {
        *self.needs_reopen.lock().unwrap() = None;
    }
}

// ---------------------------------------------------------------- ACL 包装
// 非 Windows 平台只保证能编译，实际功能不可用。

#[cfg(windows)]
fn with_sid<T>(f: impl FnOnce(&acl::SidBuf) -> Result<T>) -> Result<T> {
    let sid = acl::current_user_sid()?;
    f(&sid)
}

#[cfg(not(windows))]
fn with_sid<T>(_f: impl FnOnce(&()) -> Result<T>) -> Result<T> {
    Err(GateError::Other("IP 锁只在 Windows 上可用".into()))
}

#[cfg(windows)]
pub fn is_locked(p: &Path) -> Result<bool> {
    with_sid(|sid| acl::is_locked(p, sid))
}

#[cfg(not(windows))]
pub fn is_locked(_p: &Path) -> Result<bool> {
    Ok(false)
}

/// 上锁全部可锁目标。
///
/// G1：这里必须是「全部」，不能只锁租出去的那一条 ——
/// 自动更新写的新 exe 继承的是干净 ACL，只重锁租出的路径会漏掉它。
#[cfg(windows)]
pub fn lock_all() -> Result<usize> {
    with_sid(|sid| {
        let mut n = 0;
        let mut failed: Vec<(PathBuf, GateError)> = Vec::new();
        for p in targets::lockable() {
            // 双保险：即使 lockable() 哪天回归漏进了 app-* 副本，也不给它上锁 ——
            // 一加 Deny，桌面端开新窗口就崩。
            if targets::is_desktop_runtime_copy(&p) {
                continue;
            }
            match acl::lock(&p, sid) {
                Ok(()) => n += 1,
                Err(e) => failed.push((p, e)),
            }
        }
        // 一个都没锁上才算失败；部分失败不阻断 —— 宁可多锁也不要敞着。
        if n == 0 && !failed.is_empty() {
            return Err(failed.remove(0).1);
        }
        // 但部分失败**必须写进日志**。原来这里只记成功的个数，锁不上的那几份
        // 悄无声息 —— 机器级 winget（装在 Program Files 下，普通用户改不了 ACL）
        // 就会是这种情况，而界面上只会少一个数。
        match failed.first() {
            None => log::write(&format!("已上锁 {n} 个可执行文件")),
            Some((p, e)) => log::write(&format!(
                "已上锁 {n} 个可执行文件，另有 {} 个锁不上（例如 {}：{e}）",
                failed.len(),
                p.display()
            )),
        }
        Ok(n)
    })
}

#[cfg(not(windows))]
pub fn lock_all() -> Result<usize> {
    Err(GateError::Other("IP 锁只在 Windows 上可用".into()))
}

#[cfg(windows)]
pub fn unlock_all() -> Result<usize> {
    with_sid(|sid| {
        let mut n = 0;
        for p in targets::lockable() {
            if acl::unlock(&p, sid).is_ok() {
                n += 1;
            }
        }
        log::write(&format!("已解锁 {n} 个可执行文件"));
        Ok(n)
    })
}

#[cfg(not(windows))]
pub fn unlock_all() -> Result<usize> {
    Err(GateError::Other("IP 锁只在 Windows 上可用".into()))
}

/// 摘掉指定几个文件上的 Deny ACE。
///
/// 存在的理由只有一个：**把 Codex 移出门禁管辖时得能把它身上的锁摘掉。**
/// 那一刻它已经不在 `targets::lockable()` 里了，`unlock_all` 再也不会碰它 ——
/// 不单独摘的话，用户关掉开关之后 codex 仍然跑不起来，而面板显示一切正常。
#[cfg(windows)]
pub fn unlock_paths(paths: &[PathBuf]) -> usize {
    with_sid(|sid| {
        let mut n = 0;
        for p in paths {
            if acl::unlock(p, sid).is_ok() {
                n += 1;
            }
        }
        Ok(n)
    })
    .unwrap_or(0)
}

#[cfg(not(windows))]
pub fn unlock_paths(_paths: &[PathBuf]) -> usize {
    0
}

/// 清理升级残留的可绕过副本。
///
/// K1：**发起升级的那个会话自己占着最新一份，删不掉**（Windows 允许改名
/// 正在运行的 exe，不允许删除）。所以这里删不掉是正常的，不当错误处理；
/// 正确时机是**下一次**升级的开头。
pub fn clean_stale_copies() -> Vec<(PathBuf, bool)> {
    targets::stale_copies()
        .into_iter()
        .map(|p| {
            let ok = std::fs::remove_file(&p).is_ok();
            if ok {
                log::write(&format!("已清理残留副本 {}", p.display()));
            }
            (p, ok)
        })
        .collect()
}

pub fn collect_targets() -> Vec<targets::Target> {
    targets::lockable_with_kinds()
        .into_iter()
        .map(|(p, kind)| targets::Target {
            locked: is_locked(&p).unwrap_or(false),
            exists: p.exists(),
            kind,
            path: p,
        })
        .collect()
}

// ---------------------------------------------------------------- 放行

/// 放行并落盘。
///
/// ⚠ **落盘必须在这里做，不能塞回 `Lease::grant_with_mode` 里面。**
/// 第一版就是塞在里面的，代价是 `cargo test` 直接把使用者真实的
/// `lease.json` 写成了测试数据 —— 而下一次面板启动会照着那条伪造记录去
/// 恢复租约。见 `lease.rs` 里 `mutating_a_lease_never_touches_the_disk`。
fn lease_grant(state: &GateState, holder: &str, paths: Vec<PathBuf>, mode: Option<WatchMode>) {
    let mut l = state.lease.lock().unwrap();
    l.grant_with_mode(holder, paths, mode);
    lease::persist(&l);
}

/// 收回租约并落盘。
fn lease_release(state: &GateState) {
    let mut l = state.lease.lock().unwrap();
    l.release();
    lease::persist(&l);
}

/// 当前出口 IP 验得过、而且在白名单里吗？
///
/// 抽出来是因为放行、维护窗口收尾、重整待命三处走的必须是**同一套判定** ——
/// 各写一份迟早会有一份漏掉「白名单为空」这种边界。
async fn verified_ip() -> std::result::Result<String, GateError> {
    let allow = allowlist::read()?;
    match crate::probe::ip::public_ip().await.ok() {
        None => Err(GateError::IpUnknown),
        Some(ip) if !allowlist::contains(&allow, &ip) => Err(GateError::IpNotAllowed { ip }),
        Some(ip) => Ok(ip),
    }
}

/// 门禁放行：验 IP 通过才解锁。IP 不合格时**一个请求都不会发出去**。
pub async fn open_authorized(holder: &str, state: &GateState) -> Result<()> {
    open_authorized_with_mode(holder, None, state).await
}

/// 带看门狗档位的放行。
///
/// 档位要跟着租约一起落盘，面板重启后才接得回原来那一档。
/// 接错档等于偷偷改了安全口径 —— 把桌面端接成 Cli，
/// 「查不到 IP 立即关闭」就悄悄变成了 180 秒宽限。
pub async fn open_authorized_with_mode(
    holder: &str,
    mode: Option<WatchMode>,
    state: &GateState,
) -> Result<()> {
    match verified_ip().await {
        Err(GateError::IpUnknown) => {
            let _ = lock_all();
            Err(GateError::IpUnknown)
        }
        Err(e @ GateError::IpNotAllowed { .. }) => {
            let _ = lock_all();
            log::write(&format!("拒绝放行：{e}"));
            Err(e)
        }
        Err(e) => Err(e),
        Ok(ip) => {
            let granted = targets::lockable();
            unlock_all()?;
            lease_grant(state, holder, granted, mode);
            state.clear_needs_reopen();
            log::write(&format!("公网 IP 在白名单内（{ip}），已放行 {holder}"));
            Ok(())
        }
    }
}

/// 收回租约。G1：重锁全部副本，不只是租出去的那条。
pub fn release_lease(state: &GateState) -> Result<()> {
    lease_release(state);
    state.clear_needs_reopen();
    lock_all()?;
    log::write("租约已收回，全部副本重新上锁");
    Ok(())
}

/// 使用者手点的「重新放行」（托盘菜单与总览横幅都走这条）。
///
/// 沿用上次的持有者标识；没有记录就按 `manual` 记，
/// 免得日志里出现一个空的 holder。
pub async fn reopen_now(state: &GateState) -> Result<String> {
    let (holder, mode) = {
        let l = state.lease.lock().unwrap();
        (
            l.holder.clone(),
            l.mode,
        )
    };
    let holder = holder
        .or_else(|| lease::load().and_then(|l| l.holder))
        .unwrap_or_else(|| "manual".into());
    let mode = mode.or_else(|| lease::load().and_then(|l| l.mode));

    open_authorized_with_mode(&holder, mode, state).await?;
    Ok(format!("已重新放行 {holder}"))
}

// ---------------------------------------------------------------- 维护窗口

/// 维护窗口的收尾结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct MaintenanceOutcome {
    pub relocked: usize,
    /// 收尾时有没有把租约重新放出去。
    pub reopened: bool,
    pub detail: String,
}

/// 一段「面板自己要动 claude.exe / 要收进程」的时间。
///
/// # 它解决的问题
///
/// 升级、装包、一键关闭、回滚快照、切 Codex 门禁开关 —— 这五处收尾时都是
/// 无条件 `lock_all()`。于是使用者手上明明拿着租约，只是点了一次「一键关闭」，
/// 门就被关死了，而且**不会自己开回来**：`watchdog::decide` 要求 `lease_held`
/// 才会走 `Tick::ReclaimLease`，可租约根本没被收回，是门被单方面锁上了。
///
/// 实机日志里这个形态出现过两次（2026-09-10 07:26:59 升级重锁、
/// 07:30:42 一键关闭重锁），使用者的体感都是「过段时间桌面端就不行了」。
///
/// # 两种开法
///
/// | 构造 | 解锁吗 | 用在哪 |
/// |---|:--:|---|
/// | [`Maintenance::with_unlock`] | 是 | 要**写** claude.exe 的：升级、winget 装包 |
/// | [`Maintenance::observing`] | 否 | 只是会顺手重锁的：一键关闭、回滚快照、门禁开关 |
///
/// 一键关闭不需要写 claude.exe，就不该为它开门 —— 能少开一秒是一秒。
///
/// # 为什么不是纯 RAII
///
/// 收尾要重新验一次出口 IP，那是 async 的，而 `Drop` 不能 await。
/// 所以正常路径走 [`Maintenance::finish`]，`Drop` 只留一个**同步兜底**：
/// 没调 `finish` 就说明中途提前返回或者 panic 了，那就无条件重锁。
/// 宁可多锁一次，也不要留一扇没人看着的开门。
pub struct Maintenance {
    prior: Option<(String, Option<WatchMode>)>,
    unlocked: bool,
    finished: bool,
}

impl Maintenance {
    fn prior_of(state: &GateState) -> Option<(String, Option<WatchMode>)> {
        let l = state.lease.lock().unwrap();
        l.holder.clone().map(|h| (h, l.mode))
    }

    /// 开窗口并解锁。给要写 claude.exe 的操作用。
    pub fn with_unlock(state: &GateState) -> Self {
        let prior = Self::prior_of(state);
        let unlocked = match unlock_all() {
            Ok(_) => true,
            Err(e) => {
                // 解不开也要继续 —— 后面那个操作自己会失败并报出真正的原因，
                // 比在这里拦下来更有诊断价值。
                log::write(&format!("维护窗口：解锁失败（继续执行）：{e}"));
                false
            }
        };
        Self {
            prior,
            unlocked,
            finished: false,
        }
    }

    /// 开窗口但**不解锁**。给只是会顺手重锁的操作用。
    pub fn observing(state: &GateState) -> Self {
        Self {
            prior: Self::prior_of(state),
            unlocked: false,
            finished: false,
        }
    }

    /// 这个窗口到底有没有解开锁。调用方据此决定报错文案。
    pub fn did_unlock(&self) -> bool {
        self.unlocked
    }

    /// 收尾：重锁；原来有租约、而且出口 IP 现在仍然验得过，就重新放行。
    pub async fn finish(mut self, state: &GateState) -> MaintenanceOutcome {
        self.finished = true;

        // 先关门，再决定要不要重新开。顺序不能反 —— 反过来会在
        // 「验 IP 的那几秒」里留一扇没有重锁过的门。
        let relocked = match lock_all() {
            Ok(n) => n,
            Err(e) => {
                log::write(&format!("维护窗口：重锁失败：{e}"));
                0
            }
        };

        let Some((holder, mode)) = self.prior.take() else {
            return MaintenanceOutcome {
                relocked,
                reopened: false,
                detail: format!("重新上锁 {relocked} 个可执行文件。"),
            };
        };

        match verified_ip().await {
            Ok(ip) => {
                let granted = targets::lockable();
                match unlock_all() {
                    Ok(_) => {
                        lease_grant(state, &holder, granted, mode);
                        state.clear_needs_reopen();
                        let detail = format!(
                            "重新上锁 {relocked} 个可执行文件，随后确认出口 IP {ip} 仍在白名单内，已把租约重新放行给 {holder}。"
                        );
                        log::write(&detail);
                        MaintenanceOutcome {
                            relocked,
                            reopened: true,
                            detail,
                        }
                    }
                    Err(e) => {
                        let why = format!("重锁之后没能把租约还给 {holder}：{e}");
                        lease_release(state);
                        state.mark_needs_reopen(why.clone());
                        log::write(&why);
                        MaintenanceOutcome {
                            relocked,
                            reopened: false,
                            detail: why,
                        }
                    }
                }
            }
            Err(e) => {
                // 门保持关着 —— 这是对的。但要**说出来**，
                // 否则使用者只会在下次开新会话时撞见 Claude Code couldn't start。
                let why = format!(
                    "重新上锁 {relocked} 个可执行文件。原来放行给 {holder} 的租约没能恢复：{e}。Claude 桌面端现在开新会话会失败，到总览点一次「重新放行」即可。"
                );
                lease_release(state);
                state.mark_needs_reopen(why.clone());
                log::write(&why);
                MaintenanceOutcome {
                    relocked,
                    reopened: false,
                    detail: why,
                }
            }
        }
    }
}

impl Drop for Maintenance {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // 走到这里说明中途提前返回或者 panic 了。
        // 同步兜底，只关门，不尝试重新放行（那需要 await）。
        match lock_all() {
            Ok(n) => log::write(&format!("维护窗口异常结束，已兜底重新上锁 {n} 个可执行文件")),
            Err(e) => log::write(&format!("维护窗口异常结束，兜底重锁也失败了：{e}")),
        }
    }
}

/// 面板启动时试着把上次的租约接回来。
///
/// **调用方必须已经先 `lock_all()` 过一遍。** 这里是「关好门之后再看要不要开」，
/// 不是「先别关门等我查完」—— 后者会在查 IP 的那几秒里留一扇没人看着的门，
/// 而面板刚启动时恰恰最可能是上次崩溃 / 断电留下的场面。
///
/// 返回接回来的看门狗档位，调用方据此把看门狗重新挂上。
pub async fn try_restore_lease(state: &GateState) -> Option<WatchMode> {
    let rec = lease::load()?;
    let holder = rec.holder.clone().unwrap_or_else(|| "unknown".into());

    match verified_ip().await {
        Ok(ip) => {
            let granted = targets::lockable();
            if let Err(e) = unlock_all() {
                log::write(&format!("面板重启：上次的租约（{holder}）解锁失败：{e}"));
                lease::forget();
                return None;
            }
            lease_grant(state, &holder, granted, rec.mode);
            state.clear_needs_reopen();
            log::write(&format!(
                "面板重启：上次的租约（{holder}）仍然有效，出口 IP {ip} 在白名单内，已重新放行"
            ));
            rec.mode
        }
        Err(e) => {
            // 记录作废，免得下次启动又拿同一条陈旧记录去试。
            lease::forget();
            state.mark_needs_reopen(format!(
                "面板重启时上次的租约（{holder}）没能恢复：{e}。Claude 桌面端现在开新会话会失败。"
            ));
            log::write(&format!(
                "面板重启：上次的租约（{holder}）没能恢复：{e}，门保持关闭"
            ));
            None
        }
    }
}

fn refresh_tray(app: &Option<tauri::AppHandle>) {
    if let Some(a) = app {
        crate::tray::refresh(a);
    }
}

/// 看门狗主循环。决策全部委托给 `watchdog::decide`，这里只负责执行副作用。
///
/// ⚠ `decide` 与 `WatchMode::unknown_grace()` 是**安全口径**，本轮一行没改，
/// 那 10 条单测就是它们的护栏。这里新增的只有「收摊之后不退出，转入重整待命」。
pub async fn run_watchdog(
    mode: watchdog::WatchMode,
    state: std::sync::Arc<GateState>,
    mut stop: tokio::sync::watch::Receiver<bool>,
    app: Option<tauri::AppHandle>,
) {
    use watchdog::{decide, StopReason, Tick};

    *state.watchdog_running.lock().unwrap() = true;
    let mut unknown_since: Option<Instant> = None;
    // 重整待命中要还给谁。`None` = 正常巡检。
    let mut rearm_for: Option<String> = None;

    loop {
        if *stop.borrow_and_update() {
            break;
        }

        // ---- 重整待命：门已经关上、进程也收了，只等出口 IP 回到白名单。
        //
        // **它只恢复租约，不启动任何进程。** 「门开着」这个不变量没被放松：
        // 仍然是「出口 IP 已核实且在白名单里」才开。
        if let Some(holder) = rearm_for.clone() {
            if let Ok(ip) = verified_ip().await {
                let granted = targets::lockable();
                if unlock_all().is_ok() {
                    lease_grant(&state, &holder, granted, Some(mode));
                    state.clear_needs_reopen();
                    log::write(&format!(
                        "重整待命结束：出口 IP {ip} 回到白名单，已重新放行 {holder}（未启动任何进程）"
                    ));
                    refresh_tray(&app);
                    rearm_for = None;
                    unknown_since = None;
                    continue;
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(REARM_INTERVAL) => {}
                _ = stop.changed() => {}
            }
            continue;
        }

        let allow = allowlist::read().unwrap_or_default();
        let ip = crate::probe::ip::public_ip().await.ok();

        unknown_since = match (&ip, unknown_since) {
            (Some(_), _) => None,
            (None, Some(t)) => Some(t),
            (None, None) => Some(Instant::now()),
        };
        let unknown_for = unknown_since
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO);

        let lease_held = state.lease.lock().unwrap().is_held();
        let targets_locked = collect_targets().iter().any(|t| t.locked);

        match decide(
            ip.as_deref(),
            &allow,
            mode,
            lease_held,
            targets_locked,
            unknown_for,
        ) {
            Tick::Nothing => {}
            Tick::ReclaimLease => {
                if unlock_all().is_ok() {
                    state.clear_needs_reopen();
                    log::write(&format!(
                        "公网 IP 仍在白名单内（{}），Claude 已重新解锁。",
                        ip.as_deref().unwrap_or("?")
                    ));
                }
            }
            Tick::LockKeepProcess => {
                let _ = lock_all();
                log::write("查不到公网 IP，已上锁但保留进程，等网络恢复");
            }
            Tick::StopEverything(reason) => {
                let msg = match &reason {
                    StopReason::IpChanged(ip) => format!("出口 IP 变为 {ip}，不在白名单内"),
                    StopReason::IpUnknownTooLong => "持续查不到公网 IP 超过宽限期".into(),
                    StopReason::IpUnknownNoGrace => "查不到公网 IP（桌面端不给宽限）".into(),
                };
                log::write(&format!("收摊：{msg}"));

                let holder = state.lease.lock().unwrap().holder.clone();
                let _ = lock_all();
                if mode == watchdog::WatchMode::Desktop {
                    // 结果必须写进日志。旧版这里的错误整个被丢掉 ——
                    // 日志写着「收摊」，桌面端其实还开着，没有任何线索可查。
                    match crate::killswitch::close_desktop() {
                        Ok(n) => log::write(&format!("收摊：已关闭桌面端 {n} 个进程")),
                        Err(e) => log::write(&format!("收摊：关闭桌面端失败，它可能还开着：{e}")),
                    }
                }
                lease_release(&state);
                state.mark_needs_reopen(format!(
                    "{msg}，门禁已关闭。Claude 桌面端现在开新会话会失败。"
                ));
                refresh_tray(&app);

                // 收摊之后**不要就此退出**。老版本在这里 break，
                // 于是从此没有任何东西看着：出口 IP 后来回来了也不会有人开门，
                // 使用者只会在下次开新会话时撞见 Claude Code couldn't start。
                if crate::settings::gate_auto_rearm() {
                    rearm_for = Some(holder.unwrap_or_else(|| "manual".into()));
                    log::write(
                        "已转入重整待命：出口 IP 回到白名单后自动重新放行，期间不启动任何进程",
                    );
                } else {
                    break;
                }
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(mode.interval()) => {}
            _ = stop.changed() => {}
        }
    }

    *state.watchdog_running.lock().unwrap() = false;
}
