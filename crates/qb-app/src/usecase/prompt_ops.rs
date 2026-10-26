// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 提示词库（中转站重构阶段 5，cc-switch 的 M3）：一库提示词，每个目标挑一条写进它的提示词文件。
//!
//! cc-switch 一个客户端一个文件（`CLAUDE.md`、`AGENTS.md`、`GEMINI.md`、Hermes 的 `SOUL.md`），启用一条 = 整份写进去、同一客户端互斥。
//! QB Gate 的 Claude Code / Codex / Gemini CLI 有好几个目录，**写进哪一份由使用者选**（计划 §7 阶段 5）：
//! 目标同 MCP / Skills（`ext_place`），每个目标各自挑一条（记在 `metadata` 的 `prompt-active:<目标>`）。
//!
//! 守住的几条（照 cc-switch 的「启用前先回填」）：
//! - ⛔ **写之前先把文件现在的内容存回库里**：这个目标已经有启用的那一条 —— 文件被手动改过就回填进那一条；没有 —— 文件里有内容、
//!   库里又没有一模一样的，就存成一条「原始提示词」备份。所以换来换去，使用者原来写的东西不会丢；
//! - 改一条正在用的提示词，用着它的那几个目标跟着重写；文件在这期间被手动改过的，改动先另存一条再覆盖；
//! - 正在用的不能删（先在那个目标换成别的）；官方目录要使用者自己挑；目录不在（没装）不建；
//! - 写盘走 `config_io::commit`（核对没被外部改过、留原内容、失败回滚）。

use super::ext_place::Place;
use crate::error::{GateError, Result};
use crate::repository::Repository;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

const TABLE: &str = "prompts";
/// MiniMax Code 的全局指令上限（cc-switch 的 `validate_prompt_content`）。
const MINIMAX_LIMIT: usize = 32 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Prompt {
    /// 新建时留空，后端编。
    pub id: String,
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub description: Option<String>,
    #[ts(type = "number")]
    #[serde(default)]
    pub created_ms: i64,
    #[ts(type = "number")]
    #[serde(default)]
    pub updated_ms: i64,
}

/// 一个能写提示词的地方。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PromptTarget {
    pub id: String,
    pub label: String,
    /// 那个提示词文件。
    pub path: String,
    pub official: bool,
    pub available: bool,
    /// 这里现在用的是哪一条（面板写的）。
    pub active: Option<String>,
    /// 文件在不在、有多长（字符）。
    #[ts(type = "number | null")]
    pub chars: Option<usize>,
    /// 文件跟用着的那一条对不上（被手动改过）。下次换的时候会先回填。
    pub drift: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PromptsView {
    pub prompts: Vec<Prompt>,
    pub targets: Vec<PromptTarget>,
}

#[derive(Debug, Clone)]
struct Target {
    id: String,
    label: String,
    path: PathBuf,
    official: bool,
    available: bool,
    limit: Option<usize>,
}

fn targets(place: &Place) -> Vec<Target> {
    use crate::relay::coexist::CoexistTool;
    let mut out: Vec<Target> = place
        .client_slots()
        .into_iter()
        .map(|s| {
            let file = if s.id.starts_with("claude-code:") {
                "CLAUDE.md"
            } else if s.id.starts_with("codex:") {
                "AGENTS.md"
            } else {
                "GEMINI.md"
            };
            Target {
                path: s.root.join(file),
                id: s.id,
                label: s.label,
                official: s.official,
                available: s.available,
                limit: None,
            }
        })
        .collect();
    let h = &place.home;
    let mut tool = |id: &str, label: &str, root: PathBuf, file: &str, limit: Option<usize>| {
        out.push(Target {
            id: id.into(),
            label: label.into(),
            path: root.join(file),
            official: false,
            available: root.is_dir(),
            limit,
        })
    };
    tool(
        "grok-build",
        "Grok Build",
        h.join(".grok"),
        "AGENTS.md",
        None,
    );
    tool(
        "opencode",
        "OpenCode",
        place.coexist_dir(CoexistTool::OpenCode),
        "AGENTS.md",
        None,
    );
    tool(
        "minimax-code",
        "MiniMax Code",
        place.coexist_dir(CoexistTool::MiniMaxCode),
        "AGENTS.md",
        Some(MINIMAX_LIMIT),
    );
    tool(
        "openclaw",
        "OpenClaw",
        place.coexist_dir(CoexistTool::OpenClaw),
        "AGENTS.md",
        None,
    );
    tool(
        "hermes",
        "Hermes",
        place.coexist_dir(CoexistTool::Hermes),
        "SOUL.md",
        None,
    );
    tool("pi", "Pi", h.join(".pi").join("agent"), "AGENTS.md", None);
    out
}

fn target(place: &Place, id: &str) -> Result<Target> {
    targets(place)
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| GateError::Other(format!("不认得这个目标：{id}")))
}

fn active_key(target: &str) -> String {
    format!("prompt-active:{target}")
}

pub(crate) fn active(db: &Repository, target: &str) -> Option<String> {
    db.meta(&active_key(target))
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn read(path: &std::path::Path) -> Result<(Option<Vec<u8>>, Option<String>)> {
    let bytes = crate::config_io::read_optional(path)?;
    let text = bytes
        .as_ref()
        .map(|b| String::from_utf8_lossy(b).into_owned());
    Ok((bytes, text))
}

fn prompts(db: &Repository) -> Result<Vec<Prompt>> {
    let mut out: Vec<Prompt> = db.list(TABLE)?;
    out.sort_by_key(|p| std::cmp::Reverse(p.updated_ms));
    Ok(out)
}

fn view(place: &Place, db: &Repository) -> Result<PromptsView> {
    let all = prompts(db)?;
    let targets = targets(place)
        .into_iter()
        .map(|t| {
            let live = read(&t.path).ok().and_then(|(_, s)| s);
            let active = active(db, &t.id);
            let drift = match (&active, &live) {
                (Some(id), Some(text)) => all
                    .iter()
                    .find(|p| &p.id == id)
                    .is_some_and(|p| p.content.trim() != text.trim()),
                (Some(_), None) => true,
                _ => false,
            };
            PromptTarget {
                id: t.id,
                label: t.label,
                path: t.path.display().to_string(),
                official: t.official,
                available: t.available,
                active,
                chars: live.map(|s| s.chars().count()),
                drift,
            }
        })
        .collect();
    Ok(PromptsView {
        prompts: all,
        targets,
    })
}

pub fn list(db: &Repository) -> Result<PromptsView> {
    view(&Place::real(db)?, db)
}

/// 文件现在的内容存回库里（cc-switch 的「启用前回填」）：这个目标有用着的那一条就回填进去，没有就另存一条「原始提示词」。
fn backfill(db: &Repository, t: &Target, live: &str) -> Result<()> {
    if live.trim().is_empty() {
        return Ok(());
    }
    let all = prompts(db)?;
    if let Some(mut p) = active(db, &t.id).and_then(|id| all.iter().find(|p| p.id == id).cloned()) {
        if p.content.trim() != live.trim() {
            p.content = live.to_string();
            p.updated_ms = now_ms();
            db.put(TABLE, &p.id, &p)?;
        }
        return Ok(());
    }
    if all.iter().any(|p| p.content.trim() == live.trim()) {
        return Ok(());
    }
    let p = Prompt {
        id: format!("backup-{}", crate::config_io::id()),
        name: format!(
            "原始提示词 · {} · {}",
            t.label,
            chrono::Local::now().format("%Y-%m-%d %H:%M")
        ),
        content: live.to_string(),
        description: Some(format!(
            "面板第一次写 {} 之前，那份文件原来的内容",
            t.path.display()
        )),
        created_ms: now_ms(),
        updated_ms: now_ms(),
    };
    db.put(TABLE, &p.id, &p)
}

fn write(place: &Place, t: &Target, content: &str) -> Result<()> {
    if let Some(limit) = t.limit {
        if content.len() > limit {
            return Err(GateError::Other(format!(
                "{} 的全局指令最多 {} KiB",
                t.label,
                limit / 1024
            )));
        }
    }
    let (old, _) = read(&t.path)?;
    if let Some(dir) = t.path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    place.commit(vec![crate::config_io::Edit {
        path: t.path.clone(),
        expected: old,
        body: Some(content.as_bytes().to_vec()),
    }])
}

/// 在一个目标用上这一条：先回填文件现在的内容，再整份写进去。
pub fn enable(db: &Repository, target: &str, id: &str) -> Result<PromptsView> {
    let place = Place::real(db)?;
    enable_at(&place, db, target, id)?;
    crate::audit::write(&format!("提示词：{target} 用上了 {id}"));
    view(&place, db)
}

pub(crate) fn enable_at(place: &Place, db: &Repository, tid: &str, id: &str) -> Result<()> {
    let t = target(place, tid)?;
    if !t.available {
        return Err(GateError::Other(format!(
            "{} 没找到（{}）：多半还没装，不替它建",
            t.label,
            t.path.display()
        )));
    }
    let p: Prompt = db
        .find(TABLE, id)?
        .ok_or_else(|| GateError::Other(format!("没有这条提示词：{id}")))?;
    if let (_, Some(live)) = read(&t.path)? {
        backfill(db, &t, &live)?;
    }
    // 回填可能刚改了这一条（它本来就是用着的那条、文件被手动改过）：重新读一遍。
    let p: Prompt = db.find(TABLE, id)?.unwrap_or(p);
    write(place, &t, &p.content)?;
    db.set_meta(&active_key(tid), id)
}

/// 新建或修改。正在用的那几个目标跟着重写（文件被手动改过的，改动先另存一条）。
pub fn save(db: &Repository, prompt: Prompt) -> Result<PromptsView> {
    let place = Place::real(db)?;
    save_at(&place, db, prompt)?;
    view(&place, db)
}

fn save_at(place: &Place, db: &Repository, mut p: Prompt) -> Result<String> {
    if p.name.trim().is_empty() {
        return Err(GateError::Other("给提示词起个名字".into()));
    }
    let old: Option<Prompt> = if p.id.is_empty() {
        None
    } else {
        db.find(TABLE, &p.id)?
    };
    if p.id.is_empty() {
        p.id = format!("prompt-{}", crate::config_io::id());
    }
    p.created_ms = old.as_ref().map(|o| o.created_ms).unwrap_or_else(now_ms);
    p.updated_ms = now_ms();
    db.put(TABLE, &p.id, &p)?;
    for t in targets(place) {
        if active(db, &t.id).as_deref() != Some(p.id.as_str()) || !t.available {
            continue;
        }
        if let (Some(old), (_, Some(live))) = (&old, read(&t.path)?) {
            if live.trim() != old.content.trim() && !live.trim().is_empty() {
                let keep = Prompt {
                    id: format!("backup-{}", crate::config_io::id()),
                    name: format!(
                        "手动改过的 · {} · {}",
                        t.label,
                        chrono::Local::now().format("%Y-%m-%d %H:%M")
                    ),
                    content: live,
                    description: Some(format!(
                        "{} 被手动改过，面板覆盖之前另存的",
                        t.path.display()
                    )),
                    created_ms: now_ms(),
                    updated_ms: now_ms(),
                };
                db.put(TABLE, &keep.id, &keep)?;
            }
        }
        write(place, &t, &p.content)?;
    }
    Ok(p.id)
}

/// 删掉一条。正在哪个目标用着就不让删。
pub fn delete(db: &Repository, id: &str) -> Result<PromptsView> {
    let place = Place::real(db)?;
    delete_at(&place, db, id)?;
    view(&place, db)
}

fn delete_at(place: &Place, db: &Repository, id: &str) -> Result<()> {
    if let Some(t) = targets(place)
        .into_iter()
        .find(|t| active(db, &t.id).as_deref() == Some(id))
    {
        return Err(GateError::Other(format!(
            "{} 正在用这一条：先在那里换成别的再删",
            t.label
        )));
    }
    db.remove(TABLE, id)
}

/// 把一个目标里现在的文件导成一条提示词（不写文件）。
pub fn import(db: &Repository, target: &str) -> Result<PromptsView> {
    let place = Place::real(db)?;
    import_at(&place, db, target)?;
    view(&place, db)
}

fn import_at(place: &Place, db: &Repository, tid: &str) -> Result<()> {
    let t = target(place, tid)?;
    let (_, live) = read(&t.path)?;
    let live = live
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| GateError::Other(format!("{} 里还没有提示词文件", t.label)))?;
    if prompts(db)?.iter().any(|p| p.content.trim() == live.trim()) {
        return Err(GateError::Other("库里已经有一模一样的一条".into()));
    }
    save_at(
        place,
        db,
        Prompt {
            id: String::new(),
            name: format!("从 {} 导入", t.label),
            content: live,
            description: Some(format!("导入自 {}", t.path.display())),
            created_ms: 0,
            updated_ms: 0,
        },
    )
    .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(name: &str, content: &str) -> Prompt {
        Prompt {
            id: String::new(),
            name: name.into(),
            content: content.into(),
            description: None,
            created_ms: 0,
            updated_ms: 0,
        }
    }

    #[test]
    fn switching_prompts_never_loses_what_the_file_had() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let p = Place::under(&root);
        let file = root.join("env-claude").join("CLAUDE.md");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "我原来写的规矩\n").unwrap();

        let a = save_at(&p, &db, prompt("A", "规矩 A")).unwrap();
        let b = save_at(&p, &db, prompt("B", "规矩 B")).unwrap();
        enable_at(&p, &db, "claude-code:relay", &a).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "规矩 A");
        let all = prompts(&db).unwrap();
        assert!(
            all.iter()
                .any(|x| x.content == "我原来写的规矩\n" && x.id.starts_with("backup-")),
            "原来的另存了"
        );

        // 手动改了文件再换：改动回填进 A。
        std::fs::write(&file, "规矩 A（我手动加了一句）").unwrap();
        enable_at(&p, &db, "claude-code:relay", &b).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "规矩 B");
        let a_now: Prompt = db.find(TABLE, &a).unwrap().unwrap();
        assert_eq!(a_now.content, "规矩 A（我手动加了一句）");

        // 改正在用的 B：文件跟着变；正在用的不能删。
        let mut b_new: Prompt = db.find(TABLE, &b).unwrap().unwrap();
        b_new.content = "规矩 B v2".into();
        save_at(&p, &db, b_new).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "规矩 B v2");
        assert!(delete_at(&p, &db, &b)
            .unwrap_err()
            .to_string()
            .contains("正在用"));
        delete_at(&p, &db, &a).unwrap();

        // 官方默认目录没装：不写、不建。
        assert!(enable_at(&p, &db, "claude-code:default", &b).is_err());
        assert!(!root.join("home").join(".claude").exists());

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn importing_reads_the_file_without_touching_it_and_minimax_has_a_limit() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let p = Place::under(&root);
        let soul = root.join("home").join(".hermes").join("SOUL.md");
        std::fs::create_dir_all(soul.parent().unwrap()).unwrap();
        std::fs::write(&soul, "你是一只猫").unwrap();
        import_at(&p, &db, "hermes").unwrap();
        assert_eq!(prompts(&db).unwrap()[0].content, "你是一只猫");
        assert!(import_at(&p, &db, "hermes").is_err(), "一模一样的不重复导");
        assert_eq!(std::fs::read_to_string(&soul).unwrap(), "你是一只猫");

        std::fs::create_dir_all(root.join("home").join(".minimax")).unwrap();
        let big = save_at(&p, &db, prompt("big", &"字".repeat(11_000))).unwrap();
        assert!(
            enable_at(&p, &db, "minimax-code", &big).is_err(),
            "MiniMax Code 最多 32 KiB"
        );

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
