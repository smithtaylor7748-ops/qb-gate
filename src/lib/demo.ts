import { demoWorkspaceCall } from "./workspaceDemo";
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
  AccountProbe,
  EgressChecks,
  TokenSummary,
  AccountsReport,
  BackupEntry,
  BrowserAudit,
  CategoryListing,
  Checkup,
  CleanupReport,
  EnvHit,
  DnsReport,
  FirewallRule,
  GateStatus,
  HookStatus,
  InstallProbe,
  IpInfo,
  KillReport,
  ManagedExternal,
  ManagedStatus,
  NetAdapter,
  OfficialCatalogStatus,
  PanelVerdict,
  PluginStatus,
  Preset,
  Profile,
  ProfileStore,
  Progress,
  ProxyState,
  PurityCriteria,
  Settings,
  SnapshotEntry,
  SoftwareReport,
  TavernConfig,
  TavernSurvey,
  TokenUsage,
  TraceReport,
  UpdateStatus,
  UpgradePlan,
  VersionEntry,
} from "./api";

export const DEMO_ENABLED = import.meta.env.VITE_DEMO === "1";

const ipv6Status = {
  disable: true,
  busy: false,
  bindings: [
    { id: "demo-ethernet", name: "以太网", enabled: false },
    { id: "demo-wifi", name: "WLAN（原本已禁用）", enabled: false },
  ],
  restore_pending: 2,
  error: null,
};

const HOME = "C:\\Users\\demo";
const APPS = `${HOME}\\AppData\\Local\\ClaudeIpGate\\apps`;

const ip: IpInfo = {
  ip: "203.0.113.7",
  asn: 64501,
  asOrganization: "Example Broadband LLC",
  country: "United States",
  countryCode: "US",
  region: "Virginia",
  city: "Ashburn",
  timezone: "America/New_York",
  fraudScore: 0,
  isResidential: true,
  isBroadcast: false,
};

const purity: PanelVerdict = {
  purity: "Pass",
  residential: "Pass",
  native: "Pass",
  passed: true,
  note: "面板自查只作参考，三项硬指标以 IPQualityScore 与 ippure.com 的结论为准。",
};

const criteria: PurityCriteria = {
  ipqs: {
    url: "https://www.ipqualityscore.com/free-ip-lookup-proxy-vpn-test",
    criteria:
      "Fraud Score ≤ 5，且 Proxy / VPN / TOR / Recent Abuse 全为 No，Connection Type = Residential",
  },
  ippure: {
    url: "https://ippure.com/",
    criteria: "IPPure 系数 ≤ 5%，IP来源 = 原生IP，IP属性 = 住宅IP",
  },
  optional: [
    { name: "ipinfo.io", url: "https://ipinfo.io/" },
    { name: "Scamalytics", url: "https://scamalytics.com/ip" },
  ],
  maxFraudScore: 5,
  iproyal: "https://iproyal.cn/?r=sulianyan",
};

const gate: GateStatus = {
  current_ip: "203.0.113.7",
  allowlist: ["203.0.113.7"],
  ip_allowed: true,
  targets: [
    {
      path: `${APPS}\\claude-code\\claude.exe`,
      kind: "Managed",
      exists: true,
      locked: true,
    },
    {
      path: `${HOME}\\.local\\bin\\claude.exe`,
      kind: "Cli",
      exists: true,
      locked: true,
    },
    {
      path: `${HOME}\\AppData\\Roaming\\Claude\\claude-code\\2.0.14\\claude.exe`,
      kind: "CliVersioned",
      exists: true,
      locked: true,
    },
    {
      path: `${HOME}\\AppData\\Local\\AnthropicClaude\\claude.exe`,
      kind: "DesktopStub",
      exists: true,
      locked: true,
    },
  ],
  all_locked: true,
  lease: { holders: {}, holder: null, granted: [], mode: null },
  watchdog_running: false,
  stale_copies: [],
  recent_log: [
    "2026-09-12 09:41:07  出口 IP 203.0.113.7 在白名单内",
    "2026-09-12 09:41:07  4 个副本已上锁（托管 / 官方安装器 / 版本库 / 桌面端存根）",
    "2026-09-12 09:12:33  租约已收回，看门狗停止",
    "2026-09-12 08:55:19  已放行 claude-code，看门狗接管（Cli 档）",
  ],
  needs_reopen: null,
};

const software: SoftwareReport = {
  claudeCode: {
    id: "claude-code",
    name: "Claude Code",
    installed: true,
    version: "2.0.14",
    path: `${APPS}\\claude-code\\claude.exe`,
    advisory: null,
  },
  claudeCodeInstalls: [
    {
      kind: "managed",
      path: `${APPS}\\claude-code\\claude.exe`,
      lockable: true,
      launchable: true,
      preferred: true,
    },
    {
      kind: "native",
      path: `${HOME}\\.local\\bin\\claude.exe`,
      lockable: true,
      launchable: true,
      preferred: false,
    },
    {
      kind: "native_version",
      path: `${HOME}\\.local\\share\\claude\\versions\\2.0.14`,
      lockable: true,
      launchable: false,
      preferred: false,
    },
  ],
  claudeDesktop: {
    id: "claude-desktop",
    name: "Claude 桌面端",
    installed: true,
    version: "0.14.2",
    path: `${HOME}\\AppData\\Local\\AnthropicClaude\\claude.exe`,
    advisory: null,
  },
  codex: {
    id: "codex",
    name: "Codex",
    installed: true,
    version: "0.28.0",
    path: `${APPS}\\codex\\codex.exe`,
    advisory: "默认不在 IP 门禁范围内，可在「设置 → 门禁范围」里打开。",
  },
  browsers: [
    {
      id: "chrome",
      name: "Google Chrome",
      installed: true,
      version: "141.0.7390.55",
      path: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
      advisory: null,
    },
  ],
};

/**
 * 详情页的 token 统计。
 *
 * 数字照实机的形状编：缓存读远大于输入（长会话靠缓存命中），
 * `duplicates` 和留下的条数一个量级 —— 那是续接会话重放出来的，
 * 界面上那句「不去重会接近两倍」不是吓唬人。
 */
const tokenUsage: TokenUsage = {
  buckets: (() => {
    const out: TokenUsage["buckets"] = [];
    for (let i = 0; i < 12; i++) {
      const d = new Date();
      d.setDate(d.getDate() - i);
      const day = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
        d.getDate(),
      ).padStart(2, "0")}`;
      out.push({
        day,
        model: "claude-opus-5",
        input: 300 + i * 17,
        output: 90_000 - i * 3_100,
        cache_write: 420_000 - i * 9_000,
        cache_read: 14_000_000 - i * 310_000,
        messages: 140 - i * 4,
      });
      if (i % 3 === 0) {
        out.push({
          day,
          model: "claude-sonnet-5",
          input: 120,
          output: 6_400,
          cache_write: 31_000,
          cache_read: 880_000,
          messages: 11,
        });
      }
    }
    // 后端给的是按 (day, model) 升序，演示夹具不该比它宽松。
    return out.sort(
      (a, b) => a.day.localeCompare(b.day) || a.model.localeCompare(b.model),
    );
  })(),
  sessions: 37,
  files_read: 41,
  files_failed: 0,
  duplicates: 2_860,
  undated: 0,
  // 默认目录里没有账户标记的那部分。实机上今天的记录全落在这一档，
  // 所以夹具里也必须有 —— 夹具不出现的形态，界面就没人验过。
  unattributed: [
    {
      day: "2026-09-16",
      model: "claude-opus-5",
      input: 1_680,
      output: 635_633,
      cache_write: 1_835_505,
      cache_read: 289_051_108,
      messages: 780,
    },
  ],
};

/**
 * 出口一致性。故意让四种状态都出现一次 —— 截图那一圈要能看出
 * 「查不了」跟「对得上」长得不一样。
 */
const egress: EgressChecks = {
  undoable: [],
  checked_at: "2026-09-16 04:12",
  items: [
    {
      id: "egress_consistency",
      label: "出口一致性",
      state: "pass",
      detail: "绕过代理与跟随代理量到的都是 US，同一个出口。",
      fixable: false,
      manual: null,
    },
    {
      id: "ipv6",
      label: "IPv6",
      state: "warn",
      detail: "IPv6 没有关闭。有 IPv6 的网络里，请求可能从另一条路出去。",
      fixable: false,
      manual:
        "关闭：reg add HKLM\\SYSTEM\\CurrentControlSet\\Services\\Tcpip6\\Parameters /v DisabledComponents /t REG_DWORD /d 255 /f",
    },
    {
      id: "browser_locale",
      label: "浏览器语言",
      state: "warn",
      detail:
        "Chrome 报的语言是 zh-CN、zh、en-US，而出口在 US。网站拿到的 Accept-Language 和 IP 地区对不上。面板只把这件事告诉你，不替你改浏览器或系统的身份。",
      fixable: false,
      manual: null,
    },
    {
      id: "browser_webrtc",
      label: "WebRTC 出口",
      state: "warn",
      detail:
        "没设过 WebRtcIPHandling 策略 —— 用的是 Chrome 自己的默认值，不等于安全。收紧到 disable_non_proxied_udp 会保留通话功能，但不许走非代理的 UDP。",
      fixable: true,
      manual: null,
    },
    {
      id: "browser_doh",
      label: "浏览器 DoH",
      state: "unknown",
      detail:
        "没设过 DnsOverHttpsMode 策略，走的是 Chrome 自己的默认值 —— 那个值随版本和地区变，面板读不到，所以这一项算「查不了」。",
      fixable: true,
      manual: null,
    },
  ],
};

/**
 * 用量小结。按档位从 `tokenUsage` 的桶里现算，跟真后端一个口径 ——
 * 夹具自己另编一套数的话，界面上那几个格子就永远验不出算错了没有。
 */
function summaryFor(days: number): TokenSummary {
  const today = new Date();
  const key = (d: Date) =>
    `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
      d.getDate(),
    ).padStart(2, "0")}`;
  let since = "";
  if (days > 0) {
    const d = new Date(today);
    d.setDate(d.getDate() - (days - 1));
    since = key(d);
  }
  const picked = tokenUsage.buckets.filter((b) => !since || b.day >= since);
  const unattr = tokenUsage.unattributed.filter(
    (b) => !since || b.day >= since,
  );
  const sum = (f: (b: TokenUsage["buckets"][number]) => number) =>
    picked.reduce((a, b) => a + f(b), 0);
  const input = sum((b) => b.input);
  const cacheRead = sum((b) => b.cache_read);
  const cacheWrite = sum((b) => b.cache_write);
  const readTotal = input + cacheRead + cacheWrite;
  return {
    days,
    input,
    output: sum((b) => b.output),
    cache_write: cacheWrite,
    cache_read: cacheRead,
    messages: sum((b) => b.messages),
    hit_rate: readTotal > 0 ? cacheRead / readTotal : null,
    unattributed: unattr.reduce(
      (a, b) => a + b.input + b.output + b.cache_write + b.cache_read,
      0,
    ),
    unattributed_messages: unattr.reduce((a, b) => a + b.messages, 0),
    // 演示口径：Opus 5 的输入 $5、缓存读 $0.5，差 $4.5 / 百万。
    saved_usd: cacheRead > 0 ? (cacheRead / 1_000_000) * 4.5 : null,
    unpriced_models: 0,
    priced_from_snapshot: cacheRead > 0 ? true : null,
  };
}

const accountProbe: AccountProbe = {
  state: "locally_expired",
  detail:
    "本地这份访问令牌 61 小时前就过期了，没有发请求 —— 拿过期令牌去问，被拒是必然的，报给你只会是假警报。访问令牌 8–12 小时一换，是正常轮换：用这个账户跑一次 Claude Code，它会自己换新，再回来测。",
  checked_at: "2026-09-16 06:12",
};

const accounts: AccountsReport = {
  slots: [
    {
      label: "demo-main",
      active: true,
      logged_in: true,
      cli_days_left: 23,
      account_uuid: "00000000-0000-4000-8000-000000000001",
      email: "demo@example.com",
      org_name: "Demo 的个人空间",
      expires_at: "2026-10-09 15:35",
      dir: "C:\\Users\\demo\\AppData\\Local\\ClaudeIpGate\\claude-profile-demo-main",
      desktop_dir: "C:\\Users\\demo\\AppData\\Roaming\\Claude-demo-main",
      plan: "Claude Pro",
      billing: "官网订阅",
      plan_fetched_at: "2026-09-12 09:04",
      desktop_profile: true,
      // 当前登录的那个：桌面端有实时样本，五小时窗口的恢复时刻是推算的。
      usage: {
        source: "desktop",
        measured_at: new Date(Date.now() - 6 * 60_000).toISOString(),
        age_minutes: 6,
        five_hour: {
          used: 62,
          resets_at: new Date(Date.now() + 97 * 60_000).toISOString(),
          estimated: true,
        },
        seven_day: {
          used: 79,
          resets_at: new Date(Date.now() + 3 * 86_400_000).toISOString(),
          estimated: false,
        },
      },
    },
    {
      label: "demo-alt",
      active: false,
      logged_in: true,
      cli_days_left: 61,
      account_uuid: "00000000-0000-4000-8000-000000000002",
      email: "demo-alt@example.com",
      org_name: "Demo Alt 的个人空间",
      expires_at: "2026-11-16 08:10",
      dir: "C:\\Users\\demo\\AppData\\Local\\ClaudeIpGate\\claude-profile-demo-alt",
      desktop_dir: "C:\\Users\\demo\\AppData\\Roaming\\Claude-demo-alt",
      plan: "Claude Max 5x",
      billing: "Google Play 订阅",
      plan_fetched_at: "2026-09-10 21:37",
      desktop_profile: false,
      // 没登录的槽位不可能有实时数据 —— 物理限制。只有 Claude Code 留下的快照，
      // 而且是十天前的：`resets_at` 早过去了，所以两个窗口都没有恢复时刻。
      usage: {
        source: "cache",
        measured_at: "2026-09-02T18:07:05.310Z",
        age_minutes: 14_400,
        five_hour: { used: 25, resets_at: null, estimated: false },
        seven_day: { used: 13, resets_at: null, estimated: false },
      },
    },
    {
      label: "demo-empty",
      active: false,
      logged_in: false,
      cli_days_left: null,
      account_uuid: null,
      email: null,
      org_name: null,
      expires_at: null,
      dir: "C:\\Users\\demo\\AppData\\Local\\ClaudeIpGate\\claude-profile-demo-empty",
      desktop_dir: "C:\\Users\\demo\\AppData\\Roaming\\Claude-demo-empty",
      plan: null,
      billing: null,
      plan_fetched_at: null,
      desktop_profile: false,
      // 两源都没有 —— 什么都不显示，不给假读数。
      usage: null,
    },
  ],
  caveat:
    "剩余天数只读本地时间戳，查不出「被风控下线」。唯一能确认的办法是实际发一次认证请求。",
  planCaveat:
    "套餐读自槽位目录里的 .claude.json（官方客户端写下的档案缓存），不发任何网络请求，可能过期。",
  sync: { done: [], failed: [] },
  desktop: { managed: true, active: "demo-main" },
  bridgePresent: true,
};

// Six fictional slots exercise four-per-page navigation in the demo.
for (let i = 3; i < 6; i++)
  accounts.slots.push({
    ...structuredClone(accounts.slots[0]),
    label: `demo-work-${i}`,
    active: false,
    email: `work${i}@example.test`,
  });

const progress: Progress = {
  steps: {
    purity: {
      state: "passed",
      risk: "low",
      detail: "已在 IPQS 与 ippure 复核通过",
      updated_at: "2026-09-12 09:06",
    },
    environment: {
      state: "passed",
      risk: "low",
      detail: "Claude Code / 桌面端 / Codex 都已装",
      updated_at: "2026-09-12 09:08",
    },
    dns: {
      state: "passed",
      risk: "low",
      detail: "简易通过：10 个探针都由境外解析器回报",
      updated_at: "2026-09-12 09:10",
    },
    iplock: {
      state: "passed",
      risk: "low",
      detail: "4 个副本全部上锁，白名单 1 条",
      updated_at: "2026-09-12 09:12",
    },
    accounts: {
      state: "passed",
      risk: "low",
      detail: "当前槽位 demo-main，凭证剩 23 天",
      updated_at: "2026-09-12 09:14",
    },
  },
  completed_once: true,
};

const dns: DnsReport = {
  egress_asn: "AS64501",
  resolvers: [
    {
      address: "203.0.113.53",
      country_code: "US",
      country_name: "United States",
      asn: "AS64501 Example Broadband LLC",
      from_adapter: false,
      interface: null,
      is_private: false,
      is_domestic: false,
    },
    {
      address: "198.51.100.53",
      country_code: "US",
      country_name: "United States",
      asn: "AS64502 Example Anycast DNS",
      from_adapter: false,
      interface: null,
      is_private: false,
      is_domestic: false,
    },
  ],
  passed: true,
  findings: [],
  upstream_conclusion: null,
  note: "10 个探针域名全部由出口同侧的解析器回报，物理网卡的 DNS 没有漏出去。高级通过仍要交给 Codex 复核。",
  score: 100,
  ethernet_safe: true,
};

const hook: HookStatus = {
  installed: true,
  slot: "demo-main",
  settings_path: `${HOME}\\AppData\\Local\\ClaudeIpGate\\claude-profile-demo-main\\settings.json`,
  script_path: `${HOME}\\AppData\\Local\\ClaudeIpGate\\hooks\\gate-check.ps1`,
  recent_blocks: [
    "2026-09-11 22:14:03  出口 IP 198.51.100.22 不在白名单内",
    "2026-09-11 22:13:58  门禁裁决已过期 142 秒（面板没在跑，或看门狗已停）",
  ],
};

const demoEnv: EnvHit[] = [
  {
    name: "ANTHROPIC_BASE_URL",
    scope: "用户环境变量",
    // 注意这里只剩 host —— 真实场景里这个变量的路径段可能带 token，
    // 所以 Rust 侧 mask_env_value 只留 origin。演示数据要跟真实输出长得一样。
    shown: "https://relay.example.com",
  },
  {
    name: "ANTHROPIC_API_KEY",
    scope: "用户环境变量",
    shown: "（已设置，值不显示）",
  },
  { name: "HTTPS_PROXY", scope: "当前进程", shown: "http://127.0.0.1:7890" },
];

const checkup: Checkup = {
  items: [
    {
      id: "proxy",
      label: "系统代理",
      state: "pass",
      detail: "系统代理没开 —— 出口由路由/TUN 决定，跟面板量到的是同一条路",
      fixable: false,
      manual: null,
    },
    {
      id: "egress_consistency",
      label: "出口一致性",
      state: "fail",
      detail:
        "两条路出去的国家不一样：绕过代理是 US，跟随系统代理是 HK。有程序会从另一个国家出去。",
      fixable: false,
      manual: "常见原因是系统代理或某个环境变量里的代理只接管了一部分流量。",
    },
    {
      id: "ipv6",
      label: "IPv6",
      state: "warn",
      detail:
        "IPv6 开着。隧道只接管 IPv4 时，v6 流量会绕过它直接从本地出去 —— 这是最常见的一种「代理开着但还是暴露了」。",
      fixable: false,
      manual:
        "管理员身份运行，然后重启：" +
        "\nreg add HKLM\\SYSTEM\\CurrentControlSet\\Services\\Tcpip6\\Parameters /v DisabledComponents /t REG_DWORD /d 0xff /f",
    },
    {
      id: "doh",
      label: "浏览器 DoH 策略",
      state: "unknown",
      detail:
        "没有配置 DoH 策略 —— 浏览器用的是它自己的默认值，可能在用内置的 DoH 解析器。",
      fixable: false,
      manual: "判断不了就交给「DNS 泄露 → 高级通过」那条让 Codex 看一眼。",
    },
    {
      id: "env_residue",
      label: "环境变量残留",
      state: "warn",
      detail:
        "找到 3 个相关的环境变量。设了 ANTHROPIC_BASE_URL —— Claude Code 会走它指的地方，而不是中转站页上显示的那条。两处说的不是一件事。设了代理变量 —— 面板测出口时是绕过系统代理的，所以面板量到的出口和请求实际走的路可能不一样。",
      fixable: false,
      manual:
        "改用户环境变量：设置 → 系统 → 系统信息 → 高级系统设置 → 环境变量。面板不替你删。",
    },
    {
      id: "secrets",
      label: "MCP / 配置里的明文密钥",
      state: "fail",
      detail:
        "在 2 处看到疑似明文密钥字段。面板只报位置不报内容，自己去看一眼。",
      fixable: false,
      manual: "这些文件会被同步盘、备份、以及你随手贴出来的截图带走。",
    },
  ],
  secrets: [
    { file: `${HOME}\\.mcp.json`, field: "api_key" },
    { file: `${HOME}\\.codex\\config.toml`, field: "token" },
  ],
  env: demoEnv,
};

const settings: Settings = {
  codex_under_gate: false,
  gate_auto_rearm: true,
  managed_apps_dir: null,
  country_allowlist: ["US"],
  hook_enabled: true,
  disable_telemetry: false,
  // 这三个照搬 Rust 那边的默认值：显示语言是关的，另外两个是开的。
  // 演示态没必要在这里自作主张 —— 它要展示的正是「三个开关里有一个
  // 故意默认关着」，改成三个全开就把那件事藏掉了。
  align_timezone_on_start: true,
  align_locale_on_start: true,
  align_display_language_on_start: false,
};

const managed: ManagedStatus = {
  root: APPS,
  default_root: APPS,
  is_default: true,
  apps: [
    {
      app: "claude-code",
      path: `${APPS}\\claude-code\\claude.exe`,
      installed: true,
      version: "2.0.14",
      installed_at: "2026-09-11 17:22",
    },
    {
      app: "codex",
      path: `${APPS}\\codex\\codex.exe`,
      installed: true,
      version: "0.28.0",
      installed_at: "2026-09-11 17:26",
    },
  ],
};

const versionHistory: VersionEntry[] = [
  {
    version: "2.0.14",
    path: `${APPS}\\claude-code\\versions\\2.0.14\\claude.exe`,
    sha256: "a1b2c3d4e5f6",
    archived_at: "2026-09-11 17:22",
    is_current: true,
    locked: true,
  },
  {
    version: "2.0.11",
    path: `${APPS}\\claude-code\\versions\\2.0.11\\claude.exe`,
    sha256: "f6e5d4c3b2a1",
    archived_at: "2026-09-02 10:05",
    is_current: false,
    locked: true,
  },
  {
    version: "2.0.9",
    path: `${APPS}\\claude-code\\versions\\2.0.9\\claude.exe`,
    sha256: "0f1e2d3c4b5a",
    archived_at: "2026-08-24 08:41",
    is_current: false,
    locked: true,
  },
];

const installProbe: InstallProbe = {
  winget_available: true,
  winget_version: "v1.9.25200",
  packages: [
    {
      target: "claude-code",
      id: "Anthropic.ClaudeCode",
      found: true,
      available_version: "2.0.14",
      installed_version: "2.0.14",
    },
    {
      target: "claude-desktop",
      id: "Anthropic.Claude",
      found: true,
      available_version: "0.14.2",
      installed_version: "0.14.2",
    },
    {
      target: "codex",
      id: "OpenAI.Codex",
      found: true,
      available_version: "0.28.0",
      installed_version: "0.28.0",
    },
  ],
};

const presets: Preset[] = [
  {
    id: "demo-preset",
    name: "示例预设",
    target: "claude-code",
    base_url: "https://api.example.com/v1",
    wire_api: "responses",
    auth_style: "bearer_token",
    model: "claude-sonnet-4-5",
    website: "https://example.com",
    note: "示例数据，不是任何真实服务。",
  },
  {
    id: "demo-preset-codex",
    name: "示例预设（Codex）",
    target: "codex",
    base_url: "https://codex.example.com/v1",
    wire_api: "chat",
    auth_style: "env_key",
    model: "gpt-5-codex",
    website: "https://example.com",
    note: "示例数据，不是任何真实服务。",
  },
];

const profiles: Profile[] = [
  {
    id: "demo-profile-1",
    name: "日常（demo-main + 官方直连）",
    note: "账户 demo-main，时区跟着出口走",
    account: "demo-main",
    relays: {},
    timezone: "America/New_York",
    sort: 0,
    created_at: "2026-09-11 20:31",
  },
  {
    id: "demo-profile-2",
    name: "中转（demo-alt + 演示中转站）",
    note: null,
    account: "demo-alt",
    relays: { "claude-code": "demo-relay-1", codex: "demo-relay-2" },
    timezone: null,
    sort: 1,
    created_at: "2026-09-11 20:33",
  },
];

const profileStore: ProfileStore = { profiles, last_applied: "demo-profile-1" };

const snapshots: SnapshotEntry[] = [
  {
    id: "20260912-090412",
    path: `${HOME}\\AppData\\Local\\ClaudeIpGate\\snapshots\\20260912-090412`,
    manifest: {
      schema: 1,
      environments: [],
      created: "2026-09-12 09:04:12",
      note: "切到 demo-main 之前",
      active_account: "demo-main",
      timezone: "America/New_York",
      all_locked: true,
      files: ["settings.json", "allowlist.txt", "relay.json"],
      absent: [],
      hashes: {},
    },
  },
];

const plugins: PluginStatus[] = [
  {
    id: "sillytavern",
    name: "酒馆 SillyTavern",
    state: "ready",
    detail: "依赖齐了。点启动会先验出口 IP，再拉起桥接与酒馆。",
    checks: [
      { label: "SillyTavern 根目录", ok: true, detail: `${HOME}\\SillyTavern` },
      {
        label: "桥接根目录（含 bridge.py）",
        ok: true,
        detail: `${HOME}\\SillyTavern\\bridge`,
      },
      { label: "启动脚本", ok: true, detail: "start-sillytavern.cmd" },
      { label: "端口 5001 / 8000", ok: true, detail: "都空着" },
    ],
  },
];

const catalog: OfficialCatalogStatus = {
  configured: false,
  source: null,
  signed: false,
  detail:
    "官方清单仓库尚未创建，商店保持「仅内置插件」模式，不接受任意下载地址。",
};

const tavern: TavernConfig = {
  bridge_root: `${HOME}\\SillyTavern\\bridge`,
  sillytavern_root: `${HOME}\\SillyTavern`,
  st_launcher: `${HOME}\\SillyTavern\\start-sillytavern.cmd`,
  bridge_port: 5001,
  st_port: 8000,
};

/** 定位结果。演示里给一条「正在跑的」加两条扫出来的，其中一条是备份 —— 排序看得出来。 */
const survey: TavernSurvey = {
  bridge: [
    {
      path: `${HOME}\\SillyTavern\\bridge`,
      evidence: "running",
      note: "正在运行",
      score: 994,
    },
    {
      path: `${HOME}\\Downloads\\bridge.backup-20260605`,
      evidence: "scan",
      note: "路径里有备份/旧版字样",
      score: -408,
    },
  ],
  sillytavern: [
    {
      path: `${HOME}\\SillyTavern`,
      evidence: "scan",
      note: "30 天内改过",
      score: 152,
    },
  ],
  launcher: [
    {
      path: `${HOME}\\SillyTavern\\start-sillytavern.cmd`,
      evidence: "scan",
      note: "扫描命中",
      score: 148,
    },
  ],
  scanned_dirs: 3184,
  truncated: false,
};

const assets: CategoryListing[] = [
  {
    id: "worlds",
    label: "世界书",
    dir: `${HOME}\\SillyTavern\\data\\default-user\\worlds`,
    exists: true,
    items: [
      {
        name: "Demo-World.json",
        path: "worlds\\Demo-World.json",
        size: 48213,
        modified: "2026-09-10 22:14",
        is_dir: false,
      },
    ],
  },
  {
    id: "characters",
    label: "角色卡",
    dir: `${HOME}\\SillyTavern\\data\\default-user\\characters`,
    exists: true,
    items: [
      {
        name: "Demo-A.png",
        path: "characters\\Demo-A.png",
        size: 412336,
        modified: "2026-09-09 19:02",
        is_dir: false,
      },
      {
        name: "Demo-B.png",
        path: "characters\\Demo-B.png",
        size: 388104,
        modified: "2026-09-09 19:03",
        is_dir: false,
      },
    ],
  },
];

const backups: BackupEntry[] = [
  {
    id: "20260911-210455",
    path: `${HOME}\\AppData\\Local\\ClaudeIpGate\\tavern-backups\\20260911-210455`,
    created: "2026-09-11 21:04:55",
    size: 1248902,
  },
];

const traces: TraceReport = {
  traces: [
    {
      kind: "credential",
      label: "槽位凭证",
      path: `${HOME}\\AppData\\Local\\ClaudeIpGate\\claude-profile-demo-main`,
      detail: "面板管理的账户槽位，清理时永远不碰",
    },
    {
      kind: "install",
      label: "托管安装",
      path: `${APPS}\\claude-code`,
      detail: "Claude Code 2.0.14",
    },
  ],
  chrome_installed: true,
  chrome_path: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  // Chrome 开着也照扫（0.19.x）。这里特意留 3 个没读开 —— 界面要因此
  // 多出一句「结论只覆盖读开的那部分」，而那是这一栏里最长的一段字，
  // `test:ui` 的溢出探针正好拿它当压力。
  chrome_running: true,
  chrome_scanned: true,
  chrome_files_read: 41,
  chrome_files_locked: 3,
  winget_available: true,
};

/**
 * Chrome 隐私审计。
 *
 * ⛔ **值要往长了写，不许图省事填「已开」「没有」。**
 *
 * 这一条是补窟窿：0.19.0 之前演示数据里根本没有 `browser_audit`，于是
 * `test:ui` 那 72 轮从来没渲染过隐私审计那六行 —— 而它们当时正整整往右
 * 吐出 57px，压在隔壁「卸载」栏的正文上（原因见 `Environment.tsx` 里
 * 「这里不许用 grid」那一段）。溢出探针查得出这种事，只是它没东西可查。
 *
 * 所以 `webrtc` / `doh` 用的是真实长度的策略值加来源后缀 —— 那是这一栏
 * 里最长的一种内容，而栏宽只有三分之一，装不下就该当场红。
 */
const browserAudit: BrowserAudit = {
  chrome_installed: true,
  chrome_path: traces.chrome_path,
  webrtc: { value: "disable_non_proxied_udp", scope: "current_user" },
  doh: { value: "automatic", scope: "machine" },
  extensions: [
    {
      id: "cjpalhdlnbpafiamejdnhcphjbkeiagm",
      name: "uBlock Origin",
      version: "1.60.0",
      profile: "Default",
      risky: ["读取和更改所有网站的数据", "拦截或修改网络请求（webRequest）"],
    },
    {
      // 清单名是 `__MSG_appName__` 那种，查不到真名就留空 —— 不编。
      id: "gighmmpiobklfepjocnamgkkbiglidom",
      name: null,
      version: "5.3.2",
      profile: "Profile 1",
      risky: ["读取和更改所有网站的数据", "读取 Cookie", "向页面注入脚本"],
    },
    {
      id: "nkbihfbeogaeaoehlefnkodbefgpgknn",
      name: "MetaMask",
      version: "12.0.4",
      profile: "Profile 1",
      risky: [],
    },
  ],
  unchecked: [
    "只列出「装着的」扩展 —— 是否已启用要解析 Chrome 的内部状态文件，面板不去猜，请在 chrome://extensions 里自行核对。",
  ],
};

/** 系统代理。开着且带端口，是这一栏里最长的那种值。 */
const proxy: ProxyState = {
  enabled: true,
  server: "127.0.0.1:7890",
  bypass: "localhost;127.*;10.*;192.168.*",
  pac: null,
};

/** 面板加过的出站锁。 */
const firewallRules: FirewallRule[] = [
  {
    name: "QB Gate · 拦 chrome.exe 出站（以太网）",
    program: traces.chrome_path,
    interface: "以太网",
    enabled: true,
  },
];

const adapters: NetAdapter[] = [
  {
    name: "以太网",
    description: "Realtek Gaming 2.5GbE Family Controller",
    up: true,
  },
  { name: "WLAN", description: "Intel(R) Wi-Fi 6E AX211 160MHz", up: true },
  {
    name: "vEthernet (WSL)",
    description: "Hyper-V Virtual Ethernet Adapter",
    up: false,
  },
];

const upgrade: UpgradePlan = {
  installed: "2.0.14",
  available: "2.0.14",
  action: "up_to_date",
  detail: "latest 渠道上没有更新的版本。",
};

const update: UpdateStatus = {
  current_version: "0.9.0",
  repository: "smithtaylor7748-ops/qb-gate",
  configured: true,
  update_available: false,
  latest_version: "0.9.0",
  detail: "QB Gate 不做自动更新，这里只是查一下 Releases。",
};

const kill: KillReport = {
  targets: [],
  killed: [],
  failed: [],
  relocked: 0,
  spared: [],
};

const cleanup: CleanupReport = {
  done: [],
  failed: [],
  notes: ["演示模式不执行任何实际操作。"],
};

const externals: ManagedExternal[] = [];

/**
 * 命令名 → 演示数据。与 `api.ts` 末尾那张表一一对应。
 *
 * 没列进来的命令一律抛错，**不返回空值** —— 「查不到」和「演示里没做」是两回事，
 * 一个假装成功的空结果最难查。
 */

// ------------------------------------------------------------------ 中转站
//
// 线路 id 的格式是 `软件 + U+001F + 站点 id + U+001F + 分组名`（Route::make_id）。
const US = String.fromCharCode(31); // 单元分隔符，同 Route::make_id
const rid = (client: string, station: string, group: string) =>
  [client, station, group].join(US);

const R_MAIN = rid("claude-code", "example", "DS 专线");
const R_CLAUDE = rid("claude-code", "example", "Claude 高倍组");
const R_CHEAP = rid("claude-code", "moxi", "DS 低价组");
// 桌面端跟 Claude Code **共用 example 这个站**，但那是另一条线、另一把 key。
// ⛔ 三个软件底下都要有线路，而且站点要有重叠 —— 只在 Claude Code 底下铺夹具的话，
// 「线路按软件独立、站点按域名共享」恰好是看不出来的那一条：切到 Codex 看见空列表，
// 分不清是「按软件分对了」还是「夹具漏了」。
const R_DESK = rid("claude-desktop", "example", "桌面端专用组");
const R_DESK2 = rid("claude-desktop", "moxi", "DS 低价组");
const R_CODEX = rid("codex", "moxi", "GPT 标准组");

const stationRoutes = [
  {
    id: R_MAIN,
    client: "claude-code",
    station_id: "example",
    group: "DS 专线",
    credential_id: "cred-1",
    nominal_rate: 0.2,
    latest_mult: 1.0, // 检验过、属实
    last_audit_ms: 1757820000000n,
    protocols: { anthropic: true, openai_chat: null, openai_responses: null },
    // 开了计费翻倍：输出按输入的 5 倍计费
    rates: {
      model_ratio: 0.2,
      completion_ratio: 5,
      cache_ratio: 0.1,
      create_cache_ratio: 1.25,
      group_ratio: null,
      peak_rate: null,
      per_request_price: null,
      input_price: null,
      cache_read_price: null,
      cache_write_price: null,
      output_price: null,
    },
    rates_model: "claude-opus-5",
  },
  {
    id: R_CLAUDE,
    client: "claude-code",
    station_id: "example",
    group: "Claude 高倍组",
    credential_id: "cred-2",
    nominal_rate: 2.5,
    latest_mult: null, // 从没检验过 —— 界面写「标称 · 未核实」
    last_audit_ms: null,
    protocols: { anthropic: true, openai_chat: null, openai_responses: null },
    // 不翻倍
    rates: {
      model_ratio: 2.5,
      completion_ratio: 1,
      cache_ratio: 0.1,
      create_cache_ratio: 1.25,
      group_ratio: null,
      peak_rate: null,
      per_request_price: null,
      input_price: null,
      cache_read_price: null,
      cache_write_price: null,
      output_price: null,
    },
    // 从没检验过 —— 这组价是手上一次留下的，模型名也就无从谈起。
    rates_model: null,
  },
  {
    id: R_CHEAP,
    client: "claude-code",
    station_id: "moxi",
    group: "DS 低价组",
    credential_id: "cred-3",
    nominal_rate: 0.12,
    latest_mult: 1.3, // 实扣是标称的 1.3 倍 —— 划掉标称写实测
    last_audit_ms: 1757900000000n,
    protocols: { anthropic: true, openai_chat: null, openai_responses: null },
    // ⛔ moxi 是 **sub2api 系**：它公布的是绝对单价（美元／百万 token），
    // 折扣全在分组倍率上。example 那家是 New API 系，公布的是相对倍率。
    // **两家都要有夹具** —— 只铺一种的话，另一套口径在界面上永远看不见，
    // 而它恰好是「站点单价就是官方单价」那条使用者亲自指出来的分支。
    rates: {
      model_ratio: null,
      completion_ratio: null,
      cache_ratio: null,
      create_cache_ratio: null,
      group_ratio: 0.12,
      peak_rate: null,
      per_request_price: null,
      // 官方 claude-opus-5 是 5 / 25；分组倍率 ×0.12 之后是 0.6 / 3。
      input_price: 5,
      cache_read_price: 0.5,
      cache_write_price: 6.25,
      output_price: 25,
    },
    rates_model: "claude-opus-5",
  },
  {
    // 桌面端：跟 Claude Code 共用 example 这个站，但**是另一条线**（另一把 key、
    // 另一份健康度）。站点那一层不按软件复制，否则同一份余额会在两张卡上各写一遍。
    id: R_DESK,
    client: "claude-desktop",
    station_id: "example",
    group: "桌面端专用组",
    credential_id: "cred-2",
    nominal_rate: 0.35,
    latest_mult: 1.0,
    last_audit_ms: 1757860000000n,
    protocols: { anthropic: true, openai_chat: null, openai_responses: null },
    rates: {
      model_ratio: 0.35,
      completion_ratio: 1,
      cache_ratio: 0.1,
      create_cache_ratio: 1.25,
      group_ratio: null,
      peak_rate: null,
      per_request_price: null,
      input_price: null,
      cache_read_price: null,
      cache_write_price: null,
      output_price: null,
    },
    rates_model: "claude-opus-5",
  },
  {
    // 同一个站点的同一个分组名，在桌面端底下是**另一行** —— id 里带软件那一段。
    id: R_DESK2,
    client: "claude-desktop",
    station_id: "moxi",
    group: "DS 低价组",
    credential_id: null,
    nominal_rate: 0.12,
    latest_mult: null,
    last_audit_ms: null,
    protocols: { anthropic: true, openai_chat: null, openai_responses: null },
    // 同一家站点，所以也是绝对单价那一套。**没检验过，所以不知道是哪个模型的** ——
    // 这一条在界面上要显示成「算不出倍率」，不是「免费」。
    rates: {
      model_ratio: null,
      completion_ratio: null,
      cache_ratio: null,
      create_cache_ratio: null,
      group_ratio: 0.12,
      peak_rate: null,
      per_request_price: null,
      input_price: 5,
      cache_read_price: 0.5,
      cache_write_price: 6.25,
      output_price: 25,
    },
    rates_model: null,
  },
  {
    // Codex 走 OpenAI 协议。anthropic 那一项是 null（没探过），⛔ 不是 false。
    id: R_CODEX,
    client: "codex",
    station_id: "moxi",
    group: "GPT 标准组",
    credential_id: "cred-3",
    nominal_rate: 0.8,
    latest_mult: 1.45,
    last_audit_ms: 1757890000000n,
    protocols: { anthropic: null, openai_chat: true, openai_responses: true },
    // GPT 那一档的官方价是 1.25 / 10；这个分组不打折（×0.8 是它自己的分组倍率）。
    rates: {
      model_ratio: null,
      completion_ratio: null,
      cache_ratio: null,
      create_cache_ratio: null,
      group_ratio: 0.8,
      peak_rate: null,
      per_request_price: null,
      input_price: 1.25,
      cache_read_price: 0.125,
      cache_write_price: 1.5625,
      output_price: 10,
    },
    rates_model: "gpt-5",
  },
];

/** 演示里的路由状态。按软件各记一份，跟真后端的 `ClientRouter` 对齐。 */
let stationRouterUp = true;
let stationSwitches = 3;
const stationCurrent: Record<string, string> = {
  // 只有 Claude Code 选了上游 —— 另外两个显示「还没选上游」，
  // 那是刚装完面板的真实样子，也让「三个软件各走各的」一眼看得出来。
  "claude-code": R_MAIN,
};
/**
 * 排队中的那条，也按软件各记一份 —— 跟真后端的 `request_switch` 一样。
 *
 * ⛔ 点行只排队，下一发请求才落定。演示以前点行直接改 current，于是界面
 * 「只认 current_route」的毛病在截图里永远看不出来：真机上面板刚启动时
 * 点哪一行都不高亮，右列「启动」一直灰着。
 */
const stationPending: Record<string, string> = {};

const stationHealth = [
  {
    route_id: R_MAIN,
    requests: 412,
    success_rate: 0.994,
    cache_hit_rate: 0.71,
    ttft_p95_ms: 1180,
    cost_24h: 2.11,
    tokens_24h: 4_820_000,
    input_tokens: 1_180_000,
    cache_read_tokens: 3_240_000,
    cache_write_tokens: 220_000,
    output_tokens: 180_000,
    error: null,
  },
  {
    route_id: R_CLAUDE,
    requests: 96,
    success_rate: 0.989,
    cache_hit_rate: 0.64,
    ttft_p95_ms: 2020,
    cost_24h: 1.44,
    tokens_24h: 610_000,
    input_tokens: 190_000,
    cache_read_tokens: 360_000,
    cache_write_tokens: 34_000,
    output_tokens: 26_000,
    error: null,
  },
  {
    // 拉不到账单：上面那些是「没有数据」，不是「零请求」。
    route_id: R_CHEAP,
    requests: 0,
    success_rate: null,
    cache_hit_rate: null,
    ttft_p95_ms: null,
    cost_24h: null,
    tokens_24h: null,
    input_tokens: null,
    cache_read_tokens: null,
    cache_write_tokens: null,
    output_tokens: null,
    error: "拉不到账单：连接超时",
  },
  {
    route_id: R_DESK,
    requests: 58,
    success_rate: 0.982,
    cache_hit_rate: 0.44,
    ttft_p95_ms: 1620,
    cost_24h: 0.83,
    tokens_24h: 288_000,
    input_tokens: 96_000,
    cache_read_tokens: 150_000,
    cache_write_tokens: 21_000,
    output_tokens: 21_000,
    error: null,
  },
  {
    // 从没跑过：**零请求**，跟上面 R_CHEAP 的「拉不到账单」是两回事。
    route_id: R_DESK2,
    requests: 0,
    success_rate: null,
    cache_hit_rate: null,
    ttft_p95_ms: null,
    cost_24h: null,
    tokens_24h: null,
    input_tokens: null,
    cache_read_tokens: null,
    cache_write_tokens: null,
    output_tokens: null,
    error: null,
  },
  {
    route_id: R_CODEX,
    requests: 173,
    success_rate: 0.961,
    cache_hit_rate: 0.29,
    ttft_p95_ms: 2440,
    cost_24h: 1.92,
    tokens_24h: 1_640_000,
    input_tokens: null,
    cache_read_tokens: null,
    cache_write_tokens: null,
    output_tokens: null,
    error: null,
  },
];

const stationDecision = {
  basis: "real-rate",
  ranking: {
    rows: [
      {
        route_id: R_MAIN,
        score: 0.78,
        per_axis: [
          ["cheap", { kind: "ratio", value: 0.78 }],
          ["fast", { kind: "ratio", value: 1.0 }],
          ["stable", { kind: "ratio", value: 1.0 }],
        ],
        weakest: "cheap",
        missing: [],
        failed_floors: [],
        eligible: true,
        tripped: false,
      },
      {
        route_id: R_CLAUDE,
        score: 0.58,
        per_axis: [
          ["cheap", { kind: "ratio", value: 0.06 }],
          ["fast", { kind: "ratio", value: 0.58 }],
          ["stable", { kind: "ratio", value: 0.99 }],
        ],
        weakest: "cheap",
        missing: [],
        failed_floors: [],
        eligible: true,
        tripped: false,
      },
      {
        // 账单拉不到 → 「快」「稳」没有证据 → 不给总分，**不是 0 分**
        route_id: R_CHEAP,
        score: null,
        per_axis: [
          ["cheap", { kind: "ratio", value: 1.0 }],
          ["fast", { kind: "missing" }],
          ["stable", { kind: "missing" }],
        ],
        weakest: "cheap",
        missing: ["fast", "stable"],
        failed_floors: [],
        eligible: true,
        tripped: false,
      },
    ],
    winner: R_MAIN,
    switched: false,
    held_by_hysteresis: true,
    indistinguishable: [],
    floors_relaxed: false,
    breakers_relaxed: false,
  },
};

/**
 * 请求日志。**按 `Date.now()` 往回铺 7 天，不写死时间戳。**
 *
 * ⛔ 写死绝对时间的话，「近 24 小时」筛完永远是空的：曲线不画、四个总量全是 0，
 * 而那看起来像「功能没做」而不是「夹具过期了」。
 *
 * 随机数带种子 —— 形状每次一样，截图才比得了。
 */
const ANCHOR = Date.now();
const seeded = (seed: number) => {
  let x = seed >>> 0;
  return () => {
    x = (x * 1664525 + 1013904223) >>> 0;
    return x / 4294967296;
  };
};

const LOG_SHAPES: {
  route: string;
  client: string;
  perDay: number;
  models: string[];
  baseTtft: number;
  succ: number;
}[] = [
  {
    route: R_MAIN,
    client: "claude-code",
    perDay: 58,
    models: ["deepseek-chat", "deepseek-reasoner"],
    baseTtft: 620,
    succ: 0.994,
  },
  {
    route: R_CLAUDE,
    client: "claude-code",
    perDay: 14,
    models: ["claude-sonnet-4.5", "claude-opus-5"],
    baseTtft: 1500,
    succ: 0.989,
  },
  {
    route: R_DESK,
    client: "claude-desktop",
    perDay: 9,
    models: ["claude-sonnet-4.5"],
    baseTtft: 980,
    succ: 0.982,
  },
  {
    route: R_CODEX,
    client: "codex",
    perDay: 25,
    models: ["gpt-5.6-sol", "gpt-5.6-mini"],
    baseTtft: 1340,
    succ: 0.961,
  },
];

const stationLogs = (() => {
  const rows: {
    id: string;
    at_ms: bigint;
    route_id: string;
    client: string;
    model: string;
    status: number | null;
    first_token_ms: bigint | null;
    total_ms: bigint | null;
    retry_after_ms: bigint | null;
  }[] = [];
  let n = 0;
  for (const s of LOG_SHAPES) {
    const rand = seeded(s.route.length * 7919 + s.perDay);
    const mean = 86_400_000 / s.perDay;
    let t = ANCHOR;
    for (let i = 0; i < s.perDay * 7; i++) {
      t -= Math.max(1000, Math.round(rand() * 2 * mean));
      const x = rand();
      const bad = x > s.succ;
      const status = bad ? (x > 0.997 ? 500 : 429) : 200;
      rows.push({
        id: `log-${++n}`,
        at_ms: BigInt(t),
        route_id: s.route,
        client: s.client,
        model: s.models[Math.floor(rand() * s.models.length)],
        status,
        // 上游没回时首字取不到 —— `null`，⛔ 不拿 0 顶上。
        first_token_ms:
          status === 200
            ? BigInt(Math.round(s.baseTtft * 0.5 + rand() * s.baseTtft * 1.6))
            : null,
        total_ms:
          status === 500 ? null : BigInt(Math.round(1600 + rand() * 24000)),
        retry_after_ms:
          status === 429 ? BigInt(Math.round(10 + rand() * 25) * 1000) : null,
      });
    }
  }
  // 新的在前，跟 Rust 侧 `station_logs` 的顺序一致。
  return rows.sort((a, b) => Number(b.at_ms - a.at_ms));
})();

// 一轮检验：倍率对不上（实扣是标称的 1.3 倍），缓存和成功率量到了，
// 上下文窗口 / 最大输出 / 首字这一轮没测到 —— **没测到会压低可信度**，
// 这正是 `trust_of` 的语义：分母是六项全额，不是「测到的那几项」。
const stationAudits = [
  {
    route_id: R_CHEAP,
    round: {
      at_ms: 1757900000000n,
      trust: 29,
      // 只测到两项 —— 低分是因为没测够，不是因为这站坏
      evidence: "insufficient",
      model: "claude-opus-5",
      // 四类价目对照。moxi 是 sub2api 系 —— 站点那一列是**绝对单价**，
      // 不是倍率。⛔ 缺的那一类写 null，界面渲染成「—」；写 0 会被读成免费。
      rates: [
        {
          category: "input",
          station_ratio: null,
          station_price: 0.6,
          basis_kind: "absolute-prices",
          official: 5,
          basis: "published",
        },
        {
          category: "cache-read",
          station_ratio: null,
          station_price: 0.06,
          basis_kind: "absolute-prices",
          official: 0.5,
          basis: "published",
        },
        {
          category: "cache-write",
          station_ratio: null,
          station_price: 0.75,
          basis_kind: "absolute-prices",
          official: 6.25,
          basis: "derived",
        },
        {
          category: "output",
          station_ratio: null,
          station_price: 3,
          basis_kind: "absolute-prices",
          official: 25,
          basis: "published",
        },
      ],
      mult: 1.3,
      checks: [
        {
          kind: "rate",
          claimed: 0.12,
          previous: null,
          measured: 0.156,
          verdict: "differs",
        },
        {
          kind: "cache-hit",
          claimed: null,
          previous: null,
          measured: 0.49,
          verdict: "unmeasured",
        },
        {
          kind: "context-window",
          claimed: null,
          previous: null,
          measured: null,
          verdict: "unmeasured",
        },
        {
          kind: "max-output",
          claimed: null,
          previous: null,
          measured: null,
          verdict: "unmeasured",
        },
        {
          kind: "first-token",
          claimed: null,
          previous: null,
          measured: 1550,
          verdict: "unmeasured",
        },
        {
          kind: "success-rate",
          claimed: null,
          previous: null,
          measured: 0.964,
          verdict: "unmeasured",
        },
      ],
    },
  },
];

/**
 * 三个软件各自的调度状态。
 *
 * ⛔ **三个都要有。** 只铺 Claude Code 那一条的话，切到 Codex 看见「没开」，
 * 分不清是「按软件分对了」还是「夹具漏了」—— 跟线路那边同一条理由。
 *
 * 默认全关：自动换上游是会花钱的事，截图里不该显示成默认开着。
 */
const stationSchedules = [
  {
    client: "claude-code",
    enabled: false,
    prefs: {
      axes: ["stable", "fast"],
      floors: {
        min_success_rate: null,
        max_ttft_p95_ms: 3000,
        max_rate: null,
      },
    },
    last_switch_ms: null,
    last_note: "",
  },
  {
    client: "claude-desktop",
    enabled: false,
    prefs: {
      axes: ["cheap"],
      floors: {
        min_success_rate: 0.95,
        max_ttft_p95_ms: null,
        max_rate: null,
      },
    },
    last_switch_ms: null,
    last_note: "",
  },
  {
    client: "codex",
    enabled: false,
    prefs: {
      axes: ["fast"],
      floors: {
        min_success_rate: 0.99,
        max_ttft_p95_ms: null,
        max_rate: null,
      },
    },
    last_switch_ms: null,
    last_note: "",
  },
];

/** 演示用的那份 settings.json。⛔ 里面没有任何真实路径或 Key。 */
const DEMO_SETTINGS = [
  "{",
  '  "env": {',
  '    "DISABLE_AUTOUPDATER": "1"',
  "  }",
  "}",
].join("\n");

/** 演示用的桌面端那份配置：第三方网关模式，指向本机路由。 */
const DEMO_DESKTOP_CONFIG = [
  "{",
  '  "mcpServers": {},',
  '  "deploymentMode": "3p",',
  '  "inferenceProvider": "gateway",',
  '  "inferenceGatewayBaseUrl": "http://127.0.0.1:15721/cd",',
  '  "inferenceGatewayAuthScheme": "bearer",',
  '  "inferenceGatewayApiKey": "qb-gate-local-router"',
  "}",
].join("\n");

/** 演示用的 Codex 那份。 */
const DEMO_CODEX_CONFIG = [
  'model = "gpt-5"',
  'model_reasoning_effort = "high"',
  "",
  "[model_providers.qb_relay]",
  'base_url = "http://127.0.0.1:15721/v1"',
].join("\n");

/** 这个软件那份配置长什么样。⛔ 三个软件三份文件、三种格式标签。 */
function clientConfigOf(client: string) {
  const dir = "C:\\Users\\demo\\AppData\\Local\\ClaudeIpGate\\environments";
  if (client === "codex")
    return {
      path: dir + "\\qb-router-codex\\config.toml",
      text: DEMO_CODEX_CONFIG,
      format: "toml",
    };
  if (client === "claude-desktop")
    return {
      path: dir + "\\qb-router-claude-desktop\\claude_desktop_config.json",
      text: DEMO_DESKTOP_CONFIG,
      format: "desktop",
    };
  return {
    path: dir + "\\qb-router-claude-code\\settings.json",
    text: DEMO_SETTINGS,
    format: "json",
  };
}

const STATION_NO_TABLE = "这个站点没公布价目表（/api/pricing），模型名要自己填";

/**
 * 一个站点公布的模型表。
 *
 * ⛔ **一个开翻倍、一个不开** —— 界面上那句「计费翻倍 ×5」只有在有对照时
 * 才看得出意义：×0.15 翻 5 倍比 ×0.4 不翻倍更贵（输出占比高的活儿上）。
 */
const stationModels = [
  {
    model: "claude-opus-5",
    rates: {
      model_ratio: 0.15,
      completion_ratio: 5,
      cache_ratio: 0.1,
      create_cache_ratio: 1.25,
      group_ratio: null,
      peak_rate: null,
      per_request_price: null,
      input_price: null,
      cache_read_price: null,
      cache_write_price: null,
      output_price: null,
    },
    groups: [],
  },
  {
    model: "claude-sonnet-5",
    rates: {
      model_ratio: 0.4,
      completion_ratio: 1,
      cache_ratio: 0.1,
      create_cache_ratio: 1.25,
      group_ratio: null,
      peak_rate: null,
      per_request_price: null,
      input_price: null,
      cache_read_price: null,
      cache_write_price: null,
      output_price: null,
    },
    groups: [],
  },
  {
    model: "deepseek-chat",
    rates: {
      model_ratio: 0.02,
      completion_ratio: null,
      cache_ratio: null,
      create_cache_ratio: null,
      group_ratio: null,
      peak_rate: null,
      per_request_price: null,
      input_price: null,
      cache_read_price: null,
      cache_write_price: null,
      output_price: null,
    },
    groups: [],
  },
];

const FIXTURES: Record<string, (args?: Record<string, unknown>) => unknown> = {
  // ---------------------------------------------------------------- 中转站
  //
  // 三条线路，两个站点。**倍率那一格三种状态各来一个** ——
  // 截图要看得出「未核实 / 已核实 / 实测对不上」长什么样，
  // 只放一种的话，那两条分支在截图里就永远看不见。
  // ⛔ **每次都给一个新数组。** 真 IPC 那边每一发都是重新反序列化出来的 Vec，
  // 引用必然是新的；夹具图省事回同一个引用的话，`setRoutes(同一个引用)` 会被
  // React 直接跳过，而挂在 `[routes]` 上的 useMemo 也不重算。
  //
  // 症状极具迷惑性：分页角标是渲染期直接 `routes.filter()` 算的，**它会更新**；
  // 列表走的是 useMemo，**它不更新**。于是「删掉一条，角标少了 1，行还在」。
  // 看起来像列表组件有 bug，实际是夹具跟真接口的行为不一样。
  station_routes: () => [...stationRoutes],
  // 加 / 改 / 删都要能跑通：少了这两条，演示里点「保存」和「移除」会**直接抛**，
  // 而抛出来的是「找不到这条命令」—— 看起来像后端坏了，不像夹具没写。
  station_put_route: (args) => {
    const route = args?.route as (typeof stationRoutes)[number] | undefined;
    if (route) {
      if (args?.previousId && args.previousId !== route.id) {
        const old = stationRoutes.findIndex((r) => r.id === args.previousId);
        if (old >= 0) stationRoutes.splice(old, 1);
      }
      const at = stationRoutes.findIndex((r) => r.id === route.id);
      if (at >= 0) stationRoutes[at] = route;
      else stationRoutes.push(route);
    }
    return [...stationRoutes];
  },
  station_remove_route: (args) => {
    const at = stationRoutes.findIndex((r) => r.id === args?.id);
    if (at >= 0) stationRoutes.splice(at, 1);
    return [...stationRoutes];
  },
  // 本机路由的演示状态。**按软件各记一份** —— 跟真后端一样。
  //
  // 只留一份 current 的话，演示里在 Codex 上切一条线会把 Claude Code 的
  // 也换掉，而那正是这一版要修掉的行为：演示如果还是旧的，
  // 界面上看起来没问题，实际验的是一个已经不存在的后端。
  station_router_status: (args) => ({
    running: stationRouterUp,
    base_url: stationRouterUp ? "http://127.0.0.1:15721" : null,
    current_route:
      stationCurrent[String(args?.client ?? "claude-code")] ?? null,
    current_by_client: { ...stationCurrent },
    pending_route:
      stationPending[String(args?.client ?? "claude-code")] ?? null,
    switches: stationSwitches,
    pending_logs: 0,
  }),
  station_router_start: (args) => {
    stationRouterUp = true;
    return FIXTURES.station_router_status(args);
  },
  station_router_stop: (args) => {
    stationRouterUp = false;
    return FIXTURES.station_router_status(args);
  },
  // 切哪个软件的上游，由**这条线自己的 client** 决定 —— 跟真后端同一条规矩。
  // 拿参数里的 client 当依据的话，演示就验不出那个 bug。
  station_select_route: (args) => {
    const route = stationRoutes.find(
      (r) => r.id === args?.id || r.id === args?.routeId,
    );
    if (route) {
      const who = route.client;
      if (args?.immediate) {
        // `set_now`：直接落定，顺手撤掉排队。
        if (stationCurrent[who] !== route.id) stationSwitches += 1;
        stationCurrent[who] = route.id;
        delete stationPending[who];
      } else if (stationCurrent[who] === route.id) {
        // `request_switch`：换成当前这条等于取消排队。
        delete stationPending[who];
      } else {
        stationPending[who] = route.id;
      }
    }
    return FIXTURES.station_router_status({ client: route?.client });
  },
  // 一个动作干三件事：切上游 → 拉路由 → 起客户端进程。演示里前两件照做，
  // 第三件回一条假会话 —— ⛔ 但**不能只做前两件**：那样演示里点完「启动」
  // 看起来一切正常，而真机上少起的恰恰是最容易漏的那一步。
  station_launch: (args) => {
    const who = String(args?.client ?? "claude-code");
    // 带 routeId = 行上的「指定这一条再启动」，后端走 set_now；
    // 不带 = 右列的「启动」，选中的是哪条就走哪条，一点不动。
    if (args?.routeId)
      FIXTURES.station_select_route({ routeId: args.routeId, immediate: true });
    stationRouterUp = true;
    // 起完就当第一发请求已经发出去了：排队那条落定（对应后端 `begin()`）。
    const queued = stationPending[who];
    if (queued) {
      if (stationCurrent[who] !== queued) stationSwitches += 1;
      stationCurrent[who] = queued;
      delete stationPending[who];
    }
    return new Promise((done) => setTimeout(done, 1600)).then(() =>
      demoWorkspaceCall("session_launch", {
        client: who,
        kind: "relay",
        id: `qb-router-${who}`,
      }),
    );
  },
  // ⚡ 探测。演示里回一个**三态齐全**的结果：探通 / 明确拒绝 / 探不出来
  // 各来一个 —— 只回「全部支持」的话，那两条分支在截图里永远看不见，
  // 而「探不出来 ≠ 不支持」正是这一块最要紧的那条。
  station_probe: () => ({
    protocols: { anthropic: true, openai_chat: false, openai_responses: null },
    models: stationModels,
    pricing_problem: null,
  }),
  // 这份配置**整个软件共用**，不是一条线路一份 —— 换上游不重启客户端。
  // 演示里给一份只开了「禁用自动升级」的：那一项默认就该是开的，
  // 而另外五项关着，好让六个开关的两种状态在截图里都看得见。
  // ⛔ **三个软件三份文件、三种格式。** 都回 json 的话，桌面端那一页会摆出
  // Claude Code 的六个开关 —— 而那些键桌面端根本不读，勾了没有任何反应，
  // 界面上却看起来一切正常。演示必须能看出这三档的区别。
  station_client_config: (args) =>
    clientConfigOf(String(args?.client ?? "claude-code")),
  // 存了就真的回存进去那一份 —— 只回固定值的话，改完点保存
  // 编辑器会弹回旧内容，看起来像保存没生效。
  station_client_config_save: (args) => {
    const base = clientConfigOf(String(args?.client ?? "claude-code"));
    return { ...base, text: String(args?.text ?? base.text) };
  },
  station_refresh_health: () => stationHealth,
  station_decide: () => stationDecision,
  station_schedules: () => stationSchedules,
  // 开关落到夹具里，跟真后端一样是**有状态**的 —— 只回一个固定值的话，
  // 点完「启动智能调度」按钮弹回去，看起来像按钮坏了。
  station_schedule_set: (args) => {
    const who = String(args?.client ?? "claude-code");
    const s =
      stationSchedules.find((x) => x.client === who) ?? stationSchedules[0];
    if (args?.enabled != null) {
      s.enabled = Boolean(args.enabled);
      s.last_note = s.enabled ? "现任仍然是最好的那条" : "";
    }
    if (args?.prefs != null) s.prefs = args.prefs as (typeof s)["prefs"];
    return { ...s };
  },
  station_logs: () => stationLogs,
  station_audits: (args) => (args?.routeId === R_CHEAP ? stationAudits : []),
  // 「选中转站 → 选分组 → 选模型」那第三级。
  //
  // 两种情形都要有:有价目表的(能列模型、能看出哪个开了计费翻倍)和
  // 没价目表的(退化成自己填模型名)—— 截图里少一种,那条分支就永远看不见。
  station_models: (args) =>
    args?.routeId === R_CHEAP
      ? { models: [], recommended: null, problem: STATION_NO_TABLE }
      : {
          models: stationModels,
          recommended: "claude-opus-5",
          problem: null,
        },
  gate_status: () => gate,
  allowlist_read: () => gate.allowlist,
  hook_status: () => hook,
  country_presets: () => [
    ["只留美国", ["US"]],
    [
      "常用支持地区",
      [
        "AU",
        "CA",
        "CH",
        "DE",
        "ES",
        "FR",
        "GB",
        "IE",
        "IT",
        "JP",
        "KR",
        "NL",
        "NZ",
        "PL",
        "SE",
        "SG",
        "TW",
        "US",
      ],
    ],
  ],
  probe_ip: () => ip,
  probe_ip_lookup: ({ ip: target } = {}) => {
    const value = String(target).trim();
    const v4 =
      /^(\d{1,3}\.){3}\d{1,3}$/.test(value) &&
      value.split(".").every((n) => Number(n) <= 255);
    const v6 = /^[a-f0-9:]+$/i.test(value) && value.includes(":");
    if (!v4 && !v6)
      throw new Error(
        "请输入有效的 IPv4 或 IPv6 地址，不要填写网址、端口或代理账号。",
      );
    return {
      ip: value,
      source: "IPQuery",
      checked_at: new Date().toISOString(),
      asn: "AS64496",
      organization: "Example ISP",
      country: "United States",
      country_code: "US",
      city: "Example city",
      timezone: "America/New_York",
      risk_score: 12,
      is_vpn: false,
      is_proxy: false,
      is_tor: false,
      is_datacenter: null,
    };
  },
  probe_purity: () => purity,
  probe_dns: () => dns,
  purity_criteria: () => criteria,
  detect_software: () => software,
  install_probe: () => installProbe,
  claude_traces: () => traces,
  browser_audit: () => browserAudit,
  proxy_read: () => proxy,
  ipv6_status: () => structuredClone(ipv6Status),
  ipv6_set: (args) => {
    ipv6Status.disable = Boolean(args?.disable);
    ipv6Status.bindings[0].enabled = !ipv6Status.disable;
    ipv6Status.restore_pending = ipv6Status.disable ? 2 : 0;
    return structuredClone(ipv6Status);
  },
  firewall_rules: () => firewallRules,
  firewall_adapters: () => adapters,
  checkup_scan: () => checkup,
  accounts_list: () => accounts,
  accounts_tokens: () => tokenUsage,
  accounts_token_summary: (args) => summaryFor(Number(args?.days ?? 1)),
  // 演示里给「本地令牌已过期」那一档 —— 实机上它就是最常见的那个，
  // 而且它是唯一一个**不发请求**的分支，最该让人在截图里先看见。
  account_probe: () => accountProbe,
  egress_checks_scan: () => structuredClone(egress),
  egress_checks_fix: (args) => {
    const item = egress.items.find((it) => it.id === args?.id && it.fixable);
    if (!item) throw new Error("演示：该项不能自动修复");
    item.state = "pass";
    item.fixable = false;
    item.detail = "演示：当前用户策略已设置，仍需重启 Chrome 核对。";
    egress.undoable.push(item.id);
    return "演示：已保存原值并设置策略，可以撤销。";
  },
  egress_checks_undo: (args) => {
    const item = egress.items.find((it) => it.id === args?.id);
    if (!item || !egress.undoable.includes(item.id))
      throw new Error("演示：没有可撤销的修改");
    item.state = "warn";
    item.fixable = true;
    item.detail = "演示：已还原策略原值。";
    egress.undoable = egress.undoable.filter((id) => id !== item.id);
    return "演示：已撤销";
  },
  managed_status: () => managed,
  managed_externals: () => externals,
  managed_history: () => versionHistory,
  managed_rollback: () => "已回滚到 2.0.11，已重新上锁 5 个副本",
  managed_cleanup: () => cleanup,
  // ⛔ 按 target 过滤。不过滤的话 Codex 的预设会出现在 Claude Code 的弹窗里，
  // 点一下把 base_url 填成一个 Claude Code 根本连不上的地址 ——
  // 而真后端 (`relay::presets::for_target`) 是过滤了的，演示不该比它宽松。
  relay_presets: (args) =>
    presets.filter((p) => !args?.target || p.target === args.target),
  tz_current: () => "America/New_York",
  snapshot_list: () => snapshots,
  profile_list: () => profileStore,
  settings_load: () => settings,
  progress_load: () => structuredClone(progress),
  progress_set: (args) => {
    const id = String(args?.id);
    progress.steps[id] = {
      state: args?.state as "passed",
      risk: args?.risk as "low",
      detail: String(args?.detail ?? ""),
      updated_at: new Date().toISOString(),
    };
    return structuredClone(progress);
  },
  upgrade_plan: () => upgrade,
  update_status: () => update,
  killswitch_preview: () => kill,
  plugin_list: () => plugins,
  plugin_catalog_status: () => catalog,
  tavern_config: () => tavern,
  tavern_locate: () => survey,
  tavern_assets: () => assets,
  tavern_backups: () => backups,
};

/** Only a missing fixture should fall through to the other demo dispatcher. */
export class MissingDemoCommand extends Error {}

/** 演示模式下代替 `invoke`。故意留 120ms，让加载态也长得像真的。 */
export async function demoCall<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  await new Promise((r) => setTimeout(r, 120));
  if (cmd.startsWith("codex_")) return codexDemo(cmd, args ?? {}) as T;
  const make = FIXTURES[cmd];
  if (!make)
    throw new MissingDemoCommand(
      `演示模式：「${cmd}」没有演示数据，这个操作只在装好的面板里可用。`,
    );
  return make(args) as T;
}

// Fictional desktop account fixtures; never used by production builds.
const codexSlots = Array.from({ length: 6 }, (_, i) => ({
  id: `codex-demo-${i}`,
  label: i === 0 ? "个人账户" : `工作账户 ${i}`,
  active: i === 0,
  logged_in: i < 5,
  auth_state: i < 5 ? "已登录 · 本地凭据" : "未登录",
  email: i < 5 ? `account${i}@example.test` : null,
  plan: i < 5 ? "pro" : null,
}));
let codexLaunched: string | null = codexSlots[0].id;
function codexDemo(cmd: string, args: Record<string, unknown>) {
  if (cmd === "codex_accounts")
    return {
      slots: structuredClone(codexSlots),
      launched_id: codexLaunched,
      launched_pid: codexLaunched ? 100 : null,
      launched_at: codexLaunched ? "demo-start" : null,
    };
  if (cmd === "codex_desktop_status")
    return {
      executable: "C:/Demo/Codex/ChatGPT.exe",
      version: "26.9.0",
      running: !!codexLaunched,
      processes: codexLaunched ? [{ pid: 100, started: "demo-start" }] : [],
    };
  if (cmd === "codex_usage")
    return {
      input: 82450,
      output: 12380,
      cached: 63100,
      reasoning: 5320,
      total: 94830,
      sessions: 8,
      files_read: 10,
      files_failed: 0,
      duplicates: 2,
      incomplete: 0,
      checked_at: "2026-09-16 10:35",
    };
  if (cmd === "codex_create") {
    const id = `demo-${codexSlots.length}`;
    codexSlots.push({
      id,
      label: String(args.label),
      active: false,
      logged_in: false,
      auth_state: "未登录",
      email: null,
      plan: null,
    });
    return id;
  }
  if (cmd === "codex_switch" || cmd === "codex_launch") {
    for (const s of codexSlots) s.active = s.id === args.id;
    codexLaunched = cmd === "codex_launch" ? String(args.id) : null;
    return null;
  }
  if (cmd === "codex_archive") {
    const i = codexSlots.findIndex((s) => s.id === args.id);
    if (i >= 0) codexSlots.splice(i, 1);
    return null;
  }
  if (cmd === "codex_close") {
    codexLaunched = null;
    return null;
  }
  throw new MissingDemoCommand(`Unknown Codex fixture: ${cmd}`);
}
