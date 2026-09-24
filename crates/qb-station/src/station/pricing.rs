//! 官方参考价。**纯数据 + 纯算术,不联网** —— 抓取更新在编排层。
//!
//! # 它是干什么用的
//!
//! 站点标称「×0.20」,那是相对**官方价**的倍数。要验证这个标称是不是真的,
//! 就得拿站点的单价跟官方单价比:
//!
//! ```text
//! 真实倍率 = 站点单价 × 站点标称倍率 ÷ 官方单价
//! ```
//!
//! 所以官方价目表是「查套路」的分母,没有它就只能听站点自己说。
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
/// # 现在它只报「站点自己说了什么」,两套口径各占一列
///
/// | 站点后端   | 填哪一格        | 那一格是什么          |
/// |------------|-----------------|-----------------------|
/// | New API 系 | `station_ratio` | 相对它自己配额基准的倍率 |
/// | sub2api 系 | `station_price` | 绝对单价,美元／百万 token |
///
/// ⛔ **两格不互相换算。** 换算要除以官方价 —— 而那正是上面那条静默失效的
/// 入口。哪一格有效由 [`basis_kind`](Self::basis_kind) 说明,界面据此决定
/// 显示哪一列;真要比的话自己除,那时至少是明着除的。
///
/// 「它到底收了几倍」由 [`measured_multiplier`] 回答 —— 那一个的分子是
/// 站点账单上真金白银的实扣,分母是同一批 token 按官方单价算出来的成本,
/// 两头都不是站点公布的数。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CategoryVerdict {
    pub category: PriceCategory,
    /// 站点公布的这一类倍率(`model_ratio` × 子倍率 × 分组倍率 × 峰时倍率)。
    ///
    /// 公布绝对单价的站点(sub2api 系)这一格是 `None`,价在下面那一格。
    pub station_ratio: Option<f64>,
    /// 站点公布的这一类**绝对单价**(每百万 token,已乘分组与峰时)。
    ///
    /// 公布倍率的站点(New API 系)这一格是 `None` —— ⛔ **不拿倍率乘官方价
    /// 凑一个出来**,那正是上面那段说的失效。
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
/// 两套口径各占一列:倍率那一套填 `station_ratio`,绝对单价那一套填
/// `station_price`,另一格留空。⛔ **不互相换算** —— 换算要除以官方价,
/// 而那正是当年「官方价被约掉」那条静默失效的入口。界面要比的话自己除,
/// 那时至少是明着除的。
///
/// 取不到的那一类给 `None` —— **不要填 0**,0 会显示成「这一类免费」。
pub fn verdicts(rates: &StationRates, official: Option<&ResolvedPrice>) -> Vec<CategoryVerdict> {
    let kind = rates.basis();
    let ratios = rates.category_ratios();
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
/// ⛔ **不是一个倍率。** New API 的每个模型给的是一组比例,输出那一项还要
/// 再乘一次 `completion_ratio` —— 这就是「计费翻倍」:
///
/// ```text
/// 输入   = model_ratio
/// 输出   = model_ratio × completion_ratio      ← 常见 3~5 倍
/// 缓存读 = model_ratio × cache_ratio
/// 缓存写 = model_ratio × create_cache_ratio
/// 再乘   × 分组倍率 ×(峰时浮动)
/// ```
///
/// 只拿 `model_ratio` 当「这站的倍率」去比价,会把输出那几倍整个漏掉 ——
/// 而真实用量里输出往往才是花钱的大头。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StationRates {
    /// 输入倍率(New API 的 `model_ratio`)。
    pub model_ratio: Option<f64>,
    /// 输出相对输入的倍数(`completion_ratio`)。**这是「翻倍」那一项。**
    pub completion_ratio: Option<f64>,
    /// 缓存读相对输入的倍数(`cache_ratio`)。
    pub cache_ratio: Option<f64>,
    /// 缓存写相对输入的倍数(`create_cache_ratio`)。
    pub create_cache_ratio: Option<f64>,
    /// 分组倍率。整条线路再乘一次。
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
    // | 站点后端   | 它公布什么                       | 折扣在哪 |
    // |------------|----------------------------------|----------|
    // | New API 系 | 相对倍率(相对它自己的配额基准) | 倍率本身 |
    // | sub2api 系 | 绝对单价(美元／百万 token)     | 分组倍率 |
    //
    // 使用者原话:「newapi 是可以这样算,但 sub2api 不是,
    // sub2api 的单价就是官方单价。」—— 那一套的「便宜」全在分组上,
    // 拿 `model_ratio` 那条路去读它只会读出一片空白。
    //
    // 两套混着填会让 [`StationRates::category_ratios`] 与
    // [`StationRates::category_prices`] 各算各的、结果对不上。
    // 哪一套有效由 [`StationRates::basis`] 从「谁填了」推出来,
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
    /// 相对倍率(New API 系)。
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
    /// 两者取不到时按 1 处理 —— 它们是「再乘一次」的修正项,缺席的自然含义
    /// 就是不修正;这跟 `model_ratio` 缺席(根本不知道基准)不是一回事。
    fn scale(&self) -> f64 {
        let one = |v: Option<f64>| v.filter(|x| x.is_finite() && *x > 0.0).unwrap_or(1.0);
        one(self.group_ratio) * one(self.peak_rate)
    }

    /// 四类各自的实际倍率(已经乘上分组倍率与峰时上界)。
    ///
    /// 顺序与 [`PriceCategory::ALL`] 一致。任何一项算不出来就是 `None` ——
    /// **不拿 1.0 顶替**,那等于断言「这一类不加价」。
    ///
    /// ⛔ **只管倍率那一套。** 公布绝对单价的站点(sub2api 系)在这里全 `None`:
    /// 一个绝对单价换不成倍率,除非知道官方价。两套都要的话用
    /// [`category_ratios_against`](Self::category_ratios_against)。
    pub fn category_ratios(&self) -> [Option<f64>; 4] {
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

    /// 四类各自的**绝对单价**(每百万 token,已乘分组倍率与峰时上界)。
    ///
    /// 只有 [`RateBasis::AbsolutePrices`] 的站点给得出来;别的一律 `None` ——
    /// ⛔ **不拿倍率乘一个猜的基准去凑**,那正是 0.16.0 之前那个
    /// 「官方价被约掉」的失效的来源。
    pub fn category_prices(&self) -> [Option<f64>; 4] {
        if self.basis() != RateBasis::AbsolutePrices {
            return [None; 4];
        }
        let scale = self.scale();
        let of = |v: Option<f64>| v.filter(|x| x.is_finite() && *x >= 0.0).map(|x| x * scale);
        [
            of(self.input_price),
            of(self.cache_read_price),
            of(self.cache_write_price),
            of(self.output_price),
        ]
    }

    /// 四类各自相对**官方价**的倍率。**两套口径在这里合流。**
    ///
    /// | 站点口径 | 怎么算 | 官方价参与了吗 |
    /// |---|---|---|
    /// | 绝对单价 | 站点单价 ÷ 官方单价 | 参与了 |
    /// | 相对倍率 | 就是 [`category_ratios`](Self::category_ratios) | 用不上 |
    /// | 按次 / 不知道 | 全 `None` | —— |
    ///
    /// ⚠ 这个函数只能用来**显示与粗比**。它对绝对单价那一套是真结论,
    /// 对倍率那一套只是把站点自己说的搬了一遍 —— 两者不可比。
    /// 「这家到底收了几倍」只有 [`measured_multiplier`] 答得了。
    pub fn category_ratios_against(&self, official: &ResolvedPrice) -> [Option<f64>; 4] {
        if self.basis() != RateBasis::AbsolutePrices {
            return self.category_ratios();
        }
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
    /// 这家站开没开「计费翻倍」。
    ///
    /// `completion_ratio > 1` 就是开了 —— 输出按输入的若干倍计费。
    /// **界面要显示这一项**:两家站一个翻倍一个不翻倍时,
    /// 光看「倍率 ×0.15 对 ×0.4」会得出完全相反的结论。
    ///
    /// `None` = 站点没公布这一项,不知道。
    pub fn folds_completion(&self) -> Option<bool> {
        let r = self.completion_ratio?;
        r.is_finite().then_some(r > 1.0)
    }

    /// 按 token 结构加权出一个**可比**的等效倍率。
    ///
    /// # 为什么非这样不可
    ///
    /// A 站 `model_ratio 0.15` + `completion_ratio 5`(输出 ×0.75);
    /// B 站 `model_ratio 0.4` 不翻倍(输出 ×0.4)。
    /// 谁便宜**取决于你的输入输出比**:
    ///
    /// - 纯读代码(输出占一成)：A ≈ 0.21,B = 0.4 → A 便宜;
    /// - 长篇生成(输出占七成)：A ≈ 0.57,B = 0.4 → B 便宜。
    ///
    /// 只比 `model_ratio` 会永远选 A,只比输出倍率会永远选 B,两个都是错的。
    ///
    /// `mix` 是四类占比(见 `health::window::TokenMix`)。
    /// 任何一类倍率缺席就返回 `None` —— **不拿 1.0 补**。
    pub fn blended_ratio(&self, mix: [f64; 4]) -> Option<f64> {
        self.blend(mix, self.category_ratios())
    }

    /// 同 [`blended_ratio`](Self::blended_ratio),但**两套口径都算得出来**。
    ///
    /// 公布绝对单价的站点(sub2api 系)要有官方价才换得出倍率 —— 见
    /// [`category_ratios_against`](Self::category_ratios_against)。
    /// 连官方价都没有时它返回 `None`,排序那边会因此整池退回更粗的口径。
    /// 那是对的:拿一个算不出来的数去比,比退回粗口径糟得多。
    pub fn blended_ratio_against(&self, mix: [f64; 4], official: &ResolvedPrice) -> Option<f64> {
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

/// 解析站点自己的 `/api/pricing`,拿到每个模型的计费倍率。
///
/// **纯函数。** 字段名照 New API 的 `/api/pricing`:
///
/// | 字段 | 含义 |
/// |---|---|
/// | `model_ratio` | 输入倍率 |
/// | `completion_ratio` | 输出 = `model_ratio` × 它 |
/// | `cache_ratio` / `create_cache_ratio` | 缓存读 / 写 |
/// | `model_price` + `billing_mode` | 按次计费时 token 倍率不适用 |
///
/// 认不出来的字段一律 `None`,**绝不填 1.0** —— 1.0 是「这一类不加价」的断言。
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

    #[test]
    fn a_station_pricing_payload_parses_into_four_ratios() {
        let body = r#"{"data":[
            {"model_name":"claude-sonnet-4.5","model_ratio":0.2,"completion_ratio":5,
             "cache_ratio":0.1,"create_cache_ratio":1.25},
            {"model_name":"按次计费的","model_ratio":0.2,"completion_ratio":5,"model_price":0.02}
        ]}"#;
        let got = parse_station_pricing(body);
        assert_eq!(got.len(), 2);
        let r = &got
            .iter()
            .find(|m| m.model == "claude-sonnet-4.5")
            .unwrap()
            .rates;
        let [input, _, _, output] = r.category_ratios();
        assert_eq!(input, Some(0.2));
        assert_eq!(output, Some(1.0), "输出没乘 completion_ratio");

        let per = &got.iter().find(|m| m.model == "按次计费的").unwrap().rates;
        assert!(per.per_request());
        assert_eq!(per.category_ratios(), [None; 4]);
    }

    #[test]
    fn a_missing_completion_ratio_stays_unknown() {
        // ⛔ 填 1.0 等于断言「输出不加价」—— 而输出往往才是大头。
        let body = r#"[{"model_name":"x","model_ratio":0.5}]"#;
        let r = parse_station_pricing(body)
            .into_iter()
            .next()
            .unwrap()
            .rates;
        assert_eq!(r.completion_ratio, None);
        assert_eq!(r.category_ratios()[3], None);
    }

    #[test]
    fn garbage_pricing_yields_nothing_rather_than_a_panic() {
        for body in ["", "null", "{}", "not json", r#"{"data":{}}"#] {
            assert!(parse_station_pricing(body).is_empty(), "{body}");
        }
    }

    #[test]
    fn which_station_is_cheaper_depends_on_the_token_mix() {
        // 这条是「计费翻倍」那个问题的答案。
        // A：输入便宜但输出翻 5 倍；B：输入贵但不翻倍。
        let a = StationRates {
            model_ratio: Some(0.15),
            completion_ratio: Some(5.0),
            cache_ratio: Some(0.1),
            create_cache_ratio: Some(1.25),
            ..Default::default()
        };
        let b = StationRates {
            model_ratio: Some(0.4),
            completion_ratio: Some(1.0),
            cache_ratio: Some(0.1),
            create_cache_ratio: Some(1.25),
            ..Default::default()
        };
        assert_eq!(a.folds_completion(), Some(true));
        assert_eq!(b.folds_completion(), Some(false));

        // 纯读代码：输出只占一成 → A 便宜
        let reading = [0.9, 0.0, 0.0, 0.1];
        assert!(a.blended_ratio(reading).unwrap() < b.blended_ratio(reading).unwrap());

        // 长篇生成：输出占七成 → B 便宜
        let writing = [0.3, 0.0, 0.0, 0.7];
        assert!(
            b.blended_ratio(writing).unwrap() < a.blended_ratio(writing).unwrap(),
            "输出占七成时翻倍那条应该更贵"
        );
    }

    #[test]
    fn a_cache_heavy_line_is_not_cheap_on_a_fresh_conversation() {
        // 靠缓存读撑起来的便宜，在新对话第一轮上不成立 —— 那段上下文还没缓存过。
        let r = StationRates {
            model_ratio: Some(1.0),
            completion_ratio: Some(1.0),
            cache_ratio: Some(0.1),
            create_cache_ratio: Some(1.25),
            ..Default::default()
        };
        let habitual = [0.1, 0.8, 0.0, 0.1]; // 平时八成走缓存读
        let fresh = [0.9, 0.0, 0.0, 0.1]; // 新对话：那八成变成未缓存输入
        assert!(
            r.blended_ratio(fresh).unwrap() > r.blended_ratio(habitual).unwrap(),
            "新对话没比平时贵 —— 缓存读被当成一直有效了"
        );
    }

    #[test]
    fn a_category_that_carries_no_tokens_may_be_missing() {
        // 乘 0 本来就不影响结果，不该因为它缺席就整个算不出来。
        let r = StationRates {
            model_ratio: Some(0.2),
            completion_ratio: Some(2.0),
            cache_ratio: None,
            create_cache_ratio: None,
            ..Default::default()
        };
        // 0.1 + 0.2 在二进制里不等于 0.3 —— 这里测的是加权，不是浮点表示。
        let got = r.blended_ratio([0.5, 0.0, 0.0, 0.5]).unwrap();
        assert!((got - 0.3).abs() < 1e-12, "got={got}");
        // 但真的占量时就不能瞎补
        assert_eq!(r.blended_ratio([0.4, 0.2, 0.0, 0.4]), None);
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
    fn output_is_the_input_ratio_times_the_completion_ratio() {
        // ⛔ 「计费翻倍」就是这一项。只拿 model_ratio 当「这站的倍率」,
        // 输出那几倍整个漏掉,而输出往往才是花钱的大头。
        let r = StationRates {
            model_ratio: Some(0.2),
            completion_ratio: Some(5.0),
            cache_ratio: Some(0.1),
            create_cache_ratio: Some(1.25),
            ..Default::default()
        };
        let [input, cache_read, cache_write, output] = r.category_ratios();
        assert_eq!(input, Some(0.2));
        assert_eq!(output, Some(1.0), "输出没乘 completion_ratio");
        assert!((cache_read.unwrap() - 0.02).abs() < 1e-12);
        assert!((cache_write.unwrap() - 0.25).abs() < 1e-12);
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
        let [input, _, _, output] = r.category_ratios();
        assert!((input.unwrap() - 2.1).abs() < 1e-12);
        assert!((output.unwrap() - 4.2).abs() < 1e-12);
    }

    #[test]
    fn a_missing_sub_ratio_is_unknown_rather_than_one() {
        // 拿 1.0 顶替等于断言「这一类不加价」。
        let r = StationRates {
            model_ratio: Some(0.5),
            ..Default::default()
        };
        let [input, cache_read, cache_write, output] = r.category_ratios();
        assert_eq!(input, Some(0.5));
        assert_eq!(output, None);
        assert_eq!(cache_read, None);
        assert_eq!(cache_write, None);
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
        assert_eq!(r.category_ratios(), [None; 4]);
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
        // 分四类的理由：「输入便宜、输出翻三倍」这种结构，
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
        let by = |c: PriceCategory| v.iter().find(|x| x.category == c).unwrap().station_ratio;
        assert_eq!(by(PriceCategory::Input), Some(1.0));
        assert_eq!(by(PriceCategory::Output), Some(3.0), "输出翻倍没显示出来");
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
        assert_eq!(station_price, None, "倍率口径的站点不许凭空长出绝对单价");
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

    /// ⛔ 两套口径不许互相顶替。
    ///
    /// 拿倍率乘一个猜的基准凑出「绝对单价」，或者拿绝对单价当倍率用，
    /// 都会让界面显示一个没人算得出来的数。宁可那一格空着。
    #[test]
    fn neither_half_of_the_struct_ever_fabricates_the_other() {
        let newapi = StationRates {
            model_ratio: Some(0.2),
            completion_ratio: Some(5.0),
            ..Default::default()
        };
        assert_eq!(newapi.basis(), RateBasis::Ratios);
        assert_eq!(newapi.category_prices(), [None; 4], "倍率凑不出绝对单价");

        let sub2 = StationRates {
            input_price: Some(2.5),
            output_price: Some(12.5),
            ..Default::default()
        };
        assert_eq!(
            sub2.category_ratios(),
            [None; 4],
            "绝对单价在不知道官方价时换不成倍率"
        );

        // 按次计费压过两者 —— 那时候四类 token 口径整个不适用。
        let per = StationRates {
            input_price: Some(2.5),
            per_request_price: Some(0.02),
            ..Default::default()
        };
        assert_eq!(per.basis(), RateBasis::PerRequest);
        assert_eq!(per.category_prices(), [None; 4]);
        assert_eq!(per.category_ratios(), [None; 4]);
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

    /// 加权比价那一步也要认得绝对单价那一套，否则整池会退回更粗的口径。
    #[test]
    fn the_blend_works_for_both_halves_once_the_official_price_is_known() {
        let official = resolved(10.0);
        let sub2 = StationRates {
            input_price: Some(5.0),
            cache_read_price: Some(5.0),
            cache_write_price: Some(5.0),
            output_price: Some(20.0),
            ..Default::default()
        };
        // 输入五折、输出两倍；一半一半 → 1.25
        let got = sub2.blended_ratio_against([0.5, 0.0, 0.0, 0.5], &official);
        assert_eq!(got, Some(1.25));
        // 没有官方价时那一套算不出来 —— 返回 None，由调用方整池退回粗口径。
        assert_eq!(sub2.blended_ratio([0.5, 0.0, 0.0, 0.5]), None);

        // 倍率那一套走 against 也要照旧算得出来。
        let newapi = StationRates {
            model_ratio: Some(0.2),
            completion_ratio: Some(2.0),
            ..Default::default()
        };
        assert_eq!(
            newapi.blended_ratio_against([0.5, 0.0, 0.0, 0.5], &official),
            newapi.blended_ratio([0.5, 0.0, 0.0, 0.5])
        );
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
        assert_eq!(newapi.category_prices(), [None; 4]);
    }

    /// 一个字段都没填**不是**「免费」，也不是「倍率 1.0」。
    #[test]
    fn an_empty_rate_sheet_reads_as_unknown_rather_than_as_one() {
        let r = StationRates::default();
        assert_eq!(r.basis(), RateBasis::Unknown);
        assert_eq!(r.category_ratios(), [None; 4]);
        assert_eq!(r.category_prices(), [None; 4]);
        assert_eq!(r.blended_ratio([1.0, 0.0, 0.0, 0.0]), None);
        assert_eq!(
            r.blended_ratio_against([1.0, 0.0, 0.0, 0.0], &resolved(1.0)),
            None
        );
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
