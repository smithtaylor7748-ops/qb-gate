// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 站点余额与编程套餐额度的「问」那一半（中转站重构阶段 6 · U3）。认哪一家、怎么解析在
//! `qb-station::station::site_quota`（纯函数）。
//!
//! # 什么时候联网（计划 §7 阶段 6：只在打开页面或点刷新时问）
//!
//! | `force` | 做什么 |
//! |---|---|
//! | `false`（打开中转站页、切到这个软件） | 这把 Key 5 分钟内问过就用那一份、不联网；否则问一次 |
//! | `true`（点站点卡表头的刷新图标） | 问一次 |
//!
//! 没有定时器。站点地址认不出是哪一家（绝大多数中转站）就**一个请求都不发**，直接回 `None`。
//! Key 只发给它本来就配给的那一家（`site_quota` 模块头「只认主机名」）；不跟随跳转。
//! 结果**只显示**：调度、排序、换线一概不读它。

use super::station_billing;
use super::station_ops;
use crate::domain::{Credential, Provider};
use crate::error::{GateError, Result};
use crate::repository::Repository;
use qb_station::station::route::Route;
use qb_station::station::site_quota::{self, Detected, KeyHeader, SiteQuota};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// 打开页面时，这么久之内问过的就不再问。
const FRESH_MS: i64 = 5 * 60_000;

/// 每把 Key（id + 版本）最近一次问到的，带问的时刻。只在内存里。
static LAST: Mutex<Option<HashMap<String, (i64, SiteQuota)>>> = Mutex::new(None);

fn cached(key: &str) -> Option<(i64, SiteQuota)> {
    LAST.lock().ok()?.as_ref()?.get(key).cloned()
}

fn remember(key: String, at: i64, q: SiteQuota) {
    if let Ok(mut map) = LAST.lock() {
        map.get_or_insert_with(HashMap::new).insert(key, (at, q));
    }
}

/// 要问的东西：哪一家、哪把 Key、记在哪个缓存键下。读库这一步不跨 `await`。
struct Ask {
    detected: Detected,
    key: String,
    cache_key: String,
    site: String,
}

fn prepare(route_id: &str) -> Result<Option<Ask>> {
    let db = Repository::open()?;
    let routes: Vec<Route> = db.list("station_routes").unwrap_or_default();
    let route = routes
        .into_iter()
        .find(|r| r.id == route_id)
        .ok_or_else(|| GateError::Other("找不到这条线路".into()))?;
    let provider: Provider = db
        .get("providers", &route.station_id)
        .map_err(|_| GateError::Other("这条线路的站点不存在".into()))?;
    let Some(detected) = site_quota::detect(&provider.base_url) else {
        return Ok(None);
    };
    let source = station_ops::source(&db, &route)?;
    // Key 换了（同一个 id、版本加一）就不能再拿旧 Key 问到的那份顶着。
    let credential: Credential = db.get(
        "credentials",
        route.credential_id.as_deref().unwrap_or_default(),
    )?;
    Ok(Some(Ask {
        detected,
        key: source.key,
        cache_key: format!("{}#{}", credential.id, credential.revision),
        site: provider.name,
    }))
}

/// 站点卡表头那一格。认不出是哪一家就是 `None`（不联网）。
pub async fn quota(route_id: &str, force: bool) -> Result<Option<SiteQuota>> {
    let Some(ask) = prepare(route_id)? else {
        return Ok(None);
    };
    let now = chrono::Utc::now().timestamp_millis();
    if !force {
        if let Some((at, q)) = cached(&ask.cache_key) {
            if now - at < FRESH_MS {
                return Ok(Some(q));
            }
        }
    }
    let mut q = fetch(ask.detected, &ask.key, &station_billing::http_client()?).await?;
    q.route_id = route_id.to_string();
    remember(ask.cache_key, now, q.clone());
    crate::audit::write(&format!(
        "问了一次站点「{}」的{}（只读，不记内容）",
        ask.site,
        match q.kind {
            site_quota::SiteQuotaKind::Balance => "余额",
            site_quota::SiteQuotaKind::Plan => "编程套餐额度",
        }
    ));
    Ok(Some(q))
}

async fn fetch(d: Detected, key: &str, http: &reqwest::Client) -> Result<SiteQuota> {
    fetch_at(d, key, http, d.request().url).await
}

/// `url` 单独拿出来是为了测试能指到本机的假服务；正式路径永远是 [`Detected::request`] 给的那个。
async fn fetch_at(d: Detected, key: &str, http: &reqwest::Client, url: &str) -> Result<SiteQuota> {
    let request = d.request();
    let mut builder = http.get(url);
    builder = match request.key_header {
        KeyHeader::Bearer => builder.bearer_auth(key),
        KeyHeader::Raw => builder.header("authorization", key),
    };
    for (name, value) in request.headers {
        builder = builder.header(*name, *value);
    }
    let response = builder.send().await.map_err(|e| {
        GateError::Other(format!(
            "{} 的额度接口{}",
            d.name(),
            if e.is_timeout() {
                "超时了"
            } else {
                "连不上，看看网络"
            }
        ))
    })?;
    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        return Err(GateError::Other(
            d.rejected(status).map(str::to_string).unwrap_or_else(|| {
                format!(
                    "{} 拒绝了这把 Key（HTTP {status}）—— 到它的后台看看 Key 还在不在、有没有查询权限",
                    d.name()
                )
            }),
        ));
    }
    if !(200..300).contains(&status) {
        // 不原样搬回复内容（可能带 HTML 或别的东西），见 `station_billing::http_problem`。
        return Err(GateError::Other(station_billing::http_problem(
            status,
            false,
            &format!("{} 的额度接口", d.name()),
        )));
    }
    let body = response
        .text()
        .await
        .map_err(|_| GateError::Other(format!("{} 的额度接口回复没读完", d.name())))?;
    let v: Value = serde_json::from_str(&body)
        .map_err(|_| GateError::Other(format!("{} 的额度接口回的不是 JSON", d.name())))?;
    let mut q = site_quota::parse(d, &v).map_err(GateError::Other)?;
    q.fetched_at = chrono::Utc::now().to_rfc3339();
    Ok(q)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// 起一个只回一次的假服务，把收到的请求头原样交回来。
    fn serve_once(status: &str, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/quota", listener.local_addr().unwrap());
        let status = status.to_string();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let reply = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(reply.as_bytes()).unwrap();
            request
        });
        (url, handle)
    }

    #[tokio::test]
    async fn zhipu_gets_the_raw_key_and_a_rejection_never_echoes_the_reply() {
        let http = reqwest::Client::new();
        let (url, seen) = serve_once(
            "200 OK",
            r#"{"success":true,"data":{"level":"lite","limits":[{"type":"TOKENS_LIMIT","unit":3,"percentage":20}]}}"#,
        );
        let q = fetch_at(Detected::ZhipuCn, "sk-zhipu", &http, &url)
            .await
            .unwrap();
        let request = seen.join().unwrap().to_ascii_lowercase();
        assert!(
            request.contains("authorization: sk-zhipu\r\n"),
            "智谱不加 Bearer：{request}"
        );
        assert_eq!(q.windows[0].used_percent, 20.0);
        assert!(!q.fetched_at.is_empty());

        let (url, seen) = serve_once("401 Unauthorized", r#"{"secret":"<html>sk-leak</html>"}"#);
        let err = fetch_at(Detected::DeepSeek, "sk-ds", &http, &url)
            .await
            .unwrap_err()
            .to_string();
        assert!(seen
            .join()
            .unwrap()
            .to_ascii_lowercase()
            .contains("authorization: bearer sk-ds"));
        assert!(
            err.contains("HTTP 401") && !err.contains("sk-leak"),
            "{err}"
        );

        let (url, _) = serve_once("500 Internal Server Error", r#"<html>sk-leak</html>"#);
        let err = fetch_at(Detected::Kimi, "k", &http, &url)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("HTTP 500") && !err.contains("sk-leak"),
            "{err}"
        );

        let (url, _) = serve_once("403 Forbidden", "{}");
        let err = fetch_at(Detected::OpencodeGo, "k", &http, &url)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("没有 OpenCode Go 订阅"), "{err}");
    }

    #[test]
    fn a_fresh_answer_is_reused_and_a_new_key_revision_is_not() {
        let q = SiteQuota {
            provider: "DeepSeek".into(),
            kind: site_quota::SiteQuotaKind::Balance,
            balances: vec![],
            windows: vec![],
            plan: None,
            warning: None,
            fetched_at: String::new(),
            route_id: "r".into(),
        };
        remember("cred-test#1".into(), 1_000, q.clone());
        assert_eq!(cached("cred-test#1").map(|(at, _)| at), Some(1_000));
        assert!(cached("cred-test#2").is_none(), "换了 Key 不拿旧的顶");
    }
}
