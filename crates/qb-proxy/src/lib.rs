// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// Licensed under AGPL-3.0-only with the additional terms permitted by its section 7:
// see LICENSE and LICENSE-ADDITIONAL-TERMS.md at the repository root.
//! 本机路由的纯逻辑层（第 3 层）。
//!
//! 一发请求在本机路由里要回答的问题，**凡是不用联网就答得出的**都在这里：
//!
//! | 模块 | 回答的问题 |
//! |---|---|
//! | [`proxy_error`] | 这一发出了什么错、回给客户端什么 |
//! | [`classify`] | 这个错要不要换下一条线、算不算线路的故障、记进熔断的哪一档 |
//! | [`envelope`] | HTTP 200 里是不是藏着一个错误（还在重试循环里时认出来，才换得了线） |
//! | [`model_mapper`] | 这条线上该把模型名换成什么 |
//! | [`thinking_rectifier`] · [`thinking_budget_rectifier`] · [`media_sanitizer`] · [`tool_media`] · [`opaque_state_rectifier`] | 上游因为请求形状拒绝时，怎么改一下同一条线再试一次 |
//! | [`thinking_optimizer`] · [`cache_injector`] | Bedrock 线发送前的优化（默认关） |
//! | [`body_filter`] | 哪些私有字段不该转给上游 |
//! | [`usage`] | 响应里用了多少 token、按价目算多少钱 |
//! | [`session`] | 这一发属于哪段对话 |
//! | [`sse_text`] | SSE 流怎么切块、跨块的 UTF-8 怎么拼 |
//! | [`convert`] | 客户端跟线路上游说的协议不一样时，请求和响应怎么换（阶段 3） |
//! | [`aggregate`] | 聚合模型的 id 怎么编、怎么解、解不出来回什么（阶段 3） |
//!
//! # 来源
//!
//! 除 `classify` / `envelope` 的拼装以外，多数文件搬自 cc-switch v4.0.14
//! （`src-tauri/src/proxy/`，MIT，提交 `5de6feb4`），文件头各有一行出处；改了什么写在
//! 各文件头和 `ATTRIBUTION.md`。**文件名照它的原样**，以后跟上游时好逐个对着挑 ——
//! 只有两个例外：`error.rs` → `proxy_error.rs`、`sse.rs` → `sse_text.rs`。workspace 里的模块名
//! 必须全局唯一（`architecture.rs` 把同名模块合成一个点，环就查不出来了），而
//! `qb-foundation::error`、`qb-station::sse` 先占了这两个名字。
//!
//! # 这一层不做的
//!
//! 不联网、不碰文件、不碰数据库、不取「现在几点」之外的系统状态 —— 那些在
//! `qb-app::local_router`。⛔ 也不认识官方账户：这里决定的是「换下一条中转线」，
//! 物理上够不着账户槽位（`architecture.rs` 的 `proxy_logic_can_never_reach_account_switching`）。

pub mod aggregate;
pub mod body_filter;
pub mod cache_injector;
pub mod classify;
pub mod codex_compaction;
pub mod convert;
pub mod envelope;
pub mod media_sanitizer;
pub mod model_capabilities;
pub mod model_mapper;
pub mod opaque_state_rectifier;
pub mod proxy_error;
pub mod session;
pub mod sse_text;
pub mod thinking_budget_rectifier;
pub mod thinking_optimizer;
pub mod thinking_rectifier;
pub mod tool_media;
pub mod types;
pub mod usage;
