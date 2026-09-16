/**
 * 所有共享数据的集中声明。
 *
 * 页面里**不要**自己写 fetcher —— 同一个 key 必须对应同一个请求和同一套
 * 轮询策略，否则又会退回到「三个页面各打一遍 probeIp」的老样子。
 *
 * 策略的取舍：
 *   - 打第三方接口的（ip / purity）给 stale，不轮询：省额度，切页回来才刷。
 *   - 本机状态（gate / plugins）可以轮询，反正是本地 ACL 和进程查询。
 *   - 慢的（dns 约 6 秒、signals 含 1 秒 WebRTC 超时）一律手动。
 */

import { api, type Channel, type IpInfo } from "./api";
import { DEMO_ENABLED } from "./demo";
import { peek, res } from "./store";
import { runScan } from "./signals";

export const R = {
  /** 出口 IP。全项目就这一处探测，Home / Purity / IpLock / Environment 共用。 */
  ip: res(() => api.probeIp(), { staleMs: 60_000, auto: false }),
  criteria: res(() => api.purityCriteria()),
  ipv6: res(() => api.ipv6Status(), { pollMs: 10_000 }),

  /** 门禁状态。本地 ACL 查询，轮询无妨。 */
  gate: res(() => api.gateStatus(), { pollMs: 15_000 }),

  /** 会话内门禁装没装。读两个本地文件，便宜。 */
  hook: res(() => api.hookStatus()),

  software: res(() => api.detectSoftware()),
  /**
   * 账户槽位。**要轮询** —— 用量读数是官方客户端自己往本机写的文件，
   * 桌面端约 15 分钟落一条样本，而这条资源原来没有 `pollMs` 也没有
   * `staleMs`，加上全局的 `refetchOnWindowFocus: false`，等于
   * 「一次会话只打一发」：文件更新了，页面纹丝不动。
   *
   * 轮询是安全的：`accounts_list` 是纯本地文件读，没有 PowerShell、
   * 没有网络请求，也不跑 `sync_bridge`（命令里传的是
   * `SyncReport::default()`）。符合本文件开头那条「本机状态可以轮询」。
   *
   * ⛔ **别把 token 统计塞进 `accounts_list`。** 那要扫几十 MB 的会话转写，
   * 进来之后这条轮询就变成每 30 秒磨一次盘。它走自己的按需命令
   * （`accounts_tokens`，只在详情页打开时跑）。
   */
  accounts: res(() => api.accountsList(), { pollMs: 30_000 }),
  progress: res(() => api.progressLoad()),
  /**
   * 官方端点预设。**只收厂商公开文档里的直连地址，一个第三方中转站都不放** ——
   * 理由写在 `src-tauri/src/relay/presets.rs` 的文件头。
   *
   * 两个 target 一起取：新系统的「服务商」这一层没有 target 概念，
   * target 是每个使用环境自己选的。
   */
  presets: res(async () => {
    const all = [
      ...(await api.relayPresets("claude-code")),
      ...(await api.relayPresets("codex")),
    ];
    // 按 id 去重：Rust 侧的 id 本来就是全局唯一的（presets.rs 有单测钉着），
    // 但演示夹具不看 target、两次都回同一批，合起来就重了。
    return [...new Map(all.map((p) => [p.id, p])).values()];
  }),
  tz: res(() => api.tzCurrent()),
  settings: res(() => api.settingsLoad()),
  snapshots: res(() => api.snapshotList()),
  profiles: res(() => api.profileList()),

  plugins: res(() => api.pluginList(), { pollMs: 20_000 }),

  /**
   * 出口一致性（综合评分第五项）。里面有两轮真实探测（绕过代理 / 跟随代理），
   * 所以跟 `dns` 同一档：点了才跑，跑过的留着。
   *
   * 出口国家从 `ip` 那份读数里取 —— 全项目只探一处（见本文件开头）。
   * `peek` 而不是再打一发：这条资源自己不该触发探测，没测过就传 `null`，
   * 后端那一项会如实报「查不了」。
   */
  egress: res(() => api.egressChecks(peek<IpInfo>("ip")?.countryCode ?? null), {
    auto: false,
    persist: true,
  }),

  /**
   * DNS 探测约 6 秒（10 个探针域名各等 3 秒上限），只在用户点了才跑。
   *
   * `persist` 是必须的：不留的话面板一重启，综合评分里这 25 分就回到
   * 「未检测」—— 昨天测过也白测，而重测要再等六秒。
   */
  dns: res(() => api.probeDns(), { auto: false, persist: true }),

  /**
   * 中文环境识别。纯本地计算，但 `runScan()` 是串行 `for await`，
   * WebRTC 那项自带 1 秒超时，所以也别自动跑。
   */
  signals: res(() => runScan(), { auto: false, persist: true }),

  /**
   * 本机环境体检。要跑几个 `reg query` 子进程、还要读一遍配置文件，
   * 跟 traces / dns 同一档：**用户点了才跑**。
   *
   * 演示模式下自动跑一次 —— 截图脚本点不了按钮，而一张「还没体检过」的空卡片
   * 说明不了这个功能在查什么。正式构建里 `DEMO_ENABLED` 是编译期常量 false。
   */
  checkup: res(() => api.checkupScan(), { auto: DEMO_ENABLED, persist: true }),

  /**
   * 升级计划。**渠道由界面那个下拉决定，不许写死。**
   *
   * 写死 `'latest'` 的后果不是「少一个功能」，是**界面在说谎**：选了 stable
   * 之后点「检查」，拿回来的仍然是 latest 的计划（「可升级 → 2.1.x」），
   * 而点「升级」时后端按 stable 重新算一遍，结论多半是「已是最新」——
   * 于是一次 `toast.ok('已是最新')` 配着一行写着「可升级」的读数，
   * 而且**什么都没装**。
   *
   * 「默认别落到 stable 上，那会降级」（档案 §4.5）由下拉的默认值 `latest`
   * 保证，不该由这里写死来保证 —— 写死同时把使用者的选择一起吞了。
   */
  upgradeOf: (channel: Channel) =>
    res(() => api.upgradePlan(channel), { auto: false }),

  /** winget 安装能力探测。 */
  install: res(() => api.installProbe()),

  /** 托管安装的现状（根目录、两个软件装没装、版本）。本地文件读取，便宜。 */
  managed: res(() => api.managedStatus()),

  /**
   * 面板没装的外部副本。每一份要删的文件都要先验签名（PowerShell，几百毫秒一个），
   * 所以**不自动跑**，用户点了才扫。
   */
  externals: res(() => api.managedExternals(), { auto: false }),

  /**
   * 版本库里留着的旧版本。读本地目录，便宜。
   *
   * 原来这一块自己在组件里 `useState` + `useEffect` 取，于是升级完之后
   * 列表不会变 —— 刚被归档的那一份要等你切走再切回来才出现。进资源层是
   * 为了让 `AFTER.install` 管得到它。
   */
  versions: res(() => api.managedHistory("claude-code")),

  /**
   * Claude 痕迹与 Chrome 状态。
   *
   * 要扫 Chrome 的 Cookie / History（可能几十 MB），所以**不自动跑** ——
   * 跟 dns / signals 同一档，用户点了才扫。
   */
  traces: res(() => api.claudeTraces(), { auto: false }),

  /**
   * 出站锁：面板加过哪几条规则、本机有哪些网卡。
   *
   * 两个都要起 PowerShell 子进程，跟 traces / externals 同一档：**点了才跑**。
   */
  firewallRules: res(() => api.firewallRules(), { auto: false }),
  adapters: res(() => api.firewallAdapters(), { auto: false }),

  /** 系统代理现状。两次 `reg query`，跟 `tz` 一样便宜，可以自动取。 */
  proxy: res(() => api.proxyRead()),
  /** 面板动手之前那一份原值。`null` = 没改过，界面就不该显示「还原」。 */
  proxyBackup: res(() => api.proxyBackup()),

  /**
   * Chrome 隐私面审计：要遍历每个 Profile 下的扩展清单并读注册表策略，
   * 跟 traces 同一档 —— **点了才跑**。
   */
  browserAudit: res(() => api.browserAudit(), { auto: false }),

  tavernConfig: res(() => api.tavernConfig()),
  tavernAssets: res(() => api.tavernAssets()),
  tavernBackups: res(() => api.tavernBackups()),
} as const;

/** 破坏性操作之后要作废哪些资源，集中列在这里，免得每个调用点各记一份。 */
export const AFTER = {
  /** 上锁 / 解锁 / 清残留 / 改白名单 —— 门禁状态变了。 */
  gate: ["gate"] as const,
  /** 放行或收回租约：门禁变了，账户的登录态也可能跟着变。 */
  lease: ["gate", "accounts"] as const,
  /** 装完 / 升完：软件版本、门禁目标、升级计划全都要重算。 */
  install: [
    "software",
    "gate",
    "upgrade",
    "install",
    "managed",
    "externals",
    "versions",
  ] as const,
  /**
   * 重装或清空浏览器：软件清单变了，痕迹也该重扫。
   *
   * `browserAudit` **必须**在里面。那份审计记着上一轮的扩展清单和 Chrome
   * 路径，而这两样正是刚刚被清掉的东西 —— 漏掉它，界面会在一个全新的、
   * 一个扩展都没有的 Chrome 上继续列出旧扩展的高危权限，而且不会自己消失
   * （`browserAudit` 是 `auto:false`，不作废就一直挂着）。
   */
  browser: ["software", "traces", "browserAudit"] as const,
  /** 改或还原系统代理之后：现状与那份备份都变了。 */
  proxy: ["proxy", "proxyBackup"] as const,
  //
  // ⚠ 出站锁规则表（`firewallRules`）与 Chrome 隐私审计（`browserAudit`）
  // **故意不在这张表里**。两份都是 `auto:false`，而 `invalidate` 对手动资源
  // 是「扔掉」（见 store.ts）—— 使用者正盯着那一块点按钮，扔掉会让它当场
  // 空一下，而他要看的恰恰是「我刚点的那一条变没变」。
  // 那两处在软件页里直接调 `refresh()`，重测一遍。
  //
  /** 酒馆启停。 */
  tavern: ["plugins", "gate"] as const,
  /** 应用档案 / 回滚快照：账户、中转站、门禁、快照列表全都可能变了。 */
  profile: ["accounts", "gate", "tz", "snapshots", "profiles"] as const,
};
