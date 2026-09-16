//! 官方账户（L2）。
//!
//! | 模块 | 干什么 |
//! |---|---|
//! | [`accounts`] | 槽位：每一份官方登录状态各自一个目录，切换是换目录 |
//! | [`residue`] | 官方目录里有没有中转 / API 认证残留 |
//!
//! # 合规边界写在这一层
//!
//! **不做自动换号。** 没有定时器、没有看门狗触发点、没有任何按额度或 429
//! 自动切槽位的路径。存在那条路径，「多槽位」的定位就从「管理你自己的账户」
//! 变成了「规避限制」—— 见 `DISCLAIMER.md` 第 4 节那四条设计约束的前两条。
//!
//! 中转站的熔断事件**代码上**到不了这里：熔断住在 `qb-relay` / `qb-station`，
//! 它们不依赖这个 crate，这个 crate 也不依赖它们。
//!
//! # 用量只读本机文件
//!
//! `accounts::usage` 读的是官方客户端自己写在本机的那份，零网络请求，
//! 只用于显示，面板不据此做任何决定。

pub use qb_contract::domain;
pub use qb_foundation::{audit, error, paths};
pub use qb_install::install;
pub use qb_platform::{config_io, process};

pub mod accounts;
pub mod residue;

pub mod codex;
