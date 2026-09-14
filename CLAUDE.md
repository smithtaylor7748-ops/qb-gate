# 改这个仓库之前先读这里

给接手这个项目的人（以及 AI 助手）看的硬规矩。每一条都是踩出来的，不是推理出来的。

实现层面的「为什么这么写」在 [docs/DESIGN-NOTES.zh-CN.md](docs/DESIGN-NOTES.zh-CN.md)，
修不掉的缺口在 [docs/KNOWN-ISSUES.zh-CN.md](docs/KNOWN-ISSUES.zh-CN.md)。

---

## ⛔ 改完了 = 本机那份也更新了

**任何一轮改动落地，都要把本机装着的那份一起更新掉。** 不只是修 bug ——
界面重排、加功能、改文案，一律算。

**这一步不需要使用者开口要，它是「改完了」的定义的一部分。**

仓库改好了、本机还跑着旧版，等于什么都没修 —— 而且下次排查时会对着新代码看旧行为，
得出的结论全是错的。这个坑的代价不是「少更新一次」，是**后面每一次调试都在骗自己**。

### 注意哪些命令不会更新本机那份

| 命令 | 产出 | 会不会动本机装的那份 |
|---|---|---|
| `npm run build` | 只出 `dist/` | ❌ |
| `npm run tauri dev` / `npm run dev` | 临时进程，关了就没 | ❌ |
| `npm run demo` | 同上，而且喂的是假数据 | ❌ |
| **`npm run tauri build`** | `target/release/bundle/nsis/QB Gate_<版本>_x64-setup.exe`（workspace 的 target 在仓库根） | 出安装包，**还要装** |

### 收工清单

```bash
npm run build && npm test && npm run types:check && npm run format:check && npm run test:ui && npm run release:check
```

```bash
cargo test --workspace && cargo clippy --workspace --lib -- -D warnings && cargo fmt --all -- --check && cargo deny check
```

```bash
npm run tauri build      # 出安装包（release 编译约 3 分钟）
```

**这两组就是 CI 跑的那一套**，别只跑前两条就以为绿了 —— `test:ui`（72 个响应式/
主题组合 + 5 个交互流程）和 `cargo deny`（advisories / bans / licenses / sources）
各自抓过别处抓不到的东西。`cargo test --workspace` 的通过数**只许涨不许跌**。

然后：

1. **版本号往前提。** 不是三处，是**四个文件五个位置**，漏一个
   `npm run release:check` 当场报错：

   | 文件 | 位置 |
   |---|---|
   | `package.json` | `version` |
   | `package-lock.json` | 顶层 `version` **和** `packages[""].version` 两处 |
   | `src-tauri/Cargo.toml` | `version`（必须写字面量，release-check 拿正则读它） |
   | `src-tauri/tauri.conf.json` | `version` |
   | `Cargo.toml`（仓库根） | `[workspace.package]` 的 `version` —— 其余 13 个 crate 从这里取 |

   **权威是 `npm run release:check`，不是这张表**：表会过期，那条命令不会。
   文档里一旦引用了某个版本号，就必须真的存在那一版；
2. 装上新的安装包；
3. **装之前先确认旧的那份叫什么名字。** 改过名（ClaudeGate → QB Gate）之后，
   新安装包**不会覆盖**旧名字那一份，会变成两套并存：两个卸载项、两个快捷方式、
   两套运行期数据。先卸旧的再装新的。

---

## ⛔ 中转会话不归门禁的**关停**策略管

`LaunchTarget::gated()` 与 `LaunchTarget::stops_with_gate()` 是**两个**判断，
不许合并：

| 问题 | 谁回答 | 中转的答案 |
|---|---|---|
| 起之前要不要验 IP 解锁、起完要不要持租约 | `gated()` | **要**。Deny ACE 是按文件加的，不认身份；让中转绕过解锁，就是拿中转会话把 claude.exe 解锁、再从终端起官方的 —— 现成的绕过入口 |
| 门禁判不过时要不要收掉这个会话 | `stops_with_gate()` | **不收**。中转请求打第三方端点、用你自己买的 Key、不带官方 OAuth 身份，Anthropic 那边看不见。收它换不到任何保护，却会把正在写的对话弄丢 —— 而中转站的典型使用者恰恰就是出口 IP 会变的那群人 |

同理，会话内 hook 不写进中转环境目录，而且 `set_hook` 会**主动摘掉**旧版本留在
那里的那一份（只是不再写入的话，升级上来的人身上会留一个没人管却一直在拦的 hook）。

`usecase::gate_ops::stop_managed` 里那句
`if all_sessions { execute() } else { execute_official() }`
是同一条不变量的另一半：门禁驱动的关停按 PID 杀进程时也要放过中转，
否则前面判断白做。

（这个函数原来在 `gate::stop_managed`。A1 把它连同 `run_watchdog` 一起搬进了
`usecase::gate_ops` —— 它要同时碰 killswitch / plugins / sessions / tray /
operations，是跨域编排而不是门禁自己的事。留在 `gate` 里正是 `gate` 变成
「伪装成底层的编排器」的原因。判定与执行仍在 `gate`。）

## ⛔ 不许加的功能

这三类不是「暂时没做」，是**明确不做**。加进来会让整个项目的定位垮掉，
也会让 [DISCLAIMER.md](DISCLAIMER.md) 变成谎话。

| 不做什么 | 为什么 |
|---|---|
| **设备指纹伪装**（UUID / 主机名 / MAC / machine-id 改写） | 主要用途是多账号规避，与硬约束「所有账户必须本人拥有」直接冲突。DISCLAIMER 写死了「不承诺防封」与「不为规避封禁而设计，也无法达到该目的」（按这两句话去搜，别记行号 —— 行号每改一次 DISCLAIMER 就漂一次） |
| **内置代理 / VPN / 流量绑定**（强制所有流量走代理、断网即停、mTLS 中继） | DISCLAIMER 里那句「不提供、不内置、不分发任何代理、VPN、翻墙或网络绕过功能」。做了这个，这句话就是假的 |
| **自动换号**（按额度、429、限流自动切槽位） | 合规边界四条的前两条。存在这条路径，「多槽位」的定位就从「管理你自己的账户」变成了「规避限制」 |
| **联网查额度**（调 OAuth 内部接口、抓 `/usage` 背后的端点） | 未公开接口，上游一改就断；而且 DISCLAIMER 写着不调它。用量只许读官方客户端**自己写在本机的文件**（`accounts/usage.rs`），零网络请求，只用于显示，面板不据此做任何决定 |

检测与**如实报告**不在此列 —— 面板可以告诉你「你的时区和出口对不上」，
但不替你改机器身份。两者的区别是：前者让使用者知情，后者替他伪装。

---

## ⛔ 抄代码之前先看 license

上游的许可状况逐条记在 [ATTRIBUTION.md](ATTRIBUTION.md)。三条铁律：

1. **MIT 可以抄**，保留版权声明即可（与本项目的 GPL-3.0 相容）；
2. **没有 license 文件 = 保留全部权利，一行都不能抄**。看可以看，实现必须自己写。
   目前已知：`dai-chao/Agent-Guard`、`iprisk-top`、`Trentct/claude-code-ban-risk`、
   `jlcodes/cockpit-tools`；
3. **CC BY-NC-SA 不能抄**：SA 会把本项目拖成同一个协议，NC 会禁止商业使用。

抄了什么、没抄什么、为什么没抄，都要写进 ATTRIBUTION.md —— 这不是礼貌，
是社区开源推广申明里承诺过的事。

---

## ⛔ 看门狗查不到 IP 就立刻收，两档都没有宽限

`WatchMode::unknown_grace()` 两档都返回 `None`。**这是使用者明确选的严格档，
不是忘了写。**

改回去只要一个函数（返回 `Some(Duration::from_secs(180))`，`decide` 里的分支还在），
但改之前先看 `watchdog.rs` 里那段说明和 `cli_no_longer_gets_a_grace_period` 这条
测试 —— 它们写着这个决定的代价：网络抖一下就会关掉正在用的 Claude，未保存的
对话会丢。

### 这句「改一个函数就行」曾经是假的

有一版 `run_watchdog` 自己写了一套「不通过就收」，`decide` 退化成只有单测在调。
当时**毫无症状** —— 两档宽限都是 `None`，自己写的那套算出来的结果跟 `decide` 一样，
20 条单测照样全过。代价是：下一个人照着上面那句话改完 `unknown_grace()`、
跑通全部测试、以为宽限期回来了，而实际一秒都没加上。

### 守着这条链的是一条测试，不是可见性

`Tick` / `StopReason` / `decide` 原来收窄到 `pub(super)` / `pub(crate)`：
谁把判定搬回 `run_watchdog` 里自己写，它们就成了 dead_code，
`cargo clippy --lib -- -D warnings` 直接编译失败。

**W2 拆 crate 之后这道门没了**：`decide` 在 `qb-iplock`、`run_watchdog` 在
`qb-app`，跨 crate 调用只能是 `pub`，而 `pub` 在 `pub mod` 里逃出了 dead_code
分析。顶替它的是 `src-tauri/tests/architecture.rs` 里的
`the_watchdog_still_routes_through_the_judge` —— 它直接读源码，断言
`run_watchdog` 体内有 `watchdog::decide(`。

**比原来那道门更准**：dead_code 只能证明「有人在调」，这条证明的是
「看门狗在调」。删这条测试等于把这一节的教训扔掉。

**判定仍然是分开的**（E4 没有被推翻）：`IpUnknown` / `CountryUnknown` 与
`IpNotAllowed` / `CountryNotAllowed` 是四个不同的值，日志上是四句不同的话。
改的只是「查不到」这一档的处置。日志上分不分得开决定了使用者该去查网络还是
去换节点 —— 这两件事的处理方式完全相反，合并了谁都查不出来。

---

## ⛔ 单测不许碰真实的运行期状态

不联网、不动真 ACL、不碰真进程，**也不写 `%LOCALAPPDATA%\ClaudeIpGate\` 下的任何文件**。

纯数据类型不做 I/O，落盘一律由调用方显式做。

这一条是拿实机代价换来的：一条单测把使用者真实的 `lease.json` 写成了测试数据，
而症状伪装成「功能正常工作」。

---

## ⛔ 分层由四条测试守着，不是由自觉

Rust 侧是一个 Cargo workspace：`qb-foundation`(L0) → `qb-contract` → `qb-platform`
→ 九个领域 crate → `qb-app`(编排) → `src-tauri`(命令/托盘/装配)。

`src-tauri/tests/architecture.rs` 里有九条测试钉着它，其中四条是硬规矩：

| 测试 | 它不许发生什么 |
|---|---|
| `module_cycles_only_ever_shrink` | 任意一组模块互相到得了（算的是**强连通分量**，不是成对互指 —— 早先只查成对，漏掉了一个 11 模块的环整整一轮） |
| `crates_only_depend_downwards` | crate 往上层依赖；同层依赖必须登记在 `ALLOWED_SIDEWAYS` 里，而且那张表**只许变短** |
| `only_the_app_crate_knows_about_tauri` | 领域或编排 crate 的 `Cargo.toml` 里出现 `tauri` |
| `the_watchdog_still_routes_through_the_judge` | 看门狗自己写一套判定，绕开 `watchdog::decide` |

**最后一条尤其别删**：它顶替的是一道拆 crate 之后失效的编译期护栏，见上面那一节。

新增 crate 要在 `LAYERS` 里给它一个层号 —— 忘了加会直接报错，这是故意的：
「这个 crate 在哪一层」是必须当场想清楚的事，不是可以以后再说的事。

---

## ⛔ 「Claude 装在哪」全项目只有一张表

`crates/qb-install/src/install/inventory.rs`。检测、启动、上锁、升级、残留清理、
版本库全都从它拿，**不许在别处再拼路径**。

v0.8.0 之前四处各拼一份、已经对不上 —— 只用 winget 装的人被锁在门外。

同理，**门禁判定全项目只有一个函数**：`crates/qb-iplock/src/gate/judge.rs`。
看门狗、会话内 hook、手动放行三处共用。各判各的，漏掉某一维的那一处就是绕过入口。

---

## ⛔ 每一个完整可执行的 claude.exe 副本都必须锁上

漏掉一个，那一个就是现成的绕过入口。包括：

- 面板托管的那份；
- **版本库里留给回滚用的历史版本**（`versions/<版本>/`）；
- 官方安装器的版本库、下载缓存、winget / Scoop / PATH 上的、编辑器扩展自带的。

唯一的例外是桌面端 `app-<版本>\claude.exe` —— 给它加 Deny ACE 会让桌面端开新窗口
就崩，只能靠看门狗收。

---

## 文案里不要写 Markdown 的 `**`

Rust 侧传给界面的 `detail` / `manual` 之类是**纯文本渲染**的，
写 `**强调**` 会在界面上显示成两个星号。要强调就在 TSX 里用 `<strong>`。

（源码注释里的 `**` 不受影响，那是给读代码的人看的。）
