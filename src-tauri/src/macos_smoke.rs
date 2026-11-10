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
    let phase = std::fs::read_to_string(dir.join("phase")).unwrap_or_else(|_| "smoke".into());
    let injection_dir = dir.clone();
    let automatic_exit = phase.trim() == "smoke";
    let stage_dir = dir.clone();
    app.listen("qb-macos-ui-stage", move |event| {
        if let Ok(stage) = serde_json::from_str::<String>(event.payload()) {
            if matches!(stage.as_str(), "await-url" | "await-invalid") {
                let _ = std::fs::write(stage_dir.join(stage), "ready");
            }
        }
    });
    let handle = app.clone();
    app.listen("qb-macos-ui-smoke", move |event| {
        let result = serde_json::from_str::<serde_json::Value>(event.payload());
        let ok = result.as_ref().is_ok_and(|v| v["ok"] == true);
        let _ = std::fs::write(dir.join("webview.json"), event.payload());
        if !automatic_exit {
            return;
        }
        let handle = handle.clone();
        tauri::async_runtime::spawn(async move {
            // Allow the workflow to capture a screenshot of the actual native window.
            tokio::time::sleep(std::time::Duration::from_secs(4)).await;
            handle.exit(if ok { 0 } else { 1 });
        });
    });
    let handle = app.clone();
    let config = serde_json::json!({
        "phase": phase.trim(),
        "home": dirs::home_dir(),
        "root": injection_dir,
    });
    let script = format!(
        "window.__QB_MACOS_SMOKE_CONFIG__ = {config};\n{}",
        include_str!("macos_smoke.js")
    );
    let control_dir = injection_dir.clone();
    let control_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        for _ in 0..1200 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let control = control_dir.join("control");
            let Ok(action) = std::fs::read_to_string(&control) else {
                continue;
            };
            let _ = std::fs::remove_file(control);
            match action.trim() {
                "close" => {
                    if let Some(window) = control_handle.get_webview_window("main") {
                        let _ = window.close();
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let visible = control_handle
                        .get_webview_window("main")
                        .is_some_and(|window| window.is_visible().unwrap_or(false));
                    let _ = std::fs::write(
                        control_dir.join("closed.json"),
                        serde_json::json!({"visible": visible, "tray": crate::tray::available()})
                            .to_string(),
                    );
                }
                "update" => {
                    let package = control_dir.join("package");
                    let result = (|| -> crate::error::Result<()> {
                        let sums = std::fs::read_to_string(package.join("SHA256SUMS.txt"))?;
                        let hash = crate::install::self_update::expected_sha256(
                            &sums,
                            crate::update::CURRENT_VERSION,
                        )?;
                        let installer = package.join(crate::install::self_update::INSTALLER);
                        if !crate::install::self_update::file_matches(&installer, &hash) {
                            return Err(crate::error::GateError::Other(
                                "Smoke update image hash differs".into(),
                            ));
                        }
                        crate::update::set_pending(installer, hash);
                        Ok(())
                    })();
                    let _ = std::fs::write(control_dir.join("update-prepared.json"), serde_json::json!({"ok": result.is_ok(), "error": result.err().map(|e| e.to_string())}).to_string());
                    control_handle.exit(0);
                    break;
                }
                "exit" => {
                    control_handle.exit(0);
                    break;
                }
                _ => {}
            }
        }
    });
    tauri::async_runtime::spawn(async move {
        for _ in 0..180 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if injection_dir.join("webview.json").exists() {
                break;
            }
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.show();
                if let Err(error) = window.eval(&script) {
                    let _ = std::fs::write(
                        injection_dir.join("injection-error.log"),
                        error.to_string(),
                    );
                }
            }
        }
    });
}

/// Record the real operating-system reopen event for the disposable packaged-app check.
#[cfg(target_os = "macos")]
pub fn reopened(app: &tauri::AppHandle) {
    let Some(dir) = output_dir() else { return };
    let visible = app
        .get_webview_window("main")
        .is_some_and(|window| window.is_visible().unwrap_or(false));
    let _ = std::fs::write(
        dir.join("reopened.json"),
        serde_json::json!({"visible": visible}).to_string(),
    );
}
