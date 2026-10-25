// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 写入引擎：读字节 → 按补丁在内存里算出新内容（[`plan`]）→ 交给 QB Gate 自己的两阶段提交
//! （[`Planned::into_edit`] → `config_io::commit`）。
//!
//! cc-switch 的这个文件还管临时文件（`stage`）、首写备份（`ensure_first_write_backup`）和设备状态目录
//! （`DeviceStore`）。QB Gate 不搬那几样：`config_io::commit` 已经在做同样的事，而且做得更多 ——
//! 写前核对字节没被外部改过、事务日志里留着加密的原内容（就是备份，回滚和崩溃恢复都靠它）、
//! 换上之后再读一遍核对。两套并存的话，同一个文件会有两份「原内容」，恢复时不知道信哪份。
//!
//! 冲突检测是乐观的：QB Gate 自己的写入方共用 [`lock_app`] 这把锁，彼此不会覆盖；客户端自己这类外部进程
//! 不参与这把锁，计划时读到的字节就是 `Edit::expected`，提交前文件变了 `config_io::commit` 会拒绝。

use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};

use sha2::{Digest, Sha256};

use super::patch::{LivePatch, LiveWriteError};

/// 一个受引擎管理的客户端文件。`private` 为真表示文件里有 Key（cc-switch 按 0600 写；Windows 上没有这一位，
/// QB Gate 的环境目录在 `%LOCALAPPDATA%` 下、本来就只有本人能读，这里只留着记号给以后的平台用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveFile {
    pub path: PathBuf,
    pub private: bool,
}

impl LiveFile {
    pub fn private(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            private: true,
        }
    }

    pub fn shared(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            private: false,
        }
    }
}

/// 文件内容的 hash；文件不存在是 `None`。
pub fn digest(bytes: Option<&[u8]>) -> Option<String> {
    bytes.map(sha256_hex)
}

/// 十六进制的 SHA-256。
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 读当前字节；文件不存在返回 `None`，其他读取错误照报。
pub fn read_current(path: &Path) -> Result<Option<Vec<u8>>, LiveWriteError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(LiveWriteError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// 在内存里算好的一次写入。
#[derive(Debug, Clone)]
pub struct Planned {
    pub file: LiveFile,
    /// 写前内容的 hash，`None` 表示文件不存在。
    pub pre: Option<String>,
    pre_bytes: Option<Vec<u8>>,
    /// 写后内容的 hash；`None` 表示删掉这个文件。
    pub planned: Option<String>,
    bytes: Option<Vec<u8>>,
}

impl Planned {
    pub fn is_noop(&self) -> bool {
        self.pre == self.planned
    }

    pub fn pre_bytes(&self) -> Option<&[u8]> {
        self.pre_bytes.as_deref()
    }

    /// 写后的字节；`None` 表示删掉这个文件。
    pub fn bytes(&self) -> Option<&[u8]> {
        self.bytes.as_deref()
    }

    /// 换成 `config_io` 的一条改动：计划时读到的字节就是「提交前文件必须还是这样」的那份。
    pub fn into_edit(self) -> crate::config_io::Edit {
        crate::config_io::Edit {
            path: self.file.path,
            expected: self.pre_bytes,
            body: self.bytes,
        }
    }
}

/// 读当前内容并在内存里算出新内容；什么都不写。
pub fn plan(file: &LiveFile, patch: &dyn LivePatch) -> Result<Planned, LiveWriteError> {
    let pre_bytes = read_current(&file.path)?;
    plan_from(file, patch, pre_bytes)
}

/// 用调用方已经读到的字节算（同一个文件要先读来判断别的事时，免得读两次、两次之间被改）。
pub fn plan_from(
    file: &LiveFile,
    patch: &dyn LivePatch,
    pre_bytes: Option<Vec<u8>>,
) -> Result<Planned, LiveWriteError> {
    let bytes = patch.apply_file(&file.path, pre_bytes.as_deref())?;
    Ok(Planned {
        file: file.clone(),
        pre: digest(pre_bytes.as_deref()),
        pre_bytes,
        planned: digest(bytes.as_deref()),
        bytes,
    })
}

/// 一个客户端的写锁：QB Gate 里改这个客户端配置文件的所有写入方都要先拿到它。不可重入。
#[derive(Debug)]
pub struct AppWriteGuard {
    app: String,
}

impl AppWriteGuard {
    pub fn app(&self) -> &str {
        &self.app
    }
}

fn lock_table() -> &'static (Mutex<HashSet<String>>, Condvar) {
    static LOCKS: OnceLock<(Mutex<HashSet<String>>, Condvar)> = OnceLock::new();
    LOCKS.get_or_init(|| (Mutex::new(HashSet::new()), Condvar::new()))
}

pub fn lock_app(app: &str) -> AppWriteGuard {
    let (held, released) = lock_table();
    let mut held = held.lock().unwrap_or_else(|e| e.into_inner());
    while held.contains(app) {
        held = released.wait(held).unwrap_or_else(|e| e.into_inner());
    }
    held.insert(app.to_string());
    AppWriteGuard {
        app: app.to_string(),
    }
}

impl Drop for AppWriteGuard {
    fn drop(&mut self) {
        let (held, released) = lock_table();
        held.lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.app);
        released.notify_all();
    }
}

#[cfg(test)]
mod tests {
    //! 搬自 cc-switch 的同名测试；`stage` / 首写备份那两条没搬（那两样交给 `config_io::commit`，
    //! 它有自己的测试）。临时目录照 QB Gate 的写法用 `temp_dir` + 随机名（workspace 里没有 `tempfile`）。
    use super::*;
    use crate::live::patch::json::JsonPatch;
    use crate::live::patch::KeyPath;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn set_patch(key: &str, value: &str) -> JsonPatch {
        JsonPatch {
            set: vec![(KeyPath::new(&[key]), json!(value))],
            ..JsonPatch::default()
        }
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(crate::config_io::id());
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn plan_writes_nothing_and_refuses_broken_files() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        fs::write(&path, "{ broken").unwrap();
        let before = fs::metadata(&path).unwrap().modified().unwrap();

        let err = plan(&LiveFile::shared(&path), &set_patch("a", "b")).expect_err("refused");
        assert!(matches!(err, LiveWriteError::Parse { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ broken");
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "no temp files");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn plan_reports_noops() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        fs::write(&path, "{\n  \"a\": \"b\"\n}").unwrap();
        let planned = plan(&LiveFile::shared(&path), &set_patch("a", "b")).unwrap();
        assert!(planned.is_noop());
        let planned = plan(&LiveFile::shared(&path), &set_patch("a", "c")).unwrap();
        assert!(!planned.is_noop());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_plan_becomes_an_edit_that_insists_on_the_bytes_it_read() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        fs::write(&path, "{\"a\":\"b\"}").unwrap();
        let edit = plan(&LiveFile::shared(&path), &set_patch("a", "c"))
            .unwrap()
            .into_edit();
        assert_eq!(edit.expected.as_deref(), Some(&b"{\"a\":\"b\"}"[..]));
        // 计划之后文件被别的程序改了：提交被拒，文件保持别人改的那样。
        fs::write(&path, "{\"a\":\"someone else\"}").unwrap();
        let journal = dir.join("journal");
        assert!(crate::config_io::commit_in(&journal, vec![edit]).is_err());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\"a\":\"someone else\"}"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn app_lock_serializes_writers_of_the_same_app() {
        let guard = lock_app("lock-test-app");
        let entered = Arc::new(AtomicBool::new(false));
        let flag = entered.clone();
        let waiter = std::thread::spawn(move || {
            let _other = lock_app("lock-test-app");
            flag.store(true, Ordering::SeqCst);
        });
        let _unrelated = lock_app("lock-test-other-app");
        std::thread::sleep(Duration::from_millis(50));
        assert!(!entered.load(Ordering::SeqCst), "second writer must wait");
        drop(guard);
        waiter.join().unwrap();
        assert!(entered.load(Ordering::SeqCst));
    }
}
