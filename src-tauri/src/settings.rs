//! 面板自己的开关。存 `%LOCALAPPDATA%\ClaudeIpGate\settings.json`。
//!
//! # 只放「会改变程序行为」的开关
//!
//! 界面偏好（折叠状态、当前分栏那些）走 `useSession`，关掉面板就没了，
//! 不进这个文件。这里放的是**下次启动仍然生效、且会改变门禁行为**的东西。
//!
//! # 每个开关的默认值都必须是「最不意外」的那个
//!
//! 默认值决定了用户装完不动任何设置时的行为。任何会让某个命令**突然跑不起来**
//! 的开关，默认都得是关的 —— 否则用户升级一次面板，第二天发现 codex 打不开，
//! 而他根本不知道是这个程序干的。

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Codex 要不要也归 IP 门禁管。
    ///
    /// **默认 false，而且必须一直是 false。** 打开之后 `codex` 会跟
    /// `claude.exe` 一样被加上 Deny ExecuteFile —— 出口 IP 不在白名单时
    /// 命令直接被系统拒绝执行。这对「请求不能从没核实过的 IP 出去」是对的，
    /// 但对一个正在用 Codex 干活的人来说是个突然的变化，
    /// 所以只能由他自己在设置里打开。
    pub codex_under_gate: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            codex_under_gate: false,
        }
    }
}

pub fn path() -> std::path::PathBuf {
    crate::gate::state_dir().join("settings.json")
}

/// 读。文件不在或者内容坏了都退回默认值 ——
/// 一个读不出来的设置文件不该让面板起不来。
pub fn load() -> Settings {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(s: &Settings) -> Result<()> {
    if let Some(d) = path().parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path(), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

/// 门禁那边每次枚举目标都要问一次，单独提出来省得到处 `load()`。
pub fn codex_under_gate() -> bool {
    load().codex_under_gate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_gate_defaults_to_off() {
        // 打开它会让 codex 在 IP 不合规时**直接跑不起来**。
        // 默认开着的话，用户升级一次面板第二天就发现 codex 打不开，
        // 而且不知道是这个程序干的。
        assert!(!Settings::default().codex_under_gate);
    }

    #[test]
    fn broken_or_missing_file_falls_back_to_defaults() {
        for text in ["", "{ not json", "null", "[]"] {
            let s: Settings = serde_json::from_str(text).unwrap_or_default();
            assert!(!s.codex_under_gate);
        }
    }

    #[test]
    fn unknown_keys_do_not_break_older_builds() {
        // 新版加了字段、用户又装回旧版时，旧版得能照常读。
        let s: Settings =
            serde_json::from_str(r#"{"codex_under_gate":true,"something_new":42}"#).unwrap();
        assert!(s.codex_under_gate);
    }

    #[test]
    fn missing_field_uses_the_default_not_an_error() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(!s.codex_under_gate);
    }
}
