<!-- release-notes: 0.25.15 -->
<!-- zh -->

修复「一键关闭所有 Claude」和 IP 锁关闭 Claude 时频繁报错、关不干净的问题：已经在退出或已经退出的进程不再被当成没关掉，Claude 页的关闭中途出错也会把其余步骤做完。一键关闭改为「宁可错杀，不可放过」，会连同 Claude 拉起的 MCP 服务器、命令行和它启动的工具一起关掉；GPT、Gemini、反重力和共用的酒馆不受影响。

中转站不再受官方账户 IP 锁的约束：不开代理也能启动中转，出口 IP 变化不会关掉中转会话或影响其中的搜索功能，官方账户的 IP 锁保持不变。

新增 Chrome 浏览器助手：与面板配对后可单独为当前 Chrome 资料开启 IP 保护；查套路可复用 Chrome 中已登录的 New API / Sub2API 账户，并读取站点公开的模型价格。

<!-- en -->

Fix frequent errors and incomplete shutdowns when closing Claude with "Close all Claude" or the IP lock: processes that are already exiting or gone no longer count as failures, and the Claude page finishes every remaining step even if one fails. "Close all Claude" now leans toward closing too much rather than too little, and also stops the MCP servers, terminals, and tools that Claude started; GPT, Gemini, Antigravity, and the shared tavern keep running.

Relay stations are no longer bound by the official-account IP lock: relays start without a proxy, and an exit IP change no longer stops relay sessions or breaks their search. The IP lock for official accounts is unchanged.

Add the Chrome browser helper: once paired with the panel, it can turn on IP protection for the current Chrome profile, and the plan checker can reuse New API / Sub2API accounts already signed in to Chrome and read the public model prices those sites publish.
