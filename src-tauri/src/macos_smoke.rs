//! Optional CI diagnostic for the real packaged WKWebView. Disabled outside a disposable runner.
use std::path::PathBuf;
use tauri::{Listener, Manager};

/// Only called after output_dir's disposable-runner gate. The native IPC smoke
/// uses a local controller and a locale file under its isolated HOME; it never
/// invokes real STUN, IPinfo, PeeringDB or an installed user's proxy controller.
fn diagnostic_fixture(dir: &std::path::Path) -> std::io::Result<serde_json::Value> {
    let home = dirs::home_dir()
        .ok_or_else(|| std::io::Error::other("Smoke HOME is unavailable"))?
        .canonicalize()?;
    if !home.starts_with(dir) || home == dir {
        return Err(std::io::Error::other("Smoke HOME is not isolated"));
    }
    let root = dirs::config_dir()
        .ok_or_else(|| std::io::Error::other("Smoke config root is unavailable"))?
        .canonicalize()?;
    if !root.starts_with(&home) {
        return Err(std::io::Error::other("Smoke config root is not isolated"));
    }
    let desktop = root.join("Claude");
    match std::fs::symlink_metadata(&desktop) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(std::io::Error::other(
                "Smoke desktop root is not a plain directory",
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&desktop)?
        }
        Err(error) => return Err(error),
    }
    let desktop = desktop.canonicalize()?;
    if !desktop.starts_with(&home) {
        return Err(std::io::Error::other("Smoke desktop root is not isolated"));
    }
    let config = desktop.join("config.json");
    let fixture = br#"{"locale":"ja-JP","unrelated":"native-locale-fixture-only"}"#;
    match std::fs::symlink_metadata(&config) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(std::io::Error::other(
                    "Smoke locale config is not a plain file",
                ));
            }
            if std::fs::read(&config)?.as_slice() != fixture {
                return Err(std::io::Error::other(
                    "Refusing to replace non-fixture locale config",
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&config)?
                .write_all(fixture)?;
        }
        Err(error) => return Err(error),
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let secret = "qb-native-controller-fixture";
    tauri::async_runtime::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
            return;
        };
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(180);
        for _ in 0..24 {
            let Ok(Ok((mut stream, _))) =
                tokio::time::timeout_at(deadline, listener.accept()).await
            else {
                break;
            };
            let exchange = async {
                let mut request = Vec::new();
                loop {
                    let mut bytes = [0; 512];
                    let Ok(n) = stream.read(&mut bytes).await else {
                        return;
                    };
                    if n == 0 || request.len() + n > 4096 {
                        return;
                    }
                    request.extend_from_slice(&bytes[..n]);
                    if request.windows(4).any(|part| part == b"\r\n\r\n") {
                        break;
                    }
                }
                let Ok(request) = std::str::from_utf8(&request) else {
                    return;
                };
                let authenticated =
                    request
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .any(|(name, value)| {
                            name.eq_ignore_ascii_case("authorization")
                                && value.trim() == format!("Bearer {secret}")
                        });
                let path = request
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default();
                let body = match path {
                    "/configs" => serde_json::json!({"mode":"rule"}),
                    "/rules" => {
                        serde_json::json!({"rules":[{"type":"MATCH","proxy":"fixture-group"}]})
                    }
                    "/proxies" => serde_json::json!({"proxies":{
                        "fixture-group":{"type":"Selector","all":["fixture-node"],"now":"fixture-node","udp":true},
                        "fixture-node":{"type":"Shadowsocks","udp":true}
                    }}),
                    _ => serde_json::Value::Null,
                };
                let (status, body) = if !authenticated {
                    ("401 Unauthorized", "{}".to_owned())
                } else if body.is_null() {
                    ("404 Not Found", "{}".to_owned())
                } else {
                    ("200 OK", body.to_string())
                };
                let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = stream.write_all(response.as_bytes()).await;
            };
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), exchange).await;
        }
    });
    Ok(serde_json::json!({"endpoint":endpoint,"secret":secret}))
}

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
    let diagnostics = diagnostic_fixture(&dir).ok();
    let phase = std::fs::read_to_string(dir.join("phase")).unwrap_or_else(|_| "smoke".into());
    let injection_dir = dir.clone();
    let automatic_exit = phase.trim() == "smoke";
    let stage_dir = dir.clone();
    app.listen("qb-macos-ui-stage", move |event| {
        if let Ok(stage) = serde_json::from_str::<String>(event.payload()) {
            if matches!(
                stage.as_str(),
                "await-url" | "await-invalid" | "url-inbox" | "url-confirmation"
            ) {
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
        "diagnostics": diagnostics,
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
