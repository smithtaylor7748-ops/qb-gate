//! 全局配置快照与回滚。
//!
//! # 为什么需要它
//!
//! 这个面板会改动散在四个地方的东西：`~/.claude/settings.json`、
//! `~/.codex/` 下两个文件、`%LOCALAPPDATA%\ClaudeIpGate` 里的白名单与目录、
//! 还有 NTFS ACL、目录联结点、系统时区。
//!
//! 各处原本各有各的备份（中转站有时间戳快照、酒馆有目录复制），但**没有一个
//! 「把当下整体存一份」的入口**。改坏了要一处处往回翻，而人在那个时候恰恰
//! 记不清自己动过哪几处。
//!
//! # 备份是复制，不是打包
//!
//! 沿用酒馆资产那套做法：快照就是一个目录，里面按原样放着那些文件。
//! **不需要本程序也能恢复** —— 出问题时可以直接进文件夹把文件拷回去。
//! 打成 zip 会在最需要它的时候多一道坎。
//!
//! # 不快照什么
//!
//! - **凭证文件**（`.credentials.json`）。快照会被拷来拷去，把 OAuth 凭证
//!   复制到第二个地方是在扩大暴露面，而它本来就能重新登录拿回来。
//! - **`~/.claude.json`**。那里面是会话历史与项目列表，几十上百 KB，
//!   而且不是这个面板改的东西。
//! - **账户槽位目录本身**。它们已经在 `%LOCALAPPDATA%\ClaudeIpGate` 下了，
//!   再复制一份等于把整个账户树翻倍。快照只记住**当时激活的是哪个**。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{GateError, Result};

/// 快照里收哪些文件。
///
/// 每一条都是「这个面板会去改」的东西。加新条目前先问一句：
/// 改坏了用户能自己修回来吗？能的话就不必进快照。
fn tracked_files() -> Vec<(&'static str, PathBuf)> {
    let home = dirs::home_dir().unwrap_or_default();
    let state = crate::gate::state_dir();
    vec![
        ("claude-settings.json", home.join(".claude").join("settings.json")),
        ("codex-config.toml", home.join(".codex").join("config.toml")),
        ("codex-auth.json", home.join(".codex").join("auth.json")),
        ("allowlist.txt", state.join("allowlist.txt")),
        ("relay.json", state.join("relay.json")),
        ("settings.json", state.join("settings.json")),
        ("progress.json", state.join("progress.json")),
        ("profiles.json", state.join("profiles.json")),
    ]
}

/// 快照里记下来、但不作为文件复制的那些状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub created: String,
    pub note: String,
    /// 当时激活的账户槽位标签。
    pub active_account: Option<String>,
    /// 当时的系统时区（Windows 名）。
    pub timezone: Option<String>,
    /// 当时门禁是不是全锁着。恢复时据此决定要不要重新上锁。
    pub all_locked: bool,
    /// 实际存进来的文件（`tracked_files` 里当时存在的那些）。
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotEntry {
    pub id: String,
    pub path: String,
    pub manifest: Manifest,
}

pub fn root() -> PathBuf {
    crate::gate::state_dir().join("snapshots")
}

/// 保留多少份。超出的从最旧的开始删。
const KEEP: usize = 20;

/// 快照 id 只能是时间戳格式。
///
/// 恢复接口收的是用户传进来的 id，直接拼路径就会被 `..\..\` 走出去。
/// 这跟酒馆备份恢复用的是同一条判断。
fn valid_id(id: &str) -> bool {
    id.len() == 15
        && id.as_bytes()[8] == b'-'
        && id
            .bytes()
            .enumerate()
            .all(|(i, b)| if i == 8 { b == b'-' } else { b.is_ascii_digit() })
}

/// 这份快照当时激活的账户。id 不合规或读不到清单就是 `None`。
///
/// 命令层用它判断「回滚之前要不要先清场」—— 换号就得先关掉全部 Claude。
pub fn account_of(id: &str) -> Option<String> {
    if !valid_id(id) {
        return None;
    }
    let text = std::fs::read_to_string(root().join(id).join("manifest.json")).ok()?;
    serde_json::from_str::<Manifest>(&text).ok()?.active_account
}

pub fn create(note: &str) -> Result<SnapshotEntry> {
    let id = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let dir = root().join(&id);
    std::fs::create_dir_all(&dir)?;

    let mut files = Vec::new();
    for (name, src) in tracked_files() {
        if src.is_file() && std::fs::copy(&src, dir.join(name)).is_ok() {
            files.push(name.to_string());
        }
    }

    let targets = crate::gate::collect_targets();
    let manifest = Manifest {
        created: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        note: note.to_string(),
        active_account: crate::accounts::slots()
            .into_iter()
            .find(|s| s.active)
            .map(|s| s.label),
        timezone: crate::sysenv::current_windows_tz().ok(),
        all_locked: !targets.is_empty() && targets.iter().all(|t| t.locked),
        files,
    };
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    rotate();
    crate::gate::log::write(&format!("已创建配置快照 {id}"));
    Ok(SnapshotEntry {
        id,
        path: dir.display().to_string(),
        manifest,
    })
}

fn rotate() {
    let Ok(rd) = std::fs::read_dir(root()) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    if dirs.len() <= KEEP {
        return;
    }
    dirs.sort();
    for old in dirs.iter().take(dirs.len() - KEEP) {
        let _ = std::fs::remove_dir_all(old);
    }
}

pub fn list() -> Vec<SnapshotEntry> {
    let Ok(rd) = std::fs::read_dir(root()) else {
        return Vec::new();
    };
    let mut out: Vec<SnapshotEntry> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let id = e.file_name().into_string().ok()?;
            let manifest = std::fs::read_to_string(e.path().join("manifest.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            Some(SnapshotEntry {
                path: e.path().display().to_string(),
                id,
                manifest,
            })
        })
        .collect();
    // 新的排前面。
    out.sort_by(|a, b| b.id.cmp(&a.id));
    out
}

/// 回滚到某一份快照。
///
/// **恢复之前先把现状再存一份。** 「回滚」本身也是个能出错的操作 ——
/// 恢复错了份、或者恢复完发现问题在别处，用户得能再退回来。
/// 这跟酒馆资产恢复是同一条规矩。
pub fn restore(id: &str) -> Result<String> {
    if !valid_id(id) {
        return Err(GateError::Other(format!("快照 id 格式不对：{id}")));
    }
    let dir = root().join(id);
    if !dir.is_dir() {
        return Err(GateError::NotFound(dir.display().to_string()));
    }

    let before = create("回滚前自动存档")?;

    let manifest: Manifest = std::fs::read_to_string(dir.join("manifest.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();

    let mut restored = 0usize;
    for (name, dst) in tracked_files() {
        let src = dir.join(name);
        if !src.is_file() {
            continue;
        }
        if let Some(parent) = dst.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::copy(&src, &dst).is_ok() {
            restored += 1;
        }
    }

    // 账户槽位：只切联结点，不动目录本身。
    let mut extra = String::new();
    if let Some(label) = &manifest.active_account {
        match crate::accounts::switch(label) {
            Ok(_) => extra.push_str(&format!("，账户已切回 {label}")),
            Err(e) => extra.push_str(&format!("，账户切回 {label} 失败：{e}")),
        }
    }

    // 门禁：当时锁着就锁回去。**当时没锁就不要顺手锁上** ——
    // 那是在恢复里夹带一个用户没要求的动作。
    if manifest.all_locked {
        if let Ok(n) = crate::gate::lock_all() {
            extra.push_str(&format!("，重新上锁 {n} 处"));
        }
    }

    // 时区**不自动改** —— 它要管理员权限会弹 UAC，在一个「恢复配置」的
    // 操作里突然弹 UAC 会让人以为程序出了问题。如实告诉用户去哪改。
    if let Some(tz) = &manifest.timezone {
        if crate::sysenv::current_windows_tz().ok().as_deref() != Some(tz.as_str()) {
            extra.push_str(&format!("。注意：快照当时的系统时区是 {tz}，与现在不同，需要到「环境与安装」页手动切换（要管理员权限）"));
        }
    }

    let detail = format!(
        "已回滚到快照 {id}，恢复 {restored} 个文件{extra}。回滚前的现状已存为 {}。",
        before.id
    );
    crate::gate::log::write(&detail);
    Ok(detail)
}

pub fn remove(id: &str) -> Result<()> {
    if !valid_id(id) {
        return Err(GateError::Other(format!("快照 id 格式不对：{id}")));
    }
    let dir = root().join(id);
    if dir.is_dir() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

/// 快照目录，给「在资源管理器里打开」用。
pub fn dir_of(id: &str) -> Result<PathBuf> {
    if !valid_id(id) {
        return Err(GateError::Other(format!("快照 id 格式不对：{id}")));
    }
    Ok(root().join(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_must_be_a_timestamp() {
        assert!(valid_id("20260909-141530"));
        assert!(valid_id("20000101-000000"));
    }

    #[test]
    fn path_traversal_is_rejected() {
        // 恢复接口收的是用户传进来的 id，直接拼路径就会被走出去。
        for bad in [
            "..",
            "../..",
            r"..\..\Windows",
            "20260909-14153",   // 少一位
            "20260909_141530",  // 分隔符不对
            "2026090-9141530",  // 分隔符位置不对
            "abcdefgh-ijklmno",
            "",
        ] {
            assert!(!valid_id(bad), "这个应该被拒绝：{bad}");
        }
    }

    #[test]
    fn credentials_are_never_tracked() {
        // 快照会被拷来拷去。把 OAuth 凭证复制到第二个地方是在扩大暴露面，
        // 而它本来就能重新登录拿回来。
        for (name, path) in tracked_files() {
            let p = path.to_string_lossy().to_lowercase();
            assert!(!p.contains("credentials"), "{name} 指向了凭证文件：{p}");
            assert!(
                !p.ends_with(".claude.json"),
                "{name} 指向了会话历史文件：{p}"
            );
        }
    }

    #[test]
    fn tracked_names_are_unique_and_flat() {
        // 快照目录是平铺的，两条 tracked 用同一个文件名就会互相覆盖。
        let names: Vec<&str> = tracked_files().iter().map(|(n, _)| *n).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "快照文件名撞了：{names:?}");
        for n in names {
            assert!(!n.contains('/') && !n.contains('\\'), "文件名不该带路径：{n}");
        }
    }
}
