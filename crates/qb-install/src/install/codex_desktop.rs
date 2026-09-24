//! Codex desktop inventory and explicit process control. The Store package identity
//! plus its exact executable path are the evidence; a process name is never enough.
use crate::error::{GateError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use ts_rs::TS;

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct CodexDesktop {
    pub executable: Option<String>,
    pub version: Option<String>,
    pub running: bool,
    pub processes: Vec<CodexDesktopProcess>,
}

/// 一个 Codex 桌面端**实例**（Electron 主进程；渲染 / GPU 这些子进程不单列）。
#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct CodexDesktopProcess {
    pub pid: u32,
    pub started: String,
    /// 这个实例用的 Electron 资料目录（`--user-data-dir`）。`None` = 默认那份
    /// （`%APPDATA%\Codex`：开始菜单、任务栏、`codex://` 链接、别的程序起的都是它）。
    pub profile: Option<String>,
    /// 是不是面板起的（资料目录在面板自己的目录下，见 [`is_panel_profile`]）。
    ///
    /// 起槽位 / 切槽位之前只关这些 —— 见 [`close_ours`]。
    pub ours: bool,
}

/// PowerShell 那边交回来的原样一条：命令行在 Rust 里解析（可测），**不往界面送**。
#[derive(Deserialize)]
struct RawInventory {
    executable: Option<String>,
    version: Option<String>,
    #[serde(default)]
    processes: Vec<RawProcess>,
}
#[derive(Deserialize)]
struct RawProcess {
    pid: u32,
    started: String,
    #[serde(default)]
    command_line: Option<String>,
}

const INVENTORY: &str = r#"
$ErrorActionPreference = 'Stop'
$pkg = Get-AppxPackage -Name OpenAI.Codex | Where-Object { $_.PackageFamilyName -eq 'OpenAI.Codex_2p2nqsd0c76g0' } | Sort-Object Version -Descending | Select-Object -First 1
$exe = $null
if ($pkg) {
  foreach ($name in @('app\ChatGPT.exe', 'app\Codex.exe')) {
    $candidate = Join-Path $pkg.InstallLocation $name
    if (Test-Path -LiteralPath $candidate -PathType Leaf) { $exe = $candidate; break }
  }
}
$processes = @()
if ($exe) { $processes = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $exe } | ForEach-Object { @{ pid = [int]$_.ProcessId; started = $_.CreationDate.ToUniversalTime().ToString('o'); command_line = $_.CommandLine } }) }
@{ executable = $exe; version = $(if ($pkg) { [string]$pkg.Version } else { $null }); processes = @($processes) } | ConvertTo-Json -Compress -Depth 4
"#;

pub fn detect() -> Result<CodexDesktop> {
    let out = crate::process::powershell_std(INVENTORY).output()?;
    if !out.status.success() {
        return Err(GateError::Other(
            "无法读取 Codex 桌面端安装或进程状态".into(),
        ));
    }
    let raw: RawInventory = serde_json::from_slice(&out.stdout)?;
    let state_dir = qb_foundation::paths::state_dir();
    let home = dirs::home_dir().unwrap_or_default();
    Ok(summarize(raw, &state_dir, &home))
}

/// 原样一份 → 界面要的那份。**纯函数**。
///
/// `running` 看的是**全部**进程（包括读不出命令行的）—— 「查不到」不许降级成「没有」（§7.17）；
/// `processes` 只列主进程。命令行读不出来的（比如以管理员跑的那份）当成主进程、默认资料、
/// 不是面板的：宁可多报一个窗口，也不把一份认不出来的当成自己的去关。
fn summarize(raw: RawInventory, state_dir: &Path, home: &Path) -> CodexDesktop {
    let running = !raw.processes.is_empty();
    let processes = raw
        .processes
        .into_iter()
        .filter_map(|p| {
            let profile = match p.command_line.as_deref() {
                Some(cl) => main_profile(cl)?,
                None => None,
            };
            let ours = profile
                .as_deref()
                .is_some_and(|dir| is_panel_profile(dir, state_dir, home));
            Some(CodexDesktopProcess {
                pid: p.pid,
                started: p.started,
                profile,
                ours,
            })
        })
        .collect();
    CodexDesktop {
        executable: raw.executable,
        version: raw.version,
        running,
        processes,
    }
}

/// 把一条 Windows 命令行拆成参数（够用的那一半 `CommandLineToArgvW` 规则：
/// 引号里的空白不断开，引号本身去掉）。只用来认 `--type=` 与 `--user-data-dir`。
fn split_command_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

/// 这条命令行是不是 Electron **主进程**、它用哪份资料。
///
/// Chromium 的子进程（渲染、GPU、工具进程、崩溃上报）一律带 `--type=`，主进程没有。
/// 返回 `None` = 子进程；`Some(None)` = 主进程、默认资料；`Some(Some(dir))` = 主进程、指定资料。
pub fn main_profile(command_line: &str) -> Option<Option<String>> {
    let args = split_command_line(command_line);
    if args.iter().skip(1).any(|a| a.starts_with("--type=")) {
        return None;
    }
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if let Some(dir) = a.strip_prefix("--user-data-dir=") {
            let dir = dir.trim();
            return Some((!dir.is_empty()).then(|| dir.to_string()));
        }
        if a == "--user-data-dir" {
            return Some(
                it.next()
                    .map(|d| d.trim().to_string())
                    .filter(|d| !d.is_empty()),
            );
        }
    }
    Some(None)
}

fn normalize_dir(p: &str) -> String {
    p.trim()
        .trim_matches('"')
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// 这份资料目录是不是面板起的那种。
///
/// 面板起 Codex 桌面端时给的 `--user-data-dir` 都在面板自己的状态目录下
/// （账户槽位 `codex-accounts\<id>\desktop`、中转环境的 `…\desktop`）；
/// 唯一的例外是一个 GPT 槽位都没有时退回的 `~\.codex\desktop`（`workspace::codex_official_dirs`）。
/// 默认资料（没带这个开关的）、别的程序给的目录，都不是。
pub fn is_panel_profile(profile: &str, state_dir: &Path, home: &Path) -> bool {
    let p = normalize_dir(profile);
    let root = normalize_dir(&state_dir.display().to_string());
    let fallback = normalize_dir(&home.join(".codex").join("desktop").display().to_string());
    (!root.is_empty() && p.starts_with(&(root + "\\"))) || p == fallback
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Codex 打包应用的 PackageFamilyName。整机唯一，重新注册与识别都认它。
///
/// ⛔ **Store 包的身份全项目只有这一处**（跟 `inventory` 是 Claude 的唯一位置表、
/// `antigravity` 是反重力的唯一位置表同一个道理）：识别、重新注册、直装（`codex_store`）、
/// 完全卸载都从这里取，不许在别处再写一遍 `2p2nqsd0c76g0`。
pub const PACKAGE_FAMILY: &str = "OpenAI.Codex_2p2nqsd0c76g0";

/// 包名（PackageFamilyName 里 `_` 前面那一段）。直装时按它过滤 Store 回的候选包。
pub const PACKAGE_NAME: &str = "OpenAI.Codex";

/// Microsoft Store 的 product id（`apps.microsoft.com/detail/9plm9xgg6vks`）。
/// 直装走 DisplayCatalog / FE3 查它，winget 的 msstore 源也认它。
/// 2026-07 上游把显示名改成 ChatGPT 之后 product id **没变**（codex-app-mirror 的
/// `chatgpt-rebrand-recovery.md` 记着这件事），所以这里不跟着显示名走。
pub const PRODUCT_ID: &str = "9PLM9XGG6VKS";

/// 启动被系统拒绝（拒绝访问 / os error 5）时给使用者的可操作说明。
///
/// # 为什么单独拎出来讲
///
/// Codex 桌面端是 Microsoft Store 打包应用。它自动更新后，新版本里带了一个
/// 需要**管理员**才能注册的打包服务；更新一旦没走完，当前用户的注册就失效，
/// 于是**任何**启动方式（std spawn、CreateProcessW、应用激活）都被系统回
/// 「拒绝访问 (os error 5)」。这不是本工具的进程创建方式的问题，换哪种都一样，
/// 真正的修法是让管理员把这个包重新注册回来。
///
/// 实测（2026-09-19）：`Add-AppxPackage -RegisterByFamilyName` 非提权时报
/// `0x80073D28 Administrator privileges required to install packaged service`。
///
/// ⚠ 纯文本渲染，别写 Markdown 的 `**`（见 CLAUDE.md）。
/// `pub`：起桌面端（账户页、中转、识别环境）都走 `workspace::launch` 的 `CreateProcessW`，
/// 撞上这件事时报的是「会话进程操作失败：拒绝访问 (0x80070005)」—— 由那边翻译成这段说明。
pub fn registration_repair_message() -> String {
    format!(
        "Codex 桌面端（Microsoft Store 版）无法启动：Windows 拒绝了启动请求（拒绝访问，os error 5）。\
         通常是 Codex 自动更新后，新版本的打包服务需要管理员重新注册，当前用户的注册已失效——\
         这时任何启动方式都会被系统拒绝。修复办法（任选其一，完成后重试）：\
         1）点账户页的「修复 Codex 注册（需要管理员）」按钮；\
         2）打开 Microsoft Store → 库，更新 Codex；\
         3）以管理员运行 PowerShell 执行：Add-AppxPackage -RegisterByFamilyName -MainPackage {PACKAGE_FAMILY}。"
    )
}

/// 以管理员重新为当前用户注册 Codex 打包应用。
///
/// 只做重新注册这一件事：不改任何安全设置、不动别的包、不复制任何凭据。
/// 用 `Start-Process -Verb RunAs` 触发 UAC；使用者不授权就如实返回「已取消」。
///
/// # ⛔ 为什么必须提权
///
/// 新版 Codex 的包里含打包服务，注册它需要管理员——非提权注册会当场报
/// `0x80073D28`。这跟 §7.41 里官方沙箱初始化要提权是同一类：本机安装态修复，
/// 不是账号或接口问题，也不能靠换 API Key 绕过。
pub fn repair_registration() -> Result<()> {
    // 提权子进程里跑的注册命令：成功 exit 0，失败 exit 1。
    let inner = format!(
        "try {{ Add-AppxPackage -RegisterByFamilyName -MainPackage {PACKAGE_FAMILY} -ErrorAction Stop; exit 0 }} catch {{ exit 1 }}"
    );
    // 外层（非提权）：拉起提权 PowerShell 并等待，回读退出码。
    // 使用者在 UAC 上点「否」时 Start-Process 抛错，归一成 cancelled，不当成失败崩掉。
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$inner = {inner}
try {{
  $p = Start-Process powershell -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList '-NoProfile','-NonInteractive','-Command',$inner
}} catch {{
  Write-Output 'REPAIR=cancelled'
  exit 0
}}
Write-Output ('REPAIR=' + $p.ExitCode)
"#,
        inner = quote(&inner)
    );
    let out = crate::process::powershell_std(&script).output()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.contains("REPAIR=0") {
        return Ok(());
    }
    if stdout.contains("REPAIR=cancelled") {
        return Err(GateError::Other(
            "已取消管理员授权，未修复 Codex 注册。".into(),
        ));
    }
    Err(GateError::Other(format!(
        "重新注册 Codex 失败。请改用 Microsoft Store → 库 更新 Codex，或以管理员运行 PowerShell：Add-AppxPackage -RegisterByFamilyName -MainPackage {PACKAGE_FAMILY}。"
    )))
}

/// Only called after the user explicitly confirms stopping desktop tasks.
/// Recheck path + creation time before killing; refuse our own ancestor chain.
pub fn close() -> Result<()> {
    let Some(exe) = detect()?.executable else {
        return Ok(());
    };
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$exe = {exe}
$all = @(Get-CimInstance Win32_Process)
$ancestors = @{{}}
$next = {our_pid}
while ($next -and -not $ancestors.ContainsKey([int]$next)) {{
  $ancestors[[int]$next] = $true
  $entry = $all | Where-Object {{ $_.ProcessId -eq $next }} | Select-Object -First 1
  $next = $entry.ParentProcessId
}}
$targets = @($all | Where-Object {{ $_.ExecutablePath -eq $exe }})
foreach ($p in $targets) {{ if ($ancestors.ContainsKey([int]$p.ProcessId)) {{ throw 'Codex 是面板的父进程，请先从开始菜单独立打开 QB Gate，再关闭桌面端或切换账户。' }} }}
foreach ($p in $targets) {{
  $live = Get-CimInstance Win32_Process -Filter ('ProcessId=' + $p.ProcessId)
  if ($live -and $live.ExecutablePath -eq $exe -and $live.CreationDate -eq $p.CreationDate) {{
    $proc = Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue
    if ($proc) {{ $null = $proc.CloseMainWindow() }}
  }}
}}
if ($targets.Count -gt 0) {{ Start-Sleep -Milliseconds 1500 }}
foreach ($p in $targets) {{
  $live = Get-CimInstance Win32_Process -Filter ('ProcessId=' + $p.ProcessId)
  if ($live -and $live.ExecutablePath -eq $exe -and $live.CreationDate -eq $p.CreationDate) {{
    & taskkill /PID ([string]$p.ProcessId) /T /F 2>$null | Out-Null
  }}
}}
for ($i = 0; $i -lt 25; $i++) {{
  if (@(Get-CimInstance Win32_Process | Where-Object {{ $_.ExecutablePath -eq $exe }}).Count -eq 0) {{ exit 0 }}
  Start-Sleep -Milliseconds 200
}}
throw 'Codex 桌面端未完全退出（可能以管理员或沙箱提权运行，普通权限无法结束它），操作已停止；未切换账户或启动新窗口。请从任务栏手动退出 Codex，或以管理员运行本面板后重试。'
"#,
        exe = quote(&exe),
        our_pid = std::process::id()
    );
    let out = crate::process::powershell_std(&script).output()?;
    if !out.status.success() {
        return Err(GateError::Other(format!(
            "未能关闭 Codex 桌面端：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

/// 只关**面板起的** Codex 桌面端（[`CodexDesktopProcess::ours`]），连同它们的子进程树。
/// 返回关了几个实例。
///
/// # 为什么不再一律全关（2026-09-23）
///
/// 0.25.0 起「起槽位 / 切槽位」之前走的是 [`close`]：按官方包的 exe 路径把**所有**
/// Codex 桌面端都收掉，理由是「单实例，开着再起只会把旧窗口拉到前面」。那条理由只对
/// **同一份资料**成立 —— Electron 的单实例锁按 `--user-data-dir` 算，面板给每个槽位的是
/// 它自己那份，开始菜单 / 别的程序起的默认实例挡不住它。全关的代价却是真的：别处起的
/// 那份被面板收掉，而起它的那个程序（多开工具、`codex://` 链接、自动更新）会再把它拉起来，
/// 使用者看到的就是「突然又弹出一个 Codex 窗口」。cockpit-tools 的多开也是按实例的
/// 资料目录 / `CODEX_HOME` 认自己那份、只关自己那份。
///
/// 使用者在 GPT 页点「一键关闭」仍然走 [`close`]（全关，确认框里写着「所有」）。
///
/// 安全检查跟 [`close`] 一样：面板的祖先链上有它就拒绝；动手前按 PID + 创建时间再核一次。
pub fn close_ours() -> Result<usize> {
    let d = detect()?;
    let Some(exe) = d.executable else {
        return Ok(0);
    };
    let targets: Vec<&CodexDesktopProcess> = d.processes.iter().filter(|p| p.ours).collect();
    if targets.is_empty() {
        return Ok(0);
    }
    let list = targets
        .iter()
        .map(|p| format!("@{{ pid = {}; started = {} }}", p.pid, quote(&p.started)))
        .collect::<Vec<_>>()
        .join(", ");
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$exe = {exe}
$targets = @({list})
$all = @(Get-CimInstance Win32_Process)
$ancestors = @{{}}
$next = {our_pid}
while ($next -and -not $ancestors.ContainsKey([int]$next)) {{
  $ancestors[[int]$next] = $true
  $entry = $all | Where-Object {{ $_.ProcessId -eq $next }} | Select-Object -First 1
  $next = $entry.ParentProcessId
}}
foreach ($t in $targets) {{ if ($ancestors.ContainsKey([int]$t.pid)) {{ throw 'Codex 是面板的父进程，请先从开始菜单独立打开 QB Gate，再切换账户或启动。' }} }}
function Same($t) {{
  $live = Get-CimInstance Win32_Process -Filter ('ProcessId=' + $t.pid)
  return ($live -and $live.ExecutablePath -eq $exe -and $live.CreationDate.ToUniversalTime().ToString('o') -eq $t.started)
}}
foreach ($t in $targets) {{
  if (Same $t) {{
    $proc = Get-Process -Id $t.pid -ErrorAction SilentlyContinue
    if ($proc) {{ $null = $proc.CloseMainWindow() }}
  }}
}}
Start-Sleep -Milliseconds 1500
foreach ($t in $targets) {{ if (Same $t) {{ & taskkill /PID ([string]$t.pid) /T /F 2>$null | Out-Null }} }}
for ($i = 0; $i -lt 25; $i++) {{
  $left = @($targets | Where-Object {{ Same $_ }})
  if ($left.Count -eq 0) {{ exit 0 }}
  Start-Sleep -Milliseconds 200
}}
throw '面板起的 Codex 桌面端没有完全退出（可能以管理员或沙箱提权运行），操作已停止；未切换账户或启动新窗口。请从任务栏手动退出它后重试。'
"#,
        exe = quote(&exe),
        list = list,
        our_pid = std::process::id()
    );
    let out = crate::process::powershell_std(&script).output()?;
    if !out.status.success() {
        return Err(GateError::Other(format!(
            "未能关闭面板起的 Codex 桌面端：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(targets.len())
}

// 0.25.0 起这里不再有裸 spawn 的 `launch()`：起桌面端走 `qb-app::workspace::launch`
// （验 IP → `sessions::start` 托管 → 租约），目录仍是 `<槽位>\home` + `<槽位>\desktop`
// （`workspace::codex_slot_dirs`）。拒绝访问 (os error 5) 的翻译在那条路上也做了
// （`looks_like_access_denied` → `registration_repair_message`）。
// 这个文件只剩盘点、关闭、重新注册三件事，而且从不碰默认的 ~/.codex。

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn powershell_path_is_a_literal_even_with_quotes() {
        assert_eq!(quote("C:\\O'Brien\\$profile"), "'C:\\O''Brien\\$profile'");
    }
    #[test]
    fn inventory_requires_the_official_package_family() {
        assert!(INVENTORY.contains("PackageFamilyName -eq 'OpenAI.Codex_2p2nqsd0c76g0'"));
        assert!(INVENTORY.contains("ExecutablePath -eq $exe"));
    }

    /// 识别用的包族名与重新注册用的常量必须是同一个 —— 漂了的话，
    /// 「检测到的那份」和「注册回来的那份」就成了两个包。
    #[test]
    fn the_repair_family_matches_the_inventory_family() {
        assert_eq!(PACKAGE_FAMILY, "OpenAI.Codex_2p2nqsd0c76g0");
        assert!(INVENTORY.contains(PACKAGE_FAMILY));
    }

    /// 包名就是包族名 `_` 前面那一段；直装按它过滤候选，两者不能各写各的。
    #[test]
    fn the_package_name_is_the_family_prefix() {
        assert_eq!(
            PACKAGE_FAMILY.split('_').next(),
            Some(PACKAGE_NAME),
            "PACKAGE_NAME 必须等于 PACKAGE_FAMILY 的前缀"
        );
        assert!(INVENTORY.contains(&format!("-Name {PACKAGE_NAME} ")));
    }

    /// 面板给的参数是整条加引号的（`quote_argument`），别的程序可能不加 ——
    /// 两种都要认得出目录；子进程（带 `--type=`）不算一个实例。
    #[test]
    fn a_main_process_is_told_apart_from_its_children_and_its_profile_is_read() {
        let exe =
            r#""C:\Program Files\WindowsApps\OpenAI.Codex_1_x64__2p2nqsd0c76g0\app\ChatGPT.exe""#;
        assert_eq!(
            main_profile(&format!(
                r#"{exe} "--user-data-dir=C:\Users\test\App Data\ClaudeIpGate\codex-accounts\x\desktop""#
            )),
            Some(Some(
                r"C:\Users\test\App Data\ClaudeIpGate\codex-accounts\x\desktop".into()
            ))
        );
        assert_eq!(
            main_profile(&format!(r#"{exe} --user-data-dir="D:\p q\desktop" --flag"#)),
            Some(Some(r"D:\p q\desktop".into()))
        );
        assert_eq!(
            main_profile(&format!(r#"{exe} --user-data-dir D:\x"#)),
            Some(Some(r"D:\x".into()))
        );
        assert_eq!(main_profile(exe), Some(None), "没带开关 = 默认资料");
        assert_eq!(
            main_profile(&format!(
                r#"{exe} --type=renderer "--user-data-dir=C:\x\desktop""#
            )),
            None,
            "子进程不是一个实例"
        );
        assert_eq!(
            main_profile(&format!(r#"{exe} --type=crashpad-handler --no-rate-limit"#)),
            None
        );
    }

    /// 只有面板状态目录下的（加上没有槽位时退回的 `~\.codex\desktop`）才算面板的；
    /// 大小写、斜杠方向、结尾的反斜杠都不影响；同名前缀的兄弟目录不算。
    #[test]
    fn only_profiles_under_the_panel_state_dir_are_ours() {
        let state = Path::new(r"C:\Users\test\AppData\Local\ClaudeIpGate");
        let home = Path::new(r"C:\Users\test");
        assert!(is_panel_profile(
            r"c:/users/test/appdata/local/claudeipgate/codex-accounts/x/desktop/",
            state,
            home
        ));
        assert!(is_panel_profile(
            r"C:\Users\test\.codex\desktop",
            state,
            home
        ));
        assert!(!is_panel_profile(
            r"C:\Users\test\AppData\Roaming\Codex",
            state,
            home
        ));
        assert!(!is_panel_profile(
            r"C:\Users\test\AppData\Local\ClaudeIpGate-other\desktop",
            state,
            home
        ));
        assert!(!is_panel_profile(r"C:\Users\test\.codex", state, home));
    }

    /// 读不出命令行的那份当成「不是面板的」—— 不许把认不出来的当成自己的去关；
    /// 但它仍然算「在跑」（查不到 ≠ 没有，§7.17）。
    #[test]
    fn processes_without_a_command_line_still_count_as_running_but_never_as_ours() {
        let raw = RawInventory {
            executable: Some("C:\\app\\ChatGPT.exe".into()),
            version: None,
            processes: vec![
                RawProcess {
                    pid: 1,
                    started: "t1".into(),
                    command_line: None,
                },
                RawProcess {
                    pid: 2,
                    started: "t2".into(),
                    command_line: Some(
                        r#""C:\app\ChatGPT.exe" "--user-data-dir=C:\S\codex-accounts\a\desktop""#
                            .into(),
                    ),
                },
                RawProcess {
                    pid: 3,
                    started: "t3".into(),
                    command_line: Some(r#""C:\app\ChatGPT.exe" --type=gpu-process"#.into()),
                },
            ],
        };
        let d = summarize(raw, Path::new(r"C:\S"), Path::new(r"C:\H"));
        assert!(d.running);
        assert_eq!(d.processes.len(), 2, "子进程不单列");
        assert!(!d.processes[0].ours && d.processes[0].profile.is_none());
        assert!(d.processes[1].ours);
    }

    /// 拒绝访问（os error 5）的说明必须给出可照做的修复命令，
    /// 而不是把裸 IO 错误丢给使用者。
    #[test]
    fn the_repair_message_tells_the_user_what_to_run() {
        let m = registration_repair_message();
        assert!(m.contains("RegisterByFamilyName"));
        assert!(m.contains(PACKAGE_FAMILY));
        // 纯文本渲染，不许夹 Markdown 强调符（见 CLAUDE.md）。
        assert!(!m.contains("**"));
    }
}
