import { invoke } from '@tauri-apps/api/core';

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
   * 必须与 Rust `gate::targets::TargetKind` 的四个变体一一对应。
   *
   * `CliVersioned` 一度漏在这里 —— commit 14e1b80 给 `targets.rs` 加了
   * `%APPDATA%\Claude\claude-code\<版本>\` 这个布局却没同步前端类型，
   * 而它恰恰是本机最常见的一种副本。中文名在 `ui/labels.ts`。
   */
  kind: 'Cli' | 'CliVersioned' | 'DesktopStub' | 'StaleCopy';
  exists: boolean;
  locked: boolean;
}

export interface GateStatus {
  current_ip?: string | null;
  allowlist: string[];
  ip_allowed: boolean;
  targets: GateTarget[];
  all_locked: boolean;
  lease: { holder?: string | null; granted: string[] };
  watchdog_running: boolean;
  stale_copies: string[];
  recent_log: string[];
}

export interface Software {
  id: string;
  name: string;
  installed: boolean;
  version?: string | null;
  path?: string | null;
  advisory?: string | null;
}

export interface SoftwareReport {
  claudeCode: Software;
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
}

export interface AccountMigration {
  migrated: string[];
  backup?: string | null;
}

export interface AccountsReport {
  slots: Slot[];
  caveat: string;
  migration?: AccountMigration | null;
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

export interface Provider {
  target?: 'codex' | 'claude';
  id: string;
  name: string;
  base_url: string;
  model?: string | null;
  wire_api?: WireApi;
  auth_style?: AuthStyle;
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
export type LaunchTarget = 'claude-code' | 'claude-desktop';

export interface LaunchResult {
  target: LaunchTarget;
  /** 真正被拉起来的那个可执行文件。 */
  path: string;
  pid?: number | null;
  /** 挂上了哪一档看门狗。桌面端那档查不到 IP 会立即关闭，不给宽限。 */
  watchdog: 'Cli' | 'Desktop';
  detail: string;
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
  method: 'winget' | 'official_script' | 'npm_global' | 'manual_download';
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
  | 'unknown';

export interface UpgradePlan {
  installed?: string | null;
  available?: string | null;
  action: UpgradeAction;
  detail: string;
}

export type Evidence = 'AnthropicSigned' | 'BridgeAndDataDir';

export interface KillTarget {
  pid: number;
  name: string;
  path?: string | null;
  evidence: Evidence;
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
  gateRelease: () => call<void>('gate_release'),
  gateCleanStale: () => call<Array<[string, boolean]>>('gate_clean_stale'),
  /**
   * 备用入口：常规读取走 `gateStatus()`，它已经带 `allowlist` 字段。
   * 保留是因为这个文件与 Rust 的 handler 列表一一对应，这是它的既定不变量 ——
   * 为省三行破坏对应关系不划算。
   */
  allowlistRead: () => call<string[]>('allowlist_read'),
  allowlistWrite: (entries: string[]) => call<void>('allowlist_write', { entries }),
  allowlistAddCurrent: () => call<string[]>('allowlist_add_current'),
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
   * Authenticode），ClaudeGate 不需要自己再钉一份哈希 —— 而钉不上就意味着
   * 安装按钮永远是灰的，那才是真正的问题。
   */
  installProbe: () => call<InstallProbe>('install_probe'),
  installRun: (target: InstallTarget) => call<InstallResult>('install_run', { target }),

  // 账户
  accountsList: () => call<AccountsReport>('accounts_list'),
  accountsSwitch: (label: string) => call<void>('accounts_switch', { label }),

  // 中转站
  /** 每个 target 各一条（Claude / Codex），没配过的不出现。 */
  relayCurrent: () => call<Provider[]>('relay_current'),
  relayApply: (provider: Provider & { api_key?: string }) =>
    call<void>('relay_apply', { provider }),

  // 时区
  tzCurrent: () => call<string>('tz_current'),
  tzApply: (iana: string, restoreOnExit: boolean) =>
    call<{ original: string; applied: string; restore_on_exit: boolean }>('tz_apply', {
      iana,
      restoreOnExit,
    }),
  tzRestore: () => call<void>('tz_restore'),

  // 进度
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
