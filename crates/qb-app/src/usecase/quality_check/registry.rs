use super::*;
use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;

const MAX_REQUESTS: u32 = 15;
const MAX_RUN_TIME: Duration = Duration::from_secs(3600);
const KEEP: Duration = Duration::from_secs(7200);
#[derive(Default)]
struct Registry {
    active: Option<(String, String, tokio::sync::watch::Sender<bool>)>,
    runs: HashMap<String, Run>,
    finished: VecDeque<(String, Instant)>,
    cancelled: VecDeque<(String, Instant)>,
}
struct Run {
    count: u32,
    elapsed: Duration,
    updated: Instant,
    target: (QualityTargetKind, String, String, String),
    source: Option<String>,
    revoked: bool,
    gate_epoch: Option<(u64, bool)>,
}
static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
static SOURCE_SALT: OnceLock<[u8; 16]> = OnceLock::new();
pub(super) fn source_stamp(fingerprint: &str) -> String {
    use sha2::{Digest, Sha256};
    let salt = SOURCE_SALT.get_or_init(|| *uuid::Uuid::new_v4().as_bytes());
    let mut hash = Sha256::new();
    hash.update(salt);
    hash.update(fingerprint.as_bytes());
    hex::encode(hash.finalize())
}
fn registry() -> &'static Mutex<Registry> {
    REGISTRY.get_or_init(Default::default)
}
impl Registry {
    fn prune(&mut self) {
        self.runs.retain(|_, run| run.updated.elapsed() < KEEP);
        self.finished.retain(|(_, at)| at.elapsed() < KEEP);
        self.cancelled.retain(|(_, at)| at.elapsed() < KEEP);
    }
    fn start(
        &mut self,
        request: &QualityCheckRequest,
    ) -> Result<Option<tokio::sync::watch::Receiver<bool>>> {
        self.prune();
        if self
            .cancelled
            .iter()
            .any(|(id, _)| id == &request.request_id)
        {
            return Ok(None);
        }
        if self.active.is_some() {
            return Err(GateError::Other(
                "已有质量检测正在执行，请完成或取消后再试".into(),
            ));
        }
        if self
            .finished
            .iter()
            .any(|(id, _)| id == &request.request_id)
        {
            return Err(GateError::Other(
                "这个检测请求已经执行，请新建请求 ID".into(),
            ));
        }
        if !self.runs.contains_key(&request.run_id) && self.runs.len() >= 64 {
            return Err(GateError::Other("近期检测批次过多，请稍后再试".into()));
        }
        let identity = (
            request.target.kind,
            request.target.id.clone(),
            request.model.clone(),
            request.effort.clone(),
        );
        let run = self.runs.entry(request.run_id.clone()).or_insert(Run {
            count: 0,
            elapsed: Duration::ZERO,
            updated: Instant::now(),
            target: identity.clone(),
            source: None,
            revoked: false,
            gate_epoch: None,
        });
        if run.revoked {
            return Ok(None);
        }
        if run.target != identity {
            return Err(GateError::Other(
                "同一检测批次不能更换目标、模型或推理强度，请新建批次".into(),
            ));
        }
        if run.count >= MAX_REQUESTS
            || run.elapsed + Duration::from_secs(u64::from(request.timeout_secs)) > MAX_RUN_TIME
        {
            return Err(GateError::Other(
                "每批最多 15 次请求，累计执行时间最多 1 小时".into(),
            ));
        }
        run.count += 1;
        run.updated = Instant::now();
        let (tx, rx) = tokio::sync::watch::channel(false);
        self.active = Some((request.request_id.clone(), request.run_id.clone(), tx));
        Ok(Some(rx))
    }
    fn cancel(&mut self, id: &str) -> bool {
        self.prune();
        if let Some((active, run_id, sender)) = &self.active {
            if active == id {
                sender.send_replace(true);
                if let Some(run) = self.runs.get_mut(run_id) {
                    run.revoked = true;
                }
                return true;
            }
        }
        if !self.cancelled.iter().any(|(known, _)| known == id) {
            if self.cancelled.len() >= 256 {
                self.cancelled.pop_front();
            }
            self.cancelled.push_back((id.to_owned(), Instant::now()));
        }
        false
    }
    fn bind_source(&mut self, run_id: &str, fingerprint: String) -> Result<()> {
        let run = self
            .runs
            .get_mut(run_id)
            .ok_or_else(|| GateError::Other("检测批次已经结束".into()))?;
        if run.source.as_ref().is_some_and(|old| old != &fingerprint) {
            return Err(GateError::Other(
                "检测期间线路来源、出站代理或模型映射已修改，请新建批次；本次未发送".into(),
            ));
        }
        run.source = Some(fingerprint);
        Ok(())
    }
    fn check_gate(
        &mut self,
        run_id: &str,
        generation: u64,
        gated: bool,
        paused: bool,
    ) -> Result<()> {
        let run = self
            .runs
            .get_mut(run_id)
            .ok_or_else(|| GateError::Other("检测批次已经结束".into()))?;
        if run.revoked
            || run
                .gate_epoch
                .is_some_and(|old| old != (generation, gated) || (gated && paused))
        {
            run.revoked = true;
            return Err(GateError::Other(
                "门禁状态已变更，本检测批次已取消；请手动新建批次".into(),
            ));
        }
        run.gate_epoch = Some((generation, gated));
        Ok(())
    }
    fn gate_stopped(&mut self, all: bool) {
        for run in self.runs.values_mut() {
            if all
                || (run.target.0 == QualityTargetKind::CodexAccount
                    && run.gate_epoch.is_none_or(|(_, gated)| gated))
            {
                run.revoked = true;
            }
        }
        if let Some((_, run_id, sender)) = &self.active {
            if self.runs.get(run_id).is_some_and(|run| run.revoked) {
                sender.send_replace(true);
            }
        }
    }
}
pub(super) struct Ticket {
    id: String,
    run: String,
    started: Instant,
    stop: tokio::sync::watch::Receiver<bool>,
}
impl Ticket {
    pub async fn cancelled(&mut self) {
        if *self.stop.borrow_and_update() {
            return;
        }
        let _ = self.stop.changed().await;
    }
}
impl Drop for Ticket {
    fn drop(&mut self) {
        if let Ok(mut registry) = registry().lock() {
            if registry
                .active
                .as_ref()
                .is_some_and(|(id, _, _)| id == &self.id)
            {
                registry.active = None;
            }
            if let Some(run) = registry.runs.get_mut(&self.run) {
                run.elapsed += self.started.elapsed();
                run.updated = Instant::now();
            }
            if registry.finished.len() >= 1024 {
                registry.finished.pop_front();
            }
            registry
                .finished
                .push_back((self.id.clone(), Instant::now()));
        }
    }
}
pub(super) fn begin(request: &QualityCheckRequest) -> Result<Option<Ticket>> {
    let rx = registry()
        .lock()
        .map_err(|_| GateError::Other("检测状态不可用".into()))?
        .start(request)?;
    Ok(rx.map(|stop| Ticket {
        id: request.request_id.clone(),
        run: request.run_id.clone(),
        started: Instant::now(),
        stop,
    }))
}
pub(super) fn cancel(id: &str) -> Result<bool> {
    uuid::Uuid::parse_str(id).map_err(|_| GateError::Other("无效的检测请求 ID".into()))?;
    Ok(registry()
        .lock()
        .map_err(|_| GateError::Other("检测状态不可用".into()))?
        .cancel(id))
}

pub(super) fn bind_source(run_id: &str, fingerprint: String) -> Result<()> {
    registry()
        .lock()
        .map_err(|_| GateError::Other("检测状态不可用".into()))?
        .bind_source(run_id, fingerprint)
}
pub(super) fn check_gate(run_id: &str, gate: &crate::gate::GateState, gated: bool) -> Result<()> {
    registry()
        .lock()
        .map_err(|_| GateError::Other("检测状态不可用".into()))?
        .check_gate(
            run_id,
            gate.generation.load(std::sync::atomic::Ordering::SeqCst),
            gated,
            gate.manual_paused.load(std::sync::atomic::Ordering::SeqCst),
        )
}
pub(super) fn revoke(run_id: &str) {
    if let Ok(mut registry) = registry().lock() {
        if let Some(run) = registry.runs.get_mut(run_id) {
            run.revoked = true;
        }
        if let Some((_, active_run, sender)) = &registry.active {
            if active_run == run_id {
                sender.send_replace(true);
            }
        }
    }
}
pub(super) fn revoked(run_id: &str) -> bool {
    registry()
        .lock()
        .is_ok_and(|registry| registry.runs.get(run_id).is_some_and(|run| run.revoked))
}
pub(super) fn gate_stopped(all: bool) {
    if let Ok(mut registry) = registry().lock() {
        registry.gate_stopped(all);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> QualityCheckRequest {
        QualityCheckRequest {
            run_id: uuid::Uuid::new_v4().to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
            target: QualityTarget {
                kind: QualityTargetKind::Relay,
                id: "test".into(),
            },
            model: "m".into(),
            effort: "high".into(),
            prompt: "test".into(),
            max_output_tokens: 100,
            timeout_secs: 10,
        }
    }
    #[test]
    fn cancellation_before_registration_prevents_dispatch_and_batches_are_bounded() {
        let mut registry = Registry::default();
        let mut request = request();
        registry.cancel(&request.request_id);
        assert!(registry.start(&request).unwrap().is_none());
        request.request_id = uuid::Uuid::new_v4().to_string();
        for _ in 0..15 {
            assert!(registry.start(&request).unwrap().is_some());
            registry.active = None;
            request.request_id = uuid::Uuid::new_v4().to_string();
        }
        assert!(registry.start(&request).is_err());
    }
    #[test]
    fn one_active_request_and_total_runtime_limit_are_enforced() {
        let mut registry = Registry::default();
        let request = request();
        registry.start(&request).unwrap();
        assert!(registry.start(&request).is_err());
        registry.active = None;
        registry.runs.get_mut(&request.run_id).unwrap().elapsed = MAX_RUN_TIME;
        assert!(registry.start(&request).is_err());
    }
    #[test]
    fn a_batch_pins_target_model_effort_and_source_without_storing_credentials() {
        let mut registry = Registry::default();
        let mut request = request();
        registry.start(&request).unwrap();
        registry
            .bind_source(&request.run_id, "opaque-source-a".into())
            .unwrap();
        registry
            .bind_source(&request.run_id, "opaque-source-a".into())
            .unwrap();
        assert!(registry
            .bind_source(&request.run_id, "opaque-source-b".into())
            .is_err());
        registry.active = None;
        request.request_id = uuid::Uuid::new_v4().to_string();
        request.effort = "ultra".into();
        assert!(registry.start(&request).is_err());
        request.effort = "high".into();
        request.model = "other".into();
        assert!(registry.start(&request).is_err());
        request.model = "m".into();
        request.target.id = "other".into();
        assert!(registry.start(&request).is_err());
    }
    #[test]
    fn gate_revoke_during_and_between_samples_cancels_batch_without_reopen() {
        let mut registry = Registry::default();
        let mut request = request();
        request.target.kind = QualityTargetKind::CodexAccount;
        let receiver = registry.start(&request).unwrap().unwrap();
        registry
            .check_gate(&request.run_id, 5, true, false)
            .unwrap();
        registry.gate_stopped(false);
        assert!(*receiver.borrow());
        registry.active = None;
        request.request_id = uuid::Uuid::new_v4().to_string();
        assert!(registry.start(&request).unwrap().is_none());
        request.run_id = uuid::Uuid::new_v4().to_string();
        registry.start(&request).unwrap();
        registry
            .check_gate(&request.run_id, 6, true, false)
            .unwrap();
        registry.active = None;
        assert!(registry.check_gate(&request.run_id, 7, true, true).is_err());
        assert!(registry.start(&request).unwrap().is_none());
        request.run_id = uuid::Uuid::new_v4().to_string();
        request.target.kind = QualityTargetKind::Relay;
        let receiver = registry.start(&request).unwrap().unwrap();
        registry.gate_stopped(false);
        assert!(!*receiver.borrow());
        registry.gate_stopped(true);
        assert!(*receiver.borrow());
    }
}
