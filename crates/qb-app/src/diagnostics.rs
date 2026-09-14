//! Explicit, bounded diagnostics. No automatic calls on query, focus or retries.
use crate::{
    domain::{Client, ProbeCheck, ProbeReport},
    error::{GateError, Result},
    repository::Repository,
    workspace,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Clone, Serialize, Deserialize, ts_rs::TS)]
pub struct ProbeRequest {
    #[serde(default)]
    pub auth_style: String,
    pub base_url: String,
    pub client: Client,
    pub credential_id: Option<String>,
    pub draft_key: Option<String>,
    pub model: String,
    pub wire_api: String,
    pub environment_id: Option<String>,
    pub revision: u32,
    pub test_call: bool,
    pub test_stream: bool,
    pub test_tools: bool,
}

fn check(name: &str, status: &str, detail: impl Into<String>, start: Instant) -> ProbeCheck {
    ProbeCheck {
        name: name.into(),
        status: status.into(),
        detail: detail.into(),
        elapsed_ms: start.elapsed().as_millis().min(u32::MAX as u128) as u32,
    }
}
fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .connect_timeout(std::time::Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(Into::into)
}
fn authenticated(
    request: reqwest::RequestBuilder,
    request_kind: Client,
    key: Option<&str>,
    auth_style: &str,
) -> reqwest::RequestBuilder {
    let request = if request_kind == Client::ClaudeCode {
        request.header("anthropic-version", "2023-06-01")
    } else {
        request
    };
    if let Some(key) = key.filter(|k| !k.is_empty() && auth_style != "none") {
        if request_kind == Client::ClaudeCode && auth_style != "bearer_token" {
            request.header("x-api-key", key)
        } else {
            request.bearer_auth(key)
        }
    } else {
        request
    }
}
fn request_body(r: &ProbeRequest, stream: bool, tools: bool) -> (String, serde_json::Value) {
    let prompt = if tools {
        "Call the qb_ping tool once with value ok."
    } else {
        "Reply with OK."
    };
    let (path, mut body) = if r.client == Client::ClaudeCode {
        (
            "messages",
            serde_json::json!({"model":r.model,"max_tokens":64,"messages":[{"role":"user","content":prompt}],"stream":stream}),
        )
    } else if r.wire_api == "chat" {
        (
            "chat/completions",
            serde_json::json!({"model":r.model,"max_tokens":64,"messages":[{"role":"user","content":prompt}],"stream":stream}),
        )
    } else {
        (
            "responses",
            serde_json::json!({"model":r.model,"max_output_tokens":64,"input":prompt,"stream":stream}),
        )
    };
    if tools {
        let schema = serde_json::json!({"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false});
        if r.client == Client::ClaudeCode {
            body["tools"] = serde_json::json!([{"name":"qb_ping","description":"Return a diagnostic marker","input_schema":schema}]);
            body["tool_choice"] = serde_json::json!({"type":"tool","name":"qb_ping"});
        } else if r.wire_api == "chat" {
            body["tools"] = serde_json::json!([{"type":"function","function":{"name":"qb_ping","description":"Return a diagnostic marker","parameters":schema}}]);
            body["tool_choice"] =
                serde_json::json!({"type":"function","function":{"name":"qb_ping"}});
        } else {
            body["tools"] = serde_json::json!([{"type":"function","name":"qb_ping","description":"Return a diagnostic marker","parameters":schema}]);
            body["tool_choice"] = serde_json::json!({"type":"function","name":"qb_ping"});
        }
    }
    (path.into(), body)
}

pub async fn run(r: ProbeRequest) -> Result<ProbeReport> {
    let base = crate::endpoint::endpoint_base(&r.base_url)?;
    let key = if r.auth_style == "none" {
        None
    } else if let Some(k) = r.draft_key.as_deref() {
        if k.is_empty() {
            None
        } else {
            Some(k.to_string())
        }
    } else {
        r.credential_id
            .as_deref()
            .map(|id| Repository::open()?.key(id))
            .transpose()?
    };
    let c = client()?;
    let start = Instant::now();
    let mut report = ProbeReport {
        id: crate::config_io::id(),
        environment_id: r.environment_id.clone(),
        revision: r.revision,
        checked_at: chrono::Utc::now().to_rfc3339(),
        checks: Vec::new(),
        models: Vec::new(),
        evidence: Vec::new(),
        request_export: String::new(),
    };
    let response = authenticated(
        c.get(workspace::endpoint(&base, "models")?),
        r.client,
        key.as_deref(),
        &r.auth_style,
    )
    .send()
    .await;
    match response {
        Err(e) => {
            report.checks.push(check(
                "网络",
                "failed",
                if e.is_timeout() {
                    "连接或响应超时"
                } else {
                    "连接失败，请检查端点与网络"
                },
                start,
            ));
            // 这一行必须先占位：下面几项实际调用要靠 `find(name == "认证")`
            // 把结论写回来。不占位的话，模型目录连不上、而实际调用返回 401
            // 的那种情况，认证结论会静默丢掉。
            report.checks.push(check(
                "认证",
                "unknown",
                "没能连上模型目录，这一步判断不了凭证",
                start,
            ));
        }
        Ok(response) => {
            let status = response.status();
            report.checks.push(check(
                "网络",
                "passed",
                format!("收到 HTTP {}", status.as_u16()),
                start,
            ));
            report.checks.push(check(
                "认证",
                if status.as_u16() == 401 || status.as_u16() == 403 {
                    "failed"
                } else {
                    "unknown"
                },
                if status.as_u16() == 401 {
                    "API 凭证未通过认证"
                } else if status.as_u16() == 403 {
                    "服务端拒绝此凭证或此来源"
                } else {
                    "模型目录的响应不能单独证明调用权限"
                },
                start,
            ));
            for (header, label) in [
                ("x-amzn-requestid", "AWS 响应头线索"),
                ("x-goog-request-id", "Google 响应头线索"),
                ("request-id", "请求追踪头线索"),
            ] {
                if response.headers().contains_key(header) {
                    report.evidence.push(format!(
                        "{label}；响应头可以被转发或修改，不能据此认证真实后端"
                    ));
                }
            }
            if status.is_success() {
                match bounded_json(response).await {
                    Ok(v) => {
                        if let Some(rows) = v
                            .get("data")
                            .or_else(|| v.get("models"))
                            .and_then(|v| v.as_array())
                        {
                            report.models = rows
                                .iter()
                                .filter_map(|m| {
                                    m.get("id")
                                        .or_else(|| m.get("name"))
                                        .and_then(|v| v.as_str())
                                        .map(String::from)
                                })
                                .collect();
                            report.checks.push(check(
                                "模型列表",
                                "passed",
                                format!("发现 {} 个模型", report.models.len()),
                                start,
                            ));
                        } else {
                            report.checks.push(check(
                                "模型列表",
                                "failed",
                                "响应没有可识别的模型数组",
                                start,
                            ));
                        }
                    }
                    Err(e) => report
                        .checks
                        .push(check("模型列表", "failed", e.to_string(), start)),
                }
            } else {
                report.checks.push(check(
                    "模型列表",
                    "failed",
                    format!("HTTP {}；未读取错误正文，避免记录凭证回显", status.as_u16()),
                    start,
                ));
            }
        }
    }
    // 真实调用顺手把响应痕迹留下来，循环结束后交给 `relay::backend::classify`。
    // 只取第一次 —— 三个勾选项打的是同一家，重复判定只会把证据列表刷屏。
    let mut traces: Option<crate::relay::backend::Traces> = None;
    for (name, enabled, stream, tools) in [
        ("实际模型调用", r.test_call, false, false),
        ("流式响应", r.test_stream, true, false),
        ("工具调用", r.test_tools, false, true),
    ] {
        if !enabled {
            continue;
        }
        let start = Instant::now();
        if r.model.trim().is_empty() {
            report
                .checks
                .push(check(name, "skipped", "请先选择模型", start));
            continue;
        }
        let (path, body) = request_body(&r, stream, tools);
        let url = workspace::endpoint(&base, &path)?;
        if report.request_export.is_empty() {
            report.request_export = export_request(&r, &url, &body)?;
        }
        let result = authenticated(
            c.post(url).json(&body),
            r.client,
            key.as_deref(),
            &r.auth_style,
        )
        .send()
        .await;
        match result {
            Err(e) => report.checks.push(check(
                name,
                "failed",
                if e.is_timeout() {
                    "请求超时"
                } else {
                    "连接中断"
                },
                start,
            )),
            Ok(response) => {
                let status = response.status();
                if traces.is_none() {
                    traces = Some(crate::relay::backend::Traces {
                        headers: response
                            .headers()
                            .iter()
                            .map(|(n, v)| {
                                (
                                    n.as_str().to_lowercase(),
                                    v.to_str().unwrap_or("").to_string(),
                                )
                            })
                            .collect(),
                        status: status.as_u16(),
                        ..Default::default()
                    });
                }
                if matches!(status.as_u16(), 401 | 403) {
                    if let Some(auth) = report.checks.iter_mut().find(|c| c.name == "认证") {
                        auth.status = "failed".into();
                        auth.detail = format!("实际调用返回 HTTP {}", status.as_u16());
                    }
                }
                if !status.is_success() {
                    report.checks.push(check(
                        name,
                        "failed",
                        format!("HTTP {}", status.as_u16()),
                        start,
                    ));
                    continue;
                }
                let validated = if stream {
                    validate_stream(response).await
                } else {
                    match bounded_json(response).await {
                        Err(e) => Err(e),
                        Ok(v) => {
                            if let Some(t) = traces.as_mut() {
                                if t.body_id.is_none() && t.body_model.is_none() {
                                    t.body_id =
                                        v.get("id").and_then(|x| x.as_str()).map(String::from);
                                    t.body_model =
                                        v.get("model").and_then(|x| x.as_str()).map(String::from);
                                }
                            }
                            validate_body(&v, tools)
                        }
                    }
                };
                match validated {
                    Ok(()) => {
                        report
                            .checks
                            .push(check(name, "passed", "协议响应验证通过", start));
                        if let Some(auth) = report.checks.iter_mut().find(|c| c.name == "认证") {
                            auth.status = "passed".into();
                            auth.detail = "所选模型实际调用成功".into();
                        }
                    }
                    Err(e) => report
                        .checks
                        .push(check(name, "failed", e.to_string(), start)),
                }
            }
        }
    }
    // 采到的痕迹交给纯函数判定。这是 `relay::backend::classify` 唯一的调用点：
    // 判定不再经过任何会把结论降级的中间层，`Strong` 就是 `Strong`。
    if let Some(t) = traces {
        let r = crate::relay::backend::classify(&t);
        report.evidence.push(r.detail);
        report.evidence.extend(r.evidence);
    }
    if report.evidence.is_empty() {
        report
            .evidence
            .push("没有足够的后端来源证据；这不代表服务伪造".into());
    }
    let db = Repository::open()?;
    db.put("diagnostics", &report.id, &report)?;
    db.prune_history()?;
    Ok(report)
}

async fn bounded_json(mut response: reqwest::Response) -> Result<serde_json::Value> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(GateError::Other("响应超过 2 MB 诊断上限".into()));
        }
    }
    serde_json::from_slice(&bytes).map_err(|_| GateError::Other("响应不是有效 JSON".into()))
}
/// 响应体本身合不合格。
///
/// 从 `run` 里那个闭包提出来的 —— 提出来才能让调用方先留下 `id` / `model`
/// 给后端判定用，再把同一个 body 交给它验。
fn validate_body(v: &serde_json::Value, tools: bool) -> Result<()> {
    if tools && !has_tool_call(v) {
        return Err(GateError::Other(
            "响应成功，但没有返回所请求的工具调用".into(),
        ));
    }
    if !tools && v.get("error").is_some() {
        return Err(GateError::Other("HTTP 成功，但响应体包含错误".into()));
    }
    if !tools
        && !(v.get("content").is_some() || v.get("choices").is_some() || v.get("output").is_some())
    {
        return Err(GateError::Other("未识别到模型输出".into()));
    }
    Ok(())
}

fn has_tool_call(v: &serde_json::Value) -> bool {
    v.get("content")
        .and_then(|v| v.as_array())
        .is_some_and(|a| {
            a.iter()
                .any(|v| v["type"] == "tool_use" && v["name"] == "qb_ping")
        })
        || v.get("output").and_then(|v| v.as_array()).is_some_and(|a| {
            a.iter()
                .any(|v| v["type"] == "function_call" && v["name"] == "qb_ping")
        })
        || v["choices"].as_array().is_some_and(|a| {
            a.iter().any(|v| {
                v["message"]["tool_calls"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v["function"]["name"] == "qb_ping"))
            })
        })
}
async fn validate_stream(mut response: reqwest::Response) -> Result<()> {
    if !response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.contains("text/event-stream"))
    {
        return Err(GateError::Other("响应没有使用 SSE 流协议".into()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > 1024 * 1024 {
            return Err(GateError::Other("流式诊断超过 1 MB 上限".into()));
        }
    }
    validate_stream_bytes(&bytes)
}
fn validate_stream_bytes(bytes: &[u8]) -> Result<()> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| GateError::Other("流式响应包含无效 UTF-8".into()))?
        .replace("\r\n", "\n");
    let mut completed = false;
    let mut output = false;
    for block in text.split("\n\n").filter(|s| !s.trim().is_empty()) {
        let payload = block
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if payload.is_empty() {
            continue;
        }
        if payload == "[DONE]" {
            completed = true;
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(&payload)
            .map_err(|_| GateError::Other("SSE 事件不是有效 JSON".into()))?;
        if value.get("error").is_some()
            || matches!(
                value["type"].as_str(),
                Some("error" | "response.failed" | "response.incomplete")
            )
        {
            return Err(GateError::Other("流式响应报告错误或未完成".into()));
        }
        if matches!(
            value["type"].as_str(),
            Some("message_stop" | "response.completed")
        ) {
            completed = true;
        }
        let nonempty = |v: &serde_json::Value| v.as_str().is_some_and(|s| !s.is_empty());
        if nonempty(&value["delta"]["text"])
            || nonempty(&value["delta"])
            || value["choices"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|r| nonempty(&r["delta"]["content"])))
        {
            output = true;
        }
    }
    if completed && output {
        Ok(())
    } else {
        Err(GateError::Other(
            "流已结束，但未收到输出与协议完成标记，可能中途断开".into(),
        ))
    }
}
fn export_request(r: &ProbeRequest, url: &str, body: &serde_json::Value) -> Result<String> {
    let mut headers = "  Content-Type: application/json\n".to_string();
    if r.client == Client::ClaudeCode {
        headers.push_str("  anthropic-version: 2023-06-01\n");
    }
    if r.auth_style != "none" {
        headers.push_str(
            if r.client == Client::ClaudeCode && r.auth_style != "bearer_token" {
                "  x-api-key: {{API_KEY}}\n"
            } else {
                "  Authorization: Bearer {{API_KEY}}\n"
            },
        );
    }
    Ok(format!("meta {{\n  name: QB Gate diagnostic\n  type: http\n  seq: 1\n}}\npost {{\n  url: {url}\n  body: json\n  auth: none\n}}\nheaders {{\n{headers}}}\nbody:json {{\n{}\n}}",serde_json::to_string_pretty(body)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stream_requires_output_completion_and_no_error() {
        assert!(validate_stream_bytes(b"data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"ok\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n").is_ok());
        for invalid in [
            "data: [DONE]\n\n",
            "data: {\"choices\":[]}\n\ndata: [DONE]\n\n",
            "data: {\"type\":\"content_block_delta\"}\n\n",
            "data: {broken}\n\n",
            "data: {\"type\":\"error\"}\n\ndata: [DONE]\n\n",
        ] {
            assert!(validate_stream_bytes(invalid.as_bytes()).is_err());
        }
    }
    #[test]
    fn auth_styles_and_endpoint_use_the_draft_input() {
        let client = client().unwrap();
        for (kind, style, header, value) in [
            (Client::ClaudeCode, "env_key", "x-api-key", "draft-key"),
            (
                Client::ClaudeCode,
                "bearer_token",
                "authorization",
                "Bearer draft-key",
            ),
            (
                Client::Codex,
                "env_key",
                "authorization",
                "Bearer draft-key",
            ),
        ] {
            let request = authenticated(
                client.get(workspace::endpoint("https://example.test/v1/", "models").unwrap()),
                kind,
                Some("draft-key"),
                style,
            )
            .build()
            .unwrap();
            assert_eq!(request.url().as_str(), "https://example.test/v1/models");
            assert_eq!(request.headers()[header], value);
        }
        let request = authenticated(
            client.get("https://example.test"),
            Client::Codex,
            Some("unused"),
            "none",
        )
        .build()
        .unwrap();
        assert!(!request.headers().contains_key("authorization"));
    }
    #[test]
    fn tool_success_requires_a_structured_call() {
        assert!(!has_tool_call(&serde_json::json!({"text":"qb_ping"})));
        assert!(has_tool_call(
            &serde_json::json!({"output":[{"type":"function_call","name":"qb_ping"}]})
        ));
    }
}
