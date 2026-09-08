//! Codex 中转站配置。
//!
//! 能力对标 cc-switch（MIT，https://github.com/farion1231/cc-switch）：
//! 多供应商、一键切换、写进 Codex 自己的配置文件。这里只做 Codex 需要的两个：
//!   `~/.codex/config.toml`  模型供应商与 base_url
//!   `~/.codex/auth.json`    API Key
//!
//! **API Key 不落明文到本项目自己的配置里。** 写进 Codex 的 auth.json 是 Codex
//! 本身的要求；面板界面上只显示掩码，读回来时也不把完整 Key 回传前端。

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub model: Option<String>,
    /// 只在写入方向使用。`skip_serializing` 保证它不会随响应回到前端。
    #[serde(default, skip_serializing)]
    pub api_key: Option<String>,
}

pub fn codex_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".codex")
}

fn atomic_write(p: &std::path::Path, body: &str) -> Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    // 先备份再写，坏了能退回去。
    if p.exists() {
        let bak = p.with_file_name(format!(
            "{}.bak",
            p.file_name().and_then(|n| n.to_str()).unwrap_or("config")
        ));
        let _ = std::fs::copy(p, bak);
    }
    // 原子写：临时文件 + 改名，避免 Codex 读到半截配置。
    let tmp = p.with_file_name(format!(
        "{}.tmp",
        p.file_name().and_then(|n| n.to_str()).unwrap_or("config")
    ));
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, p)?;
    Ok(())
}

/// 写 config.toml。
///
/// 用 toml_edit 增量改而不是整个重新序列化，保住用户自己加的其他配置项 ——
/// 覆盖式写入会把人家手调的参数悄悄抹掉。
pub fn apply_provider(p: &Provider) -> Result<()> {
    let cfg_path = codex_dir().join("config.toml");
    let existing = std::fs::read_to_string(&cfg_path).unwrap_or_default();
    let mut doc = existing
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_default();

    doc["model_provider"] = toml_edit::value(p.id.as_str());
    if let Some(m) = &p.model {
        doc["model"] = toml_edit::value(m.as_str());
    }
    let id = p.id.as_str();
    doc["model_providers"][id]["name"] = toml_edit::value(p.name.as_str());
    doc["model_providers"][id]["base_url"] = toml_edit::value(p.base_url.as_str());
    doc["model_providers"][id]["wire_api"] = toml_edit::value("chat");
    doc["model_providers"][id]["env_key"] = toml_edit::value("OPENAI_API_KEY");

    atomic_write(&cfg_path, &doc.to_string())?;

    if let Some(key) = &p.api_key {
        let auth = serde_json::json!({ "OPENAI_API_KEY": key });
        atomic_write(
            &codex_dir().join("auth.json"),
            &serde_json::to_string_pretty(&auth)?,
        )?;
    }
    crate::gate::log::write(&format!("Codex 中转站切换为 {}", p.name));
    Ok(())
}

/// 读回当前配置，**不含 Key**。
pub fn current_provider() -> Option<Provider> {
    let text = std::fs::read_to_string(codex_dir().join("config.toml")).ok()?;
    let doc = text.parse::<toml_edit::DocumentMut>().ok()?;
    let id = doc.get("model_provider")?.as_str()?.to_string();
    let node = doc.get("model_providers")?.get(&id)?;
    Some(Provider {
        name: node
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string(),
        base_url: node
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        model: doc.get("model").and_then(|v| v.as_str()).map(String::from),
        api_key: None,
        id,
    })
}

/// 掩码，给界面显示用。
pub fn mask(key: &str) -> String {
    let n = key.chars().count();
    if n <= 8 {
        return "•".repeat(n);
    }
    let head: String = key.chars().take(4).collect();
    let tail: String = key.chars().skip(n - 4).collect();
    format!("{head}{}{tail}", "•".repeat(n.saturating_sub(8).min(16)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_long_key_without_leaking_middle() {
        let m = mask("sk-abcdefghijklmnopqrstuvwxyz");
        assert!(m.starts_with("sk-a"));
        assert!(m.ends_with("wxyz"));
        assert!(!m.contains("efghij"));
    }

    #[test]
    fn masks_short_key_entirely() {
        assert_eq!(mask("abcd"), "••••");
        assert_eq!(mask(""), "");
    }

    #[test]
    fn provider_never_serializes_api_key() {
        let p = Provider {
            id: "x".into(),
            name: "X".into(),
            base_url: "https://x".into(),
            model: None,
            api_key: Some("sk-secret-value".into()),
        };
        let j = serde_json::to_string(&p).unwrap();
        assert!(!j.contains("sk-secret-value"));
        assert!(!j.contains("api_key"));
    }
}
