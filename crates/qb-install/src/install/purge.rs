//! 完全卸载的**只读盘点**（0.19.0）。
//!
//! # 跟「清外部副本」是两件事
//!
//! [`crate::install::managed::externals`] 清的是**面板没装的多余副本**：托管那份、
//! 版本库、账户凭证、配置一概不碰 —— 它的前提是「留着托管那份继续用」。
//!
//! 完全卸载的前提相反：**这台机器上不再要这个软件了**。所以托管那份、版本库、
//! 配置、会话、认证、环境变量、Shell 里为它设的那几行、启动项、账户槽位，全都要清。
//! 使用者的原话：「卸载就是全部卸载，不留一丝一毫，没必要有选项。」
//!
//! # 为什么规划是纯函数
//!
//! 跟 `managed::plan_claude_externals` 同一个理由：真正容易出错的是**边界**
//! （该不该算进来、归属依据够不够），不是 I/O。事实由调用方先探好塞进 [`Facts`]，
//! 这里只做判断，于是每一条边界都钉得住一条单测，而单测不碰真实运行期状态。
//!
//! # 两条不许违反的
//!
//! 1. **认证与环境变量只报名字和位置，永远不带值。** [`Item`] 结构上就装不下值 ——
//!    跟 `sysenv::SecretHit`、`relay::ProviderView` 同一条原则：报出来的东西迟早
//!    会被打进日志、截图或者贴进 issue，所以不是靠「记得别打印」，是让它根本拿不到。
//! 2. **每一项都要带得出归属依据**（[`Item::why`]）。删之前界面要逐条显示
//!    「路径 / 类型 / 凭什么认定它属于这个软件」，不能只因为名字里有 claude 就删。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

use crate::install::inventory::{Install, Kind};

/// 要卸哪一个。
///
/// 比 `managed::App` 多的几个：Claude 桌面端（官方安装器装的，面板接管不了它的目录）、
/// Chrome（浏览器，只在使用者点名时才处理）、Codex 桌面端（Store 包，0.28.0）、
/// 反重力与 Gemini CLI（0.28.0 —— 软件页能装能检测却不能卸，是同一类不对称）。
// `Deserialize` 是必须的：它要当 IPC 命令的参数从界面传进来。
// 只有 `Serialize` 的话 `#[tauri::command]` 展开时就编译不过。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "PurgeTarget")]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    ClaudeCode,
    Codex,
    ClaudeDesktop,
    Chrome,
    /// Codex 桌面端（Microsoft Store 包）。跟 `Codex`（CLI）分开：两者共用账户槽位里的
    /// `home`（登录身份），卸掉一个不能顺手把另一个的身份也删了。
    CodexDesktop,
    /// 反重力 Hub + IDE 一起（同一个 Google 账户，只卸一个没有意义）。
    Antigravity,
    /// 官方 Gemini CLI（npm 包）与它的账户槽位。
    GeminiCli,
}

impl Target {
    pub fn label(self) -> &'static str {
        match self {
            Target::ClaudeCode => "Claude Code",
            Target::Codex => "Codex",
            Target::ClaudeDesktop => "Claude 桌面端",
            Target::Chrome => "Google Chrome",
            Target::CodexDesktop => "Codex 桌面端",
            Target::Antigravity => "反重力",
            Target::GeminiCli => "Gemini CLI",
        }
    }

    /// 删 exe 之前要核的签名主体关键词。
    ///
    /// 跟 `winget::InstallTarget::expected_signer` 是同一套口径：拿 Anthropic
    /// 去验 Codex 等于验了个寂寞。
    ///
    /// 反重力的 IDE 壳可能是第三方签的（本机就是汉化壳），所以它的规划里**只删整个目录**
    /// （`DeleteDir` 不逐个核 exe 签名），这个关键词只在真有 `DeleteFile` 时才用得上。
    pub fn signer(self) -> &'static str {
        match self {
            Target::ClaudeCode | Target::ClaudeDesktop => "anthropic",
            Target::Codex | Target::CodexDesktop => "openai",
            Target::Chrome | Target::Antigravity | Target::GeminiCli => "google",
        }
    }
}

/// 一项属于哪一类。界面按这个分组，确认框也按这个分段列。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "PurgeCategory")]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// 可执行文件与启动器。
    Program,
    /// 面板版本库里留给回滚的旧版本。**清外部副本永不碰它，完全卸载要清。**
    VersionStore,
    /// 下载缓存、npm 中断残留、文件早已不在的卸载登记。
    Cache,
    /// 配置、会话记录、待办、shell 快照、日志。
    Config,
    /// OAuth 凭证、凭据管理器条目。**只报名字。**
    Auth,
    /// 环境变量与 PATH 项。**只报名字。**
    EnvPath,
    /// Shell 配置文件里专门为它设的那几行。**只报文件与行号。**
    ShellLine,
    /// 计划任务、启动项、自动更新器、执行别名。
    Startup,
    /// 面板自己管的账户槽位。最贵的一类：删完所有账户都要重新登录。
    AccountSlot,
    /// 浏览器用户资料（书签、密码、扩展、全部站点登录态）。
    BrowserData,
}

/// 怎么处理这一项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "PurgeAction")]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// 删文件。**删之前逐个再核一次签名。**
    DeleteFile,
    /// 删整个目录。
    DeleteDir,
    NpmUninstall,
    WingetUninstall,
    ScoopUninstall,
    /// 删注册表里的卸载登记。
    RegistryDelete,
    /// 删用户级环境变量。
    EnvUnset,
    /// 把 PATH 里的某一项摘掉（只摘那一项，不动 PATH 里别的）。
    PathEntryRemove,
    /// 删 Shell 配置文件里的某几行。
    ShellLineRemove,
    /// 删凭据管理器条目。
    CredentialDelete,
    /// 删计划任务 / 启动项 / 执行别名。
    StartupRemove,
    /// 卸载一个 Store 包（`Remove-AppxPackage`，按包族名认）。
    AppxRemove,
}

/// 盘点出来的一项。
///
/// ⛔ **结构上装不下密钥、Token、Cookie 的内容。** 加字段之前先读文件头那两条。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, rename = "PurgeItem")]
pub struct Item {
    pub target: Target,
    pub category: Category,
    pub action: Action,
    /// 绝对路径，或者要卸载的包名 / 登记项名 / 变量名。
    pub subject: String,
    /// 归属依据：凭什么认定它属于这个软件。界面上逐条显示。
    pub why: String,
}

impl Item {
    fn new(
        target: Target,
        category: Category,
        action: Action,
        subject: impl Into<String>,
        why: impl Into<String>,
    ) -> Self {
        Item {
            target,
            category,
            action,
            subject: subject.into(),
            why: why.into(),
        }
    }
}

/// 规划要用到的事实。**全部由调用方先探好**，这里不碰磁盘、不跑进程。
///
/// 只带名字的那几项（[`Self::env_names`]、[`Self::credentials`]）是有意的：
/// 采集器读得到值，但不许把值带进来。
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// `inventory::scan` 的结果。**只有 Claude Code 的落点** ——
    /// inventory 那张表只收 claude.exe。
    pub installs: Vec<Install>,
    /// Codex 的候选路径（`detect::codex_candidates()` 里真实存在的那些）。
    ///
    /// 单独一个字段而不是塞进 [`Self::installs`]：那张表的每一项都带
    /// `Kind`，而 Codex 的落点没有对应的 `Kind`，硬塞就得编一个假的种类。
    pub codex_paths: Vec<PathBuf>,
    /// `inventory::stale_copies` 的结果（升级残留）。
    pub stale: Vec<PathBuf>,
    /// 面板托管根目录。
    pub managed_root: PathBuf,
    /// `%APPDATA%`，用来认出桌面端带的副本。
    pub roaming: Option<PathBuf>,
    /// 配置与会话目录（`~\.claude`、`~\.codex`、`%APPDATA%\Claude` 这些）。
    pub config_dirs: Vec<PathBuf>,
    /// 单个配置文件（`.claude.json`、`.claude.json.backup`）。
    pub config_files: Vec<PathBuf>,
    /// 凭证文件。
    pub credential_files: Vec<PathBuf>,
    /// 凭据管理器条目名。**只有名字。**
    pub credentials: Vec<String>,
    /// 用户级环境变量名。**只有名字。**
    pub env_names: Vec<String>,
    /// PATH 里只服务于这个软件的项。
    pub path_entries: Vec<String>,
    /// Shell 配置里为它设的行：(文件, 行号)。**不带行的内容。**
    pub shell_lines: Vec<(PathBuf, u32)>,
    /// 计划任务 / 启动项 / 执行别名的名字。
    pub startup: Vec<String>,
    /// 面板自己管的账户槽位目录。
    pub account_slots: Vec<PathBuf>,
    /// 浏览器用户资料目录。
    pub browser_data: Vec<PathBuf>,
    /// Codex 桌面端的 Store 包在不在（`Get-AppxPackage` 查到的）。
    pub codex_desktop_package: bool,
    /// 反重力两个产品的安装目录（此刻存在的）。整目录删，不逐个核 exe。
    pub antigravity_dirs: Vec<PathBuf>,
    /// Gemini CLI 是不是 npm 全局装的。
    pub gemini_npm: bool,
    /// 面板托管目录下那份 Gemini CLI（`<根>\gemini-cli`）。
    pub gemini_managed: Option<PathBuf>,
}

/// 大小写与斜杠都不敏感地判断 `p` 是不是在 `root` 里面。
fn inside(p: &Path, root: &Path) -> bool {
    crate::install::managed::is_inside(p, root)
}

/// 规划一次完全卸载。**纯函数，不碰磁盘。**
///
/// 跟清外部副本最大的不同：托管那份与版本库**要清**。它们是这台机器上
/// 最后一份可用的程序，所以清之前调用方必须已经拿到使用者的确认。
pub fn plan(target: Target, f: &Facts) -> Vec<Item> {
    let mut out = Vec::new();
    match target {
        Target::ClaudeCode => plan_claude_code(f, &mut out),
        Target::Codex => plan_codex(f, &mut out),
        Target::ClaudeDesktop => plan_desktop(f, &mut out),
        Target::Chrome => plan_chrome(f, &mut out),
        Target::CodexDesktop => plan_codex_desktop(f, &mut out),
        Target::Antigravity => plan_antigravity(f, &mut out),
        Target::GeminiCli => plan_gemini(f, &mut out),
    }
    plan_shared(target, f, &mut out);
    out
}

/// Codex 桌面端：Store 包走 `Remove-AppxPackage`（不是删文件 —— `WindowsApps` 下的东西
/// 只有部署服务能动）。它的数据（`Packages\OpenAI.Codex_…`、每个槽位的 `desktop\`）
/// 由调用方塞进 `config_dirs`，走共用那段。**账户槽位的 `home` 不在内**：CLI 也在用它。
fn plan_codex_desktop(f: &Facts, out: &mut Vec<Item>) {
    if f.codex_desktop_package {
        out.push(Item::new(
            Target::CodexDesktop,
            Category::Program,
            Action::AppxRemove,
            crate::install::codex_desktop::PACKAGE_FAMILY,
            "Microsoft Store 包（Get-AppxPackage 按包族名查到）",
        ));
    }
}

/// 反重力：两个安装目录整目录删。**不逐个 exe 核签名** —— IDE 的壳可能是第三方汉化工具签的，
/// 按「不是 Google 签的不删」会把它永远留在那里。归属依据是位置表（`install::antigravity`），
/// 不是名字。数据目录由调用方塞进 `config_dirs`。
fn plan_antigravity(f: &Facts, out: &mut Vec<Item>) {
    for d in &f.antigravity_dirs {
        out.push(Item::new(
            Target::Antigravity,
            Category::Program,
            Action::DeleteDir,
            d.display().to_string(),
            "反重力的安装目录（位置表 install::antigravity 里的那两处之一）",
        ));
    }
}

/// Gemini CLI：npm 全局包用 npm 卸；面板托管那份整目录删；槽位由调用方塞进 `account_slots`。
/// **不整份删 `~\.gemini`** —— 反重力的数据也在那底下。
fn plan_gemini(f: &Facts, out: &mut Vec<Item>) {
    let t = Target::GeminiCli;
    if f.gemini_npm {
        out.push(Item::new(
            t,
            Category::Program,
            Action::NpmUninstall,
            "@google/gemini-cli",
            "npm 全局包（连 gemini.cmd 一起）",
        ));
    }
    if let Some(d) = &f.gemini_managed {
        out.push(Item::new(
            t,
            Category::Program,
            Action::DeleteDir,
            d.display().to_string(),
            "面板托管目录下那份（npm 装进 <根>\\gemini-cli）",
        ));
    }
    for p in &f.account_slots {
        out.push(Item::new(
            t,
            Category::AccountSlot,
            Action::DeleteDir,
            p.display().to_string(),
            "面板管理的 Gemini 账户槽位（GEMINI_CLI_HOME，酒馆的 Gemini 桥接用的是同一份）",
        ));
    }
}

fn plan_claude_code(f: &Facts, out: &mut Vec<Item>) {
    let t = Target::ClaudeCode;
    let mut npm = false;
    let mut winget = false;
    let mut scoop = false;
    for i in &f.installs {
        match i.kind {
            // npm / winget / scoop 用它们自己的卸载命令，连启动器和账本一起走。
            Kind::Npm | Kind::NpmNative => npm = true,
            Kind::Winget => winget = true,
            Kind::Scoop => scoop = true,
            // 托管那份与版本库：清外部副本永不碰，完全卸载要清。
            Kind::Managed => out.push(Item::new(
                t,
                Category::Program,
                Action::DeleteFile,
                i.path.display().to_string(),
                "面板托管的那份",
            )),
            Kind::ManagedVersion => out.push(Item::new(
                t,
                Category::VersionStore,
                Action::DeleteFile,
                i.path.display().to_string(),
                "面板版本库里留给回滚的旧版本",
            )),
            Kind::Native | Kind::NativeVersion | Kind::Programs | Kind::Path => {
                out.push(Item::new(
                    t,
                    Category::Program,
                    Action::DeleteFile,
                    i.path.display().to_string(),
                    "官方安装器装的或它留下的",
                ))
            }
            // 桌面端带的副本属于桌面端，编辑器扩展属于编辑器，MSIX 属于系统包管理。
            // 卸 Claude Code 不动它们 —— 要卸桌面端就选桌面端那一项。
            Kind::DesktopManaged | Kind::MsixManaged | Kind::Editor | Kind::DesktopStub => {}
        }
    }
    for s in &f.stale {
        // 桌面端资料目录里的残留是桌面端自己的事。
        if !crate::install::inventory::is_desktop_managed_path(s, f.roaming.as_deref()) {
            out.push(Item::new(
                t,
                Category::Cache,
                Action::DeleteFile,
                s.display().to_string(),
                "升级残留",
            ));
        }
    }
    if npm {
        out.push(Item::new(
            t,
            Category::Program,
            Action::NpmUninstall,
            "@anthropic-ai/claude-code",
            "npm 全局包（连 claude.cmd 一起）",
        ));
    }
    if winget {
        out.push(Item::new(
            t,
            Category::Program,
            Action::WingetUninstall,
            "Anthropic.ClaudeCode",
            "winget 包（连 Links 的 shim 与登记一起）",
        ));
    }
    if scoop {
        out.push(Item::new(
            t,
            Category::Program,
            Action::ScoopUninstall,
            "claude-code",
            "scoop 包",
        ));
    }
    for p in &f.account_slots {
        out.push(Item::new(
            t,
            Category::AccountSlot,
            Action::DeleteDir,
            p.display().to_string(),
            "面板管理的账户槽位（酒馆桥接用的是同一份）",
        ));
    }
}

fn plan_codex(f: &Facts, out: &mut Vec<Item>) {
    let t = Target::Codex;
    let mut npm = false;
    let mut winget = false;
    for p in &f.codex_paths {
        let n = crate::install::inventory::norm(p);
        // Codex 桌面版自带的与 MSIX 属于别的程序。
        if n.contains("\\openai\\codex\\bin\\") || n.contains("\\windowsapps\\") {
            continue;
        }
        if n.ends_with("\\npm\\codex.cmd") || n.contains("\\node_modules\\@openai\\") {
            npm = true;
        } else if n.contains("\\winget\\") {
            winget = true;
        } else {
            let managed = inside(p, &f.managed_root);
            out.push(Item::new(
                t,
                Category::Program,
                Action::DeleteFile,
                p.display().to_string(),
                if managed {
                    "面板托管的那份"
                } else {
                    "本机其它位置的 Codex"
                },
            ));
        }
    }
    if npm {
        out.push(Item::new(
            t,
            Category::Program,
            Action::NpmUninstall,
            "@openai/codex",
            "npm 全局包（连 codex.cmd 一起）",
        ));
    }
    if winget {
        out.push(Item::new(
            t,
            Category::Program,
            Action::WingetUninstall,
            "OpenAI.Codex",
            "winget 包",
        ));
    }
}

fn plan_desktop(f: &Facts, out: &mut Vec<Item>) {
    let t = Target::ClaudeDesktop;
    // 桌面端的位置被官方安装器写死，面板接管不了 —— 走 winget 卸载，
    // 而不是逐个删文件。
    out.push(Item::new(
        t,
        Category::Program,
        Action::WingetUninstall,
        "Anthropic.Claude",
        "官方安装器装的桌面端",
    ));
    for i in &f.installs {
        if matches!(i.kind, Kind::DesktopStub | Kind::DesktopManaged) {
            out.push(Item::new(
                t,
                Category::Program,
                Action::DeleteFile,
                i.path.display().to_string(),
                "桌面端存根或它带的副本",
            ));
        }
    }
}

fn plan_chrome(f: &Facts, out: &mut Vec<Item>) {
    let t = Target::Chrome;
    out.push(Item::new(
        t,
        Category::Program,
        Action::WingetUninstall,
        "Google.Chrome",
        "winget 包",
    ));
    for p in &f.browser_data {
        out.push(Item::new(
            t,
            Category::BrowserData,
            Action::DeleteDir,
            p.display().to_string(),
            "浏览器用户资料：书签、密码、扩展、全部站点登录态",
        ));
    }
}

/// 配置、认证、环境变量、Shell 行、启动项 —— 四个目标共用同一套规则。
fn plan_shared(t: Target, f: &Facts, out: &mut Vec<Item>) {
    for p in &f.config_dirs {
        out.push(Item::new(
            t,
            Category::Config,
            Action::DeleteDir,
            p.display().to_string(),
            "配置、会话记录与缓存目录",
        ));
    }
    for p in &f.config_files {
        out.push(Item::new(
            t,
            Category::Config,
            Action::DeleteFile,
            p.display().to_string(),
            "配置文件",
        ));
    }
    for p in &f.credential_files {
        out.push(Item::new(
            t,
            Category::Auth,
            Action::DeleteFile,
            p.display().to_string(),
            "本地凭证文件（只报位置，不读内容）",
        ));
    }
    for name in &f.credentials {
        out.push(Item::new(
            t,
            Category::Auth,
            Action::CredentialDelete,
            name.clone(),
            "凭据管理器条目（只报名字，不读内容）",
        ));
    }
    for name in &f.env_names {
        out.push(Item::new(
            t,
            Category::EnvPath,
            Action::EnvUnset,
            name.clone(),
            "为这个软件设的用户级环境变量（只报名字，不读值）",
        ));
    }
    for entry in &f.path_entries {
        out.push(Item::new(
            t,
            Category::EnvPath,
            Action::PathEntryRemove,
            entry.clone(),
            "PATH 里只服务于这个软件的一项",
        ));
    }
    for (file, line) in &f.shell_lines {
        out.push(Item::new(
            t,
            Category::ShellLine,
            Action::ShellLineRemove,
            format!("{}:{line}", file.display()),
            "Shell 配置里为这个软件设变量的行（只报位置，不读内容）",
        ));
    }
    for name in &f.startup {
        out.push(Item::new(
            t,
            Category::Startup,
            Action::StartupRemove,
            name.clone(),
            "计划任务 / 启动项 / 执行别名",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install(kind: Kind, p: &str) -> Install {
        Install {
            kind,
            path: PathBuf::from(p),
            lockable: true,
            launchable: true,
            preferred: false,
        }
    }

    fn claude_facts() -> Facts {
        Facts {
            installs: vec![
                install(Kind::Managed, r"C:\M\claude-code\claude.exe"),
                install(
                    Kind::ManagedVersion,
                    r"C:\M\claude-code\versions\2.0.11\claude.exe",
                ),
                install(Kind::Native, r"C:\Users\me\.local\bin\claude.exe"),
                install(Kind::Npm, r"C:\Users\me\AppData\Roaming\npm\claude.cmd"),
                install(
                    Kind::DesktopManaged,
                    r"C:\Users\me\AppData\Roaming\Claude\claude-code\2.0.13\claude.exe",
                ),
                install(
                    Kind::Editor,
                    r"C:\Users\me\.vscode\extensions\anthropic.claude-code-1\claude.exe",
                ),
            ],
            managed_root: PathBuf::from(r"C:\M"),
            roaming: Some(PathBuf::from(r"C:\Users\me\AppData\Roaming")),
            account_slots: vec![PathBuf::from(
                r"C:\Users\me\AppData\Local\ClaudeIpGate\claude-profile-main",
            )],
            config_dirs: vec![PathBuf::from(r"C:\Users\me\.claude")],
            config_files: vec![PathBuf::from(r"C:\Users\me\.claude.json")],
            credential_files: vec![PathBuf::from(r"C:\Users\me\.claude\.credentials.json")],
            credentials: vec!["Anthropic Claude Code".into()],
            env_names: vec!["ANTHROPIC_AUTH_TOKEN".into()],
            path_entries: vec![r"C:\Users\me\.local\bin".into()],
            shell_lines: vec![(
                PathBuf::from(r"C:\Users\me\Documents\PowerShell\profile.ps1"),
                12,
            )],
            startup: vec!["App Execution Alias: claude.exe".into()],
            ..Facts::default()
        }
    }

    /// 完全卸载**要**清托管那份和版本库 —— 这正是它跟「清外部副本」的分界线。
    #[test]
    fn a_full_purge_takes_the_managed_copy_and_the_version_store() {
        let plan = plan(Target::ClaudeCode, &claude_facts());
        assert!(plan
            .iter()
            .any(|i| i.category == Category::Program
                && i.subject.ends_with(r"M\claude-code\claude.exe")));
        assert!(plan.iter().any(|i| i.category == Category::VersionStore));
    }

    /// 别的程序的副本一律不碰：桌面端带的、编辑器扩展里的。
    #[test]
    fn other_programs_copies_are_never_touched() {
        let plan = plan(Target::ClaudeCode, &claude_facts());
        assert!(
            !plan.iter().any(|i| i.subject.contains(".vscode")
                || i.subject.contains(r"Roaming\Claude\claude-code")),
            "{plan:?}"
        );
    }

    /// 账户槽位单独成一类 —— 它最贵，界面要能把它单独标出来。
    #[test]
    fn account_slots_are_their_own_category() {
        let plan = plan(Target::ClaudeCode, &claude_facts());
        let slots: Vec<_> = plan
            .iter()
            .filter(|i| i.category == Category::AccountSlot)
            .collect();
        assert_eq!(slots.len(), 1);
        assert!(slots[0].subject.contains("claude-profile-main"));
    }

    /// ⛔ 结构上装不下密钥值。这条断言是防未来的人「顺手」加一个 value 字段。
    #[test]
    fn items_carry_no_secret_values() {
        let plan = plan(Target::ClaudeCode, &claude_facts());
        let json = serde_json::to_string(&plan).unwrap();
        assert!(json.contains("ANTHROPIC_AUTH_TOKEN"), "名字要报出来");
        assert!(
            !json.contains("\"value\"") && !json.contains("\"secret\""),
            "不许带值：{json}"
        );
    }

    /// 0.28.0 的三个目标要用的事实（跟 Claude 那份合在一起，方便「每一项都有依据」那条测试）。
    fn new_facts() -> Facts {
        Facts {
            codex_desktop_package: true,
            antigravity_dirs: vec![
                PathBuf::from(r"C:\Users\me\AppData\Local\Programs\antigravity"),
                PathBuf::from(r"C:\Users\me\AppData\Local\Programs\Antigravity IDE"),
            ],
            gemini_npm: true,
            gemini_managed: Some(PathBuf::from(r"C:\M\gemini-cli")),
            ..claude_facts()
        }
    }

    /// Codex 桌面端是 Store 包：走 Remove-AppxPackage，按包族名认，**不**逐个删 WindowsApps 下的文件。
    #[test]
    fn the_codex_desktop_is_removed_as_a_store_package() {
        let plan = plan(Target::CodexDesktop, &new_facts());
        let appx: Vec<_> = plan
            .iter()
            .filter(|i| i.action == Action::AppxRemove)
            .collect();
        assert_eq!(appx.len(), 1);
        assert_eq!(appx[0].subject, "OpenAI.Codex_2p2nqsd0c76g0");
        assert!(
            !plan
                .iter()
                .any(|i| i.subject.to_lowercase().contains("windowsapps")),
            "{plan:?}"
        );
        // 没查到包就一项都不出 —— 不编一个「要卸的 Store 包」。
        let none = super::plan(
            Target::CodexDesktop,
            &Facts {
                codex_desktop_package: false,
                ..Facts::default()
            },
        );
        assert!(none.iter().all(|i| i.action != Action::AppxRemove));
    }

    /// 反重力整目录删（IDE 壳可能是第三方签的），两个目录都在清单里，一个 DeleteFile 都没有。
    #[test]
    fn antigravity_is_removed_by_directory_not_by_signed_exe() {
        let plan = plan(Target::Antigravity, &new_facts());
        let dirs: Vec<&str> = plan
            .iter()
            .filter(|i| i.action == Action::DeleteDir && i.category == Category::Program)
            .map(|i| i.subject.as_str())
            .collect();
        assert_eq!(dirs.len(), 2, "{plan:?}");
        assert!(dirs.iter().any(|d| d.ends_with("Antigravity IDE")));
        // 程序那一类里一个「删单个 exe」都没有 —— 那才会按签名主体逐个核。
        assert!(plan
            .iter()
            .filter(|i| i.category == Category::Program)
            .all(|i| i.action != Action::DeleteFile));
    }

    /// Gemini CLI：npm 卸 + 托管目录删 + 槽位删；`~\.gemini` 整份不许出现（反重力也在用它）。
    #[test]
    fn gemini_cli_takes_npm_managed_and_slots_but_never_the_shared_gemini_dir() {
        let plan = plan(Target::GeminiCli, &new_facts());
        assert!(plan
            .iter()
            .any(|i| i.action == Action::NpmUninstall && i.subject == "@google/gemini-cli"));
        assert!(plan
            .iter()
            .any(|i| i.action == Action::DeleteDir && i.subject.ends_with("gemini-cli")));
        assert!(plan.iter().any(|i| i.category == Category::AccountSlot));
        assert!(
            !plan
                .iter()
                .any(|i| i.subject.to_lowercase().ends_with(r"\.gemini")),
            "{plan:?}"
        );
    }

    /// 每一项都必须说得出归属依据 —— 界面要逐条显示「凭什么删它」。
    #[test]
    fn every_item_states_why_it_belongs_to_this_software() {
        for target in [
            Target::ClaudeCode,
            Target::Codex,
            Target::ClaudeDesktop,
            Target::Chrome,
            Target::CodexDesktop,
            Target::Antigravity,
            Target::GeminiCli,
        ] {
            for i in plan(target, &new_facts()) {
                assert!(!i.why.trim().is_empty(), "{i:?} 没有归属依据");
                assert!(!i.subject.trim().is_empty(), "{i:?} 没有对象");
            }
        }
    }

    /// 桌面端走 winget 卸载，不是逐个删文件 —— 它的位置被官方安装器写死。
    #[test]
    fn the_desktop_app_is_uninstalled_through_winget() {
        let plan = plan(Target::ClaudeDesktop, &claude_facts());
        assert!(plan
            .iter()
            .any(|i| i.action == Action::WingetUninstall && i.subject == "Anthropic.Claude"));
    }
}
