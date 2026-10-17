// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 跟键顺序无关的 JSON：排好键再序列化、再取哈希。
//!
//! # 为什么放在地基层
//!
//! workspace 的 `serde_json` 开了 `preserve_order`（2026-10-17，阶段 1 第 2 步）：
//! `Value` 里的对象按**插入 / 解析顺序**存，不再按字母序。这对转发请求体是对的 ——
//! 改一个字段不该把整个请求的键重新排一遍（结构化输出的属性顺序会影响模型生成的顺序）——
//! 但任何「把 `Value` 序列化了拿去比、拿去存哈希」的地方从此都跟键的顺序有关了。
//! 要稳定的哈希就走这里：扩展中心的 MCP 条目指纹（`qb-extensions` 的 `hash_json`）用它，
//! 从 cc-switch 搬来的请求改写代码（`qb-proxy`）也用它。
//!
//! [`canonicalize_value`] 排序用的是 `String` 的 `Ord`（逐字节），跟没开 `preserve_order`
//! 时 `BTreeMap` 的顺序**完全一样** —— 所以 `serde_json::to_vec(&canonicalize_value(v))`
//! 跟老版本对同一个 `Value` 序列化出来的字节逐字节相同，升级前存下的哈希照样对得上。

use serde_json::Value;
use sha2::{Digest, Sha256};

/// 把每一层对象的键都按字母序重排（数组的元素顺序不动）。
pub fn canonicalize_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize_value).collect()),
        Value::Object(map) => {
            let mut entries = map.into_iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));

            let mut sorted = serde_json::Map::new();
            for (key, value) in entries {
                sorted.insert(key, canonicalize_value(value));
            }
            Value::Object(sorted)
        }
        other => other,
    }
}

/// 紧凑、键按字母序的 JSON 文本。不改动 `value` 本身。
pub fn canonical_json_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => serde_json::to_string(value)
            .expect("serializing a JSON string for canonical output should not fail"),
        Value::Array(values) => {
            let parts = values.iter().map(canonical_json_string).collect::<Vec<_>>();
            format!("[{}]", parts.join(","))
        }
        Value::Object(map) => {
            let mut entries = map.iter().collect::<Vec<_>>();
            entries.sort_by_key(|(left, _)| *left);
            let parts = entries
                .into_iter()
                .map(|(key, value)| {
                    let key = serde_json::to_string(key).expect(
                        "serializing a JSON object key for canonical output should not fail",
                    );
                    format!("{key}:{}", canonical_json_string(value))
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", parts.join(","))
        }
    }
}

/// 认得出是 JSON 就规范化，认不出（纯文本）就原样返回。
pub fn canonicalize_json_string_if_parseable(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return value.to_string();
    }

    serde_json::from_str::<Value>(trimmed)
        .map(|parsed| canonical_json_string(&parsed))
        .unwrap_or_else(|_| value.to_string())
}

/// 工具调用的 `arguments` 字符串规范化。
///
/// 跟 [`canonicalize_json_string_if_parseable`] 只差一处：空串（或全是空白）变成 `"{}"`。
/// 没有参数的工具调用必须写成 `"{}"` —— 严格的上游（如 Minimax）见到 `arguments: ""`
/// 回 400「invalid function arguments json string」，宽松的（OpenAI、Kimi）才当成空对象。
pub fn canonicalize_tool_arguments_str(value: &str) -> String {
    if value.trim().is_empty() {
        return "{}".to_string();
    }
    canonicalize_json_string_if_parseable(value)
}

/// Responses / Chat 条目里的 `arguments` 字段规范化：字符串照上面那条规范化
/// （空的变 `"{}"`），结构化的值按规范形式序列化，缺了这个字段给 `"{}"`。
pub fn canonicalize_tool_arguments(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => canonicalize_tool_arguments_str(s),
        Some(v) => canonical_json_string(v),
        None => "{}".to_string(),
    }
}

/// 规范形式的 SHA-256 前 8 字节（16 位十六进制）。没有值时是 `absent`。
pub fn short_value_hash(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return "absent".to_string();
    };
    short_sha256_hex(canonical_json_string(value).as_bytes())
}

/// SHA-256 前 8 字节（16 位十六进制）。
pub fn short_sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_json_string_sorts_nested_object_keys() {
        let left = json!({
            "b": 2,
            "a": {
                "d": true,
                "c": [3, {"z": 1, "y": 2}]
            }
        });
        let right = json!({
            "a": {
                "c": [3, {"y": 2, "z": 1}],
                "d": true
            },
            "b": 2
        });

        assert_eq!(canonical_json_string(&left), canonical_json_string(&right));
        assert_eq!(
            short_value_hash(Some(&left)),
            short_value_hash(Some(&right))
        );
    }

    #[test]
    fn canonicalize_value_sorts_map_storage_order() {
        let value = canonicalize_value(json!({"b": 2, "a": 1}));

        assert_eq!(serde_json::to_string(&value).unwrap(), r#"{"a":1,"b":2}"#);
    }

    #[test]
    fn canonicalize_json_string_if_parseable_sorts_keys_and_removes_whitespace() {
        assert_eq!(
            canonicalize_json_string_if_parseable(r#"{ "b": 2, "a": 1 }"#),
            r#"{"a":1,"b":2}"#
        );
    }

    #[test]
    fn canonicalize_json_string_if_parseable_preserves_plain_text() {
        assert_eq!(
            canonicalize_json_string_if_parseable("plain text"),
            "plain text"
        );
    }

    #[test]
    fn canonicalize_tool_arguments_str_coerces_empty_to_object() {
        assert_eq!(canonicalize_tool_arguments_str(""), "{}");
        assert_eq!(canonicalize_tool_arguments_str("   "), "{}");
        assert_eq!(canonicalize_tool_arguments_str("\n\t"), "{}");
    }

    #[test]
    fn canonicalize_tool_arguments_str_canonicalizes_valid_json() {
        assert_eq!(
            canonicalize_tool_arguments_str(r#"{ "b": 2, "a": 1 }"#),
            r#"{"a":1,"b":2}"#
        );
    }

    #[test]
    fn canonicalize_tool_arguments_handles_field_variants() {
        // Missing field -> empty object.
        assert_eq!(canonicalize_tool_arguments(None), "{}");
        // Empty string field -> empty object.
        assert_eq!(canonicalize_tool_arguments(Some(&json!(""))), "{}");
        // String field with JSON -> canonicalized.
        assert_eq!(
            canonicalize_tool_arguments(Some(&json!(r#"{"b":2,"a":1}"#))),
            r#"{"a":1,"b":2}"#
        );
        // Structured (non-string) field -> canonical serialization.
        assert_eq!(
            canonicalize_tool_arguments(Some(&json!({"b": 2, "a": 1}))),
            r#"{"a":1,"b":2}"#
        );
    }

    #[test]
    fn the_serde_serialization_of_a_canonical_value_matches_the_canonical_string() {
        // 两条路给同一个字节串：哈希拿哪一条算都一样。
        let v =
            json!({"z": [1, {"b": "二", "a": null}], "a": 1.5, "m": {"y": false, "x": "\"q\""}});
        assert_eq!(
            serde_json::to_string(&canonicalize_value(v.clone())).unwrap(),
            canonical_json_string(&v)
        );
    }

    #[test]
    fn preserve_order_is_on_for_the_whole_workspace() {
        // 开了 `preserve_order` 之后对象按插入顺序存。这条一旦红了，说明有人把那个特性关掉了 ——
        // 本机路由改请求体时会把整个请求的键按字母重排（结构化输出的属性顺序会变），
        // 先读本模块头和 DESIGN-NOTES 再决定。
        let v = json!({"b": 1, "a": 2});
        assert_eq!(serde_json::to_string(&v).unwrap(), r#"{"b":1,"a":2}"#);
    }
}
