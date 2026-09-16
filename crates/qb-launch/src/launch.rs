//! Executable inventory adapters and client defaults. Process launch is owned by sessions.

use crate::error::{GateError, Result};
use crate::gate::watchdog::WatchMode;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchTarget {
    ClaudeCode,
    ClaudeDesktop,
    Codex,
}

impl LaunchTarget {
    /// 客户端 → 启动目标。**全项目只有这一张映射表。**
    ///
    /// 跟「Claude 装在哪只有一张 inventory」是同一个理由：可执行文件、
    /// 门禁开关、看门狗档位三样都从这里分叉。各映射各的，漏掉的那一处
    /// 就会悄悄接错档 —— `session_launch` 原来就硬写着 `WatchMode::Cli`，
    /// 经它启动的桌面端挂的是命令行那一档。
    pub fn of(client: crate::domain::Client) -> Self {
        match client {
            crate::domain::Client::ClaudeCode => LaunchTarget::ClaudeCode,
            crate::domain::Client::ClaudeDesktop => LaunchTarget::ClaudeDesktop,
            crate::domain::Client::Codex => LaunchTarget::Codex,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LaunchTarget::ClaudeCode => "Claude Code",
            LaunchTarget::ClaudeDesktop => "Claude 桌面端",
            LaunchTarget::Codex => "Codex 桌面端",
        }
    }

    /// 租约持有者标识，会写进 ip-gate.log。
    pub fn holder(self) -> &'static str {
        match self {
            LaunchTarget::ClaudeCode => "claude-code",
            LaunchTarget::ClaudeDesktop => "claude-desktop",
            LaunchTarget::Codex => "codex",
        }
    }

    /// 挂哪一档看门狗。
    ///
    /// 桌面端那档查不到 IP **立即关闭不给宽限** —— 它冻不住，
    /// 「等等看」的实际含义就是让它在无法核实的网络上继续跑。
    pub fn watch_mode(self) -> WatchMode {
        match self {
            LaunchTarget::ClaudeCode => WatchMode::Cli,
            LaunchTarget::ClaudeDesktop | LaunchTarget::Codex => WatchMode::Desktop,
        }
    }

    /// 这个可执行文件归门禁管吗 —— 起之前要验 IP 解锁，起来之后持租约。
    ///
    /// Codex 只有在设置里打开开关之后才归门禁管；关着的时候
    /// 「启动 Codex」就是单纯起个进程，不验 IP、不动任何 ACL。
    ///
    /// ⚠ 这**不是**「判不过时收不收这个会话」—— 那是 [`Self::stops_with_gate`]。
    /// 两件事必须分开，理由见那个函数。
    pub fn gated(self) -> bool {
        match self {
            LaunchTarget::Codex => crate::settings::codex_under_gate(),
            _ => true,
        }
    }

    /// 门禁判不过的时候，要不要收掉这个会话？
    ///
    /// # 为什么跟 [`Self::gated`] 是两件事
    ///
    /// Deny ACE 是**按文件**加的，不认身份：中转会话要起得来，claude.exe
    /// 同样得先解锁。所以「中转不挂门禁」不可能是「中转绕过门禁」——
    /// 那就成了现成的绕过入口（拿中转会话把文件解锁，再从终端起官方的）。
    /// 解锁与持租约照旧走 `gated`。
    ///
    /// 变的是**处置**：出口 IP 一变，看门狗照常上锁、照常收掉官方进程，
    /// 但**不动中转会话**。理由是门禁存在的唯一目的 —— 别让你的 Anthropic
    /// 官方账号在没核实过的出口上发请求。中转请求打的是第三方端点、
    /// 用的是你自己买的 API Key、不带官方 OAuth 身份，Anthropic 那边根本
    /// 看不见。收它，保护收益是零，代价是把正在写的对话弄丢 ——
    /// 而中转站的典型使用者，恰恰就是出口 IP 会变的那群人。
    ///
    /// 同理，会话内 hook 也不往中转环境目录里写（`gate::hook::set_hook`）。
    pub fn stops_with_gate(self, identity: crate::domain::IdentityKind) -> bool {
        self.gated() && identity != crate::domain::IdentityKind::Relay
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LaunchResult {
    pub target: LaunchTarget,
    pub path: String,
    pub pid: Option<u32>,
    pub watchdog: WatchMode,
    pub detail: String,
    /// Claude Code 用的是哪个账户槽位。`None` = 没有激活槽位，用的是默认目录。
    pub slot: Option<String>,
}

/// 关掉非必要遥测的那几个变量。
///
/// **只列 Claude Code 自己文档里有的，外加一个通用标准 `DO_NOT_TRACK`。**
/// 不照着别的项目抄一长串：设一个不存在的变量等于什么都没关，
/// 而界面上却写着「已关闭」—— 那是在说谎。界面上会把这几个原样列出来，
/// 使用者自己就能核。
///
/// ⚠ **这跟封号风险无关。** 你的 API 请求照样带着账号凭证发给 Anthropic，
/// 出口 IP 照样是那个 IP。它关掉的只是崩溃报告与使用统计这类非必要流量，
/// 跟在编辑器里关遥测是同一件事。
pub const TELEMETRY_OFF: &[(&str, &str)] = &[
    ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
    ("DISABLE_TELEMETRY", "1"),
    ("DISABLE_ERROR_REPORTING", "1"),
    ("DISABLE_BUG_COMMAND", "1"),
    ("DO_NOT_TRACK", "1"),
];

/// Claude Code 的启动环境。纯函数，可单测 —— 两个变量为什么必须有，见文件头。
pub fn claude_code_env(slot_dir: Option<&Path>) -> Vec<(&'static str, OsString)> {
    claude_code_env_with(slot_dir, crate::settings::load().disable_telemetry)
}

pub fn claude_code_env_with(
    slot_dir: Option<&Path>,
    disable_telemetry: bool,
) -> Vec<(&'static str, OsString)> {
    let mut v: Vec<(&'static str, OsString)> = vec![("DISABLE_AUTOUPDATER", "1".into())];
    if let Some(d) = slot_dir {
        v.push(("CLAUDE_CONFIG_DIR", d.as_os_str().to_owned()));
    }
    if disable_telemetry {
        for (k, val) in TELEMETRY_OFF {
            v.push((k, (*val).into()));
        }
    }
    v
}

/// 桌面端存根路径。
///
/// **必须是 `AnthropicClaude\claude.exe` 这个根下的存根**，不能是 `app-*` 下那份：
/// 后者是运行时副本，我们既不给它上锁，也不该绕过存根直接拉它 ——
/// 存根负责挑当前版本，跳过去会在升级后指向旧版。
pub fn desktop_stub() -> Option<PathBuf> {
    let p = dirs::data_local_dir()?
        .join("AnthropicClaude")
        .join("claude.exe");
    p.is_file().then_some(p)
}

/// 决定要起哪个文件。
pub fn resolve(target: LaunchTarget) -> Result<PathBuf> {
    match target {
        // 与检测、上锁共用同一张位置表（install::inventory），
        // 免得「检测到的那份」「锁住的那份」「启动的那份」各是各的。
        // npm 装的 claude.cmd 也在候选里（排最后）—— 它锁不上，但照样要先验 IP 再起。
        LaunchTarget::ClaudeCode => {
            crate::install::inventory::preferred_cli(&crate::install::inventory::Roots::current())
                .map(|i| i.path)
                .ok_or_else(|| {
                    GateError::Other("找不到 Claude Code，请先在「环境与安装」里装好。".into())
                })
        }
        LaunchTarget::ClaudeDesktop => desktop_stub().ok_or_else(|| {
            GateError::Other(
                "找不到 Claude 桌面端的启动存根（%LOCALAPPDATA%\\AnthropicClaude\\claude.exe）。\
                 没装的话先在「环境与安装」里装好；如果是 MSIX 方式装的，请从开始菜单打开 ——\
                 面板没法替 MSIX 版上锁或启动。"
                    .into(),
            )
        }),
        // 与检测共用同一份候选路径表，免得「检测到的那份」和「启动的那份」
        // 不是同一个。
        LaunchTarget::Codex => crate::install::codex_desktop::detect()?
            .executable
            .map(PathBuf::from)
            .ok_or_else(|| {
                GateError::Other("未找到 Codex 桌面端，请先安装 Microsoft Store 桌面应用。".into())
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 遥测开关**默认不设任何变量** —— 关掉的开关就该什么都不做。
    #[test]
    fn telemetry_vars_are_absent_by_default() {
        let env = claude_code_env_with(None, false);
        for (k, _) in TELEMETRY_OFF {
            assert!(!env.iter().any(|(n, _)| n == k), "{k} 不该在默认环境里");
        }
    }

    /// 打开之后这几个必须都在，而且值要对得上界面上列的那份。
    ///
    /// 界面上把变量名原样印出来供使用者核对 —— 两边对不上就是在骗他。
    #[test]
    fn telemetry_vars_match_what_the_ui_lists() {
        let env = claude_code_env_with(None, true);
        for (k, want) in TELEMETRY_OFF {
            let got = env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
            assert_eq!(
                got.as_deref().and_then(|o| o.to_str()),
                Some(*want),
                "{k} 没设或者值不对"
            );
        }
        // 原有的两个变量不能因为加了遥测就丢了。
        assert!(env.iter().any(|(n, _)| *n == "DISABLE_AUTOUPDATER"));
    }

    #[test]
    fn desktop_gets_the_no_grace_watchdog() {
        // 这两档不能弄反。两个档位查不到 IP 都立即关闭；
        // 反了的话要么会误杀正在工作的 Claude Code，要么会让桌面端
        // 在无法核实的网络上多跑三分钟。
        assert_eq!(LaunchTarget::ClaudeDesktop.watch_mode(), WatchMode::Desktop);
        assert_eq!(LaunchTarget::ClaudeCode.watch_mode(), WatchMode::Cli);
    }

    /// 「解锁 / 持租约」与「判不过时收不收」必须是两个判断。
    ///
    /// 合成一个的话只有两种结局，都不能要：
    /// * 合成「中转不走门禁」→ claude.exe 还锁着，中转会话根本起不来；
    ///   真让它绕过解锁，就是现成的绕过入口（拿中转把文件解锁，
    ///   再从终端起官方的）。
    /// * 合成「中转照收」→ 换个 WiFi 就把正在写的中转对话弄丢，
    ///   而中转请求根本不经过 Anthropic，收它一点保护都没换来。
    #[test]
    fn relay_sessions_still_unlock_but_are_never_stopped_by_the_gate() {
        use crate::domain::IdentityKind;
        for target in [LaunchTarget::ClaudeCode, LaunchTarget::ClaudeDesktop] {
            // 可执行文件那一侧：两种身份都要先解锁才起得来。
            assert!(target.gated(), "{target:?} 必须先验 IP 才能解锁");
            assert!(target.stops_with_gate(IdentityKind::Official));
            assert!(
                !target.stops_with_gate(IdentityKind::Relay),
                "{target:?} 的中转会话不该被门禁收掉"
            );
        }
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
    fn claude_code_gets_the_slot_dir_and_no_autoupdate() {
        // 回归：移植时这两个变量都丢了。丢了 CLAUDE_CONFIG_DIR，切到哪个槽位
        // Claude Code 都读默认目录、弹登录界面；丢了 DISABLE_AUTOUPDATER，
        // 自动更新写的新 exe 没有 Deny，门禁跟着旧文件没了。
        let dir =
            std::path::Path::new(r"C:\Users\me\AppData\Local\ClaudeIpGate\claude-profile-main");
        let env = claude_code_env(Some(dir));
        let get = |k: &str| env.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
        assert_eq!(get("CLAUDE_CONFIG_DIR"), Some(dir.as_os_str().to_owned()));
        assert_eq!(get("DISABLE_AUTOUPDATER"), Some("1".into()));
        // 必须是具体槽位目录，不能是 claude-profile 联结点本身 ——
        // 联结点换指向之后旧会话会穿过它把 token 写进别的槽位。
        assert!(!get("CLAUDE_CONFIG_DIR")
            .unwrap()
            .to_string_lossy()
            .ends_with(r"\claude-profile"));
    }

    #[test]
    fn without_a_slot_claude_code_keeps_its_own_default_dir() {
        let env = claude_code_env(None);
        assert!(env.iter().all(|(k, _)| *k != "CLAUDE_CONFIG_DIR"));
        assert!(env
            .iter()
            .any(|(k, v)| *k == "DISABLE_AUTOUPDATER" && v == "1"));
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
