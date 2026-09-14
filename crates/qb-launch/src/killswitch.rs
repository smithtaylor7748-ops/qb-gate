//! 一键关闭所有 Claude。
//!
//! 沿用现有 ClaudeKillSwitch.ps1 的**双重证据**判定，这是这个文件的全部要点：
//!
//!   一个进程只有满足下面之一才会被收：
//!     ① 可执行文件由 Anthropic 签名（Authenticode 主体含 Anthropic）
//!     ② 命令行**同时**命中 `bridge.py` 与本项目的数据目录
//!     ③ 进程是 `node.exe`，**并且**命令行指进 `node_modules\@anthropic-ai\claude-code\`
//!        —— npm 装的 Claude Code。node.exe 是 OpenJS 签名的，①永远认不出它（v0.8.0）
//!
//! **绝不能按进程名杀。** 叫 `claude.exe` 的东西可能是别人的；叫 `python.exe`
//! 的更是满地都是。按名字杀会误伤到用户正在干的别的活，而这个按钮的语义是
//! 「关掉我的 Claude」，不是「清理系统」。
//!
//! 收完之后要重新上锁——否则下一次双击 exe 就真的能起来了。
//!
//! **枚举失败必须报出去，不能退化成「0 个进程」。** 这一条是 2026-09-09 补的：
//! 原来的 `unwrap_or_default()` 把「PowerShell 报错」「JSON 没解出来」
//! 「真的一个都没有」三件事压成同一个结果，界面上一律显示「0 个进程」。
//! 实机表现是 14 个 claude.exe 正在跑、面板信誓旦旦说没有 —— 一个说谎的
//! 空结果比一个报错难查得多。见 §7 新增的那条。

use crate::domain::{Client, IdentityKind, Session};
use crate::error::{GateError, Result};
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum Evidence {
    /// 可执行文件由 Anthropic 签名。
    AnthropicSigned,
    /// 命令行同时命中 bridge.py 与数据目录。
    BridgeAndDataDir,
    /// node.exe 跑的是 `@anthropic-ai/claude-code` 包 —— npm 装的 Claude Code。
    NpmPackage,
}

/// 这个进程属于哪一边。**只用于告诉用户「关掉了什么」，不参与判定。**
///
/// 切账户的确认框按它报数：「会关掉：桌面端 13 个、Claude Code 会话 2 个、酒馆桥接 1 个」。
/// （v0.8.0 还拿它决定切完「重新打开哪个」；v0.9.0 起切完不启动任何东西。）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "KillRole")]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// 桌面端本身，以及它 Code 页拉起的会话（`%APPDATA%\Claude*\claude-code\…`）
    Desktop,
    /// 其它 Claude Code 会话（终端里的、面板起的、npm 的、编辑器扩展的）
    Code,
    /// 酒馆桥接
    Bridge,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct KillTarget {
    /// 进程创建时间（Windows FILETIME）。**过 IPC 时是字符串。**
    ///
    /// 跟 `domain::Session::process_created` 同一个理由：这个值超过
    /// JS 的安全整数上限，当数字传过去会被静默截断。而它正是
    /// `sessions::terminate_verified` 用来判断「PID 是不是被复用了」的依据 ——
    /// 抹掉几位之后那个判断会错，且完全没有症状。
    ///
    /// 手写的那份前端类型把它声明成 `number`，这个问题就一直藏着，
    /// 直到 B1 把类型换成 ts-rs 生成的才露出来（生成的是 `bigint`）。
    #[serde(with = "qb_contract::filetime")]
    #[ts(type = "string")]
    pub process_created: u64,
    pub pid: u32,
    pub name: String,
    pub path: Option<String>,
    pub evidence: Evidence,
    pub role: Role,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct KillReport {
    pub targets: Vec<KillTarget>,
    pub killed: Vec<u32>,
    pub failed: Vec<(u32, String)>,
    pub relocked: usize,
    /// 枚举到但**故意没动**的进程，连同放过的理由。界面上要显示出来。
    pub spared: Vec<String>,
}

/// PowerShell 枚举出来的一行原始进程信息。
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct RawProcess {
    #[serde(default, rename = "Created")]
    pub created: u64,
    #[serde(default, rename = "ProcessId")]
    pub pid: u32,
    /// 父进程 PID。只为了算「自己的祖先链」，见 `triage`。
    #[serde(default, rename = "ParentId")]
    pub ppid: u32,
    #[serde(default, rename = "Name")]
    pub name: String,
    #[serde(default, rename = "ExecutablePath")]
    pub path: Option<String>,
    #[serde(default, rename = "CommandLine")]
    pub cmdline: Option<String>,
    /// Authenticode 签名主体。取不到就是 None，**取不到不等于没签名**，
    /// 但我们宁可放过也不误杀。
    #[serde(default, rename = "Signer")]
    pub signer: Option<String>,
}

/// 判定单个进程是否该收。纯函数，全部分支可单测。
///
/// `data_dir` 是本项目的桥接数据目录，比对时统一转小写做包含匹配 ——
/// Windows 路径大小写不敏感，但 `Win32_Process` 报回来的大小写并不稳定。
pub fn classify(p: &RawProcess, data_dir: &str) -> Option<Evidence> {
    // ① Anthropic 签名
    if let Some(signer) = &p.signer {
        if signer.to_lowercase().contains("anthropic") {
            return Some(Evidence::AnthropicSigned);
        }
    }

    // ② 命令行双重命中：bridge.py 与数据目录，缺一不可
    if let Some(cmd) = &p.cmdline {
        let c = cmd.to_lowercase();
        let d = data_dir.to_lowercase();
        if c.contains("bridge.py") && !d.is_empty() && c.contains(&d) {
            return Some(Evidence::BridgeAndDataDir);
        }
    }

    // ③ npm 装的 Claude Code：进程是 node.exe **并且**命令行指进那个包。
    //    两条缺一不可 —— 光是 node.exe 满地都是；光是命令行里有这串字，
    //    可能只是别的程序把它当参数传了一下。
    if p.name.eq_ignore_ascii_case("node.exe") {
        if let Some(cmd) = &p.cmdline {
            let c = cmd.to_lowercase().replace('/', "\\");
            if c.contains("\\node_modules\\@anthropic-ai\\claude-code\\") {
                return Some(Evidence::NpmPackage);
            }
        }
    }

    None
}

/// 一个该收的进程属于哪一边。见 [`Role`]。纯函数，根目录由调用方给。
pub fn role_of(
    p: &RawProcess,
    e: Evidence,
    local: Option<&std::path::Path>,
    roaming: Option<&std::path::Path>,
) -> Role {
    use crate::install::inventory::{is_desktop_managed_path, is_desktop_process_path};
    match e {
        Evidence::BridgeAndDataDir => Role::Bridge,
        Evidence::NpmPackage => Role::Code,
        Evidence::AnthropicSigned => {
            let Some(path) = p.path.as_deref().map(std::path::Path::new) else {
                return Role::Code;
            };
            if is_desktop_process_path(path, local) || is_desktop_managed_path(path, roaming) {
                Role::Desktop
            } else {
                Role::Code
            }
        }
    }
}

fn data_dir() -> String {
    dirs::data_local_dir()
        .map(|p| p.join("ClaudeTavernBridge").display().to_string())
        .unwrap_or_default()
}

/// 一次枚举的结果。
///
/// 除了候选进程，还带上**面板自己的祖先链** —— `triage` 要用它把自己摘出去。
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Snapshot {
    #[serde(default, rename = "SelfChain")]
    pub self_chain: Vec<u32>,
    #[serde(default, rename = "Procs")]
    pub procs: Vec<RawProcess>,
}

/// 一个进程的处置结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Triage {
    Kill(Evidence),
    /// 放过，附上理由 —— 界面上要原样显示。
    Spare(String),
}

/// 在 `classify` 之上再加一层「不许自杀」的否决。纯函数，可单测。
///
/// `taskkill /T` 收的是整棵子树。面板要是被人从某个 Claude 会话里拉起来
/// （开发时很常见），那个会话就是面板的祖先，收掉它会把面板一起带走 ——
/// 而账户切换正好会在「联结点已经摘掉、还没重建」的中间态上调这个函数，
/// 死在那一刻等于把用户的槽位留在半残状态。
///
/// 否决在 `classify` **之后**表达、之前生效：证据够不够是一回事，
/// 能不能动是另一回事，两件事分开才说得清放过的理由。
pub fn triage(p: &RawProcess, data_dir: &str, self_chain: &[u32]) -> Triage {
    if self_chain.contains(&p.pid) {
        return Triage::Spare(format!(
            "PID {} {} —— 面板自己或它的祖先，收了会把面板一起带走，放过",
            p.pid, p.name
        ));
    }
    match classify(p, data_dir) {
        Some(e) => Triage::Kill(e),
        None => Triage::Spare(format!(
            "PID {} {} —— 两条证据都不满足，放过",
            p.pid, p.name
        )),
    }
}

/// 枚举候选进程并附带签名信息。
///
/// 只枚举 `claude.exe` / `Update.exe` / `python.exe` / `pythonw.exe` / `node.exe`
/// 这几类，是为了少调几次 `Get-AuthenticodeSignature`（那个很慢）——
/// **缩小的是枚举范围，不是判定标准**。判定仍然只认那两条证据。
///
/// `node.exe` 是 2026-09-09 补的：npm 装的 Claude Code 跑起来就是 node.exe，
/// 原来的清单里没有它，注释里却写着有 —— 注释对了，代码漏了。
const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'

# 输出编码钉死成 UTF-8。这个 PowerShell 是 Rust 用 CREATE_NO_WINDOW 拉起来的，
# 没有控制台，[Console]::OutputEncoding 会落到系统代码页上，中文路径到了
# Rust 那边就成了一串替换字符。钉死之后跟系统装的是哪个区域设置无关。
try { [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false } catch { }

$all = @(Get-CimInstance Win32_Process -ErrorAction Stop)

# 面板自己的祖先链，从本进程一路往上走到头。
$parent = @{}
foreach ($p in $all) { $parent[[int]$p.ProcessId] = [int]$p.ParentProcessId }
$chain = New-Object System.Collections.ArrayList
$cur = __SELF_PID__
$hops = 0
while ($cur -gt 0 -and $parent.ContainsKey($cur) -and $hops -lt 64) {
  [void]$chain.Add($cur)
  $cur = $parent[$cur]
  $hops++
}

$names = @('claude.exe','Update.exe','python.exe','pythonw.exe','node.exe')

# 按可执行文件路径缓存签名结果。本机十几个 claude.exe 共用同一个路径，
# 不缓存就是十几次 Get-AuthenticodeSignature，每次几百毫秒。
$cache = @{}
$rows = @(foreach ($p in $all) {
  if ($names -notcontains $p.Name) { continue }
  $signer = $null
  $exe = $p.ExecutablePath
  if ($exe) {
    if ($cache.ContainsKey($exe)) {
      $signer = $cache[$exe]
    } else {
      try {
        $sig = Get-AuthenticodeSignature -LiteralPath $exe -ErrorAction Stop
        if ($sig -and $sig.SignerCertificate) { $signer = $sig.SignerCertificate.Subject }
      } catch { }
      $cache[$exe] = $signer
    }
  }
  [pscustomobject]@{
    ProcessId      = [int]$p.ProcessId
    ParentId       = [int]$p.ParentProcessId
    Name           = [string]$p.Name
    ExecutablePath = $exe
    CommandLine    = $p.CommandLine
    Signer         = $signer
    Created        = $(try { (Get-Process -Id $p.ProcessId -ErrorAction Stop).StartTime.ToUniversalTime().ToFileTimeUtc() } catch { 0 })
  }
})

# 这一行有两个坑，都实机踩过：
#   1. `-AsArray` 是 PowerShell 6.2+ 才有的参数。Windows PowerShell 5.1 上
#      直接抛「找不到匹配参数 AsArray」，整条管线报错、stdout 全空 ——
#      这就是「一键关闭一直显示 0 个进程」的根因。
#   2. 管道形式 `$rows | ConvertTo-Json` 在只有一个元素时会退化成裸对象，
#      Rust 侧按数组解就失败，又变回 0 个。
#   `-InputObject @(...)` 是 5.1 上唯一两头都对的写法：空数组出 `[]`，
#   单元素出 `[{...}]`，嵌在对象里的数组也保得住。
ConvertTo-Json -InputObject ([pscustomobject]@{ SelfChain = @($chain); Procs = $rows }) -Compress -Depth 5
"#;

#[cfg(windows)]
async fn enumerate() -> Result<Snapshot> {
    let script = SCRIPT.replace("__SELF_PID__", &std::process::id().to_string());
    let out = crate::process::hidden_tokio(tokio::process::Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .await
        .map_err(|e| GateError::Other(format!("启动 PowerShell 枚举进程失败：{e}")))?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let text = stdout.trim();

    // 一个进程都没匹配上也会输出 `[]`，所以空输出只可能是出错了。
    // 这里绝对不能吞 —— 吞掉就退回成「面板信誓旦旦说 0 个」的老毛病。
    if text.is_empty() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let err = stderr.trim();
        return Err(GateError::Other(format!(
            "枚举进程没拿到任何输出（PowerShell 退出码 {}）：{}",
            out.status.code().unwrap_or(-1),
            if err.is_empty() {
                "stderr 也是空的"
            } else {
                err
            }
        )));
    }

    serde_json::from_str::<Snapshot>(text).map_err(|e| {
        GateError::Other(format!(
            "解析进程清单失败：{e}；PowerShell 原样输出前 300 字：{}",
            text.chars().take(300).collect::<String>()
        ))
    })
}

#[cfg(not(windows))]
async fn enumerate() -> Result<Snapshot> {
    Ok(Snapshot::default())
}

/// 只看不动：列出会被收的进程，交给界面让用户确认。
///
/// **返回 `Result`。** 枚举失败要能一路冒到界面上 —— 老版本这里吞成空清单，
/// 结果是十几个 claude.exe 正在跑、面板显示「0 个进程」，而且没有任何线索。
pub async fn preview() -> Result<KillReport> {
    let dd = data_dir();
    let snap = enumerate().await?;
    let local = dirs::data_local_dir();
    let roaming = dirs::config_dir();
    let mut targets = Vec::new();
    // 放过的理由分两桶。界面上只显示前几条，而「面板自己被放过了」是用户
    // 唯一会追问的那一条 —— 它必须排在十几个「不满足证据」前面，
    // 否则正好被截断在看不见的地方。
    let mut spared_self = Vec::new();
    let mut spared_other = Vec::new();

    for p in snap.procs {
        let on_self_chain = snap.self_chain.contains(&p.pid);
        match triage(&p, &dd, &snap.self_chain) {
            Triage::Kill(evidence) => {
                let role = role_of(&p, evidence, local.as_deref(), roaming.as_deref());
                targets.push(KillTarget {
                    process_created: p.created,
                    pid: p.pid,
                    name: p.name,
                    path: p.path,
                    evidence,
                    role,
                })
            }
            Triage::Spare(why) => {
                if on_self_chain {
                    spared_self.push(why);
                } else {
                    spared_other.push(why);
                }
            }
        }
    }

    let mut spared = spared_self;
    spared.extend(spared_other);

    Ok(KillReport {
        targets,
        killed: Vec::new(),
        failed: Vec::new(),
        relocked: 0,
        spared,
    })
}

// ------------------------------------------------------------ 桌面端进程

/// 列 `claude.exe` / `Update.exe` 的 PID 与路径。**脚本里不拼任何路径** ——
/// 过滤在 Rust 里做（`inventory::is_desktop_process_path`）。
///
/// 旧版 `detect::kill_desktop_processes` 把前缀拼进 PowerShell 的单引号串再
/// `-like`，用户名带 `'` 就是语法错误、带 `[` `]` 就成了通配符，而输出和错误
/// 全被丢掉。现在枚举失败会返回 `Err`，调用方必须写进日志。
///
/// 脚本里**一个双引号都没有**：它整段作为 `-Command` 的参数传过去，双引号要经过
/// 一轮命令行转义，Windows PowerShell 5.1 在这一步上有过把引号吃掉的前科。
/// 所以不用 `-Filter "Name='…'"`，改成在管道里按名字筛。
const DESKTOP_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
try { [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false } catch { }
$names = @('claude.exe', 'Update.exe')
$rows = @(Get-CimInstance Win32_Process -ErrorAction Stop | Where-Object { $names -contains $_.Name } | ForEach-Object {
  [pscustomobject]@{ ProcessId = [int]$_.ProcessId; ExecutablePath = $_.ExecutablePath }
})
ConvertTo-Json -InputObject $rows -Compress
"#;

#[derive(Debug, serde::Deserialize)]
struct DesktopRow {
    #[serde(rename = "ProcessId")]
    pid: u32,
    #[serde(default, rename = "ExecutablePath")]
    path: Option<String>,
}

/// 从 PowerShell 的输出里挑出桌面端进程。纯函数，可单测。
pub fn parse_desktop_rows(
    text: &str,
    local: Option<&std::path::Path>,
) -> Result<Vec<(u32, String)>> {
    let text = text.trim();
    // 空数组也会输出 `[]`，所以空串只可能是出错了 —— 同 §7.17。
    if text.is_empty() {
        return Err(GateError::Other("枚举桌面端进程没拿到任何输出".into()));
    }
    let rows: Vec<DesktopRow> = serde_json::from_str(text).map_err(|e| {
        GateError::Other(format!(
            "解析桌面端进程清单失败：{e}；原样输出前 200 字：{}",
            text.chars().take(200).collect::<String>()
        ))
    })?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let p = r.path?;
            crate::install::inventory::is_desktop_process_path(std::path::Path::new(&p), local)
                .then_some((r.pid, p))
        })
        .collect())
}

/// 正在跑的桌面端进程。**枚举失败返回 `Err`，不许退化成「没有」。**
#[cfg(windows)]
pub fn desktop_processes() -> Result<Vec<(u32, String)>> {
    let out = crate::process::hidden_std(std::process::Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", DESKTOP_SCRIPT])
        .output()
        .map_err(|e| GateError::Other(format!("启动 PowerShell 枚举桌面端进程失败：{e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.trim().is_empty() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(GateError::Other(format!(
            "枚举桌面端进程没拿到任何输出（PowerShell 退出码 {}）：{}",
            out.status.code().unwrap_or(-1),
            if err.is_empty() {
                "stderr 也是空的".into()
            } else {
                err
            }
        )));
    }
    parse_desktop_rows(&stdout, dirs::data_local_dir().as_deref())
}

#[cfg(not(windows))]
pub fn desktop_processes() -> Result<Vec<(u32, String)>> {
    Ok(Vec::new())
}

/// 关掉桌面端。看门狗「查不到 IP 立即关闭桌面端」走的就是这条。
///
/// 返回关掉了几个。**枚举失败或一个都没关掉（而明明有）时返回 `Err`** ——
/// 调用方要写进日志，不能再出现「日志说收摊了、桌面端还开着」。
#[cfg(windows)]
pub fn close_desktop() -> Result<usize> {
    let procs = desktop_processes()?;
    let mut closed = 0usize;
    let mut last_err = None;
    for (pid, _) in &procs {
        // /T：存根会拉起 app-* 下的真身，只收父进程会留下一堆孤儿。
        let out = crate::process::hidden_std(std::process::Command::new("taskkill"))
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
        match out {
            Ok(o) if o.status.success() => closed += 1,
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                // 被前一个 /T 当子进程一起收走了 —— 这不是失败。
                if err.contains("找不到") || err.to_lowercase().contains("not found") {
                    closed += 1;
                } else {
                    last_err = Some(format!("PID {pid}：{err}"));
                }
            }
            Err(e) => last_err = Some(format!("PID {pid}：{e}")),
        }
    }
    // 有一个没关掉，桌面端就可能还开着 —— 必须报出去，不能拿「关掉了 N 个」盖过去。
    match last_err {
        Some(e) => Err(GateError::Other(format!(
            "关掉了 {closed} 个桌面端进程，但还有没关掉的：{e}"
        ))),
        None => Ok(closed),
    }
}

#[cfg(not(windows))]
pub fn close_desktop() -> Result<usize> {
    Ok(0)
}

/// 真正执行：收掉全部满足证据的 Claude 进程，收完重新上锁。
pub async fn execute() -> Result<KillReport> {
    execute_scoped(false).await
}
pub async fn preview_official() -> Result<KillReport> {
    let mut report = preview().await?;
    report
        .targets
        .retain(|t| !crate::sessions::owns_pid(t.pid, crate::domain::IdentityKind::Relay));
    Ok(report)
}
pub async fn execute_official() -> Result<KillReport> {
    execute_scoped(true).await
}
async fn execute_scoped(official_only: bool) -> Result<KillReport> {
    let mut report = preview().await?;
    if official_only {
        report
            .targets
            .retain(|t| !crate::sessions::owns_pid(t.pid, crate::domain::IdentityKind::Relay));
    }
    for t in &report.targets {
        match crate::sessions::terminate_verified(t.pid, t.process_created) {
            Ok(()) => report.killed.push(t.pid),
            Err(e) => report.failed.push((t.pid, e.to_string())),
        }
    }
    if !official_only {
        report.relocked = crate::gate::lock_all()?;
    }
    Ok(report)
}

// ------------------------------------------------------- 切账户之前的清场

/// 这个会话归「切账户之前必须先收掉」的范畴吗。
///
/// **官方身份，而且不是 Codex。** 两个条件都有理由：
///
/// * 中转会话不算 —— 它用的是你自己买的 Key、打第三方端点，跟切哪个官方
///   账户毫无关系，收它只会把正在写的对话弄丢（`stops_with_gate()` 那条
///   不变量的同一条推论）。
/// * Codex 不算 —— 它有自己的登录目录，官方槽位换不到它头上。
///
/// # 为什么要提成一个函数
///
/// `clear_for_switch` 里原来把这个条件**内联写了两遍**：一遍给
/// `sessions::ensure_verified` 的过滤器，一遍给「要停掉哪些」的清单。
/// 两份就会分叉，而分叉的症状是「校验时算的是一批、真去停的是另一批」——
/// 没有任何报错，只是某个会话没被收掉，然后在它脚下换了资料目录。
pub fn needs_clearing(s: &Session) -> bool {
    s.context.identity_kind == IdentityKind::Official && s.context.client != Client::Codex
}

/// 清场没清干净时给使用者看的那句话。
///
/// **必须带上「账户未切换」四个字。** 调用方确实会停下（返回 Err），
/// 但使用者看到的如果只是「PID 1234: 拒绝访问」，他没法判断自己现在
/// 到底在哪个账户上 —— 而这正是他点那一下想知道的事。
pub fn unfinished_detail(failed: &[(u32, String)]) -> String {
    format!(
        "清场未完成，账户未切换：{}",
        failed
            .iter()
            .map(|(pid, reason)| format!("PID {pid}: {reason}"))
            .collect::<Vec<_>>()
            .join("；")
    )
}

#[cfg(test)]
mod tests {
    use crate::domain::LaunchContext;

    fn session(kind: IdentityKind, client: Client, state: &str) -> Session {
        Session {
            id: "s1".into(),
            context: LaunchContext {
                client,
                identity_kind: kind,
                identity_id: "slot".into(),
                config_dir: String::new(),
                working_dir: String::new(),
            },
            pid: 1234,
            process_created: 0,
            started_at: String::new(),
            state: state.into(),
            gated: true,
            detail: String::new(),
            config_revision: 0,
        }
    }

    #[test]
    fn only_official_non_codex_sessions_are_cleared_before_a_switch() {
        assert!(needs_clearing(&session(
            IdentityKind::Official,
            Client::ClaudeCode,
            "running"
        )));
        assert!(needs_clearing(&session(
            IdentityKind::Official,
            Client::ClaudeDesktop,
            "running"
        )));
    }

    #[test]
    fn a_relay_session_is_never_cleared_for_an_account_switch() {
        // 中转会话用的是你自己买的 Key、打第三方端点，跟切哪个官方账户
        // 毫无关系。收它换不到任何保护，却会把正在写的对话弄丢。
        assert!(!needs_clearing(&session(
            IdentityKind::Relay,
            Client::ClaudeCode,
            "running"
        )));
    }

    #[test]
    fn codex_is_not_cleared_either() {
        // Codex 有自己的登录目录，官方槽位换不到它头上。
        assert!(!needs_clearing(&session(
            IdentityKind::Official,
            Client::Codex,
            "running"
        )));
    }

    #[test]
    fn the_unfinished_message_says_the_account_was_not_switched() {
        // 只报「PID 1234: 拒绝访问」的话，使用者没法判断自己现在在哪个账户上 ——
        // 而那正是他点那一下想知道的事。
        let text = unfinished_detail(&[(1234, "拒绝访问".into()), (5678, "超时".into())]);
        assert!(text.contains("账户未切换"), "{text}");
        assert!(text.contains("1234") && text.contains("拒绝访问"), "{text}");
        assert!(text.contains("5678"), "失败的每一个都要列出来：{text}");
        assert!(!text.contains("**"), "传给界面的是纯文本");
    }
    use super::*;

    const DD: &str = r"C:\Users\me\AppData\Local\ClaudeTavernBridge";

    fn proc(name: &str, cmd: Option<&str>, signer: Option<&str>) -> RawProcess {
        RawProcess {
            created: 1,
            pid: 1234,
            ppid: 1,
            name: name.into(),
            path: Some(format!(r"C:\x\{name}")),
            cmdline: cmd.map(String::from),
            signer: signer.map(String::from),
        }
    }

    #[test]
    fn anthropic_signature_is_enough() {
        let p = proc(
            "claude.exe",
            None,
            Some("CN=Anthropic PBC, O=Anthropic PBC"),
        );
        assert_eq!(classify(&p, DD), Some(Evidence::AnthropicSigned));
    }

    #[test]
    fn bridge_plus_data_dir_is_enough() {
        let cmd = format!(r"python.exe -u C:\proj\bridge.py --data-dir {DD} --port 5001");
        let p = proc("python.exe", Some(&cmd), None);
        assert_eq!(classify(&p, DD), Some(Evidence::BridgeAndDataDir));
    }

    #[test]
    fn bridge_without_data_dir_is_spared() {
        // 别人的 bridge.py —— 只有一条证据，不能动。
        let p = proc(
            "python.exe",
            Some(r"python.exe -u D:\other\bridge.py"),
            None,
        );
        assert_eq!(classify(&p, DD), None);
    }

    #[test]
    fn data_dir_without_bridge_is_spared() {
        let cmd = format!(r"python.exe -u other.py --data-dir {DD}");
        let p = proc("python.exe", Some(&cmd), None);
        assert_eq!(classify(&p, DD), None);
    }

    #[test]
    fn name_alone_never_qualifies() {
        // 这条是本文件存在的理由：叫 claude.exe 不构成任何证据。
        let p = proc("claude.exe", Some("claude.exe --help"), None);
        assert_eq!(classify(&p, DD), None);
    }

    #[test]
    fn unrelated_signer_is_spared() {
        let p = proc("python.exe", None, Some("CN=Python Software Foundation"));
        assert_eq!(classify(&p, DD), None);
    }

    #[test]
    fn path_case_differences_still_match() {
        let cmd =
            r"python.exe -u bridge.py --data-dir c:\users\me\appdata\local\claudetavernbridge";
        let p = proc("python.exe", Some(cmd), None);
        assert_eq!(classify(&p, DD), Some(Evidence::BridgeAndDataDir));
    }

    // ---------------------------------------------------------- triage

    #[test]
    fn self_and_ancestors_are_never_killed() {
        // 证据是够的 —— 但它是面板自己的祖先，收了会把面板一起带走。
        let p = proc("claude.exe", None, Some("CN=Anthropic PBC"));
        match triage(&p, DD, &[9999, 1234, 7]) {
            Triage::Spare(why) => assert!(why.contains("祖先"), "理由要说清为什么放过：{why}"),
            other => panic!("祖先链上的进程不能被收：{other:?}"),
        }
    }

    #[test]
    fn evidence_still_kills_when_not_an_ancestor() {
        let p = proc("claude.exe", None, Some("CN=Anthropic PBC"));
        assert_eq!(
            triage(&p, DD, &[9999, 7]),
            Triage::Kill(Evidence::AnthropicSigned)
        );
    }

    #[test]
    fn spare_reason_survives_for_no_evidence() {
        let p = proc("claude.exe", Some("claude.exe --help"), None);
        match triage(&p, DD, &[]) {
            Triage::Spare(why) => assert!(why.contains("两条证据")),
            other => panic!("没证据就必须放过：{other:?}"),
        }
    }

    // ------------------------------------------------- PowerShell 输出形状
    //
    // 这三条守的是「一键关闭一直显示 0 个进程」那个坑：Windows PowerShell 5.1
    // 的 ConvertTo-Json 输出形状很挑，解不出来就静默变成 0 个。

    #[test]
    fn snapshot_parses_multi_element_output() {
        let raw = r#"{"SelfChain":[10,4],"Procs":[
            {"ProcessId":1,"ParentId":10,"Name":"claude.exe","ExecutablePath":"C:\\a.exe","CommandLine":null,"Signer":"CN=Anthropic PBC"},
            {"ProcessId":2,"ParentId":1,"Name":"node.exe","ExecutablePath":null,"CommandLine":"node x","Signer":null}]}"#;
        let snap: Snapshot = serde_json::from_str(raw).expect("多元素必须解得开");
        assert_eq!(snap.self_chain, vec![10, 4]);
        assert_eq!(snap.procs.len(), 2);
        assert_eq!(snap.procs[1].name, "node.exe");
        // 转义反斜杠要还原成真路径，不能停在字面量上。
        assert_eq!(snap.procs[0].path.as_deref(), Some(r"C:\a.exe"));
    }

    #[test]
    fn snapshot_parses_single_element_output() {
        // 5.1 上单元素最容易退化成裸对象。真退化了这条会先红。
        let raw =
            r#"{"SelfChain":[10],"Procs":[{"ProcessId":1,"ParentId":10,"Name":"claude.exe"}]}"#;
        let snap: Snapshot = serde_json::from_str(raw).expect("单元素必须解得开");
        assert_eq!(snap.procs.len(), 1);
        assert_eq!(snap.self_chain, vec![10]);
    }

    #[test]
    fn snapshot_parses_empty_output() {
        let snap: Snapshot = serde_json::from_str(r#"{"SelfChain":[],"Procs":[]}"#).unwrap();
        assert!(snap.procs.is_empty());
        assert!(snap.self_chain.is_empty());
    }

    #[test]
    fn script_does_not_use_powershell_7_only_switches() {
        // -AsArray 要到 PowerShell 6.2 才有。5.1 上它让整条管线报错、
        // stdout 全空，而调用方看到的只是「0 个进程」。
        // 注释里提到 -AsArray 是在解释为什么不能用它，所以只能断言
        // **真正那一行调用**没用上，不能断言全文没出现过。
        let call = SCRIPT
            .lines()
            .find(|l| l.trim_start().starts_with("ConvertTo-Json"))
            .expect("脚本里得有一行 ConvertTo-Json");
        assert!(!call.contains("-AsArray"), "5.1 上没有 -AsArray：{call}");
        assert!(
            call.contains("-InputObject"),
            "必须用 -InputObject，管道形式单元素会退化成裸对象：{call}"
        );
    }

    #[test]
    fn script_covers_node_exe() {
        // npm 装的 Claude Code 跑起来是 node.exe，漏了它就漏掉一整类会话。
        assert!(SCRIPT.contains("node.exe"));
    }

    #[test]
    fn script_has_a_self_pid_placeholder_to_fill() {
        // 忘了替换的话 PowerShell 会在 $cur = __SELF_PID__ 上语法报错，
        // 那时候祖先链是空的，面板就可能把自己收掉。
        assert!(SCRIPT.contains("__SELF_PID__"));
        let filled = SCRIPT.replace("__SELF_PID__", "4242");
        assert!(!filled.contains("__SELF_PID__"));
    }

    #[test]
    fn empty_data_dir_does_not_match_everything() {
        // data_dir 取不到时不能退化成「命令行里有 bridge.py 就杀」。
        let p = proc("python.exe", Some("python.exe -u bridge.py"), None);
        assert_eq!(classify(&p, ""), None);
    }

    // ---------------------------------------------------------- ③ npm

    #[test]
    fn npm_claude_code_is_recognised_by_runtime_plus_package_path() {
        // node.exe 是 OpenJS 签名的，① 永远认不出 npm 装的 Claude Code。
        let cmd = r#""C:\Program Files\nodejs\node.exe" C:\Users\me\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\cli.js"#;
        let p = proc("node.exe", Some(cmd), Some("CN=OpenJS Foundation"));
        assert_eq!(classify(&p, DD), Some(Evidence::NpmPackage));
        // 正斜杠写法也要认。
        let p = proc(
            "node.exe",
            Some("node C:/x/node_modules/@anthropic-ai/claude-code/cli.js"),
            None,
        );
        assert_eq!(classify(&p, DD), Some(Evidence::NpmPackage));
    }

    #[test]
    fn node_alone_or_package_path_alone_is_not_enough() {
        // 只有 node.exe：满地都是。
        let p = proc("node.exe", Some(r"node C:\work\server.js"), None);
        assert_eq!(classify(&p, DD), None);
        // 只有那串路径、但进程不是 node：可能只是别的程序把它当参数传了一下。
        let p = proc(
            "code.exe",
            Some(
                r"code.exe C:\Users\me\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\README.md",
            ),
            None,
        );
        assert_eq!(classify(&p, DD), None);
        // 名字相近的别的包不算。
        let p = proc(
            "node.exe",
            Some(r"node C:\x\node_modules\@anthropic-ai\sdk\index.js"),
            None,
        );
        assert_eq!(classify(&p, DD), None);
    }

    // ---------------------------------------------------------- 角色

    #[test]
    fn roles_tell_desktop_from_code() {
        let local = std::path::Path::new(r"C:\Users\me\AppData\Local");
        let roaming = std::path::Path::new(r"C:\Users\me\AppData\Roaming");
        let at = |path: &str| RawProcess {
            created: 1,
            pid: 1,
            ppid: 0,
            name: "claude.exe".into(),
            path: Some(path.into()),
            cmdline: None,
            signer: Some("CN=Anthropic PBC".into()),
        };
        let role = |path: &str| {
            role_of(
                &at(path),
                Evidence::AnthropicSigned,
                Some(local),
                Some(roaming),
            )
        };

        assert_eq!(
            role(r"C:\Users\me\AppData\Local\AnthropicClaude\app-1.49585.0\claude.exe"),
            Role::Desktop
        );
        // 桌面端 Code 页拉起的会话归桌面端。
        assert_eq!(
            role(r"C:\Users\me\AppData\Roaming\Claude\claude-code\2.1.266\claude.exe"),
            Role::Desktop
        );
        assert_eq!(role(r"C:\Users\me\.local\bin\claude.exe"), Role::Code);

        let bridge = proc("python.exe", Some("x"), None);
        assert_eq!(
            role_of(
                &bridge,
                Evidence::BridgeAndDataDir,
                Some(local),
                Some(roaming)
            ),
            Role::Bridge
        );
        assert_eq!(
            role_of(&bridge, Evidence::NpmPackage, Some(local), Some(roaming)),
            Role::Code
        );
    }

    // ------------------------------------------------------- 桌面端进程

    #[test]
    fn desktop_rows_are_filtered_in_rust_not_in_the_script() {
        let local = std::path::Path::new(r"C:\Users\Demo'Quote\AppData\Local");
        let raw = r#"[
            {"ProcessId":10,"ExecutablePath":"C:\\Users\\Demo'Quote\\AppData\\Local\\AnthropicClaude\\app-1.2.3\\claude.exe"},
            {"ProcessId":11,"ExecutablePath":"C:\\Users\\Demo'Quote\\.local\\bin\\claude.exe"},
            {"ProcessId":12,"ExecutablePath":null},
            {"ProcessId":13,"ExecutablePath":"C:\\Users\\Demo'Quote\\AppData\\Local\\AnthropicClaude\\Update.exe"}
        ]"#;
        let got = parse_desktop_rows(raw, Some(local)).unwrap();
        assert_eq!(
            got.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
            vec![10, 13]
        );
        // 用户名里的单引号不能再让任何东西坏掉 —— 脚本里压根没有路径。
        assert!(!DESKTOP_SCRIPT.contains("-like"));
    }

    #[test]
    fn desktop_rows_distinguish_none_from_error() {
        assert!(parse_desktop_rows("[]", None).unwrap().is_empty());
        assert!(
            parse_desktop_rows("", None).is_err(),
            "空输出只可能是出错了"
        );
        assert!(parse_desktop_rows("not json", None).is_err());
    }

    #[test]
    fn desktop_script_uses_the_51_safe_json_form() {
        let call = DESKTOP_SCRIPT
            .lines()
            .find(|l| l.trim_start().starts_with("ConvertTo-Json"))
            .unwrap();
        assert!(
            call.contains("-InputObject") && !call.contains("-AsArray"),
            "{call}"
        );
        // 整段脚本走 -Command 传参，双引号要过一轮命令行转义 —— 干脆一个都不要。
        assert!(!DESKTOP_SCRIPT.contains('"'), "脚本里不许有双引号");
    }
}
