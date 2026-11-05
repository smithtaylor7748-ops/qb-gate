// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 会话管理器（中转站重构阶段 6 · S1，cc-switch 的 `session_manager` 与 `providers/claude.rs` / `codex.rs`）：
//! Claude Code 和 Codex 留在本机的会话记录 —— 列表、搜索、阅读、导出 Markdown、复制恢复命令、删除（先进面板的回收站）。
//!
//! # 读哪些地方
//!
//! 跟 MCP / Skills 同一张「目标」表（[`super::ext_place::Place::client_slots`]）：每个软件的中转环境、官方默认目录、每个官方槽位。
//!
//! - Claude Code：`<CLAUDE_CONFIG_DIR>\projects\<项目>\<会话 id>.jsonl`；
//! - Codex：`<CODEX_HOME>\sessions\YYYY\MM\DD\rollout-*.jsonl` 和 `archived_sessions\*.jsonl`。
//!
//! Gemini CLI、OpenCode、Pi 这些没搬：cc-switch 给每家写了一个解析器，格式各变各的（claude 那一份就 106 KB）。
//!
//! # 守住的
//!
//! - **只读**：列表、阅读、搜索、导出都不改原文件。会话里有对话内容：只在本机处理、不进日志、不外传。
//! - **删除 = 挪进 `state_dir\session-trash`**（跟酒馆回收站同一套 `Trash`），能原样放回；**2 分钟内还在写的不让删**
//!   （多半正开着 —— 挪走正在写的文件，客户端那边接着写会写到哪去说不清）。
//! - **恢复命令只复制**，不替使用者开终端（CLAUDE.md「别把 shell 当启动目标」）。
//! - 面板给界面的会话 id 是 `<目标 id>|<相对路径>`。拿回来时核对：目标在当前这张表里、相对路径只在 `projects/`、
//!   `sessions/`、`archived_sessions/` 下面、没有 `..`、扩展名是 `.jsonl` —— 编一个 id 读不到别处的文件。

use super::ext_place::{Place, Slot};
use crate::accounts::tokens;
use crate::error::{GateError, Result};
use qb_extensions::plugins::tavern_library::{Trash, TrashEntry};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use ts_rs::TS;

/// 读开头找标题、项目目录时最多读多少。
const HEAD_MAX: u64 = 256 << 10;
/// 界面上一段最多显示多少字（多的截断）。导出 Markdown 时只截工具调用、工具输出和思考，使用者和助手说的话整段导出。
const TEXT_MAX: usize = 6_000;
/// 「在内容里搜」一次最多现读多少个文件、多少字节（缓存命中的不算）。
const CONTENT_LIMITS: Limits = Limits {
    files: 2_000,
    bytes: 512 << 20,
};
/// 这么久之内还在写的会话不让删。
const BUSY: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SessionSummary {
    /// 面板里的 id：`<目标 id>|<相对路径>`。
    pub id: String,
    /// `claude-code` / `codex`。
    pub client: String,
    /// 目标 id（`claude-code:slot:<标签>` 之类）。
    pub source: String,
    /// 「Claude Code · 官方槽位 xxx」。
    pub source_label: String,
    pub official: bool,
    /// 客户端自己的会话 id（恢复命令用它）。
    pub session_id: String,
    /// 第一条像样的提问（截短）；没有就是一句说明。
    pub title: String,
    /// 会话所在的项目目录。
    pub cwd: Option<String>,
    /// 第一条记录的时刻（RFC3339）。
    pub started_at: Option<String>,
    /// 文件最后写入的时刻（RFC3339）。
    pub updated_at: String,
    #[ts(type = "number")]
    pub size: u64,
    /// 文件的完整路径。
    pub path: String,
    /// `chat`（对话）/ `subagent`（子代理）。
    pub kind: String,
    /// 分组用的项目名：项目目录的最后一段；读不出目录时用 Claude Code 的项目文件夹名。
    pub project: String,
    /// 这个会话用掉的 token（从转写里的用量记录数；没有用量记录就是空）。
    pub tokens: Option<SessionTokens>,
    /// 两分钟内还在写（多半正开着）。
    pub running: bool,
    /// 官方默认目录里的、找不到归属依据的会话（只在按槽位列时出现）。
    pub unowned: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SessionTokens {
    /// 输入（Claude Code 含缓存读写；Codex 含缓存命中）。
    #[ts(type = "number")]
    pub input: u64,
    #[ts(type = "number")]
    pub output: u64,
    #[ts(type = "number")]
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SessionSourceCount {
    pub id: String,
    pub label: String,
    pub client: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SessionList {
    pub items: Vec<SessionSummary>,
    /// 符合条件的一共多少个（`items` 只是这一页）。
    pub total: u32,
    /// 每个目标各有多少个（不受搜索影响，给筛选那一排用）。
    pub sources: Vec<SessionSourceCount>,
    /// 内容搜索读到上限就停了：后面的没搜。
    pub search_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SessionMessage {
    /// `user` / `assistant` / `tool` / `system`。
    pub role: String,
    /// `text` / `tool_use` / `tool_result` / `thinking` / `summary`。
    pub kind: String,
    /// 工具名（`tool_use` / `tool_result`）。
    pub name: Option<String>,
    pub text: String,
    /// 这一段太长，后面截掉了。
    pub truncated: bool,
    pub at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SessionDetail {
    pub summary: SessionSummary,
    pub messages: Vec<SessionMessage>,
    /// 一共多少段（`messages` 只是这一页）。
    pub total: u32,
    pub offset: u32,
    /// PowerShell 里跑的恢复命令。
    pub resume: String,
    /// 跑之前要知道的那句话（门禁、本机路由、要装命令行）。
    pub resume_note: String,
}

// ------------------------------------------------------------------ 来源与文件

/// 会话记录在哪几个目标里（Claude Code、Codex 两家）。
fn sources(place: &Place) -> Vec<Slot> {
    place
        .client_slots()
        .into_iter()
        .filter(|s| s.id.starts_with("claude-code:") || s.id.starts_with("codex:"))
        .filter(|s| s.root.is_dir())
        .collect()
}

fn client_of(source: &str) -> &'static str {
    if source.starts_with("codex:") {
        "codex"
    } else {
        "claude-code"
    }
}

fn jsonl_in(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("jsonl"))
            && e.file_type().is_ok_and(|t| t.is_file())
        {
            out.push(p);
        }
    }
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

/// 一个会话文件，外加 Claude Code 从路径上就能看出来的那几样。
#[derive(Debug, Clone, Default)]
struct SessionFile {
    path: PathBuf,
    /// Claude Code：会话 id（子代理转写 = 父会话 id）。
    session: Option<String>,
    /// Claude Code：项目文件夹名（官方按 cwd 编码出来的那一段）。
    project: Option<String>,
    /// Claude Code：`<会话>\subagents\` 下面的子代理转写。
    subagent: bool,
}

fn under_subagents(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "subagents")
}

/// 一个目标里的全部会话文件。不跟联结点 / 符号链接走（`file_type` 不追链接）。
/// Claude Code 连子代理转写（`<会话>\subagents\*.jsonl`、`workflows\<wf>\*.jsonl`）一起列。
fn session_files(source: &Slot) -> Vec<SessionFile> {
    if client_of(&source.id) == "codex" {
        let mut out = Vec::new();
        for y in subdirs(&source.root.join("sessions")) {
            for m in subdirs(&y) {
                for d in subdirs(&m) {
                    jsonl_in(&d, &mut out);
                }
            }
        }
        jsonl_in(&source.root.join("archived_sessions"), &mut out);
        return out
            .into_iter()
            .map(|path| SessionFile {
                path,
                ..Default::default()
            })
            .collect();
    }
    tokens::transcripts(&source.root.join("projects"))
        .into_iter()
        .map(|t| SessionFile {
            subagent: under_subagents(&t.path),
            session: Some(t.session_id),
            project: Some(t.project),
            path: t.path,
        })
        .collect()
}

fn rel_of(root: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    Some(parts.join("/"))
}

/// 界面给回来的 id → (目标, 文件)。核对见模块头。
fn resolve(place: &Place, id: &str) -> Result<(Slot, PathBuf)> {
    let bad = || GateError::Other("这个会话 id 不对（可能已经删了或挪走了）".into());
    let (source, rel) = id.split_once('|').ok_or_else(bad)?;
    let slot = sources(place)
        .into_iter()
        .find(|s| s.id == source)
        .ok_or_else(bad)?;
    let allowed: &[&str] = if client_of(source) == "codex" {
        &["sessions", "archived_sessions"]
    } else {
        &["projects"]
    };
    let parts: Vec<&str> = rel.split('/').collect();
    if parts.len() < 2
        || !allowed.contains(&parts[0])
        || parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains(['\\', ':']))
        || !rel.to_ascii_lowercase().ends_with(".jsonl")
    {
        return Err(bad());
    }
    let mut path = slot.root.clone();
    for p in parts {
        path.push(p);
    }
    if !path.is_file() {
        return Err(GateError::NotFound("这个会话的文件不在了".into()));
    }
    Ok((slot, path))
}

// ------------------------------------------------------------------ 开头：标题、项目目录

/// 从开头读出来的那几样。
#[derive(Debug, Clone, Default, PartialEq)]
struct Head {
    session_id: Option<String>,
    cwd: Option<String>,
    started_at: Option<String>,
    title: Option<String>,
    /// Codex：`session_meta.source` 说这是子代理开的线程。
    subagent: bool,
    tokens: Option<SessionTokens>,
    /// 旧格式转写里 `bridge-session` 行上的 `ownerAccountUuid`（归属判定第 ③ 级）。
    legacy_owner: Option<String>,
}

/// 一段话压成一行、截短，当标题。
fn one_line(text: &str, max: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max {
        let mut s: String = flat.chars().take(max).collect();
        s.push('…');
        s
    } else {
        flat
    }
}

/// 能不能当标题：不是空的、不是 `<…>` 包起来的上下文 / 命令回显。
fn titleish(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty() && !t.starts_with('<')
}

/// Claude Code 一行里用户说的文字（字符串，或者 content 数组里的 text 块）。
fn claude_user_text(line: &Value) -> Option<String> {
    if line.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    match line.pointer("/message/content")? {
        Value::String(s) => Some(s.clone()),
        Value::Array(blocks) => {
            let text: Vec<&str> = blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect();
            (!text.is_empty()).then(|| text.join("\n"))
        }
        _ => None,
    }
}

/// Codex 一条 `message` 里的文字。
fn codex_text(payload: &Value) -> Option<String> {
    let text: Vec<&str> = payload
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|c| c.get("text").and_then(Value::as_str))
        .collect();
    (!text.is_empty()).then(|| text.join("\n"))
}

fn read_head(path: &Path, codex: bool) -> Head {
    let mut head = Head::default();
    if !codex {
        head.session_id = path.file_stem().map(|s| s.to_string_lossy().to_string());
    }
    let Ok(file) = std::fs::File::open(path) else {
        return head;
    };
    let reader = BufReader::new(file.take(HEAD_MAX));
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if codex {
            match v.get("type").and_then(Value::as_str) {
                Some("session_meta") => {
                    let p = v.get("payload").unwrap_or(&Value::Null);
                    head.session_id = p.get("id").and_then(Value::as_str).map(str::to_string);
                    head.cwd = p.get("cwd").and_then(Value::as_str).map(str::to_string);
                    head.subagent = p
                        .get("source")
                        .and_then(Value::as_object)
                        .is_some_and(|o| o.contains_key("subagent"));
                    head.started_at = p
                        .get("timestamp")
                        .or_else(|| v.get("timestamp"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                Some("response_item") if head.title.is_none() => {
                    let p = v.get("payload").unwrap_or(&Value::Null);
                    if p.get("type").and_then(Value::as_str) == Some("message")
                        && p.get("role").and_then(Value::as_str) == Some("user")
                    {
                        if let Some(t) = codex_text(p).filter(|t| titleish(t)) {
                            head.title = Some(one_line(&t, 120));
                        }
                    }
                }
                _ => {}
            }
        } else {
            if head.cwd.is_none() {
                head.cwd = v.get("cwd").and_then(Value::as_str).map(str::to_string);
            }
            if head.started_at.is_none() {
                head.started_at = v
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            if head.title.is_none() && v.get("type").and_then(Value::as_str) == Some("user") {
                if let Some(t) = claude_user_text(&v).filter(|t| titleish(t)) {
                    head.title = Some(one_line(&t, 120));
                }
            }
        }
        if head.title.is_some() && head.cwd.is_some() && head.session_id.is_some() {
            break;
        }
    }
    let (tokens, owner) = read_usage(path, codex);
    head.tokens = tokens;
    head.legacy_owner = owner;
    head
}

/// 整份扫一遍：token 用量，外加 Claude Code 旧格式的归属标记。只解析带关键字的行。
fn read_usage(path: &Path, codex: bool) -> (Option<SessionTokens>, Option<String>) {
    let Ok(file) = std::fs::File::open(path) else {
        return (None, None);
    };
    let mut entries: Vec<tokens::Entry> = Vec::new();
    let mut last: Option<SessionTokens> = None;
    let mut owner: Option<String> = None;
    for line in BufReader::new(file).lines() {
        // 编码坏的那一行跳过；别的读错误停读（同 `tokens::read_entries` 的理由）。
        let line = match line {
            Ok(l) => l,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => continue,
            Err(_) => break,
        };
        if codex {
            if !line.contains("\"token_count\"") {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            // 每条都是累计值：留最后一条。
            if let Some(u) = v.pointer("/payload/info/total_token_usage") {
                let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
                let (input, output) = (n("input_tokens"), n("output_tokens"));
                let total = match n("total_tokens") {
                    0 => input + output,
                    t => t,
                };
                last = Some(SessionTokens {
                    input,
                    output,
                    total,
                });
            }
        } else {
            if line.contains("\"usage\"") {
                if let Some(e) = tokens::parse_line(&line) {
                    entries.push(e);
                }
            }
            if owner.is_none() && line.contains("ownerAccountUuid") {
                owner = serde_json::from_str::<Value>(&line).ok().and_then(|v| {
                    v.get("ownerAccountUuid")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
            }
        }
    }
    if codex {
        return (last, None);
    }
    let (kept, _) = tokens::dedupe(entries);
    if kept.is_empty() {
        return (None, owner);
    }
    let mut t = SessionTokens::default();
    for e in kept {
        t.input += (e.input + e.cache_write + e.cache_read).max(0) as u64;
        t.output += e.output.max(0) as u64;
    }
    t.total = t.input + t.output;
    (Some(t), owner)
}

/// 文件 → (修改时刻, 大小, 开头那几样)。
type HeadCache = HashMap<PathBuf, (SystemTime, u64, Head)>;

/// 开头那几样按（路径、修改时刻、大小）记在内存里：列表一刷就是几百上千个文件，不能每次都重读。
static HEADS: Mutex<Option<HeadCache>> = Mutex::new(None);

fn head_of(path: &Path, codex: bool, modified: SystemTime, size: u64) -> Head {
    if let Ok(cache) = HEADS.lock() {
        if let Some((m, s, h)) = cache.as_ref().and_then(|c| c.get(path)) {
            if *m == modified && *s == size {
                return h.clone();
            }
        }
    }
    let head = read_head(path, codex);
    if let Ok(mut cache) = HEADS.lock() {
        cache
            .get_or_insert_with(HashMap::new)
            .insert(path.to_path_buf(), (modified, size, head.clone()));
    }
    head
}

fn rfc3339(t: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()
}

/// 项目目录的最后一段（`D:\work\demo` → `demo`）。
fn project_name(cwd: &str) -> Option<String> {
    cwd.trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next()
        .filter(|s| !s.is_empty() && !s.ends_with(':'))
        .map(str::to_string)
}

/// 单个文件（阅读、导出、删除用）。Claude Code 子代理转写的会话 id 取父会话目录名。
fn summarize(source: &Slot, path: &Path) -> Option<SessionSummary> {
    let subagent = under_subagents(path);
    let file = SessionFile {
        path: path.to_path_buf(),
        session: (subagent && client_of(&source.id) == "claude-code")
            .then(|| {
                let parent = path.parent()?.parent()?;
                // `<会话>\subagents\x.jsonl` 或 `<会话>\subagents\workflows\<wf>\x.jsonl`
                let parent = if parent.file_name()? == "subagents" {
                    parent.parent()?
                } else if parent.file_name()? == "workflows" {
                    parent.parent()?.parent()?
                } else {
                    parent
                };
                Some(parent.file_name()?.to_string_lossy().to_string())
            })
            .flatten(),
        project: None,
        subagent,
    };
    summarize_file(source, &file).map(|(s, _)| s)
}

fn summarize_file(source: &Slot, file: &SessionFile) -> Option<(SessionSummary, Head)> {
    let path = file.path.as_path();
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let codex = client_of(&source.id) == "codex";
    let head = head_of(path, codex, modified, meta.len());
    let rel = rel_of(&source.root, path)?;
    let project = head
        .cwd
        .as_deref()
        .and_then(project_name)
        .or_else(|| file.project.clone())
        .unwrap_or_default();
    let running = SystemTime::now()
        .duration_since(modified)
        .is_ok_and(|age| age < BUSY);
    let summary = SessionSummary {
        id: format!("{}|{rel}", source.id),
        client: client_of(&source.id).into(),
        source: source.id.clone(),
        source_label: source.label.clone(),
        official: source.official,
        session_id: file
            .session
            .clone()
            .or_else(|| head.session_id.clone())
            .unwrap_or_default(),
        title: head
            .title
            .clone()
            .unwrap_or_else(|| "（开头没有文字提问）".into()),
        cwd: head.cwd.clone(),
        started_at: head.started_at.clone(),
        updated_at: rfc3339(modified),
        size: meta.len(),
        path: path.display().to_string(),
        kind: if file.subagent || head.subagent {
            "subagent"
        } else {
            "chat"
        }
        .into(),
        project,
        tokens: head.tokens,
        running,
        unowned: false,
    };
    Some((summary, head))
}

// ------------------------------------------------------------------ 列表与搜索

/// 列表的筛选条件。
#[derive(Debug, Clone, Default)]
pub struct SessionQuery {
    /// 标题关键词（也搜项目目录、会话 id、来源名）。不分大小写。
    pub text: String,
    /// 内容关键词：读会话文件找（有上限，见 [`CONTENT_LIMITS`]）。不分大小写。
    pub content: String,
    /// `chat` / `subagent`；`None` = 全部。
    pub kind: Option<String>,
    /// 只看这个目标（`claude-code:relay` 之类）。
    pub source: Option<String>,
    pub offset: u32,
    pub limit: u32,
}

/// 内容搜索一次最多现读多少。
#[derive(Debug, Clone, Copy)]
struct Limits {
    files: usize,
    bytes: u64,
}

/// (文件, 关键词) → (修改时刻, 大小, 有没有)。文件一改就作废。
type HitCache = HashMap<(PathBuf, String), (SystemTime, u64, bool)>;
static HITS: Mutex<Option<HitCache>> = Mutex::new(None);

/// 只留内容里有 `needle`（已转小写）的。缓存里有的不读；现读的超过 `lim` 就不再读，那些当没搜到。
/// 回 `true` = 有会话因为上限没搜。
fn content_filter(items: &mut Vec<SessionSummary>, needle: &str, lim: Limits) -> bool {
    let (mut files, mut bytes, mut truncated) = (0usize, 0u64, false);
    items.retain(|s| {
        let path = Path::new(&s.path);
        let Some((modified, size)) = std::fs::metadata(path)
            .ok()
            .and_then(|m| Some((m.modified().ok()?, m.len())))
        else {
            return false;
        };
        let key = (path.to_path_buf(), needle.to_string());
        let cached = HITS.lock().ok().and_then(|c| {
            c.as_ref()?
                .get(&key)
                .filter(|(m, z, _)| *m == modified && *z == size)
                .map(|x| x.2)
        });
        if let Some(hit) = cached {
            return hit;
        }
        if files >= lim.files || bytes + size > lim.bytes {
            truncated = true;
            return false;
        }
        files += 1;
        bytes += size;
        let hit = std::fs::read(path)
            .map(|b| String::from_utf8_lossy(&b).to_lowercase().contains(needle))
            .unwrap_or(false);
        if let Ok(mut c) = HITS.lock() {
            let c = c.get_or_insert_with(HashMap::new);
            if c.len() > 20_000 {
                c.clear();
            }
            c.insert(key, (modified, size, hit));
        }
        hit
    });
    truncated
}

/// 按槽位列 Claude Code 时，官方默认目录里归这个账户的会话（归属判定同 `tokens::owner_of` 那三级）。
/// 有依据归到别的账户的不要；一点依据都没有的标 `unowned`，单列一组，不猜。
fn default_dir_for_slot(place: &Place, label: &str) -> Vec<SessionSummary> {
    let Some(default) = sources(place)
        .into_iter()
        .find(|s| s.id == "claude-code:default")
    else {
        return Vec::new();
    };
    let me = place.claude_accounts.get(label);
    let index = tokens::OwnerIndex::scan(&place.desktop_profiles);
    session_files(&default)
        .iter()
        .filter_map(|f| {
            let (mut s, head) = summarize_file(&default, f)?;
            let owner = f
                .session
                .as_deref()
                .and_then(|id| index.owner_of_session(id).map(str::to_string))
                .or_else(|| {
                    f.project
                        .as_deref()
                        .and_then(tokens::owner_from_project_name)
                })
                .or(head.legacy_owner);
            match owner {
                None => {
                    s.unowned = true;
                    Some(s)
                }
                Some(o) if me.is_some_and(|m| m.eq_ignore_ascii_case(&o)) => Some(s),
                Some(_) => None,
            }
        })
        .collect()
}

fn list_at(place: &Place, q: &SessionQuery) -> SessionList {
    list_with(place, q, CONTENT_LIMITS)
}

fn list_with(place: &Place, q: &SessionQuery, lim: Limits) -> SessionList {
    let mut all: Vec<SessionSummary> = Vec::new();
    let mut counts: Vec<SessionSourceCount> = Vec::new();
    for source in sources(place) {
        let items: Vec<SessionSummary> = session_files(&source)
            .iter()
            .filter_map(|f| summarize_file(&source, f).map(|(s, _)| s))
            .collect();
        counts.push(SessionSourceCount {
            id: source.id.clone(),
            label: source.label.clone(),
            client: client_of(&source.id).into(),
            count: items.len() as u32,
        });
        if q.source.as_deref().is_none_or(|s| s == source.id) {
            all.extend(items);
        }
    }
    if let Some(label) = q
        .source
        .as_deref()
        .and_then(|s| s.strip_prefix("claude-code:slot:"))
    {
        all.extend(default_dir_for_slot(place, label));
    }
    if let Some(kind) = q.kind.as_deref().filter(|k| !k.is_empty()) {
        all.retain(|s| s.kind == kind);
    }
    all.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let needle = q.text.trim().to_lowercase();
    if !needle.is_empty() {
        all.retain(|s| {
            [
                Some(s.title.as_str()),
                s.cwd.as_deref(),
                Some(s.session_id.as_str()),
                Some(s.source_label.as_str()),
                Some(s.project.as_str()),
            ]
            .into_iter()
            .flatten()
            .any(|f| f.to_lowercase().contains(&needle))
        });
    }
    let content = q.content.trim().to_lowercase();
    let truncated = !content.is_empty() && content_filter(&mut all, &content, lim);
    let total = all.len() as u32;
    let items = all
        .into_iter()
        .skip(q.offset as usize)
        .take(q.limit.clamp(1, 500) as usize)
        .collect();
    SessionList {
        items,
        total,
        sources: counts,
        search_truncated: truncated,
    }
}

// ------------------------------------------------------------------ 阅读

fn cut(text: &str, max: usize) -> (String, bool) {
    if text.chars().count() > max {
        (text.chars().take(max).collect(), true)
    } else {
        (text.to_string(), false)
    }
}

fn message(
    role: &str,
    kind: &str,
    name: Option<String>,
    text: &str,
    at: Option<String>,
) -> SessionMessage {
    SessionMessage {
        role: role.into(),
        kind: kind.into(),
        name,
        text: text.trim().to_string(),
        truncated: false,
        at,
    }
}

/// 截到 `max` 个字（解析时不截：界面截、导出只截工具那几类）。
fn clip(mut m: SessionMessage, max: usize) -> SessionMessage {
    let (text, truncated) = cut(&m.text, max);
    m.text = text;
    m.truncated = m.truncated || truncated;
    m
}

/// 工具结果里的文字：字符串，或者一组 `{type: text, text}`。
fn result_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|i| match i {
                Value::String(s) => Some(s.clone()),
                other => other
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| {
                        (other.get("type").and_then(Value::as_str) == Some("image"))
                            .then(|| "[图片]".into())
                    }),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Claude Code 的一行 → 零到几段。
fn claude_messages(v: &Value, out: &mut Vec<SessionMessage>) {
    let at = v
        .get("timestamp")
        .and_then(Value::as_str)
        .map(str::to_string);
    match v.get("type").and_then(Value::as_str) {
        Some("summary") => {
            if let Some(s) = v.get("summary").and_then(Value::as_str) {
                out.push(message("system", "summary", None, s, at));
            }
        }
        Some("user") => {
            if v.get("isMeta").and_then(Value::as_bool) == Some(true) {
                return;
            }
            match v.pointer("/message/content") {
                Some(Value::String(s)) => out.push(message("user", "text", None, s, at)),
                Some(Value::Array(blocks)) => {
                    for b in blocks {
                        match b.get("type").and_then(Value::as_str) {
                            Some("text") => {
                                let t = b.get("text").and_then(Value::as_str).unwrap_or("");
                                out.push(message("user", "text", None, t, at.clone()));
                            }
                            Some("tool_result") => {
                                let t = result_text(b.get("content").unwrap_or(&Value::Null));
                                out.push(message("tool", "tool_result", None, &t, at.clone()));
                            }
                            Some("image") => {
                                out.push(message("user", "text", None, "[图片]", at.clone()))
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        Some("assistant") => {
            for b in v
                .pointer("/message/content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        let t = b.get("text").and_then(Value::as_str).unwrap_or("");
                        out.push(message("assistant", "text", None, t, at.clone()));
                    }
                    Some("thinking") => {
                        let t = b.get("thinking").and_then(Value::as_str).unwrap_or("");
                        if !t.trim().is_empty() {
                            out.push(message("assistant", "thinking", None, t, at.clone()));
                        }
                    }
                    Some("tool_use") => {
                        let name = b.get("name").and_then(Value::as_str).map(str::to_string);
                        let input = b.get("input").map(|i| i.to_string()).unwrap_or_default();
                        out.push(message("assistant", "tool_use", name, &input, at.clone()));
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

/// Codex 的一行 → 零到几段。只看 `response_item`（`event_msg` 里的同一句话是重复的）。
fn codex_messages(v: &Value, out: &mut Vec<SessionMessage>) {
    if v.get("type").and_then(Value::as_str) != Some("response_item") {
        return;
    }
    let at = v
        .get("timestamp")
        .and_then(Value::as_str)
        .map(str::to_string);
    let p = v.get("payload").unwrap_or(&Value::Null);
    let name = || p.get("name").and_then(Value::as_str).map(str::to_string);
    match p.get("type").and_then(Value::as_str) {
        Some("message") => {
            let role = p.get("role").and_then(Value::as_str).unwrap_or("assistant");
            let Some(text) = codex_text(p) else { return };
            // 环境说明、AGENTS.md 那些是 Codex 自己塞进去的上下文，不是使用者说的话。
            if role == "user" && !titleish(&text) {
                return;
            }
            let role = if role == "user" { "user" } else { "assistant" };
            out.push(message(role, "text", None, &text, at));
        }
        Some("reasoning") => {
            let text: Vec<&str> = p
                .get("summary")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|s| s.get("text").and_then(Value::as_str))
                .collect();
            if !text.is_empty() {
                out.push(message("assistant", "thinking", None, &text.join("\n"), at));
            }
        }
        Some("function_call") => {
            let args = p.get("arguments").and_then(Value::as_str).unwrap_or("");
            out.push(message("assistant", "tool_use", name(), args, at));
        }
        Some("custom_tool_call") => {
            let input = p.get("input").and_then(Value::as_str).unwrap_or("");
            out.push(message("assistant", "tool_use", name(), input, at));
        }
        Some("local_shell_call") => {
            let cmd = p
                .pointer("/action/command")
                .and_then(Value::as_array)
                .map(|c| {
                    c.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            out.push(message(
                "assistant",
                "tool_use",
                Some("shell".into()),
                &cmd,
                at,
            ));
        }
        Some("function_call_output") | Some("custom_tool_call_output") => {
            // 有的版本是一段字符串，有的是 `{"output": "...", "metadata": …}` 的 JSON 文本。
            let raw = p.get("output").map(result_text).unwrap_or_default();
            let text = serde_json::from_str::<Value>(&raw)
                .ok()
                .and_then(|o| o.get("output").and_then(Value::as_str).map(str::to_string))
                .unwrap_or(raw);
            out.push(message("tool", "tool_result", None, &text, at));
        }
        _ => {}
    }
}

fn all_messages(path: &Path, codex: bool) -> Result<Vec<SessionMessage>> {
    let file = std::fs::File::open(path)?;
    let mut out = Vec::new();
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if codex {
            codex_messages(&v, &mut out);
        } else {
            claude_messages(&v, &mut out);
        }
    }
    out.retain(|m| !m.text.is_empty());
    Ok(out)
}

/// PowerShell 单引号字符串（里面的 `'` 写两遍）。
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// 恢复命令和要先知道的那句话。
fn resume(summary: &SessionSummary, root: &Path) -> (String, String) {
    let mut parts: Vec<String> = Vec::new();
    if let Some(cwd) = &summary.cwd {
        parts.push(format!("Set-Location -LiteralPath {}", ps_quote(cwd)));
    }
    let default = summary.source.ends_with(":default");
    let codex = summary.client == "codex";
    if !default {
        let var = if codex {
            "CODEX_HOME"
        } else {
            "CLAUDE_CONFIG_DIR"
        };
        parts.push(format!(
            "$env:{var} = {}",
            ps_quote(&root.display().to_string())
        ));
    }
    parts.push(if codex {
        format!("codex resume {}", summary.session_id)
    } else {
        format!("claude --resume {}", summary.session_id)
    });
    let cli = if codex { "codex" } else { "claude" };
    let note = if summary.source.ends_with(":relay") {
        format!("这是中转环境的会话：要本机路由开着（从中转站页起一次这个软件就会拉起它），还要装了 {cli} 命令行。")
    } else {
        format!("这是官方账户的会话：门禁开着时 Windows 会拒绝直接启动 {cli}，先在面板里放行；还要装了 {cli} 命令行。")
    };
    (parts.join("; "), note)
}

fn read_at(place: &Place, id: &str, offset: u32, limit: u32) -> Result<SessionDetail> {
    let (source, path) = resolve(place, id)?;
    let summary = summarize(&source, &path)
        .ok_or_else(|| GateError::Other("读不出这个会话文件的信息".into()))?;
    let all = all_messages(&path, summary.client == "codex")?;
    let total = all.len() as u32;
    let messages = all
        .into_iter()
        .skip(offset as usize)
        .take(limit.clamp(1, 1000) as usize)
        .map(|m| clip(m, TEXT_MAX))
        .collect();
    let (resume, resume_note) = resume(&summary, &source.root);
    Ok(SessionDetail {
        summary,
        messages,
        total,
        offset,
        resume,
        resume_note,
    })
}

// ------------------------------------------------------------------ 导出

fn md_fence(text: &str) -> String {
    // 内容里有 ``` 就加长围栏，免得提前结束。
    let mut fence = "```".to_string();
    while text.contains(&fence) {
        fence.push('`');
    }
    format!("{fence}\n{text}\n{fence}")
}

fn markdown_at(place: &Place, id: &str) -> Result<String> {
    let (source, path) = resolve(place, id)?;
    let s = summarize(&source, &path)
        .ok_or_else(|| GateError::Other("读不出这个会话文件的信息".into()))?;
    let mut out = format!("# {}\n\n", s.title);
    out.push_str(&format!("- 来源：{}\n", s.source_label));
    out.push_str(&format!("- 会话 id：`{}`\n", s.session_id));
    if let Some(cwd) = &s.cwd {
        out.push_str(&format!("- 项目目录：`{cwd}`\n"));
    }
    if let Some(at) = &s.started_at {
        out.push_str(&format!("- 开始：{at}\n"));
    }
    out.push_str(&format!("- 最后写入：{}\n\n---\n\n", s.updated_at));
    for m in all_messages(&path, s.client == "codex")? {
        let m = if m.kind == "text" || m.kind == "summary" {
            m
        } else {
            clip(m, TEXT_MAX)
        };
        let more = if m.truncated {
            "\n\n（后面截掉了）"
        } else {
            ""
        };
        let block = match (m.role.as_str(), m.kind.as_str()) {
            ("user", _) => format!("## 用户\n\n{}{more}", m.text),
            ("assistant", "text") => format!("## 助手\n\n{}{more}", m.text),
            ("assistant", "thinking") => {
                format!("> 思考\n>\n> {}{more}", m.text.replace('\n', "\n> "))
            }
            ("assistant", "tool_use") => format!(
                "**调用工具 {}**\n\n{}{more}",
                m.name.as_deref().unwrap_or(""),
                md_fence(&m.text)
            ),
            ("tool", _) => format!("**工具输出**\n\n{}{more}", md_fence(&m.text)),
            _ => format!("_{}_", m.text),
        };
        out.push_str(&block);
        out.push_str("\n\n");
    }
    Ok(out)
}

/// 导出到使用者在保存框里选的地方。只认 `.md`；同名就覆盖（保存框自己会问一句）。
fn export_at(place: &Place, id: &str, target: &Path) -> Result<()> {
    if !target
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("md"))
    {
        return Err(GateError::Other("只导出成 .md 文件".into()));
    }
    let text = markdown_at(place, id)?;
    crate::config_io::replace(target, Some(text.as_bytes()))?;
    Ok(())
}

// ------------------------------------------------------------------ 删除与回收站

fn trash(place: &Place) -> Trash {
    Trash::at(place.state.join("session-trash"))
}

/// 挪进回收站。Claude Code 同名的那个目录（子任务记录之类）一起挪。
fn delete_at(place: &Place, id: &str) -> Result<TrashEntry> {
    let (source, path) = resolve(place, id)?;
    let modified = std::fs::metadata(&path)?.modified()?;
    if SystemTime::now()
        .duration_since(modified)
        .is_ok_and(|age| age < BUSY)
    {
        return Err(GateError::Other(
            "这个会话两分钟内还有写入，多半正开着 —— 关掉那个会话再删".into(),
        ));
    }
    let title = summarize(&source, &path)
        .map(|s| s.title)
        .unwrap_or_else(|| "会话".into());
    let mut items = vec![path.clone()];
    if client_of(&source.id) == "claude-code" {
        if let Some(stem) = path.file_stem() {
            let sibling = path.with_file_name(stem);
            if sibling.is_dir() {
                items.push(sibling);
            }
        }
    }
    // 审计那一行在命令层写（`commands::sessions`）：这里写的话单测会往使用者真实的 ip-gate.log 里追加。
    trash(place).put("session", &format!("{} · {title}", source.label), &items)
}

fn trash_list_at(place: &Place) -> Vec<TrashEntry> {
    trash(place).list()
}

fn trash_restore_at(place: &Place, id: &str) -> Result<TrashEntry> {
    trash(place).restore(id)
}

fn trash_purge_at(place: &Place, id: Option<&str>) -> Result<u32> {
    trash(place).purge(id)
}

// ------------------------------------------------------------------ 给命令层的入口

/// 真实的那张目标表（单测走 `Place::under`，不碰真实状态）。
fn real() -> Result<Place> {
    let db = crate::repository::Repository::open()?;
    Place::real(&db)
}

pub fn list(q: &SessionQuery) -> Result<SessionList> {
    Ok(list_at(&real()?, q))
}

pub fn read(id: &str, offset: u32, limit: u32) -> Result<SessionDetail> {
    read_at(&real()?, id, offset, limit)
}

pub fn markdown(id: &str) -> Result<String> {
    markdown_at(&real()?, id)
}

pub fn export(id: &str, target: &Path) -> Result<()> {
    export_at(&real()?, id, target)
}

/// 在资源管理器里选中这个会话文件（id 照样核对，编不出别处的路径）。
pub fn reveal(id: &str) -> Result<()> {
    let (_, path) = resolve(&real()?, id)?;
    #[cfg(windows)]
    {
        // explorer 的退出码不可靠（选中了也回 1），起来就算。
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(GateError::Other("只在 Windows 上能打开所在位置".into()))
    }
}

pub fn delete(id: &str) -> Result<TrashEntry> {
    delete_at(&real()?, id)
}

pub fn trash_list() -> Result<Vec<TrashEntry>> {
    Ok(trash_list_at(&real()?))
}

pub fn trash_restore(id: &str) -> Result<TrashEntry> {
    trash_restore_at(&real()?, id)
}

pub fn trash_purge(id: Option<&str>) -> Result<u32> {
    trash_purge_at(&real()?, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn place() -> (PathBuf, Place) {
        let root = std::env::temp_dir().join(format!(
            "qb-sessions-{}-{}",
            std::process::id(),
            crate::config_io::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let p = Place::under(&root);
        (root, p)
    }

    fn write(path: &Path, lines: &[Value]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = std::fs::File::create(path).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
    }

    fn age(path: &Path, secs: u64) {
        let t = SystemTime::now() - Duration::from_secs(secs);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    fn claude_sample(root: &Path) -> PathBuf {
        let path = root.join("slot-a/projects/D--work-demo/0b1c-uuid.jsonl");
        write(
            &path,
            &[
                serde_json::json!({"type": "user", "isMeta": true, "cwd": "D:\\work\\demo", "timestamp": "2026-10-20T01:00:00Z",
                    "message": {"role": "user", "content": "Caveat: local command"}}),
                serde_json::json!({"type": "user", "cwd": "D:\\work\\demo", "timestamp": "2026-10-20T01:00:01Z",
                    "message": {"role": "user", "content": "<command-name>/clear</command-name>"}}),
                serde_json::json!({"type": "user", "cwd": "D:\\work\\demo", "timestamp": "2026-10-20T01:00:02Z",
                    "message": {"role": "user", "content": [{"type": "text", "text": "帮我把\n登录页  改成深色"}]}}),
                serde_json::json!({"type": "assistant", "timestamp": "2026-10-20T01:00:05Z", "message": {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "先看样式"},
                    {"type": "text", "text": "好的"},
                    {"type": "tool_use", "name": "Edit", "input": {"file": "login.css"}}]}}),
                serde_json::json!({"type": "user", "timestamp": "2026-10-20T01:00:06Z", "message": {"role": "user", "content": [
                    {"type": "tool_result", "content": [{"type": "text", "text": "```done```"}]}]}}),
                serde_json::json!({"type": "file-history-snapshot", "snapshot": {}}),
            ],
        );
        path
    }

    #[test]
    fn claude_sessions_are_listed_with_a_real_title_and_read_without_meta_lines() {
        let (root, p) = place();
        let path = claude_sample(&root);
        age(&path, 600);
        let list = list_at(
            &p,
            &SessionQuery {
                limit: 50,
                ..Default::default()
            },
        );
        assert_eq!(list.total, 1);
        let s = &list.items[0];
        assert_eq!(
            s.title, "帮我把 登录页 改成深色",
            "跳过 isMeta 和 <command-name> 那两行，压成一行"
        );
        assert_eq!(s.cwd.as_deref(), Some("D:\\work\\demo"));
        assert_eq!(s.session_id, "0b1c-uuid");
        assert_eq!(s.source, "claude-code:slot:a");
        assert!(s.official);
        let d = read_at(&p, &s.id, 0, 100).unwrap();
        let kinds: Vec<(&str, &str)> = d
            .messages
            .iter()
            .map(|m| (m.role.as_str(), m.kind.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("user", "text"),
                ("user", "text"),
                ("assistant", "thinking"),
                ("assistant", "text"),
                ("assistant", "tool_use"),
                ("tool", "tool_result")
            ]
        );
        assert_eq!(d.messages[4].name.as_deref(), Some("Edit"));
        assert!(d.resume.contains("$env:CLAUDE_CONFIG_DIR = '"));
        assert!(d.resume.ends_with("claude --resume 0b1c-uuid"));
        assert!(d.resume_note.contains("门禁"));
        let md = markdown_at(&p, &s.id).unwrap();
        assert!(md.starts_with("# 帮我把 登录页 改成深色"));
        assert!(
            md.contains("````\n```done```\n````"),
            "内容里有 ``` 就加长围栏"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn codex_rollouts_skip_injected_context_and_duplicate_events() {
        let (root, p) = place();
        let path = root.join("env-codex/sessions/2026/10/20/rollout-2026-10-20T01-00-00-abc.jsonl");
        write(
            &path,
            &[
                serde_json::json!({"timestamp": "2026-10-20T01:00:00Z", "type": "session_meta",
                    "payload": {"id": "abc-123", "timestamp": "2026-10-20T01:00:00Z", "cwd": "D:\\proj"}}),
                serde_json::json!({"type": "response_item", "payload": {"type": "message", "role": "user",
                    "content": [{"type": "input_text", "text": "<environment_context>cwd</environment_context>"}]}}),
                serde_json::json!({"type": "response_item", "payload": {"type": "message", "role": "user",
                    "content": [{"type": "input_text", "text": "修一下 CI"}]}}),
                serde_json::json!({"type": "event_msg", "payload": {"type": "user_message", "message": "修一下 CI"}}),
                serde_json::json!({"type": "response_item", "payload": {"type": "function_call", "name": "shell",
                    "arguments": "{\"command\":[\"cargo\",\"test\"]}", "call_id": "c1"}}),
                serde_json::json!({"type": "response_item", "payload": {"type": "function_call_output", "call_id": "c1",
                    "output": "{\"output\":\"ok\",\"metadata\":{}}"}}),
                serde_json::json!({"type": "response_item", "payload": {"type": "message", "role": "assistant",
                    "content": [{"type": "output_text", "text": "好了"}]}}),
            ],
        );
        age(&path, 600);
        let list = list_at(
            &p,
            &SessionQuery {
                limit: 50,
                ..Default::default()
            },
        );
        let s = list.items.iter().find(|s| s.client == "codex").unwrap();
        assert_eq!(
            (s.title.as_str(), s.session_id.as_str()),
            ("修一下 CI", "abc-123")
        );
        assert!(!s.official);
        let d = read_at(&p, &s.id, 0, 100).unwrap();
        let texts: Vec<&str> = d.messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "修一下 CI",
                "{\"command\":[\"cargo\",\"test\"]}",
                "ok",
                "好了"
            ]
        );
        assert!(d.resume.ends_with("codex resume abc-123"));
        assert!(d.resume_note.contains("本机路由"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_forged_id_cannot_reach_another_file() {
        let (root, p) = place();
        claude_sample(&root);
        std::fs::write(root.join("slot-a/secret.jsonl"), "{}").unwrap();
        for id in [
            "claude-code:slot:a|../secret.jsonl",
            "claude-code:slot:a|secret.jsonl",
            "claude-code:slot:a|projects/D--work-demo/../../secret.jsonl",
            "claude-code:slot:a|projects/D--work-demo/0b1c-uuid.json",
            "claude-code:slot:zzz|projects/D--work-demo/0b1c-uuid.jsonl",
            "codex:relay|projects/D--work-demo/0b1c-uuid.jsonl",
            "claude-code:slot:a|projects/C:/x.jsonl",
        ] {
            assert!(resolve(&p, id).is_err(), "{id}");
        }
        assert!(resolve(
            &p,
            "claude-code:slot:a|projects/D--work-demo/0b1c-uuid.jsonl"
        )
        .is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deleting_moves_into_the_trash_refuses_a_busy_file_and_can_be_undone() {
        let (root, p) = place();
        let path = claude_sample(&root);
        std::fs::create_dir_all(path.with_file_name("0b1c-uuid").join("subagents")).unwrap();
        let id = "claude-code:slot:a|projects/D--work-demo/0b1c-uuid.jsonl";
        let err = delete_at(&p, id).unwrap_err().to_string();
        assert!(err.contains("两分钟内"), "{err}");
        age(&path, 600);
        let entry = delete_at(&p, id).unwrap();
        assert!(!path.exists() && !path.with_file_name("0b1c-uuid").exists());
        assert_eq!(entry.originals.len(), 2, "同名目录一起挪");
        assert_eq!(trash_list_at(&p).len(), 1);
        trash_restore_at(&p, &entry.id).unwrap();
        assert!(path.exists());
        assert_eq!(
            list_at(
                &p,
                &SessionQuery {
                    limit: 5,
                    ..Default::default()
                }
            )
            .total,
            1
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_matches_title_and_only_reads_content_when_asked() {
        let (root, p) = place();
        let path = claude_sample(&root);
        age(&path, 600);
        let q = |text: &str, deep: bool| {
            let (text, content) = if deep {
                (String::new(), text.to_string())
            } else {
                (text.to_string(), String::new())
            };
            list_at(
                &p,
                &SessionQuery {
                    text,
                    content,
                    limit: 10,
                    ..Default::default()
                },
            )
            .total
        };

        assert_eq!(q("深色", false), 1);
        assert_eq!(q("login.css", false), 0, "标题、目录里没有");
        assert_eq!(q("LOGIN.CSS", true), 1, "内容里搜，不分大小写");
        assert_eq!(q("work\\demo", false), 1, "项目目录");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reading_clips_long_text_but_the_export_keeps_what_was_said() {
        let (root, p) = place();
        let long = "长".repeat(TEXT_MAX + 50);
        let path = root.join("slot-a/projects/x/long.jsonl");
        write(
            &path,
            &[
                serde_json::json!({"type": "user", "cwd": "D:/x", "message": {"role": "user", "content": long}}),
                serde_json::json!({"type": "user", "message": {"role": "user", "content": [
                    {"type": "tool_result", "content": long}]}}),
            ],
        );
        let id = "claude-code:slot:a|projects/x/long.jsonl";
        let d = read_at(&p, id, 0, 10).unwrap();
        assert!(d
            .messages
            .iter()
            .all(|m| m.truncated && m.text.chars().count() == TEXT_MAX));
        let md = markdown_at(&p, id).unwrap();
        assert!(md.contains(&long), "使用者说的话整段导出");
        assert_eq!(
            md.matches("（后面截掉了）").count(),
            1,
            "只截了工具输出那一段"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resume_commands_quote_paths_for_powershell() {
        let s = SessionSummary {
            id: String::new(),
            client: "claude-code".into(),
            source: "claude-code:default".into(),
            source_label: String::new(),
            official: true,
            session_id: "u1".into(),
            title: String::new(),
            cwd: Some("D:\\it's mine".into()),
            started_at: None,
            updated_at: String::new(),
            size: 0,
            path: String::new(),
            kind: "chat".into(),
            project: String::new(),
            tokens: None,
            running: false,
            unowned: false,
        };
        let (cmd, _) = resume(&s, Path::new("C:\\Users\\demo\\.claude"));
        assert_eq!(
            cmd, "Set-Location -LiteralPath 'D:\\it''s mine'; claude --resume u1",
            "默认目录不设变量"
        );
    }

    fn q(source: Option<&str>, kind: Option<&str>) -> SessionQuery {
        SessionQuery {
            source: source.map(str::to_string),
            kind: kind.map(str::to_string),
            limit: 100,
            ..Default::default()
        }
    }

    #[test]
    fn subagent_transcripts_are_listed_as_their_own_kind_with_token_totals() {
        let (root, p) = place();
        let main = claude_sample(&root);
        let sub = main
            .with_file_name("0b1c-uuid")
            .join("subagents/agent-1.jsonl");
        write(
            &sub,
            &[
                serde_json::json!({"type": "user", "cwd": "D:\\work\\demo", "message": {"role": "user", "content": "查一下样式"}}),
                serde_json::json!({"type": "assistant", "requestId": "r1", "timestamp": "2026-10-20T01:00:05Z",
                    "message": {"id": "m1", "model": "claude-opus-5", "stop_reason": "end_turn",
                        "usage": {"input_tokens": 10, "output_tokens": 5, "cache_creation_input_tokens": 1, "cache_read_input_tokens": 4}}}),
            ],
        );
        let chats = list_at(&p, &q(None, Some("chat")));
        assert_eq!(chats.total, 1);
        assert_eq!(chats.items[0].project, "demo", "项目名取 cwd 最后一段");
        let subs = list_at(&p, &q(None, Some("subagent")));
        assert_eq!(subs.total, 1);
        let s = &subs.items[0];
        assert_eq!(
            (s.title.as_str(), s.session_id.as_str()),
            ("查一下样式", "0b1c-uuid"),
            "子代理归父会话"
        );
        assert_eq!(
            s.tokens,
            Some(SessionTokens {
                input: 15,
                output: 5,
                total: 20
            })
        );
        assert!(s.running, "刚写的");
        assert_eq!(list_at(&p, &q(None, None)).total, 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn content_search_stops_at_the_cap_and_reuses_what_it_already_read() {
        let (root, p) = place();
        for i in 0..3 {
            let path = root.join(format!("slot-a/projects/x/s{i}.jsonl"));
            write(
                &path,
                &[
                    serde_json::json!({"type": "user", "message": {"role": "user", "content": format!("第 {i} 个 NEEDLE")}}),
                ],
            );
        }
        let lim = Limits {
            files: 2,
            bytes: 1 << 20,
        };
        let query = SessionQuery {
            content: "needle".into(),
            limit: 10,
            ..Default::default()
        };
        let first = list_with(&p, &query, lim);
        assert_eq!(first.total, 2, "只现读两个文件");
        assert!(first.search_truncated);
        let second = list_with(&p, &query, lim);
        assert_eq!(second.total, 3, "读过的走缓存，不占上限");
        assert!(!second.search_truncated);
        let tiny = Limits {
            files: 10,
            bytes: 1,
        };
        let none = list_with(
            &p,
            &SessionQuery {
                content: "别的词".into(),
                limit: 10,
                ..Default::default()
            },
            tiny,
        );
        assert_eq!((none.total, none.search_truncated), (0, true), "字节上限");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_slot_list_takes_owned_default_dir_sessions_and_marks_the_unknown_ones() {
        const ME: &str = "11111111-2222-3333-4444-555555555555";
        const OTHER: &str = "99999999-2222-3333-4444-555555555555";
        let (root, mut p) = place();
        p.claude_accounts.insert("a".into(), ME.into());
        claude_sample(&root);
        let home = root.join("home/.claude/projects");
        let line = serde_json::json!({"type": "user", "cwd": "D:\\x", "message": {"role": "user", "content": "你好"}});
        write(
            &home.join(format!("C--scratch-workspaces-{ME}-org-1/mine.jsonl")),
            std::slice::from_ref(&line),
        );
        write(
            &home.join(format!("C--scratch-workspaces-{OTHER}-org-1/theirs.jsonl")),
            std::slice::from_ref(&line),
        );
        write(
            &home.join("D--x/legacy.jsonl"),
            &[
                serde_json::json!({"type": "bridge-session", "ownerAccountUuid": ME}),
                line.clone(),
            ],
        );
        write(&home.join("D--x/stray.jsonl"), std::slice::from_ref(&line));
        let list = list_at(&p, &q(Some("claude-code:slot:a"), None));
        let mut got: Vec<(String, bool)> = list
            .items
            .iter()
            .map(|s| (s.session_id.clone(), s.unowned))
            .collect();
        got.sort();
        assert_eq!(
            got,
            [
                ("0b1c-uuid".to_string(), false),
                ("legacy".to_string(), false),
                ("mine".to_string(), false),
                ("stray".to_string(), true),
            ],
            "归到别的账户的不要，归不出的单列"
        );
        let default_ids: Vec<&str> = list
            .items
            .iter()
            .filter(|s| s.session_id != "0b1c-uuid")
            .map(|s| s.source.as_str())
            .collect();
        assert!(default_ids.iter().all(|s| *s == "claude-code:default"));
        assert_eq!(
            list_at(&p, &q(Some("claude-code:default"), None)).total,
            4,
            "直接看默认目录还是全有"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
