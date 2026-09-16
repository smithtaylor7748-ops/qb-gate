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
throw 'Codex 桌面端未完全退出，操作已停止；未切换账户或启动新窗口。'
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
    let mut child = crate::process::hidden_std(cmd).spawn()?;
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
}
