# QB Gate · macOS Apple Silicon

浏览器最低要求依据：[Tailwind CSS 官方兼容性说明](https://tailwindcss.com/docs/compatibility)（Safari 16.4）。

**已于 2026-11-10 发布 [macos-v0.25.12-preview.1](https://github.com/smithtaylor7748-ops/qb-gate/releases/tag/macos-v0.25.12-preview.1)**，应用版本保持 0.25.12，标签指向已验收源码 `5f9703d19ab6602c2704ad704f282757d58190c3`。适用 Apple Silicon（M1 及更新芯片），最低 macOS 13.3；实际原生运行验证使用 macOS 15，未在 13.3 实机验收，不支持 Intel Mac。

同一提交的 [Mac 十项作业](https://github.com/smithtaylor7748-ops/qb-gate/actions/runs/42788560066)与 [Windows 回归](https://github.com/smithtaylor7748-ops/qb-gate/actions/runs/42788560193)全部通过。**自动验证不等于所有真实账号和首次系统授权流程正常**，待验范围见下文。

## 当前源码修复（2026-11-10，未发布）

应用版本仍为 0.25.12。以下修复尚不在上面的 `preview.1` 下载包中。Windows 本地完整回归已通过：Rust 3160 项（2 项既有忽略）、前端 370 项、IPC 353 个类型、UI 151 个布局 / 主题场景与 27 个交互流程，以及构建、格式、Clippy、依赖审查。本轮 Mac 原生验证尚待执行；下文发布包的测试数量只代表原发布提交。

- **各项检测**：读取 Mac 系统代理、默认路由及系统 DNS 解析器路由；“跟随系统代理”使用启用系统配置支持的 HTTP 客户端。PAC / 自动发现代理不执行，SOCKS / 接口限定代理不能准确复现时显示未知。DNS 路由缺证据不报通过或满分；IPv6 服务异常、无有效回显或返回 IPv4 均不能证明本机没有 IPv6。Mac 浏览器管理策略未读取时明确未知；环境变量仅代表面板进程，不声称已检查终端启动文件。旧 DNS / 体检 / 出口一致性持久报告需重测。
- **IP 锁**：“检测当前出口”会真正探测，但只更新观察结果，不授权启动、不解除手动暂停。首装可检测后添加当前 IP；Mac 页可查看监督状态、复验重开、确认关闭和拦截日志。继续使用受管启动检查与看门狗，仍不具备 Windows 全局执行锁。
- **登录与打开**：Claude 主动账户检测和凭据状态统一读取当前托管槽位的官方 Keychain；读不出时显示原因。Terminal 或系统授权等待结束后，客户端运行前再次检查当前出口及门禁是否已变更。工具查找与子进程使用同一 PATH，纳入用户 nvm 目录；原生 Claude 的版本命名入口也纳入候选识别，回收仍核对签名及既有范围。
- **删除槽位**：Mac 删除后保留官方 Keychain，不等于退出 Claude 登录。为避免同名新槽位继承旧身份，删除成功的名称及其大小写 / Unicode 规范化变体不再用于新建；旧版残留的关联凭据也会阻止同名新建。需要新账户时使用新槽位名，官方退出在 Claude Code 中完成。

系统代理例外清单、绕过简单主机名或面板进程中的代理变量改变选路时，也不会仅凭相同回显就宣称浏览器出口一致；结果显示未知，提示只列相关变量名，不显示代理凭据。

新增原生只读检查实际执行 `scutil`、`route`、`ifconfig` 与 `networksetup`，核对真实 OS 输出及接口限定路由；每条采集命令有超时和输出上限，不修改网络。受管启动验证使用隔离临时资料和测试程序。以上测试即使通过，也不能替代双真实账户、首次 Terminal 授权和实际换网验收。

`.github/workflows/macos-core.yml` 提供仅做回归的 Apple Silicon 工作流，触发分支为 `codex/macos-core-*`。它执行源码、网络只读采集、隔离 Keychain、准备 / 取消 / 开始受管进程与 nvm 解释器验证，不生成安装包或发布 Release。

## 安装

[下载 DMG](https://github.com/smithtaylor7748-ops/qb-gate/releases/download/macos-v0.25.12-preview.1/QB-Gate-macOS-arm64.dmg)，打开后将 QB Gate 拖入 Applications，再从 Applications 打开。发布页另附应用归档、`SHA256SUMS.txt`、中文安装说明及许可证。

DMG 为 16,710,185 字节，SHA-256：`2afd5b20b270b9f28a26ea889bca159233deb4474ff89af46c4c859821ee93b0`。仅下载 DMG 时运行 `shasum -a 256 QB-Gate-macOS-arm64.dmg` 并与此值核对；下载两份二进制与清单后可运行下方完整清单校验。

当前构建使用 ad-hoc 签名，尚无 Apple Developer ID 签名与公证。如果 macOS 阻止首次打开，请先确认来源并核对 SHA-256，再到“系统设置 → 隐私与安全性”使用“仍要打开”。不需要关闭 Gatekeeper。需要无警告分发时，应在正式发布流程配置开发者证书与公证，当前开发构建不读取这些凭据。

Mac 终端校验下载文件：

```sh
shasum -a 256 -c SHA256SUMS.txt
```

## 平台范围

- 本地中转线路、模型路由与统计沿用现有跨平台核心。应用资料存于当前用户的 `Library/Application Support`，QB Gate 自己保存的密钥由登录钥匙串保护。各官方客户端的登录与账户切换需要分别验证，不能将 QB Gate 密钥存储等同于官方凭据迁移。
- 原生实际安装验证覆盖 Claude Code 2.1.311 / 2.1.322（升级及双向回滚）、Codex CLI 0.167.0、Gemini CLI 0.67.0；OpenCode 1.19.5、Grok Build 1.0.77、Pi 1.5.1、MiniMax Code 0.7.6、OpenClaw 2026.11.2 的生产安装、检测、更新、真实版本执行、卸载及外部安装保持均通过。附加 CLI 使用独立用户目录，不修改终端全局 PATH。
- SillyTavern 1.19.0 官方安装、依赖安装、实际本地 HTTP、控制台、复用、停止及进程清理通过；模型桥接推理未验。Hermes 自动安装、升级和卸载暂不支持，使用官方文档手动安装；桌面应用也从各自官方渠道安装。
- Codex 版本信息来自 GitHub 公开发布 API，匿名请求按出口 IP 共享额度。触发限流时界面报告可重试时间，下载仍必须通过官方 SHA-256 校验；CI 的只读认证请求成功不代表所有网络环境都能下载。
- 从面板启动的受管会话在启动前检查出口 IP，并由看门狗监督。Finder、其他终端或外部工具自行启动的程序不经过启动前检查，但门禁失效时，看门狗仍可能按官方程序路径识别规则关闭检测到的官方进程。
- macOS 没有 Windows 的 NTFS Deny ACE 执行锁、Windows 防火墙规则或注册表环境清理。界面明确显示这些机制不适用，不宣称整机已保护。
- 开机登录使用当前用户 LaunchAgent；Claude 会话内门禁使用原生辅助命令。真实账户登录、系统钥匙串授权、Terminal 交互以及断网时会话回收仍需 Mac 实机验收。
- 安装包登记 `qbgate://` 导入链接，macOS 打开链接后只显示预览确认，不自动导入或启动。CC Switch 的默认协议处理者不在 Mac 上改动，`ccswitch://` 链接可复制到面板预览并确认。

## 更新

本次发布是独立 prerelease，通过上述发布页手动下载安装，不进入稳定版自动更新通道；Windows latest 仍为 `v0.25.12`。不要把本预览标签写入 `update.json`：现有更新器只接受 `v` 加三段数字，并从 stable latest 获取清单。

Mac 更新器按同一发布的 `SHA256SUMS.txt` 核验 arm64 DMG，再退出并打开镜像，需手动替换应用。原生自动验证已通过**同版本**真实镜像校验、退出打开、复制替换和重启资料保留；互联网在线升级及跨版本资料迁移尚未验收。

## 在 Mac 构建

需要 Xcode Command Line Tools、Node.js 24、Rust stable（至少 1.89），以及足够的磁盘空间。只在 Mac 运行以下命令：

```sh
xcode-select --install
npm ci
npm run build:mac
```

`build:mac` 固定目标 `aarch64-apple-darwin`，调用 Tauri 生成真实 `.app` 与 `.dmg`，复制安装镜像、应用归档、哈希和许可证到 `dist-installers`。支持通过 `CARGO_TARGET_DIR` 将 Rust 构建缓存放到其他磁盘。Windows 运行此脚本会明确说明需要 Mac，不生成伪安装包。

## 自动验证与构建

`.github/workflows/macos.yml` 在标准 `macos-15` arm64 runner 上运行，开头核对 `uname -m`。源码检查、应用打包、原生运行时与官方 CLI 安装分为独立任务。检查包括格式、前端、类型契约、Rust 单测、Clippy、浏览器回归、真实 Keychain 加解密与受管进程、CLI 官方哈希/架构/启动/回滚，以及挂载 DMG、安装并启动真实 WKWebView、调用原生 IPC 和切换主要页面。使用临时目录、隔离的临时钥匙串和测试密钥，不登录真实用户账号。

已发布包来自上述同一提交成功构建，公开 Release 的二进制与构建产物一致。Rust 测试 Mac 3,084 / Windows 3,132 通过，各有 2 项既有文档示例忽略；两平台前端 355、IPC 契约 353、UI 151 个布局/主题场景与 27 个交互流程通过。

实际应用内还验证了 MCP、Skills、提示词、工具文件、线路、项目、备份、会话历史的合成资料生命周期与重启持久化；系统 URL 确认/取消、窗口关闭唤回、原生退出、LaunchAgent 启停、Keychain 锁定/错误钥匙串拒绝均通过。LaunchAgent 注册成功不等于已执行真实系统注销/登录。

## Mac 实机验收

1. 首次 Gatekeeper 打开、Terminal 自动化授权的允许/拒绝，以及实际菜单和键盘交互。
2. 用新的测试槽位验证真实 Claude / Codex / Gemini 登录、切换、额度与刷新；Claude Desktop、Codex Desktop、Antigravity 的身份隔离分别验收。
3. 配置白名单，验证不合格 IP 阻止受管启动，以及断网/切换 IP 时真实官方进程的监督回收。
4. 使用自己的中转服务和酒馆桥接核对真实模型请求、流式输出与费用。
5. 实际系统注销/登录自动启动；未来正式 Mac Release 的在线升级及跨版本资料迁移。

验收当前源码修复时，还须覆盖：首次检测不会自动开门；Terminal 等待授权期间关门或换网后不能继续启动；Finder 启动下 nvm 的 Node 可发现并可执行；两个真实 Claude 槽位的主动检测、钥匙串拒绝、删除后同名新建拒绝，以及 DNS / PAC / IPv6 缺证据时不会显示通过。

这些步骤包含真实系统交互，不能用 Windows 检查或浏览器演示代替。

## Claude 官方账户兼容性证据

CLI 启动同时传入同一槽位的 `CLAUDE_CONFIG_DIR` 与 `CLAUDE_SECURESTORAGE_CONFIG_DIR`，并清理继承的旧值；传入的是槽位本体路径，不将切换用的符号链接路径参与钥匙串键名计算。前者是[官方设置文档](https://code.claude.com/docs/en/settings)公开的配置目录选项，后者及钥匙串命名是发行代码的兼容细节，不属于官方稳定接口承诺。

只读核对了[官方 npm 2.1.63 发行包](https://registry.npmjs.org/@anthropic-ai/claude-code/-/claude-code-2.1.63.tgz)与本机官方 2.1.299.0 程序中的嵌入实现：非默认目录经 NFC 规范化，再取 SHA-256 前 8 位，service 为 `Claude Code-credentials-<摘要>`。旧版 account 使用当前用户名；新版使用 `claude-code-user`，也支持同 service 的 `#m` 元数据和 `#0` 等分片。读取只针对 QB Gate 自己管理的槽位，不查询无后缀的全局 service，不新建、复制、覆盖或删除官方钥匙串凭据。

登录读取被系统拒绝或分片不完整时显示「登录状态未知」及原因；额度读取失败原样报告，不因此覆盖身份。Windows 文件存储保持原有路径。Mac 上额度请求完成后重新比较当前访问令牌，防止刷新期间换号把结果记到旧身份；文件时间戳型失败缓存不用于 Mac 钥匙串。

已发布预览版构建时实际下载并执行 Claude Code 2.1.311 / 2.1.322，核对官方摘要和 arm64，但没有登录真实账户，也没有完整审计这些版本内部的 Keychain 协议。当前未发布修复统一了主动检测与状态读取，并防止已删除槽名再次使用旧凭据；这些改动的 Mac 原生及真实登录验证仍待完成。必须用两个新测试槽位分别登录，验证切换后的账号、额度和退出/重开隔离；Claude Desktop 的系统钥匙串行为单独验收，不能用 CLI 安装成功代替。
