//! 酒馆的 GPT 后端：面板进程内的 OpenAI 兼容桥接，每个请求驱动一次官方 `codex exec`。
//!
//! SillyTavern 只会说 OpenAI 那套话（`POST /v1/chat/completions`）。Claude 那条桥
//! 是使用者自己写的 `bridge.py`（驱动 `claude -p`）；GPT 这条由面板自带 ——
//! 使用者 2026-09-20 定的，并且要并进插件商店里现有的酒馆插件，不另起一个。
//!
//! # ⛔ 五条硬约束（动任何一条同步改 DISCLAIMER §8 与 CLAUDE.md）
//!
//! 1. **只驱动未修改的官方 Codex CLI 的公开无交互模式** `codex exec --json`
//!    （`codex exec --help` 2026-09-20 在 codex-cli 0.154.0 上核过的旗标）。
//!    不改它的二进制、不用它内部的 app-server 协议。
//! 2. **`auth.json` 零接触。** 桥接不读、不复制、不转发任何令牌；身份全靠
//!    `CODEX_HOME=<当前激活 GPT 槽位>\home` 交给 CLI 自己去读。
//!    这跟 `turnstate_ops` 那条「`auth.json` 一个字不碰」是同一条纪律。
//! 3. **只绑 `127.0.0.1`**，Bearer token 由面板生成、只给本机酒馆用；没有 API Key 回退。
//! 4. **`model_provider` 强制官方。** `-c model_provider="openai"`：槽位若开着 turn-state
//!    识别，`config.toml` 指向本机路由 —— 桥接不该受它牵连（路由没起就是 503）。
//! 5. **默认 `--ephemeral`**：角色扮演对话不落进槽位的会话历史；使用者在插件面板里
//!    打开「记入槽位用量」才写 `home\sessions`。
//!
//! # 政策线（如实写，不替谁背书）
//!
//! Anthropic 的《Legal and compliance》禁止第三方「route requests through … plan
//! credentials」与「intermediate … session tokens」，但不禁止使用者本人用未修改的
//! 官方二进制登录自己的订阅；OpenAI 对「第三方前端经官方 CLI 用 ChatGPT 登录」
//! **没有明确允许或禁止的公开条款**（2026-09-20 查：codex 讨论 #8338 维护者只指向
//! Terms of Use）。所以这条桥只能长成「本机上使用者自己的酒馆调本机上使用者自己的
//! Codex CLI」这个形状，DISCLAIMER 写明是否合规由使用者自行判断。
//!
//! # 它做不到的（KNOWN-ISSUES 也写了）
//!
//! - **没有增量流式。** `codex exec --json` 的 `agent_message` 只有完成事件，
//!   酒馆开了 stream 也只会整段出现（我们发一条 chunk 再 `[DONE]`）。
//! - 不传图、不给工具：请求里的图片段落被丢掉；`-s read-only` + 提示词里明说不要动工具。
//!   Codex 自带的系统提示替换不了，角色扮演的质量取决于模型自己。
//! - 每个请求起一个进程，冷启动一两秒。酒馆一次只发一发，所以请求串行化。

use crate::error::{GateError, Result};
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use ts_rs::TS;

/// 一次 `codex exec` 最长跑多久。角色扮演一轮几十秒是常态，长思考也不该超过五分钟。
const TURN_TIMEOUT: Duration = Duration::from_secs(300);
/// 请求体上限。酒馆把整段对话 + 世界书都塞进来，几百 KB 是常态，8 MiB 够了。
const MAX_BODY: usize = 8 * 1024 * 1024;
/// 酒馆的模型下拉里得有个名字。配置里没填模型时用它，桥接见到它就不传 `-m`。
pub const DEFAULT_MODEL_LABEL: &str = "codex-default";

// ------------------------------------------------------------------ 状态

/// 给界面看的状态。`running = false` 时后几项仍然有用（下一次会用哪个槽位、哪份 exe）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct GptBridgeStatus {
    pub running: bool,
    pub port: u16,
    /// 酒馆里 Custom 源要填的地址，带 `/v1`。
    pub url: String,
    /// 在用（或将要用）的 GPT 槽位标签。没有激活槽位就是 `None`。
    pub slot: Option<String>,
    pub slot_logged_in: bool,
    pub codex_exe: Option<String>,
    /// Bearer token 存在哪。内容不在这里 —— 要用 `token()` 单独取。
    pub token_path: String,
    pub model: String,
    pub effort: String,
    pub persist_sessions: bool,
    /// 一句人话：在跑 / 没跑 / 缺什么。
    pub detail: String,
}

struct Running {
    port: u16,
    slot_label: String,
    home: PathBuf,
    exe: PathBuf,
    runtime: PathBuf,
    token: String,
    model: String,
    effort: String,
    system_prompt: String,
    persist: bool,
    stop: tokio::sync::watch::Sender<bool>,
    /// 一次只跑一个 `codex exec`。
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
    crate::paths::state_dir().join("plugins").join("gpt-bridge")
}

fn token_path() -> PathBuf {
    data_dir().join("token.txt")
}

pub fn url_of(port: u16) -> String {
    format!("http://127.0.0.1:{port}/v1")
}

/// 读（或第一次生成）Bearer token。**只在使用者点「复制密钥」和桥接起来时读**。
///
/// 两个 v4 uuid 拼起来：256 位随机，来源是操作系统的随机数。
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

/// 当前激活的 GPT 槽位：`(标签, home, 登录没)`。
fn active_slot() -> Result<Option<(String, PathBuf, bool)>> {
    let root = qb_accounts::codex::root();
    let list = qb_accounts::codex::list(&root)?;
    let Some(active) = list.slots.into_iter().find(|s| s.active) else {
        return Ok(None);
    };
    let dir = qb_accounts::codex::directory(&root, &active.id)?;
    Ok(Some((active.label, dir.join("home"), active.logged_in)))
}

pub fn status() -> GptBridgeStatus {
    let cfg = crate::plugins::sillytavern::load_config();
    let exe = crate::plugins::sillytavern::find_official_codex().ok();
    let slot = active_slot().ok().flatten();
    let model = if cfg.gpt_model.trim().is_empty() {
        DEFAULT_MODEL_LABEL.to_string()
    } else {
        cfg.gpt_model.trim().to_string()
    };
    if let Some(r) = current() {
        return GptBridgeStatus {
            running: true,
            port: r.port,
            url: url_of(r.port),
            slot: Some(r.slot_label.clone()),
            slot_logged_in: true,
            codex_exe: Some(r.exe.display().to_string()),
            token_path: token_path().display().to_string(),
            model: if r.model.is_empty() {
                DEFAULT_MODEL_LABEL.into()
            } else {
                r.model.clone()
            },
            effort: r.effort.clone(),
            persist_sessions: r.persist,
            detail: format!("在监听 127.0.0.1:{} · 用槽位「{}」", r.port, r.slot_label),
        };
    }
    let detail = match (&slot, &exe) {
        (None, _) => "没有激活的 GPT 账户槽位：先到「官方账户 · GPT」新建并登录".into(),
        (Some((label, _, false)), _) => {
            format!("槽位「{label}」还没登录：先在 GPT 页启动桌面端完成登录")
        }
        (Some(_), None) => "没找到官方 Codex CLI：到「软件」页安装，或 npm 装 @openai/codex".into(),
        (Some((label, _, true)), Some(_)) => format!("未运行 · 下次会用槽位「{label}」"),
    };
    GptBridgeStatus {
        running: false,
        port: cfg.gpt_bridge_port,
        url: url_of(cfg.gpt_bridge_port),
        slot: slot.as_ref().map(|s| s.0.clone()),
        slot_logged_in: slot.as_ref().is_some_and(|s| s.2),
        codex_exe: exe.map(|p| p.display().to_string()),
        token_path: token_path().display().to_string(),
        model,
        effort: cfg.gpt_effort.clone(),
        persist_sessions: cfg.gpt_persist_sessions,
        detail,
    }
}

// ------------------------------------------------------------------ 起 / 停

/// 起桥接。已经在跑就原样返回（幂等）。
///
/// 端口被别人占着 → 报错、**不停止任何进程**（跟酒馆插件同一条纪律）。
pub async fn start() -> Result<GptBridgeStatus> {
    if running() {
        return Ok(status());
    }
    let cfg = crate::plugins::sillytavern::load_config_checked()?;
    let Some((slot_label, home, logged_in)) = active_slot()? else {
        return Err(GateError::Other(
            "没有激活的 GPT 账户槽位。先到「官方账户 · GPT」新建并登录一个。".into(),
        ));
    };
    if !logged_in {
        return Err(GateError::Other(format!(
            "GPT 槽位「{slot_label}」还没登录。先在 GPT 页「启动 Codex 桌面端」完成 ChatGPT 登录。"
        )));
    }
    let exe = crate::plugins::sillytavern::find_official_codex()?;
    let token = token()?;
    let runtime = data_dir().join("runtime");
    std::fs::create_dir_all(&runtime)?;

    let port = cfg.gpt_bridge_port;
    let want = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(want).await.map_err(|e| {
        GateError::Other(format!(
            "GPT 桥接绑不上 127.0.0.1:{port} —— {e}。端口多半被别的程序占了；没有停止任何进程，到酒馆插件里换一个端口再试。"
        ))
    })?;

    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    let job = crate::process::KillOnCloseJob::new()?;
    let running = Arc::new(Running {
        port,
        slot_label: slot_label.clone(),
        home,
        exe,
        runtime,
        token,
        model: cfg.gpt_model.trim().to_string(),
        effort: cfg.gpt_effort.trim().to_string(),
        system_prompt: cfg.gpt_system_prompt.trim().to_string(),
        persist: cfg.gpt_persist_sessions,
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
        "酒馆 GPT 桥接已起：127.0.0.1:{port}，槽位「{slot_label}」，只驱动官方 codex exec"
    ));
    Ok(status())
}

/// 停桥接：不再收连接，结束正在跑的 `codex exec`。没在跑就什么都不做。
pub fn stop() -> Result<()> {
    let Some(r) = cell().lock().unwrap().take() else {
        return Ok(());
    };
    let _ = r.stop.send(true);
    r.job.terminate_all();
    crate::audit::write(&format!("酒馆 GPT 桥接已停（127.0.0.1:{}）", r.port));
    Ok(())
}

// ------------------------------------------------------------------ HTTP

pub(crate) type Body = Full<Bytes>;

pub(crate) fn json_response(status: StatusCode, v: &serde_json::Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(v.to_string())))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())))
}

pub(crate) fn error_response(status: StatusCode, message: impl Into<String>) -> Response<Body> {
    json_response(
        status,
        &serde_json::json!({ "error": { "message": message.into(), "type": "qb_gate_gpt_bridge" } }),
    )
}

/// 常量时间比较，别让 token 靠响应时间一位一位猜出来。
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub(crate) fn authorized(req: &Request<Incoming>, token: &str) -> bool {
    req.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
        .is_some_and(|t| constant_time_eq(t.trim().as_bytes(), token.as_bytes()))
}

async fn handle(
    req: Request<Incoming>,
    r: Arc<Running>,
) -> std::result::Result<Response<Body>, hyper::Error> {
    let path = req.uri().path().to_string();
    let method = req.method().clone();
    let resp = match (method.as_str(), path.as_str()) {
        ("GET", "/health") => json_response(
            StatusCode::OK,
            &serde_json::json!({ "status": "ok", "backend": "codex-exec", "slot": r.slot_label }),
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
                    "data": [{ "id": id, "object": "model", "owned_by": "openai-codex-cli" }]
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

/// 读整个请求体。出错回 `(状态码, 文案)`，由调用方拼响应 —— 直接回 `Response`
/// 会让 `Err` 变体比 `Ok` 大好几倍（clippy `result_large_err`）。
pub(crate) async fn read_body(
    req: Request<Incoming>,
) -> std::result::Result<Bytes, (StatusCode, String)> {
    let collected = req
        .into_body()
        .collect()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("读请求体失败：{e}")))?;
    let bytes = collected.to_bytes();
    if bytes.len() > MAX_BODY {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("请求体超过 {} MiB", MAX_BODY / 1024 / 1024),
        ));
    }
    Ok(bytes)
}

// ------------------------------------------------------------------ 请求 → 提示词

/// 酒馆送来的一条消息（OpenAI Chat Completions 形状）。`content` 可能是字符串，
/// 也可能是分段数组；图片段落一律丢掉（`codex exec` 这条路不传图）。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default)]
    pub content: serde_json::Value,
    #[serde(default)]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn text(&self) -> String {
        match &self.content {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(parts) => parts
                .iter()
                .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatRequest {
    #[serde(default)]
    pub(crate) model: String,
    #[serde(default)]
    pub(crate) messages: Vec<ChatMessage>,
    #[serde(default)]
    pub(crate) stream: bool,
}

/// 固定的框架说明。中英各一遍：角色卡多半是中文，模型对英文指令的服从最稳。
const FRAME_HEADER: &str = "\
你是一个角色扮演对话中的 assistant。下面依次给出：系统设定（角色卡、世界书、规则），\
然后是到目前为止的对话记录（JSON 数组，role 为 user / assistant / system）。\
请只输出 assistant 的下一条回复正文：不要解释、不要加前缀或标签、不要复述设定、\
不要执行任何命令、不要读写任何文件、不要调用任何工具、不要提到你在一个终端或代码环境里。\
\n\nYou are the assistant in a role-play conversation. Below are the system instructions \
(character card, lore, rules) followed by the conversation so far as a JSON array of \
{role, content}. Reply with ONLY the assistant's next message body: no explanations, \
no prefixes or labels, no restating the setup, do not run commands, do not read or write files, \
do not call tools, and never mention being in a terminal or coding environment.\n";

/// 把一份 Chat Completions 请求拼成交给 `codex exec` 的提示词。纯函数，有单测。
pub fn frame(messages: &[ChatMessage]) -> String {
    frame_with_prompt(messages, "")
}

/// 将厂商面板保存的额外系统提示词加入共享角色扮演框架。
pub fn frame_with_prompt(messages: &[ChatMessage], extra_prompt: &str) -> String {
    let mut out = String::from(FRAME_HEADER);
    let system: Vec<String> = messages
        .iter()
        .filter(|m| m.role == "system")
        .map(ChatMessage::text)
        .filter(|t| !t.trim().is_empty())
        .collect();
    out.push_str("\n===== 系统设定 / SYSTEM =====\n");
    if !extra_prompt.trim().is_empty() {
        out.push_str(extra_prompt.trim());
        out.push('\n');
    }
    if system.is_empty() {
        out.push_str("（无）\n");
    } else {
        for s in system {
            out.push_str(&s);
            out.push('\n');
        }
    }
    out.push_str("\n===== 对话记录 / CONVERSATION (JSON) =====\n");
    let turns: Vec<serde_json::Value> = messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| serde_json::json!({ "role": m.role, "content": m.text() }))
        .collect();
    out.push_str(&serde_json::to_string_pretty(&turns).unwrap_or_else(|_| "[]".into()));
    out.push_str("\n\n===== 现在输出 assistant 的下一条回复 / NOW WRITE THE ASSISTANT'S NEXT MESSAGE =====\n");
    out
}

// ------------------------------------------------------------------ codex exec

/// 一次 `codex exec` 的结局。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExecOutcome {
    /// 最后一条 `agent_message` 的正文。
    pub text: Option<String>,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    /// `turn.failed` / `error` 事件带回的话。
    pub errors: Vec<String>,
}

/// 解析 `codex exec --json` 的 JSONL 输出。纯函数，形状按 OpenAI 公开文档
/// （`thread.started` / `turn.started` / `item.completed` / `turn.completed` / `turn.failed` / `error`）。
/// 认不出的行一律跳过 —— 上游加新事件不该让这里炸。
pub fn parse_events(stdout: &str) -> ExecOutcome {
    let mut out = ExecOutcome::default();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "item.completed" => {
                let item = v.get("item");
                if item.and_then(|i| i.get("type")).and_then(|t| t.as_str())
                    == Some("agent_message")
                {
                    if let Some(text) = item.and_then(|i| i.get("text")).and_then(|t| t.as_str()) {
                        out.text = Some(text.to_string());
                    }
                }
            }
            "turn.completed" => {
                if let Some(u) = v.get("usage") {
                    let n = |k: &str| u.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
                    out.input_tokens = n("input_tokens");
                    out.cached_input_tokens = n("cached_input_tokens");
                    out.output_tokens = n("output_tokens");
                }
            }
            "turn.failed" => {
                if let Some(m) = v
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                {
                    out.errors.push(m.to_string());
                }
            }
            "error" => {
                if let Some(m) = v.get("message").and_then(|m| m.as_str()) {
                    out.errors.push(m.to_string());
                }
            }
            _ => {}
        }
    }
    out
}

/// 交给 `codex exec` 的参数。纯函数，单测钉着五条硬约束里能钉的那几条。
pub fn exec_args(runtime: &Path, model: &str, effort: &str, persist: bool) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "exec".into(),
        "--json".into(),
        "--skip-git-repo-check".into(),
        "--color".into(),
        "never".into(),
        "-s".into(),
        "read-only".into(),
        "-C".into(),
        runtime.display().to_string(),
        // 硬约束 4：槽位配置里的 model_provider（识别接管时指向本机路由）不许影响这条桥。
        "-c".into(),
        "model_provider=\"openai\"".into(),
        // 槽位是桌面端建的，凭证在文件里；别让 CLI 去 keyring 找。
        "-c".into(),
        "cli_auth_credentials_store=\"file\"".into(),
    ];
    if !effort.is_empty() {
        args.push("-c".into());
        args.push(format!("model_reasoning_effort=\"{effort}\""));
    }
    if !model.is_empty() && model != DEFAULT_MODEL_LABEL {
        args.push("-m".into());
        args.push(model.to_string());
    }
    if !persist {
        // 硬约束 5。
        args.push("--ephemeral".into());
    }
    // 提示词从 stdin 读。
    args.push("-".into());
    args
}

/// 起一次 `codex exec`，喂提示词，等它跑完。
async fn run_exec(r: &Running, model: &str, prompt: String) -> Result<ExecOutcome> {
    let args = exec_args(&r.runtime, model, &r.effort, r.persist);
    let env = crate::sessions::sanitized_environment(
        std::env::vars(),
        vec![("CODEX_HOME".into(), r.home.display().to_string())],
    );
    let mut cmd = tokio::process::Command::new(&r.exe);
    cmd.args(&args)
        .current_dir(&r.runtime)
        .env_clear()
        .envs(env)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = crate::process::hidden_tokio(cmd).spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            GateError::Other(format!(
                "Windows 拒绝了启动 Codex CLI（{}）：{e}。多半是执行锁还锁着 —— 门禁没放行时桥接起不了 codex。",
                r.exe.display()
            ))
        } else {
            GateError::Other(format!("起不了 Codex CLI（{}）：{e}", r.exe.display()))
        }
    })?;
    // 硬约束：子进程挂进 Job，面板退出 / stop() 时一个都不留。
    if let Err(e) = r.job.assign(&child) {
        let _ = child.kill().await;
        return Err(e);
    }
    if let Some(mut stdin) = child.stdin.take() {
        // 写完就关：`-` 是读到 EOF 才开始。写失败多半是它已经退出了，
        // 那时候下面 wait 会把 stderr 报出来，不在这里遮住。
        let _ = stdin.write_all(prompt.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }
    let waited = tokio::time::timeout(TURN_TIMEOUT, child.wait_with_output()).await;
    let output = match waited {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(GateError::Other(format!("等 Codex CLI 退出时出错：{e}"))),
        Err(_) => {
            // 超时：kill_on_drop 会在 child drop 时收掉它；Job 也兜着。
            return Err(GateError::Other(format!(
                "Codex CLI 超过 {} 秒没回完，已结束这次请求。",
                TURN_TIMEOUT.as_secs()
            )));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut outcome = parse_events(&stdout);
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
                "Codex CLI 退出（{}）但没有回任何 agent_message",
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
    // 一次只跑一个 codex exec。
    let _turn = r.turn.lock().await;
    let outcome = match run_exec(r, &model, prompt).await {
        Ok(o) => o,
        Err(e) => return error_response(StatusCode::BAD_GATEWAY, e.to_string()),
    };
    let Some(text) = outcome.text else {
        crate::audit::write(&format!(
            "酒馆 GPT 桥接：一次请求没拿到回复（{}）",
            outcome.errors.join("；")
        ));
        return error_response(
            StatusCode::BAD_GATEWAY,
            format!("Codex 没有回复：{}", outcome.errors.join("；")),
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
        "prompt_tokens_details": { "cached_tokens": outcome.cached_input_tokens }
    });
    if req.stream {
        // exec 没有增量：一条 chunk 带全文，再一条收尾，再 [DONE]。
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

    fn msg(role: &str, content: serde_json::Value) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content,
            name: None,
        }
    }

    /// 公开文档里的样例行，一字不改。
    #[test]
    fn the_documented_event_stream_parses() {
        let stdout = r#"{"type":"thread.started","thread_id":"0199a213-81c0-7800-8aa1-bbab2a035a53"}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"**Scanning docs**"}}
{"type":"item.completed","item":{"id":"item_3","type":"agent_message","text":"Done. I updated the docs and added examples."}}
{"type":"turn.completed","usage":{"input_tokens":24763,"cached_input_tokens":24448,"output_tokens":122}}
"#;
        let o = parse_events(stdout);
        assert_eq!(
            o.text.as_deref(),
            Some("Done. I updated the docs and added examples.")
        );
        assert_eq!(
            (o.input_tokens, o.cached_input_tokens, o.output_tokens),
            (24763, 24448, 122)
        );
        assert!(o.errors.is_empty());
    }

    #[test]
    fn failures_and_transient_errors_are_collected_not_dropped() {
        let stdout = r#"{"type":"thread.started","thread_id":"x"}
{"type":"error","message":"stream error: broken pipe"}
{"type":"turn.failed","error":{"message":"model response stream ended unexpectedly"}}
not json at all
"#;
        let o = parse_events(stdout);
        assert_eq!(o.text, None);
        assert_eq!(
            o.errors,
            vec![
                "stream error: broken pipe",
                "model response stream ended unexpectedly"
            ]
        );
    }

    /// 两条 agent_message 取最后一条 —— 前面那条多半是「我先看看」。
    #[test]
    fn the_last_agent_message_wins() {
        let stdout = r#"{"type":"item.completed","item":{"id":"a","type":"agent_message","text":"first"}}
{"type":"item.completed","item":{"id":"b","type":"agent_message","text":"second"}}
"#;
        assert_eq!(parse_events(stdout).text.as_deref(), Some("second"));
    }

    /// 硬约束能钉的那几条：只读沙箱、官方 provider、默认 ephemeral、stdin 提示词。
    #[test]
    fn exec_args_pin_the_hard_constraints() {
        let args = exec_args(Path::new("C:/rt"), "", "medium", false);
        assert_eq!(args[0], "exec");
        assert!(args.contains(&"--json".to_string()));
        assert!(args.windows(2).any(|w| w == ["-s", "read-only"]));
        assert!(args
            .windows(2)
            .any(|w| w == ["-c", "model_provider=\"openai\""]));
        assert!(args.contains(&"--ephemeral".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("-"));
        // 没填模型就不传 -m；占位名也不传。
        assert!(!args.contains(&"-m".to_string()));
        let args = exec_args(Path::new("C:/rt"), DEFAULT_MODEL_LABEL, "", true);
        assert!(!args.contains(&"-m".to_string()));
        assert!(
            !args.contains(&"--ephemeral".to_string()),
            "记入槽位时不 ephemeral"
        );
        assert!(!args.iter().any(|a| a.starts_with("model_reasoning_effort")));
        let args = exec_args(Path::new("C:/rt"), "gpt-5.6-sol", "high", false);
        assert!(args.windows(2).any(|w| w == ["-m", "gpt-5.6-sol"]));
        assert!(args
            .windows(2)
            .any(|w| w == ["-c", "model_reasoning_effort=\"high\""]));
    }

    /// 框架：系统段在前、对话是 JSON、图片段落被丢掉、明说不要动工具。
    #[test]
    fn the_frame_keeps_system_first_and_drops_images() {
        let messages = vec![
            msg("system", serde_json::json!("你是艾拉。")),
            msg("user", serde_json::json!("你好")),
            msg("assistant", serde_json::json!("你好呀。")),
            msg(
                "user",
                serde_json::json!([
                    {"type":"text","text":"看这张图"},
                    {"type":"image_url","image_url":{"url":"data:image/png;base64,AAAA"}}
                ]),
            ),
        ];
        let p = frame(&messages);
        let sys_at = p.find("你是艾拉。").unwrap();
        let conv_at = p.find("\"你好\"").unwrap();
        assert!(sys_at < conv_at, "系统设定要排在对话前面");
        assert!(p.contains("看这张图"));
        assert!(!p.contains("base64"), "图片段落不许进提示词");
        assert!(p.contains("不要调用任何工具"));
        assert!(p.contains("do not call tools"));
        // 对话段是合法 JSON 数组，三条（system 不在里面）。
        let start = p.find('[').unwrap();
        let end = p.rfind(']').unwrap();
        let turns: Vec<serde_json::Value> = serde_json::from_str(&p[start..=end]).unwrap();
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0]["role"], "user");
    }

    #[test]
    fn a_request_without_system_says_so() {
        let p = frame(&[msg("user", serde_json::json!("hi"))]);
        assert!(p.contains("（无）"));
    }

    #[test]
    fn bearer_compare_is_exact_and_length_sensitive() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn content_parts_collapse_to_their_text() {
        let m = msg(
            "user",
            serde_json::json!([{"type":"text","text":"a"},{"type":"text","text":"b"}]),
        );
        assert_eq!(m.text(), "a\nb");
        assert_eq!(msg("user", serde_json::json!(null)).text(), "");
    }
}
