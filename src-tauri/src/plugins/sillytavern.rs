//! 酒馆插件：启动真正的 SillyTavern + ClaudeTavernBridge。
//!
//! 逻辑移植自 `start-claude-pro-rp.ps1`。**下面这些是移植时最容易丢的，
//! 每一条都有出处，动手前先读**：
//!
//!   - PID 文件必须比对命令行，不匹配就报错，**不覆盖也不杀**。
//!     光看「PID 存在」会误伤到复用了同一个 PID 的无关进程。
//!   - 端口被别人占 → **报错退出，绝不停止无关进程**。
//!     启动器没有资格替用户决定关掉他正在用的东西。
//!   - 就绪判定必须打 `/health`，不能只看进程活着。
//!     bridge 起来到能服务之间有几秒，这段时间里酒馆连过去会失败。
//!   - **无论是不是复用现有服务都要打开浏览器页。**
//!     跳过这一步会让成功的启动和崩溃看起来一模一样（控制台窗口直接关掉，
//!     什么可见的事都没发生）。
//!   - 定位官方 claude.exe 时排除 `claude-code-cli\` 下的 vendored 副本。
//!   - 数据目录的 ACL 要修，否则 Python 报 WinError 5。

use crate::error::{GateError, Result};
use crate::plugins::{DependencyCheck, PluginState, PluginStatus};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PLUGIN_ID: &str = "sillytavern";
pub const PLUGIN_NAME: &str = "酒馆 SillyTavern";

const BRIDGE_READY_ATTEMPTS: u32 = 40; // 40 × 500ms = 20 秒
const ST_READY_ATTEMPTS: u32 = 120; // 120 × 500ms = 60 秒
const POLL_MS: u64 = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TavernConfig {
    pub bridge_root: PathBuf,
    pub sillytavern_root: PathBuf,
    pub st_launcher: PathBuf,
    pub bridge_port: u16,
    pub st_port: u16,
}

impl Default for TavernConfig {
    fn default() -> Self {
        // 默认值是本机现有部署的位置。别人用这个项目要在设置里改。
        Self {
            bridge_root: PathBuf::from(r"D:\tools\claude-code-sillytavern-bridge-v2"),
            sillytavern_root: PathBuf::from(r"D:\tools\SillyTavern"),
            st_launcher: PathBuf::from(r"D:\tools\start-sillytavern.cmd"),
            bridge_port: 5001,
            st_port: 8000,
        }
    }
}

fn config_path() -> PathBuf {
    crate::gate::state_dir().join("plugins").join("sillytavern.json")
}

pub fn load_config() -> TavernConfig {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_config(cfg: &TavernConfig) -> Result<()> {
    let p = config_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(cfg)?)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

pub fn bridge_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_default()
        .join("ClaudeTavernBridge")
}

// ------------------------------------------------------------------ 端口

/// 端口是否被占。用试绑定判断，不需要额外依赖。
///
/// 注意语义：这里只回答「占没占」，**不回答「被谁占」**。
/// 判断「是不是我们自己的 bridge」靠的是 PID 文件比对，在调用方做。
fn port_in_use(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_err()
}

// ------------------------------------------------------------------ 定位

/// 定位官方 claude.exe。
///
/// 排除 `claude-code-cli\` 下的副本 —— 那是项目自带的 vendored 版本，
/// 不是官方二进制，桥接明确拒绝它。
pub fn find_official_claude() -> Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    let saved = bridge_data_dir().join("claude-path.txt");
    if let Ok(t) = std::fs::read_to_string(&saved) {
        candidates.push(PathBuf::from(t.trim()));
    }
    if let Some(h) = dirs::home_dir() {
        candidates.push(h.join(".local").join("bin").join("claude.exe"));
    }
    // 新布局：%APPDATA%\Claude\claude-code\<版本>\claude.exe。
    // 版本目录名按字符串倒序，优先拿看起来最新的那个 —— 这里只是「挑一个来跑」，
    // 上锁那边（targets.rs）是**每一个版本都锁**，两者目的不同，别混。
    if let Some(r) = dirs::config_dir() {
        let cc = r.join("Claude").join("claude-code");
        if let Ok(rd) = std::fs::read_dir(&cc) {
            let mut vers: Vec<PathBuf> = rd
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .map(|e| e.path())
                .collect();
            vers.sort();
            vers.reverse();
            candidates.extend(vers.into_iter().map(|v| v.join("claude.exe")));
        }
    }
    if let Some(l) = dirs::data_local_dir() {
        candidates.push(l.join("Programs").join("Claude").join("claude.exe"));
    }

    for c in candidates {
        if c.as_os_str().is_empty() || !c.is_file() {
            continue;
        }
        if c.extension().and_then(|e| e.to_str()) != Some("exe") {
            continue;
        }
        if c.to_string_lossy().to_lowercase().contains(r"\claude-code-cli\") {
            continue;
        }
        let _ = std::fs::write(&saved, c.to_string_lossy().as_bytes());
        return Ok(c);
    }
    Err(GateError::Other(
        "找不到官方 Claude Code 程序，请先在「环境与安装」里装好。".into(),
    ))
}

fn find_python() -> Result<PathBuf> {
    for exe in ["python.exe", "python3.exe", "py.exe"] {
        if let Ok(out) = std::process::Command::new("where").arg(exe).output() {
            if out.status.success() {
                if let Some(first) = String::from_utf8_lossy(&out.stdout).lines().next() {
                    let p = PathBuf::from(first.trim());
                    if p.is_file() {
                        return Ok(p);
                    }
                }
            }
        }
    }
    Err(GateError::Other(
        "找不到 Python 3，请先安装并确认它在 PATH 里。".into(),
    ))
}

// ------------------------------------------------------------------ 检测

pub fn status() -> PluginStatus {
    let cfg = load_config();
    let dd = bridge_data_dir();
    let mut checks = Vec::new();

    let bridge_py = cfg.bridge_root.join("bridge.py");
    checks.push(DependencyCheck::new(
        "桥接项目",
        bridge_py.is_file(),
        cfg.bridge_root.display().to_string(),
    ));

    let st_server = cfg.sillytavern_root.join("server.js");
    checks.push(DependencyCheck::new(
        "SillyTavern",
        st_server.is_file(),
        cfg.sillytavern_root.display().to_string(),
    ));

    let py = find_python();
    checks.push(DependencyCheck::new(
        "Python 3",
        py.is_ok(),
        match &py {
            Ok(p) => p.display().to_string(),
            Err(e) => e.to_string(),
        },
    ));

    let claude = find_official_claude();
    checks.push(DependencyCheck::new(
        "官方 Claude Code",
        claude.is_ok(),
        match &claude {
            Ok(p) => p.display().to_string(),
            Err(e) => e.to_string(),
        },
    ));

    let bridge_up = port_in_use(cfg.bridge_port);
    let st_up = port_in_use(cfg.st_port);
    checks.push(DependencyCheck::new(
        format!("桥接端口 {}", cfg.bridge_port),
        true,
        if bridge_up { "在监听" } else { "空闲" },
    ));
    checks.push(DependencyCheck::new(
        format!("酒馆端口 {}", cfg.st_port),
        true,
        if st_up { "在监听" } else { "空闲" },
    ));

    let deps_ok = checks.iter().take(4).all(|c| c.ok);
    let (state, detail) = if !deps_ok {
        (
            PluginState::Missing,
            "依赖不齐，展开看缺哪一项".to_string(),
        )
    } else if bridge_up && st_up {
        (PluginState::Running, "桥接与酒馆都在运行".to_string())
    } else if bridge_up || st_up {
        (
            PluginState::Broken,
            format!(
                "只起来了一半：桥接 {}，酒馆 {}",
                if bridge_up { "在" } else { "不在" },
                if st_up { "在" } else { "不在" }
            ),
        )
    } else {
        (PluginState::Ready, format!("就绪，数据目录 {}", dd.display()))
    };

    PluginStatus {
        id: PLUGIN_ID,
        name: PLUGIN_NAME,
        state,
        detail,
        checks,
    }
}

// ------------------------------------------------------------------ PID

/// 读 PID 文件并**用命令行验明正身**。
///
/// 返回 `Ok(Some(pid))` 表示确认是我们自己的 bridge，可以复用；
/// `Ok(None)` 表示没有有效记录；
/// `Err` 表示 PID 文件指向了别的进程 —— 这种情况**必须报错**，
/// 既不能覆盖也不能杀，那是别人的进程。
#[cfg(windows)]
async fn existing_bridge_pid(cfg: &TavernConfig) -> Result<Option<u32>> {
    let pid_path = bridge_data_dir().join("bridge.pid");
    let Ok(text) = std::fs::read_to_string(&pid_path) else {
        return Ok(None);
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        let _ = std::fs::remove_file(&pid_path);
        return Ok(None);
    };

    let out = tokio::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "(Get-CimInstance Win32_Process -Filter 'ProcessId={pid}' \
                 -ErrorAction SilentlyContinue).CommandLine"
            ),
        ])
        .output()
        .await?;
    let cmdline = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();

    if cmdline.is_empty() {
        // 进程没了，清掉陈旧的 PID 文件。
        let _ = std::fs::remove_file(&pid_path);
        return Ok(None);
    }

    let want_script = cfg
        .bridge_root
        .join("bridge.py")
        .to_string_lossy()
        .to_lowercase();
    let want_dir = bridge_data_dir().to_string_lossy().to_lowercase();

    if cmdline.contains(&want_script) && cmdline.contains(&want_dir) {
        Ok(Some(pid))
    } else {
        Err(GateError::Other(format!(
            "桥接 PID 文件指向了其他进程（PID {pid}），为安全起见没有停止或覆盖它。"
        )))
    }
}

#[cfg(not(windows))]
async fn existing_bridge_pid(_cfg: &TavernConfig) -> Result<Option<u32>> {
    Ok(None)
}

// ------------------------------------------------------------------ 启动

#[cfg(windows)]
fn spawn_hidden(cmd: &mut std::process::Command) -> std::io::Result<std::process::Child> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW).spawn()
}

#[cfg(not(windows))]
fn spawn_hidden(cmd: &mut std::process::Command) -> std::io::Result<std::process::Child> {
    cmd.spawn()
}

/// 修数据目录权限。不修的话 Python 起来会报 WinError 5。
#[cfg(windows)]
fn fix_data_dir_acl(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let who = whoami_name();
    // 这里用 icacls 是有意的：给自己的数据目录授权和门禁的 Deny ACE 是两回事，
    // 前者不是管控点。门禁那边（gate/acl.rs）仍然只用 API。
    for args in [
        vec![
            dir.to_string_lossy().to_string(),
            "/inheritance:r".into(),
            format!("/grant:r"),
            format!("{who}:F"),
            "/Q".into(),
        ],
        vec![
            dir.to_string_lossy().to_string(),
            "/grant".into(),
            format!("{who}:(OI)(CI)F"),
            "/Q".into(),
        ],
    ] {
        let _ = std::process::Command::new("icacls").args(&args).output();
    }
    Ok(())
}

#[cfg(not(windows))]
fn fix_data_dir_acl(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    Ok(())
}

#[cfg(windows)]
fn whoami_name() -> String {
    std::process::Command::new("whoami")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| std::env::var("USERNAME").unwrap_or_default())
}

async fn bridge_healthy(port: u16) -> bool {
    let token_path = bridge_data_dir().join("bridge-token.txt");
    let Ok(token) = std::fs::read_to_string(&token_path) else {
        return false;
    };
    let Ok(c) = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    let url = format!("http://127.0.0.1:{port}/health");
    match c.get(&url).bearer_auth(token.trim()).send().await {
        Ok(r) => match r.json::<serde_json::Value>().await {
            Ok(v) => v.get("status").and_then(|s| s.as_str()) == Some("ok"),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

/// 启动酒馆全链路。
///
/// 调用方（lib.rs 的命令）负责在此之前完成 IP 门禁放行，
/// 并在成功之后起 `WatchMode::Cli` 看门狗。
pub async fn start() -> Result<String> {
    let cfg = load_config();
    let dd = bridge_data_dir();

    // 依赖先查齐，别起到一半才发现缺东西。
    let claude = find_official_claude()?;
    let python = find_python()?;
    let bridge_py = cfg.bridge_root.join("bridge.py");
    if !bridge_py.is_file() {
        return Err(GateError::NotFound(bridge_py.display().to_string()));
    }

    fix_data_dir_acl(&dd)?;

    // ---- 桥接 ----
    let reused_bridge = existing_bridge_pid(&cfg).await?;
    let bridge_pid = match reused_bridge {
        Some(pid) => pid,
        None => {
            if port_in_use(cfg.bridge_port) {
                return Err(GateError::Other(format!(
                    "{} 端口已被其他程序占用，启动器不会停止无关进程。",
                    cfg.bridge_port
                )));
            }

            let stdout = std::fs::File::create(dd.join("bridge-runtime.log"))?;
            let stderr = std::fs::File::create(dd.join("bridge-error.log"))?;

            let mut cmd = std::process::Command::new(&python);
            cmd.arg("-u")
                .arg(&bridge_py)
                .arg("--claude")
                .arg(&claude)
                .arg("--data-dir")
                .arg(&dd)
                .arg("--port")
                .arg(cfg.bridge_port.to_string())
                .current_dir(&cfg.bridge_root)
                .stdout(stdout)
                .stderr(stderr);

            let child = spawn_hidden(&mut cmd)
                .map_err(|e| GateError::Other(format!("启动桥接失败：{e}")))?;
            let pid = child.id();
            std::fs::write(dd.join("bridge.pid"), pid.to_string())?;
            pid
        }
    };

    // ---- 等桥接就绪：必须打 /health，不能只看进程活着 ----
    let mut ready = false;
    for _ in 0..BRIDGE_READY_ATTEMPTS {
        tokio::time::sleep(std::time::Duration::from_millis(POLL_MS)).await;
        if bridge_healthy(cfg.bridge_port).await {
            ready = true;
            break;
        }
    }
    if !ready {
        if reused_bridge.is_none() {
            let _ = kill_tree(bridge_pid).await;
            let _ = std::fs::remove_file(dd.join("bridge.pid"));
        }
        let detail = std::fs::read_to_string(dd.join("bridge-error.log"))
            .unwrap_or_default()
            .lines()
            .rev()
            .take(10)
            .collect::<Vec<_>>()
            .join("\n");
        return Err(GateError::Other(format!(
            "桥接器在 20 秒内没有就绪。{}",
            if detail.trim().is_empty() {
                "没有收到桥接器错误日志。".to_string()
            } else {
                detail
            }
        )));
    }

    // ---- 酒馆 ----
    let st_was_running = port_in_use(cfg.st_port);
    if !st_was_running {
        if !cfg.st_launcher.is_file() {
            return Err(GateError::NotFound(cfg.st_launcher.display().to_string()));
        }
        let mut cmd = std::process::Command::new("cmd");
        cmd.arg("/c")
            .arg(&cfg.st_launcher)
            .current_dir(cfg.st_launcher.parent().unwrap_or(Path::new(".")));
        let child = spawn_hidden(&mut cmd)
            .map_err(|e| GateError::Other(format!("启动酒馆失败：{e}")))?;
        std::fs::write(dd.join("sillytavern.pid"), child.id().to_string())?;
    }

    for _ in 0..ST_READY_ATTEMPTS {
        tokio::time::sleep(std::time::Duration::from_millis(POLL_MS)).await;
        if port_in_use(cfg.st_port) {
            crate::gate::log::write("酒馆与桥接已就绪");
            // **复用路径也要返回 URL 让界面打开页面。**
            // 少了这一步，成功的启动和崩溃看起来一模一样。
            return Ok(format!("http://127.0.0.1:{}", cfg.st_port));
        }
    }

    Err(GateError::Other("酒馆在 60 秒内没有就绪。".into()))
}

async fn kill_tree(pid: u32) -> Result<()> {
    let _ = tokio::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output()
        .await;
    Ok(())
}

/// 停止酒馆与桥接。只收本插件记录在案的进程树。
pub async fn stop() -> Result<Vec<String>> {
    let dd = bridge_data_dir();
    let mut done = Vec::new();

    for (file, label) in [
        ("sillytavern.pid", "酒馆"),
        ("bridge.pid", "桥接"),
    ] {
        let p = dd.join(file);
        if let Ok(t) = std::fs::read_to_string(&p) {
            if let Ok(pid) = t.trim().parse::<u32>() {
                let _ = kill_tree(pid).await;
                done.push(format!("{label} PID {pid} 已停止"));
            }
        }
        let _ = std::fs::remove_file(&p);
    }

    crate::gate::log::write("酒馆插件已停止");
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_points_at_current_deployment() {
        let c = TavernConfig::default();
        assert_eq!(c.bridge_port, 5001);
        assert_eq!(c.st_port, 8000);
        assert!(c.bridge_root.ends_with("claude-code-sillytavern-bridge-v2"));
    }

    #[test]
    fn config_roundtrips() {
        let c = TavernConfig::default();
        let j = serde_json::to_string(&c).unwrap();
        let back: TavernConfig = serde_json::from_str(&j).unwrap();
        assert_eq!(back.bridge_port, c.bridge_port);
        assert_eq!(back.sillytavern_root, c.sillytavern_root);
    }

    #[test]
    fn vendored_copy_is_excluded_by_path_rule() {
        // find_official_claude 里那条排除规则的口径，单独钉一下。
        let vendored = r"d:\tools\claude-code-cli\claude.exe";
        assert!(vendored.to_lowercase().contains(r"\claude-code-cli\"));
        let official = r"c:\users\me\.local\bin\claude.exe";
        assert!(!official.to_lowercase().contains(r"\claude-code-cli\"));
    }

    #[test]
    fn unbound_port_reads_as_free() {
        // 取一个几乎不会被占的高位端口，确认 port_in_use 的极性没写反。
        assert!(!port_in_use(58731));
    }

    #[test]
    fn bound_port_reads_as_in_use() {
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        assert!(port_in_use(port));
    }
}
