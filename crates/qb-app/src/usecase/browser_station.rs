//! Explicit browser imports: site-bound credentials and anonymous price snapshots.
//! No model requests are sent here. Public prices never become billing evidence.
use super::station_billing::{self, StationBillingSettings};
use crate::{
    domain::Provider,
    error::{GateError, Result},
    repository::Repository,
};
use qb_station::station::pricing::{self, Catalog, StationModel, StationRates};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use ts_rs::TS;

const PRICE_TTL: Duration = Duration::from_secs(300);
const MAX_MODELS: usize = 5000;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BrowserStation {
    pub id: String,
    pub name: String,
    pub origin: String,
}

// Deliberately no Debug/TS: request credentials must not enter diagnostics or UI types.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserStationConnect {
    pub station_id: String,
    pub origin: String,
    pub backend: String,
    pub captured: Value,
    #[serde(default)]
    pub cookie: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserStationPricing {
    pub station_id: String,
    pub origin: String,
    pub backend: String,
    pub pricing: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BrowserStationPrice {
    pub model: String,
    pub group: String,
    pub group_name: String,
    /// Input, cache read, cache write, output; station-credit USD per million tokens.
    pub prices_per_mtok: [Option<f64>; 4],
    pub official_multipliers: [Option<f64>; 4],
    pub per_request_price: Option<f64>,
    pub notice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BrowserStationPrices {
    pub station_id: String,
    pub origin: String,
    pub backend: String,
    pub fetched_at_ms: f64,
    pub source_path: String,
    pub notice: String,
    pub models: Vec<BrowserStationPrice>,
}

#[derive(Clone)]
struct PriceEntry {
    model: StationModel,
    group_name: String,
    fixed: bool,
    per_request: bool,
    request_price: Option<f64>,
    notice: String,
}

impl PriceEntry {
    fn prices(&self) -> [Option<f64>; 4] {
        if !self.fixed || self.per_request {
            return [None; 4];
        }
        let r = &self.model.rates;
        let scale = r.group_ratio.unwrap_or(1.0);
        let base = if r.input_price.is_some() || r.output_price.is_some() {
            [
                r.input_price,
                r.cache_read_price,
                r.cache_write_price,
                r.output_price,
            ]
        } else {
            let clean = |n: Option<f64>| n.filter(|n| n.is_finite() && *n >= 0.0);
            let input = clean(r.model_ratio).map(|n| n * pricing::NEW_API_USD_PER_MTOK);
            [
                input,
                input.zip(clean(r.cache_ratio)).map(|(n, r)| n * r),
                input.zip(clean(r.create_cache_ratio)).map(|(n, r)| n * r),
                input.zip(clean(r.completion_ratio)).map(|(n, r)| n * r),
            ]
        };
        base.map(|n| {
            n.filter(|n| n.is_finite() && *n >= 0.0)
                .map(|n| n * scale)
                .filter(|n| n.is_finite())
        })
    }

    fn cacheable(&self) -> bool {
        // The legacy pricing model treats zero multipliers as unknown. Keep explicit
        // zero prices in the browser view without changing that historical contract.
        self.fixed
            && self.model.rates.group_ratio.is_some_and(|n| n > 0.0)
            && self
                .prices()
                .into_iter()
                .zip(self.model.rates.category_prices())
                .all(|(a, b)| match (a, b) {
                    (Some(a), Some(b)) => (a - b).abs() <= a.abs().max(1.0) * 1e-12,
                    (None, None) => true,
                    _ => false,
                })
            && (!self.per_request || self.request_price.is_some_and(|n| n > 0.0))
    }

    fn view(&self, catalog: &Catalog) -> BrowserStationPrice {
        let prices = self.prices();
        let official = catalog.resolve(&self.model.model);
        let multipliers = std::array::from_fn(|i| {
            let cost = official
                .as_ref()?
                .category(pricing::PriceCategory::ALL[i])
                .per_mtok;
            if !cost.is_finite() || cost <= 0.0 {
                return None;
            }
            prices[i].map(|n| n / cost).filter(|n| n.is_finite())
        });
        BrowserStationPrice {
            model: self.model.model.clone(),
            group: self.model.groups[0].clone(),
            group_name: self.group_name.clone(),
            prices_per_mtok: prices,
            official_multipliers: multipliers,
            per_request_price: if self.fixed && self.per_request {
                self.request_price
                    .zip(self.model.rates.group_ratio)
                    .map(|(p, r)| p * r)
                    .filter(|n| n.is_finite())
            } else {
                None
            },
            notice: self.notice.clone(),
        }
    }
}

struct Snapshot {
    base: String,
    revision: u32,
    received: Instant,
    entries: Vec<PriceEntry>,
}

fn snapshots() -> &'static Mutex<HashMap<String, Snapshot>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Snapshot>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn problem(message: &str) -> GateError {
    GateError::Other(message.into())
}

fn checked_provider(db: &Repository, id: &str, expected: &str) -> Result<Provider> {
    let provider: Provider = db.get("providers", id)?;
    let origin = station_billing::origin(&provider.base_url).map_err(GateError::Other)?;
    // The caller sends an origin, never an arbitrary target URL/path/query.
    if expected != origin {
        return Err(problem("当前网页与所选 QB 站点不同源，请重新选择"));
    }
    Ok(provider)
}

pub fn stations() -> Result<Vec<BrowserStation>> {
    stations_in(&Repository::open()?)
}

fn stations_in(db: &Repository) -> Result<Vec<BrowserStation>> {
    let mut result: Vec<_> = db
        .list::<Provider>("providers")?
        .into_iter()
        .filter_map(|p| {
            station_billing::origin(&p.base_url)
                .ok()
                .map(|origin| BrowserStation {
                    id: p.id,
                    name: p.name,
                    origin,
                })
        })
        .collect();
    result.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(result)
}

pub async fn connect(request: BrowserStationConnect) -> Result<StationBillingSettings> {
    checked_provider(&Repository::open()?, &request.station_id, &request.origin)?;
    station_billing::connect_extension(
        &request.station_id,
        &request.origin,
        &request.backend,
        request.captured,
        request.cookie,
    )
    .await
}

fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(qb_station::station::billing::number)
        .filter(|n| n.is_finite() && *n >= 0.0)
}

fn identifier(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|n| n.to_string()))
}

fn parse_public(backend: &str, value: &Value) -> Result<Vec<PriceEntry>> {
    if value.get("success") == Some(&Value::Bool(false))
        || value
            .get("code")
            .and_then(Value::as_i64)
            .is_some_and(|n| n != 0 && n != 200)
    {
        return Err(problem("站点没有成功返回公开价目表"));
    }
    let mut out = Vec::new();
    match backend {
        "newapi" => {
            let base = pricing::parse_station_pricing(&value.to_string());
            let groups = value.get("group_ratio").and_then(Value::as_object);
            for model in base {
                let raw = value
                    .get("data")
                    .unwrap_or(value)
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|r| {
                        r.get("model_name")
                            .or_else(|| r.get("model"))
                            .and_then(Value::as_str)
                            == Some(model.model.as_str())
                    });
                let mode = raw
                    .and_then(|r| r.get("billing_mode"))
                    .and_then(Value::as_str);
                if raw
                    .and_then(|r| r.get("enable_groups"))
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty)
                {
                    continue;
                }
                let per_request = model.rates.per_request()
                    || mode == Some("per_request")
                    || raw
                        .and_then(|r| r.get("quota_type"))
                        .and_then(Value::as_i64)
                        == Some(1);
                let request_price = raw
                    .and_then(|r| number(r.get("model_price")))
                    .or(model.rates.per_request_price);
                let names: Vec<String> = if model.groups.is_empty() {
                    groups
                        .map(|g| g.keys().filter(|g| g.as_str() != "auto").cloned().collect())
                        .unwrap_or_else(|| vec!["default".into()])
                } else {
                    model.groups.clone()
                };
                for group in names {
                    if group == "auto" || group.len() > 256 || model.model.len() > 256 {
                        continue;
                    }
                    let mut model = model.clone();
                    model.rates.group_ratio = number(groups.and_then(|g| g.get(&group)));
                    model.groups = vec![group.clone()];
                    let unsupported = mode
                        .is_some_and(|m| !["ratio", "token", "tokens", "per_request"].contains(&m))
                        || raw
                            .and_then(|r| r.get("quota_type"))
                            .and_then(Value::as_i64)
                            .is_some_and(|n| n != 0 && n != 1);
                    let fixed = model.rates.group_ratio.is_some() && !unsupported;
                    out.push(PriceEntry {
                        model,
                        group_name: group,
                        fixed,
                        per_request,
                        request_price,
                        notice: if unsupported {
                            "计费模式不支持固定 token 价格，请以站点规则为准"
                        } else if fixed {
                            ""
                        } else {
                            "分组倍率未知，未计算折后价格"
                        }
                        .into(),
                    });
                    if out.len() > MAX_MODELS {
                        return Err(problem("公开价目表条目过多"));
                    }
                }
            }
        }
        "sub2" => {
            let data = value.get("data").unwrap_or(value);
            let groups = data
                .get("groups")
                .and_then(Value::as_array)
                .ok_or_else(|| problem("Sub2API 模型广场没有提供分组价格"))?;
            for group in groups {
                let Some(id) = identifier(&group["id"]) else {
                    continue;
                };
                let name = identifier(&group["name"]).unwrap_or_else(|| id.clone());
                // This endpoint was requested anonymously. A personal rate is not a public rate.
                if group
                    .get("user_rate_multiplier")
                    .is_some_and(|v| !v.is_null())
                {
                    return Err(problem(
                        "请读取未携带登录凭证的公开价格，不能混用账户专属报价",
                    ));
                }
                let rate = number(group.get("rate_multiplier"));
                for row in group
                    .get("models")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let Some(model) = identifier(&row["name"]) else {
                        continue;
                    };
                    let p = &row["pricing"];
                    let per_request = p["billing_mode"] == "per_request";
                    let request_price = number(p.get("per_request_price"));
                    let unit = |k| {
                        number(p.get(k))
                            .map(|n| n * 1_000_000.0)
                            .filter(|n| n.is_finite())
                    };
                    let complex = group["peak_rate_enabled"] == true
                        || row.get("time_pricing").is_some_and(|v| !v.is_null())
                        || p.get("reasoning_effort_multipliers").is_some_and(|v| {
                            !v.is_null() && v.as_object().is_none_or(|m| !m.is_empty())
                        })
                        || p.get("intervals")
                            .and_then(Value::as_array)
                            .is_some_and(|v| !v.is_empty())
                        || p.get("billing_mode")
                            .and_then(Value::as_str)
                            .is_some_and(|s| !["tokens", "token", "per_request"].contains(&s));
                    let fixed = !complex && rate.is_some();
                    let rates = StationRates {
                        input_price: unit("input_price"),
                        output_price: unit("output_price"),
                        cache_read_price: unit("cache_read_price"),
                        cache_write_price: unit("cache_write_price"),
                        per_request_price: number(p.get("per_request_price")).filter(|n| *n > 0.0),
                        group_ratio: rate,
                        ..Default::default()
                    };
                    out.push(PriceEntry {
                        model: StationModel {
                            model,
                            rates,
                            groups: vec![id.clone()],
                        },
                        group_name: name.clone(),
                        fixed,
                        per_request,
                        request_price,
                        notice: if complex {
                            "含阶梯、分时或非 token 定价，请以站点完整规则为准"
                        } else if rate.is_none() {
                            "分组倍率未知，未计算折后价格"
                        } else {
                            ""
                        }
                        .into(),
                    });
                    if out.len() > MAX_MODELS {
                        return Err(problem("公开价目表条目过多"));
                    }
                }
            }
        }
        _ => return Err(problem("仅支持 New API 与 Sub2API")),
    }
    if out.is_empty() {
        return Err(problem("公开接口没有可识别的模型价格"));
    }
    Ok(out)
}

pub fn pricing(request: BrowserStationPricing) -> Result<BrowserStationPrices> {
    pricing_in(&Repository::open()?, request)
}

fn pricing_in(db: &Repository, request: BrowserStationPricing) -> Result<BrowserStationPrices> {
    let provider = checked_provider(db, &request.station_id, &request.origin)?;
    let entries = parse_public(&request.backend, &request.pricing)?;
    let fetched = db
        .meta("station_prices")?
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let catalog = Catalog::new(fetched);
    let models = entries.iter().map(|entry| entry.view(&catalog)).collect();
    let mut cache = snapshots()
        .lock()
        .map_err(|_| problem("浏览器价目表缓存不可用"))?;
    cache.retain(|_, value| value.received.elapsed() < PRICE_TTL);
    if cache.len() >= 64 && !cache.contains_key(&request.station_id) {
        return Err(problem("暂存价目表的站点过多，请稍后再试"));
    }
    cache.insert(
        request.station_id.clone(),
        Snapshot {
            base: crate::endpoint::endpoint_base(&provider.base_url)?,
            revision: provider.revision,
            received: Instant::now(),
            entries,
        },
    );
    Ok(BrowserStationPrices {
        station_id: request.station_id,
        origin: request.origin,
        source_path: if request.backend == "newapi" {
            "/api/pricing"
        } else {
            "/api/v1/model-plaza"
        }
        .into(),
        backend: request.backend,
        fetched_at_ms: chrono::Utc::now().timestamp_millis() as f64,
        notice: "站点公开报价，按 QB 官方参考价计算；不是实扣倍率。仅在本次运行中暂存 5 分钟。"
            .into(),
        models,
    })
}

/// Exact provider identity, full endpoint and revision; never persist an anonymous quote as an audit.
pub(crate) fn cached_models(
    station_id: &str,
    revision: u32,
    base: &str,
    group: &str,
) -> Option<Vec<StationModel>> {
    let cache = snapshots().lock().ok()?;
    let snapshot = cache.get(station_id)?;
    if snapshot.base != base
        || snapshot.revision != revision
        || snapshot.received.elapsed() >= PRICE_TTL
    {
        return None;
    }
    let group = if group.is_empty() { "default" } else { group };
    let models: Vec<_> = snapshot
        .entries
        .iter()
        .filter(|e| {
            e.cacheable() && (e.model.groups.iter().any(|g| g == group) || e.group_name == group)
        })
        .map(|e| {
            let mut m = e.model.clone();
            m.groups = vec![group.into()];
            m
        })
        .collect();
    (!models.is_empty()).then_some(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn public_prices_keep_units_groups_and_complex_rules_distinct() {
        let new = parse_public("newapi", &json!({"data":[{"model_name":"claude-opus-5",
            "model_ratio":2.5,"completion_ratio":5,"enable_groups":["team"]}],"group_ratio":{"team":0.2}})).unwrap();
        assert_eq!(
            new[0].model.rates.category_prices(),
            [Some(1.0), None, None, Some(5.0)]
        );
        let sub = parse_public("sub2", &json!({"data":{"groups":[{"id":9,"name":"team","rate_multiplier":0.2,
            "models":[{"name":"claude-opus-5","pricing":{"input_price":0.000005,"output_price":0.000025,"billing_mode":"tokens"}}]}]}})).unwrap();
        assert_eq!(
            sub[0].model.rates.category_prices(),
            new[0].model.rates.category_prices()
        );
        let complex = parse_public(
            "sub2",
            &json!({"data":{"groups":[{"id":9,"rate_multiplier":0.2,"peak_rate_enabled":true,
            "models":[{"name":"fixture","pricing":{"input_price":0.000005}}]}]}}),
        )
        .unwrap();
        assert!(!complex[0].fixed);
        assert!(parse_public(
            "sub2",
            &json!({"data":{"groups":[{"id":9,"user_rate_multiplier":0.1}]}})
        )
        .is_err());
        let unknown = parse_public(
            "newapi",
            &json!({"data":[{"model_name":"fixture","model_ratio":2.5}]}),
        )
        .unwrap();
        assert!(!unknown[0].fixed);
    }

    #[test]
    fn explicit_free_prices_and_fixed_request_prices_remain_visible_without_legacy_cache_coercion()
    {
        let catalog = Catalog::new(vec![]);
        let free = parse_public(
            "newapi",
            &json!({"data":[{"model_name":"claude-opus-5","model_ratio":2.5,
            "completion_ratio":5,"enable_groups":["free"]}],"group_ratio":{"free":0}}),
        )
        .unwrap();
        let view = free[0].view(&catalog);
        assert_eq!(view.prices_per_mtok, [Some(0.0), None, None, Some(0.0)]);
        assert_eq!(view.official_multipliers[0], Some(0.0));
        assert!(!free[0].cacheable());
        let sub = parse_public("sub2", &json!({"data":{"groups":[{"id":1,"rate_multiplier":0.2,"models":[
            {"name":"claude-opus-5","pricing":{"billing_mode":"token","input_price":0,"output_price":0}},
            {"name":"request-model","pricing":{"billing_mode":"per_request","per_request_price":0.5}}
        ]}]}})).unwrap();
        assert_eq!(
            sub[0].view(&catalog).prices_per_mtok,
            [Some(0.0), None, None, Some(0.0)]
        );
        assert!(!sub[0].cacheable());
        assert_eq!(sub[1].view(&catalog).per_request_price, Some(0.1));
        assert_eq!(sub[1].view(&catalog).official_multipliers, [None; 4]);
        assert!(sub[1].cacheable());
        let per = parse_public(
            "newapi",
            &json!({"data":[{"model_name":"request-model","quota_type":1,"model_price":0}],
            "group_ratio":{"default":1}}),
        )
        .unwrap();
        assert_eq!(per[0].view(&catalog).per_request_price, Some(0.0));
        assert_eq!(per[0].view(&catalog).prices_per_mtok, [None; 4]);
        assert!(!per[0].cacheable());
        assert!(parse_public(
            "newapi",
            &json!({"data":[{"model_name":"disabled","model_ratio":1,"enable_groups":[]}],
            "group_ratio":{"default":1}})
        )
        .is_err());
    }

    #[test]
    fn public_cache_is_bound_to_site_endpoint_revision_group_and_expiry() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        let id = format!("fixture-{}", crate::config_io::id());
        let provider = Provider {
            id: id.clone(),
            name: "fixture".into(),
            base_url: "https://fixture.invalid/v1".into(),
            website: String::new(),
            note: String::new(),
            tags: vec![],
            favorite: false,
            revision: 3,
            topup_per_usd: None,
        };
        db.put("providers", &id, &provider).unwrap();
        assert_eq!(
            stations_in(&db).unwrap()[0].origin,
            "https://fixture.invalid"
        );
        assert!(checked_provider(&db, &id, "https://other.invalid").is_err());
        assert!(checked_provider(&db, &id, "https://fixture.invalid/").is_err());
        let result = pricing_in(&db, BrowserStationPricing {station_id:id.clone(),origin:"https://fixture.invalid".into(),backend:"newapi".into(),
            pricing:json!({"data":[{"model_name":"claude-opus-5","model_ratio":2.5}],"group_ratio":{"default":0.2}})}).unwrap();
        assert_eq!(result.models[0].prices_per_mtok[0], Some(1.0));
        let base = crate::endpoint::endpoint_base(&provider.base_url).unwrap();
        assert!(cached_models(&id, 3, &base, "").is_some());
        assert!(cached_models(&id, 4, &base, "").is_none());
        assert!(cached_models(&id, 3, "https://other.invalid", "").is_none());
        assert!(cached_models(&id, 3, &base, "private").is_none());
        snapshots().lock().unwrap().get_mut(&id).unwrap().received = Instant::now() - PRICE_TTL;
        assert!(cached_models(&id, 3, &base, "").is_none());
        snapshots().lock().unwrap().remove(&id);
        assert!(db.meta(&format!("station-billing:{id}")).unwrap().is_none());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
