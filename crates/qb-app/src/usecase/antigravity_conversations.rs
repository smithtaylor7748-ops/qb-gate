//! 反重力的对话列表（账户页「会话」弹窗的反重力那一页）。
//!
//! 对话库在 `~\.gemini\antigravity*\conversations\<对话 id>.db`（`qb_accounts::antigravity::usage`
//! 说过：**全机共用，不分账户**）。这里只读：时间、大小、能读出来的标题。不删、不改。
//!
//! 标题：`steps` 里第一条 `step_type = 14`（使用者输入）那一步，`step_payload` 的 19.2 字段
//! （2026-11-05 对着本机的库逐字段看出来的，没有 Google 的 schema）。读不出就是 `None`，界面写「读不出标题」。
//! 旧格式 `.pb` 不解，只列时间和大小。

use qb_accounts::antigravity::{proto::Message, usage::conversation_dirs};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AntigravityConversation {
    /// 对话 id（文件名去掉扩展名）。
    pub id: String,
    /// 哪个目录（`antigravity` / `antigravity-ide` / …）。
    pub dir: String,
    pub title: Option<String>,
    /// 最后写入（库和它的 `-wal` 取晚的那个），RFC3339。
    pub updated_at: String,
    /// 库加 `-wal` 的大小。
    #[ts(type = "number")]
    pub size: u64,
    /// 旧格式 `.pb` 归档。
    pub legacy: bool,
}

/// 标题按（路径、修改时刻）记着：几百个库，每次开弹窗都打开一遍不划算。
type TitleCache = HashMap<PathBuf, (SystemTime, Option<String>)>;
static TITLES: Mutex<Option<TitleCache>> = Mutex::new(None);

fn read_title(db: &Path) -> Option<String> {
    let conn = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let payload: Vec<u8> = conn
        .query_row(
            "SELECT step_payload FROM steps WHERE step_type = 14 AND step_payload IS NOT NULL ORDER BY idx LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok()?;
    let text = Message::parse(&payload).ok()?.sub(19)?.str(2)?.to_string();
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }
    Some(if flat.chars().count() > 120 {
        flat.chars().take(120).collect::<String>() + "…"
    } else {
        flat
    })
}

fn title_of(db: &Path, modified: SystemTime) -> Option<String> {
    if let Some(t) = TITLES.lock().ok().and_then(|c| {
        c.as_ref()?
            .get(db)
            .filter(|(m, _)| *m == modified)
            .map(|x| x.1.clone())
    }) {
        return t;
    }
    let title = read_title(db);
    if let Ok(mut c) = TITLES.lock() {
        c.get_or_insert_with(HashMap::new)
            .insert(db.to_path_buf(), (modified, title.clone()));
    }
    title
}

/// `home` 下面全部对话，新的在前。同一个 id 在备份目录里再出现一次只列第一份（同用量那边）。
pub fn list_under(home: &Path) -> Vec<AntigravityConversation> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for dir in conversation_dirs(home) {
        let dir_name = dir
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            let legacy = match p.extension().and_then(|e| e.to_str()) {
                Some("db") => false,
                Some("pb") => true,
                _ => continue,
            };
            let Some(id) = p.file_stem().map(|s| s.to_string_lossy().to_string()) else {
                continue;
            };
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(meta) = std::fs::metadata(&p) else {
                continue;
            };
            let mut size = meta.len();
            let mut modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            if let Ok(wal) = std::fs::metadata(p.with_extension("db-wal")) {
                size += wal.len();
                if let Ok(m) = wal.modified() {
                    modified = modified.max(m);
                }
            }
            out.push(AntigravityConversation {
                title: if legacy { None } else { title_of(&p, modified) },
                id,
                dir: dir_name.clone(),
                updated_at: chrono::DateTime::<chrono::Utc>::from(modified).to_rfc3339(),
                size,
                legacy,
            });
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

pub fn list() -> crate::error::Result<Vec<AntigravityConversation>> {
    let home =
        dirs::home_dir().ok_or_else(|| crate::error::GateError::Other("找不到用户目录".into()))?;
    Ok(list_under(&home))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 长度前缀字段：tag、长度、内容。
    fn field(n: u32, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut v = ((n << 3) | 2) as u64;
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                break;
            }
            out.push(b | 0x80);
        }
        assert!(body.len() < 128);
        out.push(body.len() as u8);
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn titles_come_from_the_first_user_step_and_missing_ones_stay_empty() {
        let root = std::env::temp_dir().join(format!("qb-ag-conv-{}", crate::config_io::id()));
        let conv = root.join(".gemini/antigravity/conversations");
        let backup = root.join(".gemini/antigravity-backup/conversations");
        std::fs::create_dir_all(&conv).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        let make = |path: &Path, payload: Option<Vec<u8>>| {
            let c = Connection::open(path).unwrap();
            c.execute_batch(
                "CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER, step_payload BLOB);",
            )
            .unwrap();
            if let Some(p) = payload {
                c.execute("INSERT INTO steps VALUES (0, 15, x'00')", [])
                    .unwrap();
                c.execute("INSERT INTO steps VALUES (1, 14, ?1)", [p])
                    .unwrap();
            }
        };
        let hi = field(19, &field(2, "  帮我\n看看日志 ".as_bytes()));
        make(&conv.join("a.db"), Some(hi));
        make(&conv.join("b.db"), None);
        make(&backup.join("a.db"), None);
        std::fs::write(conv.join("old.pb"), b"x").unwrap();
        let mut got: Vec<(String, Option<String>, bool)> = list_under(&root)
            .into_iter()
            .map(|c| (c.id, c.title, c.legacy))
            .collect();
        got.sort();
        assert_eq!(
            got,
            [
                ("a".into(), Some("帮我 看看日志".into()), false),
                ("b".into(), None, false),
                ("old".into(), None, true),
            ],
            "备份目录里同名的只列一份"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
