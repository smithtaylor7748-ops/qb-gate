//! Gemini CLI 的登录槽位（0.26.0）—— 给酒馆的 Gemini 桥接用。
//!
//! # 为什么是 Gemini CLI 而不是反重力本体
//!
//! 反重力 Hub 没有公开的无交互模式（它的 `--headless` 走语言服务器未公开的 stdin 协议），
//! 令牌在 Windows 凭据管理器里、目录隔离隔不开。能像 `codex exec` 那样被酒馆驱动的，
//! 是 Google 自己的 **Gemini CLI**：公开的 `-p` / stdin 无交互模式、`--output-format json`，
//! 而且 `GEMINI_CLI_HOME` 能把整个 `~/.gemini` 挪到槽位目录里
//! （`packages/core/src/utils/paths.ts`，2026-09-20 核过源码）。
//! 同一个 Google 账户，在 Gemini CLI 里再登一次 —— 额度是否与反重力订阅共享**未核实**，
//! KNOWN-ISSUES 写了。
//!
//! # 桥接凭据零接触
//!
//! 一个槽位就是一个 `home\` 目录（`GEMINI_CLI_HOME`），CLI 自己把 `.gemini\oauth_creds.json`
//! 与 `settings.json` 写在里面。列槽位、判登录态、桥接都只看那个文件**在不在**：
//! 不读、不复制、不转发。「登录」= 起一个带窗口的 Gemini CLI 交互会话让它自己走 Google OAuth；
//! 「登录了没」= 文件在不在，再加上联网额度那条路问出来的「Google 不认了」
//! （`qb-app::usecase::login_health`）。
//!
//! 唯一的例外：使用者点 Gemini CLI 额度的「刷新」时，[`cli_token`] 读出访问令牌、刷新令牌与
//! 过期时刻；访问令牌过期了就在内存里换一张（2026-09-23，同反重力那条）。**不写回这个文件**
//! —— Google 的刷新令牌换新之后不作废，CLI 手里那份照样能用。不用于桥接、不落盘、
//! 不返回给前端，也没有后台定时器。
use crate::{
    config_io,
    error::{GateError, Result},
    oauth::OAuthToken,
};
use qb_platform::credentials::Secret;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GeminiSlot {
    pub id: String,
    pub label: String,
    pub active: bool,
    pub logged_in: bool,
    /// 一句人话：已登录 / 未登录 / 文件不可读。
    pub auth_state: String,
    /// `GEMINI_CLI_HOME` 指向的目录，给界面显示。
    pub home: String,
}

#[derive(Debug, Default, Clone, Serialize, TS)]
#[ts(export)]
pub struct GeminiAccounts {
    pub slots: Vec<GeminiSlot>,
}

#[derive(Default, Serialize, Deserialize)]
struct Index {
    slots: Vec<Entry>,
    active: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Entry {
    id: String,
    label: String,
}

pub fn root() -> PathBuf {
    crate::paths::state_dir().join("gemini-accounts")
}

/// 一个槽位的 `GEMINI_CLI_HOME`。CLI 在它下面建 `.gemini\`。
pub fn home_of(slot_dir: &Path) -> PathBuf {
    slot_dir.join("home")
}

/// CLI 自己写的凭据文件。**面板只看它在不在。**
pub fn creds_path(home: &Path) -> PathBuf {
    home.join(".gemini").join("oauth_creds.json")
}

/// 这个 Gemini CLI 槽位存在本机的那一份令牌（2026-09-23）。
///
/// **只在使用者点 Gemini CLI 额度的「刷新」时读**；列槽位、判登录态仍然只看文件在不在。
/// 令牌装进 [`Secret`]（抹零、没有 `Debug` / `Serialize` / `Clone`），不进任何账户结构、
/// 日志或 IPC 返回值。
///
/// 原来这里只取 `access_token` —— 而 Gemini CLI 的访问令牌只有一小时，CLI 不跑就不换，
/// 于是十次刷新九次拿着过期的去问、被回 401，界面说「登录令牌已失效」，其实登得好好的。
/// 现在连刷新令牌和过期时刻一起读，过期了由 `qb-app::usecase::tavern_quota` 在内存里换新。
pub fn cli_token(home: &Path) -> Result<Option<OAuthToken>> {
    let Some(bytes) = config_io::read_optional(&creds_path(home))? else {
        return Ok(None);
    };
    parse_cli_creds(Secret::new(bytes).as_bytes())
}

/// 纯函数：`oauth_creds.json` → 令牌。形状是 google-auth-library 的 `Credentials`：
/// `access_token` / `refresh_token` / `expiry_date`（**毫秒**）。两张令牌都没有 = 没登录（`None`）；
/// 不是 JSON = 形状不认识（`Err`，不当成没登录，§7.17）。
pub fn parse_cli_creds(bytes: &[u8]) -> Result<Option<OAuthToken>> {
    let v: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| GateError::Other("Gemini CLI 的凭据文件不是 JSON，形状不认识".into()))?;
    let secret = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| Secret::new(s.as_bytes().to_vec()))
    };
    let access = secret("access_token");
    let refresh = secret("refresh_token");
    if access.is_none() && refresh.is_none() {
        return Ok(None);
    }
    let expires_at = v
        .get("expiry_date")
        .and_then(|x| x.as_i64().or_else(|| x.as_f64().map(|f| f as i64)))
        .map(|t| if t > 100_000_000_000 { t / 1000 } else { t });
    Ok(Some(OAuthToken {
        access,
        refresh,
        expires_at,
    }))
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
        return Err(GateError::Other("无效的 Gemini 账户槽位".into()));
    }
    let index = load(root)?;
    if !index.slots.iter().any(|e| e.id == id) {
        return Err(GateError::Other("Gemini 账户槽位不存在".into()));
    }
    let dir = root.join(id);
    config_io::ensure_plain_path(&dir)?;
    Ok(dir)
}

/// 凭据文件在不在、能不能读元数据。**不读内容。**
///
/// 0.32.0 起 `antigravity::account` 也要问这一位（一条账户槽位的 CLI 那一半），
/// 所以放开可见性。**「不读内容」那条口径不变**：这里只看 `metadata`。
pub fn login_state(home: &Path) -> (bool, String) {
    let c = login_check(home);
    (c.logged_in, c.detail)
}

/// 登没登录，外加**读不读得出来**（2026-09-25）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliLogin {
    pub logged_in: bool,
    /// 凭据文件在、但读不出来。这时 `logged_in` 是 false，但**不是「未登录」**（§7.17）。
    pub unreadable: bool,
    pub detail: String,
}

pub fn login_check(home: &Path) -> CliLogin {
    let (logged_in, unreadable, detail) = match std::fs::metadata(creds_path(home)) {
        Ok(m) if m.is_file() && m.len() > 2 => (
            true,
            false,
            "已登录 · 本地凭据（由 Gemini CLI 自己保管）".to_string(),
        ),
        Ok(_) => (
            false,
            false,
            "凭据文件是空的：在登录窗口里重新登录".to_string(),
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (false, false, "未登录".to_string()),
        Err(e) => (false, true, format!("凭据文件读不出来（{e}）")),
    };
    CliLogin {
        logged_in,
        unreadable,
        detail,
    }
}

pub fn list(root: &Path) -> Result<GeminiAccounts> {
    let index = load(root)?;
    let mut slots = Vec::new();
    for e in index.slots {
        if !valid_id(&e.id) {
            return Err(GateError::Other("账户索引包含无效路径，未读取".into()));
        }
        let home = home_of(&root.join(&e.id));
        config_io::ensure_plain_path(&home)?;
        let (logged_in, auth_state) = login_state(&home);
        slots.push(GeminiSlot {
            active: index.active.as_deref() == Some(&e.id),
            id: e.id,
            label: e.label,
            logged_in,
            auth_state,
            home: home.display().to_string(),
        });
    }
    Ok(GeminiAccounts { slots })
}

/// 当前激活的槽位：`(标签, home, 登录没)`。
pub fn active(root: &Path) -> Result<Option<(String, PathBuf, bool)>> {
    let list = list(root)?;
    let Some(s) = list.slots.into_iter().find(|s| s.active) else {
        return Ok(None);
    };
    Ok(Some((s.label, PathBuf::from(s.home), s.logged_in)))
}

pub fn create(root: &Path, label: &str) -> Result<String> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 40 || label.chars().any(char::is_control) {
        return Err(GateError::Other("槽位名称需要 1–40 个可见字符".into()));
    }
    let mut index = load(root)?;
    if index.slots.iter().any(|e| e.label == label) {
        return Err(GateError::Other("已有同名 Gemini 槽位".into()));
    }
    config_io::ensure_plain_path(root)?;
    let id = config_io::id();
    let dir = root.join(&id);
    // 只建目录。`.gemini\` 里的东西（settings.json / oauth_creds.json）全由 CLI 自己写。
    std::fs::create_dir_all(home_of(&dir).join(".gemini"))?;
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
    save(root, &index)
}

/// 从清单里移除，文件挪进 `archived\` 留着可恢复。激活中的不许移。
pub fn archive(root: &Path, id: &str) -> Result<()> {
    let dir = directory(root, id)?;
    let mut index = load(root)?;
    if index.active.as_deref() == Some(id) {
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

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("qb-gemini-test-{}", config_io::id()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn slots_are_isolated_homes_and_login_is_just_the_file_being_there() {
        let root = temp_root();
        let a = create(&root, "个人").unwrap();
        let b = create(&root, "工作").unwrap();
        assert!(create(&root, "个人").is_err(), "同名拒绝");
        let a_home = home_of(&directory(&root, &a).unwrap());
        assert!(a_home.join(".gemini").is_dir());
        // 还没登录。
        let l = list(&root).unwrap();
        assert!(l.slots.iter().all(|s| !s.logged_in));
        assert_eq!(l.slots.iter().filter(|s| s.active).count(), 1);
        assert!(l.slots[0].active, "第一个建的槽位自动激活");
        // CLI 写了凭据文件 → 已登录。面板不读内容，随便写点什么都算。
        std::fs::write(creds_path(&a_home), b"{\"x\":1}").unwrap();
        let l = list(&root).unwrap();
        assert!(l.slots.iter().find(|s| s.id == a).unwrap().logged_in);
        assert!(!l.slots.iter().find(|s| s.id == b).unwrap().logged_in);
        // 空文件不算登录。
        std::fs::write(creds_path(&a_home), b"").unwrap();
        assert!(
            !list(&root)
                .unwrap()
                .slots
                .iter()
                .find(|s| s.id == a)
                .unwrap()
                .logged_in
        );
        // 激活中的不许移；切走之后可以。
        assert!(archive(&root, &a).is_err());
        select(&root, &b).unwrap();
        assert_eq!(active(&root).unwrap().unwrap().0, "工作");
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
        assert!(valid_id("20260920-account1"));
        let root = temp_root();
        assert!(select(&root, "../outside").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// 凭据路径就是 Gemini CLI 自己的：`<GEMINI_CLI_HOME>\.gemini\oauth_creds.json`。
    #[test]
    fn creds_path_matches_gemini_cli() {
        let home = Path::new(r"C:\x\gemini-accounts\abc\home");
        assert!(creds_path(home).ends_with(r"home\.gemini\oauth_creds.json"));
        assert_eq!(home_of(Path::new(r"C:\x\gemini-accounts\abc")), home);
    }

    /// google-auth-library 的 `Credentials` 形状：过期时刻是**毫秒**，要换成秒。
    /// 值全是编的。
    #[test]
    fn cli_creds_keep_both_tokens_and_convert_the_expiry_to_seconds() {
        let t = parse_cli_creds(
            br#"{"access_token":"fake-access","refresh_token":"fake-refresh","token_type":"Bearer","expiry_date":1790000000000}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            t.access.as_ref().and_then(|s| s.as_str()),
            Some("fake-access")
        );
        assert_eq!(
            t.refresh.as_ref().and_then(|s| s.as_str()),
            Some("fake-refresh")
        );
        assert_eq!(t.expires_at, Some(1_790_000_000));
        assert!(t.access_usable(1_789_000_000, 300));
        assert!(!t.access_usable(1_790_000_000, 300), "过期了就不能用");
    }

    /// 两张令牌都没有 = 没登录；不是 JSON = 形状不认识（报错，不降级成「没登录」）。
    #[test]
    fn empty_creds_are_no_login_and_garbage_is_an_error() {
        assert!(parse_cli_creds(br#"{"access_token":"","scope":"x"}"#)
            .unwrap()
            .is_none());
        assert!(parse_cli_creds(b"not json").is_err());
        // 没写过期时刻：令牌照读，调用方按「已过期」处理（先换新再问）。
        let t = parse_cli_creds(br#"{"refresh_token":"fake-refresh"}"#)
            .unwrap()
            .unwrap();
        assert!(t.expires_at.is_none() && !t.access_usable(0, 0));
    }
}
