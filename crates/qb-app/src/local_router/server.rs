//! 监听、收连接、把一发请求分给两条路：中转线路（[`forward`](super::forward)）、
//! 官方 Codex（[`official`](super::official)）。
//!
//! 这一层只做三件事：认软件、绑这个软件此刻的当前线路、收齐请求体。**绑定拿的是一份拷贝** ——
//! 之后换上游与这一发无关（`qb_station::router::UpstreamSwitch` 的规矩）。

use super::log::Record;
use super::*;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{HeaderMap, Method, Request};
use hyper_util::rt::TokioIo;
use std::time::Instant;

/// 一发请求收齐之后交给下面两条路的东西。
pub(super) struct Inbound {
    pub caller: Client,
    pub method: Method,
    /// **剥掉软件前缀之后**的路径（含查询串），见 [`route_of`]。
    pub path: String,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub started: Instant,
    pub now_ms: i64,
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
    // ⛔ 非回环却没有口令：不起。同一个局域网里的任何人都能拿你的 Key 打你的额度（模块头）。
    if config.unguarded() {
        return Err(GateError::Other(format!(
            "本机路由开到 {} 上必须设路由口令：不设的话，同一个网络里谁都能拿你的 Key 打你的额度。",
            config.listen
        )));
    }
    let want = config.bind();
    let listener = tokio::net::TcpListener::bind(want).await.map_err(|e| {
        GateError::Other(format!(
            "本机路由绑不上 {want} —— {e}。这个端口多半被别的程序占了,换一个再试。",
        ))
    })?;
    let key: Option<Arc<str>> = config
        .key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(Arc::from);
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
            let Ok((stream, peer)) = accepted else {
                // 单次 accept 失败不该让整个路由停掉 —— 客户端会重连。
                continue;
            };
            let state = Arc::clone(&accept_state);
            let client = client.clone();
            let key = key.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |req| {
                    handle(
                        req,
                        Arc::clone(&state),
                        client.clone(),
                        peer.ip(),
                        key.clone(),
                    )
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), svc)
                    .await;
            });
        }
    });

    Ok(RouterHandle {
        addr,
        state,
        stop,
        log_writer: None,
    })
}

async fn handle(
    req: Request<Incoming>,
    state: Arc<Mutex<RouterState>>,
    // 不叫 client —— 下面 `caller` 才是「哪个软件发来的」。
    http: reqwest::Client,
    peer: std::net::IpAddr,
    key: Option<Arc<str>>,
) -> std::result::Result<Response<ProxyBody>, std::convert::Infallible> {
    let started = Instant::now();
    let now_ms = chrono::Utc::now().timestamp_millis();

    // 别的机器来的先验口令（本机来的不查，见模块头）。在绑线之前做：进不来的不占在飞计数。
    if let Err((status, why)) = admit(peer, req.headers(), key.as_deref(), false) {
        return Ok(text(status, why));
    }

    let raw_path = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "/".into());
    // 没有中转路径的软件（反重力）拿到的是 `NO_RELAY_PREFIX` 前缀。理论上永远
    // 到不了这里（入口就拒绝建环境），真到了也不许落到别的软件的线上 —— 把原因说清楚。
    if raw_path.starts_with(NO_RELAY_PREFIX) {
        return Ok(text(
            StatusCode::NOT_FOUND,
            "这个软件没有中转路径：它的 API 端点写死在客户端自己的程序里，本机路由无法为它换上游。",
        ));
    }
    // 先认这一发是哪个软件发来的,再按那个软件去绑线 ——
    // 三个软件各有各的当前上游,认错软件就会打到别人的站上。
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
            .and_then(|id| s.upstreams.get(&id).cloned())
    };

    let Some(upstream) = bound else {
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

    // 从这里起，这一发的收尾（在飞计数减一、日志入队）都由 `Record` 的 Drop 做 ——
    // 不管从哪条路返回、流在哪一刻被丢掉。
    let mut record = Record::new(
        Arc::clone(&state),
        caller,
        started,
        RouterRequest {
            id: format!("{}-{}", now_ms, started.elapsed().as_nanos()),
            at_ms: now_ms,
            client: client_key(caller).to_string(),
            route_id: upstream.route_id.clone(),
            ..Default::default()
        },
    );

    let (parts, body) = req.into_parts();
    let body = match body.collect().await {
        Ok(b) => b.to_bytes(),
        Err(e) => {
            record.log.fault = Some(qb_proxy::classify::Fault::ClientAbort.key().to_string());
            record.log.error = Some(format!("读不到客户端请求体：{e}"));
            return Ok(text(
                StatusCode::BAD_REQUEST,
                &format!("读不到客户端请求体：{e}"),
            ));
        }
    };
    // 模型名从请求体里抠 —— 日志表上「模型」那一栏要用。抠不出来就空着。
    record.log.model = model_of(&body);

    let inbound = Inbound {
        caller,
        method: parts.method,
        path,
        headers: parts.headers,
        body,
        started,
        now_ms,
    };

    // 官方 Codex turn-state 模式:保留客户端自带的 OAuth,不剥不换、不进重试。见 `UpstreamAuth`。
    let response = if matches!(upstream.auth, UpstreamAuth::OAuthPassthrough) {
        // ⛔ 官方线只给本机用（`admit` 的第三条）。口令对了也不行。
        if let Err((status, why)) = admit(peer, &inbound.headers, key.as_deref(), true) {
            record.log.fault = Some(qb_proxy::classify::Fault::ClientRequest.key().to_string());
            record.log.error = Some(why.to_string());
            record.log.status = Some(status.as_u16());
            return Ok(text(status, why));
        }
        // 官方线永远用直连的那个客户端，不走出站代理（`RouterState::relay_http` 只给中转线路）。
        official::relay(&state, &http, inbound, upstream, record).await
    } else {
        let relay_http = match state.lock() {
            Ok(s) => s.relay_http(&http),
            Err(_) => http.clone(),
        };
        forward::relay(&state, &relay_http, inbound, upstream, record).await
    };
    Ok(response)
}
