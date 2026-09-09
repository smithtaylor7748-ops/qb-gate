//! 系统托盘：不打开主窗口也能看状态、切账户、切中转站。
//!
//! # 为什么值得做
//!
//! 切账户和切中转站是**高频、低风险**的两件事，但现在都要开窗口、找到页面、
//! 点两下。托盘把它们收成两级菜单，一次点击完成。
//!
//! # 菜单是每次打开前重建的
//!
//! 账户槽位和中转站记录随时会变（用户在面板里加了一条、或者凭证过期了）。
//! Tauri 的托盘菜单是静态对象，**不重建就会显示上一次的内容** ——
//! 一个显示着已删除供应商的菜单比没有菜单更糟。
//!
//! # 政策边界
//!
//! 托盘里的账户切换跟面板里的是**同一个函数**，一样是纯人工触发：
//! 点一下切一次，没有定时器、没有轮询、没有「用完自动换」。
//! 见 README 合规边界第 2 条。

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

use crate::relay::RelayTarget;

pub const TRAY_ID: &str = "claudegate";

/// 菜单项 id 的前缀。用 `:` 分段，解析时按第一段分发。
const ID_OPEN: &str = "open";
const ID_QUIT: &str = "quit";
const ID_ACCOUNT: &str = "acct";
const ID_RELAY: &str = "relay";

/// 建托盘。只在 `setup()` 里调一次。
pub fn init<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = build_menu(app)?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("ClaudeGate")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(|tray, event| {
            // 左键单击 = 把窗口叫回来。这是 Windows 上托盘程序的惯例，
            // 用户不点右键也能回到面板。
            if let TrayIconEvent::Click { button, .. } = event {
                if button == tauri::tray::MouseButton::Left {
                    show_main(tray.app_handle());
                }
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// 重建菜单。账户或中转站变了之后调它，否则托盘里还是上一次的内容。
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    if let Ok(menu) = build_menu(app) {
        if let Some(tray) = app.tray_by_id(TRAY_ID) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let open = MenuItem::with_id(app, ID_OPEN, "打开面板", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, ID_QUIT, "退出 ClaudeGate", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    // ---- 状态行。不可点，只是让人扫一眼就知道现在什么情况。
    let targets = crate::gate::collect_targets();
    let locked = targets.iter().filter(|t| t.locked).count();
    let status = MenuItem::with_id(
        app,
        "status",
        format!("执行锁 {} / {}", locked, targets.len()),
        false,
        None::<&str>,
    )?;

    // ---- 账户
    let slots = crate::accounts::slots();
    let account_items: Vec<MenuItem<R>> = slots
        .iter()
        .map(|s| {
            // 当前那个前面打勾。用文字而不是 CheckMenuItem —— 后者在
            // Windows 托盘子菜单里跟「可以取消勾选」是同一个视觉语言，
            // 但账户不存在「取消激活」这个操作。
            let mark = if s.active { "● " } else { "   " };
            let plan = s.plan.clone().unwrap_or_else(|| "套餐未知".into());
            MenuItem::with_id(
                app,
                format!("{ID_ACCOUNT}:{}", s.label),
                format!("{mark}{} · {plan}", s.label),
                !s.active,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;

    let account_refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = account_items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<R>)
        .collect();

    let active_label = slots
        .iter()
        .find(|s| s.active)
        .map(|s| s.label.clone())
        .unwrap_or_else(|| "无".into());
    let accounts_menu = Submenu::with_items(
        app,
        format!("账户 · {active_label}"),
        !account_refs.is_empty(),
        &account_refs,
    )?;

    // ---- 中转站，每个 target 一个子菜单
    let store = crate::relay::store::load();
    let mut relay_menus: Vec<Submenu<R>> = Vec::new();
    for t in RelayTarget::ALL {
        let rows = store.view_of(t);
        let items: Vec<MenuItem<R>> = rows
            .iter()
            .map(|p| {
                let mark = if p.active { "● " } else { "   " };
                MenuItem::with_id(
                    app,
                    format!("{ID_RELAY}:{}:{}", t.as_str(), p.meta.id),
                    format!("{mark}{}", p.meta.name),
                    !p.active,
                    None::<&str>,
                )
            })
            .collect::<tauri::Result<_>>()?;
        let refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = items
            .iter()
            .map(|i| i as &dyn tauri::menu::IsMenuItem<R>)
            .collect();

        let current = rows
            .iter()
            .find(|p| p.active)
            .map(|p| p.meta.name.clone())
            .unwrap_or_else(|| "未配置".into());
        relay_menus.push(Submenu::with_items(
            app,
            format!("{} · {current}", t.label()),
            !refs.is_empty(),
            &refs,
        )?);
    }

    let mut items: Vec<&dyn tauri::menu::IsMenuItem<R>> = vec![&open, &status, &sep1, &accounts_menu];
    for m in &relay_menus {
        items.push(m);
    }
    items.push(&sep2);
    items.push(&quit);

    Menu::with_items(app, &items)
}

fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref().to_string();

    match id.split(':').collect::<Vec<_>>().as_slice() {
        [ID_OPEN] => show_main(app),
        [ID_QUIT] => app.exit(0),

        // 跟面板里走的是同一个函数。**纯人工触发** —— 点一下切一次。
        [ID_ACCOUNT, label] => {
            let label = (*label).to_string();
            match crate::accounts::switch(&label) {
                Ok(()) => crate::gate::log::write(&format!("托盘：账户切换到 {label}")),
                Err(e) => crate::gate::log::write(&format!("托盘：账户切换失败：{e}")),
            }
            refresh(app);
        }

        [ID_RELAY, target, pid] => {
            let Some(t) = RelayTarget::ALL
                .into_iter()
                .find(|x| x.as_str() == *target)
            else {
                return;
            };
            match crate::relay::activate(t, pid) {
                Ok(()) => crate::gate::log::write(&format!("托盘：{} 中转站已切换", t.label())),
                Err(e) => crate::gate::log::write(&format!("托盘：中转站切换失败：{e}")),
            }
            refresh(app);
        }

        _ => {}
    }
}

fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 菜单项 id 是拿 `:` 分段解析的，所以**分段本身不能出现在数据里**。
    ///
    /// 账户标签来自目录名 `claude-profile-<标签>`，中转站 id 是本项目生成的
    /// `<slug>-<时间戳>` —— 两者都不该含冒号。真出现了的话，分发会落到
    /// `_ => {}` 那条静默什么都不做，症状是「点了没反应」。
    #[test]
    fn generated_ids_never_contain_the_separator() {
        for t in RelayTarget::ALL {
            assert!(!t.as_str().contains(':'), "{}", t.as_str());
        }
        assert!(!ID_ACCOUNT.contains(':'));
        assert!(!ID_RELAY.contains(':'));
        assert!(!ID_OPEN.contains(':'));
        assert!(!ID_QUIT.contains(':'));
    }

    #[test]
    fn relay_ids_round_trip_through_the_menu_id_format() {
        // 拼出来的 id 必须能按三段解回去，否则菜单点了没反应。
        for t in RelayTarget::ALL {
            let id = format!("{ID_RELAY}:{}:{}", t.as_str(), "packycode-18f2a");
            let parts: Vec<&str> = id.split(':').collect();
            assert_eq!(parts.len(), 3, "{id}");
            assert_eq!(parts[0], ID_RELAY);
            assert_eq!(parts[1], t.as_str());
            assert_eq!(parts[2], "packycode-18f2a");
        }
    }
}
