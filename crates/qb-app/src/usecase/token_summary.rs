//! 账户用量小结：今天 / 最近 N 天用了多少 token、按官方 API 价折算多少美元、缓存命中多少。
//!
//! 跨域编排，所以住在这一层 —— token 从 `qb-accounts` 数出来，
//! 官方单价在 `qb-station`，两个是同层 crate，谁也不能依赖谁
//! （`crates_only_depend_downwards` 卡着）。
//!
//! # 美元：按官方 API 价折算（2026-09-24 使用者定的）
//!
//! 0.25.1 之前这里**只**给「缓存省下多少」，理由是订阅账户按 API 单价折出来的总额
//! 不是你实际付的钱（你付的是月费）。使用者 09-24 要的是「每天、7 天、30 天花了多少刀的额度」
//! —— 跟 cc-switch / ccusage 同一个口径：**同样的 token 走 API 要付多少**。所以现在给
//! `cost_usd`，界面**必须**同时写明「按官方 API 价折算，订阅不按这个收费」。算式：
//!
//! ```text
//! 输入×输入价 + 输出×输出价 + 5 分钟档缓存写×5m 写价 + 1 小时档缓存写×1h 写价 + 缓存读×读价
//! ```
//!
//! 缓存写分两档（`tokens.rs` 文件头）：Claude Code 的缓存写实测**全是 1 小时档**，
//! 按 5 分钟档的价算，这一块少 37.5%。两档都从 [`pricing::ResolvedPrice`] 拿：
//! 官方表里标了就用标的，没标按官方倍数推（1h 写 = 输入 ×2），推的在「按什么价算」里标出来。
//!
//! 「缓存省下」（`saved_usd` = 缓存读 ×（输入价 − 缓存读价））照旧算，降到说明里。
//!
//! # ⛔ 取价要走 `Catalog`，不是 `pricing::lookup`
//!
//! `pricing::lookup` 只查**编译进去的那张快照表**，实机转写里最常用的几个模型
//! 往往不在里面 —— 于是一个价都查不到，界面上那一格**永远是个破折号**。
//! 面板启动时抓回来的价存在库里（`metadata.station_prices`），[`pricing::Catalog`]
//! 先查抓回来的，再退回内置快照。装配在命令层（`src-tauri`）—— 那里才够得着 `Repository`。
//!
//! # ⛔ 认不出的模型跳过并报数，不凑
//!
//! 认不出价的那部分不进美元，报在 `unpriced`（模型名 + 回复数 + token 数）——
//! 美元数旁边必须看得到「有多少没算进来」。拿别的模型的价去凑，凑出来的数字
//! 看起来跟真的一模一样。非美元计价的价也当认不出（不做汇率换算）。
//!
//! # 「今天 / 7 天 / 30 天」三档一次给齐
//!
//! [`SpendWindows`] **不跟着 `days` 走**：使用者要的是同时看见三个数，
//! 让界面为此发三次请求（每次都整遍扫描转写）不值得 —— 桶已经在手里了。

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use ts_rs::TS;

use crate::accounts::tokens::{self, TokenBucket, TokenUsage};
use qb_station::station::pricing;

/// 「最近请求」给界面的最多条数。
pub const RECENT_VIEW_CAP: usize = 200;

/// 一天的合计。用量明细页的趋势图与按天明细拿它画。
///
/// ⛔ **没有记录的那一天不出现在这个列表里。** 补一个 0 进来看上去无害，
/// 但那是在断言「这天没用」—— 而真相可能是那天的记录读不出来。
/// 画图那一侧自己决定怎么处理缺口（柱状图不画那一根，提示里写「没有记录」）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TokenDay {
    /// 本地 `YYYY-MM-DD`。
    pub day: String,
    #[ts(type = "number")]
    pub input: i64,
    #[ts(type = "number")]
    pub output: i64,
    #[ts(type = "number")]
    pub cache_read: i64,
    /// 缓存写合计（两档）。
    #[ts(type = "number")]
    pub cache_write: i64,
    /// 其中 1 小时档。
    #[ts(type = "number")]
    pub cache_write_1h: i64,
    #[ts(type = "number")]
    pub messages: i64,
    /// 这一天按官方 API 价折算多少美元（认得出价的那部分）。一个价都认不出是 `None`。
    pub cost_usd: Option<f64>,
}

/// 今天某一个小时的合计。「今天」那一档按小时画柱子用。只列有记录的小时。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TokenHour {
    /// 本地小时，0–23。
    pub hour: u8,
    #[ts(type = "number")]
    pub input: i64,
    #[ts(type = "number")]
    pub output: i64,
    #[ts(type = "number")]
    pub cache_read: i64,
    #[ts(type = "number")]
    pub cache_write: i64,
    #[ts(type = "number")]
    pub cache_write_1h: i64,
    #[ts(type = "number")]
    pub messages: i64,
    pub cost_usd: Option<f64>,
}

/// 一个模型在这一档时间范围里的合计。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TokenModel {
    pub model: String,
    #[ts(type = "number")]
    pub messages: i64,
    #[ts(type = "number")]
    pub input: i64,
    #[ts(type = "number")]
    pub output: i64,
    #[ts(type = "number")]
    pub cache_read: i64,
    #[ts(type = "number")]
    pub cache_write: i64,
    #[ts(type = "number")]
    pub cache_write_1h: i64,
    /// `None` = 这个模型没有官方价，没算进美元。
    pub cost_usd: Option<f64>,
}

/// 美元按四类拆开（缓存写含两档）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CostParts {
    pub input: f64,
    pub output: f64,
    pub cache_write: f64,
    pub cache_read: f64,
}

impl CostParts {
    pub fn total(&self) -> f64 {
        self.input + self.output + self.cache_write + self.cache_read
    }

    fn add(&mut self, o: CostParts) {
        self.input += o.input;
        self.output += o.output;
        self.cache_write += o.cache_write;
        self.cache_read += o.cache_read;
    }
}

/// 一个时间窗口花了多少。
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SpendWindow {
    /// 认得出价的那部分折算多少美元。窗口里一个价都认不出（或者根本没记录）是 `None`。
    pub usd: Option<f64>,
    /// 窗口里有几条回复。`0` = 没有记录 —— 界面要说「没有记录」，不说「$0」。
    #[ts(type = "number")]
    pub messages: i64,
    /// 其中几条的模型没有官方价，没算进 `usd`。
    #[ts(type = "number")]
    pub unpriced_messages: i64,
}

/// 今天 / 含今天的 7 天 / 含今天的 30 天。**不随 `days` 变**，见模块头。
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SpendWindows {
    pub today: SpendWindow,
    pub last_7d: SpendWindow,
    pub last_30d: SpendWindow,
}

/// 「最近请求」的一行，带这一条折算的美元。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TokenRequestView {
    pub day: String,
    pub time: String,
    pub model: String,
    #[ts(type = "number")]
    pub input: i64,
    #[ts(type = "number")]
    pub output: i64,
    #[ts(type = "number")]
    pub cache_write: i64,
    #[ts(type = "number")]
    pub cache_write_1h: i64,
    #[ts(type = "number")]
    pub cache_read: i64,
    pub project: String,
    pub session: String,
    pub cost_usd: Option<f64>,
}

/// 一个认不出价的模型，以及它在这一档里有多少量。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UnpricedModel {
    pub model: String,
    #[ts(type = "number")]
    pub messages: i64,
    /// 四类 token 合计。
    #[ts(type = "number")]
    pub tokens: i64,
}

/// 「按什么价算」那张小表的一行。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PriceUsed {
    pub model: String,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    /// 这三项是不是按官方倍数推的（官方表里没单独标）。
    pub cache_read_derived: bool,
    pub cache_write_5m_derived: bool,
    pub cache_write_1h_derived: bool,
    /// 这次启动抓回来的（`true`）还是内置快照（`false`）。
    pub live: bool,
    /// 核对 / 抓取那天，`YYYY-MM-DD`。
    pub verified_at: String,
}

/// 这份统计覆盖了多少。只有 Claude 的转写路径给得出来。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Coverage {
    #[ts(type = "number")]
    pub sessions: i64,
    #[ts(type = "number")]
    pub files_read: i64,
    #[ts(type = "number")]
    pub files_failed: i64,
    #[ts(type = "number")]
    pub duplicates: i64,
    #[ts(type = "number")]
    pub undated: i64,
}

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
    /// 缓存写合计（两档）。
    #[ts(type = "number")]
    pub cache_write: i64,
    /// 其中 1 小时档。
    #[ts(type = "number")]
    pub cache_write_1h: i64,
    #[ts(type = "number")]
    pub cache_read: i64,
    /// 回复数。四类全 0 的报错不算（见 `empty_replies`）。
    #[ts(type = "number")]
    pub messages: i64,
    /// 缓存命中率 0–1：`缓存读 ÷（输入 + 缓存读 + 缓存写）`。
    ///
    /// **输出不进分母** —— 输出是生成出来的，从来就没有「命中」一说，
    /// 把它算进去只会让这个比例随着回答长短乱飘。
    /// 分母为 0 时是 `None`（还没读过任何上文），不是 0。
    pub hit_rate: Option<f32>,
    /// 缓存省下多少（美元）：缓存读 ×（输入价 − 缓存读价）。算不出来就是 `None`。
    pub saved_usd: Option<f32>,
    /// 有几个模型的官方价没认出来，那部分没算进美元。明细在 `unpriced`。
    #[ts(type = "number")]
    pub unpriced_models: i64,
    /// 同一档时间范围里，**归不到任何槽位**的用量（默认目录里没有归属标记的
    /// 那部分）。界面必须单独显示，不许并进总数。
    #[ts(type = "number")]
    pub unattributed: i64,
    /// 上面那部分有几条回复。
    #[ts(type = "number")]
    pub unattributed_messages: i64,
    /// 用的是编译进去的价格快照（`true`）还是抓回来的官方价（`false`）。
    /// 界面要分得出来 —— 快照可能已经过期。
    ///
    /// 一个价都没取到时是 `None`：那时候「用的是快照还是实时价」根本不成立。
    pub priced_from_snapshot: Option<bool>,
    /// 按天的合计，旧到新。只列**真有记录的那几天**，见 [`TokenDay`]。
    pub daily: Vec<TokenDay>,
    /// 这一档按官方 API 价折算的美元（认得出价的那部分）。一个价都认不出是 `None`。
    pub cost_usd: Option<f64>,
    /// 上面那个数按四类拆开。
    pub cost_parts: Option<CostParts>,
    /// 今天 / 7 天 / 30 天，不随 `days` 变。
    pub spend: SpendWindows,
    /// 按模型，美元多的在前（没有价的排后面）。
    pub models: Vec<TokenModel>,
    /// `days == 1` 且要明细时才有：今天按小时。
    pub hourly: Vec<TokenHour>,
    /// 要明细时才有：这一档里最近的回复，新的在前，最多 [`RECENT_VIEW_CAP`] 条。
    pub recent: Vec<TokenRequestView>,
    /// 这一档里四类全 0、没入账的回复数（报错、被打断）。
    #[ts(type = "number")]
    pub empty_replies: i64,
    /// 这一档里最后一条回复的本地时刻，`YYYY-MM-DD HH:MM:SS`。
    pub last_at: Option<String>,
    /// 认不出价的模型与它们的量。
    pub unpriced: Vec<UnpricedModel>,
    /// 这一档里用到的价（要明细时才有）。
    pub prices_used: Vec<PriceUsed>,
    /// 覆盖了多少文件。反重力 / GPT 那两条路径给不出来，是 `None`。
    pub coverage: Option<Coverage>,
}

/// 用量明细页那一格「按账户」的一行是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AccountSpendKind {
    /// 面板里的一个槽位。
    Slot,
    /// 默认目录里三级都归不出主的那部分。
    Unattributed,
    /// 归得出主、但那个账户不是面板里任何一个槽位。
    OtherAccounts,
}

/// 「按账户」的一行。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AccountSpend {
    pub kind: AccountSpendKind,
    /// 槽位标识（`kind == Slot` 时）；其余两种是空串。
    pub label: String,
    #[ts(type = "number")]
    pub messages: i64,
    #[ts(type = "number")]
    pub input: i64,
    #[ts(type = "number")]
    pub output: i64,
    #[ts(type = "number")]
    pub cache_write: i64,
    #[ts(type = "number")]
    pub cache_read: i64,
    pub cost_usd: Option<f64>,
    /// 其中几条没有官方价。
    #[ts(type = "number")]
    pub unpriced_messages: i64,
}

/// 用量明细页要的全部：当前槽位的完整小结 + 按账户。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UsageOverview {
    pub summary: TokenSummary,
    pub by_account: Vec<AccountSpend>,
}

// ------------------------------------------------------------------ 算钱

/// 一个桶（或一条回复）的四类 token。
#[derive(Debug, Clone, Copy, Default)]
struct Tok {
    input: i64,
    output: i64,
    cache_write: i64,
    cache_write_1h: i64,
    cache_read: i64,
}

impl Tok {
    fn of(b: &TokenBucket) -> Self {
        Self {
            input: b.input,
            output: b.output,
            cache_write: b.cache_write,
            cache_write_1h: b.cache_write_1h,
            cache_read: b.cache_read,
        }
    }

    fn total(&self) -> i64 {
        self.input + self.output + self.cache_write + self.cache_read
    }
}

/// 按官方 API 价折算，四类分开。见模块头的算式。
fn cost_parts(t: Tok, p: &pricing::ResolvedPrice) -> CostParts {
    let per = |n: i64, price: f64| n as f64 * price / 1_000_000.0;
    let one_hour = t.cache_write_1h.clamp(0, t.cache_write.max(0));
    let five_min = t.cache_write - one_hour;
    CostParts {
        input: per(t.input, p.input.per_mtok),
        output: per(t.output, p.output.per_mtok),
        cache_write: per(five_min, p.cache_write.per_mtok)
            + per(one_hour, p.cache_write_1h.per_mtok),
        cache_read: per(t.cache_read, p.cache_read.per_mtok),
    }
}

/// 一次统计里反复查同一个模型的价 —— 查一次记下来。只认美元价。
struct Pricer<'a> {
    catalog: &'a pricing::Catalog,
    cache: HashMap<String, Option<pricing::ResolvedPrice>>,
}

impl<'a> Pricer<'a> {
    fn new(catalog: &'a pricing::Catalog) -> Self {
        Self {
            catalog,
            cache: HashMap::new(),
        }
    }

    fn get(&mut self, model: &str) -> Option<&pricing::ResolvedPrice> {
        let catalog = self.catalog;
        self.cache
            .entry(model.to_string())
            // ⛔ `resolve` 而不是 `pricing::lookup` —— 见模块头。
            .or_insert_with(|| {
                catalog
                    .resolve(model)
                    .filter(|p| p.currency.eq_ignore_ascii_case("USD"))
            })
            .as_ref()
    }

    fn cost(&mut self, model: &str, t: Tok) -> Option<CostParts> {
        self.get(model).map(|p| cost_parts(t, p))
    }
}

fn add_opt(slot: &mut Option<f64>, v: f64) {
    *slot = Some(slot.unwrap_or(0.0) + v);
}

// ------------------------------------------------------------------ 小结

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
    let in_range = |day: &str| since.as_deref().is_none_or(|s| day >= s);
    let picked: Vec<&TokenBucket> = buckets.iter().filter(|b| in_range(&b.day)).collect();
    let mut pricer = Pricer::new(prices);

    let mut out = TokenSummary {
        days,
        input: 0,
        output: 0,
        cache_write: 0,
        cache_write_1h: 0,
        cache_read: 0,
        messages: 0,
        hit_rate: None,
        saved_usd: None,
        unattributed: 0,
        unattributed_messages: 0,
        unpriced_models: 0,
        priced_from_snapshot: None,
        daily: Vec::new(),
        cost_usd: None,
        cost_parts: None,
        spend: SpendWindows {
            today: window(buckets, Some(today.to_string()), &mut pricer),
            last_7d: window(buckets, since_day(7, today), &mut pricer),
            last_30d: window(buckets, since_day(30, today), &mut pricer),
        },
        models: Vec::new(),
        hourly: Vec::new(),
        recent: Vec::new(),
        empty_replies: 0,
        last_at: None,
        unpriced: Vec::new(),
        prices_used: Vec::new(),
        coverage: None,
    };
    for b in unattributed.iter().filter(|b| in_range(&b.day)) {
        out.unattributed += b.input + b.output + b.cache_write + b.cache_read;
        out.unattributed_messages += b.messages;
    }

    // 按天、按模型合并。`BTreeMap` 自带按日期字符串排序，而 `YYYY-MM-DD`
    // 的字典序就是时间序 —— 跟 `since_day` 那边用字符串比大小同一个理由。
    let mut by_day: BTreeMap<&str, TokenDay> = BTreeMap::new();
    let mut by_model: BTreeMap<&str, TokenModel> = BTreeMap::new();
    let mut unpriced: BTreeMap<&str, UnpricedModel> = BTreeMap::new();
    let mut parts = CostParts::default();
    let mut priced_any = false;
    let mut live_seen = false;
    let mut saved = 0.0_f64;
    let mut saved_any = false;

    for b in &picked {
        out.input += b.input;
        out.output += b.output;
        out.cache_write += b.cache_write;
        out.cache_write_1h += b.cache_write_1h;
        out.cache_read += b.cache_read;
        out.messages += b.messages;

        let t = Tok::of(b);
        let price = pricer.get(&b.model).cloned();
        let cost = price.as_ref().map(|p| cost_parts(t, p));

        let d = by_day.entry(b.day.as_str()).or_insert_with(|| TokenDay {
            day: b.day.clone(),
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            cache_write_1h: 0,
            messages: 0,
            cost_usd: None,
        });
        d.input += b.input;
        d.output += b.output;
        d.cache_read += b.cache_read;
        d.cache_write += b.cache_write;
        d.cache_write_1h += b.cache_write_1h;
        d.messages += b.messages;

        let m = by_model
            .entry(b.model.as_str())
            .or_insert_with(|| TokenModel {
                model: b.model.clone(),
                messages: 0,
                input: 0,
                output: 0,
                cache_read: 0,
                cache_write: 0,
                cache_write_1h: 0,
                cost_usd: None,
            });
        m.messages += b.messages;
        m.input += b.input;
        m.output += b.output;
        m.cache_read += b.cache_read;
        m.cache_write += b.cache_write;
        m.cache_write_1h += b.cache_write_1h;

        match (price, cost) {
            (Some(p), Some(c)) => {
                add_opt(&mut d.cost_usd, c.total());
                add_opt(&mut m.cost_usd, c.total());
                parts.add(c);
                priced_any = true;
                live_seen |= p.live;
                // 省下的钱按**模型**分别算：不同模型的输入价差好几倍，
                // 混在一起乘一个「平均价」算出来的数没有意义。
                if b.cache_read > 0 {
                    saved += b.cache_read as f64 * (p.input.per_mtok - p.cache_read.per_mtok)
                        / 1_000_000.0;
                    saved_any = true;
                }
            }
            _ => {
                let u = unpriced
                    .entry(b.model.as_str())
                    .or_insert_with(|| UnpricedModel {
                        model: b.model.clone(),
                        messages: 0,
                        tokens: 0,
                    });
                u.messages += b.messages;
                u.tokens += t.total();
            }
        }
    }

    out.daily = by_day.into_values().collect();

    let mut models: Vec<TokenModel> = by_model.into_values().collect();
    models.sort_by(|a, b| {
        b.cost_usd
            .unwrap_or(-1.0)
            .total_cmp(&a.cost_usd.unwrap_or(-1.0))
            .then_with(|| {
                let ta = a.input + a.output + a.cache_read + a.cache_write;
                let tb = b.input + b.output + b.cache_read + b.cache_write;
                tb.cmp(&ta)
            })
    });
    out.prices_used = models
        .iter()
        .filter_map(|m| price_row(&m.model, pricer.get(&m.model)?))
        .collect();
    out.models = models;

    let read_total = out.input + out.cache_read + out.cache_write;
    if read_total > 0 {
        out.hit_rate = Some(out.cache_read as f32 / read_total as f32);
    }

    out.unpriced = unpriced.into_values().collect();
    out.unpriced_models = out.unpriced.len() as i64;
    if priced_any {
        out.cost_usd = Some(parts.total());
        out.cost_parts = Some(parts);
        out.priced_from_snapshot = Some(!live_seen);
    }
    if saved_any {
        out.saved_usd = Some(saved as f32);
    }
    out
}

/// 一个窗口（从 `since` 那天起，`None` = 不限）里花了多少。
fn window(buckets: &[TokenBucket], since: Option<String>, pricer: &mut Pricer) -> SpendWindow {
    let mut w = SpendWindow::default();
    for b in buckets
        .iter()
        .filter(|b| since.as_deref().is_none_or(|s| b.day.as_str() >= s))
    {
        w.messages += b.messages;
        match pricer.cost(&b.model, Tok::of(b)) {
            Some(c) => add_opt(&mut w.usd, c.total()),
            None => w.unpriced_messages += b.messages,
        }
    }
    w
}

fn price_row(model: &str, p: &pricing::ResolvedPrice) -> Option<PriceUsed> {
    use pricing::PriceBasis::Derived;
    Some(PriceUsed {
        model: model.to_string(),
        input: p.input.per_mtok,
        output: p.output.per_mtok,
        cache_read: p.cache_read.per_mtok,
        cache_write_5m: p.cache_write.per_mtok,
        cache_write_1h: p.cache_write_1h.per_mtok,
        cache_read_derived: p.cache_read.basis == Derived,
        cache_write_5m_derived: p.cache_write.basis == Derived,
        cache_write_1h_derived: p.cache_write_1h.basis == Derived,
        live: p.live,
        verified_at: p.verified_at.clone(),
    })
}

/// 在 [`summarize_with`] 之上补齐只有 Claude 转写路径才有的那几样：
/// 报错条数、覆盖率、最后一条的时刻；`detail` 时再加最近请求、按小时、价目表。
///
/// 账户卡每分钟重读一次，不要明细（`detail: false`）—— 那几样只有用量明细页用得上。
pub fn summarize_usage(
    usage: &TokenUsage,
    days: i64,
    today: &str,
    prices: &pricing::Catalog,
    detail: bool,
) -> TokenSummary {
    let mut s = summarize_with(&usage.buckets, &usage.unattributed, days, today, prices);
    let since = since_day(days, today);
    let in_range = |day: &str| since.as_deref().is_none_or(|x| day >= x);

    s.empty_replies = usage
        .empty_days
        .iter()
        .filter(|d| in_range(&d.day))
        .map(|d| d.replies)
        .sum();
    s.coverage = Some(Coverage {
        sessions: usage.sessions,
        files_read: usage.files_read,
        files_failed: usage.files_failed,
        duplicates: usage.duplicates,
        undated: usage.undated,
    });
    let recent: Vec<&tokens::TokenRequest> =
        usage.recent.iter().filter(|r| in_range(&r.day)).collect();
    s.last_at = recent.first().map(|r| format!("{} {}", r.day, r.time));

    if !detail {
        s.prices_used.clear();
        return s;
    }

    let mut pricer = Pricer::new(prices);
    s.recent = recent
        .into_iter()
        .take(RECENT_VIEW_CAP)
        .map(|r| TokenRequestView {
            day: r.day.clone(),
            time: r.time.clone(),
            model: r.model.clone(),
            input: r.input,
            output: r.output,
            cache_write: r.cache_write,
            cache_write_1h: r.cache_write_1h,
            cache_read: r.cache_read,
            project: r.project.clone(),
            session: r.session.clone(),
            cost_usd: pricer
                .cost(
                    &r.model,
                    Tok {
                        input: r.input,
                        output: r.output,
                        cache_write: r.cache_write,
                        cache_write_1h: r.cache_write_1h,
                        cache_read: r.cache_read,
                    },
                )
                .map(|c| c.total()),
        })
        .collect();

    if days == 1 {
        let mut by_hour: BTreeMap<u8, TokenHour> = BTreeMap::new();
        for b in usage.hourly.iter().filter(|b| b.day == today) {
            let h = by_hour.entry(b.hour).or_insert_with(|| TokenHour {
                hour: b.hour,
                input: 0,
                output: 0,
                cache_read: 0,
                cache_write: 0,
                cache_write_1h: 0,
                messages: 0,
                cost_usd: None,
            });
            h.input += b.input;
            h.output += b.output;
            h.cache_read += b.cache_read;
            h.cache_write += b.cache_write;
            h.cache_write_1h += b.cache_write_1h;
            h.messages += b.messages;
            let t = Tok {
                input: b.input,
                output: b.output,
                cache_write: b.cache_write,
                cache_write_1h: b.cache_write_1h,
                cache_read: b.cache_read,
            };
            if let Some(c) = pricer.cost(&b.model, t) {
                add_opt(&mut h.cost_usd, c.total());
            }
        }
        s.hourly = by_hour.into_values().collect();
    }
    s
}

/// 这一档从哪一天算起（含）。`0` = 不限。
pub fn since_day(days: i64, today: &str) -> Option<String> {
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

/// 读一个槽位（外加默认目录里归得上它的那部分）并按 `days` 出小结。账户卡用，不带明细。
///
/// `account_uuid` 是这个槽位的 `oauthAccount.accountUuid`；默认目录里的转写
/// 靠它对上归属索引 —— 三级判定见 `tokens.rs` 文件头。索引每次现建：
/// 桌面端资料目录里几十个 15 KB 的小 JSON，跟扫几十 MB 转写比可以忽略，
/// 而且省掉一份要自己维护失效的缓存。
pub fn for_slot_with_default_dir(
    slot_dir: &std::path::Path,
    account_uuid: Option<&str>,
    days: i64,
    prices: &pricing::Catalog,
) -> TokenSummary {
    let roots = crate::accounts::AccountRoots::current();
    let home = roots.default_config_dir();
    let index = tokens::OwnerIndex::scan(&roots.desktop_profile_dirs());
    let usage = tokens::for_slot_and_default(slot_dir, home.as_deref(), account_uuid, &index);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    summarize_usage(&usage, days, &today, prices, false)
}

/// 用量明细页：一次扫描，当前槽位要完整明细，每个槽位再给一行「按账户」。
pub fn overview(label: &str, days: i64, prices: &pricing::Catalog) -> UsageOverview {
    let roots = crate::accounts::AccountRoots::current();
    let home = roots.default_config_dir();
    let index = tokens::OwnerIndex::scan(&roots.desktop_profile_dirs());
    let mut slots: Vec<tokens::SlotRef> = crate::accounts::slots_in(&roots)
        .into_iter()
        .map(|s| tokens::SlotRef {
            dir: roots.slot_dir(&s.label),
            label: s.label,
            account_uuid: s.account_uuid,
        })
        .collect();
    // 查的这个槽位万一不在列表里（刚删掉、目录读不开）：单独补上，别让整页空掉。
    if !label.is_empty() && !slots.iter().any(|s| s.label == label) {
        let dir = roots.slot_dir(label);
        slots.push(tokens::SlotRef {
            account_uuid: crate::accounts::account_uuid_of(&dir),
            dir,
            label: label.to_string(),
        });
    }
    let all = tokens::scan_all_slots(&slots, home.as_deref(), &index);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    overview_from(&all, label, days, &today, prices)
}

/// `overview` 的纯函数部分。
pub fn overview_from(
    all: &tokens::AllSlotsUsage,
    label: &str,
    days: i64,
    today: &str,
    prices: &pricing::Catalog,
) -> UsageOverview {
    let mut summary = None;
    let mut by_account = Vec::new();
    for (l, usage) in &all.slots {
        let s = summarize_usage(usage, days, today, prices, l == label);
        by_account.push(spend_row(AccountSpendKind::Slot, l, &s));
        if l == label {
            summary = Some(s);
        }
    }
    if let Some((_, first)) = all.slots.first() {
        let s = summarize(&first.unattributed, days, today, prices);
        if s.messages > 0 {
            by_account.push(spend_row(AccountSpendKind::Unattributed, "", &s));
        }
    }
    let other = summarize(&all.other_accounts, days, today, prices);
    if other.messages > 0 {
        by_account.push(spend_row(AccountSpendKind::OtherAccounts, "", &other));
    }
    UsageOverview {
        summary: summary.unwrap_or_else(|| summarize(&[], days, today, prices)),
        by_account,
    }
}

/// GPT（Codex）一侧的用量明细：同一份 [`TokenSummary`]，外加 Codex 自己的覆盖证据。
///
/// `summary.messages` 在这一侧是**请求数**（每条 `token_count` 的正差分算一次），
/// 不是回复数；缓存写恒为 0（OpenAI 的记录不分缓存写）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CodexUsageSummary {
    pub summary: TokenSummary,
    /// 这一档里单次输入超过 272K（长上下文那一档）的请求数。
    /// 非 0 时美元是**下限**：那几次按短上下文价算的，长上下文输入价更高。
    #[ts(type = "number")]
    pub long_context_requests: i64,
    #[ts(type = "number")]
    pub sessions: i64,
    #[ts(type = "number")]
    pub files_read: i64,
    #[ts(type = "number")]
    pub files_failed: i64,
    #[ts(type = "number")]
    pub duplicates: i64,
    #[ts(type = "number")]
    pub incomplete: i64,
    pub checked_at: String,
}

/// 把一份（不设截止、从头读的）Codex 用量折成用量明细页要的形状。
///
/// 要从头读：「今天 / 7 天 / 30 天」三档（[`SpendWindows`]）需要 30 天的桶，
/// 跟选中的是哪一档无关。
pub fn codex_summary(
    usage: &qb_accounts::codex::usage::CodexUsage,
    days: i64,
    today: &str,
    prices: &pricing::Catalog,
) -> CodexUsageSummary {
    let since = since_day(days, today);
    CodexUsageSummary {
        summary: summarize(&usage.buckets, days, today, prices),
        long_context_requests: usage
            .long_context
            .iter()
            .filter(|d| since.as_deref().is_none_or(|s| d.day.as_str() >= s))
            .map(|d| i64::from(d.requests))
            .sum(),
        sessions: i64::from(usage.sessions),
        files_read: i64::from(usage.files_read),
        files_failed: i64::from(usage.files_failed),
        duplicates: i64::from(usage.duplicates),
        incomplete: i64::from(usage.incomplete),
        checked_at: usage.checked_at.clone(),
    }
}

fn spend_row(kind: AccountSpendKind, label: &str, s: &TokenSummary) -> AccountSpend {
    AccountSpend {
        kind,
        label: label.to_string(),
        messages: s.messages,
        input: s.input,
        output: s.output,
        cache_write: s.cache_write,
        cache_read: s.cache_read,
        cost_usd: s.cost_usd,
        unpriced_messages: s.unpriced.iter().map(|u| u.messages).sum(),
    }
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
            cache_write_1h: 0,
            cache_read: read,
            messages: 1,
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
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
    /// 抓官方定价页并存进库。只查内置那份的话，官方调一次价，这里的美元数就
    /// 静默地按旧价算下去，而界面还写着「按本次抓到的官方价算」。
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
        let live = pricing::Catalog::new(vec![fetched(model, 9.0, 25.0, 1.0, 11.25, None)]);
        let got = summarize(&b, 1, "2026-09-16", &live);
        assert_eq!(
            got.saved_usd,
            Some(8.0),
            "官方改了价，这个数就得跟着改 —— 这正是走 Catalog 而不是 lookup 的理由"
        );
        assert_eq!(got.priced_from_snapshot, Some(false));
    }

    fn fetched(
        model: &str,
        input: f64,
        output: f64,
        read: f64,
        write_5m: f64,
        write_1h: Option<f64>,
    ) -> pricing::FetchedPrice {
        pricing::FetchedPrice {
            model: model.into(),
            input_per_mtok: input,
            output_per_mtok: output,
            cache_read_per_mtok: Some(read),
            cache_write_per_mtok: Some(write_5m),
            cache_write_1h_per_mtok: write_1h,
            long_context: None,
            currency: "USD".into(),
            source_url: "https://example.invalid".into(),
            fetched_at: "2026-09-24".into(),
        }
    }

    /// 一个价都没取到时，「用的是快照还是实时价」根本不成立。
    #[test]
    fn no_price_at_all_reports_neither_source() {
        let b = vec![bucket("2026-09-16", "没人认得的模型", 0, 1_000, 0, 0)];
        let s = summarize(&b, 1, "2026-09-16", &snapshot_only());
        assert_eq!(s.priced_from_snapshot, None);
        assert_eq!(s.cost_usd, None, "一个价都认不出时不给美元");
    }

    /// ⛔ 认不出的模型**跳过**，不拿别的模型的价去凑 ——
    /// 凑出来的数字看起来跟真的一模一样。
    #[test]
    fn an_unknown_model_is_skipped_and_counted_not_guessed() {
        let b = vec![bucket("2026-09-16", "某个没见过的模型", 0, 1_000_000, 0, 0)];
        let s = summarize(&b, 1, "2026-09-16", &snapshot_only());
        assert_eq!(s.saved_usd, None, "一个都算不出来时不给数字");
        assert_eq!(s.unpriced_models, 1, "算不出来的要报出来");
        assert_eq!(s.unpriced[0].tokens, 1_000_000);
        assert_eq!(s.unpriced[0].messages, 1);
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

    /// 美元的算式：四类各乘各的价，缓存写分两档。
    /// 内置快照 claude-opus-5：入 $5 / 出 $25 / 读 $0.5（推）/ 5m 写 $6.25（推）/ 1h 写 $10（推）。
    #[test]
    fn cost_multiplies_each_category_by_its_own_price() {
        let mut b = bucket(
            "2026-09-24",
            "claude-opus-5",
            1_000_000,
            1_000_000,
            2_000_000,
            1_000_000,
        );
        b.cache_write_1h = 1_000_000;
        let s = summarize(&[b], 1, "2026-09-24", &snapshot_only());
        let parts = s.cost_parts.expect("认得出的模型要算得出");
        assert!(close(parts.input, 5.0));
        assert!(close(parts.output, 25.0));
        assert!(close(parts.cache_read, 0.5));
        assert!(
            close(parts.cache_write, 6.25 + 10.0),
            "一百万 5m 写 + 一百万 1h 写 = 6.25 + 10：{}",
            parts.cache_write
        );
        assert!(close(s.cost_usd.unwrap(), 5.0 + 25.0 + 0.5 + 16.25));
    }

    /// ⛔ 实机那一天的形状：缓存写全是 1 小时档。按 5 分钟档的价算会少 37.5%。
    /// 抓回来的 claude-opus-5-5：入 $4、5m 写 $5、1h 写 $8。
    #[test]
    fn a_one_hour_cache_write_costs_the_one_hour_price_not_the_five_minute_one() {
        let live = pricing::Catalog::new(vec![fetched(
            "claude-opus-5-5",
            4.0,
            20.0,
            0.2,
            5.0,
            Some(8.0),
        )]);
        let mut b = bucket("2026-09-24", "claude-opus-5-5", 0, 0, 1_000_000, 0);
        b.cache_write_1h = 1_000_000;
        let s = summarize(&[b.clone()], 1, "2026-09-24", &live);
        assert!(close(s.cost_usd.unwrap(), 8.0), "{:?}", s.cost_usd);
        // 同样的量全是 5 分钟档：$5。
        b.cache_write_1h = 0;
        let s5 = summarize(&[b], 1, "2026-09-24", &live);
        assert!(close(s5.cost_usd.unwrap(), 5.0));
        // 库里的旧行没有 1h 价：按输入 ×2 推，还是 $8，并标成推的。
        let old =
            pricing::Catalog::new(vec![fetched("claude-opus-5-5", 4.0, 20.0, 0.2, 5.0, None)]);
        let mut c = bucket("2026-09-24", "claude-opus-5-5", 0, 0, 1_000_000, 0);
        c.cache_write_1h = 1_000_000;
        let so = summarize(&[c], 1, "2026-09-24", &old);
        assert!(close(so.cost_usd.unwrap(), 8.0));
        assert!(so.prices_used[0].cache_write_1h_derived);
    }

    /// 认得出的算进美元，认不出的单独报 —— 不许把认不出的那部分说成 $0。
    #[test]
    fn mixed_priced_and_unpriced_models_are_reported_apart() {
        let b = vec![
            bucket("2026-09-24", "claude-opus-5", 1_000_000, 0, 0, 0),
            bucket("2026-09-24", "谁也不认识", 1_000_000, 0, 0, 0),
        ];
        let s = summarize(&b, 1, "2026-09-24", &snapshot_only());
        assert!(close(s.cost_usd.unwrap(), 5.0));
        assert_eq!(s.unpriced_models, 1);
        assert_eq!(s.models[0].model, "claude-opus-5", "有价的排前面");
        assert_eq!(s.models[1].cost_usd, None);
        assert_eq!(s.spend.today.unpriced_messages, 1);
        assert_eq!(s.spend.today.messages, 2);
    }

    /// 「今天 / 7 天 / 30 天」三档一次给齐，跟选的是哪一档无关。
    #[test]
    fn spend_windows_do_not_follow_the_selected_range() {
        let b = vec![
            bucket("2026-09-24", "claude-opus-5", 1_000_000, 0, 0, 0), // $5
            bucket("2026-09-19", "claude-opus-5", 2_000_000, 0, 0, 0), // $10
            bucket("2026-09-01", "claude-opus-5", 4_000_000, 0, 0, 0), // $20
            bucket("2026-08-01", "claude-opus-5", 8_000_000, 0, 0, 0), // $40
        ];
        for days in [1, 7, 30, 0] {
            let s = summarize(&b, days, "2026-09-24", &snapshot_only());
            assert!(close(s.spend.today.usd.unwrap(), 5.0), "days={days}");
            assert!(close(s.spend.last_7d.usd.unwrap(), 15.0), "days={days}");
            assert!(close(s.spend.last_30d.usd.unwrap(), 35.0), "days={days}");
        }
        let s = summarize(&b, 7, "2026-09-24", &snapshot_only());
        assert!(
            close(s.cost_usd.unwrap(), 15.0),
            "选中那一档的合计跟着 days 走"
        );
        // 没有记录的窗口：没有美元、回复数是 0 —— 界面据此说「没有记录」而不是「$0」。
        let empty = summarize(&b, 1, "2026-10-30", &snapshot_only());
        assert_eq!(empty.spend.today.usd, None);
        assert_eq!(empty.spend.today.messages, 0);
    }

    /// 每一天、每一个模型都带自己那份美元。
    #[test]
    fn daily_and_model_rows_carry_their_own_cost() {
        let b = vec![
            bucket("2026-09-23", "claude-opus-5", 1_000_000, 0, 0, 0),
            bucket("2026-09-24", "claude-opus-5", 2_000_000, 0, 0, 0),
            bucket("2026-09-24", "claude-sonnet-5", 1_000_000, 0, 0, 0),
        ];
        let s = summarize(&b, 7, "2026-09-24", &snapshot_only());
        assert_eq!(s.daily.len(), 2);
        assert!(close(s.daily[0].cost_usd.unwrap(), 5.0));
        assert!(close(s.daily[1].cost_usd.unwrap(), 10.0 + 2.0));
        let opus = s
            .models
            .iter()
            .find(|m| m.model == "claude-opus-5")
            .unwrap();
        assert!(close(opus.cost_usd.unwrap(), 15.0));
        assert_eq!(s.prices_used.len(), 2);
    }

    fn request(day: &str, time: &str, input: i64) -> tokens::TokenRequest {
        tokens::TokenRequest {
            day: day.into(),
            time: time.into(),
            model: "claude-opus-5".into(),
            input,
            output: 0,
            cache_write: 0,
            cache_write_1h: 0,
            cache_read: 0,
            project: "D--p".into(),
            session: "abcd1234".into(),
        }
    }

    /// 报错条数、最近请求、最后一条的时刻都跟着时间档筛；按小时只在「今天」给。
    #[test]
    fn the_transcript_extras_follow_the_selected_range() {
        let usage = TokenUsage {
            buckets: vec![bucket("2026-09-24", "claude-opus-5", 2, 0, 0, 0)],
            empty_replies: 3,
            empty_days: vec![
                tokens::TokenEmptyDay {
                    day: "2026-09-24".into(),
                    replies: 1,
                },
                tokens::TokenEmptyDay {
                    day: "2026-09-01".into(),
                    replies: 2,
                },
            ],
            recent: vec![
                request("2026-09-24", "02:39:00", 1_000_000),
                request("2026-09-01", "10:00:00", 1),
            ],
            hourly: vec![
                tokens::TokenHourBucket {
                    day: "2026-09-24".into(),
                    hour: 2,
                    model: "claude-opus-5".into(),
                    input: 2,
                    output: 0,
                    cache_write: 0,
                    cache_write_1h: 0,
                    cache_read: 0,
                    messages: 1,
                },
                tokens::TokenHourBucket {
                    day: "2026-09-23".into(),
                    hour: 23,
                    model: "claude-opus-5".into(),
                    input: 9,
                    output: 0,
                    cache_write: 0,
                    cache_write_1h: 0,
                    cache_read: 0,
                    messages: 1,
                },
            ],
            files_read: 4,
            ..Default::default()
        };
        let today = summarize_usage(&usage, 1, "2026-09-24", &snapshot_only(), true);
        assert_eq!(today.empty_replies, 1, "今天那一档不许带上三周前的报错");
        assert_eq!(today.recent.len(), 1);
        assert!(close(today.recent[0].cost_usd.unwrap(), 5.0));
        assert_eq!(today.last_at.as_deref(), Some("2026-09-24 02:39:00"));
        assert_eq!(today.hourly.len(), 1, "昨天 23 点那一格不属于今天");
        assert_eq!(today.hourly[0].hour, 2);
        assert_eq!(today.coverage.as_ref().unwrap().files_read, 4);

        let all = summarize_usage(&usage, 0, "2026-09-24", &snapshot_only(), true);
        assert_eq!(all.empty_replies, 3);
        assert!(all.hourly.is_empty(), "按小时只给「今天」那一档");

        let card = summarize_usage(&usage, 1, "2026-09-24", &snapshot_only(), false);
        assert!(
            card.recent.is_empty() && card.prices_used.is_empty(),
            "账户卡不要明细"
        );
        assert_eq!(
            card.last_at.as_deref(),
            Some("2026-09-24 02:39:00"),
            "最后一条照样给"
        );
    }

    /// 按账户：每个槽位一行，未归属与「别的账户」各一行（有量才出现）。
    #[test]
    fn the_overview_has_one_row_per_slot_plus_the_unowned_parts() {
        let slot = |b: Vec<TokenBucket>| TokenUsage {
            buckets: b,
            unattributed: vec![bucket("2026-09-24", "claude-opus-5", 3_000_000, 0, 0, 0)],
            ..Default::default()
        };
        let all = tokens::AllSlotsUsage {
            slots: vec![
                (
                    "main".into(),
                    slot(vec![bucket(
                        "2026-09-24",
                        "claude-opus-5",
                        1_000_000,
                        0,
                        0,
                        0,
                    )]),
                ),
                (
                    "demo-b".into(),
                    slot(vec![bucket(
                        "2026-09-24",
                        "claude-opus-5",
                        2_000_000,
                        0,
                        0,
                        0,
                    )]),
                ),
            ],
            other_accounts: vec![],
        };
        let o = overview_from(&all, "main", 1, "2026-09-24", &snapshot_only());
        assert!(close(o.summary.cost_usd.unwrap(), 5.0), "小结是 main 的");
        assert_eq!(
            o.by_account.len(),
            3,
            "两个槽位 + 未归属；别的账户没量就不出现"
        );
        assert_eq!(o.by_account[1].label, "demo-b");
        assert!(close(o.by_account[1].cost_usd.unwrap(), 10.0));
        assert_eq!(o.by_account[2].kind, AccountSpendKind::Unattributed);
        assert!(close(o.by_account[2].cost_usd.unwrap(), 15.0));
    }
}
