//! 两级熔断。纯状态机,**不取系统时间** —— `now_ms` 一律由调用方传进来。
//!
//! # 两级,不是一级
//!
//! | 上游回了什么 | 处置 | 为什么 |
//! |---|---|---|
//! | `429` | **只降权,不熔断**;读 `Retry-After` | 被限流说明这条线是**通的**,只是你用太快了。熔断它等于把一条能用的线自己掐掉 |
//! | `401` / `403` / `402` | **立即熔断，关一小时** | Key 错了、没权限、余额光了。再打一百次也是同样的回答,只是白烧请求 |
//! | `5xx` / 超时 | **连续 5 次**才熔断 | 单发 500 是噪声。一次抖动就掐线,等于把抖动放大成故障 |
//!
//! 这三档合并成一档的话:要么把限流当故障(丢掉一条好线),
//! 要么把认证失败当抖动(白烧五次请求才反应过来)。
//!
//! # 回探靠降权,不靠额外的调度器
//!
//! 熔断到期后进入[半开](State::HalfOpen):它**重新有资格参与竞争**,
//! 但带一个很重的降权([`HALF_OPEN_DEMOTION`])。于是只有在没有更好的线时
//! 它才会被选中 —— 这正好就是「定期回探」,而且用的是真实流量,
//! 不需要再造一套探针。探成功就闭合,再失败就按退避重新熔断。
//!
//! # ⛔ 这里的熔断绝不许触发官方账户槽位切换
//!
//! 中转站的线路挂了,该做的是换一条中转线路,**不是去动官方账户**。
//! 两条路径在代码上是隔死的:这个模块只认识 [`RouteBreaker`],
//! 既不认识账户、也不认识槽位,物理上写不出那句调用。
//! `qb-app` 侧另有一条测试钉着这件事。
//!
//! # 2026-10-17 起合进来的几样（中转站重构阶段 1）
//!
//! | 来源 | 加了什么 |
//! |---|---|
//! | cc-switch v4.0.14 `proxy/circuit_breaker.rs`（MIT） | **半开只放一发**（[`RouteBreaker::allow`] 给的 [`Permit`]）、**错误率阈值**（最近窗口里失败比例过线也熔断）、**中性结果**（[`RouteBreaker::release`]：放回半开名额、不动任何计数）、半开时失败**立刻**重新熔断 |
//! | sub2guard v2（使用者自己的调度） | **确定性故障熔断 1 小时**（余额 / 认证 / 退役：重试没有意义），以及「我充值了，立刻重测」（[`RouteBreaker::retest`]） |
//!
//! 半开期间的回探仍然靠真实流量（上面那一节）；变的是同一时刻只放一发过去探，
//! 探的那一发失败就按退避重新关上，不再要连续失败 5 次。
//!
//! 熔断的几个数可调（[`BreakerSettings`]，每个软件一份，在路由设置里）。

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Duration;
use ts_rs::TS;

/// 连续几次服务端错误才熔断。
///
/// 单发 500 是噪声,一次抖动就掐线等于把抖动放大成故障。
pub const TRIP_AFTER_CONSECUTIVE_ERRORS: u32 = 5;

/// 第一次熔断关多久。之后每熔断一次翻倍,封顶 [`MAX_TRIP`]。
pub const BASE_TRIP: Duration = Duration::from_secs(30);

/// 熔断最长关多久。
///
/// 有上限是有意的:站点修好了而我们还关着,对使用者来说跟站点没修好没区别。
pub const MAX_TRIP: Duration = Duration::from_secs(600);

/// 没读到 `Retry-After` 时,限流降权持续多久。
pub const DEFAULT_RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(60);

/// 被限流期间的降权系数。
///
/// 0.5 是判断,不是实测值:要重到「有同档次的替补时就让位」,
/// 又要轻到「它是全场唯一能用的线时仍然选它」。**限流不是故障** ——
/// 降到 0 等于熔断,那就把这一级并回上一级了。
pub const RATE_LIMITED_DEMOTION: f64 = 0.5;

/// 半开期间的降权系数。
///
/// 比限流重得多:限流的线是确知能用的,半开的线是**还没确认修好的**。
/// 只有在没有更好的线时才该轮到它 —— 那一次就是回探。
pub const HALF_OPEN_DEMOTION: f64 = 0.2;

/// 确定性故障（余额不足 / 认证失败 / 域名退役）关多久。
///
/// sub2guard 的结论：这类错是整把 Key 的状态，重测三次秒回三次同一个错 —— 30 秒后再探只是白烧请求。
/// 关一个小时；使用者充了值可以点「立刻重测」（[`RouteBreaker::retest`]）提前放出来。
pub const DETERMINISTIC_TRIP: Duration = Duration::from_secs(3600);

/// 错误率看最近多少次结果。限流和中性结果不算进去。
///
/// cc-switch 是从上一次闭合起累计的：一条跑了一整天的线，失败比例会被成千上万次成功摊平，
/// 一阵集中的失败永远推不过阈值。这里看最近一段（够 [`BreakerSettings::min_requests`] 才算）。
pub const ERROR_RATE_WINDOW: usize = 20;

/// 熔断的几个可调的数。每个软件一份（路由设置），默认值见 [`Default`]。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct BreakerSettings {
    /// 连续几次暂时性故障（5xx / 超时）才熔断。QB Gate 原来写死的 5。
    pub trip_after_consecutive: u32,
    /// 最近窗口里失败比例达到多少也熔断（0–1）。cc-switch 的 `error_rate_threshold`。
    pub error_rate: f64,
    /// 窗口里至少有几次结果才看错误率。cc-switch 的 `min_requests`。
    pub min_requests: u32,
    /// 半开时要连续成功几次才闭合。QB Gate 原来是一次（cc-switch 的 `success_threshold` 默认 2）。
    pub half_open_successes: u32,
}

impl Default for BreakerSettings {
    fn default() -> Self {
        Self {
            trip_after_consecutive: TRIP_AFTER_CONSECUTIVE_ERRORS,
            error_rate: 0.6,
            min_requests: 10,
            half_open_successes: 1,
        }
    }
}

/// [`RouteBreaker::allow`] 的回答。**放行了就必须还回来**（[`RouteBreaker::record_after`] 或
/// [`RouteBreaker::release`]），否则半开那一个名额一直被占着，线就永远探不回来。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permit {
    pub allowed: bool,
    /// 这一发占的是半开的探测名额。
    pub half_open: bool,
}

/// 上游这一次回了什么。**三档必须分开**,见模块头那张表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    /// `429`。`retry_after` 来自响应头,没有就用 [`DEFAULT_RATE_LIMIT_COOLDOWN`]。
    RateLimited {
        retry_after_ms: Option<i64>,
    },
    /// `401` / `403` / `402` —— 认证、权限、余额。重试没有意义。
    Rejected,
    /// `5xx` 或超时。
    ServerError,
}

impl Outcome {
    /// 由 HTTP 状态码判档。**这是 §4.6 那张表唯一的落地处** ——
    /// 本机路由和检验都走它,各判各的必然漂移出第二套规则。
    ///
    /// `retry_after_ms` 由调用方从 `Retry-After` 头换算好再传进来:
    /// 那个头有「秒数」和「HTTP 日期」两种写法,解析它需要当前时间,
    /// 而这个模块不许取系统时间。
    pub fn from_status(status: u16, retry_after_ms: Option<i64>) -> Self {
        match status {
            // 限流:线是通的,只是你用太快了。
            429 => Outcome::RateLimited { retry_after_ms },
            // 认证 / 权限 / 余额 —— 再打一百次也是同样的回答。
            401..=403 => Outcome::Rejected,
            s if (500..600).contains(&s) => Outcome::ServerError,
            s if (200..400).contains(&s) => Outcome::Ok,
            // 其余 4xx 是这一发请求自己的问题(参数错、模型名写错),
            // **不是线路的问题**。算故障会把一条好线按自己的 bug 掐掉。
            _ => Outcome::Ok,
        }
    }

    /// 超时。连不上和连上了不回,对熔断来说是同一档。
    pub fn timeout() -> Self {
        Outcome::ServerError
    }
}

/// 熔断器的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// 正常。
    Closed,
    /// 熔断中,不参与竞争。
    Open,
    /// 熔断已到期,重新参与竞争但带重降权 —— 下一次被选中就是回探。
    HalfOpen,
}

/// 一条线路的熔断器。
///
/// **纯状态机**:所有方法都要求调用方把 `now_ms` 传进来。
/// 自己去取系统时间的话,「熔断到期」这类边界条件根本测不稳。
#[derive(Debug, Clone, PartialEq)]
pub struct RouteBreaker {
    consecutive_errors: u32,
    /// 熔断到这一刻为止。
    open_until_ms: Option<i64>,
    /// 限流降权到这一刻为止。**跟熔断分开记** ——
    /// 合成一个字段就没法表达「限流但没熔断」。
    demoted_until_ms: Option<i64>,
    /// 熔断过几次,退避按它翻倍。闭合时清零。
    trips: u32,
    /// 上一次熔断是因为什么。界面要照这个写「它为什么被关掉」。
    last_trip: Option<Outcome>,
    /// 最近几次结果，`true` = 失败。只记 `Ok` / `ServerError` / `Rejected`，最多 [`ERROR_RATE_WINDOW`] 个。
    recent: VecDeque<bool>,
    /// 半开的那一个探测名额现在被占着。
    probing: bool,
    /// 半开以来连续成功了几次。
    half_open_ok: u32,
    settings: BreakerSettings,
}

impl Default for RouteBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteBreaker {
    pub fn new() -> Self {
        Self::with_settings(BreakerSettings::default())
    }

    pub fn with_settings(settings: BreakerSettings) -> Self {
        Self {
            consecutive_errors: 0,
            open_until_ms: None,
            demoted_until_ms: None,
            trips: 0,
            last_trip: None,
            recent: VecDeque::new(),
            probing: false,
            half_open_ok: 0,
            settings,
        }
    }

    /// 换一套参数。**不动当前状态** —— 正熔断着的照旧熔断着（同 cc-switch 的热更新）。
    pub fn set_settings(&mut self, settings: BreakerSettings) {
        self.settings = settings;
    }

    pub fn settings(&self) -> BreakerSettings {
        self.settings
    }

    /// 发请求之前问一句能不能发。
    ///
    /// | 状态 | 回答 |
    /// |---|---|
    /// | 闭合 | 放行 |
    /// | 熔断中 | 不放 |
    /// | 半开 | **只放一发**（占住探测名额），这一发还回来之前别的都不放 |
    pub fn allow(&mut self, now_ms: i64) -> Permit {
        match self.state(now_ms) {
            State::Closed => Permit {
                allowed: true,
                half_open: false,
            },
            State::Open => Permit {
                allowed: false,
                half_open: false,
            },
            State::HalfOpen if self.probing => Permit {
                allowed: false,
                half_open: false,
            },
            State::HalfOpen => {
                self.probing = true;
                Permit {
                    allowed: true,
                    half_open: true,
                }
            }
        }
    }

    /// 只还回半开名额，不动任何计数 —— 这一发的结果不算这条线的账
    /// （请求自己的错、客户端断开、整流后同一条线重试……）。cc-switch 的 `release_half_open_permit`。
    pub fn release(&mut self, permit: Permit) {
        if permit.half_open {
            self.probing = false;
        }
    }

    /// 还回名额，再记结果。
    pub fn record_after(&mut self, permit: Permit, outcome: Outcome, now_ms: i64) {
        self.release(permit);
        self.record(outcome, now_ms);
    }

    /// 「我充值了，立刻重测」：熔断中的线马上转半开，下一发真实请求就是回探。
    ///
    /// 不是直接闭合 —— 充没充上、Key 改没改对，要那一发请求说了算。返回它原来是不是熔断着。
    pub fn retest(&mut self, now_ms: i64) -> bool {
        let was_open = self.state(now_ms) == State::Open;
        if was_open {
            self.open_until_ms = Some(now_ms);
        }
        self.probing = false;
        self.half_open_ok = 0;
        was_open
    }

    /// 熔断到什么时候（界面写「还要关多久」）。没熔断过就是 `None`。
    pub fn open_until_ms(&self) -> Option<i64> {
        self.open_until_ms
    }

    /// 这一刻处在哪一档。
    pub fn state(&self, now_ms: i64) -> State {
        match self.open_until_ms {
            Some(until) if now_ms < until => State::Open,
            // 熔断过、已到期、还没被一次成功请求确认修好 —— 半开。
            Some(_) => State::HalfOpen,
            None => State::Closed,
        }
    }

    /// 要不要把它排除在竞争之外。**只有 Open 才排除**;
    /// 半开的线要留在池子里,回探靠的就是它。
    pub fn tripped(&self, now_ms: i64) -> bool {
        self.state(now_ms) == State::Open
    }

    /// 这一刻的降权系数,落在 `(0, 1]`。
    ///
    /// 半开与限流可能同时成立,取**更重的那个** —— 两个都在说「少用它」,
    /// 相乘会叠出一个过度的惩罚,而它们并不是独立的两件坏事。
    pub fn demotion(&self, now_ms: i64) -> f64 {
        let half_open = (self.state(now_ms) == State::HalfOpen).then_some(HALF_OPEN_DEMOTION);
        let limited = self
            .demoted_until_ms
            .filter(|&until| now_ms < until)
            .map(|_| RATE_LIMITED_DEMOTION);
        match (half_open, limited) {
            (Some(a), Some(b)) => a.min(b),
            (Some(v), None) | (None, Some(v)) => v,
            (None, None) => 1.0,
        }
    }

    /// 上一次熔断是因为什么。
    pub fn last_trip(&self) -> Option<Outcome> {
        self.last_trip
    }

    pub fn trips(&self) -> u32 {
        self.trips
    }

    /// 记一次请求结果。
    pub fn record(&mut self, outcome: Outcome, now_ms: i64) {
        match outcome {
            Outcome::Ok => {
                if self.state(now_ms) == State::HalfOpen {
                    self.half_open_ok += 1;
                    if self.half_open_ok < self.settings.half_open_successes.max(1) {
                        // 还没成功够数：仍是半开，等下一发探。
                        return;
                    }
                    self.recent.clear();
                } else {
                    self.push_recent(false);
                }
                self.consecutive_errors = 0;
                // 确认修好了:闭合,并且**把退避计数清零** ——
                // 不清的话,一条偶尔抽风的线会被越关越久,最后等同于永久拉黑。
                self.open_until_ms = None;
                self.demoted_until_ms = None;
                self.trips = 0;
                self.last_trip = None;
                self.half_open_ok = 0;
                self.probing = false;
            }
            Outcome::RateLimited { retry_after_ms } => {
                // **不动 consecutive_errors,也不熔断。** 被限流的线是通的。
                let cooldown = retry_after_ms
                    .filter(|ms| *ms > 0)
                    .unwrap_or(DEFAULT_RATE_LIMIT_COOLDOWN.as_millis() as i64);
                let until = now_ms.saturating_add(cooldown);
                // 取更晚的那个:连着两个 429,后一个不该把冷却期缩短。
                self.demoted_until_ms = Some(self.demoted_until_ms.unwrap_or(until).max(until));
            }
            Outcome::Rejected => {
                // Key 错了 / 没权限 / 没余额 —— 立即熔断,不数次数,而且关得久（sub2guard：确定性故障）。
                self.push_recent(true);
                self.trip_for(
                    Outcome::Rejected,
                    DETERMINISTIC_TRIP.as_millis() as i64,
                    now_ms,
                );
            }
            Outcome::ServerError => {
                self.push_recent(true);
                if self.state(now_ms) == State::HalfOpen {
                    // 探的那一发失败了：按退避立刻关回去（cc-switch 的 HalfOpen → Open）。
                    self.trip(Outcome::ServerError, now_ms);
                    return;
                }
                self.consecutive_errors += 1;
                if self.consecutive_errors >= self.settings.trip_after_consecutive.max(1)
                    || self.error_rate_exceeded()
                {
                    self.trip(Outcome::ServerError, now_ms);
                }
            }
        }
    }

    /// 最近窗口里失败比例过线了没有（窗口不够 `min_requests` 不算）。
    fn error_rate_exceeded(&self) -> bool {
        let n = self.recent.len();
        if n == 0 || n < self.settings.min_requests.max(1) as usize {
            return false;
        }
        let failed = self.recent.iter().filter(|&&f| f).count();
        failed as f64 / n as f64 >= self.settings.error_rate
    }

    fn push_recent(&mut self, failed: bool) {
        self.recent.push_back(failed);
        while self.recent.len() > ERROR_RATE_WINDOW {
            self.recent.pop_front();
        }
    }

    /// 熔断,按熔断过的次数退避。
    fn trip(&mut self, why: Outcome, now_ms: i64) {
        let shift = self.trips.min(16);
        let base = BASE_TRIP.as_millis() as i64;
        let span = base
            .saturating_mul(1i64.checked_shl(shift).unwrap_or(i64::MAX))
            .min(MAX_TRIP.as_millis() as i64);
        self.trip_for(why, span, now_ms);
    }

    fn trip_for(&mut self, why: Outcome, span_ms: i64, now_ms: i64) {
        self.trips = self.trips.saturating_add(1);
        self.consecutive_errors = 0;
        self.half_open_ok = 0;
        self.probing = false;
        // 熔断之后错误率从头算：不然关回来的那一刻窗口里还全是失败，第一发失败就又关上。
        self.recent.clear();
        self.open_until_ms = Some(now_ms.saturating_add(span_ms));
        self.last_trip = Some(why);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_700_000_000_000;
    const SEC: i64 = 1_000;

    /// 用连续的服务端错误把它关上（走退避那条路；`Rejected` 现在直接关一小时）。
    fn trip_by_errors(b: &mut RouteBreaker, at: i64) {
        for _ in 0..TRIP_AFTER_CONSECUTIVE_ERRORS {
            b.record(Outcome::ServerError, at);
        }
    }

    #[test]
    fn rate_limiting_demotes_but_never_trips() {
        // 被限流说明这条线是通的。熔断它等于把一条能用的线自己掐掉 ——
        // 而中转站的典型使用者恰恰经常撞限流。
        let mut b = RouteBreaker::new();
        for _ in 0..20 {
            b.record(
                Outcome::RateLimited {
                    retry_after_ms: Some(5 * SEC),
                },
                T0,
            );
        }
        assert!(!b.tripped(T0));
        assert_eq!(b.state(T0), State::Closed);
        assert_eq!(b.demotion(T0), RATE_LIMITED_DEMOTION);
        assert_eq!(b.trips(), 0);
        // 冷却期一过就恢复原样。
        assert_eq!(b.demotion(T0 + 5 * SEC), 1.0);
    }

    #[test]
    fn retry_after_is_honoured_and_never_shortened_by_a_later_429() {
        let mut b = RouteBreaker::new();
        b.record(
            Outcome::RateLimited {
                retry_after_ms: Some(120 * SEC),
            },
            T0,
        );
        // 紧接着又来一个只说「等 1 秒」的 —— 不许把 120 秒缩短。
        b.record(
            Outcome::RateLimited {
                retry_after_ms: Some(SEC),
            },
            T0,
        );
        assert_eq!(b.demotion(T0 + 60 * SEC), RATE_LIMITED_DEMOTION);
        assert_eq!(b.demotion(T0 + 121 * SEC), 1.0);
    }

    #[test]
    fn a_missing_retry_after_falls_back_to_the_default_cooldown() {
        let mut b = RouteBreaker::new();
        b.record(
            Outcome::RateLimited {
                retry_after_ms: None,
            },
            T0,
        );
        let d = DEFAULT_RATE_LIMIT_COOLDOWN.as_millis() as i64;
        assert_eq!(b.demotion(T0 + d - 1), RATE_LIMITED_DEMOTION);
        assert_eq!(b.demotion(T0 + d), 1.0);
    }

    #[test]
    fn auth_and_billing_failures_trip_on_the_first_hit() {
        // Key 错了、没权限、余额光了 —— 再打一百次也是同样的回答。
        let mut b = RouteBreaker::new();
        b.record(Outcome::Rejected, T0);
        assert!(b.tripped(T0));
        assert_eq!(b.last_trip(), Some(Outcome::Rejected));
        assert_eq!(
            b.open_until_ms(),
            Some(T0 + DETERMINISTIC_TRIP.as_millis() as i64)
        );
    }

    #[test]
    fn a_single_server_error_is_noise_and_five_in_a_row_is_a_fault() {
        let mut b = RouteBreaker::new();
        for i in 1..TRIP_AFTER_CONSECUTIVE_ERRORS {
            b.record(Outcome::ServerError, T0);
            assert!(!b.tripped(T0), "第 {i} 次就熔断了,抖动被放大成故障");
        }
        b.record(Outcome::ServerError, T0);
        assert!(b.tripped(T0));
        assert_eq!(b.last_trip(), Some(Outcome::ServerError));
    }

    #[test]
    fn one_success_resets_the_streak_so_scattered_errors_never_trip() {
        // 「连续」5 次,不是「累计」5 次。偶发错误不该攒着。
        let mut b = RouteBreaker::new();
        for _ in 0..20 {
            for _ in 0..TRIP_AFTER_CONSECUTIVE_ERRORS - 1 {
                b.record(Outcome::ServerError, T0);
            }
            b.record(Outcome::Ok, T0);
        }
        assert!(!b.tripped(T0));
    }

    #[test]
    fn rate_limiting_does_not_count_towards_the_server_error_streak() {
        // 429 混在 5xx 里时,不能帮着把熔断门槛推过去 —— 它是另一档。
        let mut b = RouteBreaker::new();
        for _ in 0..4 {
            b.record(Outcome::ServerError, T0);
        }
        for _ in 0..10 {
            b.record(
                Outcome::RateLimited {
                    retry_after_ms: None,
                },
                T0,
            );
        }
        assert!(!b.tripped(T0));
    }

    #[test]
    fn an_expired_trip_becomes_half_open_and_rejoins_the_pool_demoted() {
        // 回探就是「重新有资格,但带重降权」—— 只有没有更好的线时才轮到它。
        let mut b = RouteBreaker::new();
        trip_by_errors(&mut b, T0);
        let until = T0 + BASE_TRIP.as_millis() as i64;
        assert_eq!(b.state(until - 1), State::Open);
        assert!(b.tripped(until - 1));

        assert_eq!(b.state(until), State::HalfOpen);
        assert!(!b.tripped(until), "半开的线要留在池子里,否则永远探不回来");
        assert_eq!(b.demotion(until), HALF_OPEN_DEMOTION);
    }

    #[test]
    fn a_successful_probe_closes_the_breaker_and_clears_the_backoff() {
        // 不清退避的话,一条偶尔抽风的线会被越关越久,最后等同于永久拉黑 ——
        // 而「不拉黑」是这个项目写死的立场。
        let mut b = RouteBreaker::new();
        b.record(Outcome::Rejected, T0);
        b.record(Outcome::Rejected, T0); // 第二次,退避翻倍
        assert_eq!(b.trips(), 2);

        b.record(Outcome::Ok, T0);
        assert_eq!(b.state(T0), State::Closed);
        assert_eq!(b.demotion(T0), 1.0);
        assert_eq!(b.trips(), 0);
        assert_eq!(b.last_trip(), None);
    }

    #[test]
    fn repeated_trips_back_off_but_stop_at_the_cap() {
        // 有上限是有意的:站点修好了而我们还关着,
        // 对使用者来说跟站点没修好没区别。
        let mut b = RouteBreaker::new();
        let mut previous = 0i64;
        for _ in 0..12 {
            trip_by_errors(&mut b, T0);
            let span = b.open_until_ms.unwrap() - T0;
            assert!(span >= previous, "退避不该变短");
            assert!(span <= MAX_TRIP.as_millis() as i64, "超过了封顶");
            previous = span;
        }
        assert_eq!(previous, MAX_TRIP.as_millis() as i64);
    }

    #[test]
    fn half_open_and_rate_limited_take_the_heavier_penalty_not_the_product() {
        // 两个都在说「少用它」,相乘会叠出一个过度的惩罚,
        // 而它们并不是独立的两件坏事。
        let mut b = RouteBreaker::new();
        trip_by_errors(&mut b, T0);
        let until = T0 + BASE_TRIP.as_millis() as i64;
        b.record(
            Outcome::RateLimited {
                retry_after_ms: Some(10 * SEC),
            },
            until,
        );
        assert_eq!(b.state(until), State::HalfOpen);
        assert_eq!(
            b.demotion(until),
            HALF_OPEN_DEMOTION.min(RATE_LIMITED_DEMOTION)
        );
    }

    #[test]
    fn a_deterministic_failure_closes_the_line_for_an_hour_and_retest_lets_it_probe_now() {
        // sub2guard：余额 / 认证 / 退役是整把 Key 的状态，30 秒后再探只是白烧请求。
        let mut b = RouteBreaker::new();
        b.record(Outcome::Rejected, T0);
        let hour = DETERMINISTIC_TRIP.as_millis() as i64;
        assert!(b.tripped(T0 + hour - 1), "确定性故障不到一小时就放出来了");
        assert_eq!(b.state(T0 + hour), State::HalfOpen);
        // 「我充值了，立刻重测」：马上转半开，下一发真实请求就是回探 —— 不是直接闭合。
        assert!(b.retest(T0 + 10 * SEC));
        assert_eq!(b.state(T0 + 10 * SEC), State::HalfOpen);
        assert!(b.allow(T0 + 10 * SEC).allowed);
        // 没熔断着的线点「重测」什么都不变。
        let mut healthy = RouteBreaker::new();
        assert!(!healthy.retest(T0));
        assert_eq!(healthy.state(T0), State::Closed);
    }

    #[test]
    fn half_open_lets_exactly_one_probe_through_until_it_comes_back() {
        // cc-switch 的 `allow_half_open_probe`：半开只放一发。
        let mut b = RouteBreaker::new();
        trip_by_errors(&mut b, T0);
        let at = T0 + BASE_TRIP.as_millis() as i64;
        let probe = b.allow(at);
        assert!(probe.allowed && probe.half_open);
        assert!(!b.allow(at).allowed, "半开放了第二发");
        // 中性地还回来（这一发不算线路的账）：名额空出来，状态不动。
        b.release(probe);
        assert_eq!(b.state(at), State::HalfOpen);
        assert!(b.allow(at).allowed);
        // 闭合状态下放行不占名额。
        let mut closed = RouteBreaker::new();
        let p = closed.allow(T0);
        assert!(p.allowed && !p.half_open);
        assert!(closed.allow(T0).allowed);
    }

    #[test]
    fn a_failed_probe_shuts_the_line_again_at_once_with_a_longer_backoff() {
        let mut b = RouteBreaker::new();
        trip_by_errors(&mut b, T0);
        let at = T0 + BASE_TRIP.as_millis() as i64;
        let probe = b.allow(at);
        b.record_after(probe, Outcome::ServerError, at);
        assert_eq!(b.state(at), State::Open, "探的那一发失败了却没关回去");
        assert_eq!(b.trips(), 2);
        assert_eq!(
            b.open_until_ms(),
            Some(at + 2 * BASE_TRIP.as_millis() as i64)
        );
        // 关回去之后名额也空了（下一次半开时还能探）。
        let next = at + 2 * BASE_TRIP.as_millis() as i64;
        assert!(b.allow(next).allowed);
    }

    #[test]
    fn half_open_can_require_more_than_one_success_before_closing() {
        let mut b = RouteBreaker::with_settings(BreakerSettings {
            half_open_successes: 2,
            ..Default::default()
        });
        trip_by_errors(&mut b, T0);
        let at = T0 + BASE_TRIP.as_millis() as i64;
        let p = b.allow(at);
        b.record_after(p, Outcome::Ok, at);
        assert_eq!(b.state(at), State::HalfOpen, "只成功一次就闭合了");
        let p = b.allow(at);
        assert!(p.allowed, "第一发探完名额没还");
        b.record_after(p, Outcome::Ok, at);
        assert_eq!(b.state(at), State::Closed);
        assert_eq!(b.trips(), 0);
    }

    #[test]
    fn a_high_recent_error_rate_trips_even_without_five_in_a_row() {
        // cc-switch 的错误率阈值：失败夹着成功，连续计数永远到不了 5，但一半以上都在失败。
        let mut b = RouteBreaker::with_settings(BreakerSettings {
            error_rate: 0.6,
            min_requests: 10,
            ..Default::default()
        });
        for i in 0..10 {
            b.record(
                if i % 3 == 1 {
                    Outcome::Ok
                } else {
                    Outcome::ServerError
                },
                T0,
            );
        }
        assert!(b.tripped(T0), "最近 10 次里 7 次失败却没熔断");
        assert_eq!(b.last_trip(), Some(Outcome::ServerError));
    }

    #[test]
    fn the_error_rate_waits_for_enough_samples_and_ignores_rate_limits() {
        let mut b = RouteBreaker::new();
        // 样本不够：3 次里 2 次失败不算。
        b.record(Outcome::ServerError, T0);
        b.record(Outcome::Ok, T0);
        b.record(Outcome::ServerError, T0);
        assert!(!b.tripped(T0));
        // 限流不进窗口：再来 50 个 429 也推不过阈值。
        for _ in 0..50 {
            b.record(
                Outcome::RateLimited {
                    retry_after_ms: None,
                },
                T0,
            );
        }
        assert!(!b.tripped(T0));
    }

    #[test]
    fn settings_change_without_resetting_the_state_and_old_data_reads_back() {
        let mut b = RouteBreaker::new();
        trip_by_errors(&mut b, T0);
        b.set_settings(BreakerSettings {
            trip_after_consecutive: 2,
            ..Default::default()
        });
        assert!(b.tripped(T0), "换参数把正熔断着的线放出来了");
        assert_eq!(b.settings().trip_after_consecutive, 2);
        // 存下来的设置缺字段时按默认补。
        let parsed: BreakerSettings = serde_json::from_str("{\"error_rate\":0.7}").unwrap();
        assert_eq!(parsed.error_rate, 0.7);
        assert_eq!(parsed.trip_after_consecutive, TRIP_AFTER_CONSECUTIVE_ERRORS);
    }

    #[test]
    fn a_fresh_breaker_is_closed_and_undemoted() {
        let b = RouteBreaker::new();
        assert_eq!(b.state(T0), State::Closed);
        assert!(!b.tripped(T0));
        assert_eq!(b.demotion(T0), 1.0);
        assert_eq!(b.last_trip(), None);
    }
}
