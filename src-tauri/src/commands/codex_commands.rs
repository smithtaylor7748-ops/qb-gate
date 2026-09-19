//! Codex desktop commands. All mutations require an explicit UI action.
use crate::{
    error::{GateError, Result},
    operations,
};
use qb_accounts::codex;
use qb_install::install::codex_desktop;

#[tauri::command]
pub async fn codex_accounts() -> Result<codex::CodexAccounts> {
    tokio::task::spawn_blocking(|| codex::list(&codex::root()))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}
#[tauri::command]
pub async fn codex_desktop_status() -> Result<codex_desktop::CodexDesktop> {
    tokio::task::spawn_blocking(codex_desktop::detect)
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}
#[tauri::command]
pub async fn codex_close() -> Result<()> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(codex_desktop::close)
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}
/// 以管理员重新注册 Codex 打包应用（更新后打包服务需管理员注册，否则启动一律拒绝访问）。
/// 只做重新注册，会弹 UAC；使用者不授权就返回「已取消」。
///
/// ⛔ **故意不拿 `operations::exclusive`**：它只是起一个提权的外部命令、不动面板自己的
/// 任何状态，而 UAC 提示可能等上几分钟 —— 看门狗每一轮都要拿同一把锁，握着锁等 UAC
/// 等于让门禁在这段时间里停摆。
#[tauri::command]
pub async fn codex_repair_registration() -> Result<()> {
    tokio::task::spawn_blocking(codex_desktop::repair_registration)
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}
#[tauri::command]
pub async fn codex_create(label: String) -> Result<String> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(move || codex::create(&codex::root(), &label))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}
#[tauri::command]
pub async fn codex_switch(id: String) -> Result<()> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(move || qb_app::usecase::codex_accounts::switch(&id))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}
#[tauri::command]
pub async fn codex_launch(id: String) -> Result<()> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(move || qb_app::usecase::codex_accounts::launch(&id))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}
#[tauri::command]
pub async fn codex_archive(id: String) -> Result<()> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(move || codex::archive(&codex::root(), &id))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}
#[tauri::command]
pub async fn codex_usage(id: String, days: u32) -> Result<codex::usage::CodexUsage> {
    if ![0, 1, 7].contains(&days) {
        return Err(GateError::Other("不支持的用量时间范围".into()));
    }
    tokio::task::spawn_blocking(move || {
        let dir = codex::directory(&codex::root(), &id)?;
        codex::usage::read(&dir.join("home"), days)
    })
    .await
    .map_err(|e| GateError::Other(e.to_string()))?
}
