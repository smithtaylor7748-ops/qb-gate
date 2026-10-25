//! 把一条线路写进共存型工具（OpenCode、MiniMax Code、OpenClaw、Hermes、Pi，中转站重构阶段 4，cc-switch 的 C3 / C4）。
//!
//! ⛔ **写的是真 Key**（使用者 2026-10-25 定的）：这五个工具不经本机路由，站点地址和 Key 明文写进它们自己的配置文件。
//! 只在点「写入」时写；只动 QB Gate 自己写的那一条（`qb-gate-…`）；工具的配置目录不在（多半没装）就不替它建。
//! 写盘走 `config_io::commit`：写前核对文件没被外部改过、日志里留着原内容，失败回滚。
//!
//! 形状与文件位置在 `relay::coexist`。

use crate::domain::Provider;
use crate::error::{GateError, Result};
use crate::relay::coexist::{self, CoexistEntry, CoexistModel, CoexistProtocol, CoexistTool};
use crate::repository::Repository;
use qb_proxy::convert::plan::ApiFormat;
use qb_station::station::route::Route;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// 一个工具里这条线的样子（线路弹窗里那张表的一行）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CoexistStatus {
    pub tool: CoexistTool,
    pub label: String,
    /// 配置文件在哪（界面照实显示：使用者有权知道改的是哪个文件）。
    pub path: String,
    /// 配置目录在不在。不在多半是没装：写入按钮灰掉。
    pub installed: bool,
    /// 这条线已经写进去了。
    pub written: bool,
    /// 读这份配置出的问题（解析不了之类）。有问题时写入也会拒绝。
    pub problem: Option<String>,
}

/// 在哪找工具的配置、在哪记写盘日志。平时是真用户目录；单测换成临时目录（单测不碰真实状态）。
struct Place {
    home: PathBuf,
    env: fn(&str) -> Option<String>,
    /// 写盘日志放哪。`None` = `config_io::commit` 自己的那个目录。
    journal: Option<PathBuf>,
}

impl Place {
    fn real() -> Result<Self> {
        Ok(Place {
            home: dirs::home_dir().ok_or_else(|| GateError::Other("找不到用户目录".into()))?,
            env: |k| std::env::var(k).ok(),
            journal: None,
        })
    }

    fn config_dir(&self, tool: CoexistTool) -> PathBuf {
        tool.config_dir(&self.home, self.env)
    }

    /// 候选里先有的那个算数；都没有就用第一个。
    fn config_path(&self, tool: CoexistTool) -> PathBuf {
        let candidates = tool.config_candidates(&self.home, self.env);
        candidates
            .iter()
            .find(|p| p.is_file())
            .cloned()
            .unwrap_or_else(|| candidates[0].clone())
    }

    fn commit(&self, path: PathBuf, expected: Option<Vec<u8>>, body: String) -> Result<()> {
        let edits = vec![crate::config_io::Edit {
            path,
            expected,
            body: Some(body.into_bytes()),
        }];
        match &self.journal {
            Some(dir) => crate::config_io::commit_in(dir, edits),
            None => crate::config_io::commit(edits),
        }
        .map(drop)
    }
}

fn read_text(path: &Path) -> Result<(Option<Vec<u8>>, String)> {
    let bytes = crate::config_io::read_optional(path)?;
    let text = String::from_utf8(bytes.clone().unwrap_or_default())
        .map_err(|_| GateError::Other(format!("{} 不是 UTF-8 文本", path.display())))?;
    Ok((bytes, text))
}

/// 线路的上游协议换成共存型工具认的那三种。Gemini 原生这五个工具都不收。
fn protocol(route: &Route) -> Result<CoexistProtocol> {
    match crate::local_router::speaks(route) {
        ApiFormat::Anthropic => Ok(CoexistProtocol::Anthropic),
        ApiFormat::OpenaiChat => Ok(CoexistProtocol::OpenaiChat),
        ApiFormat::OpenaiResponses => Ok(CoexistProtocol::OpenaiResponses),
        ApiFormat::GeminiNative => Err(GateError::Other(
            "这条线说的是 Gemini 原生协议：这几个工具都不收它".into(),
        )),
    }
}

/// 线路模型映射里的目标模型（逐个指定的在前，再按档位），去重。
fn models(route: &Route) -> Vec<CoexistModel> {
    let m = &route.model_map;
    let mut out: Vec<CoexistModel> = Vec::new();
    for id in m.exact.iter().map(|p| p.to.clone()).chain(
        [
            &m.default,
            &m.opus,
            &m.sonnet,
            &m.haiku,
            &m.fable,
            &m.subagent,
        ]
        .into_iter()
        .flatten()
        .cloned(),
    ) {
        let id = id.trim().to_string();
        if !id.is_empty() && !out.iter().any(|x| x.id == id) {
            out.push(CoexistModel { id, name: None });
        }
    }
    out
}

fn entry(db: &Repository, route: &Route) -> Result<CoexistEntry> {
    let source = super::station_ops::source(db, route)?;
    let provider: Provider = db.get("providers", &route.station_id)?;
    let group = if route.group.is_empty() {
        "默认分组"
    } else {
        route.group.as_str()
    };
    Ok(CoexistEntry {
        id: coexist::entry_id(&provider.name, &route.group, &route.id),
        name: format!("{} · {group}（QB Gate）", provider.name),
        base_url: source.base_url,
        // ⛔ 真 Key：共存型工具不经本机路由（模块头）。
        api_key: source.key,
        protocol: protocol(route)?,
        models: models(route),
    })
}

fn status_of(place: &Place, tool: CoexistTool, id: &str) -> CoexistStatus {
    let path = place.config_path(tool);
    let installed = place.config_dir(tool).is_dir();
    let (written, problem) = match read_text(&path) {
        Ok((_, text)) => match coexist::written_ids(tool, &text) {
            Ok(ids) => (ids.iter().any(|x| x == id), None),
            Err(e) => (false, Some(e.to_string())),
        },
        Err(e) => (false, Some(e.to_string())),
    };
    CoexistStatus {
        tool,
        label: tool.label().to_string(),
        path: path.display().to_string(),
        installed,
        written,
        problem,
    }
}

fn route_of(db: &Repository, route_id: &str) -> Result<Route> {
    db.get("station_routes", route_id)
        .map_err(|_| GateError::Other("这条线路不存在".into()))
}

fn entry_id_of(db: &Repository, route: &Route) -> Result<String> {
    let provider: Provider = db.get("providers", &route.station_id)?;
    Ok(coexist::entry_id(&provider.name, &route.group, &route.id))
}

/// 这条线在五个工具里各是什么样子。只读。
pub fn status(db: &Repository, route_id: &str) -> Result<Vec<CoexistStatus>> {
    status_at(&Place::real()?, db, route_id)
}

fn status_at(place: &Place, db: &Repository, route_id: &str) -> Result<Vec<CoexistStatus>> {
    let id = entry_id_of(db, &route_of(db, route_id)?)?;
    Ok(CoexistTool::ALL
        .iter()
        .map(|&t| status_of(place, t, &id))
        .collect())
}

/// 把这条线写进（或更新）这个工具。⛔ 写的是真 Key。
pub fn write(db: &Repository, route_id: &str, tool: CoexistTool) -> Result<CoexistStatus> {
    let s = write_at(&Place::real()?, db, route_id, tool)?;
    crate::audit::write(&format!(
        "已把线路 {route_id} 写进 {}（{}，含 API Key）",
        tool.label(),
        s.path
    ));
    Ok(s)
}

fn write_at(
    place: &Place,
    db: &Repository,
    route_id: &str,
    tool: CoexistTool,
) -> Result<CoexistStatus> {
    let route = route_of(db, route_id)?;
    let e = entry(db, &route)?;
    let dir = place.config_dir(tool);
    if !dir.is_dir() {
        return Err(GateError::Other(format!(
            "没找到 {} 的配置目录（{}）：多半还没装。装好、打开过一次之后再写",
            tool.label(),
            dir.display()
        )));
    }
    let _lock = crate::relay_live::engine::lock_app(tool.label());
    let path = place.config_path(tool);
    let (old, text) = read_text(&path)?;
    let next = coexist::upsert(tool, &text, &e)?;
    place.commit(path, old, next)?;
    Ok(status_of(place, tool, &e.id))
}

/// 把这条线从这个工具里删掉（只删 QB Gate 写的那一条）。
pub fn remove(db: &Repository, route_id: &str, tool: CoexistTool) -> Result<CoexistStatus> {
    let (s, changed) = remove_at(&Place::real()?, db, route_id, tool)?;
    if changed {
        crate::audit::write(&format!(
            "已把线路 {route_id} 从 {} 里删掉（{}）",
            tool.label(),
            s.path
        ));
    }
    Ok(s)
}

fn remove_at(
    place: &Place,
    db: &Repository,
    route_id: &str,
    tool: CoexistTool,
) -> Result<(CoexistStatus, bool)> {
    let id = entry_id_of(db, &route_of(db, route_id)?)?;
    let _lock = crate::relay_live::engine::lock_app(tool.label());
    let path = place.config_path(tool);
    let (old, text) = read_text(&path)?;
    let next = coexist::remove(tool, &text, &id)?;
    let changed = next != text;
    if changed {
        place.commit(path, old, next)?;
    }
    Ok((status_of(place, tool, &id), changed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use qb_contract::domain::Client;
    use qb_station::station::route::{ModelPair, RouteModelMap, UpstreamFormat};

    #[test]
    fn the_line_s_protocol_and_mapping_become_the_entry_s() {
        let mut r = Route::new(Client::ClaudeCode, "s", "g");
        assert_eq!(protocol(&r).unwrap(), CoexistProtocol::Anthropic);
        r.client = Client::Codex;
        assert_eq!(protocol(&r).unwrap(), CoexistProtocol::OpenaiResponses);
        r.upstream_format = Some(UpstreamFormat::OpenaiChat);
        assert_eq!(protocol(&r).unwrap(), CoexistProtocol::OpenaiChat);
        r.upstream_format = Some(UpstreamFormat::GeminiNative);
        assert!(protocol(&r).is_err(), "Gemini 原生这几个工具都不收");

        r.model_map = RouteModelMap {
            exact: vec![ModelPair {
                from: "a".into(),
                to: "glm-5".into(),
            }],
            default: Some("glm-4.7".into()),
            opus: Some("glm-5".into()),
            sonnet: Some("  ".into()),
            ..Default::default()
        };
        let ids: Vec<String> = models(&r).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, ["glm-5", "glm-4.7"], "逐个指定的在前、去重、空的不要");
    }

    #[test]
    fn writes_only_where_the_tool_is_installed_and_removes_only_its_own_entry() {
        use crate::domain::Credential;
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let provider = Provider {
            id: "site".into(),
            name: "Fixture".into(),
            base_url: "https://fixture.invalid/v1".into(),
            website: String::new(),
            note: String::new(),
            tags: vec![],
            favorite: false,
            revision: 1,
            topup_per_usd: None,
        };
        db.put("providers", "site", &provider).unwrap();
        let credential = Credential {
            id: "k".into(),
            provider_id: "site".into(),
            label: "k".into(),
            masked: "***".into(),
            available: true,
            revision: 1,
        };
        db.set_credential(&credential, &crate::secret::seal("fixture-key").unwrap())
            .unwrap();
        let mut route = Route::new(Client::ClaudeCode, "site", "g");
        route.credential_id = Some("k".into());
        // 没有模型的线写不进去（这几个工具要一份模型清单）。
        route.model_map.default = Some("glm-5".into());
        db.put("station_routes", &route.id, &route).unwrap();
        let place = Place {
            home: root.join("home"),
            env: |_| None,
            journal: Some(root.join("journal")),
        };

        // 没装（配置目录不在）就拒绝，也不替它建。
        let err = write_at(&place, &db, &route.id, CoexistTool::OpenCode).unwrap_err();
        assert!(err.to_string().contains("没找到"), "{err}");
        assert!(!place.config_dir(CoexistTool::OpenCode).exists());

        let dir = place.config_dir(CoexistTool::OpenCode);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("opencode.jsonc");
        std::fs::write(
            &file,
            "{
  // 使用者自己的注释
  \"provider\": { \"mine\": { \"npm\": \"x\" } }
}
",
        )
        .unwrap();
        let s = write_at(&place, &db, &route.id, CoexistTool::OpenCode).unwrap();
        assert!(s.written && s.installed && s.problem.is_none(), "{s:?}");
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            text.contains("fixture-key"),
            "共存型工具写的是真 Key：
{text}"
        );
        assert!(
            text.contains("// 使用者自己的注释") && text.contains("\"mine\""),
            "{text}"
        );
        let all = status_at(&place, &db, &route.id).unwrap();
        assert_eq!(
            all.iter().filter(|x| x.written).count(),
            1,
            "只写了 OpenCode：{all:?}"
        );

        let (s, changed) = remove_at(&place, &db, &route.id, CoexistTool::OpenCode).unwrap();
        assert!(changed && !s.written);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            !text.contains("fixture-key") && !text.contains(coexist::ID_PREFIX),
            "{text}"
        );
        assert!(
            text.contains("// 使用者自己的注释") && text.contains("\"mine\""),
            "{text}"
        );
        let (_, again) = remove_at(&place, &db, &route.id, CoexistTool::OpenCode).unwrap();
        assert!(!again, "没有自己那一条时不该再写盘");

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
