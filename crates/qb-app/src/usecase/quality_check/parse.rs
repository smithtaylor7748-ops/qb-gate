//! 有界解析器；成功必须有协议终态，半句文本和 EOF 不算成功。
use super::*;
use serde_json::Value;

pub(super) const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_TEXT: usize = 128 * 1024;
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Protocol {
    Cli,
    Responses,
    Chat,
}
pub(super) struct Decoder {
    protocol: Protocol,
    streaming: bool,
    pending: Vec<u8>,
    data: String,
    total: usize,
    terminal: bool,
}
fn fail(message: &str) -> GateError {
    GateError::Other(message.into())
}
impl Decoder {
    pub fn new(protocol: Protocol, streaming: bool) -> Self {
        Self {
            protocol,
            streaming,
            pending: Vec::new(),
            data: String::new(),
            total: 0,
            terminal: false,
        }
    }
    pub fn push(
        &mut self,
        bytes: &[u8],
        result: &mut QualityCheckResult,
        elapsed: u64,
    ) -> Result<()> {
        self.total = self.total.saturating_add(bytes.len());
        if self.total > MAX_BYTES {
            return Err(fail("检测响应超过 2 MiB，已停止读取"));
        }
        self.pending.extend_from_slice(bytes);
        if !self.streaming {
            return Ok(());
        }
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = self.pending.drain(..=end).collect();
            self.line(&line, result, elapsed)?;
        }
        Ok(())
    }
    fn line(&mut self, bytes: &[u8], result: &mut QualityCheckResult, elapsed: u64) -> Result<()> {
        let line = std::str::from_utf8(bytes)
            .map_err(|_| fail("检测响应包含无效 UTF-8"))?
            .trim_end_matches(['\r', '\n']);
        if self.protocol == Protocol::Cli {
            if !line.trim().is_empty() {
                self.event(line, result, elapsed)?;
            }
        } else if line.is_empty() {
            if !self.data.is_empty() {
                let data = std::mem::take(&mut self.data);
                self.event(data.trim_end_matches('\n'), result, elapsed)?;
            }
        } else if let Some(data) = line.strip_prefix("data:") {
            self.data.push_str(data.strip_prefix(' ').unwrap_or(data));
            self.data.push('\n');
        }
        Ok(())
    }
    pub fn finish(&mut self, result: &mut QualityCheckResult, elapsed: u64) -> Result<()> {
        if !self.pending.is_empty() {
            let remaining = std::mem::take(&mut self.pending);
            if self.streaming {
                self.line(&remaining, result, elapsed)?;
            } else {
                let value: Value = serde_json::from_slice(&remaining)
                    .map_err(|_| fail("检测返回的不是有效 JSON"))?;
                self.whole(&value, result, elapsed)?;
            }
        }
        if !self.data.is_empty() {
            let data = std::mem::take(&mut self.data);
            self.event(data.trim_end_matches('\n'), result, elapsed)?;
        }
        if !self.terminal {
            return Err(fail("检测响应在完整成功终态之前中断"));
        }
        if result.text.trim().is_empty() {
            return Err(fail("检测已结束，但没有可评估的文本"));
        }
        Ok(())
    }
    fn event(&mut self, data: &str, result: &mut QualityCheckResult, elapsed: u64) -> Result<()> {
        if data == "[DONE]" {
            return Ok(());
        }
        let value: Value = serde_json::from_str(data).map_err(|_| fail("检测事件不是有效 JSON"))?;
        if value.get("error").is_some_and(|e| !e.is_null()) {
            return Err(fail("上游返回检测错误；请检查所选模型、推理强度和账户额度"));
        }
        let kind = value["type"].as_str().unwrap_or("");
        match self.protocol {
            Protocol::Cli => match kind {
                "turn.completed" => {
                    self.terminal = true;
                    usage(&value["usage"], result, Protocol::Cli);
                }
                "turn.failed" | "error" => {
                    return Err(fail("Codex 检测回合失败；请检查模型、推理强度和账户状态"))
                }
                "item.started" | "item.updated" | "item.completed" => {
                    let item = &value["item"];
                    match item["type"].as_str().unwrap_or("") {
                        "agent_message" => {
                            let text = item["text"].as_str().unwrap_or("");
                            if kind == "item.completed" {
                                if text.len() > MAX_TEXT {
                                    return Err(fail("检测文本超过 128 KiB，已停止本次检测"));
                                }
                                result.text = text.to_owned();
                                // codex exec JSONL 在整条消息完成后才输出文本，无法测首 token。
                                result.first_token_ms = None;
                            }
                        }
                        // 官方 JSONL 也把配置弃用警告作为 item.error；完整 turn.failed/退出码
                        // 才决定任务失败，警告原文不进入检测文本或报告。
                        "reasoning" => {}
                        "error" => {
                            if item["message"]
                                .as_str()
                                .is_some_and(|m| m.to_ascii_lowercase().contains("model rerouted"))
                            {
                                return Err(fail("官方 CLI 已改用其它模型，本次无法按原模型计分"));
                            }
                        }
                        _ => {
                            return Err(fail(
                                "检测期间出现工具或非文本操作，已结束专属 Codex 进程树",
                            ))
                        }
                    }
                }
                _ => {}
            },
            Protocol::Responses => match kind {
                "response.output_text.delta" => {
                    append(result, value["delta"].as_str().unwrap_or(""), elapsed)?;
                }
                "response.output_item.added" | "response.output_item.done" => {
                    reject_tool(&value["item"])?
                }
                "response.completed" => {
                    self.responses(&value["response"], result, elapsed)?;
                }
                "response.failed" | "response.incomplete" | "error" => {
                    return Err(fail("Responses 检测未完整完成；可能达到输出上限或上游失败"))
                }
                _ => {
                    if let Some(model) = value["response"]["model"].as_str() {
                        result.actual_model = Some(model.into());
                    }
                }
            },
            Protocol::Chat => self.chat(&value, result, elapsed, true)?,
        }
        Ok(())
    }
    fn whole(
        &mut self,
        value: &Value,
        result: &mut QualityCheckResult,
        elapsed: u64,
    ) -> Result<()> {
        if value.get("error").is_some_and(|e| !e.is_null()) {
            return Err(fail("上游返回检测错误；请检查模型、推理强度和账户额度"));
        }
        match self.protocol {
            Protocol::Responses => self.responses(value, result, elapsed),
            Protocol::Chat => self.chat(value, result, elapsed, false),
            Protocol::Cli => Err(fail("CLI 检测必须返回 JSONL 事件")),
        }
    }
    fn responses(
        &mut self,
        value: &Value,
        result: &mut QualityCheckResult,
        elapsed: u64,
    ) -> Result<()> {
        usage(&value["usage"], result, Protocol::Responses);
        if let Some(model) = value["model"].as_str() {
            result.actual_model = Some(model.into());
        }
        let mut full = String::new();
        if let Some(items) = value["output"].as_array() {
            for item in items {
                reject_tool(item)?;
            }
            let selected = items
                .iter()
                .rev()
                .find(|item| item["type"] == "message" && item["phase"] == "final_answer")
                .or_else(|| {
                    items.iter().rev().find(|item| {
                        item["type"] == "message" && item.get("phase").is_none_or(Value::is_null)
                    })
                });
            if let Some(content) = selected.and_then(|item| item["content"].as_array()) {
                for part in content {
                    if part["type"] == "output_text" {
                        full.push_str(part["text"].as_str().unwrap_or(""));
                    }
                }
            }
            // 完整输出数组存在时，它才是最终答案；不能把此前 commentary 增量冒充最终答案。
            if !items.is_empty() && full.is_empty() {
                result.text.clear();
            }
        }
        if !full.is_empty() {
            let measured = result.first_token_ms;
            result.text.clear();
            append(result, &full, elapsed)?;
            result.first_token_ms = measured;
        }
        if value["status"] != "completed" {
            return Err(fail("Responses 没有返回 completed 成功终态"));
        }
        self.terminal = true;
        Ok(())
    }
    fn chat(
        &mut self,
        value: &Value,
        result: &mut QualityCheckResult,
        elapsed: u64,
        streaming: bool,
    ) -> Result<()> {
        usage(&value["usage"], result, Protocol::Chat);
        if let Some(model) = value["model"].as_str() {
            result.actual_model = Some(model.into());
        }
        if let Some(choice) = value["choices"]
            .as_array()
            .and_then(|xs| xs.iter().find(|c| c["index"].as_u64().unwrap_or(0) == 0))
        {
            let content = if streaming {
                &choice["delta"]
            } else {
                &choice["message"]
            };
            if content
                .get("tool_calls")
                .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
                || content.get("function_call").is_some_and(|v| !v.is_null())
            {
                return Err(fail("检测响应要求执行工具，已停止本次检测"));
            }
            append(result, content["content"].as_str().unwrap_or(""), elapsed)?;
            if !streaming {
                result.first_token_ms = None;
            }
            if let Some(reason) = choice["finish_reason"].as_str() {
                if reason != "stop" {
                    return Err(fail("Chat 检测未自然完成（输出上限、过滤或工具调用）"));
                }
                self.terminal = true;
            }
        }
        Ok(())
    }
}
fn reject_tool(item: &Value) -> Result<()> {
    match item["type"].as_str() {
        Some("message" | "reasoning") | None => Ok(()),
        _ => Err(fail("检测响应包含工具调用，已停止本次检测")),
    }
}
fn append(result: &mut QualityCheckResult, text: &str, elapsed: u64) -> Result<()> {
    if result.text.len().saturating_add(text.len()) > MAX_TEXT {
        return Err(fail("检测文本超过 128 KiB，已停止本次检测"));
    }
    if !text.is_empty() {
        result.first_token_ms.get_or_insert(elapsed);
        result.text.push_str(text);
    }
    Ok(())
}
fn usage(value: &Value, result: &mut QualityCheckResult, protocol: Protocol) {
    if !value.is_object() {
        return;
    }
    let chat = protocol == Protocol::Chat;
    result.input_tokens = value[if chat {
        "prompt_tokens"
    } else {
        "input_tokens"
    }]
    .as_u64()
    .or(result.input_tokens);
    result.output_tokens = value[if chat {
        "completion_tokens"
    } else {
        "output_tokens"
    }]
    .as_u64()
    .or(result.output_tokens);
    result.cache_read_tokens = value[if chat {
        "prompt_tokens_details"
    } else {
        "input_tokens_details"
    }]["cached_tokens"]
        .as_u64()
        .or_else(|| value["cached_input_tokens"].as_u64())
        .or(result.cache_read_tokens);
    result.reasoning_tokens = value[if chat {
        "completion_tokens_details"
    } else {
        "output_tokens_details"
    }]["reasoning_tokens"]
        .as_u64()
        .or_else(|| value["reasoning_output_tokens"].as_u64())
        .or(result.reasoning_tokens);
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn result() -> QualityCheckResult {
        QualityCheckResult::blank(&QualityCheckRequest {
            run_id: uuid::Uuid::new_v4().to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
            target: QualityTarget {
                kind: QualityTargetKind::Relay,
                id: "test".into(),
            },
            model: "asked".into(),
            effort: "high".into(),
            prompt: "test".into(),
            max_output_tokens: 100,
            timeout_secs: 30,
        })
    }
    #[test]
    fn partial_text_is_not_success_and_unknown_usage_stays_unknown() {
        let mut result = result();
        let mut parser = Decoder::new(Protocol::Responses, true);
        parser
            .push(
                b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n",
                &mut result,
                10,
            )
            .unwrap();
        assert!(parser.finish(&mut result, 20).is_err());
        assert_eq!(result.input_tokens, None);
        assert_eq!(result.first_token_ms, Some(10));
    }
    #[test]
    fn utf8_chunk_boundaries_and_response_terminal_usage_are_preserved() {
        let mut result = result();
        let mut parser = Decoder::new(Protocol::Responses, true);
        let wire = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"你好\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"model\":\"declared\",\"usage\":{\"input_tokens\":4,\"output_tokens\":2,\"output_tokens_details\":{\"reasoning_tokens\":1}}}}\n\n";
        for b in wire.as_bytes() {
            parser.push(&[*b], &mut result, 5).unwrap();
        }
        parser.finish(&mut result, 9).unwrap();
        assert_eq!(result.text, "你好");
        assert_eq!(result.actual_model.as_deref(), Some("declared"));
        assert_eq!(result.reasoning_tokens, Some(1));
        assert_eq!(result.cache_read_tokens, None);
    }
    #[test]
    fn cli_requires_turn_completed_and_rejects_tools_and_failures() {
        let mut result = result();
        let mut parser = Decoder::new(Protocol::Cli, true);
        parser.push(b"{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"yes\"}}\n", &mut result, 12).unwrap();
        assert!(parser.finish(&mut result, 13).is_err());
        parser.push(b"{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":9,\"cached_input_tokens\":3,\"output_tokens\":1}}\n", &mut result, 15).unwrap();
        parser.finish(&mut result, 16).unwrap();
        assert_eq!(result.cache_read_tokens, Some(3));
        for event in [
            b"{\"type\":\"item.started\",\"item\":{\"type\":\"command_execution\"}}\n".as_slice(),
            b"{\"type\":\"turn.failed\"}\n",
        ] {
            assert!(parser.push(event, &mut result, 17).is_err());
        }
    }
    #[test]
    fn chat_done_without_finish_and_length_are_not_success() {
        let mut result = result();
        let mut parser = Decoder::new(Protocol::Chat, true);
        parser.push(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"a\"}}]}\n\ndata: [DONE]\n\n", &mut result, 3).unwrap();
        assert!(parser.finish(&mut result, 4).is_err());
        assert!(parser
            .push(
                b"data: {\"choices\":[{\"index\":0,\"finish_reason\":\"length\"}]}\n\n",
                &mut result,
                4
            )
            .is_err());
    }
    #[test]
    fn cli_uses_final_message_ignores_nonfatal_warnings_and_keeps_reasoning_usage() {
        let mut result = result();
        let mut parser = Decoder::new(Protocol::Cli, true);
        for event in [
            serde_json::json!({"type":"item.completed","item":{"type":"agent_message","text":"I will calculate"}}),
            serde_json::json!({"type":"item.completed","item":{"type":"error","message":"deprecated config"}}),
            serde_json::json!({"type":"item.completed","item":{"type":"agent_message","text":"{\"answer\":21}"}}),
            serde_json::json!({"type":"turn.completed","usage":{"input_tokens":40,"output_tokens":10,"reasoning_output_tokens":7}}),
        ] {
            parser
                .push(format!("{event}\n").as_bytes(), &mut result, 8)
                .unwrap();
        }
        parser.finish(&mut result, 12).unwrap();
        assert_eq!(result.text, "{\"answer\":21}");
        assert_eq!(result.reasoning_tokens, Some(7));
        assert_eq!(result.first_token_ms, None);
        let rerouted = serde_json::json!({"type":"item.completed","item":{"type":"error","message":"model rerouted: unavailable -> fallback"}});
        assert!(parser
            .push(format!("{rerouted}\n").as_bytes(), &mut result, 20)
            .is_err());
    }
    #[test]
    fn responses_uses_final_answer_and_nonstreaming_has_no_fake_first_token_time() {
        let value = serde_json::json!({"status":"completed","output":[
            {"type":"message","phase":"commentary","content":[{"type":"output_text","text":"calculating"}]},
            {"type":"message","phase":"final_answer","content":[{"type":"output_text","text":"{\"answer\":21}"}]}
        ]});
        let mut result = result();
        let mut parser = Decoder::new(Protocol::Responses, false);
        parser
            .push(value.to_string().as_bytes(), &mut result, 10)
            .unwrap();
        parser.finish(&mut result, 30).unwrap();
        assert_eq!(result.text, "{\"answer\":21}");
        assert_eq!(result.first_token_ms, None);
    }
}
