//! 中转用量（中转站重构阶段 6 · U1，cc-switch 的 `services/usage_stats.rs` 那一块）：本机路由五个软件
//! （Claude Code、Claude 桌面端、Codex、Gemini CLI、Grok Build）的请求 —— 按天的趋势、全年热力图、
//! 按软件 / 站点 / 模型，加上逐条日志。
//!
//! # 数据从哪来
//!
//! - 近 30 天是 `router_requests` 明细；更早的是 `usage_daily`（裁明细时按天汇总进去的，迁移 10）。
//!   两边不重叠，相加就是全部（`Repository::prune_router_requests` 同一个事务里先汇总再删）。
//! - **只有经本机路由的请求。** 共存型工具（OpenCode / OpenClaw / Hermes / Pi / MiniMax Code）和桌面端直连档
//!   写的是真 Key、不经路由，这里看不到 —— cc-switch 会去读它们各自的会话日志，没搬（界面上写明）。
//! - 一个联网请求都没有，全是本机库。美元两种：**站内价**（按线路公布的价算，落库时算好的）与**官方价折算**
//!   （同样的 token 按官方 API 价；订阅不按这个收费）。没记到的 token / 花费是「没记」，不是 0。

use crate::domain::Provider;
use crate::error::Result;
use crate::repository::Repository;
use chrono::{Duration, Local, NaiveDate, TimeZone};
use qb_platform::repository::{RouterRequestPage, UsageDayRow, UsageHourRow};
use qb_station::router::RouterRequest;
use qb_station::station::route::Route;
use serde::Serialize;
use std::collections::BTreeMap;
use ts_rs::TS;

/// 热力图看多少天：53 个整周，跟 GitHub 那种全年格子一样。
pub const YEAR_DAYS: i64 = 371;

/// 一组请求的合计。token / 花费是 `None` = 这一组里一条都没记到。
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RelayUsageTotals {
    #[ts(type = "number")]
    pub requests: u64,
    #[ts(type = "number")]
    pub failed: u64,
    #[ts(type = "number | null")]
    pub input_tokens: Option<u64>,
    #[ts(type = "number | null")]
    pub output_tokens: Option<u64>,
    #[ts(type = "number | null")]
    pub cache_read_tokens: Option<u64>,
    #[ts(type = "number | null")]
    pub cache_write_tokens: Option<u64>,
    /// 站内价（美元）。
    pub station_cost: Option<f64>,
    /// 官方价折算（美元）—— 不是账单。
    pub official_cost: Option<f64>,
}

fn add_u(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x + y),
        (x, y) => x.or(y),
    }
}

fn add_f(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x + y),
        (x, y) => x.or(y),
    }
}

impl RelayUsageTotals {
    fn add(&mut self, r: &UsageDayRow) {
        self.requests += r.requests;
        self.failed += r.failed;
        self.input_tokens = add_u(self.input_tokens, r.input_tokens);
        self.output_tokens = add_u(self.output_tokens, r.output_tokens);
        self.cache_read_tokens = add_u(self.cache_read_tokens, r.cache_read_tokens);
        self.cache_write_tokens = add_u(self.cache_write_tokens, r.cache_write_tokens);
        self.station_cost = add_f(self.station_cost, r.station_cost);
        self.official_cost = add_f(self.official_cost, r.official_cost);
    }

    fn from_hour(h: &UsageHourRow) -> Self {
        Self {
            requests: h.requests,
            failed: h.failed,
            input_tokens: h.input_tokens,
            output_tokens: h.output_tokens,
            cache_read_tokens: h.cache_read_tokens,
            cache_write_tokens: h.cache_write_tokens,
            station_cost: h.station_cost,
            official_cost: h.official_cost,
        }
    }
}

/// 一天（只列有请求的那几天；没请求的那天前端画空、写「—」）。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RelayUsageDay {
    /// 本地日期 `YYYY-MM-DD`。
    pub day: String,
    pub totals: RelayUsageTotals,
}

/// 今天的一个小时（只列有请求的那几个小时）。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RelayUsageHour {
    /// 本地小时 0–23。
    pub hour: u32,
    pub totals: RelayUsageTotals,
}

/// 按某一维的一组（软件 / 站点 / 模型）。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RelayUsageGroup {
    /// 软件的 kebab-case 名、站点 id、模型名。
    pub key: String,
    /// 界面上叫什么（站点名；删掉的线路写明「已删除」）。
    pub label: String,
    pub totals: RelayUsageTotals,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RelayUsageReport {
    /// 只看哪个软件（kebab-case）；`None` = 五个软件全部。
    pub client: Option<String>,
    /// 1 = 今天、7、30；0 = 全部。
    pub days: u32,
    pub totals: RelayUsageTotals,
    /// 范围里有请求的那几天，旧的在前。
    pub series: Vec<RelayUsageDay>,
    /// 只有 `days == 1` 时有：今天按小时。
    pub hours: Vec<RelayUsageHour>,
    /// 范围里按软件分（`client` 选了一个时就只有它一组）。
    pub by_client: Vec<RelayUsageGroup>,
    /// 按站点分（同一个站点在几个软件下的线路合在一起），请求多的在前。
    pub by_site: Vec<RelayUsageGroup>,
    /// 按客户端请求里写的模型分，请求多的在前。
    pub by_model: Vec<RelayUsageGroup>,
    /// 全年热力图：近 [`YEAR_DAYS`] 天里有请求的那几天（不跟着 `days` 走）。
    pub year: Vec<RelayUsageDay>,
    /// 最早有记录的那一天（明细或汇总里），没有就是 `None`。
    pub first_day: Option<String>,
}

/// 一页逐条日志。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RelayRequestPage {
    pub items: Vec<RouterRequest>,
    /// 符合条件的一共几条（明细只留 30 天）。
    #[ts(type = "number")]
    pub total: u64,
}

/// 本地某天零点的毫秒数。
fn midnight_ms(day: NaiveDate) -> i64 {
    let naive = day.and_hms_opt(0, 0, 0).unwrap_or_default();
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|t| t.timestamp_millis())
        // 零点刚好落在夏令时跳过去的那一小时里：退回 UTC 零点，差一小时不影响按天的汇总。
        .unwrap_or_else(|| naive.and_utc().timestamp_millis())
}

/// 线路 id → (站点 id, 分组)。删掉的线路照 `Route::make_id` 的拼法拆。
fn site_of(route_id: &str, routes: &BTreeMap<String, Route>) -> (String, String) {
    if let Some(r) = routes.get(route_id) {
        return (r.station_id.clone(), r.group.clone());
    }
    let mut parts = route_id.split('\u{1f}');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(_), Some(station), Some(group)) => (station.to_string(), group.to_string()),
        _ => (route_id.to_string(), String::new()),
    }
}

fn sorted(groups: BTreeMap<String, (String, RelayUsageTotals)>) -> Vec<RelayUsageGroup> {
    let mut out: Vec<RelayUsageGroup> = groups
        .into_iter()
        .map(|(key, (label, totals))| RelayUsageGroup { key, label, totals })
        .collect();
    out.sort_by(|a, b| {
        b.totals
            .requests
            .cmp(&a.totals.requests)
            .then_with(|| a.key.cmp(&b.key))
    });
    out
}

fn by_day(rows: &[UsageDayRow]) -> Vec<RelayUsageDay> {
    let mut days: BTreeMap<String, RelayUsageTotals> = BTreeMap::new();
    for r in rows {
        days.entry(r.day.clone()).or_default().add(r);
    }
    days.into_iter()
        .map(|(day, totals)| RelayUsageDay { day, totals })
        .collect()
}

/// 用量页那一整份。`client` 是 kebab-case 的软件名（`None` = 全部），`days` 0 = 全部。
pub fn report(db: &Repository, client: Option<&str>, days: u32) -> Result<RelayUsageReport> {
    let today = Local::now().date_naive();
    let first_day = db.usage_first_day()?;
    let start = match days {
        0 => first_day
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .unwrap_or(today),
        n => today - Duration::days(i64::from(n.max(1)) - 1),
    };
    let rows = db.usage_days(
        &start.format("%Y-%m-%d").to_string(),
        midnight_ms(start),
        client,
    )?;
    let year_start = today - Duration::days(YEAR_DAYS - 1);
    let year_rows = db.usage_days(
        &year_start.format("%Y-%m-%d").to_string(),
        midnight_ms(year_start),
        client,
    )?;
    let hours = if days == 1 {
        db.usage_hours(midnight_ms(today), client)?
            .iter()
            .map(|h| RelayUsageHour {
                hour: h.hour,
                totals: RelayUsageTotals::from_hour(h),
            })
            .collect()
    } else {
        Vec::new()
    };

    let routes: BTreeMap<String, Route> = db
        .list::<Route>("station_routes")?
        .into_iter()
        .map(|r| (r.id.clone(), r))
        .collect();
    let sites: BTreeMap<String, String> = db
        .list::<Provider>("providers")?
        .into_iter()
        .map(|p| (p.id, p.name))
        .collect();

    let mut totals = RelayUsageTotals::default();
    let mut clients: BTreeMap<String, (String, RelayUsageTotals)> = BTreeMap::new();
    let mut by_site: BTreeMap<String, (String, RelayUsageTotals)> = BTreeMap::new();
    let mut by_model: BTreeMap<String, (String, RelayUsageTotals)> = BTreeMap::new();
    for r in &rows {
        totals.add(r);
        clients
            .entry(r.client.clone())
            .or_insert_with(|| (r.client.clone(), RelayUsageTotals::default()))
            .1
            .add(r);
        let (site_key, label) = if r.route_id == "official" {
            (
                "official".to_string(),
                "官方 Codex（turn-state）".to_string(),
            )
        } else {
            let (station, _) = site_of(&r.route_id, &routes);
            let label = sites
                .get(&station)
                .cloned()
                .unwrap_or_else(|| format!("{station}（站点已删除）"));
            (station, label)
        };
        by_site
            .entry(site_key)
            .or_insert_with(|| (label, RelayUsageTotals::default()))
            .1
            .add(r);
        let model = if r.model.is_empty() {
            "（没写模型）".to_string()
        } else {
            r.model.clone()
        };
        by_model
            .entry(model.clone())
            .or_insert_with(|| (model, RelayUsageTotals::default()))
            .1
            .add(r);
    }

    Ok(RelayUsageReport {
        client: client.map(str::to_string),
        days,
        totals,
        series: by_day(&rows),
        hours,
        by_client: sorted(clients),
        by_site: sorted(by_site),
        by_model: sorted(by_model),
        year: by_day(&year_rows),
        first_day,
    })
}

/// 逐条日志的一页（新的在前）。明细只留 30 天，更早的只剩按天汇总。
pub fn requests(db: &Repository, page: &RouterRequestPage) -> Result<RelayRequestPage> {
    let (items, total) = db.router_request_page::<RouterRequest>(page)?;
    Ok(RelayRequestPage { items, total })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        day: &str,
        client: &str,
        route: &str,
        model: &str,
        n: u64,
        cost: Option<f64>,
    ) -> UsageDayRow {
        UsageDayRow {
            day: day.into(),
            client: client.into(),
            route_id: route.into(),
            model: model.into(),
            upstream_model: String::new(),
            requests: n,
            failed: 1,
            input_tokens: cost.map(|_| 10),
            station_cost: cost,
            ..Default::default()
        }
    }

    #[test]
    fn a_day_split_between_rollup_and_live_rows_is_one_day_and_nothing_becomes_zero() {
        let rows = [
            row("2026-10-01", "codex", "r", "gpt-5", 2, Some(0.5)),
            row("2026-10-01", "codex", "r", "gpt-5", 3, None),
            row("2026-10-03", "claude-code", "r2", "opus", 1, None),
        ];
        let days = by_day(&rows);
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].totals.requests, 5);
        assert_eq!(days[0].totals.station_cost, Some(0.5));
        assert_eq!(days[0].totals.input_tokens, Some(10));
        assert_eq!(
            days[1].totals.station_cost, None,
            "一条都没记到花费的那天是「没记」，不是 0"
        );
    }

    #[test]
    fn deleted_routes_still_land_on_their_site() {
        let routes = BTreeMap::new();
        let id = Route::make_id(qb_contract::domain::Client::Codex, "moxi", "default");
        assert_eq!(site_of(&id, &routes), ("moxi".into(), "default".into()));
        assert_eq!(site_of("weird", &routes), ("weird".into(), String::new()));
    }

    #[test]
    fn groups_are_busiest_first() {
        let mut g = BTreeMap::new();
        for (k, n) in [("a", 1u64), ("b", 5), ("c", 5)] {
            let mut t = RelayUsageTotals::default();
            t.requests = n;
            g.insert(k.to_string(), (k.to_string(), t));
        }
        let keys: Vec<String> = sorted(g).into_iter().map(|x| x.key).collect();
        assert_eq!(keys, ["b", "c", "a"]);
    }
}
