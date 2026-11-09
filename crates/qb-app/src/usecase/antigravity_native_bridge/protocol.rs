//! Text-only Chat Completions ↔ Antigravity wire format, independently implemented in QB.
//! No prompts, tools, account rotation or model aliases are injected here.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct AntigravityBridgeModel {
    pub id: String,
    pub name: String,
}

pub fn models(body: &Value) -> Result<Vec<AntigravityBridgeModel>, String> {
    let entries = body["models"].as_object().ok_or("官方回复中没有模型清单")?;
    let mut out = Vec::new();
    for (id, info) in entries {
        let lower = id.to_ascii_lowercase();
        if !["gemini-", "claude-", "gpt-"]
            .iter()
            .any(|p| lower.starts_with(p))
            || ["image", "audio", "embedding", "tts"]
                .iter()
                .any(|p| lower.contains(p))
            || id.len() > 128
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
            || info.get("isInternal").and_then(Value::as_bool) == Some(true)
            || info.get("disabled").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        out.push(AntigravityBridgeModel {
            id: id.clone(),
            name: info["displayName"].as_str().unwrap_or(id).to_string(),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    if out.is_empty() {
        return Err("这个账户没有返回桥接支持的文本模型；请先在官方 IDE 检查模型权限".into());
    }
    Ok(out)
}

pub struct Prepared {
    pub stream: bool,
    pub include_usage: bool,
    pub request: Value,
}

/// Machine reasons from a closed allowlist; never return a server-supplied message or ID.
pub fn known_error_reason(value: &Value) -> Option<&'static str> {
    const KNOWN: &[&str] = &[
        "UNSUPPORTED_CLIENT",
        "SUBSCRIPTION_REQUIRED",
        "SERVICE_DISABLED",
        "ACCESS_TOKEN_SCOPE_INSUFFICIENT",
        "INSUFFICIENT_AUTHENTICATION_SCOPES",
        "RESOURCE_EXHAUSTED",
        "MODEL_NOT_FOUND",
        "VALIDATION_REQUIRED",
        "PERMISSION_DENIED",
        "UNAUTHENTICATED",
        "INVALID_ARGUMENT",
        "UNAVAILABLE",
    ];
    let error = value.get("error").unwrap_or(value);
    let reasons = error
        .get("details")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|detail| detail.get("reason").and_then(Value::as_str));
    reasons
        .chain(error.get("status").and_then(Value::as_str))
        .find_map(|code| KNOWN.iter().copied().find(|known| *known == code))
}

pub fn prepare(body: &[u8], model: &str, prompt: &str) -> Result<Prepared, String> {
    let v: Value = serde_json::from_slice(body).map_err(|_| "请求不是有效 JSON")?;
    if !v.is_object() {
        return Err("请求应为 JSON 对象".into());
    }
    if v.get("response_format")
        .is_some_and(|x| !x.is_null() && x.get("type").and_then(Value::as_str) != Some("text"))
    {
        return Err("原生反重力桥接只支持文本输出格式".into());
    }
    if let Some(want) = v.get("model") {
        if want.as_str() != Some(model) {
            return Err("模型与已启动的桥接不一致；请在酒馆选择 /models 返回的模型".into());
        }
    }
    for key in ["tools", "functions"] {
        if v.get(key)
            .is_some_and(|x| !x.is_null() && x.as_array().is_none_or(|a| !a.is_empty()))
        {
            return Err("原生反重力桥接只支持文本对话，不支持工具调用".into());
        }
    }
    for key in ["tool_choice", "function_call"] {
        if v.get(key)
            .is_some_and(|x| !x.is_null() && x.as_str() != Some("none"))
        {
            return Err("原生反重力桥接不支持工具调用".into());
        }
    }
    if v.get("n").is_some_and(|n| n.as_u64() != Some(1)) {
        return Err("原生反重力桥接每次只生成一个回复（n = 1）".into());
    }
    if v.get("modalities").is_some_and(|x| x != &json!(["text"]))
        || v.get("audio").is_some_and(|x| !x.is_null())
    {
        return Err("原生反重力桥接只支持文本".into());
    }
    let messages = v["messages"]
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or("messages 不能为空")?;
    let mut systems = Vec::new();
    if !prompt.trim().is_empty() {
        systems.push(prompt.trim().to_string());
    }
    let mut contents: Vec<Value> = Vec::new();
    for message in messages {
        if ["tool_calls", "function_call"].iter().any(|key| {
            message
                .get(key)
                .is_some_and(|x| !x.is_null() && x.as_array().is_none_or(|a| !a.is_empty()))
        }) {
            return Err("历史消息包含工具调用；请使用独立的纯文本聊天".into());
        }
        let text = match &message["content"] {
            Value::String(s) => s.clone(),
            Value::Array(parts) => {
                let mut text = Vec::new();
                for part in parts {
                    if part["type"].as_str() != Some("text") {
                        return Err("消息包含图片、音频或未知内容；桥接只接受文本".into());
                    }
                    text.push(part["text"].as_str().ok_or("text 内容应为字符串")?);
                }
                text.join("\n")
            }
            _ => return Err("消息 content 应为字符串或 text 分段数组".into()),
        };
        let role = match message["role"].as_str() {
            Some("system" | "developer") => {
                systems.push(text);
                continue;
            }
            Some("user") => "user",
            Some("assistant") => "model",
            _ => return Err("只支持 system、developer、user、assistant 文本消息".into()),
        };
        if let Some(last) = contents.last_mut().filter(|x| x["role"] == role) {
            last["parts"]
                .as_array_mut()
                .unwrap()
                .push(json!({"text": text}));
        } else {
            contents.push(json!({"role": role, "parts": [{"text": text}]}));
        }
    }
    if contents.is_empty() || !contents.iter().any(|c| c["role"] == "user") {
        return Err("至少需要一条 user 消息".into());
    }
    let mut request = json!({"contents": contents});
    if !systems.is_empty() {
        request["systemInstruction"] = json!({"parts": [{"text": systems.join("\n\n")}]});
    }
    let mut generation = serde_json::Map::new();
    for (input, output, max) in [("temperature", "temperature", 2.0), ("top_p", "topP", 1.0)] {
        if let Some(value) = v.get(input).filter(|x| !x.is_null()) {
            let n = value
                .as_f64()
                .filter(|n| *n >= 0.0 && *n <= max)
                .ok_or("采样参数超出支持范围")?;
            generation.insert(output.into(), json!(n));
        }
    }
    if let Some(value) = v
        .get("max_completion_tokens")
        .or_else(|| v.get("max_tokens"))
        .filter(|x| !x.is_null())
    {
        let n = value
            .as_u64()
            .filter(|n| *n > 0 && *n <= 1_048_576)
            .ok_or("输出 token 上限应为正整数")?;
        generation.insert("maxOutputTokens".into(), json!(n));
    }
    if let Some(stop) = v.get("stop").filter(|x| !x.is_null()) {
        let stops = match stop {
            Value::String(s) => vec![s.clone()],
            Value::Array(a) => a
                .iter()
                .map(|x| x.as_str().map(str::to_owned).ok_or("stop 应为字符串数组"))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err("stop 应为字符串或字符串数组".into()),
        };
        if stops.len() > 5 || stops.iter().any(|s| s.is_empty()) {
            return Err("stop 最多支持 5 条非空字符串".into());
        }
        generation.insert("stopSequences".into(), json!(stops));
    }
    request["generationConfig"] = Value::Object(generation);
    let stream = v
        .get("stream")
        .map(|x| x.as_bool().ok_or("stream 应为布尔值"))
        .transpose()?
        .unwrap_or(false);
    Ok(Prepared {
        stream,
        include_usage: v
            .pointer("/stream_options/include_usage")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        request,
    })
}

pub fn envelope(project: &str, model: &str, request: Value) -> Value {
    json!({
        "project": project, "model": model, "request": request,
        "requestId": format!("agent-{}", uuid::Uuid::new_v4()),
        "userAgent": "antigravity", "requestType": "agent"
    })
}

#[derive(Default)]
pub struct Decoder {
    buffer: String,
    remainder: Vec<u8>,
    pub text: String,
    pub finish: Option<&'static str>,
    pub usage: Value,
    bytes: usize,
}

impl Decoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<String>, String> {
        self.bytes = self.bytes.saturating_add(bytes.len());
        if self.bytes > 16 * 1024 * 1024 {
            return Err("官方回复超过桥接大小上限".into());
        }
        qb_proxy::sse_text::append_utf8_safe(&mut self.buffer, &mut self.remainder, bytes);
        let mut deltas = Vec::new();
        while let Some(block) = qb_proxy::sse_text::take_sse_block(&mut self.buffer) {
            self.block(&block, &mut deltas)?;
        }
        Ok(deltas)
    }

    fn block(&mut self, block: &str, out: &mut Vec<String>) -> Result<(), String> {
        let data = block
            .lines()
            .filter_map(|l| qb_proxy::sse_text::strip_sse_field(l, "data"))
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            return Ok(());
        }
        let v: Value = serde_json::from_str(&data).map_err(|_| "官方流式回复包含无效 JSON")?;
        let v = v.get("response").unwrap_or(&v);
        if v.get("error").is_some() {
            let code = known_error_reason(v)
                .map(|c| format!("（{c}）"))
                .unwrap_or_default();
            return Err(format!("官方流式回复返回错误{code}；未切换账户或线路"));
        }
        if v.pointer("/promptFeedback/blockReason").is_some() {
            self.finish = Some("content_filter");
        }
        if let Some(usage) = v.get("usageMetadata") {
            let input = usage["promptTokenCount"].as_u64().unwrap_or(0);
            let output = usage["candidatesTokenCount"].as_u64().unwrap_or(0);
            let reasoning = usage["thoughtsTokenCount"].as_u64().unwrap_or(0);
            let completion = output.saturating_add(reasoning);
            self.usage = json!({
                "prompt_tokens": input, "completion_tokens": completion,
                "total_tokens": usage["totalTokenCount"].as_u64().unwrap_or(input.saturating_add(completion)),
                "prompt_tokens_details": {"cached_tokens": usage["cachedContentTokenCount"].as_u64().unwrap_or(0)},
                "completion_tokens_details": {"reasoning_tokens": reasoning}
            });
        }
        if let Some(candidates) = v.get("candidates").and_then(Value::as_array) {
            if candidates.len() > 1 {
                return Err("官方回复包含多个候选，桥接无法安全合并".into());
            }
            for candidate in candidates {
                if let Some(parts) = candidate
                    .pointer("/content/parts")
                    .and_then(Value::as_array)
                {
                    for part in parts {
                        if part.get("functionCall").is_some()
                            || part.get("inlineData").is_some()
                            || part.get("fileData").is_some()
                        {
                            return Err("官方返回了非文本内容；此桥接不执行工具或输出媒体".into());
                        }
                        if part["thought"].as_bool() == Some(true) {
                            continue;
                        }
                        if let Some(text) = part
                            .get("text")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                        {
                            self.text.push_str(text);
                            out.push(text.to_string());
                        } else if !part.is_object()
                            || part.as_object().is_some_and(|m| {
                                m.keys().any(|k| k != "thoughtSignature" && k != "text")
                            })
                        {
                            return Err("官方返回了未知内容类型，桥接未把它当作成功文本".into());
                        }
                    }
                }
                if let Some(reason) = candidate.get("finishReason").and_then(Value::as_str) {
                    self.finish = Some(match reason {
                        "STOP" => "stop",
                        "MAX_TOKENS" => "length",
                        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => {
                            "content_filter"
                        }
                        _ => return Err("官方没有正常完成回复，请在官方客户端检查此模型".into()),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn end(&mut self) -> Result<Vec<String>, String> {
        if !self.remainder.is_empty() {
            return Err("官方流在 UTF-8 字符中间断开".into());
        }
        let tail = std::mem::take(&mut self.buffer);
        let mut out = Vec::new();
        if !tail.trim().is_empty() {
            self.block(&tail, &mut out)?;
        }
        if self.finish.is_none() || (self.text.is_empty() && self.finish != Some("content_filter"))
        {
            return Err("官方流提前结束，没有完整的文本回复；未重试或切换账户".into());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_request_preserves_roleplay_and_controls_without_tools() {
        let body = json!({"model":"claude-example", "messages":[
            {"role":"system","content":"角色卡"}, {"role":"user","content":[{"type":"text","text":"继续"}]},
            {"role":"assistant","content":"先前回复"}, {"role":"user","content":"下一轮"}],
            "stream":true, "temperature":0.7, "max_tokens":500, "stop":["END"]});
        let r = prepare(body.to_string().as_bytes(), "claude-example", "独立提示").unwrap();
        assert!(r.stream);
        assert_eq!(
            r.request["systemInstruction"]["parts"][0]["text"],
            "独立提示\n\n角色卡"
        );
        assert_eq!(r.request["contents"][1]["role"], "model");
        assert_eq!(r.request["generationConfig"]["maxOutputTokens"], 500);
        assert!(r.request.get("tools").is_none());
    }

    #[test]
    fn unsafe_or_unknown_requests_never_silently_lose_content() {
        for message in [
            json!({"role":"tool","content":"result"}),
            json!({"role":"user","content":[{"type":"image_url","image_url":{"url":"https://example.test/private"}}]}),
            json!({"role":"assistant","content":"x","tool_calls":[{"id":"1"}]}),
            json!({"role":"user","content":{"text":"unknown"}}),
        ] {
            assert!(prepare(
                json!({"messages":[message]}).to_string().as_bytes(),
                "gemini-test",
                ""
            )
            .is_err());
        }
        for extra in [
            json!({"tools":[{}]}),
            json!({"n":2}),
            json!({"model":"other"}),
            json!({"stream":"yes"}),
        ] {
            let mut body = json!({"messages":[{"role":"user","content":"hello"}]});
            body.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            assert!(prepare(body.to_string().as_bytes(), "gemini-test", "").is_err());
        }
    }

    #[test]
    fn model_discovery_never_invents_or_exposes_internal_media_models() {
        let list = models(&json!({"models":{
            "gemini-real":{"displayName":"Gemini Real"}, "claude-real":{},
            "gemini-image":{}, "chat_internal":{}, "gpt-hidden":{"isInternal":true}
        }}))
        .unwrap();
        assert_eq!(
            list.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["claude-real", "gemini-real"]
        );
        assert!(models(&json!({"models":{}})).is_err());
    }

    #[test]
    fn fragmented_stream_omits_thoughts_preserves_utf8_and_usage() {
        let s = concat!(
            "data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"秘密\",\"thought\":true},{\"text\":\"你好\"}]}}]}}\r\n\r\n",
            "data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"世界\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":10,\"candidatesTokenCount\":4,\"thoughtsTokenCount\":2,\"cachedContentTokenCount\":3}}}\n\n");
        let mut d = Decoder::default();
        let mut chunks = Vec::new();
        for byte in s.as_bytes() {
            chunks.extend(d.feed(&[*byte]).unwrap());
        }
        chunks.extend(d.end().unwrap());
        assert_eq!(chunks.concat(), "你好世界");
        assert_eq!(d.text, "你好世界");
        assert_eq!(d.usage["completion_tokens"], 6);
        assert_eq!(d.usage["total_tokens"], 16);
        assert_eq!(d.usage["prompt_tokens_details"]["cached_tokens"], 3);
    }

    #[test]
    fn upstream_failure_is_not_a_successful_empty_completion_or_secret_echo() {
        let mut d = Decoder::default();
        d.feed(b"data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"partial\"}]}}]}}\n\n").unwrap();
        assert!(d.end().is_err());
        let error = Decoder::default()
            .feed(b"data: {\"error\":{\"message\":\"Bearer private-token\"}}\n\n")
            .unwrap_err();
        assert!(!error.contains("private-token"));
        assert!(Decoder::default()
            .feed(b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{}}]}}]}\n\n")
            .is_err());
    }

    #[test]
    fn known_error_codes_are_useful_without_echoing_arbitrary_reasons() {
        assert_eq!(
            known_error_reason(
                &json!({"error":{"status":"PERMISSION_DENIED","details":[{"reason":"UNSUPPORTED_CLIENT"}]}})
            ),
            Some("UNSUPPORTED_CLIENT")
        );
        assert_eq!(
            known_error_reason(
                &json!({"error":{"status":"secret-token","details":[{"reason":"account-private"}],"message":"private content"}})
            ),
            None
        );
    }
}
