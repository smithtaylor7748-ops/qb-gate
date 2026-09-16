//! 账户用量小结：今天用了多少 token、缓存命中多少、缓存替你省下多少钱。
//!
//! 跨域编排，所以住在这一层 —— token 从 `qb-accounts` 数出来，
//! 官方单价在 `qb-station`，两个是同层 crate，谁也不能依赖谁
//! （`crates_only_depend_downwards` 卡着）。
//!
//! # ⛔ 这里的美元是「缓存省下的」，不是「你花了多少」
//!
//! 订阅账户按官方 API 单价折出来的总额**不是你实际付的钱**（你付的是月费），
//! 摆出来只会误导 —— 所以这里不给「花了多少」，只给一个能说清口径的数：
//!
//! ```text
//! 省下的 = 缓存读的 token 数 ×（输入单价 − 缓存读单价）
//! ```
//!
//! 也就是「这些 token 如果不走缓存、按整价重新读一遍，要多花多少」。
//! 它回答的是「缓存有没有用」，不冒充账单。界面上必须把这句口径写出来。
//!
//! # ⛔ 取价要走 `Catalog`，不是 `pricing::lookup`
//!
//! `pricing::lookup` 只查**编译进去的那张快照表**，而那张表里当前只有三个键
//! （`claude-fable-5` / `claude-fable-5-1` / `gpt-5.6-sol`）。实机转写里的模型是
//! `claude-opus-5` / `claude-opus-4-8` / `claude-sonnet-5` —— 一个都查不到，
//! 于是 `saved_usd` 恒为 `None`，界面上那一格**永远是个破折号**。
//!
//! 而面板启动时抓回来的价就存在库里（`station_prices`，实测 50 条、
//! 含上面那三个模型），只是这条路从来没去读它。所以这里收一个
//! [`pricing::Catalog`]：它先查抓回来的，再退回内置快照。
//! 装配在命令层（`src-tauri`）—— 那里才够得着 `Repository`。
//!
//! 认不出的模型直接跳过，不拿别的模型的价去凑 ——
//! 凑出来的数字看起来一样正常。

use serde::Serialize;
use ts_rs::TS;

use crate::accounts::tokens::{self, TokenBucket};
use qb_station::station::pricing;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TokenSummary {
    /// 这一档统计的是几天（`1` = 今天，`0` = 全部）。
    #[ts(type = "number")]
    pub days: i64,
    #[ts(type = "number")]
    pub input: i64,
    #[ts(type = "number")]
    pub output: i64,
    #[ts(type = "number")]
    pub cache_write: i64,
    #[ts(type = "number")]
    pub cache_read: i64,
    /// 回复数。
    #[ts(type = "number")]
    pub messages: i64,
    /// 缓存命中率 0–1：`缓存读 ÷（输入 + 缓存读 + 缓存写）`。
    ///
    /// **输出不进分母** —— 输出是生成出来的，从来就没有「命中」一说，
    /// 把它算进去只会让这个比例随着回答长短乱飘。
    /// 分母为 0 时是 `None`（还没读过任何上文），不是 0。
    pub hit_rate: Option<f32>,
    /// 缓存省下多少（美元）。口径见模块头。算不出来就是 `None`。
    pub saved_usd: Option<f32>,
    /// 有几个模型的官方价没认出来，那部分没算进 `saved_usd`。
    #[ts(type = "number")]
    pub unpriced_models: i64,
    /// 同一档时间范围里，**归不到任何槽位**的用量（默认目录里没有归属标记的
    /// 那部分）。见 `tokens.rs` 文件头：今天这一整天的记录全都没有标记，
    /// 所以这个数经常比账户自己的还大。界面必须单独显示，不许并进总数。
    #[ts(type = "number")]
    pub unattributed: i64,
    /// 上面那部分有几条回复。
    #[ts(type = "number")]
    pub unattributed_messages: i64,
    /// 用的是编译进去的价格快照（`true`）还是抓回来的官方价（`false`）。
    /// 界面要分得出来 —— 快照可能已经过期。
    ///
    /// 一个价都没取到时是 `None`：那时候「用的是快照还是实时价」根本不成立，
    /// 报 `true` 等于说「用了快照」，而实际什么都没用上。
    pub priced_from_snapshot: Option<bool>,
}

/// 把桶按天数筛一遍再合计。
///
/// `days`：`1` = 今天（本地日历日），`7` = 含今天的最近七天，`0` = 全部。
/// 比的是日期串本身（桶的 `day` 是本地日期），**不经过 UTC** ——
/// 换算一次就会在时区偏移上把「今天」切错一天。
pub fn summarize(
    buckets: &[TokenBucket],
    days: i64,
    today: &str,
    prices: &pricing::Catalog,
) -> TokenSummary {
    summarize_with(buckets, &[], days, today, prices)
}

/// 同上，外加「归不到槽位」那一列。
pub fn summarize_with(
    buckets: &[TokenBucket],
    unattributed: &[TokenBucket],
    days: i64,
    today: &str,
    prices: &pricing::Catalog,
) -> TokenSummary {
    let since = since_day(days, today);
    let picked: Vec<&TokenBucket> = buckets
        .iter()
        .filter(|b| since.as_deref().is_none_or(|s| b.day.as_str() >= s))
        .collect();

    let mut out = TokenSummary {
        days,
        input: 0,
        output: 0,
        cache_write: 0,
        cache_read: 0,
        messages: 0,
        hit_rate: None,
        saved_usd: None,
        unattributed: 0,
        unattributed_messages: 0,
        unpriced_models: 0,
        priced_from_snapshot: None,
    };
    for b in unattributed
        .iter()
        .filter(|b| since.as_deref().is_none_or(|s| b.day.as_str() >= s))
    {
        out.unattributed += b.input + b.output + b.cache_write + b.cache_read;
        out.unattributed_messages += b.messages;
    }
    for b in &picked {
        out.input += b.input;
        out.output += b.output;
        out.cache_write += b.cache_write;
        out.cache_read += b.cache_read;
        out.messages += b.messages;
    }

    let read_total = out.input + out.cache_read + out.cache_write;
    if read_total > 0 {
        out.hit_rate = Some(out.cache_read as f32 / read_total as f32);
    }

    // 省下的钱按**模型**分别算：不同模型的输入价差好几倍，
    // 混在一起乘一个「平均价」算出来的数没有意义。
    let mut saved = 0.0_f64;
    let mut priced_any = false;
    let mut unpriced: Vec<&str> = Vec::new();
    let mut live_seen = false;
    for b in &picked {
        if b.cache_read == 0 {
            continue;
        }
        // ⛔ `prices.resolve` 而不是 `pricing::lookup` —— 见模块头。
        match prices.resolve(&b.model) {
            Some(r) => {
                live_seen |= r.live;
                let full = r.category(pricing::PriceCategory::Input).per_mtok;
                let cached = r.category(pricing::PriceCategory::CacheRead).per_mtok;
                saved += b.cache_read as f64 * (full - cached) / 1_000_000.0;
                priced_any = true;
            }
            None => {
                if !unpriced.contains(&b.model.as_str()) {
                    unpriced.push(&b.model);
                }
            }
        }
    }
    out.unpriced_models = unpriced.len() as i64;
    if priced_any {
        out.saved_usd = Some(saved as f32);
        out.priced_from_snapshot = Some(!live_seen);
    }
    out
}

/// 这一档从哪一天算起（含）。`0` = 不限。
fn since_day(days: i64, today: &str) -> Option<String> {
    if days <= 0 {
        return None;
    }
    let d = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d").ok()?;
    let from = d.checked_sub_days(chrono::Days::new((days - 1) as u64))?;
    Some(from.format("%Y-%m-%d").to_string())
}

/// 读一个槽位并按 `days` 出小结。
pub fn for_slot(slot_dir: &std::path::Path, days: i64, prices: &pricing::Catalog) -> TokenSummary {
    for_slot_with_default_dir(slot_dir, None, days, prices)
}

/// 读一个槽位（外加默认目录里归得上它的那部分）并按 `days` 出小结。
///
/// `account_uuid` 是这个槽位的 `oauthAccount.accountUuid`，
/// 归属判定的唯一依据 —— 见 `tokens.rs` 文件头。
pub fn for_slot_with_default_dir(
    slot_dir: &std::path::Path,
    account_uuid: Option<&str>,
    days: i64,
    prices: &pricing::Catalog,
) -> TokenSummary {
    let home = crate::accounts::AccountRoots::current().default_config_dir();
    let usage = tokens::for_slot_and_default(slot_dir, home.as_deref(), account_uuid);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    summarize_with(&usage.buckets, &usage.unattributed, days, &today, prices)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 只有内置快照、没有抓回来的价。
    fn snapshot_only() -> pricing::Catalog {
        pricing::Catalog::new(Vec::new())
    }

    fn bucket(day: &str, model: &str, input: i64, read: i64, write: i64, out: i64) -> TokenBucket {
        TokenBucket {
            day: day.into(),
            model: model.into(),
            input,
            output: out,
            cache_write: write,
            cache_read: read,
            messages: 1,
        }
    }

    #[test]
    fn one_day_means_today_only() {
        let b = vec![
            bucket("2026-09-16", "claude-opus-5", 10, 0, 0, 1),
            bucket("2026-09-15", "claude-opus-5", 99, 0, 0, 1),
        ];
        assert_eq!(summarize(&b, 1, "2026-09-16", &snapshot_only()).input, 10);
    }

    /// 七天是**含今天**的七天，不是「今天往前数七整天」。
    #[test]
    fn seven_days_includes_today_and_six_before() {
        let b = vec![
            bucket("2026-09-16", "m", 1, 0, 0, 0),
            bucket("2026-09-10", "m", 1, 0, 0, 0),
            bucket("2026-09-09", "m", 1, 0, 0, 0),
        ];
        assert_eq!(
            summarize(&b, 7, "2026-09-16", &snapshot_only()).input,
            2,
            "9-10 在内、9-09 在外"
        );
        assert_eq!(
            summarize(&b, 0, "2026-09-16", &snapshot_only()).input,
            3,
            "0 = 全部"
        );
    }

    /// 命中率的分母里**没有输出** —— 输出是生成的，没有「命中」一说。
    #[test]
    fn output_never_enters_the_hit_rate() {
        let b = vec![bucket("2026-09-16", "m", 100, 300, 0, 9_999)];
        let s = summarize(&b, 1, "2026-09-16", &snapshot_only());
        assert_eq!(s.hit_rate, Some(0.75));
    }

    /// 一个字都还没读过时是「不知道」，不是 0 —— 0 会被读成「一次都没命中」。
    #[test]
    fn no_reads_at_all_has_no_hit_rate() {
        let b = vec![bucket("2026-09-16", "m", 0, 0, 0, 500)];
        assert_eq!(
            summarize(&b, 1, "2026-09-16", &snapshot_only()).hit_rate,
            None
        );
        assert_eq!(
            summarize(&[], 1, "2026-09-16", &snapshot_only()).hit_rate,
            None
        );
    }

    /// 省下的钱 = 缓存读的量 ×（输入价 − 缓存读价）。
    #[test]
    fn saving_is_the_gap_between_full_price_and_cache_price() {
        let model = pricing::TABLE[0].0;
        let p = pricing::TABLE[0].1.as_resolved();
        let full = p.category(pricing::PriceCategory::Input).per_mtok;
        let cached = p.category(pricing::PriceCategory::CacheRead).per_mtok;

        let b = vec![bucket("2026-09-16", model, 0, 1_000_000, 0, 0)];
        let got = summarize(&b, 1, "2026-09-16", &snapshot_only())
            .saved_usd
            .expect("认得出的模型要算得出");
        assert!(
            (got as f64 - (full - cached)).abs() < 0.01,
            "一百万个缓存读 token 省下的应该正好是两档单价之差：{got} vs {}",
            full - cached
        );
    }

    /// ⛔ 抓回来的官方价要赢过内置快照。
    ///
    /// 内置表是 2026-06-24 核对的一份**编译进去的**快照；面板每次启动都会去
    /// 抓官方定价页并存进库（实测 50 条）。0.20.0 的这条路走的是
    /// `pricing::lookup`，只查内置那份，抓回来的那 50 条一条都用不上 ——
    /// 官方调一次价，这里的美元数就静默地按旧价算下去，而界面还写着
    /// 「按本次抓到的官方价算」。
    #[test]
    fn fetched_prices_beat_the_builtin_snapshot() {
        let model = "claude-opus-5";
        let b = vec![bucket("2026-09-16", model, 0, 1_000_000, 0, 0)];

        // 内置快照：输入 $5，缓存读按标准倍数推出来的 $0.5 —— 差 $4.5。
        let snap = summarize(&b, 1, "2026-09-16", &snapshot_only());
        assert_eq!(snap.saved_usd, Some(4.5));
        assert_eq!(
            snap.priced_from_snapshot,
            Some(true),
            "没有抓回来的价时要如实说用的是快照"
        );

        // 抓回来的价把它顶掉：输入 $9、缓存读 $1 —— 差 $8。
        let live = pricing::Catalog::new(vec![pricing::FetchedPrice {
            model: model.into(),
            input_per_mtok: 9.0,
            output_per_mtok: 25.0,
            cache_read_per_mtok: Some(1.0),
            cache_write_per_mtok: Some(11.25),
            long_context: None,
            currency: "USD".into(),
            source_url: "https://example.invalid".into(),
            fetched_at: "2026-09-16".into(),
        }]);
        let got = summarize(&b, 1, "2026-09-16", &live);
        assert_eq!(
            got.saved_usd,
            Some(8.0),
            "官方改了价，这个数就得跟着改 —— 这正是走 Catalog 而不是 lookup 的理由"
        );
        assert_eq!(got.priced_from_snapshot, Some(false));
    }

    /// 一个价都没取到时，「用的是快照还是实时价」根本不成立。
    #[test]
    fn no_price_at_all_reports_neither_source() {
        let b = vec![bucket("2026-09-16", "没人认得的模型", 0, 1_000, 0, 0)];
        assert_eq!(
            summarize(&b, 1, "2026-09-16", &snapshot_only()).priced_from_snapshot,
            None
        );
    }

    /// ⛔ 认不出的模型**跳过**，不拿别的模型的价去凑 ——
    /// 凑出来的数字看起来跟真的一模一样。
    #[test]
    fn an_unknown_model_is_skipped_and_counted_not_guessed() {
        let b = vec![bucket("2026-09-16", "某个没见过的模型", 0, 1_000_000, 0, 0)];
        let s = summarize(&b, 1, "2026-09-16", &snapshot_only());
        assert_eq!(s.saved_usd, None, "一个都算不出来时不给数字");
        assert_eq!(s.unpriced_models, 1, "算不出来的要报出来");
    }

    /// 没有缓存读就没有「省下」这回事。
    #[test]
    fn nothing_cached_means_nothing_saved() {
        let b = vec![bucket("2026-09-16", pricing::TABLE[0].0, 500, 0, 0, 500)];
        assert_eq!(
            summarize(&b, 1, "2026-09-16", &snapshot_only()).saved_usd,
            None
        );
    }
}
