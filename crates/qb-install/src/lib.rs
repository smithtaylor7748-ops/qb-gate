//! 安装（L2）。
//!
//! # 「Claude 装在哪」全项目只有一张表
//!
//! [`install::inventory`]。检测、启动、上锁、升级、残留清理、版本库全都从它拿，
//! **不许在别处再拼路径**。v0.8.0 之前四处各拼一份、已经对不上 ——
//! 只用 winget 装的人被锁在门外。
//!
//! # 这个 crate 里有两种东西
//!
//! | 性质 | 模块 | 谁会依赖它 |
//! |---|---|---|
//! | 事实：机器上有什么 | `inventory` · `detect` · `versions` | 门禁、账户、启动、关停 |
//! | 操作：装 / 升 / 换 | `managed` · `winget` · `upgrade` · `chrome` | 只有编排层 |
//!
//! 两种混在一个模块里的时候，`install` 被两层身份撕扯着 ——
//! `gate ↔ install` 那对环就是这么来的（门禁要事实，而安装要门禁去上锁）。
//! 现在**装机编排整段住在 `usecase::install_ops`**，这里只剩事实与单步操作，
//! 所以这个 crate 不认识门禁。

pub use qb_foundation::{audit, error, sink};
#[cfg(windows)]
pub use qb_platform::acl;
pub use qb_platform::{config_io, process, settings, signature};

pub mod install;
