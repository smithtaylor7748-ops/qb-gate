//! 官方 Codex（turn-state 识别）：**不进重试、不换线、不走出站代理**，原样直通。
//!
//! ⛔ 这是「路由绝不承载官方身份」那条安全属性的唯一受控例外（[`UpstreamAuth::OAuthPassthrough`]）。
//! 请求带着使用者自己的 ChatGPT OAuth 转到官方端点，所以：
//! - **不换线**：失败了换到别的线，等于把官方身份的请求打到某条中转线的 Key 上（cc-switch 的
//!   `provider_supports_failover` 同样让官方线永不参与重试）；
//! - **不整流、不改请求体**：官方自己签发的状态原样带回去；
//! - **整机一条**（[`OFFICIAL_CODEX_ROUTE_ID`]）。
//!
//! 跟中转线路共用的只有收尾：日志、用量、Codex 2xx 流的健康判定推迟到流结束。

use super::forward::{describe_send_error, stream_body};
use super::log::{Record, StreamWatch};
use super::server::Inbound;
use super::*;
use qb_proxy::classify::{self, Fault};
use qb_station::router::RouteAttempt;
use qb_station::schedule::Permit;
use std::time::Instant;

/// 官方线只有一条、不进熔断放行（跟 cc-switch「只有一家时跳过熔断」同一个道理），名额是空的。
const NO_PERMIT: Permit = Permit {
    allowed: true,
    half_open: false,
};

pub(super) async fn relay(
    state: &Arc<Mutex<RouterState>>,
    http: &reqwest::Client,
    inbound: Inbound,
    upstream: Upstream,
    mut record: Record,
) -> Response<ProxyBody> {
    let Inbound {
        caller,
        method,
        path,
        headers: incoming,
        body,
        started,
        now_ms,
    } = inbound;
    let model = record.log.model.clone();
    let route_id = upstream.route_id.clone();

    // 客户端有没有自己带 turn-state —— 带了就保留它自己的,我们不覆盖。
    let client_has_state = incoming.contains_key(turnstate::HEADER);

    // 头:只丢逐跳头,**保留客户端的 OAuth**。
    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in &incoming {
        if !HOP_BY_HOP.contains(&name.as_str().to_ascii_lowercase().as_str()) {
            headers.insert(name.clone(), value.clone());
        }
    }

    // 注入开着、客户端没带、库里有可用值时,补上一张 turn-state。
    // 客户端自己带了就保留(见上面 `client_has_state`),不弄坏它自己的状态。
    let injected = if caller == Client::Codex && !client_has_state {
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

    let tried = Instant::now();
    let url = join_url(&upstream.base_url, &path);
    let sent = http
        .request(method, &url)
        .headers(headers)
        .body(body)
        .send()
        .await;

    let mut resp = match sent {
        Ok(r) => r,
        Err(e) => {
            // 连不上或超时 —— 对熔断来说是同一档。登记 + 立刻按超时判健康。**不换线。**
            let error = describe_send_error(&e);
            let class = classify::classify(&error, None);
            if let Ok(mut s) = state.lock() {
                s.breaker_mut(&route_id, caller)
                    .record(Outcome::timeout(), now_ms);
            }
            record.log.attempts.push(RouteAttempt {
                route_id: route_id.clone(),
                status: None,
                fault: Some(class.fault.key().to_string()),
                ms: tried.elapsed().as_millis() as u64,
            });
            record.log.fault = Some(class.fault.key().to_string());
            record.log.error = Some(error.to_string());
            return text(StatusCode::BAD_GATEWAY, &format!("上游没回应：{e}"));
        }
    };

    let status = resp.status().as_u16();
    let retry = retry_after_ms(
        resp.headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok()),
    );
    let outcome = Outcome::from_status(status, retry);

    // 把响应带回的 turn-state **被动采集**进来(免费),并对我们注入过的那一发更新 strikes。
    // 这张头同时照常转回客户端,让 Codex 自己的状态也接得上。
    if caller == Client::Codex {
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

    // 首字：拉出第一块再计时。拿「响应头回来了」当首字会系统性地偏小。
    let first = resp.chunk().await;
    record.log.first_token_ms = Some(started.elapsed().as_millis() as u64);
    record.log.status = Some(status);
    if let Outcome::RateLimited { retry_after_ms } = outcome {
        record.log.retry_after_ms = retry_after_ms;
    }
    let ok = (200..300).contains(&status);
    let fault = (!ok).then(|| classify::classify_status(status).key().to_string());
    record.log.attempts.push(RouteAttempt {
        route_id: route_id.clone(),
        status: Some(status),
        fault: fault.clone(),
        ms: tried.elapsed().as_millis() as u64,
    });
    record.log.fault = fault;

    let primed = match first {
        Ok(Some(b)) => vec![b],
        Ok(None) => Vec::new(),
        Err(e) => {
            // 响应头回来了、第一块就读断：还什么都没交给客户端，如实回 502，别交一个空的「成功」。
            if let Ok(mut s) = state.lock() {
                s.breaker_mut(&route_id, caller)
                    .record(Outcome::ServerError, now_ms);
            }
            record.log.fault = Some(Fault::UpstreamDown.key().to_string());
            record.log.error = Some(format!("读不到上游的第一块数据：{e}"));
            return text(
                StatusCode::BAD_GATEWAY,
                &format!("读不到上游的第一块数据：{e}"),
            );
        }
    };

    // ⛔ Codex 的 2xx 流:健康判定推迟到流结束（HTTP 200 的 SSE 里会报 `response.failed`）。
    let defer = caller == Client::Codex && ok;
    if !defer {
        if let Ok(mut s) = state.lock() {
            s.breaker_mut(&route_id, caller).record(outcome, now_ms);
        }
    }
    let mut watch = StreamWatch::new(caller);
    if defer {
        watch = watch.defer_health(&route_id, NO_PERMIT);
    }
    // 官方线照旧不设静默超时：它原来就没有，这次重构不改官方线的行为。
    let body = stream_body(primed, resp.bytes_stream(), watch, record, None);
    match out.body(body) {
        Ok(r) => r,
        Err(e) => text(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("拼不出响应：{e}"),
        ),
    }
}
