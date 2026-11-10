# QB Gate · Mac 安装与使用

适用于 **Apple Silicon（M1 及更新芯片）、macOS 13.3 及以上**。本次发布目标为 **macos-v0.25.14-preview.1**，应用内版本号为 **0.25.14**。安装包正在构建与验收，下列下载入口在正式上传附件后生效。

[下载 Mac 安装包](https://github.com/smithtaylor7748-ops/qb-gate/releases/download/macos-v0.25.14-preview.1/QB-Gate-macOS-arm64.dmg) · [查看本版发布说明](https://github.com/smithtaylor7748-ops/qb-gate/releases/tag/macos-v0.25.14-preview.1) · [下载 SHA256SUMS.txt](https://github.com/smithtaylor7748-ops/qb-gate/releases/download/macos-v0.25.14-preview.1/SHA256SUMS.txt)

## 本版补充环境检测

本地开发分支已同步 STUN / 同族 HTTP 出口对照、代理运行规则、ASN 网络类型及 Claude 客户端语言检测，入口为「出口一致性 → 补充环境检测」。均由按钮手动启动，结果不保存历史，不修改网络设置或参与门禁；WebRTC 未完成采集保持未知，UA-CH 与本次请求 UA 的平台声明比较。语言配置从 `~/Library/Application Support/Claude/config.json` 读取，多份或缺失证据不猜测。

本版纳入已通过 [Mac 原生回归](https://github.com/smithtaylor7748-ops/qb-gate/actions/runs/42868567916) 和 [Windows CI](https://github.com/smithtaylor7748-ops/qb-gate/actions/runs/42868568031) 的 `b963a96` 检测源码：Mac Rust 3160 项、Windows 3206 项，均 0 失败；两端前端 388 项、IPC 362、UI 151 + 28 通过。0.25.14 的新 DMG 与原生 WKWebView 检查由本次完整构建重新验证，完成前不将旧源码回归视为新安装包已通过。

## 安装与首次打开

1. 下载 `QB-Gate-macOS-arm64.dmg`，双击打开。
2. 将 **QB Gate** 拖入 **Applications（应用程序）**。
3. 从“应用程序”打开 QB Gate，安装完成后可推出 DMG。
4. 进入 **软件与安装**，安装需要的 Claude Code、Codex CLI 或 Gemini CLI。Claude、Codex 等桌面应用点“官方下载”，安装到“应用程序”后回面板点“重新检测”。

预览包使用 ad-hoc 签名。首次打开如被 macOS 拦下，确认下载自上面的发布页后，到 **系统设置 → 隐私与安全性 → 仍要打开**，按系统提示确认即可。

需要校验文件时，将本版 `SHA256SUMS.txt` 和 DMG 放在同一目录，打开终端进入该目录，运行：

```sh
shasum -a 256 QB-Gate-macOS-arm64.dmg
```

将输出与清单中同名文件的 SHA-256 比较。若同时下载了发布页的 `.app.tar.gz` 归档，也可运行 `shasum -a 256 -c SHA256SUMS.txt` 一次核对两份文件。

## 先完成各项检测

在 **官方账户** 页点击 **一键全面体检**，完成后点击顶部各项查看明细。换网络、换代理或更新本预览版后，重新测一次即可。

| 检测项     | 怎么使用                                                                                                                          |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------- |
| IP 纯净度  | 查看当前出口，按页面入口打开 IPQS、ippure，核对后记录人工复核结果。                                                               |
| DNS 泄露   | 查看真实 DNS 查询回显，以及 Mac 系统解析器和路由检查结果；配置代理或 VPN 后可重新检测。                                           |
| 中文环境   | 点开“环境体检”查看语言、时区、WebRTC 等信号。登录网站前，点“用默认浏览器测”；使用其他浏览器时，将一次性链接复制到那个浏览器打开。 |
| IP 锁      | 点击“检测当前出口”，核对当前 IP、白名单和门禁状态。添加与放行步骤见下一节。                                                       |
| 出口一致性 | 查看直连、系统代理与浏览器观测的出口信息，并按明细处理差异。                                                                      |

Mac 检测读取系统代理、DNS 和实际路由。遇到复杂代理配置、缺失路由或无效 IPv6 回显时，明细会显示“未知”或“待确认”及原因；按提示检查网络和代理后，重测对应项目即可。

## 配置 IP 锁

1. 在 **官方账户** 页打开 **IP 锁**，点 **检测当前出口**。
2. 确认出口后，点 **加入当前 IP**；首次配置也可点 **把当前 IP 加进白名单**。如需限定国家，在同页设置允许地区。
3. 从 QB Gate 启动官方客户端。面板会在启动前检查 IP，运行期间按设定间隔巡检，默认 5 秒。
4. 如门禁已暂停或关闭，先恢复合格网络，再到 **执行锁 → 受管启动与 IP 检查** 点 **验证并重新放行**。单独检测只更新读数，暂停状态会保留。
5. 需要主动暂停时，先保存工作，再点 **关闭受管门禁** 并确认。原因和处理记录可在同页的 `ip-gate.log` 查看。

出口不在白名单、地区不合格或无法确认出口时，受门禁管理的官方会话会关闭，因此换网前请先保存工作。Mac 采用启动检查与进程监督；从 Finder 或其他终端启动的程序不经过面板的启动前检查，巡检发现的官方进程仍可能被关闭。Windows 的系统执行锁仅在 Windows 提供。

## Claude 官方账户：登录、切换和打开

1. 在 **软件与安装** 确认 Claude Code 已安装，并完成上面的 IP 白名单配置。
2. 打开 **官方账户 → Claude**，点 **新建**，填写一个槽位名，例如 `work-01`，保留“建好之后立刻登录”，点 **建好并登录**。
3. 在打开的 Claude Code 终端中按官方提示完成登录。首次启动若出现控制 Terminal 的系统授权，允许后继续；曾拒绝时可到 **系统设置 → 隐私与安全性 → 自动化** 调整 QB Gate 的 Terminal 权限，再从面板重试。
4. 登录完成后回到账户页刷新，查看账户身份、登录状态与额度。日常使用时点右侧 **Claude Code** 打开当前槽位。
5. 使用另一个账户时，新建另一槽位，或在已有账户行点 **切换／切换并登录**。切换前保存正在进行的 Claude 工作，并按确认框完成切换。

如显示登录已失效，在当前槽位重新打开 Claude Code，按官方流程重新登录；如显示钥匙串读取问题，先按 macOS 提示解锁或授权，再刷新。Terminal 或授权等待结束后，面板会再次核对出口和门禁状态，再启动客户端。

**Claude 桌面端**从右侧对应入口打开，并在官方窗口中完成它自己的登录。Claude Code 的槽位操作按以上步骤进行。

Claude Code 的登录信息由 macOS Keychain 保存。QB Gate 为每个槽位使用独立目录，并让配置目录与安全存储目录保持一致；面板读取当前托管槽位的官方凭据来显示身份和额度，登录、令牌刷新与退出由 Claude Code 完成。

删除槽位前先切到另一个账户。**删除槽位会保留官方 Keychain 登录信息；如需退出账户，先在 Claude Code 内退出。** 删除过的名称不再用于新建，大小写或 Unicode 等价写法也算同名；新账户请使用新名字，例如 `work-02`，以免关联到旧身份。

## 更新到 0.25.14 预览版 1

Mac 预览版通过本页或发布页 **手动下载更新**。从 0.25.12 预览版更新后，应用内显示 0.25.14。

1. 保存工作，结束正在运行的受管会话，并退出 QB Gate。
2. 下载并打开本版 DMG，将 QB Gate 拖入“应用程序”，选择 **替换**。
3. 从“应用程序”重新打开，重新运行一次全面体检，再按需要启动客户端。

替换应用会保留用户资料和 Keychain；账户槽位、配置等资料存放在当前用户的 `Library/Application Support` 下。退出面板会结束由它管理的会话，更新前请先保存。

Windows 使用 Windows 稳定版的更新提示或 [稳定版发布页](https://github.com/smithtaylor7748-ops/qb-gate/releases/latest)中的 Windows 安装包。Mac 预览版使用独立下载入口。

## 其他功能与验证记录

Mac 版还提供中转线路与模型路由、用量统计、MCP、Skills、提示词、项目、备份和会话历史。附加 CLI 与 SillyTavern 可在“软件与安装”管理；Hermes 按官方文档手动安装。开机登录启动使用当前用户的 LaunchAgent。`qbgate://` 链接会先显示导入预览，确认后导入；`ccswitch://` 链接可复制到面板预览。

已发布 preview.2 的核心修复已通过 [Mac 原生 CI](https://github.com/smithtaylor7748-ops/qb-gate/actions/runs/42810788872)与 [Windows 回归 CI](https://github.com/smithtaylor7748-ops/qb-gate/actions/runs/42810788907)。Mac 验证环境为 Apple Silicon / macOS 15，Rust 3114 项、前端 370 项、IPC 353 个类型、UI 151 个布局与主题场景及 27 个交互流程通过；另有 2 项既有文档示例忽略。原生验证覆盖系统网络采样、隔离 Keychain、受管进程启动与回收、Finder 环境下的 nvm 工具发现。

真实账户登录、首次 Terminal 系统授权和实际 VPN 换网属于使用者环境下的交互验证；以上 CI 使用隔离测试资料。进一步的平台说明与排查信息见 [本版已知问题](https://github.com/smithtaylor7748-ops/qb-gate/blob/macos-v0.25.14-preview.1/docs/KNOWN-ISSUES.zh-CN.md)。

## 从源码构建

在 Apple Silicon Mac 上准备 Xcode Command Line Tools、Node.js 24 和 Rust stable（至少 1.89）。Mac 源码随预览标签提供，在项目目录切换到本版标签后构建：

```sh
xcode-select --install
git checkout macos-v0.25.14-preview.1
npm ci
npm run build:mac
```

构建目标为 `aarch64-apple-darwin`，安装镜像、应用归档、校验清单和许可证输出到 `dist-installers`。可用 `CARGO_TARGET_DIR` 指定 Rust 构建缓存目录。
