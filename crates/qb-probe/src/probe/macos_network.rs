//! macOS 网络配置只读采集。系统工具使用绝对路径，不执行 shell 或 PAC 脚本。
//! 无法识别的配置保留未知，不能把工具失败或未知接口当作直连。

use std::net::IpAddr;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProxyConfig {
    pub enabled: Option<bool>,
    pub server: Option<String>,
    pub pac: Option<String>,
    pub auto_detect: Option<bool>,
    /// SOCKS、按接口分组及代理绕过规则不能由 reqwest 的系统 HTTP 代理读取完整复现。
    pub unsupported: bool,
}

/// 只取 scutil 顶层设置；嵌套的按接口设置不能冒充全局代理。
pub fn parse_proxy(text: &str) -> Option<ProxyConfig> {
    if !text.trim_start().starts_with("<dictionary> {") || !text.trim_end().ends_with('}') {
        return None;
    }
    let mut values = std::collections::BTreeMap::new();
    let mut depth = 0usize;
    let mut scoped = false;
    let mut exceptions = false;
    let mut in_exceptions = false;
    for line in text.lines() {
        let line = line.trim();
        if depth == 1 {
            if let Some((key, value)) = line.split_once(" : ") {
                if matches!(key, "__SCOPED__" | "__SUPPLEMENTAL__") {
                    scoped = true;
                }
                if key == "ExceptionsList" {
                    if value != "<array> {" {
                        return None;
                    }
                    in_exceptions = true;
                }
                if !value.ends_with('{') {
                    values.insert(key, value);
                }
            }
        }
        if in_exceptions && depth == 2 {
            if line == "}" {
                in_exceptions = false;
            } else {
                let (_, value) = line.split_once(':')?;
                exceptions |= !value.trim().is_empty();
            }
        }
        if line.ends_with('{') {
            depth += 1;
        } else if line == "}" {
            depth = depth.checked_sub(1)?;
        }
    }
    if depth != 0 {
        return None;
    }
    let flag = |key: &str| match values.get(key).copied() {
        None | Some("0") => Some(false),
        Some("1") => Some(true),
        _ => None,
    };
    let http = flag("HTTPEnable")?;
    let https = flag("HTTPSEnable")?;
    let socks = flag("SOCKSEnable")?;
    let pac_on = flag("ProxyAutoConfigEnable")?;
    let exclude_simple = flag("ExcludeSimpleHostnames")?;
    let mut servers = Vec::new();
    for (name, enabled) in [("HTTP", http), ("HTTPS", https), ("SOCKS", socks)] {
        if enabled {
            let host = values.get(format!("{name}Proxy").as_str()).copied()?;
            let port = values
                .get(format!("{name}Port").as_str())?
                .parse::<u16>()
                .ok()?;
            if host.is_empty() || port == 0 {
                return None;
            }
            servers.push(format!("{name}={host}:{port}"));
        }
    }
    Some(ProxyConfig {
        enabled: Some(http || https || socks),
        server: (!servers.is_empty()).then(|| servers.join("；")),
        // 不显示 PAC URL：路径可能含口令。启用但缺 URL 仍须阻止假比较。
        pac: pac_on.then(|| "已启用（不执行脚本）".into()),
        auto_detect: flag("ProxyAutoDiscoveryEnable"),
        // 未启用手动代理时，系统保留的默认例外清单不会改变这轮选路。
        unsupported: socks || scoped || ((http || https) && (exceptions || exclude_simple)),
    })
}

/// 与锁定的 hyper-util 的 from_env 一致：大写存在时（即使为空）优先于小写；
/// REQUEST_METHOD 存在则禁用整个代理匹配器。只返回变量名，绝不回显代理凭据。
/// 注入读取器让回归测试无需修改进程环境，避免并发测试相互污染。
pub fn proxy_environment_override(
    mut read: impl FnMut(&str) -> Option<String>,
    request_method_present: bool,
) -> Option<String> {
    if request_method_present {
        return Some(
            "当前进程存在 REQUEST_METHOD，HTTP 客户端会禁用系统代理；不能据此确认浏览器实际选路"
                .into(),
        );
    }
    let mut names = Vec::new();
    for (upper, lower) in [
        ("ALL_PROXY", "all_proxy"),
        ("HTTP_PROXY", "http_proxy"),
        ("HTTPS_PROXY", "https_proxy"),
        ("NO_PROXY", "no_proxy"),
    ] {
        let selected = read(upper)
            .map(|value| (upper, value))
            .or_else(|| read(lower).map(|value| (lower, value)));
        if let Some((name, value)) = selected {
            // 空白/空逗号项不会加入 NoProxy；HTTP(S) 非空无效值仍会阻止
            // 系统设置回填，因此不能按 trim 后为空就忽略。
            let effective = if upper == "NO_PROXY" {
                value.split(',').any(|part| !part.trim().is_empty())
            } else {
                !value.is_empty()
            };
            if effective {
                names.push(name);
            }
        }
    }
    (!names.is_empty()).then(|| format!(
        "当前进程的 {} 会影响 HTTP 探测选路，可能覆盖系统代理或绕过规则；不能据此确认浏览器实际路径",
        names.join("、")
    ))
}

/// route -n get 返回的实际出口接口；不按路由条目名猜优先级。
pub fn route_interface(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (key, value) = line.trim().split_once(':')?;
        let name = value.trim();
        (key == "interface" && valid_interface(name)).then(|| name.to_string())
    })
}

pub fn valid_interface(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// 内核接口种类来自 ifconfig 的头部标志。POINTOPOINT 是隧道；ether 还必须由
/// networksetup 的硬件设备清单佐证，不能把 TAP 等虚拟以太网直接当物理网卡。
/// 回环及无法识别的类型未知。UP 不等于默认路由接管，调用方必须先查询实际路由。
pub fn interface_tunnel(text: &str, hardware: bool) -> Option<bool> {
    let header = text.lines().next()?;
    let flags = header.split_once('<')?.1.split_once('>')?.0;
    let flags: Vec<_> = flags.split(',').collect();
    if !flags.contains(&"UP") || flags.contains(&"LOOPBACK") {
        return None;
    }
    if flags.contains(&"POINTOPOINT") {
        return Some(true);
    }
    (hardware && text.lines().any(|l| l.trim_start().starts_with("ether "))).then_some(false)
}

pub fn hardware_interfaces(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| {
            let name = line.trim().strip_prefix("Device:")?.trim();
            valid_interface(name).then_some(name)
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsServer {
    pub ip: IpAddr,
    pub interface: Option<String>,
}

/// scutil --dns 包含默认、域名补充和按接口解析器；全部保留供保守检查。
/// mDNS 没有 nameserver，不作为公网 DNS。格式损坏不降级为空清单。
pub fn parse_dns(text: &str) -> Option<Vec<DnsServer>> {
    if !text.trim_start().starts_with("DNS configuration") {
        return None;
    }
    let mut out = Vec::new();
    let mut ips = Vec::new();
    let mut interface = None;
    let flush = |out: &mut Vec<DnsServer>, ips: &mut Vec<IpAddr>, interface: &Option<String>| {
        for ip in ips.drain(..) {
            let row = DnsServer {
                ip,
                interface: interface.clone(),
            };
            if !out.contains(&row) {
                out.push(row);
            }
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("resolver #") || line.starts_with("DNS configuration") {
            flush(&mut out, &mut ips, &interface);
            interface = None;
        } else if line.starts_with("nameserver[") {
            let raw = line.split_once(':')?.1.trim();
            // IPv6 zone 属于接口标识，不属于 IP 本体。
            let (address, zone) = raw
                .split_once('%')
                .map_or((raw, None), |(a, z)| (a, Some(z)));
            ips.push(address.parse().ok()?);
            if let Some(zone) = zone.filter(|z| valid_interface(z)) {
                interface = Some(zone.to_string());
            }
        } else if line.starts_with("if_index") {
            let name = line.split_once('(')?.1.split_once(')')?.0;
            if !valid_interface(name) {
                return None;
            }
            interface = Some(name.to_string());
        }
    }
    flush(&mut out, &mut ips, &interface);
    Some(out)
}

#[cfg(target_os = "macos")]
pub fn read_command(program: &str, args: &[&str]) -> Option<String> {
    read_command_bounded(program, args, std::time::Duration::from_secs(3))
}

#[cfg(target_os = "macos")]
fn read_command_bounded(
    program: &str,
    args: &[&str],
    timeout: std::time::Duration,
) -> Option<String> {
    use std::io::Read;
    use std::process::Stdio;
    let deadline = std::time::Instant::now() + timeout;
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(1024 * 1024 + 1).read_to_end(&mut bytes);
        let _ = sender.send(result.ok().map(|_| bytes));
    });
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let bytes = receiver
                    .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                    .ok()??;
                if bytes.len() > 1024 * 1024 {
                    return None;
                }
                return String::from_utf8(bytes).ok();
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub fn proxy_config() -> Option<ProxyConfig> {
    parse_proxy(&read_command("/usr/sbin/scutil", &["--proxy"])?)
}

#[cfg(target_os = "macos")]
pub fn route(ip: IpAddr, interface: Option<&str>) -> Option<(String, bool)> {
    if ip.is_loopback() || ip.is_unspecified() {
        return None;
    }
    let address = ip.to_string();
    let mut args = vec!["-n", "get", if ip.is_ipv6() { "-inet6" } else { "-inet" }];
    if let Some(name) = interface {
        if !valid_interface(name) {
            return None;
        }
        args.extend(["-ifscope", name]);
    }
    args.push(&address);
    let name = route_interface(&read_command("/sbin/route", &args)?)?;
    let info = read_command("/sbin/ifconfig", &[&name])?;
    let hardware = if info
        .lines()
        .next()
        .is_some_and(|h| h.contains("POINTOPOINT"))
    {
        false
    } else {
        let ports = read_command("/usr/sbin/networksetup", &["-listallhardwareports"])?;
        hardware_interfaces(&ports).contains(&name.as_str())
    };
    let tunnel = interface_tunnel(&info, hardware)?;
    Some((name, tunnel))
}

/// 两个 IPv4 半区分别查询，防止只看到 /0 而漏掉 VPN 的两条 /1。
#[cfg(target_os = "macos")]
pub fn default_routes() -> Option<Vec<(String, bool)>> {
    ["64.0.0.1", "192.0.2.1"]
        .iter()
        .map(|ip| route(ip.parse().ok()?, None))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_proxy_parsing_keeps_pac_socks_and_unknown_separate() {
        assert_eq!(
            parse_proxy("<dictionary> {\n}\n").unwrap().enabled,
            Some(false)
        );
        let p = parse_proxy("<dictionary> {\n HTTPSEnable : 1\n HTTPSProxy : 127.0.0.1\n HTTPSPort : 7890\n ProxyAutoConfigEnable : 1\n ProxyAutoConfigURLString : https://example.test/private-token\n}\n").unwrap();
        assert_eq!(p.enabled, Some(true));
        assert!(p.pac.is_some());
        assert!(!format!("{p:?}").contains("private-token"));
        assert!(parse_proxy("<dictionary> {\n HTTPEnable : 1\n}\n").is_none());
        assert!(parse_proxy("command failed").is_none());
        let p = parse_proxy("<dictionary> {\n __SCOPED__ : <dictionary> {\n en0 : <dictionary> {\n HTTPEnable : 1\n }\n }\n}\n").unwrap();
        assert!(p.unsupported);
    }

    #[test]
    fn system_proxy_bypass_rules_are_unsupported_only_when_applicable() {
        let proxy = " HTTPSEnable : 1\n HTTPSProxy : 127.0.0.1\n HTTPSPort : 7890\n";
        let exceptions = " ExceptionsList : <array> {\n  0 : *.local\n  1 : claude.ai\n }\n";
        assert!(
            parse_proxy(&format!("<dictionary> {{\n{proxy}{exceptions}}}\n"))
                .unwrap()
                .unsupported
        );
        assert!(
            parse_proxy(&format!(
                "<dictionary> {{\n{proxy} ExcludeSimpleHostnames : 1\n}}\n"
            ))
            .unwrap()
            .unsupported
        );
        assert!(
            !parse_proxy(&format!(
                "<dictionary> {{\n{exceptions} ExcludeSimpleHostnames : 1\n}}\n"
            ))
            .unwrap()
            .unsupported
        );
        assert!(!parse_proxy(&format!("<dictionary> {{\n{proxy} ExceptionsList : <array> {{\n }}\n ExcludeSimpleHostnames : 0\n}}\n")).unwrap().unsupported);
        assert!(parse_proxy("<dictionary> {\n ExceptionsList : invalid\n}\n").is_none());
    }

    #[test]
    fn proxy_environment_follows_reqwest_precedence_without_exposing_values() {
        let note = |pairs: &[(&str, &str)], cgi| {
            proxy_environment_override(
                |name| {
                    pairs
                        .iter()
                        .find(|(key, _)| *key == name)
                        .map(|(_, value)| value.to_string())
                },
                cgi,
            )
        };
        for key in [
            "ALL_PROXY",
            "all_proxy",
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "NO_PROXY",
            "no_proxy",
        ] {
            assert!(note(&[(key, "proxy.example.test")], false)
                .unwrap()
                .contains(key));
        }
        for empty in ["", " ", ", ,\t,"] {
            assert!(note(&[("NO_PROXY", empty)], false).is_none());
        }
        // An empty uppercase value suppresses a nonempty lowercase value in hyper-util.
        assert!(note(
            &[
                ("HTTPS_PROXY", ""),
                ("https_proxy", "http://proxy.example.test")
            ],
            false
        )
        .is_none());
        assert!(note(&[("NO_PROXY", ""), ("no_proxy", "*")], false).is_none());
        assert!(note(&[("https_proxy", " ")], false).is_some());
        assert!(note(&[], false).is_none());
        assert!(note(&[], true).unwrap().contains("REQUEST_METHOD"));
        let message = note(
            &[(
                "HTTPS_PROXY",
                "http://alice:private-password@proxy.example.test:7890",
            )],
            false,
        )
        .unwrap();
        assert!(!message.contains("private-password"));
        assert!(!message.contains("alice"));
    }

    #[test]
    fn routes_require_kernel_interface_evidence() {
        assert_eq!(
            route_interface("   interface: utun8\n"),
            Some("utun8".into())
        );
        assert_eq!(
            interface_tunnel(
                "utun8: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1500\n",
                false
            ),
            Some(true)
        );
        assert_eq!(interface_tunnel("en0: flags=8863<UP,BROADCAST,RUNNING,MULTICAST> mtu 1500\n ether aa:bb:cc:dd:ee:ff\n", true), Some(false));
        assert_eq!(
            interface_tunnel("lo0: flags=8049<UP,LOOPBACK,RUNNING> mtu 16384\n", false),
            None
        );
        assert_eq!(
            interface_tunnel("utun8: flags=8050<POINTOPOINT,RUNNING> mtu 1500\n", false),
            None
        );
        assert!(route_interface("route: not in table").is_none());
        assert_eq!(
            interface_tunnel(
                "tap0: flags=8863<UP,BROADCAST,RUNNING> mtu 1500\n ether aa:bb:cc:dd:ee:ff\n",
                false
            ),
            None
        );
        assert_eq!(hardware_interfaces("Hardware Port: Wi-Fi\nDevice: en0\nEthernet Address: aa:bb:cc:dd:ee:ff\n\nHardware Port: USB LAN\nDevice: en5\n"), vec!["en0", "en5"]);
    }

    #[test]
    fn dns_keeps_scoped_ipv6_and_domain_resolvers() {
        let rows = parse_dns("DNS configuration\nresolver #1\n nameserver[0] : 1.1.1.1\nresolver #2\n domain : example.test\n nameserver[0] : 10.0.0.53\n if_index : 7 (en0)\nDNS configuration (for scoped queries)\nresolver #1\n nameserver[0] : fe80::1%en0\n if_index : 7 (en0)\n").unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].interface, None);
        assert_eq!(rows[1].interface.as_deref(), Some("en0"));
        assert_eq!(rows[2].ip, "fe80::1".parse::<IpAddr>().unwrap());
        assert!(parse_dns("DNS configuration\nresolver #1\n nameserver[0] : invalid\n").is_none());
        assert!(parse_dns("No DNS configuration available").is_none());
    }

    /// 在 macOS CI 实际执行固定系统工具，只读系统设置，不联网请求、不改网络。
    /// 这些断言检查实际 OS 输出和 route 的选项，不能由 Windows 夹具测试替代。
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_network_native_read_only_smoke() {
        let proxy = read_command("/usr/sbin/scutil", &["--proxy"]).expect("scutil --proxy failed");
        assert!(
            parse_proxy(&proxy).is_some(),
            "actual scutil proxy output must parse"
        );
        let dns = read_command("/usr/sbin/scutil", &["--dns"]).expect("scutil --dns failed");
        let servers = parse_dns(&dns).expect("actual scutil DNS output must parse");
        assert!(
            !servers.is_empty(),
            "native CI must have a configured DNS resolver"
        );
        let ports = read_command("/usr/sbin/networksetup", &["-listallhardwareports"])
            .expect("networksetup read failed");
        assert!(!hardware_interfaces(&ports).is_empty());
        let routes =
            default_routes().expect("both IPv4 route probes must resolve on the native runner");
        assert_eq!(routes.len(), 2);
        // 验证 -ifscope 真正被 macOS route 接受，而不是只对拼接字符串做断言。
        let (interface, _) = &routes[0];
        assert!(route("64.0.0.1".parse().unwrap(), Some(interface)).is_some());
        // DNS 含 IPv6 / loopback 时可能合法地没有可判定的转发路径，不能要求全绿。
        for server in servers {
            let _ = route(server.ip, server.interface.as_deref());
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_network_command_timeout_is_bounded() {
        let start = std::time::Instant::now();
        assert!(
            read_command_bounded("/bin/sleep", &["5"], std::time::Duration::from_millis(50))
                .is_none()
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }
}
