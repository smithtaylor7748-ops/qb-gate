# 改这个仓库之前先读这里

给接手这个项目的人（以及 AI 助手）看的硬规矩。每一条都是踩出来的，不是推理出来的。

实现层面的「为什么这么写」在 [docs/DESIGN-NOTES.zh-CN.md](docs/DESIGN-NOTES.zh-CN.md)，
修不掉的缺口在 [docs/KNOWN-ISSUES.zh-CN.md](docs/KNOWN-ISSUES.zh-CN.md)。

---

## ⛔ 一轮需求做完了，先问，别自己去出安装包

**做完使用者这一轮的需求之后，收工清单跑完、文档更新完，停下来问一句：
「出安装包，还是你还有别的需求？」**（2026-09-20 使用者定的）

两条路，别自己替他选：

| 他的回答         | 你做什么                                                                                                                                   |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| **还有别的需求** | 只把文档更新到位（`CHANGELOG` 的 `## 未发布` 段 / `KNOWN-ISSUES` / `DESIGN-NOTES` / 本文件 / 桌面 `claude-op` 档案），**不出安装包，也不提版本号**。他会另开一轮对话接着提 |
| **出安装包**     | **这时才提版本号**（五个位置，见「收工清单」第 3 步），把 `CHANGELOG` 的 `## 未发布` 换成这一版的标题，再跑 `npm run tauri build`，然后给他一条 **PowerShell** 更新本机那份的指令（见下面「更新本机那份」） |

**为什么要问**：出一次 release 编译要 3–5 分钟，而他常常是一连几轮需求一起攒着做。
一轮做完就出一次包，出的全是中间版本 —— 他装不过来，装了也马上过时。
更要紧的是：**面板起着这个桌面端的 Job，装新版会把正在干活的这条对话一起关掉**（§7.38），
所以装这件事从来都是他自己来，不是你。

### ⛔ 版本号只在「出包」那一刻提（2026-09-22 使用者定的）

**一轮需求 ≠ 一个版本。** 这是使用者自己发现的：哪怕只是改一句文案，收工清单也照样
把版本号往前提一格 —— 0.25.0 到 0.32.0 是三天走完的八个版本，而真正装到他机器上的
只有两三个。根因在收工清单的顺序上：**原来是先提版本号、再问出不出包**，
于是答「还有别的需求」的那几轮，版本号也已经提掉了。

改成**先问、后提**：

| 时机                       | 版本号               | `CHANGELOG`                        |
| -------------------------- | -------------------- | ---------------------------------- |
| 一轮做完、他还有别的需求   | **一个字都不动**     | 追加到 `## 未发布` 那一段底下      |
| 他答「出包」               | 这时才提（五个位置） | `## 未发布` → `## <版本> — <日期>` |

三条理由，每一条都真的发生过：

1. **提版本号 = 整个 workspace 重编。** 版本号在仓库根 `Cargo.toml` 的
   `[workspace.package]` 里，其余 13 个 crate 全从那里取 —— 改它等于让所有 crate 失效。
   D 盘被 `target/` 撑满两次（72 GB、16.9 GB）都是这么来的，坑 7.67。
2. **每一版都是一个零信誉的新哈希。** 杀软按哈希放行，多发一版就要多申诉一次，
   信誉也就永远攒不起来 —— 见「杀软会把这个面板当木马」那一节。
3. **文档里引用的版本号必须真的存在那一版。** 版本号提得比发布快，文档里就会站着
   一堆从来没有人装过的版本号，下一个人照着它排查，查的是一个不存在的东西。

⚠ **这条规矩只有一半有测试守着。** `npm run release:check` 核那五处版本号彼此一致，0.25.3 起还核
`.github/release-notes.md` 写的版本号（更新弹窗显示的就是它）；但**不看 `CHANGELOG`** ——
`## 未发布` 停在那里不会让任何一项检查变红。那一半全靠照做。

⛔ **提成多少由使用者定，别替他「纠正」。** 2026-09-24 他先要 0.24.10（「这个是我要求的」），
随即改成 **0.25.1**（「不不不，0.25.1 吧」），发的就是 0.25.1 —— 它在 0.32.0 之后发布、数字却更小，
接的是 09-22 那一轮提出来的 0.24.9。下一版提多少，出包那一刻先问他；别按「应该比 0.32.0 大」
自己改成 0.33.0，也别把它当成笔误去修。他改口了就照最后那句做（先停掉已经在编的那一版）。
（安装包比版本用的是 Tauri 的 `nsis_tauri_utils::SemverCompare`（按语义化版本逐段比数字，2026-09-24 从
CLI 二进制里核过）：0.25.1 装在 0.24.9 上是升级；**装着 0.26–0.32 的机器上它会被当成降级**。
0.25.3 起面板自己的更新提醒也按版本号比（`install::self_update::is_newer`，逐段比数字、只提醒比自己大的）——
所以号由他定，但**必须比 GitHub 上已经发出去的那一版大**，否则装着旧版的人收不到提醒。问号时把这句说给他。
0.26–0.32 从没发到 GitHub 上，只有作者本机装过。）

### 更新本机那份

仓库改好了、本机还跑着旧版，等于什么都没修 —— 而且下次排查时会对着新代码看旧行为，
得出的结论全是错的。这个坑的代价不是「少更新一次」，是**后面每一次调试都在骗自己**。

所以决定要出包的时候，把这两条**一起**给他（PowerShell，不是 bash）：

```powershell
npm run tauri build
```

```powershell
Start-Process "D:\claude-gate\target\release\bundle\nsis\QB Gate_<版本>_x64-setup.exe"
```

⚠ **装之前先确认旧的那份叫什么名字。** 改过名（ClaudeGate → QB Gate）之后，
新安装包**不会覆盖**旧名字那一份，会变成两套并存：两个卸载项、两个快捷方式、两套运行期数据。
先卸旧的再装新的。

### 注意哪些命令不会更新本机那份

| 命令                                | 产出                                                                                      | 会不会动本机装的那份 |
| ----------------------------------- | ----------------------------------------------------------------------------------------- | -------------------- |
| `npm run build`                     | 只出 `dist/`                                                                              | ❌                   |
| `npm run tauri dev` / `npm run dev` | 临时进程，关了就没                                                                        | ❌                   |
| `npm run demo`                      | 同上，而且喂的是假数据                                                                    | ❌                   |
| **`npm run tauri build`**           | `target/release/bundle/nsis/QB Gate_<版本>_x64-setup.exe`（workspace 的 target 在仓库根） | 出安装包，**还要装** |

### ⛔ 给使用者的命令一律写 PowerShell 形式

**使用者的终端是 Windows PowerShell，不是 Git Bash。** 这不是风格偏好 ——
下面这些 bash 写法在他那里是**当场报错**：

| bash 写法          | 在 PowerShell 里的下场           | 该写成                             |
| ------------------ | -------------------------------- | ---------------------------------- |
| `A && B`           | 语法错误（5.1 没有管道链操作符） | 逐条列出，或 `A; if ($?) { B }`    |
| `sha256sum f`      | `CommandNotFoundException`       | `Get-FileHash f -Algorithm SHA256` |
| `/d/claude-gate/…` | 不是有效路径                     | `D:\claude-gate\…`                 |
| `2>/dev/null`      | 建出一个名叫 `null` 的文件       | `2>$null`                          |
| `head -n 5 f`      | 没有这个命令                     | `Get-Content f -TotalCount 5`      |

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

**这两组就是 CI 跑的那一套**，别只跑前两条就以为绿了 —— `test:ui`（2026-09-25 是 163 个响应式/
主题组合 + 17 个交互流程）和 `cargo deny`（advisories / bans / licenses / sources）
各自抓过别处抓不到的东西。`cargo test --workspace` 的通过数**只许涨不许跌**
（0.29.0 是 **1081**，0.30.0 是 **1106**，0.31.0 是 **1110**，0.32.0 是 **1154**，
0.25.1 是 **1204**，0.25.2 是 **1268**，0.25.3 是 **1288**，0.25.4 是 **1334**）。

⛔ **跑完这十项不要接着出安装包** —— 先问「出包还是有别的需求」，见本文件开头那一节。

然后：

1. 更新文档：`CHANGELOG.md`（**写进 `## 未发布` 那一段，别新起一个版本号标题**）、
   `docs/KNOWN-ISSUES.zh-CN.md`（新的限制与未验项）、
   `docs/DESIGN-NOTES.zh-CN.md`（这一轮「为什么这么写」）、本文件（新的硬规矩）、
   以及桌面上的 `claude-op` 档案；
2. **问一句「出安装包还是有别的需求」**，按开头那一节走；
3. **只有他答「出包」才做这一步 —— 版本号往前提。** 不是三处，是**四个文件五个位置**，
   漏一个 `npm run release:check` 当场报错：

   | 文件                        | 位置                                                            |
   | --------------------------- | --------------------------------------------------------------- |
   | `package.json`              | `version`                                                       |
   | `package-lock.json`         | 顶层 `version` **和** `packages[""].version` 两处               |
   | `src-tauri/Cargo.toml`      | `version`（必须写字面量，release-check 拿正则读它）             |
   | `src-tauri/tauri.conf.json` | `version`                                                       |
   | `Cargo.toml`（仓库根）      | `[workspace.package]` 的 `version` —— 其余 13 个 crate 从这里取 |

   **权威是 `npm run release:check`，不是这张表**：表会过期，那条命令不会。
   文档里一旦引用了某个版本号，就必须真的存在那一版。同一步里把 `CHANGELOG.md` 的
   `## 未发布` 换成 `## <版本> — <日期>`，**并把 `.github/release-notes.md` 整份换成这一版的更新说明**
   （0.25.3 起；装着旧版的人在更新弹窗里读到的就是它，`release:check` 核它的版本号），再去 `npm run tauri build`。
   ⛔ 新版本号必须比已经发出去的**大** —— 面板的更新提醒按版本号比，小了就没人收得到（见「应用自己的更新」）。

---

## ⛔ 中转会话不归门禁的**关停**策略管

`LaunchTarget::gated()` 与 `LaunchTarget::stops_with_gate()` 是**两个**判断，
不许合并：

| 问题                                     | 谁回答              | 中转的答案                                                                                                                                                                                  |
| ---------------------------------------- | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 起之前要不要验 IP 解锁、起完要不要持租约 | `gated()`           | **要**。Deny ACE 是按文件加的，不认身份；让中转绕过解锁，就是拿中转会话把 claude.exe 解锁、再从终端起官方的 —— 现成的绕过入口                                                               |
| 门禁判不过时要不要收掉这个会话           | `stops_with_gate()` | **不收**。中转请求打第三方端点、用你自己买的 Key、不带官方 OAuth 身份，Anthropic 那边看不见。收它换不到任何保护，却会把正在写的对话弄丢 —— 而中转站的典型使用者恰恰就是出口 IP 会变的那群人 |

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

## ⛔ 这是个开源项目：别人的机器跟你的不一样

**每一处改动都要先问一句「在别人电脑上还成立吗」**（2026-09-20 使用者定的）。
面板会被 clone、被装到别人机器上，而那台机器上的组合跟你这台几乎必然不同。

写死本机事实是这个项目最容易犯、也最难发现的错 —— 因为在**你**的机器上它工作得好好的。

| 别写死                                  | 该怎么做                                                                                                                                                                         |
| --------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 具体路径（`C:\Users\<你>\…`、某个盘符） | 走 `dirs::` 与**唯一那张位置表**（`install::inventory` / `codex_desktop` / `antigravity`），不在别处拼                                                                           |
| 端口号                                  | 让系统挑，再从对方写下的地方读回来。0.27.0 给反重力 IDE 传 `--remote-debugging-port=0` 就是这个原因 —— EasyAG 写死的 9333 在别人机器上会撞占用，而症状是「偶尔不生效」，查不出来 |
| 「装了 X」                              | 四种组合都要活：两个都装 / 只装一个 / 一个都没装。没装的那一档要说得出**下一步做什么**，不是一片空白                                                                             |
| 「跑着 Y」                              | 查不到就如实说「查不到」，**不许降级成「没有」**（§7.17）                                                                                                                        |
| 某个第三方壳 / 补丁存在                 | 本机 IDE 被第三方汉化壳包过，别人的可能是原版。两种都要走得通                                                                                                                    |
| 中文 Windows、某个 DPI、某个窗口尺寸    | `test:ui` 四档视口 + 两套主题是底线，别只看你这一档                                                                                                                              |
| 某个版本的官方客户端                    | 版本相关的措辞（日志里那句「Auth succeeded」之类）读不到时写「未见记录」，不写「没有」                                                                                           |

单测**不许碰真实的运行期状态**（见下面那一节）也是同一条规矩的一半：
在 CI 的干净容器里跑不过的测试，等于没有测试。

## ⛔ 不许加的功能

这三类不是「暂时没做」，是**明确不做**。加进来会让整个项目的定位垮掉，
也会让 [DISCLAIMER.md](DISCLAIMER.md) 变成谎话。

| 不做什么                                                                        | 为什么                                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **设备指纹伪装**（UUID / 主机名 / MAC / machine-id 改写）                       | 主要用途是多账号规避，与硬约束「所有账户必须本人拥有」直接冲突。DISCLAIMER 写死了「不对账户状态作任何承诺」与「不为规避封禁而设计，也无法达到该目的」（按这两句话去搜，别记行号 —— 行号每改一次 DISCLAIMER 就漂一次）                                                                                                                                                               |
| **内置代理 / VPN**（自己当代理、mTLS 中继、链式转发、把**整机**流量强制走代理） | DISCLAIMER 第 5 节。做了这个，那一节就是假的。⚠ 0.19.0 开了两个**有边界的**口子（浏览器出站锁、系统代理修改），见下面「两个口子」一节 —— 那两个口子之外，这一条照旧                                                                                                                                                                                                                 |
| **自动换号**（按额度、429、限流自动切槽位）                                     | 合规边界四条的前两条。存在这条路径，「多槽位」的定位就从「管理你自己的账户」变成了「规避限制」。联网问到的额度也一样：**只显示，不触发自动换号、模型路由或限流规避**                                                                                                                                                                                                              |

（原来这张表还有一行「自动联网查额度」，写着「不做后台轮询」。2026-09-23 使用者把这条规矩删了 ——
现在联网额度怎么问是一个**设计**，不是禁令，写在下面「联网额度」那一节。）

检测与**如实报告**不在此列 —— 面板可以告诉你「你的时区和出口对不上」，
但不替你改机器身份。两者的区别是：前者让使用者知情，后者替他伪装。

### 联网额度：只在点刷新图标时问，一次一个账户（2026-09-23 改写，使用者定的）

0.32.0 这里还叫「反重力 Hub 的配额是全项目唯一一处联网」。之后使用者一步步放开：
09-21 授权 GPT 与 Gemini CLI 查官方内部额度接口；09-23 删掉「不做后台轮询」那条规矩、
要反重力账户照 cockpit-tools 把联网能拿到的都显示出来，并拍板「令牌过期时面板在内存里换新」。
现在的来源：

| 问什么                                         | 走哪条路                                                                                              | 什么时候联网                       |
| ---------------------------------------------- | ----------------------------------------------------------------------------------------------------- | ---------------------------------- |
| Claude 的 5 小时 / 7 天                        | `plan-usage-history.json` + `.claude.json`                                                            | **从不**（刷新图标只重读本机）     |
| GPT 的 5 小时 / 7 天                           | ChatGPT `backend-api/wham/usage`（那个 Codex 槽位的令牌，`usecase::tavern_quota`）                    | 点那一行的刷新图标                 |
| GPT 的本机快照                                 | Codex 自己写在会话记录里的 `rate_limits`                                                              | 从不                               |
| Gemini CLI 各模型额度                          | Code Assist `loadCodeAssist` + `retrieveUserQuota`（当前账户 CLI 那一半的令牌）                        | 点用量卡 Gemini CLI 那一格的「刷新」 |
| 反重力账户：档位、AI 积分、Claude / Gemini × 5h / 周 | `loadCodeAssist` + `retrieveUserQuotaSummary`（免费档再加 `fetchAvailableModels`），`usecase::antigravity_quota` | 点那一行的刷新图标                 |
| 反重力 Hub：同上                                | 同上，令牌是 Hub 在凭据管理器里那一份                                                                  | 点用量卡 Hub 那一格的「刷新」       |
| 反重力 IDE 写在本机的邮箱 / 档位 / 各模型比例  | `state.vscdb` 的 `userStatus`                                                                         | 从不（没点过刷新时的兜底）         |
| 反重力 Hub 的邮箱                              | 凭据里 `id_token` 的载荷，本机 base64 解码                                                            | 从不                               |

**只手动刷新是使用者 09-23 选的设计**（问他时给过「打开页面查一次」「自动轮询 + 手动」两个选项，
他选了只手动）：打开页面、切页回来、打开酒馆弹窗都只读「最近一次」问到的（命令带 `refresh: false`，
后端绝不联网，没问过就是 `null`）；联网只从刷新图标发出，一次只问那一个账户；「最近一次」没有 TTL，
界面标明是几点问的。0.32.0 之后那一版 GPT 页挂载时、之后每 5 分钟对**所有**槽位各问一次
`wham/usage`（窗口缩到托盘也照问），而文档写的是「只在打开或点刷新时」—— 那段代码已经删了。
要改成别的节奏先问使用者，别自己加定时器。

仍然是硬约束的，动任何一条同步改 [DISCLAIMER.md](DISCLAIMER.md) §6 / §7 与 [ATTRIBUTION.md](ATTRIBUTION.md)：

1. **只读，只问自己的账户。** 只发上表那几个只读调用。⛔ **不发 `onboardUser`**：cockpit-tools 在
   没有 project 时会发它，而它会把账户登记到某个档位上、改账户状态。没有 project 就发 `{}`。
2. **结果只显示。** 不触发自动换号、模型路由或限流规避（「不许加的功能」那张表的自动换号一行）。
3. **Google 的令牌（反重力、Gemini CLI）过期时在内存里换新，不写回。** 09-23 使用者推翻了 0.32.0
   「不跑 OAuth、不持有 client_id/secret、不刷新令牌」那条：点刷新时访问令牌过期（或五分钟内过期），
   就拿官方客户端存在本机的那份刷新令牌向 `oauth2.googleapis.com/token` 换一张新的，只放内存
   （按刷新令牌的指纹认，同一个槽位里换了人登录就作废），**不写回 IDE 的库、不写回凭据管理器、
   不写回 `oauth_creds.json`**。同一天使用者又说「GPT 和 Gemini 的登录态对着那个开源项目修一下」，
   Gemini CLI 那条也接进了同一套（`usecase::google_oauth`）—— 原来它拿着过期一小时的令牌去问，
   401 被说成「登录失效」。安全的前提是 **Google 的刷新令牌换新之后不作废**，CLI 手里那份照样能用。
   ⛔ **GPT（Codex）的令牌面板永远不换。** OpenAI 的刷新令牌每换一次就轮换：面板一换，槽位 `auth.json`
   里那份立刻作废，桌面端下次换新时被登出、弹出登录页 —— cockpit-tools 也写着「官方客户端在用这个账户时
   不替它换」。GPT 的额度刷新只看本机时钟（JWT 的 `exp`），到点了就如实说「开一下桌面端它会自己换新」，
   **别把 `google_oauth` 接到 GPT 上**。
4. ⛔ **客户端标识不进仓库。** 换新要带签发那张令牌的客户端的 id / secret。**不许写成常量**：
   要换的那一刻从本机装的官方客户端里现读 —— 反重力走位置表 `install::antigravity::oauth_client_sources`
   （IDE 的 `resources\app\out\main.js`，读不出再扫语言服务器），Gemini CLI 走
   `install::detect::gemini_cli_oauth_client_sources`（包的入口脚本与同目录的 `.js`）—— 按出现位置配对，
   Google 回 `invalid_client` 就换下一组。认准的只记「哪个文件第几组」，标识用完即丢。
   这是跟 cockpit-tools / Antigravity-Tools-Lite 仍然不同的地方：它们把 Google 发给 Antigravity 的凭据
   硬编码进自己程序、自己跑登录、自己存令牌；这里不跑登录、不存令牌。
5. **令牌不落任何地方。** 不进日志、事件、审计、返回值。IDE 那一行由 `qb-accounts::antigravity::token`
   读，Hub 那一条由 `qb-platform::credentials::read_generic` 读，都装进 `Drop` 时抹零、没有
   `Debug` / `Serialize` / `Clone` 的 `Secret`。审计只写「问了一次」「换新了一次」。
6. **读不出来就说读不出来。** 端点改版 / 字段改名 / 网络不通 / 令牌换不下来 → 界面显示一句能照着做的话，
   **永远不显示一个猜出来的数**。唯一的「推」：一格有 `resetTime` 却没有 `remainingFraction` 时按 0 算
   （proto3 的 JSON 把零值字段整个省掉），那一格带 `remaining_implied`，悬停里说出来。
7. **请求形状照官方 IDE**：`User-Agent: antigravity/<本机 IDE 的 ideVersion> windows/<arch>`，
   `metadata.ideType = ANTIGRAVITY`。版本从 `product.json` 读，不写死（开源兼容性那一节）。
8. **「登录已失效」只来自服务端的明确拒绝**（2026-09-23，`usecase::login_health`）。本机文件只说得出
   「有没有令牌」；点刷新时 Google 回 `invalid_grant`、或 GPT 的访问令牌**没到点**却被回 401，才记一笔，
   账户行上那一半改说「登录已失效」、露出「登录」。按**凭据文件的修改时刻 + 长度**记、不读内容，
   官方客户端重新登录或自己换新了令牌（文件一变）就自动作废；只在内存里。原因文案一律以
   「登录已失效」开头 —— 界面按这个前缀把它跟「从没登过」分开标红，改措辞之前先看两处前端。
   ⛔ **「访问令牌到点了」不许说成「登录失效」**：那是这一条要修的错本身。

开关 `settings.antigravity_hub_quota`（默认开）字段名没改、语义扩成「反重力联网额度」，关掉之后点刷新只得到一句「设置里关掉了」。

⛔ **响应形状不许猜。** 这几个接口没有公开文档。换机器、上游改版、界面开始显示「读不出来」时先跑
（会用你自己账户的令牌联网一次）：

```powershell
cargo run -p qb-app --example antigravity-hub-quota
```

```powershell
cargo run -p qb-app --example antigravity-hub-quota -- --account <槽位 id>
```

它只打印键名、类型、长度，**一个值都不打印**。同一族的还有
`cargo run -p qb-accounts --example antigravity-hub-cred`（看那条凭据的形状与令牌何时过期）
与 `cargo run -p qb-accounts --example codex-ratelimits`（看 Codex 的额度记录）。

### 本机路由不在此列（0.14.0，使用者定的）

中转站那个只绑 `127.0.0.1` 的 API 路由器**是允许的**，别按上面那一行把它删掉。
使用者的原话：「这个只是中转站，而文档里说不做的是账户，两者不一样。」

分界线是**它替谁做决定**：

|            | 允许                                             | 不允许                                                 |
| ---------- | ------------------------------------------------ | ------------------------------------------------------ |
| 换的是什么 | 你自己填进去的中转站                             | 官方 OAuth 账户槽位                                    |
| 为什么可以 | 你自己买的 Key、第三方端点，Anthropic 那边看不见 | 按额度 / 429 自动切槽位 = 「自动换号」，合规边界头两条 |

两条路径在代码上是隔死的：`qb-station` 不依赖 `qb-accounts` / `qb-launch`，
`src-tauri/tests/architecture.rs` 的 `relay_breakers_can_never_reach_account_switching`
钉着这件事。**别为了图省事把那条测试删掉或者往 `ALLOWED_SIDEWAYS` 里补一行。**

路由器本身的三条硬约束（`crates/qb-app/src/local_router.rs`）：只绑回环、
不改系统代理设置、不做链式转发。这三条是 DISCLAIMER 第 5.1 节的措辞依据 ——
**动了其中任何一条，就要同步改那一节**，否则免责声明描述的是另一个软件。

### 体检的无凭据探测与一次性采集页（2026-09-24，使用者选的）

对照 zzusec/CheckClaude，使用者选了三组：服务可达 + claude.ai 解析、代理形态 + IPv6 真实出口、
真实浏览器采集（三路出口没选）。多出来的联网与本机服务边界如下，**动任何一条同步改
[DISCLAIMER.md](DISCLAIMER.md) 的联网清单**：

| 做什么                                                                      | 边界                                                                                                                                             |
| --------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| POST `api.anthropic.com/v1/messages`（body `{}`）                           | **不带任何凭据**，只看状态码（401 放行 / 403 拦截）。⛔ 别为了「更准」塞 OAuth 令牌或 API Key —— 那就成了拿使用者的账户去试探                    |
| GET `claude.ai` / `www.anthropic.com` 的 `robots.txt`                       | 同上，无凭据；带 `cf-mitigated` 的 403 是 Cloudflare 验证页，算「查不出」                                                                       |
| IPv6 回显（`api6.ipify.org` → `ipv6.icanhazip.com` → `v6.ident.me`）与 `ipinfo.io/<v6>/country` | 只用**只有 AAAA** 的服务 —— 双栈域名在没有 v6 时退回 v4，得出错结论                                                                              |
| 真实浏览器采集（`qb-app::browser_probe`）                                   | 只绑 `127.0.0.1:0`、路径带随机令牌、校验 `Host`、只收一份 ≤ 64 KB、120 秒、收完即关；结果只在进程内存（不落盘、不进日志与审计）                 |

四样都**只在使用者点体检 / 点「用默认浏览器测」时跑**，没有定时器；结果只进评分与报告，**不进门禁判定**
（跟出口一致性同一条）。

### 应用自己的更新：启动时问一次，点了才装（0.25.3，使用者定的）

使用者 2026-09-24：「我 GitHub 上上传了新版本，已经安装老版本的用户打开软件时（弹窗）收到更新通知，还有一键更新功能」。
问他时定了两件：**验真用同一个 Release 里的 `SHA256SUMS.txt`**（不另管 Tauri updater 那把签名私钥 —— 丢了私钥，
已装的人就再也收不到一键更新）；**带这个功能的第一版是 0.25.3**。代码四处：
`crates/qb-install/src/install/self_update.rs`（问、下、核；纯函数有单测）、`src-tauri/src/update.rs`（内存里的状态、
退出时交给安装包）、`src/features/UpdateDialog.tsx`（全局只挂一份的弹窗 + 启动那一次检查）、设置页「软件更新」。

| 做什么         | 边界                                                                                                                                                                                                                   |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 启动时问一次   | 只取 `github.com/<REPO>/releases/latest/download/update.json`，**不走 `api.github.com`**（未登录一个出口 IP 一小时 60 次，共用 VPN 出口的人会撞上限、永远收不到提醒）。设置 `update_check_on_start`（默认开）关掉就一个请求都不发；没有定时器 |
| 弹窗           | **只提醒、不自动装**。「以后再说」下次启动再弹；「跳过这个版本」记在 `update_skipped_version`，只经 `update_skip` 改（`settings_save` 保留旧值）。安装的代价常驻在按钮上方，不放悬停                               |
| 一键更新       | 下载地址按常量拼（`update.json` 只提供版本号与说明，里面没有一个字段会被当成地址）；先取那个标签下的 `SHA256SUMS.txt`，固定名安装包与带版本号那份的哈希必须一致；边下边算，**对不上就删掉、不装**                  |
| 交给安装包     | `app.exit(0)` → 退出处理**先重锁、再**启动安装包（`/P /UPDATE /R`：被动模式、覆盖安装不动快捷方式、装完重开面板）；启动前再算一遍哈希                                                                             |

五条要守住的：

1. ⛔ **只认比当前大的三段数字版本**（`self_update::is_newer`）。所以**以后每一版的版本号都必须比已经发出去的大** ——
   0.25.1 那种在 0.32.0 之后发、数字却更小的号，装着更大号的人收不到提醒。出包时问版本号，把这句一起告诉使用者。
2. **只提醒、不自动装。** 别加「静默更新」「定时检查」：面板一退出，它起的桌面端、对话、酒馆全跟着 Job 走（§7.38），
   装这件事必须是使用者点出来的。
3. **先退出、后启动安装包。** Tauri 安装包在 `/P` 模式下发现程序还在跑，会直接把它结束（NSIS 模板的
   `CheckIfAppIsRunning`，2026-09-24 从 CLI 2.11.4 里核过）—— 被结束的面板跑不到退出处理，执行锁停在半路。
   `update::tests::the_exit_handler_relocks_before_it_launches_the_installer` 读源码钉着这个顺序。
4. **发版要换更新说明。** `.github/release-notes.md` 是弹窗里「更新内容」和 Release 正文的唯一来源
   （`release.yml` 调 `scripts/update-manifest.mjs` 生成 `update.json`）。版本号跟 `package.json` 对不上，
   `npm run release:check` 当场红。写给使用者看，说大白话。
5. **fork 要改 `REPO`**（`self_update.rs`），不然改版的使用者会被「更新」回原版。README 许可证一节写了这句。

⚠ **端到端还没真跑过**：0.25.3 是第一版，要等下一版发出来，装着 0.25.3 的机器才第一次真正走完
「弹窗 → 下载 → 核对 → 退出 → 安装 → 重开」。见 KNOWN-ISSUES。**每次发版之后先跑一遍**（只问、只下、只核，不装）：

```powershell
cargo run -p qb-install --example self-update-check -- --as <上一版> --download
```

### 两个口子（0.19.0，使用者定的）

0.22.1 使用者另行明确要求：IP 纯净度中的「禁用本机 IPv6」默认开启，启动时应用，
关闭开关时恢复每张网卡原值。这是 IPv6 网卡绑定的独立例外，不改变下面代理与
防火墙的点击约束。原值必须先持久化；UAC 拒绝、部分失败、网卡移除时保留恢复记录，
界面以实际读取的绑定状态报告结果。启动应用完成后才恢复租约和启动看门狗。

使用者明确要求做这两件事，于是上面那条「内置代理 / VPN」开了两个**有边界的**口子。
`DISCLAIMER` 第 5.2 节是它们对外的措辞依据 —— **动了下面任何一条边界，就要同步改那一节**。

| 口子             | 允许                                                                                          | 不允许                                                         |
| ---------------- | --------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| **浏览器出站锁** | 给使用者点名的**那一个**浏览器 exe 加 Windows 防火墙**出站**规则：只放行指定的 VPN / TUN 接口 | 碰别的程序；加入站规则；改路由表、改 DNS、做转发；「全局」模式 |
| **系统代理修改** | 使用者当次点了「修」才改，改前把原值记下来，面板里能回滚                                      | 自动改；启动时改；看门狗或任何定时器碰它                       |

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

### 酒馆的 GPT 桥接只驱动官方 CLI（0.25.0，使用者定的）

使用者要 GPT 也能酒馆聊天，并且**并进插件商店里现有的酒馆插件、由面板自带桥接**
（Claude 那条桥仍是他自己的 `bridge.py`，面板不分发）。代码在
`crates/qb-app/src/gpt_bridge.rs`（127.0.0.1 上的 OpenAI 兼容端点，每个请求起一次
`codex exec`）；插件侧 `plugins::sillytavern` 拆成「起 Claude 桥接」「起酒馆」两段，
`plugin_start(backend)` 按后端选路，酒馆两条桥共用。

五条硬约束，动任何一条同步改 [DISCLAIMER.md](DISCLAIMER.md) §8：

1. **只驱动未修改的官方 Codex CLI 的公开无交互模式** `codex exec --json`。不改它的二进制、
   不用 app-server 协议、不自己打 `chatgpt.com`。`exec_args` 有测试钉着旗标。
2. **`auth.json` 零接触。** 不读、不复制、不转发任何令牌；身份全靠 `CODEX_HOME=<激活 GPT 槽位>\home`
   交给 CLI 自己去读。这跟 turn-state 那条「`auth.json` 一个字不碰」同源。
3. **只绑 `127.0.0.1`**，Bearer token 面板生成、只给本机酒馆；**没有 API Key 回退**。
4. **`-c model_provider="openai"` 强制官方**：槽位开着识别时 `config.toml` 指向本机路由，桥接不受牵连。
5. **默认 `--ephemeral`**，对话不落槽位会话历史；插件面板里「记入槽位用量」打开才写。

配套三条：子进程一律挂进 `KillOnCloseJob`（面板退出 / `stop()` 不留孤儿 `codex.exe`）；
`gate_ops::stop_managed` 收酒馆时一并 `gpt_bridge::stop()`（IP 不合格时它跟酒馆一样归门禁收）；
GPT 那条按 `LaunchTarget::Codex.gated()` 决定验不验 IP —— 移出门禁的 GPT 起酒馆也不验。
政策线如实写：Anthropic 明文禁止第三方中转订阅凭证，OpenAI 对此**没有**公开明确条款 ——
所以桥接只能长成「本机酒馆调本机 CLI」这个形状，DISCLAIMER 写明由使用者自行判断。

⛔ **等酒馆就绪的窗口别再调小**（0.31.0）。SillyTavern 自带的 `Start.bat` **每次都先跑一遍
`npm install`** 再 `node server.js`：本机实测（依赖全热）从起进程到 `GET /` 回 200 要 **47.2 秒**，
冷启动更久。窗口曾经是 60 秒，超时走回滚**把这次起的酒馆杀掉** —— 于是每点一次「起 GPT 酒馆」
就把上一次正要起来的那份掐掉一次，怎么点都失败（审计日志 09-21 02:43:49 起、02:44:57 收，
正好 68 秒）。现在 180 秒、等待期间每 4 秒把「已等多久」刷到界面上，**等超了不杀**：
进程活着就留着，如实说「可能还在装依赖，起来之后再点一次会直接复用」。
健康检查一律 `.no_proxy()` —— reqwest 默认读 `HTTP_PROXY`，会把对 `127.0.0.1` 的探测送去代理。

**三条桥的端口与模型全由面板管**（0.31.0，使用者定的）：入口是酒馆磁贴
**右上角**那颗小按钮（`Tile` 的 `corner`，磁贴本身是 `<button>`，角标只能是兄弟节点）。
「还缺什么」两处共用 `lib/tavernReady.ts` 的纯函数，各写一份迟早会互相矛盾。

⛔ **那个弹窗是一个组件，不是一段内联 JSX**（0.32.0）。
0.31.0 它写在 `AntigravityBand.tsx` 里、靠那个文件的局部 state 驱动 ——
于是**只有反重力页开得了它**，而 Claude / GPT 两页的酒馆磁贴上没有入口，
可那两条桥的端口与模型恰恰也在这里改。现在它是
`src/features/tavern/BridgeSettings.tsx`，**三个账户页的酒馆磁贴 + 插件详情页四处共用**。
插件页里那三处重复的端口 / 模型输入框删了 —— 同一个字段两处各放一份，
迟早出现一个没保存、另一个显示旧值的局面（坑 7.58 那种「同一块屏幕上两句矛盾的话」）。

### Claude 桥的设置页与监控页收进面板（0.32.0，使用者定的）

使用者看到的那个「控制面板」**不是面板的东西** —— 它是 `bridge.py` 自己起的两个
HTML 页（`GET /settings` 与 `GET /monitor`），由他本机那份 SillyTavern 扩展
（`claude-tavern-bridge`，**面板不分发它**）用 `<iframe>` 嵌进酒馆的扩展抽屉里。
改一句提示词要先起酒馆、进抽屉、等 iframe 加载；而用 GPT / Gemini 那两条桥聊天时，
那个 iframe 指着没人听的 5001，弹 `ERR_CONNECTION_REFUSED`（坑 7.66）。

现在面板直接调它的 HTTP 接口（`plugins::tavern_bridge_api`），七个旋钮与调用日志
做成原生界面（`src/plugins/ClaudeBridgePanel.tsx`）。三条硬要求：

1. **一律 `.no_proxy()`。** 坑 7.64 就是 `tavern_healthy` 漏了这一句。
2. **鉴权只在真要发请求那一刻读 `bridge-token.txt`**，不进 status / 日志 / 事件。
3. ⛔ **能力探测，不许假设。** `bridge.py` 是使用者自己那份，别人机器上可能是别的版本、
   甚至根本没有。连不上 / 404 / 字段缺，一律如实说「这份桥接没有这个接口」或
   「Claude 桥接没在跑」，**不许把空表显示成 0 条**（§7.17）。
   错误文案要**能照着做**：连不上 = 去起酒馆，404 = 升级 bridge.py，401 = 密钥对不上。

模型那 13 个、effort 那 5 档是按 `bridge.py` 2.4.0 抄的**快照**，而
**桥当前用着的那个值永远算合法选项**（`with_current`）—— 不然上游加一个新模型时，
一打开设置就把人家的模型悄悄换成列表里的第一个。保存被拒就把桥回的原话照搬到界面上。

⚠ 改这七项里任何一项，`bridge.py` 都会 `cancel_active(retire=True)` 退掉当前那条 SDK 会话 ——
代价常驻在保存按钮旁边，不放悬停提示。路径 / 资产 / 备份仍在插件详情页。

### 反重力（Google Antigravity，0.26.0，使用者定的）

使用者要把 `DSDS-CMHL/EasyAntigravity`（MIT）融进来：反重力做官方账户、进软件页、被 IP 锁保护、
保留它的汉化 / 自动审批 / 高危拦截；接酒馆。2026-09-20 读它的 `app.asar` 与语言服务器二进制之后
定下的边界，**动任何一条同步改 [DISCLAIMER.md](DISCLAIMER.md) §6 / §8 与 [ATTRIBUTION.md](ATTRIBUTION.md)**：

1. **没有中转路径。** Hub 把 `--api_server_url` / `--cloud_code_endpoint` 写死在 `dist/languageServer.js`
   里、只认 Google 登录。`Client::Antigravity` / `AntigravityIde` 在中转侧的每个入口**显式拒绝**
   （`workspace::reject_if_no_relay_path`，`environment_save` / `router_environment` / `save_plan` /
   `launch(Relay)` 都调它；`local_router::client_base_url` 给的是 `NO_RELAY_PREFIX`，路由见到直接 404）。
   别为了让 match 完整而静默套用别的软件的前缀 —— 那会写出一份指错地方、没人说得清为什么起不来的配置。
   「本机路由加 Gemini 协议线路」是**另一轮**的事，供酒馆等客户端用，不是给反重力本体。
2. **Hub 没有槽位；IDE 有（0.30.0，使用者定的）。** Hub 的令牌在 Windows 凭据管理器（语言服务器
   二进制里 `CredRead/CredWrite` 43 处、`oauth_creds` 0 处），目录隔离隔不开，换号在它自己界面里做。
   **IDE 不一样**（2026-09-21 实机核过）：它是 VS Code 分支，令牌在 `<用户数据目录>\User\globalStorage\state.vscdb`
   里，Hub 的目录下根本没有这个库；VS Code 认标准的 `--user-data-dir`。所以 IDE 槽位 = 一个用户数据目录
   （`qb-accounts::antigravity::ide`，`state_dir\antigravity-ide-accounts\<id>\user-data`），跟
   `CLAUDE_CONFIG_DIR` / `CODEX_HOME` / `GEMINI_CLI_HOME` 同一个套路：**面板换的是目录，登录在 IDE 自己的
   窗口里做，面板不跑登录**（「登录了没」只问令牌那一行的 `length(value)`；令牌的值只在使用者点联网额度的
   刷新图标时读、只进内存，见「联网额度」）。目录经
   `sessions::ANTIGRAVITY_IDE_USER_DATA_ENV` 带给 `desktop_arguments`，那是「按 client 给参数」的唯一一处。
   探针（临时目录起 IDE → 要求重新登录 → 登第二个账户 → 两份资料各持各的邮箱、同时跑互不影响）
   在本机走通过；别的机器上没验，KNOWN-ISSUES 写了。三条跟着它：

   - **新槽位只复制默认资料的 `User\settings.json` / `keybindings.json`**（偏好），`globalStorage`
     一个字节不搬 —— 复制式迁移造过两份 refresh token（§7.29）；使用者原来那份默认资料目录不动，
     没有激活槽位时起的就是它；
   - **切换 = 换激活槽位，不关不起任何东西**（同 Codex 页）；正在跑的 IDE 继续用它起来时那份资料；
   - **面板起的那份 IDE 的 `DevToolsActivePort` 在槽位目录里**，不在 `%APPDATA%\Antigravity IDE`。
     `antigravity_ui::set_ide_user_data_dir` 由编排层（`antigravity_ops::ide_profile_in_use`）在起 IDE、
     刷新状态、面板启动时灌进去 —— 漏了这一步，汉化引擎对着默认目录里上一次运行留下的端口文件附加，
     症状又是「汉化偶尔不生效」。

   ⛔ **一条槽位 = 一个 Google 账户，底下两半（0.32.0，使用者定的）。** 0.30.0–0.31.0 是两套
   互不相干的槽位、界面上两个页签：IDE 那套走 `--user-data-dir`，Gemini CLI 那套走
   `GEMINI_CLI_HOME`。同一个账户要建两次、登两次，而只用 IDE 的人永远看着一句
   「Gemini CLI · 0 个槽位」—— 它读起来像个故障，其实只是「你还没建」。
   现在一条 = 一个账户（`qb-accounts::antigravity::account`，
   `state_dir\antigravity-accounts\<id>\{ide-user-data,cli-home}`），两半各有一枚徽标、
   各有一个登录按钮，一个 `active` 管两半。三条跟着它：

   - **升级迁移只换索引，一个文件都不搬**（`account::migrate`，幂等，面板每次启动都调）。
     老的两份清单各自升成一条「只填了一半」的行，`ide_dir` / `cli_dir` **指向老位置**。
     ⛔ **不猜配对** —— 面板没有任何依据断定某个 IDE 槽位和某个 Gemini 槽位是同一个
     Google 账户（IDE 的邮箱在状态库里，而 CLI 那边**只看凭据文件在不在、从不读内容**）。
     要合由使用者自己点「并入…」（`account::attach`，同样只改索引）。
     ⛔ **不搬目录** —— §7.33「迁移搬一半」就是这么来的；指路不搬家，出了事删掉
     `index.json` 就回到原状。
   - **切一次动两半**，所以原来挂在 `gemini_switch` 上那道「酒馆的 Gemini 桥接在跑时不许切」
     的守卫**搬进了 `antigravity_ide_select`**。留在原处就等于没有：桥接会继续拿着上一条
     槽位的 `GEMINI_CLI_HOME` 跑，而界面显示的是新的那条。
   - `gemini_accounts` / `gemini_create` / `gemini_switch` / `gemini_archive` 四条命令没有了，
     `gemini_login` 留着（它是真正跟 CLI 绑死的那个动作，`id` 现在是**账户槽位**的 id）。

   **账户状态读 IDE 自己写在本机的文件。** IDE 把邮箱、档位、每个模型的剩余额度比例与重置时间
   写在 `state.vscdb` 的 `antigravityUnifiedStateSync.userStatus` 里（base64 protobuf 套 base64，
   字段号见 `qb-accounts::antigravity::status` 文件头，2026-09-21 实机解的）。⚠ 但它**每个模型只有
   一个比例、一个重置时刻**，没有 5 小时 / 每周两个窗口，而且只有 IDE 开着时才更新 —— 2026-09-23
   使用者对着「剩 100% · 5 天后重置」说「显示不准」，那个槽位的数停在两天前。所以现在它只是
   **没点过刷新时的兜底**：账户行上写「IDE 写入 hh:mm」，不许说成此刻；点那一行的刷新图标之后
   换成联网问到的四格（见上面「联网额度」）。

   **Hub 那一侧是分开的两件事**（0.32.0）：**邮箱**在凭据里那个 `id_token` 的载荷中，
   一次本机 base64 解码就有，**零网络**（`qb-accounts::antigravity::hub`）；
   **档位与额度** Hub 在本机一个字都没写，只能联网问 —— 跟账户行同一套（`usecase::antigravity_quota`），
   入口是用量卡 Hub 那一格的「刷新」。
   ⛔ **Hub 与 IDE 是两个账户、两套额度，界面上分两行，绝不合并成一个数** ——
   使用者完全可以在 Hub 里登 A、在 IDE 槽位里登 B，合并就是编一个不存在的账户。
   用量同理：读 `~\.gemini\antigravity*\conversations\*.db` 的 `gen_metadata`（protobuf，自写的线格式
   读取器 `antigravity::proto`，不引 prost），旧 `.pb` 归档不猜、只报数；备份目录里同名对话只计一次。
   字段含义是按 12,410 条实测记录推断的，界面写「按实测推断」。

3. **Hub + IDE 都锁，一个开关。** 位置表只有一张：`crates/qb-install/src/install/antigravity.rs`
   （主程序、语言服务器、第三方汉化壳留下的 `*.original.exe`；卸载器除外）。`settings.antigravity_outside_gate`
   默认 false = 归门禁（反义存法同 GPT）。看门狗桌面档、零宽限。一键关闭的证据是**安装目录**
   （`Evidence::AntigravityInstall`），不按签名、不按名字 —— 别的 IDE 也有叫 `language_server.exe` 的。
4. **起之前先关正在跑的**（`workspace::close_antigravity_for_launch`，三步同 `close_desktop_for_relay`）：
   它是 Electron 单实例 + 托盘后台运行，不退干净新起的只会把旧窗口拉到前面，进程不归面板托管、租约挂不上。
   关不干净就报错不起。
5. **汉化 / 自动审批 / 高危拦截 = 只注入脚本，不改二进制。** `plugins::antigravity_ui` 读
   `DevToolsActivePort`、连 CDP、`Runtime.evaluate` EasyAG 那段脚本
   （`assets/antigravity/inject.js`，逐字，六个占位符由 `assemble_script` 替换，测试
   `the_injected_engine_keeps_its_placeholders_until_assembly_replaces_them_all` 钉着）。不落文件到它的目录。

   0.27.0 起三条跟着它：

   - **Hub 与 IDE 都注入**，引擎同时看两个源，一个掉线不停另一个。**IDE 的调试端口由面板启动时补上**
     （`qb-platform::sessions::desktop_arguments` 给 `--remote-debugging-port=0`，只绑回环、随机端口）——
     VS Code 分支默认不开端口。别写死端口号：占用时会静默失败，症状是「汉化偶尔不生效」。
     使用者自己从开始菜单起的 IDE 没有端口，界面上要如实这么说；
   - ⛔ **「注入成功」必须回读核实**。0.26.0 只数发出去几次，而 `Runtime.evaluate` 的回执被整个丢掉 ——
     脚本炸了界面照样显示「已注入 N 个页面」。现在注入后紧跟一条**面板自己的** `VERIFY_EXPR`
     （只读，不许出现任何赋值，有测试钉着），`exceptionDetails` 记进日志，界面显示核实过的页面数；
   - ⛔ **自动附加要问「CDP 应答得了吗」，不要问「端口文件变了吗」**。0.26.0 等的是端口文件 mtime 变化，
     而 Chromium 在进程刚起来的几百毫秒就写完了 —— 条件永远不成立，实机日志里连着两次都是
     「30 秒没等到调试端口文件」，而汉化从来没生效过。失败原因要落到界面上
     （`antigravity_ui::note_attach_error`），只写审计日志等于没人看见。

6. **不做「免 TUN 代理」。** EasyAG 那个 `version.dll` 是来源许可不明的第三方代理核心，注入
   `language_server.exe`，每次更新回写 —— 撞「抄代码先看 license」与「不改官方二进制」。使用者决定不做，
   也不提供环境变量之类的替代实现。以后有人想加，先看 ATTRIBUTION 那一节。
7. **酒馆接的是 Gemini CLI，不是反重力本体。** Hub 没有公开的无交互模式（`--headless` 走语言服务器
   未公开的 stdin 协议）。`gemini_bridge.rs` 照 `gpt_bridge.rs` 的五条约束改写：只驱动未修改的官方 CLI 的
   公开无交互模式（stdin + `--output-format json`）、桥接路径对 `oauth_creds.json` 零接触、只绑 127.0.0.1；
   使用者点 Gemini CLI 额度的「刷新」时另有一次性令牌探针（访问令牌过期了在内存里换新，见「联网额度」
   第 3 条），不参与桥接、不保存、不自动运行，
   不设 `GEMINI_API_KEY`、门禁跟 `LaunchTarget::Antigravity.gated()`。界面与 DISCLAIMER §8 都写明
   「驱动的是 Gemini CLI」「额度是否共享未核实」。
8. **面板不分发、不托管、不镜像 Google 的安装包**（0.29.0 改写了这一条的落地方式，边界没变）。
   0.28.0 之前这条的做法是「软件页给个官网链接，使用者自己去点」；现在是**替他从同一个官方地址
   把同一个官方安装器取回来，验完签名再交给它自己安装** —— 面板不改包、不重新打包，
   装出来的那一份跟他自己去官网点下载得到的是同一个文件。跟 Codex 桌面端直装同一条边界。
   代码在 `crates/qb-install/src/install/antigravity_setup.rs`，四条硬规矩：

   - **A 路 winget 官方包，B 路从 Google 自己的域直下**（`allowed_host`：`storage.googleapis.com` /
     `edgedl.me.gvt1.com` / `dl.google.com`，**只收 https**）。winget 源会落后（实测 2.12.2 vs 官网 2.15.1），
     所以 A 路成功之后仍然回读检测；
   - ⛔ **官网不公布安装包哈希**，所以完整性闸门是「域限定 + TLS + Authenticode 主体含 Google」，
     读不出签名 = 没验成 = 不装。算出来的 SHA-256 **只记进日志、不作判据** ——
     没有可信对照值时拿它当判据就是自己跟自己比。**别在代码里编一个「预期哈希」常量**：
     版本一变就过期，而过期的形态是「所有人都装不了」；
   - ⛔ **抓下载页要自己解 gzip。** 那个站点不管发什么 `Accept-Encoding` 都压着发（连显式
     `identity` 也是），而面板的 reqwest 没开解压特性。症状极具欺骗性：200、`text/html`、
     `.text()` 给得出六万多字符 —— 那是 gzip 字节被有损解码出来的替换字符，一条地址都没有，
     而第一版代码把它报成了「页面可能改版了」。**不要为此给 workspace 的 reqwest 开 `gzip` 特性**：
     那会让本机路由那条要求逐字节流式转发的上游连接也跟着变（同坑 7.52 的取舍）；
   - **装之前先关、装之后重新上锁。** 它是 Electron 单实例 + 托盘后台运行，开着的时候安装器
     换不动文件而且多半不报错（§7.21）；装完是全新的 exe、继承干净 ACL，
     `Maintenance::observing` → `finish` 负责把 Deny 加回去。**漏了这一步，使用者装完一次反重力，
     IP 锁就对它失效了，而界面上一切正常。**

   **Gemini CLI 仍然不分发**（装的是 npm 源上的官方包），但 0.29.0 起**不再弹窗口**：
   走 `winget::run_streaming("cmd", ["/c","npm","install","-g",…])`，输出流进软件页的进度条，
   等它装完、回读检测核对。原来那个 `cmd /k` 窗口里 **npm 一次都没跑起来过** ——
   见下面「别把 shell 当启动目标」。

   ⛔ **回读检测的位置表不许猜**（0.31.0）。0.26.0 把入口写死成
   `<前缀>\node_modules\@google\gemini-cli\dist\index.js`，而官方包的入口一直是
   `bundle\gemini.js`（`package.json` 的 `"bin"`）。0.29.0 修好 npm 那条命令之后，
   **包每次都真的装上了、回读每次都落空**，于是连着两天报「还是装不上」（审计日志 09-21
   02:32 / 18:16），软件页一直显示未安装，酒馆的 Gemini 桥接也从没起来过。
   入口现在**问包自己**（读 `bin`），两种见过的布局只作兜底；包目录也不再只认
   `%APPDATA%\npm` —— `NPM_CONFIG_PREFIX` 与 `%ProgramFiles%\nodejs`（nvm-windows / MSI）一并查。
   换机器或包换布局时先跑 `cargo run -p qb-install --example gemini-resolve`，
   它会把「找过哪些路径、每条在不在」打出来。

### Codex 桌面端直装（0.28.0，使用者定的）

使用者要把 `chrichuang218/codex-windows-cn` 与 `Wangnov/codex-app-mirror`（都是 MIT）的本事融进来：
**不打开 Store 也能装 Codex 桌面端**。代码在 `crates/qb-install/src/install/codex_store.rs`
（+ 同名目录里三份 SOAP 信封与一张根证书），命令 `codex_desktop_install` / `codex_desktop_latest`。
五条硬规矩：

1. **装出来的必须是正规注册的 Store 包**：A 路 `winget install --source msstore`，B 路 FE3 直连下 `.msix`
   再 `Add-AppxPackage`。**不采用**两个来源的「解压 `app/` 未打包运行」与「`versions/` + junction 多版本」——
   丢包身份（`codex://`、自动更新），而面板启动的本来就是 `Get-AppxPackage` 查到的那份。理由见 DESIGN-NOTES。
2. **Store 包的身份只在 `codex_desktop.rs` 一处**（`PRODUCT_ID` / `PACKAGE_NAME` / `PACKAGE_FAMILY`），
   `codex_store` 与 `purge` 都只引用。product id 不跟显示名走（2026-07 改名 ChatGPT 时它没变）。
3. **三道闸一道不能省**：下载域限定 `delivery.mp.microsoft.com`（`allowed_host`）、清单 SHA-256 有就核、
   Authenticode 主体含 OpenAI（读不出 = 没验成 = 不装）。下载走 FE3 给的 `http://` 原样地址 ——
   那个 CDN 主机名没配证书，别再试着换 https。
4. **内置的 Microsoft Root CA 2011 只加给 `codex_store` 自己的 reqwest 客户端**，指纹有单测钉着。
   不许为此给整个 workspace 开 `rustls-tls-native-roots`。
5. **单测不联网。** 解析器全是纯函数，拿录下来的报文形状测。微软改了报文时先跑
   `cargo run -p qb-install --example codex-store-resolve -- dump` 看清楚再改解析器；`locate` 模式只到
   拿地址为止，**不下载不安装**。

⛔ **`Add-AppxPackage` 非提权撞 `0x80073D28`（包里带打包服务）时走 UAC 提权，使用者拒绝就如实报「已取消」**，
跟 `codex_desktop::repair_registration` 同一条路。装之前先关正在跑的桌面端（`usecase::codex_accounts::close`），
关不掉不装。

### GPT 默认归门禁管（0.25.0，使用者定的）

`Settings.codex_outside_gate`（默认 `false`）取代了 `codex_under_gate`（默认 `false`）：
**反过来存是升级路径** —— 旧文件里的 `codex_under_gate: false` 成了未知键被忽略，所有人升级后
自动落到「归门禁」，不用迁移代码。读的一侧仍叫 `settings::codex_under_gate()`。
它推翻了 `settings.rs` 文件头「会让命令突然跑不起来的开关默认关」那条，使用者原话
「默认定死被 IP 锁接管」；开关搬进了评分栏「IP 锁」弹窗（`AllowlistPanel` 的 `GateRange`）。
账户页起 GPT 桌面端走 `workspace::launch(Client::Codex, Official, <槽位 id>)`——
跟 Claude 页同一条链（验 IP → `sessions::start` → 租约 → 看门狗 Desktop 档）；
`codex_desktop::launch()` 那个裸 spawn 删了，别加回来。

### ⛔ 起 / 切 GPT 槽位之前只关面板自己起的 Codex（2026-09-23）

使用者：「GPT 有时候突然弹出第二个使用页面」。0.25.0 起「起槽位 / 切槽位」之前走的是
`codex_desktop::close()` —— 按官方包的 exe 路径把**所有** Codex 桌面端都收掉，理由是「单实例，
开着再起只会把旧窗口拉到前面」。那条理由只对**同一份资料**成立：Electron 的单实例锁按
`--user-data-dir` 算，面板给每个槽位的是它自己那份，开始菜单 / `codex://` 链接 / 别的多开工具
起的默认实例挡不住它。全关的代价却是真的：别处起的那份被面板收掉，起它的那个程序再把它拉起来，
看到的就是突然又弹出一个 Codex 窗口。

现在 `usecase::codex_accounts::{launch, switch}` 走 `close_ours`：托管会话按会话停，
加上**资料目录在面板状态目录下**的实例（`codex_desktop::is_panel_profile`，上一次运行留下的也算）；
读不出命令行的实例当成别人的、不关（不许把认不出来的当成自己的去关）。
使用者点「一键关闭」、装桌面端之前仍走 `close`（全关，确认框里写着「所有」）。
探测只列 Electron **主进程**（`--type=` 的子进程不单列），每个带上 `profile` 与 `ours`，
前端的确认框、磁贴副标题只数 `ours`。cockpit-tools 的多开也是按实例的资料目录认自己那份、只关自己那份。

---

## ⛔ 一键汉化：Claude 只接上游安全模式，GPT 只写它自己的设置项（2026-09-25，使用者定的）

使用者：「gpt 虽然有中文，但默认不是，加一个一键汉化按钮自动设置中文」「claude 没有中文，以插件的形式装载
javaht/claude-desktop-zh-cn，确保这个开源项目后续的更新可以匹配，插件商店可以同步这个开源项目的更新」
「确保以上两个改动符合两个厂商的使用政策」。问他时定了四件：**Claude 只接上游安全模式 + 事后核验**；
**打开汉化弹窗或插件页时查上游新版**（没有定时器、启动时不查）；**Claude 自己更新后按钮变「需重新应用」，
点一下补上**（不改启动流程）；**GPT 改全部槽位 + 默认那一份 + 以后新建的槽位**。

### GPT（`usecase::codex_locale`）

1. **只写 `CODEX_HOME\config.toml` 里 `[desktop]` 表的 `localeOverride`** —— 跟在 GPT「设置 → General →
   Language」里选中文写下的是同一行（toml_edit 增量改，别的一个字不动；`auth.json` 零接触）。
   ⛔ 不改 ChatGPT.exe / app.asar、不注入翻译 —— OpenAI 使用条款不许 modify 它的服务。
2. ⛔ **不碰 `enable_i18n`。** OpenAI 用这个远端开关按账户 / 机器放中文界面（openai/codex#19239）：
   没放开时设了照样是英文。界面如实说，不提供任何绕过（条款：不得 circumvent restrictions）。
3. 面板起的 GPT 开着 → 先 `close_ours`（确认框写明任务会停）→ 写 → 命令层按当前槽位重开（那里挂看门狗）；
   别处起的那份不关（2026-09-23 那条），它开着时默认那一份这次不改、如实说；出站插件在跑时拒绝（它整份接管
   `config.toml`）。**中转环境不碰**（受环境哈希追踪，改了会被判「外部修改」）。
4. 「恢复默认」**只删值为 `zh-CN` 的那一行** —— 使用者在 GPT 里自己选的别的语言不是我们的残留。
5. `settings.gpt_ui_zh` 记的是使用者的选择（新槽位按它预写，`codex_accounts::create`），**只经
   `codex_locale_set` 改**：`settings_save` 原样保留旧值。

### Claude（插件 `claude-desktop-zh-cn`：`plugins::claude_zh` + `usecase::claude_zh_ops`）

上游 Windows 脚本五个动作、两种补丁模式，面板只碰两个动作、一种模式：

| 上游 | 面板 |
| --- | --- |
| `install zh-CN -PatchMode safe`：放三份翻译 JSON、改 `ion-dist\assets\v1\*.js` 的语言白名单与硬编码英文、写 `config.json` 的 `locale`、最后自己重启 Claude | ✅ 一键汉化 |
| `uninstall`：从它自己的 `.zh-cn-backups` 还原、删翻译、locale 设回 `en-US` | ✅ 恢复英文 |
| `-PatchMode official`：改 `app.asar` 并**重写 `Claude.exe` 内嵌的完整性哈希**（签名变 `HashMismatch`） | ⛔ 绕过防篡改 = Anthropic 消费者条款 §3「bypassing … protective measures」 |
| `frida-launch` / `scripts\experimental\`：Frida 在内存里改掉「带调试开关就拒绝启动」的闸门 | ⛔ 同上；这些文件一个字节都不落盘（`zipread` 先列目录、挑好再解） |
| `disable-updates` / `sync-skills` | ⛔ 跟汉化无关 |

八条要守住的：

1. **参数只由 `claude_zh::script_args` 拼，动作只有 `Action::{Install, Uninstall}`。** 测试
   `the_arguments_can_never_ask_for_anything_but_safe_mode` 钉着拼不出 official / frida / 关更新 / 同步 skills
   （改坏它会红，2026-09-25 实测过）。`-ExecutionPolicy RemoteSigned`，不用 `Bypass`（少一个杀软信号）。
2. **不信上游，只信文件**：每次应用 / 还原前后算 `app.asar`、`claude.exe` 的 SHA-256 并读 Authenticode
   **状态**（`signature::status_of`；`signer_of` 只给主体，`HashMismatch` 的文件照样报 Anthropic）。
   变了 → 立刻跑上游 `uninstall`、那一版进黑名单、界面标红。基线读不出来就**不做**。
3. ⛔ **反重力那套 CDP 注入不许搬到 Claude 上**：Claude 桌面端见到 `--remote-debugging-port` 就拒绝启动
   （2.9939.2 字符串核过），给它加调试开关或绕过那道检查就是上表第四行。
4. **门禁先判**（`gate::judge_now`，只判不解锁）：上游脚本最后会用 `app-*\claude.exe` 自己重启 Claude，
   那份按规矩不能加 Deny；IP 不合格就不做。它起来之后面板等它出现、按证据关掉（`close_desktop_for_relay`）
   —— 那份不归面板管、没有租约。重开走现有的「Claude 桌面端」磁贴与强制确认框。
5. **查新版只在打开汉化弹窗 / 插件页、或点「检查更新」时**，只问 `github.com/<上游>/releases/latest` 的跳转，
   ⛔ **不走 `api.github.com`**（同 self_update）。上游脚本自己问 GitHub 的那一下用
   `CLAUDE_ZH_SKIP_UPDATE_CHECK=1` 关掉。上游仓库地址是常量，不收使用者输入。
6. **每一版下载后重核**：`LICENSE` 仍是 MIT 原文、脚本 `param(...)` 里仍有 `safe` / `install` / `uninstall` /
   `zh-CN`（`script_caps`，能力探测不假设）；不对就不用、留着旧版。下载「只下、只核」在内存里做
   （`fetch_verified`），核完才落盘（`install_package`，暂存目录整份写完再换名）。
7. **只做位置表认得的 Squirrel 装法**（`inventory::desktop_app_dir`，`detect::claude_desktop` 也用它）。
   MSIX 要 UAC 接管 WindowsApps 的 ACL，面板不做、如实说。
8. **别的账户资料也设 zh-CN**：上游只改 `%APPDATA%\Claude`（联结点指着的当前账户）。面板把其余
   `Claude-<名字>\config.json` 的 `locale` 也设上 —— `set_top_level_string` **只改那一个值的字节**
   （里面有 `oauth:tokenCache`，整份解析再序列化会重排键、等于把令牌读出来写回去）。改之前的值按
   **真实目录**（联结点解开）记在 `state_dir\claude-zh\state.json`，恢复英文时写回。

诊断：`cargo run -p qb-app --example claude-zh -- check`（只查、只下、只核，**不跑上游脚本、不写本机任何东西**）。
上游改了归档结构、换了许可证、改了脚本参数时先跑它。

⚠ **调试时别在真机上点 Claude 的「一键汉化」/「恢复英文」**：它会关掉 Claude 桌面端，Code 页里跑着的会话
也在里面 —— 你正在那里面干活的话就是把自己关了（同「启动 桌面端」那条）。

---

## ⛔ 别把 shell 当启动目标（0.29.0）

`sessions::OwnedProgram::launch_detached_console` / `NativeProcess::create` 会给**每一个**参数
套引号（`quote_argument` 无条件加）。对普通程序这是对的 —— Windows 的 CRT 按这套规则反解析。
但 `cmd.exe` / `powershell.exe` **自己解析命令行、而且不先脱引号**，于是 `"/c"` / `"/k"` /
`"-File"` 统统不再是开关。

实机代价：0.26.0–0.28.0 的 `gemini_cli_install` 交出去的是

```text
"C:\Windows\System32\cmd.exe" "/k" "npm install -g @google/gemini-cli && …"
```

`cmd` 把后面那一整串当命令名去找，报 `is not recognized`，`/k` 让窗口留在原地 ——
使用者看到的是**「弹出了一个命令窗口，什么都没装」**，而面板这边一路 `Ok(())`、
界面还提示「已打开安装窗口」并立刻重新检测（那时 npm 一动都没动）。**三个版本没人发现。**

现在 `NativeProcess::create` 直接拒绝这种调用（带参数的 `cmd` / `powershell` / `pwsh`），
测试 `a_shell_can_never_be_the_launch_target_when_it_has_switches` 钉着。要跑 shell 走这两条：

| 要什么                   | 用哪个                                                                                                   |
| ------------------------ | -------------------------------------------------------------------------------------------------------- |
| 等它结束、要进度、要错误 | `qb_install::install::winget::run_streaming("cmd", &["/c", …])` —— 每个参数一个 argv，隐藏窗口，逐行回显 |
| 给使用者一个看得见的窗口 | 把脚本写成 `.cmd` / `.bat` 文件，把**那个文件**当成 exe 传进去（走 batch 分支，开关由我们自己拼）        |

**同一族的另一处**（`crates/qb-sysenv/src/sysenv/ipv6.rs`）：提权脚本要作为一个参数穿过
`Start-Process -ArgumentList`，而 PowerShell 5.1 在那里会把**内嵌的双引号**吃掉。
实测三种形状：单行无引号 ✅、单行含双引号 → **exit 0 而什么都没做**、多行只有单引号 ✅。
所以那段脚本的计划用 PowerShell 数组字面量写、不用 `ConvertFrom-Json`，
并有测试 `the_elevated_script_never_contains_a_double_quote` 钉着。
⛔ **往那段脚本里加任何带 `"` 的东西之前先读那条测试** —— 加了之后的症状是
「点了没反应，也没有报错」，不是编译失败。

## ⛔ 杀软会把这个面板当木马（0.29.0）

本机 2026-09 连着三次 `Trojan:Win32/Bearfoos.A!ml` / `B!ml`，装好的 `qb-gate.exe`
与安装包都被 Defender 隔离过。`!ml` = 机器学习启发式。根因三条：**没签名 + 零信誉 +
行为像木马**，而第三条就是这个产品本身（写 Deny ACE、收进程、下 exe 再执行、往别人的
Electron 里注 JS、扫 Chrome 的 Cookies、改防火墙与代理），删不掉。

处置、申诉步骤、0.29.0 减了哪些信号：[docs/ANTIVIRUS.zh-CN.md](docs/ANTIVIRUS.zh-CN.md)。
三条签名路线各自的代价：[docs/KNOWN-ISSUES.zh-CN.md](docs/KNOWN-ISSUES.zh-CN.md)。

改代码时顺手守住的三条：

1. **别再加新的「隐藏窗口 + 提权 + base64」组合。** 提权和隐藏窗口是功能需要，base64 不是；
2. **构建配置里的东西改了要回读核对**（PE 版本资源、内嵌清单）。`npm run tauri build` 的
   退出码证明不了清单进去了 —— 命令在 ANTIVIRUS 第 6 节；
3. ⛔ **永远不要在面板里加「一键给自己加杀软排除项」。** 程序给自己开白名单本身就是
   恶意软件行为，加了只会让下一版被判得更重。文档里给现成命令，由使用者自己在管理员窗口执行。

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

| 要换什么 | 怎么换                                                                                                                            |
| -------- | --------------------------------------------------------------------------------------------------------------------------------- |
| 数据目录 | `CLAUDE_USER_DATA_DIR` 环境变量。**它优先级最高** —— 桌面端自己那套 3p 目录重定位包在 `if (!process.env.CLAUDE_USER_DATA_DIR)` 里 |
| 推理端点 | 它自己那份 `claude_desktop_config.json` 里的 `deploymentMode: "3p"` + `inferenceProvider: "gateway"` + `inferenceGatewayBaseUrl`  |

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

## ⛔ 分层由五条测试守着，不是由自觉

Rust 侧是一个 Cargo workspace：`qb-foundation`(L0) → `qb-contract` → `qb-platform`
→ 九个领域 crate → `qb-app`(编排) → `src-tauri`(命令/托盘/装配)。

`src-tauri/tests/architecture.rs` 里有十二条测试钉着它，其中五条是硬规矩：

| 测试                                                          | 它不许发生什么                                                                                                                                                                                                                  |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `module_cycles_only_ever_shrink`                              | 任意一组模块互相到得了（算的是**强连通分量**，不是成对互指 —— 早先只查成对，漏掉了一个 11 模块的环整整一轮）                                                                                                                    |
| `crates_only_depend_downwards`                                | crate 往上层依赖；同层依赖必须登记在 `ALLOWED_SIDEWAYS` 里，而且那张表**只许变短**                                                                                                                                              |
| `only_the_app_crate_knows_about_tauri`                        | 领域或编排 crate 的 `Cargo.toml` 里出现 `tauri`                                                                                                                                                                                 |
| `the_watchdog_still_routes_through_the_judge`                 | 看门狗自己写一套判定，绕开 `watchdog::decide`                                                                                                                                                                                   |
| `helpers_called_under_the_exclusive_lock_never_take_it_again` | 被拿着 `operations::exclusive()` 的命令调用的辅助函数（`managed_install`）自己再拿一次锁 —— tokio 的 Mutex 不可重入，第二次就是永远等着。0.22.6–0.27.0 软件页的「安装」与托管那份的「升级」一点下去就转圈不停、没有报错，正是它 |

**看门狗那一条尤其别删**：它顶替的是一道拆 crate 之后失效的编译期护栏，见上面那一节。
**锁那一条也别删**：编译器看不出死锁，单测碰不到 Tauri 命令，只有读源码钉得住。
`operations::exclusive()` 只在 `#[tauri::command]` 那一层拿，辅助函数一律不拿。

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

## ⛔ `test:ui` 也收 React 的「两条 key 一样」（2026-09-23）

`scripts/ui-regression.mjs` 的 `watchErrors` 除了抛出来的异常，还把开发模式下 React 的
`Encountered two children with the same key` 算页面错误。原来只收 `pageerror`，而这句只是一次
`console.error` —— 快速跳转拿 `path` 当 key（「官方账户」和每个 Claude 槽位都是 `/`），
收窄过滤之后列表里留着 21 条旧项、同一个账户画三遍，**这一圈一直全绿**。别把它删回只收异常。

同一次还学到：**新写的断言，先看它会不会红。** 把修复临时改回去跑一遍 ——
那次「收窄之后只剩一条」的断言用 `fill` 整串填时**照样绿**（一次重渲复现不出旧项），
改成逐字 `pressSequentially` 才红。使用者是逐字打字的，测试也要逐字打。
写出来就是绿的断言，跟 `real_multiplier` 那 20 条全绿的单测是同一类东西。

---

## 用量的美元、订阅页的倍率：两条口径（2026-09-24，使用者定的）

**用量按官方 API 价折算成美元**（同样的 token 走 API 要付多少）。使用者要的就是这个数（「要看见每天，
7 天，30 天花了多少刀的额度」），推翻了 `token_summary.rs` 原来那条「只给缓存省下的，不给花了多少」。
代价是它会被读成账单，所以：

- 「订阅不按这个收费」**跟数字放在一起**（卡片页脚、`/usage` 标题下），不许藏进悬停；
- 缓存写分 5 分钟 / 1 小时两档计价（1 小时 = 输入 ×2）—— 实机 Claude Code 的缓存写全是 1 小时档，按一档算少 37.5%；
- 认不出价的模型单独列，**不拿别的模型的价去凑**；四类全 0 的（`<synthetic>` 报错）不算回复、不入账，单独数。

⛔ Claude 的 5 小时 / 7 天**仍然从不联网**：使用者 09-24 被问到时选了「不加，保持只读本机」，
别接 `/api/oauth/usage`。

**订阅页的等效倍率按 1 美元 = 7 元折成元**再除（`src/features/subscription/multiplier.ts` 的 `YUAN_PER_USD`）。
中转站的倍率按站内额度算，常见充值是 1 元 = 1 美元额度，1× 就是「每 $1 牌价的用量付 1 元」；
官方订阅付的是真美元，美元直接除美元会把官方各档算便宜 7 倍（使用者拿计算器算出 0.1、表里却写 0.014，就是这个）。

- 页面上标「1:7」，写明 7 是往贵了取的、实际一般在 6.8 左右 —— ⛔ 别换成实时汇率，也别改回美元除美元；
- 同一档在页面上**只许一个数**：价目表、刻度条、首页两格、计算器都走 `planMultiplier`；
- 页面上只写「元」。订阅页「不提特定国家、货币」的其余部分照旧，`Subscription.test.tsx` 的 BANNED 盯着渲染出来的文字；
- 按 1:7 算，官方各档跟逆向中转是同一个价位 —— 页面上「比任何中转站都便宜」这类话不许再写回去（测试盯着）；
- 计算器里「中转站给你的额度」是站内余额，**按那条线路的分组倍率扣**：实际倍率 = 付的元 ÷ 额度 × 分组倍率
  （`relayMultiplier`，0.25.4；「中转站百科」帖：「输入 5 元，给你倍率 0.1，实际就是 5 × 0.1 = 0.5 元」）。
  少乘这一项会把中转算贵好几倍。

---

## ⛔ 「倍率」不是一个数

中转站的计费是四类各算各的。New API 系给的是一组比例，换成单价是这样（跟 New API 自己的定价页同一个算法）：

```text
输入   = model_ratio                        × $2/百万
输出   = model_ratio × completion_ratio     × $2/百万
缓存读 = model_ratio × cache_ratio          × $2/百万
缓存写 = model_ratio × create_cache_ratio   × $2/百万
再乘   × 分组倍率 ×（峰时浮动，取 max 当上界）
```

⛔ **`model_ratio` 是单价，不是「官方的几倍」**：单位是 $2 / 百万 token（New API 源码 `1 === $0.002 / 1K tokens`，
`pricing::NEW_API_USD_PER_MTOK`）。照官方价抄的 Opus 5 是 2.5。⛔ **`completion_ratio > 1` 不是「计费翻倍」**：
它是「输出价 ÷ 输入价」，Claude 官方本来就是 5。0.25.3 及以前两条都读反了 —— 照官方价收费的站在查套路报告里
是「输入 2.500×、输出 12.500×」，线路上挂着「翻倍 ×5」（7.91）。

所以任何一类的倍率只有一个算法：**站点单价（已乘分组）÷ 官方单价**（`category_ratios_against`）。
输出另外加没加价，看站点的输出 ÷ 输入比官方的多出多少（`output_markup`，界面挂「输出加价」）。
A 站分组 ×0.15 但把补全倍率调成官方的两倍、B 站 ×0.25 照官方填：纯读代码 A 便宜、长篇生成 B 便宜 ——
判定在 `pricing::StationRates::blended_ratio` —— 按这条线**实际的输入输出比**加权，**分组倍率不知道就不算**
（折扣大多在分组上，不打折的牌价跟别家的折后价没法比）。

### ⛔ 两种后端公布价格的形状不同，但最后都是单价

| 站点后端   | 它在 API 里给什么                     | 长什么样                       | 折扣在哪 |
| ---------- | ------------------------------------- | ------------------------------ | -------- |
| New API 系 | 倍率（1 = $2 / 百万 token 的单价）    | `model_ratio: 2.5`             | 分组倍率（`/api/pricing` **顶层** `group_ratio`），或直接压低倍率 |
| sub2api 系 | **绝对单价**                          | `input_price: 5`（美元／百万） | 分组倍率（`/api/v1/groups/rates`） |

使用者的原话：「newapi 是可以这样算，但 sub2api 不是，sub2api 的单价就是官方单价。」
「New API 的倍率算在了单价内，所以单价便宜。」—— 站长可以把折扣放在分组上，也可以直接压进 `model_ratio`；
两种都得先换成单价再除官方单价，读出来才是同一个数。

`StationRates` 有**两套互斥的字段**，哪一套有效由 `basis()` 从
「谁填了」推出来。**不设一个单独的 kind 字段** —— 字段和内容对不上时
（适配器改了、迁移漏了、手改过配置），字段会骗人而内容不会。

- `category_prices()` 两套都给单价（New API 乘 `NEW_API_USD_PER_MTOK`）。⛔ **官方价不是它的输入** ——
  下面那条「官方价被约掉」的失效，就是拿倍率乘**官方价**凑单价；
- `newapi_ratios()` 是 New API 的原始倍率乘积，只供对照，**不是倍数**（0.25.3 及以前叫 `category_ratios`，名字本身就是误读的入口）；
- 分组倍率按线路的分组从 `/api/pricing` 顶层读（`parse_station_pricing_in_group`，留空按 `default`，`auto` 不认）；
- 换成倍率**必须知道是哪个模型的**（官方价按模型查），所以 `Route` 有 `rates_model` —— 两种后端都要。

### ⛔ 跨站比较先折成同一种钱：充值比例（0.25.4，使用者定的）

账单、倍率都按站内额度记，而「1 美元额度付几元」每家站不一样（linux.do「中转站百科」帖说的「倍率陷阱」：
7 元一美元、标 ×0.1 的站比 1 元一美元、标 ×0.5 的还贵）。每个站点一格 `Provider.topup_per_usd`，留空按 1。

- `station_ops::decide` 把「便宜」的三种口径和「倍率不超过」底线都先乘上各自站点的比例；
- 底线比 `Candidate::real_rate`，**不比 `Candidate::rate`** —— 后者是「便宜」那一维的值，整池走实扣单价时是 1e-6 量级；
- ⛔ **查套路不折算**：那一步问的是「收的跟它自己说的对不对得上」，两边都是站内口径（使用者 09-18「不用管货币单位」）；
- ⛔ New API `/api/status` 的 `price`（后台「充值价格」）**只做参考按钮，不自动采用**：默认值就是 7.3，很多站没改过。

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

- **界面必须显示「输出加价」**（`StationCenter.tsx` 线路行那枚黄标，读 `Route.output_markup`，检验时拿官方价算好）。
  没算过时什么都不显示 —— **「不知道」不是「没加价」**；⛔ 别改回 `completion_ratio > 1`（见上面，7.91）；
- **新对话没有缓存读**（`TokenMix::fresh_conversation`）。靠高缓存命中撑起来的
  便宜线，在新对话第一轮上并不便宜；
- **有真实账单就用真实账单**。`24h 实扣 ÷ 实际 token` 排在加权倍率前面 ——
  那是实际付出去的钱，比任何推算都准。

### 官方参考价没有 API，只能抓文档页

**地址**（2026-09-14 实访确认，改之前先 curl）：

|           | 地址                                                  | 表格                                                                                              |
| --------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Anthropic | `platform.claude.com/docs/en/about-claude/pricing.md` | `## Model pricing` 一节，5 列：`基础输入｜5m 缓存写｜1h 缓存写｜缓存读｜输出`，模型列是**显示名** |
| OpenAI    | `developers.openai.com/api/docs/pricing.md`           | `### Standard pricing data` 一节，8 列：短上下文 4 列 + **长上下文 4 列**                         |

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

| 来源                   | 该做什么                   |
| ---------------------- | -------------------------- |
| 六项都测了、四项对不上 | 这站确实有问题，**换站**   |
| 只测到一项、其余没测到 | 还不能下结论，**再跑一轮** |

只给一个分数的话，两者在界面上长得一模一样。所以 `AuditRound` 同时带
`trust` 和 `evidence`（`EvidenceLevel`），**界面必须两个一起显示**。

---

## 文案里不要写 Markdown 的 `**`

Rust 侧传给界面的 `detail` / `manual` 之类是**纯文本渲染**的，
写 `**强调**` 会在界面上显示成两个星号。要强调就在 TSX 里用 `<strong>`。

（源码注释里的 `**` 不受影响，那是给读代码的人看的。）
