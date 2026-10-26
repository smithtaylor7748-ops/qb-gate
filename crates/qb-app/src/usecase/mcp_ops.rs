// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! MCP 面板（中转站重构阶段 5，cc-switch 的 M1）：一条服务器勾上哪些「目标」，就写进哪些配置。
//!
//! cc-switch 一个客户端一个开关（`enabled_claude`……），写进那个客户端唯一的一份配置。QB Gate 的 Claude Code / Codex /
//! Gemini CLI 都有好几个目录：中转环境（面板自己的）、官方默认目录、官方槽位。所以开关是按**目标**来的
//! （[`TARGETS`] 那几种，id 形如 `claude-code:relay`、`claude-code:slot:<标签>`、`opencode`）。
//!
//! 守住的几条：
//! - **只动库里登记的那几条**：取消勾选 / 删除时只删这条服务器自己的键，使用者手写的别的 MCP 一条不碰；
//! - 官方目录、槽位、各个工具都是**使用者自己勾**才写，新建的服务器默认只勾中转环境；
//! - 工具或目录不在（没装）就拒绝，不替它建；配置解析不了就停，不覆盖（各写法见 `qb_extensions::mcp`）；
//! - 写盘走 `config_io::commit`（核对没被外部改过、留原内容、失败回滚）；写的是中转环境里有配置清单的那几份
//!   （Codex 的 `config.toml`、Gemini CLI 的 `settings.json`、Grok Build 的 `config.toml`）时，写完按中转环境重新应用一次，
//!   指纹对上，下次启动不会报「配置被外部改过」；
//! - 连接定义里常有 Key（`env`、`headers`），**库里加密存**（跟原来扩展中心导入的 MCP 一样）。

use crate::domain::{Client, ExtensionInstallation, IdentityKind};
use crate::error::{GateError, Result};
use crate::mcp::{self, Format, McpServer};
use crate::repository::Repository;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ts_rs::TS;

const TABLE: &str = "mcp_servers";
const MIGRATED: &str = "mcp-migrated-installations";

/// 一个写得进 MCP 的地方。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct McpTarget {
    pub id: String,
    pub label: String,
    pub path: String,
    /// 官方那一侧（默认目录、槽位）。界面上要使用者自己勾。
    pub official: bool,
    /// 目录在不在（不在多半是没装：勾不上）。
    pub available: bool,
    /// 这份配置里现在有哪几条（含使用者手写的）。
    pub entries: Vec<String>,
    /// 读这份配置出的问题（解析不了之类）。
    pub problem: Option<String>,
}

/// 面板要的整份：服务器、目标。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct McpView {
    pub servers: Vec<McpServer>,
    pub targets: Vec<McpTarget>,
}

/// 从一个目标导入的结果。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct McpImportReport {
    /// 新加进来的。
    pub added: Vec<String>,
    /// 已经有、这次给它勾上了这个目标的。
    pub linked: Vec<String>,
    /// 没导入的和原因（同名但连接方式不一样、格式不对）。
    pub skipped: Vec<String>,
    pub view: McpView,
}

/// 库里存的样子：连接定义加密。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stored {
    id: String,
    name: String,
    sealed: String,
    targets: Vec<String>,
    description: Option<String>,
    homepage: Option<String>,
    tags: Vec<String>,
}

fn seal(s: &McpServer) -> Result<Stored> {
    Ok(Stored {
        id: s.id.clone(),
        name: s.name.clone(),
        sealed: crate::secret::seal(&serde_json::to_string(&s.server)?)?,
        targets: s.targets.clone(),
        description: s.description.clone(),
        homepage: s.homepage.clone(),
        tags: s.tags.clone(),
    })
}

fn open(s: Stored) -> Result<McpServer> {
    let text = crate::secret::open(&s.sealed)
        .ok_or_else(|| GateError::Other(format!("MCP 服务器 {} 的配置解不开，请重新导入", s.id)))?;
    Ok(McpServer {
        id: s.id,
        name: s.name,
        server: serde_json::from_str(&text)?,
        targets: s.targets,
        description: s.description,
        homepage: s.homepage,
        tags: s.tags,
    })
}

fn servers(db: &Repository) -> Result<Vec<McpServer>> {
    let mut out: Vec<McpServer> = db
        .list::<Stored>(TABLE)?
        .into_iter()
        .map(open)
        .collect::<Result<_>>()?;
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

// ------------------------------------------------------------------ 目标

/// 一个目标落到哪份文件、用哪种写法。
#[derive(Debug, Clone)]
struct Target {
    id: String,
    label: String,
    path: PathBuf,
    format: Format,
    official: bool,
    available: bool,
    /// 中转环境里有配置清单的那几份：写完要按这个软件的中转环境重新应用一次。
    reapply: Option<Client>,
}

/// 在哪找、写盘日志放哪。平时是真目录；单测换成临时目录（单测不碰真实状态）。
struct Place {
    home: PathBuf,
    relay: BTreeMap<&'static str, PathBuf>,
    claude_slots: Vec<(String, PathBuf)>,
    codex_slots: Vec<(String, String, PathBuf)>,
    env: fn(&str) -> Option<String>,
    journal: Option<PathBuf>,
    windows: bool,
    reapply: bool,
}

impl Place {
    fn real(db: &Repository) -> Result<Self> {
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
            journal: None,
            windows: cfg!(windows),
            reapply: true,
        })
    }

    fn targets(&self) -> Vec<Target> {
        use crate::relay::coexist::CoexistTool;
        let h = &self.home;
        let t = |id: &str,
                 label: &str,
                 path: PathBuf,
                 format: Format,
                 official: bool,
                 available: bool,
                 reapply: Option<Client>| Target {
            id: id.into(),
            label: label.into(),
            path,
            format,
            official,
            available,
            reapply,
        };
        let mut out = vec![
            t(
                "claude-code:relay",
                "Claude Code · 中转环境",
                self.relay["claude-code"].join(".claude.json"),
                Format::ClaudeJson,
                false,
                true,
                None,
            ),
            t(
                "claude-code:default",
                "Claude Code · 官方默认目录",
                h.join(".claude.json"),
                Format::ClaudeJson,
                true,
                h.join(".claude").is_dir() || h.join(".claude.json").is_file(),
                None,
            ),
        ];
        for (label, dir) in &self.claude_slots {
            out.push(t(
                &format!("claude-code:slot:{label}"),
                &format!("Claude Code · 官方槽位 {label}"),
                dir.join(".claude.json"),
                Format::ClaudeJson,
                true,
                dir.is_dir(),
                None,
            ));
        }
        out.push(t(
            "codex:relay",
            "Codex · 中转环境",
            self.relay["codex"].join("config.toml"),
            Format::CodexToml,
            false,
            true,
            Some(Client::Codex),
        ));
        out.push(t(
            "codex:default",
            "Codex · 官方默认目录",
            h.join(".codex").join("config.toml"),
            Format::CodexToml,
            true,
            h.join(".codex").is_dir(),
            None,
        ));
        for (id, label, home) in &self.codex_slots {
            out.push(t(
                &format!("codex:slot:{id}"),
                &format!("Codex · 官方槽位 {label}"),
                home.join("config.toml"),
                Format::CodexToml,
                true,
                home.is_dir(),
                None,
            ));
        }
        out.push(t(
            "gemini-cli:relay",
            "Gemini CLI · 中转环境",
            self.relay["gemini-cli"]
                .join(".gemini")
                .join("settings.json"),
            Format::GeminiJson,
            false,
            true,
            Some(Client::GeminiCli),
        ));
        out.push(t(
            "gemini-cli:default",
            "Gemini CLI · 官方默认目录",
            h.join(".gemini").join("settings.json"),
            Format::GeminiJson,
            true,
            h.join(".gemini").is_dir(),
            None,
        ));
        // Grok Build 的中转配置本来就写在它自己的 `~\.grok\config.toml`（阶段 4）：只有这一份。
        out.push(t(
            "grok-build",
            "Grok Build",
            h.join(".grok").join("config.toml"),
            Format::GrokToml,
            false,
            h.join(".grok").is_dir(),
            Some(Client::GrokBuild),
        ));
        let coexist = |tool: CoexistTool| -> (PathBuf, bool) {
            let candidates = tool.config_candidates(h, self.env);
            let path = candidates
                .iter()
                .find(|p| p.is_file())
                .cloned()
                .unwrap_or_else(|| candidates[0].clone());
            (path, tool.config_dir(h, self.env).is_dir())
        };
        let (p, a) = coexist(CoexistTool::OpenCode);
        out.push(t(
            "opencode",
            "OpenCode",
            p,
            Format::OpenCode,
            false,
            a,
            None,
        ));
        let dir = CoexistTool::MiniMaxCode.config_dir(h, self.env);
        out.push(t(
            "minimax-code",
            "MiniMax Code",
            dir.join("mcp.json"),
            Format::MiniMax,
            false,
            dir.is_dir(),
            None,
        ));
        let (p, a) = coexist(CoexistTool::Hermes);
        out.push(t("hermes", "Hermes", p, Format::Hermes, false, a, None));
        let dir = h.join(".pi").join("agent");
        out.push(t(
            "pi",
            "Pi",
            dir.join("mcp.json"),
            Format::Pi,
            false,
            dir.is_dir(),
            None,
        ));
        out
    }

    fn target(&self, id: &str) -> Result<Target> {
        self.targets()
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| GateError::Other(format!("不认得这个目标：{id}")))
    }

    fn commit(&self, edits: Vec<crate::config_io::Edit>) -> Result<()> {
        if edits.is_empty() {
            return Ok(());
        }
        match &self.journal {
            Some(dir) => crate::config_io::commit_in(dir, edits),
            None => crate::config_io::commit(edits),
        }
        .map(drop)
    }
}

// ------------------------------------------------------------------ 一份配置里的读写

fn read(path: &Path) -> Result<(Option<Vec<u8>>, String)> {
    let bytes = crate::config_io::read_optional(path)?;
    let text = String::from_utf8(bytes.clone().unwrap_or_default())
        .map_err(|_| GateError::Other(format!("{} 不是 UTF-8 文本", path.display())))?;
    Ok((bytes, text))
}

fn json_key(format: Format) -> &'static str {
    if format == Format::OpenCode {
        "mcp"
    } else {
        "mcpServers"
    }
}

fn yaml_to_json(v: serde_yaml_ng::Value) -> Result<Value> {
    serde_json::to_value(v).map_err(|e| GateError::Other(e.to_string()))
}

/// 这份配置里现有的条目（各家原样）。
fn entries(t: &Target, text: &str) -> Result<BTreeMap<String, Value>> {
    let map = |v: Option<&Value>| -> Result<BTreeMap<String, Value>> {
        match v {
            None | Some(Value::Null) => Ok(BTreeMap::new()),
            Some(Value::Object(o)) => Ok(o.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
            Some(_) => Err(GateError::Other(format!(
                "{} 里的 MCP 那一节不是对象",
                t.path.display()
            ))),
        }
    };
    match t.format {
        Format::CodexToml | Format::GrokToml => {
            let doc: toml_edit::DocumentMut = text.parse().map_err(|e| {
                GateError::Other(format!("{} 不是合法的 TOML：{e}", t.path.display()))
            })?;
            Ok(mcp::toml_entries(&doc)
                .into_iter()
                .filter_map(|(k, v)| Some((k, mcp::toml_item_json(&v)?)))
                .collect())
        }
        Format::OpenCode => map(crate::relay::coexist::jsonc_value(text)?.get("mcp")),
        Format::Hermes => {
            let section = crate::relay_live::patch::yaml::section(text, "mcp_servers")?;
            map(section.map(yaml_to_json).transpose()?.as_ref())
        }
        _ => {
            let (doc, _) = crate::relay_live::patch::json::parse(
                &t.path,
                (!text.trim().is_empty()).then_some(text.as_bytes()),
            )
            .map_err(|e| GateError::Other(e.to_string()))?;
            map(doc.get(json_key(t.format)))
        }
    }
}

/// 换上 / 删掉一条（`spec` 为 `None` = 删），回新的全文。
fn write(t: &Target, text: &str, id: &str, spec: Option<&Value>, windows: bool) -> Result<String> {
    let existing = entries(t, text)?;
    if t.format == Format::Pi && spec.is_some() {
        mcp::pi_name_ok(id, &existing.keys().cloned().collect::<Vec<_>>())?;
    }
    let native = |spec: &Value| mcp::to_native(t.format, spec, existing.get(id), windows);
    match t.format {
        Format::CodexToml | Format::GrokToml => {
            let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| {
                GateError::Other(format!("{} 不是合法的 TOML：{e}", t.path.display()))
            })?;
            mcp::toml_set(
                &mut doc,
                id,
                spec.map(|s| mcp::to_toml(t.format, s)).transpose()?,
            )?;
            Ok(doc.to_string())
        }
        Format::OpenCode => {
            crate::relay::coexist::jsonc_set(text, &["mcp", id], spec.map(native).transpose()?)
        }
        Format::Hermes => {
            let mut section = match crate::relay_live::patch::yaml::section(text, "mcp_servers")? {
                Some(v) => yaml_to_json(v)?,
                None => json!({}),
            };
            let obj = section.as_object_mut().ok_or_else(|| {
                GateError::Other(
                    "Hermes 的 mcp_servers 不是映射：为避免覆盖你的配置，没有写".into(),
                )
            })?;
            match spec {
                Some(s) => {
                    obj.insert(id.into(), native(s)?);
                }
                None => {
                    obj.shift_remove(id);
                }
            }
            let yaml: serde_yaml_ng::Value =
                serde_json::from_value(section).map_err(|e| GateError::Other(e.to_string()))?;
            crate::relay_live::patch::yaml::replace_section(text, "mcp_servers", &yaml)
        }
        _ => {
            let bytes = (!text.trim().is_empty()).then_some(text.as_bytes());
            let (mut doc, style) = crate::relay_live::patch::json::parse(&t.path, bytes)
                .map_err(|e| GateError::Other(e.to_string()))?;
            let root = doc.as_object_mut().expect("parse 只放行对象");
            let key = json_key(t.format);
            if root.get(key).is_some_and(|v| !v.is_object()) {
                return Err(GateError::Other(format!(
                    "{} 的 {key} 不是对象：为避免覆盖你的配置，没有写",
                    t.path.display()
                )));
            }
            let servers = root.entry(key).or_insert_with(|| json!({}));
            let servers = servers.as_object_mut().expect("上面查过");
            match spec {
                Some(s) => {
                    servers.insert(id.into(), native(s)?);
                }
                None => {
                    servers.shift_remove(id);
                }
            }
            let out = crate::relay_live::patch::json::serialize(&t.path, &doc, &style)
                .map_err(|e| GateError::Other(e.to_string()))?;
            String::from_utf8(out).map_err(|e| GateError::Other(e.to_string()))
        }
    }
}

// ------------------------------------------------------------------ 编排

fn view(place: &Place, db: &Repository) -> Result<McpView> {
    let targets = place
        .targets()
        .into_iter()
        .map(|t| {
            let (entries, problem) = match read(&t.path).and_then(|(_, text)| entries(&t, &text)) {
                Ok(e) => (e.into_keys().collect(), None),
                Err(e) => (Vec::new(), Some(e.to_string())),
            };
            McpTarget {
                id: t.id,
                label: t.label,
                path: t.path.display().to_string(),
                official: t.official,
                available: t.available,
                entries,
                problem,
            }
        })
        .collect();
    Ok(McpView {
        servers: servers(db)?,
        targets,
    })
}

/// 把一条服务器的目标改成 `next`：新勾的写上、取消的删掉、还勾着的按新定义重写。`spec` 为 `None` = 整条删掉。
fn sync(
    place: &Place,
    id: &str,
    spec: Option<&Value>,
    previous: &[String],
    next: &[String],
) -> Result<()> {
    let mut all: Vec<&String> = previous.iter().chain(next).collect();
    all.sort();
    all.dedup();
    let mut edits = Vec::new();
    let mut reapply = Vec::new();
    for tid in all {
        let t = place.target(tid)?;
        let want = spec.filter(|_| next.contains(tid));
        if want.is_some() && !t.available {
            return Err(GateError::Other(format!(
                "{} 没找到（{}）：多半还没装，不替它建",
                t.label,
                t.path.display()
            )));
        }
        let (old, text) = read(&t.path)?;
        let body = write(&t, &text, id, want, place.windows)?;
        if body == text {
            continue;
        }
        if let Some(dir) = t.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if let Some(c) = t.reapply {
            reapply.push(c);
        }
        edits.push(crate::config_io::Edit {
            path: t.path,
            expected: old,
            body: Some(body.into_bytes()),
        });
    }
    place.commit(edits)?;
    if place.reapply {
        for c in reapply {
            // 只重新应用已经建过的中转环境（没建过就没有指纹要对）。
            let db = Repository::open()?;
            if db
                .get::<crate::domain::Environment>(
                    "environments",
                    &crate::workspace::router_environment_id(c),
                )
                .is_ok()
            {
                crate::workspace::apply(&crate::workspace::router_environment_id(c))?;
            }
        }
    }
    Ok(())
}

/// 整份：服务器、目标。第一次打开时把原来扩展中心装的 MCP 迁进来（[`migrate_installations`]）。
pub fn list(db: &Repository) -> Result<McpView> {
    let place = Place::real(db)?;
    migrate_installations(&place, db)?;
    view(&place, db)
}

/// 新建或修改一条（`previous_id` 是改名前的名字）。目标按 `server.targets` 写。
pub fn save(db: &Repository, server: McpServer, previous_id: Option<&str>) -> Result<McpView> {
    let place = Place::real(db)?;
    save_at(&place, db, server, previous_id)?;
    crate::audit::write("MCP：保存了一条服务器并同步到勾选的目标");
    view(&place, db)
}

fn save_at(
    place: &Place,
    db: &Repository,
    mut server: McpServer,
    previous_id: Option<&str>,
) -> Result<()> {
    mcp::validate_id(&server.id)?;
    mcp::validate(&server.server)?;
    if server.name.trim().is_empty() {
        server.name = server.id.clone();
    }
    server.targets.sort();
    server.targets.dedup();
    let old_id = previous_id.unwrap_or(&server.id).to_string();
    let old: Option<McpServer> = db.find::<Stored>(TABLE, &old_id)?.map(open).transpose()?;
    if old_id != server.id && db.find::<Stored>(TABLE, &server.id)?.is_some() {
        return Err(GateError::Other(format!(
            "已经有一条叫 {} 的 MCP 服务器",
            server.id
        )));
    }
    if previous_id.is_none() && old.is_some() {
        return Err(GateError::Other(format!(
            "已经有一条叫 {} 的 MCP 服务器",
            server.id
        )));
    }
    let previous = old.as_ref().map(|o| o.targets.clone()).unwrap_or_default();
    if old_id != server.id {
        // 改了名：旧名字那几条先删掉。
        sync(place, &old_id, None, &previous, &[])?;
        sync(
            place,
            &server.id,
            Some(&server.server),
            &[],
            &server.targets,
        )?;
        db.remove(TABLE, &old_id)?;
    } else {
        sync(
            place,
            &server.id,
            Some(&server.server),
            &previous,
            &server.targets,
        )?;
    }
    db.put(TABLE, &server.id, &seal(&server)?)
}

/// 删掉一条：从勾着的目标里都删掉，再删库里那一行。
pub fn delete(db: &Repository, id: &str) -> Result<McpView> {
    let place = Place::real(db)?;
    delete_at(&place, db, id)?;
    crate::audit::write(&format!("MCP：删掉了服务器 {id}（从勾着的目标里一起删）"));
    view(&place, db)
}

fn delete_at(place: &Place, db: &Repository, id: &str) -> Result<()> {
    let Some(s) = db.find::<Stored>(TABLE, id)?.map(open).transpose()? else {
        return Ok(());
    };
    sync(place, id, None, &s.targets, &[])?;
    db.remove(TABLE, id)
}

/// 勾上 / 取消一个目标。
pub fn set_target(db: &Repository, id: &str, target: &str, on: bool) -> Result<McpView> {
    let place = Place::real(db)?;
    set_target_at(&place, db, id, target, on)?;
    view(&place, db)
}

fn set_target_at(place: &Place, db: &Repository, id: &str, target: &str, on: bool) -> Result<()> {
    let mut s = db
        .find::<Stored>(TABLE, id)?
        .map(open)
        .transpose()?
        .ok_or_else(|| GateError::Other(format!("没有叫 {id} 的 MCP 服务器")))?;
    let previous = s.targets.clone();
    s.targets.retain(|t| t != target);
    if on {
        s.targets.push(target.into());
        s.targets.sort();
    }
    sync(place, id, Some(&s.server), &previous, &s.targets)?;
    db.put(TABLE, id, &seal(&s)?)
}

/// 从一个目标把现有的 MCP 导进来：新名字新建一条（只勾这个目标），同名且连接一样的给它勾上这个目标；
/// 同名但连接不一样的跳过、报出来（不替使用者挑哪一份对）。不写任何文件。
pub fn import(db: &Repository, target: &str) -> Result<McpImportReport> {
    let place = Place::real(db)?;
    let (added, linked, skipped) = import_at(&place, db, target)?;
    Ok(McpImportReport {
        added,
        linked,
        skipped,
        view: view(&place, db)?,
    })
}

fn same(a: &Value, b: &Value) -> bool {
    let core = |v: &Value| {
        let mut o = v.as_object().cloned().unwrap_or_default();
        o.retain(|k, _| ["command", "args", "env", "cwd", "url", "headers"].contains(&k.as_str()));
        (mcp::transport(v).to_string(), o)
    };
    core(a) == core(b)
}

type Imported = (Vec<String>, Vec<String>, Vec<String>);

fn import_at(place: &Place, db: &Repository, target: &str) -> Result<Imported> {
    let t = place.target(target)?;
    let (_, text) = read(&t.path)?;
    let (mut added, mut linked, mut skipped) = (Vec::new(), Vec::new(), Vec::new());
    for (id, native) in entries(&t, &text)? {
        let spec = mcp::from_native(t.format, &native);
        let spec = match spec.and_then(|s| mcp::validate_id(&id).map(|_| s)) {
            Ok(s) => s,
            Err(e) => {
                skipped.push(format!("{id}：{e}"));
                continue;
            }
        };
        match db.find::<Stored>(TABLE, &id)?.map(open).transpose()? {
            Some(mut s) if same(&s.server, &spec) => {
                if !s.targets.iter().any(|x| x == target) {
                    s.targets.push(target.into());
                    s.targets.sort();
                    db.put(TABLE, &id, &seal(&s)?)?;
                    linked.push(id);
                }
            }
            Some(_) => skipped.push(format!(
                "{id}：面板里已经有同名的、连接方式不一样（没覆盖）"
            )),
            None => {
                let s = McpServer {
                    id: id.clone(),
                    name: id.clone(),
                    server: spec,
                    targets: vec![target.into()],
                    description: None,
                    homepage: None,
                    tags: Vec::new(),
                };
                db.put(TABLE, &id, &seal(&s)?)?;
                added.push(id);
            }
        }
    }
    Ok((added, linked, skipped))
}

/// 原来扩展中心「装」进中转环境 / 官方目录的 MCP（`installations` 表里 MCP 那几行）迁成新的记录：
/// 名字照旧（安装时用的扩展 id），连接定义从那份配置里读回来，目标按装在哪推出来。只迁一次；老表留着不删。
fn migrate_installations(place: &Place, db: &Repository) -> Result<()> {
    if db.meta(MIGRATED)?.as_deref() == Some("1") {
        return Ok(());
    }
    let catalog = crate::extensions::catalog_in(db).unwrap_or_default();
    for i in db
        .list::<ExtensionInstallation>("installations")
        .unwrap_or_default()
    {
        let is_mcp = i.extension_id.starts_with("mcp-")
            || catalog
                .iter()
                .any(|m| m.id == i.extension_id && m.kind == crate::domain::ExtensionKind::Mcp);
        if !is_mcp {
            continue;
        }
        let client = match i.client {
            Client::ClaudeCode => "claude-code",
            Client::Codex => "codex",
            _ => continue,
        };
        let target = match i.identity_kind {
            IdentityKind::Relay
                if i.environment_id == crate::workspace::router_environment_id(i.client) =>
            {
                format!("{client}:relay")
            }
            IdentityKind::Relay => continue, // 2026-09-27 之前手建的直连环境：那一页早就删了，不迁
            _ if i.environment_id.is_empty() => format!("{client}:default"),
            _ if i.client == Client::ClaudeCode => format!("claude-code:slot:{}", i.environment_id),
            _ => "codex:default".to_string(),
        };
        let Ok(t) = place.target(&target) else {
            continue;
        };
        let Ok((_, text)) = read(&t.path) else {
            continue;
        };
        let Some(native) = entries(&t, &text)
            .ok()
            .and_then(|e| e.get(&i.extension_id).cloned())
        else {
            continue;
        };
        let Ok(spec) = mcp::from_native(t.format, &native) else {
            continue;
        };
        let mut s = match db
            .find::<Stored>(TABLE, &i.extension_id)?
            .map(open)
            .transpose()?
        {
            Some(s) => s,
            None => McpServer {
                id: i.extension_id.clone(),
                name: catalog
                    .iter()
                    .find(|m| m.id == i.extension_id)
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|| i.extension_id.clone()),
                server: spec,
                targets: Vec::new(),
                description: None,
                homepage: None,
                tags: Vec::new(),
            },
        };
        if !s.targets.contains(&target) {
            s.targets.push(target);
            s.targets.sort();
        }
        db.put(TABLE, &s.id, &seal(&s)?)?;
    }
    db.set_meta(MIGRATED, "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(root: &Path) -> Place {
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
            journal: Some(root.join("journal")),
            windows: false,
            reapply: false,
        }
    }

    fn server(targets: &[&str]) -> McpServer {
        McpServer {
            id: "fs".into(),
            name: "文件系统".into(),
            server: json!({ "type": "stdio", "command": "npx", "args": ["-y", "server-fs"], "env": { "TOKEN": "fixture-secret" } }),
            targets: targets.iter().map(|s| s.to_string()).collect(),
            description: None,
            homepage: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn a_server_lands_only_in_the_ticked_targets_and_leaves_the_user_s_own_alone() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let p = place(&root);
        std::fs::create_dir_all(root.join("env-codex")).unwrap();
        std::fs::write(
            root.join("env-codex").join("config.toml"),
            "model = \"x\"\n\n[mcp_servers.mine]\ncommand = \"node\"\n",
        )
        .unwrap();

        save_at(&p, &db, server(&["claude-code:relay", "codex:relay"]), None).unwrap();
        let claude: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("env-claude").join(".claude.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(claude["mcpServers"]["fs"]["command"], "npx");
        let codex = std::fs::read_to_string(root.join("env-codex").join("config.toml")).unwrap();
        assert!(
            codex.contains("[mcp_servers.fs]") && codex.contains("[mcp_servers.mine]"),
            "{codex}"
        );
        assert!(
            !root.join("home").join(".claude.json").exists(),
            "官方默认目录没勾就不写"
        );

        // 库里加密存：表里看不到明文的 Key。
        let raw: Vec<Value> = db.list(TABLE).unwrap();
        assert!(!raw[0].to_string().contains("fixture-secret"), "{}", raw[0]);
        assert_eq!(
            servers(&db).unwrap()[0].server["env"]["TOKEN"],
            "fixture-secret"
        );

        // 取消 Codex：只删自己那一条。
        set_target_at(&p, &db, "fs", "codex:relay", false).unwrap();
        let codex = std::fs::read_to_string(root.join("env-codex").join("config.toml")).unwrap();
        assert!(
            !codex.contains("mcp_servers.fs") && codex.contains("[mcp_servers.mine]"),
            "{codex}"
        );

        // 没装的工具勾不上，也不替它建目录。
        let err = set_target_at(&p, &db, "fs", "opencode", true).unwrap_err();
        assert!(err.to_string().contains("没找到"), "{err}");
        assert!(!root.join("home").join(".config").exists());

        // 删掉：勾着的目标里一起删。
        delete_at(&p, &db, "fs").unwrap();
        let claude: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("env-claude").join(".claude.json")).unwrap(),
        )
        .unwrap();
        assert!(claude["mcpServers"].get("fs").is_none());
        assert!(servers(&db).unwrap().is_empty());

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn importing_links_identical_servers_and_reports_conflicts() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let p = place(&root);
        std::fs::create_dir_all(root.join("home").join(".config").join("opencode")).unwrap();
        std::fs::write(
            root.join("home").join(".config").join("opencode").join("opencode.jsonc"),
            "{\n  // 我的\n  \"mcp\": {\n    \"fs\": { \"type\": \"local\", \"command\": [\"npx\", \"-y\", \"server-fs\"], \"environment\": { \"TOKEN\": \"fixture-secret\" } },\n    \"other\": { \"type\": \"remote\", \"url\": \"https://x\" },\n    \"clash\": { \"type\": \"local\", \"command\": [\"python\"] }\n  }\n}\n",
        )
        .unwrap();
        save_at(&p, &db, server(&["claude-code:relay"]), None).unwrap();
        let mut clash = server(&["claude-code:relay"]);
        clash.id = "clash".into();
        save_at(&p, &db, clash, None).unwrap();

        let (added, linked, skipped) = import_at(&p, &db, "opencode").unwrap();
        assert_eq!(added, ["other"]);
        assert_eq!(linked, ["fs"], "连接一样的给它勾上 OpenCode");
        assert_eq!(skipped.len(), 1, "{skipped:?}");
        assert!(skipped[0].starts_with("clash"));
        let all = servers(&db).unwrap();
        let fs = all.iter().find(|s| s.id == "fs").unwrap();
        assert_eq!(fs.targets, ["claude-code:relay", "opencode"]);

        // 勾着 OpenCode 再改定义：OpenCode 那份按 JSONC 改，注释留着。
        let mut changed = fs.clone();
        changed.server["args"] = json!(["-y", "server-fs@2"]);
        save_at(&p, &db, changed, Some("fs")).unwrap();
        let text = std::fs::read_to_string(
            root.join("home")
                .join(".config")
                .join("opencode")
                .join("opencode.jsonc"),
        )
        .unwrap();
        assert!(
            text.contains("// 我的") && text.contains("server-fs@2"),
            "{text}"
        );

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn old_extension_center_installs_are_read_into_the_new_records() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let p = place(&root);
        std::fs::create_dir_all(root.join("slot-a")).unwrap();
        std::fs::write(
            root.join("slot-a").join(".claude.json"),
            r#"{"mcpServers":{"mcp-git":{"command":"uvx","args":["mcp-server-git"]}},"projects":{}}"#,
        )
        .unwrap();
        db.put(
            "installations",
            "i1",
            &ExtensionInstallation {
                id: "i1".into(),
                extension_id: "mcp-git".into(),
                environment_id: "a".into(),
                identity_kind: IdentityKind::Official,
                client: Client::ClaudeCode,
                version: "1".into(),
                state: "installed".into(),
                path: root.join("slot-a").display().to_string(),
                content_hash: String::new(),
                installed_at: String::new(),
            },
        )
        .unwrap();
        migrate_installations(&p, &db).unwrap();
        let all = servers(&db).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "mcp-git");
        assert_eq!(all[0].targets, ["claude-code:slot:a"]);
        assert_eq!(all[0].server["command"], "uvx");
        assert!(
            db.list::<ExtensionInstallation>("installations")
                .unwrap()
                .len()
                == 1,
            "老表留着"
        );
        // 只迁一次。
        db.remove(TABLE, "mcp-git").unwrap();
        migrate_installations(&p, &db).unwrap();
        assert!(servers(&db).unwrap().is_empty());

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
