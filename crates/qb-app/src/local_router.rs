//! 本机路由：`127.0.0.1` 上的一个反向代理,把客户端的请求转给当前选中的中转线路。
//!
//! # 它是什么,不是什么
//!
//! **是**:一个 API 路由器。客户端把 base_url 指到本机,之后换上游不用重启客户端 ——
//! 这就是每页顶上「只有一处能启动」的意义。
//!
//! **不是**:代理 / VPN / 翻墙工具。它不改系统代理设置、不接管任何别的程序的流量、
//! 不做链式转发,也没有任何「强制所有流量走代理」「断网即停」的开关。
//! 它只转发**指到它自己身上的**那些 API 请求,目的地是使用者自己填进去的中转站。
//! `DISCLAIMER.md` 里那一节按这个口径写。
//!
//! # ⛔ 只绑 127.0.0.1
//!
//! 绝不绑 `0.0.0.0`。绑上去的那一刻,同一个局域网里的任何人都能拿你的 Key
//! 打你的额度,而且账单看起来完全正常 —— 这个软件是开源分发的,
//! 谁都可能在咖啡馆的 Wi-Fi 上跑它。[`RouterConfig::bind`] 因此没有做成可配置项。
//!
//! # 端口被占了怎么办
//!
//! 不 panic、不静默换端口。默认 15721,占用时**如实报错并把端口号写在错误里** ——
//! 开源分发意味着别人机器上什么都可能占着这个端口,
//! 而「启动了但客户端连不上」是最难查的那种故障。
//! 需要换端口时由调用方传 [`RouterConfig::port`],绑好之后
//! [`RouterHandle::addr`] 给的是**真实绑上的地址**,不是请求的那个。

use crate::error::{GateError, Result};
use futures_util::StreamExt;
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use hyper::body::{Bytes, Frame, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use qb_contract::domain::Client;
use qb_station::router::{ClientRouter, RequestLog};
use qb_station::schedule::{Outcome, RouteBreaker};
use qb_station::sse::StreamScanner;
use qb_station::turnstate::{self, AccountKind, Injection, Policy, Store};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 默认端口。
pub const DEFAULT_PORT: u16 = 15721;

/// 逐跳头:**绝不能原样转发**。
///
/// 它们描述的是「这一段连接」怎么走,不是「这个请求是什么」。原样转给上游会让
/// 连接复用和分块编码两头打架,症状是偶发的截断响应 —— 而且只在流式下出现。
const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "host",
    "content-length",
];

/// 客户端自己带的凭证头:**一律丢掉,换成这条线路自己的**。
///
/// 不丢的话,客户端里残留的旧 Key 会跟着请求一起上去 —— 上游可能用它而不是我们的,
/// 于是「换了线路但账还记在旧 Key 上」,排查时完全看不出来。
const CLIENT_AUTH_HEADERS: &[&str] = &["authorization", "x-api-key", "api-key"];

/// 上游凭证怎么带。
#[derive(Debug, Clone, PartialEq)]
pub enum UpstreamAuth {
    /// `Authorization: Bearer <key>`
    Bearer(String),
    /// `x-api-key: <key>` —— Anthropic Messages 用这个。
    ApiKey(String),
    /// 不带 —— 上游自己不校验,或者由使用者在别处配好。
    None,
    /// 官方 Codex turn-state 模式:**保留客户端自带的 OAuth,不剥不换**,
    /// 并对这条线启用 turn-state 采集 / 注入(见 [`RouterState::turnstate`]）。
    ///
    /// ⛔ 这是「路由绝不承载官方身份」那条安全属性的**唯一受控例外** ——
    /// 只用于使用者自己 ChatGPT 登录的官方 Codex,仍只绑回环、不改系统代理、不做链式转发。
    /// 使用者 2026-09-18 明确同意:本机路由只路由 `127.0.0.1`,不属于当初决定不引入的那部分。
    OAuthPassthrough,
}

/// 一条可以被转发到的线路。
#[derive(Debug, Clone)]
pub struct Upstream {
    pub route_id: String,
    /// 上游基地址,例如 `https://example.com/v1`。
    pub base_url: String,
    pub auth: UpstreamAuth,
}

/// 官方 Codex 的 turn-state 运行态。**一个 model 一份 [`Store`]。**
///
/// ⛔ 只服务官方 Codex（[`UpstreamAuth::OAuthPassthrough`] 那条线）。Claude 不碰。
///
/// # 采集是「被动」的
///
/// 每一发真实官方响应都带回一张新的 turn-state,直接喂给 [`Store::offer`] ——
/// **免费**,不额外发探测请求、不换出口、不消耗多余额度。这条正好符合本项目
/// 「不拿合成请求烧额度」的一贯纪律。注入只在客户端**自己没带** turn-state 时补上,
/// 客户端带了就保留(不弄坏它自己的状态)。
#[derive(Default)]
pub struct TurnState {
    /// 注入总开关。关着时仍被动采集与展示,但不往请求里塞。
    enabled: bool,
    /// 账号规则(个人 10 块 / Team 12 块）。默认个人。
    kind: TurnKind,
    /// 一个 model 一份状态机。
    stores: HashMap<String, Store>,
}

/// 账号规则,带一个 `Default`(个人)—— [`AccountKind`] 自己不好给默认值。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TurnKind {
    #[default]
    Personal,
    Team,
}

impl From<TurnKind> for AccountKind {
    fn from(k: TurnKind) -> Self {
        match k {
            TurnKind::Personal => AccountKind::Personal,
            TurnKind::Team => AccountKind::Team,
        }
    }
}

/// 路由器的可变状态。
#[derive(Default)]
pub struct RouterState {
    /// **按软件分**。三个软件连的是同一个路由，但各走各的上游。
    switch: ClientRouter,
    upstreams: HashMap<String, Upstream>,
    breakers: HashMap<String, RouteBreaker>,
    logs: Vec<RequestLog>,
    /// 官方 Codex 的 turn-state。
    turnstate: TurnState,
}

impl RouterState {
    /// 用中转线路整批替换上游表。
    ///
    /// ⛔ **官方 Codex 那条（`OAuthPassthrough`）不在其列 —— 它会被保留下来。**
    /// 中转线路来自 SQLite 的 `station_routes`（`commands::station::load_upstreams`），
    /// 每次加/删线路都会整批重装一次;而官方 turn-state 上游是另一条路
    /// （[`arm_official_codex`](Self::arm_official_codex)）临时挂上去的,不在那张表里。
    /// 不保留的话,使用者接进路由之后随手编辑一条中转线路,官方上游就被这次重装
    /// 悄悄冲掉了 —— Codex 下一发请求 503,而界面上完全看不出是被这一步冲的。
    pub fn replace_upstreams(&mut self, upstreams: Vec<Upstream>) {
        let official: Vec<Upstream> = self
            .upstreams
            .values()
            .filter(|u| matches!(u.auth, UpstreamAuth::OAuthPassthrough))
            .cloned()
            .collect();
        self.upstreams = upstreams
            .into_iter()
            .map(|u| (u.route_id.clone(), u))
            .collect();
        for u in official {
            self.upstreams.insert(u.route_id.clone(), u);
        }
    }

    /// 登记/更新一条线路。
    pub fn put_upstream(&mut self, u: Upstream) {
        self.upstreams.insert(u.route_id.clone(), u);
    }

    /// 挂上官方 Codex 的 turn-state 上游并选中它。**只对 Codex。**
    ///
    /// 这是「本机路由不承载官方身份」那条边界的唯一受控例外
    /// （[`UpstreamAuth::OAuthPassthrough`]）:请求带着使用者自己的 ChatGPT OAuth
    /// 转发到官方端点,仍只绑回环、不改系统代理、不做链式转发。
    ///
    /// route_id 用固定常量 [`OFFICIAL_CODEX_ROUTE_ID`] —— 一台机器只有一条官方线,
    /// 固定下来才认得出、才好在 [`replace_upstreams`](Self::replace_upstreams) 里保住它。
    pub fn arm_official_codex(&mut self) {
        self.put_upstream(Upstream {
            route_id: OFFICIAL_CODEX_ROUTE_ID.to_string(),
            base_url: OFFICIAL_CODEX_BASE.to_string(),
            auth: UpstreamAuth::OAuthPassthrough,
        });
        self.set_now(Client::Codex, OFFICIAL_CODEX_ROUTE_ID);
    }

    /// 官方 Codex 上游挂上了没有。
    pub fn official_codex_armed(&self) -> bool {
        self.upstreams
            .get(OFFICIAL_CODEX_ROUTE_ID)
            .is_some_and(|u| matches!(u.auth, UpstreamAuth::OAuthPassthrough))
    }

    /// 摘掉官方 Codex 上游。
    ///
    /// 不动 `current` 的选择 —— 若它还指着这条,下一发请求会如实回 503
    /// 「还没选上游」,而不是悄悄回落到别的线路上（那会把官方身份的请求
    /// 打到某条中转线的 Key 上）。
    pub fn disarm_official_codex(&mut self) {
        self.upstreams.remove(OFFICIAL_CODEX_ROUTE_ID);
    }

    /// 这个软件当前选中的线路 id。
    pub fn current(&self, client: Client) -> Option<String> {
        self.switch.current(client).map(str::to_string)
    }

    /// 三个软件各自的当前上游。界面按软件分别显示「正在路由」要用它。
    pub fn currents(&self) -> std::collections::BTreeMap<Client, String> {
        self.switch.currents()
    }

    /// 这个软件排队等下一发请求生效的那条。
    pub fn pending(&self, client: Client) -> Option<String> {
        self.switch.pending(client).map(str::to_string)
    }

    /// 给某个软件换上游。**只在两次请求之间生效** —— 正在飞的请求不受影响,
    /// 判定在 `qb_station::router::ClientRouter` 里,这里只是转调。
    pub fn request_switch(&mut self, client: Client, route_id: impl Into<String>) {
        self.switch.request_switch(client, route_id);
    }

    /// 第一次选上游,或使用者手动指定。
    pub fn set_now(&mut self, client: Client, route_id: impl Into<String>) {
        self.switch.set_now(client, route_id);
    }

    pub fn switches(&self) -> u64 {
        self.switch.switches()
    }

    /// 每条线此刻的 `(降权系数, 是否熔断)`。
    ///
    /// **排序要的熔断状态只能从这里取。** 在别处再算一份的话,
    /// 排序看到的和真正转发时用的会漂开 —— 界面显示「已熔断」却还在往那条线打,
    /// 或者反过来。
    pub fn breaker_state(&self, now_ms: i64) -> HashMap<String, (f64, bool)> {
        self.breakers
            .iter()
            .map(|(id, b)| (id.clone(), (b.demotion(now_ms), b.tripped(now_ms))))
            .collect()
    }

    /// 这条线现在能不能用(没熔断)。
    pub fn usable(&self, route_id: &str, now_ms: i64) -> bool {
        self.breakers
            .get(route_id)
            .is_none_or(|b| !b.tripped(now_ms))
    }

    /// 取走已经攒下的请求日志。落库由调用方做 —— 这一层不碰数据库。
    pub fn drain_logs(&mut self) -> Vec<RequestLog> {
        std::mem::take(&mut self.logs)
    }

    pub fn logs(&self) -> &[RequestLog] {
        &self.logs
    }

    // ---------------------------------------------------------- 官方 Codex turn-state
    //
    // ⛔ 只服务官方 Codex（`UpstreamAuth::OAuthPassthrough`）。Claude 不碰。

    /// 配置注入开关 + 账号规则。**面板设置入口。**
    pub fn turnstate_configure(&mut self, enabled: bool, kind: TurnKind) {
        // 换了账号规则,旧 Store 的期望块数就不对了 —— 清掉重来。
        if self.turnstate.kind != kind {
            self.turnstate.stores.clear();
        }
        self.turnstate.enabled = enabled;
        self.turnstate.kind = kind;
    }

    /// 注入是否开着。
    pub fn turnstate_enabled(&self) -> bool {
        self.turnstate.enabled
    }

    /// 当前账号规则（个人 / Team）。
    ///
    /// 给界面回读用：账户页的开关必须显示**后端真实**的开关与规则，
    /// 不能靠前端本地一个 `useState(false)` 猜 —— 面板重开、换页之后那个猜的值
    /// 就跟后端对不上了，界面上「关」着而实际在注。
    pub fn turnstate_kind(&self) -> TurnKind {
        self.turnstate.kind
    }

    fn turnstate_store(&mut self, model: &str) -> &mut Store {
        let policy = Policy::for_kind(self.turnstate.kind.into());
        self.turnstate
            .stores
            .entry(model.to_string())
            .or_insert_with(|| Store::new(policy))
    }

    /// 取一份可注入的 turn-state（注入关着 / 没有可用值时返回 `None`）。
    fn turnstate_acquire(&mut self, model: &str, now_ms: i64) -> Option<Injection> {
        if !self.turnstate.enabled {
            return None;
        }
        self.turnstate_store(model).acquire(now_ms)
    }

    /// 把官方响应带回的 turn-state 被动采集进来（免费,不额外发请求）。
    fn turnstate_offer(&mut self, model: &str, value: &str, now_ms: i64) {
        if let Some(shape) = turnstate::parse(value) {
            self.turnstate_store(model).offer(shape, now_ms);
        }
    }

    /// 观测注入过的那一发请求回来的 turn-state,更新 strikes。
    fn turnstate_observe(&mut self, model: &str, value: &str, used: &Injection, now_ms: i64) {
        if let Some(store) = self.turnstate.stores.get_mut(model) {
            store.observe(value, used, now_ms);
        }
    }

    /// 每个 model 当前的 turn-state 外形,给界面看。**不含值本身。**
    pub fn turnstate_status(&self, now_ms: i64) -> Vec<turnstate::ModelStatus> {
        let mut rows: Vec<_> = self
            .turnstate
            .stores
            .iter()
            .map(|(model, store)| turnstate::ModelStatus {
                model: model.clone(),
                status: store.status(now_ms),
            })
            .collect();
        rows.sort_by(|a, b| a.model.cmp(&b.model));
        rows
    }
}

/// 启动参数。
#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// 监听端口。0 = 让系统挑一个空闲的(测试用)。
    pub port: u16,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self { port: DEFAULT_PORT }
    }
}

impl RouterConfig {
    /// **写死 127.0.0.1,没有可配置项。** 理由见模块头。
    fn bind(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))
    }
}

/// 跑起来的路由器。
///
/// **`stop` 被丢掉时路由自己会停。** 放在这里而不是留一个全局开关,
/// 是因为「谁持有这个 handle,谁决定它活多久」比一个谁都能拨的开关好查。
pub struct RouterHandle {
    /// **真实绑上的地址**,不是请求的那个(端口传 0 时由系统分配)。
    pub addr: SocketAddr,
    pub state: Arc<Mutex<RouterState>>,
    stop: tokio::sync::watch::Sender<bool>,
}

impl RouterHandle {
    /// 停掉监听。已经在飞的请求会跑完 —— **半路掐断等于把使用者正在写的
    /// 那一轮对话弄丢**,而停路由从来不是紧急操作。
    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }
}

impl RouterHandle {
    /// 客户端该把 base_url 指到哪。
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.addr.port())
    }
}

/// **这个软件**该把 base_url 指到哪。
///
/// 跟 [`RouterHandle::base_url`] 差的就是那一段前缀 —— 而那一段正是
/// [`route_of`] 用来认软件的唯一依据:Claude Code 与 Claude 桌面端走的是
/// 同一套 Anthropic 协议,协议上分不开。
///
/// ⛔ **这里和 [`route_of`] 必须对着写。** 一头加了前缀另一头不剥,
/// 上游收到的是 `https://站点/v1/cd/v1/messages`,404;而两头都不加的话
/// 桌面端的请求会被记成 Claude Code 的,日志和健康度全串到另一条线上。
/// `the_base_url_we_hand_out_is_the_one_route_of_reads_back` 钉着这件事。
pub fn client_base_url(client: Client, port: u16) -> String {
    let base = format!("http://127.0.0.1:{port}");
    match client {
        Client::ClaudeCode => base,
        // Models and auxiliary requests do not identify their client by endpoint.
        Client::Codex => format!("{base}{CODEX_PREFIX}"),
        Client::ClaudeDesktop => format!("{base}{DESKTOP_PREFIX}"),
    }
}

/// 走本机路由的环境在配置文件里填哪把 Key。
///
/// # 为什么不是留空
///
/// 路由会把客户端带上来的鉴权头**整个丢掉**,换成当前上游那把
/// (见 [`CLIENT_AUTH_HEADERS`]),所以这里填什么都到不了站点。
/// 但**留空不行**:客户端发现没有 Key 就会转去走官方 OAuth 登录,
/// 而那条路会把一个官方身份塞进中转环境目录里 —— Codex 那边有一条
/// 专门的断言在拦它(「中转环境目录出现 OAuth 凭证」)。
///
/// 填一个一眼认得出来源的常量,而不是随机串:使用者在配置文件里看见它时
/// 该立刻明白这不是他的 Key。
pub const ROUTER_KEY: &str = "qb-gate-local-router";

/// 官方 Codex turn-state 上游的固定 route_id。整机一条。
///
/// 固定而不是随机:一台机器只有一条官方线,固定下来
/// [`RouterState::replace_upstreams`] 才认得出要保住哪一条,
/// 界面也才对得上「已接入」的是不是它。
pub const OFFICIAL_CODEX_ROUTE_ID: &str = "qb-router-codex-official";

/// 官方 Codex 的真实端点。请求带着客户端自己的 ChatGPT OAuth 转发到这里。
///
/// ⛔ **不带 `/v1`。** 官方端点是 `.../backend-api/codex/responses`（在装机版
/// Codex 二进制里逐字核过);客户端那头的 base_url 也不带 `/v1`
/// （见 `workspace::env_base` 对官方环境的分支）——两头都不带,
/// `route_of` 剥掉 `/codex` 之后 [`join_url`] 拼出来正好是官方那条路径。
/// 任何一头多一段 `/v1`,上游收到的就是 `.../codex/v1/responses`,404。
pub const OFFICIAL_CODEX_BASE: &str = "https://chatgpt.com/backend-api/codex";

type ProxyBody = BoxBody<Bytes, std::io::Error>;

/// 日志里 client 那一栏写什么。
///
/// 跟 Client 在 IPC / TS 那边的 kebab-case 写法**是同一个字符串** ——
/// 另写一份映射就会漂,而漂了之后前端按 client 过滤会静默筛不出东西。
fn client_key(c: Client) -> &'static str {
    match c {
        Client::ClaudeCode => "claude-code",
        Client::ClaudeDesktop => "claude-desktop",
        Client::Codex => "codex",
    }
}

fn text(status: StatusCode, body: &str) -> Response<ProxyBody> {
    let mut r = Response::new(
        Full::new(Bytes::from(body.to_owned()))
            .map_err(|e| match e {})
            .boxed(),
    );
    *r.status_mut() = status;
    r
}

/// `Retry-After` 换算成毫秒。
///
/// 只认秒数那种写法。HTTP 日期那种要拿当前时间去减,而判定层不许取系统时间;
/// 认不出来就返回 `None`,熔断那边会退回默认冷却期 —— 比猜一个数安全。
pub fn retry_after_ms(raw: Option<&str>) -> Option<i64> {
    let secs: f64 = raw?.trim().parse().ok()?;
    (secs.is_finite() && secs >= 0.0).then_some((secs * 1000.0) as i64)
}

/// 把上游基地址和客户端请求的路径拼起来。
///
/// 两边的斜杠都规整掉 —— `base/` + `/v1/x` 拼成 `base//v1/x` 的话,
/// 有些上游会 404,而错误信息里完全看不出是多了一个斜杠。
pub fn join_url(base: &str, path_and_query: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path_and_query.trim_start_matches('/');
    let path = if base.ends_with("/v1") {
        path.strip_prefix("v1/").unwrap_or(path)
    } else {
        path
    };
    format!("{base}/{path}")
}

/// Claude 桌面端的路径前缀。
///
/// Claude 桌面端和 Codex 都使用自己的前缀。模型列表与辅助端点在多个客户端
/// 之间同名，不能根据协议路径猜身份；旧版无前缀 Responses 路径仍兼容。
const DESKTOP_PREFIX: &str = "/cd";
const CODEX_PREFIX: &str = "/codex";

/// 从请求路径认出这一发是哪个软件发来的,并给出**要转发给上游的路径**。
///
/// | 客户端打过来的 | 认成 | 转给上游的 |
/// |---|---|---|
/// | `/cd/v1/messages` | Claude 桌面端 | `/v1/messages` |
/// | `/v1/messages` | Claude Code | 原样 |
/// | `/codex/v1/...` | Codex | `/v1/...` |
/// | `/v1/responses` · `/v1/chat/completions` | 旧版 Codex | 原样 |
/// | 其它 | Claude Code（兜底） | 原样 |
///
/// # ⛔ 前缀必须剥掉再转发
///
/// 不剥的话上游收到的是 `https://站点/v1/cd/v1/messages` —— 404,
/// 而错误信息里完全看不出多了一段。
///
/// # 为什么兜底是 Claude Code 而不是报错
///
/// 客户端会打一些我们没列举的辅助路径（`/v1/models` 之类）。
/// 一律拒绝会让那些请求全挂;而认错软件的代价只是日志上那一栏归错类,
/// 不影响这一发请求本身打到哪。两害相权取轻。
pub fn route_of(path: &str) -> (Client, String) {
    if let Some(rest) = path.strip_prefix(CODEX_PREFIX) {
        if rest.is_empty() || rest.starts_with('/') {
            return (
                Client::Codex,
                if rest.is_empty() { "/" } else { rest }.to_string(),
            );
        }
    }
    if let Some(rest) = path.strip_prefix(DESKTOP_PREFIX) {
        // `/cd` 后面必须是 `/` 或者到头 —— 否则 `/cdx/...` 会被误剥。
        if rest.is_empty() || rest.starts_with('/') {
            let forward = if rest.is_empty() { "/" } else { rest };
            return (Client::ClaudeDesktop, forward.to_string());
        }
    }
    // OpenAI 那两个端点自带区分,不需要前缀。只看路径部分,别被查询串骗了。
    let bare = path.split('?').next().unwrap_or(path);
    if bare.ends_with("/responses") || bare.ends_with("/chat/completions") {
        return (Client::Codex, path.to_string());
    }
    (Client::ClaudeCode, path.to_string())
}

/// 从请求体里把模型名抠出来。
///
/// 三家的请求体都有一个顶层 `model` 字段。抠不出来就是空串 ——
/// **不猜**:日志上「模型」那一栏空着，比写一个错的模型名好查得多。
///
/// 只看顶层、只认字符串,不递归找 —— 递归会在某些嵌套结构里捞出别的东西来。
pub fn model_of(body: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("model")?.as_str().map(str::to_string))
        .unwrap_or_default()
}
/// 该不该把这个请求头转给上游。
pub fn forwardable(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !HOP_BY_HOP.contains(&lower.as_str()) && !CLIENT_AUTH_HEADERS.contains(&lower.as_str())
}

/// 启动本机路由。
///
/// 绑不上端口时**如实报错**,把端口号写进错误里 —— 开源分发意味着
/// 别人机器上什么都可能占着这个端口。
pub async fn serve(
    config: RouterConfig,
    state: Arc<Mutex<RouterState>>,
    client: reqwest::Client,
) -> Result<RouterHandle> {
    let want = config.bind();
    let listener = tokio::net::TcpListener::bind(want).await.map_err(|e| {
        GateError::Other(format!(
            "本机路由绑不上 127.0.0.1:{} —— {e}。这个端口多半被别的程序占了,换一个再试。",
            want.port()
        ))
    })?;
    let addr = listener
        .local_addr()
        .map_err(|e| GateError::Other(format!("拿不到本机路由的真实端口：{e}")))?;

    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    let accept_state = Arc::clone(&state);
    tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                // 停的时候不再收新连接;已经在飞的那些自己跑完。
                _ = stopped.changed() => break,
                a = listener.accept() => a,
            };
            let Ok((stream, _)) = accepted else {
                // 单次 accept 失败不该让整个路由停掉 —— 客户端会重连。
                continue;
            };
            let state = Arc::clone(&accept_state);
            let client = client.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |req| proxy(req, Arc::clone(&state), client.clone()));
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), svc)
                    .await;
            });
        }
    });

    Ok(RouterHandle { addr, state, stop })
}

/// 请求结束时把总耗时补回日志里。
///
/// 用 `Drop` 而不是在 handler 末尾写:响应体是流式的,handler 早就返回了,
/// 真正的「这一发请求结束」是那个流被丢掉的时候。
struct FinishLog {
    state: Arc<Mutex<RouterState>>,
    id: String,
    started: Instant,
    /// 哪个软件那一发 —— 收尾要还到它自己的计数上。
    client: Client,
    /// 这一发绑的线路 —— 流结束后按 SSE 结局补记健康度要用。
    route_id: String,
    /// Codex 的 2xx 流才有:顺流累积的 SSE 扫描器。
    ///
    /// **健康判定推迟到这里**:HTTP 200 但流里 `response.failed` / 断流的那种,
    /// 只看状态码会误判成成功。`None` = 非 Codex 或非 2xx,健康早已按状态码记过。
    scan: Arc<Mutex<Option<StreamScanner>>>,
}

impl Drop for FinishLog {
    fn drop(&mut self) {
        let elapsed = self.started.elapsed().as_millis() as u64;
        // 流真的结束了才知道它的结局。取出扫描器折算成熔断器认识的结果。
        let deferred = self
            .scan
            .lock()
            .ok()
            .and_then(|mut g| g.take())
            .map(|s| s.finish().to_outcome());
        if let Ok(mut s) = self.state.lock() {
            s.switch.end(self.client);
            if let Some(log) = s.logs.iter_mut().find(|l| l.id == self.id) {
                log.total_ms = Some(elapsed);
            }
            if let Some(outcome) = deferred {
                let now_ms = chrono::Utc::now().timestamp_millis();
                s.breakers
                    .entry(self.route_id.clone())
                    .or_default()
                    .record(outcome, now_ms);
            }
        }
    }
}

async fn proxy(
    req: Request<Incoming>,
    state: Arc<Mutex<RouterState>>,
    client: reqwest::Client,
) -> std::result::Result<Response<ProxyBody>, std::convert::Infallible> {
    let started = Instant::now();
    let now_ms = chrono::Utc::now().timestamp_millis();

    let raw_path = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "/".into());
    // 先认这一发是哪个软件发来的,再按那个软件去绑线 ——
    // 三个软件各有各的当前上游,认错软件就会打到别人的站上。
    // 变量名叫 caller 不叫 client —— 这个函数的参数里已经有一个 reqwest::Client。
    let (caller, path) = route_of(&raw_path);

    // 这一发请求绑定一条线路。**拿到的是一份拷贝** —— 之后换上游与它无关。
    let bound = {
        let Ok(mut s) = state.lock() else {
            return Ok(text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "本机路由内部状态损坏",
            ));
        };
        s.switch
            .begin(caller)
            .and_then(|id| s.upstreams.get(&id).cloned().map(|u| (id, u)))
    };

    let Some((route_id, upstream)) = bound else {
        // 这个软件还没选上游。**如实报错,不制造死局,也不回落到别的软件的线** ——
        // 回落会让它悄悄用上别人的 Key,账记在别人头上,而且完全看不出来。
        if let Ok(mut s) = state.lock() {
            s.switch.end(caller);
        }
        return Ok(text(
            StatusCode::SERVICE_UNAVAILABLE,
            "本机路由还没有给这个软件选定上游线路：请在中转站页面切到它，选一条再启动。",
        ));
    };

    let log_id = format!("{}-{}", now_ms, started.elapsed().as_nanos());
    let method = req.method().clone();

    // 官方 Codex turn-state 模式:保留客户端自带的 OAuth,不剥不换。见 `UpstreamAuth`。
    let official = matches!(upstream.auth, UpstreamAuth::OAuthPassthrough);
    // 客户端有没有自己带 turn-state —— 带了就保留它自己的,我们不覆盖。
    let client_has_state = req.headers().contains_key(turnstate::HEADER);

    // 头:丢掉逐跳头;中转模式还要丢掉客户端自带的凭证再换上这条线路自己的,
    // 官方模式则保留客户端的 OAuth。
    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in req.headers() {
        let keep = if official {
            !HOP_BY_HOP.contains(&name.as_str().to_ascii_lowercase().as_str())
        } else {
            forwardable(name.as_str())
        };
        if keep {
            headers.insert(name.clone(), value.clone());
        }
    }
    match &upstream.auth {
        UpstreamAuth::Bearer(k) => {
            if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {k}")) {
                headers.insert(reqwest::header::AUTHORIZATION, v);
            }
        }
        UpstreamAuth::ApiKey(k) => {
            if let Ok(v) = reqwest::header::HeaderValue::from_str(k) {
                headers.insert("x-api-key", v);
            }
        }
        // 官方模式用客户端自带的 OAuth,不注入;None 也不注入。
        UpstreamAuth::None | UpstreamAuth::OAuthPassthrough => {}
    }

    let body = match req.into_body().collect().await {
        Ok(b) => b.to_bytes(),
        Err(e) => {
            if let Ok(mut s) = state.lock() {
                s.switch.end(caller);
            }
            return Ok(text(
                StatusCode::BAD_REQUEST,
                &format!("读不到客户端请求体：{e}"),
            ));
        }
    };

    // 模型名从请求体里抠 —— 日志表上「模型」那一栏要用。抠不出来就空着。
    let model = model_of(&body);

    // 官方 Codex:注入开着、客户端没带、库里有可用值时,补上一张 turn-state。
    // 客户端自己带了就保留(见上面 `client_has_state`),不弄坏它自己的状态。
    let injected = if official && caller == Client::Codex && !client_has_state {
        state
            .lock()
            .ok()
            .and_then(|mut s| s.turnstate_acquire(&model, now_ms))
    } else {
        None
    };
    if let Some(inj) = &injected {
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&inj.value) {
            headers.insert(turnstate::HEADER, v);
        }
    }

    // 转给上游的是**剥掉前缀之后**的路径（见 `route_of`）。不剥的话上游
    // 收到的是 /v1/cd/v1/messages —— 404,而错误里完全看不出多了一段。
    let url = join_url(&upstream.base_url, &path);
    let sent = client
        .request(method, &url)
        .headers(headers)
        .body(body)
        .send()
        .await;

    let mut resp = match sent {
        Ok(r) => r,
        Err(e) => {
            // 连不上或超时 —— 对熔断来说是同一档。登记 + 立刻按超时判健康。
            record_log(
                &state,
                &log_id,
                now_ms,
                &route_id,
                caller,
                &model,
                None,
                None,
                Outcome::timeout(),
            );
            record_breaker(&state, &route_id, Outcome::timeout(), now_ms);
            if let Ok(mut s) = state.lock() {
                s.switch.end(caller);
            }
            return Ok(text(StatusCode::BAD_GATEWAY, &format!("上游没回应：{e}")));
        }
    };

    let status = resp.status().as_u16();
    let retry = retry_after_ms(
        resp.headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok()),
    );
    let outcome = Outcome::from_status(status, retry);

    // 官方 Codex:把响应带回的 turn-state **被动采集**进来(免费),
    // 并对我们注入过的那一发更新 strikes。这张头同时照常转回客户端,
    // 让 Codex 自己的状态也接得上。
    if official && caller == Client::Codex {
        let returned = resp
            .headers()
            .get(turnstate::HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        if let Ok(mut s) = state.lock() {
            s.turnstate_offer(&model, &returned, now_ms);
            if let Some(inj) = &injected {
                s.turnstate_observe(&model, &returned, inj, now_ms);
            }
        }
    }

    let mut out = Response::builder().status(status);
    for (name, value) in resp.headers() {
        if forwardable(name.as_str()) {
            out = out.header(name, value);
        }
    }

    // 首字：拉出第一块再计时。流式接口的价值全在这个数上,
    // 拿「响应头回来了」当首字会系统性地偏小。
    let first = resp.chunk().await;
    let first_token_ms = Some(started.elapsed().as_millis() as u64);

    // ⛔ Codex 的 2xx 流:健康判定推迟到流结束。
    //
    // Codex 的 Responses 接口会在 HTTP 200 的 SSE 流里报 `response.failed` / 断流,
    // 只看状态码会把这些记成成功、熔断永远不触发(见 `qb_station::sse` 模块头)。
    // 所以这里先只登记请求,不动熔断;流结束时在 `FinishLog::drop` 按 SSE 结局补记。
    //
    // **只对 Codex。** 使用者明确要求不碰 Claude —— Claude Code / 桌面端一律维持原样:
    // 立刻按状态码判健康,一个字节都不改。
    let defer_health = caller == Client::Codex && (200..300).contains(&status);
    record_log(
        &state,
        &log_id,
        now_ms,
        &route_id,
        caller,
        &model,
        Some(status),
        first_token_ms,
        outcome,
    );
    if !defer_health {
        record_breaker(&state, &route_id, outcome, now_ms);
    }

    // 顺流累积的 SSE 扫描器 —— 只对要推迟判定的那种流建。
    let scan: Arc<Mutex<Option<StreamScanner>>> =
        Arc::new(Mutex::new(defer_health.then(StreamScanner::new)));
    if let Ok(Some(b)) = &first {
        if let Ok(mut g) = scan.lock() {
            if let Some(s) = g.as_mut() {
                s.feed(b);
            }
        }
    }

    let finish = FinishLog {
        state: Arc::clone(&state),
        id: log_id,
        started,
        client: caller,
        route_id: route_id.clone(),
        scan: Arc::clone(&scan),
    };

    let head = match first {
        Ok(Some(b)) => Some(Ok(Frame::data(b))),
        Ok(None) => None,
        Err(e) => Some(Err(std::io::Error::other(e.to_string()))),
    };
    let rest = resp.bytes_stream().map(move |c| match c {
        Ok(b) => {
            // 顺流喂给扫描器,不缓冲整条响应 —— 只留未终止的那一小段尾巴。
            if let Ok(mut g) = scan.lock() {
                if let Some(s) = g.as_mut() {
                    s.feed(&b);
                }
            }
            Ok(Frame::data(b))
        }
        Err(e) => Err(std::io::Error::other(e.to_string())),
    });
    // `finish` 被搬进流里：流被丢掉的那一刻才是这一发请求真的结束了。
    let stream = futures_util::stream::iter(head).chain(rest).map(move |c| {
        let _keep = &finish;
        c
    });

    // 显式点名 BodyExt::boxed —— StreamExt 也有一个 boxed,重名。
    let body = BodyExt::boxed(StreamBody::new(stream));
    match out.body(body) {
        Ok(r) => Ok(r),
        Err(e) => Ok(text(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("拼不出响应：{e}"),
        )),
    }
}

/// 只登记这一发请求的日志行(不碰熔断)。
///
/// ⛔ 日志与熔断**分开**:Codex 的 2xx 流要先登记、把健康判定推迟到流结束
/// (见 `proxy` 里的 `defer_health`);其它情况登记完紧接着 [`record_breaker`]。
#[allow(clippy::too_many_arguments)]
fn record_log(
    state: &Arc<Mutex<RouterState>>,
    id: &str,
    at_ms: i64,
    route_id: &str,
    client: Client,
    model: &str,
    status: Option<u16>,
    first_token_ms: Option<u64>,
    outcome: Outcome,
) {
    let retry_after_ms = match outcome {
        Outcome::RateLimited { retry_after_ms } => retry_after_ms,
        _ => None,
    };
    let Ok(mut s) = state.lock() else { return };
    s.logs.push(RequestLog {
        id: id.to_string(),
        at_ms,
        route_id: route_id.to_string(),
        // 这两栏原来是写死的空串 —— 日志表上「来源」和「模型」两列因此
        // 永远是空的,而那看起来像「还没有请求」而不是「没记」。
        client: client_key(client).to_string(),
        model: model.to_string(),
        status,
        first_token_ms,
        total_ms: None,
        retry_after_ms,
    });
}

/// 把一次结果记进这条线路的熔断器。
fn record_breaker(state: &Arc<Mutex<RouterState>>, route_id: &str, outcome: Outcome, at_ms: i64) {
    let Ok(mut s) = state.lock() else { return };
    s.breakers
        .entry(route_id.to_string())
        .or_default()
        .record(outcome, at_ms);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 发出去的 base_url 和收回来的路径判定**必须是同一套前缀**。
    ///
    /// 这两头分别在 [`client_base_url`] 和 [`route_of`] 里,中间隔着整个客户端进程。
    /// 对不上的症状是「桌面端的请求全记在 Claude Code 名下」—— 日志、健康度、
    /// 曲线图统统串到另一条线上,而每一发请求都成功,看不出任何异常。
    #[test]
    fn the_base_url_we_hand_out_is_the_one_route_of_reads_back() {
        for client in [Client::ClaudeCode, Client::ClaudeDesktop, Client::Codex] {
            let base = client_base_url(client, DEFAULT_PORT);
            // 客户端会在 base 后面接上它自己那条路径。
            let path = match client {
                Client::Codex => "/v1/responses",
                _ => "/v1/messages",
            };
            let full = format!("{base}{path}");
            let after_host = full
                .strip_prefix(&format!("http://127.0.0.1:{DEFAULT_PORT}"))
                .expect("base 必须只绑回环");
            let (got, forward) = route_of(after_host);
            assert_eq!(got, client, "{full} 被认成了 {got:?}");
            assert_eq!(forward, path, "{full} 转给上游时前缀没剥干净");
        }
    }

    /// ⛔ Key 不能留空。留空客户端会转去走官方 OAuth,
    /// 而那条路会把一个官方身份塞进中转环境目录。
    #[test]
    fn the_placeholder_key_is_recognisable_and_not_empty() {
        assert!(!ROUTER_KEY.trim().is_empty());
        assert!(ROUTER_KEY.contains("qb-gate"), "看见它要认得出来源");
    }

    #[test]
    fn hop_by_hop_and_client_credentials_are_never_forwarded() {
        // 逐跳头转上去会让连接复用和分块编码两头打架,症状是偶发截断,
        // 而且只在流式下出现 —— 最难查的那一类。
        for h in ["Connection", "transfer-encoding", "Host", "content-length"] {
            assert!(!forwardable(h), "{h} 不该被转发");
        }
        // 客户端里残留的旧 Key 跟着上去,会让账记在旧 Key 上而完全看不出来。
        for h in ["Authorization", "x-api-key", "API-Key"] {
            assert!(!forwardable(h), "{h} 不该被转发");
        }
        for h in ["content-type", "accept", "anthropic-version", "user-agent"] {
            assert!(forwardable(h), "{h} 应当转发");
        }
    }

    #[test]
    fn urls_join_without_doubling_or_dropping_slashes() {
        // `base/` + `/v1/x` 拼成 `base//v1/x` 的话有些上游直接 404,
        // 而错误信息里看不出是多了一个斜杠。
        for (base, path) in [
            ("https://x.test/v1", "/messages"),
            ("https://x.test/v1/", "/messages"),
            ("https://x.test/v1", "messages"),
            ("https://x.test/v1/", "messages"),
            ("https://x.test/v1", "/v1/messages"),
            ("https://x.test/v1/", "/v1/messages"),
        ] {
            assert_eq!(join_url(base, path), "https://x.test/v1/messages");
        }
    }

    #[test]
    fn retry_after_only_accepts_what_it_can_actually_read() {
        assert_eq!(retry_after_ms(Some("5")), Some(5_000));
        assert_eq!(retry_after_ms(Some(" 2.5 ")), Some(2_500));
        assert_eq!(retry_after_ms(Some("0")), Some(0));
        // HTTP 日期那种写法要拿当前时间去减 —— 认不出来就退回默认冷却期,
        // 比猜一个数安全。
        assert_eq!(retry_after_ms(Some("Wed, 21 Oct 2026 07:28:00 GMT")), None);
        assert_eq!(retry_after_ms(Some("-1")), None);
        assert_eq!(retry_after_ms(None), None);
    }

    /// 路径前缀认软件。**Claude Code 与桌面端协议一样，只能靠前缀分。**
    #[test]
    fn the_path_prefix_is_what_tells_claude_code_and_the_desktop_apart() {
        assert_eq!(route_of("/v1/messages").0, Client::ClaudeCode);
        assert_eq!(route_of("/cd/v1/messages").0, Client::ClaudeDesktop);
        // Codex 的两个端点自带区分，不需要前缀。
        assert_eq!(route_of("/v1/responses").0, Client::Codex);
        assert_eq!(route_of("/v1/chat/completions").0, Client::Codex);
        assert_eq!(
            route_of("/codex/v1/models"),
            (Client::Codex, "/v1/models".into())
        );
        assert_eq!(
            route_of("/codex/v1/responses/compact?x=1"),
            (Client::Codex, "/v1/responses/compact?x=1".into())
        );
        assert_eq!(route_of("/codex-other/v1/models").0, Client::ClaudeCode);
    }

    /// 前缀必须**剥掉**再转发。
    ///
    /// 不剥的话上游收到的是 /v1/cd/v1/messages —— 404，
    /// 而错误信息里完全看不出多了一段。
    #[test]
    fn the_desktop_prefix_is_stripped_before_the_request_goes_upstream() {
        assert_eq!(route_of("/cd/v1/messages").1, "/v1/messages");
        assert_eq!(route_of("/cd/").1, "/");
        assert_eq!(route_of("/cd").1, "/");
        // 查询串要原样带着走。
        assert_eq!(route_of("/cd/v1/models?limit=1").1, "/v1/models?limit=1");
        // 别的软件的路径一个字都不许动。
        assert_eq!(route_of("/v1/messages").1, "/v1/messages");
    }

    /// `/cdx/...` 不是桌面端 —— 前缀后面必须是 `/` 或者到头。
    #[test]
    fn a_path_that_merely_starts_with_the_prefix_letters_is_not_the_desktop() {
        let (c, p) = route_of("/cdx/v1/messages");
        assert_eq!(c, Client::ClaudeCode);
        assert_eq!(p, "/cdx/v1/messages", "不该被剥掉一段");
    }

    /// 查询串里出现 /responses 不该把它认成 Codex。
    #[test]
    fn only_the_path_decides_the_client_not_the_query_string() {
        assert_eq!(
            route_of("/v1/messages?next=/responses").0,
            Client::ClaudeCode
        );
    }

    /// 认不出来的路径兜底到 Claude Code，**不拒绝**。
    ///
    /// 客户端会打一些我们没列举的辅助路径。一律拒绝会让那些请求全挂；
    /// 而认错软件的代价只是日志上那一栏归错类。
    #[test]
    fn an_unlisted_path_falls_back_instead_of_failing_the_request() {
        assert_eq!(route_of("/v1/models").0, Client::ClaudeCode);
        assert_eq!(route_of("/").0, Client::ClaudeCode);
    }

    /// 模型名从请求体顶层抠。**抠不出来就空着，不猜。**
    #[test]
    fn the_model_comes_from_the_body_and_is_empty_when_it_cannot_be_read() {
        assert_eq!(
            model_of(br#"{"model":"claude-opus-5","x":1}"#),
            "claude-opus-5"
        );
        // 不是 JSON、没有这个字段、字段不是字符串 —— 一律空串。
        assert_eq!(model_of(b"not json"), "");
        assert_eq!(model_of(br#"{"x":1}"#), "");
        assert_eq!(model_of(br#"{"model":42}"#), "");
        assert_eq!(model_of(b""), "");
        // 只看顶层：嵌套里的 model 不算，递归会在别的结构里捞出错东西。
        assert_eq!(model_of(br#"{"body":{"model":"deep"}}"#), "");
    }

    /// 日志里的 client 写法必须跟 IPC/TS 那边一字不差。
    ///
    /// 漂了之后前端按 client 过滤会**静默筛不出东西** —— 不报错，只是空表。
    #[test]
    fn the_logged_client_key_matches_the_wire_format() {
        assert_eq!(client_key(Client::ClaudeCode), "claude-code");
        assert_eq!(client_key(Client::ClaudeDesktop), "claude-desktop");
        assert_eq!(client_key(Client::Codex), "codex");
        // 跟 serde 导出的那一份对齐（Route::make_id 用的也是这三个字符串）。
        for c in [Client::ClaudeCode, Client::ClaudeDesktop, Client::Codex] {
            let wire = serde_json::to_string(&c).unwrap();
            assert_eq!(wire.trim_matches('"'), client_key(c));
        }
    }
    #[test]
    fn the_listener_address_is_always_loopback() {
        // 绑上 0.0.0.0 的那一刻,同一个局域网里的任何人都能拿你的 Key
        // 打你的额度,而且账单看起来完全正常。这个软件是开源分发的。
        for port in [0u16, 15721, 65535] {
            let addr = RouterConfig { port }.bind();
            assert!(addr.ip().is_loopback(), "{addr} 不是回环地址");
            assert_eq!(addr.ip().to_string(), "127.0.0.1");
        }
    }

    #[test]
    fn switching_upstreams_never_touches_account_slots() {
        // 这个模块只认识线路 id 和 base_url。账户槽位切换在 qb-accounts,
        // 而 qb-station 压根不依赖它 —— architecture.rs 里有一条测试钉着。
        let mut s = RouterState::default();
        s.put_upstream(Upstream {
            route_id: "r1".into(),
            base_url: "https://x.test".into(),
            auth: UpstreamAuth::None,
        });
        s.set_now(Client::ClaudeCode, "r1");
        s.request_switch(Client::ClaudeCode, "r2");
        assert_eq!(
            s.current(Client::ClaudeCode).as_deref(),
            Some("r1"),
            "换上游不该立刻生效"
        );
    }

    #[test]
    fn an_unknown_route_is_not_usable_but_does_not_panic() {
        let s = RouterState::default();
        assert!(s.usable("从没见过的线路", 0), "没有熔断记录 = 还没坏过");
    }

    /// ⛔ 中转线路整批重装**不许冲掉**官方 Codex 上游。
    ///
    /// 官方线是 `arm_official_codex` 临时挂上去的,不在 SQLite 的 station_routes 里;
    /// 使用者接进路由之后随手编辑一条中转线路会触发 `load_upstreams` → `replace_upstreams`,
    /// 不保留的话官方线就没了,Codex 下一发请求 503 而看不出所以然。
    #[test]
    fn reloading_relay_upstreams_keeps_the_official_codex_line() {
        let mut s = RouterState::default();
        s.arm_official_codex();
        assert!(s.official_codex_armed());
        // 模拟一次中转线路重装(只带中转线,不带官方线)。
        s.replace_upstreams(vec![Upstream {
            route_id: "relay-1".into(),
            base_url: "https://relay.test/v1".into(),
            auth: UpstreamAuth::Bearer("k".into()),
        }]);
        assert!(
            s.official_codex_armed(),
            "官方 Codex 上游被中转线路重装冲掉了"
        );
        assert_eq!(
            s.current(Client::Codex).as_deref(),
            Some(OFFICIAL_CODEX_ROUTE_ID),
            "官方线还在,但 Codex 的当前选择没保住"
        );
    }

    /// 官方线摘掉之后**不回落**到别的线路 —— 宁可 503,不拿官方身份打中转 Key。
    #[test]
    fn disarming_the_official_line_does_not_fall_back_to_a_relay() {
        let mut s = RouterState::default();
        s.put_upstream(Upstream {
            route_id: "relay-1".into(),
            base_url: "https://relay.test/v1".into(),
            auth: UpstreamAuth::Bearer("k".into()),
        });
        s.arm_official_codex();
        s.disarm_official_codex();
        assert!(!s.official_codex_armed());
        // current 仍指着已摘掉的官方线 —— begin() 会拿不到 upstream,转发层据此回 503。
        let bound = s
            .current(Client::Codex)
            .and_then(|id| (id == OFFICIAL_CODEX_ROUTE_ID).then_some(()));
        assert!(bound.is_some(), "current 不该被偷偷改到某条中转线上");
    }

    /// 官方线是 `OAuthPassthrough`,而 `OAuthPassthrough` 只会是官方线 ——
    /// 这条等式是 `replace_upstreams` 用来认「要保住哪一条」的依据。
    #[test]
    fn the_official_line_is_the_only_oauth_passthrough() {
        let mut s = RouterState::default();
        s.arm_official_codex();
        let passthrough: Vec<_> = s
            .upstreams
            .values()
            .filter(|u| matches!(u.auth, UpstreamAuth::OAuthPassthrough))
            .map(|u| u.route_id.clone())
            .collect();
        assert_eq!(passthrough, vec![OFFICIAL_CODEX_ROUTE_ID.to_string()]);
    }

    // ---------------------------------------------------------------- 端到端
    //
    // 下面这几条真的起一个假上游、真的转一次。
    //
    // **这不算「联网」**:全程只在 127.0.0.1 上,没有任何外部请求,
    // 不动真 ACL、不碰真进程、不写 %LOCALAPPDATA%。而且非这么测不可 ——
    // 对 `join_url` 和 `forwardable` 各写一条断言,证明不了它们真的被接起来了。
    // 拼错一个字段、少插一个头,那些单测照样全绿。

    type Seen = Arc<Mutex<Vec<(String, hyper::header::HeaderMap)>>>;

    /// 一个只在回环上跑的假上游。把收到的路径和头记下来。
    async fn stub_upstream(status: u16, body: &'static str, seen: Seen) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("假上游绑不上回环");
        let addr = listener.local_addr().expect("拿不到假上游端口");
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let seen = Arc::clone(&seen);
                tokio::spawn(async move {
                    let svc = service_fn(move |req: Request<Incoming>| {
                        let seen = Arc::clone(&seen);
                        async move {
                            let path = req.uri().path_and_query().map(|p| p.as_str().to_string());
                            if let Ok(mut g) = seen.lock() {
                                g.push((path.unwrap_or_default(), req.headers().clone()));
                            }
                            let mut r = Response::new(Full::new(Bytes::from(body)));
                            *r.status_mut() = StatusCode::from_u16(status).unwrap();
                            if status == 429 {
                                r.headers_mut().insert("retry-after", "7".parse().unwrap());
                            }
                            Ok::<_, std::convert::Infallible>(r)
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), svc)
                        .await;
                });
            }
        });
        addr
    }

    async fn router_to(upstream: SocketAddr, auth: UpstreamAuth) -> (RouterHandle, Seen) {
        let seen: Seen = Arc::new(Mutex::new(Vec::new()));
        let state = Arc::new(Mutex::new(RouterState::default()));
        {
            let mut s = state.lock().unwrap();
            s.put_upstream(Upstream {
                route_id: "r1".into(),
                base_url: format!("http://127.0.0.1:{}/v1", upstream.port()),
                auth,
            });
            s.set_now(Client::ClaudeCode, "r1");
        }
        let handle = serve(RouterConfig { port: 0 }, state, reqwest::Client::new())
            .await
            .expect("本机路由起不来");
        (handle, seen)
    }

    #[tokio::test]
    async fn a_request_is_forwarded_with_our_key_and_the_clients_own_key_stripped() {
        let seen: Seen = Arc::new(Mutex::new(Vec::new()));
        let up = stub_upstream(200, "hello-from-upstream", Arc::clone(&seen)).await;

        let state = Arc::new(Mutex::new(RouterState::default()));
        {
            let mut s = state.lock().unwrap();
            s.put_upstream(Upstream {
                route_id: "r1".into(),
                base_url: format!("http://127.0.0.1:{}/v1", up.port()),
                auth: UpstreamAuth::Bearer("real-key".into()),
            });
            s.set_now(Client::ClaudeCode, "r1");
        }
        let h = serve(
            RouterConfig { port: 0 },
            Arc::clone(&state),
            reqwest::Client::new(),
        )
        .await
        .expect("本机路由起不来");

        let body = reqwest::Client::new()
            .post(format!("{}/messages", h.base_url()))
            // 客户端里残留的旧 Key —— 绝不能跟着上去。
            .header("authorization", "Bearer client-leftover")
            .header("x-api-key", "client-leftover-2")
            .header("anthropic-version", "2023-06-01")
            .body("{}")
            .send()
            .await
            .expect("打不通本机路由")
            .text()
            .await
            .expect("读不到响应体");

        assert_eq!(body, "hello-from-upstream");

        let got = seen.lock().unwrap().clone();
        assert_eq!(got.len(), 1, "上游没收到请求");
        let (path, headers) = &got[0];
        assert_eq!(path, "/v1/messages", "路径没拼对");
        assert_eq!(
            headers.get("authorization").map(|v| v.to_str().unwrap()),
            Some("Bearer real-key"),
            "上游拿到的不是这条线路自己的 Key"
        );
        assert!(
            headers.get("x-api-key").is_none(),
            "客户端残留的 x-api-key 跟着上去了 —— 账会记在旧 Key 上而且查不出来"
        );
        // 不相干的业务头要原样转过去。
        assert_eq!(
            headers
                .get("anthropic-version")
                .map(|v| v.to_str().unwrap()),
            Some("2023-06-01")
        );

        // 日志记下来了,而且请求结束后 in_flight 归零。
        let s = state.lock().unwrap();
        let logs = s.logs();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].status, Some(200));
        assert_eq!(logs[0].route_id, "r1");
        assert!(logs[0].first_token_ms.is_some(), "首字延迟没量到");
    }

    #[tokio::test]
    async fn a_429_demotes_the_route_and_carries_retry_after_without_tripping_it() {
        // 被限流说明这条线是通的 —— 熔断它等于把一条能用的线自己掐掉。
        let seen: Seen = Arc::new(Mutex::new(Vec::new()));
        let up = stub_upstream(429, "slow down", Arc::clone(&seen)).await;
        let (h, _) = router_to(up, UpstreamAuth::None).await;

        let resp = reqwest::Client::new()
            .get(format!("{}/messages", h.base_url()))
            .send()
            .await
            .expect("打不通本机路由");
        assert_eq!(resp.status(), 429, "上游的状态码要原样传回客户端");

        let s = h.state.lock().unwrap();
        assert!(s.usable("r1", 0), "429 把线路熔断了 —— 它只该降权");
        assert_eq!(
            s.logs()[0].retry_after_ms,
            Some(7_000),
            "Retry-After 没读出来"
        );
    }

    #[tokio::test]
    async fn with_no_upstream_selected_it_says_so_instead_of_hanging() {
        // 不制造死局,也不静默吞掉 —— 「启动了但没反应」是最难查的那种。
        let state = Arc::new(Mutex::new(RouterState::default()));
        let h = serve(RouterConfig { port: 0 }, state, reqwest::Client::new())
            .await
            .expect("本机路由起不来");
        let resp = reqwest::Client::new()
            .get(format!("{}/messages", h.base_url()))
            .send()
            .await
            .expect("打不通本机路由");
        assert_eq!(resp.status(), 503);
        // 断言的是「它说了」，不是逐字对文案 —— 文案会改，而这条测试
        // 要守的是「不许挂在那儿不吭声」。
        assert!(resp.text().await.unwrap().contains("选定上游线路"));
    }

    #[tokio::test]
    async fn a_busy_port_is_reported_with_its_number_rather_than_panicking() {
        // 开源分发意味着别人机器上什么都可能占着 15721,
        // 而「启动了但客户端连不上」是最难查的那种故障。
        let squatter = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = squatter.local_addr().unwrap().port();
        // 用 match 而不是 expect_err:RouterHandle 没有 Debug,
        // 而给它派生 Debug 会连带要求整个 RouterState 都有 —— 不值当。
        let err = match serve(
            RouterConfig { port },
            Arc::new(Mutex::new(RouterState::default())),
            reqwest::Client::new(),
        )
        .await
        {
            Ok(_) => panic!("端口被占了却启动成功了"),
            Err(e) => e,
        };
        let msg = format!("{err}");
        assert!(msg.contains(&port.to_string()), "错误里没写端口号：{msg}");
    }

    #[tokio::test]
    async fn stopping_the_router_frees_the_port_for_a_restart() {
        // 托盘应用要能停了再起。停不掉的话,改端口这类操作只能靠重启整个面板。
        let state = Arc::new(Mutex::new(RouterState::default()));
        let h = serve(RouterConfig { port: 0 }, state, reqwest::Client::new())
            .await
            .expect("起不来");
        let port = h.addr.port();
        h.stop();
        // 让 accept 循环看到停止信号。
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            if tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))
                .await
                .is_ok()
            {
                return;
            }
        }
        panic!("停了之后端口 {port} 仍然占着");
    }

    #[tokio::test]
    async fn switching_upstreams_takes_effect_on_the_next_request_not_the_current_one() {
        // 换上游只在两次请求之间生效。中途换掉,那一发请求会拿着 A 站的
        // 上下文去打 B 站。
        let seen_a: Seen = Arc::new(Mutex::new(Vec::new()));
        let seen_b: Seen = Arc::new(Mutex::new(Vec::new()));
        let a = stub_upstream(200, "from-a", Arc::clone(&seen_a)).await;
        let b = stub_upstream(200, "from-b", Arc::clone(&seen_b)).await;

        let state = Arc::new(Mutex::new(RouterState::default()));
        {
            let mut s = state.lock().unwrap();
            for (id, addr) in [("a", a), ("b", b)] {
                s.put_upstream(Upstream {
                    route_id: id.into(),
                    base_url: format!("http://127.0.0.1:{}/v1", addr.port()),
                    auth: UpstreamAuth::None,
                });
            }
            s.set_now(Client::ClaudeCode, "a");
        }
        let h = serve(
            RouterConfig { port: 0 },
            Arc::clone(&state),
            reqwest::Client::new(),
        )
        .await
        .expect("本机路由起不来");

        let client = reqwest::Client::new();
        let first = client
            .get(format!("{}/m", h.base_url()))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(first, "from-a");

        state
            .lock()
            .unwrap()
            .request_switch(Client::ClaudeCode, "b");
        // 排队了,但还没生效。
        assert_eq!(
            state.lock().unwrap().current(Client::ClaudeCode).as_deref(),
            Some("a")
        );

        let second = client
            .get(format!("{}/m", h.base_url()))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(second, "from-b", "下一发请求没换到 B");
        assert_eq!(
            state.lock().unwrap().current(Client::ClaudeCode).as_deref(),
            Some("b")
        );
    }

    // ---------------------------------------------------------- SSE 健康判定
    //
    // Codex 的 Responses 会在 HTTP 200 的 SSE 流里报失败。只看状态码会把这些
    // 记成成功、熔断永不触发。下面两条真的转一次流,断言健康度看的是流的结局。

    use qb_station::schedule::breaker::TRIP_AFTER_CONSECUTIVE_ERRORS;

    async fn drive_codex(state: &Arc<Mutex<RouterState>>, sse_body: &'static str, times: u32) {
        let up = stub_upstream(200, sse_body, Arc::new(Mutex::new(Vec::new()))).await;
        {
            let mut s = state.lock().unwrap();
            s.put_upstream(Upstream {
                route_id: "r1".into(),
                base_url: format!("http://127.0.0.1:{}/v1", up.port()),
                auth: UpstreamAuth::None,
            });
            s.set_now(Client::Codex, "r1");
        }
        let h = serve(
            RouterConfig { port: 0 },
            Arc::clone(state),
            reqwest::Client::new(),
        )
        .await
        .expect("本机路由起不来");
        let client = reqwest::Client::new();
        for _ in 0..times {
            // Codex 靠路径前缀 /codex 认出来,再整发消耗完流,FinishLog 才 drop。
            let _ = client
                .post(format!("{}/codex/v1/responses", h.base_url()))
                .body("{\"model\":\"gpt-5.6-sol\"}")
                .send()
                .await
                .expect("打不通本机路由")
                .bytes()
                .await
                .expect("读不完响应流");
        }
    }

    #[tokio::test]
    async fn a_codex_200_with_a_failed_sse_stream_is_counted_as_a_fault() {
        // 这正是要修的 bug:HTTP 200,但流里明确 response.failed。
        let state = Arc::new(Mutex::new(RouterState::default()));
        drive_codex(
            &state,
            "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"internal_error\"}}}\n\n",
            TRIP_AFTER_CONSECUTIVE_ERRORS,
        )
        .await;
        // drop 是在服务端流被丢弃时发生的,跟客户端读完有一点点错位 —— 轮询等它补记。
        for _ in 0..100 {
            let now = chrono::Utc::now().timestamp_millis();
            if !state.lock().unwrap().usable("r1", now) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("Codex 的 200+response.failed 没有被计为失败,线路始终没熔断");
    }

    #[tokio::test]
    async fn a_codex_200_that_completes_normally_never_trips() {
        // 正常完成的流不该被误判成故障 —— 打两倍熔断门槛的次数也不许熔断。
        let state = Arc::new(Mutex::new(RouterState::default()));
        drive_codex(
            &state,
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n",
            TRIP_AFTER_CONSECUTIVE_ERRORS * 2,
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let now = chrono::Utc::now().timestamp_millis();
        assert!(
            state.lock().unwrap().usable("r1", now),
            "正常完成的 Codex 流被误判成故障了"
        );
    }

    // ------------------------------------------------- 官方 Codex turn-state
    //
    // 采集 + 注入。用回环假上游模拟官方端点:它在响应头回一张 turn-state,
    // 并把收到的请求头记下来,好断言 OAuth 被保留、注入发生在该发生的时候。

    /// 造一张合格的个人（10 块）turn-state,签发时间取传入的秒数。
    fn personal_token(issued_s: u64) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut raw = vec![0u8; 57 + 16 * 10];
        raw[0] = 0x80;
        raw[1..9].copy_from_slice(&issued_s.to_be_bytes());
        let mut out = String::new();
        for chunk in raw.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = *chunk.get(1).unwrap_or(&0) as u32;
            let b2 = *chunk.get(2).unwrap_or(&0) as u32;
            let n = (b0 << 16) | (b1 << 8) | b2;
            for i in 0..chunk.len() + 1 {
                out.push(A[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
            }
        }
        out
    }

    /// 一个官方端点的假身:响应头回一张 turn-state,请求头记进 `seen`。
    async fn stub_turnstate(seen: Seen, ts: String) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("假上游绑不上回环");
        let addr = listener.local_addr().expect("拿不到假上游端口");
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let seen = Arc::clone(&seen);
                let ts = ts.clone();
                tokio::spawn(async move {
                    let svc = service_fn(move |req: Request<Incoming>| {
                        let seen = Arc::clone(&seen);
                        let ts = ts.clone();
                        async move {
                            if let Ok(mut g) = seen.lock() {
                                let path =
                                    req.uri().path_and_query().map(|p| p.as_str().to_string());
                                g.push((path.unwrap_or_default(), req.headers().clone()));
                            }
                            let mut r = Response::new(Full::new(Bytes::from(
                                "data: {\"type\":\"response.completed\"}\n\n",
                            )));
                            r.headers_mut()
                                .insert("x-codex-turn-state", ts.parse().unwrap());
                            Ok::<_, std::convert::Infallible>(r)
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), svc)
                        .await;
                });
            }
        });
        addr
    }

    fn official_router() -> (Arc<Mutex<RouterState>>, String) {
        let ts = personal_token(chrono::Utc::now().timestamp() as u64);
        (Arc::new(Mutex::new(RouterState::default())), ts)
    }

    #[tokio::test]
    async fn official_codex_captures_turn_state_and_injects_it_on_the_next_request() {
        let (state, ts) = official_router();
        let seen: Seen = Arc::new(Mutex::new(Vec::new()));
        let up = stub_turnstate(Arc::clone(&seen), ts.clone()).await;
        {
            let mut s = state.lock().unwrap();
            s.put_upstream(Upstream {
                route_id: "official".into(),
                base_url: format!("http://127.0.0.1:{}/v1", up.port()),
                auth: UpstreamAuth::OAuthPassthrough,
            });
            s.set_now(Client::Codex, "official");
            s.turnstate_configure(true, TurnKind::Personal);
        }
        let h = serve(
            RouterConfig { port: 0 },
            Arc::clone(&state),
            reqwest::Client::new(),
        )
        .await
        .expect("本机路由起不来");
        let client = reqwest::Client::new();

        // 第一发:客户端带自己的 OAuth,不带 turn-state。
        let _ = client
            .post(format!("{}/codex/v1/responses", h.base_url()))
            .header("authorization", "Bearer user-oauth-token")
            .body("{\"model\":\"gpt-5.6-sol\"}")
            .send()
            .await
            .expect("打不通")
            .bytes()
            .await
            .expect("读不完");

        {
            let g = seen.lock().unwrap();
            let (_p, h0) = &g[0];
            assert_eq!(
                h0.get("authorization").map(|v| v.to_str().unwrap()),
                Some("Bearer user-oauth-token"),
                "官方模式必须保留客户端自带的 OAuth"
            );
            assert!(
                h0.get("x-codex-turn-state").is_none(),
                "库里还没值,第一发不该凭空注入"
            );
        }

        // 采集在响应返回前同步完成 —— 此刻库里应已有一张可用的 292。
        let st = state
            .lock()
            .unwrap()
            .turnstate_status(chrono::Utc::now().timestamp_millis());
        assert!(
            st.iter().any(|row| row.model == "gpt-5.6-sol"
                && row.status.usable
                && row.status.blocks == 10),
            "第一发响应里的 turn-state 没被采集进库"
        );

        // 第二发:客户端仍不带 —— 路由应把采到的那张注入上去。
        let _ = client
            .post(format!("{}/codex/v1/responses", h.base_url()))
            .header("authorization", "Bearer user-oauth-token")
            .body("{\"model\":\"gpt-5.6-sol\"}")
            .send()
            .await
            .expect("打不通")
            .bytes()
            .await
            .expect("读不完");

        let g = seen.lock().unwrap();
        let (_p, h1) = g.last().unwrap();
        assert_eq!(
            h1.get("x-codex-turn-state").map(|v| v.to_str().unwrap()),
            Some(ts.as_str()),
            "第二发应注入采到的 turn-state"
        );
    }

    #[tokio::test]
    async fn official_codex_never_overwrites_a_client_supplied_turn_state() {
        let (state, stored) = official_router();
        let client_own = personal_token(chrono::Utc::now().timestamp() as u64 - 5);
        let seen: Seen = Arc::new(Mutex::new(Vec::new()));
        let up = stub_turnstate(Arc::clone(&seen), stored).await;
        {
            let mut s = state.lock().unwrap();
            s.put_upstream(Upstream {
                route_id: "official".into(),
                base_url: format!("http://127.0.0.1:{}/v1", up.port()),
                auth: UpstreamAuth::OAuthPassthrough,
            });
            s.set_now(Client::Codex, "official");
            s.turnstate_configure(true, TurnKind::Personal);
        }
        let h = serve(
            RouterConfig { port: 0 },
            Arc::clone(&state),
            reqwest::Client::new(),
        )
        .await
        .expect("本机路由起不来");
        let client = reqwest::Client::new();

        // 先跑一发把库填上(不带 turn-state)。
        let _ = client
            .post(format!("{}/codex/v1/responses", h.base_url()))
            .header("authorization", "Bearer user-oauth-token")
            .body("{\"model\":\"gpt-5.6-sol\"}")
            .send()
            .await
            .expect("打不通")
            .bytes()
            .await
            .expect("读不完");

        // 这一发客户端自己带了一张 —— 必须保留它的,不能用库里的覆盖。
        let _ = client
            .post(format!("{}/codex/v1/responses", h.base_url()))
            .header("authorization", "Bearer user-oauth-token")
            .header("x-codex-turn-state", client_own.as_str())
            .body("{\"model\":\"gpt-5.6-sol\"}")
            .send()
            .await
            .expect("打不通")
            .bytes()
            .await
            .expect("读不完");

        let g = seen.lock().unwrap();
        let (_p, last) = g.last().unwrap();
        assert_eq!(
            last.get("x-codex-turn-state").map(|v| v.to_str().unwrap()),
            Some(client_own.as_str()),
            "客户端自带的 turn-state 被覆盖了"
        );
    }

    #[tokio::test]
    async fn turn_state_stays_off_for_claude_routes() {
        // ⛔ 只改 Codex:Claude Code 走的是普通中转模式,turn-state 一概不碰。
        let (state, ts) = official_router();
        let seen: Seen = Arc::new(Mutex::new(Vec::new()));
        let up = stub_turnstate(Arc::clone(&seen), ts).await;
        {
            let mut s = state.lock().unwrap();
            // 注意:这里故意用普通 Bearer 中转,不是 OAuthPassthrough。
            s.put_upstream(Upstream {
                route_id: "relay".into(),
                base_url: format!("http://127.0.0.1:{}/v1", up.port()),
                auth: UpstreamAuth::Bearer("relay-key".into()),
            });
            s.set_now(Client::ClaudeCode, "relay");
            s.turnstate_configure(true, TurnKind::Personal);
        }
        let h = serve(
            RouterConfig { port: 0 },
            Arc::clone(&state),
            reqwest::Client::new(),
        )
        .await
        .expect("本机路由起不来");
        let _ = reqwest::Client::new()
            .post(format!("{}/v1/messages", h.base_url()))
            .header("authorization", "Bearer client-oauth")
            .body("{\"model\":\"claude-opus-5\"}")
            .send()
            .await
            .expect("打不通")
            .bytes()
            .await
            .expect("读不完");

        // Claude 路由:中转模式照旧剥掉客户端凭证换成线路 Key,且不采集 turn-state。
        {
            let g = seen.lock().unwrap();
            let (_p, h0) = &g[0];
            assert_eq!(
                h0.get("authorization").map(|v| v.to_str().unwrap()),
                Some("Bearer relay-key"),
                "中转模式该换成线路自己的 Key"
            );
        }
        assert!(
            state
                .lock()
                .unwrap()
                .turnstate_status(chrono::Utc::now().timestamp_millis())
                .is_empty(),
            "Claude 路由不该产生任何 turn-state 状态"
        );
    }
}
