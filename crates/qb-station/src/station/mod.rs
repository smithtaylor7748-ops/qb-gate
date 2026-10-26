//! 中转站域。
//!
//! 目前只有纯数据类型（`model`）。适配器、协议探测、面板登录会在
//! P1 阶段接进来；本机路由与查套路在 P2 / P4。
//!
//! **这个模块曾经是孤儿**：`model.rs` 写出来了却没有 `mod.rs`、也不在 `lib.rs`
//! 的 `pub mod` 清单里，于是 187 行代码不编译、不进二进制、里面的
//! `ts_rs` 类型从没导出过。看起来像完成的功能，实际是死的 —— 别再让它退回去。

pub mod audit;
pub mod billing;
/// 站点的备用地址与测速（中转站重构阶段 4，多端点）。
pub mod endpoints;
pub mod model;
pub mod pricing;
pub mod route;
/// 站点的余额与编程套餐额度（中转站重构阶段 6 · U3）：纯解析，发请求在 `qb-app`。
pub mod site_quota;
