//! 运行期数据目录。**地基层：不许依赖本项目的任何别的模块。**
//!
//! # 为什么从 `gate` 里搬出来
//!
//! 这个函数原来叫 `gate::state_dir`。它跟门禁毫无关系 —— 它回答的是
//! 「本机数据放哪」，而**全项目 12 个模块只为了问这一句就 `use crate::gate`**：
//! `config_io`、`repository`、`settings`、`progress`、`profile`、`legacy`、
//! `relay`、`plugins` …… 这些模块本该在门禁下面，却因此反过来依赖它。
//!
//! 结果是 `gate` 的入度高到 25，成了一个「所有人都依赖、又反过来调所有人」的
//! 伪底层 —— 15 对循环依赖里有 9 对缠着它。搬走这一个函数（加上 `audit`），
//! 12 个模块立刻脱钩。
//!
//! # 规矩
//!
//! 这里只放**纯路径拼接**：不读盘、不建目录、不判断存在性。谁要用谁自己
//! `create_dir_all`。混进 I/O 的话，这一层就又变成一个「所有人都得依赖的
//! 活动件」，跟搬出来之前是一个下场。

use std::path::PathBuf;

/// `%LOCALAPPDATA%\ClaudeIpGate`。
///
/// 取不到 `data_local_dir` 时退回当前目录 —— **不 panic**。
/// 面板起不来比数据目录不标准严重得多，而这个函数在启动最早期就会被调到。
pub fn state_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ClaudeIpGate")
}

/// 酒馆桥接的数据目录。
///
/// 放在这里而不是 `plugins::sillytavern` 里，是因为 `accounts` 也要用它 ——
/// Claude 的配置可能落在这个目录下，切账户时得认得出来。原来 `AccountRoots::current`
/// 为了问一句路径而 `use crate::plugins`，而 `plugins` 反过来又要 `AccountRoots`：
/// `accounts ↔ plugins` 这对环就是这么来的。一个零依赖的纯路径函数不该制造这个。
pub fn tavern_bridge_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_default()
        .join("ClaudeTavernBridge")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_dir_always_ends_with_the_product_folder() {
        // 目录名是使用者机器上真实存在的那一个。改它等于让所有老用户的
        // 账户槽位、租约、白名单、快照一起「消失」—— 这条钉住它。
        assert!(state_dir().ends_with("ClaudeIpGate"));
    }

    #[test]
    fn a_missing_local_app_data_falls_back_instead_of_panicking() {
        // 这个函数在启动最早期被调用（panic hook 与日志都要它）。
        // 它 panic 的话，面板会在任何诊断手段就位之前就死掉。
        let p = state_dir();
        assert!(p.is_absolute() || p.starts_with("."));
    }
}
