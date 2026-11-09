//! Explicit Antigravity IDE → local SillyTavern bridge. No automatic startup.

use crate::{error::Result, events, gate, operations, AppState};
use qb_app::usecase::antigravity_native_bridge::{
    self as bridge, AntigravityBridgeConfig, AntigravityBridgeModel, AntigravityBridgeStatus,
};

const HOLDER: &str = "antigravity-native-bridge";

#[tauri::command]
pub fn tavern_antigravity_status() -> Result<AntigravityBridgeStatus> {
    bridge::status()
}

#[tauri::command]
pub async fn tavern_antigravity_save(config: AntigravityBridgeConfig) -> Result<()> {
    let _guard = operations::exclusive().await?;
    bridge::save_config(config)
}

#[tauri::command]
pub async fn tavern_antigravity_models(account_id: String) -> Result<Vec<AntigravityBridgeModel>> {
    let _guard = operations::exclusive().await?;
    bridge::refresh_models(&account_id).await
}

#[tauri::command]
pub fn tavern_antigravity_token() -> Result<String> {
    bridge::token()
}

#[tauri::command]
pub async fn tavern_antigravity_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<AntigravityBridgeStatus> {
    let _guard = operations::exclusive().await?;
    let gated = crate::launch::LaunchTarget::Antigravity.gated();
    let was_running = bridge::running();
    if gated {
        gate::open_authorized(HOLDER, &state.gate).await?;
    }
    let rep = events::Reporter::new(app.clone(), events::TASK_TAVERN_START, 5);
    let result: Result<_> = async {
        let status = bridge::start().await?;
        crate::plugins::sillytavern::start_tavern_only(&rep).await?;
        Ok(status)
    }
    .await;
    if result.is_err() && !was_running {
        let _ = bridge::stop();
        if gated {
            let _ = gate::release_holder(&state.gate, HOLDER);
        }
    }
    if gated && bridge::running() {
        crate::app::start_watchdog(gate::watchdog::WatchMode::Cli, &state, Some(app.clone()));
    }
    operations::changed(&events::ui(&app), "session", "sillytavern");
    result
}

#[tauri::command]
pub async fn tavern_antigravity_stop(state: tauri::State<'_, AppState>) -> Result<()> {
    let _guard = operations::exclusive().await?;
    bridge::stop()?;
    gate::release_holder(&state.gate, HOLDER)
}
