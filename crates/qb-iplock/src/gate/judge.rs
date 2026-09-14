//! 门禁的**唯一**判定函数。
//!
//! 三个调用点走的是同一套结论：
//!
//! | 谁 | 多久一次 | 判不过怎么办 |
//! |---|---|---|
//! | 看门狗 | 15 / 20 秒 | 上锁，按 `watchdog::decide` 决定收不收进程 |
//! | 会话内 hook | 每次请求前 | 退出码 2，拦下这一次请求 |
//! | 面板手动检测 / 放行 | 使用者点 | 拒绝放行 |
//!
//! 各判各的迟早会对不上 —— 与硬约束 10「『Claude 装在哪』全项目只有一张表」
//! 同一个理由。加国家层的时候尤其危险：三处只要有一处漏了国家这一维，
//! 那一处就是现成的绕过入口。
//!
//! # 国家层为什么是「空 = 不启用」
//!
//! 不能让空名单等于全拒。使用者装完面板、还没来得及配国家白名单时，
//! 全拒会把他直接关在门外 —— 与硬约束 4「白名单为空时不许上锁」同一类事故。
//! 所以空名单 = 这一层不参与判定，界面上必须显著标注「国家层未启用」。

use crate::probe::ip::Reading;

/// 「只留美国」。
pub const PRESET_US: &[&str] = &["US"];

/// 「常用支持地区」。
///
/// ⚠ **这不是 Anthropic 的官方完整清单**，面板也不假装知道那份清单。
/// 它只是一个省得手打的起手式，使用者应当自己核对后增删。
///
/// 港澳（`HK` / `MO`）与中国大陆（`CN`）**刻意不在里面** —— 与中文环境识别
/// 那套计分口径一致：台湾是完整支持地区不计分，港澳属受限地区保留风险分。
pub const PRESET_COMMON: &[&str] = &[
    "US", "CA", "GB", "IE", "DE", "FR", "NL", "SE", "CH", "ES", "IT", "PL", "AU", "NZ", "JP", "KR",
    "SG", "TW",
];

/// 一轮判定的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Judgement {
    /// IP 在白名单里，国家也过了（或国家层没启用）。
    Allowed { ip: String },
    /// 三个源都没答上来 —— 这跟「答上来了但不合格」是两回事，别合并。
    IpUnknown,
    /// 查到了 IP，不在白名单里。
    IpNotAllowed { ip: String },
    /// IP 在白名单里，但它落在国家白名单之外。
    CountryNotAllowed { ip: String, country: String },
    /// 国家层启用了，但没有任何一个源答得出国家。
    CountryUnknown { ip: String },
    /// 多个源给出不同的国家。按最严的算，原文留在 `detail` 里。
    CountryConflict { ip: String, detail: String },
}

impl Judgement {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Judgement::Allowed { .. })
    }

    /// 给日志、hook 的 stderr、界面共用的一句人话。
    ///
    /// **「查不到」和「不合格」必须在措辞上就分得开** —— 合并成一句
    /// 「门禁不通过」会让使用者把网络抖动和真的换了出口当成同一件事，
    /// 而这两件事的处理方式完全相反。
    pub fn reason(&self) -> String {
        match self {
            Judgement::Allowed { ip } => format!("出口 IP {ip} 在白名单内"),
            Judgement::IpUnknown => "查不到出口 IP（三个探测源都没答上来）".into(),
            Judgement::IpNotAllowed { ip } => format!("出口 IP {ip} 不在白名单内"),
            Judgement::CountryNotAllowed { ip, country } => {
                format!("出口 IP {ip} 落在 {country}，不在国家白名单内")
            }
            Judgement::CountryUnknown { ip } => {
                format!("出口 IP {ip} 查得到，但没有任何探测源答得出国家")
            }
            Judgement::CountryConflict { ip, detail } => {
                format!("出口 IP {ip} 的国家有冲突（{detail}），按不合格处理")
            }
        }
    }
}

/// 国家这一维单独的结论。`judge` 与 `may_add` 共用。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Country {
    /// 国家白名单为空 —— 这一层不启用。
    NotEnforced,
    Ok(String),
    Unknown,
    Conflict(String),
    NotAllowed(String),
}

fn country_check(r: &Reading, country_allow: &[String]) -> Country {
    if country_allow.is_empty() {
        return Country::NotEnforced;
    }
    let seen = r.distinct_countries();
    match seen.len() {
        0 => Country::Unknown,
        1 => {
            let c = seen.into_iter().next().unwrap();
            if country_allow.iter().any(|a| a.eq_ignore_ascii_case(&c)) {
                Country::Ok(c)
            } else {
                Country::NotAllowed(c)
            }
        }
        // 两个源说了两个国家。哪个对不知道，但「不知道」在 fail-closed 下
        // 就是不合格。原文一并带走，否则这种误杀事后查不出来。
        _ => Country::Conflict(r.detail()),
    }
}

/// 门禁判定。看门狗、hook、手动放行都从这里拿结论。
pub fn judge(r: &Reading, allow: &[String], country_allow: &[String]) -> Judgement {
    let Some(ip) = r.ip.clone() else {
        return Judgement::IpUnknown;
    };
    if !allow.iter().any(|a| a == &ip) {
        return Judgement::IpNotAllowed { ip };
    }
    match country_check(r, country_allow) {
        Country::NotEnforced | Country::Ok(_) => Judgement::Allowed { ip },
        Country::Unknown => Judgement::CountryUnknown { ip },
        Country::Conflict(detail) => Judgement::CountryConflict { ip, detail },
        Country::NotAllowed(country) => Judgement::CountryNotAllowed { ip, country },
    }
}

/// 「当前这个出口 IP 能不能加进白名单」。
///
/// 只验国家 —— IP 当然还不在名单里，那正是要加它的原因。
/// 这一关堵的是「先把脏 IP 塞进白名单，再回头抱怨门禁没用」：
/// 白名单是门禁唯一的事实来源，让不合格的国家进得来，下游全部白做。
pub fn may_add(r: &Reading, country_allow: &[String]) -> std::result::Result<String, Judgement> {
    let Some(ip) = r.ip.clone() else {
        return Err(Judgement::IpUnknown);
    };
    match country_check(r, country_allow) {
        Country::NotEnforced | Country::Ok(_) => Ok(ip),
        Country::Unknown => Err(Judgement::CountryUnknown { ip }),
        Country::Conflict(detail) => Err(Judgement::CountryConflict { ip, detail }),
        Country::NotAllowed(country) => Err(Judgement::CountryNotAllowed { ip, country }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IP: &str = "203.0.113.7";

    fn reading(countries: &[(&str, &str)]) -> Reading {
        Reading {
            ip: Some(IP.into()),
            countries: countries
                .iter()
                .map(|(s, c)| (s.to_string(), c.to_string()))
                .collect(),
        }
    }

    fn allow() -> Vec<String> {
        vec![IP.to_string()]
    }

    fn us() -> Vec<String> {
        vec!["US".to_string()]
    }

    #[test]
    fn ip_allowed_and_country_allowed_passes() {
        let j = judge(
            &reading(&[("ippure", "US"), ("cloudflare", "US")]),
            &allow(),
            &us(),
        );
        assert_eq!(j, Judgement::Allowed { ip: IP.into() });
        assert!(j.is_allowed());
    }

    /// 国家白名单为空 = 这一层不启用，**不是全拒**。
    ///
    /// 搞反了的话，使用者装完面板还没配国家名单就被自己的工具关在门外 ——
    /// 与硬约束 4「白名单为空时不许上锁」同一类事故。
    #[test]
    fn empty_country_list_means_layer_off_not_deny_all() {
        let j = judge(&reading(&[("ippure", "HK")]), &allow(), &[]);
        assert_eq!(j, Judgement::Allowed { ip: IP.into() });
    }

    #[test]
    fn country_outside_the_list_is_rejected() {
        let j = judge(&reading(&[("ippure", "HK")]), &allow(), &us());
        assert_eq!(
            j,
            Judgement::CountryNotAllowed {
                ip: IP.into(),
                country: "HK".into()
            }
        );
    }

    /// 国家层开着、但没人答得出国家 —— 独立一种，不能混进 `CountryNotAllowed`。
    /// 看门狗对这两者的处理不一样：前者是服务挂了（留宽限），后者是真的不合格（立刻收）。
    #[test]
    fn country_unknown_is_its_own_verdict() {
        let j = judge(&reading(&[]), &allow(), &us());
        assert_eq!(j, Judgement::CountryUnknown { ip: IP.into() });
    }

    #[test]
    fn conflicting_countries_fail_closed_and_keep_the_evidence() {
        let j = judge(
            &reading(&[("ippure", "US"), ("cloudflare", "HK")]),
            &allow(),
            &us(),
        );
        match j {
            Judgement::CountryConflict { detail, .. } => {
                assert!(detail.contains("ippure=US"), "冲突原文必须留住：{detail}");
                assert!(detail.contains("cloudflare=HK"));
            }
            other => panic!("冲突必须判不合格，实际是 {other:?}"),
        }
    }

    /// 双栈机器上两个源都报 US（一个走 v6 一个走 v4）不算冲突。
    #[test]
    fn same_country_from_several_sources_is_not_a_conflict() {
        let j = judge(
            &reading(&[("ippure", "US"), ("cloudflare", "us"), ("ipinfo", "US")]),
            &allow(),
            &us(),
        );
        assert!(j.is_allowed());
    }

    #[test]
    fn ip_not_in_allowlist_beats_country_check() {
        let j = judge(&reading(&[("ippure", "US")]), &[], &us());
        assert_eq!(j, Judgement::IpNotAllowed { ip: IP.into() });
    }

    #[test]
    fn no_ip_at_all_is_ip_unknown() {
        let j = judge(&Reading::default(), &allow(), &us());
        assert_eq!(j, Judgement::IpUnknown);
    }

    // ---------------------------------------------------------- may_add

    #[test]
    fn may_add_accepts_when_country_passes() {
        assert_eq!(
            may_add(&reading(&[("ippure", "US")]), &us()),
            Ok(IP.to_string())
        );
    }

    /// 核心不变量：国家不合格的 IP **一个字都不许进白名单**。
    #[test]
    fn may_add_rejects_a_country_outside_the_list() {
        let e = may_add(&reading(&[("ippure", "HK")]), &us()).unwrap_err();
        assert_eq!(
            e,
            Judgement::CountryNotAllowed {
                ip: IP.into(),
                country: "HK".into()
            }
        );
    }

    #[test]
    fn may_add_rejects_when_country_is_unknown_or_conflicting() {
        assert!(matches!(
            may_add(&reading(&[]), &us()),
            Err(Judgement::CountryUnknown { .. })
        ));
        assert!(matches!(
            may_add(&reading(&[("ippure", "US"), ("ipinfo", "JP")]), &us()),
            Err(Judgement::CountryConflict { .. })
        ));
    }

    /// 国家层没启用时，加白名单照旧 —— 不能因为加了这一层就把老行为改掉。
    #[test]
    fn may_add_is_unchanged_when_the_layer_is_off() {
        assert_eq!(
            may_add(&reading(&[("ippure", "HK")]), &[]),
            Ok(IP.to_string())
        );
    }

    #[test]
    fn presets_exclude_restricted_regions() {
        assert!(PRESET_COMMON.contains(&"TW"), "台湾是完整支持地区");
        for r in ["HK", "MO", "CN"] {
            assert!(
                !PRESET_COMMON.contains(&r),
                "{r} 属受限地区，不该进默认预设"
            );
        }
        assert_eq!(PRESET_US, &["US"]);
    }
}
