//! 酒馆插件：启动真正的 SillyTavern + ClaudeTavernBridge。
//!
//! 逻辑移植自 `start-claude-pro-rp.ps1`。**下面这些是移植时最容易丢的，
//! 每一条都有出处，动手前先读**：
//!
//!   - PID 文件必须比对命令行，不匹配就报错，**不覆盖也不杀**。
//!     光看「PID 存在」会误伤到复用了同一个 PID 的无关进程。
//!   - 端口被别人占 → **报错退出，绝不停止无关进程**。
//!     启动器没有资格替用户决定关掉他正在用的东西。
//!   - 就绪判定必须打 `/health`，不能只看进程活着。
//!     bridge 起来到能服务之间有几秒，这段时间里酒馆连过去会失败。
//!   - **无论是不是复用现有服务都要打开浏览器页。**
//!     跳过这一步会让成功的启动和崩溃看起来一模一样（控制台窗口直接关掉，
//!     什么可见的事都没发生）。
//!   - 定位官方 claude.exe 时排除 `claude-code-cli\` 下的 vendored 副本。
//!   - 数据目录的 ACL 要修，否则 Python 报 WinError 5。

use crate::error::{GateError, Result};
use crate::plugins::{tavern_locate, DependencyCheck, PluginState, PluginStatus};
use crate::sink::ProgressSink;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PLUGIN_ID: &str = "sillytavern";
pub const PLUGIN_NAME: &str = "酒馆 SillyTavern";

const BRIDGE_READY_ATTEMPTS: u32 = 40; // 40 × 500ms = 20 秒

/// 等酒馆应答 HTTP 的上限：360 × 500ms = **180 秒**。
///
/// # 为什么从 60 秒改成 180 秒
///
/// 酒馆的启动脚本（SillyTavern 自带的 `Start.bat`）**每次都先跑一遍
/// `npm install`** 校验依赖，装完才 `node server.js`。所以「启动酒馆」这件事
/// 从来就不是起一个进程那么快：
///
/// * 2026-09-21 本机实测，依赖全是热的、npm 只花 2 秒的情况下，从起进程到
///   `GET /` 回 200 + `text/html` 用了 **47.2 秒**；
/// * 冷启动（当天第一次、npm 要联网、杀软在扫 449 个包）远不止这个数。
///
/// 60 秒的窗口正好卡在这条线上：审计日志里 2026-09-21 02:43:49 起、02:44:57 收 ——
/// 68 秒，就是「等满 60 秒 + 回滚」。而回滚会**把那份正在启动的酒馆杀掉**，
/// 于是使用者看到的是「GPT 酒馆启动失败」，重试一次还是失败，
/// 因为每次都在它起来之前把它掐了。
///
/// ⚠ 这个数只是上限，不是等待时长：起来了就立刻返回。调大它不会让正常启动变慢。
const ST_READY_ATTEMPTS: u32 = 360;
const POLL_MS: u64 = 500;
/// 等待期间每隔多少次轮询往界面上报一句「已等 N 秒」。8 × 500ms = 4 秒。
const ST_TICK_EVERY: u32 = 8;

/// 酒馆的三个后端。一份酒馆、三条桥：Claude 走使用者自己的 `bridge.py`，
/// GPT 走面板内置的桥接（`qb-app::gpt_bridge`，每个请求驱动一次官方 `codex exec`），
/// Gemini 走面板内置的另一条（`qb-app::gemini_bridge`，每个请求驱动一次官方 Gemini CLI 的
/// 无交互模式；0.26.0，反重力本体没有公开的无交互 CLI，见 `qb-accounts::gemini`）。
/// 三条桥可以同时开，酒馆里各配一个「Custom」连接档。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum TavernBackend {
    Claude,
    Gpt,
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct TavernConfig {
    pub bridge_root: PathBuf,
    pub sillytavern_root: PathBuf,
    pub st_launcher: PathBuf,
    pub bridge_port: u16,
    pub st_port: u16,
    /// 内置 GPT 桥接监听的端口（0.25.0）。旧配置文件里没有这几项，按默认值读。
    #[serde(default = "default_gpt_port")]
    pub gpt_bridge_port: u16,
    /// 交给 `codex exec -m` 的模型；空 = 不传，用 Codex 自己的默认模型。
    #[serde(default)]
    pub gpt_model: String,
    /// 追加到酒馆角色卡之前的 GPT 系统提示词；空 = 只使用请求里的角色卡。
    #[serde(default)]
    pub gpt_system_prompt: String,
    /// 推理强度（`model_reasoning_effort`）：`low` / `medium` / `high`；空 = 不传。
    #[serde(default = "default_gpt_effort")]
    pub gpt_effort: String,
    /// 要不要把酒馆对话记进槽位的会话目录（`home\sessions`，用量卡会数到它）。
    /// **默认不记**（`--ephemeral`）：角色扮演对话不该悄悄落进 Codex 的会话历史。
    #[serde(default)]
    pub gpt_persist_sessions: bool,
    /// 内置 Gemini 桥接监听的端口（0.26.0）。
    #[serde(default = "default_gemini_port")]
    pub gemini_bridge_port: u16,
    /// 交给 Gemini CLI `-m` 的模型；空 = 不传，用 CLI 自己的默认模型。
    #[serde(default)]
    pub gemini_model: String,
    /// 追加到酒馆角色卡之前的 Gemini 系统提示词；空 = 只使用请求里的角色卡。
    #[serde(default)]
    pub gemini_system_prompt: String,
}

fn default_gpt_port() -> u16 {
    5002
}
fn default_gemini_port() -> u16 {
    5003
}
fn default_gpt_effort() -> String {
    "medium".into()
}

impl Default for TavernConfig {
    fn default() -> Self {
        // 三个路径**故意留空**。
        //
        // 这里原来写死的是作者本机那份部署（`D:\tools\…`）。在作者机器上没出过事，
        // 在别人机器上则是两个问题：一是面板一上来就报一串跟他毫无关系的盘符路径，
        // 二是把作者的目录布局随源码发了出去。
        //
        // 留空之后，`status()` 里的存在性检查一律不过，插件如实停在
        // `PluginState::Missing`（「依赖不齐，展开看缺哪一项」）——
        // 这正是「还没配」的准确描述。用户在设置里填完自己的路径就好。
        // 端口保留默认值：5001 / 5002 / 8000 是两条桥与酒馆各自的约定端口，与机器无关。
        Self {
            bridge_root: PathBuf::new(),
            sillytavern_root: PathBuf::new(),
            st_launcher: PathBuf::new(),
            bridge_port: 5001,
            st_port: 8000,
            gpt_bridge_port: default_gpt_port(),
            gpt_model: String::new(),
            gpt_system_prompt: String::new(),
            gpt_effort: default_gpt_effort(),
            gpt_persist_sessions: false,
            gemini_bridge_port: default_gemini_port(),
            gemini_model: String::new(),
            gemini_system_prompt: String::new(),
        }
    }
}

/// 路径里还空着的那几项。**「还没配」和「配错了」必须分开。**
///
/// 空 `PathBuf` 拼 `bridge.py` 得到的是个相对文件名，于是
/// `start()` 的 `NotFound` 报出来是「路径不存在: bridge.py」——
/// 这句话既没说是哪个 bridge.py，也没说该去哪填，使用者唯一能做的是猜。
/// （默认值留空是对的，见 `TavernConfig::default`；错的是留空之后
/// 走的还是「路径不存在」那条错误。）
///
/// 两种情况的下一步完全相反：**没配**要去设置里填自己那份部署的位置，
/// **配错**要拿报出来的绝对路径去对照盘上的真实位置改。合成一句，
/// 谁都查不出来 —— 跟 `IpUnknown` / `IpNotAllowed` 不许合并是同一条道理。
///
/// GPT 后端用不着 `bridge.py`（桥接是面板内置的），只要酒馆那两个路径。
pub fn unset_paths(cfg: &TavernConfig, backend: TavernBackend) -> Vec<&'static str> {
    let claude_bridge = (&cfg.bridge_root, "桥接项目目录");
    let tavern = [
        (&cfg.sillytavern_root, "SillyTavern 目录"),
        (&cfg.st_launcher, "酒馆启动脚本"),
    ];
    let all: Vec<(&PathBuf, &'static str)> = match backend {
        TavernBackend::Claude => std::iter::once(claude_bridge).chain(tavern).collect(),
        // GPT / Gemini 的桥接都是面板内置的，只要酒馆那两个路径。
        TavernBackend::Gpt | TavernBackend::Gemini => tavern.into_iter().collect(),
    };
    all.into_iter()
        .filter(|(p, _)| p.as_os_str().is_empty())
        .map(|(_, label)| label)
        .collect()
}

/// 没配时给出的那句话。**面板不分发 SillyTavern，也不分发 bridge.py**
/// （见 DISCLAIMER 第 296 行），所以这里只能指路，不能替他装。
fn setup_hint(missing: &[&str]) -> String {
    format!(
        "还没配路径（{}）。点「自动定位」让面板在本机找，或自己填。",
        missing.join("、")
    )
}

// ------------------------------------------------------------------ 定位

/// 枚举本机所有命令行里带 `bridge.py` 的进程。
///
/// 这是 [`tavern_locate::TavernEvidence::Running`] 的来源，也是整条定位链里
/// **唯一零扫描**的一档：跑过一次桥接的人，不用翻盘就能拿到准确路径。
///
/// 只读一次 `Win32_Process`，不动任何进程 —— 收进程是 `killswitch` 的事，
/// 而且那边要的是双重证据，跟这里的用途完全不同。
#[cfg(windows)]
async fn running_bridge_cmdlines() -> Vec<String> {
    let out = crate::process::powershell_tokio(
        "@(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | \
         Where-Object { $_.CommandLine -like '*bridge.py*' } | \
         ForEach-Object { $_.CommandLine }) -join \"`n\"",
    )
    .output()
    .await;
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        // 枚举不到就退回扫盘那一档，不是错误。
        Err(_) => Vec::new(),
    }
}

#[cfg(not(windows))]
async fn running_bridge_cmdlines() -> Vec<String> {
    Vec::new()
}

/// `bridge.pid` 指的那个进程的命令行（进程已经没了就是 `None`）。
#[cfg(windows)]
async fn pidfile_cmdline() -> Option<String> {
    let text = std::fs::read_to_string(bridge_data_dir().join("bridge.pid")).ok()?;
    let pid: u32 = text.trim().parse().ok()?;
    let out = crate::process::powershell_tokio(&format!(
        "(Get-CimInstance Win32_Process -Filter 'ProcessId={pid}' \
         -ErrorAction SilentlyContinue).CommandLine"
    ))
    .output()
    .await
    .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(not(windows))]
async fn pidfile_cmdline() -> Option<String> {
    None
}

/// 在本机找酒馆与桥接。**只读，不写配置** —— 采用哪一条由使用者点。
///
/// `deep` 是使用者显式选的第二档：默认档几秒回来，深扫最长 90 秒。
pub async fn locate(deep: bool) -> Result<tavern_locate::TavernSurvey> {
    let cfg = load_config();
    let mut roots = tavern_locate::Roots::current();
    roots.running_cmdlines = running_bridge_cmdlines().await;
    roots.pidfile_cmdline = pidfile_cmdline().await;
    roots.configured = vec![
        cfg.bridge_root.clone(),
        cfg.sillytavern_root.clone(),
        cfg.st_launcher.clone(),
    ];
    let budget = if deep {
        tavern_locate::Budget::deep()
    } else {
        tavern_locate::Budget::quick()
    };
    // 扫盘是同步阻塞 I/O，可能跑满预算 —— 放到阻塞线程池里，
    // 否则它会把整个 tokio 运行时卡住，连界面的心跳都停。
    tokio::task::spawn_blocking(move || tavern_locate::locate(&roots, &budget))
        .await
        .map_err(|e| GateError::Other(format!("定位任务未能完成：{e}")))
}

fn config_path() -> PathBuf {
    crate::paths::state_dir()
        .join("plugins")
        .join("sillytavern.json")
}

pub fn load_config() -> TavernConfig {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn load_config_checked() -> Result<TavernConfig> {
    match crate::config_io::read_optional(&config_path())? {
        Some(b) => Ok(serde_json::from_slice(&b)?),
        None => Ok(TavernConfig::default()),
    }
}
/// 存之前的校验。**纯函数** —— `save_config` 要读磁盘，而单测不许碰真实的运行期状态，
/// 判定留在它里面就等于没有测试。
///
/// ⛔ **四个端口一个都不能漏。** 0.26.0 加了 `gemini_bridge_port` 却没加进这张表，
/// 于是它能被存成 `0`、或者跟另外三个撞号 —— 撞号的症状是「Gemini 桥接起不来」，
/// 而没有任何地方说得清为什么（配置是它自己存坏的）。错误文案也要一起说全：
/// 少说一个，使用者就会对着一句不完整的话去改另外三个。
pub fn validate(cfg: &TavernConfig) -> Result<()> {
    let ports = [
        cfg.bridge_port,
        cfg.gpt_bridge_port,
        cfg.gemini_bridge_port,
        cfg.st_port,
    ];
    let distinct = ports.iter().collect::<std::collections::HashSet<_>>().len();
    if ports.contains(&0) || distinct != ports.len() {
        return Err(GateError::Other(
            "酒馆、Claude 桥接、GPT 桥接、Gemini 桥接需要四个互不相同的有效端口".into(),
        ));
    }
    if !["", "low", "medium", "high"].contains(&cfg.gpt_effort.trim()) {
        return Err(GateError::Other(
            "GPT 推理强度只能是 low / medium / high（或留空用默认）".into(),
        ));
    }
    Ok(())
}

pub fn save_config(cfg: &TavernConfig) -> Result<()> {
    let expected = crate::config_io::read_optional(&config_path())?;
    if let Some(b) = &expected {
        let _: TavernConfig = serde_json::from_slice(b)?;
    }
    validate(cfg)?;
    crate::config_io::commit(vec![crate::config_io::Edit {
        path: config_path(),
        expected,
        body: Some(serde_json::to_vec_pretty(cfg)?),
    }])?;
    Ok(())
}

/// 见 [`crate::paths::tavern_bridge_dir`]。这里只转调，
/// 路径的唯一定义在 `paths` 里。
pub fn bridge_data_dir() -> PathBuf {
    crate::paths::tavern_bridge_dir()
}

// ------------------------------------------------------------------ 端口

/// 端口是否被占。用试绑定判断，不需要额外依赖。
///
/// 注意语义：这里只回答「占没占」，**不回答「被谁占」**。
/// 判断「是不是我们自己的 bridge」靠的是 PID 文件比对，在调用方做。
fn port_in_use(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_err()
}

// ------------------------------------------------------------------ 定位

/// 定位官方 claude.exe（给桥接的 `--claude` 用，所以必须是 exe）。
///
/// v0.8.0 起跟检测、启动、上锁用同一张表（`install::inventory`）。
/// 原来这里自己拼一份候选，**而且把 `claude-path.txt` 里记住的路径排第一** ——
/// 记住的是哪份就一直用哪份，装了新的也不换。现在记住的路径只当最后的兜底：
/// 表里一份都找不到时，才看使用者是不是手动指过一个表外的位置。
///
/// 排除 `claude-code-cli\` 下的副本 —— 那是项目自带的 vendored 版本，
/// 不是官方二进制，桥接明确拒绝它（inventory 里也排除了）。
pub fn find_official_claude() -> Result<PathBuf> {
    let saved = bridge_data_dir().join("claude-path.txt");
    let from_inventory =
        crate::install::inventory::preferred_exe(&crate::install::inventory::Roots::current());

    let chosen = from_inventory.or_else(|| {
        let t = std::fs::read_to_string(&saved).ok()?;
        let c = PathBuf::from(t.trim());
        let ok = !c.as_os_str().is_empty()
            && c.is_file()
            && c.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            && !c
                .to_string_lossy()
                .to_lowercase()
                .contains(r"\claude-code-cli\")
            && !crate::install::inventory::is_desktop_runtime_copy(&c);
        ok.then_some(c)
    });

    match chosen {
        Some(c) => Ok(c),
        None => Err(GateError::Other(
            "找不到官方 Claude Code 程序，请先在「环境与安装」里装好。".into(),
        )),
    }
}

fn find_python() -> Result<PathBuf> {
    for exe in ["python.exe", "python3.exe", "py.exe"] {
        if let Ok(out) = crate::process::hidden_std(std::process::Command::new("where"))
            .arg(exe)
            .output()
        {
            if out.status.success() {
                if let Some(first) = String::from_utf8_lossy(&out.stdout).lines().next() {
                    let p = PathBuf::from(first.trim());
                    if p.is_file() {
                        return Ok(p);
                    }
                }
            }
        }
    }
    Err(GateError::Other(
        "找不到 Python 3，请先安装并确认它在 PATH 里。".into(),
    ))
}

// ------------------------------------------------------------------ 检测

/// 定位官方 Codex CLI（内置 GPT 桥接要拿它跑 `codex exec`）。
///
/// 用检测那张表（`install::detect::codex_candidates`）。**先要 exe，再要 .cmd**：
/// `.cmd` 得经 `cmd /c` 起，而 `codex exec -c key="value"` 那种带引号的参数过 cmd
/// 那一层会被吃掉引号 —— 表里 npm 包内部那份原生 exe 就是为此收进来的。
/// **不在这里起任何进程** —— 版本号、登录态那些留给桥接启动时报。
pub fn find_official_codex() -> Result<PathBuf> {
    let candidates: Vec<PathBuf> = crate::install::detect::codex_candidates()
        .into_iter()
        .filter(|p| p.is_file())
        .collect();
    let is_exe = |p: &PathBuf| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    candidates
        .iter()
        .find(|p| is_exe(p))
        .or_else(|| candidates.first())
        .cloned()
        .ok_or_else(|| {
            GateError::NotFound(
                "官方 Codex CLI（codex.exe / codex.cmd）。到「软件」页安装，或用 npm 装 @openai/codex".into(),
            )
        })
}

pub fn status() -> PluginStatus {
    status_with(None, None)
}

/// 同 [`status`]，外加内置 GPT 桥接的真实状态。
///
/// 桥接活在面板进程里（`qb-app::gpt_bridge`），这一层（L2）看不见它；命令层拿得到
/// 就传进来，传 `None` 时退回「端口占没占」这个近似值 —— 那只回答「有人在听」，
/// 不回答「是不是我们」。
pub fn status_with(gpt_running: Option<bool>, gemini_running: Option<bool>) -> PluginStatus {
    let cfg = load_config();
    let dd = bridge_data_dir();
    let mut checks = Vec::new();

    // 路径没填时**不能把空串当路径显示** —— 那一行在界面上是一片空白，
    // 看起来像「检测挂了」，而它其实是「你还没填」。
    let shown = |p: &Path| {
        if p.as_os_str().is_empty() {
            "未配置".to_string()
        } else {
            p.display().to_string()
        }
    };

    let bridge_py = cfg.bridge_root.join("bridge.py");
    checks.push(DependencyCheck::new(
        "桥接项目",
        !cfg.bridge_root.as_os_str().is_empty() && bridge_py.is_file(),
        shown(&cfg.bridge_root),
    ));

    let st_server = cfg.sillytavern_root.join("server.js");
    checks.push(DependencyCheck::new(
        "SillyTavern",
        !cfg.sillytavern_root.as_os_str().is_empty() && st_server.is_file(),
        shown(&cfg.sillytavern_root),
    ));

    checks.push(DependencyCheck::new(
        "酒馆启动脚本",
        cfg.st_launcher.is_file(),
        shown(&cfg.st_launcher),
    ));

    let py = find_python();
    checks.push(DependencyCheck::new(
        "Python 3",
        py.is_ok(),
        match &py {
            Ok(p) => p.display().to_string(),
            Err(e) => e.to_string(),
        },
    ));

    let claude = find_official_claude();
    checks.push(DependencyCheck::new(
        "官方 Claude Code",
        claude.is_ok(),
        match &claude {
            Ok(p) => p.display().to_string(),
            Err(e) => e.to_string(),
        },
    ));

    // 到这里为止是 Claude 后端的依赖项。Claude 那条桥缺东西不该把 GPT 那条也判成
    // 「依赖不齐」（反之亦然），所以两组分开数。
    let claude_deps_len = checks.len();

    // GPT 后端的依赖只有一样：官方 Codex CLI。桥接是内置的，酒馆两条路径共用。
    let codex = find_official_codex();
    checks.push(DependencyCheck::new(
        "官方 Codex CLI（GPT 桥接用）",
        codex.is_ok(),
        match &codex {
            Ok(p) => p.display().to_string(),
            Err(e) => e.to_string(),
        },
    ));
    let gpt_deps_ok = codex.is_ok();

    // Gemini 后端的依赖：官方 Gemini CLI + node。
    let gemini = find_official_gemini();
    checks.push(DependencyCheck::new(
        "官方 Gemini CLI（Gemini 桥接用）",
        gemini.is_ok(),
        match &gemini {
            Ok(g) => format!("{} · {}", g.node.display(), g.entry.display()),
            Err(e) => e.to_string(),
        },
    ));
    let gemini_deps_ok = gemini.is_ok();

    // 后面几条是端口现况（永远 ok，只是给人看的）。
    // 原来这行写的是 `take(4)` —— 加一条依赖检查就会有一项不参与判定，
    // 而且毫无症状。改成记住条数，加多少条都不会漏。
    let bridge_up = port_in_use(cfg.bridge_port);
    let gpt_up = gpt_running.unwrap_or_else(|| port_in_use(cfg.gpt_bridge_port));
    let gemini_up = gemini_running.unwrap_or_else(|| port_in_use(cfg.gemini_bridge_port));
    let st_up = port_in_use(cfg.st_port);
    checks.push(DependencyCheck::new(
        format!("Claude 桥接端口 {}", cfg.bridge_port),
        true,
        if bridge_up { "在监听" } else { "空闲" },
    ));
    checks.push(DependencyCheck::new(
        format!("GPT 桥接端口 {}", cfg.gpt_bridge_port),
        true,
        match (gpt_running, gpt_up) {
            (Some(true), _) => "面板内置桥接在监听",
            (None, true) => "在监听",
            _ => "空闲",
        },
    ));
    checks.push(DependencyCheck::new(
        format!("Gemini 桥接端口 {}", cfg.gemini_bridge_port),
        true,
        match (gemini_running, gemini_up) {
            (Some(true), _) => "面板内置桥接在监听",
            (None, true) => "在监听",
            _ => "空闲",
        },
    ));
    checks.push(DependencyCheck::new(
        format!("酒馆端口 {}", cfg.st_port),
        true,
        if st_up { "在监听" } else { "空闲" },
    ));

    let tavern_deps_ok = checks
        .iter()
        .take(claude_deps_len)
        .filter(|c| c.label == "SillyTavern" || c.label == "酒馆启动脚本")
        .all(|c| c.ok);
    let claude_deps_ok = checks.iter().take(claude_deps_len).all(|c| c.ok);
    let unset_claude = unset_paths(&cfg, TavernBackend::Claude);
    let unset_gpt = unset_paths(&cfg, TavernBackend::Gpt);
    let claude_owned = bridge_up && owned_running("bridge").unwrap_or(false);
    let st_owned = st_up && owned_running("sillytavern").unwrap_or(false);
    let any_builtin_up = gpt_up || gemini_up;
    let (state, detail) = if !unset_gpt.is_empty() {
        // 酒馆本身的路径都没填：两条桥谁也起不了。「还没配」排在「依赖不齐」前面：
        // 路径空着时后者那句「展开看缺哪一项」会把人引去查 Python 和 Claude，
        // 而真正缺的是他自己还没填的路径。
        (PluginState::Missing, setup_hint(&unset_gpt))
    } else if !tavern_deps_ok || (!claude_deps_ok && !gpt_deps_ok && !gemini_deps_ok) {
        (PluginState::Missing, "依赖不齐，展开看缺哪一项".to_string())
    } else if st_owned && (claude_owned || any_builtin_up) {
        let mut backends = Vec::new();
        if claude_owned {
            backends.push("Claude 桥接");
        }
        if gpt_up {
            backends.push("GPT 桥接");
        }
        if gemini_up {
            backends.push("Gemini 桥接");
        }
        (
            PluginState::Running,
            format!("酒馆正在运行，后端：{}", backends.join("与")),
        )
    } else if bridge_up || any_builtin_up || st_up {
        (
            PluginState::Broken,
            format!(
                "只起来了一半：Claude 桥接 {}，GPT 桥接 {}，Gemini 桥接 {}，酒馆 {}",
                if bridge_up { "在" } else { "不在" },
                if gpt_up { "在" } else { "不在" },
                if gemini_up { "在" } else { "不在" },
                if st_up { "在" } else { "不在" }
            ),
        )
    } else {
        let mut can = Vec::new();
        if unset_claude.is_empty() && claude_deps_ok {
            can.push("Claude");
        }
        if gpt_deps_ok {
            can.push("GPT");
        }
        if gemini_deps_ok {
            can.push("Gemini");
        }
        let sides = if can.is_empty() {
            "三条桥都还没齐".to_string()
        } else if can.len() == 3 {
            "Claude、GPT、Gemini 三条桥都能起".to_string()
        } else {
            format!("能起 {} 桥接", can.join(" / "))
        };
        (
            PluginState::Ready,
            format!("就绪 · {sides} · 数据目录 {}", dd.display()),
        )
    };

    PluginStatus {
        id: PLUGIN_ID,
        name: PLUGIN_NAME,
        state,
        detail,
        checks,
    }
}

/// 官方 Gemini CLI：`(node.exe, 入口 index.js)`。给内置 Gemini 桥接用。
///
/// 位置表在 `install::detect::gemini_cli_candidates`。**不在这里起任何进程。**
pub fn find_official_gemini() -> Result<GeminiCli> {
    let entry = crate::install::detect::gemini_cli_candidates()
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| {
            GateError::NotFound(format!(
                "官方 Gemini CLI（@google/gemini-cli）。到「软件」页用 npm 装，\
                 或手动 npm i -g @google/gemini-cli。{}",
                crate::install::detect::gemini_cli_searched()
            ))
        })?;
    let node = crate::install::detect::node_exe().ok_or_else(|| {
        GateError::NotFound(
            "node.exe。Gemini CLI 是 Node 包，请先装 Node.js 并确认它在 PATH 里".into(),
        )
    })?;
    Ok(GeminiCli { node, entry })
}

/// 起 Gemini CLI 要的两样。
#[derive(Debug, Clone)]
pub struct GeminiCli {
    pub node: PathBuf,
    pub entry: PathBuf,
}

// ------------------------------------------------------------------ PID

/// 读 PID 文件并**用命令行验明正身**。
///
/// 返回 `Ok(Some(pid))` 表示确认是我们自己的 bridge，可以复用；
/// `Ok(None)` 表示没有有效记录；
/// `Err` 表示 PID 文件指向了别的进程 —— 这种情况**必须报错**，
/// 既不能覆盖也不能杀，那是别人的进程。
#[cfg(windows)]
async fn existing_bridge_pid(cfg: &TavernConfig) -> Result<Option<u32>> {
    let pid_path = bridge_data_dir().join("bridge.pid");
    let Ok(text) = std::fs::read_to_string(&pid_path) else {
        return Ok(None);
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        let _ = std::fs::remove_file(&pid_path);
        return Ok(None);
    };

    let out = crate::process::powershell_tokio(&format!(
        "(Get-CimInstance Win32_Process -Filter 'ProcessId={pid}' \
         -ErrorAction Stop).CommandLine"
    ))
    .output()
    .await?;
    if !out.status.success() {
        return Err(GateError::Other(
            "无法枚举旧桥接进程，未覆盖 PID 记录".into(),
        ));
    }
    let cmdline = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();

    if cmdline.is_empty() {
        // 进程没了，清掉陈旧的 PID 文件。
        let _ = std::fs::remove_file(&pid_path);
        return Ok(None);
    }

    let want_script = cfg
        .bridge_root
        .join("bridge.py")
        .to_string_lossy()
        .to_lowercase();
    let want_dir = bridge_data_dir().to_string_lossy().to_lowercase();

    if cmdline.contains(&want_script) && cmdline.contains(&want_dir) {
        Ok(Some(pid))
    } else {
        Err(GateError::Other(format!(
            "桥接 PID 文件指向了其他进程（PID {pid}），为安全起见没有停止或覆盖它。"
        )))
    }
}

#[cfg(not(windows))]
async fn existing_bridge_pid(_cfg: &TavernConfig) -> Result<Option<u32>> {
    Ok(None)
}

// ------------------------------------------------------------------ 启动

/// Creating a private application directory must not reset an existing user's ACLs.
fn fix_data_dir_acl(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let probe = dir.join(format!(".qb-write-check-{}", crate::config_io::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|e| GateError::Other(format!("桥接数据目录无法写入：{e}；请检查此目录权限")))?;
    std::fs::remove_file(probe)?;
    Ok(())
}

async fn bridge_healthy(port: u16) -> bool {
    let token_path = bridge_data_dir().join("bridge-token.txt");
    let Ok(token) = std::fs::read_to_string(&token_path) else {
        return false;
    };
    let Ok(c) = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    let url = format!("http://127.0.0.1:{port}/health");
    match c.get(&url).bearer_auth(token.trim()).send().await {
        Ok(r) => match r.json::<serde_json::Value>().await {
            Ok(v) => v.get("status").and_then(|s| s.as_str()) == Some("ok"),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

/// 启动酒馆全链路。
///
/// 调用方（lib.rs 的命令）负责在此之前完成 IP 门禁放行，
/// 并在成功之后起 `WatchMode::Cli` 看门狗。
static PROGRAMS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::BTreeMap<String, crate::sessions::OwnedProgram>>,
> = std::sync::OnceLock::new();
fn programs(
) -> &'static std::sync::Mutex<std::collections::BTreeMap<String, crate::sessions::OwnedProgram>> {
    PROGRAMS.get_or_init(Default::default)
}
fn program_path(name: &str) -> PathBuf {
    crate::paths::state_dir()
        .join("plugins")
        .join(format!("{name}-process.json"))
}
fn owned_running(name: &str) -> Result<bool> {
    let mut map = programs().lock().unwrap();
    if !map.contains_key(name) {
        if let Some(bytes) = crate::config_io::read_optional(&program_path(name))? {
            let record = serde_json::from_slice(&bytes)?;
            if let Some(program) = crate::sessions::OwnedProgram::reopen(record)? {
                map.insert(name.into(), program);
            }
        }
    }
    map.get(name).map(|p| p.running()).unwrap_or(Ok(false))
}
fn start_owned(
    name: &str,
    exe: &Path,
    args: Vec<String>,
    cwd: &Path,
    env: Vec<(String, String)>,
) -> Result<()> {
    let program = crate::sessions::OwnedProgram::launch(exe, &args, cwd, env)?;
    if let Err(error) = crate::config_io::replace(
        &program_path(name),
        Some(&serde_json::to_vec(&program.record)?),
    ) {
        let _ = program.stop();
        return Err(error);
    }
    if name == "bridge" {
        if let Err(error) = crate::config_io::replace(
            &bridge_data_dir().join("bridge.pid"),
            Some(program.record.pid.to_string().as_bytes()),
        ) {
            let _ = program.stop();
            let _ = crate::config_io::replace(&program_path(name), None);
            return Err(error);
        }
    }
    programs().lock().unwrap().insert(name.into(), program);
    Ok(())
}
fn stop_owned(name: &str) -> Result<()> {
    let _ = owned_running(name)?;
    let mut map = programs().lock().unwrap();
    if let Some(p) = map.get(name) {
        p.stop()?;
    } else {
        crate::config_io::replace(&program_path(name), None)?;
        return Ok(());
    }
    map.remove(name);
    crate::config_io::replace(&program_path(name), None)?;
    if name == "bridge" {
        crate::config_io::replace(&bridge_data_dir().join("bridge.pid"), None)?;
    }
    Ok(())
}
struct StartAttempt {
    created: Vec<String>,
    finished: bool,
}
impl Drop for StartAttempt {
    fn drop(&mut self) {
        if !self.finished {
            for name in self.created.iter().rev() {
                if let Err(e) = stop_owned(name) {
                    crate::audit::write(&format!("启动失败后的进程清理未完成：{e}"));
                }
            }
        }
    }
}
/// 酒馆起来了没。
///
/// ⚠ **必须 `.no_proxy()`** —— 跟 [`bridge_healthy`] 同一个理由，这里原来漏了。
/// reqwest 默认会读环境里的 `HTTP_PROXY` / 系统代理设置，而这个面板的使用者
/// 十有八九正开着某个代理工具（门禁那一套就是围着出口 IP 转的）。
/// 一旦环境里有 `HTTP_PROXY`，这条对 `127.0.0.1` 的健康检查就会被送去代理，
/// 于是**酒馆明明起来了，面板永远等不到它**，最后报「未通过就绪检查」。
/// 本机当前 `ProxyEnable=0`、没有环境变量，所以没撞上；别人的机器不一定。
async fn tavern_healthy(port: u16) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return false;
    };
    let Ok(response) = client.get(format!("http://127.0.0.1:{port}/")).send().await else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/html"))
}
/// 酒馆页面的地址。`start` 把它交回前端，由前端的 `openUrl` 打开 ——
/// 所以它必须落在 `src-tauri/capabilities/default.json` 给 opener 配的
/// 网址范围里，`src-tauri/tests/capabilities.rs` 拿这个函数去核。
pub fn page_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}
/// 起 Claude 桥接 + 酒馆（0.20 起的老路，行为不变）。
///
/// 阶段 1–3 起 `bridge.py`，4–5 起酒馆 —— 后两段跟 GPT 那条路共用
/// （[`start_tavern_only`]），所以拆成两个函数。
pub async fn start(rep: &dyn ProgressSink) -> Result<String> {
    let cfg = load_config_checked()?;
    // 路径没填就在这里停 —— **必须在起任何进程、甚至查 Python 之前**。
    // 往下走的话，第一个撞上的是 `bridge_py.is_file()`，报出来是
    // 「路径不存在: bridge.py」（空目录拼出来的相对文件名），
    // 使用者看不出这是「还没配」而不是「装坏了」。
    let unset = unset_paths(&cfg, TavernBackend::Claude);
    if !unset.is_empty() {
        return Err(GateError::Other(setup_hint(&unset)));
    }
    let mut attempt = StartAttempt {
        created: vec![],
        finished: false,
    };
    start_claude_bridge(rep, &cfg, &mut attempt).await?;
    let url = start_tavern(rep, &cfg, &mut attempt).await?;
    attempt.finished = true;
    rep.done("酒馆与桥接已就绪");
    Ok(url)
}

/// 只起酒馆（GPT 后端用：桥接活在面板进程里，由 `qb-app::gpt_bridge` 先起好）。
///
/// 酒馆已经在跑就直接回页面地址 —— 两条桥可以共用同一份酒馆。
pub async fn start_tavern_only(rep: &dyn ProgressSink) -> Result<String> {
    let cfg = load_config_checked()?;
    let unset = unset_paths(&cfg, TavernBackend::Gpt);
    if !unset.is_empty() {
        return Err(GateError::Other(setup_hint(&unset)));
    }
    let mut attempt = StartAttempt {
        created: vec![],
        finished: false,
    };
    let url = start_tavern(rep, &cfg, &mut attempt).await?;
    attempt.finished = true;
    rep.done("酒馆已就绪（内置桥接）");
    Ok(url)
}

/// 阶段 1–3：官方 Claude Code + Python + `bridge.py` → 起桥接 → 等 `/health`。
async fn start_claude_bridge(
    rep: &dyn ProgressSink,
    cfg: &TavernConfig,
    attempt: &mut StartAttempt,
) -> Result<()> {
    let dd = bridge_data_dir();
    rep.phase(1, "检查依赖与官方身份");
    let claude = find_official_claude()?;
    if bridge_data_dir().is_dir() {
        crate::config_io::commit(vec![crate::config_io::Edit::text(
            bridge_data_dir().join("claude-path.txt"),
            claude.display().to_string(),
        )?])?;
    }
    let python = find_python()?;
    let bridge_py = cfg.bridge_root.join("bridge.py");
    if !bridge_py.is_file() {
        return Err(GateError::NotFound(bridge_py.display().to_string()));
    }
    let official = crate::accounts::active_slot_dir(&crate::accounts::AccountRoots::current())
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".claude"));
    crate::residue::check_official(crate::domain::Client::ClaudeCode, &official)?;
    fix_data_dir_acl(&dd)?;
    rep.phase(2, "启动 Claude 桥接");
    if !owned_running("bridge")? {
        if existing_bridge_pid(cfg).await?.is_some() {
            if !bridge_healthy(cfg.bridge_port).await {
                return Err(GateError::Other(
                    "已有桥接进程未通过健康检查，请在原启动器中处理".into(),
                ));
            }
        } else {
            if port_in_use(cfg.bridge_port) {
                return Err(GateError::Other(
                    "桥接端口被其他程序占用，未停止任何进程".into(),
                ));
            }
            start_owned(
                "bridge",
                &python,
                vec![
                    "-u".into(),
                    bridge_py.display().to_string(),
                    "--claude".into(),
                    claude.display().to_string(),
                    "--data-dir".into(),
                    dd.display().to_string(),
                    "--port".into(),
                    cfg.bridge_port.to_string(),
                ],
                &cfg.bridge_root,
                vec![
                    ("CLAUDE_CONFIG_DIR".into(), official.display().to_string()),
                    ("DISABLE_AUTOUPDATER".into(), "1".into()),
                ],
            )?;
            attempt.created.push("bridge".into());
        }
    }
    rep.phase(3, "等桥接健康检查通过（最长 20 秒）");
    let mut ready = false;
    for _ in 0..BRIDGE_READY_ATTEMPTS {
        if bridge_healthy(cfg.bridge_port).await {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(POLL_MS)).await;
    }
    if !ready {
        return Err(GateError::Other(
            "桥接未通过健康检查；本次启动的进程将清理".into(),
        ));
    }
    Ok(())
}

/// 阶段 4–5：起酒馆（已经在跑就复用）→ 等 HTTP 就绪 → 回页面地址。
async fn start_tavern(
    rep: &dyn ProgressSink,
    cfg: &TavernConfig,
    attempt: &mut StartAttempt,
) -> Result<String> {
    rep.phase(4, "启动酒馆");
    if !owned_running("sillytavern")? {
        if port_in_use(cfg.st_port) {
            return Err(GateError::Other(
                "酒馆端口被未登记的进程占用，请在原酒馆界面处理后重试；没有停止或冒充该服务".into(),
            ));
        }
        if !cfg.st_launcher.is_file() {
            return Err(GateError::NotFound(cfg.st_launcher.display().to_string()));
        }
        start_owned(
            "sillytavern",
            &cfg.st_launcher,
            vec![],
            cfg.st_launcher.parent().unwrap_or(Path::new(".")),
            vec![],
        )?;
        attempt.created.push("sillytavern".into());
    }
    let limit = ST_READY_ATTEMPTS * POLL_MS as u32 / 1000;
    rep.phase(
        5,
        &format!("等酒馆 HTTP 服务就绪（最长 {limit} 秒；它自己会先跑一遍 npm 校验依赖）"),
    );
    for i in 0..ST_READY_ATTEMPTS {
        if !owned_running("sillytavern")? {
            return Err(GateError::Other(
                "酒馆进程在就绪前退出了。到它自己的窗口看最后几行 —— \
                 多半是依赖没装上或者端口被占，面板没有停止任何进程。"
                    .into(),
            ));
        }
        if tavern_healthy(cfg.st_port).await {
            return Ok(page_url(cfg.st_port));
        }
        // 一声不吭地等三分钟，跟卡死没有区别。每 4 秒把段标题刷成「等了多久」。
        //
        // 用 `phase` 不用 `log`：这一页只渲染段标题，日志行没有地方显示。
        if i > 0 && i % ST_TICK_EVERY == 0 {
            rep.phase(
                5,
                &format!(
                    "等酒馆应答 127.0.0.1:{}：已等 {} 秒 / 最长 {limit} 秒（它自己会先跑一遍 npm 校验依赖，首次启动慢是正常的）",
                    cfg.st_port,
                    i * POLL_MS as u32 / 1000,
                ),
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(POLL_MS)).await;
    }
    // ⛔ 等超了**不等于**它坏了，所以不杀。
    //
    // 原来这里直接落进 `StartAttempt::drop` 的回滚，把这次起的酒馆一并收掉 ——
    // 而它当时很可能只是还在装依赖（见 `ST_READY_ATTEMPTS` 的说明）。
    // 结果是：使用者每点一次「启动」，就把上一次正要起来的酒馆掐掉一次，
    // 永远等不到那一刻。进程是活的就留着，如实说清楚现在是什么情况。
    if owned_running("sillytavern").unwrap_or(false) {
        attempt.created.retain(|n| n != "sillytavern");
        return Err(GateError::Other(format!(
            "等了 {limit} 秒，酒馆还没在 127.0.0.1:{} 上应答。\
             进程还活着，面板没有停它 —— 到它自己的窗口看看进度（首次启动要装依赖，可能更久），\
             起来之后再点一次「启动」就会直接复用它。",
            cfg.st_port
        )));
    }
    Err(GateError::Other(format!(
        "等了 {limit} 秒，酒馆既没应答也不在了；本次新启动的进程将清理。"
    )))
}
pub async fn stop() -> Result<Vec<String>> {
    let mut done = Vec::new();
    let mut failures = Vec::new();
    for name in ["sillytavern", "bridge"] {
        match stop_owned(name) {
            Ok(()) => done.push(format!("{name} 的登记进程已停止")),
            Err(e) => failures.push(e.to_string()),
        }
    }
    if !failures.is_empty() {
        return Err(GateError::Other(failures.join("；")));
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 就绪窗口必须比一次真实冷启动长。
    ///
    /// 2026-09-21 本机实测：依赖全是热的（`npm install` 只花 2 秒）时，从起进程到
    /// `GET /` 回 200 + `text/html` 用了 **47.2 秒**。当时的窗口是 60 秒 ——
    /// 冷启动一超，回滚就把那份正在启动的酒馆杀掉，于是「GPT 酒馆」怎么点都失败
    ///（审计日志 2026-09-21 02:43:49 起、02:44:57 收，正好 68 秒）。
    ///
    /// 调小这个数之前先想清楚：它只是**上限**，起来了就立刻返回，调大不拖慢任何正常启动。
    #[test]
    fn the_readiness_window_outlasts_a_real_cold_start() {
        let secs = ST_READY_ATTEMPTS * POLL_MS as u32 / 1000;
        assert!(
            secs >= 150,
            "酒馆就绪窗口只有 {secs} 秒，实测热启动就要 47 秒，冷启动更久"
        );
        // 等待期间要能周期性上报，否则界面上跟卡死没区别。
        let tick = ST_TICK_EVERY * POLL_MS as u32 / 1000;
        assert!(
            tick > 0 && tick < secs,
            "每 {tick} 秒才上报一次（总窗口 {secs} 秒），界面上看不出还在等"
        );
    }

    /// 默认配置里**不许出现任何一台具体机器的路径**。
    ///
    /// 开源之前这里写死过作者本机的 `D:\tools\…`，别人装上就看到一串
    /// 跟自己无关的盘符。端口是协议约定，与机器无关，照旧带默认值。
    #[test]
    fn default_config_carries_no_machine_specific_paths() {
        let c = TavernConfig::default();
        assert_eq!(c.bridge_port, 5001);
        assert_eq!(c.st_port, 8000);
        for p in [&c.bridge_root, &c.sillytavern_root, &c.st_launcher] {
            assert_eq!(
                p.as_os_str().len(),
                0,
                "默认路径必须为空，实得 {}",
                p.display()
            );
        }
    }

    /// 0.26.0–0.31.0 的校验表里没有 `gemini_bridge_port`：它能存成 0、也能跟别的撞号，
    /// 而症状只是「Gemini 桥接起不来」。这条钉着那张表是全的。
    #[test]
    fn every_port_including_gemini_must_be_valid_and_distinct() {
        assert!(validate(&TavernConfig::default()).is_ok());

        let mut zero = TavernConfig::default();
        zero.gemini_bridge_port = 0;
        assert!(validate(&zero).is_err(), "0 不是有效端口");

        for clash in [
            TavernConfig {
                gemini_bridge_port: TavernConfig::default().bridge_port,
                ..TavernConfig::default()
            },
            TavernConfig {
                gemini_bridge_port: TavernConfig::default().gpt_bridge_port,
                ..TavernConfig::default()
            },
            TavernConfig {
                gemini_bridge_port: TavernConfig::default().st_port,
                ..TavernConfig::default()
            },
        ] {
            let e = validate(&clash).expect_err("撞号必须拒掉");
            let msg = e.to_string();
            assert!(msg.contains("Gemini"), "错误文案要说全四条，实得：{msg}");
        }
    }

    #[test]
    fn config_roundtrips() {
        let c = TavernConfig::default();
        let j = serde_json::to_string(&c).unwrap();
        let back: TavernConfig = serde_json::from_str(&j).unwrap();
        assert_eq!(back.bridge_port, c.bridge_port);
        assert_eq!(back.sillytavern_root, c.sillytavern_root);
    }

    #[test]
    fn vendored_copy_is_excluded_by_path_rule() {
        // find_official_claude 里那条排除规则的口径，单独钉一下。
        let vendored = r"d:\tools\sillytavern\claude-code-cli\claude.exe";
        assert!(vendored.to_lowercase().contains(r"\claude-code-cli\"));
        let official = r"c:\users\me\.local\bin\claude.exe";
        assert!(!official.to_lowercase().contains(r"\claude-code-cli\"));
    }

    /// 默认配置 = 还没配，**三项一条不漏地报出来**。
    ///
    /// 这条钉的是 `default_config_carries_no_machine_specific_paths` 的另一半：
    /// 留空是对的，但留空之后得有人认出「这是还没配」。认不出来的那一版里，
    /// 点一次酒馆只会得到「路径不存在: bridge.py」。
    #[test]
    fn a_fresh_install_reports_all_three_paths_as_unset() {
        let unset = unset_paths(&TavernConfig::default(), TavernBackend::Claude);
        assert_eq!(unset, ["桥接项目目录", "SillyTavern 目录", "酒馆启动脚本"]);
    }

    /// 那句话必须**给出下一步动作**，而不是「配置有误」。
    ///
    /// 「自动定位」是这一档唯一不用人自己去翻盘的出路 —— 提不到它，
    /// 使用者就只剩「在三个空框里凭记忆填绝对路径」这一条。
    #[test]
    fn the_setup_hint_offers_the_next_action() {
        let msg = setup_hint(&unset_paths(
            &TavernConfig::default(),
            TavernBackend::Claude,
        ));
        assert!(msg.contains("自动定位"), "得给出下一步：{msg}");
        assert!(msg.contains("桥接项目目录"), "得说还缺哪几项：{msg}");
        // GPT 后端用不着 bridge.py：缺的只有酒馆那两项。
        let gpt = unset_paths(&TavernConfig::default(), TavernBackend::Gpt);
        assert_eq!(gpt, ["SillyTavern 目录", "酒馆启动脚本"]);
    }

    /// 填了路径就不再算「没配」—— 哪怕填的是个不存在的目录。
    ///
    /// 这一档要继续走到 `is_file()` 那条 `NotFound`，报出**绝对路径**
    /// 让人自己对照着改。把它也吞进「还没配」的话，填错路径的人
    /// 会被一直劝去填一个他明明已经填过的框。
    #[test]
    fn a_filled_in_but_wrong_path_is_not_the_unconfigured_case() {
        let cfg = TavernConfig {
            bridge_root: PathBuf::from(r"x:\nope\bridge-v2"),
            sillytavern_root: PathBuf::from(r"x:\nope\SillyTavern"),
            st_launcher: PathBuf::from(r"x:\nope\start.cmd"),
            ..Default::default()
        };
        assert!(unset_paths(&cfg, TavernBackend::Claude).is_empty());
    }

    /// 填了一半也算没配 —— 空的那一项照样会拼出相对路径。
    #[test]
    fn a_half_filled_config_still_counts_as_unconfigured() {
        let cfg = TavernConfig {
            bridge_root: PathBuf::from(r"x:\nope\bridge-v2"),
            ..Default::default()
        };
        assert_eq!(
            unset_paths(&cfg, TavernBackend::Claude),
            ["SillyTavern 目录", "酒馆启动脚本"]
        );
    }

    /// 0.25.0 之前的 `sillytavern.json` 没有 GPT 那几项，读进来要落默认值，
    /// 而不是整份读失败退回三个空路径（那会让配好的人看起来像「还没配」）。
    #[test]
    fn an_old_config_file_without_gpt_fields_still_loads() {
        let text = r#"{"bridge_root":"x:/b","sillytavern_root":"x:/st","st_launcher":"x:/st.cmd","bridge_port":5001,"st_port":8000}"#;
        let cfg: TavernConfig = serde_json::from_str(text).unwrap();
        assert_eq!(cfg.gpt_bridge_port, 5002);
        assert_eq!(cfg.gpt_effort, "medium");
        assert!(cfg.gpt_model.is_empty());
        assert!(!cfg.gpt_persist_sessions);
        assert_eq!(cfg.bridge_port, 5001);
    }

    #[test]
    fn unbound_port_reads_as_free() {
        // 取一个几乎不会被占的高位端口，确认 port_in_use 的极性没写反。
        assert!(!port_in_use(58731));
    }

    #[test]
    fn bound_port_reads_as_in_use() {
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        assert!(port_in_use(port));
    }
}
