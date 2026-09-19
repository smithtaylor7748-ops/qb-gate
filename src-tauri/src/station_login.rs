//! Isolated station login. Remote pages receive no QB Gate IPC access.
use crate::error::{GateError, Result};
use qb_app::usecase::station_billing::{self, StationBillingSettings};
use tauri::Manager;

pub async fn login(app: tauri::AppHandle, station_id: String) -> Result<StationBillingSettings> {
    let base = {
        let provider: crate::domain::Provider =
            crate::repository::Repository::open()?.get("providers", &station_id)?;
        station_billing::origin(&provider.base_url).map_err(GateError::Other)?
    };
    let origin_json = serde_json::to_string(&base)?;
    let label = format!("station-login-{}", uuid::Uuid::new_v4());
    let url: tauri::Url = format!("{base}/login")
        .parse()
        .map_err(|_| GateError::Other("站点地址无效".into()))?;
    let script = format!(
        r#"
        (() => {{
          if (location.origin !== {origin_json}) return;
          const original = window.fetch;
          window.__qbStationLogin = [];
          const remember = v => {{ window.__qbStationLogin = [...window.__qbStationLogin, v].slice(-12); }};
          const ownApi = value => {{
            try {{ const u = new URL(value, location.href); return u.origin === location.origin && u.pathname.startsWith("/api/"); }}
            catch {{ return false; }}
          }};
          const captureHeaders = headers => {{
            try {{
              const h = new Headers(headers);
              const token = (h.get("authorization") || "").replace(/^Bearer\s+/i, "");
              const user_id = h.get("new-api-user") || "";
              if (token || user_id) remember({{access_token:token,user_id}});
            }} catch {{}}
          }};
          window.fetch = async function(...args) {{
            if (ownApi(args[0] instanceof Request ? args[0].url : args[0])) {{
              captureHeaders(args[1]?.headers || (args[0] instanceof Request ? args[0].headers : undefined));
            }}
            const result = await original.apply(this, args);
            try {{
              const url = new URL(result.url);
              if (url.origin === location.origin && /^\/api\/(?:user\/(?:login|self|auth\/refresh)|v1\/auth\/(?:login|me|refresh))$/.test(url.pathname)) {{
                result.clone().json().then(remember).catch(() => {{}});
              }}
            }} catch {{}}
            return result;
          }};
          const state = new WeakMap();
          const open = XMLHttpRequest.prototype.open;
          const header = XMLHttpRequest.prototype.setRequestHeader;
          const send = XMLHttpRequest.prototype.send;
          XMLHttpRequest.prototype.open = function(method,url,...rest) {{
            state.set(this,{{url,headers:{{}}}}); return open.call(this,method,url,...rest);
          }};
          XMLHttpRequest.prototype.setRequestHeader = function(name,value) {{
            const s = state.get(this); if(s) s.headers[name] = value;
            return header.call(this,name,value);
          }};
          XMLHttpRequest.prototype.send = function(...args) {{
            const s = state.get(this);
            if (s && ownApi(s.url)) {{
              captureHeaders(s.headers);
              this.addEventListener("load", () => {{
                try {{
                  const u = new URL(this.responseURL);
                  if (u.origin === location.origin && /\/(?:login|self|me|refresh)$/.test(u.pathname)) {{
                    remember(this.responseType === "json" ? this.response : JSON.parse(this.responseText));
                  }}
                }} catch {{}}
              }},{{once:true}});
            }}
            return send.apply(this,args);
          }};
        }})();
    "#
    );
    let window =
        tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::External(url.clone()))
            .title("登录中转站 · 完成后自动返回 QB Gate")
            .inner_size(1020.0, 780.0)
            .incognito(true)
            .data_directory(
                app.path()
                    .app_local_data_dir()
                    .map_err(|e| GateError::Other(e.to_string()))?
                    .join("station-login"),
            )
            .initialization_script(script)
            .on_navigation(|url| {
                matches!(url.scheme(), "https" | "http")
                    && url.username().is_empty()
                    && url.password().is_none()
            })
            .build()
            .map_err(|e| GateError::Other(format!("无法打开独立登录窗口：{e}")))?;
    let capture_script = format!(
        r#"(() => {{
        if (location.origin !== {origin_json}) return [];
        const values = [];
        for (const storage of [localStorage, sessionStorage]) {{
          for (const key of ["user", "auth", "auth_data", "access_token", "token", "refresh_token"]) {{
            try {{
              const raw = storage.getItem(key);
              if (!raw || raw.length > 65536) continue;
              if (["access_token", "token", "refresh_token"].includes(key)) values.push({{[key]: raw.replace(/^"|"$/g, "")}});
              else values.push(JSON.parse(raw));
            }} catch {{}}
          }}
        }}
        return [...values, ...(window.__qbStationLogin || [])];
    }})()"#
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
    let mut last_attempt = String::new();
    let mut last_problem = String::new();
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        if app.get_webview_window(&label).is_none() {
            return Err(GateError::Other("登录窗口已关闭，未保存新的凭证".into()));
        }
        if !window
            .url()
            .is_ok_and(|u| u.origin().ascii_serialization() == base)
        {
            continue;
        }
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        window
            .eval_with_callback(capture_script.clone(), move |value| {
                let _ = tx.try_send(value);
            })
            .map_err(|_| GateError::Other("无法读取站点登录结果".into()))?;
        let Ok(Some(raw)) =
            tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv()).await
        else {
            continue;
        };
        if raw.len() > 1024 * 1024 {
            continue;
        }
        let captured = serde_json::from_str::<serde_json::Value>(&raw).unwrap_or_default();
        if !captured.as_array().is_some_and(|v| !v.is_empty()) {
            continue;
        }
        let mut jar = std::collections::BTreeMap::new();
        for path in ["/api/user/self", "/api/v1/auth/me"] {
            if let Ok(scope) = url.join(path) {
                for cookie in window.cookies_for_url(scope).unwrap_or_default() {
                    jar.insert(cookie.name().to_string(), cookie.value().to_string());
                }
            }
        }
        let cookie = jar
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        // Do not hammer the account endpoint with the same incomplete capture.
        use sha2::{Digest, Sha256};
        let fingerprint = hex::encode(Sha256::digest(format!("{raw}{cookie}")));
        if fingerprint == last_attempt {
            continue;
        }
        last_attempt = fingerprint;
        match station_billing::connect_browser(&station_id, &base, captured, cookie).await {
            Ok(settings) => {
                let _ = window.close();
                return Ok(settings);
            }
            Err(error) => last_problem = error.to_string(),
        }
    }
    let _ = window.close();
    Err(GateError::Other(format!(
        "登录等待已结束。请确认已完成站点登录。{last_problem}"
    )))
}
