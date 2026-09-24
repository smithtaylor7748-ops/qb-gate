//! 酒馆的 Gemini 后端：面板进程内的 OpenAI 兼容桥接，每个请求驱动一次官方 Gemini CLI（0.26.0）。
//!
//! 使用者要「把反重力也接进酒馆」。反重力 Hub 本身**没有公开的无交互模式**（它的
//! `--headless` 走语言服务器未公开的 stdin 协议，令牌在凭据管理器里），能像 `codex exec`
//! 那样被驱动的是 Google 自己的 Gemini CLI：同一个 Google 账户、公开的无交互模式、
//! `--output-format json`、`GEMINI_CLI_HOME` 隔离槽位。所以这条桥驱动的是 **Gemini CLI，
//! 不是反重力本体** —— 界面与 DISCLAIMER §8 都这么写；额度是否与反重力订阅共享**未核实**。
//!
//! # ⛔ 五条硬约束（动任何一条同步改 DISCLAIMER §8 与 CLAUDE.md）
//!
//! 1. **只驱动未修改的官方 Gemini CLI 的公开无交互模式**：stdin 喂提示词（非 TTY 即无交互，
//!    `docs/cli/headless.md`），`--output-format json`。不改它的代码、不调它内部模块。
//! 2. **桥接路径对 `oauth_creds.json` 零接触。** 不读、不复制、不转发；身份全靠
//!    `GEMINI_CLI_HOME=<当前激活 Gemini 槽位>\home` 交给 CLI 自己去读。
//!    另有用户显式触发的额度探针会短暂读取 access token；它不参与桥接请求，
//!    不保存、不返回，也不自动运行。
//! 3. **只绑 `127.0.0.1`**，Bearer token 由面板生成、只给本机酒馆用；没有 API Key 回退
//!    （不设 `GEMINI_API_KEY`，那会让 CLI 绕开账户登录走计费 API）。
//! 4. **不给工具、不传图。** 提示词框架明说不要动工具；请求里的图片段落被丢掉。
//! 5. **门禁跟着反重力那一档**（`LaunchTarget::Antigravity.gated()`）：同一个 Google 账户，
//!    IP 不合格时桥接跟酒馆一起归门禁收（`gate_ops::stop_managed`）。
//!
//! # 它做不到的
//!
//! - **没有增量流式。** `--output-format json` 是整段回；酒馆开了 stream 也只会整段出现。
//! - 每个请求起一个 Node 进程，冷启动两三秒；请求串行化。
//! - 用量数字是 CLI `stats` 里的近似值，只用于显示。

use crate::error::{GateError, Result};
use crate::gpt_bridge::{
    authorized, error_response, frame_with_prompt, json_response, read_body, Body, ChatRequest,
};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use ts_rs::TS;

/// 一次 Gemini CLI 最长跑多久。
const TURN_TIMEOUT: Duration = Duration::from_secs(300);
/// 酒馆的模型下拉里得有个名字。配置里没填模型时用它，桥接见到它就不传 `-m`。
pub const DEFAULT_MODEL_LABEL: &str = "gemini-default";

// ------------------------------------------------------------------ 状态

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct GeminiBridgeStatus {
    pub running: bool,
    pub port: u16,
    /// 酒馆里 Custom 源要填的地址，带 `/v1`。
    pub url: String,
    /// 在用（或将要用）的 Gemini 槽位标签。没有激活槽位就是 `None`。
    pub slot: Option<String>,
    pub slot_logged_in: bool,
    /// `node.exe` + 入口脚本。
    pub gemini_cli: Option<String>,
    pub token_path: String,
    pub model: String,
    pub detail: String,
}

struct Running {
    port: u16,
    slot_label: String,
    home: PathBuf,
    node: PathBuf,
    entry: PathBuf,
    runtime: PathBuf,
    token: String,
    model: String,
    system_prompt: String,
    stop: tokio::sync::watch::Sender<bool>,
    /// 一次只跑一个 CLI。
    turn: tokio::sync::Mutex<()>,
    /// 子进程全挂进这个 Job：面板退出 / `stop()` 时一个都不留。
    job: crate::process::KillOnCloseJob,
}

static BRIDGE: OnceLock<Mutex<Option<Arc<Running>>>> = OnceLock::new();

fn cell() -> &'static Mutex<Option<Arc<Running>>> {
    BRIDGE.get_or_init(Default::default)
}
fn current() -> Option<Arc<Running>> {
    cell().lock().ok().and_then(|g| g.clone())
}
pub fn running() -> bool {
    current().is_some()
}

fn data_dir() -> PathBuf {
    crate::paths::state_dir()
        .join("plugins")
        .join("gemini-bridge")
}
fn token_path() -> PathBuf {
    data_dir().join("token.txt")
}
pub fn url_of(port: u16) -> String {
    format!("http://127.0.0.1:{port}/v1")
}

/// 读（或第一次生成）Bearer token。跟 GPT 桥接一样：两个 v4 uuid 拼起来，256 位随机。
pub fn token() -> Result<String> {
    let p = token_path();
    if let Some(existing) = crate::config_io::read_optional(&p)? {
        let s = String::from_utf8_lossy(&existing).trim().to_string();
        if s.len() >= 32 {
            return Ok(s);
        }
    }
    let fresh = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    std::fs::create_dir_all(data_dir())?;
    crate::config_io::commit(vec![crate::config_io::Edit::text(p, fresh.clone())?])?;
    Ok(fresh)
}

fn active_slot() -> Result<Option<(String, PathBuf, bool)>> {
    // 0.32.0：CLI 那一半跟 IDE 共用一条账户槽位。
    qb_accounts::antigravity::account::active_cli(&qb_accounts::antigravity::account::root())
}

pub fn status() -> GeminiBridgeStatus {
    let cfg = crate::plugins::sillytavern::load_config();
    let cli = crate::plugins::sillytavern::find_official_gemini().ok();
    let slot = active_slot().ok().flatten();
    let model = if cfg.gemini_model.trim().is_empty() {
        DEFAULT_MODEL_LABEL.to_string()
    } else {
        cfg.gemini_model.trim().to_string()
    };
    if let Some(r) = current() {
        return GeminiBridgeStatus {
            running: true,
            port: r.port,
            url: url_of(r.port),
            slot: Some(r.slot_label.clone()),
            slot_logged_in: true,
            gemini_cli: Some(format!("{} {}", r.node.display(), r.entry.display())),
            token_path: token_path().display().to_string(),
            model: if r.model.is_empty() {
                DEFAULT_MODEL_LABEL.into()
            } else {
                r.model.clone()
            },
            detail: format!("在监听 127.0.0.1:{} · 用槽位「{}」", r.port, r.slot_label),
        };
    }
    let detail = match (&slot, &cli) {
        (None, _) => "没有激活的 Gemini 槽位：先到「官方账户 · 反重力」新建并登录".into(),
        (Some((label, _, false)), _) => {
            format!("槽位「{label}」还没登录：先在反重力页点「登录 Gemini CLI」完成 Google 登录")
        }
        (Some(_), None) => {
            "没找到官方 Gemini CLI：到「软件」页安装（npm i -g @google/gemini-cli）".into()
        }
        (Some((label, _, true)), Some(_)) => format!("未运行 · 下次会用槽位「{label}」"),
    };
    GeminiBridgeStatus {
        running: false,
        port: cfg.gemini_bridge_port,
        url: url_of(cfg.gemini_bridge_port),
        slot: slot.as_ref().map(|s| s.0.clone()),
        slot_logged_in: slot.as_ref().is_some_and(|s| s.2),
        gemini_cli: cli.map(|g| format!("{} {}", g.node.display(), g.entry.display())),
        token_path: token_path().display().to_string(),
        model,
        detail,
    }
}

// ------------------------------------------------------------------ 起 / 停

/// 起桥接。已经在跑就原样返回（幂等）。端口被占 → 报错、**不停止任何进程**。
pub async fn start() -> Result<GeminiBridgeStatus> {
    if running() {
        return Ok(status());
    }
    let cfg = crate::plugins::sillytavern::load_config_checked()?;
    let Some((slot_label, home, logged_in)) = active_slot()? else {
        return Err(GateError::Other(
            "没有激活的 Gemini 槽位。先到「官方账户 · 反重力」新建一个并登录 Gemini CLI。".into(),
        ));
    };
    if !logged_in {
        return Err(GateError::Other(format!(
            "Gemini 槽位「{slot_label}」还没登录。先在反重力页点「登录 Gemini CLI」，在弹出的窗口里完成 Google 登录。"
        )));
    }
    let cli = crate::plugins::sillytavern::find_official_gemini()?;
    let token = token()?;
    let runtime = data_dir().join("runtime");
    std::fs::create_dir_all(&runtime)?;

    let port = cfg.gemini_bridge_port;
    let want = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(want).await.map_err(|e| {
        GateError::Other(format!(
            "Gemini 桥接绑不上 127.0.0.1:{port} —— {e}。端口多半被别的程序占了；没有停止任何进程，到酒馆插件里换一个端口再试。"
        ))
    })?;

    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    let job = crate::process::KillOnCloseJob::new()?;
    let running = Arc::new(Running {
        port,
        slot_label: slot_label.clone(),
        home,
        node: cli.node,
        entry: cli.entry,
        runtime,
        token,
        model: cfg.gemini_model.trim().to_string(),
        system_prompt: cfg.gemini_system_prompt.trim().to_string(),
        stop,
        turn: tokio::sync::Mutex::new(()),
        job,
    });
    let accept = Arc::clone(&running);
    tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                _ = stopped.changed() => break,
                a = listener.accept() => a,
            };
            let Ok((stream, _)) = accepted else {
                continue;
            };
            let r = Arc::clone(&accept);
            tokio::spawn(async move {
                let svc = service_fn(move |req| handle(req, Arc::clone(&r)));
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), svc)
                    .await;
            });
        }
    });
    *cell().lock().unwrap() = Some(running);
    crate::audit::write(&format!(
        "酒馆 Gemini 桥接已起：127.0.0.1:{port}，槽位「{slot_label}」，只驱动官方 Gemini CLI 的无交互模式"
    ));
    Ok(status())
}

/// 停桥接：不再收连接，结束正在跑的 CLI。没在跑就什么都不做。
pub fn stop() -> Result<()> {
    let Some(r) = cell().lock().unwrap().take() else {
        return Ok(());
    };
    let _ = r.stop.send(true);
    r.job.terminate_all();
    crate::audit::write(&format!("酒馆 Gemini 桥接已停（127.0.0.1:{}）", r.port));
    Ok(())
}

// ------------------------------------------------------------------ HTTP

async fn handle(
    req: Request<Incoming>,
    r: Arc<Running>,
) -> std::result::Result<Response<Body>, hyper::Error> {
    let path = req.uri().path().to_string();
    let method = req.method().clone();
    let resp = match (method.as_str(), path.as_str()) {
        ("GET", "/health") => json_response(
            StatusCode::OK,
            &serde_json::json!({ "status": "ok", "backend": "gemini-cli", "slot": r.slot_label }),
        ),
        ("GET", "/v1/models") => {
            if !authorized(&req, &r.token) {
                return Ok(error_response(
                    StatusCode::UNAUTHORIZED,
                    "缺少或错误的 Bearer token",
                ));
            }
            let id = if r.model.is_empty() {
                DEFAULT_MODEL_LABEL.to_string()
            } else {
                r.model.clone()
            };
            json_response(
                StatusCode::OK,
                &serde_json::json!({
                    "object": "list",
                    "data": [{ "id": id, "object": "model", "owned_by": "google-gemini-cli" }]
                }),
            )
        }
        ("POST", "/v1/chat/completions") => {
            if !authorized(&req, &r.token) {
                return Ok(error_response(
                    StatusCode::UNAUTHORIZED,
                    "缺少或错误的 Bearer token",
                ));
            }
            let body = match read_body(req).await {
                Ok(b) => b,
                Err((status, message)) => return Ok(error_response(status, message)),
            };
            chat(&body, &r).await
        }
        _ => error_response(
            StatusCode::NOT_FOUND,
            "这条桥只有 /health、/v1/models、/v1/chat/completions",
        ),
    };
    Ok(resp)
}

// ------------------------------------------------------------------ Gemini CLI

/// 一次 CLI 的结局。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExecOutcome {
    pub text: Option<String>,
    pub input_tokens: i64,
    pub cached_tokens: i64,
    pub output_tokens: i64,
    pub errors: Vec<String>,
}

/// 解析 `--output-format json` 的输出：`{"response": "...", "stats": {...}, "error": {...}}`。
///
/// 纯函数。stdout 前后若混进了别的行（CLI 偶尔往 stdout 打提示），取第一个 `{` 到
/// 最后一个 `}` 之间的那一段再解。`stats.models.<模型>.tokens` 按公开文档的形状
/// （`prompt` / `candidates` / `cached` / `thoughts`）尽力取，取不到就是 0 —— 只用于显示。
pub fn parse_output(stdout: &str) -> ExecOutcome {
    let mut out = ExecOutcome::default();
    let trimmed = stdout.trim();
    let candidate = match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(a), Some(b)) if a < b => &trimmed[a..=b],
        _ => trimmed,
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(candidate) else {
        if !trimmed.is_empty() {
            out.errors
                .push(format!("CLI 输出不是 JSON：{}", clip(trimmed, 200)));
        }
        return out;
    };
    if let Some(text) = v.get("response").and_then(|t| t.as_str()) {
        if !text.trim().is_empty() {
            out.text = Some(text.to_string());
        }
    }
    if let Some(err) = v.get("error") {
        let m = err
            .get("message")
            .and_then(|m| m.as_str())
            .map(String::from)
            .unwrap_or_else(|| err.to_string());
        out.errors.push(m);
    }
    if let Some(models) = v.pointer("/stats/models").and_then(|m| m.as_object()) {
        for (_, m) in models {
            let n = |k: &str| {
                m.pointer(&format!("/tokens/{k}"))
                    .and_then(|x| x.as_i64())
                    .unwrap_or(0)
            };
            out.input_tokens += n("prompt");
            out.cached_tokens += n("cached");
            out.output_tokens += n("candidates") + n("thoughts");
        }
    }
    out
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

/// 交给 Gemini CLI 的参数（在 `node <入口>` 之后）。纯函数，单测钉着硬约束里能钉的那几条。
pub fn cli_args(entry: &Path, model: &str) -> Vec<String> {
    let mut args = vec![
        entry.display().to_string(),
        // 硬约束 1：公开的 JSON 输出模式；提示词走 stdin（非 TTY 即无交互）。
        "--output-format".into(),
        "json".into(),
    ];
    if !model.is_empty() && model != DEFAULT_MODEL_LABEL {
        args.push("-m".into());
        args.push(model.to_string());
    }
    args
}

/// 起一次 Gemini CLI，喂提示词，等它跑完。
async fn run_cli(r: &Running, model: &str, prompt: String) -> Result<ExecOutcome> {
    let args = cli_args(&r.entry, model);
    let env = crate::sessions::sanitized_environment(
        std::env::vars(),
        vec![
            // 硬约束 2：身份靠这一个变量交给 CLI 自己。
            ("GEMINI_CLI_HOME".into(), r.home.display().to_string()),
            // 凭据过期时别弹浏览器（那会挂到超时）；CLI 会把授权链接打到输出里，
            // 我们当成错误报给酒馆，使用者去反重力页重新登录。
            ("NO_BROWSER".into(), "true".into()),
        ],
    );
    let mut cmd = tokio::process::Command::new(&r.node);
    cmd.args(&args)
        .current_dir(&r.runtime)
        .env_clear()
        .envs(env)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = crate::process::hidden_tokio(cmd)
        .spawn()
        .map_err(|e| GateError::Other(format!("起不了 Gemini CLI（{}）：{e}", r.node.display())))?;
    if let Err(e) = r.job.assign(&child) {
        let _ = child.kill().await;
        return Err(e);
    }
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }
    let waited = tokio::time::timeout(TURN_TIMEOUT, child.wait_with_output()).await;
    let output = match waited {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(GateError::Other(format!("等 Gemini CLI 退出时出错：{e}"))),
        Err(_) => {
            return Err(GateError::Other(format!(
                "Gemini CLI 超过 {} 秒没回完，已结束这次请求。",
                TURN_TIMEOUT.as_secs()
            )));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut outcome = parse_output(&stdout);
    if outcome.text.is_none() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr
            .lines()
            .rev()
            .take(8)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        if !tail.trim().is_empty() {
            outcome.errors.push(tail.trim().to_string());
        }
        if outcome.errors.is_empty() {
            outcome.errors.push(format!(
                "Gemini CLI 退出（{}）但没有回任何 response",
                output.status
            ));
        }
    }
    Ok(outcome)
}

async fn chat(body: &Bytes, r: &Arc<Running>) -> Response<Body> {
    let req: ChatRequest = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                format!("请求不是合法的 chat completions JSON：{e}"),
            )
        }
    };
    if req.messages.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "messages 为空");
    }
    let model = if req.model.trim().is_empty() || req.model.trim() == DEFAULT_MODEL_LABEL {
        r.model.clone()
    } else {
        req.model.trim().to_string()
    };
    let prompt = frame_with_prompt(&req.messages, &r.system_prompt);
    let _turn = r.turn.lock().await;
    let outcome = match run_cli(r, &model, prompt).await {
        Ok(o) => o,
        Err(e) => return error_response(StatusCode::BAD_GATEWAY, e.to_string()),
    };
    let Some(text) = outcome.text else {
        crate::audit::write(&format!(
            "酒馆 Gemini 桥接：一次请求没拿到回复（{}）",
            outcome.errors.join("；")
        ));
        return error_response(
            StatusCode::BAD_GATEWAY,
            format!("Gemini CLI 没有回复：{}", outcome.errors.join("；")),
        );
    };
    let shown_model = if model.is_empty() {
        DEFAULT_MODEL_LABEL.to_string()
    } else {
        model
    };
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4().simple());
    let created = chrono::Utc::now().timestamp();
    let usage = serde_json::json!({
        "prompt_tokens": outcome.input_tokens,
        "completion_tokens": outcome.output_tokens,
        "total_tokens": outcome.input_tokens + outcome.output_tokens,
        "prompt_tokens_details": { "cached_tokens": outcome.cached_tokens }
    });
    if req.stream {
        let first = serde_json::json!({
            "id": id, "object": "chat.completion.chunk", "created": created, "model": shown_model,
            "choices": [{ "index": 0, "delta": { "role": "assistant", "content": text }, "finish_reason": null }]
        });
        let last = serde_json::json!({
            "id": id, "object": "chat.completion.chunk", "created": created, "model": shown_model,
            "choices": [{ "index": 0, "delta": {}, "finish_reason": "stop" }],
            "usage": usage
        });
        let sse = format!("data: {first}\n\ndata: {last}\n\ndata: [DONE]\n\n");
        return Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .body(Full::new(Bytes::from(sse)))
            .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())));
    }
    json_response(
        StatusCode::OK,
        &serde_json::json!({
            "id": id, "object": "chat.completion", "created": created, "model": shown_model,
            "choices": [{ "index": 0, "message": { "role": "assistant", "content": text }, "finish_reason": "stop" }],
            "usage": usage
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 公开文档里 JSON 输出的形状：`response` + `stats`。
    #[test]
    fn the_documented_json_output_parses() {
        let stdout = r#"{"response":"你好呀。","stats":{"models":{"gemini-2.5-pro":{"api":{"totalRequests":1},"tokens":{"prompt":120,"candidates":30,"total":150,"cached":100,"thoughts":5,"tool":0}}}}}"#;
        let o = parse_output(stdout);
        assert_eq!(o.text.as_deref(), Some("你好呀。"));
        assert_eq!(
            (o.input_tokens, o.cached_tokens, o.output_tokens),
            (120, 100, 35)
        );
        assert!(o.errors.is_empty());
    }

    #[test]
    fn errors_are_collected_and_noise_around_the_json_is_tolerated() {
        let o = parse_output("Loaded cached credentials.\n{\"response\":\"\",\"error\":{\"type\":\"AuthError\",\"message\":\"not logged in\"}}\n");
        assert_eq!(o.text, None);
        assert_eq!(o.errors, vec!["not logged in"]);
        let o = parse_output("garbage");
        assert_eq!(o.text, None);
        assert!(o.errors[0].contains("不是 JSON"));
        assert_eq!(parse_output("").errors.len(), 0);
    }

    /// 硬约束能钉的：JSON 输出模式、提示词不进参数（走 stdin）、默认不传 -m。
    #[test]
    fn cli_args_pin_the_hard_constraints() {
        let args = cli_args(Path::new(r"C:\npm\gemini\dist\index.js"), "");
        assert!(args[0].ends_with("index.js"));
        assert!(args.windows(2).any(|w| w == ["--output-format", "json"]));
        assert!(
            !args.iter().any(|a| a == "-p" || a == "--prompt"),
            "提示词走 stdin，不进命令行"
        );
        assert!(!args.contains(&"-m".to_string()));
        let args = cli_args(Path::new("x.js"), DEFAULT_MODEL_LABEL);
        assert!(!args.contains(&"-m".to_string()));
        let args = cli_args(Path::new("x.js"), "gemini-2.5-pro");
        assert!(args.windows(2).any(|w| w == ["-m", "gemini-2.5-pro"]));
        // 不许出现 API Key 相关旗标。
        assert!(!args.iter().any(|a| a.to_lowercase().contains("api")));
    }
}
