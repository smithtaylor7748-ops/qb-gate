//! NTFS ACL 门禁。
//!
//! 门禁的本体不是脚本里的 if 判断，而是给 claude.exe 加一条针对**当前用户**的
//! `Deny ExecuteFile` ACE。锁上之后连双击都会被系统拒绝执行，绕不过去。
//!
//! 三条硬约束，改这个文件之前先读：
//!   1. 只用 Win32 ACL API，**不要**退化成调 icacls / takeown。
//!      那些外部命令会连带改掉继承、所有者，波及无关文件。
//!   2. `app-<版本>\claude.exe` **不能**加 Deny ACE ——
//!      一加，桌面端开新窗口就崩。那个副本只能靠看门狗 taskkill 收，
//!      过滤逻辑在 targets.rs，不在这里。
//!   3. 只 deny **当前用户**，不动 SYSTEM 与 Administrators ——
//!      这样出问题时管理员仍然能收拾局面。
//!
//! 查询用 `GetExplicitEntriesFromAclW` 而不是自己走 ACE 链表：后者要用到
//! `ACE_HEADER`，而它在 windows-rs 里挂在 Wdk 下面，为了读一个标志位
//! 去引整个 Wdk 不划算，而且原始 ACE 布局比这个 API 更容易随版本漂。

#![cfg(windows)]

use crate::error::{GateError, Result};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, LocalFree, ERROR_SUCCESS, HANDLE, HLOCAL};
use windows::Win32::Security::Authorization::{
    GetExplicitEntriesFromAclW, GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW,
    ACCESS_MODE, DENY_ACCESS, EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT,
    TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
};
use windows::Win32::Security::{
    EqualSid, GetLengthSid, GetTokenInformation, TokenUser, ACL as WIN_ACL,
    DACL_SECURITY_INFORMATION, NO_INHERITANCE, PSECURITY_DESCRIPTOR, PSID, TOKEN_QUERY, TOKEN_USER,
};
use windows::Win32::Storage::FileSystem::FILE_EXECUTE;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

fn wide(p: &Path) -> Vec<u16> {
    p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

fn win_err(api: &'static str, code: u32) -> GateError {
    GateError::WinApi { api, code }
}

/// 当前进程 token 里的用户 SID，按字节持有。
/// `psid()` 返回的是指向这段字节的裸指针，所以 `SidBuf` 活着期间不能移动它。
pub struct SidBuf(Vec<u8>);

impl SidBuf {
    pub fn psid(&self) -> PSID {
        PSID(self.0.as_ptr() as *mut c_void)
    }
}

/// 取当前用户的 SID。
pub fn current_user_sid() -> Result<SidBuf> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| win_err("OpenProcessToken", e.code().0 as u32))?;

        // 第一次调用只为问长度，必然失败，不判错。
        let mut needed = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
        if needed == 0 {
            let _ = CloseHandle(token);
            return Err(win_err("GetTokenInformation(size)", 0));
        }

        let mut buf = vec![0u8; needed as usize];
        let r = GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr() as *mut c_void),
            needed,
            &mut needed,
        );
        let _ = CloseHandle(token);
        r.map_err(|e| win_err("GetTokenInformation", e.code().0 as u32))?;

        let tu = &*(buf.as_ptr() as *const TOKEN_USER);
        let len = GetLengthSid(tu.User.Sid) as usize;
        let bytes = std::slice::from_raw_parts(tu.User.Sid.0 as *const u8, len).to_vec();
        Ok(SidBuf(bytes))
    }
}

/// 安全描述符 + 它内部的 DACL 指针。DACL 由描述符持有，不单独释放。
struct Dacl {
    sd: PSECURITY_DESCRIPTOR,
    dacl: *mut WIN_ACL,
}

impl Drop for Dacl {
    fn drop(&mut self) {
        if !self.sd.0.is_null() {
            unsafe {
                let _ = LocalFree(HLOCAL(self.sd.0));
            }
        }
    }
}

fn read_dacl(path: &Path) -> Result<Dacl> {
    if !path.exists() {
        return Err(GateError::NotFound(path.display().to_string()));
    }
    let w = wide(path);
    let mut dacl: *mut WIN_ACL = std::ptr::null_mut();
    let mut sd = PSECURITY_DESCRIPTOR::default();
    let rc = unsafe {
        GetNamedSecurityInfoW(
            PCWSTR(w.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut dacl),
            None,
            &mut sd,
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(win_err("GetNamedSecurityInfoW", rc.0));
    }
    Ok(Dacl { sd, dacl })
}

fn apply(path: &Path, sid: &SidBuf, mode: ACCESS_MODE) -> Result<()> {
    let cur = read_dacl(path)?;

    let mut ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_EXECUTE.0,
        grfAccessMode: mode,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: PWSTR(sid.psid().0 as *mut u16),
        },
    };

    let mut new_acl: *mut WIN_ACL = std::ptr::null_mut();
    let rc = unsafe {
        SetEntriesInAclW(
            Some(std::slice::from_mut(&mut ea)),
            Some(cur.dacl as *const WIN_ACL),
            &mut new_acl,
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(win_err("SetEntriesInAclW", rc.0));
    }

    let w = wide(path);
    let rc = unsafe {
        SetNamedSecurityInfoW(
            PCWSTR(w.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(new_acl as *const WIN_ACL),
            None,
        )
    };
    unsafe {
        let _ = LocalFree(HLOCAL(new_acl as *mut c_void));
    }
    if rc != ERROR_SUCCESS {
        return Err(win_err("SetNamedSecurityInfoW", rc.0));
    }
    Ok(())
}

/// 上锁：加一条 Deny ExecuteFile。已经锁着时再调一次是安全的（幂等）。
pub fn lock(path: &Path, sid: &SidBuf) -> Result<()> {
    apply(path, sid, DENY_ACCESS)
}

/// 这条显式条目是不是「我们加的那条锁」。
fn is_our_deny(e: &EXPLICIT_ACCESS_W, sid: &SidBuf) -> bool {
    if e.grfAccessMode != DENY_ACCESS
        || e.grfAccessPermissions & FILE_EXECUTE.0 == 0
        || e.Trustee.TrusteeForm != TRUSTEE_IS_SID
    {
        return false;
    }
    let ace_sid = PSID(e.Trustee.ptstrName.0 as *mut c_void);
    unsafe { EqualSid(ace_sid, sid.psid()).is_ok() }
}

/// 解锁：把我们那条 Deny ExecuteFile 摘掉。
///
/// **不要退回去用 `SetEntriesInAclW` + `REVOKE_ACCESS`。** 实测（见本文件
/// 的往返测试）它对 deny 条目会返回成功却什么都不做，结果就是锁上之后再也
/// 解不开 —— 面板会彻底打不开 Claude。
///
/// 现在的做法是自己重建 DACL：取出**显式**条目（`GetExplicitEntriesFromAclW`
/// 不返回继承来的那些），滤掉我们那条，再用 `oldacl = None` 从零建一个。
/// 因为没有设 PROTECTED 标志，继承来的权限在 `SetNamedSecurityInfoW` 之后
/// 会自动回来，所以滤剩空数组也是安全的 —— 那正是文件最初的样子。
pub fn unlock(path: &Path, sid: &SidBuf) -> Result<()> {
    let cur = read_dacl(path)?;
    if cur.dacl.is_null() {
        return Ok(());
    }

    let mut count = 0u32;
    let mut list: *mut EXPLICIT_ACCESS_W = std::ptr::null_mut();
    let rc = unsafe { GetExplicitEntriesFromAclW(cur.dacl, &mut count, &mut list) };
    if rc != ERROR_SUCCESS {
        return Err(win_err("GetExplicitEntriesFromAclW", rc.0));
    }

    let result = (|| -> Result<()> {
        let kept: Vec<EXPLICIT_ACCESS_W> = if list.is_null() || count == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(list, count as usize) }
                .iter()
                .filter(|e| !is_our_deny(e, sid))
                .copied()
                .collect()
        };

        // 没有我们的条目就别去动这个文件的 ACL。
        if kept.len() as u32 == count {
            return Ok(());
        }

        let mut new_acl: *mut WIN_ACL = std::ptr::null_mut();
        // oldacl = None：从零构建，只保留过滤后的显式条目。
        let rc = unsafe { SetEntriesInAclW(Some(&kept), None, &mut new_acl) };
        if rc != ERROR_SUCCESS {
            return Err(win_err("SetEntriesInAclW(revoke)", rc.0));
        }

        let w = wide(path);
        let rc = unsafe {
            SetNamedSecurityInfoW(
                PCWSTR(w.as_ptr()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(new_acl as *const WIN_ACL),
                None,
            )
        };
        unsafe {
            let _ = LocalFree(HLOCAL(new_acl as *mut c_void));
        }
        if rc != ERROR_SUCCESS {
            return Err(win_err("SetNamedSecurityInfoW(revoke)", rc.0));
        }
        Ok(())
    })();

    // kept 里的 SID 指针指向 list 缓冲区，必须等新 ACL 建完再释放。
    if !list.is_null() {
        unsafe {
            let _ = LocalFree(HLOCAL(list as *mut c_void));
        }
    }
    result
}

/// 查这个文件上有没有属于当前用户的 Deny ExecuteFile。
pub fn is_locked(path: &Path, sid: &SidBuf) -> Result<bool> {
    let cur = read_dacl(path)?;
    if cur.dacl.is_null() {
        // 没有 DACL 表示人人可访问，等于没锁。
        return Ok(false);
    }

    let mut count = 0u32;
    let mut list: *mut EXPLICIT_ACCESS_W = std::ptr::null_mut();
    let rc = unsafe { GetExplicitEntriesFromAclW(cur.dacl, &mut count, &mut list) };
    if rc != ERROR_SUCCESS {
        return Err(win_err("GetExplicitEntriesFromAclW", rc.0));
    }
    if list.is_null() || count == 0 {
        return Ok(false);
    }

    let found = unsafe {
        let f = std::slice::from_raw_parts(list, count as usize)
            .iter()
            .any(|e| is_our_deny(e, sid));
        let _ = LocalFree(HLOCAL(list as *mut c_void));
        f
    };
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ACL 往返：在**临时替身文件**上加锁、查、解锁。
    ///
    /// 刻意不碰任何真的 claude.exe —— 那会影响使用者当前正在跑的会话。
    /// 这个测试要回答的是「这套 FFI 到底管不管用」，替身文件足够。
    #[test]
    fn lock_query_unlock_roundtrip_on_a_stand_in_file() {
        let sid = match current_user_sid() {
            Ok(s) => s,
            // 拿不到 SID（受限环境）就跳过，不要把 CI 判红。
            Err(_) => return,
        };

        let path = std::env::temp_dir().join(format!("claude-gate-acl-{}.bin", std::process::id()));
        std::fs::write(&path, b"stand-in").expect("写替身文件");

        assert!(!is_locked(&path, &sid).unwrap(), "新建文件不该带锁");

        lock(&path, &sid).expect("上锁");
        assert!(is_locked(&path, &sid).unwrap(), "上锁后应查得到");

        // 幂等：再锁一次不该出错，也不该变成两条。
        lock(&path, &sid).expect("重复上锁应当无害");
        assert!(is_locked(&path, &sid).unwrap());

        unlock(&path, &sid).expect("解锁");
        assert!(!is_locked(&path, &sid).unwrap(), "解锁后不该再查到");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_reports_not_found_rather_than_panicking() {
        let sid = match current_user_sid() {
            Ok(s) => s,
            Err(_) => return,
        };
        let p = std::env::temp_dir().join("claude-gate-does-not-exist-xyz.bin");
        assert!(matches!(
            is_locked(&p, &sid),
            Err(GateError::NotFound(_))
        ));
    }
}
