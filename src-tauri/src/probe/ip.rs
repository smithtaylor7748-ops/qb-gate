//! 出口 IP 与纯净度探测。
//!
//! **面板自测不是权威判定。** 权威判定由用户自己去 IPQualityScore 与
//! ippure.com 看，通过标准写死在 `verdict.rs` 的常量里，界面上原样展示。
//! 这里做的只是「一定的查纯净度能力」，UI 上必须标注不权威。

use crate::error::{GateError, Result};
use serde::{Deserialize, Serialize};

/// ippure 的公开接口，无需 API Key。
/// 站方注明「尚处在测试阶段，可能会有变动」—— 字段缺失要当正常情况处理，
/// 不要 unwrap，也不要缺字段就报错。
const IPPURE_INFO: &str = "https://my.ippure.com/v1/info";

/// 只用来兜底拿一个出口 IP，不参与纯净度判定。
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
    let c = client()?;
    let resp = c.get(IPPURE_INFO).send().await?;
    if !resp.status().is_success() {
        return Err(GateError::Other(format!(
            "ippure 接口返回 {}",
            resp.status()
        )));
    }
    Ok(resp.json::<IpInfo>().await?)
}

/// 只要一个出口 IP。门禁每轮都调这个，所以要便宜且有兜底。
pub async fn public_ip() -> Result<String> {
    if let Ok(info) = ip_info().await {
        if !info.ip.is_empty() {
            return Ok(info.ip);
        }
    }
    let c = client()?;
    for url in FALLBACK_IP {
        if let Ok(r) = c.get(*url).send().await {
            if let Ok(t) = r.text().await {
                let t = t.trim().to_string();
                if !t.is_empty() {
                    return Ok(t);
                }
            }
        }
    }
    Err(GateError::IpUnknown)
}
