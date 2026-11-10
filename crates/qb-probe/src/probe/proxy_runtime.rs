//! Read-only evidence from an explicitly selected local Mihomo controller.
//! The protocol, domain list and rule-checking approach were reviewed against
//! Claude Switcher v1.7.1 (cb6a72b255fde20f44d5157d5d6538c759458ae5),
//! core/claude-switch.sh, MIT, Copyright (c) 2026 Claude Switcher Contributors.
//! See the repository's third-party notices for the retained MIT license.
//! This Rust implementation checks ordered rule semantics, not a fixed prefix.
//! No packets are sent to the checked domains and no controller secret is stored.

use crate::error::{GateError, Result};
use reqwest::{header, Client, Url};
use serde::Serialize;
use serde_json::Value;
use std::{collections::HashSet, net::SocketAddr, time::Duration};
use ts_rs::TS;

const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_RULES: usize = 20_000;
const DOMAINS: &[&str] = &[
    "claude.ai",
    "api.anthropic.com",
    "anthropic.com",
    "www.anthropic.com",
    "claude.com",
    "claudeusercontent.com",
    "challenges.cloudflare.com",
    "datadoghq.com",
    "segment.io",
    "stripe.com",
    "stripe.network",
    "stripecdn.com",
    "intercom.io",
    "intercomcdn.com",
    "intercomassets.com",
];
const UDP_PORTS: &[u16] = &[3478, 5349, 19302];
const SCOPE: &str = "仅核对所选本机控制接口当前报告的规则与节点；域名项为列出的主机 TCP/443，UDP 项为常见 STUN 端口。不验证真实流量、浏览器行为、全部子域名或其他 UDP 端口，也不作为门禁放行依据。";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RuntimeVerdict {
    Verified,
    Risk,
    Unknown,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RuntimeCheck {
    pub target: String,
    pub transport: String,
    pub verdict: RuntimeVerdict,
    /// One-based index in the running kernel's ordered rules; absent in global mode.
    pub rule_index: Option<u32>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RuntimeReport {
    pub mode: String,
    pub verdict: RuntimeVerdict,
    pub checked_at: String,
    pub checks: Vec<RuntimeCheck>,
    pub scope: String,
}

fn error(message: &str) -> GateError {
    GateError::Other(message.to_owned())
}

/// Only literal loopback IPs with an explicit port are accepted. In particular,
/// localhost names, credentials, paths, query strings and remote IPs are rejected.
fn endpoint(raw: &str) -> Result<Url> {
    let raw = raw.trim();
    let invalid = || {
        error("请填写本机控制接口的完整地址，例如 http://127.0.0.1:9090；必须使用回环 IP 和明确端口，不能含账号、路径或查询参数。")
    };
    if raw.len() > 256 {
        return Err(invalid());
    }
    let (scheme, authority) = raw.split_once("://").ok_or_else(invalid)?;
    if !matches!(scheme, "http" | "https") {
        return Err(invalid());
    }
    let address = authority
        .strip_suffix('/')
        .unwrap_or(authority)
        .parse::<SocketAddr>()
        .map_err(|_| invalid())?;
    if !address.ip().is_loopback() || address.port() == 0 {
        return Err(invalid());
    }
    let url = Url::parse(raw).map_err(|_| invalid())?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(url)
}

fn authorization(secret: Option<&str>) -> Result<Option<header::HeaderValue>> {
    let Some(secret) = secret.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if secret.len() > 4096 || secret.chars().any(char::is_control) {
        return Err(error("控制接口密钥格式无效。"));
    }
    let mut value = header::HeaderValue::from_str(&format!("Bearer {secret}"))
        .map_err(|_| error("控制接口密钥格式无效。"))?;
    value.set_sensitive(true);
    Ok(Some(value))
}

async fn read_json(client: &Client, base: &Url, path: &str) -> Result<Value> {
    let mut url = base.clone();
    url.set_path(path);
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| error("无法读取本机控制接口：连接失败、证书验证失败或请求超时。"))?;
    match response.status().as_u16() {
        200..=299 => {}
        401 | 403 => return Err(error("本机控制接口拒绝访问，请核对本次填写的密钥。")),
        300..=399 => return Err(error("本机控制接口返回重定向，已拒绝跟随。")),
        _ => return Err(error("本机控制接口未返回成功响应，无法验证运行规则。")),
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BODY as u64)
    {
        return Err(error("本机控制接口响应过大，已停止读取。"));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| error("本机控制接口响应读取失败或超时。"))?
    {
        if chunk.len() > MAX_BODY.saturating_sub(body.len()) {
            return Err(error("本机控制接口响应过大，已停止读取。"));
        }
        body.extend_from_slice(&chunk);
    }
    let value: Value =
        serde_json::from_slice(&body).map_err(|_| error("本机控制接口返回的数据格式无法识别。"))?;
    if !value.is_object() {
        return Err(error("本机控制接口返回的数据格式无法识别。"));
    }
    Ok(value)
}

/// Read the selected controller only. Never discovers ports, saves credentials,
/// changes rules, inherits system proxies or follows a redirect to another host.
pub async fn inspect(endpoint_text: &str, secret: Option<&str>) -> Result<RuntimeReport> {
    let base = endpoint(endpoint_text)?;
    let mut headers = header::HeaderMap::new();
    if let Some(value) = authorization(secret)? {
        headers.insert(header::AUTHORIZATION, value);
    }
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .default_headers(headers)
        .build()
        .map_err(|_| error("无法创建本机控制接口检查请求。"))?;
    let query = async {
        let configs = read_json(&client, &base, "/configs").await?;
        let mode = configs.get("mode").and_then(Value::as_str).unwrap_or("");
        let (rules, proxies) = match mode.to_ascii_lowercase().as_str() {
            "rule" => (
                read_json(&client, &base, "/rules").await?,
                read_json(&client, &base, "/proxies").await?,
            ),
            "global" => (Value::Null, read_json(&client, &base, "/proxies").await?),
            _ => (Value::Null, Value::Null),
        };
        Ok(assess(&configs, &rules, &proxies))
    };
    tokio::time::timeout(Duration::from_secs(12), query)
        .await
        .map_err(|_| error("本机运行规则检查超时，未得出验证结论。"))?
}

#[derive(Clone, Copy)]
struct Destination<'a> {
    host: Option<&'a str>,
    udp: bool,
    port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Matches {
    Yes,
    No,
    Unknown,
}

impl From<bool> for Matches {
    fn from(value: bool) -> Self {
        if value {
            Self::Yes
        } else {
            Self::No
        }
    }
}

fn normalize(kind: &str) -> String {
    kind.chars()
        .filter(|c| !matches!(c, '-' | '_'))
        .flat_map(char::to_uppercase)
        .collect()
}

// Split only at the outer level. Unknown/malformed expressions must never skip
// an earlier rule and accidentally certify the later rule as the effective one.
fn parts(expression: &str) -> Option<Vec<&str>> {
    if expression.len() > 8192 {
        return None;
    }
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut result = Vec::new();
    for (i, c) in expression.char_indices() {
        match c {
            '(' => {
                depth += 1;
                if depth > 32 {
                    return None;
                }
            }
            ')' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                result.push(expression[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    result.push(expression[start..].trim());
    (!result.iter().any(|s| s.is_empty())).then_some(result)
}

fn unwrap_parentheses(mut text: &str) -> &str {
    text = text.trim();
    while text.starts_with('(') && text.ends_with(')') {
        let inside = &text[1..text.len() - 1];
        if parts(inside).is_none() {
            break;
        }
        text = inside.trim();
    }
    text
}

fn port_matches(payload: &str, port: u16) -> Matches {
    let mut found = false;
    for range in payload.split('/') {
        let pair = if let Some((low, high)) = range.split_once('-') {
            low.parse::<u16>().ok().zip(high.parse::<u16>().ok())
        } else {
            range.parse::<u16>().ok().map(|n| (n, n))
        };
        let Some((low, high)) = pair.filter(|(low, high)| low <= high && *low > 0) else {
            return Matches::Unknown;
        };
        found |= (low..=high).contains(&port);
    }
    found.into()
}

fn expression_matches(expression: &str, dst: Destination<'_>, depth: usize) -> Matches {
    let expression = unwrap_parentheses(expression);
    let Some((kind, payload)) = expression.split_once(',') else {
        return Matches::Unknown;
    };
    condition_matches(kind, payload, dst, depth + 1)
}

fn condition_matches(kind: &str, payload: &str, dst: Destination<'_>, depth: usize) -> Matches {
    if depth > 16 || payload.len() > 8192 {
        return Matches::Unknown;
    }
    let payload = payload.trim();
    match normalize(kind).as_str() {
        "MATCH" => Matches::Yes,
        "NETWORK" => match payload.to_ascii_lowercase().as_str() {
            "tcp" => (!dst.udp).into(),
            "udp" => dst.udp.into(),
            _ => Matches::Unknown,
        },
        "DSTPORT" => port_matches(payload, dst.port),
        "DOMAIN" | "DOMAINSUFFIX" | "DOMAINKEYWORD" => {
            let Some(host) = dst.host else {
                return Matches::Unknown;
            };
            let pattern = payload.to_ascii_lowercase();
            if pattern.is_empty()
                || !pattern
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-'))
            {
                return Matches::Unknown;
            }
            match normalize(kind).as_str() {
                "DOMAIN" => (host == pattern).into(),
                "DOMAINSUFFIX" => {
                    (host == pattern || host.ends_with(&format!(".{pattern}"))).into()
                }
                _ => host.contains(&pattern).into(),
            }
        }
        "AND" | "OR" => {
            let Some(children) = parts(unwrap_parentheses(payload)) else {
                return Matches::Unknown;
            };
            if children.len() < 2 {
                return Matches::Unknown;
            }
            let is_and = normalize(kind) == "AND";
            let mut unknown = false;
            for child in children {
                match expression_matches(child, dst, depth + 1) {
                    Matches::No if is_and => return Matches::No,
                    Matches::Yes if !is_and => return Matches::Yes,
                    Matches::Unknown => unknown = true,
                    _ => {}
                }
            }
            if unknown {
                Matches::Unknown
            } else {
                is_and.into()
            }
        }
        "NOT" => match expression_matches(payload, dst, depth + 1) {
            Matches::Yes => Matches::No,
            Matches::No => Matches::Yes,
            Matches::Unknown => Matches::Unknown,
        },
        // IP/CIDR/GEOIP, rule providers, process/source/DSCP rules and other
        // extensions need information absent from this read-only snapshot.
        _ => Matches::Unknown,
    }
}

fn node_verdict(proxies: &Value, initial: &str, udp: bool) -> (RuntimeVerdict, &'static str) {
    let mut name = initial;
    let mut seen = HashSet::new();
    for _ in 0..64 {
        if !seen.insert(name) {
            return (
                RuntimeVerdict::Unknown,
                "所选代理组存在循环引用，无法确认最终节点。",
            );
        }
        match name {
            "DIRECT" => return (RuntimeVerdict::Risk, "有效路径指向直连。"),
            "REJECT" | "REJECT-DROP" | "REJECTDROP" => {
                return if udp {
                    (
                        RuntimeVerdict::Verified,
                        "有效路径显式拒绝此 UDP 端口；这不是全部 UDP 已被阻断的证明。",
                    )
                } else {
                    (
                        RuntimeVerdict::Risk,
                        "有效路径拒绝连接，此域名无法通过该路径访问。",
                    )
                };
            }
            "PASS" => {
                return (
                    RuntimeVerdict::Unknown,
                    "所选节点为继续匹配，无法确认最终路径。",
                )
            }
            _ => {}
        }
        let Some(node) = proxies
            .get("proxies")
            .and_then(|p| p.get(name))
            .filter(|v| v.is_object())
        else {
            return (
                RuntimeVerdict::Unknown,
                "规则引用的节点不存在或控制接口没有返回该节点。",
            );
        };
        let kind = node.get("type").and_then(Value::as_str).unwrap_or("");
        match normalize(kind).as_str() {
            "DIRECT" => return (RuntimeVerdict::Risk, "所选代理组最终指向直连节点。"),
            "REJECT" | "REJECTDROP" => {
                return if udp {
                    (RuntimeVerdict::Verified, "所选代理组最终拒绝此 UDP 端口。")
                } else {
                    (RuntimeVerdict::Risk, "所选代理组最终拒绝连接。")
                };
            }
            "PASS" => {
                return (
                    RuntimeVerdict::Unknown,
                    "所选代理组最终继续匹配，路径未确认。",
                )
            }
            _ => {}
        }
        if udp {
            // A group's disable flag takes precedence over the selected leaf's
            // capability. Check every level, including nested selectors.
            for field in ["disable-udp", "disable_udp", "disableUdp"] {
                match node.get(field) {
                    Some(Value::Bool(true)) => {
                        return (
                            RuntimeVerdict::Unknown,
                            "有效路径中的节点或代理组禁用了 UDP，不能按最终节点的能力判为已验证。",
                        );
                    }
                    Some(Value::Bool(false)) | None => {}
                    _ => {
                        return (
                            RuntimeVerdict::Unknown,
                            "有效路径中的 UDP 禁用设置格式无法识别。",
                        );
                    }
                }
            }
        }
        if let Some(all) = node.get("all") {
            if !matches!(
                normalize(kind).as_str(),
                "SELECTOR" | "URLTEST" | "FALLBACK"
            ) {
                return (
                    RuntimeVerdict::Unknown,
                    "该代理组按连接动态选择或串联节点，无法由单个当前节点确认全部路径。",
                );
            }
            // Mihomo reports effective UDP capability on proxy groups too.
            // An omitted or malformed capability is missing evidence, not an
            // implicit inheritance guarantee; TCP checks remain unaffected.
            if udp && node.get("udp").and_then(Value::as_bool) != Some(true) {
                return (RuntimeVerdict::Unknown, "有效路径中的代理组未明确报告支持 UDP；最终节点支持 UDP 不能替代该组的能力证据。");
            }
            let Some(selected) = node.get("now").and_then(Value::as_str) else {
                return (
                    RuntimeVerdict::Unknown,
                    "代理组没有可确认的当前节点；链式或负载均衡组需要进一步验证。",
                );
            };
            if !all
                .as_array()
                .is_some_and(|values| values.iter().any(|v| v.as_str() == Some(selected)))
            {
                return (
                    RuntimeVerdict::Unknown,
                    "代理组当前节点不在成员清单中，运行信息不一致。",
                );
            }
            name = selected;
            continue;
        }
        if !matches!(
            normalize(kind).as_str(),
            "SS" | "SHADOWSOCKS"
                | "SSR"
                | "SHADOWSOCKSR"
                | "VMESS"
                | "VLESS"
                | "TROJAN"
                | "HYSTERIA"
                | "HYSTERIA2"
                | "TUIC"
                | "WIREGUARD"
                | "SOCKS5"
                | "HTTP"
                | "SSH"
                | "SNELL"
                | "MIERU"
                | "ANYTLS"
        ) {
            return (
                RuntimeVerdict::Unknown,
                "节点类型或代理组结构无法识别，未确认代理路径。",
            );
        }
        if udp && node.get("udp").and_then(Value::as_bool) != Some(true) {
            return (
                RuntimeVerdict::Unknown,
                "最终代理节点未明确报告支持 UDP，无法验证该端口的代理能力。",
            );
        }
        return (
            RuntimeVerdict::Verified,
            "有效规则指向代理节点；这里只验证运行配置，不证明实际出站流量。",
        );
    }
    (RuntimeVerdict::Unknown, "代理组引用层数过多，已停止解析。")
}

fn rule_verdict(
    rules: &Value,
    proxies: &Value,
    dst: Destination<'_>,
) -> (RuntimeVerdict, Option<u32>, &'static str) {
    let Some(rules) = rules
        .get("rules")
        .and_then(Value::as_array)
        .filter(|rules| rules.len() <= MAX_RULES)
    else {
        return (
            RuntimeVerdict::Unknown,
            None,
            "运行规则列表缺失、格式异常或条目过多。",
        );
    };
    for (index, rule) in rules.iter().enumerate() {
        let index = Some(index as u32 + 1);
        let disabled = rule
            .get("extra")
            .and_then(|v| v.get("disabled"))
            .or_else(|| rule.get("disabled"));
        match disabled {
            Some(Value::Bool(true)) => continue,
            Some(Value::Bool(false)) | None => {}
            _ => {
                return (
                    RuntimeVerdict::Unknown,
                    index,
                    "前序规则的启用状态无法识别。",
                )
            }
        }
        let kind = rule.get("type").and_then(Value::as_str).unwrap_or("");
        let payload = rule.get("payload").and_then(Value::as_str).unwrap_or("");
        match condition_matches(kind, payload, dst, 0) {
            Matches::No => continue,
            Matches::Unknown => {
                return (
                    RuntimeVerdict::Unknown,
                    index,
                    "前序规则依赖地址、进程、规则集或未支持的条件，无法确认后续规则是否实际生效。",
                )
            }
            Matches::Yes => {}
        }
        let Some(target) = rule.get("proxy").and_then(Value::as_str) else {
            return (
                RuntimeVerdict::Unknown,
                index,
                "命中规则缺少可识别的目标节点。",
            );
        };
        if target == "PASS" {
            continue;
        }
        let (verdict, detail) = node_verdict(proxies, target, dst.udp);
        return (verdict, index, detail);
    }
    (
        RuntimeVerdict::Unknown,
        None,
        "没有找到可确定的有效匹配规则；不推断默认出站行为。",
    )
}

fn assess(configs: &Value, rules: &Value, proxies: &Value) -> RuntimeReport {
    let mode = match configs
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "rule" => "rule",
        "global" => "global",
        "direct" => "direct",
        _ => "unknown",
    };
    let destinations = DOMAINS
        .iter()
        .map(|host| Destination {
            host: Some(host),
            udp: false,
            port: 443,
        })
        .chain(UDP_PORTS.iter().map(|port| Destination {
            host: None,
            udp: true,
            port: *port,
        }));
    let checks: Vec<_> = destinations
        .map(|dst| {
            let (verdict, rule_index, detail) = match mode {
                "rule" => rule_verdict(rules, proxies, dst),
                "global" => {
                    let (verdict, detail) = node_verdict(proxies, "GLOBAL", dst.udp);
                    (verdict, None, detail)
                }
                "direct" => (
                    RuntimeVerdict::Risk,
                    None,
                    "控制接口报告为直连模式，置顶规则不参与匹配。",
                ),
                _ => (RuntimeVerdict::Unknown, None, "无法识别当前运行模式。"),
            };
            RuntimeCheck {
                target: dst
                    .host
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("UDP/{}（任意目标）", dst.port)),
                transport: format!("{}/{}", if dst.udp { "UDP" } else { "TCP" }, dst.port),
                verdict,
                rule_index,
                detail: detail.to_owned(),
            }
        })
        .collect();
    let verdict = if checks
        .iter()
        .any(|check| check.verdict == RuntimeVerdict::Risk)
    {
        RuntimeVerdict::Risk
    } else if checks
        .iter()
        .any(|check| check.verdict == RuntimeVerdict::Unknown)
    {
        RuntimeVerdict::Unknown
    } else {
        RuntimeVerdict::Verified
    };
    RuntimeReport {
        mode: mode.to_owned(),
        verdict,
        checked_at: chrono::Utc::now().to_rfc3339(),
        checks,
        scope: SCOPE.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    fn proxies() -> Value {
        json!({"proxies": {
            "choice": {"type":"Selector", "all":["node"], "now":"node", "udp":true},
            "node": {"type":"Shadowsocks", "udp":true}
        }})
    }

    fn known_proxies() -> Value {
        proxies()
    }

    fn web() -> Destination<'static> {
        Destination {
            host: Some("claude.ai"),
            udp: false,
            port: 443,
        }
    }

    fn evaluate(
        rules: Value,
        proxies: Value,
        dst: Destination<'_>,
    ) -> (RuntimeVerdict, Option<u32>, &'static str) {
        rule_verdict(&json!({"rules": rules}), &proxies, dst)
    }

    #[test]
    fn loopback_addresses_require_explicit_port_and_no_credentials_or_path() {
        for address in [
            "http://127.0.0.1:9090",
            "https://[::1]:9090/",
            "http://127.0.0.2:80",
        ] {
            assert!(endpoint(address).is_ok(), "{address}");
        }
        for address in [
            "http://localhost:9090",
            "http://example.test:9090",
            "http://192.168.1.1:9090",
            "http://127.0.0.1",
            "http://127.0.0.1:0",
            "http://user:secret@127.0.0.1:9090",
            "http://127.0.0.1:9090/rules",
            "http://127.0.0.1:9090/?secret=test",
            "http://127.0.0.1:9090/#secret",
            "file:///tmp/controller",
            "http://2130706433:9090",
        ] {
            assert!(endpoint(address).is_err(), "{address}");
        }
        assert!(authorization(Some("secret\r\nInjected: yes")).is_err());
        assert!(authorization(Some("example-secret"))
            .unwrap()
            .unwrap()
            .is_sensitive());
    }

    #[test]
    fn effective_first_rule_wins_instead_of_domain_presence() {
        let report = evaluate(
            json!([
                {"type":"MATCH", "proxy":"DIRECT"},
                {"type":"DOMAIN-SUFFIX", "payload":"claude.ai", "proxy":"choice"}
            ]),
            known_proxies(),
            web(),
        );
        assert_eq!((report.0, report.1), (RuntimeVerdict::Risk, Some(1)));
    }

    #[test]
    fn unrelated_and_disabled_rules_do_not_require_fixed_prefix() {
        let mut rules =
            vec![json!({"type":"DOMAIN", "payload":"unrelated.example", "proxy":"DIRECT"}); 40];
        rules.push(json!({"type":"MATCH", "proxy":"DIRECT", "extra":{"disabled":true}}));
        rules.push(json!({"type":"DOMAIN-SUFFIX", "payload":"claude.ai", "proxy":"choice"}));
        let report = evaluate(Value::Array(rules), known_proxies(), web());
        assert_eq!((report.0, report.1), (RuntimeVerdict::Verified, Some(42)));
    }

    #[test]
    fn earlier_unresolved_rules_prevent_false_pass() {
        for kind in ["RuleSet", "GeoIP", "IPCIDR", "ProcessName", "Unrecognized"] {
            let report = evaluate(
                json!([
                    {"type":kind, "payload":"some-condition", "proxy":"DIRECT"},
                    {"type":"MATCH", "proxy":"choice"}
                ]),
                known_proxies(),
                web(),
            );
            assert_eq!((report.0, report.1), (RuntimeVerdict::Unknown, Some(1)));
        }
    }

    #[test]
    fn domain_suffix_matches_boundaries_and_requires_the_real_target() {
        assert_eq!(
            condition_matches("DOMAIN-SUFFIX", "laude.ai", web(), 0),
            Matches::No
        );
        let report = evaluate(
            json!([{"type":"DOMAIN-SUFFIX", "payload":"claude.ai", "proxy":"DIRECT"}]),
            known_proxies(),
            web(),
        );
        assert_eq!(report.0, RuntimeVerdict::Risk);
        assert_eq!(
            condition_matches(
                "DOMAIN-SUFFIX",
                "anthropic.com",
                Destination {
                    host: Some("api.anthropic.com"),
                    ..web()
                },
                0
            ),
            Matches::Yes
        );
    }

    #[test]
    fn logical_conditions_use_three_states_and_preserve_protocol_and_port() {
        let udp = Destination {
            host: None,
            udp: true,
            port: 3478,
        };
        assert_eq!(
            condition_matches("AND", "((NETWORK,UDP),(DST-PORT,3478))", udp, 0),
            Matches::Yes
        );
        assert_eq!(
            condition_matches("AND", "((NETWORK,UDP),(RULE-SET,private))", web(), 0),
            Matches::No
        );
        assert_eq!(
            condition_matches("AND", "((NETWORK,TCP),(RULE-SET,private))", web(), 0),
            Matches::Unknown
        );
        assert_eq!(
            condition_matches("OR", "((NETWORK,TCP),(RULE-SET,private))", web(), 0),
            Matches::Yes
        );
        assert_eq!(
            condition_matches("NOT", "(NETWORK,UDP)", web(), 0),
            Matches::Yes
        );
        assert_eq!(
            condition_matches("AND", "((NETWORK,UDP),(DST-PORT,3478)", udp, 0),
            Matches::Unknown
        );
        assert_eq!(port_matches("80/443/1000-2000", 443), Matches::Yes);
        assert_eq!(port_matches("34780", 3478), Matches::No);
    }

    #[test]
    fn udp_rejection_is_distinguished_from_proxy_support() {
        let udp = Destination {
            host: None,
            udp: true,
            port: 3478,
        };
        let rules =
            json!([{"type":"AND", "payload":"((NETWORK,UDP),(DST-PORT,3478))", "proxy":"REJECT"}]);
        assert_eq!(
            evaluate(rules, known_proxies(), udp).0,
            RuntimeVerdict::Verified
        );
        let mut p = known_proxies();
        p["proxies"]["node"]["udp"] = json!(false);
        assert_eq!(
            evaluate(json!([{"type":"MATCH", "proxy":"choice"}]), p, udp).0,
            RuntimeVerdict::Unknown
        );
    }

    #[test]
    fn nested_groups_reject_direct_cycles_missing_and_inconsistent_members() {
        let mut p = known_proxies();
        p["proxies"]["choice"]["all"] = json!(["DIRECT"]);
        p["proxies"]["choice"]["now"] = json!("DIRECT");
        assert_eq!(node_verdict(&p, "choice", false).0, RuntimeVerdict::Risk);
        p["proxies"]["choice"]["all"] = json!(["choice"]);
        p["proxies"]["choice"]["now"] = json!("choice");
        assert_eq!(node_verdict(&p, "choice", false).0, RuntimeVerdict::Unknown);
        assert_eq!(
            node_verdict(&p, "missing", false).0,
            RuntimeVerdict::Unknown
        );
        p["proxies"]["choice"]["now"] = json!("node");
        assert_eq!(node_verdict(&p, "choice", false).0, RuntimeVerdict::Unknown);
    }

    #[test]
    fn every_group_can_veto_udp_even_when_the_leaf_supports_it() {
        let mut p = known_proxies();
        p["proxies"]["choice"]["udp"] = json!(false);
        assert_eq!(node_verdict(&p, "choice", true).0, RuntimeVerdict::Unknown);
        assert_eq!(
            node_verdict(&p, "choice", false).0,
            RuntimeVerdict::Verified
        );

        p["proxies"]["choice"] =
            json!({"type":"Selector", "all":["nested"], "now":"nested", "udp":true});
        p["proxies"]["nested"] =
            json!({"type":"Fallback", "all":["node"], "now":"node", "udp":false});
        assert_eq!(node_verdict(&p, "choice", true).0, RuntimeVerdict::Unknown);
        p["proxies"]["nested"]["udp"] = json!(true);
        assert_eq!(node_verdict(&p, "choice", true).0, RuntimeVerdict::Verified);
        for field in ["disable-udp", "disable_udp", "disableUdp"] {
            p["proxies"]["nested"][field] = json!(true);
            assert_eq!(node_verdict(&p, "choice", true).0, RuntimeVerdict::Unknown);
            p["proxies"]["nested"]
                .as_object_mut()
                .unwrap()
                .remove(field);
        }
    }

    #[test]
    fn missing_group_udp_capability_is_unknown_without_affecting_tcp() {
        let mut p = known_proxies();
        p["proxies"]["choice"]
            .as_object_mut()
            .unwrap()
            .remove("udp");
        assert_eq!(node_verdict(&p, "choice", true).0, RuntimeVerdict::Unknown);
        assert_eq!(
            node_verdict(&p, "choice", false).0,
            RuntimeVerdict::Verified
        );
        p["proxies"]["choice"]["udp"] = json!("true");
        assert_eq!(node_verdict(&p, "choice", true).0, RuntimeVerdict::Unknown);
    }

    #[test]
    fn global_mode_checks_selected_node_and_direct_mode_cannot_pass() {
        let mut p = known_proxies();
        p["proxies"]["GLOBAL"] =
            json!({"type":"Selector", "all":["choice"], "now":"choice", "udp":true});
        assert_eq!(
            assess(&json!({"mode":"global"}), &Value::Null, &p).verdict,
            RuntimeVerdict::Verified
        );
        p["proxies"]["GLOBAL"] =
            json!({"type":"Selector", "all":["DIRECT"], "now":"DIRECT", "udp":true});
        assert_eq!(
            assess(&json!({"mode":"global"}), &Value::Null, &p).verdict,
            RuntimeVerdict::Risk
        );
        assert_eq!(
            assess(&json!({"mode":"direct"}), &Value::Null, &Value::Null).verdict,
            RuntimeVerdict::Risk
        );
        assert_eq!(
            assess(
                &json!({"mode":"secret-unrecognized"}),
                &Value::Null,
                &Value::Null
            )
            .mode,
            "unknown"
        );
    }

    #[test]
    fn missing_rules_or_unsupported_node_types_remain_unknown() {
        assert_eq!(
            rule_verdict(&json!({}), &known_proxies(), web()).0,
            RuntimeVerdict::Unknown
        );
        assert_eq!(
            evaluate(json!([]), known_proxies(), web()).0,
            RuntimeVerdict::Unknown
        );
        let p = json!({"proxies":{"node":{"type":"FutureProxy", "udp":true}}});
        assert_eq!(node_verdict(&p, "node", false).0, RuntimeVerdict::Unknown);
    }

    async fn mock(responses: Vec<String>) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for response in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buffer = [0; 1024];
                    let count = stream.read(&mut buffer).await.unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..count]);
                    if request.windows(4).any(|v| v == b"\r\n\r\n") {
                        break;
                    }
                }
                requests.push(String::from_utf8(request).unwrap());
                let _ = stream.write_all(response.as_bytes()).await;
            }
            requests
        });
        (address, task)
    }

    fn response(value: Value) -> String {
        let body = value.to_string();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[tokio::test]
    async fn reads_only_expected_paths_with_ephemeral_auth_and_redacted_report() {
        let (address, server) = mock(vec![
            response(json!({"mode":"rule"})),
            response(json!({"rules":[{"type":"MATCH", "proxy":"choice"}]})),
            response(known_proxies()),
        ])
        .await;
        let report = inspect(&address, Some("example-controller-secret"))
            .await
            .unwrap();
        assert_eq!(report.verdict, RuntimeVerdict::Verified);
        let text = serde_json::to_string(&report).unwrap();
        assert!(!text.contains("example-controller-secret"));
        assert!(!text.contains(&address));
        for (request, path) in server
            .await
            .unwrap()
            .iter()
            .zip(["/configs", "/rules", "/proxies"])
        {
            assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer example-controller-secret"));
        }
    }

    #[tokio::test]
    async fn redirects_auth_failures_and_invalid_json_are_sanitized() {
        for reply in [
            "HTTP/1.1 302 Found\r\nLocation: http://192.0.2.1/private-secret\r\nContent-Length: 0\r\n\r\n".to_owned(),
            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".to_owned(),
            "HTTP/1.1 200 OK\r\nContent-Length: 14\r\n\r\nprivate-secret".to_owned(),
            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", MAX_BODY + 1),
        ] {
            let (address, server) = mock(vec![reply]).await;
            let message = inspect(&address, Some("private-secret")).await.unwrap_err().to_string();
            assert!(!message.contains("private-secret"));
            assert!(!message.contains(&address));
            assert_eq!(server.await.unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn streaming_body_limit_applies_without_content_length() {
        let body = "a".repeat(MAX_BODY + 1);
        let reply = format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n", body.len());
        let (address, server) = mock(vec![reply]).await;
        assert!(inspect(&address, None)
            .await
            .unwrap_err()
            .to_string()
            .contains("响应过大"));
        server.await.unwrap();
    }
}
