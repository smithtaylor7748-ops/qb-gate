//! 反重力的登录令牌：IDE 槽位里那一行、Hub 凭据里那一段（2026-09-23）。
//!
//! # 为什么现在要读它
//!
//! 使用者 2026-09-23 拍板两件事：
//!
//! 1. 反重力账户的额度要**联网**查 —— 本机 `userStatus` 每个模型只有一个剩余比例、
//!    一个重置时刻，没有「5 小时 / 每周」两个窗口，而且只有 IDE 开着时才更新；
//! 2. 访问令牌过期时，**面板在内存里换新**（访问令牌只有一小时，只有 IDE 开着它才自己换）。
//!
//! 于是在使用者点反重力那颗刷新图标的那一刻，面板要拿到这个账户的访问令牌与刷新令牌。
//! 这推翻了 [`super::status`] 文件头「令牌的值从不读进内存」那条 —— 那条现在只对
//! 登录态（`login_state`，只问长度）成立。
//!
//! # 形状（2026-09-23 实机**只打形状**核过，一个值都没打印）
//!
//! IDE：`state.vscdb` 的 `antigravityUnifiedStateSync.oauthToken`，base64 → protobuf：
//!
//! ```text
//! 1 (repeated) 条目 → {1: 键名, 2: {1: <base64 字符串>}}
//!     其中一条的值再 base64 → {
//!         1  access_token（ya29. 开头）
//!         2  token_type（"Bearer"）
//!         3  refresh_token（1// 开头）
//!         4  过期时刻，google.protobuf.Timestamp → 1: 秒
//!     }
//! ```
//!
//! 实机上有两条条目，只有一条是这个形状。**按「里面有没有令牌」认，不按键名认** ——
//! 键名是 IDE 内部的叫法，下一版可能就改。
//!
//! Hub：凭据管理器里 `gemini:antigravity` 那条 JSON 的 `token{access_token, refresh_token, expiry}`，
//! `expiry` 是 RFC3339（[`super::hub`] 文件头有整条 blob 的形状）。
//!
//! # ⛔ 三条
//!
//! 1. **令牌只装在 [`Secret`] 里**：`Drop` 时抹零，没有 `Debug` / `Serialize` / `Clone`。
//!    [`OAuthToken`] 因此也没有 —— 它永远不许进 `AntigravityStatus`、日志、事件、审计。
//! 2. **只读。** 不写回 IDE 的库、不写回凭据管理器。换新来的访问令牌只放在
//!    `qb-app::usecase::antigravity_quota` 的内存里。
//! 3. **解不开就报错，不当成「没登录」**（§7.17）。

use super::proto::{timestamp_secs, Message};
use super::status::{open_read_only, state_db, TOKEN_KEY};
use crate::error::{GateError, Result};
use base64::Engine as _;
use qb_platform::credentials::Secret;
use rusqlite::OptionalExtension;
use std::path::Path;

/// 令牌的形状挪到了 [`crate::oauth`]（Gemini CLI 那一半也用它；留在这里会跟 `gemini` 成环）。
pub use crate::oauth::OAuthToken;

/// 读 IDE 用户数据目录（槽位或默认那份）里那一行令牌。
///
/// - `Ok(None)`：库不在、没有那一行、那一行是空的 —— 这份资料没登录。
/// - `Err`：库打不开、形状不认识 —— 界面照实说「读不出来」。
pub fn ide_token(user_data: &Path) -> Result<Option<OAuthToken>> {
    let db = state_db(user_data);
    if !db.is_file() {
        return Ok(None);
    }
    let conn = open_read_only(&db)?;
    let value: Option<Vec<u8>> = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = ?1",
            [TOKEN_KEY],
            |r| {
                // 跟 `status::identity` 同一个口径：文本列、blob 列两种都收。
                r.get::<_, rusqlite::types::Value>(0).map(|v| match v {
                    rusqlite::types::Value::Text(s) => s.into_bytes(),
                    rusqlite::types::Value::Blob(b) => b,
                    _ => Vec::new(),
                })
            },
        )
        .optional()
        .map_err(|e| GateError::Other(format!("读反重力 IDE 的令牌失败：{e}")))?;
    let Some(value) = value.filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    // 那一行本身就是令牌的一种写法 —— 读出来的字节同样要抹零。
    let value = Secret::new(value);
    decode_ide_token(value.as_bytes()).map(Some)
}

/// 纯函数：IDE 那一行的值 → 令牌。单测拿造出来的形状喂。
pub fn decode_ide_token(value: &[u8]) -> Result<OAuthToken> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let text = std::str::from_utf8(value)
        .map_err(|_| err("IDE 的令牌那一行不是文本"))?
        .trim();
    let outer = Secret::new(
        b64.decode(text)
            .map_err(|_| err("IDE 的令牌那一行不是 base64"))?,
    );
    let outer_msg =
        Message::parse(outer.as_bytes()).map_err(|e| err(&format!("IDE 的令牌外层：{e}")))?;
    for entry in outer_msg.all_sub(1) {
        let Some(inner_b64) = entry.sub(2).and_then(|v| v.str(1)) else {
            continue;
        };
        let Ok(inner) = b64.decode(inner_b64.trim()) else {
            continue;
        };
        let inner = Secret::new(inner);
        let Ok(m) = Message::parse(inner.as_bytes()) else {
            continue;
        };
        let access = m.str(1).filter(|s| !s.is_empty());
        let refresh = m.str(3).filter(|s| !s.is_empty());
        if access.is_none() && refresh.is_none() {
            continue;
        }
        return Ok(OAuthToken {
            access: access.map(|s| Secret::new(s.as_bytes().to_vec())),
            refresh: refresh.map(|s| Secret::new(s.as_bytes().to_vec())),
            expires_at: m.bytes(4).and_then(timestamp_secs).filter(|s| *s > 0),
        });
    }
    Err(err(
        "IDE 的令牌那一行里没有认得出的令牌条目 —— IDE 可能换了存法",
    ))
}

/// 读 Hub 在凭据管理器里那一条，取令牌。没有那一条就是 `Ok(None)`。
pub fn hub_token() -> Result<Option<OAuthToken>> {
    let Some(blob) = qb_platform::credentials::read_generic(super::hub::HUB_CREDENTIAL_TARGET)?
    else {
        return Ok(None);
    };
    if blob.is_empty() {
        return Ok(None);
    }
    parse_hub_token(blob.as_bytes()).map(Some)
}

/// 纯函数：Hub 的凭据 blob → 令牌。
pub fn parse_hub_token(bytes: &[u8]) -> Result<OAuthToken> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| err("Hub 的凭据不是 UTF-8 文本，形状不认识"))?
        .trim_start_matches('\u{feff}');
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| err(&format!("Hub 的凭据不是 JSON：{e}")))?;
    let t = v
        .get("token")
        .ok_or_else(|| err("Hub 的凭据里没有 token 那一段"))?;
    let secret = |key: &str| {
        t.get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| Secret::new(s.as_bytes().to_vec()))
    };
    let token = OAuthToken {
        access: secret("access_token"),
        refresh: secret("refresh_token"),
        expires_at: t
            .get("expiry")
            .and_then(serde_json::Value::as_str)
            .and_then(|e| chrono::DateTime::parse_from_rfc3339(e).ok())
            .map(|d| d.timestamp()),
    };
    if token.access.is_none() && token.refresh.is_none() {
        return Err(err("Hub 的凭据里既没有访问令牌，也没有刷新令牌"));
    }
    Ok(token)
}

fn err(msg: &str) -> GateError {
    GateError::Other(msg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ⛔ 下面所有「令牌」都是现编的字符串，不是任何真令牌 —— 单测不许碰真实的运行期状态。

    fn varint_bytes(mut v: u64) -> Vec<u8> {
        let mut b = Vec::new();
        while v >= 0x80 {
            b.push((v as u8 & 0x7f) | 0x80);
            v >>= 7;
        }
        b.push(v as u8);
        b
    }
    fn field_bytes(number: u32, payload: &[u8]) -> Vec<u8> {
        let mut b = varint_bytes(u64::from(number) << 3 | 2);
        b.extend(varint_bytes(payload.len() as u64));
        b.extend_from_slice(payload);
        b
    }
    fn field_varint(number: u32, v: u64) -> Vec<u8> {
        let mut b = varint_bytes(u64::from(number) << 3);
        b.extend(varint_bytes(v));
        b
    }

    /// 照实机形状拼一行：两条条目，第一条不是令牌、第二条才是。
    fn ide_row(expiry: Option<u64>) -> Vec<u8> {
        let b64 = base64::engine::general_purpose::STANDARD;
        let mut inner = field_bytes(1, b"fake-access");
        inner.extend(field_bytes(2, b"Bearer"));
        inner.extend(field_bytes(3, b"fake-refresh"));
        if let Some(e) = expiry {
            inner.extend(field_bytes(4, &field_varint(1, e)));
        }
        let token_value = field_bytes(1, b64.encode(&inner).as_bytes());
        let mut token_entry = field_bytes(1, b"someTokenKey");
        token_entry.extend(field_bytes(2, &token_value));

        let other_value = field_bytes(1, b"not a token at all");
        let mut other_entry = field_bytes(1, b"someOtherSentinelKey");
        other_entry.extend(field_bytes(2, &other_value));

        let mut outer = field_bytes(1, &other_entry);
        outer.extend(field_bytes(1, &token_entry));
        b64.encode(outer).into_bytes()
    }

    #[test]
    fn the_ide_row_is_found_by_its_contents_not_by_its_key_name() {
        let t = decode_ide_token(&ide_row(Some(1_790_000_000))).expect("解得开");
        assert_eq!(
            t.access.as_ref().and_then(Secret::as_str),
            Some("fake-access")
        );
        assert_eq!(
            t.refresh.as_ref().and_then(Secret::as_str),
            Some("fake-refresh")
        );
        assert_eq!(t.expires_at, Some(1_790_000_000));
    }

    #[test]
    fn an_ide_row_we_do_not_understand_is_an_error_not_a_logged_out_account() {
        // §7.17：解不开 ≠ 没登录。
        assert!(decode_ide_token(b"!!! not base64 !!!").is_err());
        let b64 = base64::engine::general_purpose::STANDARD;
        let no_token = b64.encode(field_bytes(1, &field_bytes(1, b"key-only")));
        assert!(decode_ide_token(no_token.as_bytes()).is_err());
    }

    #[test]
    fn a_missing_expiry_means_the_access_token_is_not_trusted() {
        let t = decode_ide_token(&ide_row(None)).expect("解得开");
        assert_eq!(t.expires_at, None);
        assert!(!t.access_usable(0, 300), "不知道几点过期，就当它不能用");
    }

    #[test]
    fn the_access_token_is_retired_a_margin_before_it_actually_expires() {
        let t = decode_ide_token(&ide_row(Some(10_000))).expect("解得开");
        assert!(t.access_usable(9_000, 300));
        assert!(!t.access_usable(9_800, 300), "五分钟之内过期的就要换");
        assert!(!t.access_usable(20_000, 300));
    }

    #[test]
    fn the_hub_blob_yields_both_tokens_and_the_expiry() {
        let blob = br#"{"auth_method":"google","id_token":"a.b.c",
            "token":{"access_token":"fake-access","refresh_token":"fake-refresh",
                     "token_type":"Bearer","expiry":"2026-09-21T10:00:00.5+08:00"}}"#;
        let t = parse_hub_token(blob).expect("解得开");
        assert_eq!(
            t.access.as_ref().and_then(Secret::as_str),
            Some("fake-access")
        );
        assert_eq!(
            t.refresh.as_ref().and_then(Secret::as_str),
            Some("fake-refresh")
        );
        // +08:00 的 10:00:00.5 = UTC 02:00:00.5；小数秒截掉。
        assert_eq!(t.expires_at, Some(1_789_956_000));
    }

    #[test]
    fn a_hub_blob_without_tokens_is_an_error() {
        assert!(parse_hub_token(br#"{"token":{}}"#).is_err());
        assert!(parse_hub_token(br#"{"id_token":"a.b.c"}"#).is_err());
        assert!(parse_hub_token(b"not json").is_err());
    }
}
