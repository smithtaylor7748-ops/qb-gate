# 第三方来源与致谢

本项目的检测部分大量参考了已有开源实现。凡是抄了代码的，逐条列在下面；
凡是只用了公开接口协议、没有复制代码的，也单独说明，避免含糊。

---

## 直接改编了代码

### FuckClaude — 中文环境识别十项指纹

- 仓库：https://github.com/LinXiaoTao/FuckClaude
- 许可：**MIT**，Copyright (c) 2026 LinXiaoTao
- 用在：`src/lib/signals.ts`

权重表、评分函数、字体名单、国产浏览器与设备的正则名单基本按原样保留，
只做了本地化注释与类型微调。两处关键判定原样继承，不要在维护中改掉：

- **台湾不计分。** `Asia/Taipei` 与 `zh-TW` 是 Anthropic 完整支持的地区，
  给它们加分属于误报（对应上游 issue #11）。港澳属受限地区，保留部分风险分。
- **繁体字体压在命中阈值以下。** 给 0.2 分，阈值是 0.25，刚好不算命中 ——
  繁体字体在台湾很常见，而且区分不出 TW 与 HK/MO。

MIT 要求保留版权声明与许可声明，已在 `src/lib/signals.ts` 文件头注明。

### cc-switch — 中转站配置的形态

- 仓库：https://github.com/farion1231/cc-switch
- 许可：**MIT**，Copyright (c) Jason Young
- 用在：`src-tauri/src/relay/mod.rs`

参考的是「多供应商 + 一键切换 + 直接写进 CLI 自己的配置文件」这套产品形态，
以及原子写（临时文件 + 改名）与自动备份的做法。代码为独立实现，未复制。
技术栈选型（Tauri 2 + React + TypeScript）同样是跟着它走的。

---

## 界面依赖

前端重做（2026-09-09）引入的两个库，都是 MIT，都作为普通依赖使用，
没有改动其源码：

### Tailwind CSS v4

- 仓库：https://github.com/tailwindlabs/tailwindcss
- 许可：**MIT**，Copyright (c) Tailwind Labs, Inc.
- 用在：`src/styles/`，只用它的工具类与 `@theme` 令牌层

配色**没有用它的调色板**：`src/styles/tokens.css` 里那套暖褐底加赭橙的变量
是本项目原有的，逐行搬过来的，一个 hex 都没改。
Tailwind 在这里只负责生成间距、字号、圆角这些工具类。

### lucide-react

- 仓库：https://github.com/lucide-icons/lucide
- 许可：**ISC**，Copyright (c) Lucide Icons and Contributors
  （项目本身是 Feather Icons 的分支，那部分沿用其 MIT 声明）
- 用在：全部界面图标

替掉了原来用 `▾ ▸ ● ○ ✓ 📁` 这些字符当图标的做法 ——
那些在不同字体下宽度和基线都不一样，而且读屏器会把它们念出来。

---

## 作为独立程序启动，没有链接也没有改源码

### SillyTavern — 酒馆插件

- 仓库：https://github.com/SillyTavern/SillyTavern
- 许可：**AGPL-3.0**
- 用在：`src-tauri/src/plugins/sillytavern.rs`

**ClaudeGate 没有复制、修改或链接 SillyTavern 的任何代码。** 插件做的事情是：
以独立进程启动它自带的启动脚本、轮询端口判断就绪、需要时按 PID 停止，
以及读写它的数据目录做备份恢复。这属于「把它当成一个外部程序来用」，
不构成 AGPL 意义上的衍生作品，因此 ClaudeGate 本身仍是 MIT。

**但有两条线要自己守住：**

- 如果你**改了** SillyTavern 的源码，并且拿它**对外提供网络服务**，
  AGPL 要求你公开修改后的源码。本项目默认部署是本机回环（127.0.0.1:8000），
  只有自己用，不触发这条；一旦你把它暴露到公网就要重新评估。
- 插件里那套启动时序（端口、就绪判定、回滚）是从本机现有的
  `start-claude-pro-rp.ps1` 移植的，那是用户自己的脚本，不涉及第三方许可。

---

## 只用了公开接口，没有复制代码

### Claude 官方 Usage 页面与更新边界

总览的订阅卡片只打开 Claude 官方 `Settings → Usage` 页面，不读取额度、429、限流或 OAuth 内部接口。
ClaudeGate 自身的 GitHub Releases 更新在仓库创建和签名公钥配置前保持停用，不下载或执行未经签名的包。

### bash.ws — DNS 泄露测试的权威 NS 回显

- 服务：https://bash.ws/dnsleak
- 用在：`src-tauri/src/probe/dns.rs`

用的是 bash.ws 自己对外提供的接口协议：

1. `GET https://bash.ws/id` 取一个测试 id（**必须由服务端签发**，自造的 id
   查回来永远是空清单，会把「没查到」误读成「没泄露」）
2. 依次解析 `1..10.<id>.bash.ws`，权威域名服务器记录是谁来查的
3. `GET https://bash.ws/dnsleak/test/<id>?json` 取回解析器清单

### DNSLeakTester — 参考实现，未采用其代码

- 仓库：https://github.com/ygbull/DNSLeakTester

上面那套 bash.ws 调用时序（探针 10 个、间隔 200ms、等待 3000ms）是照着它的
实现校准的。**但该仓库没有 LICENSE 文件**，只有 README 里的一个 MIT 徽章，
`package.json` 里也没有 license 字段 —— 授权状态不明确。

因此本项目**没有复制它的任何代码**，只实现了 bash.ws 的公开接口协议本身
（接口调用方式属于事实性接口，不构成受保护的表达）。在其许可澄清之前，
维护时也请不要从该仓库复制代码。

### ippure.com — 纯净度与 Claude 环境检测

- 站点：https://ippure.com/ ，`/claude.html`，`/DNS-Leak-Detect.html`
- 公开接口：`https://my.ippure.com/v1/info`（无需 API Key）

面板自测用的是它的公开接口。三项硬指标的字段口径
（IPPure 系数 / IP来源=原生IP / IP属性=住宅IP）也来自该站。
站方注明接口尚在测试阶段、可能变动，所以代码里对缺字段的处理是
**如实报「未知」，不猜成通过**。

### 其他看过但未采用的

- MyIP — https://github.com/jason5ng32/MyIP（MIT）。功能全面的 IP 工具箱，
  自建部署友好。本项目没有直接用它的代码，但它的检测项划分值得参考。
- webrtc-privacy — https://github.com/ntblk/webrtc-privacy（MIT）。
  WebRTC 检测的思路与 `signals.ts` 里那段同源（STUN + 收 ICE 候选）。

---

## 关于「Unicode 隐写术」那个说法

ippure 与 FuckClaude 都提到：Claude Code 在 `ANTHROPIC_BASE_URL` 指向中转端点时，
会读取系统时区与中转 hostname，并把结果用 Unicode 隐写术藏进 system prompt
「Today's date」那一行（日期分隔符与四种视觉几乎相同的撇号变体）。

**这是第三方逆向分析主张，本项目未做独立验证，不作为既定事实陈述。**
面板界面上引用时会标注来源与「未证实」。同时注意其触发条件是**走中转端点**；
官方 OAuth 直连路径不在该描述范围内。
