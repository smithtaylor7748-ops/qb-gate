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

#[cfg(test)]
use crate::relay::RelayTarget;
use tauri::Emitter;

pub const TRAY_ID: &str = "qbgate";

/// 托盘图标到底建起来没有。
///
/// 「关窗口 = 收进托盘」只有在托盘真的存在时才成立。有些精简版 Windows
/// 没有通知区域，`init()` 会失败 —— 那时候再把窗口藏起来，程序就变成了
/// 一个看不见、也叫不回来的进程，只能去任务管理器里结束它。
/// 所以那条路径要先问一句这里。
static TRAY_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 托盘可用吗？没建起来就别把主窗口藏掉。
pub fn available() -> bool {
    TRAY_UP.load(std::sync::atomic::Ordering::Relaxed)
}

/// 菜单项 id 的前缀。用 `:` 分段，解析时按第一段分发。
const ID_OPEN: &str = "open";
const ID_QUIT: &str = "quit";
const ID_ACCOUNT: &str = "acct";
const ID_RELAY: &str = "relay";
const ID_REOPEN: &str = "reopen";

/// 建托盘。只在 `setup()` 里调一次。
pub fn init<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = build_menu(app)?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("QB Gate")
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
    TRAY_UP.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// 重建菜单。账户或中转站变了之后调它，否则托盘里还是上一次的内容。
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    crate::operations::changed(&crate::events::ui(app), "all", "tray");
    if let Ok(menu) = build_menu(app) {
        if let Some(tray) = app.tray_by_id(TRAY_ID) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let open = MenuItem::with_id(app, ID_OPEN, "打开面板", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, ID_QUIT, "退出 QB Gate", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    // ---- 状态行。不可点，只是让人扫一眼就知道现在什么情况。
    //
    // 这里必须**同时**显示租约，不能只显示锁了几个。
    // 「4/4 已锁」在使用者眼里是「一切正常」，可它恰恰是
    // 「Claude 桌面端现在开不了新会话」的样子 —— 门关着、没有租约。
    let targets = crate::gate::collect_targets();
    let locked = targets.iter().filter(|t| t.locked).count();
    let holder = {
        let st = app.state::<crate::AppState>();
        let h = st.gate.lease.lock().unwrap().holder.clone();
        h
    };
    let status = MenuItem::with_id(
        app,
        "status",
        match &holder {
            Some(h) => format!("执行锁 {} / {} · 已放行给 {h}", locked, targets.len()),
            None => format!("执行锁 {} / {} · 门禁关闭中", locked, targets.len()),
        },
        false,
        None::<&str>,
    )?;

    // ---- 重新放行。门被面板自己关上时，这是最快的一条出路。
    //
    // 常驻而不是「按需出现」：菜单是每次打开前重建的，一个时有时无的
    // 菜单项会让人记不住它在哪 —— 而需要它的时刻恰恰是最慌的时刻。
    let reopen = MenuItem::with_id(
        app,
        ID_REOPEN,
        if holder.is_some() {
            "重新放行（验一次出口 IP）"
        } else {
            "重新放行（门禁关闭中）"
        },
        true,
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
            // ⛔ 菜单项的 **id 仍然是 label** —— 点下去要按它找槽位。变的只是看得见的那一行。
            MenuItem::with_id(
                app,
                format!("{ID_ACCOUNT}:{}", s.label),
                format!("{mark}{} · {plan}", s.display_name()),
                !s.active,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;

    // 托盘弹不了确认框，所以代价直接写在菜单里：点一下就先关掉全部 Claude（v0.9.0）。
    let account_warn = MenuItem::with_id(
        app,
        "acct-warn",
        "点了会先关闭全部 Claude，未保存的对话会丢",
        false,
        None::<&str>,
    )?;
    let account_sep = PredefinedMenuItem::separator(app)?;

    let mut account_refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = Vec::new();
    if !account_items.is_empty() {
        account_refs.push(&account_warn);
        account_refs.push(&account_sep);
    }
    account_refs.extend(
        account_items
            .iter()
            .map(|i| i as &dyn tauri::menu::IsMenuItem<R>),
    );

    let active_label = slots
        .iter()
        .find(|s| s.active)
        .map(|s| s.display_name())
        .unwrap_or_else(|| "无".into());
    let accounts_menu = Submenu::with_items(
        app,
        format!("账户 · {active_label}"),
        !account_items.is_empty(),
        &account_refs,
    )?;

    let relay = MenuItem::with_id(app, ID_RELAY, "中转站 · 管理与独立启动", true, None::<&str>)?;

    let mut items: Vec<&dyn tauri::menu::IsMenuItem<R>> =
        vec![&open, &status, &reopen, &sep1, &accounts_menu];
    items.push(&relay);
    items.push(&sep2);
    items.push(&quit);

    Menu::with_items(app, &items)
}

fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref().to_string();

    match id.split(':').collect::<Vec<_>>().as_slice() {
        [ID_OPEN] => show_main(app),
        [ID_QUIT] => app.exit(0),

        // 重新放行。**只恢复租约，不启动任何进程** ——
        // 跟面板里那个横幅走的是同一个函数。
        [ID_REOPEN] => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let st = app.state::<crate::AppState>();
                let Ok(_guard) = crate::operations::exclusive().await else {
                    return;
                };
                match crate::app::reopen_and_rewatch(&st, None).await {
                    Ok(d) => crate::audit::write(&format!("托盘：{d}")),
                    Err(e) => crate::audit::write(&format!("托盘：重新放行失败：{e}")),
                }
                refresh(&app);
            });
        }

        // **纯人工触发** —— 点一下切一次。跟面板里同一个语义（v0.9.0）：
        // 先清场（关掉全部 Claude），再换指向（Claude Code、酒馆桥接，桌面端见下），**不自动启动**。
        //
        // 托盘弹不了确认框，但也不需要：清场之后没有任何需要确认的冲突
        // （桌面端也被关了），直接切即可。风险（未保存的对话会丢）写在账户子菜单顶上。
        // 收进程要 await，所以放进异步任务里跑。
        //
        // 两条跟对话框对齐的规矩：
        //   * **清场失败就不切** —— `switch_in` 不再自己查桌面端，信的是调用方清过场；
        //   * 桌面端用 `Auto`（这个槽位有自己的桌面端资料才跟，没有就不动）——
        //     对话框里那个复选框的默认值就是它。用 `Follow` 会给没有桌面端资料的槽位
        //     建一份空白的，桌面端莫名其妙就被登出了。
        [ID_ACCOUNT, label] => {
            let label = (*label).to_string();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let Ok(_guard) = crate::operations::exclusive().await else {
                    return;
                };
                let st = app.state::<crate::AppState>();
                if let Err(e) = crate::usecase::account_ops::preflight_switch(&label) {
                    crate::audit::write(&format!("托盘：账户切换预检失败：{e}"));
                    refresh(&app);
                    return;
                }
                match crate::app::clear_for_switch(&st, true).await {
                    Ok(r) => crate::audit::write(&format!(
                        "托盘：切账户前清场，关掉 {} 个 Claude 进程",
                        r.killed.len()
                    )),
                    Err(e) => {
                        crate::audit::write(&format!("托盘：清场失败，没有切换（槽位没动）：{e}"));
                        refresh(&app);
                        return;
                    }
                }
                match crate::usecase::account_ops::switch_with(
                    &label,
                    crate::accounts::DesktopMode::Auto,
                ) {
                    Ok(_) => crate::audit::write(&format!("托盘：账户切换到 {label}")),
                    Err(e) => crate::audit::write(&format!("托盘：账户切换失败：{e}")),
                }
                refresh(&app);
            });
        }

        [ID_RELAY] => {
            show_main(app);
            let _ = app.emit(qb_contract::channels::NAVIGATE, "/relays");
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
        assert!(!ID_REOPEN.contains(':'));
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
