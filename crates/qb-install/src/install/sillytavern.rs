//! SillyTavern（酒馆）装在哪、是不是面板装的（2026-09-29，使用者定的）。
//!
//! 使用者：「扩展页的酒馆删掉，酒馆变成本项目的功能之一，软件页增加下载酒馆的功能，然后被本项目接管」。
//! 下载 / 更新 / 卸载在 [`super::sillytavern_setup`]；这里只有**位置表**和几个只读的小函数 ——
//! 跟 `antigravity.rs` + `antigravity_setup.rs` 同一种分法。
//!
//! # 位置表只有这一张
//!
//! | 什么 | 在哪 |
//! |---|---|
//! | 面板装的那份 | `<托管根>\sillytavern`（默认 `%LOCALAPPDATA%\ClaudeIpGate\apps\sillytavern`） |
//! | 更新前那一版（只留一份） | 同级的 `sillytavern.previous` |
//! | 解好、正在装依赖的新版 | 同级的 `sillytavern.staging` |
//! | 下载与解压的临时目录 | 同级的 `sillytavern.work` |
//! | 启动脚本 | `<目录>\Start.bat`（SillyTavern 自带） |
//! | 「这是面板装的」 | `<目录>\.qb-managed.json`（[`Marker`]） |
//!
//! 三个同级目录跟正式目录在同一个父目录下 —— 就位、回滚全是同一个盘上的改名，
//! 不会出现「跨盘复制到一半」的状态。
//!
//! ⚠ **换托管根时不搬它**（`managed::migrate_files` 只搬 `App::ALL`）。酒馆配置里记的是绝对路径，
//! 老位置照旧能用；换了根之后再「下载安装」会装到新根下的一份新的。KNOWN-ISSUES 里写着。
//!
//! # 「是不是面板装的」只认标记
//!
//! 更新、卸载**只碰带标记的目录**。使用者自己的那份（git 克隆的、解压的、别的启动器装的）
//! 面板只起、只停、只读，不改一个文件 —— 目录名叫什么、在哪个盘都不作数。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{GateError, Result};

pub const DIR: &str = "sillytavern";
pub const PREVIOUS: &str = "sillytavern.previous";
pub const STAGING: &str = "sillytavern.staging";
pub const WORK: &str = "sillytavern.work";
pub const LAUNCHER: &str = "Start.bat";
pub const MARKER: &str = ".qb-managed.json";

/// 某个托管根下，面板装的那份酒馆该在哪。
pub fn managed_dir_in(root: &Path) -> PathBuf {
    root.join(DIR)
}

/// 当前托管根下，面板装的那份酒馆该在哪（在不在都返回路径）。
pub fn managed_dir() -> PathBuf {
    managed_dir_in(&super::managed::root())
}

/// 跟 `dir` 同级的某个工作目录。`dir` 没有父目录（根目录）时退回 `dir` 自己下面 ——
/// 实际装不到那种地方，这只是不让函数 panic。
fn sibling(dir: &Path, name: &str) -> PathBuf {
    dir.parent()
        .map(|p| p.join(name))
        .unwrap_or_else(|| dir.join(name))
}

pub fn previous_of(dir: &Path) -> PathBuf {
    sibling(dir, PREVIOUS)
}

pub fn staging_of(dir: &Path) -> PathBuf {
    sibling(dir, STAGING)
}

pub fn work_of(dir: &Path) -> PathBuf {
    sibling(dir, WORK)
}

pub fn launcher_in(dir: &Path) -> PathBuf {
    dir.join(LAUNCHER)
}

/// 面板装的那份里放的标记。**只用来认「这是面板装的」和显示**，不参与任何判据以外的决定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "TavernMarker")]
pub struct Marker {
    /// SillyTavern 的版本号（`package.json` 的 `version`）。
    pub version: String,
    /// 装的是哪个发布标签。
    pub tag: String,
    /// RFC 3339。
    pub installed_at: String,
    /// 下载下来的压缩包的 SHA-256。**只留痕，不作判据**（官方不公布哈希）。
    pub archive_sha256: String,
    /// 从哪个地址下的。
    pub source: String,
}

pub fn read_marker(dir: &Path) -> Option<Marker> {
    let bytes = std::fs::read(dir.join(MARKER)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn write_marker(dir: &Path, m: &Marker) -> Result<()> {
    std::fs::write(dir.join(MARKER), serde_json::to_vec_pretty(m)?)?;
    Ok(())
}

pub fn is_managed(dir: &Path) -> bool {
    !dir.as_os_str().is_empty() && read_marker(dir).is_some()
}

/// `package.json` 里用得到的两项。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Package {
    pub name: Option<String>,
    pub version: Option<String>,
    /// `engines.node`，原样（例如 `>= 20`）。
    pub node_engine: Option<String>,
}

/// 纯函数：从 `package.json` 的正文里读。读不出的项是 `None`，不猜。
pub fn parse_package(json: &str) -> Package {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Package::default();
    };
    let s = |p: &str| {
        v.pointer(p)
            .and_then(serde_json::Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    Package {
        name: s("/name"),
        version: s("/version"),
        node_engine: s("/engines/node"),
    }
}

pub fn read_package(dir: &Path) -> Option<Package> {
    let text = std::fs::read_to_string(dir.join("package.json")).ok()?;
    Some(parse_package(&text))
}

/// 这个目录像不像一份 SillyTavern：有 `server.js`，`package.json` 的 `name` 含 sillytavern。
/// 跟 `plugins::tavern_locate::is_sillytavern_root` 同一个认法（那边给扫盘用，这边给安装用）。
pub fn looks_like_sillytavern(dir: &Path) -> bool {
    dir.join("server.js").is_file()
        && read_package(dir)
            .and_then(|p| p.name)
            .is_some_and(|n| n.to_lowercase().contains("sillytavern"))
}

/// 纯函数：`config.yaml` 顶层的 `port:`。**只读**，面板不改酒馆自己的配置。
///
/// 只认顶格写的那一行（`port: 8000`，可带引号、行尾注释）—— 缩进了的是别的表里的 `port`。
/// 认不出就是 `None`，界面上说「读不出」，不按 8000 猜。
pub fn parse_config_port(yaml: &str) -> Option<u16> {
    for line in yaml.lines() {
        let Some(rest) = line.strip_prefix("port:") else {
            continue;
        };
        let value = rest.split('#').next().unwrap_or("").trim();
        let value = value.trim_matches(|c| c == '"' || c == '\'');
        return value.parse::<u16>().ok().filter(|p| *p != 0);
    }
    None
}

pub fn config_port(dir: &Path) -> Option<u16> {
    parse_config_port(&std::fs::read_to_string(dir.join("config.yaml")).ok()?)
}

/// 目录里有没有东西。读不开当成「有」—— 宁可拒绝，也不往读不开的目录里装。
pub fn non_empty(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(mut r) => r.next().is_some(),
        Err(e) => e.kind() != std::io::ErrorKind::NotFound,
    }
}

/// 装之前核一遍目标：空的、或者是面板上次装的，才往里放。
pub fn check_target(dir: &Path) -> Result<()> {
    if non_empty(dir) && !is_managed(dir) {
        return Err(GateError::Other(format!(
            "{} 里已经有东西，而且不是面板装的 —— 面板不覆盖它。挪走它，或者在设置里换一个托管目录。",
            dir.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_working_directories_are_siblings_of_the_install() {
        // 就位与回滚都是同一个父目录下的改名 —— 不跨盘，不会停在复制到一半。
        let dir = PathBuf::from(r"D:\apps\sillytavern");
        assert_eq!(
            previous_of(&dir),
            PathBuf::from(r"D:\apps\sillytavern.previous")
        );
        assert_eq!(
            staging_of(&dir),
            PathBuf::from(r"D:\apps\sillytavern.staging")
        );
        assert_eq!(work_of(&dir), PathBuf::from(r"D:\apps\sillytavern.work"));
        assert_eq!(
            launcher_in(&dir),
            PathBuf::from(r"D:\apps\sillytavern\Start.bat")
        );
        assert_eq!(
            managed_dir_in(Path::new(r"D:\apps")),
            PathBuf::from(r"D:\apps\sillytavern")
        );
    }

    #[test]
    fn package_json_gives_version_and_the_node_requirement_as_written() {
        let p = parse_package(
            r#"{"name":"sillytavern","version":"1.18.0","engines":{"node":">= 20"}}"#,
        );
        assert_eq!(p.name.as_deref(), Some("sillytavern"));
        assert_eq!(p.version.as_deref(), Some("1.18.0"));
        assert_eq!(p.node_engine.as_deref(), Some(">= 20"));
        // 缺项是「不知道」，不是编一个默认值。
        let bare = parse_package(r#"{"name":"sillytavern"}"#);
        assert_eq!((bare.version, bare.node_engine), (None, None));
        assert_eq!(parse_package("not json"), Package::default());
    }

    #[test]
    fn only_a_top_level_port_line_counts() {
        assert_eq!(parse_config_port("listen: false\nport: 8000\n"), Some(8000));
        assert_eq!(parse_config_port("port: \"8123\" # 改过\n"), Some(8123));
        // 缩进的是别的表里的 port（例如某个扩展的配置），不是酒馆自己的。
        assert_eq!(parse_config_port("sso:\n  port: 9000\n"), None);
        assert_eq!(parse_config_port("port: soon\n"), None);
        assert_eq!(parse_config_port("port: 0\n"), None);
        assert_eq!(parse_config_port(""), None);
    }

    #[test]
    fn a_directory_is_managed_only_when_it_carries_our_marker() {
        let root = std::env::temp_dir().join(format!("qb-st-marker-{}", std::process::id()));
        let dir = root.join("sillytavern");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!is_managed(&dir), "没有标记的目录不是面板装的");
        // 目录里有东西、又没有标记：不往里装。
        std::fs::write(dir.join("server.js"), "").unwrap();
        assert!(check_target(&dir).is_err());
        let m = Marker {
            version: "1.18.0".into(),
            tag: "1.18.0".into(),
            installed_at: "2026-09-29T00:00:00Z".into(),
            archive_sha256: "00".into(),
            source: "https://github.com/SillyTavern/SillyTavern/archive/refs/tags/1.18.0.zip"
                .into(),
        };
        write_marker(&dir, &m).unwrap();
        assert!(is_managed(&dir));
        assert_eq!(read_marker(&dir), Some(m));
        assert!(check_target(&dir).is_ok(), "面板自己装的那份可以重装");
        // 空路径永远不是（配置里没填时就是空的）。
        assert!(!is_managed(Path::new("")));
        // 不存在的目录可以装。
        assert!(check_target(&root.join("nope")).is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }
}
