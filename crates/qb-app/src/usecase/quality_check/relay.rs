use super::parse::{Decoder, Protocol};
use super::*;
use crate::local_router::{diagnostic::Target, RouterState};
use serde_json::{json, Value};

fn body(request: &QualityCheckRequest, model: &str, chat: bool) -> Value {
    let mut body = if chat {
        json!({"model":model,"messages":[{"role":"user","content":request.prompt}],"stream":true,"stream_options":{"include_usage":true},"max_completion_tokens":request.max_output_tokens})
    } else {
        json!({"model":model,"input":[{"role":"user","content":[{"type":"input_text","text":request.prompt}]}],"stream":true,"store":false,"max_output_tokens":request.max_output_tokens})
    };
    if !request.effort.is_empty() {
        if chat {
            body["reasoning_effort"] = json!(request.effort);
        } else {
            body["reasoning"] = json!({"effort":request.effort});
        }
    }
    body
}

pub(super) async fn run(
    request: &QualityCheckRequest,
    result: &mut QualityCheckResult,
    state: &Arc<Mutex<RouterState>>,
    http: &QualityRelayHttp,
) -> Result<()> {
    let started = Instant::now();
    let target = Target::capture(state, &request.target.id, &request.model)?;
    let fingerprint = format!("{}:{}", target.fingerprint(), http.fingerprint);
    registry::bind_source(&request.run_id, fingerprint.clone())?;
    result.source_stamp = Some(registry::source_stamp(&fingerprint));
    result.target_label = target.redact(&target.label);
    result.protocol = if target.chat {
        "chat-completions"
    } else {
        "responses"
    }
    .into();
    let mut record = target.record(state, &request.request_id, &request.model, started);
    let outcome = async {
        let (mut response, _permit) = target
            .send(
                state,
                &http.client,
                &body(request, &target.model, target.chat),
                started,
            )
            .await?;
        record.log.status = Some(response.status().as_u16());
        if !response.status().is_success() {
            // 不把可能回显 Key / URL / 提示词的上游错误原文写入报告或日志。
            return Err(GateError::Other(format!(
                "检测线路返回 HTTP {}；未重试、换线或跟随重定向",
                response.status().as_u16()
            )));
        }
        let streaming = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.to_ascii_lowercase().contains("text/event-stream"));
        let mut decoder = Decoder::new(
            if target.chat {
                Protocol::Chat
            } else {
                Protocol::Responses
            },
            streaming,
        );
        loop {
            match response.chunk().await {
                Ok(Some(bytes)) => {
                    let parsed = decoder.push(&bytes, result, started.elapsed().as_millis() as u64);
                    // 任何退出路径都不能把上游回显的线路 Key 留在部分结果中。
                    result.text = target.redact(&result.text);
                    result.actual_model = result.actual_model.take().map(|m| target.redact(&m));
                    record.log.input_tokens = result.input_tokens;
                    record.log.output_tokens = result.output_tokens;
                    record.log.cache_read_tokens = result.cache_read_tokens;
                    record.log.first_token_ms = result.first_token_ms;
                    parsed?;
                }
                Ok(None) => break,
                Err(_) => {
                    return Err(GateError::Other(
                        "检测响应读取中断，未完成的输出不会计为成功".into(),
                    ))
                }
            }
        }
        let finished = decoder.finish(result, started.elapsed().as_millis() as u64);
        result.text = target.redact(&result.text);
        result.actual_model = result.actual_model.take().map(|m| target.redact(&m));
        record.log.input_tokens = result.input_tokens;
        record.log.output_tokens = result.output_tokens;
        record.log.cache_read_tokens = result.cache_read_tokens;
        record.log.first_token_ms = result.first_token_ms;
        finished
    }
    .await;
    match &outcome {
        Ok(()) => {
            record.log.fault = None;
            record.log.error = None;
        }
        Err(e) => {
            record.log.fault = Some("quality-failed".into());
            record.log.error = Some(e.to_string());
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_router::Upstream;
    use qb_contract::domain::Client;
    use qb_station::router::{RouteLimits, RouterSettings, RoutingMode};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct Mock {
        url: String,
        hits: Arc<Mutex<Vec<Value>>>,
        task: tokio::task::JoinHandle<()>,
    }
    impl Drop for Mock {
        fn drop(&mut self) {
            self.task.abort();
        }
    }
    async fn mock(stall: bool) -> Mock {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let hits = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&hits);
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut incoming = Vec::new();
            let mut buf = [0u8; 4096];
            let (head, length) = loop {
                let n = stream.read(&mut buf).await.unwrap();
                assert!(n > 0);
                incoming.extend_from_slice(&buf[..n]);
                assert!(incoming.len() < 64 * 1024);
                if let Some(end) = incoming.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&incoming[..end]).to_ascii_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:").map(str::trim))
                        .unwrap()
                        .parse()
                        .unwrap();
                    break (end + 4, length);
                }
            };
            while incoming.len() < head + length {
                let n = stream.read(&mut buf).await.unwrap();
                assert!(n > 0);
                incoming.extend_from_slice(&buf[..n]);
            }
            seen.lock()
                .unwrap()
                .push(serde_json::from_slice(&incoming[head..head + length]).unwrap());
            let prefix = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"{\\\"answer\\\":21}\"}\n\n";
            let terminal = "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"model\":\"declared-model\",\"usage\":{\"input_tokens\":19,\"output_tokens\":6,\"input_tokens_details\":{\"cached_tokens\":3}},\"output\":[{\"type\":\"message\",\"phase\":\"final_answer\",\"content\":[{\"type\":\"output_text\",\"text\":\"{\\\"answer\\\":21}\"}]}]}}\n\n";
            let body = format!("{prefix}{terminal}");
            let header = format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            stream.write_all(header.as_bytes()).await.unwrap();
            stream.write_all(prefix.as_bytes()).await.unwrap();
            if stall {
                std::future::pending::<()>().await;
            }
            stream.write_all(terminal.as_bytes()).await.unwrap();
        });
        Mock { url, hits, task }
    }
    fn mock_request(id: &str) -> QualityCheckRequest {
        QualityCheckRequest {
            run_id: uuid::Uuid::new_v4().to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
            target: QualityTarget {
                kind: QualityTargetKind::Relay,
                id: id.into(),
            },
            model: "asked".into(),
            effort: "ultra".into(),
            prompt: "do not log this prompt".into(),
            max_output_tokens: 512,
            timeout_secs: 5,
        }
    }
    fn mock_state(a: &Mock, b: &Mock) -> Arc<Mutex<RouterState>> {
        let a_id = "codex\u{1f}mock-a\u{1f}group";
        let b_id = "codex\u{1f}mock-b\u{1f}group";
        let mut s = RouterState::default();
        s.put_upstream(Upstream {
            route_id: a_id.into(),
            base_url: a.url.clone(),
            alternates: vec![b.url.clone()],
            model_exact: vec![("asked".into(), "mapped".into())],
            ..Default::default()
        });
        s.put_upstream(Upstream {
            route_id: b_id.into(),
            base_url: b.url.clone(),
            ..Default::default()
        });
        s.set_now(Client::Codex, b_id);
        s.request_switch(Client::Codex, a_id);
        let mut settings = RouterSettings::for_client(Client::Codex);
        settings.routing_mode = RoutingMode::Pool;
        settings.allowed_routes = vec![a_id.into(), b_id.into()];
        settings.admission.route_limits.insert(
            a_id.into(),
            RouteLimits {
                max_concurrent: 1,
                rpm: 10,
                check_quota: false,
            },
        );
        s.set_settings(settings);
        Arc::new(Mutex::new(s))
    }
    fn http() -> QualityRelayHttp {
        QualityRelayHttp {
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            fingerprint: "mock-network".into(),
        }
    }
    fn active(state: &Arc<Mutex<RouterState>>, id: &str) -> u32 {
        state
            .lock()
            .unwrap()
            .admission_views(Client::Codex, chrono::Utc::now().timestamp_millis())
            .into_iter()
            .find(|view| view.route_id == id)
            .map_or(0, |view| view.active)
    }
    #[tokio::test]
    async fn loopback_complete_cancel_and_timeout_keep_target_accounting_and_permit_lifetime() {
        let alternate = mock(false).await;
        let upstream = mock(false).await;
        let state = mock_state(&upstream, &alternate);
        let request = mock_request("codex\u{1f}mock-a\u{1f}group");
        let id = request.target.id.clone();
        let before = {
            let s = state.lock().unwrap();
            (s.current(Client::Codex), s.pending(Client::Codex))
        };
        let result = super::super::run(
            request.clone(),
            Arc::clone(&state),
            Arc::new(crate::gate::GateState::default()),
            Some(http()),
            || panic!("relay must not open gate"),
        )
        .await
        .unwrap();
        assert_eq!(result.status, QualityCheckStatus::Completed);
        assert_eq!(result.text, "{\"answer\":21}");
        assert_eq!(result.actual_model.as_deref(), Some("declared-model"));
        assert_eq!(result.input_tokens, Some(19));
        assert_eq!(result.cache_read_tokens, Some(3));
        assert!(result.first_token_ms.is_some());
        assert!(result.source_stamp.is_some());
        assert_eq!(active(&state, &id), 0);
        assert!(alternate.hits.lock().unwrap().is_empty());
        let sent = upstream.hits.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["model"], "mapped");
        assert_eq!(sent[0]["reasoning"]["effort"], "ultra");
        drop(sent);
        {
            let mut s = state.lock().unwrap();
            assert_eq!((s.current(Client::Codex), s.pending(Client::Codex)), before);
            let logs = s.drain_logs();
            assert_eq!(logs.len(), 1);
            assert_eq!(logs[0].route_id, id);
            assert_eq!(logs[0].input_tokens, Some(19));
            assert!(logs[0].fault.is_none());
        }
        for cancel_request in [true, false] {
            let stalled = mock(true).await;
            let state = mock_state(&stalled, &alternate);
            let mut request = mock_request(&id);
            if !cancel_request {
                request.timeout_secs = 1;
            }
            let request_id = request.request_id.clone();
            let routed = Arc::clone(&state);
            let task = tokio::spawn(async move {
                super::super::run(
                    request,
                    routed,
                    Arc::new(crate::gate::GateState::default()),
                    Some(http()),
                    || panic!("relay gate"),
                )
                .await
                .unwrap()
            });
            tokio::time::timeout(Duration::from_secs(3), async {
                while stalled.hits.lock().unwrap().is_empty() || active(&state, &id) != 1 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            if cancel_request {
                assert!(super::super::cancel(&request_id).unwrap());
            }
            let result = tokio::time::timeout(Duration::from_secs(3), task)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                result.status,
                if cancel_request {
                    QualityCheckStatus::Cancelled
                } else {
                    QualityCheckStatus::Timeout
                }
            );
            assert_eq!(active(&state, &id), 0);
            assert!(alternate.hits.lock().unwrap().is_empty());
            let logs = state.lock().unwrap().drain_logs();
            assert_eq!(logs.len(), 1);
            assert_eq!(logs[0].route_id, id);
            assert!(logs[0].fault.is_some());
        }
    }
    #[test]
    fn native_protocols_preserve_exact_effort_and_do_not_expose_tools_or_history() {
        let request = QualityCheckRequest {
            run_id: uuid::Uuid::new_v4().to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
            target: QualityTarget {
                kind: QualityTargetKind::Relay,
                id: "mock".into(),
            },
            model: "asked".into(),
            effort: "ultra".into(),
            prompt: "answer".into(),
            max_output_tokens: 512,
            timeout_secs: 30,
        };
        let responses = body(&request, "mapped", false);
        let chat = body(&request, "mapped", true);
        assert_eq!(responses["reasoning"]["effort"], "ultra");
        assert_eq!(chat["reasoning_effort"], "ultra");
        assert_eq!(responses["model"], "mapped");
        assert_eq!(chat["max_completion_tokens"], 512);
        for body in [responses, chat] {
            assert!(body.get("tools").is_none());
            assert!(body.get("previous_response_id").is_none());
        }
    }
}
