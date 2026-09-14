//! IP 门禁（L2）。
//!
//! # 判定全项目只有一个函数
//!
//! `gate::judge`。看门狗、会话内 hook、手动放行三处共用。各判各的，
//! 漏掉某一维的那一处就是绕过入口。
//!
//! # 两个不许合并的判断
//!
//! `LaunchTarget::gated()`（起之前要不要验 IP）与 `stops_with_gate()`
//! （判不过要不要收掉）是**两个**问题。中转会话的答案分别是「要」和「不收」：
//! Deny ACE 按文件加、不认身份，让中转绕过解锁就是现成的绕过入口；
//! 而中转请求打第三方端点、不带官方 OAuth 身份，收它换不到任何保护，
//! 却会把正在写的对话弄丢。
//!
//! # 「查不到 IP」没有宽限期
//!
//! `watchdog::WatchMode::unknown_grace()` 两档都返回 `None`。
//! **这是使用者明确选的严格档，不是忘了写。**
//!
//! # 每一个完整可执行的副本都必须锁上
//!
//! 漏掉一个，那一个就是绕过入口 —— 包括版本库里留给回滚用的历史版本。
//! 唯一的例外是桌面端 `app-<版本>\claude.exe`（加 Deny 会让它开新窗口就崩），
//! 那一份靠看门狗收。

pub use qb_contract::domain;
pub use qb_foundation::{audit, error, paths};
pub use qb_install::install;
#[cfg(windows)]
pub use qb_platform::acl;
pub use qb_platform::{config_io, operations, repository, settings};
pub use qb_probe::probe;

pub mod gate;
