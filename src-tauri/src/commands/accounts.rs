//! 官方账户槽位相关的命令。
//!
//! ⚠ **这里没有、也不许有自动切换的入口。** 切槽位只能由使用者点出来，
//! 见 `DISCLAIMER.md` 第 4 节那四条设计约束的前两条。

use crate::{
    accounts, audit,
    error::{GateError, Result},
    killswitch, operations, tray, usecase, AppState,
};

// ------------------------------------------------------------------ 账户

#[tauri::command]
pub fn accounts_list() -> accounts::AccountsReport {
    accounts::report(
        &accounts::AccountRoots::current(),
        accounts::SyncReport::default(),
    )
}

/// 新建一个空槽位。只由界面点击触发。
#[tauri::command]
pub async fn accounts_create(
    app: tauri::AppHandle,
    label: String,
) -> Result<accounts::CreateOutcome> {
    let _guard = operations::exclusive_soon().await?;
    let out = accounts::create_slot(&accounts::AccountRoots::current(), label.trim())?;
    for n in &out.notes {
        audit::write(n);
    }
    audit::write(&format!(
        "新建账户槽位 {}{}",
        out.label,
        if out.activated {
            "（原来没有激活槽位，它直接成了当前的）"
        } else {
            ""
        }
    ));
    tray::refresh(&app);
    Ok(out)
}

/// 删掉一个槽位。**只由界面上的手动点击触发。**
///
/// 判定与目录操作都在 `accounts::delete_slot`（当前槽位拒删的理由写在那儿）。
/// 这里只负责：排他锁、记账、刷托盘。
#[tauri::command]
pub async fn accounts_delete(
    app: tauri::AppHandle,
    label: String,
    drop_desktop: bool,
) -> Result<accounts::DeleteOutcome> {
    let _guard = operations::exclusive_soon().await?;
    let out = accounts::delete_slot(
        &accounts::AccountRoots::current(),
        label.trim(),
        drop_desktop,
    )?;
    for n in &out.notes {
        audit::write(n);
    }
    audit::write(&format!(
        "删除账户槽位 {}（{}）",
        out.label,
        out.removed.join("、")
    ));
    // 托盘菜单是静态对象，不重建就还列着那个已经没了的槽位。
    tray::refresh(&app);
    Ok(out)
}

/// 这个槽位用掉了多少 token。零网络请求，只读槽位目录里的会话转写。
///
/// 是 `async` + `spawn_blocking` 而不是同步命令：实测两个槽位合计 90 MB，
/// 一遍约半秒 —— 直接在 async 命令体里跑会把 tokio 的 worker 堵住，
/// 那半秒里界面上别的请求全排在后面（跟 `accounts_switch` 里
/// `mklink` 那一段同一个理由）。
#[tauri::command]
pub async fn accounts_tokens(label: String) -> Result<accounts::tokens::TokenUsage> {
    let dir = accounts::AccountRoots::current().slot_dir(label.trim());
    tokio::task::spawn_blocking(move || accounts::tokens::for_slot(&dir))
        .await
        .map_err(|e| GateError::Other(format!("统计 token 的任务异常结束：{e}")))
}

/// 账户用量小结：今天（或最近 N 天）用了多少 token、缓存命中多少、
/// 缓存替你省下多少钱。零网络请求 —— 数的是本机已经落盘的会话转写。
///
/// `days`：`1` = 今天，`7` = 含今天的最近七天，`0` = 全部。
///
/// 价目在这一层装配：`Catalog` 要读库（抓回来的官方价），
/// 而 `qb-app` 够不着 `Repository` 的那一半。⛔ 别在 `token_summary` 里
/// 改回 `pricing::lookup` —— 那只查内置快照，实机最常用的几个模型全在外面。
#[tauri::command]
pub async fn accounts_token_summary(
    label: String,
    days: i64,
) -> Result<usecase::token_summary::TokenSummary> {
    let roots = accounts::AccountRoots::current();
    let dir = roots.slot_dir(label.trim());
    let uuid = accounts::account_uuid_of(&dir);
    let prices = qb_station::station::pricing::Catalog::new(super::station::stored_prices());
    tokio::task::spawn_blocking(move || {
        usecase::token_summary::for_slot_with_default_dir(&dir, uuid.as_deref(), days, &prices)
    })
    .await
    .map_err(|e| GateError::Other(format!("统计 token 的任务异常结束：{e}")))
}

/// 用量明细页：当前槽位的完整小结（按天 / 按模型 / 按小时 / 最近请求 / 美元）
/// 加上「按账户」—— 默认目录只扫一遍，分给所有槽位。零网络请求。
///
/// 跟 `accounts_token_summary` 分开：账户卡每分钟读一次，只要那几个合计；
/// 这一条要把所有槽位都扫一遍，只有打开用量明细页时才值得。
#[tauri::command]
pub async fn accounts_usage_overview(
    label: String,
    days: i64,
) -> Result<usecase::token_summary::UsageOverview> {
    let prices = qb_station::station::pricing::Catalog::new(super::station::stored_prices());
    tokio::task::spawn_blocking(move || {
        usecase::token_summary::overview(label.trim(), days, &prices)
    })
    .await
    .map_err(|e| GateError::Other(format!("统计 token 的任务异常结束：{e}")))
}

/// 「这个账户现在还能用吗」—— 拿槽位里的令牌向官方发一次最小认证请求。
///
/// **只由使用者当次点击触发。** 这是面板唯一一处带着官方身份对外发请求的地方，
/// 判定与边界全在 `usecase::account_probe` 的文件头里：
/// 打的是公开的 `/v1/models`，不查额度、不打模型、不写回任何东西，
/// 本地令牌已过期时**连请求都不发**（免得把必然的 401 报成假警报）。
#[tauri::command]
pub async fn account_probe(label: String) -> Result<usecase::account_probe::ProbeResult> {
    let dir = accounts::AccountRoots::current().slot_dir(label.trim());
    let r = usecase::account_probe::probe(&dir, &reqwest::Client::new()).await;
    audit::write(&format!("账户 {} 认证实测：{:?}", label.trim(), r.state));
    Ok(r)
}

#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct SwitchReport {
    /// 切换前清场（关闭全部 Claude）的报告。
    closed: killswitch::KillReport,
    /// 换了哪几处指向：Claude Code / 酒馆桥接 / 桌面端。
    switched: Vec<String>,
    notes: Vec<String>,
}

/// 切换账户。**只由界面上的手动点击触发，不要从任何自动路径调用它。**
///
/// v0.9.0 的语义：**先清场，再切，不自动启动。**
///
///   1. `killswitch::execute()` 关掉全部正在跑的 Claude —— 桌面端、所有 Claude Code
///      会话、酒馆桥接。清场之后没有任何进程还连着旧账户，也就不存在
///      「在跑着的桌面端脚下换它的资料目录」这种会把两个账户搅在一起的中间态。
///   2. `accounts::switch_with` 把三处指向都换到目标槽位。
///   3. 结束。**不起任何进程** —— 用户回总览自己点要启动的东西。
///
/// `desktop` 决定桌面端跟不跟着切（`Follow` / `Keep`）。收进程会顺手重锁，
/// `Maintenance::observing` 守着让重锁不等于把门关死。
#[tauri::command]
pub async fn accounts_switch(
    app: tauri::AppHandle,
    label: String,
    desktop: bool,
    state: tauri::State<'_, AppState>,
) -> Result<SwitchReport> {
    let _guard = operations::exclusive().await?;
    let mode = if desktop {
        accounts::DesktopMode::Follow
    } else {
        accounts::DesktopMode::Keep
    };

    usecase::account_ops::preflight_switch(&label)?;
    // ---- 1. 清场：关掉全部 Claude（桌面端也在内）。失败就到此为止，槽位一点没动。
    let closed =
        crate::app::clear_for_switch(&state, matches!(mode, accounts::DesktopMode::Follow)).await?;

    // ---- 2. 换指向。
    //
    // 整段是同步的，而且**里面会 fork 好几次 `cmd /C mklink`**（三处指向：
    // Claude Code / 酒馆桥接 / 桌面端，每一处建联结点都是一次进程）。
    // 直接在 async 命令体里跑，堵的是 tokio 的 worker 线程 ——
    // 切账户那一两秒里，界面上别的请求全都排在后面。
    let label_for_switch = label.clone();
    let out = tokio::task::spawn_blocking(move || {
        usecase::account_ops::switch_with(&label_for_switch, mode)
    })
    .await
    .map_err(|e| GateError::Other(format!("切换账户的任务异常结束：{e}")))??;
    // 托盘菜单是静态对象，不重建就会显示上一次的账户。
    tray::refresh(&app);
    Ok(SwitchReport {
        closed,
        switched: out.switched,
        notes: out.notes,
    })
}
