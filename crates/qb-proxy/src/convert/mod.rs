// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 协议转换（中转站重构阶段 3，cc-switch 的 R2）：客户端说的协议跟线路上游说的不一样时，
//! 请求换过去、响应换回来。**全部是纯计算** —— 收 `serde_json::Value` / 字节流，回同样的东西；
//! 发请求、读响应、选哪一种转换在 `qb-app::local_router`。
//!
//! 文件照搬 cc-switch v4.0.14 的 `src-tauri/src/proxy/providers/`（加上 `proxy/error_mapper.rs`、
//! `proxy/gemini_url.rs`），测试一起搬；改的只有路径（`crate::proxy::*` → 本 crate 的对应模块，
//! `json_canonical` → `qb_foundation::json_canonical`）和日志宏（`log::` → `tracing::`）。
//! 每个文件头一行写着出处。
//!
//! | 客户端 → 上游 | 请求 | 响应（整份 / 流式） |
//! |---|---|---|
//! | Anthropic → OpenAI Chat | [`transform`] | [`transform`] / [`streaming`]（含 [`inline_think`]） |
//! | Anthropic → OpenAI Responses | [`transform_responses`] | [`transform_responses`] / [`streaming_responses`]（含 [`reasoning_bridge`]） |
//! | Anthropic → Gemini 原生 | [`transform_gemini`]（含 [`gemini_schema`]、[`gemini_shadow`]） | [`transform_gemini`] / [`streaming_gemini`] |
//! | Codex（Responses）→ OpenAI Chat | [`transform_codex_chat`]（含 [`codex_chat_common`]、[`codex_chat_history`]） | [`transform_codex_chat`] / [`streaming_codex_chat`] |
//! | Codex（Responses）→ Anthropic | [`transform_codex_anthropic`] | [`transform_codex_anthropic`] / [`streaming_codex_anthropic`] |
//! | Codex 原生 Responses 透传时的清洗 | [`transform_codex_responses_namespace`]、[`transform_codex_responses_xai_sanitize`]、[`responses_late_arguments`] | 同左 |
//! | Codex → Chat 时 Moonshot 的 schema | [`transform_codex_chat_moonshot_schema`] | — |
//!
//! **没搬的**（cc-switch 的 Copilot / OAuth 专用部分，QB Gate 不做）：`copilot_auth.rs`、`copilot_model_map.rs`、
//! `codex_oauth_auth.rs`、`xai_oauth_auth.rs`，以及适配器里的对应分支；`gemini.rs` / `auth.rs` / `adapter.rs`
//! 是 cc-switch 的 Gemini CLI 客户端与鉴权抽象 —— QB Gate 的鉴权在 `local_router::UpstreamAuth`，
//! Gemini CLI 这个客户端在阶段 4 才加。适配器里跟转换有关的那一半（选哪种格式、DeepSeek 的 thinking 历史、
//! Codex 转 Chat 的 reasoning 参数）改写在 [`plan`]，按线路自己的 `upstream_format` 与地址判断，
//! 不再读 cc-switch 的供应商配置。

pub mod codex_chat_common;
pub mod codex_chat_history;
pub mod codex_responses_sse;
pub mod error_mapper;
pub mod gemini_schema;
pub mod gemini_shadow;
pub mod gemini_url;
pub mod inline_think;
pub mod models;
pub mod plan;
pub mod reasoning_bridge;
pub mod responses_late_arguments;
pub mod sse_aggregate;
pub mod streaming;
pub mod streaming_codex_anthropic;
pub mod streaming_codex_chat;
pub mod streaming_gemini;
pub mod streaming_responses;
pub mod transform;
pub mod transform_codex_anthropic;
pub mod transform_codex_chat;
pub mod transform_codex_chat_moonshot_schema;
pub mod transform_codex_responses_namespace;
pub mod transform_codex_responses_xai_sanitize;
pub mod transform_gemini;
pub mod transform_responses;

use serde::{Deserialize, Serialize};

/// Codex Responses -> Chat Completions 的 reasoning 能力描述。
///
/// 照搬 cc-switch `provider.rs` 的同名结构（那边挂在供应商的 `meta` 上）；QB Gate 由 [`plan`] 按线路地址
/// 与模型名推出来。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CodexChatReasoningConfig {
    #[serde(rename = "supportsThinking", skip_serializing_if = "Option::is_none")]
    pub supports_thinking: Option<bool>,
    #[serde(rename = "supportsEffort", skip_serializing_if = "Option::is_none")]
    pub supports_effort: Option<bool>,
    #[serde(rename = "thinkingParam", skip_serializing_if = "Option::is_none")]
    pub thinking_param: Option<String>,
    #[serde(rename = "effortParam", skip_serializing_if = "Option::is_none")]
    pub effort_param: Option<String>,
    #[serde(rename = "effortValueMode", skip_serializing_if = "Option::is_none")]
    pub effort_value_mode: Option<String>,
    /// 声明性字段：标注上游 reasoning 的回传位置（reasoning_content / reasoning /
    /// reasoning_details / think_tags）。当前响应侧 `extract_reasoning_field_text`
    /// 靠穷举字段提取、并不读取本字段；保留作文档说明与未来按格式分发（如 think_tags）的预留。
    #[serde(rename = "outputFormat", skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    /// 运行时字段（不持久化）：当前请求模型在平台侧声明的合法 effort 档位。
    /// 仅 "zen" 值映射消费：Some → 钳到合法档；None → 不发 effort 字段。
    #[serde(skip)]
    pub effort_levels: Option<Vec<String>>,
}
