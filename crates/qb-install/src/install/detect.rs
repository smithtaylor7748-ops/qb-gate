//! 本机环境检测：Claude 桌面端 / Claude Code / 浏览器 / Codex。

use serde::Serialize;
use std::path::PathBuf;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Software {
    pub id: &'static str,
    pub name: &'static str,
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<PathBuf>,
    /// 检测到但建议重装时给出的理由。
    pub advisory: Option<String>,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}
fn local() -> PathBuf {
    dirs::data_local_dir().unwrap_or_default()
}

/// Claude Code 检测。
///
/// # v0.8.0 之前只查 `~\.local\bin`
///
/// 于是 winget / npm / Scoop 装的一律报「未安装」。现在跟启动、上锁用同一张表
/// （`install::inventory`），报的是「启动 Claude Code 时真正会用的那一份」。
///
/// **版本号从文件属性读，不运行它。** 原来这里跑 `claude.exe --version` ——
/// 而没有租约时那个文件上挂着 Deny ExecuteFile，版本号必然读不出来。
/// upgrade.rs 文件头第 3 条早就写着这条规矩，这里没跟上。
pub async fn claude_code() -> Software {
    use super::inventory::{self, Kind};
    let roots = inventory::Roots::current();
    let all = inventory::scan(&roots);
    let primary = all.iter().find(|i| i.preferred);

    let version = match primary {
        Some(i) if i.kind == Kind::Npm => inventory::npm_version(&roots),
        Some(i) => super::upgrade::file_version(&i.path).await,
        None => None,
    };

    let others = all
        .iter()
        .filter(|i| i.kind != Kind::DesktopStub && !i.launchable)
        .count();
    let advisory = match primary {
        Some(i) if i.kind == Kind::Npm => Some(
            "这一份是 npm 装的，经 node.exe 运行 —— 执行锁挡不住它。面板仍然在启动前验 IP，\
             一键关闭也能收掉它；想让执行锁真正生效，改用官方安装器或 winget 装一份。"
                .to_string(),
        ),
        None if others > 0 => Some(format!(
            "没找到能直接启动的 Claude Code，但本机有 {others} 份别的程序自带的副本\
             （编辑器扩展、安装器版本库等），它们照样会被上锁。"
        )),
        _ => None,
    };

    Software {
        id: "claude-code",
        name: "Claude Code",
        installed: primary.is_some(),
        version,
        path: primary.map(|i| i.path.clone()),
        advisory,
    }
}

/// 本机全部 Claude Code 副本（不含桌面端存根），给环境页列清单。
pub fn claude_code_installs() -> Vec<super::inventory::Install> {
    use super::inventory::{self, Kind};
    inventory::scan(&inventory::Roots::current())
        .into_iter()
        .filter(|i| i.kind != Kind::DesktopStub)
        .collect()
}

pub async fn claude_desktop() -> Software {
    let dir = local().join("AnthropicClaude");
    let stub = dir.join("claude.exe");
    if stub.exists() {
        // 版本从 app-<版本> 目录名读，比启动一次进程便宜得多。
        // **按数字比**：按字符串比 `1.9.0` 会排在 `1.49585.0` 前面。
        let version = std::fs::read_dir(&dir).ok().and_then(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter_map(|n| n.strip_prefix("app-").map(String::from))
                .max_by_key(|v| super::inventory::version_key(v))
        });
        return Software {
            id: "claude-desktop",
            name: "Claude 桌面端",
            installed: true,
            version,
            path: Some(stub),
            advisory: None,
        };
    }

    // 没有 Squirrel 存根时再看是不是 MSIX 装的（企业分发常见）。
    // 那种装法面板管不了：WindowsApps 下的文件改不了 ACL，也没有存根可以拉起。
    // 如实说出来，别报成「未安装」让人再去装一份。
    if let Some(m) = msix_desktop().await {
        return Software {
            id: "claude-desktop",
            name: "Claude 桌面端",
            installed: true,
            version: m.version,
            path: m.location.map(PathBuf::from),
            advisory: Some(
                "这是 MSIX 方式装的桌面端：执行锁加不到它身上，面板也不能替你启动它，\
                 请从开始菜单打开。看门狗在出口 IP 不对时仍会关掉它。"
                    .into(),
            ),
        };
    }

    Software {
        id: "claude-desktop",
        name: "Claude 桌面端",
        installed: false,
        version: None,
        path: None,
        advisory: None,
    }
}

struct MsixPackage {
    version: Option<String>,
    location: Option<String>,
}

/// 查 MSIX 版桌面端。发布者含 Anthropic、名字含 Claude。
///
/// ⚠ 本机没有 MSIX 版，这段只对着 `Get-AppxPackage` 的文档写，没在实机上见过真包。
/// 查询失败就当没有 —— 这只影响「装没装」的显示，不影响任何锁。
#[cfg(windows)]
async fn msix_desktop() -> Option<MsixPackage> {
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$p = @(Get-AppxPackage | Where-Object { $_.Publisher -like '*Anthropic*' -and $_.Name -like '*Claude*' } | ForEach-Object {
  [pscustomobject]@{ Version = [string]$_.Version; InstallLocation = [string]$_.InstallLocation }
})
ConvertTo-Json -InputObject $p -Compress
"#;
    let out = crate::process::powershell_tokio(SCRIPT)
        .output()
        .await
        .ok()?;
    let v: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).ok()?;
    let first = v.as_array()?.first()?;
    Some(MsixPackage {
        version: first
            .get("Version")
            .and_then(|x| x.as_str())
            .map(String::from),
        location: first
            .get("InstallLocation")
            .and_then(|x| x.as_str())
            .map(String::from),
    })
}

#[cfg(not(windows))]
async fn msix_desktop() -> Option<MsixPackage> {
    None
}

/// Codex CLI 可能落在哪。
///
/// 顺序 = 优先级。**npm 全局排第一** —— 旧版只查 `.local\bin` 和
/// `Programs\codex` 两处，而实机上装的是 npm 全局那份，于是环境页
/// 一直把已装的 Codex 显示成「未安装」。
///
/// `%LOCALAPPDATA%\OpenAI\Codex\bin\<hash>\codex.exe` 的 `<hash>` 是随版本
/// 变的目录名，所以那一层要枚举，不能拼死。
pub fn codex_candidates() -> Vec<PathBuf> {
    let mut out = vec![
        // 面板托管的那份排第一（v0.9.0）：面板自己装的，位置由面板说了算。
        crate::install::managed::exe(crate::install::managed::App::Codex),
        // npm 全局。装的是 .cmd 批处理，不是 exe。
        PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
            .join("npm")
            .join("codex.cmd"),
        home().join(".local").join("bin").join("codex.exe"),
        local().join("Programs").join("codex").join("codex.exe"),
    ];

    // winget（portable zip）：解压在 Packages 下，Links 里放一个 codex.exe 的 shim。
    // 原来这张表里没有它们 —— winget 装的 Codex 检测不到，也锁不到。
    let wg = local().join("Microsoft").join("WinGet");
    if let Ok(rd) = std::fs::read_dir(wg.join("Packages")) {
        for e in rd.filter_map(|e| e.ok()) {
            if e.file_name()
                .to_string_lossy()
                .to_lowercase()
                .starts_with("openai.codex")
            {
                if let Ok(files) = std::fs::read_dir(e.path()) {
                    // 只认主程序。同一个目录里还有 codex-command-runner.exe、
                    // codex-windows-sandbox-setup.exe 两个辅助程序（winget 清单实测），
                    // 按「codex 开头的 exe」认的话会把辅助程序当成 Codex 去启动。
                    out.extend(files.filter_map(|f| f.ok()).map(|f| f.path()).filter(|p| {
                        let n = p
                            .file_name()
                            .map(|n| n.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        n == "codex.exe"
                            || (n.starts_with("codex-") && n.ends_with("-pc-windows-msvc.exe"))
                    }));
                }
            }
        }
    }
    out.push(wg.join("Links").join("codex.exe"));

    // 官方桌面版把 CLI 塞在一个按版本变名的哈希目录下。
    let bin = local().join("OpenAI").join("Codex").join("bin");
    if let Ok(rd) = std::fs::read_dir(&bin) {
        let mut nested: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path().join("codex.exe"))
            .filter(|p| p.exists())
            .collect();
        nested.sort();
        out.append(&mut nested);
    }

    // MSIX（Microsoft Store / winget 的 store 源）。
    out.push(
        local()
            .join("Microsoft")
            .join("WindowsApps")
            .join("codex.exe"),
    );
    out
}

pub async fn codex() -> Software {
    use crate::install::managed;
    let found = codex_candidates().into_iter().find(|p| p.exists());
    let is_managed = found.as_deref().is_some_and(|p| {
        crate::install::inventory::same_path(p, &managed::exe(managed::App::Codex))
    });
    // 托管那份的版本读安装记录 —— Codex 的 exe 里没有版本资源，而它归门禁管时是锁着的，跑不起来。
    //
    // 其余的：这里敢直接跑 exe，是因为那些**不是面板装的**，而且 Codex 默认不在门禁里。
    // 打开「Codex 也归门禁管」之后跑不起来就读不出版本 —— 如实显示「已安装、版本未知」。
    let version = match &found {
        Some(_) if is_managed => {
            managed::record_of(&managed::root(), managed::App::Codex).map(|r| r.version)
        }
        Some(p) => parse_codex_version(codex_raw_version(p).await.as_deref()),
        None => None,
    };
    Software {
        id: "codex",
        name: "Codex CLI",
        installed: found.is_some(),
        version,
        path: found,
        advisory: None,
    }
}

/// 跑一次 `--version`，**批处理要走 `cmd /c`**。
///
/// npm 全局装出来的是 `codex.cmd`，不是 exe。CreateProcess 不认批处理，
/// 直接 `Command::new("codex.cmd")` 拿不到输出 —— 版本号会永远是 None。
async fn codex_raw_version(p: &PathBuf) -> Option<String> {
    let is_batch = p
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));

    let out = if is_batch {
        crate::process::hidden_tokio(tokio::process::Command::new("cmd"))
            .arg("/c")
            .arg(p)
            .arg("--version")
            .output()
            .await
            .ok()?
    } else {
        crate::process::hidden_tokio(tokio::process::Command::new(p))
            .arg("--version")
            .output()
            .await
            .ok()?
    };
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// `codex --version` 打的是 `codex-cli 0.153.4`，取后面那段。
///
/// 拿整行当版本号会让「装没装上」之外的一切比较都失效 ——
/// 升级判断按三段版本比，`codex-cli 0.153.4` 解析不出来。
pub fn parse_codex_version(raw: Option<&str>) -> Option<String> {
    let line = raw?.lines().next()?.trim();
    let tail = line.rsplit(char::is_whitespace).next().unwrap_or(line);
    (!tail.is_empty() && tail.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .then(|| tail.to_string())
}

/// 浏览器只探常见安装位置，用于「是否需要重装 / 语言时区是否同步」的提示。
///
/// ⚠ Chrome 的落点走 [`crate::install::chrome::chrome_exe`]，**不要在这里
/// 再拼一遍**。原来这里只看两个 Program Files 位置，漏掉了 per-user 安装
/// （`%LOCALAPPDATA%\Google\Chrome\Application\chrome.exe`）——
/// 于是 per-user 装的 Chrome 被报成「未安装」，而重装流程会据此直接跳到
/// 「装一个新的」，把使用者已有的那份连同 claude.ai 登录态原地留着。
/// 两处各拼一份路径表，迟早会像这样漂开。
pub fn browsers() -> Vec<Software> {
    let chrome = crate::install::chrome::chrome_exe();
    let edge = edge_exe();

    vec![
        Software {
            id: "chrome",
            name: "Google Chrome",
            installed: chrome.is_some(),
            version: None,
            path: chrome,
            advisory: None,
        },
        Software {
            id: "edge",
            name: "Microsoft Edge",
            installed: edge.is_some(),
            version: None,
            path: edge,
            advisory: None,
        },
    ]
}

/// Edge 的三个落点。原来只看 `Program Files (x86)` —— ARM64 和部分新装的机器
/// 在 `Program Files` 下，还有 per-user 装在 `%LOCALAPPDATA%` 的。
fn edge_exe() -> Option<PathBuf> {
    let rel = ["Microsoft", "Edge", "Application", "msedge.exe"];
    let mut bases: Vec<PathBuf> = ["ProgramFiles(x86)", "ProgramFiles", "ProgramW6432"]
        .iter()
        .filter_map(|k| std::env::var_os(k).map(PathBuf::from))
        .collect();
    bases.push(local());
    bases
        .into_iter()
        .map(|b| rel.iter().fold(b, |acc, r| acc.join(r)))
        .find(|p| p.is_file())
}

// 桌面端进程的识别与关闭搬去了 `killswitch::close_desktop`：
// 旧版在这里把路径拼进 PowerShell 的单引号串再 `-like`，用户名带 `'` 或 `[ ]`
// 就静默失效，而且结果全被丢掉。见那边的说明。

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_version_drops_the_product_name() {
        // `codex --version` 打的是 `codex-cli 0.153.4`。拿整行当版本号，
        // 三段版本比较会直接失效。
        assert_eq!(
            parse_codex_version(Some("codex-cli 0.153.4")).as_deref(),
            Some("0.153.4")
        );
        assert_eq!(
            parse_codex_version(Some("codex-cli 0.153.4\n")).as_deref(),
            Some("0.153.4")
        );
        // 有些构建只打版本号。
        assert_eq!(
            parse_codex_version(Some("0.146.1")).as_deref(),
            Some("0.146.1")
        );
    }

    #[test]
    fn codex_version_returns_none_when_it_is_not_a_version() {
        // 「拿不到版本」和「拿到一句错误信息」必须都算 None，
        // 否则界面上会显示一行报错当版本号。
        assert_eq!(parse_codex_version(None), None);
        assert_eq!(parse_codex_version(Some("")), None);
        assert_eq!(parse_codex_version(Some("command not found")), None);
    }

    #[test]
    fn the_managed_copy_comes_first_then_npm_global() {
        // v0.9.0：面板托管的那份排第一 —— 面板自己装的，位置由面板说了算。
        // 其次仍是 npm 全局：实机上 Codex 就装在那里，旧版不查它，
        // 于是把已装的 Codex 一直显示成「未安装」。
        let c = codex_candidates();
        assert!(
            crate::install::inventory::same_path(
                &c[0],
                &crate::install::managed::exe(crate::install::managed::App::Codex)
            ),
            "托管那份必须排第一：{}",
            c[0].display()
        );
        assert_eq!(
            c[1].file_name().and_then(|n| n.to_str()),
            Some("codex.cmd"),
            "其次是 npm 全局那份"
        );
        assert!(c.len() >= 5, "候选路径不该少于五条，实际 {}", c.len());
    }
}

/// 「环境与安装」页要的一整份本机现状。
///
/// # 为什么它是个结构体而不是 `serde_json::json!{}`
///
/// 原来 `detect_software` 命令返回的是 `serde_json::Value`，前端手抄了一个
/// `SoftwareReport` 接口。那个接口**在 Rust 侧根本不存在** —— 它没有源头，
/// 所以也没有任何东西能发现它抄错了或者过期了。
/// `api.ts:63-72` 的注释里记着一次真实事故：`targets.rs` 加了新布局，
/// 前端类型没跟上，而 `npm run types:check` 看不见手写的那些。
///
/// 现在它有源头了：这个结构体由 ts-rs 导出，两边对不上 CI 当场红。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareReport {
    pub claude_code: Software,
    /// 本机全部 Claude Code 副本（不含桌面端存根）。
    pub claude_code_installs: Vec<super::inventory::Install>,
    pub claude_desktop: Software,
    pub codex: Software,
    pub browsers: Vec<Software>,
}

/// 盘一次本机现状。
pub async fn report() -> SoftwareReport {
    SoftwareReport {
        claude_code: claude_code().await,
        claude_code_installs: claude_code_installs(),
        claude_desktop: claude_desktop().await,
        codex: codex().await,
        browsers: browsers(),
    }
}
