//! 反重力（Hub / IDE）与 Gemini CLI 槽位的启动、关闭、状态 —— 显式动作，与中转调度隔离（0.26.0）。
//!
//! # 起反重力走 Claude 页那条门禁链
//!
//! [`crate::workspace::launch`]：`gated()` 验 IP 解锁 → 先关掉正在跑的那份（单实例）→
//! `sessions::start`（Job Object）→ 租约 → 命令层挂看门狗（桌面档，零宽限）。
//! 没有槽位（令牌在 Windows 凭据管理器，目录隔离隔不开），起的就是使用者本机那份登录。
//!
//! # 起反重力之后自动附加汉化引擎
//!
//! Hub / IDE 起来要几秒才把调试端口立起来。`attach_on_launch` 开着时这里起一个后台任务
//! 探活，探通了再 `antigravity_ui::start()`。
//!
//! ## ⛔ 判定要问「CDP 应答得了吗」，不要问「端口文件变了吗」
//!
//! 0.26.0 等的是 `DevToolsActivePort` 的**修改时间发生变化**：`workspace::launch` 返回之后
//! 才采样基准值，而 Chromium 在进程刚起来的几百毫秒里就把文件写完了 —— 采样到的已经是新值，
//! `before == now` 恒成立，30 次全落空。实机日志里连着两次都是同一句
//! 「起 Hub 之后 30 秒没等到调试端口文件」，而那个文件明明早就写好了。
//!
//! 症状是**汉化根本不生效，而且没有任何人在报错**：引擎从没附加过，界面上一片安静，
//! 只有审计日志里那一行。所以现在还多做两件事：失败原因写进
//! [`antigravity_ui::note_attach_error`]（界面上看得见），窗口放到 60 s（IDE 起得慢）。
//!
//! # Gemini CLI 槽位
//!
//! 「登录」= 起一个带窗口的 Gemini CLI 交互会话（`GEMINI_CLI_HOME` 指向槽位），
//! 让 CLI 自己走 Google OAuth；面板只看 `oauth_creds.json` 在不在。
//! 「安装」= `npm install -g @google/gemini-cli`，输出流进软件页的进度条（0.29.0 改的，
//! 原来是弹一个 `cmd /k` 窗口而那个窗口里 npm 从来没跑起来过，见
//! [`gemini_cli_install`] 的说明）。面板不分发这个包，装的是 npm 源上的官方包。
//!
//! # 反重力 IDE 槽位、账户状态与本机用量（0.30.0）
//!
//! - **IDE 槽位** = 一个 `--user-data-dir`（`qb_accounts::antigravity::ide`）。「登录」= 起 IDE，
//!   在它的窗口里登 Google；面板只问状态库里令牌那一行的长度。Hub 没有槽位（令牌在凭据管理器）。
//! - **账户状态与额度** 读的是 IDE 自己写在 `state.vscdb` 里的 `userStatus`（邮箱、档位、
//!   各模型剩余比例与重置时间）—— 零网络请求，**不调 Google 的内部接口**。读哪一份：
//!   激活槽位的，没有槽位就是默认资料目录那份。
//! - **用量** 从语言服务器的对话记录库数出来（`qb_accounts::antigravity::usage`），
//!   装进 Claude 那套 [`super::token_summary`]，命中率 / 缓存省下 / 时间档同一个算法。
//!
//! 起 IDE 之前把激活槽位的目录告诉汉化引擎（[`antigravity_ui::set_ide_user_data_dir`]）——
//! 面板起的那份 IDE 的 `DevToolsActivePort` 在槽位目录里，不在 `%APPDATA%\Antigravity IDE`。
use crate::error::{GateError, Result};
use crate::gate::GateState;
use qb_accounts::antigravity::{account, hub, ide, status as ag_status, usage as ag_usage};
use qb_contract::domain::{Client, IdentityKind, Session};
use qb_install::install::antigravity::{self, Product};
use qb_station::station::pricing;
use serde::Serialize;
use std::path::PathBuf;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ProductStatus {
    pub product: Product,
    pub label: String,
    pub installed: bool,
    pub path: String,
    /// 面板托管、正在跑的会话（不含使用者自己从开始菜单起的 —— 那要枚举进程，太重，软件页才做）。
    pub session_id: Option<String>,
    pub pid: Option<u32>,
    /// 语言服务器的数据目录（只显示）。
    pub data_dir: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AntigravityStatus {
    pub hub: ProductStatus,
    pub ide: ProductStatus,
    /// 归不归门禁管（设置里存的是反义的 `antigravity_outside_gate`）。
    pub under_gate: bool,
    /// Hub 的「自动检查更新」。`None` = 读不到（还没起过）。
    pub auto_update: Option<bool>,
    /// 语言服务器日志里见没见过「Auth succeeded」。`None` = 没日志。**零凭证信号。**
    pub login_seen: Option<bool>,
    /// Hub 登没登录（凭据管理器里那条 `gemini:antigravity` 在不在）。**不读内容。**
    pub hub_logged_in: bool,
    /// Hub 登的是谁（0.32.0）。
    ///
    /// **零网络请求** —— 邮箱是从那条凭据里的 `id_token` 载荷本机解出来的，
    /// 不是问 Google 要的。Hub 在本机别处一个字的身份都没写，见
    /// [`qb_accounts::antigravity::hub`] 的模块头。
    pub hub_identity: Option<hub::AntigravityHubIdentity>,
    /// 读 Hub 身份时出的错（凭据读不出来 / 形状不认识），界面照实显示。
    ///
    /// ⛔ **读不出来不是「没登录」**（§7.17）。两者在界面上是两句话。
    pub hub_identity_error: Option<String>,
    pub gemini_cli_installed: bool,
    /// 反重力账户槽位（0.32.0）：**一个 Google 账户一条，底下两半**
    /// （IDE 的 `--user-data-dir` + Gemini CLI 的 `GEMINI_CLI_HOME`）。
    ///
    /// 取代了 0.30.0 的 `ide_accounts` + `gemini` 两份清单 —— 那两份在界面上是两个页签，
    /// 于是同一个账户要建两次、登两次，而只用 IDE 的人永远看着一句
    /// 「Gemini CLI · 0 个槽位」。升级迁移见 [`account::migrate`]。
    pub accounts: account::AntigravityAccounts,
    /// 下面那份账户状态读自哪一份 IDE 资料：`"槽位「工作」"` 或 `"默认资料目录"`。
    pub identity_source: String,
    /// IDE 写在本机的账户状态（邮箱 / 档位 / 各模型剩余额度）。`None` = 那份资料还没登录过。
    pub identity: Option<ag_status::AntigravityIdentity>,
    /// 读账户状态时出的错（库正忙 / 损坏），界面照实显示；`None` = 没出错。
    pub identity_error: Option<String>,
}

/// 此刻该读哪一份 IDE 资料目录：激活槽位的，没有就是默认那份。
///
/// 顺带把它告诉汉化引擎 —— IDE 的调试端口文件在这个目录里。
fn ide_profile_in_use() -> Result<(String, PathBuf)> {
    let roaming = dirs::config_dir().unwrap_or_default();
    let active = account::active_ide(&account::root())?;
    let (source, dir) = match &active {
        Some((_, label, dir)) => (format!("账户「{label}」"), dir.clone()),
        None => (
            "默认资料目录".to_string(),
            antigravity::user_data_dir(Product::Ide, &roaming),
        ),
    };
    crate::plugins::antigravity_ui::set_ide_user_data_dir(active.map(|(_, _, dir)| dir));
    Ok((source, dir))
}

/// 面板启动时调一次：先把 0.30.0 的两套老槽位升上来（[`account::migrate`]，幂等），
/// 再把激活槽位的目录告诉汉化引擎（见 [`ide_profile_in_use`]）。
///
/// 迁移失败不让面板起不来：写一条审计，界面照旧 —— 老索引一个字节没动，
/// 下次启动还会再试一次。
pub fn sync_ide_profile() -> Result<()> {
    match account::migrate(&account::root(), &ide::root(), &qb_accounts::gemini::root()) {
        Ok(0) => {}
        Ok(n) => crate::audit::write(&format!(
            "反重力账户槽位：从旧清单升上来 {n} 条（只换索引，目录一个都没搬）"
        )),
        Err(e) => crate::audit::write(&format!("反重力账户槽位升级没成功：{e}")),
    }
    ide_profile_in_use().map(|_| ())
}

fn client_of(product: Product) -> Client {
    match product {
        Product::Hub => Client::Antigravity,
        Product::Ide => Client::AntigravityIde,
    }
}

fn product_status(product: Product, sessions: &[Session]) -> ProductStatus {
    let local = dirs::data_local_dir().unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_default();
    let exe = product.launcher(&local);
    let client = client_of(product);
    let live = sessions
        .iter()
        .find(|s| s.context.client == client && s.state == "running");
    ProductStatus {
        product,
        label: product.label().into(),
        installed: exe.is_file(),
        path: exe.display().to_string(),
        session_id: live.map(|s| s.id.clone()),
        pid: live.map(|s| s.pid),
        data_dir: product.data_dir(&home).display().to_string(),
    }
}

pub fn status() -> Result<AntigravityStatus> {
    let _ = crate::sessions::refresh();
    let sessions = crate::sessions::list();
    let roaming = dirs::config_dir().unwrap_or_default();
    let (identity_source, profile) = ide_profile_in_use()?;
    // 身份读不出来不让整页报错：槽位列表、启动磁贴都还要用。错误单独带回去显示。
    let (identity, identity_error) = match ag_status::identity(&profile) {
        Ok(id) => (id, None),
        Err(e) => (None, Some(e.to_string())),
    };
    // Hub 那一侧同理：读不出来不让整页报错，也**不降级成「没登录」**。
    let (hub_identity, hub_identity_error) = hub_identity();
    Ok(AntigravityStatus {
        hub: product_status(Product::Hub, &sessions),
        ide: product_status(Product::Ide, &sessions),
        under_gate: crate::settings::antigravity_under_gate(),
        auto_update: antigravity::auto_update_enabled(&roaming),
        login_seen: antigravity::login_seen_in_log(&roaming),
        hub_logged_in: hub_logged_in(),
        hub_identity,
        hub_identity_error,
        gemini_cli_installed: qb_install::install::detect::gemini_cli_candidates()
            .into_iter()
            .any(|p| p.is_file()),
        accounts: with_login_health(account::list(&account::root())?),
        identity_source,
        identity,
        identity_error,
    })
}

/// 联网额度那条路问出来的「Google 不认这份登录了」叠到账户列表上（2026-09-23）。
///
/// 本机文件只说得出「有没有令牌」—— IDE 那半只问令牌那一行的长度，CLI 那半只看凭据文件
/// 在不在。刷新令牌被 Google 作废之后，这两样都还在，账户行上就一直是「已登录」、没有「登录」
/// 可点。[`super::login_health`] 记着点刷新时 Google 回的 `invalid_grant`，凭据文件一变就作废。
fn with_login_health(mut accounts: account::AntigravityAccounts) -> account::AntigravityAccounts {
    use super::login_health;
    for a in &mut accounts.slots {
        if let Some(home) = a.cli_dir.as_deref() {
            let creds = qb_accounts::gemini::creds_path(std::path::Path::new(home));
            if let Some(reason) = login_health::check(&login_health::gemini_cli_key(&a.id), &creds)
            {
                a.cli_logged_in = false;
                a.cli_auth_state = reason;
            }
        }
        if let Some(dir) = a.ide_dir.as_deref() {
            let db = ag_status::state_db(std::path::Path::new(dir));
            if let Some(reason) =
                login_health::check(&login_health::antigravity_ide_key(&a.id), &db)
            {
                a.ide_logged_in = false;
                a.ide_auth_state = reason;
            }
        }
    }
    accounts
}

// ------------------------------------------------------------------ Hub 的账户（0.32.0）

/// Hub 登没登录。只问那条凭据在不在，**不读内容**。
fn hub_logged_in() -> bool {
    hub::logged_in()
}

/// Hub 登的是谁。零网络请求，理由见 [`hub`] 的模块头。
///
/// 读不出来时返回 `(None, Some(原因))` —— **不许降级成「没登录」**（§7.17）：
/// 「没登录」和「读不出来」在界面上是两句话，下一步做什么完全不同。
fn hub_identity() -> (Option<hub::AntigravityHubIdentity>, Option<String>) {
    match hub::identity() {
        Ok(id) => (id, None),
        Err(e) => (None, Some(e.to_string())),
    }
}

// ------------------------------------------------------------------ 反重力账户槽位（0.32.0）

/// 新建一个账户槽位：**IDE 与 Gemini CLI 两半一起建**。
///
/// IDE 那一半的偏好（`settings.json` / `keybindings.json`）从默认资料目录复制，
/// 登录一个字节不复制（§7.29：复制式迁移造过两份 refresh token）。
pub fn ide_create(label: &str) -> Result<String> {
    let roaming = dirs::config_dir().unwrap_or_default();
    let seed = antigravity::user_data_dir(Product::Ide, &roaming);
    let id = account::create(
        &account::root(),
        label,
        seed.is_dir().then_some(seed.as_path()),
    )?;
    crate::audit::write(&format!("新建反重力账户槽位 {id}（{label}）"));
    Ok(id)
}

/// 切换 = 换激活槽位。**不关、不起任何东西**：正在跑的 IDE 继续用它起来时那份资料，
/// 下次从面板起 IDE 才用新槽位（跟 Codex 页同一个语义，界面上写明）。
pub fn ide_select(id: &str) -> Result<()> {
    account::select(&account::root(), id)?;
    let _ = ide_profile_in_use();
    crate::audit::write(&format!("反重力账户切换到 {id}"));
    Ok(())
}

/// 把 `from` 那条并进 `into`（使用者自己认这两条是同一个 Google 账户）。
///
/// 面板**不替他认** —— IDE 的邮箱在状态库里，而 CLI 那边只看凭据文件在不在、
/// 从不读内容，没有任何依据把两者对上号。见 [`account::attach`]。
pub fn ide_attach(into: &str, from: &str) -> Result<()> {
    account::attach(&account::root(), into, from)?;
    crate::audit::write(&format!(
        "反重力账户 {from} 并入 {into}（只改索引，没搬目录）"
    ));
    Ok(())
}

pub fn ide_rename(id: &str, label: &str) -> Result<()> {
    account::rename(&account::root(), id, label)?;
    crate::audit::write(&format!("反重力账户 {id} 改名为「{label}」"));
    Ok(())
}

pub fn ide_archive(id: &str) -> Result<()> {
    account::archive(&account::root(), id)?;
    // 这条槽位联网问到的额度、换新来的令牌，一起从内存里忘掉。
    super::antigravity_quota::forget(&super::antigravity_quota::Source::Account(id.to_string()));
    super::tavern_quota::forget_gemini(id);
    crate::audit::write(&format!("反重力 IDE 槽位 {id} 已移除（文件留在归档里）"));
    Ok(())
}

// ------------------------------------------------------------------ 本机用量（0.30.0）

/// 一个模型在这一档时间范围里的合计。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AntigravityModelUsage {
    pub model: String,
    #[ts(type = "number")]
    pub input: i64,
    #[ts(type = "number")]
    pub output: i64,
    #[ts(type = "number")]
    pub cache_read: i64,
    #[ts(type = "number")]
    pub messages: i64,
}

/// 反重力的本机用量：Claude 那套小结 + 按模型的明细 + 覆盖了多少的证据。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AntigravityUsage {
    pub summary: super::token_summary::TokenSummary,
    /// 按模型，输入多的在前。
    pub models: Vec<AntigravityModelUsage>,
    #[ts(type = "number")]
    pub files_read: u32,
    #[ts(type = "number")]
    pub files_failed: u32,
    #[ts(type = "number")]
    pub legacy_skipped: u32,
    #[ts(type = "number")]
    pub incomplete: u32,
    #[ts(type = "number")]
    pub duplicates: u32,
    pub scanned_dirs: Vec<String>,
    /// 本地时间 `HH:MM`。
    pub checked_at: String,
}

/// 扫一遍对话记录库并按 `days` 出小结（`1` = 今天，`7` / `30`，`0` = 全部）。
///
/// 零网络。`prices` 只用来算「缓存省下」；认不出价的模型跳过并报数，不拿别的模型的价去凑。
pub fn usage(days: i64, prices: &pricing::Catalog) -> AntigravityUsage {
    let home = dirs::home_dir().unwrap_or_default();
    let scan = ag_usage::scan(&ag_usage::conversation_dirs(&home));
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    summarize_scan(scan, days, &today, prices)
}

/// `usage` 的纯函数部分：给定扫描结果与今天，出小结与按模型明细。
pub fn summarize_scan(
    scan: ag_usage::AntigravityUsageScan,
    days: i64,
    today: &str,
    prices: &pricing::Catalog,
) -> AntigravityUsage {
    use std::collections::BTreeMap;
    let summary = super::token_summary::summarize(&scan.buckets, days, today, prices);
    let since = super::token_summary::since_day(days, today);
    let mut per: BTreeMap<&str, AntigravityModelUsage> = BTreeMap::new();
    for b in scan
        .buckets
        .iter()
        .filter(|b| since.as_deref().is_none_or(|s| b.day.as_str() >= s))
    {
        let e = per
            .entry(b.model.as_str())
            .or_insert_with(|| AntigravityModelUsage {
                model: b.model.clone(),
                input: 0,
                output: 0,
                cache_read: 0,
                messages: 0,
            });
        e.input += b.input;
        e.output += b.output;
        e.cache_read += b.cache_read;
        e.messages += b.messages;
    }
    let mut models: Vec<AntigravityModelUsage> = per.into_values().collect();
    models.sort_by_key(|m| std::cmp::Reverse(m.input + m.cache_read));
    AntigravityUsage {
        summary,
        models,
        files_read: scan.files_read,
        files_failed: scan.files_failed,
        legacy_skipped: scan.legacy_skipped,
        incomplete: scan.incomplete,
        duplicates: scan.duplicates,
        scanned_dirs: scan.scanned_dirs,
        checked_at: chrono::Local::now().format("%H:%M").to_string(),
    }
}

/// 起某个产品。门禁不过就一个进程都不起。
pub async fn launch(product: Product, gate: &GateState) -> Result<Session> {
    // IDE 的调试端口文件跟着槽位走：起之前先让汉化引擎知道该去哪个目录读。
    if product == Product::Ide {
        ide_profile_in_use()?;
    }
    let session =
        crate::workspace::launch(client_of(product), IdentityKind::Official, "", None, gate)
            .await?;
    // 0.27.0：两个产品都挂 —— IDE 的调试端口是面板起它的时候补上的（`desktop_arguments`），
    // 使用者自己从开始菜单起的那份没有端口，也就附不上去。
    if crate::plugins::antigravity_ui::load_config().attach_on_launch {
        tokio::spawn(attach_when_ready(product));
    }
    Ok(session)
}

/// 自动附加的等待上限。IDE 是 VS Code 分支，冷启动比 Hub 慢，给到 60 s。
const ATTACH_WINDOW: u32 = 60;

/// 等反重力把调试端口立起来再附加引擎。
///
/// **探的是「列得出页面吗」**，不是「文件变了吗」—— 见模块文档里那一节。
async fn attach_when_ready(product: Product) {
    for _ in 0..ATTACH_WINDOW {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if crate::plugins::antigravity_ui::running() {
            // 另一个产品的那一轮已经把引擎起起来了：它本来就同时看两个源。
            return;
        }
        if !crate::plugins::antigravity_ui::cdp_ready(product).await {
            continue;
        }
        match crate::plugins::antigravity_ui::start().await {
            Ok(_) => crate::plugins::antigravity_ui::note_attach_error(""),
            Err(e) => {
                let msg = format!("起{}之后自动附加汉化引擎失败：{e}", product.label());
                crate::plugins::antigravity_ui::note_attach_error(&msg);
                crate::audit::write(&msg);
            }
        }
        return;
    }
    // 等不到的时候说清楚是哪个产品、下一步能做什么 —— 0.26.0 这句只进审计日志，
    // 界面上一片安静，使用者看到的就只有「还是英文」。
    let msg = match product {
        Product::Hub => format!(
            "起反重力 Hub 之后 {ATTACH_WINDOW} 秒连不上它的调试端口，汉化引擎没有自动附加；可在反重力页手动点「附加」"
        ),
        Product::Ide => format!(
            "起反重力 IDE 之后 {ATTACH_WINDOW} 秒连不上它的调试端口，汉化引擎没有自动附加。IDE 的端口由面板启动时补上；若装了第三方汉化壳，可能是那层壳没把 --remote-debugging-port 传给里层"
        ),
    };
    crate::plugins::antigravity_ui::note_attach_error(&msg);
    crate::audit::write(&msg);
}

/// 关掉某个产品：面板托管的按会话停并交回租约，其余按安装目录核验后关。
pub async fn close(product: Product, gate: &GateState) -> Result<usize> {
    if product == Product::Hub {
        let _ = crate::plugins::antigravity_ui::stop();
    }
    crate::workspace::close_antigravity_for_launch(product, gate, |_| {}).await
}

pub fn set_auto_update(enabled: bool) -> Result<()> {
    let roaming = dirs::config_dir().unwrap_or_default();
    antigravity::set_auto_update(&roaming, enabled)?;
    crate::audit::write(&format!(
        "反重力 Hub 的自动检查更新已{}（app_storage.json 并入一个键，重启 Hub 生效）",
        if enabled { "打开" } else { "关闭" }
    ));
    Ok(())
}

// ------------------------------------------------------------------ 账户的 Gemini CLI 那一半

/// 起一个带窗口的 Gemini CLI 交互会话让使用者登录。**面板不碰凭据。**
///
/// `id` 是**账户槽位**的 id（0.32.0 起两半同属一条槽位）。那条槽位还没有 CLI
/// 那一半时现建一个目录 —— 「登录」这个动作本身就该把它准备好。
pub fn gemini_login(id: &str) -> Result<()> {
    let home = account::ensure_cli_dir(&account::root(), id)?;
    std::fs::create_dir_all(&home)?;
    let cli = crate::plugins::sillytavern::find_official_gemini()?;
    let env = vec![("GEMINI_CLI_HOME".to_string(), home.display().to_string())];
    let args = vec![cli.entry.display().to_string()];
    crate::sessions::OwnedProgram::launch_detached_console(&cli.node, &args, &home, env)?;
    // 点了「登录」就把上一次问出来的「登录已失效」清掉（`login_health` 文件头写着要这么做，
    // 原来只在移除槽位时清）—— 不然凭据文件没变的话，那一行一直红着、刷新也被挡着（2026-09-25）。
    super::login_health::forget(&super::login_health::gemini_cli_key(id));
    crate::audit::write(&format!(
        "已为 Gemini 槽位 {id} 打开 Gemini CLI 登录窗口（GEMINI_CLI_HOME 指向槽位目录）"
    ));
    Ok(())
}

/// `gemini_cli_install` 的段数。前端进度条按这个总数画。
pub const GEMINI_INSTALL_TOTAL: u32 = 3;

/// `npm install -g @google/gemini-cli`，输出流进软件页的进度条。面板不分发这个包。
///
/// # ⛔ 开关要裸着交给 `cmd`，不能加引号
///
/// 0.26.0–0.28.0 这里走的是 `sessions::launch_detached_console(cmd.exe, ["/k", "npm install …"])`。
/// [`crate::sessions`] 那条路会给**每一个**参数套引号（`quote_argument` 无条件加），
/// 于是 `CreateProcessW` 拿到的是：
///
/// ```text
/// "C:\Windows\System32\cmd.exe" "/k" "npm install -g @google/gemini-cli && …"
/// ```
///
/// `cmd.exe` 认不出加了引号的 `/k`，把后面那一整串当成一个命令名去找，报
/// `'"npm install …' is not recognized`；而 `/k` 又让那个窗口留在原地。
/// 症状就是使用者报的**「只弹出了命令窗口，没有安装」** —— npm 一次都没跑起来过，
/// 而面板这边照样返回 `Ok(())`、界面照样提示「已打开安装窗口」。
///
/// 现在走 [`winget::run_streaming`]：开关裸着、每个参数一个 argv、隐藏窗口、
/// **等它退出**、输出逐行进度条。跟 `install_ops` 里装 Codex CLI 那条是同一条路。
///
/// 另外两件 0.28.0 之前没做的：
///
/// - **装之前先查 node/npm 在不在**。原来只查 `cmd.exe` —— 那个一定在，所以没装 Node 的
///   机器上得到的是一个一闪而过的窗口，没有任何人说得出少了什么；
/// - **装完回读检测核对，不看退出码**（§7.21 那条教训）。
pub async fn gemini_cli_install(rep: &dyn crate::sink::ProgressSink) -> Result<String> {
    use qb_install::install::{detect, winget};

    rep.phase(1, "检查 Node.js 与 npm");
    let node = detect::node_exe().ok_or_else(|| {
        GateError::Other(
            "没找到 Node.js（node.exe 不在 PATH 上，也不在 %ProgramFiles%\\nodejs）。\
             Gemini CLI 是一个 npm 包，先装 Node.js LTS（https://nodejs.org/）再回来点这里。"
                .into(),
        )
    })?;
    rep.log(1, &format!("node：{}", node.display()));

    rep.phase(2, "npm install -g @google/gemini-cli");
    let mut log: Vec<String> = Vec::new();
    // npm 在 Windows 上是 npm.cmd，得走 cmd /c。开关与每个参数各占一个 argv。
    let args: Vec<String> = ["/c", "npm", "install", "-g", "@google/gemini-cli"]
        .into_iter()
        .map(String::from)
        .collect();
    let ok = winget::run_streaming("cmd", &args, rep, 2, &mut log).await?;

    // 退出码只是线索，装没装成看盘。
    rep.phase(3, "回读检测核对");
    let found = detect::gemini_cli();
    if !found.installed {
        let tail = log
            .iter()
            .rev()
            .take(3)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join(" / ");
        // ⛔ 报错里必须带上**找过哪些路径**。
        //
        // 0.29.0–0.30.0 这句话只有前半截，而当时入口写死成 `dist\index.js`、官方包的
        // 入口却是 `bundle\gemini.js`：npm 每次都真的装成功，检测每次都落空，
        // 使用者连着两天看到的都是同一句「还是装不上」，而没有任何线索指向位置表
        //（审计日志 2026-09-21 02:32 / 18:16）。位置对不上是这条链最常见的失效，
        // 说出来才查得动。
        return Err(GateError::Other(format!(
            "npm {}，而检测仍然找不到 Gemini CLI 的入口文件。{}。\
             npm 最后几行：{}",
            if ok { "报告成功" } else { "没有成功" },
            detect::gemini_cli_searched(),
            if tail.is_empty() {
                "（没有输出）"
            } else {
                &tail
            }
        )));
    }
    let detail = format!(
        "Gemini CLI {} 已装好（npm 全局）。",
        found.version.clone().unwrap_or_else(|| "版本读不出".into())
    );
    crate::audit::write(&detail);
    Ok(detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn products_map_onto_their_own_clients() {
        assert_eq!(client_of(Product::Hub), Client::Antigravity);
        assert_eq!(client_of(Product::Ide), Client::AntigravityIde);
    }

    fn bucket(
        day: &str,
        model: &str,
        input: i64,
        read: i64,
        out: i64,
        n: i64,
    ) -> qb_accounts::accounts::tokens::TokenBucket {
        qb_accounts::accounts::tokens::TokenBucket {
            day: day.into(),
            model: model.into(),
            input,
            output: out,
            cache_write: 0,
            cache_write_1h: 0,
            cache_read: read,
            messages: n,
        }
    }

    /// 时间档筛过之后再按模型合计；排序按「读过的 token 多的在前」；覆盖证据原样带出。
    #[test]
    fn the_per_model_breakdown_respects_the_day_window_and_keeps_the_evidence_counts() {
        let scan = ag_usage::AntigravityUsageScan {
            buckets: vec![
                bucket("2026-09-21", "gemini-3.7-flash", 100, 5_000, 10, 3),
                bucket("2026-09-21", "claude-opus-4-6-thinking", 900, 0, 50, 1),
                bucket("2026-09-01", "gemini-3.7-flash", 999_999, 0, 1, 1),
            ],
            files_read: 2,
            files_failed: 1,
            legacy_skipped: 16,
            incomplete: 0,
            duplicates: 0,
            scanned_dirs: vec!["x".into()],
        };
        let u = summarize_scan(scan, 1, "2026-09-21", &pricing::Catalog::new(Vec::new()));
        assert_eq!(u.summary.input, 1000, "9-01 那桶不在「今天」里");
        assert_eq!(u.summary.messages, 4);
        assert_eq!(u.models.len(), 2);
        assert_eq!(
            u.models[0].model, "gemini-3.7-flash",
            "读得多的在前（输入 + 缓存读）"
        );
        assert_eq!(u.models[0].cache_read, 5_000);
        assert_eq!(u.models[1].input, 900);
        assert_eq!((u.files_read, u.files_failed, u.legacy_skipped), (2, 1, 16));
        assert_eq!(u.scanned_dirs, vec!["x"]);
    }
}
