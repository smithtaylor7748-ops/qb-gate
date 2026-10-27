// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 其余六个命令行工具 —— OpenCode、Grok Build、OpenClaw、Hermes、Pi、MiniMax Code —— 装在哪、是什么版本、
//! 升级和卸载该交给谁（2026-10-27，中转站重构阶段 7 · X3，照 cc-switch v4.0.14 `commands/misc.rs` 的工具管理）。
//!
//! Claude Code、Codex、Gemini CLI 不在这里：它们走软件页原来那几张卡（托管安装、门禁上锁、清外部副本），
//! 位置表是 [`super::inventory`] / [`super::detect`]。这六个不归门禁管，也不进托管目录。
//!
//! # 跟 cc-switch 不一样的地方
//!
//! | | cc-switch | 这里 |
//! |---|---|---|
//! | 认版本 | 对找到的每一份跑 `--version` | **只读文件**：从 npm 那类启动壳（`.cmd`）顺着找到包目录，读它的 `package.json`；读不出就说读不出。打开页面不起任何第三方程序（跟 `inventory` 的「只读文件系统」一条） |
//! | 命令行默认用哪份 | 起 `where.exe` | 按合并后的 PATH 顺序自己找（`.exe` 先于 `.cmd`，跟 Windows 一样） |
//! | 官方安装脚本 | `powershell -EncodedCommand "irm … \| iex"` | **先下载到本机给人看、核哈希，确认之后才跑**（`usecase::cli_tools_ops`）；不用 base64（`CLAUDE.md`「杀软会把这个面板当木马」第 1 条） |
//! | 升级失败的兜底 | `<工具> update \|\| <官方脚本>` | 不自动接脚本：脚本只走上一行那条「给人看过」的路 |
//! | WSL | 支持 | **不搬**（门禁锁不到 WSL 里的东西，阶段 7 的计划写明） |
//! | 卸载 | 67 KB 的 `tool_uninstall.rs`，每个工具各一套 | 只做包管理器装的（npm / pnpm / Volta）；别的给出要做什么、由使用者自己来 |
//!
//! # 升级锚定到「命令行实际用的那一份」
//!
//! cc-switch 吃过的亏：裸 `npm i -g` 装到 PATH 上第一个 npm 那里，而命令行用的是别处那份，于是「升级成功、版本不变」。
//! 所以升级先认出默认那一份是谁装的（[`Source`]），再交给同一个包管理器（[`update_plan`]）；认不出就不替人升。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ------------------------------------------------------------------ 六个工具

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum CliTool {
    #[serde(rename = "opencode")]
    OpenCode,
    GrokBuild,
    #[serde(rename = "openclaw")]
    OpenClaw,
    Hermes,
    Pi,
    /// 不照 kebab-case 拆成 `mini-max-code`：这个工具自己写作 MiniMax（跟 `relay::coexist` 同一个名字）。
    #[serde(rename = "minimax-code")]
    MiniMaxCode,
}

/// 官方的 Windows 安装脚本。只有这三个有（OpenCode 的官方脚本只有 bash 版，Windows 上走 npm）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallScript {
    pub url: &'static str,
    /// 指定装到哪的环境变量（MiniMax Code 的 `MCODE_INSTALL_DIR`）。升级「脚本装的那一份」时带上它，写回同一处。
    pub dir_env: Option<&'static str>,
}

/// 「最新版本」去哪问。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatestFrom {
    Npm(&'static str),
    /// 先 npm，问不到再看 GitHub 最新发布（OpenCode）。
    NpmThenGithub(&'static str, &'static str),
    /// 先 GitHub 最新发布，问不到再 PyPI（Hermes：PyPI 上的 `hermes-agent` 0.19.0 之后不更新了）。
    GithubThenPypi(&'static str, &'static str),
}

impl CliTool {
    pub const ALL: [CliTool; 6] = [
        CliTool::OpenCode,
        CliTool::GrokBuild,
        CliTool::OpenClaw,
        CliTool::Hermes,
        CliTool::Pi,
        CliTool::MiniMaxCode,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CliTool::OpenCode => "OpenCode",
            CliTool::GrokBuild => "Grok Build",
            CliTool::OpenClaw => "OpenClaw",
            CliTool::Hermes => "Hermes",
            CliTool::Pi => "Pi",
            CliTool::MiniMaxCode => "MiniMax Code",
        }
    }

    /// 在终端里敲的那个名字。
    pub fn command(self) -> &'static str {
        match self {
            CliTool::OpenCode => "opencode",
            CliTool::GrokBuild => "grok",
            CliTool::OpenClaw => "openclaw",
            CliTool::Hermes => "hermes",
            CliTool::Pi => "pi",
            CliTool::MiniMaxCode => "mcode",
        }
    }

    /// npm 上的包。Hermes 是 Python 的，没有。
    pub fn npm_package(self) -> Option<&'static str> {
        match self {
            CliTool::OpenCode => Some("opencode-ai"),
            CliTool::GrokBuild => Some("@xai-official/grok"),
            CliTool::OpenClaw => Some("openclaw"),
            CliTool::Hermes => None,
            CliTool::Pi => Some("@earendil-works/pi-coding-agent"),
            CliTool::MiniMaxCode => Some("@minimax-ai/code"),
        }
    }

    /// `npm i -g` 要多带的参数。MiniMax Code 依赖 better-sqlite3 的安装脚本：npm 12 默认拦依赖的 install 脚本，
    /// 只放行 `--allow-scripts` 列出的包，被拦之后 SQLite 用不了；`--ignore-scripts=false` / `--include=optional`
    /// 抵消使用者 npmrc 里的相反设置（cc-switch 的 `npm_install_extra_args`，npm 10/11 上没有副作用）。
    pub fn npm_extra_args(self) -> &'static [&'static str] {
        match self {
            CliTool::MiniMaxCode => &[
                "--ignore-scripts=false",
                "--include=optional",
                "--allow-scripts=@minimax-ai/code,better-sqlite3",
            ],
            _ => &[],
        }
    }

    pub fn script(self) -> Option<InstallScript> {
        match self {
            CliTool::GrokBuild => Some(InstallScript {
                url: "https://x.ai/cli/install.ps1",
                dir_env: None,
            }),
            CliTool::MiniMaxCode => Some(InstallScript {
                url: "https://filecdn.minimax.chat/public/install.ps1",
                dir_env: Some("MCODE_INSTALL_DIR"),
            }),
            CliTool::Hermes => Some(InstallScript {
                url: "https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.ps1",
                dir_env: None,
            }),
            _ => None,
        }
    }

    pub fn latest_from(self) -> LatestFrom {
        match self {
            CliTool::OpenCode => LatestFrom::NpmThenGithub("opencode-ai", "anomalyco/opencode"),
            CliTool::Hermes => {
                LatestFrom::GithubThenPypi("NousResearch/hermes-agent", "hermes-agent")
            }
            other => LatestFrom::Npm(other.npm_package().unwrap_or_default()),
        }
    }
}

// ------------------------------------------------------------------ 去哪找

/// 扫描的起点。平时用 [`Roots::current`]，单测自己搭一棵临时目录树。
#[derive(Debug, Clone, Default)]
pub struct Roots {
    pub home: Option<PathBuf>,
    /// `%APPDATA%`
    pub roaming: Option<PathBuf>,
    /// `%LOCALAPPDATA%`
    pub local: Option<PathBuf>,
    /// `%ProgramFiles%` 等。
    pub program_files: Vec<PathBuf>,
    /// `%ProgramData%`
    pub program_data: Option<PathBuf>,
    /// 合并后的 PATH：面板进程自己的 + 注册表里当前用户与整机的（`%VAR%` 已展开）。
    ///
    /// 只看进程自己的不够：一键更新之后被安装包重新拉起的面板，进程 PATH 里可能没有用户那一半
    /// （cc-switch #6061 —— 用户 PATH 上的 npm 目录、各家安装器的目录全成了「没装」）。
    pub path_dirs: Vec<PathBuf>,
    /// 几个包管理器 / 安装器认的环境变量（`PNPM_HOME`、`VOLTA_HOME`、`GROK_BIN_DIR`……）。
    pub env: Vec<(String, PathBuf)>,
}

/// [`Roots::env`] 里收哪几个。
const ENV_DIRS: &[&str] = &[
    "PNPM_HOME",
    "VOLTA_HOME",
    "NVM_HOME",
    "NVM_SYMLINK",
    "SCOOP",
    "SCOOP_GLOBAL",
    "OPENCODE_INSTALL_DIR",
    "XDG_BIN_DIR",
    "GROK_BIN_DIR",
    "MCODE_INSTALL_DIR",
];

impl Roots {
    pub fn current() -> Self {
        let env_dir = |k: &str| {
            std::env::var_os(k)
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
        };
        let mut program_files: Vec<PathBuf> = Vec::new();
        for k in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(p) = env_dir(k) {
                if !program_files.iter().any(|q| same_dir(q, &p)) {
                    program_files.push(p);
                }
            }
        }
        Self {
            home: dirs::home_dir(),
            roaming: dirs::config_dir(),
            local: dirs::data_local_dir(),
            program_files,
            program_data: env_dir("ProgramData"),
            path_dirs: effective_path_dirs(),
            env: ENV_DIRS
                .iter()
                .filter_map(|k| env_dir(k).map(|v| (k.to_string(), v)))
                .collect(),
        }
    }

    fn env(&self, key: &str) -> Option<&Path> {
        self.env
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_path())
    }
}

/// 进程 PATH + 注册表里当前用户与整机的 `Path`（`%VAR%` 展开），保序去重。
fn effective_path_dirs() -> Vec<PathBuf> {
    use qb_platform::registry::{read, Hive};
    const MACHINE: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
    let reg = |hive: Hive, key: &str, name: &str| {
        read(hive, key, name)
            .ok()
            .flatten()
            .and_then(|v| v.as_text().map(str::to_string))
    };
    let lookup = |name: &str| {
        std::env::var(name)
            .ok()
            .or_else(|| reg(Hive::CurrentUser, "Environment", name))
            .or_else(|| reg(Hive::LocalMachine, MACHINE, name))
    };
    let process = std::env::var("PATH").unwrap_or_default();
    let user = reg(Hive::CurrentUser, "Environment", "Path").unwrap_or_default();
    let machine = reg(Hive::LocalMachine, MACHINE, "Path").unwrap_or_default();
    merge_path_lists(&[
        process,
        expand_vars(&user, &lookup),
        expand_vars(&machine, &lookup),
    ])
}

/// 展开 `%VAR%`。查不到、或者套娃太深（有人把变量写成互相引用）就原样留着。
pub fn expand_vars(raw: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    fn go(raw: &str, lookup: &dyn Fn(&str) -> Option<String>, depth: u32) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut rest = raw;
        while let Some(open) = rest.find('%') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let Some(close) = after.find('%') else {
                out.push('%');
                out.push_str(after);
                return out;
            };
            let name = &after[..close];
            let ident =
                !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            match ident.then(|| lookup(name)).flatten() {
                Some(v) if depth < 8 => out.push_str(&go(&v, lookup, depth + 1)),
                _ => {
                    out.push('%');
                    out.push_str(name);
                    out.push('%');
                }
            }
            rest = &after[close + 1..];
        }
        out.push_str(rest);
        out
    }
    go(raw, lookup, 0)
}

/// 几段 `;` 分隔的 PATH 按顺序合起来，同一个目录（不分大小写、不管结尾的 `\`）只留第一次出现的。
pub fn merge_path_lists(parts: &[String]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for part in parts {
        for seg in part.split(';') {
            let s = seg.trim().trim_matches('"');
            if s.is_empty() {
                continue;
            }
            let p = PathBuf::from(s);
            if !out.iter().any(|q| same_dir(q, &p)) {
                out.push(p);
            }
        }
    }
    out
}

/// 比较用：小写、`/` 换成 `\`、去掉结尾的 `\`。
pub fn norm(p: &Path) -> String {
    p.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn same_dir(a: &Path, b: &Path) -> bool {
    norm(a) == norm(b)
}

/// 应用执行别名目录：那里的 exe 是去商店的重解析点，不是命令行工具。
fn is_app_alias_dir(p: &Path) -> bool {
    norm(p).ends_with("\\microsoft\\windowsapps")
}

fn children(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    v.sort();
    v
}

/// 这个工具去哪些目录找，顺序 = 优先级（照 cc-switch `build_tool_search_paths` 的 Windows 那一支）：
/// 官方安装器的落点在前（它们只改用户 PATH，进程手里的旧 PATH 看不见），npm / 各家 Node 管理器在后，最后是合并后的 PATH。
pub fn search_dirs(tool: CliTool, r: &Roots) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if !p.as_os_str().is_empty()
            && !is_app_alias_dir(&p)
            && !out.iter().any(|q| same_dir(q, &p))
        {
            out.push(p);
        }
    };
    let home = r.home.clone();

    match tool {
        CliTool::GrokBuild => {
            if let Some(d) = r.env("GROK_BIN_DIR") {
                push(d.to_path_buf());
            }
            if let Some(h) = &home {
                push(h.join(".grok").join("bin"));
            }
        }
        // Windows 上 `mcode.cmd` 直接放在安装根目录下（不在 `bin`）。
        CliTool::MiniMaxCode => {
            if let Some(d) = r.env("MCODE_INSTALL_DIR") {
                push(d.to_path_buf());
            }
            if let Some(h) = &home {
                push(h.join(".minimax-code"));
            }
        }
        CliTool::OpenCode => {
            for k in ["OPENCODE_INSTALL_DIR", "XDG_BIN_DIR"] {
                if let Some(d) = r.env(k) {
                    push(d.to_path_buf());
                }
            }
            if let Some(h) = &home {
                push(h.join(".opencode").join("bin"));
            }
        }
        _ => {}
    }

    if let Some(h) = &home {
        push(h.join(".local").join("bin"));
        push(h.join(".volta").join("bin"));
        push(h.join(".bun").join("bin"));
        push(h.join("scoop").join("shims"));
    }
    if let Some(ro) = &r.roaming {
        push(ro.join("npm"));
        if tool == CliTool::Hermes {
            for v in children(&ro.join("Python")) {
                push(v.join("Scripts"));
            }
        }
    }
    if let Some(l) = &r.local {
        if tool == CliTool::Hermes {
            for v in children(&l.join("Programs").join("Python")) {
                push(v.join("Scripts"));
            }
        }
        push(l.join("pnpm"));
        push(l.join("Volta").join("bin"));
        push(l.join("Yarn").join("bin"));
        push(l.join("mise").join("shims"));
    }
    for pf in &r.program_files {
        push(pf.join("nodejs"));
    }
    if let Some(d) = r.env("PNPM_HOME") {
        push(d.to_path_buf());
    }
    if let Some(d) = r.env("VOLTA_HOME") {
        push(d.join("bin"));
    }
    if let Some(d) = r.env("NVM_SYMLINK") {
        push(d.to_path_buf());
    }
    for k in ["SCOOP", "SCOOP_GLOBAL"] {
        if let Some(d) = r.env(k) {
            push(d.join("shims"));
        }
    }
    // nvm-windows：每个 Node 版本一个目录，全局包的启动壳就在那一层。
    let mut nvm_roots: Vec<PathBuf> = r
        .env("NVM_HOME")
        .map(Path::to_path_buf)
        .into_iter()
        .collect();
    if let Some(ro) = &r.roaming {
        nvm_roots.push(ro.join("nvm"));
    }
    for root in nvm_roots {
        for v in children(&root) {
            push(v);
        }
    }
    if let Some(pd) = &r.program_data {
        push(pd.join("scoop").join("shims"));
    }
    for d in &r.path_dirs {
        push(d.clone());
    }
    out
}

/// 一个目录里这个工具的入口：`.exe` 先于 `.cmd`（跟 Windows 按 PATHEXT 找的顺序一样）；两个都在只算一份。
/// 没扩展名的那种是给 POSIX shell 的脚本，Windows 上跑不起来，不收。
pub fn entry_in(tool: CliTool, dir: &Path) -> Option<PathBuf> {
    ["exe", "cmd"]
        .iter()
        .map(|ext| dir.join(format!("{}.{ext}", tool.command())))
        .find(|p| p.is_file())
}

// ------------------------------------------------------------------ 谁装的

/// 这一份是谁装的。决定升级 / 卸载交给谁。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "CliToolSource")]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// npm 全局（默认的 `%APPDATA%\npm`，或者 Node 自己目录当前缀的那种）。
    Npm,
    /// nvm-windows 某个 Node 版本目录里的 npm 全局包。
    Nvm,
    Pnpm,
    Volta,
    Yarn,
    Bun,
    Scoop,
    Mise,
    /// 工具自己的官方安装器（`~\.grok\bin`、`~\.minimax-code`、`~\.opencode\bin`）。
    Installer,
    /// Python 的 `Scripts` 目录（Hermes）。
    Python,
    Unknown,
}

/// 按路径认是谁装的。纯字符串，不碰文件（npm 那一档另由 [`scan`] 按「启动壳指着 node_modules」补认）。
pub fn source_of(tool: CliTool, entry: &Path) -> Source {
    let s = norm(entry);
    let has = |frag: &str| s.contains(frag);
    let installer = match tool {
        CliTool::GrokBuild => has("\\.grok\\bin\\"),
        CliTool::MiniMaxCode => has("\\.minimax-code\\"),
        CliTool::OpenCode => has("\\.opencode\\bin\\"),
        _ => false,
    };
    if installer {
        Source::Installer
    } else if has("\\pnpm\\") {
        Source::Pnpm
    } else if has("\\volta\\") || has("\\.volta\\") {
        Source::Volta
    } else if has("\\yarn\\") {
        Source::Yarn
    } else if has("\\.bun\\") {
        Source::Bun
    } else if has("\\scoop\\") {
        Source::Scoop
    } else if has("\\mise\\") {
        Source::Mise
    } else if has("\\nvm\\") {
        Source::Nvm
    } else if has("\\appdata\\roaming\\npm\\") || has("\\nodejs\\") {
        Source::Npm
    } else if has("\\scripts\\") && (has("\\python") || has("\\pip")) {
        Source::Python
    } else {
        Source::Unknown
    }
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Npm => "npm",
            Source::Nvm => "nvm-windows",
            Source::Pnpm => "pnpm",
            Source::Volta => "Volta",
            Source::Yarn => "Yarn",
            Source::Bun => "Bun",
            Source::Scoop => "Scoop",
            Source::Mise => "mise",
            Source::Installer => "官方安装器",
            Source::Python => "Python",
            Source::Unknown => "来源不明",
        }
    }
}

/// 从 npm / pnpm / Yarn 生成的 `.cmd` 启动壳里，找出它指着的包目录（`…\node_modules\<包>`）。**纯函数。**
///
/// 启动壳长这样（npm 的 cmd-shim）：`"%_prog%"  "%dp0%\node_modules\opencode-ai\bin\opencode" %*`。
/// `%dp0%` / `%~dp0` 是启动壳自己所在的目录。也认绝对路径。认不出就是 `None` —— 版本就说读不出，不猜。
pub fn package_dir_from_shim(text: &str, shim_dir: &Path, package: &str) -> Option<PathBuf> {
    let needle = format!("node_modules\\{}", package.replace('/', "\\")).to_ascii_lowercase();
    let flat = text.replace('/', "\\");
    let lower = flat.to_ascii_lowercase();
    let mut from = 0;
    while let Some(off) = lower[from..].find(&needle) {
        let at = from + off;
        from = at + needle.len();
        // 包名后面必须是分隔符或者引号 —— `node_modules\pi` 不能认成 `node_modules\pi-extra`。
        let next = flat[at + needle.len()..].chars().next();
        if !matches!(next, Some('\\') | Some('"') | Some(' ') | None) {
            continue;
        }
        // 往前找这一段路径的开头：引号、空白或者等号。
        let start = flat[..at]
            .rfind(['"', ' ', '\t', '=', '\n', '\r'])
            .map(|i| i + 1)
            .unwrap_or(0);
        let prefix = &flat[start..at];
        let prefix_lower = prefix.to_ascii_lowercase();
        let base = if let Some(rest) = prefix_lower
            .strip_prefix("%dp0%")
            .or_else(|| prefix_lower.strip_prefix("%~dp0"))
        {
            let rest = &prefix[prefix.len() - rest.len()..];
            shim_dir.join(rest.trim_start_matches('\\'))
        } else if prefix.len() >= 3 && prefix.as_bytes()[1] == b':' {
            PathBuf::from(prefix)
        } else {
            continue;
        };
        let dir = base.join("node_modules").join(package.replace('/', "\\"));
        return Some(normalize_dots(&dir));
    }
    None
}

/// 把路径里的 `.` / `..` 化掉（不碰文件系统）：Yarn 的启动壳写的是 `%~dp0\..\Data\global\…`。
fn normalize_dots(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// 包目录里 `package.json` 的版本。**不运行它。**
pub fn package_version(dir: &Path) -> Option<String> {
    let v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("package.json")).ok()?).ok()?;
    v.get("version")?.as_str().map(str::to_string)
}

/// 本机的一份。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, rename = "CliInstall")]
pub struct Install {
    /// 入口（命令行里敲名字时跑的那个文件）。
    pub path: PathBuf,
    pub source: Source,
    /// 从包目录的 `package.json` 读的；读不出（不是 npm 那类装法）是 `None`，**不运行它去问**。
    pub version: Option<String>,
    /// npm 那类装法的包目录。
    pub package_dir: Option<PathBuf>,
    /// 命令行里敲 `opencode` 时跑的就是这一份（合并后的 PATH 上第一个）。
    pub default: bool,
}

/// 合并后的 PATH 上，命令行敲这个名字会跑哪一份。
pub fn path_default(tool: CliTool, r: &Roots) -> Option<PathBuf> {
    r.path_dirs
        .iter()
        .filter(|d| !is_app_alias_dir(d))
        .find_map(|d| entry_in(tool, d))
}

/// 本机装了几份、各是谁装的、什么版本。**只读文件系统**，不运行任何程序、不联网。
///
/// 同一个真实文件（联结点、符号链接）只算一份；命令行默认的那一份排第一。
pub fn scan(tool: CliTool, r: &Roots) -> Vec<Install> {
    let default = path_default(tool, r).map(|p| real_key(&p));
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<Install> = Vec::new();
    for dir in search_dirs(tool, r) {
        let Some(entry) = entry_in(tool, &dir) else {
            continue;
        };
        let key = real_key(&entry);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key.clone());
        let package_dir = tool.npm_package().and_then(|pkg| {
            let ext = entry.extension().and_then(|e| e.to_str()).unwrap_or("");
            let from_shim = ext
                .eq_ignore_ascii_case("cmd")
                .then(|| std::fs::read_to_string(&entry).ok())
                .flatten()
                .and_then(|t| package_dir_from_shim(&t, &dir, pkg));
            from_shim
                .or_else(|| Some(dir.join("node_modules").join(pkg.replace('/', "\\"))))
                .filter(|d| d.join("package.json").is_file())
        });
        let mut source = source_of(tool, &entry);
        // 启动壳指着某个 node_modules、路径又认不出是哪家的：按 npm 算（自定义了全局前缀的 npm）。
        if source == Source::Unknown && package_dir.is_some() {
            source = Source::Npm;
        }
        out.push(Install {
            version: package_dir.as_deref().and_then(package_version),
            package_dir,
            source,
            default: default.as_deref() == Some(key.as_str()),
            path: entry,
        });
    }
    out.sort_by_key(|i| !i.default);
    out
}

/// 去重用的键：能解析就用真实路径（穿过联结点），不能就用原样。
fn real_key(p: &Path) -> String {
    std::fs::canonicalize(p)
        .map(|c| norm(&c))
        .unwrap_or_else(|_| norm(p))
}

/// 装了好几份而且版本对不上（或者有的读不出版本）—— 升级只会动其中一份，界面要说清楚。
pub fn is_conflict(installs: &[Install]) -> bool {
    if installs.len() < 2 {
        return false;
    }
    let first = &installs[0].version;
    installs
        .iter()
        .any(|i| &i.version != first || i.version.is_none())
}

// ------------------------------------------------------------------ 交给谁做

/// 一步要做的事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// 起这个程序（可以是 `.cmd`：标准库会替它套 `cmd.exe`、按批处理的规矩转义参数）。
    Run {
        program: PathBuf,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
    /// 要跑官方安装脚本：先下载给人看、核哈希，确认之后才跑（不在这里跑）。
    Script {
        script: InstallScript,
        env: Vec<(String, String)>,
    },
    /// 面板不替你做。里面是一句给人看的话（要在终端里跑什么、或者去哪改）。
    Manual(String),
}

impl Step {
    /// 给界面看的那一行命令。
    pub fn shown(&self) -> String {
        match self {
            Step::Run { program, args, env } => {
                let mut s = String::new();
                for (k, v) in env {
                    s.push_str(&format!("$env:{k} = '{v}'; "));
                }
                let prog = program.display().to_string();
                if prog.contains(' ') {
                    s.push_str(&format!("& \"{prog}\""));
                } else {
                    s.push_str(&prog);
                }
                for a in args {
                    s.push(' ');
                    s.push_str(a);
                }
                s
            }
            Step::Script { script, env } => {
                let dir = env
                    .iter()
                    .map(|(k, v)| format!("（{k} = {v}）"))
                    .collect::<String>();
                format!("下载并运行官方安装脚本 {}{dir}", script.url)
            }
            Step::Manual(m) => m.clone(),
        }
    }
}

/// 同一个目录里的某个程序（`npm.cmd`、`pnpm.exe`……），按给的扩展名顺序找第一个在的。
fn sibling(entry: &Path, name: &str, exts: &[&str]) -> Option<PathBuf> {
    let dir = entry.parent()?;
    exts.iter()
        .map(|ext| dir.join(format!("{name}.{ext}")))
        .find(|p| p.is_file())
}

/// PATH 上的 `npm.cmd`（没有就看 `%ProgramFiles%\nodejs`）。新装、以及默认 `%APPDATA%\npm` 前缀那一份的升级用它。
pub fn npm_on_path(r: &Roots) -> Option<PathBuf> {
    let nodejs: Vec<PathBuf> = r.program_files.iter().map(|p| p.join("nodejs")).collect();
    r.path_dirs
        .iter()
        .chain(nodejs.iter())
        .map(|d| d.join("npm.cmd"))
        .find(|p| p.is_file())
}

fn npm_args(verb: &[&str], tool: CliTool, pkg: &str, latest: bool) -> Vec<String> {
    let mut v: Vec<String> = verb.iter().map(|s| s.to_string()).collect();
    v.push(if latest {
        format!("{pkg}@latest")
    } else {
        pkg.to_string()
    });
    if latest {
        v.extend(tool.npm_extra_args().iter().map(|s| s.to_string()));
    }
    v
}

/// 新装：用 npm。没有 npm 就是 `Manual`（先装 Node.js）；Hermes 没有 npm 包，只能走官方脚本。
pub fn npm_install_plan(tool: CliTool, r: &Roots) -> Step {
    let Some(pkg) = tool.npm_package() else {
        return Step::Manual(format!("{} 没有 npm 包，用官方安装脚本装。", tool.label()));
    };
    match npm_on_path(r) {
        Some(npm) => Step::Run {
            program: npm,
            args: npm_args(&["i", "-g"], tool, pkg, true),
            env: Vec::new(),
        },
        None => Step::Manual(format!(
            "没找到 npm（Node.js 没装，或者不在 PATH 上）。{} 是 npm 包：先装 Node.js LTS（https://nodejs.org/），再回来点这里。",
            tool.label()
        )),
    }
}

/// 命令行默认用的那一份。没有默认标记但只有一份，就是那一份；好几份又分不清，就是 `None`（不替人猜）。
pub fn default_install(installs: &[Install]) -> Option<&Install> {
    installs.iter().find(|i| i.default).or(match installs {
        [only] => Some(only),
        _ => None,
    })
}

/// 升级：交给装它的那个包管理器 / 它自己的升级命令，写回同一处（照 cc-switch 的 `anchored_command_from_paths` Windows 版）。
pub fn update_plan(tool: CliTool, installs: &[Install], r: &Roots) -> Step {
    let Some(inst) = default_install(installs) else {
        return Step::Manual(if installs.is_empty() {
            format!("{} 还没装。", tool.label())
        } else {
            format!(
                "装了 {} 份，分不清命令行用的是哪一份（都不在 PATH 上）。先把不用的那几份删掉，或者把要用的那份放进 PATH。",
                installs.len()
            )
        });
    };
    let pkg = tool.npm_package();
    match (inst.source, pkg) {
        (Source::Npm | Source::Nvm, Some(pkg)) => {
            // Node 自己目录当前缀（nvm-windows、`%ProgramFiles%\nodejs`）：同一层就有 npm.cmd，锚定到它。
            // 默认的 `%APPDATA%\npm` 前缀里没有 npm.cmd，用 PATH 上那个（它的全局前缀就是这里）。
            let npm = sibling(&inst.path, "npm", &["cmd", "exe"]).or_else(|| npm_on_path(r));
            match npm {
                Some(npm) => Step::Run {
                    program: npm,
                    args: npm_args(&["i", "-g"], tool, pkg, true),
                    env: Vec::new(),
                },
                None => Step::Manual(format!(
                    "这一份是 npm 装的，但没找到 npm。在装它的那个 Node.js 的终端里跑：npm i -g {pkg}@latest"
                )),
            }
        }
        (Source::Pnpm, Some(pkg)) => match sibling(&inst.path, "pnpm", &["cmd", "exe"]) {
            Some(pnpm) => Step::Run {
                program: pnpm,
                args: vec!["add".into(), "-g".into(), format!("{pkg}@latest")],
                env: Vec::new(),
            },
            None => Step::Manual(format!("这一份是 pnpm 装的：在终端里跑 pnpm add -g {pkg}@latest")),
        },
        (Source::Volta, Some(pkg)) => match sibling(&inst.path, "volta", &["exe", "cmd"]) {
            Some(volta) => Step::Run {
                program: volta,
                args: vec!["install".into(), pkg.to_string()],
                env: Vec::new(),
            },
            None => Step::Manual(format!("这一份是 Volta 装的：在终端里跑 volta install {pkg}")),
        },
        // Grok 官方安装器装的：它自己的 `grok update`（cc-switch Windows 上也是先跑它）。
        (Source::Installer, _) if tool == CliTool::GrokBuild => Step::Run {
            program: inst.path.clone(),
            args: vec!["update".into()],
            env: Vec::new(),
        },
        // MiniMax Code 安装器装的：`mcode update` 在不是终端的环境里只打一句提示就退出、什么都不装（cc-switch 注明），
        // 所以重跑官方脚本、指定写回同一个目录。
        (Source::Installer, _) if tool == CliTool::MiniMaxCode => {
            let root = inst.path.parent().map(Path::to_path_buf).unwrap_or_default();
            match tool.script() {
                Some(script) => Step::Script {
                    script,
                    env: vec![(
                        script.dir_env.unwrap_or("MCODE_INSTALL_DIR").to_string(),
                        root.display().to_string(),
                    )],
                },
                None => Step::Manual("重跑 MiniMax Code 的官方安装脚本。".into()),
            }
        }
        // OpenCode 自己的 `upgrade` 在 Windows 上可能弹交互提示（anomalyco/opencode#17295），静默跑会挂住 —— 不替人跑。
        (Source::Installer, _) => Step::Manual(format!(
            "这一份是 {} 自己的安装器装的：在终端里跑 {} upgrade",
            tool.label(),
            tool.command()
        )),
        // Hermes：它自己的 `hermes update`（Hermes 知道自己装在哪个 Python 环境里）。
        (_, None) if tool == CliTool::Hermes => Step::Run {
            program: inst.path.clone(),
            args: vec!["update".into()],
            env: Vec::new(),
        },
        (Source::Yarn, Some(pkg)) => {
            Step::Manual(format!("这一份是 Yarn 装的：在终端里跑 yarn global add {pkg}@latest"))
        }
        (Source::Bun, Some(pkg)) => {
            Step::Manual(format!("这一份是 Bun 装的：在终端里跑 bun add -g {pkg}@latest"))
        }
        (source, _) => Step::Manual(format!(
            "这一份（{}）面板认不出是怎么装的，不替你升级 —— 裸跑 npm 只会在别处另装一份，命令行用的还是这一份。\
             用当初装它的方式升级。",
            source.label()
        )),
    }
}

/// 卸载某一份：只做包管理器装的（npm / nvm-windows / pnpm / Volta）。别的给出要做什么。
pub fn uninstall_plan(tool: CliTool, inst: &Install, r: &Roots) -> Step {
    let Some(pkg) = tool.npm_package() else {
        return Step::Manual(format!(
            "{} 是 Python 装的：按它官方文档卸载（面板不替你删 Python 环境里的东西）。",
            tool.label()
        ));
    };
    match inst.source {
        Source::Npm | Source::Nvm => {
            match sibling(&inst.path, "npm", &["cmd", "exe"]).or_else(|| npm_on_path(r)) {
                Some(npm) => Step::Run {
                    program: npm,
                    args: npm_args(&["uninstall", "-g"], tool, pkg, false),
                    env: Vec::new(),
                },
                None => Step::Manual(format!("没找到 npm。在终端里跑：npm uninstall -g {pkg}")),
            }
        }
        Source::Pnpm => match sibling(&inst.path, "pnpm", &["cmd", "exe"]) {
            Some(pnpm) => Step::Run {
                program: pnpm,
                args: vec!["remove".into(), "-g".into(), pkg.to_string()],
                env: Vec::new(),
            },
            None => Step::Manual(format!("在终端里跑 pnpm remove -g {pkg}")),
        },
        Source::Volta => match sibling(&inst.path, "volta", &["exe", "cmd"]) {
            Some(volta) => Step::Run {
                program: volta,
                args: vec!["uninstall".into(), pkg.to_string()],
                env: Vec::new(),
            },
            None => Step::Manual(format!("在终端里跑 volta uninstall {pkg}")),
        },
        Source::Yarn => Step::Manual(format!("在终端里跑 yarn global remove {pkg}")),
        Source::Bun => Step::Manual(format!("在终端里跑 bun remove -g {pkg}")),
        Source::Installer => Step::Manual(format!(
            "这一份是 {} 自己的安装器装的：按它官方文档卸载，或者删掉 {} 所在的目录并从 PATH 里去掉它。",
            tool.label(),
            inst.path.display()
        )),
        other => Step::Manual(format!(
            "这一份（{}）面板认不出是怎么装的，不替你删。用当初装它的方式卸载。",
            other.label()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一棵临时的假机器。结束时整棵删掉。
    struct Tree(PathBuf);
    impl Tree {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "qb-cli-tools-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn file(&self, rel: &str, body: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
            p
        }
        fn roots(&self, path_dirs: &[&str]) -> Roots {
            Roots {
                home: Some(self.0.join("home")),
                roaming: Some(self.0.join("roaming")),
                local: Some(self.0.join("local")),
                program_files: vec![self.0.join("pf")],
                program_data: Some(self.0.join("pd")),
                path_dirs: path_dirs.iter().map(|d| self.0.join(d)).collect(),
                env: Vec::new(),
            }
        }
    }
    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// npm 的 cmd-shim 生成的那种启动壳（照 npm 10 的原样，只改了包名）。
    fn npm_shim(pkg_path: &str) -> String {
        format!(
            "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\n\
             IF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n)\r\n\r\n\
             endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\{pkg_path}\" %*\r\n"
        )
    }

    #[test]
    fn the_shim_leads_to_the_package_and_its_version() {
        let t = Tree::new();
        let shim = t.file(
            "roaming/npm/opencode.cmd",
            &npm_shim("opencode-ai\\bin\\opencode"),
        );
        t.file(
            "roaming/npm/node_modules/opencode-ai/package.json",
            r#"{"name":"opencode-ai","version":"0.15.8"}"#,
        );
        let dir = shim.parent().unwrap();
        let pkg =
            package_dir_from_shim(&std::fs::read_to_string(&shim).unwrap(), dir, "opencode-ai")
                .unwrap();
        assert_eq!(
            norm(&pkg),
            norm(&dir.join("node_modules").join("opencode-ai"))
        );
        assert_eq!(package_version(&pkg).as_deref(), Some("0.15.8"));
    }

    #[test]
    fn shim_parsing_handles_scoped_names_yarn_parents_and_absolute_paths() {
        let dir = Path::new(r"C:\Users\me\AppData\Local\Yarn\bin");
        let yarn = r#"@"%~dp0\..\Data\global\node_modules\@earendil-works\pi-coding-agent\dist\cli.js" %*"#;
        assert_eq!(
            norm(&package_dir_from_shim(yarn, dir, "@earendil-works/pi-coding-agent").unwrap()),
            norm(Path::new(
                r"C:\Users\me\AppData\Local\Yarn\Data\global\node_modules\@earendil-works\pi-coding-agent"
            ))
        );
        let abs = r#""D:\npm-global\node_modules\openclaw\openclaw.mjs" %*"#;
        assert_eq!(
            norm(&package_dir_from_shim(abs, dir, "openclaw").unwrap()),
            norm(Path::new(r"D:\npm-global\node_modules\openclaw"))
        );
        // 包名只是前缀的不算；指着别的包的不算；相对路径认不出就不猜。
        assert!(
            package_dir_from_shim(r#""%dp0%\node_modules\pi-extra\x.js""#, dir, "pi").is_none()
        );
        assert!(package_dir_from_shim(r#""%dp0%\node_modules\other\x.js""#, dir, "pi").is_none());
        assert!(package_dir_from_shim(r#""..\node_modules\pi\x.js""#, dir, "pi").is_none());
    }

    #[test]
    fn sources_are_read_from_the_entry_path() {
        let s = |p: &str| source_of(CliTool::OpenClaw, Path::new(p));
        assert_eq!(
            s(r"C:\Users\me\AppData\Roaming\npm\openclaw.cmd"),
            Source::Npm
        );
        assert_eq!(s(r"C:\Program Files\nodejs\openclaw.cmd"), Source::Npm);
        assert_eq!(
            s(r"C:\Users\me\AppData\Roaming\nvm\v24.16.0\openclaw.cmd"),
            Source::Nvm
        );
        assert_eq!(
            s(r"C:\Users\me\AppData\Local\pnpm\openclaw.cmd"),
            Source::Pnpm
        );
        assert_eq!(
            s(r"C:\Users\me\AppData\Local\Volta\bin\openclaw.exe"),
            Source::Volta
        );
        assert_eq!(
            s(r"C:\Users\me\AppData\Local\Yarn\bin\openclaw.cmd"),
            Source::Yarn
        );
        assert_eq!(s(r"C:\Users\me\.bun\bin\openclaw.exe"), Source::Bun);
        assert_eq!(s(r"C:\Users\me\scoop\shims\openclaw.exe"), Source::Scoop);
        assert_eq!(s(r"D:\tools\openclaw.exe"), Source::Unknown);
        assert_eq!(
            source_of(
                CliTool::GrokBuild,
                Path::new(r"C:\Users\me\.grok\bin\grok.exe")
            ),
            Source::Installer
        );
        assert_eq!(
            source_of(
                CliTool::MiniMaxCode,
                Path::new(r"C:\Users\me\.minimax-code\mcode.cmd")
            ),
            Source::Installer
        );
        assert_eq!(
            source_of(
                CliTool::Hermes,
                Path::new(
                    r"C:\Users\me\AppData\Local\Programs\Python\Python312\Scripts\hermes.exe"
                )
            ),
            Source::Python
        );
    }

    #[test]
    fn two_copies_are_found_the_path_one_is_default_and_versions_come_from_files() {
        let t = Tree::new();
        // 默认 npm 前缀里一份（0.15.8），pnpm 里一份（0.14.0）；PATH 上 pnpm 在前。
        t.file(
            "roaming/npm/opencode.cmd",
            &npm_shim("opencode-ai\\bin\\opencode"),
        );
        t.file(
            "roaming/npm/node_modules/opencode-ai/package.json",
            r#"{"version":"0.15.8"}"#,
        );
        t.file(
            "local/pnpm/opencode.cmd",
            &npm_shim("..\\pnpm\\global\\5\\node_modules\\opencode-ai\\bin\\opencode")
                .replace("%dp0%\\node_modules\\..\\pnpm", "%dp0%"),
        );
        t.file(
            "local/pnpm/global/5/node_modules/opencode-ai/package.json",
            r#"{"version":"0.14.0"}"#,
        );
        let r = t.roots(&["local/pnpm", "roaming/npm"]);
        let found = scan(CliTool::OpenCode, &r);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].default, "PATH 上第一个排最前");
        assert_eq!(found[0].source, Source::Pnpm);
        assert_eq!(found[0].version.as_deref(), Some("0.14.0"));
        assert_eq!(found[1].source, Source::Npm);
        assert_eq!(found[1].version.as_deref(), Some("0.15.8"));
        assert!(is_conflict(&found));
    }

    #[test]
    fn exe_wins_over_cmd_in_the_same_folder_and_aliases_are_skipped() {
        let t = Tree::new();
        t.file("pathdir/grok.cmd", "@echo off");
        t.file("pathdir/grok.exe", "MZ");
        t.file("local/Microsoft/WindowsApps/grok.exe", "MZ");
        let r = t.roots(&["local/Microsoft/WindowsApps", "pathdir"]);
        let found = scan(CliTool::GrokBuild, &r);
        assert_eq!(found.len(), 1);
        assert!(found[0].path.ends_with("grok.exe"));
        assert!(found[0].default);
        assert!(found[0].version.is_none(), "原生程序不运行它去问版本");
    }

    #[test]
    fn effective_path_merges_and_expands() {
        let lookup = |n: &str| match n {
            "USERPROFILE" => Some(r"C:\Users\me".to_string()),
            "LOOP" => Some("%LOOP%".to_string()),
            _ => None,
        };
        assert_eq!(
            expand_vars(r"%USERPROFILE%\.local\bin;%NOPE%\x;%LOOP%", &lookup),
            r"C:\Users\me\.local\bin;%NOPE%\x;%LOOP%"
        );
        let merged = merge_path_lists(&[r"C:\A;C:\b\".to_string(), r"c:\a;C:\B;;C:\C".to_string()]);
        assert_eq!(merged.len(), 3, "{merged:?}");
    }

    #[test]
    fn updates_go_back_to_whoever_installed_the_default_copy() {
        let t = Tree::new();
        let npm = t.file("pf/nodejs/npm.cmd", "@echo off");
        let r = t.roots(&["pf/nodejs"]);
        let inst = |source: Source, path: PathBuf, default: bool| Install {
            path,
            source,
            version: None,
            package_dir: None,
            default,
        };

        // 默认 %APPDATA%\npm：用 PATH 上的 npm，带 MiniMax 的额外参数。
        let p = update_plan(
            CliTool::MiniMaxCode,
            &[inst(Source::Npm, t.0.join("roaming/npm/mcode.cmd"), true)],
            &r,
        );
        assert_eq!(
            p,
            Step::Run {
                program: npm.clone(),
                args: vec![
                    "i".into(),
                    "-g".into(),
                    "@minimax-ai/code@latest".into(),
                    "--ignore-scripts=false".into(),
                    "--include=optional".into(),
                    "--allow-scripts=@minimax-ai/code,better-sqlite3".into()
                ],
                env: vec![]
            }
        );

        // nvm-windows：同一层的 npm.cmd。
        let nvm_npm = t.file("roaming/nvm/v24.16.0/npm.cmd", "");
        let p = update_plan(
            CliTool::OpenClaw,
            &[inst(
                Source::Nvm,
                t.0.join("roaming/nvm/v24.16.0/openclaw.cmd"),
                true,
            )],
            &r,
        );
        assert!(matches!(p, Step::Run { ref program, .. } if *program == nvm_npm));

        // pnpm：同一层的 pnpm。
        let pnpm = t.file("local/pnpm/pnpm.exe", "MZ");
        let p = update_plan(
            CliTool::Pi,
            &[inst(Source::Pnpm, t.0.join("local/pnpm/pi.cmd"), true)],
            &r,
        );
        assert_eq!(
            p,
            Step::Run {
                program: pnpm,
                args: vec![
                    "add".into(),
                    "-g".into(),
                    "@earendil-works/pi-coding-agent@latest".into()
                ],
                env: vec![]
            }
        );

        // Grok 安装器：它自己的 update。MiniMax 安装器：重跑脚本、写回同一个目录。
        let grok = t.0.join("home/.grok/bin/grok.exe");
        let p = update_plan(
            CliTool::GrokBuild,
            &[inst(Source::Installer, grok.clone(), true)],
            &r,
        );
        assert!(
            matches!(p, Step::Run { ref program, ref args, .. } if *program == grok && args == &["update"])
        );
        let p = update_plan(
            CliTool::MiniMaxCode,
            &[inst(
                Source::Installer,
                t.0.join("home/.minimax-code/mcode.cmd"),
                true,
            )],
            &r,
        );
        match p {
            Step::Script { script, env } => {
                assert!(script.url.ends_with("install.ps1"));
                assert_eq!(env[0].0, "MCODE_INSTALL_DIR");
                assert!(env[0].1.ends_with(".minimax-code"));
            }
            other => panic!("{other:?}"),
        }

        // 认不出的原生程序：不替人升级。好几份又分不清默认：也不升。
        let p = update_plan(
            CliTool::OpenClaw,
            &[inst(Source::Unknown, t.0.join("tools/openclaw.exe"), true)],
            &r,
        );
        assert!(matches!(p, Step::Manual(ref m) if m.contains("不替你升级")));
        let p = update_plan(
            CliTool::OpenClaw,
            &[
                inst(Source::Npm, t.0.join("a/openclaw.cmd"), false),
                inst(Source::Npm, t.0.join("b/openclaw.cmd"), false),
            ],
            &r,
        );
        assert!(matches!(p, Step::Manual(ref m) if m.contains("分不清")));
    }

    #[test]
    fn new_installs_need_npm_and_hermes_needs_its_script() {
        let t = Tree::new();
        let r = t.roots(&[]);
        assert!(matches!(
            npm_install_plan(CliTool::OpenCode, &r),
            Step::Manual(ref m) if m.contains("Node.js")
        ));
        assert!(matches!(
            npm_install_plan(CliTool::Hermes, &r),
            Step::Manual(_)
        ));
        let npm = t.file("pf/nodejs/npm.cmd", "");
        assert_eq!(
            npm_install_plan(CliTool::OpenCode, &t.roots(&[])),
            Step::Run {
                program: npm,
                args: vec!["i".into(), "-g".into(), "opencode-ai@latest".into()],
                env: vec![]
            }
        );
        assert!(CliTool::Hermes.script().is_some());
        assert!(
            CliTool::OpenCode.script().is_none(),
            "OpenCode 的官方脚本只有 bash 版"
        );
    }

    #[test]
    fn uninstall_only_drives_package_managers() {
        let t = Tree::new();
        let npm = t.file("pf/nodejs/npm.cmd", "");
        let r = t.roots(&["pf/nodejs"]);
        let i = Install {
            path: t.0.join("roaming/npm/pi.cmd"),
            source: Source::Npm,
            version: None,
            package_dir: None,
            default: true,
        };
        assert_eq!(
            uninstall_plan(CliTool::Pi, &i, &r),
            Step::Run {
                program: npm,
                args: vec![
                    "uninstall".into(),
                    "-g".into(),
                    "@earendil-works/pi-coding-agent".into()
                ],
                env: vec![]
            }
        );
        let grok = Install {
            source: Source::Installer,
            path: t.0.join("home/.grok/bin/grok.exe"),
            ..i.clone()
        };
        assert!(matches!(
            uninstall_plan(CliTool::GrokBuild, &grok, &r),
            Step::Manual(_)
        ));
        assert!(matches!(
            uninstall_plan(CliTool::Hermes, &i, &r),
            Step::Manual(_)
        ));
    }

    #[test]
    fn the_shown_command_is_something_a_person_can_paste_into_powershell() {
        let s = Step::Run {
            program: PathBuf::from(r"C:\Program Files\nodejs\npm.cmd"),
            args: vec!["i".into(), "-g".into(), "openclaw@latest".into()],
            env: vec![],
        };
        assert_eq!(
            s.shown(),
            r#"& "C:\Program Files\nodejs\npm.cmd" i -g openclaw@latest"#
        );
    }

    #[test]
    fn tool_names_match_the_coexist_tools_on_the_wire() {
        // 跟 `relay::coexist::CoexistTool` 的序列化名一致（两个枚举在不同 crate，界面上拿同一个名字对）。
        let names: Vec<String> = CliTool::ALL
            .iter()
            .map(|t| {
                serde_json::to_value(t)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            names,
            [
                "opencode",
                "grok-build",
                "openclaw",
                "hermes",
                "pi",
                "minimax-code"
            ]
        );
    }
}
