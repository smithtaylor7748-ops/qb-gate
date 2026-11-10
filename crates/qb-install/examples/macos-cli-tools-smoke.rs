//! One official npm tool per independent native CI job. No login or model request.
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("macOS required");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use qb_install::install::{
        cli_tools::{CliTool, Roots, Step},
        detect, macos_cli_tools,
    };
    use std::{
        path::{Path, PathBuf},
        time::Duration,
    };
    let args: Vec<_> = std::env::args().skip(1).collect();
    let tool = CliTool::ALL
        .into_iter()
        .find(|t| {
            Some(t.command()) == args.first().map(String::as_str) && t.npm_package().is_some()
        })
        .ok_or("Expected opencode/grok/pi/mcode/openclaw and isolated root")?;
    let root = PathBuf::from(args.get(1).ok_or("Expected isolated root")?);
    if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true")
        || !root.is_absolute()
        || !root.is_dir()
    {
        return Err("Requires disposable GitHub macOS runner and existing private root".into());
    }
    let node = detect::node_exe().ok_or("Node missing")?;
    let node_version = detect::node_version(&node).ok_or("Node version missing")?;
    println!("Node: {node_version}; tool: {}", tool.label());
    if !qb_install::install::cli_tools_version::node_range_satisfied(
        ">=24.16.0 <25 || >=26.1.0",
        &node_version,
    )
    .unwrap_or(false)
    {
        return Err("Native tool checks require Node >=24.16 <25 or >=26.1".into());
    }
    let dir = macos_cli_tools::directory(tool);
    if !dir.starts_with(&root) {
        return Err(format!("Managed directory escapes isolated home: {}", dir.display()).into());
    }
    let installed = macos_cli_tools::install_into(tool, &dir, &qb_foundation::sink::Silent).await?;
    let version = installed
        .version
        .as_deref()
        .ok_or("Missing installed version")?;
    let found = qb_install::install::cli_tools::scan(tool, &Roots::current());
    if !found
        .iter()
        .any(|i| i.path == installed.path && i.version.as_deref() == Some(version) && i.default)
    {
        return Err("Production discovery did not select the installed version".into());
    }
    if qb_install::install::macos_install::command(tool.command()).as_deref()
        != Some(installed.path.as_path())
    {
        return Err("Production launch lookup did not select managed entry".into());
    }
    let Step::Run { program, args, env } =
        qb_install::install::cli_tools::update_plan(tool, &found, &Roots::current())
    else {
        return Err("Managed update plan was not runnable".into());
    };
    if args.iter().any(|a| a == "-g")
        || !args
            .windows(2)
            .any(|a| a == ["--prefix", dir.to_str().unwrap()])
    {
        return Err("Update escaped its managed prefix".into());
    }
    let update_env: Vec<_> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    if !qb_install::install::winget::run_streaming_env(
        &program.display().to_string(),
        &args,
        &update_env,
        &qb_foundation::sink::Silent,
        2,
        &mut Vec::new(),
    )
    .await?
    {
        return Err("Production update failed".into());
    }
    let updated = macos_cli_tools::inspect(tool, &dir)?;
    let version = updated
        .version
        .as_deref()
        .ok_or("Updated version missing")?;
    if updated.path != installed.path {
        return Err("Update changed entry location".into());
    }
    let external = root.join("external/bin").join(tool.command());
    std::fs::create_dir_all(external.parent().unwrap())?;
    std::fs::write(&external, "external-install-sentinel")?;
    let mut external_install = installed.clone();
    external_install.path = external.clone();
    if !matches!(
        qb_install::install::cli_tools::uninstall_plan(tool, &external_install, &Roots::current()),
        Step::Manual(_)
    ) {
        return Err("External installation could be uninstalled by managed action".into());
    }
    let path = std::env::join_paths([
        node.parent().ok_or("node has no parent")?,
        Path::new("/usr/bin"),
        Path::new("/bin"),
    ])?;
    let mut command = tokio::process::Command::new(&installed.path);
    command
        .env_clear()
        .env("HOME", &root)
        .env("PATH", &path)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("CI", "true")
        .env("NO_COLOR", "1")
        .current_dir(&root)
        .arg("--version")
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(90), command.output()).await??;
    let printed = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() || !printed.contains(version) {
        return Err(format!(
            "{} --version failed: status {}, output {}",
            tool.command(),
            output.status,
            printed.chars().take(1000).collect::<String>()
        )
        .into());
    }
    let Step::Run { program, args, env } =
        qb_install::install::cli_tools::uninstall_plan(tool, &installed, &Roots::current())
    else {
        return Err("Managed uninstall plan was not runnable".into());
    };
    let env: Vec<_> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let ok = qb_install::install::winget::run_streaming_env(
        &program.display().to_string(),
        &args,
        &env,
        &qb_foundation::sink::Silent,
        3,
        &mut Vec::new(),
    )
    .await?;
    if !ok
        || installed.path.exists()
        || qb_install::install::cli_tools::scan(tool, &Roots::current())
            .iter()
            .any(|i| i.path == installed.path)
    {
        return Err("Managed uninstall did not remove installed entry".into());
    }
    if std::fs::read_to_string(external)? != "external-install-sentinel" {
        return Err("External installation changed".into());
    }
    println!(
        "{}",
        serde_json::json!({"tool":tool.command(),"package":tool.npm_package(),"version":version,"node":node_version,"production_install":true,"production_update":true,"production_detect":true,"production_launch_lookup":true,"actual_version":printed.trim(),"production_uninstall":true,"external_preserved":true,"login_or_inference":false})
    );
    Ok(())
}
