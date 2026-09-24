//! 安装 / 升级 / 托管目录 / 残留清理相关的命令。

use crate::sink::ProgressSink;
use crate::{
    audit,
    error::{GateError, Result},
    events::{self, Reporter},
    gate, install, killswitch, operations, sessions, settings, tray, update, usecase, AppState,
};

// -------------------------------------------------------------- 版本库

/// 托管目录的版本库里有哪几版可以退回去。
#[tauri::command]
pub fn managed_history(which: install::managed::App) -> Vec<install::versions::VersionEntry> {
    let root = settings::managed_apps_dir();
    let dir = install::managed::app_dir(&root, which);
    let current = install::managed::record_of(&root, which).map(|r| r.version);
    install::versions::history(&dir, which, current.as_deref())
}

/// 回滚到版本库里的某一版。
///
/// **会先把当前这份收进版本库再换** —— 反过来的话中间那一刻当前版本已经没了，
/// 换不上去就两头空。换完要重新上锁（调用方负责，跟安装走同一条路）。
#[tauri::command]
pub async fn managed_rollback(which: install::managed::App, version: String) -> Result<String> {
    let _guard = operations::exclusive().await?;
    sessions::ensure_verified(|_| true)?;
    if sessions::list().iter().any(|s| s.state == "running") {
        return Err(GateError::Other("请先停止受管会话，再回滚软件版本".into()));
    }
    let root = settings::managed_apps_dir();
    let dir = install::managed::app_dir(&root, which);
    let current = install::managed::record_of(&root, which);
    let msg = install::versions::rollback(&dir, which, &version, current.as_ref())?;
    // 换上来的那份此刻是从版本库拷过来的，ACL 跟着源文件走 —— 必须重锁一遍，
    // 否则回滚等于顺手把门禁摘了。
    let n = gate::lock_all()?;
    Ok(format!("{msg}，已重新上锁 {n} 个副本"))
}

// ------------------------------------------------------------------ 环境

#[tauri::command]
pub async fn detect_software() -> install::detect::SoftwareReport {
    install::detect::report().await
}

/// winget 在不在、两个包查不查得到、各自什么版本。
#[tauri::command]
pub async fn install_probe() -> install::winget::InstallProbe {
    install::winget::probe().await
}

/// 装一个目标。
///
/// * Claude Code / Codex（v0.9.0）：**面板托管安装** —— 从官方源下载、核对官方给的 SHA-256
///   与数字签名、放进托管目录（`install::managed`）。位置由面板说了算，面板再也不用猜。
/// * Claude 桌面端：winget 装官方安装器。它的位置被官方安装器写死，面板接管不了，
///   装完从注册表把实际位置记下来。
#[tauri::command]
pub async fn install_run(
    app: tauri::AppHandle,
    target: install::winget::InstallTarget,
    state: tauri::State<'_, AppState>,
) -> Result<install::winget::InstallResult> {
    let _guard = operations::exclusive().await?;
    use install::winget::InstallTarget as T;
    let managed_app = match target {
        T::ClaudeCode => Some(install::managed::App::ClaudeCode),
        T::Codex => Some(install::managed::App::Codex),
        T::ClaudeDesktop => None,
    };
    match managed_app {
        Some(a) => managed_install(app, a, "latest", &state).await,
        None => {
            let rep = Reporter::new(app, events::TASK_INSTALL, install::winget::TOTAL);
            // `winget::install` 自己会解锁、自己会重锁（那六步的第 1 步和第 6 步），
            // 所以这里只观察、不重复解锁。守卫在这里只干一件事：
            // **重锁之后把使用者原来拿着的租约还回去**。
            let guard = gate::Maintenance::observing(&state.gate);
            let r = usecase::install_ops::winget_install(target, &rep).await;
            let done = guard.finish(&state.gate).await;
            if let Err(e) = &r {
                rep.fail(&format!("{e} {}", done.detail));
            }
            r
        }
    }
}

/// 托管安装 / 升级。安装器只管文件，重锁与还租约在这里的维护窗口收尾时做。
///
/// ⛔ **调用方必须已经拿着 `operations::exclusive()` 的锁**（`install_run` / `upgrade_execute`
/// 都是）。这里**不许再拿一次**：那把锁是 tokio 的 `Mutex`，不可重入 —— 0.22.6 到 0.27.0
/// 这里多了一句 `operations::exclusive().await?`，于是软件页的「安装 / 重新下载安装」
/// 和托管那份的「升级」一点下去就在自己手里的锁上永远等着：按钮转圈不停、
/// 一段进度都不出、也没有任何报错。托管安装在待办里一直标着「未验证」，正是它。
async fn managed_install(
    app: tauri::AppHandle,
    which: install::managed::App,
    channel: &str,
    state: &AppState,
) -> Result<install::winget::InstallResult> {
    use install::winget::{InstallResult, InstallTarget, Method};
    let rep = Reporter::new(app, events::TASK_INSTALL, install::managed::TOTAL);
    let guard = gate::Maintenance::observing(&state.gate);
    let r = install::managed::install(which, channel, &rep).await;
    let done = guard.finish(&state.gate).await;
    let target = match which {
        install::managed::App::ClaudeCode => InstallTarget::ClaudeCode,
        install::managed::App::Codex => InstallTarget::Codex,
    };
    match r {
        Ok(o) => {
            let detail = format!(
                "{} {} 已装进托管目录 {}。{}",
                which.label(),
                o.version,
                o.path.display(),
                done.detail
            );
            audit::write(&detail);
            rep.done(&detail);
            Ok(InstallResult {
                target,
                ok: true,
                method: Method::Managed,
                relocked: done.relocked,
                signature_ok: Some(true),
                detail,
                log: o.log,
            })
        }
        Err(e) => {
            let msg = format!("{e} {}", done.detail);
            audit::write(&format!("托管安装 {} 失败：{e}", which.label()));
            rep.fail(&msg);
            Err(GateError::Other(msg))
        }
    }
}

/// 托管目录的现状：根目录在哪、Claude Code 与 Codex 装没装、什么版本。
#[tauri::command]
pub fn managed_status() -> install::managed::Status {
    install::managed::status()
}

/// **当场实测**一个目录能不能当托管根目录（建得出、写得进、锁得上也解得开）。
#[tauri::command]
pub fn managed_probe_dir(path: String) -> install::managed::Probe {
    install::managed::probe_dir(std::path::Path::new(path.trim()))
}

#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct MigrateReport {
    from: std::path::PathBuf,
    to: std::path::PathBuf,
    moved: Vec<String>,
    /// 为了搬走正在运行的 Claude Code，先关掉了几个 Claude 进程。
    closed: usize,
}

/// 换托管根目录。已经装了东西就**一键迁移**过去；什么都没装就只是记下新位置。
///
/// 顺序：实测新目录 → （托管的 Claude Code 正在跑就先关掉全部 Claude）→ 旧目录里还有
/// 别的程序在跑就拒绝 → 搬文件（失败整体回滚）→ 写设置（写不进去就搬回去）→
/// 维护窗口收尾时按新位置重新上锁。写设置必须在重锁之前 ——
/// `lock_all` 从设置里读根目录，顺序反了会锁到旧位置上去。
#[tauri::command]
pub async fn managed_set_dir(
    app: tauri::AppHandle,
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<MigrateReport> {
    let _guard = operations::exclusive().await?;
    use install::managed;
    let new = std::path::PathBuf::from(path.trim());
    let old = managed::root();
    let mut report = MigrateReport {
        from: old.clone(),
        to: new.clone(),
        moved: Vec::new(),
        closed: 0,
    };
    // 前置判断在 `managed::plan_relocate` 里，是纯函数、有单测。
    // 放在这里的话它跟下面那一百行 I/O 缠在一起，一条测试都写不了 ——
    // 而真正容易出错的恰恰是这几条前置条件，不是 I/O 本身。
    let probe = managed::probe_dir(&new);
    let has_files = managed::App::ALL
        .iter()
        .any(|a| managed::exe_in(&old, *a).is_file());

    let store = |dir: &std::path::Path| -> Result<()> {
        let mut s = settings::load();
        s.managed_apps_dir = managed::managed_dir_setting(dir, &settings::default_managed_dir());
        usecase::settings_ops::save(&s)
    };

    match managed::plan_relocate(&old, &new, probe.ok, has_files) {
        managed::Relocate::Same => return Ok(report),
        managed::Relocate::Nested => {
            return Err(GateError::Other(
                "新目录不能在旧目录里面，旧目录也不能在新目录里面 —— 搬的时候会把自己套进去。"
                    .into(),
            ))
        }
        managed::Relocate::Unusable => {
            return Err(GateError::Other(
                probe.reason.unwrap_or_else(|| "这个目录不能用".into()),
            ))
        }
        managed::Relocate::JustRecord => {
            store(&new)?;
            audit::write(&format!(
                "托管目录改为 {}（原来没装东西，不用搬）",
                new.display()
            ));
            return Ok(report);
        }
        managed::Relocate::Migrate => {}
    }

    let guard = gate::Maintenance::observing(&state.gate);
    // 托管的 claude.exe 正被会话占着的话搬不动（跨盘时删不掉源）。只在确实有进程
    // 从旧目录里跑着时才清场 —— 换个目录不该顺手把没关系的对话全关了。
    let running_here = killswitch::preview()
        .await
        .map(|p| {
            p.targets.iter().any(|t| {
                t.path
                    .as_deref()
                    .is_some_and(|x| managed::is_inside(std::path::Path::new(x), &old))
            })
        })
        .unwrap_or(false);
    if running_here {
        match killswitch::execute().await {
            Ok(k) => report.closed = k.killed.len(),
            Err(e) => {
                guard.finish(&state.gate).await;
                return Err(e);
            }
        }
    }
    // 再看一遍旧目录里还有没有程序在跑（比如 Codex —— 一键关闭不认它）。正在运行的 exe
    // 搬不动，搬一半比不搬危险得多，所以有就一个文件都不动，列出来让使用者自己关。
    // 刚收掉的进程要一小会儿才从进程表里消失，清过场的话多看几轮。
    let mut busy = Vec::new();
    for round in 0..if running_here { 6 } else { 1 } {
        if round > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
        busy = match managed::running_from(&old).await {
            Ok(b) => b,
            Err(e) => {
                guard.finish(&state.gate).await;
                return Err(e);
            }
        };
        if busy.is_empty() {
            break;
        }
    }
    if !busy.is_empty() {
        guard.finish(&state.gate).await;
        return Err(GateError::Other(format!(
            "还有程序正从旧目录运行，搬不动：{}。先把它关掉再迁移（面板不按进程名杀进程）。什么都没动。",
            busy.iter().map(|(pid, p)| format!("PID {pid} {p}")).collect::<Vec<_>>().join("；")
        )));
    }
    let moved = match managed::migrate_files(&old, &new) {
        // 设置写不进去就把文件搬回去 —— 不然文件在新目录、设置指着旧目录，重锁锁不到它们。
        Ok(notes) => match store(&new) {
            Ok(()) => Ok(notes),
            Err(e) => Err(GateError::Other(format!(
                "托管目录的设置没写进去（{e}），{}",
                match managed::migrate_files(&new, &old) {
                    Ok(_) => "文件已经搬回原处，托管目录没有改。".to_string(),
                    Err(b) => format!("往回搬也失败了：{b}"),
                }
            ))),
        },
        Err(e) => Err(e),
    };
    if moved.is_err() {
        let n = managed::lock_strays(&new);
        if n > 0 {
            audit::write(&format!(
                "托管目录迁移没成，给留在 {} 的 {n} 个文件加上了执行锁",
                new.display()
            ));
        }
    }
    let done = guard.finish(&state.gate).await;
    report.moved = moved?;
    audit::write(&format!(
        "托管目录从 {} 迁到 {}：{}。{}",
        old.display(),
        new.display(),
        report.moved.join("；"),
        done.detail
    ));
    tray::refresh(&app);
    Ok(report)
}

/// 面板没装的、多余的 Claude Code / Codex 副本，以及准备怎么清。只看不动。
#[tauri::command]
pub async fn managed_externals() -> Vec<install::managed::External> {
    install::managed::externals().await
}

/// **彻底清除**一个软件的外部副本（使用者确认过清单之后）。
///
/// 先关掉全部 Claude（正在跑的删不掉），清完在维护窗口收尾时重锁 —— 清单变短了，锁也跟着变。
/// 不清的话什么都不用做：inventory 照样把它们全锁上。
#[tauri::command]
pub async fn managed_cleanup(
    which: install::managed::App,
    state: tauri::State<'_, AppState>,
) -> Result<install::managed::CleanupReport> {
    let _guard = operations::exclusive().await?;
    let guard = gate::Maintenance::observing(&state.gate);
    if which == install::managed::App::ClaudeCode {
        if let Err(e) = killswitch::execute().await {
            guard.finish(&state.gate).await;
            return Err(e);
        }
    }
    let r = install::managed::cleanup(which).await;
    guard.finish(&state.gate).await;
    if let Ok(rep) = &r {
        for d in &rep.done {
            audit::write(&format!("清除外部副本：{d}"));
        }
        for f in &rep.failed {
            audit::write(&format!("清除外部副本失败：{f}"));
        }
    }
    r
}

// ------------------------------------------------------- 完全卸载（0.19.0）

/// 完全卸载之前的**只读盘点**：这个软件在本机的全部落点，按类分好，
/// 每一项都带绝对路径与归属依据。
///
/// **只读。** 不删任何东西、不改任何设置 —— 界面拿它列确认框，
/// 使用者输入确认词之后才轮到执行那一半。
///
/// 跟 `managed_externals` 的分界线：那个清的是「面板没装的多余副本」，
/// 托管那份、版本库、配置、认证、账户槽位一概不碰；这个的前提相反 ——
/// **这台机器上不再要这个软件了**，上面那些全都要清。
///
/// 不占 `operations::exclusive()`：它是只读的，不该把别的活挡在外面
/// （跟 `managed_externals` 同一个道理）。
#[tauri::command]
pub async fn purge_plan(target: install::purge::Target) -> Result<Vec<install::purge::Item>> {
    usecase::purge_ops::plan(target).await
}

/// **执行**完全卸载。调用之前界面必须已经让使用者看过盘点并输入了确认词。
///
/// 跟 `purge_plan` 相反，这个**要占独占锁**：它会关进程、摘执行锁、删文件，
/// 跟别的长任务撞上会互相毁。
///
/// 结束时复扫一遍，把还剩下的照实列在 `left` 里 —— 不看命令报了什么，
/// 看重新扫一遍还剩什么。
#[tauri::command]
pub async fn purge_execute(
    target: install::purge::Target,
    state: tauri::State<'_, AppState>,
) -> Result<usecase::purge_ops::Report> {
    let _guard = operations::exclusive().await?;
    usecase::purge_ops::execute(target, &state.gate).await
}

// --------------------------------------------------- Claude 痕迹与 Chrome

/// 这台机器以前装过 / 登录过 Claude 吗。
///
/// 只读检测，不改任何东西。**Chrome 正在跑时它的资料文件被占着扫不了** ——
/// 那种情况下 `chrome_scanned` 是 false，界面要说「先关掉 Chrome 再检测」，
/// 不能显示成「没找到痕迹」。
#[tauri::command]
pub async fn claude_traces() -> install::chrome::TraceReport {
    install::chrome::claude_traces().await
}

/// 卸掉 Chrome、删干净用户资料、再装回来。没装过就只装。
///
/// ⚠ **这个命令会毁掉数据**：书签、密码、扩展、全部站点数据一起没，不可恢复。
/// 界面负责在调用之前弹那一次确认框 —— 使用者要求确认之后全程自动、
/// 中途不再问，所以那一次确认必须把代价说全。
///
/// 只碰 Chrome。Edge、Firefox 一概不动。
#[tauri::command]
pub async fn chrome_reinstall(app: tauri::AppHandle) -> Result<String> {
    let _guard = operations::exclusive().await?;
    let rep = Reporter::new(app, events::TASK_CHROME, install::chrome::REINSTALL_TOTAL);
    let r = install::chrome::reinstall(&rep).await;
    match &r {
        Ok(d) => rep.done(d),
        Err(e) => rep.fail(&e.to_string()),
    }
    r
}

// ------------------------------------------------------------ 升级

#[tauri::command]
pub async fn upgrade_plan(channel: install::upgrade::Channel) -> install::upgrade::UpgradePlan {
    install::upgrade::plan(channel).await
}

#[tauri::command]
pub async fn upgrade_execute(
    app: tauri::AppHandle,
    channel: install::upgrade::Channel,
    force: bool,
    state: tauri::State<'_, AppState>,
) -> Result<String> {
    let _guard = operations::exclusive().await?;
    // v0.9.0：面板托管的那份在，就升它 —— 从官方源下新版、核对、替换，旧的留底。
    // 不在才走下面原来那条（官方安装脚本升 `~\.local\bin` 那份）。
    if install::managed::is_installed(install::managed::App::ClaudeCode) {
        let plan = install::upgrade::plan(channel).await;
        if !force
            && matches!(
                plan.action,
                install::upgrade::Action::UpToDate | install::upgrade::Action::WouldDowngrade
            )
        {
            return Ok(plan.detail);
        }
        let r = managed_install(
            app,
            install::managed::App::ClaudeCode,
            channel.as_str(),
            &state,
        )
        .await?;
        return Ok(r.detail);
    }

    // 七段：比版本 / 清残留 / 下脚本 / 跑安装器 / 按哈希核对 / 必要时自己补完 /
    // 重锁并恢复租约。第 5、6 两段是 v0.7.0 新增的，见 upgrade.rs 文件头。
    let rep = Reporter::new(app, events::TASK_UPGRADE, 7);
    let r = usecase::install_ops::upgrade(channel, force, &state.gate, &rep).await;
    if let Err(e) = &r {
        rep.fail(&e.to_string());
    }
    r
}

// ------------------------------------------------------------ 应用更新（0.25.3）

/// 上一次问到的更新状态。**不联网。**
#[tauri::command]
pub fn update_status() -> update::UpdateStatus {
    update::status()
}

/// 问一次 GitHub 有没有新版。
///
/// `manual = false` 是界面启动时那一次 —— 设置里关了「启动时检查更新」就不发请求；
/// `manual = true` 是设置页的「检查更新」。**只问，不下载、不安装。**
#[tauri::command]
pub async fn update_check(manual: bool) -> update::UpdateStatus {
    update::check(manual).await
}

/// 「跳过这个版本」。`None` = 取消跳过。
#[tauri::command]
pub async fn update_skip(version: Option<String>) -> Result<update::UpdateStatus> {
    let _guard = operations::exclusive_soon().await?;
    update::skip(version)
}

/// **一键更新**：下载 → 按同一个 Release 的 `SHA256SUMS.txt` 核对 → 面板退出 →
/// 退出处理重锁之后启动安装包（`/P /UPDATE /R`，装完自己重新打开面板）。
///
/// 只能由使用者在更新弹窗里点出来。拿的是**不查就绪**的那把锁：启动卡在「需要完成数据恢复」时
/// 也许正是新版能读懂的数据，那时候更新不该被拦下（2026-09-22 那次降级事故的反面）。
/// 锁本身还是要拿 —— 别的安装、迁移正跑着的时候退出，等于把它们腰斩。
#[tauri::command]
pub async fn update_install(app: tauri::AppHandle) -> Result<()> {
    let _guard = operations::exclusive_unchecked().await;
    let rep = Reporter::new(app.clone(), events::TASK_SELF_UPDATE, 4);
    match update::prepare(&rep).await {
        Ok((path, sha256)) => {
            audit::write(&format!(
                "一键更新：{} 已下载并核对 SHA-256，面板退出后启动安装包",
                path.display()
            ));
            rep.done("已核对，面板即将退出并开始安装，装完会自己重新打开");
            update::set_pending(path, sha256);
            // 留一小会儿给界面把最后那句显示出来，再退。
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            app.exit(0);
            Ok(())
        }
        Err(e) => {
            audit::write(&format!("一键更新失败：{e}"));
            rep.fail(&e.to_string());
            Err(e)
        }
    }
}
