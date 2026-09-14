//! 出口探测（L2）。
//!
//! 回答三个问题：**我的出口 IP 是什么、它落在哪个国家、它干不干净**，
//! 外加一轮 DNS 泄漏检测。
//!
//! # 它为什么是一片叶子
//!
//! 这一层只往外问、不往里改。门禁拿它的结论做判定（`qb-gate`），
//! 环境体检拿它做报告（`sysenv`），但它自己**不认识任何一个上层**。
//! 保持这一点，`gate ↔ probe` 那种环就长不出来。
//!
//! # 结论与处置分开
//!
//! `verdict` 只给「过 / 不过 / 查不到」，**不决定查不到时要不要收**。
//! 那是门禁的策略（见 `watchdog::WatchMode::unknown_grace`），
//! 混进来的话安全口径就会在两个地方各写一份。

pub use qb_foundation::{error, sink};
pub use qb_platform::process;

pub mod probe;
