//! 插件（本机应用接入）与酒馆桥接相关的命令。

use crate::sink::ProgressSink;
use crate::{
    error::Result,
    events::{self, Reporter},
    gate, operations, plugins, AppState,
};

// ------------------------------------------------------------ 插件

#[tauri::command]
pub fn plugin_list() -> Vec<plugins::PluginStatus> {
    vec![plugins::sillytavern::status()]
}

#[tauri::command]
pub fn plugin_catalog_status() -> plugins::OfficialCatalogStatus {
    plugins::official_catalog_status()
}

/// 启动酒馆：先过 IP 门禁，再拉起桥接与酒馆，最后挂上看门狗。
///
/// 门禁不过就一个进程都不起 —— 这是接管启动链之后仍要守住的那条线。
#[tauri::command]
pub async fn plugin_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String> {
    let _guard = operations::exclusive().await?;
    gate::open_authorized("sillytavern", &state.gate).await?;

    let rep = Reporter::new(app.clone(), events::TASK_TAVERN_START, 5);
    let url = match plugins::sillytavern::start(&rep).await {
        Ok(u) => u,
        Err(e) => {
            // 起失败就把租约还回去，不能让门开着。
            let _ = gate::release_holder(&state.gate, "sillytavern");
            rep.fail(&e.to_string());
            return Err(e);
        }
    };

    crate::app::start_watchdog(gate::watchdog::WatchMode::Cli, &state, Some(app));

    Ok(url)
}

#[tauri::command]
pub async fn plugin_stop(state: tauri::State<'_, AppState>) -> Result<Vec<String>> {
    let _guard = operations::exclusive().await?;
    let done = plugins::sillytavern::stop().await?;
    gate::release_holder(&state.gate, "sillytavern")?;
    Ok(done)
}

#[tauri::command]
pub fn tavern_config() -> plugins::sillytavern::TavernConfig {
    plugins::sillytavern::load_config()
}

#[tauri::command]
pub async fn tavern_config_save(cfg: plugins::sillytavern::TavernConfig) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    plugins::sillytavern::save_config(&cfg)
}

// ------------------------------------------------------------ 酒馆资产

#[tauri::command]
pub fn tavern_assets() -> Vec<plugins::tavern_assets::CategoryListing> {
    plugins::tavern_assets::list_all()
}

#[tauri::command]
pub async fn tavern_backup() -> Result<plugins::tavern_assets::BackupEntry> {
    let _guard = operations::exclusive_soon().await?;
    plugins::tavern_assets::backup()
}

#[tauri::command]
pub fn tavern_backups() -> Vec<plugins::tavern_assets::BackupEntry> {
    plugins::tavern_assets::list_backups()
}

#[tauri::command]
pub async fn tavern_restore(backup_id: String) -> Result<String> {
    let _guard = operations::exclusive_soon().await?;
    plugins::tavern_assets::restore(&backup_id)
}
