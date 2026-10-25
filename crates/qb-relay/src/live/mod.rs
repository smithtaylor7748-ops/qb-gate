// Portions derived from cc-switch v4.0.14 (https://github.com/farion1231/cc-switch, commit 5de6feb4e5adda61f4ad824ce0df2c449245f447), Copyright (c) 2025 Jason Young, MIT License — see ATTRIBUTION.md.
//! 配置写入引擎（中转站重构阶段 4，cc-switch 的 P2，`src-tauri/src/live/`）：只改客户端配置里点名的键，其余字节不碰。
//!
//! - [`patch`]：按格式保序改写（JSON、TOML、`.env`），**解析不了就停，绝不退回空文档**；
//! - [`floor`]：每个客户端的关键字段（请求发到哪、凭什么鉴权、哪个模型、哪种协议）的谓词和清单；
//! - [`engine`]：写锁与计划（读字节 → 在内存里算出新内容），写盘交给 `config_io::commit`。
//! - [`jsonc_document`]：带注释的 JSONC / JSON5 按原样改写（cc-switch 根目录的同名文件，共存型工具用）。
//!
//! # 跟 cc-switch 不一样的地方
//!
//! - 写盘不用它的临时文件 + 首写备份，用 QB Gate 自己的两阶段提交（[`engine`] 顶上写了为什么）。
//! - `residue.rs`（清 cc-switch 旧版自己写过的残留值）不搬：QB Gate 从没写过那些值。
//! - `project/*`（把 cc-switch 的供应商行投影成关键字段）不整份搬：QB Gate 没有那种供应商行，
//!   各客户端的写法在 `crate::relay` 与各客户端自己的模块里，按需取用它的规则。
//! - ⛔ **只写中转环境目录（`qb-router-*`）和新客户端自己的配置，不碰官方槽位**（交接文档阶段 4 第 1 条）。

pub mod engine;
pub mod floor;
pub mod jsonc_document;
pub mod patch;
