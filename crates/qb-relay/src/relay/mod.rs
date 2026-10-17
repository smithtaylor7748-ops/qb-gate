//! 中转：把一条「地址 + 协议 + 凭证形态」写进各个工具自己的配置文件。
//!
//!   `config.toml`    Codex 的模型供应商与 base_url
//!   `settings.json`  Claude Code 的 env 段
//!
//! 调用方是 `qb_app::workspace::configuration_files` —— 写的是每个中转环境自己的隔离目录，
//! 不是 `~\.codex` / `~\.claude`。形态参考 cc-switch（MIT，https://github.com/farion1231/cc-switch），
//! 实现是自己写的，见 `ATTRIBUTION.md`。
//!
//! # 原来这里还有什么（2026-09-27 删了）
//!
//! 数据库之前那一代的中转站目录：`relay.json` 的读写与增删改查（`store`）、
//! 「从当前配置导入」（`current_providers` / `import_live`）、拉模型列表与测延迟（`probe`）、
//! 按响应头判断「真实后端」的分类器（`backend`，只有已经删掉的草稿诊断在用）。
//! 迁移之后这些一个调用者都没有了。`relay.json` 的老数据只在 `repository::migrate()`
//! 里按 v1 的形状读一次，那边有自己的结构体，不依赖这里。
//!
//! # 两条踩过的坑（2026-09-09 实机核对后修）
//!
//! 1. **`wire_api` 不能写死成 `chat`。** 实机在用的中转站是 `responses`，
//!    写死会把可用配置改坏。现在由 [`WireApi`] 决定，默认 `responses`。
//! 2. **两种凭证不能同时留着。** `env_key` 与 `experimental_bearer_token`、
//!    `ANTHROPIC_API_KEY` 与 `ANTHROPIC_AUTH_TOKEN` 同时在的时候客户端用哪个说不准，
//!    症状是「改了没生效」。
//!
//! 两条都有单测钉着，见文件末尾。

pub mod presets;

use crate::error::{GateError, Result};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 中转服务的哪个工具。预设按它分（`presets::for_target`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum RelayTarget {
    ClaudeDesktop,
    ClaudeCode,
    Codex,
}

impl RelayTarget {
    pub const ALL: [RelayTarget; 3] = [
        RelayTarget::ClaudeDesktop,
        RelayTarget::ClaudeCode,
        RelayTarget::Codex,
    ];

    /// 与 serde 输出一致。**两边必须对得上** —— 有单测钉着。
    pub fn as_str(self) -> &'static str {
        match self {
            RelayTarget::ClaudeDesktop => "claude-desktop",
            RelayTarget::ClaudeCode => "claude-code",
            RelayTarget::Codex => "codex",
        }
    }
}

/// 上游协议。
///
/// `responses` 是 Codex 原生协议，绝大多数中转站用它；`chat` 是 OpenAI
/// Chat Completions。**默认必须是 `responses`** —— 写死成 `chat` 正是坑 1。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[ts(export)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AuthStyle {
    /// Codex：provider 段写 `env_key`，Key 由启动时的环境变量（或环境目录里的 `auth.json`）给。
    /// Claude：写 `ANTHROPIC_API_KEY`。
    #[default]
    EnvKey,
    /// Codex：provider 段直接写 `experimental_bearer_token`。
    /// Claude：写 `ANTHROPIC_AUTH_TOKEN`。
    BearerToken,
    /// 不写任何凭证 —— 官方 OAuth 登录，或者 Key 由用户自己管。
    None,
}

/// 写进配置要用到的那几项。
///
/// 原来它是 `relay.json` 目录里一条供应商的公开信息，还带着 id、所属工具、备注、网址、
/// 图标、排序、创建时间 —— 那些只给目录与界面用，目录删了之后一个都没人读。
#[derive(Debug, Clone)]
pub struct ProviderMeta {
    /// 写进 `config.toml` 的 `model_providers.<slug>`。Codex 用，Claude 侧忽略。
    pub slug: String,
    pub name: String,
    pub base_url: String,
    pub model: Option<String>,
    /// 小模型 / 快模型那一档（落到 `ANTHROPIC_SMALL_FAST_MODEL`）。
    ///
    /// Claude Code 会拿它跑后台的轻活（补全会话标题之类）。**很多中转站的
    /// 小模型跟主模型不是同一个名字**，只配主模型的话那些后台请求会直接报错，
    /// 而报错信息跟你正在做的事完全无关，极难联想。
    ///
    /// `None` = 不设这个变量，交给 Claude Code 用它自己的默认值。
    /// 形态参考 z-switch（MIT）的多档模型配置。
    pub small_fast_model: Option<String>,
    pub wire_api: WireApi,
    pub auth_style: AuthStyle,
}

// ------------------------------------------------------- 纯函数（可单测）

/// 生成新的 `config.toml` 内容。
///
/// 用 toml_edit 增量改而不是整个重新序列化，保住用户自己加的其他配置项 ——
/// 覆盖式写入会把 `[projects.*]`、`[mcp_servers.*]`、`[plugins.*]` 这些
/// 全抹掉。实机上这类段有三十多个。
pub fn codex_config_toml(existing: &str, m: &ProviderMeta, key: Option<&str>) -> String {
    let mut doc = existing
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_default();

    let slug = m.slug.as_str();
    doc["model_provider"] = toml_edit::value(slug);
    if let Some(model) = &m.model {
        doc["model"] = toml_edit::value(model.as_str());
    } else {
        doc.remove("model");
    }

    ensure_provider_table(&mut doc, slug);

    let Some(t) = doc["model_providers"][slug].as_table_mut() else {
        return doc.to_string();
    };

    t["name"] = toml_edit::value(m.name.as_str());
    t["base_url"] = toml_edit::value(m.base_url.as_str());
    t["wire_api"] = toml_edit::value(m.wire_api.as_str());

    // 换凭证形态时**必须把另一种清掉**。两种同时留在 provider 段里，
    // Codex 用哪个说不准，症状是「改了没生效」。
    match m.auth_style {
        AuthStyle::EnvKey => {
            t["env_key"] = toml_edit::value("OPENAI_API_KEY");
            t.remove("experimental_bearer_token");
        }
        AuthStyle::BearerToken => {
            if let Some(k) = key {
                t["experimental_bearer_token"] = toml_edit::value(k);
            }
            t.remove("env_key");
        }
        AuthStyle::None => {
            t.remove("env_key");
            t.remove("experimental_bearer_token");
        }
    }

    doc.to_string()
}

/// 保证 `model_providers.<slug>` 是**标准表**（`[model_providers.x]`），
/// 不是内联表（`model_providers = { x = {...} }`）。
///
/// 这不是排版洁癖。`doc["a"]["b"]["c"] = value(..)` 这种链式索引在一份**空**
/// 文档上建出来的是内联表，而 `Item::as_table_mut()` 对内联表返回 `None` ——
/// 于是「切换凭证形态时清掉另一种」在新建的配置上会**静默失效**，
/// 两种凭证一起留在文件里。用户已有的配置里是标准表，所以这个 bug 只在
/// 全新配置上出现，最难发现的那一类。
fn ensure_provider_table(doc: &mut toml_edit::DocumentMut, slug: &str) {
    use toml_edit::{Item, Table};

    if !doc.as_table().contains_key("model_providers") {
        let mut t = Table::new();
        // implicit = 不为父表单独打一行 `[model_providers]`。
        t.set_implicit(true);
        doc.insert("model_providers", Item::Table(t));
    } else if let Some(inline) = doc["model_providers"].as_inline_table().cloned() {
        // 用户手写成内联表的，转成标准表再改。
        doc["model_providers"] = Item::Table(inline.into_table());
    }

    let Some(parent) = doc["model_providers"].as_table_mut() else {
        return;
    };
    parent.set_implicit(true);

    match parent.get(slug) {
        None => {
            parent.insert(slug, Item::Table(Table::new()));
        }
        Some(item) => {
            if let Some(inline) = item.as_inline_table().cloned() {
                parent.insert(slug, Item::Table(inline.into_table()));
            }
        }
    }
}

/// 生成新的 `~/.claude/settings.json` 内容，保留 env 之外的其他设置。
///
/// `ANTHROPIC_API_KEY` 与 `ANTHROPIC_AUTH_TOKEN` **两个只留一个** ——
/// 同时存在时 Claude Code 用哪个是不确定的，切换中转站后会出现
/// 「改了没生效」这种最难查的症状。
pub fn claude_settings_json(existing: &str, m: &ProviderMeta, key: Option<&str>) -> Result<String> {
    let mut doc = crate::config_io::object(existing)?;
    if doc.get("env").is_some_and(|v| !v.is_object()) {
        return Err(GateError::Other("env 必须是对象，原配置未改动".into()));
    }
    if doc.get("env").is_none() {
        doc["env"] = serde_json::json!({});
    }
    let env = doc
        .get_mut("env")
        .and_then(|v| v.as_object_mut())
        .expect("env object");

    env.insert(
        "ANTHROPIC_BASE_URL".into(),
        serde_json::Value::String(m.base_url.clone()),
    );
    if let Some(model) = &m.model {
        env.insert(
            "ANTHROPIC_MODEL".into(),
            serde_json::Value::String(model.clone()),
        );
    } else {
        env.remove("ANTHROPIC_MODEL");
    }
    // 留空 = 把这个变量**删掉**，而不是留着上一家的值。
    // 不删的话切换中转站之后，小模型还指着上一家的名字，后台请求会报一个
    // 跟你正在做的事毫无关系的错。
    match &m.small_fast_model {
        Some(sm) if !sm.trim().is_empty() => {
            env.insert(
                "ANTHROPIC_SMALL_FAST_MODEL".into(),
                serde_json::Value::String(sm.trim().to_string()),
            );
        }
        _ => {
            env.remove("ANTHROPIC_SMALL_FAST_MODEL");
        }
    }

    env.remove("ANTHROPIC_API_KEY");
    env.remove("ANTHROPIC_AUTH_TOKEN");
    match m.auth_style {
        AuthStyle::EnvKey => {
            env.remove("ANTHROPIC_AUTH_TOKEN");
            if let Some(k) = key {
                env.insert(
                    "ANTHROPIC_API_KEY".into(),
                    serde_json::Value::String(k.to_string()),
                );
            }
        }
        AuthStyle::BearerToken => {
            env.remove("ANTHROPIC_API_KEY");
            if let Some(k) = key {
                env.insert(
                    "ANTHROPIC_AUTH_TOKEN".into(),
                    serde_json::Value::String(k.to_string()),
                );
            }
        }
        AuthStyle::None => {}
    }

    Ok(serde_json::to_string_pretty(&doc)?)
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

    fn meta(auth_style: AuthStyle, wire_api: WireApi) -> ProviderMeta {
        ProviderMeta {
            slug: "myrelay".into(),
            name: "我的中转".into(),
            base_url: "https://api.example.com".into(),
            model: Some("gpt-5.2".into()),
            small_fast_model: None,
            wire_api,
            auth_style,
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
    fn target_string_matches_its_serde_name() {
        // `active` 那张表按 as_str() 做键，serde 那边按 rename_all 输出。
        // 两边对不上就会出现「切了但界面上没变」。
        for t in RelayTarget::ALL {
            let j = serde_json::to_string(&t).unwrap();
            assert_eq!(j, format!("\"{}\"", t.as_str()));
        }
    }

    #[test]
    fn wire_api_defaults_to_responses_not_chat() {
        // 坑 1：写死成 chat 会把实机在用的 responses 中转站改坏。
        assert_eq!(WireApi::default(), WireApi::Responses);
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
        let out = codex_config_toml(
            existing,
            &meta(AuthStyle::BearerToken, WireApi::Responses),
            Some("sk-x"),
        );
        assert!(out.contains("model_reasoning_effort"));
        assert!(out.contains(r#"[projects.'c:\users\me\code']"#));
        assert!(out.contains("[mcp_servers.node_repl]"));
        assert!(out.contains(r#"[plugins."documents@runtime"]"#));
        assert!(out.contains(r#"model_provider = "myrelay""#));
    }

    #[test]
    fn codex_toml_writes_the_requested_wire_api() {
        let out = codex_config_toml("", &meta(AuthStyle::EnvKey, WireApi::Responses), None);
        assert!(out.contains(r#"wire_api = "responses""#));
        assert!(!out.contains(r#"wire_api = "chat""#));

        let out = codex_config_toml("", &meta(AuthStyle::EnvKey, WireApi::Chat), None);
        assert!(out.contains(r#"wire_api = "chat""#));
    }

    #[test]
    fn switching_auth_style_clears_the_other_credential() {
        // 两种凭证同时留在 provider 段里，Codex 用哪个说不准 ——
        // 又是一个「改了没生效」型的坑。
        let with_bearer = codex_config_toml(
            "",
            &meta(AuthStyle::BearerToken, WireApi::Responses),
            Some("sk-x"),
        );
        assert!(with_bearer.contains("experimental_bearer_token"));

        let switched = codex_config_toml(
            &with_bearer,
            &meta(AuthStyle::EnvKey, WireApi::Responses),
            Some("sk-x"),
        );
        assert!(switched.contains("env_key"));
        assert!(
            !switched.contains("experimental_bearer_token"),
            "切到 env_key 之后 bearer token 还留着：{switched}"
        );
    }

    #[test]
    fn claude_settings_keep_unrelated_keys() {
        let existing = r#"{"permissions":{"allow":["Bash"]},"env":{"FOO":"bar"}}"#;
        let m = meta(AuthStyle::EnvKey, WireApi::Responses);
        let out = claude_settings_json(existing, &m, Some("sk-x")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["permissions"]["allow"][0], "Bash");
        assert_eq!(v["env"]["FOO"], "bar");
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "https://api.example.com");
    }

    #[test]
    fn claude_settings_never_keep_both_key_and_token() {
        // 两个同时存在时用哪个是不确定的，会变成「改了没生效」。
        let mut m = meta(AuthStyle::BearerToken, WireApi::Responses);
        let out = claude_settings_json(r#"{"env":{"ANTHROPIC_API_KEY":"old"}}"#, &m, Some("sk-x"))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["env"].get("ANTHROPIC_API_KEY").is_none());
        assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-x");

        m.auth_style = AuthStyle::EnvKey;
        let out = claude_settings_json(
            r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"old"}}"#,
            &m,
            Some("sk-x"),
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
        assert_eq!(v["env"]["ANTHROPIC_API_KEY"], "sk-x");
    }

    #[test]
    fn fresh_config_uses_a_standard_table_not_an_inline_one() {
        // 链式索引在空文档上建出来的是内联表，而 as_table_mut() 对内联表
        // 返回 None —— 清凭证那步会静默失效。这个 bug 只在全新配置上出现。
        let out = codex_config_toml("", &meta(AuthStyle::EnvKey, WireApi::Responses), None);
        assert!(
            out.contains("[model_providers.myrelay]"),
            "写成了内联表：{out}"
        );
        assert!(!out.contains("model_providers = {"), "写成了内联表：{out}");
        // 建出来的东西自己得能再解析回去。
        assert!(out.parse::<toml_edit::DocumentMut>().is_ok());
    }

    #[test]
    fn hand_written_inline_table_is_converted_and_still_editable() {
        // 用户手写成内联表的配置也得改得动。
        let existing = r#"model_providers = { myrelay = { name = "旧", base_url = "https://old", experimental_bearer_token = "sk-old" } }"#;
        let out = codex_config_toml(existing, &meta(AuthStyle::EnvKey, WireApi::Responses), None);
        assert!(!out.contains("experimental_bearer_token"), "{out}");
        assert!(out.contains("env_key"));
        assert!(out.contains("https://api.example.com"));
    }

    #[test]
    fn auth_style_none_writes_no_credential_at_all() {
        // 官方 OAuth 登录的场景：只改端点，一个凭证都不许写进去。
        let m = meta(AuthStyle::None, WireApi::Responses);
        let out = claude_settings_json("{}", &m, Some("sk-should-not-appear")).unwrap();
        assert!(!out.contains("sk-should-not-appear"));

        let out = codex_config_toml("", &meta(AuthStyle::None, WireApi::Responses), Some("sk-x"));
        assert!(!out.contains("sk-x"));
        assert!(!out.contains("env_key"));
    }
}
