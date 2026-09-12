//! 出口 IP、国家与纯净度探测。
//!
//! **面板自测不是权威判定。** 权威判定由用户自己去 IPQualityScore 与
//! ippure.com 看，通过标准写死在 `verdict.rs` 的常量里，界面上原样展示。
//! 这里做的只是「一定的查纯净度能力」，UI 上必须标注不权威。
//!
//! # 为什么是三个源并发，而不是一个源加兜底
//!
//! 门禁改成 fail-closed 之后（查不到 = 不合格），单一数据源的可用性直接等于
//! 使用者的可用性 —— ippure 抽一次风，他就写不了代码。所以三家并发问，
//! **任一家答上来就算「查到了」**。这不是放宽判定口径，是让「查不到」
//! 真的只在三家全挂时才成立。
//!
//! 三家都问国家，是国家白名单层（`gate::judge`）的输入：
//! 两家说的不一样本身就是信号，交给 `judge` 按最严的算。

use crate::error::{GateError, Result};
use serde::{Deserialize, Serialize};

/// ippure 的公开接口，无需 API Key。
/// 站方注明「尚处在测试阶段，可能会有变动」—— 字段缺失要当正常情况处理，
/// 不要 unwrap，也不要缺字段就报错。
const IPPURE_INFO: &str = "https://my.ippure.com/v1/info";

/// Cloudflare 的 trace 端点。纯文本 `key=value` 逐行，无需 Key，
/// 可用性是这三家里最好的。`loc=` 给两位国家码。
const CF_TRACE: &str = "https://www.cloudflare.com/cdn-cgi/trace";

/// ipinfo 的免费端点，无需 Token 也能拿到 ip 与 country。
const IPINFO: &str = "https://ipinfo.io/json";

/// 三家全挂时最后再试一次，只为拿一个 IP，**不参与国家判定**。
const FALLBACK_IP: &[&str] = &["https://api.ipify.org", "https://icanhazip.com"];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IpInfo {
    pub ip: String,
    #[serde(default)]
    pub asn: Option<u32>,
    #[serde(default, rename = "asOrganization")]
    pub as_organization: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default, rename = "countryCode")]
    pub country_code: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    /// IANA 时区名，例如 America/New_York。启动前的时区对齐就是拿它比对。
    #[serde(default)]
    pub timezone: Option<String>,
    /// 0–100，越低越干净。
    #[serde(default, rename = "fraudScore")]
    pub fraud_score: Option<u32>,
    #[serde(default, rename = "isResidential")]
    pub is_residential: Option<bool>,
    #[serde(default, rename = "isBroadcast")]
    pub is_broadcast: Option<bool>,
}

/// 一轮多源探测的结果。
///
/// `countries` 里一个源一条，**只收真答上来的**。空 = 三家都没给出国家，
/// 这跟「三家都说 US」是两回事，`judge` 必须能分得开。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reading {
    pub ip: Option<String>,
    /// (源名, ISO 3166-1 alpha-2 大写)
    pub countries: Vec<(String, String)>,
}

impl Reading {
    /// 各源报的国家，去重后按字母序。用来判一致 / 冲突。
    ///
    /// **这里再规整一次大小写**，尽管 `reading()` 收集时已经规整过了。
    /// 重复不是白费：判定侧把「US」和「us」看成两个值就会报国家冲突，
    /// 而国家冲突在 fail-closed 下等于收掉正在进行的会话。
    /// 这条不变量必须待在用它的地方，不能指望远处的调用方记得。
    pub fn distinct_countries(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .countries
            .iter()
            .map(|(_, c)| c.trim().to_uppercase())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// 冲突时写进日志的原文，形如 `ippure=US cloudflare=HK`。
    ///
    /// 必须原样记下来 —— 国家冲突会导致误杀，而事后只看「已收摊」这四个字
    /// 根本查不出是哪一家在胡说（档案 §7.17：一个说谎的结果比报错难查）。
    pub fn detail(&self) -> String {
        self.countries
            .iter()
            .map(|(s, c)| format!("{s}={c}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// 把各家给的国家码规整成两位大写；认不出来的一律当没答。
///
/// Cloudflare 在认不出时给 `XX`，走 Tor 时给 `T1` —— 这两个都**不是国家**，
/// 当成国家会让白名单判定凭空多出一个永远不匹配的值。
fn normalize_country(raw: &str) -> Option<String> {
    let c = raw.trim().to_uppercase();
    if c.len() != 2 || !c.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return None;
    }
    if c == "XX" || c == "T1" {
        return None;
    }
    Some(c)
}

/// 构造一个**绕过系统代理**的客户端。
///
/// 这一条是刻意的，不要删：门禁要量的是隧道真正的出口，
/// 不能让某个代理软件把出口 IP 伪装成别的。
///
/// 但记住它的失效条件 —— 现在的隧道是路由模式（xray_tun 持有 0.0.0.0/0），
/// 查询请求同样从隧道出去，所以 no_proxy 只是纯保险。
/// **哪天换成系统代理模式的机场，这句话会让门禁量到你真实的 ISP 出口**，
/// 到那时必须回来改这里。
fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("QB Gate/0.1")
        .build()
        .map_err(GateError::from)
}

/// 完整纯净度信息。失败时返回 Err，调用方自己决定要不要降级。
pub async fn ip_info() -> Result<IpInfo> {
    ip_info_with(&client()?).await
}

async fn ip_info_with(c: &reqwest::Client) -> Result<IpInfo> {
    let resp = c.get(IPPURE_INFO).send().await?;
    if !resp.status().is_success() {
        return Err(GateError::Other(format!(
            "ippure 接口返回 {}",
            resp.status()
        )));
    }
    Ok(resp.json::<IpInfo>().await?)
}

/// Cloudflare trace：纯文本逐行 `key=value`。
async fn cf_trace(c: &reqwest::Client) -> Option<(Option<String>, Option<String>)> {
    let text = c.get(CF_TRACE).send().await.ok()?.text().await.ok()?;
    let mut ip = None;
    let mut loc = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("ip=") {
            let v = v.trim();
            if !v.is_empty() {
                ip = Some(v.to_string());
            }
        } else if let Some(v) = line.strip_prefix("loc=") {
            loc = normalize_country(v);
        }
    }
    Some((ip, loc))
}

#[derive(Deserialize)]
struct IpInfoIo {
    #[serde(default)]
    ip: Option<String>,
    #[serde(default)]
    country: Option<String>,
}

async fn ipinfo(c: &reqwest::Client) -> Option<(Option<String>, Option<String>)> {
    let v: IpInfoIo = c.get(IPINFO).send().await.ok()?.json().await.ok()?;
    let country = v.country.as_deref().and_then(normalize_country);
    Some((v.ip.filter(|s| !s.is_empty()), country))
}

/// 三家并发问一轮。任一家答上来就算查到了。
///
/// **IP 取第一个答上来的，不做冲突判定。** 双栈机器上 Cloudflare 常回 IPv6、
/// ippure 回 IPv4，两者都对 —— 拿它当冲突会让门禁一直误杀。
/// 国家不一样才是真信号，那个交给 `judge`。
pub async fn reading() -> Reading {
    let Ok(c) = client() else {
        return Reading::default();
    };

    let (pure, cf, info) = tokio::join!(ip_info_with(&c), cf_trace(&c), ipinfo(&c));

    let mut out = Reading::default();
    let mut push = |src: &str, ip: Option<String>, country: Option<String>| {
        if out.ip.is_none() {
            out.ip = ip.filter(|s| !s.is_empty());
        }
        if let Some(c) = country {
            out.countries.push((src.to_string(), c));
        }
    };

    // 顺序就是 IP 的优先级：ippure 是纯净度那套数据的同一个源，优先用它的。
    if let Ok(i) = pure {
        let country = i.country_code.as_deref().and_then(normalize_country);
        push("ippure", Some(i.ip), country);
    }
    if let Some((ip, loc)) = cf {
        push("cloudflare", ip, loc);
    }
    if let Some((ip, country)) = info {
        push("ipinfo", ip, country);
    }

    if out.ip.is_none() {
        out.ip = fallback_ip(&c).await;
    }
    out
}

async fn fallback_ip(c: &reqwest::Client) -> Option<String> {
    for url in FALLBACK_IP {
        if let Ok(r) = c.get(*url).send().await {
            if let Ok(t) = r.text().await {
                let t = t.trim().to_string();
                if !t.is_empty() {
                    return Some(t);
                }
            }
        }
    }
    None
}

/// 只要一个出口 IP。走的是同一轮多源探测 —— 三家全挂才算查不到。
pub async fn public_ip() -> Result<String> {
    reading().await.ip.ok_or(GateError::IpUnknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn country_codes_are_normalized_to_two_upper_letters() {
        assert_eq!(normalize_country("us"), Some("US".into()));
        assert_eq!(normalize_country(" Tw "), Some("TW".into()));
    }

    /// Cloudflare 认不出时给 `XX`、走 Tor 给 `T1`。
    /// 这两个当成国家会在白名单里凭空多出一个永远不匹配的值。
    #[test]
    fn cloudflare_placeholders_are_not_countries() {
        assert_eq!(normalize_country("XX"), None);
        assert_eq!(normalize_country("T1"), None);
        assert_eq!(normalize_country(""), None);
        assert_eq!(normalize_country("USA"), None);
        assert_eq!(normalize_country("1"), None);
    }

    #[test]
    fn distinct_countries_dedupes_and_sorts() {
        let r = Reading {
            ip: Some("203.0.113.7".into()),
            countries: vec![
                ("cloudflare".into(), "US".into()),
                ("ippure".into(), "US".into()),
            ],
        };
        assert_eq!(r.distinct_countries(), vec!["US".to_string()]);
    }

    #[test]
    fn conflicting_sources_stay_visible_in_detail() {
        // 冲突原文必须留得住 —— 误杀之后就靠这一行查是哪家在胡说。
        let r = Reading {
            ip: Some("203.0.113.7".into()),
            countries: vec![
                ("ippure".into(), "US".into()),
                ("cloudflare".into(), "HK".into()),
            ],
        };
        assert_eq!(r.distinct_countries(), vec!["HK".to_string(), "US".to_string()]);
        assert_eq!(r.detail(), "ippure=US cloudflare=HK");
    }

    #[test]
    fn no_country_answers_is_not_the_same_as_agreement() {
        let r = Reading { ip: Some("203.0.113.7".into()), countries: vec![] };
        assert!(r.distinct_countries().is_empty());
        assert_eq!(r.detail(), "");
    }
}
