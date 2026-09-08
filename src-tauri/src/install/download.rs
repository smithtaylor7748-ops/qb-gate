//! 安装包下载与校验。
//!
//! 仓库不分发官方安装器本体，只存地址与 SHA-256。运行时拉取后必须校验，
//! **不匹配一律拒绝安装**，不给「继续」的选项。

use crate::error::{GateError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installer {
    pub id: String,
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub installers: Vec<Installer>,
}

pub fn cache_dir() -> PathBuf {
    crate::gate::state_dir().join("installers-cache")
}

pub fn load_lockfile(app_dir: &std::path::Path) -> Result<Lockfile> {
    let p = app_dir.join("installers.lock.json");
    let text = std::fs::read_to_string(&p)
        .map_err(|_| GateError::NotFound(p.display().to_string()))?;
    Ok(serde_json::from_str(&text)?)
}

fn sha256_file(p: &std::path::Path) -> Result<String> {
    let bytes = std::fs::read(p)?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(hex::encode(h.finalize()))
}

/// 下载并校验。返回落盘路径。校验失败会**删掉**下载的文件再报错，
/// 免得留一个坏包在缓存里下次被当成好的。
pub async fn fetch_verified(inst: &Installer) -> Result<PathBuf> {
    // 哈希没钉住就不给装。这里**故意**不提供「跳过校验继续」的选项 ——
    // 一个能被绕过的校验比没有校验更危险，它会让人以为验过了。
    if inst.sha256.trim().is_empty() {
        return Err(GateError::Other(format!(
            "{} 尚未钉定 SHA-256，拒绝下载安装。\
             请先手动核对官方安装包，再用 scripts/pin-hash.mjs 写回 installers.lock.json。",
            inst.name
        )));
    }

    let dir = cache_dir();
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(&inst.filename);

    if dest.exists() {
        if sha256_file(&dest)?.eq_ignore_ascii_case(&inst.sha256) {
            return Ok(dest);
        }
        let _ = std::fs::remove_file(&dest);
    }

    let c = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()?;
    let bytes = c.get(&inst.url).send().await?.error_for_status()?.bytes().await?;
    std::fs::write(&dest, &bytes)?;

    let got = sha256_file(&dest)?;
    if !got.eq_ignore_ascii_case(&inst.sha256) {
        let _ = std::fs::remove_file(&dest);
        return Err(GateError::Other(format!(
            "{} 校验失败：期望 {}，实际 {}。已删除下载文件，拒绝安装。",
            inst.name, inst.sha256, got
        )));
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_comparison_is_case_insensitive() {
        let a = "ABCDEF0123";
        let b = "abcdef0123";
        assert!(a.eq_ignore_ascii_case(b));
    }

    #[test]
    fn lockfile_parses() {
        let j = r#"{"installers":[{"id":"cc","name":"Claude Code","url":"https://x",
                     "sha256":"aa","filename":"c.exe"}]}"#;
        let lf: Lockfile = serde_json::from_str(j).unwrap();
        assert_eq!(lf.installers.len(), 1);
        assert_eq!(lf.installers[0].id, "cc");
    }
}
