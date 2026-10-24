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

use crate::domain::{Client, Credential, Provider};
use crate::error::{GateError, Result};
use crate::repository::Repository;
use qb_platform::repository::RouterRequestQuery;
use qb_station::health::{self, Windows};
use qb_station::router::RouterRequest;
use qb_station::schedule::{
    cheap_values, cost_per_token, rank, Candidate, CheapBasis, CheapInput, Prefs, Ranking, Schedule,
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
    incumbent: Option<&str>,
    // 官方价目。四类倍率都要拿站点单价除它,见下面那段。
    catalog: &pricing::Catalog,
    // 每家站的充值比例。不在表里的按 1 算。
    topup: &HashMap<String, f64>,
) -> (Ranking, CheapBasis) {
    let topup_of = |r: &Route| {
        topup
            .get(&r.station_id)
            .copied()
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(crate::domain::DEFAULT_TOPUP_PER_USD)
    };
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
            None,
            &pricing::Catalog::new(Vec::new()),
            &HashMap::new(),
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
            None,
            &catalog,
            &HashMap::new(),
        );
        assert_eq!(naive.winner.as_deref(), Some(routes[0].id.as_str()));

        let topup = HashMap::from([(routes[0].station_id.clone(), 7.0)]);
        let (ranking, basis) = decide(
            &routes,
            &HashMap::new(),
            &HashMap::new(),
            &prefs,
            None,
            &catalog,
            &topup,
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
            None,
            &catalog,
            &topup,
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
            None,
            &pricing::Catalog::new(Vec::new()),
            &HashMap::new(),
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
            None,
            &pricing::Catalog::new(Vec::new()),
            &HashMap::new(),
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
            &HashMap::new(),
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
