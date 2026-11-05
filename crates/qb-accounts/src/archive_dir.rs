//! 把一个槽位目录挪走（归档 / 删除前的暂存）。三家共用。
//!
//! 先原子改名；目录里有文件被占着（os error 32 / 5）就退避重试约 3 秒；还不行就问
//! Windows Restart Manager 是谁占着，报一句人话。改名失败 = 一个文件都没动。
//! ⛔ 绝不「复制后删源」：那样会在本机多出一份登录令牌。
use crate::error::{GateError, Result};
use std::path::Path;
use std::time::{Duration, Instant};

/// 「已归档的账户」折叠区里的一条（GPT / 反重力共用）。`key` 交回给「恢复」。
#[derive(Clone, Debug, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ArchivedSlot {
    pub key: String,
    pub label: String,
    pub archived_at: Option<String>,
    /// 已从列表移除、目录还没挪进归档（当时被占着，稍后再试）。
    pub pending: bool,
}

/// 「被占着」那类错误的开头。前端靠它决定要不要给「先从列表移除，目录稍后归档」。
pub const BUSY_PREFIX: &str = "没动任何文件；";

pub fn move_dir(src: &Path, dest: &Path) -> Result<()> {
    move_dir_within(src, dest, Duration::from_secs(3))
}

/// 同 [`move_dir`]，重试预算可调（后台补归档用 0：试一次就走）。
pub fn move_dir_within(src: &Path, dest: &Path, budget: Duration) -> Result<()> {
    let start = Instant::now();
    let mut wait = Duration::from_millis(100);
    loop {
        match std::fs::rename(src, dest) {
            Ok(()) => return Ok(()),
            Err(e) if is_sharing(&e) => {
                if start.elapsed() + wait > budget {
                    return Err(busy_error(src));
                }
                std::thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_millis(800));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

fn is_sharing(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(32) | Some(5))
}

pub fn is_busy(e: &GateError) -> bool {
    matches!(e, GateError::Other(m) if m.starts_with(BUSY_PREFIX))
}

pub fn busy_error(dir: &Path) -> GateError {
    let who = holders(dir);
    let reason = if who.is_empty() {
        "目录被别的程序占着（没查出是谁：可能是停在这个目录里的终端、资源管理器窗口、杀毒软件或索引服务）".to_string()
    } else {
        format!("被 {} 占着", who.join("、"))
    };
    GateError::Other(format!("{BUSY_PREFIX}{reason}，关掉后再点。"))
}

/// 目录里的文件被哪些进程开着（「名字（pid N）」）。只读查询，查不出就空。
pub fn holders(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect(dir, &mut files, 0);
    rm::query(&files)
        .into_iter()
        .map(|(name, pid)| format!("{name}（pid {pid}）"))
        .collect()
}

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>, depth: u32) {
    if depth > 8 || out.len() >= 2000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            collect(&e.path(), out, depth + 1);
        } else {
            out.push(e.path());
        }
        if out.len() >= 2000 {
            return;
        }
    }
}

#[cfg(windows)]
mod rm {
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::System::RestartManager::{
        RmEndSession, RmGetList, RmRegisterResources, RmStartSession, CCH_RM_SESSION_KEY,
        RM_PROCESS_INFO,
    };

    pub fn query(files: &[PathBuf]) -> Vec<(String, u32)> {
        if files.is_empty() {
            return Vec::new();
        }
        let wide: Vec<Vec<u16>> = files
            .iter()
            .map(|p| p.as_os_str().encode_wide().chain([0]).collect())
            .collect();
        let ptrs: Vec<PCWSTR> = wide.iter().map(|w| PCWSTR(w.as_ptr())).collect();
        let mut session = 0u32;
        let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
        // SAFETY: 所有指针都指向本函数里活着的缓冲区；会话在返回前结束。
        unsafe {
            if RmStartSession(&mut session, 0, PWSTR(key.as_mut_ptr())).0 != 0 {
                return Vec::new();
            }
            let mut out = Vec::new();
            if RmRegisterResources(session, Some(&ptrs), None, None).0 == 0 {
                let mut needed = 0u32;
                let mut count = 0u32;
                let mut reasons = 0u32;
                let _ = RmGetList(session, &mut needed, &mut count, None, &mut reasons);
                if needed > 0 {
                    let mut infos = vec![RM_PROCESS_INFO::default(); needed as usize + 4];
                    count = infos.len() as u32;
                    if RmGetList(
                        session,
                        &mut needed,
                        &mut count,
                        Some(infos.as_mut_ptr()),
                        &mut reasons,
                    )
                    .0 == 0
                    {
                        for info in &infos[..count as usize] {
                            let end = info
                                .strAppName
                                .iter()
                                .position(|c| *c == 0)
                                .unwrap_or(info.strAppName.len());
                            let name = String::from_utf16_lossy(&info.strAppName[..end]);
                            out.push((name, info.Process.dwProcessId));
                        }
                    }
                }
            }
            let _ = RmEndSession(session);
            out
        }
    }
}

#[cfg(not(windows))]
mod rm {
    pub fn query(_: &[std::path::PathBuf]) -> Vec<(String, u32)> {
        Vec::new()
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("qb-archive-dir-{}", crate::config_io::id()));
        std::fs::create_dir_all(d.join("slot/home")).unwrap();
        std::fs::write(d.join("slot/home/auth.json"), b"{}").unwrap();
        d
    }
    fn hold(p: &Path) -> std::fs::File {
        // FILE_SHARE_READ | FILE_SHARE_WRITE，故意不带 FILE_SHARE_DELETE。
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2)
            .open(p)
            .unwrap()
    }

    #[test]
    fn held_file_is_reported_busy_and_nothing_moves() {
        let d = tmp();
        let f = hold(&d.join("slot/home/auth.json"));
        let err = move_dir_within(
            &d.join("slot"),
            &d.join("moved"),
            Duration::from_millis(300),
        )
        .unwrap_err();
        assert!(is_busy(&err), "{err}");
        assert!(d.join("slot/home/auth.json").is_file());
        assert!(!d.join("moved").exists());
        let pid = std::process::id().to_string();
        assert!(err.to_string().contains(&pid), "{err}");
        drop(f);
        move_dir(&d.join("slot"), &d.join("moved")).unwrap();
        assert!(d.join("moved/home/auth.json").is_file());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn released_during_backoff_succeeds() {
        let d = tmp();
        let f = hold(&d.join("slot/home/auth.json"));
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            drop(f);
        });
        move_dir(&d.join("slot"), &d.join("moved")).unwrap();
        t.join().unwrap();
        assert!(d.join("moved/home/auth.json").is_file());
        let _ = std::fs::remove_dir_all(d);
    }
}
