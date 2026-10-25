//! 中转站编排：把领域算法接到真实的网络、数据库和本机路由上。
//!
//! 判定全部在 `qb-station` 里(纯函数、全测过),**这一层只做搬运**:
//! 去拉账单、把结果喂给聚合与排序、把结论写进路由器和库。
//!
//! # 免费的和要花钱的
//!
//! [`refresh_health`] 读的是站点**自己的账单日志**,不发送模型请求,
//! 所以可以自动刷新。真打模型测速和站点检验要花钱,**只在使用者点的时候跑** ——
//! 这一层不提供任何自动触发它们的入口。
//!
//! 唯一的例外是花钱测活(2026-10-24 起,中转站重构阶段 2,见 [`super::station_liveness`]):
//! **默认关**,使用者在调度设置里自己打开之后才由调度循环按每日预算自动跑,每次一个 token。
//!
//! # ⛔ 这里绝不许碰官方账户槽位
//!
//! 中转线路挂了就换中转线路。按额度 / 429 自动切官方槽位是
//! CLAUDE.md 明确不做的「自动换号」。`qb-station` 物理上不认识账户,
//! `architecture.rs` 有测试钉着。

use crate::domain::{Client, Credential, Provider};
use crate::error::{GateError, Result};
use crate::local_router::RouterState;
use crate::repository::Repository;
use qb_platform::repository::RouterRequestQuery;
use qb_station::health::{self, Windows};
use qb_station::router::{
    DispatchQueue, ListenScope, RouterNetwork, RouterRequest, RouterSettings,
};
use qb_station::schedule::liveness::{self, ProbeAttempt, ProbeState, SchedulerAction};
use qb_station::schedule::{
    cheap_values, cost_per_token, rank, Candidate, CheapBasis, CheapInput, Dispatch, LineFacts,
    Pin, Prefs, Ranking, Schedule,
};
use qb_station::station::audit::{AuditRound, Check, CheckKind};
use qb_station::station::endpoints::StationEndpoints;
use qb_station::station::model::{Protocols, UsageRow};
use qb_station::station::pricing::{self, FetchedPrice};
use qb_station::station::route::Route;
use std::collections::HashMap;

/// 一条线路要去哪儿拉账单。
#[derive(Clone, Default)]
pub struct BillingSource {
    pub route_id: String,
    /// 站点基地址。
    pub base_url: String,
    /// 这条线那把 key。
    pub key: String,
    pub client: qb_contract::domain::Client,
    pub group: String,
    pub billing: Option<super::station_billing::LedgerAuth>,
    pub billing_problem: Option<String>,
}

impl std::fmt::Debug for BillingSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BillingSource")
            .field("route_id", &self.route_id)
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}

/// Forwarding, pricing, audits and scheduling must resolve the same route key.
pub fn source(db: &Repository, route: &Route) -> Result<BillingSource> {
    let provider: Provider = db
        .get("providers", &route.station_id)
        .map_err(|_| GateError::Other("这条线路的站点不存在，请编辑线路重新选择站点".into()))?;
    let id = route.credential_id.as_deref().ok_or_else(|| {
        GateError::Other("这条线路没有保存 API Key，请编辑线路重新填写并保存".into())
    })?;
    let credential: Credential = db.get("credentials", id)?;
    if credential.provider_id != route.station_id {
        return Err(GateError::Other(
            "这把 API Key 不属于所选站点，请重新选择凭证".into(),
        ));
    }
    let key = db.key(id)?;
    if key.trim().is_empty() {
        return Err(GateError::Other("API Key 为空，请编辑线路重新填写".into()));
    }
    let base_url = crate::endpoint::endpoint_base(&provider.base_url)?;
    let (billing, billing_problem) =
        match super::station_billing::load(db, &route.station_id, &base_url) {
            Ok(value) => (value, None),
            Err(e) => (None, Some(e.to_string())),
        };
    Ok(BillingSource {
        route_id: route.id.clone(),
        base_url,
        key,
        client: route.client,
        group: route.group.clone(),
        billing,
        billing_problem,
    })
}

pub fn sources(db: &Repository, routes: &[Route]) -> Vec<BillingSource> {
    routes.iter().filter_map(|r| source(db, r).ok()).collect()
}

/// Keep broken routes visible, with an actionable reason instead of blank metrics.
pub fn route_health<'a>(
    db: &Repository,
    routes: &[Route],
    client: &'a reqwest::Client,
    now_ms: i64,
) -> impl std::future::Future<Output = HashMap<String, RouteHealth>> + Send + 'a {
    let sources = sources(db, routes);
    let errors = source_errors(db, routes);
    async move {
        let mut health = refresh_health(&sources, client, now_ms).await;
        health.extend(errors);
        health
    }
}

/// 拿不到账单来源的那几条（没绑 Key、站点被删了……），原因照实写进健康度里。
fn source_errors(db: &Repository, routes: &[Route]) -> HashMap<String, RouteHealth> {
    routes
        .iter()
        .filter_map(|route| {
            source(db, route).err().map(|error| {
                (
                    route.id.clone(),
                    RouteHealth {
                        error: Some(error.to_string()),
                        ..Default::default()
                    },
                )
            })
        })
        .collect()
}

/// 排序用的健康度（中转站重构阶段 1 第 10 步）：**本机路由自己记过的线用它自己的日志**，
/// 没记过的退回站点账单。
///
/// 原来「快 / 稳」只从站点账单来：没在「查套路」里连过站点后台的线永远「没有证据」，
/// 排序退化成只比倍率（交接文档 §2.12 第 6 条）。本机路由的日志是真实流量、成败都记
/// （`status_reported` 为真），而且不用登录任何后台。
///
/// 有本机证据的线**不再去拉账单**：拉了也用不上，白给站点发一个请求。
/// 站点账单那张卡（`route_health` / `station_refresh_health`）照旧只看账单，口径不混。
pub fn decision_health<'a>(
    db: &Repository,
    routes: &[Route],
    client: &'a reqwest::Client,
    now_ms: i64,
) -> impl std::future::Future<Output = HashMap<String, RouteHealth>> + Send + 'a {
    let local = local_health(db, routes, now_ms);
    let rest: Vec<Route> = routes
        .iter()
        .filter(|r| !local.contains_key(&r.id))
        .cloned()
        .collect();
    let sources = sources(db, &rest);
    let errors = source_errors(db, &rest);
    async move {
        let mut health = refresh_health(&sources, client, now_ms).await;
        health.extend(errors);
        health.extend(local.into_iter().map(|(id, windows)| {
            (
                id,
                RouteHealth {
                    windows,
                    error: None,
                },
            )
        }));
        health
    }
}

/// 本机路由日志看多久以前的（跟健康度最长那个窗口一样，7 天）。
const LOCAL_HEALTH_LOOKBACK_MS: i64 = 7 * 24 * 3600 * 1000;

/// 一个软件最多读多少条本机日志来算健康度（7 天、每分钟几发，几万条足够；再多就截掉最旧的）。
const LOCAL_HEALTH_ROWS: u32 = 50_000;

/// 本机路由自己的日志算出来的健康度，按线路。没有日志的线不在里面。读不了库就当没有（退回账单）。
pub fn local_health(db: &Repository, routes: &[Route], now_ms: i64) -> HashMap<String, Windows> {
    let mut clients: Vec<Client> = routes.iter().map(|r| r.client).collect();
    clients.sort_by_key(|c| schedule_key(*c).unwrap_or_default());
    clients.dedup();
    let mut requests = Vec::new();
    for client in clients {
        let Ok(key) = schedule_key(client) else {
            continue;
        };
        match db.router_requests::<RouterRequest>(&RouterRequestQuery {
            since_ms: Some(now_ms - LOCAL_HEALTH_LOOKBACK_MS),
            client: Some(key),
            route_id: None,
            limit: LOCAL_HEALTH_ROWS,
        }) {
            Ok(rows) => requests.extend(rows),
            Err(e) => tracing::warn!("读本机路由日志算健康度失败（这一轮退回站点账单）：{e}"),
        }
    }
    let mut by_route = router_usage_rows(&requests);
    routes
        .iter()
        .filter_map(|r| {
            let rows = by_route.remove(&r.id)?;
            Some((r.id.clone(), health::windows(&rows, now_ms)))
        })
        .collect()
}

/// 本机路由的请求日志换成每条线的证据行。**一发请求试过几条线，就给那几条线各记一行。**
///
/// - 换走了的那几条：记一次失败（成没成功是它自己的事，跟最后那条无关）。
/// - 最后回话的那条：成败照日志（`RouterRequest::ok`，含流半路断掉），token、首字、花费都记在它身上。
/// - 不算线路的错的（请求本身的问题、客户端断开、这条线不接、没有这个模型、本机这一侧）**不记** ——
///   熔断那边对它们记中性，这里也不拿它们拉低成功率。整流前那一次也不记（同一条线紧接着又试了一次）。
pub fn router_usage_rows(requests: &[RouterRequest]) -> HashMap<String, Vec<UsageRow>> {
    use qb_proxy::classify::Fault;
    const NEUTRAL: &[Fault] = &[
        Fault::ClientRequest,
        Fault::ClientAbort,
        Fault::NotServedHere,
        Fault::ModelMissing,
        Fault::Infra,
    ];
    let neutral = |key: &str| NEUTRAL.iter().any(|f| f.key() == key);
    let mut out: HashMap<String, Vec<UsageRow>> = HashMap::new();
    for req in requests {
        let last = req.attempts.len().saturating_sub(1);
        for (i, att) in req.attempts.iter().enumerate() {
            if att
                .fault
                .as_deref()
                .is_some_and(|f| f.starts_with("rectify-"))
            {
                continue;
            }
            let final_one = i == last && att.route_id == req.route_id;
            let fault = if final_one {
                req.fault.as_deref()
            } else {
                att.fault.as_deref()
            };
            if fault.is_some_and(neutral) {
                continue;
            }
            let ok = if final_one {
                req.ok()
            } else {
                att.fault.is_none() && att.status.is_some_and(|s| (200..300).contains(&s))
            };
            let mut row = UsageRow {
                request_id: req.id.clone(),
                at_ms: req.at_ms,
                model: req.model.clone(),
                total_ms: Some(att.ms),
                ok,
                status_reported: true,
                ..Default::default()
            };
            if final_one {
                let read = req.cache_read_tokens;
                let write = req.cache_write_tokens;
                // OpenAI 协议（Codex）报的输入含缓存读写；UsageRow 要的是没缓存的那部分。
                row.input_uncached = req.input_tokens.map(|input| {
                    if req.client == "codex" {
                        input
                            .saturating_sub(read.unwrap_or(0))
                            .saturating_sub(write.unwrap_or(0))
                    } else {
                        input
                    }
                });
                row.cache_read = read;
                row.cache_write = write;
                row.output = req.output_tokens;
                row.first_token_ms = req.first_token_ms;
                row.cost = req.station_cost;
            }
            out.entry(att.route_id.clone()).or_default().push(row);
        }
    }
    out
}

/// 账单日志的端点。
///
/// New API 系的站点都是这个路径。**认不出来的站点拉不到就是拉不到** ——
/// 界面上写「站点未提供」,不要拿一个空窗口冒充「一切正常」。
pub const SELF_LOG_PATH: &str = "/api/log/self";

/// 站点公布自己计费倍率的端点。
pub const STATION_PRICING_PATH: &str = "/api/pricing";

/// 一条线路这一轮拉到了什么。
#[derive(Debug, Clone, Default)]
pub struct RouteHealth {
    pub windows: Windows,
    /// 拉取失败的原因。`Some` 时上面那些窗口是空的,**不是「零请求」**。
    pub error: Option<String>,
}

/// 去站点拉账单并聚合成 1H / 1D / 7D。
///
/// 单条失败不影响别条 —— 一家站点挂了不该让整页的健康度都变成空白。
pub async fn ledger_rows(
    source: &BillingSource,
    client: &reqwest::Client,
) -> std::result::Result<Vec<qb_station::station::model::UsageRow>, String> {
    if let Some(problem) = &source.billing_problem {
        return Err(problem.clone());
    }
    let auth = source
        .billing
        .as_ref()
        .ok_or("尚未连接账单：打开「查套路 → 连接后台」，用站点账号密码或浏览器登录")?;
    let rows = super::station_billing::usage(&source.base_url, auth, client).await?;
    if !source.group.is_empty() && rows.iter().any(|r| r.group.is_empty()) {
        return Err("账单缺少分组信息，无法确认属于这条线路；未把其它分组计入".into());
    }
    Ok(rows
        .into_iter()
        .filter(|r| r.group == source.group)
        .collect())
}

pub async fn refresh_health(
    sources: &[BillingSource],
    client: &reqwest::Client,
    now_ms: i64,
) -> HashMap<String, RouteHealth> {
    let mut out = HashMap::new();
    for source in sources {
        let health = match ledger_rows(source, client).await {
            Ok(rows) => RouteHealth {
                windows: health::windows(&rows, now_ms),
                error: None,
            },
            Err(error) => RouteHealth {
                error: Some(error),
                ..Default::default()
            },
        };
        out.insert(source.route_id.clone(), health);
    }
    out
}

/// New API 在 `/api/status` 里公布的在线充值价:买 1 美元站内额度付几元
/// (它后台「充值价格」那一格,New API 自己的定价页拿它当 `priceRate`)。
///
/// ⛔ **只当参考,不自动采用。** 这一格默认 7.3,很多站根本没改 —— 它们靠兑换码
/// 在别处按 1 元卖;照这个数折算会把那些站冤枉贵七倍。所以界面上只把它写在
/// 「充值比例」旁边,填多少由使用者定。sub2api 没有这一项,返回 `None`。
pub async fn published_topup_price(base_url: &str, client: &reqwest::Client) -> Option<f64> {
    let base = base_url.trim_end_matches('/').trim_end_matches("/v1");
    let v = super::station_billing::json(client.get(format!("{base}/api/status")), "站点状态")
        .await
        .ok()?;
    let data = v.get("data").unwrap_or(&v);
    // 只认 New API 的状态页 —— 带着 quota_per_unit 才是它。
    data.get("quota_per_unit")?;
    qb_station::station::billing::number(data.get("price")?).filter(|n| n.is_finite() && *n > 0.0)
}

/// 三种协议各探一次要打哪条路径。
///
/// ⛔ **一次一种,探三次。** 合成一次(比如只打 `/v1/models`)省不下什么,
/// 却答不了真正要问的问题:这家站点**这一条协议**通不通。`/v1/models` 通
/// 不代表 `/v1/responses` 通 —— 很多中转站只转发 Anthropic 那一套。
/// 把探出来的结论写进 [`Protocols`] 的哪一格。
type SetProtocol = fn(&mut Protocols, Option<bool>);

const PROTOCOL_PATHS: [(&str, SetProtocol); 3] = [
    ("/v1/messages", |p, v| p.anthropic = v),
    ("/v1/responses", |p, v| p.openai_responses = v),
    ("/v1/chat/completions", |p, v| p.openai_chat = v),
];

/// 一个状态码说明这条协议通不通。**三态。**
///
/// 探测发的是一个**故意不合法的空请求体**,所以正常反应就是 400 ——
/// 上游认得这条路径、解析了我们的 body、然后说「你这个 body 不对」。
/// 那正是「这条协议在」的证明,而且**一个 token 都不花**。
///
/// | 状态码 | 结论 | 为什么 |
/// |---|---|---|
/// | 200 / 400 / 422 | `Some(true)` | 上游解析了这条路径上的请求 |
/// | 404 / 405 / 501 | `Some(false)` | 上游明确说这条路径不存在 |
/// | 401 / 403 / 429 / 5xx | `None` | 是 Key 或站点的问题,**跟协议无关** |
///
/// ⛔ **探不出来的不许猜成「不支持」。** 三态存在的全部意义就是接住
/// 最后那一行:Key 填错时三条协议会一起 401,猜成「都不支持」的话,
/// 这条线路在界面上会整个消失,而使用者只是打错了一个字符。
pub fn protocol_verdict(status: u16) -> Option<bool> {
    match status {
        200..=299 | 400 | 422 => Some(true),
        404 | 405 | 501 => Some(false),
        _ => None,
    }
}

/// 探这家站点支持哪几种协议。**免费** —— 发的是不合法的空请求体,
/// 上游连模型都不会碰。
///
/// 探不出来的那几项留 `None`,见 [`protocol_verdict`]。
pub async fn probe_protocols(base_url: &str, key: &str, client: &reqwest::Client) -> Protocols {
    let base = base_url.trim_end_matches('/').trim_end_matches("/v1");
    let mut out = Protocols::default();
    for (path, set) in PROTOCOL_PATHS {
        let mut req = client
            .post(format!("{base}{path}"))
            .header("content-type", "application/json")
            // Anthropic 那条要这个头,少了会被判成格式错误而不是路径不存在 ——
            // 两者都回 400,结论一样,但带上它更接近真实调用。
            .header("anthropic-version", "2023-06-01")
            .body("{}");
        if !key.trim().is_empty() {
            req = req.bearer_auth(key.trim()).header("x-api-key", key.trim());
        }
        let v = match req.send().await {
            Ok(r) => protocol_verdict(r.status().as_u16()),
            // 连不上是网络的问题,不是协议的问题。
            Err(_) => None,
        };
        set(&mut out, v);
    }
    out
}

/// 这一跳该不该换上游,换到哪:派单队列的第一条不是现任就换过去。`None` = 不换。
///
/// # ⛔ 防抖与「不合格的不当第一」都不在这里
///
/// 两件事都由派单([`dispatch`](qb_station::schedule::dispatch))做过了:挑战者要连续几轮
/// 领先够多才换,兜底 / 故障档的线永远不会被换上来当第一(CLAUDE.md「熔断中或不合格的线
/// 不当第一」)。这里再判一次的话就是两道叠在一起,结果是永远不换,而且没有任何地方说得清
/// 为什么。所以这里只问一句:第一条跟现任是不是同一条。
pub fn next_upstream(dispatch: &Dispatch, current: Option<&str>) -> Option<String> {
    let first = dispatch.order.first()?;
    (Some(first.as_str()) != current).then(|| first.clone())
}

// ------------------------------------------------------------ 派单（中转站重构阶段 2）

/// 一个软件这一轮派单要的证据（健康度以外的那些）。
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    /// 每条线在本机路由日志里的证据行（[`router_usage_rows`]）。
    pub usage: HashMap<String, Vec<UsageRow>>,
    /// 每条线最近的测活（新 → 旧）。
    pub probes: HashMap<String, Vec<ProbeAttempt>>,
    /// 花钱测活开着没有。开着时「快」整池改用测活量出来的速度（[`probe_speeds`]）。
    pub paid_probing: bool,
    /// 界面上的显示名（站点 · 分组），见 [`route_label`]。
    pub labels: HashMap<String, String>,
    /// 健康度从站点账单来的那几条，账单是什么时候拉的（它们没有逐条的时间戳，防抖拿这个当「新证据」）。
    pub billing_at: Option<i64>,
}

/// 速度看最近几次花钱测活。
const PROBE_SPEED_SAMPLES: usize = 5;
/// 速度只看多久以内的测活。
const PROBE_SPEED_WINDOW_MS: i64 = 6 * 3600 * 1000;

/// 花钱测活量出来的速度（总耗时中位数，毫秒）与样本数，按线路。
///
/// **整池都有才给**（`None` = 退回本机日志 / 账单的体验分）—— 跟「便宜」整池统一口径一个道理：
/// p1 的体验分是长请求的生成时间，热备只有一个 token 的测活，混着比出来的比值没有意义。
pub fn probe_speeds(
    routes: &[Route],
    evidence: &Evidence,
    now_ms: i64,
) -> Option<HashMap<String, (f64, u32)>> {
    if !evidence.paid_probing || routes.is_empty() {
        return None;
    }
    routes
        .iter()
        .map(|r| {
            let attempts = evidence.probes.get(&r.id)?;
            let (total, _, n) = liveness::speed(
                attempts,
                None,
                now_ms,
                PROBE_SPEED_SAMPLES,
                PROBE_SPEED_WINDOW_MS,
            );
            Some((r.id.clone(), (total? as f64, n)))
        })
        .collect()
}

/// 线路在界面上的名字：站点名 · 分组（站点删了就用站点 id）。
pub fn route_label(route: &Route, providers: &HashMap<String, String>) -> String {
    let station = providers
        .get(&route.station_id)
        .filter(|n| !n.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| route.station_id.clone());
    if route.group.is_empty() {
        station
    } else {
        format!("{station} · {}", route.group)
    }
}

/// 所有站点的显示名，按站点 id。
pub fn provider_names(db: &Repository) -> HashMap<String, String> {
    db.list::<Provider>("providers")
        .unwrap_or_default()
        .into_iter()
        .map(|p| (p.id, p.name))
        .collect()
}

/// 派单要的每条线的事实（[`LineFacts`]）。
///
/// - 样本数：整池用测活速度时是花钱测活的成功次数，否则是本机日志 / 账单里一天的请求数。
/// - 最新证据：本机日志与测活里最新的那一条；只有账单的线用账单拉取的时刻。
/// - 测活判死：[`liveness::assess`] 判死了，**而且判死之后本机路由没在它上面成功过** ——
///   真实流量成功了就是活着，不能让一次测活把正在干活的线打进故障档。
/// - 映射缺模型：这条线上「上游说没有」的模型里，被这条线自己的模型映射改过名的那几个
///   （写成「客户端的名字 → 映射成的名字」）。
pub fn line_facts(
    routes: &[Route],
    health: &HashMap<String, RouteHealth>,
    evidence: &Evidence,
    speeds: Option<&HashMap<String, (f64, u32)>>,
    now_ms: i64,
) -> HashMap<String, LineFacts> {
    routes
        .iter()
        .map(|r| {
            let usage: &[UsageRow] = evidence.usage.get(&r.id).map_or(&[], Vec::as_slice);
            let probes: &[ProbeAttempt] = evidence.probes.get(&r.id).map_or(&[], Vec::as_slice);
            let last_usage = usage.iter().map(|u| u.at_ms).max();
            let last_probe = probes.first().map(|a| a.at_ms);
            let billed = health
                .get(&r.id)
                .is_some_and(|h| h.windows.day.requests > 0);
            let evidence_at = last_usage
                .max(last_probe)
                .or(evidence.billing_at.filter(|_| billed));
            let samples = match speeds {
                Some(s) => s.get(&r.id).map_or(0, |(_, n)| *n),
                None => health.get(&r.id).map_or(0, |h| {
                    u32::try_from(h.windows.day.requests).unwrap_or(u32::MAX)
                }),
            };
            let judged = liveness::assess(probes, now_ms);
            let newest_fault = probes.iter().find(|a| a.is_fault()).map(|a| a.at_ms);
            let recovered = usage
                .iter()
                .any(|u| u.ok && newest_fault.is_some_and(|t| u.at_ms > t));
            let probe_down =
                (judged.state == ProbeState::Down && !recovered).then(|| judged.reason.clone());
            let mapping = crate::local_router::Upstream::default().with_model_map(&r.model_map);
            let mapping_missing = judged
                .missing_models
                .iter()
                .filter_map(|m| mapping.map_model(m).map(|to| format!("{m} → {to}")))
                .collect();
            (
                r.id.clone(),
                LineFacts {
                    samples,
                    evidence_at,
                    probe_down,
                    mapping_missing,
                    label: evidence.labels.get(&r.id).cloned().unwrap_or_default(),
                },
            )
        })
        .collect()
}

/// 一个软件这一轮派单的结果。
#[derive(Debug, Clone)]
pub struct Round {
    pub ranking: Ranking,
    pub basis: CheapBasis,
    pub dispatch: Dispatch,
    /// 「快」这一轮用的是不是测活速度。
    pub probe_speed: bool,
}

/// 排一轮派单：健康度 + 证据 → 排序（不带现任）→ 分档、防抖、热备。**不联网、不碰路由器。**
#[allow(clippy::too_many_arguments)]
pub fn plan_round(
    routes: &[Route],
    health: &HashMap<String, RouteHealth>,
    breaker_state: &HashMap<String, (f64, bool)>,
    schedule: &Schedule,
    queue: &DispatchQueue,
    incumbent: Option<&str>,
    catalog: &pricing::Catalog,
    topup: &HashMap<String, f64>,
    evidence: &Evidence,
    now_ms: i64,
) -> Round {
    let speeds = probe_speeds(routes, evidence, now_ms);
    let speed_ms: Option<HashMap<String, f64>> = speeds
        .as_ref()
        .map(|s| s.iter().map(|(id, (ms, _))| (id.clone(), *ms)).collect());
    let (ranking, basis) = decide(
        routes,
        health,
        breaker_state,
        &schedule.prefs,
        catalog,
        topup,
        speed_ms.as_ref(),
    );
    let facts = line_facts(routes, health, evidence, speeds.as_ref(), now_ms);
    let dispatch = qb_station::schedule::dispatch(
        &ranking,
        &facts,
        incumbent,
        &queue.order,
        queue.pin.as_ref(),
        &queue.memory,
        &schedule.dispatch,
        now_ms,
    );
    Round {
        ranking,
        basis,
        dispatch,
        probe_speed: speeds.is_some(),
    }
}

/// 调度开着时手动选的线钉多久。
pub const PIN_MS: i64 = 4 * 3600 * 1000;

/// 把这一轮派单存进派单队列（顺序、防抖记忆、每一行、告警），过期的钉住顺手清掉。
pub fn save_dispatch(
    db: &Repository,
    client: Client,
    dispatch: &Dispatch,
    now_ms: i64,
) -> Result<DispatchQueue> {
    let mut q = load_dispatch_queue(db, client)?;
    q.order = dispatch.order.clone();
    q.memory = dispatch.memory.clone();
    q.rows = dispatch.rows.clone();
    q.alerts = dispatch.alerts.clone();
    if q.pin.as_ref().is_some_and(|p| p.until_ms <= now_ms) {
        q.pin = None;
    }
    q.updated_ms = now_ms;
    db.put(DISPATCH_TABLE, &schedule_key(client)?, &q)?;
    Ok(q)
}

/// 钉住一条线 [`PIN_MS`]（`None` = 取消钉住）。只收这个软件自己的线。
pub fn set_pin(
    db: &Repository,
    client: Client,
    route_id: Option<&str>,
    now_ms: i64,
) -> Result<DispatchQueue> {
    let mut q = load_dispatch_queue(db, client)?;
    q.pin = match route_id {
        Some(id) => {
            if !id.starts_with(&format!("{}\u{1f}", schedule_key(client)?)) {
                return Err(GateError::Other(format!("钉住的线不是这个软件的（{id}）")));
            }
            Some(Pin {
                route_id: id.to_string(),
                until_ms: now_ms + PIN_MS,
            })
        }
        None => None,
    };
    // 钉住换了人，防抖的记忆作废（那是跟上一个现任比出来的）。
    q.memory = Default::default();
    q.updated_ms = now_ms;
    db.put(DISPATCH_TABLE, &schedule_key(client)?, &q)?;
    Ok(q)
}

/// 动作日志落在哪张表（迁移 7）。
const ACTIONS_TABLE: &str = "scheduler_actions";
/// 动作日志留多少条（一天几条，够看几个月）。
pub const SCHEDULER_ACTIONS_KEEP: usize = 500;

/// 这一轮跟上一轮比做了什么：换了第一条、热备改了顺序。
pub fn round_actions(
    client: Client,
    previous_order: &[String],
    incumbent: Option<&str>,
    dispatch: &Dispatch,
    now_ms: i64,
) -> Result<Vec<SchedulerAction>> {
    let mut out = Vec::new();
    let first = dispatch.order.first();
    if first.is_some_and(|f| Some(f.as_str()) != incumbent) {
        out.push(action(
            client,
            "switch",
            first.cloned(),
            &dispatch.note,
            now_ms,
        )?);
    }
    let standby = |order: &[String]| -> Vec<String> {
        order
            .iter()
            .filter(|id| Some(*id) != first)
            .cloned()
            .collect()
    };
    let before = standby(previous_order);
    // 第一次排（上一轮没有顺序）不算「改了顺序」。
    if !before.is_empty() && before != standby(&dispatch.order) {
        out.push(action(client, "reorder", None, "热备顺序改了", now_ms)?);
    }
    Ok(out)
}

/// 一条动作（`kind` 的几种写法见 [`SchedulerAction`]）。
pub fn action(
    client: Client,
    kind: &str,
    route_id: Option<String>,
    note: &str,
    now_ms: i64,
) -> Result<SchedulerAction> {
    Ok(SchedulerAction {
        id: uuid::Uuid::new_v4().to_string(),
        at_ms: now_ms,
        client: schedule_key(client)?,
        kind: kind.into(),
        route_id,
        note: note.into(),
    })
}

/// 记几条动作，只留最新的 [`SCHEDULER_ACTIONS_KEEP`] 条。
pub fn record_actions(db: &Repository, actions: &[SchedulerAction]) -> Result<()> {
    if actions.is_empty() {
        return Ok(());
    }
    for a in actions {
        db.put(ACTIONS_TABLE, &a.id, a)?;
    }
    db.prune(ACTIONS_TABLE, SCHEDULER_ACTIONS_KEEP, |_| false)?;
    Ok(())
}

/// 最近的动作，新的在前。`client` 给了就只要这个软件的。
pub fn scheduler_actions(
    db: &Repository,
    client: Option<Client>,
    limit: usize,
) -> Result<Vec<SchedulerAction>> {
    let key = client.map(schedule_key).transpose()?;
    let mut all: Vec<SchedulerAction> = db.list(ACTIONS_TABLE)?;
    all.retain(|a| key.as_ref().is_none_or(|k| &a.client == k));
    all.sort_by_key(|a| std::cmp::Reverse(a.at_ms));
    all.truncate(limit);
    Ok(all)
}

/// 每家站点的充值比例(1 美元站内额度付几元),按站点 id。
///
/// 没填的站不在表里 —— [`decide`] 对它按 1 元 = 1 美元额度算
/// ([`DEFAULT_TOPUP_PER_USD`](crate::domain::DEFAULT_TOPUP_PER_USD))。
pub fn topup_ratios(db: &Repository) -> HashMap<String, f64> {
    db.list::<Provider>("providers")
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.topup_per_usd.is_some())
        .map(|p| {
            let v = p.topup_per_usd();
            (p.id, v)
        })
        .collect()
}

/// 这家站 1 美元站内额度付几元。没填(或填的不是正数)按 1 算
/// ([`DEFAULT_TOPUP_PER_USD`](crate::domain::DEFAULT_TOPUP_PER_USD))。
pub fn topup_for(topup: &HashMap<String, f64>, station_id: &str) -> f64 {
    topup
        .get(station_id)
        .copied()
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(crate::domain::DEFAULT_TOPUP_PER_USD)
}

/// 把线路 + 健康度拼成排序用的候选,再排一次。
///
/// `demotion` / `tripped` 由调用方从各自的熔断器里取 —— 熔断器的状态
/// 归本机路由管,这里不复制一份(两份必然漂移)。
///
/// # ⛔ 跨站比较先折成同一种钱
///
/// 账单、倍率都是按站内额度记的,而站内 1 美元额度要付几元每家站不一样
/// (`topup`,见 [`topup_ratios`])。「便宜」的三种口径和「倍率不高于」那条底线
/// 都先乘上各自站点的充值比例,折成「每 $1 官方牌价付几元」再比 ——
/// 否则 7 元一美元、标 ×0.1 的站会压过 1 元一美元、标 ×0.5 的站,而它其实更贵。
pub fn decide(
    routes: &[Route],
    health: &HashMap<String, RouteHealth>,
    breaker_state: &HashMap<String, (f64, bool)>,
    prefs: &Prefs,
    // 官方价目。四类倍率都要拿站点单价除它,见下面那段。
    catalog: &pricing::Catalog,
    // 每家站的充值比例。不在表里的按 1 算。
    topup: &HashMap<String, f64>,
    // 花钱测活量出来的速度(毫秒,按线路)。给了就整池拿它当「快」(见 [`probe_speeds`])。
    speed: Option<&HashMap<String, f64>>,
) -> (Ranking, CheapBasis) {
    let topup_of = |r: &Route| topup_for(topup, &r.station_id);
    // 「便宜」整池统一口径：所有线都拿得出 24h 实扣单价才用它,
    // 差一条就整池退回真实倍率。混着比出来的比值没有意义。
    let inputs: Vec<CheapInput> = routes
        .iter()
        .map(|r| {
            let t = topup_of(r);
            let day = health.get(&r.id).map(|h| &h.windows.day);
            // 加权等效倍率:拿这条线实际的 token 结构去加权它四类的
            // 「站点单价 ÷ 官方单价」。这是「一家把输出另外加了价、一家照官方填,
            // 怎么比」的答案。
            //
            // 官方价按「这组价是哪个模型的」去查,**两种后端都非有它不可** ——
            // New API 的倍率换成单价之后也要除官方单价才是倍数。查不到就算不出
            // 加权倍率,整池退回更粗的口径。**那是对的** —— 拿一个算不出来的数
            // 去比,比退回粗口径糟得多。
            let official = r
                .rates_model
                .as_deref()
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .and_then(|m| catalog.resolve(m));
            let blended = day
                .and_then(|d| d.mix.shares())
                .zip(official.as_ref())
                .and_then(|(mix, o)| r.rates.blended_ratio(mix, o));
            CheapInput {
                route_id: r.id.clone(),
                cost_per_token: day
                    .and_then(|d| cost_per_token(d.cost, d.tokens))
                    .map(|v| v * t),
                blended_ratio: blended.map(|v| v * t),
                real_rate: r.rate_for_ranking().map(|v| v * t),
            }
        })
        .collect();
    let (basis, cheap) = cheap_values(&inputs);

    let candidates: Vec<Candidate> = routes
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let day = health.get(&r.id).map(|h| &h.windows.day);
            let (demotion, tripped) = breaker_state.get(&r.id).copied().unwrap_or((1.0, false));
            Candidate {
                route_id: r.id.clone(),
                rate: cheap[i],
                // 底线「倍率不高于」比真实倍率(没有就退回加权倍率),都已折过充值比例。
                // ⛔ 不拿 `cheap[i]`:整池走实扣单价时那是 1e-6 量级,底线等于没设。
                real_rate: inputs[i].real_rate.or(inputs[i].blended_ratio),
                experience_ms: match speed {
                    Some(v) => v.get(&r.id).copied(),
                    None => day.and_then(|d| d.experience_ms),
                },
                ttft_p95_ms: day.and_then(|d| d.ttft_p95_ms),
                success_rate: day.and_then(|d| d.success_rate),
                demotion,
                tripped,
            }
        })
        .collect();

    // ⛔ 不给 incumbent:换不换第一条只由派单那一道防抖管(`schedule::dispatch` 的模块头)。
    (rank(&candidates, prefs, None), basis)
}

/// 一轮检验的产物:结论,外加**顺手学到的那家站点的价目**。
///
/// 价目单独带出来而不是塞进 [`AuditRound`],是因为两者的去处不同:
/// 结论进检验历史(一轮一条,不可变),价目回流到线路上(只留最新一份,
/// 排序每次都要读)。塞进同一个结构里,排序就得去翻历史里最近的那一轮 ——
/// 而「最近一轮」这个概念在并发下不稳。
#[derive(Debug, Clone)]
pub struct AuditOutcome {
    pub round: AuditRound,
    /// 站点公布的这个模型的价。`None` = 它没公布价目表,或者表里没这个模型。
    pub rates: Option<pricing::StationRates>,
    /// 上面那份价是哪个模型的。写回线路的 `rates_model`。
    pub model: String,
}

/// Six controlled requests; an optional seventh cold-prefix control requires explicit consent.
pub async fn run_audit(
    source: &BillingSource,
    claimed_rate: Option<f64>,
    model: &str,
    catalog: &pricing::Catalog,
    previous: Option<&AuditRound>,
    client: &reqwest::Client,
    now_ms: i64,
) -> AuditOutcome {
    run_audit_with(
        source,
        claimed_rate,
        model,
        catalog,
        previous,
        client,
        now_ms,
        false,
        &|_, _, _| {},
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_audit_with(
    source: &BillingSource,
    claimed_rate: Option<f64>,
    model: &str,
    catalog: &pricing::Catalog,
    previous: Option<&AuditRound>,
    client: &reqwest::Client,
    now_ms: i64,
    cold: bool,
    notify: &(dyn Fn(u32, u32, &str) + Send + Sync),
) -> AuditOutcome {
    let (batch, mut problems) = super::station_batch::run(
        &source.base_url,
        &source.key,
        source.client,
        &source.group,
        source.billing.as_ref(),
        model,
        catalog,
        client,
        cold,
        now_ms,
        notify,
    )
    .await;
    let rate = batch
        .total_billed
        .zip(batch.official_cost)
        .filter(|(b, o)| b.is_finite() && *b >= 0.0 && o.is_finite() && *o > 0.0)
        .filter(|_| batch.samples.iter().all(|s| s.match_kind == "request-id"))
        .map(|(b, o)| b / o);
    let ttfts: Vec<_> = batch
        .samples
        .iter()
        .filter_map(|s| s.first_token_ms)
        .collect();
    let ttft = (!ttfts.is_empty()).then(|| ttfts.iter().sum::<f64>() / ttfts.len() as f64);
    let checks = CheckKind::ALL
        .into_iter()
        .map(|kind| {
            let claimed = if kind == CheckKind::Rate {
                claimed_rate
            } else {
                None
            };
            let prev = previous
                .and_then(|r| r.checks.iter().find(|c| c.kind == kind))
                .and_then(|c| c.measured);
            let value = match kind {
                CheckKind::Rate => rate,
                CheckKind::CacheHit => batch.prefix_reuse,
                CheckKind::FirstToken => ttft,
                _ => None,
            };
            value
                .map(|v| Check::measured(kind, claimed, prev, v))
                .unwrap_or_else(|| Check::unmeasured(kind, claimed, prev))
        })
        .collect();
    problems.push(
        "API 用量、账单与余额均由站点提供；本报告不鉴定模型身份、上下文上限或全站成功率".into(),
    );
    let station_rates = fetch_station_models(source, client)
        .await
        .into_iter()
        .find(|m| m.model == model && m.in_group(&source.group))
        .map(|m| m.rates);
    let rates = station_rates
        .as_ref()
        .map(|r| pricing::verdicts(r, catalog.resolve(model).as_ref()))
        .unwrap_or_default();
    let mut round = AuditRound::with_rates(now_ms, checks, rates, model.into());
    round.problems = problems;
    round.batch = Some(batch);
    AuditOutcome {
        round,
        rates: station_rates,
        model: model.into(),
    }
}

/// 去站点拉它自己公布的模型表(`/api/pricing`):每个模型的计费倍率 +
/// 它属于哪几个分组。
///
/// 界面上「站点 → 分组 → 模型」那三级就是从这里来的。
///
/// 拉不到就是空表 —— 上层照样能用(检验那一项记「没测到」,界面上让使用者
/// 自己填模型名),**不编数**。
pub async fn fetch_station_models(
    source: &BillingSource,
    client: &reqwest::Client,
) -> Vec<pricing::StationModel> {
    fetch_models(source, client).await.0
}

pub async fn fetch_models(
    source: &BillingSource,
    client: &reqwest::Client,
) -> (Vec<pricing::StationModel>, Option<String>) {
    let base = source
        .base_url
        .trim_end_matches('/')
        .trim_end_matches("/v1");
    let pricing = super::station_billing::json(
        client.get(format!("{base}{STATION_PRICING_PATH}")),
        "站点价目表",
    )
    .await;
    let problem = match pricing {
        Ok(value) => {
            // 分组倍率在响应顶层,按这条线路的分组取 —— 大多数站的折扣就在这个数上。
            let models = pricing::parse_station_pricing_in_group(&value.to_string(), &source.group);
            if !models.is_empty() {
                return (models, None);
            }
            "站点价目表未提供可识别的模型".into()
        }
        Err(problem) => problem,
    };
    if let Some(auth) = &source.billing {
        if let Ok(models) =
            super::station_billing::models(&source.base_url, auth, client, &source.group).await
        {
            if !models.is_empty() {
                return (models, None);
            }
        }
    }
    match super::station_billing::json(
        client
            .get(format!("{base}/v1/models"))
            .bearer_auth(&source.key),
        "模型列表",
    )
    .await
    {
        Ok(value) => {
            let models = value
                .get("data")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| v.get("id").and_then(serde_json::Value::as_str))
                .filter(|s| !s.trim().is_empty())
                .map(|model| pricing::StationModel {
                    model: model.into(),
                    rates: Default::default(),
                    groups: vec![],
                })
                .collect();
            (
                models,
                Some(format!(
                    "{problem}；已尝试使用 API Key 的模型列表，未编造价格。也可以手动输入模型"
                )),
            )
        }
        Err(error) => (
            Vec::new(),
            Some(format!("{problem}；{error}。可手动输入模型")),
        ),
    }
}

/// 官方定价文档的地址与表格格式。
///
/// ⚠ **这是文档页,不是 API。** Models API 不返回价格,官方也没有别的
/// 机器可读的价格接口 —— 所以「实时更新」只能是抓这两个页面。
///
/// ⛔ **两家的表格完全不是一个形状**,所以格式要跟着 URL 一起写死:
///
/// - Anthropic 5 列:`基础输入 | 5m 缓存写 | 1h 缓存写 | 缓存读 | 输出`,
///   模型那一列是显示名;
/// - OpenAI 8 列:短上下文 4 列 + 长上下文 4 列。
///
/// 配反了会读出一堆错价 —— `parse_pricing_markdown` 里有一条测试专门钉这件事
/// (拿错格式解析必须返回空)。
///
/// ⚠ 地址是 2026-09-14 实访确认的。之前写的
/// `platform.claude.com/docs/en/pricing.md` **是 404** ——
/// 那意味着更新永远失败、永远悄悄沿用内置快照,而且没有任何症状。
/// 改地址之前先 curl 一下。
pub const PRICING_SOURCES: &[(&str, &str, pricing::PricingFormat)] = &[
    (
        "Anthropic",
        "https://platform.claude.com/docs/en/about-claude/pricing.md",
        pricing::PricingFormat::Anthropic,
    ),
    (
        "OpenAI",
        "https://developers.openai.com/api/docs/pricing.md",
        pricing::PricingFormat::OpenAi,
    ),
];

/// 抓一轮官方价。**启动时跑一次。**
///
/// 返回 `(抓到的价, 每个来源的失败原因)`。
///
/// ⛔ **抓失败不是错误。** 网络不通、页面改版都很常见,这时继续用内置快照,
/// 把原因带回去让界面写一句「用的是内置价目(某年某月核对)」——
/// 而不是让整个中转站页面报错。
pub async fn fetch_prices(
    client: &reqwest::Client,
    today: &str,
) -> (Vec<FetchedPrice>, Vec<String>) {
    let mut prices: Vec<FetchedPrice> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    for (name, url, format) in PRICING_SOURCES {
        let got = client
            .get(*url)
            .send()
            .await
            .and_then(|r| r.error_for_status());
        match got {
            Ok(resp) => match resp.text().await {
                Ok(body) => {
                    let parsed = pricing::parse_pricing_markdown(&body, today, url, *format);
                    if parsed.is_empty() {
                        problems.push(format!(
                            "{name} 的定价页拿到了但认不出表格（多半是改版了），继续用内置价目"
                        ));
                    } else {
                        prices.extend(parsed);
                    }
                }
                Err(e) => problems.push(format!("{name} 定价页读不出正文：{e}")),
            },
            Err(e) => problems.push(format!("{name} 定价页拉不到：{e}")),
        }
    }
    (prices, problems)
}

/// 请求日志留多少天，更早的落库时裁掉。
///
/// 上限跟别的历史一样是**有意的**:请求日志涨得比谁都快,不封顶的话库会一直长。
/// 原来按条数留 2000 条（`request_logs` 只能全表读、逐条删）；新表按 `at_ms` 建了索引，
/// 改按天留 —— 「最近 30 天花了多少」才有得算，用量页要的就是这个窗口。
pub const ROUTER_REQUEST_KEEP_DAYS: i64 = 30;

/// 智能调度落在哪张表：一个软件一行。
const SCHEDULE_TABLE: &str = "station_schedule";

/// 调度那一行的主键：软件在 IPC 上的名字。**直接取 `Client` 的 serde 写法**，
/// 不再另写一份映射 —— 两份映射迟早会漂，漂了之后存的和读的就不是同一行。
fn schedule_key(client: Client) -> Result<String> {
    match serde_json::to_value(client)? {
        serde_json::Value::String(key) => Ok(key),
        other => Err(GateError::Other(format!(
            "软件名序列化成了意外的形状：{other}"
        ))),
    }
}

/// 读一个软件的智能调度。**没存过 = 关着的那一份**（[`Schedule::new`]）。
///
/// ⛔ 只有「没有这一行」才回落成默认值；表不在、库读不了、存的读不出来都是错误，照实往上报。
/// 0.16.0–0.25.6 这里是 `get(..).unwrap_or_else(|_| Schedule::new(c))`：
/// 那张表从没建过，每一次读都失败、都被吞成「关着」，而界面上看起来只是「没开」。
pub fn load_schedule(db: &Repository, client: Client) -> Result<Schedule> {
    Ok(db
        .find(SCHEDULE_TABLE, &schedule_key(client)?)?
        .unwrap_or_else(|| Schedule::new(client)))
}

/// 存一个软件的智能调度（主键取 `schedule.client`）。
pub fn save_schedule(db: &Repository, schedule: &Schedule) -> Result<()> {
    db.put(SCHEDULE_TABLE, &schedule_key(schedule.client)?, schedule)
}

// ------------------------------------------------------------ 本机路由的设置与派单队列

/// 本机路由每个软件一份的设置落在哪张表（迁移 6）。主键跟调度一样是软件名。
const ROUTER_SETTINGS_TABLE: &str = "router_settings";
/// 派单队列（当前那条 + 手动顺序）落在哪张表（迁移 6）。
const DISPATCH_TABLE: &str = "dispatch_queue";

/// 有中转路径的三个软件。反重力没有（`workspace::reject_if_no_relay_path`）。
pub const RELAY_CLIENTS: [Client; 3] = [Client::ClaudeCode, Client::ClaudeDesktop, Client::Codex];

/// 读一个软件的本机路由设置。**没存过 = 默认那一份**（[`RouterSettings::for_client`]）；
/// 别的失败照实报（跟 [`load_schedule`] 同一条规矩）。
pub fn load_router_settings(db: &Repository, client: Client) -> Result<RouterSettings> {
    let mut s: RouterSettings = db
        .find(ROUTER_SETTINGS_TABLE, &schedule_key(client)?)?
        .unwrap_or_else(|| RouterSettings::for_client(client));
    // 主键说了算：存的那一份里 client 写错了也不许串到别的软件上。
    s.client = client;
    Ok(s)
}

/// 存一个软件的本机路由设置。先理顺聚合名单（[`settle_aggregate`]），再 [`RouterSettings::validate`]，
/// 不合格就整份不存。返回真正存下的那份（聚合模型的 key 是这里分的，界面要读回去）。
pub fn save_router_settings(db: &Repository, settings: &RouterSettings) -> Result<RouterSettings> {
    let mut settings = settings.clone();
    settle_aggregate(db, &mut settings)?;
    settings.validate().map_err(GateError::Other)?;
    db.put(
        ROUTER_SETTINGS_TABLE,
        &schedule_key(settings.client)?,
        &settings,
    )?;
    Ok(settings)
}

/// 聚合名单（中转站重构阶段 3）存之前理一遍：
///
/// - **key 登记簿以库里那份为准、只增不减**，界面传来的 `keys` 不认：key 一旦换了指向，客户端带着的旧 id
///   会被悄悄发到另一条线（cc-switch 的 `allocate_key` 同样只增不减）；
/// - 名单只收这个软件自己的线（混进别的软件的、官方 Codex 那条，整份拒绝 —— 跟派单顺序同一条规矩）；去空、去重；
/// - 名单里还没 key 的按站点名 → 站点域名 → 分组名分一个（中文名转不出 key，就往后取）；
/// - 每条线发布的模型去空白、去重，只留名单里那些线的。
fn settle_aggregate(db: &Repository, settings: &mut RouterSettings) -> Result<()> {
    let client = settings.client;
    let prefix = format!("{}\u{1f}", schedule_key(client)?);
    let stored = load_router_settings(db, client)?.aggregate;
    let a = &mut settings.aggregate;
    let mut members: Vec<String> = Vec::new();
    for id in std::mem::take(&mut a.members) {
        let id = id.trim().to_string();
        if id.is_empty() || members.contains(&id) {
            continue;
        }
        if !id.starts_with(&prefix) {
            return Err(GateError::Other(format!(
                "聚合名单里混进了别的软件的线（{id}）：每个软件只能聚合自己的中转线"
            )));
        }
        members.push(id);
    }
    a.models = std::mem::take(&mut a.models)
        .into_iter()
        .filter(|(id, _)| members.contains(id))
        .map(|(id, list)| {
            let mut clean: Vec<String> = Vec::new();
            for m in list {
                let m = m.trim().to_string();
                if !m.is_empty() && !clean.contains(&m) {
                    clean.push(m);
                }
            }
            (id, clean)
        })
        .filter(|(_, list)| !list.is_empty())
        .collect();
    a.keys = stored.keys;
    let fresh: Vec<&String> = members
        .iter()
        .filter(|id| !a.keys.values().any(|r| r == *id))
        .collect();
    if !fresh.is_empty() {
        let routes: Vec<Route> = db.list("station_routes")?;
        let providers: Vec<Provider> = db.list("providers")?;
        for id in fresh {
            let route = routes.iter().find(|r| &r.id == id);
            let provider = route.and_then(|r| providers.iter().find(|p| p.id == r.station_id));
            let name = provider.map(|p| p.name.as_str()).unwrap_or("");
            let host = provider.map(|p| host_hint(&p.base_url)).unwrap_or("");
            let group = route.map(|r| r.group.as_str()).unwrap_or("");
            qb_proxy::aggregate::allocate_key(&mut a.keys, id, &[name, host, group]);
        }
    }
    a.members = members;
    Ok(())
}

/// 站点地址里拿来生成聚合 key 的那一段：主机名去掉 `api.` / `www.` 和顶级域（`https://api.kimi.com/v1` → `kimi`）。
fn host_hint(base_url: &str) -> &str {
    let rest = base_url.split_once("://").map_or(base_url, |(_, r)| r);
    let host = rest.split(['/', ':', '?']).next().unwrap_or("");
    let host = host
        .strip_prefix("api.")
        .or_else(|| host.strip_prefix("www."))
        .unwrap_or(host);
    host.rsplit_once('.').map_or(host, |(name, _)| name)
}

// ------------------------------------------------------------------ 站点的备用地址（阶段 4，多端点）

const ENDPOINTS_TABLE: &str = "station_endpoints";

/// 读一个站点的备用地址。没存过 = 一个都没有。
pub fn load_endpoints(db: &Repository, provider_id: &str) -> Result<StationEndpoints> {
    let mut e: StationEndpoints = db
        .find(ENDPOINTS_TABLE, provider_id)?
        .unwrap_or_else(|| StationEndpoints::new(provider_id));
    // 主键说了算（跟路由设置同一条）。
    e.provider_id = provider_id.to_string();
    Ok(e)
}

pub fn save_endpoints(db: &Repository, e: &StationEndpoints) -> Result<()> {
    db.put(ENDPOINTS_TABLE, &e.provider_id, e)
}

/// 给站点加一个备用地址（跟主地址或已有的重复就不加）。地址照主地址的规矩查：不许带凭证、查询串、片段。
pub fn add_endpoint(
    db: &Repository,
    provider_id: &str,
    url: &str,
    now_ms: i64,
) -> Result<StationEndpoints> {
    let provider: Provider = db
        .get("providers", provider_id)
        .map_err(|_| GateError::Other("这个站点不存在".into()))?;
    let url = crate::endpoint::endpoint_base(url)?;
    let mut e = load_endpoints(db, provider_id)?;
    e.add(&url, &provider.base_url, now_ms)
        .map_err(GateError::Other)?;
    save_endpoints(db, &e)?;
    Ok(e)
}

pub fn remove_endpoint(db: &Repository, provider_id: &str, url: &str) -> Result<StationEndpoints> {
    let mut e = load_endpoints(db, provider_id)?;
    e.remove(url);
    save_endpoints(db, &e)?;
    Ok(e)
}

/// 把一个备用地址换成主地址：站点的 `base_url` 改成它，原来的主地址挪进备用（cc-switch「用这个地址」那一下）。
pub fn promote_endpoint(
    db: &Repository,
    provider_id: &str,
    url: &str,
    now_ms: i64,
) -> Result<Provider> {
    let mut provider: Provider = db
        .get("providers", provider_id)
        .map_err(|_| GateError::Other("这个站点不存在".into()))?;
    let mut e = load_endpoints(db, provider_id)?;
    let Some(chosen) = e
        .endpoints
        .iter()
        .find(|x| qb_station::station::endpoints::same(&x.url, url))
        .map(|x| x.url.clone())
    else {
        return Err(GateError::Other("这个地址不在这个站点的备用地址里".into()));
    };
    let old_main = std::mem::replace(&mut provider.base_url, chosen.clone());
    e.remove(&chosen);
    e.add(&old_main, &provider.base_url, now_ms)
        .map_err(GateError::Other)?;
    provider.revision = provider.revision.saturating_add(1);
    db.put("providers", provider_id, &provider)?;
    save_endpoints(db, &e)?;
    Ok(provider)
}

/// 一条线的备用地址，按故障转移的先后（[`qb_station::station::endpoints::failover_order`]），每个都照主地址的规矩规范化过；
/// 规范化不了的跳过。读不了库就当没有。
pub fn alternates(db: &Repository, provider: &Provider) -> Vec<String> {
    let Ok(e) = load_endpoints(db, &provider.id) else {
        return Vec::new();
    };
    let Ok(main) = crate::endpoint::endpoint_base(&provider.base_url) else {
        return Vec::new();
    };
    qb_station::station::endpoints::failover_order(&main, &e.endpoints)
        .into_iter()
        .filter_map(|u| crate::endpoint::endpoint_base(&u).ok())
        .collect()
}

/// 测一组地址的响应耗时（cc-switch `SpeedtestService::test_endpoints`）：先发一发热身（复用连接、躲开首包的额外耗时），
/// 第二发计时。任何状态码都算通 —— 测的是地址，不带 Key、不调用模型、不花钱。只在点「测速」时跑。
pub async fn test_endpoints(
    client: &reqwest::Client,
    urls: Vec<String>,
    timeout_secs: Option<u64>,
) -> Vec<qb_station::station::endpoints::EndpointLatency> {
    use qb_station::station::endpoints::{sanitize_timeout, EndpointLatency};
    let timeout = std::time::Duration::from_secs(sanitize_timeout(timeout_secs));
    let tasks = urls.into_iter().map(|url| {
        let client = client.clone();
        async move {
            let Ok(parsed) = reqwest::Url::parse(url.trim()) else {
                return EndpointLatency {
                    error: Some("地址无效".into()),
                    url,
                    latency_ms: None,
                    status: None,
                };
            };
            let _ = client.get(parsed.clone()).timeout(timeout).send().await;
            let start = std::time::Instant::now();
            match client.get(parsed).timeout(timeout).send().await {
                Ok(resp) => EndpointLatency {
                    url,
                    latency_ms: Some(start.elapsed().as_millis() as u64),
                    status: Some(resp.status().as_u16()),
                    error: None,
                },
                Err(e) => EndpointLatency {
                    url,
                    latency_ms: None,
                    status: e.status().map(|s| s.as_u16()),
                    error: Some(if e.is_timeout() {
                        "请求超时".into()
                    } else if e.is_connect() {
                        "连接失败".into()
                    } else {
                        // 不带地址（`without_url`）：错误文字里会带上整个请求地址。
                        e.without_url().to_string()
                    }),
                },
            }
        }
    });
    futures_util::future::join_all(tasks).await
}

/// 读一个软件的派单队列。没存过 = 空的（没有上次选中的那条、没有顺序）。
pub fn load_dispatch_queue(db: &Repository, client: Client) -> Result<DispatchQueue> {
    let mut q: DispatchQueue = db
        .find(DISPATCH_TABLE, &schedule_key(client)?)?
        .unwrap_or_else(|| DispatchQueue::new(client));
    q.client = client;
    Ok(q)
}

/// 存派单顺序。**只收这个软件自己的线**（按 `Route::make_id` 的软件前缀认），去重、去空；
/// 别的软件的线混进来就整份拒绝 —— 换过去等于拿这个软件的请求打别人的 Key。
pub fn save_dispatch_order(
    db: &Repository,
    client: Client,
    order: Vec<String>,
    now_ms: i64,
) -> Result<DispatchQueue> {
    let prefix = format!("{}\u{1f}", schedule_key(client)?);
    let mut clean: Vec<String> = Vec::new();
    for id in order {
        let id = id.trim().to_string();
        if id.is_empty() || clean.contains(&id) {
            continue;
        }
        if !id.starts_with(&prefix) {
            return Err(GateError::Other(format!(
                "派单顺序里混进了别的软件的线（{id}）：每个软件只能排自己的线"
            )));
        }
        clean.push(id);
    }
    let mut q = load_dispatch_queue(db, client)?;
    q.order = clean;
    q.updated_ms = now_ms;
    db.put(DISPATCH_TABLE, &schedule_key(client)?, &q)?;
    Ok(q)
}

/// 记下这个软件上次选中的那条（手动选的、调度换过去的都记），面板重启后接着用。
pub fn remember_current(
    db: &Repository,
    client: Client,
    route_id: &str,
    now_ms: i64,
) -> Result<()> {
    let mut q = load_dispatch_queue(db, client)?;
    if q.current.as_deref() == Some(route_id) {
        return Ok(());
    }
    q.current = Some(route_id.to_string());
    q.updated_ms = now_ms;
    db.put(DISPATCH_TABLE, &schedule_key(client)?, &q)
}

/// 把落盘的设置、派单顺序、上次选中的那条装回路由器（面板启动时、路由拉起来之前）。
///
/// 上次选中的那条**只在这个软件眼下还没选时**装回去 —— 面板开着期间使用者或调度刚选过的，
/// 不该被库里那份旧的盖掉。那条线后来被删了也照样装回去：下一发如实回 503「还没选上游」，
/// 而不是悄悄换成另一条（那是替使用者做决定）。
pub fn restore_router(db: &Repository, state: &mut RouterState) -> Result<()> {
    for client in RELAY_CLIENTS {
        state.set_settings(load_router_settings(db, client)?);
        let q = load_dispatch_queue(db, client)?;
        state.set_queue(client, q.order);
        if state.current(client).is_none() {
            if let Some(id) = q.current {
                state.set_now(client, id);
            }
        }
    }
    // 出站代理：设了就把中转线路的客户端换成走它的（官方线不受影响，见 `RouterState::relay_http`）。
    state.set_relay_http(relay_client(&load_router_network(db)?)?);
    Ok(())
}

// ------------------------------------------------------------ 本机路由的网络设置（R6）

/// 本机路由的网络设置存在 metadata 的这个键下（三个软件共用一份，不用另建表）。
const NETWORK_META: &str = "router_network";

/// 读本机路由的网络设置。没存过 = 只本机、不走代理。
pub fn load_router_network(db: &Repository) -> Result<RouterNetwork> {
    match db.meta(NETWORK_META)? {
        Some(raw) => Ok(serde_json::from_str(&raw)?),
        None => Ok(RouterNetwork::default()),
    }
}

/// 存本机路由的网络设置。出站代理不合格、开到所有网卡上却没有口令 —— 整份不存。
pub fn save_router_network(db: &Repository, net: &RouterNetwork) -> Result<()> {
    if let Some(url) = &net.outbound_proxy {
        qb_station::router::check_proxy_url(url).map_err(GateError::Other)?;
    }
    if net.scope == ListenScope::AllInterfaces && net.key.trim().is_empty() {
        return Err(GateError::Other(
            "开到所有网卡上之前要先有路由口令（不设的话，同一个网络里谁都能拿你的 Key 打你的额度）"
                .into(),
        ));
    }
    db.set_meta(NETWORK_META, &serde_json::to_string(net)?)
}

/// 新的路由口令：两段 v4 UUID（操作系统的密码学随机数），带个前缀好认。
pub fn new_router_key() -> String {
    format!(
        "qbr-{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// 中转线路走的出站客户端。没设出站代理就是 `None`（用路由默认的直连客户端）。
///
/// 连接超时跟默认那个一样是 15 秒；读超时由路由自己的首包 / 静默 / 非流式三道管，这里不设总超时 ——
/// SSE 生成可以合法地跑很久。
pub fn relay_client(net: &RouterNetwork) -> Result<Option<reqwest::Client>> {
    let Some(url) = net.outbound_proxy.as_deref() else {
        return Ok(None);
    };
    qb_station::router::check_proxy_url(url).map_err(GateError::Other)?;
    let proxy = reqwest::Proxy::all(url).map_err(|e| {
        GateError::Other(format!(
            "出站代理 {} 用不了：{e}",
            qb_station::router::mask_proxy_url(url)
        ))
    })?;
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .proxy(proxy)
        .build()
        .map_err(|e| GateError::Other(format!("建不出走代理的客户端：{e}")))?;
    Ok(Some(client))
}

/// 本机常见的代理端口（cc-switch v4.0.14 `commands/global_proxy.rs` 的 `PROXY_PORTS`，MIT）。
/// `(端口, 先按哪种协议认, 是不是 HTTP + SOCKS 混合端口)`。
pub const COMMON_PROXY_PORTS: &[(u16, &str, bool)] = &[
    (7890, "http", true),     // Clash（混合端口）
    (7897, "http", true),     // Clash Verge（混合端口）
    (7891, "socks5", false),  // Clash 只开 SOCKS
    (1080, "socks5", false),  // 通用 SOCKS5
    (8080, "http", false),    // 通用 HTTP
    (8888, "http", false),    // Charles / Fiddler
    (3128, "http", false),    // Squid
    (10808, "socks5", false), // V2Ray SOCKS
    (10809, "http", false),   // V2Ray HTTP
];

/// 扫一遍本机常见代理端口，回连得上的那些（`http://127.0.0.1:7890` 这种写法；混合端口两种都给）。
///
/// **只连 `127.0.0.1`、每个端口 100 毫秒、不发任何数据**（cc-switch 的 `scan_local_proxies`）。
/// 只在使用者点「扫描」时跑。
pub fn scan_local_proxies() -> Vec<String> {
    let mut found = Vec::new();
    for &(port, kind, mixed) in COMMON_PROXY_PORTS {
        let addr = std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port));
        if std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(100))
            .is_ok()
        {
            found.push(format!("{kind}://127.0.0.1:{port}"));
            if mixed {
                found.push(format!("socks5://127.0.0.1:{port}"));
            }
        }
    }
    found
}

/// 经出站代理试连一个地址：收到任何 HTTP 回答就算代理通（状态码不管 —— 站点回 401 / 404 也说明连上了）。
///
/// 试的是**使用者自己加的那条线**的地址，不是 cc-switch 那几个公共网站：面板的联网清单里没有它们
/// （DISCLAIMER 的联网一节），为了测代理去连一个使用者没填过的网站，是在替他发一个他没要的请求。
/// 只在点「测试」时跑。
pub async fn test_outbound_proxy(url: &str, target: &str) -> Result<u64> {
    let client = relay_client(&RouterNetwork {
        outbound_proxy: Some(url.to_string()),
        ..Default::default()
    })?
    .ok_or_else(|| GateError::Other("出站代理是空的".into()))?;
    let started = std::time::Instant::now();
    client
        .get(target)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| {
            GateError::Other(format!(
                "经 {} 连不上 {}：{}",
                qb_station::router::mask_proxy_url(url),
                host_of(target),
                qb_proxy::proxy_error::describe_error_chain(&e.without_url())
            ))
        })?;
    Ok(started.elapsed().as_millis() as u64)
}

/// 地址里只留主机（报错时用：别把路径里可能带着的东西印出来）。
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
        .to_string()
}

/// 把攒下的请求日志落库：先补两种花费，再写，再裁掉 [`ROUTER_REQUEST_KEEP_DAYS`] 天以前的。
///
/// 线路的价从库里的 `station_routes` 现读（这一批里用到哪条读哪条，表很小）。
pub fn persist_router_requests(
    db: &Repository,
    mut logs: Vec<RouterRequest>,
    catalog: &pricing::Catalog,
    now_ms: i64,
) -> Result<()> {
    if logs.is_empty() {
        return Ok(());
    }
    let routes: Vec<Route> = db.list("station_routes")?;
    price_router_requests(&mut logs, &routes, catalog);
    db.insert_router_requests(&logs)?;
    db.prune_router_requests(now_ms - ROUTER_REQUEST_KEEP_DAYS * 24 * 3600 * 1000)?;
    Ok(())
}

/// 最近的请求日志，新的在前。可按软件 / 线路筛。
pub fn router_requests(
    db: &Repository,
    client: Option<Client>,
    route_id: Option<String>,
    limit: u32,
) -> Result<Vec<RouterRequest>> {
    let client = client.map(schedule_key).transpose()?;
    db.router_requests(&RouterRequestQuery {
        since_ms: None,
        client,
        route_id,
        limit,
    })
}

/// 给每条日志补上两种花费（美元）。**取不到就是 `None`，不拿 0 顶上。**
///
/// - 站内：这条线自己公布的四类单价（[`pricing::StationRates::category_prices`]：sub2api 的绝对单价，
///   或 New API 的倍率 × 它源码里写死的 2 美元 / 百万 token）。按次计价、不知道价、或者有 token
///   的那一类没有价 → `None`。
/// - 官方价折算：同样的 token 走官方 API 要多少（[`pricing::Catalog`]，按客户端请求里写的模型名查，
///   查不到再试上游模型名）。只认美元价，不做汇率换算。**不是账单** —— 订阅不按这个收费。
///
/// Codex（OpenAI 协议）报上来的输入 token 含缓存读写，按 cc-switch 的口径先扣掉再按输入价算
/// （[`qb_proxy::usage::calculator::CostCalculator::calculate_for_app`]）。
pub fn price_router_requests(
    logs: &mut [RouterRequest],
    routes: &[Route],
    catalog: &pricing::Catalog,
) {
    use qb_proxy::usage::calculator::{CostCalculator, ModelPricing};
    use qb_proxy::usage::parser::TokenUsage;
    let by_id: HashMap<&str, &Route> = routes.iter().map(|r| (r.id.as_str(), r)).collect();
    for log in logs.iter_mut() {
        let (Some(input), Some(output)) = (log.input_tokens, log.output_tokens) else {
            continue;
        };
        let narrow = |v: Option<u64>| u32::try_from(v.unwrap_or(0)).unwrap_or(u32::MAX);
        let usage = TokenUsage {
            input_tokens: u32::try_from(input).unwrap_or(u32::MAX),
            output_tokens: u32::try_from(output).unwrap_or(u32::MAX),
            cache_read_tokens: narrow(log.cache_read_tokens),
            cache_creation_tokens: narrow(log.cache_write_tokens),
            cache_creation_1h_tokens: narrow(log.cache_write_1h_tokens),
            ..Default::default()
        };
        // `calculate_for_app` 认的写法：Codex 的输入含缓存，Claude 的不含。
        let app = if log.client == "codex" {
            "codex"
        } else {
            "claude"
        };

        log.station_cost = by_id.get(log.route_id.as_str()).and_then(|route| {
            let [p_in, p_read, p_write, p_out] = route.rates.category_prices();
            let need = |price: Option<f64>, tokens: u32| match price {
                Some(p) => Some(p),
                None if tokens == 0 => Some(0.0),
                None => None,
            };
            let pricing = ModelPricing::new(
                need(p_in, usage.input_tokens)?,
                need(p_out, usage.output_tokens)?,
                need(p_read, usage.cache_read_tokens)?,
                need(p_write, usage.cache_creation_tokens)?,
            );
            Some(CostCalculator::calculate_for_app(app, &usage, &pricing).total_cost)
        });

        log.official_cost = [log.model.as_str(), log.upstream_model.as_str()]
            .into_iter()
            .filter(|m| !m.is_empty())
            .find_map(|m| catalog.resolve(m))
            .filter(|p| p.currency.eq_ignore_ascii_case("USD"))
            .map(|p| {
                let pricing = ModelPricing::new(
                    p.input.per_mtok,
                    p.output.per_mtok,
                    p.cache_read.per_mtok,
                    p.cache_write.per_mtok,
                );
                CostCalculator::calculate_for_app(app, &usage, &pricing).total_cost
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qb_station::schedule::Axis;

    fn route(id: &str, rate: f64) -> Route {
        Route {
            nominal_rate: Some(rate),
            ..Route::new(qb_contract::domain::Client::ClaudeCode, id, "")
        }
    }

    fn health_with(cost: f64, tokens: u64, ttft: u64, ok: u64, bad: u64) -> RouteHealth {
        let now = 1_700_000_000_000i64;
        let mut rows = Vec::new();
        for i in 0..ok {
            rows.push(UsageRow {
                at_ms: now - 1000 - i as i64,
                input_uncached: Some(tokens),
                cache_read: Some(0),
                cache_write: Some(0),
                output: Some(0),
                first_token_ms: Some(ttft),
                cost: Some(cost / (ok + bad) as f64),
                ok: true,
                status_reported: true,
                ..Default::default()
            });
        }
        for i in 0..bad {
            rows.push(UsageRow {
                at_ms: now - 2000 - i as i64,
                ok: false,
                status_reported: true,
                ..Default::default()
            });
        }
        RouteHealth {
            windows: health::windows(&rows, now),
            error: None,
        }
    }

    fn priced_log(client: &str, route: &str, model: &str) -> RouterRequest {
        RouterRequest {
            id: format!("{route}-{model}"),
            at_ms: 1_700_000_000_000,
            client: client.into(),
            route_id: route.into(),
            model: model.into(),
            input_tokens: Some(1_000_000),
            output_tokens: Some(100_000),
            cache_read_tokens: Some(0),
            cache_write_tokens: Some(0),
            cache_write_1h_tokens: Some(0),
            ..Default::default()
        }
    }

    fn usd(model: &str, input: f64, output: f64) -> FetchedPrice {
        FetchedPrice {
            model: model.into(),
            input_per_mtok: input,
            output_per_mtok: output,
            cache_read_per_mtok: Some(input / 10.0),
            cache_write_per_mtok: Some(input * 1.25),
            cache_write_1h_per_mtok: None,
            long_context: None,
            currency: "USD".into(),
            source_url: "https://example.test/pricing".into(),
            fetched_at: "2026-10-24".into(),
        }
    }

    #[test]
    fn both_costs_are_filled_from_the_line_s_own_price_and_the_official_one() {
        let mut sub2 = route("claude-code\u{1f}s\u{1f}", 1.0);
        sub2.rates.input_price = Some(3.0);
        sub2.rates.output_price = Some(15.0);
        sub2.rates.cache_read_price = Some(0.3);
        sub2.rates.cache_write_price = Some(3.75);
        let mut logs = vec![priced_log("claude-code", &sub2.id, "claude-opus-5")];
        let catalog = pricing::Catalog::new(vec![usd("claude-opus-5", 5.0, 25.0)]);
        price_router_requests(&mut logs, &[sub2], &catalog);
        // 1M 输入 × 3 + 0.1M 输出 × 15 = 4.5（站内）；按官方 5 / 25 = 7.5。
        assert!((logs[0].station_cost.unwrap() - 4.5).abs() < 1e-9);
        assert!((logs[0].official_cost.unwrap() - 7.5).abs() < 1e-9);
    }

    #[test]
    fn a_cost_nobody_can_price_is_none_not_zero() {
        // 线路没填价（按次 / 没检验过）、模型官方表里没有、日志里没 token —— 一律 None。
        let unknown = route("claude-code\u{1f}u\u{1f}", 1.0);
        let mut logs = vec![
            priced_log("claude-code", &unknown.id, "relay-only-model"),
            RouterRequest {
                route_id: unknown.id.clone(),
                model: "claude-opus-5".into(),
                ..Default::default()
            },
        ];
        let catalog = pricing::Catalog::new(vec![usd("claude-opus-5", 5.0, 25.0)]);
        price_router_requests(&mut logs, &[unknown], &catalog);
        assert_eq!(logs[0].station_cost, None, "线路没价却算出了站内花费");
        assert_eq!(
            logs[0].official_cost, None,
            "认不出的模型拿别的价凑了一个数"
        );
        assert_eq!(
            (logs[1].station_cost, logs[1].official_cost),
            (None, None),
            "没 token 的那一发不是 0 元，是没记"
        );
    }

    #[test]
    fn codex_input_tokens_include_the_cache_and_are_not_charged_twice() {
        // OpenAI 协议的 input_tokens 含缓存读：不扣的话，缓存读那部分按输入价又收一遍。
        let mut log = priced_log("codex", "codex\u{1f}x\u{1f}", "gpt-5.6-sol");
        log.cache_read_tokens = Some(600_000);
        log.output_tokens = Some(0);
        let mut logs = vec![log];
        let catalog = pricing::Catalog::new(vec![usd("gpt-5.6-sol", 4.0, 20.0)]);
        price_router_requests(&mut logs, &[], &catalog);
        // 0.4M 新输入 × 4 + 0.6M 缓存读 × 0.4 = 1.6 + 0.24。
        assert!((logs[0].official_cost.unwrap() - 1.84).abs() < 1e-9);
    }

    #[test]
    fn persisted_requests_read_back_newest_first_and_old_ones_are_pruned() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        let day = 24 * 3600 * 1000;
        let now = 1_700_000_000_000i64;
        let mut fresh = priced_log("codex", "codex\u{1f}a\u{1f}", "gpt");
        fresh.id = "fresh".into();
        fresh.at_ms = now - day;
        let mut stale = fresh.clone();
        stale.id = "stale".into();
        stale.at_ms = now - (ROUTER_REQUEST_KEEP_DAYS + 1) * day;
        let mut other = fresh.clone();
        other.id = "other".into();
        other.client = "claude-code".into();
        other.at_ms = now - 2 * day;
        persist_router_requests(
            &db,
            vec![stale, fresh.clone(), other],
            &pricing::Catalog::new(vec![]),
            now,
        )
        .unwrap();
        let all = router_requests(&db, None, None, 10).unwrap();
        assert_eq!(
            all.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["fresh", "other"],
            "{ROUTER_REQUEST_KEEP_DAYS} 天以前的没裁掉，或者顺序不对"
        );
        let codex = router_requests(&db, Some(Client::Codex), None, 10).unwrap();
        assert_eq!(codex, vec![fresh], "按软件筛不对，或者读回来变了样");
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn router_settings_and_the_dispatch_queue_survive_a_panel_restart() {
        // 原来当前上游只在内存里：面板一重启，客户端下一发收到 503「还没选上游」。
        let root = std::env::temp_dir().join(crate::config_io::id());
        let a = Route::make_id(Client::Codex, "s", "a");
        let b = Route::make_id(Client::Codex, "s", "b");
        {
            let db = Repository::open_in(&root).unwrap();
            let mut s = load_router_settings(&db, Client::Codex).unwrap();
            assert_eq!(
                s,
                RouterSettings::for_client(Client::Codex),
                "没存过 = 默认那份"
            );
            s.auto_failover = false;
            s.max_attempts = 2;
            save_router_settings(&db, &s).unwrap();
            save_dispatch_order(&db, Client::Codex, vec![b.clone(), a.clone(), b.clone()], 7)
                .unwrap();
            remember_current(&db, Client::Codex, &a, 8).unwrap();
        }
        let db = Repository::open_in(&root).unwrap();
        let mut state = RouterState::default();
        restore_router(&db, &mut state).unwrap();
        assert!(!state.settings(Client::Codex).auto_failover);
        assert_eq!(state.settings(Client::Codex).max_attempts, 2);
        assert_eq!(state.current(Client::Codex).as_deref(), Some(a.as_str()));
        let q = load_dispatch_queue(&db, Client::Codex).unwrap();
        assert_eq!(q.order, [b.clone(), a.clone()], "重复的那条没去掉");
        // 别的软件不受影响。
        assert_eq!(state.current(Client::ClaudeCode), None);
        assert!(state.settings(Client::ClaudeCode).auto_failover);

        // 面板开着期间刚选过的，不被库里那份旧的盖掉。
        let mut busy = RouterState::default();
        busy.set_now(Client::Codex, b.clone());
        restore_router(&db, &mut busy).unwrap();
        assert_eq!(busy.current(Client::Codex).as_deref(), Some(b.as_str()));
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn probe(at_ms: i64, ok: bool, fault: Option<&str>, total_ms: u64) -> ProbeAttempt {
        ProbeAttempt {
            at_ms,
            ok,
            fault: fault.map(str::to_string),
            total_ms: Some(total_ms),
            paid: true,
            model: "m".into(),
            ..Default::default()
        }
    }

    #[test]
    fn probe_speed_is_used_only_when_paid_probing_covers_the_whole_pool() {
        let now = 1_700_000_000_000i64;
        let routes = [route("a", 1.0), route("b", 1.0)];
        let mut ev = Evidence {
            paid_probing: true,
            ..Default::default()
        };
        ev.probes.insert(
            routes[0].id.clone(),
            vec![probe(now - 1_000, true, None, 800)],
        );
        assert!(
            probe_speeds(&routes, &ev, now).is_none(),
            "b 还没测过：整池退回体验分"
        );
        ev.probes.insert(
            routes[1].id.clone(),
            vec![
                probe(now - 1_000, true, None, 400),
                probe(now - 2_000, true, None, 600),
            ],
        );
        let s = probe_speeds(&routes, &ev, now).unwrap();
        assert_eq!(s[&routes[1].id], (500.0, 2));
        ev.paid_probing = false;
        assert!(probe_speeds(&routes, &ev, now).is_none(), "关着就不用");

        // 用上测活速度时，「快」按它排：b 比 a 快。
        ev.paid_probing = true;
        let round = plan_round(
            &routes,
            &HashMap::new(),
            &HashMap::new(),
            &Schedule {
                prefs: Prefs {
                    axes: vec![Axis::Fast],
                    ..Default::default()
                },
                ..Schedule::new(Client::ClaudeCode)
            },
            &DispatchQueue::new(Client::ClaudeCode),
            None,
            &pricing::Catalog::new(vec![]),
            &HashMap::new(),
            &ev,
            now,
        );
        assert!(round.probe_speed);
        assert_eq!(round.dispatch.order.first(), Some(&routes[1].id));
        assert_eq!(
            next_upstream(&round.dispatch, None),
            Some(routes[1].id.clone())
        );
    }

    #[test]
    fn a_probe_failure_is_overruled_by_a_real_success_afterwards() {
        let now = 1_700_000_000_000i64;
        let mut r = route("a", 1.0);
        r.model_map.default = Some("relay-x".into());
        let routes = [r];
        let mut ev = Evidence::default();
        let mut missing = probe(now - 90_000, false, Some("model-missing"), 50);
        missing.model = "claude-opus-5".into();
        ev.probes.insert(
            routes[0].id.clone(),
            vec![probe(now - 60_000, false, Some("auth"), 30), missing],
        );
        let facts = line_facts(&routes, &HashMap::new(), &ev, None, now);
        let f = &facts[&routes[0].id];
        assert!(
            f.probe_down.as_deref().unwrap_or("").contains("凭据"),
            "{f:?}"
        );
        assert_eq!(f.mapping_missing, ["claude-opus-5 → relay-x"]);
        assert_eq!(f.evidence_at, Some(now - 60_000));

        // 判死之后真实流量在它上面成功了：活着，不进故障档。
        ev.usage.insert(
            routes[0].id.clone(),
            vec![UsageRow {
                at_ms: now - 10_000,
                ok: true,
                ..Default::default()
            }],
        );
        let facts = line_facts(&routes, &HashMap::new(), &ev, None, now);
        assert_eq!(facts[&routes[0].id].probe_down, None);
        assert_eq!(facts[&routes[0].id].evidence_at, Some(now - 10_000));
    }

    #[test]
    fn a_line_known_only_from_the_bill_dates_its_evidence_by_the_fetch() {
        let routes = [route("a", 1.0)];
        let ev = Evidence {
            billing_at: Some(42),
            ..Default::default()
        };
        let mut h = HashMap::new();
        h.insert(routes[0].id.clone(), health_with(1.0, 1000, 500, 4, 0));
        let facts = line_facts(&routes, &h, &ev, None, 1_700_000_000_000);
        assert_eq!(facts[&routes[0].id].evidence_at, Some(42));
        assert_eq!(facts[&routes[0].id].samples, 4);
        // 账单里一条都没有：不算有证据。
        let facts = line_facts(&routes, &HashMap::new(), &ev, None, 1_700_000_000_000);
        assert_eq!(facts[&routes[0].id].evidence_at, None);
    }

    #[test]
    fn a_pin_belongs_to_one_client_expires_and_resets_the_debounce_memory() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        let mine = Route::make_id(Client::Codex, "s", "a");
        let theirs = Route::make_id(Client::ClaudeCode, "s", "a");
        assert!(set_pin(&db, Client::Codex, Some(&theirs), 0).is_err());
        let d = Dispatch {
            order: vec![mine.clone()],
            rows: Vec::new(),
            memory: qb_station::schedule::DispatchMemory {
                challenger: Some("x".into()),
                streak: 1,
                signature: None,
            },
            alerts: vec!["P1 告警".into()],
            note: String::new(),
            switched: false,
        };
        save_dispatch(&db, Client::Codex, &d, 1).unwrap();
        let q = set_pin(&db, Client::Codex, Some(&mine), 1_000).unwrap();
        assert_eq!(q.pin.as_ref().unwrap().until_ms, 1_000 + PIN_MS);
        assert_eq!(q.memory, Default::default(), "换了钉住，防抖记忆作废");
        assert_eq!(q.alerts, ["P1 告警"]);
        // 还没到期：存这一轮不动它；过期了：顺手清掉。
        assert!(save_dispatch(&db, Client::Codex, &d, 2_000)
            .unwrap()
            .pin
            .is_some());
        assert!(save_dispatch(&db, Client::Codex, &d, 1_000 + PIN_MS)
            .unwrap()
            .pin
            .is_none());
        assert!(set_pin(&db, Client::Codex, Some(&mine), 0)
            .unwrap()
            .pin
            .is_some());
        assert!(set_pin(&db, Client::Codex, None, 0).unwrap().pin.is_none());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_round_logs_a_switch_and_a_reorder_but_not_the_first_ordering() {
        let d = |order: &[&str]| Dispatch {
            order: order.iter().map(|s| s.to_string()).collect(),
            rows: Vec::new(),
            memory: Default::default(),
            alerts: Vec::new(),
            note: "换了".into(),
            switched: false,
        };
        let kinds = |prev: &[&str], inc: Option<&str>, now: &[&str]| -> Vec<String> {
            let prev: Vec<String> = prev.iter().map(|s| s.to_string()).collect();
            round_actions(Client::Codex, &prev, inc, &d(now), 0)
                .unwrap()
                .into_iter()
                .map(|a| a.kind)
                .collect()
        };
        assert!(
            kinds(&[], Some("a"), &["a", "b", "c"]).is_empty(),
            "第一次排不算改顺序"
        );
        assert!(kinds(&["a", "b", "c"], Some("a"), &["a", "b", "c"]).is_empty());
        assert_eq!(
            kinds(&["a", "b", "c"], Some("a"), &["a", "c", "b"]),
            ["reorder"]
        );
        // 原来的第一条退到热备第一位、其余顺序不变：只是换了第一条。
        assert_eq!(
            kinds(&["a", "b", "c"], Some("a"), &["b", "a", "c"]),
            ["switch"]
        );
        assert_eq!(
            kinds(&["a", "b", "c"], Some("a"), &["b", "c", "a"]),
            ["switch", "reorder"]
        );
        assert_eq!(
            kinds(&["a", "b"], Some("a"), &["b", "a"]),
            ["switch"],
            "只是第一条换了"
        );
    }

    #[test]
    fn scheduler_actions_are_capped_newest_first_and_filtered_by_client() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        let mk = |i: i64, client: &str| SchedulerAction {
            id: format!("{client}-{i}"),
            at_ms: i,
            client: client.into(),
            kind: "switch".into(),
            ..Default::default()
        };
        let batch: Vec<SchedulerAction> = (0..SCHEDULER_ACTIONS_KEEP as i64 + 5)
            .map(|i| mk(i, if i % 2 == 0 { "codex" } else { "claude-code" }))
            .collect();
        for chunk in batch.chunks(50) {
            record_actions(&db, chunk).unwrap();
        }
        let all = scheduler_actions(&db, None, usize::MAX).unwrap();
        assert_eq!(all.len(), SCHEDULER_ACTIONS_KEEP);
        assert_eq!(all[0].at_ms, SCHEDULER_ACTIONS_KEEP as i64 + 4, "新的在前");
        let codex = scheduler_actions(&db, Some(Client::Codex), 3).unwrap();
        assert_eq!(codex.len(), 3);
        assert!(codex.iter().all(|a| a.client == "codex"));
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_route_label_is_the_station_name_and_the_group() {
        let names = HashMap::from([("s".to_string(), "某站".to_string())]);
        let mut r = Route::new(Client::Codex, "s", "vip");
        assert_eq!(route_label(&r, &names), "某站 · vip");
        r.group.clear();
        assert_eq!(route_label(&r, &names), "某站");
        r.station_id = "gone".into();
        assert_eq!(route_label(&r, &names), "gone");
    }

    fn tried(
        route: &str,
        status: Option<u16>,
        fault: Option<&str>,
    ) -> qb_station::router::RouteAttempt {
        qb_station::router::RouteAttempt {
            route_id: route.into(),
            status,
            fault: fault.map(str::to_string),
            ms: 100,
        }
    }

    #[test]
    fn every_line_a_request_tried_gets_its_own_evidence_row() {
        let a = "claude-code\u{1f}s\u{1f}a";
        let b = "claude-code\u{1f}s\u{1f}b";
        let at = 1_700_000_000_000;
        let reqs = vec![
            // a 502 → 换到 b 成功：a 记一次失败，b 记一次成功（token、首字、花费都在 b 身上）。
            RouterRequest {
                id: "1".into(),
                at_ms: at,
                client: "claude-code".into(),
                route_id: b.into(),
                status: Some(200),
                first_token_ms: Some(800),
                input_tokens: Some(10),
                output_tokens: Some(5),
                cache_read_tokens: Some(100),
                cache_write_tokens: Some(0),
                station_cost: Some(0.01),
                attempts: vec![
                    tried(a, Some(502), Some("upstream-down")),
                    tried(b, Some(200), None),
                ],
                ..Default::default()
            },
            // 400：请求自己的错，不记（熔断那边也是中性）。
            RouterRequest {
                id: "2".into(),
                at_ms: at,
                client: "claude-code".into(),
                route_id: a.into(),
                status: Some(400),
                fault: Some("client-request".into()),
                attempts: vec![tried(a, Some(400), Some("client-request"))],
                ..Default::default()
            },
            // 整流前那一次不记，整流后成功的那次记。
            RouterRequest {
                id: "3".into(),
                at_ms: at,
                client: "claude-code".into(),
                route_id: a.into(),
                status: Some(200),
                attempts: vec![
                    tried(a, Some(400), Some("rectify-thinking-signature")),
                    tried(a, Some(200), None),
                ],
                ..Default::default()
            },
            // 200 但流半路断了：最后那条按日志的结局记失败。
            RouterRequest {
                id: "4".into(),
                at_ms: at,
                client: "claude-code".into(),
                route_id: b.into(),
                status: Some(200),
                fault: Some("timeout".into()),
                attempts: vec![tried(b, Some(200), None)],
                ..Default::default()
            },
        ];
        let rows = router_usage_rows(&reqs);
        let oks = |id: &str| -> Vec<bool> { rows[id].iter().map(|r| r.ok).collect() };
        assert_eq!(
            oks(a),
            [false, true],
            "a：换走的那次算失败、400 不记、整流前那次不记"
        );
        assert_eq!(oks(b), [true, false]);
        assert!(
            rows[a].iter().all(|r| r.status_reported),
            "本机路由的成败是报了的"
        );
        let b_ok = &rows[b][0];
        assert_eq!(
            (
                b_ok.first_token_ms,
                b_ok.input_uncached,
                b_ok.cache_read,
                b_ok.cost
            ),
            (Some(800), Some(10), Some(100), Some(0.01))
        );
        assert_eq!(rows[a][0].output, None, "换走的那一次不该带上别人的 token");

        // Codex（OpenAI 协议）的输入含缓存：没缓存的那部分要先扣掉。
        let codex = RouterRequest {
            id: "5".into(),
            client: "codex".into(),
            route_id: "codex\u{1f}s\u{1f}".into(),
            status: Some(200),
            input_tokens: Some(1000),
            cache_read_tokens: Some(600),
            cache_write_tokens: Some(0),
            attempts: vec![tried("codex\u{1f}s\u{1f}", Some(200), None)],
            ..Default::default()
        };
        let rows = router_usage_rows(&[codex]);
        assert_eq!(rows["codex\u{1f}s\u{1f}"][0].input_uncached, Some(400));
    }

    #[test]
    fn ranking_evidence_comes_from_the_router_s_own_log_where_there_is_one() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        let now = 1_700_000_000_000i64;
        let a = route("claude-code\u{1f}s\u{1f}a", 1.0);
        let b = route("claude-code\u{1f}s\u{1f}b", 1.0);
        let reqs: Vec<RouterRequest> = (0..4)
            .map(|i| RouterRequest {
                id: format!("r{i}"),
                at_ms: now - 60_000,
                client: "claude-code".into(),
                route_id: a.id.clone(),
                status: Some(if i == 0 { 502 } else { 200 }),
                fault: (i == 0).then(|| "upstream-down".to_string()),
                first_token_ms: Some(700),
                attempts: vec![tried(
                    &a.id,
                    Some(if i == 0 { 502 } else { 200 }),
                    (i == 0).then_some("upstream-down"),
                )],
                ..Default::default()
            })
            .collect();
        db.insert_router_requests(&reqs).unwrap();
        let local = local_health(&db, &[a.clone(), b.clone()], now);
        assert!(
            local.contains_key(&a.id),
            "本机路由记过的线没用上它自己的日志"
        );
        assert!(
            !local.contains_key(&b.id),
            "没记过的线不该凭空有证据（它退回站点账单）"
        );
        let day = &local[&a.id].day;
        assert_eq!(day.requests, 4);
        assert_eq!(
            day.success_rate,
            Some(0.75),
            "成功率不是按本机路由的成败算的"
        );
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_router_network_is_saved_only_when_it_is_safe_and_well_formed() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        assert_eq!(load_router_network(&db).unwrap(), RouterNetwork::default());
        // 开到所有网卡上却没口令：不存。
        let open = RouterNetwork {
            scope: ListenScope::AllInterfaces,
            ..Default::default()
        };
        assert!(save_router_network(&db, &open).is_err());
        // 代理地址不合格：不存，报错里不带密码。
        let bad = RouterNetwork {
            outbound_proxy: Some("user:secret@127.0.0.1".into()),
            ..Default::default()
        };
        let err = save_router_network(&db, &bad).unwrap_err().to_string();
        assert!(!err.contains("secret"), "{err}");
        // 合格的那份存了读得回来。
        let key = new_router_key();
        assert!(key.starts_with("qbr-") && key.len() > 60, "{key}");
        assert_ne!(key, new_router_key(), "口令不是随机的");
        let good = RouterNetwork {
            scope: ListenScope::AllInterfaces,
            key,
            outbound_proxy: Some("socks5h://127.0.0.1:1080".into()),
        };
        save_router_network(&db, &good).unwrap();
        assert_eq!(load_router_network(&db).unwrap(), good);
        assert!(relay_client(&good).unwrap().is_some());
        assert!(relay_client(&RouterNetwork::default()).unwrap().is_none());
        // restore 装回去：中转线路走代理。
        let mut state = RouterState::default();
        restore_router(&db, &mut state).unwrap();
        assert!(state.relay_proxied());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn a_dead_outbound_proxy_is_reported_without_its_password_or_the_target_path() {
        let dead = {
            let l = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
            l.local_addr().unwrap().port()
        };
        let err = test_outbound_proxy(
            &format!("http://u:topsecret@127.0.0.1:{dead}"),
            "https://relay.example/v1?token=abc",
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(!err.contains("topsecret"), "{err}");
        assert!(!err.contains("token=abc"), "{err}");
        assert!(err.contains("relay.example"), "{err}");
        assert_eq!(host_of("https://a:b@h.example:8443/x?y"), "h.example:8443");
    }

    #[test]
    fn bad_router_settings_and_foreign_lines_in_the_queue_are_refused() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        let mut bad = RouterSettings::for_client(Client::ClaudeCode);
        bad.breaker.error_rate = 0.0;
        assert!(save_router_settings(&db, &bad).is_err());
        assert_eq!(
            load_router_settings(&db, Client::ClaudeCode).unwrap(),
            RouterSettings::for_client(Client::ClaudeCode),
            "不合格的那份存进去了"
        );
        // Codex 的线混进 Claude Code 的派单顺序：换过去就是拿 Claude 的请求打 Codex 的 Key。
        let foreign = Route::make_id(Client::Codex, "s", "");
        let err = save_dispatch_order(&db, Client::ClaudeCode, vec![foreign], 1).unwrap_err();
        assert!(err.to_string().contains("别的软件"), "{err}");
        assert!(load_dispatch_queue(&db, Client::ClaudeCode)
            .unwrap()
            .order
            .is_empty());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn aggregate_keys_are_handed_out_by_the_backend_and_never_move() {
        // key 换了指向，客户端带着的旧 id 会被悄悄发到另一条线 —— 所以界面传来的 keys 一概不认。
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        for (id, name, url) in [
            ("kimi", "Kimi", "https://api.kimi.com/v1"),
            ("zp", "智谱", "https://open.bigmodel.cn/api/anthropic"),
        ] {
            let provider = Provider {
                id: id.into(),
                name: name.into(),
                base_url: url.into(),
                website: String::new(),
                note: String::new(),
                tags: vec![],
                favorite: false,
                revision: 1,
                topup_per_usd: None,
            };
            db.put("providers", id, &provider).unwrap();
            let r = Route::new(Client::ClaudeCode, id, "默认");
            db.put("station_routes", &r.id, &r).unwrap();
        }
        let kimi = Route::make_id(Client::ClaudeCode, "kimi", "默认");
        let zp = Route::make_id(Client::ClaudeCode, "zp", "默认");

        let mut s = RouterSettings::for_client(Client::ClaudeCode);
        s.aggregate.enabled = true;
        s.aggregate.members = vec![kimi.clone(), " ".into(), zp.clone(), kimi.clone()];
        s.aggregate.keys.insert("forged".into(), zp.clone());
        s.aggregate.models.insert(
            zp.clone(),
            vec![" glm-5[1M] ".into(), "glm-5[1M]".into(), "".into()],
        );
        s.aggregate
            .models
            .insert("not-a-member".into(), vec!["x".into()]);
        let saved = save_router_settings(&db, &s).unwrap();
        assert_eq!(
            saved.aggregate.members,
            [kimi.clone(), zp.clone()],
            "去空、去重"
        );
        assert_eq!(saved.aggregate.keys.get("kimi"), Some(&kimi), "按站点名分");
        assert_eq!(
            saved.aggregate.keys.get("open-bigmodel"),
            Some(&zp),
            "中文名转不出就用域名"
        );
        assert!(
            !saved.aggregate.keys.contains_key("forged"),
            "界面传来的 key 进了登记簿"
        );
        assert_eq!(saved.aggregate.models.len(), 1, "名单外的线的模型列表留着");
        assert_eq!(saved.aggregate.models[&zp], ["glm-5[1M]"]);
        assert_eq!(
            load_router_settings(&db, Client::ClaudeCode).unwrap(),
            saved
        );

        // 移出名单再加回来：还是原来那个 key（旧会话里的 id 继续有效）；移出期间 key 也不让给别人。
        let mut out = saved.clone();
        out.aggregate.members = vec![zp.clone()];
        out.aggregate.keys.clear();
        let out = save_router_settings(&db, &out).unwrap();
        assert_eq!(
            out.aggregate.keys.get("kimi"),
            Some(&kimi),
            "移出名单就删了登记"
        );
        let mut back = out.clone();
        back.aggregate.members.push(kimi.clone());
        let back = save_router_settings(&db, &back).unwrap();
        assert_eq!(back.aggregate.keys, saved.aggregate.keys);

        // 别的软件的线、官方 Codex 那条：整份拒绝。
        for foreign in [
            Route::make_id(Client::Codex, "kimi", "默认"),
            crate::local_router::OFFICIAL_CODEX_ROUTE_ID.to_string(),
        ] {
            let mut bad = back.clone();
            bad.aggregate.members.push(foreign.clone());
            let err = save_router_settings(&db, &bad).unwrap_err();
            assert!(err.to_string().contains("别的软件"), "{foreign}: {err}");
        }
        assert_eq!(load_router_settings(&db, Client::ClaudeCode).unwrap(), back);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn host_hints_keep_only_the_distinctive_part_of_a_station_address() {
        assert_eq!(host_hint("https://api.kimi.com/v1"), "kimi");
        assert_eq!(host_hint("https://www.relay.example/v1"), "relay");
        assert_eq!(host_hint("http://127.0.0.1:3000"), "127.0.0");
        assert_eq!(host_hint("relay.test"), "relay");
    }

    #[test]
    fn a_station_s_alternate_addresses_are_stored_ordered_and_promoted() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        let provider = Provider {
            id: "site".into(),
            name: "Fixture".into(),
            base_url: "https://main.test".into(),
            website: String::new(),
            note: String::new(),
            tags: vec![],
            favorite: false,
            revision: 1,
            topup_per_usd: None,
        };
        db.put("providers", "site", &provider).unwrap();

        add_endpoint(&db, "site", "https://b.test/", 1).unwrap();
        let e = add_endpoint(&db, "site", "https://MAIN.test", 2).unwrap();
        assert_eq!(e.endpoints.len(), 1, "跟主地址重复的不加");
        assert!(
            add_endpoint(&db, "site", "https://u:p@c.test", 3).is_err(),
            "带凭证的地址"
        );
        assert!(
            add_endpoint(&db, "nobody", "https://c.test", 3).is_err(),
            "站点不存在"
        );
        add_endpoint(&db, "site", "https://c.test", 4).unwrap();
        let mut e = load_endpoints(&db, "site").unwrap();
        e.record(
            &qb_station::station::endpoints::EndpointLatency {
                url: "https://c.test".into(),
                latency_ms: Some(30),
                status: Some(404),
                error: None,
            },
            5,
        );
        save_endpoints(&db, &e).unwrap();
        assert_eq!(
            alternates(&db, &provider),
            ["https://c.test", "https://b.test"],
            "测过的快的在前"
        );

        let promoted = promote_endpoint(&db, "site", "https://c.test/", 6).unwrap();
        assert_eq!(promoted.base_url, "https://c.test");
        assert_eq!(promoted.revision, 2);
        let after: Vec<String> = load_endpoints(&db, "site")
            .unwrap()
            .endpoints
            .into_iter()
            .map(|x| x.url)
            .collect();
        assert_eq!(
            after,
            ["https://b.test", "https://main.test"],
            "原来的主地址挪进备用"
        );
        assert!(promote_endpoint(&db, "site", "https://stranger.test", 7).is_err());
        assert!(remove_endpoint(&db, "site", "https://b.test")
            .unwrap()
            .endpoints
            .iter()
            .all(|x| x.url != "https://b.test"));
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_schedule_switched_on_is_still_on_after_the_panel_reopens_the_database() {
        // 「落盘的承诺」：开了存进库，面板重启（重新打开库）读回来还是开着。
        // 迁移 4 之前 save 必报「未知的数据表」、load 把错误吞成「关着」——
        // 智能调度在真机上从没开成过，而演示模式喂的是假数据，看不出来。
        let root = std::env::temp_dir().join(crate::config_io::id());
        {
            let db = Repository::open_in(&root).unwrap();
            // 没存过 = 关着的那一份，不是错误。
            assert_eq!(
                load_schedule(&db, Client::Codex).unwrap(),
                Schedule::new(Client::Codex)
            );
            let mut s = load_schedule(&db, Client::Codex).unwrap();
            s.enabled = true;
            s.prefs = Prefs::batch();
            s.last_switch_ms = Some(1_700_000_000_000);
            s.last_note = "现任仍然是最好的那条".into();
            save_schedule(&db, &s).unwrap();
        }
        let db = Repository::open_in(&root).unwrap();
        let back = load_schedule(&db, Client::Codex).unwrap();
        assert!(back.enabled);
        assert_eq!(back.prefs, Prefs::batch());
        assert_eq!(back.last_switch_ms, Some(1_700_000_000_000));
        assert_eq!(back.last_note, "现任仍然是最好的那条");
        // 一个软件一份：开了 Codex 不会把另外两个一起打开。
        assert!(!load_schedule(&db, Client::ClaudeCode).unwrap().enabled);
        assert!(!load_schedule(&db, Client::ClaudeDesktop).unwrap().enabled);
        // 主键就是 IPC 上的软件名（`Client` 的 serde 写法）。
        let raw: serde_json::Value = db.get("station_schedule", "codex").unwrap();
        assert_eq!(raw["enabled"], true);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unreadable_schedule_is_reported_instead_of_read_as_off() {
        // 「读不出来」不许降级成「关着」（§7.17）：那正是调度表的 bug 藏了这么久的原因。
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        db.put(
            "station_schedule",
            "claude-code",
            &serde_json::json!({"enabled": true}),
        )
        .unwrap();
        assert!(load_schedule(&db, Client::ClaudeCode).is_err());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn route_source_uses_its_bound_database_key_and_rejects_mismatches() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        let provider = Provider {
            id: "site".into(),
            name: "Fixture".into(),
            base_url: "https://fixture.invalid/v1/responses".into(),
            website: String::new(),
            note: String::new(),
            tags: vec![],
            favorite: false,
            revision: 1,
            topup_per_usd: None,
        };
        db.put("providers", "site", &provider).unwrap();
        let mut route = route("site", 1.0);
        route.station_id = "site".into();
        assert!(source(&db, &route)
            .unwrap_err()
            .to_string()
            .contains("API Key"));
        for id in ["first", "second"] {
            let credential = Credential {
                id: id.into(),
                provider_id: "site".into(),
                label: id.into(),
                masked: "***".into(),
                available: true,
                revision: 1,
            };
            db.set_credential(
                &credential,
                &crate::secret::seal(&format!("fixture-{id}")).unwrap(),
            )
            .unwrap();
        }
        route.credential_id = Some("second".into());
        let resolved = source(&db, &route).unwrap();
        assert_eq!(resolved.key, "fixture-second");
        assert_eq!(resolved.base_url, "https://fixture.invalid/v1");
        let mut other = provider.clone();
        other.id = "other".into();
        db.put("providers", "other", &other).unwrap();
        route.station_id = "other".into();
        assert!(source(&db, &route)
            .unwrap_err()
            .to_string()
            .contains("不属于"));
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_real_bill_beats_the_advertised_multiplier_when_every_route_has_one() {
        // §4.3 那条 ⚠ 的落地:×0.20 但缓存全废,比 ×0.35 有命中的贵。
        let routes = [route("贵但缓存好", 0.35), route("便宜但缓存废", 0.20)];
        let mut h = HashMap::new();
        // 同样 1000 token,前者只扣了 1.0,后者扣了 3.0。
        h.insert(routes[0].id.clone(), health_with(1.0, 1000, 500, 4, 0));
        h.insert(routes[1].id.clone(), health_with(3.0, 1000, 500, 4, 0));

        let (ranking, basis) = decide(
            &routes,
            &h,
            &HashMap::new(),
            &Prefs {
                axes: vec![Axis::Cheap],
                ..Default::default()
            },
            &pricing::Catalog::new(Vec::new()),
            &HashMap::new(),
            None,
        );
        assert_eq!(basis, CheapBasis::CostPerToken);
        assert_eq!(
            ranking.winner.as_deref(),
            Some(routes[0].id.as_str()),
            "按标称倍率会选错那一条"
        );
    }

    /// 帖子里的「倍率陷阱」:7 元买 1 美元额度、标 ×0.1 的站,折成每 $1 牌价是 0.7 元,
    /// 比 1 元买 1 美元额度、标 ×0.5 的站(0.5 元)贵。不折算就会选错。
    #[test]
    fn a_low_multiplier_sold_at_seven_yuan_per_dollar_loses_to_a_higher_one_at_one() {
        let routes = [route("七比一", 0.1), route("一比一", 0.5)];
        let prefs = Prefs {
            axes: vec![Axis::Cheap],
            ..Default::default()
        };
        let catalog = pricing::Catalog::new(Vec::new());
        // 不知道充值比例时(都按 1 算)标 ×0.1 那家赢 —— 这正是陷阱。
        let (naive, _) = decide(
            &routes,
            &HashMap::new(),
            &HashMap::new(),
            &prefs,
            &catalog,
            &HashMap::new(),
            None,
        );
        assert_eq!(naive.winner.as_deref(), Some(routes[0].id.as_str()));

        let topup = HashMap::from([(routes[0].station_id.clone(), 7.0)]);
        let (ranking, basis) = decide(
            &routes,
            &HashMap::new(),
            &HashMap::new(),
            &prefs,
            &catalog,
            &topup,
            None,
        );
        assert_eq!(basis, CheapBasis::RealRate);
        assert_eq!(ranking.winner.as_deref(), Some(routes[1].id.as_str()));

        // 底线「倍率不高于 0.6」也按折过的比:0.7 过线,0.5 没过。
        let strict = Prefs {
            floors: qb_station::schedule::Floors {
                max_rate: Some(0.6),
                ..Default::default()
            },
            ..prefs
        };
        let (floored, _) = decide(
            &routes,
            &HashMap::new(),
            &HashMap::new(),
            &strict,
            &catalog,
            &topup,
            None,
        );
        let seven = floored
            .rows
            .iter()
            .find(|r| r.route_id == routes[0].id)
            .unwrap();
        assert!(!seven.eligible, "折成 0.7 元的那家该被底线挡住");
        assert!((seven.failed_floors[0].actual - 0.7).abs() < 1e-12);
    }

    /// New API 的线路按单价 ÷ 官方单价加权,分组折扣要算进去。
    ///
    /// 0.25.3 及以前:加权直接拿原始倍率(Opus 5 照官方价是 2.5 / 12.5)算,而且分组倍率
    /// 从来没读到 —— 两条同样照官方价抄、分组 ×0.2 和 ×0.8 的线,加权出来一模一样。
    #[test]
    fn new_api_routes_are_blended_against_the_official_price_with_their_group_discount() {
        let official = pricing::lookup("claude-opus-5").unwrap();
        let rates = |group: f64| pricing::StationRates {
            model_ratio: Some(2.5),
            completion_ratio: Some(5.0),
            cache_ratio: Some(0.1),
            create_cache_ratio: Some(1.25),
            group_ratio: Some(group),
            ..Default::default()
        };
        let mut cheap = route("二折组", 0.2);
        cheap.rates = rates(0.2);
        cheap.rates_model = Some("claude-opus-5".into());
        let mut dear = route("八折组", 0.8);
        dear.rates = rates(0.8);
        dear.rates_model = Some("claude-opus-5".into());
        let routes = [dear, cheap];
        let mut h = HashMap::new();
        // 两条都拉到了 token 结构,但有一条拉不到实扣 —— 整池退到加权倍率。
        h.insert(routes[0].id.clone(), health_with(1.0, 1000, 500, 4, 0));
        let mut no_cost = health_with(1.0, 1000, 500, 4, 0);
        no_cost.windows.day.cost = None;
        h.insert(routes[1].id.clone(), no_cost);
        let (ranking, basis) = decide(
            &routes,
            &h,
            &HashMap::new(),
            &Prefs {
                axes: vec![Axis::Cheap],
                ..Default::default()
            },
            &pricing::Catalog::new(Vec::new()),
            &HashMap::new(),
            None,
        );
        assert_eq!(basis, CheapBasis::BlendedRatio);
        assert_eq!(ranking.winner.as_deref(), Some(routes[1].id.as_str()));
        // 加权值就是分组倍率本身(照官方价抄的站四类同折),不是 2.5 那一档的数。
        let mix = h[&routes[1].id].windows.day.mix.shares().unwrap();
        let b = routes[1]
            .rates
            .blended_ratio(mix, &official.as_resolved())
            .unwrap();
        assert!((b - 0.2).abs() < 1e-12, "b={b}");
    }

    #[test]
    fn one_route_without_a_bill_drops_the_pool_back_to_advertised_rates() {
        let routes = [route("有账单", 0.9), route("没账单", 0.2)];
        let mut h = HashMap::new();
        h.insert(routes[0].id.clone(), health_with(1.0, 1000, 500, 4, 0));
        // 第二条拉账单失败 —— 空窗口,而且带着原因。
        h.insert(
            routes[1].id.clone(),
            RouteHealth {
                error: Some("拉不到账单：timeout".into()),
                ..Default::default()
            },
        );
        let (ranking, basis) = decide(
            &routes,
            &h,
            &HashMap::new(),
            &Prefs {
                axes: vec![Axis::Cheap],
                ..Default::default()
            },
            &pricing::Catalog::new(Vec::new()),
            &HashMap::new(),
            None,
        );
        assert_eq!(basis, CheapBasis::RealRate);
        assert_eq!(ranking.winner.as_deref(), Some(routes[1].id.as_str()));
    }

    #[test]
    fn a_failed_fetch_is_an_error_not_a_route_with_zero_traffic() {
        // 空窗口和「拉取失败」必须分得开:前者是「这段时间没跑东西」,
        // 后者是「我们不知道」。混成一个,界面会把挂掉的站显示成健康的。
        let h = RouteHealth {
            error: Some("拉不到账单：dns".into()),
            ..Default::default()
        };
        assert!(h.error.is_some());
        assert_eq!(h.windows.day.success_rate, None);
        assert_eq!(h.windows.day.requests, 0);
    }

    #[test]
    fn breaker_state_comes_from_the_router_rather_than_being_recomputed() {
        // 两份熔断状态必然漂移 —— 这里只接收,不自己算。
        let routes = [route("a", 1.0), route("b", 1.0)];
        let mut bs = HashMap::new();
        bs.insert(routes[0].id.clone(), (1.0, true)); // a 熔断中
        let (ranking, _) = decide(
            &routes,
            &HashMap::new(),
            &bs,
            &Prefs {
                axes: vec![Axis::Cheap],
                ..Default::default()
            },
            &pricing::Catalog::new(Vec::new()),
            &HashMap::new(),
            None,
        );
        assert_eq!(ranking.winner.as_deref(), Some(routes[1].id.as_str()));
        let a = ranking
            .rows
            .iter()
            .find(|r| r.route_id == routes[0].id)
            .unwrap();
        assert!(a.tripped && !a.eligible);
    }

    fn row(id: &str, eligible: bool, tripped: bool) -> qb_station::schedule::Row {
        qb_station::schedule::Row {
            route_id: id.into(),
            score: Some(1.0),
            per_axis: Vec::new(),
            weakest: None,
            missing: Vec::new(),
            failed_floors: Vec::new(),
            eligible,
            tripped,
        }
    }

    fn ranking(winner: &str, rows: Vec<qb_station::schedule::Row>) -> Ranking {
        Ranking {
            rows,
            winner: Some(winner.into()),
            switched: false,
            held_by_hysteresis: false,
            indistinguishable: Vec::new(),
            floors_relaxed: false,
            breakers_relaxed: false,
        }
    }

    /// 排一轮派单（一轮确认就换、证据都够）再问 [`next_upstream`]。
    fn next_after(r: &Ranking, current: Option<&str>) -> Option<String> {
        let facts: HashMap<String, LineFacts> = r
            .rows
            .iter()
            .map(|row| {
                (
                    row.route_id.clone(),
                    LineFacts {
                        samples: 10,
                        evidence_at: Some(1),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let params = qb_station::schedule::DispatchParams {
            confirm: 1,
            ..Default::default()
        };
        let d = qb_station::schedule::dispatch(
            r,
            &facts,
            current,
            &[],
            None,
            &Default::default(),
            &params,
            0,
        );
        next_upstream(&d, current)
    }

    fn scored(id: &str, score: Option<f64>, tripped: bool) -> qb_station::schedule::Row {
        qb_station::schedule::Row {
            score,
            ..row(id, !tripped, tripped)
        }
    }

    /// 第一条就是现任时**不换**。
    ///
    /// 换一次的代价不是零：正在飞的请求会落在另一把 key 上、日志断成两段、
    /// 熔断窗口重来。每一跳都"换"到自己身上，等于每分钟制造一次这些代价，
    /// 而排名一动不动。
    #[test]
    fn the_scheduler_does_not_switch_to_the_route_that_is_already_current() {
        let r = ranking(
            "a",
            vec![scored("a", Some(2.0), false), scored("b", Some(1.0), false)],
        );
        assert_eq!(next_after(&r, Some("a")), None);
        assert_eq!(next_after(&r, Some("b")), Some("a".into()));
        assert_eq!(next_after(&r, None), Some("a".into()));
    }

    /// ⛔ 熔断中或没过底线的那条，即使排序把它顶上来也不换。
    ///
    /// 底线全卡光时 rank 会放开底线重排一轮 —— 那是为了不制造死局
    /// （界面上还能看见名次），**不是**为了把不合格的那条送上生产。
    /// 这两件事混在一起的话，一次全站抖动会把所有人换到熔断中的线路上。
    /// 2026-10-24 起这条规矩在派单里（`schedule::dispatch` 的模块头），这里照它问。
    #[test]
    fn a_tripped_or_ineligible_winner_is_never_switched_to() {
        let tripped = ranking(
            "a",
            vec![scored("a", Some(5.0), true), scored("b", Some(1.0), false)],
        );
        assert_eq!(next_after(&tripped, Some("b")), None);
        let mut floored = scored("a", Some(5.0), false);
        floored.eligible = false;
        floored.failed_floors = vec![qb_station::schedule::FloorMiss {
            axis: Axis::Stable,
            actual: 0.5,
            limit: 0.9,
        }];
        let blocked = ranking("a", vec![floored.clone(), scored("b", Some(1.0), false)]);
        assert_eq!(next_after(&blocked, Some("b")), None);
        // 现任自己熔断了、另一条只是没过底线：兜底的也不换上来，留着现任（本机路由会自己跳过熔断着的线）。
        let worse = ranking("a", vec![floored, scored("b", Some(1.0), true)]);
        assert_eq!(next_after(&worse, Some("b")), None);
    }

    /// 排不出分就不换 —— 没勾维度、全没证据都算这一档。
    ///
    /// 还没有当前上游时（2026-10-24 起）挑排第一的那条：没有第一条，客户端每一发都是「还没选上游」。
    #[test]
    fn no_winner_means_the_incumbent_stays() {
        let none = Ranking {
            winner: None,
            ..ranking(
                "a",
                vec![scored("a", None, false), scored("b", None, false)],
            )
        };
        assert_eq!(next_after(&none, Some("b")), None);
        assert_eq!(next_after(&none, None), Some("a".into()));
    }

    /// ⛔ Key 或站点的问题不许被读成「这条协议不支持」。
    ///
    /// Key 打错一个字符时三条协议会一起 401。猜成「都不支持」的话，
    /// 这条线路在界面上会整个消失 —— 而使用者只是少粘了一位。
    #[test]
    fn an_auth_failure_leaves_the_protocol_unknown_rather_than_unsupported() {
        for code in [401, 403, 429, 500, 502, 503, 504] {
            assert_eq!(
                protocol_verdict(code),
                None,
                "{code} 被当成了协议结论，而它说的是 Key 或站点的事"
            );
        }
    }

    /// 400 是**探通了**的证明，不是失败。
    ///
    /// 探测发的是故意不合法的空 body，所以正常反应就是「你这个 body 不对」——
    /// 上游认得这条路径、解析了我们的请求。把 400 读成失败的话，
    /// 每一家站点都会被探成「什么协议都不支持」。
    #[test]
    fn a_bad_request_proves_the_endpoint_is_there() {
        assert_eq!(protocol_verdict(400), Some(true));
        assert_eq!(protocol_verdict(422), Some(true));
        assert_eq!(protocol_verdict(200), Some(true));
    }

    /// 只有上游**明确说这条路径不存在**才算不支持。
    #[test]
    fn only_an_explicit_missing_endpoint_reads_as_unsupported() {
        for code in [404, 405, 501] {
            assert_eq!(protocol_verdict(code), Some(false), "{code}");
        }
    }

    #[test]
    fn the_self_log_path_is_defined_once() {
        // 散在各处拼路径正是 inventory.rs 那条规矩针对的事。
        assert_eq!(SELF_LOG_PATH, "/api/log/self");
    }
}
