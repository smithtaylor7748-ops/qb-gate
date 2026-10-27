//! 环境变量清理（2026-10-27，中转站重构阶段 7 · X5）。判定、备份与还原都在 `sysenv::envvars`；
//! 这里接上真的注册表和面板数据目录。
//!
//! 审计只在这一层写（坑 7.107：写在 usecase / 领域里会被单测写进使用者真实的 `ip-gate.log`）。
//! 改注册表、广播 `WM_SETTINGCHANGE` 放进阻塞任务：广播最多等 5 秒，不能卡在主线程上。

use crate::{
    audit,
    error::{GateError, Result},
    paths,
    sysenv::envvars,
};

fn dir() -> std::path::PathBuf {
    envvars::backup_dir(&paths::state_dir())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| GateError::Other(format!("没跑完：{e}")))?
}

/// 列出相关的环境变量（当前用户 / 整机 / 只在面板进程里）。只读。
#[tauri::command]
pub async fn env_vars_scan() -> Result<Vec<envvars::EnvEntry>> {
    blocking(envvars::scan).await
}

/// 删掉勾选的那几个（只删当前用户那一处）：先备份，备份写成功了才删。
#[tauri::command]
pub async fn env_vars_remove(names: Vec<String>) -> Result<envvars::EnvRemoval> {
    let r =
        blocking(move || envvars::remove(&envvars::Registry, &dir(), &names, chrono::Local::now()))
            .await?;
    if !r.removed.is_empty() {
        audit::write(&format!(
            "环境变量清理：删掉当前用户的 {}（备份 {}）",
            r.removed.join("、"),
            r.backup_id.as_deref().unwrap_or("-")
        ));
    }
    Ok(r)
}

/// 备份列表（不带值）。
#[tauri::command]
pub async fn env_backups() -> Result<Vec<envvars::EnvBackupInfo>> {
    blocking(|| envvars::backups(&dir())).await
}

/// 按一份备份还原：现在没有的写回去，现在有一个不一样的不盖。
#[tauri::command]
pub async fn env_backup_restore(id: String) -> Result<envvars::EnvRestoreReport> {
    let key = id.clone();
    let r =
        blocking(move || envvars::restore(&envvars::Registry, &dir(), &key, chrono::Local::now()))
            .await?;
    if !r.restored.is_empty() {
        audit::write(&format!(
            "环境变量清理：按备份 {id} 写回 {}",
            r.restored.join("、")
        ));
    }
    Ok(r)
}

/// 删掉一份备份。**不可恢复**（界面上先确认）。
#[tauri::command]
pub async fn env_backup_delete(id: String) -> Result<()> {
    let key = id.clone();
    blocking(move || envvars::delete_backup(&dir(), &key)).await?;
    audit::write(&format!("环境变量清理：删掉备份 {id}"));
    Ok(())
}
