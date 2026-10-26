// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! Claude 官方订阅额度（中转站重构阶段 6，cc-switch 的 U2 —— `services/subscription.rs` 的 Claude 那一段）：
//! 带槽位里 Claude Code 存的访问令牌问一次 `api.anthropic.com/api/oauth/usage`，拿 5 小时 / 7 天、按模型的周额度、超额用量。
//!
//! # 推翻了什么、没推翻什么
//!
//! 09-24 使用者选过「Claude 的 5 小时 / 7 天从不联网」；中转站重构的 35 项里他勾了 U2（官方订阅额度联网），
//! 这一条就此推翻。**节奏没变**（CLAUDE.md「联网额度」那一节，09-23 使用者选的设计）：
//!
//! | `refresh` | 做什么 |
//! |---|---|
//! | `false` | 只读「最近一次」问到的（[`LAST`]），**绝不联网**；没问过就是 `None` |
//! | `true` | 联网问一次这一个槽位，成功就记成「最近一次」 |
//!
//! 联网只从使用者点的那颗刷新图标发出，一次只问一个账户；「最近一次」没有 TTL，界面标明是几点问的。
//! 结果**只显示**，不驱动任何切换（`architecture.rs` 的 `quota_modules_never_drive_switching` 钉着）。
//!
//! # 令牌
//!
//! - 只读槽位里 Claude Code 自己存的 `.credentials.json`（`claudeAiOauth`），装进 [`Secret`]，只放进请求头；
//!   不进日志、审计、返回值。
//! - ⛔ **本机时钟看访问令牌到点了就不发**，也**不替 Claude Code 换新**（跟 GPT 同一条：刷新令牌每换一次就轮换，
//!   面板一换，Claude Code 手里那份就作废、下次被登出）。cc-switch 会拿过期令牌硬问一次，这里不照做 ——
//!   拿过期令牌问，401 是必然的，而那不是「登录失效」。
//! - 令牌没到点、服务端却回 401 / 403，才记进 [`super::login_health`]：账户行上改说「登录已失效」。
//!
//! # 窗口
//!
//! 照 cc-switch 的 `parse_claude_quota`：老的顶层窗口（`five_hour`、`seven_day`、`seven_day_opus`……，认不出名字的也留着），
//! 新版 `limits[]` 里 `weekly_scoped` / `weekly` 的按模型周额度（Fable / Opus / Sonnet）盖掉同名的老窗口；
//! 某个使用场景的子限额（`scope.surface` 不为空）不并进整个模型的周额度；单行坏了只丢那一行。

use super::login_health;
use super::tavern_quota::Last;
use crate::accounts::usage::{SlotUsage, UsageSource, UsageWindow};
use crate::accounts::{AccountRoots, AccountsReport};
use crate::error::{GateError, Result};
use qb_platform::credentials::Secret;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;
use ts_rs::TS;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// 访问令牌还剩不到这么多就当到点了（发出去也多半在路上过期）。
const EXPIRY_SLACK_MS: i64 = 60_000;

/// 一个额度窗口。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ClaudeQuotaWindow {
    /// 接口里的名字（`five_hour`、`seven_day`、`seven_day_opus`……）。
    pub key: String,
    /// 界面上叫什么（「5 小时」「7 天 · Opus」……；认不出的就是 `key`）。
    pub label: String,
    /// 已用百分比 0–100。
    pub used_percent: f64,
    /// 重置时刻（RFC3339）；接口没给就是 `None`。
    pub resets_at: Option<String>,
}

/// 超额用量（用完订阅额度之后按量付费那一块）。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ClaudeExtraUsage {
    pub enabled: bool,
    pub monthly_limit: Option<f64>,
    pub used_credits: Option<f64>,
    pub utilization: Option<f64>,
    pub currency: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ClaudeQuota {
    /// 槽位标签。
    pub label: String,
    pub windows: Vec<ClaudeQuotaWindow>,
    pub extra: Option<ClaudeExtraUsage>,
    /// 问的时刻（RFC3339）。
    pub fetched_at: String,
    pub source: String,
}

/// 「最近一次」问到的，按槽位标签存。**只存最近一次，没有 TTL。**
static LAST: Last<ClaudeQuota> = Last::new();

/// 槽位删了之后调：额度和「登录已失效」那一笔都别再躺在内存里。
pub fn forget(label: &str) {
    LAST.forget(label);
    login_health::forget(&login_health::claude_key(label));
}

/// 窗口在界面上叫什么。
fn label_of(key: &str) -> String {
    match key {
        "five_hour" => "5 小时".into(),
        "seven_day" => "7 天".into(),
        "seven_day_fable" => "7 天 · Fable".into(),
        "seven_day_opus" => "7 天 · Opus".into(),
        "seven_day_sonnet" => "7 天 · Sonnet".into(),
        other => other.to_string(),
    }
}

/// 认得的窗口，也是排列顺序（cc-switch 的 `KNOWN_TIERS`）。
const KNOWN: [&str; 5] = [
    "five_hour",
    "seven_day",
    "seven_day_fable",
    "seven_day_opus",
    "seven_day_sonnet",
];

/// `{"utilization": 12.0, "resets_at": "…"}` 这种老窗口；`utilization` 没有就不算一个窗口。
fn legacy_window(key: &str, v: &Value) -> Option<ClaudeQuotaWindow> {
    let used = v.get("utilization")?.as_f64()?;
    let resets_at = match v.get("resets_at") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return None,
    };
    Some(ClaudeQuotaWindow {
        key: key.to_string(),
        label: label_of(key),
        used_percent: used,
        resets_at,
    })
}

/// 解析接口的回复。认不出任何窗口就报错（不显示一个猜出来的数）。
pub fn parse(body: &Value, label: &str) -> Result<ClaudeQuota> {
    let mut windows: Vec<ClaudeQuotaWindow> = Vec::new();
    if let Some(obj) = body.as_object() {
        for key in KNOWN {
            if let Some(w) = obj.get(key).and_then(|v| legacy_window(key, v)) {
                windows.push(w);
            }
        }
        for (key, v) in obj {
            if key == "extra_usage" || key == "limits" || KNOWN.contains(&key.as_str()) {
                continue;
            }
            if let Some(w) = legacy_window(key, v) {
                windows.push(w);
            }
        }
    }
    // 新版按模型的周额度：盖掉同名的老窗口；同一个模型只认第一行；单行坏了只丢那一行。
    let mut scoped: Vec<&str> = Vec::new();
    if let Some(limits) = body.get("limits").and_then(Value::as_array) {
        for limit in limits {
            if limit.get("kind").and_then(Value::as_str) != Some("weekly_scoped")
                || limit.get("group").and_then(Value::as_str) != Some("weekly")
                || limit
                    .pointer("/scope/surface")
                    .is_some_and(|v| !v.is_null())
            {
                continue;
            }
            let Some(model) = limit
                .pointer("/scope/model/display_name")
                .and_then(Value::as_str)
            else {
                continue;
            };
            let key = match model.trim().to_ascii_lowercase().as_str() {
                "fable" => "seven_day_fable",
                "opus" => "seven_day_opus",
                "sonnet" => "seven_day_sonnet",
                _ => continue,
            };
            let Some(percent) = limit.get("percent").and_then(Value::as_f64) else {
                continue;
            };
            let resets_at = match limit.get("resets_at") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) => Some(s.clone()),
                Some(_) => continue,
            };
            if !percent.is_finite() || percent < 0.0 || scoped.contains(&key) {
                continue;
            }
            scoped.push(key);
            let w = ClaudeQuotaWindow {
                key: key.to_string(),
                label: label_of(key),
                used_percent: percent,
                resets_at,
            };
            match windows.iter_mut().find(|x| x.key == key) {
                Some(existing) => *existing = w,
                None => windows.push(w),
            }
        }
    }
    windows.sort_by_key(|w| {
        KNOWN
            .iter()
            .position(|k| *k == w.key)
            .unwrap_or(KNOWN.len())
    });
    if windows.is_empty() {
        return Err(GateError::Other(
            "Claude 额度接口的回复里没有认得出的窗口（接口可能改版了）".into(),
        ));
    }
    let extra = body
        .get("extra_usage")
        .filter(|v| v.is_object())
        .map(|e| ClaudeExtraUsage {
            enabled: e
                .get("is_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            monthly_limit: e.get("monthly_limit").and_then(Value::as_f64),
            used_credits: e.get("used_credits").and_then(Value::as_f64),
            utilization: e.get("utilization").and_then(Value::as_f64),
            currency: e
                .get("currency")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    Ok(ClaudeQuota {
        label: label.to_string(),
        windows,
        extra,
        fetched_at: chrono::Utc::now().to_rfc3339(),
        source: USAGE_URL.into(),
    })
}

/// 读槽位里 Claude Code 存的访问令牌与到期毫秒数。令牌装进 [`Secret`]：只许放进请求头。
fn credential(slot_dir: &Path) -> Option<(Secret, Option<i64>)> {
    let text = std::fs::read_to_string(slot_dir.join(".credentials.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let o = v
        .get("claudeAiOauth")
        .or_else(|| v.get("claude.ai_oauth"))?;
    let token = o.get("accessToken")?.as_str().filter(|t| !t.is_empty())?;
    let expires = o.get("expiresAt").and_then(|e| match e {
        // 秒和毫秒都见过（cc-switch 的 `is_token_expired`）。
        Value::Number(n) => n
            .as_i64()
            .map(|t| if t > 1_000_000_000_000 { t } else { t * 1000 }),
        Value::String(s) => chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.timestamp_millis()),
        _ => None,
    });
    Some((Secret::new(token.as_bytes().to_vec()), expires))
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15))
        .user_agent("QB-Gate claude-quota")
        .build()
        .map_err(|e| GateError::Other(format!("额度查询客户端创建失败：{e}")))
}

/// 一个 Claude 槽位的额度。`refresh = false` 只读「最近一次」，绝不联网；`true` 联网问一次（见模块头）。
pub async fn quota(label: &str, refresh: bool) -> Result<Option<ClaudeQuota>> {
    let label = label.trim();
    if !refresh {
        return Ok(LAST.get(label));
    }
    let dir = AccountRoots::current().slot_dir(label);
    let q = fetch_at(&dir, label, &client()?, USAGE_URL).await?;
    LAST.put(label, q.clone());
    crate::audit::write(&format!(
        "问了一次 Claude 槽位「{label}」的额度（只读，不记内容）"
    ));
    Ok(Some(q))
}

/// 访问令牌到点了：**面板不替它换**，只说怎么让 Claude Code 自己换。
fn token_expired(label: &str, exp_ms: i64) -> GateError {
    let when = chrono::DateTime::from_timestamp_millis(exp_ms)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format(" %m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default();
    GateError::Other(format!(
        "Claude 槽位「{label}」本机这份访问令牌{when} 已经到点了，没有发请求（登录本身没问题）。\
         Claude 的令牌只由 Claude Code 自己换新 —— 面板替它换会让它手里那份作废、被登出。\
         用这个账户跑一次 Claude Code，它会自己换新，再点刷新。"
    ))
}

/// 服务端不认这份登录了：账户行和报错用的那句话。前缀「登录已失效」界面拿去分辨，别改措辞。
fn rejected(status: u16) -> String {
    format!(
        "登录已失效：服务端拒绝了这份登录（HTTP {status}）· 用这个账户跑一次 Claude Code，刷新令牌还有效它会自己接上，失效了它会要你重新登录"
    )
}

async fn fetch_at(
    slot_dir: &Path,
    label: &str,
    http: &reqwest::Client,
    url: &str,
) -> Result<ClaudeQuota> {
    let Some((token, expires)) = credential(slot_dir) else {
        return Err(GateError::Other(format!(
            "Claude 槽位「{label}」还没登录过（读不到 .credentials.json 里的 OAuth 凭证）"
        )));
    };
    if let Some(exp) = expires {
        if exp <= chrono::Utc::now().timestamp_millis() + EXPIRY_SLACK_MS {
            return Err(token_expired(label, exp));
        }
    }
    let bearer = token
        .as_str()
        .ok_or_else(|| GateError::Other("访问令牌不是文本".into()))?;
    let response = http
        .get(url)
        .bearer_auth(bearer)
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| GateError::Other(format!("Claude 额度接口连接失败：{e}")))?;
    let status = response.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        let reason = rejected(status.as_u16());
        login_health::record(
            &login_health::claude_key(label),
            &slot_dir.join(".credentials.json"),
            &reason,
        );
        return Err(GateError::Other(format!("Claude 槽位「{label}」{reason}")));
    }
    let body = response
        .text()
        .await
        .map_err(|e| GateError::Other(format!("Claude 额度接口的回复没读完：{e}")))?;
    if !status.is_success() {
        let short: String = body.chars().take(120).collect();
        return Err(GateError::Other(format!(
            "Claude 额度接口回了 HTTP {}：{short}",
            status.as_u16()
        )));
    }
    let v: Value = serde_json::from_str(&body)
        .map_err(|e| GateError::Other(format!("Claude 额度接口的回复不是 JSON：{e}")))?;
    parse(&v, label)
}

/// 把联网问到的「最近一次」窗口摆成槽位横条要的形状（来源标「官方接口」）。
fn as_slot_usage(q: &ClaudeQuota, now_ms: i64) -> Option<SlotUsage> {
    let measured = chrono::DateTime::parse_from_rfc3339(&q.fetched_at).ok()?;
    let window = |key: &str| {
        q.windows.iter().find(|w| w.key == key).map(|w| {
            let reset_ms = w
                .resets_at
                .as_deref()
                .and_then(|r| chrono::DateTime::parse_from_rfc3339(r).ok())
                .map(|d| d.timestamp_millis());
            UsageWindow {
                used: w.used_percent.round().clamp(0.0, 100.0) as u8,
                resets_at: w.resets_at.clone(),
                estimated: false,
                // 重置时刻夹在「问的时刻」与「现在」之间：这份读数是重置之前的。
                reset_passed: reset_ms
                    .is_some_and(|r| r > measured.timestamp_millis() && r <= now_ms),
            }
        })
    };
    Some(SlotUsage {
        source: UsageSource::Official,
        measured_at: q.fetched_at.clone(),
        age_minutes: ((now_ms - measured.timestamp_millis()) / 60_000).max(0),
        five_hour: window("five_hour"),
        seven_day: window("seven_day"),
    })
}

/// 列账户时叠上联网那条路的两样：「最近一次」的额度（比本机读数新才用）和「登录已失效」。
pub fn overlay(mut report: AccountsReport) -> AccountsReport {
    overlay_slots(&mut report.slots);
    report
}

fn overlay_slots(slots: &mut [crate::accounts::Slot]) {
    let now = chrono::Utc::now().timestamp_millis();
    for s in slots {
        let cred = std::path::Path::new(&s.dir).join(".credentials.json");
        if let Some(reason) = login_health::check(&login_health::claude_key(&s.label), &cred) {
            s.logged_in = false;
            s.login_problem = Some(reason);
        }
        if let Some(official) = LAST.get(&s.label).and_then(|q| as_slot_usage(&q, now)) {
            let newer = match &s.usage {
                None => true,
                Some(local) => {
                    chrono::DateTime::parse_from_rfc3339(&local.measured_at)
                        .map(|t| t.timestamp_millis())
                        .unwrap_or(0)
                        <= chrono::DateTime::parse_from_rfc3339(&official.measured_at)
                            .map(|t| t.timestamp_millis())
                            .unwrap_or(0)
                }
            };
            if newer {
                s.usage = Some(official);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scoped(model: &str, percent: f64) -> Value {
        json!({
            "kind": "weekly_scoped", "group": "weekly", "percent": percent,
            "resets_at": "2026-09-12T00:00:00Z", "is_active": true,
            "scope": { "model": { "id": null, "display_name": model }, "surface": null }
        })
    }

    fn scratch() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("qb-claude-quota-{}", crate::config_io::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_cred(dir: &Path, token: &str, expires_at: i64) {
        std::fs::write(
            dir.join(".credentials.json"),
            json!({ "claudeAiOauth": { "accessToken": token, "expiresAt": expires_at } })
                .to_string(),
        )
        .unwrap();
    }

    #[test]
    fn legacy_windows_unknown_windows_and_extra_usage_are_kept_in_order() {
        let q = parse(
            &json!({
                "five_hour": { "utilization": 12.0, "resets_at": "2026-09-09T15:00:00Z" },
                "seven_day": { "utilization": 25.0, "resets_at": null },
                "seven_day_opus": { "utilization": 8.0 },
                "seven_day_sonnet": null,
                "other_window": { "utilization": 4.0 },
                "extra_usage": { "is_enabled": true, "monthly_limit": 100.0,
                    "used_credits": 9.0, "utilization": 9.0, "currency": "USD" }
            }),
            "main",
        )
        .unwrap();
        let shown: Vec<(&str, f64)> = q
            .windows
            .iter()
            .map(|w| (w.label.as_str(), w.used_percent))
            .collect();
        assert_eq!(
            shown,
            [
                ("5 小时", 12.0),
                ("7 天", 25.0),
                ("7 天 · Opus", 8.0),
                ("other_window", 4.0)
            ]
        );
        assert_eq!(
            q.windows[0].resets_at.as_deref(),
            Some("2026-09-09T15:00:00Z")
        );
        let extra = q.extra.unwrap();
        assert!(extra.enabled);
        assert_eq!(
            (extra.used_credits, extra.monthly_limit),
            (Some(9.0), Some(100.0))
        );
    }

    #[test]
    fn per_model_weekly_limits_override_the_old_windows_and_bad_rows_are_dropped() {
        let mut fable = scoped("  fAbLe  ", 0.0);
        fable["resets_at"] = Value::Null;
        let mut surface = scoped("Opus", 99.0);
        surface["scope"]["surface"] = json!("claude_code");
        let mut stringy = scoped("Sonnet", 1.0);
        stringy["percent"] = json!("37");
        let q = parse(
            &json!({
                "seven_day_fable": { "utilization": 80.0 },
                "seven_day_opus": { "utilization": 20.0 },
                "limits": [scoped("Sonnet", 5.0), fable, scoped("Fable", 90.0), surface, stringy, scoped("Opus", 6.0)]
            }),
            "main",
        )
        .unwrap();
        let shown: Vec<(&str, f64)> = q
            .windows
            .iter()
            .map(|w| (w.key.as_str(), w.used_percent))
            .collect();
        assert_eq!(
            shown,
            [
                ("seven_day_fable", 0.0),
                ("seven_day_opus", 6.0),
                ("seven_day_sonnet", 5.0)
            ],
            "同一个模型只认第一行；某个使用场景的子限额、坏了的行都不算"
        );
        // 一个窗口都认不出来：报错，不显示猜出来的数。
        assert!(parse(&json!({ "limits": [] }), "main").is_err());
    }

    /// ⛔ 到点了不发请求（发出去只会换来一个 401，而那不是登录失效），也不换新。
    #[tokio::test]
    async fn an_expired_token_is_not_sent_and_says_how_claude_code_renews_it() {
        let d = scratch();
        write_cred(
            &d,
            "sk-ant-oat-do-not-leak",
            chrono::Utc::now().timestamp_millis() - 3_600_000,
        );
        // 客户端指向连不上的代理：真发出去这条测试会超时失败。
        let dead = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all("http://127.0.0.1:1").unwrap())
            .timeout(Duration::from_millis(400))
            .build()
            .unwrap();
        let e = fetch_at(&d, "main", &dead, USAGE_URL)
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("没有发请求") && e.contains("自己换新"), "{e}");
        assert!(!e.contains("sk-ant-oat-do-not-leak"), "令牌漏进报错了");
        std::fs::remove_dir_all(d).unwrap();
    }

    /// 令牌没到点、服务端却拒绝：记一笔「登录已失效」，列账户时那一行改成没登录。
    #[tokio::test]
    async fn a_live_token_rejected_by_the_server_marks_the_slot_logged_out() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            let _ = sock
                .write_all(b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}")
                .await;
        });
        let d = scratch();
        let label = format!("t-{}", crate::config_io::id());
        write_cred(&d, "tok", chrono::Utc::now().timestamp_millis() + 3_600_000);
        let http = reqwest::Client::builder().no_proxy().build().unwrap();
        let e = fetch_at(&d, &label, &http, &format!("http://{addr}/api/oauth/usage"))
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("登录已失效"), "{e}");
        let mut slots = vec![crate::accounts::Slot {
            label: label.clone(),
            active: true,
            logged_in: true,
            cli_days_left: Some(30),
            account_uuid: None,
            email: None,
            org_name: None,
            expires_at: None,
            dir: d.display().to_string(),
            desktop_dir: None,
            plan: None,
            billing: None,
            plan_fetched_at: None,
            desktop_profile: false,
            usage: None,
            login_problem: None,
        }];
        overlay_slots(&mut slots);
        assert!(!slots[0].logged_in);
        assert!(slots[0]
            .login_problem
            .as_deref()
            .unwrap()
            .starts_with("登录已失效"));
        // Claude Code 重新登录、换了令牌（凭据文件一变）：那一笔自动作废。
        std::thread::sleep(Duration::from_millis(20));
        write_cred(
            &d,
            "fresh-token-after-relogin",
            chrono::Utc::now().timestamp_millis() + 7_200_000,
        );
        slots[0].logged_in = true;
        slots[0].login_problem = None;
        overlay_slots(&mut slots);
        assert!(slots[0].logged_in, "凭据文件变了就不该再说登录已失效");
        forget(&label);
        std::fs::remove_dir_all(d).unwrap();
    }

    /// 联网问到的那份比本机读数新才盖上去；窗口在问过之后重置了，标出来（不当当前值）。
    #[test]
    fn the_official_reading_wins_only_when_newer_and_a_passed_reset_is_flagged() {
        let now = chrono::Utc::now();
        let q = ClaudeQuota {
            label: "main".into(),
            windows: vec![
                ClaudeQuotaWindow {
                    key: "five_hour".into(),
                    label: "5 小时".into(),
                    used_percent: 91.6,
                    resets_at: Some((now - chrono::Duration::minutes(5)).to_rfc3339()),
                },
                ClaudeQuotaWindow {
                    key: "seven_day".into(),
                    label: "7 天".into(),
                    used_percent: 30.0,
                    resets_at: Some((now + chrono::Duration::days(2)).to_rfc3339()),
                },
            ],
            extra: None,
            fetched_at: (now - chrono::Duration::minutes(30)).to_rfc3339(),
            source: USAGE_URL.into(),
        };
        let u = as_slot_usage(&q, now.timestamp_millis()).unwrap();
        assert_eq!(u.source, UsageSource::Official);
        assert_eq!(u.age_minutes, 30);
        let fh = u.five_hour.unwrap();
        assert_eq!(fh.used, 92);
        assert!(fh.reset_passed, "问过之后五小时窗口已经重置了");
        assert!(!u.seven_day.unwrap().reset_passed);
    }
}
