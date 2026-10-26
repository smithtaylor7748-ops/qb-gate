//! MCP / Skills / 提示词共用的「地方」（中转站重构阶段 5）：使用者目录、三个软件的中转环境目录、官方槽位、写盘日志放哪。
//!
//! 三个面板都按**目标**写（中转环境 / 官方默认目录 / 官方槽位 / 各个工具），目标落到哪个文件或目录各管各的，
//! 但「那几个根目录在哪」只有这一处。单测换成临时目录（[`Place::under`]），**单测不碰真实状态**。

use crate::domain::Client;
use crate::error::{GateError, Result};
use crate::repository::Repository;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) struct Place {
    pub home: PathBuf,
    /// 三个软件的中转环境目录（`claude-code` / `codex` / `gemini-cli`）。
    pub relay: BTreeMap<&'static str, PathBuf>,
    /// 官方 Claude Code 槽位：(标签, `CLAUDE_CONFIG_DIR`)。
    pub claude_slots: Vec<(String, PathBuf)>,
    /// 官方 Codex 槽位：(槽位 id, 标签, `CODEX_HOME`)。
    pub codex_slots: Vec<(String, String, PathBuf)>,
    /// 读环境变量（MiniMax Code 的数据目录）。
    pub env: fn(&str) -> Option<String>,
    /// 面板自己的数据目录（Skills 的存放处、备份）。
    pub state: PathBuf,
    /// 写盘日志放哪；`None` = `config_io::commit` 自己的。
    pub journal: Option<PathBuf>,
    pub windows: bool,
    /// 写完中转环境里有配置清单的文件后要不要重新应用（单测里不要）。
    pub reapply: bool,
}

impl Place {
    pub fn real(db: &Repository) -> Result<Self> {
        let relay_dir = |c: Client| {
            crate::config_io::environment_dir(&db.root, &crate::workspace::router_environment_id(c))
        };
        let roots = crate::accounts::AccountRoots::current();
        let claude_slots = crate::accounts::slots_in(&roots)
            .into_iter()
            .map(|s| {
                let dir = roots.slot_dir(&s.label);
                (s.label, dir)
            })
            .collect();
        let codex_root = qb_accounts::codex::root();
        let codex_slots = qb_accounts::codex::list(&codex_root)
            .map(|l| l.slots)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|s| {
                let dir = qb_accounts::codex::directory(&codex_root, &s.id).ok()?;
                Some((s.id, s.label, crate::workspace::codex_slot_dirs(&dir).0))
            })
            .collect();
        Ok(Place {
            home: dirs::home_dir().ok_or_else(|| GateError::Other("找不到用户目录".into()))?,
            relay: BTreeMap::from([
                ("claude-code", relay_dir(Client::ClaudeCode)?),
                ("codex", relay_dir(Client::Codex)?),
                ("gemini-cli", relay_dir(Client::GeminiCli)?),
            ]),
            claude_slots,
            codex_slots,
            env: |k| std::env::var(k).ok(),
            state: crate::paths::state_dir(),
            journal: None,
            windows: cfg!(windows),
            reapply: true,
        })
    }

    /// 单测用：一切都在 `root` 下面。
    #[cfg(test)]
    pub fn under(root: &Path) -> Self {
        Place {
            home: root.join("home"),
            relay: BTreeMap::from([
                ("claude-code", root.join("env-claude")),
                ("codex", root.join("env-codex")),
                ("gemini-cli", root.join("env-gemini")),
            ]),
            claude_slots: vec![("a".into(), root.join("slot-a"))],
            codex_slots: Vec::new(),
            env: |_| None,
            state: root.join("state"),
            journal: Some(root.join("journal")),
            windows: false,
            reapply: false,
        }
    }

    pub fn commit(&self, edits: Vec<crate::config_io::Edit>) -> Result<()> {
        if edits.is_empty() {
            return Ok(());
        }
        match &self.journal {
            Some(dir) => crate::config_io::commit_in(dir, edits),
            None => crate::config_io::commit(edits),
        }
        .map(drop)
    }

    /// 写完中转环境里有配置清单的那几份之后，按那个软件的中转环境重新应用一次（指纹对上）。没建过的环境不管。
    pub fn reapply(&self, clients: &[Client]) -> Result<()> {
        if !self.reapply {
            return Ok(());
        }
        let db = Repository::open()?;
        for &c in clients {
            let id = crate::workspace::router_environment_id(c);
            if db
                .get::<crate::domain::Environment>("environments", &id)
                .is_ok()
            {
                crate::workspace::apply(&id)?;
            }
        }
        Ok(())
    }

    /// 某个共存型工具的配置目录（MiniMax Code 认环境变量）。
    pub fn coexist_dir(&self, tool: crate::relay::coexist::CoexistTool) -> PathBuf {
        tool.config_dir(&self.home, self.env)
    }

    /// 某个共存型工具的配置文件（候选里先有的那个；都没有用第一个）。
    pub fn coexist_file(&self, tool: crate::relay::coexist::CoexistTool) -> PathBuf {
        let candidates = tool.config_candidates(&self.home, self.env);
        candidates
            .iter()
            .find(|p| p.is_file())
            .cloned()
            .unwrap_or_else(|| candidates[0].clone())
    }
}

/// 一个目标的身份（三个面板共用的那一半）：id、显示名、是不是官方那一侧、对应哪个中转环境。
#[derive(Debug, Clone)]
pub(crate) struct Slot {
    pub id: String,
    pub label: String,
    pub official: bool,
    /// 这个目标的根目录（Claude Code 的 `CLAUDE_CONFIG_DIR`、Codex 的 `CODEX_HOME`、Gemini 的 `.gemini`……）。
    pub root: PathBuf,
    /// 根目录在不在（不在多半是没装）。
    pub available: bool,
    /// 中转环境那几个：写完要按它重新应用。
    pub relay: Option<Client>,
}

impl Place {
    /// 三个软件（Claude Code、Codex、Gemini CLI）的目标：中转环境、官方默认目录、官方槽位。
    pub fn client_slots(&self) -> Vec<Slot> {
        let h = &self.home;
        let s = |id: String,
                 label: String,
                 official: bool,
                 root: PathBuf,
                 available: bool,
                 relay: Option<Client>| Slot {
            id,
            label,
            official,
            root,
            available,
            relay,
        };
        let mut out = vec![
            s(
                "claude-code:relay".into(),
                "Claude Code · 中转环境".into(),
                false,
                self.relay["claude-code"].clone(),
                true,
                Some(Client::ClaudeCode),
            ),
            s(
                "claude-code:default".into(),
                "Claude Code · 官方默认目录".into(),
                true,
                h.join(".claude"),
                h.join(".claude").is_dir(),
                None,
            ),
        ];
        for (label, dir) in &self.claude_slots {
            out.push(s(
                format!("claude-code:slot:{label}"),
                format!("Claude Code · 官方槽位 {label}"),
                true,
                dir.clone(),
                dir.is_dir(),
                None,
            ));
        }
        out.push(s(
            "codex:relay".into(),
            "Codex · 中转环境".into(),
            false,
            self.relay["codex"].clone(),
            true,
            Some(Client::Codex),
        ));
        out.push(s(
            "codex:default".into(),
            "Codex · 官方默认目录".into(),
            true,
            h.join(".codex"),
            h.join(".codex").is_dir(),
            None,
        ));
        for (id, label, home) in &self.codex_slots {
            out.push(s(
                format!("codex:slot:{id}"),
                format!("Codex · 官方槽位 {label}"),
                true,
                home.clone(),
                home.is_dir(),
                None,
            ));
        }
        out.push(s(
            "gemini-cli:relay".into(),
            "Gemini CLI · 中转环境".into(),
            false,
            self.relay["gemini-cli"].join(".gemini"),
            true,
            Some(Client::GeminiCli),
        ));
        out.push(s(
            "gemini-cli:default".into(),
            "Gemini CLI · 官方默认目录".into(),
            true,
            h.join(".gemini"),
            h.join(".gemini").is_dir(),
            None,
        ));
        out
    }
}

/// 某个目录里是不是有东西指向别处（联结点 / 符号链接）。写 Skill 之前看一眼，不顺着链接写到别的地方去。
pub(crate) fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if m.file_attributes() & 0x400 != 0 {
                return true;
            }
        }
        m.file_type().is_symlink()
    })
}
