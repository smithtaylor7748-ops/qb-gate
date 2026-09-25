//! 智能调度：**勾的几项里,最弱的那一项最好的那条赢。**
//!
//! 纯函数,不联网、不落盘、不取系统时间。排序算法与它的三条实测理由在
//! [`rank`]；这里只放输入形状与三个预设。
//!
//! # 三项可单选也可多选
//!
//! 便宜 / 快 / 稳是**复选框,不是推子**。推子被三条实测否掉了,见 [`rank`] 模块头。
//!
//! # 预设不是魔法数字
//!
//! 三个预设都能用「勾哪几项 + 设哪条底线」完整写出来,界面上照着念就行:
//!
//! | 预设 | 勾 | 底线 |
//! |---|---|---|
//! | 写代码 | 稳 + 快 | 首字 P95 ≤ 3s |
//! | 跑批量 | 便宜 | 成功率 ≥ 95% |
//! | 对话 | 快 | 成功率 ≥ 99% |

pub mod breaker;
pub mod cheap;
pub mod rank;

pub use breaker::{Outcome, RouteBreaker, State};
pub use cheap::{cheap_values, cost_per_token, CheapBasis, CheapInput};
pub use rank::{rank, AxisScore, FloorMiss, Ranking, Row, HYSTERESIS, INDISTINGUISHABLE_SPREAD};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 可以勾的三个维度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum Axis {
    /// 倍率。**比的是真实倍率**(标称 × mult),不是站点自己标的那个。
    Cheap,
    /// 快。**排序按体验分**(首字 + 生成速度×典型长度),不是只看首字 ——
    /// 只看首字会选中「首字最快、但一次回答要吐 95 秒」的那条,
    /// 见 `health::window::Window::experience_ms`。
    ///
    /// 注意底线那边仍然按**首字 P95** 筛:使用者在界面上设的是首字,
    /// 就该按首字筛。两者分开是有意的。
    Fast,
    /// 成功率。
    Stable,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::Cheap, Axis::Fast, Axis::Stable];

    /// 这一维是不是越小越好。归一化要靠它把三维掰到同一个方向上,
    /// 最弱项规则才能跨维度比较。
    pub fn smaller_is_better(self) -> bool {
        matches!(self, Axis::Cheap | Axis::Fast)
    }

    /// 界面上的名字。**纯文本渲染,不许写 Markdown 的星号。**
    pub fn label(self) -> &'static str {
        match self {
            Axis::Cheap => "便宜",
            Axis::Fast => "快",
            Axis::Stable => "稳",
        }
    }
}

/// 可选的底线。**默认一条都不设。**
///
/// 先筛掉再排序。全卡光时不许死局 —— 见 [`rank`]：照常走,但每行标出
/// 没过哪条线、差多少。
///
/// 交接档案里把这一组写成「两条可选底线」,但同一节又要求倍率底线按真实倍率比,
/// 所以实际是三条。三条都是独立可选的,少写一条会让「跑批量」那个预设没法表达。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Floors {
    /// 成功率不低于。
    pub min_success_rate: Option<f64>,
    /// 首字 P95 不高于(毫秒)。
    pub max_ttft_p95_ms: Option<u64>,
    /// 真实倍率不高于。
    pub max_rate: Option<f64>,
}

/// 调度偏好：勾了哪几项 + 设了哪几条底线。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Prefs {
    /// 勾中的维度。**空着是合法的** —— 那表示「随便,别乱换」,
    /// 排序会因此排不出名次,现任就一直留着。
    pub axes: Vec<Axis>,
    pub floors: Floors,
}

impl Prefs {
    /// 写代码：稳 + 快,首字 P95 ≤ 3s。
    pub fn coding() -> Self {
        Self {
            axes: vec![Axis::Stable, Axis::Fast],
            floors: Floors {
                max_ttft_p95_ms: Some(3_000),
                ..Default::default()
            },
        }
    }

    /// 跑批量：便宜,成功率 ≥ 95%。
    pub fn batch() -> Self {
        Self {
            axes: vec![Axis::Cheap],
            floors: Floors {
                min_success_rate: Some(0.95),
                ..Default::default()
            },
        }
    }

    /// 对话：快,成功率 ≥ 99%。
    pub fn chat() -> Self {
        Self {
            axes: vec![Axis::Fast],
            floors: Floors {
                min_success_rate: Some(0.99),
                ..Default::default()
            },
        }
    }
}

/// 一个软件的智能调度**驻留状态**。
///
/// # ⛔ 这不是界面上的一个勾
///
/// 0.16.0 之前「智能调度」只是 React 的一个 useState:开着的时候界面变个样,
/// 而没有任何东西在换上游。刷新页面就没了,关掉面板更没了 —— 使用者以为
/// 它一直在盯着,实际上从点下去那一刻起什么都没发生过。
///
/// 落盘之后它才是一个承诺:面板重启接着跑,而**面板没跑的时候它也不跑**
/// (见 `station::run_schedules` 的模块说明)—— 后一半必须在界面上说清楚,
/// 不然使用者会以为关了面板还有人替他挑线路。
///
/// # 一个软件一份
///
/// 三个软件各有各的当前上游,调度自然也各有各的开关与偏好。共用一份的话,
/// 在 Codex 底下按「便宜」调度会把 Claude Code 的上游一起换掉。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Schedule {
    pub client: qb_contract::domain::Client,
    /// 开着没有。
    pub enabled: bool,
    pub prefs: Prefs,
    /// 上一次**真的换了**上游是什么时候。`None` = 开着但还没换过。
    ///
    /// ⛔ 跟「上一次算过」分开:算了没换是常态(迟滞、没有更好的),
    /// 合成一个的话界面会把「一直没换」显示成「调度没在跑」。
    pub last_switch_ms: Option<i64>,
    /// 上一次算完的结论,一句话。界面照实显示,**不自己再算一遍** ——
    /// 再算一遍就是两份判定,而两份判定必然漂移。
    pub last_note: String,
}

impl Schedule {
    /// 刚建出来的那一份:关着,偏好是「写代码」那一档。
    ///
    /// 默认关着是有理由的 —— 自动换上游会花钱,而且换的是使用者正在用的
    /// 那条线。这种事必须由他自己点一下。
    pub fn new(client: qb_contract::domain::Client) -> Self {
        Self {
            client,
            enabled: false,
            prefs: Prefs::coding(),
            last_switch_ms: None,
            last_note: String::new(),
        }
    }
}

/// 参与排序的一条线路。
///
/// **每一项都可能是「没有证据」** —— 取不到就是 `None`,绝不拿 0 顶上:
/// 0 是断言「它很差」,没有数据是「还没有断言」。这两件事在界面上的处置完全不同。
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub route_id: String,
    /// 「便宜」这一维上的值,越小越便宜。口径由 `cheap_values` **整池**选
    /// (24h 实扣单价 / 加权倍率 / 真实倍率),都已按各站的充值比例折过。
    ///
    /// ⛔ 这个数的量级随口径变(实扣单价是 1e-6 那一档),**不能拿去跟底线比** ——
    /// 底线比的是下面那个 [`real_rate`](Self::real_rate)。
    pub rate: Option<f64>,
    /// 底线「倍率不高于」比的那个数:这条线的**真实倍率**(标称 × mult;没有时退回
    /// 加权倍率),按这家站的充值比例折成「每 $1 官方牌价付几元」。造假的站不拉黑 ——
    /// 真实倍率仍然过线的照样留在池子里,只是拿真实值参与比较。
    ///
    /// 0.25.3 及以前底线直接拿 [`rate`](Self::rate) 比:整池走 24h 实扣单价时那是
    /// 1e-6 量级,「倍率不高于 0.5」永远过得去,等于没设。
    pub real_rate: Option<f64>,
    /// 体验分(毫秒,越小越好)。**排序用这个。**
    pub experience_ms: Option<f64>,
    /// 首字 P95。**只给底线筛选用**,不参与排序。
    pub ttft_p95_ms: Option<u64>,
    pub success_rate: Option<f64>,
    /// 降权系数,落在 `(0, 1]`。**429 的落脚点就在这里** ——
    /// 它只降权、不熔断,所以乘在总分上,而不是做成第四个维度
    /// (限流不是「质量差」,混进最弱项会污染那条规则的语义)。
    pub demotion: f64,
    /// 熔断中。不参与竞争,但**照样出现在结果里**,界面写「熔断中」而不是 0 分。
    pub tripped: bool,
}

/// ⛔ **手写的,不是 derive 的。**
///
/// `#[derive(Default)]` 会把 `demotion` 给成 `0.0` —— 那是「打零分」的意思,
/// 于是整池每条线都是 0 分、谁也排不出来,而且**不会报任何错**。
/// 降权的中性值是 1.0(不降权),不是 0。
impl Default for Candidate {
    fn default() -> Self {
        Self {
            route_id: String::new(),
            rate: None,
            real_rate: None,
            experience_ms: None,
            ttft_p95_ms: None,
            success_rate: None,
            demotion: 1.0,
            tripped: false,
        }
    }
}

impl Candidate {
    /// 一条没有任何降权、没有熔断的线路。
    pub fn new(route_id: impl Into<String>) -> Self {
        Self {
            route_id: route_id.into(),
            ..Default::default()
        }
    }

    pub fn rate(mut self, v: f64) -> Self {
        self.rate = Some(v);
        self
    }

    /// 底线「倍率不高于」比的那个数。
    pub fn real_rate(mut self, v: f64) -> Self {
        self.real_rate = Some(v);
        self
    }

    /// 首字 P95。**顺带把体验分也设成它** —— 只有首字证据时体验分本来
    /// 就回落到首字,测试里分开写两遍容易漏掉一个。
    pub fn ttft_p95_ms(mut self, v: u64) -> Self {
        self.ttft_p95_ms = Some(v);
        self.experience_ms = Some(v as f64);
        self
    }

    /// 体验分。**排序真正用的那个数。**
    pub fn experience_ms(mut self, v: f64) -> Self {
        self.experience_ms = Some(v);
        self
    }

    pub fn success_rate(mut self, v: f64) -> Self {
        self.success_rate = Some(v);
        self
    }
}
