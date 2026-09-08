//! DNS 泄露检测（简易通过）。
//!
//! 两条腿走路，缺一条都会漏：
//!
//! **① 真实解析测试**（主）——借 bash.ws 的权威 NS 回显。
//!   做法：随机生成一个 id，逐个解析 `1..10.<id>.bash.ws`，
//!   bash.ws 的权威域名服务器会记录**是谁来查的**，再拉
//!   `https://bash.ws/dnsleak/test/<id>?json` 把解析器清单读回来。
//!   这是 dnsleaktest.com 那一套的公开接口，不需要自建服务器。
//!   手法照抄 ygbull/DNSLeakTester（MIT）。
//!
//! **② 网卡配置检查**（辅）——枚举各网卡配的 DNS，看有没有
//!   「物理网卡指向内网路由器」。这种情况 ① 有时看不见，
//!   但它正是 TUN 开启期间防泄漏边界的缺口。
//!
//! 两者都是**简易通过**。深度排查（抓包、WebRTC、路由）交给高级通过的 Codex。

use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct Resolver {
    pub address: String,
    pub country_code: Option<String>,
    pub country_name: Option<String>,
    pub asn: Option<String>,
    /// 来自网卡配置而非真实解析回显。
    pub from_adapter: bool,
    pub interface: Option<String>,
    pub is_private: bool,
    pub is_domestic: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsReport {
    /// 出口 IP 的 ASN，用于和解析器 ASN 比对。
    pub egress_asn: Option<String>,
    pub resolvers: Vec<Resolver>,
    pub passed: bool,
    pub findings: Vec<String>,
    /// bash.ws 自己给的结论行，原样透出。
    pub upstream_conclusion: Option<String>,
    pub note: &'static str,
}

#[derive(Debug, Deserialize)]
struct BashWsEntry {
    ip: Option<String>,
    country_name: Option<String>,
    #[serde(rename = "country")]
    country_code: Option<String>,
    asn: Option<String>,
    /// "ip" = 你的出口地址；"dns" = 一台解析器；"conclusion" = 结论行
    #[serde(rename = "type")]
    kind: Option<String>,
}

fn is_private(ip: &str) -> bool {
    ip.starts_with("10.")
        || ip.starts_with("192.168.")
        || ip.starts_with("127.")
        || ip.starts_with("169.254.")
        || (ip.starts_with("172.")
            && ip
                .split('.')
                .nth(1)
                .and_then(|o| o.parse::<u8>().ok())
                .is_some_and(|o| (16..=31).contains(&o)))
}

const PROBE_COUNT: u8 = 10;
const DELAY_BETWEEN_PROBES_MS: u64 = 200;
const WAIT_AFTER_PROBES_MS: u64 = 3000;

/// 测试 id **必须由 bash.ws 签发**，不能自己随机生成 ——
/// 权威 NS 只认它自己发出去的 id，自造的 id 查回来永远是空清单，
/// 于是「没查到解析器」会被误读成「没有泄露」。
async fn obtain_test_id() -> Result<String> {
    let c = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let id = c
        .get("https://bash.ws/id")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?
        .trim()
        .to_string();
    if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err(crate::error::GateError::Other(
            "bash.ws 返回的测试 id 不合法".into(),
        ));
    }
    Ok(id)
}

/// 逐个解析探针域名，触发权威 NS 记录「谁来查的」。
///
/// 两个要点：
///   - 走 `lookup_host` 而不是自己发 UDP。必须用**操作系统的解析路径**，
///     自己发包就绕过了要检测的那条链路，测了个寂寞。
///   - 串行 + 间隔 200ms。并发打十个查询容易被解析器合并或限流，
///     回显清单会不全。
async fn trigger_probes(id: &str) {
    for i in 1..=PROBE_COUNT {
        let host = format!("{i}.{id}.bash.ws:80");
        let _ = tokio::net::lookup_host(host).await;
        tokio::time::sleep(std::time::Duration::from_millis(DELAY_BETWEEN_PROBES_MS)).await;
    }
}

async fn fetch_results(id: &str) -> Result<Vec<BashWsEntry>> {
    let c = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let url = format!("https://bash.ws/dnsleak/test/{id}?json");
    let text = c.get(&url).send().await?.error_for_status()?.text().await?;
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

/// 网卡 DNS 配置。只读操作，走 PowerShell 比 GetAdaptersAddresses FFI 省两百行，
/// 且没有安全影响。（ACL 那边不一样 —— 那是管控点，必须用 API，见 gate/acl.rs。）
#[cfg(windows)]
async fn adapter_resolvers() -> Vec<(String, String)> {
    let Ok(out) = tokio::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-DnsClientServerAddress -AddressFamily IPv4 | \
             Where-Object { $_.ServerAddresses.Count -gt 0 } | \
             ForEach-Object { $n=$_.InterfaceAlias; $_.ServerAddresses | \
             ForEach-Object { \"$n`t$_\" } }",
        ])
        .output()
        .await
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .filter(|(_, ip)| !ip.is_empty())
        .collect()
}

#[cfg(not(windows))]
async fn adapter_resolvers() -> Vec<(String, String)> {
    Vec::new()
}

pub async fn check() -> Result<DnsReport> {
    let id = obtain_test_id().await?;
    trigger_probes(&id).await;
    // 权威 NS 记账有延迟，等满 3 秒再取，否则常常读到半截清单。
    tokio::time::sleep(std::time::Duration::from_millis(WAIT_AFTER_PROBES_MS)).await;

    let entries = fetch_results(&id).await.unwrap_or_default();

    let mut egress_asn = None;
    let mut upstream_conclusion = None;
    let mut resolvers: Vec<Resolver> = Vec::new();

    for e in entries {
        match e.kind.as_deref() {
            Some("ip") => egress_asn = e.asn.clone(),
            Some("conclusion") => {
                upstream_conclusion = e.country_name.clone().or_else(|| e.ip.clone())
            }
            Some("dns") => {
                let Some(ip) = e.ip.clone() else { continue };
                let cc = e.country_code.clone();
                resolvers.push(Resolver {
                    is_private: is_private(&ip),
                    is_domestic: cc.as_deref() == Some("CN"),
                    address: ip,
                    country_code: cc,
                    country_name: e.country_name.clone(),
                    asn: e.asn.clone(),
                    from_adapter: false,
                    interface: None,
                });
            }
            _ => {}
        }
    }

    // 补上网卡配置里的解析器（去重）。
    for (iface, ip) in adapter_resolvers().await {
        if resolvers.iter().any(|r| r.address == ip) {
            continue;
        }
        resolvers.push(Resolver {
            is_private: is_private(&ip),
            is_domestic: false,
            address: ip,
            country_code: None,
            country_name: None,
            asn: None,
            from_adapter: true,
            interface: Some(iface),
        });
    }

    let findings = evaluate(&resolvers);

    Ok(DnsReport {
        passed: findings.is_empty() && !resolvers.is_empty(),
        egress_asn,
        upstream_conclusion,
        note: "简易检测覆盖真实解析回显与网卡配置两层。\
               应用层自带解析器、DoH 直连等情况仍看不到；\
               深度排查请用高级通过让 Codex 抓包核实。",
        resolvers,
        findings,
    })
}

/// 判定逻辑抽出来，便于单测。
fn evaluate(resolvers: &[Resolver]) -> Vec<String> {
    let mut findings = Vec::new();
    if resolvers.is_empty() {
        findings.push("没有读到任何解析器，无法判定（可能是网络不通或上游限流）".into());
        return findings;
    }
    for r in resolvers {
        if r.is_domestic {
            findings.push(format!(
                "解析器 {} 位于国内，域名请求会暴露给国内 DNS",
                r.address
            ));
        }
        // 物理网卡指向内网路由器 —— TUN 开着时这就是防泄漏边界的缺口。
        if r.is_private
            && r.from_adapter
            && !r
                .interface
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains("tun")
        {
            findings.push(format!(
                "{} 的 DNS 指向内网地址 {}，隧道开启时可能绕过防泄漏边界",
                r.interface.as_deref().unwrap_or("物理网卡"),
                r.address
            ));
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn res(addr: &str, domestic: bool, from_adapter: bool, iface: Option<&str>) -> Resolver {
        Resolver {
            is_private: is_private(addr),
            is_domestic: domestic,
            address: addr.into(),
            country_code: domestic.then(|| "CN".into()),
            country_name: None,
            asn: None,
            from_adapter,
            interface: iface.map(String::from),
        }
    }

    #[test]
    fn recognises_rfc1918() {
        assert!(is_private("192.168.1.1"));
        assert!(is_private("10.0.0.1"));
        assert!(is_private("172.18.0.2"));
        assert!(!is_private("1.1.1.1"));
        assert!(!is_private("172.253.91.149"));
    }

    #[test]
    fn all_foreign_resolvers_pass() {
        let r = vec![
            res("1.1.1.1", false, false, None),
            res("8.8.8.8", false, false, None),
        ];
        assert!(evaluate(&r).is_empty());
    }

    #[test]
    fn domestic_resolver_is_flagged() {
        let r = vec![res("114.114.114.114", true, false, None)];
        let f = evaluate(&r);
        assert_eq!(f.len(), 1);
        assert!(f[0].contains("国内"));
    }

    #[test]
    fn physical_nic_pointing_at_router_is_flagged() {
        let r = vec![res("192.168.1.1", false, true, Some("以太网"))];
        assert_eq!(evaluate(&r).len(), 1);
    }

    #[test]
    fn tun_adapter_private_dns_is_fine() {
        // TUN 网卡上的 172.18.0.2 是合成地址，属于正常工作状态，不该报。
        let r = vec![res("172.18.0.2", false, true, Some("xray_tun"))];
        assert!(evaluate(&r).is_empty());
    }

    #[test]
    fn empty_result_is_not_a_pass() {
        let f = evaluate(&[]);
        assert_eq!(f.len(), 1, "读不到解析器要说无法判定，不能当通过");
    }
}
