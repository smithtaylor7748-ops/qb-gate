//! 真实浏览器采集：在 127.0.0.1 上开一个**一次性**的小服务，让系统默认浏览器打开
//! `probe.html`，把那个浏览器里采到的东西交回面板（2026-09-24）。
//!
//! 参照 [CheckClaude](https://github.com/zzusec/CheckClaude) 的 BrowserBridge（MIT，© 2026 zzusec）
//! 的做法 —— 随机令牌、只绑回环、采完就关 —— 代码自己写。
//!
//! # 为什么需要
//!
//! 「中文环境」那十项原来在面板内置的 WebView2 里算。那不是使用者登 claude.ai 的浏览器：
//! 语言列表、WebRTC 策略、扩展都可能不一样。`Accept-Language` 这类请求头更是只有服务端看得到。
//!
//! # 边界（每一条都是故意的）
//!
//! - **只绑 `127.0.0.1:0`**，端口让系统挑（CLAUDE.md「别写死端口」）；本机路由那条「只绑回环」同一个理由。
//! - 路径带一个随机令牌（UUID v4，122 位随机数）。页面与交回结果都要带它；静态资源（`/assets/*`，
//!   公开的构建产物）只在这一次采集活着的时候给。别的路径一律 404。
//! - 校验 `Host` 头是 `127.0.0.1:<端口>` / `localhost:<端口>`：挡 DNS 重绑定 —— 别的网页借一个
//!   解析到 127.0.0.1 的域名来打这个端口，`Host` 对不上。
//! - 结果体 ≤ 64 KB，必须是 JSON；**只收一份**，收到就关监听；120 秒没人交回也关。
//! - 结果只放进程内存（[`last_report`]），不落盘、不进日志 —— 语言、时区、WebRTC 地址都算个人信息。
//! - 这一层不认识 Tauri（分层测试卡着）：页面从哪来由 [`AssetSource`] 决定，正式包里是
//!   `src-tauri` 用内嵌资源实现的那一个。

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use ts_rs::TS;

use crate::error::{GateError, Result};

/// 交回结果的上限。十项原始值 + 几个短字符串，远远用不到。
pub const MAX_REPORT: usize = 64 * 1024;
/// 多久没人交回就关。
pub const TIMEOUT: Duration = Duration::from_secs(120);
/// 报告多新才算数（界面与出口一致性那几行只认这么新的）。
pub const FRESH_FOR: Duration = Duration::from_secs(60 * 60);

/// 页面与它引用的静态资源从哪来。
pub trait AssetSource: Send + Sync + 'static {
    /// `path` 不带前导 `/`（`probe.html`、`assets/probe-abc.js`）。回 (MIME, 字节)。
    fn get(&self, path: &str) -> Option<(String, Vec<u8>)>;
}

/// 浏览器交回来的东西。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct BrowserReport {
    /// 页面自己采的（`src/probe/main.ts` 定的形状），原样的 JSON 文本。
    /// 当**不可信数据**：界面解析时逐项校验。
    pub page_json: String,
    /// 请求头里的 `Accept-Language`（网站真正看到的那一个）。
    pub accept_language: Option<String>,
    /// 请求头里的 `Sec-CH-UA-Platform`（Chromium 系才有）。
    pub ch_platform: Option<String>,
    pub user_agent: Option<String>,
    /// 收到的本地时刻，`YYYY-MM-DD HH:MM:SS`。
    pub received_at: String,
    /// 收到时的 Unix 秒。判断新不新用它。
    #[ts(type = "number")]
    pub received_unix: i64,
}

/// 开了一次采集：给浏览器打开的地址，与之后等结果用的令牌。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ProbeStart {
    pub url: String,
    pub token: String,
    /// 面板有没有替你把默认浏览器打开。打不开时界面让你复制链接自己开。
    pub opened: bool,
}

type Waiter = tokio::sync::oneshot::Receiver<BrowserReport>;

fn pending() -> &'static Mutex<HashMap<String, Waiter>> {
    static P: OnceLock<Mutex<HashMap<String, Waiter>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

fn last() -> &'static Mutex<Option<BrowserReport>> {
    static L: OnceLock<Mutex<Option<BrowserReport>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(None))
}

/// 最近一次交回来的报告（不管多旧）。
pub fn last_report() -> Option<BrowserReport> {
    last().lock().ok().and_then(|g| g.clone())
}

/// 最近一次、而且还在 [`FRESH_FOR`] 之内的报告。
pub fn fresh_report() -> Option<BrowserReport> {
    let now = chrono::Local::now().timestamp();
    last_report().filter(|r| now - r.received_unix <= FRESH_FOR.as_secs() as i64)
}

/// 开一次采集。返回的地址交给默认浏览器打开。
pub async fn start(assets: Arc<dyn AssetSource>) -> Result<ProbeStart> {
    start_with(assets, TIMEOUT).await
}

pub async fn start_with(assets: Arc<dyn AssetSource>, timeout: Duration) -> Result<ProbeStart> {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|e| GateError::Other(format!("开不了本机的采集服务：{e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| GateError::Other(format!("拿不到采集服务的端口：{e}")))?
        .port();
    let token = uuid::Uuid::new_v4().simple().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel::<BrowserReport>();
    if let Ok(mut p) = pending().lock() {
        p.insert(token.clone(), rx);
    }
    let shared = Arc::new(Shared {
        token: token.clone(),
        port,
        assets,
        tx: Mutex::new(Some(tx)),
        done: tokio::sync::Notify::new(),
    });

    tokio::spawn(async move {
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);
        loop {
            let accepted = tokio::select! {
                _ = &mut deadline => break,
                _ = shared.done.notified() => break,
                a = listener.accept() => a,
            };
            let Ok((stream, _)) = accepted else { continue };
            let s = Arc::clone(&shared);
            tokio::spawn(async move {
                let svc = service_fn(move |req| handle(req, Arc::clone(&s)));
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), svc)
                    .await;
            });
        }
        // 监听在这里被丢掉 = 端口关了。没交回的那一份：丢掉发送端，等的那头拿到「没有」，
        // 没人来等的那个接收端也一并清掉。
        let unsent = shared.tx.lock().ok().and_then(|mut t| t.take()).is_some();
        if unsent {
            if let Ok(mut p) = pending().lock() {
                p.remove(&shared.token);
            }
        }
    });

    Ok(ProbeStart {
        url: format!("http://127.0.0.1:{port}/{token}/"),
        token,
        opened: false,
    })
}

/// 等这一次采集的结果。超时或没人交回是 `None`。
///
/// 收到的那一份在交回的那一刻就记进 [`last_report`] 了 —— 界面没在等（弹窗关了）也不丢。
pub async fn wait(token: &str) -> Option<BrowserReport> {
    let rx = pending().lock().ok()?.remove(token)?;
    rx.await.ok()
}

struct Shared {
    token: String,
    port: u16,
    assets: Arc<dyn AssetSource>,
    tx: Mutex<Option<tokio::sync::oneshot::Sender<BrowserReport>>>,
    done: tokio::sync::Notify,
}

fn text(status: StatusCode, body: &'static str) -> Response<Full<Bytes>> {
    let mut r = Response::new(Full::new(Bytes::from_static(body.as_bytes())));
    *r.status_mut() = status;
    r.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    r
}

fn header(req: &Request<Incoming>, name: &str) -> Option<String> {
    req.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(300).collect())
}

/// `Host` 头必须是这个端口的回环地址。
pub fn host_ok(host: Option<&str>, port: u16) -> bool {
    matches!(host, Some(h) if h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"))
}

/// 静态资源只认 `assets/` 底下这几种后缀的构建产物。
pub fn asset_ok(path: &str) -> bool {
    let Some(name) = path.strip_prefix("assets/") else {
        return false;
    };
    !name.contains('/')
        && !name.contains("..")
        && [".js", ".css", ".woff2", ".woff", ".svg", ".png"]
            .iter()
            .any(|ext| name.ends_with(ext))
}

async fn handle(
    req: Request<Incoming>,
    s: Arc<Shared>,
) -> std::result::Result<Response<Full<Bytes>>, std::convert::Infallible> {
    if !host_ok(header(&req, "host").as_deref(), s.port) {
        return Ok(text(StatusCode::MISDIRECTED_REQUEST, "wrong host"));
    }
    let path = req.uri().path().trim_start_matches('/').to_string();
    let page = format!("{}/", s.token);
    let report = format!("{}/report", s.token);

    if req.method() == Method::GET && path == page {
        let Some((_, bytes)) = s.assets.get("probe.html") else {
            return Ok(text(
                StatusCode::NOT_FOUND,
                "probe.html not found (dev: run npm run build first)",
            ));
        };
        let mut r = Response::new(Full::new(Bytes::from(bytes)));
        let h = r.headers_mut();
        h.insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_static("text/html; charset=utf-8"),
        );
        h.insert(
            hyper::header::CACHE_CONTROL,
            hyper::header::HeaderValue::from_static("no-store"),
        );
        // 页面只许加载自己这个源的脚本与样式、只许往自己这个源交结果。
        h.insert(
            hyper::header::CONTENT_SECURITY_POLICY,
            hyper::header::HeaderValue::from_static(
                "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; font-src 'self'",
            ),
        );
        h.insert(
            hyper::header::REFERRER_POLICY,
            hyper::header::HeaderValue::from_static("no-referrer"),
        );
        return Ok(r);
    }

    if req.method() == Method::GET && asset_ok(&path) {
        return Ok(match s.assets.get(&path) {
            Some((mime, bytes)) => {
                let mut r = Response::new(Full::new(Bytes::from(bytes)));
                if let Ok(v) = hyper::header::HeaderValue::from_str(&mime) {
                    r.headers_mut().insert(hyper::header::CONTENT_TYPE, v);
                }
                r
            }
            None => text(StatusCode::NOT_FOUND, "not found"),
        });
    }

    if req.method() == Method::POST && path == report {
        let accept_language = header(&req, "accept-language");
        let ch_platform =
            header(&req, "sec-ch-ua-platform").map(|p| p.trim_matches('"').to_string());
        let user_agent = header(&req, "user-agent");
        let body = match Limited::new(req.into_body(), MAX_REPORT).collect().await {
            Ok(b) => b.to_bytes(),
            Err(_) => return Ok(text(StatusCode::PAYLOAD_TOO_LARGE, "too large")),
        };
        let Ok(page_json) = String::from_utf8(body.to_vec()) else {
            return Ok(text(StatusCode::BAD_REQUEST, "not utf-8"));
        };
        if serde_json::from_str::<serde_json::Value>(&page_json).is_err() {
            return Ok(text(StatusCode::BAD_REQUEST, "not json"));
        }
        let now = chrono::Local::now();
        let r = BrowserReport {
            page_json,
            accept_language,
            ch_platform,
            user_agent,
            received_at: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            received_unix: now.timestamp(),
        };
        // 只收一份：第二份（刷新了页面、别的浏览器也打开了这个链接）直接拒掉。
        let Some(tx) = s.tx.lock().ok().and_then(|mut t| t.take()) else {
            return Ok(text(StatusCode::GONE, "already used"));
        };
        if let Ok(mut l) = last().lock() {
            *l = Some(r.clone());
        }
        // 等的那头可能已经不在了（弹窗关了）—— 那也没关系，上面已经记下了。
        let _ = tx.send(r);
        s.done.notify_one();
        return Ok(text(StatusCode::NO_CONTENT, ""));
    }

    Ok(text(StatusCode::NOT_FOUND, "not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Mem;
    impl AssetSource for Mem {
        fn get(&self, path: &str) -> Option<(String, Vec<u8>)> {
            match path {
                "probe.html" => Some(("text/html".into(), b"<html>probe</html>".to_vec())),
                "assets/probe-x.js" => Some(("text/javascript".into(), b"1".to_vec())),
                _ => None,
            }
        }
    }

    /// 用裸 TCP 发一个请求，读回状态码与响应体。单测不联网：只连回环上刚开的那个端口。
    async fn send(port: u16, raw: String) -> (u16, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        s.write_all(raw.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(3), s.read_to_end(&mut buf)).await;
        let text = String::from_utf8_lossy(&buf).to_string();
        let code = text
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        (code, text)
    }

    fn get(path: &str, host: &str) -> String {
        format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
    }

    fn post(port: u16, path: &str, body: &str) -> String {
        format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept-Language: en-US,en;q=0.9\r\n\
             Sec-CH-UA-Platform: \"Windows\"\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn port_of(url: &str) -> u16 {
        url.trim_start_matches("http://127.0.0.1:")
            .split('/')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    #[tokio::test]
    async fn the_page_needs_the_token_and_the_right_host() {
        let st = start_with(Arc::new(Mem), Duration::from_secs(10))
            .await
            .unwrap();
        let port = port_of(&st.url);
        let me = format!("127.0.0.1:{port}");
        let (ok, body) = send(port, get(&format!("/{}/", st.token), &me)).await;
        assert_eq!(ok, 200);
        assert!(body.contains("probe"));
        assert!(body
            .to_ascii_lowercase()
            .contains("content-security-policy"));
        assert_eq!(send(port, get("/wrong-token/", &me)).await.0, 404);
        assert_eq!(
            send(port, get(&format!("/{}/", st.token), "evil.example:80"))
                .await
                .0,
            421,
            "Host 对不上（DNS 重绑定）一律拒"
        );
        assert_eq!(send(port, get("/assets/probe-x.js", &me)).await.0, 200);
        assert_eq!(send(port, get("/assets/../secret.txt", &me)).await.0, 404);
        assert_eq!(send(port, get("/etc/passwd", &me)).await.0, 404);
    }

    #[tokio::test]
    async fn only_one_json_report_is_taken_and_headers_are_recorded() {
        let st = start_with(Arc::new(Mem), Duration::from_secs(10))
            .await
            .unwrap();
        let port = port_of(&st.url);
        let path = format!("/{}/report", st.token);
        assert_eq!(send(port, post(port, &path, "not json")).await.0, 400);
        let token = st.token.clone();
        let waiter = tokio::spawn(async move { wait(&token).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(send(port, post(port, &path, r#"{"v":1}"#)).await.0, 204);
        let r = waiter.await.unwrap().expect("交回的那一份要等得到");
        assert_eq!(r.page_json, r#"{"v":1}"#);
        assert_eq!(r.accept_language.as_deref(), Some("en-US,en;q=0.9"));
        assert_eq!(r.ch_platform.as_deref(), Some("Windows"));
        assert_eq!(
            last_report().map(|x| x.page_json),
            Some(r#"{"v":1}"#.to_string())
        );
        // 收完就关：再连就连不上了（或者被拒）。
        tokio::time::sleep(Duration::from_millis(100)).await;
        let again = tokio::net::TcpStream::connect(("127.0.0.1", port)).await;
        if let Ok(_) = again {
            let (code, _) = send(port, post(port, &path, r#"{"v":1}"#)).await;
            assert!(code == 0 || code == 410, "第二份不许收：{code}");
        }
    }

    #[tokio::test]
    async fn an_oversized_report_is_refused() {
        let st = start_with(Arc::new(Mem), Duration::from_secs(10))
            .await
            .unwrap();
        let port = port_of(&st.url);
        let big = format!(r#"{{"x":"{}"}}"#, "a".repeat(MAX_REPORT + 10));
        let (code, _) = send(port, post(port, &format!("/{}/report", st.token), &big)).await;
        assert_eq!(code, 413);
    }

    #[tokio::test]
    async fn nobody_reporting_times_out_to_none() {
        let st = start_with(Arc::new(Mem), Duration::from_millis(200))
            .await
            .unwrap();
        assert_eq!(wait(&st.token).await, None);
        assert_eq!(wait("never-issued").await, None);
    }

    #[test]
    fn host_and_asset_rules() {
        assert!(host_ok(Some("127.0.0.1:5000"), 5000));
        assert!(host_ok(Some("localhost:5000"), 5000));
        assert!(!host_ok(Some("127.0.0.1:5001"), 5000));
        assert!(!host_ok(None, 5000));
        assert!(asset_ok("assets/probe-abc.js"));
        assert!(!asset_ok("assets/sub/x.js"));
        assert!(!asset_ok("assets/..%2f.js"));
        assert!(!asset_ok("index.html"));
        assert!(!asset_ok("assets/x.exe"));
    }
}
