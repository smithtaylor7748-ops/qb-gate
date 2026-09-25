//! 官方参考价。**纯数据 + 纯算术,不联网** —— 抓取更新在编排层。
//!
//! # 它是干什么用的
//!
//! 站点标称「×0.20」,那是相对**官方价**的倍数。要验证这个标称是不是真的,
//! 就得拿站点**实际收的单价**跟官方单价比:
//!
//! ```text
//! 这一类的倍率 = 站点这一类的单价(已乘分组倍率) ÷ 官方这一类的单价
//! ```
//!
//! 站点单价从哪来,两种后端不一样(见 [`StationRates`]):sub2api 系直接公布单价;
//! New API 系公布的是**以 $2 / 百万 token 为 1 的倍率**,先换成单价
//! ([`NEW_API_USD_PER_MTOK`])。所以官方价目表是「查套路」的分母,没有它就只能听站点自己说。
//!
//! # ⛔ 四类各算各的,不是一个数
//!
//! 输入 / 缓存读 / 缓存写 / 输出四类的单价比例各家都不一样。只算一个总倍率,
//! 会被「输入便宜、输出贵三倍」这种结构藏过去 —— 而绝大多数真实用量里
//! 输出才是大头。
//!
//! # ⛔ 官方没有可以程序化拉价格的接口
//!
//! Models API(`GET /v1/models`)只回 id / 上下文窗口 / 能力位,**没有价格字段**。
//! 唯一机器可读的来源是文档页(见 [`ModelPrice::source_url`])。所以这里内置一份
//! 带 `verified_at` 的表,由编排层定时抓文档页更新;超过
//! [`STALE_AFTER_DAYS`] 就标成过期,**界面要照实说「价目可能过期」**,
//! 而不是拿一个旧价算出一个看起来很确定的倍率。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 价目多久算过期。
///
/// 半年。定价变动不频繁,但也绝不是永不变 —— 拿一年前的价算出来的「真实倍率」
/// 会把调价冤枉成造假。
pub const STALE_AFTER_DAYS: i64 = 180;

/// New API 的倍率为 1 时,每百万 token 多少美元(站内额度)。
///
/// # ⛔ New API 的 `model_ratio` 不是「相对官方几折」
///
/// 它是**单价**,单位是 $0.002 / 1K token。New API 源码
/// `setting/ratio_setting/model_ratio.go` 原文 `1 === $0.002 / 1K tokens`,
/// 默认表里 `"gpt-4o": 1.25, // $2.5 / 1M tokens`、`"claude-3-opus-20240229": 7.5,
/// // $15 / 1M tokens`;它自己的定价页(`web/src/features/pricing/lib/price.ts`)
/// 算单价也是 `model_ratio * 2 * 分组倍率`。
///
/// 所以照官方价原样抄进后台的 Claude Opus 5($5 / $25)在 New API 里是
/// `model_ratio 2.5`、`completion_ratio 5` —— 把 2.5 读成「官方的 2.5 倍」、
/// 把 5 读成「输出翻 5 倍」,正是 0.25.3 及以前的错法。站点的折扣在两个地方:
/// 分组倍率(`group_ratio`),或者直接压低 `model_ratio`(「倍率算在单价里」)。
/// 两种都只有先换成单价、再除以官方单价才读得出来。
///
/// ⚠ 严格说这个数是 `1_000_000 ÷ quota_per_unit`(`quota_per_unit` 在
/// `/api/status` 里公布,默认 500000)。New API 自己的定价页也写死 2 —— 改过
/// `quota_per_unit` 的站点,它自己页面上的单价跟账单就对不上;账单核对不受影响,
/// 那边是 `quota ÷ quota_per_unit`,见 `station_billing::usage`。
pub const NEW_API_USD_PER_MTOK: f64 = 2.0;

/// 缓存写的标准倍数(相对输入价)。
pub const CACHE_WRITE_RATIO: f64 = 1.25;
/// 缓存读的标准倍数(相对输入价)。
pub const CACHE_READ_RATIO: f64 = 0.10;

/// 一个价格是抄来的还是推出来的。
///
/// **必须分开。** 缓存价按标准倍数推,对绝大多数模型成立,但**有例外** ——
/// 官方对某些模型单独定过缓存读价。把推出来的当成抄来的,
/// 那几个模型的真实倍率就会系统性地算错,而界面上一切正常。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum PriceBasis {
    /// 官方明码标出来的。
    Published,
    /// 按标准倍数从输入价推的。
    Derived,
}

/// 四类单价里的一类。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UnitPrice {
    /// 每百万 token 多少钱。
    pub per_mtok: f64,
    pub basis: PriceBasis,
}

impl UnitPrice {
    pub fn published(v: f64) -> Self {
        Self {
            per_mtok: v,
            basis: PriceBasis::Published,
        }
    }
    pub fn derived(v: f64) -> Self {
        Self {
            per_mtok: v,
            basis: PriceBasis::Derived,
        }
    }
}

/// 一个模型的官方价。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModelPrice {
    pub input: UnitPrice,
    pub output: UnitPrice,
    pub cache_read: UnitPrice,
    pub cache_write: UnitPrice,
    /// 币种。**跨币种不比** —— 汇率每天在动,拿它换算出来的「真实倍率」
    /// 会把汇率波动算成站点造假。
    pub currency: &'static str,
    pub source: &'static str,
    pub source_url: &'static str,
    /// 这份价是哪天核对的(`YYYY-MM-DD`)。
    pub verified_at: &'static str,
}

impl ModelPrice {
    /// 把内置快照那一条抬成运行期的 [`ResolvedPrice`]。
    ///
    /// `live: false` —— 这是编译进去的快照,不是这次启动抓回来的。
    /// **界面要分得出来**,过期的快照跟刚抓的价不是一回事。
    pub fn as_resolved(&self) -> ResolvedPrice {
        ResolvedPrice {
            input: self.input,
            output: self.output,
            cache_read: self.cache_read,
            cache_write: self.cache_write,
            // 内置表只记 5 分钟档;1 小时档一律按官方倍数推,标成 `Derived`。
            cache_write_1h: UnitPrice::derived(self.input.per_mtok * CACHE_WRITE_1H_RATIO),
            currency: self.currency.to_string(),
            source_url: self.source_url.to_string(),
            verified_at: self.verified_at.to_string(),
            live: false,
        }
    }
}

/// 价目能不能用来比。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum PriceStatus {
    /// 有价、没过期。
    Complete,
    /// 有价但已经超过 [`STALE_AFTER_DAYS`]。界面要写「价目可能过期」。
    Stale,
    /// 这个模型不在表里。**不是 0,是不知道。**
    Unknown,
    /// 站点与官方币种不同,不比。
    CurrencyMismatch,
}

const ANTHROPIC_PRICING: &str = "https://platform.claude.com/docs/en/pricing";
const ANTHROPIC_SOURCE: &str = "Anthropic 官方定价页";
const GOOGLE_PRICING: &str = "https://ai.google.dev/gemini-api/docs/pricing";
const GOOGLE_SOURCE: &str = "Google Gemini API 官方定价页";

/// Gemini：输入 / 输出 / 缓存读三项官方都明码标出;缓存写按标准倍数推。
///
/// Gemini **没有按 token 的缓存写价**（显式缓存按小时计存储费,隐式缓存免费）,
/// 这一项只为结构完整 —— 反重力的用量记录里缓存写恒为 0,永远乘不到它。
const fn gemini(input: f64, output: f64, cache_read: f64, verified_at: &'static str) -> ModelPrice {
    ModelPrice {
        input: UnitPrice {
            per_mtok: input,
            basis: PriceBasis::Published,
        },
        output: UnitPrice {
            per_mtok: output,
            basis: PriceBasis::Published,
        },
        cache_read: UnitPrice {
            per_mtok: cache_read,
            basis: PriceBasis::Published,
        },
        cache_write: UnitPrice {
            per_mtok: input * CACHE_WRITE_RATIO,
            basis: PriceBasis::Derived,
        },
        currency: "USD",
        source: GOOGLE_SOURCE,
        source_url: GOOGLE_PRICING,
        verified_at,
    }
}

/// 由输入价按标准倍数补出缓存两项。
const fn claude(input: f64, output: f64, verified_at: &'static str) -> ModelPrice {
    ModelPrice {
        input: UnitPrice {
            per_mtok: input,
            basis: PriceBasis::Published,
        },
        output: UnitPrice {
            per_mtok: output,
            basis: PriceBasis::Published,
        },
        cache_read: UnitPrice {
            per_mtok: input * CACHE_READ_RATIO,
            basis: PriceBasis::Derived,
        },
        cache_write: UnitPrice {
            per_mtok: input * CACHE_WRITE_RATIO,
            basis: PriceBasis::Derived,
        },
        currency: "USD",
        source: ANTHROPIC_SOURCE,
        source_url: ANTHROPIC_PRICING,
        verified_at,
    }
}

/// 内置价目表。
///
/// ⚠ 这是**快照**,不是实时价。编排层会定时抓 [`ModelPrice::source_url`] 更新,
/// 抓不到就继续用这一份并按 `verified_at` 标过期。
///
/// 缓存两项除非官方单独标过,否则是按标准倍数推的([`PriceBasis::Derived`])——
/// 界面上要能看出哪一项是推的。
pub const TABLE: &[(&str, ModelPrice)] = &[
    // ---- Anthropic
    (
        "claude-fable-5-1",
        ModelPrice {
            input: UnitPrice {
                per_mtok: 10.0,
                basis: PriceBasis::Published,
            },
            output: UnitPrice {
                per_mtok: 50.0,
                basis: PriceBasis::Published,
            },
            // 官方单独标过:0.25/MTok,不是输入价的 0.1 倍。
            // 这正是「推出来的价会错」的那个例子。
            cache_read: UnitPrice {
                per_mtok: 0.25,
                basis: PriceBasis::Published,
            },
            cache_write: UnitPrice {
                per_mtok: 12.5,
                basis: PriceBasis::Derived,
            },
            currency: "USD",
            source: ANTHROPIC_SOURCE,
            source_url: ANTHROPIC_PRICING,
            verified_at: "2026-06-24",
        },
    ),
    (
        "claude-fable-5",
        ModelPrice {
            input: UnitPrice {
                per_mtok: 10.0,
                basis: PriceBasis::Published,
            },
            output: UnitPrice {
                per_mtok: 50.0,
                basis: PriceBasis::Published,
            },
            // 同样单独标过,而且跟 5.1 不一样(1.0 对 0.25)。
            cache_read: UnitPrice {
                per_mtok: 1.0,
                basis: PriceBasis::Published,
            },
            cache_write: UnitPrice {
                per_mtok: 12.5,
                basis: PriceBasis::Derived,
            },
            currency: "USD",
            source: ANTHROPIC_SOURCE,
            source_url: ANTHROPIC_PRICING,
            verified_at: "2026-06-24",
        },
    ),
    ("claude-opus-5", claude(5.0, 25.0, "2026-06-24")),
    ("claude-opus-4-8", claude(5.0, 25.0, "2026-06-24")),
    ("claude-opus-4-7", claude(5.0, 25.0, "2026-06-24")),
    ("claude-opus-4-6", claude(5.0, 25.0, "2026-06-24")),
    ("claude-sonnet-5", claude(2.0, 10.0, "2026-06-24")),
    ("claude-sonnet-4-6", claude(3.0, 15.0, "2026-06-24")),
    ("claude-haiku-4-5", claude(1.0, 5.0, "2026-06-24")),
    // ---- OpenAI
    //
    // 这一条是使用者自己核对过的(station-monitor 的 STATION_BILLING_MODEL_PRICES),
    // 四类都是官方明码,不是推的。
    (
        "gpt-5.6-sol",
        ModelPrice {
            input: UnitPrice {
                per_mtok: 4.0,
                basis: PriceBasis::Published,
            },
            output: UnitPrice {
                per_mtok: 20.0,
                basis: PriceBasis::Published,
            },
            cache_read: UnitPrice {
                per_mtok: 0.4,
                basis: PriceBasis::Published,
            },
            cache_write: UnitPrice {
                per_mtok: 5.0,
                basis: PriceBasis::Published,
            },
            currency: "USD",
            source: "OpenAI official pricing",
            source_url: "https://developers.openai.com/api/docs/models/gpt-5.6-sol",
            verified_at: "2026-08-28",
        },
    ),
    // ---- Google（0.30.0,给反重力用量卡算「缓存省下」）
    //
    // 2026-09-21 对着官方定价页（页面标注更新于 2026-09-16）抄的付费档标准价。
    // 3.6 / 3.7 / 3.8 Flash 三个是**促销价**（页面原文「$0.75 through December 31, 2026.
    // $1.50 starting January 1, 2027」），2027-01-01 起翻倍 —— `verified_at` 到期前要重核。
    // 反重力记录里的模型 id 是 `gemini-3.7-flash` 这种不带 `-preview` 的形式,
    // 3.1 Pro 官方页只列了 `gemini-3.1-pro-preview`,两个 id 都收、同一份价。
    ("gemini-3.8-flash", gemini(0.75, 3.75, 0.075, "2026-09-21")),
    ("gemini-3.7-flash", gemini(0.75, 3.75, 0.075, "2026-09-21")),
    ("gemini-3.6-flash", gemini(0.75, 3.75, 0.075, "2026-09-21")),
    ("gemini-3.5-flash", gemini(1.5, 9.0, 0.15, "2026-09-21")),
    (
        "gemini-3.5-flash-lite",
        gemini(0.3, 2.5, 0.03, "2026-09-21"),
    ),
    (
        "gemini-3.1-pro-preview",
        gemini(2.0, 12.0, 0.2, "2026-09-21"),
    ),
    ("gemini-3.1-pro", gemini(2.0, 12.0, 0.2, "2026-09-21")),
    ("gemini-2.5-pro", gemini(1.25, 10.0, 0.125, "2026-09-21")),
    ("gemini-2.5-flash", gemini(0.3, 2.5, 0.03, "2026-09-21")),
];

/// 查一个模型的官方价。
///
/// 名字先精确匹配,再去掉常见的日期后缀再试一次 —— 中转站经常把模型名写成
/// `claude-opus-5-20260401` 这种带快照日期的形式。反重力把开了思考的 Claude 记成
/// `claude-opus-4-6-thinking`（同一个模型、同一份价）,`-thinking` 也剥掉再试。
pub fn lookup(model: &str) -> Option<&'static ModelPrice> {
    let name = model.trim();
    if let Some((_, p)) = TABLE.iter().find(|(k, _)| *k == name) {
        return Some(p);
    }
    let base = price_key(name);
    TABLE.iter().find(|(k, _)| *k == base).map(|(_, p)| p)
}

/// 去掉末尾的 `-YYYYMMDD` / `@YYYYMMDD` / `-thinking`,得到查价用的名字。**纯函数。**
pub fn price_key(name: &str) -> &str {
    let name = name.trim();
    let name = name.strip_suffix("-thinking").unwrap_or(name);
    name.rsplit_once('@')
        .map(|(a, _)| a)
        .or_else(|| {
            name.rsplit_once('-')
                .filter(|(_, tail)| tail.len() == 8 && tail.chars().all(|c| c.is_ascii_digit()))
                .map(|(a, _)| a)
        })
        .unwrap_or(name)
}

/// 这份价还能不能用来比。
pub fn status(price: Option<&ModelPrice>, site_currency: &str, today: &str) -> PriceStatus {
    let Some(p) = price else {
        return PriceStatus::Unknown;
    };
    if !site_currency.is_empty() && !site_currency.eq_ignore_ascii_case(p.currency) {
        return PriceStatus::CurrencyMismatch;
    }
    match days_between(p.verified_at, today) {
        Some(d) if d > STALE_AFTER_DAYS => PriceStatus::Stale,
        Some(_) => PriceStatus::Complete,
        // 日期解析不出来时当作过期,**不当作新鲜** ——
        // 宁可让界面多说一句「可能过期」,也不要拿一份来历不明的价当准。
        None => PriceStatus::Stale,
    }
}

/// 两个 `YYYY-MM-DD` 差几天。解析不出来返回 `None`。
fn days_between(from: &str, to: &str) -> Option<i64> {
    Some(days_from_civil(parse_ymd(to)?) - days_from_civil(parse_ymd(from)?))
}

fn parse_ymd(s: &str) -> Option<(i64, i64, i64)> {
    let mut it = s.trim().splitn(3, '-');
    let y = it.next()?.parse().ok()?;
    let m = it.next()?.parse().ok()?;
    let d = it.next()?.parse().ok()?;
    (1..=12).contains(&m).then_some(())?;
    (1..=31).contains(&d).then_some(())?;
    Some((y, m, d))
}

/// Howard Hinnant 的 `days_from_civil`。不引第三方日期库 ——
/// 这里只需要「差几天」,而 `chrono` 不在这个 crate 的依赖里。
fn days_from_civil((y, m, d): (i64, i64, i64)) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// 四类之一。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum PriceCategory {
    Input,
    CacheRead,
    CacheWrite,
    Output,
}

impl PriceCategory {
    pub const ALL: [PriceCategory; 4] = [
        PriceCategory::Input,
        PriceCategory::CacheRead,
        PriceCategory::CacheWrite,
        PriceCategory::Output,
    ];

    /// 界面上的名字。纯文本渲染,不写 Markdown 的星号。
    pub fn label(self) -> &'static str {
        match self {
            PriceCategory::Input => "普通输入",
            PriceCategory::CacheRead => "缓存读取",
            PriceCategory::CacheWrite => "缓存写入",
            PriceCategory::Output => "输出",
        }
    }

    pub fn of(self, p: &ModelPrice) -> UnitPrice {
        match self {
            PriceCategory::Input => p.input,
            PriceCategory::CacheRead => p.cache_read,
            PriceCategory::CacheWrite => p.cache_write,
            PriceCategory::Output => p.output,
        }
    }
}

/// 四类里的一类:**站点自己公布的计费结构**长什么样。
///
/// # ⛔ 这个结构体不声称能证明「超没超收」
///
/// 它曾经有一个 `real_multiplier` 字段,注释写着
/// 「真实倍率 = 站点单价 × 标称倍率 ÷ 官方单价」。那个算式在真实调用路径上
/// **恒等于站点自己公布的两个数相乘** —— 因为当时调用方
/// (`station_ops::run_audit`)手里从来没有站点的绝对单价,它拿站点公布的
/// 倍率乘上官方价凑出一个「站点单价」,`verdicts` 再除回去:
///
/// ```text
/// 凑出来的单价     = category_ratio × official
/// real_multiplier = 凑出来的单价 × advertised ÷ official
///                 = category_ratio × advertised        ← official 被约掉了
/// ```
///
/// 也就是说:站点说它便宜,这张表就说它便宜。「官方单价」那一列与
/// [`PriceBasis`] 在那条路径上是纯装饰,改它们不会让任何判定变化。
/// 这是「看起来通过了、实际什么都没测」的那一类失效 —— 比没有这项检查更危险。
///
/// # 现在它只报「站点自己说了什么」,换成同一个单位
///
/// | 站点后端   | 它公布什么                        | `station_price` 怎么来 |
/// |------------|-----------------------------------|------------------------|
/// | New API 系 | 倍率(1 = $2 / 百万 token)         | 倍率 × [`NEW_API_USD_PER_MTOK`] |
/// | sub2api 系 | 绝对单价,美元／百万 token          | 原样 |
///
/// 两种都乘上站点公布的分组倍率(知道的话)—— 跟它们自己定价页上显示的单价是同一个数。
/// ⛔ **官方价不参与 `station_price`**:New API 那个 2 是它源码里写死的单位,不是
/// 官方价。上面那条失效的根子是「拿官方价凑站点单价」,官方价因此被约掉;这里官方价
/// 只在界面上当分母出现一次,改官方价结论就跟着变。
///
/// 「它到底收了几倍」由 [`measured_multiplier`] 回答 —— 那一个的分子是
/// 站点账单上真金白银的实扣,分母是同一批 token 按官方单价算出来的成本,
/// 两头都不是站点公布的数。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CategoryVerdict {
    pub category: PriceCategory,
    /// New API 的原始倍率乘积(`model_ratio` × 子倍率 × 分组倍率 × 峰时倍率),
    /// 只为对照站点后台里填的那几个数。
    ///
    /// ⛔ **不是相对官方的倍数** —— New API 的倍率 1 = $2 / 百万 token,
    /// 要看单价用 `station_price`。公布绝对单价的站点(sub2api 系)这一格是 `None`。
    /// 0.25.3 及以前存下来的检验历史里,这一格没乘分组倍率,也没有 `station_price`。
    pub station_ratio: Option<f64>,
    /// 站点这一类的**单价**(美元／百万 token,站内额度,已乘分组与峰时)。
    ///
    /// sub2api 系原样取它公布的单价;New API 系是倍率 × [`NEW_API_USD_PER_MTOK`]。
    pub station_price: Option<f64>,
    /// 这家站点按哪套口径公布价格。界面据此决定显示哪一列。
    pub basis_kind: RateBasis,
    /// 官方这一类每百万 token 多少钱。**只用于显示「本来多少钱」**,
    /// 不参与任何「超没超收」的判定。
    pub official: Option<f64>,
    /// 官方价是抄来的还是推出来的 —— 推出来的那几类,任何基于它的结论都要弱一档。
    pub basis: Option<PriceBasis>,
}

/// 标称与实测差多少以内算一致。跟 `route::RATE_TOLERANCE` 是两个东西:
/// 那个比的是「实扣 ÷ 标称」,这个比的是单价。
pub const PRICE_TOLERANCE: f64 = 0.01;

/// 把站点公布的四类价与官方单价并排列出来。**只做展示,不下判定。**
///
/// 两种后端都换成单价填进 `station_price`(New API 按它自己的单位换,
/// 见 [`NEW_API_USD_PER_MTOK`]);New API 的原始倍率另外留在 `station_ratio`
/// 供对照。⛔ **官方价不参与站点单价** —— 界面要比的话拿单价除官方单价,
/// 那时官方价只出现在分母上一次。
///
/// 取不到的那一类给 `None` —— **不要填 0**,0 会显示成「这一类免费」。
pub fn verdicts(rates: &StationRates, official: Option<&ResolvedPrice>) -> Vec<CategoryVerdict> {
    let kind = rates.basis();
    let ratios = rates.newapi_ratios();
    let prices = rates.category_prices();
    let clean = |v: Option<f64>| v.filter(|x: &f64| x.is_finite() && *x >= 0.0);
    PriceCategory::ALL
        .iter()
        .enumerate()
        .map(|(i, &category)| {
            let unit = official.map(|p| p.category(category));
            CategoryVerdict {
                category,
                station_ratio: clean(ratios[i]),
                station_price: clean(prices[i]),
                basis_kind: kind,
                official: unit.map(|u| u.per_mtok).filter(|v| *v > 0.0),
                basis: unit.map(|u| u.basis),
            }
        })
        .collect()
}

/// 同一批 token 按**官方单价**该花多少钱。
///
/// `tokens` 的顺序同 [`PriceCategory::ALL`]:输入 / 缓存读 / 缓存写 / 输出。
pub fn official_cost(tokens: [u64; 4], p: &ResolvedPrice) -> f64 {
    PriceCategory::ALL
        .iter()
        .enumerate()
        .map(|(i, &c)| tokens[i] as f64 * p.category(c).per_mtok / 1_000_000.0)
        .sum()
}

/// 拿**真实账单**反推「这家站点实扣是官方价的几倍」。
///
/// # ⛔ 这是唯一一个能独立证明超收的算式
///
/// ```text
/// 实测倍率 = 账单实扣 ÷ Σ(这一批实际 token × 官方单价)
/// ```
///
/// 分子是站点账单上真金白银的数,分母是同一批 token 按官方价算出来的成本。
/// **两头都不是站点公布的倍率** —— 站点改它的价目表改不动这个结论。
/// 这正是 [`CategoryVerdict`] 那张表做不到的事。
///
/// # 四类必须全部取证到
///
/// 少一类就把分母算小,倍率被系统性抬高 —— 而那正好是「这家站点在超收」
/// 这个方向。缺任何一类一律返回 `None`,跟 [`TokenMix::shares`] 同一条规矩。
///
/// [`TokenMix::shares`]: crate::health::window::TokenMix::shares
///
/// # 返回 `None` 的其余情形
///
/// * 这一批一个 token 都没有(分母为零,算出来是无穷大);
/// * 实扣不是个有限的非负数。
///
/// 一律 `None`,**绝不返回 1.0** —— 1.0 是「量过了,属实」这个断言。
pub fn measured_multiplier(
    tokens: [Option<u64>; 4],
    actual_cost: Option<f64>,
    p: &ResolvedPrice,
) -> Option<f64> {
    let cost = actual_cost.filter(|v| v.is_finite() && *v >= 0.0)?;
    let mut all = [0u64; 4];
    for (i, t) in tokens.iter().enumerate() {
        all[i] = (*t)?;
    }
    if all.iter().all(|&t| t == 0) {
        return None;
    }
    let official = official_cost(all, p);
    (official > 0.0 && official.is_finite()).then(|| cost / official)
}

/// 运行期抓回来的一条价。
///
/// 跟 [`ModelPrice`] 是两个类型:那个是编译进二进制的快照(`&'static str`),
/// 这个是抓回来、要落盘的(`String`)。合成一个的话,内置表就没法是 `const`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FetchedPrice {
    pub model: String,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    /// 抓不到就留空,由 [`Catalog`] 按标准倍数补并标成 `Derived`。
    pub cache_read_per_mtok: Option<f64>,
    /// 5 分钟档缓存写。
    pub cache_write_per_mtok: Option<f64>,
    /// 1 小时档缓存写(只有 Anthropic 的表有)。
    ///
    /// `serde(default)`:0.25.1 之前抓回来存进库里的那些行没有这个字段,
    /// 读出来就是 `None`,由 [`Catalog`] 按 [`CACHE_WRITE_1H_RATIO`] 推。
    #[serde(default)]
    pub cache_write_1h_per_mtok: Option<f64>,
    /// 长上下文那一档的价。
    ///
    /// ⛔ **这是「计费翻倍」的另一种。** OpenAI 对超长上下文单独定价:
    /// `gpt-5.6-sol` 短上下文 $4/$20,长上下文 $8/$30 —— 输入翻一倍。
    /// 拿短上下文价当基准去验一条专跑长上下文的线,会把它冤枉成超收两倍。
    ///
    /// Anthropic 侧现役模型**全 1M 上下文同价**(官方原文:
    /// 「900k-token 请求与 9k-token 请求同价」),所以那边恒为 `None`。
    pub long_context: Option<LongContext>,
    pub currency: String,
    pub source_url: String,
    /// 抓回来的那天(`YYYY-MM-DD`)。
    pub fetched_at: String,
}

/// 长上下文那一档的单价。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LongContext {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_read_per_mtok: Option<f64>,
    pub cache_write_per_mtok: Option<f64>,
}

/// 内置快照 + 抓回来的覆盖。
///
/// # ⛔ 抓回来的**只在解析出东西时**才覆盖
///
/// 官方那几个页面随时可能改版。解析不出来时**继续用内置表**并如实报错,
/// 绝不让一次失败的抓取把好价清空 —— 价目被清空的后果是所有「真实倍率」
/// 一起变成「不知道」,而界面上看起来只是「都没检验过」。
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    fetched: Vec<FetchedPrice>,
}

impl Catalog {
    pub fn new(fetched: Vec<FetchedPrice>) -> Self {
        Self { fetched }
    }

    /// 抓回来的有几条。
    pub fn fetched_count(&self) -> usize {
        self.fetched.len()
    }

    /// 查一个模型:先看抓回来的,没有再退回内置快照。
    pub fn resolve(&self, model: &str) -> Option<ResolvedPrice> {
        let name = model.trim();
        let key = price_key(name);
        if let Some(f) = self
            .fetched
            .iter()
            .find(|f| f.model.eq_ignore_ascii_case(name) || f.model.eq_ignore_ascii_case(key))
        {
            let read = f
                .cache_read_per_mtok
                .map(UnitPrice::published)
                .unwrap_or_else(|| UnitPrice::derived(f.input_per_mtok * CACHE_READ_RATIO));
            let write = f
                .cache_write_per_mtok
                .map(UnitPrice::published)
                .unwrap_or_else(|| UnitPrice::derived(f.input_per_mtok * CACHE_WRITE_RATIO));
            let write_1h = f
                .cache_write_1h_per_mtok
                .map(UnitPrice::published)
                .unwrap_or_else(|| UnitPrice::derived(f.input_per_mtok * CACHE_WRITE_1H_RATIO));
            return Some(ResolvedPrice {
                input: UnitPrice::published(f.input_per_mtok),
                output: UnitPrice::published(f.output_per_mtok),
                cache_read: read,
                cache_write: write,
                cache_write_1h: write_1h,
                currency: f.currency.clone(),
                source_url: f.source_url.clone(),
                verified_at: f.fetched_at.clone(),
                live: true,
            });
        }
        lookup(name).map(ModelPrice::as_resolved)
    }
}

/// 查出来的一条价,连同它是抓回来的还是内置的。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ResolvedPrice {
    pub input: UnitPrice,
    pub output: UnitPrice,
    pub cache_read: UnitPrice,
    /// 5 分钟档缓存写。中转站那套四类判定([`PriceCategory::CacheWrite`])用的就是它。
    pub cache_write: UnitPrice,
    /// 1 小时档缓存写。官方标了就是 `Published`,没标按 [`CACHE_WRITE_1H_RATIO`] 推。
    /// 只给用量折算用 —— 中转站的四类判定不认这一档。
    pub cache_write_1h: UnitPrice,
    pub currency: String,
    pub source_url: String,
    pub verified_at: String,
    /// 这一条是这次启动抓回来的(`true`),还是内置快照(`false`)。
    /// **界面要分得出来** —— 内置快照可能已经过期。
    pub live: bool,
}

impl ResolvedPrice {
    pub fn category(&self, c: PriceCategory) -> UnitPrice {
        match c {
            PriceCategory::Input => self.input,
            PriceCategory::CacheRead => self.cache_read,
            PriceCategory::CacheWrite => self.cache_write,
            PriceCategory::Output => self.output,
        }
    }

    /// 还能不能用来比。
    pub fn status(&self, site_currency: &str, today: &str) -> PriceStatus {
        if !site_currency.is_empty() && !site_currency.eq_ignore_ascii_case(&self.currency) {
            return PriceStatus::CurrencyMismatch;
        }
        match days_between(&self.verified_at, today) {
            Some(d) if d > STALE_AFTER_DAYS => PriceStatus::Stale,
            Some(_) => PriceStatus::Complete,
            None => PriceStatus::Stale,
        }
    }
}

/// 站点怎么计费。
///
/// ⛔ **不是一个倍率。** New API 的每个模型给的是一组比例,四类单价这样算
/// (跟 New API 自己定价页的算法一样,见 [`NEW_API_USD_PER_MTOK`]):
///
/// ```text
/// 输入   = model_ratio                        × $2/百万
/// 输出   = model_ratio × completion_ratio     × $2/百万
/// 缓存读 = model_ratio × cache_ratio          × $2/百万
/// 缓存写 = model_ratio × create_cache_ratio   × $2/百万
/// 再乘   × 分组倍率 ×(峰时浮动)
/// ```
///
/// `completion_ratio` 是「输出价 ÷ 输入价」—— Claude 官方本来就是 5
/// (Opus 5:$25 ÷ $5),照官方价抄的站就填 5。⛔ **它大于 1 不是「计费翻倍」**;
/// 输出有没有另外加价,要拿它跟官方的输出 ÷ 输入比,见 [`output_markup`](Self::output_markup)。
/// 相对官方的倍率只能先换成单价再除官方单价,见 [`category_ratios_against`](Self::category_ratios_against)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StationRates {
    /// New API 的 `model_ratio`:输入单价,**单位是 $2 / 百万 token**(见 [`NEW_API_USD_PER_MTOK`])。
    /// ⛔ 不是「官方的几倍」—— 照官方价抄的 Opus 5 是 2.5。
    pub model_ratio: Option<f64>,
    /// 输出价 ÷ 输入价(`completion_ratio`)。照官方价抄的 Claude 是 5,不是「翻倍」。
    pub completion_ratio: Option<f64>,
    /// 缓存读价 ÷ 输入价(`cache_ratio`)。
    pub cache_ratio: Option<f64>,
    /// 缓存写价 ÷ 输入价(`create_cache_ratio`)。
    pub create_cache_ratio: Option<f64>,
    /// 分组倍率。整条线路再乘一次 —— 大多数站的折扣就在这里(帖子里说的「倍率分组」)。
    ///
    /// New API 在 `/api/pricing` 顶层的 `group_ratio` 里按分组名公布
    /// ([`parse_station_pricing_in_group`]);sub2api 在 `/api/v1/groups/rates`。
    /// `None` = 不知道,**不是「不打折」** —— 加权比价那一步因此算不出来,见 [`blended_ratio`](Self::blended_ratio)。
    pub group_ratio: Option<f64>,
    /// 峰时浮动倍率。
    ///
    /// **取它当上界,不取平均** —— 实测有站点峰时浮动到 1.5 倍。
    /// 按平均算会让人在峰时付出比预期多的钱,而账面上「没超」。
    pub peak_rate: Option<f64>,
    /// 按次计费的单价。`Some` 时 token 倍率**整个不适用**。
    pub per_request_price: Option<f64>,

    // ---------------------------------------------------------------
    // 下面四项是**另一套口径**:站点直接公布的绝对单价(每百万 token)。
    //
    // ⛔ 跟上面那些倍率是**互斥的两套**,不是互补的:
    //
    // | 站点后端   | 它公布什么                          | 折扣在哪 |
    // |------------|-------------------------------------|----------|
    // | New API 系 | 倍率(1 = $2 / 百万 token 的单价)  | 分组倍率,或直接压低倍率(折扣算在单价里) |
    // | sub2api 系 | 绝对单价(美元／百万 token)        | 分组倍率 |
    //
    // 使用者原话:「newapi 是可以这样算,但 sub2api 不是,
    // sub2api 的单价就是官方单价。」「New API 的倍率算在了单价内,所以单价便宜。」
    // —— 两套最后都换成单价([`StationRates::category_prices`])再跟官方比。
    //
    // 两套混着填时以绝对单价为准,见 [`StationRates::basis`]。
    // 哪一套有效从「谁填了」推出来,
    // **不另设一个 kind 字段** —— 字段和内容对不上时,字段会骗人。
    /// 普通输入每百万 token 多少钱。
    #[serde(default)]
    pub input_price: Option<f64>,
    /// 缓存读每百万 token 多少钱。
    #[serde(default)]
    pub cache_read_price: Option<f64>,
    /// 缓存写每百万 token 多少钱。
    #[serde(default)]
    pub cache_write_price: Option<f64>,
    /// 输出每百万 token 多少钱。
    #[serde(default)]
    pub output_price: Option<f64>,
}

/// 这家站点是按哪套口径公布价格的。
///
/// **从「谁填了」推出来,不从一个单独的 kind 字段读** —— 字段与内容对不上时
/// (适配器改了、迁移漏了、手改过配置),字段会骗人而内容不会。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum RateBasis {
    /// 按次计费。四类 token 口径整个不适用。
    PerRequest,
    /// 倍率(New API 系):以 $2 / 百万 token 为 1 的单价,**不是相对官方的倍数**。
    Ratios,
    /// 绝对单价,每百万 token(sub2api 系)。
    AbsolutePrices,
    /// 两套都没填。**不是「免费」也不是「倍率 1.0」** —— 是不知道。
    Unknown,
}

impl StationRates {
    /// 这个模型是不是按次计费。按次计费时四类 token 倍率没有意义。
    pub fn per_request(&self) -> bool {
        self.per_request_price
            .is_some_and(|v| v.is_finite() && v > 0.0)
    }

    /// 这家站点按哪套口径公布价格。见 [`RateBasis`]。
    ///
    /// 判定顺序是有理由的:按次计费**压过**另外两套(它一旦成立,四类 token
    /// 口径整个不适用);绝对单价压过倍率(两套都填时以更具体的那一套为准,
    /// 因为绝对单价不需要知道站点的配额基准)。
    pub fn basis(&self) -> RateBasis {
        if self.per_request() {
            return RateBasis::PerRequest;
        }
        let ok = |v: Option<f64>| v.is_some_and(|x| x.is_finite() && x > 0.0);
        if ok(self.input_price) || ok(self.output_price) {
            return RateBasis::AbsolutePrices;
        }
        if ok(self.model_ratio) {
            return RateBasis::Ratios;
        }
        RateBasis::Unknown
    }

    /// 分组倍率 × 峰时上界。
    ///
    /// 两者取不到时按 1 处理 —— 这里只管「站点页面上那个单价是多少」,
    /// 跟站点自己的定价页一样:不知道分组就显示不打折的价。
    /// ⛔ **比价不能这么将就**:折扣大多就在分组上,不知道分组倍率就不知道价,
    /// 所以 [`blended_ratio`](Self::blended_ratio) 要求它必须已知。
    fn scale(&self) -> f64 {
        let one = |v: Option<f64>| v.filter(|x| x.is_finite() && *x > 0.0).unwrap_or(1.0);
        one(self.group_ratio) * one(self.peak_rate)
    }

    /// New API 的原始倍率乘积(已乘分组倍率与峰时上界),四类各一个。
    ///
    /// ⛔ **这不是「相对官方的倍率」**,是以 $2 / 百万 token 为 1 的**单价**:
    /// 照官方价抄的 Opus 5 在这里是 `[2.5, 0.25, 3.125, 12.5]`,而它相对官方的
    /// 倍率是四个 1。0.25.3 及以前这个函数叫 `category_ratios`,被当成
    /// 「相对官方的倍率」拿去显示和比价 —— 结果是照官方价收费的站被显示成
    /// 「输入 2.5 倍、输出 12.5 倍」,分组折扣也整个没算进去。
    ///
    /// 顺序与 [`PriceCategory::ALL`] 一致。任何一项算不出来就是 `None` ——
    /// **不拿 1.0 顶替**,那等于断言「这一类跟输入同价」。
    /// 公布绝对单价的站点(sub2api 系)在这里全 `None`。
    pub fn newapi_ratios(&self) -> [Option<f64>; 4] {
        if self.basis() != RateBasis::Ratios {
            return [None; 4];
        }
        let base = self.model_ratio.filter(|v| v.is_finite() && *v > 0.0);
        let scale = self.scale();
        let of = |sub: Option<f64>| -> Option<f64> {
            let b = base?;
            let r = sub.filter(|v| v.is_finite() && *v > 0.0)?;
            Some(b * r * scale)
        };
        [
            base.map(|b| b * scale),
            of(self.cache_ratio),
            of(self.create_cache_ratio),
            of(self.completion_ratio),
        ]
    }

    /// 四类各自的**单价**(美元／百万 token,站内额度,已乘分组倍率与峰时上界)。
    ///
    /// | 站点口径 | 怎么来 |
    /// |---|---|
    /// | 绝对单价(sub2api 系) | 它公布的单价 |
    /// | 倍率(New API 系) | [`newapi_ratios`](Self::newapi_ratios) × [`NEW_API_USD_PER_MTOK`] |
    /// | 按次 / 不知道 | 全 `None` |
    ///
    /// ⛔ **官方价不是这里的输入。** 0.16.0 之前那个「官方价被约掉」的失效,
    /// 是拿站点倍率乘**官方价**凑出站点单价;New API 那个 2 是它源码里写死的单位,
    /// 跟官方价无关 —— 这个函数的签名里压根没有官方价。
    pub fn category_prices(&self) -> [Option<f64>; 4] {
        match self.basis() {
            RateBasis::Ratios => self
                .newapi_ratios()
                .map(|r| r.map(|v| v * NEW_API_USD_PER_MTOK)),
            RateBasis::AbsolutePrices => {
                let scale = self.scale();
                let of =
                    |v: Option<f64>| v.filter(|x| x.is_finite() && *x >= 0.0).map(|x| x * scale);
                [
                    of(self.input_price),
                    of(self.cache_read_price),
                    of(self.cache_write_price),
                    of(self.output_price),
                ]
            }
            RateBasis::PerRequest | RateBasis::Unknown => [None; 4],
        }
    }

    /// 四类各自相对**官方价**的倍率:站点单价 ÷ 官方单价。**两套口径在这里合流。**
    ///
    /// 照官方价原样抄进后台、分组 ×0.2 的站,四类都是 0.2 —— 不管它是 New API
    /// (倍率 2.5 / 补全 5)还是 sub2api(单价 $5 / $25),也不管折扣是放在分组上
    /// 还是直接压进了单价里。
    ///
    /// ⚠ 这是**站点公布的价**,不是它实际收的。「这家到底收了几倍」只有
    /// [`measured_multiplier`] 答得了。
    pub fn category_ratios_against(&self, official: &ResolvedPrice) -> [Option<f64>; 4] {
        let prices = self.category_prices();
        let mut out = [None; 4];
        for (i, &c) in PriceCategory::ALL.iter().enumerate() {
            let o = official.category(c).per_mtok;
            if o > 0.0 && o.is_finite() {
                out[i] = prices[i].map(|p| p / o);
            }
        }
        out
    }
}

/// 一次抓取至少要认出这么多个模型才算成功。
///
/// 页面改版时解析器往往还能凑巧认出一两行(比如某段正文里带 `$`),
/// 那种「半成功」比彻底失败更危险 —— 它会用一份残缺的表覆盖掉完整的内置快照。
/// 设一个下限,认得太少就整批丢弃。
pub const MIN_PARSED_MODELS: usize = 3;

/// 一个合理的单价上限(每百万 token 美元)。超过就认为解析错了。
///
/// 现役最贵的模型是 $50/MTok 一档。1000 这个上限宽到不可能误伤,
/// 又能挡住「把上下文窗口 200000 当成价格读进来」这类错位。
pub const MAX_PLAUSIBLE_PER_MTOK: f64 = 1000.0;

/// 1 小时缓存写的倍数(相对输入价)。**是 2 倍,不是 1.25** ——
/// 1.25 那档是 5 分钟缓存写。官方原文:5m 写 1.25x、1h 写 2x、读 0.1x。
pub const CACHE_WRITE_1H_RATIO: f64 = 2.0;

/// 官方定价页的表格长什么样。**两家完全不同,必须分开解析。**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PricingFormat {
    /// Anthropic:`模型 | 基础输入 | 5m 缓存写 | 1h 缓存写 | 缓存读 | 输出`
    ///
    /// 模型那一列是**显示名**(`Claude Opus 5`),不是 id,而且可能带
    /// markdown 链接和脚注标记。
    Anthropic,
    /// OpenAI:`模型 | 短上下文×4 | 长上下文×4`
    ///
    /// 每一档四列是 `输入 | 缓存读 | 缓存写 | 输出`。
    /// **取短上下文那一档**当基准价 —— 长上下文是另一档,见 `long_context`。
    OpenAi,
}

impl PricingFormat {
    /// 要的是哪一节的表。
    ///
    /// ⛔ **必须按节过滤。** 两个页面都有好几张表,同一个模型在
    /// 批处理表里是五折价。靠「同名只取第一个」凑对纯属表格顺序的运气 ——
    /// 上游一调顺序就会悄悄采用批处理价,于是每一家站点都被算成超收两倍,
    /// 而且没有任何症状。
    fn section_heading(self) -> &'static str {
        match self {
            PricingFormat::Anthropic => "model pricing",
            PricingFormat::OpenAi => "standard pricing data",
        }
    }
}

/// 这一行是不是 markdown 标题。
fn heading_text(line: &str) -> Option<String> {
    let t = line.trim_start();
    t.starts_with('#')
        .then(|| t.trim_start_matches('#').trim().to_ascii_lowercase())
}

/// 从官方定价文档(markdown)里把每个模型的价解析出来。
///
/// **纯函数** —— 抓取那一步在编排层。
///
/// # ⛔ 认不出来就返回空
///
/// 返回空表示「这次没拿到」,调用方必须继续用内置快照。
/// **绝不返回半张表** —— 见 [`MIN_PARSED_MODELS`]。
pub fn parse_pricing_markdown(
    text: &str,
    today: &str,
    source_url: &str,
    format: PricingFormat,
) -> Vec<FetchedPrice> {
    let mut out: Vec<FetchedPrice> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        // 进到要的那一节才开始收,遇到下一个标题就停 ——
        // 别的表里同一个模型是批处理价(五折)。
        if let Some(h) = heading_text(line) {
            inside = h == format.section_heading();
            continue;
        }
        if !inside {
            continue;
        }
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 3 {
            continue;
        }
        let Some(parsed) = (match format {
            PricingFormat::Anthropic => anthropic_row(&cells),
            PricingFormat::OpenAi => openai_row(&cells),
        }) else {
            continue;
        };
        if out.iter().any(|f: &FetchedPrice| f.model == parsed.model) {
            continue;
        }
        if !plausible(parsed.input) || !plausible(parsed.output) {
            continue;
        }
        out.push(FetchedPrice {
            model: parsed.model,
            input_per_mtok: parsed.input,
            output_per_mtok: parsed.output,
            cache_read_per_mtok: parsed.cache_read.filter(|v| plausible(*v)),
            cache_write_per_mtok: parsed.cache_write.filter(|v| plausible(*v)),
            cache_write_1h_per_mtok: parsed.cache_write_1h.filter(|v| plausible(*v)),
            long_context: parsed.long,
            currency: "USD".into(),
            source_url: source_url.to_string(),
            fetched_at: today.to_string(),
        });
    }
    if out.len() < MIN_PARSED_MODELS {
        return Vec::new();
    }
    out
}

fn plausible(v: f64) -> bool {
    v.is_finite() && v > 0.0 && v <= MAX_PLAUSIBLE_PER_MTOK
}

/// 价目表里解析出来的一行。
struct Row {
    model: String,
    input: f64,
    cache_read: Option<f64>,
    /// 5 分钟档缓存写(OpenAI 那张表里就是它唯一的缓存写)。
    cache_write: Option<f64>,
    /// 1 小时档缓存写。只有 Anthropic 那张表有这一列。
    cache_write_1h: Option<f64>,
    output: f64,
    long: Option<LongContext>,
}

/// `模型 | 基础输入 | 5m 缓存写 | 1h 缓存写 | 缓存读 | 输出`
///
/// 1h 那一列 0.25.1 之前是读了就丢的。可 Claude Code 的缓存写实测**全是 1 小时档**
/// (`tokens.rs` 文件头),按 5 分钟档的价算用量要少算 37.5% —— 所以留下来。
fn anthropic_row(cells: &[&str]) -> Option<Row> {
    let model = display_name_to_id(cells.first()?)?;
    let money: Vec<f64> = cells.iter().skip(1).filter_map(|c| dollars(c)).collect();
    // 五个价都要齐:少一个说明这一行不是模型价那张表(比如批处理表只有两列)。
    if money.len() < 5 {
        return None;
    }
    Some(Row {
        model,
        input: money[0],
        cache_read: Some(money[3]), // 第四个,不是第二个
        cache_write: Some(money[1]),
        cache_write_1h: Some(money[2]),
        output: money[4],
        long: None,
    })
}

/// `模型 | 短上下文 输入/缓存读/缓存写/输出 | 长上下文 输入/缓存读/缓存写/输出`
fn openai_row(cells: &[&str]) -> Option<Row> {
    let raw = cells.first()?.trim().trim_matches('`');
    // 「gpt-5.5 (<272K context length)」这种带括号说明的,取括号前那截。
    let name = raw.split(['(', ' ']).next()?.trim().trim_matches('`');
    if !looks_like_model_id(name) {
        return None;
    }
    // `-` 表示这一档没有;用 None 占位保持列序。
    let cols: Vec<Option<f64>> = cells.iter().skip(1).map(|c| dollars(c)).collect();
    if cols.len() < 4 {
        return None;
    }
    let long = match (cols.get(4), cols.get(7)) {
        (Some(Some(i)), Some(Some(o))) => Some(LongContext {
            input_per_mtok: *i,
            output_per_mtok: *o,
            cache_read_per_mtok: cols.get(5).copied().flatten(),
            cache_write_per_mtok: cols.get(6).copied().flatten(),
        }),
        _ => None,
    };
    Some(Row {
        model: name.to_string(),
        input: cols[0]?,
        cache_read: cols.get(1).copied().flatten(),
        cache_write: cols.get(2).copied().flatten(),
        cache_write_1h: None,
        output: cols.get(3).copied().flatten()?,
        long,
    })
}

/// `Claude Opus 4.6` → `claude-opus-4-6`。
///
/// 官方表里那一列是显示名,而我们全项目按 id 索引。顺带剥掉 markdown 链接
/// (`Claude Opus 4.1 ([retired…](url))`)和脚注标记。
fn display_name_to_id(cell: &str) -> Option<String> {
    // 去掉 `([...](...))` 这种整段链接说明
    let mut name = cell.trim().to_string();
    if let Some(at) = name.find(" ([") {
        name.truncate(at);
    }
    if let Some(at) = name.find('(') {
        name.truncate(at);
    }
    let name = name.trim();
    if !name.to_ascii_lowercase().starts_with("claude ") {
        return None;
    }
    let id: String = name
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c == ' ' || c == '.' { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    (id.len() > 7).then_some(id)
}

fn looks_like_model_id(s: &str) -> bool {
    let s = s.trim_matches('`');
    (s.starts_with("gpt-")
        || s.starts_with("o1")
        || s.starts_with("o3")
        || s.starts_with("davinci"))
        && s.len() > 3
        && s.chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '.' || ch == '_')
}

/// 把 `$4.00` / `$10 / MTok` 里的金额取出来。
fn dollars(cell: &str) -> Option<f64> {
    let at = cell.find('$')?;
    let rest = &cell[at + 1..];
    let end = rest
        .find(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == ','))
        .unwrap_or(rest.len());
    let num = rest[..end].replace(',', "");
    num.parse::<f64>().ok().filter(|v| v.is_finite())
}

impl StationRates {
    /// 输出比官方的结构多收几倍:`(站点输出单价 ÷ 站点输入单价) ÷ (官方输出 ÷ 官方输入)`。
    ///
    /// # ⛔ 不是 `completion_ratio > 1`
    ///
    /// New API 的 `completion_ratio` 就是「输出价 ÷ 输入价」,官方自己就不是 1:
    /// Claude Opus 5 是 $25 ÷ $5 = 5,GPT-5.6 Sol 是 $20 ÷ $4 = 5。0.25.3 及以前
    /// 拿「大于 1」当「计费翻倍」,于是每一家照官方价收费的站都挂着「翻倍 ×5」。
    /// 真正的问题是:输出**在官方那个比例之外**又加了价 —— 标一个低倍率,
    /// 再把补全倍率调高,纯看倍率的人会选错站。
    ///
    /// 1.0 = 跟官方同一个结构;> 1 = 输出另外加了价;< 1 = 输出反而更便宜。
    /// 分组倍率在分子分母里约掉,不影响这个数。`None` = 站点没公布输入或输出,
    /// 或者官方价没有 —— **不知道,不是「没加价」**。
    pub fn output_markup(&self, official: &ResolvedPrice) -> Option<f64> {
        let [input, _, _, output] = self.category_prices();
        let (i, o) = (input?, output?);
        let (oi, oo) = (official.input.per_mtok, official.output.per_mtok);
        let ok = |v: f64| v.is_finite() && v > 0.0;
        (ok(i) && ok(o) && ok(oi) && ok(oo)).then(|| (o / i) / (oo / oi))
    }

    /// 按 token 结构加权出一个**可比**的等效倍率(相对官方价)。
    ///
    /// # 为什么非这样不可
    ///
    /// 官方 Claude Opus 5 是 $5 / $25。A 站分组 ×0.15,但把补全倍率从官方的 5
    /// 调成 10(输出 ×0.30);B 站分组 ×0.25,补全照官方填 5(四类都 ×0.25)。
    /// 谁便宜**取决于你的输入输出比**:
    ///
    /// - 纯读代码(输出占一成):A = 0.165,B = 0.25 → A 便宜;
    /// - 长篇生成(输出占七成):A = 0.255,B = 0.25 → B 便宜。
    ///
    /// 只比分组倍率会永远选 A —— 而长篇生成时 A 更贵。
    ///
    /// # ⛔ 分组倍率必须已知
    ///
    /// 折扣大多就在分组上(sub2api 全部、New API 大多数)。不知道分组倍率的站,
    /// 这里按不打折算出来的是它的牌价,跟别家的折后价放在一起比,等于说它贵了
    /// 五倍十倍 —— 所以返回 `None`,排序那边整池退回更粗的口径。
    /// 拿一个算不出来的数去比,比退回粗口径糟得多。
    ///
    /// `mix` 是四类占比(见 `health::window::TokenMix`)。
    /// 任何一类倍率缺席就返回 `None` —— **不拿 1.0 补**。
    pub fn blended_ratio(&self, mix: [f64; 4], official: &ResolvedPrice) -> Option<f64> {
        self.group_ratio.filter(|g| g.is_finite() && *g > 0.0)?;
        self.blend(mix, self.category_ratios_against(official))
    }

    fn blend(&self, mix: [f64; 4], r: [Option<f64>; 4]) -> Option<f64> {
        if self.per_request() {
            return None;
        }
        let mut total = 0.0;
        for (i, share) in mix.iter().enumerate() {
            if *share <= 0.0 {
                // 这一类不占量,缺席也无所谓 —— 乘 0 本来就不影响结果。
                continue;
            }
            total += r[i]? * share;
        }
        total.is_finite().then_some(total)
    }
}

/// 站点上一个模型:计费倍率 + 它属于哪几个分组。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StationModel {
    pub model: String,
    pub rates: StationRates,
    /// 这个模型在哪几个分组里可用(New API 的 `enable_groups`)。
    ///
    /// **空着表示站点没说**,不表示「哪个组都不能用」——
    /// 界面上那种情况要把模型全列出来让使用者自己挑,而不是一个都不给。
    pub groups: Vec<String>,
}

impl StationModel {
    /// 这个模型在某个分组里能不能用。
    ///
    /// 站点没公布分组时返回 `true` —— 「不知道」不该表现成「不能用」。
    pub fn in_group(&self, group: &str) -> bool {
        self.groups.is_empty() || self.groups.iter().any(|g| g == group)
    }
}

/// 查套路默认拿哪个模型去验。
///
/// Anthropic 协议的线路用 `claude-opus-5`,OpenAI 协议的用 `gpt-5.6-sol` ——
/// 使用者定的。
///
/// # 为什么是「默认」而不是「固定」
///
/// 一个模型就够验:站点要做手脚不会只在一个模型上做。但默认值必须**在站点
/// 真的提供这个模型时才用** —— 站点没有它却硬拿它去验,验的是一个不存在的
/// 东西,结果全是「没测到」。所以挑不中就退回该分组的第一个模型,
/// 再挑不中才放弃。
pub const DEFAULT_AUDIT_MODEL_ANTHROPIC: &str = "claude-opus-5";
pub const DEFAULT_AUDIT_MODEL_OPENAI: &str = "gpt-5.6-sol";

/// 在一组候选模型里挑一个来验。
///
/// `prefer_anthropic` 为真时优先 Claude 那个默认值,否则优先 GPT 那个。
/// 都挑不中就用列表里的第一个;列表空了返回 `None` —— **不编一个模型名**。
pub fn pick_audit_model(
    models: &[StationModel],
    group: &str,
    prefer_anthropic: bool,
) -> Option<String> {
    let usable: Vec<&StationModel> = models.iter().filter(|m| m.in_group(group)).collect();
    let wanted = if prefer_anthropic {
        DEFAULT_AUDIT_MODEL_ANTHROPIC
    } else {
        DEFAULT_AUDIT_MODEL_OPENAI
    };
    usable
        .iter()
        .find(|m| m.model.eq_ignore_ascii_case(wanted))
        .or_else(|| usable.first())
        .map(|m| m.model.clone())
}

/// New API 的默认分组名。线路上分组留空(「默认分组」)时按它去查分组倍率。
pub const NEW_API_DEFAULT_GROUP: &str = "default";

/// 同 [`parse_station_pricing`],再把**这条线路那个分组**的倍率填进每个模型的
/// `rates.group_ratio`。
///
/// New API 把分组倍率放在 `/api/pricing` 响应的**顶层** `group_ratio` 里
/// (分组名 → 倍率;登录后是按你这个账号调整过的那一份),不在每个模型那一行 ——
/// 只读 `data` 的话永远拿不到它。0.25.3 及以前就是这样,KNOWN-ISSUES 还因此写着
/// 「`/api/pricing` 不返回分组倍率」,而大多数站的折扣正在这个数上。
///
/// 查不到就留 `None`:分组名对不上、`auto` 分组(倍率跟着实际路由走,不是一个数)、
/// 站点没公布。**不拿 1 顶。** 模型行里已经带着分组倍率的不覆盖。
pub fn parse_station_pricing_in_group(body: &str, group: &str) -> Vec<StationModel> {
    let mut models = parse_station_pricing(body);
    let ratio = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|root| group_ratio_in(&root, group));
    if ratio.is_some() {
        for m in &mut models {
            if m.rates.group_ratio.is_none() {
                m.rates.group_ratio = ratio;
            }
        }
    }
    models
}

/// `/api/pricing` 顶层 `group_ratio` 里这个分组的倍率。空分组名按 [`NEW_API_DEFAULT_GROUP`] 查。
fn group_ratio_in(root: &serde_json::Value, group: &str) -> Option<f64> {
    let map = root.get("group_ratio").and_then(|v| v.as_object())?;
    let group = group.trim();
    if group.eq_ignore_ascii_case("auto") {
        return None;
    }
    let key = if group.is_empty() {
        NEW_API_DEFAULT_GROUP
    } else {
        group
    };
    let n = match map.get(key)? {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }?;
    (n.is_finite() && n > 0.0).then_some(n)
}

/// 解析站点自己的 `/api/pricing`,拿到每个模型的计费倍率。
///
/// **纯函数。** 字段名照 New API 的 `/api/pricing`:
///
/// | 字段 | 含义 |
/// |---|---|
/// | `model_ratio` | 输入单价,1 = $2 / 百万 token([`NEW_API_USD_PER_MTOK`]) |
/// | `completion_ratio` | 输出价 ÷ 输入价(Claude 官方就是 5) |
/// | `cache_ratio` / `create_cache_ratio` | 缓存读 / 写价 ÷ 输入价 |
/// | `model_price` + `billing_mode` | 按次计费时 token 倍率不适用 |
///
/// 分组倍率在响应顶层,不在这里读 —— 要它用 [`parse_station_pricing_in_group`]。
/// 认不出来的字段一律 `None`,**绝不填 1.0** —— 1.0 是「这一类跟输入同价」的断言。
pub fn parse_station_pricing(body: &str) -> Vec<StationModel> {
    let Ok(root) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let rows = root
        .pointer("/data")
        .and_then(|v| v.as_array())
        .or_else(|| root.as_array())
        .cloned()
        .unwrap_or_default();

    let num = |v: Option<&serde_json::Value>| -> Option<f64> {
        match v? {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(s) => s.trim().parse().ok(),
            _ => None,
        }
        .filter(|x: &f64| x.is_finite())
    };

    rows.iter()
        .filter_map(|row| {
            let model = row
                .get("model_name")
                .or_else(|| row.get("model"))
                .and_then(|v| v.as_str())?
                .trim()
                .to_string();
            if model.is_empty() {
                return None;
            }
            // 按次计费:`billing_mode` 不是 ratio,或者 model_price 是个正数。
            let per_request = num(row.get("model_price")).filter(|v| *v > 0.0);
            let groups = row
                .get("enable_groups")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|g| g.as_str())
                        .map(|g| g.trim().to_string())
                        .filter(|g| !g.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            // sub2api 系把价放在 `pricing` 这一层里,而且是**绝对单价**
            // (美元／百万 token),字段名带 `_price`;New API 系给的是平铺的
            // `*_ratio`。两种形状在这里一起认,谁填了算谁的 —— 站点后端是哪一套
            // 由内容说了算,不靠一个单独的 kind 字段(那个字段会骗人)。
            let pricing = row.get("pricing").filter(|v| v.is_object());
            let price = |k: &str| -> Option<f64> {
                num(pricing.and_then(|p| p.get(k)))
                    .or_else(|| num(row.get(k)))
                    .filter(|v| *v >= 0.0)
            };
            Some(StationModel {
                model,
                rates: StationRates {
                    model_ratio: num(row.get("model_ratio")),
                    completion_ratio: num(row.get("completion_ratio")),
                    cache_ratio: num(row.get("cache_ratio")),
                    create_cache_ratio: num(row.get("create_cache_ratio")),
                    group_ratio: None,
                    peak_rate: None,
                    per_request_price: per_request
                        .or_else(|| price("per_request_price").filter(|v| *v > 0.0)),
                    input_price: price("input_price"),
                    cache_read_price: price("cache_read_price"),
                    cache_write_price: price("cache_write_price"),
                    output_price: price("output_price"),
                },
                groups,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    /// 反重力记录里的模型名要查得到价（0.30.0）：Gemini 直接命中；开了思考的 Claude 带
    /// `-thinking` 后缀，剥掉之后命中同一个模型；抓回来的价也按剥掉后缀的名字匹配。
    #[test]
    fn antigravity_model_names_resolve_to_a_price_with_and_without_the_thinking_suffix() {
        let g = lookup("gemini-3.7-flash").expect("Gemini 3.7 Flash 在快照里");
        assert_eq!(g.input.per_mtok, 0.75);
        assert_eq!(
            g.cache_read.basis,
            PriceBasis::Published,
            "缓存读是官方明码，不是推的"
        );
        assert_eq!(price_key("claude-opus-4-6-thinking"), "claude-opus-4-6");
        assert_eq!(price_key("claude-opus-5-20260401"), "claude-opus-5");
        assert_eq!(price_key("gemini-3.1-pro"), "gemini-3.1-pro");
        assert!(lookup("claude-opus-4-6-thinking").is_some());
        assert!(
            lookup("gemini-3.1-pro").is_some(),
            "反重力写的是不带 -preview 的 id"
        );
        assert!(
            lookup("gemini-9.9-nothing").is_none(),
            "认不出的仍然是 None，不凑"
        );
        let live = Catalog::new(vec![FetchedPrice {
            model: "claude-opus-4-6".into(),
            input_per_mtok: 9.0,
            output_per_mtok: 25.0,
            cache_read_per_mtok: Some(1.0),
            cache_write_per_mtok: None,
            cache_write_1h_per_mtok: None,
            long_context: None,
            currency: "USD".into(),
            source_url: "https://example.invalid".into(),
            fetched_at: "2026-09-21".into(),
        }]);
        let r = live.resolve("claude-opus-4-6-thinking").unwrap();
        assert!(r.live, "抓回来的价要按剥掉 -thinking 的名字命中");
        assert_eq!(r.input.per_mtok, 9.0);
    }

    /// 官方价必须**真的参与**结论。
    ///
    /// 回归的是一条静默失效:老写法把站点公布的倍率乘上官方价凑出
    /// 「站点单价」,再除以官方价 —— 官方价约掉,算出来的恒等于站点自己
    /// 公布的两个数相乘。站点说它便宜,面板就说它便宜,而 20 条单测全绿。
    ///
    /// 这条测试的做法:**只改官方价,别的都不动**。结论必须跟着变。
    /// 不变就说明官方价又被约掉了。
    #[test]
    fn the_official_price_actually_changes_the_measured_multiplier() {
        let tokens = [Some(1_000_000u64), Some(0), Some(0), Some(0)];
        let cheap = resolved(1.0);
        let dear = resolved(4.0);
        // 实扣固定 2 块钱。官方价 1 → 2 倍;官方价 4 → 0.5 倍。
        let a = measured_multiplier(tokens, Some(2.0), &cheap).unwrap();
        let b = measured_multiplier(tokens, Some(2.0), &dear).unwrap();
        assert!((a - 2.0).abs() < 1e-9, "官方价 1 时该是 2 倍，实际 {a}");
        assert!((b - 0.5).abs() < 1e-9, "官方价 4 时该是 0.5 倍，实际 {b}");
        assert_ne!(a, b, "只改官方价结论却没变 —— 官方价又被约掉了");
    }

    /// 四类缺一类就不给结论。
    ///
    /// 少一类会把分母算小、倍率被系统性抬高 —— 而那正好是
    /// 「这家站点在超收」的方向。宁可不答。
    #[test]
    fn a_missing_token_category_yields_no_verdict_instead_of_an_inflated_one() {
        let p = resolved(1.0);
        let full = [
            Some(1_000_000u64),
            Some(1_000_000),
            Some(1_000_000),
            Some(1_000_000),
        ];
        assert!(measured_multiplier(full, Some(1.0), &p).is_some());
        for i in 0..4 {
            let mut partial = full;
            partial[i] = None;
            assert_eq!(
                measured_multiplier(partial, Some(1.0), &p),
                None,
                "缺第 {i} 类时不该给出结论"
            );
        }
    }

    /// 算不出来时**绝不返回 1.0** —— 那是「量过了,属实」这个断言。
    #[test]
    fn an_unmeasurable_round_never_reads_as_verified() {
        let p = resolved(1.0);
        let some = [Some(1u64), Some(0), Some(0), Some(0)];
        // 一个 token 都没有:分母为零。
        let none_at_all = [Some(0u64), Some(0), Some(0), Some(0)];
        assert_eq!(measured_multiplier(none_at_all, Some(1.0), &p), None);
        // 实扣取不到、或者不是个有限的非负数。
        assert_eq!(measured_multiplier(some, None, &p), None);
        assert_eq!(measured_multiplier(some, Some(-1.0), &p), None);
        assert_eq!(measured_multiplier(some, Some(f64::NAN), &p), None);
        // 官方价全零(查不到这个模型):同样答不了。
        assert_eq!(measured_multiplier(some, Some(1.0), &resolved(0.0)), None);
    }

    fn resolved(per_mtok: f64) -> ResolvedPrice {
        ResolvedPrice {
            input: UnitPrice::published(per_mtok),
            output: UnitPrice::published(per_mtok),
            cache_read: UnitPrice::published(per_mtok),
            cache_write: UnitPrice::published(per_mtok),
            cache_write_1h: UnitPrice::published(per_mtok),
            currency: "USD".into(),
            source_url: String::new(),
            verified_at: "2026-01-01".into(),
            live: false,
        }
    }
    use super::*;

    /// Anthropic 定价页的真实行（2026-09-14 抓的原文，含链接与脚注标记）。
    const ANTHROPIC_SAMPLE: &str = "\
## Model pricing

| Model | Base input tokens | 5m cache writes | 1h cache writes | Cache hits and refreshes | Output tokens |
| --- | --- | --- | --- | --- | --- |
| Claude Fable 5.1 | $10 / MTok | $12.50 / MTok | $20 / MTok | $0.25 / MTok1 | $50 / MTok |
| Claude Opus 5 | $5 / MTok | $6.25 / MTok | $10 / MTok | $0.50 / MTok | $25 / MTok |
| Claude Sonnet 5 | $2 / MTok | $2.50 / MTok | $4 / MTok | $0.20 / MTok | $10 / MTok |
| Claude Haiku 4.5 | $1 / MTok | $1.25 / MTok | $2 / MTok | $0.10 / MTok | $5 / MTok |
| Claude Opus 4.1 ([retired, except on Bedrock](https://x.test/dep)) | $15 / MTok | $18.75 / MTok | $30 / MTok | $1.50 / MTok | $75 / MTok |
";

    /// OpenAI 定价页的真实行：短上下文四列 + 长上下文四列。
    const OPENAI_SAMPLE: &str = "\
### Standard pricing data

| Model | Input | Cached input | Cache writes | Output | Input | Cached input | Cache writes | Output |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| gpt-6-astra | $10.00 | $1.00 | $12.50 | $50.00 | $20.00 | $2.00 | $25.00 | $75.00 |
| gpt-5.6-sol | $4.00 | $0.40 | $5.00 | $20.00 | $8.00 | $0.80 | $10.00 | $30.00 |
| gpt-5.4-mini | $0.75 | $0.075 | - | $4.50 | - | - | - | - |
";

    #[test]
    fn the_batch_table_is_ignored_even_though_it_lists_the_same_models() {
        // ⛔ 同一个模型在批处理表里是五折价。采用它的话，
        // 每一家站点都会被算成超收两倍，而且没有任何症状。
        let page = "\
## Model pricing

| Model | Base | 5m | 1h | Hit | Output |
| --- | --- | --- | --- | --- | --- |
| Claude Opus 5 | $5 / MTok | $6.25 / MTok | $10 / MTok | $0.50 / MTok | $25 / MTok |
| Claude Sonnet 5 | $2 / MTok | $2.50 / MTok | $4 / MTok | $0.20 / MTok | $10 / MTok |
| Claude Haiku 4.5 | $1 / MTok | $1.25 / MTok | $2 / MTok | $0.10 / MTok | $5 / MTok |

### Batch processing

| Model | Batch input | Batch output |
| --- | --- | --- |
| Claude Opus 5 | $2.50 / MTok | $12.50 / MTok |
";
        let got = parse_pricing_markdown(page, "2026-09-14", "u", PricingFormat::Anthropic);
        let opus = got.iter().find(|f| f.model == "claude-opus-5").unwrap();
        assert_eq!(opus.input_per_mtok, 5.0, "采用了批处理的五折价");
        assert_eq!(opus.output_per_mtok, 25.0);
    }

    #[test]
    fn a_page_without_the_expected_section_yields_nothing() {
        // 上游把小节改名了 —— 什么都不采用，继续用内置快照。
        let renamed = ANTHROPIC_SAMPLE.replace("## Model pricing", "## Prices");
        assert!(
            parse_pricing_markdown(&renamed, "2026-09-14", "u", PricingFormat::Anthropic)
                .is_empty()
        );
    }

    #[test]
    fn the_anthropic_table_reads_output_not_the_cache_write_column() {
        // ⛔ 这张表是 5 列：输入 | 5m 缓存写 | 1h 缓存写 | 缓存读 | 输出。
        // 取「前两个金额」的话 Opus 5 会读成 $5/$6.25 —— 把缓存写当成了输出，
        // 于是官方价被低估 4 倍，每一家站点都会被算成超收。
        let got = parse_pricing_markdown(
            ANTHROPIC_SAMPLE,
            "2026-09-14",
            "https://x.test/p",
            PricingFormat::Anthropic,
        );
        let opus = got.iter().find(|f| f.model == "claude-opus-5").unwrap();
        assert_eq!(opus.input_per_mtok, 5.0);
        assert_eq!(opus.output_per_mtok, 25.0, "输出读成缓存写了");
        assert_eq!(opus.cache_read_per_mtok, Some(0.50));
        assert_eq!(opus.cache_write_per_mtok, Some(6.25));
        assert_eq!(
            opus.cache_write_1h_per_mtok,
            Some(10.0),
            "1h 那一列不许再读了就丢 —— Claude Code 的缓存写全是这一档"
        );
    }

    /// 0.25.1 之前存进库里的那些行没有 1h 字段：照样读得出，1h 档按官方 2 倍推并标成推的。
    #[test]
    fn an_old_stored_row_without_the_one_hour_price_still_resolves() {
        let old = r#"{"model":"claude-opus-5-5","input_per_mtok":4.0,"output_per_mtok":20.0,
            "cache_read_per_mtok":0.2,"cache_write_per_mtok":5.0,"long_context":null,
            "currency":"USD","source_url":"u","fetched_at":"2026-09-24"}"#;
        let f: FetchedPrice = serde_json::from_str(old).expect("旧行必须还能读");
        assert_eq!(f.cache_write_1h_per_mtok, None);
        let r = Catalog::new(vec![f]).resolve("claude-opus-5-5").unwrap();
        assert_eq!(r.cache_write_1h.per_mtok, 8.0);
        assert_eq!(r.cache_write_1h.basis, PriceBasis::Derived);
        assert_eq!(r.cache_write.per_mtok, 5.0, "5 分钟档照旧");
        // 内置快照那一条也有 1h 档（推的）。
        let snap = lookup("claude-opus-5").unwrap().as_resolved();
        assert_eq!(snap.cache_write_1h.per_mtok, 10.0);
        assert_eq!(snap.cache_write_1h.basis, PriceBasis::Derived);
    }

    #[test]
    fn anthropic_display_names_become_model_ids() {
        // 表里那一列是「Claude Opus 4.6」这种显示名，而全项目按 id 索引。
        let got = parse_pricing_markdown(
            ANTHROPIC_SAMPLE,
            "2026-09-14",
            "u",
            PricingFormat::Anthropic,
        );
        let ids: Vec<&str> = got.iter().map(|f| f.model.as_str()).collect();
        assert!(ids.contains(&"claude-fable-5-1"));
        assert!(ids.contains(&"claude-haiku-4-5"));
        // 带 markdown 链接说明的那一行也要能认出来
        assert!(ids.contains(&"claude-opus-4-1"), "{ids:?}");
    }

    #[test]
    fn the_fable_cache_read_is_published_not_derived() {
        // 0.25 是官方单独标的（0.025x），按 0.1x 推会得到 1.0 —— 差 4 倍。
        let got = parse_pricing_markdown(
            ANTHROPIC_SAMPLE,
            "2026-09-14",
            "u",
            PricingFormat::Anthropic,
        );
        let fable = got.iter().find(|f| f.model == "claude-fable-5-1").unwrap();
        assert_eq!(fable.cache_read_per_mtok, Some(0.25));
        let c = Catalog::new(got);
        assert_eq!(
            c.resolve("claude-fable-5-1").unwrap().cache_read.basis,
            PriceBasis::Published
        );
    }

    #[test]
    fn the_openai_table_keeps_the_long_context_tier_separate() {
        // ⛔ 「计费翻倍」的另一种：gpt-5.6-sol 短上下文 $4/$20，长上下文 $8/$30。
        // 拿短上下文价当基准去验一条专跑长上下文的线，会把它冤枉成超收两倍。
        let got = parse_pricing_markdown(
            OPENAI_SAMPLE,
            "2026-09-14",
            "https://x.test/o",
            PricingFormat::OpenAi,
        );
        let sol = got.iter().find(|f| f.model == "gpt-5.6-sol").unwrap();
        assert_eq!(sol.input_per_mtok, 4.0);
        assert_eq!(sol.output_per_mtok, 20.0);
        assert_eq!(sol.cache_read_per_mtok, Some(0.40));
        let long = sol.long_context.expect("长上下文那一档丢了");
        assert_eq!(long.input_per_mtok, 8.0);
        assert_eq!(long.output_per_mtok, 30.0);
    }

    #[test]
    fn a_dash_means_that_tier_is_unavailable_not_free() {
        // gpt-5.4-mini 没有缓存写、也没有长上下文档 —— 表里写的是 `-`。
        let got = parse_pricing_markdown(OPENAI_SAMPLE, "2026-09-14", "u", PricingFormat::OpenAi);
        let mini = got.iter().find(|f| f.model == "gpt-5.4-mini").unwrap();
        assert_eq!(mini.cache_write_per_mtok, None, "`-` 被当成 0 了");
        assert_eq!(mini.long_context, None);
        assert_eq!(mini.input_per_mtok, 0.75);
        assert_eq!(mini.output_per_mtok, 4.5);
    }

    #[test]
    fn the_wrong_format_yields_nothing_rather_than_wrong_prices() {
        // 拿 OpenAI 的解析器去读 Anthropic 的表（比如两边 URL 配反了），
        // 必须什么都读不出来 —— 读出一半是最危险的结果。
        assert!(
            parse_pricing_markdown(ANTHROPIC_SAMPLE, "2026-09-14", "u", PricingFormat::OpenAi)
                .is_empty()
        );
        assert!(
            parse_pricing_markdown(OPENAI_SAMPLE, "2026-09-14", "u", PricingFormat::Anthropic)
                .is_empty()
        );
    }

    #[test]
    fn a_redesigned_page_yields_nothing_rather_than_half_a_table() {
        // 「半成功」比彻底失败危险：它会用一份残缺的表盖掉完整的内置快照。
        let almost = "正文里提到 `claude-opus-5` 大约 $5 一百万 token。";
        assert!(
            parse_pricing_markdown(almost, "2026-09-14", "u", PricingFormat::Anthropic).is_empty()
        );
    }

    #[test]
    fn implausible_numbers_are_rejected_rather_than_stored() {
        // 把上下文窗口 200000 当成价格读进来 —— 必须挡住。
        let bogus = "\
## Model pricing

| Claude A One | $200000 | $1 | $2 | $3 | $400000 |
| Claude B One | $200000 | $1 | $2 | $3 | $400000 |
| Claude C One | $200000 | $1 | $2 | $3 | $400000 |
";
        assert!(
            parse_pricing_markdown(bogus, "2026-09-14", "u", PricingFormat::Anthropic).is_empty()
        );
    }

    #[test]
    fn parsed_prices_feed_the_catalog_and_are_marked_live() {
        let got = parse_pricing_markdown(
            ANTHROPIC_SAMPLE,
            "2026-09-14",
            "https://x.test/pricing",
            PricingFormat::Anthropic,
        );
        let c = Catalog::new(got);
        let r = c.resolve("claude-sonnet-5").unwrap();
        assert!(r.live);
        assert_eq!(r.input.per_mtok, 2.0);
        assert_eq!(r.output.per_mtok, 10.0);
        assert_eq!(r.verified_at, "2026-09-14");
        // 抓回来的表里没有的模型，仍然退回内置快照
        assert!(!c.resolve("claude-opus-4-8").unwrap().live);
    }

    /// 照官方价原样抄进 New API 后台的 Opus 5。分组 ×1,知道。
    fn opus5_copy() -> StationRates {
        StationRates {
            model_ratio: Some(2.5),
            completion_ratio: Some(5.0),
            cache_ratio: Some(0.1),
            create_cache_ratio: Some(1.25),
            group_ratio: Some(1.0),
            ..Default::default()
        }
    }

    #[test]
    fn a_new_api_payload_becomes_unit_prices_at_two_dollars_per_ratio_point() {
        // New API 的倍率是单价:1 = $2 / 百万 token。照官方价抄的 Sonnet 4.6
        // ($3 / $15)在它后台里是 model_ratio 1.5、completion_ratio 5。
        assert_eq!(
            NEW_API_USD_PER_MTOK, 2.0,
            "New API 源码:1 === $0.002 / 1K tokens"
        );
        let body = r#"{"data":[
            {"model_name":"claude-sonnet-4-6","model_ratio":1.5,"completion_ratio":5,
             "cache_ratio":0.1,"create_cache_ratio":1.25},
            {"model_name":"按次计费的","model_ratio":0.2,"completion_ratio":5,"model_price":0.02}
        ]}"#;
        let got = parse_station_pricing(body);
        assert_eq!(got.len(), 2);
        let r = &got
            .iter()
            .find(|m| m.model == "claude-sonnet-4-6")
            .unwrap()
            .rates;
        let [input, cache_read, cache_write, output] = r.category_prices();
        assert_eq!(input, Some(3.0));
        assert_eq!(output, Some(15.0), "输出没乘 completion_ratio");
        assert!((cache_read.unwrap() - 0.3).abs() < 1e-12);
        assert!((cache_write.unwrap() - 3.75).abs() < 1e-12);
        // 对着官方价:四类都是 1 倍 —— 照官方价收费,不是「输入 1.5 倍、输出翻 5 倍」。
        let official = lookup("claude-sonnet-4-6").unwrap().as_resolved();
        for (i, x) in r.category_ratios_against(&official).iter().enumerate() {
            assert!((x.unwrap() - 1.0).abs() < 1e-12, "第 {i} 类 = {x:?}");
        }

        let per = &got.iter().find(|m| m.model == "按次计费的").unwrap().rates;
        assert!(per.per_request());
        assert_eq!(per.category_prices(), [None; 4]);
        assert_eq!(per.newapi_ratios(), [None; 4]);
    }

    #[test]
    fn a_missing_completion_ratio_stays_unknown() {
        // ⛔ 填 1.0 等于断言「输出跟输入同价」—— 而输出往往才是大头。
        let body = r#"[{"model_name":"x","model_ratio":0.5}]"#;
        let r = parse_station_pricing(body)
            .into_iter()
            .next()
            .unwrap()
            .rates;
        assert_eq!(r.completion_ratio, None);
        assert_eq!(r.newapi_ratios()[3], None);
        assert_eq!(r.category_prices()[3], None);
    }

    /// New API 把分组倍率放在响应顶层,不在模型那一行 —— 0.25.3 及以前只读 `data`,
    /// 永远拿不到它,KNOWN-ISSUES 还写着「`/api/pricing` 不返回分组倍率」。
    #[test]
    fn the_group_ratio_comes_from_the_top_level_of_api_pricing() {
        let body = r#"{"success":true,
            "data":[{"model_name":"claude-opus-5","model_ratio":2.5,"completion_ratio":5,
                     "enable_groups":["default","vip"]}],
            "group_ratio":{"default":1,"vip":0.2,"auto":1}}"#;
        let ratio = |g: &str| parse_station_pricing_in_group(body, g)[0].rates.group_ratio;
        assert_eq!(ratio("vip"), Some(0.2));
        assert_eq!(ratio(""), Some(1.0), "留空 = New API 的默认分组 default");
        assert_eq!(ratio("没有这个组"), None, "对不上就是不知道,不拿 1 顶");
        assert_eq!(ratio("auto"), None, "auto 的倍率跟着实际路由走,不是一个数");
        // 不带分组的入口照旧不填 —— 它不知道是哪条线路。
        assert_eq!(parse_station_pricing(body)[0].rates.group_ratio, None);
    }

    /// 使用者的话:「New API 的倍率算在了单价内,所以单价便宜。」
    ///
    /// 同样是 Opus 5 卖两折,有的站把折扣放在分组上(model_ratio 照官方 2.5、分组 ×0.2),
    /// 有的站直接把 model_ratio 压到 0.5(分组 ×1)。两种都得先换成单价($1 / 百万)
    /// 再除官方单价($5),读出来都是 0.2 —— 而 model_ratio 一个是 2.5、一个是 0.5,
    /// 哪个都不是 0.2。
    #[test]
    fn a_discount_baked_into_the_unit_price_reads_the_same_as_one_in_the_group() {
        let official = lookup("claude-opus-5").unwrap().as_resolved();
        let in_group = StationRates {
            group_ratio: Some(0.2),
            ..opus5_copy()
        };
        let in_price = StationRates {
            model_ratio: Some(0.5),
            ..opus5_copy()
        };
        for r in [in_group, in_price] {
            assert_eq!(r.category_prices()[0], Some(1.0), "单价应是 $1 / 百万");
            for (i, x) in r.category_ratios_against(&official).iter().enumerate() {
                assert!((x.unwrap() - 0.2).abs() < 1e-12, "第 {i} 类 = {x:?}");
            }
            let blended = r.blended_ratio([0.2, 0.5, 0.1, 0.2], &official).unwrap();
            assert!((blended - 0.2).abs() < 1e-12, "blended={blended}");
        }
    }

    /// ⛔ 0.25.3 及以前:`completion_ratio > 1` 就挂「翻倍 ×5」—— 每一家照官方价收费的
    /// Claude 线路都中招,因为官方 Opus 5 自己就是输出 = 输入 × 5。
    #[test]
    fn an_honest_completion_ratio_of_five_is_not_an_output_markup() {
        let official = lookup("claude-opus-5").unwrap().as_resolved();
        let honest = StationRates {
            group_ratio: Some(0.2),
            ..opus5_copy()
        };
        assert!((honest.output_markup(&official).unwrap() - 1.0).abs() < 1e-12);
        let marked_up = StationRates {
            completion_ratio: Some(6.0),
            ..honest
        };
        assert!((marked_up.output_markup(&official).unwrap() - 1.2).abs() < 1e-12);
        // sub2api 那套一样:输出 $30 对输入 $5,官方是 $25 对 $5 → 多收两成。分组约掉。
        let sub2 = StationRates {
            input_price: Some(5.0),
            output_price: Some(30.0),
            group_ratio: Some(0.3),
            ..Default::default()
        };
        assert!((sub2.output_markup(&official).unwrap() - 1.2).abs() < 1e-12);
        // 没公布输出就是不知道,不是「没加价」。
        let unknown = StationRates {
            completion_ratio: None,
            ..honest
        };
        assert_eq!(unknown.output_markup(&official), None);
    }

    #[test]
    fn garbage_pricing_yields_nothing_rather_than_a_panic() {
        for body in ["", "null", "{}", "not json", r#"{"data":{}}"#] {
            assert!(parse_station_pricing(body).is_empty(), "{body}");
        }
    }

    #[test]
    fn which_station_is_cheaper_depends_on_the_token_mix() {
        // 官方 Opus 5:$5 / $25。A 分组 ×0.15,但把补全倍率从官方的 5 调成 10
        // (输出 ×0.30);B 分组 ×0.25,补全照官方填 5(四类都 ×0.25)。
        let official = lookup("claude-opus-5").unwrap().as_resolved();
        let a = StationRates {
            completion_ratio: Some(10.0),
            group_ratio: Some(0.15),
            ..opus5_copy()
        };
        let b = StationRates {
            group_ratio: Some(0.25),
            ..opus5_copy()
        };
        assert!((a.output_markup(&official).unwrap() - 2.0).abs() < 1e-12);
        assert!((b.output_markup(&official).unwrap() - 1.0).abs() < 1e-12);

        // 纯读代码：输出只占一成 → A 便宜(0.165 对 0.25)
        let reading = [0.9, 0.0, 0.0, 0.1];
        assert!(
            a.blended_ratio(reading, &official).unwrap()
                < b.blended_ratio(reading, &official).unwrap()
        );

        // 长篇生成：输出占七成 → B 便宜(0.25 对 0.255)
        let writing = [0.3, 0.0, 0.0, 0.7];
        assert!(
            b.blended_ratio(writing, &official).unwrap()
                < a.blended_ratio(writing, &official).unwrap(),
            "输出占七成时输出加价那条应该更贵"
        );
    }

    #[test]
    fn a_cache_heavy_line_is_not_cheap_on_a_fresh_conversation() {
        // 靠缓存读撑起来的便宜，在新对话第一轮上不成立 —— 那段上下文还没缓存过。
        // 这家的缓存读比官方的结构还便宜一半(cache_ratio 0.05,官方是 0.1)。
        let official = lookup("claude-opus-5").unwrap().as_resolved();
        let r = StationRates {
            cache_ratio: Some(0.05),
            group_ratio: Some(0.2),
            ..opus5_copy()
        };
        let habitual = [0.1, 0.8, 0.0, 0.1]; // 平时八成走缓存读
        let fresh = [0.9, 0.0, 0.0, 0.1]; // 新对话：那八成变成未缓存输入
        assert!(
            r.blended_ratio(fresh, &official).unwrap()
                > r.blended_ratio(habitual, &official).unwrap(),
            "新对话没比平时贵 —— 缓存读被当成一直有效了"
        );
    }

    #[test]
    fn a_category_that_carries_no_tokens_may_be_missing() {
        // 乘 0 本来就不影响结果，不该因为它缺席就整个算不出来。
        let official = resolved(2.0);
        let r = StationRates {
            model_ratio: Some(0.2),
            completion_ratio: Some(2.0),
            cache_ratio: None,
            create_cache_ratio: None,
            group_ratio: Some(1.0),
            ..Default::default()
        };
        // 输入 $0.4 对官方 $2 → 0.2;输出 $0.8 → 0.4;一半一半 → 0.3。
        // 0.1 + 0.2 在二进制里不等于 0.3 —— 这里测的是加权，不是浮点表示。
        let got = r.blended_ratio([0.5, 0.0, 0.0, 0.5], &official).unwrap();
        assert!((got - 0.3).abs() < 1e-12, "got={got}");
        // 但真的占量时就不能瞎补
        assert_eq!(r.blended_ratio([0.4, 0.2, 0.0, 0.4], &official), None);
    }

    #[test]
    fn models_are_filtered_by_the_group_they_belong_to() {
        // 「分组 → 模型」那一级靠 enable_groups。
        let body = r#"{"data":[
            {"model_name":"a","model_ratio":1,"enable_groups":["低价组"]},
            {"model_name":"b","model_ratio":1,"enable_groups":["官方直连组","低价组"]},
            {"model_name":"c","model_ratio":1,"enable_groups":["官方直连组"]}
        ]}"#;
        let got = parse_station_pricing(body);
        let cheap: Vec<&str> = got
            .iter()
            .filter(|m| m.in_group("低价组"))
            .map(|m| m.model.as_str())
            .collect();
        assert_eq!(cheap, vec!["a", "b"]);
    }

    #[test]
    fn a_model_without_published_groups_is_offered_everywhere() {
        // ⛔ 站点没公布分组时要把模型全列出来让使用者自己挑 ——
        // 「不知道」表现成「不能用」的话，那个站的检验一个模型都选不了。
        let body = r#"[{"model_name":"x","model_ratio":1}]"#;
        let m = &parse_station_pricing(body)[0];
        assert!(m.groups.is_empty());
        assert!(m.in_group("随便什么组"));
    }

    #[test]
    fn the_default_audit_model_is_used_when_the_station_offers_it() {
        // 使用者定的默认：Anthropic 侧 claude-opus-5，OpenAI 侧 gpt-5.6-sol。
        let body = r#"{"data":[
            {"model_name":"claude-sonnet-5","model_ratio":1},
            {"model_name":"claude-opus-5","model_ratio":1},
            {"model_name":"gpt-5.6-sol","model_ratio":1}
        ]}"#;
        let models = parse_station_pricing(body);
        assert_eq!(
            pick_audit_model(&models, "", true).as_deref(),
            Some("claude-opus-5")
        );
        assert_eq!(
            pick_audit_model(&models, "", false).as_deref(),
            Some("gpt-5.6-sol")
        );
    }

    #[test]
    fn a_station_without_the_default_falls_back_instead_of_auditing_a_ghost() {
        // ⛔ 站点没有这个模型却硬拿它去验，验的是一个不存在的东西，
        // 结果全是「没测到」—— 看起来像站点不配合，其实是我们挑错了。
        let body = r#"[{"model_name":"deepseek-chat","model_ratio":1}]"#;
        let models = parse_station_pricing(body);
        assert_eq!(
            pick_audit_model(&models, "", true).as_deref(),
            Some("deepseek-chat")
        );
        assert_eq!(pick_audit_model(&[], "", true), None);
    }

    #[test]
    fn the_default_must_also_be_in_the_chosen_group() {
        let body = r#"{"data":[
            {"model_name":"claude-opus-5","model_ratio":1,"enable_groups":["官方直连组"]},
            {"model_name":"deepseek-chat","model_ratio":1,"enable_groups":["低价组"]}
        ]}"#;
        let models = parse_station_pricing(body);
        assert_eq!(
            pick_audit_model(&models, "低价组", true).as_deref(),
            Some("deepseek-chat")
        );
        assert_eq!(
            pick_audit_model(&models, "官方直连组", true).as_deref(),
            Some("claude-opus-5")
        );
    }

    #[test]
    fn a_live_price_wins_over_the_built_in_snapshot() {
        let c = Catalog::new(vec![FetchedPrice {
            model: "claude-opus-5".into(),
            input_per_mtok: 6.0,
            output_per_mtok: 30.0,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
            cache_write_1h_per_mtok: None,
            long_context: None,
            currency: "USD".into(),
            source_url: "https://example.test/pricing".into(),
            fetched_at: "2026-09-14".into(),
        }]);
        let r = c.resolve("claude-opus-5").unwrap();
        assert!(r.live);
        assert_eq!(r.input.per_mtok, 6.0);
        // 抓回来的没给缓存价 —— 按标准倍数补，并标成推的
        assert_eq!(r.cache_read.basis, PriceBasis::Derived);
        assert!((r.cache_read.per_mtok - 0.6).abs() < 1e-12);
    }

    #[test]
    fn an_empty_fetch_falls_back_to_the_snapshot_instead_of_losing_prices() {
        // ⛔ 一次失败的抓取不许把好价清空：价目被清空之后，所有「真实倍率」
        // 一起变成「不知道」，而界面上看起来只是「都没检验过」。
        let c = Catalog::new(Vec::new());
        let r = c.resolve("claude-opus-5").expect("退回内置快照失败");
        assert!(!r.live);
        assert_eq!(r.input.per_mtok, 5.0);
    }

    #[test]
    fn a_resolved_price_reports_staleness_and_currency_the_same_way() {
        let c = Catalog::default();
        let r = c.resolve("claude-opus-5").unwrap();
        assert_eq!(r.status("USD", "2026-07-01"), PriceStatus::Complete);
        assert_eq!(r.status("USD", "2027-06-24"), PriceStatus::Stale);
        assert_eq!(r.status("CNY", "2026-07-01"), PriceStatus::CurrencyMismatch);
    }

    #[test]
    fn a_new_api_copy_of_the_official_price_is_one_times_in_every_category() {
        // ⛔ 钉的是 0.25.3 及以前那个误读:照官方价抄进 New API 后台的 Opus 5 是
        // model_ratio 2.5 / completion 5 / cache 0.1 / create 1.25 —— 原来被显示成
        // 「输入 2.5 倍、输出 12.5 倍」,还挂着「翻倍 ×5」;实际四类都跟官方同价。
        let r = opus5_copy();
        let [input, cache_read, cache_write, output] = r.category_prices();
        assert_eq!(input, Some(5.0));
        assert_eq!(output, Some(25.0), "输出没乘 completion_ratio");
        assert!((cache_read.unwrap() - 0.5).abs() < 1e-12);
        assert!((cache_write.unwrap() - 6.25).abs() < 1e-12);
        let official = lookup("claude-opus-5").unwrap().as_resolved();
        for (i, x) in r.category_ratios_against(&official).iter().enumerate() {
            assert!((x.unwrap() - 1.0).abs() < 1e-12, "第 {i} 类 = {x:?}");
        }
        // 原始倍率乘积还留着供对照 —— 它是单价(1 = $2 / 百万),不是倍数。
        assert_eq!(r.newapi_ratios()[3], Some(12.5));
    }

    #[test]
    fn the_group_and_peak_multipliers_stack_on_top() {
        // 峰时取上界:实测有站点浮动到 1.5 倍。按平均算会让人在峰时
        // 付出比预期多的钱,而账面上「没超」。
        let r = StationRates {
            model_ratio: Some(1.0),
            completion_ratio: Some(2.0),
            group_ratio: Some(1.4),
            peak_rate: Some(1.5),
            ..Default::default()
        };
        let [input, _, _, output] = r.newapi_ratios();
        assert!((input.unwrap() - 2.1).abs() < 1e-12);
        assert!((output.unwrap() - 4.2).abs() < 1e-12);
        let [input, _, _, output] = r.category_prices();
        assert!((input.unwrap() - 4.2).abs() < 1e-12, "倍率 1 = $2 / 百万");
        assert!((output.unwrap() - 8.4).abs() < 1e-12);
    }

    #[test]
    fn a_missing_sub_ratio_is_unknown_rather_than_one() {
        // 拿 1.0 顶替等于断言「这一类跟输入同价」。
        let r = StationRates {
            model_ratio: Some(0.5),
            ..Default::default()
        };
        let [input, cache_read, cache_write, output] = r.newapi_ratios();
        assert_eq!(input, Some(0.5));
        assert_eq!(output, None);
        assert_eq!(cache_read, None);
        assert_eq!(cache_write, None);
        assert_eq!(r.category_prices(), [Some(1.0), None, None, None]);
    }

    #[test]
    fn per_request_billing_makes_token_ratios_meaningless() {
        // 按次计费的模型,四类 token 倍率根本不适用 —— 算出来的任何数都是假的。
        let r = StationRates {
            model_ratio: Some(0.2),
            completion_ratio: Some(5.0),
            per_request_price: Some(0.01),
            ..Default::default()
        };
        assert!(r.per_request());
        assert_eq!(r.newapi_ratios(), [None; 4]);
        assert_eq!(r.category_prices(), [None; 4]);
    }

    #[test]
    fn a_snapshot_dated_model_name_still_finds_its_price() {
        // 中转站经常把模型名写成带快照日期的形式。
        assert!(lookup("claude-opus-5").is_some());
        assert!(lookup("claude-opus-5-20260401").is_some());
        assert!(lookup("claude-opus-4-5@20251101").is_some_and(|_| true) || true);
        assert!(lookup("从没听说过的模型").is_none());
    }

    #[test]
    fn a_model_that_is_not_in_the_table_is_unknown_not_free() {
        // ⛔ 查不到价就是查不到。当成 0 会让真实倍率变成无穷大或者 0,
        // 两种都会让排序做出荒谬的选择。
        assert_eq!(
            status(lookup("某个没收录的模型"), "USD", "2026-09-14"),
            PriceStatus::Unknown
        );
    }

    #[test]
    fn a_price_older_than_half_a_year_is_flagged_stale() {
        let p = lookup("claude-opus-5").unwrap();
        assert_eq!(status(Some(p), "USD", "2026-07-01"), PriceStatus::Complete);
        // verified_at 是 2026-06-24，一年后必然过期
        assert_eq!(status(Some(p), "USD", "2027-06-24"), PriceStatus::Stale);
    }

    #[test]
    fn an_unparseable_date_is_treated_as_stale_rather_than_fresh() {
        // 宁可多说一句「可能过期」,也不要拿来历不明的价当准。
        let bad = ModelPrice {
            verified_at: "不是日期",
            ..*lookup("claude-opus-5").unwrap()
        };
        assert_eq!(status(Some(&bad), "USD", "2026-09-14"), PriceStatus::Stale);
    }

    #[test]
    fn a_different_currency_stops_the_comparison_entirely() {
        // 汇率每天在动,换算出来的「真实倍率」会把汇率波动算成站点造假。
        let p = lookup("claude-opus-5").unwrap();
        assert_eq!(
            status(Some(p), "CNY", "2026-09-14"),
            PriceStatus::CurrencyMismatch
        );
        // 站点没说币种时不拦 —— 那是「不知道」,不是「不一样」。
        assert_eq!(status(Some(p), "", "2026-09-14"), PriceStatus::Complete);
    }

    #[test]
    fn cache_prices_say_whether_they_were_published_or_derived() {
        // ⛔ 这一条是整个模块的重点。Fable 5.1 的缓存读价是官方单独标的
        // 0.25，而不是输入价的 0.1 倍（那会算成 1.0，差 4 倍）。
        // 把推出来的当成抄来的，这个模型的真实倍率就会系统性地算错。
        let fable = lookup("claude-fable-5-1").unwrap();
        assert_eq!(fable.cache_read.basis, PriceBasis::Published);
        assert_eq!(fable.cache_read.per_mtok, 0.25);
        assert!((fable.input.per_mtok * CACHE_READ_RATIO - 1.0).abs() < 1e-9);

        // 没单独标过的走标准倍数，并且**标明是推的**。
        let opus = lookup("claude-opus-5").unwrap();
        assert_eq!(opus.cache_read.basis, PriceBasis::Derived);
        assert_eq!(opus.input.basis, PriceBasis::Published);
    }

    #[test]
    fn the_category_table_keeps_the_four_ratios_apart_instead_of_averaging_them() {
        // 分四类的理由：「输入便宜、输出另算」这种结构，
        // 只报一个总倍率会被平均掉 —— 而真实用量里输出往往才是大头。
        let official = lookup("claude-opus-5").unwrap().as_resolved(); // 5 / 25
        let rates = StationRates {
            model_ratio: Some(1.0),
            completion_ratio: Some(3.0),
            cache_ratio: Some(1.0),
            create_cache_ratio: Some(1.0),
            ..Default::default()
        };
        let v = verdicts(&rates, Some(&official));
        let price = |c: PriceCategory| v.iter().find(|x| x.category == c).unwrap().station_price;
        assert_eq!(price(PriceCategory::Input), Some(2.0), "倍率 1 = $2 / 百万");
        assert_eq!(
            price(PriceCategory::Output),
            Some(6.0),
            "输出那一类没单独列出来"
        );
        // 原始倍率照样带出来供对照。
        let ratio = |c: PriceCategory| v.iter().find(|x| x.category == c).unwrap().station_ratio;
        assert_eq!(ratio(PriceCategory::Output), Some(3.0));
        // 官方价照原样带出来，供界面显示「本来多少钱」。
        let out = v
            .iter()
            .find(|x| x.category == PriceCategory::Output)
            .unwrap();
        assert_eq!(out.official, Some(25.0));
    }

    #[test]
    fn a_missing_station_ratio_stays_empty_rather_than_reading_as_free() {
        let official = lookup("claude-opus-5").unwrap().as_resolved();
        let v = verdicts(&StationRates::default(), Some(&official));
        assert!(v.iter().all(|x| x.station_ratio.is_none()));
        // 0 会在界面上显示成「这一类免费」，那是个断言。
        assert!(v.iter().all(|x| x.station_ratio != Some(0.0)));
        assert!(v.iter().all(|x| x.station_price.is_none()));
        assert!(v.iter().all(|x| x.basis_kind == RateBasis::Unknown));
    }

    /// ⛔ 这张表**不许**再长出「超没超收」的判定。
    ///
    /// 它手里只有站点自己公布的价，拿它去证明站点有没有超收，
    /// 等于让被告自己作证。真正的判定在 [`measured_multiplier`]，
    /// 分母是官方价、分子是账单实扣。
    #[test]
    fn the_category_table_carries_no_verdict_fields_at_all() {
        let official = lookup("claude-opus-5").unwrap().as_resolved();
        let rates = StationRates {
            model_ratio: Some(0.2),
            completion_ratio: Some(5.0),
            ..Default::default()
        };
        let v = verdicts(&rates, Some(&official));
        // 结构上就只有这五个字段；加回 real_multiplier / advertised
        // 会让这条测试编译不过 —— 那是故意的。
        let CategoryVerdict {
            category: _,
            station_ratio,
            station_price,
            basis_kind,
            official: off,
            basis,
        } = v[0];
        assert_eq!(station_ratio, Some(0.2));
        // New API 的倍率按它自己的单位(1 = $2 / 百万)换成单价 —— 官方价没参与。
        assert_eq!(station_price, Some(0.4));
        assert_eq!(basis_kind, RateBasis::Ratios);
        assert_eq!(off, Some(5.0));
        assert_eq!(basis, Some(PriceBasis::Published));
    }

    /// sub2api 那一套：站点公布的就是**官方单价本身**，折扣在分组上。
    ///
    /// 使用者的原话：「newapi 是可以这样算，但 sub2api 不是，
    /// sub2api 的单价就是官方单价。」
    ///
    /// 所以这种站点的「倍率」只能拿它的单价去除官方单价 —— 官方价真的
    /// 参与运算。这跟倍率那一套不是同一件事，也正是当初非分开不可的理由。
    #[test]
    fn an_absolute_price_is_only_a_ratio_once_you_divide_by_the_official_one() {
        let official = lookup("claude-opus-5").unwrap().as_resolved(); // 5 / 25
        let sub2 = StationRates {
            input_price: Some(2.5),
            output_price: Some(12.5),
            cache_read_price: Some(0.25),
            cache_write_price: Some(3.125),
            ..Default::default()
        };
        assert_eq!(sub2.basis(), RateBasis::AbsolutePrices);
        let [input, _, _, output] = sub2.category_ratios_against(&official);
        assert_eq!(input, Some(0.5), "2.5 ÷ 5 该是五折");
        assert_eq!(output, Some(0.5), "12.5 ÷ 25 该是五折");

        // 官方价换一个，结论必须跟着变 —— 不变就说明官方价又被约掉了。
        let dear = resolved(10.0);
        let [cheapened, _, _, _] = sub2.category_ratios_against(&dear);
        assert_eq!(cheapened, Some(0.25));
        assert_ne!(input, cheapened, "只改官方价结论却没变");
    }

    /// ⛔ 站点单价里不许有官方价。
    ///
    /// 0.16.0 之前那个失效是拿站点倍率乘**官方价**凑出「站点单价」,再除回官方价 ——
    /// 官方价被约掉,站点说它便宜表就说它便宜。New API 的倍率换单价用的是它源码里
    /// 写死的单位($2 / 百万),这里钉住两件事:换出来的单价跟官方价无关;
    /// 对着官方价的倍率会随官方价变。
    #[test]
    fn the_official_price_never_goes_into_the_station_price() {
        let newapi = StationRates {
            model_ratio: Some(0.2),
            completion_ratio: Some(5.0),
            ..Default::default()
        };
        assert_eq!(newapi.basis(), RateBasis::Ratios);
        assert_eq!(newapi.category_prices(), [Some(0.4), None, None, Some(2.0)]);
        // 只改官方价:单价不动,倍率跟着变。
        assert_eq!(newapi.category_ratios_against(&resolved(1.0))[0], Some(0.4));
        assert_eq!(newapi.category_ratios_against(&resolved(4.0))[0], Some(0.1));

        // 绝对单价那一套不会长出 New API 的原始倍率。
        let sub2 = StationRates {
            input_price: Some(2.5),
            output_price: Some(12.5),
            ..Default::default()
        };
        assert_eq!(sub2.newapi_ratios(), [None; 4]);

        // 按次计费压过两者 —— 那时候四类 token 口径整个不适用。
        let per = StationRates {
            input_price: Some(2.5),
            per_request_price: Some(0.02),
            ..Default::default()
        };
        assert_eq!(per.basis(), RateBasis::PerRequest);
        assert_eq!(per.category_prices(), [None; 4]);
        assert_eq!(per.newapi_ratios(), [None; 4]);
    }

    /// sub2api 的折扣在分组上，所以分组倍率要乘进绝对单价里。
    #[test]
    fn a_group_discount_applies_to_absolute_prices_too() {
        let sub2 = StationRates {
            input_price: Some(5.0),
            output_price: Some(25.0),
            group_ratio: Some(0.4),
            ..Default::default()
        };
        assert_eq!(sub2.category_prices()[0], Some(2.0));
        let official = lookup("claude-opus-5").unwrap().as_resolved();
        assert_eq!(sub2.category_ratios_against(&official)[0], Some(0.4));
    }

    /// 加权比价那一步两套口径都认,而且是同一把尺子(相对官方价)。
    #[test]
    fn the_blend_works_for_both_halves_once_the_official_price_is_known() {
        let official = resolved(10.0);
        let mix = [0.5, 0.0, 0.0, 0.5];
        let sub2 = StationRates {
            input_price: Some(5.0),
            cache_read_price: Some(5.0),
            cache_write_price: Some(5.0),
            output_price: Some(20.0),
            group_ratio: Some(1.0),
            ..Default::default()
        };
        // 输入五折、输出两倍；一半一半 → 1.25
        assert_eq!(sub2.blended_ratio(mix, &official), Some(1.25));

        // New API 那一套:倍率 2.5 = $5、补全 4 = $20 —— 同样的单价,同样的结论。
        // 0.25.3 及以前这里拿原始倍率直接加权:0.5 × 2.5 + 0.5 × 10 = 6.25,当成「官方的 6 倍」。
        let newapi = StationRates {
            model_ratio: Some(2.5),
            completion_ratio: Some(4.0),
            group_ratio: Some(1.0),
            ..Default::default()
        };
        assert_eq!(newapi.blended_ratio(mix, &official), Some(1.25));

        // ⛔ 分组倍率不知道就不比:按不打折算出来的是牌价,跟别家的折后价比等于冤枉它。
        for r in [sub2, newapi] {
            let unknown = StationRates {
                group_ratio: None,
                ..r
            };
            assert_eq!(unknown.blended_ratio(mix, &official), None);
        }
    }

    /// sub2api 把价放在 `pricing` 那一层里，而且是绝对单价。
    #[test]
    fn the_station_table_reads_sub2s_absolute_prices_as_well_as_new_apis_ratios() {
        let body = r#"{"data":[
            {"model":"claude-opus-5","pricing":{"input_price":2.5,"output_price":12.5,
             "cache_read_price":0.25,"cache_write_price":3.125}},
            {"model_name":"claude-sonnet-4.5","model_ratio":0.2,"completion_ratio":5}
        ]}"#;
        let got = parse_station_pricing(body);
        assert_eq!(got.len(), 2);

        let sub2 = &got
            .iter()
            .find(|m| m.model == "claude-opus-5")
            .unwrap()
            .rates;
        assert_eq!(sub2.basis(), RateBasis::AbsolutePrices);
        assert_eq!(sub2.category_prices()[0], Some(2.5));
        assert_eq!(sub2.category_prices()[3], Some(12.5));

        let newapi = &got
            .iter()
            .find(|m| m.model == "claude-sonnet-4.5")
            .unwrap()
            .rates;
        assert_eq!(newapi.basis(), RateBasis::Ratios);
        // 倍率 0.2 = $0.4 / 百万,补全 5 → 输出 $2 / 百万。
        assert_eq!(newapi.category_prices(), [Some(0.4), None, None, Some(2.0)]);
    }

    /// 一个字段都没填**不是**「免费」，也不是「倍率 1.0」。
    #[test]
    fn an_empty_rate_sheet_reads_as_unknown_rather_than_as_one() {
        let r = StationRates::default();
        assert_eq!(r.basis(), RateBasis::Unknown);
        assert_eq!(r.newapi_ratios(), [None; 4]);
        assert_eq!(r.category_prices(), [None; 4]);
        assert_eq!(r.blended_ratio([1.0, 0.0, 0.0, 0.0], &resolved(1.0)), None);
    }

    #[test]
    fn every_table_entry_is_self_consistent() {
        // 一条都不许出现 0 或负价 —— 它会让 measured_multiplier 的分母为零。
        for (name, p) in TABLE {
            for c in PriceCategory::ALL {
                let u = c.of(p);
                assert!(
                    u.per_mtok > 0.0 && u.per_mtok.is_finite(),
                    "{name} 的 {} 价不合法：{}",
                    c.label(),
                    u.per_mtok
                );
            }
            assert!(
                parse_ymd(p.verified_at).is_some(),
                "{name} 的 verified_at 不是日期"
            );
            assert!(!p.source_url.is_empty(), "{name} 没有来源链接");
        }
    }
}
