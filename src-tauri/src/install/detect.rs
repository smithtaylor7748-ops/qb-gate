//! 本机环境检测：Claude 桌面端 / Claude Code / 浏览器 / Codex。

use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct Software {
    pub id: &'static str,
    pub name: &'static str,
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<PathBuf>,
    /// 检测到但建议重装时给出的理由。
    pub advisory: Option<String>,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}
fn local() -> PathBuf {
    dirs::data_local_dir().unwrap_or_default()
}

async fn exe_version(p: &PathBuf) -> Option<String> {
    let out = tokio::process::Command::new(p)
        .arg("--version")
        .output()
        .await
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

pub async fn claude_code() -> Software {
    let p = home().join(".local").join("bin").join("claude.exe");
    let installed = p.exists();
    Software {
        id: "claude-code",
        name: "Claude Code",
        version: if installed { exe_version(&p).await } else { None },
        path: installed.then(|| p.clone()),
        installed,
        advisory: None,
    }
}

pub fn claude_desktop() -> Software {
    let dir = local().join("AnthropicClaude");
    let stub = dir.join("claude.exe");
    let installed = stub.exists();
    // 版本从 app-<版本> 目录名读，比启动一次进程便宜得多。
    let version = std::fs::read_dir(&dir).ok().and_then(|rd| {
        rd.filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|n| n.strip_prefix("app-").map(String::from))
            .max()
    });
    Software {
        id: "claude-desktop",
        name: "Claude 桌面端",
        installed,
        version,
        path: installed.then_some(stub),
        advisory: None,
    }
}

pub fn codex() -> Software {
    let candidates = [
        home().join(".local").join("bin").join("codex.exe"),
        local().join("Programs").join("codex").join("codex.exe"),
    ];
    let found = candidates.into_iter().find(|p| p.exists());
    Software {
        id: "codex",
        name: "Codex CLI",
        installed: found.is_some(),
        version: None,
        path: found,
        advisory: None,
    }
}

/// 浏览器只探常见安装位置，用于「是否需要重装 / 语言时区是否同步」的提示。
pub fn browsers() -> Vec<Software> {
    let pf = PathBuf::from(std::env::var("ProgramFiles").unwrap_or_default());
    let pf86 = PathBuf::from(std::env::var("ProgramFiles(x86)").unwrap_or_default());
    let list: [(&'static str, &'static str, PathBuf); 2] = [
        (
            "chrome",
            "Google Chrome",
            pf.join("Google").join("Chrome").join("Application").join("chrome.exe"),
        ),
        (
            "edge",
            "Microsoft Edge",
            pf86.join("Microsoft").join("Edge").join("Application").join("msedge.exe"),
        ),
    ];
    list.into_iter()
        .map(|(id, name, p)| Software {
            id,
            name,
            installed: p.exists(),
            version: None,
            path: p.exists().then_some(p),
            advisory: None,
        })
        .collect()
}

/// 桌面端进程 —— `AnthropicClaude\` 前缀匹配。
///
/// **这个前缀就是把 Claude Code 摘出去的那道判断**：Claude Code 的 exe 在
/// `.local\bin` 和 winget 目录下，永远不会进这个列表。改这里之前想清楚，
/// 收错进程会连正在工作的 Claude Code 会话一起杀掉。
#[cfg(windows)]
pub fn kill_desktop_processes() {
    let prefix = local().join("AnthropicClaude");
    let script = format!(
        "Get-CimInstance Win32_Process -Filter \"Name='claude.exe' OR Name='Update.exe'\" | \
         Where-Object {{ $_.ExecutablePath -like '{}*' }} | \
         ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}",
        prefix.display()
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output();
}

#[cfg(not(windows))]
pub fn kill_desktop_processes() {}
