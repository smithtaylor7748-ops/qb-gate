//! API Key 的落盘加密。
//!
//! # 为什么需要这个模块
//!
//! 中转站从「单条配置」变成「多条可切换」之后，Key **必须存下来** ——
//! 否则每切一次供应商都要重新输一遍，一键切换就没有意义了。
//!
//! 但 `relay/mod.rs` 文件头那条不变量还得成立：
//! **API Key 不落明文到本项目自己的配置里。**
//!
//! 两个要求同时满足的办法是 Windows DPAPI（`CryptProtectData`）：
//! 密文只有**同一个 Windows 用户**在**同一台机器**上解得开。
//! 把 `relay.json` 拷走、或者换个账户登录，里面的 Key 都是一串没用的十六进制。
//!
//! # 这不是万能的
//!
//! DPAPI 防的是「配置文件被拷走 / 被别的账户读到」。它**防不住**
//! 已经拿到本用户会话的代码 —— 那种情况下攻击者本来就能直接调
//! `CryptUnprotectData`。这跟门禁的定位一致：防的是数据外流，
//! 不是防本机管理员。
//!
//! # 非 Windows
//!
//! 退回明文并在密文前打 `plain:` 前缀。本项目只发 Windows 版，
//! 这条分支纯粹是为了 `cargo test` 能在别的平台上跑起来。

use crate::error::Result;

/// 密文的前缀。带前缀是为了能分辨「这是 DPAPI 密文」还是
/// 「这是早期版本留下的明文」，升级时不至于把明文当密文去解。
const SEALED: &str = "dpapi:";
const PLAIN: &str = "plain:";

/// 加密。返回值可以安全地写进 `relay.json`。
pub fn seal(plain: &str) -> Result<String> {
    if plain.is_empty() {
        return Ok(String::new());
    }
    seal_impl(plain)
}

/// 解密。认三种输入：本模块产出的两种前缀，以及**没有前缀的裸明文**
/// （早期版本或用户手改配置留下的），裸明文原样返回。
pub fn open(stored: &str) -> Option<String> {
    if stored.is_empty() {
        return None;
    }
    if let Some(hexed) = stored.strip_prefix(SEALED) {
        return open_impl(hexed);
    }
    if let Some(p) = stored.strip_prefix(PLAIN) {
        return Some(p.to_string());
    }
    // 没前缀 = 裸明文。照收，下次保存时会被换成密文。
    Some(stored.to_string())
}

/// 这条记录里的 Key 是加密存的吗？界面上要如实告诉用户。
pub fn is_sealed(stored: &str) -> bool {
    stored.starts_with(SEALED)
}

// ------------------------------------------------------------ Windows

#[cfg(windows)]
fn seal_impl(plain: &str) -> Result<String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    use windows::core::PCWSTR;

    let mut input = plain.as_bytes().to_vec();
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_mut_ptr(),
    };
    let mut out = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptProtectData(
            &in_blob,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .map_err(|e| crate::error::GateError::Other(format!("加密 API Key 失败：{e}")))?;

        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        // DPAPI 的输出缓冲区归调用方释放，忘了就是每存一次漏一次。
        let _ = LocalFree(HLOCAL(out.pbData as *mut core::ffi::c_void));
        Ok(format!("{SEALED}{}", hex::encode(bytes)))
    }
}

#[cfg(windows)]
fn open_impl(hexed: &str) -> Option<String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let mut raw = hex::decode(hexed).ok()?;
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: raw.len() as u32,
        pbData: raw.as_mut_ptr(),
    };
    let mut out = CRYPT_INTEGER_BLOB::default();

    unsafe {
        // 换了机器或换了 Windows 用户就会走到这里。**不能 panic** ——
        // 那意味着换台机器打开面板就直接崩。返回 None，界面上显示
        // 「这条的 Key 解不开，请重新填」。
        CryptUnprotectData(
            &in_blob,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .ok()?;

        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(HLOCAL(out.pbData as *mut core::ffi::c_void));
        String::from_utf8(bytes).ok()
    }
}

// -------------------------------------------------------- 非 Windows

#[cfg(not(windows))]
fn seal_impl(plain: &str) -> Result<String> {
    Ok(format!("{PLAIN}{plain}"))
}

#[cfg(not(windows))]
fn open_impl(_hexed: &str) -> Option<String> {
    // 非 Windows 上根本产不出 dpapi: 前缀的东西。
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_key_stays_empty() {
        assert_eq!(seal("").unwrap(), "");
        assert_eq!(open(""), None);
    }

    #[test]
    fn round_trips() {
        let sealed = seal("sk-secret-value").unwrap();
        // 密文里不能出现明文，否则加密就白做了。
        assert!(!sealed.contains("sk-secret-value"));
        assert_eq!(open(&sealed).as_deref(), Some("sk-secret-value"));
    }

    #[test]
    fn bare_plaintext_is_accepted_for_backward_compat() {
        // 早期版本或用户手改留下的裸明文，读得出来，下次保存会换成密文。
        assert_eq!(open("sk-legacy").as_deref(), Some("sk-legacy"));
        assert!(!is_sealed("sk-legacy"));
    }

    #[test]
    fn undecryptable_ciphertext_returns_none_not_panic() {
        // 换机器 / 换 Windows 用户会走到这里。panic 意味着面板直接崩。
        assert_eq!(open("dpapi:not-hex-at-all"), None);
        assert_eq!(open("dpapi:00112233"), None);
    }

    #[cfg(windows)]
    #[test]
    fn sealed_values_carry_the_marker() {
        let sealed = seal("sk-x").unwrap();
        assert!(is_sealed(&sealed));
        assert!(sealed.starts_with("dpapi:"));
    }
}
