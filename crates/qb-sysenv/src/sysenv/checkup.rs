//! 运行环境体检。
//!
//! 「中文环境识别」那 10 项指纹是浏览器里算的（`src/lib/signals.ts`，改编自
//! FuckClaude）。这里补的是**只有本机才看得到**的那几项：系统代理、IPv6、
//! 浏览器 DoH 策略、以及 MCP 配置里有没有明文密钥。
//!
//! 项目划分参考 check-cc 与 claude-antiban-macos（都是 MIT）的体检清单，
//! Windows 侧的读法是自己写的。
//!
//! 2026-09-24 按 [CheckClaude](https://github.com/zzusec/CheckClaude)（MIT，© 2026 zzusec）
//! 补了三组，判法自己写：
//!
//! | 项 | 原来 | 现在 |
//! |---|---|---|
//! | 代理形态（`proxy`） | 只读 `ProxyEnable` —— PAC 开着也说「系统代理没开」 | 读 PAC / 自动检测，再看默认路由是不是被虚拟网卡接走（TUN） |
//! | IPv6（`ipv6`） | 网卡上开着 IPv6 就警告 | 实测只有 v6 的回显服务：有没有 v6 出口、在哪个国家、跟 IPv4 比 |
//! | Anthropic 服务可达（`anthropic_reach`） | 没有 | 不带凭据问 API：401 = 地区放行，403 = 地区拦截 |
//! | claude.ai 的解析（`claude_dns`） | 没有 | 系统解析器解出来落在哪一段：fake-ip / Anthropic / Cloudflare / 内网（被污染） |
//!
//! 后两项与 IPv6 进「出口一致性」那 10 分（`usecase::egress_checks`），403、被污染、
//! v6 出口在别的国家三种是**关键项**：界面评分封顶（`src/lib/score.ts`）。
//!
//! # 为什么大部分项「只报告不代劳」
//!
//! 使用者选的是「诊断 + 修可控项」。可是这几项里真正**安全可逆**的只有时区：
//!
//! - 关掉系统代理 → 可能当场把他的网断掉；
//! - 关掉 IPv6 → 要管理员权限 + 重启才生效，而且他可能正靠 v6 上网；
//! - 改 DoH 策略 → 那是 HKLM 下的企业策略，动它会影响整机所有用户。
//!
//! 所以这几项给的是**原样可复制的命令**加上代价说明，由他自己决定 ——
//! 与「中文字体那 14 分修不掉就不提供修复按钮，也不假装能修」是同一条。
//! 面板不做那种「点一下，然后你发现自己上不了网」的按钮。
//!
//! # 0.19.0：系统代理这一项另有出口，但**这个模块仍然只报告**
//!
//! 使用者拍板开了「系统代理修改」这个口子，实现在 [`super::proxy`]。
//! 上面那条「可能当场把他的网断掉」一个字都没错，所以那个口子把同样的谨慎
//! 写进了约束：只在当次点击后改、改前记下原值、界面上随时能回滚，
//! **没有定时器、没有启动时触发**。
//!
//! 体检这一侧不变 —— `check_proxy` 照旧只报告现状与代价，不在这里动手。
//! 两件事分开的理由很实在：体检是**只读**的，一个只读扫描里藏着会改系统的
//! 副作用，是这类工具最不该有的东西。
//!
//! # 密钥扫描只报位置，绝不报内容
//!
//! 跟 `relay::ProviderView` 那条「结构上就装不下 Key」同一个原则：
//! 这里只回文件路径与字段名。把扫出来的密钥打进结构体，早晚会有人
//! 把它打进日志、截图、或者贴进 issue 里。

use serde::Serialize;
use std::path::PathBuf;
use ts_rs::TS;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "CheckState")]
#[serde(rename_all = "snake_case")]
pub enum State {
    Pass,
    Warn,
    Fail,
    /// 查不出来。**跟 Pass 是两回事**，界面上不许合并显示。
    Unknown,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CheckItem {
    pub id: String,
    pub label: String,
    pub state: State,
    pub detail: String,
    /// 面板能不能安全地替你修。
    pub fixable: bool,
    /// 不能代劳时，给一条原样可复制的命令或步骤。
    pub manual: Option<String>,
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

// ------------------------------------------------------------ 注册表读取

#[cfg(windows)]
fn reg_query(path: &str, name: &str) -> Option<String> {
    let out = crate::process::hidden_std(std::process::Command::new("reg"))
        .args(["query", path, "/v", name])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    // 形如：`    ProxyEnable    REG_DWORD    0x1`
    let line = text.lines().find(|l| l.contains(name))?;
    let val = line.split_whitespace().last()?.to_string();
    Some(val)
}

#[cfg(not(windows))]
fn reg_query(_path: &str, _name: &str) -> Option<String> {
    None
}

// ------------------------------------------------------------------ 各项

// ------------------------------------------------------------ 代理形态

/// 系统代理这一层的设置（`HKCU\…\Internet Settings`）。读不出来的项是 `None`。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProxySettings {
    pub enabled: Option<bool>,
    pub server: Option<String>,
    /// PAC 脚本地址（`AutoConfigURL`）。**0.25.1 之前这一项根本没读** ——
    /// PAC 开着、`ProxyEnable` 是 0 时，面板照样说「系统代理没开，出口由路由/TUN 决定」。
    pub pac: Option<String>,
    /// 「自动检测设置」（WPAD）。读自 `Connections\DefaultConnectionSettings` 的标志位。
    pub auto_detect: Option<bool>,
}

/// 默认路由（`0.0.0.0/0`，或 VPN 常用的 `0.0.0.0/1` + `128.0.0.0/1` 两半）挂在哪张网卡上。
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct RouteRow {
    pub prefix: String,
    pub alias: String,
    /// 路由跃点 + 接口跃点。小的赢。
    pub metric: i64,
    /// `Get-NetAdapter` 的 `HardwareInterface`：物理网卡是 `true`，TUN / TAP / WireGuard 这类是 `false`。
    pub hardware: bool,
    pub desc: String,
    pub up: bool,
}

/// `DefaultConnectionSettings` 那串十六进制里第 9 个字节是标志位：
/// `0x01` 直连、`0x02` 手动代理、`0x04` PAC、`0x08` 自动检测。
pub fn auto_detect_from(hex: &str) -> Option<bool> {
    let b = u8::from_str_radix(hex.get(16..18)?, 16).ok()?;
    Some(b & 0x08 != 0)
}

fn read_proxy_settings() -> ProxySettings {
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    ProxySettings {
        enabled: reg_query(KEY, "ProxyEnable").map(|v| v.ends_with('1')),
        server: reg_query(KEY, "ProxyServer"),
        pac: reg_query(KEY, "AutoConfigURL").filter(|v| !v.trim().is_empty()),
        auto_detect: reg_query(&format!(r"{KEY}\Connections"), "DefaultConnectionSettings")
            .and_then(|hex| auto_detect_from(&hex)),
    }
}

/// 默认路由那几条挂在哪。读不出来是 `None`（不是「没有隧道」）。
///
/// 脚本里不拼任何外来的字符串，也不用双引号（整段走 `-Command`，坑 7.28）；
/// 输出一律 `ConvertTo-Json -InputObject @(...)`，一条也是数组（坑 7.17）。
#[cfg(windows)]
fn read_default_routes() -> Option<Vec<RouteRow>> {
    const SCRIPT: &str = "$ErrorActionPreference = 'Stop'; \
        $rows = @(Get-NetRoute -AddressFamily IPv4 | Where-Object { $_.DestinationPrefix -in @('0.0.0.0/0','0.0.0.0/1','128.0.0.0/1') } | ForEach-Object { \
          $a = Get-NetAdapter -InterfaceIndex $_.ifIndex -IncludeHidden -ErrorAction SilentlyContinue; \
          $i = Get-NetIPInterface -InterfaceIndex $_.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue; \
          [pscustomobject]@{ prefix = [string]$_.DestinationPrefix; alias = [string]$_.InterfaceAlias; \
            metric = [int64]$_.RouteMetric + [int64]$i.InterfaceMetric; hardware = [bool]$a.HardwareInterface; \
            desc = [string]$a.InterfaceDescription; up = ([string]$a.Status -eq 'Up') } }); \
        ConvertTo-Json -Compress -Depth 3 -InputObject $rows";
    let out = crate::process::powershell_std(SCRIPT).output().ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

#[cfg(not(windows))]
fn read_default_routes() -> Option<Vec<RouteRow>> {
    None
}

/// 默认路由是不是被一张虚拟网卡（TUN / TAP / WireGuard / 某个 VPN）接走了。
///
/// **按网卡的性质认，不按名字认** —— `xray_tun`、`Clash`、`Meta`、`sing-box`、`Wintun Userspace Tunnel`，
/// 别人机器上叫什么都有可能（CLAUDE.md「别写死本机事实」）。Hyper-V 的虚拟交换机也是
/// 「非硬件网卡」，但它是宿主机自己的网，不算隧道。
pub fn tunnel_of(routes: &[RouteRow]) -> Option<&RouteRow> {
    let is_tunnel = |r: &RouteRow| r.up && !r.hardware && !r.desc.contains("Hyper-V");
    // VPN 常用的两半：比 0.0.0.0/0 更具体，一定赢。
    if let Some(r) = routes
        .iter()
        .find(|r| (r.prefix == "0.0.0.0/1" || r.prefix == "128.0.0.0/1") && is_tunnel(r))
    {
        return Some(r);
    }
    routes
        .iter()
        .filter(|r| r.prefix == "0.0.0.0/0" && r.up)
        .min_by_key(|r| r.metric)
        .filter(|r| is_tunnel(r))
}

/// 「代理形态」这一项。纯函数：读取在调用方。
///
/// 参照 CheckClaude 的「代理形态」（TUN 全局 / 系统代理 / PAC），判法按本项目的口径：
/// TUN 接管整机（含 UDP、DNS），跟面板量到的是同一条路 —— 通过；
/// 系统代理与 PAC 只管认它的程序，PAC 还按网站分流 —— 警告，面板不替你改。
pub fn proxy_form_item(s: &ProxySettings, routes: Option<&[RouteRow]>) -> CheckItem {
    const ID: &str = "proxy";
    const LABEL: &str = "代理形态";
    let tun = routes.and_then(tunnel_of);
    let tun_text = tun.map(|r| format!("TUN / 虚拟网卡接管了默认路由（{}）", r.alias));
    let pac = s.pac.as_deref();

    if let Some(url) = pac {
        let mut it = item(
            ID,
            LABEL,
            State::Warn,
            format!(
                "开着 PAC 自动分流（{url}）{}。认系统代理的程序（浏览器、桌面端）按网站分流，\
                 不同网站可能从不同出口出去；面板「跟随系统代理」那一路也不认 PAC，出口一致性那一项看不到它。",
                tun_text.as_deref().map(|t| format!("，同时{t}")).unwrap_or_default()
            ),
        );
        it.manual = Some(
            "要关就去「设置 → 网络和 Internet → 代理 → 使用设置脚本」里关，或者在代理软件里改成全局 / TUN。\
             面板不替你关 —— 你的网可能正是靠它出去的。"
                .into(),
        );
        return it;
    }
    if s.enabled == Some(true) {
        let server = s.server.as_deref().unwrap_or("（读不出地址）");
        let mut it = item(
            ID,
            LABEL,
            State::Warn,
            format!(
                "系统代理开着（{server}）{}。面板测出口时是绕过系统代理的，\
                 所以面板看到的出口和走系统代理的程序看到的可能不是同一个。",
                tun_text
                    .as_deref()
                    .map(|t| format!("，同时{t}"))
                    .unwrap_or_default()
            ),
        );
        it.manual = Some(
            "要关就去「设置 → 网络和 Internet → 代理」里关。\
             面板不替你关 —— 你的网可能正是靠它出去的，关掉会当场断网。"
                .into(),
        );
        return it;
    }
    let wpad = if s.auto_detect == Some(true) {
        "（「自动检测设置」开着：网络里若有 WPAD 配置，浏览器会自己去拿一份 PAC。）"
    } else {
        ""
    };
    match (tun_text, routes, s.enabled) {
        (Some(t), _, _) => item(
            ID,
            LABEL,
            State::Pass,
            format!(
                "{t}，没开系统代理与 PAC：所有程序的流量（含 UDP 与 DNS）都从隧道出去，\
                 跟面板量到的是同一条路。{wpad}"
            ),
        ),
        (None, Some(_), Some(false)) => item(
            ID,
            LABEL,
            State::Pass,
            format!(
                "没开系统代理、没开 PAC，也没看到隧道接管默认路由 —— 出口就是本机网络。\
                 人在海外直连时这是正常的。{wpad}"
            ),
        ),
        (None, None, Some(false)) => item(
            ID,
            LABEL,
            State::Unknown,
            "没开系统代理与 PAC；读不出默认路由挂在哪张网卡上，看不出有没有隧道。",
        ),
        _ => item(ID, LABEL, State::Unknown, "读不出系统代理设置"),
    }
}

/// 网卡上的 IPv6 绑定。只报告，不下结论（结论看真实出口，见 [`ipv6_item`]）。
fn ipv6_bindings_note(
    bindings: &std::result::Result<Vec<super::ipv6::Ipv6Binding>, String>,
) -> String {
    match bindings {
        Ok(rows) if !rows.is_empty() && rows.iter().all(|b| !b.enabled) => {
            format!("{} 张网卡的 IPv6 绑定都关着（含隐藏网卡）。", rows.len())
        }
        Ok(rows) if !rows.is_empty() => format!(
            "{} / {} 张网卡还开着 IPv6。",
            rows.iter().filter(|b| b.enabled).count(),
            rows.len()
        ),
        Ok(_) => "没有读到网卡的 IPv6 绑定。".into(),
        Err(e) => format!("读网卡的 IPv6 绑定失败：{e}。"),
    }
}

/// 「IPv6」这一项（2026-09-24 起看**真实出口**，参照 CheckClaude 的「IPv6 出口」）。
///
/// 原来只看网卡上 IPv6 开没开：开着就警告 —— 可很多宽带根本不给 v6，开着也漏不出去；
/// 反过来网卡关了、隧道没接管 v6 的情况它也说不清。现在：
///
/// | 实测 | 结论 |
/// |---|---|
/// | 只有 v6 的回显服务都连不上 | 通过：没有 v6 出口 |
/// | 回来的是 IPv4 | 通过：v6 请求被代理接走了 |
/// | 有 v6 出口，跟 IPv4 同一个国家 | 通过 |
/// | 有 v6 出口，跟 IPv4 **不是**同一个国家 | **失败**（关键项）：隧道没接管 v6，对端看得到另一个国家的地址 |
/// | 有 v6 出口，查不到国家 / IPv4 国家没测到 | 警告 |
///
/// `exit` 为 `None`（这一轮没测成）时退回只看网卡绑定的老判法。
pub fn ipv6_item(
    bindings: &std::result::Result<Vec<super::ipv6::Ipv6Binding>, String>,
    exit: Option<&crate::probe::reach::V6Exit>,
    v4_countries: &[String],
) -> CheckItem {
    use crate::probe::reach::V6Exit;
    let note = ipv6_bindings_note(bindings);
    let mut it = match exit {
        Some(V6Exit::None) => item(
            "ipv6",
            "IPv6",
            State::Pass,
            format!(
                "测不到 IPv6 出口（只有 v6 的回显服务一个都连不上）—— 没有从 v6 漏出去的路。{note}"
            ),
        ),
        Some(V6Exit::ViaProxy { ip }) => item(
            "ipv6",
            "IPv6",
            State::Pass,
            format!("IPv6 请求被代理接走了（回显看到的是 {ip}），本机没有自己的 v6 出口。{note}"),
        ),
        Some(V6Exit::Exit {
            ip,
            country: Some(cc),
        }) => {
            if v4_countries.iter().any(|c| c.eq_ignore_ascii_case(cc)) {
                item(
                    "ipv6",
                    "IPv6",
                    State::Pass,
                    format!("有 IPv6 出口 {ip}，也在 {cc}，跟 IPv4 出口一致。{note}"),
                )
            } else if v4_countries.is_empty() {
                item(
                    "ipv6",
                    "IPv6",
                    State::Warn,
                    format!(
                        "有 IPv6 出口 {ip}，在 {cc}；IPv4 出口的国家这一轮没测到，比不了。{note}"
                    ),
                )
            } else {
                item(
                    "ipv6",
                    "IPv6",
                    State::Fail,
                    format!(
                        "IPv6 出口 {ip} 在 {cc}，IPv4 出口在 {} —— 隧道没接管 IPv6，\
                         走 v6 的请求会把另一个国家的地址露给对面。{note}",
                        v4_countries.join("/")
                    ),
                )
            }
        }
        Some(V6Exit::Exit { ip, country: None }) => item(
            "ipv6",
            "IPv6",
            State::Warn,
            format!("有 IPv6 出口 {ip}，但查不到它在哪个国家。{note}"),
        ),
        None => match bindings {
            Ok(rows) if !rows.is_empty() && rows.iter().all(|b| !b.enabled) => item(
                "ipv6",
                "IPv6",
                State::Pass,
                format!("这一轮没测成 IPv6 出口；{note}"),
            ),
            Ok(rows) if !rows.is_empty() => item(
                "ipv6",
                "IPv6",
                State::Warn,
                format!("这一轮没测成 IPv6 出口；{note}隧道只接管 IPv4 时可能出现出口不一致。"),
            ),
            _ => item(
                "ipv6",
                "IPv6",
                State::Unknown,
                format!("这一轮没测成 IPv6 出口；{note}"),
            ),
        },
    };
    it.manual = Some(
        "在代理软件里打开 IPv6 接管；或者用「IP 纯净度 → 禁用本机 IPv6」开关（需要管理员授权，能随时恢复原设置）。"
            .into(),
    );
    it
}

// ------------------------------------------------------------ 服务可达与解析

fn reach_line(
    name: &str,
    o: &crate::probe::reach::HttpOutcome,
    r: crate::probe::reach::Reach,
) -> String {
    use crate::probe::reach::Reach;
    let code = o
        .status
        .map(|s| format!("HTTP {s}"))
        .unwrap_or_else(|| o.error.clone().unwrap_or_else(|| "连不上".into()));
    let what = match r {
        Reach::Open => "通",
        Reach::Blocked => "被拦（403）",
        Reach::Challenge => "Cloudflare 验证页",
        Reach::Unreachable => "连不上",
        Reach::Other(_) => "异常",
    };
    format!("{name} {what}（{code}）")
}

/// 「Anthropic 服务可达」这一项（参照 CheckClaude 的「Anthropic API 可达」「claude.ai 可达」
/// 「anthropic.com 可达」三项）。`via_proxy` 只在系统代理开着时有。
///
/// 判定以 API 那一个为准 —— 它是 Anthropic 自己对「这个出口能不能用」的回答：
/// 403 就是地区拦截（**关键项**，评分封顶）。网页那两个只降级到警告：
/// claude.ai 前面有一层人机验证，打不开不一定是地区的事。
pub fn reach_item(
    direct: &crate::probe::reach::ReachReport,
    via_proxy: Option<&crate::probe::reach::ReachReport>,
) -> CheckItem {
    use crate::probe::reach::{classify_api, classify_site, Reach};
    const ID: &str = "anthropic_reach";
    const LABEL: &str = "Anthropic 服务可达";
    let api = classify_api(&direct.api);
    let web = classify_site(&direct.web);
    let site = classify_site(&direct.site);
    let lines = [
        reach_line("API", &direct.api, api),
        reach_line("claude.ai", &direct.web, web),
        reach_line("anthropic.com", &direct.site, site),
    ]
    .join("；");
    let proxy_note = via_proxy
        .map(|p| {
            let pa = classify_api(&p.api);
            if pa == api {
                String::new()
            } else {
                format!(
                    " 跟随系统代理那一路不一样：{}（浏览器和桌面端走的是这一路）。",
                    reach_line("API", &p.api, pa)
                )
            }
        })
        .unwrap_or_default();
    let proxy_blocked = via_proxy.is_some_and(|p| classify_api(&p.api) == Reach::Blocked);

    let mut it = match api {
        Reach::Blocked => item(
            ID,
            LABEL,
            State::Fail,
            format!(
                "Anthropic 的 API 回了 403：它不接受从这个出口来的请求（地区拦截）。{lines}。{proxy_note}"
            ),
        ),
        Reach::Unreachable => item(
            ID,
            LABEL,
            State::Fail,
            format!("连不上 Anthropic 的 API。{lines}。{proxy_note}"),
        ),
        Reach::Open => {
            let pages_ok = web == Reach::Open && site == Reach::Open;
            if proxy_blocked {
                item(
                    ID,
                    LABEL,
                    State::Warn,
                    format!("直连这一路放行（API 回 {}），但{proxy_note}{lines}。", status_of(&direct.api)),
                )
            } else if pages_ok {
                item(
                    ID,
                    LABEL,
                    State::Pass,
                    format!(
                        "不带密钥问 API 回 {}（缺密钥时的正常回答，说明这个出口放行），\
                         claude.ai 与 anthropic.com 都打得开。{proxy_note}",
                        status_of(&direct.api)
                    ),
                )
            } else {
                item(
                    ID,
                    LABEL,
                    State::Warn,
                    format!("API 放行了，但网页那两个有问题：{lines}。{proxy_note}"),
                )
            }
        }
        Reach::Challenge | Reach::Other(_) => item(
            ID,
            LABEL,
            State::Unknown,
            format!("API 回的不是能说明地区的答案。{lines}。{proxy_note}"),
        ),
    };
    if it.state != State::Pass {
        it.manual = Some(
            "换到 Anthropic 支持地区的出口；确认代理是全局 / TUN 接管，而不是只有部分网站走代理。\
             这一项只发了三个不带任何凭据的请求，跟你的账户无关。"
                .into(),
        );
    }
    it
}

fn status_of(o: &crate::probe::reach::HttpOutcome) -> String {
    o.status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "—".into())
}

/// 「claude.ai 的解析」这一项（参照 CheckClaude 的「claude.ai 解析」）。
///
/// 解析到私网 / 回环是被污染或被劫持（**关键项**）；解析到认不出的公网段只警告 ——
/// 那也可能是代理用了自定义的 fake-ip 段。
pub fn dns_item(rs: &[crate::probe::reach::Resolved]) -> CheckItem {
    use crate::probe::reach::{classify_ip, DnsClass};
    const ID: &str = "claude_dns";
    const LABEL: &str = "claude.ai 的解析";
    if rs.is_empty() {
        return item(ID, LABEL, State::Unknown, "这一轮没解析。");
    }
    if let Some(r) = rs.iter().find(|r| r.addrs.is_empty()) {
        let mut it = item(
            ID,
            LABEL,
            State::Fail,
            format!(
                "{} 解析不出来（{}）。",
                r.host,
                r.error.as_deref().unwrap_or("没有地址")
            ),
        );
        it.manual = Some("检查 DNS 设置，或者让代理接管 DNS（fake-ip 模式）。".into());
        return it;
    }
    let mut worst = DnsClass::Anthropic;
    let mut bad: Option<String> = None;
    let rank = |c: DnsClass| match c {
        DnsClass::Private => 3,
        DnsClass::Other => 2,
        _ => 0,
    };
    for r in rs {
        for a in &r.addrs {
            let Ok(ip) = a.parse() else { continue };
            let c = classify_ip(ip);
            if rank(c) > rank(worst) {
                worst = c;
                bad = Some(format!("{} → {a}", r.host));
            }
        }
    }
    let seen = rs
        .iter()
        .map(|r| format!("{} → {}", r.host, r.addrs.join(", ")))
        .collect::<Vec<_>>()
        .join("；");
    let kind = |c: DnsClass| match c {
        DnsClass::FakeIp => "代理接管（fake-ip）",
        DnsClass::Anthropic => "Anthropic 自己的地址段",
        DnsClass::Cloudflare => "Cloudflare 的地址段",
        DnsClass::Private => "内网 / 回环地址",
        DnsClass::Other => "认不出的公网地址",
    };
    match worst {
        DnsClass::Private => {
            let mut it = item(
                ID,
                LABEL,
                State::Fail,
                format!(
                    "{} 是{}：claude.ai 不可能在那里 —— 解析被污染或被劫持了。{seen}。",
                    bad.unwrap_or_default(),
                    kind(DnsClass::Private)
                ),
            );
            it.manual = Some("换加密 DNS（DoH），或者让代理接管 DNS（fake-ip 模式）。".into());
            it
        }
        DnsClass::Other => {
            let mut it = item(
                ID,
                LABEL,
                State::Warn,
                format!(
                    "{} 不在 Anthropic / Cloudflare 的地址段，也不是常见的 fake-ip 段：可能被污染了。\
                     如果你的代理用了自定义的 fake-ip 段，这一条可以忽略。{seen}。",
                    bad.unwrap_or_default()
                ),
            );
            it.manual = Some("换加密 DNS（DoH），或者让代理接管 DNS。".into());
            it
        }
        _ => {
            let first = rs
                .first()
                .and_then(|r| r.addrs.first())
                .and_then(|a| a.parse().ok())
                .map(classify_ip)
                .unwrap_or(DnsClass::Anthropic);
            item(
                ID,
                LABEL,
                State::Pass,
                format!("解析正常：{}。{seen}。", kind(first)),
            )
        }
    }
}

fn check_doh() -> CheckItem {
    let chrome = reg_query(r"HKLM\SOFTWARE\Policies\Google\Chrome", "DnsOverHttpsMode");
    let edge = reg_query(r"HKLM\SOFTWARE\Policies\Microsoft\Edge", "DnsOverHttpsMode");
    match (chrome.as_deref(), edge.as_deref()) {
        (None, None) => {
            let mut it = item(
                "doh",
                "浏览器 DoH 策略",
                State::Unknown,
                "没有配置 DoH 策略 —— 浏览器用的是它自己的默认值，可能在用内置的 DoH 解析器。",
            );
            it.manual = Some(
                "要不要关掉浏览器自带的 DoH 取决于你的方案：\
                 走 TUN 全局接管时它无所谓；靠系统 DNS 分流时它会绕过你的分流。\
                 判断不了就交给「DNS 泄露 → 高级通过」那条让 Codex 看一眼。"
                    .into(),
            );
            it
        }
        (c, e) => item(
            "doh",
            "浏览器 DoH 策略",
            State::Pass,
            format!(
                "已有策略：Chrome={}，Edge={}",
                c.unwrap_or("未设"),
                e.unwrap_or("未设")
            ),
        ),
    }
}

/// 看起来像密钥的字段名。**只匹配名字，不看值。**
const SECRET_KEYS: &[&str] = &[
    "api_key",
    "apikey",
    "api-key",
    "token",
    "secret",
    "password",
    "auth_token",
    "access_key",
    "bearer",
];

/// 一处明文密钥。**只有位置，没有内容。**
#[derive(Debug, Clone, Serialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct SecretHit {
    pub file: PathBuf,
    /// 字段名，比如 `api_key`。**值永远不带出来。**
    pub field: String,
}

/// 在一段文本里找像密钥的字段名。纯函数，可单测。
///
/// 故意做得很笨（只看字段名 + 后面跟着一个非空字符串），
/// 宁可多报也不漏报 —— 这一项的产出是「你自己去看一眼这个文件」，
/// 误报的代价只是多看一眼，漏报的代价是密钥躺在那儿没人知道。
pub fn find_secret_fields(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    for k in SECRET_KEYS {
        let mut from = 0;
        while let Some(i) = lower[from..].find(k) {
            let at = from + i;
            let rest = &lower[at + k.len()..];
            // 后面得跟着 `"? *[:=] *"?` 再跟一个非空的值，才算「这里真写了个值」。
            let v = rest.trim_start_matches(['"', '\'', ' ']);
            if let Some(v) = v.strip_prefix([':', '=']) {
                let v = v.trim_start_matches(['"', '\'', ' ']);
                // `null` / `false` 是「没设」，跟空串一样不算泄漏。
                // 漏掉这一条的话，一份写着 "api_key": null 的干净配置会被报成红的，
                // 而使用者点开看什么都没有 —— 几次之后他就不看这一项了。
                let unset = v.is_empty()
                    || v.starts_with(['\n', ',', '}', ']'])
                    || v.starts_with("null")
                    || v.starts_with("false");
                if !unset {
                    if !out.contains(&k.to_string()) {
                        out.push(k.to_string());
                    }
                    break;
                }
            }
            from = at + k.len();
        }
    }
    out
}

fn scan_secrets() -> (Vec<SecretHit>, Vec<PathBuf>) {
    let mut hits = Vec::new();
    let mut looked = Vec::new();
    let home = dirs::home_dir();
    let mut files: Vec<PathBuf> = Vec::new();
    if let Some(h) = &home {
        files.push(h.join(".mcp.json"));
        files.push(h.join(".claude").join("settings.json"));
        files.push(h.join(".claude.json"));
        files.push(h.join(".codex").join("config.toml"));
    }
    // 每个账户槽位自己的 settings.json。
    let roots = crate::accounts::AccountRoots::current();
    if let Ok(rd) = std::fs::read_dir(&roots.panel) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                files.push(e.path().join("settings.json"));
                files.push(e.path().join(".mcp.json"));
            }
        }
    }
    for f in files {
        if !f.is_file() {
            continue;
        }
        looked.push(f.clone());
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        for field in find_secret_fields(&text) {
            hits.push(SecretHit {
                file: f.clone(),
                field,
            });
        }
    }
    (hits, looked)
}

fn check_secrets() -> (CheckItem, Vec<SecretHit>) {
    let (hits, looked) = scan_secrets();
    let it = if looked.is_empty() {
        item(
            "secrets",
            "MCP / 配置里的明文密钥",
            State::Unknown,
            "没找到任何配置文件可扫 —— 跟「扫过了、很干净」不是一回事。",
        )
    } else if hits.is_empty() {
        item(
            "secrets",
            "MCP / 配置里的明文密钥",
            State::Pass,
            format!("扫了 {} 个配置文件，没看到明文密钥字段。", looked.len()),
        )
    } else {
        let mut it = item(
            "secrets",
            "MCP / 配置里的明文密钥",
            State::Fail,
            format!(
                "在 {} 处看到疑似明文密钥字段。面板只报位置不报内容，自己去看一眼。",
                hits.len()
            ),
        );
        it.manual = Some(
            "这些文件会被同步盘、备份、以及你随手贴出来的截图带走。\
             能换成环境变量或者面板的中转站配置（DPAPI 加密存）就换掉。"
                .into(),
        );
        it
    };
    (it, hits)
}

// ------------------------------------------------------ 环境变量残留扫描

/// 会影响 Claude Code 行为、或者会让面板与实际请求走两条路的环境变量。
///
/// 想法来自 Agent-Guard（它的 README 开篇就是「`.zshrc` 里还留着 `HTTPS_PROXY`
/// 和 `ANTHROPIC_BASE_URL`」）。那个项目**没有源码可抄**（仓库只是落地页），
/// 所以这里从头写，Windows 侧看的是 `HKCU\Environment` 与当前进程环境。
const WATCHED_ENV: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "CLAUDE_CONFIG_DIR",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
];

/// 按**前缀**算的那一档。
///
/// `CLAUDE_CODE_*` 整族都会改 Claude Code 的行为（`CLAUDE_CODE_USE_BEDROCK`
/// / `_USE_VERTEX` 直接把它切到另一个云端点），可它不是一个固定的名字表 ——
/// 上游随时会加新的。0.19.2 之前这一项只认死名字，于是这一族**一个都查不到**，
/// 而 purge 的归属判定（`purge_ops::is_owned_env`）和启动前的残留检查
/// （`qb-accounts::residue`）早就认了它。同一件事三处口径不一样，
/// 最松的那一处就是使用者看到的那一处。
const WATCHED_ENV_PREFIX: &[&str] = &["CLAUDE_CODE_"];

/// 这个变量名归不归「环境变量残留」这一项管。
///
/// 用 `to_ascii_uppercase().starts_with` 而不是切片比较：变量名理论上
/// 可以带非 ASCII，切到半个字符上会当场 panic。
pub fn is_watched_env(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    WATCHED_ENV.iter().any(|w| w.eq_ignore_ascii_case(name))
        || WATCHED_ENV_PREFIX.iter().any(|p| upper.starts_with(p))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct EnvHit {
    pub name: String,
    /// 在哪儿设的：`用户环境变量`（注册表，重启也还在）或 `当前进程`。
    pub scope: String,
    /// **已经掩码过的**值。见 `mask_env_value`。
    pub shown: String,
}

/// 值怎么显示 —— 与 `SecretHit` 同一条原则：结构里装不下的东西就别装。
///
/// | 变量 | 显示什么 |
/// |---|---|
/// | `*_API_KEY` / `*_AUTH_TOKEN` / `*SECRET*` | 只说「已设置」，值一个字都不出现 |
/// | `*_BASE_URL` / `*_PROXY` | 只留 `scheme://host[:port]` |
/// | 其余 | 原样 |
///
/// URL 只留 host 不是洁癖：有些中转站把 token 直接放在路径里
/// （`https://x.com/v1/sk-xxxx`），也有人把凭证写成 `https://user:token@host`。
/// 显示全量等于把 Key 印在界面上、截图里、issue 里。
pub fn mask_env_value(name: &str, value: &str) -> String {
    let n = name.to_uppercase();
    if n.contains("KEY") || n.contains("TOKEN") || n.contains("SECRET") || n.contains("PASSWORD") {
        return "（已设置，值不显示）".into();
    }
    if n.contains("URL") || n.contains("PROXY") {
        return origin_only(value);
    }
    value.to_string()
}

/// 从一个 URL 里只取 `scheme://host[:port]`。认不出来就说认不出来，**不回退成原值**。
fn origin_only(raw: &str) -> String {
    let v = raw.trim();
    let Some((scheme, rest)) = v.split_once("://") else {
        // 没有 scheme 的（比如 `127.0.0.1:7890` 这种代理写法）只取到第一个 `/` 为止。
        let host = v.split(['/', '?', '#']).next().unwrap_or("");
        // 仍然要去掉可能的 user:pass@
        let host = host.rsplit('@').next().unwrap_or(host);
        return if host.is_empty() {
            "（认不出）".into()
        } else {
            host.to_string()
        };
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // ⚠ `user:token@host` —— 凭证在 @ 前面，必须丢掉。
    let host = authority.rsplit('@').next().unwrap_or(authority);
    if host.is_empty() {
        "（认不出）".into()
    } else {
        format!("{scheme}://{host}")
    }
}

/// 解析 `reg query <key>` 的输出。返回 (名字, 值)。
///
/// 输出形如：`    ANTHROPIC_BASE_URL    REG_SZ    https://x.com/v1`
/// 值里可以有空格，所以**按类型段切**，不能 `split_whitespace` 取最后一个。
pub fn parse_reg_values(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with("HKEY_") {
            continue;
        }
        // 找 `REG_xxx` 这一段，它前面是名字，后面是值。
        let Some(ti) = t.find("    REG_") else {
            continue;
        };
        let name = t[..ti].trim().to_string();
        let after = &t[ti + 4..];
        let Some(vi) = after.find("    ") else {
            continue;
        };
        let value = after[vi..].trim().to_string();
        if !name.is_empty() {
            out.push((name, value));
        }
    }
    out
}

#[cfg(windows)]
fn user_env_vars() -> Vec<(String, String)> {
    let Ok(out) = crate::process::hidden_std(std::process::Command::new("reg"))
        .args(["query", r"HKCU\Environment"])
        .output()
    else {
        return Vec::new();
    };
    parse_reg_values(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(not(windows))]
fn user_env_vars() -> Vec<(String, String)> {
    Vec::new()
}

pub fn scan_env() -> (CheckItem, Vec<EnvHit>) {
    let mut hits = Vec::new();

    for (name, value) in user_env_vars() {
        if is_watched_env(&name) && !value.trim().is_empty() {
            hits.push(EnvHit {
                shown: mask_env_value(&name, &value),
                name,
                scope: "用户环境变量".into(),
            });
        }
    }
    // 进程环境要**遍历**，不能照着固定名字表一个个查 ——
    // 前缀那一档（`CLAUDE_CODE_*`）没有名字表可查。
    for (name, v) in std::env::vars() {
        if !is_watched_env(&name) || v.trim().is_empty() {
            continue;
        }
        // 注册表里已经报过同名的就不重复 —— 进程环境多半就是从那儿来的。
        if hits.iter().any(|h| h.name.eq_ignore_ascii_case(&name)) {
            continue;
        }
        hits.push(EnvHit {
            shown: mask_env_value(&name, &v),
            name,
            scope: "当前进程".into(),
        });
    }
    // 遍历出来的顺序不定，排一下：同一台机器两次体检的列表顺序不该跳来跳去。
    hits.sort_by(|a, b| a.name.cmp(&b.name));

    let redirecting = hits
        .iter()
        .any(|h| h.name.eq_ignore_ascii_case("ANTHROPIC_BASE_URL"));
    let proxied = hits.iter().any(|h| h.name.to_uppercase().contains("PROXY"));

    let it = if hits.is_empty() {
        item(
            "env_residue",
            "环境变量残留",
            State::Pass,
            "没有会影响 Claude Code 的环境变量残留。",
        )
    } else {
        let mut why: Vec<&str> = Vec::new();
        if redirecting {
            why.push(
                "设了 ANTHROPIC_BASE_URL —— Claude Code 会走它指的地方，\
                 而不是中转站页上显示的那条。两处说的不是一件事。",
            );
        }
        if proxied {
            why.push(
                "设了代理变量 —— 面板测出口时是绕过系统代理的，\
                 所以面板量到的出口和请求实际走的路可能不一样。",
            );
        }
        let mut it = item(
            "env_residue",
            "环境变量残留",
            if redirecting || proxied {
                State::Warn
            } else {
                State::Pass
            },
            format!("找到 {} 个相关的环境变量。{}", hits.len(), why.join("")),
        );
        if redirecting || proxied {
            it.manual = Some(
                "改用户环境变量：设置 → 系统 → 系统信息 → 高级系统设置 → 环境变量。\
                 改完要重开终端 / 重开 Claude Code 才生效。\
                 面板不替你删 —— 这些变量可能正是你别的工作要用的。"
                    .into(),
            );
        }
        it
    };
    (it, hits)
}

// ---------------------------------------------------------- 出口一致性

/// 两条路出去的地方一样吗。
///
/// 面板测出口时**绕过系统代理**（量的是隧道），别的程序不一定 ——
/// 走系统代理的那些看到的可能是另一个出口。这一项就是把这个差异摆出来，
/// 专治那个最难查的问题：「面板说我在美国，为什么还是被当成国内」。
///
/// ⛔ 结果**不进门禁判定**。`gate::judge` 的输入永远只来自绕过代理的那一份 ——
/// 让代理软件决定门禁看到的出口，随便一个本地代理就能把出口伪装成白名单里那个。
pub fn compare_egress(
    direct: &crate::probe::ip::Reading,
    via_proxy: &crate::probe::ip::Reading,
) -> CheckItem {
    let (Some(a), Some(b)) = (direct.ip.as_deref(), via_proxy.ip.as_deref()) else {
        return item(
            "egress_consistency",
            "出口一致性",
            State::Unknown,
            "两条路里至少一条没探到出口，比不了。这跟「一致」不是一回事。",
        );
    };
    let ca = direct.distinct_countries();
    let cb = via_proxy.distinct_countries();

    if !ca.is_empty() && !cb.is_empty() && ca != cb {
        let mut it = item(
            "egress_consistency",
            "出口一致性",
            State::Fail,
            format!(
                "两条路出去的国家不一样：绕过代理是 {}，跟随系统代理是 {}。\
                 有程序会从另一个国家出去。",
                ca.join("/"),
                cb.join("/")
            ),
        );
        it.manual = Some(
            "常见原因是系统代理或某个环境变量里的代理只接管了一部分流量。\
             对照上面「环境变量残留」和「系统代理」两项一起看。"
                .into(),
        );
        return it;
    }
    if a != b && (ca.is_empty() || cb.is_empty()) {
        return item(
            "egress_consistency",
            "出口一致性",
            State::Unknown,
            format!(
                "出口 IP 不同（直连 {a}，系统代理 {b}），至少一侧国家未知，无法判断地区是否一致。"
            ),
        );
    }
    if a != b {
        return item(
            "egress_consistency",
            "出口一致性",
            State::Warn,
            format!(
                "国家一致但 IP 不同（绕过代理 {a}，跟随系统代理 {b}）。\
                 双栈机器上很常见（一边 v4 一边 v6），通常不是问题。"
            ),
        );
    }
    item(
        "egress_consistency",
        "出口一致性",
        State::Pass,
        format!(
            "绕过系统代理和跟随系统代理，出口 IP 均为 {a}；国家：{}。",
            if ca.is_empty() {
                "未知".into()
            } else {
                ca.join("/")
            }
        ),
    )
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Checkup {
    pub items: Vec<CheckItem>,
    /// 明文密钥的位置清单。**没有任何密钥内容。**
    pub secrets: Vec<SecretHit>,
    /// 相关环境变量的清单。值都掩码过，见 `mask_env_value`。
    pub env: Vec<EnvHit>,
    /// 这一轮量到的出口地址：绕过系统代理那一路的 IP，外加（有的话）IPv6 出口。
    /// 真实浏览器的 WebRTC 候选地址拿它比 —— 候选地址就是出口的话不算泄露。
    pub exit_ips: Vec<String>,
}

/// 跑一轮体检。
///
/// 是 `async` 的唯一原因是出口一致性那一项要真发两轮请求（绕过代理 / 跟随代理）。
/// 其余几项全是本地读取，不联网。
pub async fn scan() -> Checkup {
    // 本地那几项全是同步的，而且**里面一共 fork 五次 `reg.exe`**
    // （代理、IPv6、DoH、环境变量、密钥扫描）。直接在 async 函数体里跑，
    // 堵的是 tokio 的 worker 线程 —— 同一时刻界面上别的请求全都卡住，
    // 表现是「点体检的时候整个面板僵一下」。
    //
    // 丢进 `spawn_blocking`：它跑在专门的阻塞线程池里，跟下面那两个
    // 网络请求真正并发。
    let local = tokio::task::spawn_blocking(|| {
        let (sec_item, secrets) = check_secrets();
        let (env_item, env) = scan_env();
        (
            read_proxy_settings(),
            read_default_routes(),
            super::ipv6::bindings().map_err(|e| e.to_string()),
            check_doh(),
            env_item,
            env,
            sec_item,
            secrets,
        )
    });

    // 网络那几项全并发：两轮出口、服务可达、claude.ai 的解析、IPv6 出口。
    // 服务可达与 IPv6 走**绕过系统代理**那一份客户端 —— 跟门禁同一条路（`ip::build_client`）。
    let direct_client = crate::probe::ip::build_client(true).ok();
    let (direct, via_proxy, reach, dns, v6) = tokio::join!(
        crate::probe::ip::reading(),
        crate::probe::ip::reading_via_system_proxy(),
        async {
            match &direct_client {
                Some(c) => Some(crate::probe::reach::reach(c).await),
                None => None,
            }
        },
        crate::probe::reach::resolve_claude(),
        async {
            match &direct_client {
                Some(c) => Some(crate::probe::reach::ipv6_exit(c).await),
                None => None,
            }
        },
    );

    // 阻塞任务 panic 了也要给出一份能看的报告 —— 体检页整个白屏
    // 比少一项糟得多。
    let (proxy_settings, routes, bindings, doh, env_item, env, sec_item, secrets) =
        local.await.unwrap_or_else(|_| {
            (
                ProxySettings::default(),
                None,
                Err("本地检查任务异常结束".into()),
                unavailable("doh", "DNS over HTTPS"),
                unavailable("env", "环境变量残留"),
                Vec::new(),
                unavailable("secrets", "明文密钥"),
                Vec::new(),
            )
        });

    // 系统代理开着时，服务可达再按「跟随系统代理」问一轮：浏览器与桌面端走的是那一路。
    let via_proxy_reach = if proxy_settings.enabled == Some(true) {
        match crate::probe::ip::build_client(false) {
            Ok(c) => Some(crate::probe::reach::reach(&c).await),
            Err(_) => None,
        }
    } else {
        None
    };

    let mut exit_ips: Vec<String> = direct.ip.iter().cloned().collect();
    if let Some(crate::probe::reach::V6Exit::Exit { ip, .. }) = &v6 {
        exit_ips.push(ip.clone());
    }

    Checkup {
        exit_ips,
        items: vec![
            proxy_form_item(&proxy_settings, routes.as_deref()),
            compare_egress(&direct, &via_proxy),
            reach
                .as_ref()
                .map(|r| reach_item(r, via_proxy_reach.as_ref()))
                .unwrap_or_else(|| unavailable("anthropic_reach", "Anthropic 服务可达")),
            dns_item(&dns),
            ipv6_item(&bindings, v6.as_ref(), &direct.distinct_countries()),
            doh,
            env_item,
            sec_item,
        ],
        secrets,
        env,
    }
}

/// 本地那几项没跑起来时的占位。**如实说「查不了」，不报成「通过」。**
fn unavailable(id: &str, label: &str) -> CheckItem {
    item(
        id,
        label,
        State::Unknown,
        "这一项这次没能查（本地检查任务异常结束）",
    )
}

/// 目前没有任何一项是面板代劳的 —— 见文件头。保留这个入口是为了
/// 让「以后有安全可逆的项时加在哪」这件事只有一个答案。
pub fn fix(id: &str) -> Result<String> {
    Err(crate::error::GateError::Other(format!(
        "「{id}」这一项面板不代劳，旁边写了怎么自己动手以及代价。"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::reach::{HttpOutcome, ReachReport, Resolved, V6Exit};

    fn route(prefix: &str, alias: &str, metric: i64, hardware: bool, desc: &str) -> RouteRow {
        RouteRow {
            prefix: prefix.into(),
            alias: alias.into(),
            metric,
            hardware,
            desc: desc.into(),
            up: true,
        }
    }

    /// 实机的形状：隧道网卡跃点更小，接走了默认路由。按网卡**性质**认，名字随便叫。
    #[test]
    fn a_virtual_adapter_carrying_the_default_route_is_a_tunnel() {
        let routes = vec![
            route(
                "0.0.0.0/0",
                "以太网",
                35,
                true,
                "Realtek Gaming 2.5GbE Family Controller",
            ),
            route("0.0.0.0/0", "随便起的名字", 5, false, "Xray Tunnel"),
        ];
        assert_eq!(
            tunnel_of(&routes).map(|r| r.alias.as_str()),
            Some("随便起的名字")
        );
        // 物理网卡跃点更小：流量没进隧道。
        let routes = vec![
            route("0.0.0.0/0", "Ethernet", 5, true, "Intel"),
            route("0.0.0.0/0", "wg0", 50, false, "WireGuard Tunnel"),
        ];
        assert_eq!(tunnel_of(&routes), None);
        // VPN 那种两半：比 0/0 具体，一定赢。
        let routes = vec![
            route("0.0.0.0/0", "Ethernet", 5, true, "Intel"),
            route("0.0.0.0/1", "OpenVPN", 100, false, "TAP-Windows Adapter V9"),
        ];
        assert!(tunnel_of(&routes).is_some());
        // Hyper-V 的外部交换机也是「非硬件网卡」，但那是宿主机自己的网。
        let routes = vec![route(
            "0.0.0.0/0",
            "vEthernet (External)",
            5,
            false,
            "Hyper-V Virtual Ethernet Adapter",
        )];
        assert_eq!(tunnel_of(&routes), None);
    }

    /// 0.25.1 之前的漏洞：PAC 开着、`ProxyEnable` 是 0，面板说「系统代理没开」。
    #[test]
    fn a_pac_script_is_reported_even_when_the_proxy_switch_is_off() {
        let s = ProxySettings {
            enabled: Some(false),
            server: Some("127.0.0.1:10808".into()),
            pac: Some("http://127.0.0.1:10809/pac".into()),
            auto_detect: Some(false),
        };
        let it = proxy_form_item(&s, Some(&[]));
        assert_eq!(it.state, State::Warn);
        assert!(it.detail.contains("PAC"), "{}", it.detail);
        assert!(it.manual.is_some());
    }

    #[test]
    fn proxy_form_states() {
        let off = ProxySettings {
            enabled: Some(false),
            ..Default::default()
        };
        let tun = vec![route("0.0.0.0/0", "xray_tun", 0, false, "Xray Tunnel")];
        assert_eq!(proxy_form_item(&off, Some(&tun)).state, State::Pass);
        assert!(proxy_form_item(&off, Some(&tun))
            .detail
            .contains("xray_tun"));
        assert_eq!(proxy_form_item(&off, Some(&[])).state, State::Pass, "直连");
        assert_eq!(
            proxy_form_item(&off, None).state,
            State::Unknown,
            "路由读不出来"
        );
        let on = ProxySettings {
            enabled: Some(true),
            server: Some("127.0.0.1:7890".into()),
            ..Default::default()
        };
        assert_eq!(proxy_form_item(&on, Some(&tun)).state, State::Warn);
        assert_eq!(
            proxy_form_item(&ProxySettings::default(), None).state,
            State::Unknown
        );
    }

    /// `DefaultConnectionSettings` 第 9 个字节的 0x08 是「自动检测设置」。
    #[test]
    fn the_auto_detect_flag_is_read_from_the_ninth_byte() {
        assert_eq!(auto_detect_from("460000001300000009000000"), Some(true));
        assert_eq!(auto_detect_from("460000001300000001000000"), Some(false));
        assert_eq!(auto_detect_from("4600"), None);
    }

    /// IPv6 看真实出口：没有 v6 出口就通过，哪怕网卡上还开着。
    #[test]
    fn ipv6_is_judged_by_the_real_exit_not_the_adapter_switch() {
        let open = Ok(vec![super::super::ipv6::Ipv6Binding {
            id: "a".into(),
            name: "以太网".into(),
            enabled: true,
        }]);
        let us = vec!["US".to_string()];
        assert_eq!(
            ipv6_item(&open, Some(&V6Exit::None), &us).state,
            State::Pass
        );
        assert_eq!(
            ipv6_item(
                &open,
                Some(&V6Exit::ViaProxy {
                    ip: "203.0.113.7".into()
                }),
                &us
            )
            .state,
            State::Pass
        );
        let exit = |cc: Option<&str>| V6Exit::Exit {
            ip: "2001:db8::1".into(),
            country: cc.map(String::from),
        };
        assert_eq!(
            ipv6_item(&open, Some(&exit(Some("US"))), &us).state,
            State::Pass
        );
        assert_eq!(
            ipv6_item(&open, Some(&exit(Some("CN"))), &us).state,
            State::Fail,
            "v6 出口在别的国家：隧道没接管 v6"
        );
        assert_eq!(ipv6_item(&open, Some(&exit(None)), &us).state, State::Warn);
        assert_eq!(
            ipv6_item(&open, Some(&exit(Some("CN"))), &[]).state,
            State::Warn
        );
        // 这一轮没测成：退回只看网卡的老判法。
        assert_eq!(ipv6_item(&open, None, &us).state, State::Warn);
    }

    fn o(status: Option<u16>) -> HttpOutcome {
        HttpOutcome {
            url: "u".into(),
            status,
            cf_challenge: false,
            error: status.is_none().then(|| "连不上".to_string()),
        }
    }
    fn report(api: Option<u16>, web: Option<u16>, site: Option<u16>) -> ReachReport {
        ReachReport {
            api: o(api),
            web: o(web),
            site: o(site),
        }
    }

    #[test]
    fn the_api_answer_decides_the_reach_item() {
        assert_eq!(
            reach_item(&report(Some(401), Some(200), Some(200)), None).state,
            State::Pass
        );
        let blocked = reach_item(&report(Some(403), Some(200), Some(200)), None);
        assert_eq!(blocked.state, State::Fail);
        assert!(blocked.detail.contains("403"));
        assert_eq!(
            reach_item(&report(None, None, None), None).state,
            State::Fail
        );
        assert_eq!(
            reach_item(&report(Some(401), Some(403), Some(200)), None).state,
            State::Warn,
            "网页被拦只降到警告"
        );
        assert_eq!(
            reach_item(&report(Some(500), Some(200), Some(200)), None).state,
            State::Unknown
        );
        // 直连放行、跟随系统代理那一路被拦：浏览器那边会出事。
        let proxied = report(Some(403), Some(200), Some(200));
        let it = reach_item(&report(Some(401), Some(200), Some(200)), Some(&proxied));
        assert_eq!(it.state, State::Warn);
        assert!(it.detail.contains("跟随系统代理"), "{}", it.detail);
    }

    fn resolved(host: &str, addrs: &[&str]) -> Resolved {
        Resolved {
            host: host.into(),
            addrs: addrs.iter().map(|s| s.to_string()).collect(),
            error: addrs.is_empty().then(|| "解析失败".to_string()),
        }
    }

    #[test]
    fn claude_dns_states() {
        assert_eq!(
            dns_item(&[
                resolved("claude.ai", &["160.79.104.10"]),
                resolved("api.anthropic.com", &["160.79.104.10"])
            ])
            .state,
            State::Pass,
            "实机 2026-09-24 的形状"
        );
        assert_eq!(
            dns_item(&[resolved("claude.ai", &["198.18.0.9"])]).state,
            State::Pass
        );
        assert_eq!(
            dns_item(&[resolved("claude.ai", &["127.0.0.1"])]).state,
            State::Fail
        );
        assert_eq!(
            dns_item(&[resolved("claude.ai", &["31.13.94.37"])]).state,
            State::Warn
        );
        assert_eq!(dns_item(&[resolved("claude.ai", &[])]).state, State::Fail);
        assert_eq!(dns_item(&[]).state, State::Unknown);
    }

    #[test]
    fn different_ips_without_country_evidence_are_not_declared_same_country() {
        let a = crate::probe::ip::Reading {
            ip: Some("203.0.113.7".into()),
            countries: Vec::new(),
        };
        let b = crate::probe::ip::Reading {
            ip: Some("203.0.113.8".into()),
            countries: vec![("test".into(), "US".into())],
        };
        assert_eq!(compare_egress(&a, &b).state, State::Unknown);
    }

    /// `CLAUDE_CODE_*` 整族都要认。0.19.2 之前只认死名字，
    /// 于是 `CLAUDE_CODE_USE_BEDROCK` 这种把 Claude Code 切到另一个
    /// 云端点的变量，这一项一个都查不到 —— 而 purge 和启动前的残留检查
    /// 早就认它了。同一件事三处口径不一样，最松的那处就是使用者看到的那处。
    #[test]
    fn the_claude_code_family_is_watched_by_prefix() {
        assert!(is_watched_env("CLAUDE_CODE_USE_BEDROCK"));
        assert!(is_watched_env("CLAUDE_CODE_USE_VERTEX"));
        assert!(is_watched_env("claude_code_something_new"), "大小写不敏感");
        assert!(is_watched_env("ANTHROPIC_BASE_URL"), "死名字那一档还在");
        assert!(!is_watched_env("CLAUDE"), "前缀要完整，不是沾边就算");
        assert!(!is_watched_env("PATH"));
    }

    /// 非 ASCII 的变量名不许把判定弄崩 —— 切片比较会切在半个字符上。
    #[test]
    fn a_non_ascii_env_name_does_not_panic() {
        assert!(!is_watched_env("变量"));
        assert!(!is_watched_env("é"));
    }

    #[test]
    fn finds_plain_secrets_in_json_and_toml() {
        assert_eq!(
            find_secret_fields(r#"{"mcpServers":{"x":{"env":{"API_KEY":"sk-abc123"}}}}"#),
            vec!["api_key".to_string()]
        );
        assert_eq!(
            find_secret_fields("[provider]\ntoken = \"tok_live_1\"\n"),
            vec!["token".to_string()]
        );
    }

    /// 空值不报。`"api_key": ""` 是「还没填」，不是「泄漏」。
    #[test]
    fn empty_or_absent_values_are_not_hits() {
        assert!(find_secret_fields(r#"{"api_key": ""}"#).is_empty());
        assert!(find_secret_fields(r#"{"api_key": null}"#).is_empty());
        assert!(find_secret_fields("just the word token in prose").is_empty());
    }

    #[test]
    fn each_field_name_is_reported_once() {
        let hits = find_secret_fields(r#"{"api_key":"a","other":{"api_key":"b"}}"#);
        assert_eq!(hits, vec!["api_key".to_string()]);
    }

    /// 结构上就装不下密钥值 —— 与 `relay::ProviderView` 同一条原则。
    /// 这个断言是防未来的人「顺手」加个 value 字段。
    #[test]
    fn secret_hits_carry_no_value_field() {
        let h = SecretHit {
            file: PathBuf::from("C:\\x\\.mcp.json"),
            field: "api_key".into(),
        };
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.contains("api_key"));
        assert!(!json.contains("value"), "SecretHit 不许带密钥内容：{json}");
    }

    // -------------------------------------------------- 环境变量掩码

    /// 核心不变量：**Key 的值一个字符都不许出现在输出里。**
    #[test]
    fn secret_env_values_never_appear() {
        for name in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "MY_SECRET",
            "X_PASSWORD",
        ] {
            let out = mask_env_value(name, "sk-ant-super-secret-123");
            assert!(!out.contains("sk-ant"), "{name} 把值漏出来了：{out}");
            assert!(!out.contains("123"), "{name} 把值漏出来了：{out}");
        }
    }

    /// BASE_URL 的**路径段里可能带 token**，所以只留 host。
    /// 这条挂了，Key 会被印在界面上、截图里、issue 里。
    #[test]
    fn base_url_keeps_only_the_origin() {
        assert_eq!(
            mask_env_value(
                "ANTHROPIC_BASE_URL",
                "https://relay.example.com/v1/sk-tok-abcd1234"
            ),
            "https://relay.example.com"
        );
        assert_eq!(
            mask_env_value(
                "ANTHROPIC_BASE_URL",
                "https://x.example.com:8443/v1?key=abc"
            ),
            "https://x.example.com:8443"
        );
    }

    /// `https://user:token@host` —— 凭证在 @ 前面，必须丢掉。
    #[test]
    fn credentials_in_the_authority_are_dropped() {
        let out = mask_env_value("HTTPS_PROXY", "http://alice:hunter2@proxy.example.com:7890");
        assert_eq!(out, "http://proxy.example.com:7890");
        assert!(!out.contains("hunter2"));
        assert!(!out.contains("alice"));
    }

    #[test]
    fn proxy_without_a_scheme_still_loses_the_path() {
        assert_eq!(
            mask_env_value("HTTP_PROXY", "127.0.0.1:7890"),
            "127.0.0.1:7890"
        );
        assert_eq!(
            mask_env_value("HTTP_PROXY", "user:pw@127.0.0.1:7890"),
            "127.0.0.1:7890"
        );
    }

    /// 模型名、配置目录不是秘密，原样显示才有用。
    #[test]
    fn harmless_values_are_shown_as_is() {
        assert_eq!(
            mask_env_value("ANTHROPIC_MODEL", "claude-sonnet-4-5"),
            "claude-sonnet-4-5"
        );
    }

    // -------------------------------------------------- reg query 解析

    /// 值里有空格 —— 所以不能 `split_whitespace` 取最后一段。
    #[test]
    fn reg_values_with_spaces_survive() {
        let text = concat!(
            r"HKEY_CURRENT_USER\Environment",
            "\n",
            r"    ANTHROPIC_BASE_URL    REG_SZ    https://x.com/v1",
            "\n",
            r"    FOO    REG_EXPAND_SZ    C:\Program Files\a b",
            "\n",
        );
        let got = parse_reg_values(text);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, "ANTHROPIC_BASE_URL");
        assert_eq!(got[0].1, "https://x.com/v1");
        assert_eq!(got[1].1, r"C:\Program Files\a b");
    }

    #[test]
    fn reg_header_line_is_not_a_value() {
        assert!(parse_reg_values(r"HKEY_CURRENT_USER\Environment").is_empty());
        assert!(parse_reg_values("").is_empty());
    }

    // -------------------------------------------------- 出口一致性

    fn reading(ip: Option<&str>, countries: &[(&str, &str)]) -> crate::probe::ip::Reading {
        crate::probe::ip::Reading {
            ip: ip.map(String::from),
            countries: countries
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
        }
    }

    #[test]
    fn same_egress_both_ways_passes() {
        let a = reading(Some("203.0.113.7"), &[("ippure", "US")]);
        assert_eq!(compare_egress(&a, &a).state, State::Pass);
    }

    /// 两条路出去的国家不一样 —— 这正是「面板说我在美国却被当成国内」的成因。
    #[test]
    fn different_country_is_a_failure_and_names_both() {
        let direct = reading(Some("203.0.113.7"), &[("ippure", "US")]);
        let proxied = reading(Some("198.51.100.9"), &[("ippure", "HK")]);
        let it = compare_egress(&direct, &proxied);
        assert_eq!(it.state, State::Fail);
        assert!(
            it.detail.contains("US") && it.detail.contains("HK"),
            "两个都要列出来：{}",
            it.detail
        );
    }

    /// 双栈机器常见：国家一样、IP 不同。是提醒，不是错误。
    #[test]
    fn same_country_different_ip_is_only_a_warning() {
        let direct = reading(Some("203.0.113.7"), &[("ippure", "US")]);
        let proxied = reading(Some("203.0.113.8"), &[("ippure", "US")]);
        assert_eq!(compare_egress(&direct, &proxied).state, State::Warn);
    }

    /// 有一条路探不到 → Unknown，**不是 Pass**。
    /// 「比不了」和「一致」是两回事，合并了就会在真有问题时报绿灯。
    #[test]
    fn missing_one_side_is_unknown_not_pass() {
        let direct = reading(Some("203.0.113.7"), &[("ippure", "US")]);
        let nothing = reading(None, &[]);
        assert_eq!(compare_egress(&direct, &nothing).state, State::Unknown);
        assert_eq!(compare_egress(&nothing, &direct).state, State::Unknown);
    }

    #[test]
    fn unknown_is_not_pass() {
        // 「没扫到文件」和「扫了很干净」必须是两种结论。
        let it = item("x", "x", State::Unknown, "");
        assert_ne!(it.state, State::Pass);
    }
}
