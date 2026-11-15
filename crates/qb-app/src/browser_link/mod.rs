//! Chrome 浏览器助手的本机连接桥（协议 2，2026-11-14）。
//!
//! 端点、消息、配对与重连时序、保护状态机写在 `docs/DESIGN-NOTES.zh-CN.md`
//! 「浏览器助手连接协议（协议 2）」一节 —— 插件那一侧只照那一节写。这里记实现上的不变量：
//!
//! - **没配对过不监听任何端口。** 有配对才随面板启动（[`Link::resume`]）；最后一条配对解除就停。
//! - 端口让系统挑，记进 `browser-link.json` 下次接着用；被占用就让系统另挑，已配对的插件
//!   在使用者点「一键重连」之后经认领页面得知新端口。不写死端口、不扫端口段。
//! - 长期口令按配对加密落盘（`secret::seal`），只在首次配对的回包里上一次线路；之后两边只交换
//!   HMAC 证明（[`proof`]），桥先证明自己认得口令。HTTP 请求带的是只活在一条连接里的会话口令。
//! - 一条配对 = 一个浏览器资料里的那份插件；同时最多 [`MAX_PAIRINGS`] 个，各连各的、各管各的保护。
//! - 放不放行只用 IP 锁同一个判定，外加每条连接自己的开启时出口：白名单内换 IP、查不到、
//!   手动关门、配置代次变化、检测过期、面板联动停止都算不放行。
//! - 晚到的开启不覆盖较新的停止：每条连接一个 `revision`，停止 / 断开 / 失败都把它加一，
//!   开启在等检测锁之前记下、探测回来之后核对。面板这一层的启停另有一个 `epoch`，同一个道理。
//! - 面板正常退出先给每条连接发 `offline`，再断开；异常断开没有这条消息。两种情况插件都拦。
//! - 这一层不认识 Tauri、不写审计日志、不打开浏览器：事件交给调用方给的回调（[`Config::events`]）。

mod link_auth;
mod link_store;
#[cfg(test)]
mod tests;

pub use link_auth::{hmac_sha256, proof};

use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::{watch, Notify, OwnedSemaphorePermit, Semaphore};
use tokio_tungstenite::tungstenite::{self, protocol::Role, Message};
use ts_rs::TS;

use crate::browser_probe::AssetSource;
use crate::error::{GateError, Result};
use crate::gate::{self, judge::Judgement, GateState};
use crate::usecase::browser_station;
use link_auth::{
    browser_label, extension_id, is_hex, pairing_id, short_text, token, token_matches,
};
use link_store::Record;

/// 线协议版本。插件与桥对不上时，桥对旧路径回 410（见设计说明「兼容策略」）。
pub const PROTOCOL: u32 = 2;
/// 同时保留的配对上限（一个浏览器资料一条）。
pub const MAX_PAIRINGS: usize = 8;
/// 认领码多久失效。
pub const CLAIM_TTL: Duration = Duration::from_secs(300);
/// 插件拿到一条「可以放行」之后，没再收到新状态的话最多信它多久。
pub const ALLOW_TTL: Duration = Duration::from_secs(10);

/// 用过 / 过期的认领码再留多久（让认领页面还能显示「已连接」「已失效」）。
const CLAIM_KEEP: Duration = Duration::from_secs(600);
const MAX_CLAIMS: usize = 4;
const MAX_TCP: usize = 32;
/// 保护通道（含还没握完手的）的上限。认证过的连接每条配对最多一条。
const MAX_SOCKETS: usize = 16;
const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_LINK_BODY: usize = 4096;
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(12);
const HEARTBEAT: Duration = Duration::from_secs(2);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(15);
const STALE_AFTER: Duration = Duration::from_secs(20);
/// 正常停止时给每条连接多久把下线消息发出去。
const FAREWELL: Duration = Duration::from_millis(1500);
const IDLE_INTERVAL: Duration = Duration::from_secs(5);
// Account verification can make four sequential requests with a 25-second
// per-request timeout. The bridge deadline must allow that bounded workflow.
const ACCOUNT_TIMEOUT: Duration = Duration::from_secs(105);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(110);
const DISCONNECTED: &str = "插件连接已断开，请重新连接";

const PAGE_HTML: &str = include_str!("page/link.html");
const PAGE_JS: &str = include_str!("page/link.js");
const PAGE_CSS: &str = include_str!("page/link.css");
// 颜色只在面板的 tokens.css 里定义：认领页面原样带上那一份，自己不另起色值。
const TOKENS_CSS: &str = include_str!("../../../../src/styles/tokens.css");
const PAGE_CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

// ------------------------------------------------------------------ 对外的形状

/// 「连接浏览器」「一键重连」的结果。
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct BrowserLinkStart {
    /// 在浏览器里打开的认领页地址（`#c=` 后面是一次性认领码）。只给面板界面，不进日志。
    pub url: Option<String>,
    /// 面板有没有替你打开默认浏览器（由命令层填）。
    pub opened: bool,
    pub port: u16,
    /// 认领码失效的 Unix 秒；没有认领码是 0。
    #[ts(type = "number")]
    pub expires_at: u64,
    /// 一键重连时拿回了原来的端口：已配对的插件会自己连上来。
    pub rebound: bool,
}

/// 放行 / 拦截的原因代号。插件按它决定提示语之外的行为，**只认 `allowed` 为放行**。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProtectionCode {
    /// 插件没开保护（或已暂停）。
    NotArmed,
    /// 正在核验出口。
    Checking,
    /// 可以放行。
    Allowed,
    /// 出口 IP 跟开启（或上次放行）时不一样，白名单内换 IP 也算。要使用者重新开启才放行。
    IpChanged,
    IpUnknown,
    IpNotAllowed,
    CountryNotAllowed,
    CountryUnknown,
    CountryConflict,
    ProbeTimeout,
    /// IP 锁被手动关闭。
    ManualPaused,
    /// IP 锁或网络设置变了（门禁代次变化）。
    ConfigChanged,
    /// 出口检测太久没更新。
    Stale,
    /// 插件没连着。
    Disconnected,
    /// 面板这边的联动服务停了。
    PanelStopping,
}

/// 一个浏览器（一条连接）此刻的保护状态。
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct BrowserProtection {
    /// 现在可以放行受保护的网站。只有这一位是 `true` 才放行。
    pub allow: bool,
    pub code: ProtectionCode,
    pub reason: String,
    /// 插件开着保护（含已触发、等使用者重新开启的状态）。
    pub armed: bool,
    pub checking: bool,
    /// 最近一次核实的出口。
    pub ip: Option<String>,
    /// 开启（或上次放行）时的出口。
    pub baseline: Option<String>,
    #[ts(type = "number")]
    pub checked_at: u64,
}

/// 一条配对（一个浏览器资料里的插件）。
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct BrowserLinkPairing {
    pub pairing_id: String,
    /// 「Chrome 131」这类，按配对时的 User-Agent 起的名字。
    pub label: String,
    pub extension_id: String,
    pub extension_version: String,
    #[ts(type = "number")]
    pub paired_at: u64,
    pub connected: bool,
    /// 没连着，而且它记得的端口不是现在这个：要点「一键重连」。
    pub needs_relink: bool,
    pub protection: BrowserProtection,
    /// 这个浏览器最近交回的 WebRTC / 代理设置（只有设置名，`Enhancement` 的 JSON）。
    pub enhancement_json: Option<String>,
}

/// 最近一次 IP 判定（任一浏览器触发的那次）。
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct BrowserLinkCheck {
    pub ip: Option<String>,
    pub allowed: bool,
    pub code: ProtectionCode,
    pub reason: String,
    #[ts(type = "number")]
    pub checked_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BrowserLinkClaimState {
    Pending,
    Claimed,
    Expired,
}

/// 最近一次「连接浏览器」发出去的认领码怎么样了。
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct BrowserLinkClaim {
    pub state: BrowserLinkClaimState,
    #[ts(type = "number")]
    pub expires_at: u64,
    /// 认领它的浏览器。
    pub browser: Option<String>,
    /// 插件试过、没成功的原因（例如配对满了）。
    pub problem: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, TS)]
#[ts(export)]
pub struct BrowserLinkStatus {
    /// 在本机回环上监听着。没配对过时永远是 `false`。
    pub listening: bool,
    pub port: Option<u16>,
    /// 有已配对的浏览器还在找旧端口：界面给「一键重连」。
    pub relink_needed: bool,
    pub pairings: Vec<BrowserLinkPairing>,
    pub last_check: Option<BrowserLinkCheck>,
    pub claim: Option<BrowserLinkClaim>,
    /// 开不起来、记录读不懂之类，一句能照着做的话。
    pub problem: Option<String>,
}

/// 一条连接为什么结束（正常结束才有；异常断开没有）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    /// 面板正常退出。
    PanelExit,
    /// 联动服务换端口重启（一键重连拿回了原来的端口）。
    Restarting,
    /// 面板里解除了这条配对。
    Revoked,
    /// 同一条配对又建了一条新连接（插件后台重启之类），旧的让位。
    Replaced,
}

impl Ending {
    fn message(self) -> Value {
        match self {
            Ending::PanelExit => json!({"type": "offline", "protocol": PROTOCOL,
                "code": "panel_exit", "reason": "QB Gate 面板已退出"}),
            Ending::Restarting => json!({"type": "offline", "protocol": PROTOCOL,
                "code": "restarting", "reason": "QB Gate 正在重启浏览器联动"}),
            Ending::Revoked => json!({"type": "revoked", "protocol": PROTOCOL,
                "reason": "已在 QB Gate 面板里解除这个浏览器的配对"}),
            Ending::Replaced => json!({"type": "replaced", "protocol": PROTOCOL,
                "reason": "同一个插件建立了新的连接"}),
        }
    }
}

// ------------------------------------------------------------------ 配置

pub type Authority = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Judgement> + Send>> + Send + Sync>;
pub type Interval = Arc<dyn Fn() -> Duration + Send + Sync>;
pub type Events = Arc<dyn Fn(&str) + Send + Sync>;

/// 「查套路免登录」「公开价格」那几条路由读写什么。
#[derive(Clone)]
pub enum Stations {
    /// 面板：真实的中转站数据（读库、加密保存账单登录、进查价缓存）。
    Repository,
    /// 单测与开发桥：**不碰任何数据库**。站点列表就是这一份；账户导入只核对格式与同源、不验证不保存；
    /// 公开价格只解析与换算。
    Fixture(Arc<Vec<browser_station::BrowserStation>>),
}

#[derive(Clone)]
pub struct Config {
    /// `browser-link.json` 放在哪。正式面板是数据目录；单测与开发工具给临时目录。
    pub dir: PathBuf,
    pub gate: Arc<GateState>,
    /// IP 判定。正式面板是 `gate::judge_now`。
    pub authority: Authority,
    pub assets: Arc<dyn AssetSource>,
    /// 有开着的保护时多久复查一次出口。
    pub interval: Interval,
    /// 定期复查要不要跑（单测关掉，手动调 `monitor_check`）。
    pub monitor: bool,
    /// 配对、解除、换端口这类事件的一句话（不含任何口令、认领码）。
    pub events: Option<Events>,
    pub stations: Stations,
}

impl Config {
    pub fn production(gate: Arc<GateState>, assets: Arc<dyn AssetSource>, events: Events) -> Self {
        Self {
            dir: crate::paths::state_dir(),
            gate,
            authority: Arc::new(|| Box::pin(gate::judge_now())),
            assets,
            interval: Arc::new(|| crate::settings::gate_check_interval().min(IDLE_INTERVAL)),
            monitor: true,
            events: Some(events),
            stations: Stations::Repository,
        }
    }
}

static PRODUCTION: OnceLock<Link> = OnceLock::new();

/// 面板用的那一份。第一次调用时定下配置，之后的参数被忽略。
pub fn production(
    gate: Arc<GateState>,
    assets: Arc<dyn AssetSource>,
    events: Events,
) -> &'static Link {
    PRODUCTION.get_or_init(|| Link::new(Config::production(gate, assets, events)))
}

/// 面板退出时用：没建过就什么都不做。
pub fn production_if_created() -> Option<&'static Link> {
    PRODUCTION.get()
}

fn now_unix() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

// ------------------------------------------------------------------ 内部状态

#[derive(Clone, Debug)]
struct Protection {
    armed: bool,
    safe: bool,
    checking: bool,
    code: ProtectionCode,
    reason: String,
    ip: Option<String>,
    baseline: Option<String>,
    generation: u64,
    checked: Option<Instant>,
    checked_at: u64,
    revision: u64,
}

impl Protection {
    fn new(reason: &str) -> Self {
        Self {
            armed: false,
            safe: false,
            checking: false,
            code: ProtectionCode::NotArmed,
            reason: reason.into(),
            ip: None,
            baseline: None,
            generation: 0,
            checked: None,
            checked_at: 0,
            revision: 0,
        }
    }

    fn unconnected() -> Self {
        let mut p = Self::new("插件未连接");
        p.code = ProtectionCode::Disconnected;
        p
    }

    fn fail(&mut self, code: ProtectionCode, reason: &str) {
        self.revision = self.revision.wrapping_add(1);
        self.safe = false;
        self.code = code;
        self.reason = reason.into();
    }

    fn allow(&self) -> bool {
        self.armed && self.safe && !self.checking && self.code == ProtectionCode::Allowed
    }

    fn view(&self) -> BrowserProtection {
        BrowserProtection {
            allow: self.allow(),
            code: self.code,
            reason: self.reason.clone(),
            armed: self.armed,
            checking: self.checking,
            ip: self.ip.clone(),
            baseline: self.baseline.clone(),
            checked_at: self.checked_at,
        }
    }
}

struct Conn {
    pairing_id: String,
    origin: String,
    session: String,
    protection: Protection,
    seq: u64,
    wake: Arc<Notify>,
    ending: Option<Ending>,
}

/// 状态变了：序号加一，叫醒那条连接把新状态推出去。
fn touch(conn: &mut Conn) {
    conn.seq = conn.seq.wrapping_add(1);
    conn.wake.notify_one();
}

#[derive(Default)]
struct Runtime {
    /// 连接断开时的最后状态（界面上照样显示开着、不放行）。
    last: Option<Protection>,
    enhancement_json: Option<String>,
}

struct Claim {
    serial: u64,
    code: String,
    expires: Instant,
    expires_at: u64,
    used: Option<String>,
    problem: Option<String>,
}

impl Claim {
    fn expired(&self) -> bool {
        Instant::now() >= self.expires
    }
    fn forgotten(&self) -> bool {
        Instant::now() >= self.expires + CLAIM_KEEP
    }
}

/// 常量时间地找：每一个都比一遍，不在第一个对上时早退。
fn find_claim(claims: &[Claim], code: &str) -> Option<usize> {
    let mut found = None;
    for (i, claim) in claims.iter().enumerate() {
        if token_matches(&claim.code, code) {
            found = Some(i);
        }
    }
    found
}

#[derive(Default)]
struct Inner {
    records: Vec<Record>,
    /// 插件们最近一次被告知的端口；下次启动先试它。
    preferred: u16,
    claims: Vec<Claim>,
    next_claim: u64,
    conns: HashMap<u64, Conn>,
    next_conn: u64,
    runtime: HashMap<String, Runtime>,
    last_check: Option<BrowserLinkCheck>,
    probe_started: Option<Instant>,
}

struct Shared {
    port: u16,
    config: Config,
    inner: Mutex<Inner>,
    running: AtomicBool,
    check_lock: tokio::sync::Mutex<()>,
    recheck: Notify,
    shutdown: watch::Sender<bool>,
    tcp: Arc<Semaphore>,
    sockets: Arc<Semaphore>,
    socket_tasks: Mutex<tokio::task::JoinSet<()>>,
    gone: Notify,
}

struct Outcome {
    ip: Option<String>,
    safe: bool,
    code: ProtectionCode,
    reason: String,
}

fn judgement_code(verdict: &Judgement) -> ProtectionCode {
    match verdict {
        Judgement::Allowed { .. } => ProtectionCode::Allowed,
        Judgement::IpUnknown => ProtectionCode::IpUnknown,
        Judgement::IpNotAllowed { .. } => ProtectionCode::IpNotAllowed,
        Judgement::CountryNotAllowed { .. } => ProtectionCode::CountryNotAllowed,
        Judgement::CountryUnknown { .. } => ProtectionCode::CountryUnknown,
        Judgement::CountryConflict { .. } => ProtectionCode::CountryConflict,
    }
}

/// 两个地址是不是同一个（IPv6 的不同写法算同一个）。
fn same_ip(a: &str, b: &str) -> bool {
    match (a.parse::<std::net::IpAddr>(), b.parse::<std::net::IpAddr>()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn verdict_result(verdict: &Judgement, baseline: Option<&str>) -> Outcome {
    match verdict {
        Judgement::Allowed { ip } if baseline.is_some_and(|old| !same_ip(old, ip)) => Outcome {
            ip: Some(ip.clone()),
            safe: false,
            code: ProtectionCode::IpChanged,
            reason: "出口 IP 已变化，浏览器保护已触发".into(),
        },
        Judgement::Allowed { ip } => Outcome {
            ip: Some(ip.clone()),
            safe: true,
            code: ProtectionCode::Allowed,
            reason: "出口 IP 已核实，可以放行".into(),
        },
        other => Outcome {
            ip: None,
            safe: false,
            code: judgement_code(other),
            reason: other.reason(),
        },
    }
}

fn timeout_outcome() -> Outcome {
    Outcome {
        ip: None,
        safe: false,
        code: ProtectionCode::ProbeTimeout,
        reason: "出口 IP 检测超时".into(),
    }
}

fn check_view(verdict: &Judgement) -> BrowserLinkCheck {
    let ip = match verdict {
        Judgement::IpUnknown => None,
        Judgement::Allowed { ip }
        | Judgement::IpNotAllowed { ip }
        | Judgement::CountryNotAllowed { ip, .. }
        | Judgement::CountryUnknown { ip }
        | Judgement::CountryConflict { ip, .. } => Some(ip.clone()),
    };
    BrowserLinkCheck {
        ip,
        allowed: verdict.is_allowed(),
        code: judgement_code(verdict),
        reason: verdict.reason(),
        checked_at: now_unix(),
    }
}

impl Shared {
    fn emit(&self, message: &str) {
        if let Some(events) = &self.config.events {
            events(message);
        }
    }

    /// 放行中的状态遇到停服、手动关门、代次变化、检测过期时锁存为失败。读一次不会变回放行。
    fn refresh(&self, p: &mut Protection) -> bool {
        if !(p.armed && p.safe) {
            return false;
        }
        let (code, reason) = if !self.running.load(Ordering::SeqCst) {
            (ProtectionCode::PanelStopping, "面板联动服务已停止")
        } else if self.config.gate.manual_paused.load(Ordering::SeqCst) {
            (ProtectionCode::ManualPaused, "IP 锁已手动关闭")
        } else if self.config.gate.generation.load(Ordering::SeqCst) != p.generation {
            (
                ProtectionCode::ConfigChanged,
                "IP 锁或网络设置发生变化，请重新开启保护",
            )
        } else if p.checked.is_none_or(|t| t.elapsed() > STALE_AFTER) {
            (ProtectionCode::Stale, "出口检测已过期")
        } else {
            return false;
        };
        p.fail(code, reason);
        true
    }

    /// 给插件的一条状态。调用方拿着 `inner` 锁。
    fn status_value(&self, conn: &mut Conn) -> Value {
        if self.refresh(&mut conn.protection) {
            touch(conn);
        }
        let p = &conn.protection;
        json!({
            "type": "status",
            "protocol": PROTOCOL,
            "seq": conn.seq,
            "allow": p.allow(),
            "code": p.code,
            "reason": p.reason,
            "armed": p.armed,
            "checking": p.checking,
            "ip": p.ip,
            "baseline": p.baseline,
            "checked_at": p.checked_at,
            "ttl_ms": ALLOW_TTL.as_millis() as u64,
        })
    }

    fn conn_status(&self, id: u64) -> Option<Value> {
        let mut inner = self.inner.lock().unwrap();
        let conn = inner.conns.get_mut(&id).filter(|c| c.ending.is_none())?;
        Some(self.status_value(conn))
    }

    fn any_armed(&self) -> bool {
        self.inner
            .lock()
            .unwrap()
            .conns
            .values()
            .any(|c| c.protection.armed && c.protection.safe)
    }

    fn issue_claim(&self) -> BrowserLinkStart {
        let code = token();
        let mut inner = self.inner.lock().unwrap();
        inner.claims.retain(|c| !c.forgotten());
        while inner.claims.len() >= MAX_CLAIMS {
            let oldest = inner
                .claims
                .iter()
                .enumerate()
                .min_by_key(|(_, c)| c.serial)
                .map(|(i, _)| i)
                .unwrap_or(0);
            inner.claims.remove(oldest);
        }
        let expires_at = now_unix() + CLAIM_TTL.as_secs();
        inner.next_claim += 1;
        let serial = inner.next_claim;
        inner.claims.push(Claim {
            serial,
            code: code.clone(),
            expires: Instant::now() + CLAIM_TTL,
            expires_at,
            used: None,
            problem: None,
        });
        BrowserLinkStart {
            url: Some(format!("http://127.0.0.1:{}/link#c={code}", self.port)),
            opened: false,
            port: self.port,
            expires_at,
            rebound: false,
        }
    }

    fn connected_any(&self) -> bool {
        self.inner
            .lock()
            .unwrap()
            .conns
            .values()
            .any(|c| c.ending.is_none())
    }

    /// 解除一条配对：先落盘，再让它的连接收到 `revoked` 断开。返回剩下几条。
    fn revoke(&self, pairing: &str) -> Result<usize> {
        let mut inner = self.inner.lock().unwrap();
        let index = inner
            .records
            .iter()
            .position(|r| r.pairing_id == pairing)
            .ok_or_else(|| GateError::Other("这个浏览器的配对已经不在了".into()))?;
        let mut records = inner.records.clone();
        let removed = records.remove(index);
        link_store::save(&self.config.dir, inner.preferred, &records)?;
        inner.records = records;
        for conn in inner.conns.values_mut() {
            if conn.pairing_id == pairing {
                conn.ending.get_or_insert(Ending::Revoked);
                conn.wake.notify_one();
            }
        }
        inner.runtime.remove(pairing);
        let left = inner.records.len();
        drop(inner);
        self.emit(&format!(
            "浏览器助手：已解除「{}」的配对（还剩 {left} 个）",
            removed.label
        ));
        Ok(left)
    }

    fn status(&self, problem: Option<String>) -> BrowserLinkStatus {
        let mut inner = self.inner.lock().unwrap();
        inner.claims.retain(|c| !c.forgotten());
        for conn in inner.conns.values_mut() {
            if conn.ending.is_none() && self.refresh(&mut conn.protection) {
                touch(conn);
            }
        }
        let pairings: Vec<_> = inner
            .records
            .iter()
            .map(|r| pairing_view(&inner, r, Some(self.port)))
            .collect();
        let claim = inner
            .claims
            .iter()
            .max_by_key(|c| c.serial)
            .map(|c| BrowserLinkClaim {
                state: if c.used.is_some() {
                    BrowserLinkClaimState::Claimed
                } else if c.expired() {
                    BrowserLinkClaimState::Expired
                } else {
                    BrowserLinkClaimState::Pending
                },
                expires_at: c.expires_at,
                browser: c.used.clone(),
                problem: c.problem.clone(),
            });
        BrowserLinkStatus {
            listening: self.running.load(Ordering::SeqCst),
            port: Some(self.port),
            relink_needed: pairings.iter().any(|p| p.needs_relink),
            pairings,
            last_check: inner.last_check.clone(),
            claim,
            problem,
        }
    }
}

fn pairing_view(inner: &Inner, record: &Record, port: Option<u16>) -> BrowserLinkPairing {
    let conn = inner
        .conns
        .iter()
        .filter(|(_, c)| c.ending.is_none() && c.pairing_id == record.pairing_id)
        .max_by_key(|(id, _)| **id)
        .map(|(_, c)| c);
    let runtime = inner.runtime.get(&record.pairing_id);
    let protection = match conn {
        Some(c) => c.protection.view(),
        None => runtime
            .and_then(|r| r.last.as_ref())
            .map(Protection::view)
            .unwrap_or_else(|| Protection::unconnected().view()),
    };
    BrowserLinkPairing {
        pairing_id: record.pairing_id.clone(),
        label: record.label.clone(),
        extension_id: record.extension_id.clone(),
        extension_version: record.extension_version.clone(),
        paired_at: record.paired_at,
        connected: conn.is_some(),
        needs_relink: port.is_some_and(|p| conn.is_none() && record.port != p),
        protection,
        enhancement_json: runtime.and_then(|r| r.enhancement_json.clone()),
    }
}

// ------------------------------------------------------------------ 启停

struct Instance {
    shared: Arc<Shared>,
    task: tokio::task::JoinHandle<()>,
}

async fn bind(preferred: u16) -> std::io::Result<TcpListener> {
    if preferred != 0 {
        if let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, preferred)).await {
            return Ok(listener);
        }
    }
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await
}

fn launch(config: &Config, loaded: link_store::Loaded, listener: TcpListener) -> Result<Instance> {
    let port = listener.local_addr()?.port();
    let (shutdown, _) = watch::channel(false);
    let shared = Arc::new(Shared {
        port,
        config: config.clone(),
        inner: Mutex::new(Inner {
            records: loaded.records,
            preferred: loaded.port,
            ..Default::default()
        }),
        running: AtomicBool::new(true),
        check_lock: tokio::sync::Mutex::new(()),
        recheck: Notify::new(),
        shutdown,
        tcp: Arc::new(Semaphore::new(MAX_TCP)),
        sockets: Arc::new(Semaphore::new(MAX_SOCKETS)),
        socket_tasks: Mutex::new(tokio::task::JoinSet::new()),
        gone: Notify::new(),
    });
    let worker = shared.clone();
    let monitor = config.monitor;
    let task = tokio::spawn(async move { serve(listener, worker, monitor).await });
    Ok(Instance { shared, task })
}

async fn shutdown(instance: Instance, ending: Ending) {
    let shared = instance.shared;
    shared.running.store(false, Ordering::SeqCst);
    {
        let mut inner = shared.inner.lock().unwrap();
        for conn in inner.conns.values_mut() {
            conn.ending.get_or_insert(ending);
            conn.wake.notify_one();
        }
    }
    // 先让每条连接把下线消息发出去、自己断开，再关监听。
    let _ = tokio::time::timeout(FAREWELL, async {
        loop {
            let gone = shared.gone.notified();
            let empty = shared.inner.lock().unwrap().conns.is_empty();
            if empty {
                break;
            }
            gone.await;
        }
    })
    .await;
    shared.shutdown.send_replace(true);
    let _ = instance.task.await;
    let mut tasks = std::mem::take(&mut *shared.socket_tasks.lock().unwrap());
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
}

/// 面板这一侧的浏览器联动：一个进程一份（面板用 [`production`]，单测与开发工具各建各的）。
pub struct Link {
    config: Config,
    slot: tokio::sync::Mutex<Option<Instance>>,
    epoch: AtomicU64,
    problem: Mutex<Option<String>>,
}

impl Link {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            slot: tokio::sync::Mutex::new(None),
            epoch: AtomicU64::new(0),
            problem: Mutex::new(None),
        }
    }

    fn note(&self, problem: Option<String>) {
        *self.problem.lock().unwrap() = problem;
    }

    async fn start(
        &self,
        slot: &mut Option<Instance>,
        loaded: link_store::Loaded,
        listener: Option<TcpListener>,
    ) -> Result<()> {
        let preferred = loaded.port;
        let dropped = loaded.dropped;
        let listener = match listener {
            Some(listener) => listener,
            None => bind(preferred)
                .await
                .map_err(|e| GateError::Other(format!("浏览器助手开不了本机端口：{e}")))?,
        };
        let instance = launch(&self.config, loaded, listener)?;
        let port = instance.shared.port;
        instance
            .shared
            .emit(&format!("浏览器助手：在 127.0.0.1:{port} 上等插件连接"));
        if preferred != 0 && port != preferred {
            instance.shared.emit(&format!(
                "浏览器助手：原来的端口 {preferred} 被占用，改用 {port}；已配对的浏览器要在面板点「一键重连」"
            ));
        }
        *slot = Some(instance);
        self.note((dropped > 0).then(|| {
            format!("有 {dropped} 条配对记录解不开（多半换过 Windows 用户或电脑），已忽略；那些浏览器需要重新连接")
        }));
        Ok(())
    }

    fn snapshot(&self, slot: &Option<Instance>) -> BrowserLinkStatus {
        let problem = self.problem.lock().unwrap().clone();
        match slot {
            Some(instance) => instance.shared.status(problem),
            None => match link_store::load(&self.config.dir) {
                Ok(loaded) => {
                    let inner = Inner::default();
                    BrowserLinkStatus {
                        pairings: loaded
                            .records
                            .iter()
                            .map(|r| pairing_view(&inner, r, None))
                            .collect(),
                        problem,
                        ..Default::default()
                    }
                }
                Err(e) => BrowserLinkStatus {
                    problem: Some(problem.unwrap_or_else(|| {
                        format!("{e}；点「连接浏览器」会把它挪到一边、重新开始")
                    })),
                    ..Default::default()
                },
            },
        }
    }

    pub async fn status(&self) -> BrowserLinkStatus {
        let slot = self.slot.lock().await;
        self.snapshot(&slot)
    }

    /// 面板启动时调：有配对才监听；没配对过什么都不做。
    pub async fn resume(&self) -> BrowserLinkStatus {
        let epoch = self.epoch.load(Ordering::SeqCst);
        let mut slot = self.slot.lock().await;
        if self.epoch.load(Ordering::SeqCst) == epoch && slot.is_none() {
            match link_store::load(&self.config.dir) {
                Ok(loaded) if loaded.records.is_empty() => {}
                Ok(loaded) => {
                    if let Err(e) = self.start(&mut slot, loaded, None).await {
                        self.note(Some(e.to_string()));
                    }
                }
                Err(e) => self.note(Some(format!(
                    "{e}；点「连接浏览器」会把它挪到一边、重新开始"
                ))),
            }
        }
        self.snapshot(&slot)
    }

    /// 「连接浏览器」：没在监听就开起来，发一个一次性认领码。打开浏览器是调用方的事。
    pub async fn connect(&self) -> Result<BrowserLinkStart> {
        let epoch = self.epoch.load(Ordering::SeqCst);
        let mut slot = self.slot.lock().await;
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return Err(GateError::Other(
                "浏览器联动刚被停止，这次连接已取消".into(),
            ));
        }
        if slot.is_none() {
            let loaded = match link_store::load(&self.config.dir) {
                Ok(loaded) => loaded,
                Err(_) => {
                    let aside = link_store::quarantine(&self.config.dir)?;
                    self.note(Some(format!(
                        "原来的配对记录读不懂，已挪到 {}；所有浏览器需要重新连接",
                        aside.display()
                    )));
                    link_store::Loaded::default()
                }
            };
            self.start(&mut slot, loaded, None).await?;
        }
        Ok(slot.as_ref().expect("刚开起来").shared.issue_claim())
    }

    /// 「一键重连」：端口换过、此刻又没有浏览器连着，就先试着拿回原来那个端口；
    /// 然后照样发一个认领码（打开的页面会叫醒插件，换过端口的插件也靠它学到新端口）。
    pub async fn reconnect(&self) -> Result<BrowserLinkStart> {
        let epoch = self.epoch.load(Ordering::SeqCst);
        let mut slot = self.slot.lock().await;
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return Err(GateError::Other(
                "浏览器联动刚被停止，这次重连已取消".into(),
            ));
        }
        let mut rebound = false;
        match slot.as_ref() {
            None => {
                let loaded = link_store::load(&self.config.dir)?;
                let preferred = loaded.port;
                self.start(&mut slot, loaded, None).await?;
                rebound =
                    preferred != 0 && slot.as_ref().is_some_and(|i| i.shared.port == preferred);
            }
            Some(instance) => {
                let preferred = instance.shared.inner.lock().unwrap().preferred;
                if preferred != 0
                    && preferred != instance.shared.port
                    && !instance.shared.connected_any()
                {
                    if let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, preferred)).await
                    {
                        let old = slot.take().expect("上面看过");
                        shutdown(old, Ending::Restarting).await;
                        let loaded = link_store::load(&self.config.dir)?;
                        self.start(&mut slot, loaded, Some(listener)).await?;
                        rebound = true;
                    }
                }
            }
        }
        let mut start = slot.as_ref().expect("开着").shared.issue_claim();
        start.rebound = rebound;
        Ok(start)
    }

    /// 解除一条配对。连着的那个浏览器立刻收到 `revoked` 断开；最后一条解除就停止监听。
    pub async fn unpair(&self, pairing: &str) -> Result<BrowserLinkStatus> {
        let mut slot = self.slot.lock().await;
        match slot.as_ref() {
            Some(instance) => {
                if instance.shared.revoke(pairing)? == 0 {
                    self.epoch.fetch_add(1, Ordering::SeqCst);
                    let instance = slot.take().expect("上面看过");
                    instance
                        .shared
                        .emit("浏览器助手：没有已配对的浏览器了，停止监听");
                    shutdown(instance, Ending::Revoked).await;
                }
            }
            None => {
                let loaded = link_store::load(&self.config.dir)?;
                let mut records = loaded.records;
                let before = records.len();
                records.retain(|r| r.pairing_id != pairing);
                if records.len() == before {
                    return Err(GateError::Other("这个浏览器的配对已经不在了".into()));
                }
                link_store::save(&self.config.dir, loaded.port, &records)?;
            }
        }
        Ok(self.snapshot(&slot))
    }

    /// 停止监听（面板退出时是 [`Ending::PanelExit`]）。之前排着队的启动不会再把它开起来。
    pub async fn stop(&self, ending: Ending) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let mut slot = self.slot.lock().await;
        if let Some(instance) = slot.take() {
            shutdown(instance, ending).await;
        }
    }
}

// ------------------------------------------------------------------ 监听与复查

async fn serve(listener: TcpListener, shared: Arc<Shared>, monitor_enabled: bool) {
    let mut stop = shared.shutdown.subscribe();
    let monitor = monitor_enabled.then(|| tokio::spawn(monitor(shared.clone())));
    let mut requests = tokio::task::JoinSet::new();
    loop {
        if *stop.borrow() {
            break;
        }
        let accepted = tokio::select! {
            _ = stop.changed() => break,
            _ = requests.join_next(), if !requests.is_empty() => continue,
            accepted = listener.accept() => accepted,
        };
        let Ok((stream, _)) = accepted else {
            // Windows 的 accept 偶尔回一下连接被重置之类；别为此停掉整个联动。
            tokio::time::sleep(Duration::from_millis(20)).await;
            continue;
        };
        let Ok(permit) = shared.tcp.clone().try_acquire_owned() else {
            continue;
        };
        let state = shared.clone();
        let mut shutdown = shared.shutdown.subscribe();
        requests.spawn(async move {
            let _permit = permit;
            let service = service_fn(move |request| handle(request, state.clone()));
            let connection = hyper::server::conn::http1::Builder::new()
                // Disabling keep-alive here rewrites even a 101 response's
                // Connection: Upgrade to close, making WebSocket pairing fail.
                // Ordinary HTTP responses request close in response() instead.
                .keep_alive(true)
                .max_buf_size(32 * 1024)
                .serve_connection(TokioIo::new(stream), service)
                .with_upgrades();
            tokio::select! {
                biased;
                _ = shutdown.changed() => {},
                _ = tokio::time::timeout(CONNECTION_TIMEOUT, connection) => {},
            }
        });
    }
    // Waiting for cancellation matters: a synchronous credential commit already
    // in progress must finish before stop returns; no detached request may write later.
    requests.abort_all();
    while requests.join_next().await.is_some() {}
    if let Some(monitor) = monitor {
        monitor.abort();
        let _ = monitor.await;
    }
}

async fn monitor(shared: Arc<Shared>) {
    let mut shutdown = shared.shutdown.subscribe();
    loop {
        if *shutdown.borrow() {
            break;
        }
        let interval = if shared.any_armed() {
            (shared.config.interval)()
        } else {
            IDLE_INTERVAL
        };
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = tokio::time::sleep(interval) => {},
            _ = shared.recheck.notified() => {},
        }
        monitor_check(&shared).await;
    }
}

/// 复查一轮：所有放行中的连接共用一次探测，各按自己的开启时出口比。
async fn monitor_check(shared: &Shared) {
    let targets: Vec<(u64, u64)> = {
        let mut inner = shared.inner.lock().unwrap();
        let mut out = Vec::new();
        for (id, conn) in inner.conns.iter_mut() {
            if conn.ending.is_some() {
                continue;
            }
            if shared.refresh(&mut conn.protection) {
                touch(conn);
            }
            if conn.protection.armed && conn.protection.safe {
                out.push((*id, conn.protection.revision));
            }
        }
        out
    };
    if targets.is_empty() {
        return;
    }
    let _serial = shared.check_lock.lock().await;
    if !shared.running.load(Ordering::SeqCst) {
        return;
    }
    let generation = shared.config.gate.generation.load(Ordering::SeqCst);
    let paused = shared.config.gate.manual_paused.load(Ordering::SeqCst);
    let verdict = if paused {
        None
    } else {
        Some(tokio::time::timeout(PROBE_TIMEOUT, (shared.config.authority)()).await)
    };
    let now_paused = shared.config.gate.manual_paused.load(Ordering::SeqCst);
    let now_generation = shared.config.gate.generation.load(Ordering::SeqCst);
    let mut inner = shared.inner.lock().unwrap();
    if let Some(Ok(v)) = &verdict {
        inner.last_check = Some(check_view(v));
    }
    for (id, revision) in targets {
        let Some(conn) = inner.conns.get_mut(&id) else {
            continue;
        };
        let p = &mut conn.protection;
        if conn.ending.is_some() || p.revision != revision || !p.armed || !p.safe {
            continue;
        }
        if paused || now_paused {
            p.fail(ProtectionCode::ManualPaused, "IP 锁已手动关闭");
        } else if generation != now_generation || generation != p.generation {
            p.fail(
                ProtectionCode::ConfigChanged,
                "检测期间 IP 锁或网络设置发生变化",
            );
        } else {
            let outcome = match &verdict {
                Some(Ok(v)) => verdict_result(v, p.baseline.as_deref()),
                _ => timeout_outcome(),
            };
            p.checked_at = now_unix();
            p.checked = Some(Instant::now());
            p.ip = outcome.ip;
            p.code = outcome.code;
            p.reason = outcome.reason;
            if outcome.safe {
                p.safe = true;
            } else {
                p.revision = p.revision.wrapping_add(1);
                p.safe = false;
            }
        }
        touch(conn);
    }
}

/// 开启（或恢复）保护。`baseline` 是插件记得的上次放行时的出口：跟现在不一样就判「出口已变化」。
async fn arm(
    shared: &Shared,
    id: u64,
    baseline: Option<String>,
) -> std::result::Result<Value, String> {
    // 等锁之前先记下来：排在探测后面的开启，不能把排在它之后的停止盖掉。
    let requested = {
        let inner = shared.inner.lock().unwrap();
        inner
            .conns
            .get(&id)
            .filter(|c| c.ending.is_none())
            .map(|c| c.protection.revision)
            .ok_or(DISCONNECTED)?
    };
    let _serial = shared.check_lock.lock().await;
    let generation = {
        let mut inner = shared.inner.lock().unwrap();
        let conn = inner
            .conns
            .get_mut(&id)
            .filter(|c| c.ending.is_none())
            .ok_or(DISCONNECTED)?;
        if shared.refresh(&mut conn.protection) {
            touch(conn);
        }
        if conn.protection.revision != requested
            || conn.protection.armed
            || !shared.running.load(Ordering::SeqCst)
        {
            return Ok(shared.status_value(conn));
        }
        if shared.config.gate.manual_paused.load(Ordering::SeqCst) {
            conn.protection.fail(
                ProtectionCode::ManualPaused,
                "IP 锁已手动关闭，请先在面板重新放行",
            );
            touch(conn);
            return Ok(shared.status_value(conn));
        }
        conn.protection.checking = true;
        conn.protection.code = ProtectionCode::Checking;
        conn.protection.reason = "正在核验出口 IP".into();
        touch(conn);
        shared.config.gate.generation.load(Ordering::SeqCst)
    };
    let verdict = tokio::time::timeout(PROBE_TIMEOUT, (shared.config.authority)()).await;
    let mut inner = shared.inner.lock().unwrap();
    if let Ok(v) = &verdict {
        inner.last_check = Some(check_view(v));
    }
    let conn = inner.conns.get_mut(&id).ok_or(DISCONNECTED)?;
    conn.protection.checking = false;
    if conn.protection.revision != requested
        || conn.ending.is_some()
        || !shared.running.load(Ordering::SeqCst)
    {
        // 探测期间来了更新的停止 / 断开：这次结果作废。
        if conn.protection.code == ProtectionCode::Checking {
            conn.protection.code = ProtectionCode::NotArmed;
            conn.protection.reason = "浏览器保护未开启".into();
        }
        touch(conn);
        return Ok(shared.status_value(conn));
    }
    if generation != shared.config.gate.generation.load(Ordering::SeqCst)
        || shared.config.gate.manual_paused.load(Ordering::SeqCst)
    {
        conn.protection.fail(
            ProtectionCode::ConfigChanged,
            "检测期间 IP 锁或网络设置发生变化",
        );
        touch(conn);
        return Ok(shared.status_value(conn));
    }
    let outcome = match &verdict {
        Ok(v) => verdict_result(v, baseline.as_deref()),
        Err(_) => timeout_outcome(),
    };
    let p = &mut conn.protection;
    p.checked_at = now_unix();
    p.checked = Some(Instant::now());
    p.ip = outcome.ip.clone();
    p.safe = outcome.safe;
    p.code = outcome.code;
    p.reason = outcome.reason;
    if outcome.safe {
        p.armed = true;
        p.baseline = outcome.ip;
        p.generation = generation;
    } else if outcome.code == ProtectionCode::IpChanged {
        // 恢复时出口已经变了：保护照样算开着（已触发），使用者重新开启才换基准。
        p.armed = true;
        p.baseline = baseline;
        p.generation = generation;
    }
    touch(conn);
    Ok(shared.status_value(conn))
}

fn disarm(shared: &Shared, id: u64) -> std::result::Result<Value, String> {
    let mut inner = shared.inner.lock().unwrap();
    let conn = inner
        .conns
        .get_mut(&id)
        .filter(|c| c.ending.is_none())
        .ok_or(DISCONNECTED)?;
    let p = &mut conn.protection;
    p.revision = p.revision.wrapping_add(1);
    p.armed = false;
    p.safe = false;
    p.checking = false;
    p.baseline = None;
    p.code = ProtectionCode::NotArmed;
    p.reason = "已暂停浏览器保护".into();
    touch(conn);
    Ok(shared.status_value(conn))
}

// ------------------------------------------------------------------ HTTP

fn header<'a>(request: &'a Request<Incoming>, name: &str) -> Option<&'a str> {
    request.headers().get(name)?.to_str().ok()
}

fn response(code: StatusCode, value: Value, origin: Option<&str>) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(value.to_string())));
    *response.status_mut() = code;
    let headers = response.headers_mut();
    headers.insert(
        "content-type",
        "application/json; charset=utf-8".parse().unwrap(),
    );
    headers.insert("cache-control", "no-store".parse().unwrap());
    headers.insert("connection", "close".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("vary", "Origin".parse().unwrap());
    if let Some(value) = origin.and_then(|origin| origin.parse().ok()) {
        headers.insert("access-control-allow-origin", value);
    }
    response
}

fn error(
    code: StatusCode,
    message: &str,
    origin: Option<&str>,
    machine: Option<&str>,
) -> Response<Full<Bytes>> {
    let mut value = json!({"error": message, "protocol": PROTOCOL});
    if let Some(machine) = machine {
        value["code"] = json!(machine);
    }
    response(code, value, origin)
}

fn page(body: &'static str, content_type: &'static str) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from_static(body.as_bytes())));
    let headers = response.headers_mut();
    headers.insert("content-type", content_type.parse().unwrap());
    headers.insert("cache-control", "no-store".parse().unwrap());
    headers.insert("connection", "close".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("x-frame-options", "DENY".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    headers.insert("content-security-policy", PAGE_CSP.parse().unwrap());
    response
}

fn preflight(origin: &str) -> Response<Full<Bytes>> {
    let mut reply = response(StatusCode::NO_CONTENT, Value::Null, Some(origin));
    let headers = reply.headers_mut();
    headers.insert(
        "access-control-allow-methods",
        "GET, POST, OPTIONS".parse().unwrap(),
    );
    headers.insert(
        "access-control-allow-headers",
        "Authorization, Content-Type, X-QB-Extension"
            .parse()
            .unwrap(),
    );
    headers.insert(
        "access-control-allow-private-network",
        "true".parse().unwrap(),
    );
    headers.insert("access-control-max-age", "600".parse().unwrap());
    reply
}

async fn read_body(
    request: Request<Incoming>,
    limit: usize,
) -> std::result::Result<Value, (StatusCode, &'static str)> {
    match tokio::time::timeout(
        AUTH_TIMEOUT,
        Limited::new(request.into_body(), limit).collect(),
    )
    .await
    {
        Ok(Ok(body)) => serde_json::from_slice::<Value>(&body.to_bytes())
            .map_err(|_| (StatusCode::BAD_REQUEST, "请求不是 JSON")),
        _ => Err((StatusCode::PAYLOAD_TOO_LARGE, "请求过大或读取超时")),
    }
}

async fn handle(
    mut request: Request<Incoming>,
    shared: Arc<Shared>,
) -> std::result::Result<Response<Full<Bytes>>, Infallible> {
    if !shared.running.load(Ordering::SeqCst) {
        return Ok(error(
            StatusCode::GONE,
            "联动服务已停止",
            None,
            Some("stopped"),
        ));
    }
    if !crate::browser_probe::host_ok(header(&request, "host"), shared.port)
        || request.uri().scheme().is_some()
        || request.uri().query().is_some()
    {
        return Ok(error(
            StatusCode::MISDIRECTED_REQUEST,
            "invalid host",
            None,
            None,
        ));
    }
    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    match (&method, path.as_str()) {
        (&Method::GET, "/link") => return Ok(page(PAGE_HTML, "text/html; charset=utf-8")),
        (&Method::GET, "/link/app.js") => {
            return Ok(page(PAGE_JS, "text/javascript; charset=utf-8"))
        }
        (&Method::GET, "/link/app.css") => return Ok(page(PAGE_CSS, "text/css; charset=utf-8")),
        (&Method::GET, "/link/tokens.css") => {
            return Ok(page(TOKENS_CSS, "text/css; charset=utf-8"))
        }
        (&Method::POST, "/link/state") => return Ok(link_state(request, &shared).await),
        (&Method::GET, "/v2/events") => return Ok(upgrade(&mut request, shared).await),
        _ => {}
    }
    if path == "/v1" || path.starts_with("/v1/") {
        return Ok(error(
            StatusCode::GONE,
            "这个插件版本太旧，请换用 QB Gate 自带的新版插件",
            None,
            Some("protocol_outdated"),
        ));
    }
    if path != "/link/claim" && !path.starts_with("/v2/") {
        return Ok(error(StatusCode::NOT_FOUND, "not found", None, None));
    }
    let origin = header(&request, "origin").map(str::to_owned);
    if origin.as_deref().is_some_and(|o| extension_id(o).is_none()) {
        return Ok(error(
            StatusCode::FORBIDDEN,
            "只接受 QB Gate 插件的请求",
            None,
            Some("origin"),
        ));
    }
    if method == Method::OPTIONS {
        return Ok(match origin.as_deref() {
            Some(origin) => preflight(origin),
            None => error(StatusCode::FORBIDDEN, "缺少扩展来源", None, Some("origin")),
        });
    }
    if path == "/link/claim" {
        let Some(origin) = origin.filter(|_| method == Method::POST) else {
            return Ok(error(
                StatusCode::FORBIDDEN,
                "认领必须由插件发起",
                None,
                Some("origin"),
            ));
        };
        let user_agent = header(&request, "user-agent").map(str::to_owned);
        let body = match read_body(request, MAX_LINK_BODY).await {
            Ok(body) => body,
            Err((code, message)) => return Ok(error(code, message, Some(&origin), None)),
        };
        return Ok(match claim(&shared, &origin, user_agent.as_deref(), body) {
            Ok(value) => response(StatusCode::OK, value, Some(&origin)),
            Err((code, message, machine)) => error(code, &message, Some(&origin), Some(machine)),
        });
    }
    let (id, origin) = match authenticate(&request, &shared, origin.as_deref()) {
        Ok(found) => found,
        Err(reply) => return Ok(*reply),
    };
    let body = if method == Method::POST {
        match read_body(request, MAX_BODY).await {
            Ok(body) => body,
            Err((code, message)) => return Ok(error(code, message, Some(&origin), None)),
        }
    } else {
        Value::Null
    };
    Ok(match route(&shared, id, method, &path, body).await {
        Ok(value) => response(StatusCode::OK, value, Some(&origin)),
        Err(message) => error(StatusCode::BAD_REQUEST, &message, Some(&origin), None),
    })
}

/// HTTP 请求带的是某条活着的连接的会话口令。有 Origin 就必须是那条连接的；
/// Chromium 偶尔不给 GET 带 Origin，这时 `X-QB-Extension` 必须是那条连接的扩展 ID。
fn authenticate(
    request: &Request<Incoming>,
    shared: &Shared,
    origin: Option<&str>,
) -> std::result::Result<(u64, String), Box<Response<Full<Bytes>>>> {
    let supplied = header(request, "authorization").and_then(|v| v.strip_prefix("Bearer "));
    let found = {
        let inner = shared.inner.lock().unwrap();
        let mut found = None;
        if let Some(supplied) = supplied {
            for (id, conn) in &inner.conns {
                if conn.ending.is_none() && token_matches(&conn.session, supplied) {
                    found = Some((*id, conn.origin.clone()));
                }
            }
        }
        found
    };
    let Some((id, bound)) = found else {
        return Err(Box::new(error(
            StatusCode::UNAUTHORIZED,
            "连接已失效，请重新连接 QB Gate",
            origin,
            Some("session_invalid"),
        )));
    };
    let matches = match origin {
        Some(origin) => origin == bound,
        None => header(request, "x-qb-extension") == extension_id(&bound),
    };
    if !matches {
        return Err(Box::new(error(
            StatusCode::FORBIDDEN,
            "扩展来源与连接不符",
            None,
            Some("origin"),
        )));
    }
    Ok((id, bound))
}

fn json_result(value: impl Serialize) -> std::result::Result<Value, String> {
    serde_json::to_value(value).map_err(|_| "结果无法编码".into())
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Enhancement {
    webrtc_policy: Option<String>,
    proxy_mode: Option<String>,
    proxy_control: Option<String>,
}

fn enhancement(value: Value) -> std::result::Result<String, String> {
    let data: Enhancement = serde_json::from_value(value).map_err(|_| "浏览器设置格式不正确")?;
    for (value, allowed) in [
        (
            data.webrtc_policy.as_deref(),
            &[
                "default",
                "default_public_and_private_interfaces",
                "default_public_interface_only",
                "disable_non_proxied_udp",
            ][..],
        ),
        (
            data.proxy_mode.as_deref(),
            &[
                "direct",
                "auto_detect",
                "pac_script",
                "fixed_servers",
                "system",
            ][..],
        ),
        (
            data.proxy_control.as_deref(),
            &[
                "not_controllable",
                "controlled_by_other_extensions",
                "controllable_by_this_extension",
                "controlled_by_this_extension",
            ][..],
        ),
    ] {
        if value.is_some_and(|v| !allowed.contains(&v)) {
            return Err("无法识别的浏览器设置值".into());
        }
    }
    serde_json::to_string(&data).map_err(|_| "浏览器设置无法编码".into())
}

fn finish_probe(shared: &Shared, started: Instant) {
    let mut inner = shared.inner.lock().unwrap();
    if inner.probe_started == Some(started) {
        inner.probe_started = None;
    }
}

#[derive(Deserialize)]
struct ArmRequest {
    #[serde(default)]
    baseline: Option<String>,
}

async fn route(
    shared: &Arc<Shared>,
    id: u64,
    method: Method,
    path: &str,
    body: Value,
) -> std::result::Result<Value, String> {
    if !shared.running.load(Ordering::SeqCst) {
        return Err("联动服务已停止，请重新连接".into());
    }
    match (method, path) {
        (Method::GET, "/v2/status") => shared.conn_status(id).ok_or_else(|| DISCONNECTED.into()),
        (Method::POST, "/v2/arm") => {
            let request: ArmRequest =
                serde_json::from_value(body).map_err(|_| "开启保护的请求格式不正确")?;
            if request
                .baseline
                .as_deref()
                .is_some_and(|b| b.parse::<std::net::IpAddr>().is_err())
            {
                return Err("上次放行的出口地址格式不对".into());
            }
            arm(shared, id, request.baseline).await
        }
        (Method::POST, "/v2/disarm") => disarm(shared, id),
        (Method::POST, "/v2/probe/start") => {
            let started = Instant::now();
            {
                let mut inner = shared.inner.lock().unwrap();
                if inner
                    .probe_started
                    .is_some_and(|t| t.elapsed() < crate::browser_probe::TIMEOUT)
                {
                    return Err("已有浏览器采集正在进行，请完成或等待两分钟后重试".into());
                }
                inner.probe_started = Some(started);
            }
            let probe = match crate::browser_probe::start(shared.config.assets.clone()).await {
                Ok(probe) => probe,
                Err(_) => {
                    finish_probe(shared, started);
                    return Err("无法开启浏览器采集".into());
                }
            };
            let state = shared.clone();
            let token = probe.token.clone();
            tokio::spawn(async move {
                crate::browser_probe::wait(&token).await;
                finish_probe(&state, started);
            });
            json_result(probe)
        }
        (Method::POST, "/v2/probe/enhancement") => {
            let report = enhancement(body)?;
            let mut inner = shared.inner.lock().unwrap();
            let pairing = inner
                .conns
                .get(&id)
                .map(|c| c.pairing_id.clone())
                .ok_or(DISCONNECTED)?;
            inner.runtime.entry(pairing).or_default().enhancement_json = Some(report);
            let conn = inner.conns.get_mut(&id).ok_or(DISCONNECTED)?;
            Ok(shared.status_value(conn))
        }
        (Method::GET, "/v2/stations") => match &shared.config.stations {
            Stations::Repository => {
                json_result(browser_station::stations().map_err(|_| "无法读取中转站列表")?)
            }
            Stations::Fixture(list) => json_result(list.as_ref()),
        },
        (Method::POST, "/v2/station/connect") => {
            let request: browser_station::BrowserStationConnect =
                serde_json::from_value(body).map_err(|_| "账户连接格式不正确")?;
            match &shared.config.stations {
                Stations::Repository => {
                    let result =
                        tokio::time::timeout(ACCOUNT_TIMEOUT, browser_station::connect(request))
                            .await
                            .map_err(|_| "账户验证超时，请重试")?
                            .map_err(|_| "账户连接失败，请检查站点地址、登录状态和账户信息")?;
                    json_result(result)
                }
                Stations::Fixture(list) => fixture_connect(list, request),
            }
        }
        (Method::POST, "/v2/station/pricing") => {
            let request = serde_json::from_value(body).map_err(|_| "公开价格格式不正确")?;
            let result = match &shared.config.stations {
                Stations::Repository => browser_station::pricing(request),
                Stations::Fixture(list) => browser_station::pricing_offline(list, request),
            };
            json_result(result.map_err(|_| "价格无法识别或站点不匹配")?)
        }
        _ => Err("未知的联动请求".into()),
    }
}

/// 假站点模式的账户导入：只核对同源与「交来了登录信息」，不验证、不保存、不打印任何一样东西。
fn fixture_connect(
    stations: &[browser_station::BrowserStation],
    request: browser_station::BrowserStationConnect,
) -> std::result::Result<Value, String> {
    if !stations
        .iter()
        .any(|s| s.id == request.station_id && s.origin == request.origin)
    {
        return Err("账户连接失败，请检查站点地址、登录状态和账户信息".into());
    }
    if !["newapi", "sub2"].contains(&request.backend.as_str()) {
        return Err("仅支持 New API 与 Sub2API".into());
    }
    let captured = request.captured.as_array().map_or(0, Vec::len);
    if captured == 0 && request.cookie.is_empty() {
        return Err("未找到可用的登录会话，请先在当前站点登录".into());
    }
    json_result(crate::usecase::station_billing::StationBillingSettings {
        backend: request.backend,
        user_id: String::new(),
        configured: true,
        account: "开发桥：已收到，未验证、未保存".into(),
        balance: None,
        currency: String::new(),
        verified_at: None,
    })
}

// ------------------------------------------------------------------ 认领

#[derive(Deserialize)]
struct ClaimRequest {
    claim: String,
    #[serde(default)]
    pairing: Option<String>,
    #[serde(default)]
    proof: Option<String>,
    #[serde(default)]
    extension_version: Option<String>,
}

type ClaimError = (StatusCode, String, &'static str);

/// 插件在认领页面上认领一次连接：新浏览器拿到长期口令；已配对的证明自己认得口令、学到新端口。
fn claim(
    shared: &Shared,
    origin: &str,
    user_agent: Option<&str>,
    body: Value,
) -> std::result::Result<Value, ClaimError> {
    let bad = |message: &str| (StatusCode::BAD_REQUEST, message.to_string(), "bad_request");
    let ext = extension_id(origin)
        .ok_or((
            StatusCode::FORBIDDEN,
            "认领必须由插件发起".to_string(),
            "origin",
        ))?
        .to_owned();
    let request: ClaimRequest =
        serde_json::from_value(body).map_err(|_| bad("认领请求格式不正确"))?;
    if !is_hex(&request.claim, 64) {
        return Err(bad("认领码格式不对"));
    }
    if request.pairing.as_deref().is_some_and(|p| !is_hex(p, 16)) {
        return Err(bad("配对编号格式不对"));
    }
    let version = short_text(request.extension_version.as_deref(), 32);
    let port = shared.port;
    let mut inner = shared.inner.lock().unwrap();
    inner.claims.retain(|c| !c.forgotten());
    let Some(index) = find_claim(&inner.claims, &request.claim) else {
        return Err((
            StatusCode::GONE,
            "这次连接已经失效，请回到 QB Gate 面板再点一次「连接浏览器」".into(),
            "claim_invalid",
        ));
    };
    if inner.claims[index].used.is_some() {
        return Err((
            StatusCode::GONE,
            "这次连接已经用过了，请回到 QB Gate 面板再点一次「连接浏览器」".into(),
            "claim_used",
        ));
    }
    if inner.claims[index].expired() {
        return Err((
            StatusCode::GONE,
            "这次连接已经超时，请回到 QB Gate 面板再点一次「连接浏览器」".into(),
            "claim_expired",
        ));
    }
    let mut records = inner.records.clone();
    let (at, paired) = match request.pairing.as_deref() {
        Some(pairing) => {
            let Some(i) = records
                .iter()
                .position(|r| r.pairing_id == pairing && r.extension_id == ext)
            else {
                // 不消耗认领码：插件可以当作新浏览器用同一个码再认领一次。
                return Err((
                    StatusCode::NOT_FOUND,
                    "QB Gate 不认识这个配对，请当作新浏览器重新认领".into(),
                    "unknown_pairing",
                ));
            };
            let expected = proof(&records[i].token, "claim", pairing, &request.claim);
            if !request
                .proof
                .as_deref()
                .is_some_and(|p| token_matches(&expected, p))
            {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    "配对口令对不上，请当作新浏览器重新认领".into(),
                    "bad_proof",
                ));
            }
            records[i].port = port;
            if !version.is_empty() {
                records[i].extension_version = version;
            }
            (i, false)
        }
        None => {
            if records.len() >= MAX_PAIRINGS {
                let message = format!(
                    "已经配对了 {MAX_PAIRINGS} 个浏览器，请先在 QB Gate 面板里解除不用的，再刷新这个页面"
                );
                inner.claims[index].problem = Some(message.clone());
                return Err((StatusCode::CONFLICT, message, "too_many_pairings"));
            }
            let mut id = pairing_id();
            while records.iter().any(|r| r.pairing_id == id) {
                id = pairing_id();
            }
            records.push(Record {
                pairing_id: id,
                extension_id: ext,
                token: token(),
                label: browser_label(user_agent),
                extension_version: version,
                paired_at: now_unix(),
                port,
            });
            (records.len() - 1, true)
        }
    };
    if let Err(e) = link_store::save(&shared.config.dir, port, &records) {
        let message = "QB Gate 没能保存配对记录，请稍后刷新这个页面再试".to_string();
        inner.claims[index].problem = Some(message.clone());
        drop(inner);
        shared.emit(&format!("浏览器助手：配对记录保存失败：{e}"));
        return Err((StatusCode::INTERNAL_SERVER_ERROR, message, "save_failed"));
    }
    let record = records[at].clone();
    inner.records = records;
    inner.preferred = port;
    inner.claims[index].used = Some(record.label.clone());
    inner.claims[index].problem = None;
    let count = inner.records.len();
    drop(inner);
    shared.emit(&if paired {
        format!("浏览器助手：已配对「{}」（共 {count} 个）", record.label)
    } else {
        format!("浏览器助手：「{}」已重新连接到端口 {port}", record.label)
    });
    let mut value = json!({
        "type": if paired { "paired" } else { "relinked" },
        "protocol": PROTOCOL,
        "pairing": record.pairing_id,
        "port": port,
        "proof": proof(&record.token, "claimed", &record.pairing_id, &request.claim),
    });
    if paired {
        value["token"] = json!(record.token);
    }
    Ok(value)
}

/// 认领页面自己的查询：只认同源（本机这个端口）的页面。
async fn link_state(request: Request<Incoming>, shared: &Shared) -> Response<Full<Bytes>> {
    let same = header(&request, "origin").is_some_and(|o| {
        o == format!("http://127.0.0.1:{}", shared.port)
            || o == format!("http://localhost:{}", shared.port)
    });
    if !same {
        return error(
            StatusCode::FORBIDDEN,
            "只接受认领页面自己的查询",
            None,
            Some("origin"),
        );
    }
    let body = match read_body(request, 1024).await {
        Ok(body) => body,
        Err((code, message)) => return error(code, message, None, None),
    };
    let Some(code) = body
        .get("claim")
        .and_then(Value::as_str)
        .filter(|c| is_hex(c, 64))
    else {
        return response(StatusCode::OK, json!({"state": "invalid"}), None);
    };
    let mut inner = shared.inner.lock().unwrap();
    inner.claims.retain(|c| !c.forgotten());
    let value = match find_claim(&inner.claims, code) {
        None => json!({"state": "invalid"}),
        Some(i) => {
            let c = &inner.claims[i];
            if let Some(label) = &c.used {
                json!({"state": "claimed", "browser": label})
            } else if c.expired() {
                json!({"state": "expired"})
            } else {
                json!({"state": "pending", "problem": c.problem})
            }
        }
    };
    response(StatusCode::OK, value, None)
}

// ------------------------------------------------------------------ 保护通道（WebSocket）

type Socket = tokio_tungstenite::WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>;

async fn upgrade(request: &mut Request<Incoming>, shared: Arc<Shared>) -> Response<Full<Bytes>> {
    let Some(origin) = header(request, "origin")
        .filter(|o| extension_id(o).is_some())
        .map(str::to_owned)
    else {
        return error(
            StatusCode::FORBIDDEN,
            "保护通道只接受插件连接",
            None,
            Some("origin"),
        );
    };
    let Ok(permit) = shared.sockets.clone().try_acquire_owned() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "连接数已满",
            Some(&origin),
            Some("busy"),
        );
    };
    let builder = Request::builder()
        .method(request.method())
        .uri(request.uri())
        .version(request.version());
    let mut handshake = builder.body(()).unwrap();
    *handshake.headers_mut() = request.headers().clone();
    let Ok(reply) = tungstenite::handshake::server::create_response(&handshake) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid websocket handshake",
            Some(&origin),
            None,
        );
    };
    let upgraded = hyper::upgrade::on(request);
    {
        let mut tasks = shared.socket_tasks.lock().unwrap();
        while tasks.try_join_next().is_some() {}
        tasks.spawn(socket_session(shared.clone(), origin, upgraded, permit));
    }
    let (parts, _) = reply.into_parts();
    Response::from_parts(parts, Full::new(Bytes::new()))
}

async fn send_json(socket: &mut Socket, value: &Value) -> bool {
    matches!(
        tokio::time::timeout(
            HEARTBEAT,
            socket.send(Message::Text(value.to_string().into()))
        )
        .await,
        Ok(Ok(()))
    )
}

async fn read_json(socket: &mut Socket) -> Option<Value> {
    match tokio::time::timeout(AUTH_TIMEOUT, socket.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => serde_json::from_str::<Value>(&text)
            .ok()
            .filter(Value::is_object),
        _ => None,
    }
}

async fn reject(socket: &mut Socket, code: &str, reason: &str) {
    let _ = send_json(
        socket,
        &json!({"type": "rejected", "protocol": PROTOCOL, "code": code, "reason": reason}),
    )
    .await;
}

/// 握手：插件报配对编号与随机数 → 桥用口令证明自己 → 插件证明自己 → 登记连接、发会话口令。
async fn handshake(shared: &Arc<Shared>, origin: &str, socket: &mut Socket) -> Option<u64> {
    let ext = extension_id(origin)?.to_owned();
    let hello = read_json(socket).await?;
    if hello.get("type").and_then(Value::as_str) != Some("hello") {
        return None;
    }
    if hello.get("protocol").and_then(Value::as_u64) != Some(u64::from(PROTOCOL)) {
        reject(
            socket,
            "protocol_mismatch",
            "插件和 QB Gate 的联动协议版本不一致，请换用 QB Gate 自带的插件",
        )
        .await;
        return None;
    }
    let pairing = hello
        .get("pairing")
        .and_then(Value::as_str)
        .filter(|p| is_hex(p, 16))?
        .to_owned();
    let client_nonce = hello
        .get("nonce")
        .and_then(Value::as_str)
        .filter(|n| is_hex(n, 64))?
        .to_owned();
    let version = short_text(hello.get("version").and_then(Value::as_str), 32);
    let secret = {
        let inner = shared.inner.lock().unwrap();
        inner
            .records
            .iter()
            .find(|r| r.pairing_id == pairing && r.extension_id == ext)
            .map(|r| r.token.clone())
    };
    let Some(secret) = secret else {
        reject(
            socket,
            "unknown_pairing",
            "QB Gate 不认识这个配对（可能已在面板里解除），请在面板点「连接浏览器」重新连接",
        )
        .await;
        return None;
    };
    let server_nonce = token();
    let challenge = json!({
        "type": "challenge",
        "protocol": PROTOCOL,
        "nonce": server_nonce,
        "proof": proof(&secret, "server", &client_nonce, &server_nonce),
    });
    if !send_json(socket, &challenge).await {
        return None;
    }
    let auth = read_json(socket).await?;
    if auth.get("type").and_then(Value::as_str) != Some("authenticate") {
        return None;
    }
    let expected = proof(&secret, "client", &client_nonce, &server_nonce);
    if !auth
        .get("proof")
        .and_then(Value::as_str)
        .is_some_and(|p| token_matches(&expected, p))
    {
        reject(
            socket,
            "bad_proof",
            "配对口令对不上，请在面板点「连接浏览器」重新连接",
        )
        .await;
        return None;
    }
    if !shared.running.load(Ordering::SeqCst) {
        return None;
    }
    let mut save_failed = None;
    let registered = {
        let mut inner = shared.inner.lock().unwrap();
        // 握手期间可能被解除了。
        match inner
            .records
            .iter()
            .position(|r| r.pairing_id == pairing && r.extension_id == ext)
        {
            None => None,
            Some(at) => {
                for conn in inner.conns.values_mut() {
                    if conn.pairing_id == pairing {
                        conn.ending.get_or_insert(Ending::Replaced);
                        conn.wake.notify_one();
                    }
                }
                // 它是在这个端口上连上来的，就是知道这个端口。
                let mut changed = false;
                if inner.records[at].port != shared.port {
                    inner.records[at].port = shared.port;
                    inner.preferred = shared.port;
                    changed = true;
                }
                if !version.is_empty() && inner.records[at].extension_version != version {
                    inner.records[at].extension_version = version;
                    changed = true;
                }
                if changed {
                    if let Err(e) =
                        link_store::save(&shared.config.dir, inner.preferred, &inner.records)
                    {
                        save_failed = Some(e.to_string());
                    }
                }
                inner.next_conn += 1;
                let id = inner.next_conn;
                let session = token();
                let mut conn = Conn {
                    pairing_id: pairing.clone(),
                    origin: origin.to_owned(),
                    session: session.clone(),
                    protection: Protection::new("插件已连接，保护尚未开启"),
                    seq: 1,
                    wake: Arc::new(Notify::new()),
                    ending: None,
                };
                let status = shared.status_value(&mut conn);
                inner.conns.insert(id, conn);
                inner.runtime.entry(pairing.clone()).or_default().last = None;
                Some((id, session, status))
            }
        }
    };
    if let Some(e) = save_failed {
        shared.emit(&format!("浏览器助手：配对记录保存失败：{e}"));
    }
    let Some((id, session, status)) = registered else {
        reject(
            socket,
            "unknown_pairing",
            "这个配对刚在面板里解除了，请在面板点「连接浏览器」重新连接",
        )
        .await;
        return None;
    };
    let welcome = json!({
        "type": "welcome",
        "protocol": PROTOCOL,
        "connection": id,
        "pairing": pairing,
        "session": session,
        "port": shared.port,
    });
    if !send_json(socket, &welcome).await || !send_json(socket, &status).await {
        unregister(shared, id);
        return None;
    }
    Some(id)
}

fn unregister(shared: &Shared, id: u64) {
    {
        let mut inner = shared.inner.lock().unwrap();
        if let Some(mut conn) = inner.conns.remove(&id) {
            if conn.ending != Some(Ending::Replaced) {
                let p = &mut conn.protection;
                p.checking = false;
                if p.armed {
                    // 断开不是停止：开着的保护记成「断开、不放行」，不冒充使用者关了保护。
                    p.fail(ProtectionCode::Disconnected, "插件保护通道已断开");
                } else {
                    p.code = ProtectionCode::Disconnected;
                    p.reason = "插件已断开".into();
                }
                inner.runtime.entry(conn.pairing_id).or_default().last = Some(conn.protection);
            }
        }
    }
    shared.gone.notify_waiters();
}

async fn socket_session(
    shared: Arc<Shared>,
    origin: String,
    upgraded: hyper::upgrade::OnUpgrade,
    _permit: OwnedSemaphorePermit,
) {
    let mut stop = shared.shutdown.subscribe();
    if *stop.borrow() {
        return;
    }
    let Ok(Ok(stream)) = tokio::time::timeout(AUTH_TIMEOUT, upgraded).await else {
        return;
    };
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(4096))
        .max_frame_size(Some(4096));
    let mut socket = tokio_tungstenite::WebSocketStream::from_raw_socket(
        TokioIo::new(stream),
        Role::Server,
        Some(config),
    )
    .await;
    let Some(id) = handshake(&shared, &origin, &mut socket).await else {
        let _ = tokio::time::timeout(HEARTBEAT, socket.close(None)).await;
        return;
    };
    let (wake, mut sent) = {
        let inner = shared.inner.lock().unwrap();
        match inner.conns.get(&id) {
            Some(conn) => (conn.wake.clone(), conn.seq),
            None => return,
        }
    };
    let mut heartbeat =
        tokio::time::interval_at(tokio::time::Instant::now() + HEARTBEAT, HEARTBEAT);
    let mut received = Instant::now();
    let ending = loop {
        if *stop.borrow() {
            break None;
        }
        tokio::select! {
            _ = stop.changed() => break None,
            _ = wake.notified() => {
                let next = {
                    let mut inner = shared.inner.lock().unwrap();
                    match inner.conns.get_mut(&id) {
                        None => Err(None),
                        Some(conn) => match conn.ending {
                            Some(ending) => Err(Some(ending)),
                            None if conn.seq != sent => {
                                let value = shared.status_value(conn);
                                Ok(Some((conn.seq, value)))
                            }
                            None => Ok(None),
                        },
                    }
                };
                match next {
                    Err(ending) => break ending,
                    Ok(Some((seq, value))) => {
                        sent = seq;
                        if !send_json(&mut socket, &value).await {
                            break None;
                        }
                    }
                    Ok(None) => {}
                }
            }
            _ = heartbeat.tick() => {
                if received.elapsed() > CLIENT_TIMEOUT {
                    break None;
                }
                let next = {
                    let mut inner = shared.inner.lock().unwrap();
                    inner.conns.get_mut(&id).filter(|c| c.ending.is_none()).map(|conn| {
                        let value = shared.status_value(conn);
                        (conn.seq, value)
                    })
                };
                // 收到了结束标记的那一轮由 wake 分支处理。
                if let Some((seq, value)) = next {
                    sent = seq;
                    if !send_json(&mut socket, &value).await {
                        break None;
                    }
                }
            }
            message = socket.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    let Ok(value) = serde_json::from_str::<Value>(&text) else { break None };
                    match value.get("type").and_then(Value::as_str) {
                        Some("ping") => received = Instant::now(),
                        Some("recheck") => {
                            received = Instant::now();
                            shared.recheck.notify_one();
                        }
                        _ => break None,
                    }
                }
                Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => received = Instant::now(),
                _ => break None,
            }
        }
    };
    if let Some(ending) = ending {
        let _ = send_json(&mut socket, &ending.message()).await;
    }
    let _ = tokio::time::timeout(HEARTBEAT, socket.close(None)).await;
    unregister(&shared, id);
}
