//! 中转站：多供应商目录 + 写进各个工具自己的配置文件。
//!
//! 形态对标 cc-switch（MIT，https://github.com/farion1231/cc-switch）：
//! 三个应用各一份列表、一键切换、写进目标工具自己的配置：
//!   `~/.codex/config.toml`    模型供应商与 base_url
//!   `~/.codex/auth.json`      API Key（**只并入，不整份覆盖**）
//!   `~/.claude/settings.json` Claude Code / 桌面端的 env 段
//!
//! 界面布局参考了 Cockpit Tools 的排法，但**一行代码都没有复制** ——
//! 那个项目是 CC BY-NC-SA 4.0，SA 条款会把本项目从 MIT 拖成同一个协议，
//! NC 条款还会禁止商业使用。详见 `ATTRIBUTION.md`。
//!
//! # API Key 去哪了
//!
//! 目录里的 Key 用 DPAPI 加密后存在 `relay.json`（见 [`secret`]），
//! 回给前端的 [`ProviderView`] **结构上装不下 Key**（见 [`store`]）。
//! 写进目标工具的配置是那些工具本身的要求，不在本项目的管辖内。
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

pub mod presets;
pub mod probe;
pub mod secret;
pub mod store;

use crate::error::{GateError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use store::{ProviderInput, ProviderMeta, ProviderView, RelayStore, StoredProvider};

/// 中转站服务的哪个工具。
///
/// 三个是各自独立的列表 —— 切 Codex 的供应商不该动到 Claude 那边。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

    /// 与 serde 输出一致。**两边必须对得上** —— `active` 那张表是按这个
    /// 字符串做键的，对不上就会出现「切了但界面上没变」。有单测钉着。
    pub fn as_str(self) -> &'static str {
        match self {
            RelayTarget::ClaudeDesktop => "claude-desktop",
            RelayTarget::ClaudeCode => "claude-code",
            RelayTarget::Codex => "codex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RelayTarget::ClaudeDesktop => "Claude 桌面端",
            RelayTarget::ClaudeCode => "Claude Code",
            RelayTarget::Codex => "Codex",
        }
    }

    /// 写的是哪个配置文件。
    pub fn config_path(self) -> PathBuf {
        match self {
            RelayTarget::Codex => codex_dir().join("config.toml"),
            _ => claude_settings_path(),
        }
    }
}

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

pub fn codex_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".codex")
}

pub fn claude_settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("settings.json")
}

/// 备份目录。**按时间戳分文件夹，不是单槽 `.bak`。**
///
/// 旧版每个文件只留一个 `.bak`，写第二次就把第一次的备份盖掉了 ——
/// 等于「改坏了能退回去」只在改坏的下一秒成立。
pub fn backup_dir() -> PathBuf {
    crate::gate::state_dir().join("relay-backups")
}

/// 保留多少份历史备份。超出的从最旧的开始删。
const KEEP_BACKUPS: usize = 20;

fn backup_before_write(p: &std::path::Path) {
    if !p.exists() {
        return;
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let dir = backup_dir().join(stamp);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Some(name) = p.file_name() {
        let _ = std::fs::copy(p, dir.join(name));
    }
    rotate_backups();
}

/// 只留最近 [`KEEP_BACKUPS`] 份。目录名是时间戳，按名字排序就是按时间排序。
fn rotate_backups() {
    let Ok(rd) = std::fs::read_dir(backup_dir()) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    if dirs.len() <= KEEP_BACKUPS {
        return;
    }
    dirs.sort();
    for old in dirs.iter().take(dirs.len() - KEEP_BACKUPS) {
        let _ = std::fs::remove_dir_all(old);
    }
}

pub(crate) fn atomic_write(p: &std::path::Path, body: &str) -> Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    backup_before_write(p);
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
pub fn codex_config_toml(existing: &str, m: &ProviderMeta, key: Option<&str>) -> String {
    let mut doc = existing
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_default();

    let slug = m.slug.as_str();
    doc["model_provider"] = toml_edit::value(slug);
    if let Some(model) = &m.model {
        doc["model"] = toml_edit::value(model.as_str());
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
pub fn claude_settings_json(existing: &str, m: &ProviderMeta, key: Option<&str>) -> Result<String> {
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
        serde_json::Value::String(m.base_url.clone()),
    );
    if let Some(model) = &m.model {
        env.insert(
            "ANTHROPIC_MODEL".into(),
            serde_json::Value::String(model.clone()),
        );
    }

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

// ------------------------------------------------------------ 写入

/// 把一条供应商写进它对应的工具配置。
pub fn apply_stored(p: &StoredProvider) -> Result<()> {
    let key = p.plain_key();
    let m = &p.meta;

    match m.target {
        RelayTarget::Codex => {
            let cfg = m.target.config_path();
            let existing = std::fs::read_to_string(&cfg).unwrap_or_default();
            atomic_write(&cfg, &codex_config_toml(&existing, m, key.as_deref()))?;

            // 只有 EnvKey 这一种形态才碰 auth.json。BearerToken 的 Key 已经
            // 写进 provider 段了，再动 auth.json 就是白白冒登出的风险。
            if matches!(m.auth_style, AuthStyle::EnvKey) {
                if let Some(k) = &key {
                    let auth = codex_dir().join("auth.json");
                    let existing = std::fs::read_to_string(&auth).unwrap_or_default();
                    atomic_write(&auth, &codex_auth_json(&existing, k)?)?;
                }
            }
        }
        _ => {
            let path = m.target.config_path();
            let existing = std::fs::read_to_string(&path).unwrap_or_default();
            atomic_write(&path, &claude_settings_json(&existing, m, key.as_deref())?)?;
        }
    }

    crate::gate::log::write(&format!("{} 中转站切换为 {}", m.target.label(), m.name));
    Ok(())
}

/// 启用某个 target 下的某一条，并记住它。
pub fn activate(target: RelayTarget, id: &str) -> Result<()> {
    let mut s = store::load();
    let p = s
        .get(id)
        .ok_or_else(|| GateError::Other(format!("找不到中转站记录 {id}")))?
        .clone();
    if p.meta.target != target {
        return Err(GateError::Other(format!(
            "记录 {} 属于 {}，不能在 {} 下启用",
            p.meta.name,
            p.meta.target.label(),
            target.label()
        )));
    }
    apply_stored(&p)?;
    s.active.insert(target.as_str().into(), id.to_string());
    store::save(&s)
}

// ------------------------------------------------------------ 读回

/// 从目标工具的 live 配置里读回当前值，**不含 Key**。
///
/// 每个 target 各返回一条。旧代码读到 Claude 就提前 return，
/// Codex 那边的配置在界面上永远看不见 —— 见文件头坑 3。
pub fn current_providers() -> Vec<ProviderMeta> {
    let mut out = Vec::new();
    if let Some(p) = current_claude_provider() {
        out.push(p);
    }
    if let Some(p) = current_codex_provider() {
        out.push(p);
    }
    out
}

/// 读某一个 target 的 live 配置，用于「从当前配置导入」。
pub fn import_live(target: RelayTarget) -> Option<ProviderMeta> {
    match target {
        RelayTarget::Codex => current_codex_provider(),
        _ => current_claude_provider().map(|mut m| {
            m.target = target;
            m
        }),
    }
}

fn blank_meta(target: RelayTarget) -> ProviderMeta {
    ProviderMeta {
        id: String::new(),
        target,
        slug: String::new(),
        name: String::new(),
        base_url: String::new(),
        model: None,
        wire_api: WireApi::default(),
        auth_style: AuthStyle::default(),
        note: None,
        website: None,
        icon: None,
        sort: 0,
        created_at: String::new(),
    }
}

fn current_claude_provider() -> Option<ProviderMeta> {
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

    Some(ProviderMeta {
        slug: "claude".into(),
        name: "当前 Claude 配置".into(),
        base_url: base.into(),
        model: env
            .get("ANTHROPIC_MODEL")
            .and_then(|v| v.as_str())
            .map(String::from),
        auth_style,
        ..blank_meta(RelayTarget::ClaudeCode)
    })
}

fn current_codex_provider() -> Option<ProviderMeta> {
    let text = std::fs::read_to_string(codex_dir().join("config.toml")).ok()?;
    let doc = text.parse::<toml_edit::DocumentMut>().ok()?;
    let slug = doc.get("model_provider")?.as_str()?.to_string();
    let node = doc.get("model_providers")?.get(&slug)?;

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

    Some(ProviderMeta {
        name: node
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&slug)
            .to_string(),
        base_url: node
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        model: doc.get("model").and_then(|v| v.as_str()).map(String::from),
        wire_api,
        auth_style,
        slug,
        ..blank_meta(RelayTarget::Codex)
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

    fn meta(auth_style: AuthStyle, wire_api: WireApi) -> ProviderMeta {
        ProviderMeta {
            slug: "myrelay".into(),
            name: "我的中转".into(),
            base_url: "https://api.example.com".into(),
            model: Some("gpt-5.2".into()),
            wire_api,
            auth_style,
            ..blank_meta(RelayTarget::Codex)
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
        let mut m = meta(AuthStyle::EnvKey, WireApi::Responses);
        m.target = RelayTarget::ClaudeCode;
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
        m.target = RelayTarget::ClaudeCode;
        let out =
            claude_settings_json(r#"{"env":{"ANTHROPIC_API_KEY":"old"}}"#, &m, Some("sk-x")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["env"].get("ANTHROPIC_API_KEY").is_none());
        assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-x");

        m.auth_style = AuthStyle::EnvKey;
        let out = claude_settings_json(r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"old"}}"#, &m, Some("sk-x"))
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
        let out = codex_config_toml(
            existing,
            &meta(AuthStyle::EnvKey, WireApi::Responses),
            None,
        );
        assert!(!out.contains("experimental_bearer_token"), "{out}");
        assert!(out.contains("env_key"));
        assert!(out.contains("https://api.example.com"));
    }

    #[test]
    fn auth_style_none_writes_no_credential_at_all() {
        // 官方 OAuth 登录的场景：只改端点，一个凭证都不许写进去。
        let mut m = meta(AuthStyle::None, WireApi::Responses);
        m.target = RelayTarget::ClaudeCode;
        let out = claude_settings_json("{}", &m, Some("sk-should-not-appear")).unwrap();
        assert!(!out.contains("sk-should-not-appear"));

        let out = codex_config_toml("", &meta(AuthStyle::None, WireApi::Responses), Some("sk-x"));
        assert!(!out.contains("sk-x"));
        assert!(!out.contains("env_key"));
    }
}
