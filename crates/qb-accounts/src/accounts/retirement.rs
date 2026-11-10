//! A deleted macOS slot must not reuse the Keychain identity derived from its path.
//! We retain only a slot-name marker, never official credentials. Mark before moving
//! the directory so an interrupted deletion cannot make an old identity reusable.
use super::{AccountRoots, GateError, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

fn marker(roots: &AccountRoots, label: &str) -> PathBuf {
    // macOS commonly aliases case and Unicode-normalization variants on disk.
    let name: String = label.nfc().flat_map(char::to_lowercase).collect();
    roots.panel.join("retired-claude-slots").join(format!(
        "{}.retired",
        hex::encode(Sha256::digest(name.as_bytes()))
    ))
}

fn exists(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn ensure_available(roots: &AccountRoots, label: &str) -> Result<()> {
    if exists(&marker(roots, label))? {
        return Err(GateError::Other(format!(
            "槽位名「{label}」曾在这台 Mac 上删除，不能再次使用，以免继承钥匙串中的旧登录。请使用一个新的槽位名；官方登录需在 Claude Code 中自行退出。"
        )));
    }
    Ok(())
}

/// Returns only a newly-created marker, so rollback never removes older history.
fn prepare(roots: &AccountRoots, label: &str) -> Result<Option<PathBuf>> {
    use std::io::Write;
    let path = marker(roots, label);
    if exists(&path)? {
        return Ok(None);
    }
    std::fs::create_dir_all(path.parent().expect("marker parent"))?;
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    file.write_all(b"QB Gate: retired macOS Claude slot name\n")?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        // The name must survive a crash before its credential directory moves.
        std::fs::File::open(path.parent().expect("marker parent"))?.sync_all()?;
        std::fs::File::open(&roots.panel)?.sync_all()?;
    }
    Ok(Some(path))
}

pub(super) fn stage_delete(
    roots: &AccountRoots,
    label: &str,
    source: &Path,
    staged: &Path,
) -> Result<()> {
    let added = prepare(roots, label)?;
    if let Err(error) = crate::archive_dir::move_dir(source, staged) {
        if let Some(path) = added {
            if let Err(rollback) = std::fs::remove_file(path) {
                return Err(GateError::Other(format!(
                    "槽位未删除：{error}；未能撤回同名重建限制：{rollback}。原槽位仍可继续使用。"
                )));
            }
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> AccountRoots {
        AccountRoots {
            panel: std::env::temp_dir().join(format!("qb-retirement-{}", crate::config_io::id())),
            bridge: None,
            appdata: None,
        }
    }

    #[test]
    fn a_retired_name_stays_blocked_after_reopening_and_normalizing() {
        let roots = roots();
        let source = roots.slot_dir("CAFÉ");
        std::fs::create_dir_all(&source).unwrap();
        let staged = roots.panel.join(".deleting");
        stage_delete(&roots, "CAFÉ", &source, &staged).unwrap();
        std::fs::remove_dir_all(staged).unwrap();
        let reopened = AccountRoots {
            panel: roots.panel.clone(),
            bridge: None,
            appdata: None,
        };
        assert!(ensure_available(&reopened, "CAFÉ").is_err());
        assert!(ensure_available(&reopened, "cafe\u{301}").is_err());
        ensure_available(&reopened, "new-name").unwrap();
        std::fs::remove_dir_all(roots.panel).unwrap();
    }

    #[test]
    fn failed_deletion_rolls_back_only_its_new_marker() {
        let roots = roots();
        let source = roots.slot_dir("keep");
        std::fs::create_dir_all(&source).unwrap();
        let invalid = roots.panel.join("absent-parent/staged");
        assert!(stage_delete(&roots, "keep", &source, &invalid).is_err());
        assert!(source.is_dir());
        ensure_available(&roots, "keep").unwrap();
        prepare(&roots, "keep").unwrap();
        assert!(stage_delete(&roots, "keep", &source, &invalid).is_err());
        assert!(ensure_available(&roots, "keep").is_err());
        std::fs::remove_dir_all(roots.panel).unwrap();
    }

    #[test]
    fn inability_to_persist_retirement_prevents_directory_removal() {
        let roots = roots();
        let source = roots.slot_dir("keep");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(roots.panel.join("retired-claude-slots"), b"occupied").unwrap();
        assert!(stage_delete(&roots, "keep", &source, &roots.panel.join(".deleting")).is_err());
        assert!(source.is_dir());
        std::fs::remove_dir_all(roots.panel).unwrap();
    }
}
