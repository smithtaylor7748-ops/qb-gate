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
}

export interface GateTarget {
  path: string;
  kind: 'Cli' | 'DesktopStub' | 'StaleCopy';
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

export interface Provider {
  id: string;
  name: string;
  base_url: string;
  model?: string | null;
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

export interface Installer {
  id: string;
  name: string;
  url: string;
  sha256: string;
  filename: string;
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
  allowlistRead: () => call<string[]>('allowlist_read'),
  allowlistWrite: (entries: string[]) => call<void>('allowlist_write', { entries }),
  allowlistAddCurrent: () => call<string[]>('allowlist_add_current'),
  watchdogStart: (mode: 'Cli' | 'Desktop') => call<void>('watchdog_start', { mode }),
  watchdogStop: () => call<void>('watchdog_stop'),

  // 探测
  probeIp: () => call<IpInfo>('probe_ip'),
  probePurity: () => call<PanelVerdict>('probe_purity'),
  probeDns: () => call<DnsReport>('probe_dns'),
  purityCriteria: () => call<PurityCriteria>('purity_criteria'),

  // 环境
  detectSoftware: () => call<SoftwareReport>('detect_software'),
  installersList: () => call<{ installers: Installer[] }>('installers_list'),
  installerFetch: (inst: Installer) => call<string>('installer_fetch', { inst }),

  // 账户
  accountsList: () => call<{ slots: Slot[]; caveat: string }>('accounts_list'),
  accountsSwitch: (label: string) => call<void>('accounts_switch', { label }),

  // 中转站
  relayCurrent: () => call<Provider | null>('relay_current'),
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

  // 一键关闭
  killswitchPreview: () => call<KillReport>('killswitch_preview'),
  killswitchExecute: () => call<KillReport>('killswitch_execute'),

  // 插件
  pluginList: () => call<PluginStatus[]>('plugin_list'),
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
