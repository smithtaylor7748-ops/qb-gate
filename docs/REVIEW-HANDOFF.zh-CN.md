# QB Gate v0.12.0：源码审查交接

记录日期：2026-09-12。重构基线为 v0.11.0 / `ed32bd7`，工作分支为 `codex/qb-gate-rebuild`。

本轮修改尚未提交 Git，包含新增文件和删除文件。审查必须同时查看 `git diff ed32bd7`、`git status --short` 与未跟踪文件，不能只看提交历史。没有推送、创建 PR 或发布 GitHub Release。

## 当前交付状态

已经实现可构建的 v0.12.0 重构源码：六入口前端、官方与中转配置隔离、SQLite 元数据、事务式外部配置写入、原生会话登记、多类型扩展中心，以及账户指向、快照、安装回滚和缓存修复。

这不是“原方案全部实机验收通过”的声明。自动验证及未验证边界见 [回归结果](REGRESSION-0.12.zh-CN.md)；产品限制见 [已知限制](KNOWN-ISSUES.zh-CN.md)。真实账户、真实供应商、Job Object 与第三方客户端组合、系统异常退出等仍需要独立实机验收。

## 建议阅读顺序

1. [README](../README.md)：当前定位、功能、构建方式。
2. [架构](ARCHITECTURE.zh-CN.md)：业务和状态边界。
3. [迁移说明](MIGRATION-0.12.zh-CN.md)：数据去向、凭证边界、回退规则。
4. [变更记录](../CHANGELOG.md)、[回归结果](REGRESSION-0.12.zh-CN.md)、[已知限制](KNOWN-ISSUES.zh-CN.md)。
5. [来源声明](../ATTRIBUTION.md)、[许可证](../LICENSE)、`dependencies.json`、`license-supplements.json` 和 `THIRD_PARTY_NOTICES.txt`。

`DESIGN-NOTES.zh-CN.md` 主要保留历史背景，其中旧版本描述不能代替当前代码。原始项目档案也属于背景材料。

## 代码地图

| 范围 | 主要文件 |
| --- | --- |
| 六入口界面、任务中心与草稿 | `src/features/`、`src/styles/workspace.css` |
| 查询缓存、类型化 IPC | `src/lib/store.ts`、`src/lib/workspace.ts`、`src/lib/generated/` |
| 公开数据契约与命令 | `crates/qb-contract/src/domain.rs`、`src-tauri/src/commands/`、`src-tauri/examples/export-types.rs` |
| 数据库及迁移 | `crates/qb-platform/src/repository.rs` |
| 官方与中转上下文、配置预览及回滚 | `crates/qb-app/src/workspace.rs` |
| 文件写入日志与跨数据库提交恢复 | `crates/qb-platform/src/config_io.rs`、`src-tauri/src/startup.rs` |
| 账户指向切换 | `crates/qb-accounts/src/accounts/mod.rs`、`accounts/transaction.rs` |
| 原生进程、PID 创建时间、Job Object | `crates/qb-platform/src/sessions.rs`、`process.rs` |
| 操作互斥、取消、修订事件 | `crates/qb-platform/src/operations.rs` |
| 门禁、统一判定、暂停与应急关停 | `crates/qb-iplock/src/gate/`、`crates/qb-launch/src/killswitch.rs`、`src-tauri/src/tray.rs` |
| 中转协议与诊断 | `crates/qb-app/src/diagnostics.rs`、`crates/qb-relay/src/relay/` |
| 扩展安装、MCP、Skills、应用接入 | `crates/qb-extensions/src/extensions.rs`、`crates/qb-extensions/extension-catalog.json`、`src/plugins/` |
| 配置快照、安装单元回滚 | `crates/qb-app/src/snapshot.rs`、`crates/qb-install/src/install/versions.rs`、`install/managed.rs` |
| 开源交付检查与 UI 回归 | `scripts/`、`.github/workflows/` |

## 重点审查问题

- 账户切换在清场失败、进程无法确认和指向恢复失败时，是否始终阻断并保留可恢复状态。
- 官方与中转目录、子进程环境变量、凭证存储和会话归属是否真正隔离；是否存在旧 IPC 绕过新服务的入口。
- SQLite 与外部文件事务在任何中断点能否恢复；恢复期间是否错误覆盖外部修改；日志及备份是否可能泄露凭证。
- 连续回滚是否对应真实已应用配置，而非后来保存的草稿；安装 MCP 是否保留原有外部配置冲突。
- Job Object / PID 创建时间 / 句柄关闭与恢复逻辑，及祖先进程保护是否完整；真实第三方启动器是否会逃逸或产生错误会话状态。
- 快照恢复是否保护来源、自动备份、安装位置、OAuth 和实际槽位文件；安装单元是否完整覆盖 Codex 配套文件。
- 扩展路径、重解析点、符号链接、大小和名称检查是否充分；更新冲突、卸载保留数据、凭证引用是否正确。
- 诊断的 401、404、超时、SSE 中断、空输出、工具调用与后端证据是否如实呈现；是否存在未经点击的计费请求。
- 页面、托盘和后台任务是否共享最新状态；重复点击、切页、过期请求与草稿冲突是否会造成意外写入。
- 是否有未被源文件扫描覆盖的秘密或个人信息，以及依赖声明、许可证原文、源码获取位置是否满足公开发布要求。

## 仍需专项验收或继续完善

- 本轮是架构重构和首批功能实现，旧安装服务仍有兼容 IPC；六个业务域并未全部移动成独立 crate。
- 酒馆以接入已有安装为主，本体与依赖升级交给原项目；MCP 手动导入一次选中一个服务，非批量导入。
- Skills 有来源更新检查；精选 MCP 更新跟随目录版本，不是完整通用包管理器。
- 程序内草稿跨页保留，退出后不保存含 Key 的草稿。旧服务细粒度任务事件未全部持久化。
- 原生 Windows 多会话、PID 复用、实际终端/脚本入口、实际 MCP stdio 启动与面板异常退出仍需实机测试；不能用演示 UI 截图代替。
- 本地目录使用保守的重解析点检查；OneDrive 特殊文件、`.ps1` 启动器及不同终端版本需要兼容性复查。
- 文件日志保留周期、长期历史增长和更完整的冲突合并体验还可以继续完善。
- 安装包未配置代码签名。GitHub Actions 文件已准备，但未在 GitHub 实际运行。

## 本地复查命令

在 Windows、Node.js 24、Rust stable 环境，从仓库根运行：

```powershell
git status --short
git diff --stat ed32bd7
npm ci
npm run build
npm test
npm run format:check
npm run types:check
cargo fmt --all --check
cargo test --locked --workspace
cargo clippy --locked --workspace --lib -- -D warnings
npm run test:ui
npm run release:check
```

UI 回归使用演示数据并阻断外部请求。Rust 回归使用临时目录与替身文件；不应连接真实账户、发送付费模型请求或操作真实客户端。仅审查源码时不需要启动生产面板或运行安装程序。

反馈建议逐条给出严重性、代码位置、触发条件、实际影响和可复现证据；将已证实的问题与需要实机确认的风险分开。
