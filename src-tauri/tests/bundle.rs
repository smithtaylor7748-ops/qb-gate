//! `tauri.conf.json` 里跟代码对着的值（2026-10-27，中转站重构阶段 7 · X7）。
//!
//! 这两处改错了编译照过、单测照绿，症状只在装好的面板上出现（而且是卸载之后、开机的时候），
//! 所以对着配置文件本身钉。

use std::path::Path;

fn config() -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let text = std::fs::read_to_string(&path).expect("读不到 tauri.conf.json");
    serde_json::from_str(&text).expect("tauri.conf.json 不是合法 JSON")
}

/// 开机自启登记的值名必须就是产品名：Tauri 的 NSIS 卸载程序只删 `HKCU\…\Run` 下名叫
/// `${PRODUCTNAME}` 的那一项（`qb_platform::autostart` 的模块头）。差一个字，卸掉面板之后
/// 每次开机还在起一个已经不存在的 exe。
#[test]
fn the_autostart_entry_is_named_after_the_product_so_the_uninstaller_removes_it() {
    assert_eq!(
        config()["productName"],
        qb_platform::autostart::VALUE_NAME,
        "改了 productName 就要一起改 autostart::VALUE_NAME（已经登记过的那一项要另想办法迁）"
    );
}

/// 主窗口藏着建、页面加载完才露面（`src-tauri/src/window.rs` 的模块头）。改回 `visible: true`，
/// 开机自启就会先闪一下窗口再藏起来。
#[test]
fn the_main_window_starts_hidden_and_the_panel_reveals_it() {
    let windows = config()["app"]["windows"].clone();
    let main = windows
        .as_array()
        .expect("app.windows 必须是数组")
        .iter()
        .find(|w| w["label"] == "main")
        .cloned()
        .expect("要有 main 窗口");
    assert_eq!(main["visible"], false);
}

/// 浏览器助手插件随安装包走（2026-11-19）：`build/browser-extension` 装进安装目录的 `browser-extension`。
/// 面板「打开插件文件夹」在安装版里按资源目录拼这个名字（`browser_link::folder::FOLDER_NAME`），
/// 两头对不上，安装版就永远报「没找到插件文件夹」。
#[test]
fn the_installer_carries_the_browser_extension_folder() {
    let config = config();
    let resources = config["bundle"]["resources"]
        .as_object()
        .expect("bundle.resources 必须是映射");
    assert_eq!(
        resources
            .get("../build/browser-extension/")
            .and_then(|v| v.as_str()),
        Some(format!("{}/", qb_app::browser_link::folder::FOLDER_NAME).as_str()),
        "插件分发目录要装进安装目录的 browser-extension 文件夹"
    );
    // 打包前先构建插件（build.rs 也会跑，这一句让 `npm run tauri build` 的输出里看得见）。
    let before = config["build"]["beforeBuildCommand"].as_str().unwrap_or("");
    assert!(
        before.contains("build:browser-extension"),
        "beforeBuildCommand 要构建插件：{before}"
    );
}
