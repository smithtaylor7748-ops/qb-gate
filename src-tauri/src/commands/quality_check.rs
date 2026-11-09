//! 只有人工点击才调用的单次质量检测；批次编排留在前端。
use crate::app::AppState;
use crate::error::Result;
use qb_app::usecase::quality_check::{
    self, QualityCheckRequest, QualityCheckResult, QualityTargetKind,
};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn quality_check_run(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: QualityCheckRequest,
) -> Result<QualityCheckResult> {
    quality_check::validate(&request)?;
    let relay_http = if request.target.kind == QualityTargetKind::Relay {
        // 只刷新来源和出站设置；不恢复/覆盖 current、pending 或用户的选线。
        super::station::load_upstreams(&state)?;
        let db = crate::repository::Repository::open()?;
        let network = qb_app::usecase::station_ops::load_router_network(&db)?;
        Some(quality_check::relay_client(&network)?)
    } else {
        None
    };
    quality_check::run(
        request,
        Arc::clone(&state.station),
        Arc::clone(&state.gate),
        relay_http,
        || {
            crate::app::start_watchdog(
                qb_app::launch::LaunchTarget::Codex.watch_mode(),
                &state,
                Some(app.clone()),
            );
        },
    )
    .await
}

#[tauri::command]
pub fn quality_check_cancel(request_id: String) -> Result<bool> {
    quality_check::cancel(&request_id)
}
