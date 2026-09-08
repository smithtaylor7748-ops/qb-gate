//! ip-gate.log。
//!
//! 只记时间、公网 IP、上锁/解锁动作。**不记任何对话内容**，
//! 这条约束沿用现有实现，不要往里加别的。

use std::io::Write;
use std::path::PathBuf;

pub fn path() -> PathBuf {
    crate::gate::state_dir().join("ip-gate.log")
}

pub fn write(line: &str) {
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let stamp = chrono::Local::now().format("%m-%d %H:%M:%S");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{stamp} {line}");
    }
}

pub fn tail(n: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path()) else {
        return Vec::new();
    };
    let all: Vec<&str> = text.lines().collect();
    all.iter().rev().take(n).rev().map(|s| s.to_string()).collect()
}
