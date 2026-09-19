# 交接：官方 Codex 的 turn-state（292）融合 · v0.24.0 未完部分

> ⚠ **历史文件（0.24.0 的交接），接线部分已过时。** 0.24.7 起「官方 Codex 一键接进本机路由」
> 不再走独立环境目录、不复制 OAuth、不另起 Codex —— 识别直接改**当前激活 Codex 槽位**的
> `config.toml`（`crates/qb-app/src/usecase/turnstate_ops.rs`）。下面「未完部分 / 接线步骤」
> 里凡是提到 `router_official_environment`、复制 `auth.json`、`station_turnstate_launch` 的，
> 都不要照做；以 `CLAUDE.md`「官方 Codex 的 turn-state 口子」与
> `docs/DESIGN-NOTES.zh-CN.md`「v0.24.7：识别直接作用于槽位」为准。六条硬约束不变。

> 给接手的 AI / 人。**你会冷启动，先读这份，再读仓库根 `CLAUDE.md`。** 项目在 `D:\claude-gate`。
> 使用者的终端是 **Windows PowerShell**（不是 bash）：给他的命令别用 `&&` / `sha256sum` / `/d/...`。

## 一句话现状

把 ccodex-sleep-state（GPL）的 **turn-state 采集 / 注入机制 clean-room 重写进了 QB Gate**，
**只作用于官方 Codex，不碰 Claude**。**核心机制 + IPC + 前端面板 + 文档 + 版本号（0.24.0）已完成并通过单测/构建**。
**唯一没做的大件：把「官方 Codex 一键接进本机路由」那条启动链接线（`workspace.rs`）**——
它要写 OAuth-carrying 的 Codex 配置、只能用使用者自己的 ChatGPT 登录实测，风险高，故留给这一轮做。

## 目标与红线（改任何东西前先记住）

使用者要的是：**保留 ccodex 的核心能力——提取并注入 `X-Codex-Turn-State`，用到官方 Codex 上**；
并明确说过「只改 GPT Codex，不改 Claude」「本机路由只路由 `127.0.0.1`，不属于当初决定不引入的那部分，可以做」。

⛔ **六条硬约束**（已写进 `CLAUDE.md`「官方 Codex 的 turn-state 口子」一节，动了要同步改 DISCLAIMER §5.3 与 ATTRIBUTION）：

1. **只对 `Client::Codex`。** Claude Code / 桌面端一律不进官方模式、不注入、不采集。有测试 `turn_state_stays_off_for_claude_routes` 钉着。
2. **一行 ccodex 源码都不许抄**（第三方 GPL，会堵死本项目的商业授权档）。只按公开协议行为自己写；**不引入 Mihomo / 订阅 / 出站协议 / 代理出口池**。
3. **不「换出口凑 292」**（没有出口池，只在当前连接上采集）。
4. **被动采集，不发合成探测烧额度**；客户端自己带了 turn-state 就保留它的，不覆盖。
5. **`UpstreamAuth::OAuthPassthrough` 是「路由不承载官方身份」的唯一受控例外**：只有它保留客户端 OAuth、不剥不换；仍只绑回环、不改系统代理、不做链式转发。
6. **长度不是质量/额度指标**，默认关闭，只在使用者手动开启后生效。

其余不变的老规矩（`CLAUDE.md`）：账户四条线（不读限流换号 / 手动切换 / 单账户 / 本人拥有）、抄代码先看 license、颜色只在 `tokens.css`、文案不写 Markdown `**`、单测不碰运行期状态、**改完要更新本机装的那份**。

## 已完成（都通过了 `cargo test --workspace` / `npm run build` / `types:check` / `release:check`）

**纯函数层（`crates/qb-station/`，无新依赖）**
- `src/sse.rs`：SSE 终止事件分类。`StreamScanner`（增量、不缓冲整条响应）、`classify_stream`、`StreamOutcome`（Completed/Failed/Capacity/RateLimited/Incomplete）+ `to_outcome()`（Incomplete 折成 Ok，不误杀客户端取消）。带单测。
- `src/turnstate.rs`：turn-state 信封外形解析（自写 base64url + FNV 指纹）、`Policy`、`AccountKind`、`Store`（offer/acquire/observe/needs_refresh/status，active/ready 状态机）、`Status`/`ModelStatus`（TS 导出名 `TurnStateStatus`/`TurnStateModelStatus`）。带单测。
- `src/lib.rs`：注册了 `pub mod sse; pub mod turnstate;`。

**路由层（`crates/qb-app/src/local_router.rs`）**
- `UpstreamAuth::OAuthPassthrough`：官方模式，保留客户端 OAuth。
- `TurnState`/`TurnKind` 运行态 + `RouterState` 字段 + 方法（`turnstate_configure/enabled/acquire/offer/observe/status`）。
- `proxy()`：官方检测、官方模式保留 OAuth（不走 `CLIENT_AUTH_HEADERS` 剥离）、客户端没带时注入、响应头**被动采集** + observe。
- **Part 2（顺带修的 bug）**：Codex 的 2xx 健康判定改看 SSE 结局——把 `record()` 拆成 `record_log` + `record_breaker`，Codex 2xx 推迟到 `FinishLog::drop` 用 `StreamScanner` 折算。**只对 Codex**，Claude 路由行为不变。
- 新增 5 条端到端测试（回环假上游），全过。

**接口 / 前端 / 文档**
- `src-tauri/src/commands/station.rs`：路由 client 加 `connect_timeout(15s)`（**不设读/总超时**，别加，会截断长流）；新增 IPC `station_turnstate_configure(enabled, team)` 与 `station_turnstate_status()`。
- `src-tauri/src/lib.rs`：注册两个命令；`pub use qb_station::{…, turnstate};`。
- `src-tauri/examples/export-types.rs`：导出这两个 TS 类型。
- `src/lib/station.ts`：`turnstateConfigure` / `turnstateStatus`。
- `src/features/station/StationTurnState.tsx`：面板（开关 + 个人/Team + 状态表 + 免责话术）。
- `src/features/station/StationCenter.tsx`：**Codex-only** 触发按钮（`client === "codex"`）+ Modal。
- `src/lib/demo.ts`：两个命令的演示 fixture。
- 文档：`ATTRIBUTION.md`（ccodex clean-room 一节）、`DISCLAIMER.md` §5.3、`CLAUDE.md`（turn-state 口子）、`CHANGELOG.md` 0.24.0。
- 版本号五处已提到 **0.24.0**（`release:check` 过）。

## ⚠ 未完成：把官方 Codex 一键接进本机路由（这一轮的主任务）

现状：**路由侧已就绪**（`OAuthPassthrough` 模式 + 采集/注入 + IPC 开关），但**还没有任何东西把官方 Codex 的流量导进路由**——所以端到端还跑不起来。要补的接线都在 `crates/qb-app/src/workspace.rs`（安全关键文件，改前读 `docs/DESIGN-NOTES.zh-CN.md` 与档案里 §7.10 auth.json 覆盖登出的教训）：

1. **造一个「官方 Codex turn-state」环境**（建议独立 provider id，如 `qb-router-official`，环境 id `qb-router-codex-official`，只 Codex）。参照现有 `router_environment()`（`workspace.rs:236`）。
2. **写它的 `config.toml`**（参照 `configuration_files` 的 Codex 分支 `workspace.rs:806-843`）：`base_url` 指向本机路由 `http://127.0.0.1:15721/codex/v1`（`client_base_url(Codex,…)` 已经带 `/codex/v1`），但**与中转不同**：要 `requires_openai_auth = true`、**不写** `env_key`、**不写** `forced_login_method="api"`——这样 Codex 会用 ChatGPT OAuth 打这个 base_url。
3. **写它的 `auth.json`**：**复制**（绝不移动）使用者真实 `~/.codex/auth.json` 的 OAuth 进环境目录；`workspace.rs:844` 那条「中转环境目录出现 OAuth 凭证就拒启」的拦截**要为这个官方环境开例外**（其它中转环境仍拦）。参照账户槽位 `qb-accounts/src/codex` 的 OAuth 处理。
4. **启动环境变量**（`workspace::launch` 约 `1360-1411`）：官方环境**不要**push `OPENAI_API_KEY=ROUTER_KEY`（那会逼成 api 模式）；`CODEX_HOME` 指这个环境目录。
5. **注册上游并选中**：官方环境启动时往路由 `put_upstream(Upstream{ base_url:"https://chatgpt.com/backend-api/codex", auth: OAuthPassthrough, route_id:… })` 并 `set_now(Client::Codex, …)`。注意 `load_upstreams`（`src-tauri/src/commands/station.rs:88`）用 `replace_upstreams` 会清空 map——两者的交互要理顺（别让 relay 的 `load_upstreams` 把官方上游冲掉）。
6. **开关联动**：`station_turnstate_configure(enabled=true)` 时顺带 arm 官方上游、并在界面提示「已把 Codex 接进路由，重启 Codex 新建会话」。
7. 前端：`StationTurnState.tsx` 里补一个「把 Codex 接进路由 / 启动」按钮，或复用现有 `station_launch` 那条链。

> 也可以先只给**手动接入的文档**（让使用者把 Codex `config.toml` 的 base_url 指到路由、`requires_openai_auth=true`、OAuth 登录保留），把一键启动作为后续。但使用者要的是「功能加上」，尽量做成一键。

## 收工必须跑的门（还没跑的）

逐条跑，PowerShell，**任何一条红就停**（`CLAUDE.md`「收工清单」）：

```powershell
npm test
npm run format:check
npm run test:ui
cargo clippy --workspace --lib -- -D warnings
cargo fmt --all -- --check
cargo deny check
```

已跑过且绿：`cargo test --workspace`、`npm run build`、`npm run types:check`、`npm run release:check`。

> `test:ui` 跑 demo 模式，我加的 turn-state 面板是**默认关闭的 Modal + Codex-only 按钮**，理论上不影响截图；但 `test:ui` 会切到 codex 分页吗要留意，别让右列按钮在窄屏溢出。
> `cargo deny check` 的 licenses 那条会盯有没有混进 GPL 代码/依赖——我们没引入，应当绿。

## 出包 + 装本机（`CLAUDE.md`「改完 = 本机那份也更新了」）

```powershell
npm run tauri build
```

产物 `target\release\bundle\nsis\QB Gate_0.24.0_x64-setup.exe`。**先卸旧的 QB Gate 再装新的**（改过名，不会覆盖）。
⚠ **磁盘**：这台 D: 之前被 71GB 的 `target` 占满过（我 `cargo clean` 清过一次）；`tauri build` 前确认 D: 还有空间。

## 验证边界（别把没验的写成验过）

- 单测、离线/回环端到端**已过**。
- **真实官方 Codex（ChatGPT 登录）走路由采集/注入 292，只能使用者本人用他的账号实测**——这一步谁也替不了，验收时如实记录，别把偶发成功写成稳定能力（ccodex 自己都常采不到合格 292）。

## 关键文件与参考

- 计划全文：本地计划归档（`%USERPROFILE%\.claude\plans\` 下当轮那份 `*-floating-fern.md`）
- 硬规矩：仓库根 `CLAUDE.md`（**权威**）；设计经过 `docs/DESIGN-NOTES.zh-CN.md`；已知缺口 `docs/KNOWN-ISSUES.zh-CN.md`
- 来源许可：`ATTRIBUTION.md`（ccodex-sleep-state 一节）；对外定位 `DISCLAIMER.md` §5.3
- 源码：`crates/qb-station/src/{sse,turnstate}.rs`、`crates/qb-app/src/local_router.rs`、`src-tauri/src/commands/station.rs`、`src/features/station/StationTurnState.tsx`
- 原始被融合源码：`ccodex-sleep-state-main.zip`（GPL，SHA-256 `27EC0649…72A174C`，**只读思路不抄码**；放在使用者本机下载目录）
