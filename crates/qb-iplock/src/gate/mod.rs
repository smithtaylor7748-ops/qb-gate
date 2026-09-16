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
pub mod hook;
pub mod judge;
pub mod lease;
pub mod targets;
pub mod watchdog;

use crate::error::{GateError, Result};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use ts_rs::TS;

use watchdog::WatchMode;

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export)]
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
    pub pending_stop: Mutex<Option<bool>>,
    pub manual_paused: std::sync::atomic::AtomicBool,
    pub generation: std::sync::atomic::AtomicU64,
    pub lease: Mutex<lease::Lease>,
    pub watchdog_running: Mutex<bool>,
    /// 见 [`GateStatus::needs_reopen`]。
    pub needs_reopen: Mutex<Option<String>>,
}

impl GateState {
    /// 标记「门被关上了，而且没开回来」。
    pub fn mark_needs_reopen(&self, why: impl Into<String>) {
        *self.needs_reopen.lock().unwrap() = Some(why.into());
    }

    /// 门重新开了 —— 把标记撤掉。
    pub fn clear_needs_reopen(&self) {
        *self.needs_reopen.lock().unwrap() = None;
    }
}

// ---------------------------------------------------------------- ACL 包装
// 非 Windows 平台只保证能编译，实际功能不可用。

#[cfg(windows)]
fn with_sid<T>(f: impl FnOnce(&crate::acl::SidBuf) -> Result<T>) -> Result<T> {
    let sid = crate::acl::current_user_sid()?;
    f(&sid)
}

#[cfg(not(windows))]
fn with_sid<T>(_f: impl FnOnce(&()) -> Result<T>) -> Result<T> {
    Err(GateError::Other("IP 锁只在 Windows 上可用".into()))
}

#[cfg(windows)]
pub fn is_locked(p: &Path) -> Result<bool> {
    crate::acl::locked(p)
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
            match crate::acl::lock(&p, sid) {
                Ok(()) => n += 1,
                Err(e) => failed.push((p, e)),
            }
        }
        if !failed.is_empty() {
            return Err(GateError::Other(format!(
                "已上锁 {n} 个文件，{} 个文件失败：{}",
                failed.len(),
                failed
                    .iter()
                    .map(|(p, e)| format!("{}：{e}", p.display()))
                    .collect::<Vec<_>>()
                    .join("；")
            )));
        }
        crate::audit::write(&format!("已上锁 {n} 个可执行文件"));
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
        let mut errors = Vec::new();
        for p in targets::lockable() {
            match crate::acl::unlock(&p, sid) {
                Ok(()) => n += 1,
                Err(e) => errors.push(format!("{}：{e}", p.display())),
            }
        }
        if !errors.is_empty() {
            return Err(GateError::Other(format!(
                "已解锁 {n} 个文件，另有失败：{}",
                errors.join("；")
            )));
        }
        crate::audit::write(&format!("已解锁 {n} 个可执行文件"));
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
            if crate::acl::unlock(p, sid).is_ok() {
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
                crate::audit::write(&format!("已清理残留副本 {}", p.display()));
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
pub fn lease_grant(state: &GateState, holder: &str, paths: Vec<PathBuf>, mode: Option<WatchMode>) {
    let mut l = state.lease.lock().unwrap();
    l.grant_with_mode(holder, paths, mode);
    lease::persist(&l);
}

/// 收回租约并落盘。
pub fn lease_release(state: &GateState) {
    let mut l = state.lease.lock().unwrap();
    l.release();
    lease::persist(&l);
}

/// 探一轮并给出裁决。**全项目唯一的判定入口。**
///
/// 放行、维护窗口收尾、重整待命、看门狗、会话内 hook 全走这一条 ——
/// 各写一份迟早会有一份漏掉「白名单为空」或者「国家层」这种维度，
/// 而漏掉的那一处就是现成的绕过入口。
pub async fn judge_now() -> judge::Judgement {
    let generation = *VERDICT_GENERATION.lock().unwrap();
    if let Err(error) = crate::settings::load_checked() {
        crate::audit::write(&format!("门禁配置不可读，拒绝放行：{error}"));
        let verdict = judge::Judgement::IpUnknown;
        write_verdict(&verdict, generation);
        return verdict;
    }
    let r = crate::probe::ip::reading().await;
    let allow = allowlist::read().unwrap_or_default();
    let countries = crate::settings::country_allowlist();
    let j = judge::judge(&r, &allow, &countries);
    write_verdict(&j, generation);
    j
}

/// 当前出口 IP 验得过、而且在白名单（含国家白名单）里吗？
async fn verified_ip() -> std::result::Result<String, GateError> {
    match judge_now().await {
        judge::Judgement::Allowed { ip } => Ok(ip),
        judge::Judgement::IpUnknown => Err(GateError::IpUnknown),
        // IP 不在白名单、国家不合格、国家查不到、国家冲突 —— 四种都是
        // 「查到了但没过」，放行侧的处理完全一样（上锁 + 拒绝），
        // 差别只在写给人看的那句话里。
        other => Err(GateError::GateRejected(other.reason())),
    }
}

/// 看门狗与手动检测每次都把结论落到这里，会话内 hook 只读它。
///
/// 这样 hook 不必每次请求都自己去网上问一轮 —— 那会给每条 prompt
/// 加上几百毫秒，而且三个探测源的额度也扛不住。
/// 带 `checked_at`，hook 自己判断新不新鲜（陈旧就自己探）。
pub fn verdict_path() -> PathBuf {
    crate::paths::state_dir().join("gate-verdict.json")
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Verdict {
    #[serde(default)]
    pub ip: Option<String>,
    pub allowed: bool,
    pub reason: String,
    /// Unix 秒。hook 拿它算新鲜度。
    pub checked_at: u64,
}

static VERDICT_GENERATION: Mutex<u64> = Mutex::new(0);

pub fn invalidate_verdict() -> Result<()> {
    let mut generation = VERDICT_GENERATION.lock().unwrap();
    *generation = generation.wrapping_add(1);
    crate::config_io::replace(&verdict_path(), None)
}
pub fn cached_verdict() -> Option<Verdict> {
    std::fs::read(verdict_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}
/// 判定缓存还新鲜吗。
pub fn verdict_fresh(v: &Verdict) -> bool {
    fresh_at(v.checked_at, unix_now())
}

/// 缓存的新鲜度窗口：45 秒。
///
/// 会话内 hook 每个请求都要问一次门禁，不缓存的话每个请求都要出一次网 ——
/// 那既慢又会把探测接口打到限流。45 秒是「够短，改了白名单很快生效」与
/// 「够长，连打十几个请求只查一次」之间的取舍。
const VERDICT_TTL_SECS: u64 = 45;

/// 纯判断：`checked_at` 这一刻做的判定，在 `now` 还算不算数。
///
/// **`now < checked_at` 一律算不新鲜**（返回 false）。系统时间被往回调过
/// （改时区、NTP 校正、双系统）的时候会出现这种数，而 `now - checked_at`
/// 在 u64 上是下溢 —— 算出来是个天文数字，`<= 45` 恰好为假，结果碰巧是对的。
/// 但那是巧合，不是设计。写成显式比较，免得哪天有人把类型换成 i64。
fn fresh_at(checked_at: u64, now: u64) -> bool {
    now >= checked_at && now - checked_at <= VERDICT_TTL_SECS
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 判定 → 缓存记录。**纯函数，不碰磁盘也不看时钟。**
///
/// `ip` 在「放行」和「IP 不在白名单」两种情况下都要记下来。
/// 只在放行时记的话，排障时最想知道的那个问题 ——
/// **「到底是哪个 IP 被拒的」** —— 恰好查不到。
fn verdict_of(j: &judge::Judgement, now: u64) -> Verdict {
    Verdict {
        ip: match j {
            judge::Judgement::Allowed { ip } | judge::Judgement::IpNotAllowed { ip } => {
                Some(ip.clone())
            }
            _ => None,
        },
        allowed: j.is_allowed(),
        reason: j.reason(),
        checked_at: now,
    }
}

fn with_current_verdict(generation: &Mutex<u64>, expected: u64, write: impl FnOnce()) {
    let current = generation.lock().unwrap();
    if *current == expected {
        write();
    }
}

fn write_verdict(j: &judge::Judgement, generation: u64) {
    with_current_verdict(&VERDICT_GENERATION, generation, || persist_verdict(j));
}

fn persist_verdict(j: &judge::Judgement) {
    let v = verdict_of(j, unix_now());
    let p = verdict_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    // 原子写：hook 可能正好在读，不能让它读到半截 JSON。
    if let Ok(body) = serde_json::to_string_pretty(&v) {
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, body).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
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
    let generation = state.generation.load(std::sync::atomic::Ordering::SeqCst);
    let verdict = verified_ip().await;
    if state.generation.load(std::sync::atomic::Ordering::SeqCst) != generation {
        return Err(GateError::Other("门禁操作已被更新的手动操作取消".into()));
    }
    match verdict {
        Err(GateError::IpUnknown) => {
            let _ = lock_all();
            Err(GateError::IpUnknown)
        }
        Err(e @ (GateError::IpNotAllowed { .. } | GateError::GateRejected(_))) => {
            let _ = lock_all();
            crate::audit::write(&format!("拒绝放行：{e}"));
            Err(e)
        }
        Err(e) => Err(e),
        Ok(ip) => {
            state
                .manual_paused
                .store(false, std::sync::atomic::Ordering::SeqCst);
            crate::config_io::replace(
                &crate::paths::state_dir().join("gate-intent.json"),
                Some(b"{\"paused\":false}"),
            )?;
            let granted = targets::lockable();
            unlock_all()?;
            lease_grant(state, holder, granted, mode);
            state.clear_needs_reopen();
            crate::audit::write(&format!("公网 IP 在白名单内（{ip}），已放行 {holder}"));
            Ok(())
        }
    }
}

/// 收回租约。G1：重锁全部副本，不只是租出去的那条。
pub fn release_holder(state: &GateState, holder: &str) -> Result<()> {
    let held = {
        let mut lease = state.lease.lock().unwrap();
        lease.release_holder(holder);
        lease::persist(&lease);
        lease.is_held()
    };
    if !held {
        lock_all()?;
    }
    Ok(())
}

pub fn pause(state: &GateState) -> Result<()> {
    state
        .manual_paused
        .store(true, std::sync::atomic::Ordering::SeqCst);
    state
        .generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let invalidated = invalidate_verdict();
    let persisted = crate::config_io::replace(
        &crate::paths::state_dir().join("gate-intent.json"),
        Some(b"{\"paused\":true}"),
    );
    finish_pause(invalidated, persisted)
}

/// Callers must perform emergency cleanup even when recording the pause fails.
pub fn finish_pause<T>(pause: Result<()>, action: Result<T>) -> Result<T> {
    match (pause, action) {
        (Ok(()), result) => result,
        (Err(e), Ok(_)) => Err(GateError::Other(format!(
            "操作已执行，但暂停记录未完整保存：{e}"
        ))),
        (Err(e), Err(action)) => Err(GateError::Other(format!(
            "暂停记录失败：{e}；操作未完成：{action}"
        ))),
    }
}

pub fn release_lease(state: &GateState) -> Result<()> {
    let paused = pause(state);
    lease_release(state);
    state.clear_needs_reopen();
    finish_pause(paused, lock_all())?;
    crate::audit::write("租约已收回，全部副本重新上锁");
    Ok(())
}

/// 使用者手点的「重新放行」（托盘菜单与总览横幅都走这条）。
///
/// 沿用上次的持有者标识；没有记录就按 `manual` 记，
/// 免得日志里出现一个空的 holder。
pub async fn reopen_now(state: &GateState) -> Result<String> {
    let (holder, mode) = {
        let l = state.lease.lock().unwrap();
        (l.holder.clone(), l.mode)
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

/// 收尾成功时给使用者看的那句话。
fn reopened_detail(relocked: usize, ip: &str, holder: &str) -> String {
    format!(
        "重新上锁 {relocked} 个可执行文件，随后确认出口 IP {ip} 仍在白名单内，已把租约重新放行给 {holder}。"
    )
}

/// 租约没能恢复时给使用者看的那句话。
///
/// # 这句话里有一半是必须的
///
/// 门保持关着是对的，但**必须说出来怎么自救**。不说的话，使用者下一次
/// 感知到这件事是在 Claude 桌面端开新会话时撞见 `Claude Code couldn't start`
/// —— 那时候他根本不会把它和几分钟前点过的「升级」联系起来。
/// 有单测钉着这句话里带着「重新放行」四个字。
fn restore_failed_detail(relocked: usize, holder: &str, why: &str) -> String {
    format!(
        "重新上锁 {relocked} 个可执行文件。原来放行给 {holder} 的租约没能恢复：{why}。Claude 桌面端现在开新会话会失败，到总览点一次「重新放行」即可。"
    )
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
    prior: Option<lease::Lease>,
    unlocked: bool,
    finished: bool,
}

impl Maintenance {
    fn prior_of(state: &GateState) -> Option<lease::Lease> {
        let l = state.lease.lock().unwrap();
        l.is_held().then(|| l.clone())
    }

    /// 开窗口并解锁。给要写 claude.exe 的操作用。
    pub fn with_unlock(state: &GateState) -> Self {
        let prior = Self::prior_of(state);
        let unlocked = match unlock_all() {
            Ok(_) => true,
            Err(e) => {
                // 解不开也要继续 —— 后面那个操作自己会失败并报出真正的原因，
                // 比在这里拦下来更有诊断价值。
                crate::audit::write(&format!("维护窗口：解锁失败（继续执行）：{e}"));
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
                crate::audit::write(&format!("维护窗口：重锁失败：{e}"));
                0
            }
        };

        if state
            .manual_paused
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return MaintenanceOutcome {
                relocked,
                reopened: false,
                detail: "已手动收回门禁，维护操作不会重新放行".into(),
            };
        }
        let Some(prior) = self.prior.take() else {
            return MaintenanceOutcome {
                relocked,
                reopened: false,
                detail: format!("重新上锁 {relocked} 个可执行文件。"),
            };
        };

        let holder = prior.holder.clone().unwrap_or_default();
        let generation = state.generation.load(std::sync::atomic::Ordering::SeqCst);
        match verified_ip().await {
            Ok(ip) => {
                let granted = targets::lockable();
                match unlock_all() {
                    Ok(_) => {
                        if state.generation.load(std::sync::atomic::Ordering::SeqCst) != generation
                            || state
                                .manual_paused
                                .load(std::sync::atomic::Ordering::SeqCst)
                        {
                            let _ = lock_all();
                            return MaintenanceOutcome {
                                relocked,
                                reopened: false,
                                detail: "维护期间已手动收回门禁".into(),
                            };
                        }
                        let mut lease = state.lease.lock().unwrap();
                        lease.granted.extend(granted);
                        lease::persist(&lease);
                        drop(lease);
                        state.clear_needs_reopen();
                        let detail = reopened_detail(relocked, &ip, &holder);
                        crate::audit::write(&detail);
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
                        crate::audit::write(&why);
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
                let why = restore_failed_detail(relocked, &holder, &e.to_string());
                lease_release(state);
                state.mark_needs_reopen(why.clone());
                crate::audit::write(&why);
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
            Ok(n) => crate::audit::write(&format!(
                "维护窗口异常结束，已兜底重新上锁 {n} 个可执行文件"
            )),
            Err(e) => crate::audit::write(&format!("维护窗口异常结束，兜底重锁也失败了：{e}")),
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
    let generation = state.generation.load(std::sync::atomic::Ordering::SeqCst);
    let rec = lease::load()?;
    let holder = rec.holder.clone().unwrap_or_else(|| "unknown".into());

    let verdict = verified_ip().await;
    let _guard = crate::operations::exclusive().await.ok()?;
    if state
        .manual_paused
        .load(std::sync::atomic::Ordering::SeqCst)
        || state.generation.load(std::sync::atomic::Ordering::SeqCst) != generation
    {
        return None;
    }
    match verdict {
        Ok(ip) => {
            let granted = targets::lockable();
            if let Err(e) = unlock_all() {
                crate::audit::write(&format!("面板重启：上次的租约（{holder}）解锁失败：{e}"));
                lease::forget();
                return None;
            }
            let mut restored = rec.clone();
            restored.granted.extend(granted);
            *state.lease.lock().unwrap() = restored;
            lease::persist(&state.lease.lock().unwrap());
            state.clear_needs_reopen();
            crate::audit::write(&format!(
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
            crate::audit::write(&format!(
                "面板重启：上次的租约（{holder}）没能恢复：{e}，门保持关闭"
            ));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_late_network_probe_cannot_repopulate_invalidated_gate_cache() {
        let generation = std::sync::Mutex::new(1);
        let cache = std::cell::Cell::new(None);
        super::with_current_verdict(&generation, 1, || cache.set(Some(true)));
        assert_eq!(cache.get(), Some(true));
        *generation.lock().unwrap() = 2;
        cache.set(None);
        super::with_current_verdict(&generation, 1, || cache.set(Some(true)));
        assert_eq!(cache.get(), None);
        super::with_current_verdict(&generation, 2, || cache.set(Some(false)));
        assert_eq!(cache.get(), Some(false));
    }
    use super::*;

    // ⚠ 这个文件里能测的都是**不碰真实运行期状态**的那部分：不动真 ACL、
    // 不联网、不写 %LOCALAPPDATA%\ClaudeIpGate\。`lock_all` / `unlock_all` /
    // `Maintenance::finish` 三个都要真的改文件权限或者出网，测它们就会违反
    // 那条硬规矩，所以这里测的是它们身上**纯的那一半**：新鲜度窗口、
    // 判定到缓存的映射、暂停记录与操作的四种组合、以及给使用者看的文案。

    fn verdict_at(checked_at: u64) -> Verdict {
        Verdict {
            ip: Some("1.2.3.4".into()),
            allowed: true,
            reason: "ok".into(),
            checked_at,
        }
    }

    #[test]
    fn a_verdict_is_fresh_right_up_to_the_ttl_and_stale_one_second_later() {
        // 边界两侧各钉一个。45 秒这个数字本身可以改，但「<= 是包含的」
        // 不该在某次重构里悄悄变成「<」—— 那会让 hook 每 45 秒多出一次网络往返。
        let now = 1_000_000u64;
        assert!(fresh_at(now - VERDICT_TTL_SECS, now));
        assert!(!fresh_at(now - VERDICT_TTL_SECS - 1, now));
        assert!(fresh_at(now, now));
    }

    #[test]
    fn a_clock_that_went_backwards_never_reads_as_fresh() {
        // 改时区、NTP 校正、双系统都会让 now < checked_at。
        // u64 下溢算出来的是天文数字，碰巧也 > 45 —— 但那是巧合。
        // 这条钉住「显式比较」，免得有人把类型换成 i64 之后行为静默翻转。
        assert!(!fresh_at(1_000_000, 999_999));
        assert!(!fresh_at(u64::MAX, 0));
    }

    #[test]
    fn verdict_fresh_reads_the_checked_at_field() {
        // 纯函数接上真实时钟之后仍然是同一套判断。
        assert!(verdict_fresh(&verdict_at(unix_now())));
        assert!(!verdict_fresh(&verdict_at(0)));
    }

    #[test]
    fn a_rejected_ip_is_recorded_in_the_cache_not_just_an_allowed_one() {
        // 排障时最想知道的就是「到底是哪个 IP 被拒的」。
        // 只在放行时记 ip 的话，那个问题恰好查不到。
        let v = verdict_of(
            &judge::Judgement::IpNotAllowed {
                ip: "203.0.113.9".into(),
            },
            42,
        );
        assert_eq!(v.ip.as_deref(), Some("203.0.113.9"));
        assert!(!v.allowed);
        assert_eq!(v.checked_at, 42);
        assert!(!v.reason.is_empty(), "拒绝理由是要直接显示给使用者的");
    }

    #[test]
    fn a_verdict_without_an_ip_records_none_instead_of_an_empty_string() {
        // 「查不到 IP」和「IP 是空字符串」是两件事，缓存里要分得开。
        let v = verdict_of(&judge::Judgement::IpUnknown, 7);
        assert_eq!(v.ip, None);
        assert!(!v.allowed);
    }

    #[test]
    fn an_allowed_verdict_keeps_the_ip_that_passed() {
        let v = verdict_of(
            &judge::Judgement::Allowed {
                ip: "198.51.100.7".into(),
            },
            9,
        );
        assert_eq!(v.ip.as_deref(), Some("198.51.100.7"));
        assert!(v.allowed);
    }

    #[test]
    fn finish_pause_reports_both_halves_when_either_one_fails() {
        // 四种组合，每一种的语义都不同：
        //   成功 + 成功 → 原样返回
        //   失败 + 成功 → **操作做了**，但要如实说记录没存上
        //   失败 + 失败 → 两件事都要说出来
        //   成功 + 失败 → 原样返回那个失败
        assert!(finish_pause(Ok(()), Ok(7)).is_ok());

        let e = finish_pause(Err(GateError::Other("盘满了".into())), Ok(7)).unwrap_err();
        let text = e.to_string();
        assert!(text.contains("操作已执行"), "得先说操作做了：{text}");
        assert!(text.contains("盘满了"), "得带上真正的原因：{text}");

        let e = finish_pause(
            Err(GateError::Other("盘满了".into())),
            Err::<(), _>(GateError::Other("没锁上".into())),
        )
        .unwrap_err();
        let text = e.to_string();
        assert!(text.contains("盘满了") && text.contains("没锁上"), "{text}");

        let e = finish_pause(Ok(()), Err::<(), _>(GateError::Other("没锁上".into()))).unwrap_err();
        assert!(e.to_string().contains("没锁上"));
    }

    #[test]
    fn the_needs_reopen_banner_can_be_raised_and_cleared() {
        // 这是唯一一个使用者**必须知道**却完全看不见的状态：面板收在托盘里、
        // ip-gate.log 等于没写，而症状要等他下次开新会话才出现。
        let state = GateState::default();
        assert!(state.needs_reopen.lock().unwrap().is_none());

        state.mark_needs_reopen("升级之后没能把租约还回去");
        assert_eq!(
            state.needs_reopen.lock().unwrap().as_deref(),
            Some("升级之后没能把租约还回去")
        );

        state.clear_needs_reopen();
        assert!(state.needs_reopen.lock().unwrap().is_none());
    }

    #[test]
    fn the_failure_message_tells_the_user_how_to_get_back_in() {
        // 门保持关着是对的，但不说的话，使用者下一次感知到这件事是在
        // 桌面端开新会话时撞见 `Claude Code couldn't start` ——
        // 那时候他不会把它跟几分钟前点过的「升级」联系起来。
        let text = restore_failed_detail(12, "desktop", "查不到当前出口 IP");
        assert!(text.contains("重新放行"), "得写清楚怎么自救：{text}");
        assert!(
            text.contains("查不到当前出口 IP"),
            "得带上真正的原因：{text}"
        );
        assert!(text.contains("12"), "得说重锁了几个：{text}");
        assert!(
            !text.contains("**"),
            "传给界面的是纯文本，写 ** 会显示成两个星号"
        );
    }

    #[test]
    fn the_success_message_names_the_ip_and_the_holder() {
        let text = reopened_detail(3, "1.2.3.4", "desktop");
        assert!(
            text.contains("1.2.3.4") && text.contains("desktop"),
            "{text}"
        );
        assert!(!text.contains("**"));
    }
}
