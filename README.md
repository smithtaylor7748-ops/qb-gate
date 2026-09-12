# QB Gate

Claude 运行环境控制面板。把 IP 锁、纯净度复核、DNS 泄露检测、环境检测与安装、
中转站配置、账户与启动收进一个界面。

Windows 桌面应用，Tauri 2 + React + Rust，GPL-3.0-or-later。

---

> ### ⚠ 使用前必读：[免责声明 DISCLAIMER.md](DISCLAIMER.md)
>
> - **非官方项目，与 Anthropic PBC 没有任何隶属、合作或背书关系。**
>   "Claude"、"Anthropic"、"Codex"、"OpenAI" 等商标归各自权利人所有，
>   本文档中提到它们只为说明本工具与哪些软件配合使用，不代表取得任何授权。
> - **不承诺「防封」。** 面板所有检测都是基于公开接口的参考性判断，
>   不等于任何服务商的实际判定，不构成安全保证。
> - **不含任何代理 / VPN / 翻墙功能**，只读取并展示你已有网络环境的状态。
>   网络接入是否合法由使用者自行负责。
> - **所有账户必须是使用者本人拥有的。** 不得用于规避限流、规避封禁、
>   账号共享或买卖。
> - **部分操作不可逆。** 尤其「Chrome 清空重装」会**永久删除**整个
>   Chrome `User Data`（书签、密码、扩展、全部登录态），执行前请先备份。
> - **本项目不是安全边界**，存在已知绕过路径，别当强制访问控制用。
>
> **This is an unofficial, independent project, not affiliated with or endorsed by
> Anthropic. Provided AS IS with no warranty. It does not prevent account bans and
> contains no VPN/proxy functionality. Some operations permanently delete data.
> See [DISCLAIMER.md](DISCLAIMER.md) before use.**

---

## 它解决什么

在 VPN 后面用 Claude，有一串琐碎但要命的事：出口 IP 变了没人拦、DNS 悄悄
从物理网卡漏出去、系统时区跟 IP 对不上、装了一半的旧版本残留一个没上锁的
可执行副本。这些原本散在十几个 .ps1 / .cmd 里，只有写脚本的人自己会用。

这个面板把它们收成一个界面，并且**把踩过的坑固化成回归测试**——
移植时最容易丢的恰恰是那些看起来多余的分支。

---

## 五个步骤就是侧栏

没有单独的「新手引导」页。左侧那五个菜单本身就是引导步骤，从上往下走：

| | 步骤 | 做什么 |
|---|---|---|
| 1 | IP 纯净度 | 三项硬指标复核，不合格整个面板爆红 |
| 2 | 环境与安装 | 检测、托管安装 Claude Code / Codex、清外部副本、时区对齐、中文环境识别 |
| 3 | DNS 泄露 | 简易通过（真实解析回显）与高级通过（交给 Codex） |
| 4 | IP 锁 | 白名单、执行锁、看门狗 |
| 5 | 账户与启动 | 登录、切换槽位、验证 IP 后启动 |

导航是自由的——随便点，只标状态。每行右侧标风险度，走完一轮后序号换成状态灯。
**每一步都能强制跳过**，但跳过会记账，总览上显示「跳过 N 项」，
不让它变成静默的坑。

---

## IP 锁是怎么锁的

门禁的本体不是脚本里的 if 判断，是 NTFS ACL：给 `claude.exe` 加一条针对
**当前用户**的 `Deny ExecuteFile` ACE。锁上之后连双击都会被系统拒绝执行。

所以**登录必须走面板里的受控入口**，不能叫人去双击 exe——那会被系统拒绝，
这是设计如此。

几条不显然但很贵的设计，改代码前先读：

- **「查不到 IP」和「IP 变了」必须分开处理。** 合并的写法会让网络抖一下、
  隧道重连一次、查询服务限一次流，就直接杀掉正在进行的会话。
  现在是：查不到 → 立刻上锁但**留着进程**（能不能发出请求由锁说了算，
  那才是真正的管控点），网络回来且 IP 仍合法就自动解锁续跑；
  CLI 侧 180 秒后才关停。
- **桌面端不给宽限。** 它冻不住——真正在跑的是 `app-*` 下那个不能加 Deny 的
  副本，而且早把自己加载进内存了。「等等看」的实际含义就是「让它在无法核实的
  网络上继续跑」。代价：VPN 重连或查询服务限流会直接关掉正在用的桌面端，
  丢掉没保存的对话。
- **IP 仍在白名单、租约却被人收走了 → 看门狗要自己要回来。** 否则「加白名单」
  这个动作本身（编辑器开场就重新上锁）会静默废掉正在跑的 Claude。
- **自动更新会把锁弄丢。** 官方安装器写的是全新 exe，继承干净 ACL。
  所以租约期钉 `DISABLE_AUTOUPDATER=1`，退出时重锁**全部**副本，
  安装器装完主动重锁。
- **`claude.exe.old.*` 是没有 Deny ACL 的可绕过副本。** 而且发起升级的会话
  自己占着删不掉（Windows 允许改名正在运行的 exe，不允许删除），
  所以清理放在**下一次**升级的开头。

### 要锁哪些文件

**每一个完整可执行的 `claude.exe` 副本都必须锁上。漏掉一个，那一个就是现成的绕过入口。**

「Claude 装在哪」全项目只有一张表：`src-tauri/src/install/inventory.rs`。
检测、启动、上锁、升级、残留清理都从它拿 —— 早期这四件事各拼一份路径表，
已经对不上（只用 winget 装的人：上锁表锁住了，启动表里却没有，面板报「找不到」）。

| 来源 | 位置 | 锁 | 拿来启动 |
|---|---|:--:|:--:|
| **面板托管**（v0.9.0） | `<托管目录>\claude-code\claude.exe`，默认 `%LOCALAPPDATA%\ClaudeIpGate\apps`，见[安装 Claude](#安装-claude) | ✅ | 第 1 选择 |
| 官方安装器 | `~\.local\bin\claude.exe` | ✅ | 第 2 |
| 安装器版本库 | `~\.local\share\claude\versions\<版本>`（**是文件**，每个都是完整二进制） | ✅ | — |
| 安装器下载缓存 | `~\.claude\downloads\claude-*.exe`（安装器下载后没清掉的完整二进制，实测一份 218 MB） | ✅ | — |
| winget | `%LOCALAPPDATA%` / `%ProgramFiles%` 下的 `WinGet\Packages\Anthropic.ClaudeCode*` 与 `WinGet\Links` | ✅ | 第 3 |
| Scoop | `~\scoop\apps\claude-code\…` | ✅ | 第 4 |
| 旧脚本认的位置 | `%LOCALAPPDATA%\Programs\Claude\claude.exe`（那个目录是 Electron 应用就不算） | ✅ | 第 5 |
| PATH 上的 | 任何不在上面几处、但在 `PATH` 里的 `claude.exe` | ✅ | 第 6 |
| 桌面端带的 | `%APPDATA%\Claude*\claude-code\<版本>\claude.exe`（**每个桌面端资料目录、每个版本**） | ✅ | 第 7 |
| npm | `%APPDATA%\npm\claude.cmd` | ❌ 见下 | 最后 |
| 编辑器扩展 | VS Code / Cursor / Windsurf 等的 `anthropic.claude-code-*` 扩展里自带的 | ✅ | — |
| 桌面端存根 | `%LOCALAPPDATA%\AnthropicClaude\claude.exe` | ✅ | （启动桌面端） |
| 桌面端运行时 | `%LOCALAPPDATA%\AnthropicClaude\app-<版本>\claude.exe` | **不锁** | — |

环境页会把本机找到的全部副本列出来，标明哪份拿来启动、哪份锁得上。
MSIX 版桌面端与编辑器扩展这两类的布局是按安装机制推的，**作者本机没有，未实机验证**。

### 已知缺口

- `app-<版本>\claude.exe` 不能加 Deny ACE（一加，桌面端开新窗口就崩）。
  刻意进那个目录直接双击能绕开**启动**门禁，20 秒内会被看门狗收掉——
  前提是当时有看门狗在跑。要彻底堵死需要 AppLocker / WDAC，不在本项目范围。
- **npm 装的 Claude Code 锁不上**：`claude.cmd` 是批处理，由 `cmd.exe` 读进去执行，
  真正跑的是 `node.exe`。面板仍然会在启动前验 IP，一键关闭也收得到它，
  但执行锁管不到 —— 界面上如实标「执行锁管不到」。想让执行锁生效，改用官方安装器或 winget。
- MSIX 方式装的桌面端：`WindowsApps` 下的文件改不了 ACL，面板也没有存根可以拉起它。
  看门狗在出口 IP 不对时仍会关掉它。

### 解锁不能用 REVOKE_ACCESS

实测 `SetEntriesInAclW` + `REVOKE_ACCESS` 对 **deny** 条目会**返回成功却什么都不做**
（`gate/acl.rs` 的往返测试就是钉这条的）。照着写的话，锁上之后再也解不开，
面板会彻底打不开 Claude。

现在的做法是自己重建 DACL：取显式条目（`GetExplicitEntriesFromAclW` 不返回继承来的）、
滤掉我们那条再写回去。写回时**必须主动带 `UNPROTECTED_DACL_SECURITY_INFORMATION`**，
继承来的权限才会回来；一条显式条目都不剩时写一个**零条目的合法 ACL**，
**绝不能写空指针** —— 空指针 DACL（NULL DACL）的意思是「不做任何访问检查、人人完全控制」，
跟零条目 ACL 正好相反。早期版本在这里写错过，每解锁一次就把一份 claude.exe 交给所有人。

---

## 纯净度：面板不替你判定

三项硬指标缺一不可：**纯净度 ≤ 5%**、**原生 IP**、**住宅 IP**。

权威判定交给你自己看这两家，面板给一键跳转并把通过标准写在界面上：

- **IPQualityScore** — Fraud Score ≤ 5，且 Proxy / VPN / TOR / Recent Abuse
  全部为 No，且 Connection Type = Residential
- **ippure.com** — IPPure 系数 ≤ 5%，且 IP来源 = 原生IP，且 IP属性 = 住宅IP

面板自己也查（走 `https://my.ippure.com/v1/info`，公开无 Key），但界面上
明确标注**不权威**，且拿不到的字段**如实报「未知」，不猜成通过**。

不合格时整个面板爆红，红条里给 IPRoyal 的购买入口。

---

## DNS 泄露

**简易通过**两条腿走路：

1. 真实解析回显——向 bash.ws 取测试 id，依次解析 10 个探针域名，
   由它的权威域名服务器回报「是谁来查的」
2. 网卡配置检查——看有没有「物理网卡 DNS 指向内网路由器」

**高级通过**交给 Codex 跑内置提示词：读网卡配置、路由表、系统代理、
浏览器 DoH，必要时用 PktMon 抓包核实，发现问题给出修复方案。
提示词建议配高级模型——这一条要读网络配置并做判断，弱模型容易给出
看似合理但错误的结论。

---

## 插件商店

侧栏「随时可开」里。清单驱动（`src/plugins/registry.ts`），
现在是内置数组，将来换成拉远程 index 即可，调用方不用改。

官方清单仓库尚未创建前，商店保持“仅内置插件”模式，不接受任意下载地址。
启用远程清单时必须通过签名校验，并提供来源、许可证和文件哈希，才能刷新、安装或升级。

### 酒馆 SillyTavern

启动**真正的** SillyTavern 与 Claude 桥接 —— 所以世界书、角色卡、群聊、扩展
全部原样具备，面板不重写它的界面（那是在追一个永远追不上的上游）。

面板负责的是：IP 门禁、启停、依赖检查、以及资产的盘点与备份恢复。

**三个路径默认是空的，装上之后要自己填**（插件商店 → 酒馆 → 配置）：
SillyTavern 根目录、桥接根目录（含 `bridge.py`）、启动脚本。
填之前插件如实停在「依赖不齐」，不会假装能跑。
面板**不分发** SillyTavern 与 `bridge.py`，只在你已自行安装的前提下启动它们。

启动链移植自现有的 `start-claude-pro-rp.ps1`，几条关键行为原样保留：

- PID 文件**必须比对命令行**，指向别的进程时报错，不覆盖也不杀
- 端口被别人占 → **报错退出，绝不停止无关进程**
- 就绪判定要打 `/health`，不能只看进程活着
- **无论是不是复用现有服务都要打开浏览器页** —— 少了这步，
  成功的启动和崩溃看起来一模一样

资产备份是**目录复制**不是打包：出问题时可以直接进文件夹翻，不需要本程序
也能恢复。恢复之前会自动把现状再存一份。

## 一键关闭

只收满足证据的进程，三条之一：

1. 可执行文件由 Anthropic 签名；
2. 命令行**同时**命中 `bridge.py` 与本项目数据目录；
3. 进程是 `node.exe`，**并且**命令行指进 `node_modules\@anthropic-ai\claude-code\`
   —— npm 装的 Claude Code（`node.exe` 是 OpenJS 签名的，第 1 条永远认不出它）。

**绝不按进程名杀** —— 叫 `claude.exe`、`python.exe`、`node.exe` 的东西可能是你正在干的别的活。
面板自己和它的祖先进程一律放过。枚举失败会报错，不会退化成「0 个进程」。收完自动重新上锁。

切换账户、清外部副本、迁移托管目录（旧目录里有 Claude 在跑时）用的都是这一套。

## 升级

默认走 `latest` 渠道。写死 `stable` 会导致降级（实测 stable 可能比本机还旧），
面板会直接拦下并说明原因。

装了面板托管的 Claude Code，升级就升那一份：同样先比版本、同样拦降级，再走[托管安装](#安装-claude)
那条下载 + 校验。旧版本改名留底为 `claude.exe.old.<时间>` 并单独加上执行锁，下次安装时清掉。

QB Gate 自身更新同样保持安全默认：当前 GitHub Releases 仓库尚未配置，启动时不联网检查。
仓库发布后将只接受签名的 Tauri 更新包，并在设置页提供一键更新入口。

两个坑已经填上：版本号统一取**前三段**再比（本机文件属性是 `2.1.258.0`，
渠道返回 `2.1.258`，按四段比会误判成「渠道更旧」）；本机版本从**文件属性**读，
不去运行 `claude.exe`（它上面挂着 Deny ACE，运行不了）。

## 中文环境识别

十项加权指纹，满分 100，全部在本地计算、不上传任何数据。
逻辑改编自 [FuckClaude](https://github.com/LinXiaoTao/FuckClaude)（MIT）。

面板对「能不能修」如实标注。**中文字体那 14 分修不掉**——
删掉系统中文字体会让中文显示整体崩坏，代价远大于收益，
所以面板只报告，不提供「修复」按钮，也不假装能修。

台湾（`Asia/Taipei`、`zh-TW`）是 Anthropic 完整支持的地区，**故意不计分**；
港澳属受限地区，保留部分风险分。

---

## 构建

需要 Node 18+ 与 Rust 工具链。

```bash
npm install
npm run tauri dev      # 开发
npm run tauri build    # 出安装包
```

没装 Rust 的话：

```bash
winget install Rustlang.Rustup
```

Rust 侧单测（不联网、不动真 ACL、不碰真进程）：

```bash
cd src-tauri && cargo test
```

---

## 桌面快捷方式

```powershell
.\scripts\setup-desktop.ps1            # 预演，什么都不改
.\scripts\setup-desktop.ps1 -Apply -SkipBackup   # 只建快捷方式
.\scripts\setup-desktop.ps1 -Apply     # 顺便把旧按钮收进备份文件夹
```

旧按钮是**移动**不是删除，落在桌面的 `old-claude-buttons-backup-<日期>\`，
随时能拖回来。清单是写死的 7 个，**不按通配符扫桌面** ——
那会误伤 `Turb GPT 一键开关.lnk` 这类同样带「一键」字样的无关项。

> **已知问题：安装器自己建的桌面快捷方式在中文路径下是坏的。**
> `WScript.Shell` 这个 COM 对象存不了带非 ASCII 字符的路径，
> 中文桌面（`…\OneDrive\桌面\`）会被它变成 `??`，存出来的 `.lnk` 目标是空的。
> 上面那个脚本绕开了这一点：先在纯 ASCII 的临时路径建好，再用 `Copy-Item` 搬过去。
> 如果你是从安装器装的、桌面图标点不开，跑一次这个脚本即可。
>
> 另：`.ps1` 必须存成 **UTF-8 with BOM**。Windows PowerShell 5.1 没有 BOM
> 就按 ANSI 读，中文注释会变乱码并直接引发语法错误。

## 安装 Claude

v0.9.0 起，**Claude Code 和 Codex 由面板装进它自己管的目录**，桌面端照官方位置装：

| 软件 | 装到 | 从哪下 | 校验（任何一条不过就不装） |
|---|---|---|---|
| Claude Code | `<托管目录>\claude-code\claude.exe` | `downloads.claude.ai/claude-code-releases`（官方安装脚本用的同一处） | 官方 `manifest.json` 里的 SHA-256 + Authenticode 主体含 Anthropic |
| Codex | `<托管目录>\codex\codex.exe`，连同压缩包里的两个辅助程序 | GitHub `openai/codex` 的最新 Release，下载地址钉死在这个仓库下 | Release 给的 SHA-256 摘要 + Authenticode 主体含 OpenAI |
| Claude 桌面端 | 官方位置 `%LOCALAPPDATA%\AnthropicClaude`，装完把实际位置记进日志 | `winget install --id Anthropic.Claude -e` | winget 自己校验；装完再核一次签名，**核不过只警告** |

**为什么接管目录。** 「Claude 装在哪」每台机器都不一样，上面那张锁表就有十几种落点。
面板靠扫描去找，漏一处就是「面板找不到软件」，或者漏锁一份。面板自己装的那份放在面板指定的位置，
另有一份安装记录（`<托管目录>\installs.json`：版本、SHA-256、来源、时间），启动永远先用它。

**为什么不用 winget 的 `--location`。** 这两个包在 winget 上都是 portable，理论上能指定位置。但实测
winget 源里的版本落后（2026-09-11：Claude Code 2.1.263 对官方 2.1.268，Codex 0.146.1 对 0.154.0）；
它下的就是同一个官方地址，官方连不上时它也连不上，当兜底没意义；它还会在托管目录之外留下
`WinGet\Links` 的 shim 和卸载登记。所以直接从官方源下，自己核对官方给的哈希与签名。
桌面端是例外：它的官方安装器把位置写死了，还会自己在那里更新，面板改不了它的落点，也就不假装改。

### 托管目录

默认 `%LOCALAPPDATA%\ClaudeIpGate\apps`。安装确认框里就能改；装完也能在环境页或「设置 → 托管安装目录」
里改，**已经装好的会一键搬过去**。

- **选目录时当场实测锁不锁得住。** 在目录里放一个探针文件：加 Deny、查、解开、删掉。
  exFAT / FAT32 盘没有权限系统，Program Files、网络盘加不上 Deny —— 这些当场拒绝并说明原因，
  不会「装进去之后才发现锁不上」。桌面端的目录、`~\.claude`、`~\.local`、面板自己的安装目录也不收。
- **迁移要么全搬过去，要么全留在原处。** 托管的 Claude Code 正在跑就先关闭全部 Claude；
  还有别的程序从旧目录运行（比如 Codex）就一个文件都不动，列出来让你先关。
  中途哪个搬不动，已经搬过去的搬回来 —— 两边各留一半是最坏的结果：设置指着一边，
  另一边的 exe 面板不认、也没人锁。搬完按新位置重新上锁。

### 外部副本

托管那份装好之后，本机别的 Claude Code / Codex 副本就多余了。环境页能扫出来，由你决定：

- **彻底清掉**：npm / winget / scoop 装的用它们自己的卸载命令（连启动器、`WinGet\Links` 的 shim、
  卸载登记一起走，npm 中断留下的临时目录也删）；官方安装器那份、它的版本库和下载缓存、PATH 上的、
  升级残留直接删文件，删之前逐个再核一次签名，不是 Anthropic / OpenAI 签的一律不删，
  删完顺手删掉变空的上级目录；文件早就不在、卸载登记还挂着的也一并删掉。
  清 Claude Code 之前会先关闭全部 Claude；Codex 要你自己先关（面板不按进程名杀进程）。
- **保留**：它们照样被执行锁锁住，跟以前一样。

**永远不碰**：账户凭证与配置（`~\.claude`、`claude-profile-*`、`~\.codex`）、桌面端和它自带的副本、
编辑器扩展里的副本、`~\.local\bin` 目录本身和 PATH（uv、pipx 等别的工具也往那里装东西）。
托管那份没装好之前不让清 —— 不然清完这台机器上一份都没有了。

### 完整性

早期版本自己钉 SHA-256（`installers.lock.json` + `scripts/pin-hash.mjs`），现已下线：哈希得手工钉，
钉不上安装按钮就永远是灰的 —— 而它确实一直是灰的。现在用的是**官方发布时一起给出的哈希**
（Claude Code 的 `manifest.json`、GitHub Release 的资产摘要），再加 Authenticode 签名。
托管安装里这两条都是硬条件：哈希对不上、签名不对或者读不出签名，都删掉下载的文件、不装，
原来那份原样留着。GitHub 偶尔不给某个资产的摘要，那时 Codex 只剩签名这一条可核，日志里会写明。

装完必然重新枚举副本并**重新上锁**，重锁失败会报错，不会默默放过。

---

## 账户槽位

一个槽位是一个独立的登录目录。面板启动 Claude Code 时把 `CLAUDE_CONFIG_DIR`
设成**当前槽位的具体目录**，所以每个槽位有自己的一套登录、设置和历史。

| 谁在用 | 「当前」由谁决定 | 槽位本体 |
|---|---|---|
| Claude Code（从面板启动的） | `%LOCALAPPDATA%\ClaudeIpGate\claude-profile` 联结点 | `%LOCALAPPDATA%\ClaudeIpGate\claude-profile-<标签>` |
| 酒馆桥接（装了才有） | `%LOCALAPPDATA%\ClaudeTavernBridge\claude-profile` 联结点 | **同一个目录** |
| 桌面端（可选） | `%APPDATA%\Claude` 联结点 | `%APPDATA%\Claude-<标签>` |

- **新建槽位**是空的，不从任何地方复制凭证 —— 复制凭证就是又造一份同样的
  refresh token，两边都用起来之后一边刷新就可能让另一份作废。在新槽位里登录一次即可。
  一个槽位都没有时，Claude Code 用它自己的默认目录 `~\.claude`，面板不替你换。
- **切换 = 先关闭全部 Claude，再换指向，不自动启动任何东西**（v0.9.0）。桌面端、所有 Claude Code 会话、
  酒馆桥接都关掉（走[一键关闭](#一键关闭)那套证据，未保存的对话会丢），然后三处指向一起换，
  要么全换要么都不换；之后要用哪个，自己回总览点。清过场就不会有「一个跑着的桌面端脚下换资料目录」，
  也不会有进程还连着旧账户。托盘菜单里点另一个账户也是直接这样切，不弹框
  （菜单顶上写着「点了会先关闭全部 Claude」）；托盘没有复选框，桌面端按对话框的默认值走：
  这个槽位有自己的桌面端资料才跟着切。**清场失败就不切**，槽位一点不动。
- 恢复档案 / 快照时，只有目标记录的账户跟当前不同才会先清场；同一个账户不关任何东西。
- 桌面端第一次交给面板管时，它原来那份资料会存成「切走之前那个槽位」的桌面端资料，
  切回去原样回来。
- 早期版本从旧脚本迁移时是**复制**槽位的，留下两份相同的凭证、两个同时「激活」的账户。
  现在列槽位时会把它们合成一份：旧目录改名留底（不删），原处换成指向面板槽位的联结点。

---

## 合规边界

以下四条是硬约束，任何改动都不得破坏：

1. **不读取任何限流 / 429 / 额度状态**，不存在「用完自动换号」的路径
2. 账户切换只能由人手动触发，无定时器、无 watchdog、无自动调用点
3. 任意时刻只有一个账户激活
4. **所有账户必须是使用者本人拥有的**

总览的账户卡显示每个槽位的**套餐、计费方式、登录状态与凭证剩余天数**，
用量仍然只能去 Claude 官方 `Settings → Usage` 页面看。

**套餐是读文件读出来的，不是查接口查出来的。** 每个槽位目录里都有一份
`.claude.json`，其中的 `oauthAccount` 是官方客户端自己写下的档案缓存
（套餐、计费方式、上次刷新时间）。QB Gate 只读这个本地文件 ——
不发任何网络请求，不读用量、额度、429 或 OAuth 内部接口，
也**不显示** `organizationRateLimitTier` / `userRateLimitTier`
这两个字段（名字里带 rateLimit，展示它们会让第 1 条边界变得可疑，
而它们对用户几乎没有价值）。

因为每个槽位各有一份，所以**不用切过去就能看到每个槽位的套餐** ——
这恰恰减少了「为了看一眼而切来切去」的操作，与第 2 条不冲突。

第 1 条边界的原意是**不存在「用完自动换号」的路径**，这条继续成立：
套餐只做展示，不接任何自动切换逻辑，切换依然只能由人在界面上点。

### Codex 与门禁

Codex **默认不在 IP 门禁的管辖范围内**，可以在「设置 → 门禁范围」里打开。

打开之后 `codex` 会跟 `claude.exe` 一样被加上 Deny ExecuteFile：出口 IP 不在
白名单时命令会被系统直接拒绝执行，要跑得走总览的「启动 Codex」受控入口。

默认关闭是刻意的 —— 这个开关会让一个正在用的命令突然跑不起来，
必须由使用者自己决定什么时候打开。关掉时面板会主动把 Codex 身上的锁摘掉，
否则它已经不在门禁清单里，往后谁都不会再碰它。

面板托管的 Codex（`<托管目录>\codex\codex.exe`，见[安装 Claude](#安装-claude)）在 Codex 候选里排第一：
「启动 Codex」先用它，门禁打开时也先锁它。别处的 Codex 副本同样照锁。

已知边界：npm 全局装出来的是 `codex.cmd` 批处理，给它加执行锁能挡住
`codex` 这个命令本身，但挡不住直接去调它内部的 node 脚本 ——
跟桌面端 `app-*` 那个缺口是同一类，要彻底堵死需要 AppLocker / WDAC。

卸载清理功能的定位是：修复损坏安装、移交机器、清除本人数据。
**不为规避封禁或用量限制而设计**——Anthropic 政策禁止为规避限制而
创建或轮换多个账户。

另外，关于「Claude Code 用 Unicode 隐写术编码中国用户特征」的说法：
这是**第三方逆向分析主张，本项目未做独立验证**，界面上引用时标注了来源与
未证实状态。其触发条件据描述是走中转端点，官方 OAuth 直连不在该描述范围内。

---

## 来源与致谢

检测部分大量参考已有开源实现，逐条列在 [ATTRIBUTION.md](ATTRIBUTION.md)：
抄了代码的、只用了公开接口的、以及看过但没采用的，都分开说明。

主要来源：[FuckClaude](https://github.com/LinXiaoTao/FuckClaude)（MIT，中文环境识别）、
[cc-switch](https://github.com/farion1231/cc-switch)（MIT，中转站配置形态）、
[bash.ws](https://bash.ws/dnsleak)（DNS 回显接口）、
[ippure.com](https://ippure.com/)（纯净度口径与公开接口）。

## 许可

**GNU General Public License v3.0 or later**（GPL-3.0-or-later），全文见 [LICENSE](LICENSE)。

```
Copyright (C) 2026 QB Gate contributors

本程序是自由软件：你可以依据自由软件基金会发布的 GNU 通用公共许可证
（第 3 版，或你选择的任一更新版本）的条款，重新发布和／或修改它。

发布本程序是希望它能派上用场，但**不作任何担保**；甚至不含对
适销性或特定用途适用性的默示担保。详见 GNU 通用公共许可证。

你应当已随本程序收到一份 GNU 通用公共许可证的副本。
如果没有，请见 <https://www.gnu.org/licenses/>。
```

一句话版本：**你可以自由使用、修改、再分发；但把改过的版本分发出去时，
也必须以 GPL-3.0 开放源码。**

依赖许可的相容性：上游那几个来源（FuckClaude、cc-switch、Tailwind、
lucide-react）都是 MIT，MIT 与 GPL-3.0 相容，其版权声明按要求保留在
对应文件里，逐条见 [ATTRIBUTION.md](ATTRIBUTION.md)。
SillyTavern 是 AGPL-3.0，本项目只把它当独立进程启动，未复制未修改未链接。
