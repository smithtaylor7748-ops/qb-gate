// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// Licensed under AGPL-3.0-only with the additional terms permitted by its section 7:
// see LICENSE and LICENSE-ADDITIONAL-TERMS.md at the repository root.
//! 扩展中心（L2）。
//!
//! | 模块 | 干什么 |
//! |---|---|
//! | [`extensions`] | 目录、安装预览、落盘校验、卸载 |
//! | [`plugins`] | 本机应用的接入与桥接（目前只有 SillyTavern） |
//!
//! # 为什么这两个在一起
//!
//! 「应用」是扩展的一种（`ExtensionKind::Application`），而接入 / 解除接入
//! 要问应用当前跑没跑 —— `extensions` 与 `plugins` 之间本来就有来回。
//! 放进同一个 crate，这种来回是模块内的事；分开的话它就是一条跨 crate 的
//! 横向依赖，而且很容易长成环。
//!
//! # 加第五种扩展类型时
//!
//! `extensions` 里还有 `if kind == Skill { … } else { MCP 分支 }` 这种写法，
//! 加一种新类型会被**静默当成 MCP 安装**。改成穷尽 `match` 是 C1 的活。

pub use qb_accounts::{accounts, residue};
pub use qb_contract::domain;
pub use qb_foundation::{audit, error, paths, sink};
pub use qb_install::install;
pub use qb_platform::{config_io, endpoint, process, repository, secret, sessions};

pub mod extensions;
pub mod plugins;
