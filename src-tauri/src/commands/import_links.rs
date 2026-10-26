//! 导入链接（中转站重构阶段 5，cc-switch 的 X4 Deep Link）：收链接、给界面看、确认之后导入。
//!
//! 链接从三处来：
//! - **冷启动**：点了 `qbgate://…` 而面板没开着，Windows 把链接当第一个参数起面板（[`receive`] 在装配时扫一遍启动参数）；
//! - **面板开着**：第二个实例被单实例插件拦下，参数转给这一个（`lib.rs` 的回调里同样交给 [`receive`]）；
//! - **粘贴**：界面上「从链接导入」（[`deeplink_paste`]），中转站页面上给 CC Switch 的 `ccswitch://` 链接也认。
//!
//! 协议由**安装包**注册（`tauri.conf.json` 的 `plugins.deep-link` 是给打包用的配置，NSIS 模板按它写
//! `HKCU\Software\Classes\qbgate`、卸载时删掉）。⛔ 运行时不注册、不引 `tauri-plugin-deep-link`：
//! 它在 Windows 上多出来的只有运行时写注册表（开发版一跑就把装好的面板的协议抢过去）和解析启动参数
//! （单实例插件的回调本来就拿得到参数）。
//!
//! 链接放在后端（`AppState::deeplinks`），事件只是「去看一眼」：冷启动时界面还没挂上，事件没人收。
//! ⛔ 回给界面的形状里没有明文 Key（`deeplink_ops::DeepLinkView`）。

use crate::app::AppState;
use crate::error::Result;
use qb_app::deeplink::{CC_SWITCH_SCHEME, SCHEME};
use qb_app::usecase::deeplink_ops::{DeepLinkChoice, DeepLinkEntry, DeepLinkOutcome};
use tauri::{Emitter, Manager, State};

/// 参数里有没有 `qbgate://` 链接：有就收下最后那条、通知界面，回 `true`。
///
/// 只认系统注册的那个协议：`ccswitch://` 不会从命令行来（那是 cc-switch 的），只认粘贴的。
pub fn receive(app: &tauri::AppHandle, args: &[String]) -> bool {
    let prefix = format!("{SCHEME}://");
    let Some(url) = args
        .iter()
        .rev()
        .find(|a| a.trim().to_ascii_lowercase().starts_with(&prefix))
    else {
        return false;
    };
    app.state::<AppState>().deeplinks.put(url);
    // 审计日志只记来了一条、是什么类型，不记链接本身（站点链接里有 Key）。
    crate::audit::write("导入链接：收到一条 qbgate:// 链接，等使用者确认");
    if let Err(e) = app.emit(qb_contract::channels::DEEPLINK, ()) {
        tracing::warn!("导入链接：通知界面失败（界面挂上来时会自己来取）：{e}");
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    true
}

/// 现在待处理的那一条（读不清除；界面显示之后再 [`deeplink_ack`]）。
#[tauri::command]
pub fn deeplink_peek(state: State<'_, AppState>) -> Result<Option<DeepLinkEntry>> {
    Ok(state
        .deeplinks
        .peek(&crate::repository::Repository::open()?))
}

/// 使用者点了「不导入」（或者界面显示过了）：编号对得上才清。
#[tauri::command]
pub fn deeplink_ack(state: State<'_, AppState>, id: u64) {
    state.deeplinks.ack(id);
}

/// 粘贴进来的链接（`qbgate://` 或 `ccswitch://`）：跟点开的一样收下，回给界面看的那一份。
#[tauri::command]
pub fn deeplink_paste(state: State<'_, AppState>, url: String) -> Result<Option<DeepLinkEntry>> {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with(&format!("{SCHEME}://"))
        && !lower.starts_with(&format!("{CC_SWITCH_SCHEME}://"))
    {
        return Err(crate::error::GateError::Other(format!(
            "只认 {SCHEME}:// 或 {CC_SWITCH_SCHEME}:// 开头的导入链接"
        )));
    }
    state.deeplinks.put(trimmed);
    Ok(state
        .deeplinks
        .peek(&crate::repository::Repository::open()?))
}

/// 使用者确认导入。站点导入成线路之后，跟手动存线路一样重装上游、更新聚合模型。
#[tauri::command]
pub fn deeplink_import(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: u64,
    choice: DeepLinkChoice,
) -> Result<DeepLinkOutcome> {
    let db = crate::repository::Repository::open()?;
    let out = state.deeplinks.import(&db, id, &choice)?;
    if let Some(client) = out.client {
        crate::commands::station::load_upstreams(&state)?;
        crate::commands::station::sync_aggregate_config(&state, client)?;
    }
    // 站点、凭证、线路可能多了一条：界面重拉工作区。
    crate::operations::changed(&crate::events::ui(&app), "provider", "deeplink");
    Ok(out)
}
