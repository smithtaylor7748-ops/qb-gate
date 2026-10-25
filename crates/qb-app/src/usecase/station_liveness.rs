//! 测活编排（中转站重构阶段 2）：两种测活真去发请求，结果落 `probe_attempts`（迁移 7）。
//!
//! 判读 —— 死活、退避、速度、按预算摊的复测间隔 —— 全在 `qb_station::schedule::liveness`
//! （纯函数，改写自 sub2guard v2）。这一层只做三件事：挑这一轮测哪几条（[`plan`]）、
//! 真去发（[`run`]）、记账（落库、今天花了多少）。
//!
//! # 两种测活
//!
//! | | 免费 | 花钱 |
//! |---|---|---|
//! | 发什么 | 空请求体 `{}`，打这条线的协议路径 | 一发最小的真请求：输入 `hi`，输出上限 1（Anthropic）/ 16（Responses 的下限） |
//! | 证明什么 | 线通、Key 没被拒 | 能出字、首字多快 |
//! | 什么时候跑 | 调度开着时：最近没走过真实流量的线，按退避表；p1 连着失败时整队列紧急测一遍 | 使用者打开「花钱测活」之后，按每日预算摊到有得挑的那几个软件的线上 |
//!
//! ⛔ **花钱的那种默认关**，开了也按每日预算封顶（元，已乘各站的充值比例）。不知道价的线按
//! [`UNKNOWN_PRICE_PER_MTOK`] 记上界 —— 宁可少测几次，不让预算被一个算不出来的数绕过去。
//!
//! # 免费测活怎么判
//!
//! 空请求体本来就不合法，站点回 400 / 422 正是「它解析了这一发、Key 也过了」的证明（跟
//! [`probe_protocols`](super::station_ops::probe_protocols) 同一个道理）。所以下面这些都算**测通**：
//! 2xx、请求自己的错（400 / 405 / 413 / 415 / 422 / 501）、说没有这个模型（空体里压根没写模型）、
//! 认不出文字的 500（有的站拿空体做校验时自己崩了 —— 它活着）。算故障的只有：Key 被拒、余额、退役、
//! 限流、网关那一层的 502 / 503 / 504 / 524、超时、连不上。
//!
//! # 日志纪律
//!
//! 测活记录里只有故障归类与一句话：不含请求内容（本来也只有一个 `hi`）、不含 Key、不含地址
//! （reqwest 的错误链先 `without_url()`）。

use crate::domain::Client;
use crate::error::{GateError, Result};
use crate::local_router::{join_url, retry_after_ms, Upstream};
use crate::repository::Repository;
use qb_platform::repository::ProbeAttemptQuery;
use qb_proxy::classify::{classify_message, classify_text, classify_upstream, Fault};
use qb_proxy::convert::gemini_url;
use qb_proxy::convert::plan::ApiFormat;
use qb_station::schedule::liveness::{
    assess, estimate_cost, fail_rate, next_due, probe_interval_min, speed, Health, ProbeAttempt,
    ProbeSettings, ProbeState,
};
use qb_station::station::model::UsageRow;
use qb_station::station::pricing::Catalog;
use qb_station::station::route::Route;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use ts_rs::TS;

/// 测活设置存在 metadata 的这个键下（三个软件共用一份）。
pub const PROBE_SETTINGS_META: &str = "probe_settings";

/// 免费测活整个请求最多等多久。
pub const FREE_TIMEOUT: Duration = Duration::from_secs(15);
/// 花钱测活：首包最多等多久（跟本机路由默认的首包超时同一档）。
pub const PAID_FIRST_BYTE: Duration = Duration::from_secs(30);
/// 花钱测活：整个请求最多多久。
pub const PAID_TOTAL: Duration = Duration::from_secs(60);
/// 同时测几条。
pub const PROBE_CONCURRENCY: usize = 3;
/// 测活记录留几天（判读最多看 6 小时，速度看最近几次；界面「上次测活」用不着更久）。
pub const PROBE_KEEP_DAYS: i64 = 7;
/// 一次测活的响应最多读多少（再多就不是那一个 token 了）。
const MAX_PROBE_BODY: usize = 256 * 1024;
/// 花钱测活的输出上限：Anthropic 给 1。
pub const ANTHROPIC_MAX_OUTPUT: u64 = 1;
/// Responses 的 `max_output_tokens` 下限是 16。
pub const RESPONSES_MAX_OUTPUT: u64 = 16;
/// 不知道价的线，一次花钱测活按这组单价（美元 / 百万 token，输入 / 输出）记上界 ——
/// 大约是最贵的官方档的两倍。
pub const UNKNOWN_PRICE_PER_MTOK: (f64, f64) = (30.0, 150.0);
/// 突发测活：p1 在这么久里……
pub const BURST_WINDOW_MS: i64 = 5 * 60_000;
/// ……失败了这么多次、之后没成功过，就把整个队列紧急测一遍（只测免费的）。
pub const BURST_FAILURES: usize = 2;
/// 两次突发之间至少隔多久。
pub const BURST_COOLDOWN_MS: i64 = 5 * 60_000;
/// 同一条线两次测活至少隔多久（突发那一轮也守这条，免得一分钟里把一条线打好几遍）。
pub const MIN_GAP_MS: i64 = 60_000;
/// 只有一条线的软件：花钱测活没得挑，不测（死活由免费的那种管）。sub2guard 给它们每小时一次的份额，
/// 这里不必 —— `probe_interval_min` 的 `n_single` 恒为 0。
const SINGLE_INTERVAL_MIN: f64 = 60.0;

// ------------------------------------------------------------------ 设置

/// 读测活设置。没存过 = 默认那一份（花钱的关着）。
pub fn load_probe_settings(db: &Repository) -> Result<ProbeSettings> {
    match db.meta(PROBE_SETTINGS_META)? {
        Some(raw) => Ok(serde_json::from_str(&raw)?),
        None => Ok(ProbeSettings::default()),
    }
}

/// 存测活设置。不合格整份不存。
pub fn save_probe_settings(db: &Repository, settings: &ProbeSettings) -> Result<()> {
    settings.validate().map_err(GateError::Other)?;
    db.set_meta(PROBE_SETTINGS_META, &serde_json::to_string(settings)?)
}

// ------------------------------------------------------------------ 记账

/// 落一批测活记录，顺手裁掉 [`PROBE_KEEP_DAYS`] 天以前的。
pub fn persist(db: &Repository, attempts: &[ProbeAttempt], now_ms: i64) -> Result<()> {
    if attempts.is_empty() {
        return Ok(());
    }
    db.insert_probe_attempts(attempts)?;
    db.prune_probe_attempts(now_ms - PROBE_KEEP_DAYS * 24 * 3600 * 1000)?;
    Ok(())
}

/// 一条线最近的测活（新 → 旧）。
pub fn recent(db: &Repository, route_id: &str, limit: u32) -> Result<Vec<ProbeAttempt>> {
    db.probe_attempts(&ProbeAttemptQuery {
        route_id: Some(route_id.to_string()),
        limit,
        ..Default::default()
    })
}

/// `since_ms` 以来花钱测活花了多少（元）：`(下限之和, 上界之和)`。预算按上界扣。
pub fn spent_since(db: &Repository, since_ms: i64) -> Result<(f64, f64)> {
    let paid: Vec<ProbeAttempt> = db.probe_attempts(&ProbeAttemptQuery {
        since_ms: Some(since_ms),
        paid: Some(true),
        limit: 100_000,
        ..Default::default()
    })?;
    Ok(paid.iter().fold((0.0, 0.0), |(lo, hi), a| {
        (lo + a.cost_lo.unwrap_or(0.0), hi + a.cost_hi.unwrap_or(0.0))
    }))
}

/// 本地时间今天零点（毫秒）。「每日预算」按使用者自己的日子算。
pub fn local_midnight_ms(now_ms: i64) -> i64 {
    use chrono::{Local, TimeZone};
    let Some(now) = Local.timestamp_millis_opt(now_ms).single() else {
        return now_ms - now_ms.rem_euclid(24 * 3600 * 1000);
    };
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|t| Local.from_local_datetime(&t).earliest())
        .map_or(now_ms, |t| t.timestamp_millis())
}

// ------------------------------------------------------------------ 花多少钱

/// 花钱测活的输出上限（按上游说的协议，阶段 3 起不再按软件）：Responses 的下限是 16，别的给 1。
pub fn max_output(speaks: ApiFormat) -> u64 {
    if speaks == ApiFormat::OpenaiResponses {
        RESPONSES_MAX_OUTPUT
    } else {
        ANTHROPIC_MAX_OUTPUT
    }
}

/// 免费测活打 Gemini 原生时 URL 里要有个模型名（空请求体里没有）：给一个常见的，站点不认就是 404，
/// 那一次按「这条线不接这一发」跳过，不算故障。
const FREE_GEMINI_PROBE_MODEL: &str = "gemini-2.5-flash";

/// 在这条线上花钱测一次 `model` 大概多少钱（元）：`(下限, 上界)`。下限 `None` = 不知道价，
/// 上界按 [`UNKNOWN_PRICE_PER_MTOK`] 记。
///
/// 价按这个顺序找：这条线自己公布的四类单价 → 官方价 × 这条线的真实倍率 → 不知道。
/// 都是站内额度的美元，最后乘这家站的充值比例折成元。
pub fn probe_cost(
    route: &Route,
    model: Option<&str>,
    catalog: &Catalog,
    topup: f64,
) -> (Option<f64>, f64) {
    let out = max_output(crate::local_router::speaks(route));
    let [p_in, _, _, p_out] = route.rates.category_prices();
    let known = p_in.zip(p_out).or_else(|| {
        let official = catalog.resolve(model?)?;
        let rate = route.rate_for_ranking()?;
        (official.currency == "USD").then_some((
            official.input.per_mtok * rate,
            official.output.per_mtok * rate,
        ))
    });
    match known {
        Some((input, output)) => {
            let (lo, hi) = estimate_cost(input, output, out);
            (Some(lo * topup), hi * topup)
        }
        None => {
            let (_, hi) = estimate_cost(UNKNOWN_PRICE_PER_MTOK.0, UNKNOWN_PRICE_PER_MTOK.1, out);
            (None, hi * topup)
        }
    }
}

// ------------------------------------------------------------------ 这一轮测哪几条

/// 测哪种。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    Free,
    Paid,
}

/// 一条线眼下的测活状况（[`plan`] 的输入）。
#[derive(Debug, Clone, Default)]
pub struct LineProbes {
    pub route_id: String,
    /// 新 → 旧，免费的花钱的都在。
    pub attempts: Vec<ProbeAttempt>,
    /// 本机路由最近一次把请求打到这条线上的时刻（成败都算）。
    pub last_traffic_ms: Option<i64>,
    /// 花钱测活用哪个模型。`None` = 没得测（还没见过这个软件的真实流量用什么模型，或者都缺）。
    pub paid_model: Option<String>,
    /// 一次花钱测活的申报上界（元，[`probe_cost`] 的第二项）。
    pub cost_hi: f64,
}

/// 一个软件的线（队列顺序）。
#[derive(Debug, Clone)]
pub struct Pool {
    pub client: Client,
    pub lines: Vec<LineProbes>,
    /// p1 连着失败了（[`burst_needed`]）：这一轮整队列都测一遍免费的。
    pub burst: bool,
}

/// [`plan`] 挑出来的一次测活。
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    pub client: Client,
    pub route_id: String,
    pub kind: ProbeKind,
    /// 花钱的那种测哪个模型。免费的是空串（空请求体里不带模型）。
    pub model: String,
}

/// 这一轮测哪几条。**纯函数**（时间、花了多少都由调用方给）。
///
/// - 花钱的（开着时）：只测**有得挑**的软件的线（两条以上），没判死、到点了（间隔按预算摊，
///   [`probe_interval_min`]）、今天剩下的预算还够这一次的上界。
/// - 免费的：没轮上花钱的那几条里，最近 `free_interval_min` 分钟没走过真实流量的（真实流量本身就是证据），
///   按 [`next_due`] 的退避表到点了的。**判死了的线照测** —— 哪怕它还在接流量，不然它永远翻不了身。
///   突发那一轮（`pool.burst`）不看到没到点。
/// - 同一条线两次测活至少隔 [`MIN_GAP_MS`]。
pub fn plan(
    pools: &[Pool],
    settings: &ProbeSettings,
    spent_today: f64,
    now_ms: i64,
) -> Vec<Planned> {
    let multi: Vec<&LineProbes> = pools
        .iter()
        .filter(|p| p.lines.len() >= 2)
        .flat_map(|p| &p.lines)
        .collect();
    let paid_interval_ms = (settings.paid && !multi.is_empty()).then(|| {
        let mean_hi = multi.iter().map(|l| l.cost_hi).sum::<f64>() / multi.len() as f64;
        let cap = if mean_hi > 0.0 {
            (settings.daily_budget_cny / mean_hi)
                .floor()
                .clamp(0.0, f64::from(u32::MAX)) as u32
        } else {
            u32::MAX
        };
        let minutes = probe_interval_min(
            multi.len() as u32,
            0,
            cap,
            SINGLE_INTERVAL_MIN,
            f64::from(settings.paid_min_interval_min),
            f64::from(settings.paid_max_interval_min),
        );
        (minutes * 60_000.0) as i64
    });
    let free_interval_ms = i64::from(settings.free_interval_min) * 60_000;
    let mut spent = spent_today;
    let mut out = Vec::new();
    for pool in pools {
        let has_choice = pool.lines.len() >= 2;
        for line in &pool.lines {
            let last_at = line.attempts.first().map(|a| a.at_ms);
            if last_at.is_some_and(|t| now_ms - t < MIN_GAP_MS) {
                continue;
            }
            let health = assess(&line.attempts, now_ms);
            if let (Some(interval), true, Some(model)) =
                (paid_interval_ms, has_choice, line.paid_model.as_deref())
            {
                let last_paid = line.attempts.iter().find(|a| a.paid).map(|a| a.at_ms);
                let due = last_paid.is_none_or(|t| now_ms - t >= interval);
                if health.state != ProbeState::Down
                    && due
                    && spent + line.cost_hi <= settings.daily_budget_cny
                {
                    spent += line.cost_hi;
                    out.push(Planned {
                        client: pool.client,
                        route_id: line.route_id.clone(),
                        kind: ProbeKind::Paid,
                        model: model.to_string(),
                    });
                    continue;
                }
            }
            let free_due = pool.burst || {
                let has_traffic = line
                    .last_traffic_ms
                    .is_some_and(|t| now_ms - t < free_interval_ms);
                (health.state == ProbeState::Down || !has_traffic)
                    && next_due(&health, last_at, f64::from(settings.free_interval_min)) <= now_ms
            };
            if free_due {
                out.push(Planned {
                    client: pool.client,
                    route_id: line.route_id.clone(),
                    kind: ProbeKind::Free,
                    model: String::new(),
                });
            }
        }
    }
    out
}

/// p1 该不该触发整队列紧急测活：[`BURST_WINDOW_MS`] 内最近的 [`BURST_FAILURES`] 次以上都失败了
/// （中间没成功过），而且离上次突发过了冷却期。`rows` 是 p1 在本机路由日志里的证据行
/// （[`router_usage_rows`](super::station_ops::router_usage_rows)，不算线路的错的那几类已经去掉了）。
pub fn burst_needed(rows: &[UsageRow], last_burst_ms: Option<i64>, now_ms: i64) -> bool {
    if last_burst_ms.is_some_and(|t| now_ms - t < BURST_COOLDOWN_MS) {
        return false;
    }
    let mut recent: Vec<&UsageRow> = rows
        .iter()
        .filter(|r| (0..=BURST_WINDOW_MS).contains(&(now_ms - r.at_ms)))
        .collect();
    recent.sort_by_key(|r| std::cmp::Reverse(r.at_ms));
    recent.iter().take_while(|r| !r.ok).count() >= BURST_FAILURES
}

/// 这个软件的测活模型优先序：真实流量里用得最多的排前面（各条线尽量测同一个，速度才能比；
/// 同样多的按名字排，免得每一跳换来换去）。不是文本模型的去掉（[`probe_model_order`]）。
pub fn traffic_models(requests: &[qb_station::router::RouterRequest]) -> Vec<String> {
    let mut counts: std::collections::HashMap<&str, usize> = Default::default();
    for r in requests {
        *counts.entry(r.model.as_str()).or_default() += 1;
    }
    let mut ranked: Vec<(&str, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let traffic: Vec<String> = ranked.into_iter().map(|(m, _)| m.to_string()).collect();
    qb_station::schedule::liveness::probe_model_order(&traffic, None)
}

// ------------------------------------------------------------------ 界面

/// 失败率看最近几次测活。
const FAIL_RATE_SAMPLES: usize = 10;

/// 派单队列一行旁边的测活状况（界面「派单队列」那张表）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LineLiveness {
    pub route_id: String,
    /// 站点 · 分组。
    pub label: String,
    pub health: Health,
    pub last_probe_ms: Option<i64>,
    /// 下次免费测活大概什么时候（调度开着时）。`None` = 最近走过真实流量，用不着测。
    pub next_probe_ms: Option<i64>,
    /// 花钱测活量出来的速度（总耗时中位数，毫秒）。没开花钱测活、或者还没测出来是 `None`。
    pub speed_ms: Option<u64>,
    pub speed_samples: u32,
    /// 最近几次测活（不算跳过的那几类）里没测通的比例。没测过是 `None`。
    pub fail_rate: Option<f64>,
    /// 最近 24 小时本机路由打到这条线上几发。
    pub requests_24h: u32,
    /// 折合价：真实倍率 × 这家站的充值比例（每 $1 官方牌价付几元）。不知道是 `None`。
    pub real_rate: Option<f64>,
}

/// 拼一行 [`LineLiveness`]。`attempts` 新 → 旧；`last_traffic_ms` / `requests_24h` 来自本机路由日志。
#[allow(clippy::too_many_arguments)]
pub fn summarize(
    route_id: &str,
    label: &str,
    attempts: &[ProbeAttempt],
    last_traffic_ms: Option<i64>,
    requests_24h: u32,
    real_rate: Option<f64>,
    settings: &ProbeSettings,
    now_ms: i64,
) -> LineLiveness {
    let health = assess(attempts, now_ms);
    let last_probe_ms = attempts.first().map(|a| a.at_ms);
    let has_traffic = last_traffic_ms
        .is_some_and(|t| now_ms - t < i64::from(settings.free_interval_min) * 60_000);
    let next_probe_ms = (health.state == ProbeState::Down || !has_traffic).then(|| {
        next_due(
            &health,
            last_probe_ms,
            f64::from(settings.free_interval_min),
        )
        .max(now_ms)
    });
    let (speed_ms, _, speed_samples) = speed(attempts, None, now_ms, 5, 6 * 3600 * 1000);
    let counted = attempts.iter().any(|a| !is_skipped(a));
    LineLiveness {
        route_id: route_id.to_string(),
        label: label.to_string(),
        health,
        last_probe_ms,
        next_probe_ms,
        speed_ms,
        speed_samples,
        fail_rate: counted.then(|| fail_rate(attempts, FAIL_RATE_SAMPLES)),
        requests_24h,
        real_rate,
    }
}

fn is_skipped(a: &ProbeAttempt) -> bool {
    a.fault
        .as_deref()
        .is_some_and(|f| qb_station::schedule::liveness::SKIP.contains(&f))
}

// ------------------------------------------------------------------ 真去发

/// 测活要打的那条线。**明文 Key 只在后端**，`Debug` 不印它。
#[derive(Clone)]
pub struct ProbeTarget {
    pub route_id: String,
    pub client: Client,
    /// 上游实际说的协议（[`crate::local_router::speaks`]）：测活跟真实流量打同一个协议的路径、带同一种鉴权头。
    speaks: ApiFormat,
    base_url: String,
    key: String,
    /// 只用它的模型映射（客户端的模型名 → 这条线上的），见 [`Upstream::map_model`]。
    mapping: Upstream,
    /// 一次花钱测活的花费区间（元），见 [`probe_cost`]。
    pub cost: (Option<f64>, f64),
}

impl std::fmt::Debug for ProbeTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProbeTarget")
            .field("route_id", &self.route_id)
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}

impl ProbeTarget {
    /// 从库里装一条线（跟本机路由转发用同一把 Key、同一个基地址，见 `station_ops::source`）。
    pub fn load(db: &Repository, route: &Route, cost: (Option<f64>, f64)) -> Result<Self> {
        let source = super::station_ops::source(db, route)?;
        Ok(Self {
            route_id: route.id.clone(),
            client: route.client,
            speaks: crate::local_router::speaks(route),
            base_url: source.base_url,
            key: source.key,
            mapping: Upstream::default().with_model_map(&route.model_map),
            cost,
        })
    }

    /// 测试用：直接给地址和 Key。
    pub fn new(route_id: &str, client: Client, base_url: &str, key: &str) -> Self {
        Self {
            route_id: route_id.to_string(),
            client,
            speaks: if client == Client::Codex {
                ApiFormat::OpenaiResponses
            } else {
                ApiFormat::Anthropic
            },
            base_url: base_url.to_string(),
            key: key.to_string(),
            mapping: Upstream::default(),
            cost: (None, 0.0),
        }
    }

    /// 测试用：这条线的上游说别的协议。
    pub fn speaking(mut self, speaks: ApiFormat) -> Self {
        self.speaks = speaks;
        self
    }

    /// 跟本机路由转发时一样的路径与鉴权，按上游说的协议（阶段 3 起）：Anthropic 走 Messages + `x-api-key`，
    /// OpenAI 两种走各自的路径 + Bearer，Gemini 走 `generateContent` + `x-goog-api-key`。
    /// `model` 只有 Gemini 用得上（模型名在 URL 里）。
    fn post(
        &self,
        http: &reqwest::Client,
        body: String,
        model: Option<&str>,
        stream: bool,
    ) -> reqwest::RequestBuilder {
        let url = match self.speaks {
            ApiFormat::Anthropic => join_url(&self.base_url, "/v1/messages"),
            ApiFormat::OpenaiResponses => join_url(&self.base_url, "/v1/responses"),
            ApiFormat::OpenaiChat => join_url(&self.base_url, "/v1/chat/completions"),
            ApiFormat::GeminiNative => {
                let model =
                    gemini_url::normalize_gemini_model_id(model.unwrap_or(FREE_GEMINI_PROBE_MODEL));
                let endpoint = if stream {
                    format!("/v1beta/models/{model}:streamGenerateContent?alt=sse")
                } else {
                    format!("/v1beta/models/{model}:generateContent")
                };
                gemini_url::resolve_gemini_native_url(&self.base_url, &endpoint, false)
            }
        };
        let req = http
            .post(url)
            .header("content-type", "application/json")
            .body(body);
        match self.speaks {
            ApiFormat::Anthropic => req
                .header("x-api-key", self.key.trim())
                .header("anthropic-version", "2023-06-01"),
            ApiFormat::OpenaiChat | ApiFormat::OpenaiResponses => req.bearer_auth(self.key.trim()),
            ApiFormat::GeminiNative => req.header("x-goog-api-key", self.key.trim()),
        }
    }
}

/// 并发 [`PROBE_CONCURRENCY`] 条地测一批。回来的顺序不保证。
pub async fn run(
    jobs: Vec<(ProbeTarget, ProbeKind, String)>,
    http: &reqwest::Client,
    now_ms: i64,
) -> Vec<ProbeAttempt> {
    use futures_util::stream::{self, StreamExt};
    stream::iter(jobs)
        .map(|(target, kind, model)| async move {
            match kind {
                ProbeKind::Free => free_probe(&target, http, now_ms).await,
                ProbeKind::Paid => paid_probe(&target, &model, http, now_ms).await,
            }
        })
        .buffer_unordered(PROBE_CONCURRENCY)
        .collect()
        .await
}

fn attempt(target: &ProbeTarget, now_ms: i64, model: &str, paid: bool) -> ProbeAttempt {
    ProbeAttempt {
        id: uuid::Uuid::new_v4().to_string(),
        route_id: target.route_id.clone(),
        at_ms: now_ms,
        model: model.to_string(),
        paid,
        ..Default::default()
    }
}

fn failed(mut a: ProbeAttempt, fault: Fault, error: String) -> ProbeAttempt {
    a.ok = false;
    a.fault = Some(fault.key().to_string());
    a.error = Some(error);
    a
}

/// 发不出去 / 读到一半断了：超时算超时，别的先认文字（connection refused / reset），认不出算上游挂了。
fn transport_fault(e: reqwest::Error) -> (Fault, String) {
    let timeout = e.is_timeout();
    let text = qb_proxy::proxy_error::describe_error_chain(&e.without_url());
    let fault = if timeout {
        Fault::Timeout
    } else {
        classify_text(&text).unwrap_or(Fault::UpstreamDown)
    };
    (fault, format!("{}：{text}", fault.label()))
}

/// 读响应体（有上限），读不完的部分丢掉。
async fn read_capped(resp: &mut reqwest::Response) -> std::result::Result<Vec<u8>, reqwest::Error> {
    let mut buf = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        let room = MAX_PROBE_BODY.saturating_sub(buf.len());
        buf.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if buf.len() >= MAX_PROBE_BODY {
            break;
        }
    }
    Ok(buf)
}

/// 免费测活：空请求体打协议路径。怎么判见模块说明。
pub async fn free_probe(target: &ProbeTarget, http: &reqwest::Client, now_ms: i64) -> ProbeAttempt {
    let a = attempt(target, now_ms, "", false);
    let started = Instant::now();
    let sent = target
        .post(http, "{}".into(), None, false)
        .timeout(FREE_TIMEOUT)
        .send()
        .await;
    let mut resp = match sent {
        Ok(r) => r,
        Err(e) => {
            let (fault, error) = transport_fault(e);
            return ProbeAttempt {
                total_ms: Some(started.elapsed().as_millis() as u64),
                ..failed(a, fault, error)
            };
        }
    };
    let status = resp.status().as_u16();
    let retry = retry_after_ms(
        resp.headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok()),
    );
    let body = read_capped(&mut resp).await.unwrap_or_default();
    let total_ms = Some(started.elapsed().as_millis() as u64);
    if (200..300).contains(&status) {
        return ProbeAttempt {
            ok: true,
            total_ms,
            ..a
        };
    }
    let text = String::from_utf8_lossy(&body);
    let class = classify_upstream(status, Some(&text), retry);
    let reachable = matches!(class.fault, Fault::ClientRequest | Fault::ModelMissing)
        || (status == 500 && classify_text(&text).is_none());
    if reachable {
        return ProbeAttempt {
            ok: true,
            total_ms,
            ..a
        };
    }
    ProbeAttempt {
        total_ms,
        ..failed(
            a,
            class.fault,
            format!("HTTP {status}：{}", class.fault.label()),
        )
    }
}

/// 花钱测活的请求体（按上游说的协议）。模型名先按这条线的映射换过。
fn paid_body(speaks: ApiFormat, model: &str) -> Value {
    match speaks {
        ApiFormat::OpenaiResponses => {
            json!({"model": model, "input": "hi", "max_output_tokens": RESPONSES_MAX_OUTPUT, "store": false, "stream": true})
        }
        ApiFormat::OpenaiChat => {
            json!({"model": model, "max_tokens": ANTHROPIC_MAX_OUTPUT, "messages": [{"role": "user", "content": "hi"}], "stream": true})
        }
        ApiFormat::GeminiNative => {
            json!({"contents": [{"role": "user", "parts": [{"text": "hi"}]}], "generationConfig": {"maxOutputTokens": ANTHROPIC_MAX_OUTPUT}})
        }
        ApiFormat::Anthropic => {
            json!({"model": model, "max_tokens": ANTHROPIC_MAX_OUTPUT, "messages": [{"role": "user", "content": "hi"}], "stream": true})
        }
    }
}

/// 一段 2xx 的响应体里有没有报错：SSE 的 `error` / `response.failed` 事件，或者整份 JSON 的错误信封。
/// 有就回那句报错（给 [`classify_message`] 认）；`Ok(false)` = 没报错但也没见到结束事件（流提前断了）。
fn read_reply(body: &str) -> std::result::Result<bool, String> {
    let head = body.trim_start();
    if !(head.starts_with("data:") || head.starts_with("event:")) {
        // 站点没理会 stream:true，回了整份 JSON。
        let Ok(v) = serde_json::from_str::<Value>(head) else {
            return Ok(false);
        };
        if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
            return Err(error_text(err));
        }
        return Ok(["content", "output", "choices", "candidates"]
            .iter()
            .any(|k| v.get(*k).is_some()));
    }
    let mut done = false;
    for line in body.lines() {
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            done = true;
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        match v.get("type").and_then(Value::as_str).unwrap_or("") {
            "error" => return Err(error_text(v.get("error").unwrap_or(&v))),
            "response.failed" => {
                return Err(error_text(
                    v.pointer("/response/error").unwrap_or(&Value::Null),
                ))
            }
            // 输出上限到了（推理模型 16 个 token 可能全花在思考上）：模型在干活，算通。
            "message_stop" | "response.completed" | "response.incomplete" => done = true,
            _ => {}
        }
        // Chat 的结束块带 `finish_reason`；Gemini 的流没有结束事件，见到候选就算通。
        let chat_finished = v
            .pointer("/choices/0/finish_reason")
            .is_some_and(|r| !r.is_null());
        if chat_finished || v.get("candidates").is_some() {
            done = true;
        }
    }
    Ok(done)
}

fn error_text(err: &Value) -> String {
    err.get("message")
        .and_then(Value::as_str)
        .or_else(|| err.as_str())
        .unwrap_or("upstream request failed")
        .chars()
        .take(300)
        .collect()
}

/// 花钱测活：真打一发最小的请求，量首包与总耗时。
///
/// 花费：2xx 之后的都按估算记（哪怕流半路断了，站点可能已经扣了）；没到 2xx 的不记（失败秒回不扣钱）。
pub async fn paid_probe(
    target: &ProbeTarget,
    model: &str,
    http: &reqwest::Client,
    now_ms: i64,
) -> ProbeAttempt {
    let a = attempt(target, now_ms, model, true);
    let upstream_model = target
        .mapping
        .map_model(model)
        .unwrap_or_else(|| model.to_string());
    let started = Instant::now();
    let sent = target
        .post(
            http,
            paid_body(target.speaks, &upstream_model).to_string(),
            Some(&upstream_model),
            true,
        )
        .header("accept", "text/event-stream")
        .timeout(PAID_TOTAL)
        .send();
    let mut resp = match tokio::time::timeout(PAID_FIRST_BYTE, sent).await {
        Err(_) => {
            return ProbeAttempt {
                total_ms: Some(started.elapsed().as_millis() as u64),
                ..failed(a, Fault::Timeout, "首包 30 秒没来".into())
            }
        }
        Ok(Err(e)) => {
            let (fault, error) = transport_fault(e);
            return ProbeAttempt {
                total_ms: Some(started.elapsed().as_millis() as u64),
                ..failed(a, fault, error)
            };
        }
        Ok(Ok(r)) => r,
    };
    let status = resp.status().as_u16();
    let retry = retry_after_ms(
        resp.headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok()),
    );
    if !(200..300).contains(&status) {
        let body = read_capped(&mut resp).await.unwrap_or_default();
        let class = classify_upstream(status, Some(&String::from_utf8_lossy(&body)), retry);
        return ProbeAttempt {
            total_ms: Some(started.elapsed().as_millis() as u64),
            ..failed(
                a,
                class.fault,
                format!("HTTP {status}：{}", class.fault.label()),
            )
        };
    }
    let charged = ProbeAttempt {
        cost_lo: target.cost.0,
        cost_hi: Some(target.cost.1),
        ..a
    };
    let mut first_ms = None;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                first_ms.get_or_insert(started.elapsed().as_millis() as u64);
                let room = MAX_PROBE_BODY.saturating_sub(buf.len());
                buf.extend_from_slice(&chunk[..chunk.len().min(room)]);
                if buf.len() >= MAX_PROBE_BODY {
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => {
                let (fault, error) = transport_fault(e);
                return ProbeAttempt {
                    first_ms,
                    total_ms: Some(started.elapsed().as_millis() as u64),
                    ..failed(charged, fault, error)
                };
            }
        }
    }
    let total_ms = Some(started.elapsed().as_millis() as u64);
    match read_reply(&String::from_utf8_lossy(&buf)) {
        Ok(true) => ProbeAttempt {
            ok: true,
            first_ms,
            total_ms,
            ..charged
        },
        Ok(false) => ProbeAttempt {
            first_ms,
            total_ms,
            ..failed(
                charged,
                Fault::UpstreamDown,
                "流提前结束（没见到结束事件）".into(),
            )
        },
        Err(message) => {
            let fault = classify_message(&message);
            ProbeAttempt {
                first_ms,
                total_ms,
                ..failed(
                    charged,
                    fault,
                    format!("HTTP 200 里报错：{}", fault.label()),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qb_station::schedule::liveness::{BUSY, DETERMINISTIC, MODEL_MISSING, SKIP, TRANSIENT};

    const NOW: i64 = 10_000_000_000;

    fn att(at_ms: i64, ok: bool, fault: Option<&str>, paid: bool) -> ProbeAttempt {
        ProbeAttempt {
            at_ms,
            ok,
            fault: fault.map(str::to_string),
            paid,
            ..Default::default()
        }
    }

    fn line(id: &str, attempts: Vec<ProbeAttempt>) -> LineProbes {
        LineProbes {
            route_id: id.into(),
            attempts,
            paid_model: Some("m".into()),
            cost_hi: 0.01,
            ..Default::default()
        }
    }

    fn pool(client: Client, lines: Vec<LineProbes>) -> Pool {
        Pool {
            client,
            lines,
            burst: false,
        }
    }

    #[test]
    fn the_liveness_key_sets_match_the_fault_groups() {
        // 测活判读认的是字符串键，分组在 `qb_proxy::classify`；两边各写一份，这里钉住它们对得上。
        use qb_proxy::classify::FaultGroup;
        let all = [
            Fault::Balance,
            Fault::Auth,
            Fault::Retired,
            Fault::UpstreamDown,
            Fault::Timeout,
            Fault::Overloaded,
            Fault::Unknown,
            Fault::RateLimit,
            Fault::Concurrency,
            Fault::ModelMissing,
            Fault::Infra,
            Fault::NotServedHere,
            Fault::ClientRequest,
            Fault::ClientAbort,
        ];
        for f in all {
            let k = f.key();
            let expected = match f.group() {
                FaultGroup::Deterministic => DETERMINISTIC.contains(&k),
                FaultGroup::Transient => TRANSIENT.contains(&k),
                FaultGroup::Busy => BUSY.contains(&k),
                FaultGroup::ModelMissing => k == MODEL_MISSING && SKIP.contains(&k),
                FaultGroup::Neutral => SKIP.contains(&k),
            };
            assert!(expected, "{k} 在测活判读里归错了组");
        }
        assert_eq!(
            DETERMINISTIC.len() + TRANSIENT.len() + BUSY.len() + SKIP.len(),
            all.len(),
            "测活判读多认了一个分类器没有的键"
        );
    }

    #[test]
    fn free_probes_wait_for_traffic_and_backoff_but_a_dead_line_is_retried() {
        let s = ProbeSettings::default();
        let fresh = line("a", vec![]);
        let mut busy = line("b", vec![]);
        busy.last_traffic_ms = Some(NOW - 60_000);
        let up = line("c", vec![att(NOW - 5 * 60_000, true, None, false)]);
        let mut dead_but_busy = line(
            "d",
            vec![att(NOW - 61 * 60_000, false, Some("auth"), false)],
        );
        dead_but_busy.last_traffic_ms = Some(NOW - 60_000);
        let got = plan(
            &[pool(
                Client::ClaudeCode,
                vec![fresh, busy, up, dead_but_busy],
            )],
            &s,
            0.0,
            NOW,
        );
        let ids: Vec<&str> = got.iter().map(|p| p.route_id.as_str()).collect();
        // a 没测过 → 测；b 刚走过流量 → 不测；c 5 分钟前测通、间隔 10 分钟 → 不测；
        // d 判死了（确定性故障一小时后再试）、虽然在接流量也照测。
        assert_eq!(ids, ["a", "d"]);
        assert!(got
            .iter()
            .all(|p| p.kind == ProbeKind::Free && p.model.is_empty()));
    }

    #[test]
    fn a_burst_probes_the_whole_queue_but_keeps_the_minimum_gap() {
        let s = ProbeSettings::default();
        let mut p = pool(
            Client::Codex,
            vec![
                line("a", vec![att(NOW - 2 * 60_000, true, None, false)]),
                line("b", vec![att(NOW - 30_000, true, None, false)]),
            ],
        );
        assert!(plan(&[p.clone()], &s, 0.0, NOW).is_empty());
        p.burst = true;
        let ids: Vec<String> = plan(&[p], &s, 0.0, NOW)
            .into_iter()
            .map(|p| p.route_id)
            .collect();
        assert_eq!(ids, ["a"], "半分钟前刚测过的那条不再测");
    }

    #[test]
    fn paid_probes_only_run_when_switched_on_with_a_choice_and_within_budget() {
        let off = ProbeSettings::default();
        let on = ProbeSettings {
            paid: true,
            daily_budget_cny: 0.025,
            ..Default::default()
        };
        let two = pool(Client::Codex, vec![line("a", vec![]), line("b", vec![])]);
        let one = pool(Client::ClaudeCode, vec![line("solo", vec![])]);
        let pools = [two, one];
        assert!(plan(&pools, &off, 0.0, NOW)
            .iter()
            .all(|p| p.kind == ProbeKind::Free));
        let got = plan(&pools, &on, 0.0, NOW);
        let kinds: Vec<(&str, ProbeKind)> =
            got.iter().map(|p| (p.route_id.as_str(), p.kind)).collect();
        // 预算 0.025 元、一次上界 0.01：只够两次；只有一条线的软件没得挑，只测免费的。
        assert_eq!(
            kinds,
            [
                ("a", ProbeKind::Paid),
                ("b", ProbeKind::Paid),
                ("solo", ProbeKind::Free)
            ]
        );
        // 今天已经花到快封顶了：花钱的一次都不排，退回免费的。
        let got = plan(&pools, &on, 0.02, NOW);
        assert_eq!(got.iter().filter(|p| p.kind == ProbeKind::Paid).count(), 0);
    }

    #[test]
    fn a_dead_line_gets_only_free_probes_and_a_line_without_a_model_is_not_paid_probed() {
        let on = ProbeSettings {
            paid: true,
            ..Default::default()
        };
        let mut no_model = line("n", vec![]);
        no_model.paid_model = None;
        let dead = line(
            "d",
            vec![att(NOW - 2 * 3_600_000, false, Some("balance"), true)],
        );
        let got = plan(&[pool(Client::Codex, vec![no_model, dead])], &on, 0.0, NOW);
        assert!(got.iter().all(|p| p.kind == ProbeKind::Free), "{got:?}");
        assert_eq!(got.len(), 2);
    }

    fn row(at_ms: i64, ok: bool) -> UsageRow {
        UsageRow {
            at_ms,
            ok,
            ..Default::default()
        }
    }

    #[test]
    fn a_burst_needs_two_recent_failures_with_no_success_since_and_a_cooldown() {
        let fails = [row(NOW - 10_000, false), row(NOW - 20_000, false)];
        assert!(burst_needed(&fails, None, NOW));
        assert!(!burst_needed(&fails, Some(NOW - 60_000), NOW), "冷却期内");
        let recovered = [
            row(NOW - 5_000, true),
            row(NOW - 10_000, false),
            row(NOW - 20_000, false),
        ];
        assert!(!burst_needed(&recovered, None, NOW), "之后成功过");
        let old = [row(NOW - 10_000, false), row(NOW - 6 * 60_000, false)];
        assert!(!burst_needed(&old, None, NOW), "另一次在窗口外");
    }

    #[test]
    fn free_and_paid_replies_are_read_the_documented_way() {
        assert_eq!(
            read_reply(
                "data: {\"type\":\"message_start\"}\n\ndata: {\"type\":\"message_stop\"}\n\n"
            ),
            Ok(true)
        );
        assert_eq!(
            read_reply("event: response.incomplete\ndata: {\"type\":\"response.incomplete\"}\n\n"),
            Ok(true),
            "输出上限到了也算通"
        );
        assert_eq!(
            read_reply("data: {\"type\":\"message_start\"}\n\n"),
            Ok(false)
        );
        let err = read_reply(
            "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n",
        )
        .unwrap_err();
        assert_eq!(classify_message(&err), Fault::Overloaded);
        let envelope =
            read_reply("{\"error\":{\"message\":\"insufficient balance\"}}").unwrap_err();
        assert_eq!(classify_message(&envelope), Fault::Balance);
        assert_eq!(
            read_reply("{\"content\":[{\"type\":\"text\",\"text\":\"h\"}]}"),
            Ok(true)
        );
    }

    #[test]
    fn a_probe_costs_a_fraction_of_a_cent_and_an_unknown_price_uses_the_ceiling() {
        let mut route = Route {
            client: Client::Codex,
            ..Default::default()
        };
        let catalog = Catalog::new(vec![]);
        let (lo, hi) = probe_cost(&route, None, &catalog, 7.0);
        assert!(lo.is_none());
        let ceiling = estimate_cost(UNKNOWN_PRICE_PER_MTOK.0, UNKNOWN_PRICE_PER_MTOK.1, 16).1 * 7.0;
        assert!((hi - ceiling).abs() < 1e-12);
        route.rates.input_price = Some(1.25);
        route.rates.output_price = Some(10.0);
        let (lo, hi) = probe_cost(&route, None, &catalog, 7.0);
        let (elo, ehi) = estimate_cost(1.25, 10.0, 16);
        assert!((lo.unwrap() - elo * 7.0).abs() < 1e-12 && (hi - ehi * 7.0).abs() < 1e-12);
        assert!(hi < ceiling);
    }

    /// 假上游收到的每一发：`(路径, x-api-key, authorization, 请求体)`。
    type Seen = std::sync::Arc<std::sync::Mutex<Vec<(String, String, String, String)>>>;

    /// 回固定状态码与响应体的假上游（只绑回环）。
    async fn stub(status: u16, content_type: &'static str, body: &'static str) -> (String, Seen) {
        use http_body_util::{BodyExt, Full};
        use hyper::{body::Incoming, service::service_fn, Request, Response};
        let seen: Seen = Default::default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let log = std::sync::Arc::clone(&seen);
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let log = std::sync::Arc::clone(&log);
                tokio::spawn(async move {
                    let svc = service_fn(move |req: Request<Incoming>| {
                        let log = std::sync::Arc::clone(&log);
                        async move {
                            let header = |n: &str| {
                                req.headers()
                                    .get(n)
                                    .and_then(|v| v.to_str().ok())
                                    .unwrap_or("")
                                    .to_string()
                            };
                            let (path, key, auth) = (
                                req.uri().path().to_string(),
                                header("x-api-key"),
                                header("authorization"),
                            );
                            let raw = req.into_body().collect().await.unwrap().to_bytes();
                            log.lock().unwrap().push((
                                path,
                                key,
                                auth,
                                String::from_utf8_lossy(&raw).into_owned(),
                            ));
                            let mut r = Response::new(Full::new(hyper::body::Bytes::from(body)));
                            *r.status_mut() = hyper::StatusCode::from_u16(status).unwrap();
                            r.headers_mut()
                                .insert("content-type", content_type.parse().unwrap());
                            Ok::<_, std::convert::Infallible>(r)
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(hyper_util::rt::TokioIo::new(socket), svc)
                        .await;
                });
            }
        });
        (base, seen)
    }

    async fn free(status: u16, body: &'static str) -> ProbeAttempt {
        let (base, _) = stub(status, "application/json", body).await;
        let t = ProbeTarget::new("r", Client::ClaudeCode, &base, "k");
        free_probe(&t, &reqwest::Client::new(), NOW).await
    }

    #[tokio::test]
    async fn a_free_probe_sends_an_empty_body_with_the_routers_auth_and_reads_400_as_alive() {
        let (base, seen) = stub(
            400,
            "application/json",
            r#"{"error":{"message":"model is required"}}"#,
        )
        .await;
        let t = ProbeTarget::new("r", Client::ClaudeCode, &format!("{base}/v1"), "line-key");
        let a = free_probe(&t, &reqwest::Client::new(), NOW).await;
        assert!(a.ok && !a.paid && a.cost_hi.is_none(), "{a:?}");
        assert_eq!(a.model, "", "免费的不写模型，免得冲掉「缺模型」的记忆");
        let hits = seen.lock().unwrap().clone();
        assert_eq!(
            hits,
            [(
                "/v1/messages".to_string(),
                "line-key".to_string(),
                String::new(),
                "{}".to_string()
            )]
        );

        let (base, seen) = stub(422, "application/json", "{}").await;
        let t = ProbeTarget::new("r", Client::Codex, &base, "codex-key");
        assert!(free_probe(&t, &reqwest::Client::new(), NOW).await.ok);
        let hits = seen.lock().unwrap().clone();
        assert_eq!(hits[0].0, "/v1/responses");
        assert_eq!(hits[0].2, "Bearer codex-key");
    }

    #[tokio::test]
    async fn a_free_probe_counts_only_what_the_module_doc_lists_as_faults() {
        assert_eq!(free(401, "{}").await.fault.as_deref(), Some("auth"));
        assert_eq!(
            free(400, r#"{"error":{"message":"insufficient balance"}}"#)
                .await
                .fault
                .as_deref(),
            Some("balance"),
            "400 里写着余额不足：照文字"
        );
        assert_eq!(free(429, "{}").await.fault.as_deref(), Some("rate-limit"));
        assert_eq!(
            free(503, "{}").await.fault.as_deref(),
            Some("upstream-down")
        );
        assert!(
            free(500, r#"{"error":"nil pointer"}"#).await.ok,
            "认不出文字的 500：它活着"
        );
        assert_eq!(
            free(500, r#"{"error":"no available accounts"}"#)
                .await
                .fault
                .as_deref(),
            Some("upstream-down")
        );
        assert!(free(200, "{}").await.ok);
        // 连不上：拿一个刚放掉的端口。
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let t = ProbeTarget::new(
            "r",
            Client::ClaudeCode,
            &format!("http://127.0.0.1:{port}"),
            "k",
        );
        let a = free_probe(&t, &reqwest::Client::new(), NOW).await;
        assert!(
            !a.ok && a.fault.as_deref() == Some("upstream-down"),
            "{a:?}"
        );
        assert!(
            !a.error.unwrap_or_default().contains("127.0.0.1"),
            "报错里不许带地址"
        );
    }

    #[tokio::test]
    async fn a_paid_probe_measures_first_byte_charges_after_2xx_and_maps_the_model() {
        let (base, seen) = stub(
            200,
            "text/event-stream",
            "event: message_start\ndata: {\"type\":\"message_start\"}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        )
        .await;
        let mut t = ProbeTarget::new("r", Client::ClaudeCode, &base, "k");
        t.mapping =
            Upstream::default().with_model_map(&qb_station::station::route::RouteModelMap {
                default: Some("relay-sonnet".into()),
                ..Default::default()
            });
        t.cost = (Some(0.001), 0.002);
        let a = paid_probe(&t, "claude-sonnet-5", &reqwest::Client::new(), NOW).await;
        assert!(a.ok && a.paid, "{a:?}");
        assert_eq!(a.model, "claude-sonnet-5", "记的是客户端那头的模型名");
        assert!(a.first_ms.is_some() && a.total_ms >= a.first_ms);
        assert_eq!((a.cost_lo, a.cost_hi), (Some(0.001), Some(0.002)));
        let body: Value = serde_json::from_str(&seen.lock().unwrap()[0].3).unwrap();
        assert_eq!(body["model"], "relay-sonnet", "发出去的是这条线上的名字");
        assert_eq!(body["max_tokens"], 1);

        let (base, _) = stub(
            200,
            "text/event-stream",
            "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n",
        )
        .await;
        let mut t = ProbeTarget::new("r", Client::ClaudeCode, &base, "k");
        t.cost = (Some(0.001), 0.002);
        let a = paid_probe(&t, "m", &reqwest::Client::new(), NOW).await;
        assert_eq!(a.fault.as_deref(), Some("overloaded"));
        assert_eq!(a.cost_hi, Some(0.002), "2xx 之后的失败也可能扣了钱，照记");

        let (base, _) = stub(402, "application/json", "{}").await;
        let mut t = ProbeTarget::new("r", Client::Codex, &base, "k");
        t.cost = (Some(0.001), 0.002);
        let a = paid_probe(&t, "m", &reqwest::Client::new(), NOW).await;
        assert_eq!(a.fault.as_deref(), Some("balance"));
        assert_eq!(a.cost_hi, None, "没到 2xx 的不记钱");
    }

    #[tokio::test]
    async fn run_probes_a_batch_and_returns_one_attempt_each() {
        let (base, _) = stub(400, "application/json", "{}").await;
        let jobs: Vec<_> = (0..5)
            .map(|i| {
                (
                    ProbeTarget::new(&format!("r{i}"), Client::ClaudeCode, &base, "k"),
                    ProbeKind::Free,
                    String::new(),
                )
            })
            .collect();
        let mut got: Vec<String> = run(jobs, &reqwest::Client::new(), NOW)
            .await
            .into_iter()
            .map(|a| a.route_id)
            .collect();
        got.sort();
        assert_eq!(got, ["r0", "r1", "r2", "r3", "r4"]);
    }

    #[test]
    fn probe_settings_round_trip_and_bad_ones_are_not_saved() {
        let dir = std::env::temp_dir().join(format!("qb-liveness-{}", uuid::Uuid::new_v4()));
        let db = Repository::open_in(&dir).unwrap();
        assert_eq!(load_probe_settings(&db).unwrap(), ProbeSettings::default());
        let on = ProbeSettings {
            paid: true,
            daily_budget_cny: 1.5,
            ..Default::default()
        };
        save_probe_settings(&db, &on).unwrap();
        assert_eq!(load_probe_settings(&db).unwrap(), on);
        let bad = ProbeSettings {
            free_interval_min: 0,
            ..Default::default()
        };
        assert!(save_probe_settings(&db, &bad).is_err());
        assert_eq!(
            load_probe_settings(&db).unwrap(),
            on,
            "不合格的那份没存进去"
        );
        // 记账：只数花钱的，按上界扣。
        let mut paid = att(NOW, true, None, true);
        paid.id = "p".into();
        paid.cost_lo = Some(0.001);
        paid.cost_hi = Some(0.003);
        let mut free = att(NOW, true, None, false);
        free.id = "f".into();
        persist(&db, &[paid, free], NOW).unwrap();
        let (lo, hi) = spent_since(&db, NOW - 1).unwrap();
        assert!((lo - 0.001).abs() < 1e-12 && (hi - 0.003).abs() < 1e-12);
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_summary_row_says_when_the_next_probe_is_and_skips_it_for_busy_lines() {
        let s = ProbeSettings::default();
        let attempts = [
            att(NOW - 60_000, true, None, true),
            att(NOW - 120_000, false, Some("timeout"), true),
            att(NOW - 180_000, false, Some("infra"), false),
        ];
        let mut first = attempts.to_vec();
        first[0].total_ms = Some(900);
        let row = summarize("r", "站 · 组", &first, None, 0, Some(0.4), &s, NOW);
        assert_eq!(row.health.state, ProbeState::Up);
        assert_eq!(row.last_probe_ms, Some(NOW - 60_000));
        assert_eq!(row.next_probe_ms, Some(NOW - 60_000 + 10 * 60_000));
        assert_eq!((row.speed_ms, row.speed_samples), (Some(900), 1));
        assert_eq!(row.fail_rate, Some(0.5), "本机这一侧的那次不算");
        // 刚走过真实流量：用不着测。
        let busy = summarize("r", "", &first, Some(NOW - 1_000), 3, None, &s, NOW);
        assert_eq!(busy.next_probe_ms, None);
        assert_eq!(busy.requests_24h, 3);
        // 没测过：现在就测，失败率没有。
        let fresh = summarize("r", "", &[], None, 0, None, &s, NOW);
        assert_eq!((fresh.next_probe_ms, fresh.fail_rate), (Some(NOW), None));
    }

    #[test]
    fn the_probe_model_is_the_most_used_text_model() {
        let req = |m: &str| qb_station::router::RouterRequest {
            model: m.into(),
            ..Default::default()
        };
        let requests = [
            req("b"),
            req("gpt-image-2"),
            req("a"),
            req("b"),
            req("gpt-image-2"),
            req("gpt-image-2"),
        ];
        assert_eq!(
            traffic_models(&requests),
            ["b", "a"],
            "画图模型测不了；同样多按名字排"
        );
        assert!(traffic_models(&[]).is_empty());
    }

    #[tokio::test]
    async fn a_line_that_speaks_another_protocol_is_probed_in_that_protocol() {
        // 阶段 3：Claude 的线标成 OpenAI Chat，真实流量走 /v1/chat/completions + Bearer，测活也得一样，
        // 不然测的是一条这条线根本不接的路径。
        let (base, seen) = stub(
            400,
            "application/json",
            r#"{"error":{"message":"messages required"}}"#,
        )
        .await;
        let t = ProbeTarget::new("r", Client::ClaudeCode, &format!("{base}/v1"), "line-key")
            .speaking(ApiFormat::OpenaiChat);
        assert!(free_probe(&t, &reqwest::Client::new(), NOW).await.ok);
        let hits = seen.lock().unwrap().clone();
        assert_eq!(hits[0].0, "/v1/chat/completions");
        assert_eq!(hits[0].2, "Bearer line-key");
        assert_eq!(hits[0].1, "", "OpenAI 协议不带 x-api-key");

        let (base, seen) = stub(
            200,
            "text/event-stream",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"h\"}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"length\"}]}\n\ndata: [DONE]\n\n",
        )
        .await;
        let t = ProbeTarget::new("r", Client::Codex, &base, "k").speaking(ApiFormat::OpenaiChat);
        let a = paid_probe(&t, "glm-5", &reqwest::Client::new(), NOW).await;
        assert!(a.ok, "{a:?}");
        let body: Value = serde_json::from_str(&seen.lock().unwrap()[0].3).unwrap();
        assert_eq!(body["max_tokens"], 1, "Chat 的输出上限给 1");
        assert!(body["messages"].is_array());
        assert_eq!(max_output(ApiFormat::OpenaiChat), 1);
        assert_eq!(max_output(ApiFormat::OpenaiResponses), RESPONSES_MAX_OUTPUT);
    }

    #[test]
    fn the_day_starts_at_local_midnight() {
        let m = local_midnight_ms(NOW);
        assert!(m <= NOW && NOW - m < 24 * 3_600_000);
        assert_eq!(local_midnight_ms(m), m);
    }
}
