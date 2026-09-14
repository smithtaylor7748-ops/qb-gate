//! 面板起来了没有。**平台层：只依赖 `error`。**
//!
//! # 为什么从 `startup` 里搬出来
//!
//! `operations` 在每次拿互斥锁之前都要问一句「启动恢复完成了吗」——
//! 没完成就不许动任何东西，否则会在一个半初始化的状态上写盘。
//!
//! 但那个状态原来住在 `startup` 里，而 `startup` 反过来又要用 `operations`
//! 的互斥锁和变更广播 —— `operations ↔ startup` 这对环就是这么来的。
//!
//! 拆法很直接：**状态本身不属于「启动流程」，它属于「整个进程的就绪态」**。
//! 搬到这里之后，`startup` 负责写，`operations` 负责读，两边都只向下依赖。
//!
//! # 这两个静态量为什么是全局的
//!
//! 它们在 Tauri 的 `AppState` 建起来**之前**就要能写 —— 启动恢复失败时，
//! `.setup()` 会提前 return，那时候还没有 State 可用。这是少数几个
//! 真正需要进程级静态的地方，不是图省事。

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::error::{GateError, Result};

static FAILURE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
/// 结不清的恢复记录。只有启动恢复写，只有「放弃这几条」那个命令读。
static BLOCKED: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();

#[derive(Clone, serde::Serialize)]
pub struct Status {
    pub ready: bool,
    pub error: Option<String>,
    /// 结不清的恢复记录，原样给使用者看。
    ///
    /// 非空时恢复页要多给一个「放弃这几条记录」的按钮 —— 这是唯一一条
    /// 使用者能靠自己走出恢复页的路。没有它，他只能去
    /// `%LOCALAPPDATA%` 里猜该删哪个文件。
    pub blocked: Vec<String>,
}

pub fn blocked_paths() -> Vec<PathBuf> {
    BLOCKED
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .clone()
}

pub fn status() -> Status {
    let error = FAILURE
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .clone();
    Status {
        ready: error.is_none(),
        blocked: blocked_paths()
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        error,
    }
}

/// 没就绪就别动任何东西。
///
/// `operations` 的三个入口都先过这一关 —— 在一个半初始化的状态上写盘，
/// 比什么都不做糟得多。
pub fn ensure_ready() -> Result<()> {
    match status().error {
        Some(e) => Err(GateError::Other(format!("启动恢复尚未完成：{e}"))),
        None => Ok(()),
    }
}

pub fn set_failure(error: Option<String>) {
    *FAILURE.get_or_init(Default::default).lock().unwrap() = error;
}

pub fn set_blocked(paths: Vec<PathBuf>) {
    *BLOCKED.get_or_init(Default::default).lock().unwrap() = paths;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_process_is_ready_and_has_nothing_blocked() {
        // 默认必须是「就绪」。反过来（默认不就绪、等 initialize 置位）的话，
        // 任何在 initialize 之前跑的东西都会被挡下来，而症状是面板起不来。
        assert!(ensure_ready().is_ok());
        assert!(status().ready);
        assert!(status().blocked.is_empty());
    }
}
