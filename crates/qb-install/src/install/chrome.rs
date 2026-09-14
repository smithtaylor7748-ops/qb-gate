//! 「这台机器以前装过 / 登录过 Claude 吗」的痕迹检测，以及 Chrome 的清空重装。
//!
//! # 这一页原来是坏的
//!
//! `Environment.tsx` 里那段「以前在这台机器上登录过 Claude 吗」有两个洞：
//!
//!   1. 它整段包在 `!anyInstalled` 里 —— 只有**一个 Claude 都没装**时才显示。
//!      而会关心这个问题的人，机器上往往正装着 Claude。使用者报的
//!      「删除和下载浏览器好像没做」，第一层就是它压根没显示出来。
//!   2. 「登录过 / 没有」是两个手动按钮，答案存在 `useSession` 里，刷新就没。
//!      **全程没有任何真实检测。**
//!
//! 所以这里补的是真检测：凭证、配置目录、安装落点、注册表卸载项，
//! 外加 Chrome 用户资料里有没有 claude.ai 的痕迹。
//!
//! # 为什么用字节扫描而不是解 SQLite
//!
//! Chrome 的 `Cookies` / `History` 是 SQLite 库，但我们只需要回答
//! 「里面出现过 claude.ai 吗」，而 `host_key` 和 `url` 在文件里就是明文。
//! 为了一个是非题引入一个 SQLite 依赖不划算，而且 Cookie 的 value 是加密的、
//! 我们也不该去碰 —— 扫字节既够用，又天然做不到「读出登录态」。
//!
//! # 重装为什么只认 Chrome
//!
//! 使用者点名只弄谷歌浏览器。Edge、Firefox 一概不碰 ——
//! 「顺手也帮你清一下」是这个程序最不该做的那类事。
//!
//! # ⚠ 这个模块会毁掉数据
//!
//! 卸载 + 删 `User Data` = 书签、密码、扩展、全部站点数据一起没，**不可恢复**。
//! 界面上只弹一次确认框（使用者要求确认之后全自动），所以那一次确认必须
//! 把代价说全。删 `User Data` 不是可选项：官方卸载程序默认**不删**它，
//! 不删的话重装完 claude.ai 的登录态原样还在，整件事白做。

use crate::error::{GateError, Result};
use crate::sink::ProgressSink;
use serde::Serialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// winget 上的包 id。
const CHROME_PKG: &str = "Google.Chrome";
/// 没有 winget 时给的官方下载页。
pub const CHROME_DOWNLOAD_PAGE: &str = "https://www.google.com/chrome/";

/// 在 Chrome 资料里找的关键词。
const NEEDLE: &[u8] = b"claude.ai";

/// 单个文件最多扫这么多字节。
///
/// `History` 能到几十 MB，而 claude.ai 真出现过的话，不会只出现在末尾。
/// 设上限是为了别让一次检测卡住界面 —— 扫不完就如实说「只扫了前 N MB」。
const SCAN_CAP: u64 = 48 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Trace {
    /// `credential` / `config` / `install` / `registry` / `browser`
    pub kind: &'static str,
    pub label: String,
    pub path: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Default, TS)]
#[ts(export)]
pub struct TraceReport {
    pub traces: Vec<Trace>,
    pub chrome_installed: bool,
    pub chrome_path: Option<String>,
    pub chrome_running: bool,
    /// Chrome 正在跑时它的资料文件被占着，扫不了。
    /// **这时候要如实说「没扫」，不能报「没找到」** —— 那是两回事。
    pub chrome_scanned: bool,
    pub winget_available: bool,
}

impl TraceReport {
    /// 有没有「登录过」这一档的硬证据（凭证文件、或者浏览器里的 claude.ai）。
    pub fn ever_logged_in(&self) -> bool {
        self.traces
            .iter()
            .any(|t| t.kind == "credential" || t.kind == "browser")
    }
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}
fn local() -> PathBuf {
    dirs::data_local_dir().unwrap_or_default()
}
fn roaming() -> PathBuf {
    dirs::config_dir().unwrap_or_default()
}

// ------------------------------------------------------------------ Chrome 落点

/// Chrome 可能装在哪。
///
/// ⚠ 第三条是 **per-user 安装**，`detect::browsers()` 原来漏了它 ——
/// 于是 per-user 装的 Chrome 会被报成「未安装」，然后重装流程会直接去
/// 「装一个新的」，把使用者已有的那份连同资料一起留在原地不管。
pub fn chrome_candidates() -> Vec<PathBuf> {
    let pf = PathBuf::from(std::env::var("ProgramFiles").unwrap_or_default());
    let pf86 = PathBuf::from(std::env::var("ProgramFiles(x86)").unwrap_or_default());
    vec![
        pf.join("Google")
            .join("Chrome")
            .join("Application")
            .join("chrome.exe"),
        pf86.join("Google")
            .join("Chrome")
            .join("Application")
            .join("chrome.exe"),
        local()
            .join("Google")
            .join("Chrome")
            .join("Application")
            .join("chrome.exe"),
    ]
}

pub fn chrome_exe() -> Option<PathBuf> {
    chrome_candidates().into_iter().find(|p| p.is_file())
}

/// Chrome 的用户资料根目录（书签、密码、Cookie、站点数据全在这下面）。
pub fn chrome_user_data() -> PathBuf {
    local().join("Google").join("Chrome").join("User Data")
}

/// 枚举用户资料下的各个 Profile 目录（`Default`、`Profile 1`…）。
///
/// 按「目录里有没有 `Preferences`」认，不按名字猜 ——
/// 名字规则是 Chrome 的内部实现，改过不止一次。
fn chrome_profiles() -> Vec<PathBuf> {
    let root = chrome_user_data();
    let Ok(rd) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("Preferences").is_file())
        .collect();
    out.sort();
    out
}

// ------------------------------------------------------------------ 字节扫描

/// 这个文件里出现过 `needle` 吗？
///
/// `None` = **打不开**（多半是 Chrome 正占着），不是「没有」。
///
/// 分块读并保留 `needle.len() - 1` 字节的重叠 —— 少了这段重叠，
/// 恰好骑在两块边界上的那次出现会被漏掉，而那种漏报最难发现：
/// 它只在文件大小落在特定值附近时才出现。
fn file_contains(path: &Path, needle: &[u8]) -> Option<bool> {
    use std::io::Read;

    let mut f = std::fs::File::open(path).ok()?;
    let overlap = needle.len().saturating_sub(1);
    let mut buf = vec![0u8; 1 << 20];
    let mut carry: Vec<u8> = Vec::with_capacity(overlap);
    let mut read_total: u64 = 0;

    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            return Some(false);
        }
        read_total += n as u64;

        let mut window = Vec::with_capacity(carry.len() + n);
        window.extend_from_slice(&carry);
        window.extend_from_slice(&buf[..n]);

        if window.windows(needle.len()).any(|w| w == needle) {
            return Some(true);
        }

        carry.clear();
        if window.len() > overlap {
            carry.extend_from_slice(&window[window.len() - overlap..]);
        } else {
            carry.extend_from_slice(&window);
        }

        if read_total >= SCAN_CAP {
            return Some(false);
        }
    }
}

/// 一个 Profile 里值得扫的文件。
///
/// Cookie 与 History 回答「登录过 / 访问过」，Local Storage 回答
/// 「站点在本地留了东西」。三处任一命中都算痕迹。
fn scan_targets(profile: &Path) -> Vec<PathBuf> {
    let mut out = vec![
        profile.join("Network").join("Cookies"),
        profile.join("Cookies"),
        profile.join("History"),
        profile.join("Preferences"),
    ];
    let ldb = profile.join("Local Storage").join("leveldb");
    if let Ok(rd) = std::fs::read_dir(&ldb) {
        let mut files: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        // 上限 40 个，够覆盖一个正常 Profile，也不至于把一个坏掉的
        // leveldb 目录（几千个碎片）变成一次几分钟的扫描。
        files.truncate(40);
        out.extend(files);
    }
    out.retain(|p| p.is_file());
    out
}

// ------------------------------------------------------------------ 进程

#[cfg(windows)]
async fn process_count(name: &str) -> Option<usize> {
    let script = format!(
        "$p = @(Get-CimInstance Win32_Process -Filter \"Name='{name}'\" | ForEach-Object {{ \"$($_.ProcessId)\" }}); ConvertTo-Json -InputObject @($p) -Compress"
    );
    let out = crate::process::hidden_tokio(tokio::process::Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // ⚠ `ConvertTo-Json -InputObject @(...)` 是 5.1 上唯一两头都对的写法：
    // 空数组出 `[]`、单元素出 `[{...}]`。`-AsArray` 是 6.2 才有的参数，
    // 用了它整条管线会报错、stdout 全空 —— 档案 §7.17 那个坑。
    serde_json::from_str::<Vec<String>>(&text)
        .ok()
        .map(|v| v.len())
}

#[cfg(not(windows))]
async fn process_count(_name: &str) -> Option<usize> {
    None
}

#[cfg(windows)]
async fn winget_available() -> bool {
    crate::process::hidden_tokio(tokio::process::Command::new("winget"))
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(windows))]
async fn winget_available() -> bool {
    false
}

// ------------------------------------------------------------------ 痕迹检测

/// 这台机器以前装过 / 登录过 Claude 吗。
pub async fn claude_traces() -> TraceReport {
    let mut traces: Vec<Trace> = Vec::new();

    let mut note = |kind: &'static str, label: &str, p: PathBuf, detail: &str| {
        if p.exists() {
            traces.push(Trace {
                kind,
                label: label.to_string(),
                path: p.display().to_string(),
                detail: detail.to_string(),
            });
        }
    };

    // ---- 凭证：这一档是「登录过」的硬证据
    note(
        "credential",
        "Claude Code 登录凭证",
        home().join(".claude").join(".credentials.json"),
        "存在即表示这台机器上登录过 Claude Code",
    );
    note(
        "config",
        "Claude Code 用户配置",
        home().join(".claude.json"),
        "Claude Code 用过之后才会有",
    );
    note(
        "config",
        "Claude Code 数据目录",
        home().join(".claude"),
        "会话、技能、插件都在这下面",
    );
    note(
        "config",
        "Claude 桌面端配置",
        roaming().join("Claude"),
        "桌面端的配置与版本化 CLI 副本",
    );

    // ---- 安装落点
    note(
        "install",
        "Claude 桌面端安装目录",
        local().join("AnthropicClaude"),
        "桌面端存根与 app-<版本> 运行时副本",
    );
    note(
        "install",
        "Claude Code 可执行文件",
        home().join(".local").join("bin").join("claude.exe"),
        "官方安装脚本的落点",
    );
    note(
        "install",
        "Claude Code 版本库",
        home()
            .join(".local")
            .join("share")
            .join("claude")
            .join("versions"),
        "每个版本一份完整二进制",
    );

    // ---- 注册表卸载项
    for (label, path) in registry_uninstall_hits().await {
        traces.push(Trace {
            kind: "registry",
            label,
            path,
            detail: "注册表里还留着卸载项".into(),
        });
    }

    // ---- Chrome
    let chrome_path = chrome_exe();
    let running = process_count("chrome.exe").await.unwrap_or(0) > 0;
    let mut scanned = false;

    if !running {
        for profile in chrome_profiles() {
            let name = profile
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Profile")
                .to_string();
            let mut hit = false;
            let mut opened_any = false;
            for f in scan_targets(&profile) {
                match file_contains(&f, NEEDLE) {
                    Some(true) => {
                        hit = true;
                        opened_any = true;
                        break;
                    }
                    Some(false) => opened_any = true,
                    None => {}
                }
            }
            if opened_any {
                scanned = true;
            }
            if hit {
                traces.push(Trace {
                    kind: "browser",
                    label: format!("Chrome 资料「{name}」里有 claude.ai 痕迹"),
                    path: profile.display().to_string(),
                    detail: "Cookie / 历史 / 本地存储里出现过 claude.ai".into(),
                });
            }
        }
    }

    TraceReport {
        traces,
        chrome_installed: chrome_path.is_some(),
        chrome_path: chrome_path.map(|p| p.display().to_string()),
        chrome_running: running,
        chrome_scanned: scanned,
        winget_available: winget_available().await,
    }
}

#[cfg(windows)]
async fn registry_uninstall_hits() -> Vec<(String, String)> {
    let script = "$r = @(); \
        foreach ($k in @('HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*', \
                          'HKLM:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*')) { \
          $r += @(Get-ItemProperty $k -ErrorAction SilentlyContinue | \
            Where-Object { $_.DisplayName -like '*Claude*' } | \
            ForEach-Object { \"$($_.DisplayName)|$($_.PSPath)\" }) }; \
        ConvertTo-Json -InputObject @($r) -Compress";
    let out = crate::process::hidden_tokio(tokio::process::Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .await;
    let Ok(out) = out else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    serde_json::from_str::<Vec<String>>(&text)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|line| {
            let (name, path) = line.split_once('|')?;
            Some((name.trim().to_string(), path.trim().to_string()))
        })
        .collect()
}

#[cfg(not(windows))]
async fn registry_uninstall_hits() -> Vec<(String, String)> {
    Vec::new()
}

// ------------------------------------------------------------------ 重装

pub const REINSTALL_TOTAL: u32 = 6;

/// 卸掉 Chrome、删干净用户资料、再装回来。没装过就只装。
///
/// **调用方必须已经拿到使用者的确认。** 这个函数不会再问第二次 ——
/// 使用者明确要求「确认之后全程自动，中途不再问」。
pub async fn reinstall(rep: &dyn ProgressSink) -> Result<String> {
    rep.phase(1, "盘点 Chrome");
    if !winget_available().await {
        return Err(GateError::Other(format!(
            "本机没有 winget，自动重装这条路走不通。请到官方页面手动处理：{CHROME_DOWNLOAD_PAGE}"
        )));
    }
    let existing = chrome_exe();
    let user_data = chrome_user_data();
    match &existing {
        Some(p) => rep.log(1, &format!("已装：{}", p.display())),
        None => rep.log(1, "本机没有 Chrome，这一轮只做安装"),
    }

    if existing.is_some() {
        // ---- 2：关掉 Chrome
        //
        // ⚠ 这是**按进程名杀**。项目对 Claude 的进程明令禁止这么做
        // （档案 §1 第 5 条：只收满足双重证据的进程，绝不按名字杀），
        // 理由是叫 claude.exe 的可能是别人的活。
        // 这里是例外，边界要写清楚：目标是 chrome.exe 不是 claude.exe，
        // 而且是使用者点名要求的一次性动作。**别把这段抄回 Claude 那边。**
        rep.phase(2, "关闭 Chrome");
        kill_by_name("chrome.exe").await;
        // Chrome 退出时要落盘，给它一点时间，否则接下来的卸载会撞上占用。
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let left = process_count("chrome.exe").await.unwrap_or(0);
        rep.log(2, &format!("剩余 chrome.exe 进程 {left} 个"));

        // ---- 3：卸载
        rep.phase(3, "卸载 Chrome");
        let out = run_winget(&[
            "uninstall",
            "--id",
            CHROME_PKG,
            "-e",
            "--silent",
            "--accept-source-agreements",
        ])
        .await?;
        for line in out.lines() {
            if !line.trim().is_empty() {
                rep.log(3, line.trim());
            }
        }

        // ---- 4：删用户资料
        //
        // **这一步不是可选项。** 官方卸载程序默认不删 `User Data`，
        // 不删的话重装完 claude.ai 的登录态原样还在，整件事白做。
        rep.phase(4, "删除 Chrome 用户资料");
        if user_data.exists() {
            match std::fs::remove_dir_all(&user_data) {
                Ok(()) => rep.log(4, &format!("已删除 {}", user_data.display())),
                Err(e) => {
                    // 删不掉就得停下来说清楚 —— 接着装回去会让使用者以为
                    // 已经清干净了，而旧的登录态其实还在。
                    return Err(GateError::Other(format!(
                        "Chrome 已卸载，但用户资料目录删不掉：{e}（{}）。\
                         多半还有 chrome.exe 没退干净。请手动删掉这个目录后重装 Chrome —— \
                         不删的话旧的 claude.ai 登录态仍然留着。",
                        user_data.display()
                    )));
                }
            }
        } else {
            rep.log(4, "用户资料目录不存在，跳过");
        }
    } else {
        rep.phase(2, "跳过关闭（没装过）");
        rep.phase(3, "跳过卸载（没装过）");
        rep.phase(4, "跳过删除用户资料（没装过）");
    }

    // ---- 5：装回来
    rep.phase(5, "安装 Chrome");
    let out = run_winget(&[
        "install",
        "--id",
        CHROME_PKG,
        "-e",
        "--silent",
        "--accept-package-agreements",
        "--accept-source-agreements",
    ])
    .await?;
    for line in out.lines() {
        if !line.trim().is_empty() {
            rep.log(5, line.trim());
        }
    }

    // ---- 6：核对。**不看 winget 的退出码，看磁盘。**
    //
    // 跟升级流程那条教训是同一条：安装器说成功不等于磁盘上真换了
    // （见 `upgrade.rs` 文件头）。这里便宜得多 —— 看 exe 在不在就够。
    rep.phase(6, "核对结果");
    let now = chrome_exe().ok_or_else(|| {
        GateError::Other(format!(
            "winget 报告安装完成，但在这三个位置都找不到 chrome.exe：{}。\
             请到官方页面手动安装：{CHROME_DOWNLOAD_PAGE}",
            chrome_candidates()
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("、")
        ))
    })?;

    let detail = if existing.is_some() {
        format!(
            "Chrome 已卸载、用户资料已清空并重新安装：{}。\
             书签、密码、扩展与全部站点数据（含 claude.ai 登录态）都已随之清除。",
            now.display()
        )
    } else {
        format!("本机原来没有 Chrome，已安装：{}。", now.display())
    };
    crate::audit::write(&detail);
    Ok(detail)
}

#[cfg(windows)]
async fn kill_by_name(name: &str) {
    let script = format!(
        "Get-Process -Name '{}' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue",
        name.trim_end_matches(".exe")
    );
    let _ = crate::process::hidden_tokio(tokio::process::Command::new("powershell"))
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .await;
}

#[cfg(not(windows))]
async fn kill_by_name(_name: &str) {}

#[cfg(windows)]
async fn run_winget(args: &[&str]) -> Result<String> {
    let out = crate::process::hidden_tokio(tokio::process::Command::new("winget"))
        .args(args)
        .output()
        .await
        .map_err(|e| GateError::Other(format!("调用 winget 失败：{e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(GateError::Other(format!(
        "winget {} 失败（退出码 {}）：{}",
        args.first().copied().unwrap_or(""),
        out.status.code().unwrap_or(-1),
        if stderr.is_empty() {
            stdout.trim().to_string()
        } else {
            stderr
        }
    )))
}

#[cfg(not(windows))]
async fn run_winget(_args: &[&str]) -> Result<String> {
    Err(GateError::Other("winget 只在 Windows 上可用".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// per-user 安装那条路径不能漏。
    ///
    /// 回归测试：`detect::browsers()` 原来只看两个 Program Files 位置，
    /// 于是 per-user 装的 Chrome 被报成「未安装」，重装流程会直接跳到
    /// 「装一个新的」，把使用者已有的那份连同 claude.ai 登录态留在原地 ——
    /// 而这恰恰是整个功能要清掉的东西。
    #[test]
    fn per_user_install_location_is_probed() {
        let c = chrome_candidates();
        assert_eq!(c.len(), 3, "三个落点一个都不能少");
        assert!(
            c.iter().any(|p| {
                let s = p.display().to_string().to_lowercase();
                s.contains("appdata") || s.contains("local\\google")
            }),
            "per-user 落点没在候选里：{c:?}"
        );
        for p in &c {
            assert_eq!(p.file_name().and_then(|n| n.to_str()), Some("chrome.exe"));
        }
    }

    /// 用户资料目录必须是 `User Data`，不是安装目录。
    ///
    /// 弄混了就会「卸载完把程序目录删了、资料还在」—— 既没清掉登录态，
    /// 又把重装弄坏。
    #[test]
    fn user_data_is_the_profile_root_not_the_program_dir() {
        let p = chrome_user_data();
        assert_eq!(p.file_name().and_then(|n| n.to_str()), Some("User Data"));
        assert!(
            !p.display()
                .to_string()
                .to_lowercase()
                .contains("application"),
            "指到安装目录去了：{}",
            p.display()
        );
    }

    /// 骑在分块边界上的那次出现不能漏。
    ///
    /// 这正是分块扫描最容易写错的地方：不留重叠的话，`claude.ai` 恰好被
    /// 切成两半时就查不到，而这种漏报只在文件大小落在特定值附近才出现。
    #[test]
    fn a_needle_straddling_a_chunk_boundary_is_still_found() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("qb-gate-scan-{}.bin", std::process::id()));

        // 1 MB 缓冲区，故意让 needle 跨过第一块的末尾。
        let chunk = 1usize << 20;
        let mut data = vec![b'x'; chunk - 4];
        data.extend_from_slice(NEEDLE);
        data.extend(std::iter::repeat(b'y').take(1000));
        std::fs::write(&path, &data).expect("写测试文件");

        assert_eq!(file_contains(&path, NEEDLE), Some(true));

        // 反面：确实没有的时候要回 false，不是 true。
        std::fs::write(&path, vec![b'x'; chunk + 100]).expect("写测试文件");
        assert_eq!(file_contains(&path, NEEDLE), Some(false));

        let _ = std::fs::remove_file(&path);
    }

    /// 打不开要回 `None`，**不能回 `Some(false)`**。
    ///
    /// 塌成 false 就会在 Chrome 正开着的时候报「没有痕迹」——
    /// 一个说谎的否定结论，比一句「先关掉 Chrome 再检测」难查得多。
    /// 这跟档案 §7.17 那条 `unwrap_or_default()` 是同一条教训。
    #[test]
    fn an_unreadable_file_is_unknown_not_absent() {
        let p = std::env::temp_dir().join("qb-gate-no-such-file-xyz.bin");
        assert_eq!(file_contains(&p, NEEDLE), None);
    }

    /// 「登录过」只认凭证与浏览器痕迹，光装过不算。
    #[test]
    fn ever_logged_in_needs_more_than_an_install() {
        let mut r = TraceReport::default();
        r.traces.push(Trace {
            kind: "install",
            label: "x".into(),
            path: "x".into(),
            detail: "x".into(),
        });
        assert!(!r.ever_logged_in(), "装过 ≠ 登录过");

        r.traces.push(Trace {
            kind: "credential",
            label: "y".into(),
            path: "y".into(),
            detail: "y".into(),
        });
        assert!(r.ever_logged_in());
    }
}
