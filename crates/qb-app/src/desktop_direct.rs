//! Claude 桌面端直连档（中转站重构阶段 4，cc-switch 的 C5「桌面端直连」；使用者 2026-10-26 定的：写真 Key）。
//!
//! 平时桌面端经本机路由：它那份 `claude_desktop_config.json` 里的网关地址是本机路由、Key 是占位串。
//! 选了直连的那条线之后，同一份文件里的网关地址换成**那个站点**、Key 换成**这条线绑着的真 Key**（明文）——
//! 桌面端不再经过本机路由：换线、故障转移、熔断、请求日志、模型映射、协议转换都不管它。
//!
//! 所以只收**不需要本机路由也能用**的线（cc-switch 的 `validate_direct_provider` 同样的几条）：
//! - 是桌面端自己的线，绑着 Key、Key 属于这个站点；
//! - 没设模型映射 —— 直连时没人换模型名，桌面端发的 `claude-*` 原样到站点；
//! - 上游协议是 Anthropic（没设就是跟桌面端一样）、协议探测没判成「不支持」。
//!
//! 这个模块只读库、不写任何文件：`workspace::configuration_files` 写桌面端配置时来这里问「直连谁、地址和 Key 是什么」。
//! 开关、校验聚合、重写配置在 `usecase::station_ops`（这里不能往上依赖编排层，见 `architecture.rs` 的环检测）。

use crate::domain::{Client, Credential, Provider};
use crate::error::{GateError, Result};
use crate::repository::Repository;
use qb_station::station::route::{Route, RouteModelMap, UpstreamFormat};

/// 库里 `metadata` 记直连那条线的键。空串 / 没有 = 经本机路由。
pub const META_KEY: &str = "desktop-direct-route";

/// 直连要写进桌面端配置的那两样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectTarget {
    pub route_id: String,
    /// 站点地址，不带 `/v1`（桌面端自己往后接 `/v1/messages`，跟经本机路由时一样）。
    pub base_url: String,
    /// ⛔ 真 Key。只交给写配置的那一步，别的地方不要拿它。
    pub key: String,
}

/// 现在选的是哪条线。没选就是经本机路由。
pub fn chosen(db: &Repository) -> Option<String> {
    db.meta(META_KEY)
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
}

/// 这条线能不能直连，能的话直连到哪、用哪把 Key。不能就说清楚为什么。
pub fn check(db: &Repository, route: &Route) -> Result<DirectTarget> {
    if route.client != Client::ClaudeDesktop {
        return Err(GateError::Other("只有桌面端自己的线能直连".into()));
    }
    if route.model_map != RouteModelMap::default() {
        return Err(GateError::Other(
            "这条线设了模型映射：直连时没有本机路由替它换模型名，桌面端发的 claude-* 会原样到站点。先清空映射，或者继续经本机路由".into(),
        ));
    }
    if !matches!(
        route.upstream_format,
        None | Some(UpstreamFormat::Anthropic)
    ) {
        return Err(GateError::Other(
            "这条线的上游不是 Anthropic 协议：直连时没有本机路由替它转换。继续经本机路由".into(),
        ));
    }
    if route.protocols.anthropic == Some(false) {
        return Err(GateError::Other(
            "这个站点探测出来不支持 Anthropic 协议，桌面端直连用不了".into(),
        ));
    }
    let provider: Provider = db
        .get("providers", &route.station_id)
        .map_err(|_| GateError::Other("这条线的站点不在了".into()))?;
    let id = route
        .credential_id
        .as_deref()
        .ok_or_else(|| GateError::Other("这条线没绑 API Key".into()))?;
    let credential: Credential = db.get("credentials", id)?;
    if credential.provider_id != route.station_id {
        return Err(GateError::Other("这把 API Key 不属于这条线的站点".into()));
    }
    let key = db.key(id)?;
    if key.trim().is_empty() {
        return Err(GateError::Other("这条线的 API Key 是空的".into()));
    }
    let base = crate::endpoint::endpoint_base(&provider.base_url)?;
    Ok(DirectTarget {
        route_id: route.id.clone(),
        base_url: base.strip_suffix("/v1").unwrap_or(&base).to_string(),
        key,
    })
}

/// 按库里存的那条线算出直连目标。没选直连就是 `None`；选了但那条线现在不合格就报错（不悄悄退回本机路由 ——
/// 使用者以为在直连，实际却经本机路由、走的是另一条线，那才说不清）。
pub fn resolve(db: &Repository) -> Result<Option<DirectTarget>> {
    let Some(id) = chosen(db) else {
        return Ok(None);
    };
    let route: Route = db.get("station_routes", &id).map_err(|_| {
        GateError::Other(
            "桌面端直连的那条线不在了：到中转站的桌面端那一页关掉直连，或者换一条".into(),
        )
    })?;
    check(db, &route)
        .map(Some)
        .map_err(|e| GateError::Other(format!("桌面端直连的那条线现在用不了：{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(db: &Repository) -> Route {
        db.put(
            "providers",
            "site",
            &Provider {
                id: "site".into(),
                name: "Fixture".into(),
                base_url: "https://fixture.invalid/v1".into(),
                website: String::new(),
                note: String::new(),
                tags: vec![],
                favorite: false,
                revision: 1,
                topup_per_usd: None,
            },
        )
        .unwrap();
        db.set_credential(
            &Credential {
                id: "k".into(),
                provider_id: "site".into(),
                label: "k".into(),
                masked: "***".into(),
                available: true,
                revision: 1,
            },
            &crate::secret::seal("fixture-real-key").unwrap(),
        )
        .unwrap();
        let mut r = Route::new(Client::ClaudeDesktop, "site", "g");
        r.credential_id = Some("k".into());
        db.put("station_routes", &r.id, &r).unwrap();
        r
    }

    #[test]
    fn only_a_line_that_needs_no_router_can_go_direct() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        let r = seed(&db);

        let t = check(&db, &r).unwrap();
        assert_eq!(t.base_url, "https://fixture.invalid", "不带 /v1");
        assert_eq!(t.key, "fixture-real-key", "直连写的是真 Key");

        let mut mapped = r.clone();
        mapped.model_map.opus = Some("glm-5".into());
        assert!(check(&db, &mapped)
            .unwrap_err()
            .to_string()
            .contains("模型映射"));
        let mut chat = r.clone();
        chat.upstream_format = Some(UpstreamFormat::OpenaiChat);
        assert!(check(&db, &chat).is_err());
        let mut no_anthropic = r.clone();
        no_anthropic.protocols.anthropic = Some(false);
        assert!(check(&db, &no_anthropic).is_err());
        let mut code = r.clone();
        code.client = Client::ClaudeCode;
        assert!(check(&db, &code).is_err());
        let mut bare = r.clone();
        bare.credential_id = None;
        assert!(check(&db, &bare).is_err());

        assert_eq!(resolve(&db).unwrap(), None, "没选就是经本机路由");
        db.set_meta(META_KEY, &r.id).unwrap();
        assert_eq!(resolve(&db).unwrap().unwrap().route_id, r.id);
        db.set_meta(META_KEY, "gone").unwrap();
        assert!(
            resolve(&db).is_err(),
            "选的线不在了要报错，不悄悄退回本机路由"
        );

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
