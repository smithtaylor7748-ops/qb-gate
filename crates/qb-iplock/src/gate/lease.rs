//! 执行租约。
//!
//! 「租约」= 门禁临时摘掉 Deny ACE，放某个进程起来。租约在外时目标是解锁的；
//! 收回租约就是重新上锁。
//!
//! G1 的教训写在这里：租约退出时必须重锁**全部**副本，而不只是租出去的那一个。
//! 因为 Claude 自动更新会写一个全新的 exe，新文件继承干净 ACL，
//! 那条 Deny 跟着旧文件一起没了 —— 只重锁「本次租出去的那条路径」会漏掉它。
//!
//! # 为什么要落盘（v0.7.0）
//!
//! 租约原来只活在内存里。于是**面板每重启一次，门就自己关上一次**：
//! `setup()` 开场 `lock_all()`，而重锁挡住的正是
//! `%APPDATA%\Claude\claude-code\<版本>\claude.exe` —— 桌面端 Code 页
//! 每开一个新会话都要拉起它。使用者的体感是「用着用着就不行了」，
//! 报错是 `Claude Code couldn't start`，而他只是重启了一次面板。
//!
//! 更糟的是这种状态**没有任何东西会去救**：`watchdog::decide` 要求
//! `lease_held` 才会走 `Tick::ReclaimLease`，而租约刚刚随进程一起没了。
//!
//! 落盘之后 `setup()` 能问出「上次退出时门是开着的吗」，
//! **然后重新验一次 IP 再决定**。注意落盘的是「谁在用」，不是「可以放行」——
//! 恢复租约仍然要过 `allowlist` 那一关，这条不变量没有被放松。

use std::collections::BTreeSet;
use std::path::PathBuf;
use ts_rs::TS;

use super::watchdog::WatchMode;

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct Lease {
    #[serde(default)]
    pub holders: std::collections::BTreeMap<String, Option<WatchMode>>,
    pub holder: Option<String>,
    #[serde(default)]
    pub granted: BTreeSet<PathBuf>,
    /// 这个租约挂的是哪一档看门狗。
    ///
    /// 面板重启后要按原样把看门狗接回去 —— 接错档比不接更糟：
    /// 把桌面端接成 Cli 档，等于把「查不到 IP 立即关闭」偷偷改成 180 秒宽限，
    /// 而使用者以为自己还在严格档上。
    #[serde(default)]
    pub mode: Option<WatchMode>,
}

/// ⚠ **这几个方法一律不落盘。**
///
/// 落盘由 `gate::mod` 里的 `lease_grant` / `lease_release` 显式做。
///
/// 第一版把 `persist()` 写在 `grant` / `release` 里面，代价当场就付了：
/// `cargo test` 一跑，`the_watch_mode_survives_a_round_trip` 这条**单元测试
/// 直接把使用者真实的 `%LOCALAPPDATA%\ClaudeIpGate\lease.json` 写掉了**，
/// 于是下一次面板启动读到一条测试伪造的租约记录，跑去尝试恢复它。
///
/// 那一次恢复恰好也验了 IP、结果也正确，所以它**看起来什么事都没有** ——
/// 这正是最难发现的那一类。档案里那条「全部打桩：不联网、不动真 ACL、
/// 不碰真进程」少写了一句：**也不许写运行期状态文件**。
///
/// 纯数据类型不做 I/O，这条不变量由下面 `mutating_a_lease_never_touches_the_disk`
/// 钉着。
impl Lease {
    pub fn normalize(&mut self) {
        if self.holders.is_empty() {
            if let Some(h) = &self.holder {
                self.holders.insert(h.clone(), self.mode);
            }
        }
    }
    pub fn is_held(&self) -> bool {
        self.holder.is_some()
    }

    /// 带看门狗档位的放行。**不落盘**，见上面那段。
    pub fn grant_with_mode(&mut self, holder: &str, paths: Vec<PathBuf>, mode: Option<WatchMode>) {
        self.normalize();
        self.holders.insert(holder.to_string(), mode);
        self.holder = Some(self.holders.keys().cloned().collect::<Vec<_>>().join("、"));
        self.granted.extend(paths);
        self.mode = if self
            .holders
            .values()
            .any(|m| *m == Some(WatchMode::Desktop))
        {
            Some(WatchMode::Desktop)
        } else {
            Some(WatchMode::Cli)
        };
    }

    pub fn release_holder(&mut self, holder: &str) {
        self.normalize();
        self.holders.remove(holder);
        if self.holders.is_empty() {
            self.release();
        } else {
            self.holder = Some(self.holders.keys().cloned().collect::<Vec<_>>().join("、"));
            self.mode = if self
                .holders
                .values()
                .any(|m| *m == Some(WatchMode::Desktop))
            {
                Some(WatchMode::Desktop)
            } else {
                Some(WatchMode::Cli)
            };
        }
    }

    /// **不落盘**，见上面那段。
    pub fn release(&mut self) {
        self.holders.clear();
        self.holder = None;
        self.granted.clear();
        self.mode = None;
    }
}

pub fn path() -> PathBuf {
    crate::paths::state_dir().join("lease.json")
}

/// 落盘。**失败只记日志，不往上抛。**
///
/// 租约本身是内存里那份说了算，磁盘那份只是给下次启动看的线索。
/// 为了写不进一个提示文件就让「放行」这个动作失败，是本末倒置。
pub fn persist(l: &Lease) {
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let r = match serde_json::to_string_pretty(l) {
        Ok(text) => crate::config_io::replace(&p, Some(text.as_bytes())).map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    if let Err(e) = r {
        crate::audit::write(&format!("租约落盘失败（不影响本次放行）：{e}"));
    }
}

/// 读回上次的租约记录。
///
/// **读到不等于可以放行。** 调用方必须再验一次出口 IP ——
/// 这个文件只回答「上次退出时门是开着的、给谁开的」。
pub fn load() -> Option<Lease> {
    let text = std::fs::read_to_string(path()).ok()?;
    let mut l: Lease = serde_json::from_str(&text).ok()?;
    l.normalize();
    l.is_held().then_some(l)
}

/// 清掉磁盘上那份。恢复失败时用，免得下次启动又拿着同一条陈旧记录去试。
pub fn forget() {
    let _ = std::fs::remove_file(path());
}

#[cfg(test)]
mod tests {
    #[test]
    fn releasing_official_preserves_relay_holder_and_permission_intent() {
        let mut lease = super::Lease::default();
        lease.grant_with_mode("official", vec![], Some(super::WatchMode::Desktop));
        lease.grant_with_mode("relay", vec![], Some(super::WatchMode::Cli));
        lease.release_holder("official");
        assert!(lease.is_held());
        assert_eq!(lease.holder.as_deref(), Some("relay"));
        assert_eq!(lease.mode, Some(super::WatchMode::Cli));
        lease.release_holder("official");
        assert!(lease.is_held());
        lease.release_holder("relay");
        assert!(!lease.is_held());
    }
    #[test]
    fn old_single_holder_is_preserved_when_new_relay_launches() {
        let mut lease: super::Lease =
            serde_json::from_str(r#"{"holder":"old-official","granted":[],"mode":"Cli"}"#).unwrap();
        lease.grant_with_mode("relay", vec![], Some(super::WatchMode::Cli));
        assert!(lease.holders.contains_key("old-official"));
        lease.release_holder("relay");
        assert_eq!(lease.holder.as_deref(), Some("old-official"));
    }

    use super::*;

    #[test]
    fn a_fresh_lease_is_not_held() {
        assert!(!Lease::default().is_held());
    }

    #[test]
    fn release_clears_the_mode_too() {
        // 档位留着不清，下次启动会拿一个没有租约的 mode 去接看门狗。
        let mut l = Lease::default();
        l.mode = Some(WatchMode::Desktop);
        l.holder = Some("claude-desktop".into());
        l.release();
        assert!(l.mode.is_none());
        assert!(!l.is_held());
    }

    /// 改租约**不许碰磁盘**。
    ///
    /// 回归测试，而且这个坑是实机付过代价的：第一版把 `persist()` 写在
    /// `grant` / `release` 里，`cargo test` 一跑就把使用者真实的
    /// `lease.json` 写成了测试数据，下一次面板启动照着它去恢复租约。
    /// 那次恢复还验了 IP、结果也对，所以**表面上什么事都没有** ——
    /// 最难发现的一类。
    #[test]
    fn mutating_a_lease_never_touches_the_disk() {
        let p = path();
        let before = std::fs::metadata(&p).and_then(|m| m.modified()).ok();

        let mut l = Lease::default();
        l.grant_with_mode(
            "test-holder",
            vec![PathBuf::from("x")],
            Some(WatchMode::Cli),
        );
        l.release();

        let after = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
        assert_eq!(
            before,
            after,
            "单元测试把真实的 lease.json 写掉了：{}",
            p.display()
        );
    }

    /// 记录里必须带得住档位 —— 接错档等于偷偷改了安全口径。
    #[test]
    fn the_watch_mode_survives_a_round_trip() {
        let mut l = Lease::default();
        l.grant_with_mode(
            "claude-desktop",
            vec![PathBuf::from("C:/x/claude.exe")],
            Some(WatchMode::Desktop),
        );
        let text = serde_json::to_string(&l).unwrap();
        let back: Lease = serde_json::from_str(&text).unwrap();
        assert_eq!(back.holder.as_deref(), Some("claude-desktop"));
        assert_eq!(back.mode, Some(WatchMode::Desktop));
        assert_eq!(back.granted.len(), 1);
    }

    /// 旧版写下的记录（没有 `mode` 字段）不能让新版读崩。
    #[test]
    fn an_older_record_without_a_mode_still_loads() {
        let back: Lease = serde_json::from_str(r#"{"holder":"claude-code","granted":[]}"#).unwrap();
        assert!(back.is_held());
        assert!(back.mode.is_none());
    }

    /// 没有 holder 的记录等于「上次退出时门是关着的」，不该被当成可恢复的租约。
    #[test]
    fn a_released_record_is_not_a_restorable_lease() {
        let l: Lease = serde_json::from_str(r#"{"holder":null,"granted":[]}"#).unwrap();
        assert!(!l.is_held(), "holder 为空就不是租约，别拿它去恢复");
    }
}
