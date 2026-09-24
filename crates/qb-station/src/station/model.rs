//! 中转站的数据形状。**纯类型,不做任何 I/O** —— 落盘一律由调用方显式做。
//!
//! # 为什么「线路」是站点 + 分组 + 令牌
//!
//! New API 的分组同时是两样东西:**渠道权限边界**和**计费系数**。
//! 而一把令牌只绑一个分组,只能用该分组内渠道提供的模型。计费是
//! `配额消耗 = (输入token + 输出token × 补全倍率) × 模型倍率 × 分组倍率`。
//!
//! 所以倍率、可用模型、健康度**全是分组级的**,按站点显示一个「最低倍率」
//! 会骗人:那个最低值可能来自一个你的令牌根本用不了的分组。
//!
//! **只有余额是账号级的**,整站共享 —— 你在 GPT 分组上烧的钱会吃掉
//! Claude 分组的余额。这一点最容易误解,界面上必须写明。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 站点后端是哪一家。决定用哪个适配器去读倍率、余额和账单。
///
/// `Generic` 不是「未知」,是「只有 API Key、没有面板」—— 这类站点能启动、
/// 能测活,但读不到倍率和账单,界面上那些格子要写「站点未提供」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum StationKind {
    NewApi,
    Sub2,
    OneApi,
    Generic,
}

/// 站点暴露了哪些协议端点。
///
/// 这决定一条线路能不能用某个客户端启动:Claude Code 要 Anthropic Messages,
/// Codex 要 OpenAI。**「模型是谁家的」和「用哪个客户端启动」是正交的** ——
/// DeepSeek 有 Anthropic 协议端点,所以 ds 模型可以用 Claude Code 跑。
///
/// 三个都是 `Option<bool>`:`None` = 还没探过,跟「探过了,没有」是两回事。
/// 混成 `false` 的话,界面会把「还没测」显示成「不支持」,用户会以为这站废了。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Protocols {
    pub anthropic: Option<bool>,
    pub openai_chat: Option<bool>,
    pub openai_responses: Option<bool>,
}

impl Protocols {
    /// 这套协议能不能起这个客户端。**探不出来时返回 `None`,不猜。**
    pub fn supports(self, client: crate::domain::Client) -> Option<bool> {
        match client {
            crate::domain::Client::ClaudeCode | crate::domain::Client::ClaudeDesktop => {
                self.anthropic
            }
            // Codex 两种协议任一即可;都还没探过才是 None。
            crate::domain::Client::Codex => match (self.openai_responses, self.openai_chat) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(false) || b.unwrap_or(false)),
            },
            // 反重力没有中转路径（端点写死在客户端里），**不是「还没探」，是「探了也没用」**：
            // 回 `Some(false)`，界面上如实显示成「不支持」而不是「未检测」。
            crate::domain::Client::Antigravity | crate::domain::Client::AntigravityIde => {
                Some(false)
            }
        }
    }
}

/// 余额读没读到。**`Missing` 与 `Invalid` 必须分开** ——
/// 前者是站点没这个字段(那就永远别再提示用户去查),后者是字段在但解不出数
/// (那是适配器该修的 bug)。合并成一个「不可用」,两种都查不出来。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum BalanceStatus {
    Ok,
    Missing,
    Invalid,
    Unavailable,
}

/// 账号级余额。整站四个厂商的分组**共用这一笔**。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Balance {
    /// 可用余额。`None` 时界面写「站点未提供」,**不许填 0** ——
    /// 0 和「读不到」在界面上是两种完全不同的处置:前者该充值,后者该查接口。
    pub available: Option<f64>,
    pub used: Option<f64>,
    pub currency: String,
    pub status: BalanceStatus,
    pub checked_at: String,
    /// 这次没读到、显示的是上一次的值。界面上要标「· 上次」。
    pub stale: bool,
}

impl Default for Balance {
    fn default() -> Self {
        Self {
            available: None,
            used: None,
            currency: String::new(),
            status: BalanceStatus::Unavailable,
            checked_at: String::new(),
            stale: false,
        }
    }
}

/// 倍率跟上次快照比是涨是跌。
///
/// **粘性语义**:只在真正变化时更新,之后一直保留最后一次的方向。
/// 每次刷新都重置成 `Same` 的话,「昨天悄悄涨价了」这件事在界面上活不过一次刷新。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum RateChange {
    New,
    Up,
    Down,
    #[default]
    Same,
}

/// 一个分组。倍率与可用模型都在这一层。
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct StationGroup {
    pub id: String,
    pub name: String,
    pub description: String,
    /// 站点标称的基础倍率。
    pub base_rate: Option<f64>,
    /// 实际生效的倍率。排名用这个。
    pub effective_rate: Option<f64>,
    pub models: Vec<String>,
    /// 自动路由组。
    ///
    /// **不参与自动排名** —— 它落到哪个组就按哪个组结账,倍率不可预先确定,
    /// 拿一个不确定的数去跟确定的数排序,排出来的名次是假的。
    /// 但允许手动启用,所以不能直接过滤掉。
    pub auto: bool,
    pub rate_change: RateChange,
    pub previous_rate: Option<f64>,
    pub last_rate_change: String,
}

/// 排名门槛。低于这个倍率的分组不参与排序。
///
/// 站点常有 0 倍率或极低倍率的「免费体验组」,它们要么有严格配额、要么随时下线,
/// 让它们长期霸占第一名,排序就没有意义了。这个值从压缩包原样搬来。
pub const RANK_RATE_FLOOR: f64 = 0.001;

impl StationGroup {
    /// 这个分组的倍率能不能参与排名。
    ///
    /// 三种排除:没有倍率(读不到)、auto 组(不确定)、低于门槛(免费组)。
    pub fn rankable(&self) -> bool {
        !self.auto
            && self
                .effective_rate
                .is_some_and(|r| r >= RANK_RATE_FLOOR && r.is_finite())
    }
}

/// 账单日志里的一行,已经归一成统一口径。
///
/// **每个字段独立表达「有没有」** —— 站点少给一个字段时不能拿 0 顶上,
/// 否则缓存命中率会被稀释成一个看起来很正常的小数,而真相是「这站压根没报缓存」。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageRow {
    /// Ledger request identifier, used to correlate one paid probe without guessing.
    pub request_id: String,
    /// 毫秒时间戳。时间窗聚合按它切。
    pub at_ms: i64,
    pub group: String,
    pub model: String,
    /// 未缓存输入。已经减掉了 cache_read 与 cache_write。
    pub input_uncached: Option<u64>,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
    pub output: Option<u64>,
    /// 首字延迟。New API 记在日志的 `other.frt` 里 ——
    /// **这是真实流量的测速,零成本,而且比探针准**:探针请求的形状跟生产不一样。
    pub first_token_ms: Option<u64>,
    pub total_ms: Option<u64>,
    /// 实扣金额(站内计费单位)。
    pub cost: Option<f64>,
    /// 这一条成没成功。**只有 [`status_reported`](Self::status_reported)
    /// 为真时才作数。**
    pub ok: bool,
    /// 源头到底报没报这一条的成败。
    ///
    /// **不能省。** `/api/log/self` 是一份**消费**日志:没扣到钱的失败请求
    /// 很可能压根不出现在里面。那样一来「成功率」算出来永远是 100%,
    /// 而它正是智能调度里「稳」那一维的输入 —— 等于凭空替每一家站点作证,
    /// 还让最弱项规则永远卡不到可靠性上。
    ///
    /// 源头没报时这里是 `false`,聚合会**不给成功率**(`None`,界面显示「—」),
    /// 而不是给一个好看的数。缓存命中率、首字延迟、花费不受影响 ——
    /// 那几项这一行仍然是有效证据。
    pub status_reported: bool,
}

impl UsageRow {
    /// 这一行的输入总量 = 未缓存 + 缓存读 + 缓存写。
    ///
    /// 三项**必须都取证到**才给数:少一项就算出来一个偏小的分母,
    /// 缓存命中率会被抬高 —— 那正好是造假站点希望你看到的方向。
    pub fn input_total(&self) -> Option<u64> {
        Some(self.input_uncached? + self.cache_read? + self.cache_write?)
    }
}
