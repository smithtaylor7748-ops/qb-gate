//! 主窗口什么时候露面、什么时候整个释放（2026-10-27，中转站重构阶段 7 · X7，照 cc-switch v4.0.14 的
//! `lightweight.rs` 与 `lib.rs` 里静默启动那一段）。
//!
//! # 窗口一开始是藏着的
//!
//! `tauri.conf.json` 里主窗口是 `visible: false`，页面第一次加载完（[`on_page_load`]）才露面。
//! 开机自启要一直藏着，而先露面再藏会闪一下；顺带去掉了冷启动时那一帧白底。
//! 页面要是一直没加载完（前端崩在半路），[`FALLBACK`] 之后照样露面 —— 托盘没建起来的机器上，
//! 一个永远不露面的窗口就是一个叫不回来、只能去任务管理器里结束的进程。
//!
//! # 开机自启：不露面
//!
//! 带着 `--autostart` 起来（`qb_platform::autostart`）、启动恢复也顺利（[`start`] 的 `silent`）时不露面，
//! 只留托盘。启动恢复没过（要使用者处理数据恢复）就照常露面 —— 藏起来等于让他永远看不到那个提示。
//!
//! # 轻量模式
//!
//! 设置里开了 `lightweight_on_close`，关窗口时不再藏起来，而是让它真的关掉，WebView2 跟着释放。
//! 最后一个窗口没了，Tauri 默认会退出整个程序 —— `lib.rs` 的 `RunEvent::ExitRequested` 在托盘可用时拦下这一步
//! （拦的只有「最后一个窗口关了」那一种，`code` 是 `None`；托盘的「退出」走 `app.exit(0)`，照常退）。
//! 门禁、看门狗、本机路由、智能调度都在后端，开不开它都照跑。再点托盘时 [`reveal`] 照配置重建一个。
//!
//! 托盘没建起来就不释放、也不藏：没有托盘，界面关了就叫不回来（跟 `lib.rs`「关窗口 = 收进托盘」同一个理由）。
//!
//! ⚠ 重建窗口不能在同步的事件回调里做（Tauri 文档：Windows 上会死锁，wry#583），一律丢进异步任务。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::webview::{PageLoadEvent, PageLoadPayload};
use tauri::{AppHandle, Emitter, Manager, Runtime, Webview, WebviewWindowBuilder};

pub const MAIN: &str = "main";

/// 页面加载完时要不要露面。开机自启的那一次、轻量模式重建前后各置一次。
static REVEAL_ON_LOAD: AtomicBool = AtomicBool::new(true);

/// 页面一直没加载完时，最多等这么久就直接露面。
pub const FALLBACK: Duration = Duration::from_secs(8);

/// `Builder::on_page_load` 挂的就是它。
pub fn on_page_load<R: Runtime>(webview: &Webview<R>, payload: &PageLoadPayload<'_>) {
    if webview.label() != MAIN || payload.event() != PageLoadEvent::Finished {
        return;
    }
    if REVEAL_ON_LOAD.swap(false, Ordering::SeqCst) {
        let w = webview.window();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// `setup` 的最后（托盘建完之后）调一次。
///
/// `silent`：这一次是开机自启起来的，而且托盘在、启动恢复也过了 —— 不露面。
/// 再加上开了轻量模式，就连界面都不留（省掉开机时那一份 WebView2）。
pub fn start<R: Runtime>(app: &AppHandle<R>, silent: bool) {
    if silent {
        REVEAL_ON_LOAD.store(false, Ordering::SeqCst);
        if crate::settings::load().lightweight_on_close {
            if let Some(w) = app.get_webview_window(MAIN) {
                let _ = w.destroy();
            }
            crate::audit::write("开机自启：面板只留在托盘，界面没有加载（轻量模式）");
        } else {
            crate::audit::write("开机自启：面板收在托盘里");
        }
        return;
    }
    arm_fallback(app);
}

/// 页面迟迟没加载完就直接露面（见模块头）。
fn arm_fallback<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FALLBACK).await;
        if REVEAL_ON_LOAD.swap(false, Ordering::SeqCst) {
            if let Some(w) = app.get_webview_window(MAIN) {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
    });
}

/// 窗口的关闭请求。返回 `true` = 要藏进托盘，调用方 `prevent_close` 再 `hide`；
/// `false` = 让它照常关掉（轻量模式，或者托盘没建起来时照旧退出）。
///
/// 托盘在不在由调用方传进来（`tray::available()`）：`tray` 要调这里的 [`reveal`]，
/// 这里再去问 `tray` 就是一对循环依赖（`module_cycles_only_ever_shrink` 当场抓住过）。
pub fn keep_on_close(label: &str, tray_up: bool) -> bool {
    if label != MAIN || !tray_up {
        return false;
    }
    if crate::settings::load().lightweight_on_close {
        crate::audit::write(
            "面板收进托盘并释放界面（轻量模式；门禁、看门狗、本机路由、智能调度继续运行）",
        );
        return false;
    }
    true
}

/// 把主窗口叫出来：在就露面，被轻量模式释放了就照配置重建一个。
///
/// `route`：顺带要去的页面（托盘「中转站」那一项）。窗口在的话发 `NAVIGATE`；
/// 要重建的话前端那时还没开始听事件，改成在页面脚本跑之前先把地址栏的 `#` 设好（`HashRouter` 读它）。
pub fn reveal<R: Runtime>(app: &AppHandle<R>, route: Option<&str>) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        if let Some(r) = route {
            let _ = app.emit(qb_contract::channels::NAVIGATE, r);
        }
        return;
    }
    let app = app.clone();
    let route = route.map(str::to_string);
    tauri::async_runtime::spawn(async move {
        if let Err(e) = rebuild(&app, route.as_deref()) {
            crate::audit::write(&format!("重新打开面板界面失败：{e}"));
        }
    });
}

fn rebuild<R: Runtime>(app: &AppHandle<R>, route: Option<&str>) -> tauri::Result<()> {
    // 两次托盘点击挤在一起时，第二次进来窗口可能已经有了。
    if app.get_webview_window(MAIN).is_some() {
        return Ok(());
    }
    let Some(config) = app.config().app.windows.iter().find(|w| w.label == MAIN) else {
        return Err(tauri::Error::WindowNotFound);
    };
    let mut builder = WebviewWindowBuilder::from_config(app, config)?;
    if let Some(r) = route.and_then(hash_script) {
        builder = builder.initialization_script(r);
    }
    REVEAL_ON_LOAD.store(true, Ordering::SeqCst);
    builder.build()?;
    arm_fallback(app);
    Ok(())
}

/// 重建时先把地址栏的 `#` 设成要去的页面。只认 `/` 开头、不带引号与反斜杠的路由 —— 它会被拼进一段脚本里。
fn hash_script(route: &str) -> Option<String> {
    let ok = route.starts_with('/')
        && route
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/-_?=&".contains(c));
    ok.then(|| {
        format!("if (!location.hash || location.hash === '#/') location.replace('#{route}');")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_routes_make_it_into_the_hash_script() {
        assert_eq!(
            hash_script("/relays").as_deref(),
            Some("if (!location.hash || location.hash === '#/') location.replace('#/relays');")
        );
        assert!(hash_script("relays").is_none(), "要 / 开头");
        assert!(hash_script("/x');alert(1)//").is_none());
        assert!(hash_script("/a\\b").is_none());
    }
}
