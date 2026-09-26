//! DNS 泄露检测（简易通过）。
//!
//! 两条腿走路，缺一条都会漏：
//!
//! **① 真实解析测试**（主）——借 bash.ws 的权威 NS 回显。
//!   做法：向 bash.ws 取一个测试 id（**必须由它签发**，见 [`obtain_test_id`]），逐个解析
//!   `1..10.<id>.bash.ws`，bash.ws 的权威域名服务器会记录**是谁来查的**，再拉
//!   `https://bash.ws/dnsleak/test/<id>?json` 把解析器清单读回来。
//!   这是 dnsleaktest.com 那一套的公开接口，不需要自建服务器。
//!   调用时序照 ygbull/DNSLeakTester 校准（代码没抄，见 ATTRIBUTION）。
//!
//!   ⛔ **回显的形状不假设**（2026-09-25）：有使用者那边整份回的是一个 JSON 对象，原来的
//!   `from_str::<Vec<_>>` 报「JSON 解析失败: invalid type: map, expected a sequence」，整次检测作废 ——
//!   GitHub 上发过的每一版都是这一句，跟版本无关。怎么认见 [`parse_bash_ws`]；
//!   换了机器又报「回的不是解析器清单」时，先跑 `cargo run -p qb-probe --example dns-leak` 看形状。
//!
//!   探针域名被代理的 fake-ip 接住（解析成 198.18.x.x）时，查询根本没从本机发出去，bash.ws 自然
//!   收不到回显 —— 那不是「查不到」，报告里单独说（[`no_echo_finding`]）。
//!
//! **② 网卡配置检查**（辅）——TUN 开着时，看**连着的**物理网卡上配的 DNS，
//!   查询是进隧道还是从这张网卡直接出去（`Find-NetRoute`）。Windows 会同时向每张网卡的
//!   DNS 发查询，直接出去的那一路就绕过了隧道 —— ① 有时看不见，但它正是 TUN 开启期间
//!   防泄漏边界的缺口。没开 TUN 时没有隧道可绕，这一项不适用。
//!
//! # 评分（2026-09-24 改，使用者：「合理化分数评分机制，只连 wifi 就不需要其他的评分了吧」）
//!
//! 两项，**各自适用才计分**（不适用的从分子分母里一起去掉，跟 `score.ts` 同一条）：
//! ① 里没有国内解析器 70 分（实测到的泄露）；② 连着的物理网卡上的 DNS 都走隧道 30 分（配置层的缺口）。
//! 原来是「以太网安全 50 + 无国内 DNS 30 + 证据完整 20」，三处不合理：
//! - 按网卡**名字**认以太网 —— 只连 Wi-Fi、或者系统不是中英文的，那 50 分永远拿不到；
//! - 断开的网卡（上面还挂着路由器的 DNS）、指向**隧道自己 DNS** 的物理网卡（v2rayN / xray 的 TUN
//!   会把物理网卡的 DNS 改成隧道网段里的 `172.x`）都被当成「指向内网」扣分 —— 按地址是不是内网判，
//!   不如直接问这条查询从哪张网卡出去；
//! - 「证据完整」给的是「有没有数据」，不是「安不安全」—— 没有回显时 `score.ts` 本来就整项不计。
//!
//! 两者都是**简易通过**。深度排查（抓包、WebRTC、路由）交给高级通过的 Codex。

use super::reach::{classify_ip, DnsClass};
use crate::error::{GateError, Result};
use crate::sink::ProgressSink;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::net::IpAddr;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Resolver {
    pub address: String,
    pub country_code: Option<String>,
    pub country_name: Option<String>,
    pub asn: Option<String>,
    /// 来自网卡配置而非真实解析回显。
    pub from_adapter: bool,
    pub interface: Option<String>,
    /// 网卡配置那一行来自**非硬件网卡**（TUN / TAP / WireGuard / VPN），读自
    /// `Get-NetAdapter` 的 `HardwareInterface`（2026-09-24）。原来按网卡名里有没有 `tun`
    /// 认 —— 那是本机 `xray_tun` 的名字，别人的隧道可能叫 `Clash`、`Meta`、`wg0`。
    pub tunnel: bool,
    /// 网卡配置那一行：这张网卡此刻连着（`InterfaceOperationalStatus` = Up）。
    /// 断开的网卡上还挂着上次的 DNS（比如断开的 WLAN 上的路由器地址），用不上，不参与判定。
    /// 回显那几行恒为 `true` —— 它们就是实际发生的查询。
    pub connected: bool,
    /// 网卡配置那一行：发往这个解析器的查询从哪儿出去（`Find-NetRoute`）——
    /// `Some(true)` 进隧道，`Some(false)` 从物理网卡直接出去，`None` 查不出。回显那几行是 `None`。
    pub via_tunnel: Option<bool>,
    pub is_private: bool,
    pub is_domestic: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DnsReport {
    /// 出口 IP 的 ASN，用于和解析器 ASN 比对。
    pub egress_asn: Option<String>,
    pub resolvers: Vec<Resolver>,
    pub passed: bool,
    pub findings: Vec<String>,
    /// bash.ws 自己给的结论行，原样透出。
    pub upstream_conclusion: Option<String>,
    pub note: &'static str,
    /// 100 分制（算法见文件头）。`None` = 没收到真实解析回显，判不了有没有泄露。
    pub score: Option<u8>,
    /// 连着的物理网卡上配的 DNS 都走隧道吗。`None` = 这一项不适用：没开 TUN、查不出路由、
    /// 或者没有连着的物理网卡配了 DNS。**不分有线还是 Wi-Fi** —— 原来的 `ethernet_safe`
    /// 按名字只认「以太网」，只连 Wi-Fi 的机器永远是「未检测到以太网」。
    pub adapters_safe: Option<bool>,
    /// 这一项看了哪几张网卡、或者为什么不适用。界面原样显示。
    pub adapters_note: String,
}

/// bash.ws 回显清单里的一条。
///
/// 字段一律宽容读（[`lenient`]）：它是 PHP 写的，查不到时给 `false`（参考客户端里就判
/// `country_name != false`），ASN 也可能是个数。直接用 `Option<String>` 读的话，
/// **一个 `false` 就让整份清单解析失败** —— 那是另一种「JSON 解析失败」。
#[derive(Debug, Deserialize)]
struct BashWsEntry {
    #[serde(default, deserialize_with = "lenient")]
    ip: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    country_name: Option<String>,
    #[serde(default, rename = "country", deserialize_with = "lenient")]
    country_code: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    asn: Option<String>,
    /// "ip" = 你的出口地址；"dns" = 一台解析器；"conclusion" = 结论行
    #[serde(default, rename = "type", deserialize_with = "lenient")]
    kind: Option<String>,
}

/// 字符串照收（去掉首尾空白，空串当没有），数字转成字符串，`false` / `null` / 别的一律当没有。
fn lenient<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
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

/// 系统解析器把 10 个探针域名解析成了什么 —— **只数个数，不留地址**。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Lookups {
    /// 有回答（解析出至少一个地址）的探针个数。
    pub answered: u8,
    /// 其中地址**全**落在代理 fake-ip 段的个数（198.18.0.0/15、fc00::/7，
    /// 判法跟体检的「claude.ai 的解析」是同一份：[`classify_ip`]）。
    pub fake_ip: u8,
}

impl Lookups {
    /// 记下一个探针的回答。
    fn note(&mut self, addrs: &[IpAddr]) {
        if addrs.is_empty() {
            return;
        }
        self.answered += 1;
        if addrs.iter().all(|a| classify_ip(*a) == DnsClass::FakeIp) {
            self.fake_ip += 1;
        }
    }

    /// 有回答的探针全是 fake-ip：系统解析器被代理接管了，查询没从本机发出去。
    pub fn all_fake_ip(&self) -> bool {
        self.answered > 0 && self.fake_ip == self.answered
    }
}

/// 逐个解析探针域名，触发权威 NS 记录「谁来查的」。
///
/// 两个要点：
///   - 走 `lookup_host` 而不是自己发 UDP。必须用**操作系统的解析路径**，
///     自己发包就绕过了要检测的那条链路，测了个寂寞。
///   - 串行 + 间隔 200ms。并发打十个查询容易被解析器合并或限流，
///     回显清单会不全。
///
/// 解析结果只用来数 fake-ip（[`Lookups`]），地址本身不留。
async fn trigger_probes(id: &str, rep: &dyn ProgressSink) -> Lookups {
    let mut seen = Lookups::default();
    for i in 1..=PROBE_COUNT {
        // 十个域名逐个报，界面上才看得出它在动 —— 整套要六秒多，
        // 一动不动的六秒和卡死没法区分。
        rep.phase(
            u32::from(i),
            &format!("解析第 {i} / {PROBE_COUNT} 个探针域名"),
        );
        let host = format!("{i}.{id}.bash.ws:80");
        if let Ok(Ok(addrs)) = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            tokio::net::lookup_host(host),
        )
        .await
        {
            seen.note(&addrs.map(|a| a.ip()).collect::<Vec<_>>());
        }
        tokio::time::sleep(std::time::Duration::from_millis(DELAY_BETWEEN_PROBES_MS)).await;
    }
    seen
}

/// 取回解析器清单的**原文**。怎么读交给 [`read_echo`] —— 诊断例子要看的正是原文的形状。
async fn fetch_results(id: &str) -> Result<String> {
    let c = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let url = format!("https://bash.ws/dnsleak/test/{id}?json");
    Ok(c.get(&url).send().await?.error_for_status()?.text().await?)
}

/// 回显那一半：向 bash.ws 取测试 id、用系统解析器解析 10 个探针域名、等权威 NS 记账、取回原文。
///
/// [`check`] 与诊断例子（`examples/dns-leak.rs`）走的是这同一条路。**会联网。**
pub async fn fetch_echo(rep: &dyn ProgressSink) -> Result<(String, Lookups)> {
    rep.phase(0, "向 bash.ws 取测试 id");
    let id = obtain_test_id().await?;
    let lookups = trigger_probes(&id, rep).await;

    // 权威 NS 记账有延迟，等满 3 秒再取，否则常常读到半截清单。
    rep.phase(PROBE_COUNT.into(), "等权威域名服务器记账（3 秒）");
    tokio::time::sleep(std::time::Duration::from_millis(WAIT_AFTER_PROBES_MS)).await;

    rep.phase(PROBE_COUNT.into(), "取回解析器清单");
    Ok((fetch_results(&id).await?, lookups))
}

/// bash.ws 回显原文 → 一条条记录。**不假设它一定是数组**（2026-09-25）。
///
/// 平时是 `[{…}, {…}]`。有使用者那边整份回的是一个对象（报错原文
/// `invalid type: map, expected a sequence at line 1 column 0`），原来的 `from_str::<Vec<_>>`
/// 于是整次作废，连网卡那一半也跟着丢了。它是 PHP 写的，能想到的对象形状两种，都认：
///
/// - 下标不连续的数组被 `json_encode` 编成 `{"0":{…},"2":{…}}` —— 每个值都是一条记录，
///   按**数字**键排回原来的顺序（serde_json 的 Map 按字符串排，"10" 会跑到 "2" 前面）；
/// - 只有一条记录时直接给了那一条（对象自己带 `type`）。
///
/// `{}` 当空清单：后面照旧落到「没收到回显，判不了」，不当成通过。
/// 别的形状（错误回执之类）如实报「回的不是解析器清单」，**只说形状、不带值** ——
/// 回执里可能有使用者的出口 IP，而这句话会被截图、贴进 issue（跟 `SecretHit` 同一个原则）。
fn parse_bash_ws(text: &str) -> Result<Vec<BashWsEntry>> {
    let body = text.trim_start_matches('\u{feff}').trim();
    let value: Value = serde_json::from_str(body).map_err(|_| {
        GateError::Other(format!(
            "bash.ws 这次回的不是 JSON（{}），判不了有没有泄露。多半是它临时出错或者被拦了，过几分钟再测。",
            describe_text(body)
        ))
    })?;
    let items: Vec<Value> = match value {
        Value::Array(items) => items,
        Value::Object(map) if map.is_empty() => Vec::new(),
        Value::Object(map) if map.contains_key("type") => vec![Value::Object(map)],
        Value::Object(map) if map.values().all(Value::is_object) => {
            let mut rows: Vec<(String, Value)> = map.into_iter().collect();
            // 稳定排序：不是数字的键排在最后，彼此之间保持原来的次序。
            rows.sort_by_key(|(k, _)| k.parse::<u64>().unwrap_or(u64::MAX));
            rows.into_iter().map(|(_, v)| v).collect()
        }
        other => return Err(not_a_list(&other)),
    };
    // 数组里混着不是记录的东西：不猜它是什么，照实报形状。
    if !items.iter().all(Value::is_object) {
        return Err(not_a_list(&Value::Array(items)));
    }
    items
        .into_iter()
        .map(|v| serde_json::from_value(v).map_err(GateError::from))
        .collect()
}

fn not_a_list(v: &Value) -> GateError {
    GateError::Other(format!(
        "bash.ws 这次回的不是解析器清单（{}），判不了有没有泄露。多半是它限流或临时出错，过几分钟再测；\
         一直这样的话，请把这句话原样贴进 issue。",
        shape_of(v)
    ))
}

/// 只描述形状：顶层类型、前几个键名与值的类型。**不带任何值。**
fn shape_of(v: &Value) -> String {
    match v {
        Value::Object(map) => {
            let keys: Vec<String> = map
                .iter()
                .take(6)
                .map(|(k, v)| format!("{} {}", key_label(k), kind_of(v)))
                .collect();
            let more = if map.len() > 6 {
                format!(" 等 {} 个键", map.len())
            } else {
                String::new()
            };
            format!("对象：{}{more}", keys.join("、"))
        }
        Value::Array(items) => {
            let odd = items.iter().find(|x| !x.is_object()).map_or("?", kind_of);
            format!("数组，{} 项，里面混着{odd}", items.len())
        }
        other => kind_of(other).to_string(),
    }
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "布尔",
        Value::Number(_) => "数字",
        Value::String(_) => "字符串",
        Value::Array(_) => "数组",
        Value::Object(_) => "对象",
    }
}

/// 键名原样给出，但**像 IP 的打码**、太长的截断 —— 键名本身也可能是使用者的地址。
fn key_label(k: &str) -> String {
    if k.parse::<IpAddr>().is_ok() {
        return "<IP>".into();
    }
    let mut s: String = k.chars().take(24).collect();
    if k.chars().count() > 24 {
        s.push('…');
    }
    s
}

/// 不是 JSON 时只说有多长、第一个字符是什么（`<` 多半是一张网页）—— 正文不带。
fn describe_text(body: &str) -> String {
    match body.chars().next() {
        None => "空的".into(),
        Some(c) => format!(
            "{} 个字符，开头是「{}」",
            body.chars().count(),
            if c.is_ascii_graphic() { c } else { '?' }
        ),
    }
}

/// 从 bash.ws 回显里读出来的那一份。
#[derive(Debug, Clone, Default)]
pub struct Echo {
    /// 出口 IP 的 ASN。
    pub egress_asn: Option<String>,
    /// bash.ws 自己给的结论行。
    pub conclusion: Option<String>,
    /// 回显里的解析器（`from_adapter = false`）。
    pub resolvers: Vec<Resolver>,
}

/// bash.ws 原文 → 出口 ASN、结论与解析器清单。**纯函数**，形状怎么认见 [`parse_bash_ws`]。
pub fn read_echo(text: &str) -> Result<Echo> {
    let mut echo = Echo::default();
    for e in parse_bash_ws(text)? {
        match e.kind.as_deref().map(str::to_ascii_lowercase).as_deref() {
            Some("ip") => echo.egress_asn = e.asn,
            // 参考客户端的结论文字在 `ip` 字段里；`country_name` 有字时仍以它为先（原来的读法）。
            Some("conclusion") => echo.conclusion = e.country_name.or(e.ip),
            Some("dns") => {
                let Some(ip) = e.ip else { continue };
                let cc = e.country_code.map(|c| c.to_ascii_uppercase());
                echo.resolvers.push(Resolver {
                    is_private: is_private(&ip),
                    is_domestic: cc.as_deref() == Some("CN"),
                    address: ip,
                    country_code: cc,
                    country_name: e.country_name,
                    asn: e.asn,
                    from_adapter: false,
                    interface: None,
                    tunnel: false,
                    connected: true,
                    via_tunnel: None,
                });
            }
            _ => {}
        }
    }
    Ok(echo)
}

/// 网卡配置里的一个解析器。
#[derive(Debug, Clone, PartialEq, Eq)]
struct AdapterDns {
    name: String,
    /// 非硬件网卡（`HardwareInterface = False`）。
    tunnel: bool,
    /// 网卡连着（`InterfaceOperationalStatus = 1`，即 Up）。用数字不用 `Status` 那串字，不看系统语言。
    connected: bool,
    /// 发往这个解析器的查询从隧道出去吗（`Find-NetRoute` 选中的那张网卡是不是非硬件网卡）。
    /// `None` = 查不出（没有路由、选中的是回环这类 `Get-NetAdapter` 不认的接口）。
    via_tunnel: Option<bool>,
    ip: String,
}

/// 网卡那一层读出来的全部。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct AdapterScan {
    /// 发往一个普通公网地址的查询从隧道出去 = TUN 接管了默认路由。`None` = 查不出。
    tun_active: Option<bool>,
    rows: Vec<AdapterDns>,
}

/// 探默认路由用的地址：RFC 5737 的文档段（TEST-NET-3），谁都不会给它单独配路由，
/// 查到的就是默认路由那张网卡。不用 1.1.1.1 / 8.8.8.8 —— 代理常给这几个 DNS 单独配 /32，
/// 量到的就不是默认路由了。
const ROUTE_PROBE: &str = "203.0.113.1";

/// 网卡 DNS 配置。只读操作，走 PowerShell 比 GetAdaptersAddresses FFI 省两百行，
/// 且没有安全影响。（ACL 那边不一样 —— 那是管控点，必须用 API，见 gate/acl.rs。）
///
/// 第一行 `#tun \t 默认路由走隧道(1/0/-)`，之后每行
/// `网卡名 \t 非硬件网卡(1/0) \t 连着(1/0) \t 这条查询走隧道(1/0/-) \t 解析器地址`。
/// 网卡性质按 `HardwareInterface` 认（不按名字），查不到那张网卡的按物理网卡算 —— 宁可多看一张，不漏。
#[cfg(windows)]
async fn adapter_resolvers() -> Result<AdapterScan> {
    let out = crate::process::powershell_tokio(&adapter_script())
        .output()
        .await?;
    if !out.status.success() {
        return Err(crate::error::GateError::Other(
            "网卡 DNS 配置读取失败，未判定为无泄露".into(),
        ));
    }
    Ok(parse_adapter_lines(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(not(windows))]
async fn adapter_resolvers() -> Result<AdapterScan> {
    Ok(AdapterScan::default())
}

/// 读网卡那一层的 PowerShell。单独拿出来是为了单测看得见它的形状 —— 输出格式跟
/// [`parse_adapter_lines`] 是一对，改一边忘一边，面板就会安安静静地读到空清单。
/// 连着没连着用 `InterfaceOperationalStatus`（1 = Up）这个数，不用 `Status` 那串字。
#[cfg_attr(not(windows), allow(dead_code))]
fn adapter_script() -> String {
    format!(
        "$ErrorActionPreference = 'Stop'; $hw = @{{}}; $up = @{{}}; \
         Get-NetAdapter -IncludeHidden -ErrorAction SilentlyContinue | \
         ForEach-Object {{ $hw[$_.InterfaceAlias] = [bool]$_.HardwareInterface; \
         $up[$_.InterfaceAlias] = ($_.InterfaceOperationalStatus -eq 1) }}; \
         function Via($ip) {{ try {{ $a = (Find-NetRoute -RemoteIPAddress $ip -ErrorAction Stop | \
         Select-Object -First 1).InterfaceAlias; \
         if (-not $hw.ContainsKey($a)) {{ '-' }} elseif ($hw[$a]) {{ '0' }} else {{ '1' }} }} \
         catch {{ '-' }} }}; \
         \"#tun`t$(Via '{ROUTE_PROBE}')\"; \
         Get-DnsClientServerAddress -AddressFamily IPv4 | \
         Where-Object {{ $_.ServerAddresses.Count -gt 0 }} | \
         ForEach-Object {{ $n = $_.InterfaceAlias; $t = 0; \
         if ($hw.ContainsKey($n) -and -not $hw[$n]) {{ $t = 1 }}; $u = 0; if ($up[$n]) {{ $u = 1 }}; \
         foreach ($s in $_.ServerAddresses) {{ \"$n`t$t`t$u`t$(Via $s)`t$s\" }} }}"
    )
}

fn flag(s: &str) -> Option<bool> {
    match s.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

fn parse_adapter_lines(text: &str) -> AdapterScan {
    let mut scan = AdapterScan::default();
    for l in text.lines() {
        if let Some(v) = l.strip_prefix("#tun\t") {
            scan.tun_active = flag(v);
            continue;
        }
        let f: Vec<&str> = l.splitn(5, '\t').collect();
        let [name, tunnel, connected, via, ip] = f[..] else {
            continue;
        };
        let ip = ip.trim();
        if ip.is_empty() {
            continue;
        }
        scan.rows.push(AdapterDns {
            name: name.trim().to_string(),
            tunnel: tunnel.trim() == "1",
            connected: connected.trim() == "1",
            via_tunnel: flag(via),
            ip: ip.to_string(),
        });
    }
    scan
}

pub async fn check(rep: &dyn ProgressSink) -> Result<DnsReport> {
    let (text, lookups) = fetch_echo(rep).await?;
    let Echo {
        egress_asn,
        conclusion: upstream_conclusion,
        mut resolvers,
    } = read_echo(&text)?;

    // 补上网卡配置里的解析器（同一张网卡上的同一个地址只留一条）。
    let scan = adapter_resolvers().await?;
    for a in scan.rows {
        if resolvers
            .iter()
            .any(|r| r.from_adapter && r.address == a.ip && r.interface.as_deref() == Some(&a.name))
        {
            continue;
        }
        resolvers.push(Resolver {
            is_private: is_private(&a.ip),
            is_domestic: false,
            address: a.ip,
            country_code: None,
            country_name: None,
            asn: None,
            from_adapter: true,
            interface: Some(a.name),
            tunnel: a.tunnel,
            connected: a.connected,
            via_tunnel: a.via_tunnel,
        });
    }

    let mut findings = evaluate(&resolvers, scan.tun_active);
    if !resolvers.iter().any(|r| !r.from_adapter) {
        findings.push(no_echo_finding(lookups));
    }
    let (score, adapters_safe) = score_report(&resolvers, scan.tun_active);
    let adapters_note = adapters_note(&resolvers, scan.tun_active);

    Ok(DnsReport {
        passed: findings.is_empty() && !resolvers.is_empty(),
        egress_asn,
        upstream_conclusion,
        note: "简易检测覆盖真实解析回显与网卡配置两层。\
               应用层自带解析器、DoH 直连等情况仍看不到；\
               深度排查请用高级通过让 Codex 抓包核实。",
        resolvers,
        findings,
        score,
        adapters_safe,
        adapters_note,
    })
}

/// 没收到真实解析回显时报的那一句。
///
/// 探针全被代理的 fake-ip 接住是另一件事（使用者 2026-09-25 要的说明）：系统解析器根本没把查询
/// 发出去，bash.ws 当然收不到 —— 判不了，但也不是「查不到」。分数照旧是「—」、照旧不算通过：
/// 代理那一头拿哪台解析器去解析，这里看不见。
fn no_echo_finding(l: Lookups) -> String {
    if l.all_fake_ip() {
        "没收到 bash.ws 的真实解析回显：探针域名全被解析成了代理的 fake-ip 地址（198.18.x.x 这类），\
         系统解析器的查询被代理接住、没有从本机发出去，所以这里判不了，也不会从本机这一路泄露。\
         代理那一头用哪台解析器这里看不到，要查就用高级通过。"
            .into()
    } else {
        "未收到权威服务器的真实解析回显，不能判定 DNS 无泄露".into()
    }
}

/// 真实解析（回显）那一项占的分。它是实测到的泄露，所以占大头。
const ECHO_WEIGHT: u32 = 70;
/// 网卡配置那一项占的分。它是「可能」—— 隧道软件拦不拦得住那一路，只有回显说了算。
const ADAPTER_WEIGHT: u32 = 30;

/// 网卡配置那一项要看的行：连着的物理网卡上配的 DNS。断开的、隧道网卡自己的都不看。
fn uplink_rows(resolvers: &[Resolver]) -> impl Iterator<Item = &Resolver> {
    resolvers
        .iter()
        .filter(|r| r.from_adapter && !r.tunnel && r.connected)
}

/// 连着的物理网卡上配的 DNS 都走隧道吗。`None` = 不适用：
/// 没开 TUN（没有隧道可绕）、查不出默认路由、没有这样的网卡，或者每一条都查不出路由。
fn adapters_verdict(resolvers: &[Resolver], tun_active: Option<bool>) -> Option<bool> {
    if tun_active != Some(true) {
        return None;
    }
    let rows: Vec<&Resolver> = uplink_rows(resolvers).collect();
    if rows.iter().any(|r| r.via_tunnel == Some(false)) {
        return Some(false);
    }
    rows.iter()
        .any(|r| r.via_tunnel == Some(true))
        .then_some(true)
}

/// 100 分制，两项各自**适用才计分**（见文件头）：只有回显时是 100 或 0，两项都适用时
/// 100 / 70 / 30 / 0。**没有回显就判不了有没有泄露**，分数是 `None`（界面「—」），不是 0 ——
/// 跟 `score.ts` 的「没检测过显示 — 而不是 0」同一条；那时 `findings` 里有「不能判定」。
fn score_report(resolvers: &[Resolver], tun_active: Option<bool>) -> (Option<u8>, Option<bool>) {
    let adapters_ok = adapters_verdict(resolvers, tun_active);
    let echo: Vec<&Resolver> = resolvers.iter().filter(|r| !r.from_adapter).collect();
    if echo.is_empty() {
        return (None, adapters_ok);
    }
    let (mut got, mut total) = (0u32, ECHO_WEIGHT);
    if echo.iter().all(|r| !r.is_domestic) {
        got += ECHO_WEIGHT;
    }
    if let Some(ok) = adapters_ok {
        total += ADAPTER_WEIGHT;
        if ok {
            got += ADAPTER_WEIGHT;
        }
    }
    (Some(((got * 100 + total / 2) / total) as u8), adapters_ok)
}

/// 网卡配置那一项看了什么、或者为什么不适用。界面原样显示。
fn adapters_note(resolvers: &[Resolver], tun_active: Option<bool>) -> String {
    let names = |pick: &dyn Fn(&Resolver) -> bool| {
        let mut v: Vec<&str> = Vec::new();
        for n in resolvers
            .iter()
            .filter(|r| r.from_adapter && !r.tunnel && pick(r))
            .filter_map(|r| r.interface.as_deref())
        {
            if !v.contains(&n) {
                v.push(n);
            }
        }
        v.join("、")
    };
    let skipped = names(&|r| !r.connected);
    let skipped = if skipped.is_empty() {
        String::new()
    } else {
        format!("断开的 {skipped} 不看。")
    };
    match tun_active {
        Some(false) => {
            format!("没开 TUN（默认路由不走隧道），没有隧道可绕，这一项不计分。{skipped}")
        }
        None => format!("查不出默认路由走哪张网卡，这一项不计分。{skipped}"),
        Some(true) => {
            let checked = names(&|r| r.connected);
            if checked.is_empty() {
                return format!("没有连着的物理网卡配了 DNS，这一项不计分。{skipped}");
            }
            let verdict = match adapters_verdict(resolvers, tun_active) {
                Some(true) => "都走隧道",
                Some(false) => "有不走隧道的",
                None => "查不出走哪张网卡，不计分",
            };
            format!("看了 {checked} 上配的 DNS：{verdict}。{skipped}")
        }
    }
}

/// 判定逻辑抽出来，便于单测。
fn evaluate(resolvers: &[Resolver], tun_active: Option<bool>) -> Vec<String> {
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
    }
    // TUN 开着时，连着的物理网卡上的 DNS 查询不走隧道 —— 这就是防泄漏边界的缺口。
    // 不看地址是不是内网：隧道软件常把物理网卡的 DNS 改成隧道网段里的 172.x，那是对的；
    // 也不看断开的网卡：它上面挂着的路由器地址用不上。
    if tun_active == Some(true) {
        for r in uplink_rows(resolvers).filter(|r| r.via_tunnel == Some(false)) {
            findings.push(format!(
                "{} 的 DNS {} 不走隧道：Windows 会同时向每张网卡的 DNS 发查询，这一路绕过了隧道",
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

    /// 回显那一行（bash.ws 看到的真实解析器）。
    fn echo(addr: &str, domestic: bool) -> Resolver {
        Resolver {
            is_private: is_private(addr),
            is_domestic: domestic,
            address: addr.into(),
            country_code: domestic.then(|| "CN".into()),
            country_name: None,
            asn: None,
            from_adapter: false,
            interface: None,
            tunnel: false,
            connected: true,
            via_tunnel: None,
        }
    }

    /// 网卡配置那一行。名字随便起 —— 判定不许看名字。
    fn nic(iface: &str, addr: &str, connected: bool, via_tunnel: Option<bool>) -> Resolver {
        Resolver {
            from_adapter: true,
            interface: Some(iface.into()),
            connected,
            via_tunnel,
            ..echo(addr, false)
        }
    }

    fn tunnel_nic(iface: &str, addr: &str) -> Resolver {
        Resolver {
            tunnel: true,
            ..nic(iface, addr, true, Some(true))
        }
    }

    const TUN_ON: Option<bool> = Some(true);
    const TUN_OFF: Option<bool> = Some(false);

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
        let r = vec![echo("1.1.1.1", false), echo("8.8.8.8", false)];
        assert!(evaluate(&r, TUN_ON).is_empty());
        assert_eq!(score_report(&r, TUN_ON), (Some(100), None));
    }

    #[test]
    fn domestic_resolver_is_flagged() {
        let r = vec![echo("114.114.114.114", true)];
        let f = evaluate(&r, None);
        assert_eq!(f.len(), 1);
        assert!(f[0].contains("国内"));
        assert_eq!(score_report(&r, None).0, Some(0));
    }

    #[test]
    fn empty_result_is_not_a_pass() {
        let f = evaluate(&[], None);
        assert_eq!(f.len(), 1, "读不到解析器要说无法判定，不能当通过");
    }

    // ---- 网卡那一项：看查询从哪张网卡出去，不看名字、不看地址是不是内网

    #[test]
    fn an_uplink_whose_dns_leaves_outside_the_tunnel_is_flagged() {
        // 连着的 Wi-Fi 配着路由器的 DNS，那条路由不进隧道：Windows 会同时问它，绕过隧道。
        let r = vec![
            echo("1.1.1.1", false),
            nic("WLAN", "192.168.1.1", true, Some(false)),
        ];
        let f = evaluate(&r, TUN_ON);
        assert_eq!(f.len(), 1);
        assert!(f[0].contains("WLAN") && f[0].contains("不走隧道"), "{f:?}");
        assert_eq!(score_report(&r, TUN_ON), (Some(70), Some(false)));
    }

    #[test]
    fn the_tunnels_own_dns_on_a_physical_adapter_is_fine() {
        // 本机实测（2026-09-24）：xray 的 TUN 把有线网卡的 DNS 改成隧道网段里的 172.18.0.2，
        // `Find-NetRoute` 说它走 xray_tun。地址是内网，查询却进了隧道 —— 原来按地址判，这里被误报。
        let r = vec![
            echo("1.1.1.1", false),
            nic("以太网", "172.18.0.2", true, Some(true)),
            tunnel_nic("xray_tun", "8.8.8.8"),
        ];
        assert!(evaluate(&r, TUN_ON).is_empty());
        assert_eq!(score_report(&r, TUN_ON), (Some(100), Some(true)));
    }

    #[test]
    fn a_disconnected_adapter_is_not_looked_at() {
        // 断开的 WLAN 上还挂着路由器的 DNS（本机实测也是这样）—— 用不上，不报、不扣分。
        let r = vec![
            echo("1.1.1.1", false),
            nic("以太网", "172.18.0.2", true, Some(true)),
            nic("WLAN", "192.168.10.1", false, Some(false)),
        ];
        assert!(evaluate(&r, TUN_ON).is_empty());
        assert_eq!(score_report(&r, TUN_ON), (Some(100), Some(true)));
        assert!(adapters_note(&r, TUN_ON).contains("断开的 WLAN 不看"));
    }

    #[test]
    fn wifi_only_needs_no_ethernet() {
        // 使用者：「只连 wifi 就不需要其他的评分了吧」。没有有线网卡也能拿满分 ——
        // 原来按名字找「以太网」，找不到那 50 分就永远拿不到。
        let r = vec![
            echo("1.1.1.1", false),
            nic("WLAN", "10.8.0.1", true, Some(true)),
        ];
        assert_eq!(score_report(&r, TUN_ON), (Some(100), Some(true)));
        // 名字里带 ethernet / tun 都不算数：认的是查询走哪张网卡。
        let named = vec![
            echo("1.1.1.1", false),
            nic("tunable-ethernet", "192.168.1.1", true, Some(false)),
        ];
        assert_eq!(evaluate(&named, TUN_ON).len(), 1);
    }

    #[test]
    fn without_tun_the_adapter_item_does_not_apply() {
        // 没开 TUN：没有隧道可绕，路由器 DNS 是正常的 —— 这一项不计分，只按回显算。
        let r = vec![
            echo("1.1.1.1", false),
            nic("WLAN", "192.168.1.1", true, Some(false)),
        ];
        assert!(evaluate(&r, TUN_OFF).is_empty());
        assert_eq!(score_report(&r, TUN_OFF), (Some(100), None));
        assert!(adapters_note(&r, TUN_OFF).contains("没开 TUN"));
        // 查不出默认路由也一样不计分。
        assert_eq!(score_report(&r, None), (Some(100), None));
    }

    #[test]
    fn unknown_routes_do_not_count_either_way() {
        let r = vec![
            echo("1.1.1.1", false),
            nic("以太网", "127.0.0.1", true, None),
        ];
        assert!(evaluate(&r, TUN_ON).is_empty());
        assert_eq!(score_report(&r, TUN_ON), (Some(100), None));
    }

    #[test]
    fn a_tunnel_adapters_own_dns_is_never_judged() {
        let r = vec![
            echo("1.1.1.1", false),
            tunnel_nic("随便起的名字", "172.19.0.2"),
        ];
        assert!(evaluate(&r, TUN_ON).is_empty());
        assert_eq!(score_report(&r, TUN_ON), (Some(100), None));
    }

    // ---- 分数

    #[test]
    fn the_real_leak_outweighs_the_configuration() {
        // 回显里有国内解析器（真漏了）比网卡配置不对扣得多。
        let leak = vec![
            echo("114.114.114.114", true),
            nic("以太网", "172.18.0.2", true, Some(true)),
        ];
        assert_eq!(score_report(&leak, TUN_ON), (Some(30), Some(true)));
        let both = vec![
            echo("114.114.114.114", true),
            nic("WLAN", "192.168.1.1", true, Some(false)),
        ];
        assert_eq!(score_report(&both, TUN_ON), (Some(0), Some(false)));
    }

    #[test]
    fn no_echo_means_no_score_not_zero() {
        // 没收到回显就判不了有没有泄露：分数是「—」，不是 0，也不是只看网卡那一项的 100。
        let r = vec![nic("以太网", "172.18.0.2", true, Some(true))];
        assert_eq!(score_report(&r, TUN_ON), (None, Some(true)));
    }

    #[test]
    fn adapter_lines_carry_route_and_link_state() {
        let got = parse_adapter_lines(
            "#tun\t1\r\n以太网\t0\t1\t1\t172.18.0.2\nWLAN\t0\t0\t-\t192.168.10.1\n\
             xray_tun\t1\t1\t1\t8.8.8.8\nbad line\n以太网 2\t0\t1\t0\t\n",
        );
        assert_eq!(got.tun_active, Some(true));
        assert_eq!(
            got.rows,
            vec![
                AdapterDns {
                    name: "以太网".into(),
                    tunnel: false,
                    connected: true,
                    via_tunnel: Some(true),
                    ip: "172.18.0.2".into(),
                },
                AdapterDns {
                    name: "WLAN".into(),
                    tunnel: false,
                    connected: false,
                    via_tunnel: None,
                    ip: "192.168.10.1".into(),
                },
                AdapterDns {
                    name: "xray_tun".into(),
                    tunnel: true,
                    connected: true,
                    via_tunnel: Some(true),
                    ip: "8.8.8.8".into(),
                },
            ]
        );
        assert_eq!(parse_adapter_lines("#tun\t-\n").tun_active, None);
        assert_eq!(parse_adapter_lines("#tun\t0\n").tun_active, Some(false));
    }

    #[test]
    fn the_adapter_script_prints_what_the_parser_reads() {
        // 脚本的输出格式跟 `parse_adapter_lines` 是一对。只看形状，不跑 —— 单测不碰真机的网卡。
        let s = adapter_script();
        assert!(
            s.contains(&format!("\"#tun`t$(Via '{ROUTE_PROBE}')\"")),
            "{s}"
        );
        assert!(s.contains("\"$n`t$t`t$u`t$(Via $s)`t$s\""), "{s}");
        assert!(s.contains("InterfaceOperationalStatus -eq 1"), "{s}");
        assert!(s.contains("Find-NetRoute -RemoteIPAddress $ip"), "{s}");
        assert!(
            !s.contains("{{") && !s.contains("}}"),
            "format! 的转义要消掉：{s}"
        );
    }

    // ---- bash.ws 回显的形状（2026-09-25：有使用者那边整份回的是对象，原来整次检测报「JSON 解析失败」）

    /// 平时的样子：数组，出口一条、解析器若干、结论一条。
    const USUAL: &str = r#"[
        {"ip":"203.0.113.9","country":"US","country_name":"United States","asn":"AS64500 Example","type":"ip"},
        {"ip":"1.1.1.1","country":"AU","country_name":"Australia","asn":"AS13335 Cloudflare","type":"dns"},
        {"ip":"DNS is not leaking.","country":"","country_name":"","asn":"","type":"conclusion"}
    ]"#;

    #[test]
    fn the_usual_array_still_reads() {
        let e = read_echo(USUAL).unwrap();
        assert_eq!(e.egress_asn.as_deref(), Some("AS64500 Example"));
        // 结论文字在 `ip` 字段里、`country_name` 是空串 —— 空串当没有，才退得回 `ip`。
        assert_eq!(e.conclusion.as_deref(), Some("DNS is not leaking."));
        assert_eq!(e.resolvers.len(), 1);
        assert_eq!(e.resolvers[0].address, "1.1.1.1");
        assert!(!e.resolvers[0].is_domestic && !e.resolvers[0].from_adapter);
    }

    #[test]
    fn a_php_object_with_gaps_reads_like_the_array() {
        // PHP 的 json_encode 遇到下标不连续的数组就编成对象。键按**数字**排回去：
        // serde_json 按字符串排的话 "10" 会跑到 "2" 前面，国内那台就排到了最前。
        let text = r#"{
            "0":{"ip":"203.0.113.9","country":"US","asn":"AS64500","type":"ip"},
            "10":{"ip":"114.114.114.114","country":"CN","country_name":"China","type":"dns"},
            "2":{"ip":"8.8.8.8","country":"US","country_name":"United States","type":"dns"},
            "11":{"ip":"DNS may be leaking.","type":"conclusion"}
        }"#;
        let e = read_echo(text).expect("对象也要读得出来");
        let addrs: Vec<&str> = e.resolvers.iter().map(|r| r.address.as_str()).collect();
        assert_eq!(addrs, vec!["8.8.8.8", "114.114.114.114"]);
        assert!(e.resolvers[1].is_domestic, "国内那台不许因为形状变了就丢");
        assert_eq!(e.egress_asn.as_deref(), Some("AS64500"));
        assert_eq!(e.conclusion.as_deref(), Some("DNS may be leaking."));
    }

    #[test]
    fn a_lone_entry_object_is_one_entry() {
        let e = read_echo(r#"{"ip":"9.9.9.9","country":"CH","type":"dns"}"#).unwrap();
        assert_eq!(e.resolvers.len(), 1);
        assert_eq!(e.resolvers[0].address, "9.9.9.9");
    }

    #[test]
    fn an_empty_object_is_an_empty_list_not_a_pass() {
        let e = read_echo("{}").unwrap();
        assert!(e.resolvers.is_empty());
        // 空清单之后照旧落到「判不了」：分数是「—」，不是 100。
        assert_eq!(score_report(&e.resolvers, TUN_OFF), (None, None));
    }

    #[test]
    fn false_and_numbers_do_not_break_the_list() {
        // PHP 查不到时给 false、ASN 给个数 —— 原来一个 false 就让整份清单解析失败。
        let text = r#"[
            {"ip":"9.9.9.9","country":false,"country_name":false,"asn":19281,"type":"dns"},
            {"ip":"223.5.5.5","country":"cn","country_name":null,"type":"dns"},
            {"type":"dns","country":"CN"},
            {"ip":"198.51.100.1","type":"something-new","extra":{"nested":true}}
        ]"#;
        let e = read_echo(text).expect("宽容读，不许整份作废");
        assert_eq!(e.resolvers.len(), 2, "没有地址的那条跳过，不认识的类型跳过");
        assert_eq!(e.resolvers[0].asn.as_deref(), Some("19281"));
        assert_eq!(e.resolvers[0].country_code, None);
        assert!(e.resolvers[1].is_domestic, "小写的 cn 也是国内");
        assert_eq!(e.resolvers[1].country_code.as_deref(), Some("CN"));
    }

    #[test]
    fn an_error_receipt_names_its_shape_but_not_its_values() {
        let err = read_echo(r#"{"error":"too many requests from 198.51.100.7","code":429}"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("不是解析器清单"), "{err}");
        assert!(
            err.contains("error 字符串") && err.contains("code 数字"),
            "{err}"
        );
        assert!(
            !err.contains("198.51.100.7"),
            "回执里的 IP 不许进报错：{err}"
        );
        assert!(!err.contains("too many"), "值一个都不带：{err}");
    }

    #[test]
    fn keys_that_look_like_addresses_are_masked() {
        let err = read_echo(r#"{"198.51.100.7":1}"#).unwrap_err().to_string();
        assert!(err.contains("<IP> 数字"), "{err}");
        assert!(!err.contains("198.51.100.7"), "{err}");
    }

    #[test]
    fn a_page_that_is_not_json_is_reported_without_its_body() {
        let err = read_echo("<html>blocked for 198.51.100.7</html>")
            .unwrap_err()
            .to_string();
        assert!(err.contains("不是 JSON") && err.contains("「<」"), "{err}");
        assert!(!err.contains("198.51.100.7"), "{err}");
        assert!(read_echo("").unwrap_err().to_string().contains("空的"));
    }

    #[test]
    fn a_list_with_things_that_are_not_records_is_not_guessed() {
        let err = read_echo("[1,2]").unwrap_err().to_string();
        assert!(err.contains("数组，2 项，里面混着数字"), "{err}");
    }

    #[test]
    fn a_leading_bom_is_fine() {
        let text = format!("\u{feff}{USUAL}");
        assert_eq!(read_echo(&text).unwrap().resolvers.len(), 1);
    }

    // ---- fake-ip：探针被代理接住，查询没从本机发出去

    fn ip_list(v: &[&str]) -> Vec<IpAddr> {
        v.iter().map(|s| s.parse().unwrap()).collect()
    }

    #[test]
    fn lookups_count_a_probe_as_fake_only_when_every_address_is() {
        let mut l = Lookups::default();
        l.note(&ip_list(&["198.18.0.5"]));
        l.note(&ip_list(&["fdfe:dcba:9876::2"]));
        l.note(&ip_list(&["198.18.0.6", "93.184.216.34"]));
        l.note(&[]);
        assert_eq!(
            l,
            Lookups {
                answered: 3,
                fake_ip: 2
            }
        );
        assert!(!l.all_fake_ip(), "有一个探针解析到了真地址，就不算 fake-ip");
    }

    #[test]
    fn fake_ip_explains_the_missing_echo() {
        let all = Lookups {
            answered: 10,
            fake_ip: 10,
        };
        assert!(no_echo_finding(all).contains("fake-ip"));
        // 部分是 fake-ip、或者一个都没回答：说不清是怎么回事，照旧是那句「判不了」。
        for l in [
            Lookups {
                answered: 10,
                fake_ip: 3,
            },
            Lookups::default(),
        ] {
            let s = no_echo_finding(l);
            assert!(s.contains("不能判定") && !s.contains("fake-ip"), "{s}");
        }
    }
}
