//! One search order for discovering Mac tools and starting their child processes.
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

pub fn directories(inherited: Option<&OsStr>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates: Vec<_> = inherited
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .collect();
    if let Some(home) = home {
        for relative in [
            ".local/bin",
            ".cargo/bin",
            ".volta/bin",
            ".bun/bin",
            ".npm-global/bin",
        ] {
            candidates.push(home.join(relative));
        }
        let mut nvm: Vec<_> = std::fs::read_dir(home.join(".nvm/versions/node"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.path().join("bin"))
            .collect();
        nvm.sort();
        nvm.reverse();
        candidates.extend(nvm);
    }
    candidates.extend(
        [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin",
        ]
        .map(PathBuf::from),
    );
    let mut result = Vec::new();
    for path in candidates {
        if path.is_absolute() && !result.contains(&path) {
            result.push(path);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finder_path_finds_nvm_and_keeps_explicit_order_without_relative_entries() {
        let home = std::env::temp_dir().join(format!("qb-node-path-{}", uuid::Uuid::new_v4()));
        let bin = home.join(".nvm/versions/node/v24.0.0/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("node"), b"fixture").unwrap();
        let explicit = home.join("custom/bin");
        let inherited = std::env::join_paths([
            explicit.as_path(),
            Path::new("relative"),
            explicit.as_path(),
        ])
        .unwrap();
        let paths = directories(Some(&inherited), Some(&home));
        assert_eq!(paths.first(), Some(&explicit));
        assert_eq!(paths.iter().filter(|p| **p == explicit).count(), 1);
        assert!(paths.iter().all(|p| p.is_absolute()));
        assert!(paths.contains(&bin));
        assert_eq!(
            paths.iter().map(|p| p.join("node")).find(|p| p.is_file()),
            Some(bin.join("node"))
        );
        std::fs::remove_dir_all(home).unwrap();
    }
}
