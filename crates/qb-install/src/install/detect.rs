//! 本机环境检测：Claude 桌面端 / Claude Code / 浏览器 / Codex。

use serde::Serialize;
use std::path::{Path, PathBuf};
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
        // 版本从 app-<版本> 目录名读，比启动一次进程便宜得多（位置表那一个函数，
        // Claude 汉化插件要改的目录也从它拿）。
        let version = super::inventory::desktop_app_dir(&local()).map(|(v, _)| v);
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

/// 官方 Gemini CLI（`@google/gemini-cli`）的包目录可能落在哪。
///
/// npm 全局前缀不是只有一个地方：`NPM_CONFIG_PREFIX` 覆盖一切；默认是 `%APPDATA%\npm`；
/// nvm-windows / MSI 装的 Node 把全局包放进 `%ProgramFiles%\nodejs`。**一个都不能假设**
/// —— 这是「开源兼容性」那条规矩（别人的 npm 前缀跟你的不一样）。
/// 顺序 = 优先级：显式前缀 → npm 默认 → Node 安装目录 → 面板托管目录。
pub fn gemini_cli_roots() -> Vec<PathBuf> {
    let pkg = |prefix: PathBuf| {
        prefix
            .join("node_modules")
            .join("@google")
            .join("gemini-cli")
    };
    let env_dir = |k: &str| {
        let v = std::env::var(k).unwrap_or_default();
        (!v.trim().is_empty()).then(|| PathBuf::from(v))
    };
    [
        env_dir("NPM_CONFIG_PREFIX"),
        env_dir("APPDATA").map(|p| p.join("npm")),
        env_dir("ProgramFiles").map(|p| p.join("nodejs")),
        env_dir("LOCALAPPDATA").map(|p| p.join("npm")),
        Some(crate::install::managed::root().join("gemini-cli")),
    ]
    .into_iter()
    .flatten()
    .map(pkg)
    .collect()
}

/// 一个包目录下，官方 CLI 的入口 `.js` 可能叫什么。
///
/// # ⛔ 不写死 `dist\index.js`
///
/// 0.26.0 这里写死的就是 `dist\index.js`，而 npm 上的 `@google/gemini-cli` 把入口放在
/// `bundle\gemini.js`（它 `package.json` 的 `"bin": {"gemini": "bundle/gemini.js"}`）。
/// 猜错的代价不是「找不到」这么轻：0.29.0 把 npm 那条命令修好之后，**包真的装上了**，
/// 而面板回读检测照样落空，于是每次都报「npm 报告成功，而检测仍然找不到入口文件」，
/// 使用者看到的是「Gemini CLI 还是装不上」，软件页也一直显示未安装
/// （审计日志 2026-09-21 02:32 / 18:16 两条）。顺带，酒馆的 Gemini 桥接也因此从没起来过。
///
/// 入口现在**问包自己**（`package.json` 的 `bin`），两种见过的布局只作兜底。
pub fn gemini_cli_entries(pkg_root: &Path) -> Vec<PathBuf> {
    let declared = std::fs::read(pkg_root.join("package.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| match v.get("bin") {
            // `"bin": "bundle/gemini.js"`
            Some(serde_json::Value::String(s)) => Some(s.clone()),
            // `"bin": {"gemini": "bundle/gemini.js"}`；键改了名就取第一条。
            Some(serde_json::Value::Object(m)) => m
                .get("gemini")
                .and_then(|x| x.as_str())
                .or_else(|| m.values().find_map(|x| x.as_str()))
                .map(String::from),
            _ => None,
        })
        // manifest 里写的是 posix 分隔符，拼之前逐段拆开。
        .map(|rel| {
            rel.split('/')
                .fold(pkg_root.to_path_buf(), |p, s| p.join(s))
        });
    let mut out: Vec<PathBuf> = Vec::new();
    for p in declared.into_iter().chain([
        pkg_root.join("bundle").join("gemini.js"),
        pkg_root.join("dist").join("index.js"),
    ]) {
        // `bin` 指的通常就是兜底里的那一条，列两遍只会让诊断输出看着像出了错。
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Gemini CLI 包里哪些文件可能写着它自己的 OAuth 客户端标识（2026-09-23）。
///
/// 跟反重力那条（`antigravity::oauth_client_sources`）同一个套路：联网额度要给 Gemini CLI 的
/// 访问令牌换新时，从**本机装的官方包**里现读签发它的那个客户端的标识 —— 仓库里一个字都不写
/// （CLAUDE.md「联网额度」第 4 条）。只列存在的，按优先级：入口脚本、入口同目录的其余 `.js`
/// （打包器可能拆块，最多 [`MAX_BUNDLE_SIBLINGS`] 个）、没打包的布局里 core 包的 `oauth2.js`。
pub fn gemini_cli_oauth_client_sources() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in gemini_cli_roots() {
        for p in gemini_cli_oauth_client_sources_in(&root) {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// 一个包目录里扫哪些文件。拆块再多也只看这么多个，免得一次刷新读上百个文件。
const MAX_BUNDLE_SIBLINGS: usize = 40;

/// [`gemini_cli_oauth_client_sources`] 的一个包目录那一段（单测喂临时目录）。
pub fn gemini_cli_oauth_client_sources_in(pkg_root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if p.is_file() && !out.contains(&p) {
            out.push(p);
        }
    };
    for entry in gemini_cli_entries(pkg_root) {
        if !entry.is_file() {
            continue;
        }
        push(entry.clone());
        if let Some(dir) = entry.parent() {
            let mut siblings: Vec<PathBuf> = std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "js"))
                .collect();
            siblings.sort();
            for s in siblings.into_iter().take(MAX_BUNDLE_SIBLINGS) {
                push(s);
            }
        }
    }
    push(
        [
            "node_modules",
            "@google",
            "gemini-cli-core",
            "dist",
            "src",
            "code_assist",
            "oauth2.js",
        ]
        .iter()
        .fold(pkg_root.to_path_buf(), |p, s| p.join(s)),
    );
    out
}

/// 官方 Gemini CLI 的入口脚本可能落在哪（0.26.0；0.31.0 改成问包自己）。
///
/// 它是 Node 包，没有原生 exe：`gemini.cmd` 只是 `node <入口>` 的壳。
/// 桥接直接用 `node` 起入口脚本（不经 cmd 那层引号转义），所以这里给的是**入口 `.js`**。
pub fn gemini_cli_candidates() -> Vec<PathBuf> {
    gemini_cli_roots()
        .iter()
        .flat_map(|r| gemini_cli_entries(r))
        .collect()
}

/// 找不到时，把「找过哪些地方」说出来。
///
/// 位置对不上是这条链上最容易犯的错（见 [`gemini_cli_entries`]），而「没找到」
/// 这三个字既不说找过哪、也不说该往哪看。列出来，下一个人一眼就能对照盘上的真实位置。
pub fn gemini_cli_searched() -> String {
    let roots = gemini_cli_roots();
    let shown: Vec<String> = roots
        .iter()
        .take(3)
        .map(|p| p.display().to_string())
        .collect();
    format!(
        "找过：{}{}",
        shown.join("；"),
        if roots.len() > shown.len() {
            format!("（等 {} 处）", roots.len())
        } else {
            String::new()
        }
    )
}

/// `node.exe`：先 `where`，再 `%ProgramFiles%\nodejs`。
pub fn node_exe() -> Option<PathBuf> {
    if let Ok(out) = crate::process::hidden_std(std::process::Command::new("where"))
        .arg("node.exe")
        .output()
    {
        if out.status.success() {
            if let Some(first) = String::from_utf8_lossy(&out.stdout).lines().next() {
                let p = PathBuf::from(first.trim());
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    for k in ["ProgramFiles", "ProgramW6432"] {
        let p = PathBuf::from(std::env::var(k).unwrap_or_default())
            .join("nodejs")
            .join("node.exe");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Gemini CLI 装没装（给软件页）。版本读**包目录**的 `package.json`，不起进程。
///
/// 版本原来是从入口往上数两层取的（`dist\index.js` → `dist` → 包目录）。入口一旦不在
/// 两层深的地方（`bin` 可以指向包根下任何一个文件），数出来的就是别人的 manifest。
/// 现在包目录是已知的那一个，不靠数层数。
pub fn gemini_cli() -> Software {
    let found = gemini_cli_roots().into_iter().find_map(|root| {
        gemini_cli_entries(&root)
            .into_iter()
            .find(|p| p.is_file())
            .map(|entry| (root, entry))
    });
    let version = found.as_ref().and_then(|(root, _)| {
        let v: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("package.json")).ok()?).ok()?;
        v.get("version")?.as_str().map(String::from)
    });
    let entry = found.map(|(_, e)| e);
    Software {
        id: "gemini-cli",
        name: "Gemini CLI",
        installed: entry.is_some(),
        version,
        path: entry,
        advisory: node_exe().is_none().then(|| {
            "找不到 node.exe：Gemini CLI 是 Node 包，酒馆的 Gemini 桥接要用 node 起它。".into()
        }),
    }
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
        // npm 包里真正干活的原生 exe（0.25.0）。`codex.cmd` 只是 `node codex.js` 的壳，
        // 壳锁不住（批处理由 cmd.exe 读进去执行），这一份才是能加 Deny ACE 的本体 ——
        // 0.25.0 之前它不在表里，「挡得住 codex 命令、挡不住内部脚本」那条缺口就是它。
        // 酒馆的 GPT 桥接也优先拿它跑 `codex exec`（exe 不用过 cmd 那层引号转义）。
        PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
            .join("npm")
            .join("node_modules")
            .join("@openai")
            .join("codex")
            .join("node_modules")
            .join("@openai")
            .join("codex-win32-x64")
            .join("vendor")
            .join("x86_64-pc-windows-msvc")
            .join("bin")
            .join("codex.exe"),
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

/// Codex 桌面端（Store 包）。位置表只有 `codex_desktop` 那一张：`Get-AppxPackage` 认包族名，
/// exe 是 `app\ChatGPT.exe`（新）或 `app\Codex.exe`（旧）。
///
/// 查询失败（PowerShell 起不来）如实报「查不到」进 `advisory`，**不降级成「未安装」**。
pub async fn codex_desktop() -> Software {
    let found = tokio::task::spawn_blocking(super::codex_desktop::detect)
        .await
        .map_err(|e| crate::error::GateError::Other(e.to_string()))
        .and_then(|r| r);
    match found {
        Ok(d) => codex_desktop_software(d),
        Err(e) => Software {
            id: "codex-desktop",
            name: "Codex 桌面端",
            installed: false,
            version: None,
            path: None,
            advisory: Some(format!("查不到 Store 包的状态（不等于没装）：{e}")),
        },
    }
}

/// 把 `Get-AppxPackage` 的盘点结果折成一条 [`Software`]。**纯函数，有单测。**
///
/// # ⛔ `installed` 与 `version` 不许各算各的
///
/// 0.28.0 之前这里是 `installed: d.executable.is_some()` 配 `version: d.version`，
/// 两个字段来自探测脚本里**两个互不相干的分支**：`$pkg.Version` 只要包在册就有，
/// `$exe` 还要那两个写死的 exe 名之一真的存在。于是
/// `installed:false, version:Some("26.x")` 是可达的 —— 界面上 Pill 写「未安装」、
/// 版本行写着一个真版本号、路径行还断言「Get-AppxPackage 里没有 OpenAI.Codex」。
/// 三句话互相矛盾，而且最后那句是**假的**。
///
/// 真相是：**包在册就是装了**，只是找不到可执行文件 —— 那正是 §7.42 那种
/// 「注册失效」的样子，而且有现成的下一步（强制重装）。报成「未安装」会把人引到
/// 「去 Store 装一个」，而他早就装了。
fn codex_desktop_software(d: super::codex_desktop::CodexDesktop) -> Software {
    let exe_missing = d.executable.is_none() && d.version.is_some();
    Software {
        id: "codex-desktop",
        name: "Codex 桌面端",
        installed: d.executable.is_some() || d.version.is_some(),
        version: d.version,
        path: d.executable.map(PathBuf::from),
        advisory: exe_missing.then(|| {
            "Store 包在册，但找不到它的可执行文件 —— 多半是注册坏了。\
             点「更新 / 重装」并勾上「强制重装」可以修。"
                .to_string()
        }),
    }
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

    /// 官方 `@google/gemini-cli` 的 manifest 就是这个形状（2026-09-21 实机核过 0.60.0）。
    fn write_pkg(dir: &Path, bin: &str, entry_rel: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("package.json"),
            format!(r#"{{"name":"@google/gemini-cli","version":"0.60.0","bin":{bin}}}"#),
        )
        .unwrap();
        let entry = entry_rel
            .split('/')
            .fold(dir.to_path_buf(), |p, s| p.join(s));
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        std::fs::write(&entry, "#!/usr/bin/env node\n").unwrap();
    }

    #[test]
    fn the_entry_comes_from_the_package_manifest_not_a_guess() {
        // 0.26.0–0.30.0 写死的是 `dist\index.js`，而官方包的入口一直是
        // `bundle\gemini.js` —— 于是包装上了、面板照样报「找不到入口文件」。
        let tmp = std::env::temp_dir().join(format!("qb-gemini-entry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        write_pkg(&tmp, r#"{"gemini":"bundle/gemini.js"}"#, "bundle/gemini.js");

        let found = gemini_cli_entries(&tmp).into_iter().find(|p| p.is_file());
        assert_eq!(
            found.as_deref(),
            Some(tmp.join("bundle").join("gemini.js").as_path()),
            "入口要问 package.json 的 bin，不能靠猜目录名"
        );
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// 换新 Gemini CLI 令牌时从哪找它的客户端标识：入口排第一，同目录的拆块跟在后面，
    /// 不是 `.js` 的不扫，不存在的一律不列。**只建临时目录，不碰真实安装。**
    #[test]
    fn the_client_identity_sources_start_at_the_entry_and_only_list_real_js_files() {
        let tmp = std::env::temp_dir().join(format!("qb-gemini-oauth-src-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        write_pkg(&tmp, r#"{"gemini":"bundle/gemini.js"}"#, "bundle/gemini.js");
        std::fs::write(tmp.join("bundle").join("chunk-a.js"), "x").unwrap();
        std::fs::write(tmp.join("bundle").join("readme.md"), "x").unwrap();

        let found = gemini_cli_oauth_client_sources_in(&tmp);
        assert_eq!(found.first(), Some(&tmp.join("bundle").join("gemini.js")));
        assert!(found.contains(&tmp.join("bundle").join("chunk-a.js")));
        assert!(!found.iter().any(|p| p.ends_with("readme.md")));
        assert_eq!(found.len(), 2, "入口只列一次：{found:?}");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn a_string_bin_and_the_old_layout_both_still_resolve() {
        // `"bin": "dist/index.js"`（字符串形）与旧布局都得认 —— 兜底不是摆设：
        // 包换过一次布局，就会再换第二次。
        let tmp = std::env::temp_dir().join(format!("qb-gemini-str-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        write_pkg(&tmp, r#""dist/index.js""#, "dist/index.js");
        assert_eq!(
            gemini_cli_entries(&tmp)
                .into_iter()
                .find(|p| p.is_file())
                .as_deref(),
            Some(tmp.join("dist").join("index.js").as_path())
        );

        // manifest 读不出来时，两种见过的布局仍在候选里。
        std::fs::remove_file(tmp.join("package.json")).unwrap();
        assert_eq!(
            gemini_cli_entries(&tmp)
                .into_iter()
                .find(|p| p.is_file())
                .as_deref(),
            Some(tmp.join("dist").join("index.js").as_path())
        );
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn the_npm_prefix_is_not_assumed_to_be_appdata_npm() {
        // 开源兼容性：`NPM_CONFIG_PREFIX`、nvm-windows（`%ProgramFiles%\nodejs`）
        // 各自把全局包放在别处。只查 `%APPDATA%\npm` 的话，那些机器上「装了也认不出」。
        let roots = gemini_cli_roots();
        assert!(
            roots.len() >= 2,
            "候选前缀不该只有一个，实际 {}",
            roots.len()
        );
        assert!(
            roots
                .iter()
                .all(|r| r.ends_with(Path::new("node_modules/@google/gemini-cli"))),
            "每条候选都该指向包目录本身：{roots:?}"
        );
        // 说得出找过哪 —— 「没找到」三个字不该是唯一的线索。
        assert!(gemini_cli_searched().starts_with("找过："));
    }

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
    /// Codex 桌面端（Microsoft Store 包，0.28.0）。跟 `codex` 那个 CLI 是两个东西：
    /// 账户页起的是它，直装（`codex_store`）装的也是它。
    pub codex_desktop: Software,
    /// 反重力 Hub（0.26.0）。
    pub antigravity: Software,
    /// 反重力 IDE（0.26.0）。
    pub antigravity_ide: Software,
    /// 官方 Gemini CLI（0.26.0，酒馆的 Gemini 桥接用）。
    pub gemini_cli: Software,
    pub browsers: Vec<Software>,
}

/// 盘一次本机现状。
pub async fn report() -> SoftwareReport {
    let (antigravity, antigravity_ide) = antigravity().await;
    SoftwareReport {
        claude_code: claude_code().await,
        claude_code_installs: claude_code_installs(),
        claude_desktop: claude_desktop().await,
        codex: codex().await,
        codex_desktop: codex_desktop().await,
        antigravity,
        antigravity_ide,
        gemini_cli: gemini_cli(),
        browsers: browsers(),
    }
}

/// 反重力两个产品的检测。位置表在 `install::antigravity`；版本读主程序的
/// `VersionInfo.ProductVersion`（Hub 的 electron-builder 与 IDE 的安装器都写它），
/// 一次 PowerShell 两个都读，读不出来只影响显示、不影响任何锁。
pub async fn antigravity() -> (Software, Software) {
    use super::antigravity::Product;
    let local = local();
    let launchers: Vec<(Product, PathBuf)> = Product::ALL
        .into_iter()
        .map(|p| (p, p.launcher(&local)))
        .collect();
    let versions = product_versions(
        &launchers
            .iter()
            .filter(|(_, p)| p.is_file())
            .map(|(_, p)| p.clone())
            .collect::<Vec<_>>(),
    )
    .await;
    let mut out = launchers.into_iter().map(|(product, exe)| {
        let installed = exe.is_file();
        Software {
            id: product.key(),
            name: product.label(),
            installed,
            version: if installed {
                versions.get(&exe.display().to_string()).cloned().flatten()
            } else {
                None
            },
            path: installed.then_some(exe),
            advisory: None,
        }
    });
    let hub = out.next().expect("Product::ALL 有两个");
    let ide = out.next().expect("Product::ALL 有两个");
    (hub, ide)
}

/// 一批 exe 的 `ProductVersion`。**键是传进去的路径原样**。取不到的是 `None`。
#[cfg(windows)]
async fn product_versions(exes: &[PathBuf]) -> std::collections::HashMap<String, Option<String>> {
    let mut out = std::collections::HashMap::new();
    if exes.is_empty() {
        return out;
    }
    // 单引号在 Windows 文件名里合法，塞进 PowerShell 单引号串前要按它的规矩翻倍。
    let list = exes
        .iter()
        .map(|p| format!("'{}'", p.display().to_string().replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        "$ErrorActionPreference = 'Stop'
$rows = @(foreach ($f in @({list})) {{
  $v = $null
  try {{ $v = [string](Get-Item -LiteralPath $f -ErrorAction Stop).VersionInfo.ProductVersion }} catch {{ }}
  [pscustomobject]@{{ Path = $f; Version = $v }}
}})
ConvertTo-Json -InputObject $rows -Compress"
    );
    let Ok(o) = crate::process::powershell_tokio(&script).output().await else {
        return out;
    };
    let text = String::from_utf8_lossy(&o.stdout);
    let Ok(rows) = serde_json::from_str::<Vec<serde_json::Value>>(text.trim()) else {
        return out;
    };
    for r in rows {
        let Some(path) = r.get("Path").and_then(|v| v.as_str()) else {
            continue;
        };
        let version = r
            .get("Version")
            .and_then(|v| v.as_str())
            .map(|v| four_part_to_three(v.trim()))
            .filter(|v| !v.is_empty());
        out.insert(path.to_string(), version);
    }
    out
}

/// `2.15.0.0`（VersionInfo 的四段）→ `2.15.0`。只去掉第四段且它是 0 的情形，别的原样。
#[cfg_attr(not(windows), allow(dead_code))]
fn four_part_to_three(v: &str) -> String {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() == 4 && parts[3] == "0" {
        parts[..3].join(".")
    } else {
        v.to_string()
    }
}

#[cfg(not(windows))]
async fn product_versions(_exes: &[PathBuf]) -> std::collections::HashMap<String, Option<String>> {
    std::collections::HashMap::new()
}

#[cfg(test)]
mod antigravity_tests {
    use super::*;
    /// VersionInfo 给的是四段（`2.15.0.0`），界面上显示三段；不是 `.0` 结尾的四段原样保留。
    #[test]
    fn version_info_four_parts_display_as_three() {
        assert_eq!(four_part_to_three("2.15.0.0"), "2.15.0");
        assert_eq!(four_part_to_three("2.15.0.7"), "2.15.0.7");
        assert_eq!(four_part_to_three("1.2.3"), "1.2.3");
        assert_eq!(four_part_to_three(""), "");
    }
}

#[cfg(test)]
mod codex_desktop_tests {
    use super::*;
    use crate::install::codex_desktop::CodexDesktop;

    fn found(exe: Option<&str>, version: Option<&str>) -> CodexDesktop {
        CodexDesktop {
            executable: exe.map(String::from),
            version: version.map(String::from),
            running: false,
            processes: Vec::new(),
        }
    }

    /// ⛔ **「没装」和「有版本号」不许同时成立。**
    ///
    /// 这是 0.28.0 之前软件页那三句互相矛盾的话的根源（Pill 写未安装、版本行写着
    /// 一个真版本号、路径行断言包不在册）。`installed` 与 `version` 来自探测脚本里
    /// 两个互不相干的分支，谁都没错，错在没人负责让它们对上。
    #[test]
    fn a_registered_package_is_installed_even_when_the_exe_is_missing() {
        let s = codex_desktop_software(found(None, Some("26.915.4065.0")));
        assert!(s.installed, "包在册就是装了");
        assert_eq!(s.version.as_deref(), Some("26.915.4065.0"));
        assert!(s.path.is_none());
        // 并且要说得出下一步做什么，不是一片空白。
        let advisory = s.advisory.expect("找不到 exe 要有说明");
        assert!(advisory.contains("强制重装"), "{advisory}");
    }

    #[test]
    fn nothing_registered_is_plainly_not_installed() {
        let s = codex_desktop_software(found(None, None));
        assert!(!s.installed);
        assert!(s.version.is_none());
        assert!(s.advisory.is_none());
    }

    #[test]
    fn a_normal_install_reports_path_and_version_with_no_advisory() {
        let s = codex_desktop_software(found(Some(r"C:\x\app\Codex.exe"), Some("26.1")));
        assert!(s.installed);
        assert_eq!(s.version.as_deref(), Some("26.1"));
        assert!(s.path.is_some());
        assert!(s.advisory.is_none());
    }

    /// 反过来的那一半：有版本号 ⇒ 一定 installed。挨个形状扫一遍。
    #[test]
    fn a_version_always_implies_installed() {
        for exe in [None, Some(r"C:\x\app\ChatGPT.exe")] {
            for version in [None, Some("26.1")] {
                let s = codex_desktop_software(found(exe, version));
                if s.version.is_some() {
                    assert!(s.installed, "有版本号却报未安装：exe={exe:?}");
                }
                if s.path.is_some() {
                    assert!(s.installed, "有路径却报未安装：version={version:?}");
                }
            }
        }
    }
}
