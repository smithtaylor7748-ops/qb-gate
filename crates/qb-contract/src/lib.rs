//! QB Gate 的 IPC 契约（L0.5）。
//!
//! 前后端共用的数据形状。Rust 这边是**唯一真相源**，TypeScript 那边由
//! `src-tauri/examples/export-types.rs` 用 ts-rs 导出来，
//! `npm run types:check` 比对两边是否一致。
//!
//! # 为什么它单独一层
//!
//! 契约要被所有层引用：平台层的 `repository` 拿它做表结构、领域层拿它做返回值、
//! 命令层拿它做 IPC 签名。所以它必须在最底下，而且**不能反过来引用任何人**。
//!
//! 反例就在本项目历史里：`domain::export_types` 顺手把 `diagnostics` /
//! `workspace` / `extensions` 三个业务模块的类型也导了，于是一堆纯数据类型
//! 反向依赖了三个业务模块，`workspace` 又依赖回 `repository`、`repository`
//! 又依赖 `domain` —— 11 个模块缠成一个强连通分量，怎么拆都拆不开。
//! 汇总名单属于开发工具那一层，不属于数据类型。
//!
//! 中转站的契约（站点、分组、健康度、路由）落地时也进这里，
//! 而不是回到各自的业务模块里定义。

pub mod channels;
pub mod domain;
pub mod error_kind;
pub mod filetime;
