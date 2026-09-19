# Changelog

## 0.24.8 — 2026-09-19

- **许可：AGPL-3.0-only 之上依 AGPL 第 7 节附加三条条款**（新文件 `LICENSE-ADDITIONAL-TERMS.md`，中英文，英文为准）：(b) 传播原版或改版时必须在 README 和界面「关于」页保留署名「基于 QB Gate，版权所有 (C) 2026 smithtaylor7748-ops。源码：https://github.com/smithtaylor7748-ops/qb-gate」；(c) 改版必须显著标明、不得暗示由原作者发布或背书；(e) 不授予「QB Gate」名称与图标，改版须改名。这三类是 AGPL §7 明确允许的附加条款，接收者不能删；之外的任何限制都没有加，项目仍是开源项目。README 授权声明、设置页「关于」、各 crate 与入口文件头、安装包 `licenses/`、Release 附件都指向这份文件，`release:check` 逐处断言。
- 第三方声明（`THIRD_PARTY_NOTICES.txt` / `docs/dependencies.json`）按当前锁文件重新生成：补上 0.23–0.24 期间新增的 25 个依赖，表头从残留的「GPL-3.0-or-later」改成现在的许可。

## 0.24.7 — 2026-09-19

- **「识别（turn-state）」改为直接作用于当前激活的 Codex 账户槽位**，推翻 0.24.0–0.24.6「分身环境 + 复制 OAuth + 另起一个 Codex」的做法（使用者批准）。「开启识别」现在做的是：把官方上游挂进本机路由（`OAuthPassthrough`，仍只绑 `127.0.0.1`）→ 备份并**增量**改那个槽位的 `config.toml`（只动 `model_provider` 与 `[model_providers.qb_turnstate]`，base 不带 `/v1`，不写占位 Key）→ 落盘 marker（`turnstate-takeover.json`）。**`auth.json` 一个字不碰、不复制凭证、不另起 Codex** —— 起 Codex 仍是账户页那颗按钮。复制 OAuth 的老做法会让两份 Codex 各自轮换同一族刷新令牌而互相登出（档案 §7.29 在 Claude 上踩过一样的坑），而且「账户页起的 Codex」和「识别起的 Codex」是两个进程、两套会话，使用者分不清哪个在走路由。
- 「关闭识别」按 marker **反向恢复**那个槽位的配置（不整份盖回备份：接管期间 Codex 自己写进去的项目授权 / MCP 都留着），恢复失败时保留 marker 让人再点一次。**面板启动时发现上次没关干净的 marker 会自动关闭并恢复**（本机路由随面板消失，留着会让那个槽位的 Codex 对着死端口），并清理 0.24.0–0.24.6 留下的 `qb-router-codex-official` 环境记录与目录（连同复制进去的 OAuth 快照）。
- IPC：`station_turnstate_enable` / `station_turnstate_disable` 取代 `station_turnstate_launch` / `station_turnstate_disarm`；`RouterStatus` 新增 `turnstate_takeover`（被接管的槽位名，落盘 marker 是唯一真相，面板重启后内存里的 `turnstate_official_armed` 归零而它不会）。账户页标题栏的「识别」按钮改按它显示开 / 关。
- 出站插件的互斥守卫改看落盘 marker（不只看内存里的路由状态），理由改成现在的事实：两者**都要改同一个槽位的 `config.toml`**，同时开就是两个程序抢同一个文件。

## 0.24.6 — 2026-09-19

- 修复：账户页「识别（turn-state）」挂上官方线之后**没有任何地方能断开**，而扩展中心出站插件的互斥守卫又指着这个不存在的「断开」—— 使用者被锁死在两者之间（实机截图撞到）。弹窗新增「断开识别线」（IPC `station_turnstate_disarm`，后端的 `disarm_official_codex` 早就有，只是没接）；断开只摘上游，不动正在跑的 Codex，界面提示把它关掉。
- 互斥守卫改成**双向**并改写理由：`station_turnstate_launch` 在插件跑着时同样拒绝。之前的文案「两者都要接管 Codex 的入口」不对——核心识别线用独立环境目录，插件管槽位 / 默认 `~/.codex`，**不共享配置文件**；真正的顾虑是两个程序各自给同一个 ChatGPT 账号维护 turn-state，互相干扰的可能性无法排除，保守起见一次只开一个。
- 出站插件改为**跟着当前激活的 Codex 账户槽位**：启动时 `CODEX_HOME` 指到槽位目录（没有槽位才用默认 `~/.codex`），接管的目录记进托管记录，停止时 `restore` 恢复**当初接管的那份**（中途切槽位也不会恢复错）。状态里新增「接管的 Codex 目录」。之前只管默认 `~/.codex`，通过 QB Gate 槽位用 Codex 的人开了插件也管不到自己在用的那份。
- 修复：安装失败残留的 `.incoming-*` 暂存目录会被定位器**优先**选中（`.` 排在字母前）；现在跳过 `.` 开头目录，安装前先清残留。解压后换目录时对杀软短暂占用做重试。
- 修复：插件状态把「进程在跑但端口还没听起来」写成「正在监听」；读版本不再每 10 秒 spawn 一次进程（按路径 + 修改时间缓存）。
- 修复：中转 / 识别环境起 Codex 撞上 Store 包注册失效（`CreateProcessW` 报 0x80070005）时也给出「修复 Codex 注册」的可操作说明，与账户页一致。「修复 Codex 注册」不再握着全局互斥锁等 UAC（等待期间看门狗会停摆）。

## 0.24.5 — 2026-09-19

- 「订阅」页重做（`src/features/subscription/`，原来的单文件页删除）：新增默认落地的**首页 · 为什么自己订阅** —— 不正规中转站的六类问题（不稳定与首字延迟、降智与偷换模型、官方反制、限流、卖数据 / 投毒 / 偷币、跑路涨价连坐），每条标明是报道、研究、官方条款还是使用者反馈，未经官方证实的写「未证实」；自己订阅拿到什么；官方各档价目表（Claude Pro / Max 5x / Max 20x，ChatGPT Go / Plus / Pro 5x / Pro 20x，网页价与 iOS 内购价分列，Claude Max 在 iOS 贵 25%）；按 SemiAnalysis 2026-06 实测上限换算出的「等效倍率」与中转站倍率区间放在同一把对数尺子上；**倍率计算器**（付的钱 ÷ 一个月能用多少刀，支持汇率与「中转站给的额度」对比）。
- 口径：全页不提任何特定国家、货币或发卡行，「国内」明确定义为两家都正式提供服务的国家和地区并链接两家官方名单；免税州地址与地址生成器（usaddressgen.com）只作格式参考、填本人真实地址，这类提醒统一红字并引用该网站自己的声明；删掉虚拟卡 / 卡商推荐；接码平台只作社区做法提及。`Subscription.test.tsx` 有禁词守卫盯着渲染文字。
- 资料补全：Apple ID 已改名 Apple 账户（account.apple.com）；礼品卡面额按套餐给；iOS 订的只能在 Apple 管理、别在网页重复订；退款渠道；ChatGPT Go；Anthropic 2026-02 起禁止订阅 OAuth 用于第三方工具 / 中转。数据来源与复核日期记在 `docs/subscription-guide/SOURCES.md`。
- 样式：修掉页面里不存在的 Tailwind 类（`text-text*` / `border-border`，它们让正文失色、卡片边框变成粗黑线）与多包的一层 `qb-page`；改用 `Card` / `Pill` / `Bullet` / `Collapsible` / `ExternalLink` / `Field` 等现成原语，颜色全部引用 `tokens.css`，780px 以下价目表堆成卡片。
- 仓库：按使用者决定去掉 `.gitignore` 里「订阅指南永不入库」的两行；订阅页随本项目一起公开。

## 0.24.4 — 2026-09-19

- 新增「订阅」菜单栏（`/subscription`）与双 AI 官方订阅实战指南（Claude Pro 与 ChatGPT Plus）。
- 按照支付方式划分为四大核心板块：苹果生态内购（Apple 余额 · 本地扫码结算 · 99% 首选）、安卓原生订阅（Google Play 原生 · 严禁礼卡避坑 · 跨端共享）、国际银行卡直付（PC 网页直连 · 账单地址实战 · 0% 避税）、答疑公约与排查（FAQ · 拒付急救 · 免税州速查 · 用户声明公约）。
- 首次进入订阅页强制弹出《知情公约与合规声明》前置确认弹窗，用户确认签署后方可解锁查看后续具体实操步骤；答疑公约与排查页支持重新唤起阅读。
- 严格遵循开源合规：全篇严禁提及特定受限区域，明确界定「国内」特指双 AI 官方合法支持与商业开放的服务区域；真实地址提示以深红重点警示醒目标出，明确声明本工具仅为本地看门狗与安全审计开源工具，绝不提供代充、代付或账号买卖中介服务。

## 0.24.2 — 2026-09-19

- 账户页「Codex 桌面端」卡片标题栏新增「识别」入口：官方 Codex 的 turn-state 开关、个人/Team 规则、「接入并启动 Codex」（识别模式）、采集状态表与完整说明都在这个弹窗里；按钮文字如实显示后端状态（注入开没开）。`RouterStatus` 新增 `turnstate_enabled` / `turnstate_team`，界面的开关**回读后端真实状态**，不再靠前端本地猜（原来面板重开后界面显示「关」而实际在注）。窄窗口（≤780px）下这个入口收起，中转站页 Codex 分页仍可打开同一弹窗。
- 扩展中心新增「Codex 出站与换出口（ccodex 引擎）」条目（`install_method: connect`）：接入你自己安装的 `ccodex-sleep-state`（外部程序，GPL-3.0）——定位 exe、以带窗口的独立进程启动它的 `setup`、停止时结束进程并调用它自己的 `restore` 恢复 Codex 配置、打开它的网页面板。代理 / 订阅 / 机场出站与实验性的「换出口凑 292」都在那个程序里实现；**本程序核心仍不内置代理、不换出口**。它与账户页的官方 turn-state 识别线互斥（后端守卫）。IPC：`codex_egress_status` / `codex_egress_config` / `codex_egress_config_save` / `codex_egress_start` / `codex_egress_stop`。
- `OwnedProgram::launch_detached_console`：给会改写外部配置、退出时要自己恢复的外部程序用的启动方式（带控制台窗口、不随面板退出而被杀）。
- 扩展中心的该条目支持**一键下载、校验、安装**：从插件仓库（你自己的 fork）的 Releases 取 Windows 包与 `SHA256SUMS`，边下边算 SHA-256 并核对，解压到 `%LOCALAPPDATA%\Programs\ccodex-sleep-state\` 后登记 exe 位置；旧版本改名留一份。下载地址只认 github.com；正在运行时不装。IPC `codex_egress_install`，进度任务 `egress-install`。

## 0.24.1 — 2026-09-19

- 修复：官方账户·Codex 的「启动 / 切换」在 Codex（Microsoft Store 版）自动更新后可能报 `IO 失败: Access is denied. (os error 5)`。根因是新版 Codex 的打包应用带了需要管理员注册的服务，更新未走完时当前用户的注册失效，任何启动方式（std/CreateProcessW/应用激活）都会被系统拒绝 —— 与本工具用哪种进程创建方式无关。现在这种情形会给出可操作说明，而不是一句裸 IO 错误。
- 新增：账户页「修复 Codex 注册（需要管理员）」一键修复（IPC `codex_repair_registration`）。它以管理员重新为当前用户注册 Codex 打包应用（`Add-AppxPackage -RegisterByFamilyName`），只做重新注册，不改任何安全设置、不动别的包、不复制凭据；使用者不在 UAC 授权就如实返回「已取消」。也可在 Microsoft Store → 库 更新 Codex 达到同样效果。
- 关闭 Codex 桌面端失败时的文案改为可操作：提示它可能以管理员/沙箱提权运行，需从任务栏手动退出或以管理员运行面板。

## 0.24.0 — 2026-09-18

- 新增**官方 Codex 的 turn-state 采集 / 注入**（实验功能，默认关闭，**只作用于 Codex，不碰 Claude**）：本机路由的官方模式保留客户端自带的 OAuth，被动采集官方响应带回的 `X-Codex-Turn-State`（不额外发请求、不烧额度），并在客户端自己没带时补注一张；客户端带了就保留它自己的。长度只是经验筛选口径，不是质量或额度指标。机制照 ccodex-sleep-state（GPL）公开协议 clean-room 重写，未复制源码、未引入 Mihomo / 代理出口、不「换出口凑 292」。见 DISCLAIMER §5.3。
- 本机路由**健康判定改看 SSE 结局**：Codex 的 HTTP 200 但流里 `response.failed` / 断流不再被当成成功；区分容量不足（`server_is_overloaded`）与限流（`rate_limit_exceeded`）。纯函数 `qb-station::sse` 实现，只对 Codex 路由生效，Claude 路由行为不变。
- 本机路由给上游连接加了 15 秒连接超时（不设读/总超时，避免截断长回复）。
- **一键把官方 Codex 接进本机路由**（turn-state 面板里的「接入并启动 Codex」）：备好一个只属于 Codex 的官方环境（`requires_openai_auth=true`、不写占位 Key、base_url 不带 `/v1`——官方端点是 `.../backend-api/codex/responses`），启动前把你自己的 ChatGPT OAuth 从当前 Codex 账户槽位（或 `~/.codex/auth.json`）**复制**一份进环境目录（只读源、只播一次，之后交给 Codex 自己续期），在路由里挂上官方上游（`OAuthPassthrough`，「路由不承载官方身份」的唯一受控例外，仍只绑 `127.0.0.1`），再起 Codex 走这条链。真实「拿 292」取决于你自己的账号与网络，本功能不换出口、不保证 292。
- 新增 IPC：`station_turnstate_configure` / `station_turnstate_status` / `station_turnstate_launch`；`RouterStatus` 增加 `turnstate_official_armed`。

## 0.23.1 — 2026-09-18

- 「查套路」弹窗重做：后台连接、受控检验、三层报告、逐次证据和加密历史；适配浅色、深色及窄窗口，外部仅调整入口文案。
- 按 station-monitor 源码接入 New API / Sub2API 账号密码登录、Cookie/访问令牌兼容、会话刷新及余额；提供独立浏览器登录入口，支持用户在本站使用 Linux DO 或二次验证。远程登录窗口不能调用应用命令。
- 临时测试 Key 只在本轮内存中使用。默认 1 次预热加 5 次缓存复用验证；可单独确认第 7 次冷前缀对照。按新账单核对请求 ID、用量、实扣、官方成本和余额差额，缺失证据不会生成完整倍率。
- 按用户指定口径，账单读取、倍率计算和前后余额核对忽略货币单位，直接比较账单数值，不进行汇率换算。
- 保留原源码本地 CSV/JSON Worker 分析，最多每文件 5 MiB / 50,000 条，不上传。
- Codex 中转启动单独提示 Windows 沙箱初始化错误；本机已用官方初始化器修复 helper_sandbox_lock_failed，并验证沙箱命令可执行。
- ccodex-sleep-state 已评估，未引入 Go/Mihomo 服务或官方 turn-state 注入；cockpit-tools 仅参考设计。

## 0.22.6 — 2026-09-16

- 修复酒馆起来之后报 `Not allowed to open url http://127.0.0.1:8000`、页面打不开的问题。
  面板的权限文件只放行了 opener 插件的 `open_url` 命令，没给任何网址范围，
  而插件对空范围是一个都不放。现在范围是 `https://*` 加 `http://127.0.0.1:*`
  （酒馆端口可以改，所以不写死 8000）。
- 同一个原因，面板里其它外链（纯净度的来源站点、选购入口等）之前点了也打不开 ——
  它们的错误被吞掉了，只有酒馆那一处弹了出来。这次一并恢复。
- 酒馆已经起来、只是页面没打开时，不再报成启动失败：提示改为「酒馆已就绪，
  但页面没打开」并给出地址，状态照常刷新。原来那条报错出现的时候，
  桥接和酒馆其实都已经在跑、租约也拿着。
- 新增 `src-tauri/tests/capabilities.rs`：面板会打开的每个地址都必须在 opener
  范围里；范围也不许放宽到能打开本机程序。

## 0.22.5 — 2026-09-16

- 酒馆新增「自动定位」：在本机找桥接与 SillyTavern 装在哪，不再要求使用者
  凭记忆填三个绝对路径。三档证据分开显示 —— 正在跑的进程、`bridge.pid`
  记的那个、扫盘命中；备份目录（`备份N`、`*.reinstall-backup-*` 等）扣分排后面。
  候选列出来由使用者点「采用」，面板不自动写配置。默认档几秒回来，
  找不到可以点「深扫」。
- 判据是文件内容不是目录名：桥接要 `bridge.py` 里同时有 `--claude` 和
  `--data-dir`，酒馆要 `server.js` 加 `package.json` 里 name 是 sillytavern。
- 扫描有预算（默认深度 4 / 4 万目录 / 15 秒，深扫 7 / 40 万 / 90 秒），
  跳过系统目录与 `node_modules` 这类重目录，不碰网络盘。预算用完会写明
  「没扫完，结果可能不全」，不跟「没找到」混为一谈。
- 依赖检查现在真的显示出来了。`status()` 一直在算 `checks`，而前端从来没有
  渲染过它 —— 页面上那句「展开看缺哪一项」指着一个不存在的地方。
- 酒馆详情页重做：顶部一条状态条（在哪一档 + 当下该点的那个按钮），
  资产改成分类磁贴，去掉各卡片顶部的功能介绍段落。

## 0.22.4 — 2026-09-16

- 修复酒馆点了起不来、错误只说「路径不存在: bridge.py」的问题。三个路径从
  0.12.0 起默认留空（不再写死某台机器的盘符），但留空之后走的还是「路径不存在」
  那条错误 —— 空目录拼出来的相对文件名，既没说是哪个 `bridge.py`，也没说该去哪填。
  现在「还没配」和「配错了」分成两句话：没配的说明还空着哪几项、去哪里填；
  配错的照旧报绝对路径，好对着盘上的真实位置改。
- 总览的酒馆磁贴在依赖没配齐时不再是一个只会失败的按钮：贴上先写明「还没配好」，
  点下去直接到扩展页的酒馆设置，那一页的路径设置区也自动展开。
- 酒馆的依赖检查补上「酒馆启动脚本」一项。原来只查桥接和 SillyTavern 目录，
  启动脚本路径错了会一路显示「就绪」，直到启动走到第 4 步才失败。
- 路径没填时依赖列表显示「未配置」，不再是一行空白（看起来像检测坏了）。

## 0.22.3 — 2026-09-16

- 移除中转站重复页标题和常驻操作提示条；启动失败原因放在启动按钮内。
- 修复添加线路时 API Key 未保存、编辑站点地址未落盘、转发及检验仍读取旧版站点库的问题。新 Key 走现有 DPAPI 凭证存储，所有请求按线路绑定取用。
- 修复 `/v1/v1` 重复路径；Codex 增加独立前缀，模型列表和辅助请求不会串到 Claude Code。
- 中转 Codex 启动 Microsoft Store 桌面应用，使用独立 CODEX_HOME、桌面资料目录和 Chromium `--user-data-dir`；不再启动 CLI，不关闭当前其他桌面任务。
- 中转自动合并客户端写入的配置，保留项目授权、模型、MCP 和偏好；仍阻止中转目录混入 OAuth 身份。
- 启动后依据托管会话切换为“一键关闭”，手动退出后恢复“启动”。只关闭本页托管会话。
- 旧版未保存的 Key 无法恢复，已有空凭证线路需重新填写一次。


## 0.22.2 — 2026-09-16

- 本机 IP 自测只请求一次，出口地址、IPPure 系数和住宅 / 家庭 IP 判断来自同一份读数，避免重复请求失败或两次出口变化造成结果矛盾。
- 总览区分「已自测」与「已人工复核计分」；旧复核记录未关联当前 IP 时仍展示最新自测读数，不再把缺少人工复核显示成检测没完成。真实网络失败不会继续采用旧 IP 计分，重试成功后恢复。
- 删除账户总览的「还有 N 项待检查」横幅和对应提醒内容。

## 0.22.1 — 2026-09-16

- IP 纯净度增加默认开启的「禁用本机 IPv6」网卡开关；启动面板时应用，关闭时按逐网卡备份恢复原值。需要时单独请求管理员授权，实际状态、部分失败和待恢复网卡可见；不会把注册表待重启配置误报为已生效。

- IP 纯净度弹窗增加 IPRoyal 选购入口和新 IP 输入框；指定 IPv4 / IPv6 通过 IPQuery 查询风险分、代理/VPN/Tor/机房标记与网络信息，独立展示且不改变当前出口、门禁或综合评分。
- 本机检测、住宅 / 家庭 IP 判断和一键全面体检保持原用途；指定 IP 输入框是标题栏中的可选查询，结果追加在下方，不覆盖本机结果。IPv6 开关放在弹窗标题右侧；移除无法检测的原生 IP 占位项及冗长说明框。
- 修复一键全面体检被后台工作区通知取消、报 `CancelledError` 的问题；通知只刷新实时状态，保留手动检测及其结果。
- 同一资源的并发刷新共用正在执行的请求；明确作废的检测即使稍后返回，也不会重新写入历史缓存。
- Codex 桌面端启动卡增加独立「一键关闭」入口及运行状态，确认后复用安装路径、进程创建时间和祖先链校验，只关闭桌面端及其任务，保留账户与登录资料。

## 0.22.0 — 2026-09-16

- 官方账户导航改为同一块内的 Claude / Codex 切换；每页四个槽位，右侧上方启动、下方用量。
- 账户页左右等宽等高，固定在窗口内，取消页面滚动；小窗口保留四个账户与全部操作，统计说明、配置检查和环境待办在弹窗中查看。
- Codex 页接入 Microsoft Store 桌面端的独立账户登录、手动切换、可恢复归档；登录由官方客户端完成，默认 Codex 配置保留原处。
- Codex token 用量读取本地 rollout，按会话累计差值去重；缓存和推理为输入/输出的子集，不重复相加，无法核算的记录单列。
- 全面体检覆盖五项并包含本机补充检查；单项失败继续后续检测，增加检测证据及一键修复后复检。
- 浏览器策略修复保存原值、支持撤销，并尊重整机策略优先级；DNS 回显缺失与国家信息未知不再误报通过。


> **0.14.0 through 0.18.2 are missing from this file.** The changelog was left
> behind for five releases. Nothing is reconstructed here from memory — what those
> releases contained is in the GitHub Releases notes, in
> `docs/DESIGN-NOTES.zh-CN.md` and in the git history.

## 0.21.0

一轮针对账户页的排查，五条修复加一个新功能。第一条最贵：它让 0.20.0 刚做的
用量统计在这台机器上**整个是空的**。

- **token 统计读的目录，实机上两周没人写过。** 0.20.0 只数
  `<槽位目录>\projects\`。实测：两个槽位今天各 0 条回复、最后一条停在 9-02 /
  9-04，而同一天 `~\.claude\projects` 里有 780 条、2.9 亿 token。
  原因是桌面端 Code 页里跑的会话不吃 `CLAUDE_CONFIG_DIR`（档案 §7.26），
  转写全落在默认目录。于是那张用量卡的默认档「今天」四个格子**全是 0 和破折号**。
  现在两处都数：默认目录那部分按转写里的 `ownerAccountUuid` 归属
  （⚠ 它在 `bridge-session` 行上，**不在 assistant 行上** —— 去 assistant 行上找
  只会得到零命中，这是实测踩过的），一个文件一个 owner。
  **归不出来的那部分不摊给任何账户**：实测有 16% 的记录没有归属标记，
  今天这一整天的**全都没有**，所以它单独显示成「另有 X 归不到任何槽位」。
  按「默认目录现在登录的是谁」去认领很诱人，但那是拿此刻的快照认领几个月的历史。
- **用量取价走的是内置快照，不是抓回来的官方价。** `token_summary` 调
  `pricing::lookup`，那只查编译进去的那张表（2026-06-24 核对的）；而面板每次
  启动都会抓官方定价页并存进库（实测 50 条，就在 `station_prices` 里），
  这条路一条都没用上。官方调一次价，这里的美元数就静默地按旧价算下去，
  而界面还写着「按本次抓到的官方价算」—— 那句话永远为假，因为
  `ModelPrice::as_resolved()` 把 `live` 写死成 `false`。
  现在收 `pricing::Catalog`（先查抓回来的、再退回快照），装配在命令层；
  取不到任何价时 `priced_from_snapshot` 回 `None` 而不是 `true`。
- **删槽位只挡住了一个「当前」。** 面板的当前槽位看 `claude-profile` 指向谁，
  桌面端的看 `%APPDATA%\Claude` 指向谁 —— 两者**故意可以分开**
  （`DesktopMode::Keep` 就是干这个的）。原来只挡前者：面板在 A、桌面端在 B 时
  删 B 会被放行，`%APPDATA%\Claude-B` 被删掉、`%APPDATA%\Claude` 悬空，
  桌面端下次启动数据目录是空的，而界面上一个字都不报。现在两个「当前」都挡。
- **一键关闭的执行失败只活在 toast 里。** 磁贴与横条绑的都是
  `killswitch-preview`，而真正的关闭走 `killswitch-execute` —— 没有任何地方
  渲染它。`Tile.tsx` 自己的文件头写着「失败则整贴变红并留住错误原文
  （toast 会自己消失，错误不能只靠它）」，一键关闭恰恰是最不能只靠 toast 的
  那一个：没关掉的进程还连着旧账户。现在两档任务态合并渲染，关闭过程中也显示。
- **用量卡的「今天」从开面板那一刻起就冻住。** `useEffect` 只依赖
  `[label, days]`，两个都是不变的值，跑一下午也不动。现在每分钟自己重算一次
  （比账户列表那条 30 秒慢一倍，因为它要扫转写），另有一个立刻重算的按钮，
  并把上次读的时刻标出来。
- **头条数字被缓存读淹没。** 「总 token」把四类加起来，而实测缓存读占 96%
  （3.23 亿里 3.11 亿），真实输入只有 6 千多。四个格子改成
  「输入 / 输出 / 缓存命中率 / 缓存省下」，四类合计降级成副标。
- **新增「还能用吗」实测**（`usecase::account_probe`）。`EXPIRY_CAVEAT` 一直
  写着「唯一能确认的办法是实际发一次认证请求」，而面板从来没发过 ——
  剩余天数看起来健康、账户其实已被回收时，界面上看不出任何东西。
  现在详情页里有一个按钮，拿槽位里的令牌向官方公开的 `/v1/models` 发一次
  最小请求。⛔ **这不是「联网查额度」**：不打模型、不产生 token、不问额度、
  不往槽位里写任何东西。**本地令牌已过期时连请求都不发** —— 拿过期令牌去问，
  401 是必然的，报成「你的账户被拒了」是彻头彻尾的假警报；也不拿
  refreshToken 去换新令牌，那等于面板开始经手凭证。
  ⚠ `/v1/models` 认不认 Claude Code 那种 OAuth 令牌**还没用真令牌验过**
  （手上两个槽位的令牌都过期了），所以「被拒」那一档的文案里把第二种可能
  也写了出来，验到 200 之前不许删那半句。
- 修 0.20.0 的一个类型名撞车：新加的 `ProbeResult` 跟 `qb-station` 那个同名，
  ts-rs **静默覆盖**了后者 —— 正是 `types:check` 第二条断言盯的那种事。
  账户这边改名 `AccountProbe` / `AccountProbeState`。
- **「重新登录」对当前账户也走一遍完整清场，这是刻意保留的**，不是漏改。
  使用者定的：「就要这种非常保守的」。理由与「只启动、不清场」那条路的分工
  写进了 `requests.ts` 与 `AccountDetail.tsx` 的注释 —— 别当 bug 修掉。

## 0.20.0

账户页的五件事，四条是使用者报的，两条的根因在真机上量出来了。

- **额度「快照已过期」，而数据其实是新的。** `usage::for_slot` 只读
  `%APPDATA%\Claude\plan-usage-history.json` 一条路径，可 `%APPDATA%\Claude`
  是个联结点，**只指向当前槽位**的桌面端资料目录；别的槽位的历史在它自己的
  `Claude-<标签>` 里。实测 2026-09-16：`main` 槽位按 org 一条实时样本都匹配不上，
  退回 9 月 2 日的 `cachedUsageUtilization`（旧了 20093 分钟），界面打出
  「快照已过期」—— 而 `%APPDATA%\Claude-main\plan-usage-history.json` 里有它 174 条
  样本，最后一条是十二分钟前写的。现在两条路径都读，按 `(t, org)` 去重
  （当前槽位那两条会解析到同一个文件）。路径由 `AccountRoots::history_files` 拼，
  「Claude 的目录名怎么拼」仍然只有那一处。
  顺带修掉同一处的一个不对称：过期判定原来只对 `cache` 那一档生效，桌面端那一档
  再旧也当当前值显示 —— 而桌面端的样本只在它运行时才写，关一天读数就冻在最后一条。
  现在两个源同一把尺子，来源仍然标出来。
- **额度不会自己刷新。** `R.accounts` 没有 `pollMs` 也没有 `staleMs`，加上全局的
  `refetchOnWindowFocus: false`，等于一次会话只打一发 `accounts_list`：文件更新了，
  页面纹丝不动。改成 30 秒轮询（`accounts_list` 是纯本地文件读，没有 PowerShell、
  没有网络，符合 `resources.ts` 开头自己写的那条策略），并在账户区加了一个显式的
  刷新按钮。
- **新建账户可以建完就登录。** 新建框加了一个默认勾上的「建好之后立刻登录」。
  原来没有激活槽位时新槽位直接成为当前的，**不切、不关任何进程**就起 Claude Code；
  已经有别的槽位在用时走切换确认框（它本来就会先报「会关掉哪些进程」），切完自动起。
  `workspace::launch` 里那道「请先在官方账户页切换到此账户」的拦截**没有动** ——
  它守着「任意时刻只有一个账户激活」。面板全程不经手账号密码，登录界面是官方客户端弹的。
- **「管理」页删掉了。** 使用者的原话：「管理账户不就是删除吗」。那一页是上一代设计
  被丢下的产物 —— 不在 `NAV` 里（侧栏和 Ctrl+K 都搜不到）、旧一代样式，而且显示的东西
  **比它要管的那条小条条还少**（没有剩余天数、没有额度、没有到期语气色）：
  从总览点「管理」进去是一次降级。现在删除直接长在小条条上，`/accounts` 重定向到 `/`，
  那一页独有的「官方目录里的 API 配置」搬进总览的一个折叠块。
  小条条本身抽成了 `SlotRow`，总览与详情页共用。
- **可以删槽位了。** 全项目原来没有任何删除入口，唯一会删槽位目录的是按软件整片删的
  「完全卸载」。新增 `accounts_delete`：**当前激活的槽位一律拒删**（删掉它会留下一个
  悬空的 `claude-profile`，而没有任何东西会去修它，下次启动会安安静静退回
  `~\.claude` —— 那正是 §7.26 那个坑的形态），要手打「删除」两个字，
  桌面端那份资料默认跟着删（留着的话以后建同名槽位会静默继承一份旧的桌面端身份）。
- **账户详情页。** 小条条的标签点进去：身份（邮箱 / 组织 / UUID）、套餐与凭证
  （含 `plan_fetched_at` —— 它的类型注释一直写着「界面上要标出来」，而在这之前
  没有任何地方画它）、额度窗口、目录，以及一个**常驻**的「重新登录」按钮。
  常驻是有理由的：剩余天数只读本地时间戳，**查不出「被风控下线」**，
  令牌被回收时它看起来完全健康 —— 按「看起来过期没」决定要不要给这个入口，
  等于在最需要它的时候把它藏起来。
  ⛔ 仍然不显示 `organizationRateLimitTier` / `userRateLimitTier`，
  新增一条测试钉着**整个 Slot 序列化之后的文本**里不许出现那两个值。
- **用掉了多少 token，可按时间和模型筛。** 数的是槽位目录里的会话转写
  （`projects/<项目>/<会话>.jsonl`），零网络请求，跟额度那两个窗口同一条口径。
  **必须去重**：续接会话和自动压缩会把之前的回复原样重放进新文件 ——
  实测 `main` 槽位 6,049 条里有 2,860 条是重放，不去重的话输出 token 会报成
  6,377,231 而真值是 2,960,298，**2.15 倍**，而且多出来的那一倍长得完全像真数据。
  界面上写明只覆盖这个槽位目录里的会话，也不折算成钱（订阅账户按 API 单价折出来的
  金额不是你实际付的）。没做增量缓存：实测 90 MB 一遍扫完 0.46 秒，
  为这点开销换一份要自己维护失效逻辑的磁盘缓存不划算。
- **综合评分从四项变成五项**，新的那一项是「出口一致性」（权重 10，从纯净度
  35 → 30、DNS 25 → 20 各拿 5 分）。它只收**与出口 IP 对不上**的信号：
  绕过代理与跟随代理量到的是不是同一个国家、IPv6 会不会从另一条路出去、
  Chrome 报的语言跟 IP 地区合不合得上、WebRTC 会不会把真实地址捅给网页、
  DNS 是不是走了另一条路。前两项直接取 `checkup::scan()` 的结果，**不另写一遍**。
  浏览器里的 claude.ai 痕迹、扩展权限、凭据管理器、环境变量残留**不在这一项里** ——
  它们跟出口 IP 没关系，各自已经有地方。
  WebRTC / DoH 两项可以「修」（复用 `browser_audit` 已有的写入口），
  语言与出口那一项**只报告** —— 改机器身份去对上出口是 CLAUDE.md 点名不做的事。
  **「查不出来」的行从分子分母里一起去掉**，跟 `score.ts` 顶上那条同一个道理。
- **账户页重排**（使用者逐条点的，目标是一屏放完、不许滚）：删掉只放一个标题的页头（`h1` 留着，
  只对读屏器可见 —— 删掉可见标题不等于这一页没有标题），
  「重新检测 / 一键全面体检」并进综合评分那一行；
  删掉「可用 / 全部」切换（过期和没登录的槽位本来就该看得见，藏起来只会让人找不到）
  和「Usage」外链；Claude / GPT 那条页签并进侧栏，成了「官方账户」下面的两个子项；
  运行日志从这一页撤走，挪进「执行锁」那个小窗；「一键关闭所有 Claude」变成启动格里的
  第四块贴（起和收是同一件事的两头，2×2 也正好填满右栏）；对话框里的长段说明压成一句；
  槽位一页从 5 条改成 3 条，多的走页码；默认窗口 1180×820 → 1320×940。

  ⚠ 启动**试过**搬到槽位下面通栏：右下角那片空地是没了，代价是想启动得先滚一屏 ——
  使用者当场退回来。最常点的按钮不该跑到首屏之外，这条比留白好看重要。
  空地改用别的办法收：三块贴变四块、2×2 填满右栏。
- **账户页下面加了一张用量小结**：今天 / 7 天 / 全部三档，四个数 ——
  总 token、输入输出、缓存命中率、缓存省下多少钱。
  命中率的分母里**没有输出**（输出是生成的，没有「命中」一说）。
  那个美元数是「这些缓存读的 token 按整价重读要多花多少」，
  **不是你花了多少** —— 订阅账户付的是月费，界面上写着这句口径。
  单价按模型分别取（不同模型输入价差好几倍），认不出价的模型跳过并报数，
  不拿别的模型的价去凑。
- 槽位列表下面那两句口径说明（套餐读自哪儿、剩余天数查不出被风控下线）
  搬进了账户详情页的「套餐与凭证」一节 —— **只是换位置，没有删**，
  `AccountsReport` 的类型注释写着这两句要显示。
- 补掉三处缺口：`checkup` 的环境变量清单原来只认死名字，`CLAUDE_CODE_*` 整族
  （含 `_USE_BEDROCK` / `_USE_VERTEX`）一个都查不到，而 purge 的归属判定和启动前的
  残留检查早就认它了 —— 同一件事三处口径不一样，最松的那处就是使用者看到的那处；
  新增 `set_doh_policy` / `clear_doh_policy`（只写 `automatic`，**不写 `secure`** ——
  那要求同时配 `DnsOverHttpsTemplates`，没配的话 Chrome 什么都解析不出来，
  「收紧隐私」的按钮把浏览器打断网是这一类功能最糟的失败形态）；
  三处传给界面的文案里写着 Markdown 的 `**`，而那几处是纯文本渲染的，
  界面上显示的就是两个星号。

## 0.19.2

One defect the user reported against 0.19.1 — every non-ASCII name on the
software page arrived as question marks — plus everything the same root cause
was quietly breaking elsewhere, and five smaller faults found while reading that
page end to end.

- **PowerShell output was being encoded in the system code page, not UTF-8.**
  Every helper process the panel starts carries `CREATE_NO_WINDOW`, so it has no
  console; with no console `[Console]::OutputEncoding` falls back to the system
  code page, and any character that page cannot encode is replaced with a
  literal `?` *inside PowerShell*. The characters are gone before Rust sees a
  byte, so no amount of decoding brings them back. Measured on zh-CN: the
  adapter `以太网` arrived as `???` and `蓝牙网络连接` as `??????`, which is
  what the outbound-lock dialog was showing. This was never cosmetic — the
  panel sends the name the user ticked straight back as `-InterfaceAlias`, so a
  rule would have been pinned to an interface that does not exist while the UI
  reported `已加 N 条`. Five call sites already carried an inline UTF-8 prelude
  and eleven did not; all sixteen now go through `process::powershell_std` /
  `powershell_tokio`, which pin `-NoProfile -NonInteractive` and the prelude in
  one place, and an architecture test fails the build if a new call site spawns
  `powershell` by hand. Nothing here is specific to Chinese: `é`, `ü` and
  Cyrillic are equally unrepresentable in CP437/CP850. The one exemption is the
  official `install.ps1`, which runs through `-File` — a third-party script
  whose text we do not control, and whose output is English install logging.
- **The upgrade channel selector did nothing.** The plan resource had `latest`
  hardcoded, so picking `stable` and pressing Check still fetched the `latest`
  plan, while Upgrade ran `stable`. The version column would read
  "upgradeable → 2.1.x" and the button would come back "already up to date",
  having installed nothing. The channel now reaches `upgrade_plan`; the
  "don't default to stable, that downgrades" constraint is held by the
  dropdown's default value, where it belongs.
- **`invalidate` did nothing at all to manual resources.** Resources marked
  `auto: false` are not refetched, and the old code only marked them stale — so
  the previous answer stayed on screen, posing as the current one. Two visible
  instances: after an upgrade the version column still read "upgradeable →",
  and after wiping and reinstalling Chrome the privacy audit still listed the
  high-risk permissions of extensions that had just been deleted. A manual
  resource holds a *measurement*; after a destructive operation the honest state
  is "not measured yet", so `invalidate` now drops it (and its persisted copy).
  Where an immediate re-measure is wanted instead — the rule table and the
  privacy audit the user is looking at while pressing the button — the page
  calls `refresh()` explicitly.
- **A failed action left the panel showing a system that had already changed.**
  `act()` only invalidated on success, but adding an outbound lock is one rule
  per selected adapter: when the third fails the first two are already in the
  firewall, blocking traffic, and the rule table showed none of them. It now
  invalidates on both paths, and returns whether it succeeded so the dialog
  stays open on failure instead of discarding the error and the user's
  selection along with it.
- **The Add-rule button could never be pressed on a machine the panel had
  already found Chrome on.** It gated on `browser_audit.chrome_path` alone,
  while the rest of the card falls back through the trace report and the
  software report. Same lesson as the 0.19.1 "Chrome card says not installed"
  fix, one dialog over.
- **Revoking outbound locks reported success for rules it had failed to
  delete.** `remove_all` counted only the successes and swallowed the rest, and
  `remove` itself ran with `-ErrorAction SilentlyContinue`. Since these rules
  outlive the panel, a silent failure means the browser stays blocked by a rule
  the user has just been told was withdrawn. Both now report. The file's own
  test comment — "skipping silently makes a failed revoke look like a
  successful one" — had been enforced for foreign rules only.
- **A `*` in an adapter name widened a delete.** Windows names its Wi-Fi Direct
  virtual adapters `Local Area Connection* 1` (nine of them on the test
  machine), and the alias goes into the rule name, which is what
  `Remove-NetFirewallRule -DisplayName` matches on — as a wildcard. Wildcards
  are now backtick-escaped on the lookup path only; the creation path still
  writes the name literally, and a test pins that the name we create is the
  name we look up.
- **The outbound-lock dialog now says it needs administrator rights**, which
  Windows requires for any firewall change and the panel does not request, and
  that uninstalling Chrome does not take these rules with it.

## 0.19.1

Three defects the user reported against 0.19.0, all on the software page, plus
the design problem sitting behind the second one.

- **A phantom second scrollbar.** `.qb-main` was `position: static`, so an
  absolutely positioned descendant resolved its containing block all the way up
  to `<body>` and escaped the scroll container entirely, landing at its static
  position in document coordinates. The escapee was the `.sr-only` label
  `ExternalLink` renders for screen readers: it sat at document y=1949 and
  stretched a `100dvh` shell into a 1949px document, so the window grew a second
  scrollbar with ~1089px of blank space below the app (software page; the
  settings page was +257px). Fixed with `position: relative` on the scroll
  container, which closes the whole class rather than that one instance. The UI
  regression suite now asserts the document itself cannot scroll — it previously
  only checked `.qb-main` for *horizontal* overflow, which is why this shipped.
- **The Chrome card claimed "not installed" on machines that had Chrome.** It
  read `chrome_installed` only from `claude_traces` / `browser_audit`, both of
  which are `auto: false` and therefore `undefined` until the user presses Scan.
  The panel already knew the answer: `detect::browsers()` resolves Chrome through
  the same `chrome::chrome_exe()` and arrives with the auto-fetched software
  report, which is what the other three cards read. Also, this page never read
  `chrome_scanned` at all, so an absent trace report rendered as a green
  "nothing found" — "not checked" displayed as "no problem", which the modules
  involved explicitly forbid.
- **Privacy-audit rows overflowed their column.** The rows sat in a
  `display: grid` with no `grid-template-columns`, i.e. a single implicit `auto`
  track. An `auto` track is sized to max-content and overflows its container
  rather than forcing its contents to shrink, so each row grew to 325px inside a
  271px column, spilled 57px onto the neighbouring Uninstall column, and the
  value cell's `truncate` never fired because the track was always wide enough.
  Now a plain block container, with `flex-shrink-0` on the key and the action so
  only the value gives way. Demo data gained a `browser_audit` fixture and the
  regression suite gained a flow that presses Scan — without both, those six rows
  were never rendered during a test run.
- **The claude.ai trace scan no longer gives up when Chrome is running.** It was
  wrapped in `if !running`, which in practice meant the feature never ran at all:
  people who care whether their browser holds a claude.ai session keep that
  browser open. The premise was wrong — Windows lets other processes read
  `Cookies`, `History`, `Preferences` and the leveldb files while Chrome holds
  them; only leveldb's `LOCK` is exclusive, and that file is zero bytes and now
  excluded from the target list. `file_contains` had modelled "could not open" as
  `None` since the beginning, so the per-file fallback was already there and the
  outer skip bought nothing. `TraceReport` gained `chrome_files_read` and
  `chrome_files_locked` so "read 53 files, found nothing" and "could not open a
  single file" stay distinguishable; collapsing them into one boolean is the same
  "not checked shown as no problem" failure as above. Verified end to end by
  holding three profile files with an exclusive lock: the scan still ran, still
  found claude.ai in all four profiles, and counted exactly three locked.

## 0.19.0

Four things the user asked for: a real uninstall, startup alignment moved into
Settings, two bounded exceptions to the "no proxy/VPN" rule, and a Chrome privacy
audit. The software page was rewritten around them.

- **Complete uninstall.** `install::purge::plan` is a pure function over injected
  `Facts`; the collector lives in `qb-app` because one inventory has to ask three
  L2 domains at once. It covers program, version store, cache, config, sessions,
  auth, `ANTHROPIC_*` variables, PATH entries, shell config lines, credential
  manager entries, startup items and account slots. An `Item` is structurally
  incapable of holding a key, token or cookie value — only names and locations.
  PATH and shell files are rewritten once per file, all-or-nothing, so a failure
  cannot leave half a profile behind. Execution ends with a re-scan: `Report.left`
  is what is *still there*, not what the commands claimed. That proves "these
  locations are now empty", not "this machine is clean" — WSL and other user
  accounts are out of reach and stay with the prompt in the card.
- **Startup alignment** (timezone, region format, display language) moved out of
  the software page into Settings and now runs when the panel starts. `decide` is
  pure, and its first rule is that a machine already matching gets no action at
  all: "on by default" must not mean "prod the system on every launch", because
  switching a timezone raises UAC and changing the display language needs a
  sign-out. Timezone and region format default on; display language defaults
  **off** — it needs a language pack first. An unknown exit IP or an unmapped zone
  is skipped with a stated reason, never guessed. The hook sits after the
  readiness gate so a first run cannot raise UAC before the user has agreed to
  anything. This is not an anti-ban measure and the UI may not describe it as one.
- **Two bounded exceptions** to the "no built-in proxy/VPN" rule, both decided by
  the user, both written into `CLAUDE.md` and `DISCLAIMER` §5.2 in the same
  change. The browser outbound lock adds Windows Firewall **outbound Block** rules
  scoped to one exe and to the interfaces the user names, all prefixed
  `QB Gate - ` so they are recognisable in Windows' own firewall UI. No inbound
  rules and no Allow rules — outbound is permitted by default, so an "allow via
  TUN" rule would do nothing while looking like it did something. System proxy
  modification runs only on an explicit click and records the original **to disk**
  before touching anything: memory would lose it when the panel closes, and the
  promise is a rollback *in the panel*. Repeated changes keep the earliest backup,
  so "restore" means before the panel first intervened. Neither feature has a
  timer or a startup hook. Firewall rules outlive the panel and must be revoked
  before uninstalling QB Gate.
- **Chrome privacy audit.** WebRTC policy read/write, DoH read, and an extension
  high-risk permission audit that understands both MV2 (host patterns mixed into
  `permissions`) and MV3 (`host_permissions`). Reading only one shape is the
  classic false negative, and a false negative here means the panel tells you an
  extension is fine. Extensions are reported and never disabled — the module has
  no function to disable one. Policy is written to HKCU (per-user, no admin) but
  read from HKCU **and** HKLM with the source reported, so a machine-wide policy
  cannot masquerade as absent. A registry value is not proof a policy is in
  effect; the panel cannot read `chrome://policy` and says so rather than
  claiming otherwise.
- **Software page rewritten** around "which program", not "which operation" —
  uninstall used to be scattered across three different cards. Four cards, three
  fixed columns each (version, gate, uninstall), with Chrome swapping the gate
  column for the privacy audit. No fabricated counts: a card reports how many
  locations exist only after a real read-only scan has run. `unchecked` findings
  are always rendered, because "not checked" must never look like "nothing found".

Rust tests: 787. Frontend: 35 unit tests, plus 72 responsive/theme routes and 6
interaction flows in the UI regression.

## 0.13.0

The Rust side is a Cargo workspace. Layering is now the compiler's job, not a convention.

- Split 28,782 lines across 14 crates: `qb-foundation` and `qb-contract` (L0),
  `qb-platform` (L1), nine domain crates (L3), `qb-app` for cross-domain
  orchestration (L4), and `src-tauri` (L5). `src-tauri` went from 28,782 lines to
  2,747 and `lib.rs` from 1,557 to 296; its 100 commands moved into eight files
  under `commands/`.
- `tauri` now appears in exactly one `Cargo.toml`. Domain code can no longer write
  `fn run_watchdog(…, app: tauri::AppHandle)`, because that type is not in scope —
  which is why `gate/mod.rs` had 976 lines and zero tests. Domain crates take
  `ProgressSink` / `EventSink` / `Clock` instead. Their tests no longer link tao
  and wry, so the `0xc0000139` crash documented in `events.rs` is gone.
- Circular dependencies went from 15 pairs to none. An 11-module strongly
  connected component (`domain → workspace → repository → profile → snapshot →
  gate → repository`) only became visible during the split; pairwise detection had
  been green the whole time. Nine ratchet tests in `src-tauri/tests/architecture.rs`
  hold the line, each with a counter-example fixture, because a broken checker and
  a clean codebase look identical.
- `gate/mod.rs` has 10 tests where it had none, written by extracting the pure
  half rather than mocking I/O. `managed_set_dir`'s 133 lines of preconditions
  became `managed::plan_relocate`, and the session check that `clear_for_switch`
  had inlined twice became `killswitch::needs_clearing` — two copies always
  diverge, and the symptom there would have been "validates one set, stops
  another" with nothing logged.
- `GateError` crosses IPC as `{kind, message}`. 95% of its 417 construction sites
  had collapsed into `Other(String)`, and `Serialize` flattened everything to a
  bare Chinese string, so the frontend could only branch on `.includes()`. SQLite
  errors get their own `Database` variant instead of being downgraded.
- Every IPC type is generated by ts-rs; `api.ts` went from 75 hand-written
  `interface` declarations to zero. Swapping them in exposed four silent contract
  drifts and one real bug: `KillTarget.process_created` is a Windows FILETIME, and
  crossing IPC as a JSON number silently truncated it — that value is how
  `terminate_verified` decides whether a PID has been reused.
- Two IPC clients became one `call()` and one demo fixture. Event channel names
  live in `qb-contract::channels`; a typo used to compile and simply emit into
  the void.
- `panic = "unwind"` with a hook that writes the stack to disk, `tracing` with the
  existing audit log as one of its layers, and CI gates for `cargo-deny`,
  `cargo audit`, `npm audit` and dependabot.
- `settings` caches in-process and invalidates on write — `gate/targets.rs` was
  re-reading and re-parsing `settings.json` on the watchdog's 15-second hot path.
  No TTL: this setting decides which files the gate locks.
- `checkup::scan` (five `reg.exe` calls) and `accounts_switch` (three `mklink`
  calls) moved into `spawn_blocking`. They had been blocking a tokio worker, which
  showed up as the panel freezing during a checkup or an account switch.
- 462 lines of orphaned code in `station/model.rs` and `health/window.rs` were
  registered; their nine tests had never run. `relay.json` is no longer a second
  source of truth — `relay_list` and `relay_fetch_models` are gone, and it is read
  once during migration.

Behaviour is unchanged by design: this release is a structural move, so the gate,
account switching, installation, upgrades and the extension centre should behave
exactly as they did in 0.12.2. Rust tests went from 343 to 493.

## 0.12.2

Text no longer escapes its box.

- The execution-lease tile printed `lease.holder` verbatim. That string is every
  holder joined by `、`, and most holders are session ids
  (`20260913-045632-ef2e6a09…`, 48 characters with no break opportunity): five
  live sessions plus the desktop client came to three hundred characters spilling
  sideways out of a quarter-width tile. All four places that render it now go
  through one formatter (`src/lib/lease.ts`), which shortens a session id to its
  timestamp and says "已租给 <first> 等 N 个" with the full list on hover.
- `.btn` carried no padding or font size of its own — those lived only in the
  `--sm`/`--md` modifiers. Hand-written `<Link className="btn">` (three of them,
  in the DNS-leak and Chinese-environment panels) therefore rendered with zero
  padding and inherited type, so the label sat flush against the border. The base
  class now carries the `--md` values; the modifiers still override.
- `.metric-v`, `.metric-k`, `.row-main`, `.bullet` and `.notice` get
  `overflow-wrap: anywhere`. `min-width: 0` alone does not shrink a flex
  container below its longest unbreakable word, which is what Windows paths,
  environment-variable names and session ids are.
- Removed a duplicated external-link icon in Settings → About.
- `npm run test:ui` now checks, on every route and inside all six overview
  modals, that no element sticks out of its parent, and stress-tests each
  metric/row/bullet with a 48-character unbreakable token. The old check only
  looked at whether `.qb-main` scrolled horizontally — `.modal` is
  `overflow: hidden`, so text that escaped inside one changed nothing it could
  see.


## 0.12.0

- Rebuilt navigation around the workspace, official accounts, relay environments, extensions, environment/gate, and settings while preserving the warm visual identity.
- Added Router Hash routes, TanStack Query state, global search, persistent in-session drafts, task history and explicit cancellation for network checks.
- Added SQLite metadata migration, DPAPI key references, independent Claude Code/Codex relay directories, launch plans, and verified Windows sessions.
- Added reviewed configuration writes, change detection, journal recovery, configuration rollback, and a usable startup recovery view.
- Blocked account changes on failed or unverified process cleanup; added single-instance protection and coordinated mutations.
- Reworked snapshots and complete-unit version rollback, including Codex helpers and installation records.
- Replaced the Tavern-first shop with a catalog for application integrations, MCP, Skills and templates, with explicit imports and per-environment installation.
- Added diagnostic protocol checks and redacted request export, dependency notices, source checks, CI validation, and migration documentation.


### Fixed before release (source review)

- Reconnected the watchdog to `gate::watchdog::decide`. The supervisor had grown its own copy of the policy, leaving `decide`, `WatchMode` and the grace-period branch reachable only from tests: changing `unknown_grace()` as CLAUDE.md documents would have had no effect. Visibility is now narrowed so an unwired `decide` fails `cargo clippy -- -D warnings`.
- The watchdog now honours the per-mode interval and takes its mode from the lease instead of ignoring the argument; `session_launch` no longer pins every session to the CLI mode.
- Startup recovery no longer aborts on the first unreadable or unsettled journal. Bad records are listed with their paths and reasons, and the recovery view can move just those into `quarantine/` instead of leaving the panel permanently unusable.
- Relay sessions are no longer stopped by the gate. Unlocking and the lease still require a verified exit IP (the Deny ACE is per file, so bypassing it would be a bypass entrance), but an IP change no longer kills relay work and the in-session hook is kept out of relay directories.
- Capped `operations`, `sessions` and `diagnostics` history, and added a retention period for on-disk recovery journals, which each hold a sealed copy of the previous configuration. Live and unverified sessions and journals still referenced by an environment rollback are never removed.
- "Check for source updates" no longer overwrites the curated pins recorded in ATTRIBUTION.md; it reports upstream commits instead, and a single unreachable source no longer aborts the whole check. Results are now shown in the UI.
- Skills can no longer be installed into Codex, which never reads `skills/`.
- Added a way to discard an unverifiable session record, which previously blocked account switching, migration, snapshot restore and version rollback with no in-app remedy.
- User-initiated commands now queue briefly for the coordination lock instead of failing immediately when the watchdog holds it.
- `Session.process_created` crosses IPC as a string; a Windows FILETIME exceeds the JavaScript safe integer range.
- Environment rollback explains when an installed extension has rewritten the same configuration file; removing an environment now also removes its isolated directory and rollback metadata.

The scope and validation limits for this release are recorded in docs/KNOWN-ISSUES.zh-CN.md and docs/REGRESSION-0.12.zh-CN.md.
