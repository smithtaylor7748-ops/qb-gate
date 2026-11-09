//! 人工发起的单次文本质量检测。批次只限额，不后台续跑、不自动选站或换官方账户。

mod official;
mod parse;
mod registry;
mod relay;

use crate::error::{GateError, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum QualityTargetKind {
    CodexAccount,
    Relay,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QualityTarget {
    pub kind: QualityTargetKind,
    pub id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QualityCheckRequest {
    pub run_id: String,
    pub request_id: String,
    pub target: QualityTarget,
    pub model: String,
    pub effort: String,
    pub prompt: String,
    pub max_output_tokens: u32,
    pub timeout_secs: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum QualityCheckStatus {
    Completed,
    Failed,
    Cancelled,
    Timeout,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct QualityCheckResult {
    pub request_id: String,
    pub target_id: String,
    pub target_label: String,
    pub target_kind: QualityTargetKind,
    pub requested_model: String,
    /// 上游在响应中声明的模型，不是独立核实的模型身份。
    pub actual_model: Option<String>,
    /// 进程内加盐的来源修订标记，只用于发现条件变化，不证明模型/账号身份。
    pub source_stamp: Option<String>,
    pub protocol: String,
    pub status: QualityCheckStatus,
    pub text: String,
    #[ts(type = "number | null")]
    pub input_tokens: Option<u64>,
    #[ts(type = "number | null")]
    pub output_tokens: Option<u64>,
    #[ts(type = "number | null")]
    pub cache_read_tokens: Option<u64>,
    #[ts(type = "number | null")]
    pub reasoning_tokens: Option<u64>,
    #[ts(type = "number")]
    pub elapsed_ms: u64,
    #[ts(type = "number | null")]
    pub first_token_ms: Option<u64>,
    pub error: Option<String>,
    /// 官方 CLI 没有可核实的输出 token 限额接口；该目标恒为 false。
    pub output_limit_enforced: bool,
}

impl QualityCheckResult {
    fn blank(request: &QualityCheckRequest) -> Self {
        Self {
            request_id: request.request_id.clone(),
            target_id: request.target.id.clone(),
            target_label: request.target.id.clone(),
            target_kind: request.target.kind,
            requested_model: request.model.clone(),
            actual_model: None,
            source_stamp: None,
            protocol: match request.target.kind {
                QualityTargetKind::CodexAccount => "codex-cli",
                QualityTargetKind::Relay => "responses",
            }
            .into(),
            status: QualityCheckStatus::Failed,
            text: String::new(),
            input_tokens: None,
            output_tokens: None,
            cache_read_tokens: None,
            reasoning_tokens: None,
            elapsed_ms: 0,
            first_token_ms: None,
            error: None,
            output_limit_enforced: request.target.kind == QualityTargetKind::Relay,
        }
    }
}

pub fn validate(request: &QualityCheckRequest) -> Result<()> {
    if uuid::Uuid::parse_str(&request.run_id).is_err()
        || uuid::Uuid::parse_str(&request.request_id).is_err()
    {
        return Err(GateError::Other("检测批次和请求 ID 必须为 UUID".into()));
    }
    if request.target.id.is_empty()
        || request.target.id.len() > 512
        || request.model.trim().is_empty()
        || request.model.len() > 160
        || request.model.chars().any(char::is_control)
    {
        return Err(GateError::Other("请选择有效的检测目标和模型".into()));
    }
    if request.model == crate::gpt_bridge::DEFAULT_MODEL_LABEL {
        return Err(GateError::Other(
            "质量检测需要具体模型名，不能使用酒馆默认模型占位值".into(),
        ));
    }
    if ![
        "", "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
    ]
    .contains(&request.effort.as_str())
    {
        return Err(GateError::Other("不支持的推理强度".into()));
    }
    if request.prompt.trim().is_empty()
        || request.prompt.len() > 128 * 1024
        || request.prompt.chars().count() > 24_000
    {
        return Err(GateError::Other(
            "检测提示词须为 1–24000 字符，且不超过 128 KiB".into(),
        ));
    }
    if !(16..=8192).contains(&request.max_output_tokens)
        || !(1..=600).contains(&request.timeout_secs)
    {
        return Err(GateError::Other(
            "输出上限须为 16–8192 token，单次超时须为 1–600 秒".into(),
        ));
    }
    Ok(())
}

pub fn cancel(request_id: &str) -> Result<bool> {
    registry::cancel(request_id)
}

/// 门禁和全局停止在所有样本间隙也撤销批次，后续样本不得自动重新放行。
pub fn gate_stopped(all_sessions: bool) {
    registry::gate_stopped(all_sessions);
}

/// 使用同一出站代理配置，但检测禁止 HTTP 重定向和备用地址，避免结果属于另一目标。
pub struct QualityRelayHttp {
    client: reqwest::Client,
    fingerprint: String,
}

pub fn relay_client(network: &qb_station::router::RouterNetwork) -> Result<QualityRelayHttp> {
    use sha2::{Digest, Sha256};
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15));
    if let Some(url) = network.outbound_proxy.as_deref() {
        qb_station::router::check_proxy_url(url).map_err(GateError::Other)?;
        builder = builder.proxy(
            reqwest::Proxy::all(url)
                .map_err(|_| GateError::Other("检测出站代理配置无效".into()))?,
        );
    }
    Ok(QualityRelayHttp {
        client: builder
            .build()
            .map_err(|_| GateError::Other("无法建立检测 HTTP 客户端".into()))?,
        fingerprint: hex::encode(Sha256::digest(
            network.outbound_proxy.as_deref().unwrap_or("").as_bytes(),
        )),
    })
}

/// 调用方传入的回调只负责挂已有看门狗，不改变选中账户或中转线路。
pub async fn run(
    request: QualityCheckRequest,
    router: Arc<Mutex<crate::local_router::RouterState>>,
    gate: Arc<crate::gate::GateState>,
    relay_http: Option<QualityRelayHttp>,
    start_watchdog: impl Fn() + Send,
) -> Result<QualityCheckResult> {
    validate(&request)?;
    let mut result = QualityCheckResult::blank(&request);
    let Some(mut ticket) = registry::begin(&request)? else {
        result.status = QualityCheckStatus::Cancelled;
        return Ok(result);
    };
    let started = Instant::now();
    let run = async {
        match request.target.kind {
            QualityTargetKind::CodexAccount => {
                official::run(&request, &mut result, &gate, start_watchdog).await
            }
            QualityTargetKind::Relay => {
                relay::run(
                    &request,
                    &mut result,
                    &router,
                    relay_http
                        .as_ref()
                        .ok_or_else(|| GateError::Other("检测 HTTP 客户端未就绪".into()))?,
                )
                .await
            }
        }
    };
    let outcome = tokio::select! {
        biased;
        _ = ticket.cancelled() => None,
        outcome = tokio::time::timeout(Duration::from_secs(u64::from(request.timeout_secs)), run) => Some(outcome),
    };
    match outcome {
        None => result.status = QualityCheckStatus::Cancelled,
        Some(Err(_)) => {
            result.status = QualityCheckStatus::Timeout;
            result.error = Some("检测超时，已取消本次请求及其专属子进程".into());
        }
        Some(Ok(Err(error))) => {
            result.status = QualityCheckStatus::Failed;
            result.error = Some(error.to_string());
        }
        Some(Ok(Ok(()))) => result.status = QualityCheckStatus::Completed,
    }
    if registry::revoked(&request.run_id) {
        result.status = QualityCheckStatus::Cancelled;
    }
    result.elapsed_ms = started.elapsed().as_millis() as u64;
    // 结果不含请求头、凭据、原始 CLI stderr；未知用量维持 None。
    result.text = redact(&result.text);
    result.error = result.error.map(|error| redact(&error));
    Ok(result)
}

/// 只替换明显的凭据形状；不收集认证文件来做匹配。
pub(super) fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for piece in text.split_inclusive(char::is_whitespace) {
        let token = piece.trim();
        let secret = (token.contains("sk-") && token.len() > 20)
            || (token.starts_with("eyJ") && token.matches('.').count() == 2);
        if secret {
            out.push_str("[凭据已隐藏]");
            if piece.ends_with(char::is_whitespace) {
                out.push(' ');
            }
        } else {
            out.push_str(piece);
        }
    }
    out
}
