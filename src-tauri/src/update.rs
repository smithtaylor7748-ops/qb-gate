//! QB Gate 自己的更新：打开面板时弹窗提醒 + 一键更新（0.25.3，使用者定的）。
//!
//! 「问、下、核」三件事在 [`install::self_update`]（地址、版本比较、`SHA256SUMS.txt`），
//! 这里只管接口层的三件事：
//!
//! 1. **记住上一次问到的结果**，只在内存里。弹窗、设置页读它，不为了显示再联网一次；
//! 2. **什么时候问**：启动时一次（设置里的「启动时检查更新」，默认开）、设置页点「检查更新」
//!    一次。**没有定时器**，面板开一整天也只在启动那一刻问过；
//! 3. **下完之后怎么交给安装包**：先正常退出面板，在退出处理的**最后一步**才启动安装包
//!    （[`launch_pending`]，`lib.rs` 的 `RunEvent::Exit` 里、重锁之后调）。
//!
//! # 为什么是「先退出、后启动安装包」
//!
//! Tauri 的安装包在被动模式（`/P`）下发现程序还在跑，会**直接把它结束掉**（NSIS 模板里的
//! `CheckIfAppIsRunning`，被动模式不弹确认框）。被结束的面板跑不到退出处理 ——
//! 执行锁就停在被杀之前的样子（CLAUDE.md「更新本机那份」那节写过这个代价）。
//! 反过来，退出处理先把门锁好、再把安装包拉起来，安装包看到的是一个已经退干净的面板。
//!
//! # 安装包的三个参数
//!
//! `/P /UPDATE /R`（2026-09-24 从 Tauri CLI 2.11.4 内嵌的 NSIS 模板里逐行核过）：
//!
//! | 参数 | 作用 |
//! |---|---|
//! | `/P` | 被动模式：只有进度条，不问问题，装完自己关 |
//! | `/UPDATE` | 覆盖安装：不先卸旧版、不新建也不删快捷方式、不动开机自启项 |
//! | `/R` | 装完以使用者身份重新打开面板 |
//!
//! # 代价（界面上常驻说明，不放悬停）
//!
//! 面板一退出，它起的 Claude 桌面端、桌面端里的对话、酒馆与桥接会跟着 Job 一起被结束（坑 7.38）。
//! 所以**只提醒、不自动装**：装这件事永远要使用者在弹窗里点「一键更新」。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use ts_rs::TS;

use crate::error::{GateError, Result};
use crate::install::self_update::{self, UpdateInfo};
use crate::sink::ProgressSink;
use crate::{audit, settings, usecase};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 界面看到的更新状态。**只读内存与设置，拿它不会联网**（联网的是 [`check`]）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UpdateStatus {
    pub current_version: String,
    /// 设置里的「启动时检查更新」。
    pub check_on_start: bool,
    /// 上一次问到的最新发布。没问过、或者问了但从来没成功过，就是 `None`。
    pub latest: Option<UpdateInfo>,
    /// `latest` 比正在跑的这份新。
    pub update_available: bool,
    /// `latest` 正是使用者点过「跳过这个版本」的那一版 —— 启动时不弹，设置页照样能更新。
    pub skipped: bool,
    /// 上一次检查的时刻（本地时间，RFC 3339）。
    pub checked_at: Option<String>,
    /// 上一次检查失败的原因。成功一次就清掉。
    pub error: Option<String>,
}

struct Memory {
    latest: Option<UpdateInfo>,
    checked_at: Option<String>,
    error: Option<String>,
}

static MEMORY: Mutex<Memory> = Mutex::new(Memory {
    latest: None,
    checked_at: None,
    error: None,
});

/// 下好、核过、等面板退出时启动的安装包：（路径，核过的 SHA-256）。
static PENDING: Mutex<Option<(PathBuf, String)>> = Mutex::new(None);

fn compose(current: &str, s: &settings::Settings, m: &Memory) -> UpdateStatus {
    let update_available = m
        .latest
        .as_ref()
        .is_some_and(|l| self_update::is_newer(&l.version, current));
    let skipped = update_available
        && m.latest.as_ref().map(|l| l.version.as_str()) == s.update_skipped_version.as_deref();
    UpdateStatus {
        current_version: current.to_string(),
        check_on_start: s.update_check_on_start,
        latest: m.latest.clone(),
        update_available,
        skipped,
        checked_at: m.checked_at.clone(),
        error: m.error.clone(),
    }
}

/// 现在的状态。不联网。
pub fn status() -> UpdateStatus {
    let m = MEMORY.lock().unwrap_or_else(|e| e.into_inner());
    compose(CURRENT_VERSION, &settings::load(), &m)
}

/// 问一次 GitHub。
///
/// `manual = false` 是启动时那一次：设置里关了「启动时检查更新」就**一个请求都不发**，
/// 原样返回内存里的状态。`manual = true` 是设置页的「检查更新」，不看那个开关。
pub async fn check(manual: bool) -> UpdateStatus {
    if !manual && !settings::load().update_check_on_start {
        return status();
    }
    let r = self_update::fetch_latest().await;
    {
        let mut m = MEMORY.lock().unwrap_or_else(|e| e.into_inner());
        m.checked_at = Some(chrono::Local::now().to_rfc3339());
        match r {
            Ok(info) => {
                if self_update::is_newer(&info.version, CURRENT_VERSION) {
                    audit::write(&format!(
                        "检查更新：GitHub 上有新版 {}（当前 {CURRENT_VERSION}）",
                        info.version
                    ));
                }
                m.latest = Some(info);
                m.error = None;
            }
            Err(e) => {
                audit::write(&format!("检查更新失败：{e}"));
                m.error = Some(e.to_string());
            }
        }
    }
    status()
}

/// 记下「跳过这个版本」。`None` = 取消跳过。只认严格的三段数字，别的一律当取消。
pub fn skip(version: Option<String>) -> Result<UpdateStatus> {
    let mut s = settings::load_checked()?;
    s.update_skipped_version = version.filter(|v| self_update::parse_version(v).is_some());
    usecase::settings_ops::save(&s)?;
    Ok(status())
}

/// 下载并核对上一次问到的那个新版。成功之后调用方记 [`set_pending`]、再让面板退出。
pub async fn prepare(rep: &dyn ProgressSink) -> Result<(PathBuf, String)> {
    let latest = MEMORY
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .latest
        .clone();
    let info = match latest {
        Some(i) if self_update::is_newer(&i.version, CURRENT_VERSION) => i,
        _ => {
            return Err(GateError::Other(
                "没有比当前更新的版本（先点一次「检查更新」）".into(),
            ))
        }
    };
    self_update::download(&info, rep).await
}

pub fn set_pending(path: PathBuf, sha256: String) {
    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some((path, sha256));
}

/// 面板退出的最后一步：有核过的安装包就把它拉起来。**只在 `RunEvent::Exit` 里、重锁之后调。**
///
/// 从核对完到现在隔了一次退出，启动之前再算一遍哈希 —— 对不上就不启动，只记一笔。
pub fn launch_pending() {
    let pending = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take();
    let Some((path, sha256)) = pending else {
        return;
    };
    if !self_update::file_matches(&path, &sha256) {
        audit::write(&format!(
            "更新：安装包 {} 在核对之后变了（或者不见了），没有启动",
            path.display()
        ));
        return;
    }
    match spawn_installer(&path) {
        Ok(pid) => audit::write(&format!(
            "更新：面板已退出，启动安装包 {}（PID {pid}，/P /UPDATE /R）",
            path.display()
        )),
        Err(e) => audit::write(&format!(
            "更新：启动安装包失败：{e}。核对过的安装包留在 {}，可以自己双击安装",
            path.display()
        )),
    }
}

/// 上一次更新留下的安装包。启动时顺手清掉；删不掉（安装包还在收尾）就等下次。
pub fn clean_leftovers() {
    let dir = self_update::download_dir();
    if dir.exists() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

const INSTALLER_ARGS: [&str; 3] = ["/P", "/UPDATE", "/R"];

#[cfg(windows)]
fn spawn_installer(path: &Path) -> std::io::Result<u32> {
    use std::os::windows::process::CommandExt;
    // 面板自己可能是从别人的 Job 里起的（比如 Claude 桌面端的终端）。先试着脱离那个 Job；
    // Job 不许脱离时 CreateProcess 回拒绝访问，退回普通启动 —— 那种 Job 的句柄在起面板的
    // 那个程序手里，不会因为面板退出而关闭。
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let child = match std::process::Command::new(path)
        .args(INSTALLER_ARGS)
        .creation_flags(CREATE_BREAKAWAY_FROM_JOB)
        .spawn()
    {
        Ok(c) => c,
        Err(_) => std::process::Command::new(path)
            .args(INSTALLER_ARGS)
            .spawn()?,
    };
    Ok(child.id())
}

#[cfg(not(windows))]
fn spawn_installer(_path: &Path) -> std::io::Result<u32> {
    Err(std::io::Error::other("只在 Windows 上可用"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(version: &str) -> UpdateInfo {
        UpdateInfo {
            version: version.into(),
            tag: format!("v{version}"),
            published_at: None,
            notes: String::new(),
            page_url: String::new(),
        }
    }

    fn memory(latest: Option<UpdateInfo>) -> Memory {
        Memory {
            latest,
            checked_at: None,
            error: None,
        }
    }

    #[test]
    fn never_checked_never_claims_an_update() {
        let s = compose("0.25.3", &settings::Settings::default(), &memory(None));
        assert!(!s.update_available);
        assert!(!s.skipped);
        assert!(s.latest.is_none());
        assert_eq!(s.current_version, "0.25.3");
        assert!(s.check_on_start, "默认开");
    }

    #[test]
    fn a_newer_release_is_an_update_and_the_same_or_older_is_not() {
        let d = settings::Settings::default();
        assert!(compose("0.25.3", &d, &memory(Some(info("0.25.4")))).update_available);
        assert!(!compose("0.25.3", &d, &memory(Some(info("0.25.3")))).update_available);
        assert!(!compose("0.32.0", &d, &memory(Some(info("0.25.4")))).update_available);
    }

    /// 跳过的只是**那一版**：更新的版本照样提醒。
    #[test]
    fn skipping_hides_only_that_exact_version() {
        let s = settings::Settings {
            update_skipped_version: Some("0.25.4".into()),
            ..Default::default()
        };
        let st = compose("0.25.3", &s, &memory(Some(info("0.25.4"))));
        assert!(st.update_available && st.skipped);
        let st = compose("0.25.3", &s, &memory(Some(info("0.25.5"))));
        assert!(st.update_available && !st.skipped);
    }

    /// 顺序是这条功能的安全前提：安装包必须在退出处理的**最后**才起，
    /// 而且 `/P` 被动模式、`/UPDATE` 覆盖安装、`/R` 装完重开，一个都不能少。
    #[test]
    fn the_installer_is_asked_for_a_passive_in_place_update_and_a_relaunch() {
        assert_eq!(INSTALLER_ARGS, ["/P", "/UPDATE", "/R"]);
    }

    /// 退出时启动安装包那一步必须排在重锁**之后** —— 读源码钉住，删了这条就等于允许
    /// 「安装包先起、面板被它强杀、执行锁停在半路」。
    #[test]
    fn the_exit_handler_relocks_before_it_launches_the_installer() {
        let src = include_str!("lib.rs");
        let body = src
            .split_once("tauri::RunEvent::Exit")
            .expect("lib.rs 里没有 RunEvent::Exit 的处理")
            .1;
        let lock = body.find("gate::lock_all()").expect("退出处理里没有重锁");
        let launch = body
            .find("update::launch_pending()")
            .expect("退出处理里没有启动安装包");
        assert!(lock < launch, "必须先重锁、再启动安装包");
    }
}
