//! Codex desktop inventory and explicit process control. The Store package identity
//! plus its exact executable path are the evidence; a process name is never enough.
use crate::error::{GateError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct CodexDesktop {
    pub executable: Option<String>,
    pub version: Option<String>,
    pub running: bool,
    pub processes: Vec<CodexDesktopProcess>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct CodexDesktopProcess {
    pub pid: u32,
    pub started: String,
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
if ($exe) { $processes = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $exe } | ForEach-Object { @{ pid = [int]$_.ProcessId; started = $_.CreationDate.ToUniversalTime().ToString('o') } }) }
@{ executable = $exe; version = $(if ($pkg) { [string]$pkg.Version } else { $null }); running = ($processes.Count -gt 0); processes = @($processes) } | ConvertTo-Json -Compress -Depth 4
"#;

pub fn detect() -> Result<CodexDesktop> {
    let out = crate::process::powershell_std(INVENTORY).output()?;
    if !out.status.success() {
        return Err(GateError::Other(
            "无法读取 Codex 桌面端安装或进程状态".into(),
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(Into::into)
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Codex 打包应用的 PackageFamilyName。整机唯一，重新注册与识别都认它。
pub const PACKAGE_FAMILY: &str = "OpenAI.Codex_2p2nqsd0c76g0";

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
/// `pub`：`workspace::launch` 起中转 / 识别环境的 Codex 走的是 `CreateProcessW`，
/// 撞上同一件事时报的是「会话进程操作失败：拒绝访问 (0x80070005)」—— 同样要给这段说明。
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

/// Explicit profile paths are supported by the installed Electron desktop app.
/// Never launch the CLI here and never edit the default ~/.codex directory.
pub fn launch(home: &std::path::Path, desktop: &std::path::Path) -> Result<CodexDesktopProcess> {
    let app = detect()?;
    if app.running {
        return Err(GateError::Other(
            "Codex 桌面端仍在运行，请先确认关闭再切换，避免沿用旧账户。".into(),
        ));
    }
    let exe = app.executable.ok_or_else(|| {
        GateError::Other("未找到 Microsoft Store 安装的 Codex 桌面端。请先安装桌面端。".into())
    })?;
    let mut cmd = std::process::Command::new(PathBuf::from(exe));
    cmd.env("CODEX_HOME", home)
        .env("CODEX_ELECTRON_USER_DATA_PATH", desktop)
        .arg(format!("--user-data-dir={}", desktop.display()));
    for key in [
        "OPENAI_API_KEY",
        "OPENAI_BASE_URL",
        "CODEX_API_KEY",
        "CODEX_AUTH_TOKEN",
        "CODEX_ELECTRON_AGENT_RUN_ID",
    ] {
        cmd.env_remove(key);
    }
    // ⛔ 打包应用注册失效时，spawn 会返回「拒绝访问 (os error 5)」。原来直接 `?`
    // 把它当成裸 IO 错误抛出去，界面上只剩「IO 失败: Access is denied. (os error 5)」，
    // 使用者完全不知道要去重新注册。这里把这个特定情形翻译成可操作的说明。
    // 其它 IO 错误保持原样上抛（仍是 GateError::Io）。
    let mut child = match crate::process::hidden_std(cmd).spawn() {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(GateError::Other(registration_repair_message()));
        }
        Err(e) => return Err(GateError::Io(e)),
    };
    std::thread::sleep(std::time::Duration::from_millis(700));
    if let Some(status) = child.try_wait()? {
        return Err(GateError::Other(format!(
            "Codex 桌面端启动后退出（{status}），未标记为启动成功。"
        )));
    }
    detect()?
        .processes
        .into_iter()
        .find(|p| p.pid == child.id())
        .ok_or_else(|| GateError::Other("桌面端进程未通过安装路径核验，未记录为已启动".into()))
}

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
