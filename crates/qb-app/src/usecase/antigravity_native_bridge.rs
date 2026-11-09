//! Explicit, text-only Antigravity IDE bridge orchestration for local SillyTavern.
//!
//! This is separate from Gemini CLI and the relay router. One user-selected IDE
//! login, one discovered model, one official host; no rotation, relay fallback,
//! onboarding, tool execution, credential writeback or background discovery.

mod protocol;
pub use protocol::AntigravityBridgeModel;

use super::antigravity_quota::{self as oauth, Source};
use crate::error::{GateError, Result};
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full, Limited, StreamBody};
use hyper::body::{Bytes, Frame, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use qb_platform::credentials::Secret;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use ts_rs::TS;

const HOST: &str = "https://cloudcode-pa.googleapis.com";
const TURN_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_BODY: usize = 2 * 1024 * 1024;
type Body = UnsyncBoxBody<Bytes, Infallible>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(default)]
#[ts(export)]
pub struct AntigravityBridgeConfig {
    pub account_id: String,
    pub port: u16,
    pub model: String,
    pub system_prompt: String,
}

impl Default for AntigravityBridgeConfig {
    fn default() -> Self {
        Self {
            account_id: String::new(),
            port: 5004,
            model: String::new(),
            system_prompt: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct AntigravityBridgeAccount {
    pub id: String,
    pub label: String,
    pub logged_in: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct AntigravityBridgeStatus {
    pub running: bool,
    pub config: AntigravityBridgeConfig,
    pub url: String,
    pub account_label: Option<String>,
    pub accounts: Vec<AntigravityBridgeAccount>,
    /// Only a previously explicit discovery, never a network call from status.
    pub models: Vec<AntigravityBridgeModel>,
    pub detail: String,
}

struct Catalog {
    fingerprint: String,
    models: Vec<AntigravityBridgeModel>,
}

struct Session {
    source: Source,
    fingerprint: String,
    project: String,
    user_agent: String,
    client: reqwest::Client,
    models: Vec<AntigravityBridgeModel>,
}

struct Running {
    config: AntigravityBridgeConfig,
    label: String,
    local_token: String,
    session: Session,
    turn: Arc<tokio::sync::Mutex<()>>,
    stop: tokio::sync::watch::Sender<bool>,
}

static BRIDGE: OnceLock<Mutex<Option<Arc<Running>>>> = OnceLock::new();
static CATALOG: OnceLock<Mutex<HashMap<String, Catalog>>> = OnceLock::new();

fn cell() -> &'static Mutex<Option<Arc<Running>>> {
    BRIDGE.get_or_init(Default::default)
}
fn catalog() -> &'static Mutex<HashMap<String, Catalog>> {
    CATALOG.get_or_init(Default::default)
}
fn current() -> Option<Arc<Running>> {
    cell().lock().ok().and_then(|x| x.clone())
}
pub fn running() -> bool {
    current().is_some()
}
pub fn uses_account(id: &str) -> bool {
    current().is_some_and(|r| r.config.account_id == id)
}
fn data_dir() -> PathBuf {
    crate::paths::state_dir()
        .join("plugins")
        .join("antigravity-native-bridge")
}
fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/v1")
}

fn load_config() -> Result<AntigravityBridgeConfig> {
    let Some(bytes) = crate::config_io::read_optional(&data_dir().join("config.json"))? else {
        return Ok(AntigravityBridgeConfig::default());
    };
    serde_json::from_slice(&bytes)
        .map_err(|_| GateError::Other("反重力原生桥配置无法读取；请检查配置文件".into()))
}

fn validate_config(config: &AntigravityBridgeConfig) -> Result<()> {
    if config.port < 1024 || config.system_prompt.len() > 128 * 1024 || config.model.len() > 128 {
        return Err(GateError::Other(
            "端口须为 1024–65535；模型名最多 128 字节，提示词最多 128 KiB".into(),
        ));
    }
    Ok(())
}

pub fn save_config(mut config: AntigravityBridgeConfig) -> Result<()> {
    if running() {
        return Err(GateError::Other(
            "先停止原生桥接再修改账户、模型或端口".into(),
        ));
    }
    config.account_id = config.account_id.trim().to_string();
    config.model = config.model.trim().to_string();
    validate_config(&config)?;
    if !config.account_id.is_empty() {
        account_label(&config.account_id)?;
    }
    std::fs::create_dir_all(data_dir())?;
    crate::config_io::commit(vec![crate::config_io::Edit::text(
        data_dir().join("config.json"),
        serde_json::to_string_pretty(&config)?,
    )?])
    .map(|_| ())
}

pub fn token() -> Result<String> {
    let path = data_dir().join("token.txt");
    if let Some(bytes) = crate::config_io::read_optional(&path)? {
        let text = String::from_utf8_lossy(&bytes).trim().to_string();
        if text.len() >= 32 {
            return Ok(text);
        }
    }
    let text = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    std::fs::create_dir_all(data_dir())?;
    crate::config_io::commit(vec![crate::config_io::Edit::text(path, text.clone())?])?;
    Ok(text)
}

fn accounts() -> Result<Vec<AntigravityBridgeAccount>> {
    Ok(
        qb_accounts::antigravity::account::list(&qb_accounts::antigravity::account::root())?
            .slots
            .into_iter()
            .filter(|a| a.ide_dir.is_some())
            .map(|a| AntigravityBridgeAccount {
                id: a.id,
                label: a.label,
                logged_in: a.ide_logged_in,
            })
            .collect(),
    )
}

fn account_label(id: &str) -> Result<String> {
    qb_accounts::antigravity::account::ide_half(&qb_accounts::antigravity::account::root(), id)?
        .map(|(label, _)| label)
        .ok_or_else(|| {
            GateError::Other(
                "请选择仍在账户列表中的反重力 IDE 槽位；Hub 和 Gemini CLI 登录不适用于此桥".into(),
            )
        })
}

pub fn status() -> Result<AntigravityBridgeStatus> {
    let active = current();
    let config = active
        .as_ref()
        .map(|r| r.config.clone())
        .map(Ok)
        .unwrap_or_else(load_config)?;
    let label = active
        .as_ref()
        .map(|r| r.label.clone())
        .or_else(|| account_label(&config.account_id).ok());
    let models = if let Some(r) = &active {
        r.session.models.clone()
    } else {
        let print = oauth::source_fingerprint(&Source::Account(config.account_id.clone())).ok();
        catalog()
            .lock()
            .unwrap()
            .get(&config.account_id)
            .filter(|c| print.as_deref() == Some(c.fingerprint.as_str()))
            .map(|c| c.models.clone())
            .unwrap_or_default()
    };
    Ok(AntigravityBridgeStatus {
        running: active.is_some(),
        url: url(config.port),
        account_label: label.clone(),
        accounts: accounts()?,
        models,
        detail: if active.is_some() {
            format!(
                "正在使用 IDE 账户「{}」· {} · 官方生产端点",
                label.unwrap_or_default(),
                config.model
            )
        } else {
            "未运行；选择 IDE 账户，手动读取模型后启动。仅使用所选账户，不切换中转站。".into()
        },
        config,
    })
}

fn http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(TURN_TIMEOUT)
        .build()
        .map_err(|_| GateError::Other("创建官方连接失败".into()))
}

fn network_error() -> GateError {
    GateError::Other("连接反重力官方接口失败或超时；没有重试、切换账户或调用中转站".into())
}

fn upstream_error(status: StatusCode) -> GateError {
    let advice = match status.as_u16() {
        401 => "请在选定的反重力 IDE 槽位重新登录",
        403 => "请在官方 IDE 检查账户权限、订阅或验证要求",
        429 => "官方额度或速率受限，请等待恢复；不会切换账户",
        404 => "官方接口或模型不可用，请重新读取模型",
        300..=399 => "官方回复了重定向，桥接未转发凭据",
        _ => "官方未完成请求，请稍后手动重试",
    };
    GateError::Other(format!("反重力官方 HTTP {}：{advice}", status.as_u16()))
}

async fn response_error(mut response: reqwest::Response) -> GateError {
    let status = response.status();
    let mut bytes = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        if bytes.len().saturating_add(chunk.len()) > 64 * 1024 {
            break;
        }
        bytes.extend_from_slice(&chunk);
    }
    let reason = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .as_ref()
        .and_then(protocol::known_error_reason);
    match reason {
        Some(code) => GateError::Other(format!("{}（{code}）", upstream_error(status))),
        None => upstream_error(status),
    }
}

async fn post(
    client: &reqwest::Client,
    host: &str,
    action: &str,
    token: &Secret,
    ua: &str,
    body: &Value,
) -> Result<reqwest::Response> {
    client
        .post(format!("{host}/v1internal:{action}"))
        .bearer_auth(
            token
                .as_str()
                .ok_or_else(|| GateError::Other("官方访问令牌形状无效".into()))?,
        )
        .header(reqwest::header::USER_AGENT, ua)
        .json(body)
        .send()
        .await
        .map_err(|_| network_error())
}

async fn json_body(mut response: reqwest::Response) -> Result<Value> {
    if !response.status().is_success() {
        return Err(response_error(response).await);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| network_error())? {
        if bytes.len().saturating_add(chunk.len()) > MAX_BODY {
            return Err(GateError::Other("官方模型或项目回复过大".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| GateError::Other("官方回复不是有效 JSON".into()))
}

fn check_fingerprint(source: &Source, expected: &str) -> Result<()> {
    if oauth::source_fingerprint(source)? != expected {
        return Err(GateError::Other(
            "选定槽位的登录已变化；请停止原生桥接、重新读取模型后再启动".into(),
        ));
    }
    Ok(())
}

async fn pinned_token(session: &Session, refresh: bool) -> Result<Secret> {
    account_label(match &session.source {
        Source::Account(id) => id,
        Source::Hub => unreachable!(),
    })?;
    check_fingerprint(&session.source, &session.fingerprint)?;
    let token = oauth::access_token(&session.client, &session.source, refresh).await?;
    check_fingerprint(&session.source, &session.fingerprint)?;
    Ok(token)
}

async fn discover(account_id: &str) -> Result<Session> {
    account_label(account_id)?;
    let source = Source::Account(account_id.to_string());
    let fingerprint = oauth::source_fingerprint(&source)?;
    let local = dirs::data_local_dir().unwrap_or_default();
    let version = qb_install::install::antigravity::ide_version(&local);
    let mut session = Session {
        source,
        fingerprint,
        project: String::new(),
        user_agent: oauth::user_agent(version.as_deref()),
        client: http()?,
        models: Vec::new(),
    };
    let mut token = pinned_token(&session, false).await?;
    let payload =
        json!({"metadata":oauth::metadata(version.as_deref()),"mode":"FULL_ELIGIBILITY_CHECK"});
    let mut response = post(
        &session.client,
        HOST,
        "loadCodeAssist",
        &token,
        &session.user_agent,
        &payload,
    )
    .await?;
    if response.status() == StatusCode::UNAUTHORIZED {
        token = pinned_token(&session, true).await?;
        response = post(
            &session.client,
            HOST,
            "loadCodeAssist",
            &token,
            &session.user_agent,
            &payload,
        )
        .await?;
    }
    let loaded = json_body(response).await?;
    session.project = oauth::parse_load(&loaded.to_string())?.project.ok_or_else(|| GateError::Other(
        "所选 IDE 账户尚无可用项目 ID。请先在官方 IDE 完成初始化；桥接不会自动登记账户或更改订阅。".into()
    ))?;
    let response = post(
        &session.client,
        HOST,
        "fetchAvailableModels",
        &token,
        &session.user_agent,
        &json!({"project":session.project}),
    )
    .await?;
    session.models = protocol::models(&json_body(response).await?).map_err(GateError::Other)?;
    check_fingerprint(&session.source, &session.fingerprint)?;
    catalog().lock().unwrap().insert(
        account_id.to_string(),
        Catalog {
            fingerprint: session.fingerprint.clone(),
            models: session.models.clone(),
        },
    );
    Ok(session)
}

/// Explicit one-account discovery. No generation, quota refresh or account onboarding.
pub async fn refresh_models(account_id: &str) -> Result<Vec<AntigravityBridgeModel>> {
    if running() {
        return Err(GateError::Other("先停止原生桥接再重新读取模型".into()));
    }
    Ok(discover(account_id).await?.models)
}

/// Caller serializes starts and enforces the Antigravity IP gate.
pub async fn start() -> Result<AntigravityBridgeStatus> {
    if running() {
        return status();
    }
    let config = load_config()?;
    validate_config(&config)?;
    if config.account_id.is_empty() || config.model.is_empty() {
        return Err(GateError::Other(
            "先选择 IDE 账户和官方返回的模型并保存".into(),
        ));
    }
    let label = account_label(&config.account_id)?;
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, config.port)))
            .await
            .map_err(|_| {
                GateError::Other(format!("无法监听 127.0.0.1:{}；请更换端口", config.port))
            })?;
    let session = discover(&config.account_id).await?;
    if !session.models.iter().any(|m| m.id == config.model) {
        return Err(GateError::Other(
            "所选模型已不在这个账户的官方清单中；请重新读取模型并选择".into(),
        ));
    }
    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    let r = Arc::new(Running {
        config,
        label,
        session,
        local_token: token()?,
        turn: Arc::new(tokio::sync::Mutex::new(())),
        stop,
    });
    *cell().lock().unwrap() = Some(r.clone());
    tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                biased;
                _ = stopped.changed() => break,
                a = listener.accept() => a,
            };
            let Ok((stream, _)) = accepted else {
                continue;
            };
            let r = r.clone();
            tokio::spawn(async move {
                let mut stopped = r.stop.subscribe();
                if *stopped.borrow() {
                    return;
                }
                let svc = service_fn(move |req| handle(req, r.clone()));
                tokio::select! {
                    _ = stopped.changed() => {},
                    _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), svc) => {},
                }
            });
        }
    });
    crate::audit::write("反重力原生桥接已启动：仅所选 IDE 账户与官方生产端点");
    status()
}

pub fn stop() -> Result<()> {
    if let Some(r) = cell().lock().unwrap().take() {
        let _ = r.stop.send(true);
        crate::audit::write("反重力原生桥接已停止");
    }
    Ok(())
}

fn json_response(status: StatusCode, value: Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(value.to_string())).boxed_unsync())
        .unwrap()
}
fn error_response(status: StatusCode, message: impl Into<String>) -> Response<Body> {
    json_response(
        status,
        json!({"error":{"message":message.into(),"type":"qb_gate_antigravity_bridge"}}),
    )
}

async fn handle(
    req: Request<Incoming>,
    r: Arc<Running>,
) -> std::result::Result<Response<Body>, Infallible> {
    if !crate::gpt_bridge::authorized(&req, &r.local_token) {
        return Ok(error_response(
            StatusCode::UNAUTHORIZED,
            "缺少或错误的本机 Bearer token",
        ));
    }
    if *r.stop.borrow() {
        return Ok(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "桥接已停止",
        ));
    }
    match (req.method().as_str(), req.uri().path()) {
        ("GET", "/health") => Ok(json_response(
            StatusCode::OK,
            json!({"status":"ok","backend":"antigravity-native"}),
        )),
        ("GET", "/v1/models") => Ok(json_response(
            StatusCode::OK,
            json!({"object":"list","data":[{"id":r.config.model,"object":"model","owned_by":"antigravity"}]}),
        )),
        ("POST", "/v1/chat/completions") => {
            let bytes = match tokio::time::timeout(
                Duration::from_secs(30),
                Limited::new(req.into_body(), MAX_BODY).collect(),
            )
            .await
            {
                Ok(Ok(body)) => body.to_bytes(),
                _ => {
                    return Ok(error_response(
                        StatusCode::BAD_REQUEST,
                        "请求体读取失败、超时或超过 2 MiB",
                    ))
                }
            };
            let prepared = match protocol::prepare(&bytes, &r.config.model, &r.config.system_prompt)
            {
                Ok(p) => p,
                Err(e) => return Ok(error_response(StatusCode::BAD_REQUEST, e)),
            };
            let guard = match r.turn.clone().try_lock_owned() {
                Ok(g) => g,
                Err(_) => {
                    return Ok(error_response(
                        StatusCode::TOO_MANY_REQUESTS,
                        "原生桥接已有一个请求正在处理，请等它结束",
                    ))
                }
            };
            let response =
                match open_generation(&r.session, &r.config.model, prepared.request).await {
                    Ok(response) => response,
                    Err(e) => return Ok(error_response(StatusCode::BAD_GATEWAY, e.to_string())),
                };
            if prepared.stream {
                let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(8);
                let model = r.config.model.clone();
                let mut stopped = r.stop.subscribe();
                tokio::spawn(async move {
                    let _guard = guard;
                    if *stopped.borrow() {
                        return;
                    }
                    tokio::select! {
                        biased;
                        _ = stopped.changed() => {},
                        _ = tx.closed() => {},
                        _ = stream_response(response, &model, prepared.include_usage, &tx) => {},
                    }
                });
                let stream = futures_util::stream::unfold(rx, |mut rx| async move {
                    rx.recv()
                        .await
                        .map(|bytes| (Ok::<_, Infallible>(Frame::data(bytes)), rx))
                });
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "text/event-stream")
                    .header("cache-control", "no-cache")
                    .body(StreamBody::new(stream).boxed_unsync())
                    .unwrap())
            } else {
                let _guard = guard;
                match collect_response(response).await {
                    Ok(d) => Ok(json_response(
                        StatusCode::OK,
                        completion(&r.config.model, &d),
                    )),
                    Err(e) => Ok(error_response(StatusCode::BAD_GATEWAY, e)),
                }
            }
        }
        _ => Ok(error_response(
            StatusCode::NOT_FOUND,
            "仅支持 /health、/v1/models、/v1/chat/completions",
        )),
    }
}

async fn open_generation(
    session: &Session,
    model: &str,
    request: Value,
) -> Result<reqwest::Response> {
    let body = protocol::envelope(&session.project, model, request);
    let token = pinned_token(session, false).await?;
    let mut response = post(
        &session.client,
        HOST,
        "streamGenerateContent?alt=sse",
        &token,
        &session.user_agent,
        &body,
    )
    .await?;
    if response.status() == StatusCode::UNAUTHORIZED {
        // Only an authentication rejection can retry; never retry a generated turn or 429.
        let token = pinned_token(session, true).await?;
        response = post(
            &session.client,
            HOST,
            "streamGenerateContent?alt=sse",
            &token,
            &session.user_agent,
            &body,
        )
        .await?;
    }
    if !response.status().is_success() {
        return Err(response_error(response).await);
    }
    if !response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/event-stream"))
    {
        return Err(GateError::Other(
            "官方生成接口没有返回 SSE 流；未把未知回复当成成功".into(),
        ));
    }
    Ok(response)
}

async fn collect_response(
    mut response: reqwest::Response,
) -> std::result::Result<protocol::Decoder, String> {
    let mut decoder = protocol::Decoder::default();
    while let Some(bytes) = response.chunk().await.map_err(|_| "官方流读取失败或超时")? {
        decoder.feed(&bytes)?;
    }
    decoder.end()?;
    Ok(decoder)
}

fn completion(model: &str, decoder: &protocol::Decoder) -> Value {
    let mut value = json!({"id":format!("chatcmpl-{}",uuid::Uuid::new_v4()),"object":"chat.completion",
        "created":chrono::Utc::now().timestamp(),"model":model,
        "choices":[{"index":0,"message":{"role":"assistant","content":decoder.text},"finish_reason":decoder.finish}]});
    if !decoder.usage.is_null() {
        value["usage"] = decoder.usage.clone();
    }
    value
}

async fn send_sse(
    tx: &tokio::sync::mpsc::Sender<Bytes>,
    value: Value,
) -> std::result::Result<(), String> {
    tx.send(Bytes::from(format!("data: {value}\n\n")))
        .await
        .map_err(|_| "本机客户端已断开".into())
}

async fn stream_response(
    mut response: reqwest::Response,
    model: &str,
    include_usage: bool,
    tx: &tokio::sync::mpsc::Sender<Bytes>,
) {
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let created = chrono::Utc::now().timestamp();
    let chunk = |delta: Value, finish: Option<&str>| {
        json!({"id":id,"object":"chat.completion.chunk","created":created,"model":model,
        "choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
    };
    let result: std::result::Result<(), String> = async {
        send_sse(tx, chunk(json!({"role":"assistant"}), None)).await?;
        let mut decoder = protocol::Decoder::default();
        while let Some(bytes) = response.chunk().await.map_err(|_| "官方流读取失败或超时")? {
            for text in decoder.feed(&bytes)? { send_sse(tx, chunk(json!({"content":text}), None)).await?; }
        }
        for text in decoder.end()? { send_sse(tx, chunk(json!({"content":text}), None)).await?; }
        send_sse(tx, chunk(json!({}), decoder.finish)).await?;
        if include_usage && !decoder.usage.is_null() {
            send_sse(tx, json!({"id":id,"object":"chat.completion.chunk","created":created,"model":model,"choices":[],"usage":decoder.usage})).await?;
        }
        Ok(())
    }.await;
    if let Err(message) = result {
        let _ = send_sse(
            tx,
            json!({"error":{"message":message,"type":"qb_gate_antigravity_bridge"}}),
        )
        .await;
    }
    let _ = tx.send(Bytes::from_static(b"data: [DONE]\n\n")).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_do_not_select_any_account_or_model() {
        let config: AntigravityBridgeConfig = serde_json::from_str("{}").unwrap();
        assert!(config.account_id.is_empty());
        assert!(config.model.is_empty());
        assert_eq!(url(config.port), "http://127.0.0.1:5004/v1");
        validate_config(&config).unwrap();
    }

    #[test]
    fn official_errors_only_expose_status_and_action() {
        for code in [301, 401, 403, 429, 500] {
            let error = upstream_error(StatusCode::from_u16(code).unwrap()).to_string();
            assert!(error.contains(&format!("HTTP {code}")));
        }
    }

    #[tokio::test]
    async fn mock_wire_request_uses_fixed_envelope_and_does_not_follow_redirects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut input = vec![0u8; 8192];
            let n = stream.read(&mut input).await.unwrap();
            let request = String::from_utf8_lossy(&input[..n]);
            assert!(request.starts_with("POST /v1internal:streamGenerateContent?alt=sse HTTP/1.1"));
            assert!(request.contains("authorization: Bearer fixture-access"));
            assert!(request.contains("\"project\":\"fixture-project\""));
            stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
        });
        let body = protocol::envelope("fixture-project", "gemini-fixture", json!({"contents":[]}));
        let response = post(
            &http().unwrap(),
            &host,
            "streamGenerateContent?alt=sse",
            &Secret::new(b"fixture-access".to_vec()),
            "fixture-agent",
            &body,
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn mock_stream_produces_incremental_openai_chunks_and_usage() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut input = [0; 2048];
            stream.read(&mut input).await.unwrap();
            let body = concat!("data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"first\"}]}}]}}\n\n",
                "data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"second\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":10,\"candidatesTokenCount\":2}}}\n\n");
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        });
        let response = http().unwrap().get(address).send().await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        stream_response(response, "gemini-fixture", true, &tx).await;
        drop(tx);
        let mut frames = Vec::new();
        while let Some(bytes) = rx.recv().await {
            frames.push(String::from_utf8(bytes.to_vec()).unwrap());
        }
        assert!(frames[1].contains("first"));
        assert!(frames[2].contains("second"));
        assert!(frames.iter().any(|x| x.contains("\"prompt_tokens\":10")));
        assert_eq!(frames.last().unwrap(), "data: [DONE]\n\n");
        server.await.unwrap();
    }
}
