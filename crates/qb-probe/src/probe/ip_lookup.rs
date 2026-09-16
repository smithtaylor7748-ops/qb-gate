//! User-requested IP intelligence, separate from current-egress and gate readings.
//! The documented IPQuery API accepts a target IP; IPPure's /v1/info does not.
use crate::error::{GateError, Result};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use ts_rs::TS;

const ENDPOINT: &str = "https://api.ipquery.io/";

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct IpLookupReport {
    pub ip: String,
    pub source: &'static str,
    pub checked_at: String,
    pub asn: Option<String>,
    pub organization: Option<String>,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub city: Option<String>,
    pub timezone: Option<String>,
    /// IPQuery's own 0-100 risk score; never an IPPure or IPQS score.
    pub risk_score: Option<u32>,
    pub is_vpn: Option<bool>,
    pub is_proxy: Option<bool>,
    pub is_tor: Option<bool>,
    pub is_datacenter: Option<bool>,
}

#[derive(Default, Deserialize)]
struct Isp {
    asn: Option<String>,
    org: Option<String>,
    isp: Option<String>,
}
#[derive(Default, Deserialize)]
struct Location {
    country: Option<String>,
    country_code: Option<String>,
    city: Option<String>,
    timezone: Option<String>,
}
#[derive(Default, Deserialize)]
struct Risk {
    risk_score: Option<u32>,
    is_vpn: Option<bool>,
    is_proxy: Option<bool>,
    is_tor: Option<bool>,
    is_datacenter: Option<bool>,
}
#[derive(Deserialize)]
struct Response {
    ip: String,
    isp: Option<Isp>,
    location: Option<Location>,
    risk: Option<Risk>,
}

fn parse_target(raw: &str) -> Result<IpAddr> {
    let ip = raw.trim().parse::<IpAddr>().map_err(|_| {
        GateError::Other("请输入有效的 IPv4 或 IPv6 地址，不要填写网址、端口或代理账号。".into())
    })?;
    let public = match ip {
        IpAddr::V4(v4) => {
            !v4.is_private()
                && !v4.is_loopback()
                && !v4.is_link_local()
                && !v4.is_unspecified()
                && !v4.is_multicast()
                && !v4.is_broadcast()
                && v4.octets()[0] != 0
                && !(v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1]))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return parse_target(&v4.to_string());
            }
            !v6.is_loopback()
                && !v6.is_unspecified()
                && !v6.is_multicast()
                && !v6.is_unique_local()
                && !v6.is_unicast_link_local()
        }
    };
    if !public {
        return Err(GateError::Other(
            "请填写公网 IP；内网、回环或本地地址无法查询公网风险。".into(),
        ));
    }
    Ok(ip)
}

fn report(target: IpAddr, data: Response) -> Result<IpLookupReport> {
    // A provider ignoring the target must never label our own exit as the bought IP.
    if data.ip.parse::<IpAddr>().ok() != Some(target) {
        return Err(GateError::Other(
            "查询源返回的 IP 与输入不一致，未采用该结果，请重试。".into(),
        ));
    }
    let isp = data.isp.unwrap_or_default();
    let loc = data.location.unwrap_or_default();
    let risk = data.risk.unwrap_or_default();
    Ok(IpLookupReport {
        ip: target.to_string(),
        source: "IPQuery",
        checked_at: chrono::Utc::now().to_rfc3339(),
        asn: isp.asn,
        organization: isp.org.or(isp.isp),
        country: loc.country,
        country_code: loc.country_code,
        city: loc.city,
        timezone: loc.timezone,
        risk_score: risk.risk_score.filter(|n| *n <= 100),
        is_vpn: risk.is_vpn,
        is_proxy: risk.is_proxy,
        is_tor: risk.is_tor,
        is_datacenter: risk.is_datacenter,
    })
}

pub async fn lookup(raw: &str) -> Result<IpLookupReport> {
    let target = parse_target(raw)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .user_agent("QB Gate IP lookup")
        .build()?;
    // Only this fixed HTTPS service receives the IP. Never connect to user input.
    let response = client.get(format!("{ENDPOINT}{target}")).send().await?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(GateError::Other(
            "指定 IP 查询暂时限流，请稍后重试。".into(),
        ));
    }
    if !response.status().is_success() {
        return Err(GateError::Other(format!(
            "指定 IP 查询失败（HTTP {}），请稍后重试。",
            response.status()
        )));
    }
    report(target, response.json::<Response>().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_literal_ipv4_and_ipv6_but_not_urls_ports_or_credentials() {
        assert_eq!(parse_target(" 1.1.1.1 ").unwrap().to_string(), "1.1.1.1");
        assert_eq!(
            parse_target("2001:4860:4860::8888").unwrap().to_string(),
            "2001:4860:4860::8888"
        );
        for raw in [
            "",
            "1.1.1.999",
            "example.com",
            "https://1.1.1.1",
            "1.1.1.1:8080",
            "user:pass@1.1.1.1",
        ] {
            assert!(parse_target(raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn local_and_private_addresses_are_rejected_before_network_io() {
        for raw in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.1.1",
            "172.16.0.1",
            "169.254.1.1",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "::1",
            "::",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(parse_target(raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn mismatched_provider_results_are_not_accepted() {
        let response = serde_json::from_str(r#"{"ip":"8.8.8.8","risk":{"risk_score":0}}"#).unwrap();
        assert!(report("1.1.1.1".parse().unwrap(), response).is_err());
    }

    #[test]
    fn missing_risk_fields_remain_unknown_and_do_not_imply_residential() {
        let response =
            serde_json::from_str(r#"{"ip":"1.1.1.1","risk":{"is_datacenter":false}}"#).unwrap();
        let result = report("1.1.1.1".parse().unwrap(), response).unwrap();
        assert_eq!(result.risk_score, None);
        assert_eq!(result.is_vpn, None);
        assert_eq!(result.is_proxy, None);
        assert_eq!(result.is_tor, None);
        assert_eq!(result.is_datacenter, Some(false));
    }

    #[test]
    fn parses_provider_intelligence_without_changing_score_scales() {
        let response = serde_json::from_str(r#"{"ip":"1.1.1.1","isp":{"asn":"AS13335","org":"Example"},"location":{"country_code":"AU","timezone":"Australia/Sydney"},"risk":{"risk_score":72,"is_vpn":true,"is_proxy":false,"is_tor":false,"is_datacenter":true}}"#).unwrap();
        let result = report("1.1.1.1".parse().unwrap(), response).unwrap();
        assert_eq!(result.source, "IPQuery");
        assert_eq!(result.risk_score, Some(72));
        assert_eq!(result.asn.as_deref(), Some("AS13335"));
        assert_eq!(result.is_vpn, Some(true));
    }

    #[test]
    fn out_of_range_scores_are_unknown() {
        let response =
            serde_json::from_str(r#"{"ip":"1.1.1.1","risk":{"risk_score":101}}"#).unwrap();
        assert_eq!(
            report("1.1.1.1".parse().unwrap(), response)
                .unwrap()
                .risk_score,
            None
        );
    }
}
