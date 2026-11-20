//! 插件文件夹在哪（2026-11-19）：面板「扩展 → 浏览器助手」页的「打开插件文件夹」「复制路径」指的那个。
//!
//! 安装包把 `npm run build:browser-extension` 的产物装进安装目录下的 `browser-extension` 文件夹
//! （`tauri.conf.json` 的 `bundle.resources`）；开发版用仓库里构建出来的那份。**位置由调用方给**
//! （命令层从 Tauri 的资源目录拼，开发版另给仓库路径），这里只按顺序找、核对是不是那份插件。
//! 不写死任何路径，找不到就照实说找过哪里、下一步做什么。

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

/// 插件文件夹在安装目录、开发构建目录里都叫这个名字。
pub const FOLDER_NAME: &str = "browser-extension";
/// 插件 `manifest.json` 里的名字。核对「找到的是不是这份插件」用。
const MANIFEST_NAME: &str = "QB Gate 浏览器助手";

/// 这份插件文件夹是哪里来的。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExtensionFolderSource {
    /// 安装包带的，在面板安装目录下。
    Installed,
    /// 开发版：仓库里 `npm run build:browser-extension` 构建出来的那份。
    Development,
}

/// 「打开插件文件夹」「复制路径」要的东西。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct BrowserExtensionFolder {
    /// 找到的插件文件夹（里面有这份插件的 `manifest.json`）。没找到是 `null`。
    pub path: Option<String>,
    pub source: Option<ExtensionFolderSource>,
    /// 文件夹里那份插件的版本（`manifest.json` 的 `version`）。
    pub version: Option<String>,
    /// 没找到时一句能照着做的话。
    pub problem: Option<String>,
    /// 找过的位置（没找到时给人核对用）。
    pub searched: Vec<String>,
}

/// 按顺序找第一份像样的插件文件夹。`candidates` 由调用方按优先级给。
pub fn locate(candidates: &[(PathBuf, ExtensionFolderSource)]) -> BrowserExtensionFolder {
    let mut searched = Vec::new();
    let mut odd = Vec::new();
    for (dir, source) in candidates {
        searched.push(dir.display().to_string());
        match read_manifest(dir) {
            Probe::Ours(version) => {
                return BrowserExtensionFolder {
                    path: Some(dir.display().to_string()),
                    source: Some(*source),
                    version,
                    problem: None,
                    searched,
                }
            }
            Probe::Missing => {}
            Probe::NotOurs(why) => odd.push(format!("{}：{why}", dir.display())),
        }
    }
    let mut problem = String::from(
        "没找到插件文件夹。安装版的话，插件随安装包放在面板安装目录下的 browser-extension 文件夹里：\
         重新运行一次 QB Gate 安装包（覆盖安装即可）就会放回来；也可以从 GitHub 发布页下载 \
         QB-Gate-Browser-Extension.zip，解压后在 Chrome 里加载那个文件夹。\
         从源码运行的话，先在仓库里跑 npm run build:browser-extension。",
    );
    if !odd.is_empty() {
        problem.push_str(&format!(
            "另外，这些位置有文件夹但不是这份插件：{}。",
            odd.join("；")
        ));
    }
    BrowserExtensionFolder {
        path: None,
        source: None,
        version: None,
        problem: Some(problem),
        searched,
    }
}

enum Probe {
    Ours(Option<String>),
    Missing,
    NotOurs(&'static str),
}

fn read_manifest(dir: &Path) -> Probe {
    let file = dir.join("manifest.json");
    let Ok(text) = std::fs::read_to_string(&file) else {
        return if dir.exists() {
            Probe::NotOurs("里面没有 manifest.json")
        } else {
            Probe::Missing
        };
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Probe::NotOurs("manifest.json 读不懂");
    };
    if value["name"].as_str() != Some(MANIFEST_NAME) {
        return Probe::NotOurs("manifest.json 不是 QB Gate 浏览器助手");
    }
    Probe::Ours(value["version"].as_str().map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "qb-ext-folder-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn plugin(dir: &Path, name: &str, version: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("manifest.json"),
            format!(r#"{{"manifest_version":3,"name":"{name}","version":"{version}"}}"#),
        )
        .unwrap();
    }

    #[test]
    fn the_first_folder_that_holds_our_manifest_wins() {
        let root = temp();
        let dev = root.join("dev").join(FOLDER_NAME);
        let installed = root.join("install").join(FOLDER_NAME);
        plugin(&installed, MANIFEST_NAME, "0.2.0");
        // 开发目录排在前面但还没构建：跳过它，用安装目录那份。
        let found = locate(&[
            (dev.clone(), ExtensionFolderSource::Development),
            (installed.clone(), ExtensionFolderSource::Installed),
        ]);
        assert_eq!(found.path.as_deref(), Some(installed.to_str().unwrap()));
        assert_eq!(found.source, Some(ExtensionFolderSource::Installed));
        assert_eq!(found.version.as_deref(), Some("0.2.0"));
        assert!(found.problem.is_none());
        assert_eq!(found.searched.len(), 2);
        plugin(&dev, MANIFEST_NAME, "0.2.1");
        let found = locate(&[
            (dev.clone(), ExtensionFolderSource::Development),
            (installed, ExtensionFolderSource::Installed),
        ]);
        assert_eq!(found.source, Some(ExtensionFolderSource::Development));
        assert_eq!(found.version.as_deref(), Some("0.2.1"));
        std::fs::remove_dir_all(root).ok();
    }

    /// 找不到不是静默失败：给一句下一步，并说清哪些位置有东西但不是这份插件。
    #[test]
    fn nothing_found_says_where_it_looked_and_what_to_do() {
        let root = temp();
        let empty = root.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let other = root.join("other");
        plugin(&other, "别的插件", "1.0");
        let found = locate(&[
            (root.join("missing"), ExtensionFolderSource::Development),
            (empty, ExtensionFolderSource::Installed),
            (other, ExtensionFolderSource::Installed),
        ]);
        assert!(found.path.is_none() && found.source.is_none());
        let problem = found.problem.unwrap();
        assert!(problem.starts_with("没找到插件文件夹"));
        assert!(problem.contains("npm run build:browser-extension"));
        assert!(problem.contains("QB-Gate-Browser-Extension.zip"));
        assert!(problem.contains("里面没有 manifest.json"));
        assert!(problem.contains("不是 QB Gate 浏览器助手"));
        assert_eq!(found.searched.len(), 3);
        std::fs::remove_dir_all(root).ok();
    }

    /// 仓库里插件的名字改了，这里认不出、界面就一直说「没找到」。
    #[test]
    fn the_repository_manifest_carries_the_name_we_look_for() {
        let manifest = include_str!("../../../../browser-extension/manifest.json");
        let value: serde_json::Value = serde_json::from_str(manifest).unwrap();
        assert_eq!(value["name"], MANIFEST_NAME);
    }
}
