// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// Licensed under AGPL-3.0-only with the additional terms permitted by its section 7:
// see LICENSE and LICENSE-ADDITIONAL-TERMS.md at the repository root.
//! 中转（L2）：官方直连端点预设、协议与凭证形态、把一条中转写进工具配置的纯函数。
//!
//! 线路池、测速测活、智能调度、站点检验都在 `qb-station`；站点与 Key 存在工作区数据库
//! （`qb_platform::repository`），不在这里。
//!
//! 2026-09-27 之前这里还装着数据库之前那一代的中转站目录（`relay.json` 的增删改查、
//! 「从当前配置导入」、拉模型与测延迟、按响应头判断真实后端）。迁移之后它们一个调用者都没有了，
//! 连同「服务商与环境」旧页一起删掉。
//!
//! # 它为什么是一片叶子
//!
//! 这里只跟「一个地址 + 一种协议 + 一种凭证形态」打交道，不需要认识账户、门禁、装机。
//!
//! 2026-10-25 起（中转站重构阶段 4）多了 [`live`]：照搬 cc-switch 的配置写入引擎（保序补丁、关键字段），
//! [`relay`] 里写中转环境的那几个函数改在它上面写。

pub use qb_foundation::{error, paths};
pub use qb_platform::{config_io, endpoint, secret};

pub mod live;
pub mod relay;
