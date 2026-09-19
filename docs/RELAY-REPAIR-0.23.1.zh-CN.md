# 中转启动、站点检验与源码融合说明

日期：2026-09-18；对应 QB Gate 0.23.1。

## 最初的源码有没有融合

`ccodex-sleep-state-main.zip` 已评估，**没有整包合入**，没有运行它的安装脚本，
也没有引入它的 Go 服务、Mihomo 或 turn-state 注入。

本次核对压缩包 SHA-256：
`27ec0649ed492393e7ef644fd5e6b95a56f4739bb17924ecd3658941972a174c`。

它的 README 与实现区分官方 ChatGPT 登录、官方 API Key 和第三方 Responses 中转。
对第三方中转保留其 Key 与上游，明确不注入官方 `X-Codex-Turn-State`。
README 也说明密文长度并非 OpenAI 公布的模型质量指标。

| 部分 | 结论 |
| --- | --- |
| 配置备份、事务更新、不同鉴权隔离、错误可见 | 可以参考思路，在现有 Rust 模块中独立实现；现有项目已有相应机制 |
| 整个 Go 服务与网页 | 技术上可作为独立程序运行，但不适合作为这次故障的修复；尚未集成 |
| Mihomo 出站、订阅和链式代理 | 超出现有 QB Gate 本机路由的边界，不引入 |
| 官方 turn-state 采集与注入 | 不解决中转 API 查账或 Windows setup 失败，不引入 |
| GPL 源码直接合并 | 按仓库现有双授权规则不直接复制；改变发行方式或另获授权需另行评估 |

本次实际参考并落实的是后来提供的 `station-monitor-standalone.zip` 的鉴权、
登录会话、六次受控请求、缓存复用、单位换算和请求账单配对。Rust 后端按协议适配；
测试材料和本地导入 Worker 来自该源码。cockpit-tools 仅参考
实例管理思路，其 README 当前声明 CC BY-NC-SA 4.0，未复制源码。

## Codex 的 helper_failed

本机官方日志的具体错误为 `helper_sandbox_lock_failed`，
`SetNamedSecurityInfoW sandbox dir failed: 5`。
独立中转目录的旧 `.sandbox-bin` ACL 缺少官方刷新所需的 `WRITE_DAC`。
旧 `setup_marker.json` 又让官方初始化路径跳过完整管理员初始化，反复走普通刷新。

本机修复过程：备份旧标记和配置，将旧标记改名保留，使用当前安装的官方 Codex
`windowsSandbox/setupStart` 执行 elevated 初始化。官方返回 `success: true`；
新进程的 `windowsSandbox/readiness` 返回 `ready`，并在该中转目录下成功执行
本地沙箱输出命令。未关闭沙箱，也未复制其它账户的沙箱秘密。

类似故障再次出现时，应先确认错误确实是上述目录权限错误，再退出受影响的中转实例，
备份其 `.sandbox/setup_marker.json` 并改名保留，重新打开该中转实例完成
“Finish Windows setup”的系统授权。不要把其它账户的 `.sandbox-secrets` 搬过来，
不要把 `danger-full-access` 当成权限修复。

参考：[官方 Windows 文档](https://developers.openai.com/codex/windows/)、
[官方初始化路径](https://github.com/openai/codex/blob/main/codex-rs/core/src/windows_sandbox.rs)、
[沙箱目录权限实现](https://github.com/openai/codex/blob/main/codex-rs/windows-sandbox-rs/src/setup_provisioning.rs)。

## 入口与后台凭证配置

入口：**中转站 → 对应线路的「查套路」**。窗口分为配置与检验、检验报告。

1. 在「连接后台账单」填写该站点网站的账号（Sub2API 为邮箱）与密码，选择自动识别或明确后端，点「登录并读取账单」。程序读取会话、用户 ID、余额与消费记录后才显示成功，不需要手工找后台令牌。
2. 使用 Linux DO 或二次验证时，选择「浏览器 / Linux DO → 打开独立登录窗口」，在本站页面完成登录。仅提取当前站点的会话；窗口没有 QB Gate 命令权限。五分钟未完成或窗口关闭时终止等待。
3. 选择模型，填写**本次临时 API Key**。Key 提交后清空，只用于本轮，不保存在线路、历史、URL 或日志中。后台登录会话及账号密码使用本机 DPAPI 加密保存，可刷新或断开。
4. 默认运行 **1 次预热 + 5 次缓存验证**。使用原源码的自然代码审查材料及五个追问，每批随机标识，同批保留固定前缀。每次输出最多 112 Token，实际费用以站点为准。
5. 如需第 7 次冷前缀对照，勾选该项并单独确认额外费用。新前缀改变段落顺序；不会在默认六次中暗自增加这次请求。
6. 报告包含「基础报告 / 注意报告 / 完整证据」，保留逐次 API/账单 Token、实扣、匹配依据、前后余额和缺失原因。新历史以 DPAPI 加密后的结构化记录保存，兼容旧历史。
7. 检验报告下的「本地账单分析」可导入 CSV/JSON。使用原源码 Worker，本机处理，每文件上限 5 MiB / 50,000 条；不上传，原始 quota 缺少配额换算比例时不自动折算成金额。

## 核心协议与适配边界

- New API：`/api/user/login`，读取 Cookie 与用户 ID；必要时按原源码依次尝试 `/api/user/auth/refresh` 和 `/api/user/token`。后台请求携带 Cookie/访问令牌及 `New-Api-User`。余额从 `/api/user/self` 读取，配额按公开 `quota_per_unit` 换算。
- Sub2API：`/api/v1/auth/login` 的 email/password；access/refresh token。余额读取 auth/me（404 时兼容 user/profile），消费读取 usage；后台模型和分组读取 groups/available、groups/rates、channels/available，原始每 Token 价格换算为每百万 Token。
- 检验前验证后台登录，401 时可使用已加密保存的密码或 Sub2 refresh token 更新会话。登录失败不发模型请求。模型调用与后台登录使用两套独立凭证；禁止携带凭证跟随 HTTP 重定向。
- Codex 使用 Responses；Claude 线路适配 Messages，保留缓存块。gpt-5.6 的显式缓存参数按用户源码发送；只有明确拒绝相关字段的 400/422 才回退自动缓存，网络失败/5xx/流中断不重放。
- 账单轮询按原源码最多七次 1 秒、七次 10 秒，只重复读取。首先以请求 ID 匹配新的同模型、同分组账单；缺少相同 ID 时，只有 Token 在两边都唯一才标为较弱的「仅 Token」匹配。存在歧义则不分配。
- 整轮实扣倍率要求全部请求及账单完整，且请求 ID 相同；官方成本来自同批 API Token，而非站点公布倍率。Token 匹配的弱证据不回写调度倍率。站点公布的分类价格单独呈现。
- 五次前缀平均复用扣除预热时已有的背景缓存，材料不足 1024 Token 时不给复用结论。低于源码 90% 参考线提示复核，不直接判定造假。
- 原压缩包引用但未包含 tools/linuxdo-oauth-broker.mjs。本项目用独立 WebView2 登录窗口实现同一用户流程，用户在站点本身点击 Linux DO；未猜测 OAuth 回调或复制 Node 浏览器代理。

## 验证范围与限制

模型身份、全站成功率、上下文上限和最大输出不由本轮测试推断。API 用量、账单和余额均由站点提供；同账户其它消费、充值、延迟入账会影响余额差额。按用户指定口径，账单、倍率和余额差额直接比较数值，忽略货币单位，不做汇率换算；New API 原始配额仍先除以 quota_per_unit 得到账单金额。

健康度为最近一页、最多 100 条的时间窗样本。模型无参考价格、字段缺失、登录失效、Cloudflare 拒绝访问等均保留明确原因。

免费诊断曾出现 Cloudflare 403，后续公共价目表已恢复 200；未把站点访问状态与已修复的本机沙箱权限故障混为一谈。本次没有使用真实凭证发送付费检验；需要用户在新界面登录自己的站点并提供临时 Key 后运行。

回归覆盖登录 Cookie/刷新兼容、Sub2 email 协议、鉴权分离、六次受控请求、唯一账单匹配、401/重定向停止计费、临时 Key 清空和第七次费用确认。另验证响应式主题、报告切换与本地账单 Worker。
