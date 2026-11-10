<!-- release-notes: 0.25.14 -->
<!-- zh -->

新增按需环境检测：UDP / HTTP 出口对照、代理运行规则、ASN 网络类型和 Claude 客户端语言配置；修复 WebRTC 超时或异常被误判为通过的问题。检测结果不保存出口变化历史，也不参与门禁放行。

修复 Claude Code 原生更新时下载文件被占用的问题，并补充 Claude 酒馆内置隐私插件兼容处理。

<!-- en -->

Add manual environment diagnostics for UDP / HTTP egress, proxy runtime rules, ASN network type, and Claude client language. Fix WebRTC timeouts and errors being treated as successful checks. These diagnostics do not retain egress history or authorize gate access.

Fix download-file contention during native Claude Code updates and add compatibility handling for the built-in Claude tavern privacy plugin.
