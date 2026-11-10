//! Real official SillyTavern installation and local server; no model bridge or account.
//! Run only via scripts/macos-sillytavern-smoke.mjs on a disposable macOS runner.
use qb_extensions::plugins::sillytavern as tavern;
use qb_foundation::sink::ProgressSink;
use qb_install::install::{detect, sillytavern as installed_tavern, sillytavern_setup};
use std::{fs, net::TcpListener, path::PathBuf, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Reporter;
impl ProgressSink for Reporter {
    fn phase(&self, step: u32, text: &str) {
        println!("PHASE {step}: {text}");
    }
    fn log(&self, step: u32, text: &str) {
        println!("LOG {step}: {text}");
    }
    fn done(&self, text: &str) {
        println!("DONE: {text}");
    }
    fn fail(&self, text: &str) {
        eprintln!("FAIL: {text}");
    }
}

fn main() -> Result<()> {
    #[cfg(target_os = "macos")]
    if let Some(code) = qb_platform::macos::handle_helper_args() {
        std::process::exit(code);
    }
    if !cfg!(target_os = "macos")
        || std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true")
        || std::env::var("QB_SILLYTAVERN_KEYCHAIN").as_deref() != Ok("isolated")
    {
        return Err("Use the isolated wrapper on a disposable macOS Actions runner".into());
    }
    let root = PathBuf::from(std::env::var("QB_SILLYTAVERN_SMOKE_ROOT")?).canonicalize()?;
    let runner = PathBuf::from(std::env::var("RUNNER_TEMP")?).canonicalize()?;
    let home = dirs::home_dir().ok_or("No HOME")?.canonicalize()?;
    if root == runner
        || !root.starts_with(&runner)
        || home != root.join("home")
        || !qb_foundation::paths::state_dir().starts_with(&home)
        || !installed_tavern::managed_dir().starts_with(&home)
    {
        return Err("Refusing a smoke outside its isolated runner HOME".into());
    }
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        if std::env::args().nth(1).as_deref() == Some("--cleanup") {
            // Once the owner exits, production helpers must have stopped their groups.
            tavern::stop().await?;
            if tavern::console(false, None, 0)?.running {
                return Err("SillyTavern remains alive after cleanup".into());
            }
            println!("PASS production cleanup confirms no registered SillyTavern process remains");
            return Ok(());
        }
        let result = exercise(&root).await;
        let stopped = tavern::stop().await;
        if let Ok(console) = tavern::console(false, None, 0) {
            fs::write(
                root.join("console.json"),
                serde_json::to_vec_pretty(&console)?,
            )?;
        }
        result?;
        stopped?;
        Ok(())
    })
}

async fn exercise(root: &std::path::Path) -> Result<()> {
    if installed_tavern::managed_dir().exists() {
        return Err("Smoke install target must be absent".into());
    }
    let node = detect::node_exe().ok_or("A real Node.js LTS installation is required")?;
    let node_version = std::process::Command::new(&node)
        .arg("--version")
        .output()?;
    if !node_version.status.success() {
        return Err("Detected Node executable failed --version".into());
    }
    println!(
        "Node: {}",
        String::from_utf8_lossy(&node_version.stdout).trim()
    );
    // The installer itself resolves and executes npm, validates engines.node, and
    // follows only the official GitHub source/archive hosts. It does not require Git.
    let installed = sillytavern_setup::install(None, false, None, &Reporter).await?;
    if installed.launcher != installed.dir.join("server.js")
        || !installed
            .dir
            .join("node_modules/express/package.json")
            .is_file()
    {
        return Err("Production install did not yield a server with installed dependencies".into());
    }
    let marker = installed_tavern::read_marker(&installed.dir).ok_or("Missing managed marker")?;
    if marker.source != sillytavern_setup::archive_url(&marker.tag)
        || marker.version != installed.version
        || marker.archive_sha256.len() != 64
        || !marker.archive_sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Official archive provenance or installed version is inconsistent".into());
    }
    fs::write(
        root.join("installed.json"),
        serde_json::to_vec_pretty(&marker)?,
    )?;
    println!("PASS official SillyTavern {} source download, production npm install, managed marker and server.js", installed.version);

    let ports = (0..4)
        .map(|_| TcpListener::bind(("127.0.0.1", 0)))
        .collect::<std::io::Result<Vec<_>>>()?;
    let st_port = ports[0].local_addr()?.port();
    let mut cfg = tavern::load_config_checked()?;
    cfg.bridge_port = ports[1].local_addr()?.port();
    cfg.gpt_bridge_port = ports[2].local_addr()?.port();
    cfg.gemini_bridge_port = ports[3].local_addr()?.port();
    cfg.st_port = st_port;
    tavern::save_config(&cfg)?;
    tavern::adopt(&installed.dir, Some(st_port))?;

    // Keep the official defaults, changing only this fixture's local endpoint and
    // disabling browser launch, plugins and automatic optional downloads/updates.
    let mut yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(
        installed.dir.join("default/config.yaml"),
    )?)?;
    let overrides: serde_yaml_ng::Value = serde_yaml_ng::from_str(&format!(
        "port: {st_port}\nlisten: false\nprotocol: {{ ipv4: true, ipv6: false }}\nbrowserLaunch: {{ enabled: false }}\nautorun: false\nextensions: {{ enabled: false, autoUpdate: false, models: {{ autoDownload: false }} }}\nenableDownloadableTokenizers: false\nenableServerPlugins: false\nenableServerPluginsAutoUpdate: false\n"
    ))?;
    yaml.as_mapping_mut()
        .ok_or("Official config is not a mapping")?
        .extend(
            overrides
                .as_mapping()
                .ok_or("Invalid fixture settings")?
                .clone(),
        );
    fs::write(
        installed.dir.join("config.yaml"),
        serde_yaml_ng::to_string(&yaml)?,
    )?;
    if installed_tavern::config_port(&installed.dir) != Some(st_port) {
        return Err("Production config parser did not read the fixture port".into());
    }
    // A real foreign listener must be preserved and rejected before process launch.
    if tavern::start_tavern_only(&Reporter).await.is_ok()
        || ports[0].local_addr()?.port() != st_port
    {
        return Err("Occupied-port launch was not safely rejected".into());
    }
    println!("PASS production start refuses an occupied port and preserves its listener");
    drop(ports);
    let url = tavern::start_tavern_only(&Reporter).await?;
    if url != tavern::page_url(st_port) {
        return Err("Unexpected local server URL".into());
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()?;
    let response = client.get(&url).send().await?;
    if !response.status().is_success()
        || !response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("text/html"))
        || !response
            .text()
            .await?
            .to_ascii_lowercase()
            .contains("sillytavern")
    {
        return Err("Real local SillyTavern HTML did not pass HTTP health".into());
    }
    let console = tavern::console(false, None, 0)?;
    if !console.exists || !console.running || console.lines.is_empty() || console.run.is_empty() {
        return Err("Production console does not show the running server's log".into());
    }
    fs::write(
        root.join("running-console.json"),
        serde_json::to_vec_pretty(&console)?,
    )?;
    if tavern::start_tavern_only(&Reporter).await? != url
        || tavern::console(false, None, 0)?.run != console.run
    {
        return Err("Repeated start did not reuse the same owned server".into());
    }
    println!("PASS production start runs real node server.js; localhost HTML, console output and existing-server reuse verified");
    tavern::stop().await?;
    for _ in 0..100 {
        if std::net::TcpStream::connect_timeout(
            &([127, 0, 0, 1], st_port).into(),
            Duration::from_millis(100),
        )
        .is_err()
        {
            if tavern::console(false, None, 0)?.running {
                return Err("Console still reports a running stopped server".into());
            }
            println!("PASS production stop terminates the managed server, releases its loopback port and updates console state");
            fs::write(
                root.join("result.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "ok": true, "version": installed.version, "port": st_port,
                    "checks": ["official source install", "real npm dependencies", "occupied port refusal", "production managed launch", "localhost HTML", "console logs", "reuse existing server", "production stop"],
                    "scope": "No model bridge, real account, inference, optional model downloads or browser interaction."
                }))?,
            )?;
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("SillyTavern port remains in use after production stop".into())
}
