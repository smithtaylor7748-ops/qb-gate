//! 插件（本机应用接入）、酒馆与三条桥相关的命令。
//!
//! 酒馆 2026-09-29 起是面板的内置功能（不在扩展中心）。同一天第三轮对调之后：用酒馆（控制台、角色卡、
//! 世界书、扩展、回收站、停止）在账户页酒馆磁贴右上角的「酒馆」弹窗里；酒馆本体的下载 / 更新 / 指定版本 /
//! 卸载在软件页的 SillyTavern 卡；三家的桥接设置在软件页 Claude Code / Codex CLI / Gemini CLI 三张卡的最下面。
//! 命令名里的 `plugin_` 是历史名字，没改。

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

// ------------------------------------------------------------ Claude 汉化残留（2026-09-27）
//
// Claude 汉化撤掉了（官方账户下必然被改回英文，为什么见 `plugins::claude_zh` 文件头），
// 只剩一次性清理 0.25.4–0.25.5 留下的东西。

/// 还剩什么：各个 `app-*` 里上游的中文文件与备份、中文界面语言的资料、上游建的 `Claude-3p`、
/// 面板下载过的上游副本。**只读，不联网**。
#[tauri::command]
pub async fn claude_zh_leftovers() -> plugins::claude_zh::ClaudeZhLeftovers {
    qb_app::usecase::claude_zh_ops::leftovers().await
}

/// 一次性清理。要往 Claude 现在用的那一版里写时**会关掉 Claude 桌面端**（含 Code 页里的会话），
/// 界面事前说明；不用写就不关。进度走 `claude-zh` 任务。
#[tauri::command]
pub async fn claude_zh_cleanup(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<qb_app::usecase::claude_zh_ops::ClaudeZhOutcome> {
    use qb_app::usecase::claude_zh_ops as ops;
    let rep = Reporter::new(app.clone(), events::TASK_CLAUDE_ZH, ops::TOTAL);
    let result = async {
        let _guard = operations::exclusive().await?;
        ops::cleanup(&state.gate, &rep).await
    }
    .await;
    match &result {
        Ok(o) if o.ok => rep.done("汉化残留清掉了"),
        Ok(o) => rep.fail(o.lines.last().map(String::as_str).unwrap_or("没全清掉")),
        Err(e) => rep.fail(&e.to_string()),
    }
    operations::changed(&events::ui(&app), "session", "claude");
    result
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

/// 停酒馆：三条桥一起停（Claude 的登记进程 + 内置 GPT / Gemini 桥接），再还租约。
/// 入口在三个账户页的「酒馆」弹窗底部（2026-09-29；第三轮之前那个弹窗是桥接设置）。
#[tauri::command]
pub async fn plugin_stop(state: tauri::State<'_, AppState>) -> Result<Vec<String>> {
    let _guard = operations::exclusive().await?;
    stop_tavern_all(&state).await
}

/// [`plugin_stop`] 的身子，软件页装 / 更新 / 卸载 SillyTavern 之前也要先停。没在跑时什么都不做。
///
/// ⛔ **不拿独占锁**：调用方都已经拿着了，tokio 的 Mutex 不可重入，在这里再拿一次就是永远等着
/// （`architecture.rs` 的 `helpers_called_under_the_exclusive_lock_never_take_it_again` 那一族）。
async fn stop_tavern_all(state: &AppState) -> Result<Vec<String>> {
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

// ------------------------------------------------------------ 软件页：托管 SillyTavern（2026-09-29）
//
// 使用者：「酒馆变成本项目的功能之一，软件页增加下载酒馆的功能，然后被本项目接管」。
// 下载与位置表在 `qb_install::install::{sillytavern, sillytavern_setup}`，边界写在那个模块头。
// 这一层只管：拿锁、先停酒馆、装完把酒馆配置指过去（`plugins::sillytavern::adopt`）、审计与进度收尾。

/// 酒馆端口上有人在听、又不是面板起的：导入 / 更新 / 卸载都先不做。
fn refuse_if_foreign_tavern(what: &str) -> Result<()> {
    if plugins::sillytavern::foreign_tavern_running() {
        return Err(crate::error::GateError::Other(format!(
            "酒馆端口上有一份不是面板起的酒馆在跑。先关掉它自己的那个窗口再{what} —— \
             开着的时候它的文件换不掉、删不干净，从它那里复制也可能复制到写了一半的聊天。面板不替你关它。"
        )));
    }
    Ok(())
}

fn finish_setup(
    app: &tauri::AppHandle,
    rep: &Reporter,
    what: &str,
    result: Result<(
        qb_install::install::sillytavern_setup::Installed,
        Option<String>,
    )>,
) -> Result<String> {
    match result {
        Ok((inst, note)) => {
            let mut lines = vec![format!(
                "SillyTavern {} 已{what}：{}",
                inst.version,
                inst.dir.display()
            )];
            lines.extend(inst.notes);
            lines.extend(note);
            let msg = lines.join("；");
            crate::audit::write(&format!("酒馆{what}：{msg}"));
            if let Some(sha) = inst.log.iter().find(|l| l.contains("SHA-256")) {
                crate::audit::write(sha);
            }
            operations::changed(&events::ui(app), "session", "sillytavern");
            rep.done(&msg);
            Ok(msg)
        }
        Err(e) => {
            let msg = e.to_string();
            crate::audit::write(&format!("酒馆{what}失败：{msg}"));
            rep.fail(&msg);
            Err(e)
        }
    }
}

/// 软件页 SillyTavern 卡要的全部事实。**不联网。**
#[tauri::command]
pub async fn tavern_setup_status() -> plugins::sillytavern::TavernSetupStatus {
    plugins::sillytavern::setup_status().await
}

/// 问 SillyTavern 官方 GitHub 最新的正式版。**联网** —— 只在使用者点「检查新版本」、
/// 打开安装 / 更新确认框时发，没有定时器。
#[tauri::command]
pub async fn tavern_latest() -> Result<qb_install::install::sillytavern_setup::TavernRelease> {
    qb_install::install::sillytavern_setup::latest().await
}

/// 下载安装一份面板托管的 SillyTavern；`import_from` 给了就从那份复制数据（**源不动**）；
/// `install_node` 勾了才在 Node.js 缺失或不够新时用 winget 装 LTS。进度走 `install-sillytavern`。
#[tauri::command]
pub async fn tavern_install(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    import_from: Option<String>,
    install_node: bool,
    tag: Option<String>,
) -> Result<String> {
    use qb_install::install::sillytavern_setup as setup;
    let _guard = operations::exclusive().await?;
    let rep = Reporter::new(app.clone(), events::TASK_INSTALL_SILLYTAVERN, setup::TOTAL);
    let import = import_from
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from);
    let result = async {
        if import.is_some() {
            refuse_if_foreign_tavern("导入")?;
        }
        stop_tavern_all(&state).await?;
        let inst = setup::install(import.as_deref(), install_node, tag.as_deref(), &rep).await?;
        let note = plugins::sillytavern::adopt(&inst.dir, inst.config_port)?;
        Ok((inst, note))
    }
    .await;
    finish_setup(&app, &rep, "装好", result)
}

/// 把面板装的那份换成最新的正式版（`tag` 给了就换成那一版，可以更旧）：数据复制过去，
/// 原来那份留在 `sillytavern.previous`（只留一份）。
#[tauri::command]
pub async fn tavern_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    install_node: bool,
    tag: Option<String>,
) -> Result<String> {
    use qb_install::install::{sillytavern as st, sillytavern_setup as setup};
    let _guard = operations::exclusive().await?;
    let rep = Reporter::new(app.clone(), events::TASK_INSTALL_SILLYTAVERN, setup::TOTAL);
    let result = async {
        let dir = plugins::sillytavern::load_config_checked()?.sillytavern_root;
        if !st::is_managed(&dir) {
            return Err(crate::error::GateError::Other(
                "现在用的这份酒馆不是面板装的，面板不替它更新（用它自己的方式更新，例如 git pull）。"
                    .into(),
            ));
        }
        refuse_if_foreign_tavern("更新")?;
        stop_tavern_all(&state).await?;
        let inst = setup::update(&dir, install_node, tag.as_deref(), &rep).await?;
        let note = plugins::sillytavern::adopt(&inst.dir, inst.config_port)?;
        Ok((inst, note))
    }
    .await;
    finish_setup(&app, &rep, "更新", result)
}

/// 卸载面板装的那份：先把数据复制备份，再删；配置里指着它的路径清空。
///
/// 卸哪一份：配置里在用的那份是面板装的就卸它；否则卸托管目录里那份（使用者换回了自己的酒馆，
/// 面板装的那份闲在那儿）。都不是面板装的就不卸 —— 使用者自己的酒馆面板一个字不删。
#[tauri::command]
pub async fn tavern_uninstall(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<qb_install::install::sillytavern_setup::TavernUninstalled> {
    use qb_install::install::{sillytavern as st, sillytavern_setup as setup};
    let _guard = operations::exclusive().await?;
    let current = plugins::sillytavern::load_config_checked()?.sillytavern_root;
    let dir = if st::is_managed(&current) {
        current
    } else if st::is_managed(&st::managed_dir()) {
        st::managed_dir()
    } else {
        return Err(crate::error::GateError::Other(
            "没有面板装的那份酒馆可卸（你自己的那份面板不删）。".into(),
        ));
    };
    refuse_if_foreign_tavern("卸载")?;
    stop_tavern_all(&state).await?;
    let backups = crate::paths::state_dir().join("tavern-uninstall");
    let target = dir.clone();
    let result = tokio::task::spawn_blocking(move || setup::uninstall(&target, &backups))
        .await
        .map_err(|e| crate::error::GateError::Other(e.to_string()))?;
    match &result {
        Ok(r) => {
            plugins::sillytavern::release_paths_if(&dir)?;
            crate::audit::write(&format!(
                "卸载了面板装的酒馆 {}，数据备份在 {}",
                dir.display(),
                r.backup
            ));
        }
        Err(e) => crate::audit::write(&format!("卸载酒馆失败：{e}")),
    }
    operations::changed(&events::ui(&app), "session", "sillytavern");
    result
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

/// 当前 GPT 槽位的官方内部额度窗口（软件页 Codex CLI 卡最下面「酒馆 · GPT 桥接设置」用）。
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
    // bridge.py 2.4.0 的 MAX_TELEMETRY_PAGE_SIZE 是 100，超过就回 400。
    plugins::tavern_bridge_api::telemetry(offset.max(0), limit.clamp(1, 100)).await
}

/// 清空调用日志。**不可逆** —— 界面上走 ConfirmDialog。
#[tauri::command]
pub async fn tavern_bridge_telemetry_clear() -> Result<i64> {
    let _guard = operations::exclusive_soon().await?;
    plugins::tavern_bridge_api::clear_telemetry().await
}

/// 一条调用的全文（原来是 `bridge.py` 监控页的「展开查看输入/输出」）。只在使用者点「展开」时读。
#[tauri::command]
pub async fn tavern_bridge_call_detail(
    id: String,
) -> Result<plugins::tavern_bridge_api::BridgeCallDetail> {
    plugins::tavern_bridge_api::call_detail(&id).await
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

// ------------------------------------------------------------ 软件页 SillyTavern 卡：版本（2026-09-29）

/// 最近的正式版清单（发布页的 Atom，最多 10 个，带更新说明）。**联网**，只在使用者点了之后发。
#[tauri::command]
pub async fn tavern_releases(
) -> Result<Vec<qb_install::install::sillytavern_setup::TavernReleaseNote>> {
    qb_install::install::sillytavern_setup::releases().await
}

/// 某一个正式版（「指定版本」里手填的标签也走这里核格式、读它要多新的 Node）。**联网**。
#[tauri::command]
pub async fn tavern_release(
    tag: String,
) -> Result<qb_install::install::sillytavern_setup::TavernRelease> {
    qb_install::install::sillytavern_setup::release(Some(&tag)).await
}

/// 「换回留着的那一版」：面板装的那份与 `sillytavern.previous` 对调。`carry_data` = 把现在的数据带过去。
#[tauri::command]
pub async fn tavern_swap_previous(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    carry_data: bool,
) -> Result<String> {
    use qb_install::install::{sillytavern as st, sillytavern_setup as setup};
    let _guard = operations::exclusive().await?;
    let rep = Reporter::new(app.clone(), events::TASK_INSTALL_SILLYTAVERN, 1);
    rep.phase(1, "换回留着的那一版");
    let result = async {
        let dir = plugins::sillytavern::load_config_checked()?.sillytavern_root;
        if !st::is_managed(&dir) {
            return Err(crate::error::GateError::Other(
                "现在用的这份酒馆不是面板装的，面板不替它换版本。".into(),
            ));
        }
        refuse_if_foreign_tavern("换版本")?;
        stop_tavern_all(&state).await?;
        let backups = crate::paths::state_dir().join("tavern-rollback");
        let target = dir.clone();
        let inst = tokio::task::spawn_blocking(move || {
            setup::swap_with_previous(&target, carry_data, &backups)
        })
        .await
        .map_err(|e| crate::error::GateError::Other(e.to_string()))??;
        let note = plugins::sillytavern::adopt(&inst.dir, inst.config_port)?;
        Ok((inst, note))
    }
    .await;
    finish_setup(&app, &rep, "换好", result)
}

// ------------------------------------------------------------ 「酒馆」弹窗：控制台（2026-09-29；第二轮在软件页的卡上）

/// 读一次酒馆的控制台（面板起的那份的输出）。`run` / `offset` 是上一次读到哪；本机文件，不联网。
#[tauri::command]
pub fn tavern_console(
    previous: bool,
    run: Option<String>,
    offset: u64,
) -> Result<plugins::tavern_console::ConsoleChunk> {
    plugins::sillytavern::console(previous, run.as_deref(), offset)
}

// ------------------------------------------------------------ 「酒馆」弹窗：角色卡 · 世界书 · 扩展 · 回收站（2026-09-29；第二轮在软件页的卡上）
//
// 读的命令不拿锁；改文件的拿 `exclusive_soon`（都是几秒内的事）；git 的**不拿**全局锁 ——
// 克隆可能要几分钟，而看门狗每 5 秒要那把锁（见 `tavern_library::git_lock`）。

fn paths(v: Vec<String>) -> Vec<std::path::PathBuf> {
    v.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .collect()
}

fn joined<T>(r: std::result::Result<Result<T>, tokio::task::JoinError>) -> Result<T> {
    r.map_err(|e| crate::error::GateError::Other(e.to_string()))?
}

#[tauri::command]
pub async fn tavern_characters() -> Result<Vec<plugins::tavern_library::CharacterCard>> {
    joined(tokio::task::spawn_blocking(plugins::tavern_library::list_characters).await)
}

#[tauri::command]
pub async fn tavern_character_detail(
    file: String,
) -> Result<plugins::tavern_library::CharacterDetail> {
    joined(
        tokio::task::spawn_blocking(move || plugins::tavern_library::character_detail(&file)).await,
    )
}

#[tauri::command]
pub fn tavern_character_thumbnail(file: String) -> Result<Option<String>> {
    plugins::tavern_library::character_thumbnail(&file)
}

#[tauri::command]
pub async fn tavern_characters_import(
    files: Vec<String>,
) -> Result<plugins::tavern_library::ImportReport> {
    let _guard = operations::exclusive_soon().await?;
    plugins::tavern_library::import_characters(&paths(files))
}

#[tauri::command]
pub async fn tavern_character_delete(
    file: String,
    with_chats: bool,
) -> Result<plugins::tavern_library::TrashEntry> {
    let _guard = operations::exclusive_soon().await?;
    plugins::tavern_library::delete_character(&file, with_chats)
}

#[tauri::command]
pub async fn tavern_worlds() -> Result<Vec<plugins::tavern_library::WorldInfo>> {
    joined(tokio::task::spawn_blocking(plugins::tavern_library::list_worlds).await)
}

#[tauri::command]
pub async fn tavern_world_detail(file: String) -> Result<plugins::tavern_library::WorldDetail> {
    joined(tokio::task::spawn_blocking(move || plugins::tavern_library::world_detail(&file)).await)
}

#[tauri::command]
pub async fn tavern_worlds_import(
    files: Vec<String>,
) -> Result<plugins::tavern_library::ImportReport> {
    let _guard = operations::exclusive_soon().await?;
    plugins::tavern_library::import_worlds(&paths(files))
}

#[tauri::command]
pub async fn tavern_world_delete(file: String) -> Result<plugins::tavern_library::TrashEntry> {
    let _guard = operations::exclusive_soon().await?;
    plugins::tavern_library::delete_world(&file)
}

#[tauri::command]
pub async fn tavern_extensions() -> Result<plugins::tavern_library::TavernExtensionList> {
    joined(tokio::task::spawn_blocking(plugins::tavern_library::list_extensions).await)
}

/// **联网**（`git fetch`），只在点「检查更新」时发。
#[tauri::command]
pub async fn tavern_extension_check(
    name: String,
    scope: plugins::tavern_library::ExtensionScope,
) -> Result<plugins::tavern_library::ExtensionCheck> {
    plugins::tavern_library::check_extension(&name, scope).await
}

/// **联网**（`git pull --ff-only`）。
#[tauri::command]
pub async fn tavern_extension_update(
    name: String,
    scope: plugins::tavern_library::ExtensionScope,
) -> Result<String> {
    plugins::tavern_library::update_extension(&name, scope).await
}

/// **联网**（`git clone --depth 1`），地址是使用者自己填的（只收 https）。
#[tauri::command]
pub async fn tavern_extension_install(url: String) -> Result<String> {
    plugins::tavern_library::install_extension(&url).await
}

#[tauri::command]
pub async fn tavern_extension_delete(
    name: String,
    scope: plugins::tavern_library::ExtensionScope,
) -> Result<plugins::tavern_library::TrashEntry> {
    let _guard = operations::exclusive_soon().await?;
    plugins::tavern_library::delete_extension(&name, scope)
}

#[tauri::command]
pub fn tavern_trash() -> Vec<plugins::tavern_library::TrashEntry> {
    plugins::tavern_library::Trash::default_root().list()
}

#[tauri::command]
pub async fn tavern_trash_restore(id: String) -> Result<plugins::tavern_library::TrashEntry> {
    let _guard = operations::exclusive_soon().await?;
    let entry = plugins::tavern_library::Trash::default_root().restore(&id)?;
    crate::audit::write(&format!("酒馆：回收站的 {id} 放回去了"));
    Ok(entry)
}

/// 真删：`id` 给了删那一项，不给清空整个回收站。
#[tauri::command]
pub async fn tavern_trash_purge(id: Option<String>) -> Result<u32> {
    let _guard = operations::exclusive_soon().await?;
    let n = plugins::tavern_library::Trash::default_root().purge(id.as_deref())?;
    crate::audit::write(&format!("酒馆：回收站删掉了 {n} 项"));
    Ok(n)
}
