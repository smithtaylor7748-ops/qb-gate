// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 开机自启（2026-10-27，中转站重构阶段 7 · X7，照 cc-switch v4.0.14 的 `auto_launch.rs`）。
//!
//! 登记在当前用户的 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`，值名 `QB Gate`，
//! 内容是 `"<这个 qb-gate.exe 的完整路径>" --autostart`。带着 [`ARG`] 起来的面板不露窗口、只留托盘
//! （`src-tauri/src/window.rs`）。
//!
//! # 值名必须就是产品名
//!
//! Tauri 的 NSIS 卸载程序（升级时不跑这一步）会删 `HKCU\…\Run` 下名叫 `${PRODUCTNAME}` 的那一项 ——
//! 2026-10-27 从 Tauri CLI 2.11.4 内嵌的模板里核过：`DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"`，
//! 外面包着 `${If} $UpdateMode <> 1`。所以 [`VALUE_NAME`] 跟 `tauri.conf.json` 的 `productName` 一个字都不能差：
//! 差了，卸掉面板之后每次开机还在起一个已经不存在的 exe。`src-tauri/tests/bundle.rs` 对着配置文件钉这件事。
//!
//! # 任务管理器里的「禁用」
//!
//! 使用者在任务管理器 / 设置 › 启动应用里关掉它时，Windows **不删** `Run` 那一项，而是在
//! [`APPROVED_KEY`] 下记一条同名的二进制值（首字节是奇数 = 禁用）。只看 `Run` 会把「被禁用了」说成「开着」。
//! 在面板里重新打开是使用者的明确意思：那条禁用记录一起删掉（没有记录 = 允许）。
//!
//! # 开发版不登记
//!
//! 登记的是「这个 exe」的路径。`npm run tauri dev` 起的是 `target\debug\qb-gate.exe` —— 登记下去，
//! 开机起的就是一份编译中间产物，连着开发分支的数据库结构（坑 7.106 那一类事）。开发版直接拒绝。
//!
//! cc-switch 用的是 `auto-launch` crate；这里只有 Windows 的 `Run` 一处，直接走平台层的 [`crate::registry`]，
//! 不另引依赖（也不起 `reg.exe`，理由在 `registry` 的模块头）。

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::error::{GateError, Result};
use crate::registry::{self, Hive, Value};

/// 开机自启登记在哪。
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// 任务管理器 / 设置 › 启动应用的「禁用」记在哪。
pub const APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
/// 值名。⛔ 必须等于 `tauri.conf.json` 的 `productName`，见模块头。
pub const VALUE_NAME: &str = "QB Gate";
/// 开机自启带的参数：面板据此不露窗口。
pub const ARG: &str = "--autostart";

/// 开机自启此刻的样子。**以注册表为准，不在设置文件里另存一份** —— 两处存同一件事，迟早一处说开一处说关。
#[derive(Debug, Clone, Serialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct AutostartStatus {
    /// `Run` 下有没有 `QB Gate` 这一项。
    pub registered: bool,
    /// 有，但在任务管理器 / 设置 › 启动应用里被关掉了。
    pub disabled_by_user: bool,
    /// 登记的那一行命令，原样（界面上显示，好让人看出指向哪一份）。
    pub command: Option<String>,
    /// 登记的 exe 就是现在跑着的这一个。
    pub points_here: bool,
    /// 开机时真的会起这一份：登记了、没被禁用、指向这里。
    pub effective: bool,
    /// 这是开发版：不许登记（见模块头）。
    pub dev_build: bool,
}

/// 登记进 `Run` 的那一行：路径一律加引号（路径里有空格 —— 默认装在 `…\QB Gate\`），后面跟 [`ARG`]。
pub fn command_for(exe: &Path) -> String {
    format!("\"{}\" {ARG}", exe.display())
}

/// 从登记的那一行里取出 exe 路径。认引号包着的，也认没引号的（别的工具、旧写法）。
pub fn exe_in(command: &str) -> Option<PathBuf> {
    let c = command.trim();
    if let Some(rest) = c.strip_prefix('"') {
        let end = rest.find('"')?;
        let p = &rest[..end];
        return (!p.trim().is_empty()).then(|| PathBuf::from(p));
    }
    // 没引号：到 `.exe` 为止（路径里可能有空格，不能按空格切）。
    let lower = c.to_ascii_lowercase();
    let end = lower.find(".exe").map(|i| i + 4).unwrap_or(c.len());
    let p = c[..end].trim();
    (!p.is_empty()).then(|| PathBuf::from(p))
}

/// `StartupApproved\Run` 里那条记录说的是不是「禁用」。首字节奇数（3、7……）是禁用，偶数（2、6）是允许；
/// 读不出来（空的）按允许算 —— 跟 Windows 自己一样，没有记录就是允许。
pub fn disabled_by(approved: &[u8]) -> bool {
    approved.first().is_some_and(|b| b & 1 == 1)
}

/// 两个路径是不是同一个 exe。Windows 路径不分大小写，`/` 和 `\` 一样。
pub fn same_exe(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    norm(a) == norm(b)
}

/// 这一次是不是开机自启起来的。
pub fn launched_by_autostart<I: IntoIterator<Item = S>, S: AsRef<str>>(args: I) -> bool {
    args.into_iter().any(|a| a.as_ref() == ARG)
}

fn dev_build() -> bool {
    cfg!(debug_assertions)
}

fn this_exe() -> Result<PathBuf> {
    std::env::current_exe().map_err(|e| GateError::Other(format!("读不到面板自己的路径：{e}")))
}

/// 读现状。
pub fn status() -> Result<AutostartStatus> {
    let command = registry::read(Hive::CurrentUser, RUN_KEY, VALUE_NAME)?
        .and_then(|v| v.as_text().map(str::to_string));
    let disabled_by_user = match registry::read(Hive::CurrentUser, APPROVED_KEY, VALUE_NAME)? {
        Some(Value::Binary(b)) => disabled_by(&b),
        _ => false,
    };
    let here = this_exe()?;
    let points_here = command
        .as_deref()
        .and_then(exe_in)
        .is_some_and(|p| same_exe(&p, &here));
    let registered = command.is_some();
    Ok(AutostartStatus {
        registered,
        disabled_by_user: registered && disabled_by_user,
        points_here,
        effective: registered && !disabled_by_user && points_here,
        command,
        dev_build: dev_build(),
    })
}

/// 登记成「开机起这一份」。已经登记了别的路径（另一份 QB Gate）就改成这一份。
pub fn enable() -> Result<AutostartStatus> {
    if dev_build() {
        return Err(GateError::Other(
            "这是开发版（npm run tauri dev），不登记开机自启 —— 登记下去开机起的会是一份编译中间产物。用装好的面板来开。"
                .into(),
        ));
    }
    let exe = this_exe()?;
    registry::write_user(RUN_KEY, VALUE_NAME, &Value::Text(command_for(&exe)))?;
    // 任务管理器里点过「禁用」的话，那条记录还在 —— 不删，`Run` 写了也不起。
    registry::delete_user(APPROVED_KEY, VALUE_NAME)?;
    status()
}

/// 取消登记。`StartupApproved` 里的同名记录一起清掉，不留残渣。
pub fn disable() -> Result<AutostartStatus> {
    registry::delete_user(RUN_KEY, VALUE_NAME)?;
    registry::delete_user(APPROVED_KEY, VALUE_NAME)?;
    status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_quotes_a_path_with_spaces_and_reads_back() {
        let exe = Path::new(r"C:\Users\张三\AppData\Local\QB Gate\qb-gate.exe");
        let c = command_for(exe);
        assert_eq!(
            c,
            r#""C:\Users\张三\AppData\Local\QB Gate\qb-gate.exe" --autostart"#
        );
        assert_eq!(exe_in(&c).as_deref(), Some(exe));
        assert!(launched_by_autostart(["qb-gate.exe", ARG]));
        assert!(!launched_by_autostart(["qb-gate.exe", "--autostartx"]));
    }

    #[test]
    fn an_unquoted_registration_still_names_its_exe() {
        assert_eq!(
            exe_in(r"C:\Program Files\QB Gate\qb-gate.exe --autostart").as_deref(),
            Some(Path::new(r"C:\Program Files\QB Gate\qb-gate.exe"))
        );
        assert_eq!(exe_in(r#""""#), None);
        assert_eq!(exe_in("   "), None);
    }

    #[test]
    fn startup_approved_odd_first_byte_means_disabled() {
        assert!(!disabled_by(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert!(!disabled_by(&[6, 0, 0, 0]));
        assert!(disabled_by(&[3, 0, 0, 0, 0x10, 0x20, 0, 0, 0, 0, 0, 0]));
        assert!(disabled_by(&[7]));
        assert!(!disabled_by(&[]), "没有记录 = 允许");
    }

    #[test]
    fn exe_paths_compare_like_windows_does() {
        assert!(same_exe(
            Path::new(r"C:\Users\A\AppData\Local\QB Gate\qb-gate.exe"),
            Path::new("c:/users/a/appdata/local/qb gate/QB-GATE.EXE")
        ));
        assert!(!same_exe(
            Path::new(r"C:\a\qb-gate.exe"),
            Path::new(r"D:\a\qb-gate.exe")
        ));
    }

    #[test]
    fn a_dev_build_refuses_to_register_itself() {
        // `cargo test` 是 debug 编的：这条走的正是开发版那条路，**在碰注册表之前**就拒绝。
        if dev_build() {
            let e = enable().unwrap_err().to_string();
            assert!(e.contains("开发版"), "{e}");
        }
    }
}
