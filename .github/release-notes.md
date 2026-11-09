<!-- release-notes: 0.25.12 -->
<!-- zh -->

## 更新内容（相对 v0.25.11）

- 同一对话的并行请求超过线路并发上限时，可按独立开关分流到明确授权的其他线路；手动选线、调度和故障备用的行为统一，并修复切站后仍走旧站等问题。
- IP 锁巡检间隔支持 1–30 秒调整，默认 5 秒；官方账户额度增加重置倒计时，断网时保留已有快照，Gemini 4 等独立额度池分别显示。
- GPT 质量检查新增糖果题、离线模型指纹和动画人工复核，绑定所选账户或线路，支持取消和历史比较。结果仅供判断本次表现，不认证模型身份。
- 修复 GPT / Gemini 酒馆适配，增加反重力原生文本桥接与可选 Codex 会话修复，Claude 专用关闭不再影响其他客户端。
- 用量统计改用完整请求记录，新增明确时间的柱状图，修复请求数受 2000 条日志上限影响；完善 CC Switch 链接导入和依赖安全更新。

<!-- en -->

## What's new (since v0.25.11)

- Add an explicit switch to distribute concurrent conversation requests beyond a route's capacity across approved routes. Align manual selection, scheduling and failover, and fix stale route selection.
- Allow IP checks every 1–30 seconds (5 seconds by default). Show cached quota reset countdowns and separate quota pools, including Gemini 4 when returned by the upstream.
- Add opt-in GPT quality sampling with a math task, an offline model fingerprint and manual animation review, with pinned targets, cancellation and history. Results do not authenticate model identity.
- Improve GPT / Gemini tavern compatibility, add a native Antigravity text bridge and optional Codex session recovery, and keep Claude-only shutdown scoped to Claude.
- Aggregate complete usage records with timestamped bars, remove the 2,000-log limit from request totals, improve CC Switch link import, and update vulnerable dependencies.
