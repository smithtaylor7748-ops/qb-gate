//! IP 锁总装。
//!
//! 对应现有 ClaudeIpGate.ps1 的八个 Mode：
//!   Check / Preflight / Install / Edit / OpenBridge / OpenCode / OpenDesktop / Watch*
//! 在这里收敛成几个命令 + 一个看门狗任务。

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
}

#[derive(Default)]
pub struct GateState {
    pub lease: Mutex<lease::Lease>,
    pub watchdog_running: Mutex<bool>,
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
        let mut first_err = None;
        for p in targets::lockable() {
            // 双保险：即使 lockable() 哪天回归漏进了 app-* 副本，也不给它上锁 ——
            // 一加 Deny，桌面端开新窗口就崩。
            if targets::is_desktop_runtime_copy(&p) {
                continue;
            }
            match acl::lock(&p, sid) {
                Ok(()) => n += 1,
                Err(e) => {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
            }
        }
        // 一个都没锁上才算失败；部分失败记日志但不阻断 —— 宁可多锁也不要敞着。
        if n == 0 {
            if let Some(e) = first_err {
                return Err(e);
            }
        }
        log::write(&format!("已上锁 {n} 个可执行文件"));
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
    targets::lockable()
        .into_iter()
        .map(|p| targets::Target {
            locked: is_locked(&p).unwrap_or(false),
            exists: p.exists(),
            kind: targets::kind_of(&p),
            path: p,
        })
        .collect()
}

/// 门禁放行：验 IP 通过才解锁。IP 不合格时**一个请求都不会发出去**。
pub async fn open_authorized(holder: &str, state: &GateState) -> Result<()> {
    let allow = allowlist::read()?;
    let ip = crate::probe::ip::public_ip().await.ok();

    match ip.as_deref() {
        None => {
            let _ = lock_all();
            Err(GateError::IpUnknown)
        }
        Some(ip) if !allowlist::contains(&allow, ip) => {
            let _ = lock_all();
            log::write(&format!("拒绝放行：出口 IP {ip} 不在白名单内"));
            Err(GateError::IpNotAllowed { ip: ip.to_string() })
        }
        Some(ip) => {
            let granted = targets::lockable();
            unlock_all()?;
            state
                .lease
                .lock()
                .unwrap()
                .grant(holder, granted);
            log::write(&format!("公网 IP 在白名单内（{ip}），已放行 {holder}"));
            Ok(())
        }
    }
}

/// 收回租约。G1：重锁全部副本，不只是租出去的那条。
pub fn release_lease(state: &GateState) -> Result<()> {
    state.lease.lock().unwrap().release();
    lock_all()?;
    log::write("租约已收回，全部副本重新上锁");
    Ok(())
}

/// 看门狗主循环。决策全部委托给 `watchdog::decide`，这里只负责执行副作用。
pub async fn run_watchdog(
    mode: watchdog::WatchMode,
    state: std::sync::Arc<GateState>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    use watchdog::{decide, StopReason, Tick};

    *state.watchdog_running.lock().unwrap() = true;
    let mut unknown_since: Option<Instant> = None;

    loop {
        if *stop.borrow_and_update() {
            break;
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
                let _ = lock_all();
                if mode == watchdog::WatchMode::Desktop {
                    crate::install::detect::kill_desktop_processes();
                }
                state.lease.lock().unwrap().release();
                break;
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(mode.interval()) => {}
            _ = stop.changed() => {}
        }
    }

    *state.watchdog_running.lock().unwrap() = false;
}
