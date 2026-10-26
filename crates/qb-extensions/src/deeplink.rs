// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 导入链接（中转站重构阶段 5，cc-switch 的 X4 Deep Link）：`qbgate://v1/import?resource=…` → 一个导入请求。
//!
//! 链接格式照搬 cc-switch 的 v1（`ccswitch://v1/import?…`），所以中转站页面上给 CC Switch 的导入链接
//! 粘贴进来也认。**系统里只注册 `qbgate://`**（安装包按 `tauri.conf.json` 的 `plugins.deep-link` 写进 HKCU）；
//! `ccswitch://` 是 cc-switch 的，不抢。
//!
//! 这里只解析、只核对形状 —— 不碰文件、不联网、不写库。导入在 `qb-app::usecase::deeplink_ops`，
//! 导入之前界面一律先让使用者确认，导入之后什么都不自动启用。
//!
//! 链接是不可信输入，守住的几条（照 cc-switch）：
//! - 地址只收 http / https；资源类型、软件名按白名单；
//! - 远程配置（`configUrl`）拒绝：不替链接去拉文件；
//! - Codex / Grok Build 的配置里让客户端执行命令、读本机凭据的键拒绝；只用 `env_key` 间接给 Key 的拒绝
//!   （那把 Key 在本机环境变量里，导进来就是把它发到链接给的地址去）；
//! - 图标（`icon` / `iconUrl`）丢掉：QB Gate 没有站点图标，也不替链接去下载东西；
//! - 链接要求「导入后直接用」（`enabled=true`）、带用量脚本（`usageScript` 一族）的，认出来但不照做，
//!   记在 [`ProviderLink::ignored`] 里由界面照实列出来（用量脚本要到阶段 6 才有）。

use crate::error::{GateError, Result};
use base64::prelude::*;
use serde_json::Value;
use std::collections::HashMap;

/// 系统里注册的协议名。
pub const SCHEME: &str = "qbgate";
/// cc-switch 的协议：格式相同，只认粘贴进来的（不在系统里注册）。
pub const CC_SWITCH_SCHEME: &str = "ccswitch";

/// 站点链接给哪个软件（QB Gate 能建线路的那几个）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkApp {
    Claude,
    Codex,
    Gemini,
    GrokBuild,
}

/// 链接声明的上游协议（cc-switch 的 `apiFormat`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkFormat {
    Anthropic,
    OpenAiChat,
    OpenAiResponses,
    GeminiNative,
}

impl LinkFormat {
    fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "anthropic" => Ok(Self::Anthropic),
            "openai_chat" => Ok(Self::OpenAiChat),
            "openai_responses" => Ok(Self::OpenAiResponses),
            "gemini_native" => Ok(Self::GeminiNative),
            other => Err(bad(format!(
                "不认识的上游协议「{other}」（apiFormat 只能是 anthropic、openai_chat、openai_responses、gemini_native）"
            ))),
        }
    }

    /// 这个软件经本机路由能不能用这种上游（照 cc-switch 的 `native_value`：Gemini CLI 只有原生接口，
    /// Codex / Grok Build 转不了 Gemini）。
    fn usable_by(self, app: LinkApp) -> bool {
        use LinkFormat::*;
        match app {
            LinkApp::Claude => true,
            LinkApp::Codex | LinkApp::GrokBuild => self != GeminiNative,
            LinkApp::Gemini => self == GeminiNative,
        }
    }
}

/// 一条站点链接。`api_key` 只在后端用：给界面的形状由 `qb-app` 另做、Key 打码。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProviderLink {
    pub app: Option<LinkApp>,
    pub name: String,
    /// 第一个是主地址，其余是备用地址（多端点）。
    pub endpoints: Vec<String>,
    pub api_key: Option<String>,
    pub homepage: Option<String>,
    pub notes: Option<String>,
    /// 主模型（Claude 的 `ANTHROPIC_MODEL`、Codex / Gemini / Grok 的 `model`）。
    pub model: Option<String>,
    pub haiku_model: Option<String>,
    pub sonnet_model: Option<String>,
    pub opus_model: Option<String>,
    pub api_format: Option<LinkFormat>,
    /// 认出来但没照做的部分，界面照实列出来。
    pub ignored: Vec<String>,
}

/// 一条 MCP 链接：`config` 里 `mcpServers` 的每一条。
#[derive(Debug, Clone, PartialEq)]
pub struct McpLink {
    /// 链接说要给哪些软件用（只给界面看：导入后一个目标都不勾）。
    pub apps: Vec<String>,
    pub servers: Vec<(String, Value)>,
    pub ignored: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromptLink {
    pub app: String,
    pub name: String,
    pub content: String,
    pub description: Option<String>,
    pub ignored: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkillLink {
    /// `owner/name`。
    pub repo: String,
    pub directory: Option<String>,
    pub branch: Option<String>,
}

impl SkillLink {
    /// Skills 面板认的来源地址（`https://github.com/<仓库>/tree/<分支>/<目录>`）。
    pub fn source(&self) -> String {
        crate::skills::tree_url(
            &self.repo,
            self.branch.as_deref().unwrap_or("HEAD"),
            self.directory.as_deref().unwrap_or(""),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeepLink {
    Provider(ProviderLink),
    Mcp(McpLink),
    Prompt(PromptLink),
    Skill(SkillLink),
}

fn bad(msg: impl Into<String>) -> GateError {
    GateError::Other(msg.into())
}

/// 解析一条导入链接（`qbgate://` 或粘贴进来的 `ccswitch://`）。
pub fn parse(raw: &str) -> Result<DeepLink> {
    let url = url::Url::parse(raw.trim()).map_err(|e| bad(format!("这不是一条链接：{e}")))?;
    let scheme = url.scheme();
    if scheme != SCHEME && scheme != CC_SWITCH_SCHEME {
        return Err(bad(format!(
            "只认 {SCHEME}:// 和 {CC_SWITCH_SCHEME}:// 开头的导入链接，这条是 {scheme}://"
        )));
    }
    let version = url.host_str().unwrap_or("");
    if version != "v1" {
        return Err(bad(format!("不认识的链接版本「{version}」（只认 v1）")));
    }
    if url.path() != "/import" {
        return Err(bad(format!(
            "链接的路径应该是 /import，这条是 {}",
            url.path()
        )));
    }
    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
    let resource = params
        .get("resource")
        .ok_or_else(|| bad("链接里没写导入什么（resource）"))?;
    match resource.as_str() {
        "provider" => parse_provider(&params).map(DeepLink::Provider),
        "mcp" => parse_mcp(&params).map(DeepLink::Mcp),
        "prompt" => parse_prompt(&params).map(DeepLink::Prompt),
        "skill" => parse_skill(&params).map(DeepLink::Skill),
        other => Err(bad(format!(
            "不认识的导入类型「{other}」（只认 provider、mcp、prompt、skill）"
        ))),
    }
}

fn get<'a>(params: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    params.get(key).map(|v| v.trim()).filter(|v| !v.is_empty())
}

fn owned(params: &HashMap<String, String>, key: &str) -> Option<String> {
    get(params, key).map(str::to_string)
}

/// Base64 的参数**不修剪空格**：URL 里的 `+` 常被解成空格，修剪掉就少一个字（[`decode_base64`] 会把空格换回 `+`）。
fn base64_param<'a>(params: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .map(String::as_str)
        .filter(|v| !v.trim().is_empty())
}

fn wants_enabled(params: &HashMap<String, String>) -> bool {
    get(params, "enabled").is_some_and(|v| v == "true")
}

/// 地址只收 http / https。
fn check_url(value: &str, field: &str) -> Result<()> {
    let url = url::Url::parse(value).map_err(|e| bad(format!("{field} 不是一个地址：{e}")))?;
    match url.scheme() {
        "http" | "https" => Ok(()),
        other => Err(bad(format!(
            "{field} 只能是 http / https 地址，这个是 {other}://"
        ))),
    }
}

const COEXIST_APPS: [&str; 4] = ["opencode", "openclaw", "hermes", "pi"];

fn parse_provider(params: &HashMap<String, String>) -> Result<ProviderLink> {
    let app_name = get(params, "app").ok_or_else(|| bad("站点链接里没写给哪个软件（app）"))?;
    let app = match app_name {
        "claude" => LinkApp::Claude,
        "codex" => LinkApp::Codex,
        "gemini" => LinkApp::Gemini,
        "grokbuild" => LinkApp::GrokBuild,
        other if COEXIST_APPS.contains(&other) => {
            return Err(bad(format!(
                "这条链接是给 {other} 的：QB Gate 里 OpenCode / OpenClaw / Hermes / Pi 用的线路从线路弹窗的「写进其他工具」来 —— \
                 先把这个站点加成 Claude Code 或 Codex 的线路，再写进去"
            )))
        }
        other => return Err(bad(format!("不认识的软件「{other}」"))),
    };
    let name = owned(params, "name").ok_or_else(|| bad("站点链接里没写名字（name）"))?;
    if params.contains_key("configUrl") {
        return Err(bad(
            "链接要面板去下载配置（configUrl）：不替链接拉文件，请让对方把配置直接放进链接（config）",
        ));
    }
    let mut link = ProviderLink {
        app: Some(app),
        name,
        endpoints: owned(params, "endpoint")
            .map(|ep| split_endpoints(&ep))
            .unwrap_or_default(),
        api_key: owned(params, "apiKey"),
        homepage: owned(params, "homepage"),
        notes: owned(params, "notes"),
        model: owned(params, "model"),
        haiku_model: owned(params, "haikuModel"),
        sonnet_model: owned(params, "sonnetModel"),
        opus_model: owned(params, "opusModel"),
        api_format: get(params, "apiFormat")
            .map(LinkFormat::parse)
            .transpose()?,
        ignored: Vec::new(),
    };
    if let Some(config) = base64_param(params, "config") {
        let value = decode_config(config, get(params, "configFormat").unwrap_or("json"))?;
        merge_config(&mut link, app, &value)?;
    }
    if link.endpoints.is_empty() {
        return Err(bad("链接里没有站点地址（endpoint）"));
    }
    for (i, ep) in link.endpoints.iter().enumerate() {
        check_url(ep, &format!("第 {} 个地址", i + 1))?;
    }
    if let Some(hp) = &link.homepage {
        check_url(hp, "主页")?;
    }
    if let Some(format) = link.api_format {
        if !format.usable_by(app) {
            return Err(bad(format!(
                "这个软件经本机路由用不了链接写的上游协议（{}）",
                get(params, "apiFormat").unwrap_or_default()
            )));
        }
    }
    if wants_enabled(params) {
        link.ignored
            .push("链接要求导入后直接用这条线：QB Gate 不替你切，导入后在中转站页点它".into());
    }
    if params.keys().any(|k| k.starts_with("usage")) {
        link.ignored
            .push("用量查询脚本：QB Gate 的自定义用量脚本要到阶段 6 才有，这一部分没导入".into());
    }
    Ok(link)
}

/// 逗号分隔的多个地址，去掉空白和结尾的 `/`。
fn split_endpoints(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// 解链接里 Base64 的参数（照 cc-switch 的 `decode_base64_param`：URL 里 `+` 常被解成空格、补位 `=` 常被吃掉，
/// 标准和 URL 安全两种字母表都试）。
pub fn decode_base64(field: &str, raw: &str) -> Result<Vec<u8>> {
    let trimmed = raw.trim_matches(|c| c == '\r' || c == '\n');
    let mut candidates: Vec<String> = Vec::new();
    if trimmed.contains(' ') {
        candidates.push(trimmed.replace(' ', "+"));
    }
    if !trimmed.is_empty() && !candidates.iter().any(|c| c == trimmed) {
        candidates.push(trimmed.to_string());
    }
    for c in candidates.clone() {
        let pad = (4 - c.len() % 4) % 4;
        if pad != 0 {
            candidates.push(format!("{c}{}", "=".repeat(pad)));
        }
    }
    for c in &candidates {
        for engine in [
            &BASE64_STANDARD,
            &BASE64_STANDARD_NO_PAD,
            &BASE64_URL_SAFE,
            &BASE64_URL_SAFE_NO_PAD,
        ] {
            if let Ok(bytes) = engine.decode(c) {
                return Ok(bytes);
            }
        }
    }
    Err(bad(format!(
        "{field} 不是有效的 Base64（链接里的 + 要写成 %2B，或者用 URL 安全的 Base64）"
    )))
}

fn decode_text(field: &str, raw: &str) -> Result<String> {
    String::from_utf8(decode_base64(field, raw)?)
        .map_err(|_| bad(format!("{field} 解出来不是 UTF-8 文字")))
}

/// `config` 参数：Base64 的 JSON 或 TOML，统一成 JSON 值。
fn decode_config(raw: &str, format: &str) -> Result<Value> {
    let text = decode_text("config", raw)?;
    match format {
        "json" => {
            serde_json::from_str(&text).map_err(|e| bad(format!("config 不是有效的 JSON：{e}")))
        }
        "toml" => toml_json(&text, "config"),
        other => Err(bad(format!(
            "不认识的配置格式「{other}」（只认 json、toml）"
        ))),
    }
}

fn toml_json(text: &str, what: &str) -> Result<Value> {
    toml_edit::de::from_str::<Value>(text).map_err(|e| bad(format!("{what} 不是有效的 TOML：{e}")))
}

fn str_at<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// 链接参数优先，配置里的只补空着的。
fn fill(slot: &mut Option<String>, value: Option<&str>) {
    if slot.is_none() {
        *slot = value.map(str::to_string);
    }
}

fn merge_config(link: &mut ProviderLink, app: LinkApp, config: &Value) -> Result<()> {
    let (endpoint, key, model, env_key) = match app {
        LinkApp::Claude => {
            let env = config
                .get("env")
                .filter(|e| e.is_object())
                .ok_or_else(|| bad("Claude Code 的配置里要有 env"))?;
            fill(
                &mut link.haiku_model,
                str_at(env, "ANTHROPIC_DEFAULT_HAIKU_MODEL"),
            );
            fill(
                &mut link.sonnet_model,
                str_at(env, "ANTHROPIC_DEFAULT_SONNET_MODEL"),
            );
            fill(
                &mut link.opus_model,
                str_at(env, "ANTHROPIC_DEFAULT_OPUS_MODEL"),
            );
            (
                str_at(env, "ANTHROPIC_BASE_URL").map(str::to_string),
                str_at(env, "ANTHROPIC_AUTH_TOKEN")
                    .or_else(|| str_at(env, "ANTHROPIC_API_KEY"))
                    .map(str::to_string),
                str_at(env, "ANTHROPIC_MODEL").map(str::to_string),
                false,
            )
        }
        LinkApp::Gemini => (
            str_at(config, "GOOGLE_GEMINI_BASE_URL")
                .or_else(|| str_at(config, "GEMINI_BASE_URL"))
                .map(str::to_string),
            str_at(config, "GEMINI_API_KEY").map(str::to_string),
            str_at(config, "GEMINI_MODEL").map(str::to_string),
            false,
        ),
        LinkApp::Codex => codex_config(config)?,
        LinkApp::GrokBuild => grok_config(config)?,
    };
    if link.endpoints.is_empty() {
        link.endpoints = endpoint.map(|e| split_endpoints(&e)).unwrap_or_default();
    }
    fill(&mut link.api_key, key.as_deref());
    fill(&mut link.model, model.as_deref());
    // 只用 env_key 间接给 Key、链接自己又没给：那把 Key 在这台电脑的环境变量里，导进来就是把它发到链接给的地址去。
    if link.api_key.is_none() && env_key {
        return Err(bad(
            "这条链接只用 env_key 间接给 Key（Key 在这台电脑的环境变量里）：不能从链接导入 —— 请手动添加、自己填 Key",
        ));
    }
    Ok(())
}

/// Codex / Grok Build 的配置文本：`{"config": "<toml>"}`，或者整份就是那份配置（TOML 转来的 JSON 对象）。
fn config_document(config: &Value, what: &str) -> Result<Value> {
    match config.get("config").and_then(Value::as_str) {
        Some(text) => toml_json(text, what),
        None => Ok(config.clone()),
    }
}

/// 让 Codex 执行命令（`auth`）、读本机云凭据或环境变量取凭据的键（照 cc-switch 的 `codex_link`）。
const CODEX_REJECTED: [&str; 4] = ["auth", "aws", "gateway_oauth", "env_http_headers"];
/// 让 Grok Build 运行凭据提供器、读环境变量或本机证书的键（照 cc-switch 的 `grok_link`）。
const GROK_REJECTED: [&str; 3] = ["auth_provider", "env_http_headers", "mtls_cert_dir"];

fn reject_keys(table: &Value, keys: &[&str], who: &str) -> Result<()> {
    if let Some(k) = keys.iter().find(|k| table.get(**k).is_some()) {
        return Err(bad(format!(
            "{who} 的配置里有 {k}：它会让客户端执行命令或读这台电脑上的凭据，不能从链接导入 —— 请手动添加这个站点"
        )));
    }
    Ok(())
}

/// （地址, Key, 模型, 选中的那张表用不用 `env_key` 取 Key）。
type Extracted = (Option<String>, Option<String>, Option<String>, bool);

fn codex_config(config: &Value) -> Result<Extracted> {
    let doc = config_document(config, "Codex 的 config.toml")?;
    let providers = doc.get("model_providers").and_then(Value::as_object);
    let table = match (str_at(&doc, "model_provider"), providers) {
        (Some(id), Some(p)) => p.get(id).cloned(),
        (None, Some(p)) if p.len() == 1 => p.values().next().cloned(),
        _ => None,
    }
    .unwrap_or(Value::Null);
    reject_keys(&table, &CODEX_REJECTED, "Codex")?;
    let key = config
        .get("auth")
        .and_then(|a| str_at(a, "OPENAI_API_KEY"))
        .or_else(|| str_at(&table, "experimental_bearer_token"))
        .map(str::to_string);
    Ok((
        str_at(&table, "base_url")
            .or_else(|| str_at(&doc, "base_url"))
            .map(str::to_string),
        key,
        str_at(&doc, "model").map(str::to_string),
        table.get("env_key").is_some(),
    ))
}

fn grok_config(config: &Value) -> Result<Extracted> {
    let doc = config_document(config, "Grok Build 的 config.toml")?;
    let models = doc.get("model").and_then(Value::as_object);
    let default = doc.get("models").and_then(|m| str_at(m, "default"));
    let table = match (default, models) {
        (Some(name), Some(m)) => m.get(name).cloned(),
        (None, Some(m)) if m.len() == 1 => m.values().next().cloned(),
        _ => None,
    }
    .unwrap_or(Value::Null);
    reject_keys(&table, &GROK_REJECTED, "Grok Build")?;
    Ok((
        str_at(&table, "base_url").map(str::to_string),
        str_at(&table, "api_key").map(str::to_string),
        str_at(&table, "model").map(str::to_string),
        table.get("env_key").is_some(),
    ))
}

/// MCP 链接的 `apps`、提示词链接的 `app` 认的软件（cc-switch 的全部；只给界面看）。
const KNOWN_APPS: [&str; 10] = [
    "claude",
    "codex",
    "gemini",
    "grokbuild",
    "grok",
    "opencode",
    "openclaw",
    "hermes",
    "pi",
    "mcode",
];

fn parse_mcp(params: &HashMap<String, String>) -> Result<McpLink> {
    let apps: Vec<String> = get(params, "apps")
        .ok_or_else(|| bad("MCP 链接里没写给哪些软件（apps）"))?
        .split(',')
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect();
    if let Some(a) = apps.iter().find(|a| !KNOWN_APPS.contains(&a.as_str())) {
        return Err(bad(format!("MCP 链接里有不认识的软件「{a}」")));
    }
    let config = decode_text(
        "config",
        base64_param(params, "config").ok_or_else(|| bad("MCP 链接里没有配置（config）"))?,
    )?;
    let json: Value =
        serde_json::from_str(&config).map_err(|e| bad(format!("MCP 配置不是有效的 JSON：{e}")))?;
    let servers = json
        .get("mcpServers")
        .and_then(Value::as_object)
        .ok_or_else(|| bad("MCP 配置里要有 mcpServers"))?;
    if servers.is_empty() {
        return Err(bad("MCP 配置里一条服务器都没有"));
    }
    let mut out = Vec::new();
    for (id, spec) in servers {
        crate::mcp::validate_id(id)?;
        crate::mcp::validate(spec).map_err(|e| bad(format!("MCP「{id}」：{e}")))?;
        out.push((id.clone(), spec.clone()));
    }
    let mut ignored = Vec::new();
    if wants_enabled(params) {
        ignored.push(
            "链接要求导入后直接启用：QB Gate 不替你写进任何配置，导入后去 MCP 分页勾目标".into(),
        );
    }
    Ok(McpLink {
        apps,
        servers: out,
        ignored,
    })
}

fn parse_prompt(params: &HashMap<String, String>) -> Result<PromptLink> {
    let app = owned(params, "app").ok_or_else(|| bad("提示词链接里没写给哪个软件（app）"))?;
    if !KNOWN_APPS.contains(&app.as_str()) || app == "grok" {
        return Err(bad(format!("提示词链接里有不认识的软件「{app}」")));
    }
    let name = owned(params, "name").ok_or_else(|| bad("提示词链接里没写名字（name）"))?;
    let content = decode_text(
        "content",
        base64_param(params, "content").ok_or_else(|| bad("提示词链接里没有内容（content）"))?,
    )?;
    let mut ignored = Vec::new();
    if wants_enabled(params) {
        ignored
            .push("链接要求导入后直接启用：QB Gate 只放进提示词库，导入后去提示词分页挑它".into());
    }
    Ok(PromptLink {
        app,
        name,
        content,
        description: owned(params, "description"),
        ignored,
    })
}

/// GitHub 仓库名、目录、分支里能出现的字。
fn plain_segment(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn parse_skill(params: &HashMap<String, String>) -> Result<SkillLink> {
    let repo = owned(params, "repo").ok_or_else(|| bad("Skill 链接里没写仓库（repo）"))?;
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() != 2 || !parts.iter().all(|p| plain_segment(p)) {
        return Err(bad(format!(
            "Skill 链接的仓库要写成 owner/name，这条是「{repo}」"
        )));
    }
    let directory = owned(params, "directory").map(|d| d.trim_matches('/').to_string());
    if let Some(d) = &directory {
        if !d.split('/').all(plain_segment) {
            return Err(bad(format!("Skill 链接的目录不对：「{d}」")));
        }
    }
    let branch = owned(params, "branch");
    if let Some(b) = &branch {
        if !b.split('/').all(plain_segment) {
            return Err(bad(format!("Skill 链接的分支不对：「{b}」")));
        }
    }
    Ok(SkillLink {
        repo,
        directory,
        branch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(s: &str) -> String {
        BASE64_STANDARD.encode(s)
    }

    fn enc(s: &str) -> String {
        url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
    }

    fn provider(url: &str) -> ProviderLink {
        match parse(url).unwrap() {
            DeepLink::Provider(p) => p,
            other => panic!("不是站点链接：{other:?}"),
        }
    }

    #[test]
    fn a_provider_link_reads_every_field_and_both_schemes_parse_the_same() {
        let tail = "v1/import?resource=provider&app=claude&name=Test%20Provider&homepage=https%3A%2F%2Fexample.com\
                    &endpoint=https%3A%2F%2Fapi1.example.com%2F%20,%20https%3A%2F%2Fapi2.example.com&apiKey=sk-test-123\
                    &icon=claude&iconUrl=https%3A%2F%2Fexample.com%2Fi.png&model=m-main&haikuModel=m-h&apiFormat=openai_chat";
        let a = provider(&format!("qbgate://{tail}"));
        let b = provider(&format!("ccswitch://{tail}"));
        assert_eq!(a, b, "两种协议同一个格式");
        assert_eq!(a.app, Some(LinkApp::Claude));
        assert_eq!(a.name, "Test Provider");
        assert_eq!(
            a.endpoints,
            ["https://api1.example.com", "https://api2.example.com"],
            "多个地址去空白、去结尾斜杠"
        );
        assert_eq!(a.api_key.as_deref(), Some("sk-test-123"));
        assert_eq!(a.model.as_deref(), Some("m-main"));
        assert_eq!(a.haiku_model.as_deref(), Some("m-h"));
        assert_eq!(a.api_format, Some(LinkFormat::OpenAiChat));
        assert!(a.ignored.is_empty(), "图标直接丢掉，不算没导入的部分");
    }

    #[test]
    fn untrusted_parts_are_refused_or_reported_never_followed() {
        let base = "qbgate://v1/import?resource=provider&app=claude&name=T&endpoint=https%3A%2F%2Fapi.example.com";
        // 远程配置：拒绝。
        assert!(parse(&format!(
            "{base}&configUrl=https%3A%2F%2Fx.example%2Fc.json"
        ))
        .is_err());
        // 地址只收 http / https。
        assert!(parse("qbgate://v1/import?resource=provider&app=claude&name=T&endpoint=file%3A%2F%2F%2Fc%3A%2Fx").is_err());
        assert!(parse(&format!("{base}&homepage=javascript%3Aalert(1)")).is_err());
        // 没有地址：拒绝。
        assert!(parse("qbgate://v1/import?resource=provider&app=claude&name=T&apiKey=k").is_err());
        // 「导入后直接用」和用量脚本：认出来，记成没照做的部分。
        let p = provider(&format!("{base}&enabled=true&usageScript={}", b64("x")));
        assert_eq!(p.ignored.len(), 2, "{:?}", p.ignored);
        // 共存型工具：说清楚该怎么做。
        let e = parse("qbgate://v1/import?resource=provider&app=opencode&name=T&endpoint=https%3A%2F%2Fa.example")
            .unwrap_err()
            .to_string();
        assert!(e.contains("写进其他工具"), "{e}");
        // 协议名、版本、路径、类型不对的都拒绝。
        for bad in [
            "https://v1/import?resource=provider",
            "qbgate://v2/import?resource=provider&app=claude&name=T&endpoint=https%3A%2F%2Fa.example",
            "qbgate://v1/export?resource=provider",
            "qbgate://v1/import?resource=theme",
            "qbgate://v1/import",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_api_format_has_to_fit_the_software() {
        let link = |app: &str, f: &str| {
            parse(&format!(
                "qbgate://v1/import?resource=provider&app={app}&name=T&endpoint=https%3A%2F%2Fa.example&apiFormat={f}"
            ))
        };
        assert!(link("claude", "gemini_native").is_ok());
        assert!(link("codex", "anthropic").is_ok());
        assert!(
            link("codex", "gemini_native").is_err(),
            "Codex 转不了 Gemini"
        );
        assert!(
            link("gemini", "openai_chat").is_err(),
            "Gemini CLI 只有原生接口"
        );
        assert!(link("grokbuild", "openai_responses").is_ok());
        assert!(link("claude", "soap").is_err());
    }

    #[test]
    fn config_fills_only_what_the_link_left_out() {
        let claude = b64(
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://cfg.example.com","ANTHROPIC_AUTH_TOKEN":"cfg-key",
                "ANTHROPIC_MODEL":"cfg-main","ANTHROPIC_DEFAULT_OPUS_MODEL":"cfg-opus"}}"#,
        );
        let p = provider(&format!(
            "qbgate://v1/import?resource=provider&app=claude&name=T&apiKey=url-key&config={}",
            enc(&claude)
        ));
        assert_eq!(p.endpoints, ["https://cfg.example.com"]);
        assert_eq!(p.api_key.as_deref(), Some("url-key"), "链接参数优先");
        assert_eq!(p.model.as_deref(), Some("cfg-main"));
        assert_eq!(p.opus_model.as_deref(), Some("cfg-opus"));

        let gemini = b64(
            r#"{"GOOGLE_GEMINI_BASE_URL":"https://g.example.com","GEMINI_API_KEY":"g","GEMINI_MODEL":"gm"}"#,
        );
        let g = provider(&format!(
            "qbgate://v1/import?resource=provider&app=gemini&name=G&config={}",
            enc(&gemini)
        ));
        assert_eq!(
            (
                g.endpoints[0].as_str(),
                g.api_key.as_deref(),
                g.model.as_deref()
            ),
            ("https://g.example.com", Some("g"), Some("gm"))
        );
    }

    #[test]
    fn codex_and_grok_configs_refuse_local_credentials_and_env_key_only_links() {
        let codex = |toml: &str, auth: &str| {
            let cfg = serde_json::json!({ "auth": serde_json::from_str::<Value>(auth).unwrap(), "config": toml });
            parse(&format!(
                "qbgate://v1/import?resource=provider&app=codex&name=C&config={}",
                enc(&b64(&cfg.to_string()))
            ))
        };
        let ok = codex(
            "model_provider = \"relay\"\nmodel = \"gpt-x\"\n[model_providers.relay]\nbase_url = \"https://c.example.com/v1\"\n",
            r#"{"OPENAI_API_KEY":"ck"}"#,
        )
        .unwrap();
        let DeepLink::Provider(c) = ok else { panic!() };
        assert_eq!(
            (
                c.endpoints[0].as_str(),
                c.api_key.as_deref(),
                c.model.as_deref()
            ),
            ("https://c.example.com/v1", Some("ck"), Some("gpt-x"))
        );
        // 只给 env_key、不给 Key：拒绝；链接参数自己带了 Key 就用它（照 cc-switch）。
        let env_only =
            "model_provider = \"r\"\n[model_providers.r]\nbase_url = \"https://c.example.com\"\nenv_key = \"MY_KEY\"\n";
        assert!(codex(env_only, "{}").is_err());
        let cfg = serde_json::json!({ "config": env_only });
        assert!(parse(&format!(
            "qbgate://v1/import?resource=provider&app=codex&name=C&apiKey=url-key&config={}",
            enc(&b64(&cfg.to_string()))
        ))
        .is_ok());
        // 让 Codex 执行命令取凭据：拒绝。
        assert!(codex(
            "model_provider = \"r\"\n[model_providers.r]\nbase_url = \"https://c.example.com\"\n[model_providers.r.auth]\ncommand = \"x\"\n",
            r#"{"OPENAI_API_KEY":"ck"}"#
        )
        .is_err());

        let grok = |toml: &str| {
            parse(&format!(
                "qbgate://v1/import?resource=provider&app=grokbuild&name=G&configFormat=toml&config={}",
                enc(&b64(toml))
            ))
        };
        let DeepLink::Provider(g) = grok(
            "[models]\ndefault = \"relay\"\n[model.relay]\nmodel = \"grok-4.5\"\nbase_url = \"https://x.example.com/v1\"\napi_key = \"gk\"\n",
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(g.api_key.as_deref(), Some("gk"));
        assert!(
            grok("[model.relay]\nbase_url = \"https://x.example.com\"\nenv_key = \"K\"\n").is_err()
        );
        assert!(grok("[model.relay]\nbase_url = \"https://x.example.com\"\napi_key = \"k\"\nmtls_cert_dir = \"C:/x\"\n").is_err());
    }

    #[test]
    fn mcp_prompt_and_skill_links() {
        let mcp = b64(
            r#"{"mcpServers":{"fetch":{"type":"stdio","command":"uvx","args":["mcp-server-fetch"]}}}"#,
        );
        let DeepLink::Mcp(m) = parse(&format!(
            "qbgate://v1/import?resource=mcp&apps=claude,codex&config={}&enabled=true",
            enc(&mcp)
        ))
        .unwrap() else {
            panic!()
        };
        assert_eq!(m.apps, ["claude", "codex"]);
        assert_eq!(m.servers[0].0, "fetch");
        assert_eq!(m.ignored.len(), 1, "不自动启用，说出来");
        assert!(parse(&format!(
            "qbgate://v1/import?resource=mcp&apps=notepad&config={}",
            enc(&mcp)
        ))
        .is_err());

        // 「+」在 URL 里常被解成空格：照样解得出来（cc-switch 的那个坑）。
        let DeepLink::Prompt(p) =
            parse("qbgate://v1/import?resource=prompt&app=codex&name=PromptPlus&content=Pj4+")
                .unwrap()
        else {
            panic!()
        };
        assert_eq!(p.content, ">>>");

        let DeepLink::Skill(s) = parse(
            "qbgate://v1/import?resource=skill&repo=owner/repo&directory=skills/pdf&branch=dev",
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(
            s.source(),
            "https://github.com/owner/repo/tree/dev/skills/pdf"
        );
        let DeepLink::Skill(s) = parse("qbgate://v1/import?resource=skill&repo=o/r").unwrap()
        else {
            panic!()
        };
        assert_eq!(s.source(), "https://github.com/o/r/tree/HEAD");
        for bad in ["o", "o/r/x", "../r", "o/r%20x"] {
            assert!(
                parse(&format!("qbgate://v1/import?resource=skill&repo={bad}")).is_err(),
                "{bad}"
            );
        }
        assert!(parse("qbgate://v1/import?resource=skill&repo=o/r&directory=../etc").is_err());
    }
}
