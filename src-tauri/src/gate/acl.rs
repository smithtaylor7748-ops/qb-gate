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
    DENY_ACCESS, EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, TRUSTEE_IS_SID,
    TRUSTEE_IS_USER, TRUSTEE_W,
};
use windows::Win32::Security::{
    EqualSid, GetLengthSid, GetTokenInformation, InitializeAcl, TokenUser, ACL as WIN_ACL,
    ACL_REVISION, DACL_SECURITY_INFORMATION, NO_INHERITANCE, PSECURITY_DESCRIPTOR, PSID,
    TOKEN_QUERY, TOKEN_USER, UNPROTECTED_DACL_SECURITY_INFORMATION,
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

/// 把一个 DACL 写到文件上，并**要求系统重新套用父目录的可继承 ACE**。
///
/// ⚠ 不带 `UNPROTECTED_DACL_SECURITY_INFORMATION` 并**不等于**「继承会自动
/// 回来」，只等于「不改这个文件的继承开关」。这里原来那句注释写反了，
/// 代价见 `unlock`。而我们每次都是从「显式条目」重建 DACL，
/// `GetExplicitEntriesFromAclW` 又根本不返回继承来的那些 —— 不主动把继承
/// 打开，文件就只剩我们写进去的那几条。
///
/// 为什么是「一律打开」而不是「文件原来挡着继承就尊重它」：
/// 本项目锁的这几个 exe，正常状态都是「不挡继承、权限全部来自父目录」。
/// 2026-09-10 实机核对 ——
///
/// | 文件 | protected | 继承 ACE | 显式 ACE |
/// |---|:--:|:--:|:--:|
/// | `AnthropicClaude\app-1.49585.0\claude.exe`（门禁从不碰） | 否 | 3 | 0 |
/// | `AnthropicClaude\Update.exe`（门禁从不碰） | 否 | 3 | 0 |
/// | 被门禁锁过的那四份 | **是** | **0** | 1–2 |
///
/// 也就是说 `SE_DACL_PROTECTED` 在这些文件上**本身就是损坏的一部分**，
/// 是老版本写 NULL DACL 时一起带上去的，不是使用者的本意，不该拿来当依据。
fn write_dacl(path: &Path, dacl: *const WIN_ACL) -> Result<()> {
    let info = DACL_SECURITY_INFORMATION | UNPROTECTED_DACL_SECURITY_INFORMATION;
    let w = wide(path);
    let rc = unsafe {
        SetNamedSecurityInfoW(
            PCWSTR(w.as_ptr()),
            SE_FILE_OBJECT,
            info,
            None,
            None,
            Some(dacl),
            None,
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(win_err("SetNamedSecurityInfoW", rc.0));
    }
    Ok(())
}

/// 把文件恢复成「没有任何显式条目、权限全部来自继承」——也就是一份刚装好的
/// `claude.exe` 本来的样子。
///
/// ⚠ 写进去的是一个**长度为零的合法 ACL**，不是空指针。两者在 Windows 里的
/// 含义正好相反：
///
/// | 写进去的 | 含义 |
/// |---|---|
/// | 零条目 ACL | 没有显式条目 → 配合 UNPROTECTED，父目录的可继承 ACE 贴回来 |
/// | NULL DACL | **不做任何访问检查 → 人人完全控制** |
///
/// 老版本 `unlock` 把 `SetEntriesInAclW` 在「一条条目都不剩」时吐出来的空指针
/// 直接写了下去，于是**每解锁一次就把一份 claude.exe 变成人人可改**。
/// 本机四份副本 2026-09-10 实测全都停在这个状态（`icacls` 报
/// 「No permissions are set. All users have full control.」）。
fn restore_inherited(path: &Path) -> Result<()> {
    let mut acl = WIN_ACL::default();
    unsafe {
        InitializeAcl(
            &mut acl,
            std::mem::size_of::<WIN_ACL>() as u32,
            ACL_REVISION,
        )
        .map_err(|e| win_err("InitializeAcl", e.code().0 as u32))?;
    }
    write_dacl(path, &acl)
}

/// 上锁：加一条 Deny ExecuteFile。已经锁着时再调一次是安全的（幂等）。
pub fn lock(path: &Path, sid: &SidBuf) -> Result<()> {
    let cur = read_dacl(path)?;

    // 这里最要紧的一件事，是**上锁不能把文件锁到读都读不了**。
    //
    // 老版本 unlock 会把 DACL 写成空指针（人人完全控制）。在那种文件上再上锁，
    // `SetEntriesInAclW` 没有旧条目可合并，建出来的新 DACL 里就**只有**那条
    // Deny —— 一条 allow 都没有。后果不是「锁上了」而是「这个文件消失了」：
    // `installed_version()` 读不出版本号，「环境与安装」页于是把装好的
    // Claude Code 显示成「未安装」。`write_dacl` 一律带上 UNPROTECTED，
    // 父目录的可继承 ACE 会跟这条 Deny 一起回来，读的权限不会跟着丢。
    let mut ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_EXECUTE.0,
        grfAccessMode: DENY_ACCESS,
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
    let r = write_dacl(path, new_acl);
    unsafe {
        let _ = LocalFree(HLOCAL(new_acl as *mut c_void));
    }
    r
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
///
/// ⚠ **2026-09-10 修**：原来这里还有一句「因为没有设 PROTECTED 标志，继承来的
/// 权限会自动回来，所以滤剩空数组也是安全的」——两个判断都是错的，实机验证：
///
///   1. 不设 PROTECTED 只是「不改这个文件的继承开关」，不会让继承回来。
///      要让父目录的可继承 ACE 重新贴上，必须**主动**带 UNPROTECTED。
///   2. 滤剩空数组一点都不安全：`SetEntriesInAclW` 在没有条目时返回的是
///      **空指针**，把它写下去就是 NULL DACL —— 人人完全控制。
///
/// 而 claude.exe 的常态恰好就是「唯一的显式条目就是我们加的那条 Deny」，
/// 所以这条路径**每次解锁都会走到**。连锁反应：
///
///   解锁 → NULL DACL（人人可改）→ 下次上锁没有旧条目可合并 → DACL 里只剩
///   一条 Deny，一条 allow 都没有 → 当前用户连版本号都读不出来 →
///   升级页把装好的 Claude Code 报成「未安装」。
pub fn unlock(path: &Path, sid: &SidBuf) -> Result<()> {
    let cur = read_dacl(path)?;
    // 空指针 DACL 不是「已经解锁了」，是**坏了**。老版本在这里直接 return Ok，
    // 等于把损坏状态原样留着。修回去：这是本机四份副本的自愈入口。
    if cur.dacl.is_null() {
        return restore_inherited(path);
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

        // 一条显式条目都不剩 —— 这就是 claude.exe 的常态。**绝不能**把
        // `SetEntriesInAclW` 这时吐出来的空指针写下去，那是 NULL DACL。
        if kept.is_empty() {
            return restore_inherited(path);
        }

        let mut new_acl: *mut WIN_ACL = std::ptr::null_mut();
        // oldacl = None：从零构建，只保留过滤后的显式条目。
        let rc = unsafe { SetEntriesInAclW(Some(&kept), None, &mut new_acl) };
        if rc != ERROR_SUCCESS {
            return Err(win_err("SetEntriesInAclW(revoke)", rc.0));
        }
        if new_acl.is_null() {
            // 上面已经挡掉了 kept 为空的情况，走到这里说明 API 的行为跟
            // 预期不一样。宁可退回「靠继承」也不要写 NULL DACL。
            return restore_inherited(path);
        }

        // 保留的这几条是**显式**条目，继承来的那半边在重建时丢了，
        // 靠 `write_dacl` 里那个 UNPROTECTED 把它们贴回来。
        let r = write_dacl(path, new_acl);
        unsafe {
            let _ = LocalFree(HLOCAL(new_acl as *mut c_void));
        }
        r
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

/// 一个路径所在的卷用的是什么文件系统、支不支持权限（ACL）。
///
/// 给「选托管目录时当场实测」用（v0.9.0）：exFAT / FAT32 的 U 盘、移动硬盘
/// 根本没有 Windows 权限系统，Deny ExecuteFile 加不上去。与其装进去之后
/// 才发现「锁不上」，不如选的时候就说清楚。
///
/// 返回 `(文件系统名, 是否支持持久 ACL)`。查不到就是 `None`（比如路径所在的盘不存在）。
pub fn volume_filesystem(path: &Path) -> Option<(String, bool)> {
    use windows::Win32::Storage::FileSystem::{GetVolumeInformationW, GetVolumePathNameW};
    // FILE_PERSISTENT_ACLS：这个卷会保存并执行 ACL。NTFS / ReFS 有，FAT 系列没有。
    const FILE_PERSISTENT_ACLS: u32 = 0x0000_0008;

    let p = wide(path);
    let mut root = vec![0u16; 1024];
    unsafe { GetVolumePathNameW(PCWSTR(p.as_ptr()), &mut root) }.ok()?;

    let mut fs_name = vec![0u16; 64];
    let mut flags = 0u32;
    unsafe {
        GetVolumeInformationW(
            PCWSTR(root.as_ptr()),
            None,
            None,
            None,
            Some(&mut flags),
            Some(&mut fs_name),
        )
    }
    .ok()?;
    let end = fs_name.iter().position(|&c| c == 0).unwrap_or(fs_name.len());
    Some((
        String::from_utf16_lossy(&fs_name[..end]),
        flags & FILE_PERSISTENT_ACLS != 0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_temp_dir_is_on_a_volume_that_keeps_acls() {
        // 系统盘（临时目录所在）总是 NTFS。查不到就跳过，不把受限环境判红。
        if let Some((fs, acls)) = volume_filesystem(&std::env::temp_dir()) {
            assert!(!fs.is_empty());
            assert!(acls, "{fs} 应当支持 ACL");
        }
    }

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

        let path = std::env::temp_dir().join(format!("qb-gate-acl-{}.bin", std::process::id()));
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

    /// 解锁之后 DACL **不能**是空指针。
    ///
    /// 回归测试：老版本在「滤剩零条显式条目」时把 `SetEntriesInAclW` 返回的
    /// 空指针直接写了下去。Windows 对 NULL DACL 的解释是「不做访问检查」——
    /// 也就是人人完全控制。看起来解锁成功了，实际上是把这份 claude.exe
    /// 交给了机器上的所有人。
    #[test]
    fn unlock_never_leaves_a_null_dacl() {
        let sid = match current_user_sid() {
            Ok(s) => s,
            Err(_) => return,
        };
        let path =
            std::env::temp_dir().join(format!("qb-gate-nulldacl-{}.bin", std::process::id()));
        std::fs::write(&path, b"stand-in").expect("写替身文件");

        lock(&path, &sid).expect("上锁");
        unlock(&path, &sid).expect("解锁");

        let after = read_dacl(&path).expect("解锁后读 DACL");
        assert!(
            !after.dacl.is_null(),
            "解锁后 DACL 是空指针 —— 这等于人人完全控制，不是「恢复原样」"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// 锁 → 解 → 再锁，文件必须仍然读得出来。
    ///
    /// 回归测试：NULL DACL 之后再上锁，没有旧条目可以合并，新 DACL 里就只剩
    /// 一条 Deny、一条 allow 都没有 —— 当前用户连读都读不了。实机后果是
    /// `installed_version()` 读不出版本号，升级页把装好的 Claude Code
    /// 报成「未安装」，装完之后那句提示也变成「已安装 。」。
    ///
    /// Deny 的是 ExecuteFile，**读**从来就不该受影响。
    #[test]
    fn a_second_lock_cycle_still_leaves_the_file_readable() {
        let sid = match current_user_sid() {
            Ok(s) => s,
            Err(_) => return,
        };
        let path =
            std::env::temp_dir().join(format!("qb-gate-relock-{}.bin", std::process::id()));
        std::fs::write(&path, b"stand-in").expect("写替身文件");

        lock(&path, &sid).expect("上锁");
        unlock(&path, &sid).expect("解锁");
        lock(&path, &sid).expect("再上锁");

        assert!(is_locked(&path, &sid).unwrap(), "第二次上锁应当生效");
        assert_eq!(
            std::fs::read(&path).expect("锁着也必须读得到 —— Deny 的是执行不是读"),
            b"stand-in"
        );

        unlock(&path, &sid).expect("解锁");
        let _ = std::fs::remove_file(&path);
    }

    /// 造出老版本留下的那种坏 DACL：**挡着继承、只有一条 Deny、一条 allow 都没有**。
    ///
    /// 这不是假想的状态，是 2026-09-10 实机上四份 claude.exe 的原样：
    /// `control=0x9404`（含 `SE_DACL_PROTECTED`）、0 条继承 ACE、
    /// `icacls` 只列得出一行 `DESKTOP-…<用户名>:(DENY)(X)`。
    fn damage(path: &Path, sid: &SidBuf) {
        let mut ea = EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_EXECUTE.0,
            grfAccessMode: DENY_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_USER,
                ptstrName: PWSTR(sid.psid().0 as *mut u16),
            },
        };
        let mut acl: *mut WIN_ACL = std::ptr::null_mut();
        let w = wide(path);
        unsafe {
            let rc = SetEntriesInAclW(Some(std::slice::from_mut(&mut ea)), None, &mut acl);
            assert_eq!(rc, ERROR_SUCCESS, "造损坏状态：建 ACL");
            let rc = SetNamedSecurityInfoW(
                PCWSTR(w.as_ptr()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION
                    | windows::Win32::Security::PROTECTED_DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(acl as *const WIN_ACL),
                None,
            );
            assert_eq!(rc, ERROR_SUCCESS, "造损坏状态：写 DACL");
            let _ = LocalFree(HLOCAL(acl as *mut c_void));
        }
    }

    /// 在**已经坏掉**的文件上再上锁，必须把它救回来，而不是维持原样。
    ///
    /// 回归测试。第一版修复只在「DACL 是空指针」时才修，结果实机上没生效 ——
    /// 那四份文件当时已经被上一轮上锁推进了「挡继承 + 只剩一条 Deny」的
    /// 下一档，`read_dacl` 拿到的是一个非空但一条 allow 都没有的 DACL，
    /// 修复路径根本没进去。装完新面板 `icacls` 依旧只列出一行 `(DENY)(X)`，
    /// 版本号依旧读不出来。
    #[test]
    fn locking_a_damaged_file_repairs_it_instead_of_keeping_it_unreadable() {
        let sid = match current_user_sid() {
            Ok(s) => s,
            Err(_) => return,
        };
        let path =
            std::env::temp_dir().join(format!("qb-gate-damaged-{}.bin", std::process::id()));
        std::fs::write(&path, b"stand-in").expect("写替身文件");

        damage(&path, &sid);
        assert!(
            std::fs::read(&path).is_err(),
            "先确认这个损坏状态是真的读不了 —— 否则这条测试什么都没测到"
        );

        lock(&path, &sid).expect("在坏掉的文件上上锁");
        assert!(is_locked(&path, &sid).unwrap(), "上锁要生效");
        assert_eq!(
            std::fs::read(&path).expect("上锁之后必须重新读得到"),
            b"stand-in"
        );

        unlock(&path, &sid).expect("解锁");
        assert!(!is_locked(&path, &sid).unwrap());
        assert!(std::fs::read(&path).is_ok(), "解锁之后当然也要读得到");

        let _ = std::fs::remove_file(&path);
    }

    /// 同样的损坏状态，直接解锁也要能救回来。
    ///
    /// 走的是另一条路径：`unlock` 那边滤剩零条显式条目。使用者点「应急解锁」
    /// 时走的就是这条，不该要求他先上一次锁。
    #[test]
    fn unlocking_a_damaged_file_also_repairs_it() {
        let sid = match current_user_sid() {
            Ok(s) => s,
            Err(_) => return,
        };
        let path =
            std::env::temp_dir().join(format!("qb-gate-damaged2-{}.bin", std::process::id()));
        std::fs::write(&path, b"stand-in").expect("写替身文件");

        damage(&path, &sid);
        assert!(std::fs::read(&path).is_err());

        unlock(&path, &sid).expect("解锁");
        assert!(!is_locked(&path, &sid).unwrap());
        assert_eq!(
            std::fs::read(&path).expect("解锁之后必须读得到"),
            b"stand-in"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_reports_not_found_rather_than_panicking() {
        let sid = match current_user_sid() {
            Ok(s) => s,
            Err(_) => return,
        };
        let p = std::env::temp_dir().join("qb-gate-does-not-exist-xyz.bin");
        assert!(matches!(
            is_locked(&p, &sid),
            Err(GateError::NotFound(_))
        ));
    }
}
