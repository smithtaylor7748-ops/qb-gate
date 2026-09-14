//! 官方账户槽位相关的命令。
//!
//! ⚠ **这里没有、也不许有自动切换的入口。** 切槽位只能由使用者点出来，
//! 见 `DISCLAIMER.md` 与 README 的合规边界前两条。

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
