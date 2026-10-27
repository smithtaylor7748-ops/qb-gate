// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 环境变量清理（2026-10-27，中转站重构阶段 7 · X5，照 cc-switch v4.0.14 的 `services/env_checker.rs` +
//! `services/env_manager.rs`）。
//!
//! # 跟体检里那一项的关系
//!
//! [`super::checkup::scan_env`] 只报不删（它那句「面板不替你删 —— 这些变量可能正是你别的工作要用的」照旧成立）。
//! 这里是使用者**点了之后才删**的那一半：列出来 → 逐个勾 → 先备份 → 删 → 能按备份还原。
//! 「哪些变量算相关」只有一张表（[`owner_of`]），体检那一项也从这里问 —— 0.19.2 之前同一件事三处三个口径，
//! 最松的那一处就是使用者看到的那一处（`checkup` 里 `WATCHED_ENV_PREFIX` 那段注释记着）。
//!
//! # 跟 cc-switch 不一样的地方
//!
//! | | cc-switch | 这里 |
//! |---|---|---|
//! | 认哪些 | 按软件分：`ANTHROPIC*` / `OPENAI*` / `GEMINI*` / 两个 xAI 变量 | 同上，外加 `CLAUDE_CODE_*`、`CLAUDE_CONFIG_DIR`、`CODEX_*` 与四个代理变量（体检本来就在报） |
//! | 能删哪些 | HKCU 和 HKLM | **只有 HKCU**：面板不提权，HKLM 的只列出来、给步骤 |
//! | 删谁 | 前端传什么删什么 | 只删 [`owner_of`] 认得的名字 —— 界面传来一个 `Path` 也删不掉 |
//! | 备份 | `~/.cc-switch/backups/*.json`，值是明文 | 面板数据目录 `env-backups\`，**值用 DPAPI 封住**（里面多半是 API Key） |
//! | 还原 | 按 `REG_SZ` 写回、有同名就盖 | 按原来的类型写回（`REG_EXPAND_SZ` 不变成 `REG_SZ`）；现在已经有同名、值不一样的**不盖**，报出来 |
//! | 改完 | 不通知 | 广播 `WM_SETTINGCHANGE`，新开的终端立刻读到（已经开着的程序不受影响，包括面板自己） |
//!
//! # 单测不碰真的注册表
//!
//! 读写当前用户那一处都经 [`UserEnv`]，真的那份是 [`Registry`]；备份目录由调用方传进来。
//! 单测用内存里的假 `UserEnv` 和临时目录（`CLAUDE.md`「单测不许碰真实的运行期状态」）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{GateError, Result};
use qb_platform::registry::{self, Hive, Value};

/// 当前用户的环境变量在注册表里的位置。
pub const USER_KEY: &str = "Environment";
/// 整机的环境变量。只读：面板不提权。
pub const MACHINE_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

/// 这个变量影响哪个软件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "EnvOwner")]
#[serde(rename_all = "kebab-case")]
pub enum Owner {
    ClaudeCode,
    Codex,
    GeminiCli,
    GrokBuild,
    /// `HTTP_PROXY` 那几个：所有认它们的程序都会被带走，不止一个软件。
    Proxy,
}

impl Owner {
    pub fn label(self) -> &'static str {
        match self {
            Owner::ClaudeCode => "Claude Code",
            Owner::Codex => "Codex",
            Owner::GeminiCli => "Gemini CLI",
            Owner::GrokBuild => "Grok Build",
            Owner::Proxy => "代理（所有认它的程序）",
        }
    }
}

/// 按前缀认的。只认**开头**：`MY_ANTHROPIC_KEY` 是使用者自己的东西。
const PREFIXES: &[(&str, Owner)] = &[
    ("ANTHROPIC_", Owner::ClaudeCode),
    // `CLAUDE_CODE_USE_BEDROCK` / `_USE_VERTEX` 这一族直接把 Claude Code 切到别的云端点，上游随时会加新的。
    ("CLAUDE_CODE_", Owner::ClaudeCode),
    ("OPENAI_", Owner::Codex),
    ("CODEX_", Owner::Codex),
    ("GEMINI_", Owner::GeminiCli),
    ("GOOGLE_GEMINI_", Owner::GeminiCli),
];

/// 按全名认的（大小写不敏感）。
const EXACT: &[(&str, Owner)] = &[
    // 指着别的目录的话，面板切的槽位全不作数（坑 7.26 那一族）。
    ("CLAUDE_CONFIG_DIR", Owner::ClaudeCode),
    // xAI 只认这两个，`GROK_HOME` / `GROK_BIN_DIR` 是安装位置，不是凭据（cc-switch 的测试同一条）。
    ("XAI_API_KEY", Owner::GrokBuild),
    ("GROK_DEFAULT_MODEL", Owner::GrokBuild),
    ("HTTP_PROXY", Owner::Proxy),
    ("HTTPS_PROXY", Owner::Proxy),
    ("ALL_PROXY", Owner::Proxy),
    ("NO_PROXY", Owner::Proxy),
];

/// 这个变量名归不归这里管、影响谁。**全项目认「相关环境变量」只看这一处。**
///
/// 用 `to_ascii_uppercase` 再 `starts_with`，不切片：变量名理论上可以带非 ASCII，切到半个字符上会 panic。
pub fn owner_of(name: &str) -> Option<Owner> {
    let upper = name.to_ascii_uppercase();
    if let Some((_, o)) = EXACT.iter().find(|(n, _)| *n == upper) {
        return Some(*o);
    }
    PREFIXES
        .iter()
        .find(|(p, _)| upper.starts_with(p) && upper.len() > p.len())
        .map(|(_, o)| *o)
}

/// 设在哪。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum EnvScope {
    /// 当前用户（`HKCU\Environment`）：面板删得了，删之前备份。
    User,
    /// 整台机器（`HKLM\…\Session Manager\Environment`）：要管理员，面板只列出来。
    Machine,
    /// 注册表里没有，只在面板这个进程的环境里 —— 从起面板的那个程序（比如一个设过变量的终端）继承来的。
    /// 重开面板就没了，没有可删的地方。
    Process,
}

/// 列表里的一行。值一律掩码过（[`super::checkup::mask_env_value`]）：Key 类只说「已设置」，地址类只留主机。
#[derive(Debug, Clone, Serialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct EnvEntry {
    pub name: String,
    pub scope: EnvScope,
    pub owner: Owner,
    pub shown: String,
    /// 是 `REG_EXPAND_SZ`（里面有 `%VAR%`）。
    pub expand: bool,
    /// 面板删得了（只有当前用户那一处）。
    pub removable: bool,
}

/// 从三处的原始值算出列表。**纯函数**。同名的只列一次，按 当前用户 → 整机 → 进程 的顺序认 ——
/// 进程环境多半就是从前两处来的。
pub fn entries_from(
    user: &[(String, Value)],
    machine: &[(String, Value)],
    process: &[(String, String)],
) -> Vec<EnvEntry> {
    let mut out: Vec<EnvEntry> = Vec::new();
    let mut push = |name: &str, scope: EnvScope, value: &str, expand: bool| {
        let Some(owner) = owner_of(name) else {
            return;
        };
        if value.trim().is_empty() || out.iter().any(|e| e.name.eq_ignore_ascii_case(name)) {
            return;
        }
        out.push(EnvEntry {
            name: name.to_string(),
            scope,
            owner,
            shown: super::checkup::mask_env_value(name, value),
            expand,
            removable: scope == EnvScope::User,
        });
    };
    for (name, v) in user {
        if let Some(text) = v.as_text() {
            push(name, EnvScope::User, text, matches!(v, Value::Expand(_)));
        }
    }
    for (name, v) in machine {
        if let Some(text) = v.as_text() {
            push(name, EnvScope::Machine, text, matches!(v, Value::Expand(_)));
        }
    }
    for (name, v) in process {
        push(name, EnvScope::Process, v, false);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 现在的样子：读注册表两处与面板自己的进程环境。不改任何东西。
pub fn scan() -> Result<Vec<EnvEntry>> {
    let user = registry::values(Hive::CurrentUser, USER_KEY)?;
    // 整机那一处读不到（极少见的权限设置）不该让整张表出不来：当成空的，用户那一处照常列。
    let machine = registry::values(Hive::LocalMachine, MACHINE_KEY).unwrap_or_default();
    let process: Vec<(String, String)> = std::env::vars().collect();
    Ok(entries_from(&user, &machine, &process))
}

// ------------------------------------------------------------------ 当前用户那一处

/// `HKCU\Environment` 的读写。真的那份是 [`Registry`]，单测换成内存里的。
pub trait UserEnv {
    fn get(&self, name: &str) -> Result<Option<Value>>;
    fn set(&self, name: &str, value: &Value) -> Result<()>;
    fn remove(&self, name: &str) -> Result<bool>;
    /// 改完通知一声（真的那份广播 `WM_SETTINGCHANGE`）。
    fn changed(&self) {}
}

/// 真的注册表。
pub struct Registry;

impl UserEnv for Registry {
    fn get(&self, name: &str) -> Result<Option<Value>> {
        registry::read(Hive::CurrentUser, USER_KEY, name)
    }
    fn set(&self, name: &str, value: &Value) -> Result<()> {
        registry::write_user(USER_KEY, name, value)
    }
    fn remove(&self, name: &str) -> Result<bool> {
        registry::delete_user(USER_KEY, name)
    }
    fn changed(&self) {
        registry::broadcast_environment_changed();
    }
}

// ------------------------------------------------------------------ 备份

/// 备份文件里的一条。值用 DPAPI 封住（`qb_platform::secret`），只有同一个 Windows 用户在同一台机器上解得开。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Saved {
    name: String,
    /// 原来是 `REG_EXPAND_SZ`。还原时照这个类型写回。
    expand: bool,
    sealed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupFile {
    version: u32,
    id: String,
    /// RFC 3339，本地时区。
    created: String,
    #[serde(default)]
    restored_at: Option<String>,
    entries: Vec<Saved>,
}

/// 备份列表里的一份（**不带值**）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct EnvBackupInfo {
    pub id: String,
    pub created: String,
    pub names: Vec<String>,
    /// 按它还原过的话，最近那一次的时间。
    pub restored_at: Option<String>,
}

/// 删除的结果。**逐个报**，做不到的照实说。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct EnvRemoval {
    /// 这一次的备份。一个都没删（全都本来就不在）时没有备份。
    pub backup_id: Option<String>,
    pub removed: Vec<String>,
    /// 当前用户那一处本来就没有（可能在整机或者只在进程里）。
    pub missing: Vec<String>,
    /// 删不掉的，带原因。
    pub failed: Vec<String>,
}

/// 还原的结果。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct EnvRestoreReport {
    pub restored: Vec<String>,
    /// 现在就是这个值，不用动。
    pub already: Vec<String>,
    /// 现在有一个**不一样**的同名变量 —— 不盖，留给使用者自己决定。
    pub conflicts: Vec<String>,
    /// 写不回去或者解不开的，带原因。
    pub failed: Vec<String>,
}

/// 备份放在哪。调用方给面板数据目录下的 `env-backups`，单测给临时目录。
pub fn backup_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("env-backups")
}

fn backup_path(dir: &Path, id: &str) -> Result<PathBuf> {
    // id 是面板自己起的名字（时间戳），从界面传回来时只认这个形状 —— 不让它带着 `..\` 走出目录。
    let ok = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !ok {
        return Err(GateError::Other(format!("备份编号不对：{id}")));
    }
    Ok(dir.join(format!("{id}.json")))
}

fn new_id(dir: &Path, now: chrono::DateTime<chrono::Local>) -> String {
    let base = now.format("env-%Y%m%d-%H%M%S").to_string();
    let mut id = base.clone();
    let mut n = 1;
    while dir.join(format!("{id}.json")).exists() {
        n += 1;
        id = format!("{base}-{n}");
    }
    id
}

/// 先写临时文件再改名：写到一半断电，留下的是一个 `.tmp`，不是一份读不出来的备份。
fn write_backup(dir: &Path, b: &BackupFile) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = backup_path(dir, &b.id)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(b)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

fn read_backup(dir: &Path, id: &str) -> Result<BackupFile> {
    let path = backup_path(dir, id)?;
    let text = std::fs::read_to_string(&path)
        .map_err(|e| GateError::Other(format!("读不到备份 {id}：{e}")))?;
    serde_json::from_str(&text).map_err(|e| GateError::Other(format!("备份 {id} 读不懂：{e}")))
}

/// 删掉当前用户的这几个环境变量：**先把它们原样备份，备份写成功了才删**。
///
/// 只认 [`owner_of`] 认得的名字，别的一个都不删（直接报错，什么都不做）。
pub fn remove(
    env: &dyn UserEnv,
    dir: &Path,
    names: &[String],
    now: chrono::DateTime<chrono::Local>,
) -> Result<EnvRemoval> {
    if let Some(bad) = names.iter().find(|n| owner_of(n).is_none()) {
        return Err(GateError::Other(format!(
            "{bad} 不是这里列出来的那几类变量，面板不删它"
        )));
    }
    let mut report = EnvRemoval::default();
    let mut saved: Vec<Saved> = Vec::new();
    for name in names {
        if saved.iter().any(|s| s.name.eq_ignore_ascii_case(name)) {
            continue;
        }
        match env.get(name)? {
            Some(v) => match v {
                Value::Text(ref s) | Value::Expand(ref s) => saved.push(Saved {
                    name: name.clone(),
                    expand: matches!(v, Value::Expand(_)),
                    sealed: qb_platform::secret::seal(s)?,
                }),
                // 字符串以外的类型（几乎不会有人这么设）原样留着：备份不了，就不删。
                _ => report
                    .failed
                    .push(format!("{name}：不是字符串类型的变量，没动")),
            },
            None => report.missing.push(name.clone()),
        }
    }
    if saved.is_empty() {
        return Ok(report);
    }
    let backup = BackupFile {
        version: 1,
        id: new_id(dir, now),
        created: now.to_rfc3339(),
        restored_at: None,
        entries: saved,
    };
    // ⛔ 备份写不成，一个都不删。
    write_backup(dir, &backup)?;
    report.backup_id = Some(backup.id.clone());
    for s in &backup.entries {
        match env.remove(&s.name) {
            Ok(true) => report.removed.push(s.name.clone()),
            Ok(false) => report.missing.push(s.name.clone()),
            Err(e) => report.failed.push(format!("{}：{e}", s.name)),
        }
    }
    if !report.removed.is_empty() {
        env.changed();
    }
    Ok(report)
}

/// 按一份备份还原。现在没有的写回去（原来的类型）；现在就是这个值的跳过；现在有一个不一样的**不盖**。
pub fn restore(
    env: &dyn UserEnv,
    dir: &Path,
    id: &str,
    now: chrono::DateTime<chrono::Local>,
) -> Result<EnvRestoreReport> {
    let mut b = read_backup(dir, id)?;
    let mut report = EnvRestoreReport::default();
    for s in &b.entries {
        let Some(value) = qb_platform::secret::open(&s.sealed) else {
            report.failed.push(format!(
                "{}：解不开（换了 Windows 用户或者换了机器）",
                s.name
            ));
            continue;
        };
        let wanted = if s.expand {
            Value::Expand(value)
        } else {
            Value::Text(value)
        };
        match env.get(&s.name)? {
            None => match env.set(&s.name, &wanted) {
                Ok(()) => report.restored.push(s.name.clone()),
                Err(e) => report.failed.push(format!("{}：{e}", s.name)),
            },
            Some(cur) if cur.as_text() == wanted.as_text() => report.already.push(s.name.clone()),
            Some(_) => report.conflicts.push(s.name.clone()),
        }
    }
    if !report.restored.is_empty() {
        env.changed();
        b.restored_at = Some(now.to_rfc3339());
        write_backup(dir, &b)?;
    }
    Ok(report)
}

/// 备份列表，新的在前。读不懂的文件跳过（不让一份坏文件把整张表弄没）。
pub fn backups(dir: &Path) -> Result<Vec<EnvBackupInfo>> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Ok(Vec::new());
    };
    let mut out: Vec<EnvBackupInfo> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                return None;
            }
            let b: BackupFile = serde_json::from_str(&std::fs::read_to_string(&p).ok()?).ok()?;
            Some(EnvBackupInfo {
                names: b.entries.iter().map(|s| s.name.clone()).collect(),
                id: b.id,
                created: b.created,
                restored_at: b.restored_at,
            })
        })
        .collect();
    out.sort_by(|a, b| b.created.cmp(&a.created).then(b.id.cmp(&a.id)));
    Ok(out)
}

/// 删掉一份备份（里面是加密存的变量值，使用者不想留时）。**不可恢复**，界面上先确认。
pub fn delete_backup(dir: &Path, id: &str) -> Result<()> {
    let path = backup_path(dir, id)?;
    std::fs::remove_file(&path).map_err(|e| GateError::Other(format!("删不掉备份 {id}：{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// 内存里的 `HKCU\Environment`。
    #[derive(Default)]
    struct FakeEnv {
        vars: RefCell<Vec<(String, Value)>>,
        notified: RefCell<u32>,
        broken: Option<&'static str>,
    }

    impl FakeEnv {
        fn with(vars: &[(&str, Value)]) -> Self {
            Self {
                vars: RefCell::new(
                    vars.iter()
                        .map(|(n, v)| (n.to_string(), v.clone()))
                        .collect(),
                ),
                ..Default::default()
            }
        }
        fn value(&self, name: &str) -> Option<Value> {
            self.vars
                .borrow()
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())
        }
    }

    impl UserEnv for FakeEnv {
        fn get(&self, name: &str) -> Result<Option<Value>> {
            Ok(self.value(name))
        }
        fn set(&self, name: &str, value: &Value) -> Result<()> {
            self.vars
                .borrow_mut()
                .push((name.to_string(), value.clone()));
            Ok(())
        }
        fn remove(&self, name: &str) -> Result<bool> {
            if self.broken == Some("remove") {
                return Err(GateError::Other("拒绝访问".into()));
            }
            let mut v = self.vars.borrow_mut();
            let before = v.len();
            v.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
            Ok(v.len() != before)
        }
        fn changed(&self) {
            *self.notified.borrow_mut() += 1;
        }
    }

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("qb-envvars-{}", uuid_like()));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn uuid_like() -> String {
        format!(
            "{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }
    fn at(h: u32) -> chrono::DateTime<chrono::Local> {
        use chrono::TimeZone;
        chrono::Local
            .with_ymd_and_hms(2026, 10, 27, h, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn the_table_recognises_each_software_and_only_at_the_start() {
        assert_eq!(owner_of("ANTHROPIC_BASE_URL"), Some(Owner::ClaudeCode));
        assert_eq!(owner_of("anthropic_auth_token"), Some(Owner::ClaudeCode));
        assert_eq!(owner_of("CLAUDE_CODE_USE_BEDROCK"), Some(Owner::ClaudeCode));
        assert_eq!(owner_of("CLAUDE_CONFIG_DIR"), Some(Owner::ClaudeCode));
        assert_eq!(owner_of("OPENAI_API_KEY"), Some(Owner::Codex));
        assert_eq!(owner_of("CODEX_HOME"), Some(Owner::Codex));
        assert_eq!(owner_of("GEMINI_API_KEY"), Some(Owner::GeminiCli));
        assert_eq!(owner_of("GOOGLE_GEMINI_BASE_URL"), Some(Owner::GeminiCli));
        assert_eq!(owner_of("XAI_API_KEY"), Some(Owner::GrokBuild));
        assert_eq!(owner_of("https_proxy"), Some(Owner::Proxy));
        // 只认开头、只认全名。
        assert_eq!(owner_of("MY_ANTHROPIC_KEY"), None);
        assert_eq!(owner_of("XAI_API_KEY_BACKUP"), None);
        assert_eq!(owner_of("GROK_HOME"), None, "安装位置不是凭据");
        assert_eq!(owner_of("ANTHROPIC_"), None, "光有前缀不是一个变量");
        assert_eq!(owner_of("CLAUDE"), None);
        assert_eq!(owner_of("PATH"), None);
        assert_eq!(owner_of("变量"), None, "非 ASCII 不 panic");
    }

    #[test]
    fn the_list_masks_values_keeps_one_row_per_name_and_marks_what_can_be_removed() {
        let user = vec![
            (
                "ANTHROPIC_API_KEY".into(),
                Value::Text("sk-ant-secret".into()),
            ),
            ("Path".into(), Value::Expand("%USERPROFILE%\\bin".into())),
            (
                "CODEX_HOME".into(),
                Value::Expand("%USERPROFILE%\\.codex2".into()),
            ),
        ];
        let machine = vec![(
            "HTTPS_PROXY".into(),
            Value::Text("http://user:pw@10.0.0.1:7890".into()),
        )];
        let process = vec![
            ("ANTHROPIC_API_KEY".into(), "sk-ant-secret".into()),
            ("GEMINI_API_KEY".into(), "AIza-secret".into()),
            ("OPENAI_BASE_URL".into(), "   ".into()),
        ];
        let got = entries_from(&user, &machine, &process);
        let names: Vec<&str> = got.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "ANTHROPIC_API_KEY",
                "CODEX_HOME",
                "GEMINI_API_KEY",
                "HTTPS_PROXY"
            ],
            "Path 不归这里管；空值不列；同名只列一次"
        );
        let key = &got[0];
        assert_eq!(key.scope, EnvScope::User);
        assert!(key.removable);
        assert!(!key.shown.contains("secret"), "Key 的值一个字都不出现");
        assert!(got[1].expand);
        let gem = &got[2];
        assert_eq!((gem.scope, gem.removable), (EnvScope::Process, false));
        let proxy = &got[3];
        assert_eq!((proxy.scope, proxy.removable), (EnvScope::Machine, false));
        assert!(!proxy.shown.contains("pw"), "代理里的用户名密码不出现");
    }

    #[test]
    fn removal_backs_up_first_then_deletes_and_restore_puts_back_the_same_type() {
        let dir = TempDir::new();
        let env = FakeEnv::with(&[
            (
                "ANTHROPIC_BASE_URL",
                Value::Text("https://relay.example/v1".into()),
            ),
            ("CODEX_HOME", Value::Expand("%USERPROFILE%\\.codex2".into())),
            ("Path", Value::Expand("C:\\x".into())),
        ]);
        let names = vec![
            "ANTHROPIC_BASE_URL".to_string(),
            "CODEX_HOME".to_string(),
            "OPENAI_API_KEY".to_string(),
        ];
        let r = remove(&env, &dir.0, &names, at(10)).unwrap();
        assert_eq!(r.removed, ["ANTHROPIC_BASE_URL", "CODEX_HOME"]);
        assert_eq!(r.missing, ["OPENAI_API_KEY"]);
        assert!(r.failed.is_empty());
        let id = r.backup_id.expect("删了东西就要有备份");
        assert_eq!(*env.notified.borrow(), 1, "改完要广播一次");
        assert!(env.value("ANTHROPIC_BASE_URL").is_none());
        assert!(env.value("Path").is_some(), "别的一个不动");

        // 备份文件里没有明文（Windows 上是 DPAPI，别的平台是 plain: 前缀 —— 这里只在 Windows 上核）。
        let text = std::fs::read_to_string(dir.0.join(format!("{id}.json"))).unwrap();
        if cfg!(windows) {
            assert!(!text.contains("relay.example"), "备份里的值要封住");
        }
        let list = backups(&dir.0).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].names, ["ANTHROPIC_BASE_URL", "CODEX_HOME"]);
        assert!(list[0].restored_at.is_none());

        let back = restore(&env, &dir.0, &id, at(11)).unwrap();
        assert_eq!(back.restored, ["ANTHROPIC_BASE_URL", "CODEX_HOME"]);
        assert_eq!(
            env.value("CODEX_HOME"),
            Some(Value::Expand("%USERPROFILE%\\.codex2".into())),
            "REG_EXPAND_SZ 写回去还得是 REG_EXPAND_SZ"
        );
        assert_eq!(
            env.value("ANTHROPIC_BASE_URL"),
            Some(Value::Text("https://relay.example/v1".into()))
        );
        assert!(backups(&dir.0).unwrap()[0].restored_at.is_some());

        // 再还原一次：都已经在了，什么都不写。
        let again = restore(&env, &dir.0, &id, at(12)).unwrap();
        assert!(again.restored.is_empty());
        assert_eq!(again.already.len(), 2);
    }

    #[test]
    fn restore_never_overwrites_a_different_value_set_since() {
        let dir = TempDir::new();
        let env = FakeEnv::with(&[("ANTHROPIC_MODEL", Value::Text("old".into()))]);
        let r = remove(&env, &dir.0, &["ANTHROPIC_MODEL".into()], at(10)).unwrap();
        env.set("ANTHROPIC_MODEL", &Value::Text("new".into()))
            .unwrap();
        let back = restore(&env, &dir.0, r.backup_id.as_deref().unwrap(), at(11)).unwrap();
        assert_eq!(back.conflicts, ["ANTHROPIC_MODEL"]);
        assert_eq!(
            env.value("ANTHROPIC_MODEL"),
            Some(Value::Text("new".into()))
        );
    }

    #[test]
    fn an_unrecognised_name_stops_everything_before_anything_is_touched() {
        let dir = TempDir::new();
        let env = FakeEnv::with(&[
            ("ANTHROPIC_MODEL", Value::Text("x".into())),
            ("Path", Value::Text("C:\\x".into())),
        ]);
        let err = remove(
            &env,
            &dir.0,
            &["ANTHROPIC_MODEL".into(), "Path".into()],
            at(10),
        )
        .unwrap_err();
        assert!(err.to_string().contains("Path"));
        assert!(env.value("ANTHROPIC_MODEL").is_some(), "一个都不删");
        assert!(backups(&dir.0).unwrap().is_empty(), "也不留备份");
    }

    #[test]
    fn nothing_present_means_no_backup_and_no_broadcast() {
        let dir = TempDir::new();
        let env = FakeEnv::default();
        let r = remove(&env, &dir.0, &["OPENAI_API_KEY".into()], at(10)).unwrap();
        assert!(r.backup_id.is_none());
        assert_eq!(r.missing, ["OPENAI_API_KEY"]);
        assert_eq!(*env.notified.borrow(), 0);
    }

    #[test]
    fn a_failed_delete_is_reported_and_the_backup_is_still_there() {
        let dir = TempDir::new();
        let mut env = FakeEnv::with(&[("XAI_API_KEY", Value::Text("xai-1".into()))]);
        env.broken = Some("remove");
        let r = remove(&env, &dir.0, &["XAI_API_KEY".into()], at(10)).unwrap();
        assert!(r.removed.is_empty());
        assert_eq!(r.failed.len(), 1);
        assert!(r.failed[0].contains("拒绝访问"));
        assert!(r.backup_id.is_some(), "备份先写，删失败了它也在");
    }

    #[test]
    fn backup_ids_from_the_interface_cannot_walk_out_of_the_folder() {
        let dir = TempDir::new();
        for bad in ["..\\settings", "../x", "a/b", "", "a.json"] {
            assert!(backup_path(&dir.0, bad).is_err(), "{bad}");
            assert!(delete_backup(&dir.0, bad).is_err());
        }
        assert!(backup_path(&dir.0, "env-20261027-100000-2").is_ok());
    }

    #[test]
    fn two_backups_in_the_same_second_get_different_ids_and_list_newest_first() {
        let dir = TempDir::new();
        let env = FakeEnv::with(&[
            ("GEMINI_API_KEY", Value::Text("a".into())),
            ("GROK_DEFAULT_MODEL", Value::Text("b".into())),
        ]);
        let a = remove(&env, &dir.0, &["GEMINI_API_KEY".into()], at(10)).unwrap();
        let b = remove(&env, &dir.0, &["GROK_DEFAULT_MODEL".into()], at(10)).unwrap();
        assert_ne!(a.backup_id, b.backup_id);
        let list = backups(&dir.0).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(
            Some(list[0].id.clone()),
            b.backup_id,
            "同一秒的，后来的在前"
        );
        delete_backup(&dir.0, list[0].id.as_str()).unwrap();
        assert_eq!(backups(&dir.0).unwrap().len(), 1);
    }
}
