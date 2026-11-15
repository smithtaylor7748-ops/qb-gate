//! 浏览器助手的独立测试桥（2026-11-14）：不开面板、不碰真实运行期状态，给插件做真实浏览器验收。
//!
//! ```powershell
//! cargo run -p qb-app --example browser-link-dev
//! ```
//!
//! ```powershell
//! cargo run -p qb-app --example browser-link-dev -- --fresh --browser "C:\Program Files\Google\Chrome\Application\chrome.exe"
//! ```
//!
//! 选项：
//!
//! - `--dir <目录>`：配对记录放哪，默认 `%TEMP%\qb-browser-link-dev`。**不碰** `%LOCALAPPDATA%\ClaudeIpGate`。
//!   不加 `--fresh` 时沿用上次的配对 —— 拿来测「面板重启之后插件自己连回来」。
//! - `--fresh`：先删掉那个目录里的配对记录。
//! - `--open`：发认领码时用默认浏览器打开认领页；`--browser <exe>`：改用这个浏览器打开（默认浏览器不是 Chrome 时用）。
//! - `--station <名字>=<来源>`：可以重复。给「查套路免登录」「公开价格」用的假站点（例如 `Fixture=http://127.0.0.1:8080`）。
//!   账户导入只核对同源与格式，不验证、不保存；公开价格只解析。**这里不打开任何数据库。**
//! - `--dist <目录>`：浏览器检查页（`probe.html`）从哪读，默认仓库根的 `dist`（先 `npm run build`）。
//!
//! 出口判定是假的：默认「放行 203.0.113.10」。在这个窗口里打字，或者调控制端点切换。控制端点只绑 127.0.0.1、
//! 端口系统挑；地址与口令写进 `<目录>\dev-control.json`（也打印在屏幕上），每个请求带 `X-QB-Dev-Token: <口令>`。
//!
//! | 控制端点 | 窗口里打 | 做什么 |
//! |---|---|---|
//! | `GET /state` | `status` | 假判定、门禁标志、连接桥状态（`BrowserLinkStatus`） |
//! | `POST /verdict {"mode":"allowed","ip":"…"}` | `allow [ip]` | 放行（可指定出口） |
//! | `POST /verdict {"mode":"changed"}` | `change` | 换成另一个**白名单内**的出口（测「白名单内换 IP 也拦」） |
//! | `POST /verdict {"mode":"unknown"}` | `unknown` | 查不到出口 |
//! | `POST /verdict {"mode":"rejected"}` | `reject` | 出口不在白名单 |
//! | `POST /verdict {"mode":"hang"}` | `hang` | 判定卡住（测检测超时，12 秒） |
//! | `POST /pause {"paused":true}` | `pause` / `unpause` | IP 锁被手动关闭 / 恢复 |
//! | `POST /generation` | `gen` | IP 锁或网络设置变化（代次加一） |
//! | `POST /connect` | `connect` | 发认领码，回认领页地址 |
//! | `POST /reconnect` | `reconnect` | 一键重连 |
//! | `POST /unpair {"pairing":"…"}` | `unpair <编号>` | 解除配对 |
//! | `POST /offline` | `offline` | 正常停止：先给每个浏览器发「面板下线」 |
//! | `POST /resume` | `resume` | 重新开始监听（沿用上次的端口） |
//! | `POST /occupy` | `occupy` | 停止 → 占住原端口 → 重新开始（模拟端口被别的程序占用，会换到新端口） |
//! | `POST /release` | `release` | 放开被占住的端口 |
//! | `POST /crash` | `crash` | 不发下线消息、直接结束进程（模拟面板崩溃） |
//! | `POST /exit` | `exit` | 正常停止后退出 |
//!
//! 屏幕上打印的认领地址里带一次性认领码，几分钟内有效；配对口令永远不打印。

use std::io::BufRead;
use std::net::{Ipv4Addr, TcpListener as StdListener};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use qb_app::browser_link::{self, Authority, BrowserLinkStatus, Config, Ending, Link, Stations};
use qb_app::browser_probe::AssetSource;
use qb_app::gate::{judge::Judgement, GateState};
use qb_app::usecase::browser_station::BrowserStation;
use serde_json::{json, Value};

const DEFAULT_IP: &str = "203.0.113.10";
const OTHER_IP: &str = "203.0.113.11";

#[derive(Clone)]
enum Mode {
    Allowed(String),
    Unknown,
    Rejected(String),
    Hang,
}

impl Mode {
    fn describe(&self) -> String {
        match self {
            Mode::Allowed(ip) => format!("放行 {ip}"),
            Mode::Unknown => "查不到出口".into(),
            Mode::Rejected(ip) => format!("{ip} 不在白名单"),
            Mode::Hang => "判定卡住".into(),
        }
    }
}

/// 浏览器检查页从仓库的 `dist` 读（`npm run build` 的产物）。路径由 `browser_probe` 先核过。
struct DistAssets(Option<PathBuf>);

impl AssetSource for DistAssets {
    fn get(&self, path: &str) -> Option<(String, Vec<u8>)> {
        let file = self.0.as_ref()?.join(path);
        let bytes = std::fs::read(&file).ok()?;
        let mime = match file.extension().and_then(|e| e.to_str()) {
            Some("html") => "text/html; charset=utf-8",
            Some("js") => "text/javascript",
            Some("css") => "text/css",
            Some("svg") => "image/svg+xml",
            Some("png") => "image/png",
            Some("woff2") => "font/woff2",
            Some("woff") => "font/woff",
            _ => "application/octet-stream",
        };
        Some((mime.into(), bytes))
    }
}

struct Dev {
    link: Link,
    mode: Arc<Mutex<Mode>>,
    gate: Arc<GateState>,
    occupied: Mutex<Option<StdListener>>,
    open: bool,
    browser: Option<String>,
}

fn stamp() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

fn say(message: &str) {
    println!("[{}] {message}", stamp());
}

fn summary(status: &BrowserLinkStatus) -> String {
    let mut out = match status.port {
        Some(port) if status.listening => format!("监听 127.0.0.1:{port}"),
        _ => "未监听".into(),
    };
    if status.relink_needed {
        out.push_str("（有浏览器还在找旧端口，需要 reconnect）");
    }
    for p in &status.pairings {
        out.push_str(&format!(
            "\n    · {} [{}] {} · {}：{}",
            p.label,
            p.pairing_id,
            if p.connected {
                "已连接"
            } else if p.needs_relink {
                "需要重连"
            } else {
                "未连接"
            },
            if p.protection.allow {
                "放行"
            } else {
                "拦截"
            },
            p.protection.reason
        ));
    }
    out
}

impl Dev {
    fn open_url(&self, url: &str) {
        if !self.open && self.browser.is_none() {
            return;
        }
        let program = self.browser.clone().unwrap_or_else(|| "explorer".into());
        match std::process::Command::new(&program).arg(url).spawn() {
            Ok(_) => say(&format!("已用 {program} 打开认领页")),
            Err(e) => say(&format!(
                "没能打开浏览器（{e}），请把上面的地址粘进装了插件的浏览器"
            )),
        }
    }

    async fn state(&self) -> Value {
        json!({
            "verdict": self.mode.lock().unwrap().describe(),
            "manual_paused": self.gate.manual_paused.load(Ordering::SeqCst),
            "generation": self.gate.generation.load(Ordering::SeqCst),
            "occupied": self.occupied.lock().unwrap().as_ref().and_then(|l| l.local_addr().ok()).map(|a| a.port()),
            "link": self.link.status().await,
        })
    }

    fn set_mode(&self, mode: Mode) -> Value {
        say(&format!("假判定 → {}", mode.describe()));
        *self.mode.lock().unwrap() = mode;
        json!({"verdict": self.mode.lock().unwrap().describe()})
    }

    async fn act(&self, command: &str, body: &Value) -> Result<Value, String> {
        let text = |key: &str| body.get(key).and_then(Value::as_str).map(str::to_owned);
        match command {
            "state" | "status" => Ok(self.state().await),
            "verdict" => {
                let mode = match text("mode").as_deref() {
                    Some("allowed") => {
                        Mode::Allowed(text("ip").unwrap_or_else(|| DEFAULT_IP.into()))
                    }
                    Some("changed") => {
                        let now = match &*self.mode.lock().unwrap() {
                            Mode::Allowed(ip) => ip.clone(),
                            _ => DEFAULT_IP.into(),
                        };
                        Mode::Allowed(
                            if now == OTHER_IP {
                                DEFAULT_IP
                            } else {
                                OTHER_IP
                            }
                            .into(),
                        )
                    }
                    Some("unknown") => Mode::Unknown,
                    Some("rejected") => {
                        Mode::Rejected(text("ip").unwrap_or_else(|| "198.51.100.7".into()))
                    }
                    Some("hang") => Mode::Hang,
                    _ => {
                        return Err("mode 只认 allowed / changed / unknown / rejected / hang".into())
                    }
                };
                Ok(self.set_mode(mode))
            }
            "pause" => {
                let paused = body.get("paused").and_then(Value::as_bool).unwrap_or(true);
                self.gate.manual_paused.store(paused, Ordering::SeqCst);
                say(if paused {
                    "IP 锁：手动关闭"
                } else {
                    "IP 锁：恢复"
                });
                Ok(json!({"manual_paused": paused}))
            }
            "generation" => {
                let n = self.gate.generation.fetch_add(1, Ordering::SeqCst) + 1;
                say(&format!("IP 锁代次 → {n}"));
                Ok(json!({"generation": n}))
            }
            "connect" | "reconnect" => {
                let start = if command == "connect" {
                    self.link.connect().await
                } else {
                    self.link.reconnect().await
                }
                .map_err(|e| e.to_string())?;
                if start.rebound {
                    say("拿回了原来的端口，已配对的插件会自己连上来");
                }
                if let Some(url) = &start.url {
                    say(&format!(
                        "认领页（{} 秒内有效）：{url}",
                        browser_link::CLAIM_TTL.as_secs()
                    ));
                    self.open_url(url);
                }
                serde_json::to_value(start).map_err(|e| e.to_string())
            }
            "unpair" => {
                let pairing = text("pairing").ok_or("要给 pairing")?;
                let status = self
                    .link
                    .unpair(&pairing)
                    .await
                    .map_err(|e| e.to_string())?;
                serde_json::to_value(status).map_err(|e| e.to_string())
            }
            "offline" => {
                self.link.stop(Ending::PanelExit).await;
                say("已正常停止（发过「面板下线」）");
                Ok(json!({"listening": false}))
            }
            "resume" => {
                let status = self.link.resume().await;
                serde_json::to_value(status).map_err(|e| e.to_string())
            }
            "occupy" => {
                let port = self
                    .link
                    .status()
                    .await
                    .port
                    .ok_or("现在没在监听，没有端口可占")?;
                self.link.stop(Ending::PanelExit).await;
                let listener = StdListener::bind((Ipv4Addr::LOCALHOST, port))
                    .map_err(|e| format!("占不住端口 {port}：{e}"))?;
                *self.occupied.lock().unwrap() = Some(listener);
                say(&format!("已占住端口 {port}，重新开始监听"));
                let status = self.link.resume().await;
                serde_json::to_value(status).map_err(|e| e.to_string())
            }
            "release" => {
                let port = self
                    .occupied
                    .lock()
                    .unwrap()
                    .take()
                    .and_then(|l| l.local_addr().ok())
                    .map(|a| a.port());
                say(&format!("放开被占住的端口：{port:?}"));
                Ok(json!({"released": port}))
            }
            "crash" => {
                say("不发下线消息，直接结束进程");
                std::process::exit(3);
            }
            "exit" => {
                self.link.stop(Ending::PanelExit).await;
                say("已正常停止，退出");
                std::process::exit(0);
            }
            other => Err(format!("不认识的命令：{other}（打 help 看列表）")),
        }
    }
}

fn reply(code: StatusCode, value: Value) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(value.to_string())));
    *response.status_mut() = code;
    response.headers_mut().insert(
        "content-type",
        "application/json; charset=utf-8".parse().unwrap(),
    );
    response
}

async fn control(
    request: Request<Incoming>,
    dev: Arc<Dev>,
    port: u16,
    token: Arc<String>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let host = request.headers().get("host").and_then(|v| v.to_str().ok());
    if !qb_app::browser_probe::host_ok(host, port) {
        return Ok(reply(
            StatusCode::MISDIRECTED_REQUEST,
            json!({"error": "wrong host"}),
        ));
    }
    let supplied = request
        .headers()
        .get("x-qb-dev-token")
        .and_then(|v| v.to_str().ok());
    if supplied != Some(token.as_str()) {
        return Ok(reply(
            StatusCode::UNAUTHORIZED,
            json!({"error": "缺少或不对的 X-QB-Dev-Token"}),
        ));
    }
    let method = request.method().clone();
    let command = request.uri().path().trim_start_matches('/').to_string();
    let body = if method == Method::POST {
        match Limited::new(request.into_body(), 64 * 1024).collect().await {
            Ok(body) => serde_json::from_slice(&body.to_bytes()).unwrap_or(Value::Null),
            Err(_) => {
                return Ok(reply(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    json!({"error": "too large"}),
                ))
            }
        }
    } else {
        Value::Null
    };
    let allowed = match (&method, command.as_str()) {
        (&Method::GET, "state") => true,
        (&Method::POST, "state") => false,
        (&Method::POST, _) => true,
        _ => false,
    };
    if !allowed {
        return Ok(reply(
            StatusCode::METHOD_NOT_ALLOWED,
            json!({"error": "GET /state，其余一律 POST"}),
        ));
    }
    Ok(match dev.act(&command, &body).await {
        Ok(value) => reply(StatusCode::OK, value),
        Err(message) => reply(StatusCode::BAD_REQUEST, json!({"error": message})),
    })
}

fn help() {
    println!(
        "命令：status · connect · reconnect · allow [ip] · change · unknown · reject · hang · pause · unpause · gen ·\n      unpair <编号> · offline · resume · occupy · release · crash · exit · help"
    );
}

async fn typed(dev: &Dev, line: &str) {
    let mut words = line.split_whitespace();
    let Some(word) = words.next() else { return };
    let arg = words.next();
    let (command, body) = match word {
        "allow" => ("verdict", json!({"mode": "allowed", "ip": arg})),
        "change" => ("verdict", json!({"mode": "changed"})),
        "unknown" => ("verdict", json!({"mode": "unknown"})),
        "reject" => ("verdict", json!({"mode": "rejected"})),
        "hang" => ("verdict", json!({"mode": "hang"})),
        "pause" => ("pause", json!({"paused": true})),
        "unpause" => ("pause", json!({"paused": false})),
        "gen" => ("generation", Value::Null),
        "unpair" => ("unpair", json!({"pairing": arg})),
        "help" => {
            help();
            return;
        }
        other => (other, Value::Null),
    };
    match dev.act(command, &body).await {
        Ok(value) if command == "state" || command == "status" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_default()
            )
        }
        Ok(_) => {}
        Err(message) => say(&format!("没做成：{message}")),
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let flag = |name: &str| args.iter().any(|a| a == name);
    let dir = value("--dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("qb-browser-link-dev"));
    let state_dir = qb_app::paths::state_dir();
    if dir.starts_with(&state_dir) {
        eprintln!(
            "--dir 不能放在面板的数据目录（{}）里：开发桥不碰真实的运行期状态",
            state_dir.display()
        );
        std::process::exit(2);
    }
    std::fs::create_dir_all(&dir).expect("建不了开发桥的目录");
    if flag("--fresh") {
        let _ = std::fs::remove_file(dir.join("browser-link.json"));
    }
    let stations: Vec<BrowserStation> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--station")
        .filter_map(|(i, _)| args.get(i + 1))
        .enumerate()
        .filter_map(|(n, spec)| {
            let (name, origin) = spec.split_once('=')?;
            Some(BrowserStation {
                id: format!("dev-{}", n + 1),
                name: name.into(),
                origin: origin.trim_end_matches('/').into(),
            })
        })
        .collect();
    let dist = value("--dist")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dist")))
        .filter(|d| d.join("probe.html").is_file());

    let mode = Arc::new(Mutex::new(Mode::Allowed(DEFAULT_IP.into())));
    let gate = Arc::new(GateState::default());
    let authority: Authority = {
        let mode = mode.clone();
        Arc::new(move || {
            let mode = mode.lock().unwrap().clone();
            Box::pin(async move {
                match mode {
                    Mode::Allowed(ip) => Judgement::Allowed { ip },
                    Mode::Unknown => Judgement::IpUnknown,
                    Mode::Rejected(ip) => Judgement::IpNotAllowed { ip },
                    Mode::Hang => std::future::pending().await,
                }
            })
        })
    };
    let config = Config {
        dir: dir.clone(),
        gate: gate.clone(),
        authority,
        assets: Arc::new(DistAssets(dist.clone())),
        interval: Arc::new(|| Duration::from_secs(2)),
        monitor: true,
        events: Some(Arc::new(|message: &str| say(message))),
        stations: Stations::Fixture(Arc::new(stations.clone())),
    };
    let dev = Arc::new(Dev {
        link: Link::new(config),
        mode,
        gate,
        occupied: Mutex::new(None),
        open: flag("--open"),
        browser: value("--browser"),
    });

    // 控制端点。
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("开不了控制端点");
    let port = listener.local_addr().unwrap().port();
    let token = Arc::new(uuid::Uuid::new_v4().simple().to_string());
    let control_file = dir.join("dev-control.json");
    std::fs::write(
        &control_file,
        serde_json::to_vec_pretty(&json!({
            "control": format!("http://127.0.0.1:{port}"),
            "token": token.as_str(),
            "dir": dir,
        }))
        .unwrap(),
    )
    .expect("写不了 dev-control.json");
    {
        let dev = dev.clone();
        let token = token.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    continue;
                };
                let dev = dev.clone();
                let token = token.clone();
                tokio::spawn(async move {
                    let service = service_fn(move |request| {
                        control(request, dev.clone(), port, token.clone())
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
    }

    println!("QB Gate 浏览器助手 · 独立测试桥（不开面板、不碰真实数据）");
    println!("  配对记录：{}", dir.join("browser-link.json").display());
    println!(
        "  控制端点：http://127.0.0.1:{port}（口令在 {}）",
        control_file.display()
    );
    println!(
        "  浏览器检查页：{}",
        dist.as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|| "没有（先 npm run build，或给 --dist）".into())
    );
    if !stations.is_empty() {
        for s in &stations {
            println!("  假站点：{} [{}] {}", s.name, s.id, s.origin);
        }
    }
    help();

    let status = dev.link.resume().await;
    if status.listening {
        say("沿用上次的配对，已开始监听；插件会自己连上来");
    } else {
        say("还没有配对：发一个认领码");
        typed(&dev, "connect").await;
    }

    // 状态一变就打印一行（谁连上了、放行还是拦截）。
    {
        let dev = dev.clone();
        tokio::spawn(async move {
            let mut last = String::new();
            loop {
                let now = summary(&dev.link.status().await);
                if now != last {
                    say(&now);
                    last = now;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(16);
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.blocking_send(line).is_err() {
                break;
            }
        }
    });
    while let Some(line) = rx.recv().await {
        typed(&dev, line.trim()).await;
    }
    // 标准输入关了（被别的程序起、没接终端）：一直跑，靠控制端点的 /exit 结束。
    std::future::pending::<()>().await;
}
