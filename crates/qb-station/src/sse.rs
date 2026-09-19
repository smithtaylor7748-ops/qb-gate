//! SSE 终止事件分类。**纯函数 / 纯状态,不取系统时间、不联网、不落盘。**
//!
//! # 它修的是哪个 bug
//!
//! 本机路由原先只按 HTTP 状态码判健康([`crate::schedule::Outcome::from_status`])。
//! 可是 Codex 的 Responses 接口会**在 HTTP 200 的 SSE 流里**报失败:
//! `type: response.failed` / `response.incomplete` / `error`,或干脆没等到
//! `response.completed` 就断。只看状态码的话,这些一律记成成功,熔断永远不触发。
//!
//! 这个模块把「流最终是什么结局」抽成一个可单测的纯判定,让路由在流结束时
//! 把结果折算回 [`Outcome`] 补记给熔断器。
//!
//! # 三档失败必须分开(和熔断表对齐)
//!
//! | 流里出现 | 归档 | 为什么 |
//! |---|---|---|
//! | `server_is_overloaded` / `slow_down`(Anthropic `overloaded_error`) | [`StreamOutcome::Capacity`] | 容量不足,**不是 429**。当作服务端错误计入连续计数,不当限流 |
//! | `rate_limit_exceeded` / `insufficient_quota`(Anthropic `rate_limit_error`) | [`StreamOutcome::RateLimited`] | 明确限流,只降权不熔断 |
//! | 其它 `error` / `response.failed` / `response.incomplete` | [`StreamOutcome::Failed`] | 服务端明确失败 |
//! | 流结束既无完成标记也无失败标记 | [`StreamOutcome::Incomplete`] | 半截断开(也可能是客户端主动取消) |
//!
//! # ⛔ clean-room
//!
//! 分类口径参照 ccodex-sleep-state(GPL-3.0)README 里公开描述的官方 Codex SSE
//! 行为与本仓库既有的 `diagnostics::validate_stream_bytes`,**逐行自己写**,未复制其源码。
//! 见 `ATTRIBUTION.md`。

use crate::schedule::Outcome;

/// 一条 SSE 流最终是什么结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamOutcome {
    /// 收到 `response.completed` / `message_stop` / `[DONE]`,且此前没有失败事件。
    Completed,
    /// 流里明确报了失败(非容量、非限流)。
    Failed,
    /// 上游明确说容量不足 —— **不是 429**。
    Capacity,
    /// 上游在回复流里报限流 / 额度不足。
    RateLimited,
    /// 流结束了却既没有完成标记也没有失败标记 —— 半截断开。
    Incomplete,
}

impl StreamOutcome {
    /// 折算成熔断器认识的 [`Outcome`]。
    ///
    /// ⛔ **`Incomplete` 折成 `Ok`,不当故障。** 半截断开既可能是上游截断,
    /// 也可能是**使用者主动取消**了这一轮 —— 把用户取消记成路由故障,
    /// 连着取消五次就会把一条好线误熔断。宁可漏判,不可错杀。
    /// 明确的 `Failed` / `Capacity` 才计入服务端错误连续计数。
    pub fn to_outcome(self) -> Outcome {
        match self {
            StreamOutcome::Completed | StreamOutcome::Incomplete => Outcome::Ok,
            StreamOutcome::Failed | StreamOutcome::Capacity => Outcome::ServerError,
            // SSE 正文里没有 Retry-After,交给熔断器用默认冷却期。
            StreamOutcome::RateLimited => Outcome::RateLimited {
                retry_after_ms: None,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    Failed,
    Capacity,
    RateLimited,
}

/// 单个失败事件里的 code / error.type 归档。
fn classify_code(code: &str) -> Failure {
    match code {
        "server_is_overloaded" | "slow_down" | "overloaded_error" => Failure::Capacity,
        "rate_limit_exceeded" | "insufficient_quota" | "rate_limit_error" => Failure::RateLimited,
        _ => Failure::Failed,
    }
}

/// 单个 SSE 事件里最不透明的那一层里,把错误码抠出来。
fn error_code(value: &serde_json::Value) -> &str {
    for ptr in [
        "/code",
        "/error/code",
        "/error/type",
        "/response/error/code",
        "/response/error/type",
    ] {
        if let Some(code) = value.pointer(ptr).and_then(serde_json::Value::as_str) {
            if !code.is_empty() {
                return code;
            }
        }
    }
    ""
}

/// 顺着流一块块喂进来的增量扫描器。
///
/// **不缓冲整条响应** —— 只保留「还没遇到空行的那一小段尾巴」和两个布尔标志,
/// 内存是 O(单个事件大小),不随响应长度增长。路由因此仍然是纯流式透传。
pub struct StreamScanner {
    /// 还没遇到 `\n\n` 的尾巴。已剥掉 `\r`。
    tail: Vec<u8>,
    completed: bool,
    failure: Option<Failure>,
    /// 单个事件超过 [`Self::MAX_TAIL`] 时置位:那个事件无法解析,但不影响后续。
    overflowed: bool,
}

impl Default for StreamScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamScanner {
    /// 单个未终止事件保留多少字节的上限。终止事件都很小,超过这个多半是
    /// 一大段正文 delta —— 丢掉它的前半段不影响我们找 `\n\n` 边界与终止事件。
    const MAX_TAIL: usize = 1 << 20;

    pub fn new() -> Self {
        Self {
            tail: Vec::new(),
            completed: false,
            failure: None,
            overflowed: false,
        }
    }

    /// 喂进一块原始字节。可以任意切分,事件跨块也没关系。
    pub fn feed(&mut self, chunk: &[u8]) {
        // 去掉 `\r`,把 CRLF 归一成 LF —— 跨块的 `\r` `\n` 也就不会漏判边界。
        self.tail
            .extend(chunk.iter().copied().filter(|&b| b != b'\r'));
        while let Some(i) = find(&self.tail, b"\n\n") {
            let block = self.tail[..i].to_vec();
            self.process_block(&block);
            self.tail.drain(..i + 2);
        }
        if self.tail.len() > Self::MAX_TAIL {
            // 单个事件过大:只留末尾一小段继续找边界,标记这一段没能解析。
            let keep = self.tail.len() - 4096;
            self.tail.drain(..keep);
            self.overflowed = true;
        }
    }

    /// 流结束。把尾巴当作最后一个事件处理,给出结局。
    pub fn finish(mut self) -> StreamOutcome {
        let block = std::mem::take(&mut self.tail);
        self.process_block(&block);
        match self.failure {
            Some(Failure::Failed) => StreamOutcome::Failed,
            Some(Failure::Capacity) => StreamOutcome::Capacity,
            Some(Failure::RateLimited) => StreamOutcome::RateLimited,
            None if self.completed => StreamOutcome::Completed,
            None => StreamOutcome::Incomplete,
        }
    }

    fn process_block(&mut self, block: &[u8]) {
        let Ok(text) = std::str::from_utf8(block) else {
            return;
        };
        let mut event_name = "";
        let mut payload = String::new();
        for line in text.split('\n') {
            if let Some(rest) = line.strip_prefix("event:") {
                event_name = rest.trim();
            } else if let Some(rest) = line.strip_prefix("data:") {
                if !payload.is_empty() {
                    payload.push('\n');
                }
                payload.push_str(rest.strip_prefix(' ').unwrap_or(rest));
            }
        }
        let payload = payload.trim();
        if payload.is_empty() {
            return;
        }
        if payload == "[DONE]" {
            self.completed = true;
            return;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
            return;
        };
        let kind = value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(event_name);
        if matches!(kind, "response.completed" | "message_stop") {
            self.completed = true;
        }
        let is_failure = matches!(kind, "error" | "response.failed" | "response.incomplete")
            || value.get("error").is_some();
        if is_failure {
            let next = classify_code(error_code(&value));
            self.failure = Some(match (self.failure, next) {
                // 已经记下的第一个失败保留;但一个具体的容量 / 限流可以取代
                // 先前那个笼统的 Failed —— 那两个更有指导意义。
                (Some(Failure::Failed), specific) => specific,
                (Some(existing), _) => existing,
                (None, next) => next,
            });
        }
    }
}

/// 整段字节一次性判定 —— 单测与非流式场景用。流式路径用 [`StreamScanner`]。
pub fn classify_stream(bytes: &[u8]) -> StreamOutcome {
    let mut scanner = StreamScanner::new();
    scanner.feed(bytes);
    scanner.finish()
}

/// 朴素子串查找。SSE 分块只找 `\n\n`,响应不大,不值得引一个依赖。
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_completed_responses_stream_is_ok() {
        let sse = "event: response.completed\n\
                   data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
        assert_eq!(classify_stream(sse.as_bytes()), StreamOutcome::Completed);
        assert_eq!(classify_stream(sse.as_bytes()).to_outcome(), Outcome::Ok);
    }

    #[test]
    fn http_200_with_response_failed_is_a_failure_not_success() {
        // 这正是本模块要修的 bug:HTTP 200,但流里明确失败。
        let sse = "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"internal_error\"}}}\n\n";
        assert_eq!(classify_stream(sse.as_bytes()), StreamOutcome::Failed);
        assert_eq!(
            classify_stream(sse.as_bytes()).to_outcome(),
            Outcome::ServerError
        );
    }

    #[test]
    fn a_completed_marker_after_a_failure_does_not_cancel_it() {
        // 失败先出现,后面就算又来个 completed 也不能把它洗白。
        let sse = "data: {\"type\":\"response.failed\",\"error\":{\"code\":\"boom\"}}\n\n\
                   data: {\"type\":\"response.completed\"}\n\n";
        assert_eq!(classify_stream(sse.as_bytes()), StreamOutcome::Failed);
    }

    #[test]
    fn overloaded_is_capacity_not_rate_limit() {
        let sse = "data: {\"type\":\"error\",\"code\":\"server_is_overloaded\"}\n\n";
        assert_eq!(classify_stream(sse.as_bytes()), StreamOutcome::Capacity);
        // 容量算服务端错误(计入连续计数),不是 429。
        assert_eq!(
            classify_stream(sse.as_bytes()).to_outcome(),
            Outcome::ServerError
        );
    }

    #[test]
    fn explicit_rate_limit_in_stream_is_rate_limited() {
        let sse = "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"rate_limit_exceeded\"}}}\n\n";
        assert_eq!(classify_stream(sse.as_bytes()), StreamOutcome::RateLimited);
        assert_eq!(
            classify_stream(sse.as_bytes()).to_outcome(),
            Outcome::RateLimited {
                retry_after_ms: None
            }
        );
    }

    #[test]
    fn anthropic_overloaded_error_event_is_capacity() {
        // 只改 Codex 时不会走到这里,但纯函数认得 Anthropic 形状也无害、便于将来。
        let sse =
            "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}\n\n";
        assert_eq!(classify_stream(sse.as_bytes()), StreamOutcome::Capacity);
    }

    #[test]
    fn a_specific_failure_upgrades_an_earlier_generic_one() {
        let sse = "data: {\"type\":\"error\",\"code\":\"unknown\"}\n\n\
                   data: {\"type\":\"error\",\"code\":\"insufficient_quota\"}\n\n";
        assert_eq!(classify_stream(sse.as_bytes()), StreamOutcome::RateLimited);
    }

    #[test]
    fn a_stream_that_never_completes_is_incomplete_but_not_a_fault() {
        let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n";
        assert_eq!(classify_stream(sse.as_bytes()), StreamOutcome::Incomplete);
        // ⛔ 不当故障:可能是客户端主动取消,别误熔断好线。
        assert_eq!(classify_stream(sse.as_bytes()).to_outcome(), Outcome::Ok);
    }

    #[test]
    fn done_sentinel_counts_as_completed() {
        let sse = "data: [DONE]\n\n";
        assert_eq!(classify_stream(sse.as_bytes()), StreamOutcome::Completed);
    }

    #[test]
    fn events_split_across_chunks_are_still_parsed() {
        let mut s = StreamScanner::new();
        // 把一个 response.failed 事件从中间切开,分两次喂。
        s.feed(b"data: {\"type\":\"resp");
        s.feed(b"onse.failed\",\"error\":{\"code\":\"x\"}}\n\n");
        assert_eq!(s.finish(), StreamOutcome::Failed);
    }

    #[test]
    fn crlf_line_endings_are_handled() {
        let sse = "data: {\"type\":\"response.completed\"}\r\n\r\n";
        assert_eq!(classify_stream(sse.as_bytes()), StreamOutcome::Completed);
    }

    #[test]
    fn a_terminal_event_without_trailing_blank_line_is_still_seen() {
        // 最后一个事件没有以空行收尾,finish() 也要认出来。
        let sse = "data: {\"type\":\"response.completed\"}";
        assert_eq!(classify_stream(sse.as_bytes()), StreamOutcome::Completed);
    }

    #[test]
    fn garbage_between_events_is_ignored() {
        let sse = "data: not json\n\n\
                   data: {\"type\":\"response.completed\"}\n\n";
        assert_eq!(classify_stream(sse.as_bytes()), StreamOutcome::Completed);
    }
}
