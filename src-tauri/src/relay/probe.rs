//! 两个联网小工具：拉模型列表、测端点延迟。
//!
//! # 都是用户点了才跑
//!
//! 这两个动作会**把 API Key 发到用户填的那个地址上**。绝不能自动触发 ——
//! 用户可能只是在表单里打字，地址还没填完。所以没有轮询、没有 onChange
//! 探测，只有明确的按钮。

use serde::Serialize;
use std::time::{Duration, Instant};

use crate::error::{GateError, Result};

/// 单次请求的上限。中转站慢是常态，但 15 秒还没响应就该告诉用户了。
const TIMEOUT: Duration = Duration::from_secs(15);

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| GateError::Other(format!("建 HTTP 客户端失败：{e}")))
}

/// `base_url` 后面接一段路径。
///
/// 用户填的地址结尾带不带 `/` 都得能用 —— 这是最常见的一种「配了却不通」。
fn join(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelList {
    pub models: Vec<String>,
    pub detail: String,
}

/// 调 OpenAI 兼容的 `/models`，把可用模型列出来。
///
/// 不是每家都提供这个端点。**拿不到就如实说拿不到**，让用户手填模型 id，
/// 不要伪造一个列表。
pub async fn fetch_models(base_url: &str, api_key: Option<&str>) -> Result<ModelList> {
    let url = join(base_url, "models");
    let mut req = client()?.get(&url);
    if let Some(k) = api_key.filter(|k| !k.is_empty()) {
        req = req.bearer_auth(k);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| GateError::Other(format!("请求 {url} 失败：{e}")))?;

    let status = resp.status();
    if !status.is_success() {
        let hint = match status.as_u16() {
            401 | 403 => "认证失败，检查 API Key",
            404 | 405 => "这家没有提供 /models 端点，模型 id 需要手填",
            _ => "上游返回了错误",
        };
        return Err(GateError::Other(format!("{hint}（HTTP {status}）")));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| GateError::Other(format!("返回的不是 JSON：{e}")))?;

    // OpenAI 形状是 { data: [ { id } ] }；有些家直接给数组。
    let items = body
        .get("data")
        .and_then(|v| v.as_array())
        .or_else(|| body.as_array())
        .ok_or_else(|| GateError::Other("返回内容不符合 OpenAI 兼容格式".into()))?;

    let mut models: Vec<String> = items
        .iter()
        .filter_map(|m| {
            m.get("id")
                .and_then(|v| v.as_str())
                .or_else(|| m.as_str())
                .map(String::from)
        })
        .collect();
    models.sort();
    models.dedup();

    let detail = if models.is_empty() {
        "端点通了，但没返回任何模型".into()
    } else {
        format!("拿到 {} 个模型", models.len())
    };
    Ok(ModelList { models, detail })
}

#[derive(Debug, Clone, Serialize)]
pub struct LatencyResult {
    pub base_url: String,
    /// 毫秒。`None` = 没连上。
    pub ms: Option<u64>,
    pub ok: bool,
    pub detail: String,
}

/// 测一个端点的往返延迟。
///
/// **测的是「连得上、多久」，不是「Key 对不对」** —— 401 也算连上了，
/// 因为那说明网络通、服务在。把 401 记成失败会让用户去查网络，
/// 而问题其实在 Key 上。
pub async fn measure(base_url: &str, api_key: Option<&str>) -> LatencyResult {
    let url = join(base_url, "models");
    let started = Instant::now();

    let built = match client() {
        Ok(c) => c,
        Err(e) => {
            return LatencyResult {
                base_url: base_url.into(),
                ms: None,
                ok: false,
                detail: e.to_string(),
            }
        }
    };
    let mut req = built.get(&url);
    if let Some(k) = api_key.filter(|k| !k.is_empty()) {
        req = req.bearer_auth(k);
    }

    match req.send().await {
        Ok(r) => {
            let ms = started.elapsed().as_millis() as u64;
            let status = r.status();
            LatencyResult {
                base_url: base_url.into(),
                ms: Some(ms),
                // 连上了就算通。401 说明服务在，只是 Key 不对。
                ok: true,
                detail: if status.is_success() {
                    format!("{ms} ms")
                } else {
                    format!("{ms} ms（HTTP {status}，服务在，但这次没通过认证）")
                },
            }
        }
        Err(e) => LatencyResult {
            base_url: base_url.into(),
            ms: None,
            ok: false,
            detail: if e.is_timeout() {
                format!("超时（超过 {} 秒）", TIMEOUT.as_secs())
            } else {
                format!("连不上：{e}")
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_slash_does_not_double_up() {
        // 「配了却不通」里最常见的一种。
        assert_eq!(join("https://a.com/v1", "models"), "https://a.com/v1/models");
        assert_eq!(join("https://a.com/v1/", "models"), "https://a.com/v1/models");
        assert_eq!(join("https://a.com/v1//", "/models"), "https://a.com/v1/models");
    }
}
