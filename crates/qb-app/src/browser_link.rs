//! Explicitly enabled, authenticated loopback bridge for the Chrome companion.
//!
//! Pairing tokens and browser settings remain in memory. Only the extension closes
//! its own windows; this module never launches or kills a browser. The existing IP
//! judge remains the authority, with an additional immutable IP baseline per arm.

use std::convert::Infallible;
use std::future::Future;
use std::net::Ipv4Addr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
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
use tokio::sync::{watch, Semaphore};
use tokio_tungstenite::tungstenite::{self, protocol::Role, Message};
use ts_rs::TS;

use crate::browser_probe::AssetSource;
use crate::error::Result;
use crate::gate::{self, judge::Judgement, GateState};
use crate::usecase::browser_station;

const MAX_BODY: usize = 2 * 1024 * 1024;
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(12);
const HEARTBEAT: Duration = Duration::from_secs(2);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(15);
const STALE_AFTER: Duration = Duration::from_secs(20);
// Account verification can make four sequential requests with a 25-second
// per-request timeout. The bridge deadline must allow that bounded workflow.
const ACCOUNT_TIMEOUT: Duration = Duration::from_secs(105);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(110);

#[derive(Clone, Serialize, TS)]
#[ts(export)]
pub struct BrowserLinkStart {
    pub endpoint: String,
    pub token: String,
    pub pairing: String,
}

#[derive(Clone, Serialize, TS)]
#[ts(export)]
pub struct BrowserLinkStatus {
    pub running: bool,
    pub connected: bool,
    pub armed: bool,
    pub safe: bool,
    pub ip: Option<String>,
    pub reason: String,
    #[ts(type = "number")]
    pub checked_at: u64,
    pub enhancement_json: Option<String>,
}

impl Default for BrowserLinkStatus {
    fn default() -> Self {
        Self {
            running: false,
            connected: false,
            armed: false,
            safe: false,
            ip: None,
            reason: "浏览器联动未开启".into(),
            checked_at: 0,
            enhancement_json: None,
        }
    }
}

#[derive(Default)]
struct Session {
    status: BrowserLinkStatus,
    origin: Option<String>,
    baseline: Option<String>,
    generation: u64,
    revision: u64,
    checked: Option<Instant>,
    probe_started: Option<Instant>,
}

type Authority = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Judgement> + Send>> + Send + Sync>;

struct Shared {
    port: u16,
    token: String,
    gate: Arc<GateState>,
    authority: Authority,
    assets: Arc<dyn AssetSource>,
    session: Mutex<Session>,
    running: AtomicBool,
    check_lock: tokio::sync::Mutex<()>,
    recheck: tokio::sync::Notify,
    shutdown: watch::Sender<bool>,
    connections: Arc<Semaphore>,
    websocket: Arc<Semaphore>,
}

struct Running {
    shared: Arc<Shared>,
    task: tokio::task::JoinHandle<()>,
}

fn current() -> &'static tokio::sync::Mutex<Option<Running>> {
    static CURRENT: OnceLock<tokio::sync::Mutex<Option<Running>>> = OnceLock::new();
    CURRENT.get_or_init(|| tokio::sync::Mutex::new(None))
}

fn token() -> String {
    // UUIDs supply 122 random bits each. Discard the fixed version/variant nibbles
    // from three UUIDs, then take 256 random bits, without a new RNG dependency.
    (0..3)
        .flat_map(|_| {
            uuid::Uuid::new_v4()
                .simple()
                .to_string()
                .chars()
                .enumerate()
                .filter_map(|(i, c)| (!matches!(i, 12 | 16)).then_some(c))
                .collect::<Vec<_>>()
        })
        .take(64)
        .collect()
}

pub async fn start(gate: Arc<GateState>, assets: Arc<dyn AssetSource>) -> Result<BrowserLinkStart> {
    let mut slot = current().lock().await;
    if let Some(old) = slot.take() {
        shutdown(old).await;
    }
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let port = listener.local_addr()?.port();
    let token = token();
    let endpoint = format!("http://127.0.0.1:{port}");
    let pairing = json!({"endpoint": endpoint, "token": token}).to_string();
    let (shutdown, _) = watch::channel(false);
    let shared = Arc::new(Shared {
        port,
        token: token.clone(),
        gate,
        authority: Arc::new(|| Box::pin(gate::judge_now())),
        assets,
        session: Mutex::new(Session {
            status: BrowserLinkStatus {
                running: true,
                reason: "等待 Chrome 插件配对".into(),
                ..Default::default()
            },
            ..Default::default()
        }),
        running: AtomicBool::new(true),
        check_lock: tokio::sync::Mutex::new(()),
        recheck: tokio::sync::Notify::new(),
        shutdown,
        connections: Arc::new(Semaphore::new(8)),
        websocket: Arc::new(Semaphore::new(1)),
    });
    let worker = shared.clone();
    let task = tokio::spawn(async move { serve(listener, worker, true).await });
    *slot = Some(Running { shared, task });
    Ok(BrowserLinkStart {
        endpoint,
        token,
        pairing,
    })
}

async fn shutdown(running: Running) {
    // Keep armed=true in the final snapshot: disconnection must close an armed
    // extension, including deliberate stop or pairing rotation in the panel.
    running.shared.running.store(false, Ordering::SeqCst);
    running.shared.shutdown.send_replace(true);
    let _ = running.task.await;
}

pub async fn stop() {
    if let Some(running) = current().lock().await.take() {
        shutdown(running).await;
    }
}

pub async fn status() -> BrowserLinkStatus {
    current()
        .lock()
        .await
        .as_ref()
        .map(|r| snapshot(&r.shared))
        .unwrap_or_default()
}

fn fail(session: &mut Session, reason: &str) {
    session.revision = session.revision.wrapping_add(1);
    session.status.safe = false;
    session.status.reason = reason.to_owned();
}

fn snapshot(shared: &Shared) -> BrowserLinkStatus {
    let mut s = shared.session.lock().unwrap();
    if s.status.armed && s.status.safe {
        if !shared.running.load(Ordering::SeqCst) {
            fail(&mut s, "面板联动服务已停止");
        } else if shared.gate.manual_paused.load(Ordering::SeqCst) {
            fail(&mut s, "IP 锁已手动关闭");
        } else if shared.gate.generation.load(Ordering::SeqCst) != s.generation {
            fail(&mut s, "IP 锁或网络设置发生变化，请重新开启保护");
        } else if s.checked.is_none_or(|t| t.elapsed() > STALE_AFTER) {
            fail(&mut s, "出口检测已过期");
        }
    }
    s.status.running = shared.running.load(Ordering::SeqCst);
    s.status.clone()
}

async fn serve(listener: tokio::net::TcpListener, shared: Arc<Shared>, monitor_enabled: bool) {
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
        let Ok((stream, _)) = accepted else { break };
        let Ok(permit) = shared.connections.clone().try_acquire_owned() else {
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
        let interval = if snapshot(&shared).armed {
            crate::settings::gate_check_interval().min(Duration::from_secs(5))
        } else {
            Duration::from_secs(5)
        };
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = tokio::time::sleep(interval) => {},
            _ = shared.recheck.notified() => {},
        }
        let status = snapshot(&shared);
        if status.armed && status.safe {
            check(&shared, false).await;
        }
    }
}

fn verdict_result(verdict: &Judgement, baseline: Option<&str>) -> (Option<String>, bool, String) {
    match verdict {
        Judgement::Allowed { ip } if baseline.is_some_and(|old| old != ip) => (
            Some(ip.clone()),
            false,
            "出口 IP 已变化，浏览器保护已触发".into(),
        ),
        Judgement::Allowed { ip } => (Some(ip.clone()), true, "出口 IP 已核实，保护中".into()),
        other => (None, false, other.reason()),
    }
}

async fn check(shared: &Shared, arm: bool) -> BrowserLinkStatus {
    // Capture intent before waiting: an already queued arm must not revive
    // protection after a newer explicit disarm while another probe holds the lock.
    let requested_revision = shared.session.lock().unwrap().revision;
    let _serial = shared.check_lock.lock().await;
    if shared.session.lock().unwrap().revision != requested_revision {
        return snapshot(shared);
    }
    let before = snapshot(shared);
    if !shared.running.load(Ordering::SeqCst) || !before.connected {
        return before;
    }
    if !arm && (!before.armed || !before.safe) {
        return before;
    }
    if arm && before.armed {
        return before;
    }
    if shared.gate.manual_paused.load(Ordering::SeqCst) {
        let mut s = shared.session.lock().unwrap();
        fail(&mut s, "IP 锁已手动关闭，请先在面板重新放行");
        return s.status.clone();
    }
    let generation = shared.gate.generation.load(Ordering::SeqCst);
    let revision = shared.session.lock().unwrap().revision;
    let verdict = tokio::time::timeout(PROBE_TIMEOUT, (shared.authority)()).await;
    let mut s = shared.session.lock().unwrap();
    if s.revision != revision || !shared.running.load(Ordering::SeqCst) || !s.status.connected {
        return s.status.clone();
    }
    if generation != shared.gate.generation.load(Ordering::SeqCst)
        || shared.gate.manual_paused.load(Ordering::SeqCst)
    {
        fail(&mut s, "检测期间 IP 锁或网络设置发生变化");
        return s.status.clone();
    }
    let (ip, safe, reason) = verdict
        .as_ref()
        .map(|v| verdict_result(v, if arm { None } else { s.baseline.as_deref() }))
        .unwrap_or_else(|_| (None, false, "出口 IP 检测超时".into()));
    s.status.checked_at = chrono::Utc::now().timestamp().max(0) as u64;
    s.checked = Some(Instant::now());
    s.status.ip = ip.clone();
    s.status.safe = safe;
    s.status.reason = reason;
    if arm && safe {
        s.status.armed = true;
        s.baseline = ip;
        s.generation = generation;
    }
    s.status.clone()
}

fn header<'a>(request: &'a Request<Incoming>, name: &str) -> Option<&'a str> {
    request.headers().get(name)?.to_str().ok()
}

fn extension_id(origin: &str) -> Option<&str> {
    let id = origin.strip_prefix("chrome-extension://")?;
    (id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))).then_some(id)
}

fn token_matches(expected: &str, supplied: &str) -> bool {
    if supplied.len() != expected.len() {
        return false;
    }
    expected
        .bytes()
        .zip(supplied.bytes())
        .fold(0u8, |different, (a, b)| different | (a ^ b))
        == 0
}

fn origin_allowed(shared: &Shared, origin: &str, bind: bool) -> bool {
    if extension_id(origin).is_none() {
        return false;
    }
    let mut s = shared.session.lock().unwrap();
    match s.origin.as_deref() {
        Some(bound) => bound == origin,
        None if bind => {
            s.origin = Some(origin.to_owned());
            true
        }
        None => true,
    }
}

fn http_origin(request: &Request<Incoming>, shared: &Shared) -> Option<String> {
    let claimed = header(request, "x-qb-extension");
    if let Some(origin) = header(request, "origin") {
        let id = extension_id(origin)?;
        if claimed.is_some_and(|v| v != id) {
            return None;
        }
        return origin_allowed(shared, origin, false).then(|| origin.to_owned());
    }
    // Chromium can omit Origin on extension GETs. This fallback requires a prior
    // authenticated extension-origin binding; it never bootstraps pairing by itself.
    let bound = shared.session.lock().unwrap().origin.clone()?;
    (claimed == extension_id(&bound)).then_some(bound)
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

fn error(code: StatusCode, message: &str, origin: Option<&str>) -> Response<Full<Bytes>> {
    response(code, json!({"error": message}), origin)
}

async fn handle(
    mut request: Request<Incoming>,
    shared: Arc<Shared>,
) -> std::result::Result<Response<Full<Bytes>>, Infallible> {
    if !shared.running.load(Ordering::SeqCst) {
        return Ok(error(StatusCode::GONE, "联动服务已停止", None));
    }
    if !crate::browser_probe::host_ok(header(&request, "host"), shared.port)
        || request.uri().scheme().is_some()
        || request.uri().query().is_some()
    {
        return Ok(error(StatusCode::MISDIRECTED_REQUEST, "invalid host", None));
    }
    let path = request.uri().path().to_owned();
    if request.method() == Method::GET && path == "/v1/events" {
        return Ok(upgrade(&mut request, shared).await);
    }
    let Some(origin) = http_origin(&request, &shared) else {
        return Ok(error(StatusCode::FORBIDDEN, "扩展来源未配对", None));
    };
    if request.method() == Method::OPTIONS {
        let mut reply = response(StatusCode::NO_CONTENT, Value::Null, Some(&origin));
        reply.headers_mut().insert(
            "access-control-allow-methods",
            "GET, POST, OPTIONS".parse().unwrap(),
        );
        reply.headers_mut().insert(
            "access-control-allow-headers",
            "Authorization, Content-Type, X-QB-Extension"
                .parse()
                .unwrap(),
        );
        reply.headers_mut().insert(
            "access-control-allow-private-network",
            "true".parse().unwrap(),
        );
        return Ok(reply);
    }
    let supplied = header(&request, "authorization").and_then(|v| v.strip_prefix("Bearer "));
    if !supplied.is_some_and(|v| token_matches(&shared.token, v))
        || !origin_allowed(&shared, &origin, true)
    {
        return Ok(error(StatusCode::UNAUTHORIZED, "配对已失效", Some(&origin)));
    }
    let method = request.method().clone();
    let body = if method == Method::POST {
        match tokio::time::timeout(
            AUTH_TIMEOUT,
            Limited::new(request.into_body(), MAX_BODY).collect(),
        )
        .await
        {
            Ok(Ok(body)) => match serde_json::from_slice::<Value>(&body.to_bytes()) {
                Ok(value) => value,
                Err(_) => {
                    return Ok(error(
                        StatusCode::BAD_REQUEST,
                        "请求不是 JSON",
                        Some(&origin),
                    ))
                }
            },
            _ => {
                return Ok(error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "请求过大或读取超时",
                    Some(&origin),
                ))
            }
        }
    } else {
        Value::Null
    };
    let result = route(&shared, method, &path, body).await;
    Ok(match result {
        Ok(value) => response(StatusCode::OK, value, Some(&origin)),
        Err(message) => error(StatusCode::BAD_REQUEST, &message, Some(&origin)),
    })
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
    let mut session = shared.session.lock().unwrap();
    if session.probe_started == Some(started) {
        session.probe_started = None;
    }
}

async fn route(
    shared: &Arc<Shared>,
    method: Method,
    path: &str,
    body: Value,
) -> std::result::Result<Value, String> {
    if !shared.running.load(Ordering::SeqCst) {
        return Err("联动服务已停止，请重新配对".into());
    }
    match (method, path) {
        (Method::GET, "/v1/status") => json_result(snapshot(shared)),
        (Method::POST, "/v1/arm") => {
            if !snapshot(shared).connected {
                return Err("请先连接插件保护通道".into());
            }
            let status = check(shared, true).await;
            if !status.armed || !status.safe {
                return Err(status.reason);
            }
            json_result(status)
        }
        (Method::POST, "/v1/disarm") => {
            let mut s = shared.session.lock().unwrap();
            s.revision = s.revision.wrapping_add(1);
            s.status.armed = false;
            s.status.safe = false;
            s.baseline = None;
            s.status.reason = "已暂停浏览器保护".into();
            json_result(s.status.clone())
        }
        (Method::POST, "/v1/probe/start") => {
            let started = Instant::now();
            {
                let mut s = shared.session.lock().unwrap();
                if s.probe_started
                    .is_some_and(|t| t.elapsed() < crate::browser_probe::TIMEOUT)
                {
                    return Err("已有浏览器采集正在进行，请完成或等待两分钟后重试".into());
                }
                s.probe_started = Some(started);
            }
            let probe = match crate::browser_probe::start(shared.assets.clone()).await {
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
        (Method::POST, "/v1/probe/enhancement") => {
            let report = enhancement(body)?;
            shared.session.lock().unwrap().status.enhancement_json = Some(report);
            json_result(snapshot(shared))
        }
        (Method::GET, "/v1/stations") => {
            json_result(browser_station::stations().map_err(|_| "无法读取中转站列表")?)
        }
        (Method::POST, "/v1/station/connect") => {
            let request = serde_json::from_value(body).map_err(|_| "账户连接格式不正确")?;
            let result = tokio::time::timeout(ACCOUNT_TIMEOUT, browser_station::connect(request))
                .await
                .map_err(|_| "账户验证超时，请重试")?
                .map_err(|_| "账户连接失败，请检查站点地址、登录状态和账户信息")?;
            json_result(result)
        }
        (Method::POST, "/v1/station/pricing") => {
            let request = serde_json::from_value(body).map_err(|_| "公开价格格式不正确")?;
            json_result(browser_station::pricing(request).map_err(|_| "价格无法识别或站点不匹配")?)
        }
        _ => Err("未知的联动请求".into()),
    }
}

async fn upgrade(request: &mut Request<Incoming>, shared: Arc<Shared>) -> Response<Full<Bytes>> {
    let Some(origin) = header(request, "origin").map(str::to_owned) else {
        return error(StatusCode::FORBIDDEN, "missing extension origin", None);
    };
    if !origin_allowed(&shared, &origin, false) {
        return error(StatusCode::FORBIDDEN, "invalid extension origin", None);
    }
    let Ok(permit) = shared.websocket.clone().try_acquire_owned() else {
        return error(StatusCode::CONFLICT, "保护通道已连接", Some(&origin));
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
        );
    };
    let upgraded = hyper::upgrade::on(request);
    tokio::spawn(async move {
        let _permit = permit;
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
        let first = tokio::time::timeout(AUTH_TIMEOUT, socket.next()).await;
        let authenticated = match first {
            Ok(Some(Ok(Message::Text(text)))) => {
                serde_json::from_str::<Value>(&text).ok().is_some_and(|v| {
                    v.get("type").and_then(Value::as_str) == Some("authenticate")
                        && v.get("token")
                            .and_then(Value::as_str)
                            .is_some_and(|t| token_matches(&shared.token, t))
                        && origin_allowed(&shared, &origin, true)
                })
            }
            _ => false,
        };
        if !authenticated || !shared.running.load(Ordering::SeqCst) {
            let _ = socket.close(None).await;
            return;
        }
        {
            let mut s = shared.session.lock().unwrap();
            s.status.connected = true;
            if !s.status.armed {
                s.status.reason = "插件已连接，保护尚未开启".into();
            }
        }
        let mut stop = shared.shutdown.subscribe();
        let mut heartbeat = tokio::time::interval(HEARTBEAT);
        let mut received = Instant::now();
        loop {
            if *stop.borrow() {
                break;
            }
            tokio::select! {
                _ = stop.changed() => break,
                _ = heartbeat.tick() => {
                    if received.elapsed() > CLIENT_TIMEOUT { break; }
                    let mut value = serde_json::to_value(snapshot(&shared)).unwrap();
                    value["type"] = json!("status");
                    if !matches!(tokio::time::timeout(HEARTBEAT, socket.send(Message::Text(value.to_string().into()))).await, Ok(Ok(()))) { break; }
                }
                message = socket.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            let Ok(value) = serde_json::from_str::<Value>(&text) else { break };
                            match value.get("type").and_then(Value::as_str) {
                                Some("ping") => received = Instant::now(),
                                Some("recheck") => {
                                    received = Instant::now();
                                    shared.recheck.notify_one();
                                }
                                _ => break,
                            }
                        }
                        Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => received = Instant::now(),
                        _ => break,
                    }
                }
            }
        }
        {
            let mut s = shared.session.lock().unwrap();
            s.revision = s.revision.wrapping_add(1);
            s.status.connected = false;
            if s.status.armed {
                fail(&mut s, "插件保护通道已断开");
            } else {
                s.status.reason = "插件已断开，请重新连接".into();
            }
        }
        let _ = tokio::time::timeout(HEARTBEAT, socket.close(None)).await;
    });
    let (parts, _) = reply.into_parts();
    Response::from_parts(parts, Full::new(Bytes::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    const ORIGIN: &str = "chrome-extension://abcdefghijklmnopabcdefghijklmnop";
    const OTHER_ORIGIN: &str = "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    struct MemoryAssets;
    impl AssetSource for MemoryAssets {
        fn get(&self, _: &str) -> Option<(String, Vec<u8>)> {
            None
        }
    }

    async fn fixture() -> (Arc<Shared>, tokio::task::JoinHandle<()>) {
        fixture_with(Arc::new(|| {
            Box::pin(async {
                Judgement::Allowed {
                    ip: "203.0.113.1".into(),
                }
            })
        }))
        .await
    }

    async fn fixture_with(authority: Authority) -> (Arc<Shared>, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let (shutdown, _) = watch::channel(false);
        let shared = Arc::new(Shared {
            port,
            token: token(),
            gate: Arc::new(GateState::default()),
            authority,
            assets: Arc::new(MemoryAssets),
            session: Mutex::new(Session {
                status: BrowserLinkStatus {
                    running: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            running: AtomicBool::new(true),
            check_lock: tokio::sync::Mutex::new(()),
            recheck: tokio::sync::Notify::new(),
            shutdown,
            connections: Arc::new(Semaphore::new(8)),
            websocket: Arc::new(Semaphore::new(1)),
        });
        let worker = shared.clone();
        (
            shared,
            // No production IP probes or settings reads in loopback tests.
            tokio::spawn(async move { serve(listener, worker, false).await }),
        )
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap()
    }

    fn url(shared: &Shared, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", shared.port)
    }

    async fn socket(
        shared: &Shared,
        origin: &str,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        let mut request = format!("ws://127.0.0.1:{}/v1/events", shared.port)
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("origin", origin.parse().unwrap());
        tokio_tungstenite::connect_async(request).await.unwrap().0
    }

    async fn authenticate(
        shared: &Shared,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        let mut ws = socket(shared, ORIGIN).await;
        ws.send(Message::Text(
            json!({"type":"authenticate", "token":shared.token})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        let first = tokio::time::timeout(Duration::from_secs(3), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let value: Value = serde_json::from_str(first.to_text().unwrap()).unwrap();
        assert_eq!(value["type"], "status");
        assert_eq!(value["connected"], true);
        ws
    }

    #[tokio::test]
    async fn arming_requires_a_live_socket_and_changed_ip_cannot_be_rearmed_without_disarm() {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let authority: Authority = Arc::new(move || {
            let first = calls.fetch_add(1, Ordering::SeqCst) == 0;
            Box::pin(async move {
                Judgement::Allowed {
                    ip: if first { "203.0.113.1" } else { "203.0.113.2" }.into(),
                }
            })
        });
        let (shared, task) = fixture_with(authority).await;
        let client = client();
        let arm = || {
            client
                .post(url(&shared, "/v1/arm"))
                .header("Origin", ORIGIN)
                .bearer_auth(&shared.token)
                .json(&json!({}))
        };
        assert_eq!(arm().send().await.unwrap().status().as_u16(), 400);
        let _ws = authenticate(&shared).await;
        let armed: Value = arm().send().await.unwrap().json().await.unwrap();
        assert_eq!(armed["armed"], true);
        assert_eq!(armed["safe"], true);
        let changed = check(&shared, false).await;
        assert!(changed.armed);
        assert!(!changed.safe);
        assert_eq!(arm().send().await.unwrap().status().as_u16(), 400);
        route(&shared, Method::POST, "/v1/disarm", json!({}))
            .await
            .unwrap();
        assert_eq!(arm().send().await.unwrap().status().as_u16(), 200);
        assert_eq!(snapshot(&shared).ip.as_deref(), Some("203.0.113.2"));
        shutdown(Running { shared, task }).await;
    }

    #[tokio::test]
    async fn an_arm_queued_behind_a_probe_cannot_override_later_disarm() {
        let (shared, task) = fixture().await;
        let _ws = authenticate(&shared).await;
        let guard = shared.check_lock.lock().await;
        let pending = shared.clone();
        let mut arm = tokio::spawn(async move { check(&pending, true).await });
        assert!(tokio::time::timeout(Duration::from_millis(50), &mut arm)
            .await
            .is_err());
        route(&shared, Method::POST, "/v1/disarm", json!({}))
            .await
            .unwrap();
        drop(guard);
        let result = arm.await.unwrap();
        assert!(!result.armed);
        assert!(!result.safe);
        shutdown(Running { shared, task }).await;
    }

    #[tokio::test]
    async fn a_late_successful_probe_cannot_undo_explicit_disarm() {
        let entered = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        let authority: Authority = {
            let entered = entered.clone();
            let resume = resume.clone();
            Arc::new(move || {
                let entered = entered.clone();
                let resume = resume.clone();
                Box::pin(async move {
                    entered.notify_one();
                    resume.notified().await;
                    Judgement::Allowed {
                        ip: "203.0.113.1".into(),
                    }
                })
            })
        };
        let (shared, task) = fixture_with(authority).await;
        let _ws = authenticate(&shared).await;
        let pending = shared.clone();
        let probe = tokio::spawn(async move { check(&pending, true).await });
        entered.notified().await;
        route(&shared, Method::POST, "/v1/disarm", json!({}))
            .await
            .unwrap();
        resume.notify_one();
        let status = probe.await.unwrap();
        assert!(!status.armed);
        assert!(!status.safe);
        shutdown(Running { shared, task }).await;
    }

    #[tokio::test]
    async fn loopback_requests_require_host_origin_and_token_without_pairing_on_failure() {
        let (shared, task) = fixture().await;
        let client = client();
        let endpoint = url(&shared, "/v1/status");
        for (origin, token, expected) in [
            ("https://example.invalid", shared.token.as_str(), 403),
            (ORIGIN, "wrong", 401),
        ] {
            let reply = client
                .get(&endpoint)
                .header("Origin", origin)
                .bearer_auth(token)
                .send()
                .await
                .unwrap();
            assert_eq!(reply.status().as_u16(), expected);
            assert!(shared.session.lock().unwrap().origin.is_none());
        }
        let reply = client
            .get(&endpoint)
            .header("Host", "malicious.invalid")
            .header("Origin", ORIGIN)
            .bearer_auth(&shared.token)
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status().as_u16(), 421);
        let reply = client
            .get(&endpoint)
            .header("X-QB-Extension", extension_id(ORIGIN).unwrap())
            .bearer_auth(&shared.token)
            .send()
            .await
            .unwrap();
        assert_eq!(
            reply.status().as_u16(),
            403,
            "missing Origin cannot establish the first binding"
        );
        let reply = client
            .get(&endpoint)
            .header("Origin", ORIGIN)
            .bearer_auth(&shared.token)
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status().as_u16(), 200);
        assert_eq!(reply.headers()["access-control-allow-origin"], ORIGIN);
        let reply = client
            .get(&endpoint)
            .header("Origin", OTHER_ORIGIN)
            .bearer_auth(&shared.token)
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status().as_u16(), 403);
        shutdown(Running { shared, task }).await;
    }

    #[tokio::test]
    async fn authenticated_socket_enables_originless_chrome_get_and_disconnect_latches_failure() {
        let (shared, task) = fixture().await;
        let mut ws = authenticate(&shared).await;
        let response = client()
            .get(url(&shared, "/v1/status"))
            .header("X-QB-Extension", extension_id(ORIGIN).unwrap())
            .bearer_auth(&shared.token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 200);
        {
            let mut s = shared.session.lock().unwrap();
            s.status.armed = true;
            s.status.safe = true;
            s.checked = Some(Instant::now());
        }
        ws.close(None).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while snapshot(&shared).connected {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let status = snapshot(&shared);
        assert!(
            status.armed,
            "a lost connection never masquerades as explicit disarm"
        );
        assert!(!status.safe);
        assert!(status.reason.contains("断开"));
        shutdown(Running { shared, task }).await;
    }

    #[tokio::test]
    async fn wrong_websocket_token_is_rejected_before_connection_or_origin_binding() {
        let (shared, task) = fixture().await;
        let mut ws = socket(&shared, ORIGIN).await;
        ws.send(Message::Text(
            json!({"type":"authenticate","token":"incorrect"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        let reply = tokio::time::timeout(Duration::from_secs(2), ws.next())
            .await
            .unwrap();
        assert!(matches!(
            reply,
            Some(Ok(Message::Close(_))) | None | Some(Err(_))
        ));
        assert!(!snapshot(&shared).connected);
        assert!(shared.session.lock().unwrap().origin.is_none());
        shutdown(Running { shared, task }).await;
    }

    #[tokio::test]
    async fn stop_cancels_and_joins_an_in_flight_authenticated_arm_request() {
        let entered = Arc::new(tokio::sync::Notify::new());
        let dropped = Arc::new(AtomicBool::new(false));
        struct Dropped(Arc<AtomicBool>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let authority: Authority = {
            let entered = entered.clone();
            let dropped = dropped.clone();
            Arc::new(move || {
                let entered = entered.clone();
                let dropped = dropped.clone();
                Box::pin(async move {
                    let _dropped = Dropped(dropped);
                    entered.notify_one();
                    std::future::pending::<Judgement>().await
                })
            })
        };
        let (shared, task) = fixture_with(authority).await;
        let _ws = authenticate(&shared).await;
        let endpoint = url(&shared, "/v1/arm");
        let token = shared.token.clone();
        let request = tokio::spawn(async move {
            client()
                .post(endpoint)
                .header("Origin", ORIGIN)
                .bearer_auth(token)
                .json(&json!({}))
                .send()
                .await
        });
        entered.notified().await;
        shutdown(Running {
            shared: shared.clone(),
            task,
        })
        .await;
        assert!(
            dropped.load(Ordering::SeqCst),
            "stop must join the authenticated request, not leave it detached"
        );
        assert!(request.await.unwrap().is_err());
        assert!(route(&shared, Method::POST, "/v1/disarm", json!({}))
            .await
            .is_err());
        assert!(
            route(&shared, Method::POST, "/v1/probe/enhancement", json!({}))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn explicit_stop_closes_listener_and_armed_socket_without_sending_disarmed_state() {
        let (shared, task) = fixture().await;
        let mut ws = authenticate(&shared).await;
        {
            let mut s = shared.session.lock().unwrap();
            s.status.armed = true;
            s.status.safe = true;
            s.checked = Some(Instant::now());
        }
        let endpoint = url(&shared, "/v1/status");
        shutdown(Running {
            shared: shared.clone(),
            task,
        })
        .await;
        assert!(client().get(endpoint).send().await.is_err());
        tokio::time::timeout(Duration::from_secs(3), async {
            while let Some(Ok(message)) = ws.next().await {
                if message.is_close() {
                    break;
                }
                if let Message::Text(text) = message {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    assert_eq!(value["armed"], true);
                }
            }
        })
        .await
        .unwrap();
        assert!(snapshot(&shared).armed);
        assert!(!snapshot(&shared).safe);
    }

    #[tokio::test]
    async fn generation_and_staleness_fail_closed_and_cannot_become_safe_by_reading_again() {
        let (shared, task) = fixture().await;
        {
            let mut s = shared.session.lock().unwrap();
            s.status.armed = true;
            s.status.safe = true;
            s.checked = Some(Instant::now());
        }
        shared.gate.generation.fetch_add(1, Ordering::SeqCst);
        assert!(!snapshot(&shared).safe);
        shared.gate.generation.store(0, Ordering::SeqCst);
        assert!(!snapshot(&shared).safe);
        {
            let mut s = shared.session.lock().unwrap();
            s.status.safe = true;
            s.checked = Some(Instant::now() - STALE_AFTER - Duration::from_secs(1));
        }
        assert!(!snapshot(&shared).safe);
        shutdown(Running { shared, task }).await;
    }

    #[tokio::test]
    async fn oversized_request_is_rejected_without_retaining_body() {
        let (shared, task) = fixture().await;
        let response = client()
            .post(url(&shared, "/v1/probe/enhancement"))
            .header("Origin", ORIGIN)
            .bearer_auth(&shared.token)
            .body("x".repeat(MAX_BODY + 1))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 413);
        assert!(snapshot(&shared).enhancement_json.is_none());
        shutdown(Running { shared, task }).await;
    }

    #[test]
    fn a_different_allowlisted_ip_still_triggers_browser_protection() {
        let allowed = Judgement::Allowed {
            ip: "203.0.113.2".into(),
        };
        assert!(!verdict_result(&allowed, Some("203.0.113.1")).1);
        assert!(verdict_result(&allowed, Some("203.0.113.2")).1);
        assert!(!verdict_result(&Judgement::IpUnknown, None).1);
    }

    #[test]
    fn only_exact_chrome_extension_origins_are_accepted() {
        assert!(extension_id("chrome-extension://abcdefghijklmnopabcdefghijklmnop").is_some());
        for origin in [
            "https://example.com",
            "null",
            "chrome-extension://short",
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop/",
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop.evil",
        ] {
            assert!(extension_id(origin).is_none());
        }
    }

    #[test]
    fn tokens_rotate_and_match_without_prefix_acceptance() {
        let a = token();
        let b = token();
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert!(token_matches(&a, &a));
        assert!(!token_matches(&a, &b));
        assert!(!token_matches(&a, &a[..63]));
    }

    #[test]
    fn enhancement_never_accepts_proxy_urls_or_extra_credentials() {
        assert!(enhancement(
            json!({"proxy_mode":"system","webrtc_policy":"default","proxy_control":null})
        )
        .is_ok());
        assert!(enhancement(json!({"proxy_mode":"http://secret@example.com"})).is_err());
        assert!(enhancement(json!({"proxy_mode":"system","password":"private"})).is_err());
    }
}
