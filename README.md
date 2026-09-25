# QB Gate

**简体中文** | [English](README.en.md)

工作需要经常换 IP 的人，常会碰到一件事：网络出口一变，电脑上开着的 AI 软件并不会停下来，而是继续从一个你没确认过的出口发请求。QB Gate 替你盯着这件事 —— **出口 IP 在你认可的名单里，本机的 Claude、GPT（Codex）和反重力才打得开；用着用着 IP 变了或者查不到，它会立刻把这些软件关掉。**

[![Windows 一键下载安装包](https://img.shields.io/badge/Windows-%E4%B8%80%E9%94%AE%E4%B8%8B%E8%BD%BD%E5%AE%89%E8%A3%85%E5%8C%85-0078D6?style=for-the-badge)](https://github.com/smithtaylor7748-ops/qb-gate/releases/latest/download/QB-Gate-Windows-x64-setup.exe)

- 适用于 Windows 10 / 11 x64。
- 安装包还没有做代码签名，Windows 可能弹出 SmartScreen 提示；下载后可以对照 [Releases](https://github.com/smithtaylor7748-ops/qb-gate/releases/latest) 页面里的 `SHA256SUMS.txt` 核对哈希。
- 从 0.25.3 起，打开面板时会看一眼 GitHub 上有没有新版本，有就弹窗提醒，点「一键更新」就能装好（见下面「怎么更新」）。**装着 v0.24.8 或更早版本的，需要用上面的按钮手动下载安装这一次** —— 老版本里还没有这个功能。

> ⚠ **Windows 安全中心可能把它当成病毒隔离**（报 `Trojan:Win32/Bearfoos.A!ml` 之类）。这是**误报**：名字末尾的 `!ml` 表示那是机器学习猜出来的分，不是真的查到了病毒。这个面板要做的事 —— 给别的程序加执行锁、按 IP 关进程、下载官方安装包再运行 —— 在行为上跟木马很像，再加上没签名、每一版都是新文件，就容易被判成病毒。**怎么恢复、怎么加排除项、怎么向微软申诉误报**，都写在 [docs/ANTIVIRUS.zh-CN.md](docs/ANTIVIRUS.zh-CN.md)；恢复之前请先按上面那句核对 SHA-256。

|                                                             |                                                           |
| ----------------------------------------------------------- | --------------------------------------------------------- |
| ![Claude 账户页](docs/screenshots/overview.png)              | ![反重力账户页](docs/screenshots/antigravity-light.png)   |
| **官方账户 · Claude**：评分、账户槽位、启动与用量            | **官方账户 · 反重力**：Hub / IDE、槽位与联网额度       |
| ![GPT 账户页](docs/screenshots/codex-accounts-light.png)     | ![用量明细](docs/screenshots/usage.png)                   |
| **官方账户 · GPT**：Codex 桌面端账户与额度                   | **用量明细**：今天 / 7 天 / 30 天值多少美元            |
| ![软件](docs/screenshots/software.png)                       | ![环境体检](docs/screenshots/checkup-repair-light.png)    |
| **软件**：安装、升级、回滚与完全卸载                         | **环境体检**：五项评分，点开就地处理                      |

截图里全是演示数据，不是任何真实机器的状态。下文标 🆕 的，是比上一个公开版本 v0.25.3 新增或大改的地方。

## 它怎么帮你

### IP 门禁（核心）

- 先把你认可的出口 IP 加进白名单（也可以再加国家规则）。出口不在名单里，被管着的 AI 软件就启动不了。
- 放行之后，看门狗每 5 秒查一次出口：IP 变了、不合格或者查不到，立刻上锁，并关掉由面板启动的会话，不给宽限。
- 管着哪些软件：Claude（Claude Code 和桌面端）一直管；GPT（Codex）和反重力（Hub 与 IDE）默认也管，不想让它们受限，可以在「IP 锁」里移出。
- 会话内门禁（每次请求前再验一次出口）默认关，需要时自己打开。
- 关掉面板窗口只是收进托盘，门禁照样有人看着；从托盘真正退出面板时，会把锁重新锁上。

### 官方账户：Claude、GPT、反重力三页

三页长一个样：上面是综合评分，左边是账户槽位，右边是启动按钮和用量。

- **账户槽位**：同一个软件可以存好几个你本人的登录账户，显示成「邮箱 - 名字」。切换只能你手动点，同一时间只有一个在用。
- **启动与一键关闭**：启动之前先验出口 IP；「一键关闭」只关确实属于这些软件的进程，不按进程名乱杀。
- **额度**：Claude 的 5 小时 / 7 天额度只读本机文件。GPT 和反重力的额度平时也只看本机记录，点那一行的刷新图标才向官方问一次（一次只问一个账户，没有定时器）。读到的数只拿来显示，不会自动换号。
- **用量明细**：看今天、近 7 天、近 30 天的用量「值多少美元」—— 按官方 API 价折算，**不是账单**，订阅也不按这个收费；另有按天的柱状图、按模型、按账户和最近的请求。
- **GPT（Codex 桌面端）**：启动或切换账户时只关面板自己打开的那个 Codex，别处开着的不碰，不会「突然弹出第二个窗口」。登录令牌到点时如实告诉你「打开一下桌面端它会自己续上」，不误报成登录失效。
- **反重力（Google Antigravity）**：Hub 和 IDE 都能从这里启动、关闭；IDE 可以存多个登录槽位（一个 Google 账户一条，登录在 IDE 自己的窗口里做）；能看账户档位、AI 积分，以及 Claude、Gemini 两组的 5 小时 / 每周额度；还能看本机用掉了多少 token。反重力本身不能走中转站。
- 🆕 **一键汉化**：GPT 页「Codex 桌面端」卡上点「汉化」，把 GPT 设成中文界面 —— 写的是它自己设置里的语言那一项（跟你在它的设置里选中文是同一个值），不改它的任何程序文件；设了仍是英文，是 OpenAI 还没对你的账户开放中文，面板会照实说。Claude 页「启动」卡上的「汉化」装的是下面「扩展」里那个中文界面插件。

### 环境体检

- 一键把五项查一遍并打分：IP 纯净度、DNS 泄露、中文环境、IP 锁、出口一致性。点任何一格，就能在小窗里看明细、就地处理。IP 纯净度里的「禁用本机 IPv6」默认开。
- 还会查这些：
  - 不带任何账号信息，测 Anthropic 的服务在你这里通不通 —— 被地区拦截能直接看出来；
  - 看 claude.ai 解析出来的地址有没有被污染；
  - 认得出 PAC、TUN 这两种代理方式，不会误报「系统代理没开」；
  - 查 IPv6 真正从哪个国家出去，而不是只看网卡开没开 IPv6；
  - 「中文环境」可以用你平时的默认浏览器来测；
  - DNS 泄露只看连着的网卡，只连 Wi-Fi 不会被扣分；
  - 几项关键问题（比如 API 被拦、IPv6 出口在别的国家）只要有一项不过，总评最多给到「偏差」。

### 软件：装、升、卸

- **托管安装**：Claude Code、Codex CLI 从官方地址下载，核对 SHA-256 和数字签名之后装好，装完自动上锁。
- **升级与回滚**：可选最新版或稳定版，旧版本留 3 份，随时退回去。
- **Codex 桌面端不用打开微软商店也能装**：直接从微软官方下载，核对哈希和 OpenAI 的签名再装。
- **反重力、Gemini CLI 一键安装**：反重力从 Google 自己的地址取官方安装包，验过签名再装（面板不分发、不改包）；Gemini CLI 走官方 npm 包。
- **完全卸载**：先把这个软件在电脑上的所有落点列给你看，手动输入「卸载」才动手；Codex 桌面端、反重力、Gemini CLI 也能完全卸载。还附了给其他 AI 用的卸载提示词。
- **Google Chrome**：隐私检查（只看不改）、改系统代理、浏览器出站锁（都是你点了才做、可以撤销），以及完全重装（会删掉全部浏览器数据）。

### 扩展

- **酒馆（SillyTavern）三条桥**：Claude（用你自己的 bridge.py）、GPT（驱动官方 Codex CLI）、Gemini（驱动官方 Gemini CLI）。端口和模型都在面板里设（酒馆磁贴右上角那颗按钮），Claude 桥的设置和调用日志也搬进了面板，还会列出「还缺什么」。酒馆启动慢（它每次先装一遍依赖）不会再被面板误杀。
- **反重力 · 汉化与审批**：界面汉化、自动审批、高危操作拦截（规则可以自己改）。只往界面里注入脚本，不改反重力的任何文件。
- 🆕 **Claude 桌面端 · 中文界面**：接入开源项目 [javaht/claude-desktop-zh-cn](https://github.com/javaht/claude-desktop-zh-cn)（MIT），只用它的安全模式；改之前、改之后都核对 Claude 程序文件的哈希和数字签名，变了就自动还原；上游出了新版，打开这个窗口时会提示。这是非官方修改，应用或还原会关掉 Claude 桌面端（包括 Code 页里的会话）；用之前请先看[免责声明](DISCLAIMER.md)。
- **MCP、Skills、配置模板**：导入并预览差异之后，再装进指定的环境；连接测试由你手动发起。

### 订阅指引

- 讲清楚 Claude、ChatGPT 各档订阅值不值、怎么避坑。各套餐额度取 linux.do 帖子里网友估算的中间值；「等效倍率」按 1 美元 = 7 元折算，跟中转站放在同一把尺子上比。
- 🆕 倍率计算器能填中转站那条线路的倍率：站内额度是按这个倍率扣的，原来没乘这一项，会把中转算贵好几倍。

### 中转站

- 智能调度（内测中）：界面里已经能看到，但目前还不能正常使用。
- 🆕 倍率按 linux.do「中转站百科」帖的算法改对了：New API 站点公布的倍率其实是单价（1 = 每百万 token 2 美元），现在换成单价再跟官方价比；原来每条照官方价收费的线路都挂着的「翻倍 ×5」是误报，改成「输出加价」；读得到站点公布的分组倍率；每个站点能填充值比例（1 美元额度 = 几元），不同站点比便宜时先折成同一种钱。

### 设置

- **常规**：外观主题；网络恢复后自动重新放行；关闭 Claude Code 的非必要遥测；启动时按出口 IP 对齐系统时区和区域格式（默认开）、显示语言（默认关）。
- **软件更新**：启动时检查新版本（默认开，可以关）、手动「检查更新」。
- **备份与恢复**、**帮助与来源**、**高级维护**（换托管目录、恢复系统时区）。

## 怎么更新

- **装着 0.25.3 或更新的版本**：打开面板时，如果 GitHub 上有新版本，会弹窗告诉你这一版改了什么。点「一键更新」，面板会下载安装包、对照同一个发布里的 `SHA256SUMS.txt` 核对无误，然后退出并自动安装，装完自己重新打开。
- **更新时面板会先退出**：由面板启动的 Claude 桌面端、对话、酒馆会一起关掉，先把手头的活存好。暂时不想装，点「以后再说」或「跳过这个版本」；也可以在设置里关掉启动时检查。
- **装着 v0.24.8 或更早的版本**：老版本里没有这个功能，请用本页顶上的下载按钮手动装一次新版。

## 合规边界（请先读）

- **不会自动换号**：没有「额度用完、被限流、报 429 就自动切账户」的功能。账户只能你手动切，**同一时间只有一个账户在用**。
- **账户必须是你本人合法拥有的。**
- **额度只拿来显示**：Claude 只读本机文件；GPT、Gemini CLI 和反重力只在你点刷新图标时问一次官方，不按额度做任何自动决定。
- **不改设备指纹、不伪装身份、不带代理或 VPN**：它只管住你本机的 AI 软件在什么网络出口下运行。

## 免责声明

使用前请完整阅读 [免责声明 DISCLAIMER.md](DISCLAIMER.md)。要点：

- 非官方项目，与 Anthropic、OpenAI、Google 等任何服务商都没有隶属、合作或背书关系。
- **不对账户状态作任何承诺。** 本软件只管住你本机的 AI 软件，不改写设备指纹、不伪装账户身份，不为绕过、规避任何服务商的安全措施、合规要求、地区限制或封禁而设计，也做不到。
- 不含代理、VPN 或翻墙功能。网络接入是否合法，由使用者自行负责。
- 只能用于你本人合法拥有的账户，并遵守所在地法律与各服务商条款。
- 部分功能会修改系统设置（IPv6、时区与区域格式、防火墙规则、系统代理等）或永久删除数据，执行前请看清提示。
- 面板会联网的地方都列在 DISCLAIMER 第 7 节 —— 包括启动时检查更新：只问本项目 GitHub 发布页上的一个小文件，不带任何账户信息，可以在设置里关掉。

## QQ 群

「门禁值班室」：`1109462206`

使用问题、装不上、功能建议都可以来群里聊。贴日志和截图之前，先把出口 IP、账户邮箱、Token 打码。

## 许可证

本项目按 **AGPL-3.0-only** 发布，并依 AGPL 第 7 节附加了三条条款，完整条款见
[LICENSE](LICENSE) 与 [LICENSE-ADDITIONAL-TERMS.md](LICENSE-ADDITIONAL-TERMS.md)。

### 改了拿去发的人要做什么

AGPL 本身要求：附上许可证全文、保留版权声明、标明你改了什么、整个改版继续按 AGPL-3.0-only
发布并提供源码（把改版架成网络服务给别人用也算，§13）。附加条款在这之上再加三条，
接收者不能删（AGPL §7）：

1. **保留署名和仓库链接。** 源码的 README（或等效顶层文件）和界面的「关于」页里必须原样保留：
   `基于 QB Gate，版权所有 (C) 2026 smithtaylor7748-ops。源码：https://github.com/smithtaylor7748-ops/qb-gate`
2. **改版必须标明是改版**，不得暗示由原作者发布或背书。
3. **不能叫「QB Gate」。** 名称和图标不在授权范围内，改版要换名字（署名里如实提到原名除外）。

另外，改版如果保留了「一键更新」，请把检查更新指向你自己的发布页（`crates/qb-install/src/install/self_update.rs` 里的 `REPO`）—— 不然你的用户会被「更新」回原版。

不想守 AGPL 的场景（并入闭源、不公开改动地对外提供服务），需要另行取得授权 ——
见 [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md)。

### 授权声明

**这一段才是正式声明**，`Cargo.toml` / `package.json` 里的 license 字段只是给包管理器
看的提示，不构成授权。注意措辞里**没有**「或任何更新版本」—— 本项目按 AGPL **第 3 版**
授权，不自动适用 FSF 将来发布的新版本（AGPL 第 14 节）。

```
QB Gate — Windows 本地 AI 客户端工作空间
Copyright (C) 2026 smithtaylor7748-ops

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published
by the Free Software Foundation, version 3, supplemented by the additional
terms permitted under section 7 of that license and set out in
LICENSE-ADDITIONAL-TERMS.md (preservation of attribution, marking of
modified versions, and no grant of the "QB Gate" name).

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
```

## 社区

本项目在 [LINUX DO](https://linux.do/) 社区进行开源推广，感谢社区佬友的交流、反馈与建议。
