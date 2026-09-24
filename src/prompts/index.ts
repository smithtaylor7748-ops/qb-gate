/**
 * 内置提示词。用户可在界面上查看、复制、按需修改后再交给 Codex。
 *
 * 两条都刻意写成「先诊断、再报告、要确认才动手」的形态：
 * 让模型直接对系统网络配置和软件安装动手，出错的代价比多问一句大得多。
 */

export interface PromptDef {
  id: string;
  title: string;
  /** 建议使用的模型档位，界面上要提示。 */
  modelHint: string;
  body: string;
}

export const DNS_LEAK_PROMPT: PromptDef = {
  id: "dns-leak",
  title: "DNS 泄露深度排查与修复",
  modelHint:
    "建议选高级模型（推理能力强的档位）。这一条要读网络配置并做判断，弱模型容易给出看似合理但错误的结论。",
  body: `你是一名 Windows 网络诊断工程师。请在本机排查 DNS 泄露，并在获得我确认后修复。

## 背景
本机通过 VPN/代理隧道出境（可能是 TUN 路由模式，例如 v2rayN + Xray，虚拟网卡名类似 xray_tun）。
目标是：所有 DNS 查询都必须走隧道，不得从物理网卡以明文发往本地路由器或国内 DNS。

## 第一阶段：只诊断，不修改
按顺序执行并逐条报告结果，**这一阶段不要修改任何配置**：

1. 网卡与 DNS 配置
   Get-DnsClientServerAddress -AddressFamily IPv4,IPv6
   Get-NetAdapter | Where-Object Status -eq 'Up'
2. 路由表：谁持有默认路由，接口跃点各是多少
   route print 0.0.0.0
   Get-NetIPInterface | Sort-Object InterfaceMetric
3. 本地代理端口是否在监听（常见 10808 / 10809 / 7890）
   Get-NetTCPConnection -State Listen | Where-Object LocalPort -in 10808,10809,7890
4. 系统代理设置
   netsh winhttp show proxy
   Get-ItemProperty 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings' | Select ProxyEnable,ProxyServer
5. 实际解析路径：解析几个域名，看用的是哪台服务器
   Resolve-DnsName example.com -Type A
   nslookup example.com
6. 浏览器层的 DoH（Chrome/Edge 的「使用安全 DNS」会绕过系统解析器）
   检查 chrome://settings/security 与对应组策略注册表项
7. 如果具备条件，用 PktMon 做网卡级抓包核实物理网卡上有没有 53/853 出站：
   pktmon filter remove
   pktmon filter add -p 53
   pktmon start --etw -m real-time
   （抓 20 秒，其间访问几个网站，然后 pktmon stop）
   **重点：物理以太网/WLAN 上应当为 0 条 DNS；只允许出现在隧道网卡上。**

## 第二阶段：判定
基于以上证据给出结论，并明确区分这三种情况：
- **无泄露**：所有解析器都在隧道内或境外可信 DNS，物理网卡无 53/853 出站
- **配置层泄露**：物理网卡 DNS 指向路由器（如 192.168.1.1）或国内 DNS
- **应用层泄露**：系统配置看着正常，但抓包能看到物理网卡上有明文 DNS

注意一个已知的迷惑点：仅凭"配置和路由看起来对"不足以下结论，必须有抓包或解析回显佐证。

## 第三阶段：提出修复方案，等我确认后再执行
先给出方案和每一步的风险，**我说「执行」你才动手**。可选手段：
- 把物理网卡 DNS 固定为隧道的合成地址，或改回 DHCP（取决于隧道当前是开还是关）
- 用防火墙规则阻断物理接口的出站 TCP/UDP 53 与 853，只放行隧道网卡
- 关闭浏览器的安全 DNS(DoH)，或将其指向与隧道一致的上游
- 关闭 WebRTC 的本地 IP 暴露

**硬性约束：**
- 不要重启或停止 VPN 服务，除非我明确同意——那会断掉我当前所有连接
- 不要修改 Xray/v2rayN 的配置文件
- 每一处改动都先说明「改什么、为什么、怎么还原」
- 改完后重新跑一遍第一阶段验证，并报告前后对比

## 输出格式
1. 诊断结果表（检查项 / 实测值 / 是否异常）
2. 判定结论（三选一，附证据）
3. 修复方案（分步，标注风险与还原方法）
4. 等待我确认`,
};

export const CLEAN_REINSTALL_PROMPT: PromptDef = {
  id: "clean-reinstall",
  title: "Claude 相关软件的完整卸载与重装",
  modelHint: "建议选高级模型。涉及删除文件与注册表，判断失误会误删无关数据。",
  body: `你是一名 Windows 系统维护工程师。请帮我把本机的 Claude 相关软件完整卸载干净，然后重新安装。

## 用途说明
我要做的是一次**彻底的干净重装**，常见原因：安装损坏、配置写坏、要把机器转交他人、
或要清除我自己留在本机的数据。目标是让重装后的环境与全新安装一致，不带旧的损坏配置。

## 第一阶段：先盘点，不删除
列出你找到的所有相关内容，**这一阶段不要删任何东西**：

1. 安装位置
   - %LOCALAPPDATA%\\AnthropicClaude\\           Claude 桌面端（含 app-<版本> 子目录）
   - %USERPROFILE%\\.local\\bin\\claude.exe       Claude Code
   - winget 安装的副本
2. 配置与数据
   - %APPDATA%\\Claude\\                         桌面端配置、登录态、Cookies
   - %USERPROFILE%\\.claude\\ 与 .claude.json     Claude Code 配置
   - %USERPROFILE%\\.credentials.json 所在位置
3. 残留
   - %USERPROFILE%\\.local\\bin\\claude.exe.old.*  升级残留的旧副本
   - 桌面与开始菜单的快捷方式
   - 相关的计划任务：Get-ScheduledTask | Where-Object TaskName -like '*laude*'
   - 注册表卸载项：HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\ 下的相关键
4. 正在运行的进程
   Get-Process claude,Update -ErrorAction SilentlyContinue | Select Id,Path

把结果整理成一张表给我：路径 / 类型 / 大小 / 是不是能安全删除。

## 第二阶段：等我确认后执行卸载
我确认之后按顺序做：

1. 先关掉所有相关进程（Claude 桌面端、Claude Code 会话）
2. 优先走官方卸载程序；没有卸载程序的再手动删目录
3. 删除上面盘点出的配置、数据、残留、快捷方式、计划任务
4. 清理注册表里的对应卸载项

**已知的坑，请特别注意：**
- \`claude.exe.old.*\` 这类残留，**如果是当前正在运行的会话自己占着，删不掉**。
  Windows 允许改名正在运行的 exe，但不允许删除它。遇到"拒绝访问"时先确认是不是被占用
  （用独占方式打开测试，而不是看 ACL——那种情况下 ACL 是正常的），
  关掉对应进程后再删。
- 删除前请再次确认路径，**不要误删 %APPDATA% 或 %LOCALAPPDATA% 下的无关目录**。

## 第三阶段：重装
1. 从官方渠道下载安装包
2. **校验 SHA-256 之后再安装**，哈希对不上就停下来告诉我
3. 安装完成后报告版本号

## 输出格式
1. 盘点表
2. 等我确认
3. 卸载执行记录（每一步的结果，失败的单独列出并说明原因）
4. 重装结果与版本号

## 面板够不到的两块，请一并盘点
QB Gate 面板的「完全卸载」只扫当前 Windows 用户账户，这两块它够不到，交给你：

1. **WSL 发行版**：先 \`wsl -l -v\` 列出每个发行版，逐个查
   \`~/.claude\`、\`~/.claude.json\`、\`~/.local/bin/claude\`、\`~/.local/share/claude\`、
   \`~/.npm-global\` 或 \`npm root -g\` 下的 \`@anthropic-ai/claude-code\`，
   以及 \`~/.bashrc\` / \`~/.zshrc\` / \`~/.profile\` 里设 \`ANTHROPIC_*\` / \`CLAUDE_*\` 的行
2. **本机其它 Windows 用户账户**：C:\\Users\\<其他用户>\\ 下同样的位置
   （.claude、.claude.json、.local\\bin、AppData\\Roaming\\Claude、AppData\\Local\\AnthropicClaude）。
   **看得到但没权限删的，列出来告诉我，不要提权硬删**

## 边界
- 只处理 Claude 相关的软件与数据，不碰其他程序
- 不要改系统安全设置、不要动防火墙规则、不要改用户账户
- 删除操作要能说清楚删的是什么；说不清楚的先问我`,
};

export const CODEX_REINSTALL_PROMPT: PromptDef = {
  id: "codex-reinstall",
  title: "Codex（CLI 与桌面端）的完整卸载与重装",
  modelHint:
    "建议选高级模型。涉及删除文件、Store 包与注册表，判断失误会误删无关数据。",
  body: `你是一名 Windows 系统维护工程师。请帮我把本机的 OpenAI Codex（命令行 CLI 与 Microsoft Store 桌面端）完整卸载干净，然后重新安装。

## 用途说明
我要做的是一次**彻底的干净重装**，常见原因：安装损坏、Store 包注册失效（启动一律「拒绝访问 os error 5」）、
配置写坏、要把机器转交他人、或要清除我自己留在本机的数据。目标是让重装后的环境与全新安装一致。

## 第一阶段：先盘点，不删除
列出你找到的所有相关内容，**这一阶段不要删任何东西**：

1. 桌面端（Microsoft Store 包）
   Get-AppxPackage -Name OpenAI.Codex | Select Name,Version,PackageFamilyName,InstallLocation
   - 数据目录：%LOCALAPPDATA%\\Packages\\OpenAI.Codex_2p2nqsd0c76g0\\
   - 桌面端自带的 CLI：%LOCALAPPDATA%\\OpenAI\\Codex\\bin\\<hash>\\codex.exe
2. 命令行 CLI 的几种落点（可能同时装了不止一份）
   - npm 全局：npm ls -g @openai/codex；%APPDATA%\\npm\\codex.cmd 与 node_modules\\@openai\\
   - winget：winget list --id OpenAI.Codex；%LOCALAPPDATA%\\Microsoft\\WinGet\\Packages\\OpenAI.Codex*
   - 官方压缩包手动解压的：%USERPROFILE%\\.local\\bin\\codex.exe、%LOCALAPPDATA%\\Programs\\codex\\
3. 配置、会话与凭证
   - %USERPROFILE%\\.codex\\（config.toml、sessions\\、auth.json —— **auth.json 只报位置，不要打开看内容**）
   - CODEX_HOME 环境变量指向的目录（如果设了）
   - 凭据管理器：cmdkey /list 里含 openai / codex 的条目（只列名字）
4. 环境变量与 Shell 配置
   - 用户级：CODEX_* 前缀的变量；PowerShell profile / .bashrc 里设它们的行
   - **OPENAI_API_KEY 别的工具也在用，列出来但默认不删，删之前问我**
5. 正在运行的进程
   Get-Process ChatGPT,Codex,codex -ErrorAction SilentlyContinue | Select Id,Path

把结果整理成一张表给我：路径 / 类型 / 大小 / 是不是能安全删除。

## 第二阶段：等我确认后执行卸载
我确认之后按顺序做：

1. 先关掉所有相关进程（桌面端窗口、终端里的 codex 会话）
2. Store 包：Get-AppxPackage -Name OpenAI.Codex | Remove-AppxPackage
   npm 装的：npm uninstall -g @openai/codex；winget 装的：winget uninstall --id OpenAI.Codex -e
   手动解压的再直接删目录
3. 删除上面盘点出的配置、会话、凭证、环境变量、Shell 配置行
4. 清理注册表里对应的卸载登记（HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\ 下）

**已知的坑，请特别注意：**
- Store 包正在跑的时候 Remove-AppxPackage 会失败，先退干净桌面端（托盘图标也要退）
- WindowsApps 下的文件不要手动删，权限被系统锁着，只能走 Remove-AppxPackage
- 正在运行的 exe 删不掉（Windows 允许改名，不允许删除），遇到「拒绝访问」先确认是不是被占用
- 删除前请再次确认路径，**不要误删 %APPDATA% 或 %LOCALAPPDATA% 下的无关目录**

## 第三阶段：重装
1. 桌面端：从 Microsoft Store 装（apps.microsoft.com/detail/9plm9xgg6vks），或用 winget install --id 9PLM9XGG6VKS --source msstore
2. CLI：从 github.com/openai/codex 的 Releases 下载，**校验 SHA-256 与 OpenAI 数字签名之后再放进 PATH**，哈希对不上就停下来告诉我
3. 安装完成后报告两边的版本号

## 面板够不到的两块，请一并盘点
QB Gate 面板的「完全卸载」只扫当前 Windows 用户账户，这两块它够不到，交给你：

1. **WSL 发行版**：先 \`wsl -l -v\` 列出每个发行版，逐个查 \`~/.codex\`、\`npm root -g\` 下的
   \`@openai/codex\`、\`~/.local/bin/codex\`，以及 shell 配置里设 \`CODEX_*\` 的行
2. **本机其它 Windows 用户账户**：C:\\Users\\<其他用户>\\ 下同样的位置。
   **看得到但没权限删的，列出来告诉我，不要提权硬删**

## 输出格式
1. 盘点表
2. 等我确认
3. 卸载执行记录（每一步的结果，失败的单独列出并说明原因）
4. 重装结果与版本号

## 边界
- 只处理 Codex 相关的软件与数据，不碰其他程序（ChatGPT 网页登录态、浏览器数据都不在内）
- 不要改系统安全设置、不要动防火墙规则、不要改用户账户
- 删除操作要能说清楚删的是什么；说不清楚的先问我`,
};

export const BROWSER_REINSTALL_PROMPT: PromptDef = {
  id: "browser-reinstall",
  title: "浏览器重装与语言时区核对",
  modelHint: "普通模型即可，但涉及删除浏览器配置文件，请确认已备份书签与密码。",
  body: `请帮我把浏览器重装一遍，并核对它的语言与时区设置是否与我的网络出口一致。

## 先提醒我
重装会清掉浏览器里的书签、密码、扩展和登录态。**开始之前先问我是否已经备份**，
我说可以了再继续。

## 第一阶段：盘点与核对
1. 已安装的浏览器与版本
2. 当前配置目录位置与体积
   - Chrome: %LOCALAPPDATA%\\Google\\Chrome\\User Data
   - Edge:   %LOCALAPPDATA%\\Microsoft\\Edge\\User Data
3. 核对以下几项，并与我的出口 IP 归属地对比，逐条报告是否一致：
   - navigator.languages（浏览器语言列表）
   - Intl.DateTimeFormat().resolvedOptions().timeZone（时区）
   - Intl.DateTimeFormat().resolvedOptions().locale（区域格式）
   - 是否开启了安全 DNS(DoH)，指向哪里
   - WebRTC 是否暴露本地/真实 IP

## 第二阶段：等我确认后重装
1. 走官方卸载程序卸载
2. 删除残留的 User Data 目录
3. 从官方站点下载安装包，校验签名后安装
4. 首次启动时按我指定的语言与时区配置

## 第三阶段：复核
重新跑一遍第一阶段的核对项，给出前后对比表。

## 边界
- 不要删除我明确说要保留的 profile
- 不要修改系统级的语言与时区（那个由控制面板单独负责）`,
};

export const ALL_PROMPTS = [
  DNS_LEAK_PROMPT,
  CLEAN_REINSTALL_PROMPT,
  CODEX_REINSTALL_PROMPT,
  BROWSER_REINSTALL_PROMPT,
];

/** 索引文档里那份 sulianyan 新手指引，DNS 页面上作为参考链接给出。 */
export const QUICKSTART_DOC = "https://docs.sulianyan.com/quickstart.html";
