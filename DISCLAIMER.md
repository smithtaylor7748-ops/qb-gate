# 免责声明

**最后更新：2026-09-11｜适用版本：v0.7.1 起**

请在下载、构建或运行 QB Gate 之前**完整读完本文件**。下载、构建或运行本项目，
即表示你已阅读、理解并接受以下全部条款；不接受其中任何一条，请不要使用本项目。

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

---

## 1. 非官方项目，与 Anthropic 没有任何关系

- QB Gate 是**独立的第三方工具**，与 Anthropic PBC **没有**任何隶属、合作、
  赞助、代理、认证或背书关系，也**未经**其审阅或许可。
- `Claude`、`Claude Code`、`Anthropic` 是 Anthropic PBC 的商标；`Codex`、`OpenAI`
  是 OpenAI 的商标；`SillyTavern`、`DeepSeek`、`Kimi`、`智谱` 等名称归各自权利人所有。
  本项目仅为说明兼容性而提及这些名称，**不主张任何权利，也不代表获得授权**。
- **本项目名称（QB Gate）不含任何他人商标。** 文档与界面中出现的
  `Claude` / `Codex` 等名称均为**叙述性使用** —— 用于说明本工具与哪些软件配合，
  不作为产品标识，也不暗示合作。维护者已知悉 Anthropic《商标指引》
  对未经许可的品牌使用与暗示合作的限制。若权利人仍认为不当，
  维护者将配合更名或下架，见本文第 10 节。
- 本项目**不分发、不镜像、不修改** Claude、Claude Code、Codex 或 SillyTavern 的
  任何二进制或源代码。安装一律调用官方渠道（winget / 官方安装脚本 / 官方下载页），
  下载与校验由这些官方渠道自己完成。

---

## 2. 不提供任何担保

本软件以「**现状**」（AS IS）提供，**不附带任何形式的明示或默示担保**，
包括但不限于对适销性、特定用途适用性、不侵权、准确性、可用性、
不中断或无错误的担保。

**在任何情况下，作者与版权持有人均不对任何索赔、损害或其他责任负责**，
无论该责任产生于合同、侵权或其他事由，也无论其是否由本软件或本软件的使用
或其他交易引起、与之相关。

本条为 `LICENSE`（GNU GPL v3）中免责条款第 15、16 节的中文表述，**如有歧义以 `LICENSE` 英文原文为准**。

---

## 3. 检测结果不是结论，本项目不承诺「防封」

面板中的 IP 纯净度、DNS 泄露、中文环境识别等所有检测，都是**基于公开接口与本地
指纹的参考性判断**，具有以下本质限制：

- **检测结果不等于任何服务商的实际判定。** 第三方风控模型不公开、随时变化，
  面板无从得知，也无法复现。
- **「纯净度 ≤ 5%」「原生 IP」「住宅 IP」不是防封承诺**，只是本项目采用的自测参考阈值。
  全部达标仍可能被限制或封禁；不达标也未必会。
- **「去除中文环境特征」不会降低任何封禁概率**，本项目从未做过这样的声明。
  Anthropic 公开说明会通过 IP 等信息推断大致位置，但**从未公布**「语言/时区与 IP 同区即安全」
  这类规则。任何据此得出的结论都是推测。
- 面板自查的数据来自 `my.ippure.com` 等**第三方公开接口**，其准确性、可用性与持续性
  由对方决定，本项目不保证。取不到的字段一律**如实显示「未知」，绝不猜成通过**。
- 关于「Claude Code 以 Unicode 隐写编码用户特征」的说法：这是**第三方逆向分析主张，
  本项目未做独立验证**，界面上已标注来源与未证实状态，**不应**被当作事实。

**任何账号被限制、封禁、扣费失败或数据损失，均与本项目无关，后果由使用者自行承担。**

---

## 4. 账户、服务条款与使用者责任

使用本项目**不能**免除你对各服务商服务条款的遵守义务。使用前请自行阅读并遵守
Anthropic《消费者条款》《使用政策》、Claude Code 的法律与合规文档，
以及 OpenAI、Google、Apple 等你所使用的各方条款。

本项目的设计约束（同样写在 `README.md` 的「合规边界」一节）：

1. **不发任何网络请求去查额度，不调用 OAuth 内部接口**；用量只读官方客户端
   自己写在本机的缓存文件，且仅用于显示，代码中不存在「用完自动换号」的路径；
2. 账户切换**只能由人手动触发**，无定时器、无看门狗、无自动调用点；
3. 任意时刻只有一个账户处于激活状态；
4. **所有账户必须是使用者本人合法拥有的**。

请注意，这些是**维护者的自我约束，不是合规背书**。特别地：

- **Anthropic 的政策禁止为规避限制或封禁而创建、轮换多个账户。**
  多槽位功能的定位是：在同一台机器上分开保存**你本人拥有的**不同账户的配置，
  用于修复损坏的安装、移交机器、清除本人数据。
  **将其用于规避限流、规避封禁、共享账号或账号买卖，属于使用者自身的违规行为，
  本项目明确不支持，也不承担任何责任。**
- **不得将本项目用于任何账号代充、代开、租借、转售或商业中介活动。**
- 卸载与清理功能会删除本机数据，**不为规避封禁而设计**，也无法达到该目的。

---

## 4.5 严格档：会话内门禁与国家白名单（v0.10.0）

这两项**默认都是关的**，打开之前请把代价读完。

### 会话内门禁（装进 Claude Code 的 hook）

它在每次请求发出前验一遍门禁，判不过就拦下这一次请求。**它是 fail-closed 的**：

- 出口 IP 不在白名单 → 拦；
- 查不到出口 IP → 拦；
- **面板没在跑（门禁裁决超过 90 秒没刷新）→ 也拦**。

也就是说，启用之后，**不开面板就用不了 Claude Code**。这是「宁可错杀不可放过」
的直接后果，由使用者自己选定。

三条自救路径，任何一条都够把自己放出来：

1. 面板 → IP 锁 → 会话内门禁 → 停用（面板自身不受 hook 影响，永远打得开）；
2. 被拦时命令行里会印出手动解除步骤；
3. 直接编辑当前槽位的 `settings.json`，删掉 `hooks` 里带 `_qb_gate` 标记的两段。

面板在**白名单为空时拒绝启用**这一项 —— 那等于每一次请求都会被拦。

### 看门狗不再给宽限期

**查不到出口 IP 时，看门狗第一轮就收摊**（上锁 + 关闭全部 Claude 进程），
Claude Code 与桌面端两档都一样。

早期版本给 CLI 档留过 180 秒宽限，现在没有了。这是使用者明确选定的严格档
（「宁可错杀不可放过」）。

**代价必须说清楚：** VPN 重连、DNS 抖动、切换节点、或者三个 IP 查询服务同时
限流，都会**直接关掉你正在用的 Claude Code 和桌面端，未保存的对话会丢**。

把误杀压到最低的办法是三源并发探测（ippure / Cloudflare / ipinfo，任一家答上来
就算查到）—— 所以「查不到」现在意味着三家全挂，而不是某一家抽风。
但这只是降低概率，不是消除。

不能接受这个代价的话，在看门狗停止的情况下使用（不点面板里的「启动」，
自己走受控入口之外的路径），或者自行修改 `WatchMode::unknown_grace`。

### 国家白名单

启用之后，出口 IP 归属的国家不在名单里，门禁一律判不合格：看门狗会收进程、
会话内门禁会拦请求、「加入当前 IP」也会被拒。

**代价：GeoIP 不准会误杀正在进行的会话。** 面板向三家（ippure / Cloudflare /
ipinfo）问国家，三家说的不一样时**按最严的算，判不合格**，冲突原文写进日志。
这意味着一次 GeoIP 数据打架就可能让你正在跑的会话被收掉 —— 日志里那一行会写清
是哪一家在胡说，但会话已经没了。

**名单为空 = 这一层不启用，不是全拒。** 这是刻意的：全拒会在使用者还没来得及
配置时就把他关在门外。

面板给的两个预设（「只留美国」「常用支持地区」）**不是 Anthropic 的官方完整
清单**，面板也不知道那份清单。它们只是省得手打的起手式，应当自行核对后增删。

---

## 5. 网络接入与所在地法律

本项目是一个**本地检测与配置工具**：它**不提供、不内置、不分发**任何代理、VPN、
翻墙或网络绕过功能，也不教授如何获取这类服务。面板中的 IP 与 DNS 检测，
只是读取并展示你**当前已有网络环境**的状态。

但必须明确：

- **国际联网的信道与接入方式在中国大陆有专门的行政法规要求。**
  本项目提供检测工具的行为，**不能**证明、也不试图证明使用者的网络接入行为合法。
- **你的网络接入是否合法，由你自己负责。** 请自行确认你使用的接入方式符合
  你所在地的法律法规。
- 本项目**不对**任何网络服务商作出推荐性的合规判断。

---

## 6. 本程序会不可逆地改变你的系统

**这不是一个只读的检测工具。** 以下操作会真实、且部分不可恢复地修改你的系统，
请在执行前确认你理解其代价：

| 功能 | 实际做了什么 | 可逆性 |
|---|---|---|
| **IP 锁** | 给 `claude.exe` / `codex` 加 NTFS `Deny ExecuteFile` ACE | 可逆。但**上锁后双击 exe 会被系统拒绝执行**，这是设计如此；必须走面板入口 |
| **看门狗** | 出口 IP 离开白名单时自动上锁并关停进程 | 进程被关停时**未保存的对话会丢失**，桌面端无宽限期 |
| **一键关闭** | 结束符合双重证据的 Claude / bridge 进程 | 未保存内容丢失 |
| **卸载重装 Claude** | 调用官方卸载器并清理本机残留 | 本机配置与数据会被删除 |
| **⚠ Chrome 清空重装** | 卸载 Chrome 并**删除整个 `User Data` 目录** | **不可恢复。书签、保存的密码、扩展、全部站点登录态一并永久丢失** |
| **账户迁移** | 复制历史账户目录到新的槽位布局 | 会先备份，但请自行确认备份完整 |

**`Chrome 清空重装` 是本项目破坏性最强的功能。** 界面上只弹一次确认框，
**点下去就没有回头路**。执行前请自行备份书签与密码。
维护者的建议很直白：**不清楚自己在做什么时，不要用这个功能。**

另需知悉的**已知门禁缺口**（设计使然，非缺陷）：
`%LOCALAPPDATA%\AnthropicClaude\app-<版本>\claude.exe` 无法加 Deny ACE；
npm 全局安装出来的 `codex.cmd` 挡得住 `codex` 命令本身，挡不住直接调用其内部 node 脚本。

> **本项目提供的不是安全边界，不要把它当作强制访问控制使用。**
> 真正的强制执行需要 AppLocker / WDAC，不在本项目范围内。

---

## 7. 数据与隐私

**本项目不设任何遥测、统计、崩溃上报或云端同步，不收集、不上传你的任何个人信息。**
所有检测与评分均在本机计算。

在你**主动触发**相应功能时，程序会向以下地址发起请求。除此之外不联网：

| 地址 | 用途 | 说明 |
|---|---|---|
| `api.ipify.org` · `icanhazip.com` | 查询本机出口 IP | 对方必然看到你的出口 IP |
| `my.ippure.com/v1/info` | IP 纯净度自查（公开接口，无 Key） | 同上 |
| `bash.ws/id` · `bash.ws/dnsleak/test/` | DNS 泄露回显测试 | **由原理决定**会让该服务看到你的 DNS 解析来源 |
| `downloads.claude.ai` | 查询 Claude Code 版本渠道 | 仅版本查询 |
| `claude.ai/install.ps1` | winget 不可用时的官方安装脚本兜底 | 仅在你点击安装时 |
| 你自行配置的中转站 `base_url` | 连通性探测 | 由你自己配置，风险自负 |

其他所有外部网址（IPQualityScore、ippure.com、各厂商控制台、官方文档、商家页面等）
一律**在你点击后由系统浏览器打开**，程序本身不代你访问，也不携带任何凭证。

关于凭证与密钥，请务必知悉：

- 中转站 API Key 使用 Windows **DPAPI**（`CryptProtectData`）加密后存放在本机
  `relay.json`。**DPAPI 防的是配置文件被拷走、或被同机其他 Windows 账户读取；
  它防不住在你自己账户下运行的恶意程序。**
- 面板读取 Claude 官方客户端自己写下的本地文件（`.claude.json` 的 `oauthAccount`
  与 `cachedUsageUtilization`、`.credentials.json` 的 `refreshTokenExpiresAt`、
  桌面端的 `plan-usage-history.json`）以显示套餐、剩余天数与用量。
  **纯本地文件读取，不发任何网络请求，不调用 OAuth 内部接口，也不调用任何额度接口。**
  用量只用于显示；面板不据此做任何决定，也没有自动切换账户的路径。
- 账户迁移会**复制**包含凭证文件在内的整个槽位目录。
  请自行确认备份目录、快照与日志的存放位置是否符合你的安全要求，
  移交或报废机器前请自行彻底清除。
- **痕迹检测**功能会扫描本机 Chrome 用户资料中 `Cookies` / `History` 文件的字节，
  仅判断其中是否**出现过 `claude.ai` 字样**。不解析 SQLite、不解密 Cookie value、
  不读取登录态、不外传。即便如此，若你介意程序读取浏览器文件，请不要使用该功能。

---

## 8. 第三方组件、外部链接与推广关系

- 第三方开源来源与许可逐条列在 [`ATTRIBUTION.md`](ATTRIBUTION.md)：
  抄了代码的、只用了公开接口的、看过但未采用的，分开说明。
- **SillyTavern 采用 AGPLv3。本项目不分发它，也不分发桥接脚本 `bridge.py`。**
  面板只在你已自行安装的前提下启动它们。
  **若你自行分发这些组件，AGPLv3 的义务由你自己承担。**
  桥接功能涉及第三方前端如何使用凭证发起请求，**其合规性取决于该桥接脚本的具体实现，
  不在本项目的审阅范围内，使用者需自行对照相应服务条款确认。**
- 外部链接指向的官方文档、第三方博客、视频、品牌名称与商家页面
  **均不在本项目开源授权范围内**，其内容、可用性与合法性由各自网站负责。
  本项目只放链接，不转载其图文。

### 推广关系披露

> **本项目中指向 IPRoyal 的购买入口带有推广代码**
> （`https://iproyal.cn/?r=sulianyan`，见 `crates/qb-probe/src/probe/verdict.rs`）。
> 通过该链接产生的购买**可能**为推广者带来收益。

除此之外，维护者与文中出现的其他任何商家、代理服务商、接码平台、支付平台
**没有**商业合作或分成关系。**列出不等于推荐，更不等于对其资质、合法性、
服务质量或资金安全的背书。** 第三方平台独立运营，交易成败与资金风险自负。

---

## 9. 无支持承诺

本项目是个人业余项目，**不提供任何服务等级承诺**：不保证响应 issue、
不保证修复缺陷、不保证兼容后续版本的 Claude / Windows，
**可能随时停止维护或删除仓库**，恕不另行通知。

本项目**仅在 Windows 上开发与测试**，其他系统不在支持范围内。

---

## 10. 权利人通知与下架

若你是相关商标、著作权或其他权利的持有人，认为本项目的任何内容侵犯了你的权利，
或认为项目名称、文案造成了混淆，请通过 GitHub Issue 或仓库主页的联系方式告知。
**维护者将在合理时间内配合修改、更名、移除相应内容或下架整个仓库，无需争议。**

---

## English Summary

**QB Gate is an unofficial, independent, personal project. It is NOT affiliated
with, endorsed by, sponsored by, or reviewed by Anthropic PBC, OpenAI, Apple,
Google, or any other company.** All trademarks belong to their respective owners.
References to "Claude", "Anthropic", "Codex" or "OpenAI" in this project are
nominative only — they identify the software this tool works with, and imply
no trademark license, affiliation or endorsement.

- **Provided "AS IS", with NO WARRANTY of any kind.** See `LICENSE` (GNU GPL v3 or later).
  The authors are not liable for any claim, damage, or other liability.
- **This tool does NOT prevent account bans.** Its IP-purity, DNS-leak, and
  locale-fingerprint checks are informational heuristics built on public
  third-party APIs. They do not reflect, predict, or influence any provider's
  actual risk decisions. No threshold here is a safety guarantee.
- **This tool contains NO proxy, VPN, or network-circumvention functionality.**
  It only reports the state of the network you already have. **You are solely
  responsible for the legality of your own network access under your local law.**
- **You must own every account you use with it.** Anthropic's policies prohibit
  creating or rotating accounts to evade limits or bans. This project makes no
  network request to query quota and calls no OAuth-internal endpoint — usage is
  read only from files the official client itself writes on your machine, for
  display — and it performs no automatic account switching, but that is a
  design constraint, not a compliance endorsement. Account resale, sharing, or
  brokering is expressly not supported.
- **Some operations are destructive and irreversible.** In particular, the Chrome
  reset feature **permanently deletes the entire Chrome `User Data` directory** —
  bookmarks, saved passwords, extensions, and all site logins. Back up first.
  The IP lock applies a `Deny ExecuteFile` ACE, after which double-clicking the
  executable is refused by Windows **by design**.
- **This is not a security boundary.** Known bypasses exist (see §6). Real
  enforcement requires AppLocker/WDAC, which is out of scope.
- **No telemetry, no data collection, no cloud sync.** Network requests are made
  only on explicit user action, to the endpoints listed in §7.
- **Affiliate disclosure:** the IPRoyal link in this project carries a referral
  code (`?r=sulianyan`) that may generate commission. No other commercial
  relationship exists with any vendor mentioned.
- **This document is not legal advice.** "GPL", "open source", "runs locally", and
  "for educational purposes only" are not blanket legal defenses.

**Rights holders:** if you believe this project infringes your rights or causes
confusion, please open an issue. The maintainer will rename, remove, or take down
the repository without dispute.
