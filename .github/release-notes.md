<!-- release-notes: 0.25.14 -->
<!-- zh -->

## Mac 0.25.14 预览版 1

- 出口一致性详情新增四项手动检测：原生 IPv4 / IPv6 UDP 与同族 HTTP 出口对照、Mihomo / Clash 运行规则核验、指定 IP 的 ASN 网络类型以及 Claude 桌面语言配置。
- 修复浏览器 WebRTC 不支持、异常、超时和旧报告可能被误判通过的问题；未知与部分完成独立显示，保留 Mac 原有系统代理、DNS、路由与 IPv6 适配。
- 检测结果只保存在本次面板内存，不记录出口变化历史、不改变网络设置、不用于门禁放行。控制密钥仅用于当次本机请求。
- 适用 Apple Silicon、macOS 13.3+。应用版本为 0.25.14，预览标签为 macos-v0.25.14-preview.1；安装包保持 ad-hoc 签名，尚未 Apple 公证。安装及更新方法见 Mac 安装说明。

<!-- en -->

## Mac 0.25.14 preview 1

- Add four explicit diagnostics: native IPv4/IPv6 UDP versus HTTP exits, active Mihomo/Clash routing rules, ASN network categories for a specified IP, and Claude Desktop locale configuration.
- Keep unsupported, failed, timed-out and legacy WebRTC results unknown, while preserving Mac system proxy, DNS, routing and IPv6 behavior.
- Keep results in memory only, with no exit-change history, network changes or gate decisions. Controller credentials are used only for the current local request.
- Requires Apple Silicon and macOS 13.3+. Application version 0.25.14; preview tag macos-v0.25.14-preview.1. The app is ad-hoc signed and not Apple-notarized. See the Mac installation guide for manual updates.
