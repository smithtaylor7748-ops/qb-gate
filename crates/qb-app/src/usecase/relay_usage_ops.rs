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
use crate::error::{GateError, Result};
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
    fn merge(&mut self, other: &Self) {
        self.requests += other.requests;
        self.failed += other.failed;
        self.input_tokens = add_u(self.input_tokens, other.input_tokens);
        self.output_tokens = add_u(self.output_tokens, other.output_tokens);
        self.cache_read_tokens = add_u(self.cache_read_tokens, other.cache_read_tokens);
        self.cache_write_tokens = add_u(self.cache_write_tokens, other.cache_write_tokens);
        self.station_cost = add_f(self.station_cost, other.station_cost);
        self.official_cost = add_f(self.official_cost, other.official_cost);
    }

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

/// 首页总量：短窗精确到毫秒；全部包含已归档的日汇总，不受日志分页上限影响。
/// 短窗最多 7 天，落在 30 天明细保留期内；只读本机库。
pub fn totals(db: &Repository, client: &str, hours: Option<u32>) -> Result<RelayUsageTotals> {
    totals_at(db, client, hours, Local::now().timestamp_millis())
}

fn totals_at(
    db: &Repository,
    client: &str,
    hours: Option<u32>,
    now_ms: i64,
) -> Result<RelayUsageTotals> {
    let mut total = RelayUsageTotals::default();
    match hours {
        Some(hours @ (1 | 24 | 168)) => {
            let row =
                db.usage_window_totals(now_ms - i64::from(hours) * 3_600_000, now_ms, client)?;
            total.merge(&RelayUsageTotals::from_hour(&row));
        }
        None => {
            for row in db.usage_overview_rows(None, now_ms, client, None, None)? {
                total.merge(&RelayUsageTotals::from_hour(&row.totals));
            }
        }
        Some(_) => {
            return Err(GateError::Other(
                "用量时间范围只支持 1、24、168 小时或全部".into(),
            ))
        }
    }
    Ok(total)
}

/// 总览柱状图的一个时间桶；起止为实际统计范围，末桶结束包含查询时刻。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RelayUsageBucket {
    #[ts(type = "number")]
    pub start_ms: i64,
    #[ts(type = "number")]
    pub end_ms: i64,
    pub totals: RelayUsageTotals,
}

/// 总量、线路计数和趋势来自同一次完整查询，使用完全相同的客户端 / 时间过滤。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RelayUsageOverview {
    #[ts(type = "number")]
    pub since_ms: i64,
    #[ts(type = "number")]
    pub through_ms: i64,
    /// 实际粒度：minute 为 5 分钟，hour 为整小时，day 为本地自然日。
    /// 全部历史只有日归档，强制使用 day，不能冒充按小时的完整历史。
    pub granularity: String,
    pub totals: RelayUsageTotals,
    pub by_route: Vec<RelayUsageGroup>,
    pub buckets: Vec<RelayUsageBucket>,
}

pub fn overview(
    db: &Repository,
    client: &str,
    hours: Option<u32>,
    granularity: &str,
    route_id: Option<&str>,
) -> Result<RelayUsageOverview> {
    overview_at(
        db,
        client,
        hours,
        granularity,
        route_id,
        Local::now().timestamp_millis(),
    )
}

fn overview_at(
    db: &Repository,
    client: &str,
    hours: Option<u32>,
    granularity: &str,
    route_id: Option<&str>,
    now_ms: i64,
) -> Result<RelayUsageOverview> {
    let since = match hours {
        Some(hours @ (1 | 24 | 168)) => Some(now_ms - i64::from(hours) * 3_600_000),
        None => None,
        Some(_) => {
            return Err(GateError::Other(
                "用量时间范围只支持 1、24、168 小时或全部".into(),
            ))
        }
    };
    if !matches!(granularity, "minute" | "hour" | "day") {
        return Err(GateError::Other("不支持的用量时间粒度".into()));
    }
    // 一周的 5 分钟柱子没有可读性；全部历史也没有按小时的归档证据。
    let granularity = match (hours, granularity) {
        (None, _) => "day",
        (Some(168), "minute") => "hour",
        (_, g) => g,
    };
    let step = match granularity {
        "minute" => Some(300_000),
        "hour" => Some(3_600_000),
        _ => None,
    };
    let rows = db.usage_overview_rows(since, now_ms, client, route_id, step)?;
    let mut total = RelayUsageTotals::default();
    let mut routes: BTreeMap<String, (String, RelayUsageTotals)> = BTreeMap::new();
    let mut buckets: BTreeMap<i64, RelayUsageTotals> = BTreeMap::new();
    for row in rows {
        let start = match step {
            Some(_) => row
                .bucket
                .parse::<i64>()
                .map_err(|_| GateError::Other("用量时间桶无效".into()))?,
            None => midnight_ms(
                NaiveDate::parse_from_str(&row.bucket, "%Y-%m-%d")
                    .map_err(|_| GateError::Other("用量日期无效".into()))?,
            ),
        };
        let values = RelayUsageTotals::from_hour(&row.totals);
        total.merge(&values);
        routes
            .entry(row.route_id.clone())
            .or_insert_with(|| (row.route_id, RelayUsageTotals::default()))
            .1
            .merge(&values);
        buckets.entry(start).or_default().merge(&values);
    }
    let since_ms = since.unwrap_or_else(|| {
        buckets
            .first_key_value()
            .map(|(at, _)| *at)
            .unwrap_or(now_ms)
    });
    let bucket_start = |at: i64| -> i64 {
        match step {
            Some(step) => at.div_euclid(step) * step,
            None => midnight_ms(
                Local
                    .timestamp_millis_opt(at)
                    .single()
                    .unwrap()
                    .date_naive(),
            ),
        }
    };
    let bucket_end = |at: i64| -> i64 {
        match step {
            Some(step) => at + step,
            None => midnight_ms(
                Local
                    .timestamp_millis_opt(at)
                    .single()
                    .unwrap()
                    .date_naive()
                    + Duration::days(1),
            ),
        }
    };
    // 把无请求的间隔补为 0，所有柱子的边界严格裁到查询范围。
    // 包括全部历史的空日期，图上时间距离不能被压成“只算有请求的天”。
    if since.is_some() || !buckets.is_empty() {
        let mut at = bucket_start(since_ms);
        while at <= now_ms {
            buckets.entry(at).or_default();
            at = bucket_end(at);
        }
    }
    Ok(RelayUsageOverview {
        since_ms,
        through_ms: now_ms,
        granularity: granularity.into(),
        totals: total,
        by_route: sorted(routes),
        buckets: buckets
            .into_iter()
            .map(|(start, totals)| RelayUsageBucket {
                start_ms: start.max(since_ms),
                end_ms: bucket_end(start).min(now_ms),
                totals,
            })
            .collect(),
    })
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
    use crate::repository::Repository;
    use std::path::PathBuf;

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

    fn database() -> (PathBuf, Repository) {
        let path = std::env::temp_dir().join(format!("qb-usage-total-{}", uuid::Uuid::new_v4()));
        (path.clone(), Repository::open_in(&path).unwrap())
    }

    #[test]
    fn home_totals_use_exact_rolling_windows_and_include_archived_rows_for_all() {
        let (path, db) = database();
        let now = 1_783_500_000_000_i64;
        let one_hour_ago = now - 3_600_000;
        let day_ago = now - 86_400_000;
        let old = now - 40 * 86_400_000;
        let rows = [
            serde_json::json!({"id":"a","at_ms":one_hour_ago+1,"client":"codex","route_id":"c\u{1f}s\u{1f}g","model":"m","status":200,"fault":null,"input_tokens":10,"output_tokens":4,"cache_read_tokens":null,"cache_write_tokens":null,"station_cost":0.5,"official_cost":1.0}),
            serde_json::json!({"id":"b","at_ms":day_ago,"client":"codex","route_id":"c\u{1f}s\u{1f}g","model":"m","status":200,"fault":null,"input_tokens":20,"output_tokens":null,"cache_read_tokens":null,"cache_write_tokens":null,"station_cost":null,"official_cost":null}),
            serde_json::json!({"id":"c","at_ms":old,"client":"codex","route_id":"c\u{1f}s\u{1f}g","model":"m","status":200,"fault":null,"input_tokens":5,"output_tokens":2,"cache_read_tokens":null,"cache_write_tokens":null,"station_cost":0.2,"official_cost":0.4}),
            serde_json::json!({"id":"d","at_ms":now,"client":"claude-code","route_id":"x","model":"m","status":200,"fault":null,"input_tokens":100,"output_tokens":100,"station_cost":100.0}),
        ];
        db.insert_router_requests(&rows).unwrap();
        db.prune_router_requests(now - 30 * 86_400_000).unwrap();

        let hour = totals_at(&db, "codex", Some(1), now).unwrap();
        assert_eq!(
            (hour.requests, hour.input_tokens, hour.station_cost),
            (1, Some(10), Some(0.5))
        );
        let day = totals_at(&db, "codex", Some(24), now).unwrap();
        assert_eq!((day.requests, day.input_tokens), (2, Some(30)));
        assert_eq!(day.station_cost, Some(0.5), "未知消费不能冒充 0");
        let all = totals_at(&db, "codex", None, now).unwrap();
        assert_eq!(
            (all.requests, all.input_tokens, all.station_cost),
            (3, Some(35), Some(0.7))
        );
        drop(db);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn overview_counts_more_than_a_log_page_and_uses_identical_route_and_bucket_windows() {
        let (path, db) = database();
        let boundary = 1_800_000_000_000_i64;
        let now = boundary + 30 * 60_000;
        let since = now - 3_600_000;
        let mut rows: Vec<_> = (0..2_301)
            .map(|i| {
                serde_json::json!({
                    "id":format!("n{i}"), "at_ms":boundary - 1, "client":"codex", "route_id":"a",
                    "model":"m", "status":200, "input_tokens":2, "station_cost":0.5
                })
            })
            .collect();
        for (id, at, client, route) in [
            ("at-start", since, "codex", "a"),
            ("before-start", since - 1, "codex", "a"),
            ("at-boundary", boundary, "codex", "b"),
            ("at-now", now, "codex", "b"),
            ("future", now + 1, "codex", "b"),
            ("other-client", boundary, "claude-code", "b"),
        ] {
            rows.push(serde_json::json!({"id":id,"at_ms":at,"client":client,"route_id":route,"model":"m","status":200}));
        }
        db.insert_router_requests(&rows).unwrap();
        let result = overview_at(&db, "codex", Some(1), "hour", None, now).unwrap();
        assert_eq!(result.totals.requests, 2_304);
        assert_eq!(
            result.totals,
            totals_at(&db, "codex", Some(1), now).unwrap()
        );
        assert_eq!(
            result
                .by_route
                .iter()
                .map(|r| r.totals.requests)
                .sum::<u64>(),
            2_304
        );
        assert_eq!(
            result
                .buckets
                .iter()
                .map(|r| r.totals.requests)
                .sum::<u64>(),
            2_304
        );
        assert_eq!(result.buckets.len(), 2);
        assert_eq!(
            (
                result.buckets[0].start_ms,
                result.buckets[0].end_ms,
                result.buckets[0].totals.requests
            ),
            (since, boundary, 2_302)
        );
        assert_eq!(
            (
                result.buckets[1].start_ms,
                result.buckets[1].end_ms,
                result.buckets[1].totals.requests
            ),
            (boundary, now, 2)
        );
        let route = overview_at(&db, "codex", Some(1), "minute", Some("a"), now).unwrap();
        assert_eq!(route.totals.requests, 2_302);
        assert_eq!(route.by_route.len(), 1);
        assert_eq!(
            route.buckets.iter().map(|r| r.totals.requests).sum::<u64>(),
            2_302
        );
        let empty = overview_at(&db, "codex", Some(1), "hour", Some("missing"), now).unwrap();
        assert_eq!(empty.totals.requests, 0);
        assert!(empty.buckets.iter().all(|b| b.totals.requests == 0));
        drop(db);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn all_time_overview_keeps_archive_and_detail_on_the_same_day_without_a_thirty_day_cutoff() {
        let (path, db) = database();
        let day = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
        let old_midnight = midnight_ms(day);
        let old_noon = old_midnight + 12 * 3_600_000;
        let now = midnight_ms(day + Duration::days(400)) + 1234;
        let rows = [
            serde_json::json!({"id":"archived","at_ms":old_midnight,"client":"codex","route_id":"a","model":"m","status":200,"input_tokens":7}),
            serde_json::json!({"id":"detail-same-day","at_ms":old_noon,"client":"codex","route_id":"a","model":"m","status":200,"input_tokens":3}),
            serde_json::json!({"id":"today","at_ms":now,"client":"codex","route_id":"b","model":"m","status":200}),
            serde_json::json!({"id":"future","at_ms":now+1,"client":"codex","route_id":"b","model":"m","status":200}),
        ];
        db.insert_router_requests(&rows).unwrap();
        db.prune_router_requests(old_noon).unwrap();
        let result = overview_at(&db, "codex", None, "hour", None, now).unwrap();
        assert_eq!(result.granularity, "day", "日归档不能假装小时证据");
        assert_eq!(result.totals.requests, 3);
        assert_eq!(result.totals.input_tokens, Some(10));
        assert_eq!(result.totals, totals_at(&db, "codex", None, now).unwrap());
        assert_eq!(result.since_ms, old_midnight);
        assert_eq!(result.buckets.len(), 401);
        assert_eq!(result.buckets[0].totals.requests, 2);
        assert_eq!(result.buckets[1].totals.requests, 0);
        assert_eq!(result.buckets.last().unwrap().end_ms, now);
        assert_eq!(
            result
                .buckets
                .iter()
                .map(|b| b.totals.requests)
                .sum::<u64>(),
            3
        );
        let route = overview_at(&db, "codex", None, "day", Some("a"), now).unwrap();
        assert_eq!(route.totals.requests, 2);
        assert_eq!(route.by_route[0].key, "a");
        drop(db);
        std::fs::remove_dir_all(path).unwrap();
    }
}
