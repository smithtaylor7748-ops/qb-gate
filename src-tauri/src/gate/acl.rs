//! NTFS ACL 门禁。
//!
//! 门禁的本体不是脚本里的 if 判断，而是给 claude.exe 加一条针对**当前用户**的
//! `Deny ExecuteFile` ACE。锁上之后连双击都会被系统拒绝执行，绕不过去。
//!
//! 两条硬约束，改这个文件之前先读：
//!   1. 只用 Win32 ACL API，**不要**退化成调 icacls / takeown。
//!      那些外部命令会连带改掉继承、所有者，波及无关文件。
//!   2. `app-<版本>\claude.exe` **不能**加 Deny ACE ——
//!      一加，桌面端开新窗口就崩。那个副本只能靠看门狗 taskkill 收，
//!      过滤逻辑在 targets.rs，不在这里。

#![cfg(windows)]

use crate::error::{GateError, Result};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, LocalFree, ERROR_SUCCESS, HANDLE, HLOCAL, PSID,
};
use windows::Win32::Security::Authorization::{
    GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W,
    NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
};
use windows::Win32::Security::{
    EqualSid, GetAce, ACCESS_DENIED_ACE, ACE_HEADER, ACL as WIN_ACL, ACCESS_DENIED_ACE_TYPE,
    DACL_SECURITY_INFORMATION, DENY_ACCESS, NO_INHERITANCE, PSECURITY_DESCRIPTOR, REVOKE_ACCESS,
    TOKEN_QUERY, TOKEN_USER, TokenUser,
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
/// PSID 只是指向这段字节的裸指针，所以 `SidBuf` 活着期间不能移动它。
pub struct SidBuf(Vec<u8>);

impl SidBuf {
    pub fn psid(&self) -> PSID {
        PSID(self.0.as_ptr() as *mut c_void)
    }
}

/// 取当前用户的 SID。门禁只 deny 当前身份 —— 不动 SYSTEM、不动 Administrators，
/// 这样出问题时管理员仍然能收拾局面。
pub fn current_user_sid() -> Result<SidBuf> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| win_err("OpenProcessToken", e.code().0 as u32))?;

        let mut needed = 0u32;
        // 第一次调用只为问长度，必然返回 false，不判错。
        let _ = windows::Win32::Security::GetTokenInformation(
            token, TokenUser, None, 0, &mut needed,
        );
        if needed == 0 {
            let _ = CloseHandle(token);
            return Err(win_err("GetTokenInformation(size)", 0));
        }

        let mut buf = vec![0u8; needed as usize];
        let r = windows::Win32::Security::GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr() as *mut c_void),
            needed,
            &mut needed,
        );
        let _ = CloseHandle(token);
        r.map_err(|e| win_err("GetTokenInformation", e.code().0 as u32))?;

        let tu = &*(buf.as_ptr() as *const TOKEN_USER);
        let sid_ptr = tu.User.Sid.0 as *const u8;
        let sid_len = windows::Win32::Security::GetLengthSid(tu.User.Sid) as usize;
        Ok(SidBuf(std::slice::from_raw_parts(sid_ptr, sid_len).to_vec()))
    }
}

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

fn apply(path: &Path, sid: &SidBuf, mode: windows::Win32::Security::ACCESS_MODE) -> Result<()> {
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
            PSID::default(),
            PSID::default(),
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

/// 解锁：撤掉那条 ACE。
///
/// REVOKE_ACCESS 会把该 trustee 的**全部**显式条目移除，包括 allow。
/// 这里能这么用，是因为门禁从来只给当前用户加过 deny 这一条；
/// 如果哪天改成也加 allow，这里必须换成精确删除。
pub fn unlock(path: &Path, sid: &SidBuf) -> Result<()> {
    apply(path, sid, REVOKE_ACCESS)
}

/// 查这个文件上有没有属于当前用户的 Deny ExecuteFile。
pub fn is_locked(path: &Path, sid: &SidBuf) -> Result<bool> {
    let cur = read_dacl(path)?;
    if cur.dacl.is_null() {
        return Ok(false);
    }
    unsafe {
        let count = (*cur.dacl).AceCount as u32;
        for i in 0..count {
            let mut ace: *mut c_void = std::ptr::null_mut();
            if GetAce(cur.dacl, i, &mut ace).is_err() {
                continue;
            }
            let header = &*(ace as *const ACE_HEADER);
            if header.AceType != ACCESS_DENIED_ACE_TYPE as u8 {
                continue;
            }
            let denied = &*(ace as *const ACCESS_DENIED_ACE);
            if denied.Mask & FILE_EXECUTE.0 == 0 {
                continue;
            }
            let ace_sid = PSID(std::ptr::addr_of!(denied.SidStart) as *mut c_void);
            if EqualSid(ace_sid, sid.psid()).is_ok() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
