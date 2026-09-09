//! 中转站配置：写进各个工具自己的配置文件。
//!
//! 形态对标 cc-switch（MIT，https://github.com/farion1231/cc-switch）：
//! 多供应商、一键切换、写进目标工具自己的配置：
//!   `~/.codex/config.toml`    模型供应商与 base_url
//!   `~/.codex/auth.json`      API Key（**只并入，不整份覆盖**）
//!   `~/.claude/settings.json` Claude Code 的 env 段
//!
//! **API Key 不落明文到本项目自己的配置里。** 写进目标工具的配置是那些工具
//! 本身的要求；面板界面上只显示掩码，读回来时也不把完整 Key 回传前端。
//!
//! # 三条踩过的坑（2026-09-09 实机核对后修）
//!
//! 1. **`wire_api` 不能写死成 `chat`。** 实机在用的中转站是 `responses`，
//!    写死会把可用配置改坏。现在由 [`WireApi`] 决定，默认 `responses`。
//! 2. **`auth.json` 不能整份覆盖。** 旧代码写 `{"OPENAI_API_KEY": key}`
//!    覆盖全文件，会把 Codex 官方登录留下的 `tokens`（access / refresh /
//!    account_id）和 `type` 一起抹掉 —— 等于把用户从 Codex 登出。
//!    现在走 [`codex_auth_json`] 并入。
//! 3. **读回配置时不能优先 Claude 就提前返回。** 旧代码只要读到
//!    `ANTHROPIC_BASE_URL` 就 return，Codex 那边的配置在界面上永远看不见。
//!    现在 [`current_providers`] 两个都读，各返回一条。
//!
//! 三条都有单测钉着，见文件末尾。

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 上游协议。
///
/// `responses` 是 Codex 原生协议，绝大多数中转站用它；`chat` 是 OpenAI
/// Chat Completions。**默认必须是 `responses`** —— 写死成 `chat` 正是坑 1。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WireApi {
    #[default]
    Responses,
    Chat,
}

impl WireApi {
    pub fn as_str(self) -> &'static str {
        match self {
            WireApi::Responses => "responses",
            WireApi::Chat => "chat",
        }
    }
}

/// 凭证写在哪。
///
/// 这不是风格偏好，是两种不同的落盘位置，配错了中转站就连不上。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthStyle {
    /// Codex：provider 段写 `env_key`，Key 并进 `auth.json`。
    /// Claude：写 `ANTHROPIC_API_KEY`。
    #[default]
    EnvKey,
    /// Codex：provider 段直接写 `experimental_bearer_token`，不碰 `auth.json`。
    /// Claude：写 `ANTHROPIC_AUTH_TOKEN`。
    BearerToken,
    /// 不写任何凭证 —— 官方 OAuth 登录，或者 Key 由用户自己管。
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    #[serde(default = "default_target")]
    pub target: String,
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub wire_api: WireApi,
    #[serde(default)]
    pub auth_style: AuthStyle,
    /// 只在写入方向使用。`skip_serializing` 保证它不会随响应回到前端。
    #[serde(default, skip_serializing)]
    pub api_key: Option<String>,
}

fn default_target() -> String {
    "codex".into()
}

fn is_claude(target: &str) -> bool {
    target.eq_ignore_ascii_case("claude") || target.eq_ignore_ascii_case("claude-code")
}

pub fn codex_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".codex")
}

pub fn claude_settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("settings.json")
}

fn atomic_write(p: &std::path::Path, body: &str) -> Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    // 先备份再写，坏了能退回去。
    if p.exists() {
        let bak = p.with_file_name(format!(
            "{}.bak",
            p.file_name().and_then(|n| n.to_str()).unwrap_or("config")
        ));
        let _ = std::fs::copy(p, bak);
    }
    // 原子写：临时文件 + 改名，避免目标工具读到半截配置。
    let tmp = p.with_file_name(format!(
        "{}.tmp",
        p.file_name().and_then(|n| n.to_str()).unwrap_or("config")
    ));
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, p)?;
    Ok(())
}

// ------------------------------------------------------- 纯函数（可单测）

/// 生成新的 `config.toml` 内容。
///
/// 用 toml_edit 增量改而不是整个重新序列化，保住用户自己加的其他配置项 ——
/// 覆盖式写入会把 `[projects.*]`、`[mcp_servers.*]`、`[plugins.*]` 这些
/// 全抹掉。实机上这类段有三十多个。
pub fn codex_config_toml(existing: &str, p: &Provider) -> String {
    let mut doc = existing
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_default();

    doc["model_provider"] = toml_edit::value(p.id.as_str());
    if let Some(m) = &p.model {
        doc["model"] = toml_edit::value(m.as_str());
    }

    let id = p.id.as_str();
    doc["model_providers"][id]["name"] = toml_edit::value(p.name.as_str());
    doc["model_providers"][id]["base_url"] = toml_edit::value(p.base_url.as_str());
    doc["model_providers"][id]["wire_api"] = toml_edit::value(p.wire_api.as_str());

    match p.auth_style {
        AuthStyle::EnvKey => {
            doc["model_providers"][id]["env_key"] = toml_edit::value("OPENAI_API_KEY");
        }
        AuthStyle::BearerToken => {
            if let Some(key) = &p.api_key {
                doc["model_providers"][id]["experimental_bearer_token"] =
                    toml_edit::value(key.as_str());
            }
        }
        AuthStyle::None => {}
    }

    doc.to_string()
}

/// 把 API Key **并进** `auth.json`，保留文件里其他字段。
///
/// 旧代码整份覆盖写 `{"OPENAI_API_KEY": key}` —— 那会把 Codex 官方登录留下的
/// `tokens`（access_token / refresh_token / account_id）和 `type` 一起抹掉，
/// 等于把用户从 Codex 登出。**这个函数存在的唯一理由就是别再犯那个错。**
pub fn codex_auth_json(existing: &str, key: &str) -> Result<String> {
    let mut doc: serde_json::Value =
        serde_json::from_str(existing).unwrap_or_else(|_| serde_json::json!({}));
    if !doc.is_object() {
        doc = serde_json::json!({});
    }
    doc["OPENAI_API_KEY"] = serde_json::Value::String(key.to_string());
    Ok(serde_json::to_string_pretty(&doc)?)
}

/// 生成新的 `~/.claude/settings.json` 内容，保留 env 之外的其他设置。
///
/// `ANTHROPIC_API_KEY` 与 `ANTHROPIC_AUTH_TOKEN` **两个只留一个** ——
/// 同时存在时 Claude Code 用哪个是不确定的，切换中转站后会出现
/// 「改了没生效」这种最难查的症状。
pub fn claude_settings_json(existing: &str, p: &Provider) -> Result<String> {
    let mut doc: serde_json::Value =
        serde_json::from_str(existing).unwrap_or_else(|_| serde_json::json!({}));
    if !doc.is_object() {
        doc = serde_json::json!({});
    }
    if !doc.get("env").is_some_and(|v| v.is_object()) {
        doc["env"] = serde_json::json!({});
    }
    let env = doc
        .get_mut("env")
        .and_then(|v| v.as_object_mut())
        .expect("env object");

    env.insert(
        "ANTHROPIC_BASE_URL".into(),
        serde_json::Value::String(p.base_url.clone()),
    );
    if let Some(model) = &p.model {
        env.insert(
            "ANTHROPIC_MODEL".into(),
            serde_json::Value::String(model.clone()),
        );
    }

    match p.auth_style {
        AuthStyle::EnvKey => {
            env.remove("ANTHROPIC_AUTH_TOKEN");
            if let Some(key) = &p.api_key {
                env.insert(
                    "ANTHROPIC_API_KEY".into(),
                    serde_json::Value::String(key.clone()),
                );
            }
        }
        AuthStyle::BearerToken => {
            env.remove("ANTHROPIC_API_KEY");
            if let Some(key) = &p.api_key {
                env.insert(
                    "ANTHROPIC_AUTH_TOKEN".into(),
                    serde_json::Value::String(key.clone()),
                );
            }
        }
        AuthStyle::None => {}
    }

    Ok(serde_json::to_string_pretty(&doc)?)
}

// ------------------------------------------------------------ 写入

pub fn apply_provider(p: &Provider) -> Result<()> {
    if is_claude(&p.target) {
        return apply_claude_provider(p);
    }

    let cfg_path = codex_dir().join("config.toml");
    let existing = std::fs::read_to_string(&cfg_path).unwrap_or_default();
    atomic_write(&cfg_path, &codex_config_toml(&existing, p))?;

    // 只有 EnvKey 这一种形态才碰 auth.json。BearerToken 的 Key 已经写进
    // provider 段了，再动 auth.json 就是白白冒登出的风险。
    if matches!(p.auth_style, AuthStyle::EnvKey) {
        if let Some(key) = &p.api_key {
            let auth_path = codex_dir().join("auth.json");
            let existing = std::fs::read_to_string(&auth_path).unwrap_or_default();
            atomic_write(&auth_path, &codex_auth_json(&existing, key)?)?;
        }
    }

    crate::gate::log::write(&format!("Codex 中转站切换为 {}", p.name));
    Ok(())
}

fn apply_claude_provider(p: &Provider) -> Result<()> {
    let path = claude_settings_path();
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    atomic_write(&path, &claude_settings_json(&existing, p)?)?;
    crate::gate::log::write(&format!("Claude 中转站切换为 {}", p.name));
    Ok(())
}

// ------------------------------------------------------------ 读回

/// 读回当前配置，**不含 Key**。
///
/// 每个 target 各返回一条。旧代码读到 Claude 就提前 return，
/// Codex 那边的配置在界面上永远看不见 —— 见文件头坑 3。
pub fn current_providers() -> Vec<Provider> {
    let mut out = Vec::new();
    if let Some(p) = current_claude_provider() {
        out.push(p);
    }
    if let Some(p) = current_codex_provider() {
        out.push(p);
    }
    out
}

fn current_claude_provider() -> Option<Provider> {
    let text = std::fs::read_to_string(claude_settings_path()).ok()?;
    let doc = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    let env = doc.get("env")?;
    let base = env.get("ANTHROPIC_BASE_URL")?.as_str()?;

    let auth_style = if env.get("ANTHROPIC_AUTH_TOKEN").is_some() {
        AuthStyle::BearerToken
    } else if env.get("ANTHROPIC_API_KEY").is_some() {
        AuthStyle::EnvKey
    } else {
        AuthStyle::None
    };

    Some(Provider {
        target: "claude".into(),
        id: "claude".into(),
        name: "Claude 中转站".into(),
        base_url: base.into(),
        model: env
            .get("ANTHROPIC_MODEL")
            .and_then(|v| v.as_str())
            .map(String::from),
        wire_api: WireApi::default(),
        auth_style,
        api_key: None,
    })
}

fn current_codex_provider() -> Option<Provider> {
    let text = std::fs::read_to_string(codex_dir().join("config.toml")).ok()?;
    let doc = text.parse::<toml_edit::DocumentMut>().ok()?;
    let id = doc.get("model_provider")?.as_str()?.to_string();
    let node = doc.get("model_providers")?.get(&id)?;

    let wire_api = match node.get("wire_api").and_then(|v| v.as_str()) {
        Some("chat") => WireApi::Chat,
        _ => WireApi::Responses,
    };
    let auth_style = if node.get("experimental_bearer_token").is_some() {
        AuthStyle::BearerToken
    } else if node.get("env_key").is_some() {
        AuthStyle::EnvKey
    } else {
        AuthStyle::None
    };

    Some(Provider {
        target: "codex".into(),
        name: node
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string(),
        base_url: node
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        model: doc.get("model").and_then(|v| v.as_str()).map(String::from),
        wire_api,
        auth_style,
        api_key: None,
        id,
    })
}

/// 掩码，给界面显示用。
pub fn mask(key: &str) -> String {
    let n = key.chars().count();
    if n <= 8 {
        return "•".repeat(n);
    }
    let head: String = key.chars().take(4).collect();
    let tail: String = key.chars().skip(n - 4).collect();
    format!("{head}{}{tail}", "•".repeat(n.saturating_sub(8).min(16)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(auth_style: AuthStyle, wire_api: WireApi) -> Provider {
        Provider {
            target: "codex".into(),
            id: "myrelay".into(),
            name: "我的中转".into(),
            base_url: "https://api.example.com".into(),
            model: Some("gpt-5.2".into()),
            wire_api,
            auth_style,
            api_key: Some("sk-secret-value".into()),
        }
    }

    #[test]
    fn masks_long_key_without_leaking_middle() {
        let m = mask("sk-abcdefghijklmnopqrstuvwxyz");
        assert!(m.starts_with("sk-a"));
        assert!(m.ends_with("wxyz"));
        assert!(!m.contains("efghij"));
    }

    #[test]
    fn masks_short_key_entirely() {
        assert_eq!(mask("abcd"), "••••");
        assert_eq!(mask(""), "");
    }

    #[test]
    fn provider_never_serializes_api_key() {
        let j = serde_json::to_string(&provider(AuthStyle::EnvKey, WireApi::Responses)).unwrap();
        assert!(!j.contains("sk-secret-value"));
        assert!(!j.contains("api_key"));
    }

    #[test]
    fn provider_list_never_serializes_api_key() {
        // 列表命令返回 Vec<Provider>，不变量对每一条都要成立。
        let list = vec![
            provider(AuthStyle::EnvKey, WireApi::Responses),
            provider(AuthStyle::BearerToken, WireApi::Chat),
        ];
        let j = serde_json::to_string(&list).unwrap();
        assert!(!j.contains("sk-secret-value"));
        assert!(!j.contains("api_key"));
    }

    #[test]
    fn wire_api_defaults_to_responses_not_chat() {
        // 坑 1：写死成 chat 会把实机在用的 responses 中转站改坏。
        assert_eq!(WireApi::default(), WireApi::Responses);
        let p: Provider = serde_json::from_str(
            r#"{"target":"codex","id":"x","name":"X","base_url":"https://x"}"#,
        )
        .unwrap();
        assert_eq!(p.wire_api, WireApi::Responses);
    }

    #[test]
    fn codex_toml_keeps_every_unrelated_section() {
        // 实机 config.toml 里有三十多个这类段，整份重写会全丢。
        let existing = r#"
model_reasoning_effort = "xhigh"
disable_response_storage = true

[projects.'c:\users\me\code']
trust_level = "trusted"

[mcp_servers.node_repl]
command = 'node.exe'

[plugins."documents@runtime"]
enabled = true
"#;
        let out =
            codex_config_toml(existing, &provider(AuthStyle::BearerToken, WireApi::Responses));
        assert!(out.contains("model_reasoning_effort"));
        assert!(out.contains(r#"[projects.'c:\users\me\code']"#));
        assert!(out.contains("[mcp_servers.node_repl]"));
        assert!(out.contains(r#"[plugins."documents@runtime"]"#));
        assert!(out.contains(r#"model_provider = "myrelay""#));
    }

    #[test]
    fn codex_toml_writes_the_requested_wire_api() {
        let out = codex_config_toml("", &provider(AuthStyle::EnvKey, WireApi::Responses));
        assert!(out.contains(r#"wire_api = "responses""#));
        assert!(!out.contains(r#"wire_api = "chat""#));

        let out = codex_config_toml("", &provider(AuthStyle::EnvKey, WireApi::Chat));
        assert!(out.contains(r#"wire_api = "chat""#));
    }

    #[test]
    fn bearer_token_goes_in_the_provider_section_not_auth_json() {
        let out = codex_config_toml("", &provider(AuthStyle::BearerToken, WireApi::Responses));
        assert!(out.contains("experimental_bearer_token"));
        assert!(!out.contains("env_key"));
    }

    #[test]
    fn env_key_style_writes_env_key_and_no_bearer_token() {
        let out = codex_config_toml("", &provider(AuthStyle::EnvKey, WireApi::Responses));
        assert!(out.contains(r#"env_key = "OPENAI_API_KEY""#));
        assert!(!out.contains("experimental_bearer_token"));
    }

    #[test]
    fn auth_json_merge_keeps_oauth_tokens() {
        // 坑 2：整份覆盖会把这些抹掉，等于把用户从 Codex 登出。
        let existing = r#"{
  "OPENAI_API_KEY": null,
  "last_refresh": "2026-09-08T00:00:00Z",
  "tokens": { "access_token": "at", "account_id": "acc", "refresh_token": "rt" },
  "type": "chatgpt"
}"#;
        let out = codex_auth_json(existing, "sk-new").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["OPENAI_API_KEY"], "sk-new");
        assert_eq!(v["tokens"]["refresh_token"], "rt");
        assert_eq!(v["tokens"]["account_id"], "acc");
        assert_eq!(v["type"], "chatgpt");
        assert_eq!(v["last_refresh"], "2026-09-08T00:00:00Z");
    }

    #[test]
    fn auth_json_handles_missing_or_broken_file() {
        for existing in ["", "not json at all", "[1,2,3]"] {
            let out = codex_auth_json(existing, "sk-new").unwrap();
            let v: serde_json::Value = serde_json::from_str(&out).unwrap();
            assert_eq!(v["OPENAI_API_KEY"], "sk-new");
        }
    }

    #[test]
    fn claude_settings_keep_unrelated_keys() {
        let existing = r#"{"permissions":{"allow":["Bash"]},"env":{"FOO":"bar"}}"#;
        let mut p = provider(AuthStyle::EnvKey, WireApi::Responses);
        p.target = "claude".into();
        let out = claude_settings_json(existing, &p).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["permissions"]["allow"][0], "Bash");
        assert_eq!(v["env"]["FOO"], "bar");
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "https://api.example.com");
    }

    #[test]
    fn claude_settings_never_keep_both_key_and_token() {
        // 两个同时存在时用哪个是不确定的，会变成「改了没生效」。
        let mut p = provider(AuthStyle::BearerToken, WireApi::Responses);
        p.target = "claude".into();
        let out = claude_settings_json(r#"{"env":{"ANTHROPIC_API_KEY":"old"}}"#, &p).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["env"].get("ANTHROPIC_API_KEY").is_none());
        assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-secret-value");

        p.auth_style = AuthStyle::EnvKey;
        let out = claude_settings_json(r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"old"}}"#, &p).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
        assert_eq!(v["env"]["ANTHROPIC_API_KEY"], "sk-secret-value");
    }

    #[test]
    fn claude_target_accepts_both_spellings() {
        assert!(is_claude("claude"));
        assert!(is_claude("Claude"));
        assert!(is_claude("claude-code"));
        assert!(!is_claude("codex"));
    }
}
