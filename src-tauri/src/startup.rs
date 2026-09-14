//! Recovery failures keep the window usable while coordinated mutations remain blocked.
use crate::{
    audit, config_io,
    domain::Operation,
    error::{GateError, Result},
    gate, paths,
    repository::Repository,
};
use std::path::PathBuf;
use tauri::Manager;
/// 恢复记录的保留期。结清超过这么久、又没人指着的，启动时清掉。
const RETENTION: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 3600);

fn files_root() -> PathBuf {
    paths::state_dir().join("operations/files")
}
fn accounts_root() -> PathBuf {
    crate::accounts::AccountRoots::current()
        .panel
        .join("operations/accounts")
}
pub fn initialize(app: &tauri::AppHandle) -> crate::readiness::Status {
    let mut blocked: Vec<(PathBuf, String)> = Vec::new();
    let result = (|| -> Result<()> {
        let files = config_io::recover(&files_root())?;
        blocked = files.blocked;
        blocked.extend(crate::accounts::recover_switches()?);
        if !blocked.is_empty() {
            // 外部配置可能停在半途，后面几步（旧数据迁移、会话恢复）不该
            // 在这种状态上继续跑。先把每一条的路径和原因说清楚。
            return Err(GateError::Other(format!(
                "{} 条配置恢复记录结不清，已保留现场：{}",
                blocked.len(),
                blocked
                    .iter()
                    .map(|(p, e)| format!("{}（{e}）", p.display()))
                    .collect::<Vec<_>>()
                    .join("；")
            )));
        }
        crate::settings::load_checked()?;
        crate::install::versions::recover_installations(&crate::settings::managed_apps_dir())?;
        let mut db = Repository::open()?;
        db.migrate()?;
        db.prune_history()?;
        // 硬盘上的恢复记录也有保留期：每一条里都封着改动前的配置全文（DPAPI
        // 加密），只写不清的话既占地方，也让一份旧 Key 副本永远躺在硬盘上。
        // 仍被某个环境「回滚配置」指着的那些一条都不动。
        let cutoff = std::time::SystemTime::now()
            .checked_sub(RETENTION)
            .unwrap_or(std::time::UNIX_EPOCH);
        for removed in config_io::retain(&files_root(), cutoff, &db.referenced_operations()?)?
            .into_iter()
            .chain(crate::accounts::retain_switches(cutoff)?)
        {
            audit::write(&format!("已清理过期恢复记录 {removed}"));
        }
        crate::sessions::recover()?;
        let intent = config_io::read_object(&paths::state_dir().join("gate-intent.json"))?;
        app.state::<crate::AppState>().gate.manual_paused.store(
            intent["paused"].as_bool().unwrap_or(false),
            std::sync::atomic::Ordering::SeqCst,
        );
        for mut op in db.list::<Operation>("operations")? {
            if op.status == "running" {
                op.status = "interrupted".into();
                op.detail = "上次运行中断，配置恢复记录已经检查".into();
                db.put("operations", &op.id, &op)?;
            }
        }
        Ok(())
    })();
    crate::readiness::set_blocked(blocked.into_iter().map(|(p, _)| p).collect());
    let error = result.err().map(|e| e.to_string());
    if let Some(e) = &error {
        app.state::<crate::AppState>()
            .gate
            .manual_paused
            .store(true, std::sync::atomic::Ordering::SeqCst);
        audit::write(&format!("启动恢复失败，已进入恢复页面：{e}"));
    }
    crate::readiness::set_failure(error);
    crate::readiness::status()
}
#[tauri::command]
pub fn startup_status() -> crate::readiness::Status {
    crate::readiness::status()
}
#[tauri::command]
pub async fn startup_retry(app: tauri::AppHandle) -> Result<crate::readiness::Status> {
    let _guard = crate::operations::exclusive_unchecked().await;
    let result = initialize(&app);
    if result.ready {
        let state = app.state::<crate::AppState>();
        // Recovery does not implicitly restore permission to launch.
        gate::pause(&state.gate)?;
        crate::app::start_watchdog(gate::watchdog::WatchMode::Cli, &state, Some(app.clone()));
        crate::operations::changed(&crate::events::ui(&app), "startup", "");
    }
    Ok(result)
}

/// 放弃结不清的那几条恢复记录：挪进 `quarantine/`（**不删**），然后重新检查。
///
/// 只在使用者看过路径与原因、并在恢复页上明确点击之后才会走到这里。
/// 放弃等于放弃这几条记录的回滚能力 —— 它们牵涉的配置文件保持当前状态。
#[tauri::command]
pub async fn startup_discard_blocked(app: tauri::AppHandle) -> Result<crate::readiness::Status> {
    let _guard = crate::operations::exclusive_unchecked().await;
    let paths = crate::readiness::blocked_paths();
    if paths.is_empty() {
        return Err(GateError::Other("当前没有结不清的恢复记录".into()));
    }
    let mut moved = Vec::new();
    for root in [files_root(), accounts_root()] {
        let group: Vec<PathBuf> = paths
            .iter()
            .filter(|p| p.parent() == Some(root.as_path()))
            .cloned()
            .collect();
        if !group.is_empty() {
            moved.extend(config_io::quarantine(&root, &group)?);
        }
    }
    audit::write(&format!(
        "使用者放弃了 {} 条结不清的恢复记录，已移入 quarantine：{}",
        moved.len(),
        moved.join("；")
    ));
    startup_retry(app).await
}
