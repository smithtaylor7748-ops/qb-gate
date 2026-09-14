//! Versioned configuration snapshots. OAuth credentials and installation locations are excluded.
use crate::{
    config_io::{self, Edit},
    error::{GateError, Result},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;
use ts_rs::TS;

fn tracked_files() -> Vec<(&'static str, PathBuf)> {
    let active = crate::accounts::active_label(&crate::accounts::AccountRoots::current());
    tracked_for(active.as_deref())
}

/// 快照要带走哪些文件。
///
/// **`relay.json` 不在这里，是有意的。** 它是迁移前的旧中转站目录，
/// `relay::store::save()` 全项目零调用者 —— 迁移之后就再没人写过它，
/// 运行期也再没人读（唯一读它的是 `repository::migrate()`，而那有
/// `legacy-v1` 哨兵挡着、恢复快照也清不掉那个哨兵，所以永不重跑）。
/// 继续备份它只会让一份冻结的 DPAPI 密文在每个快照目录里再复制一遍。
fn tracked_for(account: Option<&str>) -> Vec<(&'static str, PathBuf)> {
    let home = dirs::home_dir().unwrap_or_default();
    let state = crate::paths::state_dir();
    let roots = crate::accounts::AccountRoots::current();
    let settings = account
        .map(|label| roots.slot_dir(label))
        .unwrap_or_else(|| home.join(".claude"))
        .join("settings.json");
    vec![
        ("claude-settings.json", settings),
        ("codex-config.toml", home.join(".codex/config.toml")),
        ("allowlist.txt", state.join("allowlist.txt")),
        ("settings.json", state.join("settings.json")),
        ("progress.json", state.join("progress.json")),
        ("profiles.json", state.join("profiles.json")),
    ]
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, rename = "SnapshotManifest")]
pub struct Manifest {
    #[serde(default)]
    pub schema: u32,
    #[serde(default)]
    pub environments: Vec<(String, crate::domain::Client)>,
    pub created: String,
    pub note: String,
    pub active_account: Option<String>,
    pub timezone: Option<String>,
    pub all_locked: bool,
    pub files: Vec<String>,
    #[serde(default)]
    pub absent: Vec<String>,
    #[serde(default)]
    pub hashes: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SnapshotEntry {
    pub id: String,
    pub path: String,
    pub manifest: Manifest,
}
pub fn root() -> PathBuf {
    crate::paths::state_dir().join("snapshots")
}
const KEEP: usize = 20;
fn valid_id(id: &str) -> bool {
    let stamp = id.get(..15).unwrap_or("");
    stamp.len() == 15
        && stamp.bytes().enumerate().all(|(i, b)| {
            if i == 8 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
        && (id.len() == 15
            || (id.len() == 48
                && id.as_bytes()[15] == b'-'
                && id[16..].bytes().all(|b| b.is_ascii_hexdigit())))
}
pub fn account_of(id: &str) -> Option<String> {
    if !valid_id(id) {
        return None;
    }
    serde_json::from_slice::<Manifest>(&std::fs::read(root().join(id).join("manifest.json")).ok()?)
        .ok()?
        .active_account
}

pub fn create(note: &str) -> Result<SnapshotEntry> {
    let targets = crate::gate::collect_targets();
    let manifest = Manifest {
        schema: 3,
        created: chrono::Utc::now().to_rfc3339(),
        note: note.into(),
        active_account: crate::accounts::active_label(&crate::accounts::AccountRoots::current()),
        timezone: crate::sysenv::current_windows_tz().ok(),
        all_locked: !targets.is_empty() && targets.iter().all(|t| t.locked),
        ..Default::default()
    };
    create_workspace_snapshot(manifest, true)
}
fn create_workspace_snapshot(mut manifest: Manifest, prune: bool) -> Result<SnapshotEntry> {
    let db = crate::repository::Repository::open()?;
    let data = db.backup_data()?;
    manifest.schema = 3;
    manifest.environments = data
        .environments
        .iter()
        .map(|(e, _)| (e.id.clone(), e.client))
        .collect();
    let extra = environment_files(&db.root, &manifest)?;
    let staging = db
        .root
        .join(format!(".snapshot-data-{}.json", config_io::id()));
    config_io::replace(&staging, Some(&serde_json::to_vec_pretty(&data)?))?;
    let mut tracked = tracked_files();
    tracked.extend(
        extra
            .iter()
            .map(|(name, path)| (name.as_str(), path.clone())),
    );
    tracked.push(("workspace-data.json", staging.clone()));
    let result = create_in(&root(), &tracked, manifest, prune);
    let _ = std::fs::remove_file(staging);
    result
}
fn environment_files(
    state: &std::path::Path,
    manifest: &Manifest,
) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (id, client) in &manifest.environments {
        if !seen.insert(id) || *client == crate::domain::Client::ClaudeDesktop {
            return Err(GateError::Other(
                "快照环境清单含重复 ID 或不支持的客户端".into(),
            ));
        }
        let dir = crate::config_io::environment_dir(state, id)?;
        let name = if *client == crate::domain::Client::Codex {
            "config.toml"
        } else {
            "settings.json"
        };
        out.push((format!("environment-{id}-{name}"), dir.join(name)));
        // .claude.json can contain session state; only settings are snapshotted.
    }
    Ok(out)
}

fn create_in(
    root: &std::path::Path,
    tracked: &[(&str, PathBuf)],
    mut manifest: Manifest,
    prune: bool,
) -> Result<SnapshotEntry> {
    let id = config_io::id();
    let dir = root.join(&id);
    std::fs::create_dir_all(root)?;
    std::fs::create_dir(&dir)?;
    let result = (|| -> Result<()> {
        for (name, path) in tracked {
            if let Some(bytes) = config_io::read_optional(path)? {
                std::fs::write(dir.join(name), &bytes)?;
                manifest.files.push((*name).into());
                manifest
                    .hashes
                    .insert((*name).into(), hex::encode(Sha256::digest(&bytes)));
            } else {
                manifest.absent.push((*name).into());
            }
        }
        config_io::replace(
            &dir.join("manifest.json"),
            Some(&serde_json::to_vec_pretty(&manifest)?),
        )
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }
    if prune {
        rotate_in(root, &[&id]);
    }
    Ok(SnapshotEntry {
        id,
        path: dir.display().to_string(),
        manifest,
    })
}
fn rotate_in(root: &std::path::Path, protected: &[&str]) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    let mut dirs: Vec<_> = rd
        .flatten()
        .filter(|e| {
            valid_id(&e.file_name().to_string_lossy()) && e.path().join("manifest.json").is_file()
        })
        .map(|e| e.path())
        .collect();
    dirs.sort_by_key(|p| {
        std::fs::read(p.join("manifest.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Manifest>(&b).ok())
            .map(|m| m.created)
            .unwrap_or_default()
    });
    let excess = dirs.len().saturating_sub(KEEP);
    for p in dirs
        .into_iter()
        .filter(|p| {
            !protected.contains(&p.file_name().and_then(|s| s.to_str()).unwrap_or_default())
        })
        .take(excess)
    {
        let _ = std::fs::remove_dir_all(p);
    }
}
pub fn list() -> Vec<SnapshotEntry> {
    let Ok(rd) = std::fs::read_dir(root()) else {
        return Vec::new();
    };
    let mut out: Vec<_> = rd
        .flatten()
        .filter_map(|e| {
            let id = e.file_name().into_string().ok()?;
            if !valid_id(&id) {
                return None;
            }
            let manifest =
                serde_json::from_slice(&std::fs::read(e.path().join("manifest.json")).ok()?)
                    .ok()?;
            Some(SnapshotEntry {
                id,
                path: e.path().display().to_string(),
                manifest,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.manifest
            .created
            .cmp(&a.manifest.created)
            .then(b.id.cmp(&a.id))
    });
    out
}

fn restore_edits(
    dir: &std::path::Path,
    manifest: &Manifest,
    tracked: &[(&str, PathBuf)],
) -> Result<Vec<Edit>> {
    let mut edits = Vec::new();
    for (name, dst) in tracked {
        let expected = config_io::read_optional(dst)?;
        let body = if manifest.files.iter().any(|n| n == name) {
            let mut bytes = std::fs::read(dir.join(name))?;
            if let Some(hash) = manifest.hashes.get(*name) {
                if hex::encode(Sha256::digest(&bytes)) != *hash {
                    return Err(GateError::Other(format!("快照文件 {name} 校验失败")));
                }
            }
            if *name == "settings.json" {
                let mut old = config_io::object(
                    std::str::from_utf8(&bytes).map_err(|e| GateError::Other(e.to_string()))?,
                )?;
                let current = config_io::object(
                    &String::from_utf8(expected.clone().unwrap_or_default())
                        .map_err(|e| GateError::Other(e.to_string()))?,
                )?;
                old.as_object_mut().unwrap().remove("managed_apps_dir");
                if let Some(root) = current.get("managed_apps_dir") {
                    old["managed_apps_dir"] = root.clone();
                }
                bytes = serde_json::to_vec_pretty(&old)?;
            } else if name.ends_with(".json") {
                config_io::object(
                    std::str::from_utf8(&bytes).map_err(|e| GateError::Other(e.to_string()))?,
                )?;
            } else if name.ends_with(".toml") {
                std::str::from_utf8(&bytes)
                    .map_err(|e| GateError::Other(e.to_string()))?
                    .parse::<toml_edit::DocumentMut>()
                    .map_err(|e| GateError::Other(e.to_string()))?;
            }
            Some(bytes)
        } else if manifest.schema >= 2
            && manifest.absent.iter().any(|n| n == name)
            && *name != "settings.json"
        {
            None
        } else {
            continue;
        };
        edits.push(Edit {
            path: dst.clone(),
            expected,
            body,
        });
    }
    Ok(edits)
}

pub fn restore(id: &str) -> Result<String> {
    crate::sessions::ensure_verified(|_| true)?;
    if crate::sessions::list().iter().any(|s| s.state == "running") {
        return Err(GateError::Other(
            "请先停止受管会话，再恢复工作空间配置".into(),
        ));
    }
    crate::gate::invalidate_verdict()?;
    let dir = dir_of(id)?;
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(dir.join("manifest.json"))?)?;
    // An account snapshot restores that account's config; it never changes the active identity.
    let current = crate::accounts::active_label(&crate::accounts::AccountRoots::current());
    if manifest.active_account != current {
        return Err(GateError::Other(
            "此快照属于另一官方账户，请先在官方账户页切换后再恢复；中转环境不受影响".into(),
        ));
    }
    if manifest.schema > 3 {
        return Err(GateError::Other("此快照来自更新版本，未执行恢复".into()));
    }
    let db = crate::repository::Repository::open()?;
    let extra = environment_files(&db.root, &manifest)?;
    let mut tracked = tracked_files();
    tracked.extend(
        extra
            .iter()
            .map(|(name, path)| (name.as_str(), path.clone())),
    );
    let data = if manifest.schema >= 3 {
        let bytes = std::fs::read(dir.join("workspace-data.json"))?;
        if manifest.hashes.get("workspace-data.json") != Some(&hex::encode(Sha256::digest(&bytes)))
        {
            return Err(GateError::Other("工作空间数据校验失败".into()));
        }
        Some(serde_json::from_slice::<crate::repository::DataBackup>(
            &bytes,
        )?)
    } else {
        None
    };
    let edits = restore_edits(&dir, &manifest, &tracked)?; // Read and verify all source bytes before retention can run.
    let before = create_workspace_snapshot(
        Manifest {
            schema: 3,
            created: chrono::Utc::now().to_rfc3339(),
            note: format!("恢复 {id} 前自动存档"),
            active_account: current,
            ..Default::default()
        },
        false,
    )?;
    let count = edits.len();
    crate::repository::commit_database(&db, edits, |_| {
        if let Some(data) = &data {
            db.restore_data(data)?;
        }
        Ok(())
    })?;
    rotate_in(&root(), &[id, &before.id]);
    Ok(format!(
        "已恢复 {count} 个配置文件；恢复前备份 {}。账户选择、安装目录和系统时区保持当前设置。",
        before.id
    ))
}
pub fn remove(id: &str) -> Result<()> {
    let p = dir_of(id)?;
    if p.is_dir() {
        std::fs::remove_dir_all(p)?;
    }
    Ok(())
}
pub fn dir_of(id: &str) -> Result<PathBuf> {
    if !valid_id(id) {
        return Err(GateError::Other("快照 ID 不合法".into()));
    }
    Ok(root().join(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simultaneous_snapshots_have_unique_ids_and_protected_oldest_survives() {
        let root = std::env::temp_dir().join(format!("qb-snapshot-{}", config_io::id()));
        let live = root.join("live.json");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&live, "{}").unwrap();
        let mut ids = Vec::new();
        for _ in 0..=KEEP {
            ids.push(
                create_in(
                    &root.join("snapshots"),
                    &[("live.json", live.clone())],
                    Manifest {
                        schema: 2,
                        ..Default::default()
                    },
                    false,
                )
                .unwrap()
                .id,
            );
        }
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), KEEP + 1);
        rotate_in(&root.join("snapshots"), &[&ids[0]]);
        assert!(root.join("snapshots").join(&ids[0]).is_dir());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn checksum_failure_blocks_restore_and_absence_is_preserved() {
        let root = std::env::temp_dir().join(format!("qb-snapshot-{}", config_io::id()));
        std::fs::create_dir_all(&root).unwrap();
        let a = root.join("a.json");
        let absent = root.join("absent.json");
        std::fs::write(&a, "{}").unwrap();
        let tracked = [("a.json", a.clone()), ("absent.json", absent.clone())];
        let saved = create_in(
            &root.join("snapshots"),
            &tracked,
            Manifest {
                schema: 2,
                ..Default::default()
            },
            false,
        )
        .unwrap();
        std::fs::write(&absent, "{}").unwrap();
        let edits =
            restore_edits(std::path::Path::new(&saved.path), &saved.manifest, &tracked).unwrap();
        assert!(edits.iter().any(|e| e.path == absent && e.body.is_none()));
        std::fs::write(std::path::Path::new(&saved.path).join("a.json"), "tampered").unwrap();
        assert!(
            restore_edits(std::path::Path::new(&saved.path), &saved.manifest, &tracked).is_err()
        );
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "{}");
        std::fs::remove_dir_all(root).unwrap();
    }
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
            "20260909-14153",  // 少一位
            "20260909_141530", // 分隔符不对
            "2026090-9141530", // 分隔符位置不对
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
            assert!(
                !n.contains('/') && !n.contains('\\'),
                "文件名不该带路径：{n}"
            );
        }
    }
}
