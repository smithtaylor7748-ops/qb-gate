//! Read only Claude's locale setting; never infer the live renderer's language.
use super::checkup::{CheckItem, State};
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_BYTES: u64 = 256 * 1024;

#[derive(Deserialize)]
struct Config {
    locale: Option<String>,
}

fn normalized_locale(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.len() > 48 {
        return None;
    }
    let mut parts = value.split('-');
    let language = parts.next()?;
    if !(2..=3).contains(&language.len()) || !language.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    if !parts.all(|p| (2..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric())) {
        return None;
    }
    Some(value.to_ascii_lowercase())
}

fn read_locale(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_BYTES {
        return None;
    }
    let mut raw = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut raw).ok()?;
    if raw.len() > MAX_BYTES as usize {
        return None;
    }
    let config: Config = serde_json::from_slice(&raw).ok()?;
    normalized_locale(config.locale.as_deref()?)
}

fn result(index: usize, source: &str, value: Option<String>, expected: Option<&str>) -> CheckItem {
    let (state, detail) = match value {
        None => (State::Unknown, format!("{source}：语言未设置、配置不可读或格式不受支持；不能确定运行中的语言。")),
        Some(locale) => match expected {
            Some(wanted) if locale != wanted => (State::Warn, format!("{source}：locale={locale}，与你指定的 {wanted} 不同。语言偏好不代表地区限制或账号风险。")),
            Some(wanted) => (State::Pass, format!("{source}：locale={locale}，与指定的 {wanted} 一致。只核对配置，未验证运行中的 navigator.language。")),
            None => (State::Pass, format!("{source}：locale={locale}。已读取配置；未指定比较目标，未验证运行中的 navigator.language。")),
        },
    };
    CheckItem {
        id: format!("claude_locale_{index}"),
        label: source.into(),
        state,
        detail,
        fixable: false,
        manual: None,
    }
}

/// Callers inject roots in tests. Multiple installations are shown independently;
/// an old roaming config must not silently override a newer Store installation.
pub fn inspect_at(
    config_root: Option<&Path>,
    local_root: Option<&Path>,
    expected: Option<&str>,
) -> crate::error::Result<Vec<CheckItem>> {
    let expected = expected
        .filter(|s| !s.trim().is_empty())
        .map(|raw| {
            normalized_locale(raw).ok_or_else(|| {
                crate::error::GateError::Other(
                    "期望语言请填写语言标签，例如 en-US、ja-JP 或 zh-TW。".into(),
                )
            })
        })
        .transpose()?;
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();
    if let Some(root) = config_root {
        let path = root.join("Claude").join("config.json");
        if path.exists() {
            candidates.push(("Claude 桌面配置".into(), path));
        }
    }
    if let Some(root) = local_root {
        if let Ok(entries) = std::fs::read_dir(root.join("Claude-Data")) {
            let mut entries: Vec<_> = entries
                .filter_map(std::result::Result::ok)
                .take(64)
                .collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    let path = entry.path().join("config.json");
                    if path.exists() {
                        candidates
                            .push((format!("Claude 商店版配置 {}", candidates.len() + 1), path));
                    }
                }
            }
        }
    }
    if candidates.is_empty() {
        return Ok(vec![result(
            0,
            "Claude 客户端语言",
            None,
            expected.as_deref(),
        )]);
    }
    let mut rows: Vec<_> = candidates
        .iter()
        .enumerate()
        .map(|(i, (source, path))| result(i, source, read_locale(path), expected.as_deref()))
        .collect();
    if rows.len() > 1 {
        for row in &mut rows {
            row.detail
                .push_str(" 检出多份配置，无法仅凭文件确定当前运行实例使用哪份。");
        }
    }
    Ok(rows)
}

pub fn inspect(expected: Option<&str>) -> crate::error::Result<Vec<CheckItem>> {
    // Same platform root as AccountRoots::current and install::inventory:
    // macOS ~/Library/Application Support, Windows %APPDATA%.
    // Do not substitute ~/.config on macOS or read preferences/Keychain instead.
    let config = dirs::config_dir();
    let local = if cfg!(windows) {
        dirs::data_local_dir()
    } else {
        None
    };
    inspect_at(config.as_deref(), local.as_deref(), expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_application_support_uses_only_the_desktop_locale_fixture() {
        let root = std::env::temp_dir().join(format!(
            "qb-mac-locale-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let support = root.join("Library").join("Application Support");
        let desktop = support.join("Claude");
        let wrong = root.join(".config").join("Claude");
        std::fs::create_dir_all(&desktop).unwrap();
        std::fs::create_dir_all(&wrong).unwrap();
        std::fs::write(
            desktop.join("config.json"),
            r#"{"locale":"ja-JP","unrelated":"fixture-only-private-field"}"#,
        )
        .unwrap();
        std::fs::write(wrong.join("config.json"), r#"{"locale":"en-US"}"#).unwrap();
        let rows = inspect_at(Some(&support), None, Some("ja-JP")).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, State::Pass);
        assert!(rows[0].detail.contains("ja-jp"));
        assert!(!rows[0].detail.contains("en-us"));
        let serialized = serde_json::to_string(&rows).unwrap();
        assert!(!serialized.contains("fixture-only-private-field"));
        assert!(!serialized.contains(root.to_str().unwrap()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn language_tags_are_validated_without_assuming_english() {
        assert_eq!(
            normalized_locale("zh-Hant-TW").as_deref(),
            Some("zh-hant-tw")
        );
        assert_eq!(normalized_locale("ja-JP").as_deref(), Some("ja-jp"));
        for input in ["en-US\nsecret", "english", "en--US", "en/US"] {
            assert_eq!(normalized_locale(input), None);
        }
        assert!(inspect_at(None, None, Some("../config")).is_err());
    }

    #[test]
    fn incomplete_settings_stay_unknown_and_language_mismatch_is_only_a_warning() {
        assert_eq!(result(0, "test", None, None).state, State::Unknown);
        assert_eq!(
            result(0, "test", Some("ja-jp".into()), Some("en-us")).state,
            State::Warn
        );
        assert_eq!(
            result(0, "test", Some("ja-jp".into()), Some("ja-jp")).state,
            State::Pass
        );
    }

    #[test]
    fn reads_isolated_store_and_roaming_settings_without_returning_other_fields() {
        let root = std::env::temp_dir().join(format!(
            "qb-locale-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let roaming = root.join("roaming");
        let local = root.join("local");
        std::fs::create_dir_all(roaming.join("Claude")).unwrap();
        std::fs::create_dir_all(local.join("Claude-Data").join("test-package")).unwrap();
        std::fs::write(
            roaming.join("Claude/config.json"),
            r#"{"locale":"en-US","token":"fixture-private"}"#,
        )
        .unwrap();
        std::fs::write(
            local.join("Claude-Data/test-package/config.json"),
            r#"{"locale":"ja-JP"}"#,
        )
        .unwrap();
        let rows = inspect_at(Some(&roaming), Some(&local), None).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].detail.contains("en-us"));
        assert!(rows[1].detail.contains("ja-jp"));
        assert!(!serde_json::to_string(&rows)
            .unwrap()
            .contains("fixture-private"));
        std::fs::write(roaming.join("Claude/config.json"), "broken").unwrap();
        assert_eq!(
            inspect_at(Some(&roaming), None, None).unwrap()[0].state,
            State::Unknown
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
