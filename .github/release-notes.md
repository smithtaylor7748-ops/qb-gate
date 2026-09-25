<!-- release-notes: 0.25.4 -->
<!--
  这份文件是「这一版更新了什么」的唯一来源，发版时由 scripts/update-manifest.mjs 读：
  中文那一段进 update.json（面板里「发现新版本」弹窗显示的就是它），中英两段一起进 GitHub Release 的正文。
  每次提版本号都要**整份换成新一版的内容**（第一行的版本号跟着改），`npm run release:check` 会核。
  写法只用三种行：「## 」小标题、「- 」一条、普通一段话。写给使用者看，说大白话。
-->

<!-- zh -->

## 更新内容（相对上一个公开版本 v0.25.3）

- 新增 GPT 一键汉化：GPT 页「Codex 桌面端」卡上的「汉化」按钮，把 GPT 设成中文界面 —— 写的是它自己设置里的语言那一项（跟你在它的设置里选中文是同一个值），不改它的任何程序文件。设了仍是英文，是 OpenAI 那边还没对你的账户开放中文，面板会照实告诉你。
- 新增 Claude 桌面端中文界面（扩展里的插件「Claude 桌面端 · 中文界面」，Claude 页「启动」卡上也有「汉化」按钮）：接入开源项目 javaht/claude-desktop-zh-cn，只用它的安全模式；改之前、改之后都核对 Claude 程序文件的哈希和数字签名，一旦变了就自动还原。上游出了新版，打开这个窗口时会提示。应用或还原会关掉 Claude 桌面端（包括 Code 页里的会话），先存好手头的活。
- 中转站的倍率按 linux.do「中转站百科」帖的算法改对了：New API 站点公布的倍率其实是单价（1 = 每百万 token 2 美元），以前被当成「官方价的几倍」，查套路报告里照官方价收费的站被写成「输入 2.5 倍、输出 12.5 倍」，现在换成单价再跟官方价比。每条照官方价收费的线路都挂着的「翻倍 ×5」是误报，改成「输出加价」，只有输出真比官方的价格结构贵时才挂。站点公布的分组倍率现在也读得到了。
- 每个中转站可以填「充值比例」（1 美元额度 = 几元）：智能调度比便宜、「倍率不超过」底线，都先按它折成「每 1 美元牌价付几元」再比 —— 7 元买 1 美元额度、标 0.1 倍的站，其实比 1 元买 1 美元额度、标 0.5 倍的还贵。
- 订阅页的倍率计算器可以填中转站那条线路的倍率：站内额度是按这个倍率扣的，原来没乘这一项，会把中转算贵好几倍。

## 升级提示

- 更新（包括一键更新）时面板会先退出，由面板启动的 Claude 桌面端、对话、酒馆会一起关掉，先把手头的对话存好。
- 中转站线路上原来那枚「翻倍 ×5」会消失；「输出加价」要重新跑一次「查套路」才算得出来。

<!-- en -->

## What's new (since the previous public release v0.25.3)

- One-click Chinese UI for GPT: the Chinese button on the Codex desktop card of the GPT page switches GPT to its Chinese interface by writing its own language setting (the same value you get by choosing Chinese in its settings); none of its program files are touched. If it stays in English, OpenAI has not enabled Chinese for your account yet, and the panel says so.
- Chinese interface for the Claude desktop app (the plugin "Claude Desktop · Chinese UI" under Extensions, also a button on the Claude page's launch card): uses the open-source project javaht/claude-desktop-zh-cn in its safe mode only. The panel checks the hashes and code signatures of Claude's program files before and after, and rolls back automatically if they changed. New upstream releases are shown when you open this window. Applying or reverting closes the Claude desktop app, including sessions on its Code page, so save your work first.
- Relay-station multipliers now follow the method in the linux.do "relay station encyclopedia" post: New API stations publish ratios that are really unit prices (1 = $2 per million tokens), which the panel used to read as "times the official price" — a station charging exactly the official price showed up as "input 2.5×, output 12.5×" in the audit report. They are now converted to unit prices and compared with the official ones. The "×5 doubled billing" badge that every honestly priced route carried was a false alarm; it is replaced by "output markup", shown only when output really costs more than the official price structure. Group ratios published by the station are now read too.
- Each relay station can record its top-up ratio (how many yuan buy $1 of credit). Smart scheduling's "cheap" axis and the "multiplier at most" floor convert every route to "yuan per $1 at list price" before comparing — a station selling $1 of credit for 7 yuan and labeled 0.1× is actually more expensive than one selling it for 1 yuan labeled 0.5×.
- The multiplier calculator on the subscription page takes the relay route's multiplier into account: credit is deducted at that multiplier, and leaving it out made relays look several times more expensive.

## Upgrade notes

- Updating (including one-click updates) closes the panel first, together with the Claude desktop app, conversations and SillyTavern it started. Save your work before updating.
- The old "×5 doubled billing" badge on relay routes goes away; "output markup" is calculated the next time you run an audit on that route.
