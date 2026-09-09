//! 受控启动 Claude。
//!
//! # 为什么需要这个模块
//!
//! `gate::open_authorized` 只做「验 IP → 摘掉 Deny ACE → 记租约」，
//! **它不启动任何进程**。旧界面上那两个叫「启动 Claude Code」「启动 Claude
//! 桌面端」的按钮点下去其实一个进程都没起过 —— 用户点完什么都不发生，
//! 还得自己去找 exe 双击，而那个 exe 上恰好挂着 Deny ExecuteFile。
//!
//! 顺序不能拆开写在前端：验 IP、起进程、挂看门狗必须是一件事，
//! 中间任何一步失败都要把租约还回去并重新上锁 ——
//! **门禁不过就一个进程都不起**，这条线在这里守。
//!
//! # 启动方式为什么不一样
//!
//! | 目标 | 起法 | 为什么 |
//! |---|---|---|
//! | Claude Code | `cmd /c start "" <exe>` | 控制台程序。直接 spawn 会没有窗口，用户什么都看不到 |
//! | 桌面端 | 直接 spawn 存根 | GUI 程序，存根会自己拉起 `app-<版本>` 下的真身 |
//!
//! 桌面端起的是 `%LOCALAPPDATA%\AnthropicClaude\claude.exe` 这个 Squirrel 存根，
//! **不是** `app-*` 下那份 —— 那份不能加 Deny ACE，也不该由我们直接拉起。

use crate::error::{GateError, Result};
use crate::gate::watchdog::WatchMode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchTarget {
    ClaudeCode,
    ClaudeDesktop,
}

impl LaunchTarget {
    pub fn label(self) -> &'static str {
        match self {
            LaunchTarget::ClaudeCode => "Claude Code",
            LaunchTarget::ClaudeDesktop => "Claude 桌面端",
        }
    }

    /// 租约持有者标识，会写进 ip-gate.log。
    pub fn holder(self) -> &'static str {
        match self {
            LaunchTarget::ClaudeCode => "claude-code",
            LaunchTarget::ClaudeDesktop => "claude-desktop",
        }
    }

    /// 挂哪一档看门狗。
    ///
    /// 桌面端那档查不到 IP **立即关闭不给宽限** —— 它冻不住，
    /// 「等等看」的实际含义就是让它在无法核实的网络上继续跑。
    pub fn watch_mode(self) -> WatchMode {
        match self {
            LaunchTarget::ClaudeCode => WatchMode::Cli,
            LaunchTarget::ClaudeDesktop => WatchMode::Desktop,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchResult {
    pub target: LaunchTarget,
    pub path: String,
    pub pid: Option<u32>,
    pub watchdog: WatchMode,
    pub detail: String,
}

/// 桌面端存根路径。
///
/// **必须是 `AnthropicClaude\claude.exe` 这个根下的存根**，不能是 `app-*` 下那份：
/// 后者是运行时副本，我们既不给它上锁，也不该绕过存根直接拉它 ——
/// 存根负责挑当前版本，跳过去会在升级后指向旧版。
pub fn desktop_stub() -> Option<PathBuf> {
    let p = dirs::data_local_dir()?.join("AnthropicClaude").join("claude.exe");
    p.is_file().then_some(p)
}

/// 决定要起哪个文件。
pub fn resolve(target: LaunchTarget) -> Result<PathBuf> {
    match target {
        // 与升级流程共用同一套查找，免得「升级的那份」和「启动的那份」不是同一个。
        LaunchTarget::ClaudeCode => crate::plugins::sillytavern::find_official_claude(),
        LaunchTarget::ClaudeDesktop => desktop_stub().ok_or_else(|| {
            GateError::Other(
                "找不到 Claude 桌面端的启动存根，请先在「环境与安装」里装好桌面端。".into(),
            )
        }),
    }
}

#[cfg(windows)]
fn spawn(target: LaunchTarget, exe: &std::path::Path) -> Result<Option<u32>> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    match target {
        LaunchTarget::ClaudeCode => {
            // 控制台程序：借 `start` 让 Windows 给它开一个新控制台窗口。
            // 第一个空引号是窗口标题占位 —— 少了它，带空格的路径会被 start
            // 当成标题，然后什么都不启动。
            let mut cmd = std::process::Command::new("cmd");
            cmd.arg("/c")
                .arg("start")
                .arg("")
                .arg(exe)
                .creation_flags(CREATE_NO_WINDOW);
            if let Some(home) = dirs::home_dir() {
                cmd.current_dir(home);
            }
            cmd.spawn()
                .map_err(|e| GateError::Other(format!("启动 {} 失败：{e}", target.label())))?;
            // 经 `start` 转手之后，我们拿到的是 cmd 的 pid，不是 Claude 的，
            // 报出去只会误导 —— 干脆不报。
            Ok(None)
        }
        LaunchTarget::ClaudeDesktop => {
            let mut cmd = std::process::Command::new(exe);
            if let Some(dir) = exe.parent() {
                cmd.current_dir(dir);
            }
            let child = cmd
                .creation_flags(DETACHED_PROCESS)
                .spawn()
                .map_err(|e| GateError::Other(format!("启动 {} 失败：{e}", target.label())))?;
            Ok(Some(child.id()))
        }
    }
}

#[cfg(not(windows))]
fn spawn(_target: LaunchTarget, _exe: &std::path::Path) -> Result<Option<u32>> {
    Err(GateError::Other("启动功能只在 Windows 上可用".into()))
}

/// 验 IP → 解锁 → 起进程。
///
/// 起失败会把租约还回去并重新上锁 —— 不能留着一扇开着的门。
/// 看门狗由调用方（`lib.rs`）在成功之后挂上，因为那需要 `AppState`。
pub async fn launch(target: LaunchTarget, gate: &crate::gate::GateState) -> Result<LaunchResult> {
    // 先把 exe 找出来。找不到就直接失败，**不要先解锁再发现没东西可起**。
    let exe = resolve(target)?;

    crate::gate::open_authorized(target.holder(), gate).await?;

    let pid = match spawn(target, &exe) {
        Ok(pid) => pid,
        Err(e) => {
            let _ = crate::gate::release_lease(gate);
            return Err(e);
        }
    };

    let detail = format!(
        "{} 已放行并启动（{}）。看门狗每 {} 秒核一次出口 IP。",
        target.label(),
        exe.display(),
        target.watch_mode().interval().as_secs()
    );
    crate::gate::log::write(&detail);

    Ok(LaunchResult {
        target,
        path: exe.display().to_string(),
        pid,
        watchdog: target.watch_mode(),
        detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_gets_the_no_grace_watchdog() {
        // 这两档不能弄反。桌面端查不到 IP 是立即关闭，CLI 是留 180 秒宽限；
        // 反了的话要么会误杀正在工作的 Claude Code，要么会让桌面端
        // 在无法核实的网络上多跑三分钟。
        assert_eq!(LaunchTarget::ClaudeDesktop.watch_mode(), WatchMode::Desktop);
        assert_eq!(LaunchTarget::ClaudeCode.watch_mode(), WatchMode::Cli);
    }

    #[test]
    fn holders_are_distinct_and_stable() {
        // holder 会写进 ip-gate.log，也显示在界面上的「已租给 …」。
        assert_ne!(
            LaunchTarget::ClaudeCode.holder(),
            LaunchTarget::ClaudeDesktop.holder()
        );
        assert_eq!(LaunchTarget::ClaudeCode.holder(), "claude-code");
        assert_eq!(LaunchTarget::ClaudeDesktop.holder(), "claude-desktop");
    }

    #[test]
    fn targets_round_trip_as_kebab_case() {
        // 前端 `LaunchTarget` 是 'claude-code' | 'claude-desktop'。
        assert_eq!(
            serde_json::to_string(&LaunchTarget::ClaudeCode).unwrap(),
            "\"claude-code\""
        );
        let back: LaunchTarget = serde_json::from_str("\"claude-desktop\"").unwrap();
        assert_eq!(back, LaunchTarget::ClaudeDesktop);
    }

    #[test]
    fn desktop_stub_is_never_the_runtime_copy() {
        // 存根在 AnthropicClaude 根下；app-* 那份是运行时副本，
        // 既不上锁也不该直接拉起。这条判断与 targets.rs 的排除逻辑一致。
        if let Some(p) = desktop_stub() {
            assert!(!crate::gate::targets::is_desktop_runtime_copy(&p));
        }
    }
}
