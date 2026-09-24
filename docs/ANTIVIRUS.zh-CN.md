# Windows 安全中心把 QB Gate 当病毒杀了怎么办

> **先说结论，别让下面的篇幅误导你：**
>
> 这是**误报**，但它**会再发生**。0.29.0 做的几项改动只能降低概率、让这一次的告警可以被清掉，
> **不能保证下一版不再被报** —— 因为真正的根因是「没有代码签名 + 每次构建都是一个零信誉的新文件」，
> 而这一版**没有做代码签名**。
>
> 想彻底解决只有一条路：给 `qb-gate.exe` 和安装包做 Authenticode 代码签名。
> 路线与各自的代价见 [KNOWN-ISSUES](KNOWN-ISSUES.zh-CN.md) 里「代码签名」那一节
> —— ⚠ **那里有一条必须先看的预期纠正：EV 证书已经不给即时信誉了，没有花钱买立竿见影这条捷径。**
>
> **在签名之前，免费的办法能做到哪一步：见第 5 节。** 简短版：
> 发布前主动提交误报申诉（唯一免费且对所有人生效）+ 少发版本 + 改 `perMachine` 安装。

---

## 1. 它到底报了什么

本机 2026-09 的四次记录（`Get-MpThreatDetection` + Defender 事件日志 1116/1117 实测）：

| 时间                     | 检出名                       | ID         | 打在哪                                                    | 处置 |
| ------------------------ | ---------------------------- | ---------- | --------------------------------------------------------- | ---- |
| 2026-09-12 11:47 / 11:53 | `Trojan:Win32/Bearfoos.A!ml` | 2147731250 | `%LOCALAPPDATA%\QB Gate\qb-gate.exe`                      | 隔离 |
| 2026-09-19 15:51         | `Trojan:Win32/Bearfoos.B!ml` | 2147731849 | `target\release\bundle\nsis\QB Gate_0.24.7_x64-setup.exe` | 隔离 |
| 2026-09-20 20:32         | `Trojan:Win32/Bearfoos.A!ml` | 2147731250 | 同上（装好的 exe + 开始菜单快捷方式 + HKCU 卸载登记）     | 隔离 |
| 2026-09-22 06:05 / 06:06 | `Trojan:Win32/Bearfoos.A!ml` | 2147731250 | 同上（0.32.0 那份）                                       | 隔离 |

⚠ **2026-09-22 那次的后果不是「少了个 exe」**：使用者看到面板没了，去 GitHub 下了当时的最新
release（v0.24.8，那时 GitHub 已经落后本机好几版），装上之后整个面板卡在
「需要完成数据恢复」页，`sessions` 表里 0.26.0 之后写进去的 `"client":"antigravity"`
老版本的 `Client` 枚举认不出来。**被杀之后装回来的版本不能比你原来装的旧** —— 0.25.3 起 GitHub 上挂的就是最新版，
从那里拿是对的；但如果你本机装过更新的构建，就装回同一版。手边留一份安装包（放在下面 4.3 的排除目录里）最省事。

**看 2026-09-22 06:05 那条的 `Resources` 字段**，它一次点了三样：

```
file:   %LOCALAPPDATA%\QB Gate\qb-gate.exe
file:   ...\Start Menu\Programs\QB Gate.lnk
regkey: HKCU\...\CurrentVersion\Uninstall\QB Gate
```

「未签名 exe 装在 AppData + HKCU 卸载登记 + 开始菜单快捷方式」是模型里权重很高的**一组**
特征，因为这就是绝大多数真木马的安装形状。而 `src-tauri/tauri.conf.json` 现在正是
`"installMode": "currentUser"` —— 见 5.1 里 `perMachine` 那条候选。

**`!ml` 这个后缀是关键**：它表示这条结论来自 Defender 的**机器学习启发式模型**，
不是某个已知恶意样本的特征码命中。事件日志里的 `Detection Type: FastPath` /
`Detection Source: Real-Time Protection` 也是同一个意思 —— 没有人分析过这个文件，
是模型根据「它长什么样、它在干什么、有多少人在用它」打出来的分。

## 2. 为什么会被误报

模型看三件事，QB Gate 三件全占：

**① 谁签的 —— 没人签。**
`Get-AuthenticodeSignature "$env:LOCALAPPDATA\QB Gate\qb-gate.exe"` 实测回 `NotSigned`。
一个未签名的可执行文件，系统答不出「这是谁发布的」。

**② 多少人在跑这个文件 —— 零。**
每次 `npm run tauri build` 产出的都是一个全新的哈希。Defender 的云信誉库里没有它的任何记录，
而「全世界只有这一台机器见过」本身就是一项扣分。

**③ 它在干什么 —— 每一条都在木马特征表上。**
这一条最要命，而且**这些就是这个软件的功能，删不掉**：

| 面板在做什么                                                              | 为什么这一项会被算成可疑                                        | 代码在哪                                                                  |
| ------------------------------------------------------------------------- | --------------------------------------------------------------- | ------------------------------------------------------------------------- |
| 给别人的 exe 写 **Deny ACE**，并在每次启动时重锁                          | 「未经提示地篡改其它程序的执行权限」= 勒索 / 防御规避的典型动作 | `crates/qb-platform/src/acl.rs`                                           |
| `taskkill /T /F`、`Stop-Process -Force` 关第三方进程（含 `chrome.exe`）   | 强杀别人的进程                                                  | `crates/qb-launch/src/killswitch.rs`、`install/chrome.rs`                 |
| 从网上下 exe / zip / msix，校验后**执行**它                               | 这就是 downloader / dropper 的标准链条                          | `install/managed.rs`、`install/codex_store.rs`、`plugins/codex_egress.rs` |
| 通过 CDP 往别人的 Electron 应用里**注入 JS**                              | 跟 Electron 应用的令牌窃取程序形状一样                          | `crates/qb-extensions/src/plugins/antigravity_ui.rs`                      |
| 按**字节**扫 Chrome 的 `Cookies` / `History`；`cmdkey /list` 列凭据条目名 | 信息窃取程序的招牌动作                                          | `install/browser_audit.rs`、`usecase/purge_ops.rs`                        |
| 建 Windows 防火墙**出站阻断**规则、改系统代理、改 IPv6 网卡绑定           | 网络篡改                                                        | `crates/qb-platform/src/firewall.rs`、`crates/qb-sysenv/src/sysenv/`      |
| 隐藏窗口 + UAC 提权跑 PowerShell                                          | 无窗口提权执行                                                  | `sysenv/ipv6.rs`、`install/codex_desktop.rs`                              |

**这些动作面板全部只在使用者点击之后执行**，每一处的代价都写在界面上，
而且可以在面板里逐条撤销 —— 但这些是**意图**，杀软的模型只看**行为**。

## 3. 0.29.0 做了什么（以及没做什么）

**做了**（减少那些「正规程序都会声明、恶意程序常常懒得写」的差异）：

| 改动            | 之前                                                                                             | 现在                                                                           |
| --------------- | ------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------ |
| PE 版本资源     | `InternalName` / `OriginalFilename` / `LegalCopyright` **全是空的**，`CompanyName` 只有 `qbgate` | `CompanyName` = `smithtaylor7748-ops`、`LegalCopyright` 补全（见下面那条限制） |
| 应用清单        | 只有一个 Common-Controls 依赖，**没有 `requestedExecutionLevel`**                                | `asInvoker` + `supportedOS`（Win7–11）+ PerMonitorV2 DPI + UTF-8 代码页        |
| PowerShell 提权 | `-Verb RunAs` + `-WindowStyle Hidden` + **`-EncodedCommand`** 三件凑齐（权重最高的组合之一）     | 去掉 base64，改 `-Command`                                                     |
| 开机行为        | **每次启动**都枚举使用者桌面、按名字搬走文件                                                     | 落 marker，一台机器只做一次                                                    |
| 编译选项        | `strip = true`（连符号表一起剥）                                                                 | `strip = "debuginfo"`                                                          |

**做不到的一项**（0.29.0 构建后实测回读确认）：`InternalName` 与 `OriginalFilename`
**仍然是空的**。`tauri-build` 只设 `FileVersion` / `ProductVersion` / `ProductName` /
`CompanyName` / `FileDescription` / `LegalCopyright` 六项（`tauri-build-2.6.3/src/lib.rs:604-670`），
没有提供设这两项的入口。

⛔ **不要用 `append_rc_content` 去补第二个 `VS_VERSION_INFO` 块** —— 一个 PE 里有两份版本资源
要么编译报错、要么让文件看起来**更**可疑，跟目的相反。也不要构建完再去改二进制：
改过的 exe 跟构建产物对不上哈希，等于把可复现构建这一条也丢了。
真要补只能等 tauri-build 开这个口子（或自己 vendor 它，代价不值）。

**没做**：代码签名。见文首那段。

**故意不做**：面板里**不会**有「一键给自己加 Defender 排除项」的按钮。
一个程序自己给自己开杀软白名单，这个动作本身就是恶意软件的典型行为 ——
加了只会让下一版被判得更重。要加排除项请你自己在管理员窗口执行，命令在下面。

## 4. 现在怎么办

### 4.1 核对一下你手上这份是不是真的

在**给自己加白名单之前**先做这一步。误报的前提是「文件确实是官方构建的那一份」。

```powershell
Get-FileHash "$env:LOCALAPPDATA\QB Gate\qb-gate.exe" -Algorithm SHA256
```

拿它跟 [Releases](https://github.com/smithtaylor7748-ops/qb-gate/releases/latest) 页面里
`SHA256SUMS.txt` 的值对。**对不上就不要恢复它** —— 那说明你手上这份不是从这个仓库构建出来的。

### 4.2 从隔离区恢复（管理员 PowerShell）

```powershell
& "$env:ProgramFiles\Windows Defender\MpCmdRun.exe" -Restore -Name "Trojan:Win32/Bearfoos.A!ml"
```

看当前隔离了些什么：

```powershell
& "$env:ProgramFiles\Windows Defender\MpCmdRun.exe" -Restore -ListAll
```

### 4.3 加排除项（管理员 PowerShell）

⚠ **代价：Defender 从此完全不扫这个目录。** 自己权衡。

先看一眼现在加了没有（**非管理员连查都查不了**，会回一句 `Must be an administrator`）：

```powershell
(Get-MpPreference).ExclusionPath
```

两处一起加 —— 构建输出目录也会被反复隔离（0.24.7 那次就是安装包在 `target` 里被直接
删掉的），而且排除它还有个副作用是好的：Rust 增量编译产生的几十万个小文件不再被实时
扫描逐个拦一道：

```powershell
Add-MpPreference -ExclusionPath "$env:LOCALAPPDATA\QB Gate", "D:\claude-gate\target"
```

撤销：

```powershell
Remove-MpPreference -ExclusionPath "$env:LOCALAPPDATA\QB Gate"
```

⛔ **两条别走的近路：**

- `Set-MpPreference -ThreatIDDefaultAction 2147731250 Allow` —— 那是对**所有**
  `Bearfoos.A!ml` 开门，不是只放自己这一个文件。
- 安全中心里点「允许在设备上」—— 按哈希放行，下一次 `tauri build` 出来又是新哈希，照杀。

### 4.4 向微软提交误报申诉（**这才是正路**）

排除项只对你这一台机器有效。**申诉是唯一免费、而且能让所有人都不再被报的动作**，
微软按文件哈希放行，一般 1–3 天。

⛔ **在公告前提交，不是等用户来报。** 只要比公告（群里、论坛上说「出新版了」）早三天交，下载的人就撞不上。
这一步要登录微软账号，**只能由使用者自己交**：推 GitHub 发版之后，把 Release 上那一份安装包下下来交（CI 编的
跟本机编的哈希不同，交本机那份没用 —— 见下一条）。0.25.3 起装着旧版的人会在面板里收到更新弹窗，那也算公告。

⛔ **交的必须是别人真会下载到的那一份。** 2026-09-22 差点交错：那天磁盘上的
`qb-gate.exe` 是从隔离区还原回来的 0.32.0，而 GitHub 上挂的是 v0.24.8 ——
交它等于给一个全世界没人下载得到的哈希开白名单，白费一次。
交之前先按 4.1 核一遍哈希跟 Release 的 `SHA256SUMS.txt` 对不对得上。

1. 打开 <https://www.microsoft.com/wdsi/filesubmission>
2. 身份选 **Software developer**（不是 Home customer —— 开发者提交走的是另一条更快的队列）。
   ⚠ 选了它点 Continue 会**跳转 `login.microsoftonline.com` 要求登录微软账号**，
   匿名提交只有 Home customer 那一档
3. 上传这两个文件（**同一次发布的那一对**）：
   - `target\release\bundle\nsis\QB Gate_<版本>_x64-setup.exe`
   - 它装出来的 `%LOCALAPPDATA%\QB Gate\qb-gate.exe`
4. 其余字段照下表填。取值的命令：`Get-MpComputerStatus`（引擎/特征库版本）、
   事件日志 1116 那条的 `Security intelligence Version`

| 字段                          | 填什么                                                              | 2026-09-22 本机实测值 |
| ----------------------------- | ------------------------------------------------------------------- | --------------------- |
| Detection name（安装包）      | `Trojan:Win32/Bearfoos.B!ml`                                        | ID 2147731849         |
| Detection name（装好的 exe）  | `Trojan:Win32/Bearfoos.A!ml`                                        | ID 2147731250         |
| Security intelligence version | `Get-MpComputerStatus` 的 `AntivirusSignatureVersion`               | `1.459.332.0`         |
| Engine version                | 同上的 `AMEngineVersion`                                            | `1.1.26080.3`         |
| Product version               | 同上的 `AMProductVersion`                                           | `4.18.26080.4`        |

⚠ **当前没有检出记录的文件也可以交。** 这一档的原话就是 "Software providers wanting to
validate detection of their products" —— 拿还没发布的构建来验，是这条通道的正常用法。

5. 说明栏照抄下面这段，一个字不用改：

> QB Gate is an open-source (AGPL-3.0) Windows desktop panel for managing local AI
> coding clients. Source: https://github.com/smithtaylor7748-ops/qb-gate
>
> It is a false positive. The binary is unsigned and has near-zero prevalence, and it
> legitimately performs actions that resemble malware behaviour, all of them only on
> explicit user action and all documented in the UI:
> setting Deny ACEs on other programs' executables (its core feature: blocking AI CLIs
> when the egress IP is outside the user's allowlist), terminating those programs when
> the IP changes, downloading official vendor installers and running them after
> verifying SHA-256 and Authenticode, attaching to a local Electron app over CDP to
> inject a UI-translation script, byte-scanning Chrome profile files to answer
> "has this machine ever signed in to claude.ai", and creating outbound Windows
> Firewall rules for a user-nominated browser.
>
> It does NOT: install any persistence (no Run/RunOnce key, no scheduled task),
> write to HKLM, use any packer or obfuscation (no UPX), embed any executable payload,
> or contact any command-and-control endpoint. All network destinations are the
> official vendor domains (anthropic.com, openai.com, microsoft.com, google.com).

**每出一个新版本都要重提一次** —— 放行是按哈希的，新构建是新哈希。
所以它跟 `CLAUDE.md` 那条「版本号只在出包那一刻提」是同一件事的两半：
**少发一版，就少一次申诉，而信誉也才有机会攒起来。**

## 5. 哪些办法对**别人**起作用（2026-09-22 核对）

上面 4.2 / 4.3 只管你自己这一台。下载你的包的人那边是**两道关**，
而且只有代码签名同时治两道：

| | Defender 把文件删掉 | SmartScreen 拦住不让运行 |
| --- | --- | --- |
| 判据 | ML 模型打分（`!ml`） | 文件 / 发布者的下载信誉 |
| 误报申诉 | ✅ 有效 | ❌ 没用 |
| 代码签名 | ✅ 有效 | ✅ 有效（但要慢慢攒） |
| 降低行为触发面 | ✅ 有效 | ❌ 没用 |

### 5.1 真能起作用的，按性价比排

1. **发布前提交误报申诉**（免费）—— 见 4.4。唯一免费且对全球所有 Defender 生效的动作。
2. **少发版本**（免费）—— 同一个哈希在外面活得越久，云信誉越有机会长起来；
   而且第 1 条的工作量是按版本数算的。规矩写在 `CLAUDE.md`。
3. **代码签名**（$10/月起）—— 见 [KNOWN-ISSUES](KNOWN-ISSUES.zh-CN.md)「代码签名」那一节，
   **那里有一条必须先看的预期纠正**：EV 证书已经不给即时信誉了。
4. **改 `perMachine` 安装**（免费，**未验**）—— 把「未签名 exe 在 AppData + HKCU 卸载键 +
   开始菜单快捷方式」这组特征拆掉，见第 1 节末尾那段 `Resources`。
   代价：安装与升级每次都要过一次 UAC，跟「别人的机器不一定有管理员」冲突；
   折中是 `"installMode": "both"`，让装的人自己选。**改之前先按第 6 节的办法量。**
5. **别只盯 Defender** —— 见 5.3。

### 5.2 看着有用、其实没用的（省点力气）

| 办法                                       | 为什么不行                                                                                                                                |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| **Microsoft Store / MSIX（微软替你签）**   | 商店分发的包确实由微软签名，一劳永逸 —— 但面板要写别人 exe 的 Deny ACE、强杀第三方进程、改防火墙与代理、UAC 提权跑 PowerShell：MSIX 沙箱与商店认证几乎不可能过。**不用试** |
| **winget**                                 | 它不替你签名，根因没动；而且提交管线自己会扫毒，**被报的包会直接被拒** —— 这个误报反而挡着你进 winget                                    |
| **改发 portable zip**                      | 被报的是 exe 本身，换不换安装壳无关（0.24.7 那次 exe 和 setup 是分别被报的）                                                              |
| **SHA256SUMS / GitHub artifact attestation** | 对杀软**零作用**，只对肯手动核验的人有用。但成本近零，值得做 —— 它属于「出事后你能自证」，不属于「不出事」                              |
| **教用户加排除项**                         | 只对照做的人有用；而且「请先关闭杀毒软件」本身就是恶意软件的经典话术，写进 README 会削弱可信度。放 FAQ，排在申诉说明之后                 |

### 5.3 别只盯 Defender：这个面板的使用者是中文用户

这份文档到这里为止一直只讲 Defender，但**火绒、360 安全卫士、腾讯电脑管家同样会报**，
而目标用户装这几家的比例不低。各家都有误报申诉通道，套路跟微软一样（交样本 + 说明），
响应通常还更快。

**第一步不是挨家申诉，是先摸清到底有谁在报**：把 `*-setup.exe` 交一次 VirusTotal。
它同时会把样本分发给各家引擎，一次提交多处生效，顺带还能量化第 3 节那些缓解到底有没有用。

## 6. 给下一个维护者

- 改动**减少了哪些信号**记在这份文档第 3 节，别只写在 commit message 里。
- 想验证某一项改动有没有用：改动前后各把 `*-setup.exe` 交一次 VirusTotal，比检出家数。
  **不要凭感觉**，这类启发式的行为没法从代码推出来。
- ⛔ 不要为了「过杀软」去做加壳、改入口点、加花指令这类事。那会把误报变成真阳性，
  而且跟这个项目「所有动作都看得见、都能撤销」的定位直接冲突。
