// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 中转线路：一发请求的重试循环。
//!
//! 照 cc-switch v4.0.14 `src-tauri/src/proxy/forwarder.rs` 的 `forward_with_retry_inner`
//! （以及它的 `prime_streaming_response` / `validate_responses_stream_start` /
//! `prepare_success_response_for_failover`）写，接到 QB Gate 自己的熔断、错误分类和日志上。
//!
//! # 一发请求怎么走
//!
//! 对派单顺序里的每一条线（当前那条打头，见 [`RouterState::candidates`]）：
//!
//! 1. 试满 [`RouterSettings::max_attempts`] 条就停；「这条线没有这个模型」的 6 小时内跳过。
//! 2. 向熔断器要放行（半开只放一发）；只有一条线时不问（cc-switch 同样如此）。
//! 3. 按这条线的模型映射改请求体，确认纯文本的模型先把图片换成标记。
//! 4. 发出去：
//!    - 非流式在非流式超时内**整份读完**，2xx 里藏着的错误信封在这里认出来；
//!    - 流式在首包超时内等到第一个**有内容的**事件（Anthropic 的 `message_start`、Responses 的
//!      输出事件）再交出流 —— 交出去之前出的错还能换线，交出去之后就只能如实断开。
//! 5. 失败了：先看整流器能不能改一下请求再打**同一条线**一次（图片降级、别家的加密状态、
//!    thinking 签名、thinking 预算），再按 [`qb_proxy::classify`] 定三件事 —— 换不换下一条、
//!    算不算这条线的故障、记进熔断的哪一档。
//!
//! 全部失败时回**最后一条线**的原话（状态码、头、体都是上游的）；连不上 / 超时这种没有原话的，
//! 回 cc-switch 那种 `{"error":{...}}`。
//!
//! # 跟 cc-switch 不一样的地方
//!
//! - 只有一条线时它不碰熔断；这里**照样记**（QB Gate 原来就记，排序要用）。
//! - 所有线都在熔断时它回 503；这里**仍打当前那一条**：原来就是这样，不凭空制造死局。
//! - 自动故障转移默认开（使用者定的）；关掉就只打当前那一条。
//! - 转给中转线路的请求一律 `accept-encoding: identity`：要从响应里读用量、认出 2xx 里的错，
//!   而 reqwest 没开解压（⛔ 不许开 `gzip`，开了它会悄悄改写流）。cc-switch 只在 SSE 路径这么做。
//! - 404 / 409 / 451 换线但不伤熔断（`Fault::NotServedHere`）；cc-switch 是可重试且记失败。

use super::log::{fill_usage, parse_body_usage, Record, StreamWatch};
use super::server::Inbound;
use super::*;
use futures_util::{Stream, StreamExt};
use http_body_util::StreamBody;
use hyper::body::Frame;
use qb_proxy::classify::{self, BreakerVerdict, Fault};
use qb_proxy::proxy_error::{describe_error_chain, ProxyError};
use qb_proxy::sse_text::{append_utf8_safe, take_sse_block};
use qb_proxy::types::RectifierConfig;
use qb_proxy::{
    envelope, media_sanitizer, opaque_state_rectifier, session, thinking_budget_rectifier,
    thinking_rectifier,
};
use qb_station::router::{RectifierSwitches, RouteAttempt};
use qb_station::schedule::Permit;
use serde_json::Value;
use std::pin::Pin;
use std::time::{Duration, Instant};

/// 上游响应体最多读多少（非流式整份读、错误体）。cc-switch 的 `MAX_RESPONSE_BODY_BYTES`。
const MAX_BODY_BYTES: usize = 128 * 1024 * 1024;

/// 流式响应在交出去之前最多压多少字节等「第一个有内容的事件」。cc-switch 的 `MAX_PRIME_BYTES`。
/// 超了就照原样交出去 —— 宁可少认一次错，也不让客户端一直等。
const MAX_PRIME_BYTES: usize = 256 * 1024;

/// 第一个字节到了之后，最多再压多久等「第一个有内容的事件」。cc-switch 在
/// `hold_encrypted_agent_task_stream` 里用的也是 10 秒（`MAX_HOLD`）。
const MAX_HOLD: Duration = Duration::from_secs(10);

/// 只有一条线、或者熔断不参与时用的空名额。
const FREE: Permit = Permit {
    allowed: true,
    half_open: false,
};

type UpstreamStream = Pin<Box<dyn Stream<Item = reqwest::Result<Bytes>> + Send>>;

/// 发给一条线的请求体。没改动时就是客户端原来那串字节（不重新序列化）。
#[derive(Clone)]
struct Outgoing {
    bytes: Bytes,
    /// 能解成 JSON 对象时才有。整流器、模型映射都只动它。
    json: Option<Value>,
}

impl Outgoing {
    fn from_json(value: Value, fallback: &Bytes) -> Self {
        let bytes = serde_json::to_vec(&value)
            .map(Bytes::from)
            .unwrap_or_else(|_| fallback.clone());
        Self {
            bytes,
            json: Some(value),
        }
    }
}

/// 一条线一次的结果。
enum Attempt {
    Done(Success),
    Failed(Failure),
}

struct Success {
    status: u16,
    headers: reqwest::header::HeaderMap,
    first_ms: Option<u64>,
    body: SuccessBody,
}

enum SuccessBody {
    /// 整份读完的（非流式，或者流式请求却回了 JSON）。
    Whole(Bytes),
    /// 流：交出去之前已经读出来的几块 + 剩下的。
    Stream {
        primed: Vec<Bytes>,
        rest: UpstreamStream,
    },
}

struct Failure {
    error: ProxyError,
    retry_after_ms: Option<i64>,
    /// 最后一条也失败时回给客户端的东西。
    reply: Reply,
}

enum Reply {
    /// 上游的原话：状态码、头、体原样转回。
    Upstream {
        status: u16,
        headers: reqwest::header::HeaderMap,
        body: Bytes,
    },
    /// 没有原话（连不上、超时）：按 [`ProxyError::response_parts`] 回 JSON。
    Local,
}

impl Failure {
    fn local(error: ProxyError) -> Attempt {
        Attempt::Failed(Self {
            error,
            retry_after_ms: None,
            reply: Reply::Local,
        })
    }
}

/// 一发请求走中转线路。`first` 是 [`server`](super::server) 绑到的当前那条。
pub(super) async fn relay(
    state: &Arc<Mutex<RouterState>>,
    http: &reqwest::Client,
    inbound: Inbound,
    first: Upstream,
    mut record: Record,
) -> Response<ProxyBody> {
    let caller = inbound.caller;
    let model = record.log.model.clone();
    let (settings, mut lines) = match state.lock() {
        Ok(s) => (s.settings(caller), s.candidates(caller, &first)),
        Err(_) => return text(StatusCode::INTERNAL_SERVER_ERROR, "本机路由内部状态损坏"),
    };
    // 缺模型的线 6 小时内跳过；全都缺就一条不跳 —— 宁可让它自己报错，也不凭空制造死局。
    if let Ok(s) = state.lock() {
        let kept: Vec<Upstream> = lines
            .iter()
            .filter(|u| !s.model_missing(&u.route_id, &model, inbound.now_ms))
            .cloned()
            .collect();
        if !kept.is_empty() {
            lines = kept;
        }
    }

    let json: Option<Value> = serde_json::from_slice::<Value>(&inbound.body)
        .ok()
        .filter(Value::is_object);
    let streaming = is_streaming(&inbound, json.as_ref());
    record.log.streaming = streaming;
    if let Some(body) = &json {
        let format = if is_claude(caller) { "claude" } else { "codex" };
        let found = session::extract_session_id(&inbound.headers, body, format);
        // 只记客户端自己带来的。生成出来的那种每发都不一样，记了只会把一段对话拆成几百段。
        record.log.session_id = found.client_provided.then_some(found.session_id);
    }
    let rectifier = rectifier_config(settings.rectifier);

    // 只有一条线时不问熔断（cc-switch 同样如此）：问了，熔断着的那一条也只能是它。
    let bypass = lines.len() == 1;
    let max_attempts = settings.max_attempts.max(1) as usize;
    let mut attempted = 0usize;
    let mut last: Option<(Failure, Fault)> = None;

    let mut queue: Vec<(Upstream, Permit)> = Vec::new();
    for line in lines.iter() {
        let permit = if bypass {
            FREE
        } else {
            match state.lock() {
                Ok(mut s) => s
                    .breaker_mut(&line.route_id, caller)
                    .allow(chrono::Utc::now().timestamp_millis()),
                Err(_) => FREE,
            }
        };
        if permit.allowed {
            queue.push((line.clone(), permit));
            // 只占够要用的名额：后面的线到要试的时候再问（见下面循环里的 `take_permit`）。
            break;
        }
    }
    // 所有线都在熔断：仍打当前那一条（原来就是这样，不凭空回 503）。
    if queue.is_empty() {
        queue.push((first.clone(), FREE));
    }
    let mut next_index = lines
        .iter()
        .position(|u| u.route_id == queue[0].0.route_id)
        .map_or(lines.len(), |i| i + 1);

    while let Some((line, permit)) = queue.pop() {
        if attempted >= max_attempts {
            if let Ok(mut s) = state.lock() {
                s.breaker_mut(&line.route_id, caller).release(permit);
            }
            break;
        }
        attempted += 1;
        record.log.route_id = line.route_id.clone();

        let (mut body, upstream_model) = line_body(json.as_ref(), &inbound.body, &line, &rectifier);
        record.log.upstream_model = upstream_model;

        let tried = Instant::now();
        let mut result = attempt(http, &inbound, &line, &body, &settings, streaming).await;

        // 整流：改一下请求再打同一条线一次。不换线、不伤熔断；每条线最多一次。
        if let Attempt::Failed(failure) = &result {
            if let Some((fixed, why)) =
                rectify(caller, &inbound.path, &body, &failure.error, &rectifier)
            {
                tracing::info!(
                    "本机路由：{} 因为请求形状被拒（{}），{why}之后对同一条线重试一次",
                    line.route_id,
                    failure.error
                );
                record.log.attempts.push(RouteAttempt {
                    route_id: line.route_id.clone(),
                    status: status_of(failure),
                    fault: Some(format!("rectify-{why}")),
                    ms: tried.elapsed().as_millis() as u64,
                });
                body = fixed;
                result = attempt(http, &inbound, &line, &body, &settings, streaming).await;
            }
        }

        match result {
            Attempt::Done(ok) => {
                record.log.attempts.push(RouteAttempt {
                    route_id: line.route_id.clone(),
                    status: Some(ok.status),
                    fault: None,
                    ms: tried.elapsed().as_millis() as u64,
                });
                record.log.fault = None;
                record.log.error = None;
                return respond_ok(state, caller, &line, permit, ok, record, &settings);
            }
            Attempt::Failed(failure) => {
                let class = classify::classify(&failure.error, failure.retry_after_ms);
                record.log.attempts.push(RouteAttempt {
                    route_id: line.route_id.clone(),
                    status: status_of(&failure),
                    fault: Some(class.fault.key().to_string()),
                    ms: tried.elapsed().as_millis() as u64,
                });
                let now_ms = chrono::Utc::now().timestamp_millis();
                if let Ok(mut s) = state.lock() {
                    if class.fault == Fault::ModelMissing {
                        s.mark_model_missing(&line.route_id, &model, now_ms);
                    }
                    let breaker = s.breaker_mut(&line.route_id, caller);
                    match outcome_of(class.breaker) {
                        Some(outcome) => breaker.record_after(permit, outcome, now_ms),
                        None => breaker.release(permit),
                    }
                }
                tracing::warn!(
                    "本机路由：{} 失败（{}）：{}",
                    line.route_id,
                    class.fault.label(),
                    failure.error
                );
                let go_on = class.retry_next && settings.auto_failover;
                last = Some((failure, class.fault));
                if !go_on {
                    break;
                }
                // 下一条：按顺序往后找第一条熔断放行的。
                while next_index < lines.len() {
                    let candidate = lines[next_index].clone();
                    next_index += 1;
                    let permit = match state.lock() {
                        Ok(mut s) => s
                            .breaker_mut(&candidate.route_id, caller)
                            .allow(chrono::Utc::now().timestamp_millis()),
                        Err(_) => FREE,
                    };
                    if permit.allowed {
                        queue.push((candidate, permit));
                        break;
                    }
                }
            }
        }
    }

    let Some((failure, fault)) = last else {
        return text(
            StatusCode::SERVICE_UNAVAILABLE,
            "本机路由没能把这一发转出去：这个软件的线路都试过了。",
        );
    };
    respond_failure(failure, fault, record)
}

/// 这一发是不是流式。cc-switch 的 `is_streaming_request`。
fn is_streaming(inbound: &Inbound, json: Option<&Value>) -> bool {
    if json
        .and_then(|v| v.get("stream"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    if inbound.path.contains("streamGenerateContent") || inbound.path.contains("alt=sse") {
        return true;
    }
    inbound
        .headers
        .get(hyper::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| accept.contains("text/event-stream"))
}

fn is_claude(caller: Client) -> bool {
    matches!(caller, Client::ClaudeCode | Client::ClaudeDesktop)
}

/// 原样转发的 Responses 端点（不看查询串）。
fn is_responses(path: &str) -> bool {
    path.split('?')
        .next()
        .unwrap_or(path)
        .trim_end_matches('/')
        .ends_with("/responses")
}

fn rectifier_config(s: RectifierSwitches) -> RectifierConfig {
    RectifierConfig {
        enabled: s.enabled,
        request_thinking_signature: s.thinking_signature,
        request_thinking_budget: s.thinking_budget,
        request_media_fallback: s.media_fallback,
        request_media_heuristic: s.media_heuristic,
    }
}

/// 熔断那一档换成 `qb-station` 的结果。`Neutral` 只还名额、不记账。
fn outcome_of(v: BreakerVerdict) -> Option<Outcome> {
    match v {
        BreakerVerdict::Ok => Some(Outcome::Ok),
        BreakerVerdict::RateLimited { retry_after_ms } => {
            Some(Outcome::RateLimited { retry_after_ms })
        }
        BreakerVerdict::Rejected => Some(Outcome::Rejected),
        BreakerVerdict::ServerError => Some(Outcome::ServerError),
        BreakerVerdict::Neutral => None,
    }
}

fn status_of(failure: &Failure) -> Option<u16> {
    match (&failure.reply, &failure.error) {
        (Reply::Upstream { status, .. }, _) => Some(*status),
        (_, ProxyError::UpstreamError { status, .. }) => Some(*status),
        _ => None,
    }
}

/// 发给这条线的请求体：模型映射、确认纯文本的模型先把图片换成标记。返回 `(请求体, 上游模型)`。
fn line_body(
    json: Option<&Value>,
    raw: &Bytes,
    line: &Upstream,
    cfg: &RectifierConfig,
) -> (Outgoing, String) {
    let Some(original) = json else {
        return (
            Outgoing {
                bytes: raw.clone(),
                json: None,
            },
            String::new(),
        );
    };
    let mut value = original.clone();
    let mut changed = false;
    let mut upstream_model = String::new();
    // 先看逐个指定的，再看 cc-switch 那几档（`Upstream::map_model`）。只改顶层 `model` 一个字段。
    let requested = value
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(mapped) = requested.as_deref().and_then(|m| line.map_model(m)) {
        tracing::debug!(
            "本机路由：{} 上的模型 {:?} → {mapped}",
            line.route_id,
            requested
        );
        value["model"] = Value::String(mapped.clone());
        upstream_model = mapped;
        changed = true;
    }
    if cfg.enabled
        && cfg.request_media_fallback
        && media_sanitizer::replace_images_for_text_only_model(
            &mut value,
            &Value::Null,
            cfg.request_media_heuristic,
        ) > 0
    {
        changed = true;
    }
    if changed {
        (Outgoing::from_json(value, raw), upstream_model)
    } else {
        (
            Outgoing {
                bytes: raw.clone(),
                json: Some(value),
            },
            upstream_model,
        )
    }
}

/// 上游因为请求形状拒绝时，改一下请求再打同一条线。返回改好的请求和改了什么；不适用就是 `None`。
///
/// 顺序照 cc-switch：图片降级 → 别家签发的加密状态 → thinking 签名 → thinking 预算。
fn rectify(
    caller: Client,
    path: &str,
    body: &Outgoing,
    error: &ProxyError,
    cfg: &RectifierConfig,
) -> Option<(Outgoing, &'static str)> {
    let json = body.json.as_ref()?;
    if cfg.enabled
        && cfg.request_media_fallback
        && media_sanitizer::contains_image_blocks(json)
        && media_sanitizer::is_unsupported_image_error(error)
    {
        let mut v = json.clone();
        if media_sanitizer::replace_image_blocks_with_marker(&mut v) > 0 {
            return Some((Outgoing::from_json(v, &body.bytes), "media"));
        }
    }
    if caller == Client::Codex && is_responses(path) {
        // 中转线路对 Codex 来说永远是「第三方」：它收到的压缩密文多半是官方签发的。
        if let Some(rejection) =
            opaque_state_rectifier::detect_opaque_state_rejection(error, cfg, json, true)
        {
            let mut v = json.clone();
            if opaque_state_rectifier::rectify_opaque_state(&mut v, rejection).applied {
                return Some((Outgoing::from_json(v, &body.bytes), "opaque-state"));
            }
        }
    }
    if is_claude(caller) {
        let message = match error {
            ProxyError::UpstreamError { body, .. } => body.clone(),
            other => Some(other.to_string()),
        };
        if thinking_rectifier::should_rectify_thinking_signature(message.as_deref(), cfg) {
            let mut v = json.clone();
            if thinking_rectifier::rectify_anthropic_request(&mut v).applied {
                return Some((Outgoing::from_json(v, &body.bytes), "thinking-signature"));
            }
        }
        if thinking_budget_rectifier::should_rectify_thinking_budget(message.as_deref(), cfg) {
            let mut v = json.clone();
            if thinking_budget_rectifier::rectify_thinking_budget(&mut v).applied {
                return Some((Outgoing::from_json(v, &body.bytes), "thinking-budget"));
            }
        }
    }
    None
}

/// 转给中转线路的头：丢逐跳头、丢客户端自带的凭证换上这条线自己的，`accept-encoding` 换成 identity。
fn relay_headers(incoming: &hyper::HeaderMap, auth: &UpstreamAuth) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in incoming {
        if forwardable(name.as_str()) && name != reqwest::header::ACCEPT_ENCODING {
            headers.insert(name.clone(), value.clone());
        }
    }
    headers.insert(
        reqwest::header::ACCEPT_ENCODING,
        reqwest::header::HeaderValue::from_static("identity"),
    );
    match auth {
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
        // 官方线不会走到这里（见 `server`）；`None` 不注入。
        UpstreamAuth::None | UpstreamAuth::OAuthPassthrough => {}
    }
    headers
}

/// reqwest 发送失败换成路由的错误。cc-switch 的 `map_reqwest_send_error`
/// （`proxy_error.rs` 里写着它挪到了这里：那一层不认识 reqwest）。
///
/// ⛔ 先 `without_url()`：reqwest 的错误文字里带着整个请求地址（含查询串），这句话会进日志、
/// 回给客户端；有的上游把 Key 放在查询串里（Gemini 的 `?key=`），不剥就把 Key 写进了日志。
pub(super) fn describe_send_error(e: reqwest::Error) -> ProxyError {
    let timeout = e.is_timeout();
    let connect = e.is_connect();
    let chain = describe_error_chain(&e.without_url());
    if timeout {
        return ProxyError::Timeout(chain);
    }
    if connect {
        return ProxyError::ForwardFailed(format!("连不上上游：{chain}"));
    }
    ProxyError::ForwardFailed(chain)
}

fn secs(n: u32) -> Option<Duration> {
    (n > 0).then(|| Duration::from_secs(u64::from(n)))
}

/// 打一条线一次。
async fn attempt(
    http: &reqwest::Client,
    inbound: &Inbound,
    line: &Upstream,
    body: &Outgoing,
    settings: &RouterSettings,
    streaming: bool,
) -> Attempt {
    let url = join_url(&line.base_url, &inbound.path);
    let mut request = http
        .request(inbound.method.clone(), &url)
        .headers(relay_headers(&inbound.headers, &line.auth))
        .body(body.bytes.clone());
    let non_streaming = secs(settings.non_streaming_timeout_secs);
    let first_byte = secs(settings.first_byte_timeout_secs);
    if !streaming {
        // reqwest 的 timeout 管整个请求（含读完响应体）；流式的交给下面的首包 / 静默超时。
        if let Some(t) = non_streaming {
            request = request.timeout(t);
        }
    }
    let send = request.send();
    let sent = match (streaming, first_byte) {
        (true, Some(t)) => match tokio::time::timeout(t, send).await {
            Ok(r) => r,
            Err(_) => {
                return Failure::local(ProxyError::Timeout(format!(
                    "流式响应首包超时: {}s（上游未返回响应头）",
                    t.as_secs()
                )))
            }
        },
        _ => send.await,
    };
    let mut resp = match sent {
        Ok(r) => r,
        Err(e) => return Failure::local(describe_send_error(e)),
    };

    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let retry = retry_after_ms(
        headers
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok()),
    );
    let mut first_ms = None;

    if !(200..300).contains(&status) {
        let body = match read_whole(&mut resp, inbound.started, &mut first_ms, non_streaming).await
        {
            Ok(b) => b,
            Err(e) => return Failure::local(e),
        };
        return Attempt::Failed(Failure {
            error: ProxyError::UpstreamError {
                status,
                body: readable(&headers, &body),
            },
            retry_after_ms: retry,
            reply: Reply::Upstream {
                status,
                headers,
                body,
            },
        });
    }

    let is_json = headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.contains("json"));
    if streaming && !is_json {
        return prime(resp, inbound, first_byte).await;
    }

    // 非流式（或者流式请求却回了一整份 JSON）：读完再交，2xx 里的错误信封在这里认出来。
    let body = match read_whole(&mut resp, inbound.started, &mut first_ms, non_streaming).await {
        Ok(b) => b,
        Err(e) => return Failure::local(e),
    };
    if let Some(text) = readable(&headers, &body) {
        let hidden = if is_claude(inbound.caller) {
            envelope::anthropic_error_envelope_message(text.as_bytes())
                .map(|m| format!("Anthropic upstream returned a 2xx error envelope: {m}"))
        } else if is_responses(&inbound.path) {
            envelope::responses_error_envelope_message(text.as_bytes())
                .map(|m| format!("Responses upstream returned a 2xx failure: {m}"))
                .or_else(|| whole_sse_failure(&text))
        } else {
            whole_sse_failure(&text)
        };
        if let Some(message) = hidden {
            return Attempt::Failed(Failure {
                error: ProxyError::TransformError(message),
                retry_after_ms: None,
                reply: Reply::Upstream {
                    status,
                    headers,
                    body,
                },
            });
        }
    }
    Attempt::Done(Success {
        status,
        headers,
        first_ms,
        body: SuccessBody::Whole(body),
    })
}

/// Codex 没要流（请求里没写 `stream:true`）、上游却回了一整段 SSE：整段扫一遍结局
/// （[`qb_station::sse::StreamScanner`]），流里报的失败照样在重试循环里认出来、还能换线。
/// 原来这种情况是在流结束时补记熔断（那时已经交出去了）。
///
/// 返回的说法要让 [`classify::classify_message`] 认得出是哪一类（限流 / 过载 / 上游失败）。
fn whole_sse_failure(text: &str) -> Option<String> {
    use qb_station::sse::{StreamOutcome, StreamScanner};
    let head = text.trim_start();
    if !(head.starts_with("data:") || head.starts_with("event:")) {
        return None;
    }
    let mut scanner = StreamScanner::new();
    scanner.feed(text.as_bytes());
    match scanner.finish() {
        StreamOutcome::Failed => {
            Some("Responses stream reported a failure (upstream request failed)".to_string())
        }
        StreamOutcome::Capacity => {
            Some("Responses stream reported that the server is overloaded".to_string())
        }
        StreamOutcome::RateLimited => {
            Some("Responses stream reported a rate limit or exhausted quota".to_string())
        }
        StreamOutcome::Completed | StreamOutcome::Incomplete => None,
    }
}

/// 响应体能不能当文字读（没压缩、是 UTF-8）。读不了就是 `None`，分类只看状态码。
fn readable(headers: &reqwest::header::HeaderMap, body: &Bytes) -> Option<String> {
    let encoded = headers
        .get(reqwest::header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|e| !e.trim().eq_ignore_ascii_case("identity"));
    if encoded {
        return None;
    }
    String::from_utf8(body.to_vec()).ok()
}

/// 整份读完（有上限）。`deadline` 只给 reqwest 没管着的那种（流式请求回了 JSON）用。
async fn read_whole(
    resp: &mut reqwest::Response,
    started: Instant,
    first_ms: &mut Option<u64>,
    deadline: Option<Duration>,
) -> std::result::Result<Bytes, ProxyError> {
    let read = async {
        let mut buf: Vec<u8> = Vec::new();
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    first_ms.get_or_insert(started.elapsed().as_millis() as u64);
                    if buf.len() + chunk.len() > MAX_BODY_BYTES {
                        return Err(ProxyError::ResponseBodyTooLarge(MAX_BODY_BYTES));
                    }
                    buf.extend_from_slice(&chunk);
                }
                Ok(None) => return Ok(Bytes::from(buf)),
                Err(e) if e.is_timeout() => {
                    return Err(ProxyError::Timeout(format!(
                        "响应体读取超时（上游发完响应头后 body 没到齐）：{}",
                        describe_error_chain(&e.without_url())
                    )))
                }
                Err(e) => {
                    return Err(ProxyError::ForwardFailed(format!(
                        "读上游响应时断了：{}",
                        describe_error_chain(&e.without_url())
                    )))
                }
            }
        }
    };
    match deadline {
        Some(t) => tokio::time::timeout(t, read).await.unwrap_or_else(|_| {
            Err(ProxyError::Timeout(format!(
                "响应体读取超时: {}s（上游发完响应头后 body 没到齐）",
                t.as_secs()
            )))
        }),
        None => read.await,
    }
}

/// 流式：在首包超时内等到第一个**有内容的**事件再交出流。
///
/// - Anthropic：第一个不是 `ping` 的事件；是 `error` 就当失败（还能换线）。
/// - Responses：第一个不是生命周期（`created` / `in_progress` / `queued`）的事件；
///   `response.failed` / `error` 就当失败。网关不理 `stream:true`、回一整份 JSON 的也认得出来。
/// - 其余（Chat Completions）：等到第一块就交。
///
/// # 两道截止时间
///
/// - **第一个字节**：首包超时内一个字节都没有 → 超时，还能换线（cc-switch 的 `prime_streaming_response`）。
/// - **有内容的事件**：第一个字节到了之后最多再压 [`MAX_HOLD`]，还没见到就把读到的照原样交出去。
///   cc-switch 对原样转发的 Codex 只等首包、不看内容；这里多看一眼是为了在交出去之前认出
///   「一开头就报错」的那种（那种错是上游一收到请求就报的，不用等生成），但不能因此拖住
///   推理很久才出第一段的长回合 —— 所以压得住的只有这几秒，不是整个首包超时。
async fn prime(
    resp: reqwest::Response,
    inbound: &Inbound,
    first_byte: Option<Duration>,
) -> Attempt {
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let anthropic = is_claude(inbound.caller);
    let responses = !anthropic && is_responses(&inbound.path);
    let mut deadline = first_byte.map(|t| tokio::time::Instant::now() + t);
    let mut stream: UpstreamStream = Box::pin(resp.bytes_stream());
    let mut primed: Vec<Bytes> = Vec::new();
    let mut held = 0usize;
    let mut first_ms = None;
    let mut buffer = String::new();
    let mut remainder = Vec::new();

    let reply_with = |primed: &[Bytes]| Reply::Upstream {
        status,
        headers: headers.clone(),
        body: Bytes::from(primed.concat()),
    };

    loop {
        let next = match deadline {
            Some(d) => match tokio::time::timeout_at(d, stream.next()).await {
                Ok(next) => next,
                Err(_) if primed.is_empty() => {
                    return Failure::local(ProxyError::Timeout(format!(
                        "流式响应首包超时: {}s（上游已返回响应头但未返回数据）",
                        first_byte.map_or(0, |t| t.as_secs())
                    )))
                }
                // 有数据、只是还没等到有内容的事件（一直是心跳 / 生命周期事件）：照原样交出去。
                Err(_) => break,
            },
            None => stream.next().await,
        };
        let chunk = match next {
            None => {
                // 流在认出结局之前就结束了。
                if primed.is_empty() {
                    return Failure::local(ProxyError::ForwardFailed(
                        "流式响应在首包到达前结束".to_string(),
                    ));
                }
                let tail = buffer.trim().to_string();
                let verdict = if responses {
                    envelope::inspect_responses_json_document(&tail).or_else(|| {
                        (!tail.is_empty())
                            .then(|| envelope::inspect_responses_start_event(&tail))
                            .flatten()
                    })
                } else if anthropic && !tail.is_empty() {
                    envelope::inspect_anthropic_start_event(&tail)
                } else {
                    None
                };
                match verdict {
                    Some(Err(error)) => {
                        return Attempt::Failed(Failure {
                            error,
                            retry_after_ms: None,
                            reply: reply_with(&primed),
                        })
                    }
                    None if responses => return Attempt::Failed(Failure {
                        error: ProxyError::ForwardFailed(
                            "Responses stream ended before producing output or a terminal event"
                                .to_string(),
                        ),
                        retry_after_ms: None,
                        reply: reply_with(&primed),
                    }),
                    _ => break,
                }
            }
            Some(Err(e)) => {
                return Failure::local(ProxyError::ForwardFailed(format!(
                    "读取流式响应失败: {}",
                    describe_error_chain(&e.without_url())
                )))
            }
            Some(Ok(chunk)) => chunk,
        };
        if first_ms.is_none() {
            first_ms = Some(inbound.started.elapsed().as_millis() as u64);
            // 第一个字节到了：再压最多 MAX_HOLD 等有内容的事件（见函数头）。
            let hold = tokio::time::Instant::now() + MAX_HOLD;
            deadline = Some(deadline.map_or(hold, |d| d.min(hold)));
        }
        held += chunk.len();
        append_utf8_safe(&mut buffer, &mut remainder, &chunk);
        primed.push(chunk);
        if !anthropic && !responses {
            break;
        }

        // 网关不理 `stream:true`、回了一整份 JSON（没写 JSON 的 content-type）。
        let mut verdict = if responses {
            envelope::inspect_responses_json_document(&buffer)
        } else {
            None
        };
        while verdict.is_none() {
            let Some(block) = take_sse_block(&mut buffer) else {
                break;
            };
            verdict = if responses {
                envelope::inspect_responses_start_event(&block)
            } else {
                envelope::inspect_anthropic_start_event(&block)
            };
        }
        match verdict {
            Some(Ok(())) => break,
            Some(Err(error)) => {
                return Attempt::Failed(Failure {
                    error,
                    retry_after_ms: None,
                    reply: reply_with(&primed),
                })
            }
            None if held >= MAX_PRIME_BYTES => {
                tracing::warn!(
                    "本机路由：流式响应等了 {MAX_PRIME_BYTES} 字节还没见到内容，照原样交出去"
                );
                break;
            }
            None => {}
        }
    }

    Attempt::Done(Success {
        status,
        headers,
        first_ms,
        body: SuccessBody::Stream {
            primed,
            rest: stream,
        },
    })
}

/// 成功：记熔断、填日志、把响应交出去。
fn respond_ok(
    state: &Arc<Mutex<RouterState>>,
    caller: Client,
    line: &Upstream,
    permit: Permit,
    ok: Success,
    mut record: Record,
    settings: &RouterSettings,
) -> Response<ProxyBody> {
    let Success {
        status,
        headers,
        first_ms,
        body,
    } = ok;
    record.log.status = Some(status);
    record.log.first_token_ms = first_ms;
    record.log.retry_after_ms = None;
    let now_ms = chrono::Utc::now().timestamp_millis();

    let mut out = Response::builder().status(status);
    for (name, value) in &headers {
        if forwardable(name.as_str()) {
            out = out.header(name, value);
        }
    }

    let body = match body {
        SuccessBody::Whole(bytes) => {
            if let Some(usage) = parse_body_usage(caller, &bytes) {
                fill_usage(&mut record.log, &usage);
            }
            if let Ok(mut s) = state.lock() {
                s.breaker_mut(&line.route_id, caller)
                    .record_after(permit, Outcome::Ok, now_ms);
            }
            drop(record);
            Full::new(bytes).map_err(|e| match e {}).boxed_unsync()
        }
        SuccessBody::Stream { primed, rest } => {
            // ⛔ Codex 的流:健康判定推迟到流结束（HTTP 200 的 SSE 里会报 `response.failed`），
            // 名额也到那时才还。Claude 的流照旧在交出去时按状态码记「好」。
            let defer = caller == Client::Codex;
            let mut watch = StreamWatch::new(caller);
            if defer {
                watch = watch.defer_health(&line.route_id, permit);
            } else if let Ok(mut s) = state.lock() {
                s.breaker_mut(&line.route_id, caller)
                    .record_after(permit, Outcome::Ok, now_ms);
            }
            stream_body(
                primed,
                rest,
                watch,
                record,
                secs(settings.idle_timeout_secs),
            )
        }
    };
    match out.body(body) {
        Ok(r) => r,
        Err(e) => text(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("拼不出响应：{e}"),
        ),
    }
}

/// 全部失败：回最后一条线的原话；没有原话的回 JSON。
fn respond_failure(failure: Failure, fault: Fault, mut record: Record) -> Response<ProxyBody> {
    let Failure {
        error,
        retry_after_ms,
        reply,
    } = failure;
    record.log.fault = Some(fault.key().to_string());
    record.log.retry_after_ms = retry_after_ms;
    // 一句话摘要：**不含请求内容、不含 Key**。上游的错误体可能把请求里的东西回显出来，只留状态码。
    record.log.error = Some(match &error {
        ProxyError::UpstreamError { status, .. } => format!("{}（HTTP {status}）", fault.label()),
        other => {
            let mut s = other.to_string();
            if s.chars().count() > 300 {
                s = s.chars().take(300).collect::<String>() + "…";
            }
            format!("{}：{s}", fault.label())
        }
    });
    match reply {
        Reply::Upstream {
            status,
            headers,
            body,
        } => {
            record.log.status = Some(status);
            let mut out = Response::builder().status(status);
            for (name, value) in &headers {
                if forwardable(name.as_str()) {
                    out = out.header(name, value);
                }
            }
            drop(record);
            match out.body(Full::new(body).map_err(|e| match e {}).boxed_unsync()) {
                Ok(r) => r,
                Err(e) => text(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("拼不出响应：{e}"),
                ),
            }
        }
        Reply::Local => {
            let (status, json) = error.response_parts();
            record.log.status = Some(status);
            drop(record);
            let mut r = Response::new(
                Full::new(Bytes::from(json.to_string()))
                    .map_err(|e| match e {})
                    .boxed_unsync(),
            );
            *r.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
            r.headers_mut().insert(
                hyper::header::CONTENT_TYPE,
                hyper::header::HeaderValue::from_static("application/json"),
            );
            r
        }
    }
}

/// 把上游的流接给客户端：边转边看（用量、Codex 的结局），两块之间超过 `idle` 就断开。
///
/// `record` 被搬进流里：**流被丢掉的那一刻**才是这一发请求真的结束了（`Record` 的 Drop 收尾）。
pub(super) fn stream_body<S>(
    primed: Vec<Bytes>,
    rest: S,
    watch: StreamWatch,
    mut record: Record,
    idle: Option<Duration>,
) -> ProxyBody
where
    S: Stream<Item = reqwest::Result<Bytes>> + Send + 'static,
{
    let watch = Arc::new(Mutex::new(watch));
    if let Ok(mut w) = watch.lock() {
        for chunk in &primed {
            w.feed(chunk);
        }
    }
    record.watch = Some(Arc::clone(&watch));

    let head = futures_util::stream::iter(
        primed
            .into_iter()
            .map(|b| Ok::<_, std::io::Error>(Frame::data(b))),
    );
    let rest = futures_util::stream::unfold(
        (Box::pin(rest), watch, false),
        move |(mut stream, watch, done)| async move {
            if done {
                return None;
            }
            let next = match idle {
                Some(limit) => match tokio::time::timeout(limit, stream.next()).await {
                    Ok(next) => next,
                    Err(_) => {
                        if let Ok(mut w) = watch.lock() {
                            w.fault = Some(Fault::Timeout.key());
                        }
                        let error = std::io::Error::other(format!(
                            "上游 {} 秒没有新数据，本机路由断开了这一发（静默超时）",
                            limit.as_secs()
                        ));
                        return Some((Err(error), (stream, watch, true)));
                    }
                },
                None => stream.next().await,
            };
            match next {
                None => None,
                Some(Ok(bytes)) => {
                    if let Ok(mut w) = watch.lock() {
                        w.feed(&bytes);
                    }
                    Some((Ok(Frame::data(bytes)), (stream, watch, false)))
                }
                Some(Err(e)) => {
                    if let Ok(mut w) = watch.lock() {
                        w.fault = Some(Fault::UpstreamDown.key());
                    }
                    let error = std::io::Error::other(e.to_string());
                    Some((Err(error), (stream, watch, true)))
                }
            }
        },
    );
    let stream = head.chain(rest).map(move |frame| {
        let _keep = &record;
        frame
    });
    // 显式点名 BodyExt —— StreamExt 也有一个 boxed,重名。
    BodyExt::boxed_unsync(StreamBody::new(stream))
}

#[cfg(test)]
mod tests {
    //! 重试循环的端到端测试：在回环上起几个假上游，真的转一次。
    //!
    //! 跟 `mod.rs` 里那一组一样**不算联网**：只在 127.0.0.1 上，不动真 ACL、不碰真进程、
    //! 不写 %LOCALAPPDATA%。

    use super::*;
    use hyper::body::Incoming;
    use hyper::service::service_fn;
    use hyper::Request;
    use hyper_util::rt::TokioIo;
    use qb_proxy::model_mapper::ModelMapping;
    use std::net::Ipv4Addr;

    /// 假上游这一次怎么回。
    #[derive(Clone)]
    struct Script {
        status: u16,
        content_type: &'static str,
        chunks: Vec<&'static str>,
        /// 一个字节都不发之前先停多久（测首包超时）。
        delay_ms: u64,
        /// 发完 `chunks` 之后停多久再收尾（测静默超时）。
        stall_ms: u64,
        /// 发完之后把连接弄断（测「交出去之后出错不换线」）。
        break_after: bool,
    }

    impl Script {
        fn json(status: u16, body: &'static str) -> Self {
            Self {
                status,
                content_type: "application/json",
                chunks: vec![body],
                delay_ms: 0,
                stall_ms: 0,
                break_after: false,
            }
        }
        fn sse(chunks: Vec<&'static str>) -> Self {
            Self {
                status: 200,
                content_type: "text/event-stream",
                chunks,
                delay_ms: 0,
                stall_ms: 0,
                break_after: false,
            }
        }
    }

    /// 收到的每一发：`(路径, 请求体)`。
    type Hits = Arc<Mutex<Vec<(String, String)>>>;

    type StubBody = UnsyncBoxBody<Bytes, std::io::Error>;

    async fn stub(script: Script) -> (SocketAddr, Hits) {
        let hits: Hits = Arc::new(Mutex::new(Vec::new()));
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("假上游绑不上回环");
        let addr = listener.local_addr().expect("拿不到假上游端口");
        let seen = Arc::clone(&hits);
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let seen = Arc::clone(&seen);
                let script = script.clone();
                tokio::spawn(async move {
                    let svc = service_fn(move |req: Request<Incoming>| {
                        let seen = Arc::clone(&seen);
                        let script = script.clone();
                        async move {
                            let path = req
                                .uri()
                                .path_and_query()
                                .map(|p| p.as_str().to_string())
                                .unwrap_or_default();
                            let body = req.into_body().collect().await.unwrap().to_bytes();
                            seen.lock()
                                .unwrap()
                                .push((path, String::from_utf8_lossy(&body).into_owned()));
                            if script.delay_ms > 0 {
                                tokio::time::sleep(Duration::from_millis(script.delay_ms)).await;
                            }
                            let frames = futures_util::stream::iter(
                                script
                                    .chunks
                                    .iter()
                                    .map(|c| Ok::<_, std::io::Error>(Frame::data(Bytes::from(*c))))
                                    .collect::<Vec<_>>(),
                            );
                            let stall = script.stall_ms;
                            let broken = script.break_after;
                            let tail = futures_util::stream::once(async move {
                                if stall > 0 {
                                    tokio::time::sleep(Duration::from_millis(stall)).await;
                                }
                                if broken {
                                    Err(std::io::Error::other("假上游故意断开"))
                                } else {
                                    Ok(Frame::data(Bytes::new()))
                                }
                            });
                            let body: StubBody =
                                BodyExt::boxed_unsync(StreamBody::new(frames.chain(tail)));
                            let mut r = Response::new(body);
                            *r.status_mut() = StatusCode::from_u16(script.status).unwrap();
                            r.headers_mut()
                                .insert("content-type", script.content_type.parse().unwrap());
                            Ok::<_, std::convert::Infallible>(r)
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), svc)
                        .await;
                });
            }
        });
        (addr, hits)
    }

    fn line(id: &str, addr: SocketAddr) -> Upstream {
        Upstream {
            route_id: id.into(),
            base_url: format!("http://127.0.0.1:{}/v1", addr.port()),
            auth: UpstreamAuth::ApiKey(format!("key-{id}")),
            ..Default::default()
        }
    }

    /// 一个软件两条线：`a` 是当前那条，`b` 排在派单顺序里。
    async fn two_lines(
        client: Client,
        a: Upstream,
        b: Upstream,
        tune: impl FnOnce(&mut RouterSettings),
    ) -> RouterHandle {
        let state = Arc::new(Mutex::new(RouterState::default()));
        {
            let mut s = state.lock().unwrap();
            let mut settings = RouterSettings::for_client(client);
            tune(&mut settings);
            s.set_settings(settings);
            s.set_queue(client, vec![a.route_id.clone(), b.route_id.clone()]);
            s.set_now(client, a.route_id.clone());
            s.put_upstream(a);
            s.put_upstream(b);
        }
        serve(RouterConfig::loopback(0), state, reqwest::Client::new())
            .await
            .expect("本机路由起不来")
    }

    async fn post(h: &RouterHandle, path: &str, body: &str) -> (u16, String) {
        let resp = reqwest::Client::new()
            .post(format!("{}{path}", h.base_url()))
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("打不通本机路由");
        let status = resp.status().as_u16();
        (status, resp.text().await.unwrap_or_default())
    }

    /// 日志是流被丢掉时才进去的，跟客户端读完有一点点错位 —— 等它到。
    async fn last_log(h: &RouterHandle) -> RouterRequest {
        for _ in 0..200 {
            if let Some(log) = h.state.lock().unwrap().logs().back().cloned() {
                return log;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("日志一直没进来");
    }

    const CLAUDE_OK: &str = r#"{"id":"msg_1","type":"message","content":[],"usage":{"input_tokens":3,"output_tokens":4}}"#;

    #[tokio::test]
    async fn a_5xx_before_the_first_byte_moves_on_to_the_next_line() {
        let (a, a_hits) = stub(Script::json(502, r#"{"error":{"message":"bad gateway"}}"#)).await;
        let (b, b_hits) = stub(Script::json(200, CLAUDE_OK)).await;
        let h = two_lines(Client::ClaudeCode, line("a", a), line("b", b), |_| {}).await;

        let (status, body) = post(&h, "/v1/messages", r#"{"model":"claude-opus-5"}"#).await;
        assert_eq!(status, 200, "第一条 502 之后没换到第二条");
        assert_eq!(body, CLAUDE_OK);
        assert_eq!(a_hits.lock().unwrap().len(), 1);
        assert_eq!(b_hits.lock().unwrap().len(), 1);

        let log = last_log(&h).await;
        assert_eq!(log.route_id, "b", "日志记的不是最后回话的那条");
        assert_eq!(
            log.attempts
                .iter()
                .map(|t| t.route_id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"],
            "试过的线路链没记全"
        );
        assert_eq!(log.attempts[0].fault.as_deref(), Some("upstream-down"));
        assert_eq!(log.attempts[0].status, Some(502));
        assert!(log.ok());
        assert_eq!(
            (log.input_tokens, log.output_tokens),
            (Some(3), Some(4)),
            "非流式的用量没解出来"
        );
    }

    #[tokio::test]
    async fn a_400_goes_straight_back_without_trying_another_line_or_hurting_the_breaker() {
        // 400 是请求自己的问题：换哪条线都一样，算成线路故障会把一条好线按自己的 bug 掐掉。
        let (a, _) = stub(Script::json(400, r#"{"error":{"message":"bad param"}}"#)).await;
        let (b, b_hits) = stub(Script::json(200, CLAUDE_OK)).await;
        let h = two_lines(Client::ClaudeCode, line("a", a), line("b", b), |_| {}).await;

        for _ in 0..(qb_station::schedule::breaker::TRIP_AFTER_CONSECUTIVE_ERRORS * 2) {
            let (status, body) = post(&h, "/v1/messages", r#"{"model":"m"}"#).await;
            assert_eq!(status, 400, "上游的 400 要原样回给客户端");
            assert!(body.contains("bad param"), "上游的原话丢了：{body}");
        }
        assert!(b_hits.lock().unwrap().is_empty(), "400 也换了线");
        let now = chrono::Utc::now().timestamp_millis();
        let s = h.state.lock().unwrap();
        assert!(s.usable("a", now), "400 把线路熔断了");
        assert_eq!(s.breaker_state(now)["a"].0, 1.0, "400 让线路降了权");
        assert_eq!(
            s.logs().back().unwrap().fault.as_deref(),
            Some("client-request")
        );
    }

    #[tokio::test]
    async fn an_error_hidden_in_a_2xx_body_moves_on_to_the_next_line() {
        let (a, _) = stub(Script::json(
            200,
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
        ))
        .await;
        let (b, b_hits) = stub(Script::json(200, CLAUDE_OK)).await;
        let h = two_lines(Client::ClaudeCode, line("a", a), line("b", b), |_| {}).await;

        let (status, body) = post(&h, "/v1/messages", r#"{"model":"m"}"#).await;
        assert_eq!((status, body.as_str()), (200, CLAUDE_OK));
        assert_eq!(
            b_hits.lock().unwrap().len(),
            1,
            "2xx 里的错误信封没触发换线"
        );
        assert_eq!(
            last_log(&h).await.attempts[0].fault.as_deref(),
            Some("overloaded")
        );
    }

    #[tokio::test]
    async fn an_error_event_before_any_output_still_moves_on_to_the_next_line() {
        let (a, _) = stub(Script::sse(vec![
            "event: ping\ndata: {\"type\":\"ping\"}\n\n",
            "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n",
        ]))
        .await;
        let (b, _) = stub(Script::sse(vec![
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_b\",\"model\":\"m\",\"usage\":{\"input_tokens\":12,\"cache_read_input_tokens\":300,\"cache_creation_input_tokens\":40,\"cache_creation\":{\"ephemeral_1h_input_tokens\":40}}}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":7}}\n\n",
        ]))
        .await;
        let h = two_lines(Client::ClaudeCode, line("a", a), line("b", b), |_| {}).await;

        let (status, body) = post(&h, "/v1/messages", r#"{"model":"m","stream":true}"#).await;
        assert_eq!(status, 200);
        assert!(body.contains("msg_b"), "没换到第二条：{body}");
        assert!(
            !body.contains("overloaded_error"),
            "第一条的错误事件漏给了客户端"
        );

        let log = last_log(&h).await;
        assert!(log.streaming);
        assert_eq!(log.route_id, "b");
        assert_eq!(
            (
                log.input_tokens,
                log.output_tokens,
                log.cache_read_tokens,
                log.cache_write_tokens,
                log.cache_write_1h_tokens
            ),
            (Some(12), Some(7), Some(300), Some(40), Some(40)),
            "流里的四类 token 没记进日志"
        );
    }

    #[tokio::test]
    async fn a_stream_that_breaks_after_it_was_handed_over_is_not_retried() {
        // 交出去之后出错只能如实断开：再换一条线等于让客户端收到两段拼起来的回复。
        let mut broken = Script::sse(vec![
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_a\"}}\n\n",
        ]);
        broken.break_after = true;
        // 先让第一块真的发出去再断（不停一下的话，hyper 会在同一轮里先撞上错误、一个字节都不发）。
        broken.stall_ms = 300;
        let (a, _) = stub(broken).await;
        let (b, b_hits) = stub(Script::sse(vec!["event: message_start\ndata: {}\n\n"])).await;
        let h = two_lines(Client::ClaudeCode, line("a", a), line("b", b), |_| {}).await;

        let resp = reqwest::Client::new()
            .post(format!("{}/v1/messages", h.base_url()))
            .body(r#"{"model":"m","stream":true}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert!(
            resp.bytes().await.is_err(),
            "上游断了，客户端却读到一个完整的回复"
        );
        assert!(b_hits.lock().unwrap().is_empty(), "交出去之后还换了线");
        let log = last_log(&h).await;
        assert_eq!(log.fault.as_deref(), Some("upstream-down"));
        assert!(!log.ok());
    }

    #[tokio::test]
    async fn no_first_byte_within_the_limit_moves_on_to_the_next_line() {
        let mut slow = Script::sse(vec!["event: message_start\ndata: {}\n\n"]);
        slow.delay_ms = 3_000;
        let (a, _) = stub(slow).await;
        let (b, b_hits) = stub(Script::sse(vec![
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_b\"}}\n\n",
        ]))
        .await;
        let h = two_lines(Client::ClaudeCode, line("a", a), line("b", b), |s| {
            s.first_byte_timeout_secs = 1;
        })
        .await;

        let started = Instant::now();
        let (status, body) = post(&h, "/v1/messages", r#"{"model":"m","stream":true}"#).await;
        assert_eq!(status, 200);
        assert!(body.contains("msg_b"));
        assert_eq!(b_hits.lock().unwrap().len(), 1);
        assert!(
            started.elapsed() < Duration::from_millis(2_900),
            "首包超时没生效"
        );
        assert_eq!(
            last_log(&h).await.attempts[0].fault.as_deref(),
            Some("timeout")
        );
    }

    #[tokio::test]
    async fn only_heartbeats_before_output_hold_the_stream_for_seconds_not_the_whole_limit() {
        // 推理很久才出第一段的长回合：开头只有心跳。不许因为「还没见到内容」就判超时换线，
        // 也不许把它压到整个首包超时 —— 压满 MAX_HOLD 就照原样交出去。
        let mut thinking = Script::sse(vec!["event: ping\ndata: {\"type\":\"ping\"}\n\n"]);
        thinking.stall_ms = 30_000;
        let (a, _) = stub(thinking).await;
        let (b, b_hits) = stub(Script::sse(vec!["event: message_start\ndata: {}\n\n"])).await;
        let h = two_lines(Client::ClaudeCode, line("a", a), line("b", b), |s| {
            s.first_byte_timeout_secs = 60;
        })
        .await;

        let started = Instant::now();
        let resp = reqwest::Client::new()
            .post(format!("{}/v1/messages", h.base_url()))
            .body(r#"{"model":"m","stream":true}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let waited = started.elapsed();
        assert!(
            waited >= MAX_HOLD - Duration::from_millis(500)
                && waited < MAX_HOLD + Duration::from_secs(5),
            "开头只有心跳的流被压了 {waited:?}（应当约 {MAX_HOLD:?}）"
        );
        assert!(b_hits.lock().unwrap().is_empty(), "只有心跳就当失败换了线");
    }

    #[tokio::test]
    async fn a_stream_that_goes_quiet_is_cut_by_the_idle_timeout() {
        let mut quiet = Script::sse(vec![
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_a\"}}\n\n",
        ]);
        quiet.stall_ms = 5_000;
        let (a, _) = stub(quiet).await;
        let (b, _) = stub(Script::sse(vec!["event: message_start\ndata: {}\n\n"])).await;
        let h = two_lines(Client::ClaudeCode, line("a", a), line("b", b), |s| {
            s.idle_timeout_secs = 1;
        })
        .await;

        let started = Instant::now();
        let resp = reqwest::Client::new()
            .post(format!("{}/v1/messages", h.base_url()))
            .body(r#"{"model":"m","stream":true}"#)
            .send()
            .await
            .unwrap();
        assert!(resp.bytes().await.is_err(), "卡住的流没被断开");
        assert!(
            started.elapsed() < Duration::from_millis(4_000),
            "静默超时没生效"
        );
        assert_eq!(last_log(&h).await.fault.as_deref(), Some("timeout"));
    }

    #[tokio::test]
    async fn the_model_name_is_mapped_per_line() {
        let (a, a_hits) = stub(Script::json(200, CLAUDE_OK)).await;
        let (b, _) = stub(Script::json(200, CLAUDE_OK)).await;
        let mut mapped = line("a", a);
        mapped.model_map = ModelMapping {
            sonnet_model: Some("relay-sonnet-x".into()),
            ..Default::default()
        };
        let h = two_lines(Client::ClaudeCode, mapped, line("b", b), |_| {}).await;

        let (status, _) = post(
            &h,
            "/v1/messages",
            r#"{"model":"claude-sonnet-5","max_tokens":1}"#,
        )
        .await;
        assert_eq!(status, 200);
        let sent: Value = serde_json::from_str(&a_hits.lock().unwrap()[0].1).unwrap();
        assert_eq!(sent["model"], "relay-sonnet-x", "这条线上的模型名没换");
        assert_eq!(sent["max_tokens"], 1, "映射时把别的字段弄丢了");
        let log = last_log(&h).await;
        assert_eq!(log.model, "claude-sonnet-5");
        assert_eq!(log.upstream_model, "relay-sonnet-x");
    }

    #[tokio::test]
    async fn a_route_s_exact_pairs_win_over_tiers_and_each_line_maps_on_its_own() {
        use qb_station::station::route::{ModelPair, RouteModelMap};
        // a 失败、换到 b：发给 b 的是 b 自己那份映射，不是 a 换过的名字。
        let (a, a_hits) = stub(Script::json(502, r#"{"error":{"message":"bad gateway"}}"#)).await;
        let (b, b_hits) = stub(Script::json(200, r#"{"id":"resp_1"}"#)).await;
        let map_a = RouteModelMap {
            exact: vec![ModelPair {
                from: "GPT-5.6-SOL".into(),
                to: "a-sol".into(),
            }],
            default: Some("a-fallback".into()),
            ..Default::default()
        };
        let map_b = RouteModelMap {
            default: Some("b-everything".into()),
            ..Default::default()
        };
        let h = two_lines(
            Client::Codex,
            line("a", a).with_model_map(&map_a),
            line("b", b).with_model_map(&map_b),
            |_| {},
        )
        .await;

        post(
            &h,
            "/codex/v1/chat/completions",
            r#"{"model":"gpt-5.6-sol"}"#,
        )
        .await;
        let sent_a: Value = serde_json::from_str(&a_hits.lock().unwrap()[0].1).unwrap();
        assert_eq!(
            sent_a["model"], "a-sol",
            "逐个指定的没先于兜底生效（不分大小写）"
        );
        let sent_b: Value = serde_json::from_str(&b_hits.lock().unwrap()[0].1).unwrap();
        assert_eq!(
            sent_b["model"], "b-everything",
            "换线后用的不是那条线自己的映射"
        );
        assert_eq!(last_log(&h).await.upstream_model, "b-everything");
    }

    #[tokio::test]
    async fn a_line_without_the_model_is_skipped_for_six_hours() {
        let (a, a_hits) = stub(Script::json(
            404,
            r#"{"error":{"message":"model_not_found: claude-opus-5"}}"#,
        ))
        .await;
        let (b, b_hits) = stub(Script::json(200, CLAUDE_OK)).await;
        let h = two_lines(Client::ClaudeCode, line("a", a), line("b", b), |_| {}).await;

        for _ in 0..3 {
            let (status, _) = post(&h, "/v1/messages", r#"{"model":"claude-opus-5"}"#).await;
            assert_eq!(status, 200);
        }
        assert_eq!(a_hits.lock().unwrap().len(), 1, "缺模型的线没被跳过");
        assert_eq!(b_hits.lock().unwrap().len(), 3);
        // 别的模型照常打这条线：只跳过「这条线上的这个模型」。
        post(&h, "/v1/messages", r#"{"model":"claude-haiku-5"}"#).await;
        assert_eq!(a_hits.lock().unwrap().len(), 2);
        let now = chrono::Utc::now().timestamp_millis();
        assert!(
            h.state.lock().unwrap().usable("a", now),
            "缺模型把整条线熔断了"
        );
    }

    #[tokio::test]
    async fn with_failover_off_only_the_current_line_is_tried() {
        let (a, _) = stub(Script::json(502, r#"{"error":{"message":"bad gateway"}}"#)).await;
        let (b, b_hits) = stub(Script::json(200, CLAUDE_OK)).await;
        let h = two_lines(Client::ClaudeCode, line("a", a), line("b", b), |s| {
            s.auto_failover = false;
        })
        .await;

        let (status, _) = post(&h, "/v1/messages", r#"{"model":"m"}"#).await;
        assert_eq!(status, 502, "关了故障转移，失败要照实回给客户端");
        assert!(b_hits.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_tripped_line_is_skipped_and_its_half_open_probe_comes_back() {
        let (a, a_hits) = stub(Script::json(502, r#"{"error":{"message":"bad gateway"}}"#)).await;
        let (b, b_hits) = stub(Script::json(200, CLAUDE_OK)).await;
        let h = two_lines(Client::ClaudeCode, line("a", a), line("b", b), |_| {}).await;

        // 连续 5 次服务端错误：a 熔断。之后的请求直接从 b 开始，不再打 a。
        let trip = qb_station::schedule::breaker::TRIP_AFTER_CONSECUTIVE_ERRORS as usize;
        for _ in 0..trip {
            assert_eq!(post(&h, "/v1/messages", r#"{"model":"m"}"#).await.0, 200);
        }
        let now = chrono::Utc::now().timestamp_millis();
        assert!(
            !h.state.lock().unwrap().usable("a", now),
            "连续 5 次 502 没熔断"
        );
        assert_eq!(post(&h, "/v1/messages", r#"{"model":"m"}"#).await.0, 200);
        assert_eq!(a_hits.lock().unwrap().len(), trip, "熔断中的线还在挨打");
        assert_eq!(b_hits.lock().unwrap().len(), trip + 1);

        // 「我充值了，立刻重测」：转半开，下一发就是回探（它还是 502，于是又关回去）。
        assert!(h.state.lock().unwrap().retest("a", now));
        assert_eq!(post(&h, "/v1/messages", r#"{"model":"m"}"#).await.0, 200);
        assert_eq!(
            a_hits.lock().unwrap().len(),
            trip + 1,
            "半开没放回探的那一发"
        );
        let now = chrono::Utc::now().timestamp_millis();
        assert!(
            !h.state.lock().unwrap().usable("a", now),
            "回探失败没关回去"
        );
    }

    #[tokio::test]
    async fn the_official_codex_line_never_fails_over_to_a_relay() {
        // ⛔ 官方线失败换到中转线 = 拿官方身份的请求去打中转的 Key。
        let (official, _) = stub(Script::json(502, r#"{"error":{"message":"bad gateway"}}"#)).await;
        let (relay, relay_hits) = stub(Script::json(200, r#"{"id":"resp_1"}"#)).await;
        let state = Arc::new(Mutex::new(RouterState::default()));
        {
            let mut s = state.lock().unwrap();
            s.put_upstream(Upstream {
                route_id: OFFICIAL_CODEX_ROUTE_ID.into(),
                base_url: format!("http://127.0.0.1:{}", official.port()),
                auth: UpstreamAuth::OAuthPassthrough,
                ..Default::default()
            });
            s.put_upstream(line("relay", relay));
            s.set_queue(Client::Codex, vec!["relay".into()]);
            s.set_now(Client::Codex, OFFICIAL_CODEX_ROUTE_ID);
        }
        let h = serve(
            RouterConfig::loopback(0),
            Arc::clone(&state),
            reqwest::Client::new(),
        )
        .await
        .unwrap();
        let (status, _) = post(&h, "/codex/v1/responses", r#"{"model":"gpt-5.6-sol"}"#).await;
        assert_eq!(status, 502);
        assert!(
            relay_hits.lock().unwrap().is_empty(),
            "官方线失败后换到了中转线"
        );

        // 反过来：中转线失败时，官方线也不当替补。
        let s = state.lock().unwrap();
        let mut queue = s.queue(Client::Codex);
        queue.push(OFFICIAL_CODEX_ROUTE_ID.into());
        drop(s);
        state.lock().unwrap().set_queue(Client::Codex, queue);
        let relay_line = line("relay", relay);
        let lines = state.lock().unwrap().candidates(Client::Codex, &relay_line);
        assert!(
            lines.iter().all(|u| u.route_id != OFFICIAL_CODEX_ROUTE_ID),
            "官方线进了中转线的替补名单"
        );
    }

    #[tokio::test]
    async fn the_outbound_proxy_carries_relay_lines_and_never_the_official_one() {
        // 出站代理指向一个没人听的端口：走它的请求一定连不上。
        // 中转线路走它（于是失败），官方 Codex 那条线永远直连（于是照常成功）。
        let dead = {
            let l = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            l.local_addr().unwrap().port()
        };
        let proxied = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{dead}")).unwrap())
            .build()
            .unwrap();
        let (relay, relay_hits) = stub(Script::json(200, CLAUDE_OK)).await;
        let (official, official_hits) = stub(Script::sse(vec![
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n",
        ]))
        .await;
        let state = Arc::new(Mutex::new(RouterState::default()));
        {
            let mut s = state.lock().unwrap();
            s.put_upstream(line("relay", relay));
            s.set_now(Client::ClaudeCode, "relay");
            s.put_upstream(Upstream {
                route_id: OFFICIAL_CODEX_ROUTE_ID.into(),
                base_url: format!("http://127.0.0.1:{}", official.port()),
                auth: UpstreamAuth::OAuthPassthrough,
                ..Default::default()
            });
            s.set_now(Client::Codex, OFFICIAL_CODEX_ROUTE_ID);
            s.set_relay_http(Some(proxied));
        }
        let h = serve(
            RouterConfig::loopback(0),
            Arc::clone(&state),
            reqwest::Client::new(),
        )
        .await
        .unwrap();

        let (status, _) = post(&h, "/v1/messages", r#"{"model":"m"}"#).await;
        assert_eq!(
            status, 502,
            "中转线路没走出站代理（代理是死的，走了就该连不上）"
        );
        assert!(relay_hits.lock().unwrap().is_empty());

        let (status, _) = post(&h, "/codex/v1/responses", r#"{"model":"gpt-5.6-sol"}"#).await;
        assert_eq!(status, 200, "官方线走了出站代理");
        assert_eq!(official_hits.lock().unwrap().len(), 1);

        // 清掉出站代理：中转线路又直连了。
        state.lock().unwrap().set_relay_http(None);
        let (status, _) = post(&h, "/v1/messages", r#"{"model":"m"}"#).await;
        assert_eq!(status, 200);
    }

    #[tokio::test]
    async fn the_line_s_own_key_replaces_the_client_s_and_encoding_is_identity() {
        let (a, _) = stub(Script::json(200, CLAUDE_OK)).await;
        let state = Arc::new(Mutex::new(RouterState::default()));
        state.lock().unwrap().put_upstream(line("a", a));
        state.lock().unwrap().set_now(Client::ClaudeCode, "a");
        let h = serve(RouterConfig::loopback(0), state, reqwest::Client::new())
            .await
            .unwrap();
        // 回环假上游只记了请求体；头的断言在 mod.rs 那条老测试里。这里只确认 gzip 请求也能读出用量。
        let resp = reqwest::Client::new()
            .post(format!("{}/v1/messages", h.base_url()))
            .header("accept-encoding", "gzip, br")
            .body(r#"{"model":"m"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.text().await.unwrap(), CLAUDE_OK);
        assert_eq!(last_log(&h).await.output_tokens, Some(4));
    }
}
