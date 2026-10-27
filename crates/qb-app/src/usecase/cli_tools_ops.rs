// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 其余六个命令行工具的编排（2026-10-27，中转站重构阶段 7 · X3，照 cc-switch v4.0.14 的 `get_tool_versions` /
//! `run_tool_lifecycle_action` / `tool_node_requirement`）：现状、最新版本、装 / 升 / 卸、官方安装脚本。
//!
//! 位置与「交给谁做」的判断在 [`qb_install::install::cli_tools`]（纯函数 + 只读文件）；这里管联网与起进程。
//!
//! # 三条规矩
//!
//! 1. **联网只在点了之后**：打开软件页只读文件（[`status`]）。「检查新版本」才问 npm / GitHub / PyPI（[`latest`]）。
//! 2. **起进程走 [`winget::run_streaming_env`]**：每个参数一个 argv、隐藏窗口、等它退出、输出逐行进软件页的进度条
//!    （`CLAUDE.md`「别把 shell 当启动目标」）。`.cmd`（`npm.cmd` / `pnpm.cmd`）直接当程序交给标准库 ——
//!    它会替批处理套上 `cmd.exe` 并按批处理的规矩转义参数。装完 / 升完**回读盘核对**，不看退出码（§7.21）。
//! 3. ⛔ **官方安装脚本先下载给人看，确认之后才跑**（阶段 7 的计划写明「别直接管道给 shell」）：[`script_fetch`] 下到面板数据目录、
//!    算 SHA-256、把全文交给界面；[`script_run`] 只认那一份、哈希对得上才用 `powershell -File` 跑，跑完删掉。
//!    cc-switch 是 `powershell -EncodedCommand "irm <地址> | iex"` —— 拿到什么跑什么，而且是 base64
//!    （`CLAUDE.md`「杀软会把这个面板当木马」第 1 条：别再加隐藏窗口 + base64 的组合）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::error::{GateError, Result};
use crate::sink::ProgressSink;
use qb_install::install::cli_tools::{self, CliTool, Install, Roots, Step};
use qb_install::install::cli_tools_version as ver;
use qb_install::install::{detect, winget};

/// 装 / 升 / 卸 / 跑脚本的进度条段数：1 看清楚要做什么，2 跑，3 回读核对。
pub const TOTAL: u32 = 3;

/// 脚本最大多大。官方这几个都是几十 KB；大到这个数就不是安装脚本了。
const SCRIPT_MAX: usize = 1024 * 1024;
/// 界面上最多显示多少。
const SCRIPT_SHOWN: usize = 256 * 1024;
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

// ------------------------------------------------------------------ 现状

/// 「点了会做什么」给界面看的样子。
#[derive(Debug, Clone, Serialize, PartialEq, Eq, TS)]
#[ts(export, rename = "CliStepKind")]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    /// 面板替你跑。
    Run,
    /// 要先下载官方脚本给你看。
    Script,
    /// 面板不替你做，`shown` 里是要你自己做的事。
    Manual,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, TS)]
#[ts(export, rename = "CliStep")]
pub struct StepView {
    pub kind: StepKind,
    pub shown: String,
}

impl From<&Step> for StepView {
    fn from(s: &Step) -> Self {
        Self {
            kind: match s {
                Step::Run { .. } => StepKind::Run,
                Step::Script { .. } => StepKind::Script,
                Step::Manual(_) => StepKind::Manual,
            },
            shown: s.shown(),
        }
    }
}

/// 软件页上一个工具的样子。**只读文件得来**，没有联网、没有起它。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CliToolStatus {
    pub tool: CliTool,
    pub label: String,
    pub command: String,
    /// 本机的几份，命令行默认用的那份在最前。
    pub installs: Vec<Install>,
    /// 装了好几份、版本对不上（或读不出）。
    pub conflict: bool,
    pub npm_package: Option<String>,
    /// 有官方 Windows 安装脚本的话，它的地址。
    pub script_url: Option<String>,
    /// 用 npm 新装会做什么（没 npm 时是一句「先装 Node.js」）。
    pub install: StepView,
    /// 点「升级」会做什么。没装就没有。
    pub update: Option<StepView>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CliToolsReport {
    pub tools: Vec<CliToolStatus>,
    /// 新装用的那个 `npm.cmd`。没有 = 没装 Node.js（或不在 PATH 上）。
    pub npm: Option<PathBuf>,
}

pub fn status() -> CliToolsReport {
    status_in(&Roots::current())
}

pub fn status_in(r: &Roots) -> CliToolsReport {
    let tools = CliTool::ALL
        .iter()
        .map(|&tool| {
            let installs = cli_tools::scan(tool, r);
            CliToolStatus {
                tool,
                label: tool.label().into(),
                command: tool.command().into(),
                conflict: cli_tools::is_conflict(&installs),
                npm_package: tool.npm_package().map(str::to_string),
                script_url: tool.script().map(|s| s.url.to_string()),
                install: StepView::from(&cli_tools::npm_install_plan(tool, r)),
                update: (!installs.is_empty())
                    .then(|| StepView::from(&cli_tools::update_plan(tool, &installs, r))),
                installs,
            }
        })
        .collect();
    CliToolsReport {
        tools,
        npm: cli_tools::npm_on_path(r),
    }
}

// ------------------------------------------------------------------ 最新版本（联网，只在点了之后）

#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct CliToolLatest {
    pub tool: Option<CliTool>,
    pub latest: Option<String>,
    /// 从哪问到的：npm / GitHub / PyPI。
    pub from: Option<String>,
    /// 命令行默认那一份比它旧。读不出（版本认不出、没装）是 `None`。
    pub outdated: Option<bool>,
    /// 新版本要求的 Node（`engines.node`，原样）。
    pub node_required: Option<String>,
    /// 升级会用到的那个 Node 的版本。
    pub node_current: Option<String>,
    /// `false` = 本机 Node 不够新：先升 Node，否则装上也跑不起来。认不出写法时是 `None`（不拦人）。
    pub node_ok: Option<bool>,
    /// 问不到时的原因，照实说。
    pub problems: Vec<String>,
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("QB Gate/", env!("CARGO_PKG_VERSION")))
        // 问版本的请求拿不到就算了（显示「问不到」），别让按钮一直转。
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_default()
}

/// 带 `/` 的包名在 registry 的地址里写成 `%2f`。
fn npm_escape(pkg: &str) -> String {
    pkg.replace('/', "%2f")
}

async fn get_json(
    client: &reqwest::Client,
    url: &str,
) -> std::result::Result<serde_json::Value, String> {
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let body = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?;
    if !status.is_success() {
        let msg = body
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        return Err(format!(
            "HTTP {}{}",
            status.as_u16(),
            if msg.is_empty() {
                String::new()
            } else {
                format!("：{msg}")
            }
        ));
    }
    Ok(body)
}

/// npm 的 dist-tags 专用地址：回的就是 dist-tags 本身，几十字节。整份 packument 对高频发版的包有十几 MB（cc-switch #7339）。
pub fn npm_dist_tags_url(pkg: &str) -> String {
    format!(
        "https://registry.npmjs.org/-/package/{}/dist-tags",
        npm_escape(pkg)
    )
}

/// 某一个版本的元数据（只为读 `engines.node`），不是整份 packument。
pub fn npm_version_url(pkg: &str, version: &str) -> String {
    format!("https://registry.npmjs.org/{}/{version}", npm_escape(pkg))
}

pub fn github_latest_url(repo: &str) -> String {
    format!("https://api.github.com/repos/{repo}/releases/latest")
}

pub fn pypi_url(pkg: &str) -> String {
    format!("https://pypi.org/pypi/{pkg}/json")
}

/// 问一次最新版本，顺带看本机 Node 够不够新版用。**会联网**：只在使用者点「检查新版本」时调。
pub async fn latest(tool: CliTool) -> CliToolLatest {
    let r = Roots::current();
    let installs = cli_tools::scan(tool, &r);
    let default = cli_tools::default_install(&installs).cloned();
    let local = default.as_ref().and_then(|i| i.version.clone());
    let client = http();
    let mut out = CliToolLatest {
        tool: Some(tool),
        ..Default::default()
    };

    let npm = |pkg: &'static str| {
        let client = client.clone();
        async move {
            get_json(&client, &npm_dist_tags_url(pkg))
                .await
                .and_then(|v| ver::npm_latest(&v).ok_or_else(|| "npm 上没有 latest".to_string()))
        }
    };
    let github = |repo: &'static str| {
        let client = client.clone();
        async move {
            get_json(&client, &github_latest_url(repo))
                .await
                .and_then(|v| {
                    ver::github_release_version(&v)
                        .ok_or_else(|| "最新发布里读不出版本号".to_string())
                })
        }
    };
    let (found, from) = match tool.latest_from() {
        cli_tools::LatestFrom::Npm(pkg) => match npm(pkg).await {
            Ok(v) => (Some(v), "npm"),
            Err(e) => {
                out.problems.push(format!("npm：{e}"));
                (None, "")
            }
        },
        cli_tools::LatestFrom::NpmThenGithub(pkg, repo) => match npm(pkg).await {
            Ok(v) => (Some(v), "npm"),
            Err(e) => {
                out.problems.push(format!("npm：{e}"));
                match github(repo).await {
                    Ok(v) => (Some(v), "GitHub"),
                    Err(e) => {
                        out.problems.push(format!("GitHub：{e}"));
                        (None, "")
                    }
                }
            }
        },
        cli_tools::LatestFrom::GithubThenPypi(repo, pypi) => match github(repo).await {
            Ok(v) => (Some(v), "GitHub"),
            Err(e) => {
                out.problems.push(format!("GitHub：{e}"));
                let v = get_json(&client, &pypi_url(pypi))
                    .await
                    .map(|j| ver::pypi_version(&j));
                match v {
                    // PyPI 上的已经停更：比本机还旧的不显示。
                    Ok(v) => (ver::drop_behind_local(v, local.as_deref()), "PyPI"),
                    Err(e) => {
                        out.problems.push(format!("PyPI：{e}"));
                        (None, "")
                    }
                }
            }
        },
    };
    out.from = found.as_ref().map(|_| from.to_string());
    out.outdated = match (&local, &found) {
        (Some(l), Some(f)) => ver::is_outdated(l, f),
        _ => None,
    };

    // 新版要的 Node：只看 npm 包、而且这一份是 Node 跑的那几类（MiniMax Code 的官方安装器自带一份 Node，不看）。
    let node_matters = default.as_ref().is_none_or(|i| {
        !matches!(
            i.source,
            cli_tools::Source::Installer | cli_tools::Source::Python
        )
    });
    if let (Some(pkg), Some(v), true) = (tool.npm_package(), found.as_deref(), node_matters) {
        match get_json(&client, &npm_version_url(pkg, v)).await {
            Ok(meta) => {
                out.node_required = meta
                    .get("engines")
                    .and_then(|e| e.get("node"))
                    .and_then(|n| n.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
            }
            Err(e) => out.problems.push(format!("npm（{v} 要哪个 Node）：{e}")),
        }
        let node = default
            .as_ref()
            .and_then(|i| i.path.parent().map(|d| d.join("node.exe")))
            .filter(|p| p.is_file())
            .or_else(detect::node_exe);
        out.node_current = node
            .as_deref()
            .and_then(detect::node_version)
            .map(|s| s.trim_start_matches('v').to_string());
        if let (Some(req), Some(cur)) = (&out.node_required, &out.node_current) {
            out.node_ok = ver::node_range_satisfied(req, cur);
        }
    }
    out.latest = found;
    out
}

// ------------------------------------------------------------------ 装 / 升 / 卸

/// 跑一步。`Script` 与 `Manual` 在这里一律不跑（前者要先给人看过，后者面板不替人做）。
async fn run_step(step: &Step, rep: &dyn ProgressSink, log: &mut Vec<String>) -> Result<bool> {
    match step {
        Step::Run { program, args, env } => {
            rep.phase(2, &step.shown());
            let env: Vec<(&str, &str)> =
                env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            winget::run_streaming_env(&program.display().to_string(), args, &env, rep, 2, log).await
        }
        Step::Script { script, .. } => Err(GateError::Other(format!(
            "这一步要跑官方安装脚本（{}）：点「用官方脚本…」，先看过脚本内容再跑。",
            script.url
        ))),
        Step::Manual(m) => Err(GateError::Other(m.clone())),
    }
}

fn tail(log: &[String]) -> String {
    let t: Vec<String> = log.iter().rev().take(3).rev().cloned().collect();
    if t.is_empty() {
        "（没有输出）".into()
    } else {
        t.join(" / ")
    }
}

/// 用 npm 新装（`npm i -g <包>@latest`，PATH 上那个 npm）。装完回读核对。
pub async fn install_npm(tool: CliTool, rep: &dyn ProgressSink) -> Result<String> {
    rep.phase(1, "找 npm");
    let r = Roots::current();
    let step = cli_tools::npm_install_plan(tool, &r);
    let mut log = Vec::new();
    let ok = run_step(&step, rep, &mut log).await?;
    rep.phase(3, "回读核对");
    let found = cli_tools::scan(tool, &Roots::current());
    match cli_tools::default_install(&found).or(found.first()) {
        Some(i) => Ok(format!(
            "{} {} 已装好（{}）：{}",
            tool.label(),
            i.version.clone().unwrap_or_else(|| "版本读不出".into()),
            i.source.label(),
            i.path.display()
        )),
        None => Err(GateError::Other(format!(
            "npm {}，但哪儿都找不到 {}（npm 装到了一个不在 PATH 上的前缀？）。npm 最后几行：{}",
            if ok { "报告成功" } else { "没有成功" },
            tool.command(),
            tail(&log)
        ))),
    }
}

/// 升级命令行默认用的那一份，交给装它的那个包管理器（[`cli_tools::update_plan`]）。升完对比版本。
pub async fn update(tool: CliTool, rep: &dyn ProgressSink) -> Result<String> {
    rep.phase(1, "看命令行用的是哪一份、是谁装的");
    let r = Roots::current();
    let installs = cli_tools::scan(tool, &r);
    let before = cli_tools::default_install(&installs).cloned();
    let step = cli_tools::update_plan(tool, &installs, &r);
    let mut log = Vec::new();
    let ok = run_step(&step, rep, &mut log).await?;
    rep.phase(3, "回读核对");
    let after = cli_tools::scan(tool, &Roots::current());
    let now = before.as_ref().and_then(|b| {
        after
            .iter()
            .find(|i| cli_tools::norm(&i.path) == cli_tools::norm(&b.path))
    });
    let (old, new) = (
        before.as_ref().and_then(|b| b.version.clone()),
        now.and_then(|n| n.version.clone()),
    );
    if !ok {
        return Err(GateError::Other(format!(
            "升级没有成功。最后几行：{}",
            tail(&log)
        )));
    }
    Ok(match (old, new) {
        (Some(o), Some(n)) if o != n => format!("{} 已从 {o} 升到 {n}。", tool.label()),
        (_, Some(n)) => format!("{} 跑完了，版本还是 {n}（多半已经是最新）。", tool.label()),
        _ => format!(
            "{} 跑完了（这一份的版本面板读不出，自己在终端里看一眼）。",
            tool.label()
        ),
    })
}

/// 卸掉某一份（界面上点的那一行）。只做包管理器装的那几类（[`cli_tools::uninstall_plan`]）。
pub async fn uninstall(tool: CliTool, path: &str, rep: &dyn ProgressSink) -> Result<String> {
    rep.phase(1, "核对要卸的那一份");
    let r = Roots::current();
    let installs = cli_tools::scan(tool, &r);
    let want = cli_tools::norm(Path::new(path));
    let inst = installs
        .iter()
        .find(|i| cli_tools::norm(&i.path) == want)
        .ok_or_else(|| GateError::Other(format!("{path} 已经不在了（刷新一下看看）")))?;
    let step = cli_tools::uninstall_plan(tool, inst, &r);
    let mut log = Vec::new();
    let ok = run_step(&step, rep, &mut log).await?;
    rep.phase(3, "回读核对");
    let still = cli_tools::scan(tool, &Roots::current())
        .iter()
        .any(|i| cli_tools::norm(&i.path) == want);
    if still {
        return Err(GateError::Other(format!(
            "{} {}，但 {path} 还在。最后几行：{}",
            step.shown(),
            if ok { "报告成功" } else { "没有成功" },
            tail(&log)
        )));
    }
    Ok(format!("已卸掉 {path}。"))
}

// ------------------------------------------------------------------ 官方安装脚本：先给人看，确认之后才跑

/// 下载这份脚本是为了装，还是为了升级「脚本装的那一份」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "CliScriptPurpose")]
#[serde(rename_all = "snake_case")]
pub enum ScriptPurpose {
    Install,
    Update,
}

/// 下载下来、等使用者看过的那一份。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, rename = "CliToolScript")]
pub struct Fetched {
    pub id: String,
    pub tool: CliTool,
    pub purpose: ScriptPurpose,
    pub url: String,
    /// 跳转之后实际取到内容的地址（跟 `url` 不一样时界面写出来）。
    pub final_url: String,
    /// 下载到的原样字节的 SHA-256。
    pub sha256: String,
    #[ts(type = "number")]
    pub size: u64,
    /// 跑的时候会带的环境变量（升级 MiniMax Code 时指定写回哪个目录）。
    pub env: Vec<(String, String)>,
    /// 脚本全文（有损 UTF-8）；超过 256 KiB 的部分不显示，`truncated` 为真。
    /// **不落进 json 那一份**：存说明文件时它还是空的，全文只在交给界面的那一份里。
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub truncated: bool,
    /// 存盘时补了 UTF-8 BOM（PowerShell 5.1 读没有 BOM 的脚本按系统代码页，中文会乱）。核对哈希时先去掉它。
    #[serde(default)]
    pub bom_added: bool,
}

/// 脚本放在哪。调用方给面板数据目录下的 `tool-installers`，单测给临时目录。
pub fn script_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("tool-installers")
}

fn script_paths(dir: &Path, id: &str) -> Result<(PathBuf, PathBuf)> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !ok {
        return Err(GateError::Other(format!("脚本编号不对：{id}")));
    }
    Ok((
        dir.join(format!("{id}.ps1")),
        dir.join(format!("{id}.json")),
    ))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// 下载到的一份：登记的地址、跳转之后的地址、原样字节。
pub struct Download<'a> {
    pub url: &'a str,
    pub final_url: &'a str,
    pub bytes: &'a [u8],
}

/// 把下载到的字节落盘、写好说明，交回给界面看的那一份。**纯本地**（下载在 [`script_fetch`]），单测直接喂字节。
pub fn stage_script(
    dir: &Path,
    tool: CliTool,
    purpose: ScriptPurpose,
    got: &Download<'_>,
    env: Vec<(String, String)>,
    now: chrono::DateTime<chrono::Local>,
) -> Result<Fetched> {
    let (url, final_url, bytes) = (got.url, got.final_url, got.bytes);
    if bytes.len() > SCRIPT_MAX {
        return Err(GateError::Other(format!(
            "{url} 回了 {} 字节，不像一个安装脚本，没存",
            bytes.len()
        )));
    }
    if bytes.is_empty() {
        return Err(GateError::Other(format!("{url} 回了一个空文件")));
    }
    std::fs::create_dir_all(dir)?;
    let id = format!(
        "{}-{}",
        serde_json::to_value(tool)?.as_str().unwrap_or("tool"),
        now.format("%Y%m%d-%H%M%S-%3f")
    );
    let (ps1, json) = script_paths(dir, &id)?;
    let bom_added = !bytes.starts_with(UTF8_BOM);
    let mut on_disk = Vec::with_capacity(bytes.len() + 3);
    if bom_added {
        on_disk.extend_from_slice(UTF8_BOM);
    }
    on_disk.extend_from_slice(bytes);
    std::fs::write(&ps1, &on_disk)?;
    let shown = &bytes[..bytes.len().min(SCRIPT_SHOWN)];
    let mut f = Fetched {
        id,
        tool,
        purpose,
        url: url.into(),
        final_url: final_url.into(),
        sha256: sha256_hex(bytes),
        size: bytes.len() as u64,
        env,
        text: String::new(),
        truncated: bytes.len() > SCRIPT_SHOWN,
        bom_added,
    };
    std::fs::write(&json, serde_json::to_vec_pretty(&f)?)?;
    f.text = String::from_utf8_lossy(shown.strip_prefix(UTF8_BOM).unwrap_or(shown)).into_owned();
    Ok(f)
}

/// 下载官方安装脚本、存到本机、交给界面看。**会联网**（只连这个工具登记的那一个地址，https）。
pub async fn script_fetch(tool: CliTool, purpose: ScriptPurpose, dir: &Path) -> Result<Fetched> {
    let script = tool
        .script()
        .ok_or_else(|| GateError::Other(format!("{} 没有官方的 Windows 安装脚本", tool.label())))?;
    let env = match purpose {
        ScriptPurpose::Install => Vec::new(),
        ScriptPurpose::Update => {
            let r = Roots::current();
            match cli_tools::update_plan(tool, &cli_tools::scan(tool, &r), &r) {
                Step::Script { env, .. } => env,
                other => {
                    return Err(GateError::Other(format!(
                        "命令行用的那一份不是官方脚本装的，升级不走脚本：{}",
                        other.shown()
                    )))
                }
            }
        }
    };
    let resp = http()
        .get(script.url)
        .send()
        .await
        .map_err(|e| GateError::Other(format!("下载 {} 失败：{e}", script.url)))?;
    let final_url = resp.url().to_string();
    if !final_url.starts_with("https://") {
        return Err(GateError::Other(format!(
            "{} 跳到了不是 https 的地址（{final_url}），没下载",
            script.url
        )));
    }
    if !resp.status().is_success() {
        return Err(GateError::Other(format!(
            "下载 {} 失败：HTTP {}",
            script.url,
            resp.status().as_u16()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| GateError::Other(format!("下载 {} 没读完：{e}", script.url)))?;
    stage_script(
        dir,
        tool,
        purpose,
        &Download {
            url: script.url,
            final_url: &final_url,
            bytes: &bytes,
        },
        env,
        chrono::Local::now(),
    )
}

/// 认出要跑的那一份：编号对得上、哈希跟使用者看到的一致、盘上的内容没被换过。**纯本地**。
pub fn staged_script(dir: &Path, id: &str, sha256: &str) -> Result<(Fetched, PathBuf)> {
    let (ps1, json) = script_paths(dir, id)?;
    let meta: Fetched = serde_json::from_slice(
        &std::fs::read(&json)
            .map_err(|_| GateError::Other("这份脚本已经不在了，重新下载一次".into()))?,
    )?;
    if !meta.sha256.eq_ignore_ascii_case(sha256) {
        return Err(GateError::Other(
            "要跑的不是你看过的那一份（哈希对不上），没跑".into(),
        ));
    }
    let on_disk = std::fs::read(&ps1)?;
    let body = if meta.bom_added {
        on_disk.strip_prefix(UTF8_BOM).unwrap_or(&on_disk)
    } else {
        &on_disk[..]
    };
    if sha256_hex(body) != meta.sha256 {
        return Err(GateError::Other(
            "盘上那份脚本在你看过之后被改过（哈希对不上），没跑。重新下载一次".into(),
        ));
    }
    Ok((meta, ps1))
}

/// 跑使用者看过的那一份脚本（`powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File`），跑完删掉，回读核对。
///
/// `-NonInteractive`：脚本要是停下来问东西（`Read-Host`），直接报错而不是在看不见的窗口里永远等着。
/// `-ExecutionPolicy Bypass` 只对这一次进程生效，不改系统的执行策略。
pub async fn script_run(
    dir: &Path,
    id: &str,
    sha256: &str,
    rep: &dyn ProgressSink,
) -> Result<String> {
    rep.phase(1, "核对要跑的是你看过的那一份");
    let (meta, ps1) = staged_script(dir, id, sha256)?;
    let args: Vec<String> = [
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ]
    .into_iter()
    .map(str::to_string)
    .chain(std::iter::once(ps1.display().to_string()))
    .collect();
    rep.phase(2, &format!("运行 {}", meta.url));
    let env: Vec<(&str, &str)> = meta
        .env
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let mut log = Vec::new();
    let ran = winget::run_streaming_env("powershell", &args, &env, rep, 2, &mut log).await;
    // 跑没跑成都删：留在盘上的脚本没人看着，下次也不会再用（要用就重新下载、重新看）。
    let (_, json) = script_paths(dir, id)?;
    let _ = std::fs::remove_file(&ps1);
    let _ = std::fs::remove_file(&json);
    let ok = ran?;
    rep.phase(3, "回读核对");
    let found = cli_tools::scan(meta.tool, &Roots::current());
    match (ok, cli_tools::default_install(&found).or(found.first())) {
        (true, Some(i)) => Ok(format!(
            "{} 的官方脚本跑完了：{}（{}）",
            meta.tool.label(),
            i.path.display(),
            i.version.clone().unwrap_or_else(|| "版本读不出".into())
        )),
        (true, None) => Err(GateError::Other(format!(
            "脚本报告成功，但哪儿都找不到 {}（它可能改了 PATH，要重开面板才看得到；或者装到了面板不认识的地方）。最后几行：{}",
            meta.tool.command(),
            tail(&log)
        ))),
        (false, _) => Err(GateError::Other(format!(
            "脚本没有成功。最后几行：{}",
            tail(&log)
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "qb-cli-ops-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn now() -> chrono::DateTime<chrono::Local> {
        use chrono::TimeZone;
        chrono::Local
            .with_ymd_and_hms(2026, 10, 27, 12, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn registry_urls_escape_scoped_names() {
        assert_eq!(
            npm_dist_tags_url("@minimax-ai/code"),
            "https://registry.npmjs.org/-/package/@minimax-ai%2fcode/dist-tags"
        );
        assert_eq!(
            npm_version_url("openclaw", "2026.9.3"),
            "https://registry.npmjs.org/openclaw/2026.9.3"
        );
        assert_eq!(
            github_latest_url("NousResearch/hermes-agent"),
            "https://api.github.com/repos/NousResearch/hermes-agent/releases/latest"
        );
    }

    #[test]
    fn a_staged_script_runs_only_with_the_hash_the_person_saw_and_only_if_unchanged() {
        let dir = TempDir::new();
        let body = "Write-Host '装 Grok'\r\n".as_bytes();
        let f = stage_script(
            &dir.0,
            CliTool::GrokBuild,
            ScriptPurpose::Install,
            &Download {
                url: "https://x.ai/cli/install.ps1",
                final_url: "https://x.ai/cli/install.ps1",
                bytes: body,
            },
            vec![],
            now(),
        )
        .unwrap();
        assert_eq!(f.sha256, sha256_hex(body));
        assert!(f.bom_added);
        assert_eq!(f.text, "Write-Host '装 Grok'\r\n");
        // 盘上那份带了 BOM（PowerShell 5.1 才读得对中文）。
        let (ps1, json) = script_paths(&dir.0, &f.id).unwrap();
        assert!(std::fs::read(&ps1).unwrap().starts_with(UTF8_BOM));
        // 说明文件里不放全文。
        assert!(!std::fs::read_to_string(&json)
            .unwrap()
            .contains("Write-Host"));

        assert!(staged_script(&dir.0, &f.id, &f.sha256).is_ok());
        assert!(staged_script(&dir.0, &f.id, &f.sha256.to_uppercase()).is_ok());
        let wrong = staged_script(&dir.0, &f.id, &"0".repeat(64)).unwrap_err();
        assert!(wrong.to_string().contains("不是你看过的那一份"));
        // 看过之后被改了一个字：不跑。
        let mut tampered = std::fs::read(&ps1).unwrap();
        tampered.extend_from_slice(b"Remove-Item C:\\x\r\n");
        std::fs::write(&ps1, tampered).unwrap();
        let changed = staged_script(&dir.0, &f.id, &f.sha256).unwrap_err();
        assert!(changed.to_string().contains("被改过"));
    }

    #[test]
    fn script_ids_cannot_walk_out_and_odd_downloads_are_refused() {
        let dir = TempDir::new();
        for bad in ["..\\x", "a/b", "", "x.ps1"] {
            assert!(staged_script(&dir.0, bad, "00").is_err(), "{bad}");
        }
        let got = |bytes: &'static [u8]| Download {
            url: "u",
            final_url: "u",
            bytes,
        };
        let big: &'static [u8] = Box::leak(vec![b'a'; SCRIPT_MAX + 1].into_boxed_slice());
        assert!(stage_script(
            &dir.0,
            CliTool::Hermes,
            ScriptPurpose::Install,
            &got(big),
            vec![],
            now()
        )
        .is_err());
        assert!(stage_script(
            &dir.0,
            CliTool::Hermes,
            ScriptPurpose::Install,
            &got(b""),
            vec![],
            now()
        )
        .is_err());
        // 自带 BOM 的不再补一个。
        let f = stage_script(
            &dir.0,
            CliTool::MiniMaxCode,
            ScriptPurpose::Update,
            &got(b"\xEF\xBB\xBFecho 1"),
            vec![("MCODE_INSTALL_DIR".into(), "C:\\m".into())],
            now(),
        )
        .unwrap();
        assert!(!f.bom_added);
        assert_eq!(f.text, "echo 1");
        let (meta, _) = staged_script(&dir.0, &f.id, &f.sha256).unwrap();
        assert_eq!(
            meta.env,
            vec![("MCODE_INSTALL_DIR".to_string(), "C:\\m".to_string())]
        );
    }

    #[test]
    fn the_status_of_an_empty_machine_offers_npm_or_says_install_node() {
        let r = Roots::default();
        let s = status_in(&r);
        assert_eq!(s.tools.len(), 6);
        assert!(s.npm.is_none());
        for t in &s.tools {
            assert!(t.installs.is_empty());
            assert!(t.update.is_none(), "没装就没有升级");
            assert_eq!(t.install.kind, StepKind::Manual);
        }
        let hermes = s.tools.iter().find(|t| t.tool == CliTool::Hermes).unwrap();
        assert!(hermes
            .script_url
            .as_deref()
            .unwrap()
            .ends_with("install.ps1"));
        assert!(hermes.npm_package.is_none());
    }
}
