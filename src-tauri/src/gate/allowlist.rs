//! 白名单文件读写。
//!
//! 兼容 UTF-8 BOM —— 现有 allowlist.txt 开头就是 `EF BB BF`，
//! PowerShell 的 Get-Content 会自动剥掉，Rust 不会，得自己处理。

use crate::error::Result;
use std::path::{Path, PathBuf};

pub fn path() -> PathBuf {
    crate::gate::state_dir().join("allowlist.txt")
}

pub fn read() -> Result<Vec<String>> {
    read_from(&path())
}

pub fn read_from(p: &Path) -> Result<Vec<String>> {
    if !p.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read(p)?;
    let text = String::from_utf8_lossy(strip_bom(&raw));
    Ok(parse(&text))
}

fn strip_bom(b: &[u8]) -> &[u8] {
    b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b)
}

fn parse(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

pub fn write(entries: &[String]) -> Result<()> {
    let p = path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let body = format!(
        "# ClaudeGate 白名单 —— 一行一个出口 IP，# 开头为注释\n{}\n",
        entries.join("\n")
    );
    // 原子写：先写临时文件再改名，避免看门狗读到半截。
    let tmp = p.with_extension("txt.tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

pub fn contains(entries: &[String], ip: &str) -> bool {
    entries.iter().any(|e| e == ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_utf8_bom() {
        let raw = b"\xEF\xBB\xBF203.0.113.7\n";
        assert_eq!(strip_bom(raw), b"203.0.113.7\n");
    }

    #[test]
    fn parses_ignoring_comments_and_blanks() {
        let got = parse("# 注释\n\n203.0.113.7\n  203.0.113.9  \n");
        assert_eq!(got, vec!["203.0.113.7", "203.0.113.9"]);
    }

    #[test]
    fn empty_allowlist_matches_nothing() {
        assert!(!contains(&[], "203.0.113.7"));
    }
}
