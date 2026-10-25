// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 聚合模型的 id 怎么编、怎么解（中转站重构阶段 3，cc-switch `src-tauri/src/mode/stack.rs` 的 Stack 模式）。
//!
//! 名单里每条线的模型以**带前缀的 id** 发布给客户端，选中后请求直达那一条线；不带前缀的照旧走派单队列。
//! 规则照搬 cc-switch：
//!
//! - key 一经分配**永久**归这条线（[`allocate_key`]）：客户端会一直带着选中过的 id，key 改了指向，旧 id 就会被
//!   悄悄发到另一条线；
//! - 带前缀的 id 解不出来一律报错，**不回落**（[`Miss`]）：回落会用别家的钱、别家的模型回答，使用者看不出来；
//! - 选中聚合模型的请求不读也不写任何路由状态（熔断、故障转移），见 `qb-app::local_router`。
//!
//! 跟 cc-switch 不一样的地方：前缀从 `ccs` 换成 `qbg`（不冒用 CC Switch 的名字）；key 由线路的站点名 / 分组名生成
//! （QB Gate 的线路没有图标）；名单、发布哪些模型都存在本机路由设置里（`qb_station::router::AggregateSettings`），
//! 不写客户端的配置文件 —— Codex 的模型目录、桌面端的模型条目是阶段 4（配置写入引擎）的事。

use crate::model_mapper::strip_one_m_suffix_for_upstream;
use qb_contract::domain::Client;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Claude Code 的聚合模型 id：`qbg-claude-<key>--<model>`。id 里要有 `claude` 才进 `/model` 选择器，
/// 不以 `claude-` 开头 MAX 窗口才生效（cc-switch 实测）。
pub const CLAUDE_PREFIX: &str = "qbg-claude-";
const CLAUDE_SEPARATOR: &str = "--";
/// Codex 的聚合模型 id：`qbg-<key>/<model>`。
pub const CODEX_PREFIX: &str = "qbg-";
const CODEX_SEPARATOR: char = '/';
/// 桌面端的聚合模型 id：`claude-fable-qbg-<6 位数字>`（[`desktop_code`]）。桌面端的选择器只收含 `claude` 等字样、
/// 又不含厂商名的 id，所以 id 里放不下 key 和模型名，只放数字；末尾 8 位数字会被当成日期剥掉，所以是 6 位。
pub const DESKTOP_PREFIX: &str = "claude-fable-qbg-";
const DESKTOP_CODE_DIGITS: usize = 6;
/// key 的最大长度：只是为了模型 id 不至于太长。
const KEY_MAX_LEN: usize = 24;
/// 发给客户端的 1M 标记（Claude Code 认它，按 1M 窗口算）。
pub const ONE_M_MARKER_FOR_CLIENT: &str = "[1M]";
/// Claude Code 在没有 `CLAUDE_CODE_MAX_CONTEXT_TOKENS` 时按这个窗口算。
pub const CLAUDE_DEFAULT_WINDOW: u64 = 200_000;

/// 小写 ASCII，只保留 `[a-z0-9-]`，其余字符换成 `-`，连续的 `-` 合并，首尾的 `-` 去掉。
/// 结果里不会有 `--`（Claude id 的分隔符）和 `/`（Codex id 的分隔符）。
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= KEY_MAX_LEN {
            break;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// 给线路 `route_id` 一个 key：登记过就用原来的（移出名单后重新加入，旧会话里的 id 继续有效），没有就按
/// `hints`（站点名、分组名，按顺序取第一个能用的）生成一个新的写进登记簿。
///
/// 新 key 跟登记簿里所有的 key 去重，不只是当前名单：移出的、删掉的线的 key 也占着位置，旧 id 才不会发给新来的这条。
pub fn allocate_key(keys: &mut BTreeMap<String, String>, route_id: &str, hints: &[&str]) -> String {
    if let Some((key, _)) = keys.iter().find(|(_, id)| id.as_str() == route_id) {
        return key.clone();
    }
    let base = hints
        .iter()
        .map(|h| slug(h))
        .find(|candidate| !candidate.is_empty())
        .unwrap_or_else(|| {
            let id: String = slug(route_id)
                .chars()
                .filter(|c| *c != '-')
                .take(6)
                .collect();
            format!("r{id}")
        });
    let mut key = base.clone();
    let mut suffix = 2;
    while keys.contains_key(&key) {
        key = format!("{base}-{suffix}");
        suffix += 1;
    }
    keys.insert(key.clone(), route_id.to_string());
    key
}

/// 聚合模型 id。Claude Code 的上游是 1M 窗口时末尾带 `[1M]`；桌面端的 id 不带标记。
pub fn encode(client: Client, key: &str, model: &str, one_m: bool) -> String {
    match client {
        Client::Codex => format!("{CODEX_PREFIX}{key}{CODEX_SEPARATOR}{model}"),
        Client::ClaudeDesktop => format!("{DESKTOP_PREFIX}{}", desktop_code(key, model)),
        _ => {
            let marker = if one_m { ONE_M_MARKER_FOR_CLIENT } else { "" };
            format!("{CLAUDE_PREFIX}{key}{CLAUDE_SEPARATOR}{model}{marker}")
        }
    }
}

/// 桌面端聚合模型 id 里的数字：key 和上游模型名的 FNV-1a 散列取 6 位。不用标准库的散列：它不保证跨版本稳定，
/// 客户端会一直带着选中过的 id。
pub fn desktop_code(key: &str, model: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes().chain([0]).chain(model.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!(
        "{:0width$}",
        hash % 10u64.pow(DESKTOP_CODE_DIGITS as u32),
        width = DESKTOP_CODE_DIGITS
    )
}

/// 桌面端发请求时，1M 条目的 id 末尾带 `[1m]`：剥掉之后才是发布出去的那个 id（按它反查名单）。
pub fn strip_desktop_one_m(id: &str) -> &str {
    strip_one_m_suffix_for_upstream(id.trim()).trim()
}

/// 解码的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoded<'a> {
    /// 不带保留前缀：普通模型名，照旧走派单队列。
    Plain,
    /// 带保留前缀，但切不出 key 和模型。
    Malformed,
    /// 带保留前缀。`model` 可以含 `/` 和 `--`（key 里没有这两种分隔符，在第一个处切开）。
    Agg {
        key: &'a str,
        model: &'a str,
        /// Claude id 末尾带着 1M 标记。
        one_m: bool,
    },
    /// 桌面端的 id：只有数字，key 和模型要按名单反查。
    Code(&'a str),
}

/// 按客户端的格式解码模型 id。Claude Code：`qbg-claude-` 开头；Codex：`qbg-` 开头且含 `/`；
/// 桌面端：`claude-fable-qbg-` 加 6 位数字。
pub fn decode(client: Client, id: &str) -> Decoded<'_> {
    let (rest, separator, one_m) = match client {
        Client::ClaudeCode => {
            let Some(rest) = id.strip_prefix(CLAUDE_PREFIX) else {
                return Decoded::Plain;
            };
            let stripped = strip_one_m_suffix_for_upstream(rest);
            (stripped, CLAUDE_SEPARATOR, stripped.len() != rest.len())
        }
        Client::Codex => {
            let Some(rest) = id.strip_prefix(CODEX_PREFIX) else {
                return Decoded::Plain;
            };
            if !rest.contains(CODEX_SEPARATOR) {
                return Decoded::Plain;
            }
            (rest, "/", false)
        }
        Client::ClaudeDesktop => {
            let Some(rest) = id.strip_prefix(DESKTOP_PREFIX) else {
                return Decoded::Plain;
            };
            // 1M 条目桌面端发请求时用 `<id>[1m]`。
            let code = strip_one_m_suffix_for_upstream(rest);
            return if code.len() == DESKTOP_CODE_DIGITS && code.bytes().all(|b| b.is_ascii_digit())
            {
                Decoded::Code(code)
            } else {
                Decoded::Malformed
            };
        }
        _ => return Decoded::Plain,
    };
    match rest.split_once(separator) {
        Some((key, model)) if !key.is_empty() && !model.is_empty() => {
            Decoded::Agg { key, model, one_m }
        }
        _ => Decoded::Malformed,
    }
}

/// 带前缀的 id 为什么解不出来。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Miss {
    /// 这个软件没开聚合模型（名单留着，但不发布、也不转发）。
    Off,
    /// key 没登记，或者 id 切不出 key 和模型。
    Unknown,
    /// 这条线已经移出名单。
    Removed,
    /// 这条线已经删掉了（或者没配 Key，本机路由手里没有它）。
    Deleted,
}

impl Miss {
    /// 回给客户端的那句话。
    pub fn message(self, model: &str) -> String {
        match self {
            Miss::Off => format!(
                "聚合的模型 {model} 只能在聚合模型开着时用，这个软件现在没开，请在模型列表里重新选择 (Aggregated model {model} only works while aggregation is on; pick a model from the model list again)"
            ),
            Miss::Unknown => format!(
                "聚合的模型 {model} 在 QB Gate 里不存在，请在模型列表里重新选择 (Aggregated model {model} is unknown to QB Gate; pick a model from the model list again)"
            ),
            Miss::Removed => format!(
                "聚合的模型 {model} 已经移出聚合名单，请在模型列表里重新选择 (Aggregated model {model} was removed from the aggregation; pick a model from the model list again)"
            ),
            Miss::Deleted => format!(
                "聚合的模型 {model} 对应的线路已经删掉（或者没配 Key），请在模型列表里重新选择 (The line behind aggregated model {model} was deleted or has no key; pick a model from the model list again)"
            ),
        }
    }
}

/// 解不出来时的错误体：Claude 两个用 Anthropic 的错误信封，Codex 用 OpenAI 的（cc-switch 的 `stack_miss_body`）。
pub fn miss_body(client: Client, message: &str) -> Value {
    match client {
        Client::ClaudeCode | Client::ClaudeDesktop => json!({
            "type": "error",
            "error": { "type": "invalid_request_error", "message": message },
        }),
        _ => json!({
            "error": {
                "message": message,
                "type": "invalid_request_error",
                "param": "model",
                "code": "model_not_found",
            }
        }),
    }
}

/// 一条线发布给客户端的一个模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub route_id: String,
    /// 发布给客户端的 id（带前缀）。
    pub id: String,
    /// 发往上游的模型名（使用者写的原值，可能带 `[1M]`，转发时跟普通请求一样剥掉）。
    pub upstream: String,
    /// 选择器里显示的名字。
    pub display_name: String,
    /// 选择器里的说明：`<上游模型名> · <窗口>`。
    pub description: String,
    pub one_m: bool,
}

/// 一条线要发布的模型：按去掉 1M 标记后的名字去重（任何一处带标记就按 1M），显示名 `<模型>（<线路名>）`。
pub fn publish(
    client: Client,
    key: &str,
    route_id: &str,
    label: &str,
    models: &[String],
) -> Vec<Published> {
    let mut out: Vec<Published> = Vec::new();
    for raw in models {
        let model = strip_one_m_suffix_for_upstream(raw.trim())
            .trim()
            .to_string();
        if model.is_empty() {
            continue;
        }
        let one_m = model.len() != raw.trim().len();
        if let Some(existing) = out
            .iter_mut()
            .find(|p| strip_one_m_suffix_for_upstream(&p.upstream).trim() == model)
        {
            if one_m && !existing.one_m {
                existing.one_m = true;
                existing.upstream = raw.trim().to_string();
                existing.id = encode(client, key, &model, true);
                existing.description = model_description(&model, 1_000_000);
            }
            continue;
        }
        let window = if one_m {
            1_000_000
        } else {
            CLAUDE_DEFAULT_WINDOW
        };
        out.push(Published {
            route_id: route_id.to_string(),
            id: encode(client, key, &model, one_m),
            upstream: raw.trim().to_string(),
            display_name: display_name(&model, label),
            description: model_description(&model, window),
            one_m,
        });
    }
    out
}

/// 选择器里聚合模型的显示名：`<模型>（<线路名>）`。
pub fn display_name(model: &str, label: &str) -> String {
    if label.trim().is_empty() {
        model.to_string()
    } else {
        format!("{model}（{label}）")
    }
}

/// 选择器里聚合模型的说明：`<上游模型名> · <窗口>`。
pub fn model_description(model: &str, window: u64) -> String {
    match window_label(window) {
        Some(window) => format!("{model} · {window}"),
        None => model.to_string(),
    }
}

/// 窗口的简写：整百万写 `1M`，其余按千写 `256K`，不足一千照原样。
fn window_label(window: u64) -> Option<String> {
    match window {
        0 => None,
        w if w % 1_000_000 == 0 => Some(format!("{}M", w / 1_000_000)),
        w if w >= 1_000 => Some(format!("{}K", (w + 500) / 1_000)),
        w => Some(w.to_string()),
    }
}

/// Claude Code / 桌面端的模型发现请求：`GET /v1/models?limit=…`，经 Anthropic SDK 发出、带 `anthropic-version`
/// （cc-switch 的 `is_claude_model_discovery`）。Codex 取目录时两者都没有（它带的是 `client_version`）。
pub fn is_model_discovery(path: &str, has_anthropic_version: bool) -> bool {
    let (bare, query) = path.split_once('?').unwrap_or((path, ""));
    if bare.trim_end_matches('/') != "/v1/models" {
        return false;
    }
    if has_anthropic_version {
        return true;
    }
    let has = |name: &str| {
        query
            .split('&')
            .any(|pair| pair.split('=').next() == Some(name))
    };
    has("limit") && !has("client_version")
}

/// 模型发现的回答（Anthropic 的形状）。聚合开着时本机路由**自己答**、不联网：客户端只等 3 秒（cc-switch 同样如此）。
/// 桌面端多一个 `created_at` 和 `supports1m`。
pub fn discovery_body(client: Client, models: &[Published]) -> Value {
    let data: Vec<Value> = models
        .iter()
        .map(|m| {
            let mut item = json!({
                "type": "model",
                "id": m.id,
                "display_name": m.display_name,
            });
            if client == Client::ClaudeDesktop {
                item["created_at"] = json!("2025-01-01T00:00:00Z");
                if m.one_m {
                    item["supports1m"] = json!(true);
                }
            } else {
                item["description"] = json!(m.description);
            }
            item
        })
        .collect();
    let first_id = data.first().and_then(|d| d.get("id")).cloned();
    let last_id = data.last().and_then(|d| d.get("id")).cloned();
    json!({
        "data": data,
        "has_more": false,
        "first_id": first_id,
        "last_id": last_id,
    })
}

#[cfg(test)]
mod tests {
    //! 改写自 cc-switch `mode/stack.rs` 的测试（id 的编解码、key 的分配）；名单与发布的那几条在 `qb-app`。
    use super::*;

    #[test]
    fn keys_come_from_the_station_then_the_group() {
        let mut keys = BTreeMap::new();
        assert_eq!(
            allocate_key(&mut keys, "r1", &["Kimi For Coding", "g"]),
            "kimi-for-coding"
        );
        assert_eq!(
            allocate_key(&mut keys, "r2", &["My Relay 2.0!"]),
            "my-relay-2-0"
        );
        assert_eq!(allocate_key(&mut keys, "r3", &["智谱", "  "]), "rr3");
        assert_eq!(allocate_key(&mut keys, "r4", &["", "DeepSeek"]), "deepseek");
    }

    #[test]
    fn keys_never_contain_the_separators_and_stay_short() {
        for text in ["a--b", "a/b", "--edge--", "UPPER__lower", &"x".repeat(80)] {
            let key = slug(text);
            assert!(!key.contains("--") && !key.contains('/'), "{text} → {key}");
            assert!(
                !key.starts_with('-') && !key.ends_with('-'),
                "{text} → {key}"
            );
            assert!(key.len() <= KEY_MAX_LEN, "{text} → {key}");
        }
        assert_eq!(slug("UPPER__lower"), "upper-lower");
    }

    #[test]
    fn a_key_stays_with_its_line_forever() {
        let mut keys = BTreeMap::new();
        assert_eq!(allocate_key(&mut keys, "a", &["Kimi"]), "kimi");
        // 同名的另一条线拿不到 a 的 key（a 移出名单也不行）。
        assert_eq!(allocate_key(&mut keys, "b", &["Kimi"]), "kimi-2");
        // a 重新加入，拿回原来的 key。
        assert_eq!(allocate_key(&mut keys, "a", &["Something else"]), "kimi");
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn claude_ids_round_trip() {
        for (model, one_m) in [
            ("kimi-k3", false),
            ("glm-5.2", true),
            ("vendor/model", false),
            ("model--with--dashes", true),
        ] {
            let id = encode(Client::ClaudeCode, "kimi", model, one_m);
            assert_eq!(
                decode(Client::ClaudeCode, &id),
                Decoded::Agg {
                    key: "kimi",
                    model,
                    one_m
                },
                "{id}"
            );
        }
        // 标记大小写不敏感。
        assert_eq!(
            decode(Client::ClaudeCode, "qbg-claude-k--m[1m]"),
            Decoded::Agg {
                key: "k",
                model: "m",
                one_m: true
            }
        );
    }

    #[test]
    fn codex_ids_round_trip() {
        let id = encode(Client::Codex, "deepseek", "deepseek/deepseek-v4-pro", false);
        assert_eq!(id, "qbg-deepseek/deepseek/deepseek-v4-pro");
        assert_eq!(
            decode(Client::Codex, &id),
            Decoded::Agg {
                key: "deepseek",
                model: "deepseek/deepseek-v4-pro",
                one_m: false
            }
        );
    }

    #[test]
    fn desktop_ids_are_stable_six_digit_codes() {
        let id = encode(Client::ClaudeDesktop, "kimi", "kimi-k3", false);
        assert!(id.starts_with(DESKTOP_PREFIX));
        let code = &id[DESKTOP_PREFIX.len()..];
        assert_eq!(code.len(), 6);
        assert_eq!(
            code,
            desktop_code("kimi", "kimi-k3"),
            "同样的输入永远同样的数字"
        );
        assert_eq!(decode(Client::ClaudeDesktop, &id), Decoded::Code(code));
        assert_eq!(
            decode(Client::ClaudeDesktop, &format!("{id}[1m]")),
            Decoded::Code(code)
        );
        assert_eq!(strip_desktop_one_m(&format!("{id}[1m]")), id);
        assert_eq!(strip_desktop_one_m(&id), id);
        assert_eq!(
            decode(Client::ClaudeDesktop, "claude-fable-qbg-12345"),
            Decoded::Malformed
        );
    }

    #[test]
    fn plain_ids_and_other_clients_are_not_decoded() {
        assert_eq!(
            decode(Client::ClaudeCode, "claude-sonnet-5"),
            Decoded::Plain
        );
        assert_eq!(decode(Client::ClaudeCode, "kimi-k3"), Decoded::Plain);
        assert_eq!(
            decode(Client::Codex, "deepseek/deepseek-v4-pro"),
            Decoded::Plain
        );
        assert_eq!(
            decode(Client::Codex, "qbg-without-separator"),
            Decoded::Plain
        );
        assert_eq!(
            decode(Client::ClaudeDesktop, "qbg-claude-k--m"),
            Decoded::Plain
        );
        assert_eq!(decode(Client::Antigravity, "qbg-k/m"), Decoded::Plain);
    }

    #[test]
    fn reserved_ids_that_do_not_split_are_malformed() {
        assert_eq!(
            decode(Client::ClaudeCode, "qbg-claude-kimi"),
            Decoded::Malformed
        );
        assert_eq!(
            decode(Client::ClaudeCode, "qbg-claude---m"),
            Decoded::Malformed
        );
        assert_eq!(
            decode(Client::ClaudeCode, "qbg-claude-k--"),
            Decoded::Malformed
        );
        assert_eq!(decode(Client::Codex, "qbg-/m"), Decoded::Malformed);
        assert_eq!(decode(Client::Codex, "qbg-k/"), Decoded::Malformed);
    }

    #[test]
    fn a_line_publishes_each_model_once_and_one_m_wins() {
        let models: Vec<String> = ["glm-5", "glm-5[1M]", "  ", "kimi-k3"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let p = publish(Client::ClaudeCode, "z", "r", "智谱 · 默认", &models);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].id, "qbg-claude-z--glm-5[1M]");
        assert_eq!(p[0].upstream, "glm-5[1M]");
        assert!(p[0].description.ends_with("1M"));
        assert_eq!(p[1].display_name, "kimi-k3（智谱 · 默认）");
        assert!(p[1].description.ends_with("200K"));
    }

    #[test]
    fn discovery_is_recognized_and_answered_in_anthropic_shape() {
        assert!(is_model_discovery("/v1/models?limit=1000", false));
        assert!(is_model_discovery("/v1/models", true));
        assert!(!is_model_discovery(
            "/v1/models?client_version=0.1&limit=5",
            false
        ));
        assert!(!is_model_discovery("/v1/messages", true));
        let p = publish(
            Client::ClaudeDesktop,
            "k",
            "r",
            "站",
            &["m[1m]".to_string()],
        );
        let body = discovery_body(Client::ClaudeDesktop, &p);
        assert_eq!(body["data"][0]["supports1m"], true);
        assert_eq!(body["has_more"], false);
        assert_eq!(body["last_id"], body["data"][0]["id"]);
        let miss = miss_body(Client::Codex, &Miss::Removed.message("qbg-k/m"));
        assert_eq!(miss["error"]["code"], "model_not_found");
    }
}
