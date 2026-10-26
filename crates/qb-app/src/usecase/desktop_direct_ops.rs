//! Claude 桌面端直连档的开关（中转站重构阶段 4，cc-switch 的 C5；使用者 2026-10-26 定的：写真 Key）。
//!
//! 直连谁、合不合格在 `crate::desktop_direct`；这里管「开 / 关 / 换」：校验、记进库、按新设定重写桌面端那份配置。
//! 重写失败就把库里的设定退回去再写一次，不留下「库里说直连、文件还指着本机路由」的半截状态。
//!
//! ⛔ 开了之后桌面端的配置里是**真 Key**（明文）；关掉时那一格换回占位串。只在使用者点的时候改，
//! 改完要重开桌面端才生效（它只在启动时读这份配置）。

use crate::desktop_direct::{self, META_KEY};
use crate::domain::Client;
use crate::error::{GateError, Result};
use crate::repository::Repository;
use qb_station::station::route::Route;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 桌面端的一条线能不能直连。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DirectCandidate {
    pub route_id: String,
    /// 不能直连的原因；`None` = 能。
    pub problem: Option<String>,
}

/// 桌面端现在怎么接：经本机路由，还是直连某条线。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DesktopDirectView {
    /// 直连的那条线；`None` = 经本机路由。
    pub route_id: Option<String>,
    /// 直连时桌面端连的地址（不带 `/v1`）。
    pub base_url: Option<String>,
    /// 选了直连、但那条线现在用不了（被删、加了模型映射……）。这时桌面端起不来，界面要说清楚。
    pub problem: Option<String>,
    pub candidates: Vec<DirectCandidate>,
}

/// 现在的设定和每条桌面端线路能不能直连。只读。
pub fn status(db: &Repository) -> Result<DesktopDirectView> {
    let routes: Vec<Route> = db.list("station_routes").unwrap_or_default();
    let candidates = routes
        .iter()
        .filter(|r| r.client == Client::ClaudeDesktop)
        .map(|r| DirectCandidate {
            route_id: r.id.clone(),
            problem: desktop_direct::check(db, r).err().map(|e| e.to_string()),
        })
        .collect();
    let (base_url, problem) = match desktop_direct::resolve(db) {
        Ok(t) => (t.map(|t| t.base_url), None),
        Err(e) => (None, Some(e.to_string())),
    };
    Ok(DesktopDirectView {
        route_id: desktop_direct::chosen(db),
        base_url,
        problem,
        candidates,
    })
}

/// 开直连（`Some(线路)`）或关掉（`None`），然后按新设定重写桌面端那份配置。
pub fn set(db: &Repository, route_id: Option<&str>) -> Result<DesktopDirectView> {
    set_with(db, route_id, reapply)?;
    crate::audit::write(&match route_id {
        Some(id) => format!("桌面端改为直连线路 {id}（站点地址和真 Key 写进了桌面端配置）"),
        None => "桌面端改回经本机路由（配置里换回占位 Key）".to_string(),
    });
    status(db)
}

/// 按库里的设定重写桌面端中转环境的配置。
pub fn reapply() -> Result<()> {
    let env = crate::workspace::router_environment(Client::ClaudeDesktop)?;
    crate::workspace::apply(&env.id).map(drop)
}

fn set_with(
    db: &Repository,
    route_id: Option<&str>,
    reapply: impl Fn() -> Result<()>,
) -> Result<()> {
    if let Some(id) = route_id {
        let route: Route = db
            .get("station_routes", id)
            .map_err(|_| GateError::Other("这条线不在了".into()))?;
        desktop_direct::check(db, &route)?;
        if super::station_ops::load_router_settings(db, Client::ClaudeDesktop)?
            .aggregate
            .enabled
        {
            return Err(GateError::Other(
                "桌面端开着聚合模型：聚合靠本机路由，直连就没了。先在「路由设置」里关掉聚合".into(),
            ));
        }
    }
    let previous = desktop_direct::chosen(db);
    db.set_meta(META_KEY, route_id.unwrap_or(""))?;
    if let Err(e) = reapply() {
        db.set_meta(META_KEY, previous.as_deref().unwrap_or(""))?;
        // 退回去之后再写一次；这一次也失败的话，原来那个错更有用，照原样报。
        let _ = reapply();
        return Err(e);
    }
    Ok(())
}

/// 存线路之前问一句：这条线是直连的那条吗？是的话改完还得合格（不然桌面端下次起不来）。
/// 返回 `true` = 存完要重写桌面端配置（Key 换了、站点换了都要跟上）。
pub fn guard_route_save(db: &Repository, route: &Route, previous_id: Option<&str>) -> Result<bool> {
    let Some(chosen) = desktop_direct::chosen(db) else {
        return Ok(false);
    };
    if chosen != route.id && previous_id != Some(chosen.as_str()) {
        return Ok(false);
    }
    desktop_direct::check(db, route).map_err(|e| {
        GateError::Other(format!(
            "这条线是桌面端直连的那条，这样改完就直连不了了（{e}）。先在桌面端那一页关掉直连再改"
        ))
    })?;
    if chosen != route.id {
        db.set_meta(META_KEY, &route.id)?; // 改了名（分组）：跟着新 id 走。
    }
    Ok(true)
}

/// 删线路之前：删的是直连那条就先关掉直连（桌面端配置换回本机路由），免得桌面端指着一条不存在的线。
pub fn before_route_removed(db: &Repository, route_id: &str) -> Result<()> {
    if desktop_direct::chosen(db).as_deref() == Some(route_id) {
        set(db, None)?;
    }
    Ok(())
}

/// 存路由设置之前：桌面端直连时不许开聚合（聚合靠本机路由）。
pub fn guard_aggregate(db: &Repository, client: Client, enabling: bool) -> Result<()> {
    if client == Client::ClaudeDesktop && enabling && desktop_direct::chosen(db).is_some() {
        return Err(GateError::Other(
            "桌面端现在直连一条线、不经本机路由，聚合用不上。先在桌面端那一页关掉直连".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Credential, Provider};
    use std::cell::Cell;

    #[test]
    fn switching_writes_the_setting_and_rolls_back_when_the_rewrite_fails() {
        let root = std::env::temp_dir().join(crate::config_io::id());
        let db = Repository::open_in(&root).unwrap();
        db.put(
            "providers",
            "site",
            &Provider {
                id: "site".into(),
                name: "Fixture".into(),
                base_url: "https://fixture.invalid".into(),
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

        let calls = Cell::new(0);
        set_with(&db, Some(&r.id), || {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .unwrap();
        assert_eq!(calls.get(), 1, "开了就重写一次桌面端配置");
        assert_eq!(desktop_direct::chosen(&db).as_deref(), Some(r.id.as_str()));
        let view = status(&db).unwrap();
        assert_eq!(view.base_url.as_deref(), Some("https://fixture.invalid"));
        assert!(view.candidates.iter().all(|c| c.problem.is_none()));

        // 重写失败：库里退回原来的设定。
        let err = set_with(&db, None, || Err(GateError::Other("写不进去".into()))).unwrap_err();
        assert!(err.to_string().contains("写不进去"));
        assert_eq!(desktop_direct::chosen(&db).as_deref(), Some(r.id.as_str()));

        // 直连那条线改成不合格的样子：存之前就拦下。
        let mut mapped = r.clone();
        mapped.model_map.opus = Some("glm-5".into());
        assert!(guard_route_save(&db, &mapped, None).is_err());
        assert!(
            guard_route_save(&db, &r, None).unwrap(),
            "合格的改动要重写配置"
        );
        assert!(guard_aggregate(&db, Client::ClaudeDesktop, true).is_err());
        assert!(guard_aggregate(&db, Client::ClaudeCode, true).is_ok());

        set_with(&db, None, || Ok(())).unwrap();
        assert_eq!(desktop_direct::chosen(&db), None);

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
