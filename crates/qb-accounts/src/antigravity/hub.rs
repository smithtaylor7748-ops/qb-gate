//! 反重力 Hub 登的是哪个账户（0.32.0）。
//!
//! # 为什么 Hub 这一块要单独写
//!
//! IDE 把邮箱、档位、每个模型的剩余额度全写在自己的 `state.vscdb` 里（[`super::status`]），
//! **Hub 一样都不写**。2026-09-21 实机逐个核过：
//!
//! | 看过的地方 | 里面有什么 |
//! |---|---|
//! | `%APPDATA%\Antigravity\app_storage.json` | 16 个键，全是界面布局与引导状态，没有账户 |
//! | `~\.gemini\antigravity\antigravity_state.pbtxt` | 引导步骤、迁移状态、`installation_uuid` |
//! | `%APPDATA%\Antigravity\Local Storage` | 搜不到邮箱 |
//! | `…\logs\language_server.log` | 只有一句 `Auth succeeded`；`SetUserTier` 两次都是空串 |
//! | Windows 凭据管理器 | **只有这里有**：一条 `gemini:antigravity` |
//!
//! 所以使用者报的「反重力 IDE 登录了，反重力软件没匹配到账户」**不是 bug**：
//! 两个产品、两套令牌库，IDE 的登录进不了 Hub。
//!
//! # 邮箱是**本机解出来的，不是问来的**
//!
//! 那条凭据的 blob 是一段 UTF-8 JSON（本机实测 1439 字节）：
//!
//! ```text
//! auth_method : string
//! id_token    : string   ← OIDC 的 JWT
//! token       : { access_token, refresh_token, token_type, expiry }
//! ```
//!
//! `id_token` 的载荷里就有 `email` / `email_verified` / `exp` —— 一次 base64 解码的事，
//! **零网络请求**。这跟「用量只许读官方客户端自己写在本机的文件」是同一条口径：
//! 面板只是把 Hub 已经落在这台机器上的东西读出来显示。
//!
//! ⛔ **不验签、也不该验签。** 这里是显示，不是鉴权 —— 面板不拿它放行任何东西。
//! 载荷被人改过，最坏的后果是界面上显示一个错的邮箱；为此内置一份 Google 的公钥、
//! 再定期联网更新，换来的东西还不如代价大。
//!
//! # 唯一碰令牌的地方
//!
//! 读这条凭据推翻了 `CLAUDE.md`「令牌一个字不碰」那一条（0.32.0，使用者拍板）。
//! 边界：[`identity`] 只取 `id_token` 的**载荷**，`access_token` / `refresh_token`
//! 一个字节都不带出这个函数；返回值里没有、也永远不许有任何令牌字段。
//! 档位与配额要联网才拿得到，那部分在 `qb-app::usecase::antigravity_quota`，
//! 只在使用者点刷新图标时才发；它要的令牌由 [`super::token::hub_token`] 读，不经过这里。

use crate::error::Result;
use base64::Engine as _;
use serde::Serialize;
use ts_rs::TS;

/// Hub 的凭据目标名。`cmdkey /list` 看到的就是这一条。
///
/// ⛔ 只有这一处。别在别处再拼一遍 —— 跟「Claude 装在哪全项目只有一张表」同一条规矩。
pub const HUB_CREDENTIAL_TARGET: &str = "gemini:antigravity";

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AntigravityHubIdentity {
    /// Hub 登的那个账户。`None` = 凭据在、但载荷里没有邮箱。
    pub email: Option<String>,
    /// Google 说这个邮箱验证过没有。原样转述。
    pub email_verified: bool,
    /// `id_token` 的过期时刻，本地 `YYYY-MM-DD HH:MM`。
    ///
    /// ⚠ **过期不等于没登录** —— 旁边还有 refresh_token，Hub 自己会换新的。
    /// 界面上别把它写成「登录已失效」。
    pub token_expires_at: Option<String>,
    /// 已经过期了没有（按本机时钟）。只是说明这份读数有多旧。
    pub token_expired: bool,
    /// Hub 自己记的登录方式，原样。
    pub auth_method: Option<String>,
}

/// Hub 登没登录。**不读内容**，只问那条凭据在不在。
///
/// 这是默认路径：绝大多数时候界面只需要这一位信息。
pub fn logged_in() -> bool {
    qb_platform::credentials::exists(HUB_CREDENTIAL_TARGET)
}

/// Hub 登的是谁。读凭据 → 取 `id_token` → 解载荷。**零网络请求。**
///
/// - `Ok(None)`：没有这条凭据（没登录过），或者 blob 是空的。
/// - `Err`：凭据读不出来、blob 形状不认识 —— 界面照实说「读不出来」，
///   **不许降级成「没登录」**（§7.17）。
pub fn identity() -> Result<Option<AntigravityHubIdentity>> {
    let Some(blob) = qb_platform::credentials::read_generic(HUB_CREDENTIAL_TARGET)? else {
        return Ok(None);
    };
    if blob.is_empty() {
        return Ok(None);
    }
    parse_blob(blob.as_bytes(), chrono::Local::now().timestamp()).map(Some)
}

/// 纯函数：从凭据 blob 解出身份。单测拿造出来的 blob 打这一条。
pub fn parse_blob(bytes: &[u8], now_secs: i64) -> Result<AntigravityHubIdentity> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| err("Hub 的凭据不是 UTF-8 文本，形状不认识"))?
        .trim_start_matches('\u{feff}');
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| err(&format!("Hub 的凭据不是 JSON：{e}")))?;

    let auth_method = v
        .get("auth_method")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let jwt = v
        .get("id_token")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| err("Hub 的凭据里没有 id_token，解不出账户"))?;
    let claims = jwt_payload(jwt)?;

    let exp = claims.get("exp").and_then(serde_json::Value::as_i64);
    Ok(AntigravityHubIdentity {
        email: claims
            .get("email")
            .and_then(serde_json::Value::as_str)
            // 跟 IDE 那边一条口径：不含 '@' 的不当邮箱用（status.rs 也是这么写的）。
            .filter(|s| s.contains('@'))
            .map(str::to_owned),
        email_verified: claims
            .get("email_verified")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        token_expires_at: exp.and_then(local_minute),
        token_expired: exp.is_some_and(|e| e <= now_secs),
        auth_method,
    })
}

/// 取 JWT 的第二段（载荷）并解 JSON。不验签，理由见模块头。
fn jwt_payload(jwt: &str) -> Result<serde_json::Value> {
    let payload = jwt
        .split('.')
        .nth(1)
        .ok_or_else(|| err("id_token 不是三段式的 JWT"))?;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|e| err(&format!("id_token 的载荷 base64 解不开：{e}")))?;
    serde_json::from_slice(&raw).map_err(|e| err(&format!("id_token 的载荷不是 JSON：{e}")))
}

fn local_minute(secs: i64) -> Option<String> {
    use chrono::TimeZone as _;
    chrono::Local
        .timestamp_opt(secs, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
}

fn err(msg: &str) -> crate::error::GateError {
    crate::error::GateError::Other(msg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一份形状跟本机那条一样的 blob。
    ///
    /// ⛔ 里面的 `id_token` 是**现编的**，不是任何真令牌 —— 单测不许碰真实的运行期状态。
    fn blob(claims: serde_json::Value, auth_method: &str) -> Vec<u8> {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap());
        let jwt = format!("aGVhZGVy.{payload}.c2ln");
        serde_json::to_vec(&serde_json::json!({
            "auth_method": auth_method,
            "id_token": jwt,
            "token": { "access_token": "x", "refresh_token": "y",
                       "token_type": "Bearer", "expiry": "2026-09-21T10:00:00Z" }
        }))
        .unwrap()
    }

    #[test]
    fn the_email_comes_out_of_the_id_token_without_any_network_call() {
        let b = blob(
            serde_json::json!({"email":"someone@example.com","email_verified":true,"exp": 4_000_000_000i64}),
            "google",
        );
        let id = parse_blob(&b, 1_000).expect("解得出来");
        assert_eq!(id.email.as_deref(), Some("someone@example.com"));
        assert!(id.email_verified);
        assert_eq!(id.auth_method.as_deref(), Some("google"));
        assert!(!id.token_expired);
        assert!(id.token_expires_at.is_some());
    }

    #[test]
    fn an_expired_id_token_is_still_a_logged_in_account() {
        // 旁边有 refresh_token，Hub 自己会换新的。界面上写「登录已失效」就是错的。
        let b = blob(
            serde_json::json!({"email":"someone@example.com","exp": 1_000i64}),
            "google",
        );
        let id = parse_blob(&b, 2_000).expect("解得出来");
        assert!(id.token_expired);
        assert_eq!(
            id.email.as_deref(),
            Some("someone@example.com"),
            "过期了也照样知道登的是谁"
        );
    }

    #[test]
    fn a_claim_without_an_at_sign_is_not_treated_as_an_email() {
        let b = blob(serde_json::json!({"email":"not-an-email"}), "google");
        assert_eq!(parse_blob(&b, 0).expect("解得出来").email, None);
    }

    #[test]
    fn an_unrecognised_shape_is_an_error_not_a_silent_logged_out() {
        // §7.17：查不到就如实说「查不到」，不许降级成「没有」。
        assert!(parse_blob(b"not json at all", 0).is_err());
        assert!(parse_blob(br#"{"auth_method":"google"}"#, 0).is_err());
        assert!(parse_blob(br#"{"id_token":"only-one-segment"}"#, 0).is_err());
    }

    #[test]
    fn the_credential_target_is_written_down_exactly_once() {
        // 跟「Claude 装在哪全项目只有一张表」同一条规矩。改名字只改这里。
        assert_eq!(HUB_CREDENTIAL_TARGET, "gemini:antigravity");
    }
}
