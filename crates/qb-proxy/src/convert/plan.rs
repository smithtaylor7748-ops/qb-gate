// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 选哪一种转换、请求和地址怎么改、响应怎么换回去（中转站重构阶段 3）。
//!
//! cc-switch 把这些判断散在适配器（`providers/claude.rs`、`providers/codex.rs`）、转发器（`forwarder.rs`）
//! 和处理器（`handlers.rs`）里，靠读它自己的供应商配置（`meta.apiFormat`、`settings_config.base_url`、
//! Codex 配置里的 `wire_api`）决定。QB Gate 的线路有显式的上游协议（`Route.upstream_format`），所以这里只收
//! 两样东西：**这条线说什么协议**（[`ApiFormat`]，`None` = 跟客户端一样）和**这条线的地址**。
//! 规则照搬，只有下面几处不同：
//!
//! - 没有「猜协议」：cc-switch 会从 Codex 的地址是不是以 `/chat/completions` 结尾猜它说 Chat；QB Gate 只认线路上
//!   写明的那一个（写错了，上游会如实报错）。
//! - Copilot / Codex OAuth / xAI OAuth 的分支一概不搬（QB Gate 不做那些登录）。
//! - cc-switch 把显式配置的 `prompt_cache_key`、Codex 转 Chat 的 reasoning 参数挂在供应商 `meta` 上，界面可改；
//!   QB Gate 没有这两个设置，一律按 cc-switch「没配置时」的那条推断规则走（按地址与模型名）。
//! - 拼地址：转出去的路径写成带 `/v1` 的（`/v1/chat/completions`、`/v1/messages`），交给 `join_url` 去掉重复的
//!   那段 —— 跟 QB Gate 原样转发时拼地址是同一条规矩，不再另起一套「基地址算不算 API 根」的判断。

use super::codex_chat_common::fill_gemini_thought_signature_fallback;
use super::gemini_shadow::GeminiShadowStore;
use super::transform_codex_chat::{build_codex_tool_context_from_request, CodexToolContext};
use super::transform_codex_responses_namespace::{
    flatten_request_namespaces, namespace_restore_map, restore_response_namespaces, NamespacedName,
};
use super::transform_codex_responses_xai_sanitize::apply_xai_native_responses_request_compat;
use super::transform_gemini::AnthropicToolSchemaHints;
use super::{
    gemini_url, sse_aggregate, streaming, streaming_codex_anthropic, streaming_codex_chat,
    streaming_gemini, streaming_responses, transform, transform_codex_anthropic,
    transform_codex_chat, transform_codex_chat_moonshot_schema, transform_gemini,
    transform_responses, CodexChatReasoningConfig,
};
use crate::model_mapper::strip_one_m_suffix_for_upstream;
use crate::proxy_error::ProxyError;
use crate::types::OptimizerConfig;
use bytes::Bytes;
use futures::stream::{Stream, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

/// 上游说的协议（cc-switch 的 `apiFormat`）。跟 `qb_station::station::route::UpstreamFormat` 一一对应 ——
/// 那个在第 3 层的另一个 crate，这里够不着，由 `qb-app` 换过来。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiFormat {
    Anthropic,
    OpenaiChat,
    OpenaiResponses,
    GeminiNative,
}

impl ApiFormat {
    /// cc-switch 的写法（日志里也用它）。
    pub fn as_str(self) -> &'static str {
        match self {
            ApiFormat::Anthropic => "anthropic",
            ApiFormat::OpenaiChat => "openai_chat",
            ApiFormat::OpenaiResponses => "openai_responses",
            ApiFormat::GeminiNative => "gemini_native",
        }
    }
}

/// 这一发在这条线上要做哪一种转换。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conversion {
    /// 原样转发（客户端跟上游说同一种协议，或者这条路径不归转换管）。
    Passthrough,
    /// Claude Code / 桌面端（Anthropic Messages）→ 这条线的协议。只会是 Chat / Responses / Gemini 原生。
    ClaudeTo(ApiFormat),
    /// Codex（Responses）→ OpenAI Chat。
    ResponsesToChat,
    /// Codex（Responses）→ Anthropic Messages。
    ResponsesToAnthropic,
}

impl Conversion {
    pub fn is_passthrough(self) -> bool {
        self == Conversion::Passthrough
    }

    /// 上游说的协议（选鉴权头用）。`client_is_claude` 只在原样转发时用得上。
    pub fn upstream(self, client_is_claude: bool) -> ApiFormat {
        match self {
            Conversion::Passthrough if client_is_claude => ApiFormat::Anthropic,
            Conversion::Passthrough => ApiFormat::OpenaiResponses,
            Conversion::ClaudeTo(f) => f,
            Conversion::ResponsesToChat => ApiFormat::OpenaiChat,
            Conversion::ResponsesToAnthropic => ApiFormat::Anthropic,
        }
    }

    /// 一句话（日志、界面）。
    pub fn label(self) -> &'static str {
        match self {
            Conversion::Passthrough => "原样转发",
            Conversion::ClaudeTo(ApiFormat::OpenaiChat) => "Anthropic → OpenAI Chat",
            Conversion::ClaudeTo(ApiFormat::OpenaiResponses) => "Anthropic → OpenAI Responses",
            Conversion::ClaudeTo(ApiFormat::GeminiNative) => "Anthropic → Gemini",
            Conversion::ClaudeTo(ApiFormat::Anthropic) => "原样转发",
            Conversion::ResponsesToChat => "Responses → OpenAI Chat",
            Conversion::ResponsesToAnthropic => "Responses → Anthropic",
        }
    }
}

/// 这条线说的协议这个客户端用不了（目前只有一种：Codex 的线标成 Gemini 原生 —— cc-switch 也没有这条转换）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported(pub &'static str);

/// Claude 那边归转换管的路径（cc-switch 的 `is_claude_messages_path`）。
pub fn is_claude_messages_path(endpoint: &str) -> bool {
    matches!(
        split_endpoint_and_query(endpoint).0,
        "/v1/messages" | "/claude/v1/messages"
    )
}

/// Codex 那边归转换管的路径（cc-switch 的 `is_codex_responses_endpoint`）。
pub fn is_codex_responses_endpoint(endpoint: &str) -> bool {
    matches!(
        split_endpoint_and_query(endpoint).0,
        "/responses" | "/v1/responses" | "/responses/compact" | "/v1/responses/compact"
    )
}

/// 选转换。`path` 是要转给上游的路径（`route_of` 剥掉软件前缀之后的那个）。
///
/// 不归转换管的路径（`/v1/models`、`count_tokens`、Codex 旧版的 `/v1/chat/completions`……）一律原样转发：
/// 上游不认就如实回 404，本机路由按「这条线不接这一发」换下一条（跟 cc-switch 一样不在这里替上游答）。
pub fn select(
    client_is_claude: bool,
    path: &str,
    upstream: Option<ApiFormat>,
) -> Result<Conversion, Unsupported> {
    let Some(format) = upstream else {
        return Ok(Conversion::Passthrough);
    };
    if client_is_claude {
        if !is_claude_messages_path(path) || format == ApiFormat::Anthropic {
            return Ok(Conversion::Passthrough);
        }
        return Ok(Conversion::ClaudeTo(format));
    }
    if !is_codex_responses_endpoint(path) {
        return Ok(Conversion::Passthrough);
    }
    match format {
        ApiFormat::OpenaiResponses => Ok(Conversion::Passthrough),
        ApiFormat::OpenaiChat => Ok(Conversion::ResponsesToChat),
        ApiFormat::Anthropic => Ok(Conversion::ResponsesToAnthropic),
        ApiFormat::GeminiNative => Err(Unsupported(
            "Codex 说的是 Responses，这条线标成了 Gemini 原生 —— 这一对转不了（cc-switch 也没有），换一条线或改线路协议",
        )),
    }
}

// ---------------------------------------------------------------- 地址（照搬 cc-switch forwarder.rs）

pub fn split_endpoint_and_query(endpoint: &str) -> (&str, Option<&str>) {
    endpoint
        .split_once('?')
        .map_or((endpoint, None), |(path, query)| (path, Some(query)))
}

fn strip_beta_query(query: Option<&str>) -> Option<String> {
    let filtered = query.map(|query| {
        query
            .split('&')
            .filter(|pair| !pair.is_empty() && !pair.starts_with("beta="))
            .collect::<Vec<_>>()
            .join("&")
    });

    match filtered.as_deref() {
        Some("") | None => None,
        Some(_) => filtered,
    }
}

fn merge_query_params(base_query: Option<&str>, extra_param: Option<&str>) -> Option<String> {
    let mut params: Vec<String> = base_query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .filter(|pair| !pair.is_empty())
        .filter(|pair| !pair.starts_with("alt="))
        .map(ToString::to_string)
        .collect();

    if let Some(extra_param) = extra_param {
        params.push(extra_param.to_string());
    }

    if params.is_empty() {
        None
    } else {
        Some(params.join("&"))
    }
}

fn with_query(target_path: &str, query: Option<&str>) -> String {
    match query {
        Some(query) if !query.is_empty() => format!("{target_path}?{query}"),
        _ => target_path.to_string(),
    }
}

/// Claude 的 `/v1/messages` 改打哪个路径（cc-switch 的 `rewrite_claude_transform_endpoint`，去掉 Copilot 那两支）。
/// Anthropic 的 `?beta=true` 查询串去掉；Gemini 按模型与是否流式拼 `generateContent` / `streamGenerateContent?alt=sse`。
pub fn rewrite_claude_transform_endpoint(
    endpoint: &str,
    api_format: ApiFormat,
    body: &Value,
) -> String {
    let (path, query) = split_endpoint_and_query(endpoint);
    if !is_claude_messages_path(path) {
        return endpoint.to_string();
    }
    let passthrough_query = strip_beta_query(query);

    if api_format == ApiFormat::GeminiNative {
        let model = transform_gemini::extract_gemini_model(body).unwrap_or("unknown");
        // Accept both bare ids (`gemini-2.5-pro`) and the resource-name
        // form (`models/gemini-2.5-pro`) that Gemini SDKs emit.
        let model = gemini_url::normalize_gemini_model_id(model);
        let is_stream = body
            .get("stream")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let target_path = if is_stream {
            format!("/v1beta/models/{model}:streamGenerateContent")
        } else {
            format!("/v1beta/models/{model}:generateContent")
        };
        let rewritten_query = merge_query_params(
            passthrough_query.as_deref(),
            if is_stream { Some("alt=sse") } else { None },
        );
        return with_query(&target_path, rewritten_query.as_deref());
    }

    let target_path = if api_format == ApiFormat::OpenaiResponses {
        "/v1/responses"
    } else {
        "/v1/chat/completions"
    };
    with_query(target_path, passthrough_query.as_deref())
}

/// Codex 的 Responses 改打 Chat（cc-switch 的 `rewrite_codex_responses_endpoint_to_chat`）。查询串原样带上。
pub fn rewrite_codex_responses_endpoint_to_chat(endpoint: &str) -> String {
    with_query("/v1/chat/completions", split_endpoint_and_query(endpoint).1)
}

/// Codex 的 Responses 改打 Anthropic Messages（cc-switch 的 `rewrite_codex_responses_endpoint_to_anthropic`）。
pub fn rewrite_codex_responses_endpoint_to_anthropic(endpoint: &str) -> String {
    with_query("/v1/messages", split_endpoint_and_query(endpoint).1)
}

/// 基地址是不是已经写到了具体端点（cc-switch 的 `base_url_is_full_endpoint`）：使用者把 `.../v1/messages`、
/// `.../chat/completions` 整条贴进来时，不再往后拼一遍（拼两遍是一个不可重试的 400）。`endpoint_suffix` 要小写。
pub fn base_url_is_full_endpoint(base_url: &str, endpoint_suffix: &str) -> bool {
    let trimmed = base_url.trim();
    let path = match trimmed.split_once(['?', '#']) {
        Some((head, _)) => head,
        None => trimmed,
    };
    path.trim_end_matches('/')
        .to_ascii_lowercase()
        .ends_with(endpoint_suffix)
}

/// 这一种转换的请求打哪个完整地址。`join` 是 QB Gate 原样转发时拼地址那一个（`local_router::join_url`）。
pub fn upstream_url(
    conversion: Conversion,
    base_url: &str,
    endpoint: &str,
    join: impl Fn(&str, &str) -> String,
) -> String {
    match conversion {
        Conversion::ClaudeTo(ApiFormat::GeminiNative) => {
            gemini_url::resolve_gemini_native_url(base_url, endpoint, false)
        }
        Conversion::ResponsesToChat if base_url_is_full_endpoint(base_url, "/chat/completions") => {
            append_query(base_url, split_endpoint_and_query(endpoint).1)
        }
        Conversion::ResponsesToAnthropic if base_url_is_full_endpoint(base_url, "/v1/messages") => {
            append_query(base_url, split_endpoint_and_query(endpoint).1)
        }
        _ => join(base_url, endpoint),
    }
}

fn append_query(base_url: &str, query: Option<&str>) -> String {
    match query {
        Some(query) if !query.is_empty() => {
            if base_url.contains('?') {
                format!("{base_url}&{query}")
            } else {
                format!("{base_url}?{query}")
            }
        }
        _ => base_url.to_string(),
    }
}

// ---------------------------------------------------------------- 请求

/// 改请求要知道的这条线的几件事。
#[derive(Debug, Clone, Copy)]
pub struct LineInfo<'a> {
    pub route_id: &'a str,
    pub base_url: &'a str,
    /// 客户端自己带来的会话 id。**生成的那种不算** —— 每发都不一样，拿去当缓存键会把上游的前缀缓存打碎。
    pub client_session_id: Option<&'a str>,
    /// 会话 id（客户端没带就是生成的）。Gemini 的影子记录按它分会话。
    pub session_id: &'a str,
}

/// 改好的请求。
#[derive(Debug, Clone)]
pub struct Prepared {
    pub body: Value,
    /// 打这条线的路径（带查询串）。
    pub endpoint: String,
    /// Codex → Anthropic 时模型名带着 `[1m]`：剥掉之后要加 `anthropic-beta: context-1m-2025-08-07`。
    pub anthropic_one_m: bool,
}

/// Codex → Anthropic 时请求没写 `max_output_tokens` 用的上限（cc-switch 的 `DEFAULT_CODEX_ANTHROPIC_MAX_TOKENS`）：
/// 8192 是所有现行 Claude 模型和几乎所有网关都收的数，写高了在低上限的网关上是不可重试的 400。
pub const DEFAULT_CODEX_ANTHROPIC_MAX_TOKENS: u64 = 8192;

/// 把请求按这一种转换改好。`client_model` 是客户端原来写的模型名（模型映射之前的）。
///
/// Codex → Chat 之前，调用方要先用 [`super::codex_chat_history::CodexChatHistoryStore::enrich_request`]
/// 把上一回合的函数调用补回去（那一步是异步的，不放在这里）。
pub fn prepare_request(
    conversion: Conversion,
    endpoint: &str,
    body: Value,
    client_model: Option<&str>,
    line: &LineInfo<'_>,
    gemini_shadow: Option<&GeminiShadowStore>,
) -> Result<Prepared, ProxyError> {
    match conversion {
        Conversion::Passthrough | Conversion::ClaudeTo(ApiFormat::Anthropic) => Ok(Prepared {
            body,
            endpoint: endpoint.to_string(),
            anthropic_one_m: false,
        }),
        Conversion::ClaudeTo(format) => {
            let mut body = crate::model_mapper::strip_one_m_suffix_for_upstream_from_body(body);
            // 映射后的模型名不再带「省略 thinking 仍默认开思考」的语义（5.x 的 Fable / Mythos / Opus / Sonnet），
            // 按客户端原模型补成显式 adaptive，effort 换算才不会当成关闭。
            if matches!(format, ApiFormat::OpenaiChat | ApiFormat::OpenaiResponses) {
                transform::make_default_adaptive_thinking_explicit(&mut body, client_model);
            }
            let rewritten = rewrite_claude_transform_endpoint(endpoint, format, &body);
            let body = transform_claude_request_for_api_format(body, line, format, gemini_shadow)?;
            Ok(Prepared {
                body,
                endpoint: rewritten,
                anthropic_one_m: false,
            })
        }
        Conversion::ResponsesToChat => {
            let body = crate::model_mapper::strip_one_m_suffix_for_upstream_from_body(body);
            let explicit_prompt_cache_key = body
                .get("prompt_cache_key")
                .and_then(|value| value.as_str())
                .map(ToString::to_string);
            let reasoning = infer_codex_chat_reasoning_config(line.base_url, &body);
            let mut chat_body = transform_codex_chat::responses_to_chat_completions_with_reasoning(
                body,
                reasoning.as_ref(),
            )?;
            let fallbacks = fill_gemini_thought_signature_fallback(&mut chat_body);
            if fallbacks > 0 {
                tracing::debug!(
                    "[Codex] Filled {fallbacks} missing Gemini thought signature(s) with the skip placeholder"
                );
            }
            inject_codex_chat_prompt_cache_key(
                line.base_url,
                &mut chat_body,
                explicit_prompt_cache_key.as_deref(),
                line.client_session_id,
            );
            // Moonshot / Kimi Chat Completions reject `$ref` nodes that carry sibling keywords.
            if transform_codex_chat_moonshot_schema::upstream_requires_ref_sibling_all_of(
                line.base_url,
            ) {
                transform_codex_chat_moonshot_schema::wrap_ref_siblings_in_chat_tools(
                    &mut chat_body,
                );
            }
            Ok(Prepared {
                body: chat_body,
                endpoint: rewrite_codex_responses_endpoint_to_chat(endpoint),
                anthropic_one_m: false,
            })
        }
        Conversion::ResponsesToAnthropic => {
            let mut anthropic_body = transform_codex_anthropic::responses_request_to_anthropic(
                body,
                DEFAULT_CODEX_ANTHROPIC_MAX_TOKENS,
            )?;
            // `[1m]`：剥掉模型名后缀（网关不认），另加 context-1m 的 beta 头。
            let mut one_m = false;
            if let Some(model) = anthropic_body.get("model").and_then(|v| v.as_str()) {
                let stripped = strip_one_m_suffix_for_upstream(model);
                if stripped != model {
                    one_m = true;
                    anthropic_body["model"] = Value::String(stripped.to_string());
                }
            }
            // Anthropic 的提示缓存（不用 beta 头）：不打的话 system / tools / 历史每一轮都按全价重发。
            // cc-switch 这里沿用它的优化器配置里的缓存开关；QB Gate 没有那个界面，取它的默认（开）。
            crate::cache_injector::inject(
                &mut anthropic_body,
                &OptimizerConfig {
                    enabled: true,
                    thinking_optimizer: false,
                    cache_injection: true,
                },
            );
            Ok(Prepared {
                body: anthropic_body,
                endpoint: rewrite_codex_responses_endpoint_to_anthropic(endpoint),
                anthropic_one_m: one_m,
            })
        }
    }
}

// ---- 照搬 cc-switch providers/claude.rs（把 `&Provider` 换成这条线的地址）

const ANTHROPIC_THINKING_PLACEHOLDER: &str = "tool call";
const ANTHROPIC_REDACTED_THINKING_PLACEHOLDER: &str = "[redacted thinking]";
// Keep hints lowercase; matching lowercases only the input value.
// Moonshot/Kimi exited on vendor request (2026-08): their endpoints no longer
// require thinking replay on tool-call turns, and injected placeholders
// disrupt the model's chain of thought. Do not re-add without re-confirming.
const REASONING_VENDOR_HINTS: &[&str] = &["deepseek", "mimo", "xiaomimimo"];

fn is_reasoning_vendor_identifier(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    REASONING_VENDOR_HINTS
        .iter()
        .any(|hint| value.contains(hint))
}

fn should_normalize_anthropic_tool_thinking_history(base_url: &str, body: &Value) -> bool {
    body.get("model")
        .and_then(|m| m.as_str())
        .is_some_and(is_reasoning_vendor_identifier)
        || is_reasoning_vendor_identifier(base_url)
}

/// DeepSeek official Anthropic-compatible endpoint URL
const DEEPSEEK_OFFICIAL_ANTHROPIC_URL: &str = "https://api.deepseek.com/anthropic";

/// DeepSeek's official Anthropic-compatible endpoint treats
/// `thinking: { type: "disabled" }` and effort parameters (`output_config.effort`
/// or `reasoning_effort`) as mutually exclusive, returning HTTP 400.
/// Respect `thinking: disabled` and remove the conflicting effort parameters instead.
///
/// <https://github.com/deepseek-ai/DeepSeek-V3/issues/1397>
pub fn normalize_deepseek_thinking_disabled_strip_effort(body: &mut Value, base_url: &str) -> bool {
    if base_url.trim_end_matches('/') != DEEPSEEK_OFFICIAL_ANTHROPIC_URL {
        return false;
    }
    if body
        .get("thinking")
        .and_then(|t| t.get("type"))
        .and_then(|t| t.as_str())
        != Some("disabled")
    {
        return false;
    }
    let mut changed = false;
    if let Some(oc) = body
        .get_mut("output_config")
        .and_then(|v| v.as_object_mut())
    {
        changed |= oc.remove("effort").is_some();
        if oc.is_empty() {
            if let Some(obj) = body.as_object_mut() {
                obj.remove("output_config");
            }
        }
    }
    if body.get("reasoning_effort").is_some() {
        if let Some(obj) = body.as_object_mut() {
            obj.remove("reasoning_effort");
        }
        changed = true;
    }
    changed
}

/// Claude 原样转发给 Anthropic 协议的上游之前（cc-switch 的 `normalize_anthropic_messages_for_provider`）：
/// DeepSeek / MiMo 要求带工具调用的每一轮都回放 thinking —— 有的 SDK 留着工具历史却丢了 thinking，下一发就被拒。
/// 只改这一种很窄的历史形状。返回是否改了。
pub fn normalize_anthropic_messages_for_upstream(body: &mut Value, base_url: &str) -> bool {
    let mut changed = false;
    if should_normalize_anthropic_tool_thinking_history(base_url, body) {
        changed |= normalize_anthropic_tool_thinking_history(body);
    }
    changed |= normalize_deepseek_thinking_disabled_strip_effort(body, base_url);
    changed
}

fn normalize_anthropic_tool_thinking_history(body: &mut Value) -> bool {
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return false;
    };

    let mut changed = false;
    for message in messages {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        if !content
            .iter()
            .any(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
        {
            continue;
        }

        let mut has_thinking = false;
        for block in content.iter_mut() {
            match block.get("type").and_then(Value::as_str) {
                Some("thinking") => {
                    let has_non_empty_thinking = block
                        .get("thinking")
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.trim().is_empty());
                    if let Some(obj) = block.as_object_mut() {
                        if obj.remove("signature").is_some() {
                            changed = true;
                        }
                        if !has_non_empty_thinking {
                            obj.insert(
                                "thinking".to_string(),
                                json!(ANTHROPIC_THINKING_PLACEHOLDER),
                            );
                            changed = true;
                        }
                    }
                    has_thinking = true;
                }
                Some("redacted_thinking") => {
                    *block = json!({
                        "type": "thinking",
                        "thinking": ANTHROPIC_REDACTED_THINKING_PLACEHOLDER
                    });
                    has_thinking = true;
                    changed = true;
                }
                _ => {}
            }
        }

        if !has_thinking {
            content.insert(
                0,
                json!({
                    "type": "thinking",
                    "thinking": ANTHROPIC_THINKING_PLACEHOLDER
                }),
            );
            changed = true;
        }
    }

    changed
}

fn should_preserve_reasoning_content_for_openai_chat(base_url: &str, body: &Value) -> bool {
    body.get("model")
        .and_then(|m| m.as_str())
        .is_some_and(is_reasoning_vendor_identifier)
        || is_reasoning_vendor_identifier(base_url)
}

/// cc-switch 的 `transform_claude_request_for_api_format`（去掉 Copilot / Codex OAuth / xAI OAuth 三支，
/// 显式 `prompt_cache_key` 没有界面，只剩「按会话」那一档）。
fn transform_claude_request_for_api_format(
    body: Value,
    line: &LineInfo<'_>,
    api_format: ApiFormat,
    shadow_store: Option<&GeminiShadowStore>,
) -> Result<Value, ProxyError> {
    let session_cache_key = line
        .client_session_id
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match api_format {
        ApiFormat::OpenaiResponses => {
            tracing::debug!(
                "[Cache] OpenAI Responses prompt_cache_key source={}, route={}",
                if session_cache_key.is_some() {
                    "session"
                } else {
                    "none"
                },
                line.route_id
            );
            transform_responses::anthropic_to_responses(body, session_cache_key, false, false)
        }
        ApiFormat::OpenaiChat => {
            let preserve = should_preserve_reasoning_content_for_openai_chat(line.base_url, &body);
            let mut result = transform::anthropic_to_openai_with_reasoning_content(body, preserve)?;
            // 流式请求必须注入 stream_options.include_usage，否则 OpenAI 兼容上游不在 SSE 末尾吐 usage →
            // 转换出的 Anthropic message_delta 全 0 → 整笔 token 漏记。
            transform::inject_openai_stream_include_usage(&mut result);
            Ok(result)
        }
        ApiFormat::GeminiNative => transform_gemini::anthropic_to_gemini_with_shadow(
            body,
            shadow_store,
            Some(line.route_id),
            Some(line.session_id),
        ),
        ApiFormat::Anthropic => Ok(body),
    }
}

// ---- 照搬 cc-switch providers/codex.rs（把 `&Provider` 换成这条线的地址）

/// Whether a converted Codex Responses request may send `prompt_cache_key` to
/// its Chat Completions upstream. Unknown OpenAI-compatible gateways default to
/// false because many reject unsupported request fields with HTTP 400.
/// （cc-switch 还看 `meta.prompt_cache_routing` 的开关；QB Gate 没有那个设置，只剩「auto」这一档。）
pub fn should_send_codex_chat_prompt_cache_key(base_url: &str) -> bool {
    let Ok(url) = url::Url::parse(base_url) else {
        return false;
    };
    match url.host_str() {
        Some("api.openai.com") => true,
        Some("api.kimi.com") => {
            let path = url.path().trim_end_matches('/');
            path == "/coding" || path.starts_with("/coding/")
        }
        _ => false,
    }
}

/// Add a stable cache-routing key after Responses -> Chat conversion. An
/// explicit client key wins; otherwise only a real client-provided session ID
/// is eligible. Generated per-request UUIDs must never be used here.
pub fn inject_codex_chat_prompt_cache_key(
    base_url: &str,
    chat_body: &mut Value,
    explicit_key: Option<&str>,
    client_session_id: Option<&str>,
) -> bool {
    if !should_send_codex_chat_prompt_cache_key(base_url) {
        return false;
    }
    let key = explicit_key
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .or_else(|| {
            client_session_id
                .map(str::trim)
                .filter(|session_id| !session_id.is_empty())
        });
    let Some(key) = key else {
        return false;
    };
    chat_body["prompt_cache_key"] = Value::String(key.to_string());
    true
}

fn reasoning(
    supports_thinking: bool,
    supports_effort: bool,
    thinking_param: &str,
    effort_param: &str,
    effort_value_mode: Option<&str>,
    output_format: &str,
) -> CodexChatReasoningConfig {
    CodexChatReasoningConfig {
        supports_thinking: Some(supports_thinking),
        supports_effort: Some(supports_effort),
        thinking_param: Some(thinking_param.to_string()),
        effort_param: Some(effort_param.to_string()),
        effort_value_mode: effort_value_mode.map(str::to_string),
        output_format: Some(output_format.to_string()),
        effort_levels: None,
    }
}

/// Codex 转 Chat 时 reasoning 参数怎么发（cc-switch 的 `infer_codex_chat_reasoning_config`）。
///
/// cc-switch 的判断材料是「供应商名 + 地址 + 模型名」；QB Gate 的线路名是站点名（使用者自己起的），
/// 不拿来猜，只看**地址 + 模型名**。平台优先（聚合 / 托管平台的接口由平台决定，只看地址）。
pub fn infer_codex_chat_reasoning_config(
    base_url: &str,
    body: &Value,
) -> Option<CodexChatReasoningConfig> {
    let model = body
        .get("model")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let base_url = base_url.to_ascii_lowercase();

    if let Some(config) = infer_aggregator_platform_config(&base_url) {
        return Some(config);
    }

    let haystack = format!("{base_url} {model}");

    if haystack.contains("deepseek") {
        return Some(reasoning(
            true,
            true,
            "thinking",
            "reasoning_effort",
            Some("deepseek"),
            "reasoning_content",
        ));
    }

    // StepFun：step-3.5-flash-2603 支持 low/high；step-3.7-flash 支持 low/medium/high（必须 passthrough，
    // 套 low_high 会把 medium 塌成 high）；其余 step 模型不暴露 effort。全系无思考开关。
    if haystack.contains("stepfun") || haystack.contains("step-3.5-flash-2603") {
        return Some(reasoning(
            true,
            model.contains("2603") || model.contains("step-3.7-flash"),
            "none",
            "reasoning_effort",
            Some(if model.contains("2603") {
                "low_high"
            } else {
                "passthrough"
            }),
            "reasoning",
        ));
    }

    if haystack.contains("kimi") || haystack.contains("moonshot") {
        return Some(reasoning(
            true,
            false,
            "thinking",
            "none",
            None,
            "reasoning_content",
        ));
    }

    if haystack.contains("glm") || haystack.contains("zhipu") || haystack.contains("z.ai") {
        return Some(reasoning(
            true,
            false,
            "thinking",
            "none",
            None,
            "reasoning_content",
        ));
    }

    if haystack.contains("qwen") || haystack.contains("dashscope") || haystack.contains("bailian") {
        return Some(reasoning(
            true,
            false,
            "enable_thinking",
            "none",
            None,
            "reasoning_content",
        ));
    }

    if haystack.contains("minimax") {
        return Some(reasoning(
            true,
            false,
            "reasoning_split",
            "none",
            None,
            "reasoning_details",
        ));
    }

    if haystack.contains("mimo") {
        return Some(reasoning(
            true,
            false,
            "thinking",
            "none",
            None,
            "reasoning_content",
        ));
    }

    None
}

/// 聚合 / 托管平台的 reasoning 接口由平台决定（cc-switch 的 `infer_aggregator_platform_config`，只看地址）。
fn infer_aggregator_platform_config(base_url: &str) -> Option<CodexChatReasoningConfig> {
    // OpenRouter：原生归一化对象 `reasoning: { effort }`，effort 无 max（钳到 xhigh），不发 thinking。
    if base_url.contains("openrouter") {
        return Some(reasoning(
            false,
            true,
            "none",
            "reasoning.effort",
            Some("openrouter"),
            "auto",
        ));
    }
    // SiliconFlow / ModelScope：平台级 `enable_thinking`，不按 reasoning_effort 发 effort。
    if base_url.contains("siliconflow") || base_url.contains("modelscope") {
        return Some(reasoning(
            true,
            false,
            "enable_thinking",
            "none",
            None,
            "reasoning_content",
        ));
    }
    // OpenCode Zen：顶层 `reasoning_effort`，合法档位逐模型（cc-switch 从供应商的模型目录查；QB Gate 没有那张表，
    // `effort_levels` 留空 = 不发 effort 字段，跟 cc-switch「查不到」时一样）。
    if base_url.contains("opencode.ai") {
        return Some(reasoning(
            true,
            true,
            "none",
            "reasoning_effort",
            Some("zen"),
            "reasoning_content",
        ));
    }
    None
}

// ---------------------------------------------------------------- 响应

/// 把响应换回客户端那一侧要的东西（都从客户端原来那一发请求里取）。
#[derive(Debug, Clone)]
pub struct ResponseContext {
    pub conversion: Conversion,
    pub route_id: String,
    pub session_id: String,
    tool_schema_hints: Option<AnthropicToolSchemaHints>,
    hosted_web_search_name: Option<String>,
    hosted_web_search_max_uses: Option<u64>,
    codex_tool_context: CodexToolContext,
}

impl ResponseContext {
    /// `original` 是客户端发来的那一份请求体（模型映射、转换之前的）。
    pub fn new(conversion: Conversion, original: &Value, route_id: &str, session_id: &str) -> Self {
        let claude = matches!(conversion, Conversion::ClaudeTo(_));
        let codex = matches!(
            conversion,
            Conversion::ResponsesToChat | Conversion::ResponsesToAnthropic
        );
        let hints = claude
            .then(|| transform_gemini::extract_anthropic_tool_schema_hints(original))
            .filter(|h| !h.is_empty());
        Self {
            conversion,
            route_id: route_id.to_string(),
            session_id: session_id.to_string(),
            tool_schema_hints: hints,
            hosted_web_search_name: claude
                .then(|| transform_responses::anthropic_web_search_tool_name(original))
                .flatten()
                .map(ToString::to_string),
            hosted_web_search_max_uses: claude
                .then(|| transform_responses::anthropic_web_search_max_uses(original))
                .flatten(),
            codex_tool_context: if codex {
                build_codex_tool_context_from_request(original)
            } else {
                CodexToolContext::default()
            },
        }
    }

    /// 上游回成功之后要不要走流式转换器（cc-switch 各个 handler 里的判断）。
    ///
    /// - Claude 那三种：客户端要流，或者上游回的就是 SSE（`should_use_claude_transform_streaming`）；
    /// - Codex → Chat：同上；
    /// - Codex → Anthropic：上游标了 SSE，或者客户端要流而上游没标 JSON；标了 JSON 的整份读完再转
    ///   （2xx 里的错误信封、不理 `stream:true` 的网关都认得出来），客户端要流的话再拆成事件。
    pub fn stream_upstream(
        &self,
        requested_streaming: bool,
        upstream_is_sse: bool,
        upstream_is_json: bool,
    ) -> bool {
        match self.conversion {
            Conversion::ResponsesToAnthropic => {
                upstream_is_sse || (requested_streaming && !upstream_is_json)
            }
            _ => requested_streaming || upstream_is_sse,
        }
    }
}

/// 上游整份回来的 2xx 响应换成客户端那一侧的 JSON（[`parse_whole`] + [`convert_parsed`]）。
pub fn convert_whole(
    ctx: &ResponseContext,
    headers: &http::HeaderMap,
    body: &[u8],
    gemini_shadow: Option<&GeminiShadowStore>,
) -> Result<Value, ProxyError> {
    let parsed = parse_whole(ctx, headers, body)?;
    convert_parsed(ctx, parsed, gemini_shadow)
}

/// 上游整份回来的 2xx 响应解析成上游那一侧的 JSON。上游对非流请求回了没标类型的 SSE 也认（聚合成一份，
/// cc-switch #2234 的兜底）；解不出来的报错里只有类别与长度，不带正文。
pub fn parse_whole(
    ctx: &ResponseContext,
    headers: &http::HeaderMap,
    body: &[u8],
) -> Result<Value, ProxyError> {
    let text = String::from_utf8_lossy(body);
    let parsed: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_)
            if sse_aggregate::body_looks_like_sse(&text)
                && ctx.conversion != Conversion::ClaudeTo(ApiFormat::GeminiNative) =>
        {
            tracing::warn!(
                "[{}] 上游对非流请求返回未标记的 SSE 体，按 SSE 聚合兜底",
                ctx.conversion.label()
            );
            let aggregated = match ctx.conversion {
                Conversion::ClaudeTo(ApiFormat::OpenaiResponses) => {
                    sse_aggregate::responses_sse_to_response_value(&text)
                }
                Conversion::ResponsesToAnthropic => {
                    transform_codex_anthropic::anthropic_sse_to_message_value(&text)
                }
                _ => sse_aggregate::chat_sse_to_response_value(&text),
            };
            aggregated.map_err(|e| sse_aggregate::aggregate_fallback_error(e, headers, &text))?
        }
        Err(e) => {
            return Err(sse_aggregate::upstream_body_parse_error(
                "Failed to parse upstream response",
                &e,
                headers,
                &text,
            ))
        }
    };
    Ok(parsed)
}

/// 解析好的上游响应换成客户端那一侧的 JSON。
pub fn convert_parsed(
    ctx: &ResponseContext,
    parsed: Value,
    gemini_shadow: Option<&GeminiShadowStore>,
) -> Result<Value, ProxyError> {
    match ctx.conversion {
        Conversion::Passthrough | Conversion::ClaudeTo(ApiFormat::Anthropic) => Ok(parsed),
        Conversion::ClaudeTo(ApiFormat::OpenaiChat) => transform::openai_to_anthropic(parsed),
        Conversion::ClaudeTo(ApiFormat::OpenaiResponses) => {
            transform_responses::responses_to_anthropic_with_web_search_options(
                parsed,
                ctx.hosted_web_search_name.as_deref(),
                ctx.hosted_web_search_max_uses,
            )
        }
        Conversion::ClaudeTo(ApiFormat::GeminiNative) => {
            transform_gemini::gemini_to_anthropic_with_shadow_and_hints(
                parsed,
                gemini_shadow,
                Some(&ctx.route_id),
                Some(&ctx.session_id),
                ctx.tool_schema_hints.as_ref(),
            )
        }
        Conversion::ResponsesToChat => {
            transform_codex_chat::chat_completion_to_response_with_context(
                parsed,
                &ctx.codex_tool_context,
            )
        }
        Conversion::ResponsesToAnthropic => {
            transform_codex_anthropic::anthropic_response_to_responses_with_context(
                parsed,
                &ctx.codex_tool_context,
            )
        }
    }
}

/// 转好的流（客户端那一侧的 SSE）。
pub type ConvertedStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>;

/// 上游的流换成客户端那一侧的 SSE。原样转发的情况调用方自己处理，这里只管转换。
pub fn convert_stream<E>(
    ctx: &ResponseContext,
    upstream: impl Stream<Item = Result<Bytes, E>> + Send + 'static,
    gemini_shadow: Option<Arc<GeminiShadowStore>>,
) -> ConvertedStream
where
    E: std::error::Error + Send + 'static,
{
    match ctx.conversion {
        Conversion::Passthrough | Conversion::ClaudeTo(ApiFormat::Anthropic) => {
            Box::pin(upstream.map(|chunk| chunk.map_err(|e| std::io::Error::other(e.to_string()))))
        }
        Conversion::ClaudeTo(ApiFormat::OpenaiChat) => {
            Box::pin(streaming::create_anthropic_sse_stream(upstream))
        }
        Conversion::ClaudeTo(ApiFormat::OpenaiResponses) => Box::pin(
            streaming_responses::create_anthropic_sse_stream_from_responses_with_web_search_options(
                upstream,
                ctx.hosted_web_search_name.clone(),
                ctx.hosted_web_search_max_uses,
            ),
        ),
        Conversion::ClaudeTo(ApiFormat::GeminiNative) => {
            Box::pin(streaming_gemini::create_anthropic_sse_stream_from_gemini(
                upstream,
                gemini_shadow,
                Some(ctx.route_id.clone()),
                Some(ctx.session_id.clone()),
                ctx.tool_schema_hints.clone(),
            ))
        }
        Conversion::ResponsesToChat => Box::pin(
            streaming_codex_chat::create_responses_sse_stream_from_chat_with_context(
                upstream,
                ctx.codex_tool_context.clone(),
            ),
        ),
        Conversion::ResponsesToAnthropic => Box::pin(
            streaming_codex_anthropic::create_responses_sse_stream_from_anthropic_with_context(
                upstream,
                ctx.codex_tool_context.clone(),
            ),
        ),
    }
}

/// Codex → Anthropic：客户端要流、上游却整份回了 JSON 时，把那份消息拆成 Responses 的事件。
pub fn anthropic_message_as_responses_events(ctx: &ResponseContext, message: &Value) -> Vec<Bytes> {
    streaming_codex_anthropic::responses_sse_events_from_anthropic_message(
        message,
        ctx.codex_tool_context.clone(),
    )
}

/// 上游回了错（非 2xx）：错误体换成客户端那一侧认得的形状。原样转发、或者体是空的，回 `None`（原样转回）。
///
/// - Codex 那两种：cc-switch 的 `chat_error_to_response_error`（Chat 与 Anthropic 的错误体都认，
///   MiniMax 的 `base_resp`、纯文本都兜得住）→ Responses 的 `{"error":{message,type,code,param}}`；
/// - Claude 那三种：Anthropic 的 `{"type":"error","error":{type,message}}`。cc-switch 这里是原样透传
///   上游的 JSON；QB Gate 换成 Claude 认得的形状，客户端里看得到的就是那句话而不是一团别家的 JSON。
pub fn client_error_body(conversion: Conversion, body: &str) -> Option<Value> {
    if conversion.is_passthrough() || conversion == Conversion::ClaudeTo(ApiFormat::Anthropic) {
        return None;
    }
    let parsed: Value = serde_json::from_str(body).unwrap_or_else(|_| {
        const MAX_RAW_ERROR_CHARS: usize = 1024;
        let text: String = body.chars().take(MAX_RAW_ERROR_CHARS).collect();
        Value::String(text)
    });
    match conversion {
        Conversion::ResponsesToChat | Conversion::ResponsesToAnthropic => Some(
            transform_codex_chat::chat_error_to_response_error(Some(&parsed)),
        ),
        _ => {
            let error = parsed.get("error").unwrap_or(&parsed);
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
                .or_else(|| parsed.get("message").and_then(Value::as_str))
                .map(ToString::to_string)
                .unwrap_or_else(|| parsed.to_string());
            let kind = error
                .get("type")
                .and_then(Value::as_str)
                .or_else(|| error.get("status").and_then(Value::as_str))
                .unwrap_or("api_error");
            Some(json!({
                "type": "error",
                "error": { "type": kind, "message": message }
            }))
        }
    }
}

// ---------------------------------------------------------------- Codex 原样转发给 Responses 上游

/// Codex 原样转发给 Responses 上游时，响应那一侧要做的修补（cc-switch 只在原生透传分支里做）。
#[derive(Debug, Clone, Default)]
pub enum NativeRepair {
    #[default]
    None,
    /// 迟到的函数调用参数（`responses_late_arguments`）：官方以外的上游都补 —— 触发条件是协议违规本身
    /// （结束事件参数为空、增量排在后面），顺序正常的流原样放行。QB Gate 的中转线路都不是官方。
    LateArguments,
    /// 直连 xAI：还原请求里压平的命名空间函数名、把整数参数改回整数。
    Xai(HashMap<String, NamespacedName>),
}

/// 是不是直连 xAI 的 Responses（cc-switch 按地址认 `api.x.ai`）。
pub fn is_xai_native_responses_url(base_url: &str) -> bool {
    base_url.to_ascii_lowercase().contains("api.x.ai")
}

/// Codex 原样转发给 Responses 上游之前（只对 Responses 端点调用）：直连 xAI 的线压平命名空间工具、做 xAI 的
/// 字段兼容（Codex 0.142+ 发的 `{"type":"namespace"}` 工具 xAI 回 422）；别的线请求一个字节都不改。
/// 回的是响应那一侧要做的修补。
pub fn prepare_codex_native(
    body: &mut Value,
    base_url: &str,
    route_id: &str,
) -> Result<NativeRepair, ProxyError> {
    if !is_xai_native_responses_url(base_url) {
        return Ok(NativeRepair::LateArguments);
    }
    // 还原表要在压平之前从原请求里取。
    let restore = namespace_restore_map(body);
    if flatten_request_namespaces(body)? {
        tracing::debug!(
            "[Codex] Flattened namespace tools for native Responses upstream (route={route_id})"
        );
    }
    // 模型映射 QB Gate 已经在线路上做过了，这里不再按 cc-switch 的模型目录改模型名（`upstream_model` 给 `None`）。
    apply_xai_native_responses_request_compat(body, route_id, None, &Value::Null);
    Ok(NativeRepair::Xai(restore))
}

/// 原生透传的 SSE 流按 [`NativeRepair`] 修一遍。
pub fn repair_native_stream<E>(
    repair: &NativeRepair,
    upstream: impl Stream<Item = Result<Bytes, E>> + Send + 'static,
) -> ConvertedStream
where
    E: std::error::Error + Send + 'static,
{
    match repair {
        NativeRepair::None => {
            Box::pin(upstream.map(|chunk| chunk.map_err(|e| std::io::Error::other(e.to_string()))))
        }
        NativeRepair::LateArguments => {
            Box::pin(super::responses_late_arguments::create_late_arguments_repair_stream(upstream))
        }
        NativeRepair::Xai(restore) => Box::pin(
            super::transform_codex_responses_xai_sanitize::create_xai_native_responses_sse_stream(
                upstream,
                restore.clone(),
            ),
        ),
    }
}

/// 原生透传的整份响应按 [`NativeRepair`] 修一遍（只有 xAI 那一种要改整份）。不用改、或者解不出 JSON 回 `None`（原样转回）。
pub fn repair_native_whole(repair: &NativeRepair, body: &[u8]) -> Option<Bytes> {
    let NativeRepair::Xai(restore) = repair else {
        return None;
    };
    let mut value: Value = serde_json::from_slice(body).ok()?;
    restore_response_namespaces(&mut value, restore);
    super::transform_codex_responses_xai_sanitize::normalize_xai_function_call_integer_arguments(
        &mut value,
    );
    serde_json::to_vec(&value).ok().map(Bytes::from)
}

/// Codex → Anthropic 时不许带过去的头：Codex / OpenAI SDK 自己的会话与身份头（cc-switch 的
/// `is_codex_client_fingerprint_header`）。`name` 要小写（`http` crate 的头名本来就是小写）。
pub fn is_codex_client_fingerprint_header(name: &str) -> bool {
    matches!(
        name,
        "originator"
            | "session_id"
            | "session-id"
            | "thread-id"
            | "conversation_id"
            | "chatgpt-account-id"
            | "x-client-request-id"
            | "openai-beta"
            | "openai-organization"
            | "openai-project"
            | "version"
    ) || name.starts_with("x-stainless-")
        || name.starts_with("x-codex-")
        || name.starts_with("x-oai-")
        || name.starts_with("x-openai-")
        || name.starts_with("x-responsesapi-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line<'a>(base_url: &'a str, client_session_id: Option<&'a str>) -> LineInfo<'a> {
        LineInfo {
            route_id: "r",
            base_url,
            client_session_id,
            session_id: client_session_id.unwrap_or("generated"),
        }
    }

    #[test]
    fn the_conversion_follows_the_client_the_path_and_the_line_protocol() {
        use ApiFormat::*;
        assert_eq!(
            select(true, "/v1/messages", None),
            Ok(Conversion::Passthrough)
        );
        assert_eq!(
            select(true, "/v1/messages?beta=true", Some(Anthropic)),
            Ok(Conversion::Passthrough)
        );
        assert_eq!(
            select(true, "/v1/messages", Some(OpenaiChat)),
            Ok(Conversion::ClaudeTo(OpenaiChat))
        );
        assert_eq!(
            select(true, "/v1/messages", Some(GeminiNative)),
            Ok(Conversion::ClaudeTo(GeminiNative))
        );
        // 不归转换管的路径原样转发（上游不认就 404，路由换下一条）。
        assert_eq!(
            select(true, "/v1/messages/count_tokens", Some(OpenaiChat)),
            Ok(Conversion::Passthrough)
        );
        assert_eq!(
            select(false, "/v1/responses", Some(OpenaiChat)),
            Ok(Conversion::ResponsesToChat)
        );
        assert_eq!(
            select(false, "/responses/compact", Some(Anthropic)),
            Ok(Conversion::ResponsesToAnthropic)
        );
        assert_eq!(
            select(false, "/v1/responses", Some(OpenaiResponses)),
            Ok(Conversion::Passthrough)
        );
        assert_eq!(
            select(false, "/v1/models", Some(OpenaiChat)),
            Ok(Conversion::Passthrough)
        );
        assert!(select(false, "/v1/responses", Some(GeminiNative)).is_err());
        assert_eq!(
            Conversion::ClaudeTo(OpenaiResponses).upstream(true),
            OpenaiResponses
        );
        assert_eq!(Conversion::Passthrough.upstream(false), OpenaiResponses);
    }

    // ---- cc-switch forwarder.rs 的地址测试（Copilot 那几个不搬；目标路径改成带 /v1 的）

    #[test]
    fn rewrite_claude_transform_endpoint_strips_beta_for_chat_completions() {
        let endpoint = rewrite_claude_transform_endpoint(
            "/v1/messages?beta=true&foo=bar",
            ApiFormat::OpenaiChat,
            &json!({}),
        );
        assert_eq!(endpoint, "/v1/chat/completions?foo=bar");
    }

    #[test]
    fn rewrite_claude_transform_endpoint_strips_beta_for_responses() {
        let endpoint = rewrite_claude_transform_endpoint(
            "/v1/messages?beta=true",
            ApiFormat::OpenaiResponses,
            &json!({}),
        );
        assert_eq!(endpoint, "/v1/responses");
    }

    #[test]
    fn rewrite_claude_transform_endpoint_maps_gemini_generate_content_and_streaming() {
        let plain = rewrite_claude_transform_endpoint(
            "/v1/messages?beta=true",
            ApiFormat::GeminiNative,
            &json!({"model": "gemini-2.5-pro"}),
        );
        assert_eq!(plain, "/v1beta/models/gemini-2.5-pro:generateContent");
        let stream = rewrite_claude_transform_endpoint(
            "/v1/messages",
            ApiFormat::GeminiNative,
            &json!({"model": "models/gemini-2.5-flash", "stream": true}),
        );
        assert_eq!(
            stream,
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn codex_endpoints_are_rewritten_with_their_query() {
        assert_eq!(
            rewrite_codex_responses_endpoint_to_chat("/responses?x=1"),
            "/v1/chat/completions?x=1"
        );
        assert_eq!(
            rewrite_codex_responses_endpoint_to_anthropic("/v1/responses"),
            "/v1/messages"
        );
    }

    #[test]
    fn a_base_url_pasted_as_the_full_endpoint_is_not_appended_twice() {
        let join = |b: &str, p: &str| format!("{}{p}", b.trim_end_matches('/'));
        assert_eq!(
            upstream_url(
                Conversion::ResponsesToChat,
                "https://x.test/api/chat/completions",
                "/v1/chat/completions?k=1",
                join
            ),
            "https://x.test/api/chat/completions?k=1"
        );
        assert_eq!(
            upstream_url(
                Conversion::ResponsesToAnthropic,
                "https://x.test/v1/messages/",
                "/v1/messages",
                join
            ),
            "https://x.test/v1/messages/"
        );
        assert_eq!(
            upstream_url(
                Conversion::ClaudeTo(ApiFormat::OpenaiChat),
                "https://x.test",
                "/v1/chat/completions",
                join
            ),
            "https://x.test/v1/chat/completions"
        );
        assert!(upstream_url(
            Conversion::ClaudeTo(ApiFormat::GeminiNative),
            "https://generativelanguage.googleapis.com",
            "/v1beta/models/g:generateContent",
            join
        )
        .contains(":generateContent"));
    }

    // ---- cc-switch providers/claude.rs 的测试（`create_provider` 换成地址）

    #[test]
    fn deepseek_tool_history_rewrites_redacted_thinking_and_drops_signatures() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "redacted_thinking", "data": "opaque"},
                    {"type": "tool_use", "id": "call_123", "name": "read_file", "input": {"path": "README.md"}}
                ]
            }]
        });
        assert!(normalize_anthropic_messages_for_upstream(
            &mut body,
            "https://api.deepseek.com/anthropic"
        ));
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(
            content[0]["thinking"],
            ANTHROPIC_REDACTED_THINKING_PLACEHOLDER
        );

        let mut kept = json!({
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "Need to inspect the file.", "signature": "anthropic-signature"},
                    {"type": "tool_use", "id": "call_123", "name": "read_file", "input": {}}
                ]
            }]
        });
        assert!(normalize_anthropic_messages_for_upstream(
            &mut kept,
            "https://relay.test/anthropic"
        ));
        assert_eq!(
            kept["messages"][0]["content"][0]["thinking"],
            "Need to inspect the file."
        );
        assert!(kept["messages"][0]["content"][0].get("signature").is_none());
    }

    #[test]
    fn kimi_and_generic_tool_history_are_not_modified() {
        for (base, model) in [
            ("https://api.kimi.com/coding", "kimi-for-coding"),
            ("https://api.example.com/anthropic", "claude-sonnet-4.6"),
        ] {
            let mut body = json!({
                "model": model,
                "messages": [{
                    "role": "assistant",
                    "content": [{"type": "tool_use", "id": "c", "name": "read_file", "input": {}}]
                }]
            });
            let original = body.clone();
            assert!(!normalize_anthropic_messages_for_upstream(&mut body, base));
            assert_eq!(body, original);
        }
    }

    #[test]
    fn deepseek_official_strips_effort_only_when_thinking_is_disabled() {
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "thinking": { "type": "disabled" },
            "output_config": { "effort": "max" },
            "reasoning_effort": "high"
        });
        assert!(normalize_deepseek_thinking_disabled_strip_effort(
            &mut body,
            "https://api.deepseek.com/anthropic/"
        ));
        assert!(body.get("output_config").is_none());
        assert!(body.get("reasoning_effort").is_none());
        let mut enabled =
            json!({"thinking": {"type": "enabled"}, "output_config": {"effort": "max"}});
        assert!(!normalize_deepseek_thinking_disabled_strip_effort(
            &mut enabled,
            "https://api.deepseek.com/anthropic"
        ));
        let mut elsewhere = json!({"thinking": {"type": "disabled"}, "reasoning_effort": "high"});
        assert!(!normalize_deepseek_thinking_disabled_strip_effort(
            &mut elsewhere,
            "https://relay.test/anthropic"
        ));
    }

    #[test]
    fn claude_to_responses_uses_only_a_client_session_as_the_cache_key() {
        let body = json!({"model": "gpt-5", "max_tokens": 10, "messages": [{"role": "user", "content": "hi"}]});
        let with = prepare_request(
            Conversion::ClaudeTo(ApiFormat::OpenaiResponses),
            "/v1/messages?beta=true",
            body.clone(),
            Some("claude-opus-5"),
            &line("https://relay.test/v1", Some("sess-1")),
            None,
        )
        .unwrap();
        assert_eq!(with.endpoint, "/v1/responses");
        assert_eq!(with.body["prompt_cache_key"], "sess-1");
        let without = prepare_request(
            Conversion::ClaudeTo(ApiFormat::OpenaiResponses),
            "/v1/messages",
            body,
            None,
            &line("https://relay.test/v1", None),
            None,
        )
        .unwrap();
        assert!(without.body.get("prompt_cache_key").is_none());
    }

    #[test]
    fn claude_to_chat_streaming_injects_include_usage_and_strips_the_one_m_marker() {
        let body = json!({"model": "gpt-5.6-sol[1M]", "max_tokens": 10, "stream": true, "messages": [{"role": "user", "content": "hi"}]});
        let p = prepare_request(
            Conversion::ClaudeTo(ApiFormat::OpenaiChat),
            "/v1/messages",
            body,
            None,
            &line("https://relay.test", None),
            None,
        )
        .unwrap();
        assert_eq!(p.endpoint, "/v1/chat/completions");
        assert_eq!(p.body["stream_options"]["include_usage"], true);
        assert_eq!(p.body["model"], "gpt-5.6-sol");
    }

    // ---- cc-switch providers/codex.rs 的推断（供应商名不再参与）

    #[test]
    fn codex_chat_reasoning_is_inferred_from_the_address_and_the_model() {
        let ds =
            infer_codex_chat_reasoning_config("https://api.deepseek.com/v1", &json!({})).unwrap();
        assert_eq!(ds.effort_value_mode.as_deref(), Some("deepseek"));
        let or = infer_codex_chat_reasoning_config(
            "https://openrouter.ai/api/v1",
            &json!({"model": "deepseek/deepseek-v4"}),
        )
        .unwrap();
        assert_eq!(
            or.effort_param.as_deref(),
            Some("reasoning.effort"),
            "平台优先，不看模型名"
        );
        let step = infer_codex_chat_reasoning_config(
            "https://relay.test/v1",
            &json!({"model": "step-3.5-flash-2603"}),
        )
        .unwrap();
        assert_eq!(step.effort_value_mode.as_deref(), Some("low_high"));
        assert!(infer_codex_chat_reasoning_config(
            "https://relay.test/v1",
            &json!({"model": "gpt-5"})
        )
        .is_none());
    }

    #[test]
    fn the_chat_cache_key_goes_only_to_hosts_that_accept_it() {
        let mut body = json!({});
        assert!(!inject_codex_chat_prompt_cache_key(
            "https://relay.test/v1",
            &mut body,
            Some("k"),
            Some("s")
        ));
        assert!(inject_codex_chat_prompt_cache_key(
            "https://api.openai.com/v1",
            &mut body,
            None,
            Some("s")
        ));
        assert_eq!(body["prompt_cache_key"], "s");
        assert!(should_send_codex_chat_prompt_cache_key(
            "https://api.kimi.com/coding/v1"
        ));
        assert!(!should_send_codex_chat_prompt_cache_key(
            "https://api.kimi.com/v1"
        ));
    }

    #[test]
    fn codex_to_anthropic_strips_one_m_and_caches_the_prefix() {
        let body = json!({
            "model": "claude-opus-5[1m]",
            "instructions": "be brief",
            "input": [{"role": "user", "content": [{"type": "input_text", "text": "hi"}]}],
            "stream": true
        });
        let p = prepare_request(
            Conversion::ResponsesToAnthropic,
            "/v1/responses",
            body,
            None,
            &line("https://relay.test", None),
            None,
        )
        .unwrap();
        assert_eq!(p.endpoint, "/v1/messages");
        assert!(p.anthropic_one_m);
        assert_eq!(p.body["model"], "claude-opus-5");
        assert_eq!(p.body["max_tokens"], DEFAULT_CODEX_ANTHROPIC_MAX_TOKENS);
        assert!(
            p.body.to_string().contains("cache_control"),
            "提示缓存断点：{}",
            p.body
        );
    }

    #[test]
    fn codex_to_chat_converts_input_into_messages() {
        let body = json!({
            "model": "glm-5",
            "input": [{"role": "user", "content": [{"type": "input_text", "text": "hi"}]}]
        });
        let p = prepare_request(
            Conversion::ResponsesToChat,
            "/v1/responses",
            body,
            None,
            &line("https://relay.test/v1", Some("s")),
            None,
        )
        .unwrap();
        assert_eq!(p.endpoint, "/v1/chat/completions");
        assert!(p.body["messages"].is_array());
        assert!(
            p.body.get("prompt_cache_key").is_none(),
            "不认的网关不发缓存键"
        );
    }

    // ---- 响应

    #[test]
    fn a_whole_chat_response_comes_back_as_an_anthropic_message() {
        let ctx = ResponseContext::new(
            Conversion::ClaudeTo(ApiFormat::OpenaiChat),
            &json!({"model": "claude-opus-5", "messages": []}),
            "r",
            "s",
        );
        let chat = json!({
            "id": "chatcmpl-1", "model": "gpt-5", "object": "chat.completion",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "hello"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 3, "completion_tokens": 1}
        });
        let out = convert_whole(
            &ctx,
            &http::HeaderMap::new(),
            chat.to_string().as_bytes(),
            None,
        )
        .unwrap();
        assert_eq!(out["type"], "message");
        assert_eq!(out["content"][0]["text"], "hello");
        // 没标类型的 SSE 也认得出来。
        let sse = "data: {\"id\":\"c\",\"model\":\"gpt-5\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        let out = convert_whole(&ctx, &http::HeaderMap::new(), sse.as_bytes(), None).unwrap();
        assert_eq!(out["content"][0]["text"], "hi");
        // 认不出来：报错，而且报错里不带正文。
        let err = convert_whole(&ctx, &http::HeaderMap::new(), b"<html>secret</html>", None)
            .unwrap_err()
            .to_string();
        assert!(!err.contains("secret"), "{err}");
    }

    #[test]
    fn a_whole_chat_response_comes_back_to_codex_as_a_response_object() {
        let ctx = ResponseContext::new(
            Conversion::ResponsesToChat,
            &json!({"model": "glm-5", "input": []}),
            "r",
            "s",
        );
        let chat = json!({
            "id": "chatcmpl-1", "model": "glm-5",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}]
        });
        let out = convert_whole(
            &ctx,
            &http::HeaderMap::new(),
            chat.to_string().as_bytes(),
            None,
        )
        .unwrap();
        assert_eq!(out["object"], "response");
    }

    #[tokio::test]
    async fn a_chat_stream_comes_back_as_anthropic_events() {
        let ctx = ResponseContext::new(
            Conversion::ClaudeTo(ApiFormat::OpenaiChat),
            &json!({"model": "claude-opus-5", "messages": []}),
            "r",
            "s",
        );
        let chunks = vec![
            Ok::<_, std::io::Error>(Bytes::from("data: {\"id\":\"c\",\"model\":\"gpt-5\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hi\"}}]}\n\n")),
            Ok(Bytes::from("data: {\"id\":\"c\",\"model\":\"gpt-5\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n")),
        ];
        let mut out = convert_stream(&ctx, futures::stream::iter(chunks), None);
        let mut text = String::new();
        while let Some(chunk) = out.next().await {
            text.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
        }
        assert!(text.contains("message_start"), "{text}");
        assert!(text.contains("\"text\":\"hi\""), "{text}");
        assert!(text.contains("message_stop"), "{text}");
    }

    #[test]
    fn upstream_errors_come_back_in_the_clients_own_shape() {
        let openai = r#"{"error":{"message":"Invalid model","type":"invalid_request_error"}}"#;
        let claude =
            client_error_body(Conversion::ClaudeTo(ApiFormat::OpenaiChat), openai).unwrap();
        assert_eq!(claude["type"], "error");
        assert_eq!(claude["error"]["message"], "Invalid model");
        assert_eq!(claude["error"]["type"], "invalid_request_error");
        let codex = client_error_body(
            Conversion::ResponsesToAnthropic,
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
        )
        .unwrap();
        assert_eq!(codex["error"]["message"], "Overloaded");
        let plain = client_error_body(Conversion::ClaudeTo(ApiFormat::GeminiNative), "Bad Gateway")
            .unwrap();
        assert_eq!(plain["error"]["message"], "Bad Gateway");
        assert!(client_error_body(Conversion::Passthrough, openai).is_none());
    }

    #[test]
    fn codex_fingerprint_headers_are_recognized() {
        for h in [
            "originator",
            "x-stainless-os",
            "x-codex-turn",
            "chatgpt-account-id",
            "x-oai-attestation",
            "version",
        ] {
            assert!(is_codex_client_fingerprint_header(h), "{h}");
        }
        for h in [
            "content-type",
            "x-api-key",
            "anthropic-version",
            "user-agent",
        ] {
            assert!(!is_codex_client_fingerprint_header(h), "{h}");
        }
    }

    #[test]
    fn only_xai_lines_get_their_native_requests_rewritten() {
        let tools = json!({
            "model": "grok-4",
            "input": [],
            "tools": [{"type": "namespace", "name": "mcp__x", "tools": [
                {"type": "function", "name": "do", "parameters": {"type": "object"}}
            ]}]
        });
        let mut relay = tools.clone();
        let repair = prepare_codex_native(&mut relay, "https://relay.test/v1", "r").unwrap();
        assert!(matches!(repair, NativeRepair::LateArguments));
        assert_eq!(relay, tools, "别的线一个字节都不改");
        let mut xai = tools.clone();
        let repair = prepare_codex_native(&mut xai, "https://api.x.ai/v1", "r").unwrap();
        assert!(matches!(repair, NativeRepair::Xai(_)));
        assert!(
            !xai.to_string().contains("\"namespace\""),
            "命名空间压平了：{xai}"
        );
        assert!(repair_native_whole(&NativeRepair::LateArguments, b"{}").is_none());
    }

    #[test]
    fn the_stream_decision_follows_each_handler() {
        let claude = ResponseContext::new(
            Conversion::ClaudeTo(ApiFormat::OpenaiChat),
            &json!({}),
            "r",
            "s",
        );
        assert!(claude.stream_upstream(true, false, true));
        assert!(claude.stream_upstream(false, true, false));
        assert!(!claude.stream_upstream(false, false, true));
        let anth = ResponseContext::new(Conversion::ResponsesToAnthropic, &json!({}), "r", "s");
        assert!(
            !anth.stream_upstream(true, false, true),
            "标了 JSON 的整份读完再转"
        );
        assert!(anth.stream_upstream(true, false, false));
        assert!(anth.stream_upstream(false, true, false));
    }
}
