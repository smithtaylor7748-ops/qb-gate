//! Native macOS discovery. Read bundle metadata instead of guessing executable names.
use std::path::{Path, PathBuf};

pub fn path_dirs() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();
    if let Some(home) = dirs::home_dir() {
        for relative in [
            ".local/bin",
            ".cargo/bin",
            ".volta/bin",
            ".bun/bin",
            ".npm-global/bin",
        ] {
            paths.push(home.join(relative));
        }
        let nvm = home.join(".nvm/versions/node");
        let mut versions: Vec<_> = std::fs::read_dir(nvm)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path().join("bin"))
            .collect();
        versions.sort();
        versions.reverse();
        paths.extend(versions);
    }
    paths.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    let mut result = Vec::new();
    for path in paths {
        if path.is_absolute() && !result.contains(&path) {
            result.push(path);
        }
    }
    result
}

pub fn command(name: &str) -> Option<PathBuf> {
    path_dirs()
        .into_iter()
        .map(|p| p.join(name))
        .find(|p| p.is_file())
}

pub fn bundle(name: &str) -> Option<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    roots
        .into_iter()
        .map(|p| p.join(format!("{name}.app")))
        .find(|p| p.is_dir())
}

pub fn plist_value(bundle: &Path, key: &str) -> Option<String> {
    let output = std::process::Command::new("/usr/libexec/PlistBuddy")
        .arg("-c")
        .arg(format!("Print :{key}"))
        .arg(bundle.join("Contents/Info.plist"))
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && !value.is_empty()).then_some(value)
}

pub fn executable(name: &str) -> Option<PathBuf> {
    let bundle = bundle(name)?;
    executable_in(&bundle)
}

fn executable_in(bundle: &Path) -> Option<PathBuf> {
    let binary = plist_value(bundle, "CFBundleExecutable")?;
    if Path::new(&binary).components().count() != 1 {
        return None;
    }
    let path = bundle.join("Contents/MacOS").join(binary);
    path.is_file().then_some(path)
}

pub fn version(name: &str) -> Option<String> {
    plist_value(&bundle(name)?, "CFBundleShortVersionString")
}

pub fn npm_roots() -> Vec<PathBuf> {
    let mut roots = vec![super::managed::slot_dir("gemini-cli").join("node_modules")];
    if let Some(prefix) = std::env::var_os("NPM_CONFIG_PREFIX") {
        roots.push(PathBuf::from(prefix).join("lib/node_modules"));
    }
    for dir in path_dirs() {
        if let Some(prefix) = dir.parent() {
            roots.push(prefix.join("lib/node_modules"));
        }
    }
    let mut ordered = Vec::new();
    for root in roots {
        if !ordered.contains(&root) {
            ordered.push(root);
        }
    }
    ordered
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[test]
    fn macos_bundle_uses_declared_executable_with_spaces_and_rejects_escape() {
        let root = std::env::temp_dir().join(format!(
            "qb-mac 用户 O'Brien {}.app",
            crate::config_io::id()
        ));
        std::fs::create_dir_all(root.join("Contents/MacOS")).unwrap();
        std::fs::write(root.join("Contents/MacOS/Actual Launcher"), "fixture").unwrap();
        let plist = |binary: &str| {
            format!("<?xml version=\"1.0\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>CFBundleExecutable</key><string>{binary}</string></dict></plist>")
        };
        std::fs::write(root.join("Contents/Info.plist"), plist("Actual Launcher")).unwrap();
        assert_eq!(
            executable_in(&root),
            Some(root.join("Contents/MacOS/Actual Launcher"))
        );
        std::fs::write(root.join("Contents/Info.plist"), plist("../../outside")).unwrap();
        assert_eq!(executable_in(&root), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
