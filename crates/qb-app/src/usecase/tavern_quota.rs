//! GPT / Gemini CLI 的联网额度，与酒馆桥接的角色扮演自检。
//!
//! 这条路径是使用者显式授权的内部额度查询：GPT 对应 ChatGPT 的 `wham/usage`，
//! Gemini CLI 对应 Code Assist 的 `loadCodeAssist` / `retrieveUserQuota`。
//!
//! # 只在点刷新时联网（2026-09-23 使用者定的）
//!
//! 每个查询函数都带一个 `refresh`：
//!
//! | `refresh` | 做什么 |
//! |---|---|
//! | `false` | 只读「最近一次」问到的（[`LAST_GPT`] / [`LAST_GEMINI`]），**绝不联网**；没问过就是 `None` |
//! | `true` | 联网问一次，成功就记成「最近一次」 |
//!
//! 打开页面、切页回来、打开酒馆弹窗都走 `false`。0.32.0 之后那一版 GPT 页挂载时查一次、
//! 之后**每 5 分钟对所有槽位**各查一次（窗口缩到托盘也照查），而文档写的是「只在打开或点刷新时」——
//! 现在联网只从使用者点的那颗刷新图标发出，一次只查那一个账户。「最近一次」没有 TTL：
//! 界面标明是几点查的，要新的就再点一次。
//!
//! 令牌只在请求作用域内存在，不进入日志、快照或 TS 返回值。
//!
//! # 登录态（2026-09-23，使用者：「GPT 和 Gemini 的登录态对着那个开源项目修一下」）
//!
//! 两条原来都犯同一个错：**把「访问令牌到点了」说成「登录失效了」**。拿着过期的访问令牌去问，
//! 服务端回 401，界面就说「登录令牌已失效，请在对应官方客户端重新登录」—— 而那个账户登得
//! 好好的，官方客户端下次跑起来会自己换新。照 cockpit-tools 的做法（读源码核对的事实，一行没抄）
//! 分开处理：
//!
//! | | Gemini CLI | GPT（Codex） |
//! |---|---|---|
//! | 访问令牌过期 | 在内存里换一张再问（[`super::google_oauth`]，同反重力） | **不换**，如实说「到点了，开一下桌面端它会自己换新」 |
//! | 为什么不一样 | Google 的刷新令牌换新之后不作废，CLI 手里那份照样能用 | OpenAI 的刷新令牌每换一次就轮换：面板一换，桌面端手里那份就作废，下次它换新时被登出、弹出登录页 |
//! | 服务端真的不认了 | `invalid_grant` | 访问令牌没到点却回 401 |
//! | 那之后 | 记进 [`super::login_health`]：账户行上改说「登录已失效」、露出「登录」 | 同左 |

use super::google_oauth::{self, LocalClients, Refreshed, TokenCache};
use super::login_health;
use crate::error::{GateError, Result};
use chrono::{DateTime, Local, TimeZone, Utc};
use qb_extensions::plugins::sillytavern::TavernBackend;
use qb_platform::credentials::Secret;
use reqwest::StatusCode;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use ts_rs::TS;

/// 「最近一次」问到的结果，按槽位 id 存。**只存最近一次，没有 TTL。**
struct Last<T>(Mutex<Option<HashMap<String, T>>>);

impl<T: Clone> Last<T> {
    const fn new() -> Self {
        Self(Mutex::new(None))
    }
    fn get(&self, id: &str) -> Option<T> {
        self.0.lock().ok()?.as_ref()?.get(id).cloned()
    }
    fn put(&self, id: &str, value: T) {
        if let Ok(mut g) = self.0.lock() {
            g.get_or_insert_with(HashMap::new)
                .insert(id.to_string(), value);
        }
    }
    fn forget(&self, id: &str) {
        if let Ok(mut g) = self.0.lock() {
            if let Some(m) = g.as_mut() {
                m.remove(id);
            }
        }
    }
}

/// GPT：按 Codex 槽位 id。
static LAST_GPT: Last<TavernGptQuota> = Last::new();
/// Gemini CLI：按反重力账户槽位 id（CLI 是那条槽位的一半）。
static LAST_GEMINI: Last<TavernGeminiQuota> = Last::new();

/// Gemini CLI 换新来的访问令牌，按账户槽位存。**只在内存里**（见模块头）。
static GEMINI_TOKENS: TokenCache = TokenCache::new();
/// Gemini CLI 的客户端标识上次认准的是 `(文件, 第几组)`。**不存标识本身。**
static GEMINI_CLIENT: Mutex<Option<(PathBuf, usize)>> = Mutex::new(None);

/// Gemini CLI 换不下来时那两句话（[`google_oauth::refresh_error`]）。
const GEMINI_RELOGIN: &str = "点这个账户 CLI 那一半旁边的「登录」，在弹出的窗口里重新登录一次。";
const GEMINI_SELF_REFRESH: &str =
    "在这个账户的 Gemini CLI 里随便跑一句（或起一次酒馆的 Gemini 桥接），让它自己换新，再点刷新。";

/// 槽位移除之后调：别让一个不存在的槽位的额度一直躺在内存里。
pub fn forget_gpt(slot_id: &str) {
    LAST_GPT.forget(slot_id);
    login_health::forget(&login_health::codex_key(slot_id));
}

/// 同上，Gemini CLI 那一半（额度、换新来的令牌、「登录已失效」那一笔）。
pub fn forget_gemini(account_id: &str) {
    LAST_GEMINI.forget(account_id);
    GEMINI_TOKENS.forget(account_id);
    login_health::forget(&login_health::gemini_cli_key(account_id));
}

const GPT_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const GEMINI_ENDPOINT: &str = "https://cloudcode-pa.googleapis.com/v1internal:";

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TavernQuotaWindow {
    pub label: String,
    pub remaining_percent: Option<u8>,
    pub reset_at: Option<String>,
    #[ts(type = "number | null")]
    pub reset_epoch: Option<i64>,
    #[ts(type = "number | null")]
    pub window_minutes: Option<i64>,
    pub present: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TavernGptQuota {
    pub provider: String,
    pub account: Option<String>,
    pub plan_type: Option<String>,
    pub windows: Vec<TavernQuotaWindow>,
    pub fetched_at: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TavernGeminiModelQuota {
    pub model_id: Option<String>,
    pub label: String,
    pub token_type: Option<String>,
    pub remaining_percent: Option<u8>,
    pub reset_at: Option<String>,
    #[ts(type = "number | null")]
    pub reset_epoch: Option<i64>,
    /// 这一格只有 `resetTime`、没有 `remainingFraction`，按 0 算的（proto3 的 JSON 把零值字段整个省掉）。
    /// 界面上要说出来（CLAUDE.md「联网额度」第 6 条，2026-09-25 补上 —— 原来这一格被当成「没读到」，
    /// 用光的那一格反而不显示，「剩余配额」取到的是别的、更高的那一格）。
    pub remaining_implied: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TavernGeminiQuota {
    pub provider: String,
    pub account: Option<String>,
    pub project: Option<String>,
    pub tier: Option<String>,
    pub models: Vec<TavernGeminiModelQuota>,
    pub fetched_at: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TavernRoleplayTest {
    pub provider: String,
    pub ok: bool,
    pub model: Option<String>,
    pub reply: Option<String>,
    #[ts(type = "number | null")]
    pub latency_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub input_tokens: Option<i64>,
    #[ts(type = "number | null")]
    pub output_tokens: Option<i64>,
    pub detail: String,
    pub tested_at: String,
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .user_agent("QB-Gate/0.32 tavern-quota")
        .build()
        .map_err(|e| GateError::Other(format!("额度查询客户端创建失败：{e}")))
}

fn local_bridge_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        // A CLI-backed bridge can legitimately need a couple of minutes for
        // the first request.  This client is local-only and must not inherit
        // HTTP(S)_PROXY, otherwise localhost roleplay tests can be misrouted.
        .timeout(Duration::from_secs(330))
        .no_proxy()
        .user_agent("QB-Gate/0.32 tavern-roleplay")
        .build()
        .map_err(|e| GateError::Other(format!("角色扮演测试客户端创建失败：{e}")))
}

fn local_now() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn epoch_of(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(if n > 100_000_000_000 { n / 1000 } else { n });
    }
    let s = v.as_str()?;
    if let Ok(n) = s.parse::<i64>() {
        return Some(if n > 100_000_000_000 { n / 1000 } else { n });
    }
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp())
}

fn display_time(epoch: Option<i64>) -> Option<String> {
    epoch
        .and_then(|s| Local.timestamp_opt(s, 0).single())
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
}

fn status_error(status: StatusCode, body: &str, service: &str) -> GateError {
    let hint = match status {
        StatusCode::UNAUTHORIZED => "登录令牌已失效，请在对应官方客户端重新登录",
        StatusCode::FORBIDDEN => "服务商拒绝了内部额度查询，可能是账户或接口版本不支持",
        StatusCode::TOO_MANY_REQUESTS => "额度接口被限流，请稍后再刷新",
        _ => "接口暂时不可用或已改版",
    };
    let code = serde_json::from_str::<Value>(body).ok().and_then(|v| {
        v.pointer("/error/code")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    GateError::Other(match code {
        Some(code) => format!("{service} 额度查询失败（HTTP {status}，{code}）：{hint}"),
        None => format!("{service} 额度查询失败（HTTP {status}）：{hint}"),
    })
}

fn number_value(v: &Value) -> Option<f64> {
    match v {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn first_field<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|key| v.get(*key).filter(|value| !value.is_null()))
}

fn window_seconds(v: &Value) -> Option<i64> {
    first_field(
        v,
        &[
            "limit_window_seconds",
            "limitWindowSeconds",
            "window_seconds",
            "windowSeconds",
            "duration_seconds",
            "durationSeconds",
        ],
    )
    .and_then(number_value)
    .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
    .map(|seconds| seconds.round() as i64)
}

fn percent_value(v: &Value, keys: &[&str]) -> Option<u8> {
    first_field(v, keys)
        .and_then(number_value)
        .filter(|percent| percent.is_finite())
        .map(|percent| percent.round().clamp(0.0, 100.0) as u8)
}

fn fraction_value(v: &Value, keys: &[&str]) -> Option<u8> {
    first_field(v, keys)
        .and_then(number_value)
        .filter(|fraction| fraction.is_finite() && (0.0..=1.0).contains(fraction))
        .map(|fraction| (fraction * 100.0).round() as u8)
}

fn gpt_window(v: Option<&Value>, fallback: &str) -> TavernQuotaWindow {
    let Some(v) = v else {
        return TavernQuotaWindow {
            label: fallback.to_string(),
            remaining_percent: None,
            reset_at: None,
            reset_epoch: None,
            window_minutes: None,
            present: false,
        };
    };
    let remaining_percent = percent_value(
        v,
        &[
            "percent_left",
            "percentLeft",
            "remaining_percent",
            "remainingPercent",
        ],
    )
    .or_else(|| fraction_value(v, &["remaining_fraction", "remainingFraction"]))
    .or_else(|| {
        percent_value(v, &["used_percent", "usedPercent"]).map(|used| 100u8.saturating_sub(used))
    });
    let window_minutes = window_seconds(v).map(|seconds| (seconds + 59) / 60);
    let reset_epoch = first_field(
        v,
        &[
            "reset_at",
            "resetAt",
            "reset_time",
            "resetTime",
            "reset_time_ms",
            "resetTimeMs",
        ],
    )
    .and_then(epoch_of)
    .or_else(|| {
        first_field(
            v,
            &[
                "reset_after_seconds",
                "resetAfterSeconds",
                "reset_after",
                "resetAfter",
            ],
        )
        .and_then(number_value)
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| Utc::now().timestamp() + seconds.round() as i64)
    });
    TavernQuotaWindow {
        label: window_minutes
            .map(|minutes| match minutes {
                300 => "5 小时".to_string(),
                10080 => "7 天".to_string(),
                m if m % 1440 == 0 => format!("{} 天", m / 1440),
                m if m % 60 == 0 => format!("{} 小时", m / 60),
                m => format!("{} 分钟", m),
            })
            .unwrap_or_else(|| fallback.to_string()),
        remaining_percent,
        reset_at: display_time(reset_epoch),
        reset_epoch,
        window_minutes,
        // A duration alone is still useful: it tells the UI which unknown
        // window was returned, and Gauge will render it as an indeterminate
        // striped slot instead of claiming that it is empty.
        present: remaining_percent.is_some() || reset_epoch.is_some() || window_minutes.is_some(),
    }
}

fn looks_like_gpt_window(v: &Value) -> bool {
    v.is_object()
        && [
            "used_percent",
            "usedPercent",
            "percent_left",
            "percentLeft",
            "remaining_percent",
            "remainingPercent",
            "remaining_fraction",
            "remainingFraction",
            "limit_window_seconds",
            "limitWindowSeconds",
            "reset_at",
            "resetAt",
            "reset_time_ms",
            "resetTimeMs",
            "reset_after_seconds",
            "resetAfterSeconds",
        ]
        .iter()
        .any(|key| v.get(*key).is_some())
}

/// 没带时长、也没有键名说明是哪一格的窗口：**按位置猜「5 小时 / 7 天」不行**（2026-09-25）——
/// 只剩一个每周窗口时它排第一，会被叫成「5 小时」。跟本机快照那条（`codex::ratelimit::window_name`）
/// 同一条规矩：认不出来就说认不出来；带序号是为了两格不同名（界面拿名字当 key）。
fn unknown_window(index: usize) -> &'static str {
    const NAMES: [&str; 4] = [
        "窗口 1（时长未知）",
        "窗口 2（时长未知）",
        "窗口 3（时长未知）",
        "窗口 4（时长未知）",
    ];
    NAMES[index.min(NAMES.len() - 1)]
}

fn gpt_candidates(rate: &Value) -> Vec<(&Value, &'static str)> {
    let mut candidates = Vec::new();
    if let Some(items) = rate.as_array() {
        for (index, item) in items.iter().enumerate() {
            if looks_like_gpt_window(item) {
                candidates.push((item, unknown_window(index)));
            }
        }
    }
    if let Some(object) = rate.as_object() {
        for (key, fallback) in [
            ("five_hour", "5 小时"),
            ("fiveHour", "5 小时"),
            ("primary_window", "5 小时"),
            ("primaryWindow", "5 小时"),
            ("primary", "5 小时"),
            ("weekly", "7 天"),
            ("seven_day", "7 天"),
            ("sevenDay", "7 天"),
            ("secondary_window", "7 天"),
            ("secondaryWindow", "7 天"),
            ("secondary", "7 天"),
        ] {
            if let Some(value) = object.get(key).filter(|value| looks_like_gpt_window(value)) {
                candidates.push((value, fallback));
            }
        }
        for nested_key in ["windows", "rate_limits", "limits"] {
            if let Some(items) = object.get(nested_key).and_then(Value::as_array) {
                for (index, item) in items.iter().enumerate() {
                    if looks_like_gpt_window(item) {
                        candidates.push((item, unknown_window(index)));
                    }
                }
            }
        }
        if candidates.is_empty() && looks_like_gpt_window(rate) {
            candidates.push((rate, unknown_window(0)));
        }
    }
    let mut unique = Vec::with_capacity(candidates.len());
    for (value, fallback) in candidates {
        if unique
            .iter()
            .any(|(seen, _): &(&Value, &str)| std::ptr::eq(*seen, value))
        {
            continue;
        }
        unique.push((value, fallback));
    }
    unique
}

pub fn parse_gpt(body: &str, account: Option<String>) -> Result<TavernGptQuota> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| GateError::Other(format!("GPT 额度响应不是 JSON：{e}")))?;
    let rate = v
        .get("rate_limits")
        .filter(|value| !value.is_null())
        .or_else(|| v.get("rate_limit").filter(|value| !value.is_null()))
        .unwrap_or(&Value::Null);
    let windows: Vec<_> = gpt_candidates(rate)
        .into_iter()
        .map(|(window, fallback)| gpt_window(Some(window), fallback))
        .collect();
    if windows.is_empty() || windows.iter().all(|window| !window.present) {
        return Err(GateError::Other("GPT 额度响应里没有可识别的窗口".into()));
    }
    Ok(TavernGptQuota {
        provider: "gpt".into(),
        account,
        plan_type: v
            .get("plan_type")
            .and_then(Value::as_str)
            .or_else(|| v.get("planType").and_then(Value::as_str))
            .map(str::to_owned),
        windows,
        fetched_at: local_now(),
        source: GPT_USAGE_URL.into(),
    })
}

/// 一个 GPT 槽位的额度（`slot_id = None` = 激活槽位，酒馆弹窗用）。**不改变激活账户。**
///
/// `refresh = false` 只读「最近一次」，绝不联网；`true` 联网问一次（见模块头）。
pub async fn gpt_quota(slot_id: Option<&str>, refresh: bool) -> Result<Option<TavernGptQuota>> {
    let id = match slot_id {
        Some(id) => id.to_string(),
        None => match qb_accounts::codex::active_id(&qb_accounts::codex::root())? {
            Some(id) => id,
            None if refresh => return Err(GateError::Other("没有激活的 GPT 槽位".into())),
            None => return Ok(None),
        },
    };
    if !refresh {
        return Ok(LAST_GPT.get(&id));
    }
    let q = fetch_gpt(&id).await?;
    LAST_GPT.put(&id, q.clone());
    crate::audit::write(&format!(
        "问了一次 GPT 槽位「{}」的额度（只读，不记内容）",
        q.account.as_deref().unwrap_or(&id)
    ));
    Ok(Some(q))
}

async fn fetch_gpt(slot_id: &str) -> Result<TavernGptQuota> {
    let Some((label, home, _)) = qb_accounts::codex::slot(&qb_accounts::codex::root(), slot_id)?
    else {
        return Err(GateError::Other("GPT 槽位不存在".into()));
    };
    // 额度只认访问令牌（cockpit-tools 同一个口径）：刷新令牌在不在、id_token 过没过期都不挡。
    let token = qb_accounts::codex::read_access_token(&home)?
        .ok_or_else(|| GateError::Other(format!("GPT 槽位「{label}」还没有登录")))?;
    // 到点了就别发 —— 发出去只会换来一个 401，而那不是「登录失效」。
    if let Some(exp) = qb_accounts::codex::access_token_expiry(&token) {
        if exp <= Utc::now().timestamp() + 60 {
            return Err(gpt_token_expired(&label, Some(exp)));
        }
    }
    let http = client()?;
    let mut request = http
        .get(GPT_USAGE_URL)
        .bearer_auth(&token)
        .header("accept", "application/json");
    if let Some(account_id) = qb_accounts::codex::chatgpt_account_id_from_home(&home)? {
        request = request.header("ChatGPT-Account-Id", account_id);
    }
    let response = request
        .send()
        .await
        .map_err(|e| GateError::Other(format!("GPT 额度接口连接失败：{e}")))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status == StatusCode::UNAUTHORIZED {
        let code = error_code(&body);
        if code.as_deref() == Some("token_expired") {
            // 本机时钟没看出来、服务端说到点了：还是「到点了」，不是登录失效。
            return Err(gpt_token_expired(&label, None));
        }
        // 访问令牌没到点，服务端却不认 —— 这才是登录出了事（被撤销、在别处登出、刷新令牌被轮换掉）。
        let reason = gpt_rejected(code.as_deref());
        login_health::record(
            &login_health::codex_key(slot_id),
            &home.join("auth.json"),
            &reason,
        );
        return Err(GateError::Other(format!("GPT 槽位「{label}」{reason}")));
    }
    if !status.is_success() {
        return Err(status_error(status, &body, "GPT"));
    }
    parse_gpt(&body, Some(label))
}

/// GPT 的访问令牌到点了。**面板不替它换**（见模块头），只说怎么让官方客户端自己换。
fn gpt_token_expired(label: &str, exp: Option<i64>) -> GateError {
    let when = display_time(exp)
        .map(|t| format!(" {t}"))
        .unwrap_or_default();
    GateError::Other(format!(
        "GPT 槽位「{label}」的访问令牌{when} 已经到点了（登录本身没问题）。GPT 的令牌只由官方 Codex 自己换新 —— \
         面板替它换会让桌面端手里那份作废、被登出。把这个槽位的 Codex 桌面端打开一下，它会自己换新，再点刷新。"
    ))
}

/// 服务端不认这份登录了：给账户行和报错用的那句话（纯文本，能照着做）。
fn gpt_rejected(code: Option<&str>) -> String {
    format!(
        "登录已失效：服务端拒绝了这份登录（{}）· 点「登录」打开这个槽位的桌面端，刷新令牌还有效它会自己接上，失效了它会要你重新登录",
        code.unwrap_or("HTTP 401")
    )
}

/// OpenAI 的错误回复里那个短代码（`token_expired` / `token_invalidated` / …）。三种形状都见过。
fn error_code(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let code = [
        v.pointer("/error/code"),
        v.pointer("/detail/code"),
        v.pointer("/code"),
        v.get("error").filter(|e| e.is_string()),
    ]
    .into_iter()
    .flatten()
    .find_map(Value::as_str)
    .map(|s| s.chars().take(40).collect());
    code
}

fn find_string(v: &Value, keys: &[&str]) -> Option<String> {
    match v {
        Value::Object(map) => {
            for key in keys {
                if let Some(value) = map
                    .get(*key)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    return Some(value.to_owned());
                }
            }
            map.values().find_map(|value| find_string(value, keys))
        }
        Value::Array(items) => items.iter().find_map(|value| find_string(value, keys)),
        _ => None,
    }
}

fn project_of(v: &Value) -> Option<String> {
    if let Some(project) = v.get("cloudaicompanionProject") {
        if let Some(id) = project.as_str() {
            return Some(id.to_owned());
        }
        if let Some(id) = project.get("id").and_then(Value::as_str) {
            return Some(id.to_owned());
        }
    }
    find_string(v, &["project"])
}

fn collect_gemini_buckets(value: &Value, out: &mut Vec<Value>) {
    match value {
        Value::Object(map) => {
            if let Some(buckets) = map.get("buckets").and_then(Value::as_array) {
                out.extend(buckets.iter().cloned());
            }
            for child in map.values() {
                collect_gemini_buckets(child, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_gemini_buckets(item, out);
            }
        }
        _ => {}
    }
}

fn parse_gemini_models(v: &Value) -> Vec<TavernGeminiModelQuota> {
    let mut buckets = Vec::new();
    collect_gemini_buckets(v, &mut buckets);
    let mut out = Vec::new();
    for bucket in buckets {
        let remaining = bucket
            .get("remainingFraction")
            .or_else(|| bucket.get("remaining_fraction"))
            .and_then(number_value)
            .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
            .map(|n| (n * 100.0).round() as u8);
        let reset_epoch = bucket
            .get("resetTime")
            .or_else(|| bucket.get("reset_time"))
            .and_then(epoch_of);
        if remaining.is_none() && reset_epoch.is_none() {
            continue;
        }
        // 有重置时刻、没有比例：proto3 把 0 省掉了，按 0 算并标出来（跟反重力那条同一个规矩）。
        let remaining_implied = remaining.is_none();
        let remaining = remaining.or(Some(0));
        let model_id = bucket
            .get("modelId")
            .or_else(|| bucket.get("model_id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let label = model_id.clone().unwrap_or_else(|| {
            bucket
                .get("displayName")
                .or_else(|| bucket.get("display_name"))
                .or_else(|| bucket.get("window"))
                .or_else(|| bucket.get("bucketId"))
                .and_then(Value::as_str)
                .unwrap_or("Gemini 模型")
                .to_string()
        });
        out.push(TavernGeminiModelQuota {
            model_id,
            label,
            token_type: bucket
                .get("tokenType")
                .or_else(|| bucket.get("token_type"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            remaining_percent: remaining,
            reset_at: display_time(reset_epoch),
            reset_epoch,
            remaining_implied,
        });
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    // 去重键要带上标签：汇总接口（没有 project 时走它）的桶没有 `modelId` / `tokenType`，
    // 原来按那两样去重，四个桶全是 None/None，只剩排在最前的一个 —— 可能还是 Claude/GPT 那一组的，
    // 被当成 Gemini CLI 的额度显示（2026-09-25）。
    out.dedup_by(|a, b| {
        a.model_id == b.model_id && a.token_type == b.token_type && a.label == b.label
    });
    out
}

pub fn parse_gemini(
    load_body: &str,
    quota_body: &str,
    account: Option<String>,
) -> Result<TavernGeminiQuota> {
    let load: Value = serde_json::from_str(load_body)
        .map_err(|e| GateError::Other(format!("Gemini loadCodeAssist 响应不是 JSON：{e}")))?;
    let quota: Value = serde_json::from_str(quota_body)
        .map_err(|e| GateError::Other(format!("Gemini 额度响应不是 JSON：{e}")))?;
    let models = parse_gemini_models(&quota);
    if models.is_empty() {
        return Err(GateError::Other(
            "Gemini 额度响应里没有可识别的模型桶（账号可能尚未开通 Code Assist）".into(),
        ));
    }
    let source = if quota.get("groups").is_some() {
        format!("{GEMINI_ENDPOINT}retrieveUserQuotaSummary")
    } else {
        format!("{GEMINI_ENDPOINT}retrieveUserQuota")
    };
    Ok(TavernGeminiQuota {
        provider: "gemini".into(),
        account,
        project: project_of(&load),
        tier: find_string(&load, &["tierName", "tier", "userTier"]),
        models,
        fetched_at: local_now(),
        source,
    })
}

/// 激活账户 CLI 那一半的 Gemini 额度。`refresh` 的意思同 [`gpt_quota`]。
pub async fn gemini_quota(refresh: bool) -> Result<Option<TavernGeminiQuota>> {
    let root = qb_accounts::antigravity::account::root();
    let Some(id) = qb_accounts::antigravity::account::active_id(&root)? else {
        return if refresh {
            Err(GateError::Other("没有激活的 Gemini CLI 槽位".into()))
        } else {
            Ok(None)
        };
    };
    if !refresh {
        return Ok(LAST_GEMINI.get(&id));
    }
    let q = fetch_gemini().await?;
    LAST_GEMINI.put(&id, q.clone());
    crate::audit::write(&format!(
        "问了一次 Gemini CLI「{}」的额度（只读，不记内容）",
        q.account.as_deref().unwrap_or(&id)
    ));
    Ok(Some(q))
}

async fn fetch_gemini() -> Result<TavernGeminiQuota> {
    let root = qb_accounts::antigravity::account::root();
    let Some(id) = qb_accounts::antigravity::account::active_id(&root)? else {
        return Err(GateError::Other("没有激活的 Gemini CLI 槽位".into()));
    };
    let Some((label, home, logged_in)) = qb_accounts::antigravity::account::active_cli(&root)?
    else {
        return Err(GateError::Other("没有激活的 Gemini CLI 槽位".into()));
    };
    if !logged_in {
        return Err(GateError::Other(format!(
            "Gemini 槽位「{label}」还没有登录"
        )));
    }
    let client = client()?;
    let load_payload = json!({"metadata":{"ideType":"GEMINI_CLI","pluginType":"GEMINI"}});
    let mut token = gemini_access(&client, &id, &home, false).await?;
    let (mut load_status, mut load_body) =
        gemini_post(&client, "loadCodeAssist", &token, &load_payload).await?;
    if load_status == StatusCode::UNAUTHORIZED {
        // 本机时钟说还没到点、Google 却不认：换一张新的再问一次（反重力那条同一个套路）。
        token = gemini_access(&client, &id, &home, true).await?;
        (load_status, load_body) =
            gemini_post(&client, "loadCodeAssist", &token, &load_payload).await?;
    }
    if !load_status.is_success() {
        return Err(status_error(load_status, &load_body, "Gemini"));
    }
    let load_json: Value = serde_json::from_str(&load_body)
        .map_err(|e| GateError::Other(format!("Gemini loadCodeAssist 响应不是 JSON：{e}")))?;
    // 个人订阅可能没有 GCP project。官方 IDE 会在这种情况下用空请求体
    // 查询个人额度；不能把缺少 project 当成“读不出来”。
    let project = project_of(&load_json).filter(|id| id != "aicode-consumers");
    let (method, payload) = match project.as_deref() {
        Some(project) => ("retrieveUserQuota", json!({"project": project})),
        None => ("retrieveUserQuotaSummary", json!({})),
    };
    let (quota_status, quota_body) = gemini_post(&client, method, &token, &payload).await?;
    if !quota_status.is_success() {
        return Err(status_error(quota_status, &quota_body, "Gemini"));
    }
    parse_gemini(&load_body, &quota_body, Some(label))
}

/// 向 Code Assist 发一个只读调用：`(状态, 回复原文)`。
async fn gemini_post(
    client: &reqwest::Client,
    method: &str,
    token: &Secret,
    payload: &Value,
) -> Result<(StatusCode, String)> {
    let resp = client
        .post(format!("{GEMINI_ENDPOINT}{method}"))
        .bearer_auth(token.as_str().unwrap_or_default())
        .json(payload)
        .send()
        .await
        .map_err(|e| GateError::Other(format!("Gemini {method} 连接失败：{e}")))?;
    let status = resp.status();
    Ok((status, resp.text().await.unwrap_or_default()))
}

/// Gemini CLI 这个账户现在能用的访问令牌。`force` = 不管本机时钟怎么说，都换一张新的。
///
/// 读的是 CLI 自己存在本机的那一份（[`qb_accounts::gemini::cli_token`]）；过期了拿同一份里的
/// 刷新令牌在内存里换新，客户端标识从本机装的 Gemini CLI 包里现读
/// （`detect::gemini_cli_oauth_client_sources`）—— **不写回凭据文件**，见模块头。
async fn gemini_access(
    client: &reqwest::Client,
    account_id: &str,
    home: &Path,
    force: bool,
) -> Result<Secret> {
    let stored = qb_accounts::gemini::cli_token(home)?.ok_or_else(|| {
        GateError::Other(format!(
            "这个账户的 Gemini CLI 还没登录（凭据文件里没有令牌）—— {GEMINI_RELOGIN}"
        ))
    })?;
    let print = google_oauth::fingerprint(stored.refresh.as_ref());
    let t = google_oauth::now();
    if !force {
        if let Some(access) = GEMINI_TOKENS.get(account_id, &print, t) {
            return Ok(access);
        }
        if stored.access_usable(t, google_oauth::TOKEN_MARGIN_SECS) {
            if let Some(a) = stored.access.as_ref() {
                return Ok(Secret::new(a.as_bytes().to_vec()));
            }
        }
    }
    let refresh = stored.refresh.as_ref().ok_or_else(|| {
        GateError::Other(format!(
            "Gemini CLI 的访问令牌到点了，而凭据里没有刷新令牌 —— {GEMINI_RELOGIN}"
        ))
    })?;
    let clients = LocalClients {
        files: qb_install::install::detect::gemini_cli_oauth_client_sources(),
        working: &GEMINI_CLIENT,
        product: "Gemini CLI",
    };
    match google_oauth::refresh(client, refresh, clients).await {
        Refreshed::Fresh(access, expires) => {
            GEMINI_TOKENS.put(account_id, &print, &access, expires);
            Ok(access)
        }
        Refreshed::Revoked => {
            // 这才是真的「登录失效」：账户行上 CLI 那一半改说失效、露出「登录」，
            // 直到 CLI 重新登录、把凭据文件写成新的。
            login_health::record(
                &login_health::gemini_cli_key(account_id),
                &qb_accounts::gemini::creds_path(home),
                "登录已失效：Google 说刷新令牌作废了（invalid_grant）· 点 CLI 旁边的「登录」重新登录",
            );
            Err(google_oauth::refresh_error(
                Refreshed::Revoked,
                "Gemini CLI",
                GEMINI_RELOGIN,
                GEMINI_SELF_REFRESH,
            ))
        }
        other => Err(google_oauth::refresh_error(
            other,
            "Gemini CLI",
            GEMINI_RELOGIN,
            GEMINI_SELF_REFRESH,
        )),
    }
}

fn provider_name(provider: TavernBackend) -> &'static str {
    match provider {
        TavernBackend::Claude => "claude",
        TavernBackend::Gpt => "gpt",
        TavernBackend::Gemini => "gemini",
    }
}

/// 用真实的本机桥接跑一轮最小角色扮演剧本，验证酒馆实际会走的链路。
pub async fn roleplay_test(
    provider: TavernBackend,
    fixture: Option<String>,
) -> Result<TavernRoleplayTest> {
    if matches!(provider, TavernBackend::Claude) {
        return Err(GateError::Other("Claude 按要求不执行角色扮演测试".into()));
    }
    let (port, model, token) = match provider {
        TavernBackend::Gpt => {
            let status = crate::gpt_bridge::status();
            if !status.running {
                return Err(GateError::Other("GPT 桥接尚未运行".into()));
            }
            (status.port, status.model, crate::gpt_bridge::token()?)
        }
        TavernBackend::Gemini => {
            let status = crate::gemini_bridge::status();
            if !status.running {
                return Err(GateError::Other("Gemini 桥接尚未运行".into()));
            }
            (status.port, status.model, crate::gemini_bridge::token()?)
        }
        TavernBackend::Claude => unreachable!(),
    };
    let script = fixture.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| {
        "你叫林澈，是一名温和但机敏的图书管理员。用户是第一次来访的旅人。请用两句中文回应，并记住用户的身份。".into()
    });
    let started = std::time::Instant::now();
    let response = local_bridge_client()?
        .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .bearer_auth(token)
        .json(&json!({
            "model": model,
            "messages": [
                {"role":"system","content": script},
                {"role":"user","content": "我想找一本关于星海的书。请保持角色继续回应。"}
            ],
            "temperature": 0.7,
            "stream": false
        }))
        .send()
        .await
        .map_err(|e| GateError::Other(format!("角色扮演测试连接桥接失败：{e}")))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let latency_ms = started.elapsed().as_millis() as i64;
    if !status.is_success() {
        return Ok(TavernRoleplayTest {
            provider: provider_name(provider).into(),
            ok: false,
            model: Some(model),
            reply: None,
            latency_ms: Some(latency_ms),
            input_tokens: None,
            output_tokens: None,
            detail: format!("桥接返回 HTTP {status}"),
            tested_at: local_now(),
        });
    }
    let value: Value = serde_json::from_str(&body)
        .map_err(|e| GateError::Other(format!("角色扮演测试响应不是 JSON：{e}")))?;
    let reply = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let usage = value.get("usage");
    let ok = reply.as_deref().is_some_and(|text| !text.trim().is_empty());
    Ok(TavernRoleplayTest {
        provider: provider_name(provider).into(),
        ok,
        model: Some(model),
        reply,
        latency_ms: Some(latency_ms),
        input_tokens: usage
            .and_then(|v| v.get("prompt_tokens"))
            .and_then(Value::as_i64),
        output_tokens: usage
            .and_then(|v| v.get("completion_tokens"))
            .and_then(Value::as_i64),
        detail: if ok {
            "桥接返回了非空角色回复".into()
        } else {
            "桥接返回为空回复".into()
        },
        tested_at: local_now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpt_windows_are_normalized_to_remaining_percentage() {
        let body = r#"{"plan_type":"plus","rate_limit":{"primary_window":{"used_percent":25,"limit_window_seconds":18000,"reset_at":1790247800},"secondary_window":{"used_percent":80,"limit_window_seconds":604800}}}"#;
        let q = parse_gpt(body, Some("a@example.com".into())).unwrap();
        assert_eq!(q.windows[0].remaining_percent, Some(75));
        assert_eq!(q.windows[0].label, "5 小时");
        assert_eq!(q.windows[1].remaining_percent, Some(20));
    }

    #[test]
    fn gpt_plural_windows_accept_percent_left_and_reset_time_ms() {
        let body = r#"{"plan_type":"pro","rate_limits":{"primary":{"percent_left":82,"limit_window_seconds":18000,"reset_time_ms":1790247800000},"secondary":{"usedPercent":75,"limitWindowSeconds":604800}}}"#;
        let q = parse_gpt(body, None).unwrap();
        assert_eq!(q.windows.len(), 2);
        assert_eq!(q.windows[0].remaining_percent, Some(82));
        assert_eq!(q.windows[0].label, "5 小时");
        assert_eq!(q.windows[0].reset_epoch, Some(1790247800));
        assert_eq!(q.windows[1].remaining_percent, Some(25));
        assert_eq!(q.windows[1].label, "7 天");
    }

    #[test]
    fn gpt_lone_weekly_window_keeps_its_real_duration() {
        let body = r#"{"rate_limits":{"primary_window":{"percent_left":48,"limit_window_seconds":604800}}}"#;
        let q = parse_gpt(body, None).unwrap();
        assert_eq!(q.windows.len(), 1);
        assert_eq!(q.windows[0].label, "7 天");
        assert_eq!(q.windows[0].remaining_percent, Some(48));
    }

    #[test]
    fn gpt_duration_without_a_percentage_is_present_but_unknown() {
        let body = r#"{"rate_limit":{"primary_window":{"limit_window_seconds":18000}}}"#;
        let q = parse_gpt(body, None).unwrap();
        assert!(q.windows[0].present);
        assert_eq!(q.windows[0].remaining_percent, None);
    }

    /// 数组里的窗口没带时长：不按位置猜「5 小时 / 7 天」（只剩一个每周窗口时它排第一）。
    #[test]
    fn windows_without_a_duration_are_not_guessed_from_their_position() {
        let body =
            r#"{"rate_limits":[{"used_percent":40},{"used_percent":10},{"used_percent":5}]}"#;
        let q = parse_gpt(body, None).unwrap();
        let labels: Vec<&str> = q.windows.iter().map(|w| w.label.as_str()).collect();
        assert!(labels.iter().all(|l| l.contains("时长未知")), "{labels:?}");
        let mut distinct = labels.clone();
        distinct.dedup();
        assert_eq!(distinct.len(), labels.len(), "界面拿名字当 key，不许同名");
    }

    #[test]
    fn gemini_buckets_keep_model_identity_and_reset_time() {
        let load = r#"{"cloudaicompanionProject":{"id":"demo-project"},"currentTier":{"tierName":"Google AI Pro"}}"#;
        let quota = r#"{"buckets":[{"modelId":"gemini-2.5-pro","tokenType":"REQUESTS","remainingFraction":0.62,"resetTime":"2026-09-24T11:03:25Z"}]}"#;
        let q = parse_gemini(load, quota, None).unwrap();
        assert_eq!(q.models[0].remaining_percent, Some(62));
        assert_eq!(q.project.as_deref(), Some("demo-project"));
    }

    #[test]
    fn gemini_summary_groups_accept_nested_string_fraction() {
        let load = r#"{"currentTier":{"tierName":"Google AI Pro"}}"#;
        let quota = r#"{"groups":[{"buckets":[{"modelId":"gemini-2.5-flash","remainingFraction":"0.41","resetTime":"2026-09-24T11:03:25Z"}]}]}"#;
        let q = parse_gemini(load, quota, None).unwrap();
        assert_eq!(q.models[0].model_id.as_deref(), Some("gemini-2.5-flash"));
        assert_eq!(q.models[0].remaining_percent, Some(41));
    }

    #[test]
    fn missing_quota_is_an_error_not_zero() {
        assert!(parse_gemini("{}", "{\"buckets\":[]}", None).is_err());
    }

    /// 2026-09-25：没有 project 时走汇总，桶只有 `bucketId` / `window`、没有 `modelId`。
    /// 原来按 `modelId + tokenType` 去重，四个桶全是 None/None，只剩一个。
    #[test]
    fn summary_buckets_without_model_ids_are_not_collapsed_into_one() {
        let load = r#"{"currentTier":{"tierName":"Google AI Pro"}}"#;
        let quota = r#"{"buckets":[
            {"bucketId":"gemini-5h","window":"gemini-5h","remainingFraction":0.8,"resetTime":"2026-09-24T11:03:25Z"},
            {"bucketId":"gemini-weekly","window":"gemini-weekly","remainingFraction":0.3,"resetTime":"2026-09-28T11:03:25Z"},
            {"bucketId":"3p-5h","window":"3p-5h","resetTime":"2026-09-24T12:00:00Z"}
        ]}"#;
        let q = parse_gemini(load, quota, None).unwrap();
        assert_eq!(q.models.len(), 3, "{:?}", q.models);
        // 只有重置时刻、没有比例的那一格：按 0 算，并且标出来是推出来的。
        let implied = q.models.iter().find(|m| m.label == "3p-5h").unwrap();
        assert_eq!(implied.remaining_percent, Some(0));
        assert!(implied.remaining_implied);
        let read = q.models.iter().find(|m| m.label == "gemini-5h").unwrap();
        assert_eq!(read.remaining_percent, Some(80));
        assert!(!read.remaining_implied);
    }

    #[test]
    fn last_results_are_kept_per_slot_and_can_be_forgotten() {
        let l: Last<u32> = Last::new();
        assert_eq!(l.get("a"), None);
        l.put("a", 1);
        l.put("b", 2);
        l.put("a", 3);
        assert_eq!(l.get("a"), Some(3), "只存最近一次");
        l.forget("a");
        assert_eq!(l.get("a"), None);
        assert_eq!(l.get("b"), Some(2), "忘掉一个不碰别的");
    }

    /// OpenAI 的错误回复见过三种形状，代码都要认得出来；认不出来就是 `None`，不猜。
    #[test]
    fn openai_error_codes_are_read_from_all_three_shapes() {
        assert_eq!(
            error_code(r#"{"error":{"message":"x","code":"token_expired"}}"#).as_deref(),
            Some("token_expired")
        );
        assert_eq!(
            error_code(r#"{"detail":{"code":"token_invalidated"}}"#).as_deref(),
            Some("token_invalidated")
        );
        assert_eq!(
            error_code(r#"{"error":"invalid_token"}"#).as_deref(),
            Some("invalid_token")
        );
        assert_eq!(error_code("<html>"), None);
    }

    /// 「到点了」不许说成「重新登录」；「服务端不认了」一定以「登录已失效」开头 ——
    /// 界面按这个前缀把它跟「从没登过」分开标红（GptBand / AntigravitySlotRow）。
    #[test]
    fn an_expired_token_is_not_a_lost_login_and_a_rejection_says_so() {
        let expired = gpt_token_expired("个人账户", Some(1_790_000_000)).to_string();
        assert!(expired.contains("到点"), "{expired}");
        assert!(!expired.contains("重新登录"), "{expired}");
        assert!(expired.contains("登录本身没问题"), "{expired}");
        let rejected = gpt_rejected(Some("token_invalidated"));
        assert!(rejected.starts_with("登录已失效"), "{rejected}");
        assert!(rejected.contains("token_invalidated"));
        assert!(gpt_rejected(None).contains("HTTP 401"));
        assert!(!rejected.contains("**"), "纯文本渲染，不许夹 Markdown");
    }

    #[tokio::test]
    async fn without_refresh_a_named_slot_is_answered_from_memory_only() {
        // `refresh = false` 走的是这一条：没问过就是 None，**不是去问一次**。
        // 指名的槽位不读任何文件，也不联网 —— 单测不许碰真实的运行期状态。
        let got = gpt_quota(Some("never-asked-7c1d"), false)
            .await
            .expect("只读内存，不会失败");
        assert!(got.is_none());
    }
}
