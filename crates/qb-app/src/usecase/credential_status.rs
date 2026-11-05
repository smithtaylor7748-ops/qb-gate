//! 每个账户槽位一个统一的凭证状态（账户行上的常驻徽标 + 悬停原因 + 一个动作）。
//!
//! **只读本机、不联网**：凭据文件在不在、读不读得出、访问令牌的到点时刻（JWT `exp` /
//! `expiresAt`，只看公开字段），再叠上联网额度那条路问出来、记在 [`super::login_health`] 里的那一笔。
//! 分层照 Cockpit Tools 的行为（只学行为，一行代码没抄）：要重新登录 / 只是访问令牌被拒 /
//! 瞬时问题（不改状态、不清旧额度）分开；反重力另有停用 / 要网页验证 / 要订阅 / 无权限。
//!
//! ⛔ 令牌内容不进这里的任何返回值。
use super::login_health;
use crate::error::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct CredentialStatus {
    /// ok / access_expired / access_rejected / relogin / transient / logged_out / unreadable /
    /// disabled / needs_web_verify / needs_subscription / forbidden
    pub kind: String,
    /// 徽标上的两三个字。
    pub label: String,
    /// 悬停时的原因（人话）。
    pub reason: String,
    /// 行上那一个按钮：official_refresh（让官方换新）/ open_desktop / switch_open / relogin /
    /// web_verify（打开 `url`）。`None` = 没有可做的。
    pub action: Option<String>,
    pub url: Option<String>,
    /// 重新登录后邮箱跟槽位原来的对不上：两个邮箱都写出来。
    pub email_mismatch: Option<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct SlotCredential {
    pub id: String,
    pub status: CredentialStatus,
}

fn status(kind: &str, reason: impl Into<String>, action: Option<&str>) -> CredentialStatus {
    let label = match kind {
        "ok" => "正常",
        "access_expired" => "令牌到点",
        "access_rejected" => "令牌被拒",
        "relogin" => "要重新登录",
        "transient" => "暂时问不到",
        "logged_out" => "没登录",
        "unreadable" => "读不出来",
        "disabled" => "已停用",
        "needs_web_verify" => "要网页验证",
        "needs_subscription" => "要订阅",
        "forbidden" => "无权限",
        _ => "未知",
    };
    CredentialStatus {
        kind: kind.into(),
        label: label.into(),
        reason: reason.into(),
        action: action.map(str::to_owned),
        url: None,
        email_mismatch: None,
    }
}

/// 批量 / 自动刷新要不要跳过这一条（手动刷新不跳）。
pub fn skip_in_batch(s: &CredentialStatus) -> bool {
    matches!(
        s.kind.as_str(),
        "disabled" | "forbidden" | "logged_out" | "relogin"
    )
}

// ------------------------------------------------------------------ 邮箱核对

/// 每个槽位第一次看到的邮箱（内存里）。之后看到的不一样 → 提醒「两个邮箱」。
static FIRST_EMAIL: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

pub fn email_check(key: &str, email: Option<&str>) -> Option<String> {
    let email = email?.trim().to_lowercase();
    if email.is_empty() {
        return None;
    }
    let mut g = FIRST_EMAIL.lock().ok()?;
    let m = g.get_or_insert_with(HashMap::new);
    let first = m.entry(key.to_string()).or_insert_with(|| email.clone());
    (*first != email).then(|| {
        format!("这个槽位原来登的是 {first}，现在登的是 {email} —— 确认是不是登错了账户")
    })
}

/// 使用者确认过、或者槽位删了：以现在的邮箱为准。
pub fn email_accept(key: &str) {
    if let Ok(mut g) = FIRST_EMAIL.lock() {
        if let Some(m) = g.as_mut() {
            m.remove(key);
        }
    }
}

// ------------------------------------------------------------------ 叠上 login_health

fn overlay(key: &str, file: &Path, base: CredentialStatus, refresh_action: &str) -> CredentialStatus {
    let Some((kind, reason)) = login_health::check_kind(key, file) else {
        return base;
    };
    let action = match kind.as_str() {
        "access_rejected" => Some(refresh_action),
        "needs_web_verify" => Some("web_verify"),
        "relogin" | "disabled" => Some("relogin"),
        _ => None,
    };
    let mut s = status(&kind, reason, action);
    if kind == "needs_web_verify" {
        s.url = login_health::link(key);
    }
    s
}

// ------------------------------------------------------------------ GPT

/// 一份 `auth.json` → 状态。纯函数，可测。
pub fn gpt_from_file(bytes: Option<&[u8]>, now_secs: i64) -> CredentialStatus {
    let Some(bytes) = bytes else {
        return status("logged_out", "这个槽位还没登录过", Some("open_desktop"));
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return status("unreadable", "auth.json 读不出来（不是 JSON）", None);
    };
    if v["auth_mode"].as_str() == Some("apikey") {
        return status("logged_out", "API Key 模式，没有账户登录", Some("open_desktop"));
    }
    let access = v.pointer("/tokens/access_token").and_then(|t| t.as_str());
    let refresh = v.pointer("/tokens/refresh_token").and_then(|t| t.as_str());
    let (Some(access), Some(_)) = (
        access.filter(|t| !t.is_empty()),
        refresh.filter(|t| !t.is_empty()),
    ) else {
        return status("logged_out", "auth.json 里没有完整的登录", Some("open_desktop"));
    };
    match qb_accounts::codex::access_token_expiry(access) {
        Some(exp) if exp <= now_secs + 60 => status(
            "access_expired",
            "访问令牌到点了（刷新令牌可能还有效）· 点「让官方换新」由官方 Codex CLI 自己换",
            Some("official_refresh"),
        ),
        _ => status("ok", "本机凭据完整，访问令牌没到点", None),
    }
}

pub fn gpt() -> Result<Vec<SlotCredential>> {
    let root = qb_accounts::codex::root();
    let list = qb_accounts::codex::list(&root)?;
    let now = chrono::Utc::now().timestamp();
    Ok(list
        .slots
        .into_iter()
        .map(|s| {
            let auth = root.join(&s.id).join("home").join("auth.json");
            let bytes = match std::fs::read(&auth) {
                Ok(b) => Some(b),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => {
                    return SlotCredential {
                        id: s.id,
                        status: status("unreadable", "auth.json 打不开", None),
                    }
                }
            };
            let key = login_health::codex_key(&s.id);
            let mut st = overlay(&key, &auth, gpt_from_file(bytes.as_deref(), now), "official_refresh");
            st.email_mismatch = email_check(&key, s.email.as_deref());
            SlotCredential { id: s.id, status: st }
        })
        .collect())
}

// ------------------------------------------------------------------ Claude

/// `.credentials.json` → 状态（只看 `expiresAt`，不碰令牌）。
///
/// 实测（2026-11-05，闲置槽位）：`claude auth status --json` 前后凭据文件的修改时刻和长度都没变 ——
/// 它不会替过期的访问令牌换新。所以到点了只给「切换并打开」：Claude Code 跑起来会自己换。
pub fn claude_from_file(bytes: Option<&[u8]>, now_ms: i64) -> CredentialStatus {
    let Some(bytes) = bytes else {
        return status("logged_out", "这个槽位还没登录过", Some("switch_open"));
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return status("unreadable", ".credentials.json 读不出来", None);
    };
    let Some(o) = v.get("claudeAiOauth").or_else(|| v.get("claude.ai_oauth")) else {
        return status("logged_out", "没有 Claude 账户登录", Some("switch_open"));
    };
    let exp = o.get("expiresAt").and_then(|e| e.as_i64()).map(|t| {
        if t > 1_000_000_000_000 {
            t
        } else {
            t * 1000
        }
    });
    match exp {
        Some(e) if e <= now_ms + 60_000 => status(
            "access_expired",
            "访问令牌到点了 · 面板不替 Claude 换新：切换到这个账户并打开 Claude Code，它会自己换",
            Some("switch_open"),
        ),
        _ => status("ok", "本机凭据完整，访问令牌没到点", None),
    }
}

pub fn claude() -> Vec<SlotCredential> {
    let report = qb_accounts::accounts::report(&qb_accounts::accounts::AccountRoots::current());
    let now = chrono::Utc::now().timestamp_millis();
    report
        .slots
        .into_iter()
        .map(|s| {
            let cred = Path::new(&s.dir).join(".credentials.json");
            let bytes = std::fs::read(&cred).ok();
            let key = login_health::claude_key(&s.label);
            let mut st = overlay(&key, &cred, claude_from_file(bytes.as_deref(), now), "switch_open");
            st.email_mismatch = email_check(&key, s.email.as_deref());
            SlotCredential {
                id: s.label,
                status: st,
            }
        })
        .collect()
}

// ------------------------------------------------------------------ 反重力

pub fn antigravity() -> Result<Vec<SlotCredential>> {
    use qb_accounts::antigravity::{account, status as ag};
    let list = account::list(&account::root())?;
    Ok(list
        .slots
        .into_iter()
        .map(|a| {
            let key = login_health::antigravity_ide_key(&a.id);
            let base = if a.ide_dir.is_none() {
                status("logged_out", "还没有 IDE 那一半", Some("relogin"))
            } else if a.ide_unreadable {
                status("unreadable", a.ide_auth_state.clone(), None)
            } else if !a.ide_logged_in {
                status("logged_out", a.ide_auth_state.clone(), Some("relogin"))
            } else {
                status("ok", "IDE 那一半已登录", None)
            };
            let st = match a.ide_dir.as_deref() {
                Some(dir) => overlay(&key, &ag::state_db(Path::new(dir)), base, "relogin"),
                None => base,
            };
            let mut st = st;
            st.email_mismatch = email_check(&key, a.email.as_deref());
            SlotCredential { id: a.id, status: st }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpt_status_is_read_from_the_file_without_returning_tokens() {
        assert_eq!(gpt_from_file(None, 0).kind, "logged_out");
        assert_eq!(gpt_from_file(Some(b"nope"), 0).kind, "unreadable");
        // 载荷 {"exp":1790000000}
        let f = br#"{"tokens":{"access_token":"h.eyJleHAiOjE3OTAwMDAwMDB9.s","refresh_token":"r-secret"}}"#;
        let s = gpt_from_file(Some(f), 1_790_000_100);
        assert_eq!(s.kind, "access_expired");
        assert_eq!(s.action.as_deref(), Some("official_refresh"));
        assert_eq!(gpt_from_file(Some(f), 1_700_000_000).kind, "ok");
        assert!(!serde_json::to_string(&s).unwrap().contains("r-secret"));
    }

    #[test]
    fn claude_expired_only_offers_switch_and_open() {
        let f = br#"{"claudeAiOauth":{"accessToken":"t","expiresAt":1000}}"#;
        let s = claude_from_file(Some(f), 10_000_000);
        assert_eq!(s.kind, "access_expired");
        assert_eq!(s.action.as_deref(), Some("switch_open"));
    }

    #[test]
    fn a_different_email_after_relogin_names_both() {
        let k = "test:email-9f1";
        assert_eq!(email_check(k, Some("a@x.test")), None);
        let m = email_check(k, Some("b@x.test")).unwrap();
        assert!(m.contains("a@x.test") && m.contains("b@x.test"));
        email_accept(k);
        assert_eq!(email_check(k, Some("b@x.test")), None);
    }

    #[test]
    fn batch_skips_disabled_and_forbidden_only_in_batch() {
        assert!(skip_in_batch(&status("disabled", "", None)));
        assert!(skip_in_batch(&status("forbidden", "", None)));
        assert!(!skip_in_batch(&status("access_expired", "", None)));
    }
}
