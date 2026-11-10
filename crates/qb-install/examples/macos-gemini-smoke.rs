//! Native online install and version check in a private temporary directory. No OAuth or inference.
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This smoke check requires macOS.");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use qb_install::install::{detect, gemini_setup};
    use std::os::unix::fs::DirBuilderExt;
    use std::time::Duration;
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = std::env::temp_dir().join(format!(
        "QB Gemini 用户 O'Brien {}",
        qb_install::config_io::id()
    ));
    std::fs::DirBuilder::new().mode(0o700).create(&root)?;
    let scratch = Scratch(std::fs::canonicalize(root)?);
    let installed =
        gemini_setup::install_into(&scratch.0.join("app"), &qb_foundation::sink::Silent).await?;
    let home = scratch.0.join("isolated-home");
    std::fs::create_dir_all(&home)?;
    let node = detect::node_exe().ok_or("Node disappeared")?;
    let path = std::env::join_paths([
        node.parent().ok_or("No node parent")?,
        std::path::Path::new("/usr/bin"),
        std::path::Path::new("/bin"),
    ])?;
    let mut command = tokio::process::Command::new(node);
    command
        .env_clear()
        .env("HOME", &home)
        .env("GEMINI_CLI_HOME", &home)
        .env("PATH", path)
        .arg(&installed.entry)
        .arg("--version")
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(45), command.output()).await??;
    let version = String::from_utf8(output.stdout)?;
    if !output.status.success() || version.trim() != installed.version {
        return Err(format!(
            "Gemini version check failed (status {}, stdout {:?})",
            output.status,
            version.trim()
        )
        .into());
    }
    if home.join(".gemini/oauth_creds.json").exists() {
        return Err("Unexpected OAuth credentials".into());
    }
    println!("PASS: Gemini CLI {} installed through production API; actual node --version command, Unicode/space/quote path, private home; no login/inference", installed.version);
    Ok(())
}
