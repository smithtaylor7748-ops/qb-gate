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
//! macOS 使用 Keychain 保存随机主密钥，以 AES-256-GCM 加密配置与恢复记录。
//! Keychain 不可访问时拒绝保存；其他未支持的平台也不退回明文。

use crate::error::Result;

/// 密文的前缀。带前缀是为了能分辨「这是 DPAPI 密文」还是
/// 「这是早期版本留下的明文」，升级时不至于把明文当密文去解。
const SEALED: &str = "dpapi:";
const PLAIN: &str = "plain:";
const KEYCHAIN: &str = "keychain-aes-gcm-v1:";

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
    if let Some(hexed) = stored.strip_prefix(KEYCHAIN) {
        #[cfg(target_os = "macos")]
        return mac::open(hexed);
        #[cfg(not(target_os = "macos"))]
        {
            let _ = hexed;
            return None;
        }
    }
    if let Some(p) = stored.strip_prefix(PLAIN) {
        return Some(p.to_string());
    }
    // 没前缀 = 裸明文。照收，下次保存时会被换成密文。
    Some(stored.to_string())
}

// ------------------------------------------------------------ Windows

#[cfg(windows)]
fn seal_impl(plain: &str) -> Result<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

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

#[cfg(target_os = "macos")]
fn seal_impl(plain: &str) -> Result<String> {
    mac::seal(plain)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn seal_impl(plain: &str) -> Result<String> {
    let _ = plain;
    Err(crate::error::GateError::Other(
        "当前平台没有安全密钥存储，拒绝明文保存".into(),
    ))
}

#[cfg(not(windows))]
fn open_impl(_hexed: &str) -> Option<String> {
    // 非 Windows 上根本产不出 dpapi: 前缀的东西。
    None
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use crate::error::GateError;
    use ring::{
        aead,
        rand::{SecureRandom, SystemRandom},
    };
    use security_framework::os::macos::keychain::SecKeychain;
    use security_framework::passwords::{generic_password, PasswordOptions};
    use std::sync::{Mutex, OnceLock};

    const SERVICE: &str = "com.qbgate.panel.secrets";
    const ACCOUNT: &str = "configuration-master-v1";
    const AAD: &[u8] = b"QB Gate configuration v1";
    static ACCESS: Mutex<()> = Mutex::new(());

    fn failure() -> GateError {
        GateError::Other(
            "无法访问 macOS 钥匙串，未保存密钥；请解锁登录钥匙串并允许 QB Gate 访问".into(),
        )
    }

    fn keychain_failure(operation: &str, error: security_framework::Error) -> GateError {
        // OSStatus and a fixed operation label diagnose access failures without exposing keys.
        GateError::Other(format!(
            "无法访问 macOS 钥匙串（{operation}，OSStatus {}），未保存密钥；请解锁登录钥匙串并允许 QB Gate 访问",
            error.code()
        ))
    }

    fn key(create: bool) -> Result<crate::credentials::Secret> {
        // Downstream crate tests also pass through here: never access the user's real Keychain.
        if qb_foundation::test_context::running_in_a_test_binary(
            std::env::current_exe().ok().as_deref(),
        ) {
            static TEST_KEY: OnceLock<[u8; 32]> = OnceLock::new();
            let value = TEST_KEY.get_or_init(|| {
                let mut bytes = [0; 32];
                SystemRandom::new()
                    .fill(&mut bytes)
                    .expect("test random key");
                bytes
            });
            return Ok(crate::credentials::Secret::new(value.to_vec()));
        }
        let _guard = ACCESS.lock().map_err(|_| failure())?;
        match generic_password(PasswordOptions::new_generic_password(SERVICE, ACCOUNT)) {
            Ok(bytes) if bytes.len() == 32 => Ok(crate::credentials::Secret::new(bytes)),
            Err(e) if e.code() == -25300 && create => {
                let mut bytes = [0; 32];
                SystemRandom::new()
                    .fill(&mut bytes)
                    .map_err(|_| failure())?;
                let secret = crate::credentials::Secret::new(bytes.to_vec());
                bytes.fill(0);
                // Add-only: a simultaneous helper must never overwrite an existing master key.
                match SecKeychain::default()
                    .map_err(|e| keychain_failure("选择默认钥匙串", e))?
                    .add_generic_password(SERVICE, ACCOUNT, secret.as_bytes())
                {
                    Ok(()) => Ok(secret),
                    Err(e) if e.code() == -25299 => {
                        let existing = generic_password(PasswordOptions::new_generic_password(
                            SERVICE, ACCOUNT,
                        ))
                        .map_err(|e| keychain_failure("读取并发创建的主密钥", e))?;
                        if existing.len() != 32 {
                            return Err(failure());
                        }
                        Ok(crate::credentials::Secret::new(existing))
                    }
                    Err(e) => Err(keychain_failure("创建主密钥", e)),
                }
            }
            Err(e) => Err(keychain_failure("读取主密钥", e)),
            Ok(_) => Err(failure()),
        }
    }

    pub(super) fn seal(plain: &str) -> Result<String> {
        let master = key(true)?;
        let key =
            aead::UnboundKey::new(&aead::AES_256_GCM, master.as_bytes()).map_err(|_| failure())?;
        let mut nonce = [0; 12];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| failure())?;
        let mut body = plain.as_bytes().to_vec();
        aead::LessSafeKey::new(key)
            .seal_in_place_append_tag(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(AAD),
                &mut body,
            )
            .map_err(|_| failure())?;
        let mut envelope = nonce.to_vec();
        envelope.extend(body);
        Ok(format!("{KEYCHAIN}{}", hex::encode(envelope)))
    }

    pub(super) fn open(hexed: &str) -> Option<String> {
        let mut bytes = hex::decode(hexed).ok()?;
        if bytes.len() < 28 {
            return None;
        }
        let master = key(false).ok()?;
        let key = aead::UnboundKey::new(&aead::AES_256_GCM, master.as_bytes()).ok()?;
        let nonce: [u8; 12] = bytes[..12].try_into().ok()?;
        let plain = aead::LessSafeKey::new(key)
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(AAD),
                &mut bytes[12..],
            )
            .ok()?;
        String::from_utf8(plain.to_vec()).ok()
    }
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
    }

    #[test]
    fn undecryptable_ciphertext_returns_none_not_panic() {
        // 换机器 / 换 Windows 用户会走到这里。panic 意味着面板直接崩。
        assert_eq!(open("dpapi:not-hex-at-all"), None);
        assert_eq!(open("dpapi:00112233"), None);
        assert_eq!(open("keychain-aes-gcm-v1:00"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn authenticated_envelopes_reject_changes_and_use_unique_nonces() {
        let first = seal("test-secret").unwrap();
        let second = seal("test-secret").unwrap();
        assert_ne!(first, second);
        assert!(first.starts_with(KEYCHAIN));
        let mut damaged = first.into_bytes();
        let last = damaged.last_mut().unwrap();
        *last = if *last == b'0' { b'1' } else { b'0' };
        assert_eq!(open(std::str::from_utf8(&damaged).unwrap()), None);
        assert_eq!(open(&second).as_deref(), Some("test-secret"));
    }

    #[cfg(windows)]
    #[test]
    fn sealed_values_carry_the_marker() {
        let sealed = seal("sk-x").unwrap();
        assert!(sealed.starts_with(SEALED));
        assert!(sealed.starts_with("dpapi:"));
    }
}
