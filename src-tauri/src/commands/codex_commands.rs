//! GPT（Codex）桌面端账户命令。所有会改变状态的动作都由界面显式触发。
//!
//! 0.25.0 起启动 / 关闭 / 切换走 `usecase::codex_accounts`，那里接的是 Claude 页
//! 同一条门禁链（`workspace::launch`：验 IP → 托管会话 → 租约）；这里只做
//! 参数搬运、任务记录和「起来之后挂看门狗」。
use crate::{
    error::{GateError, Result},
    events, operations, AppState,
};
use qb_accounts::codex;
use qb_install::install::codex_desktop;

#[tauri::command]
pub async fn codex_accounts() -> Result<codex::CodexAccounts> {
    tokio::task::spawn_blocking(qb_app::usecase::codex_accounts::list)
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}
#[tauri::command]
pub async fn codex_desktop_status() -> Result<codex_desktop::CodexDesktop> {
    tokio::task::spawn_blocking(codex_desktop::detect)
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}
/// 关掉所有 Codex 桌面端：面板托管的按会话停并交回租约，其余按 exe 路径核验后关。
#[tauri::command]
pub async fn codex_close(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<()> {
    let _guard = operations::exclusive().await?;
    let mut task = operations::Run::start(events::sink(&app), "关闭 GPT 桌面端", "codex")?;
    let result = task.finish(qb_app::usecase::codex_accounts::close(&state.gate).await);
    operations::changed(&events::ui(&app), "session", "codex");
    result
}
/// 以管理员重新注册 Codex 打包应用（更新后打包服务需管理员注册，否则启动一律拒绝访问）。
/// 只做重新注册，会弹 UAC；使用者不授权就返回「已取消」。
///
/// ⛔ **故意不拿 `operations::exclusive`**：它只是起一个提权的外部命令、不动面板自己的
/// 任何状态，而 UAC 提示可能等上几分钟 —— 看门狗每一轮都要拿同一把锁，握着锁等 UAC
/// 等于让门禁在这段时间里停摆。
#[tauri::command]
pub async fn codex_repair_registration() -> Result<()> {
    tokio::task::spawn_blocking(codex_desktop::repair_registration)
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}
#[tauri::command]
pub async fn codex_create(label: String) -> Result<String> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(move || codex::create(&codex::root(), &label))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}
#[tauri::command]
pub async fn codex_switch(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<()> {
    let _guard = operations::exclusive().await?;
    let mut task = operations::Run::start(events::sink(&app), "切换 GPT 账户", &id)?;
    let result = task.finish(qb_app::usecase::codex_accounts::switch(&id, &state.gate).await);
    operations::changed(&events::ui(&app), "session", "codex");
    result
}

/// 验 IP → 解锁 → 起桌面端 → 挂看门狗（Desktop 档）。门禁不过就一个进程都不起。
///
/// 看门狗那一步跟 `commands/workspace.rs::session_launch` 一样按 `gated()` 判：
/// 使用者在「IP 锁」弹窗里把 GPT 移出门禁之后，起的是一个不持租约的会话，
/// 没有租约就没有东西需要看门狗盯。
#[tauri::command]
pub async fn codex_launch(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<()> {
    let _guard = operations::exclusive().await?;
    let mut task = operations::Run::start(events::sink(&app), "启动 GPT 桌面端", &id)?;
    task.phase("检查身份、配置与门禁", 20)?;
    let result = task.finish(qb_app::usecase::codex_accounts::launch(&id, &state.gate).await);
    if result.is_ok() {
        let target = crate::launch::LaunchTarget::Codex;
        if target.gated() {
            crate::app::start_watchdog(target.watch_mode(), &state, Some(app.clone()));
        }
    }
    operations::changed(&events::ui(&app), "session", "codex");
    result
}
#[tauri::command]
pub async fn codex_archive(id: String) -> Result<()> {
    let _guard = operations::exclusive().await?;
    let slot = id.clone();
    tokio::task::spawn_blocking(move || codex::archive(&codex::root(), &id))
        .await
        .map_err(|e| GateError::Other(e.to_string()))??;
    // 这个槽位联网问到的「最近一次」也一起忘掉。
    qb_app::usecase::tavern_quota::forget_gpt(&slot);
    Ok(())
}
/// 装（或更新）Codex 桌面端：winget 的 Store 源优先，没成就 FE3 直连下载 + `Add-AppxPackage`
/// （`qb_install::install::codex_store`，0.28.0）。`local` 给了就只装那一份 `.msix`。
///
/// 顺序：拿独占锁 → **先关掉正在跑的桌面端**（Store 包在跑时更新会失败；界面上事前说明）
/// → 维护窗口 → 装 → 收尾重锁并把租约还回去 → 刷新界面。
/// 进度走 `TASK_INSTALL_CODEX_DESKTOP`，七段（`codex_store::TOTAL`）。
///
/// 装完的核对在 `codex_store::install` 里（回读 `Get-AppxPackage`，不看退出码）。
#[tauri::command]
pub async fn codex_desktop_install(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    local: Option<String>,
    force: bool,
) -> Result<String> {
    use crate::sink::ProgressSink;
    let _guard = operations::exclusive().await?;
    let rep = events::Reporter::new(
        app.clone(),
        events::TASK_INSTALL_CODEX_DESKTOP,
        qb_install::install::codex_store::TOTAL,
    );
    // 关桌面端走面板托管会话 + 官方包 exe 路径那一套（`usecase::codex_accounts::close`），
    // 不按进程名杀。关不掉就不装 —— 装一半的 Store 包比不装更难收拾。
    if let Err(e) = qb_app::usecase::codex_accounts::close(&state.gate).await {
        let msg = format!("没能先关闭 Codex 桌面端，没有安装：{e}");
        rep.fail(&msg);
        return Err(GateError::Other(msg));
    }
    let guard = crate::gate::Maintenance::observing(&state.gate);
    let local_path = local
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from);
    let r = qb_install::install::codex_store::install(local_path.as_deref(), force, &rep).await;
    let done = guard.finish(&state.gate).await;
    operations::changed(&events::ui(&app), "session", "codex");
    match r {
        Ok(o) => {
            let detail = format!(
                "Codex 桌面端 {} 已装好（{}）。{}",
                o.version.clone().unwrap_or_else(|| "版本读不出".into()),
                match o.method {
                    qb_install::install::codex_store::Method::Winget => "winget · Store 源",
                    qb_install::install::codex_store::Method::Direct =>
                        "FE3 直连下载 + Add-AppxPackage",
                    qb_install::install::codex_store::Method::LocalFile => "本地 .msix",
                },
                done.detail
            );
            for line in &o.log {
                rep.log(7, line);
            }
            crate::audit::write(&detail);
            rep.done(&detail);
            Ok(detail)
        }
        Err(e) => {
            let msg = format!("{e} {}", done.detail);
            crate::audit::write(&format!("Codex 桌面端安装失败：{e}"));
            rep.fail(&msg);
            Err(GateError::Other(msg))
        }
    }
}

/// Store 上现在是哪一版（只查元数据，不下载不安装）。给软件页的「检查」用。
///
/// 不拿独占锁：只读，几秒钟的网络往返，不该把看门狗巡检挡在外面。
#[tauri::command]
pub async fn codex_desktop_latest() -> Result<String> {
    let arch = qb_install::install::codex_store::Arch::current();
    Ok(qb_install::install::codex_store::resolve(arch)
        .await?
        .version)
}

/// 用量明细页的 GPT 一侧：按天、按模型、按官方 API 价折算的美元，今天 / 7 天 / 30 天三档。
///
/// **零网络请求** —— 读的是这个槽位 `home\sessions` 里 Codex 自己写的会话记录；
/// 价是库里抓回来的官方价（没有就用内置快照），跟 Claude 那一侧同一份 `Catalog`。
/// 从头读（不设截止）：三档美元要 30 天的桶，跟选中的是哪一档无关。
#[tauri::command]
pub async fn codex_usage_summary(
    id: String,
    days: i64,
) -> Result<qb_app::usecase::token_summary::CodexUsageSummary> {
    if ![0, 1, 7, 30].contains(&days) {
        return Err(GateError::Other("不支持的用量时间范围".into()));
    }
    let prices = qb_station::station::pricing::Catalog::new(super::station::stored_prices());
    tokio::task::spawn_blocking(move || {
        let dir = codex::directory(&codex::root(), &id)?;
        let usage = codex::usage::read(&dir.join("home"), 0)?;
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        Ok(qb_app::usecase::token_summary::codex_summary(
            &usage, days, &today, &prices,
        ))
    })
    .await
    .map_err(|e| GateError::Other(e.to_string()))?
}

#[tauri::command]
pub async fn codex_usage(id: String, days: u32) -> Result<codex::usage::CodexUsage> {
    if ![0, 1, 7].contains(&days) {
        return Err(GateError::Other("不支持的用量时间范围".into()));
    }
    tokio::task::spawn_blocking(move || {
        let dir = codex::directory(&codex::root(), &id)?;
        codex::usage::read(&dir.join("home"), days)
    })
    .await
    .map_err(|e| GateError::Other(e.to_string()))?
}

/// 这个槽位的额度窗口（5 小时 / 7 天）。
///
/// **零网络请求** —— 读的是 Codex 自己写在会话记录里的 `rate_limits`
/// （`qb_accounts::codex::ratelimit`），跟 Claude 读 `plan-usage-history.json` 同一条口径。
///
/// 不拿独占锁：从最新的会话记录往回翻、翻到第一份带额度的就停，通常只读一两个文件。
///
/// ⚠ 它是**上一次请求时的快照**，不是此刻。界面必须标「Codex 写入 hh:mm」。
/// 一份都找不到时 `found` 是 `None` —— 那时界面说「还没有带额度信息的会话记录」，
/// **不是 0%**。
#[tauri::command]
pub async fn codex_rate_limits(id: String) -> Result<codex::ratelimit::CodexRateLimitScan> {
    tokio::task::spawn_blocking(move || {
        let dir = codex::directory(&codex::root(), &id)?;
        // 200 个文件是上界：老槽位可能攒了上千份 rollout，而「一年前那份的额度」
        // 对使用者没有任何意义。
        codex::ratelimit::read(&dir.join("home"), 200)
    })
    .await
    .map_err(|e| GateError::Other(e.to_string()))?
}
