// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 共存型工具（中转站重构阶段 4，cc-switch 的 C3 / C4）：把一条线路写进 OpenCode、MiniMax Code、OpenClaw、Hermes、Pi
//! 各自的配置文件，跟使用者自己配的供应商**并排共存**，在工具里选。
//!
//! ⛔ **这里写的是真 Key**（使用者 2026-10-25 定的：照 cc-switch 直连写真 Key）。这五个工具不经本机路由，
//! 站点地址和 Key 明文写进它们自己的配置文件 —— 「Key 不出后端」只对 Claude Code / Codex / 桌面端 / Gemini CLI /
//! Grok Build（经本机路由、占位 Key）成立。仓库 `CLAUDE.md`、DISCLAIMER 写明了。
//!
//! 只动 QB Gate 自己写的那几条（id 以 [`ID_PREFIX`] 开头）：使用者自己的供应商、别的设置、注释一概不碰；
//! 原文件解析不了就报错、一个字不写（[`crate::live`] 的规矩）。
//!
//! | 工具 | 文件 | 那一条在哪 | 格式 |
//! |---|---|---|---|
//! | OpenCode | `~\.config\opencode\opencode.jsonc`（没有就 `.json`） | `provider.<id>` | JSONC（按原样改写） |
//! | MiniMax Code | `~\.minimax\config.yaml`（认 `MINIMAX_DATA_DIR` / `MAVIS_DATA_DIR`） | `custom_provider.<id>` | YAML（换那一个顶层小节） |
//! | OpenClaw | `~\.openclaw\openclaw.json` | `models.providers.<id>` | JSON5（按原样改写） |
//! | Hermes | `~\.hermes\config.yaml` | `custom_providers` 列表里 `name = <id>` 的那一项 | YAML（换那一个顶层小节） |
//! | Pi | `~\.pi\agent\models.json` | `providers.<id>` | JSON（保序补丁） |
//!
//! 每个工具那一条的形状照 cc-switch 的写法（`opencode_config.rs`、`mcode_config.rs`、`openclaw_config.rs`、
//! `hermes_config.rs`、`pi_config/`，以及它们的预设）。

use crate::error::{GateError, Result};
use crate::live::jsonc_document::JsoncDocument;
use crate::live::patch::json::JsonPatch;
use crate::live::patch::{yaml, KeyPath, LivePatch};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// QB Gate 写进去的那几条的 id 前缀。只认、只动、只删这样开头的。
pub const ID_PREFIX: &str = "qb-gate-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum CoexistTool {
    OpenCode,
    /// 不照 kebab-case 拆成 `mini-max-code`：这个工具自己写作 MiniMax。
    #[serde(rename = "minimax-code")]
    MiniMaxCode,
    OpenClaw,
    Hermes,
    Pi,
}

impl CoexistTool {
    pub const ALL: [CoexistTool; 5] = [
        CoexistTool::OpenCode,
        CoexistTool::MiniMaxCode,
        CoexistTool::OpenClaw,
        CoexistTool::Hermes,
        CoexistTool::Pi,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CoexistTool::OpenCode => "OpenCode",
            CoexistTool::MiniMaxCode => "MiniMax Code",
            CoexistTool::OpenClaw => "OpenClaw",
            CoexistTool::Hermes => "Hermes",
            CoexistTool::Pi => "Pi",
        }
    }

    /// 配置文件的候选（先有的那个算数；都没有就用第一个）。`env` 读环境变量（测试里给假的）。
    pub fn config_candidates(
        self,
        home: &Path,
        env: impl Fn(&str) -> Option<String>,
    ) -> Vec<PathBuf> {
        match self {
            CoexistTool::OpenCode => {
                let dir = home.join(".config").join("opencode");
                vec![dir.join("opencode.jsonc"), dir.join("opencode.json")]
            }
            CoexistTool::MiniMaxCode => {
                let dir = ["MINIMAX_DATA_DIR", "MAVIS_DATA_DIR"]
                    .into_iter()
                    .filter_map(&env)
                    .map(|v| v.trim().to_string())
                    .find(|v| !v.is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".minimax"));
                vec![dir.join("config.yaml")]
            }
            CoexistTool::OpenClaw => vec![home.join(".openclaw").join("openclaw.json")],
            CoexistTool::Hermes => vec![home.join(".hermes").join("config.yaml")],
            CoexistTool::Pi => vec![home.join(".pi").join("agent").join("models.json")],
        }
    }

    /// 这个工具的配置目录在不在（不在多半是没装：不替它建）。
    pub fn config_dir(self, home: &Path, env: impl Fn(&str) -> Option<String>) -> PathBuf {
        self.config_candidates(home, env)[0]
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home.to_path_buf())
    }
}

/// 写进去的那一条说什么协议（由线路的上游协议定）。Gemini 原生这五个工具都不收。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum CoexistProtocol {
    Anthropic,
    OpenaiChat,
    OpenaiResponses,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoexistModel {
    pub id: String,
    pub name: Option<String>,
}

/// 要写进去的一条。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoexistEntry {
    /// 以 [`ID_PREFIX`] 开头。
    pub id: String,
    /// 工具里显示的名字（`站点 · 分组`）。
    pub name: String,
    /// QB Gate 的站点基地址（不带 `/v1`，本机路由也是这么拼的）。
    pub base_url: String,
    /// ⛔ 真 Key。
    pub api_key: String,
    pub protocol: CoexistProtocol,
    pub models: Vec<CoexistModel>,
}

/// 一条线路在共存型工具里的 id：`qb-gate-<站点>-<分组>`（ASCII 小写），后面跟 6 位散列防撞。
pub fn entry_id(station: &str, group: &str, route_id: &str) -> String {
    let slug = |s: &str| {
        let mut out = String::new();
        for c in s.chars() {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                out.push(c);
            } else if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
        }
        out.trim_matches('-').chars().take(20).collect::<String>()
    };
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in route_id.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let parts: Vec<String> = [slug(station), slug(group)]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    let stem = if parts.is_empty() {
        "line".to_string()
    } else {
        parts.join("-")
    };
    format!("{ID_PREFIX}{stem}-{:06x}", hash & 0xff_ffff)
}

fn bare(base: &str) -> &str {
    let base = base.trim().trim_end_matches('/');
    base.strip_suffix("/v1").unwrap_or(base)
}

/// 这个工具这一种协议要的基地址：OpenAI 那两种都带 `/v1`；Anthropic 只有 OpenCode（`@ai-sdk/anthropic`）带，
/// 别的三个自己接 `/v1/messages`（照 cc-switch 各工具预设里的写法）。
pub fn tool_base_url(tool: CoexistTool, protocol: CoexistProtocol, base: &str) -> String {
    let root = bare(base);
    match (protocol, tool) {
        (CoexistProtocol::Anthropic, CoexistTool::OpenCode) => format!("{root}/v1"),
        (CoexistProtocol::Anthropic, _) => root.to_string(),
        _ => format!("{root}/v1"),
    }
}

fn utf8(bytes: Vec<u8>) -> Result<String> {
    String::from_utf8(bytes).map_err(|e| GateError::Other(e.to_string()))
}

fn own(id: &str) -> Result<()> {
    if id.starts_with(ID_PREFIX) {
        Ok(())
    } else {
        Err(GateError::Other(format!(
            "只动 QB Gate 自己写的那几条（{ID_PREFIX}…），{id} 不是"
        )))
    }
}

// ------------------------------------------------------------------ 每个工具那一条的形状

fn opencode_entry(e: &CoexistEntry) -> Value {
    let npm = match e.protocol {
        CoexistProtocol::Anthropic => "@ai-sdk/anthropic",
        CoexistProtocol::OpenaiChat => "@ai-sdk/openai-compatible",
        CoexistProtocol::OpenaiResponses => "@ai-sdk/openai",
    };
    let mut models = Map::new();
    for m in &e.models {
        models.insert(
            m.id.clone(),
            json!({ "name": m.name.clone().unwrap_or_else(|| m.id.clone()) }),
        );
    }
    json!({
        "npm": npm,
        "name": e.name,
        "options": {
            "baseURL": tool_base_url(CoexistTool::OpenCode, e.protocol, &e.base_url),
            "apiKey": e.api_key,
        },
        "models": models,
    })
}

fn api_kind(p: CoexistProtocol) -> &'static str {
    match p {
        CoexistProtocol::Anthropic => "anthropic-messages",
        CoexistProtocol::OpenaiChat => "openai-completions",
        CoexistProtocol::OpenaiResponses => "openai-responses",
    }
}

fn model_list(e: &CoexistEntry) -> Value {
    Value::Array(
        e.models
            .iter()
            .map(|m| json!({ "id": m.id, "name": m.name.clone().unwrap_or_else(|| m.id.clone()) }))
            .collect(),
    )
}

fn openclaw_entry(e: &CoexistEntry) -> Value {
    json!({
        "baseUrl": tool_base_url(CoexistTool::OpenClaw, e.protocol, &e.base_url),
        "apiKey": e.api_key,
        "api": api_kind(e.protocol),
        "models": model_list(e),
    })
}

fn pi_entry(e: &CoexistEntry) -> Value {
    json!({
        "name": e.name,
        "baseUrl": tool_base_url(CoexistTool::Pi, e.protocol, &e.base_url),
        "api": api_kind(e.protocol),
        "apiKey": e.api_key,
        "models": model_list(e),
    })
}

fn mcode_entry(e: &CoexistEntry) -> Result<serde_yaml_ng::Value> {
    let mut models = Map::new();
    for m in &e.models {
        models.insert(
            m.id.clone(),
            json!({ "name": m.name.clone().unwrap_or_else(|| m.id.clone()) }),
        );
    }
    to_yaml(&json!({
        "kind": "custom",
        "name": e.name,
        "api": api_kind(e.protocol),
        "options": {
            "baseURL": tool_base_url(CoexistTool::MiniMaxCode, e.protocol, &e.base_url),
            "apiKey": e.api_key,
        },
        "models": models,
    }))
}

fn hermes_entry(e: &CoexistEntry) -> Result<serde_yaml_ng::Value> {
    let mode = match e.protocol {
        CoexistProtocol::Anthropic => "anthropic_messages",
        CoexistProtocol::OpenaiChat => "chat_completions",
        CoexistProtocol::OpenaiResponses => "codex_responses",
    };
    let mut models = Map::new();
    for m in &e.models {
        models.insert(m.id.clone(), json!({}));
    }
    let mut v = json!({
        "name": e.id,
        "base_url": tool_base_url(CoexistTool::Hermes, e.protocol, &e.base_url),
        "api_key": e.api_key,
        "api_mode": mode,
        "models": models,
    });
    // Hermes 读单数的 `model` 当这一家的默认模型（cc-switch 的 `set_provider` 同样补上它）。
    if let Some(first) = e.models.first() {
        v["model"] = Value::String(first.id.clone());
    }
    to_yaml(&v)
}

fn to_yaml(v: &Value) -> Result<serde_yaml_ng::Value> {
    serde_yaml_ng::to_value(v).map_err(|e| GateError::Other(format!("写不出 YAML：{e}")))
}

// ------------------------------------------------------------------ 读写

/// 在一份 JSONC / JSON5 里把 `path` 那一处设成 `value`（`None` = 删掉），按原样改写。
pub(crate) fn jsonc_set(existing: &str, path: &[&str], value: Option<Value>) -> Result<String> {
    let source = if existing.trim().is_empty() {
        "{}\n"
    } else {
        existing
    };
    let mut doc = JsoncDocument::parse(source)?;
    let mut desired = doc.value().clone();
    let (last, parents) = path.split_last().expect("非空路径");
    let mut cursor = &mut desired;
    for key in parents {
        if !cursor.get(*key).is_some_and(Value::is_object) {
            if cursor.get(*key).is_some() {
                return Err(GateError::Other(format!(
                    "{} 不是对象。为避免覆盖你的配置，本次没有写入任何文件",
                    path.join(".")
                )));
            }
            cursor[*key] = json!({});
        }
        cursor = cursor.get_mut(*key).expect("刚建的");
    }
    let obj = cursor.as_object_mut().expect("上面查过是对象");
    match value {
        Some(v) => {
            obj.insert(last.to_string(), v);
        }
        None => {
            obj.shift_remove(*last);
        }
    }
    doc.apply(&desired)?;
    doc.validated_source()
}

fn jsonc_keys(existing: &str, path: &[&str]) -> Result<Vec<String>> {
    if existing.trim().is_empty() {
        return Ok(Vec::new());
    }
    let value: Value = json5::from_str(existing)
        .map_err(|e| GateError::Other(format!("无法解析这份配置：{e}")))?;
    Ok(path
        .iter()
        .try_fold(&value, |v, k| v.get(*k))
        .and_then(Value::as_object)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default())
}

/// 把这一条写进（或更新）这个工具的配置，返回新的全文。
pub fn upsert(tool: CoexistTool, existing: &str, e: &CoexistEntry) -> Result<String> {
    own(&e.id)?;
    if e.models.is_empty() {
        return Err(GateError::Other(
            "这条线没有可用的模型：先在线路弹窗的「模型映射」里写上要用的模型".into(),
        ));
    }
    match tool {
        CoexistTool::OpenCode => jsonc_set(existing, &["provider", &e.id], Some(opencode_entry(e))),
        CoexistTool::OpenClaw => jsonc_set(
            existing,
            &["models", "providers", &e.id],
            Some(openclaw_entry(e)),
        ),
        CoexistTool::Pi => {
            let patch = JsonPatch {
                set: vec![(KeyPath::new(&["providers", &e.id]), pi_entry(e))],
                ..JsonPatch::default()
            };
            utf8(patch.apply(Path::new("models.json"), Some(existing.as_bytes()))?)
        }
        CoexistTool::MiniMaxCode => {
            let mut section = match yaml::section(existing, "custom_provider")? {
                Some(serde_yaml_ng::Value::Mapping(m)) => m,
                Some(serde_yaml_ng::Value::Null) | None => Default::default(),
                Some(_) => {
                    return Err(GateError::Other(
                        "MiniMax Code 的 custom_provider 不是映射。为避免覆盖你的配置，本次没有写入任何文件".into(),
                    ))
                }
            };
            section.insert(serde_yaml_ng::Value::String(e.id.clone()), mcode_entry(e)?);
            yaml::replace_section(
                existing,
                "custom_provider",
                &serde_yaml_ng::Value::Mapping(section),
            )
        }
        CoexistTool::Hermes => {
            let mut list = hermes_list(existing)?;
            let fresh = hermes_entry(e)?;
            match list.iter_mut().find(|p| {
                p.get("name").and_then(serde_yaml_ng::Value::as_str) == Some(e.id.as_str())
            }) {
                Some(slot) => {
                    // 使用者在 Hermes 里给这一条加的字段（超时、`key_env`……）留着（cc-switch 同样合并）。
                    let mut merged = fresh;
                    if let (Some(old), serde_yaml_ng::Value::Mapping(new)) =
                        (slot.as_mapping(), &mut merged)
                    {
                        for (k, v) in old {
                            new.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }
                    *slot = merged;
                }
                None => list.push(fresh),
            }
            yaml::replace_section(
                existing,
                "custom_providers",
                &serde_yaml_ng::Value::Sequence(list),
            )
        }
    }
}

fn hermes_list(existing: &str) -> Result<Vec<serde_yaml_ng::Value>> {
    match yaml::section(existing, "custom_providers")? {
        Some(serde_yaml_ng::Value::Sequence(s)) => Ok(s),
        Some(serde_yaml_ng::Value::Null) | None => Ok(Vec::new()),
        Some(_) => Err(GateError::Other(
            "Hermes 的 custom_providers 不是列表。为避免覆盖你的配置，本次没有写入任何文件".into(),
        )),
    }
}

/// 把 QB Gate 写的那一条删掉，返回新的全文。没有就原样返回。
pub fn remove(tool: CoexistTool, existing: &str, id: &str) -> Result<String> {
    own(id)?;
    if !written_ids(tool, existing)?.iter().any(|x| x == id) {
        return Ok(existing.to_string());
    }
    match tool {
        CoexistTool::OpenCode => jsonc_set(existing, &["provider", id], None),
        CoexistTool::OpenClaw => jsonc_set(existing, &["models", "providers", id], None),
        CoexistTool::Pi => {
            let patch = JsonPatch {
                remove: vec![KeyPath::new(&["providers", id])],
                ..JsonPatch::default()
            };
            utf8(patch.apply(Path::new("models.json"), Some(existing.as_bytes()))?)
        }
        CoexistTool::MiniMaxCode => {
            let Some(serde_yaml_ng::Value::Mapping(mut section)) =
                yaml::section(existing, "custom_provider")?
            else {
                return Ok(existing.to_string());
            };
            section.remove(serde_yaml_ng::Value::String(id.to_string()));
            yaml::replace_section(
                existing,
                "custom_provider",
                &serde_yaml_ng::Value::Mapping(section),
            )
        }
        CoexistTool::Hermes => {
            let list: Vec<serde_yaml_ng::Value> = hermes_list(existing)?
                .into_iter()
                .filter(|p| p.get("name").and_then(serde_yaml_ng::Value::as_str) != Some(id))
                .collect();
            yaml::replace_section(
                existing,
                "custom_providers",
                &serde_yaml_ng::Value::Sequence(list),
            )
        }
    }
}

/// 这份配置里 QB Gate 写过的那几条的 id。
pub fn written_ids(tool: CoexistTool, existing: &str) -> Result<Vec<String>> {
    let all: Vec<String> = match tool {
        CoexistTool::OpenCode => jsonc_keys(existing, &["provider"])?,
        CoexistTool::OpenClaw => jsonc_keys(existing, &["models", "providers"])?,
        CoexistTool::Pi => {
            if existing.trim().is_empty() {
                Vec::new()
            } else {
                let v: Value = serde_json::from_str(existing.trim_start_matches('\u{feff}'))
                    .map_err(|e| GateError::Other(format!("无法解析 models.json：{e}")))?;
                v.get("providers")
                    .and_then(Value::as_object)
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default()
            }
        }
        CoexistTool::MiniMaxCode => match yaml::section(existing, "custom_provider")? {
            Some(serde_yaml_ng::Value::Mapping(m)) => m
                .keys()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        },
        CoexistTool::Hermes => hermes_list(existing)?
            .iter()
            .filter_map(|p| {
                p.get("name")
                    .and_then(serde_yaml_ng::Value::as_str)
                    .map(str::to_string)
            })
            .collect(),
    };
    Ok(all
        .into_iter()
        .filter(|id| id.starts_with(ID_PREFIX))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(protocol: CoexistProtocol) -> CoexistEntry {
        CoexistEntry {
            id: entry_id("DeepSeek", "默认", "claude-code\u{1f}ds\u{1f}默认"),
            name: "DeepSeek · 默认".into(),
            base_url: "https://api.deepseek.com/anthropic".into(),
            api_key: "sk-real".into(),
            protocol,
            models: vec![
                CoexistModel {
                    id: "deepseek-v4-pro".into(),
                    name: None,
                },
                CoexistModel {
                    id: "deepseek-flash".into(),
                    name: Some("Flash".into()),
                },
            ],
        }
    }

    #[test]
    fn ids_are_prefixed_ascii_and_stable() {
        let a = entry_id("DeepSeek", "默认", "r1");
        assert!(a.starts_with("qb-gate-deepseek-"), "{a}");
        assert_eq!(a, entry_id("DeepSeek", "默认", "r1"));
        assert_ne!(a, entry_id("DeepSeek", "默认", "r2"), "同名的两条线不许撞");
        assert!(entry_id("智谱", "默认", "r").starts_with("qb-gate-line-"));
    }

    #[test]
    fn base_urls_follow_each_tool_s_convention() {
        let b = "https://api.deepseek.com/anthropic/";
        assert_eq!(
            tool_base_url(CoexistTool::OpenCode, CoexistProtocol::Anthropic, b),
            "https://api.deepseek.com/anthropic/v1"
        );
        assert_eq!(
            tool_base_url(CoexistTool::Hermes, CoexistProtocol::Anthropic, b),
            "https://api.deepseek.com/anthropic"
        );
        assert_eq!(
            tool_base_url(
                CoexistTool::Pi,
                CoexistProtocol::OpenaiChat,
                "https://x.test/v1"
            ),
            "https://x.test/v1"
        );
        assert_eq!(
            tool_base_url(
                CoexistTool::OpenClaw,
                CoexistProtocol::OpenaiResponses,
                "https://x.test"
            ),
            "https://x.test/v1"
        );
    }

    #[test]
    fn opencode_keeps_comments_and_the_user_s_own_providers() {
        let existing = "{\n  // 我自己的\n  \"provider\": {\n    \"mine\": { \"npm\": \"@ai-sdk/openai-compatible\" },\n  },\n  \"theme\": \"tokyo\",\n}\n";
        let e = entry(CoexistProtocol::Anthropic);
        let out = upsert(CoexistTool::OpenCode, existing, &e).unwrap();
        assert!(out.contains("// 我自己的"), "注释丢了：{out}");
        let v: Value = json5::from_str(&out).unwrap();
        assert!(v["provider"]["mine"].is_object());
        assert_eq!(v["provider"][&e.id]["npm"], "@ai-sdk/anthropic");
        assert_eq!(
            v["provider"][&e.id]["options"]["apiKey"], "sk-real",
            "共存型写真 Key"
        );
        assert_eq!(
            v["provider"][&e.id]["models"]["deepseek-flash"]["name"],
            "Flash"
        );
        assert_eq!(
            written_ids(CoexistTool::OpenCode, &out).unwrap(),
            [e.id.clone()]
        );
        let back = remove(CoexistTool::OpenCode, &out, &e.id).unwrap();
        assert!(back.contains("// 我自己的"));
        assert!(written_ids(CoexistTool::OpenCode, &back)
            .unwrap()
            .is_empty());
        assert!(json5::from_str::<Value>(&back).unwrap()["provider"]["mine"].is_object());
    }

    #[test]
    fn openclaw_and_pi_put_the_line_under_their_providers() {
        let e = entry(CoexistProtocol::OpenaiChat);
        let out = upsert(
            CoexistTool::OpenClaw,
            "{ models: { providers: {} }, agents: {} }",
            &e,
        )
        .unwrap();
        let v: Value = json5::from_str(&out).unwrap();
        assert_eq!(v["models"]["providers"][&e.id]["api"], "openai-completions");
        assert_eq!(
            v["models"]["providers"][&e.id]["baseUrl"],
            "https://api.deepseek.com/anthropic/v1"
        );
        assert!(v["agents"].is_object());
        let out = upsert(
            CoexistTool::Pi,
            "{\n  \"providers\": {\n    \"external\": {}\n  }\n}\n",
            &e,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["providers"][&e.id]["models"][1]["name"], "Flash");
        assert!(v["providers"]["external"].is_object());
        assert!(written_ids(CoexistTool::Pi, &out).unwrap() == [e.id.clone()]);
    }

    #[test]
    fn yaml_tools_only_touch_their_own_section_and_entries() {
        let e = entry(CoexistProtocol::Anthropic);
        let hermes = "# hermes\nmodel:\n  default: gpt\ncustom_providers:\n- name: mine\n  base_url: https://mine.test\n";
        let out = upsert(CoexistTool::Hermes, hermes, &e).unwrap();
        assert!(
            out.starts_with("# hermes\nmodel:\n  default: gpt\n"),
            "{out}"
        );
        let list = hermes_list(&out).unwrap();
        assert_eq!(list.len(), 2, "使用者自己的那一家要留着");
        let ours = &list[1];
        assert_eq!(ours["api_mode"].as_str(), Some("anthropic_messages"));
        assert_eq!(ours["model"].as_str(), Some("deepseek-v4-pro"));
        assert_eq!(
            written_ids(CoexistTool::Hermes, &out).unwrap(),
            [e.id.clone()]
        );
        // 再写一次是更新，不是再追加一条。
        let again = upsert(CoexistTool::Hermes, &out, &e).unwrap();
        assert_eq!(hermes_list(&again).unwrap().len(), 2);
        let gone = remove(CoexistTool::Hermes, &again, &e.id).unwrap();
        assert_eq!(hermes_list(&gone).unwrap().len(), 1);

        let mcode = upsert(CoexistTool::MiniMaxCode, "theme: dark\n", &e).unwrap();
        assert!(mcode.starts_with("theme: dark\n"));
        let section = yaml::section(&mcode, "custom_provider").unwrap().unwrap();
        assert_eq!(
            section[e.id.as_str()]["options"]["apiKey"].as_str(),
            Some("sk-real")
        );
        assert_eq!(
            section[e.id.as_str()]["api"].as_str(),
            Some("anthropic-messages")
        );
    }

    #[test]
    fn only_qb_gate_entries_are_touched_and_broken_files_are_refused() {
        let e = entry(CoexistProtocol::Anthropic);
        assert!(remove(
            CoexistTool::OpenCode,
            "{\"provider\":{\"mine\":{}}}",
            "mine"
        )
        .is_err());
        let foreign = CoexistEntry {
            id: "mine".into(),
            ..e.clone()
        };
        assert!(upsert(CoexistTool::Pi, "{}", &foreign).is_err());
        assert!(upsert(CoexistTool::OpenCode, "{ broken", &e).is_err());
        assert!(upsert(CoexistTool::Hermes, "model: [x\n", &e).is_err());
        assert!(
            upsert(CoexistTool::OpenClaw, "{ models: 3 }", &e).is_err(),
            "models 不是对象"
        );
        let no_models = CoexistEntry {
            models: vec![],
            ..e
        };
        assert!(upsert(CoexistTool::Pi, "{}", &no_models).is_err());
    }

    #[test]
    fn minimax_honours_its_data_dir_variables() {
        let home = Path::new("C:/Users/demo");
        let none = |_: &str| None;
        assert_eq!(
            CoexistTool::MiniMaxCode.config_candidates(home, none)[0],
            home.join(".minimax").join("config.yaml")
        );
        let set = |k: &str| (k == "MAVIS_DATA_DIR").then(|| "D:/mavis".to_string());
        assert_eq!(
            CoexistTool::MiniMaxCode.config_candidates(home, set)[0],
            Path::new("D:/mavis").join("config.yaml")
        );
    }
}
