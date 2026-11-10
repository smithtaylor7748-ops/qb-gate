//! Explicit, unauthenticated ASN lookup. Network category is not IP reputation.
use crate::error::{GateError, Result};
use serde::Deserialize;
use std::net::IpAddr;
use std::time::Duration;

const LIMIT: usize = 128 * 1024;

#[derive(Debug)]
pub struct AsnReport {
    pub ip: String,
    pub asn: Option<u32>,
    pub organization: Option<String>,
    pub network_type: Option<String>,
}

#[derive(Deserialize)]
struct IpInfo {
    ip: String,
    org: Option<String>,
}

#[derive(Deserialize)]
struct Networks {
    data: Vec<Network>,
}

#[derive(Deserialize)]
struct Network {
    asn: u32,
    info_type: Option<String>,
}

fn display_text(raw: &str) -> Option<String> {
    let clean: String = raw.chars().filter(|c| !c.is_control()).take(120).collect();
    let clean = clean.trim();
    (!clean.is_empty()).then(|| clean.to_string())
}

fn parse_ipinfo(target: IpAddr, raw: &[u8]) -> Result<AsnReport> {
    let info: IpInfo = serde_json::from_slice(raw)
        .map_err(|_| GateError::Other("IPinfo 返回格式无法识别，未采用该结果。".into()))?;
    if super::ip_lookup::parse_target(&info.ip).ok() != Some(target) {
        return Err(GateError::Other(
            "IPinfo 返回的 IP 与查询目标不一致。".into(),
        ));
    }
    let org = info.org.as_deref().unwrap_or_default();
    let (number, name) = org.split_once(char::is_whitespace).unwrap_or((org, ""));
    let asn = number
        .strip_prefix("AS")
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|n| *n > 0);
    Ok(AsnReport {
        ip: target.to_string(),
        asn,
        organization: display_text(name),
        network_type: None,
    })
}

fn parse_network(asn: u32, raw: &[u8]) -> Result<Option<String>> {
    let response: Networks = serde_json::from_slice(raw)
        .map_err(|_| GateError::Other("PeeringDB 返回格式无法识别，网络类型未知。".into()))?;
    let matching: Vec<_> = response.data.iter().filter(|n| n.asn == asn).collect();
    // No record or multiple inconsistent records are not evidence of a category.
    if matching.len() != 1 {
        return Ok(None);
    }
    Ok(matching[0]
        .info_type
        .as_deref()
        .and_then(display_text)
        .filter(|value| !value.eq_ignore_ascii_case("Not Disclosed")))
}

async fn fetch(client: &reqwest::Client, url: &str, source: &str) -> Result<Vec<u8>> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| GateError::Other(format!("{source} 暂时无法连接，未获得检测结论。")))?;
    if !response.status().is_success() {
        return Err(GateError::Other(format!(
            "{source} 返回 HTTP {}，未获得检测结论。",
            response.status().as_u16()
        )));
    }
    if response.content_length().is_some_and(|n| n > LIMIT as u64) {
        return Err(GateError::Other(format!("{source} 响应超过大小限制。")));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| GateError::Other(format!("{source} 响应读取失败。")))?
    {
        if bytes.len() + chunk.len() > LIMIT {
            return Err(GateError::Other(format!("{source} 响应超过大小限制。")));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Only the two fixed HTTPS services receive the target IP / public ASN.
/// This never reads API keys, follows redirects, retries, or stores a history.
pub async fn lookup(raw: &str) -> Result<AsnReport> {
    let target = super::ip_lookup::parse_target(raw)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8))
        .connect_timeout(Duration::from_secs(4))
        .user_agent("QB Gate ASN diagnostics")
        .build()?;
    let raw = fetch(
        &client,
        &format!("https://ipinfo.io/{target}/json"),
        "IPinfo",
    )
    .await?;
    let mut report = parse_ipinfo(target, &raw)?;
    if let Some(asn) = report.asn {
        let raw = fetch(
            &client,
            &format!("https://www.peeringdb.com/api/net?asn={asn}&fields=asn,info_type"),
            "PeeringDB",
        )
        .await?;
        report.network_type = parse_network(asn, &raw)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_the_requested_ip_and_structured_network_category() {
        let target = "203.0.113.7".parse().unwrap();
        let report = parse_ipinfo(
            target,
            br#"{"ip":"203.0.113.7","org":"AS64500 Example ISP"}"#,
        )
        .unwrap();
        assert_eq!(report.asn, Some(64500));
        assert_eq!(report.organization.as_deref(), Some("Example ISP"));
        assert!(parse_ipinfo(target, br#"{"ip":"203.0.113.8","org":"AS64500 Example"}"#).is_err());
        assert_eq!(
            parse_network(
                64500,
                br#"{"data":[{"asn":64500,"info_type":"Cable/DSL/ISP"}]}"#
            )
            .unwrap()
            .as_deref(),
            Some("Cable/DSL/ISP")
        );
    }

    #[test]
    fn missing_or_wrong_asn_never_becomes_residential_or_hosting() {
        assert_eq!(
            parse_network(
                64500,
                br#"{"data":[{"asn":64500,"info_type":"Not Disclosed"}]}"#
            )
            .unwrap(),
            None
        );
        assert!(parse_ipinfo(
            "203.0.113.7".parse().unwrap(),
            br#"{"ip":"203.0.113.7","org":"Unknown Residential"}"#
        )
        .unwrap()
        .asn
        .is_none());
        assert_eq!(
            parse_network(
                64500,
                br#"{"data":[{"asn":64501,"info_type":"Cable/DSL/ISP"}]}"#
            )
            .unwrap(),
            None
        );
        assert_eq!(parse_network(64500, br#"{"data":[]}"#).unwrap(), None);
        assert_eq!(
            parse_network(64500, br#"{"data":[{"asn":64500,"info_type":""}]}"#).unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn local_addresses_are_rejected_before_any_request() {
        for ip in [
            "127.0.0.1",
            "192.168.1.1",
            "::1",
            "::ffff:127.0.0.1",
            "https://example.com",
        ] {
            assert!(lookup(ip).await.is_err());
        }
    }
}
