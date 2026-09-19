//! Management credentials and ledger adapters, following station-monitor-standalone.
use crate::{
    domain::Provider,
    error::{GateError, Result},
    repository::Repository,
};
use qb_station::station::{billing, model::UsageRow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StationBillingSettings {
    pub backend: String,
    pub user_id: String,
    pub configured: bool,
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub balance: Option<f64>,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub verified_at: Option<i64>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Saved {
    backend: String,
    user_id: String,
    base_url: String,
    sealed_token: String,
    #[serde(default)]
    account: String,
    #[serde(default)]
    balance: Option<f64>,
    #[serde(default)]
    currency: String,
    #[serde(default)]
    verified_at: Option<i64>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct LedgerAuth {
    pub backend: String,
    pub user_id: String,
    token: String,
    #[serde(default)]
    cookie: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    account: String,
    #[serde(default)]
    password: String,
}
impl std::fmt::Debug for LedgerAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LedgerAuth")
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

fn saved(db: &Repository, station_id: &str) -> Result<Option<Saved>> {
    db.meta(&format!("station-billing:{station_id}"))?
        .map(|raw| serde_json::from_str(&raw).map_err(Into::into))
        .transpose()
}

pub fn settings(db: &Repository, station_id: &str) -> Result<StationBillingSettings> {
    let provider: Provider = db.get("providers", station_id)?;
    let Some(s) = saved(db, station_id)? else {
        return Ok(StationBillingSettings {
            backend: "none".into(),
            ..Default::default()
        });
    };
    Ok(StationBillingSettings {
        configured: s.base_url == crate::endpoint::endpoint_base(&provider.base_url)?
            && !s.sealed_token.is_empty(),
        backend: s.backend,
        user_id: s.user_id,
        account: s.account,
        balance: s.balance,
        currency: s.currency,
        verified_at: s.verified_at,
    })
}

pub fn save(
    db: &Repository,
    station_id: &str,
    backend: &str,
    user_id: &str,
    token: Option<&str>,
) -> Result<StationBillingSettings> {
    let provider: Provider = db.get("providers", station_id)?;
    if !["none", "newapi", "sub2"].contains(&backend) {
        return Err(GateError::Other("不支持的账单后端".into()));
    }
    let base_url = crate::endpoint::endpoint_base(&provider.base_url)?;
    let user_id = user_id.trim();
    if backend == "newapi" && (user_id.is_empty() || !user_id.bytes().all(|c| c.is_ascii_digit())) {
        return Err(GateError::Other(
            "New API 账单需要数字用户 ID（New-Api-User）".into(),
        ));
    }
    let sealed_token = if backend == "none" {
        String::new()
    } else if let Some(token) = token.map(str::trim).filter(|s| !s.is_empty()) {
        if token.contains(['\r', '\n']) {
            return Err(GateError::Other("账单令牌不能包含换行".into()));
        }
        crate::secret::seal(token)?
    } else {
        saved(db, station_id)?
            .filter(|old| {
                old.base_url == base_url && old.backend == backend && old.user_id == user_id
            })
            .map(|s| s.sealed_token)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                GateError::Other("请填写站点后台的访问令牌；模型 API Key 不能代替账单登录".into())
            })?
    };
    db.set_meta(
        &format!("station-billing:{station_id}"),
        &serde_json::to_string(&Saved {
            backend: backend.into(),
            user_id: user_id.into(),
            base_url,
            sealed_token,
            account: String::new(),
            balance: None,
            currency: String::new(),
            verified_at: None,
        })?,
    )?;
    settings(db, station_id)
}

pub fn load(db: &Repository, station_id: &str, base_url: &str) -> Result<Option<LedgerAuth>> {
    let Some(s) = saved(db, station_id)? else {
        return Ok(None);
    };
    if s.backend == "none" {
        return Ok(None);
    }
    if s.base_url != base_url {
        return Err(GateError::Other(
            "站点地址已更改，请重新保存账单凭证".into(),
        ));
    }
    let token = crate::secret::open(&s.sealed_token)
        .ok_or_else(|| GateError::Other("账单凭证无法解密，请重新保存".into()))?;
    if token.starts_with('{') {
        return serde_json::from_str(&token).map(Some).map_err(Into::into);
    }
    Ok(Some(LedgerAuth {
        backend: s.backend,
        user_id: s.user_id,
        token,
        ..Default::default()
    }))
}

pub fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(8))
        .timeout(std::time::Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/124.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| GateError::Other(e.to_string()))
}

/// Never display arbitrary response bodies, which may contain credentials or HTML.
pub fn http_problem(status: u16, cloudflare: bool, purpose: &str) -> String {
    match status {
        401 if purpose.contains("账单") => {
            format!("{purpose}返回 HTTP 401：后台登录已过期，请重新登录账单账户")
        }
        401 => {
            format!("{purpose}返回 HTTP 401：访问凭证无效或已过期，请检查模型 API Key 或接口权限")
        }
        403 if cloudflare => {
            format!("{purpose}被站点 Cloudflare 拒绝（HTTP 403）；请联系站点检查 API 访问规则")
        }
        403 => format!("{purpose}返回 HTTP 403：站点拒绝访问，请检查凭证权限"),
        404 => format!("{purpose}返回 HTTP 404：站点没有这个接口，请检查后端类型和地址"),
        429 => format!("{purpose}返回 HTTP 429：站点限流，请稍后重试"),
        _ => format!("{purpose}返回 HTTP {status}"),
    }
}

pub async fn json(
    request: reqwest::RequestBuilder,
    purpose: &str,
) -> std::result::Result<Value, String> {
    let response = request
        .timeout(std::time::Duration::from_secs(25))
        .send()
        .await
        .map_err(|e| {
            format!(
                "{purpose}{}",
                if e.is_timeout() {
                    "超时"
                } else {
                    "连接失败，请检查网络与站点地址"
                }
            )
        })?;
    response_json(response, purpose).await
}

async fn response_json(
    mut response: reqwest::Response,
    purpose: &str,
) -> std::result::Result<Value, String> {
    if !response.status().is_success() {
        return Err(http_problem(
            response.status().as_u16(),
            response.headers().get("server").is_some_and(|v| {
                v.as_bytes()
                    .windows(10)
                    .any(|w| w.eq_ignore_ascii_case(b"cloudflare"))
            }),
            purpose,
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| format!("{purpose}响应中断"))?
    {
        if body.len() + chunk.len() > 4 * 1024 * 1024 {
            return Err(format!("{purpose}响应过大"));
        }
        body.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&body)
        .map_err(|_| format!("{purpose}没有返回 JSON，可能是登录页或防护页"))?;
    if value.get("success") == Some(&Value::Bool(false))
        || value
            .get("code")
            .and_then(Value::as_i64)
            .is_some_and(|v| v != 0 && v != 200)
    {
        return Err(format!(
            "{purpose}未成功，请检查登录信息、二次验证与站点权限"
        ));
    }
    Ok(value)
}

pub async fn usage(
    base_url: &str,
    auth: &LedgerAuth,
    client: &reqwest::Client,
) -> std::result::Result<Vec<UsageRow>, String> {
    let base = base_url.trim_end_matches('/').trim_end_matches("/v1");
    if auth.backend == "newapi" {
        let status = json(client.get(format!("{base}/api/status")), "站点计费单位").await?;
        let data = status.get("data").unwrap_or(&status);
        let unit = data
            .get("quota_per_unit")
            .and_then(billing::number)
            .filter(|v| v.is_finite() && *v > 0.0)
            .ok_or("站点没有公开有效的 quota_per_unit，无法把配额换成金额")?;
        let value = json(
            authorize(
                client.get(format!("{base}/api/log/self")).query(&[
                    ("p", "1"),
                    ("page_size", "100"),
                    ("type", "2"),
                ]),
                base,
                auth,
            ),
            "New API 账单",
        )
        .await?;
        let mut rows = billing::parse_self_log_checked(&value)?;
        for row in &mut rows {
            row.cost = row.cost.map(|cost| cost / unit);
            // type=2 only contains consumption: it cannot establish an overall success rate.
            row.status_reported = false;
        }
        Ok(rows)
    } else if auth.backend == "sub2" {
        let base = base.strip_suffix("/api").unwrap_or(base);
        let value = json(
            authorize(
                client
                    .get(format!("{base}/api/v1/usage"))
                    .query(&[("page", "1"), ("page_size", "100")]),
                base,
                auth,
            ),
            "Sub2API 账单",
        )
        .await?;
        billing::parse_sub2_log(&value)
    } else {
        Err("请选择受支持的账单后端".into())
    }
}

pub fn origin(base: &str) -> std::result::Result<String, String> {
    let url = reqwest::Url::parse(base).map_err(|_| "站点地址无效")?;
    if !["http", "https"].contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("站点地址必须是 HTTP(S)，且不能包含凭证".into());
    }
    Ok(url.origin().ascii_serialization())
}

fn authorize(
    request: reqwest::RequestBuilder,
    base: &str,
    auth: &LedgerAuth,
) -> reqwest::RequestBuilder {
    let mut request = request
        .header("Accept", "application/json, text/plain, */*")
        .header("Origin", base)
        .header("Referer", format!("{base}/"))
        .header("Connection", "close");
    if !auth.cookie.is_empty() {
        request = request.header("Cookie", &auth.cookie);
    }
    if !auth.token.is_empty() {
        request = request.bearer_auth(&auth.token);
    }
    if auth.backend == "newapi" && !auth.user_id.is_empty() {
        request = request.header("New-Api-User", &auth.user_id);
    }
    request
}

fn cookies(response: &reqwest::Response, previous: &str) -> String {
    let mut values = std::collections::BTreeMap::new();
    for item in previous.split(';').chain(
        response
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|h| h.to_str().ok())
            .filter_map(|s| s.split(';').next()),
    ) {
        if let Some((name, value)) = item.trim().split_once('=') {
            values.insert(name.to_string(), value.to_string());
        }
    }
    values
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ")
}

fn data(value: &Value) -> &Value {
    value.get("data").unwrap_or(value)
}
fn identifier(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_u64().map(|v| v.to_string()))
        .unwrap_or_default()
}

pub async fn detect(base: &str, client: &reqwest::Client) -> std::result::Result<String, String> {
    match json(client.get(format!("{base}/api/status")), "站点类型").await {
        Ok(v)
            if data(&v).get("quota_per_unit").is_some()
                || data(&v).get("quota_display_type").is_some() =>
        {
            Ok("newapi".into())
        }
        Ok(_) => Ok("sub2".into()),
        Err(e) if e.contains("404") => Ok("sub2".into()),
        Err(e) => Err(e),
    }
}

fn items<'a>(value: &'a Value, names: &[&str]) -> Vec<&'a Value> {
    let value = data(value);
    value
        .as_array()
        .or_else(|| {
            names
                .iter()
                .find_map(|n| value.get(*n).and_then(Value::as_array))
        })
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

/// Authenticated model discovery uses the same endpoints and per-token -> per-million
/// conversion as station-monitor. Public New API pricing remains the first choice.
pub async fn models(
    base: &str,
    auth: &LedgerAuth,
    client: &reqwest::Client,
    group: &str,
) -> std::result::Result<Vec<qb_station::station::pricing::StationModel>, String> {
    use qb_station::station::pricing::{self, StationModel, StationRates};
    let base = origin(base)?;
    if auth.backend == "newapi" {
        for path in ["/api/pricing", "/api/models"] {
            if let Ok(v) = json(
                authorize(client.get(format!("{base}{path}")), &base, auth),
                "后台模型表",
            )
            .await
            {
                let parsed = pricing::parse_station_pricing(&v.to_string());
                if !parsed.is_empty() {
                    return Ok(parsed);
                }
            }
        }
        return Err("后台没有提供可识别的模型价目表".into());
    }
    let groups = json(
        authorize(
            client.get(format!("{base}/api/v1/groups/available")),
            &base,
            auth,
        ),
        "后台分组",
    )
    .await?;
    let rates = json(
        authorize(
            client.get(format!("{base}/api/v1/groups/rates")),
            &base,
            auth,
        ),
        "分组倍率",
    )
    .await?;
    let channels = json(
        authorize(
            client.get(format!("{base}/api/v1/channels/available")),
            &base,
            auth,
        ),
        "后台模型与价格",
    )
    .await?;
    let target = items(&groups, &["groups", "items"])
        .into_iter()
        .find(|g| identifier(&g["id"]) == group || g["name"].as_str() == Some(group));
    let target_id = target.map(|g| identifier(&g["id"])).unwrap_or_default();
    let target_name = target.and_then(|g| g["name"].as_str()).unwrap_or(group);
    let rates = data(&rates).get("rates").unwrap_or(data(&rates));
    let rate = rates
        .get(&target_id)
        .or_else(|| rates.get(target_name))
        .and_then(|v| {
            billing::number(v).or_else(|| v.get("rate_multiplier").and_then(billing::number))
        })
        .or_else(|| {
            target
                .and_then(|v| v.get("rate_multiplier"))
                .and_then(billing::number)
        });
    let mut out = Vec::new();
    for channel in items(&channels, &["platforms", "channels", "items"]) {
        let bindings: Vec<_> = items(channel, &["groups"])
            .into_iter()
            .flat_map(|g| {
                if g.is_object() {
                    vec![identifier(&g["id"]), identifier(&g["name"])]
                } else {
                    vec![identifier(g)]
                }
            })
            .filter(|g| !g.is_empty())
            .collect();
        if !bindings.is_empty()
            && !bindings
                .iter()
                .any(|g| g == group || g == &target_id || g == target_name)
        {
            continue;
        }
        for item in items(channel, &["supported_models", "models"]) {
            let name = item
                .as_str()
                .or_else(|| item.get("name").and_then(Value::as_str))
                .or_else(|| item.get("id").and_then(Value::as_str))
                .unwrap_or("");
            if name.is_empty() {
                continue;
            }
            let p = &item["pricing"];
            let unit = |key| {
                p.get(key)
                    .and_then(billing::number)
                    .filter(|n| n.is_finite() && *n >= 0.0)
                    .map(|n| n * 1_000_000.0)
            };
            out.push(StationModel {
                model: name.into(),
                groups: vec![group.into()],
                rates: StationRates {
                    input_price: unit("input_price"),
                    output_price: unit("output_price"),
                    cache_read_price: unit("cache_read_price"),
                    cache_write_price: unit("cache_write_price"),
                    per_request_price: p.get("per_request_price").and_then(billing::number),
                    group_ratio: rate,
                    ..Default::default()
                },
            });
        }
    }
    Ok(out)
}

pub async fn balance(
    base: &str,
    auth: &LedgerAuth,
    client: &reqwest::Client,
) -> std::result::Result<(Option<f64>, String), String> {
    let base = origin(base)?;
    if auth.backend == "newapi" {
        let profile = json(
            authorize(client.get(format!("{base}/api/user/self")), &base, auth),
            "后台账单账户",
        )
        .await?;
        let status = json(client.get(format!("{base}/api/status")), "余额单位").await?;
        let unit = data(&status)
            .get("quota_per_unit")
            .and_then(billing::number)
            .filter(|v| *v > 0.0);
        let currency = data(&status)
            .get("quota_display_type")
            .and_then(Value::as_str)
            .unwrap_or("USD")
            .to_uppercase();
        let amount = data(&profile)
            .get("quota")
            .and_then(billing::number)
            .zip(unit)
            .map(|(q, u)| q / u);
        Ok((amount.filter(|v| v.is_finite()), currency))
    } else {
        let profile = match json(
            authorize(client.get(format!("{base}/api/v1/auth/me")), &base, auth),
            "后台账单账户",
        )
        .await
        {
            Err(e) if e.contains("404") => {
                json(
                    authorize(
                        client.get(format!("{base}/api/v1/user/profile")),
                        &base,
                        auth,
                    ),
                    "后台账单账户",
                )
                .await?
            }
            result => result?,
        };
        let profile = data(&profile);
        let amount = profile
            .get("balance")
            .or_else(|| profile.pointer("/user/balance"))
            .and_then(billing::number);
        Ok((amount.filter(|v| v.is_finite()), "USD".into()))
    }
}

async fn password_login(
    base: &str,
    backend: &str,
    account: &str,
    password: &str,
    client: &reqwest::Client,
) -> std::result::Result<LedgerAuth, String> {
    let mut auth = LedgerAuth {
        backend: backend.into(),
        account: account.into(),
        password: password.into(),
        ..Default::default()
    };
    let (path, body) = if backend == "newapi" {
        (
            "/api/user/login",
            serde_json::json!({"username":account,"password":password}),
        )
    } else {
        (
            "/api/v1/auth/login",
            serde_json::json!({"email":account,"password":password}),
        )
    };
    let response = authorize(client.post(format!("{base}{path}")), base, &auth)
        .json(&body)
        .send()
        .await
        .map_err(|_| "站点登录连接失败，请检查网络与地址")?;
    auth.cookie = cookies(&response, "");
    let value = response_json(response, "站点登录").await?;
    let info = data(&value);
    let user = info.get("user").unwrap_or(info);
    if info.get("require_2fa").or_else(|| info.get("requires_2fa")) == Some(&Value::Bool(true)) {
        return Err("账号需要二次验证，请使用「浏览器 / Linux DO」完成登录".into());
    }
    auth.user_id = identifier(&user["id"]);
    auth.token = info
        .get("access_token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .into();
    auth.refresh_token = info
        .get("refresh_token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .into();
    if (backend == "newapi" && auth.user_id.is_empty())
        || (auth.token.is_empty() && auth.cookie.is_empty())
    {
        return Err("登录响应缺少用户 ID 或可复用会话，请使用浏览器登录".into());
    }
    if backend == "newapi" && balance(base, &auth, client).await.is_err() {
        // Same compatibility sequence as the supplied monitor: cookie -> refresh -> user token.
        if let Ok(response) = authorize(
            client.post(format!("{base}/api/user/auth/refresh")),
            base,
            &auth,
        )
        .send()
        .await
        {
            auth.cookie = cookies(&response, &auth.cookie);
            if let Ok(v) = response_json(response, "账单会话刷新").await {
                if let Some(token) = data(&v).get("access_token").and_then(Value::as_str) {
                    auth.token = token.into();
                }
            }
        }
        if auth.token.is_empty() {
            let v = json(
                authorize(client.get(format!("{base}/api/user/token")), base, &auth),
                "账单访问令牌",
            )
            .await?;
            auth.token = data(&v).as_str().unwrap_or("").into();
        }
    }
    Ok(auth)
}

fn persist_auth(
    db: &Repository,
    station_id: &str,
    expected_base: &str,
    auth: &LedgerAuth,
    amount: Option<f64>,
    currency: String,
) -> Result<StationBillingSettings> {
    let provider: Provider = db.get("providers", station_id)?;
    let base_url = crate::endpoint::endpoint_base(&provider.base_url)?;
    if origin(&base_url).map_err(GateError::Other)? != expected_base {
        return Err(GateError::Other(
            "登录期间站点地址已变更，请重新连接".into(),
        ));
    }
    let record = Saved {
        backend: auth.backend.clone(),
        user_id: auth.user_id.clone(),
        base_url,
        sealed_token: crate::secret::seal(&serde_json::to_string(auth)?)?,
        account: auth.account.clone(),
        balance: amount,
        currency,
        verified_at: Some(chrono::Utc::now().timestamp_millis()),
    };
    db.set_meta(
        &format!("station-billing:{station_id}"),
        &serde_json::to_string(&record)?,
    )?;
    settings(db, station_id)
}

pub async fn connect(
    station_id: &str,
    backend: &str,
    account: &str,
    password: &str,
) -> Result<StationBillingSettings> {
    if account.trim().is_empty() || password.is_empty() {
        return Err(GateError::Other("请输入站点账号和密码".into()));
    }
    let base = {
        let provider: Provider = Repository::open()?.get("providers", station_id)?;
        origin(&provider.base_url).map_err(GateError::Other)?
    };
    let client = http_client()?;
    let backend = if backend == "auto" {
        detect(&base, &client).await.map_err(GateError::Other)?
    } else {
        backend.into()
    };
    if !["newapi", "sub2"].contains(&backend.as_str()) {
        return Err(GateError::Other("请选择 New API 或 Sub2API".into()));
    }
    let auth = password_login(&base, &backend, account.trim(), password, &client)
        .await
        .map_err(GateError::Other)?;
    let (amount, currency) = balance(&base, &auth, &client)
        .await
        .map_err(GateError::Other)?;
    usage(&base, &auth, &client)
        .await
        .map_err(GateError::Other)?;
    persist_auth(
        &Repository::open()?,
        station_id,
        &base,
        &auth,
        amount,
        currency,
    )
}

/// Browser capture is accepted only by the native, station-bound login window.
pub async fn connect_browser(
    station_id: &str,
    expected_base: &str,
    captured: Value,
    cookie: String,
) -> Result<StationBillingSettings> {
    let base = {
        let provider: Provider = Repository::open()?.get("providers", station_id)?;
        let base = origin(&provider.base_url).map_err(GateError::Other)?;
        if base != expected_base {
            return Err(GateError::Other("站点地址已变更，请重新登录".into()));
        }
        base
    };
    let client = http_client()?;
    let backend = detect(&base, &client).await.map_err(GateError::Other)?;
    let mut auth = LedgerAuth {
        backend,
        cookie,
        ..Default::default()
    };
    for value in captured.as_array().into_iter().flatten() {
        let info = data(value);
        let user = info.get("user").unwrap_or(info);
        let id = identifier(
            user.get("id")
                .or_else(|| info.get("user_id"))
                .unwrap_or(&Value::Null),
        );
        if !id.is_empty() {
            auth.user_id = id;
        }
        if let Some(t) = info
            .get("access_token")
            .or_else(|| info.get("token"))
            .and_then(Value::as_str)
        {
            if !t.is_empty() {
                auth.token = t.into();
            }
        }
        if let Some(t) = info.get("refresh_token").and_then(Value::as_str) {
            auth.refresh_token = t.into();
        }
        if let Some(name) = user
            .get("username")
            .or_else(|| user.get("email"))
            .and_then(Value::as_str)
        {
            auth.account = name.into();
        }
    }
    if auth.token.is_empty() && auth.cookie.is_empty() {
        return Err(GateError::Other("等待站点登录完成".into()));
    }
    let (amount, currency) = balance(&base, &auth, &client)
        .await
        .map_err(GateError::Other)?;
    usage(&base, &auth, &client)
        .await
        .map_err(GateError::Other)?;
    persist_auth(
        &Repository::open()?,
        station_id,
        &base,
        &auth,
        amount,
        currency,
    )
}

pub async fn refresh(station_id: &str) -> Result<StationBillingSettings> {
    let (base, mut auth) = {
        let db = Repository::open()?;
        let provider: Provider = db.get("providers", station_id)?;
        let base = crate::endpoint::endpoint_base(&provider.base_url)?;
        let auth = load(&db, station_id, &base)?
            .ok_or_else(|| GateError::Other("请先登录账单账户".into()))?;
        (origin(&base).map_err(GateError::Other)?, auth)
    };
    let client = http_client()?;
    let mut result = balance(&base, &auth, &client).await;
    if result.as_ref().is_err_and(|e| e.contains("401")) {
        if !auth.password.is_empty() {
            auth = password_login(&base, &auth.backend, &auth.account, &auth.password, &client)
                .await
                .map_err(GateError::Other)?;
        } else if auth.backend == "sub2" && !auth.refresh_token.is_empty() {
            let v = json(
                authorize(
                    client.post(format!("{base}/api/v1/auth/refresh")),
                    &base,
                    &auth,
                )
                .json(&serde_json::json!({"refresh_token":auth.refresh_token})),
                "账单会话刷新",
            )
            .await
            .map_err(GateError::Other)?;
            auth.token = data(&v)["access_token"]
                .as_str()
                .ok_or_else(|| GateError::Other("请重新在浏览器登录".into()))?
                .into();
            if let Some(t) = data(&v)["refresh_token"].as_str() {
                auth.refresh_token = t.into();
            }
        }
        result = balance(&base, &auth, &client).await;
    }
    let (amount, currency) = result.map_err(GateError::Other)?;
    usage(&base, &auth, &client)
        .await
        .map_err(GateError::Other)?;
    persist_auth(
        &Repository::open()?,
        station_id,
        &base,
        &auth,
        amount,
        currency,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn source_login_uses_cookie_then_refresh_and_sub2_email_protocol() {
        use http_body_util::{BodyExt, Full};
        use hyper::{
            body::{Bytes, Incoming},
            service::service_fn,
            Request, Response,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let service = service_fn(|request: Request<Incoming>| async move {
                        let path = request.uri().path().to_string();
                        let headers = request.headers().clone();
                        let raw = request.into_body().collect().await.unwrap().to_bytes();
                        let payload: Value = serde_json::from_slice(&raw).unwrap_or_default();
                        let (status, body, cookie) = match path.as_str() {
                            "/api/user/login" => {
                                assert_eq!(payload["username"], "fixture-user");
                                assert_eq!(payload["password"], "fixture-password");
                                (
                                    200,
                                    serde_json::json!({"success":true,"data":{"id":19,"username":"fixture-user"}}),
                                    "session=fixture-cookie; HttpOnly; Path=/",
                                )
                            }
                            "/api/user/self" if !headers.contains_key("authorization") => (
                                401,
                                serde_json::json!({"error":"do-not-display-secret"}),
                                "",
                            ),
                            "/api/user/auth/refresh" => {
                                assert_eq!(headers["new-api-user"], "19");
                                assert_eq!(headers["cookie"], "session=fixture-cookie");
                                (
                                    200,
                                    serde_json::json!({"success":true,"data":{"user":{"id":19},"access_token":"fixture-management-token"}}),
                                    "session=rotated; HttpOnly; Path=/",
                                )
                            }
                            "/api/v1/auth/login" => {
                                assert_eq!(payload["email"], "fixture@example.invalid");
                                assert!(payload.get("username").is_none());
                                (
                                    200,
                                    serde_json::json!({"code":0,"data":{"access_token":"fixture-sub2-token","refresh_token":"fixture-refresh","user":{"id":21}}}),
                                    "",
                                )
                            }
                            _ => panic!("unexpected path {path}"),
                        };
                        Ok::<_, std::convert::Infallible>(
                            Response::builder()
                                .status(status)
                                .header("set-cookie", cookie)
                                .body(Full::new(Bytes::from(body.to_string())))
                                .unwrap(),
                        )
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(hyper_util::rt::TokioIo::new(socket), service)
                        .await;
                });
            }
        });
        let http = http_client().unwrap();
        let auth = password_login(&base, "newapi", "fixture-user", "fixture-password", &http)
            .await
            .unwrap();
        assert_eq!(auth.token, "fixture-management-token");
        assert_eq!(auth.cookie, "session=rotated");
        assert_eq!(auth.user_id, "19");
        let sub = password_login(
            &base,
            "sub2",
            "fixture@example.invalid",
            "fixture-password",
            &http,
        )
        .await
        .unwrap();
        assert_eq!(sub.token, "fixture-sub2-token");
        assert_eq!(sub.refresh_token, "fixture-refresh");
        assert!(sub.cookie.is_empty());
        assert!(!format!("{auth:?}").contains("fixture-password"));
        assert!(!format!("{auth:?}").contains("fixture-cookie"));
        server.abort();
    }
    #[tokio::test]
    async fn audit_uses_distinct_credentials_and_matches_the_paid_request() {
        audit_fixture("USD", "USD", true).await;
    }

    #[tokio::test]
    async fn audit_and_health_compare_numbers_regardless_of_currency_labels() {
        for (before, after) in [
            ("CNY", "CNY"),
            ("CNY", "EUR"),
            ("CREDITS", "CREDITS"),
            ("", ""),
        ] {
            audit_fixture(before, after, false).await;
        }
    }

    async fn audit_fixture(
        currency_before: &'static str,
        currency_after: &'static str,
        check_failures: bool,
    ) {
        use super::super::station_ops::{self, BillingSource};
        use http_body_util::{BodyExt, Full};
        use hyper::{
            body::{Bytes, Incoming},
            service::service_fn,
            Request, Response,
        };
        use std::sync::{
            atomic::{AtomicBool, AtomicU16, Ordering},
            Arc, Mutex,
        };
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&requests);
        let matching = Arc::new(AtomicBool::new(true));
        let match_ledger = Arc::clone(&matching);
        let ledger_status = Arc::new(AtomicU16::new(200));
        let mock_status = Arc::clone(&ledger_status);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let observed = Arc::clone(&observed);
                let match_ledger = Arc::clone(&match_ledger);
                let mock_status = Arc::clone(&mock_status);
                tokio::spawn(async move {
                    let service = service_fn(move |request: Request<Incoming>| {
                        let observed = Arc::clone(&observed);
                        let match_ledger = Arc::clone(&match_ledger);
                        let mock_status = Arc::clone(&mock_status);
                        async move {
                            let path = request.uri().path().to_string();
                            let auth = request
                                .headers()
                                .get("authorization")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("")
                                .to_string();
                            let user = request
                                .headers()
                                .get("new-api-user")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("")
                                .to_string();
                            observed.lock().unwrap().push((path.clone(), auth, user));
                            let generation = observed
                                .lock()
                                .unwrap()
                                .iter()
                                .filter(|(p, _, _)| p == "/v1/responses")
                                .count();
                            let _ = request.into_body().collect().await;
                            if path == "/api/log/self" && mock_status.load(Ordering::SeqCst) != 200
                            {
                                return Ok::<_, std::convert::Infallible>(
                                    Response::builder()
                                        .status(mock_status.load(Ordering::SeqCst))
                                        .header("location", "/credential-sink")
                                        .body(Full::new(Bytes::from("private-response-marker")))
                                        .unwrap(),
                                );
                            }
                            let _request_id = if match_ledger.load(Ordering::SeqCst) {
                                format!("fixture-request-{generation}")
                            } else {
                                "unrelated-request".into()
                            };
                            let body = match path.as_str() {
                                "/api/status" => serde_json::json!({"data":{"quota_per_unit":500000,"quota_display_type":if generation == 0 { currency_before } else { currency_after }}}).to_string(),
                                "/api/log/self" => serde_json::json!({"success":true,"data":{"items":(1..=generation).map(|n| serde_json::json!({"created_at":1789732800i64,"model_name":"gpt-5.6-sol","group":"team","request_id":if match_ledger.load(Ordering::SeqCst) {format!("fixture-request-{n}")} else {format!("unrelated-{n}")},"prompt_tokens":1000,"completion_tokens":2,"quota":5000,"other":{"cache_tokens":0,"cache_creation_tokens":0}})).collect::<Vec<_>>()}}).to_string(),
                                "/api/user/self" => serde_json::json!({"success":true,"data":{"id":12,"quota":500000 - 5000 * generation}}).to_string(),
                                "/api/pricing" | "/api/models" => r#"{"data":[]}"#.into(),
                                "/v1/models" => r#"{"data":[{"id":"gpt-5.6-sol"}]}"#.into(),
                                "/v1/responses" => concat!(
                                    "data: {\"type\":\"response.created\"}\n\n",
                                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"OK\"}\n\n",
                                    "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1000,\"input_tokens_details\":{\"cached_tokens\":0,\"cache_write_tokens\":0},\"output_tokens\":2}}}\n\n"
                                ).into(),
                                _ => panic!("unexpected fixture path {path}"),
                            };
                            Ok::<_, std::convert::Infallible>(
                                Response::builder()
                                    .header("x-request-id", format!("fixture-request-{generation}"))
                                    .body(Full::new(Bytes::from(body)))
                                    .unwrap(),
                            )
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(hyper_util::rt::TokioIo::new(socket), service)
                        .await;
                });
            }
        });
        let mut source = BillingSource {
            route_id: "fixture".into(),
            base_url: format!("http://{address}"),
            key: "fixture-model-key".into(),
            client: qb_contract::domain::Client::Codex,
            group: "team".into(),
            ..Default::default()
        };
        let catalog = qb_station::station::pricing::Catalog::new(vec![]);
        let client = http_client().unwrap();
        let missing = station_ops::run_audit(
            &source,
            Some(1.0),
            "gpt-5.6-sol",
            &catalog,
            None,
            &client,
            1789732800000,
        )
        .await;
        assert!(missing.round.problems.iter().any(|p| p.contains("未发送")));
        assert!(!requests
            .lock()
            .unwrap()
            .iter()
            .any(|(p, _, _)| p == "/v1/responses"));
        source.billing = Some(LedgerAuth {
            backend: "newapi".into(),
            user_id: "12".into(),
            token: "fixture-management-token".into(),
            ..Default::default()
        });
        let result = station_ops::run_audit(
            &source,
            Some(0.5),
            "gpt-5.6-sol",
            &catalog,
            None,
            &client,
            1789732800000,
        )
        .await;
        let expected = qb_station::station::pricing::measured_multiplier(
            [Some(1000), Some(0), Some(0), Some(2)],
            Some(0.01),
            &catalog.resolve("gpt-5.6-sol").unwrap(),
        )
        .unwrap();
        assert!((result.round.mult.unwrap() - expected / 0.5).abs() < 0.000001);
        let batch = result.round.batch.as_ref().unwrap();
        assert_eq!(batch.currency, currency_before);
        assert_eq!(batch.samples.len(), 6);
        assert!((batch.total_billed.unwrap() - 0.06).abs() < 0.000001);
        assert_eq!(batch.balance_before, Some(1.0));
        assert!((batch.balance_after.unwrap() - 0.94).abs() < 0.000001);
        assert!((batch.balance_delta.unwrap() - 0.06).abs() < 0.000001);
        assert!(!result.round.problems.iter().any(|p| p.contains("币种")));
        let health =
            station_ops::refresh_health(std::slice::from_ref(&source), &client, 1789732800000)
                .await;
        assert!(health["fixture"].error.is_none());
        assert_eq!(health["fixture"].windows.day.success_rate, None);
        assert_eq!(
            requests
                .lock()
                .unwrap()
                .iter()
                .filter(|(p, _, _)| p == "/v1/responses")
                .count(),
            6
        );
        if !check_failures {
            server.abort();
            return;
        }
        matching.store(false, Ordering::SeqCst);
        let unmatched = station_ops::run_audit(
            &source,
            Some(0.5),
            "gpt-5.6-sol",
            &catalog,
            None,
            &client,
            1789732800000,
        )
        .await;
        assert_eq!(unmatched.round.mult, None);
        assert!(unmatched
            .round
            .problems
            .iter()
            .any(|p| p.contains("不完整")));
        for status in [401, 302] {
            ledger_status.store(status, Ordering::SeqCst);
            let rejected = station_ops::run_audit(
                &source,
                Some(0.5),
                "gpt-5.6-sol",
                &catalog,
                None,
                &client,
                1789732800000,
            )
            .await;
            assert!(rejected
                .round
                .problems
                .iter()
                .any(|p| p.contains("未发送计费请求")));
            assert!(!rejected
                .round
                .problems
                .iter()
                .any(|p| p.contains("private-response-marker")));
        }
        let log = requests.lock().unwrap();
        assert_eq!(
            log.iter().filter(|(p, _, _)| p == "/v1/responses").count(),
            12
        );
        assert!(!log.iter().any(|(p, _, _)| p == "/credential-sink"));
        assert!(log
            .iter()
            .filter(|(p, _, _)| p == "/api/log/self")
            .all(|(_, auth, user)| auth == "Bearer fixture-management-token" && user == "12"));
        assert!(log
            .iter()
            .filter(|(p, _, _)| p.starts_with("/v1/"))
            .all(|(_, auth, user)| auth == "Bearer fixture-model-key" && user.is_empty()));
        server.abort();
    }
    #[test]
    fn credentials_cannot_follow_a_changed_site() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        let mut p = Provider {
            id: "s".into(),
            name: "fixture".into(),
            base_url: "https://fixture.invalid".into(),
            website: String::new(),
            note: String::new(),
            tags: vec![],
            favorite: false,
            revision: 1,
        };
        db.put("providers", "s", &p).unwrap();
        save(&db, "s", "newapi", "12", Some("fixture-ledger-token")).unwrap();
        #[cfg(windows)]
        assert!(!db
            .meta("station-billing:s")
            .unwrap()
            .unwrap()
            .contains("fixture-ledger-token"));
        assert_eq!(
            load(&db, "s", &p.base_url).unwrap().unwrap().token,
            "fixture-ledger-token"
        );
        p.base_url = "https://other.invalid".into();
        db.put("providers", "s", &p).unwrap();
        assert!(!settings(&db, "s").unwrap().configured);
        assert!(load(&db, "s", &p.base_url).is_err());
        assert!(save(&db, "s", "newapi", "12", None).is_err());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
