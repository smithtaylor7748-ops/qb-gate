//! 站点检验：拿站点**标称**的那几项跟**实测**对质,并把结论换算成可信度。
//!
//! 纯类型与纯算术,不联网 —— 真的去打那几发请求是编排层的事。
//!
//! # ⛔ 只在使用者点的时候跑
//!
//! 这一项要花钱(真打模型)。健康度那一套是免费的(读站点自己的账单日志),
//! 所以可以自动刷新;这一套不行。行内可单跑、勾选可批跑,费用按条数累加。
//!
//! # ⛔ 造假不等于拉黑
//!
//! 检验出对不上,结果只有一个:**把真实倍率算出来,拿真实值去排序。**
//! 这个模块里没有任何「排除」「拉黑」「禁用」的表达,因为那件事不许做 ——
//! DISCLAIMER 写的是「只检测、只如实报告,不拉黑、不替你换站」。
//!
//! # 三根条子与差值片
//!
//! 报告页每一项画三根条子:**标称 / 上次 / 实测**。首次检验时第三根
//! 不画成 0,写「首次检验」—— 0 是断言「测出来是零」,没测过是还没有断言。
//! 差值片同理:[`TrustDelta::First`] 与 [`TrustDelta::Same`] 是两回事。
//!
//! # 这六项是定过的
//!
//! 交接档案只写了「六项构造与比对」没列名单,这份名单是补出来并
//! **经使用者确认**的(2026-09-14):倍率 / 缓存命中 / 上下文窗口 /
//! 最大输出 / 首字延迟 / 成功率。
//!
//! 要加要减只动 [`CheckKind`] 一处 —— 权重、可信度、「哪几项对不上」
//! 全是从它派生的。**别在别处再写一份六项的清单。**

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 检验哪一项。
///
/// 加一项只要动这个枚举和 [`CheckKind::weight`] —— 其余全是派生的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum CheckKind {
    /// 实扣是标称的几倍。**这一项的结论会改写真实倍率**,见 `station::route`。
    Rate,
    /// 缓存命中率。报告页要能摊开 24 小时的原始证据。
    CacheHit,
    /// 上下文窗口。
    ContextWindow,
    /// 最大输出 tokens。
    MaxOutput,
    /// 首字延迟。
    FirstToken,
    /// 成功率。
    SuccessRate,
}

impl CheckKind {
    pub const ALL: [CheckKind; 6] = [
        CheckKind::Rate,
        CheckKind::CacheHit,
        CheckKind::ContextWindow,
        CheckKind::MaxOutput,
        CheckKind::FirstToken,
        CheckKind::SuccessRate,
    ];

    /// 界面上的名字。**清单页直接写名字,不写「4/6 项」** ——
    /// 「哪几项对不上」决定了使用者接下来查什么,一个分数说明不了这件事。
    ///
    /// 纯文本渲染,不许写 Markdown 的星号。
    pub fn label(self) -> &'static str {
        match self {
            CheckKind::Rate => "倍率",
            CheckKind::CacheHit => "缓存命中",
            CheckKind::ContextWindow => "上下文窗口",
            CheckKind::MaxOutput => "最大输出",
            CheckKind::FirstToken => "首字延迟",
            CheckKind::SuccessRate => "成功率",
        }
    }

    /// 这一项在可信度里占多重。
    ///
    /// 倍率算两份:它直接对应你付了多少钱,标错了是收费问题;
    /// 其余几项标错了主要是能力描述问题。这是判断,不是实测值 ——
    /// 但它是**唯一**一处判断,改这里就够了。
    pub fn weight(self) -> u32 {
        match self {
            CheckKind::Rate => 2,
            _ => 1,
        }
    }
}

/// 这一项对没对上。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// 实测与标称相符。
    Matches,
    /// 对不上。**不拉黑** —— 只是把真实值记下来。
    Differs,
    /// 这一轮没测到(站点没提供、或者这一项跑失败了)。
    ///
    /// 跟 [`Differs`](Verdict::Differs) 是两回事:没测到不是证据。
    Unmeasured,
}

/// 一项检验的结果。三个数对应报告页那三根条子。
///
/// TS 侧叫 `AuditCheck`:`qb-probe` 那边已经有一个 `Check`(纯枚举
/// Pass/Fail/Unknown),两个类型会导出成同一个 `Check.ts`,**后写的把先写的
/// 盖掉而且不报错**。实测过一次:`AuditRound.checks` 当时被标成了
/// `Array<"Pass"|"Fail"|"Unknown">`,`npm run types:check` 照样全绿 ——
/// 它比的是「生成的和仓库里的一不一样」,而两次生成都一样地错。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, rename = "AuditCheck")]
pub struct Check {
    pub kind: CheckKind,
    /// 标称：站点自己说的。
    pub claimed: Option<f64>,
    /// 上次：上一轮实测到的。首次检验时是 `None` ——
    /// **第三根条子不画成 0,写「首次检验」。**
    pub previous: Option<f64>,
    /// 实测：这一轮量出来的。
    pub measured: Option<f64>,
    pub verdict: Verdict,
}

impl Check {
    /// 没测到的一项。构造它比手写四个 `None` 安全 ——
    /// 漏掉 `verdict` 会让它默认算成「相符」,等于替站点作证。
    pub fn unmeasured(kind: CheckKind, claimed: Option<f64>, previous: Option<f64>) -> Self {
        Self {
            kind,
            claimed,
            previous,
            measured: None,
            verdict: Verdict::Unmeasured,
        }
    }

    /// 由标称与实测判定,容差用 `station::route` 那一个 —— **只有一处容差**。
    pub fn measured(
        kind: CheckKind,
        claimed: Option<f64>,
        previous: Option<f64>,
        measured: f64,
    ) -> Self {
        let verdict = match claimed {
            Some(c) if c.is_finite() && c > 0.0 && measured.is_finite() => {
                if ((measured / c) - 1.0).abs() <= super::route::RATE_TOLERANCE {
                    Verdict::Matches
                } else {
                    Verdict::Differs
                }
            }
            // 站点没标这一项 —— 测到了也无从对质,不算它对也不算它错。
            _ => Verdict::Unmeasured,
        };
        Self {
            kind,
            claimed,
            previous,
            measured: Some(measured),
            verdict,
        }
    }
}

/// 缓存命中率低到多少该提醒。
///
/// 0.8 —— 取自使用者自己的 station-monitor(`CACHE_ATTENTION_THRESHOLD`)。
/// 低于它不等于站点造假:可能只是你的用法本来就复用得少。所以是「提醒」,
/// 不是「判定」。
pub const CACHE_ATTENTION_THRESHOLD: f64 = 0.80;

/// 这一轮的结论有多少证据撑着。
///
/// ⛔ **必须跟可信度分开。** 可信度 29 分有两种完全不同的来源:
///
/// - 六项都测到了,其中四项对不上 → **证据充分,这站确实有问题**;
/// - 只测到两项,其余没测到 → **证据不足,还不能下结论**。
///
/// 只给一个分数的话,这两种在界面上长得一模一样,而使用者该做的事完全相反:
/// 前者该换站,后者该再跑一轮检验。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceLevel {
    /// 六项都测到了。结论可以照着做决定。
    Sufficient,
    /// 测到了一部分。结论有方向,但别当定论。
    Partial,
    /// 几乎没测到。**这时的低可信度说明不了任何事。**
    Insufficient,
    /// 测到的项里有互相矛盾的 —— 比如站点账单和 API 用量对不上。
    Conflict,
}

/// 可信度跟上一轮比。
///
/// [`First`](TrustDelta::First) 与 [`Same`](TrustDelta::Same) 必须分开:
/// 「首次检验」和「与上次持平」在界面上是两句不同的话。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "by")]
pub enum TrustDelta {
    First,
    Same,
    Up(u8),
    Down(u8),
}

/// 一轮检验。
///
/// **`mult` 记在每一轮上,不是记在线路上** —— 历史里才看得出这家是
/// 越来越离谱还是在收敛。线路上只缓存最近一轮的值,因为排序每次都要用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AuditRound {
    pub at_ms: i64,
    pub checks: Vec<Check>,
    /// 可信度 0–100。
    pub trust: u8,
    /// 实扣是标称的几倍。由 [`CheckKind::Rate`] 那一项算出来,
    /// 没测到就是 `None`(而不是 1.0 —— 那等于断言「属实」)。
    pub mult: Option<f64>,
    /// 这一轮有多少证据撑着。**界面必须跟可信度一起显示** ——
    /// 29 分「证据不足」和 29 分「证据充分」该做的事完全相反。
    pub evidence: EvidenceLevel,
    /// 四类单价各自的对照(输入 / 缓存读 / 缓存写 / 输出)。
    ///
    /// ⛔ **倍率不是一个数。** 只报一个总倍率,会被「输入便宜、输出贵三倍」
    /// 这种结构藏过去 —— 而真实用量里输出往往才是花钱的大头。
    /// 报告页那几行画的就是这个。
    pub rates: Vec<super::pricing::CategoryVerdict>,
    /// 这一轮验的是哪个模型。空串 = 老数据,不知道。
    ///
    /// ⛔ **上面那张价目表离开它就没法读。** 「官方单价 $5」是哪个模型的
    /// $5?不同模型差十倍,看的人没法自己核对,只能选择信或不信 ——
    /// 而「只能选择信」正是这个面板整体想避免的事。
    #[serde(default)]
    pub model: String,
}

impl AuditRound {
    pub fn new(at_ms: i64, checks: Vec<Check>) -> Self {
        Self::with_rates(at_ms, checks, Vec::new(), String::new())
    }

    /// 带上四类单价对照,以及**那张表是哪个模型的**。
    pub fn with_rates(
        at_ms: i64,
        checks: Vec<Check>,
        rates: Vec<super::pricing::CategoryVerdict>,
        model: String,
    ) -> Self {
        let mut round = Self {
            trust: trust_of(&checks),
            mult: mult_of(&checks),
            evidence: EvidenceLevel::Insufficient,
            at_ms,
            checks,
            rates,
            model,
        };
        round.evidence = round.evidence_of();
        round
    }

    /// 这一轮有没有**测出**超收。`None` = 没测到,答不了。
    ///
    /// # 为什么不再按「哪几类」给
    ///
    /// 这里原来是 `overcharged_categories()`,从四类单价对照表里挑出
    /// `overcharging() == Some(true)` 的那几类。但那张表算的是
    /// 「站点公布的倍率 × 站点公布的标称」—— 官方价在算式里被约掉了,
    /// 站点说它便宜表就说它便宜,挑出来的「超收类别」**永远是空的**。
    /// 详见 [`CategoryVerdict`](super::pricing::CategoryVerdict) 的说明。
    ///
    /// 现在唯一的证据是 [`mult`](Self::mult):账单实扣 ÷ 官方成本,再跟标称比。
    /// 它是**整条线一个数**,没有按类别拆的能力 —— 24h 聚合窗口只给一个总实扣,
    /// 拆不出每一类各花了多少。宁可少答一个维度,
    /// 也不要拿一个算不出来的东西冒充答案。
    pub fn overcharging(&self) -> Option<bool> {
        let m = self.mult?;
        (m.is_finite() && m > 0.0).then_some(m > 1.0 + super::route::RATE_TOLERANCE)
    }

    /// 哪几项对不上。**按名字给,不给「4/6 项」** ——
    /// 使用者接下来查什么,取决于是哪几项。
    pub fn mismatched(&self) -> Vec<CheckKind> {
        self.checks
            .iter()
            .filter(|c| c.verdict == Verdict::Differs)
            .map(|c| c.kind)
            .collect()
    }

    /// 这一轮有多少证据撑着。
    ///
    /// 跟 [`trust`](Self::trust) 是两件事:那个是「这站可不可信」,
    /// 这个是「上面那句话本身可不可信」。
    fn evidence_of(&self) -> EvidenceLevel {
        let measured = self
            .checks
            .iter()
            .filter(|c| c.verdict != Verdict::Unmeasured)
            .count();
        let differs = self
            .checks
            .iter()
            .filter(|c| c.verdict == Verdict::Differs)
            .count();
        // 「量出来的项里一大半对不上」= 互相矛盾,不是单纯的不可信。
        if measured >= 2 && differs * 2 > measured {
            return EvidenceLevel::Conflict;
        }
        match measured {
            0 | 1 => EvidenceLevel::Insufficient,
            n if n == CheckKind::ALL.len() => EvidenceLevel::Sufficient,
            _ => EvidenceLevel::Partial,
        }
    }

    /// 缓存命中率低到该提醒了没有。`None` = 这一轮没测缓存。
    pub fn cache_needs_attention(&self) -> Option<bool> {
        let c = self.checks.iter().find(|c| c.kind == CheckKind::CacheHit)?;
        let v = c.measured?;
        Some(v < CACHE_ATTENTION_THRESHOLD)
    }

    /// 跟上一轮比。
    pub fn delta_from(&self, previous: Option<&AuditRound>) -> TrustDelta {
        match previous {
            None => TrustDelta::First,
            Some(p) if p.trust == self.trust => TrustDelta::Same,
            Some(p) if self.trust > p.trust => TrustDelta::Up(self.trust - p.trust),
            Some(p) => TrustDelta::Down(p.trust - self.trust),
        }
    }
}

/// 实扣是标称的几倍。
fn mult_of(checks: &[Check]) -> Option<f64> {
    let c = checks.iter().find(|c| c.kind == CheckKind::Rate)?;
    let (claimed, measured) = (c.claimed?, c.measured?);
    (claimed.is_finite() && claimed > 0.0 && measured.is_finite() && measured >= 0.0)
        .then(|| measured / claimed)
}

/// 可信度：相符的那几项的权重 ÷ **全部六项**的权重。
///
/// 分母是六项全额,不是「测到的那几项」—— 只测了两项就给 100 分的话,
/// 「可信度」量的就变成了「我们测了多少」,而不是「这家可不可信」。
/// 没测到的一项因此会压低分数,这是有意的:未核实本来就不该读作可信。
pub fn trust_of(checks: &[Check]) -> u8 {
    let total: u32 = CheckKind::ALL.iter().map(|k| k.weight()).sum();
    let credit: u32 = checks
        .iter()
        .filter(|c| c.verdict == Verdict::Matches)
        .map(|c| c.kind.weight())
        .sum();
    ((credit as f64 / total as f64) * 100.0).round().min(100.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_700_000_000_000;

    fn matching(kind: CheckKind) -> Check {
        Check::measured(kind, Some(1.0), None, 1.0)
    }

    #[test]
    fn a_low_trust_score_means_two_different_things_and_evidence_tells_them_apart() {
        // ⛔ 这一条是这个类型存在的全部理由。
        // 甲：六项全测到、四项对不上 —— 证据充分，这站确实有问题，该换站。
        let caught = AuditRound::new(
            T0,
            CheckKind::ALL
                .iter()
                .enumerate()
                .map(|(i, k)| {
                    if i < 4 {
                        Check::measured(*k, Some(1.0), None, 2.0) // 对不上
                    } else {
                        matching(*k)
                    }
                })
                .collect(),
        );
        // 乙：只测到一项 —— 证据不足，该再跑一轮，而不是换站。
        let untested = AuditRound::new(
            T0,
            CheckKind::ALL
                .iter()
                .enumerate()
                .map(|(i, k)| {
                    if i == 0 {
                        matching(*k)
                    } else {
                        Check::unmeasured(*k, Some(1.0), None)
                    }
                })
                .collect(),
        );

        // 两者的可信度都不高，但含义完全相反。
        assert_eq!(caught.evidence, EvidenceLevel::Conflict);
        assert_eq!(untested.evidence, EvidenceLevel::Insufficient);
    }

    #[test]
    fn all_six_measured_and_matching_is_sufficient_evidence() {
        let r = AuditRound::new(T0, CheckKind::ALL.iter().copied().map(matching).collect());
        assert_eq!(r.evidence, EvidenceLevel::Sufficient);
        assert_eq!(r.trust, 100);
    }

    #[test]
    fn a_cache_hit_rate_below_the_threshold_is_a_hint_not_a_verdict() {
        // 低命中不等于造假 —— 可能只是你的用法复用得少。所以是「提醒」。
        let low = AuditRound::new(
            T0,
            vec![Check::measured(CheckKind::CacheHit, None, None, 0.42)],
        );
        assert_eq!(low.cache_needs_attention(), Some(true));
        let fine = AuditRound::new(
            T0,
            vec![Check::measured(CheckKind::CacheHit, None, None, 0.91)],
        );
        assert_eq!(fine.cache_needs_attention(), Some(false));
        // 没测过就答不了 —— 不是「没问题」。
        let never = AuditRound::new(T0, vec![Check::unmeasured(CheckKind::CacheHit, None, None)]);
        assert_eq!(never.cache_needs_attention(), None);
    }

    #[test]
    fn all_six_matching_reads_as_fully_trusted() {
        let checks: Vec<Check> = CheckKind::ALL.iter().copied().map(matching).collect();
        assert_eq!(trust_of(&checks), 100);
    }

    #[test]
    fn unmeasured_items_lower_trust_rather_than_being_ignored() {
        // 只测了两项就给 100 分的话,「可信度」量的就是「我们测了多少」,
        // 而不是「这家可不可信」。未核实不该读作可信。
        let checks = vec![matching(CheckKind::Rate), matching(CheckKind::CacheHit)];
        // 倍率 2 份 + 缓存 1 份 = 3,全额 7 份。
        assert_eq!(trust_of(&checks), 43);
        assert!(trust_of(&checks) < 100);
    }

    #[test]
    fn nothing_measured_is_zero_trust_but_that_is_not_an_accusation() {
        // 0 分说的是「没有任何核实」,清单页对这种线路画的是虚线框 + 「—」,
        // 不是把 0 当成分数展示。
        let checks: Vec<Check> = CheckKind::ALL
            .iter()
            .map(|k| Check::unmeasured(*k, Some(1.0), None))
            .collect();
        assert_eq!(trust_of(&checks), 0);
        let round = AuditRound::new(T0, checks);
        assert!(round.mismatched().is_empty(), "没测到不等于对不上");
        assert_eq!(round.mult, None, "没测到不该算成 mult=1.0");
    }

    #[test]
    fn the_documented_overcharging_case_produces_the_right_mult() {
        // 标 ×0.20 实收 ×0.26 → mult 1.3。
        let checks = vec![Check::measured(CheckKind::Rate, Some(0.20), None, 0.26)];
        let round = AuditRound::new(T0, checks);
        assert!((round.mult.unwrap() - 1.3).abs() < 1e-9);
        assert_eq!(round.mismatched(), vec![CheckKind::Rate]);
    }

    #[test]
    fn a_faking_station_is_repriced_and_never_excluded() {
        // 这个模块里压根没有「排除」这个概念 —— 结论只有一个:算出真实倍率。
        let round = AuditRound::new(
            T0,
            vec![Check::measured(CheckKind::Rate, Some(0.20), None, 0.26)],
        );
        let trust = crate::station::route::rate_trust(Some(0.20), round.mult);
        assert_eq!(trust.overcharging(), Some(true));
        assert!(
            trust.rate_for_ranking().is_some(),
            "它照样有一个参与排序的倍率"
        );
    }

    #[test]
    fn mismatches_are_reported_by_name_not_as_a_fraction() {
        // 「哪几项对不上」决定使用者接下来查什么,「4/6 项」说明不了这件事。
        let checks = vec![
            Check::measured(CheckKind::Rate, Some(1.0), None, 2.0),
            matching(CheckKind::CacheHit),
            Check::measured(CheckKind::MaxOutput, Some(8192.0), None, 4096.0),
        ];
        let round = AuditRound::new(T0, checks);
        assert_eq!(
            round.mismatched(),
            vec![CheckKind::Rate, CheckKind::MaxOutput]
        );
        let names: Vec<&str> = round.mismatched().iter().map(|k| k.label()).collect();
        assert_eq!(names, vec!["倍率", "最大输出"]);
    }

    #[test]
    fn a_first_round_is_not_the_same_as_an_unchanged_one() {
        // 「首次检验」和「与上次持平」在界面上是两句不同的话。
        let first = AuditRound::new(T0, vec![matching(CheckKind::Rate)]);
        assert_eq!(first.delta_from(None), TrustDelta::First);
        let second = AuditRound::new(T0 + 1, vec![matching(CheckKind::Rate)]);
        assert_eq!(second.delta_from(Some(&first)), TrustDelta::Same);
    }

    #[test]
    fn trust_moves_are_reported_with_their_size() {
        // 界面上那个差值片:「较上次 ↑6」。
        let before = AuditRound::new(T0, vec![matching(CheckKind::Rate)]); // 2/7 → 29
        let after = AuditRound::new(
            T0 + 1,
            vec![matching(CheckKind::Rate), matching(CheckKind::CacheHit)],
        ); // 3/7 → 43
        assert_eq!(after.delta_from(Some(&before)), TrustDelta::Up(14));
        assert_eq!(before.delta_from(Some(&after)), TrustDelta::Down(14));
    }

    #[test]
    fn a_station_that_advertises_nothing_cannot_be_contradicted() {
        // 站点没标这一项,测到了也无从对质 —— 不算它对,也不算它错。
        let c = Check::measured(CheckKind::MaxOutput, None, None, 4096.0);
        assert_eq!(c.verdict, Verdict::Unmeasured);
        assert!(AuditRound::new(T0, vec![c]).mismatched().is_empty());
    }

    #[test]
    fn small_differences_are_within_tolerance_and_share_one_definition() {
        // 容差只有一处 —— route::RATE_TOLERANCE。两处各写各的必然漂移。
        let within = Check::measured(CheckKind::Rate, Some(1.0), None, 1.02);
        assert_eq!(within.verdict, Verdict::Matches);
        let outside = Check::measured(CheckKind::Rate, Some(1.0), None, 1.5);
        assert_eq!(outside.verdict, Verdict::Differs);
    }

    #[test]
    fn the_previous_reading_is_carried_so_the_report_can_draw_three_bars() {
        // 三根条子:标称 / 上次 / 实测。首次检验时第三根不画成 0。
        let first = Check::measured(CheckKind::FirstToken, Some(800.0), None, 950.0);
        assert_eq!(first.previous, None);
        let second = Check::measured(CheckKind::FirstToken, Some(800.0), Some(950.0), 810.0);
        assert_eq!(second.previous, Some(950.0));
        assert_eq!(second.verdict, Verdict::Matches);
    }
}
