// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 项目（中转站重构阶段 5，cc-switch 的 M4 Profile）：把中转那一侧的一整套设置存成一个名字，一键换过去。
//!
//! 一个项目按软件分组（cc-switch 的 `ProfileScope`）存快照：这个软件选的线路、派单顺序、中转环境里勾着的 MCP / Skills、
//! 中转环境用的提示词。换项目（[`apply`]）时先把当前项目的这一组存一下，再把目标项目那一组落下去 —— 尽力而为，
//! 单项失败记成提醒、不整体回滚（cc-switch 同样）。
//!
//! ⛔ **只碰中转那一侧，永远不碰官方账户 / 槽位的切换**（计划 §7 阶段 5）：MCP / Skills / 提示词只动 `<软件>:relay`
//! 那个目标（Grok Build 是它唯一的那份）；线路和派单顺序本来就只是中转的。`architecture.rs` 有一条测试钉着
//! 这个模块碰不到账户切换（`project_module_can_never_reach_account_switching`）。

use super::ext_place::Place;
use crate::domain::Client;
use crate::error::{GateError, Result};
use crate::repository::Repository;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

const TABLE: &str = "projects";

/// 一个软件那一组的快照。`None` = 这一项没拍过（换过去时不动），跟「拍到的就是空的」（`Some(空)`，换过去时清空）分开。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct ProjectSlot {
    pub route: Option<String>,
    pub queue: Option<Vec<String>>,
    pub mcp: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Project {
    pub id: String,
    pub name: String,
    /// 软件（`claude-code`、`codex`……）→ 那一组的快照。
    pub slots: BTreeMap<String, ProjectSlot>,
    #[ts(type = "number")]
    pub updated_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectsView {
    pub projects: Vec<Project>,
    /// 软件 → 现在是哪个项目。
    pub current: BTreeMap<String, String>,
}

/// 换项目 / 不使用项目之后给界面的：项目列表，加上落的时候出的问题（尽力而为，单项失败不整体回滚）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectChange {
    pub view: ProjectsView,
    pub warnings: Vec<String>,
}

/// 换项目要由命令那一层落下的两样（线路、派单顺序要动本机路由的状态），加上已经落下的那几样里出的问题。
#[derive(Debug, Clone, Default)]
pub struct Applied {
    pub route: Option<String>,
    pub queue: Option<Vec<String>>,
    pub warnings: Vec<String>,
}

/// 线路、派单顺序的现状（命令那一层从本机路由的状态里拿了交进来）。
#[derive(Debug, Clone, Default)]
pub struct RouterSnapshot {
    pub route: Option<String>,
    pub queue: Vec<String>,
}

/// 能存项目的软件：经本机路由的那几个（桌面端只有线路和派单顺序）。
pub const SCOPES: [Client; 5] = [
    Client::ClaudeCode,
    Client::ClaudeDesktop,
    Client::Codex,
    Client::GeminiCli,
    Client::GrokBuild,
];

pub fn scope_name(c: Client) -> &'static str {
    match c {
        Client::ClaudeCode => "claude-code",
        Client::ClaudeDesktop => "claude-desktop",
        Client::Codex => "codex",
        Client::GeminiCli => "gemini-cli",
        Client::GrokBuild => "grok-build",
        Client::Antigravity | Client::AntigravityIde => "antigravity",
    }
}

/// 这个软件在 MCP / Skills / 提示词里对应的那个目标：只有中转环境那一份。桌面端没有。
fn relay_target(c: Client) -> Option<&'static str> {
    match c {
        Client::ClaudeCode => Some("claude-code:relay"),
        Client::Codex => Some("codex:relay"),
        Client::GeminiCli => Some("gemini-cli:relay"),
        Client::GrokBuild => Some("grok-build"),
        _ => None,
    }
}

fn current_key(c: Client) -> String {
    format!("project-current:{}", scope_name(c))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn view(db: &Repository) -> Result<ProjectsView> {
    let mut projects: Vec<Project> = db.list(TABLE)?;
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    let current = SCOPES
        .iter()
        .filter_map(|&c| {
            let id = db
                .meta(&current_key(c))
                .ok()
                .flatten()
                .filter(|s| !s.is_empty())?;
            Some((scope_name(c).to_string(), id))
        })
        .collect();
    Ok(ProjectsView { projects, current })
}

pub fn list(db: &Repository) -> Result<ProjectsView> {
    view(db)
}

/// 这个软件现在的样子。
fn snapshot(db: &Repository, c: Client, router: &RouterSnapshot) -> Result<ProjectSlot> {
    let mut slot = ProjectSlot {
        route: router.route.clone(),
        queue: Some(router.queue.clone()),
        ..Default::default()
    };
    if let Some(t) = relay_target(c) {
        slot.mcp = Some(
            super::mcp_ops::servers(db)?
                .into_iter()
                .filter(|s| s.targets.iter().any(|x| x == t))
                .map(|s| s.id)
                .collect(),
        );
        slot.skills = Some(
            super::skill_ops::records(db)?
                .into_iter()
                .filter(|s| s.targets.iter().any(|x| x == t))
                .map(|s| s.id)
                .collect(),
        );
        slot.prompt = super::prompt_ops::active(db, t);
    }
    Ok(slot)
}

/// 新建一个项目：拍下这个软件现在的样子，并把它记成这个软件的当前项目。
pub fn create(
    db: &Repository,
    name: &str,
    scope: Client,
    router: &RouterSnapshot,
) -> Result<ProjectsView> {
    let name = name.trim();
    if name.is_empty() {
        return Err(GateError::Other("给项目起个名字".into()));
    }
    if !SCOPES.contains(&scope) {
        return Err(GateError::Other("这个软件不经本机路由，没有项目".into()));
    }
    let p = Project {
        id: format!("project-{}", crate::config_io::id()),
        name: name.into(),
        slots: BTreeMap::from([(scope_name(scope).to_string(), snapshot(db, scope, router)?)]),
        updated_ms: now_ms(),
    };
    db.put(TABLE, &p.id, &p)?;
    db.set_meta(&current_key(scope), &p.id)?;
    view(db)
}

/// 改名，和 / 或按现在的样子更新这个软件那一组（别的软件那几组原样留着）。
pub fn update(
    db: &Repository,
    id: &str,
    name: Option<&str>,
    resnap: Option<(Client, &RouterSnapshot)>,
) -> Result<ProjectsView> {
    let mut p: Project = db
        .find(TABLE, id)?
        .ok_or_else(|| GateError::Other("没有这个项目".into()))?;
    if let Some(n) = name.map(str::trim).filter(|n| !n.is_empty()) {
        p.name = n.into();
    }
    if let Some((c, router)) = resnap {
        p.slots
            .insert(scope_name(c).to_string(), snapshot(db, c, router)?);
    }
    p.updated_ms = now_ms();
    db.put(TABLE, id, &p)?;
    view(db)
}

/// 删掉一个项目（只删这条记录，不动任何配置）。
pub fn delete(db: &Repository, id: &str) -> Result<ProjectsView> {
    db.remove(TABLE, id)?;
    for c in SCOPES {
        if db.meta(&current_key(c))?.as_deref() == Some(id) {
            db.set_meta(&current_key(c), "")?;
        }
    }
    view(db)
}

/// 「不使用项目」：先把当前项目这一组按现在的样子存一下，再清掉这个软件的当前项目，什么配置都不动。
///
/// cc-switch 只清指针、不存 —— 在项目里改过的设置一清就没了。这里跟换项目一样「换走时自动存下」
/// （`apply_at` 给「没存过这一组」的提醒就是这么写的）。
pub fn clear_current(
    db: &Repository,
    scope: Client,
    router: &RouterSnapshot,
) -> Result<ProjectChange> {
    if !SCOPES.contains(&scope) {
        return Err(GateError::Other("这个软件不经本机路由，没有项目".into()));
    }
    let mut warnings = Vec::new();
    if let Some(cur) = db.meta(&current_key(scope))?.filter(|s| !s.is_empty()) {
        if let Err(e) = update(db, &cur, None, Some((scope, router))) {
            warnings.push(format!("清掉之前没存下当前项目这一组：{e}"));
        }
    }
    db.set_meta(&current_key(scope), "")?;
    Ok(ProjectChange {
        view: view(db)?,
        warnings,
    })
}

/// 换到这个项目（只这个软件那一组）：先把当前项目的这一组存一下，再落 MCP / Skills / 提示词；
/// 线路和派单顺序交回命令那一层落（它们在本机路由的状态里）。
pub fn apply(db: &Repository, id: &str, scope: Client, router: &RouterSnapshot) -> Result<Applied> {
    let place = Place::real(db)?;
    apply_at(&place, db, id, scope, router)
}

fn apply_at(
    place: &Place,
    db: &Repository,
    id: &str,
    scope: Client,
    router: &RouterSnapshot,
) -> Result<Applied> {
    if !SCOPES.contains(&scope) {
        return Err(GateError::Other("这个软件不经本机路由，没有项目".into()));
    }
    let mut out = Applied::default();
    if let Some(cur) = db
        .meta(&current_key(scope))?
        .filter(|s| !s.is_empty() && s != id)
    {
        if let Err(e) = update(db, &cur, None, Some((scope, router))) {
            out.warnings
                .push(format!("换之前没存下当前项目这一组：{e}"));
        }
    }
    let p: Project = db
        .find(TABLE, id)?
        .ok_or_else(|| GateError::Other("没有这个项目".into()))?;
    let Some(slot) = p.slots.get(scope_name(scope)).cloned() else {
        out.warnings.push(
            "这个项目还没存过这个软件的设置：记成当前项目，什么都没动（换走时会自动存下）".into(),
        );
        db.set_meta(&current_key(scope), id)?;
        return Ok(out);
    };
    if let Some(t) = relay_target(scope) {
        if let Some(want) = &slot.mcp {
            for s in super::mcp_ops::servers(db)? {
                let on = s.targets.iter().any(|x| x == t);
                if on != want.contains(&s.id) {
                    if let Err(e) = super::mcp_ops::set_target_at(place, db, &s.id, t, !on) {
                        out.warnings.push(format!("MCP {}：{e}", s.id));
                    }
                }
            }
            for gone in want.iter().filter(|w| {
                !super::mcp_ops::servers(db)
                    .unwrap_or_default()
                    .iter()
                    .any(|s| &s.id == *w)
            }) {
                out.warnings.push(format!("MCP {gone} 已经删了，跳过"));
            }
        }
        if let Some(want) = &slot.skills {
            for s in super::skill_ops::records(db)? {
                let on = s.targets.iter().any(|x| x == t);
                if on != want.contains(&s.id) {
                    if let Err(e) = super::skill_ops::set_target_at(place, db, &s.id, t, !on) {
                        out.warnings.push(format!("Skill {}：{e}", s.id));
                    }
                }
            }
        }
        if let Some(pid) = &slot.prompt {
            if super::prompt_ops::active(db, t).as_deref() != Some(pid.as_str()) {
                if let Err(e) = super::prompt_ops::enable_at(place, db, t, pid) {
                    out.warnings.push(format!("提示词：{e}"));
                }
            }
        }
    }
    out.route = slot.route;
    out.queue = slot.queue;
    db.set_meta(&current_key(scope), id)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::McpServer;
    use serde_json::json;

    #[test]
    fn switching_projects_saves_the_old_one_and_lands_the_new_one_on_the_relay_side_only() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let p = Place::under(&root);
        // 一条 MCP 勾在 Claude Code 中转环境。
        let fs = McpServer {
            id: "fs".into(),
            name: "fs".into(),
            server: json!({ "type": "stdio", "command": "npx" }),
            targets: Vec::new(),
            description: None,
            homepage: None,
            tags: Vec::new(),
        };
        db.put(
            "mcp_servers",
            "fs",
            &json!({
                "id": "fs", "name": "fs",
                "sealed": crate::secret::seal(&serde_json::to_string(&fs.server).unwrap()).unwrap(),
                "targets": [], "description": null, "homepage": null, "tags": []
            }),
        )
        .unwrap();
        super::super::mcp_ops::set_target_at(&p, &db, "fs", "claude-code:relay", true).unwrap();

        let router_a = RouterSnapshot {
            route: Some("route-a".into()),
            queue: vec!["route-a".into()],
        };
        let v = create(&db, "项目 A", Client::ClaudeCode, &router_a).unwrap();
        let a = v.projects[0].id.clone();
        assert_eq!(
            v.projects[0].slots["claude-code"].mcp,
            Some(vec!["fs".into()])
        );

        // 取消 MCP，存成项目 B。
        super::super::mcp_ops::set_target_at(&p, &db, "fs", "claude-code:relay", false).unwrap();
        let router_b = RouterSnapshot {
            route: Some("route-b".into()),
            queue: vec![],
        };
        let v = create(&db, "项目 B", Client::ClaudeCode, &router_b).unwrap();
        let b = v
            .projects
            .iter()
            .find(|x| x.name == "项目 B")
            .unwrap()
            .id
            .clone();

        // 换回 A：MCP 勾回来、交出 A 的线路；B 先按现在的样子存了一下。
        let applied = apply_at(&p, &db, &a, Client::ClaudeCode, &router_b).unwrap();
        assert_eq!(applied.route.as_deref(), Some("route-a"));
        assert!(applied.warnings.is_empty(), "{:?}", applied.warnings);
        let fs_now = super::super::mcp_ops::servers(&db).unwrap();
        assert_eq!(fs_now[0].targets, ["claude-code:relay"]);
        assert_eq!(view(&db).unwrap().current["claude-code"], a);
        let b_now: Project = db.find(TABLE, &b).unwrap().unwrap();
        assert_eq!(b_now.slots["claude-code"].route.as_deref(), Some("route-b"));

        // 只在中转环境那一份：官方默认目录一个文件都没有。
        assert!(!root.join("home").join(".claude.json").exists());

        // 没存过 Codex 那一组：换过去什么都不动，只提醒。
        let applied = apply_at(&p, &db, &b, Client::Codex, &RouterSnapshot::default()).unwrap();
        assert_eq!(applied.route, None);
        assert_eq!(applied.warnings.len(), 1);

        delete(&db, &a).unwrap();
        assert!(
            !view(&db).unwrap().current.contains_key("claude-code"),
            "删了当前项目，当前那一项清掉"
        );

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn not_using_a_project_saves_it_first_and_touches_no_configuration() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let router = RouterSnapshot {
            route: Some("route-a".into()),
            queue: vec!["route-a".into()],
        };
        let v = create(&db, "项目 A", Client::Codex, &router).unwrap();
        let a = v.projects[0].id.clone();
        assert_eq!(v.current["codex"], a);

        // 在项目里换了线路，然后「不使用项目」：换之前的样子先存回 A，当前项目清掉。
        let moved = RouterSnapshot {
            route: Some("route-b".into()),
            queue: vec!["route-b".into(), "route-a".into()],
        };
        let changed = clear_current(&db, Client::Codex, &moved).unwrap();
        assert!(changed.warnings.is_empty(), "{:?}", changed.warnings);
        assert!(!changed.view.current.contains_key("codex"));
        let a_now: Project = db.find(TABLE, &a).unwrap().unwrap();
        assert_eq!(a_now.slots["codex"].route.as_deref(), Some("route-b"));
        assert_eq!(
            a_now.slots["codex"].queue,
            Some(vec!["route-b".to_string(), "route-a".to_string()])
        );
        // 没有当前项目时再点一次：什么都不存、不报错。
        let again = clear_current(&db, Client::Codex, &RouterSnapshot::default()).unwrap();
        assert!(again.warnings.is_empty());

        // 不经本机路由的软件（反重力）没有项目：建、换、清都拒绝。
        assert!(create(&db, "x", Client::Antigravity, &router).is_err());
        assert!(apply_at(&Place::under(&root), &db, &a, Client::Antigravity, &router).is_err());
        assert!(clear_current(&db, Client::Antigravity, &router).is_err());

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
