// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! Codex 会话归桶（中转站重构阶段 4，cc-switch 的 C5「Codex 历史统一」）。
//!
//! Codex 按 `model_provider` 给会话分桶：`codex resume` 和桌面端的会话列表只列**当前那一家**的会话。
//! 用过别的切换工具（cc-switch 的 `ccswitch`、各家自定义 id）、或者换过 provider 名，老会话就「不见了」——
//! 其实都在，只是挂在别的桶里。这里把使用者**逐个勾选**的桶并进当前那个桶：
//!
//! - 会话文件（`sessions\`、`archived_sessions\` 下的 `rollout-*.jsonl`）里 `session_meta` 那一行的
//!   `payload.model_provider` 改掉（cc-switch 的 `rewrite_codex_session_meta_line`）；
//! - 状态库（`state_5.sqlite`，`config.toml` 里设了 `sqlite_home` 就连那边一份）的 `threads.model_provider` 改掉。
//!
//! ⛔ 动之前先备份：改过的每个文件原样复制一份、状态库用 SQLite 的备份接口整份拷一份，放在面板自己的目录里；
//! 账本记下每个文件、每一行原来在哪个桶。**还原按账本把那一行改回去**，不拿旧文件盖回去 —— 并桶之后
//! Codex 往同一个会话里接着写过的内容不能因为还原而丢。
//!
//! 默认只列出中转环境那一份；官方 `~\.codex` 和官方槽位也列出来，但每个桶都要使用者自己勾、再确认一次才动。
//! 压缩过的会话（`.jsonl.zst`，Codex 把 7 天没动的压起来）**不动**，只报个数 —— 在 Codex 里打开一次它就解压回来了。

use crate::domain::Client;
use crate::error::{GateError, Result};
use crate::repository::Repository;
use rusqlite::{params_from_iter, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use ts_rs::TS;

/// 没写 `model_provider` 时 Codex 用的那个桶（官方登录、OpenAI API Key 的会话都在这里）。
const OPENAI: &str = "openai";
const STATE_DB: &str = "state_5.sqlite";
const ID_CHUNK: usize = 500;

/// 一个桶。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CodexBucket {
    pub provider: String,
    /// 会话文件个数（没压缩的）。
    pub sessions: u32,
    /// 状态库里的行数。
    pub threads: u32,
}

/// 一次并桶留下的备份。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CodexHistoryBackup {
    pub id: String,
    #[ts(type = "number")]
    pub at_ms: i64,
    pub from: Vec<String>,
    pub to: String,
    pub sessions: u32,
    pub threads: u32,
    /// 已经还原过了（还原只做一次）。
    pub restored: bool,
}

/// 一个 Codex 目录（`CODEX_HOME`）里的会话分桶情况。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CodexHistoryHome {
    /// `relay` / `default` / `slot:<槽位 id>`。
    pub id: String,
    pub label: String,
    pub path: String,
    /// 官方那一侧。
    pub official: bool,
    pub exists: bool,
    /// 现在 Codex 列哪个桶（`config.toml` 的 `model_provider`，没写就是 `openai`）。
    pub current: String,
    /// 除了当前那个以外的桶，会话多的在前。
    pub buckets: Vec<CodexBucket>,
    /// 压缩过、这次不动的会话文件个数。
    pub compressed: u32,
    pub backups: Vec<CodexHistoryBackup>,
    pub problem: Option<String>,
}

/// 账本：还原时照它把每一处改回去。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Ledger {
    home: String,
    at_ms: i64,
    from: Vec<String>,
    to: String,
    /// (相对 home 的路径, 原来的桶)
    files: Vec<(String, String)>,
    /// (状态库路径, 线程 id, 原来的桶)
    threads: Vec<(String, String, String)>,
    #[serde(default)]
    restored: bool,
}

/// 在哪找、备份放哪。平时是真目录；单测换成临时目录（单测不碰真实状态）。
struct Place {
    homes: Vec<(String, String, PathBuf, bool)>,
    backups: PathBuf,
}

impl Place {
    fn real(db: &Repository) -> Result<Self> {
        let mut homes = vec![(
            "relay".to_string(),
            "中转环境（Codex · 走本机路由）".to_string(),
            crate::config_io::environment_dir(
                &db.root,
                &crate::workspace::router_environment_id(Client::Codex),
            )?,
            false,
        )];
        if let Some(home) = dirs::home_dir() {
            homes.push((
                "default".into(),
                "官方 · 默认目录".into(),
                home.join(".codex"),
                true,
            ));
        }
        let root = qb_accounts::codex::root();
        if let Ok(list) = qb_accounts::codex::list(&root) {
            for s in list.slots {
                if let Ok(dir) = qb_accounts::codex::directory(&root, &s.id) {
                    let (home, _) = crate::workspace::codex_slot_dirs(&dir);
                    homes.push((
                        format!("slot:{}", s.id),
                        format!("官方 · 槽位 {}", s.label),
                        home,
                        true,
                    ));
                }
            }
        }
        Ok(Place {
            homes,
            backups: crate::paths::state_dir()
                .join("backups")
                .join("codex-history"),
        })
    }

    fn home(&self, id: &str) -> Result<(String, PathBuf)> {
        self.homes
            .iter()
            .find(|(i, ..)| i == id)
            .map(|(_, label, path, _)| (label.clone(), path.clone()))
            .ok_or_else(|| GateError::Other(format!("不认得这个 Codex 目录：{id}")))
    }
}

// ------------------------------------------------------------------ 读

fn config_text(home: &Path) -> String {
    std::fs::read_to_string(home.join("config.toml")).unwrap_or_default()
}

/// 现在 Codex 列哪个桶。
fn current_bucket(config: &str) -> String {
    config
        .parse::<toml_edit::DocumentMut>()
        .ok()
        .and_then(|d| d.get("model_provider")?.as_str().map(str::to_string))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| OPENAI.to_string())
}

fn state_dbs(home: &Path, config: &str) -> Vec<PathBuf> {
    let mut out = vec![home.join(STATE_DB)];
    if let Some(dir) = config
        .parse::<toml_edit::DocumentMut>()
        .ok()
        .and_then(|d| d.get("sqlite_home")?.as_str().map(PathBuf::from))
    {
        let p = dir.join(STATE_DB);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out.into_iter().filter(|p| p.is_file()).collect()
}

/// (没压缩的会话文件, 压缩过的个数)
fn rollouts(home: &Path) -> (Vec<PathBuf>, u32) {
    fn walk(dir: &Path, depth: u8, max: u8, plain: &mut Vec<PathBuf>, zst: &mut u32) {
        if depth > max {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                walk(&p, depth + 1, max, plain, zst);
            } else if name.ends_with(".jsonl") {
                plain.push(p);
            } else if name.ends_with(".jsonl.zst") {
                *zst += 1;
            }
        }
    }
    let (mut plain, mut zst) = (Vec::new(), 0);
    walk(&home.join("sessions"), 0, 8, &mut plain, &mut zst);
    walk(&home.join("archived_sessions"), 0, 4, &mut plain, &mut zst);
    plain.sort();
    (plain, zst)
}

/// `session_meta` 那一行写着哪个桶；不是这种行就 `None`。
fn meta_bucket(line: &str) -> Option<String> {
    if !line.contains("\"session_meta\"") || !line.contains("\"model_provider\"") {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "session_meta" {
        return None;
    }
    v.get("payload")?
        .get("model_provider")?
        .as_str()
        .map(str::to_string)
}

/// 把 `session_meta` 那一行的桶从 `from` 换成 `to`；不用换就 `None`。
fn rewrite_meta(line: &str, from: &dyn Fn(&str) -> bool, to: &str) -> Option<String> {
    let current = meta_bucket(line)?;
    if !from(&current) {
        return None;
    }
    let mut v: Value = serde_json::from_str(line).ok()?;
    v.get_mut("payload")?
        .as_object_mut()?
        .insert("model_provider".into(), Value::String(to.into()));
    serde_json::to_string(&v).ok()
}

/// 会话文件挂在哪个桶（看第一处 `session_meta`）。
fn file_bucket(path: &Path) -> Option<String> {
    use std::io::BufRead;
    let f = std::fs::File::open(path).ok()?;
    std::io::BufReader::new(f)
        .lines()
        .take(8)
        .map_while(std::io::Result::ok)
        .find_map(|l| meta_bucket(&l))
}

fn open_db(path: &Path) -> Result<Option<Connection>> {
    let conn = Connection::open(path)
        .map_err(|e| GateError::Other(format!("打不开 Codex 状态库 {}：{e}", path.display())))?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| GateError::Other(e.to_string()))?;
    let has: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('threads') WHERE name = 'model_provider'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);
    Ok(has.then_some(conn))
}

fn db_buckets(path: &Path) -> Result<BTreeMap<String, u32>> {
    let mut out = BTreeMap::new();
    let Some(conn) = open_db(path)? else {
        return Ok(out);
    };
    let mut stmt = conn
        .prepare("SELECT model_provider, COUNT(*) FROM threads GROUP BY model_provider")
        .map_err(|e| GateError::Other(e.to_string()))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(|e| GateError::Other(e.to_string()))?;
    for row in rows.flatten() {
        let name = row.0.unwrap_or_default();
        *out.entry(name).or_default() += row.1 as u32;
    }
    Ok(out)
}

fn ledgers(place: &Place, home: &Path) -> Vec<(PathBuf, Ledger)> {
    let Ok(rd) = std::fs::read_dir(&place.backups) else {
        return Vec::new();
    };
    let key = home.display().to_string();
    let mut out: Vec<(PathBuf, Ledger)> = rd
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let l: Ledger =
                serde_json::from_slice(&std::fs::read(dir.join("ledger.json")).ok()?).ok()?;
            (l.home == key).then_some((dir, l))
        })
        .collect();
    out.sort_by_key(|(_, l)| std::cmp::Reverse(l.at_ms));
    out
}

fn scan(place: &Place, id: &str, label: &str, home: &Path, official: bool) -> CodexHistoryHome {
    let config = config_text(home);
    let current = current_bucket(&config);
    let (files, compressed) = rollouts(home);
    let mut buckets: BTreeMap<String, CodexBucket> = BTreeMap::new();
    fn entry<'a>(m: &'a mut BTreeMap<String, CodexBucket>, name: &str) -> &'a mut CodexBucket {
        m.entry(name.to_string()).or_insert(CodexBucket {
            provider: name.to_string(),
            sessions: 0,
            threads: 0,
        })
    }
    for f in &files {
        if let Some(b) = file_bucket(f) {
            entry(&mut buckets, &b).sessions += 1;
        }
    }
    let mut problem = None;
    for db in state_dbs(home, &config) {
        match db_buckets(&db) {
            Ok(m) => {
                for (b, n) in m {
                    entry(&mut buckets, &b).threads += n;
                }
            }
            Err(e) => problem = Some(e.to_string()),
        }
    }
    let mut buckets: Vec<CodexBucket> = buckets
        .into_values()
        .filter(|b| b.provider != current && !b.provider.is_empty())
        .collect();
    buckets.sort_by_key(|b| std::cmp::Reverse(b.sessions.max(b.threads)));
    CodexHistoryHome {
        id: id.into(),
        label: label.into(),
        path: home.display().to_string(),
        official,
        exists: home.is_dir(),
        current,
        buckets,
        compressed,
        backups: ledgers(place, home)
            .into_iter()
            .map(|(dir, l)| CodexHistoryBackup {
                id: dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                at_ms: l.at_ms,
                from: l.from,
                to: l.to,
                sessions: l.files.len() as u32,
                threads: l.threads.len() as u32,
                restored: l.restored,
            })
            .collect(),
        problem,
    }
}

/// 每个 Codex 目录里的会话分桶情况。只读。
pub fn status(db: &Repository) -> Result<Vec<CodexHistoryHome>> {
    status_at(&Place::real(db)?)
}

fn status_at(place: &Place) -> Result<Vec<CodexHistoryHome>> {
    Ok(place
        .homes
        .iter()
        .map(|(id, label, home, official)| scan(place, id, label, home, *official))
        .collect())
}

// ------------------------------------------------------------------ 写

/// 改一个会话文件里的每一处 `session_meta`。没改就返回 `false`。写前核对文件没在这期间被 Codex 动过，保留修改时间。
fn rewrite_file(
    path: &Path,
    rewrite: impl Fn(&str) -> Option<String>,
    backup: Option<&Path>,
) -> Result<bool> {
    let meta = std::fs::metadata(path)?;
    let (mtime, len) = (meta.modified().ok(), meta.len());
    let content = std::fs::read_to_string(path)?;
    let mut out = String::with_capacity(content.len());
    let mut changed = false;
    for seg in content.split_inclusive('\n') {
        let (line, nl) = seg
            .strip_suffix('\n')
            .map(|l| (l, "\n"))
            .unwrap_or((seg, ""));
        let (line, cr) = line
            .strip_suffix('\r')
            .map(|l| (l, "\r"))
            .unwrap_or((line, ""));
        match rewrite(line) {
            Some(next) => {
                out.push_str(&next);
                changed = true;
            }
            None => out.push_str(line),
        }
        out.push_str(cr);
        out.push_str(nl);
    }
    if !changed {
        return Ok(false);
    }
    let unchanged = |m: std::fs::Metadata| m.modified().ok() == mtime && m.len() == len;
    if !unchanged(std::fs::metadata(path)?) {
        return Err(GateError::Other(format!(
            "{} 在处理期间被改过（Codex 还开着？）：先关掉 Codex 再来",
            path.display()
        )));
    }
    if let Some(dst) = backup {
        if let Some(dir) = dst.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::copy(path, dst)?;
    }
    let tmp = path.with_extension("jsonl.qb-tmp");
    std::fs::write(&tmp, out.as_bytes())?;
    if let Some(t) = mtime {
        std::fs::File::options()
            .write(true)
            .open(&tmp)?
            .set_modified(t)?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(true)
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

/// 把勾选的那几个桶并进当前那个。先备份，账本写在备份目录里。
pub fn merge(db: &Repository, home_id: &str, from: Vec<String>) -> Result<Vec<CodexHistoryHome>> {
    let place = Place::real(db)?;
    let n = merge_at(
        &place,
        home_id,
        &from,
        chrono::Utc::now().timestamp_millis(),
    )?;
    crate::audit::write(&format!(
        "Codex 会话归桶：{home_id} 里 {} 并进当前那个（{n} 处），备份在 {}",
        from.join("、"),
        place.backups.display()
    ));
    status_at(&place)
}

fn merge_at(place: &Place, home_id: &str, from: &[String], now_ms: i64) -> Result<usize> {
    let (label, home) = place.home(home_id)?;
    if !home.is_dir() {
        return Err(GateError::Other(format!("{label} 的目录不在")));
    }
    let config = config_text(&home);
    let to = current_bucket(&config);
    let from: Vec<String> = from
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && *s != to)
        .collect();
    if from.is_empty() {
        return Err(GateError::Other("没勾要并进来的桶".into()));
    }
    let _lock = crate::relay_live::engine::lock_app("Codex");
    let dir = place
        .backups
        .join(format!("{now_ms}-{}", crate::config_io::id()));
    std::fs::create_dir_all(&dir)?;
    let mut ledger = Ledger {
        home: home.display().to_string(),
        at_ms: now_ms,
        from: from.clone(),
        to: to.clone(),
        files: Vec::new(),
        threads: Vec::new(),
        restored: false,
    };
    // 账本先落一份：中途失败也知道改到了哪。
    let save = |l: &Ledger| -> Result<()> {
        std::fs::write(dir.join("ledger.json"), serde_json::to_vec_pretty(l)?)?;
        Ok(())
    };
    save(&ledger)?;

    let pick = |b: &str| from.iter().any(|f| f == b);
    let (files, _) = rollouts(&home);
    for f in files {
        let Some(was) = file_bucket(&f).filter(|b| pick(b)) else {
            continue;
        };
        let rel = f.strip_prefix(&home).unwrap_or(&f).to_path_buf();
        if rewrite_file(
            &f,
            |l| rewrite_meta(l, &pick, &to),
            Some(&dir.join("files").join(&rel)),
        )? {
            ledger
                .files
                .push((rel.to_string_lossy().replace('\\', "/"), was));
            save(&ledger)?;
        }
    }

    for (i, path) in state_dbs(&home, &config).into_iter().enumerate() {
        let Some(mut conn) = open_db(&path)? else {
            continue;
        };
        let sql = format!(
            "SELECT id, model_provider FROM threads WHERE model_provider IN ({})",
            placeholders(from.len())
        );
        let rows: Vec<(String, String)> = conn
            .prepare(&sql)
            .and_then(|mut s| {
                s.query_map(params_from_iter(from.iter()), |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?
                .collect()
            })
            .map_err(|e| GateError::Other(e.to_string()))?;
        if rows.is_empty() {
            continue;
        }
        let copy = dir.join(format!("state-{i}.sqlite"));
        conn.backup(rusqlite::MAIN_DB, &copy, None)
            .map_err(|e| GateError::Other(format!("备份 Codex 状态库失败：{e}")))?;
        let tx = conn
            .transaction()
            .map_err(|e| GateError::Other(e.to_string()))?;
        let sql = format!(
            "UPDATE threads SET model_provider = ? WHERE model_provider IN ({})",
            placeholders(from.len())
        );
        let mut values = vec![to.clone()];
        values.extend(from.iter().cloned());
        tx.execute(&sql, params_from_iter(values.iter()))
            .map_err(|e| GateError::Other(e.to_string()))?;
        tx.commit().map_err(|e| GateError::Other(e.to_string()))?;
        let p = path.display().to_string();
        ledger
            .threads
            .extend(rows.into_iter().map(|(id, was)| (p.clone(), id, was)));
        save(&ledger)?;
    }
    Ok(ledger.files.len() + ledger.threads.len())
}

/// 按账本还原一次并桶：每一处改回原来的桶（并桶之后 Codex 接着写的内容留着）。
pub fn restore(db: &Repository, home_id: &str, backup_id: &str) -> Result<Vec<CodexHistoryHome>> {
    let place = Place::real(db)?;
    let n = restore_at(&place, home_id, backup_id)?;
    crate::audit::write(&format!(
        "Codex 会话归桶：还原了 {home_id} 的 {backup_id}（{n} 处）"
    ));
    status_at(&place)
}

fn restore_at(place: &Place, home_id: &str, backup_id: &str) -> Result<usize> {
    let (_, home) = place.home(home_id)?;
    let (dir, mut ledger) = ledgers(place, &home)
        .into_iter()
        .find(|(d, _)| d.file_name().is_some_and(|n| n == backup_id))
        .ok_or_else(|| GateError::Other("找不到这份备份".into()))?;
    if ledger.restored {
        return Err(GateError::Other("这份已经还原过了".into()));
    }
    let _lock = crate::relay_live::engine::lock_app("Codex");
    let mut n = 0;
    for (rel, was) in &ledger.files {
        let path = home.join(rel);
        if !path.is_file() {
            continue; // 使用者删掉的会话不复活。
        }
        let to = ledger.to.clone();
        if rewrite_file(&path, |l| rewrite_meta(l, &|b| b == to, was), None)? {
            n += 1;
        }
    }
    let mut by_db: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for (db, id, was) in &ledger.threads {
        by_db.entry(db).or_default().push((id, was));
    }
    for (db, rows) in by_db {
        let Some(mut conn) = open_db(Path::new(db))? else {
            continue;
        };
        let tx = conn
            .transaction()
            .map_err(|e| GateError::Other(e.to_string()))?;
        for chunk in rows.chunks(ID_CHUNK) {
            for (id, was) in chunk {
                n += tx
                    .execute(
                        "UPDATE threads SET model_provider = ?1 WHERE id = ?2 AND model_provider = ?3",
                        [*was, *id, ledger.to.as_str()],
                    )
                    .map_err(|e| GateError::Other(e.to_string()))?;
            }
        }
        tx.commit().map_err(|e| GateError::Other(e.to_string()))?;
    }
    ledger.restored = true;
    std::fs::write(dir.join("ledger.json"), serde_json::to_vec_pretty(&ledger)?)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(provider: &str) -> String {
        format!(
            "{{\"timestamp\":\"t\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"x\",\"model_provider\":\"{provider}\"}}}}\n{{\"type\":\"response_item\",\"payload\":{{\"text\":\"model_provider\"}}}}\n"
        )
    }

    #[test]
    fn merging_moves_only_the_ticked_buckets_and_restore_keeps_later_writes() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let home = root.join("codex");
        let day = home.join("sessions").join("2026").join("10").join("25");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(home.join("config.toml"), "model_provider = \"qb-gate\"\n").unwrap();
        std::fs::write(day.join("rollout-a.jsonl"), session("ccswitch")).unwrap();
        std::fs::write(day.join("rollout-b.jsonl"), session("other")).unwrap();
        std::fs::write(day.join("rollout-c.jsonl"), session("qb-gate")).unwrap();
        std::fs::write(day.join("rollout-d.jsonl.zst"), b"zstd").unwrap();
        let conn = Connection::open(home.join(STATE_DB)).unwrap();
        conn.execute_batch(
            "CREATE TABLE threads(id TEXT PRIMARY KEY, model_provider TEXT);
             INSERT INTO threads VALUES('a','ccswitch'),('b','other'),('c','qb-gate');",
        )
        .unwrap();
        drop(conn);
        let place = Place {
            homes: vec![("relay".into(), "中转".into(), home.clone(), false)],
            backups: root.join("backups"),
        };

        let before = status_at(&place).unwrap().remove(0);
        assert_eq!(before.current, "qb-gate");
        assert_eq!(before.compressed, 1, "压缩过的只报个数");
        let names: Vec<&str> = before.buckets.iter().map(|b| b.provider.as_str()).collect();
        assert!(
            names.contains(&"ccswitch") && names.contains(&"other") && !names.contains(&"qb-gate")
        );

        let n = merge_at(&place, "relay", &["ccswitch".into()], 1).unwrap();
        assert_eq!(n, 2, "一个文件 + 一行");
        let a = std::fs::read_to_string(day.join("rollout-a.jsonl")).unwrap();
        assert!(a.contains("\"model_provider\":\"qb-gate\""), "{a}");
        assert!(a.contains("\"text\":\"model_provider\""), "别的行不动");
        let b = std::fs::read_to_string(day.join("rollout-b.jsonl")).unwrap();
        assert!(b.contains("\"other\""), "没勾的桶不动");
        let after = status_at(&place).unwrap().remove(0);
        assert!(!after.buckets.iter().any(|b| b.provider == "ccswitch"));
        assert_eq!(after.backups.len(), 1);
        let backup = after.backups[0].id.clone();
        assert!(
            place.backups.join(&backup).join("state-0.sqlite").is_file(),
            "状态库先备份"
        );
        assert!(
            place
                .backups
                .join(&backup)
                .join("files/sessions/2026/10/25/rollout-a.jsonl")
                .is_file(),
            "会话文件先备份"
        );

        // 并桶之后 Codex 接着往会话里写了一句：还原不能把它丢了。
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(day.join("rollout-a.jsonl"))
            .unwrap();
        std::io::Write::write_all(
            &mut f,
            b"{\"type\":\"event_msg\",\"payload\":{\"later\":true}}\n",
        )
        .unwrap();
        drop(f);
        restore_at(&place, "relay", &backup).unwrap();
        let a = std::fs::read_to_string(day.join("rollout-a.jsonl")).unwrap();
        assert!(
            a.contains("\"model_provider\":\"ccswitch\"") && a.contains("\"later\":true"),
            "{a}"
        );
        let conn = Connection::open(home.join(STATE_DB)).unwrap();
        let p: String = conn
            .query_row("SELECT model_provider FROM threads WHERE id='a'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(p, "ccswitch");
        drop(conn);
        assert!(restore_at(&place, "relay", &backup).is_err(), "只还原一次");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_current_bucket_defaults_to_openai() {
        assert_eq!(current_bucket(""), "openai");
        assert_eq!(current_bucket("model = \"x\"\n"), "openai");
        assert_eq!(
            current_bucket("model_provider = \"qb\"\n[x]\nmodel_provider = \"y\"\n"),
            "qb"
        );
    }
}
