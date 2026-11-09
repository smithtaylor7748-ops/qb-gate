//! 请求级 Codex 历史修复。按公开 Responses 条目形状独立实现；不读取或修改本机会话。
//! 只处理明确支持的字段，不用占位符代替缺失的会话上下文。

use qb_station::router::SessionRecoverySettings;
use serde_json::Value;

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Changes {
    pub ids: usize,
    pub encrypted: usize,
}

const MISSING_HISTORY: &str = "会话修复需要请求携带完整 input 历史；当前请求缺少完整历史或含服务端引用，无法安全恢复。请使用原线路继续，或让客户端重放完整历史。";
const OPAQUE_COMPACTION: &str = "会话修复停止：当前压缩历史缺少可还原的明文摘要，删除后会丢失上下文。请回到生成该压缩历史的线路恢复，或使用完整可读历史。";
const UNKNOWN_ENCRYPTION: &str = "会话修复停止：工具输出、子代理消息或未知条目包含不支持的加密状态，无法安全移除。请使用能够读取该状态的原线路。";

pub(super) fn repair(
    body: &mut Value,
    settings: SessionRecoverySettings,
) -> Result<Changes, &'static str> {
    if !settings.active() {
        return Ok(Changes::default());
    }
    if ["previous_response_id", "conversation"]
        .iter()
        .any(|key| body.get(key).is_some_and(|v| !v.is_null()))
    {
        return Err(MISSING_HISTORY);
    }
    let Some(input) = body.get("input") else {
        return Err(MISSING_HISTORY);
    };
    if input.is_string() {
        return Ok(Changes::default());
    }
    let Some(items) = input.as_array() else {
        return Err(MISSING_HISTORY);
    };
    // 完整预检再修改：后面的拒绝不能留下半份已经清理的历史。
    for item in items {
        if !item.is_object() {
            return Err(MISSING_HISTORY);
        }
        if item.get("type").and_then(Value::as_str) == Some("item_reference") {
            return Err(MISSING_HISTORY);
        }
        if settings.filter_encrypted_content {
            if qb_proxy::codex_compaction::is_compaction_item(item) {
                if readable_compaction(item).is_none() {
                    return Err(OPAQUE_COMPACTION);
                }
            } else if protocol_has_opaque(item) {
                let reasoning = item.get("type").and_then(Value::as_str) == Some("reasoning");
                let supported = reasoning
                    && !has_encrypted_parts(item)
                    && item
                        .get("encrypted_content")
                        .is_none_or(|value| value.is_string() || value.is_null());
                if !supported {
                    return Err(UNKNOWN_ENCRYPTION);
                }
            }
        }
    }

    let mut changes = Changes::default();
    let items = body.get_mut("input").and_then(Value::as_array_mut).unwrap();
    for item in items.iter_mut() {
        if settings.strip_item_ids
            && supported_id(item)
            && item
                .as_object_mut()
                .is_some_and(|object| object.remove("id").is_some())
        {
            changes.ids += 1;
        }
        if !settings.filter_encrypted_content {
            continue;
        }
        if let Some(summary) = readable_compaction(item) {
            // 这是本机可逆包装的明文摘要，不是试图解密第三方状态。
            *item = qb_proxy::codex_compaction::user_message_item(&summary);
            changes.encrypted += 1;
        } else if item.get("type").and_then(Value::as_str) == Some("reasoning")
            && item
                .as_object_mut()
                .is_some_and(|object| object.remove("encrypted_content").is_some())
        {
            changes.encrypted += 1;
            if !retained_reasoning(item) {
                *item = Value::Null;
            }
        }
    }
    items.retain(|item| !item.is_null());
    Ok(changes)
}

/// 原生 Responses 可以把密文原样交给签发它的上游；换成 Chat/Anthropic 时无法保留它。
pub(super) fn guard_conversion(body: &Value) -> Result<(), &'static str> {
    if body
        .get("input")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                qb_proxy::codex_compaction::is_compaction_item(item)
                    && readable_compaction(item).is_none()
            })
        })
    {
        return Err(OPAQUE_COMPACTION);
    }
    Ok(())
}

fn supported_id(item: &Value) -> bool {
    match item.get("type").and_then(Value::as_str) {
        Some(
            "message"
            | "reasoning"
            | "function_call"
            | "function_call_output"
            | "custom_tool_call"
            | "custom_tool_call_output"
            | "compaction"
            | "compaction_summary"
            | "context_compaction",
        ) => true,
        None => item
            .get("role")
            .and_then(Value::as_str)
            .is_some_and(|role| matches!(role, "user" | "assistant" | "system" | "developer")),
        _ => false,
    }
}

fn readable_compaction(item: &Value) -> Option<String> {
    if !qb_proxy::codex_compaction::is_compaction_item(item) {
        return None;
    }
    item.get("encrypted_content")
        .and_then(Value::as_str)
        .and_then(qb_proxy::codex_compaction::decode_compaction_summary)
        .filter(|text| !text.trim().is_empty())
}

fn retained_reasoning(item: &Value) -> bool {
    item.as_object().is_some_and(|object| {
        object.iter().any(|(key, value)| {
            // 未知明文字段原样保留，不能因为不是标准 summary/content 就丢掉整条。
            !matches!(key.as_str(), "type" | "id" | "status")
                && match value {
                    Value::Null => false,
                    Value::String(text) => !text.is_empty(),
                    Value::Array(parts) => !parts.is_empty(),
                    Value::Object(fields) => !fields.is_empty(),
                    _ => true,
                }
        })
    })
}

fn protocol_has_opaque(item: &Value) -> bool {
    item.get("encrypted_content")
        .is_some_and(|value| !value.is_null())
        || is_encrypted_part(item)
        || has_encrypted_parts(item)
}

/// 仅识别协议内容块，不递归扫描工具参数/结果中的任意业务对象。
fn has_encrypted_parts(item: &Value) -> bool {
    ["content", "output", "summary"].iter().any(|key| {
        item.get(key)
            .and_then(Value::as_array)
            .is_some_and(|parts| parts.iter().any(is_encrypted_part))
    })
}

fn is_encrypted_part(part: &Value) -> bool {
    part.get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| matches!(kind, "encrypted" | "encrypted_text" | "encrypted_content"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ciphertext_filter_is_independent_and_keeps_readable_reasoning_and_ids() {
        let mut body = json!({"input":[
            {"type":"reasoning","id":"rs_empty","summary":[],"encrypted_content":"opaque"},
            {"type":"reasoning","id":"rs_readable","summary":[{"type":"summary_text","text":"keep me"}],"encrypted_content":"opaque"},
            {"type":"message","id":"msg_1","role":"user","content":"continue"}
        ]});
        let changes = repair(
            &mut body,
            SessionRecoverySettings {
                filter_encrypted_content: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            changes,
            Changes {
                ids: 0,
                encrypted: 2
            }
        );
        assert_eq!(
            body,
            json!({"input":[
                {"type":"reasoning","id":"rs_readable","summary":[{"type":"summary_text","text":"keep me"}]},
                {"type":"message","id":"msg_1","role":"user","content":"continue"}
            ]})
        );
    }

    #[test]
    fn preflight_refusal_does_not_leave_half_repaired_history() {
        let original = json!({"input":[
            {"type":"message","id":"msg_1","role":"user","content":"keep me"},
            {"type":"item_reference","id":"msg_missing"}
        ]});
        let mut body = original.clone();
        assert_eq!(
            repair(
                &mut body,
                SessionRecoverySettings {
                    strip_item_ids: true,
                    ..Default::default()
                }
            ),
            Err(MISSING_HISTORY)
        );
        assert_eq!(body, original);
        let original = json!({"input":[{"type":"reasoning","encrypted_content":{"text":"unknown readable shape"}}]});
        let mut body = original.clone();
        assert_eq!(
            repair(
                &mut body,
                SessionRecoverySettings {
                    filter_encrypted_content: true,
                    ..Default::default()
                }
            ),
            Err(UNKNOWN_ENCRYPTION)
        );
        assert_eq!(body, original);
    }

    #[test]
    fn old_router_settings_leave_both_recovery_filters_off() {
        let settings: qb_station::router::RouterSettings =
            serde_json::from_str(r#"{"client":"codex"}"#).unwrap();
        assert!(!settings.session_recovery.active());
        let mut body = json!({"previous_response_id":"resp_1","input":[{"type":"item_reference","id":"msg_1"}]});
        let original = body.clone();
        assert_eq!(
            repair(&mut body, settings.session_recovery).unwrap(),
            Changes::default()
        );
        assert_eq!(body, original);
    }

    #[test]
    fn encrypted_business_fields_are_not_protocol_state_and_unknown_plaintext_is_kept() {
        let original = json!({"input":[
            {"type":"function_call","call_id":"call_1","name":"save","arguments":{"encrypted_content":"business-value"}},
            {"type":"function_call_output","call_id":"call_1","output":{"encrypted_content":"business-value","nested":{"type":"encrypted_content"}}},
            {"type":"reasoning","id":"rs_1","text":"nonstandard but readable","encrypted_content":"opaque"}
        ]});
        let mut body = original.clone();
        repair(
            &mut body,
            SessionRecoverySettings {
                filter_encrypted_content: true,
                ..Default::default()
            },
        )
        .unwrap();
        let mut expected = original;
        expected["input"][2]
            .as_object_mut()
            .unwrap()
            .remove("encrypted_content");
        assert_eq!(body, expected);
    }

    #[test]
    fn compaction_requires_readable_payload_only_when_filtering_or_converting() {
        for item in [
            json!({"type":"compaction","id":"cmp_1","encrypted_content":"opaque"}),
            json!({"type":"compaction","id":"cmp_1"}),
        ] {
            let mut body = json!({"input":[item]});
            repair(
                &mut body,
                SessionRecoverySettings {
                    strip_item_ids: true,
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(body["input"][0].get("id").is_none());
            assert_eq!(guard_conversion(&body), Err(OPAQUE_COMPACTION));
            assert_eq!(
                repair(
                    &mut body,
                    SessionRecoverySettings {
                        filter_encrypted_content: true,
                        ..Default::default()
                    }
                ),
                Err(OPAQUE_COMPACTION)
            );
        }
    }
}
