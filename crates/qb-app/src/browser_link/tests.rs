//! 连接桥的单测：只连本机回环上刚开的端口，配对记录放临时目录，IP 判定是假的。
//! 不联网、不碰 `%LOCALAPPDATA%\ClaudeIpGate`、不读真实设置。

use super::*;
use std::sync::atomic::AtomicUsize;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

const ORIGIN: &str = "chrome-extension://abcdefghijklmnopabcdefghijklmnop";
const OTHER_ORIGIN: &str = "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

struct MemoryAssets;
impl AssetSource for MemoryAssets {
    fn get(&self, _: &str) -> Option<(String, Vec<u8>)> {
        None
    }
}

/// 每条测试一个临时目录，用完删掉。
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("qb-browser-link-{}", crate::config_io::id()));
        std::fs::create_dir_all(&path).unwrap();
        Dir(path)
    }
    fn file(&self) -> PathBuf {
        link_store::path(&self.0)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixed(ip: &str) -> Authority {
    let ip = ip.to_string();
    Arc::new(move || {
        let ip = ip.clone();
        Box::pin(async move { Judgement::Allowed { ip } })
    })
}

/// 出口可以随时改的假判定。
fn switchable(ip: &Arc<Mutex<String>>) -> Authority {
    let ip = ip.clone();
    Arc::new(move || {
        let ip = ip.lock().unwrap().clone();
        Box::pin(async move { Judgement::Allowed { ip } })
    })
}

fn config(dir: &Dir, authority: Authority) -> Config {
    Config {
        dir: dir.0.clone(),
        gate: Arc::new(GateState::default()),
        authority,
        assets: Arc::new(MemoryAssets),
        interval: Arc::new(|| Duration::from_millis(50)),
        monitor: false,
        events: None,
        // 单测绝不开真实数据库：站点路由走假站点。
        stations: Stations::Fixture(Arc::new(vec![browser_station::BrowserStation {
            id: "fixture-1".into(),
            name: "Fixture".into(),
            origin: "https://fixture.invalid".into(),
        }])),
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

struct Reply {
    code: u16,
    body: Value,
    headers: reqwest::header::HeaderMap,
}

async fn call(
    method: reqwest::Method,
    port: u16,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> std::result::Result<Reply, reqwest::Error> {
    let mut request = client()
        .request(method, format!("http://127.0.0.1:{port}{path}"))
        .header("User-Agent", UA);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await?;
    let code = response.status().as_u16();
    let headers = response.headers().clone();
    let text = response.text().await.unwrap_or_default();
    Ok(Reply {
        code,
        body: serde_json::from_str(&text).unwrap_or(Value::String(text)),
        headers,
    })
}

async fn post(
    port: u16,
    path: &str,
    origin: Option<&str>,
    bearer: Option<&str>,
    body: Value,
) -> Reply {
    let auth = bearer.map(|b| format!("Bearer {b}"));
    let mut headers = vec![];
    if let Some(origin) = origin {
        headers.push(("Origin", origin));
    }
    if let Some(auth) = auth.as_deref() {
        headers.push(("Authorization", auth));
    }
    call(reqwest::Method::POST, port, path, &headers, Some(body))
        .await
        .unwrap()
}

async fn get(port: u16, path: &str, headers: &[(&str, &str)]) -> Reply {
    call(reqwest::Method::GET, port, path, headers, None)
        .await
        .unwrap()
}

fn claim_code(start: &BrowserLinkStart) -> String {
    start
        .url
        .as_deref()
        .unwrap()
        .split("#c=")
        .nth(1)
        .unwrap()
        .to_string()
}

/// 假插件：配对编号与长期口令（插件自己存在 chrome.storage 里的那两样）。
struct Ext {
    origin: &'static str,
    pairing: String,
    token: String,
}

async fn claim_new(port: u16, code: &str, origin: &'static str) -> Reply {
    post(
        port,
        "/link/claim",
        Some(origin),
        None,
        json!({"claim": code, "extension_version": "0.2.0"}),
    )
    .await
}

async fn pair(link: &Link, origin: &'static str) -> Ext {
    let start = link.connect().await.unwrap();
    let code = claim_code(&start);
    let reply = claim_new(start.port, &code, origin).await;
    assert_eq!(reply.code, 200, "{}", reply.body);
    assert_eq!(reply.body["type"], "paired");
    assert_eq!(reply.body["port"], start.port);
    let token = reply.body["token"].as_str().unwrap().to_string();
    let pairing = reply.body["pairing"].as_str().unwrap().to_string();
    assert_eq!(
        reply.body["proof"],
        proof(&token, "claimed", &pairing, &code)
    );
    Ext {
        origin,
        pairing,
        token,
    }
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn socket(port: u16, origin: &str) -> Ws {
    let mut request = format!("ws://127.0.0.1:{port}/v2/events")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("origin", origin.parse().unwrap());
    tokio_tungstenite::connect_async(request).await.unwrap().0
}

/// 下一条文字消息；连接断了是 `None`。
async fn next(ws: &mut Ws) -> Option<Value> {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => return Some(serde_json::from_str(&text).unwrap()),
            Ok(Some(Ok(Message::Ping(_)))) | Ok(Some(Ok(Message::Pong(_)))) => continue,
            _ => return None,
        }
    }
}

async fn send(ws: &mut Ws, value: Value) {
    ws.send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}

struct Open {
    ws: Ws,
    session: String,
    connection: u64,
}

fn hello(ext: &Ext, nonce: &str) -> Value {
    json!({"type": "hello", "protocol": 2, "pairing": ext.pairing, "nonce": nonce, "version": "0.2.0"})
}

/// 按协议握手：先验桥的证明，再交自己的证明。
async fn open(port: u16, ext: &Ext) -> Open {
    let mut ws = socket(port, ext.origin).await;
    let nonce = token();
    send(&mut ws, hello(ext, &nonce)).await;
    let challenge = next(&mut ws).await.unwrap();
    assert_eq!(challenge["type"], "challenge", "{challenge}");
    let server_nonce = challenge["nonce"].as_str().unwrap().to_string();
    assert_eq!(
        challenge["proof"],
        proof(&ext.token, "server", &nonce, &server_nonce),
        "桥要先证明自己认得口令"
    );
    send(
        &mut ws,
        json!({"type": "authenticate", "proof": proof(&ext.token, "client", &nonce, &server_nonce)}),
    )
    .await;
    let welcome = next(&mut ws).await.unwrap();
    assert_eq!(welcome["type"], "welcome", "{welcome}");
    let status = next(&mut ws).await.unwrap();
    assert_eq!(status["type"], "status");
    assert_eq!(status["allow"], false);
    for value in [&challenge, &welcome, &status] {
        assert!(
            !value.to_string().contains(&ext.token),
            "配对之后长期口令不再上线路"
        );
    }
    Open {
        session: welcome["session"].as_str().unwrap().into(),
        connection: welcome["connection"].as_u64().unwrap(),
        ws,
    }
}

async fn shared_of(link: &Link) -> Arc<Shared> {
    link.slot.lock().await.as_ref().unwrap().shared.clone()
}

async fn port_of(link: &Link) -> u16 {
    link.status().await.port.unwrap()
}

async fn arm_http(port: u16, origin: &str, session: &str, body: Value) -> Reply {
    post(port, "/v2/arm", Some(origin), Some(session), body).await
}

async fn status_http(port: u16, origin: &str, session: &str) -> Reply {
    let auth = format!("Bearer {session}");
    get(
        port,
        "/v2/status",
        &[("Origin", origin), ("Authorization", &auth)],
    )
    .await
}

async fn until_allowed(ws: &mut Ws) -> Value {
    loop {
        let message = next(ws).await.expect("连接不该断");
        if message["allow"] == true {
            return message;
        }
    }
}

async fn wait_until(mut done: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

// ------------------------------------------------------------------ 原有十四条，按协议 2 改写成等价断言

#[tokio::test]
async fn arming_requires_an_authenticated_connection_and_a_changed_ip_cannot_be_rearmed_without_disarm(
) {
    let calls = Arc::new(AtomicUsize::new(0));
    let authority: Authority = {
        let calls = calls.clone();
        Arc::new(move || {
            let first = calls.fetch_add(1, Ordering::SeqCst) == 0;
            Box::pin(async move {
                Judgement::Allowed {
                    ip: if first { "203.0.113.1" } else { "203.0.113.2" }.into(),
                }
            })
        })
    };
    let dir = Dir::new();
    let link = Link::new(config(&dir, authority));
    let ext = pair(&link, ORIGIN).await;
    let port = port_of(&link).await;
    // 长期口令不是会话口令；没有活着的保护通道就开不了保护。
    assert_eq!(
        arm_http(port, ORIGIN, &ext.token, json!({})).await.code,
        401
    );
    let conn = open(port, &ext).await;
    let armed = arm_http(port, ORIGIN, &conn.session, json!({})).await;
    assert_eq!(armed.code, 200);
    assert_eq!(armed.body["allow"], true);
    assert_eq!(armed.body["armed"], true);
    assert_eq!(armed.body["baseline"], "203.0.113.1");
    monitor_check(&*shared_of(&link).await).await;
    let changed = status_http(port, ORIGIN, &conn.session).await.body;
    assert_eq!(changed["armed"], true);
    assert_eq!(changed["allow"], false);
    assert_eq!(changed["code"], "ip_changed");
    // 再点一次开启不会悄悄换基准。
    let again = arm_http(port, ORIGIN, &conn.session, json!({})).await.body;
    assert_eq!(again["allow"], false);
    assert_eq!(again["baseline"], "203.0.113.1");
    let off = post(
        port,
        "/v2/disarm",
        Some(ORIGIN),
        Some(&conn.session),
        json!({}),
    )
    .await;
    assert_eq!(off.body["armed"], false);
    let rearmed = arm_http(port, ORIGIN, &conn.session, json!({})).await.body;
    assert_eq!(rearmed["allow"], true);
    assert_eq!(rearmed["ip"], "203.0.113.2");
    assert_eq!(rearmed["baseline"], "203.0.113.2");
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn an_arm_queued_behind_a_probe_cannot_override_later_disarm() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&link, ORIGIN).await;
    let conn = open(port_of(&link).await, &ext).await;
    let shared = shared_of(&link).await;
    let guard = shared.check_lock.lock().await;
    let pending = shared.clone();
    let id = conn.connection;
    let mut task = tokio::spawn(async move { arm(&pending, id, None).await });
    assert!(tokio::time::timeout(Duration::from_millis(50), &mut task)
        .await
        .is_err());
    disarm(&shared, id).unwrap();
    drop(guard);
    let result = task.await.unwrap().unwrap();
    assert_eq!(result["armed"], false);
    assert_eq!(result["allow"], false);
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn a_late_successful_probe_cannot_undo_explicit_disarm() {
    let entered = Arc::new(Notify::new());
    let resume = Arc::new(Notify::new());
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
    let dir = Dir::new();
    let link = Link::new(config(&dir, authority));
    let ext = pair(&link, ORIGIN).await;
    let conn = open(port_of(&link).await, &ext).await;
    let shared = shared_of(&link).await;
    let pending = shared.clone();
    let id = conn.connection;
    let probe = tokio::spawn(async move { arm(&pending, id, None).await });
    entered.notified().await;
    disarm(&shared, id).unwrap();
    resume.notify_one();
    let status = probe.await.unwrap().unwrap();
    assert_eq!(status["armed"], false);
    assert_eq!(status["allow"], false);
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn loopback_requests_require_host_origin_and_session_and_failures_change_nothing() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&link, ORIGIN).await;
    let port = port_of(&link).await;
    let conn = open(port, &ext).await;
    let good = format!("Bearer {}", conn.session);
    let id = extension_id(ORIGIN).unwrap();
    for (headers, expected) in [
        (
            vec![
                ("Origin", "https://example.invalid"),
                ("Authorization", good.as_str()),
            ],
            403,
        ),
        (
            vec![("Origin", ORIGIN), ("Authorization", "Bearer wrong")],
            401,
        ),
        (
            vec![
                ("Host", "malicious.invalid"),
                ("Origin", ORIGIN),
                ("Authorization", good.as_str()),
            ],
            421,
        ),
        (
            vec![("Origin", OTHER_ORIGIN), ("Authorization", good.as_str())],
            403,
        ),
    ] {
        assert_eq!(get(port, "/v2/status", &headers).await.code, expected);
    }
    // 没有 Origin 时，长期口令换不来任何东西（会话口令只在认证过的保护通道里发）。
    let long_lived = format!("Bearer {}", ext.token);
    let reply = get(
        port,
        "/v2/status",
        &[("X-QB-Extension", id), ("Authorization", &long_lived)],
    )
    .await;
    assert_eq!(reply.code, 401, "missing Origin cannot bootstrap anything");
    let reply = get(
        port,
        "/v2/status",
        &[("Origin", ORIGIN), ("Authorization", &good)],
    )
    .await;
    assert_eq!(reply.code, 200);
    assert_eq!(reply.headers["access-control-allow-origin"], ORIGIN);
    let status = link.status().await;
    assert_eq!(status.pairings.len(), 1, "失败的请求不会多出配对");
    assert!(status.pairings[0].connected);
    assert_eq!(shared_of(&link).await.inner.lock().unwrap().conns.len(), 1);
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn originless_chrome_get_needs_a_live_session_and_disconnect_is_not_disarm() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&link, ORIGIN).await;
    let port = port_of(&link).await;
    let mut conn = open(port, &ext).await;
    let auth = format!("Bearer {}", conn.session);
    let reply = get(
        port,
        "/v2/status",
        &[
            ("X-QB-Extension", extension_id(ORIGIN).unwrap()),
            ("Authorization", &auth),
        ],
    )
    .await;
    assert_eq!(reply.code, 200);
    assert_eq!(
        arm_http(port, ORIGIN, &conn.session, json!({})).await.body["allow"],
        true
    );
    conn.ws.close(None).await.unwrap();
    let shared = shared_of(&link).await;
    wait_until(|| shared.inner.lock().unwrap().conns.is_empty()).await;
    let pairing = &link.status().await.pairings[0];
    assert!(!pairing.connected);
    assert!(
        pairing.protection.armed,
        "a lost connection never masquerades as explicit disarm"
    );
    assert!(!pairing.protection.allow);
    assert_eq!(pairing.protection.code, ProtectionCode::Disconnected);
    assert!(pairing.protection.reason.contains("断开"));
    assert_eq!(status_http(port, ORIGIN, &conn.session).await.code, 401);
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn a_wrong_proof_or_unknown_pairing_is_rejected_before_any_connection_is_registered() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&link, ORIGIN).await;
    let port = port_of(&link).await;
    let shared = shared_of(&link).await;

    let mut ws = socket(port, ORIGIN).await;
    let nonce = token();
    send(&mut ws, hello(&ext, &nonce)).await;
    let challenge = next(&mut ws).await.unwrap();
    assert_eq!(challenge["type"], "challenge");
    send(
        &mut ws,
        json!({"type": "authenticate", "proof": proof("0".repeat(64).as_str(), "client", &nonce, challenge["nonce"].as_str().unwrap())}),
    )
    .await;
    let rejected = next(&mut ws).await.unwrap();
    assert_eq!(rejected["type"], "rejected");
    assert_eq!(rejected["code"], "bad_proof");
    assert!(next(&mut ws).await.is_none());

    // 不认识的配对、别的扩展拿着这个配对编号、协议版本对不上：一律在发证明之前拒掉。
    let stranger = Ext {
        origin: ORIGIN,
        pairing: "0123456789abcdef".into(),
        token: token(),
    };
    let impostor = Ext {
        origin: OTHER_ORIGIN,
        pairing: ext.pairing.clone(),
        token: ext.token.clone(),
    };
    for (who, code) in [
        (&stranger, "unknown_pairing"),
        (&impostor, "unknown_pairing"),
    ] {
        let mut ws = socket(port, who.origin).await;
        send(&mut ws, hello(who, &token())).await;
        let reply = next(&mut ws).await.unwrap();
        assert_eq!(reply["type"], "rejected");
        assert_eq!(reply["code"], code);
    }
    let mut ws = socket(port, ORIGIN).await;
    let mut old = hello(&ext, &token());
    old["protocol"] = json!(1);
    send(&mut ws, old).await;
    assert_eq!(next(&mut ws).await.unwrap()["code"], "protocol_mismatch");

    assert!(shared.inner.lock().unwrap().conns.is_empty());
    assert!(!link.status().await.pairings[0].connected);
    // 连接被拒不影响配对本身：正确的证明照样连得上。
    open(port, &ext).await;
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn stop_cancels_and_joins_an_in_flight_authenticated_arm_request() {
    let entered = Arc::new(Notify::new());
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
    let dir = Dir::new();
    let link = Link::new(config(&dir, authority));
    let ext = pair(&link, ORIGIN).await;
    let port = port_of(&link).await;
    let conn = open(port, &ext).await;
    let shared = shared_of(&link).await;
    let session = conn.session.clone();
    let request = tokio::spawn(async move {
        call(
            reqwest::Method::POST,
            port,
            "/v2/arm",
            &[
                ("Origin", ORIGIN),
                ("Authorization", &format!("Bearer {session}")),
            ],
            Some(json!({})),
        )
        .await
    });
    entered.notified().await;
    link.stop(Ending::PanelExit).await;
    assert!(
        dropped.load(Ordering::SeqCst),
        "stop must join the authenticated request, not leave it detached"
    );
    assert!(request.await.unwrap().is_err());
    assert!(route(
        &shared,
        conn.connection,
        Method::POST,
        "/v2/disarm",
        json!({})
    )
    .await
    .is_err());
    assert!(route(
        &shared,
        conn.connection,
        Method::POST,
        "/v2/probe/enhancement",
        json!({})
    )
    .await
    .is_err());
}

#[tokio::test]
async fn explicit_stop_sends_offline_closes_listener_and_never_reports_disarmed() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&link, ORIGIN).await;
    let port = port_of(&link).await;
    let mut conn = open(port, &ext).await;
    assert_eq!(
        arm_http(port, ORIGIN, &conn.session, json!({})).await.body["allow"],
        true
    );
    // 开启过程里推过一条「正在核验」（那时还没开）；读到放行为止，之后的每一条都必须是开着的。
    until_allowed(&mut conn.ws).await;
    link.stop(Ending::PanelExit).await;
    assert!(
        call(reqwest::Method::GET, port, "/v2/status", &[], None)
            .await
            .is_err(),
        "监听已关"
    );
    let mut offline = None;
    while let Some(message) = next(&mut conn.ws).await {
        match message["type"].as_str() {
            Some("status") => assert_eq!(message["armed"], true, "停止不冒充使用者关了保护"),
            Some("offline") => offline = Some(message),
            other => panic!("意外的消息：{other:?}"),
        }
    }
    let offline = offline.expect("面板正常退出要先发「面板下线」");
    assert_eq!(offline["code"], "panel_exit");
    let status = link.status().await;
    assert!(!status.listening);
    assert_eq!(status.pairings.len(), 1, "停止监听不等于解除配对");
}

#[tokio::test]
async fn generation_and_staleness_fail_closed_and_cannot_become_safe_by_reading_again() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&link, ORIGIN).await;
    let port = port_of(&link).await;
    let conn = open(port, &ext).await;
    assert_eq!(
        arm_http(port, ORIGIN, &conn.session, json!({})).await.body["allow"],
        true
    );
    let shared = shared_of(&link).await;
    shared.config.gate.generation.fetch_add(1, Ordering::SeqCst);
    let after = shared.conn_status(conn.connection).unwrap();
    assert_eq!(after["allow"], false);
    assert_eq!(after["code"], "config_changed");
    shared.config.gate.generation.store(0, Ordering::SeqCst);
    assert_eq!(shared.conn_status(conn.connection).unwrap()["allow"], false);
    {
        let mut inner = shared.inner.lock().unwrap();
        let p = &mut inner.conns.get_mut(&conn.connection).unwrap().protection;
        p.safe = true;
        p.code = ProtectionCode::Allowed;
        p.checked = Some(Instant::now() - STALE_AFTER - Duration::from_secs(1));
    }
    let stale = shared.conn_status(conn.connection).unwrap();
    assert_eq!(stale["allow"], false);
    assert_eq!(stale["code"], "stale");
    shared
        .config
        .gate
        .manual_paused
        .store(true, Ordering::SeqCst);
    {
        let mut inner = shared.inner.lock().unwrap();
        let p = &mut inner.conns.get_mut(&conn.connection).unwrap().protection;
        p.safe = true;
        p.code = ProtectionCode::Allowed;
        p.checked = Some(Instant::now());
    }
    assert_eq!(
        shared.conn_status(conn.connection).unwrap()["code"],
        "manual_paused"
    );
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn oversized_request_is_rejected_without_retaining_body() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&link, ORIGIN).await;
    let port = port_of(&link).await;
    let conn = open(port, &ext).await;
    let response = client()
        .post(format!("http://127.0.0.1:{port}/v2/probe/enhancement"))
        .header("Origin", ORIGIN)
        .bearer_auth(&conn.session)
        .body("x".repeat(MAX_BODY + 1))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 413);
    assert!(link.status().await.pairings[0].enhancement_json.is_none());
    // 合格的那份记在这个浏览器名下。
    let ok = post(
        port,
        "/v2/probe/enhancement",
        Some(ORIGIN),
        Some(&conn.session),
        json!({"proxy_mode": "system", "webrtc_policy": "default", "proxy_control": null}),
    )
    .await;
    assert_eq!(ok.code, 200);
    assert!(link.status().await.pairings[0].enhancement_json.is_some());
    link.stop(Ending::PanelExit).await;
}

#[test]
fn a_different_allowlisted_ip_still_triggers_browser_protection() {
    let allowed = Judgement::Allowed {
        ip: "203.0.113.2".into(),
    };
    assert!(!verdict_result(&allowed, Some("203.0.113.1")).safe);
    assert_eq!(
        verdict_result(&allowed, Some("203.0.113.1")).code,
        ProtectionCode::IpChanged
    );
    assert!(verdict_result(&allowed, Some("203.0.113.2")).safe);
    assert!(!verdict_result(&Judgement::IpUnknown, None).safe);
    assert_eq!(
        verdict_result(&Judgement::IpUnknown, None).code,
        ProtectionCode::IpUnknown
    );
    // 同一个 IPv6 地址的两种写法不算换了出口。
    let v6 = Judgement::Allowed {
        ip: "2001:db8::1".into(),
    };
    assert!(verdict_result(&v6, Some("2001:0db8:0000::0001")).safe);
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
        "chrome-extension://ABCDEFGHIJKLMNOPABCDEFGHIJKLMNOP",
    ] {
        assert!(extension_id(origin).is_none(), "{origin}");
    }
}

#[test]
fn tokens_rotate_and_match_without_prefix_acceptance() {
    let a = token();
    let b = token();
    assert_eq!(a.len(), 64);
    assert!(is_hex(&a, 64));
    assert_ne!(a, b);
    assert!(token_matches(&a, &a));
    assert!(!token_matches(&a, &b));
    assert!(!token_matches(&a, &a[..63]));
    assert!(is_hex(&pairing_id(), 16));
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

// ------------------------------------------------------------------ 新增：配对、端口、多浏览器、认领

#[tokio::test]
async fn nothing_listens_until_a_browser_is_paired() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let status = link.resume().await;
    assert!(!status.listening);
    assert!(status.port.is_none());
    assert!(status.pairings.is_empty());
    assert!(!dir.file().exists(), "没配对过不留任何文件");
    assert!(!link.status().await.listening);
    // 只发了认领码、没人认领：照样不留文件。
    link.connect().await.unwrap();
    assert!(!dir.file().exists());
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn a_pairing_survives_reopening_and_its_token_is_sealed_on_disk() {
    let dir = Dir::new();
    let first = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&first, ORIGIN).await;
    let port = port_of(&first).await;
    let saved = std::fs::read_to_string(dir.file()).unwrap();
    assert!(saved.contains(&ext.pairing));
    assert!(!saved.contains(&ext.token), "口令加密落盘，不留明文");
    first.stop(Ending::PanelExit).await;

    let reopened = Link::new(config(&dir, fixed("203.0.113.1")));
    let status = reopened.resume().await;
    assert!(status.listening, "有配对就随面板启动监听");
    assert_eq!(status.port, Some(port), "端口落盘复用");
    assert!(!status.relink_needed);
    assert_eq!(status.pairings[0].pairing_id, ext.pairing);
    assert_eq!(status.pairings[0].label, "Chrome 131");
    let conn = open(port, &ext).await;
    let armed = arm_http(port, ORIGIN, &conn.session, json!({})).await;
    assert_eq!(armed.body["allow"], true, "不用重新配对");
    reopened.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn unpairing_cuts_the_connection_at_once_and_the_last_one_stops_listening() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let a = pair(&link, ORIGIN).await;
    let b = pair(&link, OTHER_ORIGIN).await;
    let port = port_of(&link).await;
    let mut ca = open(port, &a).await;
    let mut cb = open(port, &b).await;

    let status = link.unpair(&a.pairing).await.unwrap();
    assert_eq!(status.pairings.len(), 1);
    assert!(status.listening);
    let mut revoked = false;
    while let Some(message) = next(&mut ca.ws).await {
        revoked |= message["type"] == "revoked";
    }
    assert!(revoked, "连着的那一个立刻收到撤销");
    assert_eq!(status_http(port, ORIGIN, &ca.session).await.code, 401);
    let mut ws = socket(port, ORIGIN).await;
    send(&mut ws, hello(&a, &token())).await;
    assert_eq!(next(&mut ws).await.unwrap()["code"], "unknown_pairing");
    assert_eq!(
        status_http(port, OTHER_ORIGIN, &cb.session).await.code,
        200,
        "另一个不受影响"
    );
    assert!(link.unpair(&a.pairing).await.is_err());

    let status = link.unpair(&b.pairing).await.unwrap();
    assert!(!status.listening, "最后一条解除就停止监听");
    assert!(status.pairings.is_empty());
    let mut revoked = false;
    while let Some(message) = next(&mut cb.ws).await {
        revoked |= message["type"] == "revoked";
    }
    assert!(revoked);
    assert!(tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .is_err());
    assert!(!dir.file().exists());
    // 不在监听时也能解除（读盘、改盘）。
    let c = pair(&link, ORIGIN).await;
    link.stop(Ending::PanelExit).await;
    let status = link.unpair(&c.pairing).await.unwrap();
    assert!(status.pairings.is_empty());
    assert!(!dir.file().exists());
}

#[tokio::test]
async fn an_occupied_port_is_replaced_and_relinking_teaches_the_new_one() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&link, ORIGIN).await;
    let old = port_of(&link).await;
    link.stop(Ending::PanelExit).await;
    let occupant = std::net::TcpListener::bind(("127.0.0.1", old)).unwrap();

    let status = link.resume().await;
    let new = status.port.unwrap();
    assert_ne!(new, old, "旧端口被占用就让系统另挑");
    assert!(status.relink_needed);
    assert!(status.pairings[0].needs_relink);

    let start = link.reconnect().await.unwrap();
    assert!(!start.rebound, "旧端口还被占着，拿不回来");
    assert_eq!(start.port, new);
    let code = claim_code(&start);
    let reply = post(
        new,
        "/link/claim",
        Some(ORIGIN),
        None,
        json!({"claim": code, "pairing": ext.pairing, "proof": proof(&ext.token, "claim", &ext.pairing, &code)}),
    )
    .await;
    assert_eq!(reply.code, 200, "{}", reply.body);
    assert_eq!(reply.body["type"], "relinked");
    assert_eq!(reply.body["port"], new);
    assert!(reply.body.get("token").is_none(), "重连不再发口令");
    assert_eq!(
        reply.body["proof"],
        proof(&ext.token, "claimed", &ext.pairing, &code),
        "桥证明自己认得这个配对，插件才改记新端口"
    );
    let status = link.status().await;
    assert!(!status.relink_needed);
    assert!(!status.pairings[0].needs_relink);
    open(new, &ext).await;
    let replay = claim_new(new, &code, ORIGIN).await;
    assert_eq!(replay.code, 410);
    assert_eq!(replay.body["code"], "claim_used");
    link.stop(Ending::PanelExit).await;
    drop(occupant);

    // 下次启动先试插件们最近一次被告知的那个（新）端口。
    let status = link.resume().await;
    assert_eq!(status.port, Some(new));
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn reconnect_regains_the_original_port_once_it_is_free() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&link, ORIGIN).await;
    let old = port_of(&link).await;
    link.stop(Ending::PanelExit).await;
    let occupant = std::net::TcpListener::bind(("127.0.0.1", old)).unwrap();
    let moved = link.resume().await.port.unwrap();
    assert_ne!(moved, old);
    drop(occupant);

    let start = link.reconnect().await.unwrap();
    assert!(start.rebound, "旧端口空出来了：拿回来，插件自己就找得到");
    assert_eq!(start.port, old);
    let status = link.status().await;
    assert_eq!(status.port, Some(old));
    assert!(!status.relink_needed);
    open(old, &ext).await;
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn two_browsers_connect_at_once_and_keep_separate_protection() {
    let ip = Arc::new(Mutex::new("203.0.113.1".to_string()));
    let dir = Dir::new();
    let link = Link::new(config(&dir, switchable(&ip)));
    // 同一个插件（扩展 ID 固定）装在两个 Chrome 资料里：两条配对。
    let a = pair(&link, ORIGIN).await;
    let b = pair(&link, ORIGIN).await;
    assert_ne!(a.pairing, b.pairing);
    let port = port_of(&link).await;
    let ca = open(port, &a).await;
    let cb = open(port, &b).await;
    assert_ne!(ca.session, cb.session);

    assert_eq!(
        arm_http(port, ORIGIN, &ca.session, json!({})).await.body["allow"],
        true
    );
    *ip.lock().unwrap() = "203.0.113.2".into();
    monitor_check(&*shared_of(&link).await).await;
    let sa = status_http(port, ORIGIN, &ca.session).await.body;
    let sb = status_http(port, ORIGIN, &cb.session).await.body;
    assert_eq!(sa["code"], "ip_changed");
    assert_eq!(sb["code"], "not_armed");

    assert_eq!(
        arm_http(port, ORIGIN, &cb.session, json!({})).await.body["allow"],
        true
    );
    post(
        port,
        "/v2/disarm",
        Some(ORIGIN),
        Some(&ca.session),
        json!({}),
    )
    .await;
    assert_eq!(
        status_http(port, ORIGIN, &cb.session).await.body["allow"],
        true,
        "互不影响"
    );
    let status = link.status().await;
    assert_eq!(status.pairings.len(), 2);
    assert!(status.pairings.iter().all(|p| p.connected));
    let protections: Vec<_> = status.pairings.iter().map(|p| p.protection.allow).collect();
    assert!(protections.contains(&true) && protections.contains(&false));
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn a_resume_with_a_different_baseline_blocks_until_rearmed() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.2")));
    let ext = pair(&link, ORIGIN).await;
    let port = port_of(&link).await;
    let conn = open(port, &ext).await;
    // 插件记得上次放行时是 .1；面板重启之后现在是 .2。
    let resumed = arm_http(
        port,
        ORIGIN,
        &conn.session,
        json!({"baseline": "203.0.113.1"}),
    )
    .await;
    assert_eq!(resumed.code, 200);
    assert_eq!(resumed.body["allow"], false);
    assert_eq!(resumed.body["armed"], true);
    assert_eq!(resumed.body["code"], "ip_changed");
    assert_eq!(resumed.body["baseline"], "203.0.113.1");
    let other_dir = Dir::new();
    let same = Link::new(config(&other_dir, fixed("203.0.113.2")));
    let other = pair(&same, ORIGIN).await;
    let other_port = port_of(&same).await;
    let other_conn = open(other_port, &other).await;
    let ok = arm_http(
        other_port,
        ORIGIN,
        &other_conn.session,
        json!({"baseline": "203.0.113.2"}),
    )
    .await;
    assert_eq!(ok.body["allow"], true, "出口没变就直接恢复放行");
    let bad = arm_http(
        other_port,
        ORIGIN,
        &other_conn.session,
        json!({"baseline": "not-an-ip"}),
    )
    .await;
    assert_eq!(bad.code, 400);
    link.stop(Ending::PanelExit).await;
    same.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn claim_codes_expire_cannot_be_replayed_and_only_extensions_may_claim() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let start = link.connect().await.unwrap();
    let port = start.port;
    let code = claim_code(&start);
    let page = format!("http://127.0.0.1:{port}");
    let state = |code: String| {
        let page = page.clone();
        async move {
            post(
                port,
                "/link/state",
                Some(&page),
                None,
                json!({"claim": code}),
            )
            .await
            .body
        }
    };

    assert_eq!(
        claim_new(port, &code, "https://evil.example").await.code,
        403
    );
    let originless = post(port, "/link/claim", None, None, json!({"claim": code})).await;
    assert_eq!(originless.code, 403);
    assert_eq!(state(code.clone()).await["state"], "pending");
    let unknown = post(
        port,
        "/link/claim",
        Some(ORIGIN),
        None,
        json!({"claim": code, "pairing": "0123456789abcdef", "proof": "0".repeat(64)}),
    )
    .await;
    assert_eq!(unknown.code, 404);
    assert_eq!(unknown.body["code"], "unknown_pairing");
    assert_eq!(
        state(code.clone()).await["state"],
        "pending",
        "认不出的重连不消耗认领码"
    );

    let reply = claim_new(port, &code, ORIGIN).await;
    assert_eq!(reply.code, 200);
    let ext = Ext {
        origin: ORIGIN,
        pairing: reply.body["pairing"].as_str().unwrap().into(),
        token: reply.body["token"].as_str().unwrap().into(),
    };
    let claimed = state(code.clone()).await;
    assert_eq!(claimed["state"], "claimed");
    assert_eq!(claimed["browser"], "Chrome 131");
    assert_eq!(
        claim_new(port, &code, ORIGIN).await.body["code"],
        "claim_used"
    );

    let second = claim_code(&link.connect().await.unwrap());
    let wrong = post(
        port,
        "/link/claim",
        Some(ORIGIN),
        None,
        json!({"claim": second, "pairing": ext.pairing, "proof": proof(&token(), "claim", &ext.pairing, &second)}),
    )
    .await;
    assert_eq!(wrong.code, 401);
    assert_eq!(wrong.body["code"], "bad_proof");
    assert_eq!(state(second.clone()).await["state"], "pending");
    {
        let shared = shared_of(&link).await;
        let mut inner = shared.inner.lock().unwrap();
        let at = find_claim(&inner.claims, &second).unwrap();
        inner.claims[at].expires = Instant::now();
    }
    let late = claim_new(port, &second, ORIGIN).await;
    assert_eq!(late.code, 410);
    assert_eq!(late.body["code"], "claim_expired");
    assert_eq!(state(second.clone()).await["state"], "expired");
    assert_eq!(state(token()).await["state"], "invalid");
    let foreign = post(
        port,
        "/link/state",
        Some("https://evil.example"),
        None,
        json!({"claim": second}),
    )
    .await;
    assert_eq!(foreign.code, 403, "认领页面的查询只认同源");
    assert_eq!(link.status().await.pairings.len(), 1);
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn the_pairing_limit_is_reported_on_the_page() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    for _ in 0..MAX_PAIRINGS {
        pair(&link, ORIGIN).await;
    }
    let start = link.connect().await.unwrap();
    let code = claim_code(&start);
    let reply = claim_new(start.port, &code, ORIGIN).await;
    assert_eq!(reply.code, 409);
    assert_eq!(reply.body["code"], "too_many_pairings");
    let page = format!("http://127.0.0.1:{}", start.port);
    let state = post(
        start.port,
        "/link/state",
        Some(&page),
        None,
        json!({"claim": code}),
    )
    .await
    .body;
    assert_eq!(state["state"], "pending");
    assert!(state["problem"].as_str().unwrap().contains("解除"));
    assert!(link.status().await.claim.unwrap().problem.is_some());
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn a_late_start_cannot_override_a_newer_stop() {
    let dir = Dir::new();
    {
        let first = Link::new(config(&dir, fixed("203.0.113.1")));
        pair(&first, ORIGIN).await;
        first.stop(Ending::PanelExit).await;
    }
    let link = Arc::new(Link::new(config(&dir, fixed("203.0.113.1"))));
    let guard = link.slot.lock().await;
    let resuming = {
        let link = link.clone();
        tokio::spawn(async move { link.resume().await })
    };
    let connecting = {
        let link = link.clone();
        tokio::spawn(async move { link.connect().await })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    let stopping = {
        let link = link.clone();
        tokio::spawn(async move { link.stop(Ending::PanelExit).await })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(guard);
    assert!(
        !resuming.await.unwrap().listening,
        "排在停止之前请求、之后才轮到的启动作废"
    );
    assert!(connecting.await.unwrap().is_err());
    stopping.await.unwrap();
    assert!(!link.status().await.listening);
    // 停止之后新的一次启动照常。
    assert!(link.resume().await.listening);
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn the_link_page_is_served_strictly_and_old_paths_are_gone() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let port = link.connect().await.unwrap().port;
    let html = get(port, "/link", &[]).await;
    assert_eq!(html.code, 200);
    assert!(html.headers["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/html"));
    let csp = html.headers["content-security-policy"].to_str().unwrap();
    assert!(csp.contains("script-src 'self'") && csp.contains("frame-ancestors 'none'"));
    assert_eq!(html.headers["x-frame-options"], "DENY");
    assert!(html
        .body
        .as_str()
        .unwrap()
        .contains("data-qb-gate-link=\"2\""));
    assert_eq!(
        get(port, "/link?c=00", &[]).await.code,
        421,
        "认领码不许进请求行"
    );
    assert_eq!(
        get(port, "/link", &[("Host", "evil.example:80")])
            .await
            .code,
        421
    );
    let script = get(port, "/link/app.js", &[]).await;
    assert!(script.headers["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/javascript"));
    assert!(get(port, "/link/tokens.css", &[])
        .await
        .body
        .as_str()
        .unwrap()
        .contains("--bg"));
    let old = get(port, "/v1/status", &[("Origin", ORIGIN)]).await;
    assert_eq!(old.code, 410);
    assert_eq!(old.body["code"], "protocol_outdated");
    assert_eq!(
        get(port, "/v1/events", &[("Origin", ORIGIN)]).await.code,
        410
    );
    assert_eq!(get(port, "/elsewhere", &[]).await.code, 404);
    let preflight = call(
        reqwest::Method::OPTIONS,
        port,
        "/link/claim",
        &[("Origin", ORIGIN)],
        None,
    )
    .await
    .unwrap();
    assert_eq!(preflight.code, 204);
    assert_eq!(preflight.headers["access-control-allow-origin"], ORIGIN);
    link.stop(Ending::PanelExit).await;
}

#[test]
fn hmac_matches_the_rfc_4231_vectors_and_roles_do_not_mix() {
    assert_eq!(
        hex::encode(hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    assert_eq!(
        hex::encode(hmac_sha256(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First"
        )),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
    let t = token();
    let (a, b) = (token(), token());
    let roles = ["server", "client", "claim", "claimed"].map(|r| proof(&t, r, &a, &b));
    for (i, x) in roles.iter().enumerate() {
        assert!(is_hex(x, 64));
        for y in &roles[i + 1..] {
            assert_ne!(x, y, "一种用途的证明换不成另一种");
        }
    }
    assert_eq!(browser_label(Some(UA)), "Chrome 131");
    assert_eq!(
        browser_label(Some(
            "Mozilla/5.0 Chrome/131.0.0.0 Safari/537.36 Edg/131.0.2903.70"
        )),
        "Edge 131"
    );
    assert_eq!(browser_label(None), "Chromium 内核浏览器");
}

#[tokio::test]
async fn a_status_change_is_pushed_without_waiting_for_the_heartbeat() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&link, ORIGIN).await;
    let port = port_of(&link).await;
    let mut conn = open(port, &ext).await;
    let session = conn.session.clone();
    let started = Instant::now();
    tokio::spawn(async move { arm_http(port, ORIGIN, &session, json!({})).await });
    let pushed = tokio::time::timeout(Duration::from_millis(1500), until_allowed(&mut conn.ws))
        .await
        .expect("放行要立刻推给插件，不等两秒一次的心跳");
    assert!(started.elapsed() < HEARTBEAT);
    assert_eq!(pushed["code"], "allowed");
    assert!(pushed["seq"].as_u64().unwrap() > 1);
    assert_eq!(pushed["ttl_ms"], ALLOW_TTL.as_millis() as u64);
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn an_abrupt_drop_sends_no_offline_message() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&link, ORIGIN).await;
    let port = port_of(&link).await;
    let mut conn = open(port, &ext).await;
    shared_of(&link)
        .await
        .socket_tasks
        .lock()
        .unwrap()
        .abort_all();
    while let Some(message) = next(&mut conn.ws).await {
        assert_ne!(message["type"], "offline", "异常断开没有下线消息");
    }
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn a_second_connection_for_the_same_pairing_replaces_the_first() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&link, ORIGIN).await;
    let port = port_of(&link).await;
    let mut first = open(port, &ext).await;
    let second = open(port, &ext).await;
    let mut replaced = false;
    while let Some(message) = next(&mut first.ws).await {
        replaced |= message["type"] == "replaced";
    }
    assert!(replaced);
    assert_eq!(status_http(port, ORIGIN, &first.session).await.code, 401);
    assert_eq!(status_http(port, ORIGIN, &second.session).await.code, 200);
    let shared = shared_of(&link).await;
    wait_until(|| shared.inner.lock().unwrap().conns.len() == 1).await;
    assert!(link.status().await.pairings[0].connected);
    link.stop(Ending::PanelExit).await;
}

#[tokio::test]
async fn fixture_station_routes_never_touch_a_database() {
    let dir = Dir::new();
    let link = Link::new(config(&dir, fixed("203.0.113.1")));
    let ext = pair(&link, ORIGIN).await;
    let port = port_of(&link).await;
    let conn = open(port, &ext).await;
    let auth = format!("Bearer {}", conn.session);
    let list = get(
        port,
        "/v2/stations",
        &[("Origin", ORIGIN), ("Authorization", &auth)],
    )
    .await;
    assert_eq!(list.code, 200);
    assert_eq!(list.body[0]["origin"], "https://fixture.invalid");
    let connect = |origin: &str, captured: Value| json!({"station_id": "fixture-1", "origin": origin, "backend": "newapi", "captured": captured, "cookie": ""});
    let ok = post(
        port,
        "/v2/station/connect",
        Some(ORIGIN),
        Some(&conn.session),
        connect(
            "https://fixture.invalid",
            json!([{"key": "token", "value": "fixture"}]),
        ),
    )
    .await;
    assert_eq!(ok.code, 200, "{}", ok.body);
    assert_eq!(ok.body["configured"], true);
    let other = post(
        port,
        "/v2/station/connect",
        Some(ORIGIN),
        Some(&conn.session),
        connect(
            "https://elsewhere.invalid",
            json!([{"key": "token", "value": "fixture"}]),
        ),
    )
    .await;
    assert_eq!(other.code, 400, "跨站导入照样拒绝");
    let empty = post(
        port,
        "/v2/station/connect",
        Some(ORIGIN),
        Some(&conn.session),
        connect("https://fixture.invalid", json!([])),
    )
    .await;
    assert_eq!(empty.code, 400);
    let prices = post(
        port,
        "/v2/station/pricing",
        Some(ORIGIN),
        Some(&conn.session),
        json!({"station_id": "fixture-1", "origin": "https://fixture.invalid", "backend": "newapi",
            "pricing": {"data": [{"model_name": "claude-opus-5", "model_ratio": 2.5, "completion_ratio": 5}],
                "group_ratio": {"default": 0.2}}}),
    )
    .await;
    assert_eq!(prices.code, 200, "{}", prices.body);
    assert_eq!(prices.body["models"][0]["prices_per_mtok"][0], 1.0);
    link.stop(Ending::PanelExit).await;
}

/// 固定向量：插件那一侧用 WebCrypto（`crypto.subtle` 的 HMAC-SHA-256）算出来的就是这四个数。
/// 设计说明「浏览器助手连接协议」一节抄的是同一组，插件的单测照着断言。
#[test]
fn proofs_match_the_documented_webcrypto_vectors() {
    let token = "0123456789abcdef".repeat(4);
    let (a, b) = ("a".repeat(64), "b".repeat(64));
    assert_eq!(
        proof(&token, "server", &a, &b),
        "1763e994b1beed96ee368aea557a2a0cf15065dd5b16e73ec2c12275432ffc99"
    );
    assert_eq!(
        proof(&token, "client", &a, &b),
        "442e2ed3d3898870766d04d3da58681ab6c715b6cc6db65633aa0965e9ad7e9b"
    );
    let claim = "c".repeat(64);
    assert_eq!(
        proof(&token, "claim", "0123456789abcdef", &claim),
        "5eb5a5960c281708313c0b758e614ef42d55c17abfd2fb203fe6db266f6f529e"
    );
    assert_eq!(
        proof(&token, "claimed", "0123456789abcdef", &claim),
        "4d26b086c90ffd9ae1cfaeacf6873686998cf209bca847a94db4c66b382ba83a"
    );
}
