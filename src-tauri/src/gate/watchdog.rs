//! 出口 IP 看门狗。
//!
//! 这个文件承载了现有实现里最贵的两条教训（E3 / E4），移植时最容易丢，
//! 所以决策逻辑抽成了纯函数 `decide`，全部场景有单测覆盖，不需要联网、
//! 不需要动真 ACL、不需要碰真进程就能验。
//!
//! E3 —— IP 仍在白名单里、租约却被人收走了，看门狗要自己**要回来**。
//!   为什么：你会去开白名单编辑器，八成正是因为换了网络想加新 IP，
//!   而编辑器开场就重新上锁。于是「加白名单」这个动作本身把正在跑的
//!   Claude 废掉了，而且没有任何提示。同样的洞还有 Check、Install、
//!   以及 Claude 自动更新导致的掉锁 —— 让看门狗每轮多看一眼 ACL，一并兜住。
//!
//! E4 —— 「查不到 IP」和「IP 变了」必须分开处理。
//!   合并的写法（try { 验IP } catch { 上锁; 杀进程 }）会让网络抖一下、
//!   隧道重连一次、查询服务限一次流，就直接杀掉正在进行的会话。
//!   分开之后安全强度没有降低：查不到就**立刻上锁**（能不能发出请求由锁
//!   说了算，那才是真正的管控点），进程留着等网络回来。

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WatchMode {
    /// 桥接 / 交互式 Claude Code。可以「先冻住等等看」。
    Cli,
    /// Claude 桌面端。冻不住 —— 真正在跑的是 app-* 下那个不能加 Deny 的副本，
    /// 而且它早把自己加载进内存了，事后上锁对它毫无作用。
    /// 看门狗手里只有一招：关掉它。
    Desktop,
}

impl WatchMode {
    pub fn interval(self) -> Duration {
        match self {
            WatchMode::Cli => Duration::from_secs(15),
            WatchMode::Desktop => Duration::from_secs(20),
        }
    }

    /// 查不到 IP 的宽限期。
    ///
    /// 桌面端是 `None` —— 用户明确选的严格档，查不到就立刻关，不给宽限。
    /// 理由：桌面端冻不住，「等等看」的实际含义就是
    /// 「让它在无法核实的网络上继续跑」。
    /// 代价说清楚：VPN 重连、DNS 抖动、查询服务限流都会直接关掉正在用的桌面端，
    /// 会丢没保存的对话。
    pub fn unknown_grace(self) -> Option<Duration> {
        match self {
            WatchMode::Cli => Some(Duration::from_secs(180)),
            WatchMode::Desktop => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tick {
    /// IP 在白名单里，租约状态也对，什么都不做。
    Nothing,
    /// E3：IP 仍合法但锁被别人加回来了，把租约要回来。
    ReclaimLease,
    /// E4：查不到 IP —— 上锁，但**留着进程**，等网络回来自动续。
    LockKeepProcess,
    /// 收摊：上锁 + 关停。
    StopEverything(StopReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// 查到了，但不在白名单 —— 第一轮就关停，一点不含糊。
    IpChanged(String),
    /// 一直查不到，超过宽限期。
    IpUnknownTooLong,
    /// 桌面端：查不到就立刻关，没有宽限期这回事。
    IpUnknownNoGrace,
}

/// 看门狗每一轮的决策。纯函数，无副作用，全部分支可单测。
pub fn decide(
    ip: Option<&str>,
    allow: &[String],
    mode: WatchMode,
    lease_held: bool,
    targets_locked: bool,
    unknown_for: Duration,
) -> Tick {
    match ip {
        Some(ip) if allow.iter().any(|a| a == ip) => {
            // E3：租约在外、目标却是锁着的 —— 有人（编辑器 / Check / Install /
            // 自动更新）把锁加回来了。IP 明明还合法，把租约要回来。
            if lease_held && targets_locked {
                Tick::ReclaimLease
            } else {
                Tick::Nothing
            }
        }
        // 查到了但不在白名单（含白名单被清空的情况）——两种模式都第一轮关停。
        Some(ip) => Tick::StopEverything(StopReason::IpChanged(ip.to_string())),
        None => match mode.unknown_grace() {
            None => Tick::StopEverything(StopReason::IpUnknownNoGrace),
            Some(grace) if unknown_for > grace => {
                Tick::StopEverything(StopReason::IpUnknownTooLong)
            }
            Some(_) => Tick::LockKeepProcess,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IP: &str = "203.0.113.7";
    fn allow() -> Vec<String> {
        vec![IP.to_string()]
    }
    const ZERO: Duration = Duration::from_secs(0);

    #[test]
    fn ip_normal_does_nothing() {
        let t = decide(Some(IP), &allow(), WatchMode::Cli, true, false, ZERO);
        assert_eq!(t, Tick::Nothing);
    }

    #[test]
    fn jitter_then_recover_keeps_process() {
        // 抖动中：查不到，但还在宽限期内 —— 上锁不杀。
        let t = decide(None, &allow(), WatchMode::Cli, true, false, Duration::from_secs(30));
        assert_eq!(t, Tick::LockKeepProcess);
        // 恢复后：IP 回来了且仍合法，锁还在 -> 把租约要回来。
        let t = decide(Some(IP), &allow(), WatchMode::Cli, true, true, ZERO);
        assert_eq!(t, Tick::ReclaimLease);
    }

    #[test]
    fn persistent_failure_stops_after_grace() {
        let t = decide(None, &allow(), WatchMode::Cli, true, true, Duration::from_secs(181));
        assert_eq!(t, Tick::StopEverything(StopReason::IpUnknownTooLong));
    }

    #[test]
    fn ip_really_changed_stops_on_first_round() {
        let t = decide(Some("1.2.3.4"), &allow(), WatchMode::Cli, true, false, ZERO);
        assert_eq!(
            t,
            Tick::StopEverything(StopReason::IpChanged("1.2.3.4".into()))
        );
    }

    #[test]
    fn empty_allowlist_stops() {
        let t = decide(Some(IP), &[], WatchMode::Cli, true, false, ZERO);
        assert_eq!(t, Tick::StopEverything(StopReason::IpChanged(IP.into())));
    }

    #[test]
    fn lease_stolen_is_reclaimed() {
        // E3 的回归测试：这条挂了，「加白名单」就会静默废掉正在跑的 Claude。
        let t = decide(Some(IP), &allow(), WatchMode::Cli, true, true, ZERO);
        assert_eq!(t, Tick::ReclaimLease);
    }

    #[test]
    fn no_lease_means_nothing_to_reclaim() {
        let t = decide(Some(IP), &allow(), WatchMode::Cli, false, true, ZERO);
        assert_eq!(t, Tick::Nothing);
    }

    #[test]
    fn desktop_has_no_grace_at_all() {
        // 断言桌面端在**第一次**查不到时就动手，unknown_for = 0 也照关。
        let t = decide(None, &allow(), WatchMode::Desktop, true, false, ZERO);
        assert_eq!(t, Tick::StopEverything(StopReason::IpUnknownNoGrace));
    }

    #[test]
    fn desktop_ip_changed_also_stops() {
        let t = decide(Some("1.2.3.4"), &allow(), WatchMode::Desktop, true, false, ZERO);
        assert_eq!(
            t,
            Tick::StopEverything(StopReason::IpChanged("1.2.3.4".into()))
        );
    }

    #[test]
    fn intervals_match_existing_implementation() {
        assert_eq!(WatchMode::Cli.interval(), Duration::from_secs(15));
        assert_eq!(WatchMode::Desktop.interval(), Duration::from_secs(20));
        assert_eq!(WatchMode::Desktop.unknown_grace(), None);
    }
}
