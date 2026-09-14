//! Checked, recoverable edits to external configuration files. SQLite cannot transact them.
use crate::error::{GateError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub fn id() -> String {
    format!(
        "{}-{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S"),
        uuid::Uuid::new_v4().simple()
    )
}

pub fn ensure_plain_path(path: &Path) -> Result<()> {
    for p in path.ancestors() {
        let meta = match std::fs::symlink_metadata(p) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err(GateError::Other(format!(
                    "隔离配置路径经过联结点或重解析点：{}",
                    p.display()
                )));
            }
        }
        if meta.file_type().is_symlink() {
            return Err(GateError::Other("隔离配置不能使用符号链接".into()));
        }
    }
    Ok(())
}

pub fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(GateError::Other(format!(
            "无法读取 {}：{e}",
            path.display()
        ))),
    }
}

pub fn read_text(path: &Path) -> Result<String> {
    String::from_utf8(read_optional(path)?.unwrap_or_default())
        .map_err(|e| GateError::Other(format!("{} 不是有效 UTF-8：{e}", path.display())))
}

pub fn parse_object(bytes: Option<&[u8]>) -> Result<serde_json::Value> {
    let Some(bytes) = bytes else {
        return Ok(serde_json::json!({}));
    };
    let v: serde_json::Value =
        serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))?;
    if !v.is_object() {
        return Err(GateError::Other("配置必须是 JSON 对象".into()));
    }
    Ok(v)
}
pub fn read_object(path: &Path) -> Result<serde_json::Value> {
    parse_object(read_optional(path)?.as_deref())
}
pub fn fingerprint(edits: &[Edit], revision: u32) -> String {
    let mut hash = Sha256::new();
    hash.update(revision.to_le_bytes());
    for edit in edits {
        for bytes in [
            Some(edit.path.to_string_lossy().as_bytes()),
            edit.expected.as_deref(),
            edit.body.as_deref(),
        ] {
            hash.update([u8::from(bytes.is_some())]);
            if let Some(b) = bytes {
                hash.update((b.len() as u64).to_le_bytes());
                hash.update(b);
            }
        }
    }
    hex::encode(hash.finalize())
}
pub fn object(text: &str) -> Result<serde_json::Value> {
    let v = if text.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(text)?
    };
    if !v.is_object() {
        return Err(GateError::Other(
            "配置必须是 JSON 对象；原文件未改动".into(),
        ));
    }
    Ok(v)
}

fn digest(bytes: &Option<Vec<u8>>) -> Option<String> {
    bytes.as_ref().map(|b| hex::encode(Sha256::digest(b)))
}

/// expected is captured before parsing, so external edits are detected before replacement.
#[derive(Clone)]
pub struct Edit {
    pub path: PathBuf,
    pub expected: Option<Vec<u8>>,
    pub body: Option<Vec<u8>>,
}

impl Edit {
    pub fn text(path: PathBuf, body: String) -> Result<Self> {
        Ok(Self {
            expected: read_optional(&path)?,
            path,
            body: Some(body.into_bytes()),
        })
    }
}

#[derive(Serialize, Deserialize)]
struct SavedEdit {
    path: PathBuf,
    original: Option<String>,
    next_hash: Option<String>,
}

/// 两阶段提交的日志。
///
/// 原来是 `pub(crate)`：只有 `repository` 的测试要把 `status` 改成 `prepared`，
/// 好模拟「进程在 SQLite 提交完与文件收尾之间被打断」。拆 crate 之后
/// `repository` 在另一个 crate 里，`pub(crate)` 它就看不见了。
///
/// ⚠ 放宽到 `pub` 是有代价的：`pub` 在 `pub mod` 里逃出了 dead_code 分析，
/// 以后没人用了编译器也不会提醒。所以这里写清楚**它只有一个正当用途** ——
/// 模拟崩溃。别拿它当普通的读写接口，真正的入口是 `commit` / `commit_in`。
#[derive(Serialize, Deserialize)]
pub struct Journal {
    pub status: String,
    edits: Vec<SavedEdit>,
    #[serde(default)]
    database: Option<PathBuf>,
}

/// Sibling staging + rename keeps readers from observing a truncated file.
pub fn replace(path: &Path, body: Option<&[u8]>) -> Result<()> {
    if let Some(body) = body {
        let parent = path
            .parent()
            .ok_or_else(|| GateError::Other("配置路径没有父目录".into()))?;
        std::fs::create_dir_all(parent)?;
        let stage = parent.join(format!(".qb-{}.tmp", id()));
        let result = (|| -> Result<()> {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&stage)?;
            f.write_all(body)?;
            f.sync_all()?;
            drop(f);
            std::fs::rename(&stage, path)?;
            if std::fs::read(path)? != body {
                return Err(GateError::Other("写入后校验失败".into()));
            }
            Ok(())
        })();
        if stage.exists() {
            let _ = std::fs::remove_file(stage);
        }
        result
    } else {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

pub fn commit(edits: Vec<Edit>) -> Result<String> {
    commit_in(&crate::paths::state_dir().join("operations/files"), edits)
}

pub fn commit_in(root: &Path, edits: Vec<Edit>) -> Result<String> {
    commit_inner(root, edits, None, |_| Ok(()))
}

/// 两阶段提交的本体。`database` 不为空时，文件那半边要跟一次 SQLite 提交对齐。
///
/// 跨 crate 可见是因为 `qb-gate` 那边的 `repository::commit_database` 要用它 ——
/// 事务边界属于拥有数据库的那一层，文件这半边留在这里。普通调用方用
/// [`commit`] / [`commit_in`]，别直接碰这个。
pub fn commit_inner(
    root: &Path,
    edits: Vec<Edit>,
    database: Option<PathBuf>,
    change: impl FnOnce(&str) -> Result<()>,
) -> Result<String> {
    let mut seen = std::collections::BTreeSet::new();
    for e in &edits {
        if !seen.insert(e.path.clone()) {
            return Err(GateError::Other("同一事务不能重复写同一文件".into()));
        }
        if read_optional(&e.path)? != e.expected {
            return Err(GateError::Other(format!(
                "{} 已被外部修改，请重新预览",
                e.path.display()
            )));
        }
    }
    let mut journal = Journal {
        status: "prepared".into(),
        edits: Vec::new(),
        database,
    };
    for e in &edits {
        journal.edits.push(SavedEdit {
            path: e.path.clone(),
            // Prefix makes an existing empty file distinguishable from a missing secret.
            original: e
                .expected
                .as_ref()
                .map(|b| crate::secret::seal(&format!("hex:{}", hex::encode(b))))
                .transpose()?,
            next_hash: digest(&e.body),
        });
    }
    let operation = id();
    let path = root.join(format!("{operation}.json"));
    replace(&path, Some(&serde_json::to_vec_pretty(&journal)?))?;
    let result = (|| -> Result<()> {
        for e in &edits {
            if read_optional(&e.path)? != e.expected {
                return Err(GateError::Other(format!(
                    "{} 在提交期间被修改",
                    e.path.display()
                )));
            }
            replace(&e.path, e.body.as_deref())?;
        }
        change(&operation)
    })();
    if let Err(error) = result {
        return match rollback(&mut journal, &path) {
            Ok(()) => Err(GateError::Other(format!("{error}；已恢复事务前的配置"))),
            Err(recovery) => Err(GateError::Other(format!(
                "{error}；恢复未完成：{recovery}；记录 {operation}"
            ))),
        };
    }
    journal.status = "committed".into();
    if let Err(error) = replace(&path, Some(&serde_json::to_vec_pretty(&journal)?)) {
        if journal.database.is_none() {
            rollback(&mut journal, &path)?;
            return Err(error);
        }
        // The database marker has committed. A startup recovery can finish this bookkeeping.
    }
    Ok(operation)
}

/// Build the inverse of a verified committed write, without overriding external changes.
pub fn inverse(root: &Path, operation: &str) -> Result<Vec<Edit>> {
    if operation.is_empty()
        || !operation
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(GateError::Other("恢复记录 ID 无效".into()));
    }
    let j: Journal =
        serde_json::from_slice(&std::fs::read(root.join(format!("{operation}.json")))?)?;
    if j.status != "committed" {
        return Err(GateError::Other("该配置记录尚未完成提交".into()));
    }
    j.edits
        .iter()
        .map(|saved| {
            let now = read_optional(&saved.path)?;
            if digest(&now) != saved.next_hash {
                return Err(GateError::Other(format!(
                    "{} 已被外部修改，请先处理差异",
                    saved.path.display()
                )));
            }
            Ok(Edit {
                path: saved.path.clone(),
                expected: now,
                body: original(saved)?,
            })
        })
        .collect()
}
fn original(saved: &SavedEdit) -> Result<Option<Vec<u8>>> {
    saved
        .original
        .as_deref()
        .map(|sealed| {
            let plain = crate::secret::open(sealed)
                .ok_or_else(|| GateError::Other("恢复记录无法解密".into()))?;
            hex::decode(plain.strip_prefix("hex:").unwrap_or(&plain))
                .map_err(|e| GateError::Other(e.to_string()))
        })
        .transpose()
}

fn rollback(journal: &mut Journal, path: &Path) -> Result<()> {
    let mut failures = Vec::new();
    for saved in journal.edits.iter().rev() {
        let result = (|| -> Result<()> {
            let original = original(saved)?;
            let now = read_optional(&saved.path)?;
            if now == original {
                return Ok(());
            }
            if digest(&now) != saved.next_hash {
                return Err(GateError::Other(format!(
                    "{} 有外部修改，保留现场",
                    saved.path.display()
                )));
            }
            replace(&saved.path, original.as_deref())
        })();
        if let Err(e) = result {
            failures.push(e.to_string());
        }
    }
    if !failures.is_empty() {
        return Err(GateError::Other(failures.join("；")));
    }
    journal.status = "rolled_back".into();
    replace(path, Some(&serde_json::to_vec_pretty(journal)?))
}

/// 一次启动恢复的结果。
#[derive(Default)]
pub struct Recovery {
    /// 这一轮真的处理完的记录。
    pub recovered: Vec<String>,
    /// 处理不了的记录：文件路径 + 原因。**每一条都要原样显示给使用者。**
    pub blocked: Vec<(PathBuf, String)>,
}

/// 扫一遍恢复记录，把 `prepared` 的那些按数据库提交标记结清。
///
/// # 单条失败只登记，不中断
///
/// 这里曾经是 `serde_json::from_slice(...)?` 一路往上抛：抽屉里只要有一张
/// 条子读不了或回滚不了，`startup::initialize` 就失败，面板直接卡在
/// 「需要完成数据恢复」页，**所有功能都不让用**，而页面上两个按钮都救不了。
///
/// 触发路径一点都不刁钻：提交中途断电，使用者第二天自己手改了那个
/// `settings.json`，`rollback` 一看内容对不上就报「有外部修改，保留现场」——
/// 从此永久锁死，真正的解法是去 `%LOCALAPPDATA%` 里删掉那一张，
/// 而面板从头到尾没说过是哪一张。
///
/// 现在：坏的那条进 `blocked`，好的照常结清，路径和原因一起交给恢复页，
/// 使用者看得见、也能显式放弃（`quarantine`）。
pub fn recover(root: &Path) -> Result<Recovery> {
    let mut out = Recovery::default();
    if !root.exists() {
        return Ok(out);
    }
    for e in std::fs::read_dir(root)? {
        let p = e?.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        match recover_one(&p) {
            Ok(true) => out.recovered.push(p.display().to_string()),
            Ok(false) => {}
            Err(error) => out.blocked.push((p, error.to_string())),
        }
    }
    Ok(out)
}

/// 结清一条记录。`Ok(false)` = 这条本来就不需要处理。
fn recover_one(p: &Path) -> Result<bool> {
    let mut j: Journal = serde_json::from_slice(&std::fs::read(p)?)?;
    if j.status != "prepared" {
        return Ok(false);
    }
    let committed = if let Some(database) = &j.database {
        let conn = rusqlite::Connection::open_with_flags(
            database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|e| GateError::Other(e.to_string()))?;
        let id = p.file_stem().unwrap_or_default().to_string_lossy();
        use rusqlite::OptionalExtension;
        conn.query_row(
            "SELECT value FROM metadata WHERE key=?1",
            [format!("file-commit:{id}")],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| GateError::Other(e.to_string()))?
        .as_deref()
            == Some("committed")
    } else {
        false
    };
    if committed {
        j.status = "committed".into();
        replace(p, Some(&serde_json::to_vec_pretty(&j)?))?;
    } else {
        rollback(&mut j, p)?;
    }
    Ok(true)
}

/// 清掉过期的恢复记录。返回删掉的路径。
///
/// # 为什么要有保留期
///
/// 每一条 journal 里都封着「改之前那个文件的完整内容」（DPAPI 加密）。
/// 这既是体积问题 —— 每次 apply / 装扩展 / 开关会话内门禁都写一条，永不清理，
/// 而 `recover` 每次启动都要把它们全读一遍 —— 也是保留期问题：
/// 一份 API Key 的旧副本不该在硬盘上躺一辈子。
///
/// 三类一条都不删，缺一不可：
///
/// * `prepared` —— 还没结清，归 [`recover`] 管；
/// * `protected` 里的 —— 那是某个环境**当前**的回滚记录（`environment-undo:*`
///   指着它），删了「回滚配置」按钮就报文件不存在；
/// * 还没到保留期的。
///
/// 读不出来的也留着：宁可占地方，也不删掉可能还需要的记录。
pub fn retain(
    root: &Path,
    cutoff: std::time::SystemTime,
    protected: &std::collections::BTreeSet<String>,
) -> Result<Vec<String>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut removed = Vec::new();
    for e in std::fs::read_dir(root)? {
        let p = e?.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let stem = p
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if protected.contains(&stem) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else {
            continue;
        };
        let Ok(j) = serde_json::from_slice::<Journal>(&bytes) else {
            continue;
        };
        if j.status == "prepared" {
            continue;
        }
        let expired = std::fs::metadata(&p)
            .and_then(|m| m.modified())
            .is_ok_and(|m| m < cutoff);
        if expired {
            std::fs::remove_file(&p)?;
            removed.push(p.display().to_string());
        }
    }
    Ok(removed)
}

/// 把结不清的恢复记录挪进 `quarantine/`，**不删**。
///
/// 只有使用者在恢复页上明确点过才会走到这里。挪走等于**放弃这条记录的回滚能力**：
/// 它牵涉的配置文件会保持当前状态不动。所以界面上必须先把路径和原因原样列出来，
/// 再让他点 —— 这是「保留现场」和「别把人关在门外」之间唯一说得过去的折中。
pub fn quarantine(root: &Path, paths: &[PathBuf]) -> Result<Vec<String>> {
    let dir = root.join("quarantine").join(id());
    let mut moved = Vec::new();
    for p in paths {
        // 只接受这个目录下的 .json —— 不让前端传进来的任意路径决定搬什么。
        if p.parent() != Some(root) || p.extension().and_then(|s| s.to_str()) != Some("json") {
            return Err(GateError::Other(format!(
                "{} 不是本机的恢复记录，未移动",
                p.display()
            )));
        }
        if !p.is_file() {
            continue;
        }
        std::fs::create_dir_all(&dir)?;
        let name = p
            .file_name()
            .ok_or_else(|| GateError::Other("恢复记录没有文件名".into()))?;
        std::fs::rename(p, dir.join(name))?;
        moved.push(dir.join(name).display().to_string());
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> PathBuf {
        let p = std::env::temp_dir().join(format!("qb-config-test-{}", id()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn preview_binds_original_destination_new_content_and_revision() {
        let edit = Edit {
            path: PathBuf::from("original.json"),
            expected: Some(b"old".to_vec()),
            body: Some(b"new".to_vec()),
        };
        let baseline = fingerprint(std::slice::from_ref(&edit), 1);
        assert_ne!(baseline, fingerprint(std::slice::from_ref(&edit), 2));
        for changed in [
            Edit {
                path: PathBuf::from("elsewhere.json"),
                ..edit.clone()
            },
            Edit {
                expected: None,
                ..edit.clone()
            },
            Edit {
                body: Some(b"different".to_vec()),
                ..edit.clone()
            },
        ] {
            assert_ne!(baseline, fingerprint(&[changed], 1));
        }
        assert!(parse_object(Some(b" ")).is_err());
        assert!(parse_object(None).unwrap().is_object());
    }
    #[test]
    fn failure_after_file_writes_restores_empty_file_and_absence() {
        let p = fixture();
        let a = p.join("a");
        let b = p.join("b");
        std::fs::write(&a, "").unwrap();
        let edits = vec![
            Edit {
                path: a.clone(),
                expected: Some(vec![]),
                body: Some(b"new".to_vec()),
            },
            Edit {
                path: b.clone(),
                expected: None,
                body: Some(b"new".to_vec()),
            },
        ];
        let result = commit_inner(&p.join("journal"), edits, None, |_| {
            assert_eq!(std::fs::read(&a).unwrap(), b"new");
            assert!(b.exists());
            Err(GateError::Other("injected metadata failure".into()))
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read(a).unwrap(), b"");
        assert!(!b.exists());
        std::fs::remove_dir_all(p).unwrap();
    }
    /// 一张坏条子不许把整个启动恢复带下水。
    ///
    /// 回归的是一个能把面板变砖的行为：`recover` 原来是 `?` 一路上抛，
    /// 抽屉里只要有一条读不了，`startup::initialize` 就失败，面板卡在恢复页，
    /// 而页面上没有任何按钮能救 —— 使用者只能去 `%LOCALAPPDATA%` 里猜删哪个。
    #[test]
    fn one_unreadable_journal_neither_blocks_the_others_nor_is_deleted() {
        let p = fixture();
        let root = p.join("journal");
        let file = p.join("file");
        std::fs::write(&file, "before").unwrap();
        let op = commit_in(
            &root,
            vec![Edit::text(file.clone(), "after".into()).unwrap()],
        )
        .unwrap();
        // 把这条改回 prepared：没有数据库标记，恢复时应当回滚。
        let path = root.join(format!("{op}.json"));
        let mut journal: Journal = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        journal.status = "prepared".into();
        replace(&path, Some(&serde_json::to_vec(&journal).unwrap())).unwrap();
        // 旁边再放一条读不了的。
        let broken = root.join("broken.json");
        replace(&broken, Some(b"{ not json")).unwrap();

        let out = recover(&root).unwrap();
        // 好的那条照常结清了。
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "before");
        assert_eq!(out.recovered.len(), 1);
        // 坏的那条登记在案，原文件仍然留在原地（保留现场，不自动删）。
        assert_eq!(out.blocked.len(), 1);
        assert_eq!(out.blocked[0].0, broken);
        assert!(broken.is_file());

        // 使用者显式放弃：挪进 quarantine，不是删掉。
        let moved = quarantine(&root, &[broken.clone()]).unwrap();
        assert!(!broken.exists());
        assert_eq!(moved.len(), 1);
        assert!(std::fs::read(&moved[0]).unwrap().starts_with(b"{ not json"));
        assert!(recover(&root).unwrap().blocked.is_empty());
        // 目录外的路径一律拒绝 —— 不让传进来的路径决定搬什么。
        assert!(quarantine(&root, &[p.join("file")]).is_err());
        assert!(file.is_file());
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn external_changes_are_not_overwritten() {
        let p = fixture();
        let a = p.join("a");
        let edit = Edit {
            path: a.clone(),
            expected: None,
            body: Some(b"ours".to_vec()),
        };
        std::fs::write(&a, "theirs").unwrap();
        assert!(commit_in(&p.join("journal"), vec![edit]).is_err());
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "theirs");
        std::fs::remove_dir_all(p).unwrap();
    }
    /// 保留期不许碰三类记录：还没结清的、还被环境回滚指着的、没到期的。
    ///
    /// 中间那条最要紧：`environment-undo:<id>` 指着的那一条是这个环境**当前**
    /// 的回滚记录。按保留期删掉的话，「回滚配置」按钮会报文件不存在，
    /// 而使用者完全看不出是保留期干的。
    #[test]
    fn retention_spares_unsettled_referenced_and_fresh_records() {
        let p = fixture();
        let root = p.join("journal");
        let mut ops = Vec::new();
        for name in ["settled", "referenced", "unsettled"] {
            let file = p.join(name);
            ops.push(commit_in(&root, vec![Edit::text(file, "x".into()).unwrap()]).unwrap());
        }
        // 第三条改回 prepared —— 归 recover 管，保留期不许碰。
        let path = root.join(format!("{}.json", ops[2]));
        let mut journal: Journal = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        journal.status = "prepared".into();
        replace(&path, Some(&serde_json::to_vec(&journal).unwrap())).unwrap();
        let protected = std::collections::BTreeSet::from([ops[1].clone()]);

        // 还没到期：一条都不删。
        let fresh = std::time::SystemTime::UNIX_EPOCH;
        assert!(retain(&root, fresh, &protected).unwrap().is_empty());

        // 全部到期：只有第一条该走。
        let expired = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        let removed = retain(&root, expired, &protected).unwrap();
        assert_eq!(removed.len(), 1);
        assert!(removed[0].contains(&ops[0]));
        assert!(!root.join(format!("{}.json", ops[0])).exists());
        assert!(root.join(format!("{}.json", ops[1])).is_file());
        assert!(root.join(format!("{}.json", ops[2])).is_file());
        // 被指着的那条还能反推出回滚变更 —— 保留期没把它废掉。
        assert!(inverse(&root, &ops[1]).is_ok());
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn malformed_objects_are_rejected() {
        assert!(object("{").is_err());
        assert!(object("[]").is_err());
        assert!(object("{}").is_ok());
    }
}

/// 隔离配置目录的路径。
///
/// 原来在 `workspace::environment_dir`，但 `gate::hook` 与 `extensions` 都要用它 ——
/// 两个模块为了拼一个路径而依赖了 `workspace`，`gate ↔ workspace` 与
/// `extensions ↔ workspace` 两对循环依赖就是这么来的。
///
/// 放在这里而不是 `paths` 里：它要走一遍 [`ensure_plain_path`] 实际读盘检查
/// 联结点与符号链接，而 `paths` 那层写死了「只拼路径、不读盘」。
pub fn environment_dir(root: &Path, id: &str) -> Result<PathBuf> {
    if id.is_empty() || id.len() > 80 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(GateError::Other("环境 ID 无效".into()));
    }
    let dir = root.join("environments").join(id);
    crate::config_io::ensure_plain_path(&dir)?;
    Ok(dir)
}
