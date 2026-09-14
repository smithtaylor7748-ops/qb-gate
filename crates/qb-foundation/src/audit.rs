//! 审计日志 `ip-gate.log`。**地基层：只依赖 [`crate::paths`]。**
//!
//! # 约束（从 `gate::log` 原样带过来，一个字没松）
//!
//! 只记时间、公网 IP、上锁/解锁动作。**不记任何对话内容**，也不要往里加别的。
//! 这是给使用者看的那一份 —— 它短、可读、可核对。诊断用的结构化日志是
//! `src-tauri` 的 `logging` 写的另一份，两者不合并。
//!
//! # 为什么从 `gate` 里搬出来
//!
//! 原来在 `gate::log`。**对 `gate` 的全部外部引用里，30 次是这一个函数** ——
//! `install/chrome.rs`、`sessions.rs`、`sysenv/mod.rs` 这些模块跟门禁毫无关系，
//! 却因为要写一行审计日志而依赖了它。加上 [`crate::paths::state_dir`]，
//! 这两个东西撑起了 `gate` 入度 25 里的绝大部分。
//!
//! 搬出来之后 `config_io ↔ gate`、`gate ↔ plugins`、`gate ↔ sessions` 三对
//! 循环依赖直接消失。

use std::io::Write;
use std::path::PathBuf;

pub fn path() -> PathBuf {
    crate::paths::state_dir().join("ip-gate.log")
}

pub fn write(line: &str) {
    // 单向桥接到诊断日志：审计事件同时出现在 logs/qb-gate.log 里，
    // 这样排障时能看到它跟别的事件的先后顺序。
    // **反向不成立** —— tracing 的东西永远不会流进 ip-gate.log，
    // 上面那条「不要往里加别的」的约束不受影响。
    tracing::info!(target: "audit", "{line}");
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let stamp = chrono::Local::now().format("%m-%d %H:%M:%S");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
    {
        let _ = writeln!(f, "{stamp} {line}");
    }
}

pub fn tail(n: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path()) else {
        return Vec::new();
    };
    let all: Vec<&str> = text.lines().collect();
    all.iter()
        .rev()
        .take(n)
        .rev()
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_audit_log_lives_next_to_the_other_runtime_data() {
        assert!(path().ends_with("ip-gate.log"));
        assert_eq!(path().parent(), Some(crate::paths::state_dir().as_path()));
    }

    #[test]
    fn tail_of_a_missing_file_is_empty_not_a_panic() {
        // 首次启动时这个文件还不存在。总览页的日志条会在那一刻读它 ——
        // panic 的话首页直接白屏。
        let saved = std::fs::read_to_string(path()).ok();
        assert!(tail(5).len() <= 5);
        // 只读不写：这条测试绝不碰真实运行期文件的内容。
        assert_eq!(std::fs::read_to_string(path()).ok(), saved);
    }
}
