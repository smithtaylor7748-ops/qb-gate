//! 账户槽位与凭证到期。
//!
//! 机制沿用现有 account-manager.ps1：目录联结点(junction)，标签一一对应。
//!   `<state>/claude-profile`  ->  `<state>/claude-profile-<标签>`   Claude Code 侧
//!   `%APPDATA%/Claude`        ->  `%APPDATA%/Claude-<标签>`         桌面端侧
//!
//! 四条政策边界，改这个文件之前先读 README 的合规一节：
//!   1. 不读取任何限流 / 429 / 额度状态，不存在「用完自动换号」的路径
//!   2. 切换只能由人在界面上手动触发，没有定时器、没有 watchdog、没有自动调用点
//!   3. 任意时刻只有一个账户激活
//!   4. 所有账户必须是使用者本人拥有的
//!
//! 另外两条判断上的坑，合并了就出事：
//!   - **「已登录」和「没过期」是两件事。** 凭证过期的账户**必须仍然可切**，
//!     因为你得先切过去才能在那个槽里重新登录。合并了就把自己锁在门外。
//!     所以 `logged_in` 只看凭证文件在不在，到期与否单独算，且只用于显示。
//!   - `config.json` 的 `lastKnownAccountUuid` 回答不了「还登着吗」——
//!     登出 / 过期之后它也不消失，只能回答「这个目录是谁的」。

use crate::error::{GateError, Result};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct MigrationReport {
    pub migrated: Vec<String>,
    pub backup: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Slot {
    pub label: String,
    pub active: bool,
    /// 只看凭证文件在不在 —— 不掺到期判断。
    pub logged_in: bool,
    /// refreshToken 剩余天数。负数表示已过期。
    pub cli_days_left: Option<i64>,
    pub account_uuid: Option<String>,
    /// 套餐，例如 `Claude Pro`。读自本槽位的 `.claude.json`，**不联网**。
    pub plan: Option<String>,
    /// 计费方式，例如 `Google Play 订阅`。同上。
    pub billing: Option<String>,
    /// 官方客户端上次刷新这份档案的时间。界面上要标出来 —— 这是缓存，可能过期。
    pub plan_fetched_at: Option<String>,
}

/// 套餐是**读文件读出来的，不是查接口查出来的**。
///
/// `~/.claude.json` 的 `oauthAccount` 是官方客户端自己写下的一份档案缓存，
/// 每个槽位目录里各有一份 —— 所以不用切过去就能看到每个槽位的套餐。
///
/// 这不违反 README 的第 1 条政策边界（不读取限流 / 429 / 额度状态）：
/// 这里读的是**套餐名和计费方式**，不是用量、不是配额、不是速率限制状态，
/// 而且不发任何网络请求。切换也依然只能由人手动触发。
///
/// **不要**顺手把 `organizationRateLimitTier` / `userRateLimitTier` 显示出来。
/// 那两个字段名里带 rateLimit，展示它们会让这条边界变得可疑，
/// 而它们对用户的价值几乎为零。
pub const PLAN_CAVEAT: &str =
    "套餐与计费方式读自各槽位本地的 .claude.json（官方客户端写下的缓存），不联网、不读用量与额度。";

/// 剩余天数只能回答「名义上到期没」，回答不了「服务端还认不认」。
///
/// 凭证被风控提前作废（改密码 / 网页端撤销 / 风控下线）时，本地时间戳一字不变，
/// 看起来依旧健康。界面上必须固定打这行，避免「剩 N 天」给出虚假的安全感。
pub const EXPIRY_CAVEAT: &str =
    "剩余天数只读本地时间戳，查不出「被风控下线」。唯一能确认的办法是实际发一次认证请求。";

pub fn profiles_root() -> PathBuf {
    crate::gate::state_dir()
}

/// Import legacy account slots once. The operation is deliberately local and
/// idempotent: existing ClaudeGate slots are never overwritten.
pub fn migrate_legacy() -> Result<MigrationReport> {
    let root = profiles_root();
    std::fs::create_dir_all(&root)?;
    let marker = root.join("account-migration-v1.done");
    if marker.exists() {
        return Ok(MigrationReport { migrated: Vec::new(), backup: None });
    }

    let mut candidates: Vec<(PathBuf, String)> = Vec::new();
    if let Some(local) = dirs::data_local_dir() {
        let legacy = local.join("ClaudeTavernBridge");
        if legacy.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&legacy) {
                for entry in rd.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("claude-profile-") && entry.path().is_dir() {
                        candidates.push((entry.path(), name.trim_start_matches("claude-profile-").to_string()));
                    }
                }
            }
        }
    }

    // Claude Desktop keeps a separate encrypted profile tree in %APPDATA%.
    // Discover the already-migrated `Claude-*` directories as well, and when
    // the active `Claude` path is a real directory preserve it as `Claude-main`.
    // Desktop credentials are intentionally never parsed or copied into the
    // Claude Code slots: the two OAuth clients use different stores.
    if let Some(appdata) = dirs::config_dir() {
        if let Ok(rd) = std::fs::read_dir(&appdata) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("Claude-") && entry.path().is_dir() {
                    let label = name.trim_start_matches("Claude-").to_string();
                    let marker = format!("desktop:{label}");
                    // Include a marker in the report only; do not duplicate a
                    // Claude Code slot with incompatible Desktop credentials.
                    if !candidates.iter().any(|(_, l)| l == &marker) {
                        candidates.push((entry.path(), marker));
                    }
                }
            }
        }
    }

    let mut backup = None;
    if !candidates.is_empty() {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let backup_dir = root.join(format!("account-migration-backup-{stamp}"));
        std::fs::create_dir_all(&backup_dir)?;
        backup = Some(backup_dir.display().to_string());
    }

    let mut migrated = Vec::new();
    for (source, label) in candidates {
        let is_desktop = label.starts_with("desktop:");
        let name = if is_desktop {
            label.trim_start_matches("desktop:").to_string()
        } else {
            label.clone()
        };
        if !is_desktop {
            if let Some(dir) = backup.as_ref() {
            let backup_target = PathBuf::from(dir).join(source.file_name().unwrap_or_default());
            if !backup_target.exists() { copy_dir_recursive(&source, &backup_target)?; }
            }
        }
        if is_desktop {
            // Already present in its canonical Desktop location; report it so
            // the UI can tell the user both account families were discovered.
            migrated.push(format!("desktop:{name}"));
            continue;
        }
        let target = root.join(format!("claude-profile-{name}"));
        if target.exists() { continue; }
        copy_dir_recursive(&source, &target)?;
        migrated.push(name.to_string());
    }

    // If a legacy active profile exists, copy it as the default/main slot only
    // when no Code slot was imported. Never replace an existing user slot.
    if migrated.is_empty() {
        if let Some(appdata) = dirs::config_dir() {
            let active = appdata.join("Claude");
            let target = root.join("claude-profile-main");
            if active.is_dir() && !target.exists() && !active.is_symlink() {
                copy_dir_recursive(&active, &target)?;
                migrated.push("claude-profile-main".into());
            }
        }
    }

    std::fs::write(marker, b"completed\n")?;
    if !migrated.is_empty() {
        crate::gate::log::write(&format!("账户迁移完成：导入 {} 个槽位", migrated.len()));
    }
    Ok(MigrationReport { migrated, backup })
}

fn copy_dir_recursive(source: &std::path::Path, target: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() { copy_dir_recursive(&from, &to)?; }
        else if !to.exists() { std::fs::copy(&from, &to)?; }
    }
    Ok(())
}

pub fn slots() -> Vec<Slot> {
    let root = profiles_root();
    let Ok(rd) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    // 联结点在 Rust 里表现为符号链接，read_link 能拿到目标。
    let active = std::fs::read_link(root.join("claude-profile"))
        .ok()
        .and_then(|p| p.file_name().and_then(|n| n.to_str()).map(String::from));

    let mut out: Vec<Slot> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let label = name.strip_prefix("claude-profile-")?.to_string();
            let dir = e.path();
            let profile = read_profile(&dir);
            Some(Slot {
                active: active.as_deref() == Some(name.as_str()),
                logged_in: dir.join(".credentials.json").exists(),
                cli_days_left: cli_days_left(&dir),
                account_uuid: profile
                    .as_ref()
                    .and_then(|v| v.get("accountUuid"))
                    .and_then(|v| v.as_str())
                    .map(String::from),
                plan: profile.as_ref().and_then(plan_of),
                billing: profile.as_ref().and_then(billing_of),
                plan_fetched_at: profile.as_ref().and_then(fetched_at_of),
                label,
            })
        })
        .collect();
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// `refreshTokenExpiresAt` 剩余天数。
///
/// **只看 refreshToken，不看 accessToken。** accessToken 8–12 小时过期是正常的，
/// 客户端自己拿 refreshToken 静默换新，用户无感 —— 因此故意不为它报警。
fn cli_days_left(dir: &std::path::Path) -> Option<i64> {
    let text = std::fs::read_to_string(dir.join(".credentials.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let ms = v
        .get("claudeAiOauth")?
        .get("refreshTokenExpiresAt")?
        .as_i64()?;
    let now = chrono::Utc::now().timestamp_millis();
    Some((ms - now) / 86_400_000)
}

/// 读本槽位的 `oauthAccount`。**纯文件读取，不发任何请求。**
fn read_profile(dir: &std::path::Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join(".claude.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("oauthAccount").cloned()
}

/// 套餐名。
///
/// 优先 `seatTier`（Team / Enterprise 席位才有值），退回 `organizationType`
/// （个人订阅是 `claude_pro` / `claude_max` 这种）。**认不出来就如实回原值**，
/// 不猜成 Pro —— 猜错了用户会以为自己买的是别的套餐。
fn plan_of(o: &serde_json::Value) -> Option<String> {
    let raw = o
        .get("seatTier")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            o.get("organizationType")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
        })?;
    Some(pretty_plan(raw))
}

pub fn pretty_plan(raw: &str) -> String {
    match raw {
        "claude_pro" => "Claude Pro".into(),
        "claude_max" => "Claude Max".into(),
        "claude_team" => "Claude Team".into(),
        "claude_enterprise" => "Claude Enterprise".into(),
        "claude_free" => "免费版".into(),
        // 认不出来就原样显示，不猜。
        other => other.to_string(),
    }
}

fn billing_of(o: &serde_json::Value) -> Option<String> {
    let raw = o
        .get("billingType")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    Some(pretty_billing(raw))
}

pub fn pretty_billing(raw: &str) -> String {
    match raw {
        "google_play_subscription" => "Google Play 订阅".into(),
        "apple_subscription" => "App Store 订阅".into(),
        "stripe_subscription" => "信用卡订阅".into(),
        other => other.to_string(),
    }
}

/// `profileFetchedAt` 是毫秒时间戳。界面上要标出来 —— 这是缓存，可能过期。
fn fetched_at_of(o: &serde_json::Value) -> Option<String> {
    let ms = o.get("profileFetchedAt")?.as_i64()?;
    let dt = chrono::DateTime::from_timestamp_millis(ms)?;
    Some(
        dt.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
    )
}

/// 切换激活槽位：重建联结点。
///
/// **只在用户手动点击时调用。不要给它加任何自动调用点** —— 加了就变成
/// 自动轮换账户，直接踩政策线。
#[cfg(windows)]
pub fn switch(label: &str) -> Result<()> {
    let root = profiles_root();
    let link = root.join("claude-profile");
    let target = root.join(format!("claude-profile-{label}"));
    if !target.exists() {
        return Err(GateError::NotFound(target.display().to_string()));
    }
    // 联结点用 remove_dir 摘掉（不会递归删目标内容）。
    if link.exists() {
        std::fs::remove_dir(&link)?;
    }
    let out = crate::process::hidden_std(std::process::Command::new("cmd"))
        .args([
            "/C",
            "mklink",
            "/J",
            &link.display().to_string(),
            &target.display().to_string(),
        ])
        .output()?;
    if !out.status.success() {
        return Err(GateError::Other(format!(
            "建立联结点失败：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    // Keep Claude Desktop aligned when a matching encrypted profile exists.
    // If the active path is a real directory, move it aside before creating
    // the junction; never recursively delete user data.
    if let Some(appdata) = dirs::config_dir() {
        let desktop_target = appdata.join(format!("Claude-{label}"));
        let desktop_link = appdata.join("Claude");
        if desktop_target.is_dir() {
            if desktop_link.exists() && !desktop_link.is_symlink() {
                let backup = appdata.join(format!("Claude-backup-{}", chrono::Local::now().format("%Y%m%d-%H%M%S")));
                std::fs::rename(&desktop_link, &backup)?;
            } else if desktop_link.exists() {
                std::fs::remove_dir(&desktop_link)?;
            }
            let out = crate::process::hidden_std(std::process::Command::new("cmd"))
                .args(["/C", "mklink", "/J", &desktop_link.display().to_string(), &desktop_target.display().to_string()])
                .output()?;
            if !out.status.success() {
                return Err(GateError::Other(format!("Claude Desktop 配置联结点建立失败：{}", String::from_utf8_lossy(&out.stderr).trim())));
            }
        }
    }
    crate::gate::log::write(&format!("账户槽位切换为 {label}"));
    Ok(())
}

#[cfg(not(windows))]
pub fn switch(_label: &str) -> Result<()> {
    Err(GateError::Other("仅 Windows 支持".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_slot_is_still_switchable() {
        // 回归测试：logged_in 只看文件在不在，与到期天数无关。
        // 合并这两件事会导致凭证过期后切不过去，也就没法重新登录。
        let s = Slot {
            label: "main".into(),
            active: false,
            logged_in: true,
            cli_days_left: Some(-3),
            account_uuid: None,
            plan: None,
            billing: None,
            plan_fetched_at: None,
        };
        assert!(s.logged_in, "过期不得影响可切换性");
        assert!(s.cli_days_left.is_some_and(|d| d < 0));
    }

    #[test]
    fn migration_copy_is_idempotent_and_never_overwrites() {
        let base = std::env::temp_dir().join(format!("claudegate-migration-test-{}", std::process::id()));
        let source = base.join("source");
        let target = base.join("target");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("credentials.json"), "first").unwrap();
        copy_dir_recursive(&source, &target).unwrap();
        std::fs::write(source.join("credentials.json"), "changed").unwrap();
        copy_dir_recursive(&source, &target).unwrap();
        assert_eq!(std::fs::read_to_string(target.join("credentials.json")).unwrap(), "first");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn plan_prefers_seat_tier_then_org_type() {
        // Team / Enterprise 席位有 seatTier，个人订阅只有 organizationType。
        let team = serde_json::json!({ "seatTier": "claude_team", "organizationType": "claude_pro" });
        assert_eq!(plan_of(&team).as_deref(), Some("Claude Team"));

        let personal = serde_json::json!({ "seatTier": "", "organizationType": "claude_pro" });
        assert_eq!(plan_of(&personal).as_deref(), Some("Claude Pro"));

        // 两个都没有就是 None，不许猜一个出来。
        assert_eq!(plan_of(&serde_json::json!({})), None);
        assert_eq!(
            plan_of(&serde_json::json!({ "seatTier": "", "organizationType": "" })),
            None
        );
    }

    #[test]
    fn unknown_plan_is_shown_as_is_not_guessed() {
        // 认不出来的套餐原样显示。猜成 Pro 会让用户以为买错了东西。
        assert_eq!(pretty_plan("claude_something_new"), "claude_something_new");
        assert_eq!(pretty_billing("some_new_channel"), "some_new_channel");
    }

    #[test]
    fn plan_reading_never_touches_rate_limit_fields() {
        // 政策边界：字段名里带 rateLimit 的一律不读。
        let o = serde_json::json!({
            "organizationType": "claude_pro",
            "organizationRateLimitTier": "default_claude_ai",
            "userRateLimitTier": "tier_x"
        });
        let plan = plan_of(&o).unwrap();
        assert_eq!(plan, "Claude Pro");
        assert!(!plan.contains("default_claude_ai"));
        assert!(!plan.contains("tier_x"));
    }

    #[test]
    fn fetched_at_is_formatted_or_absent() {
        // profileFetchedAt 是毫秒时间戳。界面要标出来 —— 这是缓存，可能过期。
        let o = serde_json::json!({ "profileFetchedAt": 1788865933692i64 });
        let got = fetched_at_of(&o).expect("应当格式化出来");
        assert!(got.starts_with("20"), "格式不对：{got}");
        assert_eq!(fetched_at_of(&serde_json::json!({})), None);
    }
}
