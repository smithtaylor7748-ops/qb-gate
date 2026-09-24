//! 编排层。**跨域用例住在这里。**
//!
//! 领域模块（`gate` / `install` / `accounts` / `extensions` …）互不依赖；
//! 任何需要同时动两个领域的动作 —— 比如「关停时同时杀进程、停插件、刷托盘」
//! —— 都属于这一层。把它留在任何一个领域模块里，那个模块就会变成
//! 伪装成底层的编排器，正是 `gate` 之前的下场。

pub mod account_ops;
pub mod account_probe;
pub mod egress_checks;
pub mod gate_ops;
pub mod hook_ops;
pub mod install_ops;
pub mod ipv6_ops;
pub mod locale_ops;
pub mod purge_ops;
pub mod settings_ops;
pub mod station_batch;
pub mod station_billing;
pub mod station_ops;
pub mod station_probe;
pub mod token_summary;
pub mod turnstate_ops;

pub mod antigravity_ops;
pub mod antigravity_quota;
pub mod codex_accounts;
pub mod google_oauth;
pub mod login_health;
pub mod tavern_quota;
