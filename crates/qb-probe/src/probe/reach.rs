//! Anthropic 服务可达、claude.ai 的解析、IPv6 的真实出口（2026-09-24）。
//!
//! 三件事都参照 [zzusec/CheckClaude](https://github.com/zzusec/CheckClaude)
//! （MIT，Copyright (c) 2026 zzusec）的检测项与判法，Rust 这边自己写。
//! 这一层只取数据、给出分类；「算过 / 不过」由体检那一层（`qb-sysenv::checkup`）定 ——
//! 跟 `verdict` 同一个分工：结论与处置分开。
//!
//! # 服务可达：不带凭据问一次，看回的是 401 还是 403
//!
//! 对 `api.anthropic.com/v1/messages` 发一个**不带任何密钥**的 `POST {}`：
//! 地区放行时回 401（`authentication_error`，缺密钥）；地区被拦时回 403
//! （`forbidden` / `Request not allowed`）。一分钱不花、不带身份 —— 对面看到的只是
//! 「这个 IP 来问了一下」。这是整个体检里**最直接**的一个信号：前面那些 IP、时区、
//! 指纹都是间接的，这一个是 Anthropic 自己的回答。
//!
//! `claude.ai/robots.txt` 与 `www.anthropic.com/robots.txt` 只看打不打得开。
//! 403 带 `cf-mitigated` 头的是 Cloudflare 的人机验证页 —— **说明不了地区**，不许报成拦截。
//!
//! # claude.ai 的解析
//!
//! 用系统解析器（跟 Claude Code、浏览器同一个）解析 `claude.ai` 与 `api.anthropic.com`，
//! 看落在哪一段：代理接管（fake-ip）、Anthropic 自己的段、Cloudflare 的段、
//! 私网 / 回环（被污染）、别的（可疑）。地址段是**公开的、全世界一样的**常量，
//! 不是本机事实（CLAUDE.md「别写死本机事实」说的是另一回事）。
//! 实机 2026-09-24：三个域名都解析到 `160.79.104.10`。
//!
//! # IPv6 的真实出口
//!
//! 原来体检只看「网卡上 IPv6 开没开」—— 开着不等于有 v6 出口（很多宽带根本不给 v6），
//! 关着也看不出隧道到底接没接管 v6。这里直接访问**只有 AAAA 记录**的回显服务：
//! 连不上 = 没有 v6 出口；回来的是 IPv4 = 走的是代理；回来的是 IPv6 = 真有 v6 出口，
//! 再查它落在哪个国家、跟 IPv4 出口比。只用只有 v6 的域名 —— 双栈域名在没有 v6 时
//! 会退回 v4，得出「有 v6 出口」的错结论（CheckClaude 的注释里写着这一条）。
//!
//! 三件事的请求都走 [`super::ip::build_client`]：绕过系统代理那一份跟门禁同一条路。

use crate::error::Result;
use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub const API_MESSAGES: &str = "https://api.anthropic.com/v1/messages";
pub const CLAUDE_ROBOTS: &str = "https://claude.ai/robots.txt";
pub const ANTHROPIC_ROBOTS: &str = "https://www.anthropic.com/robots.txt";

/// 只有 AAAA 记录的 IP 回显服务。按顺序试，第一个答上来的算数。
pub const V6_ONLY: &[&str] = &[
    "https://api6.ipify.org",
    "https://ipv6.icanhazip.com",
    "https://v6.ident.me",
];

// ------------------------------------------------------------------ 服务可达

/// 问一次的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HttpOutcome {
    pub url: String,
    /// `None` = 连不上（超时、DNS、TLS、被重置）。
    pub status: Option<u16>,
    /// 响应头里有 `cf-mitigated`：Cloudflare 的人机验证页。
    pub cf_challenge: bool,
    /// 连不上时的原因，给人看的一句话。
    pub error: Option<String>,
}

/// 三个地址各问一次的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReachReport {
    pub api: HttpOutcome,
    pub web: HttpOutcome,
    pub site: HttpOutcome,
}

/// 一个地址算是什么情况。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// 通了（API 回 401 / 400，网页回 2xx / 3xx）。
    Open,
    /// 403，而且不是 Cloudflare 的验证页 —— 按地区拦截算。
    Blocked,
    /// Cloudflare 的人机验证页。说明不了地区。
    Challenge,
    /// 连不上。
    Unreachable,
    /// 回了别的状态码。
    Other(u16),
}

/// API 那一个：不带密钥，401 / 400 就是「放行了，只是没带钥匙」。
pub fn classify_api(o: &HttpOutcome) -> Reach {
    match o.status {
        None => Reach::Unreachable,
        Some(401) | Some(400) => Reach::Open,
        Some(403) if o.cf_challenge => Reach::Challenge,
        Some(403) => Reach::Blocked,
        Some(s) => Reach::Other(s),
    }
}

/// 网页那两个：打得开就行。
pub fn classify_site(o: &HttpOutcome) -> Reach {
    match o.status {
        None => Reach::Unreachable,
        Some(s) if (200..400).contains(&s) => Reach::Open,
        Some(403) if o.cf_challenge => Reach::Challenge,
        Some(403) => Reach::Blocked,
        Some(s) => Reach::Other(s),
    }
}

fn outcome(url: &str, r: std::result::Result<reqwest::Response, reqwest::Error>) -> HttpOutcome {
    match r {
        Ok(resp) => HttpOutcome {
            url: url.to_string(),
            status: Some(resp.status().as_u16()),
            cf_challenge: resp.headers().contains_key("cf-mitigated"),
            error: None,
        },
        Err(e) => HttpOutcome {
            url: url.to_string(),
            status: None,
            cf_challenge: false,
            error: Some(if e.is_timeout() {
                "超时".to_string()
            } else if e.is_connect() {
                "连不上".to_string()
            } else {
                "请求失败".to_string()
            }),
        },
    }
}

/// 三个地址并发各问一次。**不带任何凭据。**
pub async fn reach(c: &reqwest::Client) -> ReachReport {
    let api = c
        .post(API_MESSAGES)
        .header("content-type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .body("{}")
        .send();
    let web = c.get(CLAUDE_ROBOTS).send();
    let site = c.get(ANTHROPIC_ROBOTS).send();
    let (a, w, s) = tokio::join!(api, web, site);
    ReachReport {
        api: outcome(API_MESSAGES, a),
        web: outcome(CLAUDE_ROBOTS, w),
        site: outcome(ANTHROPIC_ROBOTS, s),
    }
}

// ------------------------------------------------------------------ 解析

/// 一个解析结果落在哪一段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsClass {
    /// 代理接管了解析（fake-ip）：`198.18.0.0/15`，或 IPv6 的唯一本地地址 `fc00::/7`。
    FakeIp,
    /// Anthropic 自己的入口段。
    Anthropic,
    /// Cloudflare 的段。
    Cloudflare,
    /// 私网、回环、`0.0.0.0`、链路本地、运营商级 NAT —— claude.ai 不可能在这里：被污染 / 劫持。
    Private,
    /// 都不是。可能是被污染到了一个公网地址，也可能是代理用了自定义的 fake-ip 段。
    Other,
}

/// Anthropic 公布的入口段（官方文档「IP addresses」一页）。
const ANTHROPIC_V4: &[(Ipv4Addr, u8)] = &[(Ipv4Addr::new(160, 79, 104, 0), 23)];
const ANTHROPIC_V6: &[(Ipv6Addr, u8)] = &[(Ipv6Addr::new(0x2607, 0x6bc0, 0, 0, 0, 0, 0, 0), 48)];

/// Cloudflare 公布的地址段（cloudflare.com/ips）。
const CLOUDFLARE_V4: &[(Ipv4Addr, u8)] = &[
    (Ipv4Addr::new(173, 245, 48, 0), 20),
    (Ipv4Addr::new(103, 21, 244, 0), 22),
    (Ipv4Addr::new(103, 22, 200, 0), 22),
    (Ipv4Addr::new(103, 31, 4, 0), 22),
    (Ipv4Addr::new(141, 101, 64, 0), 18),
    (Ipv4Addr::new(108, 162, 192, 0), 18),
    (Ipv4Addr::new(190, 93, 240, 0), 20),
    (Ipv4Addr::new(188, 114, 96, 0), 20),
    (Ipv4Addr::new(197, 234, 240, 0), 22),
    (Ipv4Addr::new(198, 41, 128, 0), 17),
    (Ipv4Addr::new(162, 158, 0, 0), 15),
    (Ipv4Addr::new(104, 16, 0, 0), 13),
    (Ipv4Addr::new(104, 24, 0, 0), 14),
    (Ipv4Addr::new(172, 64, 0, 0), 13),
    (Ipv4Addr::new(131, 0, 72, 0), 22),
];
const CLOUDFLARE_V6: &[(Ipv6Addr, u8)] = &[
    (Ipv6Addr::new(0x2400, 0xcb00, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x2606, 0x4700, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x2803, 0xf800, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x2405, 0xb500, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x2405, 0x8100, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x2a06, 0x98c0, 0, 0, 0, 0, 0, 0), 29),
    (Ipv6Addr::new(0x2c0f, 0xf248, 0, 0, 0, 0, 0, 0), 32),
];

fn in_v4(ip: Ipv4Addr, nets: &[(Ipv4Addr, u8)]) -> bool {
    let x = u32::from(ip);
    nets.iter().any(|(n, len)| {
        let mask = if *len == 0 { 0 } else { u32::MAX << (32 - len) };
        x & mask == u32::from(*n) & mask
    })
}

fn in_v6(ip: Ipv6Addr, nets: &[(Ipv6Addr, u8)]) -> bool {
    let x = u128::from(ip);
    nets.iter().any(|(n, len)| {
        let mask = if *len == 0 {
            0
        } else {
            u128::MAX << (128 - len)
        };
        x & mask == u128::from(*n) & mask
    })
}

pub fn classify_ip(ip: IpAddr) -> DnsClass {
    match ip {
        IpAddr::V4(v4) => {
            if in_v4(v4, &[(Ipv4Addr::new(198, 18, 0, 0), 15)]) {
                DnsClass::FakeIp
            } else if in_v4(v4, ANTHROPIC_V4) {
                DnsClass::Anthropic
            } else if in_v4(v4, CLOUDFLARE_V4) {
                DnsClass::Cloudflare
            } else if v4.is_private()
                || v4.is_loopback()
                || v4.is_unspecified()
                || v4.is_link_local()
                || in_v4(
                    v4,
                    &[
                        (Ipv4Addr::new(0, 0, 0, 0), 8),
                        (Ipv4Addr::new(100, 64, 0, 0), 10),
                    ],
                )
            {
                DnsClass::Private
            } else {
                DnsClass::Other
            }
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return classify_ip(IpAddr::V4(v4));
            }
            // 唯一本地地址：Clash Meta 这类的 IPv6 fake-ip 就在这一段（默认 fdfe:dcba:9876::/64）。
            if in_v6(v6, &[(Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 0), 7)]) {
                DnsClass::FakeIp
            } else if in_v6(v6, ANTHROPIC_V6) {
                DnsClass::Anthropic
            } else if in_v6(v6, CLOUDFLARE_V6) {
                DnsClass::Cloudflare
            } else if v6.is_loopback()
                || v6.is_unspecified()
                || in_v6(v6, &[(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), 10)])
            {
                DnsClass::Private
            } else {
                DnsClass::Other
            }
        }
    }
}

/// 一个域名解析出来的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Resolved {
    pub host: String,
    /// 解析出来的地址（去重、IPv4 在前）。
    pub addrs: Vec<String>,
    /// 解析失败的原因。
    pub error: Option<String>,
}

/// 用系统解析器解析一个域名。5 秒超时。
pub async fn resolve(host: &str) -> Resolved {
    let lookup = tokio::net::lookup_host((host, 443));
    match tokio::time::timeout(std::time::Duration::from_secs(5), lookup).await {
        Ok(Ok(it)) => {
            let mut v: Vec<IpAddr> = it.map(|a| a.ip()).collect();
            v.sort_by_key(|ip| (ip.is_ipv6(), *ip));
            v.dedup();
            Resolved {
                host: host.to_string(),
                addrs: v.iter().map(ToString::to_string).collect(),
                error: None,
            }
        }
        Ok(Err(e)) => Resolved {
            host: host.to_string(),
            addrs: Vec::new(),
            error: Some(format!("解析失败：{e}")),
        },
        Err(_) => Resolved {
            host: host.to_string(),
            addrs: Vec::new(),
            error: Some("解析超时".into()),
        },
    }
}

/// `claude.ai` 与 `api.anthropic.com` 各解析一次。
pub async fn resolve_claude() -> Vec<Resolved> {
    let (a, b) = tokio::join!(resolve("claude.ai"), resolve("api.anthropic.com"));
    vec![a, b]
}

// ------------------------------------------------------------------ IPv6 出口

/// IPv6 那一条路出去是什么样。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum V6Exit {
    /// 三个只有 v6 的服务一个都连不上：这台机器没有 IPv6 出口。
    None,
    /// 回来的是 IPv4：请求被代理接走了，本机没有自己的 v6 出口。
    ViaProxy { ip: String },
    /// 真有 IPv6 出口。`country` 查不到时是 `None`。
    Exit { ip: String, country: Option<String> },
}

fn parse_ip(body: &str) -> Option<IpAddr> {
    body.trim().parse().ok()
}

/// 问一次 IPv6 出口；有 v6 出口时再查它落在哪个国家。
pub async fn ipv6_exit(c: &reqwest::Client) -> V6Exit {
    for url in V6_ONLY {
        let Ok(resp) = c.get(*url).send().await else {
            continue;
        };
        let Ok(text) = resp.text().await else {
            continue;
        };
        match parse_ip(&text) {
            Some(IpAddr::V4(v4)) => return V6Exit::ViaProxy { ip: v4.to_string() },
            Some(IpAddr::V6(v6)) => {
                let country = v6_country(c, &v6.to_string()).await;
                return V6Exit::Exit {
                    ip: v6.to_string(),
                    country,
                };
            }
            None => continue,
        }
    }
    V6Exit::None
}

async fn v6_country(c: &reqwest::Client, ip: &str) -> Option<String> {
    let text = c
        .get(format!("https://ipinfo.io/{ip}/country"))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    super::ip::normalize_country(&text)
}

/// 绕过系统代理（跟门禁同一条路）问一轮 IPv6 出口。
pub async fn ipv6_exit_direct() -> Result<V6Exit> {
    Ok(ipv6_exit(&super::ip::build_client(true)?).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(status: Option<u16>, cf: bool) -> HttpOutcome {
        HttpOutcome {
            url: "u".into(),
            status,
            cf_challenge: cf,
            error: None,
        }
    }

    /// 不带钥匙问 API：401 是「放行了」，403 是「地区拦了」，验证页说明不了地区。
    #[test]
    fn the_api_answer_reads_as_region_status() {
        assert_eq!(classify_api(&o(Some(401), false)), Reach::Open);
        assert_eq!(classify_api(&o(Some(400), false)), Reach::Open);
        assert_eq!(classify_api(&o(Some(403), false)), Reach::Blocked);
        assert_eq!(classify_api(&o(Some(403), true)), Reach::Challenge);
        assert_eq!(classify_api(&o(None, false)), Reach::Unreachable);
        assert_eq!(classify_api(&o(Some(500), false)), Reach::Other(500));
    }

    #[test]
    fn a_site_is_open_on_any_2xx_or_3xx() {
        assert_eq!(classify_site(&o(Some(200), false)), Reach::Open);
        assert_eq!(classify_site(&o(Some(301), false)), Reach::Open);
        assert_eq!(classify_site(&o(Some(403), true)), Reach::Challenge);
        assert_eq!(classify_site(&o(Some(403), false)), Reach::Blocked);
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn resolved_addresses_land_in_the_right_bucket() {
        // 实机 2026-09-24：三个域名都在这里。
        assert_eq!(classify_ip(ip("160.79.104.10")), DnsClass::Anthropic);
        assert_eq!(classify_ip(ip("160.79.105.200")), DnsClass::Anthropic);
        assert_eq!(classify_ip(ip("160.79.106.1")), DnsClass::Other, "/23 之外");
        assert_eq!(classify_ip(ip("198.18.0.23")), DnsClass::FakeIp);
        assert_eq!(classify_ip(ip("198.19.255.1")), DnsClass::FakeIp);
        assert_eq!(classify_ip(ip("104.18.32.7")), DnsClass::Cloudflare);
        assert_eq!(classify_ip(ip("172.67.1.1")), DnsClass::Cloudflare);
        assert_eq!(classify_ip(ip("127.0.0.1")), DnsClass::Private);
        assert_eq!(classify_ip(ip("0.0.0.0")), DnsClass::Private);
        assert_eq!(classify_ip(ip("10.1.2.3")), DnsClass::Private);
        assert_eq!(classify_ip(ip("192.168.1.1")), DnsClass::Private);
        assert_eq!(classify_ip(ip("100.100.1.1")), DnsClass::Private);
        assert_eq!(classify_ip(ip("31.13.94.37")), DnsClass::Other);
        assert_eq!(classify_ip(ip("fdfe:dcba:9876::2")), DnsClass::FakeIp);
        assert_eq!(classify_ip(ip("2607:6bc0::10")), DnsClass::Anthropic);
        assert_eq!(classify_ip(ip("2606:4700::6810:1")), DnsClass::Cloudflare);
        assert_eq!(classify_ip(ip("::1")), DnsClass::Private);
        assert_eq!(classify_ip(ip("::ffff:160.79.104.10")), DnsClass::Anthropic);
    }

    #[test]
    fn echo_bodies_parse_either_family() {
        assert_eq!(parse_ip("2001:db8::1\n"), Some(ip("2001:db8::1")));
        assert_eq!(parse_ip(" 203.0.113.7 "), Some(ip("203.0.113.7")));
        assert_eq!(parse_ip("<html>"), None);
    }
}
