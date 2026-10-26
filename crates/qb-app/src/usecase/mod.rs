//! 编排层。**跨域用例住在这里。**
//!
//! 领域模块（`gate` / `install` / `accounts` / `extensions` …）互不依赖；
//! 任何需要同时动两个领域的动作 —— 比如「关停时同时杀进程、停插件、刷托盘」
//! —— 都属于这一层。把它留在任何一个领域模块里，那个模块就会变成
//! 伪装成底层的编排器，正是 `gate` 之前的下场。

pub mod account_ops;
pub mod account_probe;
/// Claude Code 的两个小件：跳过首次确认、VS Code 插件联动（中转站重构阶段 4）。
pub mod claude_code_ops;
/// Codex 会话归桶（中转站重构阶段 4）：先备份、逐个桶勾选、按账本还原。
pub mod codex_history_ops;
/// 把线路写进共存型工具（中转站重构阶段 4，写真 Key）。
pub mod coexist_ops;
/// 导入链接（中转站重构阶段 5，X4）：待处理的那一条、给界面看的（Key 打码）、确认之后导入。什么都不自动启用。
pub mod deeplink_ops;
/// Claude 桌面端直连档的开关（中转站重构阶段 4，写真 Key）。
pub mod desktop_direct_ops;
pub mod egress_checks;
/// MCP / Skills / 提示词共用的根目录与目标（中转站重构阶段 5）。
pub(crate) mod ext_place;
pub mod gate_ops;
pub mod hook_ops;
pub mod install_ops;
pub mod ipv6_ops;
pub mod locale_ops;
/// MCP 面板：一条服务器勾哪些目标就写进哪些配置（中转站重构阶段 5，M1）。
pub mod mcp_ops;
/// 项目：中转那一侧的一整套设置存成名字、一键换（中转站重构阶段 5，M4）。⛔ 碰不到账户切换。
pub mod project_ops;
/// 提示词库：每个目标挑一条写进它的提示词文件（中转站重构阶段 5，M3）。
pub mod prompt_ops;
pub mod purge_ops;
pub mod settings_ops;
/// Skills 面板：装一次，勾哪些目标就复制到哪些 skills 目录（中转站重构阶段 5，M2）。
pub mod skill_ops;
pub mod station_batch;
pub mod station_billing;
pub mod station_liveness;
pub mod station_ops;
pub mod station_probe;
pub mod token_summary;
/// 共存型工具自己的说明 / 记忆文件（中转站重构阶段 5，C3 / C4 的附带件）：OpenClaw 工作区与每日记忆、Hermes 记忆、Pi 的系统提示词与模板。
pub mod tool_files_ops;
pub mod turnstate_ops;

pub mod antigravity_ops;
pub mod antigravity_quota;
pub mod claude_zh_ops;
pub mod codex_accounts;
pub mod codex_locale;
pub mod google_oauth;
pub mod login_health;
pub mod tavern_quota;
