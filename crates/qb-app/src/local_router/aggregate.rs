// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 聚合模型在本机路由里的那一半（中转站重构阶段 3）：名单 → 发布哪些模型，请求里的 id → 打哪条线。
//!
//! id 的编解码与报错的话在 `qb_proxy::aggregate`（照搬 cc-switch `mode/stack.rs`），名单存在这个软件的
//! 本机路由设置里（[`AggregateSettings`]）。这里做三件事：
//!
//! - [`published`]：名单里每条线发布哪些模型 —— 设置里写了就是写的那些，没写就是这条线模型映射里的目标模型
//!   （cc-switch：配了 Stack 模型列表用列表，没配用模型映射）；
//! - [`resolve`]：带前缀的 id 解成「哪条线 + 发往上游的模型名」；解不出来一律 [`Miss`]，**不回落**；
//! - [`decide`]：[`server`](super::server) 每发请求问一次 —— 本机自己答、直达名单里那一条，还是照旧走派单。
//!
//! ⛔ 官方 Codex 那条线永远不在聚合里（[`relayable`]）：它的请求带着官方登录身份，只走 `official.rs`。

use super::log::Record;
use super::server::Inbound;
use super::{text, ProxyBody, RouterState, Upstream, UpstreamAuth};
use hyper::body::Bytes;
use hyper::{Method, Response, StatusCode};
use qb_contract::domain::Client;
use qb_proxy::aggregate::{self, Decoded, Miss, Published};
use qb_proxy::classify::Fault;
use qb_proxy::convert::plan::ApiFormat;
use qb_proxy::model_mapper::strip_one_m_suffix_for_upstream;
use qb_station::router::AggregateSettings;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 一条线要发布的模型（发往上游的名字）：设置里写了用写的，没写用这条线模型映射里的目标模型。
pub fn line_models(line: &Upstream, settings: &AggregateSettings) -> Vec<String> {
    if let Some(listed) = settings
        .models
        .get(&line.route_id)
        .filter(|l| l.iter().any(|m| !m.trim().is_empty()))
    {
        return listed.clone();
    }
    let map = &line.model_map;
    let mut out: Vec<String> = Vec::new();
    for m in line.model_exact.iter().map(|(_, to)| to.clone()).chain(
        [
            &map.default_model,
            &map.opus_model,
            &map.sonnet_model,
            &map.haiku_model,
            &map.fable_model,
            &map.subagent_model,
        ]
        .into_iter()
        .flatten()
        .cloned(),
    ) {
        if !m.trim().is_empty() && !out.contains(&m) {
            out.push(m);
        }
    }
    out
}

/// 名单里的线发布给客户端的模型，按名单顺序。没登记 key、本机路由手里没有（删了 / 没配 Key）的线跳过。
/// 桌面端两个模型撞了同一个数字（概率很小）时后一个不发布：id 是散列出来的，换数字就不稳定了。
pub fn published(
    client: Client,
    settings: &AggregateSettings,
    upstreams: &HashMap<String, Upstream>,
) -> Vec<Published> {
    let mut out: Vec<Published> = Vec::new();
    for route_id in &settings.members {
        let Some(key) = key_of(settings, route_id) else {
            continue;
        };
        let Some(line) = upstreams.get(route_id).filter(|l| relayable(l)) else {
            continue;
        };
        for p in aggregate::publish(
            client,
            key,
            route_id,
            &line.label,
            &line_models(line, settings),
        ) {
            if out.iter().any(|q| q.id == p.id) {
                tracing::warn!(
                    "聚合模型 {} 的 id {} 跟前面的重复，不发布",
                    p.upstream,
                    p.id
                );
                continue;
            }
            out.push(p);
        }
    }
    out
}

fn key_of<'a>(settings: &'a AggregateSettings, route_id: &str) -> Option<&'a str> {
    settings
        .keys
        .iter()
        .find(|(_, id)| id.as_str() == route_id)
        .map(|(k, _)| k.as_str())
}

/// 请求里的模型 id 解成什么。
#[derive(Debug, Clone)]
pub enum Resolved {
    /// 不带保留前缀：照旧走派单队列（不读任何聚合状态）。
    Plain,
    /// 聚合模型：打这一条线，模型名换成 `upstream_model`。
    Hit {
        line: Box<Upstream>,
        upstream_model: String,
    },
    /// 带保留前缀但解不出来：报错，不回落。
    Miss(Miss),
}

/// 解析请求里的模型 id。不带保留前缀时不看名单，派单的路径不变。
pub fn resolve(
    client: Client,
    settings: &AggregateSettings,
    upstreams: &HashMap<String, Upstream>,
    model: &str,
) -> Resolved {
    let decoded = aggregate::decode(client, model);
    if decoded == Decoded::Plain {
        return Resolved::Plain;
    }
    if !settings.enabled {
        return Resolved::Miss(Miss::Off);
    }
    let (key, model_part, one_m) = match decoded {
        Decoded::Plain => return Resolved::Plain,
        Decoded::Malformed => return Resolved::Miss(Miss::Unknown),
        Decoded::Code(_) => {
            // 桌面端只有数字：按名单发布出去的那些反查（跟 `published` 同一个顺序，撞号时命中的就是列表里那个）。
            return match published(client, settings, upstreams)
                .into_iter()
                .find(|p| aggregate::strip_desktop_one_m(model) == p.id)
            {
                Some(p) => hit(upstreams, &p.route_id, p.upstream),
                None if removed_code(client, settings, upstreams, model) => {
                    Resolved::Miss(Miss::Removed)
                }
                None => Resolved::Miss(Miss::Unknown),
            };
        }
        Decoded::Agg { key, model, one_m } => (key, model, one_m),
    };
    let Some(route_id) = settings.keys.get(key) else {
        return Resolved::Miss(Miss::Unknown);
    };
    if !settings.members.contains(route_id) {
        return Resolved::Miss(Miss::Removed);
    }
    let Some(line) = upstreams.get(route_id).filter(|l| relayable(l)) else {
        return Resolved::Miss(Miss::Deleted);
    };
    // 发往上游的是这条线发布时写的原值（可能带 1M 标记）；已经不在列表里了就照原样发（上游自己决定认不认）。
    let upstream_model = aggregate::publish(
        client,
        key,
        route_id,
        &line.label,
        &line_models(line, settings),
    )
    .into_iter()
    .find(|p| {
        qb_proxy::model_mapper::strip_one_m_suffix_for_upstream(&p.upstream).trim() == model_part
    })
    .map(|p| p.upstream)
    .unwrap_or_else(|| {
        if one_m {
            format!("{model_part}{}", aggregate::ONE_M_MARKER_FOR_CLIENT)
        } else {
            model_part.to_string()
        }
    });
    hit(upstreams, route_id, upstream_model)
}

fn hit(upstreams: &HashMap<String, Upstream>, route_id: &str, upstream_model: String) -> Resolved {
    match upstreams.get(route_id).filter(|l| relayable(l)) {
        Some(line) => Resolved::Hit {
            line: Box::new(line.clone()),
            upstream_model,
        },
        None => Resolved::Miss(Miss::Deleted),
    }
}

/// 桌面端的数字 id 是不是名单外（移出去的）某条线发布过的。
fn removed_code(
    client: Client,
    settings: &AggregateSettings,
    upstreams: &HashMap<String, Upstream>,
    model: &str,
) -> bool {
    let id = aggregate::strip_desktop_one_m(model);
    settings
        .keys
        .iter()
        .filter(|(_, route_id)| !settings.members.contains(route_id))
        .any(|(key, route_id)| {
            upstreams.get(route_id).is_some_and(|line| {
                aggregate::publish(
                    client,
                    key,
                    route_id,
                    &line.label,
                    &line_models(line, settings),
                )
                .iter()
                .any(|p| p.id == id)
            })
        })
}

/// 能进聚合的线：中转线路。⛔ 官方 Codex 那条（[`UpstreamAuth::OAuthPassthrough`]）不行 —— 名单里混进它的 id
/// 也不发布、不转发（存设置时也拒收，见 `station_ops::save_router_settings`）。
fn relayable(line: &Upstream) -> bool {
    !matches!(line.auth, UpstreamAuth::OAuthPassthrough)
}

/// 一发请求在聚合这一层怎么走。
pub(super) enum Decision {
    /// 跟聚合无关（没开、不是模型发现、模型不带保留前缀）：照旧走派单队列。
    Pass,
    /// 本机自己回：模型发现的列表，或者解不出来的报错。一条线都没打。
    Answer(Response<ProxyBody>),
    /// 直达名单里的这一条（请求体里的模型名已经换成发往上游的那个）。
    Pinned(Upstream),
}

/// [`server`](super::server) 收齐请求体、分两条路之前问这里（cc-switch `handlers.rs` 的 `handle_models` 与
/// `resolve_stack_target`）。`official` = 这个软件此刻绑到的是官方 Codex 那条线。
///
/// - Claude 两个的模型发现（[`aggregate::is_model_discovery`]）：聚合开着时本机自己答、不联网（客户端只等 3 秒），
///   **不落日志** —— 一条线都没打，落了只会给当前那条记一发假的「秒回」，混进「快 / 稳」的证据；没开就照旧转给上游。
/// - 模型带保留前缀：解成名单里那一条直达；解不出来回 400、**不回落**（这一发照记日志，方便查是哪个 id）。
/// - ⛔ 官方 Codex 是当前那条时，带前缀的模型一律拒绝：Codex 这时带着官方登录的 `chatgpt-account-id` 等身份头，
///   转去中转站就把官方身份交给了第三方（cc-switch 在那条路上专门剥身份头；我们干脆不转）。
pub(super) fn decide(
    state: &Arc<Mutex<RouterState>>,
    inbound: &mut Inbound,
    record: &mut Record,
    official: bool,
) -> Decision {
    let caller = inbound.caller;
    let claude = matches!(caller, Client::ClaudeCode | Client::ClaudeDesktop);
    if claude
        && inbound.method == Method::GET
        && aggregate::is_model_discovery(
            &inbound.path,
            inbound.headers.contains_key("anthropic-version"),
        )
    {
        let models = match state.lock() {
            Ok(s) => {
                let settings = s.settings(caller).aggregate;
                if !settings.enabled {
                    return Decision::Pass;
                }
                published(caller, &settings, &s.upstreams)
            }
            Err(_) => return Decision::Pass,
        };
        record.unlogged();
        return Decision::Answer(json(
            StatusCode::OK,
            &aggregate::discovery_body(caller, &models),
        ));
    }

    let model = record.log.model.clone();
    if model.is_empty() || aggregate::decode(caller, &model) == Decoded::Plain {
        return Decision::Pass;
    }
    let resolved = match state.lock() {
        Ok(s) => resolve(caller, &s.settings(caller).aggregate, &s.upstreams, &model),
        Err(_) => {
            return Decision::Answer(text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "本机路由内部状态损坏",
            ))
        }
    };
    match resolved {
        Resolved::Plain => Decision::Pass,
        Resolved::Miss(miss) => refuse(caller, record, &miss.message(&model)),
        Resolved::Hit { .. } if official => refuse(
            caller,
            record,
            &format!(
                "官方 Codex 是当前那条线时不能用聚合的模型 {model}：这一发带着官方登录的身份，不能转给中转站。请先在中转站页给 Codex 换一条中转线 (Aggregated model {model} cannot be used while the official Codex line is current)"
            ),
        ),
        Resolved::Hit {
            line,
            upstream_model,
        } => {
            let outgoing = outgoing_model(caller, &line, &upstream_model);
            let mut body = match serde_json::from_slice::<Value>(&inbound.body) {
                Ok(v) if v.is_object() => v,
                _ => return refuse(caller, record, "请求体不是 JSON 对象，认不出要用哪个聚合模型"),
            };
            body["model"] = Value::String(outgoing.clone());
            match serde_json::to_vec(&body) {
                Ok(bytes) => inbound.body = Bytes::from(bytes),
                Err(e) => return refuse(caller, record, &format!("改写请求体失败：{e}")),
            }
            tracing::debug!(
                "本机路由：聚合模型 {model} → {}（{outgoing}）",
                line.route_id
            );
            record.log.route_id = line.route_id.clone();
            record.log.upstream_model = outgoing;
            Decision::Pinned(*line)
        }
    }
}

/// 发往上游的模型名：写的原值剥掉 `[1M]`（网关不认）。只有 Codex 转 Anthropic 那一种留着 —— 转换那一步自己剥，
/// 另加 context-1m 的 beta 头（`convert::plan::prepare_request`）。
fn outgoing_model(caller: Client, line: &Upstream, upstream: &str) -> String {
    if caller == Client::Codex && line.format == Some(ApiFormat::Anthropic) {
        upstream.trim().to_string()
    } else {
        strip_one_m_suffix_for_upstream(upstream).trim().to_string()
    }
}

/// 解不出来：按客户端那一侧的错误信封回 400（[`aggregate::miss_body`]），日志记成请求本身的问题（不算线路的错）。
fn refuse(caller: Client, record: &mut Record, message: &str) -> Decision {
    tracing::warn!("本机路由：{message}");
    record.log.status = Some(StatusCode::BAD_REQUEST.as_u16());
    record.log.fault = Some(Fault::ClientRequest.key().to_string());
    record.log.error = Some(message.to_string());
    Decision::Answer(json(
        StatusCode::BAD_REQUEST,
        &aggregate::miss_body(caller, message),
    ))
}

fn json(status: StatusCode, body: &Value) -> Response<ProxyBody> {
    let mut r = text(status, &body.to_string());
    r.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("application/json"),
    );
    r
}

#[cfg(test)]
mod tests {
    //! 改写自 cc-switch `mode/stack.rs` 的名单与解析测试。
    use super::*;
    use qb_proxy::model_mapper::ModelMapping;

    fn line(id: &str, label: &str, default: Option<&str>) -> Upstream {
        Upstream {
            route_id: id.into(),
            base_url: "https://relay.test".into(),
            label: label.into(),
            model_map: ModelMapping {
                default_model: default.map(str::to_string),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn fixture() -> (AggregateSettings, HashMap<String, Upstream>) {
        let mut s = AggregateSettings {
            enabled: true,
            members: vec!["kimi-line".into(), "zhipu-line".into()],
            ..Default::default()
        };
        aggregate::allocate_key(&mut s.keys, "kimi-line", &["Kimi"]);
        aggregate::allocate_key(&mut s.keys, "zhipu-line", &["Zhipu"]);
        aggregate::allocate_key(&mut s.keys, "gone-line", &["Gone"]);
        s.models.insert(
            "zhipu-line".into(),
            vec!["glm-5[1M]".into(), "glm-4.7".into()],
        );
        let mut up = HashMap::new();
        up.insert(
            "kimi-line".into(),
            line("kimi-line", "Kimi · 默认", Some("kimi-k3")),
        );
        up.insert("zhipu-line".into(), line("zhipu-line", "智谱", None));
        (s, up)
    }

    #[test]
    fn members_publish_their_listed_models_or_their_mapping_in_order() {
        let (s, up) = fixture();
        let ids: Vec<String> = published(Client::ClaudeCode, &s, &up)
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(
            ids,
            [
                "qbg-claude-kimi--kimi-k3",
                "qbg-claude-zhipu--glm-5[1M]",
                "qbg-claude-zhipu--glm-4.7"
            ]
        );
    }

    #[test]
    fn aggregated_ids_resolve_to_their_line_and_upstream_model() {
        let (s, up) = fixture();
        match resolve(Client::ClaudeCode, &s, &up, "qbg-claude-zhipu--glm-5[1M]") {
            Resolved::Hit {
                line,
                upstream_model,
            } => {
                assert_eq!(line.route_id, "zhipu-line");
                assert_eq!(upstream_model, "glm-5[1M]", "发往上游的是写的原值");
            }
            other => panic!("{other:?}"),
        }
        match resolve(Client::Codex, &s, &up, "qbg-kimi/whatever-model") {
            Resolved::Hit { upstream_model, .. } => {
                assert_eq!(upstream_model, "whatever-model", "不在列表里照原样发");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn aggregated_ids_that_cannot_be_resolved_never_fall_back() {
        let (mut s, up) = fixture();
        let miss = |s: &AggregateSettings, id: &str| match resolve(Client::ClaudeCode, s, &up, id) {
            Resolved::Miss(m) => m,
            other => panic!("{id}: {other:?}"),
        };
        assert_eq!(miss(&s, "qbg-claude-nobody--m"), Miss::Unknown);
        assert_eq!(miss(&s, "qbg-claude-gone--m"), Miss::Removed);
        assert_eq!(miss(&s, "qbg-claude-kimi"), Miss::Unknown);
        s.members.push("gone-line".into());
        assert_eq!(
            miss(&s, "qbg-claude-gone--m"),
            Miss::Deleted,
            "线路删了（路由手里没有）"
        );
        s.enabled = false;
        assert_eq!(miss(&s, "qbg-claude-kimi--kimi-k3"), Miss::Off);
    }

    #[test]
    fn plain_models_do_not_read_the_list() {
        let (s, up) = fixture();
        assert!(matches!(
            resolve(Client::ClaudeCode, &s, &up, "claude-opus-5"),
            Resolved::Plain
        ));
        let off = AggregateSettings::default();
        assert!(matches!(
            resolve(Client::Codex, &off, &up, "gpt-6"),
            Resolved::Plain
        ));
    }

    #[test]
    fn desktop_codes_resolve_through_the_published_list() {
        let (s, up) = fixture();
        let p = published(Client::ClaudeDesktop, &s, &up);
        let glm5 = p.iter().find(|p| p.upstream == "glm-5[1M]").unwrap();
        match resolve(Client::ClaudeDesktop, &s, &up, &format!("{}[1m]", glm5.id)) {
            Resolved::Hit {
                line,
                upstream_model,
            } => {
                assert_eq!(line.route_id, "zhipu-line");
                assert_eq!(upstream_model, "glm-5[1M]");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            resolve(Client::ClaudeDesktop, &s, &up, "claude-fable-qbg-000000"),
            Resolved::Miss(Miss::Unknown)
        ));
    }
}
