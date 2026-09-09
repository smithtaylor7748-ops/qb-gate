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
    let out = crate::process::hidden_tokio(tokio::process::Command::new(p))
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

/// Codex CLI 可能落在哪。
///
/// 顺序 = 优先级。**npm 全局排第一** —— 旧版只查 `.local\bin` 和
/// `Programs\codex` 两处，而实机上装的是 npm 全局那份，于是环境页
/// 一直把已装的 Codex 显示成「未安装」。
///
/// `%LOCALAPPDATA%\OpenAI\Codex\bin\<hash>\codex.exe` 的 `<hash>` 是随版本
/// 变的目录名，所以那一层要枚举，不能拼死。
pub fn codex_candidates() -> Vec<PathBuf> {
    let mut out = vec![
        // npm 全局。装的是 .cmd 批处理，不是 exe。
        PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
            .join("npm")
            .join("codex.cmd"),
        home().join(".local").join("bin").join("codex.exe"),
        local().join("Programs").join("codex").join("codex.exe"),
    ];

    // 官方桌面版把 CLI 塞在一个按版本变名的哈希目录下。
    let bin = local().join("OpenAI").join("Codex").join("bin");
    if let Ok(rd) = std::fs::read_dir(&bin) {
        let mut nested: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path().join("codex.exe"))
            .filter(|p| p.exists())
            .collect();
        nested.sort();
        out.append(&mut nested);
    }

    // MSIX（Microsoft Store / winget 的 store 源）。
    out.push(local().join("Microsoft").join("WindowsApps").join("codex.exe"));
    out
}

pub async fn codex() -> Software {
    let found = codex_candidates().into_iter().find(|p| p.exists());
    // ⚠ 这里敢直接跑 exe，是因为 Codex **还没有**纳入门禁。
    // 等 Codex 也加上 Deny ExecuteFile 之后，这里必须改成跟 upgrade.rs
    // 一样读文件属性 —— 否则升级流程会依赖「能把这个二进制启动起来」。
    let version = match &found {
        Some(p) => parse_codex_version(codex_raw_version(p).await.as_deref()),
        None => None,
    };
    Software {
        id: "codex",
        name: "Codex CLI",
        installed: found.is_some(),
        version,
        path: found,
        advisory: None,
    }
}

/// 跑一次 `--version`，**批处理要走 `cmd /c`**。
///
/// npm 全局装出来的是 `codex.cmd`，不是 exe。CreateProcess 不认批处理，
/// 直接 `Command::new("codex.cmd")` 拿不到输出 —— 版本号会永远是 None。
async fn codex_raw_version(p: &PathBuf) -> Option<String> {
    let is_batch = p
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));

    let out = if is_batch {
        crate::process::hidden_tokio(tokio::process::Command::new("cmd"))
            .arg("/c")
            .arg(p)
            .arg("--version")
            .output()
            .await
            .ok()?
    } else {
        crate::process::hidden_tokio(tokio::process::Command::new(p))
            .arg("--version")
            .output()
            .await
            .ok()?
    };
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// `codex --version` 打的是 `codex-cli 0.153.4`，取后面那段。
///
/// 拿整行当版本号会让「装没装上」之外的一切比较都失效 ——
/// 升级判断按三段版本比，`codex-cli 0.153.4` 解析不出来。
pub fn parse_codex_version(raw: Option<&str>) -> Option<String> {
    let line = raw?.lines().next()?.trim();
    let tail = line.rsplit(char::is_whitespace).next().unwrap_or(line);
    (!tail.is_empty() && tail.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .then(|| tail.to_string())
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
    let _ = crate::process::hidden_std(std::process::Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output();
}

#[cfg(not(windows))]
pub fn kill_desktop_processes() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_version_drops_the_product_name() {
        // `codex --version` 打的是 `codex-cli 0.153.4`。拿整行当版本号，
        // 三段版本比较会直接失效。
        assert_eq!(
            parse_codex_version(Some("codex-cli 0.153.4")).as_deref(),
            Some("0.153.4")
        );
        assert_eq!(
            parse_codex_version(Some("codex-cli 0.153.4\n")).as_deref(),
            Some("0.153.4")
        );
        // 有些构建只打版本号。
        assert_eq!(parse_codex_version(Some("0.146.1")).as_deref(), Some("0.146.1"));
    }

    #[test]
    fn codex_version_returns_none_when_it_is_not_a_version() {
        // 「拿不到版本」和「拿到一句错误信息」必须都算 None，
        // 否则界面上会显示一行报错当版本号。
        assert_eq!(parse_codex_version(None), None);
        assert_eq!(parse_codex_version(Some("")), None);
        assert_eq!(parse_codex_version(Some("command not found")), None);
    }

    #[test]
    fn npm_global_path_is_probed_first() {
        // 实机上 Codex 就装在 npm 全局那份。旧版不查这里，
        // 于是把已装的 Codex 一直显示成「未安装」。
        let c = codex_candidates();
        let first = c.first().expect("至少有一个候选路径");
        assert_eq!(
            first.file_name().and_then(|n| n.to_str()),
            Some("codex.cmd"),
            "npm 全局那份必须排第一"
        );
        assert!(c.len() >= 4, "候选路径不该少于四条，实际 {}", c.len());
    }
}
