# QB Gate · macOS Apple Silicon

浏览器最低要求依据：[Tailwind CSS 官方兼容性说明](https://tailwindcss.com/docs/compatibility)（Safari 16.4）。

本分支以 Apple Silicon（M1/M2/M3/M4 及后续 arm64）为首发目标，最低 macOS 13.3（界面使用 Tailwind v4，要求 Safari/WebKit 16.4 的 color-mix 等 CSS 支持）。当前版本号保持 0.25.12；移植变更属于未发布内容。Windows 上的检查通过不代表已经完成 Mac 实机验收，原生构建与测试结果以 macOS 工作流和交付记录为准。

## 安装

原生构建成功后交付 `QB-Gate-macOS-arm64.dmg`、应用归档与 `SHA256SUMS.txt`。打开 DMG，将 QB Gate 拖入 Applications，再从 Applications 打开。

当前构建使用 ad-hoc 签名，尚无 Apple Developer ID 签名与公证。如果 macOS 阻止首次打开，请先确认来源并核对 SHA-256，再到“系统设置 → 隐私与安全性”使用“仍要打开”。不需要关闭 Gatekeeper。需要无警告分发时，应在正式发布流程配置开发者证书与公证，当前开发构建不读取这些凭据。

Mac 终端校验下载文件：

```sh
shasum -a 256 -c SHA256SUMS.txt
```

## 平台范围

- 本地中转线路、模型路由与统计沿用现有跨平台核心。应用资料存于当前用户的 `Library/Application Support`，QB Gate 自己保存的密钥由登录钥匙串保护。各官方客户端的登录与账户切换需要分别验证，不能将 QB Gate 密钥存储等同于官方凭据迁移。
- 软件页提供 Apple Silicon Claude Code / Codex CLI 托管安装，发现 Applications 中的桌面应用和常见 Homebrew / npm 路径。桌面应用从各自官方渠道安装。
- Codex 版本信息来自 GitHub 公开发布 API，匿名请求按出口 IP 共享额度。触发限流时界面报告可重试时间，下载仍必须通过官方 SHA-256 校验；CI 的只读认证请求成功不代表所有网络环境都能下载。
- 从面板启动的受管会话在启动前检查出口 IP，并由看门狗监督。Finder、其他终端或外部工具自行启动的程序不经过启动前检查，但门禁失效时，看门狗仍可能按官方程序路径识别规则关闭检测到的官方进程。
- macOS 没有 Windows 的 NTFS Deny ACE 执行锁、Windows 防火墙规则或注册表环境清理。界面明确显示这些机制不适用，不宣称整机已保护。
- 开机登录使用当前用户 LaunchAgent；Claude 会话内门禁使用原生辅助命令。真实账户登录、系统钥匙串授权、Terminal 交互以及断网时会话回收仍需 Mac 实机验收。
- 安装包登记 `qbgate://` 导入链接，macOS 打开链接后只显示预览确认，不自动导入或启动。CC Switch 的默认协议处理者不在 Mac 上改动，`ccswitch://` 链接可复制到面板预览并确认。

## 更新

Mac 更新器只选择 `QB-Gate-macOS-arm64.dmg`，下载后按同一发布中的 `SHA256SUMS.txt` 校验。确认更新后面板退出并打开镜像，手动替换 Applications 中的应用后重开。正式 Release 没有 Mac 附件时，更新会明确失败，不下载 Windows 安装包。开发分支产物不会自动发布为正式更新。

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

首次推送 `codex/macos-port-*` 分支会触发此工作流，不需要合并主分支、打标签或创建 Release。成功产物名为 `qb-gate-macos-arm64`，保留 14 天。使用常规公开仓库标准 runner，不使用 `-large` / `-xlarge`。远程推送需按本次会话授权执行。

## Mac 实机验收

1. 首次打开、菜单栏显示、关闭窗口再唤回、退出后重开。
2. 创建测试账户资料与中转线路；确认钥匙串拒绝访问时不会明文保存密钥。
3. 安装 Claude Code / Codex CLI，启动 Terminal 受管会话；使用两个新的 Claude CLI 测试槽位分别登录、切换和重开，核对身份与额度隔离。Claude Desktop 独立验收，不以 CLI 结果替代。
4. 配置白名单，验证不合格 IP 阻止受管启动，以及断网/切换 IP 时的监督回收。
5. 检查实际桌面应用路径、登录项开关、更新 DMG 手动替换流程。

这些步骤包含真实系统交互，不能用 Windows 检查或浏览器演示代替。

## Claude 官方账户兼容性证据

CLI 启动同时传入同一槽位的 `CLAUDE_CONFIG_DIR` 与 `CLAUDE_SECURESTORAGE_CONFIG_DIR`，并清理继承的旧值；传入的是槽位本体路径，不将切换用的符号链接路径参与钥匙串键名计算。前者是[官方设置文档](https://code.claude.com/docs/en/settings)公开的配置目录选项，后者及钥匙串命名是发行代码的兼容细节，不属于官方稳定接口承诺。

只读核对了[官方 npm 2.1.63 发行包](https://registry.npmjs.org/@anthropic-ai/claude-code/-/claude-code-2.1.63.tgz)与本机官方 2.1.299.0 程序中的嵌入实现：非默认目录经 NFC 规范化，再取 SHA-256 前 8 位，service 为 `Claude Code-credentials-<摘要>`。旧版 account 使用当前用户名；新版使用 `claude-code-user`，也支持同 service 的 `#m` 元数据和 `#0` 等分片。读取只针对 QB Gate 自己管理的槽位，不查询无后缀的全局 service，不新建、复制、覆盖或删除官方钥匙串凭据。

登录读取被系统拒绝或分片不完整时显示「登录状态未知」及原因；额度读取失败原样报告，不因此覆盖身份。Windows 文件存储保持原有路径。Mac 上额度请求完成后重新比较当前访问令牌，防止刷新期间换号把结果记到旧身份；文件时间戳型失败缓存不用于 Mac 钥匙串。

2026-11-09 查询到官方当前版本 2.1.321，Darwin arm64 文件 323,686,048 字节，官方清单 SHA-256 为 `abc9feeeca8091d51b623291e044cf0528c2f6b74e0c1d6ef671d52c73f7cd3c`。受下载源限速影响，当前包没有完整下载或完成代码核验，不能据此保证其钥匙串实现与已核版本完全一致。原生验收必须用两个新测试槽位分别登录，验证切换后账号、额度及退出/重开均隔离；还需单独核对 Claude Desktop 的系统钥匙串行为，不能把 CLI 的证据等同于桌面端保证。
