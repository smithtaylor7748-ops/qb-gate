//! Explicitly paired Chrome companion. No browser process is controlled here.

use std::sync::Arc;

use crate::{error::Result, AppState};
use qb_app::{browser_link, browser_probe::AssetSource};

struct CompanionAssets(tauri::AppHandle);

impl AssetSource for CompanionAssets {
    fn get(&self, path: &str) -> Option<(String, Vec<u8>)> {
        let asset = self.0.asset_resolver().get(path.to_string())?;
        Some((asset.mime_type, asset.bytes))
    }
}

#[tauri::command]
pub async fn browser_companion_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<browser_link::BrowserLinkStart> {
    browser_link::start(state.gate.clone(), Arc::new(CompanionAssets(app))).await
}

#[tauri::command]
pub async fn browser_companion_stop() {
    browser_link::stop().await;
}

#[tauri::command]
pub async fn browser_companion_status() -> browser_link::BrowserLinkStatus {
    browser_link::status().await
}
