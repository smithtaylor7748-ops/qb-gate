//! 门禁要锁哪些可执行文件。
//!
//! **每一个完整可执行的 claude.exe 副本都必须锁上。** 漏掉一个，那一个就是
//! 绕过 IP 门禁的现成入口 —— 这是原档案 K1 记下的教训（升级残留的
//! `claude.exe.old.*` 没有 Deny ACL，等于一条敞开的路）。
//!
//! # v0.8.0：位置表搬去了 `install::inventory`
//!
//! 这个文件原来自己拼一份路径表，检测、启动、升级又各拼一份，四份已经对不上
//! （只用 winget 装的人：这里锁住了，启动那边找不到）。现在**只有**
//! `install::inventory` 知道 Claude 装在哪，这里只负责「哪些要锁」与显示分类。
//! 表格也在那个文件头上。
//!
//! 仍然成立的两条：
//!
//! * `%LOCALAPPDATA%\AnthropicClaude\app-<版本>\claude.exe` **不锁** ——
//!   一加 Deny，桌面端开新窗口就崩。那份只能靠看门狗收。这是已知残留缺口：
//!   刻意翻进 app-* 目录直接双击能绕开**启动**门禁，5 秒内会被看门狗收掉，
//!   前提是当时有看门狗在跑。要彻底堵死需要 AppLocker / WDAC，不在本项目范围。
//! * npm 装的 `claude.cmd` 锁不了（批处理由 cmd.exe 读进去执行），
//!   不进这张清单，界面上如实说明。

use std::path::{Path, PathBuf};
use ts_rs::TS;

#[derive(Debug, Clone, serde::Serialize, TS)]
#[ts(export, rename = "GateTarget")]
pub struct Target {
    pub path: PathBuf,
    pub kind: TargetKind,
    pub exists: bool,
    pub locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, TS)]
#[ts(export, rename = "GateTargetKind")]
pub enum TargetKind {
    /// 面板托管安装的那份（`<托管根目录>\claude-code\claude.exe`，v0.9.0）
    Managed,
    /// 面板版本库里留着给回滚用的旧版本 `<托管根>\claude-codeersions\<版本>\`。
    /// **每一份都是完整可执行的**，所以照样要锁 —— 漏一个就是现成的绕过入口。
    ManagedVersion,
    /// Claude Code CLI（`.local\bin`、winget、Scoop、PATH 上的、npm 包里的 exe）
    Cli,
    /// 桌面端带的版本化 CLI 副本：`%APPDATA%\Claude*\claude-code\<版本>\`
    CliVersioned,
    /// 官方原生安装器的版本库 `~\.local\share\claude\versions\<版本>` —— 每个都是完整二进制
    NativeVersion,
    /// 编辑器扩展自带的 `claude.exe`（VS Code / Cursor / Windsurf …）
    EditorExtension,
    /// 桌面端 Squirrel 存根 —— 开始菜单与桌面的 Claude.lnk 指向它
    DesktopStub,
    /// 升级残留的旧副本，**没有 Deny ACL，是可绕过的执行副本**
    StaleCopy,
    /// Codex CLI。**只在设置里打开「Codex 也归门禁管」之后才会出现在清单里。**
    CodexCli,
    /// 反重力（Hub 与 IDE 的主程序、语言服务器、第三方壳留下的 `*.original.exe`）。
    /// 默认归门禁（0.26.0，使用者定的）；在「IP 锁」弹窗里移出后不再出现在清单里。
    Antigravity,
}

/// 可锁目标清单，带分类。**不含** `AnthropicClaude\app-*`。
/// 安装清单上的一类，在门禁清单上显示成哪一类。
///
/// 原来是 `inventory::Kind` 上的一个方法，于是「Claude 装在哪」这张表
/// 反过来依赖了门禁。映射关系属于门禁这一侧 —— 是它决定怎么归类，
/// 清单只负责说有哪些。
pub fn target_kind(kind: crate::install::inventory::Kind) -> TargetKind {
    use crate::install::inventory::Kind as K;
    use TargetKind as T;
    match kind {
        K::Managed => T::Managed,
        K::ManagedVersion => T::ManagedVersion,
        K::NativeVersion => T::NativeVersion,
        K::DesktopManaged | K::MsixManaged => T::CliVersioned,
        K::Editor => T::EditorExtension,
        K::DesktopStub => T::DesktopStub,
        _ => T::Cli,
    }
}

pub fn lockable_with_kinds() -> Vec<(PathBuf, TargetKind)> {
    let roots = crate::install::inventory::Roots::current();
    let mut out: Vec<(PathBuf, TargetKind)> = crate::install::inventory::lockable_paths(&roots)
        .into_iter()
        .map(|(p, k)| (p, target_kind(k)))
        .collect();

    // Codex **默认不在门禁管辖内**。打开这个开关之后 `codex` 会跟
    // claude.exe 一样被 Deny ExecuteFile 挡住 —— 这对「请求不能从没核实过的
    // IP 出去」是对的，但对正在用 Codex 干活的人是个突然的变化，
    // 所以只能由他自己在设置里打开。见 `settings.rs`。
    if crate::settings::codex_under_gate() {
        out.extend(
            codex_lockable()
                .into_iter()
                .map(|p| (p, TargetKind::CodexCli)),
        );
    }

    // 反重力**默认归门禁**（0.26.0）。两个产品共用一个开关，位置表在 `install::antigravity`。
    if crate::settings::antigravity_under_gate() {
        out.extend(
            antigravity_lockable(&roots)
                .into_iter()
                .map(|p| (p, TargetKind::Antigravity)),
        );
    }

    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// 可锁目标清单。**不含** `AnthropicClaude\app-*`。
pub fn lockable() -> Vec<PathBuf> {
    lockable_with_kinds().into_iter().map(|(p, _)| p).collect()
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

/// 反重力侧可锁的副本：两个产品下此刻存在的每一份 exe（卸载器除外）。
///
/// 主程序也锁 —— 它不是 Claude 桌面端那种 `app-*` 运行时副本：面板起它之前先解锁、
/// 持租约期间它拉自己的子进程都在解锁状态下，看门狗收的时候先收进程再上锁。
pub fn antigravity_lockable(roots: &crate::install::inventory::Roots) -> Vec<PathBuf> {
    roots
        .local
        .as_deref()
        .map(crate::install::antigravity::lockable_paths)
        .unwrap_or_default()
}

/// 判断一个路径是不是「不能加 Deny ACE」的桌面端运行时副本。
///
/// 这条判断是把 `app-*` 排除在外的那道闸，本体在 `install::inventory`。
pub fn is_desktop_runtime_copy(p: &Path) -> bool {
    crate::install::inventory::is_desktop_runtime_copy(p)
}

/// 升级残留：`claude.exe.old.<时间戳>`。
///
/// 这些副本没有 Deny ACL，是能绕过门禁的执行副本，必须清掉。但注意：
/// **发起升级的那个会话自己占着最新的一份，删不掉**（Windows 允许改名正在
/// 运行的 exe，不允许删除）。所以清理放在**下一次**升级的开头。
///
/// 扫的是每一份副本所在的目录，不再只看 `.local\bin` 与当前桌面端资料 ——
/// winget、Scoop、别的桌面端资料目录下的残留原来都扫不到。
pub fn stale_copies() -> Vec<PathBuf> {
    crate::install::inventory::stale_copies(&crate::install::inventory::Roots::current())
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
    // 反重力按目录认（位置表只有那两个目录）：`\programs\antigravity\` 与 `\programs\antigravity ide\`。
    if lower.contains("\\programs\\antigravity\\")
        || lower.contains("\\programs\\antigravity ide\\")
    {
        return TargetKind::Antigravity;
    }
    if lower.contains("\\.local\\share\\claude\\versions\\") {
        TargetKind::NativeVersion
    } else if lower.contains("\\extensions\\anthropic.claude-code") {
        TargetKind::EditorExtension
    } else if lower.contains("\\claude-code\\") {
        // `Claude\claude-code\`、`Claude-NEW1\claude-code\`、MSIX 的 LocalCache 里那份都是这个形状。
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
    /// 回归测试：名字一漂，我们留的那份备份就永远不会被清掉 ——
    /// 备份名必须落进 `stale_copies()` 的清理范围。
    ///
    /// 那是一份没有 Deny ACL 的完整 claude.exe，也就是一条绕过门禁的路。
    #[test]
    fn backup_name_matches_stale_copies_prefix() {
        let name = crate::install::upgrade::backup_name("20260910-114500");
        assert!(name.starts_with("claude.exe.old."), "{name}");
        assert_eq!(
            kind_of(std::path::Path::new(&format!(
                r"C:\Users\me\.local\bin\{name}"
            ))),
            TargetKind::StaleCopy,
            "备份必须被认成 StaleCopy，否则清理和评分都看不见它"
        );
    }
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

    /// 反重力按目录认：两个产品下的主程序、语言服务器、第三方壳留下的 original 都是同一类。
    #[test]
    fn antigravity_is_recognised_by_its_two_install_directories() {
        for p in [
            r"C:\Users\me\AppData\Local\Programs\antigravity\Antigravity.exe",
            r"C:\Users\me\AppData\Local\Programs\antigravity\resources\bin\language_server.exe",
            r"C:\Users\me\AppData\Local\Programs\Antigravity IDE\Antigravity IDE.original.exe",
            r"C:\Users\me\AppData\Local\Programs\Antigravity IDE\resources\app\extensions\antigravity\bin\language_server_windows_x64.exe",
        ] {
            assert_eq!(kind_of(Path::new(p)), TargetKind::Antigravity, "{p}");
        }
        // 别的 IDE 的同名语言服务器不算。
        assert_ne!(
            kind_of(Path::new(
                r"C:\Users\me\AppData\Local\Programs\Windsurf\language_server.exe"
            )),
            TargetKind::Antigravity
        );
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

    #[test]
    fn classifies_the_copies_that_used_to_be_missed() {
        // v0.8.0 之前这几类根本不在清单里，自然也没有分类。
        assert_eq!(
            kind_of(Path::new(
                r"C:\Users\me\.local\share\claude\versions\2.1.267"
            )),
            TargetKind::NativeVersion
        );
        assert_eq!(
            kind_of(Path::new(
                r"C:\Users\me\.vscode\extensions\anthropic.claude-code-2.1.267-win32-x64\resources\native-binary\claude.exe"
            )),
            TargetKind::EditorExtension
        );
        assert_eq!(
            kind_of(Path::new(
                r"C:\Users\me\AppData\Roaming\Claude-NEW1\claude-code\2.1.258\claude.exe"
            )),
            TargetKind::CliVersioned
        );
    }

    #[test]
    fn inventory_kinds_map_onto_target_kinds() {
        use crate::install::inventory::Kind;
        assert_eq!(target_kind(Kind::Native), TargetKind::Cli);
        assert_eq!(target_kind(Kind::Winget), TargetKind::Cli);
        assert_eq!(target_kind(Kind::NativeVersion), TargetKind::NativeVersion);
        assert_eq!(target_kind(Kind::DesktopManaged), TargetKind::CliVersioned);
        assert_eq!(target_kind(Kind::Editor), TargetKind::EditorExtension);
        assert_eq!(target_kind(Kind::DesktopStub), TargetKind::DesktopStub);
    }
}
