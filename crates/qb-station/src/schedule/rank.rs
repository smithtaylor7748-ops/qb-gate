//! 排序：**勾的几项里,最弱的那一项最好的那条赢。**
//!
//! # 为什么不是加权平均(原方案是三根推子)
//!
//! 三条实测把推子否掉了:
//!
//! 1. 三条线稳定度 91 / 92 / 92 时,把「稳定」从 0 拉到 100,**排名一个字不变**,
//!    而界面上写着「稳定占 50%」。推子在撒谎 —— 它让使用者以为自己在调节什么。
//! 2. 没有迟滞时,两条几乎持平的线路在 200 次请求里换手 53 次,prompt 缓存全废。
//! 3. 稳 60(四成请求失败)配速度满分,跟稳 95 那条只差 0.5 分 ——
//!    可靠性被定价了,而可靠性不该可以拿速度买。
//!
//! 最弱项规则没有这三个毛病：一项垫底就是垫底,**别的项再好也补不回来**。
//!
//! # 为什么不是名次制(Borda)
//!
//! 实测【便宜 + 快】它选了**最慢**的那条(便宜第 1 · 快第 4)。
//! 「一项第一压过一项垫底」跟加权和是同一个病 —— 名次丢掉了差距有多大。
//!
//! # 为什么归一化不能用 min-max
//!
//! 池子里只有两条线时 min-max **永远输出 1.0 和 0.0**,不管实际差 0.1% 还是 10 倍。
//! 差距每次被重新拉满,[`HYSTERESIS`] 门槛就永远失效 —— 实测 0% / 5% / 15%
//! 三档换手次数一模一样。
//!
//! 改成**跟池里最好那条的比值**之后：`0% → 53 次,3% → 29 次,8% → 4 次`。
//! 比值保留了「差多少」,迟滞才有东西可以卡。

use super::{Axis, Candidate, Floors, Prefs};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 挑战者要领先现任多少才换。
///
/// **换上游会作废 prompt 缓存** —— 这个代价不写在界面上,使用者就会以为
/// 「一直选最优的」是免费的。8% 是实测值,见模块头第 2 条。
pub const HYSTERESIS: f64 = 0.08;

/// 一维上全场差距小于这个数,就当这一维分不出高下。
///
/// 归一化之后差距 = `1 - 最差那条的比值`。稳定度 91 / 92 时比值 0.989,
/// 差距 1.1% —— 低于门槛,于是这一维**如实说「分不出高下」并且不参与排序**,
/// 而不是假装它在起作用。这是模块头第 1 条实测的直接落地。
///
/// 跟 [`HYSTERESIS`] 是两件事：这个决定「这一维算不算数」,
/// 那个决定「算数的前提下要不要换」。
pub const INDISTINGUISHABLE_SPREAD: f64 = 0.02;

/// 一条线路在某一维上的归一化得分。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "value")]
pub enum AxisScore {
    /// 跟池里最好那条的比值,落在 `[0, 1]`,1.0 就是最好的那条。
    Ratio(f64),
    /// 这条线在这一维上没有证据。**不给 0** ——
    /// 0 是断言「它很差」,没有数据是「还没有断言」。
    Missing,
}

/// 没过某条底线,以及差在哪。界面要把这个原样写出来。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FloorMiss {
    pub axis: Axis,
    pub actual: f64,
    pub limit: f64,
}

/// 排序结果里的一行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Row {
    pub route_id: String,
    /// 最弱项得分(已乘降权系数)。`None` = 选中的维度里有一项没有证据,
    /// 或者没有任何维度可排。
    pub score: Option<f64>,
    /// 每一个**参与排序**的维度上的得分。分不出高下的维度不在这里。
    pub per_axis: Vec<(Axis, AxisScore)>,
    /// 决定了它总分的那一维 —— 界面上「它差在哪」就写这个。
    pub weakest: Option<Axis>,
    /// 选中但这条线没有数据的维度。
    pub missing: Vec<Axis>,
    /// 没过的底线。**过不了的线不会被拉黑**,只是不参与竞争。
    pub failed_floors: Vec<FloorMiss>,
    /// 能不能参与竞争。
    pub eligible: bool,
    /// 熔断中。界面写「熔断中」,**不写 0 分**。
    pub tripped: bool,
}

/// 排序的完整结论。**每一条都要能在界面上说清楚,不许只给一个赢家。**
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Ranking {
    /// 按最弱项得分降序;没得分的排在后面。同分按 `route_id` 定序,
    /// 保证同样的输入永远给同样的输出 —— 否则迟滞会被随机顺序绕过去。
    pub rows: Vec<Row>,
    pub winner: Option<String>,
    /// 这一轮真的换了上游没有。
    pub switched: bool,
    /// 现任因为迟滞被留下了 —— 挑战者更好,但没好够 [`HYSTERESIS`]。
    pub held_by_hysteresis: bool,
    /// 这几维全场差距太小,**已如实排除,没有参与排序**。
    pub indistinguishable: Vec<Axis>,
    /// 底线把所有还活着的线路都卡掉了,已经放开。**绝不制造死局。**
    pub floors_relaxed: bool,
    /// 全熔断了,已经放开。同样绝不制造死局 —— 上层照样要如实报错。
    pub breakers_relaxed: bool,
}

/// 跟池里最好那条比。返回 `[0, 1]`,1.0 是最好的那条。
///
/// 「越小越好」的维度(倍率、首字延迟)用 `最好 ÷ 我`,
/// 「越大越好」的(成功率)用 `我 ÷ 最好`。两边掰到同一个方向上,
/// 最弱项规则才能跨维度比较。
///
/// **成功率 0 要给 0 分,不是给 `None`** —— 那是量出来的断言,不是缺证据。
/// 倍率或延迟为 0 则是没意义的读数,当作没有证据。
fn ratio(axis: Axis, mine: f64, best: f64) -> Option<f64> {
    if !mine.is_finite() || !best.is_finite() {
        return None;
    }
    let r = if axis.smaller_is_better() {
        if mine <= 0.0 || best <= 0.0 {
            return None;
        }
        best / mine
    } else {
        if best <= 0.0 || mine < 0.0 {
            return None;
        }
        mine / best
    };
    r.is_finite().then(|| r.clamp(0.0, 1.0))
}

/// 一条线路在一维上的原始值。取不到就是 `None`,绝不拿 0 顶上。
fn raw(c: &Candidate, axis: Axis) -> Option<f64> {
    match axis {
        Axis::Cheap => c.rate,
        // ⛔ 排序读体验分,不读首字。只看首字会把「首字最快但一次回答
        // 要吐 95 秒」的那条排第一 —— 实测栽过,见 Window::experience_ms。
        Axis::Fast => c.experience_ms,
        Axis::Stable => c.success_rate,
    }
}

/// 这条线没过哪几条底线。
///
/// **倍率按真实倍率比,不按站点自己标的** —— `Candidate::rate` 进来时
/// 已经是 `标称 × mult` 了,见 `station::route`。
fn floor_misses(c: &Candidate, f: &Floors) -> Vec<FloorMiss> {
    let mut out = Vec::new();
    if let (Some(limit), Some(actual)) = (f.min_success_rate, c.success_rate) {
        if actual < limit {
            out.push(FloorMiss {
                axis: Axis::Stable,
                actual,
                limit,
            });
        }
    }
    // 底线按**首字 P95**,不按体验分:界面上使用者设的就是「首字 ≤ 3 秒」。
    if let (Some(limit), Some(actual)) = (f.max_ttft_p95_ms, c.ttft_p95_ms) {
        if actual > limit {
            out.push(FloorMiss {
                axis: Axis::Fast,
                actual: actual as f64,
                limit: limit as f64,
            });
        }
    }
    if let (Some(limit), Some(actual)) = (f.max_rate, c.rate) {
        if actual > limit {
            out.push(FloorMiss {
                axis: Axis::Cheap,
                actual,
                limit,
            });
        }
    }
    out
}

/// 排序。`incumbent` 是当前正在用的那条 —— 迟滞要拿它比。
pub fn rank(candidates: &[Candidate], prefs: &Prefs, incumbent: Option<&str>) -> Ranking {
    // ---- 一、谁有资格参与竞争
    //
    // 两道放开阀,顺序不能反:先放熔断,再放底线。全熔断时如果不先放开,
    // 底线那一步会看到一个空池子,然后把底线也放开 —— 两条信息糊成一条,
    // 界面就说不清到底是「都挂了」还是「都不达标」。
    let misses: Vec<Vec<FloorMiss>> = candidates
        .iter()
        .map(|c| floor_misses(c, &prefs.floors))
        .collect();

    let breakers_relaxed = !candidates.is_empty() && candidates.iter().all(|c| c.tripped);
    let live: Vec<bool> = candidates
        .iter()
        .map(|c| !c.tripped || breakers_relaxed)
        .collect();

    let any_live_passing = (0..candidates.len()).any(|i| live[i] && misses[i].is_empty());
    let floors_relaxed = !candidates.is_empty() && !any_live_passing;

    let eligible: Vec<bool> = (0..candidates.len())
        .map(|i| live[i] && (floors_relaxed || misses[i].is_empty()))
        .collect();
    let pool: Vec<usize> = (0..candidates.len()).filter(|&i| eligible[i]).collect();

    // ---- 二、每一维在参与竞争的那些线里找最好的那条,再算比值
    //
    // 池子里只剩一条时不谈「分不出高下」—— 没有第二条可比,那不是平局。
    let mut indistinguishable = Vec::new();
    let mut ranked_axes: Vec<(Axis, f64)> = Vec::new();
    for &axis in &prefs.axes {
        let vals: Vec<f64> = pool
            .iter()
            .filter_map(|&i| raw(&candidates[i], axis))
            .collect();
        if vals.is_empty() {
            continue;
        }
        let best = if axis.smaller_is_better() {
            vals.iter().copied().fold(f64::INFINITY, f64::min)
        } else {
            vals.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        };
        let worst_ratio = pool
            .iter()
            .filter_map(|&i| raw(&candidates[i], axis))
            .filter_map(|v| ratio(axis, v, best))
            .fold(1.0f64, f64::min);
        // 比的是**有读数的那几条**,不是整个池子。只有一条线测过时差距算出来是 0,
        // 那不是「大家一样好」,那是只有一个样本 —— 当成平局会把唯一测过的那条
        // 一起踢出排序,于是「测过的」和「没测过的」并列,谁也没得分。
        if vals.len() > 1 && (1.0 - worst_ratio) < INDISTINGUISHABLE_SPREAD {
            indistinguishable.push(axis);
        } else {
            ranked_axes.push((axis, best));
        }
    }

    // ---- 三、最弱项
    //
    // 选中的维度里只要有一项没有证据,就不给总分 —— 用 0 顶上会把
    // 「还没测」说成「很差」,而这两件事在界面上的处置完全相反。
    let mut rows: Vec<Row> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let mut per_axis = Vec::new();
        let mut missing = Vec::new();
        let mut weakest: Option<(Axis, f64)> = None;
        let mut complete = true;
        for &(axis, best) in &ranked_axes {
            match raw(c, axis).and_then(|v| ratio(axis, v, best)) {
                Some(r) => {
                    per_axis.push((axis, AxisScore::Ratio(r)));
                    if weakest.is_none_or(|(_, w)| r < w) {
                        weakest = Some((axis, r));
                    }
                }
                None => {
                    per_axis.push((axis, AxisScore::Missing));
                    missing.push(axis);
                    complete = false;
                }
            }
        }
        // 429 的落脚点就在这里。**它只降权,不熔断** —— 做成第四个维度会污染
        // 最弱项的语义(被限流不等于质量差),所以乘在总分上。
        let score = if complete && !ranked_axes.is_empty() {
            weakest.map(|(_, w)| w * c.demotion.clamp(0.0, 1.0))
        } else {
            None
        };
        rows.push(Row {
            route_id: c.route_id.clone(),
            score,
            per_axis,
            weakest: weakest.map(|(a, _)| a),
            missing,
            failed_floors: misses[i].clone(),
            eligible: eligible[i],
            tripped: c.tripped,
        });
    }

    // 同分按 id 定序：同样的输入必须给同样的输出,否则迟滞会被随机顺序绕过去。
    rows.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.route_id.cmp(&b.route_id))
    });

    // ---- 四、迟滞
    let best = rows
        .iter()
        .find(|r| r.eligible && r.score.is_some())
        .map(|r| (r.route_id.clone(), r.score.unwrap_or_default()));
    let incumbent_score = incumbent.and_then(|id| {
        rows.iter()
            .find(|r| r.route_id == id && r.eligible && r.score.is_some())
            .map(|r| r.score.unwrap_or_default())
    });

    let (winner, switched, held) = match (best, incumbent_score) {
        (Some((bid, bs)), Some(is)) => {
            let inc = incumbent.unwrap_or_default().to_string();
            if bid == inc {
                (Some(inc), false, false)
            } else if bs > is * (1.0 + HYSTERESIS) {
                (Some(bid), true, false)
            } else {
                // 挑战者更好,但没好够 —— 换一次 prompt 缓存就全废,不值。
                (Some(inc), false, true)
            }
        }
        // 现任没资格了(熔断 / 没过线 / 数据缺),直接换,不谈迟滞。
        (Some((bid, _)), None) => {
            let switched = incumbent != Some(bid.as_str());
            (Some(bid), switched, false)
        }
        // 一条都排不出名次(没有维度可排,或者数据全缺)。
        // **不许死局**：现任还能用就留着,否则挑第一条能用的。
        (None, _) => {
            let keep = incumbent
                .filter(|id| rows.iter().any(|r| r.route_id == *id && r.eligible))
                .map(str::to_string);
            let fallback = keep.or_else(|| {
                rows.iter()
                    .filter(|r| r.eligible)
                    .map(|r| r.route_id.clone())
                    .min()
            });
            let switched = fallback.as_deref() != incumbent && fallback.is_some();
            (fallback, switched, false)
        }
    };

    Ranking {
        rows,
        winner,
        switched,
        held_by_hysteresis: held,
        indistinguishable,
        floors_relaxed,
        breakers_relaxed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::{Axis, Candidate, Floors, Prefs};

    fn prefs(axes: &[Axis]) -> Prefs {
        Prefs {
            axes: axes.to_vec(),
            floors: Floors::default(),
        }
    }

    fn row<'a>(r: &'a Ranking, id: &str) -> &'a Row {
        r.rows.iter().find(|x| x.route_id == id).expect("行不存在")
    }

    #[test]
    fn the_worst_axis_decides_so_reliability_cannot_be_bought_with_speed() {
        // 实测 3：加权平均下「稳 60 配速度满分」跟「稳 95」只差 0.5 分 ——
        // 可靠性被定价了。最弱项规则里,四成请求失败就是四成请求失败,
        // 快一点补不回来。
        let cs = [
            Candidate::new("失败四成但最快")
                .success_rate(0.60)
                .ttft_p95_ms(500),
            Candidate::new("稳").success_rate(0.95).ttft_p95_ms(550),
            Candidate::new("次稳").success_rate(0.94).ttft_p95_ms(600),
        ];
        let r = rank(&cs, &prefs(&[Axis::Stable, Axis::Fast]), None);
        assert_eq!(r.winner.as_deref(), Some("稳"));
        // 而且界面上说得出它差在哪。
        assert_eq!(row(&r, "失败四成但最快").weakest, Some(Axis::Stable));
    }

    #[test]
    fn the_cheapest_line_does_not_win_by_being_catastrophically_slow() {
        // 名次制(Borda)在【便宜 + 快】上选了最慢的那条(便宜第 1 · 快第 4)——
        // 名次丢掉了「差多少」。比值保留了,所以垫底就是垫底。
        let cs = [
            Candidate::new("最便宜但最慢").rate(0.10).ttft_p95_ms(4000),
            Candidate::new("均衡").rate(0.22).ttft_p95_ms(800),
        ];
        let r = rank(&cs, &prefs(&[Axis::Cheap, Axis::Fast]), None);
        assert_eq!(r.winner.as_deref(), Some("均衡"));
    }

    #[test]
    fn ranking_uses_the_experience_score_while_the_floor_still_uses_first_token() {
        // 实测(sub2guard ㉚):2690 首字 1.4 秒全组最快,却因为 107.9 ms/token
        // 一次回答要 95.8 秒;2681 首字 4.7 秒但快 5 倍。只看首字会选错。
        let mut fast_start = Candidate::new("首字最快但卡");
        fast_start.ttft_p95_ms = Some(1_400);
        fast_start.experience_ms = Some(95_800.0);
        let mut fast_finish = Candidate::new("首字慢但流畅");
        fast_finish.ttft_p95_ms = Some(4_700);
        fast_finish.experience_ms = Some(19_000.0);

        let cs = [fast_start, fast_finish];
        let r = rank(&cs, &prefs(&[Axis::Fast]), None);
        assert_eq!(
            r.winner.as_deref(),
            Some("首字慢但流畅"),
            "排序又只看首字了"
        );

        // 而底线仍然按首字筛 —— 使用者设的是「首字 ≤ 3 秒」。
        let p = Prefs {
            axes: vec![Axis::Fast],
            floors: Floors {
                max_ttft_p95_ms: Some(3_000),
                ..Default::default()
            },
        };
        let r = rank(&cs, &p, None);
        assert_eq!(
            r.winner.as_deref(),
            Some("首字最快但卡"),
            "底线该按首字筛：只有它的首字过了 3 秒线"
        );
        assert_eq!(row(&r, "首字慢但流畅").failed_floors[0].axis, Axis::Fast);
    }

    #[test]
    fn a_near_tie_is_reported_as_such_instead_of_being_stretched_to_full_scale() {
        // min-max 在两条线的池子里**永远**输出 1.0 和 0.0,不管实际差 0.1%
        // 还是 10 倍 —— 迟滞门槛因此永远失效。比值归一化不会:
        // 差 1% 就是差 1%,低于门槛就如实说「分不出高下」。
        let close = [
            Candidate::new("a").rate(1.00),
            Candidate::new("b").rate(1.01),
        ];
        let r = rank(&close, &prefs(&[Axis::Cheap]), None);
        assert_eq!(r.indistinguishable, vec![Axis::Cheap]);
        // 分不出高下的那一维不参与排序,所以谁也没有得分。
        assert!(r.rows.iter().all(|x| x.score.is_none()));
        // 但**不许死局** —— 照样得给出一条能用的。
        assert!(r.winner.is_some());

        let far = [
            Candidate::new("a").rate(1.00),
            Candidate::new("b").rate(10.0),
        ];
        let r = rank(&far, &prefs(&[Axis::Cheap]), None);
        assert!(r.indistinguishable.is_empty());
        assert_eq!(r.winner.as_deref(), Some("a"));
    }

    #[test]
    fn an_axis_the_whole_pool_agrees_on_does_not_pretend_to_rank() {
        // 实测 1：三条线稳定度 91 / 92 / 92,把「稳定」拉满排名一个字不变,
        // 而界面写着「稳定占 50%」。推子在撒谎。这里如实说它分不出高下。
        let cs = [
            Candidate::new("a").success_rate(0.91).rate(0.5),
            Candidate::new("b").success_rate(0.92).rate(0.8),
            Candidate::new("c").success_rate(0.92).rate(1.2),
        ];
        let r = rank(&cs, &prefs(&[Axis::Stable, Axis::Cheap]), None);
        assert_eq!(r.indistinguishable, vec![Axis::Stable]);
        // 稳定度退出之后由倍率决定,最便宜的赢。
        assert_eq!(r.winner.as_deref(), Some("a"));
        assert!(row(&r, "a")
            .per_axis
            .iter()
            .all(|(ax, _)| *ax != Axis::Stable));
    }

    #[test]
    fn hysteresis_keeps_the_incumbent_until_the_challenger_is_clearly_better() {
        // 实测 2：没有迟滞时两条几乎持平的线在 200 次请求里换手 53 次,
        // prompt 缓存全废。换上游是有代价的,所以要领先够 8% 才换。
        let near = [
            Candidate::new("现任").rate(1.00),
            Candidate::new("挑战者").rate(0.95), // 好 5.3%,不够
        ];
        let r = rank(&near, &prefs(&[Axis::Cheap]), Some("现任"));
        assert_eq!(r.winner.as_deref(), Some("现任"));
        assert!(r.held_by_hysteresis);
        assert!(!r.switched);

        let clear = [
            Candidate::new("现任").rate(1.00),
            Candidate::new("挑战者").rate(0.85), // 好 17.6%,够了
        ];
        let r = rank(&clear, &prefs(&[Axis::Cheap]), Some("现任"));
        assert_eq!(r.winner.as_deref(), Some("挑战者"));
        assert!(r.switched);
        assert!(!r.held_by_hysteresis);
    }

    #[test]
    fn missing_evidence_scores_nothing_rather_than_zero() {
        // 0 是断言「它很差」,没有数据是「还没有断言」。给 0 分等于
        // 拿没测过的线去垫底,而使用者看到的是一个像样的数字。
        let cs = [
            Candidate::new("测过").rate(1.0),
            Candidate::new("没测过"), // rate 是 None
        ];
        let r = rank(&cs, &prefs(&[Axis::Cheap]), None);
        assert_eq!(row(&r, "没测过").score, None);
        assert_eq!(row(&r, "没测过").missing, vec![Axis::Cheap]);
        assert_eq!(
            row(&r, "没测过").per_axis,
            vec![(Axis::Cheap, AxisScore::Missing)]
        );
        assert_eq!(r.winner.as_deref(), Some("测过"));
    }

    #[test]
    fn floors_filter_before_ranking_and_say_what_was_missed() {
        let cs = [
            Candidate::new("便宜但不稳").rate(0.1).success_rate(0.80),
            Candidate::new("贵但稳").rate(1.0).success_rate(0.99),
        ];
        let p = Prefs {
            axes: vec![Axis::Cheap],
            floors: Floors {
                min_success_rate: Some(0.95),
                ..Default::default()
            },
        };
        let r = rank(&cs, &p, None);
        assert!(!r.floors_relaxed);
        assert_eq!(r.winner.as_deref(), Some("贵但稳"));
        let bad = row(&r, "便宜但不稳");
        assert!(!bad.eligible);
        assert_eq!(bad.failed_floors.len(), 1);
        assert_eq!(bad.failed_floors[0].axis, Axis::Stable);
        assert_eq!(bad.failed_floors[0].actual, 0.80);
        assert_eq!(bad.failed_floors[0].limit, 0.95);
    }

    #[test]
    fn floors_that_exclude_everyone_are_relaxed_rather_than_deadlocking() {
        // 「底线卡到一条不剩时不许死局：照常走,但每行标『没过线 + 差在哪』」。
        let cs = [
            Candidate::new("a").rate(0.5).success_rate(0.80),
            Candidate::new("b").rate(0.9).success_rate(0.70),
        ];
        let p = Prefs {
            axes: vec![Axis::Cheap],
            floors: Floors {
                min_success_rate: Some(0.99),
                ..Default::default()
            },
        };
        let r = rank(&cs, &p, None);
        assert!(r.floors_relaxed);
        assert_eq!(r.winner.as_deref(), Some("a"));
        // 放开了,但没假装它们过线了。
        assert!(r
            .rows
            .iter()
            .all(|x| x.eligible && !x.failed_floors.is_empty()));
    }

    #[test]
    fn a_tripped_route_still_shows_up_but_cannot_win() {
        // 「熔断的线路同理,写『熔断中』不写 0」。
        let mut tripped = Candidate::new("熔断中").rate(0.01);
        tripped.tripped = true;
        let cs = [tripped, Candidate::new("正常").rate(1.0)];
        let r = rank(&cs, &prefs(&[Axis::Cheap]), None);
        assert_eq!(r.winner.as_deref(), Some("正常"));
        let t = row(&r, "熔断中");
        assert!(t.tripped);
        assert!(!t.eligible);
        assert!(!r.breakers_relaxed);
    }

    #[test]
    fn everything_tripped_relaxes_the_breakers_instead_of_returning_nothing() {
        // 「全熔断时每个客户端仍留一条线路,如实报错,不制造死局」。
        let cs: Vec<Candidate> = ["a", "b"]
            .iter()
            .map(|id| {
                let mut c = Candidate::new(*id).rate(1.0);
                c.tripped = true;
                c
            })
            .collect();
        let r = rank(&cs, &prefs(&[Axis::Cheap]), None);
        assert!(r.breakers_relaxed);
        assert!(r.winner.is_some());
        // 放开了,但每一行仍然如实标着熔断中 —— 上层要照这个报错。
        assert!(r.rows.iter().all(|x| x.tripped));
    }

    #[test]
    fn rate_limiting_demotes_the_total_instead_of_becoming_a_fourth_axis() {
        // 429 只降权不熔断。做成第四个维度会污染最弱项的语义:
        // 被限流不等于质量差。
        let plain = [
            Candidate::new("a").rate(1.00),
            Candidate::new("b").rate(1.10),
        ];
        assert_eq!(
            rank(&plain, &prefs(&[Axis::Cheap]), None).winner.as_deref(),
            Some("a")
        );

        let mut demoted = Candidate::new("a").rate(1.00);
        demoted.demotion = 0.5; // 刚吃过 429
        let cs = [demoted, Candidate::new("b").rate(1.10)];
        let r = rank(&cs, &prefs(&[Axis::Cheap]), None);
        assert_eq!(r.winner.as_deref(), Some("b"));
        // 降权改的是总分,不是它在那一维上的读数 —— 倍率还是 1.00 那条最便宜。
        assert_eq!(
            row(&r, "a").per_axis,
            vec![(Axis::Cheap, AxisScore::Ratio(1.0))]
        );
        assert_eq!(row(&r, "a").score, Some(0.5));
    }

    #[test]
    fn no_axes_selected_means_do_not_churn() {
        // 一项都没勾 = 「随便,别乱换」。现任还能用就一直留着。
        let cs = [Candidate::new("a").rate(1.0), Candidate::new("b").rate(0.1)];
        let r = rank(&cs, &prefs(&[]), Some("a"));
        assert_eq!(r.winner.as_deref(), Some("a"));
        assert!(!r.switched);
    }

    #[test]
    fn the_same_input_always_gives_the_same_winner() {
        // 同分不定序的话,迟滞会被随机顺序绕过去 —— 换手次数又回到 53。
        let cs = [
            Candidate::new("b").rate(1.0).success_rate(0.9),
            Candidate::new("a").rate(1.0).success_rate(0.9),
            Candidate::new("c").rate(1.0).success_rate(0.9),
        ];
        let p = prefs(&[Axis::Cheap, Axis::Stable]);
        let first = rank(&cs, &p, None);
        for _ in 0..8 {
            assert_eq!(rank(&cs, &p, None), first);
        }
        // 全场持平 → 两维都分不出高下 → 按 id 挑第一条,而不是看谁排在数组前面。
        assert_eq!(first.winner.as_deref(), Some("a"));
    }

    #[test]
    fn a_default_candidate_is_undemoted_because_zero_would_mean_bottom_score() {
        // derive 出来的 Default 会把 demotion 给成 0.0 —— 整池每条线都是 0 分、
        // 谁也排不出来,而且不报任何错。降权的中性值是 1.0。
        assert_eq!(Candidate::default().demotion, 1.0);
        let cs = [
            Candidate {
                route_id: "a".into(),
                rate: Some(1.0),
                ..Default::default()
            },
            Candidate {
                route_id: "b".into(),
                rate: Some(2.0),
                ..Default::default()
            },
        ];
        let r = rank(&cs, &prefs(&[Axis::Cheap]), None);
        assert_eq!(r.winner.as_deref(), Some("a"));
        assert_eq!(row(&r, "a").score, Some(1.0), "中性降权被算成了 0");
    }

    #[test]
    fn an_empty_pool_returns_no_winner_without_claiming_anything_was_relaxed() {
        let r = rank(&[], &prefs(&[Axis::Cheap]), None);
        assert_eq!(r.winner, None);
        assert!(!r.floors_relaxed);
        assert!(!r.breakers_relaxed);
        assert!(r.rows.is_empty());
    }

    #[test]
    fn a_zero_success_rate_is_an_assertion_and_scores_zero() {
        // 跟「没测过」必须分开:量出来的 0% 就是最差,不是缺证据。
        let cs = [
            Candidate::new("全挂").success_rate(0.0),
            Candidate::new("正常").success_rate(0.9),
        ];
        let r = rank(&cs, &prefs(&[Axis::Stable]), None);
        assert_eq!(row(&r, "全挂").score, Some(0.0));
        assert!(row(&r, "全挂").missing.is_empty());
        assert_eq!(r.winner.as_deref(), Some("正常"));
    }

    #[test]
    fn presets_are_expressible_as_checkboxes_plus_floors() {
        // 预设不是魔法数字 —— 界面上照着念就行。
        assert_eq!(Prefs::coding().axes, vec![Axis::Stable, Axis::Fast]);
        assert_eq!(Prefs::coding().floors.max_ttft_p95_ms, Some(3_000));
        assert_eq!(Prefs::batch().axes, vec![Axis::Cheap]);
        assert_eq!(Prefs::batch().floors.min_success_rate, Some(0.95));
        assert_eq!(Prefs::chat().axes, vec![Axis::Fast]);
        assert_eq!(Prefs::chat().floors.min_success_rate, Some(0.99));
    }
}
