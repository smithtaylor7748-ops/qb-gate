//! 「便宜」这一维到底拿什么来比。
//!
//! # 倍率是个会骗人的口径
//!
//! 一条 ×0.20 但缓存全废的线,实际比 ×0.35 有 70% 命中的那条**贵**。
//! 倍率只描述单价,不描述你真的付了多少 —— 缓存命中率决定了你为同一段
//! 上下文付几次钱。
//!
//! 所以有真实账单时,这一维用 **24 小时实扣 ÷ 24 小时实际 token**。
//! 那是真金白银除以真实吞吐,缓存命中已经算在里面了,不需要再单独加一维。
//!
//! # 为什么整池必须用同一种口径
//!
//! 排序是**跟池里最好那条的比值**。倍率是个倍数(0.2、1.0),
//! 实扣单价是货币 ÷ token(1e-6 这个量级)——
//! 两种数混在一个池子里比,算出来的比值毫无意义:
//! 一条报了单价的线会因为数值小而永远「最便宜」,不管它其实多贵。
//!
//! 所以口径是**整池一起选的**:所有线都拿得出实扣单价才用它,
//! 差一条就整池退回倍率。宁可整池用粗一点但可比的口径,
//! 也不要一个精确但不可比的混合。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 这一池的「便宜」用的是哪种口径。界面要照实写出来 ——
/// 使用者看到的那个分数是按什么算的,不该靠猜。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum CheapBasis {
    /// 24 小时实扣 ÷ 24 小时实际 token。**算进了缓存命中。**
    CostPerToken,
    /// 按 token 结构加权出来的等效倍率。
    ///
    /// 比 [`RealRate`](CheapBasis::RealRate) 准:它把「计费翻倍」算进去了 ——
    /// 输入便宜但输出翻五倍的那条,跟输入贵但不翻倍的那条,只有按实际
    /// 输入输出比加权才比得出谁便宜。
    BlendedRatio,
    /// 真实倍率(标称 × mult)。连 token 结构都拿不到时的退路。
    RealRate,
    /// 一条线都没有可比的数 —— 这一维排不了。
    Unavailable,
}

/// 一条线路在「便宜」这一维上能拿出来的两种证据。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CheapInput {
    pub route_id: String,
    /// 24 小时实扣 ÷ 24 小时实际 token。
    pub cost_per_token: Option<f64>,
    /// 按 token 结构加权的等效倍率。
    pub blended_ratio: Option<f64>,
    /// 真实倍率。
    pub real_rate: Option<f64>,
}

/// 由一个时间窗算实扣单价。
///
/// 两项都要取证到才给数:少一项就算出一个偏小或偏大的单价,
/// 而这个数是要拿去跟别家对质的。token 为 0 时也不给 ——
/// 那是「这段时间没跑过东西」,不是「免费」。
pub fn cost_per_token(cost: Option<f64>, tokens: Option<u64>) -> Option<f64> {
    let (c, t) = (cost?, tokens?);
    (c.is_finite() && c >= 0.0 && t > 0).then(|| c / t as f64)
}

/// 给整池选口径,并算出每条线在这一维上的值。
///
/// 返回的顺序与传入的顺序一致 —— 调用方要按位置对回自己的候选列表。
pub fn cheap_values(pool: &[CheapInput]) -> (CheapBasis, Vec<Option<f64>>) {
    let usable = |v: Option<f64>| v.filter(|x| x.is_finite() && *x > 0.0);

    // 所有线都拿得出实扣单价才用它 —— 差一条就整池退回倍率。
    let all_priced = !pool.is_empty() && pool.iter().all(|p| usable(p.cost_per_token).is_some());
    if all_priced {
        return (
            CheapBasis::CostPerToken,
            pool.iter().map(|p| usable(p.cost_per_token)).collect(),
        );
    }

    // 其次是加权等效倍率 —— 它把「计费翻倍」算进去了。
    let all_blended = !pool.is_empty() && pool.iter().all(|p| usable(p.blended_ratio).is_some());
    if all_blended {
        return (
            CheapBasis::BlendedRatio,
            pool.iter().map(|p| usable(p.blended_ratio)).collect(),
        );
    }

    let any_rate = pool.iter().any(|p| usable(p.real_rate).is_some());
    if any_rate {
        (
            CheapBasis::RealRate,
            pool.iter().map(|p| usable(p.real_rate)).collect(),
        )
    } else {
        (CheapBasis::Unavailable, pool.iter().map(|_| None).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(id: &str, cpt: Option<f64>, rate: Option<f64>) -> CheapInput {
        CheapInput {
            route_id: id.into(),
            cost_per_token: cpt,
            blended_ratio: None,
            real_rate: rate,
        }
    }

    #[test]
    fn a_weighted_ratio_beats_a_bare_multiplier_when_one_station_folds_completion() {
        // 一家开了计费翻倍、倍率低，一家不翻倍、倍率高 —— 只比 real_rate
        // 会永远选前者，而那取决于输入输出比。加权口径才比得对。
        let pool = [
            CheapInput {
                route_id: "翻倍但倍率低".into(),
                cost_per_token: None,
                blended_ratio: Some(0.57), // 长篇生成时的等效倍率
                real_rate: Some(0.15),     // 只看这个会以为它最便宜
            },
            CheapInput {
                route_id: "不翻倍但倍率高".into(),
                cost_per_token: None,
                blended_ratio: Some(0.40),
                real_rate: Some(0.40),
            },
        ];
        let (basis, vals) = cheap_values(&pool);
        assert_eq!(basis, CheapBasis::BlendedRatio);
        assert!(
            vals[1].unwrap() < vals[0].unwrap(),
            "按加权口径，不翻倍那条才便宜"
        );
    }

    #[test]
    fn real_money_still_outranks_the_weighted_ratio() {
        // 有真实账单就用真实账单 —— 那是实际付出去的钱，比任何推算都准。
        let pool = [
            CheapInput {
                route_id: "a".into(),
                cost_per_token: Some(1.0e-6),
                blended_ratio: Some(0.9),
                real_rate: Some(0.9),
            },
            CheapInput {
                route_id: "b".into(),
                cost_per_token: Some(2.0e-6),
                blended_ratio: Some(0.1),
                real_rate: Some(0.1),
            },
        ];
        assert_eq!(cheap_values(&pool).0, CheapBasis::CostPerToken);
    }

    #[test]
    fn a_cheap_multiplier_with_dead_cache_loses_to_a_dearer_one_that_caches() {
        // 这就是换口径的全部理由:×0.20 缓存全废,比 ×0.35 有 70% 命中的贵。
        // 同样的 token 量下,前者实扣更多。
        let pool = [
            input("×0.20 但缓存全废", Some(2.0e-6), Some(0.20)),
            input("×0.35 但命中 70%", Some(1.2e-6), Some(0.35)),
        ];
        let (basis, vals) = cheap_values(&pool);
        assert_eq!(basis, CheapBasis::CostPerToken);
        // 越小越好 —— 第二条才是真的便宜,而按倍率排会选错。
        assert!(vals[1].unwrap() < vals[0].unwrap());
        assert!(pool[1].real_rate.unwrap() > pool[0].real_rate.unwrap());
    }

    #[test]
    fn one_line_without_a_bill_drops_the_whole_pool_back_to_multipliers() {
        // 混着比会让报了单价的那条因为数值小而永远「最便宜」——
        // 1e-6 跟 0.35 之间的比值毫无意义。
        let pool = [
            input("有账单", Some(1.0e-6), Some(0.5)),
            input("没账单", None, Some(0.2)),
        ];
        let (basis, vals) = cheap_values(&pool);
        assert_eq!(basis, CheapBasis::RealRate);
        assert_eq!(vals, vec![Some(0.5), Some(0.2)]);
    }

    #[test]
    fn a_pool_with_no_comparable_evidence_says_so_instead_of_inventing_one() {
        let pool = [input("a", None, None), input("b", None, None)];
        let (basis, vals) = cheap_values(&pool);
        assert_eq!(basis, CheapBasis::Unavailable);
        assert!(vals.iter().all(Option::is_none));
    }

    #[test]
    fn cost_per_token_needs_both_halves_and_a_nonzero_denominator() {
        assert_eq!(cost_per_token(Some(10.0), Some(100)), Some(0.1));
        assert_eq!(cost_per_token(None, Some(100)), None);
        assert_eq!(cost_per_token(Some(10.0), None), None);
        // 没跑过东西不等于免费。
        assert_eq!(cost_per_token(Some(0.0), Some(0)), None);
        assert_eq!(cost_per_token(Some(f64::NAN), Some(1)), None);
    }

    #[test]
    fn a_zero_or_negative_price_is_not_usable_evidence() {
        // 0 单价会在比值归一化里变成除零,而且它几乎肯定是坏数据。
        let pool = [
            input("a", Some(0.0), Some(0.3)),
            input("b", Some(1.0e-6), Some(0.5)),
        ];
        let (basis, _) = cheap_values(&pool);
        assert_eq!(basis, CheapBasis::RealRate);
    }

    #[test]
    fn an_empty_pool_is_unavailable_rather_than_priced() {
        let (basis, vals) = cheap_values(&[]);
        assert_eq!(basis, CheapBasis::Unavailable);
        assert!(vals.is_empty());
    }

    #[test]
    fn the_returned_order_matches_the_input_order() {
        // 调用方按位置对回自己的候选列表,错位会把分数安到别的线路上。
        let pool = [
            input("a", Some(3.0e-6), None),
            input("b", Some(1.0e-6), None),
            input("c", Some(2.0e-6), None),
        ];
        let (_, vals) = cheap_values(&pool);
        assert_eq!(vals, vec![Some(3.0e-6), Some(1.0e-6), Some(2.0e-6)]);
    }
}
