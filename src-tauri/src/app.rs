//! 应用级状态与装配（L4）。
//!
//! # 为什么单独一个模块
//!
//! [`AppState`] 与下面三个助手，**命令层和托盘菜单都要用**：托盘里那两项
//! 「重新放行」「切换账户」跟面板上的按钮走的必须是同一条路 ——
//! 两份实现就是两套安全口径，而其中一套一定会先烂掉。
//!
//! 放在 `commands` 里的话，`tray` 要调 `commands`、`commands` 又要调
//! `tray::refresh` 刷菜单 —— `commands ↔ tray` 一对环，棘轮当场报红。
//! 抽出来之后方向是单向的：`commands → app`、`tray → app`、`commands → tray`。
//!
//! # 刷新界面为什么要经过 AppState
//!
//! 看门狗跑在后台，判定完要让托盘菜单跟着变。但 `app` 直接调 `tray::refresh`
//! 又是 `app ↔ tray`。所以「怎么刷」由装配点（`lib.rs` 的 `run()`）填进
//! [`AppState::refresh_ui`] —— 那一层本来就认识两边，而 `app` 不必认识托盘。

use std::sync::Arc;

use crate::{
    error::{GateError, Result},
    events, gate, killswitch, sessions, sysenv, usecase,
};

/// 「界面该刷新了」。装配时由 `run()` 填上，内容是重建托盘菜单。
///
/// 为什么不是直接调 `tray::refresh`：见模块头。
pub type UiRefresher = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
pub struct AppState {
    pub gate: Arc<gate::GateState>,
    pub watchdog_stop: std::sync::Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    pub tz: std::sync::Mutex<Option<sysenv::TzState>>,
    /// 「界面该刷新了」。`run()` 建好托盘之后填进来。
    ///
    /// 没填时是 `None`，所有刷新都是空操作 —— demo 模式、单测、
    /// 以及托盘还没起来的那一小段启动期都走这条路，不必到处判空。
    refresh_ui: std::sync::Mutex<Option<UiRefresher>>,
    /// 中转站本机路由的状态：当前上游、熔断器、还没落库的请求日志。
    ///
    /// 跟 handle 分开放是有理由的：**路由停掉之后这些状态还要留着** ——
    /// 停了再起不该把日志和熔断记录清零。
    pub station: std::sync::Arc<std::sync::Mutex<qb_app::local_router::RouterState>>,
    /// 路由跑起来之后监听在哪。`None` = 没启动。
    pub station_addr: std::sync::Mutex<Option<std::net::SocketAddr>>,
    /// 持有它就等于持有那个监听。丢掉/`stop()` 就停。
    pub station_router: std::sync::Mutex<Option<qb_app::local_router::RouterHandle>>,
    /// 智能调度驻留循环的停止句柄。`None` / 已关闭 = 没在跑。
    ///
    /// 跟看门狗那个是**两回事**,别合并:看门狗守的是安全边界(出口 IP 变了
    /// 要收 Claude),调度换的是花钱的上游。合成一个之后,关掉调度会把
    /// 看门狗一起关掉 —— 而那扇门开着却没人核对出口 IP,正是 CLAUDE.md
    /// 里那条教训防的场面。
    pub scheduler_stop: std::sync::Mutex<Option<tokio::sync::watch::Sender<bool>>>,
}

impl AppState {
    /// 装配时把「怎么刷界面」交进来。只在 `run()` 里调一次。
    pub fn set_refresh_ui(&self, f: UiRefresher) {
        *self.refresh_ui.lock().unwrap() = Some(f);
    }

    /// 拿一份刷新回调。拿不到就给一个空操作 —— **刷不了界面绝不能让
    /// 看门狗停下来**，它守的是安全边界，托盘只是显示。
    pub fn refresher(&self) -> UiRefresher {
        self.refresh_ui
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Arc::new(|| {}))
    }
}

// ------------------------------------------------------------------ 门禁

/// 起看门狗并把停止句柄挂到 AppState 上。
///
/// 每次都换一个新的 watch channel：旧的那个 sender 被 take 走之后，
/// 上一轮的循环下次醒来会自己退出。
pub(crate) fn start_watchdog(
    mode: gate::watchdog::WatchMode,
    state: &AppState,
    app: Option<tauri::AppHandle>,
) {
    let mut current = state.watchdog_stop.lock().unwrap();
    if current.as_ref().is_some_and(|tx| !tx.is_closed()) {
        return;
    }
    let (tx, rx) = tokio::sync::watch::channel(false);
    *current = Some(tx);
    let gs = state.gate.clone();
    // 刷新界面这件事属于接口层。看门狗（编排层）只管发出「该刷了」，
    // 不认识托盘 —— 反过来的话就是 L3 依赖 L4。
    //
    // 这里也不认识托盘：回调是 `run()` 装配时填进 AppState 的，
    // 否则 `app` 调 `tray`、`tray` 又调 `app`，又是一对环。
    let refresh: usecase::gate_ops::RefreshUi = state.refresher();
    tauri::async_runtime::spawn(async move {
        let events = app.as_ref().map(events::sink);
        usecase::gate_ops::run_watchdog(mode, gs, rx, events, refresh).await;
    });
}

/// 重新放行 + 把看门狗接回去。命令与托盘菜单共用同一份。
///
/// 「接回看门狗」不是可选步骤：门开着却没有东西核对出口 IP，
/// 正是 §7.12 那条教训要防的场面。租约记录里带着档位，按原样接回去 ——
/// 接错档等于偷偷改了安全口径。
pub(crate) async fn reopen_and_rewatch(
    state: &AppState,
    app: Option<tauri::AppHandle>,
) -> Result<String> {
    let detail = gate::reopen_now(&state.gate).await?;
    let mode = state.gate.lease.lock().unwrap().mode;
    if let Some(m) = mode {
        start_watchdog(m, state, app);
    }
    Ok(detail)
}

/// 换账户之前的清场：关掉全部 Claude，收完重锁（维护窗口守着租约）。
///
/// **失败就返回错误，调用方必须停下、不许接着换指向** —— `accounts::switch_in` 从 v0.9.0 起
/// 不再自己查桌面端在不在跑，它信的就是「调用方已经清过场」。清场没成还硬切，
/// 就是在一个跑着的桌面端脚下换它的资料目录。对话框、托盘、档案 / 快照回滚都走这一个。
///
/// `wait_desktop`：taskkill 返回时进程未必已经退干净，桌面端要跟着换资料目录的话
/// 等它真的退了再动（最多约 3 秒）。
pub(crate) async fn clear_for_switch(
    state: &AppState,
    wait_desktop: bool,
) -> Result<killswitch::KillReport> {
    // 「哪些会话算数」只有一个判断：`killswitch::needs_clearing`。
    // 这里原来把同一个条件内联写了两遍（校验一遍、列清单一遍），
    // 两份就会分叉，而分叉的症状是「校验时算一批、真去停的是另一批」。
    sessions::ensure_verified(killswitch::needs_clearing)?;
    let official = sessions::list()
        .into_iter()
        .filter(|s| killswitch::needs_clearing(s) && s.state == "running")
        .collect::<Vec<_>>();
    for session in official {
        sessions::stop(&session.id)?;
        gate::release_holder(&state.gate, &session.id)?;
    }
    let closed = killswitch::execute_official().await;
    let closed = closed?;
    if !closed.failed.is_empty() {
        return Err(GateError::Other(killswitch::unfinished_detail(
            &closed.failed,
        )));
    }
    if wait_desktop {
        for _ in 0..8 {
            if killswitch::desktop_processes()?.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
    }
    let remaining = killswitch::preview_official().await?;
    if !remaining.targets.is_empty() {
        return Err(GateError::Other(
            "相关官方进程未完全退出，账户未切换".into(),
        ));
    }
    if wait_desktop && !killswitch::desktop_processes()?.is_empty() {
        return Err(GateError::Other("桌面端退出超时，账户未切换".into()));
    }
    Ok(closed)
}
