import { call } from "./ipc";
import type { Ipv6Status } from "./generated/Ipv6Status";

// ------------------------------------------------------------------ 类型
//
// **这里一个手写的 `interface` 都没有了。**
//
// 拆之前这个文件手抄了 75 个接口，而 Rust 侧改了它们**不会跟着变** ——
// `npm run types:check` 比对的是 `src/lib/generated/`，看不见手抄的那些。
// 文件里原本留着一条注释记着一次真实事故：`targets.rs` 加了新布局，
// 前端类型没同步，界面读到了不存在的字段，而所有检查照样全绿。
//
// 现在全部由 ts-rs 从 Rust 生成。Rust 侧改一个字段，`types:check` 当场红。
// 加一个新契约类型是两行：Rust 上挂 `#[derive(TS)] #[ts(export)]`，
// 再在 `src-tauri/examples/export-types.rs` 里添一行。
//
// 下面 import 一遍是给本文件的函数签名用，export 一遍是给页面用 ——
// 页面的 import 路径（`from "../lib/api"`）一个字都没变。

import type { AccountsReport } from "./generated/AccountsReport";
import type { ApplyReport } from "./generated/ApplyReport";
import type { AssetItem } from "./generated/AssetItem";
import type { AuthStyle } from "./generated/AuthStyle";
import type { BackupEntry } from "./generated/BackupEntry";
import type { BrowserAudit } from "./generated/BrowserAudit";
import type { BrowserExtension } from "./generated/BrowserExtension";
import type { CategoryListing } from "./generated/CategoryListing";
import type { Check } from "./generated/Check";
import type { CheckItem } from "./generated/CheckItem";
import type { CheckState } from "./generated/CheckState";
import type { Checkup } from "./generated/Checkup";
import type { ClaudeInstall } from "./generated/ClaudeInstall";
import type { CleanupReport } from "./generated/CleanupReport";
import type { CreateOutcome } from "./generated/CreateOutcome";
import type { DeleteOutcome } from "./generated/DeleteOutcome";
import type { DependencyCheck } from "./generated/DependencyCheck";
import type { DesktopState } from "./generated/DesktopState";
import type { DnsReport } from "./generated/DnsReport";
import type { EgressChecks } from "./generated/EgressChecks";
import type { EnvHit } from "./generated/EnvHit";
import type { Evidence } from "./generated/Evidence";
import type { ExternalMethod } from "./generated/ExternalMethod";
import type { FirewallRule } from "./generated/FirewallRule";
import type { GateStatus } from "./generated/GateStatus";
import type { GateTarget } from "./generated/GateTarget";
import type { HookStatus } from "./generated/HookStatus";
import type { InstallKind } from "./generated/InstallKind";
import type { InstallProbe } from "./generated/InstallProbe";
import type { InstallResult } from "./generated/InstallResult";
import type { InstallTarget } from "./generated/InstallTarget";
import type { IpInfo } from "./generated/IpInfo";
import type { IpLookupReport } from "./generated/IpLookupReport";
import type { KillReport } from "./generated/KillReport";
import type { KillRole } from "./generated/KillRole";
import type { GeminiBridgeStatus } from "./generated/GeminiBridgeStatus";
import type { BridgeHealth } from "./generated/BridgeHealth";
import type { BridgeSettings } from "./generated/BridgeSettings";
import type { BridgeSettingsView } from "./generated/BridgeSettingsView";
import type { TavernGptQuota } from "./generated/TavernGptQuota";
import type { TavernGeminiQuota } from "./generated/TavernGeminiQuota";
import type { TavernRoleplayTest } from "./generated/TavernRoleplayTest";
import type { BridgeTelemetry } from "./generated/BridgeTelemetry";
import type { UiStatus } from "./generated/UiStatus";
import type { UiConfig } from "./generated/UiConfig";
import type { DangerRules } from "./generated/DangerRules";
import type { KillTarget } from "./generated/KillTarget";
import type { LaunchResult } from "./generated/LaunchResult";
import type { LaunchTarget } from "./generated/LaunchTarget";
import type { Lease } from "./generated/Lease";
import type { ManagedApp } from "./generated/ManagedApp";
import type { ManagedAppStatus } from "./generated/ManagedAppStatus";
import type { ManagedExternal } from "./generated/ManagedExternal";
import type { ManagedProbe } from "./generated/ManagedProbe";
import type { ManagedStatus } from "./generated/ManagedStatus";
import type { MigrateReport } from "./generated/MigrateReport";
import type { NetAdapter } from "./generated/NetAdapter";
import type { OfficialCatalogStatus } from "./generated/OfficialCatalogStatus";
import type { PackageProbe } from "./generated/PackageProbe";
import type { PanelVerdict } from "./generated/PanelVerdict";
import type { PluginState } from "./generated/PluginState";
import type { EgressConfig } from "./generated/EgressConfig";
import type { EgressInstall } from "./generated/EgressInstall";
import type { PluginStatus } from "./generated/PluginStatus";
import type { PolicyScope } from "./generated/PolicyScope";
import type { PolicyValue } from "./generated/PolicyValue";
import type { Preset } from "./generated/Preset";
import type { Profile } from "./generated/Profile";
import type { ProfileStore } from "./generated/ProfileStore";
import type { AccountProbe } from "./generated/AccountProbe";
import type { AccountProbeState } from "./generated/AccountProbeState";
import type { Progress } from "./generated/Progress";
import type { ProxyState } from "./generated/ProxyState";
import type { PurgeAction } from "./generated/PurgeAction";
import type { PurgeCategory } from "./generated/PurgeCategory";
import type { PurgeItem } from "./generated/PurgeItem";
import type { PurgeReport } from "./generated/PurgeReport";
import type { PurgeTarget } from "./generated/PurgeTarget";
import type { PurityCriteria } from "./generated/PurityCriteria";
import type { RelayTarget } from "./generated/RelayTarget";
import type { Resolver } from "./generated/Resolver";
import type { Risk } from "./generated/Risk";
import type { SecretHit } from "./generated/SecretHit";
import type { Settings } from "./generated/Settings";
import type { Slot } from "./generated/Slot";
import type { SlotUsage } from "./generated/SlotUsage";
import type { SnapshotEntry } from "./generated/SnapshotEntry";
import type { SnapshotManifest } from "./generated/SnapshotManifest";
import type { Software } from "./generated/Software";
import type { SoftwareReport } from "./generated/SoftwareReport";
import type { StepRecord } from "./generated/StepRecord";
import type { StepState } from "./generated/StepState";
import type { SwitchReport } from "./generated/SwitchReport";
import type { SyncReport } from "./generated/SyncReport";
import type { TavernCandidate } from "./generated/TavernCandidate";
import type { GptBridgeStatus } from "./generated/GptBridgeStatus";
import type { TavernBackend } from "./generated/TavernBackend";
import type { TavernConfig } from "./generated/TavernConfig";
import type { TavernEvidence } from "./generated/TavernEvidence";
import type { TavernSurvey } from "./generated/TavernSurvey";
import type { TokenBucket } from "./generated/TokenBucket";
import type { TokenSummary } from "./generated/TokenSummary";
import type { TokenUsage } from "./generated/TokenUsage";
import type { UsageOverview } from "./generated/UsageOverview";
import type { BrowserReport } from "./generated/BrowserReport";
import type { ProbeStart } from "./generated/ProbeStart";
import type { Trace } from "./generated/Trace";
import type { TraceReport } from "./generated/TraceReport";
import type { UpdateInfo } from "./generated/UpdateInfo";
import type { UpdateStatus } from "./generated/UpdateStatus";
import type { UpgradeAction } from "./generated/UpgradeAction";
import type { UpgradePlan } from "./generated/UpgradePlan";
import type { UsageSource } from "./generated/UsageSource";
import type { UsageWindow } from "./generated/UsageWindow";
import type { VersionEntry } from "./generated/VersionEntry";
import type { WatchMode } from "./generated/WatchMode";
import type { WireApi } from "./generated/WireApi";
import type { UpgradeChannel as Channel } from "./generated/UpgradeChannel";

export type {
  AccountProbe,
  AccountProbeState,
  AccountsReport,
  ApplyReport,
  AssetItem,
  AuthStyle,
  BackupEntry,
  BrowserAudit,
  BrowserExtension,
  CategoryListing,
  Check,
  CheckItem,
  CheckState,
  Checkup,
  ClaudeInstall,
  CleanupReport,
  CreateOutcome,
  DeleteOutcome,
  DependencyCheck,
  DesktopState,
  DnsReport,
  EgressChecks,
  EnvHit,
  Evidence,
  ExternalMethod,
  FirewallRule,
  GateStatus,
  GateTarget,
  GptBridgeStatus,
  GeminiBridgeStatus,
  UiStatus,
  UiConfig,
  DangerRules,
  HookStatus,
  InstallKind,
  InstallProbe,
  InstallResult,
  InstallTarget,
  IpInfo,
  KillReport,
  KillRole,
  KillTarget,
  LaunchResult,
  LaunchTarget,
  Lease,
  ManagedApp,
  ManagedAppStatus,
  ManagedExternal,
  ManagedProbe,
  ManagedStatus,
  MigrateReport,
  NetAdapter,
  OfficialCatalogStatus,
  PackageProbe,
  PanelVerdict,
  PluginState,
  PluginStatus,
  PolicyScope,
  PolicyValue,
  Preset,
  Profile,
  ProfileStore,
  Progress,
  ProxyState,
  PurgeAction,
  PurgeCategory,
  PurgeItem,
  PurgeReport,
  PurgeTarget,
  PurityCriteria,
  RelayTarget,
  Resolver,
  Risk,
  SecretHit,
  Settings,
  Slot,
  SlotUsage,
  SnapshotEntry,
  SnapshotManifest,
  Software,
  SoftwareReport,
  StepRecord,
  StepState,
  SwitchReport,
  SyncReport,
  TavernBackend,
  TavernCandidate,
  TavernConfig,
  TavernEvidence,
  TavernSurvey,
  TokenBucket,
  TokenSummary,
  TokenUsage,
  UsageOverview,
  Trace,
  TraceReport,
  UpdateInfo,
  UpdateStatus,
  UpgradeAction,
  UpgradePlan,
  UsageSource,
  UsageWindow,
  VersionEntry,
  WatchMode,
  WireApi,
  Channel,
};

// ------------------------------------------------------------------ 调用

// `call` 在 `ipc.ts` 里 —— 全项目只有那一个。

export const api = {
  // 门禁
  gateStatus: () => call<GateStatus>("gate_status"),
  gateLockAll: () => call<number>("gate_lock_all"),
  gateUnlockAll: () => call<number>("gate_unlock_all"),
  gateOpen: (holder: string) => call<void>("gate_open", { holder }),
  /**
   * 重新放行：验一次出口 IP，过了就把门重新打开并接回看门狗。
   *
   * 跟 `gateOpen` 的区别是**不需要说出 holder** —— 沿用上一次那个，
   * 因为点它的场景永远是「门被面板自己关上了，我要开回来」。
   * 它**不启动任何进程**。
   */
  gateReopen: () => call<string>("gate_reopen"),
  gateRelease: () => call<void>("gate_release"),
  gateCleanStale: () => call<Array<[string, boolean]>>("gate_clean_stale"),
  /**
   * 备用入口：常规读取走 `gateStatus()`，它已经带 `allowlist` 字段。
   * 保留是因为这个文件与 Rust 的 handler 列表一一对应，这是它的既定不变量 ——
   * 为省三行破坏对应关系不划算。
   */
  allowlistRead: () => call<string[]>("allowlist_read"),
  allowlistWrite: (entries: string[]) =>
    call<void>("allowlist_write", { entries }),
  /**
   * 把当前出口 IP 加进白名单。**国家不合格会直接报错，一个字都不写。**
   *
   * 手改白名单（`allowlistWrite`）不做这个检查 —— 见 Rust 侧的说明：
   * 国家层是判定时生效的，塞进去的脏 IP 照样用不了。
   */
  allowlistAddCurrent: () => call<string[]>("allowlist_add_current"),
  /** 国家白名单的两个起手式：`[名字, 国家码[]]`。面板不替你选。 */
  countryPresets: () => call<Array<[string, string[]]>>("country_presets"),

  // 会话内门禁（装进 Claude Code 的 hook）
  hookStatus: () => call<HookStatus>("hook_status"),
  /** 装。**白名单为空时会拒绝** —— 装上等于每次请求都被拦。 */
  hookInstall: () => call<HookStatus>("hook_install"),
  hookUninstall: () => call<HookStatus>("hook_uninstall"),
  watchdogStart: (mode: "Cli" | "Desktop") =>
    call<void>("watchdog_start", { mode }),
  watchdogStop: () => call<void>("watchdog_stop"),

  /**
   * 验 IP → 解锁 → 真的把进程拉起来 → 挂看门狗。任何一步失败都回滚并重新上锁。
   *
   * `gate_open` 只解锁不启动，所以旧界面上那两个叫「启动 Claude Code」
   * 「启动 Claude 桌面端」的按钮其实一个进程都没起过 —— 点完什么都不发生，
   * 用户还得自己去找 exe，而 exe 上恰好挂着 Deny ACE。
   */
  launchClaude: (target: LaunchTarget) =>
    call<LaunchResult>("launch_claude", { target }),

  // 探测
  probeIp: () => call<IpInfo>("probe_ip"),
  lookupIp: (ip: string) => call<IpLookupReport>("probe_ip_lookup", { ip }),
  probePurity: () => call<PanelVerdict>("probe_purity"),
  probeDns: () => call<DnsReport>("probe_dns"),
  purityCriteria: () => call<PurityCriteria>("purity_criteria"),

  // 环境
  detectSoftware: () => call<SoftwareReport>("detect_software"),

  /**
   * 安装走 winget 优先 + 官方安装器兜底。
   *
   * 原来那套「自己下 exe 再钉 SHA-256」已经下线：完整性在 Windows 上本来就有
   * 三层保障（winget manifest、官方安装器自带的签名清单、二进制上的
   * Authenticode），QB Gate 不需要自己再钉一份哈希 —— 而钉不上就意味着
   * 安装按钮永远是灰的，那才是真正的问题。
   */
  installProbe: () => call<InstallProbe>("install_probe"),
  installRun: (target: InstallTarget) =>
    call<InstallResult>("install_run", { target }),

  /** 这台机器以前装过 / 登录过 Claude 吗。只读，不改任何东西。 */
  claudeTraces: () => call<TraceReport>("claude_traces"),
  /**
   * 卸掉 Chrome、删干净用户资料、再装回来。没装过就只装。
   *
   * ⚠ **会毁掉数据**：书签、密码、扩展、全部站点数据一起没，不可恢复。
   * 调用之前必须已经拿到使用者的确认 —— 后端不会再问第二次。
   * 只碰 Chrome，Edge / Firefox 一概不动。
   */
  chromeReinstall: () => call<string>("chrome_reinstall"),

  // 账户
  accountsList: () => call<AccountsReport>("accounts_list"),
  /** 新建一个空槽位（不复制任何凭证）。原来没有激活槽位时它直接成为当前的。 */
  accountsCreate: (label: string) =>
    call<CreateOutcome>("accounts_create", { label }),
  /**
   * 切换槽位（v0.9.0）：**先关闭全部 Claude**（桌面端、所有 Claude Code 会话、酒馆桥接），
   * 再把 Claude Code、酒馆桥接、（可选）桌面端三处指向一起换过去。**不自动启动任何东西**。
   *
   * - `desktop`：桌面端跟不跟着切（没有这个槽位的桌面端资料时，跟 = 新建一份空白的）。
   */
  accountsSwitch: (label: string, desktop: boolean) =>
    call<SwitchReport>("accounts_switch", { label, desktop }),
  /**
   * 删掉一个槽位。**不可恢复**，那个账户在这台机器上要重新登录一次。
   *
   * 当前激活的槽位删不了（后端拒），要先切到别的账户 ——
   * 删了它会留下一个悬空的 `claude-profile`，而没有任何东西会去修它。
   *
   * - `dropDesktop`：连 `%APPDATA%\Claude-<标签>` 一起删。
   *   不删的话，以后建同名槽位会静默继承那一份旧的桌面端身份。
   */
  accountsDelete: (label: string, dropDesktop: boolean) =>
    call<DeleteOutcome>("accounts_delete", { label, dropDesktop }),
  /**
   * 这个槽位用掉了多少 token。零网络请求 —— 只读槽位目录里的会话转写
   * （`projects\<项目>\<会话>.jsonl`），跟额度那两个窗口同一条口径。
   *
   * **只覆盖这个槽位目录里的会话**：没经过面板、直接用官方默认目录
   * `~\.claude` 跑的不在内。界面上要写明这一点。
   */
  accountsTokens: (label: string) =>
    call<TokenUsage>("accounts_tokens", { label }),
  /**
   * 用量小结（账户卡用，每分钟一次）：token、按官方 API 价折算的美元、
   * 今天 / 7 天 / 30 天三档、缓存命中。不带最近请求这类明细。
   *
   * `days`：`1` = 今天，`7` / `30` = 含今天的最近 N 天，`0` = 全部。
   * ⛔ 美元是「同样的 token 走 API 要付多少」，**订阅不按这个收费** —— 界面必须写明。
   */
  accountsTokenSummary: (label: string, days: number) =>
    call<TokenSummary>("accounts_token_summary", { label, days }),
  /**
   * 用量明细页：当前槽位的完整小结（按天 / 按模型 / 按小时 / 最近请求）
   * 加上「按账户」—— 后端把默认目录只扫一遍、分给所有槽位。零网络。
   */
  accountsUsageOverview: (label: string, days: number) =>
    call<UsageOverview>("accounts_usage_overview", { label, days }),
  /**
   * 用系统默认浏览器做一次「中文环境」采集（2026-09-24）：后端在 127.0.0.1 上开一个一次性
   * 小服务并打开默认浏览器。`opened: false` = 没替你打开，界面给链接让你自己开。
   */
  browserProbeStart: () => call<ProbeStart>("browser_probe_start"),
  /** 等那一次交回结果；两分钟没人交回是 `null`。 */
  browserProbeWait: (token: string) =>
    call<BrowserReport | null>("browser_probe_wait", { token }),
  /** 最近一次交回的报告（只在后端内存里，面板重启就没了）。 */
  browserProbeLast: () => call<BrowserReport | null>("browser_probe_last"),
  /**
   * 「这个账户现在还能用吗」—— 拿槽位里的令牌向官方发**一次**最小认证请求。
   *
   * ⛔ 这是面板唯一一处带着官方身份对外发请求的地方。它打的是公开的
   * `/v1/models`：不查额度、不打模型、不写回任何东西；本地令牌已过期时
   * 连请求都不发（免得把必然的 401 报成假警报）。
   * **调用之前必须已经拿到使用者当次的点击。**
   */
  accountProbe: (label: string) =>
    call<AccountProbe>("account_probe", { label }),
  /**
   * 出口一致性：跟当前出口 IP 对不上的那些东西。
   *
   * `country` 从 `R.ip` 那份读数里来 —— 出口 IP 全项目只探一处，
   * 这条命令自己不发探测。没测过就传 `null`，那一项会如实报「查不了」。
   */
  egressChecks: (country: string | null) =>
    call<EgressChecks>("egress_checks_scan", { country }),
  /** 修某一项（WebRTC / DoH 策略）。**调用之前必须已经拿到使用者当次的点击。** */
  egressFix: (id: string) => call<string>("egress_checks_fix", { id }),
  /** 撤销面板刚才改的那一项。 */
  egressUndo: (id: string) => call<string>("egress_checks_undo", { id }),

  // 托管安装（v0.9.0）
  /** 托管根目录在哪、Claude Code 与 Codex 装没装、什么版本。 */
  managedStatus: () => call<ManagedStatus>("managed_status"),
  /** **当场实测**一个目录能不能当托管根目录（建得出、写得进、锁得上也解得开）。 */
  managedProbeDir: (path: string) =>
    call<ManagedProbe>("managed_probe_dir", { path }),
  /** 换托管根目录：已装了东西就一键迁移过去（托管的 Claude Code 在跑会先关掉全部 Claude）。 */
  managedSetDir: (path: string) =>
    call<MigrateReport>("managed_set_dir", { path }),
  /** 面板没装的多余副本与准备怎么清。只看不动（要验签名，慢）。 */
  managedExternals: () => call<ManagedExternal[]>("managed_externals"),
  /** **彻底清除**一个软件的外部副本。托管那份必须已经装好。 */
  managedCleanup: (which: ManagedApp) =>
    call<CleanupReport>("managed_cleanup", { which }),
  /** 版本库里有哪几版可以退回去。最新的在前。 */
  managedHistory: (which: ManagedApp) =>
    call<VersionEntry[]>("managed_history", { which }),
  /**
   * 回滚到某一版。会**先把当前这份收进版本库再换**，换完重新上锁。
   * 正在跑的 Claude Code 不受影响（Windows 不卸已加载的映像），下次启动才生效。
   */
  managedRollback: (which: ManagedApp, version: string) =>
    call<string>("managed_rollback", { which, version }),

  // 完全卸载（0.19.0）
  /**
   * 完全卸载之前的**只读盘点**：这个软件在本机的全部落点，按类分好，
   * 每一项带绝对路径与归属依据。**只看不动。**
   *
   * 跟 `managedExternals` 的分界线：那个保留托管那份、只清多余副本；
   * 这个连托管那份、版本库、配置、认证、环境变量、账户槽位一起算进来。
   */
  purgePlan: (target: PurgeTarget) =>
    call<PurgeItem[]>("purge_plan", { target }),
  /**
   * **执行**完全卸载。调用之前界面必须已经让使用者看过盘点、输入了确认词 ——
   * 后端不会再问第二次。
   *
   * ⚠ 会关掉全部 Claude、摘执行锁、删文件与账户槽位，**不可恢复**。
   * 返回里的 `left` 是复扫之后还剩下的：空 = 真的清干净了。
   */
  purgeExecute: (target: PurgeTarget) =>
    call<PurgeReport>("purge_execute", { target }),

  relayPresets: (target: RelayTarget) =>
    call<Preset[]>("relay_presets", { target }),

  /**
   * 运行环境体检：系统代理、IPv6、浏览器 DoH、MCP 配置里的明文密钥。
   * 读注册表与本地配置，**不联网、不改任何东西**。
   */
  checkupScan: () => call<Checkup>("checkup_scan"),

  // 时区
  tzCurrent: () => call<string>("tz_current"),
  tzApply: (iana: string, restoreOnExit: boolean) =>
    call<{ original: string; applied: string; restore_on_exit: boolean }>(
      "tz_apply",
      {
        iana,
        restoreOnExit,
      },
    ),
  tzRestore: () => call<void>("tz_restore"),

  // 进度
  snapshotList: () => call<SnapshotEntry[]>("snapshot_list"),
  snapshotCreate: (note: string) =>
    call<SnapshotEntry>("snapshot_create", { note }),
  snapshotRestore: (id: string) => call<string>("snapshot_restore", { id }),
  snapshotRemove: (id: string) => call<void>("snapshot_remove", { id }),
  snapshotDir: (id: string) => call<string>("snapshot_dir", { id }),

  profileList: () => call<ProfileStore>("profile_list"),
  profileSave: (item: Profile) => call<string>("profile_save", { item }),
  profileRemove: (id: string) => call<void>("profile_remove", { id }),
  profileCapture: (name: string) => call<Profile>("profile_capture", { name }),
  /** 会切账户。**只由界面点击触发**，不要从任何自动路径调。 */
  profileApply: (id: string) => call<ApplyReport>("profile_apply", { id }),

  settingsLoad: () => call<Settings>("settings_load"),
  ipv6Status: () => call<Ipv6Status>("ipv6_status"),
  ipv6Set: (disable: boolean) => call<Ipv6Status>("ipv6_set", { disable }),
  settingsSave: (next: Settings) => call<Settings>("settings_save", { next }),

  progressLoad: () => call<Progress>("progress_load"),
  progressSet: (id: string, state: StepState, risk: Risk, detail: string) =>
    call<Progress>("progress_set", { id, state, risk, detail }),

  // 升级
  upgradePlan: (channel: Channel = "latest") =>
    call<UpgradePlan>("upgrade_plan", { channel }),
  upgradeExecute: (channel: Channel = "latest", force = false) =>
    call<string>("upgrade_execute", { channel, force }),
  // 应用自己的更新（0.25.3）。`updateStatus` 只读内存不联网；`updateCheck` 才问 GitHub。
  // ⛔ `updateInstall` 会让面板退出 —— 只能从更新弹窗里那颗「一键更新」调。
  updateStatus: () => call<UpdateStatus>("update_status"),
  updateCheck: (manual: boolean) =>
    call<UpdateStatus>("update_check", { manual }),
  updateSkip: (version: string | null) =>
    call<UpdateStatus>("update_skip", { version }),
  updateInstall: () => call<void>("update_install"),

  // 一键关闭
  officialSwitchPreview: () => call<KillReport>("official_switch_preview"),
  killswitchPreview: () => call<KillReport>("killswitch_preview"),
  killswitchExecute: () => call<KillReport>("killswitch_execute"),

  // 出站锁 / 系统代理 / 浏览器隐私面（0.19.0，「两个口子」）
  //
  // ⚠ 会改系统的那几条**只能由点击触发**。别从任何自动路径调它们 ——
  // 定时器、启动流程、看门狗都不行。Rust 侧 `commands/network.rs` 的文件头
  // 写着同一句：那条约束只能在调用侧保证。

  /** 当前由面板加的出站规则。**只列面板自己加的那些。** */
  firewallRules: () => call<FirewallRule[]>("firewall_rules"),
  /** 本机网卡。面板**不替你判断**哪块是物理网卡、哪块是 TUN。 */
  firewallAdapters: () => call<NetAdapter[]>("firewall_adapters"),
  /**
   * 给一个 exe 加出站阻止规则，点名的每块网卡各一条。
   *
   * ⚠ 规则**不随面板退出而消失**：面板关了、卸载了它还在，
   * 浏览器可能因此上不了网。卸载面板之前请先撤销（DISCLAIMER §5.2）。
   */
  firewallBlock: (exe: string, interfaces: string[]) =>
    call<FirewallRule[]>("firewall_block", { exe, interfaces }),
  /** 一键撤销面板加过的全部规则，返回删掉了几条。 */
  firewallRevokeAll: () => call<number>("firewall_revoke_all"),

  proxyRead: () => call<ProxyState>("proxy_read"),
  /** 面板动手之前那一份原值。`null` = 没改过，界面不该显示「还原」。 */
  proxyBackup: () => call<ProxyState | null>("proxy_backup"),
  /**
   * 改系统代理，返回改之前那一份。
   *
   * ⚠ **整机设置**：所有跟随系统代理的程序都会变，不只是那一个浏览器，
   * 改错会当场断网。调用之前界面必须已经把这件事说全。
   */
  proxyApply: (next: ProxyState) => call<ProxyState>("proxy_apply", { next }),
  /** 回滚到面板动手之前那一份。没有备份时报错，不静默成功。 */
  proxyRollback: () => call<string>("proxy_rollback"),

  /**
   * Chrome 隐私面的**只读**审计：策略、WebRTC、扩展的高风险权限。
   *
   * 扩展那一项**只报告** —— 面板没有禁用或删除扩展的入口，这是有意的。
   */
  browserAudit: () => call<BrowserAudit>("browser_audit"),
  /**
   * 把 WebRTC 收紧到 `disable_non_proxied_udp`（只影响当前用户）。
   *
   * ⛔ 返回的话里带着「注册表里有值 ≠ 策略已生效」——
   * **界面照原样显示，不许改写成「已生效」。** 面板读不到 chrome://policy。
   */
  browserWebrtcHarden: () => call<string>("browser_webrtc_harden"),
  /** 撤销上面那条。只删面板设的那条（HKCU），整机策略一个字不碰。 */
  browserWebrtcClear: () => call<string>("browser_webrtc_clear"),

  // 插件
  pluginList: () => call<PluginStatus[]>("plugin_list"),
  pluginCatalogStatus: () =>
    call<OfficialCatalogStatus>("plugin_catalog_status"),
  /** 起酒馆。缺省 Claude 后端（你自己的 bridge.py）；`"gpt"` 走面板内置的 GPT 桥接。 */
  pluginStart: (backend?: TavernBackend) =>
    call<string>("plugin_start", { backend: backend ?? null }),
  pluginStop: () => call<string[]>("plugin_stop"),
  /** 内置 GPT 桥接的状态（不含密钥）。 */
  tavernGptStatus: () => call<GptBridgeStatus>("tavern_gpt_status"),
  /** 酒馆 Custom 源要填的密钥。只在使用者点「复制密钥」时读。 */
  tavernGptToken: () => call<string>("tavern_gpt_token"),
  /**
   * 激活 GPT 槽位的联网额度。`refresh: false` 只读「最近一次」、**绝不联网**；
   * 只有使用者点了刷新才传 `true`（2026-09-23：联网额度只手动刷新）。
   */
  tavernGptQuota: (refresh: boolean) =>
    call<TavernGptQuota | null>("tavern_gpt_quota", { refresh }),
  /** 内置 Gemini 桥接的状态（不含密钥）。驱动的是官方 Gemini CLI，不是反重力本体。 */
  tavernGeminiStatus: () => call<GeminiBridgeStatus>("tavern_gemini_status"),
  // Claude 桥自己那两个网页搬进面板（0.32.0）。
  //
  // 调的是使用者自己那份 `bridge.py` 的 HTTP 接口（只绑回环）。
  // 面板**不分发它** —— 别人机器上可能是别的版本、甚至没有，
  // 所以这几条报错时界面要**照实显示后端那句话**，不要当成「没有数据」。
  tavernBridgeHealth: () => call<BridgeHealth>("tavern_bridge_health"),
  tavernBridgeSettings: () =>
    call<BridgeSettingsView>("tavern_bridge_settings"),
  tavernBridgeSettingsSave: (settings: BridgeSettings) =>
    call<void>("tavern_bridge_settings_save", { settings }),
  tavernBridgeTelemetry: (offset: number, limit: number) =>
    call<BridgeTelemetry>("tavern_bridge_telemetry", { offset, limit }),
  /** 清空调用日志。**不可逆** —— 调用方先弹确认框。 */
  tavernBridgeTelemetryClear: () =>
    call<number>("tavern_bridge_telemetry_clear"),
  tavernGeminiToken: () => call<string>("tavern_gemini_token"),
  /** 激活账户 Gemini CLI 那一半的联网额度。`refresh` 同上。 */
  tavernGeminiQuota: (refresh: boolean) =>
    call<TavernGeminiQuota | null>("tavern_gemini_quota", { refresh }),
  tavernBridgeRoleplayTest: (provider: "claude" | "gpt" | "gemini", fixture?: string) =>
    call<TavernRoleplayTest>("tavern_bridge_roleplay_test", {
      provider,
      fixture: fixture ?? null,
    }),

  // 反重力汉化与审批引擎（插件 antigravity-ui：只注入脚本，不起 Hub、不改文件）
  antigravityUiStatus: () => call<UiStatus>("antigravity_ui_status"),
  antigravityUiStart: () => call<UiStatus>("antigravity_ui_start"),
  antigravityUiStop: () => call<void>("antigravity_ui_stop"),
  antigravityUiConfigSave: (cfg: UiConfig) =>
    call<void>("antigravity_ui_config_save", { cfg }),
  antigravityUiRules: () => call<DangerRules>("antigravity_ui_rules"),
  antigravityUiRulesSave: (rules: DangerRules) =>
    call<void>("antigravity_ui_rules_save", { rules }),
  antigravityUiRulesReset: () =>
    call<DangerRules>("antigravity_ui_rules_reset"),

  // Claude 桌面端中文界面（插件 claude-desktop-zh-cn，2026-09-25）：
  // 运行时取上游 javaht/claude-desktop-zh-cn 的 Release，只调用它的安全模式。
  /** 现状。**不联网**（只读 Claude 安装目录、各份资料的界面语言、插件自己的记录）。 */
  claudeZhStatus: () =>
    call<import("./generated/ClaudeZhStatus").ClaudeZhStatus>(
      "claude_zh_status",
    ),
  /**
   * 问一次上游最新版。**会联网**（`github.com/<上游>/releases/latest` 的跳转，不走 api.github.com）。
   * 使用者选的节奏：只在打开汉化弹窗 / 插件页、或点「检查更新」时调，没有定时器。
   */
  claudeZhCheck: () =>
    call<import("./generated/ClaudeZhStatus").ClaudeZhStatus>(
      "claude_zh_check",
    ),
  /**
   * 一键汉化。`upgrade` = 先换成上游最新版。**会关掉 Claude 桌面端**（含 Code 页里的会话），
   * 界面事前说明。进度走 `claude-zh` 任务。
   */
  claudeZhApply: (upgrade: boolean) =>
    call<import("./generated/ClaudeZhOutcome").ClaudeZhOutcome>(
      "claude_zh_apply",
      { upgrade },
    ),
  /** 恢复英文：跑上游自己的卸载，再把各份资料的界面语言写回汉化之前的值。 */
  claudeZhRestore: () =>
    call<import("./generated/ClaudeZhOutcome").ClaudeZhOutcome>(
      "claude_zh_restore",
    ),
  // ⚠ `geminiCliInstall` 原来在这里也有一份、跟 `lib/antigravity.ts` 里那份一模一样。
  // 0.29.0 它变成了一个长任务（要带进度、要返回结果），两份定义必然漂开，
  // 所以只留 `antigravityApi.geminiCliInstall` 那一份。
  tavernConfig: () => call<TavernConfig>("tavern_config"),
  tavernConfigSave: (cfg: TavernConfig) =>
    call<void>("tavern_config_save", { cfg }),
  /** 在本机找酒馆与桥接。只读，不写配置 —— 采用哪一条由使用者点。 */
  tavernLocate: (deep: boolean) =>
    call<TavernSurvey>("tavern_locate", { deep }),

  // 酒馆资产
  tavernAssets: () => call<CategoryListing[]>("tavern_assets"),
  tavernBackup: () => call<BackupEntry>("tavern_backup"),
  tavernBackups: () => call<BackupEntry[]>("tavern_backups"),
  tavernRestore: (backupId: string) =>
    call<string>("tavern_restore", { backupId }),

  // Codex 出站与换出口插件（外部程序 ccodex-sleep-state，本面板只接入）
  codexEgressStatus: () => call<PluginStatus>("codex_egress_status"),
  codexEgressConfig: () => call<EgressConfig>("codex_egress_config"),
  codexEgressConfigSave: (cfg: EgressConfig) =>
    call<void>("codex_egress_config_save", { cfg }),
  /** 起它的 setup（接管真实 ~/.codex 并起服务）。核心官方识别线挂着时后端会拒绝。 */
  codexEgressStart: () => call<string>("codex_egress_start"),
  /** 结束本面板起的那份，再跑它自己的 restore 恢复 Codex 配置。 */
  codexEgressStop: () => call<string[]>("codex_egress_stop"),
  /** 一键从它的 Releases 下载 Windows 包、按 SHA256SUMS 校验、解压到默认位置并登记。进度走 egress-install 任务。 */
  codexEgressInstall: () => call<EgressInstall>("codex_egress_install"),
};
