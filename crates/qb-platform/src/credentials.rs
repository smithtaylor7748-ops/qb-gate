//! Windows 凭据管理器：问一条通用凭据在不在，必要时把它的 blob 读出来（0.32.0）。
//!
//! # 为什么会有这个模块
//!
//! 反重力 Hub 把它的登录**只**存在凭据管理器里（目标名 `gemini:antigravity`）——
//! 2026-09-21 实机核过：`%APPDATA%\Antigravity\app_storage.json` 的 16 个键里没有账户，
//! `~\.gemini\antigravity\antigravity_state.pbtxt` 只有引导与迁移状态，
//! Local Storage 里搜不到邮箱，`language_server.log` 只有一句 `Auth succeeded`。
//! **Hub 在本机一个字的身份都没写。** 所以「Hub 登了哪个账户」这个问题，
//! 除了问凭据管理器没有别的答法。
//!
//! # ⛔ 两个函数，两种代价，别混用
//!
//! | | 碰不碰令牌 | 用在哪 |
//! |---|---|---|
//! | [`exists`] | **不碰**。只问「有没有这条」 | 「Hub 登没登录」——默认走这条 |
//! | [`read_generic`] | **碰**。把 blob 读进内存 | 两处：`qb-accounts::antigravity::hub::identity`（只取 `id_token` 的载荷解邮箱）与 `qb-accounts::antigravity::token::hub_token`（使用者点反重力的刷新图标时，取令牌去问额度） |
//!
//! [`read_generic`] 是 0.32.0 使用者拍板开的口子（他要 Hub 那一格显示账户，而那
//! 只有拿 Hub 自己的令牌去问 Google 一条路）。它推翻了 `CLAUDE.md` 里
//! 「令牌一个字不碰」那一条 —— 推翻的理由、边界与文档改动都记在
//! `CLAUDE.md`「联网额度」那一节与 `DISCLAIMER.md` §6。2026-09-23 使用者又拍板
//! 「令牌过期时面板在内存里换新」，换来的访问令牌同样只装在 [`Secret`] 里。
//!
//! 三条守着它的规矩：
//!
//! 1. **返回值一离开作用域就抹零**（[`Secret`] 的 `Drop`）。别把它 `clone()` 到
//!    任何会被序列化的结构里 —— `AntigravityStatus` 里没有、也永远不许有令牌字段。
//! 2. **不进日志、不进事件、不进审计**。审计只写「问了一次 Hub 的账户」，不写内容。
//! 3. **非 Windows 一律返回「没有」**，本项目只发 Windows 版；这条分支是为了
//!    `cargo test` 在别的平台上跑得起来。

use crate::error::Result;

/// 一段读出来就要抹掉的字节。
///
/// 不实现 `Debug` / `Serialize` / `Clone` —— 这三样都是「令牌不小心跑出去」的现成出口。
pub struct Secret(Vec<u8>);

impl Secret {
    /// 把一段刚读出来的令牌装进来。之后它离开作用域就抹零（2026-09-23：反重力 IDE 槽位里
    /// 那一行令牌、换新令牌拿回来的访问令牌都装在这里）。
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
    /// 当 UTF-8 文本看。令牌都是 ASCII；不是 UTF-8 就是 `None`。
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        // 抹零。`write_volatile` 是为了不让优化器把这段循环整个删掉。
        for b in self.0.iter_mut() {
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

/// 这条凭据在不在。**不读内容。**
#[cfg(windows)]
pub fn exists(target: &str) -> bool {
    read_raw(target).map(|o| o.is_some()).unwrap_or(false)
}

#[cfg(not(windows))]
pub fn exists(_target: &str) -> bool {
    false
}

/// 把这条凭据的 blob 读出来。读不到（没这条 / 没权限）就是 `Ok(None)`。
///
/// ⛔ 调用方只有一处，见模块头。
#[cfg(windows)]
pub fn read_generic(target: &str) -> Result<Option<Secret>> {
    read_raw(target)
}

#[cfg(not(windows))]
pub fn read_generic(_target: &str) -> Result<Option<Secret>> {
    Ok(None)
}

#[cfg(windows)]
fn read_raw(target: &str) -> Result<Option<Secret>> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    let mut ptr: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `wide` 是以 NUL 结尾的 UTF-16；`ptr` 由 CredReadW 写入，成功时
    // 必须用 CredFree 释放，下面两条路径都走到了。
    let ok = unsafe { CredReadW(PCWSTR(wide.as_ptr()), CRED_TYPE_GENERIC, 0, &mut ptr) };
    if let Err(e) = ok {
        // 「没有这条」不是错误，是一个答案。
        if e.code() == ERROR_NOT_FOUND.to_hresult() {
            return Ok(None);
        }
        return Err(crate::error::GateError::Other(format!(
            "读凭据管理器失败（{target}）：{e}"
        )));
    }
    if ptr.is_null() {
        return Ok(None);
    }
    // SAFETY: CredReadW 成功且指针非空，结构体与它指向的 blob 在 CredFree 之前有效。
    let bytes = unsafe {
        let c = &*ptr;
        let len = c.CredentialBlobSize as usize;
        let out = if len == 0 || c.CredentialBlob.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(c.CredentialBlob, len).to_vec()
        };
        CredFree(ptr as *const _);
        out
    };
    Ok(Some(Secret(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_that_cannot_exist_answers_no_instead_of_erroring() {
        // 单测不许碰真实的运行期状态：这里问的是一个不可能存在的名字，
        // 要的只是「『没有』走的是 Ok(None) 那条路，不是 Err」。
        let name = "qb-gate-test-credential-that-never-exists-4f2b";
        assert!(!exists(name));
        assert!(read_generic(name).expect("『没有』不是错误").is_none());
    }

    #[test]
    fn a_secret_zeroes_itself_and_never_grows_a_debug_impl() {
        let s = Secret(vec![1, 2, 3]);
        assert_eq!(s.len(), 3);
        assert!(!s.is_empty());
        assert_eq!(s.as_bytes(), &[1, 2, 3]);
        // 这一行是给下一个人看的：`Secret` 不许有 Debug / Serialize / Clone。
        // 加上任何一个，令牌就有了一条能跑进日志或 IPC 的路。
        drop(s);
    }
}
