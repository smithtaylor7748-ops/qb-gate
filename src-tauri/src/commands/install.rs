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
    let dir = install::managed::app_dir_of(which);
    let current = install::managed::record_of(&dir, which).map(|r| r.version);
    install::versions::history(&dir, which, current.as_deref())
}

/// Codex 官方最新版本号（GitHub releases/latest）。只读，不下载。
#[tauri::command]
pub async fn managed_codex_latest() -> Result<String> {
    install::managed::codex_latest_version().await
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
    let dir = install::managed::app_dir_of(which);
    let current = install::managed::record_of(&dir, which);
    let msg = install::versions::rollback(&dir, which, &version, current.as_ref())?;
    // 换上来的那份此刻是从版本库拷过来的，ACL 跟着源文件走 —— 必须重锁一遍，
    // 否则回滚等于顺手把门禁摘了。
    let n = gate::lock_for_managed()?;
    if cfg!(target_os = "macos") {
        Ok(format!("{msg}；受管启动仍需通过 IP 检查"))
    } else {
        Ok(format!("{msg}，已重新上锁 {n} 个副本"))
    }
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
    // 单独迁走的软件不在旧根下，跟着根走的只有还在 `<旧根>\<键>` 的那几个。
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

// ------------------------------------------------- 按软件迁移目录（2026-11-05）

/// 一个软件现在在哪、能不能迁。`app` 是界面上的软件键（见 [`relocate_where`]）。
#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct RelocateWhere {
    pub app: String,
    /// 现在的目录（还没装就是将来会装到的地方；Codex 桌面端是 MSIX 里的主程序）。
    pub path: Option<String>,
    /// 解析联结点之后的真实目录（Claude 桌面端搬过之后跟 `path` 不同）。
    pub real: Option<String>,
    pub installed: bool,
    /// 面板能直接迁它（Codex 桌面端是 false：只能去 Windows 设置里移动）。
    pub can_move: bool,
    /// Claude 桌面端：原位置现在是面板建的联结点（可以「迁回」）。
    pub relocated: bool,
    /// Claude 桌面端：联结点还在、指向的地方空了（卸载后留下的）。
    pub dangling: bool,
    /// 给人看的一句：迁移会做什么。
    pub note: String,
}

#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct RelocateReport {
    pub app: String,
    pub to: String,
    pub notes: Vec<String>,
}

fn product_of_key(app: &str) -> Option<install::antigravity::Product> {
    match app {
        "antigravity" => Some(install::antigravity::Product::Hub),
        "antigravity-ide" => Some(install::antigravity::Product::Ide),
        _ => None,
    }
}

fn cli_of_key(app: &str) -> Option<install::managed::App> {
    match app {
        "claude-code" => Some(install::managed::App::ClaudeCode),
        "codex" => Some(install::managed::App::Codex),
        _ => None,
    }
}

fn local_dir() -> std::path::PathBuf {
    dirs::data_local_dir().unwrap_or_default()
}

fn where_now(app: &str) -> Result<RelocateWhere> {
    use install::{managed, relocate, sillytavern};
    let show = |p: &std::path::Path| Some(p.display().to_string());
    let mut w = RelocateWhere {
        app: app.to_string(),
        path: None,
        real: None,
        installed: false,
        can_move: true,
        relocated: false,
        dangling: false,
        note: String::new(),
    };
    if let Some(which) = cli_of_key(app) {
        let dir = managed::app_dir_of(which);
        w.installed = managed::exe(which).is_file();
        w.path = show(&dir);
        w.note = if w.installed {
            "整份搬到新目录（同盘改名，跨盘复制核对后删源），搬完按新位置重新上锁。".into()
        } else {
            "这台机器上还没有面板托管的那份：会从官方源下载（核对 SHA-256 与数字签名）装进新目录，之后启动优先用它。原来那份不动。".into()
        };
        return Ok(w);
    }
    if let Some(product) = product_of_key(app) {
        let local = local_dir();
        w.path = show(&product.dir(&local));
        w.installed = product.launcher(&local).is_file();
        w.can_move = w.installed;
        w.note = "用官方安装器（核对 Google 签名）带目录参数重装到新目录；账户与登录不受影响。旧目录里剩下的程序会就地上锁。".into();
        return Ok(w);
    }
    match app {
        "sillytavern" => {
            let dir = sillytavern::managed_dir();
            w.installed = sillytavern::is_managed(&dir);
            w.can_move = w.installed;
            w.path = show(&dir);
            w.note =
                "整份搬到新目录，酒馆配置里的路径一起改；搬前先把 data\\default-user 备份一份。"
                    .into();
        }
        "gemini-cli" => {
            let found = install::detect::gemini_cli();
            w.installed = found.installed;
            w.path = found.path.as_deref().and_then(show);
            w.note = "用 npm install --prefix 在新目录装一份，面板以后先用它。终端里敲 gemini 仍是旧的那份 —— 不要了自己去 npm 卸。".into();
        }
        "claude-desktop" => {
            let p = relocate::desktop_place(&local_dir());
            w.installed = p.installed;
            w.can_move = p.installed && !p.relocated;
            w.relocated = p.relocated;
            w.dangling = p.dangling;
            w.path = show(&p.link);
            w.real = show(&p.real);
            w.note = "整份复制到 <新目录>\\AnthropicClaude，核对后原位置换成指向它的目录联结点；只许本机固定 NTFS 盘。随时可以「迁回」。".into();
        }
        "codex-desktop" => {
            let d = install::codex_desktop::detect().ok();
            let exe = d.and_then(|d| d.executable);
            w.installed = exe.is_some();
            w.can_move = false;
            w.path = exe;
            w.note = "这是 Microsoft Store 的应用，面板不搬它：在 Windows 设置 → 应用 → 已安装的应用 → Codex → 移动 里换盘，然后回来点「重新核对位置」。".into();
        }
        _ => return Err(GateError::Other(format!("不认识的软件：{app}"))),
    }
    Ok(w)
}

/// 一个软件现在在哪、迁移会做什么。键：`claude-code` `codex` `sillytavern` `gemini-cli`
/// `antigravity` `antigravity-ide` `claude-desktop` `codex-desktop`。
#[tauri::command]
pub async fn relocate_where(app: String) -> Result<RelocateWhere> {
    tokio::task::spawn_blocking(move || where_now(&app))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}

fn probe_for(app: &str, path: &std::path::Path) -> install::managed::Probe {
    use install::managed;
    if app == "claude-desktop" {
        let mut p = managed::probe_dir(path);
        let target = path.join(install::relocate::DESKTOP_DIR);
        if p.ok && target.exists() {
            p.ok = false;
            p.reason = Some(format!("{} 已经存在，面板不覆盖它。", target.display()));
        }
        return p;
    }
    let mut p = usecase::relocate_ops::check_target(app, path);
    if p.ok
        && std::fs::read_dir(path)
            .map(|mut r| r.next().is_some())
            .unwrap_or(false)
    {
        p.ok = false;
        p.reason = Some("这个目录里已经有东西了，换一个空目录（或者一个还不存在的目录）。".into());
    }
    p
}

/// 实时探测：这个目录能不能给这个软件用（本机固定 NTFS 盘、锁得上、空的、不跟别的软件目录嵌套）。
#[tauri::command]
pub async fn relocate_probe(app: String, path: String) -> Result<install::managed::Probe> {
    let path = std::path::PathBuf::from(path.trim());
    tokio::task::spawn_blocking(move || probe_for(&app, &path))
        .await
        .map_err(|e| GateError::Other(e.to_string()))
}

/// 正在从 `dir` 运行的程序 → 报错（迁移前拒绝正在跑的）。比较前规范化（联结点、短名、大小写）。
async fn refuse_running(dir: &std::path::Path, what: &str) -> Result<()> {
    let busy = install::managed::running_from(dir).await?;
    if busy.is_empty() {
        return Ok(());
    }
    Err(GateError::Other(format!(
        "{what}还开着（{}），先把它完全退出再迁移。什么都没动。",
        busy.iter()
            .map(|(pid, p)| format!("PID {pid} {p}"))
            .collect::<Vec<_>>()
            .join("；")
    )))
}

fn joined<T>(r: std::result::Result<T, tokio::task::JoinError>) -> Result<T> {
    r.map_err(|e| GateError::Other(e.to_string()))
}

/// **迁移**一个软件到 `path`。每种软件的做法见 `install::relocate` 的文件头。
///
/// 拿 `operations::exclusive()`；里面不再调任何会拿这把锁的命令（坑 7.51）——
/// 收编走的是不拿锁的 `managed_install`。
#[tauri::command]
pub async fn relocate_run(
    app: tauri::AppHandle,
    which: String,
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<RelocateReport> {
    use install::managed;
    let _guard = operations::exclusive().await?;
    let new = std::path::PathBuf::from(path.trim());
    {
        let (a, n) = (which.clone(), new.clone());
        let p = joined(tokio::task::spawn_blocking(move || probe_for(&a, &n)).await)?;
        if !p.ok {
            return Err(GateError::Other(p.reason.unwrap_or_default()));
        }
    }
    let mut notes: Vec<String> = Vec::new();

    if let Some(cli) = cli_of_key(&which) {
        let key = cli.key();
        let old = managed::app_dir_of(cli);
        if !managed::exe(cli).is_file() {
            // 收编：设置先指到新目录，再走托管安装（官方源、核 SHA-256 与签名）。装不成就把设置改回去。
            let previous = settings::load().app_dirs.get(key).cloned();
            usecase::relocate_ops::store_app_dir(key, &new)?;
            match managed_install(app.clone(), cli, "latest", &state).await {
                Ok(r) => notes.push(r.detail),
                Err(e) => {
                    let mut s = settings::load();
                    match previous {
                        Some(p) => s.app_dirs.insert(key.to_string(), p),
                        None => s.app_dirs.remove(key),
                    };
                    let _ = usecase::settings_ops::save(&s);
                    return Err(e);
                }
            }
        } else {
            refuse_running(&old, cli.label()).await?;
            // 设置先写、再重锁：维护窗口收尾时 `lock_all` 按设置里的新目录锁。
            let guard = gate::Maintenance::observing(&state.gate);
            let moved = managed::move_app_dir(Some(cli), &old, &new).and_then(|()| {
                usecase::relocate_ops::store_app_dir(key, &new).map_err(|e| {
                    GateError::Other(format!(
                        "设置没写进去（{e}），{}",
                        match managed::move_app_dir(Some(cli), &new, &old) {
                            Ok(()) => "已搬回原处".to_string(),
                            Err(b) => format!("搬回也失败了：{b}"),
                        }
                    ))
                })
            });
            if moved.is_err() && new.exists() {
                managed::lock_strays(&new);
            }
            let done = guard.finish(&state.gate).await;
            moved?;
            notes.push(format!("{} 已搬到 {}", cli.label(), new.display()));
            if old.exists() {
                let n = managed::lock_strays(&old);
                notes.push(format!("旧目录还剩东西，已就地上锁 {n} 个文件"));
            }
            notes.push(done.detail);
        }
    } else if let Some(product) = product_of_key(&which) {
        let running = crate::killswitch::antigravity_processes(None).await?;
        if !running.is_empty() {
            return Err(GateError::Other(
                "反重力 / 反重力 IDE 还开着，两个都完全退出之后再迁移。什么都没动。".into(),
            ));
        }
        let guard = gate::Maintenance::observing(&state.gate);
        let r = install::antigravity_setup::install_to(product, &new, &crate::sink::Silent).await;
        let done = guard.finish(&state.gate).await;
        notes.extend(r?);
        notes.push(done.detail);
        operations::changed(&events::ui(&app), "session", product.key());
    } else {
        match which.as_str() {
            "sillytavern" => {
                let n = new.clone();
                notes = joined(
                    tokio::task::spawn_blocking(move || {
                        usecase::relocate_ops::relocate_sillytavern(&n)
                    })
                    .await,
                )??;
            }
            "gemini-cli" => notes = usecase::relocate_ops::relocate_gemini_cli(&new).await?,
            "claude-desktop" => {
                let local = local_dir();
                refuse_running(&local.join(install::relocate::DESKTOP_DIR), "Claude 桌面端")
                    .await?;
                // 复制出去的存根不带原来的执行锁 —— 收尾时按新位置（经联结点）重锁。
                // `app-*\claude.exe` 从来不锁（加 Deny 桌面端开新窗口就崩）。
                let guard = gate::Maintenance::observing(&state.gate);
                let n = new.clone();
                let r = joined(
                    tokio::task::spawn_blocking(move || {
                        install::relocate::relocate_desktop(&local, &n)
                    })
                    .await,
                );
                let done = guard.finish(&state.gate).await;
                notes.extend(r??);
                notes.push(done.detail);
            }
            other => return Err(GateError::Other(format!("{other} 不能由面板迁移"))),
        }
    }
    audit::write(&format!(
        "迁移 {which} 到 {}：{}",
        new.display(),
        notes.join("；")
    ));
    tray::refresh(&app);
    Ok(RelocateReport {
        app: which,
        to: new.display().to_string(),
        notes,
    })
}

/// Claude 桌面端「迁回」：拆掉联结点、把文件搬回 `%LOCALAPPDATA%\AnthropicClaude`。
/// 联结点指向的地方已经空了（卸载后留下的）就只是把它收掉。
#[tauri::command]
pub async fn relocate_desktop_restore(state: tauri::State<'_, AppState>) -> Result<RelocateReport> {
    let _guard = operations::exclusive().await?;
    let local = local_dir();
    let link = local.join(install::relocate::DESKTOP_DIR);
    refuse_running(&link, "Claude 桌面端").await?;
    let guard = gate::Maintenance::observing(&state.gate);
    let l = local.clone();
    let r =
        joined(tokio::task::spawn_blocking(move || install::relocate::restore_desktop(&l)).await);
    let done = guard.finish(&state.gate).await;
    let mut notes = r??;
    notes.push(done.detail);
    audit::write(&format!("Claude 桌面端迁回：{}", notes.join("；")));
    Ok(RelocateReport {
        app: "claude-desktop".into(),
        to: link.display().to_string(),
        notes,
    })
}

/// Codex 桌面端是 MSIX：面板不搬，只打开 Windows 的「已安装的应用」页并给一句怎么做。
#[tauri::command]
pub fn codex_desktop_move_settings() -> Result<String> {
    std::process::Command::new("explorer.exe")
        .arg("ms-settings:appsfeatures")
        .spawn()
        .map_err(|e| GateError::Other(format!("打不开 Windows 设置：{e}")))?;
    Ok("已打开 Windows 设置：在「已安装的应用」里找到 Codex → 右边「…」→ 移动 → 选目标盘。移完回来点「重新核对位置」。".into())
}

#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct CodexDesktopWhere {
    pub executable: Option<String>,
    /// MSIX 包的安装目录（`Get-AppxPackage` 读的 `InstallLocation`）。
    pub location: Option<String>,
    /// 按 `\WindowsApps\` 认它的那几处判断在新位置上仍成立。
    pub watchdog_ok: bool,
    pub note: String,
}

/// 用 `Get-AppxPackage` 重新读 Codex 桌面端的位置，核对按 `\WindowsApps\` 认它的判断在新盘上仍成立
/// （商店把应用移到别的盘是 `<盘>:\WindowsApps\<包全名>`；看门狗与收进程本来就按读到的路径认）。
#[tauri::command]
pub async fn codex_desktop_location() -> Result<CodexDesktopWhere> {
    let d = joined(tokio::task::spawn_blocking(install::codex_desktop::detect).await)??;
    let exe = d.executable.clone();
    // `<安装目录>\app\ChatGPT.exe` → 安装目录。
    let location = exe.as_deref().and_then(|e| {
        std::path::Path::new(e)
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.display().to_string())
    });
    let watchdog_ok = exe.as_deref().is_some_and(|e| {
        install::inventory::norm(std::path::Path::new(e)).contains("\\windowsapps\\")
    });
    let note = match (&exe, watchdog_ok) {
        (None, _) => "没找到 Codex 桌面端（没装，或者 Windows 还没移完）。".into(),
        (Some(_), true) => {
            "位置已核对：看门狗按 Get-AppxPackage 读到的这个路径认它，换盘之后照样成立。".into()
        }
        (Some(_), false) => {
            "读到的位置不在 WindowsApps 下 —— 不像商店装的那份；面板按读到的可执行文件路径认它。"
                .into()
        }
    };
    Ok(CodexDesktopWhere {
        executable: exe,
        location,
        watchdog_ok,
        note,
    })
}
