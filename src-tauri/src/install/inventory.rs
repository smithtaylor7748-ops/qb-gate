//! Claude 装在哪 —— 全项目**唯一**的一张位置表。
//!
//! # 为什么要有这个文件
//!
//! v0.7.x 之前，「Claude Code 装在哪」由四处各自维护，而且已经对不上：
//!
//! | 用途 | 原来查哪些位置 |
//! |---|---|
//! | 检测 `detect::claude_code` | 只查 `~\.local\bin` |
//! | 启动 `sillytavern::find_official_claude` | 记住的路径、`.local\bin`、桌面端带的副本、`Programs\Claude` |
//! | 上锁 `targets::lockable` | `.local\bin`、桌面端带的副本、桌面端存根、WinGet |
//! | 残留 `targets::stale_copies` | 两个目录 |
//!
//! 放到别人的机器上这会直接出事：只用 winget 装、没装桌面端的人，
//! 上锁表锁住了 WinGet 那份，启动表里却没有 WinGet —— 面板报
//! 「找不到官方 Claude Code 程序」，检测页报「未安装」，人被自己的工具关在门外。
//! 反过来，启动表里的 `Programs\Claude` 不在上锁表里：能启动、永远不上锁。
//!
//! 现在检测、启动、上锁、升级、残留清理都从这里拿。**加新位置只加在这里。**
//!
//! # 覆盖了哪些落点
//!
//! | 种类 | 位置 | 锁 | 启动 |
//! |---|---|:--:|:--:|
//! | `Native` | `~\.local\bin\claude.exe`（官方原生安装器） | ✅ | 1 |
//! | `NativeVersion` | `~\.local\share\claude\versions\<版本>` —— **是文件**，每个都是完整二进制 | ✅ | — |
//! | `Winget` | `%LOCALAPPDATA%` 与 `%ProgramFiles%` 下的 `WinGet\Packages\Anthropic.ClaudeCode*` 与 `WinGet\Links` | ✅ | 2 |
//! | `Scoop` | `~\scoop\apps\claude-code\<版本>\claude.exe` | ✅ | 3 |
//! | `Programs` | `%LOCALAPPDATA%\Programs\Claude\claude.exe`（旧脚本认的位置） | ✅ | 4 |
//! | `Path` | `PATH` 上找到的其它 `claude.exe`（旧脚本 `Get-Command claude.exe` 那一条） | ✅ | 5 |
//! | `DesktopManaged` | `%APPDATA%\Claude*\claude-code\<版本>\claude.exe` —— **每个桌面端资料目录都算** | ✅ | 6（只取当前那份） |
//! | `MsixManaged` | MSIX 版桌面端虚拟化目录里的同一种副本 | ✅ | — |
//! | `Npm` | `%APPDATA%\npm\claude.cmd` 与 `PATH` 上的 `claude.cmd` | ❌ | 7 |
//! | `NpmNative` | npm 包里自带的 `claude.exe` | ✅ | — |
//! | `Editor` | VS Code / Cursor / Windsurf 等扩展自带的 `claude.exe` | ✅ | — |
//! | `DesktopStub` | `%LOCALAPPDATA%\AnthropicClaude\claude.exe`（桌面端存根） | ✅ | — |
//!
//! 永远不收：`AnthropicClaude\app-*` 下那份（加 Deny 桌面端开新窗口就崩，档案 §1 第 2 条）、
//! 任何 Electron 应用目录里的 `claude.exe`（同一个道理）、`claude-code-cli\` 下的 vendored 副本。
//!
//! ⚠ `Npm` 不能锁：它是批处理，真正跑的是 `node.exe`。给 `.cmd` 加 Deny ExecuteFile
//! 挡不住 `cmd.exe` 把它当文本读进去执行。界面上要如实说「执行锁管不到这一份」。
//!
//! ⚠ `MsixManaged` 与 `Editor` 两类的具体目录布局**没有在实机上见过**，
//! 是按各自的安装机制推出来的。本机没有这两类，所以它们只会多扫几个不存在的目录。
//!
//! # 只读文件系统
//!
//! [`scan`] 只做 `read_dir` / `metadata`，不跑任何程序、不联网。根目录全部来自
//! [`Roots`]，单测用临时目录搭一棵假树喂进来 —— 档案 §1 第 8 条：
//! 单测不许碰真实的运行期状态。

use serde::Serialize;
use std::path::{Path, PathBuf};

/// 扫描的起点。平时用 [`Roots::current`]，单测自己搭。
#[derive(Debug, Clone, Default)]
pub struct Roots {
    pub home: Option<PathBuf>,
    /// `%APPDATA%`
    pub roaming: Option<PathBuf>,
    /// `%LOCALAPPDATA%`
    pub local: Option<PathBuf>,
    /// `%ProgramFiles%` 等（机器级 winget 装在这下面）。
    pub program_files: Vec<PathBuf>,
    /// `PATH` 里的目录。
    pub path_dirs: Vec<PathBuf>,
    /// Scoop 根目录：`$env:SCOOP`，没设就是 `~\scoop`。
    pub scoop: Option<PathBuf>,
    /// 面板托管安装的根目录（v0.9.0，`settings::managed_apps_dir`）。
    pub managed: Option<PathBuf>,
}

impl Roots {
    pub fn current() -> Self {
        fn env_dir(k: &str) -> Option<PathBuf> {
            std::env::var_os(k)
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
        }
        let mut program_files: Vec<PathBuf> = Vec::new();
        for k in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(p) = env_dir(k) {
                if !program_files.iter().any(|q| norm(q) == norm(&p)) {
                    program_files.push(p);
                }
            }
        }
        let home = dirs::home_dir();
        let scoop = env_dir("SCOOP").or_else(|| home.as_ref().map(|h| h.join("scoop")));
        let path_dirs = std::env::var_os("PATH")
            .map(|v| {
                std::env::split_paths(&v)
                    .filter(|p| !p.as_os_str().is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            home,
            roaming: dirs::config_dir(),
            local: dirs::data_local_dir(),
            program_files,
            path_dirs,
            scoop,
            managed: Some(crate::settings::managed_apps_dir()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// 面板自己装的那份：`<托管根目录>\claude-code\claude.exe`（v0.9.0）。启动优先级最高。
    Managed,
    Native,
    NativeVersion,
    Winget,
    Scoop,
    Programs,
    Path,
    DesktopManaged,
    MsixManaged,
    Npm,
    NpmNative,
    Editor,
    DesktopStub,
}

impl Kind {
    /// 能不能加执行锁。只有批处理启动器不能 —— 见文件头。
    pub fn lockable(self) -> bool {
        !matches!(self, Kind::Npm)
    }

    /// 面板会不会拿它来启动 Claude Code。
    ///
    /// `NativeVersion` 是安装器的版本库，`Editor` / `NpmNative` 是别的程序的内部件，
    /// 存根是桌面端 —— 这些都只锁不启动。
    pub fn launchable(self) -> bool {
        self.launch_rank().is_some()
    }

    /// 启动优先级，小的先用。`None` = 不拿来启动。
    fn launch_rank(self) -> Option<u8> {
        Some(match self {
            Kind::Managed => 0,
            Kind::Native => 1,
            Kind::Winget => 2,
            Kind::Scoop => 3,
            Kind::Programs => 4,
            Kind::Path => 5,
            Kind::DesktopManaged => 6,
            Kind::Npm => 7,
            _ => return None,
        })
    }

    /// 门禁清单上显示成哪一类。
    pub fn target_kind(self) -> crate::gate::targets::TargetKind {
        use crate::gate::targets::TargetKind as T;
        match self {
            Kind::Managed => T::Managed,
            Kind::NativeVersion => T::NativeVersion,
            Kind::DesktopManaged | Kind::MsixManaged => T::CliVersioned,
            Kind::Editor => T::EditorExtension,
            Kind::DesktopStub => T::DesktopStub,
            _ => T::Cli,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Install {
    pub kind: Kind,
    pub path: PathBuf,
    pub lockable: bool,
    pub launchable: bool,
    /// 这一份是不是「启动 Claude Code」此刻会用的那份。
    pub preferred: bool,
}

// ---------------------------------------------------------------- 路径判断

/// 统一成小写反斜杠，只用于比较。
pub(crate) fn norm(p: &Path) -> String {
    p.to_string_lossy().to_lowercase().replace('/', "\\")
}

/// 判断两条路径是不是同一个位置（大小写与斜杠不敏感，不跟联结点）。
pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    norm(a).trim_end_matches('\\') == norm(b).trim_end_matches('\\')
}

/// `AnthropicClaude\app-<版本>\` 下的桌面端运行时副本。
///
/// **给它加 Deny ACE，桌面端开新窗口就崩**（档案 §1 第 2 条）。这条判断是
/// 把它排除在上锁清单之外的那道闸，`gate::lock_all` 里还有一道同样的保险。
pub fn is_desktop_runtime_copy(p: &Path) -> bool {
    norm(p).contains("\\anthropicclaude\\app-")
}

/// 这个目录是不是一个 Electron 应用（桌面端就是）。
///
/// 它里面的 `claude.exe` 是桌面端的主程序，不是 CLI —— 当成 CLI 去锁，
/// 效果跟锁 `app-*` 一样：新窗口起不来。
fn is_electron_dir(dir: &Path) -> bool {
    dir.join("resources").join("app.asar").exists()
        || dir.join("chrome_100_percent.pak").exists()
        || dir.join("v8_context_snapshot.bin").exists()
}

/// 酒馆桥接项目自带的 vendored 副本，不是官方二进制，桥接明确拒绝它。
fn is_vendored(p: &Path) -> bool {
    norm(p).contains("\\claude-code-cli\\")
}

/// 这个 `claude.exe` 该不该进清单。三类永远不收，见文件头。
fn excluded(p: &Path) -> bool {
    is_desktop_runtime_copy(p)
        || is_vendored(p)
        || p.parent().is_some_and(is_electron_dir)
}

/// 桌面端**进程**的可执行文件路径吗。
///
/// 看门狗「查不到 IP 立即关闭桌面端」和切换账户前的「桌面端还开着吗」都靠它。
/// 两种落点：
///
/// * Squirrel：`%LOCALAPPDATA%\AnthropicClaude\` 下的一切（存根、`app-*`、`Update.exe`）；
/// * MSIX：`…\WindowsApps\<包全名>\…`，包全名以 `Claude` / `Anthropic` 开头、带下划线
///   （`名称_版本_架构__发布者`）。**必须带下划线** —— `%LOCALAPPDATA%\Microsoft\WindowsApps\claude.exe`
///   是应用执行别名，那一段就叫 `claude.exe`，不能误认成包目录。
///
/// **纯字符串判断，不拼进任何脚本。** 旧版把前缀拼进 PowerShell 的单引号串里
/// 再用 `-like` 比，用户名带 `'` 就是语法错误、带 `[` `]` 就被当成通配符，
/// 而错误全被丢掉 —— 日志写着「收摊」，桌面端其实还开着。
pub fn is_desktop_process_path(path: &Path, local: Option<&Path>) -> bool {
    let p = norm(path);
    if let Some(l) = local {
        let prefix = format!("{}\\", norm(&l.join("AnthropicClaude")).trim_end_matches('\\'));
        if p.starts_with(&prefix) {
            return true;
        }
    }
    const WA: &str = "\\windowsapps\\";
    if let Some(i) = p.find(WA) {
        let rest = &p[i + WA.len()..];
        if let Some((comp, tail)) = rest.split_once('\\') {
            return !tail.is_empty()
                && comp.contains('_')
                && (comp.starts_with("claude") || comp.starts_with("anthropic"));
        }
    }
    false
}

/// 桌面端带的 CLI 副本（`%APPDATA%\Claude*\claude-code\…`）吗。
///
/// 它们是桌面端 Code 页拉起的会话，归桌面端所有：一键关闭与切账户报「关掉了什么」时
/// 算桌面端的；清外部副本时也不碰它们。
pub fn is_desktop_managed_path(path: &Path, roaming: Option<&Path>) -> bool {
    let Some(r) = roaming else {
        return false;
    };
    let p = norm(path);
    let root = format!("{}\\", norm(r).trim_end_matches('\\'));
    let Some(rest) = p.strip_prefix(&root) else {
        return false;
    };
    let Some((profile, tail)) = rest.split_once('\\') else {
        return false;
    };
    (profile == "claude" || profile.starts_with("claude-")) && tail.starts_with("claude-code\\")
}

/// 版本号按数字比。`2.1.100` 必须排在 `2.1.99` 后面，`1.49585.0` 必须排在 `1.9.0` 后面 ——
/// 按字符串比这两条都是反的。
pub fn version_key(s: &str) -> Vec<u64> {
    s.split(|c: char| !c.is_ascii_digit())
        .filter(|x| !x.is_empty())
        .filter_map(|x| x.parse().ok())
        .collect()
}

/// `versions\` 下的文件名只收版本号形状的，免得把下载中的临时文件也算进来。
fn looks_like_version(name: &str) -> bool {
    name.contains('.') && name.chars().all(|c| c.is_ascii_digit() || c == '.')
}

// ---------------------------------------------------------------- 目录工具

fn entries(dir: &Path) -> Vec<std::fs::DirEntry> {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default()
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = entries(dir)
        .into_iter()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    v.sort();
    v
}

fn name_lower(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

#[cfg(windows)]
fn is_reparse_point(p: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

#[cfg(not(windows))]
fn is_reparse_point(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
}

/// 在 `dir` 下往深处找名叫 `name` 的文件。限深度，扩展目录里可能有上千个文件。
fn find_named(dir: &Path, name: &str, depth: u32) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in entries(dir) {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 && !is_reparse_point(&p) {
                out.extend(find_named(&p, name, depth - 1));
            }
        } else if name_lower(&p) == name {
            out.push(p);
        }
    }
    out
}

/// `<parent>\<版本>\claude.exe` —— 桌面端带的副本就是这个形状。
fn versioned_exes(parent: &Path) -> Vec<PathBuf> {
    subdirs(parent)
        .into_iter()
        .map(|d| d.join("claude.exe"))
        .filter(|p| p.is_file())
        .collect()
}

/// 编辑器扩展目录。都在 `~\<这个>\extensions\` 下。
const EDITOR_DIRS: [&str; 7] = [
    ".vscode",
    ".vscode-insiders",
    ".vscode-oss",
    ".cursor",
    ".windsurf",
    ".trae",
    ".kiro",
];

// ---------------------------------------------------------------- 扫描

/// 把这台机器上所有 Claude Code 可执行副本找出来。**只读文件系统。**
///
/// 同一个文件经不同路径（`%APPDATA%\Claude` 联结点与它指向的 `Claude-<标签>`）
/// 只算一次。
pub fn scan(r: &Roots) -> Vec<Install> {
    let mut found: Vec<(Kind, PathBuf)> = Vec::new();

    // 面板托管的那份放最前面：去重时先扫到的留下，它得保住 `Managed` 这个身份。
    if let Some(m) = &r.managed {
        let exe = m.join("claude-code").join("claude.exe");
        if exe.is_file() {
            found.push((Kind::Managed, exe));
        }
    }

    if let Some(h) = &r.home {
        let bin = h.join(".local").join("bin").join("claude.exe");
        if bin.is_file() {
            found.push((Kind::Native, bin));
        }

        let vdir = h.join(".local").join("share").join("claude").join("versions");
        let mut versions: Vec<PathBuf> = entries(&vdir)
            .into_iter()
            .map(|e| e.path())
            .filter(|p| p.is_file() && looks_like_version(&name_lower(p)))
            .collect();
        versions.sort_by_key(|p| std::cmp::Reverse(version_key(&name_lower(p))));
        found.extend(versions.into_iter().map(|p| (Kind::NativeVersion, p)));

        // 官方安装脚本的下载缓存 `~\.claude\downloads\claude-<版本>-win32-<架构>.exe`。
        // 脚本正常跑完会删掉它，被打断就留下一份完整的二进制（本机实测一份 218 MB，
        // 2.1.263）—— 没锁的完整副本，§1 第 1 条。只收这个文件名形状，目录里别的东西不碰。
        for e in entries(&h.join(".claude").join("downloads")) {
            let p = e.path();
            let n = name_lower(&p);
            if p.is_file() && n.starts_with("claude-") && n.ends_with(".exe") {
                found.push((Kind::NativeVersion, p));
            }
        }

        for ed in EDITOR_DIRS {
            for ext in subdirs(&h.join(ed).join("extensions")) {
                if name_lower(&ext).starts_with("anthropic.claude-code") {
                    found.extend(find_named(&ext, "claude.exe", 6).into_iter().map(|p| (Kind::Editor, p)));
                }
            }
        }
    }

    // winget：用户级在 %LOCALAPPDATA%，机器级在 %ProgramFiles%。两处的形状一样。
    let mut winget_roots: Vec<PathBuf> = Vec::new();
    if let Some(l) = &r.local {
        winget_roots.push(l.join("Microsoft").join("WinGet"));
    }
    winget_roots.extend(r.program_files.iter().map(|pf| pf.join("WinGet")));
    for w in winget_roots {
        for pkg in subdirs(&w.join("Packages")) {
            let n = name_lower(&pkg);
            if n.contains("claudecode") || n.contains("claude.code") {
                let exe = pkg.join("claude.exe");
                if exe.is_file() {
                    found.push((Kind::Winget, exe));
                }
            }
        }
        let link = w.join("Links").join("claude.exe");
        if link.is_file() {
            found.push((Kind::Winget, link));
        }
    }

    if let Some(s) = &r.scoop {
        let app = s.join("apps").join("claude-code");
        // `current` 是指向某个版本目录的联结点。启动优先用它，锁的时候
        // 它跟真目录是同一个文件，去重会收掉一份。
        let current = app.join("current").join("claude.exe");
        if current.is_file() {
            found.push((Kind::Scoop, current));
        }
        found.extend(versioned_exes(&app).into_iter().map(|p| (Kind::Scoop, p)));
    }

    if let Some(l) = &r.local {
        let programs = l.join("Programs").join("Claude").join("claude.exe");
        if programs.is_file() {
            found.push((Kind::Programs, programs));
        }
    }

    for d in &r.path_dirs {
        // 应用执行别名目录：那里的 exe 是重解析点，锁不上，也不是真身。
        if norm(d).contains("\\windowsapps") {
            continue;
        }
        let exe = d.join("claude.exe");
        if exe.is_file() {
            found.push((Kind::Path, exe));
        }
        let cmd = d.join("claude.cmd");
        if cmd.is_file() {
            found.push((Kind::Npm, cmd));
        }
    }

    if let Some(ro) = &r.roaming {
        // 每一个桌面端资料目录都要扫，不能只扫 `%APPDATA%\Claude` 指向的那个 ——
        // 否则切到 NEW1 之后，main 那边的副本就从上锁清单里掉出去了。
        for prof in subdirs(ro) {
            let n = name_lower(&prof);
            if n == "claude" || n.starts_with("claude-") {
                found.extend(
                    versioned_exes(&prof.join("claude-code"))
                        .into_iter()
                        .map(|p| (Kind::DesktopManaged, p)),
                );
            }
        }

        let npm = ro.join("npm");
        let cmd = npm.join("claude.cmd");
        if cmd.is_file() {
            found.push((Kind::Npm, cmd));
        }
        found.extend(
            find_named(&npm.join("node_modules").join("@anthropic-ai"), "claude.exe", 6)
                .into_iter()
                .map(|p| (Kind::NpmNative, p)),
        );
    }

    if let Some(l) = &r.local {
        // MSIX 版桌面端把 %APPDATA% 的写入重定向到自己包目录下的 LocalCache。
        for pkg in subdirs(&l.join("Packages")) {
            let n = name_lower(&pkg);
            if n.starts_with("claude") || n.starts_with("anthropic") {
                for prof in subdirs(&pkg.join("LocalCache").join("Roaming")) {
                    let pn = name_lower(&prof);
                    if pn == "claude" || pn.starts_with("claude-") {
                        found.extend(
                            versioned_exes(&prof.join("claude-code"))
                                .into_iter()
                                .map(|p| (Kind::MsixManaged, p)),
                        );
                    }
                }
            }
        }

        let stub = l.join("AnthropicClaude").join("claude.exe");
        if stub.is_file() {
            found.push((Kind::DesktopStub, stub));
        }
    }

    // 排除 + 去重。去重按真实文件（穿过联结点）比，先扫到的留下。
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<Install> = Vec::new();
    for (kind, path) in found {
        if kind != Kind::DesktopStub && excluded(&path) {
            continue;
        }
        let key = std::fs::canonicalize(&path)
            .map(|c| norm(&c))
            .unwrap_or_else(|_| norm(&path));
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(Install {
            kind,
            lockable: kind.lockable(),
            launchable: kind.launchable(),
            preferred: false,
            path,
        });
    }

    if let Some(i) = preferred_index(r, &out) {
        out[i].preferred = true;
    }
    out
}

/// 启动 Claude Code 该用哪一份。
///
/// 按 [`Kind::launch_rank`] 挑；桌面端带的副本只从**当前**那个资料目录
/// （`%APPDATA%\Claude` 指向的）里挑，而且挑版本号最大的 —— 桌面端会自己清掉旧版本。
fn preferred_index(r: &Roots, all: &[Install]) -> Option<usize> {
    let active_desktop = r
        .roaming
        .as_ref()
        .map(|ro| ro.join("Claude").join("claude-code"))
        .and_then(|p| std::fs::canonicalize(p).ok())
        .map(|p| norm(&p));

    let is_active_desktop = |i: &Install| {
        let Some(root) = &active_desktop else {
            return false;
        };
        std::fs::canonicalize(&i.path).is_ok_and(|c| norm(&c).starts_with(root.as_str()))
    };

    // 同一档里挑版本号最大的；Scoop 的 `current` 永远算最新 —— 它就是 Scoop 认定的当前版本。
    let version_of = |i: &Install| -> Vec<u64> {
        let dir = i.path.parent().map(name_lower).unwrap_or_default();
        if dir == "current" {
            vec![u64::MAX]
        } else {
            version_key(&dir)
        }
    };

    all.iter()
        .enumerate()
        .filter(|(_, i)| i.launchable)
        .filter(|(_, i)| i.kind != Kind::DesktopManaged || is_active_desktop(i))
        .min_by(|(_, a), (_, b)| {
            let ra = a.kind.launch_rank().unwrap_or(u8::MAX);
            let rb = b.kind.launch_rank().unwrap_or(u8::MAX);
            ra.cmp(&rb).then(version_of(b).cmp(&version_of(a)))
        })
        .map(|(i, _)| i)
}

/// 「启动 Claude Code」会用的那一份。
pub fn preferred_cli(r: &Roots) -> Option<Install> {
    scan(r).into_iter().find(|i| i.preferred)
}

/// 同上，但只要 exe（给酒馆桥接的 `--claude` 用 —— 桥接要的是可执行文件，
/// 不是批处理）。
pub fn preferred_exe(r: &Roots) -> Option<PathBuf> {
    let all = scan(r);
    if let Some(p) = all.iter().find(|i| i.preferred && i.kind != Kind::Npm) {
        return Some(p.path.clone());
    }
    let mut exes: Vec<&Install> = all
        .iter()
        .filter(|i| i.launchable && i.kind != Kind::Npm)
        .collect();
    exes.sort_by_key(|i| i.kind.launch_rank());
    exes.first().map(|i| i.path.clone())
}

/// 要加执行锁的全部路径。**不含** `app-*`（`scan` 已经排除过，这里再滤一次当保险）。
pub fn lockable_paths(r: &Roots) -> Vec<(PathBuf, Kind)> {
    let mut v: Vec<(PathBuf, Kind)> = scan(r)
        .into_iter()
        .filter(|i| i.lockable && !is_desktop_runtime_copy(&i.path))
        .map(|i| (i.path, i.kind))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

/// 官方原生安装器的落点。升级走的就是这一份，**不管它现在在不在**。
pub fn native_bin(r: &Roots) -> Option<PathBuf> {
    r.home
        .as_ref()
        .map(|h| h.join(".local").join("bin").join("claude.exe"))
}

/// 升级残留：`claude.exe.old.<时间戳>`。
///
/// 这些副本没有 Deny ACL，是能绕过门禁的执行副本。扫每一份副本所在的目录，
/// 外加原生安装器的 bin 目录与每个桌面端资料目录下的 `claude-code\` 本身。
pub fn stale_copies(r: &Roots) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut add = |d: PathBuf| {
        if !dirs.iter().any(|x| same_path(x, &d)) {
            dirs.push(d);
        }
    };
    for i in scan(r) {
        if i.kind == Kind::NativeVersion || i.kind == Kind::Npm {
            continue;
        }
        if let Some(parent) = i.path.parent() {
            add(parent.to_path_buf());
            if matches!(i.kind, Kind::DesktopManaged | Kind::MsixManaged) {
                if let Some(cc) = parent.parent() {
                    add(cc.to_path_buf());
                }
            }
        }
    }
    if let Some(bin) = native_bin(r).and_then(|b| b.parent().map(Path::to_path_buf)) {
        add(bin);
    }

    let mut out: Vec<PathBuf> = Vec::new();
    for d in dirs {
        for e in entries(&d) {
            let p = e.path();
            if p.is_file() && name_lower(&p).starts_with("claude.exe.old.") && !out.iter().any(|x| same_path(x, &p)) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// npm 装的那份是什么版本。读包里的 `package.json`，**不运行它**。
pub fn npm_version(r: &Roots) -> Option<String> {
    let pkg = r
        .roaming
        .as_ref()?
        .join("npm")
        .join("node_modules")
        .join("@anthropic-ai")
        .join("claude-code")
        .join("package.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(pkg).ok()?).ok()?;
    v.get("version")?.as_str().map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一棵临时的假机器。结束时整棵删掉。
    struct Tree {
        base: PathBuf,
    }

    impl Tree {
        fn new(tag: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "qbgate-inventory-{tag}-{}-{}",
                std::process::id(),
                chrono::Local::now().format("%H%M%S%f")
            ));
            let _ = std::fs::remove_dir_all(&base);
            std::fs::create_dir_all(&base).unwrap();
            Self { base }
        }

        fn roots(&self) -> Roots {
            Roots {
                home: Some(self.base.join("home")),
                roaming: Some(self.base.join("roaming")),
                local: Some(self.base.join("local")),
                program_files: vec![self.base.join("pf")],
                path_dirs: Vec::new(),
                scoop: Some(self.base.join("home").join("scoop")),
                managed: Some(self.base.join("managed")),
            }
        }

        fn file(&self, rel: &str) -> PathBuf {
            let p = self.base.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"MZ").unwrap();
            p
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    fn kinds(v: &[Install]) -> Vec<Kind> {
        v.iter().map(|i| i.kind).collect()
    }

    #[test]
    fn a_fresh_machine_has_nothing_and_says_so() {
        let t = Tree::new("fresh");
        let r = t.roots();
        assert!(scan(&r).is_empty());
        assert!(preferred_cli(&r).is_none());
        assert!(lockable_paths(&r).is_empty());
        assert!(stale_copies(&r).is_empty());
    }

    #[test]
    fn a_winget_only_machine_launches_the_copy_it_locks() {
        // 回归：原来上锁表有 WinGet、启动表没有 —— 只用 winget 装的人被锁在门外。
        let t = Tree::new("winget");
        let exe = t.file(r"local\Microsoft\WinGet\Packages\Anthropic.ClaudeCode_Microsoft.Winget.Source_8wekyb3d8bbwe\claude.exe");
        let r = t.roots();

        let p = preferred_cli(&r).expect("winget 那份必须能启动");
        assert_eq!(p.kind, Kind::Winget);
        assert!(same_path(&p.path, &exe));
        assert!(lockable_paths(&r).iter().any(|(q, _)| same_path(q, &exe)));
        assert_eq!(preferred_exe(&r).as_deref().map(|q| same_path(q, &exe)), Some(true));
    }

    #[test]
    fn machine_wide_winget_is_found_too() {
        let t = Tree::new("winget-machine");
        let exe = t.file(r"pf\WinGet\Packages\Anthropic.ClaudeCode_x\claude.exe");
        let r = t.roots();
        assert_eq!(preferred_cli(&r).map(|i| i.kind), Some(Kind::Winget));
        assert!(lockable_paths(&r).iter().any(|(q, _)| same_path(q, &exe)));
    }

    #[test]
    fn the_native_install_wins_when_there_are_several() {
        let t = Tree::new("native");
        let bin = t.file(r"home\.local\bin\claude.exe");
        t.file(r"local\Microsoft\WinGet\Packages\Anthropic.ClaudeCode_x\claude.exe");
        t.file(r"roaming\Claude\claude-code\2.1.260\claude.exe");
        let r = t.roots();
        let p = preferred_cli(&r).unwrap();
        assert_eq!(p.kind, Kind::Native);
        assert!(same_path(&p.path, &bin));
        // 三份全都要锁，不能只锁要启动的那份。
        assert_eq!(lockable_paths(&r).len(), 3);
    }

    #[test]
    fn the_managed_copy_wins_over_everything_and_is_locked() {
        // v0.9.0：面板自己装的那份，启动优先级最高 —— 面板再也不用猜用哪一份。
        let t = Tree::new("managed");
        let managed = t.file(r"managed\claude-code\claude.exe");
        t.file(r"home\.local\bin\claude.exe");
        t.file(r"local\Microsoft\WinGet\Packages\Anthropic.ClaudeCode_x\claude.exe");
        let r = t.roots();
        let p = preferred_cli(&r).unwrap();
        assert_eq!(p.kind, Kind::Managed);
        assert!(same_path(&p.path, &managed));
        assert_eq!(preferred_exe(&r).as_deref().map(|q| same_path(q, &managed)), Some(true));
        assert!(lockable_paths(&r).iter().any(|(q, k)| same_path(q, &managed) && *k == Kind::Managed));
        assert_eq!(lockable_paths(&r).len(), 3, "另外两份照样锁");
    }

    #[test]
    fn the_installer_download_cache_is_locked() {
        // 官方安装脚本被打断时留下的完整二进制。本机实测就有一份（2.1.263，218 MB），原来没锁。
        let t = Tree::new("dlcache");
        let cache = t.file(r"home\.claude\downloads\claude-2.1.263-win32-x64.exe");
        t.file(r"home\.claude\downloads\notes.txt");
        t.file(r"home\.claude\.credentials.json");
        let r = t.roots();
        let all = scan(&r);
        assert_eq!(kinds(&all), vec![Kind::NativeVersion]);
        assert!(same_path(&all[0].path, &cache));
        assert!(preferred_cli(&r).is_none(), "缓存只锁不启动");
    }

    #[test]
    fn native_versions_are_locked_but_never_launched() {
        // `versions\<版本>` 是文件，每个都是完整二进制（档案 §7.23）。
        // 本机实测三份都没有 Deny —— 违反 §1 第 1 条。
        let t = Tree::new("versions");
        t.file(r"home\.local\share\claude\versions\2.1.258");
        t.file(r"home\.local\share\claude\versions\2.1.267");
        t.file(r"home\.local\share\claude\versions\2.1.268.download");
        let r = t.roots();
        let all = scan(&r);
        assert_eq!(kinds(&all), vec![Kind::NativeVersion, Kind::NativeVersion], "临时文件不能算");
        assert!(all.iter().all(|i| i.lockable && !i.launchable));
        assert!(preferred_cli(&r).is_none(), "版本库不拿来启动");
    }

    #[test]
    fn desktop_runtime_copies_never_appear() {
        let t = Tree::new("runtime");
        t.file(r"local\AnthropicClaude\app-1.49585.0\claude.exe");
        let stub = t.file(r"local\AnthropicClaude\claude.exe");
        let r = t.roots();
        let all = scan(&r);
        assert_eq!(kinds(&all), vec![Kind::DesktopStub]);
        assert!(same_path(&all[0].path, &stub));
        assert!(preferred_cli(&r).is_none(), "存根是桌面端，不能当 Claude Code 起");
    }

    #[test]
    fn every_desktop_profile_is_locked_but_only_the_active_one_launches() {
        // 切到 NEW1 之后，main 资料目录下的副本不能从上锁清单里掉出去。
        let t = Tree::new("profiles");
        t.file(r"roaming\Claude\claude-code\2.1.99\claude.exe");
        let newest = t.file(r"roaming\Claude\claude-code\2.1.100\claude.exe");
        t.file(r"roaming\Claude-NEW1\claude-code\2.1.300\claude.exe");
        let r = t.roots();

        assert_eq!(lockable_paths(&r).len(), 3);
        let p = preferred_cli(&r).unwrap();
        assert_eq!(p.kind, Kind::DesktopManaged);
        // 按数字比：2.1.100 > 2.1.99；而且不去挑别的资料目录里更大的 2.1.300。
        assert!(same_path(&p.path, &newest), "挑错了：{}", p.path.display());
    }

    #[cfg(windows)]
    #[test]
    fn a_profile_reached_through_a_junction_is_counted_once() {
        let t = Tree::new("junction");
        t.file(r"roaming\Claude-main\claude-code\2.1.260\claude.exe");
        let link = t.base.join(r"roaming\Claude");
        let target = t.base.join(r"roaming\Claude-main");
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "建联结点失败");
        let r = t.roots();
        assert_eq!(lockable_paths(&r).len(), 1, "同一个文件经联结点只能算一次");
        assert_eq!(preferred_cli(&r).map(|i| i.kind), Some(Kind::DesktopManaged));
    }

    #[test]
    fn npm_launches_but_cannot_be_locked() {
        let t = Tree::new("npm");
        let cmd = t.file(r"roaming\npm\claude.cmd");
        let inner = t.file(r"roaming\npm\node_modules\@anthropic-ai\claude-code-win32-x64\claude.exe");
        let r = t.roots();
        let all = scan(&r);
        let npm = all.iter().find(|i| i.kind == Kind::Npm).expect("npm 那份要检测到");
        assert!(same_path(&npm.path, &cmd));
        assert!(!npm.lockable, "批处理挡不住，不能假装锁上了");
        assert!(npm.preferred);
        assert!(lockable_paths(&r).iter().any(|(q, k)| same_path(q, &inner) && *k == Kind::NpmNative));
        assert!(!lockable_paths(&r).iter().any(|(q, _)| same_path(q, &cmd)));
        // 桥接要 exe，不能拿批处理糊弄过去。
        assert_eq!(preferred_exe(&r), None);
    }

    #[test]
    fn editor_extension_binaries_are_locked() {
        let t = Tree::new("editor");
        let exe = t.file(r"home\.vscode\extensions\anthropic.claude-code-2.1.267-win32-x64\resources\native-binary\claude.exe");
        let cursor = t.file(r"home\.cursor\extensions\anthropic.claude-code-2.1.260\bin\claude.exe");
        t.file(r"home\.vscode\extensions\someone.else-1.0.0\claude.exe");
        let r = t.roots();
        let locks = lockable_paths(&r);
        assert_eq!(locks.len(), 2, "别人的扩展不能收");
        assert!(locks.iter().any(|(q, k)| same_path(q, &exe) && *k == Kind::Editor));
        assert!(locks.iter().any(|(q, _)| same_path(q, &cursor)));
        assert!(preferred_cli(&r).is_none(), "扩展的内部件不拿来启动");
    }

    #[test]
    fn electron_apps_are_never_treated_as_the_cli() {
        // 桌面端主程序也叫 claude.exe。当成 CLI 去锁，新窗口就起不来（§1 第 2 条）。
        let t = Tree::new("electron");
        t.file(r"local\Programs\Claude\claude.exe");
        t.file(r"local\Programs\Claude\resources\app.asar");
        let r = t.roots();
        assert!(scan(&r).is_empty());
    }

    #[test]
    fn path_hits_are_used_and_execution_aliases_are_skipped() {
        let t = Tree::new("path");
        let custom = t.file(r"tools\claude\claude.exe");
        t.file(r"local\Microsoft\WindowsApps\claude.exe");
        let mut r = t.roots();
        r.path_dirs = vec![
            t.base.join(r"tools\claude"),
            t.base.join(r"local\Microsoft\WindowsApps"),
        ];
        let all = scan(&r);
        assert_eq!(kinds(&all), vec![Kind::Path]);
        assert!(same_path(&all[0].path, &custom));
    }

    #[test]
    fn a_path_entry_pointing_at_a_known_copy_is_not_double_counted() {
        let t = Tree::new("path-dup");
        t.file(r"home\.local\bin\claude.exe");
        let mut r = t.roots();
        r.path_dirs = vec![t.base.join(r"home\.local\bin")];
        assert_eq!(kinds(&scan(&r)), vec![Kind::Native]);
    }

    #[test]
    fn vendored_copies_are_excluded() {
        let t = Tree::new("vendored");
        t.file(r"proj\claude-code-cli\claude.exe");
        let mut r = t.roots();
        r.path_dirs = vec![t.base.join(r"proj\claude-code-cli")];
        assert!(scan(&r).is_empty());
    }

    #[test]
    fn scoop_prefers_current() {
        let t = Tree::new("scoop");
        let current = t.file(r"home\scoop\apps\claude-code\current\claude.exe");
        t.file(r"home\scoop\apps\claude-code\2.1.267\claude.exe");
        let r = t.roots();
        let p = preferred_cli(&r).unwrap();
        assert_eq!(p.kind, Kind::Scoop);
        assert!(same_path(&p.path, &current));
    }

    #[test]
    fn stale_copies_are_found_next_to_every_copy() {
        let t = Tree::new("stale");
        t.file(r"home\.local\bin\claude.exe");
        let a = t.file(r"home\.local\bin\claude.exe.old.1757000000");
        t.file(r"roaming\Claude-NEW1\claude-code\2.1.258\claude.exe");
        let b = t.file(r"roaming\Claude-NEW1\claude-code\2.1.258\claude.exe.old.123");
        t.file(r"local\Microsoft\WinGet\Packages\Anthropic.ClaudeCode_x\claude.exe");
        let c = t.file(r"local\Microsoft\WinGet\Packages\Anthropic.ClaudeCode_x\claude.exe.old.9");
        let r = t.roots();
        let s = stale_copies(&r);
        assert_eq!(s.len(), 3, "{s:?}");
        for want in [a, b, c] {
            assert!(s.iter().any(|x| same_path(x, &want)), "漏了 {}", want.display());
        }
    }

    #[test]
    fn version_keys_sort_numerically() {
        assert!(version_key("2.1.100") > version_key("2.1.99"));
        assert!(version_key("app-1.49585.0") > version_key("app-1.9.0"));
        assert_eq!(version_key("2.1.267"), vec![2, 1, 267]);
        assert!(version_key("").is_empty());
    }

    #[test]
    fn desktop_process_paths_are_recognised_without_string_splicing() {
        let local = Path::new(r"C:\Users\O'Brien [x]\AppData\Local");
        // 用户名带 ' 和 [ ]：旧版拼进 PowerShell 就坏在这里。
        assert!(is_desktop_process_path(
            Path::new(r"C:\Users\O'Brien [x]\AppData\Local\AnthropicClaude\app-1.2.3\claude.exe"),
            Some(local)
        ));
        assert!(is_desktop_process_path(
            Path::new(r"c:/users/o'brien [x]/appdata/local/anthropicclaude/Update.exe"),
            Some(local)
        ));
        // MSIX 包目录。
        assert!(is_desktop_process_path(
            Path::new(r"C:\Program Files\WindowsApps\Claude_1.0.0.0_x64__abcdefg\app\claude.exe"),
            Some(local)
        ));
        // 执行别名不是包目录。
        assert!(!is_desktop_process_path(
            Path::new(r"C:\Users\O'Brien [x]\AppData\Local\Microsoft\WindowsApps\claude.exe"),
            Some(local)
        ));
        // Claude Code 的副本一个都不能认成桌面端 —— 认错了看门狗会连它一起杀。
        for cli in [
            r"C:\Users\O'Brien [x]\.local\bin\claude.exe",
            r"C:\Users\O'Brien [x]\AppData\Roaming\Claude\claude-code\2.1.260\claude.exe",
            r"C:\Users\O'Brien [x]\AppData\Local\AnthropicClaudeX\claude.exe",
        ] {
            assert!(!is_desktop_process_path(Path::new(cli), Some(local)), "{cli}");
        }
    }

    #[test]
    fn desktop_managed_paths_cover_every_profile() {
        let ro = Path::new(r"C:\Users\me\AppData\Roaming");
        assert!(is_desktop_managed_path(
            Path::new(r"C:\Users\me\AppData\Roaming\Claude\claude-code\2.1.260\claude.exe"),
            Some(ro)
        ));
        assert!(is_desktop_managed_path(
            Path::new(r"C:\Users\me\AppData\Roaming\Claude-NEW1\claude-code\2.1.258\claude.exe"),
            Some(ro)
        ));
        assert!(!is_desktop_managed_path(
            Path::new(r"C:\Users\me\AppData\Roaming\Claude Code\x\claude.exe"),
            Some(ro)
        ));
        assert!(!is_desktop_managed_path(Path::new(r"C:\Users\me\.local\bin\claude.exe"), Some(ro)));
    }

    #[test]
    fn npm_version_is_read_from_package_json_without_running_anything() {
        let t = Tree::new("npmver");
        let p = t.base.join(r"roaming\npm\node_modules\@anthropic-ai\claude-code\package.json");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, r#"{"name":"@anthropic-ai/claude-code","version":"2.1.267"}"#).unwrap();
        assert_eq!(npm_version(&t.roots()).as_deref(), Some("2.1.267"));
    }
}
