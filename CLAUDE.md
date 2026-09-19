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

### ⛔ 给使用者的命令一律写 PowerShell 形式

**使用者的终端是 Windows PowerShell，不是 Git Bash。** 这不是风格偏好 ——
下面这些 bash 写法在他那里是**当场报错**：

| bash 写法 | 在 PowerShell 里的下场 | 该写成 |
|---|---|---|
| `A && B` | 语法错误（5.1 没有管道链操作符） | 逐条列出，或 `A; if ($?) { B }` |
| `sha256sum f` | `CommandNotFoundException` | `Get-FileHash f -Algorithm SHA256` |
| `/d/claude-gate/…` | 不是有效路径 | `D:\claude-gate\…` |
| `2>/dev/null` | 建出一个名叫 `null` 的文件 | `2>$null` |
| `head -n 5 f` | 没有这个命令 | `Get-Content f -TotalCount 5` |

实测栽过一次（2026-09-15）：给他的安装包校验命令用了 `sha256sum`，
他照着最显眼的那条跑，当场 CommandNotFound。

⚠ **`A; if ($?) { B }; if ($?) { C }` 是错的** —— 第二个 `$?` 反映的是上一个
`if` 语句的结果，不是 `B` 的。要串就嵌套，或者干脆逐条跑。

### 收工清单

前端六项，**逐条跑，任何一条红了就停**（PowerShell 没有 `&&`，别伪装成一行）：

```powershell
npm run build
npm test
npm run types:check
npm run format:check
npm run test:ui
npm run release:check
```

Rust 四项，同样逐条：

```powershell
cargo test --workspace
cargo clippy --workspace --lib -- -D warnings
cargo fmt --all -- --check
cargo deny check
```

出安装包（release 编译约 3–5 分钟）。**产出安装包不等于改完了，还要装** ——
见本节开头那一条：

```powershell
npm run tauri build
```

**这两组就是 CI 跑的那一套**，别只跑前两条就以为绿了 —— `test:ui`（72 个响应式/
主题组合 + 6 个交互流程）和 `cargo deny`（advisories / bans / licenses / sources）
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
| **设备指纹伪装**（UUID / 主机名 / MAC / machine-id 改写） | 主要用途是多账号规避，与硬约束「所有账户必须本人拥有」直接冲突。DISCLAIMER 写死了「不对账户状态作任何承诺」与「不为规避封禁而设计，也无法达到该目的」（按这两句话去搜，别记行号 —— 行号每改一次 DISCLAIMER 就漂一次） |
| **内置代理 / VPN**（自己当代理、mTLS 中继、链式转发、把**整机**流量强制走代理） | DISCLAIMER 第 5 节。做了这个，那一节就是假的。⚠ 0.19.0 开了两个**有边界的**口子（浏览器出站锁、系统代理修改），见下面「两个口子」一节 —— 那两个口子之外，这一条照旧 |
| **自动换号**（按额度、429、限流自动切槽位） | 合规边界四条的前两条。存在这条路径，「多槽位」的定位就从「管理你自己的账户」变成了「规避限制」 |
| **联网查额度**（调 OAuth 内部接口、抓 `/usage` 背后的端点） | 未公开接口，上游一改就断；而且 DISCLAIMER 写着不调它。用量只许读官方客户端**自己写在本机的文件**（`accounts/usage.rs`），零网络请求，只用于显示，面板不据此做任何决定 |

检测与**如实报告**不在此列 —— 面板可以告诉你「你的时区和出口对不上」，
但不替你改机器身份。两者的区别是：前者让使用者知情，后者替他伪装。

### 本机路由不在此列（0.14.0，使用者定的）

中转站那个只绑 `127.0.0.1` 的 API 路由器**是允许的**，别按上面那一行把它删掉。
使用者的原话：「这个只是中转站，而文档里说不做的是账户，两者不一样。」

分界线是**它替谁做决定**：

| | 允许 | 不允许 |
|---|---|---|
| 换的是什么 | 你自己填进去的中转站 | 官方 OAuth 账户槽位 |
| 为什么可以 | 你自己买的 Key、第三方端点，Anthropic 那边看不见 | 按额度 / 429 自动切槽位 = 「自动换号」，合规边界头两条 |

两条路径在代码上是隔死的：`qb-station` 不依赖 `qb-accounts` / `qb-launch`，
`src-tauri/tests/architecture.rs` 的 `relay_breakers_can_never_reach_account_switching`
钉着这件事。**别为了图省事把那条测试删掉或者往 `ALLOWED_SIDEWAYS` 里补一行。**

路由器本身的三条硬约束（`crates/qb-app/src/local_router.rs`）：只绑回环、
不改系统代理设置、不做链式转发。这三条是 DISCLAIMER 第 5.1 节的措辞依据 ——
**动了其中任何一条，就要同步改那一节**，否则免责声明描述的是另一个软件。

### 两个口子（0.19.0，使用者定的）

0.22.1 使用者另行明确要求：IP 纯净度中的「禁用本机 IPv6」默认开启，启动时应用，
关闭开关时恢复每张网卡原值。这是 IPv6 网卡绑定的独立例外，不改变下面代理与
防火墙的点击约束。原值必须先持久化；UAC 拒绝、部分失败、网卡移除时保留恢复记录，
界面以实际读取的绑定状态报告结果。启动应用完成后才恢复租约和启动看门狗。

使用者明确要求做这两件事，于是上面那条「内置代理 / VPN」开了两个**有边界的**口子。
`DISCLAIMER` 第 5.2 节是它们对外的措辞依据 —— **动了下面任何一条边界，就要同步改那一节**。

| 口子 | 允许 | 不允许 |
|---|---|---|
| **浏览器出站锁** | 给使用者点名的**那一个**浏览器 exe 加 Windows 防火墙**出站**规则：只放行指定的 VPN / TUN 接口 | 碰别的程序；加入站规则；改路由表、改 DNS、做转发；「全局」模式 |
| **系统代理修改** | 使用者当次点了「修」才改，改前把原值记下来，面板里能回滚 | 自动改；启动时改；看门狗或任何定时器碰它 |

三条硬约束，跟本机路由那三条同级：

1. **按 exe 路径限定**，不按进程名、不按端口。规则名统一前缀 `QB Gate - `，
   让使用者自己在防火墙里也认得出、删得掉；
2. **加了什么必须看得见**：面板要能列出当前由它加的每一条规则，并一键撤销。
   看不见的规则等于埋雷 —— 面板卸载之后规则还在，而使用者根本不知道浏览器
   为什么上不了网；
3. **绝不自动加**。这两件事都只在使用者当次点击之后执行，没有定时器、没有启动时触发。
   跟「账户切换只能人工触发」同一类：会改变系统行为的动作，不许自己发生。

### 官方 Codex 的 turn-state 口子（0.24.0，使用者定的）

使用者明确要求：把 ccodex-sleep-state 的 turn-state 机制 clean-room 重写、**只用在官方 Codex** 上
（「只改 GPT Codex，不改 Claude」「本机路由只路由 `127.0.0.1`，不属于当初决定不引入的那部分」）。
代码在 `crates/qb-station/src/turnstate.rs`（外形解析 + active/ready，纯函数）、
`crates/qb-station/src/sse.rs`（SSE 终止事件分类，纯函数）、
`crates/qb-app/src/local_router.rs`（`UpstreamAuth::OAuthPassthrough` 官方模式 + 采集/注入）。

六条硬约束，动了任何一条就要同步改 [DISCLAIMER.md](DISCLAIMER.md) §5.3 与 [ATTRIBUTION.md](ATTRIBUTION.md)：

1. **只对 `Client::Codex`。** Claude Code / 桌面端一律不进官方模式、不注入、不采集 ——
   turn-state 是 OpenAI 的请求头，Anthropic 协议里没有对应物。有测试
   `turn_state_stays_off_for_claude_routes` 钉着。
2. **一行 ccodex 源码都不许抄。** 它是第三方 GPL，抄进来堵死商业授权档（见「抄代码之前先看 license」）。
   只按公开协议行为自己写，Mihomo / 订阅 / 出站协议 / 代理出口池一概不引入。
3. **不「换出口凑 292」。** 没有代理出口池，只在当前这一条连接上采集。
4. **被动采集，不发合成探测请求。** 只读官方响应本就带回的头喂给 `Store::offer`；
   不额外发模型请求烧额度。客户端自己带了 turn-state 就保留它的，不覆盖（不弄坏它自己的状态）。
5. **`OAuthPassthrough` 是「路由不承载官方身份」的唯一受控例外。** 只有它保留客户端的 OAuth、
   不剥不换；其余上游仍走 `CLIENT_AUTH_HEADERS` 剥离 + 换上线路自己的 Key。仍只绑回环、
   不改系统代理、不做链式转发。
6. **长度不是质量/额度指标。** 界面与文案照 ccodex 自己的口径写：符合 292/332 不证明任何服务端事实，
   也不增加额度。默认关闭，只在使用者手动开启后生效。

把官方 Codex 接进路由那段接线在 `crates/qb-app/src/usecase/turnstate_ops.rs`（槽位接管：
`apply_to_config` / `revert_config` 两个纯函数 + `enable` / `disable` + 落盘 marker）与
`src-tauri/src/commands/station.rs`（`station_turnstate_enable` / `station_turnstate_disable`）。
**0.24.7 起识别直接作用于当前激活的 Codex 账户槽位** —— 不再造 `qb-router-codex-official`
分身环境、不再复制 OAuth、不另起第二个 Codex（使用者批准推翻 0.24.0 的做法；老做法会让两份
Codex 各自轮换同一族刷新令牌而互相登出）。四条容易踩的：

- **官方 base_url 不带 `/v1`**（官方端点是 `.../backend-api/codex/responses`）；中转 Codex 才带。
  差一段 404 而看不出来。细节见 `docs/DESIGN-NOTES.zh-CN.md`「识别直接作用于槽位」一节。
- **只改槽位 `config.toml` 的两处（`model_provider` + `[model_providers.qb_turnstate]`），
  `auth.json` 一个字不碰。** 关闭时按 marker **反向恢复这两处，不整份盖回备份** —— 整份盖正是
  §7.10「auth.json 覆盖登出」那类事故的形状；接管期间 Codex 自己写进去的项目授权 / MCP 要留着。
- **marker（`state_dir/turnstate-takeover.json`）是「识别开着」的唯一真相。** 面板启动时发现它
  就**自动关闭并恢复**（`lib.rs` 的 setup 里；本机路由随面板消失，留着会让那个槽位的 Codex
  对着死端口）—— 这也是「默认关闭、只在手动开启后生效」那条硬约束的落地。互斥守卫看它，
  不只看内存里的 `official_codex_armed()`（面板重启后后者归零，前者不会）。
- **挂官方上游是显式动作**（`station_turnstate_enable`），不搭在注入开关上；`replace_upstreams`
  会保住那条 `OAuthPassthrough` 上游，别让中转线路重装把它冲掉。

**代理 / 订阅 / 机场出站 / 换出口凑 292 不在核心里**（0.24.2，使用者定的）：它们归**独立的
第三方程序** `ccodex-sleep-state`（GPL），扩展中心只以 `connect` 方式接入它
（`crates/qb-extensions/src/plugins/codex_egress.rs`：定位 exe、带窗口独立起 `setup`、
停止 = 结束进程 + 跑它自己的 `restore`；一键安装只下载它 Releases 里的**成品二进制包**并按
`SHA256SUMS` 校验，`pick_release` 只认 github.com 的下载地址）。**不编译、不复制它一行源码** ——
GPL 聚合而非衍生，商业档才不受影响。它接管的是**当前激活 Codex 槽位**的目录（`CODEX_HOME`
显式传给它，接管的目录记进托管记录，`restore` 恢复当初那份），与官方识别**双向互斥**：
0.24.7 起两边**都要改同一个槽位的 `config.toml`**（识别改 `model_provider`，插件整份接管），
同时开就是两个程序抢同一个文件。`codex_egress_start` 里的
`official_codex_armed() || turnstate_ops::takeover().is_some()` 守卫和 `station_turnstate_enable`
里的 `codex_egress::running()` 守卫都别删；解开靠 `station_turnstate_disable`（0.24.6 之前
挂上官方线就没有任何地方能摘掉，使用者被锁死在两者之间）。

---

## ⛔ 中转环境的 base_url 指的是本机路由，不是站点

`Environment.via_router` 为 `true` 的那些环境（`qb-router-<软件>`，
一个软件一个，`workspace::router_environment` 自动建）：

- base_url 走 `local_router::client_base_url(client, DEFAULT_PORT)`，
  **不看 provider 上那个地址** —— 看了的话客户端会绕过路由直连站点：
  换上游不生效、日志一条都不记、熔断永远不触发，而每一发请求都成功；
- Key 填 `local_router::ROUTER_KEY` 这个占位串。路由会把客户端带上来的
  鉴权头整个换掉，所以填什么都到不了站点；但**留空不行** —— 客户端发现
  没有 Key 会转去走官方 OAuth，而那条路会把一个官方身份塞进中转环境目录；
- 端口写死默认值。配置是落到磁盘上的，换个端口起路由会让已经写好的那份
  指向一个没人听的端口，而客户端报的是「连不上」。要支持自定义端口的话，
  端口得先变成环境自己的字段。

钉着这件事的是 `a_router_environment_points_at_the_loopback_not_at_the_station`
和 `the_base_url_we_hand_out_is_the_one_route_of_reads_back`（后者管前缀两头
对不对得上 —— 一头加了 `/cd` 另一头不剥，上游收到的是 404）。

**那份配置一个软件一份，不是一条线路一份。** 换上游不重启客户端，
客户端只在启动时读一次配置 —— 六条线轮着走用的是同一份 settings.json。
做成一条线一份的话，改另外五条完全没有反应，而没有任何地方说得清为什么。

---

### 桌面端走的是它自己的「第三方网关」模式，不是环境变量里的 base_url

0.17.0 之前这里写着「桌面端不支持独立中转环境」，`launch` 里那一档给的是
`vec![]`，注释说它不吃环境变量。**那是个没验过的假设。** 读它的
`app.asar`（1.52386.6 上逐个字符串核过）之后是这样：

| 要换什么 | 怎么换 |
|---|---|
| 数据目录 | `CLAUDE_USER_DATA_DIR` 环境变量。**它优先级最高** —— 桌面端自己那套 3p 目录重定位包在 `if (!process.env.CLAUDE_USER_DATA_DIR)` 里 |
| 推理端点 | 它自己那份 `claude_desktop_config.json` 里的 `deploymentMode: "3p"` + `inferenceProvider: "gateway"` + `inferenceGatewayBaseUrl` |

所以桌面端跟 Claude Code 是**同一个套路**（起进程时指一个独立目录，
官方那份一个字不动），只是「端点写在哪」不一样：Claude Code 在环境变量里，
桌面端在配置文件里。写配置的是 `workspace::desktop_gateway_json`。

三条硬规矩：

- **只并入，不整份覆盖。** 这个文件同时装着 `mcpServers` 和 `preferences` ——
  整份写会把使用者配了半天的 MCP 抹掉，而且没有撤销。跟 `codex_auth_json`
  那条是同一个教训；
- **有两个键故意不写**：`disableDeploymentModeChooser`（从使用者手里拿走
  「切回官方」那个开关）和 `coworkEgressAllowedHosts: ["*"]`（放宽一道
  安全限制）。中转站要的只是换个推理端点，不需要顺带把别的口子也打开；
- **base 不带 `/v1`。** 桌面端把 `inferenceGatewayBaseUrl` 当前缀、自己往后
  接 `/v1/...`。0.17.0 之前桌面端落在 Codex 那个分支上（多接一个 `/v1`），
  当时看不出症状是因为它根本起不来；现在起得来了，多一段就是 404。

### ⛔ 桌面端是 Electron 单实例，起之前必须先退干净

已经在跑的时候再起一遍，只会把旧窗口拉到前面，**新给的环境变量一个都不生效**。
不拦的话症状是「点了启动，窗口是弹出来了，可它走的还是官方」—— 没有任何报错。

0.18.2 起，中转站的「启动」**替使用者把它关掉**，不再报错让人去托盘退出
（使用者的原话：「图片内的动作自己不能做吗，非要用户手动」）。
`station_launch` 先调 `workspace::close_desktop_for_relay`：面板起的桌面端会话按会话停、
交回租约；其余的走 `killswitch::execute_desktop` —— 跟一键关闭**同一套**证据、
祖先链否决和 PID + 创建时间核验，只收 `Role::Desktop`，终端里的 Claude Code 不碰；
等进程真的从进程表里消失才往下走，**关不干净就报错、不启动**。

三处都要留：

- 界面上启动按钮**常驻**一句「开着的桌面端会先被关掉」—— 代价事前说，不放悬停提示；
- `close_desktop_for_relay` 关不干净就停，不许「关了一半接着起」；
- `workspace::launch` 里那道拦截（`desktop_processes()` 非空就报错）**不许删** ——
  别的入口调进来不会先关，那一道是兜底。

⚠ 调试时别在真机上点「启动 桌面端」：Claude 桌面端 Code 页里跑着的会话也算桌面端，
会被一起关掉 —— 如果你正是在那里面干活，就是把自己关了。

---

## ⛔ 智能调度是**落盘的承诺**，而且面板关了它就停

0.16.0 之前「智能调度」只是 React 的一个 `useState`：开着的时候界面变个样，
而没有任何东西在换上游。刷新页面就没了 —— 使用者以为它一直在盯着，
实际上从点下去那一刻起什么都没发生过。

现在它是 `station_schedule` 那张表里的一行（一个软件一行），面板启动时
会把上次开着的接回来（`lib.rs` 的 setup 里）。相应地：

- **界面上必须写明「面板关掉即停」**。循环活在面板进程里，关掉面板调度
  就不再换上游了，而客户端还在照着上一次选的那条线跑 —— 可能是几天前
  那条已经涨价的。这不是缺陷，是这个软件的定位（它是个面板，不是后台服务），
  但不说清楚就是骗人；
- **一跳 60 秒，别调小。** 每一跳给池子里每条线各拉一次站点账单。那是免费的
  （不打模型），但十条线的池子在 10 秒档上就是每分钟 60 个请求打到账单接口，
  有些站点会因此限流 —— 而限流的表现是健康度读不到，排序退化成只比倍率。
  **盯得太紧反而让它瞎掉**；
- **迟滞只有一道**，在 `schedule::rank`（它拿着 `incumbent`）。
  `next_upstream` 只问一句「赢家跟现任是不是同一条」——
  再判一次就是两道迟滞叠在一起，结果是永远不换，而且没人说得清为什么；
- **熔断中或没过底线的赢家不换。** 底线全卡光时 `rank` 会放开底线重排一轮，
  那是为了不制造死局（界面上还看得见名次），**不是**为了把不合格的送上生产。

调度循环放在 `commands/station.rs`（接口层），不放在 `app.rs`。
放 `app.rs` 里会让 `app → commands → app` 成环，`architecture.rs` 的
`module_cycles_only_ever_shrink` 当场抓住过一次。

---

## ⛔ 抄代码之前先看 license

上游的许可状况逐条记在 [ATTRIBUTION.md](ATTRIBUTION.md)。三条铁律：

1. **MIT 可以抄**，保留版权声明即可（与本项目的 AGPL-3.0 相容；MIT 允许再许可，
   所以商业授权那一档也过得去 —— 但**原版权声明必须一路带着**）；
   同理适用于 Apache-2.0、BSD、ISC 这些宽松许可。
   ⚠ **别人的 GPL / AGPL 代码现在抄不得了**：双授权之后，抄进来的 copyleft 代码
   无法再许可给商业授权那一档，会把商业这一档直接堵死。详见
   [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md) 第三节；
2. **没有 license 文件 = 保留全部权利，一行都不能抄**。看可以看，实现必须自己写。
   目前已知：`dai-chao/Agent-Guard`、`iprisk-top`、`Trentct/claude-code-ban-risk`、
   `jlcodes/cockpit-tools`；
3. **CC BY-NC-SA 不能抄**：SA 会把本项目拖成同一个协议，NC 会禁止商业使用。

抄了什么、没抄什么、为什么没抄，都要写进 ATTRIBUTION.md —— 这不是礼貌，
是社区开源推广申明里承诺过的事。

### 本项目自己的许可是 AGPL-3.0-only **+ 第 7 节附加条款**（0.24.8，使用者定的）

使用者要的是「别人改了拿去发，必须署名并链接回本仓库」。协议做不到「必须以 fork 形式发布」
（那是 GitHub 的功能，不是许可条款；写了也不可执行，而且项目就不再是开源项目），
能做到的极限是 AGPL 第 7 节允许的三类附加条款，写在
[LICENSE-ADDITIONAL-TERMS.md](LICENSE-ADDITIONAL-TERMS.md)：(b) 保留指定署名（含仓库地址）、
(c) 改版必须标明、不得冒充原版、(e) 不授予「QB Gate」名称与图标。四条硬规矩：

1. **只能是 §7 (a)–(f) 列出的那几类。** 加任何别的限制（「必须回馈上游」「不得商用」）
   就是 §7 所称的「进一步限制」，接收者有权删掉，而且整个项目从此不算开源；
2. **§7 要求「在相关源文件里指明去哪里找附加条款」**：各 crate 的 `lib.rs`、`src-tauri/src/main.rs`、
   `src/main.tsx` 头上那三行注释就是这个，别当成普通注释删掉；README 授权声明、
   安装包 `licenses/`、Release 附件、设置页「关于」四处都指着那份文件，
   `npm run release:check` 逐处断言；
3. **设置页「关于」里那行「基于 QB Gate，版权所有 (C) 2026 smithtaylor7748-ops。源码：…」
   措辞不许改** —— 它就是附加条款第 1 条要求下游保留的那一句，原版先照做，下游只要不删就合规。
   改了措辞，条款和界面就对不上；
4. 商业授权那一档可以豁免这三条（版权集中在一人手里），但 AGPL 这一档不行。
   英文文本为准，中文是译文 —— 跟 LICENSE 本身一样。

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

## ⛔ 颜色只在 `src/styles/tokens.css` 里定义

v0.13.1 之前 `tokens.css` 和 `workspace.css` 各有一套色值、两套主题机制。
手选「浅色 / 深色」看不出问题，**默认的「跟随系统」却是两套拼出来的** ——
边框、状态色来自一套，背景、正文来自另一套。`test:ui` 一直全绿。

**它为什么没拦住，比那个 bug 本身更值得记。** 那一圈截图先 `goto(origin)`、
写 `localStorage` 的 `qb-theme`，再 `goto(origin + "/#/" + route)` —— 最后这步
从 `origin` 出发只改 fragment，是**同文档导航，不重新加载**。主题在模块加载时
读一次就定下来了，于是整轮拍到的都是**上一轮**的主题：light 轮拍成「跟随系统」，
dark 轮拍成「浅色」。`docs/screenshots/*-dark.png` 从来就不是深色的，
而 72 条断言全绿 —— 那圈里的「主题」这一维是死的，两轮渲染的是同一套颜色。
（真正渲染过深色的只有后面 DPI 那两轮 —— 它们用 `colorScheme` 建独立 context，
但只断言宽度、不截图。）

0.13.1 补了两道：写完 `localStorage` 真 `reload()` 一次；每张截图落盘前断言
`document.documentElement.dataset.theme` 确实是这一轮那个 —— 文件名不许撒谎。
**以后往这圈里加主题/视口维度，先问一句「它真的重新加载了吗」。**

现在 `scripts/ui-regression.mjs` 还逐个令牌比对「跟随系统」与手选主题，
`tokens.css` 里两份浅色改了一份漏了另一份，当场红。但它只看得见 `:root` 上的变量 ——
组件样式里直接写 hex，它照样拦不住。所以：

- 别的样式表、组件、新页面**一律引用变量**，不另起色值；
- 中转站草图（V24）用的是**合并前的旧配色**，照草图做页面时**不要从草图里抄 hex**。

---

## ⛔ 桌面那份旧副本会抢 1420 端口

`.claude/launch.json` 里的 dev server 用 1420。**桌面那份旧仓库
（`%USERPROFILE%\OneDrive\桌面\claude-gate`）如果也起着 `npm run demo`，
它会先占住这个端口**，于是：

- 新起的 dev server 静默退出（端口被占），
- 浏览器打开 1420 看到的是**旧代码**，
- 而 `npm run build`、`npm run test:ui` 全是绿的 —— 它们各起各的端口。

症状是「我明明改了，界面上没变」。实测栽过一次：
改完的副标题在浏览器里还是旧的那句，查了三轮才发现监听 1420 的那个
node 进程的命令行指向 `OneDrive\桌面`。

**查法**（一行就看得出来）：

```powershell
Get-CimInstance Win32_Process -Filter "Name='node.exe'" |
  Where-Object { $_.CommandLine -match 'vite' } |
  ForEach-Object { $_.CommandLine }
```

命令行里出现 `OneDrive` 就是旧副本在跑。根治办法是把桌面那份删掉 ——
两份同名同分支的仓库并存，本来就是「对着新代码看旧行为」的入口。

### 不抢端口也会中招：起 dev server 的**工作目录**可能就是旧副本

0.15.0 又栽了一次，而且这次**没有端口冲突** —— 1420 上没有别的进程。
会话是从桌面那份目录起的，于是助手的 `preview_start` 读的是**桌面那份的**
`.claude/launch.json`、`npm run demo` 也在**桌面那份**里跑。浏览器打开 1420，
拿到的是 0.12.2 的界面：新加的三级选择器整个不存在，而看起来只像「功能没生效」。

**一眼认出来的办法**：看 dev server 启动时那行 banner 上的包名版本。

```
> qb-gate@0.12.2 demo      ← 仓库里是 0.15.0，这就是旧副本
```

**别只看端口占用，先看版本号。** 端口空着不等于跑的是这个仓库。
从别处起 dev server 时把根显式钉死：

```bash
npx vite --root D:/claude-gate --mode demo --port 1421 --strictPort
```

---

## `test:ui` 报「page.goto: Timeout」时,先怀疑热身没跑完

vite **按住所有模块请求**直到依赖预打包跑完 —— 冷缓存下要 40–60 秒。
首页这时已经回 200 了,所以只等首页就开测的话,第一发导航会卡在
DOMContentLoaded 上(文档 commit 了、`readyState` 停在 `interactive`,
而 deferred 的 module script 永远没回来),30 秒后报成浏览器超时。

**这个错会把人带偏**:服务器 curl 得通,单独开个浏览器也打得开
(那时缓存已经热了),于是看起来像 Playwright 或 Edge 坏了。
排查方法是看**哪个请求一直挂着** —— `/@vite/client` 和 `/src/main.tsx`
同时 pending 就是这件事。

`scripts/ui-regression.mjs` 现在等的是「模块真的服务得出来」而不是「首页回 200」。
**新克隆的仓库第一次跑必然撞上这个**,别把那个等待去掉。

顺带:别在 `test:ui` 跑着的时候手动开 `npx vite` —— 两个 dev server 对着同一个
`node_modules/.vite` 互相重写,症状一模一样。而且 `npx` 被杀掉时不会带走
它的子进程,残留的那个会继续捣乱。

---

## ⛔ 「倍率」不是一个数

中转站的计费是四类各算各的，而且输出那一类还要再乘一次：

```text
输入   = model_ratio
输出   = model_ratio × completion_ratio      ← 「计费翻倍」，常见 3~5 倍
缓存读 = model_ratio × cache_ratio
缓存写 = model_ratio × create_cache_ratio
再乘   × 分组倍率 ×（峰时浮动，取 max 当上界）
```

所以**只比 `model_ratio` 会选错站**：A 站 ×0.15 开翻倍、B 站 ×0.4 不翻倍，
纯读代码 A 便宜、长篇生成 B 便宜。判定在
`pricing::StationRates::blended_ratio` —— 按这条线**实际的输入输出比**加权。

### ⛔ 而且「倍率」这个词只对一半的站点成立

两种后端公布价格的方式根本不同，**混着算必然错**：

| 站点后端 | 它在 API 里给什么 | 长什么样 | 折扣在哪 |
|---|---|---|---|
| New API 系 | 相对倍率 | `model_ratio: 0.2` | 倍率本身 |
| sub2api 系 | **绝对单价** | `input_price: 5`（美元／百万） | 分组倍率 `group_ratio` |

使用者的原话：「newapi 是可以这样算，但 sub2api 不是，
sub2api 的单价就是官方单价。」—— sub2api 报的 5 / 25 就是官方价本身，
便宜全在分组上。拿 `model_ratio` 那条路去读它，读出的是一片空白。

`StationRates` 因此有**两套互斥的字段**，哪一套有效由 `basis()` 从
「谁填了」推出来。**不设一个单独的 kind 字段** —— 字段和内容对不上时
（适配器改了、迁移漏了、手改过配置），字段会骗人而内容不会。

- `category_ratios()` 只管倍率那一套，绝对单价那种站点在这里全 `None`；
- `category_prices()` 只管绝对单价那一套，倍率那种站点全 `None`；
- **两套不互相换算。** 换算要除以官方价，而那正是下面那条静默失效的入口。
  要合流只有一个口子：`category_ratios_against(official)`；
- 绝对单价那一套换成倍率**必须知道是哪个模型的**（官方价按模型查），
  所以 `Route` 有 `rates_model`。倍率那一套不需要它 —— 早先没有也看不出症状。

### ⛔ 这张四类表**不下判定**，一次也不许再长出来

`CategoryVerdict` 曾经有 `real_multiplier` 和 `advertised` 两栏，注释写着
「真实倍率 = 站点单价 × 标称倍率 ÷ 官方单价」。那个算式在真实调用路径上
**恒等于站点自己公布的两个数相乘** —— 调用方手里从来没有站点的绝对单价，
它拿站点公布的倍率乘上官方价凑出一个「站点单价」，`verdicts` 再除回去：

```text
凑出来的单价    = category_ratio × official
real_multiplier = 凑出来的单价 × advertised ÷ official
                = category_ratio × advertised        ← official 被约掉了
```

**站点说它便宜，这张表就说它便宜，而 20 条单测全绿。** 这是「看起来通过了、
实际什么都没测」的那一类失效，比没有这项检查更危险。

「它到底收了几倍」只有 `pricing::measured_multiplier` 答得了：
分子是账单实扣、分母是同一批 token 按**官方价**算出来的成本，
两头都不是站点公布的数。钉着这件事的是
`the_official_price_actually_changes_the_measured_multiplier` ——
**它只改官方价、别的都不动，结论必须跟着变**。删了它，那条失效随时会回来。

配套的三条：

- **界面必须显示「计费翻倍」**（`StationCenter.tsx` 的 `FoldPill`）。
  站点没公布 `completion_ratio` 时什么都不显示 —— **「不知道」不是「没翻倍」**；
- **新对话没有缓存读**（`TokenMix::fresh_conversation`）。靠高缓存命中撑起来的
  便宜线，在新对话第一轮上并不便宜；
- **有真实账单就用真实账单**。`24h 实扣 ÷ 实际 token` 排在加权倍率前面 ——
  那是实际付出去的钱，比任何推算都准。

### 官方参考价没有 API，只能抓文档页

**地址**（2026-09-14 实访确认，改之前先 curl）：

| | 地址 | 表格 |
|---|---|---|
| Anthropic | `platform.claude.com/docs/en/about-claude/pricing.md` | `## Model pricing` 一节，5 列：`基础输入｜5m 缓存写｜1h 缓存写｜缓存读｜输出`，模型列是**显示名** |
| OpenAI | `developers.openai.com/api/docs/pricing.md` | `### Standard pricing data` 一节，8 列：短上下文 4 列 + **长上下文 4 列** |

⛔ **三条都是踩出来的：**

1. **地址会 404 而且没有症状。** 之前写的 `docs/en/pricing.md`（少了
   `about-claude/`）是 404 —— 更新永远失败、永远悄悄沿用内置快照；
2. **必须按小节过滤。** 两个页面都有好几张表，同一个模型在批处理表里是
   **五折价**。靠「同名只取第一个」纯属表格顺序的运气，上游一调顺序就会
   采用批处理价，于是每一家站点都被算成超收两倍；
3. **两家格式完全不同，配反了会读出错价。** Anthropic 取「前两个金额」会把
   5m 缓存写当成输出（Opus 5 读成 $5/$6.25，实际 $5/$25）；OpenAI 只读短上下文
   会漏掉长上下文那一倍。`PricingFormat` 跟 URL 绑在一起，有测试钉着
   「拿错格式解析必须返回空」。

**核对工具**：`cargo run -p qb-station --example parse-live-pricing -- <文件> <anthropic|openai>`。
页面改版时先跑它，看解析出几个模型、价对不对。



Models API 只回 id / 上下文窗口 / 能力位，**没有价格字段**。所以
`pricing::TABLE` 是编译进去的快照，启动时抓 `pricing.md` 更新。

⛔ **解析不出来就整批丢弃**（`MIN_PARSED_MODELS`）。页面改版时解析器往往还能
凑巧认出一两行，那种「半成功」会用一份残缺的表盖掉完整的快照 ——
后果是大部分模型的真实倍率悄悄变成「不知道」，而界面上看起来只是「都没检验过」。

缓存两项除非官方单独标过，否则是按标准倍数推的，`PriceBasis` 标明是哪种。
**Fable 5.1 的缓存读价是官方单独标的 0.25，不是输入价的 0.1 倍**（那会算成 1.0，差 4 倍）——
这就是「推出来的价会错」的实例。

---

## ⛔ 可信度与「证据档次」是两件事

可信度 29 分有两种完全相反的来源：

| 来源 | 该做什么 |
|---|---|
| 六项都测了、四项对不上 | 这站确实有问题，**换站** |
| 只测到一项、其余没测到 | 还不能下结论，**再跑一轮** |

只给一个分数的话，两者在界面上长得一模一样。所以 `AuditRound` 同时带
`trust` 和 `evidence`（`EvidenceLevel`），**界面必须两个一起显示**。

---

## 文案里不要写 Markdown 的 `**`

Rust 侧传给界面的 `detail` / `manual` 之类是**纯文本渲染**的，
写 `**强调**` 会在界面上显示成两个星号。要强调就在 TSX 里用 `<strong>`。

（源码注释里的 `**` 不受影响，那是给读代码的人看的。）
