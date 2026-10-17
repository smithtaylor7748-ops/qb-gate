//! 配置快照的命令。
//!
//! 原来这里还有「统一档案」（`profile_*`：把账号 + 供应商 + 时区当一个整体切过去）。
//! 保存、删除、应用三条早就只回一句「旧版混合配置入口已停用」，列表与草稿也没有界面在读，
//! 2026-09-27 连同 `qb_app::profile` 一起删了。

use crate::{error::Result, operations, snapshot, tray, AppState};

// ------------------------------------------------------------------ 快照

#[tauri::command]
pub fn snapshot_list() -> Vec<snapshot::SnapshotEntry> {
    snapshot::list()
}

#[tauri::command]
pub async fn snapshot_create(note: String) -> Result<snapshot::SnapshotEntry> {
    let _guard = operations::exclusive_soon().await?;
    snapshot::create(&note)
}

/// 回滚。**恢复之前会先把现状再存一份** —— 回滚本身也是个能出错的操作。
#[tauri::command]
pub async fn snapshot_restore(
    app: tauri::AppHandle,
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String> {
    let _guard = operations::exclusive().await?;
    let _ = &state;
    let detail = snapshot::restore(&id)?;
    tray::refresh(&app);
    Ok(detail)
}

#[tauri::command]
pub async fn snapshot_remove(id: String) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    snapshot::remove(&id)
}
