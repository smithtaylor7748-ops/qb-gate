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
//! # ⛔ 这里绝不许碰官方账户槽位
//!
//! 中转线路挂了就换中转线路。按额度 / 429 自动切官方槽位是
//! CLAUDE.md 明确不做的「自动换号」。`qb-station` 物理上不认识账户,
//! `architecture.rs` 有测试钉着。

use crate::domain::{Credential, Provider};
use crate::error::{GateError, Result};
use crate::repository::Repository;
use qb_station::health::{self, Windows};
use qb_station::schedule::{
    cheap_values, cost_per_token, rank, Candidate, CheapBasis, CheapInput, Prefs, Ranking,
};
use qb_station::station::audit::{AuditRound, Check, CheckKind};
use qb_station::station::model::Protocols;
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
    let errors: HashMap<_, _> = routes
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
        .collect();
    async move {
        let mut health = refresh_health(&sources, client, now_ms).await;
        health.extend(errors);
        health
    }
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

/// 这一跳该不该换上游,换到哪。
///
/// `None` = 不换。三种情况都归到这里:排不出赢家(没勾维度、全没证据)、
/// 赢家就是现任、赢家不合格。
///
/// # ⛔ 迟滞不在这里
///
/// 「挑战者好得不够多就留现任」由 [`rank`](qb_station::schedule::rank) 做过了 ——
/// 它拿着 `incumbent` 参数。这里再判一次的话就是两道迟滞叠在一起,
/// 结果是永远不换,而且没有任何地方说得清为什么。
///
/// 所以这里只问一句:赢家跟现任是不是同一条。
pub fn next_upstream(ranking: &Ranking, current: Option<&str>) -> Option<String> {
    let winner = ranking.winner.as_deref()?;
    // 赢家必须还在场上 —— 熔断中/没过底线的那几条 rank 不会选,
    // 但放开底线那一轮会。放开是为了不制造死局,不是为了把不合格的送上去。
    let row = ranking.rows.iter().find(|r| r.route_id == winner)?;
    if !row.eligible || row.tripped {
        return None;
    }
    (Some(winner) != current).then(|| winner.to_string())
}

/// 把线路 + 健康度拼成排序用的候选,再排一次。
///
/// `demotion` / `tripped` 由调用方从各自的熔断器里取 —— 熔断器的状态
/// 归本机路由管,这里不复制一份(两份必然漂移)。
pub fn decide(
    routes: &[Route],
    health: &HashMap<String, RouteHealth>,
    breaker_state: &HashMap<String, (f64, bool)>,
    prefs: &Prefs,
    incumbent: Option<&str>,
    // 官方价目。公布绝对单价的站点要靠它才换得出倍率,见下面那段。
    catalog: &pricing::Catalog,
) -> (Ranking, CheapBasis) {
    // 「便宜」整池统一口径：所有线都拿得出 24h 实扣单价才用它,
    // 差一条就整池退回真实倍率。混着比出来的比值没有意义。
    let inputs: Vec<CheapInput> = routes
        .iter()
        .map(|r| {
            let day = health.get(&r.id).map(|h| &h.windows.day);
            // 加权等效倍率:拿这条线实际的 token 结构去加权它公布的四类价。
            // 这是「一家翻倍一家不翻倍怎么比」的答案。
            //
            // 官方价按「这组价是哪个模型的」去查。公布绝对单价的站点
            // (sub2api 系)非有它不可 —— 一个绝对单价换不成倍率,除非
            // 知道官方单价。查不到就退回只认倍率的那条路:那种站点因此
            // 算不出加权倍率,整池退回更粗的口径。**那是对的** ——
            // 拿一个算不出来的数去比,比退回粗口径糟得多。
            let official = r
                .rates_model
                .as_deref()
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .and_then(|m| catalog.resolve(m));
            let blended = day
                .and_then(|d| d.mix.shares())
                .and_then(|mix| match &official {
                    Some(o) => r.rates.blended_ratio_against(mix, o),
                    None => r.rates.blended_ratio(mix),
                });
            CheapInput {
                route_id: r.id.clone(),
                cost_per_token: day.and_then(|d| cost_per_token(d.cost, d.tokens)),
                blended_ratio: blended,
                real_rate: r.rate_for_ranking(),
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
                experience_ms: day.and_then(|d| d.experience_ms),
                ttft_p95_ms: day.and_then(|d| d.ttft_p95_ms),
                success_rate: day.and_then(|d| d.success_rate),
                demotion,
                tripped,
            }
        })
        .collect();

    (rank(&candidates, prefs, incumbent), basis)
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
            let models = pricing::parse_station_pricing(&value.to_string());
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

/// 请求日志写进库,并按上限裁掉旧的。
///
/// 上限跟别的历史一样是**有意的**:请求日志涨得比谁都快,
/// 不封顶的话库会一直长,而使用者根本不会去看三个月前那一条。
pub const REQUEST_LOG_KEEP: usize = 2000;

/// 把攒下的请求日志落库。
pub fn persist_logs(
    db: &qb_platform::repository::Repository,
    logs: &[qb_station::router::RequestLog],
) -> Result<()> {
    for log in logs {
        db.put("request_logs", &log.id, log)?;
    }
    db.prune("request_logs", REQUEST_LOG_KEEP, |_: &serde_json::Value| {
        false
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use qb_station::schedule::Axis;
    use qb_station::station::model::UsageRow;

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
            None,
            &pricing::Catalog::new(Vec::new()),
        );
        assert_eq!(basis, CheapBasis::CostPerToken);
        assert_eq!(
            ranking.winner.as_deref(),
            Some(routes[0].id.as_str()),
            "按标称倍率会选错那一条"
        );
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
            None,
            &pricing::Catalog::new(Vec::new()),
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
            None,
            &pricing::Catalog::new(Vec::new()),
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

    /// 赢家就是现任时**不换**。
    ///
    /// 换一次的代价不是零：正在飞的请求会落在另一把 key 上、日志断成两段、
    /// 熔断窗口重来。每一跳都"换"到自己身上，等于每分钟制造一次这些代价，
    /// 而排名一动不动。
    #[test]
    fn the_scheduler_does_not_switch_to_the_route_that_is_already_current() {
        let r = ranking("a", vec![row("a", true, false), row("b", true, false)]);
        assert_eq!(next_upstream(&r, Some("a")), None);
        assert_eq!(next_upstream(&r, Some("b")), Some("a".into()));
        assert_eq!(next_upstream(&r, None), Some("a".into()));
    }

    /// ⛔ 熔断中或没过底线的那条，即使排序把它顶上来也不换。
    ///
    /// 底线全卡光时 rank 会放开底线重排一轮 —— 那是为了不制造死局
    /// （界面上还能看见名次），**不是**为了把不合格的那条送上生产。
    /// 这两件事混在一起的话，一次全站抖动会把所有人换到熔断中的线路上。
    #[test]
    fn a_tripped_or_ineligible_winner_is_never_switched_to() {
        let tripped = ranking("a", vec![row("a", true, true)]);
        assert_eq!(next_upstream(&tripped, Some("b")), None);
        let blocked = ranking("a", vec![row("a", false, false)]);
        assert_eq!(next_upstream(&blocked, Some("b")), None);
    }

    /// 排不出赢家就不换 —— 没勾维度、全没证据都算这一档。
    #[test]
    fn no_winner_means_the_incumbent_stays() {
        let none = Ranking {
            winner: None,
            ..ranking("a", vec![row("a", true, false)])
        };
        assert_eq!(next_upstream(&none, Some("b")), None);
        assert_eq!(next_upstream(&none, None), None);
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
