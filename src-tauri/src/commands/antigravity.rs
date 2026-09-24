//! 反重力（Hub / IDE）与 Gemini CLI 槽位的命令（0.26.0）。所有会改变状态的动作都由界面显式触发。
//!
//! 启动 / 关闭走 `usecase::antigravity_ops`，那里接的是 Claude 页同一条门禁链
//! （`workspace::launch`：验 IP → 先关正在跑的 → 托管会话 → 租约）；这里只做
//! 参数搬运、任务记录和「起来之后挂看门狗」。
use crate::{
    error::{GateError, Result},
    events, operations, AppState,
};
use qb_app::usecase::antigravity_ops;
use qb_install::install::antigravity::Product;

#[tauri::command]
pub async fn antigravity_status() -> Result<antigravity_ops::AntigravityStatus> {
    tokio::task::spawn_blocking(antigravity_ops::status)
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}

/// 这个产品此刻有几个进程在跑。**只看不动**，给启动确认框用（0.27.0）。
///
/// 起反重力会先把正在跑的那份关掉（它是 Electron 单实例），所以界面要事先确认一次。
/// 可什么都没在跑的时候那个确认框就是纯噪音 —— 界面先问这里，真有东西在跑才弹。
///
/// 两件事必须是这个样子：
///
/// - **走的是启动链自己那套证据**（`killswitch::antigravity_processes` → `preview`，
///   按安装目录认，不按进程名）。另写一份「查查看在不在」的实现，就会出现
///   「弹窗说没有、启动时却关掉了你的窗口」这种两套说法；
/// - ⛔ **枚举失败返回 `Err`，界面照旧弹确认框**。不许把「查不出来」降级成「没有在跑」——
///   §7.17 那条（`-AsArray` 让一键关闭永远数出 0 个）就是这么栽的。
#[tauri::command]
pub async fn antigravity_running(product: Product) -> Result<usize> {
    Ok(crate::killswitch::antigravity_processes(Some(product))
        .await?
        .len())
}

/// 验 IP → 解锁 → 关掉正在跑的 → 起 → 挂看门狗（桌面档）。门禁不过就一个进程都不起。
#[tauri::command]
pub async fn antigravity_launch(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    product: Product,
) -> Result<()> {
    let _guard = operations::exclusive().await?;
    let mut task = operations::Run::start(
        events::sink(&app),
        &format!("启动{}", product.label()),
        product.key(),
    )?;
    task.phase("检查安装、门禁与正在跑的实例", 20)?;
    let result = task
        .finish(antigravity_ops::launch(product, &state.gate).await)
        .map(|_| ());
    if result.is_ok() {
        let target = match product {
            Product::Hub => crate::launch::LaunchTarget::Antigravity,
            Product::Ide => crate::launch::LaunchTarget::AntigravityIde,
        };
        if target.gated() {
            crate::app::start_watchdog(target.watch_mode(), &state, Some(app.clone()));
        }
    }
    operations::changed(&events::ui(&app), "session", product.key());
    result
}

/// 关掉某个产品：面板托管的按会话停并交回租约，其余按安装目录核验后关。
#[tauri::command]
pub async fn antigravity_close(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    product: Product,
) -> Result<usize> {
    let _guard = operations::exclusive().await?;
    let mut task = operations::Run::start(
        events::sink(&app),
        &format!("关闭{}", product.label()),
        product.key(),
    )?;
    let result = task.finish(antigravity_ops::close(product, &state.gate).await);
    operations::changed(&events::ui(&app), "session", product.key());
    result
}

/// 一键安装反重力（Hub 或 IDE）。A 路 winget，没成从 Google 官方域直下再静默装。
///
/// 顺序（照 `codex_desktop_install` 那条）：拿独占锁 → **先关掉正在跑的那份**
/// → 维护窗口 → 装 → 收尾重锁并把租约还回去 → 刷新界面。
///
/// 三件都不能省：
///
/// 1. **先关。** 反重力是 Electron 单实例 + 托盘后台运行，装的时候它开着，安装器会
///    换不动文件 —— 而且多半**不报错**（§7.21：官方安装器换不动也打印 successfully）。
///    关不掉就不装；
/// 2. **维护窗口。** 装完的是全新的 exe，继承的是干净 ACL，门禁那条 Deny 跟着旧文件
///    没了。`Maintenance::observing` → `finish` 负责重新上锁并交回租约。**漏了这一步，
///    使用者装完一次反重力，IP 锁就对它失效了，而界面上一切正常**；
/// 3. **回读核对**在 `antigravity_setup::install` 里做（看主程序在不在、版本是多少），
///    不看安装器的退出码。
///
/// ⛔ `operations::exclusive()` **只在这一层拿**。被它调用的
/// `antigravity_setup::install` 一次都不许再拿（坑 7.51）。
#[tauri::command]
pub async fn antigravity_install(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    product: Product,
    local: Option<String>,
    force: bool,
) -> Result<String> {
    use crate::sink::ProgressSink;
    let _guard = operations::exclusive().await?;
    let rep = events::Reporter::new(
        app.clone(),
        events::TASK_INSTALL_ANTIGRAVITY,
        qb_install::install::antigravity_setup::TOTAL,
    );
    // 按安装目录认的那套证据（跟一键关闭、看门狗同源），不按进程名杀。
    if let Err(e) = crate::killswitch::execute_antigravity(Some(product)).await {
        let msg = format!("没能先关闭{}，没有安装：{e}", product.label());
        rep.fail(&msg);
        return Err(GateError::Other(msg));
    }
    let guard = crate::gate::Maintenance::observing(&state.gate);
    let local_path = local
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from);
    let r = qb_install::install::antigravity_setup::install(
        product,
        local_path.as_deref(),
        force,
        &rep,
    )
    .await;
    let done = guard.finish(&state.gate).await;
    operations::changed(&events::ui(&app), "session", product.key());
    match r {
        Ok(o) => {
            let detail = format!(
                "{} {} 已装好（{}）。{}",
                product.label(),
                o.version.clone().unwrap_or_else(|| "版本读不出".into()),
                match o.method {
                    qb_install::install::antigravity_setup::Method::Winget => "winget",
                    qb_install::install::antigravity_setup::Method::Direct => "官网直下 + 验签",
                    qb_install::install::antigravity_setup::Method::LocalFile => "本地安装包",
                },
                done.detail
            );
            for line in &o.log {
                rep.log(qb_install::install::antigravity_setup::TOTAL, line);
            }
            crate::audit::write(&detail);
            rep.done(&detail);
            Ok(detail)
        }
        Err(e) => {
            let msg = format!("{e} {}", done.detail);
            crate::audit::write(&format!("{}安装失败：{e}", product.label()));
            rep.fail(&msg);
            Err(GateError::Other(msg))
        }
    }
}

/// 官网上现在是哪一版（只读下载页，不下载不安装）。给软件页的「检查」用。
///
/// 不拿独占锁：只读，一次网络往返，不该把看门狗巡检挡在外面（同 `codex_desktop_latest`）。
#[tauri::command]
pub async fn antigravity_latest(product: Product) -> Result<String> {
    use qb_install::install::antigravity_setup::{self as setup, Arch};
    Ok(setup::resolve(product, Arch::current()).await?.version)
}

/// Hub 的「自动检查更新」。只并入一个键，重启 Hub 生效。
#[tauri::command]
pub async fn antigravity_auto_update_set(enabled: bool) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    tokio::task::spawn_blocking(move || antigravity_ops::set_auto_update(enabled))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}

// ------------------------------------------------------------------ 反重力账户槽位（0.32.0）

/// 新建一个账户槽位：IDE（`--user-data-dir`）与 Gemini CLI（`GEMINI_CLI_HOME`）**两半一起建**。
/// 登录各自在官方窗口里做，面板不碰凭据。
#[tauri::command]
pub async fn antigravity_ide_create(label: String) -> Result<String> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(move || antigravity_ops::ide_create(&label))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}

/// 切换 = 换激活槽位，不关不起任何东西。正在跑的 IDE 继续用它起来时那份资料；
/// 下次从面板起 IDE 才用新槽位（界面上写明，跟 Codex 页同一个语义）。
///
/// ⛔ **酒馆的 Gemini 桥接在跑时不许切。** 0.31.0 之前这道守卫在 `gemini_switch` 上；
/// 0.32.0 一条槽位管两半，切一次连 CLI 那一半一起换 —— 守卫留在原处就等于没有，
/// 桥接会继续拿着上一条槽位的 `GEMINI_CLI_HOME` 跑，而界面显示的是新的那条。
#[tauri::command]
pub async fn antigravity_ide_select(id: String) -> Result<()> {
    let _guard = operations::exclusive().await?;
    if qb_app::gemini_bridge::running() {
        return Err(GateError::Other(
            "酒馆的 Gemini 桥接正在跑、用着当前账户。先停酒馆再切换。".into(),
        ));
    }
    tokio::task::spawn_blocking(move || antigravity_ops::ide_select(&id))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}

#[tauri::command]
pub async fn antigravity_ide_archive(id: String) -> Result<()> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(move || antigravity_ops::ide_archive(&id))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}

/// 把 `from` 那条账户并进 `into`。升级上来的老槽位是「只填了一半」的行，
/// 使用者认得出哪两条是同一个 Google 账户时用这个并起来。**只改索引，不搬目录。**
#[tauri::command]
pub async fn antigravity_account_attach(into: String, from: String) -> Result<()> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(move || antigravity_ops::ide_attach(&into, &from))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}

#[tauri::command]
pub async fn antigravity_account_rename(id: String, label: String) -> Result<()> {
    let _guard = operations::exclusive().await?;
    tokio::task::spawn_blocking(move || antigravity_ops::ide_rename(&id, &label))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}

/// 反重力 Hub 的联网额度：档位、AI 积分、5 小时 / 每周四格（2026-09-23 改走跟账户槽位同一套）。
///
/// `refresh = false` 只读「最近一次」，**绝不联网**；`true` 才问 —— 只有使用者点了刷新才传。
/// 边界（只读三个调用、令牌过期在内存里换新、不发 `onboardUser`）见
/// `qb_app::usecase::antigravity_quota` 的模块头。
///
/// 不拿独占锁：几次只读的 HTTP 往返，别把看门狗巡检挡在外面。
#[tauri::command]
pub async fn antigravity_hub_quota(
    refresh: bool,
) -> Result<Option<qb_app::usecase::antigravity_quota::AntigravityOnlineQuota>> {
    qb_app::usecase::antigravity_quota::quota(
        qb_app::usecase::antigravity_quota::Source::Hub,
        refresh,
    )
    .await
}

/// 一条反重力账户槽位的联网额度（用它 IDE 那一半的令牌）。`refresh` 同上。
#[tauri::command]
pub async fn antigravity_account_quota(
    id: String,
    refresh: bool,
) -> Result<Option<qb_app::usecase::antigravity_quota::AntigravityOnlineQuota>> {
    qb_app::usecase::antigravity_quota::quota(
        qb_app::usecase::antigravity_quota::Source::Account(id),
        refresh,
    )
    .await
}

/// 本机用量：扫语言服务器的对话记录库，按 `days` 出小结（`1` 今天 / `7` / `30` / `0` 全部）。
///
/// 零网络。不拿独占锁 —— 只读几十个 SQLite 库，百毫秒量级，别把看门狗巡检挡在外面
/// （同 `accounts_token_summary`）。价目跟 Claude 用量卡同一份 `Catalog`（抓回来的优先，
/// 没有就内置快照），只用来算「缓存省下」。
#[tauri::command]
pub async fn antigravity_usage(days: i64) -> Result<antigravity_ops::AntigravityUsage> {
    let prices = qb_station::station::pricing::Catalog::new(super::station::stored_prices());
    tokio::task::spawn_blocking(move || antigravity_ops::usage(days, &prices))
        .await
        .map_err(|e| GateError::Other(format!("统计反重力用量的任务异常结束：{e}")))
}

// ------------------------------------------------------------------ 账户的 Gemini CLI 那一半
//
// 0.32.0 起 `gemini_accounts` / `gemini_create` / `gemini_switch` / `gemini_archive`
// 没有了：两半同属一条账户槽位，列表、新建、切换、移除全走上面那组
// `antigravity_ide_*`。原来 `gemini_switch` 上那道「桥接在跑就不许切」的守卫
// **搬进了 `antigravity_ide_select`** —— 现在切一次动的是两半，守卫漏在这里
// 就等于没有。剩下这一条是真正跟 CLI 绑死的动作。

/// 打开一个 Gemini CLI 登录窗口（`GEMINI_CLI_HOME` 指向这条账户的 CLI 那一半）。面板不碰凭据。
#[tauri::command]
pub async fn gemini_login(id: String) -> Result<()> {
    let _guard = operations::exclusive_soon().await?;
    tokio::task::spawn_blocking(move || antigravity_ops::gemini_login(&id))
        .await
        .map_err(|e| GateError::Other(e.to_string()))?
}

/// `npm install -g @google/gemini-cli`，进度走 `TASK_INSTALL_GEMINI_CLI`（三段）。
///
/// 0.29.0 之前这里是 `exclusive_soon()` + 一个 `spawn_blocking`：那时它只是「弹个窗口」，
/// 瞬间就返回。现在它**等 npm 装完**（几十秒到几分钟），所以：
///
/// - 锁改成 `exclusive()`。`exclusive_soon()` 是 20 秒超时的那把，长任务拿它会在别人
///   排队时把自己等挂掉；
/// - ⛔ 锁**只在这一层拿**。`antigravity_ops::gemini_cli_install` 里一次都不许再拿 ——
///   tokio 的 Mutex 不可重入，第二次就是永远等着（坑 7.51，`architecture.rs` 的
///   `helpers_called_under_the_exclusive_lock_never_take_it_again` 钉着这件事）。
#[tauri::command]
pub async fn gemini_cli_install(app: tauri::AppHandle) -> Result<String> {
    use crate::sink::ProgressSink;
    let _guard = operations::exclusive().await?;
    let rep = events::Reporter::new(
        app.clone(),
        events::TASK_INSTALL_GEMINI_CLI,
        antigravity_ops::GEMINI_INSTALL_TOTAL,
    );
    match antigravity_ops::gemini_cli_install(&rep).await {
        Ok(detail) => {
            operations::changed(&events::ui(&app), "session", "gemini-cli");
            rep.done(&detail);
            Ok(detail)
        }
        Err(e) => {
            let msg = e.to_string();
            crate::audit::write(&format!("Gemini CLI 安装失败：{msg}"));
            rep.fail(&msg);
            Err(GateError::Other(msg))
        }
    }
}
