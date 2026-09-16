//! Tauri 命令层（L4）。**这里只做参数搬运。**
//!
//! # 为什么按域分文件
//!
//! 拆之前 72 个命令挤在 1544 行的 `lib.rs` 里，另有 30 个在 `commands.rs`，
//! 两套并存的约定（一套手抄 TS 类型、一套 ts-rs 生成）。对标的 cc-switch
//! 是 `commands/` 36 个文件，Cockpit Tools 也是 `commands/` 目录 ——
//! 这是同类项目的常规做法，而不是洁癖。
//!
//! # 三条约定
//!
//! 1. **命令体里不写业务逻辑。** 拿参数、调下层、把结果转成 IPC 形状，
//!    就这三件事。判断、编排、落盘全在 `qb-app` 或各领域 crate 里 ——
//!    这一层没有测试，写在这里的逻辑等于没有安全网。
//! 2. **新命令进这些文件，不回 `lib.rs`。** `lib.rs` 只留 app 装配。
//! 3. **返回类型用 ts-rs 导得出来的。** 还在返回 `serde_json::Value` 的那几个
//!    （`accounts_list` / `detect_software` / `purity_criteria`）是历史欠账，
//!    前端手抄了一个没有源头的形状 —— B0 会把它们定型。

pub mod accounts;
pub mod backup;
pub mod gate;
pub mod install;
pub mod ipv6;
pub mod network;
pub mod plugins;
pub mod probe;
pub mod station;
pub mod system;
pub mod workspace;

pub mod codex_commands;
