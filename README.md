# QB Gate

因为有些工作需要频繁变动 IP，而电脑上正在运行的 AI 软件因为频繁 IP 变动而极易导致风控，所以推出了这款软件。

**完整开源、没有未开源的部分**：面板的全部源码都在这个仓库里 —— 没有闭源模块、
没有预编译二进制、也没有本项目自己的服务端；安装包由 GitHub Actions 从本仓库源码构建。

[![CI](https://github.com/smithtaylor7748-ops/qb-gate/actions/workflows/ci.yml/badge.svg)](https://github.com/smithtaylor7748-ops/qb-gate/actions/workflows/ci.yml)
[![LINUX DO](https://img.shields.io/badge/LINUX-DO-FFB003.svg?logo=data:image/svg%2bxml;base64,DQo8c3ZnIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgd2lkdGg9IjEwMCIgaGVpZ2h0PSIxMDAiPjxwYXRoIGQ9Ik00Ni44Mi0uMDU1aDYuMjVxMjMuOTY5IDIuMDYyIDM4IDIxLjQyNmM1LjI1OCA3LjY3NiA4LjIxNSAxNi4xNTYgOC44NzUgMjUuNDV2Ni4yNXEtMi4wNjQgMjMuOTY4LTIxLjQzIDM4LTExLjUxMiA3Ljg4NS0yNS40NDUgOC44NzRoLTYuMjVxLTIzLjk3LTIuMDY0LTM4LjAwNC0yMS40M1EuOTcxIDY3LjA1Ni0uMDU0IDUzLjE4di02LjQ3M0MxLjM2MiAzMC43ODEgOC41MDMgMTguMTQ4IDIxLjM3IDguODE3IDI5LjA0NyAzLjU2MiAzNy41MjcuNjA0IDQ2LjgyMS0uMDU2IiBzdHlsZT0ic3Ryb2tlOm5vbmU7ZmlsbC1ydWxlOmV2ZW5vZGQ7ZmlsbDojZWNlY2VjO2ZpbGwtb3BhY2l0eToxIi8+PHBhdGggZD0iTTQ3LjI2NiAyLjk1N3EyMi41My0uNjUgMzcuNzc3IDE1LjczOGE0OS43IDQ5LjcgMCAwIDEgNi44NjcgMTAuMTU3cS00MS45NjQuMjIyLTgzLjkzIDAgOS43NS0xOC42MTYgMzAuMDI0LTI0LjM4N2E2MSA2MSAwIDAgMSA5LjI2Mi0xLjUwOCIgc3R5bGU9InN0cm9rZTpub25lO2ZpbGwtcnVsZTpldmVub2RkO2ZpbGw6IzE5MTkxOTtmaWxsLW9wYWNpdHk6MSIvPjxwYXRoIGQ9Ik03Ljk4IDcwLjkyNmMyNy45NzctLjAzNSA1NS45NTQgMCA4My45My4xMTNRODMuNDI2IDg3LjQ3MyA2Ni4xMyA5NC4wODZxLTE4LjgxIDYuNTQ0LTM2LjgzMi0xLjg5OC0xNC4yMDMtNy4wOS0yMS4zMTctMjEuMjYyIiBzdHlsZT0ic3Ryb2tlOm5vbmU7ZmlsbC1ydWxlOmV2ZW5vZGQ7ZmlsbDojZjlhZjAwO2ZpbGwtb3BhY2l0eToxIi8+PC9zdmc+)](https://linux.do)

---

## 怎么解决

核心是一道 IP 门禁：出口 IP 不在你的白名单里，AI 软件就启动不了；运行中 IP 变了或查不到，立即上锁并关闭受管会话。下面按侧栏菜单简单介绍。

### 官方账户

侧栏里分 Claude 与 Codex 桌面端两边。

- **综合评分**：IP 纯净度、DNS 泄露、中文环境、IP 锁、出口一致性五项检测，一键全面体检，点任一格在小窗里就地处理。IP 纯净度里的「禁用本机 IPv6」默认开启。
- **门禁**：IP 白名单（可加国家规则）配合执行锁，白名单之外的出口 IP 起不了 Claude；看门狗在放行期间每 15–20 秒复核一次，不合格或查不到就上锁并关闭受管会话；会话内门禁（每次请求前再验一次）默认关闭。
- **账户槽位**：新建、切换、删除你本人的登录槽位；显示套餐、凭证剩余天数、5 小时与 7 天额度和 token 用量（只读本机文件）；检查官方目录里残留的 API 配置。
- **启动与关闭**：验过出口 IP 再启动 Claude Code、Claude 桌面端、酒馆；一键关闭所有 Claude。
- **Codex 桌面端**：官方登录槽位、手动切换、本机用量。

### 中转站

- 智能调度（内测中：界面里已经出现，但目前还不能正常使用）。

### 软件

- **托管安装**：Claude Code、Codex CLI 从官方源下载，核对 SHA-256 与数字签名后装进托管目录，装完自动上锁；Claude 桌面端可重新安装。
- **升级与回滚**：可选最新版或稳定版，旧版本收进版本库（最多 3 份），随时回滚。
- **门禁范围**：Claude 的执行锁始终开启；Codex 可以选择纳入 IP 门禁（默认关闭）。
- **清理与卸载**：扫描面板之外的多余副本；完全卸载先只读盘点，手动输入「卸载」才执行；另附给其他 AI 用的卸载提示词。
- **Google Chrome**：隐私审计（只读）、修改系统代理、浏览器出站锁（都是点了才执行、可以撤销），以及完全卸载（会删除全部浏览器数据）。

### 扩展

- **酒馆（SillyTavern）**：接入你自己安装的酒馆，自动定位安装位置，管理桥接、启停、角色资产与备份。
- **MCP、Skills、配置模板**：导入并预览差异后装进指定环境，连接测试由你手动发起。

### 设置

- **常规**：外观主题；网络恢复后重新放行；关闭 Claude Code 非必要遥测；启动时按出口 IP 对齐系统时区与区域格式（默认开启）、显示语言（默认关闭）。
- **备份与恢复**：创建与恢复配置快照。
- **帮助与来源**：新手引导、许可与第三方来源、问题反馈、发行版本。
- **高级维护**：更改托管目录、恢复系统时区。

---

## 社区与反馈

这个项目从 **[LINUX DO](https://linux.do)** 起步，发布和更新也在那边 ——
谢谢愿意在自己机器上实机试、愿意把报错原样截图贴出来的佬友，
README 里那些「不显然但很贵」的分支，多半是这么来的。

想报 bug、提功能、问「装不上怎么办」，两条路：

| 去哪 | 适合什么 |
|---|---|
| [GitHub Issues](https://github.com/smithtaylor7748-ops/qb-gate/issues) | 能复现的 bug、功能提案 —— 有编号、能追溯、修完对得上版本 |
| **QQ 群「门禁值班室」**：`1109462206` | 还说不清的现场问题、装不上、想法没成形时先聊两句 |

群里聊明白的问题**最后还是要落一条 Issue** —— 聊天记录会被刷上去，Issue 不会。

贴日志和截图之前，**把出口 IP、账户邮箱、Token 打码**：纯净度页和账户卡上
就是你的真实信息。

---

## 免责声明

使用前请完整阅读 [免责声明 DISCLAIMER.md](DISCLAIMER.md)。要点：

- 非官方项目，与 Anthropic、OpenAI 等任何服务商都没有隶属、合作或背书关系。
- **不保证不被风控，不承诺防封。** 本软件只管住你本机的 AI 软件，不改写设备指纹、不伪装账户身份，不为绕过任何服务商的风控、地区限制或封禁而设计，也做不到。
- 不含代理、VPN 或翻墙功能。网络接入是否合法，由使用者自行负责。
- 只能用于你本人合法拥有的账户，并遵守所在地法律与各服务商条款。账户切换只能手动触发，同一时刻只有一个账户激活；不联网查额度（额度读数只来自本机文件，仅用于显示），也没有按限流、429 或额度自动换号的路径。
- 部分功能会修改系统设置（IPv6、时区与区域格式、防火墙规则、系统代理等）或永久删除数据，执行前请看清提示。
