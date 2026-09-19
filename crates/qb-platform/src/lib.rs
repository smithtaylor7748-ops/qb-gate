// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// Licensed under AGPL-3.0-only with the additional terms permitted by its section 7:
// see LICENSE and LICENSE-ADDITIONAL-TERMS.md at the repository root.
//! QB Gate 的平台层（L1）。
//!
//! # 这一层装什么
//!
//! 跟 Windows 和磁盘打交道的**机制**，不含任何「什么时候该这么做」的判断：
//!
//! | 模块 | 机制 |
//! |---|---|
//! | [`acl`] | 给文件加/摘 Deny ExecuteFile |
//! | [`firewall`] | 加/摘一条按 exe 限定的出站阻止规则（0.19.0） |
//! | [`secret`] | DPAPI 封/解一串字节 |
//! | [`signature`] | 问一个 exe 是谁签的 |
//! | [`process`] | 隐藏窗口地起子进程、Job 对象 |
//! | [`config_io`] | 多文件 + SQLite 的两阶段提交与崩溃恢复 |
//! | [`endpoint`] | 把一个用户随手粘的地址规整成 base URL |
//! | [`readiness`] | 进程就绪态（启动恢复完成没、有没有被挡住） |
//! | [`logging`] | `tracing` 装配与按天滚动 |
//! | [`panic_hook`] | 崩溃现场落盘 |
//!
//! 判断归领域层。举个分界线：**「怎么上锁」在这里，「出口 IP 不合格所以要上锁」
//! 在 `qb-gate`。** 混进来一条判断，这一层就会重新长成 `gate` 当年那个
//! 「所有人都依赖、又反过来调所有人」的样子。
//!
//! # 为什么再导出地基层
//!
//! 见下面 `pub use`：`crate::error::Result` 这个写法在全项目出现几百次，
//! 拆 crate 这件事本身不该把它们全改一遍。每个 crate 根部再导出一次，
//! 写法就在所有 crate 里保持一致。

// 地基层就在 `qb-foundation` 里。再导出一次，让 `crate::error::Result`、
// `crate::paths::state_dir()`、`crate::audit::write()` 这些写法在本 crate 内
// 仍然成立 —— 搬家的 diff 因此只剩 Cargo.toml 和这个文件。
pub use qb_contract::domain;
pub use qb_foundation::{audit, error, paths, sink};

#[cfg(windows)]
pub mod acl;
pub mod config_io;
pub mod endpoint;
pub mod firewall;
pub mod legacy;
pub mod logging;
pub mod operations;
pub mod panic_hook;
pub mod process;
pub mod progress;
pub mod readiness;
pub mod repository;
pub mod secret;
pub mod sessions;
pub mod settings;
pub mod signature;
