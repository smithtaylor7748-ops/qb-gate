//! Manual Windows smoke check. Temporary profiles, a loopback fixture and a fake key.
//! Opens and closes only the desktop process tree created by this example.
use http_body_util::Full;
use hyper::{
    body::{Bytes, Incoming},
    service::service_fn,
    Request, Response,
};
use hyper_util::rt::TokioIo;
use qb_app::usecase::station_ops;
use qb_app::{domain::*, local_router, repository::Repository, sessions, workspace};
use qb_station::station::route::Route;
use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let before = qb_install::install::codex_desktop::detect()?;
    let root = std::env::temp_dir().join(format!("qb-relay-smoke-{}", qb_app::config_io::id()));
    let db = Repository::open_in(&root)?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let upstream = listener.local_addr()?;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let requests = Arc::clone(&seen);
    let server = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let requests = Arc::clone(&requests);
            tokio::spawn(async move {
                let service = service_fn(move |req: Request<Incoming>| {
                    let requests = Arc::clone(&requests);
                    async move {
                        requests.lock().unwrap().push((
                            req.uri().path().to_string(),
                            req.headers()
                                .get("authorization")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("")
                                .to_string(),
                        ));
                        let body = if req.uri().path().ends_with("/models") {
                            r#"{"object":"list","data":[{"id":"fixture-model","object":"model"}]}"#
                        } else {
                            r#"{"id":"fixture-response","object":"response","status":"completed","output":[]}"#
                        };
                        Ok::<_, Infallible>(
                            Response::builder()
                                .header("content-type", "application/json")
                                .body(Full::new(Bytes::from(body)))
                                .unwrap(),
                        )
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    });
    let provider = Provider {
        id: "site".into(),
        name: "Smoke fixture".into(),
        base_url: format!("http://{upstream}/v1"),
        website: String::new(),
        note: String::new(),
        tags: vec![],
        favorite: false,
        revision: 1,
    };
    db.put("providers", "site", &provider)?;
    let credential = Credential {
        id: "key".into(),
        provider_id: "site".into(),
        label: "fixture".into(),
        masked: "***".into(),
        available: true,
        revision: 1,
    };
    db.set_credential(
        &credential,
        &qb_platform::secret::seal("fixture-upstream-key")?,
    )?;
    let route = Route {
        id: "fixture-route".into(),
        client: Client::Codex,
        station_id: "site".into(),
        credential_id: Some("key".into()),
        ..Default::default()
    };
    let source = station_ops::source(&db, &route)?;
    let shared = Arc::new(Mutex::new(local_router::RouterState::default()));
    {
        let mut state = shared.lock().unwrap();
        state.put_upstream(local_router::Upstream {
            route_id: route.id.clone(),
            base_url: source.base_url,
            auth: local_router::UpstreamAuth::Bearer(source.key),
        });
        state.set_now(Client::Codex, &route.id);
    }
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let router =
        local_router::serve(local_router::RouterConfig { port: 0 }, shared, http.clone()).await?;
    let base = workspace::client_base(
        &local_router::client_base_url(Client::Codex, router.addr.port()),
        Client::Codex,
    )?;
    for path in ["models", "responses"] {
        let response = http
            .post(format!("{base}/{path}"))
            .bearer_auth("client-placeholder")
            .json(&serde_json::json!({"model":"fixture-model"}))
            .send()
            .await?;
        assert!(
            response.status().is_success(),
            "{path}: {}",
            response.status()
        );
        let _ = response.text().await?;
    }
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .all(|(_, auth)| auth == "Bearer fixture-upstream-key"));
    assert_eq!(
        seen.lock()
            .unwrap()
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        vec!["/v1/models", "/v1/responses"]
    );
    println!("PASS route database credential -> loopback forwarding -> models/responses, client key replaced");
    let config = qb_app::config_io::environment_dir(&root, "desktop-fixture")?;
    let environment = Environment {
        id: "desktop-fixture".into(),
        name: "Fixture".into(),
        client: Client::Codex,
        provider_id: "site".into(),
        credential_id: None,
        model: "fixture-model".into(),
        small_model: String::new(),
        wire_api: "responses".into(),
        auth_style: "env_key".into(),
        revision: 1,
        applied_revision: None,
        config_dir: config.display().to_string(),
        config_state: "saved".into(),
        via_router: true,
    };
    std::fs::create_dir_all(&config)?;
    for edit in workspace::configuration_files(&db, &environment)? {
        if let Some(body) = edit.body {
            let text = String::from_utf8(body)?.replace(
                &format!(":{}", local_router::DEFAULT_PORT),
                &format!(":{}", router.addr.port()),
            );
            std::fs::write(edit.path, text)?;
        }
    }
    let executable = qb_launch::launch::resolve(qb_launch::launch::LaunchTarget::Codex)?;
    let program = sessions::OwnedProgram::launch(
        &executable,
        &[
            "--remote-debugging-port=19334".into(),
            format!("--user-data-dir={}", config.join("desktop").display()),
        ],
        &root,
        vec![
            ("CODEX_HOME".into(), config.display().to_string()),
            (
                "CODEX_ELECTRON_USER_DATA_PATH".into(),
                config.join("desktop").display().to_string(),
            ),
            ("OPENAI_API_KEY".into(), local_router::ROUTER_KEY.into()),
        ],
    )?;
    println!(
        "DESKTOP pid={} profile={} inspect=19334",
        program.record.pid,
        root.display()
    );
    tokio::time::sleep(Duration::from_secs(45)).await;
    assert!(program.running()?, "Desktop exited during startup");
    assert_eq!(
        sessions::creation_time(program.record.pid)?,
        program.record.created
    );
    program.stop()?;
    assert!(!program.running()?);
    let after = qb_install::install::codex_desktop::detect()?;
    if before.running {
        assert!(
            before.processes.iter().any(|old| after
                .processes
                .iter()
                .any(|p| p.pid == old.pid && p.started == old.started)),
            "Existing desktop must survive"
        );
    }
    router.stop();
    server.abort();
    println!("PASS desktop package launch, survived startup, owned process tree stopped, original desktop preserved");
    // Retain the temporary profile for manual inspection; it contains fixture data only.
    Ok(())
}
