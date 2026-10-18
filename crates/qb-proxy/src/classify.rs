// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
// 本文件是三张表的合并，不是逐行照搬；三个来源见下。
//! 一发失败之后的三个问题：**换不换下一条线**、**算不算这条线的故障**、**记进熔断的哪一档**。
//!
//! # 三张表合成一张（中转站重构阶段 1）
//!
//! | 来源 | 贡献了什么 |
//! |---|---|
//! | cc-switch v4.0.14 `forwarder.rs` 的 `categorize_proxy_error`（MIT） | 状态码分桶：400 405 406 413 414 415 422 501 是请求自己的错，换哪家都一样，**不换**；其余 4xx 与全部 5xx 换下一家 |
//! | QB Gate 自己的熔断三档（`qb-station::schedule::breaker`） | 429 只降权不熔断；401–403 立即熔断；5xx / 超时连续才熔断 |
//! | sub2guard v2 的 `probe.classify`（使用者自己的调度） | **先认报错文字、再看状态码**；确定性（余额 / 认证 / 退役）一次即判、暂时性要连续、限流不算故障、缺模型只跳过那个模型 |
//!
//! 文字表（[`TEXT_PATTERNS`]）逐条照 sub2guard 的 `_TEXT_PATTERNS`，顺序也照它 —— 顺序有意义。
//!
//! # ⛔ 这里的结论只决定「换哪条中转线」
//!
//! 不会、也够不着官方账户槽位。按额度 / 429 自动切官方槽位是 CLAUDE.md 明确不做的「自动换号」。

use crate::proxy_error::ProxyError;
use regex::Regex;
use std::sync::OnceLock;

/// 失败的具体类型。名字照 sub2guard 的分类标签，另加两档请求侧的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fault {
    /// 上游余额不足，要充值。
    Balance,
    /// 上游凭据失效 / 无权访问 / 分组被删。
    Auth,
    /// 上游域名已退役。
    Retired,
    /// 上游 5xx 或号池空、连接被拒 / 重置。
    UpstreamDown,
    /// 超时（连接、首包、静默、整份读完）。
    Timeout,
    /// 上游过载。
    Overloaded,
    /// 认不出来的失败。按暂时性处理（sub2guard 同样如此）。
    Unknown,
    /// 被限流。
    RateLimit,
    /// 并发已满。
    Concurrency,
    /// 上游不提供这个模型。
    ModelMissing,
    /// 本机这一侧的问题（这条线没配 Key、出站代理连不上……），不是上游的错。
    Infra,
    /// 这条线不接这一发（404 / 409 / 451，响应里也没说为什么）。换别的线试，**不算这条线的故障** ——
    /// QB Gate 原来的规矩：404 多半是模型名写错，算成故障的话连打五次就把一条健康的线掐掉；
    /// cc-switch 那边则换下一家（别家的模型映射、地域可能不同）。两条都留着。
    NotServedHere,
    /// 请求自己的问题（参数错、体太大、协议不支持）—— 换哪条线都一样。
    ClientRequest,
    /// 客户端自己断开了。
    ClientAbort,
}

/// 故障归哪一组。决定是否重试、怎么记熔断。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaultGroup {
    /// 重测没意义：是整把 key 的状态。1 次即判故障，长熔断。
    Deterministic,
    /// 可能是抖动：连续几次才判故障。
    Transient,
    /// 不算故障：被限流恰恰说明它在服务，只降权。
    Busy,
    /// 只跳过这条线上的这个模型。
    ModelMissing,
    /// 换哪条线都一样，或者是本机 / 客户端这一侧的事：不伤熔断。
    Neutral,
}

impl Fault {
    pub fn group(self) -> FaultGroup {
        match self {
            Fault::Balance | Fault::Auth | Fault::Retired => FaultGroup::Deterministic,
            Fault::UpstreamDown | Fault::Timeout | Fault::Overloaded | Fault::Unknown => {
                FaultGroup::Transient
            }
            Fault::RateLimit | Fault::Concurrency => FaultGroup::Busy,
            Fault::ModelMissing => FaultGroup::ModelMissing,
            Fault::Infra | Fault::NotServedHere | Fault::ClientRequest | Fault::ClientAbort => {
                FaultGroup::Neutral
            }
        }
    }

    /// 给日志和界面的一句话（照 sub2guard 的 `LABELS`，「Sub2API」换成本机路由的说法）。
    pub fn label(self) -> &'static str {
        match self {
            Fault::Balance => "上游余额不足，要充值",
            Fault::Auth => "上游凭据失效 / 无权访问 / 分组被删",
            Fault::Retired => "上游域名已退役",
            Fault::UpstreamDown => "上游 5xx 或号池空",
            Fault::Timeout => "超时",
            Fault::Overloaded => "上游过载",
            Fault::Unknown => "未知错误",
            Fault::RateLimit => "被限流",
            Fault::Concurrency => "并发已满",
            Fault::ModelMissing => "上游不提供这个模型",
            Fault::Infra => "本机这一侧的问题（不算上游的错）",
            Fault::NotServedHere => "这条线不接这一发（404 之类），换别的线试",
            Fault::ClientRequest => "请求本身的问题（换哪条线都一样）",
            Fault::ClientAbort => "客户端自己断开了",
        }
    }

    /// 存进日志表的稳定写法（kebab-case）。界面按它挑文案，改了要两头一起改。
    pub fn key(self) -> &'static str {
        match self {
            Fault::Balance => "balance",
            Fault::Auth => "auth",
            Fault::Retired => "retired",
            Fault::UpstreamDown => "upstream-down",
            Fault::Timeout => "timeout",
            Fault::Overloaded => "overloaded",
            Fault::Unknown => "unknown",
            Fault::RateLimit => "rate-limit",
            Fault::Concurrency => "concurrency",
            Fault::ModelMissing => "model-missing",
            Fault::Infra => "infra",
            Fault::NotServedHere => "not-served",
            Fault::ClientRequest => "client-request",
            Fault::ClientAbort => "client-abort",
        }
    }
}

/// 记进熔断的哪一档。跟 `qb_station::schedule::Outcome` 一一对应（多一档 `Neutral`），
/// 由 `qb-app` 换过去 —— 两个都是第 3 层，彼此不依赖。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerVerdict {
    /// 这条线是好的。
    Ok,
    /// 被限流：只降权，不熔断。
    RateLimited { retry_after_ms: Option<i64> },
    /// 确定性故障（余额 / 认证 / 退役）：立即熔断、而且关得久。
    Rejected,
    /// 暂时性故障：连续几次才熔断。
    ServerError,
    /// 不算这条线的账：只放回半开名额，不动任何计数。
    Neutral,
}

/// 一次失败的完整判读。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorClass {
    /// 换下一条线再试。
    pub retry_next: bool,
    pub breaker: BreakerVerdict,
    pub fault: Fault,
}

impl ErrorClass {
    fn of(fault: Fault, retry_after_ms: Option<i64>) -> Self {
        let (retry_next, breaker) = match fault.group() {
            FaultGroup::Deterministic => (true, BreakerVerdict::Rejected),
            FaultGroup::Transient => (true, BreakerVerdict::ServerError),
            FaultGroup::Busy => (true, BreakerVerdict::RateLimited { retry_after_ms }),
            // 别的线可能有这个模型；这条线只在这个模型上跳过（调用方记 6 小时）。
            FaultGroup::ModelMissing => (true, BreakerVerdict::Neutral),
            FaultGroup::Neutral => (
                // 本机这一侧的问题（比如这条线没配 Key）、这条线不接（404）换一条可能就好了；
                // 请求自己的问题与客户端断开，换哪条都一样。
                matches!(fault, Fault::Infra | Fault::NotServedHere),
                BreakerVerdict::Neutral,
            ),
        };
        Self {
            retry_next,
            breaker,
            fault,
        }
    }
}

/// sub2guard `_TEXT_PATTERNS` 原样（`re.IGNORECASE` → `(?i)`）。**顺序有意义**：先命中先算。
/// 报错串全部来自 sub2guard v1 的 probe_log / ops_error_logs 原文。
pub const TEXT_PATTERNS: &[(Fault, &str)] = &[
    (
        Fault::ModelMissing,
        r"no available channel for model|model_not_found|unknown provider for model|is not supported (by any configured account|when using)|model.{0,40}(not (found|exist|supported)|does not exist)|no such model|unsupported model|无可用渠道",
    ),
    (
        Fault::Balance,
        r"insufficient[_ ]?(account[_ ]?)?balance|insufficient[_ ]?quota|余额不足|balance is not enough|arrears",
    ),
    (
        Fault::Auth,
        r"invalid[_ ]?api[_ ]?key|api[_ ]?key[_ ]?(is[_ ]?)?disabled|invalid[_ ]?token|access forbidden|authentication failed|unauthorized|group_deleted|所属分组已删除|无权访问|不再允许当前用户使用|account (has been |is )?(disabled|suspended|banned)",
    ),
    (Fault::Retired, r"已退役|\bretired\b|no longer available"),
    (
        Fault::Concurrency,
        r"concurrency limit|too many concurrent|并发.{0,6}(超|上限)",
    ),
    (
        Fault::RateLimit,
        r"rate[_ -]?limit|too many (requests|pending)",
    ),
    (
        Fault::Overloaded,
        r"overload|service is busy|server is busy|at capacity",
    ),
    (
        Fault::Timeout,
        r"timed? ?out|deadline exceeded|context canceled",
    ),
    (
        Fault::UpstreamDown,
        r"temporarily unavailable|no available accounts|bad gateway|service unavailable|upstream request failed|stream_read_error|stream ended|测试流提前结束|connection (refused|reset)",
    ),
];

fn compiled() -> &'static [(Fault, Regex)] {
    static COMPILED: OnceLock<Vec<(Fault, Regex)>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        TEXT_PATTERNS
            .iter()
            .map(|(fault, pattern)| {
                (
                    *fault,
                    Regex::new(&format!("(?i){pattern}")).expect("sub2guard 的文字表是合法的正则"),
                )
            })
            .collect()
    })
}

/// 只从「returned 503」「HTTP 503」「error code: 503」这种上下文里取状态码，
/// 不能裸搜数字 —— 请求 id 里一长串数字随时能撞上 401 / 503（sub2guard 的 `_STATUS_RE`）。
fn status_in_text(text: &str) -> Option<u16> {
    static STATUS: OnceLock<Regex> = OnceLock::new();
    let re = STATUS.get_or_init(|| {
        Regex::new(r"(?i)(?:returned|HTTP|error code:?)\s*(\d{3})\b").expect("合法的正则")
    });
    re.captures(text)?.get(1)?.as_str().parse().ok()
}

/// 只认文字（sub2guard 的前半段）。认不出就是 `None`。
pub fn classify_text(message: &str) -> Option<Fault> {
    let text = message.trim();
    if text.is_empty() {
        return None;
    }
    compiled()
        .iter()
        .find(|(_, pattern)| pattern.is_match(text))
        .map(|(fault, _)| *fault)
}

/// 只看状态码。sub2guard 的后半段，加上 cc-switch 的「请求自己的错」那一桶。
pub fn classify_status(status: u16) -> Fault {
    match status {
        401 | 403 => Fault::Auth,
        402 => Fault::Balance,
        410 => Fault::Retired,
        429 => Fault::RateLimit,
        408 | 504 | 524 => Fault::Timeout,
        // cc-switch：客户端请求自身有问题的状态码，无论换哪个 provider 都会被拒绝。
        400 | 405 | 406 | 413 | 414 | 415 | 422 | 501 => Fault::ClientRequest,
        s if s >= 500 => Fault::UpstreamDown,
        // 404 / 409 / 451 等其余 4xx：cc-switch 换下一家（别家的 key、地域、模型映射可能不同）；
        // 但不记成这条线的故障 —— QB Gate 原来的规矩（`Outcome::from_status` 把它们当 Ok），
        // 404 多半是模型名写错，按故障算会把一条健康的线掐掉。
        s if (400..500).contains(&s) => Fault::NotServedHere,
        _ => Fault::Unknown,
    }
}

/// 一段报错文字（测活回来的、2xx 里藏的错误信封）：先认文字，再从上下文里找状态码。
pub fn classify_message(message: &str) -> Fault {
    classify_text(message)
        .or_else(|| status_in_text(message).map(classify_status))
        .unwrap_or(Fault::Unknown)
}

/// 上游回了一个非 2xx。**先认响应体里的文字**，认不出再按状态码 ——
/// 有的站余额不足回 400、缺模型回 500，只看状态码会判反。
pub fn classify_upstream(
    status: u16,
    body: Option<&str>,
    retry_after_ms: Option<i64>,
) -> ErrorClass {
    let fault = body
        .and_then(classify_text)
        .unwrap_or_else(|| classify_status(status));
    // 429 却在文字里写着别的（比如「rate limit」以外的「余额不足」）：照文字。
    // 但 429 本身就是限流 —— 文字认不出来时它落在 RateLimit，Retry-After 带上。
    ErrorClass::of(fault, retry_after_ms)
}

/// 本机路由转发一发请求失败时的总入口。
pub fn classify(error: &ProxyError, retry_after_ms: Option<i64>) -> ErrorClass {
    let fault = match error {
        ProxyError::UpstreamError { status, body } => {
            return classify_upstream(*status, body.as_deref(), retry_after_ms)
        }
        ProxyError::Timeout(_) | ProxyError::StreamIdleTimeout(_) => Fault::Timeout,
        // 连不上、读到一半断了：先认文字（connection refused / reset），认不出算上游挂了。
        ProxyError::ForwardFailed(message) | ProxyError::ProviderUnhealthy(message) => {
            classify_text(message).unwrap_or(Fault::UpstreamDown)
        }
        // HTTP 200 里藏着的错误信封：按信封里的文字判（overloaded_error、rate_limit_error……）。
        ProxyError::TransformError(message) => classify_message(message),
        ProxyError::AuthError(_) => Fault::Auth,
        ProxyError::ResponseBodyTooLarge(_) => Fault::UpstreamDown,
        // 这条线自己的配置问题（没配 Key 之类）：换一条也许就好了，不算上游的账。
        ProxyError::ConfigError(_) => Fault::Infra,
        ProxyError::InvalidRequest(_) => Fault::ClientRequest,
        // 其余（没有可用线路、内部错误、数据库……）不是换线能解决的。
        _ => Fault::ClientAbort,
    };
    ErrorClass::of(fault, retry_after_ms)
}

/// 客户端自己断开。不换线、不伤熔断（cc-switch 的 `ErrorCategory::ClientAbort`）。
pub fn client_abort() -> ErrorClass {
    ErrorClass::of(Fault::ClientAbort, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sub2guard_label_is_recognised_from_its_own_wording() {
        for (text, want) in [
            (
                "No available channel for model claude-opus-5",
                Fault::ModelMissing,
            ),
            ("model_not_found", Fault::ModelMissing),
            ("该分组下无可用渠道", Fault::ModelMissing),
            ("The model `gpt-9` does not exist", Fault::ModelMissing),
            ("insufficient_account_balance", Fault::Balance),
            ("用户余额不足", Fault::Balance),
            ("Invalid API Key", Fault::Auth),
            ("group_deleted", Fault::Auth),
            ("account has been suspended", Fault::Auth),
            ("this domain is retired", Fault::Retired),
            ("域名已退役", Fault::Retired),
            ("too many concurrent requests", Fault::Concurrency),
            ("并发数超过上限", Fault::Concurrency),
            ("Rate limit exceeded", Fault::RateLimit),
            ("Overloaded", Fault::Overloaded),
            ("server is busy", Fault::Overloaded),
            ("request timed out", Fault::Timeout),
            ("deadline exceeded", Fault::Timeout),
            ("No available accounts", Fault::UpstreamDown),
            ("connection refused", Fault::UpstreamDown),
        ] {
            assert_eq!(classify_text(text), Some(want), "{text}");
        }
        assert_eq!(classify_text("something odd"), None);
        assert_eq!(classify_text("   "), None);
    }

    #[test]
    fn the_text_table_is_ordered_like_sub2guard() {
        // 缺模型排在最前：「model ... not supported」不该被后面的哪一条抢走。
        assert_eq!(
            classify_text("model claude-x not supported, rate limit hint"),
            Some(Fault::ModelMissing)
        );
        // 余额排在认证前面。
        assert_eq!(
            classify_text("insufficient balance; unauthorized"),
            Some(Fault::Balance)
        );
    }

    #[test]
    fn status_codes_are_only_taken_from_their_context_not_from_ids() {
        assert_eq!(
            classify_message("upstream returned 503"),
            Fault::UpstreamDown
        );
        assert_eq!(classify_message("HTTP 401"), Fault::Auth);
        assert_eq!(classify_message("error code: 402"), Fault::Balance);
        // 请求 id 里的 401 / 503 不算。
        assert_eq!(
            classify_message("request req_4015031 failed"),
            Fault::Unknown
        );
    }

    #[test]
    fn cc_switch_client_side_statuses_never_fail_over_and_never_hurt_the_breaker() {
        for status in [400, 405, 406, 413, 414, 415, 422, 501] {
            let c = classify_upstream(
                status,
                Some("{\"error\":{\"message\":\"bad field\"}}"),
                None,
            );
            assert_eq!(c.fault, Fault::ClientRequest, "{status}");
            assert!(!c.retry_next, "{status} 换哪条线都一样，不该换");
            assert_eq!(c.breaker, BreakerVerdict::Neutral, "{status}");
        }
    }

    #[test]
    fn cc_switch_retryable_statuses_fail_over() {
        for status in [401, 403, 404, 408, 409, 429, 451, 500, 502, 503, 504, 529] {
            assert!(classify_upstream(status, None, None).retry_next, "{status}");
        }
    }

    #[test]
    fn a_bare_404_tries_the_next_line_without_hurting_this_one() {
        // QB Gate 原来的规矩（`a_bad_request_is_our_fault_not_the_line_s_fault`）：404 多半是模型名写错，
        // 按故障算连打五次就把健康的线掐掉。cc-switch 的规矩：换下一家试。两条都要成立。
        for status in [404, 409, 451] {
            let c = classify_upstream(status, None, None);
            assert_eq!(c.fault, Fault::NotServedHere, "{status}");
            assert!(c.retry_next, "{status}");
            assert_eq!(c.breaker, BreakerVerdict::Neutral, "{status}");
        }
        // 响应里写着「没这个模型」的 404 照文字归到缺模型（这条线上跳过这个模型）。
        assert_eq!(
            classify_upstream(404, Some("model_not_found"), None).fault,
            Fault::ModelMissing
        );
    }

    #[test]
    fn our_three_breaker_tiers_survive_the_merge() {
        // 429 只降权，带上 Retry-After。
        assert_eq!(
            classify_upstream(429, None, Some(7_000)).breaker,
            BreakerVerdict::RateLimited {
                retry_after_ms: Some(7_000)
            }
        );
        // 401–403 立即熔断（确定性）。
        for status in [401, 402, 403] {
            assert_eq!(
                classify_upstream(status, None, None).breaker,
                BreakerVerdict::Rejected,
                "{status}"
            );
        }
        // 5xx 记一次服务端错误，连续几次才熔断（次数在熔断器里数）。
        assert_eq!(
            classify_upstream(503, None, None).breaker,
            BreakerVerdict::ServerError
        );
    }

    #[test]
    fn wording_beats_the_status_code() {
        // 有的站余额不足回 400：照 cc-switch 的状态码会当成「请求的错」不换线，
        // 而真相是这把 key 没钱了 —— 换线、长熔断。
        let c = classify_upstream(400, Some("{\"error\":\"余额不足\"}"), None);
        assert_eq!(c.fault, Fault::Balance);
        assert!(c.retry_next);
        assert_eq!(c.breaker, BreakerVerdict::Rejected);
        // 缺模型回 500：不算这条线的故障，只跳过这个模型，换别的线试。
        let c = classify_upstream(500, Some("model_not_found"), None);
        assert_eq!(c.fault, Fault::ModelMissing);
        assert!(c.retry_next);
        assert_eq!(c.breaker, BreakerVerdict::Neutral);
    }

    #[test]
    fn local_and_client_side_failures_are_neutral() {
        let c = classify(
            &ProxyError::ConfigError("这条线没有可用的 API Key".into()),
            None,
        );
        assert_eq!(c.fault, Fault::Infra);
        assert!(c.retry_next, "本机这一侧的问题换一条线也许就好了");
        assert_eq!(c.breaker, BreakerVerdict::Neutral);

        let c = client_abort();
        assert!(!c.retry_next);
        assert_eq!(c.breaker, BreakerVerdict::Neutral);
    }

    #[test]
    fn timeouts_and_broken_connections_are_transient() {
        let c = classify(&ProxyError::Timeout("首包超时".into()), None);
        assert_eq!(c.fault, Fault::Timeout);
        assert_eq!(c.breaker, BreakerVerdict::ServerError);
        assert!(c.retry_next);
        let c = classify(
            &ProxyError::ForwardFailed("connection reset by peer".into()),
            None,
        );
        assert_eq!(c.fault, Fault::UpstreamDown);
        let c = classify(&ProxyError::ForwardFailed("tls handshake eof".into()), None);
        assert_eq!(c.fault, Fault::UpstreamDown, "认不出的连接错误算上游挂了");
    }

    #[test]
    fn an_error_envelope_inside_a_200_is_judged_by_its_wording() {
        let c = classify(
            &ProxyError::TransformError("overloaded_error: Overloaded".into()),
            None,
        );
        assert_eq!(c.fault, Fault::Overloaded);
        assert!(c.retry_next);
    }

    #[test]
    fn every_fault_has_a_stable_key_and_a_label() {
        let all = [
            Fault::Balance,
            Fault::Auth,
            Fault::Retired,
            Fault::UpstreamDown,
            Fault::Timeout,
            Fault::Overloaded,
            Fault::Unknown,
            Fault::RateLimit,
            Fault::Concurrency,
            Fault::ModelMissing,
            Fault::Infra,
            Fault::NotServedHere,
            Fault::ClientRequest,
            Fault::ClientAbort,
        ];
        let keys: std::collections::BTreeSet<_> = all.iter().map(|f| f.key()).collect();
        assert_eq!(keys.len(), all.len(), "键名重复了");
        assert!(all.iter().all(|f| !f.label().is_empty()));
    }
}
