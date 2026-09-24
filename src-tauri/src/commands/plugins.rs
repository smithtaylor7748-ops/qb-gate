//! 插件（本机应用接入）与酒馆桥接相关的命令。

use crate::sink::ProgressSink;
use crate::{
    error::Result,
    events::{self, Reporter},
    gate, operations, plugins, AppState,
};

// ------------------------------------------------------------ 插件

#[tauri::command]
pub fn plugin_list() -> Vec<plugins::PluginStatus> {
    vec![
        // 内置 GPT / Gemini 桥接活在面板进程里，插件层看不见它们 —— 这里把真实状态递进去。
        plugins::sillytavern::status_with(
            Some(qb_app::gpt_bridge::running()),
            Some(qb_app::gemini_bridge::running()),
        ),
        plugins::codex_egress::status(),
        plugins::antigravity_ui::plugin_status(),
    ]
}

// ------------------------------------------------------------ 反重力汉化与审批引擎

#[tauri::command]
pub fn antigravity_ui_status() -> plugins::antigravity_ui::UiStatus {
    plugins::antigravity_ui::status()
}

/// 附加到正在跑的反重力（Hub / IDE，哪个应答得了就接哪个；只注入脚本，不起它、不改文件）。
#[tauri::command]
pub async fn antigravity_ui_start() -> Result<plugins::antigravity_ui::UiStatus> {
    let _guard = operations::exclusive_soon().await?;
    plugins::antigravity_ui::start().await
}

#[tauri::command]
pub async fn antigravity_ui_stop() -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    plugins::antigravity_ui::stop()
}

#[tauri::command]
pub async fn antigravity_ui_config_save(cfg: plugins::antigravity_ui::UiConfig) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    plugins::antigravity_ui::save_config(&cfg)
}

#[tauri::command]
pub fn antigravity_ui_rules() -> Result<plugins::antigravity_ui::DangerRules> {
    plugins::antigravity_ui::load_rules()
}

#[tauri::command]
pub async fn antigravity_ui_rules_save(rules: plugins::antigravity_ui::DangerRules) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    plugins::antigravity_ui::save_rules(&rules)
}

/// 把规则恢复成内置那份（EasyAntigravity 出厂规则）。
#[tauri::command]
pub async fn antigravity_ui_rules_reset() -> Result<plugins::antigravity_ui::DangerRules> {
    let _guard = operations::exclusive_soon().await?;
    let rules = plugins::antigravity_ui::parse_rules(plugins::antigravity_ui::DEFAULT_RULES);
    plugins::antigravity_ui::save_rules(&rules)?;
    Ok(rules)
}

// ------------------------------------------------------------ Codex 出站与换出口插件

#[tauri::command]
pub fn codex_egress_status() -> plugins::PluginStatus {
    plugins::codex_egress::status()
}

#[tauri::command]
pub fn codex_egress_config() -> plugins::codex_egress::EgressConfig {
    plugins::codex_egress::load_config()
}

#[tauri::command]
pub async fn codex_egress_config_save(cfg: plugins::codex_egress::EgressConfig) -> Result<()> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(move || plugins::codex_egress::save_config(&cfg))
        .await
        .map_err(|e| crate::error::GateError::Other(e.to_string()))?
}

/// 启动插件（它会接管当前激活槽位 / 默认 `~/.codex` 的 Codex 配置并起服务）。
///
/// ⛔ 与核心的官方 turn-state 识别互斥（双向，另一半在 `station_turnstate_enable`）。
/// 0.24.7 起两边**都改同一个槽位的 `config.toml`**（识别改 `model_provider`，插件也接管它），
/// 同时开就是两个程序抢同一个文件。识别开着时请先在账户页「识别」弹窗里点「关闭识别」。
/// 看的是落盘的 marker（`turnstate_ops::takeover`），不只看内存里的路由状态 ——
/// 面板重启后后者归零，前者才是真相（启动时会自动关闭并恢复，这里再兜一道底）。
#[tauri::command]
pub async fn codex_egress_start(state: tauri::State<'_, AppState>) -> Result<String> {
    let _guard = operations::exclusive().await?;
    let armed = state
        .station
        .lock()
        .map_err(|_| crate::error::GateError::Other("中转站状态损坏".into()))?
        .official_codex_armed();
    if armed || qb_app::usecase::turnstate_ops::takeover().is_some() {
        return Err(crate::error::GateError::Other(
            "账户页的「识别（turn-state）」还开着。它和这个插件都要改同一个 Codex 槽位的配置，一次只能开一个：到账户页「识别」弹窗点「关闭识别」（会恢复槽位配置），再启动插件。"
                .into(),
        ));
    }
    tokio::task::spawn_blocking(plugins::codex_egress::start)
        .await
        .map_err(|e| crate::error::GateError::Other(e.to_string()))?
}

/// 一键下载、校验（SHA256SUMS）、解压并登记插件。进度走 `egress-install` 任务事件。
#[tauri::command]
pub async fn codex_egress_install(
    app: tauri::AppHandle,
) -> Result<plugins::codex_egress::EgressInstall> {
    let _guard = operations::exclusive().await?;
    let rep = Reporter::new(app, events::TASK_EGRESS_INSTALL, 5);
    match plugins::codex_egress::install(&rep).await {
        Ok(done) => Ok(done),
        Err(e) => {
            rep.fail(&e.to_string());
            Err(e)
        }
    }
}

/// 停止插件：结束本面板起的那份，再用它自己的 `restore` 把 Codex 配置恢复回去。
#[tauri::command]
pub async fn codex_egress_stop() -> Result<Vec<String>> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(plugins::codex_egress::stop)
        .await
        .map_err(|e| crate::error::GateError::Other(e.to_string()))?
}

#[tauri::command]
pub fn plugin_catalog_status() -> plugins::OfficialCatalogStatus {
    plugins::official_catalog_status()
}

/// 启动酒馆：先过 IP 门禁，再拉起桥接与酒馆，最后挂上看门狗。
///
/// 门禁不过就一个进程都不起 —— 这是接管启动链之后仍要守住的那条线。
///
/// `backend` 缺省是 Claude（0.20 起的老路：使用者自己的 `bridge.py`）；
/// `gpt` 走面板内置的桥接（`qb_app::gpt_bridge`，每个请求驱动一次官方 `codex exec`），
/// 酒馆本身两条路共用。GPT 那条按 `LaunchTarget::Codex.gated()` 决定验不验 IP ——
/// 使用者在「IP 锁」弹窗里把 GPT 移出门禁之后，它就跟 Codex 桌面端一样不验、不持租约。
#[tauri::command]
pub async fn plugin_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    backend: Option<plugins::sillytavern::TavernBackend>,
) -> Result<String> {
    use plugins::sillytavern::TavernBackend;
    let _guard = operations::exclusive().await?;
    let backend = backend.unwrap_or(TavernBackend::Claude);
    let gated = match backend {
        TavernBackend::Claude => true,
        TavernBackend::Gpt => crate::launch::LaunchTarget::Codex.gated(),
        // Gemini 桥接驱动的是 Google 账户（Gemini CLI），门禁跟反重力那一档走。
        TavernBackend::Gemini => crate::launch::LaunchTarget::Antigravity.gated(),
    };
    if gated {
        gate::open_authorized("sillytavern", &state.gate).await?;
    }

    let rep = Reporter::new(app.clone(), events::TASK_TAVERN_START, 5);
    let result = match backend {
        TavernBackend::Claude => plugins::sillytavern::start(&rep).await,
        TavernBackend::Gpt => {
            rep.phase(1, "起内置 GPT 桥接（官方 codex exec）");
            let bridge_was_running = qb_app::gpt_bridge::running();
            match qb_app::gpt_bridge::start().await {
                Ok(_) => match plugins::sillytavern::start_tavern_only(&rep).await {
                    Ok(u) => Ok(u),
                    Err(e) => {
                        // 酒馆没起来就把这次起的桥接收掉；本来就在跑的留着。
                        if !bridge_was_running {
                            let _ = qb_app::gpt_bridge::stop();
                        }
                        Err(e)
                    }
                },
                Err(e) => Err(e),
            }
        }
        TavernBackend::Gemini => {
            rep.phase(1, "起内置 Gemini 桥接（官方 Gemini CLI）");
            let bridge_was_running = qb_app::gemini_bridge::running();
            match qb_app::gemini_bridge::start().await {
                Ok(_) => match plugins::sillytavern::start_tavern_only(&rep).await {
                    Ok(u) => Ok(u),
                    Err(e) => {
                        if !bridge_was_running {
                            let _ = qb_app::gemini_bridge::stop();
                        }
                        Err(e)
                    }
                },
                Err(e) => Err(e),
            }
        }
    };
    let url = match result {
        Ok(u) => u,
        Err(e) => {
            // 起失败就把租约还回去，不能让门开着。
            if gated {
                let _ = gate::release_holder(&state.gate, "sillytavern");
            }
            rep.fail(&e.to_string());
            return Err(e);
        }
    };

    if gated {
        crate::app::start_watchdog(gate::watchdog::WatchMode::Cli, &state, Some(app));
    }

    Ok(url)
}

/// 停酒馆：两条桥一起停（Claude 的登记进程 + 内置 GPT 桥接），再还租约。
#[tauri::command]
pub async fn plugin_stop(state: tauri::State<'_, AppState>) -> Result<Vec<String>> {
    let _guard = operations::exclusive().await?;
    let gpt_was_running = qb_app::gpt_bridge::running();
    let gemini_was_running = qb_app::gemini_bridge::running();
    let mut done = plugins::sillytavern::stop().await?;
    qb_app::gpt_bridge::stop()?;
    if gpt_was_running {
        done.push("内置 GPT 桥接已停止".into());
    }
    qb_app::gemini_bridge::stop()?;
    if gemini_was_running {
        done.push("内置 Gemini 桥接已停止".into());
    }
    gate::release_holder(&state.gate, "sillytavern")?;
    Ok(done)
}

/// 内置 GPT 桥接的状态（在不在跑、端口、用哪个槽位、密钥文件在哪）。
#[tauri::command]
pub fn tavern_gpt_status() -> qb_app::gpt_bridge::GptBridgeStatus {
    qb_app::gpt_bridge::status()
}

/// 酒馆里 Custom 源要填的密钥。**只在使用者点「复制密钥」时读**，不随状态一起回。
#[tauri::command]
pub fn tavern_gpt_token() -> Result<String> {
    qb_app::gpt_bridge::token()
}

/// 内置 Gemini 桥接的状态。
#[tauri::command]
pub fn tavern_gemini_status() -> qb_app::gemini_bridge::GeminiBridgeStatus {
    qb_app::gemini_bridge::status()
}

#[tauri::command]
pub fn tavern_gemini_token() -> Result<String> {
    qb_app::gemini_bridge::token()
}

/// 当前 GPT 槽位的官方内部额度窗口（酒馆弹窗用）。
///
/// `refresh = false` 只读「最近一次」，**绝不联网**；`true` 才联网问一次 ——
/// 只有使用者点了刷新才传 `true`（2026-09-23 使用者定的）。
#[tauri::command]
pub async fn tavern_gpt_quota(
    refresh: bool,
) -> Result<Option<qb_app::usecase::tavern_quota::TavernGptQuota>> {
    qb_app::usecase::tavern_quota::gpt_quota(None, refresh).await
}

/// 指定 GPT 槽位的官方内部额度窗口，不改变激活账户。`refresh` 同上。
#[tauri::command]
pub async fn codex_quota(
    id: String,
    refresh: bool,
) -> Result<Option<qb_app::usecase::tavern_quota::TavernGptQuota>> {
    qb_app::usecase::tavern_quota::gpt_quota(Some(&id), refresh).await
}

/// 当前账户 Gemini CLI 那一半的 Code Assist 模型额度。`refresh` 同上。
#[tauri::command]
pub async fn tavern_gemini_quota(
    refresh: bool,
) -> Result<Option<qb_app::usecase::tavern_quota::TavernGeminiQuota>> {
    qb_app::usecase::tavern_quota::gemini_quota(refresh).await
}

/// 通过实际运行中的本机桥接做一轮最小角色扮演链路测试。
#[tauri::command]
pub async fn tavern_bridge_roleplay_test(
    provider: plugins::sillytavern::TavernBackend,
    fixture: Option<String>,
) -> Result<qb_app::usecase::tavern_quota::TavernRoleplayTest> {
    qb_app::usecase::tavern_quota::roleplay_test(provider, fixture).await
}

#[tauri::command]
pub fn tavern_config() -> plugins::sillytavern::TavernConfig {
    plugins::sillytavern::load_config()
}

/// 在本机找酒馆与桥接装在哪。**只读，不写配置** —— 采用哪一条由使用者点。
///
/// 故意不拿 `operations::exclusive`：它只读盘，跟切号 / 装机那几个互斥操作
/// 不冲突，而深扫最长要跑 90 秒 —— 占着那把锁会把别的操作全堵住。
#[tauri::command]
pub async fn tavern_locate(deep: bool) -> Result<plugins::tavern_locate::TavernSurvey> {
    plugins::sillytavern::locate(deep).await
}

#[tauri::command]
pub async fn tavern_config_save(cfg: plugins::sillytavern::TavernConfig) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    plugins::sillytavern::save_config(&cfg)
}

// ------------------------------------------------------------ Claude 桥的设置与监控（0.32.0）
//
// 这四条调的是使用者自己那份 `bridge.py` 的 HTTP 接口（只绑回环）。
// 原来这些旋钮只在它自己那个网页上，而那个网页要从酒馆的扩展抽屉里
// 用 iframe 嵌进去看。边界与三条硬要求见 `plugins::tavern_bridge_api` 的模块头。
//
// 都不拿独占锁：本机回环上的一次往返，别把看门狗巡检挡在外面。

#[tauri::command]
pub async fn tavern_bridge_health() -> Result<plugins::tavern_bridge_api::BridgeHealth> {
    plugins::tavern_bridge_api::health().await
}

#[tauri::command]
pub async fn tavern_bridge_settings() -> Result<plugins::tavern_bridge_api::BridgeSettingsView> {
    plugins::tavern_bridge_api::settings().await
}

/// 存设置。⚠ `bridge.py` 在这七项里**任何一项变了**都会退掉当前那条
/// SDK 会话，界面上写明了。
#[tauri::command]
pub async fn tavern_bridge_settings_save(
    settings: plugins::tavern_bridge_api::BridgeSettings,
) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    plugins::tavern_bridge_api::save_settings(&settings).await
}

#[tauri::command]
pub async fn tavern_bridge_telemetry(
    offset: i64,
    limit: i64,
) -> Result<plugins::tavern_bridge_api::BridgeTelemetry> {
    plugins::tavern_bridge_api::telemetry(offset, limit.clamp(1, 200)).await
}

/// 清空调用日志。**不可逆** —— 界面上走 ConfirmDialog。
#[tauri::command]
pub async fn tavern_bridge_telemetry_clear() -> Result<i64> {
    let _guard = operations::exclusive_soon().await?;
    plugins::tavern_bridge_api::clear_telemetry().await
}

// ------------------------------------------------------------ 酒馆资产

#[tauri::command]
pub fn tavern_assets() -> Vec<plugins::tavern_assets::CategoryListing> {
    plugins::tavern_assets::list_all()
}

#[tauri::command]
pub async fn tavern_backup() -> Result<plugins::tavern_assets::BackupEntry> {
    let _guard = operations::exclusive_soon().await?;
    plugins::tavern_assets::backup()
}

#[tauri::command]
pub fn tavern_backups() -> Vec<plugins::tavern_assets::BackupEntry> {
    plugins::tavern_assets::list_backups()
}

#[tauri::command]
pub async fn tavern_restore(backup_id: String) -> Result<String> {
    let _guard = operations::exclusive_soon().await?;
    plugins::tavern_assets::restore(&backup_id)
}
