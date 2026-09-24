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
    /// 正在跑的 SDK 会话数。
    #[ts(type = "number")]
    pub sdk_sessions: i64,
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

/// 一条调用记录。**字段按 `bridge.py` 给的原样转述**，读不出的留空。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct BridgeCall {
    pub id: String,
    /// 本地 `MM-DD HH:MM:SS`，读不出就是原样那串。
    pub at: String,
    pub status: String,
    pub mode: String,
    pub model: String,
    pub effort: String,
    #[ts(type = "number")]
    pub input: i64,
    #[ts(type = "number")]
    pub output: i64,
    #[ts(type = "number")]
    pub cache_read: i64,
    #[ts(type = "number")]
    pub cache_write: i64,
    /// `identical` / `append_only` / `rewound` / `first` / `system_changed` /
    /// `settings_changed` / `history_rewritten`，原样。
    pub prefix: String,
    #[ts(type = "number")]
    pub elapsed_ms: i64,
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
    /// 缓存命中率 0–1。`None` = 桥没给这一项。
    pub hit_rate: Option<f32>,
    #[ts(type = "number")]
    pub cache_read: i64,
    #[ts(type = "number")]
    pub cache_write: i64,
    /// 可统计的调用数（桥自己的口径）。
    #[ts(type = "number")]
    pub countable: i64,
    /// 前缀可复用的比例 0–1。`None` = 桥没给。
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
            "连不上 Claude 桥接（127.0.0.1:{}）：{}。它没在跑 —— 到状态条点「启动酒馆」。",
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
    let s = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Ok(BridgeHealth {
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
        sdk_sessions: v
            .get("sdk_sessions")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
    })
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

/// 纯函数，单测拿录下来的形状打它。**单测不联网。**
pub fn parse_telemetry(v: &serde_json::Value) -> BridgeTelemetry {
    let stats = v.get("cache_stats");
    let f = |k: &str| {
        stats
            .and_then(|s| s.get(k))
            .and_then(serde_json::Value::as_f64)
            .filter(|x| x.is_finite() && (0.0..=1.0).contains(x))
            .map(|x| x as f32)
    };
    let i = |k: &str| {
        stats
            .and_then(|s| s.get(k))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
    };
    BridgeTelemetry {
        records: v
            .get("records")
            .and_then(serde_json::Value::as_array)
            .map(|a| a.iter().map(parse_call).collect())
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
        hit_rate: f("hit_rate"),
        cache_read: i("cache_read"),
        cache_write: i("cache_write"),
        countable: i("countable"),
        prefix_reusable: f("prefix_reusable"),
    }
}

fn parse_call(v: &serde_json::Value) -> BridgeCall {
    let s = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let n = |k: &str| v.get(k).and_then(serde_json::Value::as_i64).unwrap_or(0);
    BridgeCall {
        id: s("id"),
        at: s("at"),
        status: s("status"),
        mode: s("mode"),
        model: s("model"),
        effort: s("effort"),
        input: n("input"),
        output: n("output"),
        cache_read: n("cache_read"),
        cache_write: n("cache_write"),
        prefix: s("prefix"),
        elapsed_ms: n("elapsed_ms"),
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

    #[test]
    fn telemetry_stats_outside_zero_to_one_are_dropped_rather_than_rescaled() {
        let v = serde_json::json!({
            "records": [], "total": 0, "offset": 0, "has_more": false,
            "cache_stats": { "hit_rate": 97, "cache_read": 12, "countable": 3 }
        });
        let t = parse_telemetry(&v);
        // 97 不是比例。换算成 0.97 就是在猜口径 —— 猜错方向恰好是「看起来很省」。
        assert_eq!(t.hit_rate, None);
        assert_eq!(t.cache_read, 12);
        assert_eq!(t.countable, 3);
    }

    #[test]
    fn a_reply_without_cache_stats_reads_as_unknown_not_as_zero() {
        let t = parse_telemetry(&serde_json::json!({ "records": [], "total": 0 }));
        assert_eq!(t.hit_rate, None, "没给不等于 0%");
        assert_eq!(t.prefix_reusable, None);
        assert!(!t.has_more);
    }

    #[test]
    fn a_call_record_keeps_whatever_the_bridge_said_and_never_invents_fields() {
        let v = serde_json::json!({
            "id": "c1", "at": "09-21 10:00:00", "status": "ok", "mode": "cli",
            "model": "claude-opus-5", "effort": "max",
            "input": 100, "output": 20, "cache_read": 80, "cache_write": 0,
            "prefix": "append_only", "elapsed_ms": 1234
        });
        let c = parse_call(&v);
        assert_eq!(c.id, "c1");
        assert_eq!(c.prefix, "append_only");
        assert_eq!(c.elapsed_ms, 1234);
        // 缺字段的记录不该整条丢掉 —— 缺的那几项留空，剩下的照样看得见。
        let sparse = parse_call(&serde_json::json!({ "id": "c2" }));
        assert_eq!(sparse.id, "c2");
        assert_eq!(sparse.status, "");
        assert_eq!(sparse.input, 0);
    }
}
