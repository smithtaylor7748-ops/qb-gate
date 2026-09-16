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
    /// 窗口里**报了成败**的行数。`requests` 减掉它就是「站点没说」的那些。
    pub status_rows: u64,
    /// 成功率。窗口里一条记录都没有时是 `None`,不是 100%。
    ///
    /// **分母是 `status_rows`,不是 `requests`。** 消费日志里没扣到钱的失败
    /// 请求很可能压根不出现,拿 `requests` 当分母会让每一家站点都是 100% ——
    /// 而这个数正是智能调度里「稳」那一维的输入。
    pub success_rate: Option<f64>,
    /// 缓存命中率 = Σ缓存读 ÷ Σ输入总量。**只累加两者都取证到的行。**
    pub cache_hit_rate: Option<f64>,
    pub ttft_p50_ms: Option<u64>,
    /// P95 比均值有用得多:长尾才是「这站有时候卡死」的真相,
    /// 均值会被大量正常请求稀释掉。
    pub ttft_p95_ms: Option<u64>,
    /// 每个输出 token 的生成耗时。Σ总耗时 ÷ Σ输出 token。
    pub ms_per_token: Option<f64>,
    /// 体验分:**这条线跑完一次典型回答大概要多久**(毫秒,越小越好)。
    ///
    /// ⛔ 排序用它,不要用 `ttft_p95_ms`。
    ///
    /// # 为什么不能只看首字
    ///
    /// 实测(使用者自己的 sub2guard 外挂,2026-09-08 ㉚):
    ///
    /// | 账号 | 首字 p50 | ms/token | 一次回答 |
    /// |---|---|---|---|
    /// | 2690 | **1.4 秒(全组最快)** | 107.9 | **95.8 秒** |
    /// | 2681 | 4.7 秒 | 21.1 | 快 5 倍 |
    ///
    /// 按首字排,2690 一直排第一,而使用者报的「卡」就是它 —— 卡在首字**之后**。
    /// 首字慢一点但生成快 5 倍的那条被压在第三,拿不到流量。
    ///
    /// 算不出来时回落到 `ttft_p95_ms`:比「什么证据都没有」强,
    /// 只是看不见首字之后的卡顿。
    pub experience_ms: Option<f64>,
    pub tokens: Option<u64>,
    pub cost: Option<f64>,
    /// 这个窗口里四类 token 各用了多少。
    ///
    /// ⛔ **比价必须按它加权。** 一条线「输入便宜、输出翻五倍」,另一条
    /// 「输入贵、输出不翻倍」—— 谁便宜完全取决于你的输入输出比。
    /// 只比一个总倍率答不了这个问题。
    pub mix: TokenMix,
}

/// 四类 token 各自的用量。**每一类都可能没取证到。**
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TokenMix {
    pub input: Option<u64>,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
    pub output: Option<u64>,
}

impl TokenMix {
    /// 四类占比,顺序同 `pricing::PriceCategory::ALL`。
    ///
    /// 四类**都要取证到**才给占比:少一类就把分母算小,剩下几类的权重被抬高,
    /// 加权出来的单价整个偏掉。
    pub fn shares(&self) -> Option<[f64; 4]> {
        let (i, r, w, o) = (
            self.input?,
            self.cache_read?,
            self.cache_write?,
            self.output?,
        );
        let total = (i + r + w + o) as f64;
        (total > 0.0).then(|| {
            [
                i as f64 / total,
                r as f64 / total,
                w as f64 / total,
                o as f64 / total,
            ]
        })
    }

    /// 「新对话」的占比:**没有缓存读**。
    ///
    /// 开一轮新对话时上游那边还没有这段上下文的缓存,所以缓存读为零、
    /// 缓存写照付。一条靠高缓存命中撑起来的便宜线,在新对话的第一轮上
    /// 并不便宜 —— 界面要能分开看这两种口径。
    pub fn fresh_conversation(&self) -> Option<[f64; 4]> {
        let (i, r, w, o) = (
            self.input?,
            self.cache_read?,
            self.cache_write?,
            self.output?,
        );
        // 原本读缓存的那部分,新对话里要当成未缓存输入付一次,并写一次缓存。
        let input = i + r;
        let total = (input + w + o) as f64;
        (total > 0.0).then(|| {
            [
                input as f64 / total,
                0.0,
                w as f64 / total,
                o as f64 / total,
            ]
        })
    }
}

/// 一次「典型回答」按多少个输出 token 算。
///
/// 它只决定「首字」和「生成速度」在体验分里的配比,**不影响相对快慢** ——
/// 两条线谁快谁慢跟这个数取多少无关,它只是把两个量纲接到一起。
/// 500 取自使用者本站 `usage_logs` 近 7 天的平均输出长度(541,中位数 200)。
pub const TYPICAL_OUTPUT_TOKENS: f64 = 500.0;

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
    let mut gen_ms: u64 = 0;
    let mut gen_tokens: u64 = 0;
    let (mut mix_in, mut mix_read, mut mix_write, mut mix_out) = (0u64, 0u64, 0u64, 0u64);
    let (mut has_in, mut has_read, mut has_write, mut has_out) = (false, false, false, false);

    for r in rows {
        // 左端开、右端闭。时间戳在未来的行直接丢 —— 时钟漂移或者站点回了个
        // 错误的时间,都不该让它污染「最近 1 小时」。
        if r.at_ms <= floor || r.at_ms > now_ms {
            continue;
        }
        w.requests += 1;
        // 没报成败的行不进成功率的分子分母 —— 它在缓存、首字、花费上仍然算数。
        if r.status_reported {
            w.status_rows += 1;
            if !r.ok {
                w.failures += 1;
            }
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
        // 生成速度:两项都要取证到才累加。少一边算出来的 ms/token 是假的。
        if let (Some(total), Some(out)) = (r.total_ms, r.output) {
            if out > 0 {
                gen_ms += total;
                gen_tokens += out;
            }
        }
        // 四类用量各自累加。哪一类一行都没报过,那一类就是「没取证到」。
        if let Some(v) = r.input_uncached {
            mix_in += v;
            has_in = true;
        }
        if let Some(v) = r.cache_read {
            mix_read += v;
            has_read = true;
        }
        if let Some(v) = r.cache_write {
            mix_write += v;
            has_write = true;
        }
        if let Some(v) = r.output {
            mix_out += v;
            has_out = true;
        }
    }

    if w.status_rows > 0 {
        w.success_rate = Some((w.status_rows - w.failures) as f64 / w.status_rows as f64);
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
    if gen_tokens > 0 {
        w.ms_per_token = Some(gen_ms as f64 / gen_tokens as f64);
    }
    // 体验分 = 首字 P95 + 每 token 耗时 × 典型回答长度。
    // 算不出生成速度时回落到纯首字 —— 比没有证据强,只是看不见后半段的卡顿。
    w.mix = TokenMix {
        input: has_in.then_some(mix_in),
        cache_read: has_read.then_some(mix_read),
        cache_write: has_write.then_some(mix_write),
        output: has_out.then_some(mix_out),
    };
    w.experience_ms = match (w.ttft_p95_ms, w.ms_per_token) {
        (Some(t), Some(mpt)) => Some(t as f64 + mpt * TYPICAL_OUTPUT_TOKENS),
        (Some(t), None) => Some(t as f64),
        _ => None,
    };
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
            status_reported: true,
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
            status_reported: true,
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
    fn a_log_that_never_reports_status_yields_no_success_rate_rather_than_100_percent() {
        // `/api/log/self` 是消费日志:没扣到钱的失败请求很可能压根不出现。
        // 拿行数当分母的话,每一家站点都是 100% —— 而这个数正是智能调度里
        // 「稳」那一维的输入,等于凭空替所有站点作证,最弱项规则再也卡不到可靠性。
        let now = 10 * HOUR_MS;
        let rows: Vec<UsageRow> = (1..=5)
            .map(|i| UsageRow {
                at_ms: now - i,
                cost: Some(0.01),
                first_token_ms: Some(300),
                ok: true,
                status_reported: false,
                ..Default::default()
            })
            .collect();
        let w = aggregate(&rows, now, HOUR_MS);
        assert_eq!(w.requests, 5);
        assert_eq!(w.status_rows, 0);
        assert_eq!(w.success_rate, None, "没报成败却给出了成功率");
        // 但这些行在别的维度上仍然是有效证据 —— 不能整行作废。
        assert_eq!(w.ttft_p50_ms, Some(300));
        assert_eq!(w.cost, Some(0.05));
    }

    #[test]
    fn success_rate_ignores_rows_whose_status_was_never_reported() {
        // 混着来的时候,分母只能是报了成败的那几行。
        let now = 10 * HOUR_MS;
        let mut bad = row(now - 1);
        bad.ok = false;
        let silent = UsageRow {
            at_ms: now - 2,
            ok: true,
            status_reported: false,
            ..Default::default()
        };
        let rows = [row(now - 3), bad, silent];
        let w = aggregate(&rows, now, HOUR_MS);
        assert_eq!((w.requests, w.status_rows, w.failures), (3, 2, 1));
        assert_eq!(w.success_rate, Some(0.5));
    }

    #[test]
    fn the_token_mix_is_what_makes_two_pricing_shapes_comparable() {
        // 一条「输入便宜、输出翻五倍」，一条「输入贵、输出不翻倍」——
        // 谁便宜取决于输入输出比，只比一个总倍率答不了。
        let now = 10 * HOUR_MS;
        let rows = [UsageRow {
            at_ms: now - 1,
            input_uncached: Some(600),
            cache_read: Some(300),
            cache_write: Some(100),
            output: Some(1000),
            ok: true,
            status_reported: true,
            ..Default::default()
        }];
        let w = aggregate(&rows, now, HOUR_MS);
        let s = w.mix.shares().unwrap();
        assert!((s[0] - 0.3).abs() < 1e-9);
        assert!((s[3] - 0.5).abs() < 1e-9, "输出占一半");
        assert!((s.iter().sum::<f64>() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_fresh_conversation_has_no_cache_reads() {
        // 靠高缓存命中撑起来的便宜线，在新对话第一轮上并不便宜 ——
        // 那段上下文上游还没缓存过。
        let now = 10 * HOUR_MS;
        let rows = [UsageRow {
            at_ms: now - 1,
            input_uncached: Some(200),
            cache_read: Some(800),
            cache_write: Some(0),
            output: Some(0),
            ok: true,
            status_reported: true,
            ..Default::default()
        }];
        let w = aggregate(&rows, now, HOUR_MS);
        let normal = w.mix.shares().unwrap();
        let fresh = w.mix.fresh_conversation().unwrap();
        assert!((normal[1] - 0.8).abs() < 1e-9, "平时八成走缓存读");
        assert_eq!(fresh[1], 0.0, "新对话不该有缓存读");
        assert!((fresh[0] - 1.0).abs() < 1e-9, "那八成要按未缓存输入付");
    }

    #[test]
    fn a_missing_category_voids_the_mix_rather_than_skewing_it() {
        // 少一类就把分母算小，剩下几类的权重被抬高，加权单价整个偏掉。
        let now = 10 * HOUR_MS;
        let rows = [UsageRow {
            at_ms: now - 1,
            input_uncached: Some(100),
            cache_read: None,
            cache_write: Some(0),
            output: Some(100),
            ok: true,
            status_reported: true,
            ..Default::default()
        }];
        let w = aggregate(&rows, now, HOUR_MS);
        assert_eq!(w.mix.shares(), None);
        assert_eq!(w.mix.fresh_conversation(), None);
    }

    #[test]
    fn the_fastest_first_token_can_still_be_the_slowest_answer() {
        // 实测(sub2guard ㉚):2690 首字全组最快,却是使用者报「卡」的那一条 ——
        // 它 107.9 ms/token,一次回答要吐 95.8 秒;2681 首字慢 3 倍但快 5 倍。
        // 只按首字排会把最卡的排第一。
        let now = 10 * HOUR_MS;
        let quick_start = UsageRow {
            at_ms: now - 1,
            first_token_ms: Some(1_400),
            total_ms: Some(95_800),
            output: Some(500),
            ok: true,
            status_reported: true,
            ..Default::default()
        };
        let quick_finish = UsageRow {
            at_ms: now - 2,
            first_token_ms: Some(4_700),
            total_ms: Some(19_000),
            output: Some(500),
            ok: true,
            status_reported: true,
            ..Default::default()
        };
        let a = aggregate(&[quick_start], now, HOUR_MS);
        let b = aggregate(&[quick_finish], now, HOUR_MS);

        // 首字：a 快得多。
        assert!(a.ttft_p95_ms < b.ttft_p95_ms);
        // 体验分：b 才是真的快。**排序要用这个。**
        assert!(
            b.experience_ms < a.experience_ms,
            "体验分没把「首字之后的卡顿」算进去：a={:?} b={:?}",
            a.experience_ms,
            b.experience_ms
        );
    }

    #[test]
    fn without_generation_evidence_the_score_falls_back_to_first_token() {
        // 比「什么证据都没有」强,只是看不见首字之后的卡顿。
        let now = 10 * HOUR_MS;
        let r = UsageRow {
            at_ms: now - 1,
            first_token_ms: Some(800),
            ok: true,
            status_reported: true,
            ..Default::default()
        };
        let w = aggregate(&[r], now, HOUR_MS);
        assert_eq!(w.ms_per_token, None);
        assert_eq!(w.experience_ms, Some(800.0));
    }

    #[test]
    fn zero_output_tokens_do_not_become_an_infinite_generation_speed() {
        let now = 10 * HOUR_MS;
        let r = UsageRow {
            at_ms: now - 1,
            first_token_ms: Some(800),
            total_ms: Some(5_000),
            output: Some(0),
            ok: true,
            status_reported: true,
            ..Default::default()
        };
        let w = aggregate(&[r], now, HOUR_MS);
        assert_eq!(w.ms_per_token, None);
        assert_eq!(w.experience_ms, Some(800.0));
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
