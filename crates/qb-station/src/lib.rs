// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// Licensed under AGPL-3.0-only with the additional terms permitted by its section 7:
// see LICENSE and LICENSE-ADDITIONAL-TERMS.md at the repository root.
//! 中转站健康度（L2）。
//!
//! | 模块 | 干什么 |
//! |---|---|
//! | [`station`] | 线路模型：站点 · 分组 · 令牌，以及倍率与余额 |
//! | [`health`] | 把账单日志按 1H / 1D / 7D 三个窗口聚合成可比较的指标 |
//! | [`sse`] | 把一条 SSE 流的结局判成成功 / 失败 / 容量 / 限流 / 断流（纯函数） |
//! | [`turnstate`] | Codex `X-Codex-Turn-State` 信封的外形解析与 active/ready 状态机（纯函数） |
//!
//! # 为什么指标是「免费」的
//!
//! 缓存命中率、首字 P50/P95、成功率、24 小时实扣，全部从站点自己的
//! `/api/log/self` 账单日志算出来 —— **那是真实流量，不是探针**，
//! 所以自动刷新可以一直开着。要花钱的只有两条：打真模型测速、查套路。
//! 那两条必须手动点。
//!
//! # 时间窗为什么要能注入时钟
//!
//! 「这条记录落在哪个窗里」的边界条件用真实时钟根本测不稳 ——
//! 测试跑得快一点慢一点结论就变了。用 `sink::Clock`。

pub use qb_contract::domain;
pub use qb_foundation::{error, sink};

pub mod health;
pub mod router;
pub mod schedule;
pub mod sse;
pub mod station;
pub mod turnstate;
