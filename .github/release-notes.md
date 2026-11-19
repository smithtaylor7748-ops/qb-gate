<!-- release-notes: 0.25.16 -->
<!-- zh -->

浏览器助手重做：插件跟着安装包一起装好，在面板「扩展 → 浏览器助手」里三步装上；配对一次长期有效，面板或浏览器重启后自己连回来。保护改为只拦受保护的网站（默认 claude.ai 等），QB Gate 确认出口 IP 正常才放行，其他网站一律不碰。旧版插件（0.1.0）连不上新面板，请按新页面重新装一次。

中转站页面理顺，版面不变：选线只剩「当前线路、备用线路、自动选线」三样，点「设为当前」才会换线，不会再点一下整行就切走；柱状图能切换看请求数、Token 或花费；当前线路被暂停时直接告诉你原因和改用了哪条备用线路。一次请求默认最多试 2 条线路，项目只管 MCP / Skills / 提示词、不再碰线路，五个软件共用的设置挪到「设置 → 中转站」。

<!-- en -->

The browser helper was rebuilt: the extension now installs with the app, and the panel's "Extensions → 浏览器助手" page walks you through three steps. Pair once and it reconnects by itself after the panel or browser restarts. Protection now blocks only protected sites (claude.ai and similar by default) until QB Gate confirms your exit IP, and never touches other sites. The old 0.1.0 extension can't connect to the new panel; reinstall it from the new page.

The relay page was cleaned up with the same layout: route selection comes down to current route, backup routes and auto-select; routes change only when you click "设为当前", never by clicking a row; the chart switches between requests, tokens and cost; and when the current route is paused, the page says why and which backup is in use. A request tries at most 2 routes by default, projects only switch MCP / Skills / prompts and no longer touch routes, and settings shared by all five apps moved to Settings → Relays.
