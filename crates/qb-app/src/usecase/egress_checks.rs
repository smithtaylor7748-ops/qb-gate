//! 出口一致性：**你现在看到的出口 IP，和实际会走出去的那条路，是不是同一个。**
//!
//! 跨域编排，所以住在这一层 —— 它要同时够到 `qb-sysenv`（系统体检那几项）
//! 和 `qb-install`（Chrome 的策略与资料）。
//!
//! # 这一项只问一个问题
//!
//! 不是「这台机器的浏览器隐私面怎么样」（那是软件页那张 Chrome 卡片），
//! 也不是「这台机器像不像中文环境」（那是 `signals.ts` 的十项）。
//! 这一项只收**与出口 IP 对不上**的信号：
//!
//! | 行 | 它怎么跟出口打架 |
//! |---|---|
//! | 出口一致性 | 绕过代理和跟随代理量到的是两个国家 —— 面板显示的出口不是请求走的那条 |
//! | Anthropic 服务可达 | 不带凭据问 API：403 = Anthropic 不接受这个出口（2026-09-24，参照 CheckClaude） |
//! | claude.ai 的解析 | 解析到内网 / 回环 = 被污染，请求根本到不了真的 claude.ai（同上） |
//! | IPv6 | 实测 v6 出口：在另一个国家的话，走 v6 的请求露的是另一个地址（同上） |
//! | 浏览器语言 | 网站拿到的 `Accept-Language` 和 IP 地区对不上 |
//! | WebRTC | 可以绕开代理直接用 UDP 打出去，把真实地址捅给网页 |
//! | 浏览器 DoH | DNS 走系统解析器，明文出去，解析请求的出口跟网页的出口不是一条 |
//!
//! 浏览器里的 claude.ai 痕迹、扩展权限、凭据管理器、环境变量残留**都不在这里** ——
//! 它们跟出口 IP 没关系。前两样在软件页的 Chrome 卡片，后两样在环境体检里。
//!
//! # ⛔ 只检测、只如实报告，不替使用者伪装
//!
//! 「浏览器语言和出口国家对不上」这一项**只报告**。改浏览器或系统的身份去
//! 「对上」出口，是 CLAUDE.md「不许加的功能」第一行点名的设备指纹伪装。
//! 检测与如实报告不在此列 —— 区别是前者让使用者知情，后者替他伪装。
//!
//! # 能修的那两项，修法复用已有的那一处
//!
//! WebRTC / DoH 走 `browser_audit` 自己的写入口（只写 HKCU、写完读回、
//! 文案里带「去 chrome://policy 核实」）。**不在这里另写一套** ——
//! 两处各写一份注册表，改一边漏一边是迟早的事。
//!
//! 所有修都只在使用者当次点击之后跑：没有定时器、没有启动时触发。

use serde::Serialize;
use ts_rs::TS;

use crate::error::Result;
use crate::install::{browser_audit, chrome};
use crate::sysenv::checkup::{self, CheckItem, State};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct EgressChecks {
    pub items: Vec<CheckItem>,
    /// 这一轮跑的时刻，本地 `YYYY-MM-DD HH:MM`。
    pub checked_at: String,
    pub undoable: Vec<String>,
}

impl EgressChecks {
    /// 这一轮拿到了几分之几。
    ///
    /// **查不出来的那几项从分子分母里一起去掉** —— 跟 `score.ts` 顶上那条
    /// 「分母只算已检测项」同一个道理：没查成的算 0 分是在冤枉使用者，
    /// 算满分是在替一个没做过的检查打包票。全都查不出来就回 `None`。
    ///
    /// `Warn` 给半分：它是「对不上，但不一定漏」，跟 `Fail`（确实是两个国家）
    /// 不该同价。
    pub fn ratio(&self) -> Option<f32> {
        let counted: Vec<&CheckItem> = self
            .items
            .iter()
            .filter(|i| i.state != State::Unknown)
            .collect();
        if counted.is_empty() {
            return None;
        }
        let got: f32 = counted
            .iter()
            .map(|i| match i.state {
                State::Pass => 1.0,
                State::Warn => 0.5,
                _ => 0.0,
            })
            .sum();
        Some(got / counted.len() as f32)
    }
}

fn item(id: &str, label: &str, state: State, detail: impl Into<String>) -> CheckItem {
    CheckItem {
        id: id.into(),
        label: label.into(),
        state,
        detail: detail.into(),
        fixable: false,
        manual: None,
    }
}

fn fixable(mut it: CheckItem) -> CheckItem {
    it.fixable = true;
    it
}

// ------------------------------------------------------------------ 各项

/// Chrome 自己的语言设置与出口国家对不对得上。
///
/// 读的是 Chrome `Preferences` 里的 `intl.accept_languages` ——
/// **不是面板那个 WebView2 的** `navigator.languages`（`signals.ts` 量的是后者）。
/// 网站看到的 `Accept-Language` 来自浏览器，跟面板没关系，
/// 所以这一项跟「中文环境」那十项不重复。
/// `langs` 由调用方读好传进来（`chrome_languages`）—— **判定不做 I/O**，
/// 否则这一项的单测就得看跑测试那台机器上 Chrome 装没装、设了什么语言。
fn browser_locale(langs: Option<String>, country: Option<&str>) -> CheckItem {
    locale_item(langs, country, "Chrome 报的语言")
}

/// 判定本体。`source` 说的是这串语言是从哪来的（Chrome 的设置，还是默认浏览器实测发出的请求头）。
fn locale_item(langs: Option<String>, country: Option<&str>, source: &str) -> CheckItem {
    let Some(langs) = langs else {
        return item(
            "browser_locale",
            "浏览器语言",
            State::Unknown,
            "读不出 Chrome 的语言设置（没装 Chrome，或者读不开 Preferences）。",
        );
    };
    let Some(cc) = country else {
        return item(
            "browser_locale",
            "浏览器语言",
            State::Unknown,
            format!("{source}是 {langs}。还没测出口 IP，对不上号 —— 先在总览点一次「重新检测」。"),
        );
    };
    // 只判一件事：中文语言 + 非中文地区出口。别的组合不下判断 ——
    // 说不清楚的信号不如不说。
    let chinese = langs.to_ascii_lowercase().contains("zh");
    let cn_exit = matches!(cc, "CN" | "HK" | "MO" | "TW");
    if chinese && !cn_exit {
        item(
            "browser_locale",
            "浏览器语言",
            State::Warn,
            format!(
                "{source}是 {langs}，而出口在 {cc}。网站拿到的 Accept-Language 和 IP 地区对不上。\
                 面板只把这件事告诉你，不替你改浏览器或系统的身份。",
            ),
        )
    } else {
        item(
            "browser_locale",
            "浏览器语言",
            State::Pass,
            format!("{source}是 {langs}，出口在 {cc}，对得上。"),
        )
    }
}

/// `zh-CN,zh;q=0.9,en;q=0.8` → `zh-CN、zh、en`。
pub fn accept_languages(header: &str) -> String {
    header
        .split(',')
        .filter_map(|p| p.split(';').next().map(str::trim))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("、")
}

/// 从 UA 认个浏览器名，只为说清楚「测的是哪一个」。认不出就说「默认浏览器」。
pub fn browser_name(ua: Option<&str>) -> String {
    let ua = ua.unwrap_or("");
    let ver = |key: &str| {
        ua.split(key)
            .nth(1)
            .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
            .filter(|v| !v.is_empty())
            .map(|v| format!(" {v}"))
            .unwrap_or_default()
    };
    if ua.contains("Edg/") {
        format!("Edge{}", ver("Edg/"))
    } else if ua.contains("Firefox/") {
        format!("Firefox{}", ver("Firefox/"))
    } else if ua.contains("OPR/") {
        format!("Opera{}", ver("OPR/"))
    } else if ua.contains("Chrome/") {
        format!("Chrome{}", ver("Chrome/"))
    } else {
        "默认浏览器".into()
    }
}

fn is_public(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(a) => {
            let o = a.octets();
            !(a.is_private()
                || a.is_loopback()
                || a.is_link_local()
                || a.is_unspecified()
                || (o[0] == 100 && (64..128).contains(&o[1])))
        }
        std::net::IpAddr::V6(a) => {
            let s = a.segments()[0];
            !(a.is_loopback()
                || a.is_unspecified()
                || (s & 0xfe00) == 0xfc00
                || (s & 0xffc0) == 0xfe80)
        }
    }
}

/// 默认浏览器**实测**的 WebRTC 候选地址对出口（2026-09-24，参照 CheckClaude 的「WebRTC 出口」）。
///
/// 只看公网地址：局域网地址与 mDNS 名字不出本机。公网地址就是出口 = 同一条路，通过；
/// 有**不是出口**的公网地址 = UDP 绕过了代理，网页看得到那个地址（关键项）。
pub fn webrtc_measured(ips: &[String], exits: &[String], browser: &str) -> CheckItem {
    let public: Vec<&String> = ips
        .iter()
        .filter(|ip| ip.parse().is_ok_and(is_public))
        .collect();
    if public.is_empty() {
        return item(
            "browser_webrtc",
            "WebRTC 出口",
            State::Pass,
            format!("{browser}实测：WebRTC 没有暴露公网地址（只有局域网 / mDNS 候选，或者一个都没有）。"),
        );
    }
    if exits.is_empty() {
        return item(
            "browser_webrtc",
            "WebRTC 出口",
            State::Unknown,
            format!(
                "{browser}实测到 WebRTC 公网地址 {}，但这一轮没测出出口 IP，比不了。",
                public
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
            ),
        );
    }
    let others: Vec<&str> = public
        .iter()
        .filter(|ip| !exits.iter().any(|e| e == **ip))
        .map(|s| s.as_str())
        .collect();
    if others.is_empty() {
        item(
            "browser_webrtc",
            "WebRTC 出口",
            State::Pass,
            format!(
                "{browser}实测：WebRTC 暴露的就是出口（{}），走的是同一条路。",
                exits.join("、")
            ),
        )
    } else {
        fixable(item(
            "browser_webrtc",
            "WebRTC 出口",
            State::Fail,
            format!(
                "{browser}实测：WebRTC 暴露了 {}，不是出口 {} —— UDP 绕过了代理，网页看得到这个地址。\
                 代理开 TUN 全局接管 UDP，或者把 Chrome 的 WebRtcIPHandling 收紧到 disable_non_proxied_udp。",
                others.join("、"),
                exits.join("、")
            ),
        ))
    }
}

/// 从真实浏览器的报告里取出页面交回的 WebRTC 地址。页面交回的是不可信数据：只收字符串、最多 16 个。
fn report_webrtc(page_json: &str) -> Option<Vec<String>> {
    let v: serde_json::Value = serde_json::from_str(page_json).ok()?;
    let arr = v.get("webrtc")?.as_array()?;
    Some(
        arr.iter()
            .filter_map(|x| x.as_str())
            .take(16)
            .map(|s| s.chars().take(64).collect())
            .collect(),
    )
}

/// 读 Chrome 各个 Profile 的 `intl.accept_languages`，去重后拼成一串。
fn chrome_languages() -> Option<String> {
    let mut out: Vec<String> = Vec::new();
    for profile in chrome::chrome_profiles() {
        let Ok(text) = std::fs::read_to_string(profile.join("Preferences")) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if let Some(l) = v
            .get("intl")
            .and_then(|i| i.get("accept_languages"))
            .and_then(|x| x.as_str())
        {
            for one in l.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                if !out.iter().any(|x| x == one) {
                    out.push(one.to_string());
                }
            }
        }
    }
    (!out.is_empty()).then(|| out.join("、"))
}

fn webrtc(a: &browser_audit::Audit) -> CheckItem {
    if !a.chrome_installed {
        return item(
            "browser_webrtc",
            "WebRTC 出口",
            State::Unknown,
            "没找到 Chrome。这一项只看 Chrome，Edge 与 Firefox 不在范围里。",
        );
    }
    if a.webrtc.as_ref().is_some_and(|p| {
        p.scope == browser_audit::Scope::Machine && p.value != browser_audit::WEBRTC_HARDENED
    }) {
        return item(
            "browser_webrtc",
            "WebRTC 出口",
            State::Warn,
            "整机策略允许其他 WebRTC 路径，当前用户设置无法覆盖；请在 chrome://policy 核对来源。",
        );
    }
    match &a.webrtc {
        Some(p) if p.value == browser_audit::WEBRTC_HARDENED => item(
            "browser_webrtc",
            "WebRTC 出口",
            State::Pass,
            format!(
                "已设为 {}（{}），不许走非代理的 UDP。",
                p.value,
                scope_word(p.scope)
            ),
        ),
        Some(p) => fixable(item(
            "browser_webrtc",
            "WebRTC 出口",
            State::Warn,
            format!(
                "现在是 {}（{}）。WebRTC 可以绕开代理直接用 UDP 打出去，\
                 把真实公网地址和局域网地址暴露给网页 —— 那跟你看到的出口不是一个。",
                p.value,
                scope_word(p.scope)
            ),
        )),
        // ⛔ 没设过策略 ≠ 安全，那是 Chrome 自己的默认值。
        None => fixable(item(
            "browser_webrtc",
            "WebRTC 出口",
            State::Warn,
            "没设过 WebRtcIPHandling 策略 —— 用的是 Chrome 自己的默认值，不等于安全。\
             收紧到 disable_non_proxied_udp 会保留通话功能，但不许走非代理的 UDP。",
        )),
    }
}

fn doh(a: &browser_audit::Audit) -> CheckItem {
    if !a.chrome_installed {
        return item(
            "browser_doh",
            "浏览器 DoH",
            State::Unknown,
            "没找到 Chrome。这一项只看 Chrome。",
        );
    }
    if a.doh
        .as_ref()
        .is_some_and(|p| p.scope == browser_audit::Scope::Machine && p.value == "off")
    {
        return item(
            "browser_doh",
            "浏览器 DoH",
            State::Warn,
            "整机策略关闭了 DoH，当前用户设置无法覆盖；请联系策略管理员。",
        );
    }
    match &a.doh {
        Some(p) if p.value == "off" => fixable(item(
            "browser_doh",
            "浏览器 DoH",
            State::Warn,
            format!(
                "被显式关掉了（{}）。DNS 查询走系统解析器、明文出去 —— \
                 解析请求走的那条路和网页走的那条可能不是一条。",
                scope_word(p.scope)
            ),
        )),
        Some(p) => item(
            "browser_doh",
            "浏览器 DoH",
            State::Pass,
            format!(
                "DnsOverHttpsMode = {}（{}）。automatic 允许退回系统 DNS；策略存在不代表实际 DNS 流量已加密，需结合真实解析回显核对。",
                p.value,
                scope_word(p.scope)
            ),
        ),
        None => fixable(item(
            "browser_doh",
            "浏览器 DoH",
            State::Unknown,
            "没设过 DnsOverHttpsMode 策略，走的是 Chrome 自己的默认值 —— \
             那个值随版本和地区变，面板读不到，所以这一项算「查不了」。\
             可以设成 automatic：有 DoH 就走 DoH，没有就退回系统解析。",
        )),
    }
}

fn scope_word(s: browser_audit::Scope) -> &'static str {
    match s {
        browser_audit::Scope::CurrentUser => "当前用户",
        browser_audit::Scope::Machine => "整机策略，不是面板设的",
    }
}

// ------------------------------------------------------------------ 入口

/// 跑一轮出口一致性检查。
///
/// `country` 是当前出口国家的两字母码，由界面从 `R.ip` 传进来 ——
/// **这里不自己发探测**：出口 IP 全项目只探一处，那条规矩写在
/// `src/lib/resources.ts` 的开头。拿不到就如实报 `Unknown`。
///
/// 出口一致性与 IPv6 两项直接取 `checkup::scan()` 的结果，**不另写一遍**：
/// 那两项的判定（尤其是「绕过代理 vs 跟随代理」要真发两轮请求）只该有一处。
pub async fn scan(country: Option<String>) -> EgressChecks {
    // 浏览器那几项是同步的本地读取（会 fork `reg.exe`、读 Chrome 的
    // Preferences），丢进阻塞线程池，跟 checkup 的两轮网络探测真正并发。
    let local = tokio::task::spawn_blocking(browser_audit::audit);
    let checkup = checkup::scan().await;

    let checked_at = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let take = |id: &str| {
        checkup
            .items
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .unwrap_or_else(|| {
                item(
                    id,
                    id,
                    State::Unknown,
                    "这一项这次没能查（本地检查任务异常结束）。",
                )
            })
    };

    // 阻塞任务挂了也要给出一份能看的报告 —— 整项白屏比少几行糟得多。
    let audit = local.await.unwrap_or_default();

    // 一小时内用默认浏览器实测过（`browser_probe`）：浏览器语言与 WebRTC 两行改用实测 ——
    // 请求头里真正发出去的 Accept-Language、真正收到的 WebRTC 候选地址，任何浏览器都算，
    // 不再只看 Chrome 的设置与策略。没测过就退回原来的读法。
    let report = crate::browser_probe::fresh_report();
    let browser = report
        .as_ref()
        .map(|r| format!("默认浏览器（{}）", browser_name(r.user_agent.as_deref())));
    let locale = match (&report, &browser) {
        (Some(r), Some(b)) if r.accept_language.is_some() => locale_item(
            r.accept_language.as_deref().map(accept_languages),
            country.as_deref(),
            &format!("{b}实测发出的 Accept-Language "),
        ),
        _ => browser_locale(chrome_languages(), country.as_deref()),
    };
    let webrtc_row = match (&report, &browser) {
        (Some(r), Some(b)) => match report_webrtc(&r.page_json) {
            Some(ips) => webrtc_measured(&ips, &checkup.exit_ips, b),
            None => webrtc(&audit),
        },
        _ => webrtc(&audit),
    };

    EgressChecks {
        items: vec![
            take("egress_consistency"),
            take("anthropic_reach"),
            take("claude_dns"),
            take("ipv6"),
            locale,
            webrtc_row,
            doh(&audit),
        ],
        checked_at,
        undoable: [
            ("browser_webrtc", "WebRtcIPHandling"),
            ("browser_doh", "DnsOverHttpsMode"),
        ]
        .into_iter()
        .filter(|(_, name)| browser_audit::policy_undoable(name))
        .map(|(id, _)| id.into())
        .collect(),
    }
}

// ------------------------------------------------------------------ 修

/// 修某一项。**只由使用者当次点击触发。**
pub fn fix(id: &str) -> Result<String> {
    match id {
        "browser_webrtc" => browser_audit::set_webrtc_policy(),
        "browser_doh" => browser_audit::set_doh_policy(),
        _ => Err(crate::error::GateError::Other(format!(
            "「{id}」这一项面板不代劳，旁边写了怎么自己动手以及代价。"
        ))),
    }
}

/// 撤销面板刚才那一下。只有写过东西的那两项有。
pub fn undo(id: &str) -> Result<String> {
    match id {
        "browser_webrtc" => browser_audit::clear_webrtc_policy(),
        "browser_doh" => browser_audit::clear_doh_policy(),
        _ => Err(crate::error::GateError::Other(format!(
            "「{id}」没有撤销入口。"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(id: &str, state: State) -> CheckItem {
        item(id, id, state, "")
    }

    fn checks(items: Vec<CheckItem>) -> EgressChecks {
        EgressChecks {
            items,
            checked_at: String::new(),
            undoable: Vec::new(),
        }
    }

    /// 真实浏览器实测的 WebRTC：候选地址就是出口不算泄露；有别的公网地址才算（关键项）。
    #[test]
    fn measured_webrtc_is_judged_against_the_exit() {
        let exits = vec!["203.0.113.7".to_string()];
        let b = "默认浏览器（Chrome 128）";
        assert_eq!(webrtc_measured(&[], &exits, b).state, State::Pass);
        assert_eq!(
            webrtc_measured(&["192.168.1.9".into(), "10.0.0.2".into()], &exits, b).state,
            State::Pass,
            "局域网地址不出本机"
        );
        assert_eq!(
            webrtc_measured(&["203.0.113.7".into()], &exits, b).state,
            State::Pass
        );
        let leak = webrtc_measured(&["198.51.100.4".into()], &exits, b);
        assert_eq!(leak.state, State::Fail);
        assert!(leak.detail.contains("198.51.100.4"));
        assert_eq!(
            webrtc_measured(&["198.51.100.4".into()], &[], b).state,
            State::Unknown
        );
    }

    #[test]
    fn accept_language_headers_and_browser_names() {
        assert_eq!(accept_languages("zh-CN,zh;q=0.9,en;q=0.8"), "zh-CN、zh、en");
        assert_eq!(accept_languages("en-US"), "en-US");
        let chrome = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
        assert_eq!(browser_name(Some(chrome)), "Chrome 128");
        assert_eq!(
            browser_name(Some(&format!("{chrome} Edg/128.0.1"))),
            "Edge 128"
        );
        assert_eq!(
            browser_name(Some(
                "Mozilla/5.0 (Windows NT 10.0; rv:130.0) Gecko/20100101 Firefox/130.0"
            )),
            "Firefox 130"
        );
        assert_eq!(browser_name(None), "默认浏览器");
        // 实测发出的中文 Accept-Language + 美国出口：警告，而且写明是哪个浏览器发的。
        let it = locale_item(
            Some(accept_languages("zh-CN,zh;q=0.9")),
            Some("US"),
            "默认浏览器（Chrome 128）实测发出的 Accept-Language ",
        );
        assert_eq!(it.state, State::Warn);
        assert!(it.detail.starts_with("默认浏览器（Chrome 128）"));
    }

    #[test]
    fn the_report_webrtc_list_is_read_defensively() {
        assert_eq!(
            report_webrtc(r#"{"webrtc":["203.0.113.7",5,"x"]}"#),
            Some(vec!["203.0.113.7".to_string(), "x".to_string()])
        );
        assert_eq!(report_webrtc(r#"{"v":1}"#), None);
        assert_eq!(report_webrtc("not json"), None);
    }

    /// ⛔ 查不出来的项从分子分母里一起去掉 —— 跟 `score.ts` 顶上那条同理。
    /// 算 0 分是在冤枉使用者，算满分是在替一个没做过的检查打包票。
    #[test]
    fn unknown_rows_leave_the_ratio_alone() {
        let all_pass = checks(vec![at("a", State::Pass), at("b", State::Pass)]);
        assert_eq!(all_pass.ratio(), Some(1.0));

        let with_unknown = checks(vec![
            at("a", State::Pass),
            at("b", State::Pass),
            at("c", State::Unknown),
        ]);
        assert_eq!(
            with_unknown.ratio(),
            Some(1.0),
            "多一个查不出来的，不该把满分拉下来"
        );
    }

    #[test]
    fn nothing_checkable_has_no_ratio_at_all() {
        assert_eq!(
            checks(vec![at("a", State::Unknown), at("b", State::Unknown)]).ratio(),
            None,
            "一项都查不成时不许给分数"
        );
        assert_eq!(checks(vec![]).ratio(), None);
    }

    /// Warn 是「对不上，但不一定漏」，跟 Fail（确实是两个国家）不该同价。
    #[test]
    fn warn_is_worth_half_and_fail_is_worth_nothing() {
        assert_eq!(checks(vec![at("a", State::Warn)]).ratio(), Some(0.5));
        assert_eq!(checks(vec![at("a", State::Fail)]).ratio(), Some(0.0));
        assert_eq!(
            checks(vec![at("a", State::Pass), at("b", State::Fail)]).ratio(),
            Some(0.5)
        );
    }

    /// 「没设过策略」不是「安全」—— 那是 Chrome 自己的默认值。
    #[test]
    fn an_unset_webrtc_policy_is_not_a_pass() {
        let a = browser_audit::Audit {
            chrome_installed: true,
            ..Default::default()
        };
        let it = webrtc(&a);
        assert_eq!(it.state, State::Warn);
        assert!(it.fixable);
    }

    /// 没装 Chrome 是「查不了」，不是「通过」。
    #[test]
    fn no_chrome_is_unknown_not_pass() {
        let a = browser_audit::Audit::default();
        assert_eq!(webrtc(&a).state, State::Unknown);
        assert_eq!(doh(&a).state, State::Unknown);
    }

    /// 两条路都不许报 Pass：读不出浏览器语言是「查不了」，
    /// 还没测出口 IP 也是「查不了」。
    #[test]
    fn locale_is_unknown_until_both_sides_are_known() {
        assert_eq!(
            browser_locale(None, Some("US")).state,
            State::Unknown,
            "读不出 Chrome 的语言"
        );
        assert_eq!(
            browser_locale(Some("zh-CN".into()), None).state,
            State::Unknown,
            "还没测出口 IP"
        );
    }

    /// 中文语言 + 非中文地区出口 —— 这一对就是「与 IP 不符」的典型。
    #[test]
    fn chinese_browser_on_a_foreign_exit_is_a_mismatch() {
        let it = browser_locale(Some("zh-CN、zh、en-US".into()), Some("US"));
        assert_eq!(it.state, State::Warn);
        assert!(
            it.detail.contains("US"),
            "报出来的话里要带上出口：{}",
            it.detail
        );
        // ⛔ 只报告：改浏览器或系统的身份去「对上」出口是明确不做的事。
        assert!(!it.fixable, "这一项不许给「修」");
    }

    /// 出口就在中文地区，或者浏览器本来就不是中文 —— 都不算矛盾。
    #[test]
    fn a_matching_pair_is_a_pass() {
        assert_eq!(
            browser_locale(Some("zh-CN".into()), Some("HK")).state,
            State::Pass,
            "出口在中文地区"
        );
        assert_eq!(
            browser_locale(Some("en-US".into()), Some("US")).state,
            State::Pass,
            "浏览器不是中文"
        );
    }

    /// 被显式关掉的 DoH 是「对不上」且可修；设过别的值就是通过。
    #[test]
    fn doh_turned_off_is_a_mismatch_that_can_be_fixed() {
        let off = browser_audit::Audit {
            chrome_installed: true,
            doh: Some(browser_audit::Policy {
                value: "off".into(),
                scope: browser_audit::Scope::CurrentUser,
            }),
            ..Default::default()
        };
        let it = doh(&off);
        assert_eq!(it.state, State::Warn);
        assert!(it.fixable);

        let on = browser_audit::Audit {
            chrome_installed: true,
            doh: Some(browser_audit::Policy {
                value: "automatic".into(),
                scope: browser_audit::Scope::Machine,
            }),
            ..Default::default()
        };
        assert_eq!(doh(&on).state, State::Pass);
    }

    #[test]
    fn unknown_ids_do_not_silently_succeed() {
        assert!(fix("egress_consistency").is_err(), "面板不代劳出口一致性");
        assert!(undo("ipv6").is_err());
    }

    /// 传给界面的是纯文本，写 Markdown 的星号会显示成两个星号。
    #[test]
    fn nothing_we_hand_the_ui_carries_markdown_bold() {
        let a = browser_audit::Audit {
            chrome_installed: true,
            ..Default::default()
        };
        for it in [webrtc(&a), doh(&a), browser_locale(None, None)] {
            assert!(!it.detail.contains("**"), "{}: {}", it.id, it.detail);
            assert!(!it.label.contains("**"), "{}", it.label);
        }
    }
}
