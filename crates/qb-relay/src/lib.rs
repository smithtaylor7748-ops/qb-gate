// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// Licensed under AGPL-3.0-only with the additional terms permitted by its section 7:
// see LICENSE and LICENSE-ADDITIONAL-TERMS.md at the repository root.
//! 中转站（L2）。
//!
//! 供应商目录、11 家预设、协议与认证方式、Key 的 DPAPI 存取。
//!
//! 连通性诊断（`diagnostics`）**不在这里** —— 它要读工作区里那条环境记录
//! 才知道拿哪把 Key 去打，所以它属于编排层，留在上面。
//!
//! # 它为什么是一片叶子
//!
//! 中转站只跟「一个地址 + 一把 Key」打交道，不需要认识账户、门禁、装机。
//! 这也是中转站重做（P1–P4：站点池 / 测速测活 / 智能调度 / 站点检验）
//! 能独立推进的前提 —— 那些新东西全都落在这个 crate 里。
//!
//! # 三个结构体，不是一个
//!
//! Key 绝不能随响应回到前端。`ProviderView` 里**根本不存在**能装下 Key 的
//! 字段，所以那件事不是靠自觉，是编译期就做不到。细节见 `relay::store`。

pub use qb_foundation::{error, paths};
pub use qb_platform::{config_io, endpoint, secret};

pub mod relay;
