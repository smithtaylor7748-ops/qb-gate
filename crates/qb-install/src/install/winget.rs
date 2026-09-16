//! 安装 Claude：winget 优先，官方安装器兜底。
//!
//! # 为什么不再自己下 exe 再钉 SHA-256
//!
//! 原来那条路（`installers.lock.json` + `scripts/pin-hash.mjs`）在 Windows 上是个
//! 不必要的死结：哈希得手工钉，钉不上安装按钮就永远是灰的 —— 而它确实一直是灰的。
//!
//! 实际上完整性已经有三层保障，QB Gate 不需要自己再钉一份：
//!
//!   1. **winget manifest** 自带 `InstallerSha256`，由 winget 自己校验；
//!   2. 官方安装脚本走 Anthropic 自己签名的 `manifest.json`；
//!   3. Windows 二进制上还有 Authenticode 签名（主体是 Anthropic PBC）。
//!
//! 装完我们再核对一次第 3 条，作为「装进来的确实是 Anthropic 的东西」的独立佐证。
//! **核不过只警告不阻断** —— 用户完全可能自己装了别的构建，那是他的自由。
//!
//! # 三个包 id（2026-09-09 实机 `winget show` 核对过）
//!
//! | target | id | 发布者 |
//! |---|---|---|
//! | claude-code | `Anthropic.ClaudeCode` | Anthropic PBC |
//! | claude-desktop | `Anthropic.Claude` | Anthropic, PBC |
//! | codex | `OpenAI.Codex` | OpenAI, Inc. |
//!
//! 桌面端那个 id 在官方文档里没写，只在第三方索引上见过 —— 所以
//! [`probe`] 会实际去 `winget show` 一次，查不到就退回「打开官方下载页」，
//! 而不是硬着头皮装一个不存在的包。
//!
//! # Codex 与另外两个不一样的地方
//!
//! 1. **它不归 Claude 门禁管**（至少现在还不），所以第 1 步解锁和第 6 步
//!    重锁对它没意义，`install()` 里按 target 跳过。
//! 2. **签名主体是 OpenAI，不是 Anthropic**，而且它的 exe 不在
//!    `gate::collect_targets()` 里 —— 那个函数只枚举 claude.exe。
//!    不特判的话会挑中某个 claude.exe，把它的 Anthropic 签名报成 Codex 的
//!    验证结果。见 [`verify_signature`]。
//! 3. **winget 上的版本可能比 npm 全局的旧**（实机：winget 0.146.1 vs
//!    npm 0.153.4），所以已装就不要去动它。

use crate::error::{GateError, Result};
use crate::sink::ProgressSink;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum InstallTarget {
    ClaudeCode,
    ClaudeDesktop,
    Codex,
}

impl InstallTarget {
    pub const ALL: [InstallTarget; 3] = [
        InstallTarget::ClaudeCode,
        InstallTarget::ClaudeDesktop,
        InstallTarget::Codex,
    ];

    /// winget 包 id。
    pub fn package_id(self) -> &'static str {
        match self {
            InstallTarget::ClaudeCode => "Anthropic.ClaudeCode",
            InstallTarget::ClaudeDesktop => "Anthropic.Claude",
            InstallTarget::Codex => "OpenAI.Codex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            InstallTarget::ClaudeCode => "Claude Code",
            InstallTarget::ClaudeDesktop => "Claude 桌面端",
            InstallTarget::Codex => "Codex CLI",
        }
    }

    /// 这个目标归 Claude 的 IP 门禁管吗？
    ///
    /// 决定 `install()` 要不要跑第 1 步解锁与第 6 步重锁。Codex 只有在设置里
    /// 打开开关之后才归门禁管；关着的时候对它跑那两步，等于无谓地把 Claude
    /// 的锁摘掉再装回去。
    pub fn under_qb_gate(self) -> bool {
        match self {
            InstallTarget::Codex => crate::settings::codex_under_gate(),
            _ => true,
        }
    }

    /// 期望的 Authenticode 签名主体关键词。
    pub fn expected_signer(self) -> &'static str {
        match self {
            InstallTarget::Codex => "openai",
            _ => "anthropic",
        }
    }
}

/// winget 装不上时还剩哪条路。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    /// 官方安装脚本 `irm https://claude.ai/install.ps1 | iex`。
    OfficialScript,
    /// `npm install -g @openai/codex`。Codex 是 npm 包，这是官方给的装法，
    /// 不是我们自己拼的下载链接。
    NpmGlobal,
    /// 只能请用户自己去官方下载页。桌面端没有等价的一行安装命令，
    /// **不要**为它拼一个下载链接 —— 那正是这次要下线的东西。
    ManualDownload,
}

pub fn fallback_for(t: InstallTarget) -> Fallback {
    match t {
        InstallTarget::ClaudeCode => Fallback::OfficialScript,
        InstallTarget::ClaudeDesktop => Fallback::ManualDownload,
        InstallTarget::Codex => Fallback::NpmGlobal,
    }
}

/// `winget install` 的完整参数。
///
/// 三个 flag 缺一不可：两个 `--accept-*` 不给就会停在交互式同意页，
/// `--disable-interactivity` 不给就可能等一个永远不会来的按键 ——
/// 而我们是在没有终端的子进程里跑它，那等于挂死。
pub fn winget_args(t: InstallTarget) -> Vec<String> {
    [
        "install",
        "--id",
        t.package_id(),
        "-e",
        "--accept-package-agreements",
        "--accept-source-agreements",
        "--disable-interactivity",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Authenticode 主体里有没有 Anthropic。
///
/// `None` = 拿不到签名信息（不等于没签名），界面上要如实说「没验成」，
/// 不能显示成「验证失败」。
pub fn signature_has_anthropic(subject: Option<&str>) -> Option<bool> {
    signature_matches(subject, "anthropic")
}

/// 签名主体里有没有指定的关键词。
///
/// `None` = 拿不到签名信息（不等于没签名）。这个三态**不能塌成 bool** ——
/// 塌了就会把「没验成」显示成「验证失败」。
pub fn signature_matches(subject: Option<&str>, want: &str) -> Option<bool> {
    subject.map(|s| s.to_lowercase().contains(&want.to_lowercase()))
}

// ---------------------------------------------------------------- 探测

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PackageProbe {
    pub target: InstallTarget,
    pub id: &'static str,
    pub found: bool,
    pub available_version: Option<String>,
    pub installed_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct InstallProbe {
    pub winget_available: bool,
    pub winget_version: Option<String>,
    pub packages: Vec<PackageProbe>,
}

/// 从 `winget show` 的输出里抠版本号。
///
/// 输出是本地化的，中文机器上是「版本:」，英文机器上是「Version:」，
/// 所以按前缀匹配会漏。这里认的是「第一行含冒号且冒号后像版本号的」。
pub fn parse_shown_version(stdout: &str) -> Option<String> {
    for line in stdout.lines().take(12) {
        let Some((_, rest)) = line.split_once(':') else {
            continue;
        };
        let v = rest.trim();
        if !v.is_empty()
            && v.chars().next().is_some_and(|c| c.is_ascii_digit())
            && v.chars().all(|c| c.is_ascii_digit() || c == '.')
            && v.contains('.')
        {
            return Some(v.to_string());
        }
    }
    None
}

#[cfg(windows)]
async fn winget_version() -> Option<String> {
    let out = crate::process::hidden_tokio(tokio::process::Command::new("winget"))
        .arg("--version")
        .output()
        .await
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(not(windows))]
async fn winget_version() -> Option<String> {
    None
}

#[cfg(windows)]
async fn show_package(t: InstallTarget) -> (bool, Option<String>) {
    let out = crate::process::hidden_tokio(tokio::process::Command::new("winget"))
        .args([
            "show",
            "--id",
            t.package_id(),
            "-e",
            "--disable-interactivity",
        ])
        .output()
        .await;
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            (true, parse_shown_version(&text))
        }
        _ => (false, None),
    }
}

#[cfg(not(windows))]
async fn show_package(_t: InstallTarget) -> (bool, Option<String>) {
    (false, None)
}

pub async fn probe() -> InstallProbe {
    let ver = winget_version().await;
    let available = ver.is_some();

    let mut packages = Vec::new();
    for t in InstallTarget::ALL {
        let (found, avail) = if available {
            show_package(t).await
        } else {
            (false, None)
        };
        packages.push(PackageProbe {
            target: t,
            id: t.package_id(),
            found,
            available_version: avail,
            installed_version: installed_version_of(t).await,
        });
    }

    InstallProbe {
        winget_available: available,
        winget_version: ver,
        packages,
    }
}

async fn installed_version_of(t: InstallTarget) -> Option<String> {
    match t {
        InstallTarget::ClaudeCode => crate::install::detect::claude_code().await.version,
        InstallTarget::ClaudeDesktop => crate::install::detect::claude_desktop().await.version,
        InstallTarget::Codex => crate::install::detect::codex().await.version,
    }
}

// ---------------------------------------------------------------- 安装

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "InstallMethod")]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Winget,
    /// 面板托管安装：官方源直下，放进托管目录（v0.9.0，`install::managed`）。
    Managed,
    OfficialScript,
    NpmGlobal,
    ManualDownload,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct InstallResult {
    pub target: InstallTarget,
    pub ok: bool,
    pub method: Method,
    pub relocked: usize,
    pub signature_ok: Option<bool>,
    pub detail: String,
    pub log: Vec<String>,
}

/// 六段。前端进度条按这个总数画，`lib.rs` 建 Reporter 时用它。
pub const TOTAL: u32 = 6;

/// 跑一个命令并把 stdout / stderr 逐行发给界面。
#[cfg(windows)]
pub async fn run_streaming(
    program: &str,
    args: &[String],
    rep: &dyn ProgressSink,
    step: u32,
    log: &mut Vec<String>,
) -> Result<bool> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut child = crate::process::hidden_tokio(tokio::process::Command::new(program))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| GateError::Other(format!("启动 {program} 失败：{e}")))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    if let Some(out) = stdout {
        let mut lines = BufReader::new(out).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            // winget 的进度条靠回车刷新，收进来是一行里塞满 \r —— 只留最后一段。
            let line = line.rsplit('\r').next().unwrap_or(&line).trim().to_string();
            if line.is_empty() {
                continue;
            }
            rep.log(step, &line);
            log.push(line);
        }
    }
    if let Some(err) = stderr {
        let mut lines = BufReader::new(err).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }
            rep.log(step, &line);
            log.push(line);
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| GateError::Other(format!("等待 {program} 结束失败：{e}")))?;
    Ok(status.success())
}

#[cfg(not(windows))]
pub async fn run_streaming(
    _program: &str,
    _args: &[String],
    _rep: &dyn ProgressSink,
    _step: u32,
    _log: &mut Vec<String>,
) -> Result<bool> {
    Err(GateError::Other("安装功能只在 Windows 上可用".into()))
}

/// 桌面端卸载登记里写的安装位置（`HKCU\…\Uninstall\AnthropicClaude` 的 `InstallLocation`）。
/// 脚本里只有常量，没有拼进任何外部输入。
#[cfg(windows)]
pub async fn desktop_install_location() -> Option<String> {
    const SCRIPT: &str = "(Get-ItemProperty 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\AnthropicClaude' -ErrorAction SilentlyContinue).InstallLocation";
    let o = crate::process::powershell_tokio(SCRIPT)
        .output()
        .await
        .ok()?;
    let s = String::from_utf8_lossy(&o.stdout)
        .trim()
        .trim_matches('"')
        .to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(not(windows))]
pub async fn desktop_install_location() -> Option<String> {
    None
}

/// 找到本目标对应的那个 exe，核对签名主体。
///
/// ⚠ **Codex 必须走自己的路径。** 调用方传进来的那些副本**只有 claude.exe**。
/// 不特判的话 Codex 会命中「第一个非桌面端的 claude.exe」，然后把那个二进制的
/// Anthropic 签名报成 Codex 的验证结果 —— 一个看起来通过、实际上什么都没验的绿勾。
pub async fn verify_signature(
    target: InstallTarget,
    desktop_stub: Option<&std::path::Path>,
) -> Option<bool> {
    let path = match target {
        InstallTarget::Codex => crate::install::detect::codex().await.path?,
        // 验「启动时会用的那一份」。原来取的是清单里第一个非存根的副本 ——
        // 清单按路径排序，现在里面还有版本库、编辑器扩展，第一个未必是刚装的那份。
        InstallTarget::ClaudeCode => {
            crate::install::inventory::preferred_exe(&crate::install::inventory::Roots::current())?
        }
        // 调用方把存根路径挑好传进来 —— 这里不再认识 `gate::targets`
        // 那套类型，否则整个 winget 流程会为了一个枚举而依赖门禁。
        InstallTarget::ClaudeDesktop => desktop_stub?.to_path_buf(),
    };
    // 复用一键关闭那边现成的签名查询，不再写第二份。
    let subject = crate::signature::signer_of(&path).await;
    signature_matches(subject.as_deref(), target.expected_signer())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_ids_are_the_ones_verified_on_the_machine() {
        // 2026-09-09 用 `winget show --id <id> -e` 实机核对过，三个都存在。
        // Codex 那次输出：`Found Codex CLI [OpenAI.Codex]` / `Version: 0.146.1`
        // / `Publisher: OpenAI, Inc.`
        assert_eq!(
            InstallTarget::ClaudeCode.package_id(),
            "Anthropic.ClaudeCode"
        );
        assert_eq!(
            InstallTarget::ClaudeDesktop.package_id(),
            "Anthropic.Claude"
        );
        assert_eq!(InstallTarget::Codex.package_id(), "OpenAI.Codex");
    }

    #[test]
    fn claude_targets_are_always_under_the_gate() {
        // 归门禁管的目标才跑「解锁 → 装 → 重锁」。Claude 那两个永远归门禁管，
        // 这条不受任何设置影响。
        assert!(InstallTarget::ClaudeCode.under_qb_gate());
        assert!(InstallTarget::ClaudeDesktop.under_qb_gate());
    }

    #[test]
    fn codex_follows_the_setting_and_defaults_to_outside_the_gate() {
        // Codex 归不归门禁管由设置决定，默认在门禁之外 ——
        // 默认开着的话，装一次 Codex 就会把 Claude 的门无谓地开一遍。
        assert!(!crate::settings::Settings::default().codex_under_gate);
        assert_eq!(
            InstallTarget::Codex.under_qb_gate(),
            crate::settings::codex_under_gate()
        );
    }

    #[test]
    fn codex_expects_openai_not_anthropic_in_its_signature() {
        // Codex 的 exe 不在 gate::collect_targets() 里（那里只有 claude.exe）。
        // 拿 Anthropic 当关键词去验 Codex，等于验了个寂寞。
        assert_eq!(InstallTarget::Codex.expected_signer(), "openai");
        assert_eq!(InstallTarget::ClaudeCode.expected_signer(), "anthropic");

        assert_eq!(
            signature_matches(Some("CN=OpenAI, Inc., O=OpenAI"), "openai"),
            Some(true)
        );
        assert_eq!(
            signature_matches(Some("CN=Anthropic PBC"), "openai"),
            Some(false)
        );
        assert_eq!(signature_matches(None, "openai"), None);
    }

    #[test]
    fn winget_args_carry_every_non_interactive_flag() {
        // 少任何一个，命令都会停在一个没人能点的同意页上 ——
        // 我们是在没有终端的子进程里跑它，那等于挂死。
        let a = winget_args(InstallTarget::ClaudeCode);
        for flag in [
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ] {
            assert!(a.iter().any(|x| x == flag), "缺 {flag}");
        }
        assert_eq!(a[0], "install");
        assert!(a.iter().any(|x| x == "-e"), "缺精确匹配 -e");
        assert!(a.iter().any(|x| x == "Anthropic.ClaudeCode"));
    }

    #[test]
    fn exact_match_flag_prevents_installing_a_lookalike() {
        // 没有 -e 时 winget 会做模糊匹配，`Anthropic.Claude` 可能命中别的包。
        for t in InstallTarget::ALL {
            assert!(winget_args(t).iter().any(|x| x == "-e"));
        }
    }

    #[test]
    fn each_target_has_the_right_fallback() {
        // 桌面端没有等价的一行安装命令。硬给它拼一个下载链接，
        // 就等于把刚下线的「自己下 exe」又搬回来了。
        assert_eq!(
            fallback_for(InstallTarget::ClaudeCode),
            Fallback::OfficialScript
        );
        assert_eq!(
            fallback_for(InstallTarget::ClaudeDesktop),
            Fallback::ManualDownload
        );
        // Codex 是 npm 包，`npm i -g @openai/codex` 是官方给的装法。
        assert_eq!(fallback_for(InstallTarget::Codex), Fallback::NpmGlobal);
    }

    #[test]
    fn signature_check_distinguishes_unknown_from_mismatch() {
        // 「拿不到签名」和「签名不是 Anthropic」是两件事，合并了就会误报。
        assert_eq!(
            signature_has_anthropic(Some("CN=Anthropic PBC, O=Anthropic PBC")),
            Some(true)
        );
        assert_eq!(
            signature_has_anthropic(Some("cn=anthropic, pbc")),
            Some(true)
        );
        assert_eq!(
            signature_has_anthropic(Some("CN=Some Other Corp")),
            Some(false)
        );
        assert_eq!(signature_has_anthropic(None), None);
    }

    #[test]
    fn parses_version_from_localized_winget_show_output() {
        let en = "Found Claude Code [Anthropic.ClaudeCode]\nVersion: 2.1.263\nPublisher: Anthropic PBC\n";
        assert_eq!(parse_shown_version(en).as_deref(), Some("2.1.263"));

        let zh = "已找到 Claude [Anthropic.Claude]\n版本: 1.44121.2\n发布者: Anthropic, PBC\n";
        assert_eq!(parse_shown_version(zh).as_deref(), Some("1.44121.2"));
    }

    #[test]
    fn version_parser_skips_non_version_colon_lines() {
        // 「发布者支持 URL: https://...」这类也含冒号，不能被当成版本。
        let s = "Found Claude [Anthropic.Claude]\nPublisher Support Url: https://example.com/1.2\nVersion: 1.44121.2\n";
        assert_eq!(parse_shown_version(s).as_deref(), Some("1.44121.2"));
    }

    #[test]
    fn version_parser_returns_none_when_absent() {
        assert_eq!(
            parse_shown_version("No package found matching input criteria."),
            None
        );
        assert_eq!(parse_shown_version(""), None);
    }

    #[test]
    fn targets_round_trip_through_serde_as_kebab_case() {
        // 前端 `InstallTarget` 是 'claude-code' | 'claude-desktop'，
        // 两边对不上的话命令会带着一个后端不认识的参数值发过来。
        let j = serde_json::to_string(&InstallTarget::ClaudeCode).unwrap();
        assert_eq!(j, "\"claude-code\"");
        let j = serde_json::to_string(&InstallTarget::ClaudeDesktop).unwrap();
        assert_eq!(j, "\"claude-desktop\"");
        let j = serde_json::to_string(&InstallTarget::Codex).unwrap();
        assert_eq!(j, "\"codex\"");

        let back: InstallTarget = serde_json::from_str("\"claude-desktop\"").unwrap();
        assert_eq!(back, InstallTarget::ClaudeDesktop);
        let back: InstallTarget = serde_json::from_str("\"codex\"").unwrap();
        assert_eq!(back, InstallTarget::Codex);
    }

    #[test]
    fn method_serializes_as_the_frontend_union() {
        assert_eq!(
            serde_json::to_string(&Method::Winget).unwrap(),
            "\"winget\""
        );
        assert_eq!(
            serde_json::to_string(&Method::OfficialScript).unwrap(),
            "\"official_script\""
        );
        assert_eq!(
            serde_json::to_string(&Method::NpmGlobal).unwrap(),
            "\"npm_global\""
        );
        assert_eq!(
            serde_json::to_string(&Method::ManualDownload).unwrap(),
            "\"manual_download\""
        );
    }
}
