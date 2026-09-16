//! Windows helper-process policy: background commands never create console windows.
//!
//! # ⛔ PowerShell 一律从这里起
//!
//! 面板拉起的 PowerShell 都带 `CREATE_NO_WINDOW`，**没有控制台**。
//! 没有控制台时 `[Console]::OutputEncoding` 会落到系统代码页上，而那个代码页
//! 编不出来的字符会被换成一个字面的 `?` —— 不是乱码，是**真的丢了**，
//! Rust 这边再怎么解码也回不来。
//!
//! 实测（zh-CN，2026-09-16）：`Get-NetAdapter` 的「以太网」到 Rust 手里是 `???`，
//! 「蓝牙网络连接」是 `??????`。出站锁那一页整列网卡名都成了问号，而那串问号
//! 还会被原样送回去当 `-InterfaceAlias` —— 规则加在一个不存在的接口上，
//! 界面却报「已加 N 条」。**这是一个会静默失效的功能，不是显示问题。**
//!
//! 所以拉 PowerShell 只走 [`powershell_std`] / [`powershell_tokio`]：
//! 它们把 `-NoProfile -NonInteractive` 和 UTF-8 前奏一次性钉死，
//! 谁也不用记得加。`src-tauri/tests/architecture.rs` 里有一条测试守着这件事。
//!
//! 这条约束跟区域设置无关：英文机器上 `é`、`ü`、西里尔字母同样编不进
//! CP437/CP850，一样会变成 `?`。开源出去之后这是**每一台机器**的问题。

#[cfg(windows)]
pub fn hidden_std(mut cmd: std::process::Command) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(windows))]
pub fn hidden_std(cmd: std::process::Command) -> std::process::Command {
    cmd
}

#[cfg(windows)]
pub fn hidden_tokio(mut cmd: tokio::process::Command) -> tokio::process::Command {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(windows))]
pub fn hidden_tokio(cmd: tokio::process::Command) -> tokio::process::Command {
    cmd
}

// ---------------------------------------------------------------- PowerShell

/// 把输出编码钉死成 UTF-8 的前奏。
///
/// `New-Object System.Text.UTF8Encoding $false` 是「不带 BOM 的 UTF-8」——
/// 用 `[Text.Encoding]::UTF8` 会在每次输出前面插一个 BOM，`serde_json` 当场
/// 解析失败。整句包在 `try`/`catch` 里：真有控制台时这一行本来就没必要，
/// 而任何一种失败都不该让整段脚本停下来。
const UTF8_PRELUDE: &str =
    "try { [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false } catch { }\n";

/// 给一段脚本加上 UTF-8 前奏。**拼脚本的唯一入口**，单测钉着它的形状。
///
/// 用换行而不是 `;` 分隔：脚本本身可能以注释（`#`）开头，用分号接的话
/// 那一行会把前奏和注释连成一句，后面整段都被注释掉。
pub fn ps_utf8(script: &str) -> String {
    format!("{UTF8_PRELUDE}{script}")
}

/// 起一条读得回非 ASCII 字符的 PowerShell（同步版）。
///
/// 已经带上 `-NoProfile -NonInteractive` 与 UTF-8 前奏，调用方只要 `.output()`。
/// **别再自己 `Command::new("powershell")`** —— 见本文件头。
pub fn powershell_std(script: &str) -> std::process::Command {
    let mut cmd = hidden_std(std::process::Command::new("powershell"));
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &ps_utf8(script),
    ]);
    cmd
}

/// 起一条读得回非 ASCII 字符的 PowerShell（异步版）。见 [`powershell_std`]。
pub fn powershell_tokio(script: &str) -> tokio::process::Command {
    let mut cmd = hidden_tokio(tokio::process::Command::new("powershell"));
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &ps_utf8(script),
    ]);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 前奏必须在最前面 —— 编码要在**第一句输出之前**就定下来。
    #[test]
    fn the_prelude_comes_before_the_script() {
        let s = ps_utf8("Get-NetAdapter");
        assert!(s.starts_with("try {"), "{s}");
        assert!(s.ends_with("Get-NetAdapter"), "{s}");
    }

    /// ⛔ 必须是**不带 BOM** 的 UTF8Encoding。
    ///
    /// 写成 `[Text.Encoding]::UTF8` 会在输出最前面插 `\u{feff}`，
    /// 而这些脚本的产物大多直接喂给 `serde_json` —— BOM 进去当场解析失败，
    /// 而且失败信息只会说「第 1 行有意外字符」，查半天查不到这里。
    #[test]
    fn the_encoding_carries_no_bom() {
        let s = ps_utf8("x");
        assert!(
            s.contains("New-Object System.Text.UTF8Encoding $false"),
            "{s}"
        );
    }

    /// 前奏与脚本之间是换行，不是分号。
    ///
    /// 用分号的话，以 `#` 注释开头的脚本会被整段接到同一行上注释掉。
    #[test]
    fn a_script_that_starts_with_a_comment_survives() {
        let s = ps_utf8("# 说明\nGet-NetAdapter");
        let last = s.lines().last().unwrap_or_default();
        assert_eq!(last, "Get-NetAdapter", "前奏把脚本吞进注释里了：{s}");
    }

    /// 前奏永远包在 try/catch 里：有控制台的场合它本来就多余，
    /// 而任何一种失败都不该把整段脚本带停。
    #[test]
    fn the_prelude_can_never_abort_the_script() {
        let s = ps_utf8("x");
        assert!(s.contains("try {"), "{s}");
        assert!(s.contains("catch { }"), "{s}");
    }
}
