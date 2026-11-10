//! CI-only native smoke. Run through scripts/macos-runtime-smoke.mjs so secrets and
//! configuration are confined to a temporary Keychain and HOME on a disposable runner.
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This smoke requires macOS");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() {
    if let Some(code) = qb_platform::macos::handle_helper_args() {
        std::process::exit(code);
    }
    if let Err(error) = smoke::run() {
        eprintln!("Native runtime smoke failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(target_os = "macos")]
mod smoke {
    use qb_platform::{macos, secret, sessions::OwnedProgram};
    use std::{
        error::Error,
        fs,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    type Result<T> = std::result::Result<T, Box<dyn Error>>;

    fn wait_for(mut condition: impl FnMut() -> bool, description: &str) -> Result<()> {
        let end = Instant::now() + Duration::from_secs(15);
        while Instant::now() < end {
            if condition() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        Err(format!("Timed out: {description}").into())
    }
    fn same_process(pid: u32, created: u64) -> bool {
        macos::processes()
            .unwrap_or_default()
            .iter()
            .any(|p| p.pid == pid && p.created == created)
    }
    fn read_pid(path: &Path) -> Result<(u32, u64)> {
        wait_for(
            || {
                fs::read_to_string(path)
                    .ok()
                    .is_some_and(|s| s.contains(','))
            },
            "fixture process marker",
        )?;
        let text = fs::read_to_string(path)?;
        let (pid, created) = text
            .trim()
            .split_once(',')
            .ok_or("invalid fixture process marker")?;
        Ok((pid.parse()?, created.parse()?))
    }
    fn fixture_child(path: &Path) -> Result<()> {
        let mut child = Command::new("/bin/sleep").arg("60").spawn()?;
        fs::write(
            path,
            format!("{},{}", child.id(), macos::creation_time(child.id())?),
        )?;
        println!("native-fixture-started");
        let _ = child.wait();
        Ok(())
    }
    fn launch_fixture(root: &Path, marker: &Path) -> Result<OwnedProgram> {
        Ok(OwnedProgram::launch_logged(
            &std::env::current_exe()?,
            &[
                "--fixture-child".into(),
                marker.to_string_lossy().into_owned(),
            ],
            root,
            vec![("QB_RUNTIME_SMOKE_FIXTURE".into(), "1".into())],
            &root.join("fixture.log"),
        )?)
    }
    fn fixture_owner(root: &Path) -> Result<()> {
        let owned = launch_fixture(root, &root.join("crash-child"))?;
        fs::write(
            root.join("crash-supervisor"),
            format!("{},{}", owned.record.pid, owned.record.created),
        )?;
        std::thread::sleep(Duration::from_secs(60));
        drop(owned);
        Ok(())
    }
    fn guard_fixture() -> Result<()> {
        if std::env::var("QB_RUNTIME_SMOKE_FIXTURE").as_deref() != Ok("1") {
            return Err("fixture invocation requires explicit smoke environment".into());
        }
        Ok(())
    }
    pub(super) fn run() -> Result<()> {
        let args: Vec<String> = std::env::args().collect();
        match args.get(1).map(String::as_str) {
            Some("--fixture-child") if args.len() == 3 => {
                guard_fixture()?;
                return fixture_child(Path::new(&args[2]));
            }
            Some("--fixture-owner") if args.len() == 3 => {
                guard_fixture()?;
                return fixture_owner(Path::new(&args[2]));
            }
            _ => {}
        }
        if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true")
            || std::env::var("QB_MACOS_SMOKE_KEYCHAIN").as_deref() != Ok("isolated")
        {
            return Err("Run this smoke only through its isolated GitHub Actions wrapper".into());
        }
        let root = PathBuf::from(std::env::var("QB_MACOS_SMOKE_ROOT")?);
        if !root.is_absolute() || !root.is_dir() {
            return Err("invalid smoke root".into());
        }
        let first = secret::seal("fixture-token-not-a-real-credential")?;
        let second = secret::seal("fixture-token-not-a-real-credential")?;
        assert!(first.starts_with("keychain-aes-gcm-v1:"));
        assert!(!first.contains("fixture-token"));
        assert_ne!(first, second);
        assert_eq!(
            secret::open(&first).as_deref(),
            Some("fixture-token-not-a-real-credential")
        );
        fs::write(root.join("sealed-fixture"), &first)?;
        println!("PASS real Keychain master key and authenticated encryption");

        let owned = launch_fixture(&root, &root.join("stop-child"))?;
        let (pid, created) = read_pid(&root.join("stop-child"))?;
        assert!(owned.running()?);
        wait_for(
            || {
                fs::read_to_string(root.join("fixture.log"))
                    .ok()
                    .is_some_and(|s| s.contains("native-fixture-started"))
            },
            "forwarded stdout",
        )?;
        owned.stop()?;
        wait_for(|| !same_process(pid, created), "owned descendant cleanup")?;
        wait_for(
            || !owned.running().unwrap_or(true),
            "owned supervisor cleanup",
        )?;
        drop(owned);
        println!("PASS managed helper spawn, stdout, explicit stop and descendant cleanup");

        let mut owner = Command::new(std::env::current_exe()?)
            .args(["--fixture-owner", &root.to_string_lossy()])
            .env("QB_RUNTIME_SMOKE_FIXTURE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;
        let (supervisor, supervisor_created) = read_pid(&root.join("crash-supervisor"))?;
        let (pid, created) = read_pid(&root.join("crash-child"))?;
        owner.kill()?;
        let _ = owner.wait();
        wait_for(
            || !same_process(supervisor, supervisor_created),
            "crashed owner's supervisor cleanup",
        )?;
        wait_for(
            || !same_process(pid, created),
            "crashed owner's descendant cleanup",
        )?;
        println!("PASS abrupt owner exit cleans up the managed process tree");

        let failed = OwnedProgram::launch(&root.join("missing-executable"), &[], &root, vec![]);
        assert!(
            failed.is_err(),
            "missing executable must fail its launch acknowledgement"
        );
        println!("PASS failed executable launch is rejected before success is reported");

        let mut command =
            qb_platform::process::hidden_tokio(tokio::process::Command::new("/bin/sleep"));
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(async {
            command.arg("60").kill_on_drop(true);
            let mut child = command.spawn()?;
            let pid = child.id().ok_or("missing child ID")?;
            let created = macos::creation_time(pid)?;
            let job = qb_platform::process::KillOnCloseJob::new()?;
            job.assign(&child)?;
            job.terminate_all();
            let _ = child.wait().await;
            assert!(!same_process(pid, created));
            Ok::<_, Box<dyn Error>>(())
        })?;
        println!("PASS short-lived CLI job supervisor and cancellation");
        Ok(())
    }
}
