// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// Licensed under AGPL-3.0-only with the additional terms permitted by its section 7:
// see LICENSE and LICENSE-ADDITIONAL-TERMS.md at the repository root.
//! 启动与关停（L2）。
//!
//! | 模块 | 干什么 |
//! |---|---|
//! | [`launch`] | 解析要起哪一个 exe、带什么环境变量 |
//! | [`killswitch`] | 一键关闭：预览要收哪些，再执行 |
//!
//! # 为什么这两件事在一起
//!
//! 它们共用同一张「哪些进程算数」的判断表，而且共用同一条不变量：
//! **按 PID 杀进程时要放过中转会话**（`gate::stop_managed` 里那句
//! `if all_sessions { execute() } else { execute_official() }`）。
//! 分在两个 crate 里的话，这条不变量就有两个地方可以各写一半。

pub use qb_contract::domain;
pub use qb_foundation::{audit, error};
pub use qb_install::install;
pub use qb_iplock::gate;
pub use qb_platform::{process, sessions, settings};

pub mod killswitch;
pub mod launch;
