//! Native, online validation of the production installer. No login is attempted.
//! Run with: cargo run --locked -p qb-install --example macos-install-smoke

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This smoke check requires a native macOS runner.");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run().await
}

#[cfg(target_os = "macos")]
mod native {
    use managed::{App, InstallOutcome, Record};
    use qb_foundation::sink::ProgressSink;
    use qb_install::install::{managed, versions};
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    type CheckResult<T> = Result<T, Box<dyn std::error::Error>>;

    struct Progress;
    impl ProgressSink for Progress {
        fn phase(&self, step: u32, phase: &str) {
            println!("[{step}] {phase}");
        }
        fn log(&self, step: u32, line: &str) {
            println!("[{step}] {line}");
        }
        fn done(&self, phase: &str) {
            println!("{phase}");
        }
        fn fail(&self, error: &str) {
            eprintln!("{error}");
        }
    }

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> std::io::Result<Self> {
            let path = std::env::temp_dir().join(format!(
                "QB Gate 安装 smoke {}",
                qb_install::config_io::id()
            ));
            std::fs::DirBuilder::new().mode(0o700).create(&path)?;
            Ok(Self(std::fs::canonicalize(path)?))
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.0) {
                eprintln!(
                    "Could not remove smoke directory {}: {error}",
                    self.0.display()
                );
            }
        }
    }

    async fn verify(app: App, dir: &Path, expected: &Record) -> CheckResult<()> {
        let path = dir.join(app.exe_name());
        let actual = managed::record_of(dir, app).ok_or("Missing installation record")?;
        if &actual != expected
            || managed::sha256_file(&path).as_deref() != Some(expected.sha256.as_str())
        {
            return Err("Installed bytes and record do not match".into());
        }
        if std::fs::metadata(&path)?.permissions().mode() & 0o777 != 0o755 {
            return Err("Installed executable is not mode 0755".into());
        }
        let arch = tokio::process::Command::new("/usr/bin/lipo")
            .arg("-archs")
            .arg(&path)
            .output()
            .await?;
        let expected_arch = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x86_64"
        };
        if !arch.status.success() || String::from_utf8_lossy(&arch.stdout).trim() != expected_arch {
            return Err(
                format!("{} is not a native {expected_arch} binary", path.display()).into(),
            );
        }

        // The version command has an empty inherited environment and an explicit,
        // private Claude configuration path. It neither logs in nor reads QB Gate's
        // normal account slots. No real user credential is copied into this test.
        let config = dir.join("isolated config");
        std::fs::create_dir_all(&config)?;
        let mut command = tokio::process::Command::new(&path);
        command
            .arg("--version")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("TMPDIR", std::env::temp_dir())
            .env("LANG", "en_US.UTF-8")
            .env("CLAUDE_CONFIG_DIR", &config)
            .env("CLAUDE_SECURESTORAGE_CONFIG_DIR", &config)
            .current_dir(dir)
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(45), command.output()).await??;
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !output.status.success() || !version.contains(&expected.version) {
            return Err(format!(
                "{} --version failed or disagreed with {}: stdout={version:?}, stderr={}",
                app.label(),
                expected.version,
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        println!(
            "VERIFIED {}: {version}; {expected_arch}; mode 0755; sha256={}",
            app.label(),
            expected.sha256
        );
        Ok(())
    }

    async fn install(app: App, channel: &str, dir: &Path) -> CheckResult<InstallOutcome> {
        let outcome = managed::install_into(app, channel, dir, &Progress).await?;
        let record = managed::record_of(dir, app).ok_or("Missing installed record")?;
        if outcome.path != dir.join(app.exe_name())
            || outcome.version != record.version
            || outcome.sha256 != record.sha256
            || !outcome
                .log
                .iter()
                .any(|line| line.starts_with("SHA-256 与官方一致："))
        {
            return Err("Installer outcome or official digest validation is missing".into());
        }
        println!("SOURCE {}: {}", app.label(), record.source);
        verify(app, dir, &record).await?;
        Ok(outcome)
    }

    pub async fn run() -> CheckResult<()> {
        let scratch = Scratch::new()?;
        println!("Native installation smoke in {}", scratch.0.display());
        let claude_dir = scratch.0.join(App::ClaudeCode.key());
        let stable = install(App::ClaudeCode, "stable", &claude_dir).await?;
        let stable_record = managed::record_of(&claude_dir, App::ClaudeCode).unwrap();
        let latest = install(App::ClaudeCode, "latest", &claude_dir).await?;
        let latest_record = managed::record_of(&claude_dir, App::ClaudeCode).unwrap();
        if stable.version != latest.version {
            let history = versions::history(&claude_dir, App::ClaudeCode, Some(&latest.version));
            if !history
                .iter()
                .any(|entry| entry.version == stable.version && !entry.locked)
            {
                return Err("Previous official Claude version was not archived".into());
            }
            println!(
                "{}",
                versions::rollback(
                    &claude_dir,
                    App::ClaudeCode,
                    &stable.version,
                    Some(&latest_record)
                )?
            );
            verify(App::ClaudeCode, &claude_dir, &stable_record).await?;
            println!(
                "{}",
                versions::rollback(
                    &claude_dir,
                    App::ClaudeCode,
                    &latest.version,
                    Some(&stable_record)
                )?
            );
            verify(App::ClaudeCode, &claude_dir, &latest_record).await?;
            println!("VERIFIED official Claude upgrade and rollback in both directions");
        } else {
            println!(
                "SKIPPED distinct-version rollback: upstream stable and latest are both {}",
                stable.version
            );
        }
        let codex_dir = scratch.0.join(App::Codex.key());
        install(App::Codex, "latest", &codex_dir).await?;
        println!("Native production installer smoke passed. Official login was not tested.");
        Ok(())
    }
}
