//! 本机准入：同步锁内领取并发名额与 RPM，异步有限等待，名额随真实响应流释放。
//! 未知额度不猜；只读已经查询的明确可用性，不在请求路径查数据库或联网。

use super::*;
use qb_station::router::{PoolStrategy, QuotaAdmissionState, RouteAdmissionView, RouteLimits};
use std::time::{Duration, Instant};

const QUOTA_FRESH_MS: i64 = 300_000;
const MAX_RETRY_MS: i64 = 3_600_000;
const RPM_WINDOW_MS: i64 = 60_000;

#[derive(Default)]
pub(super) struct Admission {
    pub active: u32,
    pub(super) waiting: u32,
    sent: VecDeque<i64>,
    retry_at: Option<i64>,
    quota: Option<(i64, Option<bool>)>,
}

impl Admission {
    fn prune(&mut self, now: i64) {
        while self.sent.front().is_some_and(|t| now - *t >= RPM_WINDOW_MS) {
            self.sent.pop_front();
        }
    }

    fn quota_state(&self, limits: &RouteLimits, now: i64) -> QuotaAdmissionState {
        use QuotaAdmissionState::*;
        if !limits.check_quota {
            return Disabled;
        }
        match self.quota {
            None | Some((_, None)) => Unknown,
            Some((at, _)) if !(0..QUOTA_FRESH_MS).contains(&(now - at)) => Stale,
            Some((_, Some(true))) => Available,
            Some((_, Some(false))) => Exhausted,
        }
    }

    fn blocked(&self, limits: &RouteLimits, now: i64) -> Option<Rejection> {
        if self.quota_state(limits, now) == QuotaAdmissionState::Exhausted {
            return Some(Rejection::new("quota-exhausted", None));
        }
        if let Some(at) = self.retry_at.filter(|at| *at > now) {
            return Some(Rejection::new("retry-after", Some(at)));
        }
        if limits.rpm > 0
            && self
                .sent
                .iter()
                .filter(|at| now - **at < RPM_WINDOW_MS)
                .count()
                >= limits.rpm as usize
        {
            return Some(Rejection::new(
                "rpm",
                self.sent
                    .iter()
                    .find(|at| now - **at < RPM_WINDOW_MS)
                    .map(|at| at + RPM_WINDOW_MS),
            ));
        }
        if limits.max_concurrent > 0 && self.active >= limits.max_concurrent {
            return Some(Rejection::new("concurrency", None));
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct Rejection {
    pub reason: &'static str,
    pub retry_at_ms: Option<i64>,
}

impl Rejection {
    fn new(reason: &'static str, retry_at_ms: Option<i64>) -> Self {
        Self {
            reason,
            retry_at_ms,
        }
    }
}

/// 名额必须活到响应流完成或取消；Drop 同时覆盖异常、超时、客户端中断。
pub struct AdmissionPermit {
    state: Arc<Mutex<RouterState>>,
    route_id: String,
}

impl Drop for AdmissionPermit {
    fn drop(&mut self) {
        if let Ok(mut s) = self.state.lock() {
            if let Some(a) = s.admissions.get_mut(&self.route_id) {
                a.active = a.active.saturating_sub(1);
            }
        }
    }
}

pub(super) struct Waiting {
    state: Arc<Mutex<RouterState>>,
    route_id: String,
    deadline: Instant,
}

impl Drop for Waiting {
    fn drop(&mut self) {
        if let Ok(mut s) = self.state.lock() {
            if let Some(a) = s.admissions.get_mut(&self.route_id) {
                a.waiting = a.waiting.saturating_sub(1);
            }
        }
    }
}

impl RouterState {
    fn try_admit(
        &mut self,
        client: Client,
        route: &str,
        require_permission: bool,
        now: i64,
    ) -> std::result::Result<(), Rejection> {
        if !self.upstreams.contains_key(route)
            || (require_permission && !self.route_allowed(client, route))
        {
            return Err(Rejection::new("permission", None));
        }
        if self
            .upstreams
            .get(route)
            .is_some_and(|u| matches!(u.auth, UpstreamAuth::OAuthPassthrough))
        {
            return Err(Rejection::new("permission", None));
        }
        let limits = self
            .settings(client)
            .admission
            .route_limits
            .get(route)
            .cloned()
            .unwrap_or_default();
        let a = self.admissions.entry(route.to_owned()).or_default();
        a.prune(now);
        if let Some(reason) = a.blocked(&limits, now) {
            return Err(reason);
        }
        a.active += 1;
        // 即使 RPM 暂关也保留最近发送计数，之后打开时不会漏掉刚发出的请求。
        // 上界来自允许的最大 RPM；超过它只保留最新的 60000 次。
        a.sent.push_back(now);
        if a.sent.len() > 60_000 {
            a.sent.pop_front();
        }
        Ok(())
    }

    /// 同步发包入口。后台探测不排队，和真实流量共用容量/RPM；手动探测可显式指定非当前线。
    pub fn admit_probe(
        state: &Arc<Mutex<Self>>,
        client: Client,
        route: &str,
        require_permission: bool,
    ) -> Option<AdmissionPermit> {
        let mut s = state.lock().ok()?;
        if require_permission && s.settings(client).routing_mode != RoutingMode::Pool {
            return None;
        }
        s.try_admit(
            client,
            route,
            require_permission,
            chrono::Utc::now().timestamp_millis(),
        )
        .ok()?;
        Some(AdmissionPermit {
            state: Arc::clone(state),
            route_id: route.to_owned(),
        })
    }

    pub fn admission_views(&self, client: Client, now: i64) -> Vec<RouteAdmissionView> {
        let settings = self.settings(client);
        let mut ids = self.client_routes(client);
        ids.sort();
        ids.into_iter()
            .map(|route_id| {
                let empty = Admission::default();
                let a = self.admissions.get(&route_id).unwrap_or(&empty);
                let limits = settings
                    .admission
                    .route_limits
                    .get(&route_id)
                    .cloned()
                    .unwrap_or_default();
                let rpm_used = a.sent.iter().filter(|t| now - **t < RPM_WINDOW_MS).count() as u32;
                // 状态查询不改变准入计数。
                let reason = if a.quota_state(&limits, now) == QuotaAdmissionState::Exhausted {
                    Some("quota-exhausted")
                } else if a.retry_at.is_some_and(|at| at > now) {
                    Some("retry-after")
                } else if limits.rpm > 0 && rpm_used >= limits.rpm {
                    Some("rpm")
                } else if limits.max_concurrent > 0 && a.active >= limits.max_concurrent {
                    Some("concurrency")
                } else {
                    None
                };
                let retry_at_ms = a.retry_at.filter(|at| *at > now).or_else(|| {
                    (reason == Some("rpm"))
                        .then(|| {
                            a.sent
                                .iter()
                                .find(|t| now - **t < RPM_WINDOW_MS)
                                .map(|at| at + RPM_WINDOW_MS)
                        })
                        .flatten()
                });
                RouteAdmissionView {
                    route_id,
                    active: a.active,
                    waiting: a.waiting,
                    rpm_used,
                    retry_at_ms,
                    reason: reason.map(str::to_string),
                    quota_checked_at_ms: a.quota.map(|q| q.0),
                    quota_state: a.quota_state(&limits, now),
                }
            })
            .collect()
    }

    pub(super) fn order_by_load(&self, client: Client, lines: &mut [Upstream]) {
        let settings = self.settings(client);
        if settings.routing_mode != RoutingMode::Pool
            || settings.pool_strategy != PoolStrategy::LeastLoaded
        {
            return;
        }
        let now = chrono::Utc::now().timestamp_millis();
        let score = |u: &Upstream| {
            let empty = Admission::default();
            let a = self.admissions.get(&u.route_id).unwrap_or(&empty);
            let limits = settings
                .admission
                .route_limits
                .get(&u.route_id)
                .cloned()
                .unwrap_or_default();
            let occupied = a.active as f64 / f64::from(limits.max_concurrent.max(1));
            (a.blocked(&limits, now).is_some(), occupied)
        };
        // 稳定排序：占用相同继续遵守用户/调度的原顺序。
        lines.sort_by(|a, b| {
            score(a)
                .partial_cmp(&score(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    pub(super) fn remember_rate_limit(&mut self, route: &str, delay: Option<i64>) {
        let now = chrono::Utc::now().timestamp_millis();
        let at = now.saturating_add(delay.unwrap_or(1000).clamp(0, MAX_RETRY_MS));
        let a = self.admissions.entry(route.to_owned()).or_default();
        a.retry_at = Some(a.retry_at.map_or(at, |old| old.max(at)));
    }

    /// 查询开始前拿版本，源重载/编辑后返回的旧额度不会挂到新 Key 上。
    pub fn quota_generation(&self) -> u64 {
        self.quota_generation
    }

    /// 用量查询脚本改变后立即作废旧证据，不为刷新证据主动联网。
    pub fn clear_quota_evidence(&mut self) {
        self.invalidate_quota();
    }

    pub fn publish_quota(
        &mut self,
        generation: u64,
        route: &str,
        checked_at: i64,
        available: Option<bool>,
    ) {
        if generation == self.quota_generation && self.upstreams.contains_key(route) {
            self.admissions.entry(route.to_owned()).or_default().quota =
                Some((checked_at, available));
        }
    }

    pub(super) fn invalidate_quota(&mut self) {
        self.quota_generation = self.quota_generation.wrapping_add(1);
        self.admissions.retain(|id, a| {
            a.quota = None;
            self.upstreams.contains_key(id) || a.active > 0 || a.waiting > 0
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn acquire(
        state: &Arc<Mutex<RouterState>>,
        client: Client,
        route: &str,
    ) -> std::result::Result<AdmissionPermit, Rejection> {
        acquire_from(state, client, route, Instant::now(), true, None).await
    }

    fn state(concurrent: u32, rpm: u32) -> Arc<Mutex<RouterState>> {
        let mut s = RouterState::default();
        s.put_upstream(Upstream {
            route_id: "a".into(),
            base_url: "http://127.0.0.1/changed".into(),
            ..Default::default()
        });
        s.set_now(Client::ClaudeCode, "a");
        let mut config = RouterSettings::for_client(Client::ClaudeCode);
        config.admission.route_limits.insert(
            "a".into(),
            RouteLimits {
                max_concurrent: concurrent,
                rpm,
                check_quota: true,
            },
        );
        s.set_settings(config);
        Arc::new(Mutex::new(s))
    }

    fn waiting_enabled(s: &Arc<Mutex<RouterState>>, ms: u32) {
        let mut s = s.lock().unwrap();
        let mut cfg = s.settings(Client::ClaudeCode);
        cfg.admission.max_wait_ms = ms;
        cfg.admission.max_waiters = 1;
        s.set_settings(cfg);
    }

    async fn wait_queued(s: &Arc<Mutex<RouterState>>) {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if s.lock()
                    .unwrap()
                    .admissions
                    .get("a")
                    .is_some_and(|a| a.waiting == 1)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn atomic_capacity_and_probe_share_a_single_budget() {
        let s = state(2, 0);
        let first = acquire(&s, Client::ClaudeCode, "a").await.unwrap();
        let probe = RouterState::admit_probe(&s, Client::ClaudeCode, "a", false).unwrap();
        let mut tasks = Vec::new();
        for _ in 0..32 {
            let s = Arc::clone(&s);
            tasks.push(tokio::spawn(async move {
                acquire(&s, Client::ClaudeCode, "a")
                    .await
                    .err()
                    .unwrap()
                    .reason
            }));
        }
        for task in tasks {
            assert_eq!(task.await.unwrap(), "concurrency");
        }
        assert_eq!(s.lock().unwrap().admissions["a"].sent.len(), 2);
        drop(first);
        let another = acquire(&s, Client::ClaudeCode, "a").await.unwrap();
        drop((another, probe));
        assert_eq!(s.lock().unwrap().admissions["a"].active, 0);
    }

    #[tokio::test]
    async fn canceled_waiter_releases_queue_and_queued_deletion_never_consumes_rpm() {
        let s = state(1, 0);
        waiting_enabled(&s, 1000);
        let held = acquire(&s, Client::ClaudeCode, "a").await.unwrap();
        let copy = Arc::clone(&s);
        let canceled = tokio::spawn(async move { acquire(&copy, Client::ClaudeCode, "a").await });
        wait_queued(&s).await;
        assert_eq!(
            acquire(&s, Client::ClaudeCode, "a")
                .await
                .err()
                .unwrap()
                .reason,
            "queue-full"
        );
        canceled.abort();
        assert!(matches!(canceled.await, Err(e) if e.is_cancelled()));
        assert_eq!(s.lock().unwrap().admissions["a"].waiting, 0);
        let copy = Arc::clone(&s);
        let revoked = tokio::spawn(async move { acquire(&copy, Client::ClaudeCode, "a").await });
        wait_queued(&s).await;
        s.lock().unwrap().remove_upstream("a");
        drop(held);
        assert_eq!(revoked.await.unwrap().err().unwrap().reason, "permission");
        let s = s.lock().unwrap();
        assert_eq!(s.admissions["a"].waiting, 0);
        assert_eq!(s.admissions["a"].sent.len(), 1);
    }

    #[tokio::test]
    async fn bounded_wait_expires_and_recovers_when_capacity_frees() {
        let s = state(1, 0);
        waiting_enabled(&s, 40);
        let held = acquire(&s, Client::ClaudeCode, "a").await.unwrap();
        assert_eq!(
            acquire(&s, Client::ClaudeCode, "a")
                .await
                .err()
                .unwrap()
                .reason,
            "wait-timeout"
        );
        assert_eq!(s.lock().unwrap().admissions["a"].waiting, 0);
        waiting_enabled(&s, 1000);
        let copy = Arc::clone(&s);
        let waiting = tokio::spawn(async move { acquire(&copy, Client::ClaudeCode, "a").await });
        wait_queued(&s).await;
        drop(held);
        drop(waiting.await.unwrap().unwrap());
        assert_eq!(s.lock().unwrap().admissions["a"].active, 0);
    }

    #[test]
    fn rpm_and_retry_after_recover_without_spurious_sends() {
        let s = state(0, 1);
        let mut s = s.lock().unwrap();
        s.try_admit(Client::ClaudeCode, "a", true, 1000).unwrap();
        assert_eq!(
            s.try_admit(Client::ClaudeCode, "a", true, 2000)
                .unwrap_err()
                .reason,
            "rpm"
        );
        assert_eq!(s.admissions["a"].sent.len(), 1);
        s.try_admit(Client::ClaudeCode, "a", true, 61_000).unwrap();
        s.admissions.get_mut("a").unwrap().retry_at = Some(122_000);
        assert_eq!(
            s.try_admit(Client::ClaudeCode, "a", true, 121_000)
                .unwrap_err()
                .reason,
            "retry-after"
        );
        s.try_admit(Client::ClaudeCode, "a", true, 122_000).unwrap();
    }

    #[test]
    fn only_fresh_explicit_unavailability_blocks_and_source_reload_invalidates_it() {
        let s = state(0, 0);
        let mut s = s.lock().unwrap();
        let generation = s.quota_generation();
        for available in [None, Some(true)] {
            s.publish_quota(generation, "a", 1000, available);
            assert!(s.try_admit(Client::ClaudeCode, "a", true, 1001).is_ok());
        }
        s.publish_quota(generation, "a", 1000, Some(false));
        assert_eq!(
            s.try_admit(Client::ClaudeCode, "a", true, 1001)
                .unwrap_err()
                .reason,
            "quota-exhausted"
        );
        assert!(s.try_admit(Client::ClaudeCode, "a", true, 301_000).is_ok());
        s.put_upstream(Upstream {
            route_id: "a".into(),
            ..Default::default()
        });
        s.publish_quota(generation, "a", 1000, Some(false));
        assert!(s.try_admit(Client::ClaudeCode, "a", true, 1001).is_ok());
    }

    #[tokio::test]
    async fn pool_waiter_occupies_one_slot_across_wakeups_and_rejects_extra_requests() {
        let s = state(1, 0);
        waiting_enabled(&s, 1000);
        {
            let mut s = s.lock().unwrap();
            let mut cfg = s.settings(Client::ClaudeCode);
            cfg.routing_mode = RoutingMode::Pool;
            cfg.allowed_routes = vec!["a".into()];
            s.set_settings(cfg);
        }
        let started = Instant::now();
        let mut waiter = None;
        for _ in 0..3 {
            pause_pool(&s, Client::ClaudeCode, "a", started, &mut waiter)
                .await
                .unwrap();
            assert_eq!(s.lock().unwrap().admissions["a"].waiting, 1);
            let mut extra = None;
            assert_eq!(
                pause_pool(&s, Client::ClaudeCode, "a", started, &mut extra)
                    .await
                    .unwrap_err()
                    .reason,
                "queue-full"
            );
        }
        drop(waiter);
        assert_eq!(s.lock().unwrap().admissions["a"].waiting, 0);
    }

    #[tokio::test]
    async fn pool_wait_deadline_cannot_be_extended_and_disabled_queue_stops_waiter() {
        let s = state(1, 0);
        waiting_enabled(&s, 45);
        {
            let mut s = s.lock().unwrap();
            let mut cfg = s.settings(Client::ClaudeCode);
            cfg.routing_mode = RoutingMode::Pool;
            cfg.allowed_routes = vec!["a".into()];
            s.set_settings(cfg);
        }
        let started = Instant::now();
        let mut waiter = None;
        pause_pool(&s, Client::ClaudeCode, "a", started, &mut waiter)
            .await
            .unwrap();
        waiting_enabled(&s, 1000);
        loop {
            if let Err(reason) = pause_pool(&s, Client::ClaudeCode, "a", started, &mut waiter).await
            {
                assert_eq!(reason.reason, "wait-timeout");
                break;
            }
        }
        assert!(started.elapsed() < Duration::from_millis(300));
        drop(waiter);
        let mut waiter = None;
        pause_pool(&s, Client::ClaudeCode, "a", Instant::now(), &mut waiter)
            .await
            .unwrap();
        {
            let mut s = s.lock().unwrap();
            let mut cfg = s.settings(Client::ClaudeCode);
            cfg.admission.max_waiters = 0;
            s.set_settings(cfg);
        }
        assert_eq!(
            pause_pool(&s, Client::ClaudeCode, "a", Instant::now(), &mut waiter)
                .await
                .unwrap_err()
                .reason,
            "wait-timeout"
        );
        drop(waiter);
        assert_eq!(s.lock().unwrap().admissions["a"].waiting, 0);
    }
}

/// 最后发送前调用；每次轮询都现读权限、设置和额度，绝不持锁 await。
pub(super) async fn pause_pool(
    state: &Arc<Mutex<RouterState>>,
    client: Client,
    anchor: &str,
    started: Instant,
    holder: &mut Option<Waiting>,
) -> std::result::Result<(), Rejection> {
    let delay = {
        let mut s = state
            .lock()
            .map_err(|_| Rejection::new("permission", None))?;
        if !s.route_allowed(client, anchor) || s.settings(client).routing_mode != RoutingMode::Pool
        {
            return Err(Rejection::new("permission", None));
        }
        let settings = s.settings(client).admission;
        if settings.max_waiters == 0 || settings.max_wait_ms == 0 {
            return Err(Rejection::new("wait-timeout", None));
        }
        if holder.is_none() {
            let a = s.admissions.entry(anchor.to_owned()).or_default();
            if a.waiting >= settings.max_waiters {
                return Err(Rejection::new("queue-full", None));
            }
            a.waiting += 1;
            *holder = Some(Waiting {
                state: Arc::clone(state),
                route_id: anchor.to_owned(),
                deadline: started + Duration::from_millis(u64::from(settings.max_wait_ms)),
            });
        }
        if let Some(waiter) = holder.as_mut() {
            waiter.deadline = waiter
                .deadline
                .min(started + Duration::from_millis(u64::from(settings.max_wait_ms)));
        }
        let remaining = holder
            .as_ref()
            .map(|w| w.deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_default();
        if remaining.is_zero() {
            return Err(Rejection::new("wait-timeout", None));
        }
        remaining.min(Duration::from_millis(20))
    };
    tokio::time::sleep(delay).await;
    let s = state
        .lock()
        .map_err(|_| Rejection::new("permission", None))?;
    if !s.route_allowed(client, anchor) || s.settings(client).routing_mode != RoutingMode::Pool {
        return Err(Rejection::new("permission", None));
    }
    if s.settings(client).admission.max_waiters == 0
        || s.settings(client).admission.max_wait_ms == 0
        || holder.as_ref().is_none_or(|w| Instant::now() >= w.deadline)
    {
        return Err(Rejection::new("wait-timeout", None));
    }
    Ok(())
}

/// 最后发送前调用；每次轮询都现读权限、设置和额度，绝不持锁 await。
pub(super) async fn acquire_from(
    state: &Arc<Mutex<RouterState>>,
    client: Client,
    route: &str,
    started: Instant,
    may_wait: bool,
    source: Option<&Upstream>,
) -> std::result::Result<AdmissionPermit, Rejection> {
    let mut waiting: Option<Waiting> = None;
    let mut deadline = None;
    loop {
        {
            let mut s = state
                .lock()
                .map_err(|_| Rejection::new("permission", None))?;
            let now = chrono::Utc::now().timestamp_millis();
            if let Some(source) = source {
                let unchanged = s.upstreams.get(route).is_some_and(|live| {
                    live.auth == source.auth
                        && live.format == source.format
                        && (live.base_url == source.base_url
                            || live.alternates.contains(&source.base_url))
                });
                if !unchanged {
                    return Err(Rejection::new("permission", None));
                }
            }
            if let Some(limit) = deadline {
                if started.elapsed() >= limit {
                    return Err(Rejection::new("wait-timeout", None));
                }
            }
            match s.try_admit(client, route, true, now) {
                Ok(()) => {
                    return Ok(AdmissionPermit {
                        state: Arc::clone(state),
                        route_id: route.to_owned(),
                    })
                }
                Err(rejection) => {
                    if matches!(rejection.reason, "permission" | "quota-exhausted") {
                        return Err(rejection);
                    }
                    let settings = s.settings(client).admission;
                    if !may_wait || settings.max_wait_ms == 0 || settings.max_waiters == 0 {
                        return Err(rejection);
                    }
                    let limit = Duration::from_millis(u64::from(settings.max_wait_ms));
                    deadline = Some(deadline.map_or(limit, |old: Duration| old.min(limit)));
                    if waiting.is_none() {
                        let a = s.admissions.entry(route.to_owned()).or_default();
                        if a.waiting >= settings.max_waiters {
                            return Err(Rejection::new("queue-full", rejection.retry_at_ms));
                        }
                        a.waiting += 1;
                        waiting = Some(Waiting {
                            state: Arc::clone(state),
                            route_id: route.to_owned(),
                            deadline: started + limit,
                        });
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
