// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// Licensed under AGPL-3.0-only with the additional terms permitted by its section 7:
// see LICENSE and LICENSE-ADDITIONAL-TERMS.md at the repository root.
//! 环境体检（L2）。
//!
//! 查的是**系统层面会让 Claude 行为异常的那些设置**：残留的
//! `ANTHROPIC_*` 环境变量、代理变量、时区与出口国家对不上，等等。
//!
//! # 检测与如实报告，不替使用者伪装
//!
//! 面板可以告诉你「你的时区和出口对不上」，但**不改机器身份**。
//! 设备指纹伪装是明确不做的三类之一（见 `CLAUDE.md`）——
//! 两者的区别是：前者让使用者知情，后者替他伪装。

pub use qb_accounts::accounts;
// `paths` 是给 `sysenv::proxy` 用的：系统代理的原值必须落盘，
// 只存在内存里的话面板一关就再也回滚不了。qb-foundation 本来就是依赖，
// 这里只是把名字再导出一次。
pub use qb_foundation::{audit, error, paths};
pub use qb_platform::process;
pub use qb_probe::probe;

pub mod sysenv;
