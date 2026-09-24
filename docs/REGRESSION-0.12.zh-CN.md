# v0.12.0 回归记录

记录日期：2026-09-12 起（含源码审查后的修复与 09-13 的前端重排）。基线 v0.11.0 / `ed32bd7`，当时的工作树版本 **v0.12.2**。结果针对本地未提交源码，不代表 GitHub CI 或真实账户验收。

> **这一份只记 v0.12 那一轮。** 之后做的整轮架构重构（13 个 crate 的 workspace、循环依赖归零、错误分类、全量 ts-rs）作为 **v0.13.0** 一起发出去，它的门禁数字见 `CHANGELOG.md` 的 0.13.0 一节与 `docs/ARCHITECTURE.zh-CN.md`；下面表里的 335 项 Rust 回归在 v0.13.0 是 **493 项**。

## 已执行

| 检查                          | 实际结果                                                               |
| ----------------------------- | ---------------------------------------------------------------------- |
| TypeScript + Vite 生产构建    | 通过                                                                   |
| Vitest 缓存回归               | 5 项通过                                                               |
| Rust 回归                     | 335 项通过，0 失败；原始基线 294 项，审查后新增 7 项                   |
| Clippy `--lib -- -D warnings` | 通过                                                                   |
| 前端格式检查                  | 通过                                                                   |
| Rust 格式                     | `cargo fmt --check` 通过                                               |
| Rust 导出 TypeScript          | 已重新生成，`npm run types:check` 一致                                 |
| 浏览器响应式、主题、缩放      | 72 个组合通过                                                          |
| 浏览器交互                    | 4 个流程通过：扩展目录隔离、搜索与返回、切页保留草稿、Escape 关闭弹窗  |
| 公开源码检查                  | 206 个候选文件通过高置信度凭证与个人路径检查；不等同于人工隐私审计     |
| npm 依赖审计                  | 最近一次结果为 0 项漏洞                                                |
| 依赖清单                      | 已记录 716 个依赖，包含构建/测试和其他平台依赖；随附可获取的许可证文本 |

浏览器结果见 `ui-regression.json`；截图见 `screenshots/`。五个入口、四种窗口宽度、浅深主题，以及 1.25 / 1.5 deviceScaleFactor 均纳入检查。截图使用虚构演示数据。

v0.12.1 的前端重排：总览恢复 v0.11.0 的综合评分版式（权重堆叠条 + 四格明细 + 账户槽位 + 启动磁贴 + 一键关闭 + 运行日志）；三项体检与门禁三档改为总览上点开的小窗，取消独立的「安全」入口；槽位横条新增五小时 / 七天用量与恢复时刻；DNS 与中文环境的结果改为跨重启保留并标注测定时刻。

新增 Rust 回归重点包括：目录切换中断恢复、文件事务失败与数据库提交恢复、快照唯一 ID 和保留策略、连续安装回滚、Codex 配套文件恢复、MCP 配置冲突与原生字段、Skill 更新冲突与卸载保留、配置预览指纹、诊断流式/工具响应、取消、环境隔离、旧数据迁移保护。最后两项新增回归验证连续环境回滚不使用未应用草稿，以及 MCP 修改不清除外部配置冲突。

## 源码审查后新增的回归

同日做了一轮源码审查，修掉的问题与对应回归：

| 问题                                                                                   | 新增回归                                                                                                                      |
| -------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| 看门狗自己写了一套处置，`decide` 退化成只有单测在调（改 `unknown_grace()` 不再有效果） | 判定接回主循环；`Tick` / `StopReason` / `decide` 收窄为 `pub(super)`，再断链会让 `cargo clippy --lib -- -D warnings` 编译失败 |
| 一条读不了或结不清的恢复记录会把面板永久锁在恢复页                                     | `one_unreadable_journal_neither_blocks_the_others_nor_is_deleted`：坏记录只登记不中断，可显式移入 quarantine，目录外路径拒绝  |
| 三张历史表只增不减，且每次操作后全量回前端                                             | `history_is_capped_but_live_sessions_are_never_dropped`：受保护的会话不占额度也不删                                           |
| 恢复记录（内含配置全文密文）永不清理                                                   | `retention_spares_unsettled_referenced_and_fresh_records`：未结清、被环境回滚指着、未到期的都不删                             |
| Skills 可以「装」到不读 `skills/` 的 Codex，还显示已安装                               | `skills_are_never_offered_or_installed_for_codex`：含目录 JSON 的声明                                                         |
| 「检查来源更新」会冲掉 ATTRIBUTION 里核对过的固定提交                                  | `curated_pins_are_the_ones_attribution_signed_for`：目录里的哈希必须能在 ATTRIBUTION.md 里找到                                |
| 中转会话被门禁按官方口径关停                                                           | `relay_sessions_still_unlock_but_are_never_stopped_by_the_gate`：解锁与关停是两个判断                                         |
| 数据库里的目录行能把精选条目重新指到别的仓库和版本                                     | `a_database_row_cannot_repoint_a_curated_entry`：合并时来源与版本一律以内置目录为准                                           |

这一轮**没有**新增实机验收；下面「未执行」一节仍然全部成立。

## 未执行或不能由这些结果证明

- 未登录、切换真实账户，未验证真实官方与中转客户端同时运行。
- 未发送付费模型请求，未对真实供应商完成端到端诊断。
- 未针对真实软件执行升级、安装根迁移、回滚或资产清理；安装单元回归使用替身文件。
- 未对真实 PID 复用、第三方启动器逃逸、断电、完整磁盘故障和 WebView 原生 IPC 执行专项验收。
- 浏览器回归不等于所有控件均完成全键盘可用性测试，也不等于 Windows 所有缩放组合已实机验证。
- 未在 GitHub 运行 CI 或发布 Release；代码签名未配置。

## 安装包与本机版本

`npm run tauri build` 已在本机完成，产物：

```
src-tauri/target/release/bundle/nsis/QB Gate_0.12.0_x64-setup.exe
SHA-256 3b888531403c66f2bb23f968df3604b432cb318a910f48f7132f0d2cb7307598
```

本机已从 **0.11.0 原地升级到 0.12.0**（2026-09-12 11:47，静默安装 `/S`，退出码 0）。
卸载项只有一条，没有出现 v0.9.0 改名时那种两套并存。

升级后核验到的：

- 注册表 `DisplayVersion` 与 `qb-gate.exe` 的 `FileVersion` 都是 0.12.0；
- 首次启动建出 `workspace.sqlite3`，旧数据迁移写了备份目录 `migration-backups/20260912-154721-…`
  （本机没有 `relay.json` / `profiles.json` / `settings.json`，实际只备份了 `lease.json`）；
- `allowlist.txt`、`progress.json`、`lease.json` 原地未动，账户槽位联结点仍指向 `claude-profile-NEW1`；
- `ip-gate.log` 没有「启动恢复失败」，租约按原样接回：
  「面板重启：上次的租约（claude-desktop）仍然有效，出口 IP … 在白名单内，已重新放行」；
- 升级过程没有终止任何 Claude 进程。

**这只证明装得上、起得来、旧状态没丢。** 新功能（中转环境、扩展安装、账户切换、
快照恢复、版本回滚）在本机一次都还没实际跑过 —— 那部分仍属下面「未执行」一节。

安装包的 `qb-gate.exe` 与 `target/release/qb-gate.exe` 哈希不同是正常的：
Tauri 打包前会给 exe 打上 bundle 类型标记（`Info Patching … with bundle type information: nsis`）。
