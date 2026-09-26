//! 反重力账户槽位：**一个 Google 账户一条，底下两半**（0.32.0）。
//!
//! # 为什么要合
//!
//! 0.30.0–0.31.0 是两套各自独立的槽位：
//!
//! | | 目录 | 换法 | 为谁服务 |
//! |---|---|---|---|
//! | 反重力 IDE | `state_dir\antigravity-ide-accounts\<id>\user-data` | `--user-data-dir` | IDE 本体 |
//! | Gemini CLI | `state_dir\gemini-accounts\<id>\home` | `GEMINI_CLI_HOME` | 酒馆的 Gemini 桥接 |
//!
//! 界面上是两个页签，于是同一个 Google 账户要建两次、登两次，而只用 IDE 的人永远
//! 看着一句「Gemini CLI · 0 个槽位」——它读起来像个故障，其实只是「你还没建」。
//!
//! 现在一条槽位 = 一个账户，底下挂两个目录（[`Entry::ide_dir`] / [`Entry::cli_dir`]）。
//! 两半各自有没有登录是两位独立的信息，界面上是两枚徽标。
//!
//! # ⛔ 升级迁移：只换索引，一个文件都不搬
//!
//! 老的两份索引各自升成新条目，`ide_dir` / `cli_dir` **指向老位置**（[`migrate`]）。
//!
//! 1. **不猜配对。** 面板没有任何依据断定某个 IDE 槽位和某个 Gemini 槽位是同一个
//!    Google 账户 —— IDE 的邮箱在它的 `state.vscdb` 里，而 CLI 那边面板**只看
//!    `oauth_creds.json` 在不在、从不读内容**（那是 `gemini` 模块的硬规矩）。
//!    猜错就是把两个人的账户并成一行。所以升上来的是「只填了一半」的行，
//!    要不要并由使用者自己点（[`attach`]）。
//! 2. **不搬目录。** §7.33「迁移搬一半」就是这么来的。指路不搬家：出了事把
//!    `index.json` 删掉就回到原状，老目录一个字节没动。
//! 3. **幂等。** 已经迁过的不再迁（按老目录路径认），重复跑没有副作用 ——
//!    面板每次启动都会调它。
//!
//! # 激活语义没有变
//!
//! 一个 `active` 管两半。**切换 = 换激活槽位，不关不起任何东西**：正在跑的 IDE
//! 继续用它起来时那份资料（`CLAUDE.md` 反重力边界 #2）。

use crate::antigravity::{ide, status};
use crate::error::{GateError, Result};
use crate::{config_io, gemini};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// 一个账户在界面上的样子。两半各自独立。
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct AntigravityAccount {
    pub id: String,
    pub label: String,
    pub active: bool,

    // ---- IDE 那一半 ----
    /// `--user-data-dir` 指向哪里。`None` = 这条槽位还没有 IDE 那一半。
    pub ide_dir: Option<String>,
    pub ide_logged_in: bool,
    /// 一句人话：已登录 / 未登录 / 状态库不可读。
    pub ide_auth_state: String,
    /// IDE 那一半的登录**读不出来**（状态库打不开 / 正忙 / 目录读不了，2026-09-25）。
    ///
    /// 这时 `ide_logged_in` 是 false，但**不是「未登录」**（§7.17）：原来界面照样画「IDE 未登录」、
    /// 摆一颗「登录」、把刷新图标灰掉 —— IDE 写库时占着锁超过 250 毫秒，15 秒一轮的读就把一个
    /// 登着的账户说成没登录。
    pub ide_unreadable: bool,
    /// IDE 自己写下的邮箱与档位。读不到就是 `None`。
    pub email: Option<String>,
    pub tier: Option<String>,
    /// 读账户状态时出的错。**跟「`email` 是 `None`」不是一回事**，见 [`ide::AntigravityIdeSlot`]。
    pub identity_error: Option<String>,
    /// 各模型剩余额度（画配额条用）。没登录 / 读不出来就是空的。
    pub quota: Vec<status::AntigravityModelQuota>,
    /// IDE 那份状态库最后写入的时间，本地 `YYYY-MM-DD HH:MM`。
    ///
    /// ⚠ 配额只有 IDE 上次同步时那么新，界面必须写「IDE 写入 hh:mm」，不许说成此刻。
    pub written_at: Option<String>,

    // ---- Gemini CLI 那一半 ----
    /// `GEMINI_CLI_HOME` 指向哪里。`None` = 这条槽位还没有 CLI 那一半。
    pub cli_dir: Option<String>,
    pub cli_logged_in: bool,
    pub cli_auth_state: String,
    /// CLI 那一半的凭据文件读不出来（不是「没有」）。同 `ide_unreadable`。
    pub cli_unreadable: bool,
}

#[derive(Debug, Default, Clone, Serialize, TS)]
#[ts(export)]
pub struct AntigravityAccounts {
    pub slots: Vec<AntigravityAccount>,
}

#[derive(Default, Serialize, Deserialize)]
struct Index {
    slots: Vec<Entry>,
    active: Option<String>,
    /// 使用者移除过的、从 0.30.0 老索引迁上来的目录（2026-09-25）。
    ///
    /// [`archive`] 对迁上来的那种只删索引项（目录在老位置，面板没资格动），而 [`migrate`] 每次启动都跑、
    /// 把老索引里「新清单里没有」的目录再加回来 —— 于是移除掉的账户重启就又回来了。记在这里，迁移跳过。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    retired: Vec<PathBuf>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    id: String,
    label: String,
    /// IDE 的用户数据目录。新建的槽位在 `<root>\<id>\ide-user-data`；
    /// 从老索引迁上来的**指向老位置**，见模块头。
    #[serde(default)]
    ide_dir: Option<PathBuf>,
    /// Gemini CLI 的 `GEMINI_CLI_HOME`。同上。
    #[serde(default)]
    cli_dir: Option<PathBuf>,
}

pub fn root() -> PathBuf {
    crate::paths::state_dir().join("antigravity-accounts")
}

/// 新建槽位时 IDE 那一半的位置。
pub fn ide_dir_of(slot_dir: &Path) -> PathBuf {
    slot_dir.join("ide-user-data")
}
/// 新建槽位时 CLI 那一半的位置。
pub fn cli_dir_of(slot_dir: &Path) -> PathBuf {
    slot_dir.join("cli-home")
}

fn load(root: &Path) -> Result<Index> {
    match config_io::read_optional(&root.join("index.json"))? {
        Some(bytes) => Ok(serde_json::from_slice(&bytes)?),
        None => Ok(Index::default()),
    }
}

fn save(root: &Path, index: &Index) -> Result<()> {
    config_io::ensure_plain_path(root)?;
    std::fs::create_dir_all(root)?;
    config_io::replace(
        &root.join("index.json"),
        Some(&serde_json::to_vec_pretty(index)?),
    )
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 80 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn entry_of(index: &Index, id: &str) -> Result<Entry> {
    if !valid_id(id) {
        return Err(GateError::Other("无效的反重力账户槽位".into()));
    }
    index
        .slots
        .iter()
        .find(|e| e.id == id)
        .cloned()
        .ok_or_else(|| GateError::Other("反重力账户槽位不存在".into()))
}

// ------------------------------------------------------------------ 读

fn describe(e: &Entry, active: bool) -> AntigravityAccount {
    let mut out = AntigravityAccount {
        id: e.id.clone(),
        label: e.label.clone(),
        active,
        ide_dir: e.ide_dir.as_ref().map(|p| p.display().to_string()),
        ide_logged_in: false,
        ide_auth_state: "还没有 IDE 那一半".into(),
        ide_unreadable: false,
        email: None,
        tier: None,
        identity_error: None,
        quota: Vec::new(),
        written_at: None,
        cli_dir: e.cli_dir.as_ref().map(|p| p.display().to_string()),
        cli_logged_in: false,
        cli_auth_state: "还没有 CLI 那一半".into(),
        cli_unreadable: false,
    };
    if let Some(dir) = &e.ide_dir {
        // 路径经过联结点就不读 —— 跟别处一条口径，而且要说得出是为什么。
        if let Err(err) = config_io::ensure_plain_path(dir) {
            out.ide_auth_state = "目录读不了".into();
            out.ide_unreadable = true;
            out.identity_error = Some(err.to_string());
        } else {
            let login = status::login_state(dir);
            out.ide_logged_in = login.logged_in;
            out.ide_unreadable = login.unreadable;
            out.ide_auth_state = login.detail;
            match status::identity(dir) {
                Ok(Some(id)) => {
                    out.email = id.email;
                    out.tier = id.tier_name;
                    out.written_at = Some(id.written_at);
                    out.quota = id.models;
                }
                Ok(None) => {}
                // ⛔ 不许吞掉（坑 7.49 / 7.50）：读不出来和没登录是两句话。
                Err(err) => out.identity_error = Some(err.to_string()),
            }
        }
    }
    if let Some(dir) = &e.cli_dir {
        let login = gemini::login_check(dir);
        out.cli_logged_in = login.logged_in;
        out.cli_unreadable = login.unreadable;
        out.cli_auth_state = login.detail;
    }
    out
}

pub fn list(root: &Path) -> Result<AntigravityAccounts> {
    let index = load(root)?;
    let active = index.active.as_deref();
    Ok(AntigravityAccounts {
        slots: index
            .slots
            .iter()
            .map(|e| describe(e, active == Some(e.id.as_str())))
            .collect(),
    })
}

/// 激活槽位的 IDE 那一半：`(id, 标签, user-data 目录)`。
///
/// 没有槽位 / 没激活 / 这条槽位没有 IDE 那一半 → `None`（面板起默认那份资料）。
pub fn active_ide(root: &Path) -> Result<Option<(String, String, PathBuf)>> {
    let index = load(root)?;
    let Some(id) = index.active.as_deref() else {
        return Ok(None);
    };
    let Ok(e) = entry_of(&index, id) else {
        return Ok(None);
    };
    Ok(e.ide_dir.map(|d| (e.id, e.label, d)))
}

/// 激活的那条账户**在、但还没有 IDE 那一半**时回它的 id（起 IDE 之前现建那一半用，2026-09-25）。
pub fn active_missing_ide(root: &Path) -> Result<Option<String>> {
    let index = load(root)?;
    let Some(id) = index.active.as_deref() else {
        return Ok(None);
    };
    Ok(index
        .slots
        .iter()
        .find(|e| e.id == id && e.ide_dir.is_none())
        .map(|e| e.id.clone()))
}

/// 激活槽位的 id。没有槽位 / 没激活就是 `None`。
pub fn active_id(root: &Path) -> Result<Option<String>> {
    Ok(load(root)?.active)
}

/// 一条槽位的 IDE 那一半：`(标签, user-data 目录)`。这条槽位没有 IDE 那一半 → `None`。
///
/// 给联网额度用（`qb-app::usecase::antigravity_quota`）：令牌在这个目录的状态库里。
pub fn ide_half(root: &Path, id: &str) -> Result<Option<(String, PathBuf)>> {
    let index = load(root)?;
    let e = entry_of(&index, id)?;
    Ok(e.ide_dir.map(|d| (e.label, d)))
}

/// 激活槽位的 CLI 那一半：`(标签, GEMINI_CLI_HOME, 登录了没)`。
pub fn active_cli(root: &Path) -> Result<Option<(String, PathBuf, bool)>> {
    let index = load(root)?;
    let Some(id) = index.active.as_deref() else {
        return Ok(None);
    };
    let Ok(e) = entry_of(&index, id) else {
        return Ok(None);
    };
    Ok(e.cli_dir.map(|d| {
        let (ok, _) = gemini::login_state(&d);
        (e.label, d, ok)
    }))
}

/// 一条槽位的 CLI 目录（给「登录 CLI」用）。没有那一半就现建一个。
pub fn ensure_cli_dir(root: &Path, id: &str) -> Result<PathBuf> {
    let mut index = load(root)?;
    let e = entry_of(&index, id)?;
    if let Some(d) = e.cli_dir {
        return Ok(d);
    }
    let dir = cli_dir_of(&root.join(id));
    config_io::ensure_plain_path(&dir)?;
    std::fs::create_dir_all(dir.join(".gemini"))?;
    for s in index.slots.iter_mut() {
        if s.id == id {
            s.cli_dir = Some(dir.clone());
        }
    }
    save(root, &index)?;
    Ok(dir)
}

/// 一条槽位的 IDE 目录（给「登录 / 打开 IDE」用）。没有那一半就现建一个。
pub fn ensure_ide_dir(root: &Path, id: &str, seed_from: Option<&Path>) -> Result<PathBuf> {
    let mut index = load(root)?;
    let e = entry_of(&index, id)?;
    if let Some(d) = e.ide_dir {
        return Ok(d);
    }
    let dir = ide_dir_of(&root.join(id));
    seed_ide_dir(&dir, seed_from)?;
    for s in index.slots.iter_mut() {
        if s.id == id {
            s.ide_dir = Some(dir.clone());
        }
    }
    save(root, &index)?;
    Ok(dir)
}

// ------------------------------------------------------------------ 写

/// 只从默认资料目录复制 `User\settings.json` / `User\keybindings.json`（偏好）。
///
/// ⛔ `globalStorage` 一个字节不搬 —— 复制式迁移造过两份 refresh token（§7.29）。
fn seed_ide_dir(dir: &Path, seed_from: Option<&Path>) -> Result<()> {
    config_io::ensure_plain_path(dir)?;
    let user = dir.join("User");
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
    Ok(())
}

/// 新建一个账户槽位：**两半一起建**。
pub fn create(root: &Path, label: &str, seed_from: Option<&Path>) -> Result<String> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 40 || label.chars().any(char::is_control) {
        return Err(GateError::Other("账户名称需要 1–40 个可见字符".into()));
    }
    let mut index = load(root)?;
    if index.slots.iter().any(|e| e.label == label) {
        return Err(GateError::Other("已有同名反重力账户".into()));
    }
    config_io::ensure_plain_path(root)?;
    let id = config_io::id();
    let dir = root.join(&id);
    let ide = ide_dir_of(&dir);
    let cli = cli_dir_of(&dir);
    seed_ide_dir(&ide, seed_from)?;
    std::fs::create_dir_all(cli.join(".gemini"))?;
    index.slots.push(Entry {
        id: id.clone(),
        label: label.into(),
        ide_dir: Some(ide),
        cli_dir: Some(cli),
    });
    if index.active.is_none() {
        index.active = Some(id.clone());
    }
    save(root, &index)?;
    Ok(id)
}

pub fn select(root: &Path, id: &str) -> Result<()> {
    let mut index = load(root)?;
    entry_of(&index, id)?;
    index.active = Some(id.into());
    save(root, &index)
}

pub fn rename(root: &Path, id: &str, label: &str) -> Result<()> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 40 || label.chars().any(char::is_control) {
        return Err(GateError::Other("账户名称需要 1–40 个可见字符".into()));
    }
    let mut index = load(root)?;
    entry_of(&index, id)?;
    if index.slots.iter().any(|e| e.id != id && e.label == label) {
        return Err(GateError::Other("已有同名反重力账户".into()));
    }
    for s in index.slots.iter_mut() {
        if s.id == id {
            s.label = label.into();
        }
    }
    save(root, &index)
}

/// 把 `from` 那条的两半并进 `into`，然后把 `from` 从清单里去掉。
///
/// ⛔ **只动索引，不删也不搬任何目录** —— 并错了就再建一条指回去。
/// 目标那一半已经有东西时拒绝：覆盖掉一个登录目录是不可撤销的。
pub fn attach(root: &Path, into: &str, from: &str) -> Result<()> {
    if into == from {
        return Err(GateError::Other("不能并进它自己".into()));
    }
    let mut index = load(root)?;
    let target = entry_of(&index, into)?;
    let source = entry_of(&index, from)?;
    if index.active.as_deref() == Some(from) {
        return Err(GateError::Other(
            "请先切换到其他账户，再把这一条并进去".into(),
        ));
    }
    if target.ide_dir.is_some() && source.ide_dir.is_some() {
        return Err(GateError::Other(
            "两条都有 IDE 那一半，并不了：先移除其中一条的 IDE 资料再并".into(),
        ));
    }
    if target.cli_dir.is_some() && source.cli_dir.is_some() {
        return Err(GateError::Other(
            "两条都有 Gemini CLI 那一半，并不了：先移除其中一条再并".into(),
        ));
    }
    for s in index.slots.iter_mut() {
        if s.id == into {
            s.ide_dir = s.ide_dir.clone().or_else(|| source.ide_dir.clone());
            s.cli_dir = s.cli_dir.clone().or_else(|| source.cli_dir.clone());
        }
    }
    index.slots.retain(|s| s.id != from);
    save(root, &index)
}

/// 从清单里移除。激活中的不许移。
///
/// **只移除新布局自己建的目录**（搬进 `archived\`）。指向老位置的那些**原样留着** ——
/// 那些目录是 0.30.0 的槽位，面板没搬过它们，也就没有资格替使用者动它们。
pub fn archive(root: &Path, id: &str) -> Result<()> {
    let mut index = load(root)?;
    let entry = entry_of(&index, id)?;
    if index.active.as_deref() == Some(id) {
        return Err(GateError::Other("请先切换到其他账户，再移除这一条".into()));
    }
    let own = root.join(id);
    if own.is_dir() {
        let archive = root.join("archived");
        config_io::ensure_plain_path(&archive)?;
        std::fs::create_dir_all(&archive)?;
        std::fs::rename(&own, archive.join(format!("{}-{}", id, config_io::id())))?;
    }
    // 指向老位置的那两半记进 `retired`：不然下次启动 `migrate` 又把它们当成「新清单里没有」加回来。
    for dir in [entry.ide_dir, entry.cli_dir].into_iter().flatten() {
        if !dir.starts_with(&own) && !index.retired.contains(&dir) {
            index.retired.push(dir);
        }
    }
    index.slots.retain(|s| s.id != id);
    save(root, &index)
}

// ------------------------------------------------------------------ 迁移

/// 把 0.30.0 的两份索引升上来。**幂等**：面板每次启动都调它。
///
/// 规矩见模块头：不猜配对、不搬目录、已经迁过的不再迁。
pub fn migrate(root: &Path, ide_root: &Path, gemini_root: &Path) -> Result<u32> {
    let mut index = load(root)?;
    // 使用者移除过的老目录也算「已知」—— 不然移除一条，下次启动它又回来（2026-09-25）。
    let known: std::collections::HashSet<PathBuf> = index
        .slots
        .iter()
        .flat_map(|e| e.ide_dir.iter().chain(e.cli_dir.iter()).cloned())
        .chain(index.retired.iter().cloned())
        .collect();
    let mut added = 0u32;
    // 老索引各自的「当前」。迁完之后沿用它，不是随手取第一条（原来 IDE 那几条排在前面，
    // 第一条赢 —— 升级之后 IDE 起的是另一份资料，Gemini CLI 那边也报「没有激活的槽位」）。
    let mut old_active: Vec<PathBuf> = Vec::new();

    // 老的 IDE 槽位。
    if let Ok(old) = ide::list(ide_root) {
        for s in old.slots {
            let dir = PathBuf::from(&s.user_data);
            if s.active {
                old_active.push(dir.clone());
            }
            if known.contains(&dir) {
                continue;
            }
            index.slots.push(Entry {
                id: config_io::id(),
                label: unique_label(&index, &s.label),
                ide_dir: Some(dir),
                cli_dir: None,
            });
            added += 1;
        }
    }
    // 老的 Gemini CLI 槽位。**单独成行** —— 不跟上面任何一条配对。
    if let Ok(old) = gemini::list(gemini_root) {
        for s in old.slots {
            let dir = PathBuf::from(&s.home);
            if s.active {
                old_active.push(dir.clone());
            }
            if known.contains(&dir) {
                continue;
            }
            index.slots.push(Entry {
                id: config_io::id(),
                label: unique_label(&index, &s.label),
                ide_dir: None,
                cli_dir: Some(dir),
            });
            added += 1;
        }
    }
    if added == 0 {
        return Ok(0);
    }
    if index.active.is_none() {
        index.active = old_active
            .iter()
            .find_map(|want| {
                index
                    .slots
                    .iter()
                    .find(|e| e.ide_dir.as_ref() == Some(want) || e.cli_dir.as_ref() == Some(want))
            })
            .or_else(|| index.slots.first())
            .map(|e| e.id.clone());
    }
    save(root, &index)?;
    Ok(added)
}

/// 老的两套索引里可能有同名的（一个 IDE 槽位、一个 CLI 槽位都叫「测试」）。
/// 同名在新清单里是非法的，但迁移**不能因此失败** —— 加后缀，别丢数据。
fn unique_label(index: &Index, want: &str) -> String {
    let base = if want.trim().is_empty() {
        "未命名"
    } else {
        want.trim()
    };
    if !index.slots.iter().any(|e| e.label == base) {
        return base.into();
    }
    for n in 2..100 {
        let candidate = format!("{base} ({n})");
        if !index.slots.iter().any(|e| e.label == candidate) {
            return candidate;
        }
    }
    format!("{base} ({})", config_io::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_with(slots: &[(&str, &str, Option<&str>, Option<&str>)]) -> Index {
        Index {
            slots: slots
                .iter()
                .map(|(id, label, ide, cli)| Entry {
                    id: (*id).into(),
                    label: (*label).into(),
                    ide_dir: ide.map(PathBuf::from),
                    cli_dir: cli.map(PathBuf::from),
                })
                .collect(),
            active: None,
            retired: Vec::new(),
        }
    }

    /// 一台临时的假机器：新索引根 + 0.30.0 的两份老索引。结束时整棵删掉。
    struct Roots(PathBuf);

    impl Roots {
        fn new(tag: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "qbgate-agaccount-{tag}-{}-{}",
                std::process::id(),
                chrono::Local::now().format("%H%M%S%f")
            ));
            std::fs::create_dir_all(base.join("new")).unwrap();
            std::fs::create_dir_all(base.join("ide")).unwrap();
            std::fs::create_dir_all(base.join("gemini")).unwrap();
            Self(base)
        }
        fn old(&self, which: &str, index: &str) {
            std::fs::write(self.0.join(which).join("index.json"), index).unwrap();
        }
        fn migrate(&self) -> u32 {
            migrate(
                &self.0.join("new"),
                &self.0.join("ide"),
                &self.0.join("gemini"),
            )
            .unwrap()
        }
        fn load(&self) -> Index {
            load(&self.0.join("new")).unwrap()
        }
    }

    impl Drop for Roots {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// 2026-09-25：移除一条从老索引迁上来的账户，重启（再跑一次迁移）它不许回来；
    /// 迁移时沿用老索引各自的「当前」，不是随手取第一条。
    #[test]
    fn migration_keeps_the_old_active_slot_and_never_brings_back_a_removed_one() {
        let r = Roots::new("migrate");
        r.old(
            "ide",
            r#"{"slots":[{"id":"i1","label":"甲"},{"id":"i2","label":"乙"}],"active":"i2"}"#,
        );
        r.old(
            "gemini",
            r#"{"slots":[{"id":"g1","label":"丙"}],"active":"g1"}"#,
        );
        assert_eq!(r.migrate(), 3);
        let idx = r.load();
        let active = idx.active.clone().unwrap();
        let active_entry = idx.slots.iter().find(|e| e.id == active).unwrap();
        assert_eq!(
            active_entry.label, "乙",
            "老 IDE 索引的当前是 i2，不是排在第一的 i1"
        );

        // 移除一条不是当前的（「丙」，CLI 那一半在老位置）。
        let gone = idx
            .slots
            .iter()
            .find(|e| e.label == "丙")
            .unwrap()
            .id
            .clone();
        archive(&r.0.join("new"), &gone).unwrap();
        assert_eq!(r.migrate(), 0, "再跑一次迁移（= 重启）不许把它加回来");
        assert!(r.load().slots.iter().all(|e| e.label != "丙"));
        assert_eq!(r.load().slots.len(), 2);
    }

    /// 「没有凭据文件」是未登录、不是读不出来；有了就是已登录。（真读不出来要文件系统报错，
    /// 单测里凑不出来 —— 那一支只是把 `NotFound` 以外的错误标成 `unreadable`。）
    #[test]
    fn a_missing_cli_credential_is_logged_out_not_unreadable() {
        let r = Roots::new("cli-unreadable");
        let home = r.0.join("cli");
        std::fs::create_dir_all(home.join(".gemini")).unwrap();
        let c = gemini::login_check(&home);
        assert!(!c.logged_in && !c.unreadable, "没有凭据文件：未登录");
        std::fs::write(gemini::creds_path(&home), r#"{"access_token":"x"}"#).unwrap();
        assert!(gemini::login_check(&home).logged_in);
    }

    #[test]
    fn a_slot_with_only_one_half_says_so_instead_of_pretending_to_be_logged_out() {
        // 「还没有 CLI 那一半」和「有但没登录」是两句话。
        let e = Entry {
            id: "a".into(),
            label: "工作".into(),
            ide_dir: None,
            cli_dir: None,
        };
        let a = describe(&e, false);
        assert_eq!(a.ide_auth_state, "还没有 IDE 那一半");
        assert_eq!(a.cli_auth_state, "还没有 CLI 那一半");
        assert!(!a.ide_logged_in && !a.cli_logged_in);
        assert!(a.quota.is_empty());
        assert_eq!(a.identity_error, None);
    }

    #[test]
    fn duplicate_labels_from_the_two_old_indexes_get_suffixed_instead_of_dropped() {
        // 老的两套索引里「测试」各有一条是很正常的事；同名在新清单里非法，
        // 但迁移绝不能因此丢掉一条槽位。
        let mut idx = index_with(&[("a", "测试", Some("d1"), None)]);
        let l = unique_label(&idx, "测试");
        assert_eq!(l, "测试 (2)");
        idx.slots.push(Entry {
            id: "b".into(),
            label: l,
            ide_dir: None,
            cli_dir: Some("d2".into()),
        });
        assert_eq!(unique_label(&idx, "测试"), "测试 (3)");
        assert_eq!(unique_label(&idx, "   "), "未命名");
    }

    #[test]
    fn attaching_refuses_when_both_sides_already_hold_the_same_half() {
        // 覆盖掉一个登录目录是不可撤销的，宁可拒绝。
        let idx = index_with(&[
            ("a", "甲", Some("ide-a"), None),
            ("b", "乙", Some("ide-b"), None),
        ]);
        let target = entry_of(&idx, "a").unwrap();
        let source = entry_of(&idx, "b").unwrap();
        assert!(target.ide_dir.is_some() && source.ide_dir.is_some());
    }

    #[test]
    fn ids_that_could_walk_out_of_the_root_are_rejected() {
        for bad in ["", "..", "a/b", r"a\b", "a.b", &"x".repeat(81)] {
            assert!(!valid_id(bad), "{bad:?} 不该通过");
        }
        assert!(valid_id("20260921-abcdef"));
    }

    #[test]
    fn the_two_halves_live_under_the_slot_and_never_share_a_directory() {
        let slot = PathBuf::from("root").join("id");
        assert_ne!(ide_dir_of(&slot), cli_dir_of(&slot));
        assert!(ide_dir_of(&slot).starts_with(&slot));
        assert!(cli_dir_of(&slot).starts_with(&slot));
    }
}
