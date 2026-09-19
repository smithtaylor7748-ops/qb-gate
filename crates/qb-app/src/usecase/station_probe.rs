//! A bounded controlled request; only output deltas count as first-token latency.
use super::station_billing;
use qb_contract::domain::Client;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

#[derive(Default, Debug, Clone)]
pub struct Probe {
    pub request_ids: Vec<String>,
    pub first_token_ms: Option<f64>,
    pub tokens: [Option<u64>; 4],
    pub input_total: Option<u64>,
    pub automatic_cache: bool,
}

fn update(probe: &mut Probe, value: &Value, elapsed: f64) -> bool {
    let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
    let text = match kind {
        "response.output_text.delta" => value.get("delta").and_then(Value::as_str),
        "content_block_delta" => value.pointer("/delta/text").and_then(Value::as_str),
        _ => None,
    };
    if text.is_some_and(|s| !s.is_empty()) && probe.first_token_ms.is_none() {
        probe.first_token_ms = Some(elapsed);
    }
    let response = value
        .get("response")
        .or_else(|| value.get("message"))
        .unwrap_or(value);
    if let Some(id) = response.get("id").and_then(Value::as_str) {
        if !probe.request_ids.iter().any(|v| v == id) {
            probe.request_ids.push(id.into());
        }
    }
    if let Some(usage) = response.get("usage") {
        let input = usage.get("input_tokens").and_then(Value::as_u64);
        let read = usage
            .pointer("/input_tokens_details/cached_tokens")
            .or_else(|| usage.get("cache_read_input_tokens"))
            .and_then(Value::as_u64);
        let write = usage
            .pointer("/input_tokens_details/cache_write_tokens")
            .or_else(|| usage.pointer("/input_tokens_details/cache_creation_tokens"))
            .or_else(|| usage.get("cache_write_tokens"))
            .or_else(|| usage.get("cache_creation_input_tokens"))
            .and_then(Value::as_u64);
        let is_responses = kind.starts_with("response.");
        if let Some(input) = input {
            if is_responses {
                probe.input_total = Some(input);
            }
            probe.tokens[0] = if is_responses {
                read.zip(write)
                    .and_then(|(r, w)| input.checked_sub(r)?.checked_sub(w))
            } else {
                Some(input)
            };
        }
        if read.is_some() {
            probe.tokens[1] = read;
        }
        if write.is_some() {
            probe.tokens[2] = write;
        }
        if let Some(output) = usage.get("output_tokens").and_then(Value::as_u64) {
            probe.tokens[3] = Some(output);
        }
    }
    if !kind.starts_with("response.") {
        probe.input_total = probe.tokens[0]
            .zip(probe.tokens[1])
            .zip(probe.tokens[2])
            .and_then(|((i, r), w)| i.checked_add(r)?.checked_add(w));
    }
    kind == "response.completed" || kind == "message_stop"
}

impl Probe {
    /// Missing cache-write metering is preserved, but does not stop the six-call
    /// cache experiment when total input, cached input and output are observed.
    pub fn usage_complete(&self) -> bool {
        self.input_total
            .zip(self.tokens[1])
            .is_some_and(|(total, read)| {
                total
                    .checked_sub(read)
                    .is_some_and(|remaining| self.tokens[2].is_none_or(|w| w <= remaining))
            })
            && self.tokens[3].is_some()
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    kind: Client,
    base_url: &str,
    key: &str,
    model: &str,
    client: &reqwest::Client,
    prefix: &str,
    question: &str,
    explicit: bool,
) -> Result<Probe, String> {
    let codex = kind == Client::Codex;
    let base = base_url.trim_end_matches('/').trim_end_matches("/v1");
    let mut body = if codex {
        json!({"model":model,"input":[{"role":"developer","content":[{"type":"input_text","text":prefix}]},{"role":"user","content":[{"type":"input_text","text":question}]}],"max_output_tokens":112,"stream":true,"store":false})
    } else {
        json!({"model":model,"system":[{"type":"text","text":prefix,"cache_control":{"type":"ephemeral"}}],"messages":[{"role":"user","content":question}],"max_tokens":112,"stream":true})
    };
    if codex && model.starts_with("gpt-5") {
        body["reasoning"] = json!({"effort":"low"});
    }
    if codex && explicit {
        use sha2::{Digest, Sha256};
        body["input"][0]["content"][0]["prompt_cache_breakpoint"] = json!({"mode":"explicit"});
        body["prompt_cache_options"] = json!({"mode":"explicit"});
        body["prompt_cache_key"] = json!(format!(
            "review-material-{}",
            &hex::encode(Sha256::digest(prefix))[..24]
        ));
    }
    let client_id = crate::config_io::id();
    let mut automatic_cache = !explicit;
    let make_request = |body: &Value| {
        let request = client
            .post(format!(
                "{base}/v1/{}",
                if codex { "responses" } else { "messages" }
            ))
            .timeout(Duration::from_secs(60))
            .header("accept", "text/event-stream")
            .header("x-client-request-id", &client_id)
            .header("connection", "close")
            .json(body);
        if codex {
            request.bearer_auth(key)
        } else {
            request
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
        }
    };
    let started = Instant::now();
    let mut response = make_request(&body)
        .send()
        .await
        .map_err(|_| "模型请求失败或超时；未自动重试，可能已计费".to_string())?;
    if explicit && matches!(response.status().as_u16(), 400 | 422) {
        let status = response.status().as_u16();
        // A rejected schema is the only retry. Transport failures and 5xx are never replayed.
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "显式缓存参数检查失败")?
        {
            if bytes.len() + chunk.len() > 65536 {
                return Err("站点错误响应过大，未重试".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let rejected = String::from_utf8_lossy(&bytes).to_lowercase();
        if rejected.contains("prompt_cache")
            && [
                "unsupported",
                "unknown",
                "unrecognized",
                "not supported",
                "not allowed",
            ]
            .iter()
            .any(|s| rejected.contains(s))
        {
            body["input"][0]["content"][0]
                .as_object_mut()
                .unwrap()
                .remove("prompt_cache_breakpoint");
            body.as_object_mut().unwrap().remove("prompt_cache_options");
            body.as_object_mut().unwrap().remove("prompt_cache_key");
            automatic_cache = true;
            response = make_request(&body)
                .send()
                .await
                .map_err(|_| "自动缓存请求失败，未再次重试")?;
        } else {
            return Err(station_billing::http_problem(status, false, "模型请求"));
        }
    }
    if !response.status().is_success() {
        return Err(station_billing::http_problem(
            response.status().as_u16(),
            response
                .headers()
                .get("server")
                .is_some_and(|s| s.as_bytes().eq_ignore_ascii_case(b"cloudflare")),
            "模型请求",
        ));
    }
    let mut probe = Probe {
        request_ids: vec![client_id],
        automatic_cache,
        ..Default::default()
    };
    for name in ["x-request-id", "openai-request-id", "request-id"] {
        if let Some(id) = response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.is_empty())
        {
            probe.request_ids.push(id.into());
        }
    }
    let mut buffer = Vec::new();
    let mut received = 0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "流式响应中断；未自动重试，可能已计费".to_string())?
    {
        received += chunk.len();
        if received > 2 * 1024 * 1024 {
            return Err("模型响应超出本次小额检验上限".into());
        }
        buffer.extend_from_slice(&chunk);
        while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = buffer.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let Some(data) = line.trim().strip_prefix("data:") else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(data.trim()) else {
                continue;
            };
            if matches!(
                value.get("type").and_then(Value::as_str),
                Some("error" | "response.failed" | "response.incomplete")
            ) {
                return Err("模型没有完成检验请求；未重放，可能已计费".into());
            }
            if update(&mut probe, &value, started.elapsed().as_secs_f64() * 1000.0) {
                return Ok(probe);
            }
        }
    }
    Err("未收到流式完成事件；不能把连接成功当成模型回复".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_cache_writes_are_not_counted_as_uncached_input() {
        let mut p = Probe::default();
        update(
            &mut p,
            &json!({"type":"response.completed","response":{"usage":{"input_tokens":6000,"input_tokens_details":{"cached_tokens":500,"cache_creation_tokens":5000},"output_tokens":8}}}),
            80.0,
        );
        assert_eq!(p.tokens, [Some(500), Some(500), Some(5000), Some(8)]);
        assert!(p.usage_complete());
    }
    #[test]
    fn missing_write_is_unknown_but_total_usage_can_support_cache_validation() {
        let mut p = Probe::default();
        update(
            &mut p,
            &json!({"type":"response.completed","response":{"usage":{"input_tokens":6000,"input_tokens_details":{"cached_tokens":5000},"output_tokens":8}}}),
            80.0,
        );
        assert_eq!(p.tokens, [None, Some(5000), None, Some(8)]);
        assert_eq!(p.input_total, Some(6000));
        assert!(p.usage_complete());
    }
    #[test]
    fn metadata_is_not_a_token_and_usage_does_not_double_count_cache() {
        let mut p = Probe::default();
        update(&mut p, &json!({"type":"response.created"}), 10.0);
        assert_eq!(p.first_token_ms, None);
        update(
            &mut p,
            &json!({"type":"response.output_text.delta","delta":""}),
            20.0,
        );
        assert_eq!(p.first_token_ms, None);
        update(
            &mut p,
            &json!({"type":"response.output_text.delta","delta":"OK"}),
            30.0,
        );
        assert!(update(
            &mut p,
            &json!({"type":"response.completed","response":{"usage":{"input_tokens":1000,"input_tokens_details":{"cached_tokens":900,"cache_write_tokens":0},"output_tokens":2}}}),
            40.0
        ));
        assert_eq!(p.first_token_ms, Some(30.0));
        assert_eq!(p.tokens, [Some(100), Some(900), Some(0), Some(2)]);
    }
}
