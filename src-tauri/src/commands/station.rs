//! 中转站命令：本机路由的启停、线路池、请求日志、健康度与排序。
//!
//! **这一层只做参数搬运。** 判定在 `qb-station`,编排在 `qb-app::usecase::station_ops`。
//!
//! # ⛔ Key 绝不出这一层
//!
//! 明文 Key 只在**装配上游**时从线路绑定的 SQLite 凭证中解密,塞进
//! `local_router::Upstream` 就留在后端。所有返回给前端的形状里都**没有**
//! 能装下 Key 的字段 —— 跟 `ProviderView` 一样,不是靠自觉,是结构上写不出来。

use crate::app::AppState;
use crate::error::{GateError, Result};
use qb_app::local_router::{self, RouterConfig, Upstream, UpstreamAuth};
use qb_app::usecase::station_billing::{self, StationBillingSettings};
use qb_app::usecase::station_ops;
use qb_contract::domain::Client;
use qb_station::router::{ListenScope, RouterNetwork, RouterRequest};
use qb_station::schedule::{CheapBasis, Prefs, Ranking, Schedule};
use qb_station::station::audit::AuditRound;
use qb_station::station::route::Route;
use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

/// 本机路由现在什么样。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RouterStatus {
    pub running: bool,
    /// 客户端该把 base_url 指到哪。`None` = 还没启动。
    pub base_url: Option<String>,
    /// **这个软件**当前的上游线路 id。
    ///
    /// 三个软件各走各的上游 —— 只给一个的话，界面上「正在路由」那一行
    /// 在另外两个分页上会显示成空的，而实际它们各自有线在跑。
    pub current_route: Option<String>,
    /// 三个软件各自的当前上游（键是 kebab-case 的软件名）。
    pub current_by_client: std::collections::BTreeMap<String, String>,
    /// 这个软件排队等下一发请求生效的那条。
    pub pending_route: Option<String>,
    /// 换过几次上游。迟滞有没有在起作用,看这个数。
    pub switches: u32,
    /// 还没落库的请求日志条数。
    pub pending_logs: u32,
    /// 官方 Codex turn-state 上游挂上了没有（Codex 接进路由了没）。
    ///
    /// **Codex 全局,与界面停在哪个分页无关** —— 官方线整机只有一条。
    /// turn-state 面板据此显示「已接入 / 未接入」。
    pub turnstate_official_armed: bool,
    /// turn-state 注入总开关现在开着没有（**后端真实状态**,界面的开关照它显示）。
    ///
    /// 只在使用者手动开启后为真;面板重启后回到关（不落盘）—— 这是「默认关闭」
    /// 那条硬约束的一部分,不是遗漏。
    pub turnstate_enabled: bool,
    /// 账号规则是不是 Team（12 块 / 332）;否则个人（10 块 / 292）。
    pub turnstate_team: bool,
    /// 识别正作用于哪个 Codex 账户槽位（槽位名）。`None` = 识别关着。
    ///
    /// 0.24.7 起识别**直接作用于激活槽位**（改它的 `config.toml`，不复制凭证、不另起
    /// Codex），落盘的 marker 是唯一真相 —— 面板重启后 `turnstate_official_armed` 归零，
    /// 而这个字段还能告诉界面「上次没关干净」（启动时会自动关闭并恢复）。
    pub turnstate_takeover: Option<String>,
    /// **这个软件**的线上，熔断器不在「正常」那一档的（熔断中 / 半开）。正常的不列。
    ///
    /// 界面据此写「熔断中 · 还要几分钟」，旁边给一个「立刻重测」（充了值之后用）。
    /// 跟转发时用的是同一份熔断器（`RouterState::breaker_views`），不另算。
    pub breakers: Vec<BreakerView>,
}

/// 一条线的熔断器此刻在哪一档。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BreakerView {
    pub route_id: String,
    pub state: BreakerPhase,
    /// 熔断到什么时候（毫秒）。半开时是已经过去的那个时刻。
    pub open_until_ms: Option<i64>,
}

/// 熔断器不在「正常」时的两档。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum BreakerPhase {
    /// 熔断中：除非所有线都熔断着，否则不打它。
    Open,
    /// 到期了：下一发就是回探（只放一发）。
    HalfOpen,
}

/// `client` 是「界面现在停在哪个分页」。`current_route` / `pending_route`
/// 回的是**那个软件**的上游；三个软件的全貌在 `current_by_client` 里。
fn status_of(state: &AppState, client: Client) -> RouterStatus {
    let addr = state.station_addr.lock().ok().and_then(|a| *a);
    let s = state.station.lock().ok();
    let now = chrono::Utc::now().timestamp_millis();
    let prefix = format!("{}\u{1f}", client_key(client));
    let breakers = s
        .as_ref()
        .map(|s| {
            s.breaker_views(now)
                .into_iter()
                .filter(|(id, _, _)| id.starts_with(&prefix))
                .filter_map(|(route_id, st, open_until_ms)| {
                    let state = match st {
                        qb_station::schedule::State::Closed => return None,
                        qb_station::schedule::State::Open => BreakerPhase::Open,
                        qb_station::schedule::State::HalfOpen => BreakerPhase::HalfOpen,
                    };
                    Some(BreakerView {
                        route_id,
                        state,
                        open_until_ms,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    RouterStatus {
        breakers,
        running: addr.is_some(),
        base_url: addr.map(|a| format!("http://127.0.0.1:{}", a.port())),
        current_route: s.as_ref().and_then(|s| s.current(client)),
        current_by_client: s
            .as_ref()
            .map(|s| {
                s.currents()
                    .into_iter()
                    .map(|(c, id)| (client_key(c).to_string(), id))
                    .collect()
            })
            .unwrap_or_default(),
        pending_route: s.as_ref().and_then(|s| s.pending(client)),
        switches: s.as_ref().map(|s| s.switches() as u32).unwrap_or(0),
        pending_logs: s.as_ref().map(|s| s.pending_logs() as u32).unwrap_or(0),
        turnstate_official_armed: s.as_ref().is_some_and(|s| s.official_codex_armed()),
        turnstate_enabled: s.as_ref().is_some_and(|s| s.turnstate_enabled()),
        turnstate_team: s
            .as_ref()
            .is_some_and(|s| s.turnstate_kind() == local_router::TurnKind::Team),
        turnstate_takeover: qb_app::usecase::turnstate_ops::takeover().map(|t| t.slot_label),
    }
}

/// 三个软件。**顺序固定** —— 界面上那三个分页照这个顺序排，
/// 而排序一变，截图对比就会红一次而没人说得清为什么变了。
const ALL_CLIENTS: &[Client] = &[Client::ClaudeCode, Client::ClaudeDesktop, Client::Codex];

/// 软件名在 IPC 上的写法。跟 `Client` 的 serde kebab-case **是同一个字符串** ——
/// 另写一份映射就会漂，而漂了之后前端按软件取当前上游会静默取不到。
fn client_key(c: Client) -> &'static str {
    match c {
        Client::ClaudeCode => "claude-code",
        Client::ClaudeDesktop => "claude-desktop",
        Client::Codex => "codex",
        // 反重力没有中转路径，不在 `ALL_CLIENTS` 里；给个名字只是让 match 完整。
        Client::Antigravity => "antigravity",
        Client::AntigravityIde => "antigravity-ide",
    }
}

/// 把 SQLite 站点与每条线路绑定的凭证装配成可转发的上游。
///
/// **明文 Key 在这里取,取完就留在后端。**
fn load_upstreams(state: &AppState) -> Result<usize> {
    let db = crate::repository::Repository::open()?;
    let routes: Vec<Route> = db.list("station_routes")?;
    let upstreams: Vec<_> = routes
        .iter()
        .filter_map(|route| {
            let source = station_ops::source(&db, route).ok()?;
            Some(
                Upstream {
                    route_id: source.route_id,
                    base_url: source.base_url,
                    auth: if route.client == Client::Codex {
                        UpstreamAuth::Bearer(source.key)
                    } else {
                        UpstreamAuth::ApiKey(source.key)
                    },
                    ..Default::default()
                }
                // 这条线自己的模型映射（2026-10-24 起）。`upstream_format` 阶段 1 还不读：只有原样转发。
                .with_model_map(&route.model_map),
            )
        })
        .collect();
    let n = upstreams.len();
    state
        .station
        .lock()
        .map_err(|_| GateError::Other("中转站状态损坏".into()))?
        .replace_upstreams(upstreams);
    Ok(n)
}

/// 请求日志多久落一次库。几秒的延迟对「看日志」无所谓（打开日志页时会先把手里的写掉），
/// 又不至于每发请求都开一次库。
const LOG_FLUSH_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// 请求日志落库的那一头：真库 + 落库时按价补两种花费。
struct DbLogSink;

impl local_router::LogSink for DbLogSink {
    fn write(&self, batch: Vec<RouterRequest>) -> qb_app::error::Result<()> {
        let db = crate::repository::Repository::open()?;
        let catalog = qb_station::station::pricing::Catalog::new(stored_prices());
        station_ops::persist_router_requests(
            &db,
            batch,
            &catalog,
            chrono::Utc::now().timestamp_millis(),
        )
    }
}

/// 拉起本机路由。已经起着就原样返回 —— **重复调不该换端口**,
/// 客户端那头的地址是照着上一次写进去的。
///
/// 不是命令:路由只在 `station_launch` / 开启识别时顺带拉起,面板退出时跟着停。
/// 原来的 `station_router_start` / `station_router_stop` 两条命令一直没有界面,
/// 2026-09-27 删了,只留这个助手。`client` 只影响回来的 `current_route` 显示哪一个,
/// 不影响路由本身:三个软件共用同一个监听端口。
async fn ensure_router(app: &tauri::AppHandle, client: Client) -> Result<RouterStatus> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    if state.station_addr.lock().ok().and_then(|a| *a).is_some() {
        return Ok(status_of(&state, client));
    }
    load_upstreams(&state)?;
    let shared = std::sync::Arc::clone(&state.station);
    // 只配连接超时:上游连不上时尽快失败并如实判健康,而**不设读/总超时** ——
    // SSE 生成可以合法地跑很久,总超时会把长回复(含 Claude 的)拦腰截断。
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    // 监听在哪、走不走出站代理（R6）。默认只本机、直连 —— 跟原来一模一样。
    let net = station_ops::load_router_network(&crate::repository::Repository::open()?)?;
    if let Ok(mut s) = state.station.lock() {
        s.set_relay_http(station_ops::relay_client(&net)?);
    }
    let config = match net.scope {
        ListenScope::Loopback => RouterConfig::loopback(local_router::DEFAULT_PORT),
        ListenScope::AllInterfaces => {
            RouterConfig::open(net.listen(), local_router::DEFAULT_PORT, net.key.clone())
        }
    };
    let mut handle = local_router::serve(config, shared, http).await?;
    // 请求日志后台批量落库（原来只在打开日志页时才落，没打开时内存里一直涨）。
    handle.write_logs_to(std::sync::Arc::new(DbLogSink), LOG_FLUSH_EVERY);
    if let Ok(mut a) = state.station_addr.lock() {
        *a = Some(handle.addr);
    }
    if let Ok(mut h) = state.station_router.lock() {
        *h = Some(handle);
    }
    Ok(status_of(&state, client))
}

/// 路由起着的话停掉再按现在的网络设置起一次（换了监听范围 / 换了口令之后）。没起着就什么都不做。
///
/// 停掉时在飞的请求照样跑完（`RouterHandle::stop`）；新的监听要等旧的那个真的放开端口，
/// 所以绑不上时隔一会儿再试，最多两秒。
async fn restart_router(app: &tauri::AppHandle) -> Result<()> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    let old = state.station_router.lock().ok().and_then(|mut h| h.take());
    let Some(old) = old else {
        return Ok(());
    };
    old.stop();
    drop(old);
    if let Ok(mut a) = state.station_addr.lock() {
        *a = None;
    }
    let mut last = None;
    for _ in 0..20 {
        match ensure_router(app, Client::ClaudeCode).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                last = Some(e);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
    Err(last.unwrap_or_else(|| GateError::Other("本机路由重启失败".into())))
}

#[tauri::command]
pub fn station_router_status(state: State<'_, AppState>, client: Client) -> RouterStatus {
    status_of(&state, client)
}

/// 配置官方 Codex 的 turn-state 注入。**实验功能,默认关闭。只对 Codex,不碰 Claude。**
///
/// `enabled` = 注入总开关;`team` = 账号规则(true → Team 12 块,false → 个人 10 块)。
/// 关着时仍会被动采集与展示,只是不往请求里塞。采集是免费的(读官方响应带回的头),
/// 不额外发探测请求、不换出口、不消耗多余额度。
#[tauri::command]
pub fn station_turnstate_configure(
    state: State<'_, AppState>,
    enabled: bool,
    team: bool,
) -> Result<()> {
    let kind = if team {
        local_router::TurnKind::Team
    } else {
        local_router::TurnKind::Personal
    };
    state
        .station
        .lock()
        .map_err(|_| GateError::Other("中转站状态损坏".into()))?
        .turnstate_configure(enabled, kind);
    Ok(())
}

/// 每个 model 当前的 turn-state 外形（块数、长度、剩余秒、strikes、观测次数）。
/// **不含 turn-state 的值本身。**
#[tauri::command]
pub fn station_turnstate_status(
    state: State<'_, AppState>,
) -> Result<Vec<qb_station::turnstate::ModelStatus>> {
    let now = chrono::Utc::now().timestamp_millis();
    Ok(state
        .station
        .lock()
        .map_err(|_| GateError::Other("中转站状态损坏".into()))?
        .turnstate_status(now))
}

/// 开启识别：把当前激活的 Codex 账户槽位接进本机路由。**只 Codex。**
///
/// 0.24.7 起**不再另建环境目录、不再复制 OAuth、不再另起 Codex**（老做法留下两份轮换式
/// 刷新令牌，谁先刷新另一份就作废 —— 档案 §7.29 同款隐患）。现在三步：
/// 1. 在路由里挂上官方上游（`OAuthPassthrough`）并拉起路由 —— 先做这步，路由起不来就
///    不去动槽位配置；
/// 2. `usecase::turnstate_ops::enable`：备份并增量改槽位的 `config.toml`
///    （`model_provider` → `qb_turnstate`，base 不带 `/v1`），写 marker；失败就把上游摘回去；
/// 3. 之后使用者照常从账户页启动那个槽位的 Codex —— 它读到的就是走路由的配置。
///    已经开着的 Codex 只在启动时读一次配置，**要重启才生效**，界面上说明。
///
/// ⛔ 与出站插件（ccodex）互斥，双向。两边**都要改同一个槽位的 `config.toml`**，
/// 同时开就是两个程序抢同一个文件，谁赢都说不清。
#[tauri::command]
pub async fn station_turnstate_enable(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<RouterStatus> {
    let _guard = crate::operations::exclusive().await?;
    let client = Client::Codex;
    if crate::plugins::codex_egress::running() {
        return Err(GateError::Other(
            "出站插件（ccodex）正在运行。它和识别都要改同一个槽位的 Codex 配置，一次只能开一个：先到扩展中心停掉插件（会恢复 Codex 配置），再开启识别。"
                .into(),
        ));
    }
    // 路由先起：官方上游挂上并选中（load_upstreams 会保住它）。
    {
        let mut s = state
            .station
            .lock()
            .map_err(|_| GateError::Other("中转站状态损坏".into()))?;
        s.arm_official_codex();
    }
    load_upstreams(&state)?;
    ensure_router(&app, client).await?;
    // 官方 base **不带 `/v1`**：官方端点是 `.../backend-api/codex/responses`，
    // `client_base_url(Codex, …)` 给的 `http://127.0.0.1:15721/codex` 正好对上。
    let base = local_router::client_base_url(client, local_router::DEFAULT_PORT);
    if let Err(e) =
        tokio::task::spawn_blocking(move || qb_app::usecase::turnstate_ops::enable(&base))
            .await
            .map_err(|e| GateError::Other(e.to_string()))?
    {
        if let Ok(mut s) = state.station.lock() {
            s.disarm_official_codex();
        }
        return Err(e);
    }
    crate::operations::changed(&crate::events::ui(&app), "turnstate", "enabled");
    Ok(status_of(&state, client))
}

/// 关闭识别：摘掉官方上游，并按 marker 把那个槽位的 `config.toml` 反向恢复。**只 Codex。**
///
/// 不动正在跑的那个 Codex（它接下来的请求会如实收到 503「还没选上游」，界面提示重启）。
/// 恢复失败时 marker 保留，报错让使用者再点一次 —— 不能让「路由摘了、配置还指着路由」
/// 这种半成品悄悄留下。
#[tauri::command]
pub async fn station_turnstate_disable(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<RouterStatus> {
    let _guard = crate::operations::exclusive().await?;
    if let Ok(mut s) = state.station.lock() {
        s.disarm_official_codex();
    }
    tokio::task::spawn_blocking(qb_app::usecase::turnstate_ops::disable)
        .await
        .map_err(|e| GateError::Other(e.to_string()))??;
    crate::operations::changed(&crate::events::ui(&app), "turnstate", "disabled");
    Ok(status_of(&state, Client::Codex))
}

/// 选一条上游。
///
/// `immediate` 为真时立刻落定(使用者手动指定);否则排队,
/// **下一发请求才生效** —— 换上游绝不许影响正在飞的请求。
#[tauri::command]
pub fn station_select_route(
    state: State<'_, AppState>,
    route_id: String,
    immediate: bool,
) -> Result<RouterStatus> {
    // ⛔ 切哪个软件的上游，由**线路自己**说了算，不由界面停在哪个分页说了算。
    //
    // 拿分页当依据的话，只要界面状态和线路对不上（切完分页还没重渲染、
    // 深链接直接打进来），就会把 A 软件的上游换成一条属于 B 软件的线 ——
    // 那条线的 Key 是给 B 配的，账会记到 B 头上，而且完全看不出来。
    let db = crate::repository::Repository::open()?;
    let route: Route = db.get("station_routes", &route_id)?;
    {
        let mut s = state
            .station
            .lock()
            .map_err(|_| GateError::Other("中转站状态损坏".into()))?;
        if immediate {
            s.set_now(route.client, route_id.clone());
        } else {
            s.request_switch(route.client, route_id.clone());
        }
    }
    // 记下来：面板重启后接着用这一条（原来只在内存里，重启后客户端收到 503「还没选上游」）。
    // 写不进去不挡这一次切换 —— 切换已经生效了，只是重启后不记得；记进审计日志。
    if let Err(e) = station_ops::remember_current(
        &db,
        route.client,
        &route_id,
        chrono::Utc::now().timestamp_millis(),
    ) {
        crate::audit::write(&format!(
            "中转站：已切到「{route_id}」，但没记下来（面板重启后不会接着用它）：{e}"
        ));
    }
    Ok(status_of(&state, route.client))
}

// ------------------------------------------------------------------ 本机路由的设置与派单顺序

/// 一个软件的本机路由设置（故障转移、三种超时、熔断参数、整流器开关）。读的是库里那份 —— 库是真相。
#[tauri::command]
pub fn station_router_settings(client: Client) -> Result<qb_station::router::RouterSettings> {
    let db = crate::repository::Repository::open()?;
    station_ops::load_router_settings(&db, client)
}

/// 存一个软件的本机路由设置，并立刻换到路由器上（下一发请求就按新的来；熔断器的当前状态不动）。
#[tauri::command]
pub fn station_router_settings_save(
    state: State<'_, AppState>,
    settings: qb_station::router::RouterSettings,
) -> Result<qb_station::router::RouterSettings> {
    if !station_ops::RELAY_CLIENTS.contains(&settings.client) {
        return Err(GateError::Other(
            "这个软件没有中转路径，本机路由不给它换上游".into(),
        ));
    }
    let db = crate::repository::Repository::open()?;
    station_ops::save_router_settings(&db, &settings)?;
    state
        .station
        .lock()
        .map_err(|_| GateError::Other("中转站状态损坏".into()))?
        .set_settings(settings.clone());
    Ok(settings)
}

/// 一个软件的派单队列：上次选中的那条 + 手动排的顺序。
#[tauri::command]
pub fn station_dispatch_queue(client: Client) -> Result<qb_station::router::DispatchQueue> {
    let db = crate::repository::Repository::open()?;
    station_ops::load_dispatch_queue(&db, client)
}

/// 存派单顺序：当前那条失败时依次改用谁。只收这个软件自己的线。
#[tauri::command]
pub fn station_dispatch_queue_save(
    state: State<'_, AppState>,
    client: Client,
    order: Vec<String>,
) -> Result<qb_station::router::DispatchQueue> {
    let db = crate::repository::Repository::open()?;
    let q = station_ops::save_dispatch_order(
        &db,
        client,
        order,
        chrono::Utc::now().timestamp_millis(),
    )?;
    state
        .station
        .lock()
        .map_err(|_| GateError::Other("中转站状态损坏".into()))?
        .set_queue(client, q.order.clone());
    Ok(q)
}

/// 「我充值了，立刻重测」：熔断中的线马上转半开，下一发真实请求就是回探（不花额外的钱）。
/// 返回它原来是不是熔断着。
#[tauri::command]
pub fn station_route_retest(state: State<'_, AppState>, route_id: String) -> Result<bool> {
    let now = chrono::Utc::now().timestamp_millis();
    Ok(state
        .station
        .lock()
        .map_err(|_| GateError::Other("中转站状态损坏".into()))?
        .retest(&route_id, now))
}

// ------------------------------------------------------------------ 本机路由的网络设置（R6）

/// 本机路由的网络设置给界面看的那一份。**出站代理去掉了用户名密码**；口令只在开到所有网卡上时给
/// （使用者要把它抄到别的机器上去）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RouterNetworkView {
    pub scope: ListenScope,
    /// 路由口令。只本机时是 `None`（用不上，也不显示）。
    pub key: Option<String>,
    /// 出站代理（`协议://主机:端口`，没有用户名密码）。`None` = 直连。
    pub outbound_proxy: Option<String>,
    /// 监听端口（别的机器连 `本机 IP:这个端口`）。
    pub port: u16,
}

fn network_view(net: &RouterNetwork) -> RouterNetworkView {
    RouterNetworkView {
        scope: net.scope,
        key: (net.scope == ListenScope::AllInterfaces).then(|| net.key.clone()),
        outbound_proxy: net
            .outbound_proxy
            .as_deref()
            .map(qb_station::router::mask_proxy_url),
        port: local_router::DEFAULT_PORT,
    }
}

#[tauri::command]
pub fn station_router_network() -> Result<RouterNetworkView> {
    let db = crate::repository::Repository::open()?;
    Ok(network_view(&station_ops::load_router_network(&db)?))
}

/// 改监听范围。开到所有网卡上时还没有口令就先生成一个。路由起着的话按新的重启一次
/// （在飞的请求照样跑完）。**开到所有网卡上时 Windows 防火墙会弹窗问一次**，界面上提前写着。
#[tauri::command]
pub async fn station_router_listen_save(
    app: tauri::AppHandle,
    scope: ListenScope,
) -> Result<RouterNetworkView> {
    let net = {
        let db = crate::repository::Repository::open()?;
        let mut net = station_ops::load_router_network(&db)?;
        net.scope = scope;
        if scope == ListenScope::AllInterfaces && net.key.trim().is_empty() {
            net.key = station_ops::new_router_key();
        }
        station_ops::save_router_network(&db, &net)?;
        net
    };
    restart_router(&app).await?;
    crate::audit::write(&format!(
        "本机路由的监听范围改成了「{}」",
        match scope {
            ListenScope::Loopback => "只本机",
            ListenScope::AllInterfaces => "所有网卡（别的机器要带路由口令）",
        }
    ));
    Ok(network_view(&net))
}

/// 换一个新的路由口令（旧的立刻失效：别的机器上的客户端要改成新的）。
#[tauri::command]
pub async fn station_router_key_rotate(app: tauri::AppHandle) -> Result<RouterNetworkView> {
    let net = {
        let db = crate::repository::Repository::open()?;
        let mut net = station_ops::load_router_network(&db)?;
        net.key = station_ops::new_router_key();
        station_ops::save_router_network(&db, &net)?;
        net
    };
    if net.scope == ListenScope::AllInterfaces {
        restart_router(&app).await?;
    }
    Ok(network_view(&net))
}

/// 设 / 清中转线路的出站代理（`None` 或空串 = 清掉，直连）。下一发请求就按新的走，不用重启路由。
/// ⛔ 只给中转线路；官方 Codex 那条线永远直连。
#[tauri::command]
pub fn station_outbound_proxy_save(
    state: State<'_, AppState>,
    url: Option<String>,
) -> Result<RouterNetworkView> {
    let db = crate::repository::Repository::open()?;
    let mut net = station_ops::load_router_network(&db)?;
    net.outbound_proxy = url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
    station_ops::save_router_network(&db, &net)?;
    let client = station_ops::relay_client(&net)?;
    state
        .station
        .lock()
        .map_err(|_| GateError::Other("中转站状态损坏".into()))?
        .set_relay_http(client);
    Ok(network_view(&net))
}

/// 经出站代理试连这个软件的第一条线（只在点「测试」时跑）。`url` 不给就试存着的那个。回耗时（毫秒）。
#[tauri::command]
pub async fn station_outbound_proxy_test(client: Client, url: Option<String>) -> Result<u64> {
    let (proxy, target) = {
        let db = crate::repository::Repository::open()?;
        let proxy = match url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty()) {
            Some(u) => u,
            None => station_ops::load_router_network(&db)?
                .outbound_proxy
                .ok_or_else(|| GateError::Other("还没设出站代理".into()))?,
        };
        let routes: Vec<Route> = db.list("station_routes")?;
        let target = routes
            .iter()
            .filter(|r| r.client == client)
            .find_map(|r| station_ops::source(&db, r).ok())
            .map(|s| s.base_url)
            .ok_or_else(|| {
                GateError::Other(
                    "这个软件底下还没有能用的线路：测代理时试连的是你自己加的那条线的地址，先加一条再测。"
                        .into(),
                )
            })?;
        (proxy, target)
    };
    station_ops::test_outbound_proxy(&proxy, &target).await
}

/// 扫本机常见的代理端口（只连 127.0.0.1、不发数据），回连得上的地址。只在点「扫描」时跑。
#[tauri::command]
pub async fn station_outbound_proxy_scan() -> Result<Vec<String>> {
    tauri::async_runtime::spawn_blocking(station_ops::scan_local_proxies)
        .await
        .map_err(|e| GateError::Other(format!("扫描没跑完：{e}")))
}

/// 面板启动时：把线路装进路由器，再把落盘的设置、派单顺序、上次选中的那条装回去。
///
/// **不拉起路由**（监听端口）—— 路由什么时候起，跟原来一样由「启动」/ 开启识别决定。
/// 这里只保证路由起来之后，各软件接着用上次那条，而不是 503「还没选上游」。
pub fn restore_router_state(state: &AppState) -> Result<()> {
    load_upstreams(state)?;
    let db = crate::repository::Repository::open()?;
    let mut s = state
        .station
        .lock()
        .map_err(|_| GateError::Other("中转站状态损坏".into()))?;
    station_ops::restore_router(&db, &mut s)
}

/// 启动这个软件,走当前选中的上游。**中转站页面上那个「启动」就是它。**
///
/// 三件事按顺序做,少一件都起不来:
///
/// 1. 切上游(给了 `route_id` 的话)—— 先切再起,不然客户端拿到的是上一条;
/// 2. 把本机路由拉起来 —— 客户端的 base_url 指着它,它不在就是「连不上」;
/// 3. 起客户端进程,用那个走本机路由的环境(没有就现建一个)。
///
/// # ⛔ 起的是客户端进程,不只是路由
///
/// 0.16.0 之前这个按钮只做第 2 步。使用者点完看见「运行中」,以为可以了,
/// 然后回终端敲 `claude` —— 那个 claude 的 base_url 还指着官方,
/// 整条中转链路一次都没被用到,而界面上一切正常。
///
/// # 中转会话不归门禁的关停策略管
///
/// 但它照样要**起之前验 IP、起完持租约**（`LaunchTarget::gated()`）——
/// Deny ACE 是按文件加的,不认身份;让中转绕过解锁,就等于拿中转会话
/// 把 claude.exe 解锁、再从终端起官方的。所以这里跟 `session_launch`
/// 走的是同一个 `workspace::launch`,看门狗也照样挂上。
#[tauri::command]
pub async fn station_launch(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    client: Client,
    route_id: Option<String>,
    working_dir: Option<String>,
) -> Result<qb_contract::domain::Session> {
    use tauri::Manager;
    let _guard = crate::operations::exclusive().await?;
    if let Some(id) = route_id.filter(|s| !s.trim().is_empty()) {
        // ⛔ 切哪个软件的上游由线路自己说了算 —— 同 station_select_route。
        let route: Route = crate::repository::Repository::open()?.get("station_routes", &id)?;
        if route.client != client {
            return Err(GateError::Other(
                "这条线路不属于这个软件，不能拿它启动".into(),
            ));
        }
        let mut s = state
            .station
            .lock()
            .map_err(|_| GateError::Other("中转站状态损坏".into()))?;
        s.set_now(route.client, id);
    }
    let selected_id = {
        let router = state
            .station
            .lock()
            .map_err(|_| GateError::Other("中转站状态损坏".into()))?;
        router.pending(client).or_else(|| router.current(client))
    }
    .ok_or_else(|| GateError::Other("请先选择一条中转线路".into()))?;
    let db = crate::repository::Repository::open()?;
    let route: Route = db.get("station_routes", &selected_id)?;
    station_ops::source(&db, &route)?;
    load_upstreams(&state)?;
    ensure_router(&app, client).await?;
    crate::sessions::refresh()?;
    if let Some(session) = crate::sessions::list().into_iter().find(|s| {
        s.state == "running"
            && s.context.client == client
            && s.context.identity_kind == qb_contract::domain::IdentityKind::Relay
            && s.context.identity_id == crate::workspace::router_environment_id(client)
    }) {
        return Ok(session);
    }
    let env = crate::workspace::router_environment(client)?;

    let mut task = crate::operations::Run::start(crate::events::sink(&app), "启动会话", &env.id)?;
    // 桌面端是单实例：开着的话先替使用者关掉，关干净了才起（见 close_desktop_for_relay）。
    // 进度写进操作记录 —— 中转站页面的启动按钮显示的就是这一句，使用者看得到它在关。
    if client == Client::ClaudeDesktop {
        let closed = crate::workspace::close_desktop_for_relay(&state.gate, |n| {
            let _ = task.phase(&format!("正在关闭运行中的桌面端（{n} 个进程）"), 10);
        })
        .await;
        if let Err(e) = closed {
            return task.finish(Err(e));
        }
    }
    task.phase("检查身份、配置与门禁", 20)?;
    let result = crate::workspace::launch(
        client,
        qb_contract::domain::IdentityKind::Relay,
        &env.id,
        working_dir,
        &app.state::<AppState>().gate,
    )
    .await;
    let session = task.finish(result)?;
    let target = crate::launch::LaunchTarget::of(client);
    if target.gated() {
        crate::app::start_watchdog(target.watch_mode(), &state, Some(app.clone()));
    }
    crate::operations::changed(&crate::events::ui(&app), "session", &session.id);
    Ok(session)
}

/// ⚡ 探测：这家站点支持哪几种协议、公布了哪些模型的价。
///
/// **免费。** 协议那一步发的是故意不合法的空 body（上游连模型都不碰），
/// 价目那一步读的是站点自己的 `/api/pricing`。两步都不产生 token。
///
/// # ⛔ 探不出来的留空，不猜
///
/// 协议是三态：探通 / 上游明确拒绝 / 不知道。Key 打错时三条会一起 401，
/// 那时候正确答案是「不知道」—— 猜成「都不支持」会让这条线路在界面上
/// 整个消失，而使用者只是少粘了一位。
///
/// 价目拉不到就如实说「这个站点没开 /api/pricing」，**不编一个倍率**。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProbeResult {
    pub protocols: qb_station::station::model::Protocols,
    /// 站点公布的模型表，已按分组过滤。空 = 没公布，或者这个分组没有模型。
    pub models: Vec<qb_station::station::pricing::StationModel>,
    /// 价目那一步的问题，一句话。`None` = 拉到了。
    pub pricing_problem: Option<String>,
    /// New API 在 `/api/status` 公布的在线充值价（1 美元额度付几元）。
    ///
    /// ⛔ **只当参考**，界面写在「充值比例」旁边、不自动填 —— 这一格默认 7.3，
    /// 很多站没改过、实际靠兑换码按 1 元卖。见 `station_ops::published_topup_price`。
    pub topup_hint: Option<f64>,
}

#[tauri::command]
pub async fn station_probe(
    base_url: String,
    key: Option<String>,
    group: Option<String>,
) -> Result<ProbeResult> {
    let base = qb_app::endpoint::endpoint_base(&base_url)?;
    let key = key.unwrap_or_default();
    // 探测要短超时：这一步是使用者点了之后站着等的，
    // 默认那个 30 秒会让人以为按钮坏了。
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .connect_timeout(std::time::Duration::from_secs(6))
        .build()
        .map_err(|e| GateError::Other(e.to_string()))?;
    let protocols = station_ops::probe_protocols(&base, &key, &client).await;
    let group = group.unwrap_or_default();
    let all = if key.trim().is_empty() {
        Vec::new()
    } else {
        station_ops::fetch_station_models(
            &station_ops::BillingSource {
                route_id: String::new(),
                base_url: base.clone(),
                key: key.clone(),
                // 带上分组：`/api/pricing` 顶层的分组倍率按它取。
                group: group.clone(),
                ..Default::default()
            },
            &client,
        )
        .await
    };
    // 不要 Key：`/api/status` 是公开的。
    let topup_hint = station_ops::published_topup_price(&base, &client).await;
    let models: Vec<_> = all.into_iter().filter(|m| m.in_group(&group)).collect();
    let pricing_problem = if key.trim().is_empty() {
        Some("还没填 Key —— 价目表要带着 Key 才拉得到".to_string())
    } else if models.is_empty() {
        Some("这个站点没公布价目表（/api/pricing），或者这个分组下没有模型".to_string())
    } else {
        None
    };
    Ok(ProbeResult {
        protocols,
        models,
        pricing_problem,
        topup_hint,
    })
}

/// 这个软件走本机路由时那份配置文件。
///
/// # ⛔ 一个软件一份，不是一条线路一份
///
/// 换上游**不重启客户端** —— 那正是本机路由存在的理由。客户端只在启动时
/// 读一次配置，所以「这条线路的配置」这个概念根本不成立:六条线路轮着走，
/// 客户端那份 settings.json 自始至终是同一份。
///
/// 做成一条线路一份的话，界面上每条线各显示一份配置，而实际生效的永远是
/// 启动时那一条的 —— 改另外五条完全没有反应，且没有任何地方说得清为什么。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClientConfig {
    /// 文件在哪。界面要照实显示 —— 使用者有权知道改的是哪个文件。
    pub path: String,
    pub text: String,
    /// `json`（Claude Code）还是 `toml`（Codex）。界面据此决定怎么校验。
    pub format: String,
}

/// 这个软件那份配置在哪、是什么格式。
///
/// ⛔ 三个软件三份文件，**格式标签也得是三个**：
///
/// | 软件 | 文件 | 标签 | 界面据此显示什么 |
/// |---|---|---|---|
/// | Claude Code | `settings.json` | `json` | 六个快捷开关 + 模型映射 |
/// | Codex | `config.toml` | `toml` | 1M 上下文 + 思考等级 |
/// | Claude 桌面端 | `claude_desktop_config.json` | `desktop` | 只给编辑器 —— 那六个开关是 Claude Code 的环境变量，桌面端不认 |
///
/// 桌面端要是也标成 `json`，界面会把 Claude Code 那六个开关摆出来，
/// 勾了写进去一堆桌面端根本不读的键 —— 而界面上看起来一切正常。
fn client_config_path(client: Client) -> Result<(std::path::PathBuf, &'static str)> {
    let env = crate::workspace::router_environment(client)?;
    let dir = std::path::PathBuf::from(&env.config_dir);
    Ok(match client {
        Client::Codex => (dir.join("config.toml"), "toml"),
        Client::ClaudeDesktop => (dir.join(qb_app::workspace::DESKTOP_CONFIG_FILE), "desktop"),
        Client::ClaudeCode => (dir.join("settings.json"), "json"),
        // `router_environment` 在上一行已经拒绝了反重力；到不了这里。
        Client::Antigravity | Client::AntigravityIde => {
            return Err(GateError::Other("反重力没有中转路径".into()))
        }
    })
}

#[tauri::command]
pub fn station_client_config(client: Client) -> Result<ClientConfig> {
    let (path, format) = client_config_path(client)?;
    let text = crate::config_io::read_text(&path)?;
    Ok(ClientConfig {
        path: path.display().to_string(),
        // 空文件在界面上显示成 `{}` 而不是一片空白 —— 空白让人以为读失败了。
        text: if text.trim().is_empty() && format != "toml" {
            "{}".into()
        } else {
            text
        },
        format: format.into(),
    })
}

/// 存这份配置。
///
/// # ⛔ 存完立刻重新应用一次
///
/// 直接写文件会让 [`workspace::configuration_state`] 读出「配置已被外部修改」，
/// 下一次启动会被它挡下来 —— 而那句话说的是「有别的程序动过」，跟实情不符。
/// 重新应用一步把指纹对上，顺带把 base_url 那几项合并回去（使用者删掉了
/// 也能自动补上）。
#[tauri::command]
pub fn station_client_config_save(client: Client, text: String) -> Result<ClientConfig> {
    let (path, format) = client_config_path(client)?;
    if format != "toml" {
        // 存之前先校验。存一份坏 JSON 进去，客户端启动时会直接崩，
        // 而错误信息在客户端那边，面板上什么都看不到。
        crate::config_io::object(&text)?;
    } else {
        text.parse::<toml_edit::DocumentMut>()
            .map_err(|e| GateError::Other(format!("这不是合法的 TOML：{e}")))?;
    }
    let previous = crate::config_io::read_optional(&path)?;
    let env = crate::workspace::router_environment(client)?;
    crate::config_io::replace(&path, Some(text.as_bytes()))?;
    if let Err(error) = crate::workspace::apply(&env.id) {
        crate::config_io::replace(&path, previous.as_deref())?;
        return Err(error);
    }
    station_client_config(client)
}

// ------------------------------------------------------------------ 线路池

#[tauri::command]
pub fn station_routes() -> Result<Vec<Route>> {
    Ok(crate::repository::Repository::open()?
        .list("station_routes")
        .unwrap_or_default())
}

#[tauri::command]
pub fn station_put_route(
    app: tauri::AppHandle,
    route: Route,
    previous_id: Option<String>,
) -> Result<Vec<Route>> {
    use tauri::Manager;
    let db = crate::repository::Repository::open()?;
    station_ops::source(&db, &route)?;
    let renamed_from = previous_id.filter(|id| id != &route.id);
    db.transaction(|| {
        db.put("station_routes", &route.id, &route)?;
        if let Some(old) = &renamed_from {
            let prior: Route = db.get("station_routes", old)?;
            if prior.client != route.client {
                return Err(GateError::Other("不能更换线路所属的软件".into()));
            }
            db.remove("station_routes", old)?;
        }
        Ok(())
    })?;
    // 线路变了,上游表要跟着重装 —— 否则新加的线路转发时找不到 base_url。
    let state = app.state::<AppState>();
    load_upstreams(&state)?;
    if let Some(old) = renamed_from {
        let mut router = state
            .station
            .lock()
            .map_err(|_| GateError::Other("中转站状态损坏".into()))?;
        let pending = router.pending(route.client);
        if pending.as_deref() == Some(&old)
            || (pending.is_none() && router.current(route.client).as_deref() == Some(&old))
        {
            router.request_switch(route.client, &route.id);
        }
    }
    station_routes()
}

#[tauri::command]
pub fn station_remove_route(app: tauri::AppHandle, id: String) -> Result<Vec<Route>> {
    use tauri::Manager;
    crate::repository::Repository::open()?.remove("station_routes", &id)?;
    load_upstreams(&app.state::<AppState>())?;
    station_routes()
}

// ------------------------------------------------------------------ 请求日志

/// 先把手里还没落库的那批写掉（后台每几秒写一次，打开日志页时不必等它），再回最近的若干条。
///
/// 读的是 `router_requests`（迁移 5，按时间建了索引、带 LIMIT），不再整表读出来再截断。
/// 老的 `request_logs` 由迁移 5 搬了过来。
#[tauri::command]
pub fn station_logs(state: State<'_, AppState>, limit: Option<u32>) -> Result<Vec<RouterRequest>> {
    let drained = {
        let mut s = state
            .station
            .lock()
            .map_err(|_| GateError::Other("中转站状态损坏".into()))?;
        s.drain_logs()
    };
    let db = crate::repository::Repository::open()?;
    let catalog = qb_station::station::pricing::Catalog::new(stored_prices());
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e) = station_ops::persist_router_requests(&db, drained.clone(), &catalog, now) {
        // 写不进去就放回去，下一轮再写 —— 别因为写库失败把这一批弄丢。
        if let Ok(mut s) = state.station.lock() {
            s.requeue_logs(drained);
        }
        return Err(e);
    }
    station_ops::router_requests(&db, None, None, limit.unwrap_or(200))
}

// ------------------------------------------------------------------ 站点检验

/// 一轮检验连同它属于哪条线路。
///
/// 表是 `(id, body)` 的,所以线路 id 要跟着 body 走 —— 靠解析主键前缀
/// 去猜属于谁,分组名里有分隔符就会猜错。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StoredAudit {
    pub route_id: String,
    pub round: AuditRound,
}

/// 这条线的检验历史,新的在前。
#[tauri::command]
pub fn station_audits(route_id: String) -> Result<Vec<StoredAudit>> {
    let records: Vec<serde_json::Value> =
        crate::repository::Repository::open()?.list("station_audits")?;
    let all: Vec<StoredAudit> = records
        .into_iter()
        .filter(|v| v["route_id"].as_str() == Some(&route_id))
        .map(|v| {
            if let Some(sealed) = v["sealed"].as_str() {
                let raw = crate::secret::open(sealed)
                    .ok_or_else(|| GateError::Other("检验历史无法解密".into()))?;
                serde_json::from_str(&raw).map_err(Into::into)
            } else {
                serde_json::from_value(v).map_err(Into::into)
            }
        })
        .collect::<Result<_>>()?;
    let mut mine: Vec<StoredAudit> = all.into_iter().filter(|a| a.route_id == route_id).collect();
    // 新的在前：报告页那排历史按钮从左到右就是从新到旧。
    mine.sort_by_key(|a| std::cmp::Reverse(a.round.at_ms));
    Ok(mine)
}

/// 跑一轮检验。
///
/// ⛔ **会计费。** 只在使用者点的时候调 —— 面板里没有任何自动触发它的路径。
///
/// 结论会回流:量到 mult 就写回线路的 `latest_mult`,于是排序和底线立刻
/// 按真实倍率走。**但不拉黑** —— 真实倍率仍然过线的照样留在池子里。
/// 一条线路(站点 + 分组)上能用哪些模型。
///
/// 界面上「站点 → 分组 → 模型」三级里的第三级 —— 前两级就是线路本身。
///
/// # ⛔ 拉不到不是错
///
/// 站点没开 `/api/pricing`、Key 过期、网断了,都只是 `models` 为空 +
/// `problem` 写一句话。**界面照样让使用者自己填模型名** ——
/// 报成错的话,这种站一个模型都验不了。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StationModelsView {
    /// 这个分组里能用的模型,按站点公布的顺序。
    pub models: Vec<qb_station::station::pricing::StationModel>,
    /// 建议先验哪个(`claude-opus-5` / `gpt-5.6-sol`,站点有才用)。
    ///
    /// `None` = 站点一个模型都没公布,界面上要让使用者自己填。
    pub recommended: Option<String>,
    /// 没拉到的原因。**非空不代表坏了**,只是这一级没法自动列出来。
    pub problem: Option<String>,
}

/// 去站点拉这条线路能用的模型。**零成本** —— 读的是站点的价目表,不打模型。
#[tauri::command]
pub async fn station_models(route_id: String) -> Result<StationModelsView> {
    let db = crate::repository::Repository::open()?;
    let routes: Vec<Route> = db.list("station_routes").unwrap_or_default();
    let Some(route) = routes.into_iter().find(|r| r.id == route_id) else {
        return Err(GateError::Other("找不到这条线路".into()));
    };
    let source = match station_ops::source(&db, &route) {
        Ok(source) => source,
        Err(error) => {
            return Ok(StationModelsView {
                models: Vec::new(),
                recommended: None,
                problem: Some(error.to_string()),
            })
        }
    };
    let (all, problem) = station_ops::fetch_models(&source, &station_billing::http_client()?).await;

    // 分组过滤在这里做一次,前端拿到的就是这个分组能用的。
    // 站点没公布分组时 in_group 恒真 —— 「不知道」不该表现成「不能用」。
    let models: Vec<_> = all
        .into_iter()
        .filter(|m| m.in_group(&route.group))
        .collect();
    let recommended = qb_station::station::pricing::pick_audit_model(
        &models,
        &route.group,
        prefers_anthropic(&route),
    );
    let problem = problem.or_else(|| {
        models
            .is_empty()
            .then(|| "这个分组没有公布模型，请手动输入".into())
    });

    Ok(StationModelsView {
        models,
        recommended,
        problem,
    })
}

/// 按使用这条线路的软件选择模型家族；站点支持多种协议不改变客户端身份。
fn prefers_anthropic(route: &Route) -> bool {
    route.client != Client::Codex
}

/// 跑一轮检验。**会计费** —— 只在使用者点的时候调。
///
/// `model` 是使用者在「站点 → 分组 → 模型」里选的那个。
/// 传 `None` 表示让后端挑(`pick_audit_model`) —— 站点真有那个默认模型才用它,
/// 没有就退回该分组的第一个。
///
/// # ⛔ 不要退回站点环境里配的那个模型名
///
/// `provider.meta.model` 是启动客户端时写进环境的,可能是空的、也可能是一个
/// 这个分组根本不提供的名字。拿它去验,验的是一个不存在的东西,六项里的倍率
/// 会全记「没测到」—— 界面上看起来像站点不配合,其实是我们挑错了模型。
#[tauri::command]
pub async fn station_run_audit(
    app: tauri::AppHandle,
    route_id: String,
    model: Option<String>,
    test_key: String,
    cold: bool,
) -> Result<StoredAudit> {
    use tauri::Emitter;
    if test_key.trim().is_empty() || test_key.contains(['\r', '\n']) {
        return Err(GateError::Other(
            "请填写本次检验的临时 API Key；不会保存到历史或线路配置".into(),
        ));
    }
    // Serialise spending: duplicate clicks/windows must never start overlapping audit batches.
    static AUDIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if AUDIT.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err(GateError::Other("已有检验正在运行，请等待完成".into()));
    }
    struct AuditGuard;
    impl Drop for AuditGuard {
        fn drop(&mut self) {
            AUDIT.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let _audit_guard = AuditGuard;
    let db = crate::repository::Repository::open()?;
    let mut routes: Vec<Route> = db.list("station_routes").unwrap_or_default();
    let Some(idx) = routes.iter().position(|r| r.id == route_id) else {
        return Err(GateError::Other("找不到这条线路".into()));
    };
    station_billing::refresh(&routes[idx].station_id).await?;
    let mut source = station_ops::source(&db, &routes[idx])?;
    source.key = test_key.trim().into();

    let previous = station_audits(route_id.clone())?
        .into_iter()
        .next()
        .map(|a| a.round);
    let now = chrono::Utc::now().timestamp_millis();
    // 官方价：优先用这次启动抓到的，没抓到就退回内置快照。
    let catalog = qb_station::station::pricing::Catalog::new(stored_prices());
    // 验哪个模型：使用者选的优先；没选就去站点的价目表里挑一个真实存在的。
    let client = station_billing::http_client()?;
    let model = match model
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
    {
        Some(m) => m,
        None => {
            let offered = station_ops::fetch_station_models(&source, &client).await;
            qb_station::station::pricing::pick_audit_model(
                &offered,
                &routes[idx].group,
                prefers_anthropic(&routes[idx]),
            )
            .unwrap_or_default()
        }
    };
    if model.trim().is_empty() {
        return Err(GateError::Other(
            "没有可用模型，请在检验窗口选择或手动填写模型；未发送计费请求".into(),
        ));
    }
    let outcome = station_ops::run_audit_with(
        &source,
        routes[idx].nominal_rate,
        &model,
        &catalog,
        previous.as_ref(),
        &client,
        now,
        cold,
        &|completed, total, stage| {
            let _ = app.emit("station-audit-progress", serde_json::json!({"routeId":route_id,"completed":completed,"total":total,"stage":stage}));
        },
    )
    .await;
    let round = outcome.round;

    // 结论回流到线路上。**mult 没量到就不写** —— 写个 1.0 等于断言「属实」。
    if let Some(m) = round.mult {
        routes[idx].latest_mult = Some(m);
    }
    // 顺手学到的价目也回流。**这一步不是可选的**:排序要拿它算加权倍率,
    // 两种后端都要靠 rates_model 才查得到官方价(New API 的倍率换成单价也还要除官方单价)。
    // 没学到就原样留着上一次那份 —— 拉取失败不等于「这家站点不公布价」。
    if let Some(r) = outcome.rates {
        // 「输出加价」要跟官方的输出 ÷ 输入比,不是看 completion_ratio 大不大于 1
        // (Claude 官方本来就是 5)。官方价里没有这个模型就是不知道。
        routes[idx].output_markup = catalog
            .resolve(&outcome.model)
            .and_then(|o| r.output_markup(&o));
        routes[idx].rates = r;
        routes[idx].rates_model = Some(outcome.model);
    }
    routes[idx].last_audit_ms = Some(now);
    db.put("station_routes", &route_id, &routes[idx])?;

    let stored = StoredAudit {
        route_id: route_id.clone(),
        round,
    };
    db.put(
        "station_audits",
        &format!("{route_id}@{now}"),
        &serde_json::json!({
            "route_id":route_id, "sealed":crate::secret::seal(&serde_json::to_string(&stored)?)?
        }),
    )?;
    Ok(stored)
}

#[tauri::command]
pub fn station_billing_settings(station_id: String) -> Result<StationBillingSettings> {
    station_billing::settings(&crate::repository::Repository::open()?, &station_id)
}

#[tauri::command]
pub async fn station_billing_connect(
    station_id: String,
    backend: String,
    account: String,
    password: String,
) -> Result<StationBillingSettings> {
    station_billing::connect(&station_id, &backend, &account, &password).await
}

#[tauri::command]
pub async fn station_billing_browser(
    app: tauri::AppHandle,
    station_id: String,
) -> Result<StationBillingSettings> {
    crate::station_login::login(app, station_id).await
}

#[tauri::command]
pub async fn station_billing_refresh(station_id: String) -> Result<StationBillingSettings> {
    station_billing::refresh(&station_id).await
}

#[tauri::command]
pub async fn station_billing_save(
    station_id: String,
    backend: String,
    user_id: String,
    token: Option<String>,
) -> Result<StationBillingSettings> {
    let _guard = crate::operations::exclusive().await?;
    station_billing::save(
        &crate::repository::Repository::open()?,
        &station_id,
        &backend,
        &user_id,
        token.as_deref(),
    )
}

// ------------------------------------------------------------------ 官方价目

/// 这次启动抓价的结果。只进审计日志（`lib.rs` 的启动任务），不过 IPC ——
/// 原来还有一条只读的 `station_prices` 命令，界面从来没调过，2026-09-27 删了。
#[derive(Debug, Clone)]
pub struct PriceStatusView {
    /// 这次启动抓到了几个模型的价。0 = 全用内置快照。
    pub live_models: u32,
    /// 内置快照里有几个。
    pub builtin_models: u32,
    /// 上一次成功抓取是哪天（`YYYY-MM-DD`）。没抓到过就是 null。
    pub fetched_at: Option<String>,
    /// 每个来源的失败原因。**非空不代表坏了** —— 内置快照仍然在用。
    pub problems: Vec<String>,
}

const PRICE_META_KEY: &str = "station_prices";

/// 抓回来的官方价。**读库只有这一处** —— 账户页的用量小结也要用它取价，
/// 所以是 `pub(crate)` 而不是私有（`commands::accounts` 在同一层）。
pub(crate) fn stored_prices() -> Vec<qb_station::station::pricing::FetchedPrice> {
    crate::repository::Repository::open()
        .ok()
        .and_then(|db| db.meta(PRICE_META_KEY).ok().flatten())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// 抓一轮官方价并存下来。**面板启动时跑一次**（`lib.rs` 的 setup 直接调，不是命令）。
///
/// ⛔ 抓失败不报错：继续用内置快照，把原因放进 `problems` 写进审计日志。
/// 一次失败的抓取绝不清空已有的价 —— 价被清空之后所有「真实倍率」
/// 一起变成「不知道」，而界面上看起来只是「都没检验过」。
pub async fn station_refresh_prices() -> Result<PriceStatusView> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let (fetched, problems) = station_ops::fetch_prices(&reqwest::Client::new(), &today).await;

    let kept = if fetched.is_empty() {
        stored_prices()
    } else {
        if let Ok(db) = crate::repository::Repository::open() {
            let _ = db.set_meta(PRICE_META_KEY, &serde_json::to_string(&fetched)?);
        }
        fetched
    };
    Ok(PriceStatusView {
        live_models: kept.len() as u32,
        builtin_models: qb_station::station::pricing::TABLE.len() as u32,
        fetched_at: kept.first().map(|f| f.fetched_at.clone()),
        problems,
    })
}

// ------------------------------------------------------------ 健康度与排序

/// 一条线路这一轮的健康度摘要。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RouteHealthView {
    pub route_id: String,
    pub requests: u32,
    /// 成功率。**站点没报成败时是 `null`,不是 100%。**
    pub success_rate: Option<f64>,
    pub cache_hit_rate: Option<f64>,
    pub ttft_p95_ms: Option<u32>,
    pub cost_24h: Option<f64>,
    /// 这个窗口里一共用了多少 token（四类之和）。
    ///
    /// 数在 `health::window::Window` 里一直是现成的，只是没进这个 view ——
    /// 界面上「总 Token」那一格因此永远是空的。
    pub tokens_24h: Option<u32>,
    /// 四类 token 各用了多少：输入 / 缓存读 / 缓存写 / 输出。
    ///
    /// ⛔ **四类要分开给。** 合成一个总数会把「输出翻五倍」这种结构藏掉 ——
    /// 而真实用量里输出往往才是花钱的大头。每一类都可能没取证到。
    pub input_tokens: Option<u32>,
    pub cache_read_tokens: Option<u32>,
    pub cache_write_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    /// 拉取失败的原因。`Some` 时上面那些数是空的,**不是「零请求」**。
    pub error: Option<String>,
}

/// 拉一轮账单并聚合。**零成本** —— 读的是站点自己的日志,不打模型。
#[tauri::command]
pub async fn station_refresh_health() -> Result<Vec<RouteHealthView>> {
    let db = crate::repository::Repository::open()?;
    let routes: Vec<Route> = db.list("station_routes").unwrap_or_default();
    let now = chrono::Utc::now().timestamp_millis();
    let health =
        station_ops::route_health(&db, &routes, &station_billing::http_client()?, now).await;
    Ok(routes
        .iter()
        .map(|r| {
            let h = health.get(&r.id);
            let day = h.map(|h| &h.windows.day);
            let mix = day.map(|d| d.mix);
            RouteHealthView {
                route_id: r.id.clone(),
                requests: day.map(|d| d.requests as u32).unwrap_or(0),
                success_rate: day.and_then(|d| d.success_rate),
                cache_hit_rate: day.and_then(|d| d.cache_hit_rate),
                ttft_p95_ms: day.and_then(|d| d.ttft_p95_ms).map(|v| v as u32),
                cost_24h: day.and_then(|d| d.cost),
                tokens_24h: day.and_then(|d| d.tokens).map(|v| v as u32),
                input_tokens: mix.and_then(|m| m.input).map(|v| v as u32),
                cache_read_tokens: mix.and_then(|m| m.cache_read).map(|v| v as u32),
                cache_write_tokens: mix.and_then(|m| m.cache_write).map(|v| v as u32),
                output_tokens: mix.and_then(|m| m.output).map(|v| v as u32),
                error: h.and_then(|h| h.error.clone()),
            }
        })
        .collect())
}

/// 排一次序。**只排,不换** —— 换上游是 [`station_select_route`] 的事,
/// 那一步要由使用者或明确的自动调度触发,不能藏在一个「看看排名」的命令里。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DecisionView {
    pub ranking: Ranking,
    /// 「便宜」这一维用的是哪种口径。界面要照实写出来。
    pub basis: CheapBasis,
}

#[tauri::command]
pub async fn station_decide(
    state: State<'_, AppState>,
    prefs: Prefs,
    client: Client,
) -> Result<DecisionView> {
    // ⛔ **只排这个软件的线路。** 整池一起排的话，Claude Code 这一栏会
    // 排出一条属于 Codex 的线，选上去之后那把 key 是给 Codex 配的，
    // 账记到另一头，而每一发请求都成功。
    let db = crate::repository::Repository::open()?;
    let routes: Vec<Route> = db
        .list::<Route>("station_routes")
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.client == client && station_ops::source(&db, r).is_ok())
        .collect();
    let now = chrono::Utc::now().timestamp_millis();

    // 健康度要现拉一轮。**不拉的话「快」和「稳」两维永远是「没有证据」**,
    // 排序退化成只比倍率 —— 界面上还摆着三个复选框,而其中两个不起作用。
    // 这一轮是免费的:本机路由自己记过的线读它自己的日志(2026-10-24 起),
    // 没记过的才去读站点的账单日志,都不打模型。
    let health =
        station_ops::decision_health(&db, &routes, &station_billing::http_client()?, now).await;

    // 熔断状态归路由器管 —— 这里只读,不另算一份(两份必然漂移)。
    let (incumbent, breakers) = {
        let s = state.station.lock().ok();
        (
            s.as_ref().and_then(|s| s.current(client)),
            s.as_ref().map(|s| s.breaker_state(now)).unwrap_or_default(),
        )
    };
    // 官方价目跟检验那边用同一份:抓回来的优先,没抓到退回内置快照。
    let catalog = qb_station::station::pricing::Catalog::new(stored_prices());
    let (ranking, basis) = station_ops::decide(
        &routes,
        &health,
        &breakers,
        &prefs,
        incumbent.as_deref(),
        &catalog,
        &station_ops::topup_ratios(&db),
    );
    Ok(DecisionView { ranking, basis })
}

// ------------------------------------------------------------------ 智能调度

// 这一段本来在 `app.rs` 里,搬过来是因为 `architecture.rs` 的
// `module_cycles_only_ever_shrink` 当场抓住了一个环:`app` 调
// `commands::station::run_schedule_tick`,而 `commands` 本来就依赖 `app`
// 拿 `AppState` —— 两个模块互相到得了,也就拆不进不同的 crate。
//
// 放在这一层是对的:这个循环要发界面事件、要读 IPC 那张表,
// 本来就是接口层的事;`app.rs` 只留那个停止句柄(它是状态,不是逻辑)。
// ------------------------------------------------------------------ 智能调度

/// 两跳之间隔多久。
///
/// # 为什么是 60 秒而不是 10 秒
///
/// 每一跳要给池子里每条线路各拉一次站点账单。那是免费的(不打模型),
/// 但不是没有代价:十条线路的池子在 10 秒档上就是每分钟 60 个请求打到
/// 中转站的账单接口,有些站点会因此限流 —— 而限流的表现是健康度读不到,
/// 于是排序退化成只比倍率。**盯得太紧反而让它瞎掉。**
///
/// 60 秒也跟迟滞是一套的:换上游的代价(正在飞的请求落到另一把 key 上、
/// 日志断成两段、熔断窗口重来)决定了换的频率本来就该低。
const SCHEDULE_TICK: std::time::Duration = std::time::Duration::from_secs(60);

/// 起智能调度的驻留循环。**重复调是安全的** —— 已经在跑就直接返回。
///
/// # ⛔ 面板关了它就停
///
/// 这个循环活在面板进程里。关掉面板,调度就不再换上游了 —— 而客户端
/// 还在照着上一次选的那条线跑。这不是缺陷,是这个软件的定位:它是个面板,
/// 不是后台服务。**但界面上必须说清楚**,否则使用者会以为关了面板还有人
/// 替他挑线路(见 [`Schedule`](qb_station::schedule::Schedule) 的文档)。
///
/// 循环自己不读界面状态:每一跳都从库里重新读设置,所以在面板上改完偏好
/// 下一跳就生效,不用重启循环。
pub(crate) fn start_scheduler(state: &AppState, app: Option<tauri::AppHandle>) {
    let mut current = state.scheduler_stop.lock().unwrap();
    if current.as_ref().is_some_and(|tx| !tx.is_closed()) {
        return;
    }
    let (tx, mut rx) = tokio::sync::watch::channel(false);
    *current = Some(tx);
    drop(current);
    let Some(app) = app else {
        // 没有 AppHandle 就拿不到 AppState(循环要用它里面的 station 锁)。
        // demo 模式和单测走这条路 —— 什么都不做,而不是假装起来了。
        return;
    };
    tauri::async_runtime::spawn(async move {
        use tauri::Manager;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(SCHEDULE_TICK) => {}
                _ = rx.changed() => return,
            }
            let state = app.state::<AppState>();
            let touched = run_schedule_tick(&state).await;
            if touched.is_empty() {
                // 一个软件都没开着 —— 循环自己退出,别空转一整晚。
                // 再开时 `station_schedule_set` 会把它拉回来。
                return;
            }
            // 界面要看得见「刚才换过了」。刷不动不影响调度本身。
            crate::operations::changed(&crate::events::ui(&app), "station", "schedule");
        }
    });
}

/// 停掉调度循环。**不动当前上游** —— 停的意思是「以后不替你换了」。
pub(crate) fn stop_scheduler(state: &AppState) {
    if let Some(tx) = state.scheduler_stop.lock().unwrap().take() {
        let _ = tx.send(true);
    }
}

/// 三个软件各自的调度状态。**一次全给** —— 界面上那三个分页要同时显示
/// 「这个软件的调度开着没有」，一个一个问会让切分页时闪一下旧值。
///
/// 没存过的软件给「关着」那一份；**读不出来就报错**，不吞成「关着」——
/// 0.16.0–0.25.6 这里吞了，调度表从没建过这件事因此一直没人看见（迁移 4 的说明）。
#[tauri::command]
pub fn station_schedules() -> Result<Vec<Schedule>> {
    let db = crate::repository::Repository::open()?;
    ALL_CLIENTS
        .iter()
        .map(|&c| station_ops::load_schedule(&db, c))
        .collect()
}

/// 开关这个软件的智能调度，顺带存下偏好。
///
/// # ⛔ 开着 = 真的有东西在换上游
///
/// 存完立刻把驻留循环拉起来(见 [`crate::app::start_scheduler`])。
/// 只存不拉的话，界面显示「调度中」而什么都没发生 —— 那正是 0.16.0 之前
/// 那个纯前端开关的毛病，只是换了个地方藏。
///
/// 关掉时**不动当前上游**:调度停下来的意思是「以后不替你换了」，
/// 不是「把你换回去」。换回去会让使用者正在跑的那条线莫名其妙地断掉。
///
/// # `enabled` 是三态
///
/// `None` = **只存偏好，别动开关**。调度设置那个弹窗每改一项就存一次,
/// 它不该顺手把调度打开 —— 「看看按这套会怎么排」和「照这套替我换」
/// 是两件事,在界面上也是两个入口。
#[tauri::command]
pub fn station_schedule_set(
    app: tauri::AppHandle,
    client: Client,
    enabled: Option<bool>,
    prefs: Option<Prefs>,
) -> Result<Schedule> {
    use tauri::Manager;
    let db = crate::repository::Repository::open()?;
    let mut s = station_ops::load_schedule(&db, client)?;
    if let Some(on) = enabled {
        s.enabled = on;
        if !on {
            // 关掉时把上一轮的结论也清掉 —— 留着的话界面会显示一句
            // 几分钟前的话，看起来像还在跑。
            s.last_note = String::new();
        }
    }
    if let Some(p) = prefs {
        s.prefs = p;
    }
    station_ops::save_schedule(&db, &s)?;
    if s.enabled {
        start_scheduler(&app.state::<AppState>(), Some(app.clone()));
    } else if station_schedules()?.iter().all(|s| !s.enabled) {
        // 最后一个也关了才停循环 —— 三个软件共用同一个循环,
        // 关掉 Codex 那个就把 Claude Code 的一起停掉是另一类 bug。
        stop_scheduler(&app.state::<AppState>());
    }
    Ok(s)
}

/// 调度循环跑一跳:每个开着的软件各排一次序，该换就换。
///
/// # 为什么一跳要重新拉健康度
///
/// 「快」和「稳」两维的证据只有站点账单里有，不拉就是 `None`，排序退化成
/// 只比倍率 —— 而界面上那三个复选框还摆着。这一轮是**免费**的:读的是
/// 站点自己的账单日志，不打模型。
///
/// # ⛔ 返回值是给日志和界面看的，不是给下一跳看的
///
/// 每一跳都从库里重新读设置与线路。把状态缓存在循环变量里的话，
/// 使用者在面板上改了偏好要等到下次重启才生效，而界面上立刻就变了 ——
/// 两边说的不是同一件事。
pub async fn run_schedule_tick(state: &AppState) -> Vec<Schedule> {
    let Ok(db) = crate::repository::Repository::open() else {
        return Vec::new();
    };
    let all: Vec<Route> = db.list("station_routes").unwrap_or_default();
    let now = chrono::Utc::now().timestamp_millis();
    let catalog = qb_station::station::pricing::Catalog::new(stored_prices());
    // 每家站 1 美元额度付几元 —— 跨站比便宜之前先折成同一种钱。每一跳重读,改完下一跳就生效。
    let topup = station_ops::topup_ratios(&db);
    let mut out = Vec::new();
    let http = station_billing::http_client();

    for &client in ALL_CLIENTS {
        let mut sched = match station_ops::load_schedule(&db, client) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    "智能调度：读不出 {} 的设置，这一跳跳过它：{e}",
                    client_key(client)
                );
                continue;
            }
        };
        if !sched.enabled {
            continue;
        }
        let routes: Vec<Route> = all
            .iter()
            .filter(|r| r.client == client && station_ops::source(&db, r).is_ok())
            .cloned()
            .collect();
        if routes.is_empty() {
            sched.last_note = "没有已配置可用 API Key 的线路，请添加或编辑线路".into();
            keep_schedule(&db, &sched);
            out.push(sched);
            continue;
        }
        let Ok(http) = &http else {
            sched.last_note = "无法建立账单连接，请检查本机网络配置".into();
            keep_schedule(&db, &sched);
            out.push(sched);
            continue;
        };
        // 本机路由自己记过的线用它的日志，没记过的读站点账单（一跳 60 秒，⛔ 不调小）。
        let health = station_ops::decision_health(&db, &routes, http, now).await;
        let (incumbent, breakers) = {
            let s = state.station.lock().ok();
            (
                s.as_ref().and_then(|s| s.current(client)),
                s.as_ref().map(|s| s.breaker_state(now)).unwrap_or_default(),
            )
        };
        let (ranking, _) = station_ops::decide(
            &routes,
            &health,
            &breakers,
            &sched.prefs,
            incumbent.as_deref(),
            &catalog,
            &topup,
        );
        match station_ops::next_upstream(&ranking, incumbent.as_deref()) {
            Some(next) => {
                let label = routes
                    .iter()
                    .find(|r| r.id == next)
                    .map(|r| {
                        if r.group.is_empty() {
                            r.station_id.clone()
                        } else {
                            format!("{} · {}", r.station_id, r.group)
                        }
                    })
                    .unwrap_or_else(|| next.clone());
                if let Ok(mut s) = state.station.lock() {
                    // 下一次请求生效，不打断正在飞的那一发。
                    s.request_switch(client, next.clone());
                }
                // 调度换过去的也记下来，面板重启后接着用（写不进去不挡这一跳）。
                if let Err(e) = station_ops::remember_current(&db, client, &next, now) {
                    tracing::warn!("智能调度：已换到 {next}，但没记下来：{e}");
                }
                sched.last_switch_ms = Some(now);
                sched.last_note = format!("已换到「{label}」");
            }
            None => {
                sched.last_note = if ranking.held_by_hysteresis {
                    "有更好的，但没好够，留着现任".into()
                } else if ranking.winner.is_none() {
                    "排不出名次：勾中的那几项这一轮都没有证据".into()
                } else {
                    "现任仍然是最好的那条".into()
                };
            }
        }
        keep_schedule(&db, &sched);
        out.push(sched);
    }
    out
}

/// 把这一跳的结论写回库。写不回去不挡这一跳（上游已经换了），但要留下痕迹 ——
/// 默默丢掉正是调度表那个 bug 藏了这么久的原因。
///
/// 是函数不是闭包：闭包会把 `&Repository` 一路揣过 `route_health(..).await`，
/// 而 `Repository` 不是 `Sync`，调度循环的 future 就不能再交给 `spawn`。
fn keep_schedule(db: &crate::repository::Repository, sched: &Schedule) {
    if let Err(e) = station_ops::save_schedule(db, sched) {
        tracing::warn!(
            "智能调度：{} 这一跳的结论没写进库：{e}",
            client_key(sched.client)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qb_station::station::model::Protocols;

    fn route_with(client: Client, protocols: Protocols) -> Route {
        Route {
            id: Route::make_id(client, "s", ""),
            client,
            station_id: "s".into(),
            group: String::new(),
            credential_id: None,
            nominal_rate: None,
            latest_mult: None,
            last_audit_ms: None,
            protocols,
            rates: Default::default(),
            output_markup: None,
            rates_model: None,
            model_map: Default::default(),
            upstream_format: None,
        }
    }

    #[test]
    fn codex_on_a_multiprotocol_station_still_uses_gpt() {
        let route = route_with(
            Client::Codex,
            Protocols {
                anthropic: Some(true),
                openai_chat: Some(true),
                openai_responses: Some(true),
            },
        );
        assert!(!prefers_anthropic(&route));
    }

    #[test]
    fn a_route_that_only_speaks_openai_is_audited_with_the_gpt_default() {
        let r = route_with(
            Client::Codex,
            Protocols {
                anthropic: Some(false),
                openai_chat: Some(true),
                openai_responses: None,
            },
        );
        assert!(!prefers_anthropic(&r));
    }

    #[test]
    fn an_unprobed_openai_only_route_still_picks_the_gpt_default() {
        // anthropic 还没探过、openai 探到了 —— 拿 claude-opus-5 去验这种线路,
        // 验的是一个它根本不提供的模型。
        let r = route_with(
            Client::Codex,
            Protocols {
                anthropic: None,
                openai_chat: None,
                openai_responses: Some(true),
            },
        );
        assert!(!prefers_anthropic(&r));
    }

    #[test]
    fn a_route_that_speaks_anthropic_is_audited_with_the_claude_default() {
        let r = route_with(
            Client::ClaudeCode,
            Protocols {
                anthropic: Some(true),
                openai_chat: Some(true),
                openai_responses: None,
            },
        );
        assert!(prefers_anthropic(&r));
    }

    #[test]
    fn a_route_nobody_has_probed_yet_defaults_to_claude() {
        // 都没探过时偏 Anthropic —— 这个面板的主要使用者跑的是 Claude Code。
        // 挑错了也不致命:pick_audit_model 会退回站点真有的第一个模型。
        assert!(prefers_anthropic(&route_with(
            Client::ClaudeCode,
            Protocols::default()
        )));
    }
}
