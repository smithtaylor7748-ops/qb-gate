//! Chrome 浏览器助手（协议 2）。连接桥在 `qb_app::browser_link`，这里做它不做的事：
//! 打开默认浏览器、写审计日志、把面板的门禁状态与内嵌资源交给它。
//!
//! 「连接浏览器」「一键重连」只由界面上的点击触发；面板启动时只在**已经配对过**时恢复监听。
//!
//! 另有插件文件夹的两条（2026-11-19）：安装包把插件装进安装目录下的 `browser-extension`
//! （`tauri.conf.json` 的 `bundle.resources`），「打开插件文件夹」「复制路径」指的就是它；
//! 开发版先找仓库里 `npm run build:browser-extension` 构建出来的那份。位置一律从 Tauri 的
//! 资源目录拼，不写死。

use std::path::PathBuf;
use std::sync::Arc;

use crate::{
    audit,
    error::{GateError, Result},
    AppState,
};
use qb_app::browser_link::folder::{self, BrowserExtensionFolder, ExtensionFolderSource};
use qb_app::{browser_link, browser_probe::AssetSource};

struct CompanionAssets(tauri::AppHandle);

impl AssetSource for CompanionAssets {
    fn get(&self, path: &str) -> Option<(String, Vec<u8>)> {
        let asset = self.0.asset_resolver().get(path.to_string())?;
        Some((asset.mime_type, asset.bytes))
    }
}

fn link(app: &tauri::AppHandle, state: &AppState) -> &'static browser_link::Link {
    browser_link::production(
        state.gate.clone(),
        Arc::new(CompanionAssets(app.clone())),
        // 事件文字里没有口令、认领码，只有「配对了谁、端口换成几」。
        Arc::new(|message: &str| audit::write(message)),
    )
}

/// 认领页地址交给默认浏览器。打不开不算失败：界面给链接让使用者换个浏览器打开。
fn open_in_browser(
    app: &tauri::AppHandle,
    mut start: browser_link::BrowserLinkStart,
) -> browser_link::BrowserLinkStart {
    use tauri_plugin_opener::OpenerExt;
    if let Some(url) = start.url.clone() {
        start.opened = app.opener().open_url(url, None::<&str>).is_ok();
    }
    start
}

#[tauri::command]
pub async fn browser_companion_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<browser_link::BrowserLinkStatus> {
    Ok(link(&app, &state).status().await)
}

/// 「连接浏览器」：开起监听（没开的话），发一个一次性认领码，用默认浏览器打开认领页。
#[tauri::command]
pub async fn browser_companion_connect(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<browser_link::BrowserLinkStart> {
    let start = link(&app, &state).connect().await?;
    Ok(open_in_browser(&app, start))
}

/// 「一键重连」：先试着拿回原来的端口，再打开认领页（让换过端口的插件学到新端口）。
#[tauri::command]
pub async fn browser_companion_reconnect(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<browser_link::BrowserLinkStart> {
    let start = link(&app, &state).reconnect().await?;
    Ok(open_in_browser(&app, start))
}

/// 解除一个浏览器的配对：它的口令立刻作废、连接立刻断开。最后一个解除后不再监听任何端口。
#[tauri::command]
pub async fn browser_companion_unpair(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    pairing_id: String,
) -> Result<browser_link::BrowserLinkStatus> {
    link(&app, &state).unpair(&pairing_id).await
}

/// 按优先级列出插件文件夹可能在的地方。
///
/// 开发版（`debug_assertions`）先看仓库里构建出来的那份 —— 改了插件源码、`cargo` 重编时
/// `build.rs` 会重新构建它；再看资源目录（`tauri-build` 也会复制一份到 `target` 里）。
/// 安装版只看资源目录，也就是面板安装目录。
fn folder_candidates(app: &tauri::AppHandle) -> Vec<(PathBuf, ExtensionFolderSource)> {
    use tauri::Manager;
    let mut out = Vec::new();
    #[cfg(debug_assertions)]
    out.push((
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("build")
            .join(folder::FOLDER_NAME),
        ExtensionFolderSource::Development,
    ));
    if let Ok(resources) = app.path().resource_dir() {
        out.push((
            resources.join(folder::FOLDER_NAME),
            ExtensionFolderSource::Installed,
        ));
    }
    out
}

fn locate_folder(app: &tauri::AppHandle) -> BrowserExtensionFolder {
    let mut found = folder::locate(&folder_candidates(app));
    // `..` 留在路径里，复制出去的那串不好认；能化简就化简（`canonicalize` 会加 `\\?\` 前缀，去掉）。
    if let Some(path) = found.path.as_deref() {
        if let Ok(clean) = tidy_path(PathBuf::from(path)) {
            found.path = Some(clean);
        }
    }
    found
}

fn tidy_path(path: PathBuf) -> std::io::Result<String> {
    let full = std::fs::canonicalize(path)?.display().to_string();
    Ok(full
        .strip_prefix(r"\\?\")
        .map(str::to_string)
        .unwrap_or(full))
}

/// 插件文件夹在哪（只读，不打开任何东西）。「复制路径」用它。
#[tauri::command]
pub fn browser_companion_folder(app: tauri::AppHandle) -> BrowserExtensionFolder {
    locate_folder(&app)
}

/// 「打开插件文件夹」：在资源管理器里打开它。找不到、打不开都照实报错，附下一步。
#[tauri::command]
pub fn browser_companion_open_folder(app: tauri::AppHandle) -> Result<BrowserExtensionFolder> {
    use tauri_plugin_opener::OpenerExt;
    let found = locate_folder(&app);
    let Some(path) = found.path.clone() else {
        return Err(GateError::Other(
            found.problem.unwrap_or_else(|| "没找到插件文件夹。".into()),
        ));
    };
    app.opener().open_path(&path, None::<&str>).map_err(|e| {
        GateError::Other(format!(
            "打不开插件文件夹（{e}）。点「复制路径」，在资源管理器地址栏里粘贴打开：{path}"
        ))
    })?;
    Ok(found)
}

/// 面板启动时调：配对过才恢复监听；没配对过什么都不做（不开任何端口）。
pub async fn resume_on_start(app: tauri::AppHandle) {
    use tauri::Manager;
    let state = app.state::<AppState>();
    let status = link(&app, &state).resume().await;
    if let Some(problem) = status.problem {
        audit::write(&format!("浏览器助手：启动时没能恢复联动：{problem}"));
    }
}

/// 面板正常退出：先给每个连着的浏览器发「面板下线」，再关监听。最多等两秒。
///
/// 在一条新线程上等：不管退出回调本身是不是跑在异步运行时里，都不会撞上「运行时里再开运行时」，
/// 也就不会因为它而让后面的重锁跑不到。
pub fn stop_on_exit() {
    let Some(link) = browser_link::production_if_created() else {
        return;
    };
    let worker = std::thread::spawn(move || {
        tauri::async_runtime::block_on(async {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                link.stop(browser_link::Ending::PanelExit),
            )
            .await;
        })
    });
    let _ = worker.join();
}
