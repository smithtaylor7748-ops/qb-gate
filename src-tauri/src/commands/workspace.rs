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
pub async fn environment_save(
    app: tauri::AppHandle,
    environment: Environment,
) -> Result<Environment> {
    let _guard = operations::exclusive_soon().await?;
    let e = workspace::environment_save(environment)?;
    operations::changed(&events::ui(&app), "environment", &e.id);
    Ok(e)
}
#[tauri::command]
pub fn environment_preview(id: String) -> Result<Vec<workspace::ConfigPreview>> {
    workspace::preview(&id)
}
#[tauri::command]
pub async fn environment_apply(
    app: tauri::AppHandle,
    id: String,
    fingerprint: String,
) -> Result<Environment> {
    let _guard = operations::exclusive_soon().await?;
    let mut task = operations::Run::start(events::sink(&app), "应用配置", &id)?;
    task.phase("校验并提交配置", 40)?;
    let result = task.finish(workspace::apply_reviewed(&id, Some(&fingerprint)));
    operations::changed(&events::ui(&app), "environment", &id);
    result
}
#[tauri::command]
pub fn workspace_references(kind: String, id: String) -> Result<Vec<String>> {
    workspace::references(&kind, &id)
}
#[tauri::command]
pub async fn workspace_remove(app: tauri::AppHandle, kind: String, id: String) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    workspace::remove(&kind, &id)?;
    operations::changed(&events::ui(&app), &kind, &id);
    Ok(())
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
    let target = crate::launch::LaunchTarget::of(client);
    if target.gated() {
        // 档位跟着启动目标走，不能硬写 Cli —— 桌面端那一档的口径不一样。
        crate::app::start_watchdog(target.watch_mode(), &state, Some(app.clone()));
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

/// 放弃一条核验不了的会话记录。见 `sessions::forget` —— 只删记录，不动进程。
#[tauri::command]
pub async fn session_forget(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
    id: String,
) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    let result = (|| {
        crate::sessions::forget(&id)?;
        crate::gate::release_holder(&state.gate, &id)
    })();
    operations::changed(&events::ui(&app), "session", &id);
    result
}

#[tauri::command]
pub async fn diagnostic_run(
    app: tauri::AppHandle,
    request: crate::diagnostics::ProbeRequest,
) -> Result<ProbeReport> {
    let mut task = operations::Run::start(
        events::sink(&app),
        "中转诊断",
        request.environment_id.as_deref().unwrap_or("draft"),
    )?;
    task.phase("验证连接与协议", 20)?;
    let cancel = task.cancellable()?;
    let result = operations::interruptible(cancel, crate::diagnostics::run(request)).await;
    let result = task.finish(result);
    operations::changed(&events::ui(&app), "diagnostic", "");
    result
}
#[tauri::command]
pub fn diagnostic_history() -> Result<Vec<ProbeReport>> {
    crate::repository::Repository::open()?.list("diagnostics")
}
#[tauri::command]
pub fn extension_catalog() -> Result<Vec<ExtensionManifest>> {
    crate::extensions::catalog()
}
#[tauri::command]
pub async fn extension_import_skills(
    app: tauri::AppHandle,
    source: String,
) -> Result<Vec<ExtensionManifest>> {
    let _guard = operations::exclusive().await?;
    let mut task = operations::Run::start(events::sink(&app), "导入 Skills", &source)?;
    let result = crate::extensions::import_skills(&source).await;
    let result = task.finish(result);
    operations::changed(&events::ui(&app), "catalog", "");
    result
}
#[tauri::command]
pub async fn extension_import_mcp(
    app: tauri::AppHandle,
    name: String,
    configuration: String,
) -> Result<ExtensionManifest> {
    let _guard = operations::exclusive_soon().await?;
    let result = crate::extensions::import_mcp(&name, &configuration)?;
    operations::changed(&events::ui(&app), "catalog", &result.id);
    Ok(result)
}
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
    let cancel = task.cancellable()?;
    let result = operations::interruptible(cancel, crate::extensions::check_mcp(request)).await;
    task.finish(result)
}

#[tauri::command]
pub async fn environment_rollback(app: tauri::AppHandle, id: String) -> Result<Environment> {
    let _guard = operations::exclusive_soon().await?;
    let e = workspace::rollback_environment(&id)?;
    operations::changed(&events::ui(&app), "environment", &id);
    Ok(e)
}
#[tauri::command]
pub fn relay_export() -> Result<workspace::RelayExport> {
    workspace::export_relays()
}
#[tauri::command]
pub async fn relay_import(app: tauri::AppHandle, bundle: workspace::RelayExport) -> Result<usize> {
    let _guard = operations::exclusive_soon().await?;
    let n = workspace::import_relays(bundle)?;
    operations::changed(&events::ui(&app), "environment", "");
    Ok(n)
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
#[tauri::command]
pub async fn launch_plan_save(app: tauri::AppHandle, plan: LaunchPlan) -> Result<LaunchPlan> {
    let _guard = operations::exclusive_soon().await?;
    let p = workspace::save_plan(plan)?;
    operations::changed(&events::ui(&app), "plan", &p.id);
    Ok(p)
}
#[tauri::command]
pub async fn launch_plan_remove(app: tauri::AppHandle, id: String) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    workspace::remove_plan(&id)?;
    operations::changed(&events::ui(&app), "plan", &id);
    Ok(())
}

#[tauri::command]
pub async fn extension_connect_application(app: tauri::AppHandle) -> Result<ExtensionInstallation> {
    let _guard = operations::exclusive_soon().await?;
    let i = crate::extensions::connect_application()?;
    operations::changed(&events::ui(&app), "installation", &i.id);
    Ok(i)
}

/// 取消一个标记了可取消的操作。
///
/// 壳在这里、本体在 `operations::cancel`：平台层不挂 `#[tauri::command]`，
/// 挂了它就跟 Tauri 绑死、搬不进 `qb-platform`。
#[tauri::command]
pub fn operation_cancel(id: String) -> Result<()> {
    operations::cancel(&id)
}
