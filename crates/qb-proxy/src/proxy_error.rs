// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 本机路由转发一发请求时可能遇到的错误，以及「要不要换下一条线」的分类。
//!
//! 跟 cc-switch 的差别：它用 axum 的 `IntoResponse` 直接出响应；本机路由是 hyper，
//! 所以这里给的是 [`ProxyError::response_parts`]（状态码 + JSON 体），由路由自己拼响应。
//! `describe_reqwest_error` 要认识 reqwest，挪到了 `qb-app::local_router`（这一层不联网）。
use serde_json::{json, Value};
use thiserror::Error;

/// HTTP 状态码。只用来给 [`ProxyError::response_parts`] 起名字，不引 `http` 的类型。
struct StatusCode;

impl StatusCode {
    const CONFLICT: u16 = 409;
    const SERVICE_UNAVAILABLE: u16 = 503;
    const INTERNAL_SERVER_ERROR: u16 = 500;
    const BAD_GATEWAY: u16 = 502;
    const BAD_REQUEST: u16 = 400;
    const UNPROCESSABLE_ENTITY: u16 = 422;
    const GATEWAY_TIMEOUT: u16 = 504;
    const UNAUTHORIZED: u16 = 401;
}

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("上游响应体超过大小上限: {0} 字节")]
    ResponseBodyTooLarge(usize),

    #[error("服务器已在运行")]
    AlreadyRunning,

    #[error("服务器未运行")]
    NotRunning,

    #[error("地址绑定失败: {0}")]
    BindFailed(String),

    #[error("停止超时")]
    StopTimeout,

    #[error("停止失败: {0}")]
    StopFailed(String),

    #[error("请求转发失败: {0}")]
    ForwardFailed(String),

    #[error("无可用的Provider")]
    NoAvailableProvider,

    #[error("所有供应商已熔断，无可用渠道")]
    AllProvidersCircuitOpen,

    #[error("未配置供应商")]
    NoProvidersConfigured,

    #[allow(dead_code)]
    #[error("Provider不健康: {0}")]
    ProviderUnhealthy(String),

    #[error("上游错误 (状态码 {status}): {body:?}")]
    UpstreamError { status: u16, body: Option<String> },

    #[error("超过最大重试次数")]
    MaxRetriesExceeded,

    #[error("数据库错误: {0}")]
    DatabaseError(String),

    #[error("配置错误: {0}")]
    ConfigError(String),

    #[allow(dead_code)]
    #[error("格式转换错误: {0}")]
    TransformError(String),

    #[allow(dead_code)]
    #[error("无效的请求: {0}")]
    InvalidRequest(String),

    #[error("超时: {0}")]
    Timeout(String),

    /// 流式响应空闲超时
    #[allow(dead_code)]
    #[error("流式响应空闲超时: {0}秒无数据")]
    StreamIdleTimeout(u64),

    /// 认证错误
    #[error("认证失败: {0}")]
    AuthError(String),

    #[allow(dead_code)]
    #[error("内部错误: {0}")]
    Internal(String),
}

impl ProxyError {
    /// 回给客户端的状态码和 JSON 体（cc-switch 里是 `IntoResponse::into_response`）。
    ///
    /// 上游自己的错误原样透传（是 JSON 就照转，不是就包一层）；本机路由自己的错误
    /// 包成 `{"error":{"message","type":"proxy_error"}}`。
    pub fn response_parts(&self) -> (u16, Value) {
        match self {
            ProxyError::UpstreamError {
                status: upstream_status,
                body: upstream_body,
            } => {
                let http_status = if (100..=999).contains(upstream_status) {
                    *upstream_status
                } else {
                    StatusCode::BAD_GATEWAY
                };

                // 尝试解析上游响应体为 JSON，如果失败则包装为字符串
                let error_body = if let Some(body_str) = upstream_body {
                    if let Ok(json_body) = serde_json::from_str::<serde_json::Value>(body_str) {
                        // 上游返回的是 JSON，直接透传
                        json_body
                    } else {
                        // 上游返回的不是 JSON，包装为错误消息
                        json!({
                            "error": {
                                "message": body_str,
                                "type": "upstream_error",
                            }
                        })
                    }
                } else {
                    json!({
                        "error": {
                            "message": format!("Upstream error (status {})", upstream_status),
                            "type": "upstream_error",
                        }
                    })
                };

                (http_status, error_body)
            }
            _ => {
                let (http_status, message) = match self {
                    ProxyError::AlreadyRunning => (StatusCode::CONFLICT, self.to_string()),
                    ProxyError::NotRunning => (StatusCode::SERVICE_UNAVAILABLE, self.to_string()),
                    ProxyError::BindFailed(_) => {
                        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
                    }
                    ProxyError::StopTimeout => {
                        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
                    }
                    ProxyError::StopFailed(_) => {
                        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
                    }
                    ProxyError::ForwardFailed(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
                    ProxyError::NoAvailableProvider => {
                        (StatusCode::SERVICE_UNAVAILABLE, self.to_string())
                    }
                    ProxyError::AllProvidersCircuitOpen => {
                        (StatusCode::SERVICE_UNAVAILABLE, self.to_string())
                    }
                    ProxyError::NoProvidersConfigured => {
                        (StatusCode::SERVICE_UNAVAILABLE, self.to_string())
                    }
                    ProxyError::ProviderUnhealthy(_) => {
                        (StatusCode::SERVICE_UNAVAILABLE, self.to_string())
                    }
                    ProxyError::MaxRetriesExceeded => {
                        (StatusCode::SERVICE_UNAVAILABLE, self.to_string())
                    }
                    ProxyError::DatabaseError(_) => {
                        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
                    }
                    ProxyError::ConfigError(_) => (StatusCode::BAD_REQUEST, self.to_string()),
                    ProxyError::TransformError(_) => {
                        (StatusCode::UNPROCESSABLE_ENTITY, self.to_string())
                    }
                    ProxyError::InvalidRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
                    ProxyError::Timeout(_) => (StatusCode::GATEWAY_TIMEOUT, self.to_string()),
                    ProxyError::StreamIdleTimeout(_) => {
                        (StatusCode::GATEWAY_TIMEOUT, self.to_string())
                    }
                    ProxyError::AuthError(_) => (StatusCode::UNAUTHORIZED, self.to_string()),
                    ProxyError::Internal(_) => {
                        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
                    }
                    ProxyError::ResponseBodyTooLarge(_) => {
                        (StatusCode::BAD_GATEWAY, self.to_string())
                    }
                    ProxyError::UpstreamError { .. } => unreachable!(),
                };

                let error_body = json!({
                    "error": {
                        "message": message,
                        "type": "proxy_error",
                    }
                });

                (http_status, error_body)
            }
        }
    }
}

/// 错误分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// 可重试错误（网络问题、5xx）
    Retryable, // 网络超时、5xx 错误
    /// 不可重试错误（4xx、认证失败）
    NonRetryable, // 认证失败、参数错误、4xx 错误
    #[allow(dead_code)]
    ClientAbort, // 客户端主动中断
}

/// 把错误连同 `source()` 原因链拼成一行。
///
/// reqwest / hyper 的外层文本只有「error sending request」「error decoding response body」
/// 这类描述，真正能区分 DNS、TCP、TLS、连接被重置的信息都在原因链里。
pub fn describe_error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let cause_text = cause.to_string();
        // 有的错误会把原因也写进自己的文本，重复的就不再追加
        if !cause_text.is_empty() && !text.ends_with(&cause_text) {
            text.push_str(": ");
            text.push_str(&cause_text);
        }
        source = cause.source();
    }
    text
}

// cc-switch 在这里还有 `describe_reqwest_error` / `categorize_error`，都要认识 reqwest 的错误类型，
// 挪到了 `qb-app::local_router::describe_reqwest_error`（这一层不联网，不依赖 reqwest）。

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Outer(std::io::Error);

    impl std::fmt::Display for Outer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("error sending request")
        }
    }

    impl std::error::Error for Outer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    #[test]
    fn describe_error_chain_appends_every_cause() {
        let error = Outer(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "tcp connect error",
        ));
        assert_eq!(
            describe_error_chain(&error),
            "error sending request: tcp connect error"
        );
    }

    #[test]
    fn upstream_errors_pass_through_and_proxy_errors_are_wrapped() {
        let (status, body) = ProxyError::UpstreamError {
            status: 429,
            body: Some(r#"{"error":{"message":"slow down"}}"#.into()),
        }
        .response_parts();
        assert_eq!(status, 429);
        assert_eq!(body["error"]["message"], "slow down");

        let (status, body) = ProxyError::UpstreamError {
            status: 502,
            body: Some("plain text".into()),
        }
        .response_parts();
        assert_eq!(status, 502);
        assert_eq!(body["error"]["message"], "plain text");
        assert_eq!(body["error"]["type"], "upstream_error");

        let (status, body) = ProxyError::Timeout("首包超时".into()).response_parts();
        assert_eq!(status, 504);
        assert_eq!(body["error"]["type"], "proxy_error");
    }
}
