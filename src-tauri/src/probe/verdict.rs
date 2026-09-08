//! 纯净度通过标准。
//!
//! 三项硬指标，**缺一不可**，缺一即面板爆红。这三条是用户定的口径，
//! 改动前先问人。界面上必须把标准原文展示出来，不能只给一个红绿灯。

use serde::Serialize;

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
pub const IPPURE_CRITERIA: &str =
    "IPPure 系数 ≤ 5%，且 IP来源 = 原生IP，且 IP属性 = 住宅IP";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Check {
    Pass,
    Fail,
    /// 接口没给这个字段 —— 如实说不知道，不要猜成通过。
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct PanelVerdict {
    pub purity: Check,
    pub residential: Check,
    pub native: Check,
    /// 三项全 Pass 才是 true。任一项 Unknown 都不算通过。
    pub passed: bool,
    pub note: &'static str,
}

pub fn evaluate(info: &crate::probe::ip::IpInfo) -> PanelVerdict {
    let purity = match info.fraud_score {
        Some(s) if s <= MAX_FRAUD_SCORE => Check::Pass,
        Some(_) => Check::Fail,
        None => Check::Unknown,
    };
    let residential = match info.is_residential {
        Some(true) => Check::Pass,
        Some(false) => Check::Fail,
        None => Check::Unknown,
    };
    // 「原生 IP」ippure 网页上有，公开接口的示例输出里没有这个字段。
    // 拿不到就如实报 Unknown，让用户去站点上自己看，不要用别的字段硬凑。
    let native = Check::Unknown;

    PanelVerdict {
        purity,
        residential,
        native,
        passed: matches!(purity, Check::Pass)
            && matches!(residential, Check::Pass)
            && matches!(native, Check::Pass),
        note: "面板自测仅供参考，不权威。以 IPQualityScore 与 ippure.com 的结果为准。",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::ip::IpInfo;

    fn info(f: Option<u32>, r: Option<bool>) -> IpInfo {
        IpInfo {
            ip: "203.0.113.7".into(),
            fraud_score: f,
            is_residential: r,
            ..Default::default()
        }
    }

    #[test]
    fn clean_residential_still_unknown_without_native_field() {
        // 接口给不出「原生 IP」，所以即使前两项通过也不能判定整体通过。
        let v = evaluate(&info(Some(3), Some(true)));
        assert_eq!(v.purity, Check::Pass);
        assert_eq!(v.residential, Check::Pass);
        assert_eq!(v.native, Check::Unknown);
        assert!(!v.passed);
    }

    #[test]
    fn score_above_threshold_fails() {
        assert_eq!(evaluate(&info(Some(6), Some(true))).purity, Check::Fail);
    }

    #[test]
    fn boundary_five_passes() {
        assert_eq!(evaluate(&info(Some(5), Some(true))).purity, Check::Pass);
    }

    #[test]
    fn datacenter_fails_residential() {
        assert_eq!(
            evaluate(&info(Some(1), Some(false))).residential,
            Check::Fail
        );
    }

    #[test]
    fn missing_fields_are_unknown_not_pass() {
        let v = evaluate(&info(None, None));
        assert_eq!(v.purity, Check::Unknown);
        assert_eq!(v.residential, Check::Unknown);
        assert!(!v.passed);
    }
}
