//! Explicit supplementary environment checks. Results stay in the caller's
//! memory; no history, timers, account credentials, or gate decisions.
use crate::error::{GateError, Result};
use crate::probe::{asn, ip, proxy_runtime, reach, stun};
use crate::sysenv::checkup::{CheckItem, State};
use serde::Serialize;
use std::net::IpAddr;
use ts_rs::TS;

static ACTIVE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn enter() -> Result<tokio::sync::MutexGuard<'static, ()>> {
    ACTIVE
        .try_lock()
        .map_err(|_| GateError::Other("补充检测正在运行，请等本轮完成后再试。".into()))
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct EnvironmentDiagnostics {
    pub checked_at: String,
    pub scope: String,
    pub items: Vec<CheckItem>,
}

fn report(scope: impl Into<String>, items: Vec<CheckItem>) -> EnvironmentDiagnostics {
    EnvironmentDiagnostics {
        checked_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        scope: scope.into(),
        items,
    }
}

fn item(
    id: impl Into<String>,
    label: impl Into<String>,
    state: State,
    detail: impl Into<String>,
) -> CheckItem {
    CheckItem {
        id: id.into(),
        label: label.into(),
        state,
        detail: detail.into(),
        fixable: false,
        manual: None,
    }
}

fn same_family(a: IpAddr, b: IpAddr) -> bool {
    matches!(
        (a, b),
        (IpAddr::V4(_), IpAddr::V4(_)) | (IpAddr::V6(_), IpAddr::V6(_))
    )
}

fn normalize_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
        _ => ip,
    }
}

fn usable_mapping(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => {
            let [first, second, third, _] = a.octets();
            !a.is_private()
                && !a.is_loopback()
                && !a.is_link_local()
                && !a.is_unspecified()
                && !a.is_multicast()
                && !a.is_broadcast()
                && first != 0
                && first < 224
                && !(first == 100 && (64..=127).contains(&second))
                && !(first == 198 && (18..=19).contains(&second))
                && !(first == 192 && second == 0 && (third == 0 || third == 2))
                && !(first == 192 && second == 88 && third == 99)
                && !(first == 198 && second == 51 && third == 100)
                && !(first == 203 && second == 0 && third == 113)
        }
        IpAddr::V6(a) => {
            !a.is_loopback()
                && !a.is_unspecified()
                && !a.is_multicast()
                && !a.is_unique_local()
                && !a.is_unicast_link_local()
                && (a.segments()[0] & 0xe000) == 0x2000
                && !(a.segments()[0] == 0x2001 && a.segments()[1] == 0x0db8)
        }
    }
}

fn append_v6_evidence(exits: &mut Vec<(String, String)>, v6: reach::V6Exit) {
    match v6 {
        reach::V6Exit::Exit { ip, .. } => exits.push(("IPv6 HTTP".into(), ip)),
        reach::V6Exit::ViaProxy { ip } => exits.push(("IPv6-only HTTP 经代理返回 IPv4".into(), ip)),
        reach::V6Exit::None => {}
    }
}

fn compare_udp(observed: &stun::StunObservation, exits: &[(String, String)]) -> (State, String) {
    let Some(address) = observed
        .mapped_address
        .as_deref()
        .and_then(|s| s.parse::<IpAddr>().ok())
        .map(normalize_ip)
    else {
        return (
            State::Unknown,
            observed
                .detail
                .clone()
                .unwrap_or_else(|| "没有得到有效 UDP 映射地址；无响应不等于没有暴露。".into()),
        );
    };
    if observed.status != stun::StunStatus::Completed {
        return (State::Unknown, "采样未完成，不能给出一致性结论。".into());
    }
    if !usable_mapping(address) {
        return (
            State::Unknown,
            "STUN 返回本地、未指定或非单播地址，不能作为公网出口证据。".into(),
        );
    }
    let mut same: Vec<IpAddr> = exits
        .iter()
        .filter_map(|(_, value)| value.parse().ok())
        .map(normalize_ip)
        .filter(|v| same_family(address, *v))
        .collect();
    same.sort();
    same.dedup();
    if same.is_empty() {
        return (
            State::Unknown,
            format!("UDP 映射为 {address}；这轮没有同地址族的 HTTP 出口，无法比较。"),
        );
    }
    if !same.contains(&address) {
        return (State::Warn, format!("UDP 映射 {address} 与本轮同地址族 HTTP 出口 {} 不同。可能是分流或不同代理出口，不能单凭差异认定为真实 IP 泄露。", same.iter().map(ToString::to_string).collect::<Vec<_>>().join("、")));
    }
    if same.len() > 1 {
        return (State::Warn, format!("UDP 映射 {address} 与部分 HTTP 观测一致，但同地址族 HTTP 来源报告了多个出口；需核对分流。"));
    }
    (
        State::Pass,
        format!("UDP 映射 {address} 与本轮同地址族 HTTP 出口一致；仅证明本次所测路径。"),
    )
}

pub async fn stun() -> Result<EnvironmentDiagnostics> {
    let _active = enter()?;
    let client = ip::build_client(true)?;
    let (udp, mut exits, v6) =
        tokio::join!(stun::probe(), ip::evidence(), reach::ipv6_exit(&client));
    append_v6_evidence(&mut exits, v6);
    let mut items = vec![item(
        "udp_coverage",
        "UDP 采样完成度",
        match udp.coverage {
            stun::StunCoverage::Complete => State::Pass,
            stun::StunCoverage::Partial => State::Warn,
            stun::StunCoverage::Unknown => State::Unknown,
        },
        format!(
            "{} / {} 项获得有效响应。未完成的地址族或节点仍为未知；完成度不代表网络安全。",
            udp.observations
                .iter()
                .filter(|o| o.status == stun::StunStatus::Completed)
                .count(),
            udp.observations.len()
        ),
    )];
    for (index, observed) in udp.observations.iter().enumerate() {
        let (state, detail) = compare_udp(observed, &exits);
        let family = match observed.family {
            stun::StunFamily::Ipv4 => "IPv4",
            stun::StunFamily::Ipv6 => "IPv6",
        };
        items.push(item(
            format!("udp_{index}"),
            format!("{} · {family}", observed.server),
            state,
            detail,
        ));
    }
    for (index, (source, address)) in exits.iter().enumerate() {
        items.push(item(
            format!("http_{index}"),
            format!("HTTP 出口 · {source}"),
            State::Pass,
            format!("本轮观测 {address}；与 UDP 按相同地址族比较。"),
        ));
    }
    Ok(report("原生 UDP 与 HTTP 出口对照。探针遵循本机路由、绕过 HTTP 系统代理；不会模拟浏览器策略。只测列出的节点，结果不用于门禁，也不写出口历史。", items))
}

pub async fn proxy(endpoint: &str, secret: Option<&str>) -> Result<EnvironmentDiagnostics> {
    let _active = enter()?;
    let result = proxy_runtime::inspect(endpoint, secret).await?;
    let items = result
        .checks
        .into_iter()
        .enumerate()
        .map(|(i, check)| {
            let state = match check.verdict {
                proxy_runtime::RuntimeVerdict::Verified => State::Pass,
                proxy_runtime::RuntimeVerdict::Risk => State::Warn,
                proxy_runtime::RuntimeVerdict::Unknown => State::Unknown,
            };
            let position = check
                .rule_index
                .map(|n| format!("运行规则第 {n} 条；"))
                .unwrap_or_default();
            item(
                format!("runtime_{i}"),
                format!("{} · {}", check.target, check.transport),
                state,
                format!("{position}{}", check.detail),
            )
        })
        .collect();
    Ok(report(
        format!("模式：{}。{}", result.mode, result.scope),
        items,
    ))
}

pub async fn asn(ip: &str) -> Result<EnvironmentDiagnostics> {
    let _active = enter()?;
    let result = asn::lookup(ip).await?;
    let state = if result.network_type.is_some() {
        State::Pass
    } else {
        State::Unknown
    };
    let asn = result
        .asn
        .map(|n| format!("AS{n}"))
        .unwrap_or_else(|| "ASN 未知".into());
    let detail = format!("{} · {asn} · {}；PeeringDB 网络类型：{}。运营商类别不能认证具体 IP 的住宅属性、信誉或可用性。", result.ip, result.organization.as_deref().unwrap_or("组织未知"), result.network_type.as_deref().unwrap_or("未确认"));
    Ok(report("仅查询本次指定 IP：IPinfo 接收该 IP，PeeringDB 接收公开 ASN。查询不携带账户凭据，结果不用于纯净度认证或门禁。", vec![item("asn_category", "ASN 网络类型", state, detail)]))
}

pub async fn locale(expected: Option<String>) -> Result<EnvironmentDiagnostics> {
    let _active = enter()?;
    let rows = tokio::task::spawn_blocking(move || {
        crate::sysenv::client_locale::inspect(expected.as_deref())
    })
    .await
    .map_err(|_| GateError::Other("客户端语言读取任务未完成。".into()))??;
    Ok(report(
        "只提取 Claude 桌面配置中的 locale，不使用登录凭据、不修改语言，也不推断正在运行的网页语言。",
        rows,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed(status: stun::StunStatus, address: Option<&str>) -> stun::StunObservation {
        stun::StunObservation {
            server: "fixture".into(),
            family: stun::StunFamily::Ipv4,
            status,
            server_address: None,
            mapped_address: address.map(str::to_string),
            mapped_port: None,
            elapsed_ms: 0,
            detail: None,
        }
    }

    #[test]
    fn missing_or_incomplete_udp_evidence_never_passes() {
        let exits = vec![("fixture".into(), "8.8.8.8".into())];
        assert_eq!(
            compare_udp(&observed(stun::StunStatus::Timeout, None), &exits).0,
            State::Unknown
        );
        assert_eq!(
            compare_udp(
                &observed(stun::StunStatus::Timeout, Some("8.8.8.8")),
                &exits
            )
            .0,
            State::Unknown
        );
        assert_eq!(
            compare_udp(
                &observed(stun::StunStatus::Completed, Some("2001:4860:4860::8888")),
                &exits
            )
            .0,
            State::Unknown
        );
    }

    #[test]
    fn same_family_split_is_reported_without_claiming_a_leak() {
        let probe = observed(stun::StunStatus::Completed, Some("8.8.8.8"));
        let mut exits = vec![
            ("one".into(), "8.8.8.8".into()),
            ("v6".into(), "2001:4860:4860::8888".into()),
        ];
        assert_eq!(compare_udp(&probe, &exits).0, State::Pass);
        exits.push(("two".into(), "8.8.4.4".into()));
        assert_eq!(compare_udp(&probe, &exits).0, State::Warn);
        let mismatch = compare_udp(
            &observed(stun::StunStatus::Completed, Some("9.9.9.9")),
            &exits,
        );
        assert_eq!(mismatch.0, State::Warn);
        assert!(mismatch.1.contains("不能单凭"));
    }

    #[test]
    fn proxy_translated_v6_endpoint_is_kept_as_ipv4_evidence() {
        let mut exits = vec![("one".into(), "8.8.8.8".into())];
        append_v6_evidence(
            &mut exits,
            reach::V6Exit::ViaProxy {
                ip: "8.8.4.4".into(),
            },
        );
        let probe = observed(stun::StunStatus::Completed, Some("8.8.8.8"));
        assert_eq!(compare_udp(&probe, &exits).0, State::Warn);
        assert!(exits[1].0.contains("经代理"));
    }

    #[test]
    fn local_mappings_are_unknown_and_mapped_ipv4_is_compared_canonically() {
        for ip in [
            "0.0.0.0",
            "127.0.0.1",
            "10.0.0.1",
            "224.0.0.1",
            "::",
            "fe80::1",
            "::ffff:127.0.0.1",
            "198.18.0.1",
            "198.19.255.255",
            "240.1.2.3",
            "192.0.2.1",
            "2001:db8::8",
        ] {
            let exits = vec![("one".into(), ip.into())];
            assert_eq!(
                compare_udp(&observed(stun::StunStatus::Completed, Some(ip)), &exits).0,
                State::Unknown
            );
        }
        let exits = vec![("one".into(), "8.8.8.8".into())];
        assert_eq!(
            compare_udp(
                &observed(stun::StunStatus::Completed, Some("::ffff:8.8.8.8")),
                &exits
            )
            .0,
            State::Pass
        );
    }
}
