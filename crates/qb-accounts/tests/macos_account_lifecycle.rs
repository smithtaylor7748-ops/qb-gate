//! Native filesystem integration through public production APIs. All credentials are synthetic.
#![cfg(target_os = "macos")]
use qb_accounts::{accounts, antigravity::account, codex, gemini};
use std::path::{Path, PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        use std::os::unix::fs::DirBuilderExt;
        let p =
            std::env::temp_dir().join(format!("QB 账户 O'Brien {}", qb_accounts::config_io::id()));
        std::fs::DirBuilder::new().mode(0o700).create(&p).unwrap();
        Self(std::fs::canonicalize(p).unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn write(path: impl AsRef<Path>, bytes: &[u8]) {
    let path = path.as_ref();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn claude_switches_three_symlinks_preserves_slot_bytes_and_refuses_active_delete() {
    let scratch = Scratch::new();
    let roots = accounts::AccountRoots {
        panel: scratch.0.join("panel"),
        bridge: Some(scratch.0.join("bridge")),
        appdata: Some(scratch.0.join("Application Support")),
    };
    for p in [
        &roots.panel,
        roots.bridge.as_ref().unwrap(),
        roots.appdata.as_ref().unwrap(),
    ] {
        std::fs::create_dir_all(p).unwrap();
    }
    assert!(accounts::create_slot(&roots, "one").unwrap().activated);
    assert!(!accounts::create_slot(&roots, "two").unwrap().activated);
    let one = roots.slot_dir("one");
    let two = roots.slot_dir("two");
    let a = br#"{"claudeAiOauth":{"accessToken":"synthetic-one","refreshToken":"synthetic-one-refresh"}}"#;
    let b = br#"{"claudeAiOauth":{"accessToken":"synthetic-two","refreshToken":"synthetic-two-refresh"}}"#;
    write(one.join(".credentials.json"), a);
    write(two.join(".credentials.json"), b);
    accounts::switch_in(&roots, "one", accounts::DesktopMode::Follow).unwrap();
    write(
        roots.desktop_dir("one").unwrap().join("fixture"),
        b"desktop-one",
    );
    accounts::switch_in(&roots, "two", accounts::DesktopMode::Follow).unwrap();
    assert_eq!(accounts::active_label(&roots).as_deref(), Some("two"));
    assert_eq!(
        accounts::desktop_state(&roots).active.as_deref(),
        Some("two")
    );
    assert_eq!(
        std::fs::read_link(roots.panel.join("claude-profile")).unwrap(),
        two
    );
    assert_eq!(
        std::fs::read_link(roots.bridge.as_ref().unwrap().join("claude-profile")).unwrap(),
        two
    );
    assert_eq!(
        std::fs::read_link(roots.appdata.as_ref().unwrap().join("Claude")).unwrap(),
        roots.desktop_dir("two").unwrap()
    );
    assert_eq!(std::fs::read(one.join(".credentials.json")).unwrap(), a);
    assert_eq!(std::fs::read(two.join(".credentials.json")).unwrap(), b);
    assert!(accounts::delete_slot(&roots, "two", true).is_err());
    accounts::switch_in(&roots, "one", accounts::DesktopMode::Keep).unwrap();
    assert!(
        accounts::delete_slot(&roots, "two", true).is_err(),
        "Desktop still uses two"
    );
    accounts::switch_in(&roots, "one", accounts::DesktopMode::Follow).unwrap();
    accounts::delete_slot(&roots, "two", true).unwrap();
    assert!(!two.exists());
    assert!(!roots.desktop_dir("two").unwrap().exists());
    assert_eq!(
        std::fs::read(roots.desktop_dir("one").unwrap().join("fixture")).unwrap(),
        b"desktop-one"
    );
    assert_eq!(accounts::slots_in(&roots).len(), 1);
}

#[test]
fn codex_switch_archive_restore_preserves_auth_and_forces_file_credentials() {
    let scratch = Scratch::new();
    let root = scratch.0.join("codex");
    let one = codex::create(&root, "个人").unwrap();
    let two = codex::create(&root, "工作").unwrap();
    let home = codex::directory(&root, &one).unwrap().join("home");
    assert!(std::fs::read_to_string(home.join("config.toml"))
        .unwrap()
        .contains("cli_auth_credentials_store = \"file\""));
    let auth = br#"{"tokens":{"access_token":"synthetic-one","account_id":"fixture-account"}}"#;
    write(home.join("auth.json"), auth);
    assert_eq!(
        codex::chatgpt_account_id_from_home(&home)
            .unwrap()
            .as_deref(),
        Some("fixture-account")
    );
    assert!(!codex::directory(&root, &two)
        .unwrap()
        .join("home/auth.json")
        .exists());
    assert!(codex::archive(&root, &one, false).is_err());
    codex::select(&root, &two).unwrap();
    codex::archive(&root, &one, false).unwrap();
    assert!(!home.exists());
    let archived = codex::list_archived(&root).unwrap();
    assert_eq!(archived.len(), 1);
    codex::restore(&root, &archived[0].key).unwrap();
    assert_eq!(std::fs::read(home.join("auth.json")).unwrap(), auth);
    codex::select(&root, &one).unwrap();
    assert!(codex::select(&root, "../outside").is_err());
    assert_eq!(codex::list(&root).unwrap().slots.len(), 2);
}

#[test]
fn gemini_accounts_have_separate_homes_and_archive_only_inactive_slot() {
    let scratch = Scratch::new();
    let root = scratch.0.join("gemini");
    let one = gemini::create(&root, "个人").unwrap();
    let two = gemini::create(&root, "工作").unwrap();
    let home = gemini::home_of(&gemini::directory(&root, &one).unwrap());
    write(
        gemini::creds_path(&home),
        br#"{"access_token":"synthetic-one"}"#,
    );
    assert!(gemini::login_check(&home).logged_in);
    assert!(
        !gemini::login_check(&gemini::home_of(&gemini::directory(&root, &two).unwrap())).logged_in
    );
    assert!(gemini::archive(&root, &one).is_err());
    gemini::select(&root, &two).unwrap();
    gemini::archive(&root, &one).unwrap();
    assert_eq!(gemini::list(&root).unwrap().slots.len(), 1);
    assert!(!home.exists());
    assert!(gemini::select(&root, "../outside").is_err());
}

#[test]
fn antigravity_paired_profiles_switch_archive_restore_without_auth_copying() {
    let scratch = Scratch::new();
    let root = scratch.0.join("antigravity");
    let one = account::create(&root, "个人", None).unwrap();
    let two = account::create(&root, "工作", None).unwrap();
    let (_, cli, _) = account::cli_of(&root, &one).unwrap().unwrap();
    let (_, ide) = account::ide_half(&root, &one).unwrap().unwrap();
    write(
        cli.join(".gemini/oauth_creds.json"),
        br#"{"access_token":"synthetic-one"}"#,
    );
    write(ide.join("User/settings.json"), br#"{"fixture":true}"#);
    assert!(account::archive(&root, &one, false).is_err());
    account::select(&root, &two).unwrap();
    assert!(!account::cli_of(&root, &two).unwrap().unwrap().2);
    account::archive(&root, &one, false).unwrap();
    let archived = account::list_archived(&root).unwrap();
    assert_eq!(archived.len(), 1);
    account::restore(&root, &archived[0].key).unwrap();
    assert_eq!(
        std::fs::read(ide.join("User/settings.json")).unwrap(),
        br#"{"fixture":true}"#
    );
    assert!(account::cli_of(&root, &one).unwrap().unwrap().2);
    assert!(account::select(&root, "../outside").is_err());
}
