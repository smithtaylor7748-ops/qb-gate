//! 门禁要锁哪些可执行文件。
//!
//! **每一个完整可执行的 claude.exe 副本都必须锁上。** 漏掉一个，那一个就是
//! 绕过 IP 门禁的现成入口 —— 这是原档案 K1 记下的教训（升级残留的
//! `claude.exe.old.*` 没有 Deny ACL，等于一条敞开的路）。
//!
//! 2026-09-08 实测本机的实际布局，比原档案记的多两处：
//!
//! | 路径 | 锁? | 说明 |
//! |---|---|---|
//! | `~\.local\bin\claude.exe` | 锁 | 官方安装脚本的落点 |
//! | `%APPDATA%\Claude\claude-code\<版本>\claude.exe` | 锁 | **新布局**，每个版本一份，全都要锁 |
//! | `%LOCALAPPDATA%\Microsoft\WinGet\Packages\Anthropic.ClaudeCode*\claude.exe` | 锁 | winget 装的那份 |
//! | `%LOCALAPPDATA%\AnthropicClaude\claude.exe` | 锁 | 桌面端 Squirrel 存根（本机当前没有） |
//! | `%LOCALAPPDATA%\AnthropicClaude\app-<版本>\claude.exe` | **不锁** | 一加 Deny，桌面端开新窗口就崩 |
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
    /// Claude Code CLI（`.local\bin` 或 winget）
    Cli,
    /// `%APPDATA%\Claude\claude-code\<版本>\` 下的版本化 CLI 副本
    CliVersioned,
    /// 桌面端 Squirrel 存根 —— 开始菜单与桌面的 Claude.lnk 指向它
    DesktopStub,
    /// 升级残留的旧副本，**没有 Deny ACL，是可绕过的执行副本**
    StaleCopy,
    /// Codex CLI。**只在设置里打开「Codex 也归门禁管」之后才会出现在清单里。**
    CodexCli,
}

fn home() -> Option<PathBuf> {
    dirs::home_dir()
}

/// `%APPDATA%`（Roaming）。Claude Code 的新布局放在这下面。
fn roaming() -> Option<PathBuf> {
    dirs::config_dir()
}

fn local_appdata() -> Option<PathBuf> {
    dirs::data_local_dir()
}

/// 枚举一个目录下所有子目录里的 `claude.exe`。
///
/// 用于 `%APPDATA%\Claude\claude-code\<版本>\` 与 winget 的 `Packages\<包名>\`：
/// 两者都是「一个版本／包一个子目录」，**每一份都是完整可执行的**，
/// 所以要全部收进来，不能只挑最新那个。
fn claude_exes_in_subdirs(parent: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.path().join("claude.exe"))
        .filter(|p| p.is_file())
        .collect()
}

/// 可锁目标清单。**不含** `AnthropicClaude\app-*`。
pub fn lockable() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();

    if let Some(h) = home() {
        out.push(h.join(".local").join("bin").join("claude.exe"));
    }

    // 新布局：%APPDATA%\Claude\claude-code\<版本>\claude.exe，每个版本都锁。
    if let Some(r) = roaming() {
        out.extend(claude_exes_in_subdirs(
            &r.join("Claude").join("claude-code"),
        ));
    }

    if let Some(la) = local_appdata() {
        // 桌面端存根（不是 app-* 里那份）
        out.push(la.join("AnthropicClaude").join("claude.exe"));

        // winget：老的 Links shim 与新的 Packages 实体，两处都看
        out.push(
            la.join("Microsoft")
                .join("WinGet")
                .join("Links")
                .join("claude.exe"),
        );
        let pkgs = la.join("Microsoft").join("WinGet").join("Packages");
        if let Ok(rd) = std::fs::read_dir(&pkgs) {
            for e in rd.filter_map(|e| e.ok()) {
                let name = e.file_name().to_string_lossy().to_lowercase();
                if name.contains("claudecode") || name.contains("claude.code") {
                    let exe = e.path().join("claude.exe");
                    if exe.is_file() {
                        out.push(exe);
                    }
                }
            }
        }
    }

    // Codex **默认不在门禁管辖内**。打开这个开关之后 `codex` 会跟
    // claude.exe 一样被 Deny ExecuteFile 挡住 —— 这对「请求不能从没核实过的
    // IP 出去」是对的，但对正在用 Codex 干活的人是个突然的变化，
    // 所以只能由他自己在设置里打开。见 `settings.rs`。
    if crate::settings::codex_under_gate() {
        out.extend(codex_lockable());
    }

    out.retain(|p| p.is_file());
    out.sort();
    out.dedup();
    out
}

/// Codex 侧可锁的副本。
///
/// ⚠ **npm 全局装出来的是 `codex.cmd`（批处理），不是 exe。**
/// 对批处理加 Deny ExecuteFile 能挡住 `codex` 这个命令本身，但挡不住
/// 有人直接去调它内部那个 node 脚本 —— 那不是这一层能解决的问题，
/// 跟 `app-*` 那个已知缺口是同一类。界面上要如实说明。
pub fn codex_lockable() -> Vec<PathBuf> {
    crate::install::detect::codex_candidates()
        .into_iter()
        .filter(|p| p.is_file())
        .collect()
}

/// 桌面端真正在跑的那些 exe 所在目录（`app-*` 与 `Update.exe`）。
/// 只用于看门狗识别进程，**不要**拿去上锁。
pub fn desktop_runtime_dir() -> Option<PathBuf> {
    local_appdata().map(|la| la.join("AnthropicClaude"))
}

/// 判断一个路径是不是「不能加 Deny ACE」的桌面端运行时副本。
///
/// 这条判断是把 `app-*` 排除在外的那道闸。改它之前先读文件头的表格。
pub fn is_desktop_runtime_copy(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase().replace('/', "\\");
    s.contains("\\anthropicclaude\\app-")
}

/// 升级残留：`claude.exe.old.<时间戳>`。
///
/// 这些副本没有 Deny ACL，是能绕过门禁的执行副本，必须清掉。但注意：
/// **发起升级的那个会话自己占着最新的一份，删不掉**（Windows 允许改名正在
/// 运行的 exe，不允许删除）。所以清理放在**下一次**升级的开头。
pub fn stale_copies() -> Vec<PathBuf> {
    let mut dirs_to_scan: Vec<PathBuf> = Vec::new();
    if let Some(h) = home() {
        dirs_to_scan.push(h.join(".local").join("bin"));
    }
    if let Some(r) = roaming() {
        let cc = r.join("Claude").join("claude-code");
        if let Ok(rd) = std::fs::read_dir(&cc) {
            dirs_to_scan.extend(rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()));
        }
        dirs_to_scan.push(cc);
    }

    let mut out = Vec::new();
    for dir in dirs_to_scan {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        out.extend(
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("claude.exe.old."))
                }),
        );
    }
    out.sort();
    out.dedup();
    out
}

pub fn kind_of(p: &Path) -> TargetKind {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.starts_with("claude.exe.old.") {
        return TargetKind::StaleCopy;
    }
    // 按文件名认，不按路径 —— Codex 的落点有五种，路径匹配迟早漏一个。
    if name.eq_ignore_ascii_case("codex.exe") || name.eq_ignore_ascii_case("codex.cmd") {
        return TargetKind::CodexCli;
    }
    let lower = p.to_string_lossy().to_lowercase().replace('/', "\\");
    if lower.contains("\\claude\\claude-code\\") {
        TargetKind::CliVersioned
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_runtime_copies_are_never_lockable() {
        // 给 app-* 加 Deny ACE 会让桌面端开新窗口就崩，这条不能破。
        assert!(is_desktop_runtime_copy(Path::new(
            r"C:\Users\me\AppData\Local\AnthropicClaude\app-1.46388.4\claude.exe"
        )));
        assert!(is_desktop_runtime_copy(Path::new(
            r"c:/users/me/appdata/local/anthropicclaude/app-1.49585.0/claude.exe"
        )));
    }

    #[test]
    fn stub_and_cli_are_not_runtime_copies() {
        assert!(!is_desktop_runtime_copy(Path::new(
            r"C:\Users\me\AppData\Local\AnthropicClaude\claude.exe"
        )));
        assert!(!is_desktop_runtime_copy(Path::new(
            r"C:\Users\me\.local\bin\claude.exe"
        )));
        assert!(!is_desktop_runtime_copy(Path::new(
            r"C:\Users\me\AppData\Roaming\Claude\claude-code\2.1.260\claude.exe"
        )));
    }

    #[test]
    fn classifies_the_new_versioned_layout() {
        assert_eq!(
            kind_of(Path::new(
                r"C:\Users\me\AppData\Roaming\Claude\claude-code\2.1.260\claude.exe"
            )),
            TargetKind::CliVersioned
        );
    }

    #[test]
    fn classifies_stub_and_plain_cli() {
        assert_eq!(
            kind_of(Path::new(
                r"C:\Users\me\AppData\Local\AnthropicClaude\claude.exe"
            )),
            TargetKind::DesktopStub
        );
        assert_eq!(
            kind_of(Path::new(r"C:\Users\me\.local\bin\claude.exe")),
            TargetKind::Cli
        );
    }

    #[test]
    fn classifies_stale_copy_regardless_of_directory() {
        assert_eq!(
            kind_of(Path::new(
                r"C:\Users\me\AppData\Roaming\Claude\claude-code\2.1.258\claude.exe.old.123"
            )),
            TargetKind::StaleCopy
        );
    }

    #[test]
    fn codex_is_recognised_by_filename_not_by_path() {
        // Codex 的落点有五种（npm / .local\bin / Programs / OpenAI\Codex\bin
        // 的哈希目录 / WindowsApps），按路径匹配迟早漏一个。
        for p in [
            r"C:\Users\me\AppData\Roaming\npm\codex.cmd",
            r"C:\Users\me\.local\bin\codex.exe",
            r"C:\Users\me\AppData\Local\OpenAI\Codex\bin\abc123\codex.exe",
            r"C:\Users\me\AppData\Local\Microsoft\WindowsApps\codex.exe",
        ] {
            assert_eq!(kind_of(Path::new(p)), TargetKind::CodexCli, "{p}");
        }
    }

    #[test]
    fn claude_paths_are_never_mistaken_for_codex() {
        assert_eq!(
            kind_of(Path::new(r"C:\Users\me\.local\bin\claude.exe")),
            TargetKind::Cli
        );
        assert_eq!(
            kind_of(Path::new(
                r"C:\Users\me\AppData\Roaming\Claude\claude-code\2.1.260\claude.exe"
            )),
            TargetKind::CliVersioned
        );
        assert_eq!(
            kind_of(Path::new(r"C:\Users\me\.local\bin\claude.exe.old.123")),
            TargetKind::StaleCopy
        );
    }

}
