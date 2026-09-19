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
    vec![
        plugins::sillytavern::status(),
        plugins::codex_egress::status(),
    ]
}

// ------------------------------------------------------------ Codex 出站与换出口插件

#[tauri::command]
pub fn codex_egress_status() -> plugins::PluginStatus {
    plugins::codex_egress::status()
}

#[tauri::command]
pub fn codex_egress_config() -> plugins::codex_egress::EgressConfig {
    plugins::codex_egress::load_config()
}

#[tauri::command]
pub async fn codex_egress_config_save(cfg: plugins::codex_egress::EgressConfig) -> Result<()> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(move || plugins::codex_egress::save_config(&cfg))
        .await
        .map_err(|e| crate::error::GateError::Other(e.to_string()))?
}

/// 启动插件（它会接管当前激活槽位 / 默认 `~/.codex` 的 Codex 配置并起服务）。
///
/// ⛔ 与核心的官方 turn-state 识别互斥（双向，另一半在 `station_turnstate_enable`）。
/// 0.24.7 起两边**都改同一个槽位的 `config.toml`**（识别改 `model_provider`，插件也接管它），
/// 同时开就是两个程序抢同一个文件。识别开着时请先在账户页「识别」弹窗里点「关闭识别」。
/// 看的是落盘的 marker（`turnstate_ops::takeover`），不只看内存里的路由状态 ——
/// 面板重启后后者归零，前者才是真相（启动时会自动关闭并恢复，这里再兜一道底）。
#[tauri::command]
pub async fn codex_egress_start(state: tauri::State<'_, AppState>) -> Result<String> {
    let _guard = operations::exclusive().await?;
    let armed = state
        .station
        .lock()
        .map_err(|_| crate::error::GateError::Other("中转站状态损坏".into()))?
        .official_codex_armed();
    if armed || qb_app::usecase::turnstate_ops::takeover().is_some() {
        return Err(crate::error::GateError::Other(
            "账户页的「识别（turn-state）」还开着。它和这个插件都要改同一个 Codex 槽位的配置，一次只能开一个：到账户页「识别」弹窗点「关闭识别」（会恢复槽位配置），再启动插件。"
                .into(),
        ));
    }
    tokio::task::spawn_blocking(plugins::codex_egress::start)
        .await
        .map_err(|e| crate::error::GateError::Other(e.to_string()))?
}

/// 一键下载、校验（SHA256SUMS）、解压并登记插件。进度走 `egress-install` 任务事件。
#[tauri::command]
pub async fn codex_egress_install(
    app: tauri::AppHandle,
) -> Result<plugins::codex_egress::EgressInstall> {
    let _guard = operations::exclusive().await?;
    let rep = Reporter::new(app, events::TASK_EGRESS_INSTALL, 5);
    match plugins::codex_egress::install(&rep).await {
        Ok(done) => Ok(done),
        Err(e) => {
            rep.fail(&e.to_string());
            Err(e)
        }
    }
}

/// 停止插件：结束本面板起的那份，再用它自己的 `restore` 把 Codex 配置恢复回去。
#[tauri::command]
pub async fn codex_egress_stop() -> Result<Vec<String>> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(plugins::codex_egress::stop)
        .await
        .map_err(|e| crate::error::GateError::Other(e.to_string()))?
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

/// 在本机找酒馆与桥接装在哪。**只读，不写配置** —— 采用哪一条由使用者点。
///
/// 故意不拿 `operations::exclusive`：它只读盘，跟切号 / 装机那几个互斥操作
/// 不冲突，而深扫最长要跑 90 秒 —— 占着那把锁会把别的操作全堵住。
#[tauri::command]
pub async fn tavern_locate(deep: bool) -> Result<plugins::tavern_locate::TavernSurvey> {
    plugins::sillytavern::locate(deep).await
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
