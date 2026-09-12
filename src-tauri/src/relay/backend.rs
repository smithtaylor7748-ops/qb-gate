//! 中转站的**真实后端**是谁。
//!
//! 你买的那条「Claude 官方」，背后可能是三样东西之一：
//!
//! | 后端 | 谁在这么干 | 看得出来的痕迹 |
//! |---|---|---|
//! | Anthropic | 官方 API Key、Max 订阅转发 | `anthropic-ratelimit-*`、`request-id: req_…` |
//! | AWS Bedrock | Kiro 逆向 | `x-amzn-requestid` / `x-amzn-trace-id`，或者一个 `anthropic-*` 头都没有 |
//! | Google Vertex | Antigravity 逆向 | `x-goog-*`、Google 的 `server` 头 |
//!
//! 判定表与「缺字段负证据」这个思路抄自 cc-proxy-detector（MIT），
//! 实现是 Rust 重写的。原版还会做行为异常分析（tool_use 配对错误、
//! 间歇 500、多模态读图失败），那些要真跑一轮会话才看得出来，不适合
//! 放在一个点一下就出结果的按钮里，所以这里只做一次请求能看出来的部分。
//!
//! # 为什么值得做
//!
//! 这是唯一一个使用者**自己查不出来**的事：中转站说自己是官方，
//! 响应也确实是 Claude 的格式，肉眼没有任何差别。但逆向通道在限流口径、
//! 可用模型、长上下文行为上都不一样，出了问题他只会觉得「Claude 今天有点傻」。
//!
//! # 铁律（与 `probe.rs` 同）
//!
//! **这个动作会把 API Key 发到用户填的那个地址上。** 只能由明确的按钮触发，
//! 不轮询、不 onChange、不在列表渲染时顺手探一遍。

use serde::Serialize;
use std::time::Duration;

use crate::error::{GateError, Result};

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Anthropic,
    Bedrock,
    Vertex,
    /// 证据不够。**这不是「检测失败」**，是「看不出来」—— 两者要分开显示。
    Unsure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// 有直接指纹（对方自己的头还在）。
    Strong,
    /// 只有负证据（该有的头没有），或者只有弱信号。
    Weak,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendReport {
    pub backend: Backend,
    pub confidence: Confidence,
    /// 猜到的逆向来源，比如 `Kiro`。没把握就是 `None`，**不硬猜**。
    pub source: Option<String>,
    /// 逐条人话证据。界面上原样列出来 —— 让使用者自己判断可信度，
    /// 而不是只给一个他没法复核的结论。
    pub evidence: Vec<String>,
    /// 限流头是不是真的（连发两次看计数动没动）。
    /// `None` = 没测（对方压根没给限流头）。
    pub ratelimit_real: Option<bool>,
    pub detail: String,
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| GateError::Other(format!("建 HTTP 客户端失败：{e}")))
}

fn join(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

/// 一次探测收上来的原始痕迹。判定是纯函数 `classify`，方便单测。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Traces {
    /// 全部响应头，名字已转小写。
    pub headers: Vec<(String, String)>,
    /// 响应体里的 `id` 字段（Anthropic 原生是 `msg_…`）。
    pub body_id: Option<String>,
    /// 响应体里的 `model` 字段。逆向通道常常回一个带前缀的名字。
    pub body_model: Option<String>,
    pub status: u16,
}

impl Traces {
    fn get(&self, k: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
    }

    fn has_prefix(&self, p: &str) -> bool {
        self.headers.iter().any(|(n, _)| n.starts_with(p))
    }
}

/// 判定。**纯函数，不联网**，所有分支可单测。
pub fn classify(t: &Traces) -> BackendReport {
    let mut evidence = Vec::new();

    let amzn = t.has_prefix("x-amzn-") || t.has_prefix("x-amz-");
    let goog = t.has_prefix("x-goog-")
        || t
            .get("server")
            .map(|s| s.contains("ESF") || s.contains("scaffolding"))
            .unwrap_or(false);
    let anthropic_rl = t.has_prefix("anthropic-ratelimit-");
    let req_id_native = t
        .get("request-id")
        .map(|v| v.starts_with("req_"))
        .unwrap_or(false);
    let msg_id_native = t
        .body_id
        .as_deref()
        .map(|v| v.starts_with("msg_"))
        .unwrap_or(false);

    // ---- 直接指纹：对方自己的头还留着，这是最硬的证据。
    if amzn {
        evidence.push("响应里带 AWS 的 `x-amzn-*` 头 —— 这是 Bedrock 直出的痕迹".into());
        return BackendReport {
            backend: Backend::Bedrock,
            confidence: Confidence::Strong,
            source: kiro_hint(t),
            evidence,
            ratelimit_real: None,
            detail: "后端是 AWS Bedrock，不是 Anthropic 官方。".into(),
        };
    }
    if goog {
        evidence.push("响应里带 Google 的 `x-goog-*` 或 Google 前端的 `server` 头".into());
        return BackendReport {
            backend: Backend::Vertex,
            confidence: Confidence::Strong,
            source: Some("Antigravity / Google Cloud Code".into()),
            evidence,
            ratelimit_real: None,
            detail: "后端是 Google Vertex AI，不是 Anthropic 官方。".into(),
        };
    }

    // ---- 正面证据：Anthropic 自己的限流头 + 原生 id 形状。
    if anthropic_rl {
        evidence.push("带 `anthropic-ratelimit-*` 限流头".into());
    }
    if req_id_native {
        evidence.push("`request-id` 是 Anthropic 的 `req_` 形状".into());
    }
    if msg_id_native {
        evidence.push("响应体 `id` 是原生的 `msg_` 形状".into());
    }
    if anthropic_rl && (req_id_native || msg_id_native) {
        return BackendReport {
            backend: Backend::Anthropic,
            confidence: Confidence::Strong,
            source: None,
            evidence,
            ratelimit_real: None,
            detail: "痕迹与 Anthropic 官方一致。".into(),
        };
    }

    // ---- 缺字段负证据：回的是 Claude 格式，却一个 Anthropic 的头都没有。
    //
    // 这是抓「深度伪装」的关键一招 —— 转换层能把响应体做得一模一样，
    // 却很难把上游本来就没有的头凭空造全。
    if t.status == 200 && !anthropic_rl && !req_id_native {
        evidence.push(
            "响应是 Claude 格式，但**一个 `anthropic-*` 头都没有** —— \
             官方直连不会这样，多半中间有一层格式转换"
                .into(),
        );
        return BackendReport {
            backend: Backend::Unsure,
            confidence: Confidence::Weak,
            source: None,
            evidence,
            ratelimit_real: None,
            detail: "不是官方直连，但看不出具体是哪个后端。".into(),
        };
    }

    if !evidence.is_empty() {
        return BackendReport {
            backend: Backend::Anthropic,
            confidence: Confidence::Weak,
            source: None,
            evidence,
            ratelimit_real: None,
            detail: "有 Anthropic 的痕迹，但证据不够硬。".into(),
        };
    }

    // 什么都没看出来就说什么都没看出来。**不猜。**
    BackendReport {
        backend: Backend::Unsure,
        confidence: Confidence::Weak,
        source: None,
        evidence: vec![format!("HTTP {}，没有任何可用于判定的头", t.status)],
        ratelimit_real: None,
        detail: "证据不足，说不准。".into(),
    }
}

/// 逆向来源的进一步线索。拿不准就返回 `None`。
fn kiro_hint(t: &Traces) -> Option<String> {
    let m = t.body_model.as_deref()?;
    if m.starts_with("kiro-") || m.contains("kiro") {
        return Some("Kiro（AWS AI IDE 逆向）".into());
    }
    None
}

/// 真跑一次。会把 Key 发给对方 —— 只能由按钮触发。
pub async fn detect(base_url: &str, api_key: Option<&str>, model: Option<&str>) -> Result<BackendReport> {
    let first = probe_once(base_url, api_key, model).await?;
    let mut report = classify(&first);

    // 限流头真伪：再发一次，看计数是不是真的在动。
    // 伪造的限流头通常是写死的常量 —— 第二次请求它一动不动。
    if first.get("anthropic-ratelimit-requests-remaining").is_some() {
        if let Ok(second) = probe_once(base_url, api_key, model).await {
            let a = first.get("anthropic-ratelimit-requests-remaining");
            let b = second.get("anthropic-ratelimit-requests-remaining");
            let moved = a != b;
            report.ratelimit_real = Some(moved);
            report.evidence.push(if moved {
                "限流计数两次请求之间确实在变 —— 像是真的".into()
            } else {
                "限流计数两次请求之间**一动不动** —— 像是写死的假头".into()
            });
            if !moved && report.backend == Backend::Anthropic {
                report.confidence = Confidence::Weak;
                report.detail = "有 Anthropic 的痕迹，但限流头疑似伪造。".into();
            }
        }
    }
    Ok(report)
}

async fn probe_once(base_url: &str, api_key: Option<&str>, model: Option<&str>) -> Result<Traces> {
    let url = join(base_url, "v1/messages");
    let body = serde_json::json!({
        "model": model.unwrap_or("claude-sonnet-4-5"),
        "max_tokens": 1,
        "messages": [{ "role": "user", "content": "hi" }],
    });

    let mut req = client()?
        .post(&url)
        .header("content-type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .json(&body);
    if let Some(k) = api_key.filter(|k| !k.is_empty()) {
        // 两种都带上：中转站有的认 x-api-key，有的认 Authorization。
        req = req.header("x-api-key", k).bearer_auth(k);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| GateError::Other(format!("请求 {url} 失败：{e}")))?;

    let status = resp.status().as_u16();
    let headers = resp
        .headers()
        .iter()
        .map(|(n, v)| (n.as_str().to_lowercase(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let json: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);

    Ok(Traces {
        headers,
        body_id: json.get("id").and_then(|v| v.as_str()).map(String::from),
        body_model: json.get("model").and_then(|v| v.as_str()).map(String::from),
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn traces(headers: &[(&str, &str)], id: Option<&str>, status: u16) -> Traces {
        Traces {
            headers: headers
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            body_id: id.map(String::from),
            body_model: None,
            status,
        }
    }

    #[test]
    fn aws_headers_mean_bedrock() {
        let r = classify(&traces(&[("x-amzn-requestid", "abc")], None, 200));
        assert_eq!(r.backend, Backend::Bedrock);
        assert_eq!(r.confidence, Confidence::Strong);
    }

    #[test]
    fn google_frontend_means_vertex() {
        let r = classify(&traces(&[("server", "ESF")], None, 200));
        assert_eq!(r.backend, Backend::Vertex);
        assert!(r.source.is_some());
    }

    #[test]
    fn ratelimit_plus_native_id_means_anthropic() {
        let r = classify(&traces(
            &[
                ("anthropic-ratelimit-requests-limit", "50"),
                ("request-id", "req_011CQ"),
            ],
            Some("msg_01X"),
            200,
        ));
        assert_eq!(r.backend, Backend::Anthropic);
        assert_eq!(r.confidence, Confidence::Strong);
    }

    /// 缺字段负证据：响应体做得再像，也造不出上游本来没有的头。
    #[test]
    fn claude_shaped_response_without_any_anthropic_header_is_suspicious() {
        let r = classify(&traces(&[("content-type", "application/json")], Some("msg_01X"), 200));
        assert_eq!(r.backend, Backend::Unsure);
        assert!(
            r.evidence.iter().any(|e| e.contains("一个 `anthropic-*` 头都没有")),
            "负证据必须写进去，否则使用者不知道结论是怎么来的"
        );
    }

    /// **证据不足要说证据不足**，不能猜一个「应该是官方吧」。
    /// 与纯净度那套「拿不到的字段如实报未知，不猜成通过」是同一条。
    #[test]
    fn no_evidence_means_unsure_not_a_guess() {
        let r = classify(&traces(&[], None, 401));
        assert_eq!(r.backend, Backend::Unsure);
        assert_eq!(r.confidence, Confidence::Weak);
    }

    #[test]
    fn kiro_is_named_when_the_model_gives_it_away() {
        let mut t = traces(&[("x-amzn-requestid", "abc")], None, 200);
        t.body_model = Some("kiro-claude-sonnet-4".into());
        let r = classify(&t);
        assert_eq!(r.backend, Backend::Bedrock);
        assert!(r.source.as_deref().unwrap().contains("Kiro"));
    }

    #[test]
    fn header_lookup_is_case_insensitive_by_construction() {
        // probe_once 收头时统一转小写，这里断言查找依赖的就是这个约定。
        let t = traces(&[("anthropic-ratelimit-requests-limit", "50")], None, 200);
        assert!(t.has_prefix("anthropic-ratelimit-"));
        assert!(t.get("anthropic-ratelimit-requests-limit").is_some());
    }
}
