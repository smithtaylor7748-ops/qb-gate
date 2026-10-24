//! 请求日志：每发一条 [`RouterRequest`]，有界缓冲，后台批量落库。
//!
//! # 为什么要后台写
//!
//! 原来日志只在前端轮询 `station_logs` 时才落库（`drain_logs` 唯一的调用点就在那里）：
//! 没打开日志页时 `RouterState.logs` 在内存里一直涨，每发请求收尾时还要线性查一遍找自己那一行。
//! 现在每发请求自己揣着自己那一条（[`Record`]），结束时入队；[`spawn_log_writer`] 每隔几秒
//! 取走一批交给 [`LogSink`]（编排层给的，它认识数据库；单测给内存里的）。

use super::*;
use qb_proxy::classify::Fault;
use qb_proxy::sse_text::{append_utf8_safe, strip_sse_field, take_sse_block};
use qb_proxy::usage::parser::TokenUsage;
use qb_station::schedule::Permit;
use qb_station::sse::StreamScanner;
use serde_json::Value;
use std::time::{Duration, Instant};

/// 落库的那一头。
pub trait LogSink: Send + Sync + 'static {
    /// 一批日志落库。返回错误时这一批放回去，下一轮再写。
    fn write(&self, batch: Vec<RouterRequest>) -> Result<()>;
}

/// 起后台写库任务：每隔 `every` 取一次待落库的，交给 `sink`。
///
/// 返回的是停止开关：`send(true)` 之后它再写最后一批就退出（面板退出、路由停掉时调）。
pub fn spawn_log_writer(
    state: Arc<Mutex<RouterState>>,
    sink: Arc<dyn LogSink>,
    every: Duration,
) -> tokio::sync::watch::Sender<bool> {
    let (tx, mut rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        loop {
            let stop = tokio::select! {
                _ = tokio::time::sleep(every) => false,
                _ = rx.changed() => true,
            };
            flush(&state, &sink).await;
            if stop {
                return;
            }
        }
    });
    tx
}

/// 取走一批交给 `sink`。写失败就放回去（仍然有界）。
pub(crate) async fn flush(state: &Arc<Mutex<RouterState>>, sink: &Arc<dyn LogSink>) {
    let batch = match state.lock() {
        Ok(mut s) => s.drain_logs(),
        Err(_) => return,
    };
    if batch.is_empty() {
        return;
    }
    let sink = Arc::clone(sink);
    let retry = batch.clone();
    // 写库是同步的 rusqlite，别占着异步线程。
    let written = tokio::task::spawn_blocking(move || sink.write(batch)).await;
    let failed = match written {
        Ok(Ok(())) => None,
        Ok(Err(e)) => Some(e.to_string()),
        Err(e) => Some(e.to_string()),
    };
    if let Some(why) = failed {
        tracing::warn!(
            "本机路由：{} 条请求日志没写进库，下一轮再写：{why}",
            retry.len()
        );
        if let Ok(mut s) = state.lock() {
            s.requeue_logs(retry);
        }
    }
}

impl RouterState {
    /// 写库失败的那一批放回队头（仍然有界：超了丢最旧的）。
    pub fn requeue_logs(&mut self, mut batch: Vec<RouterRequest>) {
        batch.append(&mut self.unflushed);
        self.unflushed = batch;
        if self.unflushed.len() > MAX_UNFLUSHED_LOGS {
            let over = self.unflushed.len() - MAX_UNFLUSHED_LOGS;
            self.unflushed.drain(..over);
            self.dropped_logs += over as u64;
        }
    }
}

/// 一发请求的收尾。**放在 `Drop` 里**：流式响应早就从 handler 返回了，
/// 真正的「这一发结束」是那个流被丢掉的时候。
///
/// 收尾做四件事：这个软件的在飞计数减一、补总耗时、把流里收到的用量与结局写进日志、入队待落库。
pub(crate) struct Record {
    state: Arc<Mutex<RouterState>>,
    client: Client,
    started: Instant,
    pub log: RouterRequest,
    /// 交给客户端的那个流（有的话）。流结束时从这里读用量与结局。
    pub watch: Option<Arc<Mutex<StreamWatch>>>,
}

impl Record {
    pub(crate) fn new(
        state: Arc<Mutex<RouterState>>,
        client: Client,
        started: Instant,
        log: RouterRequest,
    ) -> Self {
        Self {
            state,
            client,
            started,
            log,
            watch: None,
        }
    }
}

impl Drop for Record {
    fn drop(&mut self) {
        self.log.total_ms = Some(self.started.elapsed().as_millis() as u64);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let finished = self
            .watch
            .take()
            .and_then(|w| w.lock().ok().map(|mut g| g.finish()));
        let Ok(mut s) = self.state.lock() else {
            return;
        };
        if let Some(done) = finished {
            if let Some(usage) = done.usage {
                fill_usage(&mut self.log, &usage);
            }
            if let Some(fault) = done.fault {
                self.log.fault.get_or_insert(fault.to_string());
            }
            match done.deferred {
                // Codex 的 2xx 流：健康判定推迟到流结束（`StreamScanner`），这时才记熔断、还名额。
                // 半路静默超时 / 读断了也算这条线的故障 —— 那不是使用者自己取消。
                Some((route_id, permit, outcome)) => {
                    let outcome = if done.fault.is_some() {
                        Outcome::ServerError
                    } else {
                        outcome
                    };
                    if let Some(key) = fault_key_of(outcome) {
                        self.log.fault.get_or_insert_with(|| key.to_string());
                    }
                    s.breaker_mut(&route_id, self.client)
                        .record_after(permit, outcome, now_ms);
                }
                // 其余的流在交出去时已经按状态码记过「好」；半路卡住 / 断掉再补记一次故障。
                None if done.fault.is_some() => {
                    let route_id = self.log.route_id.clone();
                    s.breaker_mut(&route_id, self.client)
                        .record(Outcome::ServerError, now_ms);
                }
                None => {}
            }
        }
        s.switch.end(self.client);
        s.push_log(std::mem::take(&mut self.log));
    }
}

/// 流的结局记进日志「故障」那一栏用哪个键（跟 `qb_proxy::classify::Fault::key` 同一套）。
fn fault_key_of(outcome: Outcome) -> Option<&'static str> {
    match outcome {
        Outcome::Ok => None,
        Outcome::RateLimited { .. } => Some(Fault::RateLimit.key()),
        Outcome::Rejected => Some(Fault::Auth.key()),
        Outcome::ServerError => Some(Fault::UpstreamDown.key()),
    }
}

/// 把解出来的用量填进日志。
pub(crate) fn fill_usage(log: &mut RouterRequest, usage: &TokenUsage) {
    log.input_tokens = Some(usage.input_tokens as u64);
    log.output_tokens = Some(usage.output_tokens as u64);
    log.cache_read_tokens = Some(usage.cache_read_tokens as u64);
    log.cache_write_tokens = Some(usage.cache_creation_tokens as u64);
    log.cache_write_1h_tokens = Some(usage.cache_creation_1h_tokens as u64);
    if log.upstream_model.is_empty() {
        if let Some(model) = usage.model.as_deref().filter(|m| *m != log.model) {
            log.upstream_model = model.to_string();
        }
    }
}

/// 流结束时交出来的东西。
pub(crate) struct WatchResult {
    pub usage: Option<TokenUsage>,
    /// 流半路出的问题（静默超时、读断了）。
    pub fault: Option<&'static str>,
    /// Codex 2xx 流按 SSE 结局补记的熔断：`(线路, 交出流时占着的名额, 结局)`。
    pub deferred: Option<(String, Permit, Outcome)>,
}

/// 边转发边看的那只眼睛：Codex 的 SSE 结局（推迟的健康判定）、用量事件、半路的故障。
///
/// **不缓冲整条响应**：SSE 只留未结束的那一小段尾巴，用量只留算 token 要用的那几个事件。
pub(crate) struct StreamWatch {
    client: Client,
    scanner: Option<(String, Permit, StreamScanner)>,
    buffer: String,
    remainder: Vec<u8>,
    events: Vec<Value>,
    pub fault: Option<&'static str>,
}

/// 用量事件最多留几个（Anthropic 的 `message_delta` 可能来好几次；留第一个 `message_start` 与最近的）。
const MAX_USAGE_EVENTS: usize = 64;
/// 一个 SSE 块最长多少字节还当它是块（超了说明这不是 SSE，丢掉缓冲，不再看用量）。
const MAX_SSE_BLOCK_BYTES: usize = 4 * 1024 * 1024;

impl StreamWatch {
    pub(crate) fn new(client: Client) -> Self {
        Self {
            client,
            scanner: None,
            buffer: String::new(),
            remainder: Vec::new(),
            events: Vec::new(),
            fault: None,
        }
    }

    /// 这个流的健康判定推迟到结束（只给 Codex 的 2xx 流用，见 `qb_station::sse`）。
    ///
    /// `permit` 是交出流时还占着的熔断名额（半开探测的那一发）：流结束时随结局一起还回去，
    /// 否则半开那一个名额一直被占着，线就永远探不回来。
    pub(crate) fn defer_health(mut self, route_id: &str, permit: Permit) -> Self {
        self.scanner = Some((route_id.to_string(), permit, StreamScanner::new()));
        self
    }

    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        if let Some((_, _, s)) = self.scanner.as_mut() {
            s.feed(bytes);
        }
        append_utf8_safe(&mut self.buffer, &mut self.remainder, bytes);
        while let Some(block) = take_sse_block(&mut self.buffer) {
            self.take_event(&block);
        }
        if self.buffer.len() > MAX_SSE_BLOCK_BYTES {
            self.buffer.clear();
        }
    }

    fn take_event(&mut self, block: &str) {
        let data = block
            .lines()
            .filter_map(|line| strip_sse_field(line, "data"))
            .collect::<Vec<_>>()
            .join("\n");
        let Ok(event) = serde_json::from_str::<Value>(&data) else {
            return;
        };
        if usage_event(self.client, &event) {
            self.events.push(event);
            if self.events.len() > MAX_USAGE_EVENTS {
                // 第一个（message_start / 首块）留着，丢第二旧的。
                self.events.remove(1);
            }
        }
    }

    pub(crate) fn finish(&mut self) -> WatchResult {
        let tail = std::mem::take(&mut self.buffer);
        if !tail.trim().is_empty() {
            self.take_event(&tail);
        }
        let usage = parse_stream_usage(self.client, &self.events);
        WatchResult {
            usage,
            fault: self.fault,
            deferred: self
                .scanner
                .take()
                .map(|(route, permit, s)| (route, permit, s.finish().to_outcome())),
        }
    }
}

/// 这个事件对算用量有没有用。
fn usage_event(client: Client, event: &Value) -> bool {
    let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
    match client {
        Client::ClaudeCode | Client::ClaudeDesktop => {
            matches!(kind, "message_start" | "message_delta")
        }
        _ => {
            kind == "response.completed"
                || kind == "image_generation.completed"
                || event.get("usage").is_some_and(|u| !u.is_null())
                // Chat 流的第一块带着 id，`from_openai_stream_events` 拿它补 message_id。
                || (kind.is_empty() && event.get("id").is_some() && event.get("choices").is_some())
        }
    }
}

/// 流式用量。Claude 按 Anthropic 事件解，其余按 Codex（Responses / Chat）自动认。
pub(crate) fn parse_stream_usage(client: Client, events: &[Value]) -> Option<TokenUsage> {
    if events.is_empty() {
        return None;
    }
    match client {
        Client::ClaudeCode | Client::ClaudeDesktop => TokenUsage::from_claude_stream_events(events),
        _ => TokenUsage::from_codex_stream_events_auto(events),
    }
    .filter(TokenUsage::has_billable_tokens)
}

/// 非流式用量（整份 JSON）。
pub(crate) fn parse_body_usage(client: Client, body: &[u8]) -> Option<TokenUsage> {
    let value: Value = serde_json::from_slice(body).ok()?;
    match client {
        Client::ClaudeCode | Client::ClaudeDesktop => TokenUsage::from_claude_response(&value),
        _ => TokenUsage::from_codex_response_auto(&value),
    }
    .filter(TokenUsage::has_billable_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_watch_pulls_four_kinds_of_tokens_out_of_an_anthropic_stream() {
        let mut w = StreamWatch::new(Client::ClaudeCode);
        let sse = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-opus-5\",\"usage\":{\"input_tokens\":12,\"cache_read_input_tokens\":300,\"cache_creation_input_tokens\":40,\"cache_creation\":{\"ephemeral_1h_input_tokens\":40}}}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"hi\"}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":7}}\n\n",
        );
        // 故意切在 UTF-8 / 块边界中间喂。
        let bytes = sse.as_bytes();
        for chunk in bytes.chunks(17) {
            w.feed(chunk);
        }
        let done = w.finish();
        let u = done.usage.expect("没解出用量");
        assert_eq!(
            (
                u.input_tokens,
                u.output_tokens,
                u.cache_read_tokens,
                u.cache_creation_tokens,
                u.cache_creation_1h_tokens
            ),
            (12, 7, 300, 40, 40)
        );
        assert_eq!(done.deferred, None, "Claude 的流不推迟健康判定");
        // 只留了算 token 要用的两个事件，没缓冲内容事件。
        assert_eq!(w.events.len(), 2);
    }

    #[test]
    fn the_watch_reads_codex_usage_from_response_completed_and_defers_health() {
        let probe = Permit {
            allowed: true,
            half_open: true,
        };
        let mut w = StreamWatch::new(Client::Codex).defer_health("r1", probe);
        w.feed(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"x\"}\n\n");
        w.feed(b"data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":100,\"output_tokens\":5,\"input_tokens_details\":{\"cached_tokens\":60}}}}\n\n");
        let done = w.finish();
        let u = done.usage.unwrap();
        assert_eq!(
            (u.input_tokens, u.output_tokens, u.cache_read_tokens),
            (100, 5, 60)
        );
        assert_eq!(
            done.deferred,
            Some(("r1".to_string(), probe, Outcome::Ok)),
            "半开名额要跟着结局一起交回去"
        );
    }

    #[test]
    fn an_all_zero_usage_is_not_recorded_as_tokens() {
        // cc-switch 的 `has_billable_tokens` 闸门：全 0 的是合成出来的终止事件，不是真用量。
        let body = br#"{"usage":{"input_tokens":0,"output_tokens":0}}"#;
        assert!(parse_body_usage(Client::Codex, body).is_none());
        let body = br#"{"id":"msg_1","usage":{"input_tokens":3,"output_tokens":4}}"#;
        assert_eq!(
            parse_body_usage(Client::ClaudeCode, body)
                .unwrap()
                .output_tokens,
            4
        );
    }

    #[test]
    fn requeued_logs_go_back_in_front_and_stay_bounded() {
        let mut s = RouterState::default();
        s.push_log(RouterRequest {
            id: "new".into(),
            ..Default::default()
        });
        let failed = vec![RouterRequest {
            id: "old".into(),
            ..Default::default()
        }];
        s.requeue_logs(failed);
        let ids: Vec<_> = s.drain_logs().into_iter().map(|l| l.id).collect();
        assert_eq!(ids, ["old", "new"]);
        for i in 0..(MAX_UNFLUSHED_LOGS + 3) {
            s.push_log(RouterRequest {
                id: i.to_string(),
                ..Default::default()
            });
        }
        assert_eq!(s.pending_logs(), MAX_UNFLUSHED_LOGS);
        assert_eq!(s.dropped_logs(), 3);
        assert_eq!(s.logs().len(), RECENT_LOGS);
    }

    struct Memory(Mutex<Vec<RouterRequest>>, bool);
    impl LogSink for Memory {
        fn write(&self, batch: Vec<RouterRequest>) -> Result<()> {
            if self.1 {
                return Err(GateError::Other("库被锁了".into()));
            }
            self.0.lock().unwrap().extend(batch);
            Ok(())
        }
    }

    #[tokio::test]
    async fn the_writer_flushes_in_the_background_and_keeps_a_failed_batch() {
        let state = Arc::new(Mutex::new(RouterState::default()));
        state.lock().unwrap().push_log(RouterRequest {
            id: "a".into(),
            ..Default::default()
        });
        let ok = Arc::new(Memory(Mutex::new(Vec::new()), false));
        let sink: Arc<dyn LogSink> = ok.clone();
        flush(&state, &sink).await;
        assert_eq!(ok.0.lock().unwrap().len(), 1);
        assert_eq!(state.lock().unwrap().pending_logs(), 0);

        state.lock().unwrap().push_log(RouterRequest {
            id: "b".into(),
            ..Default::default()
        });
        let broken: Arc<dyn LogSink> = Arc::new(Memory(Mutex::new(Vec::new()), true));
        flush(&state, &broken).await;
        assert_eq!(
            state.lock().unwrap().pending_logs(),
            1,
            "写库失败的那一批丢了"
        );

        // 停止开关：发停止之后再写最后一批。
        let stop = spawn_log_writer(Arc::clone(&state), sink, Duration::from_secs(3600));
        stop.send(true).unwrap();
        for _ in 0..100 {
            if state.lock().unwrap().pending_logs() == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("停止时没把最后一批写掉");
    }
}
