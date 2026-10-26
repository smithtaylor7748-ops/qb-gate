// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 导入链接的编排（中转站重构阶段 5，cc-switch 的 X4 Deep Link）：待处理的那一条、给界面看的那一份、确认之后的导入。
//!
//! 解析在 `qb_extensions::deeplink`。这里管三件事：
//! - **待处理的那一条**（[`Inbox`]）：冷启动时链接比界面先到，先放这里，界面挂上来再取（照 cc-switch 的
//!   `PendingDeepLink`：读不清除，界面显示之后按编号确认清除；只留最新一条）。⛔ 只在内存里 —— 站点链接带着 Key，不落盘
//!   （迁移 9 建的 `deeplink_pending` 表因此没用上）。
//! - **给界面看的**（[`DeepLinkView`]）：⛔ Key 只给打码的（命令层的规矩：回给前端的形状里装不下 Key）。
//! - **确认之后导入**（[`Inbox::import`]）：站点 → 线路（站点按地址接着用已有的、Key 进凭证加密存）；
//!   MCP → 列表（一个目标都不勾）；提示词 → 库（不启用）。Skill 不在这里装：界面带着来源去 Skills 分页扫描、挑、再装，
//!   跟手动装一样。⛔ **什么都不自动启用**：不切线路、不写任何客户端配置（计划 §7 阶段 5「导入前一律弹确认、不自动启用」）。

use super::ext_place::Place;
use crate::deeplink::{self as link, DeepLink, LinkApp, LinkFormat, ProviderLink};
use crate::domain::{Client, Credential, Provider};
use crate::error::{GateError, Result};
use crate::mcp::McpServer;
use crate::repository::Repository;
use qb_station::station::route::{Route, RouteModelMap, UpstreamFormat};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use ts_rs::TS;

/// 给界面看的站点链接。⛔ 没有能装下 Key 的字段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderLinkView {
    pub client: Client,
    pub name: String,
    /// 第一个是主地址，其余导入成备用地址。
    pub endpoints: Vec<String>,
    /// 打码的 Key；`None` = 链接里没带（导入时使用者自己填）。
    pub key_masked: Option<String>,
    pub homepage: Option<String>,
    pub notes: Option<String>,
    /// 导入后线路上的模型映射（主模型进「兜底」，各档进各档，`[1M]` 剥掉 —— 跟预设一样）。
    pub model_map: RouteModelMap,
    /// 链接声明、而且跟这个软件的原生协议不一样时才有：导入后线路的「上游协议」。
    pub upstream_format: Option<UpstreamFormat>,
    /// 同一个地址已经有的站点名：导入时接着用它，不改它。
    pub existing_station: Option<String>,
    /// 认出来但没照做的部分。
    pub ignored: Vec<String>,
}

/// MCP 链接里的一条服务器。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct McpLinkServer {
    pub id: String,
    /// 一句话：`stdio · uvx mcp-server-fetch` / `http · https://…`。
    pub summary: String,
    /// 整条定义（JSON，缩进过）：启用之后它会被客户端执行，确认框里照实给看。
    pub spec: String,
    /// 面板里已经有同名的（导入时跳过、不动它）。
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct McpLinkView {
    pub apps: Vec<String>,
    pub servers: Vec<McpLinkServer>,
    pub ignored: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PromptLinkView {
    pub app: String,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub ignored: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkillLinkView {
    pub repo: String,
    /// Skills 面板认的来源地址。
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DeepLinkView {
    Provider(Box<ProviderLinkView>),
    Mcp(McpLinkView),
    Prompt(PromptLinkView),
    Skill(SkillLinkView),
    /// 解析不了：照实说为什么。
    Error {
        message: String,
    },
}

/// 待处理的一条，带编号（确认清除、界面去重都按它）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeepLinkEntry {
    #[ts(type = "number")]
    pub id: u64,
    /// 链接的协议名（`qbgate` / `ccswitch`），界面上说清楚它是哪来的。
    pub scheme: String,
    pub view: DeepLinkView,
}

/// 确认框里使用者填的。
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeepLinkChoice {
    /// 站点：导入成哪个分组（空 = 默认分组）。
    #[serde(default)]
    pub group: String,
    /// 站点：链接没带 Key 时使用者自己填的（跟线路弹窗一样，只从界面进后端、不回界面）。
    #[serde(default)]
    pub key: Option<String>,
}

/// 导入完了。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeepLinkOutcome {
    pub message: String,
    /// 站点：新线路属于哪个软件、id 是什么（界面跳到中转站页那个分页）。
    pub client: Option<Client>,
    pub route_id: Option<String>,
    /// MCP：跳过了哪几条（同名已有）。
    pub skipped: Vec<String>,
}

struct Pending {
    id: u64,
    scheme: String,
    link: std::result::Result<DeepLink, String>,
}

/// 待处理的导入链接：只在内存里，只留最新一条。
#[derive(Default)]
pub struct Inbox {
    slot: Mutex<Option<Pending>>,
    next: AtomicU64,
}

impl Inbox {
    fn slot(&self) -> std::sync::MutexGuard<'_, Option<Pending>> {
        self.slot.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// 收下一条链接（解析不了也收：界面要照实说为什么），盖掉还没处理的旧的。回编号。
    pub fn put(&self, raw: &str) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let scheme = raw
            .trim()
            .split_once("://")
            .map(|(s, _)| s.to_ascii_lowercase())
            .unwrap_or_default();
        let link = link::parse(raw).map_err(|e| e.to_string());
        *self.slot() = Some(Pending { id, scheme, link });
        id
    }

    /// 读现在待处理的那一条，不清除。
    pub fn peek(&self, db: &Repository) -> Option<DeepLinkEntry> {
        let slot = self.slot();
        let p = slot.as_ref()?;
        let view = match &p.link {
            Ok(l) => view(db, l).unwrap_or_else(|e| DeepLinkView::Error {
                message: e.to_string(),
            }),
            Err(message) => DeepLinkView::Error {
                message: message.clone(),
            },
        };
        Some(DeepLinkEntry {
            id: p.id,
            scheme: p.scheme.clone(),
            view,
        })
    }

    /// 界面显示过了（或者使用者点了「不导入」）：编号对得上才清，期间来了新链接就不动。
    pub fn ack(&self, id: u64) {
        let mut slot = self.slot();
        if slot.as_ref().is_some_and(|p| p.id == id) {
            *slot = None;
        }
    }

    /// 使用者确认导入。导入成了就清掉这一条。
    pub fn import(
        &self,
        db: &Repository,
        id: u64,
        choice: &DeepLinkChoice,
    ) -> Result<DeepLinkOutcome> {
        let place = Place::real(db)?;
        self.import_at(&place, db, id, choice)
    }

    fn import_at(
        &self,
        place: &Place,
        db: &Repository,
        id: u64,
        choice: &DeepLinkChoice,
    ) -> Result<DeepLinkOutcome> {
        let link = {
            let slot = self.slot();
            match slot.as_ref() {
                Some(p) if p.id == id => p.link.clone().map_err(GateError::Other)?,
                _ => {
                    return Err(GateError::Other(
                        "这条链接已经处理过了，或者来了一条新的".into(),
                    ))
                }
            }
        };
        let outcome = match link {
            DeepLink::Provider(p) => import_provider(db, &p, choice)?,
            DeepLink::Mcp(m) => import_mcp(place, db, &m)?,
            DeepLink::Prompt(p) => import_prompt(place, db, &p)?,
            DeepLink::Skill(_) => {
                return Err(GateError::Other(
                    "Skill 链接不在这里装：去 Skills 分页扫描这个来源、挑好再装".into(),
                ))
            }
        };
        self.ack(id);
        crate::audit::write(&format!("导入链接：{}", outcome.message));
        Ok(outcome)
    }
}

fn client_of(app: LinkApp) -> Client {
    match app {
        LinkApp::Claude => Client::ClaudeCode,
        LinkApp::Codex => Client::Codex,
        LinkApp::Gemini => Client::GeminiCli,
        LinkApp::GrokBuild => Client::GrokBuild,
    }
}

/// 链接声明的上游协议；跟这个软件的原生协议一样就不填（原样转发，跟没写一样）。
///
/// ⛔ 跟预设不一样：预设**不填**协议（它知道的是「官方端点长什么样」，不是「你这条线通不通」），
/// 链接是这个站点自己说的，而且不填的话「只说 OpenAI Chat 的站」配给 Claude Code 一发都通不了。
fn upstream_of(client: Client, f: Option<LinkFormat>) -> Option<UpstreamFormat> {
    let f = match f? {
        LinkFormat::Anthropic => UpstreamFormat::Anthropic,
        LinkFormat::OpenAiChat => UpstreamFormat::OpenaiChat,
        LinkFormat::OpenAiResponses => UpstreamFormat::OpenaiResponses,
        LinkFormat::GeminiNative => UpstreamFormat::GeminiNative,
    };
    let native = match client {
        Client::ClaudeCode | Client::ClaudeDesktop => UpstreamFormat::Anthropic,
        Client::Codex | Client::GrokBuild => UpstreamFormat::OpenaiResponses,
        Client::GeminiCli => UpstreamFormat::GeminiNative,
        Client::Antigravity | Client::AntigravityIde => return None,
    };
    (f != native).then_some(f)
}

/// 模型名去掉 `[1M]` 后缀（预设也这么剥：1M 窗口是客户端那一侧的开关，发给上游的名字不带它）。
fn strip_one_m(model: &Option<String>) -> Option<String> {
    model.as_deref().map(|m| {
        let lower = m.to_ascii_lowercase();
        if lower.ends_with("[1m]") {
            m[..m.len() - 4].trim_end().to_string()
        } else {
            m.to_string()
        }
    })
}

fn model_map_of(p: &ProviderLink) -> RouteModelMap {
    RouteModelMap {
        default: strip_one_m(&p.model),
        haiku: strip_one_m(&p.haiku_model),
        sonnet: strip_one_m(&p.sonnet_model),
        opus: strip_one_m(&p.opus_model),
        ..Default::default()
    }
}

/// 同一个站点：主机名和端口一样（站点按域名共享）。
fn same_site(a: &str, b: &str) -> bool {
    match (reqwest::Url::parse(a), reqwest::Url::parse(b)) {
        (Ok(a), Ok(b)) => {
            a.host_str().map(str::to_ascii_lowercase) == b.host_str().map(str::to_ascii_lowercase)
                && a.port_or_known_default() == b.port_or_known_default()
        }
        _ => false,
    }
}

fn existing_station(db: &Repository, base: &str) -> Result<Option<Provider>> {
    Ok(db
        .list::<Provider>("providers")?
        .into_iter()
        .find(|p| same_site(&p.base_url, base)))
}

fn summary(spec: &serde_json::Value) -> String {
    let transport = crate::mcp::transport(spec);
    let what = if transport == "stdio" {
        let mut parts = vec![spec["command"].as_str().unwrap_or("").to_string()];
        if let Some(args) = spec["args"].as_array() {
            parts.extend(args.iter().filter_map(|a| a.as_str()).map(str::to_string));
        }
        parts.join(" ")
    } else {
        spec["url"].as_str().unwrap_or("").to_string()
    };
    format!("{transport} · {what}")
}

fn view(db: &Repository, l: &DeepLink) -> Result<DeepLinkView> {
    Ok(match l {
        DeepLink::Provider(p) => {
            let client = client_of(
                p.app
                    .ok_or_else(|| GateError::Other("站点链接没写软件".into()))?,
            );
            let base = crate::endpoint::endpoint_base(&p.endpoints[0])?;
            DeepLinkView::Provider(Box::new(ProviderLinkView {
                client,
                name: p.name.clone(),
                endpoints: p.endpoints.clone(),
                key_masked: p.api_key.as_deref().map(crate::relay::mask),
                homepage: p.homepage.clone(),
                notes: p.notes.clone(),
                model_map: model_map_of(p),
                upstream_format: upstream_of(client, p.api_format),
                existing_station: existing_station(db, &base)?.map(|s| s.name),
                ignored: p.ignored.clone(),
            }))
        }
        DeepLink::Mcp(m) => {
            let have: Vec<String> = super::mcp_ops::servers(db)?
                .into_iter()
                .map(|s| s.id)
                .collect();
            DeepLinkView::Mcp(McpLinkView {
                apps: m.apps.clone(),
                servers: m
                    .servers
                    .iter()
                    .map(|(id, spec)| McpLinkServer {
                        id: id.clone(),
                        summary: summary(spec),
                        spec: serde_json::to_string_pretty(spec).unwrap_or_default(),
                        exists: have.contains(id),
                    })
                    .collect(),
                ignored: m.ignored.clone(),
            })
        }
        DeepLink::Prompt(p) => DeepLinkView::Prompt(PromptLinkView {
            app: p.app.clone(),
            name: p.name.clone(),
            description: p.description.clone(),
            content: p.content.clone(),
            ignored: p.ignored.clone(),
        }),
        DeepLink::Skill(s) => DeepLinkView::Skill(SkillLinkView {
            repo: s.repo.clone(),
            source: s.source(),
        }),
    })
}

fn import_provider(
    db: &Repository,
    p: &ProviderLink,
    choice: &DeepLinkChoice,
) -> Result<DeepLinkOutcome> {
    let client = client_of(
        p.app
            .ok_or_else(|| GateError::Other("站点链接没写软件".into()))?,
    );
    let key = p
        .api_key
        .clone()
        .or_else(|| choice.key.clone())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .ok_or_else(|| GateError::Other("链接里没有 API Key：在上面填一个再导入".into()))?;
    let group = choice.group.trim().to_string();
    let base = crate::endpoint::endpoint_base(&p.endpoints[0])?;
    let found = existing_station(db, &base)?;
    // 先查会不会撞：落了库的站点、凭证撤不回来，撞了要在动任何东西之前停。
    if let Some(s) = &found {
        let id = Route::make_id(client, &s.id, &group);
        if db.find::<Route>("station_routes", &id)?.is_some() {
            return Err(GateError::Other(format!(
                "「{}」在这个软件底下已经有「{}」这条线路了：要换 Key、地址去线路弹窗里改",
                s.name,
                if group.is_empty() {
                    "默认分组"
                } else {
                    &group
                }
            )));
        }
    }
    let station = match found {
        Some(s) => s,
        None => crate::workspace::provider_save_in(
            db,
            Provider {
                id: String::new(),
                name: p.name.clone(),
                base_url: base.clone(),
                website: p.homepage.clone().unwrap_or_default(),
                note: p.notes.clone().unwrap_or_default(),
                tags: Vec::new(),
                favorite: false,
                revision: 0,
                topup_per_usd: None,
            },
        )?,
    };
    let now = chrono::Utc::now().timestamp_millis();
    let mut notes = Vec::new();
    for extra in p.endpoints.iter().skip(1) {
        if let Err(e) = super::station_ops::add_endpoint(db, &station.id, extra, now) {
            notes.push(format!("备用地址 {extra} 没加上：{e}"));
        }
    }
    let credential = crate::workspace::credential_save_in(
        db,
        Credential {
            id: String::new(),
            provider_id: station.id.clone(),
            label: if group.is_empty() {
                "默认分组".into()
            } else {
                group.clone()
            },
            masked: String::new(),
            available: false,
            revision: 0,
        },
        "replace",
        Some(&key),
    )?;
    let route = Route {
        id: Route::make_id(client, &station.id, &group),
        client,
        station_id: station.id.clone(),
        group: group.clone(),
        credential_id: Some(credential.id),
        nominal_rate: None,
        latest_mult: None,
        last_audit_ms: None,
        protocols: Default::default(),
        rates: Default::default(),
        output_markup: None,
        rates_model: None,
        model_map: model_map_of(p),
        upstream_format: upstream_of(client, p.api_format),
    };
    db.put("station_routes", &route.id, &route)?;
    super::station_ops::source(db, &route)?;
    let mut message = format!(
        "导入了「{} · {}」：要用它，在中转站页点这一行",
        station.name,
        if group.is_empty() {
            "默认分组"
        } else {
            &group
        }
    );
    if !notes.is_empty() {
        message.push_str(&format!("（{}）", notes.join("；")));
    }
    Ok(DeepLinkOutcome {
        message,
        client: Some(client),
        route_id: Some(route.id),
        skipped: Vec::new(),
    })
}

fn import_mcp(place: &Place, db: &Repository, m: &link::McpLink) -> Result<DeepLinkOutcome> {
    let have: Vec<String> = super::mcp_ops::servers(db)?
        .into_iter()
        .map(|s| s.id)
        .collect();
    let mut added = Vec::new();
    let mut skipped = Vec::new();
    for (id, spec) in &m.servers {
        if have.contains(id) {
            skipped.push(id.clone());
            continue;
        }
        super::mcp_ops::save_at(
            place,
            db,
            McpServer {
                id: id.clone(),
                name: id.clone(),
                server: spec.clone(),
                // ⛔ 一个目标都不勾：导入不写任何客户端配置，使用者去 MCP 分页自己勾。
                targets: Vec::new(),
                description: None,
                homepage: None,
                tags: vec!["导入的".into()],
            },
            None,
        )?;
        added.push(id.clone());
    }
    let mut message = if added.is_empty() {
        "没有新加的 MCP".to_string()
    } else {
        format!(
            "把 {} 加进了 MCP 列表：还没写进任何软件，去 MCP 分页勾目标",
            added.join("、")
        )
    };
    if !skipped.is_empty() {
        message.push_str(&format!("；{} 已经有同名的，没动", skipped.join("、")));
    }
    Ok(DeepLinkOutcome {
        message,
        client: None,
        route_id: None,
        skipped,
    })
}

fn import_prompt(place: &Place, db: &Repository, p: &link::PromptLink) -> Result<DeepLinkOutcome> {
    super::prompt_ops::save_at(
        place,
        db,
        super::prompt_ops::Prompt {
            id: String::new(),
            name: p.name.clone(),
            content: p.content.clone(),
            description: p.description.clone(),
            created_ms: 0,
            updated_ms: 0,
        },
    )?;
    Ok(DeepLinkOutcome {
        message: format!(
            "把提示词「{}」放进了库里：还没用在任何地方，去提示词分页挑它",
            p.name
        ),
        client: None,
        route_id: None,
        skipped: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `{"mcpServers":{"fetch":{"type":"stdio","command":"uvx","args":["mcp-server-fetch"]}}}` 的 Base64（URL 转义过）。
    /// qb-app 不为单测拉一个 base64 依赖，解码那一侧在 `qb_extensions::deeplink` 里测。
    const MCP_FETCH: &str = "eyJtY3BTZXJ2ZXJzIjp7ImZldGNoIjp7InR5cGUiOiJzdGRpbyIsImNvbW1hbmQiOiJ1dngiLCJhcmdzIjpbIm1jcC1zZXJ2ZXItZmV0Y2giXX19fQ%3D%3D";
    /// `- 用中文回答`。
    const PROMPT_ZH: &str = "LSDnlKjkuK3mloflm57nrZQ%3D";

    fn fixture() -> (std::path::PathBuf, Repository, Place) {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root.join("db")).unwrap();
        let place = Place::under(&root);
        (root, db, place)
    }

    #[test]
    fn a_provider_link_lands_as_a_route_with_the_key_sealed_and_shows_only_a_masked_key() {
        let (root, db, place) = fixture();
        let inbox = Inbox::default();
        let id = inbox.put(
            "qbgate://v1/import?resource=provider&app=claude&name=Relay&endpoint=https%3A%2F%2Fapi.relay.example%2Fv1,https%3A%2F%2Fbackup.relay.example\
             &apiKey=sk-secret-abcdef123456&model=main%5B1m%5D&opusModel=big&apiFormat=openai_chat&enabled=true",
        );
        let entry = inbox.peek(&db).unwrap();
        assert_eq!(entry.id, id);
        assert_eq!(entry.scheme, "qbgate");
        let DeepLinkView::Provider(v) = &entry.view else {
            panic!("{:?}", entry.view)
        };
        // 给界面的那一份里没有明文 Key。
        let shown = serde_json::to_string(&entry).unwrap();
        assert!(!shown.contains("sk-secret-abcdef123456"), "{shown}");
        assert!(v.key_masked.is_some());
        assert_eq!(v.model_map.default.as_deref(), Some("main"), "[1M] 剥掉");
        assert_eq!(v.upstream_format, Some(UpstreamFormat::OpenaiChat));
        assert_eq!(v.ignored.len(), 1, "不替使用者切线路，说出来");

        let out = inbox
            .import_at(&place, &db, id, &DeepLinkChoice::default())
            .unwrap();
        let route: Route = db
            .get("station_routes", out.route_id.as_deref().unwrap())
            .unwrap();
        assert_eq!(route.client, Client::ClaudeCode);
        assert_eq!(route.model_map.opus.as_deref(), Some("big"));
        assert_eq!(
            db.key(route.credential_id.as_deref().unwrap()).unwrap(),
            "sk-secret-abcdef123456"
        );
        let e = super::super::station_ops::load_endpoints(&db, &route.station_id).unwrap();
        assert_eq!(e.endpoints.len(), 1, "第二个地址进了备用地址");
        assert!(inbox.peek(&db).is_none(), "导入成了就清掉");

        // 同一个站点、同一个分组再来一次：在动任何东西之前停。
        let again = inbox.put(
            "qbgate://v1/import?resource=provider&app=claude&name=Other&endpoint=https%3A%2F%2Fapi.relay.example&apiKey=k2",
        );
        let DeepLinkView::Provider(v2) = inbox.peek(&db).unwrap().view else {
            panic!()
        };
        assert_eq!(
            v2.existing_station.as_deref(),
            Some("Relay"),
            "按地址认出已有的站点"
        );
        let creds = db.list::<Credential>("credentials").unwrap().len();
        assert!(inbox
            .import_at(&place, &db, again, &DeepLinkChoice::default())
            .is_err());
        assert_eq!(
            db.list::<Credential>("credentials").unwrap().len(),
            creds,
            "撞了就什么都没落"
        );
        // 换个分组就成：接着用那个站点，不另建。
        let out = inbox
            .import_at(
                &place,
                &db,
                again,
                &DeepLinkChoice {
                    group: "vip".into(),
                    key: None,
                },
            )
            .unwrap();
        assert!(out.route_id.unwrap().ends_with("vip"));
        assert_eq!(db.list::<Provider>("providers").unwrap().len(), 1);

        // 链接没带 Key：要使用者填。
        let no_key = inbox.put(
            "qbgate://v1/import?resource=provider&app=codex&name=C&endpoint=https%3A%2F%2Fc.example%2Fv1",
        );
        let DeepLinkView::Provider(v3) = inbox.peek(&db).unwrap().view else {
            panic!()
        };
        assert!(v3.key_masked.is_none());
        assert_eq!(v3.upstream_format, None, "没写协议就原样转发");
        assert!(inbox
            .import_at(&place, &db, no_key, &DeepLinkChoice::default())
            .is_err());
        assert!(inbox
            .import_at(
                &place,
                &db,
                no_key,
                &DeepLinkChoice {
                    group: String::new(),
                    key: Some("typed-key".into()),
                },
            )
            .is_ok());

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mcp_and_prompt_links_go_into_the_library_and_nothing_is_enabled() {
        let (root, db, place) = fixture();
        let inbox = Inbox::default();
        let id = inbox.put(&format!(
            "qbgate://v1/import?resource=mcp&apps=claude&config={}&enabled=true",
            MCP_FETCH
        ));
        let DeepLinkView::Mcp(v) = inbox.peek(&db).unwrap().view else {
            panic!()
        };
        assert_eq!(v.servers[0].summary, "stdio · uvx mcp-server-fetch");
        assert!(!v.servers[0].exists);
        inbox
            .import_at(&place, &db, id, &DeepLinkChoice::default())
            .unwrap();
        let servers = super::super::mcp_ops::servers(&db).unwrap();
        assert_eq!(servers[0].id, "fetch");
        assert!(servers[0].targets.is_empty(), "一个目标都不勾");
        assert!(
            !root.join("home").join(".claude.json").exists(),
            "不写任何客户端配置"
        );
        // 同名的再来：跳过、不动。
        let again = inbox.put(&format!(
            "qbgate://v1/import?resource=mcp&apps=claude&config={}",
            MCP_FETCH
        ));
        let out = inbox
            .import_at(&place, &db, again, &DeepLinkChoice::default())
            .unwrap();
        assert_eq!(out.skipped, ["fetch"]);

        let p = inbox.put(&format!(
            "qbgate://v1/import?resource=prompt&app=claude&name=Rules&content={}",
            PROMPT_ZH
        ));
        inbox
            .import_at(&place, &db, p, &DeepLinkChoice::default())
            .unwrap();
        let prompts = super::super::prompt_ops::view(&place, &db).unwrap();
        assert_eq!(prompts.prompts[0].content, "- 用中文回答");
        assert!(prompts.targets.iter().all(|t| t.active.is_none()), "不启用");

        // Skill：不在这里装。
        let s = inbox.put("qbgate://v1/import?resource=skill&repo=o/r");
        assert!(inbox
            .import_at(&place, &db, s, &DeepLinkChoice::default())
            .is_err());

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_inbox_keeps_the_newest_link_until_its_own_ack() {
        let (root, db, _) = fixture();
        let inbox = Inbox::default();
        let first = inbox.put("qbgate://v1/import?resource=skill&repo=o/a");
        let second = inbox.put("not a link");
        // 新的盖掉旧的；解析不了也照实给界面。
        let entry = inbox.peek(&db).unwrap();
        assert_eq!(entry.id, second);
        assert!(matches!(entry.view, DeepLinkView::Error { .. }));
        // 读不清除；旧编号的确认不动新的。
        assert!(inbox.peek(&db).is_some());
        inbox.ack(first);
        assert!(inbox.peek(&db).is_some());
        inbox.ack(second);
        assert!(inbox.peek(&db).is_none());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
