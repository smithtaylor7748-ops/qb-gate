//! 两级熔断。纯状态机,**不取系统时间** —— `now_ms` 一律由调用方传进来。
//!
//! # 两级,不是一级
//!
//! | 上游回了什么 | 处置 | 为什么 |
//! |---|---|---|
//! | `429` | **只降权,不熔断**;读 `Retry-After` | 被限流说明这条线是**通的**,只是你用太快了。熔断它等于把一条能用的线自己掐掉 |
//! | `401` / `403` / `402` | **立即熔断** | Key 错了、没权限、余额光了。再打一百次也是同样的回答,只是白烧请求 |
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

use std::time::Duration;

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
}

impl Default for RouteBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteBreaker {
    pub fn new() -> Self {
        Self {
            consecutive_errors: 0,
            open_until_ms: None,
            demoted_until_ms: None,
            trips: 0,
            last_trip: None,
        }
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
                self.consecutive_errors = 0;
                // 一次成功就确认修好了:闭合,并且**把退避计数清零** ——
                // 不清的话,一条偶尔抽风的线会被越关越久,最后等同于永久拉黑。
                self.open_until_ms = None;
                self.demoted_until_ms = None;
                self.trips = 0;
                self.last_trip = None;
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
                // Key 错了 / 没权限 / 没余额 —— 立即熔断,不数次数。
                self.trip(Outcome::Rejected, now_ms);
            }
            Outcome::ServerError => {
                self.consecutive_errors += 1;
                if self.consecutive_errors >= TRIP_AFTER_CONSECUTIVE_ERRORS {
                    self.trip(Outcome::ServerError, now_ms);
                }
            }
        }
    }

    /// 熔断,按熔断过的次数退避。
    fn trip(&mut self, why: Outcome, now_ms: i64) {
        let shift = self.trips.min(16);
        let base = BASE_TRIP.as_millis() as i64;
        let span = base
            .saturating_mul(1i64.checked_shl(shift).unwrap_or(i64::MAX))
            .min(MAX_TRIP.as_millis() as i64);
        self.trips = self.trips.saturating_add(1);
        self.consecutive_errors = 0;
        self.open_until_ms = Some(now_ms.saturating_add(span));
        self.last_trip = Some(why);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_700_000_000_000;
    const SEC: i64 = 1_000;

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
        for why in [Outcome::Rejected] {
            let mut b = RouteBreaker::new();
            b.record(why, T0);
            assert!(b.tripped(T0));
            assert_eq!(b.last_trip(), Some(Outcome::Rejected));
        }
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
        b.record(Outcome::Rejected, T0);
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
            b.record(Outcome::Rejected, T0);
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
        b.record(Outcome::Rejected, T0);
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
    fn a_fresh_breaker_is_closed_and_undemoted() {
        let b = RouteBreaker::new();
        assert_eq!(b.state(T0), State::Closed);
        assert!(!b.tripped(T0));
        assert_eq!(b.demotion(T0), 1.0);
        assert_eq!(b.last_trip(), None);
    }
}
