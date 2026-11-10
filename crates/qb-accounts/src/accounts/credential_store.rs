//! Claude owns its OAuth storage. Read a managed slot; never migrate or overwrite Keychain items.
use qb_platform::credentials::Secret;
use std::path::Path;

/// The service format is verified against official CLI distributions, not a stable public API.
#[cfg(any(test, target_os = "macos"))]
fn service_for(dir: &str) -> String {
    use sha2::{Digest, Sha256};
    use unicode_normalization::UnicodeNormalization;
    let normalized: String = dir.nfc().collect();
    let digest = hex::encode(Sha256::digest(normalized.as_bytes()));
    format!("Claude Code-credentials-{}", &digest[..8])
}

#[cfg(any(test, target_os = "macos"))]
fn read_chunks(mut read: impl FnMut(&str) -> Option<Secret>) -> Option<Secret> {
    use base64::Engine;
    if let Some(value) = read("claude-code-user") {
        return Some(value);
    }
    let metadata = read("claude-code-user#m")?;
    let metadata: serde_json::Value = serde_json::from_slice(metadata.as_bytes()).ok()?;
    let count = metadata.get("n")?.as_u64()?;
    let length = metadata.get("l")?.as_u64()?;
    if !(1..=256).contains(&count) || length == 0 || length > count * 2400 {
        return None;
    }
    let mut encoded = Vec::with_capacity(length as usize);
    for index in 0..count {
        let chunk = read(&format!("claude-code-user#{index}"))?;
        if encoded.len() + chunk.len() > length as usize {
            return None;
        }
        encoded.extend_from_slice(chunk.as_bytes());
    }
    let encoded = Secret::new(encoded);
    if encoded.len() != length as usize {
        return None;
    }
    Some(Secret::new(
        base64::engine::general_purpose::STANDARD
            .decode(encoded.as_bytes())
            .ok()?,
    ))
}

pub fn read(dir: &Path) -> Option<Secret> {
    read_checked(dir).ok().flatten()
}

pub fn read_checked(dir: &Path) -> crate::error::Result<Option<Secret>> {
    #[cfg(target_os = "macos")]
    {
        // Hash the exact config string passed to Claude, without canonicalizing symlinks.
        // Never query the unscoped global service belonging to the user's default login.
        let roots = super::AccountRoots::current();
        let label = dir
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix(super::PREFIX));
        let test = qb_foundation::test_context::running_in_a_test_binary(
            std::env::current_exe().ok().as_deref(),
        );
        if !test
            && dir.parent() == Some(roots.panel.as_path())
            && label.is_some_and(|label| super::validate_existing_label(label).is_ok())
        {
            let service = service_for(&dir.to_string_lossy());
            let mut failed = None;
            let mut had_metadata = false;
            let value = read_chunks(|account| {
                match qb_platform::credentials::read_keychain(&service, account) {
                    Ok(value) => {
                        if account.ends_with("#m") && value.is_some() {
                            had_metadata = true;
                        }
                        value
                    }
                    Err(error) => {
                        failed = Some(error);
                        None
                    }
                }
            });
            if let Some(error) = failed {
                return Err(error);
            }
            if let Some(value) = value {
                return Ok(Some(value));
            }
            if had_metadata {
                return Err(crate::error::GateError::Other("钥匙串凭据分片不完整或格式已改变，未修改账户；请在官方 Claude Code 中核查登录。".into()));
            }
            // Older official CLI versions used the local username for this same scoped service.
            if let Ok(output) = std::process::Command::new("/usr/bin/id")
                .arg("-un")
                .output()
            {
                if output.status.success() {
                    if let Ok(account) = std::str::from_utf8(&output.stdout) {
                        if let Some(value) =
                            qb_platform::credentials::read_keychain(&service, account.trim())?
                        {
                            return Ok(Some(value));
                        }
                    }
                }
            }
        }
    }
    match std::fs::read(dir.join(".credentials.json")) {
        Ok(bytes) => Ok(Some(Secret::new(bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_exact_slot_path_has_a_scoped_service_and_unicode_matches_claude() {
        assert_ne!(
            service_for("/Users/a/slot-one"),
            service_for("/Users/a/slot-two")
        );
        assert_eq!(
            service_for("/Users/a/caf\u{e9}"),
            service_for("/Users/a/cafe\u{301}")
        );
        assert!(service_for("/Users/a/slot-one").starts_with("Claude Code-credentials-"));
        assert_ne!(
            service_for("/Users/a/slot-one"),
            service_for("/Users/a/slot-one/")
        );
    }
    #[test]
    fn keychain_chunks_require_complete_bounded_metadata_and_exact_length() {
        let values = std::collections::BTreeMap::from([
            ("claude-code-user#m", br#"{"n":2,"l":8}"#.to_vec()),
            ("claude-code-user#0", b"e30=".to_vec()),
            ("claude-code-user#1", b"".to_vec()),
        ]);
        assert!(read_chunks(|key| values.get(key).cloned().map(Secret::new)).is_none());
        let value = read_chunks(|key| match key {
            "claude-code-user#m" => Some(Secret::new(br#"{"n":1,"l":4}"#.to_vec())),
            "claude-code-user#0" => Some(Secret::new(b"e30=".to_vec())),
            _ => None,
        })
        .unwrap();
        assert_eq!(value.as_bytes(), b"{}");
    }
}
