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
use crate::sink::ProgressSink;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PLUGIN_ID: &str = "sillytavern";
pub const PLUGIN_NAME: &str = "酒馆 SillyTavern";

const BRIDGE_READY_ATTEMPTS: u32 = 40; // 40 × 500ms = 20 秒
const ST_READY_ATTEMPTS: u32 = 120; // 120 × 500ms = 60 秒
const POLL_MS: u64 = 500;

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct TavernConfig {
    pub bridge_root: PathBuf,
    pub sillytavern_root: PathBuf,
    pub st_launcher: PathBuf,
    pub bridge_port: u16,
    pub st_port: u16,
}

impl Default for TavernConfig {
    fn default() -> Self {
        // 三个路径**故意留空**。
        //
        // 这里原来写死的是作者本机那份部署（`D:\tools\…`）。在作者机器上没出过事，
        // 在别人机器上则是两个问题：一是面板一上来就报一串跟他毫无关系的盘符路径，
        // 二是把作者的目录布局随源码发了出去。
        //
        // 留空之后，`status()` 里的存在性检查一律不过，插件如实停在
        // `PluginState::Missing`（「依赖不齐，展开看缺哪一项」）——
        // 这正是「还没配」的准确描述。用户在设置里填完自己的路径就好。
        // 端口保留默认值：5001 / 8000 是桥接与酒馆各自的约定端口，与机器无关。
        Self {
            bridge_root: PathBuf::new(),
            sillytavern_root: PathBuf::new(),
            st_launcher: PathBuf::new(),
            bridge_port: 5001,
            st_port: 8000,
        }
    }
}

fn config_path() -> PathBuf {
    crate::paths::state_dir()
        .join("plugins")
        .join("sillytavern.json")
}

pub fn load_config() -> TavernConfig {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn load_config_checked() -> Result<TavernConfig> {
    match crate::config_io::read_optional(&config_path())? {
        Some(b) => Ok(serde_json::from_slice(&b)?),
        None => Ok(TavernConfig::default()),
    }
}
pub fn save_config(cfg: &TavernConfig) -> Result<()> {
    let expected = crate::config_io::read_optional(&config_path())?;
    if let Some(b) = &expected {
        let _: TavernConfig = serde_json::from_slice(b)?;
    }
    if cfg.bridge_port == 0 || cfg.st_port == 0 || cfg.bridge_port == cfg.st_port {
        return Err(GateError::Other("应用和桥接需要两个不同的有效端口".into()));
    }
    crate::config_io::commit(vec![crate::config_io::Edit {
        path: config_path(),
        expected,
        body: Some(serde_json::to_vec_pretty(cfg)?),
    }])?;
    Ok(())
}

/// 见 [`crate::paths::tavern_bridge_dir`]。这里只转调，
/// 路径的唯一定义在 `paths` 里。
pub fn bridge_data_dir() -> PathBuf {
    crate::paths::tavern_bridge_dir()
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

/// 定位官方 claude.exe（给桥接的 `--claude` 用，所以必须是 exe）。
///
/// v0.8.0 起跟检测、启动、上锁用同一张表（`install::inventory`）。
/// 原来这里自己拼一份候选，**而且把 `claude-path.txt` 里记住的路径排第一** ——
/// 记住的是哪份就一直用哪份，装了新的也不换。现在记住的路径只当最后的兜底：
/// 表里一份都找不到时，才看使用者是不是手动指过一个表外的位置。
///
/// 排除 `claude-code-cli\` 下的副本 —— 那是项目自带的 vendored 版本，
/// 不是官方二进制，桥接明确拒绝它（inventory 里也排除了）。
pub fn find_official_claude() -> Result<PathBuf> {
    let saved = bridge_data_dir().join("claude-path.txt");
    let from_inventory =
        crate::install::inventory::preferred_exe(&crate::install::inventory::Roots::current());

    let chosen = from_inventory.or_else(|| {
        let t = std::fs::read_to_string(&saved).ok()?;
        let c = PathBuf::from(t.trim());
        let ok = !c.as_os_str().is_empty()
            && c.is_file()
            && c.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            && !c
                .to_string_lossy()
                .to_lowercase()
                .contains(r"\claude-code-cli\")
            && !crate::install::inventory::is_desktop_runtime_copy(&c);
        ok.then_some(c)
    });

    match chosen {
        Some(c) => Ok(c),
        None => Err(GateError::Other(
            "找不到官方 Claude Code 程序，请先在「环境与安装」里装好。".into(),
        )),
    }
}

fn find_python() -> Result<PathBuf> {
    for exe in ["python.exe", "python3.exe", "py.exe"] {
        if let Ok(out) = crate::process::hidden_std(std::process::Command::new("where"))
            .arg(exe)
            .output()
        {
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
        (PluginState::Missing, "依赖不齐，展开看缺哪一项".to_string())
    } else if bridge_up
        && st_up
        && owned_running("bridge").unwrap_or(false)
        && owned_running("sillytavern").unwrap_or(false)
    {
        (
            PluginState::Running,
            "登记的桥接与酒馆进程正在运行".to_string(),
        )
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
        (
            PluginState::Ready,
            format!("就绪，数据目录 {}", dd.display()),
        )
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

    let out = crate::process::hidden_tokio(tokio::process::Command::new("powershell"))
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "(Get-CimInstance Win32_Process -Filter 'ProcessId={pid}' \
                 -ErrorAction Stop).CommandLine"
            ),
        ])
        .output()
        .await?;
    if !out.status.success() {
        return Err(GateError::Other(
            "无法枚举旧桥接进程，未覆盖 PID 记录".into(),
        ));
    }
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

/// Creating a private application directory must not reset an existing user's ACLs.
fn fix_data_dir_acl(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let probe = dir.join(format!(".qb-write-check-{}", crate::config_io::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|e| GateError::Other(format!("桥接数据目录无法写入：{e}；请检查此目录权限")))?;
    std::fs::remove_file(probe)?;
    Ok(())
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
static PROGRAMS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::BTreeMap<String, crate::sessions::OwnedProgram>>,
> = std::sync::OnceLock::new();
fn programs(
) -> &'static std::sync::Mutex<std::collections::BTreeMap<String, crate::sessions::OwnedProgram>> {
    PROGRAMS.get_or_init(Default::default)
}
fn program_path(name: &str) -> PathBuf {
    crate::paths::state_dir()
        .join("plugins")
        .join(format!("{name}-process.json"))
}
fn owned_running(name: &str) -> Result<bool> {
    let mut map = programs().lock().unwrap();
    if !map.contains_key(name) {
        if let Some(bytes) = crate::config_io::read_optional(&program_path(name))? {
            let record = serde_json::from_slice(&bytes)?;
            if let Some(program) = crate::sessions::OwnedProgram::reopen(record)? {
                map.insert(name.into(), program);
            }
        }
    }
    map.get(name).map(|p| p.running()).unwrap_or(Ok(false))
}
fn start_owned(
    name: &str,
    exe: &Path,
    args: Vec<String>,
    cwd: &Path,
    env: Vec<(String, String)>,
) -> Result<()> {
    let program = crate::sessions::OwnedProgram::launch(exe, &args, cwd, env)?;
    if let Err(error) = crate::config_io::replace(
        &program_path(name),
        Some(&serde_json::to_vec(&program.record)?),
    ) {
        let _ = program.stop();
        return Err(error);
    }
    if name == "bridge" {
        if let Err(error) = crate::config_io::replace(
            &bridge_data_dir().join("bridge.pid"),
            Some(program.record.pid.to_string().as_bytes()),
        ) {
            let _ = program.stop();
            let _ = crate::config_io::replace(&program_path(name), None);
            return Err(error);
        }
    }
    programs().lock().unwrap().insert(name.into(), program);
    Ok(())
}
fn stop_owned(name: &str) -> Result<()> {
    let _ = owned_running(name)?;
    let mut map = programs().lock().unwrap();
    if let Some(p) = map.get(name) {
        p.stop()?;
    } else {
        crate::config_io::replace(&program_path(name), None)?;
        return Ok(());
    }
    map.remove(name);
    crate::config_io::replace(&program_path(name), None)?;
    if name == "bridge" {
        crate::config_io::replace(&bridge_data_dir().join("bridge.pid"), None)?;
    }
    Ok(())
}
struct StartAttempt {
    created: Vec<String>,
    finished: bool,
}
impl Drop for StartAttempt {
    fn drop(&mut self) {
        if !self.finished {
            for name in self.created.iter().rev() {
                if let Err(e) = stop_owned(name) {
                    crate::audit::write(&format!("启动失败后的进程清理未完成：{e}"));
                }
            }
        }
    }
}
async fn tavern_healthy(port: u16) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return false;
    };
    let Ok(response) = client.get(format!("http://127.0.0.1:{port}/")).send().await else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/html"))
}
pub async fn start(rep: &dyn ProgressSink) -> Result<String> {
    let cfg = load_config_checked()?;
    let dd = bridge_data_dir();
    rep.phase(1, "检查依赖与官方身份");
    let claude = find_official_claude()?;
    if bridge_data_dir().is_dir() {
        crate::config_io::commit(vec![crate::config_io::Edit::text(
            bridge_data_dir().join("claude-path.txt"),
            claude.display().to_string(),
        )?])?;
    }
    let python = find_python()?;
    let bridge_py = cfg.bridge_root.join("bridge.py");
    if !bridge_py.is_file() {
        return Err(GateError::NotFound(bridge_py.display().to_string()));
    }
    let official = crate::accounts::active_slot_dir(&crate::accounts::AccountRoots::current())
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".claude"));
    crate::residue::check_official(crate::domain::Client::ClaudeCode, &official)?;
    fix_data_dir_acl(&dd)?;
    let mut attempt = StartAttempt {
        created: vec![],
        finished: false,
    };
    rep.phase(2, "启动 Claude 桥接");
    if !owned_running("bridge")? {
        if existing_bridge_pid(&cfg).await?.is_some() {
            if !bridge_healthy(cfg.bridge_port).await {
                return Err(GateError::Other(
                    "已有桥接进程未通过健康检查，请在原启动器中处理".into(),
                ));
            }
        } else {
            if port_in_use(cfg.bridge_port) {
                return Err(GateError::Other(
                    "桥接端口被其他程序占用，未停止任何进程".into(),
                ));
            }
            start_owned(
                "bridge",
                &python,
                vec![
                    "-u".into(),
                    bridge_py.display().to_string(),
                    "--claude".into(),
                    claude.display().to_string(),
                    "--data-dir".into(),
                    dd.display().to_string(),
                    "--port".into(),
                    cfg.bridge_port.to_string(),
                ],
                &cfg.bridge_root,
                vec![
                    ("CLAUDE_CONFIG_DIR".into(), official.display().to_string()),
                    ("DISABLE_AUTOUPDATER".into(), "1".into()),
                ],
            )?;
            attempt.created.push("bridge".into());
        }
    }
    rep.phase(3, "等桥接健康检查通过（最长 20 秒）");
    let mut ready = false;
    for _ in 0..BRIDGE_READY_ATTEMPTS {
        if bridge_healthy(cfg.bridge_port).await {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(POLL_MS)).await;
    }
    if !ready {
        return Err(GateError::Other(
            "桥接未通过健康检查；本次启动的进程将清理".into(),
        ));
    }
    rep.phase(4, "启动酒馆");
    if !owned_running("sillytavern")? {
        if port_in_use(cfg.st_port) {
            return Err(GateError::Other(
                "酒馆端口被未登记的进程占用，请在原酒馆界面处理后重试；没有停止或冒充该服务".into(),
            ));
        }
        if !cfg.st_launcher.is_file() {
            return Err(GateError::NotFound(cfg.st_launcher.display().to_string()));
        }
        start_owned(
            "sillytavern",
            &cfg.st_launcher,
            vec![],
            cfg.st_launcher.parent().unwrap_or(Path::new(".")),
            vec![],
        )?;
        attempt.created.push("sillytavern".into());
    }
    rep.phase(5, "等酒馆 HTTP 服务就绪（最长 60 秒）");
    for _ in 0..ST_READY_ATTEMPTS {
        if !owned_running("sillytavern")? {
            return Err(GateError::Other("酒馆进程在就绪前退出".into()));
        }
        if tavern_healthy(cfg.st_port).await {
            attempt.finished = true;
            rep.done("酒馆与桥接已就绪");
            return Ok(format!("http://127.0.0.1:{}", cfg.st_port));
        }
        tokio::time::sleep(std::time::Duration::from_millis(POLL_MS)).await;
    }
    Err(GateError::Other(
        "酒馆未通过 HTTP 就绪检查；本次新启动的进程将清理".into(),
    ))
}
pub async fn stop() -> Result<Vec<String>> {
    let mut done = Vec::new();
    let mut failures = Vec::new();
    for name in ["sillytavern", "bridge"] {
        match stop_owned(name) {
            Ok(()) => done.push(format!("{name} 的登记进程已停止")),
            Err(e) => failures.push(e.to_string()),
        }
    }
    if !failures.is_empty() {
        return Err(GateError::Other(failures.join("；")));
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认配置里**不许出现任何一台具体机器的路径**。
    ///
    /// 开源之前这里写死过作者本机的 `D:\tools\…`，别人装上就看到一串
    /// 跟自己无关的盘符。端口是协议约定，与机器无关，照旧带默认值。
    #[test]
    fn default_config_carries_no_machine_specific_paths() {
        let c = TavernConfig::default();
        assert_eq!(c.bridge_port, 5001);
        assert_eq!(c.st_port, 8000);
        for p in [&c.bridge_root, &c.sillytavern_root, &c.st_launcher] {
            assert_eq!(
                p.as_os_str().len(),
                0,
                "默认路径必须为空，实得 {}",
                p.display()
            );
        }
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
        let vendored = r"d:\tools\sillytavern\claude-code-cli\claude.exe";
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
