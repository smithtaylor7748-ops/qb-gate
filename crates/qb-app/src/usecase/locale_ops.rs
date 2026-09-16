//! 启动时把系统对齐到出口 IP 的归属地（0.19.0）：时区、区域格式、显示语言。
//!
//! # 为什么在编排层
//!
//! 要同时问两个领域：出口 IP 在哪（`probe`）、现在的系统是什么样（`sysenv`），
//! 再按面板自己的设置（`settings`）决定动不动手。三个都不该为了这件事去认识对方。
//!
//! # 判断和动手分开
//!
//! [`decide`] 是纯函数：事实（目标时区、当前时区、当前区域格式、当前语言覆盖）
//! 由调用方先取好，它只回答「该改哪几样」。这样几条最要紧的边界都钉得住单测，
//! 而单测不碰真实运行期状态。真正动手的 [`align_on_start`] 只负责取事实、执行、记日志。
//!
//! # ⛔ 已经一致就什么都不做
//!
//! 「默认开」不等于「每次启动都折腾一遍系统」。切时区要弹 UAC、改显示语言要注销 ——
//! 在已经一致的机器上做这些，等于每次开面板都骚扰使用者一次。
//! 所以 [`decide`] 的第一条就是：目标和现状相同 → 不进计划。
//!
//! # ⛔ 查不到就不猜
//!
//! 查不到出口 IP、或者对照表里没有那个时区，一律**跳过并记下原因**。
//! 猜一个地区设进去，比不设糟得多 —— 跟 `sysenv::iana_to_windows` 是同一条规矩。
//!
//! # 这不是「防封」
//!
//! DISCLAIMER 写死了「去除中文环境特征不会降低任何封禁概率」。对齐只是让
//! Claude Code 读到的时区与格式跟出口一致，日志和界面都不许写成安全措施。

use crate::error::Result;
use crate::settings::Settings;

/// 决策要用到的事实。**全部由调用方先取好**，这里不联网、不读注册表。
///
/// 每一项都是 `Option`，因为每一项都可能取不到 —— 而「取不到」必须能跟
/// 「取到了且已经一致」分开，否则会把查询失败当成对齐成功。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// 出口 IP 的 IANA 时区（`IpInfo::timezone`）。`None` = 没查到。
    pub target_zone: Option<String>,
    /// 当前系统时区的 Windows ID（`tzutil /g`）。
    pub current_tz: Option<String>,
    /// 当前区域格式（`Get-Culture`）。
    pub current_culture: Option<String>,
    /// 当前显示语言覆盖。外层 `None` = 没读到；内层 `None` = 读到了、但没设过覆盖。
    pub current_ui: Option<Option<String>>,
}

/// 该改哪几样。空计划 = 什么都不用做。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// 要切到的 Windows 时区 ID。
    pub set_timezone: Option<String>,
    /// 要设的区域格式（BCP-47）。
    pub set_culture: Option<String>,
    /// 要设的显示语言（BCP-47）。
    pub set_ui: Option<String>,
    /// 没做的那些，以及为什么。**跳过也要说得出理由**，否则界面上
    /// 「开着开关却什么都没发生」没人解释得了。
    pub notes: Vec<String>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.set_timezone.is_none() && self.set_culture.is_none() && self.set_ui.is_none()
    }
}

/// 纯判断：给定事实与设置，决定该改哪几样。**不碰系统。**
pub fn decide(f: &Facts, s: &Settings) -> Plan {
    let mut p = Plan::default();

    let want_anything =
        s.align_timezone_on_start || s.align_locale_on_start || s.align_display_language_on_start;
    if !want_anything {
        return p;
    }

    let Some(zone) = f.target_zone.as_deref().filter(|z| !z.is_empty()) else {
        p.notes
            .push("查不到出口 IP 的时区，这一轮什么都没改。".into());
        return p;
    };

    // ---- 时区
    if s.align_timezone_on_start {
        match crate::sysenv::iana_to_windows(zone) {
            Some(target) => match f.current_tz.as_deref() {
                Some(cur) if cur == target => {
                    p.notes.push(format!("系统时区已经是 {target}，跳过。"))
                }
                Some(_) => p.set_timezone = Some(target.to_string()),
                None => p.notes.push("读不到当前系统时区，没敢切。".into()),
            },
            // 对照表里没有 —— 不猜。
            None => p
                .notes
                .push(format!("对照表里没有 {zone} 对应的 Windows 时区，没切。")),
        }
    }

    // ---- 区域格式与显示语言共用同一个区域标签
    let tag = crate::sysenv::locale::locale_for_zone(zone);

    if s.align_locale_on_start {
        match tag {
            Some(tag) => match f.current_culture.as_deref() {
                Some(cur) if cur.eq_ignore_ascii_case(tag) => {
                    p.notes.push(format!("区域格式已经是 {tag}，跳过。"))
                }
                Some(_) => p.set_culture = Some(tag.to_string()),
                None => p.notes.push("读不到当前区域格式，没敢改。".into()),
            },
            None => p
                .notes
                .push(format!("对照表里没有 {zone} 对应的区域标签，没改。")),
        }
    }

    if s.align_display_language_on_start {
        match tag {
            Some(tag) => match f.current_ui.as_ref() {
                // 读到了，而且已经是这个 —— 不动。
                Some(Some(cur)) if cur.eq_ignore_ascii_case(tag) => {
                    p.notes.push(format!("显示语言覆盖已经是 {tag}，跳过。"))
                }
                Some(_) => p.set_ui = Some(tag.to_string()),
                None => p.notes.push("读不到当前显示语言覆盖，没敢改。".into()),
            },
            None => p.notes.push(format!(
                "对照表里没有 {zone} 对应的区域标签，没改显示语言。"
            )),
        }
    }

    p
}

/// 启动时跑一轮对齐。返回给日志看的人话，每行一件事。
///
/// **只在真的不一致时动手**，已经一致的机器上这个函数什么都不做。
pub async fn align_on_start() -> Result<Vec<String>> {
    let s = crate::settings::load();
    if !(s.align_timezone_on_start || s.align_locale_on_start || s.align_display_language_on_start)
    {
        return Ok(Vec::new());
    }

    let target_zone = crate::probe::ip::ip_info()
        .await
        .ok()
        .and_then(|i| i.timezone);
    let facts = Facts {
        target_zone,
        current_tz: crate::sysenv::current_windows_tz().ok(),
        current_culture: crate::sysenv::locale::current_culture().ok(),
        current_ui: crate::sysenv::locale::current_ui_override().ok(),
    };

    let plan = decide(&facts, &s);
    let mut log = plan.notes.clone();

    if let Some(tz) = &plan.set_timezone {
        // `apply_timezone` 收的是 IANA 名，自己再映射一次。
        let zone = facts.target_zone.clone().unwrap_or_default();
        match crate::sysenv::apply_timezone(&zone, false) {
            Ok(_) => log.push(format!("系统时区已切到 {tz}。")),
            // 切时区要管理员权限，拿不到就是拿不到 —— 如实说，别静默。
            Err(e) => log.push(format!("系统时区没切成：{e}")),
        }
    }
    if let Some(tag) = &plan.set_culture {
        match crate::sysenv::locale::apply_culture(tag) {
            Ok(()) => log.push(format!("区域格式已改为 {tag}（之后新开的程序生效）。")),
            Err(e) => log.push(format!("区域格式没改成：{e}")),
        }
    }
    if let Some(tag) = &plan.set_ui {
        match crate::sysenv::locale::apply_ui_language(tag) {
            // 界面是纯文本渲染的，星号会原样显示出来（CLAUDE.md 最后一节）。
            Ok(()) => log.push(format!("显示语言已设为 {tag}，要注销之后才生效。")),
            Err(e) => log.push(format!("显示语言没设成：{e}")),
        }
    }

    for line in &log {
        crate::audit::write(&format!("启动对齐：{line}"));
    }
    Ok(log)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_on() -> Settings {
        Settings {
            align_timezone_on_start: true,
            align_locale_on_start: true,
            align_display_language_on_start: true,
            ..Settings::default()
        }
    }

    /// ⛔ 最要紧的一条：**已经一致就什么都不做。**
    ///
    /// 「默认开」不等于每次开面板都切一次时区（弹 UAC）、改一次显示语言（要注销）。
    #[test]
    fn nothing_happens_when_everything_already_matches() {
        let f = Facts {
            target_zone: Some("America/New_York".into()),
            current_tz: Some("Eastern Standard Time".into()),
            current_culture: Some("en-US".into()),
            current_ui: Some(Some("en-US".into())),
        };
        let p = decide(&f, &all_on());
        assert!(p.is_empty(), "{p:?}");
        assert_eq!(p.notes.len(), 3, "三项都要说明为什么跳过：{p:?}");
    }

    /// 查不到出口 IP → 一样都不改，并且说得出原因。
    #[test]
    fn an_unknown_exit_ip_changes_nothing() {
        let f = Facts {
            current_tz: Some("China Standard Time".into()),
            current_culture: Some("zh-CN".into()),
            ..Facts::default()
        };
        let p = decide(&f, &all_on());
        assert!(p.is_empty());
        assert!(p.notes[0].contains("查不到"), "{p:?}");
    }

    /// 对照表里没有这个时区 → 不猜，跳过并说明。
    #[test]
    fn an_unmapped_zone_is_skipped_not_guessed() {
        let f = Facts {
            target_zone: Some("Mars/Olympus".into()),
            current_tz: Some("China Standard Time".into()),
            current_culture: Some("zh-CN".into()),
            current_ui: Some(None),
        };
        let p = decide(&f, &all_on());
        assert!(p.is_empty(), "{p:?}");
        assert!(p.notes.iter().any(|n| n.contains("对照表里没有")), "{p:?}");
    }

    /// 不一致就改。三样各按各的开关。
    #[test]
    fn a_mismatch_is_planned_per_switch() {
        let f = Facts {
            target_zone: Some("America/New_York".into()),
            current_tz: Some("China Standard Time".into()),
            current_culture: Some("zh-CN".into()),
            current_ui: Some(None),
        };
        let p = decide(&f, &all_on());
        assert_eq!(p.set_timezone.as_deref(), Some("Eastern Standard Time"));
        assert_eq!(p.set_culture.as_deref(), Some("en-US"));
        assert_eq!(p.set_ui.as_deref(), Some("en-US"));
    }

    /// 开关关掉的那一项不许动 —— 默认配置里显示语言就是关的。
    #[test]
    fn a_switch_that_is_off_is_never_touched() {
        let f = Facts {
            target_zone: Some("America/New_York".into()),
            current_tz: Some("China Standard Time".into()),
            current_culture: Some("zh-CN".into()),
            current_ui: Some(None),
        };
        let p = decide(&f, &Settings::default());
        assert!(p.set_timezone.is_some(), "时区默认开");
        assert!(p.set_culture.is_some(), "区域格式默认开");
        assert!(p.set_ui.is_none(), "显示语言默认关，不许改");
    }

    /// 三个开关全关 = 这个功能整个不启用，连出口 IP 都不用查。
    #[test]
    fn all_switches_off_means_no_plan_at_all() {
        let s = Settings {
            align_timezone_on_start: false,
            align_locale_on_start: false,
            align_display_language_on_start: false,
            ..Settings::default()
        };
        let f = Facts {
            target_zone: Some("America/New_York".into()),
            current_tz: Some("China Standard Time".into()),
            ..Facts::default()
        };
        let p = decide(&f, &s);
        assert!(p.is_empty());
        assert!(p.notes.is_empty(), "关掉的功能不该产生噪音：{p:?}");
    }

    /// 读不到当前值时**不动手**，而不是当成「不一致」硬改。
    #[test]
    fn unreadable_current_values_are_not_treated_as_mismatch() {
        let f = Facts {
            target_zone: Some("America/New_York".into()),
            current_tz: None,
            current_culture: None,
            current_ui: None,
        };
        let p = decide(&f, &all_on());
        assert!(p.is_empty(), "{p:?}");
        assert_eq!(p.notes.len(), 3);
    }
}
