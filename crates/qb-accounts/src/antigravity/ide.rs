//! 反重力 IDE 的登录槽位（0.30.0）—— 一个槽位一个 `--user-data-dir`。
//!
//! # 为什么 IDE 能有槽位、Hub 不能
//!
//! 0.26.0 定「没有多槽位」的依据是**Hub** 的语言服务器把令牌放在 Windows 凭据管理器
//! （按用户全局，目录隔不开）。2026-09-21 实机核过 **IDE 不一样**：它是 VS Code 分支，
//! 登录令牌在 `<用户数据目录>\User\globalStorage\state.vscdb` 里（Hub 的目录下根本没有这个库），
//! 而 VS Code 认标准的 `--user-data-dir`。给它一个空目录起来，看到的就是「Welcome ·
//! Continue with Google」—— 干净的、要重新登录的一份（探针 2026-09-21）。
//!
//! 所以这跟 Claude Code 的 `CLAUDE_CONFIG_DIR`、Codex 的 `CODEX_HOME`、Gemini CLI 的
//! `GEMINI_CLI_HOME` 是同一个套路：**面板换的是目录，登录在官方窗口里做，令牌一个字不碰。**
//! Hub 仍然是单一身份（它用凭据管理器里那份），界面上要写清楚。
//!
//! # 凭据零接触
//!
//! 「登录了没」只问状态库里令牌那一行的长度（[`super::status::login_state`]），值不读；
//! 邮箱与档位读的是 IDE 自己写下的 `userStatus`（不是令牌）。不复制、不刷新、不转发。
//!
//! # 新槽位是干净的 IDE
//!
//! 一个新的用户数据目录意味着设置、主题、快捷键都从头来。新建时**只复制**默认资料目录里的
//! `User\settings.json` 与 `User\keybindings.json`（如果有）—— 这两份是使用者自己的偏好，
//! 不含登录、不含扩展状态（`globalStorage` 一个字节不搬）。界面上要说这一句。
//!
//! # 「默认」那份不动
//!
//! 使用者原来的 `%APPDATA%\Antigravity IDE` 保持原样：没有激活槽位时，面板起的就是它
//! （跟 Claude Code「没有槽位就用 `~\.claude`」一样）。不把它「导入」成槽位 ——
//! 复制式迁移造过两份 refresh token 的事故（§7.29），这里不再来一次。

use super::status;
use crate::{
    config_io,
    error::{GateError, Result},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AntigravityIdeSlot {
    pub id: String,
    pub label: String,
    pub active: bool,
    pub logged_in: bool,
    /// 一句人话：已登录 / 未登录 / 状态库不可读。
    pub auth_state: String,
    /// IDE 自己写下的邮箱与档位（`userStatus`），读不到就是 `None`。
    pub email: Option<String>,
    pub tier: Option<String>,
    /// 读账户状态时出的错（库正忙 / 损坏 / 形状变了）。`None` = 没出错。
    ///
    /// ⛔ **它跟「`email` 是 `None`」不是一回事**：前者是「读不出来」，后者是
    /// 「这份资料还没登录过」。界面上必须是两句话 —— 下一步该做什么完全不同。
    pub identity_error: Option<String>,
    /// `--user-data-dir` 指向的目录，给界面显示。
    pub user_data: String,
}

#[derive(Debug, Default, Clone, Serialize, TS)]
#[ts(export)]
pub struct AntigravityIdeAccounts {
    pub slots: Vec<AntigravityIdeSlot>,
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
    crate::paths::state_dir().join("antigravity-ide-accounts")
}

/// 一个槽位的 `--user-data-dir`。
pub fn user_data_of(slot_dir: &Path) -> PathBuf {
    slot_dir.join("user-data")
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
        return Err(GateError::Other("无效的反重力 IDE 槽位".into()));
    }
    let index = load(root)?;
    if !index.slots.iter().any(|e| e.id == id) {
        return Err(GateError::Other("反重力 IDE 槽位不存在".into()));
    }
    let dir = root.join(id);
    config_io::ensure_plain_path(&dir)?;
    Ok(dir)
}

fn describe(root: &Path, e: &Entry, active: bool) -> Result<AntigravityIdeSlot> {
    let user_data = user_data_of(&root.join(&e.id));
    config_io::ensure_plain_path(&user_data)?;
    let login = status::login_state(&user_data);
    // 身份读不出来**不让整页报错**（槽位列表、启动磁贴都还要用），但也**不能悄悄吞掉**。
    //
    // ⛔ 0.30.0–0.31.0 这里写的是 `.ok().flatten()`：库正忙（SQLITE_BUSY 超过 250 ms）
    // 或者损坏时，槽位行安安静静显示成「没有邮箱」，而同一页的身份卡那边有
    // `identity_error`。同一台机器上两处对同一件事给两种说法，而「读不出来」那一种
    // 根本没人看得见 —— 跟坑 7.49 / 7.50 同族：**失败必须落到界面上**。
    let (identity, identity_error) = match status::identity(&user_data) {
        Ok(id) => (id, None),
        Err(e) => (None, Some(e.to_string())),
    };
    Ok(AntigravityIdeSlot {
        id: e.id.clone(),
        label: e.label.clone(),
        active,
        logged_in: login.logged_in,
        auth_state: login.detail,
        email: identity.as_ref().and_then(|i| i.email.clone()),
        tier: identity.and_then(|i| i.tier_name),
        identity_error,
        user_data: user_data.display().to_string(),
    })
}

pub fn list(root: &Path) -> Result<AntigravityIdeAccounts> {
    let index = load(root)?;
    let mut slots = Vec::new();
    for e in &index.slots {
        if !valid_id(&e.id) {
            return Err(GateError::Other("账户索引包含无效路径，未读取".into()));
        }
        slots.push(describe(root, e, index.active.as_deref() == Some(&e.id))?);
    }
    Ok(AntigravityIdeAccounts { slots })
}

/// 当前激活槽位的 `(id, 标签, user-data 目录)`。没有槽位 / 没激活 → `None`（面板起默认那份）。
pub fn active(root: &Path) -> Result<Option<(String, String, PathBuf)>> {
    let index = load(root)?;
    let Some(id) = index.active else {
        return Ok(None);
    };
    let Some(e) = index.slots.iter().find(|e| e.id == id) else {
        return Ok(None);
    };
    if !valid_id(&e.id) {
        return Err(GateError::Other("账户索引包含无效路径，未读取".into()));
    }
    Ok(Some((
        e.id.clone(),
        e.label.clone(),
        user_data_of(&root.join(&e.id)),
    )))
}

/// 新建一个槽位。`seed_from` 是默认资料目录（`%APPDATA%\Antigravity IDE`）——
/// 只从它复制 `User\settings.json` / `User\keybindings.json`，别的一概不碰。
pub fn create(root: &Path, label: &str, seed_from: Option<&Path>) -> Result<String> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 40 || label.chars().any(char::is_control) {
        return Err(GateError::Other("槽位名称需要 1–40 个可见字符".into()));
    }
    let mut index = load(root)?;
    if index.slots.iter().any(|e| e.label == label) {
        return Err(GateError::Other("已有同名反重力 IDE 槽位".into()));
    }
    config_io::ensure_plain_path(root)?;
    let id = config_io::id();
    let dir = root.join(&id);
    let user_data = user_data_of(&dir);
    let user = user_data.join("User");
    std::fs::create_dir_all(&user)?;
    if let Some(seed) = seed_from {
        for name in ["settings.json", "keybindings.json"] {
            let src = seed.join("User").join(name);
            if src.is_file() {
                // 复制失败不阻止建槽位：少一份偏好只是不方便，不是错。
                let _ = std::fs::copy(&src, user.join(name));
            }
        }
    }
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
        let root = std::env::temp_dir().join(format!("qb-ag-ide-test-{}", config_io::id()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn slots_are_isolated_user_data_dirs_and_only_preferences_get_seeded() {
        let root = temp_root();
        // 一份假的默认资料目录：偏好两份 + 一份不该被搬的 globalStorage。
        let seed = root.join("default-profile");
        std::fs::create_dir_all(seed.join("User").join("globalStorage")).unwrap();
        std::fs::write(seed.join("User").join("settings.json"), b"{\"a\":1}").unwrap();
        std::fs::write(
            seed.join("User").join("globalStorage").join("state.vscdb"),
            b"secret",
        )
        .unwrap();

        let a = create(&root, "个人", Some(&seed)).unwrap();
        let b = create(&root, "工作", None).unwrap();
        assert!(create(&root, "个人", None).is_err(), "同名拒绝");
        let a_data = user_data_of(&directory(&root, &a).unwrap());
        assert_eq!(
            std::fs::read(a_data.join("User").join("settings.json")).unwrap(),
            b"{\"a\":1}"
        );
        assert!(
            !a_data
                .join("User")
                .join("globalStorage")
                .join("state.vscdb")
                .exists(),
            "登录状态库绝不从默认资料复制"
        );
        assert!(
            !a_data.join("User").join("keybindings.json").exists(),
            "没有的就不复制"
        );
        let l = list(&root).unwrap();
        assert!(l.slots.iter().all(|s| !s.logged_in), "还没起过 IDE");
        assert_eq!(l.slots.iter().filter(|s| s.active).count(), 1);
        assert!(l.slots[0].active, "第一个建的槽位自动激活");
        assert_eq!(active(&root).unwrap().unwrap().0, a);
        // 激活中的不许移；切走之后可以。
        assert!(archive(&root, &a).is_err());
        select(&root, &b).unwrap();
        assert_eq!(active(&root).unwrap().unwrap().1, "工作");
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
        let root = temp_root();
        assert!(select(&root, "../outside").is_err());
        assert_eq!(active(&root).unwrap(), None, "没有槽位就是没有激活的");
        std::fs::remove_dir_all(root).unwrap();
    }
}
