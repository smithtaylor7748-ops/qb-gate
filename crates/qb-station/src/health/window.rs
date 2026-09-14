//! 按时间窗把账单日志聚合成界面上那几个数。**纯函数,不联网、不落盘。**
//!
//! # 为什么这些数是免费的
//!
//! 缓存命中率、首字延迟、成功率、24 小时花费,全都能从站点自己的账单日志
//! (New API 的 `/api/log/self`)算出来 —— **零成本、零额外请求,而且用的是
//! 真实流量而不是探针**。探针请求的形状跟生产请求不一样,测出来的延迟本来就不可比;
//! 账单里的 `other.frt` 是你自己那些真请求的首字延迟,准得多。
//!
//! 所以「自动刷新」这件事可以放心做:它一分钱都不花。要花钱的只有两样 ——
//! 打真模型测速、和查套路,那两个必须手动点。
//!
//! # 一条铁律:取不到就是 `None`,绝不填 0
//!
//! 站点少报一个字段时拿 0 顶上,后果不是「少了一点精度」:缓存命中率的分母会变小、
//! 命中率被抬高 —— 而那正好是造假站点希望你看到的方向。界面上 `None` 显示「—」,
//! 跟真的 0% 是两种完全不同的处置。

use crate::station::model::UsageRow;

/// 一个时间窗的聚合结果。每一项都可能是「这个窗口里没有证据」。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Window {
    pub requests: u64,
    pub failures: u64,
    /// 成功率。窗口里一条记录都没有时是 `None`,不是 100%。
    pub success_rate: Option<f64>,
    /// 缓存命中率 = Σ缓存读 ÷ Σ输入总量。**只累加两者都取证到的行。**
    pub cache_hit_rate: Option<f64>,
    pub ttft_p50_ms: Option<u64>,
    /// P95 比均值有用得多:长尾才是「这站有时候卡死」的真相,
    /// 均值会被大量正常请求稀释掉。
    pub ttft_p95_ms: Option<u64>,
    pub tokens: Option<u64>,
    pub cost: Option<f64>,
}

pub const HOUR_MS: i64 = 3_600_000;
pub const DAY_MS: i64 = 86_400_000;
pub const WEEK_MS: i64 = 7 * DAY_MS;

/// 把落在 `(now - span, now]` 里的行聚合起来。
///
/// **可见性是故意收窄的。** 一旦有人把这段算法搬进调用方自己写一遍,这个函数
/// 就没人调了,`cargo clippy --lib -- -D warnings` 会因为 dead_code 直接编译失败
/// —— 而不是留下两份会各自漂移的实现。
/// (单测不参与 `--lib` 编译,所以「只剩测试在调」＝没人调。)
pub(super) fn aggregate(rows: &[UsageRow], now_ms: i64, span_ms: i64) -> Window {
    let floor = now_ms.saturating_sub(span_ms);
    let mut w = Window::default();

    let mut cache_read_sum: u64 = 0;
    let mut input_total_sum: u64 = 0;
    let mut cache_rows = 0u64;
    let mut token_sum: u64 = 0;
    let mut token_rows = 0u64;
    let mut cost_sum = 0.0f64;
    let mut cost_rows = 0u64;
    let mut ttft: Vec<u64> = Vec::new();

    for r in rows {
        // 左端开、右端闭。时间戳在未来的行直接丢 —— 时钟漂移或者站点回了个
        // 错误的时间,都不该让它污染「最近 1 小时」。
        if r.at_ms <= floor || r.at_ms > now_ms {
            continue;
        }
        w.requests += 1;
        if !r.ok {
            w.failures += 1;
        }
        if let (Some(read), Some(total)) = (r.cache_read, r.input_total()) {
            cache_read_sum += read;
            input_total_sum += total;
            cache_rows += 1;
        }
        if let (Some(total), Some(out)) = (r.input_total(), r.output) {
            token_sum += total + out;
            token_rows += 1;
        }
        if let Some(c) = r.cost.filter(|c| c.is_finite()) {
            cost_sum += c;
            cost_rows += 1;
        }
        if let Some(ms) = r.first_token_ms {
            ttft.push(ms);
        }
    }

    if w.requests > 0 {
        w.success_rate = Some((w.requests - w.failures) as f64 / w.requests as f64);
    }
    // 分母为 0 时是 `None` 而不是 0.0:有记录但输入量全是 0,说明这批行的 token
    // 字段没取证到,算不出命中率。给个 0% 等于污蔑这家站不做缓存。
    if cache_rows > 0 && input_total_sum > 0 {
        w.cache_hit_rate = Some(cache_read_sum as f64 / input_total_sum as f64);
    }
    if token_rows > 0 {
        w.tokens = Some(token_sum);
    }
    if cost_rows > 0 {
        w.cost = Some(cost_sum);
    }
    if !ttft.is_empty() {
        ttft.sort_unstable();
        w.ttft_p50_ms = Some(percentile(&ttft, 50));
        w.ttft_p95_ms = Some(percentile(&ttft, 95));
    }
    w
}

/// 最近秩法(nearest-rank):第 ⌈p/100 × n⌉ 个样本。
///
/// 不做插值是有意的 —— 插值会造出一个**样本里根本没出现过的延迟值**,
/// 而这些数字是要拿去跟「这站是不是卡」对质的,给一个真实发生过的值更站得住。
fn percentile(sorted: &[u64], p: u64) -> u64 {
    debug_assert!(!sorted.is_empty());
    let n = sorted.len() as u64;
    let rank = (p * n).div_ceil(100).max(1);
    sorted[(rank - 1).min(n - 1) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(at_ms: i64) -> UsageRow {
        UsageRow {
            at_ms,
            ok: true,
            ..Default::default()
        }
    }

    fn full(at_ms: i64, read: u64, uncached: u64, write: u64, frt: u64) -> UsageRow {
        UsageRow {
            at_ms,
            input_uncached: Some(uncached),
            cache_read: Some(read),
            cache_write: Some(write),
            output: Some(100),
            first_token_ms: Some(frt),
            cost: Some(0.01),
            ok: true,
            ..Default::default()
        }
    }

    #[test]
    fn an_empty_window_reports_nothing_rather_than_zero() {
        // 这条是整个模块的立身之本:没有证据时界面要显示「—」。
        // 返回 0% 命中率 / 100% 成功率,等于凭空替站点作证。
        let w = aggregate(&[], 1_000_000, HOUR_MS);
        assert_eq!(w.requests, 0);
        assert_eq!(w.success_rate, None);
        assert_eq!(w.cache_hit_rate, None);
        assert_eq!(w.ttft_p50_ms, None);
        assert_eq!(w.tokens, None);
        assert_eq!(w.cost, None);
    }

    #[test]
    fn rows_outside_the_window_are_dropped_including_future_ones() {
        let now = 10 * HOUR_MS;
        let rows = [
            row(now - 2 * HOUR_MS), // 太旧
            row(now - HOUR_MS),     // 正好在左端,开区间,丢掉
            row(now - HOUR_MS + 1), // 在窗口里
            row(now),               // 右端闭,算
            row(now + 1),           // 未来的,丢掉
        ];
        assert_eq!(aggregate(&rows, now, HOUR_MS).requests, 2);
    }

    #[test]
    fn cache_hit_rate_only_counts_rows_that_reported_every_token_field() {
        let now = 10 * HOUR_MS;
        let rows = [
            full(now - 1, 900, 100, 0, 500),
            // 这行没报 cache_read —— 不能当 0 累进分母,否则命中率被稀释。
            UsageRow {
                at_ms: now - 2,
                input_uncached: Some(10_000),
                cache_read: None,
                cache_write: Some(0),
                output: Some(10),
                ok: true,
                ..Default::default()
            },
        ];
        let w = aggregate(&rows, now, HOUR_MS);
        assert_eq!(w.requests, 2);
        assert_eq!(w.cache_hit_rate, Some(0.9));
    }

    #[test]
    fn a_missing_cache_write_field_voids_the_row_rather_than_inflating_the_rate() {
        // 少一项就把分母算小、命中率抬高 —— 那正好是造假站点想让你看到的方向。
        let now = 10 * HOUR_MS;
        let rows = [UsageRow {
            at_ms: now - 1,
            input_uncached: Some(100),
            cache_read: Some(900),
            cache_write: None,
            ok: true,
            ..Default::default()
        }];
        let w = aggregate(&rows, now, HOUR_MS);
        assert_eq!(w.requests, 1);
        assert_eq!(w.cache_hit_rate, None);
    }

    #[test]
    fn zero_input_volume_is_unknown_not_zero_percent() {
        let now = 10 * HOUR_MS;
        let rows = [UsageRow {
            at_ms: now - 1,
            input_uncached: Some(0),
            cache_read: Some(0),
            cache_write: Some(0),
            ok: true,
            ..Default::default()
        }];
        assert_eq!(aggregate(&rows, now, HOUR_MS).cache_hit_rate, None);
    }

    #[test]
    fn success_rate_counts_failures() {
        let now = 10 * HOUR_MS;
        let mut bad = row(now - 1);
        bad.ok = false;
        let rows = [row(now - 2), row(now - 3), row(now - 4), bad];
        let w = aggregate(&rows, now, HOUR_MS);
        assert_eq!((w.requests, w.failures), (4, 1));
        assert_eq!(w.success_rate, Some(0.75));
    }

    #[test]
    fn percentiles_use_nearest_rank_and_return_real_samples() {
        let s: Vec<u64> = (1..=10).map(|i| i * 100).collect();
        assert_eq!(percentile(&s, 50), 500);
        assert_eq!(percentile(&s, 95), 1000);
        assert_eq!(percentile(&s, 1), 100);
        // 单样本不能越界
        assert_eq!(percentile(&[42], 95), 42);
    }

    #[test]
    fn ttft_ignores_rows_that_never_reported_it() {
        let now = 10 * HOUR_MS;
        let rows = [
            full(now - 1, 0, 10, 0, 300),
            full(now - 2, 0, 10, 0, 900),
            row(now - 3), // 没有 frt
        ];
        let w = aggregate(&rows, now, HOUR_MS);
        assert_eq!(w.requests, 3);
        assert_eq!(w.ttft_p50_ms, Some(300));
        assert_eq!(w.ttft_p95_ms, Some(900));
    }

    #[test]
    fn the_three_spans_nest() {
        // 1H ⊆ 1D ⊆ 7D。分窗口算错的话,界面上会出现「1 小时比 7 天还多」。
        let now = 30 * DAY_MS;
        let rows = [
            full(now - 10 * 60_000, 1, 1, 0, 100),
            full(now - 5 * HOUR_MS, 1, 1, 0, 100),
            full(now - 3 * DAY_MS, 1, 1, 0, 100),
            full(now - 30 * DAY_MS, 1, 1, 0, 100),
        ];
        assert_eq!(aggregate(&rows, now, HOUR_MS).requests, 1);
        assert_eq!(aggregate(&rows, now, DAY_MS).requests, 2);
        assert_eq!(aggregate(&rows, now, WEEK_MS).requests, 3);
    }
}
