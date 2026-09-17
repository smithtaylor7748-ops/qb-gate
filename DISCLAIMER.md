# 免责声明

**最后更新：2026-09-16｜适用版本：v0.22.6 及之后的版本**

请在下载、构建、安装或运行 QB Gate 之前**完整读完本文件**。下载、构建、安装或运行本项目，
即表示你已阅读、理解并接受以下全部条款；不接受其中任何一条，请不要使用本项目。

---

## 本软件的定位（先读这一节）

**为什么做它。** 有些工作需要频繁变动 IP。出口一变，电脑上还开着的 AI 软件不会跟着停下来，
会继续从一个你没确认过的网络出口发请求。QB Gate 是一个 Windows 本地工具，用来**管住你自己
电脑上的 AI 软件**：出口 IP 不在你设定的白名单里时，不让它启动；运行中出口 IP 变了或查不到，
立即上锁并关闭受门禁管理的会话。围绕这件事，它还提供网络环境自查、本人账户槽位管理，
以及相关软件的安装、升级与卸载。

**它是什么。** 一个在你本机运行的管理与自查工具。所有判断都在本机完成；
维护者没有服务器，收不到你的任何数据。

**它不是什么。**

- **不是绕过服务商限制的工具。** 它不改写设备指纹（UUID、主机名、MAC、machine-id 等），不伪装
  账户身份，**不为绕过、规避任何服务商的安全措施、合规要求、地区限制、用量限制或封禁而设计，
  也无法达到这些目的**。
- **不是代理、VPN 或翻墙工具。** 它不提供任何你本来没有的网络通路，见第 5 节。
- **不是任何服务商的产品或合作项目**，见第 1 节。
- **不是安全保证。** 它不能保证你的账户不被限制、暂停或封禁，见第 3 节。

**使用前提。** 下面任何一条不满足，请不要使用本软件：

1. 你所在的国家或地区、你的使用方式，都在各服务商允许的范围内（包括服务商公布的支持地区）；
2. 你使用的全部账户都是**你本人合法拥有**的；
3. 你的网络接入方式符合你所在地的法律法规；
4. 你不打算把本软件用于第 11 节列出的任何用途。

---

## 0. 本文件不是法律意见

本项目由个人维护。本文件、`README.md` 以及 `docs/` 下的任何风险说明，都只是维护者
自己的理解与记录，**不是法律意见，可能过时，也可能有错**。你所在司法辖区的法律、
你与各服务商之间的合同，以及这两者如何适用于你的具体用法，都需要你自己核实，
必要时咨询有资质的专业人士。

需要特别说明的是：

> **「开源协议」「本地运行」「开源免费」「仅供学习交流」「全程手动操作」
> 都不是统一的免责条件。**

它们既不能让违反服务条款的行为变得合规，也不能免除使用者在所在地法律下的责任。
项目自述的「合规边界」是维护者的设计约束，**不是外部法律结论**。
**本文件同样不能保证任何人在任何司法辖区都不承担法律责任**；它的作用是如实说明
本软件做什么、不做什么，以及风险由谁承担。

---

## 1. 非官方项目，与任何服务商都没有关系

- QB Gate 是**独立的第三方工具**，与 Anthropic PBC、OpenAI、Google、Microsoft 以及
  文中出现的任何公司**没有**任何隶属、合作、赞助、代理、认证或背书关系，
  也**未经**其审阅或许可。
- `Claude`、`Claude Code`、`Anthropic` 是 Anthropic PBC 的商标；`Codex`、`ChatGPT`、
  `OpenAI` 是 OpenAI 的商标；`Google Chrome` 是 Google LLC 的商标；`Windows`、
  `Microsoft Store` 是 Microsoft 的商标；`SillyTavern`、`DeepSeek`、`Kimi`、`智谱`、
  `IPRoyal` 等名称归各自权利人所有。本项目仅为说明兼容性而提及这些名称，
  **不主张任何权利，也不代表获得授权**。
- **本项目名称（QB Gate）不含任何他人商标。** 文档与界面中出现的
  `Claude` / `Codex` 等名称均为**叙述性使用** —— 用于说明本工具与哪些软件配合，
  不作为产品标识，也不暗示合作。维护者已知悉 Anthropic《商标指引》
  对未经许可的品牌使用与暗示合作的限制。若权利人仍认为不当，
  维护者将配合更名或下架，见本文第 10 节。
- 本项目**不分发、不镜像、不修改** Claude、Claude Code、Codex、Google Chrome 或
  SillyTavern 的任何二进制或源代码。安装一律走官方渠道（官方下载源、GitHub 上的
  官方发布页、winget）；Claude Code 与 Codex 另外核对官方给出的 SHA-256 与数字签名，
  核对不过就不装。

---

## 2. 不提供任何担保

本软件以「**现状**」（AS IS）提供，**不附带任何形式的明示或默示担保**，
包括但不限于对适销性、特定用途适用性、不侵权、准确性、可用性、
不中断或无错误的担保。

**在任何情况下，作者与版权持有人均不对任何索赔、损害或其他责任负责**，
无论该责任产生于合同、侵权或其他事由，也无论其是否由本软件或本软件的使用
或其他交易引起、与之相关。

本条为 `LICENSE`（GNU AGPL v3）中免责条款第 15、16 节的中文表述，**如有歧义以 `LICENSE` 英文原文为准**。

---

## 3. 检测结果不是结论，本项目不对账户状态作任何承诺

面板里的综合评分（IP 纯净度、DNS 泄露、中文环境、IP 锁、出口一致性）、指定 IP 查询、
账户可用性检测等，都是**基于公开接口与本机信息的参考性判断**，具有以下本质限制：

- **检测结果不等于任何服务商的实际判定。** 服务商的内部判定规则不公开、随时变化，
  面板无从得知，也无法复现。
- **IP 门禁只能减少「IP 变了还在用」这一种情形。** 它消除不了其它任何风险因素
  （账户本身、支付方式、使用行为、服务端策略等），更不能保证账户不被限制或封禁。
- **「纯净度 ≤ 5%」「原生 IP」「住宅 IP」不是账户安全承诺**，只是本项目采用的自测参考阈值。
  全部达标仍可能被限制或封禁；不达标也未必会。
- **「去除中文环境特征」不会降低任何封禁概率**，本项目从未做过这样的声明。
  启动时把系统时区、区域格式对齐到出口 IP 归属地，同样**不会降低任何封禁概率**，
  也不改变你的真实所在地。Anthropic 公开说明会通过 IP 等信息推断大致位置，
  但**从未公布**「语言/时区与 IP 同区即安全」这类规则。任何据此得出的结论都是推测。
- 面板自查的数据来自 `my.ippure.com`、`ipinfo.io`、Cloudflare、`api.ipquery.io`、
  `bash.ws` 等**第三方公开接口**，其准确性、可用性与持续性由对方决定，本项目不保证。
  取不到的字段一律**如实显示「未知」，绝不猜成通过**。
- 关于「Claude Code 以 Unicode 隐写编码用户特征」的说法：这是**第三方逆向分析主张，
  本项目未做独立验证**，界面上已标注来源与未证实状态，**不应**被当作事实。

**任何账号被限制、封禁、扣费失败或数据损失，均与本项目无关，后果由使用者自行承担。**

---

## 4. 账户、服务条款与使用者责任

使用本项目**不能**免除你对各服务商服务条款的遵守义务。使用前请自行阅读并遵守
Anthropic《消费者条款》《使用政策》及其公布的支持国家和地区、Claude Code 的法律与合规文档，
以及 OpenAI、Google、Microsoft 等你所使用的各方条款。
**服务商不向你所在地区提供服务的，不得借助本软件使用该服务。**

本项目的设计约束（`README.md` 的「免责声明」一节同样列出）：

1. **不发任何网络请求去查额度，不调用 OAuth 内部接口**；额度与用量只读官方客户端
   自己写在本机的文件，且仅用于显示，代码中不存在「用完自动换号」的路径；
2. 账户切换**只能由人手动触发**，无定时器、无看门狗、无自动调用点；
3. 任意时刻只有一个账户处于激活状态；
4. **所有账户必须是使用者本人合法拥有的**。

请注意，这些是**维护者的自我约束，不是合规背书**。特别地：

- **Anthropic 的政策禁止为规避限制或封禁而创建、轮换多个账户。**
  多槽位功能（包括 Codex 桌面端的槽位）的定位是：在同一台机器上分开保存
  **你本人拥有的**不同账户的配置。
  **将其用于规避限流、规避封禁、共享账号或账号买卖，属于使用者自身的违规行为，
  本项目明确不支持，也不承担任何责任。**
- **不得将本项目用于任何账号代充、代开、租借、转售或商业中介活动。**
- **账户可用性检测**（账户详情里手动点击才执行）会用该槽位在本机保存的登录令牌，
  向 Anthropic 官方公开的模型列表接口发一次最小请求，只问「令牌是否仍被接受」：
  不调用模型、不产生 token、不刷新也不保存任何凭证；本地令牌已过期时不发请求。
  **由第三方程序携带你的官方登录令牌发出请求是否符合服务商条款，请你自行判断；
  不确定就不要使用这一项。**
- 删除槽位、完全卸载会删除本机的登录数据，之后需要重新登录。
  卸载与清理功能的定位是修复损坏安装、移交机器、清除本人数据，
  **不为规避封禁而设计，也无法达到该目的**。

---

## 4.5 门禁的代价：执行锁、看门狗、会话内门禁与国家白名单

### 执行锁与看门狗

- 执行锁给 Claude 程序（以及你选择纳入门禁的 Codex）的每一份副本加 NTFS
  `Deny ExecuteFile` 规则。**上锁后双击程序会被系统拒绝执行**，这是设计如此，
  必须走面板入口。
- 门禁放行期间，看门狗每 15–20 秒复核一次出口 IP。**出口 IP 不在白名单、国家不合格、
  或者查不到出口 IP 时，第一轮就上锁并关闭受门禁管理的 Claude 会话**，
  Claude Code 与桌面端都一样，没有宽限期。这是使用者明确选定的严格档
  （「宁可错杀不可放过」）。
- **代价必须说清楚：** VPN 重连、DNS 抖动、切换节点、或者几个 IP 查询服务同时限流，
  都会**直接关掉你正在用的 Claude Code 和桌面端，未保存的对话会丢**。
  「网络恢复后重新放行」（默认开启）只会在复核通过后恢复放行，
  **不会**重新启动已经被关掉的会话。
- 把误杀压到最低的办法是三源并发探测（ippure / Cloudflare / ipinfo，任一家答上来
  就算查到）—— 所以「查不到」意味着三家全挂，而不是某一家抽风。
  但这只是降低概率，不是消除。

### 会话内门禁（装进 Claude Code 的 hook）

这一项**默认是关的**，打开之前请把代价读完。

它在每次请求发出前验一遍门禁，判不过就拦下这一次请求。**它是 fail-closed 的**：

- 出口 IP 不在白名单 → 拦；
- 查不到出口 IP → 拦；
- **面板没在跑（门禁裁决超过 90 秒没刷新）→ 也拦**。

也就是说，启用之后，**不开面板就用不了 Claude Code**。这是「宁可错杀不可放过」
的直接后果，由使用者自己选定。

三条自救路径，任何一条都够把自己放出来：

1. 在面板里停用会话内门禁（面板自身不受 hook 影响，永远打得开）；
2. 被拦时命令行里会印出手动解除步骤；
3. 直接编辑当前槽位的 `settings.json`，删掉 `hooks` 里带 `_qb_gate` 标记的两段。

面板在**白名单为空时拒绝启用**这一项 —— 那等于每一次请求都会被拦。

### 国家白名单

这一项**默认不启用**。启用之后，出口 IP 归属的国家不在名单里，门禁一律判不合格：
看门狗会收进程、会话内门禁会拦请求、「加入当前 IP」也会被拒。

**代价：GeoIP 不准会误杀正在进行的会话。** 面板向三家（ippure / Cloudflare /
ipinfo）问国家，三家说的不一样时**按最严的算，判不合格**，冲突原文写进日志。
这意味着一次 GeoIP 数据打架就可能让你正在跑的会话被收掉 —— 日志里那一行会写清
是哪一家在胡说，但会话已经没了。

**名单为空 = 这一层不启用，不是全拒。** 这是刻意的：全拒会在使用者还没来得及
配置时就把他关在门外。

面板给的预设**不是任何服务商的官方完整清单**，面板也不知道那份清单。
它们只是省得手打的起手式，应当自行对照服务商公布的支持地区核对后增删。
**国家白名单只约束你自己的电脑，不代表你所在地区可以使用某项服务。**

---

## 5. 网络接入与所在地法律

本项目是一个**本地管理与自查工具**：它**不提供、不内置、不分发**任何翻墙、VPN、
代理或网络绕过功能，也不教授如何获取这类服务。面板中的 IP 与 DNS 检测，
只是读取并展示你**当前已有网络环境**的状态。

面板在 IP 不合格时给出的 IP 选购入口是**推广链接**（见第 8 节），列出不等于推荐。
是否购买、购买的服务在你所在地是否合法，由你自行判断。

### 5.1 关于「中转站」与「本机路由」

> **中转站功能（含智能调度）目前处于内测阶段：界面里已经出现，但尚不能正常使用。
> 请不要把它用于任何实际工作，也不要依赖它给出的任何结果。**
> 内测期间它的行为、数据格式与界面都可能随时改变或被移除。

- 本项目**不提供、不销售、不推荐、不担保**任何中转站或 API 服务。界面里的服务商预设
  只是官方接口地址的便捷填写，**列出不等于推荐**。
- 你填入的地址与 API Key 由你自行负责。**使用第三方中转站可能违反模型服务商关于
  API 转售、共享或访问方式的条款**；是否合规、是否合法、资金是否安全，
  由你自行判断并承担后果。
- 连接诊断、站点检验会真实调用你填写的接口，**可能产生费用**，只在你点击时执行。
  检验只检测、只如实报告 —— 不拉黑、不替你换站。
- 打开中转站页面时，面板会读取你已添加站点的账单与健康数据；智能调度开启后，
  面板在运行期间每约 60 秒读取一次。这些读取不调用模型，**面板关闭即停止**。

中转站功能里有一个只监听 `127.0.0.1` 的**本机 API 路由器**。它存在的理由只有
一个：让客户端把地址指向本机之后，**切换上游中转站时不必重启客户端**。

把它和「代理软件」分开说清楚 —— 它：

- 只转发**明确指向它自己**的那些 API 请求，目的地是**你自己填进去的**中转站地址；
- **它不**修改系统代理设置，**不**接管任何其他程序的流量，**不**做链式转发，
  自身**没有**「全局代理」「断网即停」「流量绑定」这类开关（面板另有几件
  会改动系统网络配置的功能，跟本机路由无关，见下面 §5.2）；
- **只绑回环地址**，不对局域网或公网开放。

也就是说，它是一个 API 路由器，**不是翻墙工具** —— 它不会让你访问到你的网络
本来访问不到的地方。你填进去的中转站地址能否连通、是否合法，仍然由你自己负责。

### 5.2 关于会修改系统网络配置的功能

下面几件事会改动系统或浏览器的网络配置。它们与「翻墙」是两回事 ——
它们不提供任何你本来没有的网络通路，只是**防止已有的通路绕开你设定的出口**：

| 功能 | 做什么 | 边界 |
|---|---|---|
| **禁用本机 IPv6**（0.22.1 起，**默认开启**） | 启动面板时关闭整机网卡的 IPv6 绑定（含隐藏、虚拟网卡），按需请求管理员授权 | 位于 IP 纯净度中；关闭开关时恢复各网卡原设置，退出面板后保留当前状态；失败或已移除网卡的原值会保留供重试恢复 |
| **浏览器出站锁** | 给你点名的那一个浏览器加 Windows 防火墙**出站**规则：只许走你指定的 VPN / TUN 接口，物理网卡直连一律拦掉 | 只按该浏览器的 exe 路径生效，**不碰任何其它程序**；只加出站规则，不改路由表、不做转发；面板里看得到当前规则，也能一键撤销 |
| **系统代理修改** | 在你点了「修」之后修改系统代理设置，改之前记录原值 | **不自动改、不在启动时改**，只在你当次点了才改；面板里能回滚到原值 |
| **浏览器策略**（WebRTC、安全 DNS） | 在你点击后，为当前 Windows 用户写入 Chrome 策略，限制 WebRTC 暴露本机地址、调整安全 DNS | 只写当前用户（HKCU），**不碰整机策略**；面板里能撤销 |

必须说清楚的代价：

- **禁用 IPv6 可能短暂断网，或影响依赖 IPv6 的 Windows 功能。** 它不等于移除 Windows 内部的
  IPv6 回环，也不承诺防泄露或账户安全。卸载面板之前如需恢复，请先关闭开关并核对状态。
- **出站锁的防火墙规则不随面板退出而消失。** 面板关掉、甚至卸载之后规则仍然在，
  浏览器可能因此上不了网。撤销入口在面板里，**卸载面板之前请先撤销**。
- **改系统代理可能当场断网。** 改错了、或者代理本身没起来，受影响的是整台机器上
  所有跟随系统代理的程序，不只是那一个浏览器。
- 这些功能都**不会**让你访问到你的网络本来访问不到的地方，也**不能**证明你的
  网络接入行为合法。

但必须明确：

- **国际联网的信道与接入方式在中国大陆有专门的行政法规要求。**
  本项目提供检测与配置工具的行为，**不能**证明、也不试图证明使用者的网络接入行为合法。
- **你的网络接入是否合法，由你自己负责。** 请自行确认你使用的接入方式符合
  你所在地的法律法规。
- 本项目**不对**任何网络服务商作出推荐性的合规判断。

---

## 6. 本程序会改变你的系统

**这不是一个只读的检测工具。** 以下操作会真实、且部分不可恢复地修改你的系统，
请在执行前确认你理解其代价：

| 功能 | 实际做了什么 | 可逆性 |
|---|---|---|
| **执行锁** | 给 `claude.exe`（以及你选择纳入的 `codex.exe`）的每一份副本加 NTFS `Deny ExecuteFile` 规则 | 可逆。但**上锁后双击程序会被系统拒绝执行**，这是设计如此；必须走面板入口 |
| **看门狗** | 出口 IP 不合格或查不到时，上锁并关闭受门禁管理的会话 | 会话被关时**未保存的对话会丢失**，没有宽限期 |
| **会话内门禁** | 在当前槽位的 `settings.json` 里写入两条带 `_qb_gate` 标记的 hook | 可逆（面板里停用，或手动删除这两条） |
| **一键关闭 / 切换账户** | 结束满足双重证据的 Claude 进程（切换账户前也会这样做），再切换槽位指向 | 未保存内容丢失 |
| **删除账户槽位** | 删除该槽位的整个登录目录 | **不可恢复**，需要重新登录 |
| **托管安装、升级、回滚** | 下载并替换 Claude Code / Codex 程序文件，旧版本收进版本库 | 可回滚（版本库最多留 3 份） |
| **清理多余副本** | 删除面板托管目录之外的 Claude Code / Codex 副本 | **不可恢复** |
| **完全卸载**（Claude Code / Codex / Claude 桌面端） | 按类清掉程序、版本库、缓存与登记、配置与会话、认证、环境变量与 PATH 项、Shell 配置行、凭据管理器条目、启动项与账户槽位 | **不可恢复。** 账户槽位删掉之后，所有账户都要重新登录 |
| **⚠ Chrome 完全卸载** | 卸载 Chrome 并**删除整个 `User Data` 目录** | **不可恢复。书签、保存的密码、扩展、全部网站登录态一并永久丢失** |
| **禁用本机 IPv6**（默认开启） | 每次启动面板时关闭整机网卡的 IPv6 绑定 | 可逆（关闭开关恢复原值）。可能短暂断网，需要管理员授权 |
| **⚠ 浏览器出站锁** | 给指定浏览器加 Windows 防火墙出站规则，物理网卡直连一律拦掉 | 可逆（面板里撤销）。**规则不随面板退出消失** —— 面板关了、甚至卸载了它还在，浏览器可能因此上不了网 |
| **系统代理修改** | 在你点了「修」之后改系统代理设置，改前记录原值 | 可逆（回滚到原值）。**改错会当场断网**，影响整机所有跟随系统代理的程序 |
| **浏览器策略** | 为当前用户写入 Chrome 的 WebRTC、安全 DNS 策略 | 可逆（面板里撤销） |
| **启动时对齐**（时区、区域格式默认开启；显示语言默认关闭） | 每次启动面板时，按出口 IP 归属地调整系统时区、区域格式、显示语言 | 可逆（设置里关闭，「高级维护」里可恢复时区）。改时区要管理员权限；显示语言要装语言包并注销后生效 |
| **停用旧启动脚本** | 每次启动面板时，把桌面上五个固定名称的旧版启动脚本（如 `ClaudeIpGate.cmd`）移进数据目录下的备份文件夹 | 可逆（从备份文件夹搬回） |
| **配置快照恢复、中转环境配置** | 覆盖或合并写入客户端的配置文件 | 写入前会备份，但请自行确认备份完整 |
| **账户迁移** | 复制历史账户目录到新的槽位布局 | 会先备份，但请自行确认备份完整 |
| **扩展（酒馆、MCP、Skills）** | 写入所选环境的配置；按你的操作启动第三方程序（桥接、SillyTavern、MCP 服务等） | 可卸载；第三方程序做了什么，由其自身负责 |
| **AI 提示词**（卸载提示词、DNS 高级检测等） | 把内置提示词交给你选择的 AI 执行；AI 可能读取系统信息、执行命令、修改或删除文件 | 取决于 AI 实际做了什么。**请逐条审阅 AI 要执行的操作后再允许** |

**`Chrome 完全卸载` 是本项目破坏性最强的功能之一。** 点下去就没有回头路，
执行前请自行备份书签与密码。维护者的建议很直白：**不清楚自己在做什么时，不要用这个功能。**

另需知悉的**已知门禁缺口**（设计使然，非缺陷）：
`%LOCALAPPDATA%\AnthropicClaude\app-<版本>\claude.exe` 无法加 Deny ACE，只能靠看门狗关闭；
npm 全局安装出来的 `codex.cmd` 挡得住 `codex` 命令本身，挡不住直接调用其内部 node 脚本；
执行锁不会冻结已经在运行的进程发出的网络请求。完整清单见 `docs/KNOWN-ISSUES.zh-CN.md`。

> **本项目提供的不是安全边界，不要把它当作强制访问控制使用。**
> 真正的强制执行需要 AppLocker / WDAC，不在本项目范围内。

---

## 7. 数据与隐私

**本项目不设任何遥测、统计、崩溃上报或云端同步，维护者没有服务器，收不到你的任何数据。**
所有检测与评分均在本机计算，面板自己的数据存放在本机 `%LOCALAPPDATA%\ClaudeIpGate`。

### 7.1 不需要你点击就会发生的网络请求

| 什么时候 | 地址 | 用途 |
|---|---|---|
| 每次启动面板 | `platform.claude.com`、`developers.openai.com` 的定价文档页 | 更新官方参考价（中转站计价用），拉不到就用内置快照 |
| 启动时对齐（默认开启）、恢复上次的放行、门禁放行期间的看门狗巡检 | `my.ippure.com`、`www.cloudflare.com/cdn-cgi/trace`、`ipinfo.io`，兜底 `api.ipify.org`、`icanhazip.com` | 查询出口 IP 与归属国家 |
| 打开中转站页面时；开启智能调度之后每约 60 秒（内测） | 你自己添加的中转站地址 | 读取站点账单与健康数据（不调用模型） |

这些请求都会让对方看到你的出口 IP。

### 7.2 你点击之后才发生的网络请求

| 功能 | 地址 |
|---|---|
| DNS 泄露检测 | `bash.ws`（**由原理决定**，该服务会看到你的 DNS 解析来源） |
| 中文环境检测里的 WebRTC 一项 | `stun.l.google.com`（Google 的公共 STUN 服务器） |
| 指定 IP 查询 | `api.ipquery.io` |
| 账户可用性检测 | `api.anthropic.com`（携带该槽位的本地令牌，见第 4 节） |
| 安装、升级 Claude Code | `downloads.claude.ai`；兜底为官方安装脚本 `claude.ai/install.ps1` |
| 安装、升级 Codex | `api.github.com`、`github.com`（openai/codex 的官方发布页） |
| 安装 Claude 桌面端、Chrome | 由 Windows 的 `winget` 联网下载 |
| 中转站连接诊断、站点检验（内测） | 你自己填写的中转站地址（**可能产生费用**） |
| 导入 Skills | `github.com`、`raw.githubusercontent.com` 与你填写的仓库 |
| MCP 连接测试 | 你导入的 MCP 服务，或运行你导入的命令 |

其他外部网址（IPQualityScore、ippure.com、Scamalytics、IPRoyal、各厂商控制台、官方文档、
新手指引等）一律**在你点击后由系统浏览器打开**，程序本身不代你访问，也不携带任何凭证。

从面板启动的 Claude Code、Claude 桌面端、Codex、SillyTavern 等软件，会按它们自己的方式联网，
**不受本项目控制**，也不在上面两张表内。

### 7.3 面板会读取的本机文件

以下读取都**只在本机处理，不上传**：

- **官方客户端写下的文件**：槽位里的 `.claude.json`（账户档案与用量缓存）、
  `.credentials.json`（只读到期时间）、桌面端的 `plan-usage-history.json`、
  **会话转写 `projects/*/*.jsonl`**（统计 token 用量时逐行读取，**这些文件里有你的对话内容**），
  以及 Codex 的 `auth.json`（套餐类型）与本机会话记录。
- **环境体检**：系统代理、IPv6、浏览器安全 DNS 策略、相关环境变量（值经过掩码）；
  扫描 MCP / Codex 配置里有没有写成明文的密钥 —— **只报位置，不报内容**。
- **Chrome 隐私审计与痕迹检测**：读取 Chrome 的策略与扩展权限；扫描 Chrome 用户资料中
  `Cookies` / `History` 文件的字节，仅判断其中是否**出现过 `claude.ai` 字样**。
  不解析数据库、不解密 Cookie、不读取登录态、不外传。介意的话请不要使用这些功能。
- **酒馆自动定位**：按目录扫描本机磁盘，找桥接与 SillyTavern 的特征文件；
  有深度、数量与时间上限，不碰网络盘。

### 7.4 凭证与日志

- 中转站 API Key 使用 Windows **DPAPI** 加密后存放在本机。**DPAPI 防的是配置文件被拷走、
  或被同机其他 Windows 账户读取；它防不住在你自己账户下运行的恶意程序。**
- 面板**不经手你的账号密码**：登录由官方客户端自己完成；OAuth 凭证不会复制进面板的
  数据库或配置快照。但账户迁移会**复制**包含凭证文件在内的整个槽位目录。
- **日志（例如 `ip-gate.log`）会记录你每一次的出口 IP。** 分享日志、截图或提交 Issue 之前，
  请把出口 IP、账户邮箱、Token 打码。
- 移交或报废机器前，请自行彻底清除数据目录、槽位目录、快照、备份与日志。

---

## 8. 第三方组件、外部链接与推广关系

- 第三方开源来源与许可逐条列在 [`ATTRIBUTION.md`](ATTRIBUTION.md)：
  抄了代码的、只用了公开接口的、看过但未采用的，分开说明。
- **SillyTavern 采用 AGPLv3。本项目不分发它，也不分发桥接脚本 `bridge.py`。**
  面板只在你已自行安装的前提下启动它们。
  **若你自行分发这些组件，AGPLv3 的义务由你自己承担。**
  桥接功能涉及第三方前端如何使用凭证发起请求，**其合规性取决于该桥接脚本的具体实现，
  不在本项目的审阅范围内，使用者需自行对照相应服务条款确认。**
- **MCP 服务与 Skills 是第三方代码。** 扩展中心的收录不是安全认证；安装、运行之前
  请自行检查来源与内容，它们做了什么由其作者负责。
- 外部链接指向的官方文档、第三方博客、视频、品牌名称与商家页面
  **均不在本项目开源授权范围内**，其内容、可用性与合法性由各自网站负责。
  本项目只放链接，不转载其图文。

### 推广关系披露

> **本项目中指向 IPRoyal 的购买入口带有推广代码**
> （`https://iproyal.cn/?r=sulianyan`，见 `crates/qb-probe/src/probe/verdict.rs`）。
> 通过该链接产生的购买**可能**为推广者带来收益。

除这一处之外，项目中的其它外部链接**不带推广参数**。DNS 泄露页面里的「新手指引」
（`docs.sulianyan.com`）是第三方网站，内容与可用性由该网站负责。
**列出不等于推荐，更不等于对其资质、合法性、服务质量或资金安全的背书。**
第三方平台独立运营，交易成败与资金风险自负。

---

## 9. 无支持承诺

本项目是个人业余项目，**不提供任何服务等级承诺**：不保证响应 issue、
不保证修复缺陷、不保证兼容后续版本的 Claude / Codex / Windows，
**可能随时停止维护或删除仓库**，恕不另行通知。标注为「内测」的功能不保证可用。

本项目**仅在 Windows 上开发与测试**，其他系统不在支持范围内。

---

## 10. 权利人通知与下架

若你是相关商标、著作权或其他权利的持有人，认为本项目的任何内容侵犯了你的权利，
或认为项目名称、文案造成了混淆，请通过 GitHub Issue 或仓库主页的联系方式告知。
**维护者将在合理时间内配合修改、更名、移除相应内容或下架整个仓库，无需争议。**

---

## 11. 禁止用途

不得将本项目用于以下任何用途。**一旦用于这些用途，由此产生的一切后果与责任均由使用者
自行承担，与维护者和贡献者无关：**

1. 违反你所在地或服务提供地的法律法规，包括但不限于国际联网、网络安全、
   数据与个人信息保护方面的规定；
2. 违反任何服务商的服务条款或使用政策，包括在服务商不支持的国家或地区使用其服务，
   或借助时区、区域格式等设置向服务商隐瞒所在地；
3. 规避、绕过或干扰任何服务商的安全措施、合规要求、地区限制、用量限制或封禁；
4. 注册、购买、租借、共享、转售账户，或创建、轮换多个账户以规避限制；
5. 转售、分销或变相出售 API 访问能力，包括借助本项目搭建或经营中转服务；
6. 在未获授权的他人电脑、账户或网络上使用；
7. 其它任何侵害他人合法权益的行为。

---

## 12. 责任承担与条款效力

- 在适用法律允许的最大范围内，维护者与贡献者不对因使用或无法使用本项目造成的
  任何直接、间接、附带、特殊或后果性损失承担责任，包括但不限于账户被限制或封禁，
  数据丢失，系统或网络故障，费用损失，以及第三方索赔。
- 因使用者违反本声明、法律法规或服务商条款而引起的任何索赔、处罚或损失，
  由使用者自行承担。
- 本声明中任何条款被认定无效或不可执行的，不影响其余条款的效力。
- 本声明可能随版本更新而修改，以仓库中的最新版本为准；更新后继续使用，
  即视为接受更新后的内容。
- 本声明与 `LICENSE`（GNU AGPL v3）的免责条款同时适用；涉及软件许可本身的问题，
  以 `LICENSE` 为准。

---

## English Summary

**QB Gate is an unofficial, independent, personal project. It is NOT affiliated
with, endorsed by, sponsored by, or reviewed by Anthropic PBC, OpenAI, Google,
Microsoft, or any other company.** All trademarks belong to their respective owners.
References to "Claude", "Anthropic", "Codex" or "OpenAI" in this project are
nominative only — they identify the software this tool works with, and imply
no trademark license, affiliation or endorsement.

- **What it is for.** Some work requires frequently changing IP addresses, and AI
  software left running across those changes can easily trigger a provider's risk
  controls. QB Gate keeps the AI software on your own Windows machine in check: it
  will not start while your exit IP is outside the allowlist you set, and it locks
  and closes gated sessions when the IP changes or cannot be determined.
- **What it is not.** It does not spoof device fingerprints or account identity, and it
  is **not designed to evade, bypass or defeat any provider's risk controls, security
  measures, regional restrictions, usage limits or bans — nor can it do so.** It is not a
  proxy, VPN or censorship-circumvention tool. Use it only where the service is
  available to you under the provider's terms, and only with accounts you own.
- **Provided "AS IS", with NO WARRANTY of any kind.** See `LICENSE` (GNU AGPL v3).
  The authors are not liable for any claim, damage, or other liability.
- **This tool does NOT prevent account bans or risk-control actions.** Its IP-purity,
  DNS-leak, locale and egress checks are informational heuristics built on public
  third-party APIs. Aligning the system time zone and regional format with the exit IP
  does not lower any ban risk and does not change where you are.
- **Relay stations (including smart scheduling) are an internal beta: visible in the UI
  but not yet usable.** The project provides, sells and endorses no relay service. Using
  third-party relays may breach model providers' terms. The relay feature includes a
  local API router bound to `127.0.0.1` only; it forwards only API requests explicitly
  addressed to it, does not change system proxy settings, does not intercept other
  programs' traffic, and does not chain to further proxies.
- **It changes your system.** Some features are on by default: disabling IPv6 on all
  network adapters, and aligning the time zone and regional format at startup. Others run
  only when you click: Windows Firewall outbound rules for one browser, system proxy
  changes, per-user Chrome policies. Some operations are **irreversible**, notably full
  uninstall, deleting account slots, removing redundant copies, and the Chrome uninstall
  that **permanently deletes the entire `User Data` directory**. The IP lock applies a
  `Deny ExecuteFile` ACE, after which double-clicking the executable is refused by
  Windows **by design**. The watchdog closes gated sessions without a grace period;
  unsaved conversations are lost.
- **This is not a security boundary.** Known bypasses exist (see §6). Real
  enforcement requires AppLocker/WDAC, which is out of scope.
- **Accounts.** You must own every account you use with it. Anthropic's policies prohibit
  creating or rotating accounts to evade limits or bans. The project makes no network
  request to query quota and calls no OAuth-internal endpoint; usage is read only from
  files the official client writes locally, for display. Account switching is manual only.
  The optional account check sends one minimal request to Anthropic's public models
  endpoint with the slot's local token, only when you click it.
- **No telemetry, no data collection, no cloud sync, no project server.** Some requests
  happen without a click: official pricing pages at every startup, exit-IP lookups
  for startup alignment and while the gate is open, and relay billing reads when you open
  the relay page or turn on smart scheduling. Everything
  else happens only on explicit user action. All endpoints are listed in §7.
- **Affiliate disclosure:** the IPRoyal link in this project carries a referral
  code (`?r=sulianyan`) that may generate commission. No other link carries a referral
  parameter.
- **Prohibited uses** are listed in §11. **You are solely responsible for complying with
  your local law and every provider's terms.**
- **This document is not legal advice** and cannot guarantee that anyone is free of
  legal liability. "GPL", "open source", "runs locally", and "for educational purposes
  only" are not blanket legal defenses.

**Rights holders:** if you believe this project infringes your rights or causes
confusion, please open an issue. The maintainer will rename, remove, or take down
the repository without dispute.
