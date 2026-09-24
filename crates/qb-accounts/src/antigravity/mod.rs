//! 反重力（Google Antigravity）的账户侧（0.30.0）：IDE 登录槽位、IDE 写下的账户状态、本机用量。
//!
//! | 模块 | 干什么 | 碰不碰凭据 |
//! |---|---|---|
//! | [`ide`] | IDE 槽位 = 一个 `--user-data-dir`；新建 / 切换 / 移除 | 不碰：登录在官方窗口里做 |
//! | [`status`] | 从 IDE 的 `state.vscdb` 读登没登录、邮箱、档位、各模型剩余额度 | 登录态只问长度；状态不是令牌 |
//! | [`token`] | 使用者点反重力的刷新图标时，读 IDE 槽位 / Hub 的令牌（2026-09-23） | **碰**：只进内存，见它的文件头 |
//! | [`usage`] | 从语言服务器的对话记录库数 token | 不碰 |
//! | [`proto`] | 上面几个共用的 protobuf 线格式读取器 | — |
//!
//! # 三条规矩
//!
//! 1. **这个 crate 零网络请求。** 本机能读到的（邮箱、IDE 上次写下的额度、用量）都读
//!    官方客户端自己写在本机的文件。联网问额度、换新令牌在 `qb-app::usecase::antigravity_quota`，
//!    只在使用者点刷新图标时发生（2026-09-23 使用者定的）；这里只负责把令牌从本机读出来。
//! 2. **Hub 没有槽位。** 它的令牌在 Windows 凭据管理器，目录隔不开；只有 IDE 能按目录隔离。
//! 3. **不据此做任何决定。** 剩余额度只显示；没有任何「额度低了换槽位」的路径 ——
//!    合规边界头两条，跟 Claude / Codex 那边一样。
//!
//! 能力对照的来源是 `anglee0323/Antigravity-Tools-Lite` 与 `jlcodes99/cockpit-tools`
//! （都是 CC BY-NC-SA 4.0，**一行都没抄**）。它们自己跑 OAuth 登录、把令牌存在自己那里；
//! 这里仍然不跑登录、不存令牌 —— 2026-09-23 起只在点刷新时读官方客户端存在本机的那一份，
//! 过期了在内存里换新。见 ATTRIBUTION.md。

pub mod account;
pub mod hub;
pub mod ide;
pub mod proto;
pub mod status;
pub mod token;
pub mod usage;
