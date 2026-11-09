//! 内部人工检测入口；没有可由外部 HTTP 请求控制的 pin header。
use super::log::Record;
use super::*;
use qb_proxy::convert::plan::ApiFormat;
use std::time::Instant;

pub(crate) struct Target {
    source: Upstream,
    pub label: String,
    pub model: String,
    pub chat: bool,
}

impl Target {
    pub(crate) fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        // 只存不可逆摘要，既不向前端返回凭据，也不把凭据写入日志。
        let mut hash = Sha256::new();
        for piece in [
            &self.source.route_id,
            &self.source.base_url,
            &format!("{:?}", self.source.auth),
            &format!("{:?}", self.source.format),
            &format!("{:?}", self.source.alternates),
            &format!("{:?}", self.source.model_map),
            &format!("{:?}", self.source.model_exact),
        ] {
            hash.update((piece.len() as u64).to_le_bytes());
            hash.update(piece.as_bytes());
        }
        hex::encode(hash.finalize())
    }
    pub(crate) fn capture(state: &Arc<Mutex<RouterState>>, id: &str, model: &str) -> Result<Self> {
        let s = state
            .lock()
            .map_err(|_| GateError::Other("中转站状态不可用".into()))?;
        let source = s
            .upstreams
            .get(id)
            .filter(|u| {
                id.starts_with("codex\u{1f}")
                    && !matches!(u.auth, UpstreamAuth::OAuthPassthrough)
                    && s.route_allowed(Client::Codex, id)
            })
            .ok_or_else(|| {
                GateError::Other(
                    "检测线路未获授权或已删除：请先选为 Codex 当前线路，或加入已启用的授权池"
                        .into(),
                )
            })?
            .clone();
        let chat = match source.format.unwrap_or(ApiFormat::OpenaiResponses) {
            ApiFormat::OpenaiResponses => false,
            ApiFormat::OpenaiChat => true,
            _ => return Err(GateError::Other("文本质量检测仅支持 Codex 的 Responses / Chat Completions 线路；请使用对应协议的线路".into())),
        };
        Ok(Self {
            label: source.label.clone(),
            model: source.map_model(model).unwrap_or_else(|| model.to_owned()),
            chat,
            source,
        })
    }

    pub(crate) fn redact(&self, text: &str) -> String {
        let secret = match &self.source.auth {
            UpstreamAuth::Bearer(k) | UpstreamAuth::ApiKey(k) | UpstreamAuth::GoogleApiKey(k) => {
                k.as_str()
            }
            _ => "",
        };
        if secret.is_empty() {
            text.to_owned()
        } else {
            text.replace(secret, "[凭据已隐藏]")
        }
    }

    pub(crate) fn record(
        &self,
        state: &Arc<Mutex<RouterState>>,
        request_id: &str,
        requested_model: &str,
        started: Instant,
    ) -> Record {
        Record::new(
            Arc::clone(state),
            Client::Codex,
            started,
            RouterRequest {
                id: format!("quality-{request_id}"),
                at_ms: chrono::Utc::now().timestamp_millis(),
                client: "codex".into(),
                route_id: self.source.route_id.clone(),
                model: requested_model.into(),
                upstream_model: self.model.clone(),
                streaming: true,
                // 若 future 被取消，Drop 仍留下一条明确的未完成记录。
                fault: Some("quality-cancelled".into()),
                error: Some("人工检测未完整结束".into()),
                ..Default::default()
            },
        )
        .independent()
    }

    pub(crate) async fn send(
        &self,
        state: &Arc<Mutex<RouterState>>,
        http: &reqwest::Client,
        body: &serde_json::Value,
        started: Instant,
    ) -> Result<(reqwest::Response, AdmissionPermit)> {
        let path = if self.chat {
            "/v1/chat/completions"
        } else {
            "/v1/responses"
        };
        let mut builder = http
            .post(join_url(&self.source.base_url, path))
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .json(body);
        match &self.source.auth {
            UpstreamAuth::Bearer(key) => builder = builder.bearer_auth(key),
            UpstreamAuth::ApiKey(key) => builder = builder.header("x-api-key", key),
            UpstreamAuth::GoogleApiKey(key) => builder = builder.header("x-goog-api-key", key),
            UpstreamAuth::None => {}
            UpstreamAuth::OAuthPassthrough => {
                return Err(GateError::Other("官方 OAuth 线路不接受检测请求".into()))
            }
        }
        // 构造请求之后再原子核验快照和授权；等待期间编辑、删除或撤权都阻止旧来源发出。
        let request = builder
            .build()
            .map_err(|_| GateError::Other("检测线路 URL 或认证头无效".into()))?;
        let permit = admission::acquire_from(
            state,
            Client::Codex,
            &self.source.route_id,
            started,
            Some(&self.source),
            admission::WaitPolicy {
                may_wait: true,
                strict_source: true,
                ..Default::default()
            },
        )
        .await
        .map_err(|r| GateError::Other(format!("检测请求未发送：线路准入拒绝（{}）", r.reason)))?;
        let response = http.execute(request).await.map_err(|e| {
            GateError::Other(if e.is_timeout() {
                "检测线路连接超时".into()
            } else {
                "检测线路连接失败；请检查地址和出站网络".into()
            })
        })?;
        Ok((response, permit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_target_requires_real_codex_permission_and_preserves_selection() {
        let id = "codex\u{1f}site\u{1f}group";
        let mut s = RouterState::default();
        s.put_upstream(Upstream {
            route_id: id.into(),
            base_url: "https://example.invalid/v1".into(),
            ..Default::default()
        });
        let state = Arc::new(Mutex::new(s));
        assert!(Target::capture(&state, id, "gpt").is_err());
        state.lock().unwrap().set_now(Client::Codex, id);
        let captured = Target::capture(&state, id, "gpt").unwrap();
        let before = state.lock().unwrap().current(Client::Codex);
        drop(captured.record(&state, "test", "gpt", Instant::now()));
        assert_eq!(state.lock().unwrap().current(Client::Codex), before);
        state.lock().unwrap().remove_upstream(id);
        assert!(Target::capture(&state, id, "gpt").is_err());
    }

    #[tokio::test]
    async fn edited_source_is_rejected_before_any_send() {
        let id = "codex\u{1f}site\u{1f}group";
        let original = Upstream {
            route_id: id.into(),
            base_url: "https://old.invalid/v1".into(),
            ..Default::default()
        };
        let mut s = RouterState::default();
        s.put_upstream(original.clone());
        s.set_now(Client::Codex, id);
        s.put_upstream(Upstream {
            base_url: "https://new.invalid/v1".into(),
            alternates: vec![original.base_url.clone()],
            ..original.clone()
        });
        let state = Arc::new(Mutex::new(s));
        let rejected = admission::acquire_from(
            &state,
            Client::Codex,
            id,
            Instant::now(),
            Some(&original),
            admission::WaitPolicy {
                strict_source: true,
                ..Default::default()
            },
        )
        .await;
        assert!(matches!(rejected, Err(r) if r.reason == "permission"));
    }
}
