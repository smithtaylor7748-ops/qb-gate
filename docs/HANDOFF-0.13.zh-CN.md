# QB Gate · 接着做（v0.15.0 之后）

记录日期 **2026-09-14**。仓库 `D:\claude-gate`，分支 `codex/qb-gate-rebuild`。

**先读三份东西，顺序不要换：**

1. 仓库根的 [`CLAUDE.md`](../CLAUDE.md) —— 硬规矩，每条都是踩出来的
2. 本文档
3. 中转站定稿草图：**https://claude.ai/code/artifact/4ec59b26-007d-4de1-8fb0-90f8257f7b60**

---

## 零、当前状态（全部实测，不是照记忆写的）

| | |
|---|---|
| 版本 | **0.15.0**，五个文件六处版本号一致（`release:check` 是权威，不是任何一张表） |
| 提交 | `2075ebf` 交接档案 · `ba7e4de` 交接档案 · `4cc3f89` 文档与截图。**0.13.1 / 0.14.0 / 0.15.0 三轮的改动都还在工作区，未提交** |
| 本机 | **仍装着 0.13.0**（注册表实测：`DisplayVersion 0.13.0`，`uninstall.exe` 也是 0.13.0）。0.14.0 的包出过但从没装上。卸载登记只有一条（`QB Gate`）。装机由使用者自己点 —— 会话跑在门禁之下，不能自己装 |
| 门禁 | `cargo test --workspace` **684 过**（0.14.0 时 595，0.13.0 时 493）；clippy / fmt / cargo-deny 四项 / build / test / types:check / format:check / test:ui（72 路由 + 6 流程）/ release:check 全绿 |
| 安装目录残留 | `%LOCALAPPDATA%\QB Gate\export-types.exe`（0.12.0，312 KB）—— v0.12.0 第一版安装包误打进去的开发工具。后来的包已不含它，**而 NSIS 卸载器只删自己装过的文件，所以它会一直留着**。手工删 |
| 装机验收 | 上锁解锁 10 个 → 租约按原样接回；`allowlist.txt` / `progress.json` / `settings.json` 一个都没被动过 |

> 装完那天 `lease.json` 的 `granted` 从 10 变成 11 —— 不是回归。多出来的是
> `AppData\Roaming\Claude\claude-code\2.1.270\claude.exe`，Claude Code 自己更新出来的新版本目录，
> 门禁按 `inventory.rs` 的规矩把它也锁上了，这是对的。

---

## 一、⛔ 最容易误会的一件事

**这一节在 0.13.0 时说的是「中转站新界面一行代码都没写」。那句话现在不成立了 ——
0.14.0 / 0.15.0 两轮把四个分页全做完了。** 下面这段留着，是因为它记的判断方法仍然有用。

现在的实际情况（0.15.0，`grep` 实测）：

| 草图上的东西 | 现在在哪 |
|---|---|
| 中转站账户 / 智能调度 / 站点检验 / 请求日志 | `src/features/station/` 四个组件 + `StationCenter.tsx` |
| 线路池、排序、熔断 | `crates/qb-station/src/schedule/` · `station/route.rs` |
| 计价与「计费翻倍」 | `crates/qb-station/src/station/pricing.rs` |
| 检验轮次与证据档次 | `crates/qb-station/src/station/audit.rs` |
| 本机路由 | `crates/qb-app/src/local_router.rs` |

旧的「服务商 / 凭证 / 环境」那一套**没有删**，它退到第五个分页
（`RelayCenter.tsx` 的 `shell === "legacy"`）。**先别删它**：站点还得从那儿添，
§4.7 那个 880px 的添加/编辑弹窗还没做，删了四个分页就没有数据可显示。

### 留下来的方法：别信文档，去 grep

这一节原来之所以有价值，是因为它把「文档说做了」和「代码里真有」分开了。
接手时先跑一遍，对不上就以代码为准：

```bash
grep -rn "请求日志\|中转站账户\|智能调度\|站点检验" src/ | head
```

草图是 **700 行原生 JS + 写死的假数据**，仓库那边是 React + TS + ts-rs 生成类型 + SQLite。
**不能粘过去** —— 这一条到现在仍然成立，§4.7 那个弹窗还等着按这个规矩重写。

---

## 二、架构升级：做了什么、没做什么

### ✅ 已完成（都在 0.13.0 里）

| 阶段 | 实测结果 |
|---|---|
| **W 拆 workspace** | **13 个 crate**；`src-tauri` 28,782 → **2,732 行**；`lib.rs` 1,557 → **311 行**；100 个命令进 `commands/` 9 个文件 |
| | `tauri` 只出现在 **`src-tauri/Cargo.toml` 一处** —— 领域代码物理上写不出 `AppHandle` 签名 |
| **A 断环** | `architecture.rs` 的 `KNOWN_CYCLES = &[]`，**空的**。棘轮同时断言「不许出新环」与「名单不许留多余项」 |
| **W4 分层棘轮** | 9 条测试。**不许删**，理由见 `CLAUDE.md` |
| **S0 止血** | `panic = "unwind"` + `panic_hook.rs` + `tracing`；CI 有 cargo-deny / npm audit / dependabot |
| **E0** | 领域层不认识 `AppHandle`，收 `ProgressSink` / `EventSink` / `Clock` |
| **E1** | `ErrorKind` **11 个变体**，过 IPC 是 `{kind, message}`。前端按 kind 分支，**不许匹配中文** |
| **E2** | `gate/mod.rs` 0 → 10 条测试；`managed::plan_relocate`；`killswitch::needs_clearing` |
| **B0/B1** | `api.ts` 手写 interface **0 个**（原 75），`generated/` **102 个**全量 ts-rs |
| **B2/B4** | 一个 `call()`（`src/lib/ipc.ts`）；事件通道名在 `qb-contract::channels` |
| **C4 / F** | settings 进程内缓存 + 写时失效；`checkup::scan`、`accounts_switch` 进 `spawn_blocking` |
| **D1 令牌合并**（0.13.1） | 颜色与主题只在 `tokens.css`，`workspace.css` 只剩布局。留的是 workspace 那套配色 —— 使用者定的，对比度也更高（`--text-3` 压 `--surface`：5.01/6.11 对 3.64/3.43）。浅色写两遍（媒体查询 + 属性选择器），**必须一字不差** |
| | 守卫：`ui-regression.mjs` 逐个令牌比对「跟随系统」与手选主题。**已做变异验证** —— 只把媒体查询那块的 `--surface-2` 改掉、手选那块不动，当场红在「跟随系统 + 系统浅色，必须与手选浅色逐个令牌一致」 |
| **D6 拆 chunk**（0.13.1） | 六页各一个 `React.lazy` chunk，模块加载时排一个空闲回调全部预取。单 chunk 469 KB → 入口 366 KB。`Suspense` 在带 key 的 `Boundary` **外面**，否则切页每次都亮 fallback |

### ⬜ 没做的三块

**C 注册表化**

- **C0**：四份客户端枚举仍并存 —— `Client`(qb-contract/domain.rs:7) · `InstallTarget`(qb-install/winget.rs:48) ·
  `LaunchTarget`(qb-launch/launch.rs:13) · `RelayTarget`(qb-relay/mod.rs:51)，都是同样那三个客户端
- **C1**：`crates/qb-extensions/src/extensions.rs` 里 **6 处** `if kind == ExtensionKind::Skill { … } else { MCP 分支 }`
  —— **加第五种扩展类型会被静默当成 MCP 安装**，编译器不报错。要改成穷尽 `match`
- **C2**：插件机制是假的 —— `src-tauri/src/commands/plugins.rs:14` 写死 `vec![sillytavern::status()]`，
  前端 `src/plugins/registry.ts:24` 同样写死
- **C3**：没有迁移框架 —— `repository.rs:65` 的 `PRAGMA user_version=1` 恒定，第二次迁移要手写 `if version == 1`

**D 前端收口**

- **D0**：迁移停在半路 —— `src/pages/` 还剩 `Environment.tsx` `Settings.tsx` + `accounts/` `home/` `managed/` 三个目录；
  `qb-legacy-detail` 桥接 3 处
- **D2/D4/D5**：三种写样式的方式并存；`tasks.ts` vs `operations` 两套任务模型；加新页面仍是 14 处
- **D3**：缩小了 —— `PageHeader`(3) `ProgressBar`(5) `ConfirmDialog`(8) 都在用了，只剩
  `Field.tsx`（220 行 5 导出，只有 `TextField` 被用过 1 次）和 `Skeleton`（0 处）

**G 诊断包** —— 零。注意 `qb-app/src/diagnostics.rs` 是**中转站的探针请求**，不是「导出诊断包」。

**B2 剩下的一半** —— `src/ipc/` 不存在，`api.ts` 未按域拆。优先级低（已从 900 行降到 373 行）。

### 明确不做（写下来免得重新讨论）

❌ i18n（7.7 万字中文已硬编码在 TSX 里，这是决定不是遗漏）
❌ 安全不变量 property test（E2 的常规单测已覆盖）
❌ 设备指纹伪装 / 内置代理 / 自动换号 / 联网查额度（见 `CLAUDE.md`）

---

## 三、建议的下一步顺序

**D1 + D6 已经做完（0.13.1）** —— 它们本来就该排在 P1 前面：中转站要新建四个分页，
在两套打架的令牌上画新界面等于把问题翻倍，往 469 KB 单 chunk 再压四页会明显拖慢首屏。
现在这两笔一次性成本已经付过了。

**⛔ 照中转站草图（V24）做页面时不要从草图里抄 hex** —— 草图用的是合并前的旧配色。
颜色一律引用 `tokens.css` 的变量；`ui-regression.mjs` 只看得见 `:root` 上的变量，
组件里直接写 hex 它拦不住。

所以下一步直接进 **P1**，之后 P2 → P3 → P4。C 系列和 G 可以往后放；C1 那 6 处是真隐患，但只在「要加第五种扩展」时才咬人。

---

## 四、中转站：草图已定稿，设计决策全记录

草图 **https://claude.ai/code/artifact/4ec59b26-007d-4de1-8fb0-90f8257f7b60**（Version 24）
经过使用者十几轮逐条确认。**下面每一条都是他明确定过的，不要自作主张改。**

### 4.1 核心模型

- 一条**线路 = 站点 + 分组**。**一个分组一把 key，没有第三层**（使用者原话：多把 key 绑一个分组「没有」）
- **站点级**：域名、账号、**余额（整站共享）**、后端类型
- **分组级**：倍率、健康度、那把 key、**24h 实扣**
- 分组不是按模型分的，是**按通道档次**分的 —— 同一个模型会同时出现在「官方直连组」和「低价组」里
- 添加是两级：站点添加一次，之后在站点底下**一把一把加 key**
- ⚠ 现在数据模型里**「分组」还没有独立字段**。`Provider`(站点) → `Credential`(key) → `Environment`(客户端配置)，
  分组名得加在 `Credential` 上（或 `label` 兼任），分组级倍率也要加

### 4.2 四个分页

**中转站账户 · 智能调度 · 站点检验 · 请求日志**（原「路由总览」已砍；「测速测活」并入账户页卡片详情）

账户页顶部分页 = **客户端**（Claude Code / Claude 桌面端 / Codex），线路出现在哪一页**只看协议**：
`anthropic` → 前两页，`openai`/`responses` → Codex。每页顶上**一个启动条，整页只有这一处能启动** ——
本地路由 `127.0.0.1:15721` 的意义就是启一次、之后换上游不重启。

### 4.3 ⛔ 智能调度算法（这一节最贵，是实测选出来的）

**规则：勾的几项里，最弱的那一项最好的那条赢。**

- **三项可单选也可多选**：便宜 / 快 / 稳（复选框，不是推子）
- **归一化用「跟池里最好那条的比值」**，不是池内 min-max
- **两条可选底线**（默认不设）：成功率 ≥ ___、首字 P95 ≤ ___、**倍率 ≤ ___**。先筛掉再排序
- **迟滞 8%**：挑战者要领先现任 8% 才换，理由明写在界面上 —— 换上游会作废 prompt 缓存
- 某一维全场差距太小就**如实说「分不出高下」并不参与排序**，不假装它在起作用
- 底线卡到一条不剩时**不许死局**：照常走，但每行标「没过线 + 差在哪」

**为什么不是加权平均**（原方案是三根推子）—— 三条实测：

1. 三条线稳定度 91/92/92 时，把「稳定」从 0 拉到 100，**排名一个字不变**，
   而界面写着「稳定占 50%」。推子在撒谎
2. 没有迟滞时，两条几乎持平的线路在 **200 次请求里换手 53 次**，prompt 缓存全废
3. 稳 60（四成请求失败）配速度满分，跟稳 95 那条只差 **0.5 分** —— 可靠性被定价了

**为什么不是名次制（Borda）** —— 实测【便宜+快】它选了**最慢**的那条（便宜第 1 · 快第 4）。
一项第一能压过一项垫底，跟加权和同病。

**为什么归一化不能用 min-max** —— 两条线的池子里它**永远输出 1.0 和 0.0**，
不管实际差 0.1% 还是 10 倍，差距每次被重新拉满，**迟滞门槛永远失效**
（实测 0% / 5% / 15% 三档换手次数一模一样）。换成比值后：`0% → 53 次，3% → 29 次，8% → 4 次`。

**预设**不再是魔法数字：写代码 = 稳+快 · 首字 ≤3s；跑批量 = 便宜 · 成功率 ≥95%；对话 = 快 · 成功率 ≥99%。

**⚠ 还欠一条**：「便宜」现在算的是倍率，**没算缓存命中**。一条 ×0.20 但缓存全废的线，
实际比 ×0.35 有 70% 命中的贵。P1 接真实账单后改成 `24h 实扣 ÷ 实际 token`。

### 4.4 真实倍率与「造假不等于拉黑」

- 每次检验记一个 `mult`（实扣是标称的几倍），**真实倍率 = 标称 × mult**
- `mult` 放在**每一轮**上不是线路上 —— 历史里能看出这家是越来越离谱还是收敛了
- 账户页卡片的「倍率」格三态：造假 → ~~×0.20~~ **×0.26** `实测`；检验过属实 → `已核实`；没检验 → `标称 · 未核实`
- **底线按真实倍率比**，不按站点自己标的
- ⛔ **造假的站不拉黑** —— 真实倍率仍然过线的照样留在池子里。
  这是 `DISCLAIMER` 那条「只检测、只如实报告 —— 不拉黑、不替你换站」的直接落地

### 4.5 站点检验：清单页 + 报告页两层

- **清单页**（进 tab 的默认视图）：一行一条线路，**跨客户端**（检验的是站点）。
  列：勾选 · 站点·分组 · 可信度 · 上次检验 · **哪几项对不上（直接写名字，不写「4/6 项」）** · 便宜分 · 动作
- **「从没检验过」是独立行态**（虚线框 + `—`），**不写成 0 分** —— 0 是断言，没检验过是还没有断言。
  熔断的线路同理，写「熔断中」不写 0
- 行内可单跑、勾选可批跑，费用按条数累加。**只在使用者点的时候跑**
- **报告页**：六项各三根条子（标称 / 上次 / 实测）+ 差值片（`较上次 ↑6` / `与上次持平` / `首次检验`）；
  可点的检验历史（42 → 55 → 61）；便宜分怎么被扣的；缓存命中率的 24 小时原始证据
- 首次检验时第三根条子**不画成 0**，直接写「首次检验」

### 4.6 熔断（不要推翻，机制层面是对的）

`429` 只降权不熔断、读 `Retry-After`；`401/403/402` 立即熔断；`5xx/超时` 连续 5 次熔断；
熔断后定期回探；**全熔断时每个客户端仍留一条线路，如实报错，不制造死局**；只在两次请求之间切换。

⚠ 「429 只降权」在现在的评分模型里**没有落脚点** —— `total()` 只有三项，没有惩罚项。P3 要补。

### 4.7 添加/编辑弹窗（照 cc-switch 实际源码做的）

参考 **cc-switch（MIT，Copyright Jason Young，`farion1231/cc-switch`）**。
⛔ **不要照印象猜它长什么样，去读源码**：`src/components/providers/forms/`。

- **弹窗**（使用者明确要弹窗，不要整页），**880px 宽**
- **预设是一排胶囊**（不是下拉），第一颗是「自定义配置」
- **⛔ 只收厂商公布的官方端点，第三方中转站不预置** —— 理由在 `crates/qb-relay/src/relay/presets.rs` 开头：
  地址各家自己在变、很多要登录后台才看得到，预置错了用户会当成官方推荐、排查时先怀疑自己的 Key
- Key 用**眼睛图标**切明文；模型可**点下载图标问上游 `/v1/models` 要**
- **快捷开关取消勾选时，那一项从配置里整个删掉**，实时联动（cc-switch 的招牌交互）
- JSON **当场校验**，要能分清尾逗号 / 键没引号 / 括号没闭合 / 值没引号 / 多一个括号
- **高级选项默认收起**；计费配置是带开关的折叠

**Claude 侧** —— 模型映射表：`模型角色 / 显示名称 / 请求模型 / 1M`，角色 Sonnet·Opus·Fable·Haiku·Subagent。
配置区是**一排六个快捷开关 + 一个 `rows=3` 的小 JSON 编辑器**，右边有「应用通用配置」和「编辑通用配置」。

**⛔ 模型映射默认是「原样透传」** —— 使用者原话：「我 claude 添加中转站，大部分肯定使用的是中转站的 claude 模型」。
客户端要 `claude-sonnet-4.5`、中转站给 `claude-sonnet-4.5`，**不用填**。
映射是**例外**：上游根本没有 Claude 模型（DeepSeek 官方的 anthropic 端点）才要把角色指过去。
「显示名称」描述的是**上游那个模型**（填「DeepSeek V3」），**不是角色别名**。

**Codex 侧** —— 完全是另一套（`CodexFormFields.tsx`）：

- 上游格式**三个**：Chat Completions / Responses / **Anthropic Messages**（选它才展开认证字段、伪装成 Claude Code、最大输出 tokens）
- 模型映射表列是 `菜单显示名 / 实际请求模型 / 上下文窗口 / 思考等级 / 删除` —— **顺序跟 Claude 侧反着**，
  上下文是**数字输入框**不是 1M 复选框
- 有**默认模型**、**提示词缓存路由**（自动/开启/关闭）、**思考能力**两个开关
- 配置是**两个编辑器**：`auth.json`（Key 单独落这儿）+ `config.toml`

### 4.8 ⛔ 默认值（仓库里有权威定义，不许自己编）

```rust
// crates/qb-relay/src/relay/mod.rs
/// `responses` 是 Codex 原生协议，绝大多数中转站用它；`chat` 是 OpenAI
/// Chat Completions。**默认必须是 `responses`** —— 写死成 `chat` 正是坑 1。
pub enum WireApi { #[default] Responses, Chat }
pub enum AuthStyle { #[default] EnvKey, BearerToken, None }
```

`presets.rs` 的真实数据（11 条）：Claude Code 侧三家 anthropic 端点全是 `Responses + BearerToken`；
Codex 侧只有 OpenAI 官方是 `Responses`，其余第三方 `/v1` 端点都是 `Chat`，认证全是 `EnvKey`。

### 4.9 ⛔ 侧栏：照 `Shell.tsx` + `workspace.css` 抄，不要自己编

使用者明确说**喜欢现有这个侧栏**，草图里那个是我编的、要改成跟它一致。
实现就在仓库里，直接抄真值，不要对着截图量像素。

**结构**（`src/features/Shell.tsx:249-299`）

```
aside.qb-sidebar
├─ a.qb-brand                     整块可点，回首页
│   ├─ span.qb-mark   "Q" + 右下角一个 "↗"（不是圆点）
│   └─ span           "QB Gate" + small "你的 AI 工作空间"
├─ button.qb-search-trigger       🔍 + "搜索与快速跳转" + kbd "⌃ K"
├─ nav                            五项，来自 routes.ts 的 NAV，无分组
│   └─ a  ×5          Icon(19px) + span( 名字 + small( 读数 或 hint ) )
└─ div.qb-sidebar-foot            ● + "本地工作空间" + select(跟随系统/浅色/深色)
```

**第二行的规则**：`readout ? readout[0] : hint` —— **有读数用读数，没有就用 hint**。
读数来自 `sidebar.tsx::useSummaries`，只有官方账户 / 软件 / 扩展三项有。

**样式真值**（`src/styles/workspace.css:121-245`，别改数字）

| 元素 | 值 |
|---|---|
| `.qb-sidebar` | `width:228px` · `padding:30px 16px 18px` · `border-right:1px solid var(--border)` · `background:var(--surface)` |
| `.qb-brand` | `gap:11px` · `font-size:20px` · `font-weight:650` · `padding:0 12px 28px` |
| `.qb-brand small` | `10px` · `weight:400` · **`letter-spacing:1px`** · `color:var(--text-2)` · `margin-top:2px` |
| `.qb-mark` | `37×39px` · `radius:11px` · `background:var(--accent)` · `color:var(--surface)` · `font:26px/700` · `display:grid;place-items:center` |
| `.qb-mark span`（那个 ↗） | `position:absolute` · `font-size:17px` · `right:1px` · `bottom:-4px` |
| `.qb-search-trigger` | `background:var(--bg)` · `border:1px solid var(--border)` · `radius:8px` · `padding:9px` · `font:11px` · **`margin-bottom:28px`** · 图标 `Search size=16` |
| `.qb-search-trigger kbd` | `margin-left:auto` · `10px` · 内容是 **`⌃ K`**（不是 `^K`） |
| `nav` | `flex column` · **`gap:7px`** |
| `nav a` | `gap:13px` · **`padding:12px`** · `radius:10px` · `border:1px solid transparent` · `color:var(--text-2)` · 图标 `size=19` |
| `nav a small` | `display:block` · **`10px`** · `color:var(--text-3)` · `margin-top:2px` · `weight:400` |
| 读数三档 | `.ok`→`var(--ok)` · `.warn`→`var(--warn)` · `.danger`→`var(--danger)`；都加 `tabular-nums` + `weight:500` |
| `nav a:hover` | `background:var(--surface-2)` · `color:var(--text)` |
| **`nav a.active`** | `color:var(--accent)` · `background:var(--accent-bg)` · `weight:600` · **`border-color:var(--accent-border)`**（是 1px 描边，**不是**左侧色条） |
| `.qb-sidebar-foot` | `margin-top:auto` · `gap:7px` · `padding:16px 10px 0` · `font:10px` · `color:var(--text-2)` |
| foot `select` | `border:0` · `background:transparent` · `width:100%` · `margin-top:7px` |

**⚠ 窄屏**：`workspace.css` 的 `@media (max-width:1020px)` 把 `.qb-sidebar nav a > span`
和 `.qb-sidebar-foot` 整个 `display:none`，侧栏收成 72px 图标条。
**新加任何在行内占宽的元素，必须一起加进那条名单**，否则 72px 会被撑爆，
`npm run test:ui` 的三档窄屏溢出断言当场全红。

---

## 五、中转站后端 P1–P4：**已完成（0.14.0）**

领域逻辑全部落在 `qb-station`（101 条测试），本机路由在 `qb-app`（33 条）。

| 阶段 | 落在哪 | 关键不变量 |
|---|---|---|
| **P1** 站点池 + 账单 | `station/route.rs` · `station/billing.rs` · `schedule/cheap.rs` | 线路 = 站点 + 分组；真实倍率 = 标称 × mult；**「便宜」优先用 24h 实扣 ÷ 实际 token**，整池统一口径（§4.3 那条 ⚠ 已结） |
| **P2** 本机路由 + 日志 | `router.rs`（纯状态）· `qb-app/local_router.rs`（反代） | **只绑 127.0.0.1**；换上游只在两次请求之间生效；客户端自带的 Key 一律剥掉换成线路自己的 |
| **P3** 智能调度 + 熔断 | `schedule/rank.rs` · `schedule/breaker.rs` | 最弱项规则；比值归一化；迟滞 8%；429 只降权不熔断；三档分开 |
| **P4** 站点检验 | `station/audit.rs` | 六项（使用者确认过）；mult 记在每一轮上；**造假不拉黑，只重新定价** |

### 接线也做完了（同一轮）

| 接在哪 | 做了什么 |
|---|---|
| **迁移框架（C3）** | `repository.rs` 的 `MIGRATIONS` 清单,`SCHEMA = 条数`。老库就地升级、不丢行、更新版本的库拒绝降级打开 —— 四条测试钉着 |
| **新表** | `station_routes` / `station_audits` / `request_logs`,都用 `(id, body)`,通用 list/get/put/prune 直接可用 |
| **账单拉取** | `station_ops::refresh_health` 真去 `GET /api/log/self`,单条失败不影响别条 |
| **命令层** | `commands/station.rs` 十个命令,已进 `invoke_handler` |
| **契约类型** | 99 个导出名,全部唯一（`check-types.mjs` 现在会拦撞名） |

**Key 不出后端**:明文只在装配上游时从 `relay.json` 取一次,所有返回前端的形状
结构上装不下它。

### 界面也接上了（同一轮）

`src/features/station/` 四个分页，照草图 V24 做，**颜色一律用 tokens.css 的变量**
（草图用的是合并前的旧配色，不要从里面抄 hex）：

| 分页 | 有什么 |
|---|---|
| 中转站账户 | 客户端分页（按 `Route.protocols` 过滤）· 启动条（整页只有这一处能启停）· 站点 → 分组两层卡片 · 倍率三态 · 免费健康度自动拉一次 |
| 智能调度 | 三个复选框（不是推子）· 三条可选底线 · 三个预设 · 赛道（每一维一根条 = 跟最好那条的比值，最弱项标出来）· 六张熔断卡 |
| 站点检验 | 清单页（「从没检验过」是虚线框，不是 0 分）+ 报告页（标称/上次/实测三根条，首次检验不画成 0） |
| 请求日志 | 时间 · 线路 · 首字 · 耗时 · 状态；取不到写「—」不写 0 |

旧那套「供应商 / 凭证 / 环境」留在第二个外壳分页里，**别删** ——
站点还得从那儿添（§4.7 那个 880px 弹窗还没做）。带 id 的深链接
（`/relays/new`、`/relays/<id>`）直接落到旧那套。

演示数据齐了：`npm run demo` 四个分页都有东西，倍率三态各来一条。

### 线路的增删改（同一轮补上）

`RouteDialog.tsx`：选站点 · 分组名 · 标称倍率 · 绑哪把 Key ·
三个协议三态下拉。没有它的话四个分页**没有任何办法产生数据** ——
线路只能手写进库。

协议那三项是**三态**（不知道 / 支持 / 不支持），不是勾选框：
「还没探过」和「探过了没有」混成一个，界面会把没测过的线路显示成不支持，
使用者会以为那站废了。

### 计价与调度重做（同一轮，来自使用者自己的两个项目）

维护者给了两份既有实现（`station-monitor-standalone` 与 207 服务器上的
`sub2guard`），照着改掉了三处**做错或做浅**的地方。出处记在 ATTRIBUTION.md。

| 改了什么 | 为什么 |
|---|---|
| 「快」改成**体验分**（首字 + 每 token 耗时 × 500） | sub2guard ㉚ 实测：只看首字会选中最卡的那条（首字 1.4 秒、一次回答 95.8 秒） |
| 「倍率」拆成**四类各算各的** | 只比一个数会被「输入便宜、输出翻五倍」藏过去 |
| 加**官方价目表** + 启动时抓文档页更新 | 真实倍率 = 站点单价 × 标称 ÷ 官方价，没有官方价就只能听站点自己说 |
| 加 **`EvidenceLevel`** | 可信度 29 分有两种相反来源：「测了都对不上」vs「没测到」 |
| 「便宜」多一档**加权口径** | 一家翻倍一家不翻倍时，谁便宜取决于输入输出比 |

**口径优先级**（`schedule::cheap`）：24h 实扣 ÷ 实际 token → 加权等效倍率 →
真实倍率 → 没有可比的。整池统一，混着比出来的比值没有意义。

**官方价没有 API**：Models API 不返回价格。内置快照 + 启动抓 `pricing.md`，
解析不出来**整批丢弃**并沿用快照（`MIN_PARSED_MODELS`）——「半成功」比失败危险。

### 三级选择器与模型表（0.15.0）

使用者的原话：「让用户选择中转站-选择分组-选择模型」「默认 GPT5.6sol，claude opus5」。

| 做了什么 | 在哪 |
|---|---|
| `StationModel { model, rates, groups }` + `in_group()` | `pricing.rs` |
| `pick_audit_model(models, group, prefer_anthropic)` | 同上 |
| `station_models(route_id)` 命令 → `StationModelsView` | `commands/station.rs` |
| `station_run_audit(route_id, model)` 加了模型参数 | 同上 |
| `Cascade` 三级选择器 | `StationAudit.tsx` |

四条**不许合并**的判断，每条都有测试：

1. **站点没公布 `enable_groups` 时 `in_group()` 恒真。**
   「不知道」表现成「不能用」的话，那个站的检验一个模型都选不了；
2. **默认模型只在站点真提供它时才用。** 站点没有 `claude-opus-5` 却硬拿它去验，
   验的是一个不存在的东西，六项里的倍率全记「没测到」——
   界面上看起来像站点不配合，其实是我们挑错了。挑不中就退回该分组第一个真实存在的；
3. **别退回 `provider.meta.model`。** 那是启动客户端时写进环境的，可能是空的、
   也可能是这个分组根本不提供的名字（同上，验的是幽灵）；
4. **站点没开 `/api/pricing` 时模型那格变成输入框，不是灰掉。**
   灰掉的话那种站一个模型都验不了。

`prefers_anthropic(route)` 按线路探出来的协议挑默认值，两边都探到 / 都没探到时偏
Anthropic（这个面板的主要使用者跑 Claude Code）。挑错了也不致命 ——
`pick_audit_model` 会退回站点真有的第一个。

### ⛔ 还没做的

1. **§4.7 那个 880px 添加/编辑弹窗**没做。站点仍从「供应商与环境」那页添；
   分组有 `RouteDialog`。做的时候预设读 `presets.rs`，别抄草图里那份硬编码列表；
2. **上下文窗口 / 最大输出**这两项检验没做 —— 要构造超长输入试探，
   「试到多少算到顶」的判据没定过，而且每试一次都真花钱。现在记「没测到」，
   会压低可信度也会压低 `EvidenceLevel`，这是有意的；
3. ~~价目表只有 10 个模型~~ **已解决**：两家的地址都实访确认了，开机抓回来
   **50 个模型**（Anthropic 17 + OpenAI 33），含长上下文那一档。
   内置快照 10 条只作抓不到时的退路。核对工具见 CLAUDE.md；
4. **分组还不能自动发现**：`/api/pricing` 已经会拉了（检验时会拉、
   `station_models` 也会拉），但把分组列表拉下来**建线路**还没做 ——
   那要能登录站点后台。现在分组列表是从已有线路池里推出来的
   （`groupsOf`），所以线路还得手工加；
5. **`peak_rate` / `group_ratio` 没有来源**：`StationRates` 里留了字段、
   算式也把它们乘进去了，但 `/api/pricing` 不返回这两项，要从分组接口拿。
   现在恒为 `None`（按 1 处理）。

### 硬约束（都已落地并有测试钉着）

- 单测不联网、不动真 ACL、不碰真进程、不写 `%LOCALAPPDATA%\ClaudeIpGate\`。
  本机路由的端到端测试全程只在 127.0.0.1 上、用自带的假上游；
- **中转站的熔断绝不许触发官方账户槽位切换** ——
  `architecture.rs::relay_breakers_can_never_reach_account_switching`
  断言 `qb-station` 不依赖 `qb-accounts` / `qb-launch`。**做过变异验证，会红**；
- DISCLAIMER 第 5.1 节已按本机路由的真实行为改写（使用者定的，见 CLAUDE.md）。

---

## 六、⛔ 这一场踩过的坑（对接手的人最值钱的一节）

**1. 会话跑在门禁之下，不能自己装机。**
关面板会重锁全部 `claude.exe`，`usecase::gate_ops::stop_managed` 还会按 PID 收进程 ——
会话等于在抽自己的地基。**出包（`npm run tauri build`）照做，装机停下来交给使用者点。**
装完由会话做只读验收。只读查询（`Get-Process`、读运行期文件）是安全的。

**2. 改任何一处之前，先在仓库里搜有没有现成的定义。** 这一场栽了三次：

- 编了 Codex 默认协议 `chat`，而 `WireApi` 的注释写着「默认必须是 responses —— 写死成 chat 正是坑 1」
- 编了预设口径，而 `presets.rs` 开头写着「只放官方直连端点」并解释了为什么
- 编了九项带分组的侧栏，而 `routes.ts` 的 `NAV` 是五项平铺，还刻意删掉了我编进去的三个入口

**3. 参考别的项目就去读它的源码，不要靠印象。**
cc-switch 那部分我猜错了四五轮（预设是下拉还是胶囊、弹窗还是整页、模型是输入框还是映射表、
Codex 有没有 Anthropic Messages），每一次都是使用者纠正后去读源码才改对。
`docs/user-manual/zh/` 有官方中文手册，`assets/screenshots/` 有真实截图，`src/components/providers/forms/` 有源码。

**4. flex column 的子项会被压扁。**
`.db` 是 `display:flex;flex-direction:column`，子项默认 `flex-shrink:1`，
内容最少的那个会被压成只剩两条边框。先是 JSON 编辑器被压成 **2px 高**
（「可以直接编辑配置」在界面上等于不存在），补了 `flex:none`；接着两个折叠面板又中同一招。
**最后用 `.db > *{flex:none}` 整体收口。**

**5. 反斜杠会被多层转义吃掉。**
工具调用的 JSON → shell heredoc → Python 字符串，三层。
写 `"~\.claude\settings.json"` 到 JS 里会渲染成 `~.claudesettings.json`。
**用 `chr(92)` 拼，并加自检断言。** 这一场在同一处栽了三次。

**6. `node --check` 只查语法，查不出未定义符号。**
切片替换时把 `COLORS` / `dlg` / `cfgPathOf` 三个声明连带删掉了，`node --check` 全绿，
浏览器控制台才报 `COLORS is not defined`。**改完要在浏览器里真跑一遍。**

**7. 结构性 bug 用「按 ID 查元素」的断言测不出来。**
多出一个 `</div>` 导致 `#t3` 被挤出 `.main` 成了 `.frame` 的第三个网格项（挤进 176px 的侧栏列），
而我所有断言都是 `getElementById`，它不在乎元素挂在哪，**全绿**。
是使用者要求截图才暴露的。**现在有 `div` 开合计数自检，改完顺手跑一遍。**

**8. 截图能看出断言看不出的东西。** 见上一条。发界面改动前先截一张。

**9. 浏览器面板的截图会滞后。** 状态改完立刻截图可能拿到旧帧。
**用 JS 读 DOM 确认状态，截图单独一次调用。**

### 0.15.0 这一轮新栽的三个（都属于「改了但没生效，而且全绿」）

**10. ⛔ 用 `.replace()` 改代码却不 `assert` 锚点，失败时是静默的。**
这一轮**栽了两次**，两次症状都是「测试总数只涨了 1，而我以为涨了 6」：

- 第一次：插测试用的锚点过时了，`s.replace(old, new)` 原样返回，脚本照常打印成功。
  六条新测试一条都没进文件；
- 第二次：改用「从 A 到 B 整段替换」，把区间**里面别的六条测试一起删掉了**，
  其中就有 `which_station_is_cheaper_depends_on_the_token_mix` ——
  「计费翻倍怎么排序」那个问题的答案就锁在那一条里。

`cargo test` 只会显示总数变少，**不会说少了哪几条**。
**改代码的脚本，每一处 `replace` 都要 `assert old in s`，写完再断言目标真的在文件里。**

**11. ⛔ 写文件成功之后的 `print()` 报错，会让人以为整个脚本没跑。**
补 `CLAUDE.md` 那一节时 `print()` 撞上 cp1252 编码错误 —— 而 `write()` 在那之前
**已经执行完了**。我当成没写成，又跑了一遍，于是同一节在文件里出现了两次
（锚点还在最前面，第二次 `assert` 照样通过）。
**脚本的输出用 `sys.stdout.buffer.write(...encode())`，别用 `print` 打中文。**

**12. ⛔ `npm run types:check` 看不见「新加了 ts-rs 类型但忘了往 `export-types.rs` 添一行」。**
它比的是「重新生成的和仓库里那份一不一样」——
**一个从来没导出过的类型，两边都没有它，比什么都一样。**
实测漏掉 `StationModel` / `StationModelsView` / `ModelPrice` 三个，全绿；
前端只能手抄一份形状，而手抄的那份不会跟着 Rust 一起变（正是 B1 花一整轮消掉的盲区）。
`check-types.mjs` 末尾补了第三条断言：扫遍所有 `#[ts(export)]`，
每一个都得在 `src/lib/generated` 里有对应的 `.ts`。**已做变异验证** ——
临时加一个不在名单里的 `#[ts(export)]` 结构体，当场红。

---

## 七、收工清单（`CLAUDE.md` 里那份的摘要）

```bash
npm run build && npm test && npm run types:check && npm run format:check && npm run test:ui && npm run release:check
```

```bash
cargo test --workspace && cargo clippy --workspace --lib -- -D warnings && cargo fmt --all -- --check && cargo deny check
```

```bash
npm run tauri build
```

版本号**四个文件五个位置**（`package.json` · `package-lock.json` 两处 · `src-tauri/Cargo.toml` ·
`src-tauri/tauri.conf.json` · 根 `Cargo.toml` 的 `[workspace.package]`），
**权威是 `npm run release:check`，不是任何一张表**。

`cargo test --workspace` 的通过数**只许涨不许跌**（现在 493）。

---

## 八、零碎的待办

- `%LOCALAPPDATA%\QB Gate\` 里有个 **`export-types.exe`（0.12.0）** —— 开发期的类型导出工具被 NSIS
  连着打包进去了。不影响运行，但不该发给用户。要么从 bundle 里排除，要么记进 `KNOWN-ISSUES`
- `%USERPROFILE%\OneDrive\桌面\claude-gate` 是搬家时留的**安全网副本**（没有 `crates/`，停在重构前）。
  0.13.0 已验收，可以问使用者要不要删 —— 两份同名同分支的仓库并存，正是「对着新代码看旧行为」的入口
- cc-switch 还有两样我们没决定要不要做：**多端点管理 + 测速**（一个站点配几个地址、设默认、测延迟）、
  **通用供应商**（一份配置同步到多个客户端）
