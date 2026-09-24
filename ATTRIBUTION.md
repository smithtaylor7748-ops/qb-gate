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
- 用在：`crates/qb-relay/src/relay/`（`mod.rs` · `store.rs` · `presets.rs`），
  以及 v0.17.0 起的 `src/features/station/`（`RouteDialog.tsx` · `ConfigPane.tsx`）

参考的是「多供应商 + 一键切换 + 直接写进 CLI 自己的配置文件」这套产品形态，
以及原子写（临时文件 + 改名）与自动备份的做法。代码为独立实现，未复制。
技术栈选型（Tauri 2 + React + TypeScript）同样是跟着它走的。

具体参考到的几处，逐条列明：

| 参考点                                                  | 说明                                                                                                                                                                                                                                                                           |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 三个应用各一份供应商列表                                | 切一个不影响另一个，`RelayTarget` 就是照这个分的                                                                                                                                                                                                                               |
| Codex 侧按 `wire_api` 分 responses / chat 两类预设      | 这个分法是对的，照做                                                                                                                                                                                                                                                           |
| 「从当前配置导入」与编辑当前启用项时的回填              | `relay_import_live`                                                                                                                                                                                                                                                            |
| Codex 写 `config.toml` + `auth.json` 两个文件的配置形态 | 端点结构参照其公开文档                                                                                                                                                                                                                                                         |
| **复合主键分区**（应用 + 供应商各一条）                 | `Route::make_id` 从两段变三段：`软件 + U+001F + 站点 + U+001F + 分组`。v0.16.0 之前少了软件那一段，在 Codex 底下加的线路会把 Claude Code 同名那条原地覆盖                                                                                                                      |
| **per-app 当前上游**                                    | `ClientRouter` 按 `Client` 分别记 current / pending。共用一份的话，在 Codex 里切上游会把 Claude Code 的一起切掉                                                                                                                                                                |
| **`ProviderForm` 的表单形态**                           | 预设胶囊 + 72px 大图标 + 两列基本信息 +「接入」分节 + 收起的高级选项。见 `RouteDialog.tsx`                                                                                                                                                                                     |
| **六个快捷开关的键名与判定条件**                        | 逐条对着 `src/components/providers/forms/CommonConfigEditor.tsx:72-170` 核过，见下面那张表。**键名照抄** —— 自己编一套的话，写进去的东西 Claude Code 根本不认，而界面上看起来一切正常                                                                                          |
| **模型映射的键集**                                      | `src/components/providers/forms/hooks/useModelState.ts` 的 `ClaudeModelEnvField`：主模型 + Opus/Sonnet/Fable/Haiku 四档（各带一个 `_NAME` 显示名）+ `CLAUDE_CODE_SUBAGENT_MODEL`                                                                                               |
| **`ANTHROPIC_SMALL_FAST_MODEL` 是旧键**                 | 同上文件里每次写模型都 `delete env.ANTHROPIC_SMALL_FAST_MODEL`。我们照做 —— 两个键都留着的话，客户端读哪个取决于它自己的优先级，而那个优先级我们看不见                                                                                                                         |
| **1M 上下文用模型名后缀，不是环境变量**                 | `CLAUDE_ONE_M_MARKER = "[1M]"`，读时大小写不敏感。当初查不到「1M」对应的变量名是因为根本没有那个变量                                                                                                                                                                           |
| **Codex 的 1M 与思考等级**                              | `model_context_window = 1000000` 必须跟 `model_auto_compact_token_limit = 900000` 一起设（`CodexConfigSections.tsx`），`model_reasoning_effort` 取 minimal/low/medium/high。两项都只认第一个 `[section]` 之前那一段（`utils/providerConfigUtils.ts` 的 `getTopLevelEndIndex`） |
| **取消勾选时删键、`env` 空了连 `env` 一起删**           | `ConfigPane.tsx` 的 `setEnv`。留一个空的 `env: {}` 不影响运行，但会让「跟随表单」算出来的 JSON 跟手写的那份永远不相等                                                                                                                                                          |

六个开关的**判定条件**也照抄，而不是只抄键名 —— 这一条比键名更容易漏：

| 开关               | 写什么                                           | 读的时候什么才算「开着」                                                                                                   |
| ------------------ | ------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| 隐藏 AI 署名       | `attribution = {commit:"", pr:""}`               | **两项都是空串**。只判「`attribution` 在不在」的话，一个真的配了署名模板的人会显示成「已隐藏」，取消勾选把他的模板整个删掉 |
| Teammates 模式     | `env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS = "1"` | `"1"` 或数字 `1`                                                                                                           |
| 启用 Tool Search   | `env.ENABLE_TOOL_SEARCH = "true"`                | `"true"` 或 `"1"`                                                                                                          |
| 最大强度思考       | `env.CLAUDE_CODE_EFFORT_LEVEL = "max"`           | **严格等于 `"max"`**。设成 `medium` 的人不该看到这个勾是开的，否则他一取消勾选就把自己的设置删掉了                         |
| 禁用自动升级       | `env.DISABLE_AUTOUPDATER = "1"`                  | `"1"` 或数字 `1`                                                                                                           |
| 禁用 Artifact 工具 | `env.CLAUDE_CODE_DISABLE_ARTIFACT = "1"`         | `"1"` 或数字 `1`                                                                                                           |

取消勾选一律删键，**`env` 空了连 `env` 一起删** —— 留一个空的 `env: {}`
不影响客户端运行，但会让「跟随表单」算出来的 JSON 跟手写的那份永远不相等。

判定条件与 `[1M]` 标记的往返由 `src/lib/clientConfig.test.ts` 钉着（24 条）。
「禁用 Artifact」那句说明（第三方网关用严格 JSON Schema 校验工具定义，
Artifact 里的 `\p{..}` 正则会让每个请求 400）也来自 cc-switch 源码里的注释。

**路径前缀区分客户端**（`/cd` 前缀认 Claude 桌面端）是本项目自己的做法，
不是从 cc-switch 来的 —— 它没有本机路由这一层。这一条记在这里是因为
「没抄什么」同样要写明：Claude Code 与桌面端走同一套 Anthropic 协议，
协议上分不开，而 cc-switch 直接改客户端配置、不需要在运行期分辨来源。

**MIT 允许直接复制源码**（保留版权声明即可）。本项目仍然选择独立实现，
原因是 cc-switch 现在这部分已经和 SQLite DAO、本地代理层缠在一起，
逐行搬进来的维护成本高于重写。

**2026-09-24 追加：用量统计。** 使用者要「ccswitch 里显示的数据本面板也要有」，对着它的会话用量导入与
用量面板核了一遍。取的是做法，代码仍是自己写的（Rust 在 `qb-accounts::accounts::tokens`、
`qb-app::usecase::token_summary`，界面在 `src/features/Usage.tsx`）：

| 参考点                                             | 本项目怎么做的                                                                                                     |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| 多扫一层 `subagents/workflows/<wf>/*.jsonl`        | `transcripts()` 多扫这一层，归父会话                                                                               |
| 同一个 `message.id:requestId` 留最完整的一份       | 有用量 > 带 `stop_reason` > 输出大 > 时刻早（原来留先见到的那份，子代理的 `message_start` 快照会被留下）           |
| 任一计费维度 > 0 才入账                            | 四类全 0 的（`<synthetic>` 报错）不算回复、不入账，另外单独数出来显示 —— 这一步是本项目加的，不瞒                 |
| Codex 的模型取会话里最近一条 `turn_context`        | `qb-accounts::codex::usage` 把正差分记到那个模型上                                                                  |
| 用量面板的信息量（等价费用、按模型、请求日志）     | `/usage` 的概览、按模型、最近请求三块；图表是自己手写的 SVG，没有用它的图表库                                     |

### cockpit-tools —— 桌面端怎么接进中转（v0.18.0 追加）

- 仓库：https://github.com/jlcodes99/cockpit-tools
- 许可复核（2026-09-18）：根目录未提供 LICENSE，当前 README 声明
  **CC BY-NC-SA 4.0**。本项目仅参考功能与公开协议，未复制其源码，
  不将该许可下的代码并入现有双授权发行。
- 读了：`src-tauri/src/modules/claude_desktop_gateway.rs`（561 行）与
  `claude_account_desktop_profile.rs` 里写配置那几段。

**参照的是「Claude 桌面端有哪个官方机制可以用」这一条信息，不是它的实现。**
它让我知道该去桌面端的 `app.asar` 里找什么；找到之后，下面每一条都是
**在桌面端自己的 bundle 里核实过的**（1.52386.6），代码是我们自己写的：

| 从它那里知道的                                     | 我们怎么核实的                                                                                                                                                                                   |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 桌面端有个 `deploymentMode: "3p"` 的第三方网关模式 | `deploymentMode` / `inferenceProvider` / `inferenceGatewayBaseUrl` / `inferenceGatewayApiKey` / `inferenceGatewayAuthScheme` / `inferenceModels` / `supports1m` 七个键在 `app.asar` 里全部找得到 |
| 配置写在 `claude_desktop_config.json`              | 同上；这个文件同时装 `mcpServers`，所以只能并入                                                                                                                                                  |

**它没告诉我们、我们自己找到的**：`CLAUDE_USER_DATA_DIR` 这个环境变量。
cockpit 走的是「让桌面端自己算 3p 目录」那条路（它观察到的目录名是
`Claude-3p`，而那个字符串在 bundle 里根本不存在）；我们直接用桌面端主进程
启动时第一件事读的那个变量，优先级更高、目录由面板说了算，
跟 Claude Code 的 `CLAUDE_CONFIG_DIR` 是同一个形状。

**明确没抄的**：

- 它那个**每个账号一个本地 HTTP 网关**（`tiny_http` 监听随机端口，转发时做
  模型改名）。我们已经有本机路由了，而且是**一个端口服务三个软件、靠路径
  前缀区分**（`/cd`）—— 再按账号起一堆监听是另一套架构，不是这里缺的东西；
- 它写的 `disableDeploymentModeChooser` 与 `coworkEgressAllowedHosts: ["*"]`。
  前者从使用者手里拿走一个开关，后者放宽一道安全限制 —— 见 CLAUDE.md 那一节；
- 界面排法。cockpit 的布局本项目在 0.14.0 就参考过（`qb-relay/src/relay/mod.rs`
  的模块头写着），同样**一行代码都没有复制**。

### station-monitor-standalone —— 计费核验的算法思路（v0.16.0 追加）

- 来源：**使用者本人的项目**（从「账号工具箱」拆出来的独立版中转站监控）。
  不是第三方开源件，没有许可问题；记在这里是因为硬约束
  「抄了什么、没抄什么、为什么没抄，都要写进 ATTRIBUTION.md」。
- 用在：`qb-station::station::pricing::measured_multiplier` 与
  `qb-station::station::audit::EvidenceLevel`。

**参照了什么**

| 它那边                                                                                                                       | 我们这边                                                 |
| ---------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| `official_cost` / `original_cost` / `actual_cost` **三个数分开算**，`base_price_ratio` 与 `observed_multiplier` 各答各的问题 | `measured_multiplier` —— 实扣 ÷ Σ(实际 token × 官方单价) |
| 单价**从实扣反推**（`recorded_cost × 1e6 ÷ tokens`），不听站点自己报                                                         | 分子取账单实扣、分母取官方价，两头都不是站点公布的倍率   |
| `evidence_level` 四档 `sufficient` / `partial` / `insufficient` / `conflict`                                                 | `EvidenceLevel` 同样四档、同样命名                       |
| `CACHE_ATTENTION_THRESHOLD = 0.80`                                                                                           | `audit::CACHE_ATTENTION_THRESHOLD`                       |
| `PRICE_STALE_AFTER_DAYS = 180`                                                                                               | `pricing::STALE_AFTER_DAYS`                              |

**后续适配范围**

- **0.23.1 多轮缓存与余额核对**：按用户提供的源码适配六次固定前缀请求、可选第七次冷前缀、后台账号密码与浏览器会话、账单匹配和余额差额。测试材料/追问及 CSV/JSON 导入 Worker 来自用户提供的源码；Rust 编排和 React 界面适配本项目。缺少相同请求 ID 的 Token 匹配显式降级，不进入实扣倍率回写。

**计费编排独立适配**：那边是 Python + `Decimal`，这边是 Rust + `f64`，
并且加了一条那边没有的硬规矩 —— **四类 token 缺任何一类就返回 `None`**，
因为少一类会把分母算小、倍率被系统性抬高，而那正好是「这家在超收」的方向。

### 0.23.1：中转检验与 Windows 初始化排障

- 使用者提供的 `station-monitor-standalone.zip`：参考 `backend/server.py` 的
  New API 账号密码/Cookie/访问令牌与 `New-Api-User`、`quota_per_unit`，Sub2API `/api/v1/usage`
  与 `actual_cost`，以及 Responses SSE 和账单请求 ID 配对。Rust/React 实现
  位于 `station_billing.rs`、`station_probe.rs`、`station_ops.rs` 与 `StationBilling.tsx`。
- [cockpit-tools](https://github.com/jlcodes99/cockpit-tools)：继续参考独立实例与
  进程诊断思路，实际故障依据本机官方 Codex 日志、协议和上游实现定位，未复制代码。
- [OpenAI Codex Windows 指引](https://developers.openai.com/codex/windows/)、
  [官方沙箱初始化实现](https://github.com/openai/codex/blob/main/codex-rs/windows-sandbox-rs/src/setup_provisioning.rs)：
  核对旧目录缺少 `WRITE_DAC` 的原因；本机修复由已安装的官方初始化器执行，
  QB Gate 仅新增诊断展示，未复制官方沙箱代码。
- `ccodex-sleep-state-main.zip`（Go / GPL-3.0 / Mihomo）：**0.23.1 时仅评估未融合**；
  **0.24.0 起 clean-room 重写了它的 turn-state 机制与 SSE 终止事件分类**（见下「ccodex-sleep-state」一节）。
  始终**没有**复制其源码、二进制或引入 Mihomo / 代理出口。旧评估见
  [融合与修复说明](docs/RELAY-REPAIR-0.23.1.zh-CN.md)。

---

### Cockpit Tools —— ⛔ 只看界面，一行代码都不许抄

- 仓库：https://github.com/jlcodes99/cockpit-tools
- 许可：**CC BY-NC-SA 4.0**（署名-非商业性使用-相同方式共享），
  写在其 README「许可证」一节。**仓库里没有 LICENSE 文件。**
- 用在：`src/pages/Relay.tsx` 的**布局思路**

**这个协议和本项目不兼容，两条都致命：**

- **SA（相同方式共享）**：任何衍生作品必须用同一个协议发布。
  抄它的代码进来，整个 QB Gate 就得从 AGPL-3.0 变成 CC BY-NC-SA 4.0 ——
  而两者并不相容，结果是整个项目无法合法发布。
- **NC（非商业）**：其 README 明确禁止「任何未获授权的商业使用
  （含企业内部商业目的、对外商业服务、付费产品集成、二次分发售卖）」。
  本项目已经开了商业授权那一档（见 [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md)），
  NC 与之正面冲突 —— 这一条现在比以前更碰不得。

所以只借鉴了**界面布局思路**（多账号 / 多供应商用「卡片网格 + 标签 + 筛选」
压密度），实现全部自己写。布局思路本身不受版权保护，源码受。

2026-09-22 起的 GPT / Gemini 额度卡也只借鉴其公开数据模型：把服务端返回的窗口或模型桶
归一化为「剩余百分比 + 重置时间 + 数据来源」，再由前端画进度条；实现代码是本项目自行编写。
GPT 使用 ChatGPT `wham/usage`，Gemini 使用 Code Assist `loadCodeAssist` / `retrieveUserQuota`，
均只在使用者点刷新图标时查询（2026-09-23 起；此前一版会在打开页面时、并每 5 分钟对所有槽位各查一次），
不做自动换号或路由决策。

**2026-09-23 反重力账户的联网额度** —— 使用者要「参考 cockpit-tools 把它联网能拿到的条目都做出来」
（Claude / Gemini 两组 × 5 小时 / 每周、可用 AI 积分）。这一次**读了它的源码**，但只为核对**接口事实**：
端点名（`loadCodeAssist` / `retrieveUserQuotaSummary` / `fetchAvailableModels`）、四个 `bucketId`
（`gemini-5h` / `gemini-weekly` / `3p-5h` / `3p-weekly`）、积分在 `paidTier.availableCredits`、
免费档查汇总会被拒（403）、`aicode-consumers` 当作没有 project。这些是 Google 接口的行为事实，
不受版权保护；**实现（`crates/qb-app/src/usecase/antigravity_quota.rs` 与界面）一行没抄**，
解析器、配对、缓存、界面全是自己写的。两件事**故意没照它做**：它在没有 project 时会发
`onboardUser`（会改账户状态），本项目不发；它把 Google 发给反重力的 OAuth 客户端标识硬编码进
自己程序、自己跑登录、自己存令牌，本项目不跑登录、不存令牌，只在点刷新的那一刻读官方客户端
存在本机的那一份，过期了用同一份里的刷新令牌在内存里换新，换新要带的客户端标识也是那一刻从本机
装的反重力里读出来的（仓库里不出现）。

**2026-09-23 GPT / Gemini 的登录态** —— 使用者：「GPT 和 Gemini 的登录态对着那个开源项目修一下，
因为 GPT 有时候突然弹出第二个使用页面」。这一次读的是它 Codex 账户与多开的几处函数体
（`src-tauri/src/modules/codex_account_token_refresh.rs`、`codex_account_check.rs`、`codex_instance.rs`、
`process_codex_runtime.rs`、`process_path_resolution.rs` 里起 Codex 实例的那一段、`codex_temp_login.rs` 的文件头），
同样**只记行为事实、一行没抄**：

- 额度查询只依赖访问令牌；**只在访问令牌真过期时才换新**，不因为 id_token 过期或按周期保活去换 ——
  多换一次就多轮换一次 OpenAI 的刷新令牌；
- **官方客户端正在用这个账户时，外部程序不替它换新**（它把这叫「刷新令牌归官方客户端」）；
- 刷新令牌作废（`refresh_token_reused` / `refresh_token_expired` / `invalid_grant`）归「要重新登录」，
  访问令牌被服务端拒绝（401、`token_invalidated`）另算一类，跟网络、限流分开；
- Codex 桌面端的多开：每个实例一份 `CODEX_HOME` + 一份 `--user-data-dir`，**按实例的资料目录认自己那份、
  只关自己那份**；Electron 的单实例锁按资料目录算，默认实例挡不住别的资料目录。

落到本项目（`crates/qb-app/src/usecase/{google_oauth,login_health,tavern_quota,codex_accounts}.rs`、
`crates/qb-install/src/install/codex_desktop.rs`，都是自己写的）：Gemini CLI 的访问令牌过期时在内存里换新
（Google 的刷新令牌换新之后不作废，跟反重力同一套，客户端标识从本机装的 `@google/gemini-cli` 包里现读）；
GPT **不换**（OpenAI 的刷新令牌会轮换，面板一换桌面端就被登出），到点了如实说；服务端真的不认了才在账户行上
显示「登录已失效」；起 / 切 GPT 槽位之前只关面板自己起的 Codex 实例。**没照它做的**：它的「临时登录」
自己保管令牌、把账户注入各实例的 `auth.json`，本项目的 GPT 槽位仍然是官方客户端在自己的目录里登录、
自己保管令牌，面板一个字不写。

这跟本文件下面对 DNSLeakTester 的处理是同一条线：
**授权不允许就不抄代码**，只按公开信息自己实现。

---

### ccodex-sleep-state —— turn-state 机制,clean-room 重写（v0.24.0）

- 仓库：https://github.com/gylive/ccodex-sleep-state
- 许可：**GPL-3.0**，且依赖 **Mihomo（GPL-3.0）**。
- 用在：官方 Codex 的 `X-Codex-Turn-State` 采集 / 注入
  （`crates/qb-station/src/turnstate.rs`），以及 SSE 终止事件分类
  （`crates/qb-station/src/sse.rs`），二者都接进 `crates/qb-app/src/local_router.rs`。

**⛔ 一行源码都没抄,只按公开描述用 Rust 重写。** 理由是许可：本项目是
**AGPL-3.0-only + 商业授权双授权**，而 ccodex 是第三方持有版权的 GPL 代码 ——
按 [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md) 第三节,抄进来的 copyleft 代码
无法再许可给商业档,会把商业档堵死。所以跟对 cc-switch(MIT，本可直接抄却仍独立实现)
一样,这里也是 clean-room。

**参照了什么（都是公开协议 / 行为,不受版权保护）：**

- turn-state 信封的**外形**判定：base64url 解开后头字节 `0x80`、8 字节大端签发时间、
  按 16 字节一块;个人 10 块 ≈ 292 字符 / Team 12 块 ≈ 332 字符。**只读外形,不解密不验签。**
- active / ready 状态机的行为口径：响应头只做观测、不直接改 active;合格候选先进 ready;
  连续两次形状变了或临近续补窗口才晋升。
- 官方 Codex SSE 的终止事件归类：`response.completed` = 成功;`response.failed` / `error` = 失败;
  `server_is_overloaded` / `slow_down` = 容量（≠429）;`rate_limit_exceeded` / `insufficient_quota` = 限流。

**明确没有参照 / 没有做的：**

- **没有复制它的 Go 源码**（`turnstate/`、`gateway/`、`codexconfig/`、`service/` 等一概没抄）。
- **没有引入 Mihomo、订阅、出站协议适配或任何代理出口池** —— 那既是 GPL 传染源,
  又与 DISCLAIMER §5「不做内置代理」冲突。
- **没有做「换出口凑 292」**：本项目没有代理出口池,只在使用者当前这一条连接上采集。
- **没有做主动合成探测**去烧额度:改为**被动**采集（读官方响应本就带回的头），
  与本项目「不拿合成请求烧额度」的一贯纪律一致。
- turn-state 只对 **Codex** 有意义（Anthropic Claude 协议里没有对应物），**只对 `Client::Codex` 生效**。

长度只是经验筛选口径,**不是模型质量或额度指标**（ccodex 自己的 README / SECURITY 也这么说）——
界面上照此措辞,不承诺任何服务端结果。详见 [DISCLAIMER.md](DISCLAIMER.md) 关于 turn-state 的一节。

### EasyAntigravity —— 反重力的汉化、自动审批、高危拦截（v0.26.0）

- 来源：https://github.com/DSDS-CMHL/EasyAntigravity（v1.1.4，作者 Astwarp）
- 许可：**MIT**（仓库根 `LICENSE`，全文随本项目一起放在
  `crates/qb-extensions/assets/antigravity/LICENSE.EasyAntigravity`）
- **抄了什么（逐字带入，MIT 允许，版权声明保留在文件头）**：
  - `server.js` 里 `generateMasterInjectScript()` 返回的那段页面脚本 →
    `crates/qb-extensions/assets/antigravity/inject.js`。只把它六处运行时插值换成占位符，
    由 Rust 侧（`plugins::antigravity_ui::assemble_script`）在注入前替换；
  - 汉化字典 `dicts/ui_v2.json`、`dicts/common.json` → 同目录 `dicts/`，合并顺序照它
    （`ui_v2` 先、`common` 后覆盖）；
  - 高危规则 `backup/danger-rules.json` → 同目录 `danger-rules.json`，作为出厂规则，
    第一次启动复制到状态目录让使用者改。
- **按它的机制自己写的（Rust）**：读 `DevToolsActivePort` 取端口（它写死 9333）、
  `GET /json/list`、每页一条 WebSocket、`Runtime.enable` / `Page.enable` / `Runtime.evaluate`、
  三个事件 + 2 s 心跳重注入、`Runtime.consoleAPICalled` 回收 `[EA_AA]` / `[EA_ALERT]` / `[EA_OPT]`、
  连续 5 次失联自动停 —— `crates/qb-extensions/src/plugins/antigravity_ui.rs`。
- **没抄什么、为什么**：
  - **`backup/version.dll`（561 KB）与 `backup/config.json`** —— 它的「免 TUN 代理」：把这份 DLL
    丢进 `%LOCALAPPDATA%\Programs\antigravity\`，靠 DLL 搜索顺序注入 `Antigravity.exe`，
    再注入子进程 `language_server.exe` / `node.exe`，fake-IP DNS，全部流量转到本机 SOCKS5；
    客户端更新抹掉后由它的看门狗回写。`config.json` 里写着 `"_proxy_core": "based-on-antigravity-proxy"`
    —— **这份 DLL 不是 EasyAntigravity 自己的代码，来源与许可不明**，MIT 那份 LICENSE 管不到它。
    撞了本项目两条铁律：「没有 license 一行都不能抄」「不改官方二进制」。使用者 2026-09-20
    决定**不做代理功能**，本项目也不提供替代实现。
  - 它的 Node GUI（`index.html` / 19823 端口）—— 没必要，面板自己有界面。
  - 它的 `/api/launch`（裸 `spawn Antigravity.exe`）—— 起反重力走本项目自己的门禁链。

### codex-windows-cn —— Codex 桌面端不开 Store 直装：FE3 信封与匿名票据（v0.28.0）

- 来源：https://github.com/chrichuang218/codex-windows-cn（作者 vaportail；它自己写明改自
  StoreDev/StoreLib，同为 MIT）
- 许可：**MIT**（仓库根 `LICENSE`，Copyright (c) 2026 vaportail）。版权与来源写在
  `crates/qb-install/src/install/codex_store.rs` 文件头。
- **抄了什么（MIT 允许，改了占位符）**：三份 Windows Update 客户端协议的 SOAP 信封
  `src/store/templates/{GetCookie,WUIDRequest,FE3FileUrl}.xml` →
  `crates/qb-install/src/install/codex_store/{get-cookie,sync-updates,file-url}.xml`。
  改动：时间戳、cookie、类别 id、更新 id、票据、**架构**（它写死 `AMD64`，这里按本机
  x64 / arm64 替换）都换成命名占位符。它嵌在 `direct.rs` 里的**匿名 MSA 设备票据**
  （StoreLib 系工具多年共用的同一个值，免费应用不登录也能取下载地址）也原样带入。
- **按它的机制自己写的（Rust，`codex_store.rs`）**：DisplayCatalog 取 `WuCategoryId` →
  `GetCookie` → `SyncUpdates`（按 moniker 挑本机架构的最高版；「第一个 `UpdateIdentity`
  才是自己的身份、后面的是前置依赖」这条规则照它）→ `GetExtendedUpdateInfo2` 走 `/secured`。
  解析器是自己写的字符串扫描（它用 quick-xml），另加了它没有的三样：**按文件摘要对下载地址**
  （它取「最长的那条」，这里先按 `FileDigest` 对号入座，对不上才退回最长）、**清单里的
  SHA-256 核对**（它不核哈希）、**下载域限定** `delivery.mp.microsoft.com`。
- **没抄什么、为什么**：
  - **解压 `app/` 子树、未打包直接跑 `Codex.exe`**（它的 `extract.rs` / `proxy.rs`）与
    **`versions/` + `current` junction 多版本并存**（`versions.rs` / `junction.rs`）—— 会丢包身份
    （`codex://` 协议、Store 自动更新、Apps & Features 登记；codex-app-mirror 自己的
    `windows-portable-experiment.md` 把这条路标为 experimental / unsupported）。面板启动的本来
    就是正规注册的那份，所以直装的终点是 `Add-AppxPackage`，不是解压；
  - 它的启动器自更新、快捷方式、Slint/Tauri 界面 —— 与本项目无关；
  - 它 `winget.rs` 用的是 `winget download --source msstore`（拿 `.msix` 文件再自己解压）；
    本项目用 `winget install --source msstore`（让 Store 部署服务自己装），只是同一条命令的
    另一个子命令，不算抄。

### codex-app-mirror —— 只取了事实，没有用它的镜像（v0.28.0）

- 来源：https://github.com/Wangnov/codex-app-mirror（作者 Wangnov）
- 许可：**MIT**（仓库根 `LICENSE`）。
- **用了什么**：三条事实 —— Store product id `9PLM9XGG6VKS`、包族名
  `OpenAI.Codex_2p2nqsd0c76g0`（跟面板 `codex_desktop.rs` 里早就在用的一致）、以及
  `docs/chatgpt-rebrand-recovery.md` 记的「2026-07 显示名改成 ChatGPT、exe 从 `Codex.exe`
  变 `ChatGPT.exe`、**product id 没变**」—— 这决定了 `PRODUCT_ID` 不跟显示名走。
  它的 `scripts/store-link/Program.cs`（C#）跟上面 codex-windows-cn 的 `direct.rs` 是同一套
  FE3 流程，读它是为了对照两份实现的一致点（moniker 格式、x64/arm64 在设备属性里的写法、
  只认 `dl.delivery.mp.microsoft.com`），**没有从 C# 里搬任何东西**。
- **没用什么、为什么**：它的镜像基建（Cloudflare Worker / R2 / S3、`codexapp.agentsmirror.com`
  短链、GitHub Releases 上的 MSIX 副本）一概不用 —— 面板直接问微软的目录与分发接口，
  不依赖任何第三方镜像；CLAUDE.md 也写着「不在仓库分发官方安装包本体」。它的 macOS / Linux
  部分与本项目无关。

### Microsoft Root Certificate Authority 2011 —— 内置一张微软的根证书（v0.28.0）

- FE3（`fe3.delivery.mp.microsoft.com`）的证书链到 **Microsoft Root Certificate Authority 2011**：
  这个根在每台 Windows 的系统证书库里（Windows Update 自己靠它），但**不在 Mozilla 的根清单里**，
  而面板的 reqwest 走 rustls + webpki 根 —— 不带上它，`GetCookie` 那一步会被当成未知 CA 拒掉
  （2026-09-20 实机撞上）。
- 文件：`crates/qb-install/src/install/codex_store/microsoft-root-2011.pem`。来源两处对得上
  （字节相同）：`https://www.microsoft.com/pki/certs/MicRooCerAut2011_2011_03_22.crt` 与本机
  `Cert:\LocalMachine\Root`；SHA-256 指纹
  `847df6a78497943f27fc72eb93f9a637320a02b561d0a91b09e87a7807ed7c61`，有单测钉着；有效期到
  2036-03-22。**只加给 `codex_store` 自己的客户端**，面板别的网络客户端不受影响。
  根证书是公开的信任锚，不是受版权保护的代码。

### Google Antigravity —— 只用了它自己开着的公开接口，没有改它一个字（v0.26.0）

- Hub（`Programs\antigravity\Antigravity.exe`）的 `main.js` 没传 `--remote-debugging-port` 时
  自己补 `0`，端口写进 `%APPDATA%\Antigravity\DevToolsActivePort` —— 调试端口是它自己开的。
  本项目读那个文件、连 Chrome DevTools 协议（公开协议）注入脚本；不传任何参数、不改它的
  `app.asar`、不落任何文件到它的安装目录。
- 「自动检查更新」是它自己的设置键（`app_storage.json` 里的 `autoCheckForUpdates`，
  `dist/services/settingsService.js`），面板只并入这一个键。
- 2026-09-20 读过它的 `app.asar`（v2.15.0，`dist/languageServer.js` / `paths.js` / `updater.js`）
  以确认：API 端点写死在 `--api_server_url` / `--cloud_code_endpoint`，登录由语言服务器做，
  令牌在 Windows 凭据管理器（二进制里 `CredRead/CredWrite` 43 处）。**这些只用来判断能做什么、
  不能做什么**（没有中转路径、没有多槽位），没有据此调用任何未公开接口。
- **一键安装（v0.29.0）不分发、不托管、不镜像 Google 的任何文件。** 面板读它的公开下载页
  `https://antigravity.google/download`（跟使用者用浏览器打开的是同一个页面）取当前版本与
  下载地址，或者调 `winget` 装 microsoft/winget-pkgs 上的官方清单
  （`Google.Antigravity` / `Google.AntigravityIDE`，清单由 Google 维护）。下载域限定在
  Google 自己的三个分发主机上，运行安装器前核对 Authenticode 签名主体含 Google。
  **装出来的就是使用者自己去官网点下载得到的那一个文件**，面板不改包、不重新打包、
  不做二次分发。安装器的静默开关（NSIS `/S`、Inno `/VERYSILENT …`）是这两种安装器的
  公开命令行约定，不是从谁那里抄来的。
  - 参考过 `rogerogers/antigravity-updater`（MIT，bash）确认「官方下载页 + GCS 桶」是
    社区公认的取版路径；**一行代码都没抄**（它是 shell 脚本，本项目是 Rust，
    解析器与域白名单都是自写的，有单测）。
  - ⛔ `lbjlaq/Antigravity-Manager` **看都不能照着写**：它是 **CC BY-NC-SA 4.0**
    （README 明写「严禁任何形式的商业行为」），SA 会把本项目拖成同一个协议、NC 禁止商业使用，
    跟 `jlcodes/cockpit-tools` 同一档。它做的也不是安装，是多账号代理。
- **IDE 槽位、账户状态与配额、本机用量（v0.30.0）—— 仍然只用它自己写在本机的东西。**
  - IDE 是 VS Code 分支，`--user-data-dir` 是 VS Code 公开的命令行开关（它的 `main.js` 里就认）；
    一个槽位一个用户数据目录，登录在 IDE 自己的窗口里做。平时面板只问它状态库
    （`User\globalStorage\state.vscdb`，SQLite）里令牌那一行的 `length(value)`；
    2026-09-23 起使用者点那个账户的额度刷新图标时才读值（见下面「第三样」）；
  - 邮箱、档位、各模型剩余额度是 IDE 自己写在同一个库里的 `antigravityUnifiedStateSync.userStatus`
    （protobuf），面板只读；它是没点过刷新时的兜底，联网那一份见下面「第三样」；
  - 用量读语言服务器写在 `~\.gemini\antigravity*\conversations\*.db` 里的 `gen_metadata`（protobuf）。
    两处的 protobuf 都用自写的线格式读取器（`crates/qb-accounts/src/antigravity/proto.rs`）逐 tag 走，
    字段号是 2026-09-21 对着本机 12,410 条记录数出来的，没有 Google 的 schema。
  - 能力对照的来源是 `anglee0323/Antigravity-Tools-Lite`：**CC BY-NC-SA 4.0**，而且是上面那个
    `lbjlaq/Antigravity-Manager` 的定制分支 —— **一行代码都没抄，也不能抄**。只读了它的 README
    和几个模块的说明，记下它「能做什么、靠什么做」：多账号靠把 Google 给反重力用的 OAuth
    client_id / secret 硬编码进自己程序、令牌写进 IDE 的 `state.vscdb` 并改写 `telemetry.serviceMachineId`；
    配额靠 `v1internal:fetchAvailableModels`。**前两样本项目一样都不做**（第一样是冒用官方客户端凭据、
    第二样撞「设备指纹伪装」）。它的本地用量仪表盘读的是同一批 `.db`，
    两边对 1–5 号字段的读法一致（系统提示 / 输入 / 输出 / 缓存），这是对同一份数据的独立观察，不是抄。
  - **第三样（配额接口）v0.32.0 起做了，但走的不是它那条路。** 使用者 2026-09-21 拍板：
    反重力 **Hub** 的档位与配额要显示出来（实机逐个核过，Hub 在本机一个字都没写，只能联网问）；
    2026-09-23 又要账户槽位也照 cockpit-tools 显示 5 小时 / 每周四格与 AI 积分，并拍板
    「令牌过期时面板在内存里换新」。分界仍在「谁是客户端」：
    - `Antigravity-Tools-Lite` / `cockpit-tools`：**自己当客户端** —— 把 Google 发给反重力的
      OAuth client_id / secret 硬编码进自己程序，自己跑登录、自己持有与刷新令牌；
    - 本项目：**不跑登录、不存令牌、仓库里不出现任何 Google 客户端标识**。只在使用者点刷新图标
      的那一刻，读官方客户端存在本机的那一份令牌（IDE 槽位的状态库 / Hub 的凭据管理器）；
      访问令牌过期时，用同一份里的刷新令牌在内存里换一张新的，换新要带的客户端标识也是那一刻
      从本机装的反重力（IDE 的 `main.js` / 语言服务器）里读出来、用完即丢。换来的令牌不写回。
      只发只读调用，结果只用于显示。可关（`settings.antigravity_hub_quota`）。
      代码是自己写的（`crates/qb-app/src/usecase/antigravity_quota.rs`，替掉了 0.32.0 的
      `antigravity_hub.rs`），**一行没抄** —— 它们都是 CC BY-NC-SA，抄一行就把本项目拖进同一个协议。
      默认主机不是从它们那里拿的：是 2026-09-20 从 Hub 自己的 `dist/languageServer.js`
      里读出来的 `--cloud_code_endpoint`。对应的规矩在 `CLAUDE.md`「联网额度」与 `DISCLAIMER.md` §6 / §7。

### Gemini CLI —— 只驱动公开的无交互模式（v0.26.0）

- 来源：https://github.com/google-gemini/gemini-cli（Apache-2.0）。**不分发、不修改、不链接**，
  面板在你已自行 `npm install -g @google/gemini-cli` 的前提下起它。
- 用到的公开行为：非 TTY 即无交互（stdin 喂提示词）、`--output-format json`、`-m`、
  `GEMINI_CLI_HOME`（`packages/core/src/utils/paths.ts` 核过）。
- 桥接与账户列表路径对 `oauth_creds.json` 只看在不在，不读内容；用户主动打开额度面板时的
  Code Assist 额度探针是单独的、短暂的 access token 读取，不改变桥接路径。

### ccodex-sleep-state —— 作为**外部程序**由插件商店接入（v0.24.2）

- 仓库：https://github.com/smithtaylor7748-ops/qb-gate-codex-egress（使用者自己的 fork；上游 https://github.com/gylive/ccodex-sleep-state）
- 许可：**GPL-3.0**（含 Mihomo，GPL-3.0）。
- 用在：扩展中心的「Codex 出站与换出口」条目（`crates/qb-extensions/src/plugins/codex_egress.rs`、
  `src/plugins/CodexEgressPanel.tsx`），`install_method: connect`，跟 SillyTavern 同一类。

**与上一节的区别：这一节不重写它，只当作使用者自己装的第三方程序来接入。**
本项目做的只有：定位本机已解压的 `ccodex-sleep-state.exe`、以带窗口的独立进程起它的
`setup`、停止时结束进程并调用它自己的 `restore` 恢复 Codex 配置、打开它自己的网页面板。
**不编译、不复制它一行源码、不把它链接进本程序**（0.24.2 起可一键从该仓库的 Releases 下载它发布的
二进制包，按发布自带的 `SHA256SUMS` 校验后解压 —— 下载的是成品包，不是源码）—— 它以独立进程运行，
本项目与它之间只有「起 / 停 / 打开网址」三个动作（GPL 的「聚合」而非「衍生」，
跟本项目接入 AGPL 的 SillyTavern 同理）。因此本项目的 AGPL-3.0-only + 商业授权档不受影响。

代理 / 订阅 / 机场出站，以及「换出口凑 292」这项实验性能力**全在它自己的仓库里**：
本项目核心仍然不内置代理、不换出口（上一节「明确没有做的」原样成立）。
它接管的是使用者当前激活 Codex 槽位的配置（没有槽位才是默认 `~/.codex`），与本项目的官方
turn-state 识别**互斥**（0.24.7 起两者都改同一份 `config.toml`，命令层守卫）。
对外定位见 [DISCLAIMER.md](DISCLAIMER.md) §5.3 末尾。

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
（本项目自身为 AGPL-3.0-only，另提供商业授权；两者都不因这层调用而受牵连 ——
关键在于 SillyTavern 始终是**独立程序**，既不捆绑也不链接，见下面两条线）。

**但有两条线要自己守住：**

- 如果你**改了** SillyTavern 的源码，并且拿它**对外提供网络服务**，
  AGPL 要求你公开修改后的源码。本项目默认部署是本机回环（127.0.0.1:8000），
  只有自己用，不触发这条；一旦你把它暴露到公网就要重新评估。
- 插件里那套启动时序（端口、就绪判定、回滚）是从本机现有的
  `start-claude-pro-rp.ps1` 移植的，那是用户自己的脚本，不涉及第三方许可。

---

### 酒馆的 GPT 桥接 —— 面板自带，只驱动官方 Codex CLI（v0.25.0）

`crates/qb-app/src/gpt_bridge.rs` 是本项目自己写的：一个只监听 `127.0.0.1` 的
OpenAI 兼容端点，酒馆每发一条消息就起一次**未经修改的官方 Codex CLI** 的公开无交互模式
`codex exec --json`（`CODEX_HOME` 指向当前激活的 GPT 槽位，`auth.json` 零接触，只读沙箱，
默认 `--ephemeral`）。三处来源要记清：

- **`codex exec --json` 的事件形状**（`thread.started` / `turn.started` / `item.completed` /
  `turn.completed` / `turn.failed` / `error`）来自 OpenAI 的公开文档与
  `codex exec --help`（codex-cli 0.154.0，2026-09-20 实测）。只按公开协议行为写，没有读、
  没有复制 openai/codex 仓库的源码（那是 Apache-2.0，可以抄，但没有需要）。
- **「把整段对话框成一个提示词、只驱动官方 CLI 的无交互模式、从不碰凭证」这个做法**
  参考的是使用者本人的 `claude-code-sillytavern-bridge-v2`（Claude 那条桥，同一著作权人，
  见下面「使用者自己的两个项目」）。参考的是思路，Rust 实现是从零写的，
  没有复制那份 Python。
- **SillyTavern 侧什么都没动**：桥接说的是 OpenAI 那套公开接口形状
  （`/v1/chat/completions`、`/v1/models`、SSE 的 `data: [DONE]`），酒馆用它自带的
  「Custom (OpenAI-compatible)」来源接入；本项目不改酒馆配置、不分发酒馆。

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

| 参考点                                                              | 说明                                                                                                       |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| 往 `settings.json` 挂 `SessionStart` + `UserPromptSubmit`           | 两个事件缺一不可 —— 只挂前者，会话开着不关、中途换网就没人看了                                             |
| 用一个自标记（那边是 `_ip_guard`，这里是 `_qb_gate`）认自己写的条目 | 卸载只删自己那几条，绝不整段覆盖使用者的 hooks                                                             |
| 「受限国家直接拦截」这个层次                                        | 本项目做成了国家白名单（见 `gate/judge.rs`），口径相反：那边是黑名单挡受限国家，这里是白名单只放行指定国家 |

**判定本身没有抄。** 那边每次请求现查一次 IP；这里的 hook 脚本里一行判定逻辑
都没有，只读 Rust 落下的 `gate-verdict.json` —— 在 PowerShell 里再写一遍白名单
比对，就是硬约束 10 明令禁止的「在别处再拼一份」。

失败语义也不同：那边检测失败一律放行（不因网络问题阻断），这里是 fail-closed
（判不过拦、查不到拦、裁决过期也拦），由使用者明确选定。

### cc-proxy-detector — 中转站后端判定表

- 仓库：https://github.com/zxc123aa/cc-proxy-detector
- 许可：MIT

原版是 Python，这里是 Rust 重写（`relay/backend.rs`）。抄的是判定思路：

| 参考点                                                        | 说明                                                                                                                      |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| 三后端划分：Anthropic / AWS Bedrock / Google Vertex           | 所有 Claude 访问最终都落到这三家之一                                                                                      |
| 响应头指纹（`x-amzn-*`、`x-goog-*`、`anthropic-ratelimit-*`） | 直接证据                                                                                                                  |
| **缺字段负证据**                                              | 回的是 Claude 格式却一个 `anthropic-*` 头都没有 —— 转换层做得出响应体，造不出上游本来没有的头。这一招是原版最有价值的部分 |
| 限流头动态验证                                                | 连发两次看计数动没动，写死的假头不会变                                                                                    |
| 逆向来源识别（Kiro / Antigravity 等）                         | 只在有把握时报，没把握就是「说不准」                                                                                      |

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

| 参考点               | 本项目怎么做的                                                                                                                                   |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| 主模型与小模型分开配 | `ProviderMeta.small_fast_model` → `ANTHROPIC_SMALL_FAST_MODEL`。很多中转站两档不是同一个名字，只配主模型时后台请求会报一个跟你正在做的事无关的错 |
| Base URL 智能推断    | `relay::probe::normalize_base_url`，把粘进来的完整端点归一成 base。归一放在 `upsert` 里而不是表单里 —— 手填、预设、导入三条路都经过它            |

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

### CheckClaude — 服务可达、解析、代理形态、IPv6 出口、真实浏览器采集（2026-09-24 追加）

- 仓库：https://github.com/zzusec/CheckClaude
- 许可：**MIT**，Copyright (c) 2026 zzusec

使用者让「根据 CheckClaude 看看能不能优化一下环境监测」，并从它的检测项里选了三组。取的是**检测项与判法**，
代码全部自己写（Rust 在 `qb-probe::probe::reach`、`qb-sysenv::checkup`、`qb-app::browser_probe`，
前端在 `src/probe/` 与 `src/lib/browserProbe.ts`）：

| 参考点                                                                   | 本项目怎么做的                                                                                                                         |
| ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| 不带凭据问 `api.anthropic.com`：401 = 放行、403 = 地区拦截               | 「Anthropic 服务可达」一项；绕过代理那一路必测（跟门禁同一个客户端），系统代理开着时再测跟随代理那一路                                |
| `claude.ai` / `anthropic.com` 可达，`cf-mitigated` 的 403 不算拦截        | 同一项里只降到警告或「查不出」，判定以 API 为准                                                                                         |
| 解析结果分段：fake-ip、Anthropic、Cloudflare、私网（被污染）              | 「claude.ai 的解析」一项。地址段取各家公布的（Anthropic 文档的「IP addresses」、cloudflare.com/ips），写成常量                        |
| 只用只有 AAAA 的回显服务测 IPv6 出口（双栈的会退回 v4）                   | 「IPv6」一项改看真实出口：`api6.ipify.org` → `ipv6.icanhazip.com` → `v6.ident.me`，再问国家，跟 IPv4 出口比                            |
| 代理形态：PAC、自动检测（WPAD）、TUN                                      | 「代理形态」一项；TUN 按承载默认路由的网卡**是不是硬件网卡**认（`HardwareInterface`），不按名字，排除 Hyper-V 虚拟交换机               |
| BrowserBridge：回环上的一次性服务 + 随机令牌，让真实浏览器跑采集           | `qb-app::browser_probe`：端口系统挑、令牌、校验 `Host`、只收一份 ≤ 64 KB、120 秒、结果只在内存；页面跑同一份 `signals.ts`              |
| 关键项一票否决                                                            | `score.ts` 的关键项封顶：档位最高「偏差」，权重表不动                                                                                  |

**没有采纳的**：三路出口（使用者这轮没选）；24 小时出口稳定性（本项目的定位就是频繁换 IP，拿跳变扣分跟定位打架）；
虚拟机检测（开了 Hyper-V / VBS 的物理机也会被认成虚拟机）；四家情报 + ASN、边缘机房（已有三家国家互核，
它的「边缘机房」比的是 Cloudflare 的 `loc`，本项目已经在用）；一键改 DNS / 关 PAC（会改网络配置、可能当场断网，
只给命令和代价）。

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
（按 DISCLAIMER 里「不对账户状态作任何承诺」那几句去搜，**别记行号** —— 行号每改一次就漂）；
后者是**把整机流量强制走代理**，会让本项目变成「内置代理功能」，与 DISCLAIMER
第 5 节矛盾。

> **0.19.0 补一句**：面板确实加了「浏览器出站锁」，但它跟 cac 那套不是一回事 ——
> 只给使用者点名的**那一个**浏览器 exe 加**出站**防火墙规则，不接管整机流量、
> 不自己当代理、不做转发。边界写在 `CLAUDE.md` 的「两个口子」一节，
> 对外措辞在 DISCLAIMER §5.2。

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

| 来源                                                             | 用途                                                           | 许可与固定依据                                                                                                                                                    |
| ---------------------------------------------------------------- | -------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [cc-switch](https://github.com/farion1231/cc-switch)             | 配置适配、MCP/Skills 与备份组织方式的参考；QB Gate 独立实现    | MIT                                                                                                                                                               |
| [UniGetUI](https://github.com/Devolutions/UniGetUI)              | 发现、已安装、更新及任务交互参考                               | MIT；未复制应用代码                                                                                                                                               |
| [Bruno](https://github.com/usebruno/bruno)                       | 脱敏请求集合导出思路与 `.bru` 文件格式                         | MIT；未打包 Bruno                                                                                                                                                 |
| [MCP Registry](https://github.com/modelcontextprotocol/registry) | 元数据、来源与版本字段参考                                     | 未复制源码，不把收录视为安全背书                                                                                                                                  |
| [Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk) | 直接依赖客户端初始化、stdio/HTTP 传输与能力检查                | rmcp **3.3.0**，crate 声明 Apache-2.0；固定依赖许可，不假定整个生态统一许可                                                                                       |
| [Agent Skills](https://github.com/agentskills/agentskills)       | SKILL.md 元数据约束和来源固定设计                              | 仓库双许可：代码 Apache-2.0，**文档 CC-BY-4.0**。本项目参考的是规范文档那一半，按 CC-BY-4.0 署名，未复制代码。核查版本 `69ef37e9424c0a7ea9dd2293b559e43ec8176379` |
| [Anthropic Skills](https://github.com/anthropics/skills)         | 精选目录收录 skill-creator 与 webapp-testing，用户安装时才下载 | 两个目录各自 Apache-2.0；固定 `34040c9c568585f6929bedeaad110ad08f079624`。不将整个仓库视为同一许可                                                                |
| [MCP Servers](https://github.com/modelcontextprotocol/servers)   | 精选 Filesystem 与 Git MCP 运行配置                            | 仓库核查 `d73f99efbfd40c3aa1b61e88728b3d49fb52608f`；Filesystem npm **2026.8.31**，Git PyPI **2026.8.18**（包声明 MIT）；上游有许可迁移，实际安装版本分别核对     |
| [Tauri plugins](https://github.com/tauri-apps/plugins-workspace) | 直接使用单实例、对话框、链接打开插件                           | MIT OR Apache-2.0；实际版本在 Cargo.lock                                                                                                                          |
| React Router / TanStack Query                                    | 前端路由和服务端状态管理                                       | MIT；实际版本在 package-lock.json                                                                                                                                 |
| rusqlite / SQLite / ts-rs                                        | 本地元数据、事务、类型导出                                     | 分别按锁文件依赖声明；完整文本见 THIRD_PARTY_NOTICES.txt                                                                                                          |

这里写死的提交哈希是**代码强制的**：面板的「检查来源更新」只会报告上游有新提交，
不会改掉精选目录里这两个固定版本（见 `extensions::check_updates`）。需要新版本的人
自己从来源手动导入，导入进来的是一条独立记录，不影响这张表里核对过的那一版。

## 自有资产（不是第三方）

### 应用图标 `src-tauri/icons/`

- 文件：`icon.ico`、`icon.png`、`128x128.png`、`128x128@2x.png`、`32x32.png`
- 造型：盾形轮廓 + 钥匙孔，填充 `#C96442`，钥匙孔 `#1C1916`
- 用在：应用与安装包图标（`tauri.conf.json` 的 `bundle.icon`）
- 来源：**维护者自制（自绘 / AI 生成）**。不来自任何图标站或图标包，
  **没有第三方署名义务，也没有商用限制** —— 商业授权那一档不受它牵连。

> 补记于 2026-09-16。这五个文件原本是在 `51cead7` 里随功能提交一并加进来的，
> 提交说明没提图标，本文件也一直没有记录。按 CLAUDE.md「抄了什么、没抄什么都要写进
> ATTRIBUTION.md」，这是一条欠账，现在补上。

两条边界记下来，免得以后误判：

- **界面里的图标是另一回事。** 那些来自 lucide-react（ISC），见上面「界面依赖」一节。
  这里说的只有应用图标那五个文件；
- **AI 生成的图像，著作权状态在多个法域尚未确定**（美国版权局的立场是：纯 AI 生成、
  没有人类创作介入的作品不受版权保护）。这**不影响本项目使用它，也不构成侵权风险** ——
  影响的是反过来那一面：别人照抄这个图标，我们未必拦得住。
  哪天要把图标当成品牌资产去主张权利，得先换成有实质人类创作介入的版本。

---

## 使用者自己的两个项目（同一著作权人，不是第三方）

中转站的计费核验与调度口径来自维护者自己的两个既有项目。**著作权同属一人**，
不受「没有 LICENSE = 保留全部权利」那条约束——那条针对的是上游第三方。
即便如此，这里仍按规矩记清楚拿了什么：

### `station-monitor-standalone`（账号工具箱拆出的中转站监控）

拿的是**方法，不是代码**——全部用 Rust 重写，一行 Python 都没有搬：

| 拿来的                                                                                                              | 落在哪                             |
| ------------------------------------------------------------------------------------------------------------------- | ---------------------------------- |
| 真实倍率 = `站点单价 × 标称倍率 ÷ 官方单价`，**四类各算各的**                                                       | `station::pricing::verdicts`       |
| 官方价目要带 `source` / `verified_at`，超期标 stale（原值 180 天）                                                  | `pricing::STALE_AFTER_DAYS`        |
| 跨币种不比（`requires_currency_conversion`）                                                                        | `PriceStatus::CurrencyMismatch`    |
| New API 的四类倍率字段（`model_ratio` / `completion_ratio` / `cache_ratio` / `create_cache_ratio` / `model_price`） | `pricing::parse_station_pricing`   |
| 缓存命中低于 0.80 该提醒（`CACHE_ATTENTION_THRESHOLD`）                                                             | `audit::CACHE_ATTENTION_THRESHOLD` |
| **证据档次与可信度分开**（`evidence_level`：sufficient/partial/insufficient/conflict）                              | `audit::EvidenceLevel`             |

`gpt-5.6-sol` 那条价目是维护者自己核对过的（2026-08-28），原样沿用并保留了来源链接。

**没拿的**：那个项目的 `server.py` 有 11,008 行，覆盖邮箱、TOTP、局域网传输、
账号巡检等等——那些不在 QB Gate 的范围内，一概没看也没搬。

### `sub2guard`（207 服务器上的调度外挂）

拿的是两条**实测结论**，都写进了代码注释里当依据：

1. **只看首字会选中最卡的那条。** 实测 2690 首字 1.4 秒全组最快、107.9 ms/token、
   一次回答 95.8 秒；2681 首字 4.7 秒却快 5 倍。修法是体验分 =
   `首字 + 每 token 耗时 × 典型回答长度`（落在 `Window::experience_ms`）；
2. **排序指标只统计成功的请求**，所以挂掉的线路会顶着上周的好成绩排前面
   （㉙）。QB Gate 侧对应的是 `UsageRow::status_reported` 与熔断器。

峰时倍率 1.5 倍浮动「取 max 才是上界」也来自那份档案（`StationRates::peak_rate`）。

## 评估过但没有采用

### 订阅指引（Subscription Guide）— MIT，已移除

v0.12.0 的重构里带进来一份 3,310 行的订阅指引组件（`src/subscription-guide/`：
购买步骤、常见问题、视频与来源清单，附 MIT LICENSE）。**它从头到尾没有被挂上过**
—— 全树零个引用点，v0.11.0 里也不存在这个目录。

没有采用它，原因是这一版重排的主线恰恰是删掉「无用的介绍」：一份
750 行的购买教程正是使用者点名嫌多的那类内容，而且价格与步骤会过时、
没人维护就会从帮助变成误导。代码已从工作区移除，要找的话在 git 历史里。
按 MIT 的要求，这里保留出处记录。

**0.24.5 补记：现在的 `/subscription` 页（`src/features/subscription/`）是另起炉灶的原创内容与代码**，
不含那份 MIT 模块的任何代码或文字；套餐价、实测等值、中转站问题的出处逐条记在
[docs/subscription-guide/SOURCES.md](docs/subscription-guide/SOURCES.md)，页面上也有「参考来源」折叠块。
引用的都是公开网页（官方价目、App Store 商店页、帮助中心、媒体报道、arXiv 论文、GitHub issue），
只转述事实与数字，不复制原文段落。

SillyTavern 是外部 AGPL-3.0 应用。QB Gate 接入用户已有安装，不捆绑应用本体；本项目按 AGPL-3.0-only 发布，并另行提供商业授权。

[docs/dependencies.json](docs/dependencies.json) 记录依赖名称、版本、来源和许可声明；[THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt) 包含可获得的原始版权及许可文本，并随安装包提供。缺少随包许可文件时的补充文本固定到上游提交，记录于 docs/license-supplements.json。构建和测试依赖也一并列出。

### 0.22.0：Codex 桌面账户管理

按使用者给出的 [Cockpit Tools](https://github.com/jlcodes99/cockpit-tools) README 了解其账户管理、独立实例及用量展示思路。该项目采用 CC BY-NC-SA 4.0；本次没有复制其源码。

登录存储依据 [OpenAI Codex Authentication](https://developers.openai.com/codex/auth/)；桌面端的 `CODEX_HOME` 与 `CODEX_ELECTRON_USER_DATA_PATH` 行为在本机已安装的 Microsoft Store 版本 26.908.9136.0 中核对。窗口控制、独立目录、凭据元信息解析和本地 rollout 统计由本项目独立实现。桌面窗口目录参数属于当前客户端实现，后续版本需继续验证。
