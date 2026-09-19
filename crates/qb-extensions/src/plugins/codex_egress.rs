//! Codex 出站与换出口插件 —— 接入本机已安装的 `ccodex-sleep-state`（外部程序，GPL-3.0）。
//!
//! # 这个模块只做「接入」，跟酒馆同一类
//!
//! 定位本机那份 exe（或从它自己的 Releases 一键下载、按 SHA256SUMS 校验后解压）、
//! 启动它、停掉它、打开它自己的网页面板。**不编译、不复制它一行源码、不把它链进本程序** ——
//! 下载的只是它发布的二进制包。它是一个独立的第三方程序，
//! 代理 / 订阅 / 机场出站 / 换出口凑 292 全在它自己的仓库里实现；
//! 核心仍然不内置代理、不换出口（`CLAUDE.md`「不许加的功能」原样成立）。
//! 来源与许可记在 `ATTRIBUTION.md`。
//!
//! # ⛔ 跟核心的官方 turn-state 识别互斥（双向）
//!
//! 它接管的是**当前激活 Codex 槽位**的目录（没有槽位就是默认 `~/.codex`）；0.24.7 起核心的
//! 官方识别也直接改**同一个槽位**的 `config.toml`（`model_provider` 指到本机路由）——
//! 同时开就是两个程序抢同一个文件，谁后写谁生效，另一个静默失效。一次只开一个。
//! 守卫在命令层：`codex_egress_start` 看 `official_codex_armed()` 和落盘的接管 marker
//! （`turnstate_ops::takeover`），`station_turnstate_enable` 看这里的 [`running`]；
//! 解开靠账户页「关闭识别」（`station_turnstate_disable`，会恢复槽位配置）或这里的停止。
//!
//! # 启停为什么这样做
//!
//! * 启动用 [`OwnedProgram::launch_detached_console`]：带窗口、不随面板退出而被杀。
//!   它退出时要恢复 Codex 配置（Ctrl+C 走它自己的恢复流程）；被面板顺手 `KILL_ON_JOB_CLOSE`
//!   掉，Codex 就留在一个指向死服务的配置上，症状是 503 而看不出所以然。
//! * 停止 = 结束进程 **再跑它自己的 `restore`**：那条命令就是为「崩溃/被杀后把 Codex
//!   配置恢复回去、不覆盖使用者新改动」设计的。只结束不恢复等于把使用者的 Codex 弄坏。
//! * 端口被占但不是本面板起的（使用者自己双击了 `start.cmd`，或面板重启后认不出了）：
//!   如实报「不是本面板启动的」，**不冒充受管进程去停它**（跟酒馆那条「未登记的
//!   已占用端口不会被冒充为受管进程」同一条纪律）。

use crate::error::{GateError, Result};
use crate::plugins::{DependencyCheck, PluginState, PluginStatus};
use crate::sink::ProgressSink;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// 发布源：使用者自己的 fork（跟目录条目的 `source` 是同一个仓库）。
/// 一键安装从这里的 Releases 取 Windows 包与 `SHA256SUMS`。
pub const RELEASE_REPO: &str = "smithtaylor7748-ops/qb-gate-codex-egress";
const SUMS_ASSET: &str = "SHA256SUMS";

pub const PLUGIN_ID: &str = "codex-egress";
pub const PLUGIN_NAME: &str = "Codex 出站与换出口（ccodex 引擎）";
/// 程序默认监听的端口（它自己的默认 `127.0.0.1:17841`）。
pub const PORT: u16 = 17841;
/// 它自己的管理面板地址。
pub const ADMIN_URL: &str = "http://127.0.0.1:17841/admin/";
/// 发布包里的可执行文件名（沿用上游，不改名 —— 改名会跟它的 start.cmd 与文档对不上）。
pub const EXE_NAME: &str = "ccodex-sleep-state.exe";
/// 面板托管记录的名字（`plugins/<name>-process.json`）。
const OWNED: &str = "codex-egress";

/// 使用者指定的安装位置。空 = 用默认候选位置。
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EgressConfig {
    /// `ccodex-sleep-state.exe` 的完整路径；留空则按默认位置找。
    pub exe: Option<String>,
}

fn config_path() -> PathBuf {
    crate::paths::state_dir()
        .join("plugins")
        .join("codex-egress.json")
}

pub fn load_config() -> EgressConfig {
    crate::config_io::read_optional(&config_path())
        .ok()
        .flatten()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// 使用者指定的路径合不合格：必须存在，且就是 [`EXE_NAME`] 本身。
/// 指到目录、`start.cmd` 或别的文件都拒绝 —— 等到启动时才发现就晚了。纯判定，不落盘。
fn validate_configured(exe: &str) -> Result<()> {
    let p = Path::new(exe);
    if !p.is_file() {
        return Err(GateError::Other(format!("找不到文件：{exe}")));
    }
    if !p
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case(EXE_NAME))
    {
        return Err(GateError::Other(format!(
            "请选择 {EXE_NAME} 本身（发布包解压后的那个可执行文件）"
        )));
    }
    Ok(())
}

pub fn save_config(cfg: &EgressConfig) -> Result<()> {
    if let Some(exe) = cfg.exe.as_deref().filter(|s| !s.trim().is_empty()) {
        validate_configured(exe)?;
    }
    crate::config_io::replace(&config_path(), Some(&serde_json::to_vec_pretty(cfg)?))
}

/// 候选位置：使用者指定的 → 默认解压位置（`%LOCALAPPDATA%\Programs\<名字>\`）。
///
/// ⚠ 发布包（zip）里带一层顶级目录 `ccodex-sleep-state-v…-windows-amd64\`，
/// 使用者「解压到 Programs\ccodex-sleep-state」之后 exe 多半在**再下一层**。
/// 所以每个默认目录既看 `目录\exe`，也看 `目录\*\exe`（只往下找一层，不递归）——
/// 不然「装了却找不到」是这个插件最常见的第一印象。
fn candidates(cfg: &EgressConfig) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(exe) = cfg.exe.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push(PathBuf::from(exe));
    }
    if let Some(local) = dirs::data_local_dir() {
        for dir in ["ccodex-sleep-state", "qb-gate-codex-egress"] {
            out.extend(candidates_under(&local.join("Programs").join(dir)));
        }
    }
    out
}

/// 一个默认目录下的候选：`目录\exe`，再加 `目录\<每个子目录>\exe`（只一层）。
///
/// ⛔ 跳过 `.` 开头的子目录：安装失败残留的 `.incoming-*` 暂存目录里也有一份 exe，
/// 而 `.` 排在字母前面 —— 不跳过的话残留会被**优先**选中，装了新版本还在跑旧的暂存文件。
fn candidates_under(base: &Path) -> Vec<PathBuf> {
    let mut out = vec![base.join(EXE_NAME)];
    if let Ok(entries) = std::fs::read_dir(base) {
        let mut subs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && !p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with('.'))
            })
            .collect();
        subs.sort();
        out.extend(subs.into_iter().map(|d| d.join(EXE_NAME)));
    }
    out
}

/// 找到本机装的那份 exe（第一个存在的候选）。
pub fn locate() -> Option<PathBuf> {
    candidates(&load_config()).into_iter().find(|p| p.is_file())
}

/// 问一下它的版本（`exe version`）。**按「路径 + 修改时间」缓存** —— 状态每 10 秒刷一次，
/// 每次都 spawn 一个进程既浪费又在主线程上卡一下；exe 没换过，版本就不会变。
/// 读不出就 `None`，不影响别的判断。
#[allow(clippy::type_complexity)]
fn version(exe: &Path) -> Option<String> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<Option<(PathBuf, std::time::SystemTime, Option<String>)>>,
    > = std::sync::OnceLock::new();
    let mtime = std::fs::metadata(exe).and_then(|m| m.modified()).ok()?;
    let cache = CACHE.get_or_init(Default::default);
    if let Some((p, t, v)) = cache.lock().unwrap().as_ref() {
        if p == exe && *t == mtime {
            return v.clone();
        }
    }
    let v = crate::process::hidden_std(std::process::Command::new(exe))
        .arg("version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    *cache.lock().unwrap() = Some((exe.to_path_buf(), mtime, v.clone()));
    v
}

fn port_in_use() -> bool {
    std::net::TcpListener::bind(("127.0.0.1", PORT)).is_err()
}

/// 它该管哪个 Codex：**当前激活的 Codex 账户槽位**的目录；没有槽位就用它自己的默认 `~/.codex`。
///
/// 使用者通过 QB Gate 用的 Codex 都在各自的槽位目录里（`CODEX_HOME` 各自独立），
/// 让插件只管默认 `~/.codex` 等于「开了也管不到你在用的那份」。所以启动时把
/// `CODEX_HOME` 指到激活槽位，插件接管的、之后 `restore` 恢复的，都是同一份。
/// 只读槽位路径，不碰槽位里的任何文件。
fn managed_home() -> Option<PathBuf> {
    let root = qb_accounts::codex::root();
    let accounts = qb_accounts::codex::list(&root).ok()?;
    let active = accounts.slots.iter().find(|s| s.active)?;
    let home = qb_accounts::codex::directory(&root, &active.id)
        .ok()?
        .join("home");
    home.is_dir().then_some(home)
}

// ------------------------------------------------------------ 托管进程（照酒馆那套）

/// 落盘的托管记录：进程身份 + **它接管的是哪个 Codex 目录**。
/// 后者必须记下来：使用者中途切了槽位，停止时 `restore` 仍要恢复**当初接管的那份**。
#[derive(Serialize, Deserialize)]
struct EgressRun {
    record: crate::sessions::ProgramRecord,
    /// `None` = 插件自己的默认 `~/.codex`。
    home: Option<String>,
}

#[allow(clippy::type_complexity)]
static PROGRAM: std::sync::OnceLock<
    std::sync::Mutex<Option<(crate::sessions::OwnedProgram, Option<String>)>>,
> = std::sync::OnceLock::new();
#[allow(clippy::type_complexity)]
fn program() -> &'static std::sync::Mutex<Option<(crate::sessions::OwnedProgram, Option<String>)>> {
    PROGRAM.get_or_init(Default::default)
}
fn record_path() -> PathBuf {
    crate::paths::state_dir()
        .join("plugins")
        .join(format!("{OWNED}-process.json"))
}
/// 本面板起的那份还在跑吗。面板重启后命名 Job 已经没了，`reopen` 会回 `None` ——
/// 那时把过期记录清掉，如实当成「不是本面板起的」。
fn owned_running() -> Result<bool> {
    let mut slot = program().lock().unwrap();
    if slot.is_none() {
        if let Some(bytes) = crate::config_io::read_optional(&record_path())? {
            let run: EgressRun = serde_json::from_slice(&bytes)?;
            match crate::sessions::OwnedProgram::reopen(run.record)? {
                Some(p) => *slot = Some((p, run.home)),
                None => crate::config_io::replace(&record_path(), None)?,
            }
        }
    }
    slot.as_ref().map(|(p, _)| p.running()).unwrap_or(Ok(false))
}

/// 插件在不在跑（本面板起的，或端口被别的实例占着）。给互斥守卫用。
pub fn running() -> bool {
    owned_running().unwrap_or(false) || port_in_use()
}

pub fn status() -> PluginStatus {
    let cfg = load_config();
    let exe = candidates(&cfg).into_iter().find(|p| p.is_file());
    let mut checks = Vec::new();
    let (state, detail) = match exe {
        None => {
            checks.push(DependencyCheck::new(
                "程序文件",
                false,
                format!(
                    "没找到 {EXE_NAME}。点「下载并安装」从 {RELEASE_REPO} 的 Releases 取包（按 SHA256SUMS 校验后解压），\
                     或自己解压到 %LOCALAPPDATA%\\Programs\\ccodex-sleep-state，或在下面指定 exe 位置。"
                ),
            ));
            (
                PluginState::Missing,
                "还没装。它是一个独立的第三方程序（GPL-3.0），本面板只负责接入。".to_string(),
            )
        }
        Some(exe) => {
            let v = version(&exe);
            checks.push(DependencyCheck::new(
                "程序文件",
                true,
                match &v {
                    Some(v) => format!("{}（版本 {v}）", exe.display()),
                    None => format!("{}（读不出版本，可能不是这个程序）", exe.display()),
                },
            ));
            let owned = owned_running().unwrap_or(false);
            let busy = port_in_use();
            checks.push(DependencyCheck::new(
                format!("端口 {PORT}"),
                !busy || owned,
                if owned && busy {
                    "本面板启动的服务正在监听".to_string()
                } else if owned {
                    // 进程在、端口还没听起来：多半在等使用者处理它窗口里的提示，别报成「正在监听」。
                    "本面板起的进程在跑，但端口还没听起来（看看它的窗口是不是在等你确认）"
                        .to_string()
                } else if busy {
                    "被占用，但不是本面板启动的（可能是你自己双击了 start.cmd）".to_string()
                } else {
                    "空闲".to_string()
                },
            ));
            // 它接管的是哪个 Codex 目录 —— 正在跑就显示当初接管的那份，没在跑就显示下次会接管的那份。
            let home_now = program()
                .lock()
                .unwrap()
                .as_ref()
                .map(|(_, h)| h.clone())
                .unwrap_or_else(|| managed_home().map(|p| p.display().to_string()));
            checks.push(DependencyCheck::new(
                "接管的 Codex 目录",
                true,
                match home_now {
                    Some(h) => format!("{h}（当前激活的 Codex 账户槽位）"),
                    None => "默认 ~/.codex（没有激活的 Codex 账户槽位）".to_string(),
                },
            ));
            if owned {
                (PluginState::Running, format!("运行中，面板：{ADMIN_URL}"))
            } else if busy {
                (
                    PluginState::Running,
                    "已在运行，但不是本面板起的；要停请到它自己的窗口按 Ctrl+C（会自动恢复 Codex 配置）。"
                        .into(),
                )
            } else {
                (PluginState::Ready, "已安装，没在跑。".into())
            }
        }
    };
    PluginStatus {
        id: PLUGIN_ID,
        name: PLUGIN_NAME,
        state,
        detail,
        checks,
    }
}

/// 启动（`exe setup`：备份并接管 Codex 配置、起服务、自动打开它的网页面板）。
/// 返回管理面板地址。互斥守卫（核心官方线挂着就不许起）在命令层。
pub fn start() -> Result<String> {
    if owned_running()? {
        return Ok(ADMIN_URL.into());
    }
    if port_in_use() {
        return Err(GateError::Other(format!(
            "端口 {PORT} 已被占用，但不是本面板启动的。若是你自己双击 start.cmd 起的，直接用那一个；否则先把占用的程序退掉。"
        )));
    }
    let exe = locate().ok_or_else(|| {
        GateError::Other(format!(
            "没找到 {EXE_NAME}。先到插件仓库的 Releases 下载发布包解压，或在插件页指定 exe 位置。"
        ))
    })?;
    let cwd = exe
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default());
    // 接管当前激活槽位的 Codex（没有槽位就让它用自己的默认 ~/.codex）。
    // `sanitized_environment` 会剥掉继承来的 CODEX_HOME，这里显式补上要的那份。
    let home = managed_home().map(|p| p.display().to_string());
    let env: Vec<(String, String)> = home
        .iter()
        .map(|h| ("CODEX_HOME".to_string(), h.clone()))
        .collect();
    let owned =
        crate::sessions::OwnedProgram::launch_detached_console(&exe, &["setup".into()], &cwd, env)?;
    let run = EgressRun {
        record: owned.record.clone(),
        home: home.clone(),
    };
    if let Err(error) = crate::config_io::replace(&record_path(), Some(&serde_json::to_vec(&run)?))
    {
        let _ = owned.stop();
        return Err(error);
    }
    *program().lock().unwrap() = Some((owned, home));
    // 等它把端口听起来（它自己要先备份配置再起服务）。等不到不算失败 ——
    // 它可能正在等使用者在它的窗口里处理提示；界面上会如实显示端口状态。
    for _ in 0..40 {
        if port_in_use() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    crate::audit::write("启动了 Codex 出站插件（ccodex-sleep-state setup）");
    Ok(ADMIN_URL.into())
}

/// 停止：结束本面板起的那份，**再跑它自己的 `restore`** 把 Codex 配置恢复回去。
/// 不是本面板起的一律不动（如实报错，让使用者去它的窗口 Ctrl+C）。
pub fn stop() -> Result<Vec<String>> {
    if !owned_running()? {
        if port_in_use() {
            return Err(GateError::Other(
                "正在运行的那份不是本面板启动的，本面板不替它停：请到它自己的窗口按 Ctrl+C，它会自动恢复 Codex 配置。"
                    .into(),
            ));
        }
        crate::config_io::replace(&record_path(), None)?;
        return Ok(vec!["没有在跑的插件进程".into()]);
    }
    let mut done = Vec::new();
    // 当初接管的是哪个目录，restore 就恢复哪个 —— 使用者中途切了槽位也不能恢复错地方。
    let home = {
        let mut slot = program().lock().unwrap();
        let home = slot.as_ref().and_then(|(_, h)| h.clone());
        if let Some((p, _)) = slot.as_ref() {
            p.stop()?;
            done.push("已结束插件进程".into());
        }
        *slot = None;
        home
    };
    crate::config_io::replace(&record_path(), None)?;
    // 被结束的进程来不及走自己的恢复流程 —— 用它自己的 restore 补上。
    if let Some(exe) = locate() {
        let mut cmd = std::process::Command::new(&exe);
        if let Some(h) = &home {
            cmd.env("CODEX_HOME", h);
        }
        let out = crate::process::hidden_std(cmd).arg("restore").output()?;
        let text = String::from_utf8_lossy(if out.status.success() {
            &out.stdout
        } else {
            &out.stderr
        })
        .trim()
        .to_string();
        if out.status.success() {
            done.push(format!(
                "已用它自己的 restore 恢复 Codex 配置{}",
                if text.is_empty() {
                    String::new()
                } else {
                    format!("：{text}")
                }
            ));
        } else {
            done.push(format!(
                "restore 没成功{}。请在 Codex 里确认配置，或打开它的面板用「检查与修复配置」。",
                if text.is_empty() {
                    String::new()
                } else {
                    format!("：{text}")
                }
            ));
        }
    }
    crate::audit::write("停止了 Codex 出站插件并执行 restore");
    Ok(done)
}

// ------------------------------------------------------------ 一键下载、校验、安装

/// 一键安装的结果。`sha256` 是实际下载算出来的值（已与发布的 `SHA256SUMS` 核对一致）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct EgressInstall {
    pub tag: String,
    pub exe: String,
    pub sha256: String,
}

/// 这台机器该下哪个包。
fn windows_asset() -> &'static str {
    if std::env::consts::ARCH == "aarch64" {
        "ccodex-sleep-state-windows-arm64.zip"
    } else {
        "ccodex-sleep-state-windows-amd64.zip"
    }
}

/// 从 `GET /repos/{repo}/releases` 的列表里挑**最新一个**同时带有我们要的 zip 和 `SHA256SUMS` 的发布
/// （含 prerelease —— 上游的发布流水线一律标成 prerelease，`releases/latest` 会看不见它们）。
/// 返回 (tag, zip 地址, zip 大小, SHA256SUMS 地址)。纯解析，可单测。
fn pick_release(
    list: &serde_json::Value,
    asset: &str,
) -> Result<(String, String, Option<u64>, String)> {
    let releases = list
        .as_array()
        .ok_or_else(|| GateError::Other("GitHub 返回的发布列表不是数组".into()))?;
    for r in releases {
        if r["draft"].as_bool().unwrap_or(false) {
            continue;
        }
        let assets = r["assets"].as_array().cloned().unwrap_or_default();
        let find = |name: &str| {
            assets
                .iter()
                .find(|a| a["name"].as_str() == Some(name))
                .and_then(|a| {
                    let url = a["browser_download_url"].as_str()?;
                    // 只认 github.com 上的下载地址，别被一条改过的 JSON 带去别处。
                    if !url.starts_with("https://github.com/") {
                        return None;
                    }
                    Some((url.to_string(), a["size"].as_u64()))
                })
        };
        if let (Some((zip, size)), Some((sums, _))) = (find(asset), find(SUMS_ASSET)) {
            let tag = r["tag_name"].as_str().unwrap_or("").to_string();
            return Ok((tag, zip, size, sums));
        }
    }
    Err(GateError::Other(format!(
        "仓库 {RELEASE_REPO} 的 Releases 里没有同时带 {asset} 和 {SUMS_ASSET} 的发布。先在仓库打一个 v* tag 让 CI 出包。"
    )))
}

/// 从 `SHA256SUMS`（每行 `<hex>  <文件名>`）里取指定文件的哈希（小写）。纯解析，可单测。
fn expected_sha(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut it = line.split_whitespace();
        let hex = it.next()?;
        let file = it.next()?.trim_start_matches('*');
        (file == name && hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hex.to_ascii_lowercase())
    })
}

fn http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .user_agent(concat!("QB Gate/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| GateError::Other(format!("建不了网络客户端：{e}")))
}

async fn get_text(c: &reqwest::Client, url: &str) -> Result<String> {
    let r = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        c.get(url)
            .header("Accept", "application/vnd.github+json, text/plain")
            .send()
            .await?
            .error_for_status()?
            .text()
            .await
    })
    .await
    .map_err(|_| GateError::Other(format!("访问 {url} 超过 60 秒没有回应")))?;
    r.map_err(|e| GateError::Other(format!("访问 {url} 失败：{e}")))
}

/// 暂存目录里找 exe（最多往下两层）。
fn find_exe(root: &Path) -> Option<PathBuf> {
    let mut stack = vec![(root.to_path_buf(), 0u32)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth < 2 {
                    stack.push((p, depth + 1));
                }
            } else if p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(EXE_NAME))
            {
                return Some(p);
            }
        }
    }
    None
}

/// 改名目录，失败了隔 300ms 再试几次。刚解压出来的 exe 常被杀软/索引器短暂占住，
/// 第一次 `rename` 报「拒绝访问 / 正在使用」几乎都是这个，等一下就好；真失败再报。
fn rename_with_retry(from: &Path, to: &Path) -> Result<()> {
    let mut last = None;
    for _ in 0..8 {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
    }
    Err(GateError::Other(format!(
        "把 {} 换到 {} 失败：{}（多半是杀软或索引器还占着刚解压的文件，稍后重试）",
        from.display(),
        to.display(),
        last.map(|e| e.to_string()).unwrap_or_default()
    )))
}

/// 一键下载、校验、安装到默认位置，并把 exe 位置登记进配置。
///
/// 五段：查发布 → 读校验和 → 下载（边下边算 SHA-256）→ 解压到干净暂存目录再换进去 → 登记。
///
/// * 校验的是**发布里自带的** `SHA256SUMS`：它防的是下载损坏、下错包，
///   **不防仓库本身被改** —— 仓库是使用者自己的，这一层由他自己守。
/// * 正在运行时不装：Windows 上替换正在跑的 exe 会失败，而且它接管着 Codex 配置。
/// * 旧版本改名成 `<名字>.old-<时间>` 留一份，更早的清掉；程序自己的配置在
///   `%LOCALAPPDATA%\ccodex-sleep-state`，不在这个目录里，换程序不丢配置。
pub async fn install(rep: &dyn ProgressSink) -> Result<EgressInstall> {
    if owned_running()? || port_in_use() {
        return Err(GateError::Other(
            "插件正在运行，先停掉（会恢复 Codex 配置）再安装或更新。".into(),
        ));
    }
    let asset = windows_asset();
    rep.phase(1, &format!("查询 {RELEASE_REPO} 的发布"));
    let c = http()?;
    let list: serde_json::Value = serde_json::from_str(
        &get_text(
            &c,
            &format!("https://api.github.com/repos/{RELEASE_REPO}/releases?per_page=10"),
        )
        .await?,
    )?;
    let (tag, zip_url, size, sums_url) = pick_release(&list, asset)?;

    rep.phase(2, &format!("读取 {tag} 的校验和"));
    let sums = get_text(&c, &sums_url).await?;
    let expected = expected_sha(&sums, asset)
        .ok_or_else(|| GateError::Other(format!("{tag} 的 {SUMS_ASSET} 里没有 {asset} 的哈希")))?;

    rep.phase(
        3,
        &format!(
            "下载 {asset}{}",
            size.map(|s| format!("（{} MB）", s >> 20))
                .unwrap_or_default()
        ),
    );
    let dl_dir = crate::paths::state_dir().join("plugins").join("downloads");
    std::fs::create_dir_all(&dl_dir)?;
    let zip = dl_dir.join(asset);
    let got = crate::install::managed::download(&zip_url, &zip, size, rep, 3).await?;
    if got != expected {
        let _ = std::fs::remove_file(&zip);
        return Err(GateError::Other(format!(
            "下载的 {asset} 与 {tag} 发布的 SHA256SUMS 对不上（算出 {got}，应为 {expected}），已删除，未安装。"
        )));
    }

    rep.phase(4, "解压并换入");
    let base = dirs::data_local_dir()
        .ok_or_else(|| GateError::Other("找不到 %LOCALAPPDATA%".into()))?
        .join("Programs")
        .join("ccodex-sleep-state");
    std::fs::create_dir_all(&base)?;
    // 上次装到一半留下的暂存目录先清掉：定位器已经会跳过 `.` 开头的目录，
    // 但留着它们只会占地方、让人以为装了两份。
    if let Ok(rd) = std::fs::read_dir(&base) {
        for stale in rd.flatten().map(|e| e.path()).filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(".incoming-"))
        }) {
            let _ = std::fs::remove_dir_all(stale);
        }
    }
    let staging = base.join(format!(".incoming-{}", crate::config_io::id()));
    crate::install::managed::unzip(&zip, &staging).await?;
    let exe = find_exe(&staging).ok_or_else(|| {
        let _ = std::fs::remove_dir_all(&staging);
        GateError::Other(format!(
            "解压后没找到 {EXE_NAME}，这个包不是预期的发布包，未安装。"
        ))
    })?;
    let inner = exe
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(staging.clone());
    let name = if inner == staging {
        asset.trim_end_matches(".zip").to_string()
    } else {
        inner
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("ccodex-sleep-state")
            .to_string()
    };
    let target = base.join(&name);
    if target.exists() {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        rename_with_retry(&target, &base.join(format!("{name}.old-{stamp}")))?;
        // 只留最新的一份旧版本。
        if let Ok(rd) = std::fs::read_dir(&base) {
            let mut olds: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with(&format!("{name}.old-")))
                })
                .collect();
            olds.sort();
            for old in olds.iter().rev().skip(1) {
                let _ = std::fs::remove_dir_all(old);
            }
        }
    }
    rename_with_retry(&inner, &target)?;
    if inner != staging {
        let _ = std::fs::remove_dir_all(&staging);
    }
    let _ = std::fs::remove_file(&zip);
    let exe_final = target.join(EXE_NAME);

    rep.phase(5, "登记安装位置");
    save_config(&EgressConfig {
        exe: Some(exe_final.display().to_string()),
    })?;
    crate::audit::write(&format!(
        "安装了 Codex 出站插件 {tag}（{asset}，SHA-256 {got}）到 {}",
        target.display()
    ));
    rep.done(&format!("已安装 {tag}"));
    Ok(EgressInstall {
        tag,
        exe: exe_final.display().to_string(),
        sha256: got,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 发布列表：跳过 draft，取第一个同时带 zip 和 SHA256SUMS 的；下载地址只认 github.com。
    #[test]
    fn picks_the_newest_release_that_has_both_assets_and_only_github_urls() {
        let list = serde_json::json!([
            {"tag_name":"v0.2.0","draft":true,"assets":[
                {"name":"ccodex-sleep-state-windows-amd64.zip","browser_download_url":"https://github.com/x/y/releases/download/v0.2.0/a.zip","size":1},
                {"name":"SHA256SUMS","browser_download_url":"https://github.com/x/y/releases/download/v0.2.0/SHA256SUMS","size":1}]},
            {"tag_name":"v0.1.1","draft":false,"assets":[
                {"name":"ccodex-sleep-state-windows-amd64.zip","browser_download_url":"https://evil.example.com/a.zip","size":1},
                {"name":"SHA256SUMS","browser_download_url":"https://github.com/x/y/releases/download/v0.1.1/SHA256SUMS","size":1}]},
            {"tag_name":"v0.1.0","draft":false,"prerelease":true,"assets":[
                {"name":"ccodex-sleep-state-windows-amd64.zip","browser_download_url":"https://github.com/x/y/releases/download/v0.1.0/a.zip","size":17962691},
                {"name":"SHA256SUMS","browser_download_url":"https://github.com/x/y/releases/download/v0.1.0/SHA256SUMS","size":515}]}
        ]);
        let (tag, zip, size, sums) =
            pick_release(&list, "ccodex-sleep-state-windows-amd64.zip").unwrap();
        // v0.2.0 是 draft 跳过；v0.1.1 的 zip 地址不在 github.com 上，不认；落到 v0.1.0。
        assert_eq!(tag, "v0.1.0");
        assert!(zip.ends_with("/v0.1.0/a.zip"));
        assert_eq!(size, Some(17962691));
        assert!(sums.ends_with("/v0.1.0/SHA256SUMS"));
        assert!(pick_release(&list, "ccodex-sleep-state-windows-arm64.zip").is_err());
    }

    /// SHA256SUMS 的两种常见写法（两个空格 / `*` 二进制标记）都要认，其它文件的行不许串。
    #[test]
    fn reads_the_hash_for_exactly_the_named_file() {
        let sums = "aaaa  other.zip\n\
                    0123456789abcdef0123456789abcdef0123456789abcdef0123456789ABCDEF  ccodex-sleep-state-windows-amd64.zip\n\
                    fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210 *ccodex-sleep-state-windows-arm64.zip\n";
        assert_eq!(
            expected_sha(sums, "ccodex-sleep-state-windows-amd64.zip").as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert_eq!(
            expected_sha(sums, "ccodex-sleep-state-windows-arm64.zip").as_deref(),
            Some("fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210")
        );
        // `aaaa` 不是 64 位十六进制，不认。
        assert!(expected_sha(sums, "other.zip").is_none());
        assert!(expected_sha(sums, "missing.zip").is_none());
    }

    /// 解压后的 exe 可能在顶级目录里再下一层 —— 往下找两层。
    #[test]
    fn finds_the_exe_nested_in_the_zip_top_folder() {
        let root = std::env::temp_dir().join(format!("qb-egress-find-{}", crate::config_io::id()));
        let inner = root.join("ccodex-sleep-state-windows-amd64");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join(EXE_NAME), "MZ").unwrap();
        assert_eq!(find_exe(&root), Some(inner.join(EXE_NAME)));
        std::fs::remove_dir_all(root).unwrap();
    }

    /// 面板打开的是它自己的 /admin/，端口跟 [`PORT`] 是同一个数 —— 漂了就打开一个没人听的地址。
    #[test]
    fn admin_url_and_port_agree() {
        assert!(ADMIN_URL.contains(&PORT.to_string()));
        assert!(ADMIN_URL.starts_with("http://127.0.0.1:"));
    }

    /// 指定路径只认 exe 本身 —— 指到目录、start.cmd 或不存在的文件都拒绝，
    /// 启动时才发现就晚了。只验证判定，不落盘到真实状态目录。
    #[test]
    fn only_the_real_executable_is_accepted_as_a_configured_path() {
        let dir = std::env::temp_dir().join(format!("qb-egress-{}", crate::config_io::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wrong = dir.join("start.cmd");
        std::fs::write(&wrong, "@echo off").unwrap();
        let right = dir.join(EXE_NAME);
        std::fs::write(&right, "MZ").unwrap();
        assert!(validate_configured(&wrong.display().to_string()).is_err());
        assert!(validate_configured(&dir.display().to_string()).is_err());
        assert!(validate_configured(&dir.join("missing.exe").display().to_string()).is_err());
        assert!(validate_configured(&right.display().to_string()).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// 发布包解压后 exe 多半在「解压目录\顶级目录\」再下一层 —— 默认目录要往下看一层。
    #[test]
    fn the_exe_nested_one_level_under_the_default_dir_is_found() {
        let base = std::env::temp_dir().join(format!("qb-egress-nest-{}", crate::config_io::id()));
        let nested = base.join("ccodex-sleep-state-v0.1.0-windows-amd64");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join(EXE_NAME), "MZ").unwrap();
        let list = candidates_under(&base);
        // 第一个永远是「就在目录里」那一档，找不到才轮到子目录。
        assert_eq!(list[0], base.join(EXE_NAME));
        assert!(list.iter().any(|p| p == &nested.join(EXE_NAME)));
        assert!(list.iter().any(|p| p.is_file()));
        std::fs::remove_dir_all(base).unwrap();
    }

    /// 候选顺序：使用者指定的排最前，默认解压位置兜底。
    #[test]
    fn a_configured_path_comes_before_the_default_locations() {
        let cfg = EgressConfig {
            exe: Some(r"D:\somewhere\ccodex-sleep-state.exe".into()),
        };
        let list = candidates(&cfg);
        assert_eq!(
            list[0],
            PathBuf::from(r"D:\somewhere\ccodex-sleep-state.exe")
        );
        assert!(list.iter().skip(1).all(|p| p.ends_with(EXE_NAME)));
    }
}
