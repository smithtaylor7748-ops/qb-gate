//! 门禁要锁哪些可执行文件。
//!
//! 口径直接沿用现有 ClaudeIpGate.ps1，别自己发挥：
//!   锁  `%USERPROFILE%\.local\bin\claude.exe`      Claude Code 主二进制
//!   锁  winget 安装的那份副本                       历史遗留，一直锁着
//!   锁  `%LOCALAPPDATA%\AnthropicClaude\claude.exe` 桌面端 Squirrel 存根
//!   不锁 `...\AnthropicClaude\app-<版本>\claude.exe` 一加 Deny 应用开新窗口就崩
//!
//! `app-*` 那份只能靠 watchdog 的 taskkill 收。这是已知残留缺口：
//! 刻意翻进 app-* 目录直接双击能绕开**启动**门禁，20 秒内会被看门狗收掉，
//! 前提是当时有看门狗在跑。要彻底堵死需要 AppLocker / WDAC，不在本项目范围。

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Target {
    pub path: PathBuf,
    pub kind: TargetKind,
    pub exists: bool,
    pub locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum TargetKind {
    /// Claude Code CLI
    Cli,
    /// 桌面端 Squirrel 存根 —— 开始菜单和桌面的 Claude.lnk 都指向它
    DesktopStub,
    /// 升级残留的旧副本，**没有 Deny ACL，是可绕过的执行副本**
    StaleCopy,
}

fn home() -> Option<PathBuf> {
    dirs::home_dir()
}

fn local_appdata() -> Option<PathBuf> {
    dirs::data_local_dir()
}

/// 可锁目标清单。不含 `app-*`。
pub fn lockable() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(h) = home() {
        out.push(h.join(".local").join("bin").join("claude.exe"));
    }
    if let Some(la) = local_appdata() {
        out.push(la.join("AnthropicClaude").join("claude.exe"));
        // winget 的 shim 目录
        out.push(
            la.join("Microsoft")
                .join("WinGet")
                .join("Links")
                .join("claude.exe"),
        );
    }
    out.into_iter().filter(|p| p.exists()).collect()
}

/// 桌面端真正在跑的那些 exe：`AnthropicClaude\app-*\claude.exe` 与 `Update.exe`。
/// 只用于看门狗识别进程，**不要**拿去上锁。
pub fn desktop_runtime_dir() -> Option<PathBuf> {
    local_appdata().map(|la| la.join("AnthropicClaude"))
}

/// 升级残留：`claude.exe.old.<时间戳>`。
///
/// 这些副本没有 Deny ACL，是能绕过门禁的执行副本，必须清掉。但注意：
/// **发起升级的那个会话自己占着最新的一份，删不掉**（Windows 允许改名正在运行的
/// exe，不允许删除）。所以清理放在**下一次**升级的开头，而不是本次的收尾。
pub fn stale_copies() -> Vec<PathBuf> {
    let Some(h) = home() else { return Vec::new() };
    let dir = h.join(".local").join("bin");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("claude.exe.old."))
        })
        .collect()
}

pub fn kind_of(p: &Path) -> TargetKind {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.starts_with("claude.exe.old.") {
        TargetKind::StaleCopy
    } else if p
        .parent()
        .and_then(|d| d.file_name())
        .and_then(|n| n.to_str())
        == Some("AnthropicClaude")
    {
        TargetKind::DesktopStub
    } else {
        TargetKind::Cli
    }
}
