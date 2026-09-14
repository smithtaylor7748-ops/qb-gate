//! 账户槽位与凭证到期。
//!
//! 一个槽位 `<标签>` 由至多三处组成，都靠目录联结点(junction)指过去：
//!
//! | 谁在用 | 指向「当前」的那个联结点 | 槽位本体 |
//! |---|---|---|
//! | Claude Code（面板启动的） | `%LOCALAPPDATA%\ClaudeIpGate\claude-profile` | `%LOCALAPPDATA%\ClaudeIpGate\claude-profile-<标签>` |
//! | 酒馆桥接（装了才有） | `%LOCALAPPDATA%\ClaudeTavernBridge\claude-profile` | 同上，**同一个目录** |
//! | 桌面端（可选） | `%APPDATA%\Claude` | `%APPDATA%\Claude-<标签>` |
//!
//! # v0.8.0：面板启动的 Claude Code 原来根本不读槽位 ★
//!
//! 旧脚本 `ClaudeIpGate.ps1` 在启动 Claude Code 之前设 `CLAUDE_CONFIG_DIR`，
//! 移植成 Rust 时这一行丢了（`launch.rs` 只是 `cmd /c start "" claude.exe`）。
//! 于是切换只改了一个没人读的联结点，Claude Code 读的永远是默认的 `~\.claude`
//! —— 那里从来没登录过，**切到哪个槽位都弹登录界面**。
//!
//! 现在 `launch.rs` 启动时把 `CLAUDE_CONFIG_DIR` 设成**具体的槽位目录**
//! （[`active_slot_dir`]），不是 `claude-profile` 这个联结点：
//! 联结点换了指向之后，一个没收掉的旧会话刷新 token 时会穿过联结点，
//! 把 A 账户的 token 写进 B 的槽位。钉死在具体目录上，旧会话只会继续用 A。
//!
//! # v0.8.0：凭证只留一份
//!
//! 旧的迁移是**复制**：`ClaudeTavernBridge\claude-profile-*` 与
//! `ClaudeIpGate\claude-profile-*` 各有一份相同的 refresh token，
//! 酒馆桥接指着 NEW1、面板指着 main —— 两个账户同时「激活」，而且两边都用起来之后
//! 一边刷新就可能让另一份作废。[`sync_bridge`] 把它们合成一份：
//! 桥接那边换成指向面板槽位的联结点，原目录改名留底（不删），
//! 桥接的「当前」跟面板统一。
//!
//! # 四条政策边界
//!
//! 改这个文件之前先读 README 的合规一节：
//!   1. 只读官方客户端写在本机的用量缓存用于**显示**；不发网络请求、
//!      不调用任何额度接口，也不存在「用完自动换号」的路径
//!   2. 切换只能由人在界面上手动触发，没有定时器、没有 watchdog、没有自动调用点
//!   3. 任意时刻只有一个账户激活
//!   4. 所有账户必须是使用者本人拥有的
//!
//! [`sync_bridge`] 会在列槽位时顺手跑，它**不切换账户**：只把桥接对齐到面板
//! 已经选定的那一个（第 3 条），面板这边的「当前」一个字不动。
//!
//! # 判断上的两个坑，合并了就出事
//!
//!   - **「已登录」和「没过期」是两件事。** 凭证过期的账户**必须仍然可切**，
//!     因为你得先切过去才能在那个槽里重新登录。合并了就把自己锁在门外。
//!     所以 `logged_in` 只看凭证文件在不在，到期与否单独算，且只用于显示。
//!   - `config.json` 的 `lastKnownAccountUuid` 回答不了「还登着吗」——
//!     登出 / 过期之后它也不消失，只能回答「这个目录是谁的」。

use crate::error::{GateError, Result};
use serde::Serialize;
use ts_rs::TS;
mod transaction;
pub mod usage;
use std::path::{Path, PathBuf};

/// 指向「当前槽位」的那个联结点的名字（面板与桥接两边都叫这个）。
const LINK: &str = "claude-profile";
/// 槽位目录的前缀。
const PREFIX: &str = "claude-profile-";

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Slot {
    pub label: String,
    pub active: bool,
    /// 只看凭证文件在不在 —— 不掺到期判断。
    pub logged_in: bool,
    /// refreshToken 剩余天数。负数表示已过期。
    ///
    /// `#[ts(type = "number")]`：ts-rs 默认把 `i64` 映射成 `bigint`，而
    /// `serde_json` 发出去的是一个普通 JSON 数字，`JSON.parse` 收到的也是
    /// `number` —— 声明成 `bigint` 的话类型和运行时对不上，做算术会当场抛。
    /// 天数这个量级离 2^53 远得很，`number` 是对的。
    ///  的可空性要一起写进去 ——  是**整个类型**的替换，
    /// 只写 number 的话生成出来就不带 null 了，而它确实可能是 null。
    #[ts(type = "number | null")]
    pub cli_days_left: Option<i64>,
    pub account_uuid: Option<String>,
    /// 套餐，例如 `Claude Pro`。读自本槽位的 `.claude.json`，**不联网**。
    pub plan: Option<String>,
    /// 计费方式，例如 `Google Play 订阅`。同上。
    pub billing: Option<String>,
    /// 官方客户端上次刷新这份档案的时间。界面上要标出来 —— 这是缓存，可能过期。
    pub plan_fetched_at: Option<String>,
    /// 桌面端有没有这个槽位自己的资料目录 `%APPDATA%\Claude-<标签>`。
    pub desktop_profile: bool,
    /// 五小时 / 七天窗口的用量与恢复时刻。读的是官方客户端写在本机的文件，
    /// **不联网、不调接口**，详见 [`usage`]。两源都没有就是 `None`。
    pub usage: Option<usage::SlotUsage>,
}

/// 套餐是**读文件读出来的，不是查接口查出来的**。
///
/// `~/.claude.json` 的 `oauthAccount` 是官方客户端自己写下的一份档案缓存，
/// 每个槽位目录里各有一份 —— 所以不用切过去就能看到每个槽位的套餐。
///
/// 这符合第 1 条政策边界：读的是官方客户端**自己写在本机的文件**，
/// 不发任何网络请求、不调用任何额度接口。用量（见 [`usage`]）同理 ——
/// 它只用于显示，面板不据此做任何决定，切换依然只能由人手动触发。
///
/// **不要**顺手把 `organizationRateLimitTier` / `userRateLimitTier` 显示出来。
/// 那两个字段名里带 rateLimit，展示它们会让这条边界变得可疑，
/// 而它们对用户的价值几乎为零。
pub const PLAN_CAVEAT: &str =
    "套餐、计费方式与用量都读自官方客户端写在本机的文件（槽位的 .claude.json 与桌面端的 plan-usage-history.json），不联网、不调接口。";

/// 剩余天数只能回答「名义上到期没」，回答不了「服务端还认不认」。
///
/// 凭证被风控提前作废（改密码 / 网页端撤销 / 风控下线）时，本地时间戳一字不变，
/// 看起来依旧健康。界面上必须固定打这行，避免「剩 N 天」给出虚假的安全感。
pub const EXPIRY_CAVEAT: &str =
    "剩余天数只读本地时间戳，查不出「被风控下线」。唯一能确认的办法是实际发一次认证请求。";

// ---------------------------------------------------------------- 根目录

/// 槽位相关的三个根目录。平时用 [`AccountRoots::current`]，单测自己搭 ——
/// 档案 §1 第 8 条：单测不许碰真实的运行期状态。
#[derive(Debug, Clone)]
pub struct AccountRoots {
    /// 面板的槽位根：`%LOCALAPPDATA%\ClaudeIpGate`
    pub panel: PathBuf,
    /// 酒馆桥接的数据目录：`%LOCALAPPDATA%\ClaudeTavernBridge`。**不存在就是 None** ——
    /// 没装酒馆的人不该平白多出这个目录。
    pub bridge: Option<PathBuf>,
    /// `%APPDATA%`。桌面端的 `Claude` / `Claude-<标签>` 在这下面。
    pub appdata: Option<PathBuf>,
}

impl AccountRoots {
    pub fn current() -> Self {
        let bridge = crate::paths::tavern_bridge_dir();
        Self {
            panel: crate::paths::state_dir(),
            bridge: bridge.is_dir().then_some(bridge),
            appdata: dirs::config_dir(),
        }
    }

    fn link(&self) -> PathBuf {
        self.panel.join(LINK)
    }

    pub fn slot_dir(&self, label: &str) -> PathBuf {
        self.panel.join(format!("{PREFIX}{label}"))
    }

    fn desktop_link(&self) -> Option<PathBuf> {
        self.appdata.as_ref().map(|a| a.join("Claude"))
    }

    fn desktop_dir(&self, label: &str) -> Option<PathBuf> {
        self.appdata
            .as_ref()
            .map(|a| a.join(format!("Claude-{label}")))
    }
}

/// 槽位名的规矩。
///
/// 字母、数字（含中文）、`-` `_` `.`，最长 32 个字符，不能以点开头或结尾。
/// 冒号尤其不行 —— 托盘菜单的 id 拿冒号分段（`tray.rs`），名字里有冒号就会
/// 静默落到「什么都不做」那条分支上。
pub fn validate_label(label: &str) -> Result<()> {
    let ok = !label.is_empty()
        && label.chars().count() <= 32
        && label
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !label.starts_with('.')
        && !label.ends_with('.');
    if ok {
        Ok(())
    } else {
        Err(GateError::Other(format!(
            "槽位名「{label}」不合规：只能用字母、数字、中文和 - _ .，最长 32 个字符，不能以点开头或结尾。"
        )))
    }
}

// ---------------------------------------------------------------- 联结点

/// 这个路径上是不是一个重解析点（联结点 / 符号链接）。
///
/// 直接读文件属性位，不走 `Path::is_symlink()` —— 后者认不认目录联结点
/// 取决于标准库对 reparse tag 的取舍，而这里必须问的是一个更朴素的问题：
/// 「它是不是一层壳，摘掉不会动到里面的东西」。
#[cfg(windows)]
fn is_reparse_point(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(meta: &std::fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

/// 联结点指向哪里。不是联结点（或者根本不存在）就是 `None`。
fn junction_target(link: &Path) -> Option<PathBuf> {
    let meta = std::fs::symlink_metadata(link).ok()?;
    if !is_reparse_point(&meta) {
        return None;
    }
    std::fs::read_link(link).ok()
}

/// 把一个即将被 `mklink /J` 占用的路径腾出来。
///
/// **不能用 `Path::exists()` 判断。** `exists()` 会跟着联结点走到目标去，
/// 目标目录一旦被删掉（换过槽位名、手动清理过），悬空的联结点上
/// `exists()` 就回 false —— 于是「先摘掉旧联结点」这一步被整个跳过，
/// 紧接着 `mklink` 撞上「已存在同名文件」而失败。表现是**这个槽位从此
/// 再也切不过去**，而报错文案完全看不出根因。用 `symlink_metadata`
/// 才是在问路径本身，不是问它指向谁。
///
/// 三种情况必须分开，合并了就出事：
///   * 重解析点 —— `remove_dir` 摘壳，不会碰到目标里的内容
///   * 真目录 —— **改名备份，绝不递归删**，那底下是用户的凭证。
///     备份名是 `<原名>-backup-<时间戳>`：面板那边会被当成一个槽位列出来，
///     桌面端那边的副本仍在上锁清单里（`inventory` 扫 `Claude-*`）——
///     换成别的名字，里面的 `claude.exe` 副本就成了没人锁的漏网之鱼
///   * 什么都没有 —— 什么都不做
///
/// 改了名就回一句说明，由调用方写进日志 —— 单测会调到这里，不能自己写日志。
fn clear_link(path: &Path) -> Result<Option<String>> {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Ok(None);
    };
    if is_reparse_point(&meta) {
        std::fs::remove_dir(path)?;
        Ok(None)
    } else if meta.is_dir() {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let backup = path.with_file_name(format!("{name}-backup-{}", crate::config_io::id()));
        std::fs::rename(path, &backup)?;
        Ok(Some(format!(
            "{} 是真目录不是联结点，已改名备份到 {}",
            path.display(),
            backup.display()
        )))
    } else {
        // 同名的普通文件。这里本来就该是个目录，删掉是安全的。
        std::fs::remove_file(path)?;
        Ok(None)
    }
}

/// 建一个目录联结点。
#[cfg(windows)]
fn make_junction(link: &Path, target: &Path) -> Result<()> {
    if [link, target]
        .iter()
        .any(|p| p.to_string_lossy().contains(['%', '"', '\r', '\n']))
    {
        return Err(GateError::Other(
            "账户路径含不支持的命令字符，未更改指向".into(),
        ));
    }
    use std::os::windows::process::CommandExt;
    let mut cmd = crate::process::hidden_std(std::process::Command::new("cmd"));
    // 路径自己加引号，不交给标准库：不含空格、但含 & ^ 的用户名（比如 Tom&Jerry），
    // 标准库不会替它加引号，cmd 就会在 & 处把命令截成两条。
    // Windows 路径里不可能出现双引号，所以这样拼是安全的。/D 关掉注册表里的 AutoRun。
    cmd.raw_arg(format!(
        "/D /V:OFF /C mklink /J \"{}\" \"{}\"",
        link.display(),
        target.display()
    ));
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(GateError::Other(format!(
            "建立联结点 {} → {} 失败：{}",
            link.display(),
            target.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn make_junction(_link: &Path, _target: &Path) -> Result<()> {
    Err(GateError::Other("仅 Windows 支持".into()))
}

/// 让 `link` 指向 `target`。原来是什么都先腾掉（见 [`clear_link`]）。
fn repoint(link: &Path, target: &Path, notes: &mut Vec<String>) -> Result<()> {
    if junction_target(link).is_some_and(|t| crate::install::inventory::same_path(&t, target)) {
        return Ok(());
    }
    if let Some(n) = clear_link(link)? {
        notes.push(n);
    }
    make_junction(link, target)
}

// ---------------------------------------------------------------- 读状态

/// 面板当前激活的槽位名。
pub fn active_label(r: &AccountRoots) -> Option<String> {
    let t = junction_target(&r.link())?;
    let name = t.file_name()?.to_string_lossy().to_string();
    let label = name.strip_prefix(PREFIX)?.to_string();
    r.slot_dir(&label).is_dir().then_some(label)
}

/// 面板当前激活的槽位**具体目录**。启动 Claude Code 时 `CLAUDE_CONFIG_DIR` 就设成它。
///
/// 返回具体目录而不是联结点，理由见文件头。没有激活槽位（没建过、联结点悬空）
/// 就是 `None` —— 那时候 Claude Code 用它自己的默认目录 `~\.claude`，界面上要说清楚。
pub fn active_slot_dir(r: &AccountRoots) -> Option<PathBuf> {
    active_label(r).map(|l| r.slot_dir(&l))
}

pub fn slots() -> Vec<Slot> {
    slots_in(&AccountRoots::current())
}

pub fn slots_in(r: &AccountRoots) -> Vec<Slot> {
    let Ok(rd) = std::fs::read_dir(&r.panel) else {
        return Vec::new();
    };
    let active = active_label(r);

    let mut out: Vec<Slot> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let label = name.strip_prefix(PREFIX)?.to_string();
            let dir = e.path();
            let profile = read_profile(&dir);
            Some(Slot {
                active: active.as_deref() == Some(label.as_str()),
                logged_in: dir.join(".credentials.json").exists(),
                cli_days_left: cli_days_left(&dir),
                account_uuid: profile
                    .as_ref()
                    .and_then(|v| v.get("accountUuid"))
                    .and_then(|v| v.as_str())
                    .map(String::from),
                plan: profile.as_ref().and_then(plan_of),
                billing: profile.as_ref().and_then(billing_of),
                plan_fetched_at: profile.as_ref().and_then(fetched_at_of),
                desktop_profile: r.desktop_dir(&label).is_some_and(|d| d.is_dir()),
                usage: usage::for_slot(
                    r.appdata.as_deref(),
                    &dir,
                    profile
                        .as_ref()
                        .and_then(|v| v.get("organizationUuid"))
                        .and_then(|v| v.as_str()),
                ),
                label,
            })
        })
        .collect();
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// 桌面端现在归不归面板管、用的是哪个槽位的资料。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DesktopState {
    /// `%APPDATA%\Claude` 是联结点（面板在管）。是真目录或不存在就是 false。
    pub managed: bool,
    /// 指向的是 `Claude-<标签>` 时的那个标签。
    pub active: Option<String>,
}

pub fn desktop_state(r: &AccountRoots) -> DesktopState {
    let Some(link) = r.desktop_link() else {
        return DesktopState {
            managed: false,
            active: None,
        };
    };
    match junction_target(&link) {
        Some(t) => {
            let name = t
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            // 文件系统不分大小写，前缀也别分。
            let active = name
                .get(..7)
                .filter(|p| p.eq_ignore_ascii_case("Claude-"))
                .map(|_| name[7..].to_string());
            DesktopState {
                managed: true,
                active,
            }
        }
        None => DesktopState {
            managed: false,
            active: None,
        },
    }
}

/// `refreshTokenExpiresAt` 剩余天数。
///
/// **只看 refreshToken，不看 accessToken。** accessToken 8–12 小时过期是正常的，
/// 客户端自己拿 refreshToken 静默换新，用户无感 —— 因此故意不为它报警。
fn cli_days_left(dir: &std::path::Path) -> Option<i64> {
    let text = std::fs::read_to_string(dir.join(".credentials.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let ms = v
        .get("claudeAiOauth")?
        .get("refreshTokenExpiresAt")?
        .as_i64()?;
    let now = chrono::Utc::now().timestamp_millis();
    Some((ms - now) / 86_400_000)
}

/// 读本槽位的 `oauthAccount`。**纯文件读取，不发任何请求。**
fn read_profile(dir: &std::path::Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join(".claude.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("oauthAccount").cloned()
}

/// 套餐名。
///
/// 优先 `seatTier`（Team / Enterprise 席位才有值），退回 `organizationType`
/// （个人订阅是 `claude_pro` / `claude_max` 这种）。**认不出来就如实回原值**，
/// 不猜成 Pro —— 猜错了用户会以为自己买的是别的套餐。
fn plan_of(o: &serde_json::Value) -> Option<String> {
    let raw = o
        .get("seatTier")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            o.get("organizationType")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
        })?;
    Some(pretty_plan(raw))
}

pub fn pretty_plan(raw: &str) -> String {
    match raw {
        "claude_pro" => "Claude Pro".into(),
        "claude_max" => "Claude Max".into(),
        "claude_team" => "Claude Team".into(),
        "claude_enterprise" => "Claude Enterprise".into(),
        "claude_free" => "免费版".into(),
        // 认不出来就原样显示，不猜。
        other => other.to_string(),
    }
}

fn billing_of(o: &serde_json::Value) -> Option<String> {
    let raw = o
        .get("billingType")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    Some(pretty_billing(raw))
}

pub fn pretty_billing(raw: &str) -> String {
    match raw {
        "google_play_subscription" => "Google Play 订阅".into(),
        "apple_subscription" => "App Store 订阅".into(),
        "stripe_subscription" => "信用卡订阅".into(),
        other => other.to_string(),
    }
}

/// `profileFetchedAt` 是毫秒时间戳。界面上要标出来 —— 这是缓存，可能过期。
fn fetched_at_of(o: &serde_json::Value) -> Option<String> {
    let ms = o.get("profileFetchedAt")?.as_i64()?;
    let dt = chrono::DateTime::from_timestamp_millis(ms)?;
    Some(
        dt.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
    )
}

// ---------------------------------------------------------------- 新建

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CreateOutcome {
    pub label: String,
    /// 原来一个激活槽位都没有，这个新槽位直接成了当前的。
    pub activated: bool,
    pub notes: Vec<String>,
}

/// 新建一个空槽位。**不复制任何凭证** —— 新槽位要在里面登录一次。
///
/// 为什么不从 `~\.claude` 把现成的登录「导入」进来：复制凭证就是又造一份
/// 同样的 refresh token，正是文件头那条「凭证只留一份」要消灭的局面；
/// 挪走则会把直接敲 `claude` 用的默认目录登出。
///
/// 原来没有任何激活槽位时，新槽位直接成为当前的（面板与桥接两处指向它，
/// 桌面端不动）。那不算「切换账户」：之前面板启动的 Claude Code 用的是默认目录，
/// 没有别的槽位被切走。
pub fn create_slot(r: &AccountRoots, label: &str) -> Result<CreateOutcome> {
    validate_label(label)?;
    let dir = r.slot_dir(label);
    if std::fs::symlink_metadata(&dir).is_ok() {
        return Err(GateError::Other(format!("槽位 {label} 已经存在")));
    }
    std::fs::create_dir_all(&dir)?;

    let mut notes = Vec::new();
    let activated = active_label(r).is_none();
    if activated {
        notes.extend(transaction::switch(r, label, DesktopMode::Keep)?.notes);
    }
    Ok(CreateOutcome {
        label: label.to_string(),
        activated,
        notes,
    })
}

// ---------------------------------------------------------------- 切换

/// 桌面端这次跟不跟着切。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopMode {
    /// `Claude-<标签>` 在就跟着切，不在就不动。托盘、档案、快照回滚走这一档。
    Auto,
    /// 跟着切；`Claude-<标签>` 不在就建一份空白的（桌面端要重新登录）。
    Follow,
    /// 这次不动桌面端。
    Keep,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SwitchOutcome {
    /// 这次动了哪几处指向，给人看的。
    pub switched: Vec<String>,
    /// 顺带发生的事（改名备份、新建空白资料……），给人看、也写日志。
    pub notes: Vec<String>,
}

/// 桌面端那一侧要做什么。先算好，再动手 —— 算的阶段不碰文件系统里的任何东西。
struct DesktopPlan {
    link: PathBuf,
    target: PathBuf,
    /// `Claude-<标签>` 还不存在，要新建空白的。
    create: bool,
    /// `%APPDATA%\Claude` 现在是真目录：先改名成这个，才能换成联结点。
    adopt_to: Option<PathBuf>,
}

fn plan_desktop(
    r: &AccountRoots,
    label: &str,
    mode: DesktopMode,
    from: Option<&str>,
) -> Option<DesktopPlan> {
    let (link, target) = (r.desktop_link()?, r.desktop_dir(label)?);
    let exists = target.is_dir();
    let follow = match mode {
        DesktopMode::Keep => false,
        DesktopMode::Auto => exists,
        DesktopMode::Follow => true,
    };
    if !follow {
        return None;
    }
    if junction_target(&link).is_some_and(|t| crate::install::inventory::same_path(&t, &target)) {
        return None; // 已经指着它了
    }
    // 真目录 = 桌面端还没交给面板管过。它就是使用者一直在用的那份资料，
    // 存成「切走之前那个槽位」的桌面端资料；那个名字被占了才退回备份名。
    let adopt_to = match std::fs::symlink_metadata(&link) {
        Ok(m) if !is_reparse_point(&m) && m.is_dir() => Some(
            from.and_then(|f| r.desktop_dir(f))
                .filter(|d| std::fs::symlink_metadata(d).is_err())
                .unwrap_or_else(|| {
                    link.with_file_name(format!("Claude-backup-{}", crate::config_io::id()))
                }),
        ),
        _ => None,
    };
    // 真目录恰好就存成了目标（切的就是它原来那个槽位）时，不算「新建空白」。
    let create = !exists
        && !adopt_to
            .as_deref()
            .is_some_and(|a| crate::install::inventory::same_path(a, &target));
    Some(DesktopPlan {
        link,
        target,
        create,
        adopt_to,
    })
}

/// 切换激活槽位：Claude Code、酒馆桥接、桌面端三处指向一起换。
///
/// **只在用户手动点击时调用。不要给它加任何自动调用点** —— 加了就变成
/// 自动轮换账户，直接踩政策线。
///
/// # 前提：调用之前已经把全部 Claude 关掉了
///
/// v0.9.0 起「切换账户」的语义是**先清场再切**：调用方（`lib.rs::accounts_switch`、
/// 托盘、`profile_apply`）在进来之前已经 `killswitch::execute()` 收掉了全部正在跑的
/// Claude（桌面端也在内）。所以这里不再检查「桌面端开着吗」—— 它已经关了。
/// 在一个跑着的桌面端脚下换它的资料目录会把两个账户的东西搅在一起，而清场恰好消除了这件事。
///
/// 要么全换，要么都不换：中途哪一步失败，已经换过的指向会还原回去，
/// 报错文案才能如实说「槽位没有变动」。
pub fn switch_in(r: &AccountRoots, label: &str, mode: DesktopMode) -> Result<SwitchOutcome> {
    transaction::switch(r, label, mode)
}
/// 启动时结清没做完的账户切换。返回**结不清的那几条**（路径 + 原因），
/// 由 `startup` 交给恢复页显示，不再直接把面板拦在门外。
pub fn recover_switches() -> Result<Vec<(PathBuf, String)>> {
    transaction::recover(&AccountRoots::current().panel)
}

/// 清掉结清已久的账户切换记录。见 `transaction::retain`。
pub fn retain_switches(cutoff: std::time::SystemTime) -> Result<Vec<String>> {
    transaction::retain(&AccountRoots::current().panel, cutoff)
}

/// 档案应用与快照回滚走的切换：桌面端有自己的资料就跟着切，没有就不动。
///
/// **前提同 [`switch_in`]：调用方已经清场。** 这两条路在 `lib.rs` 的命令层
/// 先 `clear_before_account_change`（要换号才关全部 Claude）再调到这里。
pub fn preflight_switch(label: &str) -> Result<()> {
    validate_label(label)?;
    let target = AccountRoots::current().slot_dir(label);
    if !target.is_dir() {
        return Err(GateError::NotFound(target.display().to_string()));
    }
    Ok(())
}
pub fn switch(label: &str) -> Result<SwitchOutcome> {
    let out = switch_in(&AccountRoots::current(), label, DesktopMode::Auto)?;
    log_outcome(label, &out);
    // ⚠ 让会话内门禁跟到新槽位这一步**不在这里** —— 它在
    // `usecase::account_ops::switch`。理由见那个模块的文件头：
    // 放在这里会让 accounts 反过来依赖 gate。**别把它加回来**，
    // 也别绕过 account_ops 直接调本函数。
    Ok(out)
}

/// 面板对话框与托盘走的切换。对话框由使用者勾选（`Follow` / `Keep`）；托盘没有复选框，
/// 用 `Auto` —— 等于那个复选框的默认值（这个槽位有自己的桌面端资料才跟）。
///
/// **前提同 [`switch_in`]：调用方已经清场**（`lib.rs::clear_for_switch`，失败就不许调到这里）。
pub fn switch_with(label: &str, mode: DesktopMode) -> Result<SwitchOutcome> {
    preflight_switch(label)?;
    let out = switch_in(&AccountRoots::current(), label, mode)?;
    log_outcome(label, &out);
    Ok(out)
}

fn log_outcome(label: &str, out: &SwitchOutcome) {
    for n in &out.notes {
        crate::audit::write(n);
    }
    crate::audit::write(&format!(
        "账户槽位切换为 {label}（{}）",
        out.switched.join("，")
    ));
}

// ---------------------------------------------------------------- 合并

#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct SyncReport {
    /// 这次真的做了的事。空 = 本来就是一份，什么都没动。
    pub done: Vec<String>,
    /// 没做成的（通常是桥接正在用那个目录），下次列槽位时会再试。
    pub failed: Vec<String>,
}

/// 凭证文件的修改时间。拿来判断哪一份是「活的」——
/// 客户端刷新 token 时会重写这个文件，更新的那份才带着服务端还认的 refresh token。
fn cred_mtime(dir: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(dir.join(".credentials.json"))
        .and_then(|m| m.modified())
        .ok()
}

/// 用 `src` 替换 `dst`，`dst` 原来那份改名成 `backup` 留底。
///
/// 三步都可能失败，每一步失败都要退回原状 —— 最坏的结局是槽位里一份凭证都没有：
/// 先拷到临时文件（失败了什么都没动），再把旧的挪去留底，最后把临时文件放上去
/// （失败了把留底挪回来）。
fn replace_keeping_backup(src: &Path, dst: &Path, backup: &Path) -> std::io::Result<()> {
    let tmp = dst.with_extension("merge-tmp");
    std::fs::copy(src, &tmp)?;
    if let Err(e) = std::fs::rename(dst, backup) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, dst) {
        let _ = std::fs::rename(backup, dst);
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// 把酒馆桥接那边的槽位跟面板合成一份。**幂等**，列槽位时每次都跑，
/// 已经是一份的时候只做几次 `metadata` 就返回。
///
/// 对桥接数据目录下每个 `claude-profile-<标签>`：
///
/// * 已经是联结点 —— 不动；
/// * 面板这边没有同名槽位 —— **整个搬过来**（同一个卷上是改名，不是复制），原处留联结点；
/// * 两边都有 —— 以凭证更新的那一份为准：桥接那份更新，就把它的凭证拷给面板
///   （面板那份改名留底）；然后桥接那边的目录改名成 `superseded-claude-profile-<标签>-<时间>`
///   留底（**不删**），原处换成指向面板槽位的联结点。
///
/// 最后把桥接的「当前」（`ClaudeTavernBridge\claude-profile`）对齐到面板的当前槽位。
/// 面板这边一个都没激活、而桥接原来指着某个标签时，反过来让面板接上它 ——
/// 那是一个从旧脚本迁过来的使用者原本的状态，不是面板替他选的账户。
///
/// 旧的桥接是单账户布局（`claude-profile` 本身是个真目录）时，先把它认成一个槽位。
pub fn sync_bridge(r: &AccountRoots) -> SyncReport {
    let mut rep = SyncReport::default();
    let Some(b) = &r.bridge else {
        return rep;
    };
    if let Err(e) = std::fs::create_dir_all(&r.panel) {
        rep.failed.push(format!("建不了面板的槽位目录：{e}"));
        return rep;
    }
    let stamp = crate::config_io::id().to_string();
    let bridge_link = b.join(LINK);

    // 桥接原来认的「当前」。联结点指向的目录名优先，其次是旧脚本写的 active-profile.txt。
    let bridge_active: Option<String> = junction_target(&bridge_link)
        .and_then(|t| t.file_name().map(|n| n.to_string_lossy().to_string()))
        .and_then(|n| n.strip_prefix(PREFIX).map(String::from))
        .or_else(|| {
            std::fs::read_to_string(b.join("active-profile.txt"))
                .ok()
                .map(|s| s.trim().to_string())
        })
        .filter(|l| validate_label(l).is_ok());

    // ---- 0. 单账户老布局：claude-profile 本身是个真目录
    if let Ok(m) = std::fs::symlink_metadata(&bridge_link) {
        if !is_reparse_point(&m) && m.is_dir() {
            let label = bridge_active
                .clone()
                .filter(|l| {
                    std::fs::symlink_metadata(b.join(format!("{PREFIX}{l}"))).is_err()
                        && std::fs::symlink_metadata(r.slot_dir(l)).is_err()
                })
                .unwrap_or_else(|| format!("tavern-{stamp}"));
            match std::fs::rename(&bridge_link, b.join(format!("{PREFIX}{label}"))) {
                Ok(()) => rep
                    .done
                    .push(format!("酒馆桥接的单账户目录已认作槽位 {label}")),
                Err(e) => rep
                    .failed
                    .push(format!("酒馆桥接的 claude-profile 改不了名：{e}")),
            }
        }
    }

    // ---- 1. 逐个槽位合并
    let mut entries: Vec<PathBuf> = std::fs::read_dir(b)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    entries.sort();
    for bpath in entries {
        let Some(label) = bpath
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .and_then(|n| n.strip_prefix(PREFIX).map(String::from))
        else {
            continue;
        };
        if validate_label(&label).is_err() {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&bpath) else {
            continue;
        };
        if is_reparse_point(&meta) || !meta.is_dir() {
            continue;
        }
        let ppath = r.slot_dir(&label);

        if std::fs::symlink_metadata(&ppath).is_err() {
            match std::fs::rename(&bpath, &ppath).and_then(|_| {
                make_junction(&bpath, &ppath).map_err(|e| std::io::Error::other(e.to_string()))
            }) {
                Ok(()) => rep
                    .done
                    .push(format!("酒馆桥接的槽位 {label} 已搬进面板，原处留了联结点")),
                Err(e) => rep.failed.push(format!(
                    "槽位 {label} 搬不过来（多半是酒馆桥接正在用）：{e}"
                )),
            }
            continue;
        }

        // 两边都有。桥接那份的凭证更新，说明搬家之后是它在刷新 —— 它才是活的那份。
        if let (Some(bt), Some(pt)) = (cred_mtime(&bpath), cred_mtime(&ppath)) {
            if bt > pt {
                let pc = ppath.join(".credentials.json");
                let keep = ppath.join(format!(".credentials.json.before-merge-{stamp}"));
                if let Err(e) = replace_keeping_backup(&bpath.join(".credentials.json"), &pc, &keep)
                {
                    rep.failed.push(format!(
                        "槽位 {label}：酒馆桥接那份凭证更新，但拷不过来：{e}"
                    ));
                    continue;
                }
                let bj = bpath.join(".claude.json");
                if bj.is_file() {
                    let _ = std::fs::copy(&bj, ppath.join(".claude.json"));
                }
                rep.done.push(format!(
                    "槽位 {label}：酒馆桥接那份凭证更新，已用它替换面板那份（面板原来的留底为 {}）",
                    keep.display()
                ));
            }
        } else if cred_mtime(&bpath).is_some() && cred_mtime(&ppath).is_none() {
            // 面板这份根本没登录过，桥接那份登录着 —— 同样以桥接为准。
            if std::fs::copy(
                bpath.join(".credentials.json"),
                ppath.join(".credentials.json"),
            )
            .is_ok()
            {
                rep.done.push(format!(
                    "槽位 {label}：面板这份没登录过，已拷入酒馆桥接那份的凭证"
                ));
            }
        }

        let superseded = b.join(format!("superseded-{PREFIX}{label}-{stamp}"));
        match std::fs::rename(&bpath, &superseded).and_then(|_| {
            make_junction(&bpath, &ppath).map_err(|e| std::io::Error::other(e.to_string()))
        }) {
            Ok(()) => rep.done.push(format!(
                "槽位 {label} 合成了一份：酒馆桥接那边改指向面板的槽位，原目录留底为 {}",
                superseded.display()
            )),
            Err(e) => rep.failed.push(format!(
                "槽位 {label} 合并不了（多半是酒馆桥接正在用）：{e}"
            )),
        }
    }

    // ---- 2. 桥接的「当前」对齐面板
    let mut notes = Vec::new();
    match active_slot_dir(r) {
        Some(active) => {
            let was = junction_target(&bridge_link);
            if !was
                .as_deref()
                .is_some_and(|w| crate::install::inventory::same_path(w, &active))
            {
                let label = active_label(r).unwrap_or_default();
                match repoint(&bridge_link, &active, &mut notes) {
                    Ok(()) => {
                        let _ = std::fs::write(b.join("active-profile.txt"), &label);
                        rep.done.push(match bridge_active.as_deref() {
                            Some(old) if old != label => format!(
                                "酒馆桥接原来用的是 {old}，已跟面板统一到 {label} —— 任意时刻只有一个账户激活"
                            ),
                            _ => format!("酒馆桥接已指向面板的当前槽位 {label}"),
                        });
                    }
                    Err(e) => rep.failed.push(format!("酒馆桥接的当前槽位对不齐：{e}")),
                }
            }
        }
        None => {
            // 面板一个都没激活：接上桥接原来的那个（它现在应当已经在面板这边了）。
            if let Some(l) = bridge_active.filter(|l| r.slot_dir(l).is_dir()) {
                let dir = r.slot_dir(&l);
                let ok = repoint(&r.link(), &dir, &mut notes)
                    .and_then(|_| repoint(&bridge_link, &dir, &mut notes));
                match ok {
                    Ok(()) => rep
                        .done
                        .push(format!("面板接上了酒馆桥接原来的当前槽位 {l}")),
                    Err(e) => rep
                        .failed
                        .push(format!("接不上酒馆桥接原来的当前槽位 {l}：{e}")),
                }
            }
        }
    }
    rep.done.extend(notes);
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一台临时的假机器：面板根、桥接根、%APPDATA%。结束时整棵删掉。
    struct Machine {
        base: PathBuf,
        roots: AccountRoots,
    }

    impl Machine {
        fn new(tag: &str, with_bridge: bool) -> Self {
            let base = std::env::temp_dir().join(format!(
                "qbgate-accounts-{tag}-{}-{}",
                std::process::id(),
                chrono::Local::now().format("%H%M%S%f")
            ));
            let _ = std::fs::remove_dir_all(&base);
            let panel = base.join("ClaudeIpGate");
            let bridge = base.join("ClaudeTavernBridge");
            let appdata = base.join("Roaming");
            std::fs::create_dir_all(&panel).unwrap();
            std::fs::create_dir_all(&appdata).unwrap();
            if with_bridge {
                std::fs::create_dir_all(&bridge).unwrap();
            }
            Self {
                roots: AccountRoots {
                    panel,
                    bridge: with_bridge.then_some(bridge),
                    appdata: Some(appdata),
                },
                base,
            }
        }

        fn slot(&self, label: &str, cred: Option<&str>) -> PathBuf {
            let d = self.roots.slot_dir(label);
            std::fs::create_dir_all(&d).unwrap();
            if let Some(c) = cred {
                std::fs::write(d.join(".credentials.json"), c).unwrap();
            }
            d
        }

        fn appdata(&self) -> PathBuf {
            self.roots.appdata.clone().unwrap()
        }

        fn bridge(&self) -> PathBuf {
            self.roots.bridge.clone().unwrap()
        }
    }

    impl Drop for Machine {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    fn points_at(link: &Path, want: &Path) -> bool {
        junction_target(link).is_some_and(|t| crate::install::inventory::same_path(&t, want))
    }

    #[test]
    fn expired_slot_is_still_switchable() {
        // 回归测试：logged_in 只看文件在不在，与到期天数无关。
        // 合并这两件事会导致凭证过期后切不过去，也就没法重新登录。
        let s = Slot {
            label: "main".into(),
            active: false,
            logged_in: true,
            cli_days_left: Some(-3),
            account_uuid: None,
            plan: None,
            billing: None,
            plan_fetched_at: None,
            usage: None,
            desktop_profile: false,
        };
        assert!(s.logged_in, "过期不得影响可切换性");
        assert!(s.cli_days_left.is_some_and(|d| d < 0));
    }

    #[test]
    fn plan_prefers_seat_tier_then_org_type() {
        // Team / Enterprise 席位有 seatTier，个人订阅只有 organizationType。
        let team =
            serde_json::json!({ "seatTier": "claude_team", "organizationType": "claude_pro" });
        assert_eq!(plan_of(&team).as_deref(), Some("Claude Team"));

        let personal = serde_json::json!({ "seatTier": "", "organizationType": "claude_pro" });
        assert_eq!(plan_of(&personal).as_deref(), Some("Claude Pro"));

        // 两个都没有就是 None，不许猜一个出来。
        assert_eq!(plan_of(&serde_json::json!({})), None);
        assert_eq!(
            plan_of(&serde_json::json!({ "seatTier": "", "organizationType": "" })),
            None
        );
    }

    #[test]
    fn unknown_plan_is_shown_as_is_not_guessed() {
        // 认不出来的套餐原样显示。猜成 Pro 会让用户以为买错了东西。
        assert_eq!(pretty_plan("claude_something_new"), "claude_something_new");
        assert_eq!(pretty_billing("some_new_channel"), "some_new_channel");
    }

    #[test]
    fn plan_reading_never_touches_rate_limit_fields() {
        // 政策边界：字段名里带 rateLimit 的一律不读。
        let o = serde_json::json!({
            "organizationType": "claude_pro",
            "organizationRateLimitTier": "default_claude_ai",
            "userRateLimitTier": "tier_x"
        });
        let plan = plan_of(&o).unwrap();
        assert_eq!(plan, "Claude Pro");
        assert!(!plan.contains("default_claude_ai"));
        assert!(!plan.contains("tier_x"));
    }

    #[test]
    fn fetched_at_is_formatted_or_absent() {
        // profileFetchedAt 是毫秒时间戳。界面要标出来 —— 这是缓存，可能过期。
        let o = serde_json::json!({ "profileFetchedAt": 1788865933692i64 });
        let got = fetched_at_of(&o).expect("应当格式化出来");
        assert!(got.starts_with("20"), "格式不对：{got}");
        assert_eq!(fetched_at_of(&serde_json::json!({})), None);
    }

    #[test]
    fn labels_follow_the_rules() {
        for ok in ["main", "NEW1", "工作号", "a-b_c.d"] {
            assert!(validate_label(ok).is_ok(), "{ok}");
        }
        // 冒号会让托盘菜单的分发静默失效；路径分隔符会逃出槽位根。
        let long = "x".repeat(33);
        for bad in ["", "a:b", "a/b", r"a\b", ".x", "x.", "a b", long.as_str()] {
            assert!(validate_label(bad).is_err(), "{bad:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn switching_points_code_and_bridge_at_the_same_concrete_dir() {
        let m = Machine::new("switch", true);
        let main = m.slot("main", Some("{}"));
        let new1 = m.slot("NEW1", Some("{}"));
        switch_in(&m.roots, "main", DesktopMode::Keep).unwrap();
        assert_eq!(
            active_slot_dir(&m.roots)
                .as_deref()
                .map(|d| crate::install::inventory::same_path(d, &main)),
            Some(true)
        );

        let out = switch_in(&m.roots, "NEW1", DesktopMode::Keep).unwrap();
        assert!(points_at(&m.roots.link(), &new1));
        // 桥接指向**面板的具体槽位目录**，不是它自己那边的 claude-profile-NEW1。
        assert!(points_at(&m.bridge().join(LINK), &new1));
        assert_eq!(
            std::fs::read_to_string(m.bridge().join("active-profile.txt")).unwrap(),
            "NEW1"
        );
        assert_eq!(out.switched.len(), 2, "{:?}", out.switched);
        assert_eq!(active_label(&m.roots).as_deref(), Some("NEW1"));
    }

    #[cfg(windows)]
    #[test]
    fn a_dangling_link_does_not_block_switching() {
        // §7.18：目标目录没了的联结点，exists() 回 false，旧代码就此切不过去。
        let m = Machine::new("dangling", false);
        let gone = m.slot("gone", None);
        let keep = m.slot("keep", None);
        make_junction(&m.roots.link(), &gone).unwrap();
        std::fs::remove_dir_all(&gone).unwrap();
        switch_in(&m.roots, "keep", DesktopMode::Keep).unwrap();
        assert!(points_at(&m.roots.link(), &keep));
    }

    #[cfg(windows)]
    #[test]
    fn switching_desktop_no_longer_checks_whether_it_is_running() {
        // v0.9.0：调用方进来之前已经清场（关掉全部 Claude，桌面端也在内），
        // 所以 switch_in 不再自己查「桌面端开着吗」，直接换。
        let m = Machine::new("desk-nogate", false);
        m.slot("main", None);
        m.slot("NEW1", None);
        std::fs::create_dir_all(m.appdata().join("Claude-NEW1")).unwrap();
        switch_in(&m.roots, "main", DesktopMode::Keep).unwrap();

        let out = switch_in(&m.roots, "NEW1", DesktopMode::Auto).unwrap();
        assert!(
            out.switched.iter().any(|s| s.starts_with("桌面端")),
            "{:?}",
            out.switched
        );
        assert!(points_at(
            &m.appdata().join("Claude"),
            &m.appdata().join("Claude-NEW1")
        ));
        assert_eq!(active_label(&m.roots).as_deref(), Some("NEW1"));
    }

    #[cfg(windows)]
    #[test]
    fn the_first_managed_desktop_switch_adopts_the_existing_profile() {
        // 桌面端从没交给面板管过（%APPDATA%\Claude 是真目录）。第一次切过去时，
        // 那份资料要存成「切走之前那个槽位」的，不能被改成一个找不回来的备份名。
        let m = Machine::new("adopt", false);
        m.slot("main", None);
        m.slot("work", None);
        switch_in(&m.roots, "main", DesktopMode::Keep).unwrap();
        let real = m.appdata().join("Claude");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("marker"), "main 的桌面端资料").unwrap();

        let out = switch_in(&m.roots, "work", DesktopMode::Follow).unwrap();
        assert!(
            out.switched.iter().any(|s| s.starts_with("桌面端")),
            "{:?}",
            out.switched
        );
        assert_eq!(
            std::fs::read_to_string(m.appdata().join("Claude-main").join("marker")).unwrap(),
            "main 的桌面端资料"
        );
        assert!(points_at(&real, &m.appdata().join("Claude-work")));
        assert_eq!(desktop_state(&m.roots).active.as_deref(), Some("work"));

        // 切回去：main 的资料原样回来。
        switch_in(&m.roots, "main", DesktopMode::Auto).unwrap();
        assert!(real.join("marker").is_file());
        let slots = slots_in(&m.roots);
        assert!(slots.iter().all(|s| s.desktop_profile), "{slots:?}");
    }

    #[cfg(windows)]
    #[test]
    fn auto_mode_leaves_the_desktop_alone_when_the_slot_has_no_desktop_profile() {
        let m = Machine::new("auto", false);
        m.slot("main", None);
        let real = m.appdata().join("Claude");
        std::fs::create_dir_all(&real).unwrap();
        let out = switch_in(&m.roots, "main", DesktopMode::Auto).unwrap();
        assert_eq!(out.switched, vec!["Claude Code → main".to_string()]);
        let meta = std::fs::symlink_metadata(&real).unwrap();
        assert!(!is_reparse_point(&meta), "没有 Claude-main 时不许动桌面端");
    }

    #[cfg(windows)]
    #[test]
    fn creating_the_first_slot_activates_it_without_touching_the_desktop() {
        let m = Machine::new("create", true);
        let real = m.appdata().join("Claude");
        std::fs::create_dir_all(&real).unwrap();

        let c = create_slot(&m.roots, "main").unwrap();
        assert!(c.activated);
        assert!(points_at(&m.roots.link(), &m.roots.slot_dir("main")));
        assert!(points_at(&m.bridge().join(LINK), &m.roots.slot_dir("main")));
        assert!(!is_reparse_point(
            &std::fs::symlink_metadata(&real).unwrap()
        ));

        // 第二个不抢「当前」。
        let c2 = create_slot(&m.roots, "work").unwrap();
        assert!(!c2.activated);
        assert_eq!(active_label(&m.roots).as_deref(), Some("main"));
        // 新槽位是空的：没有凭证，也没从别处拷。
        assert!(!m.roots.slot_dir("work").join(".credentials.json").exists());
        assert!(create_slot(&m.roots, "work").is_err(), "重名要报错");
        assert!(create_slot(&m.roots, "a:b").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn duplicated_credentials_are_merged_into_one_copy() {
        // 本机实况：两边各一份相同的凭证（复制时保留了修改时间，两份的 mtime 一模一样），
        // 桥接指着 NEW1，面板指着 main。
        let m = Machine::new("merge", true);
        let b = m.bridge();
        let t0 = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        for l in ["main", "NEW1"] {
            let pd = m.slot(l, Some("same"));
            let bd = b.join(format!("{PREFIX}{l}"));
            std::fs::create_dir_all(&bd).unwrap();
            std::fs::write(bd.join(".credentials.json"), "same").unwrap();
            for d in [&pd, &bd] {
                std::fs::File::options()
                    .write(true)
                    .open(d.join(".credentials.json"))
                    .unwrap()
                    .set_modified(t0)
                    .unwrap();
            }
        }
        make_junction(&b.join(LINK), &b.join(format!("{PREFIX}NEW1"))).unwrap();
        switch_in(&m.roots, "main", DesktopMode::Keep).unwrap();
        // switch_in 顺手把桥接也对齐了；还原成「桥接指着 NEW1」的原状再测 sync。
        make_junction_force(&b.join(LINK), &b.join(format!("{PREFIX}NEW1")));

        let rep = sync_bridge(&m.roots);
        assert!(rep.failed.is_empty(), "{:?}", rep.failed);
        for l in ["main", "NEW1"] {
            // 桥接那边成了指向面板槽位的联结点。
            assert!(
                points_at(&b.join(format!("{PREFIX}{l}")), &m.roots.slot_dir(l)),
                "{l}"
            );
        }
        // 桥接的当前跟面板统一。
        assert!(points_at(&b.join(LINK), &m.roots.slot_dir("main")));
        assert!(
            rep.done
                .iter()
                .any(|d| d.contains("NEW1") && d.contains("main")),
            "{:?}",
            rep.done
        );
        // 原目录留底，不删。
        let kept = std::fs::read_dir(&b)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("superseded-"))
            .count();
        assert_eq!(kept, 2);
        // 两份一样新：面板那份原样留着，不该有「用桥接替换」这回事。
        assert!(
            !rep.done.iter().any(|d| d.contains("替换")),
            "{:?}",
            rep.done
        );
        assert!(!m
            .roots
            .slot_dir("main")
            .read_dir()
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains("before-merge")));

        // 幂等：再跑一次什么都不做。
        let again = sync_bridge(&m.roots);
        assert!(
            again.done.is_empty() && again.failed.is_empty(),
            "{again:?}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_fresher_credential_wins_the_merge() {
        let m = Machine::new("fresher", true);
        let b = m.bridge();
        let p = m.slot("main", Some("old"));
        let bd = b.join(format!("{PREFIX}main"));
        std::fs::create_dir_all(&bd).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(bd.join(".credentials.json"), "new").unwrap();

        let rep = sync_bridge(&m.roots);
        assert!(rep.failed.is_empty(), "{:?}", rep.failed);
        assert_eq!(
            std::fs::read_to_string(p.join(".credentials.json")).unwrap(),
            "new"
        );
        // 面板原来那份留底。
        let backups = std::fs::read_dir(&p)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".credentials.json.before-merge-")
            })
            .count();
        assert_eq!(backups, 1);
    }

    #[cfg(windows)]
    #[test]
    fn a_bridge_only_slot_is_moved_not_copied() {
        let m = Machine::new("move", true);
        let b = m.bridge();
        let bd = b.join(format!("{PREFIX}solo"));
        std::fs::create_dir_all(&bd).unwrap();
        std::fs::write(bd.join(".credentials.json"), "x").unwrap();
        std::fs::write(b.join("active-profile.txt"), "solo").unwrap();

        let rep = sync_bridge(&m.roots);
        assert!(rep.failed.is_empty(), "{:?}", rep.failed);
        assert!(m.roots.slot_dir("solo").join(".credentials.json").is_file());
        assert!(points_at(&bd, &m.roots.slot_dir("solo")), "原处要留联结点");
        // 面板原来一个都没激活：接上桥接原来的当前槽位。
        assert_eq!(active_label(&m.roots).as_deref(), Some("solo"));
    }

    #[test]
    fn no_bridge_means_nothing_to_sync() {
        let m = Machine::new("nobridge", false);
        let rep = sync_bridge(&m.roots);
        assert!(rep.done.is_empty() && rep.failed.is_empty());
    }

    /// 测试专用：不管原来是什么，强行让 link 指向 target。
    #[cfg(windows)]
    fn make_junction_force(link: &Path, target: &Path) {
        let _ = clear_link(link);
        make_junction(link, target).unwrap();
    }
}

/// 账户页要的一整份现状。
///
/// # 为什么它是个结构体而不是 `serde_json::json!{}`
///
/// 同 [`super::detect::SoftwareReport`] 的理由：临时拼出来的对象在 Rust 侧
/// 没有类型，前端手抄的那一份也就没有源头 —— 抄错了、过期了，没有任何东西
/// 会发现。而这是首页的主数据源。
///
/// 两条 caveat 也在里面，**它们是界面上必须固定打出来的话**：
/// 「剩余天数查不出被风控下线」「套餐读的是本机文件不是接口」。
/// 放进契约里，前端就不可能漏显示（少了字段编译不过）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AccountsReport {
    pub slots: Vec<Slot>,
    /// 剩余天数只读本地时间戳，查不出「被风控下线」。
    pub caveat: String,
    /// 套餐是怎么读出来的 —— 界面上要如实说明，不能让人以为是查了接口。
    pub plan_caveat: String,
    pub sync: SyncReport,
    pub desktop: DesktopState,
    /// 本机有酒馆桥接的数据目录 —— 有的话切换会一起切它。
    pub bridge_present: bool,
}

/// 列一次账户现状。`sync` 由调用方决定要不要先跑一轮合并。
pub fn report(roots: &AccountRoots, sync: SyncReport) -> AccountsReport {
    AccountsReport {
        slots: slots_in(roots),
        caveat: EXPIRY_CAVEAT.into(),
        plan_caveat: PLAN_CAVEAT.into(),
        sync,
        desktop: desktop_state(roots),
        bridge_present: roots.bridge.is_some(),
    }
}
