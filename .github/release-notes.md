<!-- release-notes: 0.25.3 -->
<!--
  这份文件是「这一版更新了什么」的唯一来源，发版时由 scripts/update-manifest.mjs 读：
  中文那一段进 update.json（面板里「发现新版本」弹窗显示的就是它），中英两段一起进 GitHub Release 的正文。
  每次提版本号都要**整份换成新一版的内容**（第一行的版本号跟着改），`npm run release:check` 会核。
  写法只用三种行：「## 」小标题、「- 」一条、普通一段话。写给使用者看，说大白话。
-->

<!-- zh -->

## 更新内容（相对上一个公开版本 v0.24.8）

- 新增「反重力」（Google Antigravity）：官方账户里多了第三页。Hub 和 IDE 能一键安装，启动前一样要先过 IP 门禁；IDE 可以存多个登录槽位（一个 Google 账户一条）；能看账户档位、AI 积分和 5 小时 / 每周额度（点刷新图标才联网问一次）；自带汉化、自动审批和高危操作拦截（只注入脚本，不改它的任何文件）。
- GPT（Codex 桌面端）：侧栏改名「GPT」，默认也归 IP 门禁管；额度条平时读 Codex 写在本机的记录，不联网，点刷新图标才向官方问一次；不打开微软商店也能装 Codex 桌面端；启动或切换 GPT 账户时只关面板自己打开的那份，不会再「突然弹出第二个窗口」。
- 酒馆（SillyTavern）有三条桥了：Claude（你自己的 bridge.py）、GPT（驱动官方 Codex CLI）、Gemini（驱动官方 Gemini CLI）。端口和模型都在面板里设，Claude 桥的设置页和调用日志也搬进了面板；酒馆启动慢时不会再被面板误杀。
- 用量页重做：能看今天、近 7 天、近 30 天按官方 API 价折算「值多少美元」（不是账单，订阅不按这个收费），还有按天的柱状图、按模型、按账户、最近请求。修了用量卡一直是 0、报错被算成回复、缓存写少算、东八区日期错一天。
- 环境体检更准：不带任何账号信息测 Anthropic 服务通不通；看 claude.ai 的解析有没有被污染；认得出 PAC 和 TUN 代理；查 IPv6 的真实出口；中文环境可以用你的默认浏览器测（原来测的是面板内置的浏览器）。DNS 泄露评分只看连着的网卡，只连 Wi-Fi 不再被扣分。
- 订阅页：各套餐额度换成 linux.do 帖子里网友估算的中间值；等效倍率按 1 美元 = 7 元折算，跟中转站放在同一把尺子上比。
- 软件页修好一批问题：托管安装 / 升级原来一点就一直转圈，现在能用了；Gemini CLI 原来装不上，现在能装；Codex 桌面端、反重力、Gemini CLI 都有了「完全卸载」。
- 新增更新提醒：从这一版起，打开面板时会看一眼 GitHub 上有没有新版本，有就弹窗；点「一键更新」会下载安装包、核对 SHA-256 后自动安装并重新打开面板。设置里可以关掉启动时检查。
- Windows 安全中心可能把面板误报成病毒：补了程序信息和应用清单来降低误报，处理办法见 docs/ANTIVIRUS.zh-CN.md。

## 升级提示

- 这是第一个带「一键更新」的版本：装着 v0.24.8 或更早版本的，需要手动下载安装这一次，之后的新版会在面板里提醒。
- 更新（包括一键更新）时面板会先退出，由面板启动的 Claude 桌面端、对话、酒馆会一起关掉，先把手头的对话存好。

<!-- en -->

## What's new (since the previous public release v0.24.8)

- Google Antigravity support: a third page under Official Accounts. One-click install for Hub and IDE, both behind the IP gate; multiple IDE login slots (one Google account per slot); account tier, AI credits and 5-hour / weekly quota (fetched only when you click the refresh icon); built-in Chinese localization, auto-approval and high-risk command blocking (script injection only, none of its files are modified).
- GPT (Codex desktop): renamed to "GPT" in the sidebar and now under the IP gate by default; quota bars read Codex's own local records by default and only ask the official service when you click the refresh icon; install the Codex desktop app without opening the Microsoft Store; starting or switching a GPT account only closes the copy the panel opened, so no more "a second window suddenly pops up".
- SillyTavern now has three bridges: Claude (your own bridge.py), GPT (drives the official Codex CLI) and Gemini (drives the official Gemini CLI). Ports and models are set in the panel, and the Claude bridge's settings and call log moved into the panel; a slow-starting SillyTavern is no longer killed by mistake.
- Usage page rebuilt: see what today, the last 7 days and the last 30 days would cost at official API prices in USD (not a bill; subscriptions are not charged this way), plus a daily bar chart and breakdowns by model, account and recent request. Fixed: usage cards stuck at 0, errors counted as replies, cache writes under-priced, dates off by one day in UTC+8.
- More accurate environment check: tests whether Anthropic's service is reachable without any account credentials, checks whether claude.ai resolves to a poisoned address, recognizes PAC and TUN proxies, finds the real IPv6 exit, and runs the Chinese-environment check in your default browser (it used to test the panel's built-in browser). DNS leak scoring only looks at connected adapters, so Wi-Fi-only machines are no longer penalized.
- Subscription guide: plan quotas now use the median community estimates from a linux.do thread; effective multipliers are converted at 1 USD = 7 CNY so they compare on the same scale as relay stations.
- Software page fixes: managed install / upgrade used to spin forever and now works; Gemini CLI installs correctly; Codex desktop, Antigravity and Gemini CLI can be fully uninstalled.
- Update notifications: starting with this version, the panel checks GitHub for a newer release when it opens and shows a pop-up if there is one. "Update now" downloads the installer, verifies its SHA-256, installs it and reopens the panel. The startup check can be turned off in Settings.
- Windows Security may flag the panel as a virus (a false positive): added version information and an application manifest to reduce this; see docs/ANTIVIRUS.zh-CN.md.

## Upgrade notes

- This is the first version with one-click updates. If you have v0.24.8 or older, download and install this version manually once; later versions will be offered inside the panel.
- Updating (including one-click updates) closes the panel first, together with the Claude desktop app, conversations and SillyTavern it started. Save your work before updating.
