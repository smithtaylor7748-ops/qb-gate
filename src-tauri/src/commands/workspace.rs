//! 工作区：服务商 · 凭证 · 会话，官方目录迁移，以及扩展中心。
//!
//! ⛔ 2026-09-27 删掉了一批只有「服务商与环境」旧页在用、或者从来没有界面的命令：
//! 环境的保存 / 预览 / 应用 / 回滚、中转配置导出导入、引用检查与删除、草稿诊断
//! （`diagnostic_run` / `diagnostic_history`）、启动方案、放弃会话记录、取消操作。
//! 中转站页的站点与 Key 走 `provider_save` / `credential_save`，环境由本机路由自己建
//! （`workspace::router_environment`）。加回来之前先想清楚界面上谁调它 ——
//! `tests/architecture.rs` 的 `every_registered_command_is_called_by_the_frontend` 盯着。
use crate::{domain::*, error::Result, events, operations, workspace};

#[tauri::command]
pub fn workspace_state() -> Result<Workspace> {
    workspace::snapshot()
}
#[tauri::command]
pub async fn provider_save(app: tauri::AppHandle, provider: Provider) -> Result<Provider> {
    let _guard = operations::exclusive_soon().await?;
    let p = workspace::provider_save(provider)?;
    operations::changed(&events::ui(&app), "provider", &p.id);
    Ok(p)
}
#[tauri::command]
pub async fn credential_save(
    app: tauri::AppHandle,
    credential: Credential,
    action: String,
    key: Option<String>,
) -> Result<Credential> {
    let _guard = operations::exclusive_soon().await?;
    let c = workspace::credential_save(credential, &action, key.as_deref())?;
    operations::changed(&events::ui(&app), "credential", &c.id);
    Ok(c)
}
#[tauri::command]
pub async fn session_launch(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
    client: Client,
    kind: IdentityKind,
    id: String,
    working_dir: Option<String>,
) -> Result<Session> {
    let _guard = operations::exclusive().await?;
    let mut task = operations::Run::start(events::sink(&app), "启动会话", &id)?;
    task.phase("检查身份、配置与门禁", 20)?;
    let result = workspace::launch(client, kind, &id, working_dir, &state.gate).await;
    let session = task.finish(result)?;
    // 只要这个可执行文件归门禁管（= 起的时候拿了租约），就得有看门狗盯着 ——
    // 中转会话不收，但它持着的租约同样要在 IP 变了之后被收回并重新上锁。
    // 只走中转的两档（阶段 4）没有启动目标，`workspace::launch` 已经拒绝了，走不到这里。
    if let Some(target) = crate::launch::LaunchTarget::of(client) {
        if target.gated() {
            // 档位跟着启动目标走，不能硬写 Cli —— 桌面端那一档的口径不一样。
            crate::app::start_watchdog(target.watch_mode(), &state, Some(app.clone()));
        }
    }
    operations::changed(&events::ui(&app), "session", &session.id);
    Ok(session)
}
#[tauri::command]
pub async fn session_stop(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
    id: String,
) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    let mut task = operations::Run::start(events::sink(&app), "停止会话", &id)?;
    let result = (|| {
        crate::sessions::stop(&id)?;
        crate::gate::release_holder(&state.gate, &id)
    })();
    let result = task.finish(result);
    // 失败也要广播：原来这里是 `?` 提前返回，停不掉的时候界面连状态都不刷新。
    operations::changed(&events::ui(&app), "session", &id);
    result
}

#[tauri::command]
pub fn extension_catalog() -> Result<Vec<ExtensionManifest>> {
    crate::extensions::catalog()
}
// 原来这里有 `extension_import_skills`（把 Skills 导进扩展目录）。2026-10-26 起 Skills 在「Skills」分页管
// （阶段 5，M2：`skills_discover` / `skills_install`），它跟着撤了。
// 原来这里有 `extension_import_mcp`（把一段 MCP 配置导进扩展目录）。2026-10-26 起 MCP 在「MCP」分页统一管
// （阶段 5，M1：`mcp_save` / `mcp_import`），它跟着撤了。
#[tauri::command]
pub async fn extension_preview(
    request: crate::extensions::InstallRequest,
) -> Result<crate::extensions::InstallPreview> {
    crate::extensions::preview(&request).await
}
#[tauri::command]
pub async fn extension_install(
    app: tauri::AppHandle,
    request: crate::extensions::InstallRequest,
) -> Result<ExtensionInstallation> {
    let _guard = operations::exclusive().await?;
    let mut task = operations::Run::start(events::sink(&app), "安装扩展", &request.extension_id)?;
    let result = crate::extensions::install(request).await;
    let result = task.finish(result);
    operations::changed(&events::ui(&app), "installation", "");
    result
}
#[tauri::command]
pub async fn extension_uninstall(app: tauri::AppHandle, id: String) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    let mut task = operations::Run::start(events::sink(&app), "卸载扩展", &id)?;
    let result = task.finish(crate::extensions::uninstall(&id));
    // 同 session_stop：失败也要广播，否则界面停在卸载前的状态。
    operations::changed(&events::ui(&app), "installation", &id);
    result
}
#[tauri::command]
pub async fn extension_check(
    app: tauri::AppHandle,
    request: crate::extensions::InstallRequest,
) -> Result<crate::extensions::McpCheck> {
    let mut task =
        operations::Run::start(events::sink(&app), "MCP 连接测试", &request.extension_id)?;
    let result = crate::extensions::check_mcp(request).await;
    task.finish(result)
}

/// 切 Claude 账户的确认框报数用：跟 `app::clear_for_switch` 真去关的是**同一个范围**
/// （只收 Claude 的，中转与反重力不碰）。不发任务进度事件 —— 原来切换框借用的是
/// 一键关闭的扫描命令，一打开切换框，「一键关闭」那块磁贴就跟着转、扫描失败还会变红。
#[tauri::command]
pub async fn official_switch_preview() -> Result<crate::killswitch::KillReport> {
    crate::killswitch::preview_for_switch().await
}

#[tauri::command]
pub async fn extension_check_updates(app: tauri::AppHandle) -> Result<Vec<String>> {
    let _guard = operations::exclusive().await?;
    let mut task = operations::Run::start(events::sink(&app), "检查扩展更新", "")?;
    let result = task.finish(crate::extensions::check_updates().await);
    operations::changed(&events::ui(&app), "catalog", "");
    result
}

#[tauri::command]
pub fn official_configuration_preview(
    client: Client,
    id: String,
) -> Result<Vec<workspace::ConfigPreview>> {
    workspace::official_preview(client, &id)
}
#[tauri::command]
pub async fn official_configuration_migrate(
    app: tauri::AppHandle,
    client: Client,
    id: String,
    fingerprint: String,
) -> Result<String> {
    let _guard = operations::exclusive().await?;
    // 只看 Claude 的进程：反重力开着跟迁 Claude Code 的配置毫无关系（原来它也会挡住这一步）。
    if client == Client::ClaudeCode
        && !crate::killswitch::preview_for_switch()
            .await?
            .targets
            .is_empty()
    {
        return Err(crate::error::GateError::Other(
            "请先关闭相关官方进程，再迁移配置".into(),
        ));
    }
    let result = workspace::official_migrate(client, &id, &fingerprint)?;
    operations::changed(&events::ui(&app), "environment", "");
    Ok(result)
}

// 原来这里有 `extension_connect_application`（扩展中心酒馆页那颗「接入已配置的本地安装」）。
// 2026-09-29 酒馆变成面板的内置功能、从扩展中心撤掉，它跟着删了 —— 见 `extensions::RETIRED`。

// ------------------------------------------------------------------ MCP 面板（阶段 5，cc-switch 的 M1）

/// MCP 服务器和能写进去的目标。第一次打开时把原来扩展中心装的 MCP 迁进来。
#[tauri::command]
pub fn mcp_list() -> Result<qb_app::usecase::mcp_ops::McpView> {
    qb_app::usecase::mcp_ops::list(&crate::repository::Repository::open()?)
}

/// 新建或修改一条，按它勾的目标写进各个配置（`previous_id` 是改名前的名字）。
#[tauri::command]
pub fn mcp_save(
    server: qb_app::mcp::McpServer,
    previous_id: Option<String>,
) -> Result<qb_app::usecase::mcp_ops::McpView> {
    qb_app::usecase::mcp_ops::save(
        &crate::repository::Repository::open()?,
        server,
        previous_id.as_deref(),
    )
}

/// 删掉一条（勾着的目标里一起删，使用者手写的别的 MCP 不动）。
#[tauri::command]
pub fn mcp_delete(id: String) -> Result<qb_app::usecase::mcp_ops::McpView> {
    qb_app::usecase::mcp_ops::delete(&crate::repository::Repository::open()?, &id)
}

/// 勾上 / 取消一个目标。官方目录和各个工具都要使用者自己勾。
#[tauri::command]
pub fn mcp_set_target(
    id: String,
    target: String,
    enabled: bool,
) -> Result<qb_app::usecase::mcp_ops::McpView> {
    qb_app::usecase::mcp_ops::set_target(
        &crate::repository::Repository::open()?,
        &id,
        &target,
        enabled,
    )
}

/// 把一个目标里现有的 MCP 导进面板（不写任何文件）。
#[tauri::command]
pub fn mcp_import(target: String) -> Result<qb_app::usecase::mcp_ops::McpImportReport> {
    qb_app::usecase::mcp_ops::import(&crate::repository::Repository::open()?, &target)
}

// ------------------------------------------------------------------ Skills 面板（阶段 5，cc-switch 的 M2）

/// 装好的 Skill、目标、目标里面板没管着的、卸载留下的备份。第一次打开时把原来扩展中心装的 Skill 迁进来。
#[tauri::command]
pub fn skills_list() -> Result<qb_app::usecase::skill_ops::SkillsView> {
    qb_app::usecase::skill_ops::list(&crate::repository::Repository::open()?)
}

/// GitHub 地址 / 本地目录里有哪些 Skill（只读元数据，不执行仓库脚本）。
#[tauri::command]
pub async fn skills_discover(source: String) -> Result<Vec<qb_app::skills::SkillCandidate>> {
    qb_app::skills::discover(&source).await
}

/// 一个 ZIP 里有哪些 Skill（只读）。
#[tauri::command]
pub fn skills_discover_zip(path: String) -> Result<Vec<qb_app::skills::SkillCandidate>> {
    qb_app::usecase::skill_ops::discover_zip(&path)
}

/// 装挑好的几个，复制到勾选的目标。
#[tauri::command]
pub async fn skills_install(
    picked: Vec<qb_app::skills::SkillCandidate>,
    targets: Vec<String>,
) -> Result<qb_app::usecase::skill_ops::SkillsView> {
    qb_app::usecase::skill_ops::install(picked, targets).await
}

/// 从 ZIP 装挑好的几个。
#[tauri::command]
pub fn skills_install_zip(
    path: String,
    names: Vec<String>,
    targets: Vec<String>,
) -> Result<qb_app::usecase::skill_ops::SkillsView> {
    qb_app::usecase::skill_ops::install_zip(
        &crate::repository::Repository::open()?,
        &path,
        names,
        targets,
    )
}

/// 勾上 / 取消一个目标（取消时那一份挪进备份）。
#[tauri::command]
pub fn skills_set_target(
    id: String,
    target: String,
    enabled: bool,
) -> Result<qb_app::usecase::skill_ops::SkillsView> {
    qb_app::usecase::skill_ops::set_target(
        &crate::repository::Repository::open()?,
        &id,
        &target,
        enabled,
    )
}

/// 卸载（文件挪进备份，能还原）。
#[tauri::command]
pub fn skills_uninstall(id: String) -> Result<qb_app::usecase::skill_ops::SkillsView> {
    qb_app::usecase::skill_ops::uninstall(&crate::repository::Repository::open()?, &id)
}

/// 还原一次卸载。
#[tauri::command]
pub fn skills_restore(backup: String) -> Result<qb_app::usecase::skill_ops::SkillsView> {
    qb_app::usecase::skill_ops::restore(&crate::repository::Repository::open()?, &backup)
}

/// 把目标里一个面板没管着的 Skill 导进来。
#[tauri::command]
pub fn skills_import(
    target: String,
    directory: String,
) -> Result<qb_app::usecase::skill_ops::SkillsView> {
    qb_app::usecase::skill_ops::import(&crate::repository::Repository::open()?, &target, &directory)
}

/// 看哪些有新的一版（联网，只在点的时候）。
#[tauri::command]
pub async fn skills_check_updates() -> Result<Vec<qb_app::usecase::skill_ops::SkillUpdate>> {
    qb_app::usecase::skill_ops::check_updates().await
}

/// 换成新的一版。
#[tauri::command]
pub async fn skills_update(id: String) -> Result<qb_app::usecase::skill_ops::SkillsView> {
    qb_app::usecase::skill_ops::update(id).await
}

/// 在 skills.sh 上搜（联网，只在点的时候）。
#[tauri::command]
pub async fn skills_search(query: String) -> Result<Vec<qb_app::skills::SkillsShHit>> {
    qb_app::skills::search_skills_sh(&query).await
}

// ------------------------------------------------------------------ 提示词库（阶段 5，cc-switch 的 M3）

/// 提示词和能写进去的目标（每个目标各自用哪一条）。
#[tauri::command]
pub fn prompts_list() -> Result<qb_app::usecase::prompt_ops::PromptsView> {
    qb_app::usecase::prompt_ops::list(&crate::repository::Repository::open()?)
}

/// 新建（`id` 留空）或修改；正在用的目标跟着重写。
#[tauri::command]
pub fn prompts_save(
    prompt: qb_app::usecase::prompt_ops::Prompt,
) -> Result<qb_app::usecase::prompt_ops::PromptsView> {
    qb_app::usecase::prompt_ops::save(&crate::repository::Repository::open()?, prompt)
}

/// 删掉一条（正在用的不让删）。
#[tauri::command]
pub fn prompts_delete(id: String) -> Result<qb_app::usecase::prompt_ops::PromptsView> {
    qb_app::usecase::prompt_ops::delete(&crate::repository::Repository::open()?, &id)
}

/// 在一个目标用上这一条（先把文件原来的内容存回库里）。
#[tauri::command]
pub fn prompts_enable(
    target: String,
    id: String,
) -> Result<qb_app::usecase::prompt_ops::PromptsView> {
    qb_app::usecase::prompt_ops::enable(&crate::repository::Repository::open()?, &target, &id)
}

/// 把一个目标里现在的文件导成一条（不写文件）。
#[tauri::command]
pub fn prompts_import(target: String) -> Result<qb_app::usecase::prompt_ops::PromptsView> {
    qb_app::usecase::prompt_ops::import(&crate::repository::Repository::open()?, &target)
}
