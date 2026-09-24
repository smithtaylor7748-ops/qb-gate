//! 反重力 Hub / IDE 的汉化 / 自动审批 / 高危拦截 —— 面板自己的 Chrome DevTools 协议客户端。
//!
//! # 它是什么
//!
//! EasyAntigravity（MIT，作者 Astwarp）的三个功能，按它的机制原样搬进面板：
//! Hub 是 Electron，`main.js` 没传 `--remote-debugging-port` 时自己补 `0`（随机端口，
//! 写进 `%APPDATA%\Antigravity\DevToolsActivePort`）—— 调试端口**总是开着的**。
//! IDE 是 VS Code 分支，**默认不开**端口：面板起它的时候补 `--remote-debugging-port=0`
//! （`qb-platform::sessions::desktop_arguments`），端口同样写进它自己的用户数据目录。
//! 0.27.0 起引擎**同时看两个源**，一个掉线不影响另一个。
//!
//! 面板读那个文件拿到端口，`GET /json/list` 列页面，对每个页面开一条 WebSocket，
//! `Runtime.evaluate` 一段自包含 JS（`assets/antigravity/inject.js`，逐字取自 EasyAG）：
//!
//! | 功能 | 页面里怎么做 |
//! |---|---|
//! | 汉化 | 按字典替换文本节点 / placeholder / title（`dicts/ui_v2.json` + `common.json`） |
//! | 自动审批 | 每 800 ms 扫审批卡，按 preferOption（1 仅本次 / 2 对话内 / 3 项目内 / 4 全局）点选项再点提交 |
//! | 高危拦截 | 点提交之前把命令正文过一遍 `danger-rules.json` 的正则，命中就不点、打 `[EA_ALERT]` |
//!
//! 页面刷新或导航后重新注入（`Runtime.executionContextCreated` / `Page.loadEventFired` /
//! `Page.frameNavigated`），外加每 2 s 一次心跳重注入 —— 心跳同时把最新的开关、字典、
//! 规则带过去，所以界面上改完立刻生效。脚本自带 `__ea_engine_running` 幂等。
//!
//! # 它不做什么（跟 EasyAG 的分界）
//!
//! * **不落 `version.dll`、不改 Google 安装目录里任何文件**：EasyAG 的「免 TUN 代理」是往
//!   `Programs\antigravity\` 丢一个第三方代理核心的 DLL 并注入语言服务器；使用者
//!   2026-09-20 决定不做（来源许可不明的二进制、注入官方进程、每次更新回写，
//!   撞「抄代码先看 license」与「不改官方二进制」）。
//! * **不起 Hub**：起不起走 `workspace::launch`（验 IP → 托管 → 租约）。这里只「附加」到
//!   正在跑的 Hub 上 —— 使用者自己从开始菜单起的也能附加。
//! * **不碰凭据**：CDP 只用来 `Runtime.evaluate` 我们自己的脚本；不读页面存储、不截图。
//!
//! # 「注入成功」必须回读核实，不许自称（0.27.0）
//!
//! 0.26.0 只数「`Runtime.evaluate` 发出去了几次」，而 `Runtime.evaluate` 的**回执被整个丢掉** ——
//! 脚本在页面里抛异常、或者压根注进了一个没有 `window` 的目标，界面照样显示「已注入 N 个页面」。
//! 那正是「看起来通过了、实际什么都没测」的那一类失效。
//!
//! 现在每次注入之后紧跟一条**面板自己的**表达式（[`VERIFY_EXPR`]，不是 EasyAG 的脚本），
//! 回读 `window.__ea_engine_running` 与字典条数；回执里带 `exceptionDetails` 就记一条日志。
//! 界面显示的是**核实过**的页面数。
//!
//! # 状态只在进程内
//!
//! 引擎跟 `gpt_bridge` 一样活在面板进程里，面板退出它就没了（页面里的脚本随下一次刷新消失，
//! 不留任何东西）。**两个源都**连续 5 次（约 10 s）读不到 CDP 才当反重力已退出、自动停 ——
//! 只掉一个的时候另一个要接着干活。

use crate::error::{GateError, Result};
use crate::install::antigravity::Product;
use crate::plugins::{DependencyCheck, PluginState, PluginStatus};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use ts_rs::TS;

pub const PLUGIN_ID: &str = "antigravity-ui";
pub const PLUGIN_NAME: &str = "反重力 · 汉化与审批";

/// 心跳：列页面 + 重注入的间隔。EasyAG 是 2 s，照旧。
const TICK: Duration = Duration::from_secs(2);
/// 连续几次读不到 CDP 就当 Hub 退出了。5 × 2 s ≈ 10 s，EasyAG 的口径。
const MAX_FAIL_STREAK: u32 = 5;
/// `GET /json/list` 的超时。
const HTTP_TIMEOUT: Duration = Duration::from_secs(4);
/// 界面日志留多少行。
const LOG_CAPACITY: usize = 300;

/// 注入之后回读核实用的表达式。**这是面板自己的代码，不是 EasyAG 的脚本** ——
/// `inject.js` 逐字保留，一个字都不许改（改了要同步改 ATTRIBUTION，且架构测试数着占位符）。
///
/// 只读，不改页面上任何东西。`__ea_verify` 这个键是回执的认领标记：回执里没有 `method`，
/// 靠它把「核实结果」跟别的命令回执分开，省掉一套 id 记账。
pub const VERIFY_EXPR: &str = "JSON.stringify({__ea_verify:1,run:!!window.__ea_engine_running,dict:window.__ea_dict?Object.keys(window.__ea_dict).length:0,i18n:!!(window.__ea_config&&window.__ea_config.enableI18n)})";

// ------------------------------------------------------------------ 资产（MIT，EasyAntigravity）

/// 注入脚本（模板）。六个 `__EA_*` 占位符由 [`assemble_script`] 替换。
pub const INJECT_TEMPLATE: &str = include_str!("../../assets/antigravity/inject.js");
const DICT_UI_V2: &str = include_str!("../../assets/antigravity/dicts/ui_v2.json");
const DICT_COMMON: &str = include_str!("../../assets/antigravity/dicts/common.json");
/// 内置高危规则。第一次启动时复制到状态目录让使用者改；这份永远是出厂样子。
pub const DEFAULT_RULES: &str = include_str!("../../assets/antigravity/danger-rules.json");

// ------------------------------------------------------------------ 配置

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct UiConfig {
    /// 界面汉化。
    pub enable_i18n: bool,
    /// 自动审批（审批卡自动点「提交」）。
    pub auto_accept: bool,
    /// 高危拦截（命中规则的命令不点、记日志）。只在 `auto_accept` 开着时有意义。
    pub block_dangerous: bool,
    /// 审批卡上选哪一项：1 仅本次 / 2 对话内始终 / 3 项目内始终 / 4 全局始终。
    pub prefer_option: u8,
    /// 从面板起 Hub 之后要不要自动附加引擎。
    pub attach_on_launch: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        // EasyAG 的默认值：三个都开、选项 4（全局）。
        Self {
            enable_i18n: true,
            auto_accept: true,
            block_dangerous: true,
            prefer_option: 4,
            attach_on_launch: true,
        }
    }
}

fn data_dir() -> PathBuf {
    crate::paths::state_dir().join("antigravity")
}
fn config_path() -> PathBuf {
    data_dir().join("ui-config.json")
}
/// 使用者可改的规则文件。第一次启动从内置那份复制出来。
pub fn rules_path() -> PathBuf {
    data_dir().join("danger-rules.json")
}
/// 使用者自己的补充字典（可选）。存在就并在内置字典之后，同键覆盖。
pub fn dict_override_path() -> PathBuf {
    data_dir().join("dict.override.json")
}

pub fn load_config() -> UiConfig {
    let cfg: UiConfig = crate::config_io::read_optional(&config_path())
        .ok()
        .flatten()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    normalize(cfg)
}

fn normalize(mut cfg: UiConfig) -> UiConfig {
    if !(1..=4).contains(&cfg.prefer_option) {
        cfg.prefer_option = 4;
    }
    cfg
}

pub fn save_config(cfg: &UiConfig) -> Result<()> {
    let cfg = normalize(cfg.clone());
    std::fs::create_dir_all(data_dir())?;
    crate::config_io::replace(&config_path(), Some(&serde_json::to_vec_pretty(&cfg)?))
}

// ------------------------------------------------------------------ 字典与规则

/// 合并后的字典。顺序照 EasyAG：`ui_v2.json` 先、`common.json` 后（同键后者赢），
/// 再叠使用者的 `dict.override.json`。
pub fn dictionary() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for text in [DICT_UI_V2, DICT_COMMON] {
        if let Ok(m) = serde_json::from_str::<BTreeMap<String, String>>(text) {
            out.extend(m);
        }
    }
    if let Ok(Some(bytes)) = crate::config_io::read_optional(&dict_override_path()) {
        if let Ok(m) = serde_json::from_slice::<BTreeMap<String, String>>(&bytes) {
            out.extend(m);
        }
    }
    out
}

/// 一条高危规则（跟 EasyAG 的 `danger-rules.json` 同形）。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct DangerRule {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub pattern: String,
    #[serde(default = "default_flags")]
    pub flags: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}
fn default_flags() -> String {
    "i".into()
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct DangerRules {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<DangerRule>,
}
fn default_version() -> u32 {
    1
}

impl DangerRules {
    /// 真正会送进页面的那几条：整组开着、单条开着、有 pattern。
    pub fn active(&self) -> Vec<&DangerRule> {
        if !self.enabled {
            return Vec::new();
        }
        self.rules
            .iter()
            .filter(|r| r.enabled && !r.pattern.trim().is_empty())
            .collect()
    }
}

/// 解析规则文本。空规则集退回内置那份 —— EasyAG 的口径（`loadDangerRules`）。
pub fn parse_rules(text: &str) -> DangerRules {
    match serde_json::from_str::<DangerRules>(text) {
        Ok(r) if !r.rules.is_empty() => r,
        _ => serde_json::from_str(DEFAULT_RULES).expect("内置规则是合法 JSON"),
    }
}

/// 读使用者那份规则；没有就先把内置那份复制出去再读。
pub fn load_rules() -> Result<DangerRules> {
    let p = rules_path();
    if crate::config_io::read_optional(&p)?.is_none() {
        std::fs::create_dir_all(data_dir())?;
        crate::config_io::replace(&p, Some(DEFAULT_RULES.as_bytes()))?;
    }
    Ok(parse_rules(&crate::config_io::read_text(&p)?))
}

/// 把使用者改过的规则写回去。写之前每条正则都先编译一遍 —— 页面里 `new RegExp` 失败
/// 是静默跳过的（EasyAG 的写法），在这里报出来才有人看见。
pub fn save_rules(rules: &DangerRules) -> Result<()> {
    for r in &rules.rules {
        if r.pattern.trim().is_empty() {
            return Err(GateError::Other(format!(
                "规则「{}」的 pattern 是空的",
                r.name
            )));
        }
        // JS 正则与 Rust 正则不完全一样；这里只用最宽松的检查挡住明显的括号不配对。
        let open = r.pattern.matches('(').count();
        let close = r.pattern.matches(')').count();
        if open != close {
            return Err(GateError::Other(format!(
                "规则「{}」的 pattern 括号不配对：{}",
                r.name, r.pattern
            )));
        }
    }
    std::fs::create_dir_all(data_dir())?;
    crate::config_io::replace(&rules_path(), Some(&serde_json::to_vec_pretty(rules)?))
}

/// 拼出要注入的脚本。**纯函数**：六个占位符全部替换，一个都不许留。
pub fn assemble_script(
    cfg: &UiConfig,
    dict: &BTreeMap<String, String>,
    rules: &DangerRules,
) -> String {
    let patterns: Vec<serde_json::Value> = rules
        .active()
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": if r.id.is_empty() { "rule" } else { r.id.as_str() },
                "name": if r.name.is_empty() { r.id.as_str() } else { r.name.as_str() },
                "pattern": r.pattern,
                "flags": if r.flags.is_empty() { "i" } else { r.flags.as_str() },
            })
        })
        .collect();
    INJECT_TEMPLATE
        .replace("__EA_PREFER_OPTION__", &cfg.prefer_option.to_string())
        .replace("__EA_BLOCK_DANGEROUS__", &cfg.block_dangerous.to_string())
        .replace("__EA_AUTO_ACCEPT__", &cfg.auto_accept.to_string())
        .replace("__EA_ENABLE_I18N__", &cfg.enable_i18n.to_string())
        .replace(
            "__EA_DICT__",
            &serde_json::to_string(dict).unwrap_or_else(|_| "{}".into()),
        )
        .replace(
            "__EA_PATTERNS__",
            &serde_json::to_string(&patterns).unwrap_or_else(|_| "[]".into()),
        )
}

// ------------------------------------------------------------------ 状态

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UiLogLine {
    pub at: String,
    /// `CDP` / `AUTO-ACCEPT` / `SECURITY ALERT` / `OPTION` / `SYSTEM`。
    pub category: String,
    pub message: String,
}

/// 引擎的一个来源 = 一个产品（Hub / IDE）。0.27.0 起两个一起看。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UiSource {
    pub product: Product,
    pub label: String,
    /// 这个产品此刻的调试端口。0 = 没有端口文件。
    pub port: u16,
    pub targets: u32,
    pub sockets: u32,
    /// **回读核实过**的页面数 —— 不是「发出去几次」。见模块文档。
    pub verified: u32,
    /// 页面里真实的字典条数（回读来的）。0 = 没核实到。
    pub dict_in_page: u32,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UiStatus {
    pub running: bool,
    /// 引擎连的调试端口（合计口径：第一个有端口的源）。没跑 / 读不到就是 0。
    pub port: u16,
    pub targets: u32,
    pub sockets: u32,
    /// 两个产品各自的情况。界面按源分行显示。
    pub sources: Vec<UiSource>,
    /// 回读核实过的页面数（两个源合计）。
    pub verified: u32,
    /// 上一次**自动附加**失败的原因（起了反重力但引擎没接上）。空 = 没有失败记录。
    /// 0.26.0 这句只写进审计日志，界面上一片安静 —— 使用者看到的就只有「还是英文」。
    pub last_attach_error: String,
    // 计数用 u32 过 IPC：ts-rs 把 u64 导成 bigint，界面上算加减法要到处 Number() 转。
    pub inject_count: u32,
    pub approve_count: u32,
    pub block_count: u32,
    pub cdp_error: String,
    pub config: UiConfig,
    pub dict_entries: u32,
    pub rules_total: u32,
    pub rules_on: u32,
    pub rules_path: String,
    /// Hub 装没装（位置表 `install::antigravity`）。
    pub hub_installed: bool,
    /// IDE 装没装。
    pub ide_installed: bool,
    /// 任意一个产品的调试端口文件在不在 —— 起过（不一定还在跑）。
    pub port_file_present: bool,
    pub log: Vec<UiLogLine>,
    pub detail: String,
}

/// 引擎的计数器。原子量：主循环、每条页面连接、状态读取三边都在碰它，不上锁。
#[derive(Default)]
pub struct Counters {
    targets: AtomicU64,
    sockets: AtomicU64,
    inject_count: AtomicU64,
    approve_count: AtomicU64,
    block_count: AtomicU64,
    port: AtomicU64,
}

struct Running {
    stop: tokio::sync::watch::Sender<bool>,
    counters: Arc<Counters>,
    error: Arc<Mutex<String>>,
    log: Arc<Mutex<VecDeque<UiLogLine>>>,
    /// 每个源一行，主循环每个 tick 重写一次。
    sources: Arc<Mutex<Vec<UiSource>>>,
}

/// 上一次自动附加失败的原因。跟引擎本身分开存：附加失败的时候**引擎并没有在跑**，
/// 存进 `Running` 就跟着一起没了，而这正是使用者最需要看见的那一句。
static LAST_ATTACH: OnceLock<Mutex<String>> = OnceLock::new();

fn attach_cell() -> &'static Mutex<String> {
    LAST_ATTACH.get_or_init(Default::default)
}

/// 记一句自动附加失败的原因（空串 = 清掉）。由 `usecase::antigravity_ops` 调。
pub fn note_attach_error(reason: &str) {
    if let Ok(mut g) = attach_cell().lock() {
        *g = reason.into();
    }
}

fn last_attach_error() -> String {
    attach_cell().lock().map(|g| g.clone()).unwrap_or_default()
}

static ENGINE: OnceLock<Mutex<Option<Arc<Running>>>> = OnceLock::new();

fn cell() -> &'static Mutex<Option<Arc<Running>>> {
    ENGINE.get_or_init(Default::default)
}
fn current() -> Option<Arc<Running>> {
    cell().lock().ok().and_then(|g| g.clone())
}
pub fn running() -> bool {
    current().is_some()
}

fn push_log(log: &Mutex<VecDeque<UiLogLine>>, category: &str, message: impl Into<String>) {
    if let Ok(mut l) = log.lock() {
        if l.len() >= LOG_CAPACITY {
            l.pop_front();
        }
        l.push_back(UiLogLine {
            at: chrono::Local::now().format("%H:%M:%S").to_string(),
            category: category.into(),
            message: message.into(),
        });
    }
}

fn roaming() -> PathBuf {
    dirs::config_dir().unwrap_or_default()
}
fn local() -> PathBuf {
    dirs::data_local_dir().unwrap_or_default()
}

/// 读 `DevToolsActivePort` 的第一行。文件不在 → `None`；在但不是数字 → `None`。
pub fn read_port_file(text: &str) -> Option<u16> {
    text.lines()
        .next()?
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
}

/// IDE 此刻用的是哪个用户数据目录（0.30.0）。
///
/// 反重力 IDE 有槽位之后，面板起的那份 IDE 的 `DevToolsActivePort` 在**槽位的**
/// `--user-data-dir` 里，不在 `%APPDATA%\Antigravity IDE`。这里存的是编排层
/// （`usecase::antigravity_ops`）在起 IDE / 刷新状态时告诉我们的目录；`None` = 默认那份。
/// 这个 crate 跟 `qb-accounts` 同层、不能反过来去问它，所以由上层灌进来。
static IDE_USER_DATA: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

pub fn set_ide_user_data_dir(dir: Option<PathBuf>) {
    if let Ok(mut cell) = IDE_USER_DATA.get_or_init(|| Mutex::new(None)).lock() {
        *cell = dir;
    }
}

fn ide_user_data_dir() -> Option<PathBuf> {
    IDE_USER_DATA
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|c| c.clone())
}

/// 某个产品此刻的调试端口文件在哪：IDE 有激活槽位时是槽位目录里那份。
fn port_file_of(product: Product) -> PathBuf {
    if product == Product::Ide {
        if let Some(dir) = ide_user_data_dir() {
            return dir.join("DevToolsActivePort");
        }
    }
    crate::install::antigravity::devtools_port_file(product, &roaming())
}

/// 某个产品此刻的调试端口。文件不在（没起过 / IDE 不是面板起的）→ `None`。
pub fn devtools_port(product: Product) -> Option<u16> {
    std::fs::read_to_string(port_file_of(product))
        .ok()
        .and_then(|t| read_port_file(&t))
}

/// 此刻有端口文件的每个产品。**一个都没有也照样返回空表**，别在这里报错 ——
/// 「装了哪个、起了哪个」在别人机器上是四种组合，引擎要四种都活得下去。
fn devtools_ports() -> Vec<(Product, u16)> {
    Product::ALL
        .into_iter()
        .filter_map(|p| devtools_port(p).map(|port| (p, port)))
        .collect()
}

/// CDP 真的应答得了吗 —— 端口文件在**不等于**端口活着（上一次运行留下的文件照样在）。
///
/// 这是 0.26.0 自动附加失效的根因所在：那一版等的是「端口文件的修改时间变了」，
/// 而 Chromium 在进程刚起来的几百毫秒里就把文件写完了，采样基准时的值已经是新的 ——
/// 条件永远不成立，30 次全落空，日志里只留一句「没等到调试端口文件」。
/// **判定要问「列得出页面吗」，不要问「文件变了吗」。**
pub async fn cdp_ready(product: Product) -> bool {
    let Some(port) = devtools_port(product) else {
        return false;
    };
    let Ok(http) = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .no_proxy()
        .build()
    else {
        return false;
    };
    match list_targets(&http, port).await {
        Ok(t) => !t.is_empty(),
        Err(_) => false,
    }
}

/// `GET /json/list` 并按 EasyAG 的口径筛掉 devtools 自己的页与空白页。
async fn list_targets(
    http: &reqwest::Client,
    port: u16,
) -> std::result::Result<Vec<Target>, String> {
    let targets: Vec<Target> = http
        .get(format!("http://127.0.0.1:{port}/json/list"))
        .send()
        .await
        .map_err(|e| format!("列页面失败：{e}"))?
        .json()
        .await
        .map_err(|e| format!("页面清单不是 JSON：{e}"))?;
    Ok(targets.into_iter().filter(Target::is_page).collect())
}

pub fn status() -> UiStatus {
    let cfg = load_config();
    let rules = load_rules().unwrap_or_else(|_| parse_rules(DEFAULT_RULES));
    let dict_entries = dictionary().len() as u32;
    let hub_installed = Product::Hub.launcher(&local()).is_file();
    let ide_installed = Product::Ide.launcher(&local()).is_file();
    let ports = devtools_ports();
    let port_file_present = !ports.is_empty();
    let rules_on = rules.active().len() as u32;
    let rules_total = rules.rules.len() as u32;
    if let Some(r) = current() {
        let c = &r.counters;
        let error = r.error.lock().map(|e| e.clone()).unwrap_or_default();
        let sockets = c.sockets.load(Ordering::Relaxed) as u32;
        let sources: Vec<UiSource> = r.sources.lock().map(|s| s.clone()).unwrap_or_default();
        let verified: u32 = sources.iter().map(|s| s.verified).sum();
        return UiStatus {
            running: true,
            port: c.port.load(Ordering::Relaxed) as u16,
            targets: c.targets.load(Ordering::Relaxed) as u32,
            sockets,
            sources,
            verified,
            last_attach_error: String::new(),
            inject_count: c.inject_count.load(Ordering::Relaxed) as u32,
            approve_count: c.approve_count.load(Ordering::Relaxed) as u32,
            block_count: c.block_count.load(Ordering::Relaxed) as u32,
            cdp_error: error.clone(),
            config: cfg,
            dict_entries,
            rules_total,
            rules_on,
            rules_path: rules_path().display().to_string(),
            hub_installed,
            ide_installed,
            port_file_present,
            log: r
                .log
                .lock()
                .map(|l| l.iter().cloned().collect())
                .unwrap_or_default(),
            // ⛔ 说「已在 N 个页面核实生效」而不是「已注入 N 个页面」：
            // 后者是自称，前者是回读回来的。连着 0.26.0 那个坑一起钉在这。
            detail: if verified > 0 {
                format!("运行中 · 已在 {verified} 个页面核实生效")
            } else if sockets > 0 {
                format!("运行中 · 连上 {sockets} 个页面，还没核实到引擎（看下面的日志）")
            } else if error.is_empty() {
                "运行中 · 等待反重力的页面".into()
            } else {
                format!("运行中 · {error}")
            },
        };
    }
    let attach_error = last_attach_error();
    UiStatus {
        running: false,
        port: 0,
        targets: 0,
        sockets: 0,
        sources: Vec::new(),
        verified: 0,
        last_attach_error: attach_error.clone(),
        inject_count: 0,
        approve_count: 0,
        block_count: 0,
        cdp_error: String::new(),
        config: cfg,
        dict_entries,
        rules_total,
        rules_on,
        rules_path: rules_path().display().to_string(),
        hub_installed,
        ide_installed,
        port_file_present,
        log: Vec::new(),
        detail: if !attach_error.is_empty() {
            format!("未运行 · {attach_error}")
        } else if !hub_installed && !ide_installed {
            "没装反重力：到「软件」页按官网链接装好".into()
        } else if !port_file_present {
            "未运行 · 反重力还没起过（在这一页起 Hub 或 IDE，引擎会自己附上去）".into()
        } else {
            "未运行 · 点「附加」把汉化与审批引擎接到正在跑的反重力上".into()
        },
    }
}

/// 给插件商店看的状态。
pub fn plugin_status() -> PluginStatus {
    let s = status();
    // 装了任意一个就算有得干活 —— 别人机器上四种组合（都装 / 只装一个 / 都没装）都要说得对。
    let any_installed = s.hub_installed || s.ide_installed;
    let mut checks = vec![
        DependencyCheck::new(
            "反重力",
            any_installed,
            match (s.hub_installed, s.ide_installed) {
                (true, true) => "Hub 与 IDE 都已安装（位置表 install::antigravity）".to_string(),
                (true, false) => "只装了 Hub（位置表 install::antigravity）".to_string(),
                (false, true) => "只装了 IDE（位置表 install::antigravity）".to_string(),
                (false, false) => "未安装：到「软件」页按官网链接装".to_string(),
            },
        ),
        DependencyCheck::new(
            "汉化字典",
            s.dict_entries > 0,
            format!("{} 条（EasyAntigravity，MIT）", s.dict_entries),
        ),
        DependencyCheck::new(
            "高危规则",
            s.rules_total > 0,
            format!(
                "{} 条启用 / 共 {} 条 · {}",
                s.rules_on, s.rules_total, s.rules_path
            ),
        ),
    ];
    if s.running {
        for src in &s.sources {
            checks.push(DependencyCheck::new(
                format!("CDP · {}", src.label),
                src.verified > 0,
                if src.port == 0 {
                    "没有调试端口文件（没起过，或 IDE 不是面板起的）".to_string()
                } else if !src.error.is_empty() {
                    format!("端口 {} · {}", src.port, src.error)
                } else {
                    format!(
                        "端口 {} · 页面 {} · 已连 {} · 核实生效 {}",
                        src.port, src.targets, src.sockets, src.verified
                    )
                },
            ));
        }
    }
    PluginStatus {
        id: PLUGIN_ID,
        name: PLUGIN_NAME,
        state: if !any_installed {
            PluginState::Missing
        } else if s.running {
            PluginState::Running
        } else {
            PluginState::Ready
        },
        detail: s.detail,
        checks,
    }
}

// ------------------------------------------------------------------ 起 / 停

/// 附加到正在跑的 Hub。已经在跑就原样返回（幂等）。
///
/// 这里**不起 Hub**。Hub 没起过（没有端口文件）就报错指路，不替使用者起 ——
/// 起 Hub 要过门禁，那是 `workspace::launch` 的事。
pub async fn start() -> Result<UiStatus> {
    if running() {
        return Ok(status());
    }
    let hub = Product::Hub.launcher(&local()).is_file();
    let ide = Product::Ide.launcher(&local()).is_file();
    if !hub && !ide {
        return Err(GateError::Other(
            "没装反重力。到「软件」页按官网链接装好再来。".into(),
        ));
    }
    // 端口文件在**不等于**端口活着（上一次运行留下的文件照样在），所以这里真的探一次。
    // 只要有一个源应答得了就起 —— 两个产品在别人机器上是分开装、分开起的。
    let mut reachable: Vec<(Product, u16)> = Vec::new();
    for (product, port) in devtools_ports() {
        if cdp_ready(product).await {
            reachable.push((product, port));
        }
    }
    if reachable.is_empty() {
        return Err(GateError::Other(attach_hint(hub, ide)));
    }
    // 把内置规则复制出去（第一次），让使用者有东西可改。
    load_rules()?;

    let (stop, stopped) = tokio::sync::watch::channel(false);
    let counters = Arc::new(Counters::default());
    counters
        .port
        .store(reachable[0].1 as u64, Ordering::Relaxed);
    let error = Arc::new(Mutex::new(String::new()));
    let log = Arc::new(Mutex::new(VecDeque::new()));
    let sources = Arc::new(Mutex::new(Vec::new()));
    let where_ = reachable
        .iter()
        .map(|(p, port)| format!("{}:{port}", p.label()))
        .collect::<Vec<_>>()
        .join(" / ");
    push_log(&log, "SYSTEM", format!("引擎启动，调试端口 {where_}"));
    note_attach_error("");
    let running = Arc::new(Running {
        stop,
        counters: Arc::clone(&counters),
        error: Arc::clone(&error),
        log: Arc::clone(&log),
        sources: Arc::clone(&sources),
    });
    *cell().lock().unwrap() = Some(Arc::clone(&running));
    tokio::spawn(engine_loop(stopped, counters, error, log, sources));
    crate::audit::write(&format!(
        "反重力汉化与审批引擎已附加（CDP {where_}，只注入脚本，不改任何文件）"
    ));
    Ok(status())
}

/// 一个源都探不通时那句话。**要可操作**，不是「失败了」。
///
/// IDE 那一档单独写：它的端口是面板起它的时候才补上的，使用者自己从开始菜单起的没有 ——
/// 不说清楚的话，人只会看到「附加失败」然后反复点。
fn attach_hint(hub_installed: bool, ide_installed: bool) -> String {
    let mut lines = vec!["反重力的调试端口连不上，引擎没附加。".to_string()];
    if hub_installed {
        lines.push(
            "· Hub：还没起过，或者已经退出了。在这一页点「反重力 Hub」起它，引擎会自己附上去。"
                .into(),
        );
    }
    if ide_installed {
        lines.push(
            "· IDE：它默认不开调试端口，**只有面板起的那一份**才有。请在这一页点「反重力 IDE」，别从开始菜单起。"
                .into(),
        );
        lines.push(
            "· 如果点了还是连不上：装了第三方汉化壳的话，那层壳可能没把 --remote-debugging-port 传给里层。"
                .into(),
        );
    }
    lines.join("\n")
}

/// 停引擎：断开所有 WebSocket。页面里已经注入的脚本随下一次刷新消失。
pub fn stop() -> Result<()> {
    let Some(r) = cell().lock().unwrap().take() else {
        return Ok(());
    };
    let _ = r.stop.send(true);
    crate::audit::write("反重力汉化与审批引擎已停");
    Ok(())
}

/// 引擎主循环。每个 tick：读端口 → 列页面 → 新页面开连接、旧页面心跳重注入、死页面收掉。
async fn engine_loop(
    mut stopped: tokio::sync::watch::Receiver<bool>,
    counters: Arc<Counters>,
    error: Arc<Mutex<String>>,
    log: Arc<Mutex<VecDeque<UiLogLine>>>,
    sources: Arc<Mutex<Vec<UiSource>>>,
) {
    let http = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .no_proxy()
        .build()
        .ok();
    let mut conns: HashMap<String, Conn> = HashMap::new();
    let mut fail_streak: u32 = 0;
    let set_error = |e: String| {
        if let Ok(mut g) = error.lock() {
            *g = e;
        }
    };
    loop {
        if *stopped.borrow() {
            break;
        }
        let script = Arc::new(assemble_script(
            &load_config(),
            &dictionary(),
            &load_rules().unwrap_or_else(|_| parse_rules(DEFAULT_RULES)),
        ));
        let mut rows: Vec<UiSource> = Vec::new();
        let mut alive_keys: std::collections::HashSet<String> = Default::default();
        let mut any_ok = false;

        for product in Product::ALL {
            let port = devtools_port(product).unwrap_or(0);
            let mut row = UiSource {
                product,
                label: product.label().into(),
                port,
                targets: 0,
                sockets: 0,
                verified: 0,
                dict_in_page: 0,
                error: String::new(),
            };
            if port == 0 {
                // 没有端口文件不是错误：这个产品可能压根没装、或者没起。
                rows.push(row);
                continue;
            }
            let listed = match http.as_ref() {
                None => Err("HTTP 客户端没建起来".to_string()),
                Some(h) => list_targets(h, port).await,
            };
            match listed {
                Err(e) => row.error = e,
                Ok(valid) => {
                    any_ok = true;
                    row.targets = valid.len() as u32;
                    for t in valid {
                        alive_keys.insert(t.ws.clone());
                        match conns.get(&t.ws) {
                            // 心跳重注入：刷新配置；页面被 Reload 清掉引擎也靠这一发拉回来。
                            Some(c) => {
                                let _ = c.tx.send(ConnCmd::Inject(Arc::clone(&script)));
                            }
                            None => {
                                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                                let alive = Arc::new(AtomicBool::new(true));
                                let verified = Arc::new(AtomicU64::new(0));
                                let dict_in_page = Arc::new(AtomicU64::new(0));
                                let label = if t.title.is_empty() {
                                    t.url.clone()
                                } else {
                                    t.title.clone()
                                };
                                tokio::spawn(page_connection(
                                    t.ws.clone(),
                                    format!("{} · {label}", product.label()),
                                    Arc::clone(&script),
                                    rx,
                                    PageState {
                                        alive: Arc::clone(&alive),
                                        verified: Arc::clone(&verified),
                                        dict_in_page: Arc::clone(&dict_in_page),
                                        counters: Arc::clone(&counters),
                                        log: Arc::clone(&log),
                                    },
                                ));
                                conns.insert(
                                    t.ws.clone(),
                                    Conn {
                                        tx,
                                        alive,
                                        verified,
                                        dict_in_page,
                                        product,
                                    },
                                );
                            }
                        }
                    }
                    if row.targets == 0 {
                        row.error = "CDP 没有可用的页面".into();
                    }
                }
            }
            rows.push(row);
        }

        conns.retain(|key, c| alive_keys.contains(key) && c.alive.load(Ordering::Relaxed));
        // 每个源的连接数与核实数都从存活的连接上数回来，不另记一份账。
        for row in &mut rows {
            let mine = conns.values().filter(|c| c.product == row.product);
            let mut sockets = 0u32;
            let mut verified = 0u32;
            let mut dict = 0u32;
            for c in mine {
                sockets += 1;
                if c.verified.load(Ordering::Relaxed) > 0 {
                    verified += 1;
                    dict = dict.max(c.dict_in_page.load(Ordering::Relaxed) as u32);
                }
            }
            row.sockets = sockets;
            row.verified = verified;
            row.dict_in_page = dict;
        }
        counters.targets.store(
            rows.iter().map(|r| r.targets as u64).sum(),
            Ordering::Relaxed,
        );
        counters
            .sockets
            .store(conns.len() as u64, Ordering::Relaxed);
        counters.port.store(
            rows.iter().map(|r| r.port).find(|p| *p > 0).unwrap_or(0) as u64,
            Ordering::Relaxed,
        );
        let tick_error: String = rows
            .iter()
            .filter(|r| !r.error.is_empty())
            .map(|r| format!("{}：{}", r.label, r.error))
            .collect::<Vec<_>>()
            .join("；");
        if let Ok(mut g) = sources.lock() {
            *g = rows;
        }

        // ⛔ 只有**所有**源都没应答才累加失联计数：一个产品退出不许把另一个的引擎一起停掉。
        if any_ok && !conns.is_empty() {
            fail_streak = 0;
            set_error(tick_error);
        } else {
            fail_streak += 1;
            set_error(if tick_error.is_empty() {
                "读不到调试端口".into()
            } else {
                tick_error
            });
            if fail_streak >= MAX_FAIL_STREAK {
                push_log(
                    &log,
                    "SYSTEM",
                    "CDP 连续失联约 10 秒，当作反重力已退出，引擎停止",
                );
                break;
            }
        }
        tokio::select! {
            _ = stopped.changed() => break,
            _ = tokio::time::sleep(TICK) => {}
        }
    }
    for (_, c) in conns.drain() {
        let _ = c.tx.send(ConnCmd::Close);
    }
    counters.sockets.store(0, Ordering::Relaxed);
    counters.targets.store(0, Ordering::Relaxed);
    if let Ok(mut g) = sources.lock() {
        g.clear();
    }
    // 自己停的（失联）也要把全局槽位清掉，否则界面一直显示「运行中」。
    if let Ok(mut g) = cell().lock() {
        if g.as_ref()
            .is_some_and(|r| Arc::ptr_eq(&r.counters, &counters))
        {
            *g = None;
        }
    }
}

/// 一条页面连接自己那份共享状态。打包传是因为分开传就是 9 个参数
/// （clippy 的 `too_many_arguments`），而这几个本来就是同一条连接的东西。
struct PageState {
    alive: Arc<AtomicBool>,
    verified: Arc<AtomicU64>,
    dict_in_page: Arc<AtomicU64>,
    counters: Arc<Counters>,
    log: Arc<Mutex<VecDeque<UiLogLine>>>,
}

struct Conn {
    tx: tokio::sync::mpsc::UnboundedSender<ConnCmd>,
    alive: Arc<AtomicBool>,
    /// 回读核实过（页面里真的有引擎在跑）。0 = 还没核实到。
    verified: Arc<AtomicU64>,
    /// 页面里的字典条数（回读来的）。
    dict_in_page: Arc<AtomicU64>,
    product: Product,
}

enum ConnCmd {
    Inject(Arc<String>),
    Close,
}

/// `/json/list` 里的一项。只取用得到的三个字段。
#[derive(Debug, Clone, Deserialize)]
struct Target {
    #[serde(default, rename = "webSocketDebuggerUrl")]
    ws: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
}

impl Target {
    /// EasyAG 的过滤口径：有调试地址、不是 devtools:// 自己的页、不是 data:text/html 的空白页。
    fn is_page(&self) -> bool {
        !self.ws.is_empty()
            && !self.url.starts_with("devtools://")
            && !self.url.starts_with("data:text/html")
    }
}

fn cdp_message(id: u64, method: &str, params: serde_json::Value) -> String {
    serde_json::json!({ "id": id, "method": method, "params": params }).to_string()
}

fn evaluate_message(id: u64, script: &str) -> String {
    cdp_message(
        id,
        "Runtime.evaluate",
        serde_json::json!({ "expression": script, "returnByValue": false, "awaitPromise": false }),
    )
}

type WsSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tokio_tungstenite::tungstenite::Message,
>;

async fn send_text(sink: &mut WsSink, text: String) -> std::result::Result<(), String> {
    sink.send(tokio_tungstenite::tungstenite::Message::Text(text.into()))
        .await
        .map_err(|e| e.to_string())
}

/// 一个页面一条连接：开 → `Runtime.enable` / `Page.enable` / 注入 → 收事件、按需重注入。
async fn page_connection(
    ws_url: String,
    label: String,
    first_script: Arc<String>,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<ConnCmd>,
    state: PageState,
) {
    let PageState {
        alive,
        verified,
        dict_in_page,
        counters,
        log,
    } = state;
    let result = async {
        let (ws, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(|e| format!("WebSocket 连不上：{e}"))?;
        let (mut sink, mut stream) = ws.split();
        let mut next_id: u64 = 1;
        send_text(
            &mut sink,
            cdp_message(next_id, "Runtime.enable", serde_json::json!({})),
        )
        .await
        .map_err(|e| format!("Runtime.enable 发不出去：{e}"))?;
        next_id += 1;
        send_text(
            &mut sink,
            cdp_message(next_id, "Page.enable", serde_json::json!({})),
        )
        .await
        .map_err(|e| format!("Page.enable 发不出去：{e}"))?;
        next_id += 1;
        send_text(&mut sink, evaluate_message(next_id, &first_script))
            .await
            .map_err(|e| format!("注入发不出去：{e}"))?;
        next_id += 1;
        counters.inject_count.fetch_add(1, Ordering::Relaxed);
        // 紧跟一发回读。脚本是同步的 IIFE，页面主线程按顺序跑，所以这一发看到的
        // 就是刚注进去那一份的结果 —— 不用另外等。
        send_text(&mut sink, evaluate_message(next_id, VERIFY_EXPR))
            .await
            .map_err(|e| format!("核实发不出去：{e}"))?;
        next_id += 1;
        push_log(
            &log,
            "CDP",
            format!("已连接页面并注入：{}", clip(&label, 60)),
        );
        let mut latest_script = first_script;
        loop {
            tokio::select! {
                cmd = rx.recv() => match cmd {
                    Some(ConnCmd::Inject(script)) => {
                        latest_script = script;
                        send_text(&mut sink, evaluate_message(next_id, &latest_script))
                            .await
                            .map_err(|e| format!("重注入发不出去：{e}"))?;
                        next_id += 1;
                        counters.inject_count.fetch_add(1, Ordering::Relaxed);
                        send_text(&mut sink, evaluate_message(next_id, VERIFY_EXPR))
                            .await
                            .map_err(|e| format!("核实发不出去：{e}"))?;
                        next_id += 1;
                    }
                    Some(ConnCmd::Close) | None => break,
                },
                msg = stream.next() => match msg {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                        match handle_event(&text, &counters, &log) {
                            CdpIn::Reinject => {
                                send_text(&mut sink, evaluate_message(next_id, &latest_script))
                                    .await
                                    .map_err(|e| format!("导航后重注入发不出去：{e}"))?;
                                next_id += 1;
                                counters.inject_count.fetch_add(1, Ordering::Relaxed);
                                send_text(&mut sink, evaluate_message(next_id, VERIFY_EXPR))
                                    .await
                                    .map_err(|e| format!("核实发不出去：{e}"))?;
                                next_id += 1;
                                // 页面刚换过，旧的核实结果作废，等这一发回来再说。
                                verified.store(0, Ordering::Relaxed);
                            }
                            CdpIn::Verified { engine, dict } => {
                                verified.store(u64::from(engine), Ordering::Relaxed);
                                dict_in_page.store(dict as u64, Ordering::Relaxed);
                            }
                            CdpIn::Failed(reason) => {
                                verified.store(0, Ordering::Relaxed);
                                push_log(
                                    &log,
                                    "CDP",
                                    format!("{}：注入没跑起来 · {}", clip(&label, 40), clip(&reason, 160)),
                                );
                            }
                            CdpIn::Noop => {}
                        }
                    }
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(format!("WebSocket 出错：{e}")),
                }
            }
        }
        Ok::<(), String>(())
    }
    .await;
    if let Err(e) = result {
        push_log(&log, "CDP", format!("{}：{e}", clip(&label, 40)));
    }
    verified.store(0, Ordering::Relaxed);
    alive.store(false, Ordering::Relaxed);
}

/// 一条 CDP 消息该让连接做什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdpIn {
    /// 导航 / 新执行上下文 —— 要重新注入。
    Reinject,
    /// 回读核实的结果：页面里引擎在不在、字典有多少条。
    Verified { engine: bool, dict: u32 },
    /// 注入抛异常 / 命令被拒。
    Failed(String),
    /// 收下了，不用做什么。
    Noop,
}

/// 处理一条 CDP 消息。纯函数，可单测。
///
/// ⛔ **命令回执不许再丢掉。** 0.26.0 这里对没有 `method` 的消息一律返回 `None`，
/// 于是 `Runtime.evaluate` 的 `exceptionDetails` 从来没人看 —— 脚本在页面里炸了，
/// 界面照样显示「已注入 N 个页面」。回执里现在要认两件事：异常，和我们自己的核实结果。
pub fn handle_event(text: &str, counters: &Counters, log: &Mutex<VecDeque<UiLogLine>>) -> CdpIn {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return CdpIn::Noop;
    };
    let Some(method) = v.get("method").and_then(|m| m.as_str()) else {
        return handle_reply(&v);
    };
    match method {
        // 导航 / 新执行上下文之后立刻补打（Hub 换端口 Reload 会走这里）。
        "Runtime.executionContextCreated" | "Page.loadEventFired" | "Page.frameNavigated" => {
            CdpIn::Reinject
        }
        "Runtime.consoleAPICalled" => {
            let text: String = v["params"]["args"]
                .as_array()
                .map(|args| {
                    args.iter()
                        .filter_map(|a| a.get("value").and_then(|x| x.as_str()))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            if let Some(rest) = text.split_once("[EA_AA]") {
                counters.approve_count.fetch_add(1, Ordering::Relaxed);
                push_log(log, "AUTO-ACCEPT", rest.1.trim());
            } else if let Some(rest) = text.split_once("[EA_ALERT]") {
                counters.block_count.fetch_add(1, Ordering::Relaxed);
                push_log(log, "SECURITY ALERT", rest.1.trim());
            } else if let Some(rest) = text.split_once("[EA_OPT]") {
                push_log(log, "OPTION", rest.1.trim());
            }
            CdpIn::Noop
        }
        _ => CdpIn::Noop,
    }
}

/// 命令回执（没有 `method` 的那种）。三档：协议层报错、脚本抛异常、我们自己的核实结果。
fn handle_reply(v: &serde_json::Value) -> CdpIn {
    if let Some(msg) = v
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return CdpIn::Failed(msg.into());
    }
    let Some(result) = v.get("result") else {
        return CdpIn::Noop;
    };
    if let Some(ex) = result.get("exceptionDetails") {
        // 异常正文优先取 `exception.description`（带堆栈的那一行），没有就退回 `text`。
        let reason = ex
            .get("exception")
            .and_then(|e| e.get("description"))
            .and_then(|d| d.as_str())
            .or_else(|| ex.get("text").and_then(|t| t.as_str()))
            .unwrap_or("Runtime.evaluate 抛异常");
        return CdpIn::Failed(reason.into());
    }
    // 核实结果：`VERIFY_EXPR` 返回的是一个带 `__ea_verify` 标记的 JSON 字符串。
    let Some(raw) = result
        .get("result")
        .and_then(|r| r.get("value"))
        .and_then(|v| v.as_str())
    else {
        return CdpIn::Noop;
    };
    if !raw.contains("__ea_verify") {
        return CdpIn::Noop;
    }
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw) else {
        return CdpIn::Noop;
    };
    CdpIn::Verified {
        engine: parsed.get("run").and_then(|b| b.as_bool()).unwrap_or(false),
        dict: parsed
            .get("dict")
            .and_then(|d| d.as_u64())
            .unwrap_or(0)
            .min(u64::from(u32::MAX)) as u32,
    }
}

fn clip(s: &str, n: usize) -> String {
    let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= n {
        s
    } else {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 六个占位符必须全部换掉：留一个页面里就是 `ReferenceError`，而 `Runtime.evaluate`
    /// 不会报回来 —— 症状是「附加成功、什么都没发生」。
    #[test]
    fn the_injected_engine_keeps_its_placeholders_until_assembly_replaces_them_all() {
        for k in [
            "__EA_PREFER_OPTION__",
            "__EA_BLOCK_DANGEROUS__",
            "__EA_AUTO_ACCEPT__",
            "__EA_ENABLE_I18N__",
            "__EA_DICT__",
            "__EA_PATTERNS__",
        ] {
            assert_eq!(
                INJECT_TEMPLATE.matches(k).count(),
                1,
                "{k} 在模板里该恰好出现一次"
            );
        }
        let dict: BTreeMap<String, String> = [("Settings".to_string(), "设置".to_string())]
            .into_iter()
            .collect();
        let rules = parse_rules(DEFAULT_RULES);
        let cfg = UiConfig {
            prefer_option: 2,
            block_dangerous: false,
            ..UiConfig::default()
        };
        let script = assemble_script(&cfg, &dict, &rules);
        assert!(!script.contains("__EA_"), "还有占位符没换");
        assert!(script.contains("preferOption: 2,"));
        assert!(script.contains("blockDangerous: false,"));
        assert!(script.contains(r#"window.__ea_dict = {"Settings":"设置"};"#));
        assert!(script.contains(r#""id":"rm-rf""#));
        // 幂等锁与三个日志标签都在 —— Rust 侧靠它们收日志。
        for s in ["__ea_engine_running", "[EA_AA]", "[EA_ALERT]", "[EA_OPT]"] {
            assert!(script.contains(s), "{s}");
        }
    }

    /// 内置规则合法、每条都有 pattern；整组关掉就一条都不送进页面。
    #[test]
    fn built_in_rules_parse_and_the_group_switch_works() {
        let r = parse_rules(DEFAULT_RULES);
        assert!(r.enabled);
        assert!(r.rules.len() >= 6);
        assert!(r.active().len() == r.rules.iter().filter(|x| x.enabled).count());
        let mut off = r.clone();
        off.enabled = false;
        assert!(off.active().is_empty());
        // 空规则集退回内置那份（EasyAG 的口径）。
        assert_eq!(
            parse_rules(r#"{"version":1,"rules":[]}"#).rules.len(),
            r.rules.len()
        );
        assert_eq!(parse_rules("not json").rules.len(), r.rules.len());
    }

    /// 字典：内置两份都读得出来，`common` 覆盖 `ui_v2` 的同键（跟 EasyAG 的合并顺序一样）。
    #[test]
    fn built_in_dictionaries_merge_in_easyag_order() {
        let ui: BTreeMap<String, String> = serde_json::from_str(DICT_UI_V2).unwrap();
        let common: BTreeMap<String, String> = serde_json::from_str(DICT_COMMON).unwrap();
        assert!(ui.len() > 500, "{}", ui.len());
        assert!(!common.is_empty());
        let mut merged = BTreeMap::new();
        merged.extend(ui.clone());
        merged.extend(common.clone());
        for (k, v) in &common {
            assert_eq!(merged.get(k), Some(v), "common 里的「{k}」该覆盖 ui_v2");
        }
        assert_eq!(merged.get("Settings").map(String::as_str), Some("设置"));
    }

    #[test]
    fn devtools_port_file_first_line_is_the_port() {
        assert_eq!(read_port_file("9229\n/devtools/browser/abc\n"), Some(9229));
        assert_eq!(read_port_file("0\n/x"), None, "0 = 没起来");
        assert_eq!(read_port_file(""), None);
        assert_eq!(read_port_file("abc\n"), None);
    }

    #[test]
    fn console_events_are_counted_and_navigation_asks_for_reinjection() {
        let c = Counters::default();
        let log = Mutex::new(VecDeque::new());
        let ev = |method: &str, text: &str| {
            serde_json::json!({
                "method": method,
                "params": { "args": [{ "type": "string", "value": text }] }
            })
            .to_string()
        };
        assert_eq!(
            handle_event(
                &ev("Runtime.consoleAPICalled", "[EA_AA] 放行 · 无选项组 · ls"),
                &c,
                &log
            ),
            CdpIn::Noop
        );
        assert_eq!(
            handle_event(
                &ev(
                    "Runtime.consoleAPICalled",
                    "[EA_ALERT] 拦截高危指令[rm-rf]: rm -rf /"
                ),
                &c,
                &log
            ),
            CdpIn::Noop
        );
        assert_eq!(
            handle_event(&ev("Runtime.consoleAPICalled", "unrelated"), &c, &log),
            CdpIn::Noop
        );
        assert_eq!(c.approve_count.load(Ordering::Relaxed), 1);
        assert_eq!(c.block_count.load(Ordering::Relaxed), 1);
        let lines: Vec<String> = log
            .lock()
            .unwrap()
            .iter()
            .map(|l| l.category.clone())
            .collect();
        assert_eq!(lines, vec!["AUTO-ACCEPT", "SECURITY ALERT"]);
        for m in [
            "Runtime.executionContextCreated",
            "Page.loadEventFired",
            "Page.frameNavigated",
        ] {
            assert_eq!(handle_event(&ev(m, ""), &c, &log), CdpIn::Reinject, "{m}");
        }
        // 普通命令回执：认得出来，但没什么要做的。
        assert_eq!(
            handle_event(r#"{"id":1,"result":{}}"#, &c, &log),
            CdpIn::Noop
        );
    }

    /// ⛔ **注入抛异常不许再无声无息。**
    ///
    /// 0.26.0 对没有 `method` 的消息一律返回 `None`，`Runtime.evaluate` 的
    /// `exceptionDetails` 从来没人看 —— 脚本在页面里炸了，界面照样显示「已注入 N 个页面」。
    /// 这条钉着三档回执都认得出来。
    #[test]
    fn a_failed_injection_is_reported_instead_of_silently_counted_as_success() {
        let c = Counters::default();
        let log = Mutex::new(VecDeque::new());
        let thrown = serde_json::json!({
            "id": 7,
            "result": {
                "result": { "type": "object" },
                "exceptionDetails": {
                    "text": "Uncaught",
                    "exception": { "description": "ReferenceError: window is not defined" }
                }
            }
        })
        .to_string();
        assert_eq!(
            handle_event(&thrown, &c, &log),
            CdpIn::Failed("ReferenceError: window is not defined".into())
        );
        // 协议层直接拒绝（目标不支持 Runtime 之类）也算失败。
        assert_eq!(
            handle_event(
                r#"{"id":8,"error":{"code":-32000,"message":"Not allowed"}}"#,
                &c,
                &log
            ),
            CdpIn::Failed("Not allowed".into())
        );
        // 只有 text 没有 exception.description 时退回 text。
        assert!(matches!(
            handle_event(
                r#"{"id":9,"result":{"exceptionDetails":{"text":"Uncaught SyntaxError"}}}"#,
                &c,
                &log
            ),
            CdpIn::Failed(ref s) if s == "Uncaught SyntaxError"
        ));
    }

    /// 回读核实：`VERIFY_EXPR` 的结果认得出来，而且**不是**靠 id 记账，靠标记键。
    #[test]
    fn the_verification_read_back_is_what_proves_the_engine_is_really_in_the_page() {
        let c = Counters::default();
        let log = Mutex::new(VecDeque::new());
        let reply = |value: &str| {
            serde_json::json!({
                "id": 2,
                "result": { "result": { "type": "string", "value": value } }
            })
            .to_string()
        };
        assert_eq!(
            handle_event(
                &reply(r#"{"__ea_verify":1,"run":true,"dict":2252,"i18n":true}"#),
                &c,
                &log
            ),
            CdpIn::Verified {
                engine: true,
                dict: 2252
            }
        );
        // 注进去了但引擎没跑起来（比如注进了一个没有 window 的目标）。
        assert_eq!(
            handle_event(
                &reply(r#"{"__ea_verify":1,"run":false,"dict":0,"i18n":true}"#),
                &c,
                &log
            ),
            CdpIn::Verified {
                engine: false,
                dict: 0
            }
        );
        // 别人的字符串回执不许被当成核实结果。
        assert_eq!(handle_event(&reply("hello"), &c, &log), CdpIn::Noop);
    }

    /// 核实表达式是**面板自己的**，而且只读 —— 不许在里面改页面上的任何东西。
    ///
    /// 它跑在使用者自己的编辑器里，写进去一个赋值就是「面板偷改了我的界面」。
    #[test]
    fn the_verification_expression_only_reads() {
        assert!(VERIFY_EXPR.contains("__ea_verify"), "要有认领标记");
        assert!(VERIFY_EXPR.contains("__ea_engine_running"));
        for forbidden in ["=", "click(", "innerHTML", "nodeValue", "setAttribute", "="] {
            assert!(
                !VERIFY_EXPR.contains(forbidden),
                "回读表达式里不许出现「{forbidden}」"
            );
        }
    }

    #[test]
    fn page_filter_matches_easyag() {
        let t = |ws: &str, url: &str| Target {
            ws: ws.into(),
            url: url.into(),
            title: String::new(),
        };
        assert!(t("ws://127.0.0.1:1/devtools/page/a", "https://127.0.0.1:2/").is_page());
        assert!(!t("", "https://127.0.0.1:2/").is_page());
        assert!(!t("ws://x", "devtools://devtools/bundled/x").is_page());
        assert!(!t("ws://x", "data:text/html,<html>").is_page());
    }

    #[test]
    fn prefer_option_is_clamped_to_the_four_cards() {
        let c = normalize(UiConfig {
            prefer_option: 9,
            ..UiConfig::default()
        });
        assert_eq!(c.prefer_option, 4);
        let c = normalize(UiConfig {
            prefer_option: 0,
            ..UiConfig::default()
        });
        assert_eq!(c.prefer_option, 4);
        assert_eq!(
            normalize(UiConfig {
                prefer_option: 1,
                ..UiConfig::default()
            })
            .prefer_option,
            1
        );
    }

    #[test]
    fn saving_rules_rejects_empty_or_unbalanced_patterns_before_they_reach_the_page() {
        let mut r = parse_rules(DEFAULT_RULES);
        r.rules[0].pattern = "(unclosed".into();
        assert!(save_rules_check(&r).is_err());
        r.rules[0].pattern = "  ".into();
        assert!(save_rules_check(&r).is_err());
    }

    /// `save_rules` 的校验半段，拆出来单测（不落盘）。
    fn save_rules_check(rules: &DangerRules) -> Result<()> {
        for r in &rules.rules {
            if r.pattern.trim().is_empty() {
                return Err(GateError::Other("empty".into()));
            }
            if r.pattern.matches('(').count() != r.pattern.matches(')').count() {
                return Err(GateError::Other("unbalanced".into()));
            }
        }
        Ok(())
    }
}
