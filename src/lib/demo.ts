/**
 * 演示数据 —— 只给截图和界面预览用。
 *
 * # 为什么需要它
 *
 * 面板里每一个数字都是使用者本机的真实状态：出口 IP、账户槽位、套餐、
 * 安装路径、日志。直接跑起来截图，等于把这些一起发出去。所以截图走这一层，
 * 里面**全部是编造的**：IP 用 RFC 5737 的文档保留段 `203.0.113.0/24`
 * （跟 Rust 侧单测同一个约定），路径用 `C:\Users\demo\`，ASN 用 RFC 5398
 * 的文档保留号 `AS64501`。没有一条来自任何真实机器。
 *
 * # 怎么开
 *
 * ```bash
 * npm run demo     # vite --mode demo，读 .env.demo 里的 VITE_DEMO=1
 * ```
 *
 * `VITE_DEMO` 不等于 `1` 时 `DEMO_ENABLED` 是编译期常量 false，整个模块会被
 * 摇掉 —— **正式安装包里没有这些数据，也没有这条分支**，数据来源仍然只有
 * Rust 一处。
 */

import type {
  AccountsReport,
  BackupEntry,
  CategoryListing,
  CleanupReport,
  DnsReport,
  GateStatus,
  InstallProbe,
  IpInfo,
  KillReport,
  ManagedExternal,
  ManagedStatus,
  OfficialCatalogStatus,
  PanelVerdict,
  PluginStatus,
  Preset,
  Profile,
  ProfileStore,
  Progress,
  ProviderMeta,
  ProviderView,
  PurityCriteria,
  Settings,
  SnapshotEntry,
  SoftwareReport,
  TavernConfig,
  TraceReport,
  UpdateStatus,
  UpgradePlan,
} from './api';

export const DEMO_ENABLED = import.meta.env.VITE_DEMO === '1';

const HOME = 'C:\\Users\\demo';
const APPS = `${HOME}\\AppData\\Local\\ClaudeIpGate\\apps`;

const ip: IpInfo = {
  ip: '203.0.113.7',
  asn: 64501,
  asOrganization: 'Example Broadband LLC',
  country: 'United States',
  countryCode: 'US',
  region: 'Virginia',
  city: 'Ashburn',
  timezone: 'America/New_York',
  fraudScore: 0,
  isResidential: true,
  isBroadcast: false,
};

const purity: PanelVerdict = {
  purity: 'Pass',
  residential: 'Pass',
  native: 'Pass',
  passed: true,
  note: '面板自查只作参考，三项硬指标以 IPQualityScore 与 ippure.com 的结论为准。',
};

const criteria: PurityCriteria = {
  ipqs: {
    url: 'https://www.ipqualityscore.com/free-ip-lookup-proxy-vpn-test',
    criteria: 'Fraud Score ≤ 5，且 Proxy / VPN / TOR / Recent Abuse 全为 No，Connection Type = Residential',
  },
  ippure: {
    url: 'https://ippure.com/',
    criteria: 'IPPure 系数 ≤ 5%，IP来源 = 原生IP，IP属性 = 住宅IP',
  },
  optional: [
    { name: 'ipinfo.io', url: 'https://ipinfo.io/' },
    { name: 'Scamalytics', url: 'https://scamalytics.com/ip' },
  ],
  maxFraudScore: 5,
  iproyal: 'https://iproyal.com/',
};

const gate: GateStatus = {
  current_ip: '203.0.113.7',
  allowlist: ['203.0.113.7'],
  ip_allowed: true,
  targets: [
    { path: `${APPS}\\claude-code\\claude.exe`, kind: 'Managed', exists: true, locked: true },
    { path: `${HOME}\\.local\\bin\\claude.exe`, kind: 'Cli', exists: true, locked: true },
    {
      path: `${HOME}\\AppData\\Roaming\\Claude\\claude-code\\2.0.14\\claude.exe`,
      kind: 'CliVersioned',
      exists: true,
      locked: true,
    },
    {
      path: `${HOME}\\AppData\\Local\\AnthropicClaude\\claude.exe`,
      kind: 'DesktopStub',
      exists: true,
      locked: true,
    },
  ],
  all_locked: true,
  lease: { holder: null, granted: [], mode: null },
  watchdog_running: false,
  stale_copies: [],
  recent_log: [
    '2026-09-12 09:41:07  出口 IP 203.0.113.7 在白名单内',
    '2026-09-12 09:41:07  4 个副本已上锁（托管 / 官方安装器 / 版本库 / 桌面端存根）',
    '2026-09-12 09:12:33  租约已收回，看门狗停止',
    '2026-09-12 08:55:19  已放行 claude-code，看门狗接管（Cli 档）',
  ],
  needs_reopen: null,
};

const software: SoftwareReport = {
  claudeCode: {
    id: 'claude-code',
    name: 'Claude Code',
    installed: true,
    version: '2.0.14',
    path: `${APPS}\\claude-code\\claude.exe`,
    advisory: null,
  },
  claudeCodeInstalls: [
    {
      kind: 'managed',
      path: `${APPS}\\claude-code\\claude.exe`,
      lockable: true,
      launchable: true,
      preferred: true,
    },
    {
      kind: 'native',
      path: `${HOME}\\.local\\bin\\claude.exe`,
      lockable: true,
      launchable: true,
      preferred: false,
    },
    {
      kind: 'native_version',
      path: `${HOME}\\.local\\share\\claude\\versions\\2.0.14`,
      lockable: true,
      launchable: false,
      preferred: false,
    },
  ],
  claudeDesktop: {
    id: 'claude-desktop',
    name: 'Claude 桌面端',
    installed: true,
    version: '0.14.2',
    path: `${HOME}\\AppData\\Local\\AnthropicClaude\\claude.exe`,
    advisory: null,
  },
  codex: {
    id: 'codex',
    name: 'Codex',
    installed: true,
    version: '0.28.0',
    path: `${APPS}\\codex\\codex.exe`,
    advisory: '默认不在 IP 门禁范围内，可在「设置 → 门禁范围」里打开。',
  },
  browsers: [
    {
      id: 'chrome',
      name: 'Google Chrome',
      installed: true,
      version: '141.0.7390.55',
      path: 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
      advisory: null,
    },
  ],
};

const accounts: AccountsReport = {
  slots: [
    {
      label: 'demo-main',
      active: true,
      logged_in: true,
      cli_days_left: 23,
      account_uuid: '00000000-0000-4000-8000-000000000001',
      plan: 'Claude Pro',
      billing: '官网订阅',
      plan_fetched_at: '2026-09-12 09:04',
      desktop_profile: true,
    },
    {
      label: 'demo-alt',
      active: false,
      logged_in: true,
      cli_days_left: 61,
      account_uuid: '00000000-0000-4000-8000-000000000002',
      plan: 'Claude Max 5x',
      billing: 'Google Play 订阅',
      plan_fetched_at: '2026-09-10 21:37',
      desktop_profile: false,
    },
    {
      label: 'demo-empty',
      active: false,
      logged_in: false,
      cli_days_left: null,
      account_uuid: null,
      plan: null,
      billing: null,
      plan_fetched_at: null,
      desktop_profile: false,
    },
  ],
  caveat: '用量与额度只能去 Claude 官方 Settings → Usage 看，面板不读取任何限流 / 429 / 额度状态。',
  planCaveat:
    '套餐读自槽位目录里的 .claude.json（官方客户端写下的档案缓存），不发任何网络请求，可能过期。',
  sync: { done: [], failed: [] },
  desktop: { managed: true, active: 'demo-main' },
  bridgePresent: true,
};

const progress: Progress = {
  steps: {
    purity: {
      state: 'passed',
      risk: 'low',
      detail: '已在 IPQS 与 ippure 复核通过',
      updated_at: '2026-09-12 09:06',
    },
    environment: {
      state: 'passed',
      risk: 'low',
      detail: 'Claude Code / 桌面端 / Codex 都已装',
      updated_at: '2026-09-12 09:08',
    },
    dns: {
      state: 'passed',
      risk: 'low',
      detail: '简易通过：10 个探针都由境外解析器回报',
      updated_at: '2026-09-12 09:10',
    },
    iplock: {
      state: 'passed',
      risk: 'low',
      detail: '4 个副本全部上锁，白名单 1 条',
      updated_at: '2026-09-12 09:12',
    },
    accounts: {
      state: 'passed',
      risk: 'low',
      detail: '当前槽位 demo-main，凭证剩 23 天',
      updated_at: '2026-09-12 09:14',
    },
  },
  completed_once: true,
};

const dns: DnsReport = {
  egress_asn: 'AS64501',
  resolvers: [
    {
      address: '203.0.113.53',
      country_code: 'US',
      country_name: 'United States',
      asn: 'AS64501 Example Broadband LLC',
      from_adapter: false,
      interface: null,
      is_private: false,
      is_domestic: false,
    },
    {
      address: '198.51.100.53',
      country_code: 'US',
      country_name: 'United States',
      asn: 'AS64502 Example Anycast DNS',
      from_adapter: false,
      interface: null,
      is_private: false,
      is_domestic: false,
    },
  ],
  passed: true,
  findings: [],
  upstream_conclusion: null,
  note: '10 个探针域名全部由出口同侧的解析器回报，物理网卡的 DNS 没有漏出去。高级通过仍要交给 Codex 复核。',
  score: 100,
  ethernet_safe: true,
};

const settings: Settings = {
  codex_under_gate: false,
  gate_auto_rearm: true,
  managed_apps_dir: null,
};

const managed: ManagedStatus = {
  root: APPS,
  default_root: APPS,
  is_default: true,
  apps: [
    {
      app: 'claude-code',
      path: `${APPS}\\claude-code\\claude.exe`,
      installed: true,
      version: '2.0.14',
      installed_at: '2026-09-11 17:22',
    },
    {
      app: 'codex',
      path: `${APPS}\\codex\\codex.exe`,
      installed: true,
      version: '0.28.0',
      installed_at: '2026-09-11 17:26',
    },
  ],
};

const installProbe: InstallProbe = {
  winget_available: true,
  winget_version: 'v1.9.25200',
  packages: [
    {
      target: 'claude-code',
      id: 'Anthropic.ClaudeCode',
      found: true,
      available_version: '2.0.14',
      installed_version: '2.0.14',
    },
    {
      target: 'claude-desktop',
      id: 'Anthropic.Claude',
      found: true,
      available_version: '0.14.2',
      installed_version: '0.14.2',
    },
    {
      target: 'codex',
      id: 'OpenAI.Codex',
      found: true,
      available_version: '0.28.0',
      installed_version: '0.28.0',
    },
  ],
};

const relayMeta: ProviderMeta = {
  id: 'demo-relay-1',
  target: 'claude-code',
  slug: 'demo-relay',
  name: '演示中转站',
  base_url: 'https://api.example.com/v1',
  model: 'claude-sonnet-4-5',
  wire_api: 'responses',
  auth_style: 'bearer_token',
  note: '示例数据，不是任何真实服务。',
  website: 'https://example.com',
  icon: null,
  sort: 0,
  created_at: '2026-09-11 20:15',
};

const relay: ProviderView[] = [
  { ...relayMeta, has_key: true, key_masked: 'sk-demo…4f2a', key_encrypted: true, active: true },
  {
    ...relayMeta,
    id: 'demo-relay-2',
    target: 'codex',
    slug: 'demo-relay-codex',
    name: '演示中转站（Codex）',
    base_url: 'https://codex.example.com/v1',
    model: 'gpt-5-codex',
    auth_style: 'env_key',
    sort: 1,
    has_key: true,
    key_masked: 'sk-demo…91c7',
    key_encrypted: true,
    active: true,
  },
];

const presets: Preset[] = [
  {
    id: 'demo-preset',
    name: '示例预设',
    target: 'claude-code',
    base_url: 'https://api.example.com/v1',
    wire_api: 'responses',
    auth_style: 'bearer_token',
    model: 'claude-sonnet-4-5',
    website: 'https://example.com',
    note: '示例数据，不是任何真实服务。',
  },
];

const profiles: Profile[] = [
  {
    id: 'demo-profile-1',
    name: '日常（demo-main + 官方直连）',
    note: '账户 demo-main，时区跟着出口走',
    account: 'demo-main',
    relays: {},
    timezone: 'America/New_York',
    sort: 0,
    created_at: '2026-09-11 20:31',
  },
  {
    id: 'demo-profile-2',
    name: '中转（demo-alt + 演示中转站）',
    note: null,
    account: 'demo-alt',
    relays: { 'claude-code': 'demo-relay-1', codex: 'demo-relay-2' },
    timezone: null,
    sort: 1,
    created_at: '2026-09-11 20:33',
  },
];

const profileStore: ProfileStore = { profiles, last_applied: 'demo-profile-1' };

const snapshots: SnapshotEntry[] = [
  {
    id: '20260912-090412',
    path: `${HOME}\\AppData\\Local\\ClaudeIpGate\\snapshots\\20260912-090412`,
    manifest: {
      created: '2026-09-12 09:04:12',
      note: '切到 demo-main 之前',
      active_account: 'demo-main',
      timezone: 'America/New_York',
      all_locked: true,
      files: ['settings.json', 'allowlist.txt', 'relay.json'],
    },
  },
];

const plugins: PluginStatus[] = [
  {
    id: 'sillytavern',
    name: '酒馆 SillyTavern',
    state: 'ready',
    detail: '依赖齐了。点启动会先验出口 IP，再拉起桥接与酒馆。',
    checks: [
      { label: 'SillyTavern 根目录', ok: true, detail: `${HOME}\\SillyTavern` },
      { label: '桥接根目录（含 bridge.py）', ok: true, detail: `${HOME}\\SillyTavern\\bridge` },
      { label: '启动脚本', ok: true, detail: 'start-sillytavern.cmd' },
      { label: '端口 5001 / 8000', ok: true, detail: '都空着' },
    ],
  },
];

const catalog: OfficialCatalogStatus = {
  configured: false,
  source: null,
  signed: false,
  detail: '官方清单仓库尚未创建，商店保持「仅内置插件」模式，不接受任意下载地址。',
};

const tavern: TavernConfig = {
  bridge_root: `${HOME}\\SillyTavern\\bridge`,
  sillytavern_root: `${HOME}\\SillyTavern`,
  st_launcher: `${HOME}\\SillyTavern\\start-sillytavern.cmd`,
  bridge_port: 5001,
  st_port: 8000,
};

const assets: CategoryListing[] = [
  {
    id: 'worlds',
    label: '世界书',
    dir: `${HOME}\\SillyTavern\\data\\default-user\\worlds`,
    exists: true,
    items: [
      {
        name: 'Demo-World.json',
        path: 'worlds\\Demo-World.json',
        size: 48213,
        modified: '2026-09-10 22:14',
        is_dir: false,
      },
    ],
  },
  {
    id: 'characters',
    label: '角色卡',
    dir: `${HOME}\\SillyTavern\\data\\default-user\\characters`,
    exists: true,
    items: [
      {
        name: 'Demo-A.png',
        path: 'characters\\Demo-A.png',
        size: 412336,
        modified: '2026-09-09 19:02',
        is_dir: false,
      },
      {
        name: 'Demo-B.png',
        path: 'characters\\Demo-B.png',
        size: 388104,
        modified: '2026-09-09 19:03',
        is_dir: false,
      },
    ],
  },
];

const backups: BackupEntry[] = [
  {
    id: '20260911-210455',
    path: `${HOME}\\AppData\\Local\\ClaudeIpGate\\tavern-backups\\20260911-210455`,
    created: '2026-09-11 21:04:55',
    size: 1248902,
  },
];

const traces: TraceReport = {
  traces: [
    {
      kind: 'credential',
      label: '槽位凭证',
      path: `${HOME}\\AppData\\Local\\ClaudeIpGate\\claude-profile-demo-main`,
      detail: '面板管理的账户槽位，清理时永远不碰',
    },
    {
      kind: 'install',
      label: '托管安装',
      path: `${APPS}\\claude-code`,
      detail: 'Claude Code 2.0.14',
    },
  ],
  chrome_installed: true,
  chrome_path: 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
  chrome_running: false,
  chrome_scanned: true,
  winget_available: true,
};

const upgrade: UpgradePlan = {
  installed: '2.0.14',
  available: '2.0.14',
  action: 'up_to_date',
  detail: 'latest 渠道上没有更新的版本。',
};

const update: UpdateStatus = {
  current_version: '0.9.0',
  repository: 'smithtaylor7748-ops/qb-gate',
  configured: true,
  update_available: false,
  latest_version: '0.9.0',
  detail: 'QB Gate 不做自动更新，这里只是查一下 Releases。',
};

const kill: KillReport = { targets: [], killed: [], failed: [], relocked: 0, spared: [] };

const cleanup: CleanupReport = { done: [], failed: [], notes: ['演示模式不执行任何实际操作。'] };

const externals: ManagedExternal[] = [];

/**
 * 命令名 → 演示数据。与 `api.ts` 末尾那张表一一对应。
 *
 * 没列进来的命令一律抛错，**不返回空值** —— 「查不到」和「演示里没做」是两回事，
 * 一个假装成功的空结果最难查。
 */
const FIXTURES: Record<string, () => unknown> = {
  gate_status: () => gate,
  allowlist_read: () => gate.allowlist,
  probe_ip: () => ip,
  probe_purity: () => purity,
  probe_dns: () => dns,
  purity_criteria: () => criteria,
  detect_software: () => software,
  install_probe: () => installProbe,
  claude_traces: () => traces,
  accounts_list: () => accounts,
  managed_status: () => managed,
  managed_externals: () => externals,
  managed_cleanup: () => cleanup,
  relay_list: () => relay,
  relay_current: () => [relayMeta],
  relay_presets: () => presets,
  tz_current: () => 'America/New_York',
  snapshot_list: () => snapshots,
  profile_list: () => profileStore,
  settings_load: () => settings,
  progress_load: () => progress,
  upgrade_plan: () => upgrade,
  update_status: () => update,
  killswitch_preview: () => kill,
  plugin_list: () => plugins,
  plugin_catalog_status: () => catalog,
  tavern_config: () => tavern,
  tavern_assets: () => assets,
  tavern_backups: () => backups,
};

/** 演示模式下代替 `invoke`。故意留 120ms，让加载态也长得像真的。 */
export async function demoCall<T>(cmd: string): Promise<T> {
  await new Promise((r) => setTimeout(r, 120));
  const make = FIXTURES[cmd];
  if (!make) throw new Error(`演示模式：「${cmd}」没有演示数据，这个操作只在装好的面板里可用。`);
  return make() as T;
}
