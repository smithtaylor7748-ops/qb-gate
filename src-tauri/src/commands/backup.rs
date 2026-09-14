//! 快照备份与统一档案相关的命令。

use crate::{
    error::{GateError, Result},
    operations, profile, snapshot, tray, AppState,
};

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

/// 账户要变之前先清场：关掉全部 Claude（v0.9.0「切换账户 = 先清场」）。
///
/// 只在**真的要换账户**时才清 —— `target` 跟当前激活的槽位一样（或者根本没指定账户）
/// 就什么都不关：恢复一份不涉及换号的快照，不该顺手把人正在聊的对话全关了。
/// 返回关掉了几个；没清场是 `None`。
/// 回滚。**恢复之前会先把现状再存一份** —— 回滚本身也是个能出错的操作。
///
/// 快照记着当时的账户；要换号的话先清场（见 [`clear_before_account_change`]）。
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

#[tauri::command]
pub fn snapshot_dir(id: String) -> Result<String> {
    Ok(snapshot::dir_of(&id)?.display().to_string())
}

// ------------------------------------------------------------------ 档案

#[tauri::command]
pub fn profile_list() -> profile::ProfileStore {
    profile::load()
}

#[tauri::command]
pub fn profile_save(item: profile::Profile) -> Result<String> {
    let _ = &item;
    Err(GateError::Other(
        "旧版混合配置入口已停用，请使用官方账户、中转站或独立启动方案".into(),
    ))
}

#[tauri::command]
pub fn profile_remove(id: String) -> Result<()> {
    let _ = &id;
    Err(GateError::Other(
        "旧版混合配置入口已停用，请使用官方账户、中转站或独立启动方案".into(),
    ))
}

/// 按当前状态生成一个档案草稿（不落盘，交给界面确认后再存）。
#[tauri::command]
pub fn profile_capture(name: String) -> profile::Profile {
    profile::capture(&name)
}

/// 应用一个档案。**只由界面点击触发，不要加任何自动调用点** ——
/// 它会切账户，加了就变成自动轮换账户，直接踩政策线。
///
/// 档案要换号的话先清场（见 [`clear_before_account_change`]），跟切换对话框同一个语义。
#[tauri::command]
pub async fn profile_apply(
    app: tauri::AppHandle,
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<profile::ApplyReport> {
    let _ = (&app, &id, &state);
    Err(GateError::Other(
        "旧版混合配置入口已停用，请使用官方账户、中转站或独立启动方案".into(),
    ))
}
