// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 共存型工具自己的说明 / 记忆文件（中转站重构阶段 5，C3 / C4 的附带件，照 cc-switch）：
//! OpenClaw 的工作区文件和每日记忆、Hermes 的两份记忆（加 `config.yaml` 里的开关和字数上限）、
//! Pi 的系统提示词文件和斜杠命令模板。
//!
//! 都是那个工具自己目录里的 Markdown，面板只是给它们一个编辑器 —— 不碰中转、不碰官方账户。
//! 守住的几条：
//! - **文件名一律白名单 / 固定形状**（照 cc-switch）：OpenClaw 九份工作区文件、`workspace/memory/YYYY-MM-DD.md`；
//!   Hermes 两份；Pi 两份 + `prompts/<名字>.md`（名字照 cc-switch 的 `validate_template_slug`）。别的路径一概不认；
//! - **工具没装（目录不在）就不写**，跟共存那一块一样不替它建目录（cc-switch 会建）；装了，子目录按需建；
//! - **写回前核对这期间没被别处改过**：版本 = 内容的 SHA-256（照 cc-switch 的 Pi 那一套，三家统一用）；
//!   写盘走 `config_io::commit`（留原内容、失败回滚）；单份最多 1 MiB；
//! - ⛔ **删除先进备份**（cc-switch 直接删）：挪进面板数据目录的 `tool-file-backups\<工具>\`；
//! - Pi 的 `SYSTEM.md` / `APPEND_SYSTEM.md` 不许存空的（照 cc-switch：要停用就删掉那份文件）。

use super::ext_place::Place;
use crate::error::{GateError, Result};
use crate::relay::coexist::CoexistTool;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// 单份文件最多多大（cc-switch 的 `MAX_PROMPT_FILE_BYTES`）。
const MAX_BYTES: usize = 1024 * 1024;
/// 不存在的文件的「版本」。
pub const MISSING: &str = "missing";

/// OpenClaw 的工作区文件（cc-switch 的 `ALLOWED_FILES`，说明取自它的中文文案）。
const OPENCLAW_FILES: [(&str, &str); 9] = [
    ("AGENTS.md", "Agent 操作指令和规则"),
    ("SOUL.md", "Agent 人格和沟通风格"),
    ("USER.md", "用户档案和偏好"),
    ("IDENTITY.md", "Agent 名称和头像"),
    ("TOOLS.md", "本地工具文档"),
    ("MEMORY.md", "长期记忆和决策记录"),
    ("HEARTBEAT.md", "心跳运行清单"),
    ("BOOTSTRAP.md", "首次运行仪式"),
    ("BOOT.md", "网关重启清单"),
];
const HERMES_FILES: [(&str, &str); 2] = [
    (
        "memories/MEMORY.md",
        "Agent 自己的笔记（会放进它的系统提示词）",
    ),
    ("memories/USER.md", "关于你的档案（同上）"),
];
const PI_FILES: [(&str, &str); 2] = [
    ("SYSTEM.md", "整份替换 Pi 的系统提示词"),
    ("APPEND_SYSTEM.md", "接在 Pi 的系统提示词后面"),
];

/// 一份文件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolFile {
    /// 在这个工具目录里的相对路径（`workspace/SOUL.md`、`memories/USER.md`、`prompts/review.md`……），读写都按它。
    pub name: String,
    /// 固定的那几份才有：一句话说明。
    pub label: Option<String>,
    pub exists: bool,
    pub content: String,
    /// 内容的 SHA-256（不存在是 `missing`）：写回时核对这期间没被别处改过。
    pub revision: String,
    #[ts(type = "number")]
    pub size: u64,
    #[ts(type = "number")]
    pub modified_ms: i64,
}

/// Hermes 的两份记忆开不开、各多少字（`config.yaml` 的 `memory:` 小节；默认值照 Hermes 自己的）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HermesMemorySettings {
    pub memory_enabled: bool,
    pub user_enabled: bool,
    pub memory_limit: u32,
    pub user_limit: u32,
}

impl Default for HermesMemorySettings {
    fn default() -> Self {
        Self {
            memory_enabled: true,
            user_enabled: true,
            memory_limit: 2200,
            user_limit: 1375,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolFilesView {
    pub tool: CoexistTool,
    pub dir: String,
    /// 目录在不在（不在多半是没装：不写）。
    pub installed: bool,
    /// 固定的那几份。
    pub files: Vec<ToolFile>,
    /// 一组同形状的：OpenClaw 的每日记忆（新的在前）、Pi 的模板（按名字）。
    pub entries: Vec<ToolFile>,
    /// 只有 Hermes 有。
    pub hermes: Option<HermesMemorySettings>,
}

/// 每日记忆全文搜到的一条。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolFileHit {
    pub name: String,
    pub snippet: String,
    #[ts(type = "number")]
    pub matches: u32,
}

fn revision(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn supported(tool: CoexistTool) -> Result<()> {
    match tool {
        CoexistTool::OpenClaw | CoexistTool::Hermes | CoexistTool::Pi => Ok(()),
        _ => Err(GateError::Other(format!(
            "{} 没有要在这里编辑的说明 / 记忆文件",
            tool.label()
        ))),
    }
}

/// `workspace/memory/2026-10-26.md` 这种。
fn is_daily_memory(name: &str) -> bool {
    let Some(file) = name.strip_prefix("workspace/memory/") else {
        return false;
    };
    let Some(date) = file.strip_suffix(".md") else {
        return false;
    };
    let b = date.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// Pi 模板名（cc-switch 的 `validate_template_slug`：一个能当斜杠命令的词，Windows 上也能当文件名）。
fn pi_slug_ok(slug: &str) -> bool {
    let base = slug
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let reserved = matches!(base.as_str(), "con" | "prn" | "aux" | "nul")
        || ((base.starts_with("com") || base.starts_with("lpt"))
            && base.len() == 4
            && base.as_bytes()[3].is_ascii_digit()
            && base.as_bytes()[3] != b'0');
    !slug.is_empty()
        && slug.len() <= 128
        && !slug.starts_with('.')
        && !slug.ends_with('.')
        && !reserved
        && slug.chars().all(|c| {
            !c.is_control()
                && !c.is_whitespace()
                && !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        })
}

/// 这个名字在这个工具里认不认、是不是固定的那几份之一（回说明）。
fn check_name(tool: CoexistTool, name: &str) -> Result<Option<&'static str>> {
    let fixed = fixed_files(tool)
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, label)| *label);
    if fixed.is_some() {
        return Ok(fixed);
    }
    let entry_ok = match tool {
        CoexistTool::OpenClaw => is_daily_memory(name),
        CoexistTool::Pi => name
            .strip_prefix("prompts/")
            .and_then(|f| f.strip_suffix(".md"))
            .is_some_and(pi_slug_ok),
        _ => false,
    };
    if entry_ok {
        Ok(None)
    } else {
        Err(GateError::Other(format!(
            "{} 里没有「{name}」这份可以编辑的文件",
            tool.label()
        )))
    }
}

fn fixed_files(tool: CoexistTool) -> Vec<(String, &'static str)> {
    match tool {
        CoexistTool::OpenClaw => OPENCLAW_FILES
            .iter()
            .map(|(n, l)| (format!("workspace/{n}"), *l))
            .collect(),
        CoexistTool::Hermes => HERMES_FILES
            .iter()
            .map(|(n, l)| (n.to_string(), *l))
            .collect(),
        CoexistTool::Pi => PI_FILES.iter().map(|(n, l)| (n.to_string(), *l)).collect(),
        _ => Vec::new(),
    }
}

fn path_of(dir: &Path, name: &str) -> PathBuf {
    name.split('/')
        .fold(dir.to_path_buf(), |p, seg| p.join(seg))
}

fn load(dir: &Path, name: &str, label: Option<&str>) -> Result<ToolFile> {
    let path = path_of(dir, name);
    let bytes = crate::config_io::read_optional(&path)?;
    let meta = std::fs::metadata(&path).ok();
    Ok(ToolFile {
        name: name.to_string(),
        label: label.map(str::to_string),
        exists: bytes.is_some(),
        content: bytes
            .as_deref()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default(),
        revision: bytes
            .as_deref()
            .map(revision)
            .unwrap_or_else(|| MISSING.into()),
        size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
        modified_ms: meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
    })
}

/// 一个目录里 `.md` 的名字（不递归）。
fn md_names(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".md"))
                .collect()
        })
        .unwrap_or_default()
}

fn entries(tool: CoexistTool, dir: &Path) -> Result<Vec<ToolFile>> {
    let mut out = Vec::new();
    match tool {
        CoexistTool::OpenClaw => {
            let mut names: Vec<String> = md_names(&dir.join("workspace").join("memory"))
                .into_iter()
                .map(|n| format!("workspace/memory/{n}"))
                .filter(|n| is_daily_memory(n))
                .collect();
            names.sort_by(|a, b| b.cmp(a));
            for n in names {
                out.push(load(dir, &n, None)?);
            }
        }
        CoexistTool::Pi => {
            let mut names: Vec<String> = md_names(&dir.join("prompts"))
                .into_iter()
                .map(|n| format!("prompts/{n}"))
                .filter(|n| check_name(tool, n).is_ok())
                .collect();
            names.sort();
            for n in names {
                out.push(load(dir, &n, None)?);
            }
        }
        _ => {}
    }
    Ok(out)
}

fn hermes_settings(dir: &Path) -> Result<HermesMemorySettings> {
    let mut s = HermesMemorySettings::default();
    let raw = crate::config_io::read_optional(&dir.join("config.yaml"))?
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let Some(memory) = crate::relay_live::patch::yaml::section(&raw, "memory")? else {
        return Ok(s);
    };
    let num = |k: &str| memory.get(k).and_then(|v| v.as_u64()).map(|v| v as u32);
    let flag = |k: &str| memory.get(k).and_then(|v| v.as_bool());
    s.memory_limit = num("memory_char_limit").unwrap_or(s.memory_limit);
    s.user_limit = num("user_char_limit").unwrap_or(s.user_limit);
    s.memory_enabled = flag("memory_enabled").unwrap_or(s.memory_enabled);
    s.user_enabled = flag("user_profile_enabled").unwrap_or(s.user_enabled);
    Ok(s)
}

pub fn view(tool: CoexistTool) -> Result<ToolFilesView> {
    let db = crate::repository::Repository::open()?;
    view_at(&Place::real(&db)?, tool)
}

fn view_at(place: &Place, tool: CoexistTool) -> Result<ToolFilesView> {
    supported(tool)?;
    let dir = place.coexist_dir(tool);
    let installed = dir.is_dir();
    let files = fixed_files(tool)
        .iter()
        .map(|(n, l)| load(&dir, n, Some(l)))
        .collect::<Result<Vec<_>>>()?;
    Ok(ToolFilesView {
        tool,
        dir: dir.display().to_string(),
        installed,
        files,
        entries: entries(tool, &dir)?,
        hermes: if tool == CoexistTool::Hermes {
            Some(hermes_settings(&dir)?)
        } else {
            None
        },
    })
}

/// 改之前的那几道关：认这个名字、工具装着、这期间没被别处改过。回现在的内容。
fn guard(
    place: &Place,
    tool: CoexistTool,
    name: &str,
    expected: &str,
) -> Result<(PathBuf, Option<Vec<u8>>)> {
    supported(tool)?;
    check_name(tool, name)?;
    let dir = place.coexist_dir(tool);
    if !dir.is_dir() {
        return Err(GateError::Other(format!(
            "没找到 {}（{}）：多半没装，不替它建目录",
            tool.label(),
            dir.display()
        )));
    }
    let path = path_of(&dir, name);
    let now = crate::config_io::read_optional(&path)?;
    let current = now
        .as_deref()
        .map(revision)
        .unwrap_or_else(|| MISSING.into());
    if current != expected {
        return Err(GateError::Other(format!(
            "「{name}」在你编辑的时候被别处改过了：先刷新看看现在的内容，再改"
        )));
    }
    Ok((path, now))
}

/// 存一份（`revision` 是读的时候那个版本；新建传 `missing`）。
pub fn save(tool: CoexistTool, name: &str, content: &str, revision_seen: &str) -> Result<ToolFile> {
    let db = crate::repository::Repository::open()?;
    save_at(&Place::real(&db)?, tool, name, content, revision_seen)
}

fn save_at(
    place: &Place,
    tool: CoexistTool,
    name: &str,
    content: &str,
    seen: &str,
) -> Result<ToolFile> {
    if content.len() > MAX_BYTES {
        return Err(GateError::Other("单份最多 1 MiB".into()));
    }
    if tool == CoexistTool::Pi && !name.starts_with("prompts/") && content.trim().is_empty() {
        return Err(GateError::Other(
            "Pi 的系统提示词文件不能存成空的：要停用就删掉这份文件".into(),
        ));
    }
    let (path, now) = guard(place, tool, name, seen)?;
    place.commit(vec![crate::config_io::Edit {
        path,
        expected: now,
        body: Some(content.as_bytes().to_vec()),
    }])?;
    crate::audit::write(&format!("{}：改了 {name}", tool.label()));
    load(&place.coexist_dir(tool), name, check_name(tool, name)?)
}

/// 删一份：先挪进面板的 `tool-file-backups\<工具>\`，再从工具目录里拿掉。
pub fn delete(tool: CoexistTool, name: &str, revision_seen: &str) -> Result<ToolFilesView> {
    let db = crate::repository::Repository::open()?;
    let place = Place::real(&db)?;
    delete_at(&place, tool, name, revision_seen)?;
    view_at(&place, tool)
}

fn delete_at(place: &Place, tool: CoexistTool, name: &str, seen: &str) -> Result<()> {
    let (path, now) = guard(place, tool, name, seen)?;
    let Some(bytes) = now.clone() else {
        return Ok(());
    };
    let backup = place
        .state
        .join("tool-file-backups")
        .join(tool.label().replace(' ', "-"))
        .join(format!(
            "{}-{}",
            chrono::Local::now().format("%Y%m%d-%H%M%S"),
            name.replace('/', "_")
        ));
    if let Some(parent) = backup.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&backup, &bytes)?;
    place.commit(vec![crate::config_io::Edit {
        path,
        expected: now,
        body: None,
    }])?;
    crate::audit::write(&format!(
        "{}：删了 {name}（原内容在 {}）",
        tool.label(),
        backup.display()
    ));
    Ok(())
}

/// OpenClaw 每日记忆的全文搜索（不分大小写，日期也算）。片段取命中前后几十个字。
pub fn search(query: &str) -> Result<Vec<ToolFileHit>> {
    let db = crate::repository::Repository::open()?;
    search_at(&Place::real(&db)?, query)
}

fn search_at(place: &Place, query: &str) -> Result<Vec<ToolFileHit>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let dir = place.coexist_dir(CoexistTool::OpenClaw);
    let mut hits = Vec::new();
    for f in entries(CoexistTool::OpenClaw, &dir)? {
        let lower = f.content.to_lowercase();
        let matches = lower.matches(&q).count() as u32;
        if matches == 0 && !f.name.to_lowercase().contains(&q) {
            continue;
        }
        let chars: Vec<char> = f.content.chars().collect();
        let at = lower
            .find(&q)
            .map(|byte| lower[..byte].chars().count())
            .unwrap_or(0)
            .min(chars.len());
        let start = at.saturating_sub(40);
        let end = (at + 80).min(chars.len());
        let mut snippet: String = chars[start..end].iter().collect();
        if start > 0 {
            snippet.insert(0, '…');
        }
        if end < chars.len() {
            snippet.push('…');
        }
        hits.push(ToolFileHit {
            name: f.name,
            snippet,
            matches,
        });
    }
    Ok(hits)
}

/// Hermes 两份记忆的开关和字数上限：只改 `config.yaml` 里 `memory:` 小节的这四个键，别的键、别的小节、注释都不动。
pub fn hermes_save(s: &HermesMemorySettings) -> Result<HermesMemorySettings> {
    let db = crate::repository::Repository::open()?;
    hermes_save_at(&Place::real(&db)?, s)
}

fn hermes_save_at(place: &Place, s: &HermesMemorySettings) -> Result<HermesMemorySettings> {
    use serde_yaml_ng::Value as Y;
    let dir = place.coexist_dir(CoexistTool::Hermes);
    if !dir.is_dir() {
        return Err(GateError::Other(format!(
            "没找到 Hermes（{}）：多半没装，不替它建目录",
            dir.display()
        )));
    }
    let path = dir.join("config.yaml");
    let before = crate::config_io::read_optional(&path)?;
    let raw = before
        .as_deref()
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();
    let mut memory = match crate::relay_live::patch::yaml::section(&raw, "memory")? {
        Some(Y::Mapping(m)) => m,
        _ => Default::default(),
    };
    memory.insert("memory_enabled".into(), Y::Bool(s.memory_enabled));
    memory.insert("user_profile_enabled".into(), Y::Bool(s.user_enabled));
    memory.insert("memory_char_limit".into(), Y::Number(s.memory_limit.into()));
    memory.insert("user_char_limit".into(), Y::Number(s.user_limit.into()));
    let next =
        crate::relay_live::patch::yaml::replace_section(&raw, "memory", &Y::Mapping(memory))?;
    place.commit(vec![crate::config_io::Edit {
        path,
        expected: before,
        body: Some(next.into_bytes()),
    }])?;
    crate::audit::write("Hermes：改了记忆的开关 / 字数上限");
    hermes_settings(&dir)
}

/// Hermes 的网页面板开着没有（问一下本机 `127.0.0.1:<端口>/api/status`，回 200 或 401 都算开着 —— 它的接口要会话令牌）。
/// 端口照 cc-switch：`HERMES_WEB_PORT`，没设是 9119。开着回地址，界面拿去打开；没开回 `None`（使用者自己在终端里跑 `hermes dashboard`）。
pub async fn hermes_web() -> Result<Option<String>> {
    let port = std::env::var("HERMES_WEB_PORT")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .unwrap_or(9119);
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(1200))
        .no_proxy()
        .build()
        .map_err(|e| GateError::Other(e.to_string()))?;
    Ok(client
        .get(format!("{base}/api/status"))
        .send()
        .await
        .ok()
        .map(|_| format!("{base}/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PathBuf, Place) {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let place = Place::under(&root);
        (root, place)
    }

    #[test]
    fn only_whitelisted_files_and_only_when_the_tool_is_installed() {
        let (root, place) = fixture();
        // 没装：读得到「不在」，写不进去，也不建目录。
        let v = view_at(&place, CoexistTool::OpenClaw).unwrap();
        assert!(!v.installed);
        assert_eq!(v.files.len(), 9);
        assert!(save_at(
            &place,
            CoexistTool::OpenClaw,
            "workspace/SOUL.md",
            "x",
            MISSING
        )
        .is_err());
        assert!(!place.coexist_dir(CoexistTool::OpenClaw).exists());

        std::fs::create_dir_all(place.coexist_dir(CoexistTool::OpenClaw)).unwrap();
        let soul = save_at(
            &place,
            CoexistTool::OpenClaw,
            "workspace/SOUL.md",
            "温柔",
            MISSING,
        )
        .unwrap();
        assert!(soul.exists);
        assert_eq!(soul.label.as_deref(), Some("Agent 人格和沟通风格"));
        // 白名单外的、想往外跳的，一概不认。
        for bad in [
            "workspace/../secret.md",
            "workspace/evil.md",
            "openclaw.json",
            "workspace/memory/2026-13.md",
            "workspace/memory/../../x.md",
        ] {
            assert!(
                save_at(&place, CoexistTool::OpenClaw, bad, "x", MISSING).is_err(),
                "{bad}"
            );
        }
        // 不是编辑说明 / 记忆的工具：拒绝。
        assert!(view_at(&place, CoexistTool::OpenCode).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_stale_revision_is_refused_and_deleting_keeps_a_backup() {
        let (root, place) = fixture();
        std::fs::create_dir_all(place.coexist_dir(CoexistTool::OpenClaw)).unwrap();
        let name = "workspace/memory/2026-10-26.md";
        let a = save_at(
            &place,
            CoexistTool::OpenClaw,
            name,
            "今天聊了 Rust",
            MISSING,
        )
        .unwrap();
        // 别处改过：拿旧版本写回被拒。
        std::fs::write(
            path_of(&place.coexist_dir(CoexistTool::OpenClaw), name),
            "别处改的",
        )
        .unwrap();
        assert!(save_at(&place, CoexistTool::OpenClaw, name, "我的版本", &a.revision).is_err());
        let fresh = view_at(&place, CoexistTool::OpenClaw).unwrap();
        assert_eq!(fresh.entries[0].content, "别处改的");
        // 搜得到（不分大小写），片段里有命中的那段。
        std::fs::write(
            path_of(&place.coexist_dir(CoexistTool::OpenClaw), name),
            "Discussed RUST today",
        )
        .unwrap();
        let hits = search_at(&place, "rust").unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("RUST"));
        // 删：原内容进了备份，工具目录里没了。
        let now = view_at(&place, CoexistTool::OpenClaw).unwrap().entries[0]
            .revision
            .clone();
        delete_at(&place, CoexistTool::OpenClaw, name, &now).unwrap();
        assert!(view_at(&place, CoexistTool::OpenClaw)
            .unwrap()
            .entries
            .is_empty());
        let backups: Vec<_> =
            std::fs::read_dir(place.state.join("tool-file-backups").join("OpenClaw"))
                .unwrap()
                .flatten()
                .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            std::fs::read_to_string(backups[0].path()).unwrap(),
            "Discussed RUST today"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pi_templates_and_hermes_memory_settings() {
        let (root, place) = fixture();
        let pi = place.coexist_dir(CoexistTool::Pi);
        std::fs::create_dir_all(&pi).unwrap();
        assert!(
            save_at(&place, CoexistTool::Pi, "SYSTEM.md", "   ", MISSING).is_err(),
            "系统提示词不许存空的"
        );
        save_at(
            &place,
            CoexistTool::Pi,
            "prompts/review.md",
            "审一下这段代码",
            MISSING,
        )
        .unwrap();
        for bad in [
            "prompts/con.md",
            "prompts/a b.md",
            "prompts/.x.md",
            "prompts/x:y.md",
        ] {
            assert!(
                save_at(&place, CoexistTool::Pi, bad, "x", MISSING).is_err(),
                "{bad}"
            );
        }
        let v = view_at(&place, CoexistTool::Pi).unwrap();
        assert_eq!(v.entries.len(), 1);
        assert_eq!(v.entries[0].name, "prompts/review.md");

        let hermes = place.coexist_dir(CoexistTool::Hermes);
        std::fs::create_dir_all(&hermes).unwrap();
        std::fs::write(
            hermes.join("config.yaml"),
            "# 我的注释\nmodel:\n  default: x\nmemory:\n  provider: local\n  memory_char_limit: 3000\n",
        )
        .unwrap();
        let s = view_at(&place, CoexistTool::Hermes)
            .unwrap()
            .hermes
            .unwrap();
        assert_eq!(
            (s.memory_limit, s.user_limit, s.memory_enabled),
            (3000, 1375, true)
        );
        hermes_save_at(
            &place,
            &HermesMemorySettings {
                memory_enabled: false,
                ..s
            },
        )
        .unwrap();
        let text = std::fs::read_to_string(hermes.join("config.yaml")).unwrap();
        assert!(
            text.contains("# 我的注释") && text.contains("default: x"),
            "别的小节和注释不动：{text}"
        );
        assert!(
            text.contains("provider: local"),
            "memory 小节里别的键留着：{text}"
        );
        assert!(
            !view_at(&place, CoexistTool::Hermes)
                .unwrap()
                .hermes
                .unwrap()
                .memory_enabled
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
