//! Optional CI diagnostic for the real packaged WKWebView. Disabled outside a disposable runner.
use std::path::PathBuf;
use tauri::{Listener, Manager};

fn output_dir() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") || std::env::var("GITHUB_ACTIONS").ok().as_deref() != Some("true")
    {
        return None;
    }
    let dir = PathBuf::from(std::env::var_os("QB_MACOS_UI_SMOKE_DIR")?)
        .canonicalize()
        .ok()?;
    let runner = PathBuf::from(std::env::var_os("RUNNER_TEMP")?)
        .canonicalize()
        .ok()?;
    (dir.starts_with(&runner) && dir != runner).then_some(dir)
}

pub fn install(app: &tauri::AppHandle) {
    let Some(dir) = output_dir() else { return };
    let injection_dir = dir.clone();
    let handle = app.clone();
    app.listen("qb-macos-ui-smoke", move |event| {
        let result = serde_json::from_str::<serde_json::Value>(event.payload());
        let ok = result.as_ref().is_ok_and(|v| v["ok"] == true);
        let _ = std::fs::write(dir.join("webview.json"), event.payload());
        let handle = handle.clone();
        tauri::async_runtime::spawn(async move {
            // Allow the workflow to capture a screenshot of the actual native window.
            tokio::time::sleep(std::time::Duration::from_secs(4)).await;
            handle.exit(if ok { 0 } else { 1 });
        });
    });
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        for _ in 0..60 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if injection_dir.join("webview.json").exists() {
                break;
            }
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.show();
                if let Err(error) = window.eval(include_str!("macos_smoke.js")) {
                    let _ = std::fs::write(
                        injection_dir.join("injection-error.log"),
                        error.to_string(),
                    );
                }
            }
        }
    });
}
