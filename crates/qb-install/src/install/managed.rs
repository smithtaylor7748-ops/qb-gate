//! 面板托管安装（v0.9.0）：Claude Code 与 Codex 装进面板说了算的目录。
//!
//! # 为什么要有这个
//!
//! 「Claude 装在哪」有十几种落点（`inventory.rs` 那张表），那是被动适配 ——
//! 使用者自己怎么装的，面板就去哪儿找。找漏一处就是「面板找不到软件」。
//! 这里反过来：**面板自己装的，就装在面板指定的目录里**，位置由面板说了算。
//!
//! | 软件 | 装到 | 来源 | 校验 |
//! |---|---|---|---|
//! | Claude Code | `<根>\claude-code\claude.exe` | `downloads.claude.ai/claude-code-releases`（官方安装脚本用的同一个源） | `manifest.json` 里的 SHA-256 + Authenticode 主体含 Anthropic |
//! | Codex | `<根>\codex\codex.exe` | `github.com/openai/codex` 的 latest release | GitHub 给的资产 SHA-256 + Authenticode 主体含 OpenAI |
//! | Claude 桌面端 | **不接管** | winget（官方 Squirrel 安装器） | 见 `winget.rs` |
//!
//! 桌面端为什么不接管：它的官方安装器把位置写死在 `%LOCALAPPDATA%\AnthropicClaude`，
//! 而且会自己在那里更新。面板改不了它的落点，也就不假装改了。
//!
//! ## 为什么不走 winget 的 `--location`
//!
//! winget 上这两个包都是 portable，理论上能 `--location`。但实测：
//!
//! * winget 源里的版本落后（2026-09-11：Claude Code 2.1.263 vs 官方 2.1.268，
//!   Codex 0.146.1 vs 0.154.0）；
//! * 它下的是**同一个 URL**（winget manifest 的 Installer Url 就是 downloads.claude.ai /
//!   github.com），官方源连不上时 winget 也连不上，当兜底没有意义；
//! * 它会在托管目录之外留下 `WinGet\Links` 的 shim 和卸载登记 —— 跟「位置由面板说了算」相反。
//!
//! 所以直接从官方源下，自己核对官方给的 SHA-256 与数字签名。
//!
//! # 根目录
//!
//! 默认 `%LOCALAPPDATA%\ClaudeIpGate\apps`，可以在安装时 / 设置里改（`settings::managed_apps_dir`）。
//! 改之前 [`probe_dir`] 会**当场实测**新目录锁不锁得住：非 NTFS 的盘（exFAT/FAT32）没有权限系统，
//! 没有改权限资格的目录（Program Files、网络盘）加不上 Deny —— 这两种都当场拒绝并说清原因，
//! 不让「装进去之后才发现锁不上」发生。
//!
//! # 单测
//!
//! 下载、签名、进程这些真动作不在单测里跑（档案 §1 第 8 条）。可测的都做成了纯函数：
//! 官方清单解析、GitHub 发布解析、放置与留底、迁移、外部副本的清单规划 —— 全部在临时目录里。

use crate::error::{GateError, Result};
use crate::sink::ProgressSink;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Claude Code 官方发布源。官方安装脚本 `install.ps1` 用的就是它。
pub const CLAUDE_RELEASES: &str = "https://downloads.claude.ai/claude-code-releases";
/// Codex 的 GitHub 发布接口。
pub const CODEX_LATEST_API: &str = "https://api.github.com/repos/openai/codex/releases/latest";
/// Codex 资产的下载地址必须以这个开头 —— 不跟着 JSON 里随便一个 URL 走。
pub const CODEX_DOWNLOAD_PREFIX: &str = "https://github.com/openai/codex/releases/download/";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "ManagedApp")]
#[serde(rename_all = "kebab-case")]
pub enum App {
    ClaudeCode,
    Codex,
}

impl App {
    pub const ALL: [App; 2] = [App::ClaudeCode, App::Codex];

    pub fn key(self) -> &'static str {
        match self {
            App::ClaudeCode => "claude-code",
            App::Codex => "codex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            App::ClaudeCode => "Claude Code",
            App::Codex => "Codex CLI",
        }
    }

    pub fn exe_name(self) -> &'static str {
        match self {
            App::ClaudeCode => "claude.exe",
            App::Codex => "codex.exe",
        }
    }

    /// Authenticode 签名主体里必须出现的词。
    pub fn signer(self) -> &'static str {
        match self {
            App::ClaudeCode => "anthropic",
            App::Codex => "openai",
        }
    }

    /// 替换时旧文件的留底名前缀。
    ///
    /// Claude Code 那个**必须是 `claude.exe.old.`** —— 跟 `inventory::stale_copies` 认的一致，
    /// 名字一漂，这份没有 Deny 的完整 exe 就永远不会被清掉（档案 §7.9）。
    fn backup_prefix(self) -> &'static str {
        match self {
            App::ClaudeCode => "claude.exe.old.",
            App::Codex => "codex.exe.old.",
        }
    }
}

pub fn root() -> PathBuf {
    crate::settings::managed_apps_dir()
}

pub fn app_dir(root: &Path, app: App) -> PathBuf {
    root.join(app.key())
}

pub fn exe_in(root: &Path, app: App) -> PathBuf {
    app_dir(root, app).join(app.exe_name())
}

/// 当前根目录下这个软件的 exe（在不在都返回路径）。
pub fn exe(app: App) -> PathBuf {
    exe_in(&root(), app)
}

pub fn is_installed(app: App) -> bool {
    exe(app).is_file()
}

// ---------------------------------------------------------------- 记录

/// 面板装进去的是哪个版本、哈希多少、从哪来。
///
/// Codex 的 exe 里**没有版本资源**（实测 `VersionInfo.ProductVersion` 是空的），
/// 不记下来就只能运行它去问 —— 而它归门禁管时是锁着的。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Record {
    pub version: String,
    pub sha256: String,
    pub source: String,
    pub installed_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Records {
    #[serde(default)]
    apps: std::collections::BTreeMap<String, Record>,
}

fn records_path(root: &Path) -> PathBuf {
    root.join("installs.json")
}

fn read_records(root: &Path) -> Records {
    std::fs::read_to_string(records_path(root))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn record_of(root: &Path, app: App) -> Option<Record> {
    read_records(root).apps.get(app.key()).cloned()
}

pub(crate) fn remove_record(root: &Path, app: App) -> Result<()> {
    let p = records_path(root);
    let text = crate::config_io::read_text(&p)?;
    let mut all: Records = if text.is_empty() {
        Default::default()
    } else {
        serde_json::from_str(&text)?
    };
    all.apps.remove(app.key());
    crate::config_io::replace(&p, Some(&serde_json::to_vec_pretty(&all)?))
}
pub(crate) fn write_record(root: &Path, app: App, rec: Record) -> Result<()> {
    let p = records_path(root);
    let text = crate::config_io::read_text(&p)?;
    let mut all: Records = if text.is_empty() {
        Records::default()
    } else {
        serde_json::from_str(&text)?
    };
    all.apps.insert(app.key().to_string(), rec);
    crate::config_io::replace(&p, Some(&serde_json::to_vec_pretty(&all)?))
}

// ---------------------------------------------------------------- 状态

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, rename = "ManagedAppStatus")]
pub struct AppStatus {
    pub app: App,
    pub path: PathBuf,
    pub installed: bool,
    pub version: Option<String>,
    pub installed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, rename = "ManagedStatus")]
pub struct Status {
    pub root: PathBuf,
    pub default_root: PathBuf,
    pub is_default: bool,
    pub apps: Vec<AppStatus>,
}

pub fn status() -> Status {
    let root = root();
    let default_root = crate::settings::default_managed_dir();
    let apps = App::ALL
        .into_iter()
        .map(|a| {
            let path = exe_in(&root, a);
            let installed = path.is_file();
            let rec = installed.then(|| record_of(&root, a)).flatten();
            AppStatus {
                app: a,
                installed,
                version: rec.as_ref().map(|r| r.version.clone()),
                installed_at: rec.map(|r| r.installed_at),
                path,
            }
        })
        .collect();
    Status {
        is_default: crate::install::inventory::same_path(&root, &default_root),
        root,
        default_root,
        apps,
    }
}

// ---------------------------------------------------------------- 目录实测

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, rename = "ManagedProbe")]
pub struct Probe {
    pub ok: bool,
    pub path: PathBuf,
    pub filesystem: Option<String>,
    pub reason: Option<String>,
}

/// 这个目录不能拿来当托管根目录的理由（跟锁不锁得住无关的那些）。纯函数。
///
/// * 面板自己的安装目录 —— 面板升级 / 卸载时安装器会动它；
/// * 桌面端的地盘（`%APPDATA%\Claude*`、`%LOCALAPPDATA%\AnthropicClaude`）——
///   `<根>\claude-code\claude.exe` 放进 `%APPDATA%\Claude-x\` 下，会跟桌面端自带的
///   `claude-code\<版本>\` 布局撞在一起；
/// * `~\.claude`（登录与配置）、`~\.local`（官方安装器的地盘，清理外部副本时会删里面的东西）。
pub fn forbidden_reason(
    dir: &Path,
    home: Option<&Path>,
    roaming: Option<&Path>,
    local: Option<&Path>,
    panel_dir: Option<&Path>,
) -> Option<String> {
    use crate::install::inventory::norm;
    let d = format!("{}\\", norm(dir).trim_end_matches('\\'));
    let under = |base: &Path| d.starts_with(&format!("{}\\", norm(base).trim_end_matches('\\')));

    if let Some(p) = panel_dir {
        if under(p) {
            return Some("这是面板自己的安装目录，面板升级或卸载时会动它，换一个目录。".into());
        }
    }
    if let Some(l) = local {
        if under(&l.join("AnthropicClaude")) {
            return Some("这是 Claude 桌面端的安装目录，换一个目录。".into());
        }
    }
    if let Some(r) = roaming {
        let rr = format!("{}\\", norm(r).trim_end_matches('\\'));
        if let Some(rest) = d.strip_prefix(&rr) {
            let first = rest.split('\\').next().unwrap_or("");
            if first == "claude" || first.starts_with("claude-") {
                return Some("这是 Claude 桌面端的资料目录，换一个目录。".into());
            }
        }
    }
    if let Some(h) = home {
        if under(&h.join(".claude")) {
            return Some("这是 Claude Code 的登录与配置目录，换一个目录。".into());
        }
        if under(&h.join(".local")) {
            return Some("~\\.local 是官方安装器的地盘（清理外部副本时会删里面的 Claude 文件），换一个目录。".into());
        }
    }
    None
}

/// **当场实测**这个目录能不能当托管根目录：建得出来、写得进去、锁得上也解得开。
///
/// 这是「改目录之后锁不上」的修法 —— 不是在锁不上之后想办法，而是根本不让选进来。
pub fn probe_dir(dir: &Path) -> Probe {
    let fail = |fs: Option<String>, reason: String| Probe {
        ok: false,
        path: dir.to_path_buf(),
        filesystem: fs,
        reason: Some(reason),
    };

    if !dir.is_absolute() {
        return fail(None, "要一个完整路径，例如 D:\\ClaudeApps。".into());
    }
    let panel_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    if let Some(why) = forbidden_reason(
        dir,
        dirs::home_dir().as_deref(),
        dirs::config_dir().as_deref(),
        dirs::data_local_dir().as_deref(),
        panel_dir.as_deref(),
    ) {
        return fail(None, why);
    }
    if let Err(e) = std::fs::create_dir_all(dir) {
        return fail(None, format!("建不了这个目录：{e}"));
    }

    probe_lock(dir, fail)
}

#[cfg(windows)]
fn probe_lock(dir: &Path, fail: impl Fn(Option<String>, String) -> Probe) -> Probe {
    use crate::acl;

    let fs = acl::volume_filesystem(dir);
    if let Some((name, false)) = &fs {
        return fail(
            Some(name.clone()),
            format!(
                "这个盘是 {name}，没有 Windows 的权限系统，执行锁加不上去。\
                 请选一个 NTFS 盘（通常就是 C 盘）上的目录。"
            ),
        );
    }
    let fs_name = fs.map(|(n, _)| n);

    let probe = dir.join(format!(".qbgate-lock-probe-{}.tmp", std::process::id()));
    if let Err(e) = std::fs::write(&probe, b"MZ probe") {
        return fail(fs_name, format!("这个目录写不进去：{e}"));
    }
    let round_trip = (|| -> Result<(bool, bool)> {
        let sid = acl::current_user_sid()?;
        acl::lock(&probe, &sid)?;
        let locked = acl::is_locked(&probe, &sid)?;
        acl::unlock(&probe, &sid)?;
        let still = acl::is_locked(&probe, &sid)?;
        Ok((locked, still))
    })();
    let _ = std::fs::remove_file(&probe);

    match round_trip {
        Ok((true, false)) => Probe {
            ok: true,
            path: dir.to_path_buf(),
            filesystem: fs_name,
            reason: None,
        },
        Ok((false, _)) => fail(
            fs_name,
            "锁加上去之后查不到 —— 这个目录的权限不归当前账户管（网络盘、共享盘常见这样）。\
             换一个本机 NTFS 盘上、你自己的目录。"
                .into(),
        ),
        Ok((true, true)) => fail(
            fs_name,
            "锁加得上但解不开 —— 选进来的话面板会打不开这里的软件。换一个你自己的目录。".into(),
        ),
        Err(e) => fail(
            fs_name,
            format!(
                "当前账户没有改这个目录权限的资格（{e}）。Program Files、网络盘常见这样 —— \
                 换一个你自己的目录，比如 %LOCALAPPDATA% 下面。"
            ),
        ),
    }
}

#[cfg(not(windows))]
fn probe_lock(dir: &Path, _fail: impl Fn(Option<String>, String) -> Probe) -> Probe {
    Probe {
        ok: true,
        path: dir.to_path_buf(),
        filesystem: None,
        reason: None,
    }
}

// ---------------------------------------------------------------- 官方源

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
    /// 官方给的 SHA-256（小写十六进制）。Codex 那边 GitHub 偶尔不给，那时是 `None`。
    pub sha256: Option<String>,
    pub size: Option<u64>,
}

/// `2.1.268` / `2.1.268-beta.1` 这种。官方安装脚本也做这一步：
/// 服务在某些地区不可用时返回的是 HTML 页面，不先挡住就会拿去拼 URL。
pub fn looks_like_version(s: &str) -> bool {
    let core = s.split('-').next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// 本机是不是 ARM64。64 位面板在 ARM64 Windows 上跑在模拟层里，
/// 进程自己看到的 `PROCESSOR_ARCHITECTURE` 可能是 AMD64 —— 那时装 x64 版，照样能跑（慢一点）。
fn is_arm64() -> bool {
    ["PROCESSOR_ARCHITEW6432", "PROCESSOR_ARCHITECTURE"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .any(|v| v.eq_ignore_ascii_case("ARM64"))
}

pub fn claude_platform(arm64: bool) -> &'static str {
    if arm64 {
        "win32-arm64"
    } else {
        "win32-x64"
    }
}

pub fn codex_asset(arm64: bool) -> &'static str {
    if arm64 {
        "codex-aarch64-pc-windows-msvc.exe.zip"
    } else {
        "codex-x86_64-pc-windows-msvc.exe.zip"
    }
}

/// 解析 Claude Code 官方 `manifest.json`。纯函数。
pub fn parse_claude_manifest(json: &str, version: &str, platform: &str) -> Result<Release> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| GateError::Other(format!("官方 manifest.json 解析不了：{e}")))?;
    let p = v
        .get("platforms")
        .and_then(|p| p.get(platform))
        .ok_or_else(|| GateError::Other(format!("官方清单里没有 {platform} 这个平台")))?;
    let sha = p
        .get("checksum")
        .and_then(|c| c.as_str())
        .map(|s| s.to_ascii_lowercase())
        .filter(|s| is_sha256_hex(s))
        .ok_or_else(|| GateError::Other("官方清单里的 checksum 不是一个 SHA-256".into()))?;
    Ok(Release {
        version: version.to_string(),
        url: format!("{CLAUDE_RELEASES}/{version}/{platform}/claude.exe"),
        sha256: Some(sha),
        size: p.get("size").and_then(|s| s.as_u64()),
    })
}

/// 解析 GitHub `releases/latest` 的回包。纯函数。
pub fn parse_codex_release(json: &str, asset: &str) -> Result<Release> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| GateError::Other(format!("GitHub 发布信息解析不了：{e}")))?;
    let tag = v
        .get("tag_name")
        .and_then(|t| t.as_str())
        .ok_or_else(|| GateError::Other("GitHub 发布信息里没有 tag_name".into()))?;
    let version = tag
        .trim_start_matches("rust-v")
        .trim_start_matches('v')
        .to_string();
    let a = v
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|list| {
            list.iter()
                .find(|x| x.get("name").and_then(|n| n.as_str()) == Some(asset))
        })
        .ok_or_else(|| GateError::Other(format!("Codex {tag} 里没有 {asset} 这个文件")))?;
    let url = a
        .get("browser_download_url")
        .and_then(|u| u.as_str())
        .filter(|u| u.starts_with(CODEX_DOWNLOAD_PREFIX))
        .ok_or_else(|| {
            GateError::Other(
                "Codex 的下载地址不在 github.com/openai/codex 下，面板不跟着它走".into(),
            )
        })?
        .to_string();
    let sha256 = a
        .get("digest")
        .and_then(|d| d.as_str())
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(|s| s.to_ascii_lowercase())
        .filter(|s| is_sha256_hex(s));
    Ok(Release {
        version,
        url,
        sha256,
        size: a.get("size").and_then(|s| s.as_u64()),
    })
}

fn http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .user_agent(concat!("QB Gate/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| GateError::Other(format!("建不了网络客户端：{e}")))
}

async fn get_text(c: &reqwest::Client, url: &str) -> Result<String> {
    let r = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        c.get(url)
            .header(
                "Accept",
                "application/vnd.github+json, application/json, text/plain",
            )
            .send()
            .await?
            .error_for_status()?
            .text()
            .await
    })
    .await
    .map_err(|_| GateError::Other(format!("访问 {url} 超过 60 秒没有回应")))?;
    r.map_err(|e| GateError::Other(format!("访问 {url} 失败：{e}")))
}

async fn claude_release(channel: &str) -> Result<Release> {
    let c = http()?;
    let version = get_text(&c, &format!("{CLAUDE_RELEASES}/{channel}"))
        .await?
        .trim()
        .to_string();
    if !looks_like_version(&version) {
        return Err(GateError::Other(
            "官方发布源回的不是版本号（多半是当前网络所在地区访问不了 downloads.claude.ai）。"
                .into(),
        ));
    }
    let manifest = get_text(&c, &format!("{CLAUDE_RELEASES}/{version}/manifest.json")).await?;
    parse_claude_manifest(&manifest, &version, claude_platform(is_arm64()))
}

async fn codex_release() -> Result<Release> {
    let c = http()?;
    let json = get_text(&c, CODEX_LATEST_API).await?;
    parse_codex_release(&json, codex_asset(is_arm64()))
}

/// 下载到 `dest`，边下边算 SHA-256。返回算出来的哈希（小写十六进制）。
///
/// 60 秒一个字节都没收到就放弃 —— 不设这个的话，网络断在半路会让任务永远挂在「下载中」。
async fn download(
    url: &str,
    dest: &Path,
    size_hint: Option<u64>,
    rep: &dyn ProgressSink,
    step: u32,
) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Write;

    let c = http()?;
    let mut resp = c
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| GateError::Other(format!("下载 {url} 失败：{e}")))?;
    let total = resp.content_length().or(size_hint);
    let mut file = std::fs::File::create(dest)?;
    let mut hasher = Sha256::new();
    let mut got: u64 = 0;
    let mut last_pct: i64 = -1;
    loop {
        let chunk = tokio::time::timeout(std::time::Duration::from_secs(60), resp.chunk())
            .await
            .map_err(|_| GateError::Other("下载卡住，60 秒没有收到任何数据，已停止".into()))?
            .map_err(|e| GateError::Other(format!("下载中断：{e}")))?;
        let Some(chunk) = chunk else { break };
        file.write_all(&chunk)?;
        hasher.update(&chunk);
        got += chunk.len() as u64;
        if let Some(t) = total.filter(|t| *t > 0) {
            let pct = (got * 100 / t) as i64;
            if pct >= last_pct + 5 {
                last_pct = pct;
                rep.phase(
                    step,
                    &format!("下载中 {pct}%（{} / {} MB）", got >> 20, t >> 20),
                );
            }
        }
    }
    file.flush()?;
    if let Some(t) = total {
        if got != t {
            return Err(GateError::Other(format!(
                "下载不完整：收到 {got} 字节，应为 {t} 字节"
            )));
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

/// 解压 Codex 的 zip。用 .NET 的 ZipFile（PowerShell 5.1 自带），不为它加一个 Rust 依赖。
/// 路径按 PowerShell 单引号串的规矩转义（`'` → `''`），脚本里没有双引号。
#[cfg(windows)]
async fn unzip(zip: &Path, out: &Path) -> Result<()> {
    let q = |p: &Path| p.display().to_string().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference = 'Stop'; Add-Type -AssemblyName System.IO.Compression.FileSystem; \
         [System.IO.Compression.ZipFile]::ExtractToDirectory('{}', '{}')",
        q(zip),
        q(out)
    );
    let o = crate::process::hidden_tokio(tokio::process::Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .await
        .map_err(|e| GateError::Other(format!("启动 PowerShell 解压失败：{e}")))?;
    if !o.status.success() {
        return Err(GateError::Other(format!(
            "解压 {} 失败：{}",
            zip.display(),
            String::from_utf8_lossy(&o.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
async fn unzip(_zip: &Path, _out: &Path) -> Result<()> {
    Err(GateError::Other("只在 Windows 上可用".into()))
}

/// 解压目录里的全部 exe（往下最多两层）。
fn exes_in(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0u32)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth < 2 {
                    stack.push((p, depth + 1));
                }
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// 分出 Codex 主程序和辅助程序。纯函数。
///
/// zip 里不止一个 exe（winget 清单实测）：主程序 `codex-<架构>-pc-windows-msvc.exe`，
/// 外加 `codex-command-runner.exe` 与 `codex-windows-sandbox-setup.exe` —— 后两个是
/// Codex 的 Windows 沙箱要用的，**得跟主程序放在同一个目录**，只装主程序沙箱就起不来。
pub fn split_codex_exes(all: &[PathBuf]) -> Option<(PathBuf, Vec<PathBuf>)> {
    let is_main = |p: &PathBuf| {
        let n = p
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        n.starts_with("codex-") && n.ends_with("-pc-windows-msvc.exe")
    };
    let main = all.iter().find(|p| is_main(p))?.clone();
    let helpers = all.iter().filter(|p| !is_main(p)).cloned().collect();
    Some((main, helpers))
}

pub(crate) fn sha256_file(p: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(p).ok()?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h).ok()?;
    Some(hex::encode(h.finalize()))
}

/// 把核对过的 `src` 放到 `target`：旧的改名留底，新的改名就位，再算一次哈希。
///
/// 跟 `upgrade::finish_install` 同一套闸（留底不删、放完再核、对不上回滚），
/// 只是这里 `src` 与 `target` 在同一个目录树里，放置是一次改名，不是复制。
/// 返回留底的那份（原来没有就是 `None`）。纯文件操作，可单测。
pub fn place(app: App, src: &Path, target: &Path, want_sha: &str) -> Result<Option<PathBuf>> {
    let backup = if target.exists() {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S%3f");
        let b = target.with_file_name(format!("{}{stamp}", app.backup_prefix()));
        std::fs::rename(target, &b).map_err(|e| {
            GateError::Other(format!(
                "旧的 {} 改不了名（多半正在运行）：{e}",
                target.display()
            ))
        })?;
        Some(b)
    } else {
        None
    };
    let restore = |why: String| -> GateError {
        let _ = std::fs::remove_file(target);
        if let Some(b) = &backup {
            let _ = std::fs::rename(b, target);
        }
        GateError::Other(format!("{why}。已把旧的放回去。"))
    };
    if let Err(e) = std::fs::rename(src, target) {
        return Err(restore(format!("放不进 {}：{e}", target.display())));
    }
    match sha256_file(target) {
        Some(h) if h.eq_ignore_ascii_case(want_sha) => Ok(backup),
        other => Err(restore(format!(
            "放进去之后哈希对不上（应为 {want_sha}，实际 {}）",
            other.unwrap_or_else(|| "读不出".into())
        ))),
    }
}

/// 给留底加执行锁。
///
/// 留底是一份完整可执行的旧版本，门禁清单不收它（`lock_all` 只锁在册的副本）。改名会带着
/// 原来的 ACL 走，可租约开着时原来那份本来就是解开的 —— 不单独锁，它在「下次安装时清掉」
/// 之前就是现成的绕过入口。已经在跑的进程不受影响，之后删掉它也不受影响。
/// 同一目录里上一次留下的 `*.exe.old.*`。删不掉（正被占用）就留到下一次。
fn clean_old_backups(app: App, dir: &Path) -> Vec<String> {
    let mut notes = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return notes;
    };
    for e in rd.flatten() {
        let p = e.path();
        let n = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if n.starts_with(app.backup_prefix()) && std::fs::remove_file(&p).is_ok() {
            notes.push(format!("清掉上次的留底 {}", p.display()));
        }
    }
    notes
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallOutcome {
    pub app: App,
    pub version: String,
    pub path: PathBuf,
    pub sha256: String,
    pub backup: Option<PathBuf>,
    pub signer: String,
    pub log: Vec<String>,
}

/// 安装进度的段数。前端进度条按它画。
pub const TOTAL: u32 = 7;

/// 下载官方版本、核对、放进托管目录。**重锁由调用方的维护窗口收尾时做**
/// （`lib.rs` 里 `Maintenance::observing` → `finish`），这里只管文件。
///
/// 已经装了就是升级：同一条路，旧的改名留底。
pub async fn install(app: App, channel: &str, rep: &dyn ProgressSink) -> Result<InstallOutcome> {
    let mut log = Vec::new();
    let root = root();

    rep.phase(1, "实测托管目录能不能上锁");
    let probe = probe_dir(&root);
    if !probe.ok {
        return Err(GateError::Other(format!(
            "托管目录 {} 不能用：{}",
            root.display(),
            probe.reason.unwrap_or_default()
        )));
    }
    let dir = app_dir(&root, app);
    std::fs::create_dir_all(&dir)?;
    log.extend(clean_old_backups(app, &dir));
    let dl = dir.join(".download");
    let _ = std::fs::remove_dir_all(&dl);
    std::fs::create_dir_all(&dl)?;

    rep.phase(2, "查官方最新版本");
    let rel = match app {
        App::ClaudeCode => claude_release(channel).await?,
        App::Codex => codex_release().await?,
    };
    log.push(format!("官方版本 {}：{}", rel.version, rel.url));
    rep.log(2, &format!("{} {}", app.label(), rel.version));

    rep.phase(3, &format!("下载 {} {}", app.label(), rel.version));
    let file_name = rel
        .url
        .rsplit('/')
        .next()
        .unwrap_or("download.bin")
        .to_string();
    let file = dl.join(&file_name);
    let got = download(&rel.url, &file, rel.size, rep, 3).await?;

    rep.phase(4, "核对官方给的 SHA-256");
    match &rel.sha256 {
        Some(want) if want.eq_ignore_ascii_case(&got) => {
            log.push(format!("SHA-256 与官方一致：{got}"));
        }
        Some(want) => {
            let _ = std::fs::remove_dir_all(&dl);
            return Err(GateError::Other(format!(
                "下载下来的文件跟官方给的 SHA-256 对不上（应为 {want}，实际 {got}），已删除，没有装。"
            )));
        }
        None => log.push("GitHub 没给这个文件的 SHA-256，只能靠下面的数字签名核对".into()),
    }

    rep.phase(5, "核对数字签名");
    let (bin, helpers) = match app {
        App::ClaudeCode => (file.clone(), Vec::new()),
        App::Codex => {
            let out = dl.join("extract");
            std::fs::create_dir_all(&out)?;
            unzip(&file, &out).await?;
            split_codex_exes(&exes_in(&out))
                .ok_or_else(|| GateError::Other("解压出来的东西里找不到 Codex 主程序".into()))?
        }
    };
    let signer = crate::signature::signer_of(&bin).await;
    match crate::install::winget::signature_matches(signer.as_deref(), app.signer()) {
        Some(true) => log.push(format!("签名主体：{}", signer.clone().unwrap_or_default())),
        Some(false) => {
            let _ = std::fs::remove_dir_all(&dl);
            return Err(GateError::Other(format!(
                "拒绝安装：签名主体里没有 {}（读到的是 {}）。",
                app.signer(),
                signer.unwrap_or_default()
            )));
        }
        None => {
            let _ = std::fs::remove_dir_all(&dl);
            return Err(GateError::Other(
                "拒绝安装：读不出数字签名 —— 这只能说「没验成」，面板不会把没验成的程序放进启动路径。".into(),
            ));
        }
    }
    let bin_sha =
        sha256_file(&bin).ok_or_else(|| GateError::Other("算不出下载文件的 SHA-256".into()))?;

    // Verify the entire release before changing any member of the current installation.
    let mut unit = vec![(app.exe_name().to_string(), bin.clone())];
    for h in &helpers {
        let signature = crate::signature::signer_of(h).await;
        if crate::install::winget::signature_matches(signature.as_deref(), app.signer())
            != Some(true)
        {
            return Err(GateError::Other(format!(
                "辅助程序 {} 签名无法确认，整个安装已阻断",
                h.display()
            )));
        }
        unit.push((
            h.file_name().unwrap().to_string_lossy().to_string(),
            h.clone(),
        ));
    }
    if app == App::Codex
        && ![
            "codex-command-runner.exe",
            "codex-windows-sandbox-setup.exe",
        ]
        .iter()
        .all(|name| unit.iter().any(|(n, _)| n == name))
    {
        return Err(GateError::Other(
            "Codex 发布包缺少 Windows 配套程序，未替换当前安装".into(),
        ));
    }
    rep.phase(6, "提交完整安装单元");
    let target = exe_in(&root, app);
    let previous = record_of(&root, app);
    let backup = previous
        .as_ref()
        .map(|r| crate::install::versions::version_dir(&dir, &r.version).join(app.exe_name()));
    crate::install::versions::deploy(
        &dir,
        app,
        unit,
        Record {
            version: rel.version.clone(),
            sha256: bin_sha.clone(),
            source: rel.url.clone(),
            installed_at: chrono::Utc::now().to_rfc3339(),
        },
        previous.as_ref(),
    )?;
    let _ = std::fs::remove_dir_all(&dl);
    log.push("主程序、配套文件与安装记录已一起提交".into());

    for n in crate::install::versions::prune(
        &dir,
        app,
        Some(rel.version.as_str()),
        crate::install::versions::KEEP,
    ) {
        log.push(n);
    }

    rep.phase(7, "重新上锁");
    Ok(InstallOutcome {
        app,
        version: rel.version,
        path: target,
        sha256: bin_sha,
        backup,
        signer: signer.unwrap_or_default(),
        log,
    })
}

// ---------------------------------------------------------------- 迁移

/// `a` 在 `b` 里面（或者就是 `b`）吗。大小写与斜杠不敏感。
pub fn is_inside(a: &Path, b: &Path) -> bool {
    use crate::install::inventory::norm;
    let a = format!("{}\\", norm(a).trim_end_matches('\\'));
    let b = format!("{}\\", norm(b).trim_end_matches('\\'));
    a.starts_with(&b)
}

/// 把一个目录搬到另一个位置：同一个盘上是一次改名；改名不成就复制、逐个核对、再删源。
///
/// **失败时恢复原样。** 两边各留一半是最坏的结果：设置指着一边，另一边的 exe
/// 在面板不认识的位置上，没人找得到也没人锁（`inventory` 只认设置里那个根目录）。
///
/// 改名不成的最常见原因是目录里有文件开着 —— 实测 NTFS 上目录里任何一个文件有句柄，
/// 连允许删除共享的句柄也算，改名目录就拒绝。正在运行的 exe 还删不掉，所以
/// 「复制完删源」会删到一半停下：这时用目标里核对过的那份把源补齐，再撤掉目标。
fn move_tree(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    if let Err(e) = copy_verified(src, dst) {
        // 目标是空的才会走到这里（`migrate_files` 事先查过），复制了一半的都是我们的。
        let _ = std::fs::remove_dir_all(dst);
        return Err(e);
    }
    if let Err(e) = std::fs::remove_dir_all(src) {
        return Err(GateError::Other(match restore_missing(dst, src) {
            Ok(()) => {
                let _ = std::fs::remove_dir_all(dst);
                format!("{} 里有文件正被占着（多半有程序正从这里运行），没搬，已恢复原样：{e}", src.display())
            }
            // 补不回来就不能删目标 —— 那是被删掉的那些文件唯一完整的一份。
            Err(r) => format!(
                "{} 里有文件正被占着，删到一半停下；从 {} 补回时又失败了（{r}），两处都留着没动：{e}",
                src.display(),
                dst.display()
            ),
        }));
    }
    Ok(())
}

/// `from` 里有、`to` 里没有的文件补过去。已经在的不碰 —— 被占着删不掉的那个就是还在的。
fn restore_missing(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let (a, b) = (e.path(), to.join(e.file_name()));
        if a.is_dir() {
            restore_missing(&a, &b)?;
        } else if !b.exists() {
            std::fs::copy(&a, &b)?;
        }
    }
    Ok(())
}

fn copy_verified(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let from = e.path();
        let to = dst.join(e.file_name());
        if from.is_dir() {
            copy_verified(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
            let same = std::fs::metadata(&from)?.len() == std::fs::metadata(&to)?.len()
                && sha256_file(&from) == sha256_file(&to);
            if !same {
                return Err(GateError::Other(format!(
                    "复制 {} 之后内容对不上",
                    from.display()
                )));
            }
        }
    }
    Ok(())
}

/// 把旧根目录下面板托管的东西搬到新根目录。纯文件操作，可单测。
///
/// **要么全搬过去，要么全留在原处。** 先把每个软件都查一遍（新目录里已经有同一个软件就拒绝，
/// 不覆盖别人的东西），再复制安装记录，然后逐个搬；中途哪个搬不动，已经搬过去的搬回来。
/// 旧根目录搬空了就删掉，里面还有别的文件就留着 —— 那可能是使用者自己放的。
pub fn migrate_files(old: &Path, new: &Path) -> Result<Vec<String>> {
    let mut notes = Vec::new();
    std::fs::create_dir_all(new)?;
    let apps: Vec<App> = App::ALL
        .into_iter()
        .filter(|a| app_dir(old, *a).exists())
        .collect();
    for app in &apps {
        let dst = app_dir(new, *app);
        if dst.exists()
            && std::fs::read_dir(&dst)
                .map(|mut r| r.next().is_some())
                .unwrap_or(false)
        {
            return Err(GateError::Other(format!(
                "新目录里已经有 {}（{}），面板不覆盖它。换一个空目录，或者先把它挪走。",
                app.label(),
                dst.display()
            )));
        }
    }
    // 安装记录先复制过去：这一步失败时什么都还没动。
    let (rec_old, rec_new) = (records_path(old), records_path(new));
    let has_rec = rec_old.is_file();
    if has_rec {
        std::fs::copy(&rec_old, &rec_new)?;
    }
    let mut moved: Vec<App> = Vec::new();
    for app in &apps {
        let (src, dst) = (app_dir(old, *app), app_dir(new, *app));
        let _ = std::fs::remove_dir(&dst);
        if let Err(e) = move_tree(&src, &dst) {
            let stuck: Vec<String> = moved
                .iter()
                .rev()
                .filter_map(|m| {
                    move_tree(&app_dir(new, *m), &app_dir(old, *m))
                        .err()
                        .map(|b| format!("{}（{b}）", m.label()))
                })
                .collect();
            if has_rec {
                let _ = std::fs::remove_file(&rec_new);
            }
            return Err(GateError::Other(if stuck.is_empty() {
                format!(
                    "{} 没搬成，已经全部恢复原样，托管目录没有改：{e}",
                    app.label()
                )
            } else {
                format!(
                    "{} 没搬成：{e}。回滚时 {} 没能搬回旧目录，留在了 {}",
                    app.label(),
                    stuck.join("、"),
                    new.display()
                )
            }));
        }
        moved.push(*app);
        notes.push(format!("{} 已搬到 {}", app.label(), dst.display()));
    }
    if has_rec {
        let _ = std::fs::remove_file(&rec_old);
    }
    if std::fs::remove_dir(old).is_ok() {
        notes.push(format!("旧目录 {} 已空，已删掉", old.display()));
    }
    Ok(notes)
}

/// 迁移失败、回滚也没回全时，新目录里留下的文件一个个加上执行锁。
///
/// 它们不在设置指的根目录下，`inventory` 不认，重锁也就轮不到它们 —— 不锁就是现成的绕过入口。
/// 尽力而为，返回锁上了几个。迁移前新目录里的软件目录是空的（`migrate_files` 查过），
/// 所以这里碰到的只可能是我们搬过去的东西。
#[cfg(windows)]
pub fn lock_strays(new: &Path) -> usize {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
    let Ok(sid) = crate::acl::current_user_sid() else {
        return 0;
    };
    let mut files = Vec::new();
    for app in App::ALL {
        walk(&app_dir(new, app), &mut files);
    }
    files
        .iter()
        .filter(|f| crate::acl::lock(f, &sid).is_ok())
        .count()
}

#[cfg(not(windows))]
pub fn lock_strays(_new: &Path) -> usize {
    0
}

/// 正从 `dir` 里运行着的进程（PID 与路径）。
///
/// 迁移之前要先知道：正在运行的 exe 搬不动。只列不杀 —— Claude 由调用方走一键关闭那套证据去关，
/// 别的（比如 Codex）面板不按进程名杀，交给使用者自己关。
///
/// 脚本是常量，不拼任何路径；过滤在 Rust 这边做。**枚举失败返回错误**，不当成「没有」。
#[cfg(windows)]
pub async fn running_from(dir: &Path) -> Result<Vec<(u32, String)>> {
    const SCRIPT: &str = "$ErrorActionPreference = 'Stop'; \
        try { [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false } catch { }; \
        $rows = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath } | \
          ForEach-Object { [pscustomobject]@{ Id = [int]$_.ProcessId; Path = [string]$_.ExecutablePath } }); \
        ConvertTo-Json -InputObject $rows -Compress";
    let out = crate::process::hidden_tokio(tokio::process::Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .output()
        .await
        .map_err(|e| GateError::Other(format!("启动 PowerShell 枚举进程失败：{e}")))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let rows: Vec<serde_json::Value> = serde_json::from_str(text.trim()).map_err(|e| {
        GateError::Other(format!(
            "枚举进程失败（{e}）：{}",
            String::from_utf8_lossy(&out.stderr)
                .trim()
                .chars()
                .take(200)
                .collect::<String>()
        ))
    })?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let id = u32::try_from(r.get("Id")?.as_u64()?).ok()?;
            let path = r.get("Path")?.as_str()?.to_string();
            is_inside(Path::new(&path), dir).then_some((id, path))
        })
        .collect())
}

#[cfg(not(windows))]
pub async fn running_from(_dir: &Path) -> Result<Vec<(u32, String)>> {
    Ok(Vec::new())
}

// ---------------------------------------------------------------- 外部副本

/// 怎么清掉一份外部副本。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "ExternalMethod")]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// `npm uninstall -g <包>` —— 连 `.cmd` 启动器和包目录一起走，npm 的账本也对得上。
    Npm,
    /// `winget uninstall --id <包>` —— 连 `WinGet\Links` 的 shim 和卸载登记一起走。
    Winget,
    /// `scoop uninstall <包>`。
    Scoop,
    /// 删文件：官方安装器那份、它的版本库、安装器缓存、PATH 上的、升级残留。
    Delete,
    /// 删注册表里的卸载登记（文件早就不在了，登记还挂着）。
    Registry,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, rename = "ManagedExternal")]
pub struct External {
    pub app: App,
    pub method: Method,
    /// 要删的文件，或者要卸载的包名 / 登记项名。
    pub target: String,
    /// 给人看的一句话：会做什么。
    pub action: String,
}

/// 规划 Claude Code 的外部副本清理。**纯函数**：输入 inventory 的扫描结果，不碰文件系统。
///
/// 清的只是**可执行安装**。桌面端存根与它带的副本、编辑器扩展里的副本属于别的程序；
/// 托管那份是要留下的；账户凭证与配置（`~\.claude`、`claude-profile-*`）根本不在这张表里 ——
/// inventory 只收 exe。
pub fn plan_claude_externals(
    installs: &[crate::install::inventory::Install],
    stale: &[PathBuf],
    managed_root: &Path,
    roaming: Option<&Path>,
) -> Vec<External> {
    use crate::install::inventory::Kind;
    let mut out = Vec::new();
    let mut npm = false;
    let mut winget = false;
    let mut scoop = false;
    for i in installs {
        if is_inside(&i.path, managed_root) {
            continue;
        }
        match i.kind {
            Kind::Npm | Kind::NpmNative => npm = true,
            Kind::Winget => winget = true,
            Kind::Scoop => scoop = true,
            Kind::Native | Kind::NativeVersion | Kind::Programs | Kind::Path => {
                out.push(External {
                    app: App::ClaudeCode,
                    method: Method::Delete,
                    target: i.path.display().to_string(),
                    action: format!("删除 {}", i.path.display()),
                })
            }
            // 别的程序的，或者就是托管那份（含版本库里留给回滚用的旧版本 ——
            // 它们在托管根下面，上面那个 is_inside 已经跳过了，这里只是把
            // 这个分支写明白：**清外部副本永远不碰版本库**，碰了回滚就没了）。
            Kind::Managed
            | Kind::ManagedVersion
            | Kind::DesktopManaged
            | Kind::MsixManaged
            | Kind::Editor
            | Kind::DesktopStub => {}
        }
    }
    for s in stale {
        // 桌面端资料目录里的残留是桌面端自己的事，不替它清。
        if !is_inside(s, managed_root)
            && !crate::install::inventory::is_desktop_managed_path(s, roaming)
        {
            out.push(External {
                app: App::ClaudeCode,
                method: Method::Delete,
                target: s.display().to_string(),
                action: format!("删除升级残留 {}", s.display()),
            });
        }
    }
    if npm {
        out.push(External {
            app: App::ClaudeCode,
            method: Method::Npm,
            target: "@anthropic-ai/claude-code".into(),
            action: "npm uninstall -g @anthropic-ai/claude-code（连 claude.cmd 一起）".into(),
        });
    }
    if winget {
        out.push(External {
            app: App::ClaudeCode,
            method: Method::Winget,
            target: "Anthropic.ClaudeCode".into(),
            action:
                "winget uninstall --id Anthropic.ClaudeCode（连 WinGet\\Links 的 shim 与登记一起）"
                    .into(),
        });
    }
    if scoop {
        out.push(External {
            app: App::ClaudeCode,
            method: Method::Scoop,
            target: "claude-code".into(),
            action: "scoop uninstall claude-code".into(),
        });
    }
    out
}

/// 规划 Codex 的外部副本清理。**纯函数**：输入是 `detect::codex_candidates()` 里存在的那些。
///
/// `%LOCALAPPDATA%\OpenAI\Codex\bin\<哈希>\` 是 Codex 桌面版自己带的，`WindowsApps` 是 MSIX ——
/// 都属于别的程序，不在清理范围。
pub fn plan_codex_externals(found: &[PathBuf], managed_root: &Path) -> Vec<External> {
    use crate::install::inventory::norm;
    let mut out = Vec::new();
    let mut npm = false;
    let mut winget = false;
    for p in found {
        if is_inside(p, managed_root) {
            continue;
        }
        let n = norm(p);
        if n.contains("\\openai\\codex\\bin\\") || n.contains("\\windowsapps\\") {
            continue;
        }
        if n.ends_with("\\npm\\codex.cmd") || n.contains("\\node_modules\\@openai\\") {
            npm = true;
        } else if n.contains("\\winget\\") {
            winget = true;
        } else {
            out.push(External {
                app: App::Codex,
                method: Method::Delete,
                target: p.display().to_string(),
                action: format!("删除 {}", p.display()),
            });
        }
    }
    if npm {
        out.push(External {
            app: App::Codex,
            method: Method::Npm,
            target: "@openai/codex".into(),
            action: "npm uninstall -g @openai/codex（连 codex.cmd 与 npm 的临时残留一起）".into(),
        });
    }
    if winget {
        out.push(External {
            app: App::Codex,
            method: Method::Winget,
            target: "OpenAI.Codex".into(),
            action: "winget uninstall --id OpenAI.Codex（连 WinGet\\Links 的 shim 与登记一起）"
                .into(),
        });
    }
    out
}

/// 注册表里挂着、文件却已经不在了的卸载登记（本机实测就有一条：Claude Code 2.1.229 的 winget 登记）。
///
/// 只认名字以 `prefix` 开头的 HKCU 卸载项。脚本里只拼常量前缀，不拼任何外部输入。
#[cfg(windows)]
async fn stale_registrations(app: App, prefix: &str) -> Vec<External> {
    let script = format!(
        "$ErrorActionPreference = 'SilentlyContinue'; \
         try {{ [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false }} catch {{ }}; \
         $rows = @(Get-ChildItem 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall' | \
           Where-Object {{ $_.PSChildName -like '{prefix}*' }} | ForEach-Object {{ \
             $p = Get-ItemProperty $_.PSPath; \
             [pscustomobject]@{{ Key = $_.PSChildName; Location = [string]$p.InstallLocation }} }}); \
         ConvertTo-Json -InputObject $rows -Compress"
    );
    let Ok(o) = crate::process::hidden_tokio(tokio::process::Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .await
    else {
        return Vec::new();
    };
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(String::from_utf8_lossy(&o.stdout).trim()).unwrap_or_default();
    rows.into_iter()
        .filter_map(|r| {
            let key = r.get("Key")?.as_str()?.to_string();
            let loc = r
                .get("Location")
                .and_then(|l| l.as_str())
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .to_string();
            // 文件还在的登记交给 winget uninstall 去收，这里只管「文件早没了」的。
            if !loc.is_empty() && Path::new(&loc).exists() {
                return None;
            }
            // 登记名只允许常见字符，下面要拼进 reg delete 的参数里。
            if !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
            {
                return None;
            }
            Some(External {
                app,
                method: Method::Registry,
                action: format!("删除卸载登记 {key}（对应的文件早就不在了）"),
                target: key,
            })
        })
        .collect()
}

#[cfg(not(windows))]
async fn stale_registrations(_app: App, _prefix: &str) -> Vec<External> {
    Vec::new()
}

/// 本机全部外部副本（规划好的清理动作），给界面先过目。
pub async fn externals() -> Vec<External> {
    let root = root();
    let r = crate::install::inventory::Roots::current();
    let mut out = plan_claude_externals(
        &crate::install::inventory::scan(&r),
        &crate::install::inventory::stale_copies(&r),
        &root,
        r.roaming.as_deref(),
    );
    let codex_found: Vec<PathBuf> = crate::install::detect::codex_candidates()
        .into_iter()
        .filter(|p| p.exists())
        .collect();
    out.extend(plan_codex_externals(&codex_found, &root));
    out.extend(stale_registrations(App::ClaudeCode, "Anthropic.ClaudeCode").await);
    out.extend(stale_registrations(App::Codex, "OpenAI.Codex").await);
    out
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct CleanupReport {
    pub done: Vec<String>,
    pub failed: Vec<String>,
    /// 做不到或者故意不做的，照实说。
    pub notes: Vec<String>,
}

async fn run(program: &str, args: &[&str]) -> std::result::Result<String, String> {
    match crate::process::hidden_tokio(tokio::process::Command::new(program))
        .args(args)
        .output()
        .await
    {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).trim().to_string()),
        Ok(o) => Err(format!(
            "{}{}",
            String::from_utf8_lossy(&o.stderr).trim(),
            String::from_utf8_lossy(&o.stdout).trim()
        )),
        Err(e) => Err(e.to_string()),
    }
}

/// 删掉一个文件之后，顺着往上把**已经空了**的父目录删掉（到 `stop_at` 为止）。
fn prune_empty_parents(p: &Path, stop_at: &[PathBuf]) {
    let mut cur = p.parent().map(Path::to_path_buf);
    while let Some(d) = cur {
        if stop_at
            .iter()
            .any(|s| crate::install::inventory::same_path(s, &d))
        {
            break;
        }
        if std::fs::remove_dir(&d).is_err() {
            break; // 不空，或者删不掉 —— 都到此为止
        }
        cur = d.parent().map(Path::to_path_buf);
    }
}

/// npm 中途被打断时留下的 `.<包名>-<随机>` 临时目录（本机实测就有一个 `@openai\.codex-nITqmxYC`）。
fn npm_staging_dirs(scope_dir: &Path, name: &str) -> Vec<PathBuf> {
    let prefix = format!(".{name}-");
    std::fs::read_dir(scope_dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && p.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with(&prefix))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// **彻底清除**一个软件的外部副本。使用者要求「删得彻底、一点痕迹不留」。
///
/// 前提：面板托管的那份已经装好（不然清完这台机器上一份都没有了）。
/// 调用方负责先关掉正在跑的 Claude、结束后重锁（`lib.rs` 的维护窗口）。
///
/// 边界写死：只清可执行安装。**绝不删** `~\.claude`、`claude-profile-*`、`~\.codex`
/// （账户凭证与配置）、桌面端、编辑器扩展。`~\.local\bin` 目录本身与 PATH 不动 ——
/// uv、pipx 这些别的工具也往那里装东西。
pub async fn cleanup(app: App) -> Result<CleanupReport> {
    if !is_installed(app) {
        return Err(GateError::Other(format!(
            "面板托管的 {} 还没装好，先装它再清外部副本 —— 不然清完这台机器上一份都没有了。",
            app.label()
        )));
    }
    let plan: Vec<External> = externals()
        .await
        .into_iter()
        .filter(|e| e.app == app)
        .collect();
    let mut rep = CleanupReport::default();
    if plan.is_empty() {
        rep.notes.push("没有外部副本，什么都不用清。".into());
        return Ok(rep);
    }

    let home = dirs::home_dir().unwrap_or_default();
    let roaming = dirs::config_dir().unwrap_or_default();
    let stop_at = vec![
        home.clone(),
        home.join(".local"),
        home.join(".local").join("bin"),
        home.join(".claude"),
        roaming.clone(),
        dirs::data_local_dir().unwrap_or_default(),
        dirs::data_local_dir().unwrap_or_default().join("Programs"),
    ];

    for e in &plan {
        let r: std::result::Result<String, String> = match e.method {
            Method::Npm => {
                let r = run("cmd", &["/d", "/c", "npm", "uninstall", "-g", &e.target]).await;
                let (scope, name) = e.target.split_once('/').unwrap_or(("", &e.target));
                for d in
                    npm_staging_dirs(&roaming.join("npm").join("node_modules").join(scope), name)
                {
                    match std::fs::remove_dir_all(&d) {
                        Ok(()) => rep
                            .done
                            .push(format!("删掉 npm 中断留下的临时目录 {}", d.display())),
                        Err(err) => rep.failed.push(format!("{}：{err}", d.display())),
                    }
                }
                r.map(|_| format!("已卸载 npm 包 {}", e.target))
            }
            Method::Winget => run(
                "winget",
                &[
                    "uninstall",
                    "--id",
                    &e.target,
                    "-e",
                    "--silent",
                    "--disable-interactivity",
                    "--accept-source-agreements",
                ],
            )
            .await
            .map(|_| format!("已用 winget 卸载 {}", e.target)),
            Method::Scoop => run("cmd", &["/d", "/c", "scoop", "uninstall", &e.target])
                .await
                .map(|_| format!("已用 scoop 卸载 {}", e.target)),
            Method::Registry => run(
                "reg",
                &[
                    "delete",
                    &format!(
                        "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{}",
                        e.target
                    ),
                    "/f",
                ],
            )
            .await
            .map(|_| format!("已删除卸载登记 {}", e.target)),
            Method::Delete => {
                let p = PathBuf::from(&e.target);
                // 删之前再验一次签名：只删确实是 Anthropic / OpenAI 的那份，
                // 不删一个碰巧也叫 claude.exe 的别的程序。
                let signer = crate::signature::signer_of(&p).await;
                match crate::install::winget::signature_matches(signer.as_deref(), app.signer()) {
                    Some(true) => match std::fs::remove_file(&p) {
                        Ok(()) => {
                            prune_empty_parents(&p, &stop_at);
                            Ok(format!("已删除 {}", p.display()))
                        }
                        Err(err) => Err(format!("{} 删不掉（多半正在运行）：{err}", p.display())),
                    },
                    Some(false) => Err(format!(
                        "{} 的签名不是 {}（{}），不是面板要清的东西，没删",
                        p.display(),
                        app.signer(),
                        signer.unwrap_or_default()
                    )),
                    None => Err(format!("{} 读不出签名，没验成就不删", p.display())),
                }
            }
        };
        match r {
            Ok(msg) => rep.done.push(msg),
            Err(err) => rep.failed.push(format!("{}：{err}", e.action)),
        }
    }

    if app == App::ClaudeCode {
        rep.notes.push(
            "~\\.local\\bin 目录本身与 PATH 里的这一项没动 —— uv、pipx 等别的工具也往那里装东西。"
                .into(),
        );
    }
    Ok(rep)
}

// ------------------------------------------------------- 换托管根目录的判断

/// 换托管根目录之前该怎么处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relocate {
    /// 新旧是同一个目录 —— 什么都不用做。
    Same,
    /// 一个套在另一个里面。搬的时候会把自己套进去，直接拒绝。
    Nested,
    /// 新目录用不了（建不出 / 写不进 / 锁不上）。
    Unusable,
    /// 旧目录里什么都没装 —— 只要把新位置记下来。
    JustRecord,
    /// 要真的搬文件。
    Migrate,
}

/// 纯判断，**不碰磁盘**：两个事实（新目录能不能用、旧目录里有没有东西）
/// 由调用方先探好传进来。
///
/// # 为什么要单独提出来
///
/// `managed_set_dir` 是 133 行的编排（清场、等进程退、搬文件、写设置、
/// 失败回滚、重锁），从头到尾都在做 I/O，一条测试都写不了。
/// 而里面真正容易出错的是**顺序和前置条件**，不是 I/O 本身。
///
/// 把前置条件提成这个函数之后，「同目录要短路」「互相嵌套要拒绝」
/// 「空目录不该走搬迁那条长路径」三条就都钉得住了。
pub fn plan_relocate(old: &Path, new: &Path, usable: bool, has_files: bool) -> Relocate {
    if same_path(old, new) {
        return Relocate::Same;
    }
    // 嵌套判断必须在「能不能用」之前：一个套在另一个里面时，
    // 探测多半是过得去的（目录确实建得出、写得进），然后搬到一半
    // 把自己套进去 —— 那才是真正危险的形态。
    if is_inside(new, old) || is_inside(old, new) {
        return Relocate::Nested;
    }
    if !usable {
        return Relocate::Unusable;
    }
    if has_files {
        Relocate::Migrate
    } else {
        Relocate::JustRecord
    }
}

/// 托管目录该往设置里写什么。
///
/// **等于默认目录时写 `None`。** 写成字面路径的话，默认目录哪天变了
/// （换 `%LOCALAPPDATA%` 的算法、或者官方改了推荐位置），
/// 老用户会被钉死在那个写下去的旧路径上，而且没有任何提示 ——
/// 他只会发现「新版本说的默认位置跟我这儿不一样」。
pub fn managed_dir_setting(dir: &Path, default: &Path) -> Option<PathBuf> {
    (!same_path(dir, default)).then(|| dir.to_path_buf())
}

fn same_path(a: &Path, b: &Path) -> bool {
    super::inventory::same_path(a, b)
}

#[cfg(test)]
mod tests {

    #[test]
    fn relocating_to_the_same_directory_short_circuits() {
        // 不短路的话会走完整条迁移路径：清场、关进程、搬文件 ——
        // 使用者只是在设置里点了一下确认，对话全没了。
        let d = Path::new("C:\\apps\\qb");
        assert_eq!(
            plan_relocate(d, Path::new("C:\\apps\\qb\\"), true, true),
            Relocate::Same
        );
    }

    #[test]
    fn nesting_is_refused_before_usability_is_even_considered() {
        // 顺序有讲究：嵌套的那两个目录探测多半是过得去的
        // （确实建得出、写得进），于是先查 usable 的话会放行，
        // 然后搬到一半把自己套进去。
        let outer = Path::new("C:\\apps");
        let inner = Path::new("C:\\apps\\qb");
        assert_eq!(plan_relocate(outer, inner, true, true), Relocate::Nested);
        assert_eq!(plan_relocate(inner, outer, true, true), Relocate::Nested);
        // 连「用不了」都还轮不到报。
        assert_eq!(plan_relocate(outer, inner, false, true), Relocate::Nested);
    }

    #[test]
    fn an_unusable_target_is_refused_without_touching_anything() {
        assert_eq!(
            plan_relocate(Path::new("C:\\a"), Path::new("D:\\b"), false, true),
            Relocate::Unusable
        );
    }

    #[test]
    fn an_empty_old_directory_only_records_the_new_location() {
        // 什么都没装的时候走搬迁那条长路径没有意义，而那条路径会清场。
        assert_eq!(
            plan_relocate(Path::new("C:\\a"), Path::new("D:\\b"), true, false),
            Relocate::JustRecord
        );
        assert_eq!(
            plan_relocate(Path::new("C:\\a"), Path::new("D:\\b"), true, true),
            Relocate::Migrate
        );
    }

    #[test]
    fn the_default_managed_directory_is_stored_as_none() {
        // 存字面路径的话，默认位置哪天变了，老用户会被钉死在旧路径上，
        // 而且没有任何提示。
        let default = Path::new("C:\\Users\\me\\AppData\\Local\\ClaudeIpGate\\apps");
        assert_eq!(managed_dir_setting(default, default), None);
        assert_eq!(
            managed_dir_setting(Path::new("D:\\qb-apps"), default),
            Some(PathBuf::from("D:\\qb-apps"))
        );
    }
    use super::*;

    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "qbgate-managed-{tag}-{}-{}",
                std::process::id(),
                chrono::Local::now().format("%H%M%S%f")
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const MANIFEST: &str = r#"{"version":"2.1.268","platforms":{
        "win32-x64":{"binary":"claude.exe","checksum":"1E472BCFD49449E73EF76698C87F6583055AEB88DC41CA190A9C12C6791B5EEA","size":221637792},
        "win32-arm64":{"binary":"claude.exe","checksum":"aa","size":1}}}"#;

    #[test]
    fn claude_manifest_gives_the_official_url_checksum_and_size() {
        let r = parse_claude_manifest(MANIFEST, "2.1.268", "win32-x64").unwrap();
        assert_eq!(
            r.url,
            "https://downloads.claude.ai/claude-code-releases/2.1.268/win32-x64/claude.exe"
        );
        assert_eq!(
            r.sha256.as_deref(),
            Some("1e472bcfd49449e73ef76698c87f6583055aeb88dc41ca190a9c12c6791b5eea")
        );
        assert_eq!(r.size, Some(221637792));
        // 不是 64 位十六进制的 checksum 不算数。
        assert!(parse_claude_manifest(MANIFEST, "2.1.268", "win32-arm64").is_err());
        assert!(parse_claude_manifest(MANIFEST, "2.1.268", "linux-x64").is_err());
    }

    #[test]
    fn a_region_block_page_is_not_a_version() {
        assert!(looks_like_version("2.1.268"));
        assert!(looks_like_version("2.1.268-beta.1"));
        assert!(!looks_like_version("<!DOCTYPE html><html>"));
        assert!(!looks_like_version("2.1"));
        assert!(!looks_like_version(""));
    }

    #[test]
    fn codex_release_is_pinned_to_the_openai_repo() {
        let json = r#"{"tag_name":"rust-v0.154.0","assets":[
            {"name":"codex-x86_64-pc-windows-msvc.exe.zip","size":104759949,
             "digest":"sha256:53685f9f6bd171d4bd7d6c2be724fc04f6737a4001eb8471ec59824e5adc8042",
             "browser_download_url":"https://github.com/openai/codex/releases/download/rust-v0.154.0/codex-x86_64-pc-windows-msvc.exe.zip"},
            {"name":"codex-aarch64-pc-windows-msvc.exe.zip","size":1,
             "browser_download_url":"https://evil.example.com/codex.zip"}]}"#;
        let r = parse_codex_release(json, codex_asset(false)).unwrap();
        assert_eq!(r.version, "0.154.0");
        assert_eq!(
            r.sha256.as_deref(),
            Some("53685f9f6bd171d4bd7d6c2be724fc04f6737a4001eb8471ec59824e5adc8042")
        );
        // 下载地址不在 openai/codex 下就不跟。
        assert!(parse_codex_release(json, codex_asset(true)).is_err());
        assert!(parse_codex_release(json, "nope.zip").is_err());
    }

    #[test]
    fn placing_keeps_a_backup_and_verifies_the_hash() {
        let t = Tmp::new("place");
        let dir = t.0.join("claude-code");
        std::fs::create_dir_all(dir.join(".download")).unwrap();
        let target = dir.join("claude.exe");
        std::fs::write(&target, b"old").unwrap();
        let src = dir.join(".download").join("claude.exe");
        std::fs::write(&src, b"new").unwrap();
        let sha = sha256_file(&src).unwrap();

        let backup = place(App::ClaudeCode, &src, &target, &sha)
            .unwrap()
            .expect("有旧的就要留底");
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert_eq!(std::fs::read(&backup).unwrap(), b"old");
        // 留底名必须落进 stale_copies 的清理范围。
        assert!(backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("claude.exe.old."));

        // 哈希对不上：回滚，旧的回到原位。
        let src2 = dir.join(".download").join("x.exe");
        std::fs::write(&src2, b"newer").unwrap();
        assert!(place(App::ClaudeCode, &src2, &target, &"0".repeat(64)).is_err());
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"new",
            "失败时原来那份要回到原位"
        );
    }

    #[test]
    fn migrating_moves_everything_and_refuses_to_overwrite() {
        let t = Tmp::new("migrate");
        let old = t.0.join("old");
        let new = t.0.join("new");
        std::fs::create_dir_all(app_dir(&old, App::ClaudeCode)).unwrap();
        std::fs::write(exe_in(&old, App::ClaudeCode), b"cc").unwrap();
        std::fs::create_dir_all(app_dir(&old, App::Codex)).unwrap();
        std::fs::write(exe_in(&old, App::Codex), b"cx").unwrap();
        write_record(
            &old,
            App::Codex,
            Record {
                version: "0.154.0".into(),
                ..Default::default()
            },
        )
        .unwrap();

        migrate_files(&old, &new).unwrap();
        assert_eq!(std::fs::read(exe_in(&new, App::ClaudeCode)).unwrap(), b"cc");
        assert_eq!(std::fs::read(exe_in(&new, App::Codex)).unwrap(), b"cx");
        assert_eq!(
            record_of(&new, App::Codex).unwrap().version,
            "0.154.0",
            "记录要跟着搬"
        );
        assert!(!old.exists(), "旧目录搬空了就删掉");

        // 反过来搬：目标里已经有了，不覆盖。
        let other = t.0.join("other");
        std::fs::create_dir_all(app_dir(&other, App::ClaudeCode)).unwrap();
        std::fs::write(exe_in(&other, App::ClaudeCode), b"theirs").unwrap();
        assert!(migrate_files(&new, &other).is_err());
        assert_eq!(
            std::fs::read(exe_in(&other, App::ClaudeCode)).unwrap(),
            b"theirs"
        );
    }

    /// 中途有一个搬不动（这里用一个不许删除共享的句柄模拟「Codex 正在运行」）：
    /// 已经搬过去的要搬回来，搬不动的那个要一个文件不少地留在原处，新目录里不留任何 exe。
    #[cfg(windows)]
    #[test]
    fn a_failed_migration_puts_everything_back() {
        use std::os::windows::fs::OpenOptionsExt;
        let t = Tmp::new("migrate-rollback");
        let old = t.0.join("old");
        let new = t.0.join("new");
        std::fs::create_dir_all(app_dir(&old, App::ClaudeCode)).unwrap();
        std::fs::write(exe_in(&old, App::ClaudeCode), b"cc").unwrap();
        std::fs::create_dir_all(app_dir(&old, App::Codex)).unwrap();
        std::fs::write(exe_in(&old, App::Codex), b"cx").unwrap();
        let helper = app_dir(&old, App::Codex).join("codex-command-runner.exe");
        std::fs::write(&helper, b"runner").unwrap();
        write_record(
            &old,
            App::Codex,
            Record {
                version: "0.154.0".into(),
                ..Default::default()
            },
        )
        .unwrap();

        {
            // 读写共享、不给删除共享：目录改不了名，这个文件删不掉 —— 跟正在运行的 exe 一样。
            let _busy = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0x1 | 0x2)
                .open(exe_in(&old, App::Codex))
                .unwrap();
            let err = migrate_files(&old, &new).unwrap_err().to_string();
            assert!(err.contains("恢复原样"), "{err}");
        }

        // Claude Code 先搬过去了，又被搬了回来；Codex 两个文件都在；记录没丢。
        assert_eq!(std::fs::read(exe_in(&old, App::ClaudeCode)).unwrap(), b"cc");
        assert_eq!(std::fs::read(exe_in(&old, App::Codex)).unwrap(), b"cx");
        assert_eq!(
            std::fs::read(&helper).unwrap(),
            b"runner",
            "删到一半的要补回来"
        );
        assert_eq!(record_of(&old, App::Codex).unwrap().version, "0.154.0");
        assert!(
            !app_dir(&new, App::ClaudeCode).exists(),
            "新目录里不能留一份没人锁的 claude.exe"
        );
        assert!(!app_dir(&new, App::Codex).exists());
        assert!(!records_path(&new).exists());

        // 句柄放掉之后再搬一次就能成。
        migrate_files(&old, &new).unwrap();
        assert_eq!(std::fs::read(exe_in(&new, App::Codex)).unwrap(), b"cx");
        assert!(!old.exists());
    }

    #[test]
    fn inside_checks_are_case_and_slash_insensitive() {
        assert!(is_inside(Path::new(r"C:\A\b\c"), Path::new(r"c:/a/B")));
        assert!(is_inside(Path::new(r"C:\A"), Path::new(r"C:\A\")));
        assert!(!is_inside(Path::new(r"C:\AB"), Path::new(r"C:\A")));
    }

    #[test]
    fn some_directories_are_never_acceptable_as_the_root() {
        let home = Path::new(r"C:\Users\me");
        let roaming = Path::new(r"C:\Users\me\AppData\Roaming");
        let local = Path::new(r"C:\Users\me\AppData\Local");
        let panel = Path::new(r"C:\Users\me\AppData\Local\QB Gate");
        let why = |d: &str| {
            forbidden_reason(
                Path::new(d),
                Some(home),
                Some(roaming),
                Some(local),
                Some(panel),
            )
        };
        assert!(why(r"C:\Users\me\AppData\Local\QB Gate\apps").is_some());
        assert!(why(r"C:\Users\me\AppData\Roaming\Claude-main\x").is_some());
        assert!(why(r"C:\Users\me\AppData\Roaming\Claude").is_some());
        assert!(why(r"C:\Users\me\AppData\Local\AnthropicClaude\x").is_some());
        assert!(why(r"C:\Users\me\.claude\apps").is_some());
        assert!(why(r"C:\Users\me\.local\apps").is_some());
        // 这些都行。
        assert!(why(r"C:\\Users\\me\\AppData\\Local\\ClaudeIpGate\\apps").is_none());
        assert!(why(r"D:\ClaudeApps").is_none());
        assert!(why(r"C:\Users\me\AppData\Roaming\Claude Code Tools").is_none());
    }

    #[cfg(windows)]
    #[test]
    fn a_local_ntfs_dir_passes_the_lock_probe() {
        // 在临时目录里放一个探针文件，锁上、查、解开、删掉 —— 跟 acl.rs 的往返测试同一类，
        // 只碰临时文件。
        let t = Tmp::new("probe");
        let p = probe_dir(&t.0.join("apps"));
        assert!(p.ok, "{:?}", p.reason);
        assert!(
            std::fs::read_dir(t.0.join("apps"))
                .unwrap()
                .next()
                .is_none(),
            "探针文件要删掉"
        );
        assert!(!probe_dir(Path::new("relative\\dir")).ok, "相对路径不收");
    }

    #[test]
    fn claude_externals_leave_other_programs_and_the_managed_copy_alone() {
        use crate::install::inventory::{Install, Kind};
        let mk = |kind, p: &str| Install {
            kind,
            path: PathBuf::from(p),
            lockable: true,
            launchable: true,
            preferred: false,
        };
        let managed = Path::new(r"C:\M");
        let installs = vec![
            mk(Kind::Managed, r"C:\M\claude-code\claude.exe"),
            mk(Kind::Native, r"C:\Users\me\.local\bin\claude.exe"),
            mk(
                Kind::NativeVersion,
                r"C:\Users\me\.local\share\claude\versions\2.1.267",
            ),
            mk(
                Kind::NativeVersion,
                r"C:\Users\me\.claude\downloads\claude-2.1.263-win32-x64.exe",
            ),
            mk(Kind::Npm, r"C:\Users\me\AppData\Roaming\npm\claude.cmd"),
            mk(
                Kind::NpmNative,
                r"C:\Users\me\AppData\Roaming\npm\node_modules\@anthropic-ai\x\claude.exe",
            ),
            mk(
                Kind::Winget,
                r"C:\Users\me\AppData\Local\Microsoft\WinGet\Links\claude.exe",
            ),
            mk(
                Kind::DesktopManaged,
                r"C:\Users\me\AppData\Roaming\Claude\claude-code\2.1.266\claude.exe",
            ),
            mk(
                Kind::Editor,
                r"C:\Users\me\.vscode\extensions\anthropic.claude-code-1\claude.exe",
            ),
            mk(
                Kind::DesktopStub,
                r"C:\Users\me\AppData\Local\AnthropicClaude\claude.exe",
            ),
        ];
        let plan = plan_claude_externals(
            &installs,
            &[],
            managed,
            Some(Path::new(r"C:\Users\me\AppData\Roaming")),
        );
        let deletes: Vec<&str> = plan
            .iter()
            .filter(|e| e.method == Method::Delete)
            .map(|e| e.target.as_str())
            .collect();
        assert_eq!(deletes.len(), 3, "{deletes:?}");
        assert!(deletes.iter().all(|d| !d.contains("AnthropicClaude")
            && !d.contains(".vscode")
            && !d.contains(r"Roaming\Claude\")));
        assert!(
            !deletes.iter().any(|d| d.starts_with(r"C:\M")),
            "托管那份绝不在清理表里"
        );
        // npm 两种合成一次卸载，winget 一次卸载。
        assert_eq!(plan.iter().filter(|e| e.method == Method::Npm).count(), 1);
        assert_eq!(
            plan.iter().filter(|e| e.method == Method::Winget).count(),
            1
        );
        // 账户与配置根本不会出现。
        assert!(plan
            .iter()
            .all(|e| !e.target.contains("credentials") && !e.target.contains("claude-profile")));
    }

    #[test]
    fn codex_externals_skip_the_desktop_app_and_msix_copies() {
        let managed = Path::new(r"C:\M");
        let found = vec![
            PathBuf::from(r"C:\M\codex\codex.exe"),
            PathBuf::from(r"C:\Users\me\AppData\Roaming\npm\codex.cmd"),
            PathBuf::from(r"C:\Users\me\.local\bin\codex.exe"),
            PathBuf::from(r"C:\Users\me\AppData\Local\OpenAI\Codex\bin\7ac07f\codex.exe"),
            PathBuf::from(r"C:\Users\me\AppData\Local\Microsoft\WindowsApps\codex.exe"),
            PathBuf::from(r"C:\Users\me\AppData\Local\Microsoft\WinGet\Links\codex.exe"),
        ];
        let plan = plan_codex_externals(&found, managed);
        let targets: Vec<&str> = plan.iter().map(|e| e.target.as_str()).collect();
        assert_eq!(
            targets,
            vec![
                r"C:\Users\me\.local\bin\codex.exe",
                "@openai/codex",
                "OpenAI.Codex"
            ]
        );
    }

    #[test]
    fn codex_helpers_travel_with_the_main_binary() {
        // winget 清单实测：zip 里有三个 exe。只装主程序，Codex 的 Windows 沙箱就起不来。
        let all = vec![
            PathBuf::from(r"C:\x\codex-command-runner.exe"),
            PathBuf::from(r"C:\x\codex-windows-sandbox-setup.exe"),
            PathBuf::from(r"C:\x\codex-x86_64-pc-windows-msvc.exe"),
        ];
        let (main, helpers) = split_codex_exes(&all).unwrap();
        assert!(main.ends_with("codex-x86_64-pc-windows-msvc.exe"));
        assert_eq!(helpers.len(), 2);
        assert!(split_codex_exes(&all[..2]).is_none(), "没有主程序就不装");
    }

    #[test]
    fn npm_staging_leftovers_are_found() {
        let t = Tmp::new("npmstage");
        std::fs::create_dir_all(t.0.join(".codex-nITqmxYC")).unwrap();
        std::fs::create_dir_all(t.0.join("codex")).unwrap();
        std::fs::create_dir_all(t.0.join(".codexx-1")).unwrap();
        let found = npm_staging_dirs(&t.0, "codex");
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with(".codex-nITqmxYC"));
    }
}
