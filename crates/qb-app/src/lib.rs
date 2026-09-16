//! 编排层（L3）。
//!
//! # 这一层装什么
//!
//! **需要同时碰两个以上领域的那些事。** 判据很简单：一件事只要牵扯到
//! 两个 L2 crate，它就不属于其中任何一个，属于这里。
//!
//! | 模块 | 它编排什么 |
//! |---|---|
//! | [`usecase`] | 关停（门禁 + 杀进程 + 停插件 + 刷托盘）、切号（账户 + hook）、装机（安装 + 上锁窗口）、存设置（写盘 + 作废判定缓存） |
//! | [`workspace`] | 工作区聚合：供应商 · 凭证 · 环境 · 启动方案 · 会话，以及「起一个会话」这条最长的链 |
//! | [`snapshot`] | 备份与回滚：数据库 + 账户目录 + 门禁状态一起进、一起出 |
//! | [`profile`] | 统一档案：把「账号 + 供应商 + 时区」当成一个整体切过去 |
//! | [`diagnostics`] | 拿工作区里那条环境记录去实测连通性 |
//!
//! # 它不认识 Tauri
//!
//! 这一层收 `sink::ProgressSink` / `sink::EventSink` / 回调，不收 `AppHandle`，
//! 也不收 `AppState`。所以它的测试不链接 tao/wry —— `events.rs` 里记的那个
//! `0xc0000139` 崩在 main 之前的坑，在这一层不存在。
//!
//! 唯一一处曾经的例外是 `workspace::launch(…, state: &crate::AppState)`，
//! 现在收的是 `&GateState`：它本来就只用得到那一半。
//!
//! # 政策边界
//!
//! `profile::apply` 会切账户，所以**它必须是纯人工触发的**：没有定时器、
//! 没有看门狗触发点。加一个自动调用点，「统一档案」就变成了自动轮换账户。

pub use qb_accounts::{accounts, residue};
pub use qb_contract::domain;
pub use qb_extensions::{extensions, plugins};
pub use qb_foundation::{audit, error, paths, sink};
pub use qb_install::install;
pub use qb_iplock::gate;
pub use qb_launch::{killswitch, launch};
pub use qb_platform::{
    config_io, endpoint, operations, process, repository, secret, sessions, settings, signature,
};
// 启动时对齐要先知道出口 IP 落在哪个时区 —— 那是 `qb-probe` 的事。
// 依赖本来就在 Cargo.toml 里（诊断用得到），这里补上再导出。
pub use qb_probe::probe;
pub use qb_relay::relay;
pub use qb_sysenv::sysenv;

pub mod diagnostics;
pub mod local_router;
pub mod profile;
pub mod snapshot;
pub mod usecase;
pub mod workspace;
