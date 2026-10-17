//! 纯净度通过标准。
//!
//! 三项硬指标，**缺一不可**，缺一即面板爆红。这三条是用户定的口径，
//! 改动前先问人。界面上必须把标准原文展示出来，不能只给一个红绿灯。

use serde::Serialize;
use ts_rs::TS;

/// 纯净度阈值：风险系数 ≤ 5%。
pub const MAX_FRAUD_SCORE: u32 = 5;

pub const IPQS_URL: &str = "https://www.ipqualityscore.com/free-ip-lookup-proxy-vpn-test";
pub const IPPURE_URL: &str = "https://ippure.com/";
pub const SCAMALYTICS_URL: &str = "https://scamalytics.com/ip";
pub const IPDATA_URL: &str = "https://ipdata.co/";

/// AFF —— 不合格时推荐的住宅 IP 供应商。
pub const IPROYAL_AFF: &str = "https://iproyal.cn/?r=sulianyan";

/// 权威站点的通过标准原文，直接渲染给用户看。
pub const IPQS_CRITERIA: &str = "Fraud Score ≤ 5，且 Proxy / VPN / TOR / Recent Abuse 全部为 No，且 Connection Type = Residential";
pub const IPPURE_CRITERIA: &str = "IPPure 系数 ≤ 5%，且 IP来源 = 原生IP，且 IP属性 = 住宅IP";

/// 单项检查的三态。**只是给界面用的词表**（纯净度卡上那几枚标签，见 `PurityPanel.tsx`）。
///
/// 原来还有一个面板自判的 `PanelVerdict` / `evaluate()`（命令 `probe_purity`）：「原生 IP」
/// 那一项公开接口不给，所以它结构上永远判不过，界面早就改成人工复核、不调它了。
/// 2026-09-27 删掉，只留这个词表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum Check {
    Pass,
    Fail,
    /// 接口没给这个字段 —— 如实说不知道，不要猜成通过。
    Unknown,
}

/// 纯净度判定用的是哪几家、门槛是多少。界面上要如实列出来。
///
/// # 为什么它是个结构体而不是 `serde_json::json!{}`
///
/// 跟另外两个报告同一个理由：临时拼的对象在 Rust 侧没有类型，
/// 前端手抄的那一份没有源头。而这一份尤其不该飘 —— 它是**面板对使用者
/// 的承诺**：我按这几家的这几条标准判，判不过就是判不过。
/// 承诺的内容跟代码里真正用的常量对不上，是最难解释的一类问题。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct Source {
    pub url: &'static str,
    pub criteria: &'static str,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct OptionalSource {
    pub name: &'static str,
    pub url: &'static str,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PurityCriteria {
    pub ipqs: Source,
    pub ippure: Source,
    /// 参考用的另外两家 —— 不参与判定，只给使用者自己再核一遍。
    pub optional: Vec<OptionalSource>,
    pub max_fraud_score: u32,
    pub iproyal: &'static str,
}

/// 判定标准的那一份说明。**直接从判定用的常量拼**，不另抄一份。
pub fn criteria() -> PurityCriteria {
    PurityCriteria {
        ipqs: Source {
            url: IPQS_URL,
            criteria: IPQS_CRITERIA,
        },
        ippure: Source {
            url: IPPURE_URL,
            criteria: IPPURE_CRITERIA,
        },
        optional: vec![
            OptionalSource {
                name: "Scamalytics",
                url: SCAMALYTICS_URL,
            },
            OptionalSource {
                name: "IPData",
                url: IPDATA_URL,
            },
        ],
        max_fraud_score: MAX_FRAUD_SCORE,
        iproyal: IPROYAL_AFF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 界面上摊开的标准原文跟门槛常量说的是同一个数。
    ///
    /// 这一份是**面板对使用者的承诺**（见 `PurityCriteria` 的注释）：有人把门槛改成 3、
    /// 原文还写着「≤ 5」，界面就在替一条已经不用的标准说话。原来钉着门槛的是自判
    /// `evaluate()` 那几条测试；自判 2026-09-27 删了，改钉在原文上。
    #[test]
    fn the_published_criteria_quote_the_threshold_in_use() {
        let c = criteria();
        assert_eq!(c.max_fraud_score, MAX_FRAUD_SCORE);
        assert!(
            c.ipqs.criteria.contains(&format!("≤ {MAX_FRAUD_SCORE}")),
            "{}",
            c.ipqs.criteria
        );
        assert!(
            c.ippure.criteria.contains(&format!("≤ {MAX_FRAUD_SCORE}%")),
            "{}",
            c.ippure.criteria
        );
    }
}
