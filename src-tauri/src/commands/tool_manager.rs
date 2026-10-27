//! 软件页「其他命令行工具」（2026-10-27，中转站重构阶段 7 · X3）：OpenCode、Grok Build、OpenClaw、Hermes、Pi、MiniMax Code。
//! 判断在 `qb_install::install::cli_tools`，编排在 `usecase::cli_tools_ops`；这里拿锁、接进度条、写审计。
//!
//! - 现状只读文件（`cli_tools_status`）；「检查新版本」才联网（`cli_tools_latest`）。
//! - 装 / 升 / 卸 / 跑脚本都要等进程跑完，拿 `operations::exclusive()`（长任务那把），⛔ 锁**只在这一层拿**（坑 7.51）。
//! - 官方安装脚本分两步：`cli_tools_script_fetch` 下载给界面看，`cli_tools_script_run` 只认那一份的编号与哈希。
//!   ⛔ 两步之间要有人看过、点过确认 —— 别把它们串成一条命令。

use crate::sink::ProgressSink;
use crate::{
    audit,
    error::{GateError, Result},
    events, operations, paths,
};
use qb_app::usecase::cli_tools_ops as ops;
use qb_install::install::cli_tools::CliTool;

/// 六个工具现在的样子（只读文件，不起程序、不联网）。
#[tauri::command]
pub async fn cli_tools_status() -> Result<ops::CliToolsReport> {
    tauri::async_runtime::spawn_blocking(ops::status)
        .await
        .map_err(|e| GateError::Other(format!("没跑完：{e}")))
}

/// 问一次最新版本、看本机 Node 够不够新版用。**会联网**（npm / GitHub / PyPI），只在点了「检查新版本」时调。
#[tauri::command]
pub async fn cli_tools_latest(tool: CliTool) -> Result<ops::CliToolLatest> {
    Ok(ops::latest(tool).await)
}

/// 跑一个会动盘的操作：先拿锁（`run` 是惰性的，拿到锁才开始），结束时进度条收尾、写审计、刷新界面。
async fn act(
    app: &tauri::AppHandle,
    rep: &events::Reporter,
    what: String,
    run: impl std::future::Future<Output = Result<String>>,
) -> Result<String> {
    let _guard = operations::exclusive().await?;
    let r = run.await;
    match &r {
        Ok(detail) => {
            audit::write(&format!("命令行工具：{what} —— {detail}"));
            rep.done(detail);
        }
        Err(e) => {
            audit::write(&format!("命令行工具：{what} 没成：{e}"));
            rep.fail(&e.to_string());
        }
    }
    operations::changed(&events::ui(app), "software", "cli-tools");
    r
}

/// 用 npm 新装（`npm i -g <包>@latest`）。
#[tauri::command]
pub async fn cli_tools_install(app: tauri::AppHandle, tool: CliTool) -> Result<String> {
    let rep = events::Reporter::new(app.clone(), events::TASK_CLI_TOOL, ops::TOTAL);
    let what = format!("用 npm 装 {}", tool.label());
    act(&app, &rep, what, ops::install_npm(tool, &rep)).await
}

/// 升级命令行默认用的那一份（交给装它的那个包管理器 / 它自己的升级命令）。
#[tauri::command]
pub async fn cli_tools_update(app: tauri::AppHandle, tool: CliTool) -> Result<String> {
    let rep = events::Reporter::new(app.clone(), events::TASK_CLI_TOOL, ops::TOTAL);
    let what = format!("升级 {}", tool.label());
    act(&app, &rep, what, ops::update(tool, &rep)).await
}

/// 卸掉界面上点的那一份（只做 npm / nvm-windows / pnpm / Volta 装的）。
#[tauri::command]
pub async fn cli_tools_uninstall(
    app: tauri::AppHandle,
    tool: CliTool,
    path: String,
) -> Result<String> {
    let rep = events::Reporter::new(app.clone(), events::TASK_CLI_TOOL, ops::TOTAL);
    let what = format!("卸载 {} 的 {path}", tool.label());
    act(&app, &rep, what, ops::uninstall(tool, &path, &rep)).await
}

/// 下载官方安装脚本、存到本机、把全文和哈希交给界面看。**会联网**（只连这个工具登记的那一个 https 地址）。不运行它。
#[tauri::command]
pub async fn cli_tools_script_fetch(
    tool: CliTool,
    purpose: ops::ScriptPurpose,
) -> Result<ops::Fetched> {
    let dir = ops::script_dir(&paths::state_dir());
    let f = ops::script_fetch(tool, purpose, &dir).await?;
    audit::write(&format!(
        "命令行工具：下载了 {} 的官方安装脚本 {}（{} 字节，SHA-256 {}），等你看过再跑",
        tool.label(),
        f.final_url,
        f.size,
        f.sha256
    ));
    Ok(f)
}

/// 跑使用者看过的那一份（编号 + 哈希都对得上才跑），跑完删掉。
#[tauri::command]
pub async fn cli_tools_script_run(
    app: tauri::AppHandle,
    id: String,
    sha256: String,
) -> Result<String> {
    let rep = events::Reporter::new(app.clone(), events::TASK_CLI_TOOL, ops::TOTAL);
    let dir = ops::script_dir(&paths::state_dir());
    let what = format!("运行官方安装脚本 {id}（SHA-256 {sha256}）");
    act(&app, &rep, what, ops::script_run(&dir, &id, &sha256, &rep)).await
}
