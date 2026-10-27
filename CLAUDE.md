# 改这个仓库之前先读这里

给接手这个项目的人（以及 AI 助手）看的硬规矩。每一条都是踩出来的，不是推理出来的。

实现层面的「为什么这么写」在 [docs/DESIGN-NOTES.zh-CN.md](docs/DESIGN-NOTES.zh-CN.md)，
修不掉的缺口在 [docs/KNOWN-ISSUES.zh-CN.md](docs/KNOWN-ISSUES.zh-CN.md)。

---

## ⛔ 一轮需求做完了，先问，别自己去出安装包

**做完使用者这一轮的需求之后，收工清单跑完、文档更新完，停下来问一句：
「出安装包，还是你还有别的需求？」**（2026-09-20 使用者定的）

两条路，别自己替他选：

| 他的回答         | 你做什么                                                                                                                                                                                                    |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **还有别的需求** | 只把文档更新到位（`CHANGELOG` 的 `## 未发布` 段 / `KNOWN-ISSUES` / `DESIGN-NOTES` / 本文件 / 桌面 `claude-op` 档案），**不出安装包，也不提版本号**。他会另开一轮对话接着提                                  |
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

| 时机                     | 版本号               | `CHANGELOG`                        |
| ------------------------ | -------------------- | ---------------------------------- |
| 一轮做完、他还有别的需求 | **一个字都不动**     | 追加到 `## 未发布` 那一段底下      |
| 他答「出包」             | 这时才提（五个位置） | `## 未发布` → `## <版本> — <日期>` |

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

**这两组就是 CI 跑的那一套**，别只跑前两条就以为绿了 —— `test:ui`（2026-09-27 是 151 个响应式/
主题组合 + 18 个交互流程；撤掉 Claude 汉化那一页少了 12 个组合，清废弃页面多了一个「删掉的页面回不来」；
2026-09-29 酒馆变成内置功能之后是 139 + 20：扩展中心酒馆页那条路由的 12 个组合没了，多了「三家设置按账户页分开」
「软件页 SillyTavern 卡」两个流程；**同一天第二轮（酒馆前端重构）之后是 139 + 23，0.25.6 就是这个数**：多了版本分页、控制台、角色卡 / 世界书 / 扩展三个流程；
中转站重构分支（还没发版）阶段 1–4 加到 139 + 26，阶段 5 收工时 139 + 27：多了「项目弹窗 · 粘贴导入链接 · 工具文件」一个流程；
阶段 6 收工时 **151 + 27**：中转用量页 `relays/usage` 那条路由的 12 个组合，会话分页、用量脚本、站点余额并进已有的流程）
和 `cargo deny`（advisories / bans / licenses / sources）
各自抓过别处抓不到的东西。`cargo test --workspace` 的通过数**只许涨不许跌**
（0.29.0 是 **1081**，0.30.0 是 **1106**，0.31.0 是 **1110**，0.32.0 是 **1154**，
0.25.1 是 **1204**，0.25.2 是 **1268**，0.25.3 是 **1288**，0.25.4 是 **1334**，0.25.5 是 **1360**，
0.25.6 是 **1396**（这一版攒了四轮：2026-09-27 撤 Claude 汉化之后是 1366，**清废弃页面之后是 1317**，
2026-09-29 酒馆变成内置功能之后是 1342，同一天第二轮酒馆前端重构之后是 1396）；
中转站重构分支（还没发版）阶段 4 收工时 **2777**，阶段 5 收工时 **2842**，阶段 6 收工时 **2899**；前端 182 → 195 → 215）。
⚠ **1366 → 1317 是这条规矩唯一的例外，而且每一条都对得上**：删掉 53 条、新加 4 条，净跌 49。删掉的全是
**被删掉的代码自己的测试** —— 旧中转站目录 `store.rs` 11、真实后端判定 `backend.rs` 7、纯净度自判 5、酒馆桥接旧槽位合并 4、
统一档案 4、草稿诊断 3、Hub 自动更新 2、桌面旧启动脚本搬家 2、进度计数 2、取消操作 / 回滚配置 / 两个「已取消」错误分类 /
测延迟 / 托盘旧菜单 id 各 1，外加 ts-rs 给 7 个被删的导出类型自动生成的 7 条；新加的 4 条是两条命令护栏、纯净度原文与门槛一致、
老 `progress.json` 读得进来。**别拿这个当先例**：删功能时测试跟着走是对的，删测试来让某个还在的功能「过」永远是错的。

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
   **发出去的说明写多细也由使用者定。** 0.25.5 使用者要「日志就写修复了若干 bug」：`CHANGELOG.md` 那一版和
   `.github/release-notes.md` 都只有这一句，逐条记录挪进了桌面 `claude-op` 的 `11`。**别把细节补回仓库。**

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

### ⛔ 三种关停，三个范围（2026-09-25）

`killswitch::Scope` 三档，筛法是纯函数 `in_scope`，**别合并成两档**：

| 调用方                           | 范围                                         | 入口                                            |
| -------------------------------- | -------------------------------------------- | ----------------------------------------------- |
| 使用者点「一键关闭」             | `Everything`：全收（含中转、含反重力），重锁 | `execute()`                                     |
| 门禁驱动的关停（`stop_managed`） | `Gate`：剔中转；反重力只在归门禁时收         | `execute_official()`                            |
| 切 Claude 账户之前的清场         | `AccountSwitch`：剔中转、剔反重力            | `execute_for_switch()` / `preview_for_switch()` |

原来只有前两档，切账户借用了「官方」那一档 —— 反重力是按安装目录认的，于是切 Claude 账户把 IDE 连同没保存的东西
一起关了，确认框却只数 Claude 的三类（只有反重力开着时写「会关掉：。」）；门禁那一档也不看 `antigravity_under_gate()`。
反重力是 Google 账户，跟 Claude 槽位无关，跟 `needs_clearing` 同一条。

**确认框扫的必须是真去关的那个范围**：切换框走 `official_switch_preview` → `preview_for_switch`，清场执行与清场后的
「剩没剩」复查也是 `AccountSwitch`。三处任何一处换了范围，报的数就跟做的事对不上。

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

| 不做什么                                                                        | 为什么                                                                                                                                                                                                                                                          |
| ------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **设备指纹伪装**（UUID / 主机名 / MAC / machine-id 改写）                       | 主要用途是多账号规避，与硬约束「所有账户必须本人拥有」直接冲突。DISCLAIMER 写死了「不对账户状态作任何承诺」与「不为规避封禁而设计，也无法达到该目的」（按这两句话去搜，别记行号 —— 行号每改一次 DISCLAIMER 就漂一次）                                           |
| **内置代理 / VPN**（自己当代理、mTLS 中继、链式转发、把**整机**流量强制走代理） | DISCLAIMER 第 5 节。做了这个，那一节就是假的。⚠ 0.19.0 开了两个**有边界的**口子（浏览器出站锁、系统代理修改），见下面「两个口子」一节；2026-10-24 本机路由的中转线路可以经**使用者自己填的**出站代理出去（R6，见「本机路由不在此列」）—— 那几处之外，这一条照旧 |
| **自动换号**（按额度、429、限流自动切槽位）                                     | 合规边界四条的前两条。存在这条路径，「多槽位」的定位就从「管理你自己的账户」变成了「规避限制」。联网问到的额度也一样：**只显示，不触发自动换号、模型路由或限流规避**                                                                                            |

（原来这张表还有一行「自动联网查额度」，写着「不做后台轮询」。2026-09-23 使用者把这条规矩删了 ——
现在联网额度怎么问是一个**设计**，不是禁令，写在下面「联网额度」那一节。）

检测与**如实报告**不在此列 —— 面板可以告诉你「你的时区和出口对不上」，
但不替你改机器身份。两者的区别是：前者让使用者知情，后者替他伪装。

### 联网额度：只在点刷新图标时问，一次一个账户（2026-09-23 改写，使用者定的）

0.32.0 这里还叫「反重力 Hub 的配额是全项目唯一一处联网」。之后使用者一步步放开：
09-21 授权 GPT 与 Gemini CLI 查官方内部额度接口；09-23 删掉「不做后台轮询」那条规矩、
要反重力账户照 cockpit-tools 把联网能拿到的都显示出来，并拍板「令牌过期时面板在内存里换新」。
2026-10-26 中转站重构阶段 6 他勾的 U2 把 Claude 官方订阅额度也接了进来（推翻 09-24「Claude 从不联网」），节奏照旧。
现在的来源：

| 问什么                                               | 走哪条路                                                                                                                                                | 什么时候联网                                |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| Claude 的 5 小时 / 7 天、按模型的周额度、超额用量    | Anthropic `api/oauth/usage`（那个槽位里 Claude Code 存的令牌，`usecase::claude_quota`，阶段 6 · U2）                                                    | 点那一行的刷新图标 / 详情里「向官方问一次」 |
| Claude 的本机读数                                    | 桌面端的 `plan-usage-history.json` + Claude Code 的快照（`qb-accounts` 的 `accounts/usage.rs`）                                                         | 从不                                        |
| GPT 的 5 小时 / 7 天                                 | ChatGPT `backend-api/wham/usage`，同一次并行问存下的重置 `wham/rate-limit-reset-credits`（阶段 6 · U2）；那个 Codex 槽位的令牌，`usecase::tavern_quota` | 点那一行的刷新图标                          |
| GPT 的本机快照                                       | Codex 自己写在会话记录里的 `rate_limits`                                                                                                                | 从不                                        |
| Gemini CLI 各模型额度                                | Code Assist `loadCodeAssist` + `retrieveUserQuota`（当前账户 CLI 那一半的令牌）                                                                         | 点用量卡 Gemini CLI 那一格的「刷新」        |
| 反重力账户：档位、AI 积分、Claude / Gemini × 5h / 周 | `loadCodeAssist` + `retrieveUserQuotaSummary`（免费档再加 `fetchAvailableModels`），`usecase::antigravity_quota`                                        | 点那一行的刷新图标                          |
| 反重力 Hub：同上                                     | 同上，令牌是 Hub 在凭据管理器里那一份                                                                                                                   | 点用量卡 Hub 那一格的「刷新」               |
| 反重力 IDE 写在本机的邮箱 / 档位 / 各模型比例        | `state.vscdb` 的 `userStatus`                                                                                                                           | 从不（没点过刷新时的兜底）                  |
| 反重力 Hub 的邮箱                                    | 凭据里 `id_token` 的载荷，本机 base64 解码                                                                                                              | 从不                                        |

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
   ⛔ **Claude 的令牌面板也永远不换**（阶段 6 · U2，`usecase::claude_quota`）：理由同 GPT，刷新令牌一换，
   槽位里 Claude Code 那份就作废。访问令牌按 `.credentials.json` 的 `expiresAt` 到点了就**不发请求**，
   如实说「用这个账户跑一次 Claude Code，它会自己换新，再点刷新」。cc-switch 会拿过期令牌硬问一次、
   把 401 当结果，这里不照做。
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
   IDE 本机那份（`AntigravityModelQuota`）与 Gemini CLI 的桶（`TavernGeminiModelQuota`）同一条（2026-09-25 补上）。
7. **请求形状照官方 IDE**：`User-Agent: antigravity/<本机 IDE 的 ideVersion> windows/<arch>`，
   `metadata.ideType = ANTIGRAVITY`。版本从 `product.json` 读，不写死（开源兼容性那一节）。
8. **「登录已失效」只来自服务端的明确拒绝**（2026-09-23，`usecase::login_health`）。本机文件只说得出
   「有没有令牌」；点刷新时 Google 回 `invalid_grant`、或 GPT / Claude 的访问令牌**没到点**却被回 401 / 403，才记一笔，
   账户行上那一半改说「登录已失效」、露出「登录」。按**凭据文件的修改时刻 + 长度**记、不读内容，
   官方客户端重新登录或自己换新了令牌（文件一变）就自动作废；使用者点「登录 / 打开」时也清掉
   （`codex_accounts::launch`、`antigravity_ops::gemini_login`、Claude 那一笔在 `launch_claude` 里调 `login_health::forget`，2026-09-25 补上 ——
   原来只在移除槽位时清，点了登录那一行照样挂着「登录已失效」）；只在内存里。原因文案一律以
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
`cargo run -p qb-accounts --example antigravity-hub-cred`（看那条凭据的形状与令牌何时过期）、
`cargo run -p qb-accounts --example codex-ratelimits`（看 Codex 的额度记录），
与 `cargo run -p qb-probe --example dns-leak`（真跑一次 DNS 泄露的 bash.ws 回显，看回的是数组还是对象、
认出几条、几个探针被 fake-ip 接住；**一个地址都不打印**。2026-09-25 有使用者那边回的是对象，
原来只认数组，整次检测报「JSON 解析失败」）。

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

路由器本身的三条硬约束（`crates/qb-app/src/local_router/`，2026-10-24 拆成目录）。2026-10-24 中转站重构 R6
（使用者在 35 项勾选里选的「上游出站代理 + 可改监听地址」，交接文档 §1.4）把其中两条改成了**有边界的**写法：

1. **默认只绑回环；开到所有网卡上就必须带路由口令。** 默认 `127.0.0.1`。使用者在「路由设置 · 网络」里改成
   「所有网卡」（`0.0.0.0`）时：没有口令不起（`serve` 在绑之前就拒，`opening_the_router_without_a_key_refuses_to_start`）；
   **从别的机器来的每一发**要带随机口令（`x-api-key` 或 `Authorization: Bearer`），对不上 401（`local_router::admit`）；
   验过的口令跟客户端别的凭证一起剥掉，到不了上游；**官方 Codex 那条线只给本机**（403）。
   本机进程不查口令 —— 跟只绑回环时同一个信任面：本机进程读得到配置文件里的口令，查它挡不住谁，
   只会让已经开着的客户端全部 401（交接文档 §1.4 原写「客户端配置里的 `ROUTER_KEY` 换成这个口令」，
   照这个理由改成了只查别的机器来的）。界面写明 Windows 防火墙会弹窗、拿到口令的人能用你的线路额度。
   口令 = 两段 v4 UUID（操作系统的密码学随机数），可以「换一个」，旧的立刻失效。
2. **不改系统代理设置**（不变）。
3. **出站代理只给中转线路，官方线永远直连。** 中转线路的请求可以经**使用者自己填的** HTTP / SOCKS5 代理出去
   （`RouterState::relay_http`）；官方 Codex 那条线永远用直连的客户端（`official.rs`）—— 走代理的话，
   门禁核过的出口就不是它真走的出口。`the_outbound_proxy_carries_relay_lines_and_never_the_official_one` 钉着。
   这不是「内置代理 / VPN」：面板自己不当代理、不接管别的程序的流量，只是让**指到它身上的那几发中转请求**经使用者
   已经在跑的那个代理出去。代理地址可能带用户名密码：只存在库里，给界面、日志、报错的一律去掉那两样（`mask_proxy_url`），
   代理框不回填（回填再保存会把密码冲掉）。测代理连的是**使用者自己加的那条线**，不是公共网站（联网清单里没有它们）；
   「扫描本机」只连 `127.0.0.1` 的几个常见端口、不发数据。两样都只在点的时候跑。
4. **reqwest 的错误文字先 `without_url()`** 再进日志 / 报错 / 回给客户端：那段文字里带着整个请求地址（含查询串），
   有的上游把 Key 放在查询串里。`a_dead_outbound_proxy_is_reported_without_its_password_or_the_target_path` 钉着。

这几条是 DISCLAIMER 第 5.1 节的措辞依据 ——
**动了其中任何一条，就要同步改那一节**，否则免责声明描述的是另一个软件。

#### 本机路由的数据面（2026-10-24，中转站重构阶段 1，照搬 cc-switch v4.0.14）

| 文件           | 管什么                                                                                            |
| -------------- | ------------------------------------------------------------------------------------------------- |
| `mod.rs`       | `RouterState`（当前上游、派单顺序、每个软件的路由设置、熔断器、缺模型记忆、日志缓冲）与共用小工具 |
| `server.rs`    | 监听、认软件、绑这个软件此刻的当前线路（拷贝）、收齐请求体、问聚合模型那一层，分给下面两条路      |
| `aggregate.rs` | 聚合模型（阶段 3）：模型发现本机自己答；带前缀的模型直达名单里那一条，解不出来报错、不回落        |
| `forward.rs`   | **中转线路**的重试循环（cc-switch `forward_with_retry_inner`）                                    |
| `official.rs`  | **官方 Codex**（turn-state）：不进重试、不换线、不整流，原样直通                                  |
| `log.rs`       | 一发一条 `RouterRequest`，边转边解用量，后台每 5 秒批量落 `router_requests`                       |

要守住的：

1. **换线只发生在「交出回复之前」。** 交出去之后上游再断，只能如实断开 —— 再换一条线等于让客户端收到两段拼起来的回复。
   `a_stream_that_breaks_after_it_was_handed_over_is_not_retried` 钉着。
2. **换不换、伤不伤熔断，只由 `qb_proxy::classify` 一处定。** 400 / 405 / 413 / 422 这类请求自己的错**原样回、不换线、熔断记中性**
   （`a_400_goes_straight_back_without_trying_another_line_or_hurting_the_breaker`）；404 / 409 / 451 换线但不伤熔断；
   缺模型只跳过「这条线上的这个模型」6 小时。别在 `forward.rs` 里另写一份状态码判断。
3. **官方线永不当替补、永不换线**（`the_official_codex_line_never_fails_over_to_a_relay`）。中转线失败换到官方线，
   等于拿中转那一发带着官方身份打出去；官方线失败换到中转线，等于拿官方身份的请求去打中转的 Key。
4. **全都熔断着时仍打当前那一条**，不凭空回 503 —— 原来就是这样，不制造死局。只有一条线时不问熔断放行，但照样记结果。
5. **转给中转线路的请求一律 `accept-encoding: identity`**，reqwest 仍然不开 `gzip`（开了它会悄悄解压、改写流）。
   要从响应里读用量、认出 2xx 里的错，就得拿到没压缩的字节。官方线照旧原样转客户端的头。
6. **首包到了之后最多再压 10 秒**（`forward::MAX_HOLD`）等「第一个有内容的事件」，压满就照原样交出去。
   别改成「等不到就判超时」：推理很久才出第一段的长回合开头只有心跳，那样会被当成故障换线。
7. **静默超时不比客户端自己的短**：Codex 默认 300 秒（Codex 自己的 `stream_idle_timeout_ms`），Claude 180 秒（cc-switch）。
8. **日志里不记正文、不记 Key**：`error` 那一栏对上游错误只写「类型（HTTP 状态码）」—— 上游的错误体可能把请求内容回显出来。
9. **测试数只许涨**这条在这一步的对账：`a_codex_200_with_a_failed_sse_stream_is_counted_as_a_fault` 等老的端到端测试一条没删，
   只给 `Upstream` 字面量补了 `..Default::default()`；`router_settings_default_to_cc_switch_s_per_app_table_with_failover_on`
   里 Codex 静默超时那一项从 120 改写成 300（见第 7 条）。
10. **模型映射挂在线路上、只改顶层 `model` 一个字段**（`Route.model_map` → `Upstream::with_model_map`）：先逐个指定、再档位、最后兜底；
    换线之后用的是那条线自己的那份。**官方线不换模型名**。`qb-station::RouteModelMap` 与 `qb-proxy::ModelMapping`
    同在第 3 层、互不依赖，换算放在 `qb-app`（别往 `ALLOWED_SIDEWAYS` 里补边）。`Route.upstream_format` 阶段 3 起生效，见下面「协议转换与聚合模型」。
11. **路由设置与派单队列是落盘的承诺**（迁移 6：`router_settings`、`dispatch_queue`，一个软件一行，主键是软件名）。
    读的是库里那份、存了立刻 `set_settings` / `set_queue` 到路由器上；只改界面 state 的话路由和界面会漂开。
    不合格的设置**整份拒绝、不夹到范围里**（`RouterSettings::validate`）；派单顺序混进别的软件的线整份拒绝。
    手动选的、调度换过去的都 `remember_current`；面板启动时 `restore_router_state` 装回去，**只在那个软件眼下还没选时**。
    ⛔ 面板启动**不拉起路由**（监听端口）—— 那是使用者还没回答的问题（交接文档阶段 1 第 8 步），别顺手加上。
12. **排序的证据：本机路由记过的线用它自己的日志，没记过的才读站点账单**（`station_ops::decision_health`，
    `station_decide` 与调度那一跳都用它）。一发请求试过几条线，那几条各记一行（`router_usage_rows`）：换走的那次算失败，
    token / 首字 / 花费只记在最后回话的那条上；请求本身的问题、客户端断开、这条线不接、缺模型、本机这一侧**不记**
    （跟熔断那边记中性一致），整流前那一次也不记。有本机证据的线这一跳**不去拉账单**。
    ⛔ 请求日志弹窗里那张「站点账单口径」的卡（`station_refresh_health` → `route_health`）照旧只看账单 —— 两种口径不混在一张卡上。

#### 协议转换与聚合模型（2026-10-25，中转站重构阶段 3，照搬 cc-switch v4.0.14）

转换全套在 `qb-proxy::convert`（cc-switch `proxy/providers/*` 原文件名照搬），判断与改写的入口是 `convert::plan`，
`forward.rs` 只接线；聚合模型的 id 编解码在 `qb-proxy::aggregate`，名单解析与每发请求的判断在 `local_router/aggregate.rs`。

13. **线路的「上游协议」没写 = 跟软件一样 = 原样转发**，请求一个字节都不改（只有 cc-switch 原生透传分支里那几样修补：
    Claude 给 Anthropic 上游的 DeepSeek 类历史规范化、Codex 给 Responses 上游的 xAI 改写与迟到参数修补）。
    写了且跟软件不一样才转换：Claude 两个 → Responses / Chat / Gemini 原生；Codex → Chat / Anthropic。
    **Codex 转 Gemini 没有这一对**：选到它回 `ConfigError`，跳过这条线、不伤熔断（`a_codex_line_marked_gemini_is_skipped_without_hurting_its_breaker`）。
14. **鉴权头跟着上游说的协议走**（`Upstream::relay`）：Anthropic 用 `x-api-key`，OpenAI 两种用 Bearer，Gemini 用 `x-goog-api-key`。
    测活（`station_liveness`）按同一个协议打同一个路径、带同一种头 —— 测活跟真实流量走两条路，测活就测不出真实流量的死活。
15. **出错按客户端那一侧的格式回**（`convert::error_mapper`）：Claude 收 Anthropic 的错误信封，Codex 收 OpenAI 的。
    转换的线答了解不开的东西，按「这条线坏了」换下一条（`a_converted_line_that_answers_garbage_hands_over_to_the_next_line`）。
16. **官方 Codex 那条线永远不转换、不聚合**：它就是 Responses，走 `official.rs` 原样直通。
17. **转换要跨请求记的两样东西只在内存里**（`RouterState` 的 `gemini_shadow` / `codex_chat_history`）：面板重启就清空，
    最多那一轮少带一点上下文。别落盘 —— 里面是对话内容。
18. **`Protocols::supports` 是「能直通或能转换」，`Protocols::native` 才是「能直通」**（阶段 3 之前 `supports` 的意思）。
    线路出现在哪个软件分页看前者；探完协议「上游协议」建议填什么看后者（直通**明确**不行、别的探通了才建议转换，探不出来不猜）。
19. **聚合模型（cc-switch 的 Stack）**：名单里每条线的模型以带 `qbg` 前缀的 id 发布（Claude Code `qbg-claude-<key>--<模型>`、
    Codex `qbg-<key>/<模型>`、桌面端 `claude-fable-qbg-<6 位数字>`），选中的那一发**只打那一条：不换线、不读也不写熔断**
    （`an_aggregated_line_that_fails_neither_fails_over_nor_trips_its_breaker`）；不带前缀的照旧走派单。
    - **带前缀却解不出来一律 400，不回落**（`an_aggregated_id_that_cannot_be_resolved_is_refused_without_touching_any_line`）：
      回落等于拿别家的钱、别家的模型回答，使用者看不出来。聚合关了也一样报错。
    - **key 由后端分、只增不减**（`station_ops::settle_aggregate`，`aggregate_keys_are_handed_out_by_the_backend_and_never_move`）：
      界面传来的 `keys` 一概不认；移出名单、删掉线路都不删登记 —— key 换了指向，客户端带着的旧 id 会被悄悄发到另一条线。
      名单只收这个软件自己的线，混进别的软件的、官方 Codex 那条，整份拒绝。
    - **Claude 两个的模型发现（`GET /v1/models` 带 `anthropic-version`）聚合开着时本机自己答、不联网、不进请求日志**
      （`model_discovery_is_answered_locally_only_while_aggregation_is_on`）：落了日志会给当前那条记一发假的「秒回」，混进第 12 条的证据。
      聚合关着照旧转给上游。
    - ⛔ **官方 Codex 是当前那条时，带前缀的模型一律拒绝**（`aggregated_models_are_refused_while_the_official_codex_line_is_current`）：
      Codex 这时带着官方登录的 `chatgpt-account-id` 等身份头，转去中转站就把官方身份交给了第三方。
    - **阶段 4 起写进客户端配置**（`relay::client_models`，2026-10-26）：Claude Code 写 `settings.json` 顶层 `modelPicker`、
      桌面端写 `inferenceModels`、Codex 写一份模型目录（`qb-gate-model-catalog.json`）再拿 `model_catalog_json` 指过去。
      ⛔ 只增删自己那几行（`qbg-` / `-qbg-`），使用者自己写的行、指着别处的 Codex 目录一律不碰（指着别处就不写、记下原因）；
      ⛔ **Codex 目录 = 自带那一份原样 + 聚合的行接在后面**：设了 `model_catalog_json` Codex 就只认目录里的模型，
      少了自带那一份内置模型就全没了。自带那一份现跑 `codex debug models --bundled` 拿（`aggregate_published::codex_bundled`），
      跑不了就不写目录。聚合的行用中性模板（不带自定义工具），原生 Responses 线上同名的 GPT 照抄官方那行。
      发布了哪些记在库里（`aggregate_published`），由路由设置存、线路存 / 删、面板启动时同步，变了才重写那个软件的配置；
      写配置的仍然只有 `workspace::configuration_files` 一处。写不进去不挡启动，原因在路由设置的聚合那一节显示。
      钉着的是 `relay::client_models` 的四个和 `workspace` 的 `published_aggregate_models_reach_the_client_configs`。
20. **Codex 远程压缩（V2）的原生 Responses 那一半没搬**（cc-switch `codex_compaction::create_native_compaction_sse_stream`）：
    Codex 只对名叫 `OpenAI` 的 provider 发压缩触发，QB Gate 写的中转环境 provider 叫「本机路由」，这条路现在走不到。
    阶段 4 配置写入引擎要是改了 provider 名，**同一个提交里把它搬上**，否则 Codex 压缩时会直接 Fatal。
    Chat / Anthropic 两种转换里的压缩处理已经搬了（在转换器里）。Copilot / Codex OAuth / xAI OAuth 的专用分支一概不搬（`convert/mod.rs` 写明了每一处）。

### 体检的无凭据探测与一次性采集页（2026-09-24，使用者选的）

对照 zzusec/CheckClaude，使用者选了三组：服务可达 + claude.ai 解析、代理形态 + IPv6 真实出口、
真实浏览器采集（三路出口没选）。多出来的联网与本机服务边界如下，**动任何一条同步改
[DISCLAIMER.md](DISCLAIMER.md) 的联网清单**：

| 做什么                                                                                          | 边界                                                                                                                            |
| ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| POST `api.anthropic.com/v1/messages`（body `{}`）                                               | **不带任何凭据**，只看状态码（401 放行 / 403 拦截）。⛔ 别为了「更准」塞 OAuth 令牌或 API Key —— 那就成了拿使用者的账户去试探   |
| GET `claude.ai` / `www.anthropic.com` 的 `robots.txt`                                           | 同上，无凭据；带 `cf-mitigated` 的 403 是 Cloudflare 验证页，算「查不出」                                                       |
| IPv6 回显（`api6.ipify.org` → `ipv6.icanhazip.com` → `v6.ident.me`）与 `ipinfo.io/<v6>/country` | 只用**只有 AAAA** 的服务 —— 双栈域名在没有 v6 时退回 v4，得出错结论                                                             |
| 真实浏览器采集（`qb-app::browser_probe`）                                                       | 只绑 `127.0.0.1:0`、路径带随机令牌、校验 `Host`、只收一份 ≤ 64 KB、120 秒、收完即关；结果只在进程内存（不落盘、不进日志与审计） |

四样都**只在使用者点体检 / 点「用默认浏览器测」时跑**，没有定时器；结果只进评分与报告，**不进门禁判定**
（跟出口一致性同一条）。

### 应用自己的更新：启动时问一次，点了才装（0.25.3，使用者定的）

使用者 2026-09-24：「我 GitHub 上上传了新版本，已经安装老版本的用户打开软件时（弹窗）收到更新通知，还有一键更新功能」。
问他时定了两件：**验真用同一个 Release 里的 `SHA256SUMS.txt`**（不另管 Tauri updater 那把签名私钥 —— 丢了私钥，
已装的人就再也收不到一键更新）；**带这个功能的第一版是 0.25.3**。代码四处：
`crates/qb-install/src/install/self_update.rs`（问、下、核；纯函数有单测）、`src-tauri/src/update.rs`（内存里的状态、
退出时交给安装包）、`src/features/UpdateDialog.tsx`（全局只挂一份的弹窗 + 启动那一次检查）、设置页「软件更新」。

| 做什么       | 边界                                                                                                                                                                                                                                          |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 启动时问一次 | 只取 `github.com/<REPO>/releases/latest/download/update.json`，**不走 `api.github.com`**（未登录一个出口 IP 一小时 60 次，共用 VPN 出口的人会撞上限、永远收不到提醒）。设置 `update_check_on_start`（默认开）关掉就一个请求都不发；没有定时器 |
| 弹窗         | **只提醒、不自动装**。「以后再说」下次启动再弹；「跳过这个版本」记在 `update_skipped_version`，只经 `update_skip` 改（`settings_save` 保留旧值）。安装的代价常驻在按钮上方，不放悬停                                                          |
| 一键更新     | 下载地址按常量拼（`update.json` 只提供版本号与说明，里面没有一个字段会被当成地址）；先取那个标签下的 `SHA256SUMS.txt`，固定名安装包与带版本号那份的哈希必须一致；边下边算，**对不上就删掉、不装**                                             |
| 交给安装包   | `app.exit(0)` → 退出处理**先重锁、再**启动安装包（`/P /UPDATE /R`：被动模式、覆盖安装不动快捷方式、装完重开面板）；启动前再算一遍哈希                                                                                                         |

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
`crates/qb-app/src/local_router/official.rs`（`UpstreamAuth::OAuthPassthrough` 官方模式 + 采集/注入）。

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
   不剥不换；其余上游仍走 `CLIENT_AUTH_HEADERS` 剥离 + 换上线路自己的 Key。不改系统代理；
   2026-10-24 起路由可以开到所有网卡上、中转线路可以走出站代理，但**官方线只给本机、永远直连**
   （见上面「本机路由不在此列」那一节的三条）。
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
（Claude 那条桥仍是他自己的 `bridge.py`，面板不分发）。2026-09-29 起酒馆不在扩展中心了，是面板的内置功能
（见下面「酒馆是内置功能」一节）；桥接这几条约束不变。代码在
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

⛔ **拼提示词时只把开头那段 system 提进「系统设定」**（`frame_with_prompt`，GPT / Gemini 共用；2026-09-26）。
对话开始之后酒馆插进来的 system —— 作者注释、按深度注入的世界书、预设贴在最后的字数 / 格式要求 ——
**留在它原来的位置**。原来是全部提到最前面：离回复最近的那条要求被挪到几千字之外，字数与格式就不灵了（坑 7.99）。
使用者那份 `bridge.py` 一直是这么做的，别把这里改回「system 一律置顶」；单测钉着顺序。

⛔ **等酒馆就绪的窗口别再调小**（0.31.0）。SillyTavern 自带的 `Start.bat` **每次都先跑一遍
`npm install`** 再 `node server.js`：本机实测（依赖全热）从起进程到 `GET /` 回 200 要 **47.2 秒**，
冷启动更久。窗口曾经是 60 秒，超时走回滚**把这次起的酒馆杀掉** —— 于是每点一次「起 GPT 酒馆」
就把上一次正要起来的那份掐掉一次，怎么点都失败（审计日志 09-21 02:43:49 起、02:44:57 收，
正好 68 秒）。现在 180 秒、等待期间每 4 秒把「已等多久」刷到界面上，**等超了不杀**：
进程活着就留着，如实说「可能还在装依赖，起来之后再点一次会直接复用」。
健康检查一律 `.no_proxy()` —— reqwest 默认读 `HTTP_PROXY`，会把对 `127.0.0.1` 的探测送去代理。

**三条桥的设置全由面板管，在软件页各自那张 CLI 卡的最下面**（0.31.0 起由面板管）。2026-09-29 使用者先要
「将酒馆插件的设置页写入本项目的控制面板内，而不是用打开本地链接的形式，三种不同的厂商按账户页分开」—— 当天放进了三个账户页
酒馆磁贴右上角的弹窗；同一天第三轮他说「错了错了，1弹窗太小，2这个应该在软件页面，软件页面的应该在这个官方账户弹窗页面」，
于是对调，挂哪张卡是问他时选的「分别挂到各自的软件卡下」：Claude → Claude Code 卡、GPT → Codex CLI 卡、Gemini → Gemini CLI 卡，
每张卡最下面一段「酒馆 · X 桥接设置」（`src/features/tavern/bridge/BridgeSection.tsx`），**只装这一家那条桥的全部设置**。
「还缺什么」共用 `lib/tavernReady.ts` 的纯函数，各写一份迟早会互相矛盾；账户页的酒馆磁贴在「还没配好」时按 `tavernBodyMissing`
分路 —— 缺酒馆本体（目录、启动脚本、Node.js）去软件页的 SillyTavern 卡，缺桥（bridge.py、CLI、槽位）去软件页这一家那一段
（`/software?focus=bridge-<家>`，自动展开并滚过去）。

⛔ **账户页酒馆磁贴右上角那颗小按钮开的是「酒馆」弹窗 —— 酒馆本身的控制面板**（2026-09-29 第三轮）。
三个账户页开的是**同一个**组件 `src/features/tavern/TavernPanel.tsx`（酒馆只有一份，`provider` 只决定底部那颗「这一家桥接设置」去哪）：
`Modal size="full"`（几乎占满窗口、**高度固定**，切分页不跳 —— 使用者嫌原来那个「太小」），分页 概览 · 控制台 · 角色卡 · 世界书 · 扩展 ·
备份与回收站（`card/` 下各一个组件，**只挂当前那一页**；关着的弹窗什么都不渲染、什么都不读），底部钉住「停止酒馆」（代价常驻：停的是
整个酒馆和三条桥）与去软件页的「下载与版本」「这一家桥接设置」。角标（`Tile` 的 `corner`，磁贴本身是 `<button>`，角标只能是兄弟节点）
的可访问名是「酒馆面板」—— 旁边那格磁贴自己叫「酒馆」，同名的两颗按钮读屏分不开。
由来：0.31.0 那个角标开的是写在 `AntigravityBand.tsx` 里的一段内联 JSX（只有反重力页开得了）；0.32.0 抽成 `BridgeSettings.tsx`（桥接设置）；
第三轮换成现在这个，`BridgeSettings.tsx` 删了。**「三家并排」（`provider="all"`）那种形态删了**，扩展中心的酒馆页也删了（见「酒馆是内置功能」一节）。

⛔ **软件页那三段默认收起，展开了才挂正文**（`BridgeSection`；没用 `ui/Collapsible` —— 它把正文渲染成 `hidden`，等于每次进软件页都去探
bridge.py、读消息监控、读额度，而人多半是来装软件的）。标题行里**不放 `.pill`**（`test:ui`「Pill 与版本行说同一件事」读整张卡的全部 `.pill`）。
展开之后按 `provider` 分给 `bridge/ClaudeBridge.tsx`（bridge.py 自己那两个本机网页的全部内容，`bridge/claude/` 下四段）或 `bridge/CliBridge.tsx`
（GPT / Gemini 那条内置桥的全部设置，差异全在 `bridge/cli/meta.ts` 那张表里，额度卡两家共用一份 `bridge/cli/Quota.tsx`）。
名字与地址（`BRIDGE_TITLE` / `BRIDGE_CARD` / `bridgeHref`）在 `bridge/where.ts`：快速跳转、「酒馆」弹窗、三个账户页只引它，**不引 `BridgeSection`** ——
后者会把整套桥接界面拖进首页那一包，而软件页是按需加载的。
⛔ **桥接设置里不做分页**：`test:ui` 查「每一段只有自己那条桥的字段、别家的一项都不许出现」，分页会让那条检查只看得到当前那一页；
长的那份（Claude）用钉在顶上的一排「跳到」（`bridge/SectionNav.tsx`）。
两条别改回去：

- **一家的字段只在一处。** 同一个字段两处各放一份，迟早出现一个没保存、另一个显示旧值的局面
  （坑 7.58 那种「同一块屏幕上两句矛盾的话」）。三家共用的酒馆端口在软件页的 SillyTavern 卡上，「酒馆」弹窗底部只读地提一句；
  `test:ui` 查「酒馆」弹窗的六页里一个桥接字段都没有。
- ⛔ **同一份 `TavernConfig` 有三处在改**（软件页 SillyTavern 卡：路径与酒馆端口；Claude Code 卡那一段：bridge.py 目录与端口；
  Codex CLI / Gemini CLI 卡那一段：各自的端口、模型、提示词……）：**每处只存自己那几个字段**（`features/tavern/saveTavernConfig.ts`
  先读最新一份再覆盖本处的键）。拿整份旧草稿去存会把别处刚改好的值盖回去，而且没有任何报错。

### Claude 桥的设置页与监控页收进面板（0.32.0，使用者定的）

使用者看到的那个「控制面板」**不是面板的东西** —— 它是 `bridge.py` 自己起的两个
HTML 页（`GET /settings` 与 `GET /monitor`），由他本机那份 SillyTavern 扩展
（`claude-tavern-bridge`，**面板不分发它**）用 `<iframe>` 嵌进酒馆的扩展抽屉里。
改一句提示词要先起酒馆、进抽屉、等 iframe 加载；而用 GPT / Gemini 那两条桥聊天时，
那个 iframe 指着没人听的 5001，弹 `ERR_CONNECTION_REFUSED`（坑 7.66）。

现在面板直接调它的 HTTP 接口（`plugins::tavern_bridge_api`），七个旋钮与调用日志
做成原生界面。2026-09-29 起它们在「官方账户 · Claude」酒馆磁贴右上角那个弹窗里
（`src/features/tavern/bridge/ClaudeBridge.tsx` + `bridge/claude/`；原来的 `src/plugins/ClaudeBridgePanel.tsx` 连同扩展中心的酒馆页删了），
并补齐了原来只有那两个网页才有的两样：**SDK 会话**（`/health` 的 `sdk_sessions.sessions`）与**逐条「展开输入 / 输出」**
（`/api/telemetry?id=`，只在点展开时读，里面是使用者的聊天内容，只在本机显示、不落盘不进日志）。
使用者本机那份酒馆扩展 `claude-tavern-bridge`（**仓库外、面板不分发**）同一天改成只报三条桥在不在、指路到面板，
不再嵌 `127.0.0.1:5001/settings`、没有「独立打开」（按聊天分会话的请求头照旧；改前备份
`*.before-panel-settings-20260929.bak`）。别人机器上的那份扩展（或者根本没有）不受影响 —— 面板不依赖它。四条硬要求：

1. **一律 `.no_proxy()`。** 坑 7.64 就是 `tavern_healthy` 漏了这一句。
2. **鉴权只在真要发请求那一刻读 `bridge-token.txt`**，不进 status / 日志 / 事件。
3. ⛔ **能力探测，不许假设。** `bridge.py` 是使用者自己那份，别人机器上可能是别的版本、
   甚至根本没有。连不上 / 404 / 字段缺，一律如实说「这份桥接没有这个接口」或
   「Claude 桥接没在跑」，**不许把空表显示成 0 条**（§7.17）。
   错误文案要**能照着做**：连不上 = 去起酒馆，404 = 升级 bridge.py，401 = 密钥对不上。
4. ⛔ **字段名照 `bridge.py` 的源码和录下来的真实回复写，不照想象写**（2026-09-26，坑 7.98）。
   0.32.0 的调用日志解析器读的是一套从没存在过的字段名（`at` / `input` / `output` / `elapsed_ms` / `countable`），
   单测拿同一套想象出来的形状喂、全绿，而消息监控每一行都是「输入 0 · 输出 0」。现在单测里放的是对着本机
   正在跑的 2.4.0 录下来的一页（`started_at`、`input_chars` / `output_chars`、`usage`、`cache`、`prefix.state`、
   `duration_ms`；命中率是**百分数**）。读不到的数是 `None`，界面显示「—」。

「系统提示词模式」那一格**只写事实，不写推荐**（2026-09-26）。抓过 `claude -p` 按桥的原样参数真正发出去的请求：
`append` 时使用者的规则排在 Claude Code 自带的编程助手提示词后面（那段里有「回答要简短」「结尾一两句总结」），
`replace` 只发身份一行 + 使用者的规则。原来写「append（推荐）」，对写长篇的角色扮演是反的；选哪个是使用者的事。
抓包做法（导入 `bridge.py` 自己的拼命令函数、空的临时配置目录、假 API Key、只绑回环的假端点）记在 DESIGN-NOTES。

模型那 13 个、effort 那 5 档是按 `bridge.py` 2.4.0 抄的**快照**，而
**桥当前用着的那个值永远算合法选项**（`with_current`）—— 不然上游加一个新模型时，
一打开设置就把人家的模型悄悄换成列表里的第一个。保存被拒就把桥回的原话照搬到界面上。

⚠ 改这七项里任何一项，`bridge.py` 都会 `cancel_active(retire=True)` 退掉当前那条 SDK 会话 ——
代价常驻在保存按钮旁边，不放悬停提示。bridge.py 的目录与端口在同一个弹窗的「连接」一段；
酒馆本体的路径 / 资产 / 备份在软件页的 SillyTavern 卡。

## ⛔ 酒馆是内置功能：软件页托管 SillyTavern（2026-09-29，使用者定的）

使用者：「扩展页的酒馆删掉好了，酒馆变成本项目的功能之一，然后软件页也增加下载酒馆的功能，然后被本项目接管」。
问他时定了：三家共用的酒馆本体放软件页一张卡；托管做到**安装 + 检查新版 / 更新 + 卸载**；他现有那份
（git 克隆装的 1.18.0，在另一块盘上）**装托管版时把数据复制过来、原来那份不动**；没有 Node.js 时确认框里给
「先用 winget 装 Node.js LTS」的勾选。

| 在哪                                                                                                 | 管什么                                                                                                                                                                                                                                           |
| ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 三个账户页的酒馆磁贴                                                                                 | 起酒馆（按厂商起那条桥）；右上角是这一家的桥接设置（上面那两节）                                                                                                                                                                                 |
| 软件页「SillyTavern（酒馆）」卡（`src/features/tavern/SillyTavernCard.tsx` 外壳 + `card/` 下的分页） | 2026-09-29 第二轮起分页：概览（运行、三家去哪起、依赖、位置与端口）· 版本（下载安装 / 检查新版 / 更新 / 指定版本 / 换回 / 卸载，确认框在 `card/SetupDialogs.tsx`）· 控制台 · 角色卡 · 世界书 · 扩展 · 备份与回收站（资产盘点、备份恢复、回收站） |
| `crates/qb-install/src/install/sillytavern.rs`                                                       | **位置表只有这一张**：托管目录 `<托管根>\sillytavern`、同级的 `.previous` / `.staging` / `.work`、标记 `.qb-managed.json`                                                                                                                        |
| `crates/qb-install/src/install/sillytavern_setup.rs`                                                 | 取最新版、下载、解压、导入、npm、就位与回滚、卸载（纯函数都有单测）                                                                                                                                                                              |
| 扩展中心                                                                                             | **没有酒馆了**。目录条目删了；老库里那行 `application-sillytavern`（原来「接入已配置的本地安装」写的）行不删、不列出来（`extensions::RETIRED`），「接入」那条命令连同按钮删了                                                                    |

九条要守住的，动任何一条同步改 [DISCLAIMER.md](DISCLAIMER.md) 第 1 / 6 / 7 / 8 节与 [ATTRIBUTION.md](ATTRIBUTION.md) 的 SillyTavern 那一节：

1. **只从 SillyTavern 官方 GitHub 取。** 最新版读 `github.com/SillyTavern/SillyTavern/releases/latest` 的 302 跳转
   （**不走 `api.github.com`**，同「应用自己的更新」的理由）；那个标签的 `package.json` 从 `raw.githubusercontent.com` 读
   （看它要多新的 Node）；源码包是 `archive/refs/tags/<tag>.zip`。只收 https，跳转只许落在
   `github.com` / `codeload.github.com` / `raw.githubusercontent.com`（`allowed_url`），源码包有大小上限。
2. **不打包、不镜像、不改包**：安装包里没有它的一个文件；解出来的原样放着（`Start.bat` 保持官方原样）。
   ⛔ **官方不公布源码包的哈希**：算出来的 SHA-256 **只记日志与标记**，不作判据（同反重力直下那条）；别编一个「预期哈希」常量。
3. **解压用 `managed::unzip`（.NET 的 `ZipFile`，它拒绝解到目标目录外面）**，不为此加 Rust 依赖；顶层必须正好一个目录、
   而且 `looks_like_sillytavern`（有 `server.js`、`package.json` 的 name 含 sillytavern），不然不装。
4. ⛔ **更新与卸载只碰带标记的那份**（`sillytavern::is_managed`）。使用者自己的酒馆（git 克隆的、解压的）面板只起、只停、只读；
   目标目录里有东西又没有标记就不往里装（`check_target`）。
5. **导入、迁移只复制，不搬**（`import_plan` + `copy_plan`）：`data\`、`config.yaml`、`plugins\` 与全局扩展里使用者自己加的
   （上游占位 `.gitkeep` / `plugins\package.json` 不算）；**绝不整份拷 `public\`、不拷 `node_modules`**（照它自己的 `Update-Instructions.txt`）；
   跳过符号链接与目录联结。导入的源比要装的新版还新就不导（`check_import_version`）。
6. **更新只留一份旧版**：删掉更早的 `.previous` → 现役改名 `.previous` → 暂存改名现役（`swap_in`），第二步失败把现役放回去；
   两边都放不回去时两份都留着、如实说在哪。卸载**先复制备份到 `state_dir\tavern-uninstall\<时间>`、核过备份再删**。
7. **Node.js 只在使用者勾了之后才用 winget 装**（`OpenJS.NodeJS.LTS`，参数照反重力 A 路）；够不够按它 `package.json` 的
   `engines.node` 判（`node_satisfies` 只认 `>=` / `^` / `N.x`，认不出就不下判断）。确认框里 Node 不够又没勾就不让点。
8. ⛔ **npm 不经 shell**：`node <node 目录>\node_modules\npm\bin\npm-cli.js install --prefix <暂存> …`（参数照它自带 `Start.bat` 那一行），
   PATH 前置 node 所在目录 —— 刚用 winget 装上的 Node 不在面板进程那份旧 PATH 里。起酒馆时同样把 node 目录前置到 PATH（`node_path_env`）。
   只有 node 旁边找不到 npm-cli.js（Scoop 之类的垫片）才退回 `cmd /c npm`（见「别把 shell 当启动目标」）。
9. **装 / 更新 / 卸载 / 导入之前先停面板起的那份酒馆（连同三条桥）**，命令层拿 `operations::exclusive()`，停酒馆的
   `stop_tavern_all` **不拿锁**；酒馆端口上有一份不是面板起的在跑（`foreign_tavern_running`）就不更新、不卸载、不导入 ——
   面板不替使用者关别人起的进程。代价（下载、几分钟 npm、会先停酒馆、卸载前备份到哪）常驻在确认框里。

⚠ **换托管根不搬它**（`managed::migrate_files` 只搬 `App::ALL`）；酒馆配置记的是绝对路径，老位置照旧能用。
⚠ **联网只在点了之后**：打开确认框、点「检查新版本」才问 GitHub，没有定时器。
GitHub 改了跳转或源码包布局、界面开始报「认不出」时先跑（会联网，只读，不跑 npm、不写配置）：

```powershell
cargo run -p qb-install --example sillytavern-release -- latest
```

```powershell
cargo run -p qb-install --example sillytavern-release -- download --to <一个空的短路径目录>
```

### 酒馆卡的控制台、角色卡 / 世界书 / 扩展、回收站、指定版本与换回（2026-09-29 第二轮，使用者选的）

使用者：「这次我们来优化他，前端请你进行完全重构，并且搜索 GitHub 上的开源项目，优化他的功能」；位置不变（「直接做这个基础上改」），
问他时选了三组：实时控制台日志、角色卡 / 世界书 / 扩展管理、版本回退与指定版本。对照过的项目与许可在 ATTRIBUTION（MIT 的只借思路，
AGPL 的只看思路，SillyTavern 源码只核事实）。代码：`plugins::tavern_console`、`plugins::tavern_library`、`sillytavern_setup::{releases, release,
check_switch, swap_with_previous}`、`qb-platform::sessions::OwnedProgram::launch_logged`、`qb-platform::process::decode_console_bytes`。

**控制台**（动了同步改 DISCLAIMER 的本机数据那一节）：

1. **只收面板自己起的那份酒馆**，输出**追加**进 `state_dir\plugins\sillytavern-console.log`（stdin 接 `NUL`）。⛔ 继承只放这两个句柄
   （`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`）—— `bInheritHandles` 一开、不收窄，面板里任何可继承的句柄都跟进子进程。
2. **每次起新的一份就轮换**（`.prev.log` 只留一份）；「这是哪一次」认旁边 `run` 文件里的随机 id，⛔ **不拿文件的创建时间**
   （Windows 的文件隧道会让 15 秒内删了又建的同名文件继承旧的创建时间）。
3. ⛔ **bridge.py 的输出不收**：它是使用者自己的脚本，打印什么面板说不准，收进来可能就是把它的密钥落了盘（「鉴权只在真要发请求那一刻读」那条）。
4. **报错提示只认真的会打印的句子**（`tavern_console::RULES`，照 SillyTavern 1.19.0 源码与 npm / cmd 的实际输出写），认不出不猜。
   「Warning: failed to start server on IPv6」**不算错**：面板默认关本机 IPv6，那一句每次都有。
5. 界面按偏移量读本机文件，不联网；分页不在前台不读。报错文案里指的是「软件页 SillyTavern 卡的「控制台」」，别再写「它自己的窗口」—— 那个窗口不存在。

**角色卡 / 世界书 / 扩展**：

1. ⛔ **删除 = 挪进面板的回收站**（`state_dir\tavern-trash`，`tavern_library::Trash`），能原样放回（原处有同名的不覆盖）；只有「清空」真删。
   别照 al01cn 那样直接删 —— 角色卡连着几百段聊天。
2. **只认酒馆自己的格式、导入不覆盖**：卡 = 带 `ccv3` / `chara` 文本块的 PNG，世界书 = 顶层有 `entries` 的 JSON；同名加 ` (2)`。
   别家格式说清楚为什么没进来、请在酒馆里导入 —— 面板不替它转格式。**不改卡与世界书的内容**（`tavern_assets` 那条没变）。
3. ⛔ **扩展的 git 操作不拿 `operations::exclusive()`**：克隆可能要几分钟，看门狗每 5 秒要那把锁巡检。它们之间用 `tavern_library::git_lock` 排队。
   只在点了之后跑、不经 shell、`GIT_TERMINAL_PROMPT=0`；更新只 `pull --ff-only`（本地改过就不动）；远端地址里的账号密码抹掉再显示（`redact_url`）；
   从地址安装只收 https、克隆下来没有 `manifest.json` 就删掉那份刚克隆的。
4. **不写酒馆的 `settings.json`**：扩展「停用」只读出来显示。
5. 导入、挪进回收站对「你自己的那份」酒馆也能做（数据是使用者的、动作是他点的、删除先进回收站）；**程序文件**的更新、换版本、卸载仍只对面板装的那份（上面第 4 条）。

**指定版本与换回**：

1. **版本清单读发布页的 Atom**（`RELEASES_FEED`，`github.com` 本站、`allowed_url` 放行），⛔ 不走 `api.github.com`；HTML 更新说明转纯文本，不保留任何标签。
2. `check_switch`：没指定版本只许往新了换；**指定了可以往旧了换（降级）**，同一版不换。降级的代价（新版可能改过数据格式）常驻在确认框里。
3. **「换回」是整份目录对调**（`swap_with_previous`），不下载、不跑 npm；两种数据选法都不删任何一份数据（带现在的数据过去时，
   留着那份自己的数据先备份到 `state_dir\tavern-rollback\<时间>`）。任何一步改名失败都尽力放回原样，放不回去就两份都留着、照实说在哪。

GitHub 的发布页 Atom 改了形状、界面报「一个版本都认不出来」时先跑（只读、联网一次）：

```powershell
cargo run -p qb-install --example sillytavern-release -- releases
```

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

## ⛔ 一键汉化：只剩 GPT（写它自己的设置项）；Claude 汉化撤掉了（2026-09-27，使用者定的）

2026-09-25 使用者要了两件：「gpt 虽然有中文，但默认不是，加一个一键汉化按钮自动设置中文」「claude 没有中文，以插件的
形式装载 javaht/claude-desktop-zh-cn……还有一键汉化按钮」，并要「确保以上两个改动符合两个厂商的使用政策」。
09-27 使用者报「claude 和 GPT 的汉化功能没有正常生效」，查下来是两回事（证据链在 DESIGN-NOTES「0.25.6（2026-09-27）」）：
**Claude 在官方账户下必然不生效**，问他时选了**撤掉**；GPT 的做法没错，是面板从来没真正写进去过（审计日志与操作记录里
一次「GPT 界面设为中文」都没有），而 09-26 那版修法写出来的文件 GPT 仍然读不了 —— 修好了。

### GPT（`usecase::codex_locale`）

1. **只写 `CODEX_HOME\config.toml` 里 `[desktop]` 表的 `localeOverride`** —— 跟在 GPT「设置 → General →
   Language」里选中文写下的是同一行（toml_edit 增量改；`auth.json` 零接触）。
   ⛔ 不改 ChatGPT.exe / app.asar、不注入翻译 —— OpenAI 使用条款不许 modify 它的服务。
2. ⛔ **不碰 `enable_i18n`。** OpenAI 用这个远端开关按账户 / 机器放中文界面（openai/codex#19239；26.924 的渲染层
   默认按「关」处理）：没放开时设了照样是英文。界面如实说，不提供任何绕过（条款：不得 circumvent restrictions）。
3. ⛔ **GPT 读 `config.toml` 用的是严格解析器：同一张表重复定义一次，整份桌面设置它一项都读不到**（26.924 的
   `app.asar`：「Failed to load desktop settings from config.toml」→ 当成空表）。GPT 自己会写出这种文件（09-26 本机
   slot 3：重复的 `[desktop]` 与 `[mcp_servers.node_repl.env]`，CRLF）。所以「设为中文」先过 `repair_duplicate_tables`
   （使用者 09-27 选的「修好再写」）—— 这是「`config.toml` 别的一项都不动」的**唯一例外**：
   - 只修「同一个表头出现多次」：让 toml_edit 指出重复的是哪一行（认表头的是解析器，多行数组里的 `[` 骗不到它），
     后面那段的键并进第一段，同键同值丢掉，**同键不同值就不修**；
   - 修完用严格解析器再读一遍，「并之前每个值在哪」对「并之后每个值在哪」逐项比，一项对不上就不修；
   - 纯 CRLF 的文件修完还是 CRLF；「恢复默认」时一份写坏了、里面又没有面板设的中文的文件**不去修**（不是面板弄坏的）；
   - ⛔ 别退回 09-26 那种「只合并 `[desktop]`」的文本补丁：别的表还重复着，写出来 GPT 照样读不了，面板却报成功（坑 7.101）。
4. **成功还是失败按数说**（`CodexLocaleOutcome` 的 `changed` / `already` / `repaired` / `skipped` / `default_busy`，
   前端 `gptZhResult`），⛔ 别再对中文日志行跑正则。写完每份用严格解析器回读，值不对就算「没改成」。
5. GPT 只在启动时读这一项（运行中改它不重读；在它自己的设置里选中文才是立刻生效 —— 界面上把这条路告诉使用者）。
   所以**先空跑一遍算出哪几份真要写**，一份都不用写就谁都不关；要写的那几个槽位里面板起的那份先关
   （`close_official`，确认框写明任务会停）→ 写 → 没有外部窗口时命令层按当前槽位重开（那里挂看门狗）。
   别处起的那份不关（2026-09-23 那条），它开着时默认那一份这次不改、如实说，也不自动重开面板槽位（免得旁边凭空多一个
   窗口）；出站插件在跑时拒绝（它整份接管 `config.toml`）。**中转环境不碰**（受环境哈希追踪，改了会被判「外部修改」）。
6. 「恢复默认」**只删值为 `zh-CN` 的那一行** —— 使用者在 GPT 里自己选的别的语言不是我们的残留。
7. `settings.gpt_ui_zh` 记的是使用者的选择（新槽位按它预写，`codex_accounts::create` → `seed`），**只经
   `codex_locale_set` 改**：`settings_save` 原样保留旧值。

诊断（只读：不写任何文件、不关不起 GPT、不联网，配置里别的内容一个字都不打印）：

```powershell
cargo run -p qb-app --example codex-locale -- check
```

逐份说 GPT 读不读得了、能不能修、设的是什么、GPT 上次运行时用的是什么（`computer-use\config.json` 的 `locale`；
26.924 里它是「`localeOverride` 有就用它，没有就用系统界面语言」）。设的是 zh-CN、上次运行也是 zh-CN，界面仍是英文
= OpenAI 没放开 `enable_i18n`，面板管不了。

### ⛔ Claude 汉化撤掉了，别加回来（`plugins::claude_zh` + `usecase::claude_zh_ops` 只剩清理残留）

0.25.4–0.25.5 接过 `javaht/claude-desktop-zh-cn`（MIT）Windows 脚本的安全模式。09-25 本机跑了两次、两次「核验通过」，
界面照旧是英文，使用者看二十来秒就点了「恢复英文」。根因（2.9939.2 的 `app.asar` 与 `ion-dist` 里核过）：

- **官方账户（claude.ai 登录）下，Claude 主窗口是在线的 `https://claude.ai`**：官方模式的 `getMainWindowUrl()` 返回它，
  只有第三方 API 模式才是本地的 `app://localhost`（上游安全模式改的就是那份 `ion-dist`）；
- 在线页每次加载都调 `window.electronIntl.requestLocaleChange(它自己的语言)`，claude.ai 没有中文 → 推回 en-US →
  主进程 `qit()` 切回英文，**还把 `config.json` 的 `locale` 改写掉** —— 连菜单都保不住；
- 能锁住语言的只有上游另外两种模式（改 `app.asar` 并重写 `Claude.exe` 的完整性哈希 / Frida 内存补丁），
  都是绕过 Claude 的防篡改（Anthropic 消费者条款 §3「bypassing … protective measures」），面板永远不做。
  ⛔ 反重力那套 CDP 注入同样不许搬过来：Claude 见到 `--remote-debugging-port` 就拒绝启动，绕过那道检查就是上面那一类。

问使用者时给了「保留、改成如实说明」与「撤掉」两条，**他选了撤掉**：账户页入口、扩展中心条目、下载 / 查新版 / 应用全删，
只留一次性的**「清理汉化残留」**（Claude 启动卡标题栏，只在有残留时出现，清完就没了）。五条要守住的：

1. ⛔ **清理不跑上游的卸载脚本。** 上游 `uninstall` 第三步「移除 zh-CN 语言注册」按字符串删 `"zh-CN"`，把 Claude
   **原版**文件里本来就有的那几处也删了：本机 09-25 之后 3 个前端文件跟上游备份对不上，而备份跟旧版 2.9939.1 里同名
   （文件名带内容哈希）的文件逐字一致 —— 备份才是原版。清理直接用上游自己留下的备份（`resources\.zh-cn-backups`
   里最早那一套，上游卸载的第一步用的也是它）逐字换回去，再删中文文件与备份（`claude_zh::clean_app`）。
   不起 PowerShell、不联网、不重启 Claude。
2. **只写备份里列着的前端文件（`ion-dist\` 下，`restorable`）**；备份里有 `app.asar` / `claude.exe` 之类（用过上游改程序文件
   的模式），或者根本没有备份，就不自动还原，如实让使用者到官网重装。任何一步对不上就停在那一步，**备份留着**（它是唯一的原版）。
3. **认残留要有证据**（`counts_as_leftover`）：有上游的备份目录，或者面板记过「汉化了」；只有中文文件、两样都没有的不算 ——
   以后 Claude 自带中文也长那样。`Claude-3p` 只在里面只有一个只写了 `locale` 的 `config.json`（上游建的，`is_upstream_3p`）
   时才删，删之前再核一次形状，只删那一个文件、再删**空**目录。
4. **要往 Claude 现在用的那一版里写才关 Claude**（`needs_close`，确认框写明）；只删备份、写回资料语言这类不关 ——
   使用者多半正在 Claude 的 Code 页里干活。关不干净就停（`close_desktop_for_relay`）。
5. 资料的界面语言写回汉化之前的值（`claude-zh\state.json` 的 `previous_locales`，按真实目录记），没记就 en-US；
   `set_top_level_string` **只改那一个值的字节**（`config.json` 里有 `oauth:tokenCache`）。有一份没写回，那份记录就先留着。

诊断（只读：一样都不删、不关 Claude、不联网）：

```powershell
cargo run -p qb-app --example claude-zh -- leftovers
```

⚠ **调试时别在真机上点「清理汉化残留」**：要换回原版文件时它会关掉 Claude 桌面端，Code 页里跑着的会话也在里面 ——
你正在那里面干活的话就是把自己关了（同「启动 桌面端」那条）。

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

## ⛔ Key 不出后端 —— 只管经本机路由的那几个；共存型工具和桌面端直连档写的是真 Key

「真 Key 只在后端、客户端配置里只有占位串」这条规矩**只对经本机路由的配置成立**：
Claude Code、Codex、Gemini CLI、Grok Build，以及**没开直连档时**的 Claude 桌面端。它们的配置里永远是
`local_router::ROUTER_KEY`，真 Key 由路由在转发时换上（上一节）。

**例外只有两个，都是使用者明说过的：**

1. **桌面端直连档**（2026-10-26 使用者定的，照 cc-switch 的 C5）：使用者在桌面端那一页选一条线直连之后，
   `workspace::configuration_files` 把桌面端那份 `claude_desktop_config.json` 的网关地址换成那个站点、
   `inferenceGatewayApiKey` 换成**那条线的真 Key**（`desktop_direct::resolve`）。守住的边界：
   - **只在使用者选了、再确认一次时开**（`StationDesktopDirect`，确认那一行写着真 Key 会写进桌面端配置）；关掉就换回占位串；
   - **只收不需要本机路由的线**（`desktop_direct::check`）：没设模型映射、上游是 Anthropic、绑着属于这个站点的 Key ——
     直连时没人换模型名、没人转协议；开着聚合时不许开直连、直连时不许开聚合（`desktop_direct_ops::guard_aggregate`）；
   - 选的线不在了、改得不合格了，**报错、不悄悄退回本机路由**（`resolve`）—— 使用者以为在直连、实际走的却是另一条线，
     那才说不清；存那条线之前就拦（`guard_route_save`），删那条线之前先关直连（`before_route_removed`）；
   - 线路的 Key 换了，存完立刻重写桌面端配置跟上；
   - **界面上不显示真 Key**：配置面板读到的那份把 `inferenceGatewayApiKey` 换成记号（`mask_desktop_key`），存的时候
     `apply` 按库里的设定写回真 Key；审计日志只记线路，不记 Key。
2. **共存型工具**（2026-10-25 使用者定的，照 cc-switch 的 C3 / C4）：OpenCode、MiniMax Code、OpenClaw、
   Hermes、Pi 不经本机路由，`usecase::coexist_ops::write` 把站点地址和这条线绑着的**真 Key 明文**写进它们自己的配置
   （`relay::coexist` 定位置和形状）。守住的边界：
   - **只在使用者点「写入」并在面板里再确认一次时写**（确认那一行写着要改哪个文件，`StationCoexistPanel`）；
     不随保存线路、换 Key、启动面板自动写或自动更新 —— 换了 Key 要回来再点「更新」，界面写着；
   - **只动 `qb-gate-` 开头的那一条**（`coexist::ID_PREFIX`），使用者自己配的供应商、别的字段一个不碰；
     JSONC / JSON5 走 `live::jsonc_document` 保留注释和格式，YAML 只换那一节；原文件解析不了就**停**，不覆盖；
   - 工具的配置目录不在（多半没装）就**拒绝**，不替它建；
   - 写盘走 `config_io::commit`（写前核对没被外部改过、日志里留原内容、失败回滚），审计日志只记工具和线路，不记 Key；

- **新加的任何「往别的软件配置里写东西」的功能都默认占位 Key + 本机路由。** 要再开一个写真 Key 的口子，
  得像这次一样由使用者明说，并在同一个提交里改这一节和 DISCLAIMER §5.1。

钉着这件事的是 `relay::coexist` 的单测（`only_qb_gate_entries_are_touched_and_broken_files_are_refused`、
`opencode_keeps_comments_and_the_user_s_own_providers` 等）、`coexist_ops` 的
`writes_only_where_the_tool_is_installed_and_removes_only_its_own_entry`（目录不在就拒绝、只删自己那一条，
在临时目录里跑，不碰真用户目录）和 `StationCoexistPanel.test.tsx`（不确认不写）。

**C5 里碰到官方那一侧的三件，规矩一样：默认只动中转环境，官方的要使用者点名，撤销只撤面板自己做的。**

- 跳过首次确认（`usecase::claude_code_ops`）：官方默认目录、官方槽位每一行单独点；撤销看 `metadata` 里的
  `claude-onboarding-skip:<路径>` —— 不是面板设的 `true`（使用者真走完过一次）一律不撤；
- VS Code 插件联动：只写 `CLAUDE_CONFIG_DIR` 和占位 `ANTHROPIC_AUTH_TOKEN` 两条（**不写真 Key**）；
  使用者设过同名变量、值不一样就拒绝；`disableLoginPrompt` 联动前的值记在 `vscode-claude-link:<编辑器>`，撤销时还原；
- Codex 会话归桶（`usecase::codex_history_ops`）：**不在启动时自动跑**（cc-switch 是自动的）；每个桶使用者自己勾、再确认；
  改之前先备份，还原**按账本逐处改回**，不拿旧文件盖 —— 并桶之后 Codex 往同一个会话里接着写的内容不能因为还原而丢。
- 桌面端直连档见本节开头的例外 1（2026-10-26 补上；原来写着「没做、等使用者定」）。

钉着这些的是 `claude_code_ops` 的 `onboarding_skip_only_undoes_what_the_panel_set`、
`vscode_link_needs_the_editor_and_unlink_restores_the_user_s_settings`，`relay::vscode` 的三个，
`codex_history_ops` 的 `merging_moves_only_the_ticked_buckets_and_restore_keeps_later_writes`，
`station_ops` 的 `claude_code_routes_copy_to_the_desktop_once_and_only_with_a_key`，和 `StationCodexHistory.test.tsx`。
直连档的是 `desktop_direct` 的 `only_a_line_that_needs_no_router_can_go_direct`、
`desktop_direct_ops` 的 `switching_writes_the_setting_and_rolls_back_when_the_rewrite_fails`、
`workspace` 的 `the_desktop_config_carries_the_real_key_only_in_direct_mode`、
`commands::station` 的 `the_desktop_config_view_never_shows_a_real_key` 和 `StationDesktopDirect.test.tsx`（不确认不切）。

## MCP 面板：一条服务器、按目标写（2026-10-26，中转站重构阶段 5 · M1，照 cc-switch 的统一 MCP）

扩展中心的「MCP」分页取代了原来目录里的 MCP 那一半（目录不再列 MCP 卡片，「导入 MCP 进目录」那条命令撤了）。
转换在 `qb_extensions::mcp`（纯值：统一格式 ↔ 各家写法），写到哪、怎么写盘在 `qb_app::usecase::mcp_ops`。

- **开关按目标，不按客户端**：cc-switch 一个客户端一个 `enabled_<app>`；QB Gate 的 Claude Code / Codex / Gemini CLI 都有中转环境、
  官方默认目录、官方槽位好几份配置。目标 id 形如 `claude-code:relay`、`claude-code:slot:<标签>`、`codex:default`、`opencode`，
  一条服务器记着自己勾了哪些（`McpServer.targets`）。新建的默认只勾 `claude-code:relay`；**官方那一侧（`official`）要使用者自己勾**。
- ⛔ **只增删库里登记的那几条**：取消勾选、删除时只删这条服务器自己的键；从一个目标「导入」只读不写，同名但连接方式不一样的跳过、报出来，
  不替使用者挑哪份对。
- ⛔ 工具或目录不在（没装）就拒绝，不替它建；配置解析不了、MCP 那一节不是对象 / 表就停，不覆盖。
- **各家写法的几个坑照 cc-switch**（`qb_extensions::mcp` 模块头那张表）：Codex / Grok **只写所选传输方式的字段**
  （Codex 见到另一种的字段会拒绝整份 `config.toml`，别的 MCP 一起失效）、不写 `type`；Claude Code 在 Windows 上把 `npx` 一类包成 `cmd /c`；
  Gemini 的 http 用 `httpUrl`、超时合成一个毫秒数；Hermes / MiniMax / Pi 自己的字段写的时候从原条目带过来；Pi 不收 SSE、只差 `-` / `_` 的名字算同一个。
- **连接定义库里加密存**（`secret::seal`，跟原来扩展中心导入的 MCP 一样）：`env`、`headers` 里常有 Key。界面拿得到明文（使用者自己编辑要用）。
- 写的是中转环境里有配置清单的那几份（Codex 的 `config.toml`、Gemini CLI 的 `.gemini/settings.json`、Grok Build 的 `config.toml`）时，
  写完按那个软件的中转环境重新应用一次（`workspace::apply`），指纹对上 —— 不然下次启动会报「配置被外部改过」。
- **原来扩展中心装的 MCP 第一次打开时迁进来**（`migrate_installations`，只迁一次，`metadata` 记 `mcp-migrated-installations`）：
  名字照旧（安装时用的扩展 id），连接定义从那份配置里读回来，目标按装在哪推（中转环境 / 官方默认 / 槽位）；`installations` 老表留着能读。

钉着这些的是 `qb_extensions::mcp` 的七个（各家写法、Codex 只写一种传输、内联表也删得掉、不是表就停）、
`mcp_ops` 的 `a_server_lands_only_in_the_ticked_targets_and_leaves_the_user_s_own_alone`（库里不见明文 Key、没装不建目录）、
`importing_links_identical_servers_and_reports_conflicts`（OpenCode 的注释留着）、`old_extension_center_installs_are_read_into_the_new_records`，
和 `McpPanel.test.tsx`（官方目录要自己勾、没装的不列）。

## Skills 面板：装一次、按目标复制（2026-10-26，中转站重构阶段 5 · M2，照 cc-switch 的 Skills 管理）

扩展中心的「Skills」分页取代了原来目录里的 Skill 那一半（目录不再列 Skill 卡片，「手动导入」连同 `extension_import_skills` 撤了）。
来源在 `qb_extensions::skills`（GitHub / 本地目录 / ZIP / skills.sh 搜索），装到哪、怎么同步在 `qb_app::usecase::skill_ops`；
三个面板共用的根目录与目标在 `usecase::ext_place`。

- **本体只放一份**（面板数据目录的 `skills\<名字>`，cc-switch 的 SSOT），目标里是**复制**过去的（cc-switch 默认先试符号链接；
  Windows 上建链接要开发者模式或管理员，复制最稳）。目标同 MCP（`ext_place::Place::client_slots` + 各工具的 `skills` 目录）。
- ⛔ **不覆盖不是面板装的同名 Skill**：目标里已经有同名目录、内容跟面板那一份对不上、又不是面板之前放的 —— 拒绝，界面让使用者「导入」接管。
- ⛔ **删除先进备份**（`skill-backups\`）：取消勾选、卸载、更新替换下来的那一份都挪进去，不直接删；卸载的那一份带着记录、能还原
  （还原后目标一个都不勾）。写目标时先写到旁边的临时目录再换上；不顺着联结点 / 符号链接写。
- ⛔ **只读元数据、只下文件，不执行仓库里的任何脚本**（扩展中心原来那条照旧）：GitHub 走 API 拿目录树、raw 拿文件，
  512 个 / 32 MB / 单个 8 MB；符号链接、子模块、Windows 上撞名的不收。ZIP 逐个读条目（`enclosed_name`，不用 `extract`），
  `zip` 至少 9.0（9.0 以下有解压越界的公告）。
- **「有没有更新」只比 Skill 自己那个目录**（目录下各文件的 git blob SHA 合成指纹），不比整个仓库的提交。
- 联网只在点的时候：扫描 GitHub 来源、检查更新、搜 skills.sh。
- **原来扩展中心装的 Skill 第一次打开时迁进来**（只迁一次，`metadata` 记 `skills-migrated-installations`）：从装着的目录复制一份当本体，
  目标按装在哪推；`installations` 老表留着。Codex 也认 `skills` 目录了（原来扩展中心说「Codex 不读 Skills 目录」，那是旧版本的事）。
- 联网的 async 命令**不跨 `.await` 拿着库连接**（`Repository` 不是 `Sync`，Tauri 要能跨线程的 future）：先联网、后开库。

钉着这些的是 `qb_extensions::skills` 的五个（本地来源改过要重扫、ZIP 逐个读、名字规则）、
`skill_ops` 的 `a_skill_is_copied_to_the_ticked_targets_and_removals_go_to_backups`（不覆盖别人的、没装不建、卸载能还原）、
`importing_and_updating_follow_the_panel_copy`、`old_extension_center_skill_installs_are_read_into_the_new_records`，
和 `SkillsPanel.test.tsx`（扫描之后才装、默认只放中转环境、没管着的能导入）。

## 提示词库：每个目标挑一条（2026-10-26，中转站重构阶段 5 · M3，照 cc-switch 的 Prompts）

扩展中心的「提示词」分页（`usecase::prompt_ops`）。一库提示词，每个目标挑一条，整份写进它的提示词文件：
Claude Code 的 `CLAUDE.md`（在那个 `CLAUDE_CONFIG_DIR` 里）、Codex / Grok Build / OpenCode / OpenClaw / Pi / MiniMax Code 的 `AGENTS.md`、
Gemini CLI 的 `GEMINI.md`、Hermes 的 `SOUL.md`。**写进哪一份由使用者选**（计划里那条）：目标同 MCP / Skills，各挑各的，
记在 `metadata` 的 `prompt-active:<目标>`。

- ⛔ **写之前先把文件现在的内容存回库里**（cc-switch 的「启用前回填」）：这个目标有用着的那一条 —— 文件被手动改过就回填进那一条；
  没有 —— 文件里有内容、库里没有一模一样的，就存成一条「原始提示词」。换来换去，使用者原来写的东西不丢。
- 改一条正在用的，用着它的目标跟着重写；那份文件在这期间被手动改过的，改动先另存一条「手动改过的」再覆盖。
- 正在用的不能删；没有「停用」—— 要回到原来那样，就换回那条「原始提示词」（cc-switch 也是这样）。
- MiniMax Code 的全局指令最多 32 KiB（cc-switch 的 `validate_prompt_content`）。

钉着这些的是 `prompt_ops` 的 `switching_prompts_never_loses_what_the_file_had`、
`importing_reads_the_file_without_touching_it_and_minimax_has_a_limit` 和 `PromptsPanel.test.tsx`（正在用的删不了、没装的不列）。

## 项目：只换中转那一侧（2026-10-26，中转站重构阶段 5 · M4，照 cc-switch 的 Profile）

中转站页软件分段器那一行的「项目」按钮（`StationProjects.tsx`，照 cc-switch 的项目切换器 + 管理对话框，并成一个弹窗），
后端 `usecase::project_ops`，六条命令在 `commands/station.rs`（`projects_list / create / rename / delete / apply / clear`）。
一个项目按软件分组（`claude-code` / `claude-desktop` / `codex` / `gemini-cli` / `grok-build`）存快照：
这个软件选的线路、派单顺序、中转环境里勾着的 MCP / Skills、中转环境用的提示词（桌面端只有前两样）。`None` = 这一项没拍过（换过去时不动），
跟「拍到的就是空的」分开（cc-switch 同样）。项目列表各软件共用，「当前项目」每个软件各记各的（`metadata` 的 `project-current:<软件>`）。
换项目时先把当前项目的这一组按现在的样子存一下，再落目标项目那一组；尽力而为，单项失败记成提醒（弹窗里列出来、写审计日志）、不整体回滚。
快照只由「换走时自动存」维护，不另给「重拍」（cc-switch 同样）。

- ⛔ **只碰中转那一侧，永远不碰官方账户 / 槽位的切换**：MCP / Skills / 提示词只动 `<软件>:relay` 那个目标（Grok Build 是它唯一那份），
  官方目录一概不碰。`architecture.rs` 的 `project_module_can_never_reach_account_switching` 钉着：换项目会跑到的五个模块
  （`project_ops`、`mcp_ops`、`skill_ops`、`prompt_ops`、`ext_place`）一个都不许提账户切换的入口（`account_ops`、`codex_accounts`、
  `accounts::switch`、`killswitch`……）。
- **线路和派单顺序走手动操作原来那两条路**（`select_route_in` / `save_queue_in`，手动点一行、手排顺序也用它们，不另写一份）：
  线路排队、下一发生效，调度开着时跟手动选一样钉住 4 小时；项目里存的线路已经删了 —— 不换、记提醒，不替使用者另挑一条。
  派单顺序只收这个软件还在的线；**调度开着时不落**（顺序归调度排，记提醒）。
- **「不使用项目」先把当前项目这一组存一下再清**（cc-switch 只清指针：在项目里改过的设置一清就丢）；什么配置都不动。
- 删项目只删记录，不动任何配置；删之前再确认一次。反重力不经本机路由，没有项目（建、换、清都拒绝）。

钉着的是 `project_ops` 的 `switching_projects_saves_the_old_one_and_lands_the_new_one_on_the_relay_side_only`、
`not_using_a_project_saves_it_first_and_touches_no_configuration`、上面那条架构测试，和 `StationProjects.test.tsx`
（只说中转那一侧、没落成的列出来、删除先确认、新建只拍这个软件）。

## 导入链接：先确认，什么都不自动启用（2026-10-26，中转站重构阶段 5 · X4，照 cc-switch 的 Deep Link）

格式照搬 cc-switch 的 v1（`<协议>://v1/import?resource=provider|mcp|prompt|skill&…`）。解析在 `qb_extensions::deeplink`
（只解析、只核对），编排在 `usecase::deeplink_ops`，Tauri 那一层在 `commands/import_links.rs`（模块名不叫 `deeplink`：
workspace 里模块名要全局唯一），全局只挂一份的确认框是 `DeepLinkImport.tsx`，中转站页头部有「从链接导入」。

- **系统里只注册 `qbgate://`，而且是安装包注册的**：`tauri.conf.json` 的 `plugins.deep-link.desktop.schemes` 是给打包用的配置，
  Tauri CLI 内嵌的 NSIS 模板按它写 `HKCU\Software\Classes\qbgate`，卸载时只删指向这份安装的那一条（2026-10-26 在
  `@tauri-apps/cli` 2.11.4 的二进制里核过模板原文）。⛔ **运行时不注册、不引 `tauri-plugin-deep-link`**：Windows 上它多出来的
  只有运行时写注册表（开发版一跑就把装好的面板的协议抢过去，cc-switch 开发版正是这么做的）和解析启动参数（冷启动看
  `std::env::args()`，面板开着时单实例插件的回调本来就拿得到参数）。`ccswitch://` 是 cc-switch 的，**只认粘贴进来的**。
- **链接放在后端、只在内存里、只留最新一条**（`AppState::deeplinks`）：冷启动时它比界面先到，事件
  （`workspace://deeplink`）只是「去看一眼」；读不清除，显示过 / 点了「不导入」按编号清。⛔ 不落盘 —— 站点链接带着 Key
  （迁移 9 建的 `deeplink_pending` 表因此没用上，留着不删：迁移只许追加）。审计日志只记「收到一条」，不记链接本身。
- ⛔ **回给界面的形状里没有明文 Key**（`ProviderLinkView.key_masked` 只有打码的）：导入由后端拿着 Key 做。链接没带 Key 时
  使用者在确认框里填，跟线路弹窗一样只从界面进后端、不回界面。
- ⛔ **导入前一律确认，导入后什么都不自动启用**（计划里那条）：站点 → 一个站点（同一个主机名和端口已有就接着用、不改它）、
  一把加密存的 Key、一条线路，**不切过去**；同一软件、站点、分组已经有线路就**在动任何东西之前**拒绝；其余地址进备用地址。
  MCP → 列表，**一个目标都不勾**、不写任何客户端配置，同名已有的跳过；提示词 → 库，不启用；Skill **不从链接装**，
  确认框只是带着来源（`https://github.com/<仓库>/tree/<分支>/<目录>`）去 Skills 分页，扫描、挑、装都由使用者来。
  链接里的 `enabled=true`、用量脚本（`usage*`，阶段 6 才有）认出来但不照做，确认框列成「没照做的」。
- **链接是不可信输入**（照 cc-switch）：地址只收 http / https；远程配置（`configUrl`）拒绝；图标（`icon` / `iconUrl`）丢掉、
  不替链接下载；Codex / Grok Build 配置里让客户端执行命令或读本机凭据的键（`auth`、`aws`、`env_http_headers`、`mtls_cert_dir`……）
  拒绝；只用 `env_key` 间接给 Key、链接自己又没给的拒绝（导进来等于把本机环境变量里的 Key 发到链接给的地址）；
  给 OpenCode / OpenClaw / Hermes / Pi 的站点链接拒绝并说清楚怎么做（QB Gate 里它们的线路从「写进其他工具」来）。
- **模型和协议照预设那一套落到线路上**：主模型进模型映射的「兜底」、各档进各档、`[1M]` 剥掉。**上游协议跟预设不一样 ——
  链接写了就填**（只在跟这个软件的原生协议不同时填）：预设不填协议，是因为它知道的只是「官方端点长什么样」；
  链接是这个站点自己说的，不填的话「只说 OpenAI Chat 的站」配给 Claude Code 一发都通不了。
- 导入成了线路之后跟手动存线路一样重装上游、更新聚合模型（`load_upstreams` / `sync_aggregate_config`），界面跳到中转站页那个软件的分页
  （分页记在会话态 `station.client`）。

钉着的是 `qb_extensions::deeplink` 的六个（两种协议同一个格式、不可信的部分拒绝或照实报、协议要配得上软件、配置只补链接没写的、
Codex / Grok 拒本机凭据、`+` 被解成空格照样解得出）、`deeplink_ops` 的三个（界面那一份里没有明文 Key、撞了在动任何东西之前停、
MCP 一个目标都不勾、提示词不启用、只留最新一条按编号清）和 `DeepLinkImport.test.tsx`（确认之后才导入、填的 Key 和分组带过去、
「不导入」按编号清、粘贴的链接走同一个确认框、同名的 MCP 不再给导入按钮）。⚠ 真机一项都没走：装上新版之后点一条 `qbgate://` 链接
（面板开着、没开着各一次），看注册表里那一条指向安装目录。

## 工具文件：OpenClaw / Hermes / Pi 自己的说明与记忆（2026-10-26，中转站重构阶段 5，C3 / C4 的附带件，照 cc-switch）

扩展中心「工具文件」分页（`ToolFilesPanel.tsx`），后端 `usecase::tool_files_ops`。只是这几个工具自己目录里 Markdown 的编辑器，
不碰中转、不碰官方账户（工具目录跟共存那一块同一套：`Place::coexist_dir`）：

- **OpenClaw**：`workspace\` 下九份（AGENTS / SOUL / USER / IDENTITY / TOOLS / MEMORY / HEARTBEAT / BOOTSTRAP / BOOT，cc-switch 的白名单）
  和每日记忆 `workspace\memory\YYYY-MM-DD.md`（全文搜索不分大小写）；
- **Hermes**：`memories\MEMORY.md` / `USER.md`，加 `config.yaml` 里 `memory:` 小节的四个键（`memory_enabled`、`user_profile_enabled`、
  两个 `*_char_limit`；只改这四个，同一小节里别的键、别的小节、注释都不动）。网页面板只在它开着时打开（问
  `127.0.0.1:<HERMES_WEB_PORT 或 9119>/api/status`，200 或 401 都算开着）。⛔ **不替使用者起 `hermes dashboard`**（cc-switch 会开终端跑它）：
  那要起一个跑命令的终端，正撞「别把 shell 当启动目标」；没开着就提示使用者自己在终端里跑；
- **Pi**：`SYSTEM.md`（整份替换系统提示词）/ `APPEND_SYSTEM.md`（接在后面），不许存空的（要停用就删掉那份文件，照 cc-switch）；
  斜杠命令模板 `prompts\<名字>.md`，名字照 cc-switch 的 `validate_template_slug`（不许空白、`<>:"/\|?*`、首尾的点、Windows 保留名）。

守住的几条：

- ⛔ **文件名一律白名单 / 固定形状**，别的路径（含 `..`）一概不认；**工具目录不在（没装）就不写、不建**（cc-switch 会建），装了子目录按需建；
- **写回前核对这期间没被别处改过**：版本 = 内容的 SHA-256（照 cc-switch 的 Pi，三家统一），对不上就拒绝、让使用者先刷新；
  写盘走 `config_io::commit`；单份最多 1 MiB；
- ⛔ **删除先进备份**（`tool-file-backups\<工具>\<时间>-<文件名>`；cc-switch 直接删）。
- **oh-my-opencode 没搬**：cc-switch 把它做成 OpenCode 的一类「供应商」（`omo` / `omo-slim` 分类 + 各自的当前供应商指针），
  切换时整份重写 `oh-my-opencode.json` 的 agents / categories，「停用」时**整份删掉**那个文件。QB Gate 的 OpenCode 只写自己那一条
  `qb-gate-` 供应商、没有「OpenCode 的当前供应商」这套模型；要搬等于新做一块「agent 模型方案切换」，等使用者定（claude-op `23` §0.1）。

钉着的是 `tool_files_ops` 的 `only_whitelisted_files_and_only_when_the_tool_is_installed`、
`a_stale_revision_is_refused_and_deleting_keeps_a_backup`、`pi_templates_and_hermes_memory_settings` 和 `ToolFilesPanel.test.tsx`
（按读的时候的版本存、删除先确认、没装只读、Hermes 设置存得上、网页面板答了才打开）。

## 站点卡表头的余额 / 编程套餐额度（2026-10-26，中转站重构阶段 6 · U3，照 cc-switch 的 `balance.rs` + `coding_plan.rs`）

中转站页每张站点卡的表头原来固定写「余额（整站共享）站点未提供」。现在认得出的几家用那个站点在这个软件下**第一条线路的 Key** 问一次：
余额（DeepSeek、阶跃星辰、硅基流动国内 / 国际、OpenRouter、Novita AI）或编程套餐的各窗口（Kimi For Coding、智谱 GLM / Z.ai、
MiniMax 国内 / 国际、OpenCode Go）。认哪一家、怎么解析在 `qb-station::station::site_quota`（纯函数，这个 crate 没有 reqwest），
问在 `usecase::site_quota_ops`，界面 `StationSiteQuota.tsx`。

- **什么时候问**（计划 §7 阶段 6 写的、使用者勾过的）：打开中转站页 / 切到那个软件时问一次，这把 Key（id + 版本）5 分钟内问过的直接给那一份；
  点表头那颗小刷新图标再问一次。**没有定时器。** 认不出是哪一家（绝大多数中转站）**一个请求都不发**。
- ⛔ **只按主机名认，不认子串**：cc-switch 多数按 `base_url.contains("api.deepseek.com")` 认，中转站把官方域名写进路径就会把
  **中转站的 Key** 发到 DeepSeek 去。Key 只发给它本来就配给的那一家（唯一跨主机的是 MiniMax 国内新域名 → 同一家的旧域名
  `api.minimaxi.com`，同 cc-switch）；不跟随跳转。`every_quota_request_goes_to_a_host_of_the_same_provider` 钉着。
- **读不出来的格子不给数**：cc-switch 的 Kimi 缺 `limit` / `remaining` 会按 1 / 0 算成「用光」、阶跃 / 硅基缺余额按 0；这里那一格不要，
  一格都没有就报「回复里没有认得出的……」。问不到时界面留着上一次的（悬停里时刻照实），不抹成 0；回复内容不原样进报错。
- **只显示**：调度、排序、换线一概不读它；结果只在内存里。
- 没搬：ZenMux（要另填管理地址和管理 Key）、火山方舟（要账号 AK/SK、V4 签名 —— 比 API Key 大得多的凭证，不往面板里存）、
  智谱团队版（要组织 / 项目 ID）、Command Code（四个未公开的 `/alpha` 路由，预设里也没有它）。要加先问使用者。

## 中转用量页 `/relays/usage`（2026-10-26，中转站重构阶段 6 · U1，照 cc-switch 的用量页）

本机路由五个软件（Claude Code、桌面端、Codex、Gemini CLI、Grok Build）的请求：概览、按天 / 今天按小时的趋势（`ui/BarChart`，
站内花费 / 官方价折算 / Token / 请求数四选一，一张图一个量）、全年热力图（`ui/YearHeatmap`）、按软件 / 站点 / 模型三张表、
逐条日志（按模型、只看失败筛，分页）。从中转站页总览那一格的「用量明细」进来（带着当前软件），不进侧栏；能滚动。
后端 `usecase::relay_usage_ops`，命令 `station_usage_report` / `station_usage_requests`（读之前先把内存里没落库的那批写掉）。

- **按天汇总表 `usage_daily`（迁移 10）**：`router_requests` 只留 30 天，裁之前在**同一个事务**里先按「本地日期 + 软件 + 线路 +
  模型 + 上游模型」加进汇总（`Repository::prune_router_requests`）。汇总和明细永远不重叠，相加就是全部 ——
  `pruning_rolls_requests_into_usage_daily_without_counting_twice` 钉着。token / 花费那几列可以是 NULL（「没记」，不是 0），
  合并时 NULL 加 NULL 还是 NULL。⛔ 迁移 10 之前被裁掉的明细已经没了，汇总只从现在起攒。
- 「成功」的口径全项目一个：没记故障、状态码 2xx（`RouterRequest::ok`）；SQL 那边是 `OK_SQL`，状态码 NULL 也算失败（`coalesce` 成 0，
  不然「只看失败」会漏掉断在半路的）。
- ⛔ **只有经本机路由的请求**：共存型工具和桌面端直连档写的是真 Key、不经路由，这里看不到 —— 页面上写明。cc-switch 会去读
  OpenCode / Pi 等各家的会话日志补这一块，没搬（解析器太大、各家格式各变各的；要做先问使用者）。
- 热力图：一个色相由浅到深（`--chart-1` 往底色里 `color-mix`，样式在 `workspace.css` 的 `.qb-heat-*`），四档按有请求那些天的四分位切；
  **最早那条记录之前的格子不画**（说不出那天用没用），有记录以来没请求的是中性灰。表格版是「按月看」。
- 一个联网请求都没有；数字写法跟 `/usage` 共用 `lib/usageFormat.ts`（没记到的写「—」、不到一分钱写「<$0.01」）。

## 会话管理器：扩展中心「会话」分页（2026-10-26，中转站重构阶段 6 · S1，照 cc-switch 的 `session_manager`）

Claude Code 和 Codex 留在本机的会话记录：列表（按来源筛、搜标题 / 项目目录 / 会话 id，可选连内容一起搜）、阅读（分页、工具调用
和输出折叠）、导出 Markdown（保存框，`dialog:allow-save`）、复制恢复命令、删除（先进回收站，能放回去）。后端 `usecase::session_ops`，
命令在 `commands::session_manager`（模块名不叫 `sessions`：`qb_platform::sessions` 已经占了），界面 `SessionsPanel.tsx`。
放在扩展中心而不是侧栏：侧栏六个入口在 680×640 那一档已经是满的（`routes.ts`）。

- **读哪些地方**跟 MCP / Skills 同一张目标表（`ext_place::Place::client_slots`）：中转环境、官方默认目录、每个官方槽位。
  Claude Code 读 `projects\<项目>\<会话>.jsonl`，Codex 读 `sessions\YYYY\MM\DD\rollout-*.jsonl` 与 `archived_sessions\`。
  Gemini CLI、OpenCode、Pi 的没搬（cc-switch 各写了一个解析器，格式各变各的）。
- **标题**取第一条像样的提问：跳过 `isMeta`、`<command-name>` 这类尖括号包起来的回显、Codex 塞进去的 `<environment_context>`；
  Codex 只看 `response_item`（`event_msg` 里同一句话是重复的）。开头只读 256 KB，按（路径、修改时刻、大小）缓存在内存里。
- ⛔ **会话 id 是 `<目标 id>|<相对路径>`**，拿回来时核对目标在表里、路径只在 `projects/` / `sessions/` / `archived_sessions/` 下、
  没有 `..`、是 `.jsonl` —— `a_forged_id_cannot_reach_another_file` 钉着。
- ⛔ **删除 = 挪进 `state_dir\session-trash`**（跟酒馆同一套 `tavern_library::Trash`，Claude Code 同名的目录一起挪），
  **两分钟内还在写的不让删**（多半正开着）；审计那一行写在命令层 —— 写在 usecase 里单测会往真实的 `ip-gate.log` 里追加。
- **恢复命令只复制**（PowerShell，路径单引号转义；默认目录不设变量，别处设 `CLAUDE_CONFIG_DIR` / `CODEX_HOME`），不替使用者开终端。
  旁边写明：官方账户的要门禁放行才起得来，中转环境的要本机路由开着，都要装了对应的命令行。
- 界面一段最多 6000 字；导出 Markdown 时只截工具调用、工具输出和思考，使用者和助手说的话整段导出。

## 用量脚本：给某个站点写一段 JS 问余额（2026-10-26，中转站重构阶段 6 · U4，照 cc-switch 的 `usage_script.rs`）

站点卡表头那一格的「脚本」打开编辑器（`SiteUsageScript.tsx`）：一段求值成 `({ request, extractor })` 的 JS，面板发 `request`、
把回复交给 `extractor`，它返回 `{ remaining, total?, used?, unit?, planName?, isValid?, invalidMessage? }`（或一组）。开着的话那一格就用它
（`site_quota_ops` **脚本优先**于内置认得的那几家），节奏跟 U3 一样（打开页面问一次、5 分钟内复用、点刷新再问）；能「试跑」而不存。
变量只有三个：`{{apiKey}}`（这条线路的 Key）、`{{baseUrl}}`、`{{origin}}`。存在库里 `site-usage-script:<站点 id>`。

- **沙箱是独立的小 crate `qb-script`**（L3，`rquickjs` =0.8.1，跟 cc-switch 同一个版本，QuickJS 的 C 源码用本机 MSVC 编进来）：
  只算 —— 不往里注入任何宿主函数，没有 `fetch` / 文件 / `require`；内存 16 MiB、栈 256 KiB、5 秒（解释器每轮看钟，到点中断）；
  每一步新建运行时、用完即丢。`a_runaway_script_is_stopped_and_cannot_hog_memory`、`the_sandbox_has_no_way_out` 钉着。
- ⛔ **请求地址必须跟站点同源（协议 + 主机 + 端口）而且是 https，没有例外**（本机回环地址的站点允许 http）。cc-switch 的
  「自定义模板」会跳过同源检查 —— 那样一段脚本就能把这个站点的 Key 发到任何地方。不跟随跳转、回复最多 1 MiB、回复内容不进报错。
  `the_request_must_stay_on_the_site_and_use_https`、`a_script_that_points_elsewhere_never_sends_the_key` 钉着。
- Key 只在跑的那一刻换进 `{{apiKey}}`；**脚本存在库里不加密**，编辑器上写明别把 Key 直接写进去。
- 结果只显示；导入链接里的用量脚本**照旧不导入**（阶段 5 的决定：链接带来的代码不在面板里跑）。
- 许可：`rquickjs-sys` / `rquickjs-core` 包里顶层没有许可文件，QuickJS 自己的 MIT 在 `quickjs/LICENSE` —— 补在
  `docs/license-supplements.json`，安装包的 `THIRD_PARTY_NOTICES.txt` 才带得上。升级这个依赖要连这两条一起改。

## 环境变量清理：只删当前用户那一处，先备份、能还原（2026-10-27，中转站重构阶段 7 · X5，照 cc-switch 的 `env_checker.rs` / `env_manager.rs`）

环境体检「本机体检」末尾的「清理环境变量…」（`EnvCleanup.tsx`；修复中心那一节写着去哪找）。后端 `qb-sysenv::sysenv::envvars`，
命令在 `commands::env_cleanup`（模块名不叫 `envvars`：workspace 里模块名要全局唯一）。

- **「哪些变量算相关」全项目只有一张表**：`envvars::owner_of`（`ANTHROPIC_*` / `CLAUDE_CODE_*` / `CLAUDE_CONFIG_DIR` → Claude Code，
  `OPENAI_*` / `CODEX_*` → Codex，`GEMINI_*` / `GOOGLE_GEMINI_*` → Gemini CLI，`XAI_API_KEY` / `GROK_DEFAULT_MODEL` → Grok Build，四个代理变量）。
  体检那一项的 `checkup::is_watched_env` 也改成问它（Claude Code + 代理两类；`ANTHROPIC_` 从五个死名字变成按前缀认）。
  ⚠ `purge_ops::is_owned_env`（完全卸载）**故意不并进来**：卸载问的是「专门为这个软件设的」，`OPENAI_*` / `GEMINI_API_KEY`
  别的工具也在读，卸 Codex 时删它会误伤；清理问的是「会改变它去向的」，由使用者逐个勾。两件事两个口径，别合并。
- ⛔ **只删 `owner_of` 认得的名字**：前端传来一个 `Path` 也删不掉（整次直接报错，什么都不动）。只删 HKCU；整机的（要管理员，面板不提权）
  和只在面板进程里的（起面板的那个程序传进来的）只列出来、给步骤。**什么都不预先勾。**
- ⛔ **先备份，备份写成功了才删**：面板数据目录 `env-backups\<编号>.json`，值用 DPAPI 封住、记着原来是不是 `REG_EXPAND_SZ`；
  先写 `.tmp` 再改名。还原按原来的类型写回；现在有一个**不一样的**同名变量就不盖，报出来（cc-switch 直接盖）。
  备份编号从界面传回来时只认面板自己起的那种形状，带 `..\` 的一律拒绝。
- 改完广播 `WM_SETTINGCHANGE`（`qb_platform::registry::broadcast_environment_changed`）：不发的话资源管理器手里还是旧环境，
  新开的终端照样带着刚删掉的变量。已经开着的程序（包括面板自己）不受影响，界面上写明。广播最多等 5 秒，所以命令都是 async + `spawn_blocking`。
- 单测经 `UserEnv` 用内存里的假注册表、临时目录；审计只写在命令层（坑 7.107）。

## 其他命令行工具：看现状只读文件，官方脚本先给人看（2026-10-27，中转站重构阶段 7 · X3，照 cc-switch 的工具管理）

软件页「其他命令行工具」卡（`features/CliToolsCard.tsx`）：OpenCode、Grok Build、OpenClaw、Hermes、Pi、MiniMax Code。
位置与「交给谁做」在 `qb_install::install::cli_tools`（版本号那一半在 `cli_tools_version`），编排在 `usecase::cli_tools_ops`，
命令在 `commands::tool_manager`。Claude Code / Codex / Gemini CLI **不在这里**：它们走上面各自的卡（托管安装、门禁上锁、清外部副本）。
这六个不归门禁管、不进托管目录。

- **看现状只读文件**（`cli_tools_status`，打开软件页就读）：按 cc-switch `build_tool_search_paths` 的 Windows 那一支找入口
  （官方安装器的落点在前，npm / pnpm / Volta / nvm-windows / Scoop… 在后，最后是合并后的 PATH —— 进程 PATH + 注册表里用户与整机的 `Path`，
  `%VAR%` 展开；一键更新后被拉起的面板进程 PATH 里可能没有用户那一半）。版本从 `.cmd` 启动壳顺着找到包目录读 `package.json`；
  **不运行它们去问 `--version`**（cc-switch 会），读不出就说读不出。命令行默认用哪份按合并后的 PATH 自己找（`.exe` 先于 `.cmd`），不起 `where.exe`。
- **联网只在点了之后**：「检查新版本」问 npm 的 dist-tags 专用地址（不拉整份 packument）、GitHub 最新发布、PyPI（Hermes 的 PyPI 停更了，
  比本机旧的不显示）；同一次顺带问新版本的 `engines.node`，本机 Node 不够就不给「升级」按钮（cc-switch 的 `tool_node_requirement`，范围判断照搬）。
- **升级锚定到命令行用的那一份**（`update_plan`）：npm / nvm-windows → 同一层的 `npm.cmd`（默认 `%APPDATA%\npm` 前缀用 PATH 上那个）；
  pnpm / Volta → 同一层的 pnpm / volta；Grok 官方安装器 → `grok update`；MiniMax 官方安装器 → 重跑它的脚本、`MCODE_INSTALL_DIR` 指回同一处
  （`mcode update` 在没有终端时什么都不装）；Hermes → `hermes update`；OpenCode 自己的安装器 → 只说「在终端里跑 opencode upgrade」
  （它在 Windows 上可能弹交互提示，静默跑会挂住）。**认不出是谁装的就不替人升**：裸跑 npm 只会在别处另装一份，命令行用的版本不动。
  好几份又分不清默认那一份，也不升。卸载只做 npm / nvm-windows / pnpm / Volta 装的那几类。
- 起进程一律经 `winget::run_streaming_env`（每个参数一个 argv、隐藏窗口、等它退出、输出进进度条 `cli-tool`）；`.cmd` 直接当程序交给标准库
  （它替批处理套 `cmd.exe`、按批处理的规矩转义）。装完 / 升完 / 卸完**回读盘核对**，不看退出码。锁只在命令层拿（坑 7.51）。
- ⛔ **官方 PowerShell 安装脚本（Grok Build、MiniMax Code、Hermes）先下载给人看，确认之后才跑**：`cli_tools_script_fetch` 只连这个工具登记的
  那一个地址（https，跳转到非 https 拒绝，大于 1 MiB 拒绝），存进面板数据目录 `tool-installers\`（补 UTF-8 BOM，坑 7.6）、算 SHA-256、
  全文交给界面；`cli_tools_script_run` 只认那一份的编号**和**哈希，盘上的内容被改过也拒绝；`powershell -NoProfile -NonInteractive
-ExecutionPolicy Bypass -File`（`-NonInteractive`：脚本要是停下来问东西就报错，不在看不见的窗口里永远等），跑完删掉。
  cc-switch 是 `powershell -EncodedCommand "irm … | iex"` —— 拿到什么跑什么，而且是 base64（「杀软会把这个面板当木马」第 1 条）。
  ⛔ **两步之间必须有人看过、勾过「看过了」**，别把它们串成一条命令；升级失败也不自动接脚本（cc-switch 的 `|| <官方脚本>` 没搬）。
- WSL 里的工具不管（门禁锁不到 WSL 里的东西，阶段 7 的计划写明）。

## 开机自启、轻量模式、启动时拉起本机路由（2026-10-27，中转站重构阶段 7 · X7，照 cc-switch 的 `auto_launch.rs` / `lightweight.rs`）

设置页「常规 › 后台运行」三行。面板的定位不变 —— 它是个面板，不是后台服务：门禁、看门狗、本机路由、智能调度都活在面板进程里，
**托盘的「退出 QB Gate」才真的停**（「智能调度是落盘的承诺，而且面板关了它就停」那一节照旧成立）。

- **开机自启**（`qb_platform::autostart`）：写 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 下的 `QB Gate`，
  内容 `"<qb-gate.exe>" --autostart`。带 `--autostart` 起来、启动恢复也过了，就不露面、只留托盘（`src-tauri/src/window.rs`）；
  启动恢复没过照常露面（藏起来等于让使用者永远看不到要他处理的事）。
  - ⛔ **值名必须等于 `tauri.conf.json` 的 `productName`**：Tauri 的 NSIS 卸载程序（升级时不跑）只删 `Run` 下名叫产品名的那一项
    （2026-10-27 从 CLI 2.11.4 内嵌模板核过）。`src-tauri/tests/bundle.rs` 钉着；改产品名要连这里一起改，已经登记的那一项还得另想办法迁。
  - ⛔ **只能由设置页那颗开关点出来**（`autostart_set`），别接到任何启动流程、导入链接、项目上 —— 程序自己给自己加开机启动项，
    是杀软眼里最典型的持久化行为。写注册表走 `qb_platform::registry` 直接调 API，**不起 `reg.exe`**（`reg add …\Run` 这一行命令本身就是特征）。
  - 状态以注册表为准、不进 `settings.json`：任务管理器 / 设置 › 启动应用里点「禁用」时 Windows 不删 `Run`，而是在
    `…\Explorer\StartupApproved\Run` 记一条同名的二进制值（首字节奇数 = 禁用），面板读出来照实显示；在面板里重新打开会删掉那条禁用记录。
  - 开发版（`debug_assertions`）拒绝登记：登记下去开机起的是 `target\debug` 里的中间产物，连着开发分支的库结构（坑 7.106 那一类）。
- **主窗口藏着建**（`tauri.conf.json` 的 `visible: false`）：页面第一次加载完（`on_page_load`）才露面，8 秒还没加载完也照样露面 ——
  托盘没建起来的机器上，永远不露面的窗口就是一个叫不回来的进程。顺带没了冷启动那一帧白底。
- **轻量模式**（设置 `lightweight_on_close`，默认关）：关窗口时不藏，让它真的关掉、WebView2 一起释放；最后一个窗口没了 Tauri 默认会退出，
  `RunEvent::ExitRequested` 在托盘可用时只拦 `code == None` 那一种（「最后一个窗口关了」）—— 托盘「退出」和一键更新走 `app.exit(0)`，照常退。
  托盘、第二次启动（单实例回调）、`qbgate://` 链接都经 `window::reveal` 叫窗口：在就露面，不在就照配置重建（丢进异步任务 ——
  同步回调里建窗口在 Windows 上会死锁，wry#583）。要带着去某一页（托盘「中转站」）就在重建时用初始化脚本先设好地址栏的 `#`。
  默认关的理由：释放之后再打开要重新加载，填到一半的弹窗会丢。托盘没建起来就不释放（同「关窗口 = 收进托盘」的理由）。
  `window` 不许回头调 `tray`（`tray` 要调 `window::reveal`），托盘在不在由调用方传进 `keep_on_close`。
- **面板启动时拉起本机路由**（设置 `start_router_on_launch`，**默认关**，2026-10-27 使用者定的：「加开关，默认关」）：开着时，`setup` 里把路由设置
  与上次选中的线装回去之后接着起监听（`commands::station::start_router_on_launch` → `ensure_router`），跟点「启动」时起的是同一个；
  只起监听、不起任何客户端；起不来（15721 被占着，比如 cc-switch 的本机代理）只记审计、不挡启动。阶段 1 第 8 步留下的那一问就此有了答案。

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
会把上次开着的接回来（`lib.rs` 的 setup 里）。

⛔ **那张表在 0.16.0–0.25.6 根本不存在**（2026-10-17 才查出来）：命令层一直在读写它，可没有一条迁移建过它，
`Repository::table()` 的白名单里也没有 —— 「开启」那一步必报「未知的数据表」，读的一侧又把错误吞成「关着」，
真机上调度从没开成过；演示模式喂的是假数据，`test:ui` 一直全绿。迁移 4 补上了。所以：

- 调度的存取只走 `station_ops::load_schedule` / `save_schedule`。**只有「没有这一行」才回落成「关着」**
  （`Repository::find` 给 `None`），表不在、库读不了、存的读不出来都照实报错 —— 别再写回
  `get(..).unwrap_or_else(|_| 默认值)`，这个 bug 就是这样从 0.16.0 一直藏到 0.25.6 的（§7.17）；
- **加一张表要同时做两件事**：`MIGRATIONS` 末尾追加建表语句、名字写进 `repository.rs` 的 `TABLES`。
  `every_whitelisted_table_is_created_by_a_migration` 钉着「进了白名单却没建」那一半；
  「建了却没进白名单」只能靠走真实存取路径的测试抓（`a_schedule_switched_on_is_still_on_after_the_panel_reopens_the_database`）。

相应地：

- **界面上必须写明「面板关掉即停」**。循环活在面板进程里，关掉面板调度
  就不再换上游了，而客户端还在照着上一次选的那条线跑 —— 可能是几天前
  那条已经涨价的。这不是缺陷，是这个软件的定位（它是个面板，不是后台服务），
  但不说清楚就是骗人；
- **一跳 30 秒，但站点账单最多 60 秒拉一次**（2026-10-24 起，中转站重构阶段 2；缓存在 `AppState::billing_cache`）。
  账单那一档别调小：拉账单是免费的（不打模型），但十条线的池子在 10 秒档上就是每分钟 60 个请求打到账单接口，
  有些站点会因此限流 —— 而限流的表现是健康度读不到，排序退化成只比倍率。**盯得太紧反而让它瞎掉。**
  一跳能缩到 30 秒，是因为派单的证据大半来自本机路由自己的日志、熔断器和测活，不必等账单；
- **防抖只有一道**，在 `schedule::dispatch`（sub2guard 的那一套：挑战者领先 `margin`、自己至少 `min_samples`
  个样本、连续 `confirm` 轮**有新证据**都领先才换；默认 8% / 3 / 2，每个软件一份存在 `Schedule.dispatch`）。
  `rank` 一律不给 `incumbent`（0.25.6 及以前迟滞在 `rank` 里），`next_upstream` 只问一句「派单的第一条是不是现任」——
  再判一次就是两道叠在一起，结果是永远不换，而且没人说得清为什么；
- **熔断中、测活判死、没过底线的线永远不当第一。** 派单分四档：钉住 → 合格 → 兜底（没过底线，**不拉黑**，
  照样留在队列里接失败切换）→ 故障（熔断中 / 测活判死）。现任掉档了而有合格的才立刻换；没有合格的就留着现任
  （本机路由每一发自己会跳过熔断着的线）。底线全卡光时 `rank` 会放开底线重排一轮，那是为了不制造死局，
  **不是**为了把不合格的送上生产。唯一的例外是还没有当前上游：那时挑排第一的（哪一档都行，不然客户端每一发都是
  「还没选上游」）；
- **测活判死要让位给真实流量**：判死之后本机路由在那条线上成功过，就不算判死（`station_ops::line_facts`）——
  一次测活不能把正在干活的线打进故障档；
- **调度开着时**：手动选的线钉住 4 小时（`station_select_route` 顺手钉上，它挂了就不算）；热备顺序归调度排，
  `station_dispatch_queue_save` 拒收手排的顺序（路由设置弹窗里那张表只读）；
- **测活两种**（`usecase::station_liveness`，判读在 `schedule::liveness`，改写自 sub2guard）：免费的发空请求体，
  只证明「线通、Key 认」，调度开着时自动跑，最近走过真实流量的线不测；花钱的**默认关**（使用者选的），
  开了也按每日预算（元，已乘充值比例）封顶，不知道价的线按偏贵的价（`UNKNOWN_PRICE_PER_MTOK`）记上界。
  ⛔ 这是唯一一个会**自动**花钱的入口，别再加第二个；测活记录里不许有请求内容、Key、地址。

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

**数据库这一半由代码守着**（2026-10-26 起）：测试程序里（cargo 的 `deps\<crate>-<哈希>`）`Repository::open()` /
`open_in(真实目录)` 直接报错，单测一律 `open_in(临时目录)`。那天一条单测经 `extensions::catalog()` 间接开了真实的库，
把它迁到了开发分支的结构版本 9，装着的面板（只认 3）当场停在「需要完成数据恢复」—— 间接调用读源码看不出来，规矩写着也没拦住。
钉着的是 `repository` 的 `the_real_database_refuses_to_open_inside_a_test_binary`。⚠ 这道护栏只管测试程序：
`npm run tauri dev` 和会开库的诊断例子（`usage-summary` 等）照样开真实的库、照样会把它迁上去 —— 本机装着旧版时别跑它们，
要跑就先拷一份数据换个目录。

---

## ⛔ 分层由五条测试守着，不是由自觉

Rust 侧是一个 Cargo workspace：`qb-foundation`(L0) → `qb-contract` → `qb-platform`
→ 九个领域 crate → `qb-app`(编排) → `src-tauri`(命令/托盘/装配)。

`src-tauri/tests/architecture.rs` 里有十四条测试，其中五条是分层的硬规矩（另有两条管 IPC 命令，见下一节）：

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

## ⛔ 注册的命令两边都得有人：删界面入口就连命令一起删（2026-09-27）

使用者发现「旧前端页面和旧后端代码依旧保留在项目内，使用者在某些情况下可以进入这些废弃页面」。
查下来：「服务商与环境」旧页、新手引导、中转环境模板早就不在侧栏里，可快速跳转、扩展页、设置页还在往那些地址上送人；
后端挂着三十多条界面一次都不调的命令，其中三条只回一句「旧版混合配置入口已停用」。没有任何东西拦着这类残留长出来。
现在有两道，各守一半，**别删**：

| 测试                                                                      | 它不许发生什么                                                                                                                                   |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `architecture.rs` 的 `every_registered_command_is_called_by_the_frontend` | `generate_handler!` 里有一条命令，前端业务代码里一次都没提到它的名字；或者前端 `call("x")` 调了一条没注册的命令（写错一个字母 = 「点了没反应」） |
| `src/lib/apiUsage.test.ts`                                                | `api` / `workspaceApi` / `stationApi` / `codexApi` / `antigravityApi` 里有一个成员没人调 —— 上一条只证明「名字被提到过」，包装本身就提到了       |

所以：

- **删一个界面入口，就顺着调用链把包装、命令、只有它在用的后端函数一起删。** 只删按钮、留着命令，下一个人会以为它还有人用；
  只留页面、删掉入口，它照样从快速跳转、深链接、别的页面的链接走得进去（2026-09-27 就是这么发现的）。
- **只在 Rust 里调的函数别挂 `#[tauri::command]`**（例：`station_refresh_prices` 由启动任务直接调，不是命令）。
- 按名字动态调的包装（GPT 页的 `codexApi[ask.action]`）写进 `apiUsage.test.ts` 的豁免，并写明调用点 —— 那条测试会核对调用点还在。
- 页面删了，路由也删：认不出的地址由 `Shell` 末尾那条 `*` 送回首页，**别再为旧地址单独开一条重定向**（桌面应用没有书签）。
- ⛔ 删功能**不动数据形状**：数据库表、`DataBackup`、`repository::migrate()` 照旧能读老数据；类型上删字段靠 serde 忽略未知字段，
  必要时加一条「老数据照样读得进来」的测试（`progress.rs` 那条就是）。

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
- 中转站草图（V24）用的是**合并前的旧配色**，照草图做页面时**不要从草图里抄 hex**；
- ⛔ **引用的变量名得真的存在**（2026-09-29）。没定义的 `var(--x)` 浏览器不报错，那条声明在计算时作废：
  桥接弹窗里 `--line` / `--line-soft` / `--surface-1` / `--text-1` 四个名字从没定义过，边框与底色一直没显示，
  账户页酒馆磁贴右上角那颗设置按钮是透明的；侧栏子项悬停用的 `--hover` 也是。`src/styles/tokens.test.ts` 现在查
  「所有样式里的 `var(--x)` 都在某处定义过」—— 红了先去 `tokens.css` 找对的名字，别往里补一个同义的新令牌。

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

Claude 的 5 小时 / 7 天**平时仍然只读本机**。09-24 使用者被问到时选过「不加，保持只读本机」；
2026-10-26 中转站重构阶段 6 他勾了 U2（官方订阅额度联网），`/api/oauth/usage` 接进来了，
但节奏照「联网额度」那一节：只在点那一行的刷新图标 / 详情里「向官方问一次」时问这一个账户，只显示。
⛔ 别给它加定时器、别在打开页面时问。

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

| 站点后端   | 它在 API 里给什么                  | 长什么样                       | 折扣在哪                                                          |
| ---------- | ---------------------------------- | ------------------------------ | ----------------------------------------------------------------- |
| New API 系 | 倍率（1 = $2 / 百万 token 的单价） | `model_ratio: 2.5`             | 分组倍率（`/api/pricing` **顶层** `group_ratio`），或直接压低倍率 |
| sub2api 系 | **绝对单价**                       | `input_price: 5`（美元／百万） | 分组倍率（`/api/v1/groups/rates`）                                |

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
