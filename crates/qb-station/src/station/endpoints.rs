// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 站点的备用地址（中转站重构阶段 4，cc-switch 的 P3 多端点：`services/provider/endpoints.rs` + `speedtest.rs`）。
//!
//! 同一个站点常有几个地址（国内 / 海外、主站 / 镜像）。存在 `station_endpoints` 表里，一个站点一行；
//! 「测速」只在点的时候跑，打的是这个站点自己的地址。
//!
//! 跟 cc-switch 不一样的地方：cc-switch 只让人挑一个地址用；QB Gate 让备用地址**参与故障转移** ——
//! 主地址根本连不上时，同一发请求改打备用地址（`qb-app::local_router::forward::try_line`）。
//! 顺序由 [`failover_order`] 定：主地址永远第一个，备用的按上次测速从快到慢，测不通的排最后。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 一个备用地址和它上次测速的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct StationEndpoint {
    /// 规范化过的地址（去掉两头空白和末尾的 `/`）。
    pub url: String,
    #[ts(type = "number")]
    pub added_ms: i64,
    /// 上次测速的耗时（毫秒）。没测过、或者测不通是 `None`。
    #[ts(type = "number | null")]
    pub latency_ms: Option<u64>,
    /// 上次测速拿到的状态码（任何状态码都算「通」—— 测的是地址，不是 Key）。
    pub status: Option<u16>,
    /// 上次测速失败的原因（连不上、超时……）。
    pub error: Option<String>,
    #[ts(type = "number | null")]
    pub tested_ms: Option<i64>,
}

/// 一个站点的全部备用地址（`station_endpoints` 的一行，主键是站点 id）。主地址在 `Provider.base_url`，不在这里。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct StationEndpoints {
    pub provider_id: String,
    pub endpoints: Vec<StationEndpoint>,
}

/// 一个站点最多挂几个备用地址。只是为了测速和故障转移不至于拖太久。
pub const MAX_ENDPOINTS: usize = 8;

/// 规范化一个地址：去掉两头空白和末尾的 `/`，只收 http / https（cc-switch 的 `add_custom_endpoint` 只去空白和斜杠）。
pub fn normalize(url: &str) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return Err("地址不能为空".into());
    }
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(format!("地址要以 https:// 或 http:// 开头：{url}"));
    }
    if url.chars().any(char::is_whitespace) {
        return Err(format!("地址里不能有空格：{url}"));
    }
    Ok(url)
}

/// 两个地址是不是同一个（不看大小写和末尾斜杠）。
pub fn same(a: &str, b: &str) -> bool {
    a.trim()
        .trim_end_matches('/')
        .eq_ignore_ascii_case(b.trim().trim_end_matches('/'))
}

impl StationEndpoints {
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            endpoints: Vec::new(),
        }
    }

    /// 加一个备用地址。跟主地址或已有的重复就不加（不报错）；超过 [`MAX_ENDPOINTS`] 报错。
    pub fn add(&mut self, url: &str, main: &str, now_ms: i64) -> Result<(), String> {
        let url = normalize(url)?;
        if same(&url, main) || self.endpoints.iter().any(|e| same(&e.url, &url)) {
            return Ok(());
        }
        if self.endpoints.len() >= MAX_ENDPOINTS {
            return Err(format!("一个站点最多挂 {MAX_ENDPOINTS} 个备用地址"));
        }
        self.endpoints.push(StationEndpoint {
            url,
            added_ms: now_ms,
            ..Default::default()
        });
        Ok(())
    }

    /// 删一个备用地址（没有就算了）。
    pub fn remove(&mut self, url: &str) {
        self.endpoints.retain(|e| !same(&e.url, url));
    }

    /// 记下一次测速的结果（只记这里已有的地址）。
    pub fn record(&mut self, result: &EndpointLatency, now_ms: i64) {
        if let Some(e) = self
            .endpoints
            .iter_mut()
            .find(|e| same(&e.url, &result.url))
        {
            e.latency_ms = result.latency_ms;
            e.status = result.status;
            e.error = result.error.clone();
            e.tested_ms = Some(now_ms);
        }
    }
}

/// 一次测速的结果（cc-switch 的 `EndpointLatency`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EndpointLatency {
    pub url: String,
    #[ts(type = "number | null")]
    pub latency_ms: Option<u64>,
    pub status: Option<u16>,
    pub error: Option<String>,
}

/// 故障转移时备用地址的先后：上次测通的按耗时从快到慢，没测过的随后（按加入的先后），测不通的最后。
/// 主地址不在这里面（它永远第一个试），跟主地址重复的也剔掉。
pub fn failover_order(main: &str, endpoints: &[StationEndpoint]) -> Vec<String> {
    let mut list: Vec<&StationEndpoint> =
        endpoints.iter().filter(|e| !same(&e.url, main)).collect();
    let rank = |e: &StationEndpoint| match (e.latency_ms, e.error.is_some()) {
        (Some(ms), false) => (0u8, ms),
        (None, false) => (1, 0),
        _ => (2, 0),
    };
    list.sort_by_key(|e| (rank(e), e.added_ms));
    list.into_iter().map(|e| e.url.clone()).collect()
}

/// 测速超时（秒）夹到这个范围（cc-switch 的 `sanitize_timeout`）。
pub fn sanitize_timeout(secs: Option<u64>) -> u64 {
    secs.unwrap_or(DEFAULT_TIMEOUT_SECS)
        .clamp(MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS)
}
pub const DEFAULT_TIMEOUT_SECS: u64 = 8;
pub const MIN_TIMEOUT_SECS: u64 = 2;
pub const MAX_TIMEOUT_SECS: u64 = 30;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_normalized_and_checked() {
        assert_eq!(
            normalize(" https://a.test/v1/ ").unwrap(),
            "https://a.test/v1"
        );
        assert!(normalize("").is_err());
        assert!(normalize("ftp://a.test").is_err());
        assert!(normalize("https://a .test").is_err());
    }

    #[test]
    fn adding_skips_duplicates_of_the_main_address_and_of_each_other() {
        let mut s = StationEndpoints::new("p");
        s.add("https://main.test/", "https://main.test", 1).unwrap();
        assert!(s.endpoints.is_empty(), "跟主地址重复");
        s.add("https://b.test", "https://main.test", 2).unwrap();
        s.add("HTTPS://B.TEST/", "https://main.test", 3).unwrap();
        assert_eq!(s.endpoints.len(), 1, "大小写和斜杠不同也算同一个");
        for i in 0..MAX_ENDPOINTS - 1 {
            s.add(&format!("https://x{i}.test"), "https://main.test", 4)
                .unwrap();
        }
        assert!(s
            .add("https://one-too-many.test", "https://main.test", 5)
            .is_err());
        s.remove("https://b.test/");
        assert_eq!(s.endpoints.len(), MAX_ENDPOINTS - 1);
    }

    #[test]
    fn failover_goes_fastest_first_untested_next_broken_last() {
        let e =
            |url: &str, added: i64, latency: Option<u64>, error: Option<&str>| StationEndpoint {
                url: url.into(),
                added_ms: added,
                latency_ms: latency,
                error: error.map(str::to_string),
                ..Default::default()
            };
        let order = failover_order(
            "https://main.test",
            &[
                e("https://broken.test", 1, None, Some("连接失败")),
                e("https://untested-b.test", 3, None, None),
                e("https://slow.test", 4, Some(900), None),
                e("https://untested-a.test", 2, None, None),
                e("https://fast.test", 5, Some(120), None),
                e("https://main.test/", 6, Some(10), None),
            ],
        );
        assert_eq!(
            order,
            [
                "https://fast.test",
                "https://slow.test",
                "https://untested-a.test",
                "https://untested-b.test",
                "https://broken.test"
            ]
        );
    }

    #[test]
    fn a_speed_test_result_lands_on_its_address_only() {
        let mut s = StationEndpoints::new("p");
        s.add("https://b.test", "https://main.test", 1).unwrap();
        s.record(
            &EndpointLatency {
                url: "https://b.test/".into(),
                latency_ms: Some(42),
                status: Some(404),
                error: None,
            },
            9,
        );
        s.record(
            &EndpointLatency {
                url: "https://stranger.test".into(),
                latency_ms: Some(1),
                status: None,
                error: None,
            },
            9,
        );
        assert_eq!(s.endpoints.len(), 1);
        assert_eq!(s.endpoints[0].latency_ms, Some(42));
        assert_eq!(s.endpoints[0].tested_ms, Some(9));
    }

    #[test]
    fn speed_test_timeouts_are_clamped() {
        // cc-switch `sanitize_timeout_clamps_values`。
        assert_eq!(sanitize_timeout(Some(1)), MIN_TIMEOUT_SECS);
        assert_eq!(sanitize_timeout(Some(999)), MAX_TIMEOUT_SECS);
        assert_eq!(sanitize_timeout(Some(10)), 10);
        assert_eq!(sanitize_timeout(None), DEFAULT_TIMEOUT_SECS);
    }
}
