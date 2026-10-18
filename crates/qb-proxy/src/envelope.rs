// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! HTTP 2xx 里藏着的错误。
//!
//! 有的兼容网关把错误包在 HTTP 200 里回来（Anthropic 的 `{"type":"error",...}`、Responses 的
//! `status: failed`）。**要在重试循环里认出来才换得了线** —— 等它流到客户端那头再认，
//! 这一发就已经交出去了。cc-switch 的 `forwarder.rs` 里这几个是私有函数
//! （`codex_anthropic_error_envelope_message` / `responses_error_envelope_message` /
//! `inspect_responses_json_document` / `inspect_responses_start_event`），这里原样搬出来。
//!
//! 另加一个 cc-switch 没有的：[`inspect_anthropic_start_event`]，Anthropic 流的第一个事件就是
//! `event: error`（还没出 `message_start` 就过载了）时，在交出流之前认出来 ——
//! 跟 Responses 那条同一个道理，只是 Claude 那边 cc-switch 只等首包、不看内容。

use crate::proxy_error::ProxyError;
use crate::sse_text::strip_sse_field;
use serde_json::Value;

/// Anthropic 形状的错误信封（`{"type":"error","error":{...}}`，或干脆只有 `error`）。
///
/// cc-switch 里叫 `codex_anthropic_error_envelope_message`（它只在 Codex → Anthropic 转换时用）；
/// Claude Code 直通 Anthropic 线时一样适用。
pub fn anthropic_error_envelope_message(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("error") && value.get("error").is_none() {
        return None;
    }
    let error = value.get("error").unwrap_or(&value);
    let error_type = error.get("type").and_then(Value::as_str).unwrap_or("error");
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| error.to_string());
    Some(format!("{error_type}: {message}"))
}

/// Responses（以及带 `error` 字段的 Chat Completions）在 2xx 里的失败。
pub fn responses_error_envelope_message(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let status = value.get("status").and_then(Value::as_str);
    let has_error = value.get("error").is_some_and(|error| !error.is_null());
    if !matches!(status, Some("failed" | "cancelled")) && !has_error {
        return None;
    }

    let error = value.get("error").unwrap_or(&value);
    let error_type = error
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| error.get("code").and_then(Value::as_str))
        .unwrap_or_else(|| status.unwrap_or("error"));
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .filter(|message| !message.trim().is_empty())
        .unwrap_or(match status {
            Some("cancelled") => "response generation was cancelled",
            _ => "response generation failed",
        });
    Some(format!("{error_type}: {message}"))
}

/// A streaming request may receive a whole JSON document even when the gateway
/// omits `application/json`. `None` means either "not JSON" or "not complete yet";
/// a parsed document is safe to commit unless it is a semantic failure envelope.
pub fn inspect_responses_json_document(buffer: &str) -> Option<Result<(), ProxyError>> {
    let trimmed = buffer.trim();
    if !matches!(trimmed.as_bytes().first(), Some(b'{') | Some(b'[')) {
        return None;
    }
    let _: Value = serde_json::from_str(trimmed).ok()?;
    if let Some(message) = responses_error_envelope_message(trimmed.as_bytes()) {
        return Some(Err(ProxyError::TransformError(format!(
            "Responses upstream returned a 2xx failure: {message}"
        ))));
    }
    Some(Ok(()))
}

/// Inspect one complete Responses SSE block while the response is still inside
/// the retry loop. `None` means the event is lifecycle-only and priming should
/// continue; `Some(Ok(()))` means it is safe to commit/replay the stream.
pub fn inspect_responses_start_event(block: &str) -> Option<Result<(), ProxyError>> {
    let (named_event, data_lines) = split_block(block);
    if data_lines.is_empty() {
        return None;
    }
    let value: Value = match serde_json::from_str(&data_lines.join("\n")) {
        Ok(value) => value,
        Err(_) => return None,
    };
    let event = named_event
        .as_deref()
        .filter(|event| !event.is_empty())
        .or_else(|| value.get("type").and_then(Value::as_str))
        .unwrap_or("");

    let response = value.get("response").unwrap_or(&value);
    if matches!(
        response.get("status").and_then(Value::as_str),
        Some("failed" | "cancelled")
    ) || response.get("error").is_some_and(|error| !error.is_null())
    {
        let error = response.get("error").unwrap_or(response);
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| error.as_str())
            .unwrap_or("Responses upstream failed before output");
        let error_type = error
            .get("type")
            .and_then(Value::as_str)
            .or_else(|| error.get("code").and_then(Value::as_str))
            .or_else(|| response.get("status").and_then(Value::as_str))
            .unwrap_or("upstream_error");
        return Some(Err(ProxyError::TransformError(format!(
            "Responses upstream {error_type}: {message}"
        ))));
    }

    match event {
        "response.failed" | "error" => {
            let error = response.get("error").unwrap_or(response);
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
                .unwrap_or("Responses upstream emitted an error before output");
            let error_type = error
                .get("type")
                .and_then(Value::as_str)
                .or_else(|| error.get("code").and_then(Value::as_str))
                .unwrap_or("upstream_error");
            Some(Err(ProxyError::TransformError(format!(
                "Responses upstream {error_type}: {message}"
            ))))
        }
        "response.created" | "response.in_progress" | "response.queued" => None,
        "" => None,
        // Productive output, incomplete, and completed terminals are all safe to
        // expose. Mid-stream failures after this point are surfaced by the converter
        // but intentionally do not switch providers.
        _ => Some(Ok(())),
    }
}

/// Anthropic 流的一个完整 SSE 块，还在重试循环里时看一眼（QB Gate 加的）。
///
/// `event: error` → 上游在出内容之前就报错了（最常见的是 `overloaded_error`），还能换线；
/// `message_start` 或任何内容事件 → 可以交出流；`ping` 之类 → 接着等下一块。
pub fn inspect_anthropic_start_event(block: &str) -> Option<Result<(), ProxyError>> {
    let (named_event, data_lines) = split_block(block);
    let value: Option<Value> = serde_json::from_str(&data_lines.join("\n")).ok();
    let event = named_event
        .as_deref()
        .filter(|event| !event.is_empty())
        .or_else(|| value.as_ref()?.get("type")?.as_str())
        .unwrap_or("");
    match event {
        "error" => {
            let message = value
                .as_ref()
                .and_then(|v| anthropic_error_envelope_message(v.to_string().as_bytes()))
                .unwrap_or_else(|| "error: upstream emitted an error before output".to_string());
            Some(Err(ProxyError::TransformError(format!(
                "Anthropic upstream {message}"
            ))))
        }
        "ping" | "" => None,
        _ => Some(Ok(())),
    }
}

fn split_block(block: &str) -> (Option<String>, Vec<&str>) {
    let mut named_event = None;
    let mut data_lines = Vec::new();
    for line in block.lines() {
        if let Some(event) = strip_sse_field(line, "event") {
            named_event = Some(event.trim().to_string());
        } else if let Some(data) = strip_sse_field(line, "data") {
            data_lines.push(data);
        }
    }
    (named_event, data_lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_error_envelopes_are_recognised_in_both_shapes() {
        let body =
            br#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert_eq!(
            anthropic_error_envelope_message(body).as_deref(),
            Some("overloaded_error: Overloaded")
        );
        let bare = br#"{"error":{"type":"rate_limit_error","message":"slow"}}"#;
        assert_eq!(
            anthropic_error_envelope_message(bare).as_deref(),
            Some("rate_limit_error: slow")
        );
        // 正常的消息不是信封。
        let ok = br#"{"type":"message","content":[]}"#;
        assert_eq!(anthropic_error_envelope_message(ok), None);
        assert_eq!(anthropic_error_envelope_message(b"not json"), None);
    }

    #[test]
    fn responses_failures_inside_a_200_are_recognised() {
        let failed = br#"{"status":"failed","error":{"code":"server_error","message":"boom"}}"#;
        assert_eq!(
            responses_error_envelope_message(failed).as_deref(),
            Some("server_error: boom")
        );
        let cancelled = br#"{"status":"cancelled"}"#;
        assert_eq!(
            responses_error_envelope_message(cancelled).as_deref(),
            Some("cancelled: response generation was cancelled")
        );
        let ok = br#"{"status":"completed","error":null,"output":[]}"#;
        assert_eq!(responses_error_envelope_message(ok), None);
    }

    #[test]
    fn responses_stream_priming_waits_through_lifecycle_events() {
        assert!(inspect_responses_start_event(
            "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"status\":\"in_progress\"}}"
        )
        .is_none());
        assert!(matches!(
            inspect_responses_start_event(
                "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}"
            ),
            Some(Ok(()))
        ));
        assert!(matches!(
            inspect_responses_start_event(
                "event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\",\"error\":{\"code\":\"rate_limit_exceeded\",\"message\":\"slow\"}}}"
            ),
            Some(Err(ProxyError::TransformError(_)))
        ));
        // 没有 data 行的块（注释、心跳）接着等。
        assert!(inspect_responses_start_event(": keep-alive").is_none());
    }

    #[test]
    fn a_whole_json_document_on_a_stream_is_judged_as_a_document() {
        assert!(matches!(
            inspect_responses_json_document("{\"status\":\"completed\",\"output\":[]}"),
            Some(Ok(()))
        ));
        assert!(matches!(
            inspect_responses_json_document(
                "{\"status\":\"failed\",\"error\":{\"message\":\"x\"}}"
            ),
            Some(Err(_))
        ));
        // 还不完整、或者根本是 SSE：不下结论。
        assert!(inspect_responses_json_document("{\"status\":").is_none());
        assert!(inspect_responses_json_document("event: x\ndata: {}").is_none());
    }

    #[test]
    fn an_anthropic_stream_that_opens_with_an_error_can_still_fail_over() {
        let block = "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}";
        match inspect_anthropic_start_event(block) {
            Some(Err(ProxyError::TransformError(message))) => {
                assert!(message.contains("overloaded_error"), "{message}")
            }
            other => panic!("没认出开头的 error 事件：{other:?}"),
        }
        assert!(inspect_anthropic_start_event("event: ping\ndata: {\"type\":\"ping\"}").is_none());
        assert!(matches!(
            inspect_anthropic_start_event(
                "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}"
            ),
            Some(Ok(()))
        ));
    }
}
