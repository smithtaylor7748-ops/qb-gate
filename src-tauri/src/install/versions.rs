//! 版本库与一键回滚。
//!
//! # 在这之前是什么样
//!
//! `managed::place` 把旧的那份改名成 `claude.exe.old.<时间戳>` 留在原地，
//! 下次安装时 `clean_old_backups` 一把删掉。于是：
//!
//! - 只留得住**一个**旧版本；
//! - 那个名字里没有版本号，你不知道留下来的是哪一版；
//! - 装第二次，上一版就永远没了 —— 新版本有问题时退不回去。
//!
//! 升级出事是个真实场景（`upgrade.rs` 里那条「写死 stable 会降级」的拦截
//! 就是撞出来的）。能往前走却退不回来，等于每次升级都是单程票。
//!
//! # 现在的样子
//!
//! ```text
//! <托管根>\claude-code\
//!   claude.exe                      ← 当前在用的
//!   versions\
//!     2.0.14\claude.exe + record.json
//!     2.0.11\claude.exe + record.json
//!     2.0.9\ claude.exe + record.json   ← 超过 KEEP 就从最老的开始删
//! ```
//!
//! 形态取自 cac（MIT）的版本管理；实现是自己写的（那边是 Shell）。
//!
//! # 两条不能破的
//!
//! 1. **版本库里的每一份都要上锁。** 它们是完整可执行的 `claude.exe`，
//!    漏一个就是现成的绕过入口（硬约束 1）。删之前也要先解锁，
//!    带着 Deny ACE 的文件删不掉。
//! 2. **`inventory.rs` 必须认识这个位置。** 「Claude 装在哪」全项目只有一张表，
//!    在别处再拼一份路径迟早对不上（硬约束 10）。

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{GateError, Result};
use crate::install::managed::{App, Record};

/// 版本库里保留几份。再多了从最老的开始删。
///
/// 3 是拍脑袋但有道理的：够你退回「上一版」和「上上版」，
/// 而 Claude Code 一份 200 MB 上下，留十份就是 2 GB。
pub const KEEP: usize = 3;

pub fn versions_dir(app_dir: &Path) -> PathBuf {
    app_dir.join("versions")
}

pub fn version_dir(app_dir: &Path, version: &str) -> PathBuf {
    versions_dir(app_dir).join(version)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VersionEntry {
    pub version: String,
    pub path: PathBuf,
    pub sha256: String,
    pub archived_at: String,
    /// 这一份现在就是在用的那个版本。
    pub is_current: bool,
    pub locked: bool,
}

/// 版本号能不能当目录名。
///
/// 拿版本号直接拼路径，不挡一下就是目录穿越 —— `..\..\` 或者带盘符的
/// 「版本号」会让回滚写到托管目录外面去。版本号来自官方清单，但**来源可信
/// 不等于可以不校验**：清单是网上下来的。
pub fn safe_version(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        && v != "."
        && v != ".."
}

/// 列版本库。最新的在前（按归档时间倒序，时间读不出来就按版本名倒序）。
pub fn history(app_dir: &Path, app: App, current: Option<&str>) -> Vec<VersionEntry> {
    let Ok(rd) = std::fs::read_dir(versions_dir(app_dir)) else {
        return Vec::new();
    };
    let mut out: Vec<VersionEntry> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let version = e.file_name().to_string_lossy().to_string();
            if !safe_version(&version) {
                return None;
            }
            let exe = e.path().join(app.exe_name());
            if !exe.is_file() {
                return None;
            }
            let rec: Record = std::fs::read_to_string(e.path().join("record.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            Some(VersionEntry {
                is_current: current == Some(version.as_str()),
                locked: crate::gate::is_locked(&exe).unwrap_or(false),
                version,
                sha256: rec.sha256,
                archived_at: rec.installed_at,
                path: exe,
            })
        })
        .collect();
    out.sort_by(|a, b| b.archived_at.cmp(&a.archived_at).then(b.version.cmp(&a.version)));
    out
}

/// 超出 `keep` 的那些（最老的在后，所以取尾巴）。纯函数，可单测。
///
/// **当前在用的那一版永远不删** —— 它被算进 keep 名额里没关系，
/// 但不能因为它排在末尾就被清掉。
pub fn pick_to_prune(entries: &[VersionEntry], keep: usize) -> Vec<&VersionEntry> {
    entries
        .iter()
        .filter(|e| !e.is_current)
        .skip(keep)
        .collect()
}

/// 把一份 exe 收进版本库。`exe` 会被**移动**进去。
pub fn archive(app_dir: &Path, app: App, exe: &Path, rec: &Record) -> Result<PathBuf> {
    if !safe_version(&rec.version) {
        return Err(GateError::Other(format!(
            "版本号 {:?} 不能当目录名，这一份没收进版本库",
            rec.version
        )));
    }
    let dir = version_dir(app_dir, &rec.version);
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(app.exe_name());
    // 同一版本已经在库里就覆盖 —— 重装同一版时不该堆两份。
    let _ = unlock(&dest);
    let _ = std::fs::remove_file(&dest);
    std::fs::rename(exe, &dest)
        .map_err(|e| GateError::Other(format!("收进版本库失败（多半正在运行）：{e}")))?;
    std::fs::write(dir.join("record.json"), serde_json::to_string_pretty(rec)?)?;
    lock(&dest);
    Ok(dest)
}

/// 删掉超额的旧版本。返回人话日志。
pub fn prune(app_dir: &Path, app: App, current: Option<&str>, keep: usize) -> Vec<String> {
    let all = history(app_dir, app, current);
    let mut notes = Vec::new();
    for e in pick_to_prune(&all, keep) {
        // 先解锁再删 —— 带着 Deny ACE 的文件删不掉，
        // 而删不掉又不报错的话，版本库会一直涨。
        let _ = unlock(&e.path);
        let dir = e.path.parent().unwrap_or(&e.path);
        match std::fs::remove_dir_all(dir) {
            Ok(()) => notes.push(format!("版本库：清掉旧版本 {}", e.version)),
            Err(err) => notes.push(format!("版本库：{} 删不掉（{err}），留到下次", e.version)),
        }
    }
    notes
}

/// 回滚到某一版。
///
/// 顺序是**先把当前这份收进版本库，再把目标版本换上来** —— 反过来的话
/// 中间那一刻当前版本已经没了，换不上去就两头空。
pub fn rollback(app_dir: &Path, app: App, to: &str, current: Option<&Record>) -> Result<String> {
    if !safe_version(to) {
        return Err(GateError::Other(format!("版本号 {to:?} 不合法")));
    }
    let src = version_dir(app_dir, to).join(app.exe_name());
    if !src.is_file() {
        return Err(GateError::Other(format!("版本库里没有 {to}")));
    }
    let target = app_dir.join(app.exe_name());

    // 当前这份先收进库。收不进去（正在运行）就别往下走。
    if target.is_file() {
        let rec = current.cloned().unwrap_or_default();
        if safe_version(&rec.version) {
            archive(app_dir, app, &target, &rec)?;
        } else {
            // 记录丢了、版本号读不出来 —— 不猜一个名字塞进库里，
            // 直接拦下。硬盘上多一份来路不明的 exe 比回滚失败麻烦。
            return Err(GateError::Other(
                "读不出当前装的是哪一版，回滚会把它弄丢，已拦下。先重装一次再回滚。".into(),
            ));
        }
    }

    // 目标版本从库里拷出来（**拷不是移**：库里那份要留着，
    // 否则退回去之后就再也退不回来了）。
    let _ = unlock(&src);
    let copy_result = std::fs::copy(&src, &target);
    lock(&src); // 无论成败，库里那份的锁要还回去
    copy_result.map_err(|e| GateError::Other(format!("把 {to} 换上来失败：{e}")))?;

    crate::gate::log::write(&format!("已回滚 {} 到 {to}", app.key()));
    Ok(format!("已回滚到 {to}"))
}

#[cfg(windows)]
fn lock(p: &Path) {
    let _ = crate::gate::acl::current_user_sid().and_then(|sid| crate::gate::acl::lock(p, &sid));
}

#[cfg(windows)]
fn unlock(p: &Path) -> Result<()> {
    let sid = crate::gate::acl::current_user_sid()?;
    crate::gate::acl::unlock(p, &sid)
}

#[cfg(not(windows))]
fn lock(_p: &Path) {}

#[cfg(not(windows))]
fn unlock(_p: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(v: &str, at: &str, current: bool) -> VersionEntry {
        VersionEntry {
            version: v.into(),
            path: PathBuf::from(format!("C:\\x\\versions\\{v}\\claude.exe")),
            sha256: String::new(),
            archived_at: at.into(),
            is_current: current,
            locked: true,
        }
    }

    #[test]
    fn version_names_that_would_escape_the_directory_are_rejected() {
        // 版本号来自网上下来的清单。来源可信 ≠ 可以不校验。
        for bad in ["..", ".", "", "../../windows", "C:\\evil", "a/b", "a\\b", "x y"] {
            assert!(!safe_version(bad), "{bad:?} 不该通过");
        }
        for ok in ["2.0.14", "0.28.0", "1.0.0-rc.1", "v2_1"] {
            assert!(safe_version(ok), "{ok:?} 该通过");
        }
    }

    #[test]
    fn prune_keeps_the_newest_n() {
        let all = vec![
            entry("2.0.14", "2026-09-12 09:00", false),
            entry("2.0.11", "2026-09-01 09:00", false),
            entry("2.0.9", "2026-08-20 09:00", false),
            entry("2.0.7", "2026-08-01 09:00", false),
        ];
        let doomed: Vec<&str> = pick_to_prune(&all, 3).iter().map(|e| e.version.as_str()).collect();
        assert_eq!(doomed, vec!["2.0.7"]);
    }

    /// 当前在用的那一版**永远不删**，哪怕它排在最后。
    ///
    /// 挨着删的话，回滚到一个老版本再装点别的，正在用的那份就没了 ——
    /// 而它就是 `claude.exe` 本体的备份来源。
    #[test]
    fn the_version_in_use_is_never_pruned() {
        let all = vec![
            entry("2.0.14", "2026-09-12 09:00", false),
            entry("2.0.11", "2026-09-01 09:00", false),
            entry("2.0.9", "2026-08-20 09:00", false),
            entry("2.0.7", "2026-08-01 09:00", true),
        ];
        let doomed = pick_to_prune(&all, 3);
        assert!(doomed.is_empty(), "在用的那版不该进删除名单");
    }

    #[test]
    fn nothing_to_prune_when_under_the_cap() {
        let all = vec![entry("2.0.14", "2026-09-12 09:00", false)];
        assert!(pick_to_prune(&all, KEEP).is_empty());
    }

    #[test]
    fn history_of_a_missing_dir_is_empty_not_an_error() {
        // 没装过、或者刚换了托管目录 —— 这是正常情况，不是错误。
        let out = history(Path::new("C:\\definitely\\not\\here"), App::ClaudeCode, None);
        assert!(out.is_empty());
    }
}
