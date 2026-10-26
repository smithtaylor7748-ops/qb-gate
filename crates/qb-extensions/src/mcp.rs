// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 统一的 MCP 服务器（中转站重构阶段 5，cc-switch 的 M1）：一份定义，写进好几个客户端。
//!
//! 这里只管「一条服务器长什么样、在每个客户端的配置里该写成什么样」—— 纯值的转换，不碰文件。
//! 写进哪些地方（中转环境、官方目录、各个工具）、怎么写盘，在 `qb-app` 的 `usecase::mcp_ops`。
//!
//! 统一格式照 cc-switch：`{"type": "stdio" | "http" | "sse", "command", "args", "env", "cwd", "url", "headers", …}`，
//! `type` 省略算 stdio。各家的写法（[`Format`]）：
//!
//! | 格式 | 谁 | 差别 |
//! |---|---|---|
//! | `ClaudeJson` | Claude Code（`.claude.json` 的 `mcpServers`） | 原样；Windows 上 `npx` 一类包成 `cmd /c`（Claude Code 的 `/doctor` 会提示） |
//! | `GeminiJson` | Gemini CLI（`settings.json` 的 `mcpServers`） | 不写 `type`；http 用 `httpUrl`、sse 用 `url`；超时合成一个毫秒数 `timeout` |
//! | `CodexToml` | Codex（`config.toml` 的 `[mcp_servers.<id>]`） | 不写 `type`；headers 叫 `http_headers`；只写所选传输方式的字段（Codex 见到另一种的字段会拒绝整份配置） |
//! | `GrokToml` | Grok Build（`~\.grok\config.toml`） | 同 Codex，但 headers 就叫 `headers` |
//! | `OpenCode` | OpenCode（`mcp.<id>`） | `local` / `remote`；command 和 args 合成一个数组；env 叫 `environment` |
//! | `Hermes` | Hermes（`config.yaml` 的 `mcp_servers`） | 不写 `type`；它自己的字段（`timeout`、`tools`……）写的时候留着 |
//! | `MiniMax` | MiniMax Code（`mcp.json`） | 同 Claude 的形状，加 `enabled: true`；它自己的字段留着 |
//! | `Pi` | Pi（`mcp.json`） | 同上；不收 SSE；名字只能字母数字 `_` `-`，只差 `-` / `_` 的算同一个 |

use crate::error::{GateError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use ts_rs::TS;

/// 一条 MCP 服务器。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct McpServer {
    /// 在各家配置里的键名。
    pub id: String,
    pub name: String,
    /// 统一格式的连接定义（见模块头）。
    #[ts(type = "Record<string, unknown>")]
    pub server: Value,
    /// 写进了哪些地方（`usecase::mcp_ops` 的目标 id，比如 `claude-code:relay`）。
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// 服务器写进哪种配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    ClaudeJson,
    GeminiJson,
    CodexToml,
    GrokToml,
    OpenCode,
    Hermes,
    MiniMax,
    Pi,
}

/// 名字（键）能不能用：各家配置都拿它当键，Codex / Grok 还当 TOML 表名。
pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(GateError::Other(format!(
            "MCP 服务器名只能用字母、数字、_、- 和 .（最长 64）：{id}"
        )));
    }
    Ok(())
}

/// 传输方式。显式 `type` 优先；写了 stdio 却只有 `url` 的按 http（cc-switch 的 `spec_transport_type`）。
pub fn transport(spec: &Value) -> &str {
    let has = |k: &str| {
        spec.get(k)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty())
    };
    let explicit = spec.get("type").and_then(Value::as_str);
    if matches!(explicit, None | Some("stdio")) && !has("command") && has("url") {
        return "http";
    }
    explicit.unwrap_or("stdio")
}

/// 统一格式校验（cc-switch 的 `validate_server_spec`）：stdio 要 `command`，http / sse 要 `url`。
pub fn validate(spec: &Value) -> Result<()> {
    if !spec.is_object() {
        return Err(GateError::Other(
            "MCP 服务器的连接定义必须是 JSON 对象".into(),
        ));
    }
    let field = |k: &str| {
        spec.get(k)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty())
    };
    match transport(spec) {
        "stdio" if !field("command") => Err(GateError::Other(
            "stdio 类型的 MCP 服务器缺少 command".into(),
        )),
        "http" | "sse" if !field("url") => Err(GateError::Other(
            "http / sse 类型的 MCP 服务器缺少 url".into(),
        )),
        "stdio" | "http" | "sse" => Ok(()),
        other => Err(GateError::Other(format!(
            "MCP 服务器的 type 只能是 stdio、http 或 sse：{other}"
        ))),
    }
}

/// 面板自己的字段，不写进任何客户端。
const UI_FIELDS: [&str; 8] = [
    "enabled",
    "source",
    "id",
    "name",
    "description",
    "tags",
    "homepage",
    "docs",
];

fn object(spec: &Value) -> Result<Map<String, Value>> {
    let mut o = spec
        .as_object()
        .cloned()
        .ok_or_else(|| GateError::Other("MCP 服务器的连接定义必须是 JSON 对象".into()))?;
    for k in UI_FIELDS {
        o.remove(k);
    }
    Ok(o)
}

fn nonempty_object(v: Option<&Value>) -> Option<Value> {
    v.filter(|v| v.as_object().is_some_and(|o| !o.is_empty()))
        .cloned()
}

/// Windows 上 `npx` / `npm` / `node` 一类其实是 `.cmd`，Claude Code 要求包成 `cmd /c`（cc-switch 的 `wrap_command_for_windows`）。
fn wrap_for_windows(o: &mut Map<String, Value>) {
    const WRAP: [&str; 7] = ["npx", "npm", "yarn", "pnpm", "node", "bun", "deno"];
    if o.get("type").and_then(Value::as_str).unwrap_or("stdio") != "stdio" {
        return;
    }
    let Some(cmd) = o.get("command").and_then(Value::as_str).map(str::to_string) else {
        return;
    };
    if cmd.eq_ignore_ascii_case("cmd") || cmd.eq_ignore_ascii_case("cmd.exe") {
        return;
    }
    let stem = std::path::Path::new(&cmd)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&cmd)
        .to_string();
    if !WRAP.iter().any(|w| stem.eq_ignore_ascii_case(w)) {
        return;
    }
    let mut args = vec![json!("/c"), json!(cmd)];
    args.extend(
        o.get("args")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    );
    o.insert("command".into(), json!("cmd"));
    o.insert("args".into(), Value::Array(args));
}

/// 统一格式 → 这家配置里的那一条（JSON / YAML 形状的几家）。`existing` 是那一条原来的样子：
/// Hermes / MiniMax / Pi 自己的字段从它带过来。TOML 两家用 [`to_toml`]。
pub fn to_native(
    format: Format,
    spec: &Value,
    existing: Option<&Value>,
    windows: bool,
) -> Result<Value> {
    validate(spec)?;
    let typ = transport(spec).to_string();
    let mut o = object(spec)?;
    Ok(match format {
        Format::ClaudeJson => {
            if windows {
                wrap_for_windows(&mut o);
            }
            Value::Object(o)
        }
        Format::GeminiJson => {
            o.remove("type");
            if typ == "http" {
                if let Some(url) = o.remove("url") {
                    o.insert("httpUrl".into(), url);
                }
            }
            // Gemini 只认一个毫秒数 `timeout`：两种超时取大的，没写按 10 秒 / 60 秒（cc-switch 同样）。
            let mut take = |k: &str, mul: f64| {
                o.remove(k)
                    .and_then(|v| v.as_f64())
                    .map(|n| (n * mul) as u64)
            };
            let startup = take("startup_timeout_sec", 1000.0)
                .or_else(|| take("startup_timeout_ms", 1.0))
                .unwrap_or(10_000);
            let tool = take("tool_timeout_sec", 1000.0)
                .or_else(|| take("tool_timeout_ms", 1.0))
                .unwrap_or(60_000);
            o.insert("timeout".into(), json!(startup.max(tool)));
            Value::Object(o)
        }
        Format::OpenCode => {
            let mut out = Map::new();
            if typ == "stdio" {
                out.insert("type".into(), json!("local"));
                let mut command = vec![o.get("command").cloned().unwrap_or(json!(""))];
                command.extend(
                    o.get("args")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                );
                out.insert("command".into(), Value::Array(command));
                if let Some(env) = nonempty_object(o.get("env")) {
                    out.insert("environment".into(), env);
                }
            } else {
                out.insert("type".into(), json!("remote"));
                out.insert("url".into(), o.get("url").cloned().unwrap_or(json!("")));
                if let Some(h) = nonempty_object(o.get("headers")) {
                    out.insert("headers".into(), h);
                }
            }
            out.insert("enabled".into(), json!(true));
            Value::Object(out)
        }
        Format::Hermes => {
            const EXTRA: [&str; 7] = [
                "enabled",
                "timeout",
                "connect_timeout",
                "tools",
                "sampling",
                "roots",
                "auth",
            ];
            let mut out = Map::new();
            if let Some(Value::Object(old)) = existing {
                for k in EXTRA {
                    if let Some(v) = old.get(k) {
                        out.insert(k.into(), v.clone());
                    }
                }
            }
            if typ == "stdio" {
                out.insert(
                    "command".into(),
                    o.get("command").cloned().unwrap_or(json!("")),
                );
                if let Some(a) = o
                    .get("args")
                    .filter(|a| a.as_array().is_some_and(|a| !a.is_empty()))
                {
                    out.insert("args".into(), a.clone());
                }
                if let Some(env) = nonempty_object(o.get("env")) {
                    out.insert("env".into(), env);
                }
            } else {
                out.insert("url".into(), o.get("url").cloned().unwrap_or(json!("")));
                if let Some(h) = nonempty_object(o.get("headers")) {
                    out.insert("headers".into(), h);
                }
            }
            out.insert("enabled".into(), json!(true));
            Value::Object(out)
        }
        Format::MiniMax | Format::Pi => {
            const TRANSPORT: [&str; 7] =
                ["command", "args", "env", "cwd", "url", "headers", "type"];
            if format == Format::Pi && typ == "sse" {
                return Err(GateError::Other(
                    "Pi 不支持 SSE 连接方式的 MCP 服务器，请改用 http".into(),
                ));
            }
            let mut out = existing
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            for k in TRANSPORT {
                out.remove(k);
            }
            for (k, v) in o {
                if TRANSPORT.contains(&k.as_str()) {
                    out.insert(k, v);
                }
            }
            out.insert("enabled".into(), json!(true));
            Value::Object(out)
        }
        Format::CodexToml | Format::GrokToml => {
            return Err(GateError::Other(
                "Codex / Grok Build 的条目用 to_toml".into(),
            ))
        }
    })
}

/// 这家配置里的一条 → 统一格式（导入用）。各家自己的字段（Hermes 的 `tools`、Pi 的 `exposure`……）不带进来，
/// 写回去时从原条目带。
pub fn from_native(format: Format, native: &Value) -> Result<Value> {
    let o = native
        .as_object()
        .ok_or_else(|| GateError::Other("这一条 MCP 不是对象".into()))?;
    let mut out = Map::new();
    let pick = |out: &mut Map<String, Value>, from: &str, to: &str| {
        if let Some(v) = o.get(from) {
            out.insert(to.into(), v.clone());
        }
    };
    match format {
        Format::OpenCode => {
            if o.get("type").and_then(Value::as_str).unwrap_or("local") == "local" {
                out.insert("type".into(), json!("stdio"));
                let cmd = o
                    .get("command")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if let Some(first) = cmd.first() {
                    out.insert("command".into(), first.clone());
                }
                if cmd.len() > 1 {
                    out.insert("args".into(), Value::Array(cmd[1..].to_vec()));
                }
                pick(&mut out, "environment", "env");
            } else {
                out.insert("type".into(), json!("sse"));
                pick(&mut out, "url", "url");
                pick(&mut out, "headers", "headers");
            }
        }
        _ => {
            for (k, v) in o {
                match (format, k.as_str()) {
                    (_, "enabled") => {}
                    (Format::GeminiJson, "httpUrl") => {
                        out.insert("url".into(), v.clone());
                        out.insert("type".into(), json!("http"));
                    }
                    (Format::CodexToml, "http_headers") => {
                        out.insert("headers".into(), v.clone());
                    }
                    (
                        Format::Hermes,
                        "timeout" | "connect_timeout" | "tools" | "sampling" | "roots" | "auth",
                    ) => {}
                    (
                        Format::Pi,
                        "timeout" | "description" | "exposure" | "toolExposure" | "oauth" | "auth",
                    ) => {}
                    _ => {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }
            if out.get("type").and_then(Value::as_str) == Some("streamable-http") {
                out.insert("type".into(), json!("http"));
            }
            if !out.contains_key("type") {
                let t = if out.contains_key("command") {
                    "stdio"
                } else if format == Format::GeminiJson {
                    "sse" // Gemini 只有 `url` 的是 SSE（http 用 `httpUrl`）
                } else {
                    "http"
                };
                out.insert("type".into(), json!(t));
            }
        }
    }
    let spec = Value::Object(out);
    validate(&spec)?;
    Ok(spec)
}

// ------------------------------------------------------------------ TOML（Codex、Grok Build）

fn toml_value(v: &Value) -> Option<toml_edit::Value> {
    Some(match v {
        Value::String(s) => s.as_str().into(),
        Value::Bool(b) => (*b).into(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.into(),
            None => n.as_f64()?.into(),
        },
        Value::Array(a) => {
            let mut arr = toml_edit::Array::new();
            for x in a {
                arr.push(toml_value(x)?);
            }
            toml_edit::Value::Array(arr)
        }
        Value::Object(o) => {
            let mut t = toml_edit::InlineTable::new();
            for (k, x) in o {
                t.insert(k, toml_value(x)?);
            }
            toml_edit::Value::InlineTable(t)
        }
        Value::Null => return None,
    })
}

/// 统一格式 → Codex / Grok Build 的那张表。⛔ 不写 `type`（Codex 0.158+ 当未知字段告警），
/// **只写所选传输方式的字段**：Codex 见到另一种的字段（比如 stdio 条目带 `url`）会拒绝整份配置，别的 MCP 也跟着失效。
pub fn to_toml(format: Format, spec: &Value) -> Result<toml_edit::Table> {
    validate(spec)?;
    let typ = transport(spec);
    let o = object(spec)?;
    let skip: &[&str] = if typ == "stdio" {
        &[
            "type",
            "url",
            "headers",
            "http_headers",
            "env_http_headers",
            "http_headers_helper",
            "bearer_token_env_var",
            "oauth_resource",
        ]
    } else {
        &["type", "command", "args", "env", "env_vars", "cwd"]
    };
    let mut t = toml_edit::Table::new();
    for (k, v) in &o {
        if skip.contains(&k.as_str()) {
            continue;
        }
        let key = match (format, k.as_str()) {
            (Format::CodexToml, "headers") => "http_headers",
            (Format::GrokToml, "http_headers") => "headers",
            _ => k.as_str(),
        };
        if let Some(value) = toml_value(v) {
            t.insert(key, toml_edit::Item::Value(value));
        }
    }
    Ok(t)
}

/// TOML 的一项 → JSON（读各家原样的条目用）。
pub fn toml_item_json(item: &toml_edit::Item) -> Option<Value> {
    fn value(v: &toml_edit::Value) -> Option<Value> {
        Some(match v {
            toml_edit::Value::String(s) => json!(s.value()),
            toml_edit::Value::Integer(i) => json!(*i.value()),
            toml_edit::Value::Float(f) => json!(*f.value()),
            toml_edit::Value::Boolean(b) => json!(*b.value()),
            toml_edit::Value::Datetime(d) => json!(d.value().to_string()),
            toml_edit::Value::Array(a) => Value::Array(a.iter().filter_map(value).collect()),
            toml_edit::Value::InlineTable(t) => Value::Object(
                t.iter()
                    .filter_map(|(k, v)| Some((k.to_string(), value(v)?)))
                    .collect(),
            ),
        })
    }
    match item {
        toml_edit::Item::Value(v) => value(v),
        toml_edit::Item::Table(t) => Some(Value::Object(
            t.iter()
                .filter_map(|(k, v)| Some((k.to_string(), toml_item_json(v)?)))
                .collect(),
        )),
        _ => None,
    }
}

/// Codex / Grok Build 的一张表 → 统一格式。
pub fn from_toml(format: Format, item: &toml_edit::Item) -> Result<Value> {
    let native =
        toml_item_json(item).ok_or_else(|| GateError::Other("这一条 MCP 不是表".into()))?;
    from_native(
        if format == Format::GrokToml {
            Format::GrokToml
        } else {
            Format::CodexToml
        },
        &native,
    )
}

/// `[mcp_servers]` 里换上 / 删掉一条（`table` 为 `None` = 删）。`mcp_servers` 是内联表也认（cc-switch 的 `as_table_like_mut`）；
/// 键存在却不是表就停，不替使用者重置。
pub fn toml_set(
    doc: &mut toml_edit::DocumentMut,
    id: &str,
    table: Option<toml_edit::Table>,
) -> Result<()> {
    match (doc.get_mut("mcp_servers"), table) {
        (Some(item), t) if !item.is_none() => {
            let servers = item.as_table_like_mut().ok_or_else(|| {
                GateError::Other(
                    "config.toml 的 mcp_servers 不是表：为避免覆盖你的配置，没有写".into(),
                )
            })?;
            match t {
                Some(t) => {
                    servers.insert(id, toml_edit::Item::Table(t));
                }
                None => {
                    servers.remove(id);
                }
            }
        }
        (_, Some(t)) => {
            let mut servers = toml_edit::Table::new();
            servers.set_implicit(true);
            servers.insert(id, toml_edit::Item::Table(t));
            doc.insert("mcp_servers", toml_edit::Item::Table(servers));
        }
        (_, None) => {}
    }
    Ok(())
}

/// `[mcp_servers]` 里有哪几条（导入、看状态用）。
pub fn toml_entries(doc: &toml_edit::DocumentMut) -> Vec<(String, toml_edit::Item)> {
    doc.get("mcp_servers")
        .and_then(|i| i.as_table_like())
        .map(|t| t.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
        .unwrap_or_default()
}

/// Pi 把只差 `-` / `_` 的名字当同一个；名字也只能字母数字 `_` `-`。
pub fn pi_name_ok(id: &str, existing: &[String]) -> Result<()> {
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(GateError::Other(format!(
            "Pi 的 MCP 服务器名只能包含字母、数字、_ 和 -：{id}"
        )));
    }
    let norm = |s: &str| s.replace('-', "_");
    if let Some(other) = existing
        .iter()
        .find(|n| n.as_str() != id && norm(n) == norm(id))
    {
        return Err(GateError::Other(format!(
            "Pi 把只差 - 和 _ 的服务器名当成同一个：{id} 跟已有的 {other} 冲突"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stdio() -> Value {
        json!({ "type": "stdio", "command": "npx", "args": ["-y", "pkg"], "env": { "K": "v" }, "description": "ui" })
    }
    fn http() -> Value {
        json!({ "type": "http", "url": "https://mcp.example/x", "headers": { "Authorization": "Bearer t" } })
    }

    #[test]
    fn validation_follows_the_transport() {
        assert!(validate(&stdio()).is_ok());
        assert!(validate(&json!({ "type": "stdio" })).is_err());
        assert!(validate(&json!({ "type": "sse" })).is_err());
        assert!(validate(&json!({ "type": "ws", "url": "x" })).is_err());
        assert_eq!(
            transport(&json!({ "url": "https://x" })),
            "http",
            "只有 url 的按 http"
        );
        assert!(validate_id("my.server-1_a").is_ok());
        assert!(validate_id("a b").is_err());
    }

    #[test]
    fn claude_wraps_npx_on_windows_and_drops_ui_fields() {
        let v = to_native(Format::ClaudeJson, &stdio(), None, true).unwrap();
        assert_eq!(v["command"], "cmd");
        assert_eq!(v["args"], json!(["/c", "npx", "-y", "pkg"]));
        assert!(v.get("description").is_none());
        let plain = to_native(Format::ClaudeJson, &stdio(), None, false).unwrap();
        assert_eq!(plain["command"], "npx");
    }

    #[test]
    fn gemini_uses_http_url_and_one_timeout() {
        let v = to_native(Format::GeminiJson, &http(), None, false).unwrap();
        assert_eq!(v["httpUrl"], "https://mcp.example/x");
        assert!(v.get("type").is_none() && v.get("url").is_none());
        assert_eq!(v["timeout"], 60_000);
        let back = from_native(Format::GeminiJson, &v).unwrap();
        assert_eq!(back["type"], "http");
        let sse = from_native(Format::GeminiJson, &json!({ "url": "https://x" })).unwrap();
        assert_eq!(sse["type"], "sse");
    }

    #[test]
    fn opencode_round_trips_local_and_remote() {
        let v = to_native(Format::OpenCode, &stdio(), None, false).unwrap();
        assert_eq!(
            v,
            json!({ "type": "local", "command": ["npx", "-y", "pkg"], "environment": { "K": "v" }, "enabled": true })
        );
        let back = from_native(Format::OpenCode, &v).unwrap();
        assert_eq!(back["command"], "npx");
        assert_eq!(back["args"], json!(["-y", "pkg"]));
        assert_eq!(back["env"], json!({ "K": "v" }));
        let r = to_native(Format::OpenCode, &http(), None, false).unwrap();
        assert_eq!(r["type"], "remote");
    }

    #[test]
    fn hermes_minimax_and_pi_keep_their_own_fields() {
        let old = json!({ "command": "old", "tools": { "include": ["a"] }, "timeout": 30 });
        let v = to_native(Format::Hermes, &stdio(), Some(&old), false).unwrap();
        assert_eq!(v["command"], "npx");
        assert_eq!(
            v["tools"],
            json!({ "include": ["a"] }),
            "Hermes 自己的字段留着"
        );
        assert!(v.get("type").is_none());
        let back = from_native(Format::Hermes, &v).unwrap();
        assert!(back.get("tools").is_none(), "导入时不带进来");

        let old = json!({ "command": "old", "exposure": "direct", "enabled": false });
        let v = to_native(Format::Pi, &stdio(), Some(&old), false).unwrap();
        assert_eq!(v["exposure"], "direct");
        assert_eq!(v["enabled"], true);
        assert!(to_native(
            Format::Pi,
            &json!({ "type": "sse", "url": "https://x" }),
            None,
            false
        )
        .is_err());
        assert!(pi_name_ok("a-b", &["a_b".into()]).is_err());
        assert!(pi_name_ok("a-b", &["a-b".into()]).is_ok());
    }

    #[test]
    fn codex_tables_only_carry_the_chosen_transport() {
        let mixed = json!({ "type": "stdio", "command": "npx", "url": "https://stray", "startup_timeout_sec": 20 });
        let t = to_toml(Format::CodexToml, &mixed).unwrap();
        assert!(
            t.get("url").is_none(),
            "stdio 条目不带 url（Codex 会拒绝整份配置）"
        );
        assert!(t.get("type").is_none());
        assert_eq!(t["startup_timeout_sec"].as_integer(), Some(20));
        let h = to_toml(Format::CodexToml, &http()).unwrap();
        assert!(h.get("http_headers").is_some() && h.get("headers").is_none());
        let g = to_toml(Format::GrokToml, &http()).unwrap();
        assert!(g.get("headers").is_some(), "Grok 叫 headers");

        let mut doc: toml_edit::DocumentMut = "model = \"x\"\n".parse().unwrap();
        toml_set(
            &mut doc,
            "fs",
            Some(to_toml(Format::CodexToml, &stdio()).unwrap()),
        )
        .unwrap();
        let entries = toml_entries(&doc);
        assert_eq!(entries[0].0, "fs");
        let back = from_toml(Format::CodexToml, &entries[0].1).unwrap();
        assert_eq!(back["command"], "npx");
        assert_eq!(back["type"], "stdio");
        toml_set(&mut doc, "fs", None).unwrap();
        assert!(toml_entries(&doc).is_empty());
        let mut bad: toml_edit::DocumentMut = "mcp_servers = \"x\"\n".parse().unwrap();
        assert!(
            toml_set(&mut bad, "fs", Some(toml_edit::Table::new())).is_err(),
            "不是表就停"
        );
        let mut inline: toml_edit::DocumentMut = "mcp_servers = { fs = { command = \"a\" } }\n"
            .parse()
            .unwrap();
        toml_set(&mut inline, "fs", None).unwrap();
        assert!(toml_entries(&inline).is_empty(), "内联表也删得掉");
    }
}
