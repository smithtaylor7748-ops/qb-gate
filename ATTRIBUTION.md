# 第三方来源与致谢

本项目的检测部分大量参考了已有开源实现。凡是抄了代码的，逐条列在下面；
凡是只用了公开接口协议、没有复制代码的，也单独说明，避免含糊。

---

## 直接改编了代码

### FuckClaude — 中文环境识别十项指纹

- 仓库：https://github.com/LinXiaoTao/FuckClaude
- 许可：**MIT**，Copyright (c) 2026 LinXiaoTao
- 用在：`src/lib/signals.ts`

权重表、评分函数、字体名单、国产浏览器与设备的正则名单基本按原样保留，
只做了本地化注释与类型微调。两处关键判定原样继承，不要在维护中改掉：

- **台湾不计分。** `Asia/Taipei` 与 `zh-TW` 是 Anthropic 完整支持的地区，
  给它们加分属于误报（对应上游 issue #11）。港澳属受限地区，保留部分风险分。
- **繁体字体压在命中阈值以下。** 给 0.2 分，阈值是 0.25，刚好不算命中 ——
  繁体字体在台湾很常见，而且区分不出 TW 与 HK/MO。

MIT 要求保留版权声明与许可声明，已在 `src/lib/signals.ts` 文件头注明。

### cc-switch — 中转站配置的形态

- 仓库：https://github.com/farion1231/cc-switch
- 许可：**MIT**，Copyright (c) Jason Young
- 用在：`crates/qb-relay/src/relay/`（`mod.rs` · `store.rs` · `presets.rs`）

参考的是「多供应商 + 一键切换 + 直接写进 CLI 自己的配置文件」这套产品形态，
以及原子写（临时文件 + 改名）与自动备份的做法。代码为独立实现，未复制。
技术栈选型（Tauri 2 + React + TypeScript）同样是跟着它走的。

具体参考到的几处，逐条列明：

| 参考点 | 说明 |
|---|---|
| 三个应用各一份供应商列表 | 切一个不影响另一个，`RelayTarget` 就是照这个分的 |
| Codex 侧按 `wire_api` 分 responses / chat 两类预设 | 这个分法是对的，照做 |
| 「从当前配置导入」与编辑当前启用项时的回填 | `relay_import_live` |
| Codex 写 `config.toml` + `auth.json` 两个文件的配置形态 | 端点结构参照其公开文档 |

**MIT 允许直接复制源码**（保留版权声明即可）。本项目仍然选择独立实现，
原因是 cc-switch 现在这部分已经和 SQLite DAO、本地代理层缠在一起，
逐行搬进来的维护成本高于重写。

### Cockpit Tools —— ⛔ 只看界面，一行代码都不许抄

- 仓库：https://github.com/jlcodes99/cockpit-tools
- 许可：**CC BY-NC-SA 4.0**（署名-非商业性使用-相同方式共享），
  写在其 README「许可证」一节。**仓库里没有 LICENSE 文件。**
- 用在：`src/pages/Relay.tsx` 的**布局思路**

**这个协议和本项目不兼容，两条都致命：**

- **SA（相同方式共享）**：任何衍生作品必须用同一个协议发布。
  抄它的代码进来，整个 QB Gate 就得从 GPL-3.0 变成 CC BY-NC-SA 4.0 ——
  而两者并不相容，结果是整个项目无法合法发布。
- **NC（非商业）**：其 README 明确禁止「任何未获授权的商业使用
  （含企业内部商业目的、对外商业服务、付费产品集成、二次分发售卖）」。

所以只借鉴了**界面布局思路**（多账号 / 多供应商用「卡片网格 + 标签 + 筛选」
压密度），实现全部自己写。布局思路本身不受版权保护，源码受。

这跟本文件下面对 DNSLeakTester 的处理是同一条线：
**授权不允许就不抄代码**，只按公开信息自己实现。

---

## 界面依赖

前端重做（2026-09-09）引入的两个库，都是 MIT，都作为普通依赖使用，
没有改动其源码：

### Tailwind CSS v4

- 仓库：https://github.com/tailwindlabs/tailwindcss
- 许可：**MIT**，Copyright (c) Tailwind Labs, Inc.
- 用在：`src/styles/`，只用它的工具类与 `@theme` 令牌层

配色**没有用它的调色板**：`src/styles/tokens.css` 里那套暖褐底加赭橙的变量
是本项目原有的，逐行搬过来的，一个 hex 都没改。
Tailwind 在这里只负责生成间距、字号、圆角这些工具类。

### lucide-react

- 仓库：https://github.com/lucide-icons/lucide
- 许可：**ISC**，Copyright (c) Lucide Icons and Contributors
  （项目本身是 Feather Icons 的分支，那部分沿用其 MIT 声明）
- 用在：全部界面图标

替掉了原来用 `▾ ▸ ● ○ ✓ 📁` 这些字符当图标的做法 ——
那些在不同字体下宽度和基线都不一样，而且读屏器会把它们念出来。

---

## 作为独立程序启动，没有链接也没有改源码

### SillyTavern — 酒馆插件

- 仓库：https://github.com/SillyTavern/SillyTavern
- 许可：**AGPL-3.0**
- 用在：`crates/qb-extensions/src/plugins/sillytavern.rs`

**QB Gate 没有复制、修改或链接 SillyTavern 的任何代码。** 插件做的事情是：
以独立进程启动它自带的启动脚本、轮询端口判断就绪、需要时按 PID 停止，
以及读写它的数据目录做备份恢复。这属于「把它当成一个外部程序来用」，
不构成 AGPL 意义上的衍生作品，因此 QB Gate 本身的许可不受其影响
（本项目为 GPL-3.0-or-later；AGPL-3.0 与 GPL-3.0 本身也是兼容的）。

**但有两条线要自己守住：**

- 如果你**改了** SillyTavern 的源码，并且拿它**对外提供网络服务**，
  AGPL 要求你公开修改后的源码。本项目默认部署是本机回环（127.0.0.1:8000），
  只有自己用，不触发这条；一旦你把它暴露到公网就要重新评估。
- 插件里那套启动时序（端口、就绪判定、回滚）是从本机现有的
  `start-claude-pro-rp.ps1` 移植的，那是用户自己的脚本，不涉及第三方许可。

---

## 只用了公开接口，没有复制代码

### Claude 官方 Usage 页面与更新边界

总览的订阅卡片只打开 Claude 官方 `Settings → Usage` 页面，不读取额度、429、限流或 OAuth 内部接口。
QB Gate 自身的 GitHub Releases 更新在仓库创建和签名公钥配置前保持停用，不下载或执行未经签名的包。

### bash.ws — DNS 泄露测试的权威 NS 回显

- 服务：https://bash.ws/dnsleak
- 用在：`crates/qb-probe/src/probe/dns.rs`

用的是 bash.ws 自己对外提供的接口协议：

1. `GET https://bash.ws/id` 取一个测试 id（**必须由服务端签发**，自造的 id
   查回来永远是空清单，会把「没查到」误读成「没泄露」）
2. 依次解析 `1..10.<id>.bash.ws`，权威域名服务器记录是谁来查的
3. `GET https://bash.ws/dnsleak/test/<id>?json` 取回解析器清单

### DNSLeakTester — 参考实现，未采用其代码

- 仓库：https://github.com/ygbull/DNSLeakTester

上面那套 bash.ws 调用时序（探针 10 个、间隔 200ms、等待 3000ms）是照着它的
实现校准的。**但该仓库没有 LICENSE 文件**，只有 README 里的一个 MIT 徽章，
`package.json` 里也没有 license 字段 —— 授权状态不明确。

因此本项目**没有复制它的任何代码**，只实现了 bash.ws 的公开接口协议本身
（接口调用方式属于事实性接口，不构成受保护的表达）。在其许可澄清之前，
维护时也请不要从该仓库复制代码。

### ippure.com — 纯净度与 Claude 环境检测

- 站点：https://ippure.com/ ，`/claude.html`，`/DNS-Leak-Detect.html`
- 公开接口：`https://my.ippure.com/v1/info`（无需 API Key）

面板自测用的是它的公开接口。三项硬指标的字段口径
（IPPure 系数 / IP来源=原生IP / IP属性=住宅IP）也来自该站。
站方注明接口尚在测试阶段、可能变动，所以代码里对缺字段的处理是
**如实报「未知」，不猜成通过**。

### claude-ip-guard — hook 的配置形态与自标记

- 仓库：https://github.com/cso1z/claude-ip-guard
- 许可：MIT

抄的是**形态**，不是判定逻辑：

| 参考点 | 说明 |
|---|---|
| 往 `settings.json` 挂 `SessionStart` + `UserPromptSubmit` | 两个事件缺一不可 —— 只挂前者，会话开着不关、中途换网就没人看了 |
| 用一个自标记（那边是 `_ip_guard`，这里是 `_qb_gate`）认自己写的条目 | 卸载只删自己那几条，绝不整段覆盖使用者的 hooks |
| 「受限国家直接拦截」这个层次 | 本项目做成了国家白名单（见 `gate/judge.rs`），口径相反：那边是黑名单挡受限国家，这里是白名单只放行指定国家 |

**判定本身没有抄。** 那边每次请求现查一次 IP；这里的 hook 脚本里一行判定逻辑
都没有，只读 Rust 落下的 `gate-verdict.json` —— 在 PowerShell 里再写一遍白名单
比对，就是硬约束 10 明令禁止的「在别处再拼一份」。

失败语义也不同：那边检测失败一律放行（不因网络问题阻断），这里是 fail-closed
（判不过拦、查不到拦、裁决过期也拦），由使用者明确选定。

### cc-proxy-detector — 中转站后端判定表

- 仓库：https://github.com/zxc123aa/cc-proxy-detector
- 许可：MIT

原版是 Python，这里是 Rust 重写（`relay/backend.rs`）。抄的是判定思路：

| 参考点 | 说明 |
|---|---|
| 三后端划分：Anthropic / AWS Bedrock / Google Vertex | 所有 Claude 访问最终都落到这三家之一 |
| 响应头指纹（`x-amzn-*`、`x-goog-*`、`anthropic-ratelimit-*`） | 直接证据 |
| **缺字段负证据** | 回的是 Claude 格式却一个 `anthropic-*` 头都没有 —— 转换层做得出响应体，造不出上游本来没有的头。这一招是原版最有价值的部分 |
| 限流头动态验证 | 连发两次看计数动没动，写死的假头不会变 |
| 逆向来源识别（Kiro / Antigravity 等） | 只在有把握时报，没把握就是「说不准」 |

原版还做行为异常分析（`tool_use` 配对错误、间歇 500、多模态读图失败、
写大文件失败），那些要真跑一轮会话才看得出来，不适合放在一个点一下就出结果的
按钮里，**没有搬**。

### check-cc — 出口一致性（v0.11.0 追加）

check-cc 的信号表里有 `browserIpLocation` / `browserIpOrg`：服务端看到的出口
与浏览器侧看到的出口分开列。本项目把这个思路做成了**出口一致性检查**
（`sysenv/checkup.rs::compare_egress`）——

面板测出口时是**绕过系统代理**的（量隧道），走系统代理的程序看到的可能是
另一个出口。两边一比，就能回答那个最难查的问题：「面板说我在美国，
为什么还是被当成国内」。

⚠ 结果**不进门禁判定**。门禁的输入永远只来自绕过代理的那一份 ——
让代理软件决定门禁看到的出口，随便一个本地代理就能伪造它。

**没有采纳的**：check-cc 把语言变体（`languageVariant`）做成独立的加权信号。
本项目的 `scoreLanguages` 早就把 zh-CN / zh-HK·MO / zh-TW 分成三档了
（比它的二元标记还细），再拆一项只会重复逻辑并打乱「满分 100」的权重和。
v0.11.0 只是把这个判定**写到界面上**，让使用者看得见 zh-TW 是被有意不计分的。

### z-switch — 多档模型与 Base URL 推断（v0.11.0 追加）

- 仓库：https://github.com/ZtestAi/z-switch
- 许可：**MIT**（GitHub 的识别器认不出它的 LICENSE，但文件原文是标准 MIT，
  版权归 真测 Ztest）

| 参考点 | 本项目怎么做的 |
|---|---|
| 主模型与小模型分开配 | `ProviderMeta.small_fast_model` → `ANTHROPIC_SMALL_FAST_MODEL`。很多中转站两档不是同一个名字，只配主模型时后台请求会报一个跟你正在做的事无关的错 |
| Base URL 智能推断 | `relay::probe::normalize_base_url`，把粘进来的完整端点归一成 base。归一放在 `upsert` 里而不是表单里 —— 手填、预设、导入三条路都经过它 |

⛔ **不采纳「本地路由模式」**（起 localhost 代理热切换目标）——
内置代理，与 DISCLAIMER 第 107 行冲突。

### Agent-Guard — 只取了检测项的想法（v0.11.0）

- 仓库：https://github.com/dai-chao/Agent-Guard
- 许可：**无**

⛔ 两重限制：**没有 license**（保留全部权利），而且**仓库里根本没有源码** ——
它只是一个落地页（README + safeclaude.net 的下载链接），闭源商业产品
（扫描免费、一键修复 Pro）。

取的是它 README 开篇那句话点出的问题：「`.zshrc` 里还留着 `HTTPS_PROXY` 和
`ANTHROPIC_BASE_URL`」。本项目据此做了**环境变量残留扫描**
（`sysenv/checkup.rs::scan_env`），Windows 侧读 `HKCU\Environment` 与进程环境，
实现从零写起。值的掩码规则是本项目自己的：Key 类只报「已设置」，
URL 类只留 `scheme://host`（路径里可能带 token）。

### check-cc / claude-antiban-macos — 体检项的划分

- https://github.com/yacuo/check-cc（MIT）
- https://github.com/xgq947-ship-it/claude-antiban-macos（MIT）

参考的是「一份完整体检该查哪些项」这个清单（系统代理、IPv6、DoH、
时区与语言一致性、容器痕迹）。Windows 侧的具体读法（注册表路径、
`reg query` 的输出解析）是自己写的，两边的实现语言与平台都不同。

**没有采纳的部分**：这两个项目都包含环境「加固 / 修复」动作。本项目只做
诊断与如实报告，不做设备指纹改写 —— 理由见 DISCLAIMER 第 3 节与第 95 行。

### cac — 版本库与回滚的形态

- 仓库：https://github.com/nmhjklnm/cac
- 许可：MIT

只取了「把历史版本留在一个 `versions/<版本>/` 目录里、可以一键退回去」这个
形态（`install/versions.rs`）。那边是 Shell + npm 包，实现用不上。

**v0.11.0 追加采纳**：第 2 层的遥测关闭思路 —— 但**没有照抄那 12 个变量**。
只设 Claude Code 自己文档里有的四个，外加通用标准 `DO_NOT_TRACK`
（见 `launch::TELEMETRY_OFF`，界面上原样列出供核对）。设一个不存在的变量
等于什么都没关，而界面上却写着「已关闭」—— 那是在说谎。
默认关，并在界面上写死「这跟封号风险无关」：cac 自己的 README 也说了
设备层保护「无法影响账号层风险」。

**没有采纳的部分**：cac 的设备指纹伪装（UUID / 主机名 / MAC 改写）与
强制流量绑定代理。前者与本项目「不为规避封禁而设计」的定位冲突
（DISCLAIMER 第 95、98、101 行），后者会让本项目变成「内置代理功能」，
与 DISCLAIMER 第 107 行直接矛盾。

### clash-claude-fix — DoH / WebRTC 的判定点

- 仓库：https://github.com/jack-peng12/clash-claude-fix
- 许可：MIT

参考了它对 Clash 系客户端 DoH 与 WebRTC 泄露的判定点位。

### 其他看过但未采用的

- MyIP — https://github.com/jason5ng32/MyIP（MIT）。功能全面的 IP 工具箱，
  自建部署友好。本项目没有直接用它的代码，但它的检测项划分值得参考。
- webrtc-privacy — https://github.com/ntblk/webrtc-privacy（MIT）。
  WebRTC 检测的思路与 `signals.ts` 里那段同源（STUN + 收 ICE 候选）。
- **Agent-Guard** — https://github.com/dai-chao/Agent-Guard。检测面与本项目
  重叠最多的一个（代理泄漏、时区指纹、MCP 密钥）。⛔ **仓库没有 license 文件，
  等于保留全部权利** —— 看过，一行代码都没抄，MCP 密钥扫描是自己从零写的。
- **iprisk-top** — https://github.com/iprisk-top/iprisk-top。聚合 16 个数据源的
  IP 纯净度检测。⛔ 同样无 license，没抄。
- **claude-code-ban-risk** — https://github.com/Trentct/claude-code-ban-risk。
  ⛔ 无 license，没抄。
- **clash-claude-fix** — https://github.com/jack-peng12/clash-claude-fix（MIT）。
  内容是两个 Clash Verge / FLClash 的 override 脚本，生成代理规则与 DNS 配置。
  ⛔ **不采纳**：分发代理规则配置与 DISCLAIMER 第 107 行「不提供、不内置、
  不分发任何代理、VPN」直接冲突。它的 DNS / WebRTC 判定点位本项目已经用
  更强的办法覆盖了 —— bash.ws 的真实解析回显是实测，比读配置更可靠。
- **claude-antiban-macos** 的加固动作部分（MIT）。它 8 点清单里的**检测项**
  本项目已全覆盖（时区 / 语言 / WebRTC / IPv6 / 住宅 IP 一致性），
  剩下的是 macOS 专属的**加固**动作 —— 平台不对，而且本项目只做诊断不做加固。
- **awesome-ip-purity** — https://github.com/iprisk-top/awesome-ip-purity。
  ⛔ 无 license。清单的**编排**受版权保护，没有复制；里面提到的公开站点是事实，
  本项目自己挑了 ipinfo 与 Scamalytics 放进 `PurityCriteria.optional`。
- cc-switch / cockpit-tools 那条赛道（账号与供应商切换）本项目不正面做，
  差异在执行锁那一层。cockpit-tools 的许可状况见上面单独那一节。

---

## 关于「Unicode 隐写术」那个说法

ippure 与 FuckClaude 都提到：Claude Code 在 `ANTHROPIC_BASE_URL` 指向中转端点时，
会读取系统时区与中转 hostname，并把结果用 Unicode 隐写术藏进 system prompt
「Today's date」那一行（日期分隔符与四种视觉几乎相同的撇号变体）。

**这是第三方逆向分析主张，本项目未做独立验证，不作为既定事实陈述。**
面板界面上引用时会标注来源与「未证实」。同时注意其触发条件是**走中转端点**；
官方 OAuth 直连路径不在该描述范围内。


## v0.12：工作空间、中转与多类型扩展

以下项目的使用边界分别记录。设计参考不代表整包复制；实际依赖版本以 package-lock.json / Cargo.lock 为准。

| 来源 | 用途 | 许可与固定依据 |
|---|---|---|
| [cc-switch](https://github.com/farion1231/cc-switch) | 配置适配、MCP/Skills 与备份组织方式的参考；QB Gate 独立实现 | MIT |
| [UniGetUI](https://github.com/Devolutions/UniGetUI) | 发现、已安装、更新及任务交互参考 | MIT；未复制应用代码 |
| [Bruno](https://github.com/usebruno/bruno) | 脱敏请求集合导出思路与 `.bru` 文件格式 | MIT；未打包 Bruno |
| [MCP Registry](https://github.com/modelcontextprotocol/registry) | 元数据、来源与版本字段参考 | 未复制源码，不把收录视为安全背书 |
| [Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk) | 直接依赖客户端初始化、stdio/HTTP 传输与能力检查 | rmcp **3.3.0**，crate 声明 Apache-2.0；固定依赖许可，不假定整个生态统一许可 |
| [Agent Skills](https://github.com/agentskills/agentskills) | SKILL.md 元数据约束和来源固定设计 | 仓库双许可：代码 Apache-2.0，**文档 CC-BY-4.0**。本项目参考的是规范文档那一半，按 CC-BY-4.0 署名，未复制代码。核查版本 `69ef37e9424c0a7ea9dd2293b559e43ec8176379` |
| [Anthropic Skills](https://github.com/anthropics/skills) | 精选目录收录 skill-creator 与 webapp-testing，用户安装时才下载 | 两个目录各自 Apache-2.0；固定 `34040c9c568585f6929bedeaad110ad08f079624`。不将整个仓库视为同一许可 |
| [MCP Servers](https://github.com/modelcontextprotocol/servers) | 精选 Filesystem 与 Git MCP 运行配置 | 仓库核查 `d73f99efbfd40c3aa1b61e88728b3d49fb52608f`；Filesystem npm **2026.8.31**，Git PyPI **2026.8.18**（包声明 MIT）；上游有许可迁移，实际安装版本分别核对 |
| [Tauri plugins](https://github.com/tauri-apps/plugins-workspace) | 直接使用单实例、对话框、链接打开插件 | MIT OR Apache-2.0；实际版本在 Cargo.lock |
| React Router / TanStack Query | 前端路由和服务端状态管理 | MIT；实际版本在 package-lock.json |
| rusqlite / SQLite / ts-rs | 本地元数据、事务、类型导出 | 分别按锁文件依赖声明；完整文本见 THIRD_PARTY_NOTICES.txt |

这里写死的提交哈希是**代码强制的**：面板的「检查来源更新」只会报告上游有新提交，
不会改掉精选目录里这两个固定版本（见 `extensions::check_updates`）。需要新版本的人
自己从来源手动导入，导入进来的是一条独立记录，不影响这张表里核对过的那一版。

## 评估过但没有采用

### 订阅指引（Subscription Guide）— MIT，已移除

v0.12.0 的重构里带进来一份 3,310 行的订阅指引组件（`src/subscription-guide/`：
购买步骤、常见问题、视频与来源清单，附 MIT LICENSE）。**它从头到尾没有被挂上过**
—— 全树零个引用点，v0.11.0 里也不存在这个目录。

没有采用它，原因是这一版重排的主线恰恰是删掉「无用的介绍」：一份
750 行的购买教程正是使用者点名嫌多的那类内容，而且价格与步骤会过时、
没人维护就会从帮助变成误导。代码已从工作区移除，要找的话在 git 历史里。
按 MIT 的要求，这里保留出处记录。

SillyTavern 是外部 AGPL-3.0 应用。QB Gate 接入用户已有安装，不捆绑应用本体；本项目仍按 GPL-3.0-or-later 发布。

[docs/dependencies.json](docs/dependencies.json) 记录依赖名称、版本、来源和许可声明；[THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt) 包含可获得的原始版权及许可文本，并随安装包提供。缺少随包许可文件时的补充文本固定到上游提交，记录于 docs/license-supplements.json。构建和测试依赖也一并列出。
