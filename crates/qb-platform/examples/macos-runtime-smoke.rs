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
    use qb_platform::{
        macos, secret,
        sessions::{self, OwnedProgram},
    };
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
            .expect("process enumeration must succeed before confirming cleanup")
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

    fn prepared_lifecycle(root: &Path) -> Result<()> {
        use qb_platform::domain::{Client, IdentityKind, LaunchContext};
        let marker = root.join("prepared-client-started");
        let prepare = || {
            sessions::prepare(
                LaunchContext {
                    client: Client::ClaudeDesktop,
                    identity_kind: IdentityKind::Official,
                    identity_id: "synthetic-prepared".into(),
                    config_dir: root.join("prepared-config").display().to_string(),
                    working_dir: root.display().to_string(),
                },
                &std::env::current_exe()?,
                vec![
                    ("QB_RUNTIME_SMOKE_FIXTURE".into(), "1".into()),
                    (
                        "QB_RUNTIME_PREPARED_MARKER".into(),
                        marker.display().to_string(),
                    ),
                ],
                true,
                0,
            )
        };
        let fixture_exe = std::env::current_exe()?.canonicalize()?;
        let before = macos::processes()?;
        let prepared = prepare()?;
        let helpers: Vec<_> = macos::processes()?
            .into_iter()
            .filter(|p| {
                p.ppid == std::process::id()
                    && p.path.canonicalize().ok().as_ref() == Some(&fixture_exe)
                    && !before
                        .iter()
                        .any(|old| old.pid == p.pid && old.created == p.created)
            })
            .map(|p| (p.pid, p.created))
            .collect();
        assert_eq!(helpers.len(), 1);
        let helper_args = macos::process_argv(helpers[0].0)
            .map_err(|error| format!("prepared helper argument query failed: {error}"))?;
        assert!(
            helper_args.iter().any(|arg| arg == "--qb-managed"),
            "prepared helper mode missing from {} readable arguments",
            helper_args.len()
        );
        println!(
            "PASS native helper argument query ({} arguments)",
            helper_args.len()
        );
        assert!(!marker.exists(), "prepare must not start the client");
        drop(prepared);
        wait_for(
            || {
                helpers
                    .iter()
                    .all(|(pid, created)| !same_process(*pid, *created))
            },
            "cancelled prepared helper cleanup",
        )?;
        assert!(
            !marker.exists(),
            "cancelled preparation must never start the client"
        );
        let session = prepare()?.start()?;
        wait_for(|| marker.is_file(), "prepared client resume")?;
        sessions::stop(&session.id)?;
        wait_for(
            || !same_process(session.pid, session.process_created),
            "resumed session cleanup",
        )?;
        println!(
            "PASS prepared sessions stay suspended, cancel cleanly and start only after resume"
        );
        Ok(())
    }

    fn nvm_interpreter(root: &Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let home = dirs::home_dir().ok_or("missing fixture HOME")?;
        assert!(
            home.starts_with(root),
            "Node fixture must be inside the isolated HOME"
        );
        let bin = home.join(".nvm/versions/node/v24.0.0/bin");
        fs::create_dir_all(&bin)?;
        let interpreter = bin.join("node");
        fs::write(
            &interpreter,
            b"#!/bin/sh\nprintf 'nvm-interpreter' > \"$QB_NODE_PATH_MARKER\"\n",
        )?;
        fs::set_permissions(&interpreter, fs::Permissions::from_mode(0o755))?;
        let cli = root.join("synthetic-claude");
        fs::write(&cli, b"#!/usr/bin/env node\nfixture-only\n")?;
        fs::set_permissions(&cli, fs::Permissions::from_mode(0o755))?;
        let marker = root.join("nvm-interpreter-used");
        let owned = OwnedProgram::launch_logged(
            &cli,
            &[],
            root,
            vec![
                ("PATH".into(), "/usr/bin:/bin".into()),
                ("QB_NODE_PATH_MARKER".into(), marker.display().to_string()),
            ],
            &root.join("nvm-interpreter.log"),
        )?;
        wait_for(|| marker.exists(), "Finder PATH finds nvm interpreter")?;
        assert_eq!(fs::read_to_string(marker)?, "nvm-interpreter");
        drop(owned);
        println!("PASS Finder-style PATH executes a Node shebang through isolated nvm");
        Ok(())
    }

    pub(super) fn run() -> Result<()> {
        let args: Vec<String> = std::env::args().collect();
        if let Ok(marker) = std::env::var("QB_RUNTIME_PREPARED_MARKER") {
            guard_fixture()?;
            fs::write(marker, b"prepared client started")?;
            std::thread::sleep(Duration::from_secs(60));
            return Ok(());
        }
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
        match args.get(1).map(String::as_str) {
            Some("--keychain-reopen") => {
                let stored = fs::read_to_string(root.join("sealed-fixture"))?;
                assert_eq!(
                    secret::open(&stored).as_deref(),
                    Some("fixture-token-not-a-real-credential")
                );
                println!("PASS persisted ciphertext decrypts in a fresh process with the original Keychain");
                return Ok(());
            }
            Some("--keychain-locked") => {
                // CI must return the genuine Security error instead of waiting for a GUI unlock.
                let _no_prompt =
                    security_framework::os::macos::keychain::SecKeychain::disable_user_interaction(
                    )?;
                let stored = fs::read_to_string(root.join("sealed-fixture"))?;
                assert!(secret::open(&stored).is_none());
                assert!(
                    secret::seal("locked-fixture").is_err(),
                    "Locked Keychain must not save a plaintext fallback"
                );
                println!(
                    "PASS locked Keychain refuses decrypt and save without a plaintext fallback"
                );
                return Ok(());
            }
            Some("--keychain-other") => {
                let stored = fs::read_to_string(root.join("sealed-fixture"))?;
                assert!(secret::open(&stored).is_none());
                let new = secret::seal("other-keychain-fixture")?;
                assert_eq!(
                    secret::open(&new).as_deref(),
                    Some("other-keychain-fixture")
                );
                assert!(secret::open(&stored).is_none());
                assert_eq!(fs::read_to_string(root.join("sealed-fixture"))?, stored);
                println!(
                    "PASS a different Keychain cannot decrypt or replace the original ciphertext"
                );
                return Ok(());
            }
            _ => {}
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

        prepared_lifecycle(&root)?;
        nvm_interpreter(&root)?;

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
