//! 本机路由的**纯状态**：这一发请求该打哪条线,以及换上游怎么换。
//!
//! 真正监听端口、转发字节的那一层不在这里 —— 这个 crate 连 `reqwest`
//! 都没有依赖,物理上收发不了任何东西。这里只回答两个问题:
//!
//! 1. 下一发请求绑哪条线;
//! 2. 调度器要求换上游时,已经在飞的请求会不会被影响。
//!
//! # ⛔ 只在两次请求之间切换
//!
//! 换上游**绝不许影响正在飞的请求**。中途把上游换掉,那一发请求会拿着
//! A 站的上下文去打 B 站:轻则报错,重则把半截对话计到另一家的账上,
//! 而使用者看到的是「莫名其妙断了」。
//!
//! 做法是 [`begin`](UpstreamSwitch::begin) 把线路 id **交出去一份拷贝**。
//! 请求全程拿着自己那一份,之后 `current` 怎么变都与它无关 ——
//! 这不是靠自觉遵守,是所有权本身就做不到别的。
//!
//! # 本机路由的意义是「启一次」
//!
//! 每个客户端页顶上只有一处能启动。客户端连的是
//! `127.0.0.1`,之后换上游不需要重启客户端 —— 换的是这里的 `current`,
//! 客户端那一头的地址一个字都不用动。

use crate::schedule::breaker::BreakerSettings;
use qb_contract::domain::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// 上游选择。**换上游只在两次请求之间生效。**
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpstreamSwitch {
    current: Option<String>,
    /// 已经排好队、等下一发请求生效的那条。
    pending: Option<String>,
    in_flight: u32,
    /// 换过几次。迟滞是不是真的在起作用,看这个数 ——
    /// 实测没有迟滞时 200 发请求换了 53 次。
    switches: u64,
}

impl UpstreamSwitch {
    pub fn new() -> Self {
        Self::default()
    }

    /// 现在对外宣称用的是哪条线。**这不是在飞的请求用的那条** ——
    /// 那一条在请求自己手里。
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// 排队等生效的那条。`None` = 没有待切换的。
    pub fn pending(&self) -> Option<&str> {
        self.pending.as_deref()
    }

    pub fn in_flight(&self) -> u32 {
        self.in_flight
    }

    pub fn switches(&self) -> u64 {
        self.switches
    }

    /// 调度器要求换上游。
    ///
    /// **不动 `current`** —— 只排队。换成当前这条等于取消排队。
    pub fn request_switch(&mut self, route_id: impl Into<String>) {
        let next = route_id.into();
        if Some(next.as_str()) == self.current.as_deref() {
            self.pending = None;
        } else {
            self.pending = Some(next);
        }
    }

    /// 一发请求开始。返回它这一趟要用的线路 id ——
    /// **拿到的是一份拷贝,之后换上游与它无关。**
    ///
    /// 排队中的切换在这一刻生效:两次请求之间,正是这里。
    pub fn begin(&mut self) -> Option<String> {
        if let Some(next) = self.pending.take() {
            self.current = Some(next);
            self.switches += 1;
        }
        self.in_flight += 1;
        self.current.clone()
    }

    /// 一发请求结束。
    pub fn end(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }

    /// 直接落定一条线,不排队。
    ///
    /// 只给「第一次选上游」和「使用者手动指定」用 —— 调度器**不许**走这里,
    /// 它必须走 [`request_switch`](Self::request_switch)。
    pub fn set_now(&mut self, route_id: impl Into<String>) {
        let next = route_id.into();
        if self.current.as_deref() != Some(next.as_str()) {
            self.switches += 1;
        }
        self.current = Some(next);
        self.pending = None;
    }
}

/// 按软件分开的上游选择。**每个软件各记各的当前上游。**
///
/// # 为什么不能只有一个 `current`
///
/// 三个软件（Claude Code / Claude 桌面端 / Codex）同时连着同一个本机路由。
/// 只留一份当前上游的话，在 Codex 页上换一条线会把 Claude Code 正在用的那条
/// **一起换掉** —— 而使用者完全看不出是自己刚才那一下干的：他明明在另一个
/// 分页上操作。症状会表现成「Claude Code 莫名其妙换站了」。
///
/// 每个软件一份 [`UpstreamSwitch`]，所以「换上游只在两次请求之间生效」
/// 那条不变量对每个软件**各自**成立，互不干扰。
///
/// # 没登记过的软件返回 `None`，不回落到别人的上游
///
/// 回落会让一个从没选过上游的软件**悄悄用上别人的 Key**，账记在别人头上。
/// 如实返回 `None`，转发层据此回一句「还没选上游」。
#[derive(Debug, Default)]
pub struct ClientRouter {
    per_client: BTreeMap<Client, UpstreamSwitch>,
}

impl ClientRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// 这个软件当前对外宣称用的是哪条线。
    pub fn current(&self, client: Client) -> Option<&str> {
        self.per_client.get(&client)?.current()
    }

    /// 这个软件排队等生效的那条。
    pub fn pending(&self, client: Client) -> Option<&str> {
        self.per_client.get(&client)?.pending()
    }

    /// 三个软件各自的当前上游。界面上「正在路由」按软件分别显示要用它。
    pub fn currents(&self) -> BTreeMap<Client, String> {
        self.per_client
            .iter()
            .filter_map(|(c, s)| s.current().map(|id| (*c, id.to_string())))
            .collect()
    }

    /// 全部软件加起来换过几次手。迟滞有没有起作用看这个数。
    pub fn switches(&self) -> u64 {
        self.per_client.values().map(UpstreamSwitch::switches).sum()
    }

    /// 此刻总共有几发请求在飞。
    pub fn in_flight(&self) -> u32 {
        self.per_client
            .values()
            .map(UpstreamSwitch::in_flight)
            .sum()
    }

    /// 调度器要求给这个软件换上游。**只排队，不动 `current`。**
    pub fn request_switch(&mut self, client: Client, route_id: impl Into<String>) {
        self.per_client
            .entry(client)
            .or_default()
            .request_switch(route_id);
    }

    /// 使用者手动指定，或第一次选上游。
    pub fn set_now(&mut self, client: Client, route_id: impl Into<String>) {
        self.per_client.entry(client).or_default().set_now(route_id);
    }

    /// 这个软件的一发请求开始。返回它这一趟要用的线路 id（一份拷贝）。
    ///
    /// **从没登记过的软件返回 `None`** —— 不建条目、不回落到别人的上游。
    pub fn begin(&mut self, client: Client) -> Option<String> {
        self.per_client.get_mut(&client)?.begin()
    }

    /// 这个软件的一发请求结束。
    pub fn end(&mut self, client: Client) {
        if let Some(s) = self.per_client.get_mut(&client) {
            s.end();
        }
    }
}

/// 一个软件的本机路由设置（cc-switch 的 `AppProxyConfig`，每个 app 一行）。
///
/// 存在 `router_settings` 表里，一个软件一行；没存过就是 [`RouterSettings::for_client`] 那一份。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct RouterSettings {
    pub client: Client,
    /// 自动故障转移：这条线失败了改用队列里的下一条。**默认开**（使用者定的）；
    /// 关掉就只打当前那一条、失败照实回给客户端（cc-switch 的默认是关）。
    pub auto_failover: bool,
    /// 一发请求最多试几条线（含第一条）。
    pub max_attempts: u32,
    /// 流式请求：响应头回来之后多久没等到第一块数据就算超时（秒，0 = 不限）。
    pub first_byte_timeout_secs: u32,
    /// 流式请求：两块数据之间最长等多久（秒，0 = 不限）。超了就断开，不再干等。
    pub idle_timeout_secs: u32,
    /// 非流式请求：整份响应要在多久之内读完（秒，0 = 不限）。
    pub non_streaming_timeout_secs: u32,
    pub breaker: BreakerSettings,
    /// 整流器（cc-switch 的 `RectifierConfig`，默认全开）。
    pub rectifier: RectifierSwitches,
    /// 聚合模型（中转站重构阶段 3）。老数据没有就是关着。
    pub aggregate: AggregateSettings,
}

/// 聚合模型（cc-switch 的 Stack 模式，只搬了本机路由那一半，见 `qb_proxy::aggregate`）：名单里每条线的模型以
/// 带前缀的 id 发布给客户端，选中的请求直达那一条线 —— **不换线、不碰熔断**；解不出来的 id 一律报错，
/// 不回落到当前那条（回落等于拿别家的钱、别家的模型回答，使用者看不出来）。不带前缀的请求照旧按派单队列走。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct AggregateSettings {
    pub enabled: bool,
    /// 名单：线路 id，按加入顺序（模型列表里也按这个顺序）。只收这个软件自己的线。
    pub members: Vec<String>,
    /// 每条线发布哪些模型（发往上游的名字，可以带 `[1M]`）。没有或为空 = 用这条线模型映射里的目标模型。
    pub models: std::collections::BTreeMap<String, Vec<String>>,
    /// key → 线路 id 的**永久**登记。客户端会一直带着选中过的 id：key 一旦换了指向，旧 id 会被悄悄发到另一条线。
    /// 移出名单、删掉线路都不删登记（cc-switch 的 `allocate_key` 同样如此）。
    pub keys: std::collections::BTreeMap<String, String>,
}

/// 聚合名单最多几条线、每条线最多发布几个模型、一个模型名最长几个字符。只是为了界面和模型列表不至于失控。
pub const AGGREGATE_MAX_MEMBERS: usize = 32;
pub const AGGREGATE_MAX_MODELS_PER_LINE: usize = 64;
pub const AGGREGATE_MAX_MODEL_LEN: usize = 200;

/// 聚合模型发布出去的一个（设置弹窗的预览；本机路由答模型发现时发的就是这些）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AggregateModel {
    pub route_id: String,
    /// 发布给客户端的 id（带 `qbg` 前缀；桌面端是 `claude-fable-qbg-` 加 6 位数字）。
    pub id: String,
    /// 发往上游的模型名。
    pub upstream: String,
    /// 选择器里显示的名字：`模型（线路名）`。
    pub display_name: String,
    pub one_m: bool,
}

/// 整流器的几个开关。跟 `qb_proxy::types::RectifierConfig` 一一对应，由 `qb-app` 换过去。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct RectifierSwitches {
    pub enabled: bool,
    pub thinking_signature: bool,
    pub thinking_budget: bool,
    pub media_fallback: bool,
    pub media_heuristic: bool,
}

impl Default for RectifierSwitches {
    fn default() -> Self {
        Self {
            enabled: true,
            thinking_signature: true,
            thinking_budget: true,
            media_fallback: true,
            media_heuristic: true,
        }
    }
}

impl RouterSettings {
    /// 默认值照 cc-switch 的 `proxy_config` 表（claude 一行、codex 一行），熔断的连续次数仍是 QB Gate 的 5。
    ///
    /// | | 最多试几条 | 首包 | 静默 | 非流式 | 错误率 / 最少样本 |
    /// |---|---|---|---|---|---|
    /// | Claude Code / 桌面端 | 6 | 90 | 180 | 600 | 0.7 / 15 |
    /// | Codex | 3 | 60 | **300** | 600 | 0.6 / 10 |
    ///
    /// 只有一处跟 cc-switch 不一样：Codex 的静默超时它是 120 秒，这里取 Codex 自己的默认
    /// （`stream_idle_timeout_ms` = 300 秒）。推理很久才出下一段的长回合会安静好几分钟，
    /// 路由比客户端自己先掐断，等于替它做了一个它没要的决定（原来路由根本没有静默超时）。
    pub fn for_client(client: Client) -> Self {
        let codex = client == Client::Codex;
        Self {
            client,
            auto_failover: true,
            max_attempts: if codex { 3 } else { 6 },
            first_byte_timeout_secs: if codex { 60 } else { 90 },
            idle_timeout_secs: if codex { 300 } else { 180 },
            non_streaming_timeout_secs: 600,
            breaker: BreakerSettings {
                error_rate: if codex { 0.6 } else { 0.7 },
                min_requests: if codex { 10 } else { 15 },
                ..BreakerSettings::default()
            },
            rectifier: RectifierSwitches::default(),
            aggregate: AggregateSettings::default(),
        }
    }

    /// 存之前先看一遍。**不悄悄夹到范围里** —— 使用者填的是 0.6 却存成 1.0，界面上看起来存上了。
    ///
    /// 超时 0 = 不限（cc-switch 同样如此），所以超时只有上限；上限一小时，再长就等于不限，
    /// 那就该填 0，而不是一个看不出意图的大数。
    pub fn validate(&self) -> std::result::Result<(), String> {
        const MAX_TIMEOUT_SECS: u32 = 3600;
        let mut problems = Vec::new();
        if !(1..=20).contains(&self.max_attempts) {
            problems.push(format!(
                "一发最多试几条线要在 1 到 20 之间（填的是 {}）",
                self.max_attempts
            ));
        }
        for (name, v) in [
            ("首包超时", self.first_byte_timeout_secs),
            ("静默超时", self.idle_timeout_secs),
            ("非流式超时", self.non_streaming_timeout_secs),
        ] {
            if v > MAX_TIMEOUT_SECS {
                problems.push(format!(
                    "{name}最长 {MAX_TIMEOUT_SECS} 秒（填的是 {v}）；不想限就填 0"
                ));
            }
        }
        let b = &self.breaker;
        if b.trip_after_consecutive == 0 {
            problems.push("连续几次失败才熔断不能是 0".to_string());
        }
        if !(b.error_rate.is_finite() && b.error_rate > 0.0 && b.error_rate <= 1.0) {
            problems.push(format!(
                "错误率阈值要在 0 到 1 之间、大于 0（填的是 {}）",
                b.error_rate
            ));
        }
        if b.min_requests == 0 {
            problems.push("看错误率前至少要几次结果不能是 0".to_string());
        }
        if b.half_open_successes == 0 {
            problems.push("半开时要成功几次才闭合不能是 0".to_string());
        }
        let a = &self.aggregate;
        if a.members.len() > AGGREGATE_MAX_MEMBERS {
            problems.push(format!(
                "聚合名单最多 {AGGREGATE_MAX_MEMBERS} 条线（勾了 {} 条）",
                a.members.len()
            ));
        }
        for list in a.models.values() {
            if list.len() > AGGREGATE_MAX_MODELS_PER_LINE {
                problems.push(format!(
                    "一条线最多发布 {AGGREGATE_MAX_MODELS_PER_LINE} 个聚合模型（写了 {} 个）",
                    list.len()
                ));
            }
            if let Some(bad) = list.iter().find(|m| {
                m.chars().count() > AGGREGATE_MAX_MODEL_LEN || m.chars().any(char::is_control)
            }) {
                problems.push(format!(
                    "聚合模型名太长或带了控制字符：{}",
                    bad.chars().take(40).collect::<String>()
                ));
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("；"))
        }
    }
}

impl Default for RouterSettings {
    fn default() -> Self {
        Self::for_client(Client::ClaudeCode)
    }
}

/// 本机路由监听在哪（三个软件共用一个监听端口）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum ListenScope {
    /// 只本机（`127.0.0.1`）。默认，也是 2026-10-24 之前唯一的一种。
    #[default]
    Loopback,
    /// 所有网卡（`0.0.0.0`）：WSL、局域网里别的机器也连得上。**从别的机器来的必须带路由口令。**
    AllInterfaces,
}

/// 本机路由的网络设置（中转站重构 R6，使用者选的；三个软件共用一份，存在 metadata 的 `router_network` 键下）。
///
/// ⛔ 这一份**不出后端**：口令只在使用者要开到所有网卡上时由界面显示（他要抄到别的机器上去），
/// 出站代理的地址可能带用户名密码，给界面的是去掉了那两样的写法（[`mask_proxy_url`]）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RouterNetwork {
    pub scope: ListenScope,
    /// 路由口令（随机）。只在 `AllInterfaces` 时用；空 = 还没生成过。
    pub key: String,
    /// 中转线路的出站代理（`http://` / `https://` / `socks5://` / `socks5h://`）。`None` = 直连。
    /// ⛔ 只给中转线路；官方 Codex 那条线永远直连。
    pub outbound_proxy: Option<String>,
}

impl RouterNetwork {
    /// 绑哪个地址。
    pub fn listen(&self) -> std::net::IpAddr {
        match self.scope {
            ListenScope::Loopback => std::net::Ipv4Addr::LOCALHOST.into(),
            ListenScope::AllInterfaces => std::net::Ipv4Addr::UNSPECIFIED.into(),
        }
    }
}

/// 出站代理地址看一遍：只认 `http` / `https` / `socks5` / `socks5h`，要有主机和端口。
/// 不合格就说哪里不对（不夹、不猜）。
pub fn check_proxy_url(url: &str) -> std::result::Result<(), String> {
    let url = url.trim();
    let Some((scheme, rest)) = url.split_once("://") else {
        // 认不出形状时不回显填的东西：那里面可能就是密码。
        return Err("出站代理要写成 http://主机:端口 或 socks5://主机:端口".to_string());
    };
    if !matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "socks5" | "socks5h"
    ) {
        return Err(format!(
            "出站代理只认 http / https / socks5 / socks5h（填的是 {scheme}）"
        ));
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, hp)| hp);
    let Some((host, port)) = host_port.rsplit_once(':') else {
        return Err("出站代理要带端口（例如 :7890）".to_string());
    };
    if host.trim().is_empty() {
        return Err("出站代理的主机是空的".to_string());
    }
    match port.parse::<u16>() {
        Ok(p) if p > 0 => Ok(()),
        _ => Err(format!("出站代理的端口不对：{port}")),
    }
}

/// 给日志和界面看的写法：去掉用户名和密码，只留 `协议://主机:端口`。
pub fn mask_proxy_url(url: &str) -> String {
    let url = url.trim();
    match url.split_once("://") {
        Some((scheme, rest)) => {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
            let host_port = authority.rsplit_once('@').map_or(authority, |(_, hp)| hp);
            format!("{scheme}://{host_port}")
        }
        // 认不出形状：一个字都不露，那里面可能就是密码。
        None => "（认不出的代理地址）".to_string(),
    }
}

/// 一个软件的派单队列（落盘的那一份，存在 `dispatch_queue` 表里，一个软件一行）。
///
/// 当前那条失败时按 `order` 往下试（自动故障转移，见 [`RouterSettings::auto_failover`]）。
/// 调度关着时 `order` 是使用者在「路由设置」里手动排的；**调度开着时由调度排**
/// （`schedule::dispatch`，中转站重构阶段 2），连同钉住、防抖记忆和这一轮的结论一起存在这里。
///
/// **为什么要落盘**：原来当前上游只在内存里，面板一重启就没了，客户端下一发收到 503
/// 「还没选上游」（交接文档 §2.12 第 5 条，2026-10-24 照代码核过）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct DispatchQueue {
    pub client: Client,
    /// 上次选中的那条（使用者手动选、或调度换过去的）。面板重启后从这里接着用。
    pub current: Option<String>,
    /// 当前那条失败时依次改用谁。当前那条不必在里面；删掉的线读的时候跳过。
    pub order: Vec<String>,
    /// 上次改的时刻（毫秒）。
    pub updated_ms: i64,
    /// 钉住的那条（调度开着时手动选的线钉 4 小时，除非它挂了）。
    pub pin: Option<crate::schedule::Pin>,
    /// 防抖的记忆（挑战者、连续领先几轮）。
    pub memory: crate::schedule::DispatchMemory,
    /// 调度这一轮排出来的每一行（档位、为什么）。调度关着时是空的。
    pub rows: Vec<crate::schedule::DispatchRow>,
    /// 调度这一轮的告警。
    pub alerts: Vec<String>,
}

impl DispatchQueue {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            ..Default::default()
        }
    }
}

/// 一发请求试过的一条线。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RouteAttempt {
    pub route_id: String,
    /// 上游回的状态码。连不上、超时是 `None`。
    pub status: Option<u16>,
    /// 失败了的话是哪一类（`qb_proxy::classify::Fault::key`）。成功是 `None`。
    pub fault: Option<String>,
    /// 这一条用了多久（毫秒）。
    pub ms: u64,
}

/// 一条请求日志（第二版，2026-10-17）：多了试过的线路链、四类 token、两种花费。
///
/// 存在 `router_requests` 表里（正经的列，按时间 / 线路 / 软件建了索引）；老的 [`RequestLog`]
/// 那张 `request_logs` 由迁移 5 搬过来。**每一项都可能取不到**，取不到就是 `None`，不拿 0 顶上。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RouterRequest {
    pub id: String,
    pub at_ms: i64,
    /// 哪个软件发来的（`Client` 的 kebab-case 写法）。
    pub client: String,
    /// 最后回给客户端的那条线（成功的那条，或者最后试的那条）。
    pub route_id: String,
    /// 客户端请求里写的模型。
    pub model: String,
    /// 按线路映射之后真正发上去的模型。没映射就是空串。
    pub upstream_model: String,
    pub status: Option<u16>,
    pub first_token_ms: Option<u64>,
    pub total_ms: Option<u64>,
    /// 上游的 `Retry-After`,已经换算成毫秒。
    pub retry_after_ms: Option<i64>,
    pub streaming: bool,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    /// `cache_write_tokens` 里按 1 小时档写的那部分（Claude 单独计价）。
    pub cache_write_1h_tokens: Option<u64>,
    /// 站内花费（美元，按这条线自己公布的价；落库时算）。
    pub station_cost: Option<f64>,
    /// 同样的 token 按官方价折算（美元）。**不是账单** —— 订阅不按这个收费。
    pub official_cost: Option<f64>,
    pub session_id: Option<String>,
    /// 失败了的话是哪一类（最后那一条线的）。
    pub fault: Option<String>,
    /// 失败的一句话摘要。**不含请求内容、不含 Key。**
    pub error: Option<String>,
    /// 按顺序试过的线。只试了一条也有一项。
    pub attempts: Vec<RouteAttempt>,
}

impl RouterRequest {
    /// 这一发算不算成功（2xx 且没有记故障）。
    pub fn ok(&self) -> bool {
        self.fault.is_none() && self.status.is_some_and(|s| (200..300).contains(&s))
    }
}

impl From<RequestLog> for RouterRequest {
    /// 第一版日志升上来：只有状态码与耗时，没有 token、没有线路链（那时候只试一条）。
    fn from(old: RequestLog) -> Self {
        Self {
            attempts: vec![RouteAttempt {
                route_id: old.route_id.clone(),
                status: old.status,
                fault: None,
                ms: old.total_ms.unwrap_or(0),
            }],
            id: old.id,
            at_ms: old.at_ms,
            client: old.client,
            route_id: old.route_id,
            model: old.model,
            status: old.status,
            first_token_ms: old.first_token_ms,
            total_ms: old.total_ms,
            retry_after_ms: old.retry_after_ms,
            ..Default::default()
        }
    }
}

/// 第一版请求日志（0.14.0–0.25.6 的 `request_logs` 表）。新的写 [`RouterRequest`]；
/// 这个形状留着只为读老数据（迁移 5 与它的测试）。
///
/// **每一项都可能取不到** —— 上游没回、连接断在半路都是常态,
/// 取不到就是 `None`,不拿 0 顶上。
///
/// 前端不再认它（2026-10-24 起不导出 TS 类型）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RequestLog {
    pub id: String,
    pub at_ms: i64,
    /// 打的是哪条线。**是请求开始时绑定的那条**,不是现在的 `current`。
    pub route_id: String,
    /// 哪个客户端发来的。
    pub client: String,
    pub model: String,
    pub status: Option<u16>,
    pub first_token_ms: Option<u64>,
    pub total_ms: Option<u64>,
    /// 上游的 `Retry-After`,已经换算成毫秒。
    pub retry_after_ms: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::Outcome;

    #[test]
    fn a_switch_never_disturbs_a_request_that_is_already_in_flight() {
        // 中途换掉,那一发请求会拿着 A 站的上下文去打 B 站。
        let mut s = UpstreamSwitch::new();
        s.set_now("A");
        let mine = s.begin();
        assert_eq!(mine.as_deref(), Some("A"));

        s.request_switch("B");
        // 我手里那一份没变,而且 current 也还没变 —— 切换在排队。
        assert_eq!(mine.as_deref(), Some("A"));
        assert_eq!(s.current(), Some("A"));
        assert_eq!(s.pending(), Some("B"));

        s.end();
        // 下一发才换。
        assert_eq!(s.begin().as_deref(), Some("B"));
        assert_eq!(s.current(), Some("B"));
        assert_eq!(s.pending(), None);
    }

    #[test]
    fn several_in_flight_requests_each_keep_their_own_upstream() {
        let mut s = UpstreamSwitch::new();
        s.set_now("A");
        let a1 = s.begin();
        let a2 = s.begin();
        s.request_switch("B");
        let a3 = s.begin(); // 前两发还在飞,但这一发是新的,换到 B
        assert_eq!(
            (a1.as_deref(), a2.as_deref(), a3.as_deref()),
            (Some("A"), Some("A"), Some("B"))
        );
        assert_eq!(s.in_flight(), 3);
    }

    #[test]
    fn switching_to_the_current_upstream_cancels_the_queue_instead_of_churning() {
        let mut s = UpstreamSwitch::new();
        s.set_now("A");
        s.request_switch("B");
        assert_eq!(s.pending(), Some("B"));
        // 调度器又改主意了,还是 A。
        s.request_switch("A");
        assert_eq!(s.pending(), None);
        let before = s.switches();
        s.begin();
        assert_eq!(s.switches(), before, "没换却记了一次换手");
    }

    #[test]
    fn switches_are_only_counted_when_the_upstream_really_changes() {
        // 换手次数是迟滞有没有起作用的唯一证据,多记一次就看不出来了。
        let mut s = UpstreamSwitch::new();
        s.set_now("A");
        assert_eq!(s.switches(), 1);
        s.set_now("A");
        assert_eq!(s.switches(), 1);
        for _ in 0..10 {
            s.begin();
            s.end();
        }
        assert_eq!(s.switches(), 1, "没有待切换时 begin 不该记换手");
    }

    #[test]
    fn ending_more_requests_than_began_does_not_underflow() {
        let mut s = UpstreamSwitch::new();
        s.end();
        s.end();
        assert_eq!(s.in_flight(), 0);
    }

    #[test]
    fn a_fresh_switch_has_no_upstream_and_begin_says_so() {
        // 一条上游都还没选时,begin 要如实返回 None,
        // 不能凭空给一个 id —— 那会让路由打到一条不存在的线上。
        let mut s = UpstreamSwitch::new();
        assert_eq!(s.begin(), None);
        assert_eq!(s.current(), None);
    }

    /// 在一个软件上换线，**绝不许动到别的软件**。
    ///
    /// 只留一份 `current` 时，在 Codex 页上点一下会把 Claude Code
    /// 正在用的那条一起换掉 —— 而使用者在另一个分页上，完全看不出是自己干的。
    #[test]
    fn switching_one_client_never_moves_another_clients_upstream() {
        let mut r = ClientRouter::new();
        r.set_now(Client::ClaudeCode, "cc-line");
        r.set_now(Client::Codex, "cx-line");

        r.request_switch(Client::Codex, "cx-other");
        assert_eq!(r.begin(Client::Codex).as_deref(), Some("cx-other"));
        assert_eq!(
            r.current(Client::ClaudeCode),
            Some("cc-line"),
            "改 Codex 把 Claude Code 的上游也换了"
        );
    }

    /// 从没登记过的软件返回 `None`，**不回落到别人的上游**。
    ///
    /// 回落会让它悄悄用上别人的 Key，账记在别人头上，而且完全看不出来。
    #[test]
    fn an_unregistered_client_gets_nothing_rather_than_someone_elses_key() {
        let mut r = ClientRouter::new();
        r.set_now(Client::ClaudeCode, "cc-line");
        assert_eq!(r.begin(Client::Codex), None);
        assert_eq!(r.current(Client::ClaudeDesktop), None);
        // 问一次不该把它登记进去。
        assert!(!r.currents().contains_key(&Client::Codex));
    }

    /// Claude Code 与桌面端**协议一样，但仍然是两条独立的线**。
    #[test]
    fn claude_code_and_desktop_keep_separate_upstreams() {
        let mut r = ClientRouter::new();
        r.set_now(Client::ClaudeCode, "A");
        r.set_now(Client::ClaudeDesktop, "B");
        let m = r.currents();
        assert_eq!(m.get(&Client::ClaudeCode).map(String::as_str), Some("A"));
        assert_eq!(m.get(&Client::ClaudeDesktop).map(String::as_str), Some("B"));
        assert_eq!(r.switches(), 2);
    }

    /// 每个软件各自的「只在两次请求之间切换」都要成立。
    #[test]
    fn each_client_keeps_the_in_flight_guarantee_on_its_own() {
        let mut r = ClientRouter::new();
        r.set_now(Client::ClaudeCode, "A");
        let mine = r.begin(Client::ClaudeCode);
        r.request_switch(Client::ClaudeCode, "B");
        assert_eq!(mine.as_deref(), Some("A"), "在飞的那一发被换掉了");
        assert_eq!(r.current(Client::ClaudeCode), Some("A"));
        assert_eq!(r.in_flight(), 1);
        r.end(Client::ClaudeCode);
        assert_eq!(r.in_flight(), 0);
        assert_eq!(r.begin(Client::ClaudeCode).as_deref(), Some("B"));
    }
    #[test]
    fn http_statuses_map_onto_the_three_breaker_tiers() {
        // §4.6 那张表唯一的落地处。
        assert_eq!(
            Outcome::from_status(429, Some(5_000)),
            Outcome::RateLimited {
                retry_after_ms: Some(5_000)
            }
        );
        for s in [401u16, 402, 403] {
            assert_eq!(Outcome::from_status(s, None), Outcome::Rejected, "{s}");
        }
        for s in [500u16, 502, 503, 504] {
            assert_eq!(Outcome::from_status(s, None), Outcome::ServerError, "{s}");
        }
        for s in [200u16, 201, 204, 301] {
            assert_eq!(Outcome::from_status(s, None), Outcome::Ok, "{s}");
        }
        assert_eq!(Outcome::timeout(), Outcome::ServerError);
    }

    #[test]
    fn router_settings_default_to_cc_switch_s_per_app_table_with_failover_on() {
        let claude = RouterSettings::for_client(Client::ClaudeCode);
        assert!(claude.auto_failover, "自动故障转移默认开（使用者定的）");
        assert_eq!(
            (
                claude.max_attempts,
                claude.first_byte_timeout_secs,
                claude.idle_timeout_secs,
                claude.non_streaming_timeout_secs
            ),
            (6, 90, 180, 600)
        );
        let codex = RouterSettings::for_client(Client::Codex);
        assert_eq!(
            (
                codex.max_attempts,
                codex.first_byte_timeout_secs,
                codex.idle_timeout_secs
            ),
            (3, 60, 300),
            "Codex 的静默超时不比它自己的默认（300 秒）短"
        );
        assert_eq!(codex.breaker.min_requests, 10);
        assert_eq!(
            claude.breaker.trip_after_consecutive, 5,
            "连续次数仍是 QB Gate 的 5"
        );
        assert!(claude.rectifier.enabled && claude.rectifier.thinking_signature);
        // 存下来的缺字段按默认补。
        let parsed: RouterSettings =
            serde_json::from_str(r#"{"client":"codex","max_attempts":2}"#).unwrap();
        assert_eq!(parsed.max_attempts, 2);
        assert!(parsed.auto_failover);
    }

    #[test]
    fn router_settings_out_of_range_are_rejected_not_clamped() {
        // 夹到范围里的话，使用者填的是 0.6 却存成 1.0，界面上看起来存上了。
        assert!(RouterSettings::for_client(Client::Codex).validate().is_ok());
        let mut s = RouterSettings::for_client(Client::ClaudeCode);
        s.max_attempts = 0;
        s.idle_timeout_secs = 9_999;
        s.breaker.error_rate = 1.5;
        let why = s.validate().unwrap_err();
        assert!(why.contains("1 到 20"), "{why}");
        assert!(why.contains("静默超时"), "{why}");
        assert!(why.contains("错误率"), "{why}");
        // 0 = 不限，是合法的。
        let mut unlimited = RouterSettings::for_client(Client::ClaudeCode);
        unlimited.first_byte_timeout_secs = 0;
        unlimited.idle_timeout_secs = 0;
        assert!(unlimited.validate().is_ok());
    }

    #[test]
    fn aggregate_lists_that_would_flood_the_model_picker_are_rejected() {
        let mut s = RouterSettings::for_client(Client::ClaudeCode);
        s.aggregate.enabled = true;
        s.aggregate.members = vec!["a".into()];
        s.aggregate
            .models
            .insert("a".into(), vec!["glm-5[1M]".into()]);
        assert!(s.validate().is_ok());
        s.aggregate.members = (0..=AGGREGATE_MAX_MEMBERS).map(|i| i.to_string()).collect();
        s.aggregate.models.insert(
            "b".into(),
            (0..=AGGREGATE_MAX_MODELS_PER_LINE)
                .map(|i| format!("m{i}"))
                .collect(),
        );
        s.aggregate
            .models
            .insert("c".into(), vec!["bad\nname".into()]);
        let why = s.validate().unwrap_err();
        assert!(why.contains("聚合名单最多"), "{why}");
        assert!(why.contains("一条线最多发布"), "{why}");
        assert!(why.contains("控制字符"), "{why}");
    }

    #[test]
    fn proxy_urls_are_checked_and_masked_without_leaking_the_password() {
        for ok in [
            "http://127.0.0.1:7890",
            "socks5://127.0.0.1:1080",
            "socks5h://user:p%40ss@proxy.example:1080",
            "HTTPS://proxy.example:443/",
        ] {
            assert!(check_proxy_url(ok).is_ok(), "{ok}");
        }
        for bad in [
            "127.0.0.1:7890",
            "ftp://127.0.0.1:21",
            "http://127.0.0.1",
            "http://:7890",
            "socks5://127.0.0.1:0",
            "socks5://127.0.0.1:99999",
        ] {
            assert!(check_proxy_url(bad).is_err(), "{bad} 不该过");
        }
        // 用户名密码不进界面、不进日志。
        assert_eq!(
            mask_proxy_url("socks5h://user:secret@proxy.example:1080/x"),
            "socks5h://proxy.example:1080"
        );
        let err = check_proxy_url("user:secret@host").unwrap_err();
        assert!(!err.contains("secret"), "报错里带出了密码：{err}");
        // 默认只本机。
        let net = RouterNetwork::default();
        assert_eq!(net.scope, ListenScope::Loopback);
        assert!(net.listen().is_loopback());
        let open = RouterNetwork {
            scope: ListenScope::AllInterfaces,
            ..Default::default()
        };
        assert_eq!(open.listen().to_string(), "0.0.0.0");
    }

    #[test]
    fn a_dispatch_queue_saved_without_newer_fields_still_reads() {
        let q: DispatchQueue =
            serde_json::from_str(r#"{"client":"codex","order":["a","b"]}"#).unwrap();
        assert_eq!(q.client, Client::Codex);
        assert_eq!(q.order, ["a", "b"]);
        assert_eq!(q.current, None);
    }

    #[test]
    fn a_first_generation_log_row_reads_into_the_new_shape() {
        let old: RequestLog = serde_json::from_str(
            r#"{"id":"1","at_ms":5,"route_id":"r","client":"codex","model":"gpt","status":502,"first_token_ms":9,"total_ms":40,"retry_after_ms":null}"#,
        )
        .unwrap();
        let new = RouterRequest::from(old);
        assert_eq!(new.route_id, "r");
        assert_eq!(new.status, Some(502));
        assert_eq!(new.attempts.len(), 1, "老日志只试过一条线");
        assert_eq!(new.attempts[0].ms, 40);
        assert_eq!(
            new.input_tokens, None,
            "老日志没有 token —— 是「没记」，不是 0"
        );
        assert!(!new.ok());
    }

    #[test]
    fn a_bad_request_is_our_fault_not_the_line_s_fault() {
        // 400 / 404 / 422 是这一发请求自己的问题(参数错、模型名写错)。
        // 算成故障的话,连打五次就会把一条完全健康的线掐掉。
        for s in [400u16, 404, 422] {
            assert_eq!(Outcome::from_status(s, None), Outcome::Ok, "{s}");
        }
    }
}
