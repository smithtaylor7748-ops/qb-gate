import { invoke } from '@tauri-apps/api/core';

import { DEMO_ENABLED, demoCall } from './demo';

// ------------------------------------------------------------------ 类型

export type Check = 'Pass' | 'Fail' | 'Unknown';
export type StepState = 'pending' | 'passed' | 'failed' | 'skipped';
export type Risk = 'unknown' | 'low' | 'medium' | 'high';

export interface IpInfo {
  ip: string;
  asn?: number | null;
  asOrganization?: string | null;
  country?: string | null;
  countryCode?: string | null;
  region?: string | null;
  city?: string | null;
  timezone?: string | null;
  fraudScore?: number | null;
  isResidential?: boolean | null;
  isBroadcast?: boolean | null;
}

export interface PanelVerdict {
  purity: Check;
  residential: Check;
  native: Check;
  passed: boolean;
  note: string;
}

export interface PurityCriteria {
  ipqs: { url: string; criteria: string };
  ippure: { url: string; criteria: string };
  optional: Array<{ name: string; url: string }>;
  maxFraudScore: number;
  iproyal: string;
}

export interface Resolver {
  address: string;
  country_code?: string | null;
  country_name?: string | null;
  asn?: string | null;
  from_adapter: boolean;
  interface?: string | null;
  is_private: boolean;
  is_domestic: boolean;
}

export interface DnsReport {
  egress_asn?: string | null;
  resolvers: Resolver[];
  passed: boolean;
  findings: string[];
  upstream_conclusion?: string | null;
  note: string;
  score: number;
  ethernet_safe?: boolean | null;
}

export interface GateTarget {
  path: string;
  /**
   * 必须与 Rust `gate::targets::TargetKind` 的变体一一对应。
   *
   * `CliVersioned` 一度漏在这里 —— commit 14e1b80 给 `targets.rs` 加了
   * `%APPDATA%\Claude\claude-code\<版本>\` 这个布局却没同步前端类型，
   * 而它恰恰是本机最常见的一种副本。中文名在 `ui/labels.ts`。
   * v0.8.0 加了 `NativeVersion`（安装器版本库）与 `EditorExtension`（编辑器扩展自带）。
   */
  kind:
    | 'Managed'
    | 'ManagedVersion'
    | 'Cli'
    | 'CliVersioned'
    | 'NativeVersion'
    | 'EditorExtension'
    | 'DesktopStub'
    | 'StaleCopy'
    | 'CodexCli';
  exists: boolean;
  locked: boolean;
}

export interface GateStatus {
  current_ip?: string | null;
  allowlist: string[];
  ip_allowed: boolean;
  targets: GateTarget[];
  all_locked: boolean;
  lease: {
    holder?: string | null;
    granted: string[];
    /** 这个租约挂的是哪一档看门狗。面板重启后要按原样接回去。 */
    mode?: 'Cli' | 'Desktop' | null;
  };
  watchdog_running: boolean;
  stale_copies: string[];
  recent_log: string[];
  /**
   * 门是在使用者没要求的情况下关上的，而且还没能自己开回来。
   *
   * 非空时总览要挂常驻横幅。这是唯一一个使用者**必须知道**却完全看不见的
   * 状态：面板收在托盘里，`ip-gate.log` 等于没写，而症状要等他下次在
   * Claude 桌面端开新会话时才出现 —— 那时候他不会把两件事联系起来。
   */
  needs_reopen?: string | null;
}

export interface Software {
  id: string;
  name: string;
  installed: boolean;
  version?: string | null;
  path?: string | null;
  advisory?: string | null;
}

/**
 * 一份 Claude Code 副本在哪、属于哪一类。与 Rust `install::inventory::Kind` 一一对应，
 * 中文名在 `ui/labels.ts`。
 */
export type InstallKind =
  | 'managed'
  | 'managed_version'
  | 'native'
  | 'native_version'
  | 'winget'
  | 'scoop'
  | 'programs'
  | 'path'
  | 'desktop_managed'
  | 'msix_managed'
  | 'npm'
  | 'npm_native'
  | 'editor'
  | 'desktop_stub';

export interface ClaudeInstall {
  kind: InstallKind;
  path: string;
  /** 能不能加执行锁。npm 的批处理不能 —— 界面要如实说。 */
  lockable: boolean;
  /** 面板会不会拿它来启动 Claude Code。 */
  launchable: boolean;
  /** 「启动 Claude Code」此刻会用的就是这一份。 */
  preferred: boolean;
}

export interface SoftwareReport {
  claudeCode: Software;
  /** 本机全部 Claude Code 副本（不含桌面端存根）。 */
  claudeCodeInstalls: ClaudeInstall[];
  claudeDesktop: Software;
  codex: Software;
  browsers: Software[];
}

export interface Slot {
  label: string;
  active: boolean;
  logged_in: boolean;
  cli_days_left?: number | null;
  account_uuid?: string | null;
  /** 套餐，例如 `Claude Pro`。读自本槽位的 .claude.json，**不联网**。 */
  plan?: string | null;
  /** 计费方式，例如 `Google Play 订阅`。 */
  billing?: string | null;
  /** 官方客户端上次刷新这份档案的时间。这是缓存，可能过期。 */
  plan_fetched_at?: string | null;
  /** 桌面端有没有这个槽位自己的资料目录 `%APPDATA%\Claude-<标签>`。 */
  desktop_profile: boolean;
}

/** 酒馆桥接那边重复的凭证合并成一份时，这次做了什么。 */
export interface SyncReport {
  done: string[];
  /** 没做成的（通常是桥接正在用），下次列槽位会再试。 */
  failed: string[];
}

export interface DesktopState {
  /** `%APPDATA%\Claude` 已经交给面板管（是联结点）。 */
  managed: boolean;
  /** 桌面端现在用的是哪个槽位的资料。 */
  active?: string | null;
}

export interface AccountsReport {
  slots: Slot[];
  caveat: string;
  /** 套餐是怎么读出来的 —— 界面上要如实说明，不能让人以为是查了接口。 */
  planCaveat: string;
  sync: SyncReport;
  desktop: DesktopState;
  /** 本机有酒馆桥接的数据目录 —— 有的话切换会一起切它。 */
  bridgePresent: boolean;
}

export interface CreateOutcome {
  label: string;
  /** 原来没有激活槽位，新槽位直接成了当前的。 */
  activated: boolean;
  notes: string[];
}

export interface SwitchReport {
  /** 切换前清场（关闭全部 Claude）的报告。 */
  closed: KillReport;
  /** 这次换了哪几处指向：Claude Code / 酒馆桥接 / 桌面端。 */
  switched: string[];
  notes: string[];
}

/** 上游协议。**默认 `responses`** —— 写死成 `chat` 会把中转站配置改坏。 */
export type WireApi = 'responses' | 'chat';

/**
 * 凭证写在哪。不是风格偏好，是两个不同的落盘位置：
 * `env_key` 走 Codex 的 `auth.json` / Claude 的 `ANTHROPIC_API_KEY`，
 * `bearer_token` 走 provider 段的 `experimental_bearer_token` /
 * Claude 的 `ANTHROPIC_AUTH_TOKEN`。配错了中转站连不上。
 */
export type AuthStyle = 'env_key' | 'bearer_token' | 'none';

/** 中转站服务的哪个工具。三个是各自独立的列表。 */
export type RelayTarget = 'claude-desktop' | 'claude-code' | 'codex';

/** 一条供应商的公开信息。Rust 侧三个结构体共用这一份。 */
export interface ProviderMeta {
  id: string;
  target: RelayTarget;
  /** 写进 config.toml 的 `model_providers.<slug>`。Codex 用，Claude 侧忽略。 */
  slug: string;
  name: string;
  base_url: string;
  model?: string | null;
  wire_api: WireApi;
  auth_style: AuthStyle;
  note?: string | null;
  website?: string | null;
  icon?: string | null;
  sort: number;
  created_at: string;
}

/**
 * 后端回给前端的一条。
 *
 * **这里没有、也不会有任何 Key 字段** —— Rust 侧 `ProviderView` 结构上
 * 就装不下 Key，不是靠 `skip_serializing` 记得加。只有掩码和「有没有」。
 */
export interface ProviderView extends ProviderMeta {
  has_key: boolean;
  key_masked?: string | null;
  /** 落盘时是 DPAPI 加密存的吗。裸明文要在界面上提示。 */
  key_encrypted: boolean;
  active: boolean;
}

/** 前端发过去的一条。`api_key` 留空表示**保留原有的那把**，不是清空。 */
export interface ProviderInput extends ProviderMeta {
  api_key?: string;
}

export interface Preset {
  id: string;
  name: string;
  target: RelayTarget;
  base_url: string;
  wire_api: WireApi;
  auth_style: AuthStyle;
  model?: string | null;
  website?: string | null;
  note: string;
}

export interface ModelList {
  models: string[];
  detail: string;
}

/** 中转站背后到底是谁。 */
export type Backend = 'anthropic' | 'bedrock' | 'vertex' | 'unsure';

export interface BackendReport {
  backend: Backend;
  /** `strong` = 有直接指纹；`weak` = 只有负证据或弱信号。 */
  confidence: 'strong' | 'weak';
  /** 逆向来源，比如 Kiro。没把握就是 null —— **不硬猜**。 */
  source?: string | null;
  /** 逐条人话证据，界面上原样列出来让使用者自己复核。 */
  evidence: string[];
  /** 限流头真伪（连发两次看计数动没动）。null = 对方压根没给限流头。 */
  ratelimit_real?: boolean | null;
  detail: string;
}

export interface LatencyResult {
  base_url: string;
  /** 毫秒。null = 没连上。 */
  ms?: number | null;
  ok: boolean;
  detail: string;
}

/** 面板自己的开关。存 settings.json，会改变程序行为。 */
export interface Settings {
  /**
   * Codex 要不要也归 IP 门禁管。
   *
   * **默认 false。** 打开之后 `codex` 会跟 claude.exe 一样被加 Deny
   * ExecuteFile —— 出口 IP 不在白名单时命令直接被系统拒绝执行。
   */
  codex_under_gate: boolean;

  /**
   * 门禁被动关上之后，出口 IP 回到白名单时要不要自动重新放行。
   *
   * **默认 true。** 它只恢复租约、**不启动任何进程**，而且仍然要求
   * 出口 IP 已核实且在白名单里 —— 跟看门狗的 `ReclaimLease` 是同一条不变量。
   */
  gate_auto_rearm: boolean;

  /**
   * 面板托管安装的根目录。`null` = 默认位置 `%LOCALAPPDATA%\ClaudeIpGate\apps`。
   *
   * **只读**：只能经 `managedSetDir` 改（它会先实测新目录锁不锁得住、再把已装的搬过去），
   * `settingsSave` 会忽略这个字段。
   */
  managed_apps_dir?: string | null;

  /**
   * 国家白名单（ISO 3166-1 alpha-2 大写）。
   *
   * **空 = 这一层不启用，不是全拒。** 界面上必须显著标注「国家层未启用」，
   * 别让人以为配了。启用之后：出口 IP 落在名单外、查不出国家、
   * 或者几个探测源报的国家互相打架，一律按不合格处理。
   */
  country_allowlist: string[];

  /**
   * 会话内门禁装没装。**只读**：只能经 `hookInstall` / `hookUninstall` 改，
   * `settingsSave` 会忽略这个字段（它们还要写脚本、改槽位的 settings.json）。
   */
  hook_enabled: boolean;
}

/** 会话内门禁的现状。 */
export interface HookStatus {
  installed: boolean;
  /** 装在哪个槽位。`null` = 没有激活槽位，装在 `~\.claude`。 */
  slot?: string | null;
  settings_path?: string | null;
  script_path: string;
  /** 最近几条拦截记录，最新的在前。 */
  recent_blocks: string[];
}

// ------------------------------------------------------------ 托管安装

/** 面板托管安装的两个软件。与 Rust `install::managed::App` 一致。 */
export type ManagedApp = 'claude-code' | 'codex';

export interface ManagedAppStatus {
  app: ManagedApp;
  /** 托管那份的 exe 路径（在不在都给）。 */
  path: string;
  installed: boolean;
  version?: string | null;
  installed_at?: string | null;
}

export interface ManagedStatus {
  root: string;
  default_root: string;
  is_default: boolean;
  apps: ManagedAppStatus[];
}

/** 版本库里的一版。 */
export interface VersionEntry {
  version: string;
  path: string;
  sha256: string;
  archived_at: string;
  /** 这一份现在就是在用的那个版本。 */
  is_current: boolean;
  /** 版本库里每一份都是完整可执行的，所以照样要上锁。没锁上要显眼。 */
  locked: boolean;
}

/** 「这个目录能不能当托管根目录」的当场实测结果。 */
export interface ManagedProbe {
  ok: boolean;
  path: string;
  filesystem?: string | null;
  /** 不行时的原因，直接显示。 */
  reason?: string | null;
}

export interface MigrateReport {
  from: string;
  to: string;
  moved: string[];
  /** 为了搬走正在运行的 Claude Code 先关掉的 Claude 进程数。 */
  closed: number;
}

export type ExternalMethod = 'npm' | 'winget' | 'scoop' | 'delete' | 'registry';

/** 一份面板没装的外部副本，以及准备怎么清它。 */
export interface ManagedExternal {
  app: ManagedApp;
  method: ExternalMethod;
  target: string;
  action: string;
}

export interface CleanupReport {
  done: string[];
  failed: string[];
  notes: string[];
}

// -------------------------------------------------- Claude 痕迹与 Chrome

export interface Trace {
  kind: 'credential' | 'config' | 'install' | 'registry' | 'browser';
  label: string;
  path: string;
  detail: string;
}

export interface TraceReport {
  traces: Trace[];
  chrome_installed: boolean;
  chrome_path?: string | null;
  chrome_running: boolean;
  /**
   * Chrome 的资料文件扫过了吗。
   *
   * Chrome 正在跑时那些文件被占着打不开，这时候是 `false` ——
   * 界面必须说「先关掉 Chrome 再检测」，**不能显示成「没找到痕迹」**。
   * 那是两回事，而一个说谎的否定结论最难查。
   */
  chrome_scanned: boolean;
  winget_available: boolean;
}

export interface SnapshotManifest {
  created: string;
  note: string;
  active_account?: string | null;
  timezone?: string | null;
  all_locked: boolean;
  files: string[];
}

export interface SnapshotEntry {
  id: string;
  path: string;
  manifest: SnapshotManifest;
}

/** 一套用法：账户 + 三个中转站 + 时区。没填的部分保持原样，不会被清空。 */
export interface Profile {
  id: string;
  name: string;
  note?: string | null;
  /** 账户槽位标签。null = 不动账户。 */
  account?: string | null;
  /** target → provider id。缺的不动。 */
  relays: Record<string, string>;
  timezone?: string | null;
  sort: number;
  created_at: string;
}

export interface ProfileStore {
  profiles: Profile[];
  last_applied?: string | null;
}

export interface ApplyReport {
  applied: string[];
  skipped: string[];
  failed: string[];
  snapshot?: string | null;
  detail: string;
}

export interface UpdateStatus {
  current_version: string;
  repository?: string | null;
  configured: boolean;
  update_available: boolean;
  latest_version?: string | null;
  detail: string;
}

export interface StepRecord {
  state: StepState;
  risk: Risk;
  detail: string;
  updated_at?: string | null;
}

export interface Progress {
  steps: Record<string, StepRecord>;
  completed_once: boolean;
}

/** 装什么。字符串值与 Rust `install::winget::InstallTarget` 的 serde 名一致。 */
export type InstallTarget = 'claude-code' | 'claude-desktop' | 'codex';

/** 启动什么。与 `InstallTarget` 同名不同义，各自独立。 */
export type LaunchTarget = 'claude-code' | 'claude-desktop' | 'codex';

export interface LaunchResult {
  target: LaunchTarget;
  /** 真正被拉起来的那个可执行文件。 */
  path: string;
  pid?: number | null;
  /** 挂上了哪一档看门狗。桌面端那档查不到 IP 会立即关闭，不给宽限。 */
  watchdog: 'Cli' | 'Desktop';
  detail: string;
  /** Claude Code 用的是哪个账户槽位。`null` = 没有激活槽位，用的是它自己的默认目录。 */
  slot?: string | null;
}

export interface InstallProbe {
  /** winget 本身在不在。不在就只能给「打开官方下载页」。 */
  winget_available: boolean;
  winget_version?: string | null;
  packages: Array<{
    target: InstallTarget;
    /** winget 包 id，例如 `Anthropic.ClaudeCode`。 */
    id: string;
    /** 源里查得到这个包吗。 */
    found: boolean;
    available_version?: string | null;
    installed_version?: string | null;
  }>;
}

export interface InstallResult {
  target: InstallTarget;
  ok: boolean;
  /** 实际走的是哪条路：winget，还是官方脚本 / npm 兜底。 */
  method: 'winget' | 'managed' | 'official_script' | 'npm_global' | 'manual_download';
  /** 装完重新上锁了几个副本。 */
  relocked: number;
  /** Authenticode 主体里有没有对应的签名方（Claude 看 Anthropic，Codex 看 OpenAI）。查不到不阻断，只是警告。 */
  signature_ok: boolean | null;
  detail: string;
  log: string[];
}

export type PluginState = 'missing' | 'ready' | 'running' | 'broken';

export interface DependencyCheck {
  label: string;
  ok: boolean;
  detail: string;
}

export interface PluginStatus {
  id: string;
  name: string;
  state: PluginState;
  detail: string;
  checks: DependencyCheck[];
}

export interface OfficialCatalogStatus {
  configured: boolean;
  source?: string | null;
  signed: boolean;
  detail: string;
}

export interface TavernConfig {
  bridge_root: string;
  sillytavern_root: string;
  st_launcher: string;
  bridge_port: number;
  st_port: number;
}

export interface AssetItem {
  name: string;
  path: string;
  size: number;
  modified?: string | null;
  is_dir: boolean;
}

export interface CategoryListing {
  id: string;
  label: string;
  dir: string;
  exists: boolean;
  items: AssetItem[];
}

export interface BackupEntry {
  id: string;
  path: string;
  created: string;
  size: number;
}

export type Channel = 'latest' | 'stable';

export type UpgradeAction =
  | 'fresh_install'
  | 'upgrade'
  | 'up_to_date'
  | 'would_downgrade'
  | 'unknown'
  // 文件在，版本号读不出来。跟 fresh_install 是两回事，别合并。
  | 'version_unreadable';

export interface UpgradePlan {
  installed?: string | null;
  available?: string | null;
  action: UpgradeAction;
  detail: string;
}

export type Evidence = 'AnthropicSigned' | 'BridgeAndDataDir' | 'NpmPackage';

/** 进程属于哪一边。只用来告诉用户「关掉了什么」，不参与判定。 */
export type KillRole = 'desktop' | 'code' | 'bridge';

export interface KillTarget {
  pid: number;
  name: string;
  path?: string | null;
  evidence: Evidence;
  role: KillRole;
}

export interface KillReport {
  targets: KillTarget[];
  killed: number[];
  failed: Array<[number, string]>;
  relocked: number;
  spared: string[];
}

// ------------------------------------------------------------------ 调用

/** 统一把 Rust 侧的错误字符串抛成 Error，页面上只做 try/catch。 */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // 截图 / 界面预览用的演示数据。`VITE_DEMO` 没设成 1 时这是编译期常量 false，
  // 整个分支连同 demo.ts 一起被摇掉 —— 安装包里的数据来源只有 Rust 一处。
  if (DEMO_ENABLED) return demoCall<T>(cmd);
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(typeof e === 'string' ? e : String(e));
  }
}

export const api = {
  // 门禁
  gateStatus: () => call<GateStatus>('gate_status'),
  gateLockAll: () => call<number>('gate_lock_all'),
  gateUnlockAll: () => call<number>('gate_unlock_all'),
  gateOpen: (holder: string) => call<void>('gate_open', { holder }),
  /**
   * 重新放行：验一次出口 IP，过了就把门重新打开并接回看门狗。
   *
   * 跟 `gateOpen` 的区别是**不需要说出 holder** —— 沿用上一次那个，
   * 因为点它的场景永远是「门被面板自己关上了，我要开回来」。
   * 它**不启动任何进程**。
   */
  gateReopen: () => call<string>('gate_reopen'),
  gateRelease: () => call<void>('gate_release'),
  gateCleanStale: () => call<Array<[string, boolean]>>('gate_clean_stale'),
  /**
   * 备用入口：常规读取走 `gateStatus()`，它已经带 `allowlist` 字段。
   * 保留是因为这个文件与 Rust 的 handler 列表一一对应，这是它的既定不变量 ——
   * 为省三行破坏对应关系不划算。
   */
  allowlistRead: () => call<string[]>('allowlist_read'),
  allowlistWrite: (entries: string[]) => call<void>('allowlist_write', { entries }),
  /**
   * 把当前出口 IP 加进白名单。**国家不合格会直接报错，一个字都不写。**
   *
   * 手改白名单（`allowlistWrite`）不做这个检查 —— 见 Rust 侧的说明：
   * 国家层是判定时生效的，塞进去的脏 IP 照样用不了。
   */
  allowlistAddCurrent: () => call<string[]>('allowlist_add_current'),
  /** 国家白名单的两个起手式：`[名字, 国家码[]]`。面板不替你选。 */
  countryPresets: () => call<Array<[string, string[]]>>('country_presets'),

  // 会话内门禁（装进 Claude Code 的 hook）
  hookStatus: () => call<HookStatus>('hook_status'),
  /** 装。**白名单为空时会拒绝** —— 装上等于每次请求都被拦。 */
  hookInstall: () => call<HookStatus>('hook_install'),
  hookUninstall: () => call<HookStatus>('hook_uninstall'),
  watchdogStart: (mode: 'Cli' | 'Desktop') => call<void>('watchdog_start', { mode }),
  watchdogStop: () => call<void>('watchdog_stop'),

  /**
   * 验 IP → 解锁 → 真的把进程拉起来 → 挂看门狗。任何一步失败都回滚并重新上锁。
   *
   * `gate_open` 只解锁不启动，所以旧界面上那两个叫「启动 Claude Code」
   * 「启动 Claude 桌面端」的按钮其实一个进程都没起过 —— 点完什么都不发生，
   * 用户还得自己去找 exe，而 exe 上恰好挂着 Deny ACE。
   */
  launchClaude: (target: LaunchTarget) => call<LaunchResult>('launch_claude', { target }),

  // 探测
  probeIp: () => call<IpInfo>('probe_ip'),
  probePurity: () => call<PanelVerdict>('probe_purity'),
  probeDns: () => call<DnsReport>('probe_dns'),
  purityCriteria: () => call<PurityCriteria>('purity_criteria'),

  // 环境
  detectSoftware: () => call<SoftwareReport>('detect_software'),

  /**
   * 安装走 winget 优先 + 官方安装器兜底。
   *
   * 原来那套「自己下 exe 再钉 SHA-256」已经下线：完整性在 Windows 上本来就有
   * 三层保障（winget manifest、官方安装器自带的签名清单、二进制上的
   * Authenticode），QB Gate 不需要自己再钉一份哈希 —— 而钉不上就意味着
   * 安装按钮永远是灰的，那才是真正的问题。
   */
  installProbe: () => call<InstallProbe>('install_probe'),
  installRun: (target: InstallTarget) => call<InstallResult>('install_run', { target }),

  /** 这台机器以前装过 / 登录过 Claude 吗。只读，不改任何东西。 */
  claudeTraces: () => call<TraceReport>('claude_traces'),
  /**
   * 卸掉 Chrome、删干净用户资料、再装回来。没装过就只装。
   *
   * ⚠ **会毁掉数据**：书签、密码、扩展、全部站点数据一起没，不可恢复。
   * 调用之前必须已经拿到使用者的确认 —— 后端不会再问第二次。
   * 只碰 Chrome，Edge / Firefox 一概不动。
   */
  chromeReinstall: () => call<string>('chrome_reinstall'),

  // 账户
  accountsList: () => call<AccountsReport>('accounts_list'),
  /** 新建一个空槽位（不复制任何凭证）。原来没有激活槽位时它直接成为当前的。 */
  accountsCreate: (label: string) => call<CreateOutcome>('accounts_create', { label }),
  /**
   * 切换槽位（v0.9.0）：**先关闭全部 Claude**（桌面端、所有 Claude Code 会话、酒馆桥接），
   * 再把 Claude Code、酒馆桥接、（可选）桌面端三处指向一起换过去。**不自动启动任何东西**。
   *
   * - `desktop`：桌面端跟不跟着切（没有这个槽位的桌面端资料时，跟 = 新建一份空白的）。
   */
  accountsSwitch: (label: string, desktop: boolean) =>
    call<SwitchReport>('accounts_switch', { label, desktop }),

  // 托管安装（v0.9.0）
  /** 托管根目录在哪、Claude Code 与 Codex 装没装、什么版本。 */
  managedStatus: () => call<ManagedStatus>('managed_status'),
  /** **当场实测**一个目录能不能当托管根目录（建得出、写得进、锁得上也解得开）。 */
  managedProbeDir: (path: string) => call<ManagedProbe>('managed_probe_dir', { path }),
  /** 换托管根目录：已装了东西就一键迁移过去（托管的 Claude Code 在跑会先关掉全部 Claude）。 */
  managedSetDir: (path: string) => call<MigrateReport>('managed_set_dir', { path }),
  /** 面板没装的多余副本与准备怎么清。只看不动（要验签名，慢）。 */
  managedExternals: () => call<ManagedExternal[]>('managed_externals'),
  /** **彻底清除**一个软件的外部副本。托管那份必须已经装好。 */
  managedCleanup: (which: ManagedApp) => call<CleanupReport>('managed_cleanup', { which }),
  /** 版本库里有哪几版可以退回去。最新的在前。 */
  managedHistory: (which: ManagedApp) => call<VersionEntry[]>('managed_history', { which }),
  /**
   * 回滚到某一版。会**先把当前这份收进版本库再换**，换完重新上锁。
   * 正在跑的 Claude Code 不受影响（Windows 不卸已加载的映像），下次启动才生效。
   */
  managedRollback: (which: ManagedApp, version: string) =>
    call<string>('managed_rollback', { which, version }),

  // 中转站
  relayList: () => call<ProviderView[]>('relay_list'),
  relaySave: (provider: ProviderInput) => call<string>('relay_save', { provider }),
  relayDelete: (id: string) => call<void>('relay_delete', { id }),
  relayDuplicate: (id: string) => call<string | null>('relay_duplicate', { id }),
  relayActivate: (target: RelayTarget, id: string) =>
    call<void>('relay_activate', { target, id }),
  relayReorder: (target: RelayTarget, ids: string[]) =>
    call<void>('relay_reorder', { target, ids }),
  relayImportLive: (target: RelayTarget) =>
    call<ProviderMeta | null>('relay_import_live', { target }),
  /** 每个 target 的 live 配置各一条，没配过的不出现。 */
  relayCurrent: () => call<ProviderMeta[]>('relay_current'),
  relayPresets: (target: RelayTarget) => call<Preset[]>('relay_presets', { target }),
  /** 会把 Key 发到用户填的地址上，**只在用户点了才调**。 */
  relayFetchModels: (baseUrl: string, id?: string) =>
    call<ModelList>('relay_fetch_models', { baseUrl, id }),
  relayTestLatency: (baseUrl: string, id?: string) =>
    call<LatencyResult>('relay_test_latency', { baseUrl, id }),
  /**
   * 查这家的**真实后端**：Anthropic / Bedrock(Kiro) / Vertex(Antigravity)。
   *
   * 跟上面两个一样会把 Key 发出去，而且它要真发一次 `/v1/messages`，
   * **会消耗一点点额度**。只在用户点了才调。
   */
  relayDetectBackend: (baseUrl: string, id?: string, model?: string) =>
    call<BackendReport>('relay_detect_backend', { baseUrl, id, model }),

  // 时区
  tzCurrent: () => call<string>('tz_current'),
  tzApply: (iana: string, restoreOnExit: boolean) =>
    call<{ original: string; applied: string; restore_on_exit: boolean }>('tz_apply', {
      iana,
      restoreOnExit,
    }),
  tzRestore: () => call<void>('tz_restore'),

  // 进度
  snapshotList: () => call<SnapshotEntry[]>('snapshot_list'),
  snapshotCreate: (note: string) => call<SnapshotEntry>('snapshot_create', { note }),
  snapshotRestore: (id: string) => call<string>('snapshot_restore', { id }),
  snapshotRemove: (id: string) => call<void>('snapshot_remove', { id }),
  snapshotDir: (id: string) => call<string>('snapshot_dir', { id }),

  profileList: () => call<ProfileStore>('profile_list'),
  profileSave: (item: Profile) => call<string>('profile_save', { item }),
  profileRemove: (id: string) => call<void>('profile_remove', { id }),
  profileCapture: (name: string) => call<Profile>('profile_capture', { name }),
  /** 会切账户。**只由界面点击触发**，不要从任何自动路径调。 */
  profileApply: (id: string) => call<ApplyReport>('profile_apply', { id }),

  settingsLoad: () => call<Settings>('settings_load'),
  settingsSave: (next: Settings) => call<Settings>('settings_save', { next }),

  progressLoad: () => call<Progress>('progress_load'),
  progressSet: (id: string, state: StepState, risk: Risk, detail: string) =>
    call<Progress>('progress_set', { id, state, risk, detail }),

  // 升级
  upgradePlan: (channel: Channel = 'latest') => call<UpgradePlan>('upgrade_plan', { channel }),
  upgradeExecute: (channel: Channel = 'latest', force = false) =>
    call<string>('upgrade_execute', { channel, force }),
  updateStatus: () => call<UpdateStatus>('update_status'),

  // 一键关闭
  killswitchPreview: () => call<KillReport>('killswitch_preview'),
  killswitchExecute: () => call<KillReport>('killswitch_execute'),

  // 插件
  pluginList: () => call<PluginStatus[]>('plugin_list'),
  pluginCatalogStatus: () => call<OfficialCatalogStatus>('plugin_catalog_status'),
  pluginStart: () => call<string>('plugin_start'),
  pluginStop: () => call<string[]>('plugin_stop'),
  tavernConfig: () => call<TavernConfig>('tavern_config'),
  tavernConfigSave: (cfg: TavernConfig) => call<void>('tavern_config_save', { cfg }),

  // 酒馆资产
  tavernAssets: () => call<CategoryListing[]>('tavern_assets'),
  tavernBackup: () => call<BackupEntry>('tavern_backup'),
  tavernBackups: () => call<BackupEntry[]>('tavern_backups'),
  tavernRestore: (backupId: string) => call<string>('tavern_restore', { backupId }),
};
