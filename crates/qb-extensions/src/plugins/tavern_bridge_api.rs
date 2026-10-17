//! Claude 桥接（`bridge.py`）自己那两个网页搬进面板（0.32.0，使用者定的）。
//!
//! # 原来那份「控制面板」是什么
//!
//! 使用者看到的那个控制面板其实**不是面板的东西** —— 它是 `bridge.py` 自己起的两个
//! HTML 页（`GET /settings` 与 `GET /monitor`），由他本机那份 SillyTavern 扩展
//! （`claude-tavern-bridge`，**面板不分发它**）用一个 `<iframe>` 嵌进酒馆的扩展抽屉里。
//! 于是：改一句提示词要先起酒馆、进扩展抽屉、等 iframe 加载；而 GPT / Gemini
//! 那两条桥聊天时，那个 iframe 指着没人听的 5001，弹 `ERR_CONNECTION_REFUSED`（坑 7.66）。
//!
//! 现在面板直接调它的 HTTP 接口，把七个旋钮与调用日志做成原生界面。
//! 2026-09-29 起它们在「官方账户 · Claude」酒馆磁贴右上角那个弹窗里（扩展中心的酒馆页删了），
//! 同一天第三轮挪到了软件页 Claude Code 卡最下面的「酒馆 · Claude 桥接设置」（那个弹窗换成了酒馆本身的控制面板）；
//! 并补齐了那两个网页里原来只有它们才有的两样：SDK 会话（[`BridgeHealth::sessions`]）与
//! 逐条「展开输入 / 输出」（[`call_detail`]）—— 使用者要的是「不用再开本地链接」。
//!
//! # ⛔ 三条硬要求
//!
//! 1. **一律 `.no_proxy()`。** 坑 7.64 就是 `tavern_healthy` 漏了这一句：设了
//!    `HTTP_PROXY` 的机器上，对 `127.0.0.1` 的探测被送去代理，于是「桥明明在跑、
//!    面板永远等不到它」。这个模块每一个请求都要带。
//! 2. **鉴权只在真要发请求那一刻读 `bridge-token.txt`。** 不进 status、不进日志、
//!    不进事件 —— 跟 `tavern_gpt_token` 同一条口径。
//! 3. ⛔ **能力探测，不许假设。** `bridge.py` 是**使用者自己那份**，面板不分发；
//!    别人机器上可能是别的版本、甚至根本没有这个文件。连不上 / 404 / 字段缺，
//!    一律如实说「这份桥接没有这个接口」或「Claude 桥接没在跑」，
//!    **不许把空表显示成 0 条、把读不到显示成没有**（§7.17）。
//!
//! # 选项列表是快照，而当前值永远算合法
//!
//! 模型那 13 个、effort 那 5 档是按 `bridge.py` 2.4.0 抄的快照
//! （[`MODELS`] / [`EFFORTS`] …；2026-09-21 抄，09-23 补了 `claude-opus-5-5`）。
//! 上游加一个新模型时，面板这边的下拉里没有它 ——
//! 所以**桥当前用着的那个值永远算合法选项**（[`with_current`]），不然一打开设置
//! 就把人家的模型悄悄换成列表里的第一个。保存被拒就把桥回的原话照搬到界面上。
//!
//! # 调用日志的形状照 `bridge.py` 源码读，不照想象写（2026-09-26）
//!
//! 0.32.0 那版按一套**从没存在过**的字段名解析（`at` / `mode` / `input` / `output` /
//! `cache_write` / `elapsed_ms` / `countable`）—— 2.1.0 起的每一版 `bridge.py` 都没有它们。
//! 于是消息监控每一行都是「输入 0 · 输出 0」「读 0 · 写 0」、时间和耗时是「—」，
//! 命中率被当成「不是比例」丢掉；而单测拿同一套想象出来的形状喂，全绿。
//! 现在按 2.4.0 的 `telemetry_summary` / `cache_stats` 读（录下来的真实样子在单测里），
//! 读不到的一律 `None`、界面显示「—」，不再补成 0。

use crate::error::{GateError, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use ts_rs::TS;

/// 本机回环上的桥，2 秒还不应答就是没在跑。
const TIMEOUT: Duration = Duration::from_secs(5);

/// 从 `bridge.py` 2.4.0 的 `ALLOWED_MODELS` 抄的快照，顺序照抄。2026-09-21 抄，
/// 09-23 补 `claude-opus-5-5`（桥那边同一天加进白名单与它自己的设置页）。
pub const MODELS: &[&str] = &[
    "claude-fable-5-1",
    "claude-fable-5",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-sonnet-5",
    "claude-haiku-4-5",
    "claude-opus-4-8",
    "claude-opus-4-7",
    "claude-opus-4-6",
    "claude-opus-4-5-20251101",
    "claude-sonnet-4-6",
    "claude-sonnet-4-5-20250929",
    "claude-haiku-4-5-20251001",
];
pub const EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];
pub const CACHE_TTLS: &[&str] = &["5m", "1h"];
pub const PROMPT_MODES: &[&str] = &["append", "replace"];
pub const INVOCATION_MODES: &[&str] = &["cli", "agent_sdk"];

/// 快照 + 桥当前用着的那个值。顺序保持快照原样，当前值不在里面就补到最前。
fn with_current(snapshot: &[&str], current: &str) -> Vec<String> {
    let mut out: Vec<String> = snapshot.iter().map(|s| (*s).to_string()).collect();
    if !current.is_empty() && !out.iter().any(|s| s == current) {
        out.insert(0, current.to_string());
    }
    out
}

/// `GET /health` 回来的东西。**这也是能力探测**：读得出来才谈得上后面那些。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct BridgeHealth {
    pub bridge_version: String,
    /// `cli` / `agent_sdk`。
    pub invocation_mode: String,
    pub model: String,
    pub effort: String,
    pub cache_ttl: String,
    /// 正在跑的 SDK 会话数。2.4.0 给的是一个对象（`{status, session_count, sessions}`），
    /// 也接受直接一个数。`None` = 桥没给、或者认不出。
    #[ts(type = "number | null")]
    pub sdk_sessions: Option<i64>,
    /// 每个 SDK 会话（2026-09-29，原来是 `bridge.py` 设置页里那块「当前模式 / SDK session / 本轮 token」）。
    /// CLI 模式下永远是空的：每条消息起一次 `claude -p`，没有常驻会话。
    pub sessions: Vec<BridgeSession>,
}

/// 一个 SDK 会话（Agent SDK 模式下才有）。字段照 `bridge.py` 2.4.0 的 `SdkRunner.session_status`
/// 与 `summarize_model_usage` 的源码写（2026-09-29 读的）；读不到的数是 `None`，不是 0。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct BridgeSession {
    /// 酒馆那边的扩展按聊天给的 id（`X-Claude-Tavern-Chat-Instance-Id`）。
    pub chat_instance_id: String,
    pub sdk_session_id: String,
    /// 桥的原话（`active` / `idle` / `retired` / `error` …），原样。
    pub status: String,
    /// 本机时区 `MM-DD HH:MM:SS`，由桥给的 Unix 秒换算；读不出留空。
    pub created_at: String,
    pub last_used_at: String,
    /// 最近一轮的输入 token（`usage.input_tokens`）。
    #[ts(type = "number | null")]
    pub input_tokens: Option<i64>,
    /// 最近一轮的输出 token，含思考（`usage.output_tokens`）。
    #[ts(type = "number | null")]
    pub output_tokens: Option<i64>,
    #[ts(type = "number | null")]
    pub cache_read: Option<i64>,
    #[ts(type = "number | null")]
    pub cache_write: Option<i64>,
    /// 桥写的那句缓存说明（`usage.cache_message`；usage 缺时是 `usage.message`）。
    pub cache_message: String,
}

/// 一条调用的全文（2026-09-29，原来是 `bridge.py` 监控页「展开查看输入/输出」）。
///
/// 形状照 `bridge.py` 2.4.0：`GET /api/telemetry?id=<id>` 回 `{"record": {...}}`，记录里
/// `input.system_prompt`、`input.messages[].role/content`、`output`、`error`
/// （`telemetry_summary` 就是从这几项数出 `input_chars` / `output_chars` 的）。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct BridgeCallDetail {
    pub system_prompt: Option<String>,
    pub messages: Vec<BridgeMessage>,
    /// 回复正文。
    pub output: Option<String>,
    pub error: Option<String>,
    /// 形状认不出时，整条记录原样（缩进过的 JSON）；认得出时是 `None`。
    pub raw: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct BridgeMessage {
    pub role: String,
    pub content: String,
}

/// 七个旋钮。字段名跟 `bridge.py` 的 `settings.json` 一一对应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BridgeSettings {
    /// 附加系统提示词：排在酒馆全部系统内容之前。
    #[serde(default)]
    pub priority_prompt: String,
    /// 贴尾提示词：每一轮钉在对话最后。
    #[serde(default)]
    pub tail_prompt: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub effort: String,
    #[serde(default)]
    pub cache_ttl: String,
    /// `append`（追加到 Claude 默认提示词）/ `replace`（替换）。
    #[serde(default)]
    pub system_prompt_mode: String,
    /// `cli` / `agent_sdk`。
    #[serde(default)]
    pub invocation_mode: String,
}

/// 设置 + 每一项的可选值（快照 ∪ 当前值）。界面直接拿它渲染下拉。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct BridgeSettingsView {
    pub settings: BridgeSettings,
    pub models: Vec<String>,
    pub efforts: Vec<String>,
    pub cache_ttls: Vec<String>,
    pub prompt_modes: Vec<String>,
    pub invocation_modes: Vec<String>,
}

/// 一条调用记录。**字段按 `bridge.py` 给的原样转述**，读不出的留空（数字是 `None`，不是 0）。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct BridgeCall {
    pub id: String,
    /// 本机时区的 `MM-DD HH:MM:SS`，由桥给的 `started_at`（Unix 秒）换算；读不出留空。
    pub at: String,
    pub status: String,
    /// `cli` / `agent_sdk`（桥的 `invocation_mode`）。
    pub mode: String,
    pub model: String,
    pub effort: String,
    /// 送进去的字符数：系统提示词 + 酒馆消息（桥的 `input_chars`）。
    #[ts(type = "number | null")]
    pub input_chars: Option<i64>,
    /// 回复正文的字符数，含标点与换行（桥的 `output_chars`）。
    #[ts(type = "number | null")]
    pub output_chars: Option<i64>,
    /// 这一轮的输出 token，**含思考**（桥的 `usage.output_tokens`）。
    #[ts(type = "number | null")]
    pub output_tokens: Option<i64>,
    /// 读到的提示缓存 token（桥的 `cache.read_tokens`）。
    #[ts(type = "number | null")]
    pub cache_read: Option<i64>,
    /// 新写入的提示缓存 token（桥的 `cache.creation_tokens`）。
    #[ts(type = "number | null")]
    pub cache_write: Option<i64>,
    /// `identical` / `append_only` / `rewound` / `first` / `system_changed` /
    /// `settings_changed` / `history_rewritten`，原样（桥的 `prefix.state`）。
    pub prefix: String,
    /// 桥的 `duration_ms`。
    #[ts(type = "number | null")]
    pub elapsed_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct BridgeTelemetry {
    pub records: Vec<BridgeCall>,
    #[ts(type = "number")]
    pub total: i64,
    #[ts(type = "number")]
    pub offset: i64,
    pub has_more: bool,
    /// 缓存命中率 0–1。桥给的是百分数，这里除以 100。`None` = 桥没给这一项。
    pub hit_rate: Option<f32>,
    /// 全部记录读到的缓存 token 合计（桥的 `read_tokens`）。
    #[ts(type = "number | null")]
    pub cache_read: Option<i64>,
    /// 全部记录新写入的缓存 token 合计（桥的 `creation_tokens`）。
    #[ts(type = "number | null")]
    pub cache_write: Option<i64>,
    /// 可统计的调用数（桥的 `known_calls`：带回了缓存字段的那些）。
    #[ts(type = "number | null")]
    pub countable: Option<i64>,
    /// 前缀可复用的比例 0–1 = 可复用 ÷（可复用 + 被打断），跟桥自己的监控页同一个算法。
    /// 「首轮」两边都不算。`None` = 桥没给、或者还没有可判断的记录。
    pub prefix_reusable: Option<f32>,
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        // ⛔ 见模块头第 1 条。别删。
        .no_proxy()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| GateError::Other(format!("建不出 HTTP 客户端：{e}")))
}

fn token() -> Result<String> {
    let path = super::sillytavern::bridge_data_dir().join("bridge-token.txt");
    let raw = std::fs::read_to_string(&path).map_err(|e| {
        GateError::Other(format!(
            "读不到 Claude 桥接的密钥（{}）：{e}。它由 bridge.py 自己在启动时写下 —— 先起一次酒馆。",
            path.display()
        ))
    })?;
    let t = raw.trim().to_string();
    if t.is_empty() {
        return Err(GateError::Other(
            "Claude 桥接的密钥文件是空的：起一次酒馆让 bridge.py 重新写一份".into(),
        ));
    }
    Ok(t)
}

fn port() -> u16 {
    super::sillytavern::load_config().bridge_port
}

/// 把一次请求失败翻成一句能照着做的话。
///
/// ⛔ 「失败了」三个字对使用者没有任何指导意义：连不上要去起酒馆，
/// 404 要去升级 bridge.py，401 是密钥对不上 —— 三件事做法完全不同。
fn explain(status: Option<u16>, what: &str, e: Option<String>) -> GateError {
    GateError::Other(match status {
        None => format!(
            "连不上 Claude 桥接（127.0.0.1:{}）：{}。它没在跑 —— 在「官方账户 · Claude」点「酒馆」磁贴起一次。",
            port(),
            e.unwrap_or_else(|| "没有应答".into())
        ),
        Some(404) => format!(
            "这份 bridge.py 没有 {what} 这个接口（HTTP 404）。它是你自己那份、面板不分发 —— \
             升级到带这个接口的版本，或者照旧在酒馆里改。"
        ),
        Some(401) | Some(403) => {
            "Claude 桥接不认这个密钥（密钥文件跟正在跑的那个进程对不上）。停掉酒馆再起一次。".into()
        }
        Some(s) => format!("Claude 桥接回了 HTTP {s}，{what} 这一步没成。"),
    })
}

async fn get_json(path: &str, what: &str) -> Result<serde_json::Value> {
    let url = format!("http://127.0.0.1:{}{path}", port());
    let resp = client()?
        .get(&url)
        .bearer_auth(token()?)
        .send()
        .await
        .map_err(|e| explain(None, what, Some(e.to_string())))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(explain(Some(status.as_u16()), what, None));
    }
    resp.json()
        .await
        .map_err(|e| GateError::Other(format!("Claude 桥接的回复不是 JSON（{what}）：{e}")))
}

/// 桥在不在、什么版本、当前用着什么。**这一条也是能力探测。**
pub async fn health() -> Result<BridgeHealth> {
    let v = get_json("/health", "健康检查").await?;
    Ok(parse_health(&v))
}

/// 纯函数，单测拿录下来的形状打它。
pub fn parse_health(v: &serde_json::Value) -> BridgeHealth {
    parse_health_in(v, &chrono::Local)
}

fn parse_health_in<Tz: chrono::TimeZone>(v: &serde_json::Value, tz: &Tz) -> BridgeHealth
where
    Tz::Offset: std::fmt::Display,
{
    let s = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    BridgeHealth {
        sessions: v
            .pointer("/sdk_sessions/sessions")
            .and_then(serde_json::Value::as_array)
            .map(|a| a.iter().map(|x| parse_session_in(x, tz)).collect())
            .unwrap_or_default(),
        bridge_version: s("bridge_version"),
        invocation_mode: {
            // 2.4.0 同时给 `invocation_mode` 与 `current_mode`；老版本可能只有一个。
            let a = s("invocation_mode");
            if a.is_empty() {
                s("current_mode")
            } else {
                a
            }
        },
        model: s("model"),
        effort: s("effort"),
        cache_ttl: s("cache_ttl"),
        // 2.4.0：`{"status":"active","session_count":0,"sessions":[]}`。
        sdk_sessions: v.get("sdk_sessions").and_then(|x| {
            x.as_i64()
                .or_else(|| x.get("session_count").and_then(serde_json::Value::as_i64))
        }),
    }
}

pub async fn settings() -> Result<BridgeSettingsView> {
    let v = get_json("/api/settings", "读设置").await?;
    let settings: BridgeSettings = serde_json::from_value(v).map_err(|e| {
        GateError::Other(format!(
            "Claude 桥接的设置形状不认识：{e}。它是你自己那份 bridge.py —— 版本可能对不上。"
        ))
    })?;
    Ok(BridgeSettingsView {
        models: with_current(MODELS, &settings.model),
        efforts: with_current(EFFORTS, &settings.effort),
        cache_ttls: with_current(CACHE_TTLS, &settings.cache_ttl),
        prompt_modes: with_current(PROMPT_MODES, &settings.system_prompt_mode),
        invocation_modes: with_current(INVOCATION_MODES, &settings.invocation_mode),
        settings,
    })
}

/// 存设置。
///
/// ⚠ `bridge.py` 在这七项里**任何一项变了**的时候都会退掉当前那条 SDK 会话
/// （`cancel_active(retire=True)`）—— 界面上要写明，不然使用者会以为「改个提示词
/// 而已」，结果正在进行的那轮对话上下文没了。
pub async fn save_settings(next: &BridgeSettings) -> Result<()> {
    let url = format!("http://127.0.0.1:{}/api/settings", port());
    let resp = client()?
        .put(&url)
        .bearer_auth(token()?)
        .json(next)
        .send()
        .await
        .map_err(|e| explain(None, "存设置", Some(e.to_string())))?;
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    // 桥自己会说清楚是哪一项不合法（它有自己的 allowlist）。**照搬它的原话** ——
    // 我们这边那份快照可能比它旧，自己编一句「模型不合法」只会指错方向。
    let body = resp.text().await.unwrap_or_default();
    let said = body.trim();
    Err(if said.is_empty() {
        explain(Some(status.as_u16()), "存设置", None)
    } else {
        GateError::Other(format!(
            "Claude 桥接拒绝了这次保存（HTTP {status}）：{said}"
        ))
    })
}

pub async fn telemetry(offset: i64, limit: i64) -> Result<BridgeTelemetry> {
    let v = get_json(
        &format!("/api/telemetry?offset={offset}&limit={limit}"),
        "读调用日志",
    )
    .await?;
    Ok(parse_telemetry(&v))
}

/// 非负、有限的数，按 JSON 指针取。`bridge.py` 的 `duration_ms` 是小数，一并取整。
fn count_at(v: &serde_json::Value, pointer: &str) -> Option<i64> {
    v.pointer(pointer)
        .and_then(serde_json::Value::as_f64)
        .filter(|x| x.is_finite() && *x >= 0.0)
        .map(|x| x.round() as i64)
}

/// Unix 秒 → 给定时区的 `MM-DD HH:MM:SS`。时区是参数，单测才钉得住「东八区不错一天」（7.82）。
fn stamp_in<Tz: chrono::TimeZone>(tz: &Tz, secs: f64) -> Option<String>
where
    Tz::Offset: std::fmt::Display,
{
    if !secs.is_finite() || secs <= 0.0 {
        return None;
    }
    let whole = secs.floor();
    let nanos = ((secs - whole) * 1e9) as u32;
    tz.timestamp_opt(whole as i64, nanos)
        .single()
        .map(|t| t.format("%m-%d %H:%M:%S").to_string())
}

/// 纯函数，单测拿录下来的形状打它。**单测不联网。**
///
/// 形状照 `bridge.py` 2.4.0：`GET /api/telemetry` 回
/// `{records, total, offset, limit, has_more, cache_stats}`，`records` 每条是
/// `telemetry_summary` 的输出，`cache_stats` 是 `PersistentCallLog.cache_stats`。
pub fn parse_telemetry(v: &serde_json::Value) -> BridgeTelemetry {
    parse_telemetry_in(v, &chrono::Local)
}

fn parse_telemetry_in<Tz: chrono::TimeZone>(v: &serde_json::Value, tz: &Tz) -> BridgeTelemetry
where
    Tz::Offset: std::fmt::Display,
{
    let stats = v.get("cache_stats").unwrap_or(&serde_json::Value::Null);
    // `round(hits / known * 100, 1)`：桥给的是百分数，2.1.0 起每一版都是。
    let hit_rate = stats
        .get("hit_rate")
        .and_then(serde_json::Value::as_f64)
        .filter(|x| x.is_finite() && (0.0..=100.0).contains(x))
        .map(|x| (x / 100.0) as f32);
    // 跟桥自己的监控页一样：可复用 ÷（可复用 + 被打断）；「首轮」两边都不算。
    let prefix_reusable = match (
        count_at(stats, "/prefix_reusable"),
        count_at(stats, "/prefix_broken"),
    ) {
        (Some(ok), Some(broken)) if ok + broken > 0 => Some(ok as f32 / (ok + broken) as f32),
        _ => None,
    };
    BridgeTelemetry {
        records: v
            .get("records")
            .and_then(serde_json::Value::as_array)
            .map(|a| a.iter().map(|r| parse_call_in(r, tz)).collect())
            .unwrap_or_default(),
        total: v
            .get("total")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        offset: v
            .get("offset")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        has_more: v
            .get("has_more")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        hit_rate,
        cache_read: count_at(stats, "/read_tokens"),
        cache_write: count_at(stats, "/creation_tokens"),
        countable: count_at(stats, "/known_calls"),
        prefix_reusable,
    }
}

fn parse_call_in<Tz: chrono::TimeZone>(v: &serde_json::Value, tz: &Tz) -> BridgeCall
where
    Tz::Offset: std::fmt::Display,
{
    let s = |p: &str| {
        v.pointer(p)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    BridgeCall {
        id: s("/id"),
        at: v
            .get("started_at")
            .and_then(serde_json::Value::as_f64)
            .and_then(|secs| stamp_in(tz, secs))
            .unwrap_or_default(),
        status: s("/status"),
        mode: s("/invocation_mode"),
        model: s("/model"),
        effort: s("/effort"),
        input_chars: count_at(v, "/input_chars"),
        output_chars: count_at(v, "/output_chars"),
        output_tokens: count_at(v, "/usage/output_tokens"),
        cache_read: count_at(v, "/cache/read_tokens"),
        cache_write: count_at(v, "/cache/creation_tokens"),
        prefix: s("/prefix/state"),
        elapsed_ms: count_at(v, "/duration_ms"),
    }
}

fn parse_session_in<Tz: chrono::TimeZone>(v: &serde_json::Value, tz: &Tz) -> BridgeSession
where
    Tz::Offset: std::fmt::Display,
{
    let s = |p: &str| {
        v.pointer(p)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let at = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_f64)
            .and_then(|secs| stamp_in(tz, secs))
            .unwrap_or_default()
    };
    let message = s("/usage/cache_message");
    BridgeSession {
        chat_instance_id: s("/chat_instance_id"),
        sdk_session_id: s("/sdk_session_id"),
        status: s("/status"),
        created_at: at("created_at"),
        last_used_at: at("last_used_at"),
        input_tokens: count_at(v, "/usage/input_tokens"),
        output_tokens: count_at(v, "/usage/output_tokens"),
        cache_read: count_at(v, "/usage/cache_read_input_tokens"),
        cache_write: count_at(v, "/usage/cache_creation_input_tokens"),
        cache_message: if message.is_empty() {
            s("/usage/message")
        } else {
            message
        },
    }
}

/// 一条调用的全文。**只在使用者点「展开」时读**（桥的监控页也是这样：列表只有摘要）。
pub async fn call_detail(id: &str) -> Result<BridgeCallDetail> {
    let id = id.trim();
    if id.is_empty() || id.len() > 200 || id.chars().any(char::is_control) {
        return Err(GateError::Other("这条记录的 id 不对".into()));
    }
    let url = format!("http://127.0.0.1:{}/api/telemetry", port());
    let resp = client()?
        .get(&url)
        .query(&[("id", id)])
        .bearer_auth(token()?)
        .send()
        .await
        .map_err(|e| explain(None, "读这条记录的全文", Some(e.to_string())))?;
    let status = resp.status();
    if status.as_u16() == 404 {
        // 同一个 404 有两种意思：记录不在了（日志清空过），或者这份 bridge.py 没有单条接口。
        let body = resp.text().await.unwrap_or_default();
        return Err(if body.contains("telemetry record not found") {
            GateError::Other(
                "这条记录已经不在了（调用日志可能被清空过），点「刷新」重新读一遍。".into(),
            )
        } else {
            explain(Some(404), "读单条记录全文", None)
        });
    }
    if !status.is_success() {
        return Err(explain(Some(status.as_u16()), "读这条记录的全文", None));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| GateError::Other(format!("Claude 桥接的回复不是 JSON（读单条记录）：{e}")))?;
    Ok(parse_call_detail(&v))
}

/// 纯函数。字符串原样；不是字符串的（多段 content 之类）缩进成 JSON 显示；空的算没有。
pub fn parse_call_detail(v: &serde_json::Value) -> BridgeCallDetail {
    let record = v.get("record").unwrap_or(v);
    let text = |x: &serde_json::Value| -> Option<String> {
        match x {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) => Some(s.clone()),
            other => serde_json::to_string_pretty(other).ok(),
        }
        .filter(|s| !s.trim().is_empty())
    };
    let input = record.get("input").filter(|i| i.is_object());
    let known = input.is_some() || record.get("output").is_some();
    BridgeCallDetail {
        system_prompt: input.and_then(|i| i.get("system_prompt")).and_then(text),
        messages: input
            .and_then(|i| i.get("messages"))
            .and_then(serde_json::Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|m| BridgeMessage {
                        role: m
                            .get("role")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("?")
                            .to_string(),
                        content: m.get("content").and_then(text).unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        output: record.get("output").and_then(text),
        error: record.get("error").and_then(text),
        raw: (!known).then(|| serde_json::to_string_pretty(record).unwrap_or_default()),
    }
}

/// 清空调用日志。**不可逆** —— 调用方必须先弹确认框。
pub async fn clear_telemetry() -> Result<i64> {
    let url = format!("http://127.0.0.1:{}/api/telemetry", port());
    let resp = client()?
        .delete(&url)
        .bearer_auth(token()?)
        .send()
        .await
        .map_err(|e| explain(None, "清空调用日志", Some(e.to_string())))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(explain(Some(status.as_u16()), "清空调用日志", None));
    }
    let v: serde_json::Value = resp.json().await.unwrap_or_default();
    Ok(v.get("deleted")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bridges_current_value_is_always_a_legal_option_even_when_our_snapshot_is_older() {
        // 上游加了新模型时，面板这边的快照里没有它。不把当前值补进去的话，
        // 一打开设置下拉就落到列表第一项 —— 保存一次就把人家的模型悄悄换了。
        let got = with_current(MODELS, "claude-opus-6-brand-new");
        assert_eq!(got[0], "claude-opus-6-brand-new");
        assert!(got.iter().any(|m| m == "claude-opus-5"));
        // 已经在快照里的不重复加。
        let same = with_current(MODELS, "claude-opus-5");
        assert_eq!(same.len(), MODELS.len());
        // 空值不补。
        assert_eq!(with_current(EFFORTS, "").len(), EFFORTS.len());
    }

    #[test]
    fn a_missing_endpoint_says_which_endpoint_and_what_to_do() {
        // 「失败了」三个字没有指导意义：连不上要去起酒馆，404 要去升级 bridge.py。
        let e = explain(Some(404), "读设置", None).to_string();
        assert!(e.contains("读设置") && e.contains("404"));
        let down = explain(None, "读设置", Some("connection refused".into())).to_string();
        assert!(down.contains("没在跑"), "连不上要说得出下一步：{down}");
        let auth = explain(Some(401), "读设置", None).to_string();
        assert!(auth.contains("密钥"));
    }

    /// `bridge.py` 2.4.0 的 `GET /api/telemetry?offset=0&limit=1` 真实回的样子
    /// （2026-09-26 本机录的；id 换成了假的，文案字段截短，数字原样）。
    fn recorded_page() -> serde_json::Value {
        serde_json::json!({
            "records": [{
                "id": "chatcmpl-fixture", "started_at": 1790431110.637681, "stream": true,
                "invocation_mode": "cli", "chat_instance_id": null, "sdk_session_id": null,
                "model": "claude-opus-5", "effort": "max", "cache_ttl": "1h",
                "system_prompt_mode": "append", "status": "ok", "error": null,
                "duration_ms": 53209.0,
                "usage": {
                    "available": true, "input_tokens": 2, "output_tokens": 2545,
                    "cache_read_input_tokens": 16836, "cache_creation_input_tokens": 2014,
                    "cache_status": "hit_and_created", "scope": "turn"
                },
                "cumulative_usage": { "available": true, "output_tokens": 2545 },
                "cache": {
                    "status": "hit_and_created", "read_tokens": 16836,
                    "creation_tokens": 2014, "ttl": "1h"
                },
                "prefix": {
                    "state": "append_only", "stable_chars": 12675, "total_chars": 13523,
                    "label": "只在末尾追加", "stable_ratio": 93.7
                },
                "input_chars": 13400, "message_count": 3, "output_chars": 671
            }],
            "total": 217, "offset": 0, "limit": 1, "has_more": true,
            "cache_stats": {
                "calls": 217, "known_calls": 200, "hits": 151, "created": 49, "misses": 0,
                "unknown": 17, "hit_rate": 75.5, "read_tokens": 2878730,
                "creation_tokens": 2370262,
                "prefix_states": { "append_only": 138, "identical": 14, "first": 38 },
                "prefix_reusable": 153, "prefix_broken": 26
            }
        })
    }

    fn utc_offset(hours: i32) -> chrono::FixedOffset {
        chrono::FixedOffset::east_opt(hours * 3600).unwrap()
    }

    #[test]
    fn a_real_bridge_page_fills_every_column() {
        // 0.32.0 按想象出来的字段名读，这一页读出来是「输入 0 · 输出 0」「读 0 · 写 0」、
        // 时间耗时全是「—」、命中率「—」。这里每一格都要是桥真给的那个数。
        let t = parse_telemetry_in(&recorded_page(), &utc_offset(-4));
        let c = &t.records[0];
        assert_eq!(c.at, "09-26 09:58:30");
        assert_eq!(c.status, "ok");
        assert_eq!(c.mode, "cli");
        assert_eq!(
            (c.model.as_str(), c.effort.as_str()),
            ("claude-opus-5", "max")
        );
        assert_eq!(c.input_chars, Some(13400));
        assert_eq!(
            c.output_chars,
            Some(671),
            "回复字数就是使用者要对照的那个数"
        );
        assert_eq!(c.output_tokens, Some(2545));
        assert_eq!((c.cache_read, c.cache_write), (Some(16836), Some(2014)));
        assert_eq!(c.prefix, "append_only");
        assert_eq!(c.elapsed_ms, Some(53209));

        assert_eq!((t.total, t.has_more), (217, true));
        assert!((t.hit_rate.unwrap() - 0.755).abs() < 1e-6);
        assert_eq!(t.countable, Some(200));
        assert_eq!(
            (t.cache_read, t.cache_write),
            (Some(2878730), Some(2370262))
        );
        // 桥自己的监控页写的是「153 / 179」。
        assert!((t.prefix_reusable.unwrap() - 153.0 / 179.0).abs() < 1e-6);
    }

    #[test]
    fn the_call_time_is_converted_in_the_given_zone_across_midnight() {
        // 2026-09-25T20:30:00Z：东八区已经是第二天凌晨（7.82 错一天就错在这种时刻）。
        let page = serde_json::json!({ "records": [{ "id": "x", "started_at": 1790368200.0 }] });
        assert_eq!(
            parse_telemetry_in(&page, &utc_offset(8)).records[0].at,
            "09-26 04:30:00"
        );
        assert_eq!(
            parse_telemetry_in(&page, &utc_offset(-4)).records[0].at,
            "09-25 16:30:00"
        );
        // 读不出的时间留空（界面显示「—」），不编一个 1970 年出来。
        let bad = serde_json::json!({ "records": [{ "id": "y", "started_at": "soon" }] });
        assert_eq!(parse_telemetry_in(&bad, &utc_offset(8)).records[0].at, "");
    }

    #[test]
    fn hit_rate_is_read_as_the_percentage_the_bridge_writes() {
        let stats = |rate: serde_json::Value| {
            parse_telemetry(
                &serde_json::json!({ "records": [], "cache_stats": { "hit_rate": rate } }),
            )
            .hit_rate
        };
        // `round(hits / known * 100, 1)` —— 97 就是 97%。
        assert!((stats(serde_json::json!(97)).unwrap() - 0.97).abs() < 1e-6);
        assert_eq!(
            stats(serde_json::json!(150)),
            None,
            "超过 100% 不是百分数，不换算"
        );
        assert_eq!(stats(serde_json::json!(-1)), None);
        // 桥在一条可统计记录都没有时给 null：那是「不知道」，不是 0%。
        assert_eq!(stats(serde_json::Value::Null), None);
    }

    #[test]
    fn a_record_with_missing_fields_reads_as_unknown_not_as_zero() {
        let t = parse_telemetry(&serde_json::json!({ "records": [{ "id": "c2" }], "total": 1 }));
        let c = &t.records[0];
        // 缺字段的记录不该整条丢掉 —— 缺的那几项是「不知道」，剩下的照样看得见。
        assert_eq!(c.id, "c2");
        assert_eq!(
            (c.status.as_str(), c.at.as_str(), c.prefix.as_str()),
            ("", "", "")
        );
        assert_eq!(
            (c.input_chars, c.output_chars, c.output_tokens),
            (None, None, None)
        );
        assert_eq!(
            (c.cache_read, c.cache_write, c.elapsed_ms),
            (None, None, None)
        );
        // 整页没有 cache_stats：每一格都是「—」，不是 0。
        assert_eq!((t.hit_rate, t.prefix_reusable), (None, None));
        assert_eq!(
            (t.cache_read, t.cache_write, t.countable),
            (None, None, None)
        );
        assert!(!t.has_more);
    }

    #[test]
    fn the_prefix_ratio_leaves_first_calls_out_and_needs_something_to_judge() {
        let ratio = |ok: i64, broken: i64| {
            parse_telemetry(&serde_json::json!({
                "records": [],
                "cache_stats": { "prefix_reusable": ok, "prefix_broken": broken }
            }))
            .prefix_reusable
        };
        assert_eq!(ratio(3, 1), Some(0.75));
        assert_eq!(ratio(0, 0), None, "全是首轮时没有可判断的，不是 0%");
    }

    #[test]
    fn health_reads_the_session_count_out_of_the_2_4_0_object() {
        let h = parse_health(&serde_json::json!({
            "status": "ok", "bridge_version": "2.4.0", "invocation_mode": "cli",
            "model": "claude-opus-5", "effort": "max", "cache_ttl": "1h",
            "sdk_sessions": { "status": "active", "session_count": 2, "sessions": [] }
        }));
        assert_eq!(h.bridge_version, "2.4.0");
        assert_eq!(h.sdk_sessions, Some(2));
        assert_eq!(
            parse_health(&serde_json::json!({ "sdk_sessions": 3 })).sdk_sessions,
            Some(3)
        );
        assert_eq!(parse_health(&serde_json::json!({})).sdk_sessions, None);
        // 老版本只给 `current_mode`。
        let old = parse_health(&serde_json::json!({ "current_mode": "agent_sdk" }));
        assert_eq!(old.invocation_mode, "agent_sdk");
        // CLI 模式：没有常驻会话，列表是空的（不是「读不出」）。
        assert!(h.sessions.is_empty());
    }

    /// 形状照 `bridge.py` 2.4.0 的 `SdkRunner.session_status(None)` 与 `summarize_model_usage`
    /// 源码写（2026-09-29 读的；本机跑的是 CLI 模式，没有真实会话可录）。
    #[test]
    fn health_lists_each_sdk_session_with_its_last_round() {
        let h = parse_health_in(
            &serde_json::json!({
                "status": "ok", "bridge_version": "2.4.0", "invocation_mode": "agent_sdk",
                "sdk_sessions": { "status": "active", "session_count": 1, "sessions": [{
                    "chat_instance_id": "chat-fixture", "sdk_session_id": "sdk-fixture",
                    "status": "idle", "created_at": 1790368200.0, "last_used_at": 1790368260.5,
                    "usage": {
                        "available": true, "input_tokens": 12, "output_tokens": 830,
                        "cache_read_input_tokens": 15000, "cache_creation_input_tokens": null,
                        "cache_status": "hit", "cache_message": "本轮读取到了提示缓存", "scope": "turn"
                    },
                    "cumulative_usage": { "available": true }
                }] }
            }),
            &utc_offset(8),
        );
        assert_eq!(h.sdk_sessions, Some(1));
        let s = &h.sessions[0];
        assert_eq!(
            (
                s.chat_instance_id.as_str(),
                s.sdk_session_id.as_str(),
                s.status.as_str()
            ),
            ("chat-fixture", "sdk-fixture", "idle")
        );
        assert_eq!(s.created_at, "09-26 04:30:00");
        assert_eq!(s.last_used_at, "09-26 04:31:00");
        assert_eq!((s.input_tokens, s.output_tokens), (Some(12), Some(830)));
        // 桥给 null 的那一项是「不知道」，不是 0。
        assert_eq!((s.cache_read, s.cache_write), (Some(15000), None));
        assert_eq!(s.cache_message, "本轮读取到了提示缓存");
        // usage 缺明细时桥写的是 `message`。
        let bare = parse_health(&serde_json::json!({ "sdk_sessions": { "sessions": [{
            "chat_instance_id": "c", "status": "not_found",
            "usage": { "available": false, "message": "Claude 未返回 usage 明细" }
        }] } }));
        assert_eq!(bare.sessions[0].cache_message, "Claude 未返回 usage 明细");
        assert_eq!(bare.sessions[0].output_tokens, None);
    }

    #[test]
    fn a_call_detail_reads_prompt_messages_and_reply_as_the_bridge_stores_them() {
        let d = parse_call_detail(&serde_json::json!({ "record": {
            "id": "chatcmpl-fixture", "status": "ok",
            "input": {
                "system_prompt": "你是林澈。",
                "messages": [
                    { "role": "user", "content": "你好" },
                    { "role": "assistant", "content": [{ "type": "text", "text": "多段" }] }
                ]
            },
            "output": "林澈抬起头。", "error": null
        } }));
        assert_eq!(d.system_prompt.as_deref(), Some("你是林澈。"));
        assert_eq!(d.messages.len(), 2);
        assert_eq!(
            (d.messages[0].role.as_str(), d.messages[0].content.as_str()),
            ("user", "你好")
        );
        // 不是字符串的 content 不丢，缩进成 JSON 给人看。
        assert!(d.messages[1].content.contains("多段"));
        assert_eq!(d.output.as_deref(), Some("林澈抬起头。"));
        assert_eq!((d.error, d.raw), (None, None));

        // 认不出的形状：整条原样给出来，不编。
        let odd = parse_call_detail(&serde_json::json!({ "record": { "prompt": "x" } }));
        assert!(odd.raw.as_deref().is_some_and(|r| r.contains("prompt")));
        assert!(odd.messages.is_empty() && odd.output.is_none());
        // 出错的那一条：错误原话。
        let failed = parse_call_detail(&serde_json::json!({ "record": {
            "input": { "messages": [] }, "output": "", "error": "claude exited 1"
        } }));
        assert_eq!(failed.output, None, "空回复算没有");
        assert_eq!(failed.error.as_deref(), Some("claude exited 1"));
    }
}
