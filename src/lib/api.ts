import { call } from "./ipc";

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
import type { CategoryListing } from "./generated/CategoryListing";
import type { Check } from "./generated/Check";
import type { CheckItem } from "./generated/CheckItem";
import type { CheckState } from "./generated/CheckState";
import type { Checkup } from "./generated/Checkup";
import type { ClaudeInstall } from "./generated/ClaudeInstall";
import type { CleanupReport } from "./generated/CleanupReport";
import type { CreateOutcome } from "./generated/CreateOutcome";
import type { DependencyCheck } from "./generated/DependencyCheck";
import type { DesktopState } from "./generated/DesktopState";
import type { DnsReport } from "./generated/DnsReport";
import type { EnvHit } from "./generated/EnvHit";
import type { Evidence } from "./generated/Evidence";
import type { ExternalMethod } from "./generated/ExternalMethod";
import type { GateStatus } from "./generated/GateStatus";
import type { GateTarget } from "./generated/GateTarget";
import type { HookStatus } from "./generated/HookStatus";
import type { InstallKind } from "./generated/InstallKind";
import type { InstallProbe } from "./generated/InstallProbe";
import type { InstallResult } from "./generated/InstallResult";
import type { InstallTarget } from "./generated/InstallTarget";
import type { IpInfo } from "./generated/IpInfo";
import type { KillReport } from "./generated/KillReport";
import type { KillRole } from "./generated/KillRole";
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
import type { OfficialCatalogStatus } from "./generated/OfficialCatalogStatus";
import type { PackageProbe } from "./generated/PackageProbe";
import type { PanelVerdict } from "./generated/PanelVerdict";
import type { PluginState } from "./generated/PluginState";
import type { PluginStatus } from "./generated/PluginStatus";
import type { Preset } from "./generated/Preset";
import type { Profile } from "./generated/Profile";
import type { ProfileStore } from "./generated/ProfileStore";
import type { Progress } from "./generated/Progress";
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
import type { TavernConfig } from "./generated/TavernConfig";
import type { Trace } from "./generated/Trace";
import type { TraceReport } from "./generated/TraceReport";
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
  AccountsReport,
  ApplyReport,
  AssetItem,
  AuthStyle,
  BackupEntry,
  CategoryListing,
  Check,
  CheckItem,
  CheckState,
  Checkup,
  ClaudeInstall,
  CleanupReport,
  CreateOutcome,
  DependencyCheck,
  DesktopState,
  DnsReport,
  EnvHit,
  Evidence,
  ExternalMethod,
  GateStatus,
  GateTarget,
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
  OfficialCatalogStatus,
  PackageProbe,
  PanelVerdict,
  PluginState,
  PluginStatus,
  Preset,
  Profile,
  ProfileStore,
  Progress,
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
  TavernConfig,
  Trace,
  TraceReport,
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
  settingsSave: (next: Settings) => call<Settings>("settings_save", { next }),

  progressLoad: () => call<Progress>("progress_load"),
  progressSet: (id: string, state: StepState, risk: Risk, detail: string) =>
    call<Progress>("progress_set", { id, state, risk, detail }),

  // 升级
  upgradePlan: (channel: Channel = "latest") =>
    call<UpgradePlan>("upgrade_plan", { channel }),
  upgradeExecute: (channel: Channel = "latest", force = false) =>
    call<string>("upgrade_execute", { channel, force }),
  updateStatus: () => call<UpdateStatus>("update_status"),

  // 一键关闭
  officialSwitchPreview: () => call<KillReport>("official_switch_preview"),
  killswitchPreview: () => call<KillReport>("killswitch_preview"),
  killswitchExecute: () => call<KillReport>("killswitch_execute"),

  // 插件
  pluginList: () => call<PluginStatus[]>("plugin_list"),
  pluginCatalogStatus: () =>
    call<OfficialCatalogStatus>("plugin_catalog_status"),
  pluginStart: () => call<string>("plugin_start"),
  pluginStop: () => call<string[]>("plugin_stop"),
  tavernConfig: () => call<TavernConfig>("tavern_config"),
  tavernConfigSave: (cfg: TavernConfig) =>
    call<void>("tavern_config_save", { cfg }),

  // 酒馆资产
  tavernAssets: () => call<CategoryListing[]>("tavern_assets"),
  tavernBackup: () => call<BackupEntry>("tavern_backup"),
  tavernBackups: () => call<BackupEntry[]>("tavern_backups"),
  tavernRestore: (backupId: string) =>
    call<string>("tavern_restore", { backupId }),
};
