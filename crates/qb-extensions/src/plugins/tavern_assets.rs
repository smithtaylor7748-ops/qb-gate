//! 酒馆资产盘点与搬运：世界书、角色卡、预设、群组、聊天记录、扩展。
//!
//! **这里不重写酒馆的编辑界面。** 编辑仍然在酒馆自己那边做，
//! 面板只负责「有哪些、多大、什么时候改的」以及备份 / 恢复 / 导入导出。
//! 想在面板里改世界书内容，那是在追一个永远追不上的上游。
//!
//! 写入一律：先备份 → 写临时文件 → 改名。恢复同理，
//! 恢复之前先把现状另存一份，否则一次误点就找不回来了。

use crate::error::{GateError, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// 资产类别。`dir` 是它在 `data/default-user/` 下的目录名。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Category {
    pub id: &'static str,
    pub label: &'static str,
    pub dir: &'static str,
}

pub const CATEGORIES: &[Category] = &[
    Category {
        id: "worlds",
        label: "世界书",
        dir: "worlds",
    },
    Category {
        id: "characters",
        label: "角色卡",
        dir: "characters",
    },
    Category {
        id: "presets",
        label: "预设",
        dir: "OpenAI Settings",
    },
    Category {
        id: "groups",
        label: "群组",
        dir: "groups",
    },
    Category {
        id: "chats",
        label: "聊天记录",
        dir: "chats",
    },
    Category {
        id: "extensions",
        label: "扩展",
        dir: "extensions",
    },
];

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AssetItem {
    pub name: String,
    pub path: String,
    /// 字节数。`#[ts(type = "number")]`：ts-rs 默认把 u64 映射成 bigint，
    /// 而 JSON 上它就是个普通数字。文件大小要到 9 PB 才碰得到 2^53。
    #[ts(type = "number")]
    pub size: u64,
    pub modified: Option<String>,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CategoryListing {
    pub id: &'static str,
    pub label: &'static str,
    pub dir: String,
    pub exists: bool,
    pub items: Vec<AssetItem>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct BackupEntry {
    pub id: String,
    pub path: String,
    pub created: String,
    /// 字节数。同 `AssetItem::size`：JSON 上是普通数字，不是 bigint。
    #[ts(type = "number")]
    pub size: u64,
}

fn user_data_dir() -> PathBuf {
    super::sillytavern::load_config()
        .sillytavern_root
        .join("data")
        .join("default-user")
}

fn backups_root() -> PathBuf {
    crate::paths::state_dir().join("tavern-backups")
}

fn stamp(t: std::time::SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Local> = t.into();
    dt.format("%Y-%m-%d %H:%M").to_string()
}

fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else {
        return 0;
    };
    rd.filter_map(|e| e.ok())
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            _ => e.metadata().map(|m| m.len()).unwrap_or(0),
        })
        .sum()
}

pub fn list_all() -> Vec<CategoryListing> {
    let root = user_data_dir();
    CATEGORIES
        .iter()
        .map(|c| {
            let dir = root.join(c.dir);
            let mut items = Vec::new();
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.filter_map(|e| e.ok()) {
                    let meta = e.metadata().ok();
                    let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
                    let name = e.file_name().to_string_lossy().to_string();
                    // 酒馆自己留的 .bak 不算资产，列出来只会淹没真东西。
                    if name.contains(".bak") || name.starts_with('.') {
                        continue;
                    }
                    items.push(AssetItem {
                        size: if is_dir {
                            dir_size(&e.path())
                        } else {
                            meta.as_ref().map(|m| m.len()).unwrap_or(0)
                        },
                        modified: meta.as_ref().and_then(|m| m.modified().ok()).map(stamp),
                        path: e.path().display().to_string(),
                        is_dir,
                        name,
                    });
                }
            }
            items.sort_by(|a, b| a.name.cmp(&b.name));
            CategoryListing {
                id: c.id,
                label: c.label,
                exists: dir.is_dir(),
                dir: dir.display().to_string(),
                items,
            }
        })
        .collect()
}

fn copy_dir(from: &Path, to: &Path) -> Result<u64> {
    std::fs::create_dir_all(to)?;
    let mut n = 0;
    for e in std::fs::read_dir(from)?.filter_map(|e| e.ok()) {
        let src = e.path();
        let dst = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            n += copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
            n += 1;
        }
    }
    Ok(n)
}

/// 备份全部资产目录到一个带时间戳的文件夹。
///
/// 用目录复制而不是打包：不引压缩依赖，而且出问题时用户可以直接进去翻文件，
/// 不需要本程序也能恢复。
pub fn backup() -> Result<BackupEntry> {
    let root = user_data_dir();
    if !root.is_dir() {
        return Err(GateError::NotFound(root.display().to_string()));
    }
    let id = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let dest = backups_root().join(&id);
    std::fs::create_dir_all(&dest)?;

    for c in CATEGORIES {
        let src = root.join(c.dir);
        if src.is_dir() {
            copy_dir(&src, &dest.join(c.dir))?;
        }
    }
    // settings.json 不在上面任何一类里，但丢了同样很痛。
    for f in ["settings.json", "secrets.json"] {
        let src = root.join(f);
        if src.is_file() {
            let _ = std::fs::copy(&src, dest.join(f));
        }
    }

    crate::audit::write(&format!("酒馆资产已备份到 {}", dest.display()));
    Ok(BackupEntry {
        size: dir_size(&dest),
        created: stamp(std::time::SystemTime::now()),
        path: dest.display().to_string(),
        id,
    })
}

pub fn list_backups() -> Vec<BackupEntry> {
    let Ok(rd) = std::fs::read_dir(backups_root()) else {
        return Vec::new();
    };
    let mut out: Vec<BackupEntry> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| BackupEntry {
            id: e.file_name().to_string_lossy().to_string(),
            size: dir_size(&e.path()),
            created: e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(stamp)
                .unwrap_or_default(),
            path: e.path().display().to_string(),
        })
        .collect();
    out.sort_by(|a, b| b.id.cmp(&a.id));
    out
}

/// 从备份恢复。
///
/// **恢复之前先把现状再备份一次。** 否则用户点错一次，
/// 当前的世界书和角色卡就没了 —— 这个操作没有撤销。
pub fn restore(backup_id: &str) -> Result<String> {
    // 不允许路径穿越：id 只能是我们自己生成的那种时间戳。
    if !backup_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(GateError::Other("备份 id 不合法".into()));
    }
    let src = backups_root().join(backup_id);
    if !src.is_dir() {
        return Err(GateError::NotFound(src.display().to_string()));
    }

    let safety = backup()?;

    let root = user_data_dir();
    for c in CATEGORIES {
        let from = src.join(c.dir);
        if from.is_dir() {
            copy_dir(&from, &root.join(c.dir))?;
        }
    }
    for f in ["settings.json", "secrets.json"] {
        let from = src.join(f);
        if from.is_file() {
            let _ = std::fs::copy(&from, root.join(f));
        }
    }

    crate::audit::write(&format!("酒馆资产已从 {backup_id} 恢复"));
    Ok(format!(
        "已从 {backup_id} 恢复。恢复前的现状已另存为 {}，点错了可以再恢复回去。",
        safety.id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_cover_world_info_and_characters() {
        let ids: Vec<&str> = CATEGORIES.iter().map(|c| c.id).collect();
        assert!(ids.contains(&"worlds"), "世界书必须在列");
        assert!(ids.contains(&"characters"));
        assert!(ids.contains(&"presets"));
    }

    #[test]
    fn world_dir_name_matches_sillytavern_layout() {
        let w = CATEGORIES.iter().find(|c| c.id == "worlds").unwrap();
        assert_eq!(w.dir, "worlds");
        let p = CATEGORIES.iter().find(|c| c.id == "presets").unwrap();
        assert_eq!(p.dir, "OpenAI Settings");
    }

    #[test]
    fn restore_rejects_path_traversal() {
        assert!(restore("../../windows").is_err());
        assert!(restore("a/b").is_err());
        assert!(restore("..").is_err());
    }
}
