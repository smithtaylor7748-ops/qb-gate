//! Isolated Codex desktop login slots. Credentials belong to the official client;
//! QB Gate never copies, returns, refreshes, or sends them to a third party.
use crate::{
    config_io,
    error::{GateError, Result},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

pub mod usage;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CodexSlot {
    pub id: String,
    pub label: String,
    pub active: bool,
    pub logged_in: bool,
    pub auth_state: String,
    pub email: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug, Default, Serialize, TS)]
#[ts(export)]
pub struct CodexAccounts {
    pub slots: Vec<CodexSlot>,
    pub launched_id: Option<String>,
    pub launched_pid: Option<u32>,
    pub launched_at: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct Index {
    slots: Vec<Entry>,
    active: Option<String>,
    launched: Option<String>,
    #[serde(default)]
    launched_pid: Option<u32>,
    #[serde(default)]
    launched_at: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Entry {
    id: String,
    label: String,
}

pub fn root() -> PathBuf {
    crate::paths::state_dir().join("codex-accounts")
}

fn load(root: &Path) -> Result<Index> {
    match config_io::read_optional(&root.join("index.json"))? {
        Some(bytes) => Ok(serde_json::from_slice(&bytes)?),
        None => Ok(Index::default()),
    }
}
fn save(root: &Path, index: &Index) -> Result<()> {
    config_io::ensure_plain_path(root)?;
    config_io::replace(
        &root.join("index.json"),
        Some(&serde_json::to_vec_pretty(index)?),
    )
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 80 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
pub fn directory(root: &Path, id: &str) -> Result<PathBuf> {
    if !valid_id(id) {
        return Err(GateError::Other("无效的 Codex 账户槽位".into()));
    }
    let index = load(root)?;
    if !index.slots.iter().any(|e| e.id == id) {
        return Err(GateError::Other("Codex 账户槽位不存在".into()));
    }
    let dir = root.join(id);
    config_io::ensure_plain_path(&dir)?;
    Ok(dir)
}

pub fn list(root: &Path) -> Result<CodexAccounts> {
    let index = load(root)?;
    let mut slots = Vec::new();
    for e in index.slots {
        if !valid_id(&e.id) {
            return Err(GateError::Other("账户索引包含无效路径，未读取凭据".into()));
        }
        let home = root.join(&e.id).join("home");
        config_io::ensure_plain_path(&home)?;
        let mut slot = CodexSlot {
            active: index.active.as_deref() == Some(&e.id),
            id: e.id,
            label: e.label,
            logged_in: false,
            auth_state: "未登录".into(),
            email: None,
            plan: None,
        };
        match config_io::read_optional(&home.join("auth.json")) {
            Ok(Some(bytes)) => read_identity(&bytes, &mut slot),
            Ok(None) => {}
            Err(_) => slot.auth_state = "登录文件不可读".into(),
        }
        slots.push(slot);
    }
    Ok(CodexAccounts {
        slots,
        launched_id: index.launched,
        launched_pid: index.launched_pid,
        launched_at: index.launched_at,
    })
}

fn read_identity(bytes: &[u8], slot: &mut CodexSlot) {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        slot.auth_state = "登录文件损坏".into();
        return;
    };
    if v["auth_mode"].as_str() == Some("apikey")
        || v["OPENAI_API_KEY"].as_str().is_some_and(|s| !s.is_empty())
    {
        slot.auth_state = "API 模式，请在桌面端登录账户".into();
        return;
    }
    let tokens = &v["tokens"];
    slot.logged_in = tokens["access_token"]
        .as_str()
        .is_some_and(|s| !s.is_empty())
        && tokens["refresh_token"]
            .as_str()
            .is_some_and(|s| !s.is_empty());
    if slot.logged_in {
        slot.auth_state = "已登录 · 本地凭据".into();
    }
    // Decode public claims for display only; this is not signature validation or
    // a claim that the server still accepts the credential. Never return tokens.
    if let Some(jwt) = tokens["id_token"].as_str() {
        if let Some(claims) = jwt_claims(jwt) {
            slot.email = claims["email"].as_str().map(str::to_owned);
            slot.plan = claims["https://api.openai.com/auth"]["chatgpt_plan_type"]
                .as_str()
                .map(str::to_owned);
        }
    }
}

fn jwt_claims(jwt: &str) -> Option<serde_json::Value> {
    let s = jwt.split('.').nth(1)?;
    if s.len() > 32768 {
        return None;
    }
    let mut bytes = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for c in s.bytes().take_while(|c| *c != b'=') {
        let n = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(n);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((acc >> bits) as u8);
        }
    }
    serde_json::from_slice(&bytes).ok()
}

pub fn create(root: &Path, label: &str) -> Result<String> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 40 || label.chars().any(char::is_control) {
        return Err(GateError::Other("槽位名称需要 1–40 个可见字符".into()));
    }
    let mut index = load(root)?;
    if index.slots.iter().any(|e| e.label == label) {
        return Err(GateError::Other("已有同名 Codex 槽位".into()));
    }
    config_io::ensure_plain_path(root)?;
    let id = config_io::id();
    let dir = root.join(&id);
    std::fs::create_dir_all(dir.join("desktop"))?;
    // The official client performs OAuth and maintains refresh tokens itself.
    config_io::replace(
        &dir.join("home/config.toml"),
        Some(b"forced_login_method = \"chatgpt\"\ncli_auth_credentials_store = \"file\"\n"),
    )?;
    index.slots.push(Entry {
        id: id.clone(),
        label: label.into(),
    });
    if index.active.is_none() {
        index.active = Some(id.clone());
    }
    save(root, &index)?;
    Ok(id)
}

pub fn select(root: &Path, id: &str) -> Result<()> {
    directory(root, id)?;
    let mut index = load(root)?;
    index.active = Some(id.into());
    index.launched = None;
    index.launched_pid = None;
    index.launched_at = None;
    save(root, &index)
}
pub fn mark_launched(root: &Path, id: &str, pid: u32, started: &str) -> Result<()> {
    directory(root, id)?;
    let mut index = load(root)?;
    index.active = Some(id.into());
    index.launched = Some(id.into());
    index.launched_pid = Some(pid);
    index.launched_at = Some(started.into());
    save(root, &index)
}

/// Remove the slot from the list, keeping its files in a recoverable archive.
pub fn archive(root: &Path, id: &str) -> Result<()> {
    let dir = directory(root, id)?;
    let mut index = load(root)?;
    if index.active.as_deref() == Some(id) || index.launched.as_deref() == Some(id) {
        return Err(GateError::Other(
            "请先切换到其他槽位，再移除这个账户".into(),
        ));
    }
    let archive = root.join("archived");
    config_io::ensure_plain_path(&archive)?;
    std::fs::create_dir_all(&archive)?;
    let dest = archive.join(format!("{}-{}", id, config_io::id()));
    std::fs::rename(&dir, &dest)?;
    index.slots.retain(|s| s.id != id);
    if let Err(e) = save(root, &index) {
        std::fs::rename(dest, dir)?;
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isolated_profiles_survive_switching_and_archive_without_copying_credentials() {
        let root = std::env::temp_dir().join(format!("qb-codex-test-{}", config_io::id()));
        let a = create(&root, "个人").unwrap();
        let b = create(&root, "工作").unwrap();
        let a_dir = directory(&root, &a).unwrap();
        assert!(a_dir.join("home/config.toml").is_file());
        assert!(!a_dir.join("home/auth.json").exists());
        assert!(create(&root, "个人").is_err());
        let initial = std::fs::read(root.join("index.json")).unwrap();
        assert!(select(&root, "../outside").is_err());
        assert_eq!(initial, std::fs::read(root.join("index.json")).unwrap());
        assert!(archive(&root, &a).is_err());
        select(&root, &b).unwrap();
        assert_eq!(
            list(&root)
                .unwrap()
                .slots
                .iter()
                .find(|s| s.active)
                .unwrap()
                .id,
            b
        );
        archive(&root, &a).unwrap();
        assert_eq!(list(&root).unwrap().slots.len(), 1);
        assert_eq!(std::fs::read_dir(root.join("archived")).unwrap().count(), 1);
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn traversal_and_empty_ids_are_refused() {
        for id in ["", "..", "a/b", "C:\\x", "a.b", "a b"] {
            assert!(!valid_id(id));
        }
        assert!(valid_id("20260916-account1"));
    }
    #[test]
    fn api_credentials_never_count_as_account_login_or_leave_the_backend() {
        let mut slot = CodexSlot {
            id: "1".into(),
            label: "a".into(),
            active: false,
            logged_in: false,
            auth_state: String::new(),
            email: None,
            plan: None,
        };
        read_identity(
            br#"{"auth_mode":"apikey","OPENAI_API_KEY":"secret-value"}"#,
            &mut slot,
        );
        assert!(!slot.logged_in);
        assert!(!serde_json::to_string(&slot)
            .unwrap()
            .contains("secret-value"));
        read_identity(
            br#"{"tokens":{"access_token":"secret-value","refresh_token":"refresh-value"}}"#,
            &mut slot,
        );
        assert!(slot.logged_in);
        assert!(!serde_json::to_string(&slot)
            .unwrap()
            .contains("secret-value"));
    }
    #[test]
    fn malformed_jwt_is_not_a_login_error_or_panic() {
        assert!(jwt_claims("bad").is_none());
        assert!(jwt_claims("a.!.b").is_none());
        assert_eq!(
            jwt_claims("a.eyJlbWFpbCI6ImFAZXhhbXBsZS50ZXN0In0.b").unwrap()["email"],
            "a@example.test"
        );
    }
}
