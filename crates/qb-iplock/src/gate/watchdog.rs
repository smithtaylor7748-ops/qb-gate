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
//! E4 —— 「查不到 IP」和「IP 变了」必须分开**判定**。
//!   合并的写法（try { 验IP } catch { 上锁; 杀进程 }）让你连日志都看不出
//!   到底是网络抖了还是真的换了地方。判定至今仍然是分开的：`Judgement` 里
//!   `IpUnknown` / `CountryUnknown` 与 `IpNotAllowed` / `CountryNotAllowed`
//!   是四个不同的值，日志上写的也是四句不同的话。
//!
//!   ⚠ **但「查不到」的处置已经改了。** 原来是「上锁 + 留进程 + CLI 等 180 秒」，
//!   现在两档都是立刻收摊 —— 使用者明确选的严格档，理由与代价写在
//!   `WatchMode::unknown_grace` 上。别照着这段老注释以为进程还留着。

use std::time::Duration;
use ts_rs::TS;

use super::judge::Judgement;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub enum WatchMode {
    /// 桥接 / 交互式 Claude Code。
    ///
    /// 原来这一档可以「先冻住等等看」（查不到 IP 时留 180 秒宽限）。
    /// **现在不留了**，跟桌面端一样查不到就收 —— 见 `unknown_grace`。
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

    /// 查不到 IP 的宽限期。**两档都是 `None`：查不到就立刻收，不给宽限。**
    ///
    /// # 这里推翻了 E4 的一半，是使用者明确要求的
    ///
    /// E4（见文件头）说的是「查不到 IP」与「IP 变了」必须分开处理，
    /// 因为合并会让网络抖一下就杀掉正在进行的会话。这条**仍然成立**，
    /// 而且代码里仍然分得开 —— `Judgement` 里 `IpUnknown` / `CountryUnknown`
    /// 与 `IpNotAllowed` / `CountryNotAllowed` 是四个不同的值，日志上也分得开。
    ///
    /// 变的只是「查不到」这一档的**处置**：从「上锁 + 留进程 + 等 180 秒」
    /// 改成了「上锁 + 立刻收」。使用者的原话是「宁可错杀不可放过」，
    /// 而且他指出宽限期内本来就发不出新请求（门锁着、会话内 hook 在逐次拦），
    /// 留着的只是上下文，不是可用性。
    ///
    /// **代价写在这里，别让后来的人以为是疏忽：**
    /// VPN 重连、DNS 抖动、ippure / Cloudflare / ipinfo 三家同时限流，
    /// 都会直接关掉正在用的 Claude Code 与桌面端，**未保存的对话会丢**。
    /// 三源并发探测（`probe::ip::reading`）就是为了把这种误杀压到最低 ——
    /// 「查不到」现在意味着三家全挂，不是一家抽风。
    ///
    /// 要改回去：这个函数返回 `Some(Duration::from_secs(180))` 即可，
    /// `decide` 那边的分支还在。
    pub fn unknown_grace(self) -> Option<Duration> {
        match self {
            WatchMode::Cli | WatchMode::Desktop => None,
        }
    }
}

/// 一轮巡检的处置。
///
/// # 可见性是故意收窄的（`pub(crate)`）
///
/// `Tick` / `StopReason` / [`decide`] 只给 `usecase::gate_ops::run_watchdog` 用。
///
/// # 护栏换了一种，强度没降
///
/// 原来它们是 `pub(crate)`：一旦有人把 `run_watchdog` 改成自己判、不再调
/// `decide`，这几个就成了 dead_code，`cargo clippy --lib -- -D warnings`
/// 当场编译失败。拆 crate 之后 `run_watchdog` 在另一个 crate 里，
/// `pub(crate)` 它就调不到了，只能放宽到 `pub` —— 而 `pub` 在 `pub mod` 里
/// 逃出了 dead_code 分析，那条护栏也就没了。
///
/// 换上来的是 `tests/architecture.rs` 里的
/// `the_watchdog_still_routes_through_the_judge`：它直接读 `gate_ops` 的源码，
/// 断言 `run_watchdog` 里有 `watchdog::decide(`。**比 dead_code 更准** ——
/// dead_code 只能证明「有人在调」，这条证明的是「看门狗在调」。
///
/// 这条防线是补出来的：链路曾经真的断过一次，而且当时毫无症状 ——
/// 两档 `unknown_grace()` 都是 `None`，自己写的那套跟 `decide` 结果一样，
/// 20 条单测照样全过。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tick {
    /// IP 在白名单里，租约状态也对，什么都不做。
    Nothing,
    /// E3：IP 仍合法但锁被别人加回来了，把租约要回来。
    ReclaimLease,
    /// 查不到 IP —— 上锁，但**留着进程**，等网络回来自动续。
    ///
    /// ⚠ **当前配置下产生不出这个值**：两档的 `unknown_grace()` 都是 `None`。
    /// 留着它（和 `decide` 里那条分支）是为了把宽限加回去只需要改一个函数。
    LockKeepProcess,
    /// 收摊：上锁 + 关停。
    StopEverything(StopReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// 查到了，但不在白名单 —— 第一轮就关停，一点不含糊。
    IpChanged(String),
    /// 一直查不到，超过宽限期。⚠ 同 `Tick::LockKeepProcess`：
    /// 当前没有任何一档设了宽限，所以这个值产生不出来。
    IpUnknownTooLong,
    /// 桌面端：查不到就立刻关，没有宽限期这回事。
    IpUnknownNoGrace,
    /// 国家白名单层：IP 合法但落在名单外。跟 `IpChanged` 同档，第一轮就收。
    CountryChanged { ip: String, country: String },
    /// 国家白名单层：几个探测源报的国家打架。按最严的算，证据带在 detail 里。
    CountryConflict { ip: String, detail: String },
}

impl StopReason {
    /// 这一轮是按**哪一条处置规则**收的。
    ///
    /// 跟 `Judgement::reason()` 分工明确，两句都要写进 `ip-gate.log`：
    ///
    /// * `Judgement::reason()` 说的是「门禁**为什么**不通过」——
    ///   查不到 / 不在白名单 / 国家不对 / 国家冲突，四种分得开（硬约束 E4）。
    ///   使用者拿它决定该去查网络还是去换节点。
    /// * 这一句说的是「**为什么现在就收**」—— 第一轮即收，还是等过了宽限期才收。
    ///
    /// 现在两句听着像是重复的，因为 `unknown_grace()` 两档都是 `None`，
    /// 「查不到」永远走 `IpUnknownNoGrace`。**把宽限期加回去的那一天，
    /// 这两句会立刻分叉**，而那正是排查「为什么我的会话被关了」最需要的一行。
    pub fn policy(&self) -> &'static str {
        match self {
            StopReason::IpChanged(_) => "出口 IP 换了，第一轮即关停",
            StopReason::CountryChanged { .. } => "出口国家变了，第一轮即关停",
            StopReason::CountryConflict { .. } => "国家来源冲突，按不合格关停",
            StopReason::IpUnknownNoGrace => "这一档查不到即关停，没有宽限期",
            StopReason::IpUnknownTooLong => "查不到已持续超过宽限期",
        }
    }
}

/// 看门狗每一轮的决策。纯函数，无副作用，全部分支可单测。
///
/// # 「查不到」与「查到了不合格」为什么必须分开（E4 的国家层版本）
///
/// 国家这一维完全照搬 E4 的分法，不能因为使用者要「宁可错杀」就合并：
///
/// - **国家查到了、不在名单里** → 跟 IP 换了同档，第一轮就收。这不是抖动，
///   是真的换了地方。
/// - **国家查不到**（三家都没答出国家）→ 跟查不到 IP 同档。
///
/// 「同档」现在的含义是**两者都立刻收摊**（`unknown_grace()` 返回 `None`）。
/// 分开的意义仍然在：日志上写的是「查不到」还是「落在 HK」，
/// 决定了使用者该去查网络还是去换节点 —— 这两件事的处理方式完全相反。
///
/// 会话内 hook 那一档是独立的严格 fail-closed：即使看门狗这一轮还没醒，
/// 下一次请求也发不出去。两者合起来才是「严」。
pub fn decide(
    j: &Judgement,
    mode: WatchMode,
    lease_held: bool,
    targets_locked: bool,
    unknown_for: Duration,
) -> Tick {
    match j {
        Judgement::Allowed { .. } => {
            // E3：租约在外、目标却是锁着的 —— 有人（编辑器 / Check / Install /
            // 自动更新）把锁加回来了。IP 明明还合法，把租约要回来。
            if lease_held && targets_locked {
                Tick::ReclaimLease
            } else {
                Tick::Nothing
            }
        }
        // 查到了但不在白名单（含白名单被清空的情况）——两种模式都第一轮关停。
        Judgement::IpNotAllowed { ip } => Tick::StopEverything(StopReason::IpChanged(ip.clone())),
        Judgement::CountryNotAllowed { ip, country } => {
            Tick::StopEverything(StopReason::CountryChanged {
                ip: ip.clone(),
                country: country.clone(),
            })
        }
        Judgement::CountryConflict { ip, detail } => {
            Tick::StopEverything(StopReason::CountryConflict {
                ip: ip.clone(),
                detail: detail.clone(),
            })
        }
        // 探测侧失败的两种，走同一条处置（当前无宽限，直接收）。
        Judgement::IpUnknown | Judgement::CountryUnknown { .. } => match mode.unknown_grace() {
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
    const ZERO: Duration = Duration::from_secs(0);

    fn ok() -> Judgement {
        Judgement::Allowed { ip: IP.into() }
    }
    fn changed(ip: &str) -> Judgement {
        Judgement::IpNotAllowed { ip: ip.into() }
    }
    const UNKNOWN: Judgement = Judgement::IpUnknown;

    #[test]
    fn ip_normal_does_nothing() {
        let t = decide(&ok(), WatchMode::Cli, true, false, ZERO);
        assert_eq!(t, Tick::Nothing);
    }

    /// **CLI 档也不再给宽限**（使用者明确要求，见 `unknown_grace` 的说明）。
    ///
    /// 这条断言的是一个会丢数据的行为，所以要有测试盯着：抖动一次就收摊。
    /// 哪天有人想把宽限加回来，先改这条测试 —— 逼他在改之前先看见代价。
    #[test]
    fn cli_no_longer_gets_a_grace_period() {
        // unknown_for 填多少都一样，第一轮就收。
        for waited in [ZERO, Duration::from_secs(30), Duration::from_secs(181)] {
            let t = decide(&UNKNOWN, WatchMode::Cli, true, false, waited);
            assert_eq!(
                t,
                Tick::StopEverything(StopReason::IpUnknownNoGrace),
                "等了 {waited:?} 仍然应该是立刻收"
            );
        }
    }

    #[test]
    fn recovering_after_a_stop_reclaims_the_lease() {
        // 收摊之后转入重整待命；IP 回来且仍合法、锁还在 -> 把租约要回来。
        let t = decide(&ok(), WatchMode::Cli, true, true, ZERO);
        assert_eq!(t, Tick::ReclaimLease);
    }

    #[test]
    fn ip_really_changed_stops_on_first_round() {
        let t = decide(&changed("1.2.3.4"), WatchMode::Cli, true, false, ZERO);
        assert_eq!(
            t,
            Tick::StopEverything(StopReason::IpChanged("1.2.3.4".into()))
        );
    }

    #[test]
    fn lease_stolen_is_reclaimed() {
        // E3 的回归测试：这条挂了，「加白名单」就会静默废掉正在跑的 Claude。
        let t = decide(&ok(), WatchMode::Cli, true, true, ZERO);
        assert_eq!(t, Tick::ReclaimLease);
    }

    #[test]
    fn no_lease_means_nothing_to_reclaim() {
        let t = decide(&ok(), WatchMode::Cli, false, true, ZERO);
        assert_eq!(t, Tick::Nothing);
    }

    #[test]
    fn desktop_has_no_grace_at_all() {
        // 断言桌面端在**第一次**查不到时就动手，unknown_for = 0 也照关。
        let t = decide(&UNKNOWN, WatchMode::Desktop, true, false, ZERO);
        assert_eq!(t, Tick::StopEverything(StopReason::IpUnknownNoGrace));
    }

    #[test]
    fn desktop_ip_changed_also_stops() {
        let t = decide(&changed("1.2.3.4"), WatchMode::Desktop, true, false, ZERO);
        assert_eq!(
            t,
            Tick::StopEverything(StopReason::IpChanged("1.2.3.4".into()))
        );
    }

    // ------------------------------------------------------------ 国家层

    /// 国家查到了、不在名单里 = 真的换了地方，**第一轮就收**，跟 IP 换了同档。
    #[test]
    fn country_outside_the_list_stops_on_first_round() {
        let j = Judgement::CountryNotAllowed {
            ip: IP.into(),
            country: "HK".into(),
        };
        let t = decide(&j, WatchMode::Cli, true, false, ZERO);
        assert_eq!(
            t,
            Tick::StopEverything(StopReason::CountryChanged {
                ip: IP.into(),
                country: "HK".into()
            })
        );
    }

    #[test]
    fn country_conflict_stops_and_carries_the_evidence() {
        let j = Judgement::CountryConflict {
            ip: IP.into(),
            detail: "ippure=US cloudflare=HK".into(),
        };
        match decide(&j, WatchMode::Cli, true, false, ZERO) {
            Tick::StopEverything(StopReason::CountryConflict { detail, .. }) => {
                assert!(detail.contains("cloudflare=HK"), "证据不能在这一层丢掉");
            }
            other => panic!("国家冲突必须收摊，实际是 {other:?}"),
        }
    }

    /// E4 的国家层版本：**查不出国家 ≠ 国家不合格**。
    ///
    /// 这条挂了，ipinfo 限一次流就会杀掉正在进行的会话 —— 正是 E4 当初
    /// 花实机代价买来的那个教训。宽限期内门是锁着的，会话内 hook 还在
    /// 逐次请求拦，安全强度并没有降低。
    /// 查不出国家跟查不到 IP 走同一条处置 —— 现在都是立刻收。
    ///
    /// 注意「分得开」这件事没有变：`CountryUnknown` 与 `CountryNotAllowed`
    /// 仍然是两个不同的 `Judgement`，日志上写的也是两句不同的话。
    /// 变的只是处置，不是判定。
    #[test]
    fn country_unknown_is_handled_like_an_unknown_ip() {
        let j = Judgement::CountryUnknown { ip: IP.into() };
        for mode in [WatchMode::Cli, WatchMode::Desktop] {
            let t = decide(&j, mode, true, false, Duration::from_secs(30));
            assert_eq!(t, Tick::StopEverything(StopReason::IpUnknownNoGrace));
        }
    }

    #[test]
    fn intervals_match_existing_implementation() {
        assert_eq!(WatchMode::Cli.interval(), Duration::from_secs(15));
        assert_eq!(WatchMode::Desktop.interval(), Duration::from_secs(20));
        // 两档都不给宽限 —— 这是使用者定的严格档。
        assert_eq!(WatchMode::Desktop.unknown_grace(), None);
        assert_eq!(WatchMode::Cli.unknown_grace(), None);
    }
}
