//! QB Gate —— Claude 环境控制面板。
//!
//! 命令层。业务逻辑全在各自模块里，这里只做参数搬运与状态持有。

// 地基层（L0）住在 `qb-foundation` 里，这里原样再导出一次。
//
// 为什么是 `pub use` 而不是让全项目改写成 `qb_foundation::error`：
// `crate::error::Result` 在 29 个模块里出现了几百次，而拆 crate 这件事
// 本身**不该产生任何语义变化**。再导出之后 `crate::error` / `crate::paths`
// / `crate::audit` 仍然指得通，搬家的 diff 就只剩 Cargo.toml 那几行 ——
// 一旦测试数变了或者行为变了，原因绝不会是「搬家搬丢了」。
pub use qb_foundation::{audit, error, paths, sink};

// 平台层（L1）住在 `qb-platform` 里，同样原样再导出。
//
// `acl` 只在 Windows 上存在 —— 非 Windows 那份连模块都没有，
// 所以这条也得跟着 cfg，否则跨平台编译会找不到这个名字。
#[cfg(windows)]
pub use qb_platform::acl;
pub use qb_platform::{
    config_io, endpoint, firewall, legacy, logging, operations, panic_hook, process, progress,
    readiness, repository, secret, sessions, settings, signature,
};

// 契约层（L0.5）。`crate::domain::*` 在命令层出现几十次，同样原样再导出。
pub use qb_accounts::{accounts, residue};
pub use qb_app::{diagnostics, profile, snapshot, usecase, workspace};
pub use qb_contract::domain;
pub use qb_extensions::{extensions, plugins};
pub use qb_install::install;
pub use qb_iplock::gate;
pub use qb_launch::{killswitch, launch};
pub use qb_probe::probe;
pub use qb_relay::relay;
pub use qb_station::{health, router, schedule, station};
pub use qb_sysenv::sysenv;

pub mod app;
pub mod commands;
pub mod events;
mod startup;
pub mod tray;
pub mod update;

// `crate::AppState` 在命令、托盘、启动三处出现几十次。
// 搬进 `app` 之后原样再导出，调用点一个字不用改。
pub use app::AppState;

// `Reporter` 的 phase/log/done/fail 现在是 trait 方法（E0）——
// trait 不在作用域里就调不到。

pub fn run() {
    // 第一件事。晚一步就可能漏掉启动期的崩溃 —— 而启动期恰恰是最容易崩、
    // 也最难让使用者复现的阶段（恢复失败、配置坏了、权限不够都在这一段）。
    panic_hook::install();
    // 紧跟着装日志：启动恢复那一段（配置解析、迁移、ACL 修复）
    // 恰恰是最需要日志、而使用者最难复现的地方。
    logging::install();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            use tauri::Manager;
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // **关窗口 = 收进托盘，不是退出。**
        //
        // 退出会重锁全部副本（见下面 RunEvent::Exit 那段），而重锁挡住的是
        // **下一次启动**。Claude 桌面端的 Code 页每开一个新会话就要拉起一次
        // `%APPDATA%\Claude\claude-code\<版本>\claude.exe` —— 那个路径正在
        // `targets::lockable()` 里。于是实机现象是：正在聊的那个会话好好的，
        // 一开新会话就报「Claude Code couldn't start」，而使用者只是顺手把
        // 面板窗口关了，根本不知道这两件事有关系。
        //
        // 收进托盘之后看门狗还在跑、租约还在，门是有人看着的开 ——
        // 这跟「没人看着还敞着」是两码事，安全模型没有被放松。
        // 真要退出走托盘菜单的「退出 QB Gate」，那条路照旧重锁。
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 托盘没建起来就照旧退出 —— 藏起一个叫不回来的窗口更糟。
                if window.label() == "main" && tray::available() {
                    api.prevent_close();
                    let _ = window.hide();
                    audit::write("面板收进托盘（门禁与看门狗继续运行）");
                }
            }
        })
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            startup::startup_status,
            commands::workspace::operation_cancel,
            startup::startup_retry,
            startup::startup_discard_blocked,
            commands::workspace::official_configuration_preview,
            commands::workspace::official_configuration_migrate,
            commands::workspace::launch_plan_save,
            commands::workspace::launch_plan_remove,
            commands::workspace::official_switch_preview,
            commands::workspace::environment_rollback,
            commands::workspace::relay_export,
            commands::workspace::relay_import,
            commands::workspace::workspace_state,
            commands::workspace::provider_save,
            commands::workspace::credential_save,
            commands::workspace::environment_save,
            commands::workspace::environment_preview,
            commands::workspace::environment_apply,
            commands::workspace::workspace_references,
            commands::workspace::workspace_remove,
            commands::workspace::session_launch,
            commands::workspace::session_stop,
            commands::workspace::session_forget,
            commands::workspace::diagnostic_run,
            commands::workspace::diagnostic_history,
            commands::workspace::extension_catalog,
            commands::workspace::extension_import_skills,
            commands::workspace::extension_import_mcp,
            commands::workspace::extension_check_updates,
            commands::workspace::extension_preview,
            commands::workspace::extension_install,
            commands::workspace::extension_uninstall,
            commands::workspace::extension_connect_application,
            commands::workspace::extension_check,
            commands::gate::gate_status,
            commands::gate::gate_lock_all,
            commands::gate::gate_unlock_all,
            commands::gate::gate_reopen,
            commands::gate::gate_open,
            commands::gate::gate_release,
            commands::gate::gate_clean_stale,
            commands::gate::allowlist_read,
            commands::gate::allowlist_write,
            commands::gate::allowlist_add_current,
            commands::gate::country_presets,
            commands::probe::checkup_scan,
            commands::install::managed_history,
            commands::install::managed_rollback,
            commands::gate::hook_status,
            commands::gate::hook_install,
            commands::gate::hook_uninstall,
            commands::gate::watchdog_start,
            commands::gate::watchdog_stop,
            commands::probe::probe_ip,
            commands::probe::probe_ip_lookup,
            commands::ipv6::ipv6_status,
            commands::ipv6::ipv6_set,
            commands::probe::probe_purity,
            commands::probe::probe_dns,
            commands::probe::purity_criteria,
            // 中转站（0.14.0）。启停本机路由、线路池、请求日志、健康度与排序。
            commands::station::station_router_start,
            commands::station::station_router_stop,
            commands::station::station_router_status,
            commands::station::station_select_route,
            commands::station::station_launch,
            commands::station::station_schedules,
            commands::station::station_schedule_set,
            commands::station::station_probe,
            commands::station::station_client_config,
            commands::station::station_client_config_save,
            commands::station::station_routes,
            commands::station::station_put_route,
            commands::station::station_remove_route,
            commands::station::station_logs,
            commands::station::station_refresh_health,
            commands::station::station_decide,
            commands::station::station_audits,
            commands::station::station_run_audit,
            commands::station::station_models,
            commands::station::station_refresh_prices,
            commands::station::station_prices,
            commands::install::detect_software,
            commands::install::install_probe,
            commands::install::install_run,
            commands::install::claude_traces,
            commands::install::chrome_reinstall,
            commands::system::launch_claude,
            commands::codex_commands::codex_accounts,
            commands::codex_commands::codex_desktop_status,
            commands::codex_commands::codex_close,
            commands::codex_commands::codex_create,
            commands::codex_commands::codex_switch,
            commands::codex_commands::codex_launch,
            commands::codex_commands::codex_archive,
            commands::codex_commands::codex_usage,
            commands::accounts::accounts_list,
            commands::accounts::accounts_create,
            commands::accounts::accounts_delete,
            commands::accounts::accounts_tokens,
            commands::accounts::accounts_token_summary,
            commands::accounts::account_probe,
            commands::accounts::accounts_switch,
            commands::install::managed_status,
            commands::install::managed_probe_dir,
            commands::install::managed_set_dir,
            commands::install::managed_externals,
            commands::install::managed_cleanup,
            commands::install::purge_plan,
            commands::install::purge_execute,
            // 「两个口子」与浏览器隐私面（0.19.0）。会改系统的那几条
            // **只能由点击触发** —— 别把它们接到任何定时器或启动流程上。
            commands::network::firewall_rules,
            commands::network::firewall_adapters,
            commands::network::firewall_block,
            commands::network::firewall_revoke_all,
            commands::network::proxy_read,
            commands::network::proxy_backup,
            commands::network::proxy_apply,
            commands::network::proxy_rollback,
            commands::network::browser_audit,
            commands::network::browser_webrtc_harden,
            commands::network::browser_webrtc_clear,
            commands::network::egress_checks_scan,
            commands::network::egress_checks_fix,
            commands::network::egress_checks_undo,
            commands::system::relay_presets,
            commands::system::settings_load,
            commands::system::settings_save,
            commands::backup::snapshot_list,
            commands::backup::snapshot_create,
            commands::backup::snapshot_restore,
            commands::backup::snapshot_remove,
            commands::backup::snapshot_dir,
            commands::backup::profile_list,
            commands::backup::profile_save,
            commands::backup::profile_remove,
            commands::backup::profile_capture,
            commands::backup::profile_apply,
            commands::system::tz_current,
            commands::system::tz_apply,
            commands::system::tz_restore,
            commands::system::progress_load,
            commands::system::progress_set,
            commands::install::upgrade_plan,
            commands::install::upgrade_execute,
            commands::install::update_status,
            commands::gate::killswitch_preview,
            commands::gate::killswitch_execute,
            commands::plugins::plugin_list,
            commands::plugins::plugin_catalog_status,
            commands::plugins::plugin_start,
            commands::plugins::plugin_stop,
            commands::plugins::tavern_config,
            commands::plugins::tavern_config_save,
            commands::plugins::tavern_locate,
            commands::plugins::tavern_assets,
            commands::plugins::tavern_backup,
            commands::plugins::tavern_backups,
            commands::plugins::tavern_restore,
        ])
        .setup(|app| {
            use tauri::Manager;
            // 「界面该刷新了」在这里接上托盘。
            //
            // 装配点是全项目唯一同时认识 `app` 与 `tray` 的地方 ——
            // 让 `app::start_watchdog` 自己去调 `tray::refresh` 的话，
            // `app ↔ tray` 就是一对循环依赖（棘轮会当场报红）。
            {
                let handle = app.handle().clone();
                app.state::<AppState>()
                    .set_refresh_ui(std::sync::Arc::new(move || tray::refresh(&handle)));
            }
            // 每次启动拉一轮官方价目。
            //
            // **后台跑，不挡启动**：拉不到就继续用内置快照（见
            // `station_refresh_prices`），面板照常打开。价目是「查套路」算真实
            // 倍率的分母，隔一阵子官方调一次价，不更新的话会把调价冤枉成造假。
            tauri::async_runtime::spawn(async {
                match commands::station::station_refresh_prices().await {
                    Ok(v) if v.live_models > 0 => {
                        audit::write(&format!("已更新官方价目：{} 个模型", v.live_models))
                    }
                    Ok(v) => {
                        for p in &v.problems {
                            audit::write(&format!("官方价目沿用内置快照：{p}"));
                        }
                    }
                    Err(e) => audit::write(&format!("官方价目更新失败，沿用内置快照：{e}")),
                }
            });
            if !startup::initialize(app.handle()).ready {
                if let Err(e) = tray::init(app.handle()) {
                    audit::write(&format!("托盘建立失败：{e}"));
                }
                return Ok(());
            }
            #[cfg(windows)]
            for path in gate::targets::lockable() {
                if path.is_file() {
                    match acl::repair_legacy_null_dacl(&path) {
                        Ok(true) => {
                            audit::write(&format!("已修复已知旧版 NULL DACL：{}", path.display()))
                        }
                        Err(e) => audit::write(&format!("旧版权限修复失败：{e}")),
                        _ => {}
                    }
                }
            }
            let _ = legacy::disable_known_launchers();
            // 启动时重建锁，等价于现有实现的 -Mode Check：
            // 宁可多锁一次，也不要因为上次异常退出而敞着。
            //
            // **但白名单为空时绝不上锁。** 那种情况通常是首次运行：
            // 锁上之后 open_authorized 一定过不了（IP 不可能在空名单里），
            // 用户就被自己的工具关在门外了。空名单 = 还没配置好，
            // 这时候什么都不做才是对的。
            // 先重建执行锁，再等待网卡切换；UAC 等待不能留下一扇无监督的门。
            let restore_lease = match gate::allowlist::read() {
                Ok(list) if !list.is_empty() => {
                    let _ = gate::lock_all();
                    true
                }
                _ => {
                    audit::write("白名单为空，启动时不上锁（首次运行请先添加当前 IP）");
                    false
                }
            };
            // 网卡切换先完成，再恢复租约/启动监督，避免启动时的短暂断网被误判。
            // 后台等待 UAC，窗口仍可打开；错误在 IP 纯净度中可见。
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                {
                    use tauri::Manager;
                    commands::ipv6::apply_on_start(&handle.state::<AppState>()).await;
                }
                if restore_lease {
                    // 关好门之后，再看上次退出时门是不是开着的。
                    //
                    // **顺序不能反。** 「先别关门等我查完 IP」会在查询的那几秒里
                    // 留一扇没人看着的门，而面板刚启动时恰恰最可能是上次崩溃 /
                    // 断电留下的场面。所以是「宁可多锁一次，再决定要不要开」。
                    //
                    // 不做这件事的代价，是使用者反复报的那个现象：面板一重启，
                    // 租约随进程没了，桌面端 Code 页从此开不了新会话
                    // （`Claude Code couldn't start`），而且**没有任何东西会去救** ——
                    // `watchdog::decide` 要求 `lease_held` 才会走 `ReclaimLease`。
                    let handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        use tauri::Manager;
                        let st = handle.state::<AppState>();
                        if st
                            .gate
                            .manual_paused
                            .load(std::sync::atomic::Ordering::SeqCst)
                        {
                            return;
                        }
                        if let Some(mode) = gate::try_restore_lease(&st.gate).await {
                            app::start_watchdog(mode, &st, Some(handle.clone()));
                            tray::refresh(&handle);
                        }
                    });
                }
                {
                    use tauri::Manager;
                    app::start_watchdog(
                        gate::watchdog::WatchMode::Cli,
                        &handle.state::<AppState>(),
                        Some(handle.clone()),
                    );
                    // 智能调度是**落盘的承诺**：上次开着的，这次起来要接着跑。
                    // 不接回去的话，使用者重启一次面板，调度就悄悄停了 ——
                    // 而界面上那个开关还是「开」的（它读的是同一张表）。
                    if commands::station::station_schedules()
                        .map(|all| all.iter().any(|s| s.enabled))
                        .unwrap_or(false)
                    {
                        commands::station::start_scheduler(
                            &handle.state::<AppState>(),
                            Some(handle.clone()),
                        );
                    }
                }
                // 启动对齐：把时区 / 区域格式对到出口 IP 的归属地。
                //
                // **后台跑，不挡启动。** 它要先查一次出口 IP（要联网），
                // 放在主线程上等于拿网络延迟给窗口出现的时间封顶。
                //
                // **放在就绪判定之后。** 首次运行、面板还没配置好时什么都不该做 ——
                // 那时读到的是默认值（时区对齐是开的），照做就会在使用者还没同意过
                // 任何事之前弹一个 UAC 去改系统时区。跟上面「白名单为空就不上锁」
                // 是同一条理由：还没配置好 = 现在什么都不做才是对的。
                //
                // 已经一致的机器上这里不产生任何动作（见 `locale_ops::decide` 的第一条），
                // 所以「默认开」不等于每次开面板都折腾一遍系统。每一项做没做、
                // 为什么没做，`align_on_start` 自己会写进审计日志。
                tauri::async_runtime::spawn(async {
                    if let Err(e) = usecase::locale_ops::align_on_start().await {
                        audit::write(&format!("启动对齐失败：{e}"));
                    }
                });
            });
            // 托盘：不打开主窗口也能看状态、切账户、切中转站。
            // 建不起来不该让整个程序起不来 —— 有些精简版 Windows 没有
            // 通知区域，那时候面板本身仍然完全可用。
            if let Err(e) = tray::init(app.handle()) {
                audit::write(&format!("托盘建立失败（不影响面板使用）：{e}"));
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("启动 QB Gate 失败")
        .run(|_app, event| {
            // **面板退出时把门关上。**
            //
            // 没有这一步的话：用户在租约期内关掉面板，claude.exe 上的
            // Deny ACE 就一直摘着，看门狗也随进程没了 —— 门开着，
            // 而且没有任何东西在看。这跟「跑起来之后出口 IP 悄悄变了」
            // 是同一类风险，只是触发方式变成了「关掉了面板」。
            //
            // 启动时那段注释写的是「宁可多锁一次，也不要因为上次异常退出
            // 而敞着」——那是在给这个洞打补丁。现在正常退出这条路自己堵上了，
            // 启动时那道保险仍然留着（应对崩溃 / 断电）。
            //
            // 已经在跑的进程不受影响（Windows 不会因为加了 Deny ACE 就杀掉
            // 已加载的映像），挡住的是**下一次启动**。
            if matches!(event, tauri::RunEvent::Exit) && readiness::status().ready {
                match gate::lock_all() {
                    Ok(n) => audit::write(&format!("面板退出，已重新上锁 {n} 个可执行文件")),
                    Err(e) => audit::write(&format!("面板退出时重锁失败：{e}")),
                }
            }
        });
}
