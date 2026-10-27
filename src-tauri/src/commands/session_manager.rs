//! 会话管理器（中转站重构阶段 6 · S1）。逻辑全在 `qb_app::usecase::session_ops`，这里只搬参数。
//!
//! 读会话文件是成百上千个文件的 IO：一律 `async` + `spawn_blocking`，不堵 tokio 的 worker
//! （同 `accounts_tokens` 的理由）。

use crate::error::{GateError, Result};
use qb_app::usecase::session_ops::{self, SessionDetail, SessionList, SessionQuery};
use qb_extensions::plugins::tavern_library::TrashEntry;

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(job)
        .await
        .map_err(|e| GateError::Other(format!("读会话的任务异常结束：{e}")))?
}

/// 会话列表：`text` 搜标题 / 项目目录 / 会话 id（`deep` 时连内容一起搜），`source` 只看某个目标。
#[tauri::command]
pub async fn sessions_list(
    text: String,
    source: Option<String>,
    deep: bool,
    offset: u32,
    limit: u32,
) -> Result<SessionList> {
    blocking(move || {
        session_ops::list(&SessionQuery {
            text,
            source,
            deep,
            offset,
            limit,
        })
    })
    .await
}

/// 读一个会话（分页），带恢复命令。
#[tauri::command]
pub async fn sessions_read(id: String, offset: u32, limit: u32) -> Result<SessionDetail> {
    blocking(move || session_ops::read(&id, offset, limit)).await
}

/// 导出成 Markdown，写到使用者在保存框里选的地方（只认 `.md`）。
#[tauri::command]
pub async fn sessions_export(id: String, path: String) -> Result<()> {
    blocking(move || session_ops::export(&id, std::path::Path::new(&path))).await
}

/// 挪进面板的回收站（两分钟内还在写的不让删）。
#[tauri::command]
pub async fn sessions_delete(id: String) -> Result<TrashEntry> {
    let entry = blocking(move || session_ops::delete(&id)).await?;
    crate::audit::write(&format!("会话挪进了回收站（{}）", entry.id));
    Ok(entry)
}

#[tauri::command]
pub async fn sessions_trash() -> Result<Vec<TrashEntry>> {
    blocking(session_ops::trash_list).await
}

#[tauri::command]
pub async fn sessions_trash_restore(id: String) -> Result<TrashEntry> {
    let entry = blocking(move || session_ops::trash_restore(&id)).await?;
    crate::audit::write(&format!("会话从回收站放回去了（{}）", entry.id));
    Ok(entry)
}

/// 真删：给 `id` 删那一项，不给就清空会话回收站。
#[tauri::command]
pub async fn sessions_trash_purge(id: Option<String>) -> Result<u32> {
    let n = blocking(move || session_ops::trash_purge(id.as_deref())).await?;
    crate::audit::write(&format!("会话回收站清掉了 {n} 项"));
    Ok(n)
}
