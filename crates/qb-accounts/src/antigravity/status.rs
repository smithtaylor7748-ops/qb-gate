//! 反重力 IDE 写在本机的账户状态：登没登录、谁、什么档位、各模型剩多少额度。
//!
//! # 数据从哪来：**IDE 自己写的本机文件**，零网络请求
//!
//! `<用户数据目录>\User\globalStorage\state.vscdb`（VS Code 分支的全局存储，SQLite，
//! 表 `ItemTable(key, value)`）。两个键：
//!
//! | 键 | 内容 | 我们怎么用 |
//! |---|---|---|
//! | `antigravityUnifiedStateSync.oauthToken` | 登录令牌（base64 protobuf） | 登录态**只问 `length(value) > 0`**；值只在使用者点反重力的刷新图标时由 [`super::token`] 读（2026-09-23） |
//! | `antigravityUnifiedStateSync.userStatus` | 账户状态（base64 protobuf，里面再套一层 base64） | 姓名、邮箱、档位、每个模型的剩余额度与重置时间 |
//!
//! 2026-09-21 实机对着 IDE 1.107 的库解出来的形状（`userStatus` 外层是一个 map 条目：
//! `1 → {1: "userStatusSentinelKey", 2: {1: <base64>}}`，里层解开之后）：
//!
//! ```text
//! 3   显示名
//! 7   邮箱
//! 33  模型表 → 每条 1（repeated）：
//!       1     显示名（"Gemini 3.7 Flash (High)"）
//!       2.1   模型枚举号（1298）—— 跟生成记录里 `model_enum` 对得上
//!       15.1  剩余额度比例，float32，0–1
//!       15.2  重置时间，google.protobuf.Timestamp
//!       16/17 标签（"Fast" / "Limited time"）
//! 36  档位：1 id（"g1-pro-tier"）· 2 名字（"Google AI Pro"）· 7 升级链接
//! ```
//!
//! 这跟 CLAUDE.md「用量只许读官方客户端**自己写在本机的文件**，零网络请求，只用于显示」
//! 是同一条路 —— Claude 那边读的是桌面端写的 `plan-usage-history.json`，这里读的是
//! IDE 写的 `state.vscdb`。**不调 `cloudcode-pa` 的任何接口，不拿令牌去问 Google**。
//! Antigravity-Tools-Lite（CC BY-NC-SA，一行没抄）走的是内部接口那条路，这里故意不走。
//!
//! # 它只有 IDE 上次同步时那么新
//!
//! 值是 IDE 跑着的时候由它的语言服务器推过来写下的。IDE 关着，这里就停在最后一次；
//! 界面上要写「IDE 上次写入」的时间，不许把它说成「此刻」。**Hub 没有这个库**
//! （它的状态在语言服务器进程里、令牌在凭据管理器），所以 Hub 单独装的机器上这一块是空的，
//! 界面要说得出下一步（装 IDE，或在 IDE 里登录一次）。
//!
//! # 只读
//!
//! `SQLITE_OPEN_READ_ONLY`，不建 `-shm` 之外的任何文件，IDE 正在写也照读
//! （VS Code 用的是普通回滚日志，读到一半锁住就报「稍后再读」，不报 0）。

use super::proto::{timestamp_secs, Message};
use crate::error::{GateError, Result};
use base64::Engine as _;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

pub(super) const TOKEN_KEY: &str = "antigravityUnifiedStateSync.oauthToken";
const STATUS_KEY: &str = "antigravityUnifiedStateSync.userStatus";

/// 全局存储库在用户数据目录下的位置（VS Code 的约定）。
pub fn state_db(user_data: &Path) -> PathBuf {
    user_data
        .join("User")
        .join("globalStorage")
        .join("state.vscdb")
}

/// 一个模型此刻（IDE 上次同步时）的额度。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AntigravityModelQuota {
    /// IDE 里显示的名字，原样。
    pub label: String,
    /// 模型枚举号；生成记录里 `model_enum` 是 `MODEL_PLACEHOLDER_M<号>`。`0` = 没读到。
    #[ts(type = "number")]
    pub model_id: u64,
    /// 剩余比例 0–1。`None` = 这一条没带额度信息。
    pub remaining: Option<f32>,
    /// 重置时刻，本地时间 `YYYY-MM-DD HH:MM`。
    pub reset_at: Option<String>,
    /// 重置时刻的 Unix 秒，给界面算「几小时后」用。
    #[ts(type = "number | null")]
    pub reset_epoch: Option<i64>,
    /// "Fast" / "Limited time" 之类的标签，原样。
    pub tags: Vec<String>,
}

/// IDE 写下的账户状态。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AntigravityIdentity {
    pub name: Option<String>,
    pub email: Option<String>,
    /// 档位 id（`g1-pro-tier`）与显示名（`Google AI Pro`）。
    pub tier_id: Option<String>,
    pub tier_name: Option<String>,
    pub models: Vec<AntigravityModelQuota>,
    /// 状态库文件的最后写入时间（本地 `YYYY-MM-DD HH:MM`）。这是「这份数据多新」的上界。
    pub written_at: String,
}

/// 登没登录 —— 只看令牌那一行**在不在、长度是否大于零**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginState {
    pub logged_in: bool,
    /// 一句人话。
    pub detail: String,
}

pub(super) fn open_read_only(db: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| GateError::Other(format!("打不开反重力 IDE 的状态库：{e}")))?;
    // IDE 正在写的那几十毫秒里别立刻报「正忙」—— 账户页每 15 秒读一次，
    // 一闪而过的 SQLITE_BUSY 会让身份行在「邮箱」和「读不出来」之间来回跳。
    let _ = conn.busy_timeout(std::time::Duration::from_millis(250));
    Ok(conn)
}

/// 登录态。库不在 = 这份资料目录还没起过 IDE。
pub fn login_state(user_data: &Path) -> LoginState {
    let db = state_db(user_data);
    if !db.is_file() {
        return LoginState {
            logged_in: false,
            detail: "未登录 · 还没在这个槽位起过 IDE".into(),
        };
    }
    let conn = match open_read_only(&db) {
        Ok(c) => c,
        Err(e) => {
            return LoginState {
                logged_in: false,
                detail: format!("状态库不可读：{e}"),
            }
        }
    };
    // ⛔ 只取长度。`value` 本身一个字节都不读进来 —— 面板不持有令牌。
    let len: std::result::Result<Option<i64>, _> = conn
        .query_row(
            "SELECT length(value) FROM ItemTable WHERE key = ?1",
            [TOKEN_KEY],
            |r| r.get::<_, Option<i64>>(0),
        )
        .optional()
        .map(Option::flatten);
    match len {
        Ok(Some(n)) if n > 0 => LoginState {
            logged_in: true,
            detail: "已登录 · 令牌由 IDE 自己保管在它的状态库里".into(),
        },
        Ok(_) => LoginState {
            logged_in: false,
            detail: "未登录 · 在 IDE 窗口里用 Google 登录".into(),
        },
        Err(e) => LoginState {
            logged_in: false,
            detail: format!("状态库正忙或损坏（{e}），稍后再读"),
        },
    }
}

fn local_minute(secs: i64) -> Option<String> {
    use chrono::TimeZone;
    match chrono::Local.timestamp_opt(secs, 0) {
        chrono::LocalResult::Single(t) => Some(t.format("%Y-%m-%d %H:%M").to_string()),
        _ => None,
    }
}

/// 把 `userStatus` 那条 base64 解到里层消息。**纯函数**，测试拿录下来的形状喂。
///
/// 外层：map 条目 `1 → {1: key, 2: {1: base64 字符串}}`；里层再 base64 一次。
pub fn decode_user_status(value: &[u8]) -> Result<Vec<u8>> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let outer = b64
        .decode(value)
        .map_err(|_| GateError::Other("userStatus 不是 base64".into()))?;
    let outer =
        Message::parse(&outer).map_err(|e| GateError::Other(format!("userStatus 外层：{e}")))?;
    let inner_b64 = outer
        .all_sub(1)
        .filter(|entry| entry.str(1).is_some_and(|k| k.contains("userStatus")))
        .find_map(|entry| entry.sub(2).and_then(|v| v.str(1).map(str::to_owned)))
        .ok_or_else(|| GateError::Other("userStatus 里没有状态条目".into()))?;
    b64.decode(inner_b64.trim())
        .map_err(|_| GateError::Other("userStatus 里层不是 base64".into()))
}

/// 从里层消息里把身份与额度读出来。**纯函数。**
pub fn parse_identity(inner: &[u8], written_at: String) -> Result<AntigravityIdentity> {
    let m = Message::parse(inner).map_err(|e| GateError::Other(format!("userStatus 里层：{e}")))?;
    let tier = m.sub(36);
    let mut models = Vec::new();
    if let Some(table) = m.sub(33) {
        for row in table.all_sub(1) {
            let Some(label) = row.str(1) else { continue };
            let quota = row.sub(15);
            let reset_epoch = quota
                .as_ref()
                .and_then(|q| q.bytes(2))
                .and_then(timestamp_secs)
                .filter(|s| *s > 0);
            models.push(AntigravityModelQuota {
                label: label.to_string(),
                model_id: row.sub(2).and_then(|s| s.varint(1)).unwrap_or(0),
                remaining: quota
                    .as_ref()
                    .and_then(|q| q.f32(1))
                    .filter(|f| f.is_finite() && (0.0..=1.0).contains(f)),
                reset_at: reset_epoch.and_then(local_minute),
                reset_epoch,
                tags: [16u32, 17]
                    .into_iter()
                    .filter_map(|n| row.str(n))
                    .filter(|t| !t.is_empty())
                    .map(str::to_owned)
                    .collect(),
            });
        }
    }
    Ok(AntigravityIdentity {
        name: m.str(3).filter(|s| !s.is_empty()).map(str::to_owned),
        email: m.str(7).filter(|s| s.contains('@')).map(str::to_owned),
        tier_id: tier.as_ref().and_then(|t| t.str(1)).map(str::to_owned),
        tier_name: tier.as_ref().and_then(|t| t.str(2)).map(str::to_owned),
        models,
        written_at,
    })
}

/// 读这份资料目录的账户状态。库不在 / 没登录过 → `Ok(None)`；库坏了 → `Err`。
pub fn identity(user_data: &Path) -> Result<Option<AntigravityIdentity>> {
    let db = state_db(user_data);
    let Ok(meta) = std::fs::metadata(&db) else {
        return Ok(None);
    };
    let written_at = meta
        .modified()
        .ok()
        .map(chrono::DateTime::<chrono::Local>::from)
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    let conn = open_read_only(&db)?;
    let value: Option<Vec<u8>> = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = ?1",
            [STATUS_KEY],
            |r| {
                // VS Code 存的是文本列；有的构建存成 blob。两种都收。
                r.get::<_, rusqlite::types::Value>(0).map(|v| match v {
                    rusqlite::types::Value::Text(s) => s.into_bytes(),
                    rusqlite::types::Value::Blob(b) => b,
                    _ => Vec::new(),
                })
            },
        )
        .optional()
        .map_err(|e| GateError::Other(format!("读反重力 IDE 状态库失败：{e}")))?;
    let Some(value) = value.filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let inner = decode_user_status(&value)?;
    parse_identity(&inner, written_at).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint_bytes(mut v: u64) -> Vec<u8> {
        let mut b = Vec::new();
        while v >= 0x80 {
            b.push((v as u8 & 0x7f) | 0x80);
            v >>= 7;
        }
        b.push(v as u8);
        b
    }
    fn tag(number: u32, wire: u8) -> Vec<u8> {
        varint_bytes(u64::from(number) << 3 | u64::from(wire))
    }
    fn field_bytes(number: u32, payload: &[u8]) -> Vec<u8> {
        let mut b = tag(number, 2);
        b.extend(varint_bytes(payload.len() as u64));
        b.extend_from_slice(payload);
        b
    }
    fn field_varint(number: u32, v: u64) -> Vec<u8> {
        let mut b = tag(number, 0);
        b.extend(varint_bytes(v));
        b
    }
    fn field_f32(number: u32, v: f32) -> Vec<u8> {
        let mut b = tag(number, 5);
        b.extend(v.to_bits().to_le_bytes());
        b
    }

    /// 照实机形状拼一份：一个模型 60% 剩余、一个没带额度；档位 Google AI Pro。
    fn inner_status() -> Vec<u8> {
        let mut model_a = field_bytes(1, b"Gemini 3.7 Flash (High)");
        model_a.extend(field_bytes(2, &field_varint(1, 1298)));
        let mut quota = field_f32(1, 0.6);
        quota.extend(field_bytes(2, &field_varint(1, 1_790_000_000)));
        model_a.extend(field_bytes(15, &quota));
        model_a.extend(field_bytes(16, b"Fast"));
        model_a.extend(field_bytes(17, b"Limited time"));
        let model_b = field_bytes(1, b"GPT-OSS 120B (Medium)");
        let mut table = field_bytes(1, &model_a);
        table.extend(field_bytes(1, &model_b));
        let mut tier = field_bytes(1, b"g1-pro-tier");
        tier.extend(field_bytes(2, b"Google AI Pro"));
        let mut m = field_varint(2, 1);
        m.extend(field_bytes(3, "某 人".as_bytes()));
        m.extend(field_bytes(7, b"someone@example.test"));
        m.extend(field_bytes(33, &table));
        m.extend(field_bytes(36, &tier));
        m
    }

    fn wrapped(inner: &[u8]) -> Vec<u8> {
        let b64 = base64::engine::general_purpose::STANDARD;
        let inner_b64 = b64.encode(inner);
        let value = field_bytes(1, inner_b64.as_bytes());
        let mut entry = field_bytes(1, b"userStatusSentinelKey");
        entry.extend(field_bytes(2, &value));
        let outer = field_bytes(1, &entry);
        b64.encode(outer).into_bytes()
    }

    #[test]
    fn identity_and_quota_come_out_of_the_double_base64_envelope() {
        let inner = decode_user_status(&wrapped(&inner_status())).unwrap();
        let id = parse_identity(&inner, "2026-09-21 10:00".into()).unwrap();
        assert_eq!(id.email.as_deref(), Some("someone@example.test"));
        assert_eq!(id.name.as_deref(), Some("某 人"));
        assert_eq!(id.tier_id.as_deref(), Some("g1-pro-tier"));
        assert_eq!(id.tier_name.as_deref(), Some("Google AI Pro"));
        assert_eq!(id.models.len(), 2);
        let a = &id.models[0];
        assert_eq!(a.label, "Gemini 3.7 Flash (High)");
        assert_eq!(a.model_id, 1298);
        assert!((a.remaining.unwrap() - 0.6).abs() < 1e-6);
        assert_eq!(a.reset_epoch, Some(1_790_000_000));
        assert!(a.reset_at.is_some());
        assert_eq!(a.tags, vec!["Fast", "Limited time"]);
        // 没带额度的模型：`remaining` 是 None，不是 0 —— 0 会被读成「用光了」。
        let b = &id.models[1];
        assert_eq!(b.remaining, None);
        assert_eq!(b.reset_at, None);
        assert_eq!(b.model_id, 0);
    }

    #[test]
    fn a_status_blob_without_the_entry_or_not_base64_is_an_error_not_an_empty_identity() {
        assert!(decode_user_status(b"not base64!!").is_err());
        let b64 = base64::engine::general_purpose::STANDARD;
        let other = field_bytes(1, &{
            let mut e = field_bytes(1, b"somethingElse");
            e.extend(field_bytes(2, &field_bytes(1, b"AAA=")));
            e
        });
        assert!(decode_user_status(b64.encode(other).as_bytes()).is_err());
    }

    /// 剩余比例只认 0–1 之间的有限数；越界的当没有。
    #[test]
    fn a_remaining_fraction_outside_zero_to_one_is_dropped() {
        let mut model = field_bytes(1, b"X");
        model.extend(field_bytes(15, &field_f32(1, 7.5)));
        let table = field_bytes(1, &model);
        let m = field_bytes(33, &table);
        let id = parse_identity(&m, String::new()).unwrap();
        assert_eq!(id.models[0].remaining, None);
        assert_eq!(id.email, None);
    }

    /// 登录态：库不在 → 未登录；令牌行长度 0 → 未登录；> 0 → 已登录。值本身从不读。
    #[test]
    fn login_state_only_looks_at_the_token_row_length() {
        let root = std::env::temp_dir().join(format!("qb-ag-status-{}", crate::config_io::id()));
        let db = state_db(&root);
        assert!(!login_state(&root).logged_in);
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)",
        )
        .unwrap();
        assert!(!login_state(&root).logged_in, "有库没行");
        conn.execute(
            "INSERT INTO ItemTable VALUES (?1, ?2)",
            rusqlite::params![TOKEN_KEY, ""],
        )
        .unwrap();
        assert!(!login_state(&root).logged_in, "空令牌");
        conn.execute(
            "INSERT INTO ItemTable VALUES (?1, ?2)",
            rusqlite::params![TOKEN_KEY, "opaque"],
        )
        .unwrap();
        assert!(login_state(&root).logged_in);
        // 身份：没有 userStatus 行 → None；有 → 解得出邮箱。
        assert_eq!(identity(&root).unwrap(), None);
        conn.execute(
            "INSERT INTO ItemTable VALUES (?1, ?2)",
            rusqlite::params![STATUS_KEY, wrapped(&inner_status())],
        )
        .unwrap();
        let id = identity(&root).unwrap().unwrap();
        assert_eq!(id.email.as_deref(), Some("someone@example.test"));
        assert!(!id.written_at.is_empty());
        drop(conn);
        std::fs::remove_dir_all(root).unwrap();
    }
}
