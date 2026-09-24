import { demoWorkspaceCall } from "./workspaceDemo";
import packageInfo from "../../package.json";
import {
  demoCodexSummary,
  demoSlotSummary,
  demoSummarize,
  demoTokenUsage,
  demoUsageOverview,
} from "./demoUsage";
import { addDays, today as todayYmd, ymd } from "./dates";
import type { BrowserReport } from "./generated/BrowserReport";
import type { AntigravityStatus } from "./generated/AntigravityStatus";
import type { AntigravityAccount } from "./generated/AntigravityAccount";
import type { AntigravityIdentity } from "./generated/AntigravityIdentity";
import type { AntigravityUsage } from "./generated/AntigravityUsage";
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
  GptBridgeStatus,
  GeminiBridgeStatus,
  UiStatus,
  UiConfig,
  DangerRules,
  AccountProbe,
  EgressChecks,
  TokenSummary,
  TokenBucket,
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
  codexDesktop: {
    id: "codex-desktop",
    name: "Codex 桌面端",
    installed: true,
    version: "26.9.0",
    path: "C:\\Demo\\Codex\\ChatGPT.exe",
    advisory: null,
  },
  antigravity: {
    id: "antigravity",
    name: "反重力",
    installed: true,
    version: "2.15.0",
    path: `${HOME}\\AppData\\Local\\Programs\\antigravity\\Antigravity.exe`,
    advisory: null,
  },
  antigravityIde: {
    id: "antigravity-ide",
    name: "反重力 IDE",
    installed: true,
    version: "1.19.4",
    path: `${HOME}\\AppData\\Local\\Programs\\Antigravity IDE\\Antigravity IDE.exe`,
    advisory: null,
  },
  geminiCli: {
    id: "gemini-cli",
    name: "Gemini CLI",
    installed: true,
    version: "0.9.0",
    path: `${HOME}\\AppData\\Roaming\\npm\\node_modules\\@google\\gemini-cli\\dist\\index.js`,
    advisory: null,
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
 * 详情页的 token 统计与用量小结：都在 `demoUsage.ts`（2026-09-24 搬过去，
 * 跟 Rust 的 `summarize_with` 同一个算式现算美元）。
 */
const tokenUsage: TokenUsage = demoTokenUsage;

/**
 * 出口一致性。故意让四种状态都出现一次 —— 截图那一圈要能看出
 * 「查不了」跟「对得上」长得不一样。
 */
const egress: EgressChecks = {
  undoable: [],
  checked_at: "2026-09-16 04:12",
  items: [
    {
      id: "anthropic_reach",
      label: "Anthropic 服务可达",
      state: "pass",
      detail:
        "不带密钥问 API 回 401（缺密钥时的正常回答，说明这个出口放行），claude.ai 与 anthropic.com 都打得开。",
      fixable: false,
      manual: null,
    },
    {
      id: "claude_dns",
      label: "claude.ai 的解析",
      state: "pass",
      detail:
        "解析正常：Anthropic 自己的地址段。claude.ai → 160.79.104.10；api.anthropic.com → 160.79.104.10。",
      fixable: false,
      manual: null,
    },
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

/** 用量小结（账户卡那一条）。按档位从桶里现算，见 `demoUsage.ts`。 */
function summaryFor(days: number): TokenSummary {
  return demoSlotSummary(days);
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
      tunnel: false,
      connected: true,
      via_tunnel: null,
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
      tunnel: false,
      connected: true,
      via_tunnel: null,
      is_private: false,
      is_domestic: false,
    },
    // 网卡那一层：有线网卡的 DNS 被隧道软件改成了隧道网段里的地址（走隧道，没问题），
    // 断开的 Wi-Fi 上还挂着路由器的地址（不看）。
    {
      address: "172.19.0.2",
      country_code: null,
      country_name: null,
      asn: null,
      from_adapter: true,
      interface: "以太网",
      tunnel: false,
      connected: true,
      via_tunnel: true,
      is_private: true,
      is_domestic: false,
    },
    {
      address: "192.168.1.1",
      country_code: null,
      country_name: null,
      asn: null,
      from_adapter: true,
      interface: "WLAN",
      tunnel: false,
      connected: false,
      via_tunnel: false,
      is_private: true,
      is_domestic: false,
    },
  ],
  passed: true,
  findings: [],
  upstream_conclusion: null,
  note: "10 个探针域名全部由出口同侧的解析器回报，物理网卡的 DNS 没有漏出去。高级通过仍要交给 Codex 复核。",
  score: 100,
  adapters_safe: true,
  adapters_note: "看了 以太网 上配的 DNS：都走隧道。断开的 WLAN 不看。",
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

/** 演示的真实浏览器报告：点过「用默认浏览器测」才有。 */
let demoProbeReport: BrowserReport | null = null;
function demoBrowserReport(): BrowserReport {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  const stamp = `${ymd(now)} ${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`;
  return {
    page_json: JSON.stringify({
      v: 1,
      signals: [
        { id: "timezone", raw: "America/New_York", score: 0 },
        { id: "language", raw: "en-US, en", score: 0 },
        { id: "fonts", raw: "检测到 6 款中文字体", score: 1 },
        { id: "vendorFonts", raw: "未检测到", score: 0 },
        { id: "webrtcLeak", raw: "候选地址泄露（203.0.113.7）", score: 0.5 },
        { id: "cnBrowser", raw: "Google Chrome", score: 0 },
        { id: "deviceVendor", raw: "未识别", score: 0 },
        { id: "intlLocale", raw: "en-US", score: 0 },
        { id: "timezoneOffset", raw: "UTC-4", score: 0 },
        { id: "emoji", raw: "Microsoft 风格", score: 0.1 },
      ],
      webrtc: ["203.0.113.7"],
      languages: ["en-US", "en"],
      timezone: "America/New_York",
      locale: "en-US",
      ua: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36",
      ua_platform: "Windows",
      brands: "Google Chrome 128, Chromium 128",
    }),
    accept_language: "en-US,en;q=0.9",
    ch_platform: "Windows",
    user_agent:
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36",
    received_at: stamp,
    received_unix: Math.floor(now.getTime() / 1000),
  };
}

const checkup: Checkup = {
  exit_ips: ["203.0.113.7"],
  items: [
    {
      id: "proxy",
      label: "代理形态",
      state: "warn",
      detail:
        "开着 PAC 自动分流（http://127.0.0.1:7891/proxy.pac），同时TUN / 虚拟网卡接管了默认路由（Meta）。认系统代理的程序（浏览器、桌面端）按网站分流，不同网站可能从不同出口出去；面板「跟随系统代理」那一路也不认 PAC，出口一致性那一项看不到它。",
      fixable: false,
      manual:
        "要关就去「设置 → 网络和 Internet → 代理 → 使用设置脚本」里关，或者在代理软件里改成全局 / TUN。面板不替你关 —— 你的网可能正是靠它出去的。",
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
      id: "anthropic_reach",
      label: "Anthropic 服务可达",
      state: "pass",
      detail:
        "不带密钥问 API 回 401（缺密钥时的正常回答，说明这个出口放行），claude.ai 与 anthropic.com 都打得开。",
      fixable: false,
      manual: null,
    },
    {
      id: "claude_dns",
      label: "claude.ai 的解析",
      state: "pass",
      detail:
        "解析正常：代理接管（fake-ip）。claude.ai → 198.18.0.21；api.anthropic.com → 198.18.0.22。",
      fixable: false,
      manual: null,
    },
    {
      id: "ipv6",
      label: "IPv6",
      state: "fail",
      detail:
        "IPv6 出口 2001:db8::7 在 CN，IPv4 出口在 US —— 隧道没接管 IPv6，走 v6 的请求会把另一个国家的地址露给对面。2 / 5 张网卡还开着 IPv6。",
      fixable: false,
      manual:
        "在代理软件里打开 IPv6 接管；或者用「IP 纯净度 → 禁用本机 IPv6」开关（需要管理员授权，能随时恢复原设置）。",
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
  codex_outside_gate: false,
  antigravity_outside_gate: false,
  antigravity_hub_quota: true,
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
  // 0.25.3：启动时检查更新，照 Rust 的默认值开着；没有跳过的版本。
  update_check_on_start: true,
  update_skipped_version: null,
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
      {
        label: "官方 Codex CLI（GPT 桥接用）",
        ok: true,
        detail: `${HOME}\\AppData\\Roaming\\npm\\node_modules\\@openai\\codex\\node_modules\\@openai\\codex-win32-x64\\vendor\\x86_64-pc-windows-msvc\\bin\\codex.exe`,
      },
      { label: "端口 5001 / 5002 / 8000", ok: true, detail: "都空着" },
    ],
  },
  {
    id: "codex-egress",
    name: "Codex 出站与换出口（ccodex 引擎）",
    state: "ready",
    detail: "已安装，没在跑。",
    checks: [
      {
        label: "程序文件",
        ok: true,
        detail: `${HOME}\\AppData\\Local\\Programs\\ccodex-sleep-state\\ccodex-sleep-state.exe（版本 v0.3.0）`,
      },
      { label: "端口 17841", ok: true, detail: "空闲" },
    ],
  },
  {
    id: "antigravity-ui",
    name: "反重力 · 汉化与审批",
    state: "ready",
    detail: "未运行 · 点「附加」把汉化与审批引擎接到正在跑的 Hub 上",
    checks: [
      {
        label: "反重力 Hub",
        ok: true,
        detail: "已安装（位置表 install::antigravity）",
      },
      {
        label: "汉化字典",
        ok: true,
        detail: "1312 条（EasyAntigravity，MIT）",
      },
      { label: "高危规则", ok: true, detail: "9 条启用 / 共 9 条" },
    ],
  },
];
/** 演示里的出站插件运行态：启动翻真、停止翻假，状态随之变。 */
let egressRunning = false;
let egressExe: string | null = null;

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
  gpt_bridge_port: 5002,
  gpt_model: "",
  gpt_system_prompt: "",
  gpt_effort: "medium",
  gpt_persist_sessions: false,
  gemini_bridge_port: 5003,
  gemini_model: "",
  gemini_system_prompt: "",
};

/** 内置 Gemini 桥接的演示状态。驱动的是官方 Gemini CLI，不是反重力本体。 */
const geminiBridge: GeminiBridgeStatus = {
  running: false,
  port: 5003,
  url: "http://127.0.0.1:5003/v1",
  slot: "个人 Google",
  slot_logged_in: true,
  gemini_cli: `${HOME}\\nodejs\\node.exe ${HOME}\\AppData\\Roaming\\npm\\node_modules\\@google\\gemini-cli\\dist\\index.js`,
  token_path: `${HOME}\\AppData\\Local\\ClaudeIpGate\\plugins\\gemini-bridge\\token.txt`,
  model: "gemini-default",
  detail: "未运行 · 下次会用槽位「个人 Google」",
};

/** 反重力汉化与审批引擎的演示状态：没附加，字典与规则都齐。 */
let antigravityUiRunning = false;
const antigravityUiConfig: UiConfig = {
  enable_i18n: true,
  auto_accept: true,
  block_dangerous: true,
  prefer_option: 4,
  attach_on_launch: true,
};
const antigravityUiStatus = (): UiStatus => ({
  running: antigravityUiRunning,
  port: antigravityUiRunning ? 9229 : 0,
  targets: antigravityUiRunning ? 2 : 0,
  sockets: antigravityUiRunning ? 2 : 0,
  // 演示数据里两个源都在：Hub 核实生效，IDE 没有端口（使用者自己起的那种）——
  // 这正是界面上两行必须长得不一样的那一档。
  sources: antigravityUiRunning
    ? [
        {
          product: "hub",
          label: "反重力",
          port: 9229,
          targets: 2,
          sockets: 2,
          verified: 2,
          dict_in_page: 1312,
          error: "",
        },
        {
          product: "ide",
          label: "反重力 IDE",
          port: 0,
          targets: 0,
          sockets: 0,
          verified: 0,
          dict_in_page: 0,
          error: "",
        },
      ]
    : [],
  verified: antigravityUiRunning ? 2 : 0,
  last_attach_error: "",
  inject_count: antigravityUiRunning ? 12 : 0,
  approve_count: antigravityUiRunning ? 3 : 0,
  block_count: antigravityUiRunning ? 1 : 0,
  cdp_error: "",
  config: structuredClone(antigravityUiConfig),
  dict_entries: 1312,
  rules_total: 9,
  rules_on: 9,
  rules_path: `${HOME}\\AppData\\Local\\ClaudeIpGate\\antigravity\\danger-rules.json`,
  hub_installed: true,
  ide_installed: true,
  port_file_present: true,
  log: antigravityUiRunning
    ? [
        {
          at: "10:02:11",
          category: "CDP",
          message: "已连接页面并注入：Antigravity",
        },
        {
          at: "10:03:40",
          category: "AUTO-ACCEPT",
          message: "放行 · 选项[4] 始终允许 · npm test",
        },
        {
          at: "10:05:02",
          category: "SECURITY ALERT",
          message: "拦截高危指令[rm-rf]: rm -rf ./build",
        },
      ]
    : [],
  detail: antigravityUiRunning
    ? "运行中 · 已在 2 个页面核实生效"
    : "未运行 · 点「附加」把汉化与审批引擎接到正在跑的反重力上",
});
const antigravityRules: DangerRules = {
  version: 1,
  enabled: true,
  rules: [
    {
      id: "rm-rf",
      name: "递归强制删除",
      description: "rm -rf / rm -fr / rm --force",
      pattern: "\\brm\\s+(-[a-zA-Z]*r[a-zA-Z]*f|--force)",
      flags: "i",
      enabled: true,
    },
    {
      id: "disk-wipe",
      name: "磁盘破坏",
      description: "format / diskpart / mkfs",
      pattern: "\\b(format|diskpart|mkfs|wipefs|shred)\\b",
      flags: "i",
      enabled: true,
    },
    {
      id: "git-force-push",
      name: "Git 强制推送",
      description: "git push --force / -f",
      pattern: "\\bgit\\s+push\\s+.*(-f|--force)\\b",
      flags: "i",
      enabled: true,
    },
  ],
};

/**
 * 反重力账户页的演示状态：Hub 与 IDE 都装了、都没在跑。
 *
 * 三条账户故意覆盖三种形状（一眼能看出两半是独立的）：
 *
 * 1. 两半都登了、正在用的那一条；
 * 2. 只有 IDE 那一半的（从旧清单升上来的形状）—— 它会显示「并入…」；
 * 3. 只有 CLI 那一半且还没登的。
 */
const agAccounts: AntigravityAccount[] = [
  {
    id: "ag-demo-0",
    label: "个人账户",
    active: true,
    ide_dir: `${HOME}\\AppData\\Local\\ClaudeIpGate\\antigravity-accounts\\ag-demo-0\\ide-user-data`,
    ide_logged_in: true,
    ide_auth_state: "已登录 · 令牌由 IDE 自己保管在它的状态库里",
    email: "someone@example.com",
    tier: "Google AI Pro",
    identity_error: null,
    quota: [],
    written_at: "2026-09-21 03:35",
    cli_dir: `${HOME}\\AppData\\Local\\ClaudeIpGate\\antigravity-accounts\\ag-demo-0\\cli-home`,
    cli_logged_in: true,
    cli_auth_state: "已登录 · 本地凭据（由 Gemini CLI 自己保管）",
  },
  {
    id: "ag-demo-1",
    label: "工作账户",
    active: false,
    ide_dir: `${HOME}\\AppData\\Local\\ClaudeIpGate\\antigravity-ide-accounts\\ag-ide-demo-1\\user-data`,
    ide_logged_in: false,
    ide_auth_state: "未登录 · 还没在这个账户起过 IDE",
    email: null,
    tier: null,
    identity_error: null,
    quota: [],
    written_at: null,
    cli_dir: null,
    cli_logged_in: false,
    cli_auth_state: "还没有 CLI 那一半",
  },
  {
    id: "ag-demo-2",
    label: "个人 Google",
    active: false,
    ide_dir: null,
    ide_logged_in: false,
    ide_auth_state: "还没有 IDE 那一半",
    email: null,
    tier: null,
    identity_error: null,
    quota: [],
    written_at: null,
    cli_dir: `${HOME}\\AppData\\Local\\ClaudeIpGate\\gemini-accounts\\gemini-demo-1\\home`,
    cli_logged_in: false,
    cli_auth_state: "未登录",
  },
];

const antigravityIdentity = (): AntigravityIdentity => {
  const reset = Math.floor(Date.now() / 1000) + 3 * 3600 + 20 * 60;
  const model = (
    label: string,
    model_id: number,
    remaining: number | null,
    tags: string[],
  ) => ({
    label,
    model_id,
    remaining,
    reset_at: remaining == null ? null : "2026-09-21 07:38",
    reset_epoch: remaining == null ? null : reset,
    tags,
  });
  return {
    name: "演示用户",
    email: "someone@example.com",
    tier_id: "g1-pro-tier",
    tier_name: "Google AI Pro",
    written_at: "2026-09-21 03:35",
    models: [
      model("Gemini 3.8 Flash (High)", 1318, 1, ["Fast", "Limited time"]),
      model("Gemini 3.8 Flash (Medium)", 1319, 1, ["Fast", "Limited time"]),
      model("Gemini 3.8 Flash (Low)", 1320, 1, ["Fast", "Limited time"]),
      model("Gemini 3.7 Flash (High)", 1298, 0.62, ["Fast"]),
      model("Gemini 3.7 Flash (Medium)", 1299, 0.62, ["Fast"]),
      model("Gemini 3.7 Flash (Low)", 1300, 0.62, ["Fast"]),
      model("Claude Opus 4.6 (Thinking)", 1026, 0.85, []),
      model("GPT-OSS 120B (Medium)", 342, null, []),
    ],
  };
};
/** 反重力用量：一个模型占大头、缓存读淹掉输入，跟实机的比例一个样。 */
const antigravityUsage = (days: number): AntigravityUsage => {
  const scale = days === 1 ? 1 : days === 7 ? 6 : days === 30 ? 20 : 34;
  const flash = {
    model: "gemini-3.7-flash",
    input: 3_082_000 * scale,
    output: 124_500 * scale,
    cache_read: 60_126_000 * scale,
    messages: 349 * scale,
  };
  const opus = {
    model: "claude-opus-4-6-thinking",
    input: 41_800 * scale,
    output: 830 * scale,
    cache_read: 52_200 * scale,
    messages: 1 * scale,
  };
  // 按天的桶：故意中间缺一天 —— 柱状图上没有那根柱子，跟「那天是 $0」是两回事。
  // 日期按本地日历（`dates.ts`）；原来用 `toISOString()`，东八区整张图错一天。
  // `claude-opus-4-6-thinking` 在演示价里没有 —— 「没有官方价」那条路也得有人看过。
  const span = days === 1 ? 1 : days === 7 ? 7 : days === 30 ? 14 : 20;
  const buckets: TokenBucket[] = [];
  for (let i = span - 1; i >= 0; i--) {
    if (span > 3 && i === 2) continue; // 故意缺一天
    const day = addDays(todayYmd(), -i);
    const f = (1 + Math.sin(i)) / 2 + 0.4;
    for (const m of [flash, opus]) {
      buckets.push({
        day,
        model: m.model,
        input: Math.round((m.input / span) * f),
        output: Math.round((m.output / span) * f),
        cache_write: 0,
        cache_write_1h: 0,
        cache_read: Math.round((m.cache_read / span) * f),
        messages: Math.max(1, Math.round((m.messages / span) * f)),
      });
    }
  }
  return {
    summary: demoSummarize(buckets, days),
    models: [flash, opus],
    files_read: 49,
    files_failed: 0,
    legacy_skipped: 16,
    incomplete: 0,
    duplicates: 0,
    scanned_dirs: [
      `${HOME}\\.gemini\\antigravity\\conversations`,
      `${HOME}\\.gemini\\antigravity-ide\\conversations`,
    ],
    checked_at: new Date().toTimeString().slice(0, 5),
  };
};
let antigravityLive: Record<string, boolean> = { hub: false, ide: false };
const antigravityStatus = (): AntigravityStatus => ({
  hub: {
    product: "hub",
    label: "反重力",
    installed: true,
    path: `${HOME}\\AppData\\Local\\Programs\\antigravity\\Antigravity.exe`,
    session_id: antigravityLive.hub ? "demo-session-ag" : null,
    pid: antigravityLive.hub ? 4242 : null,
    data_dir: `${HOME}\\.gemini\\antigravity`,
  },
  ide: {
    product: "ide",
    label: "反重力 IDE",
    installed: true,
    path: `${HOME}\\AppData\\Local\\Programs\\Antigravity IDE\\Antigravity IDE.exe`,
    session_id: antigravityLive.ide ? "demo-session-ag-ide" : null,
    pid: antigravityLive.ide ? 4343 : null,
    data_dir: `${HOME}\\.gemini\\antigravity-ide`,
  },
  under_gate: !settings.antigravity_outside_gate,
  auto_update: true,
  login_seen: true,
  gemini_cli_installed: true,
  hub_logged_in: true,
  // Hub 的邮箱是从凭据里那个 `id_token` 本机解出来的，零网络请求。
  // 演示里故意跟 IDE 那一侧是**同一个**账户 —— 实机上它们完全可以不是。
  hub_identity: {
    email: "someone@example.com",
    email_verified: true,
    token_expires_at: "2026-09-21 12:40",
    token_expired: false,
    auth_method: "google",
  },
  hub_identity_error: null,
  accounts: { slots: structuredClone(agAccounts) },
  identity_source: agAccounts.some((s) => s.active)
    ? `账户「${agAccounts.find((s) => s.active)?.label}」`
    : "默认资料目录",
  identity: agAccounts.find((s) => s.active)?.ide_logged_in
    ? antigravityIdentity()
    : null,
  identity_error: null,
});

/** 内置 GPT 桥接的演示状态：没在跑，但槽位、CLI、地址都齐。 */
const gptBridge: GptBridgeStatus = {
  running: false,
  port: 5002,
  url: "http://127.0.0.1:5002/v1",
  slot: "个人账户",
  slot_logged_in: true,
  codex_exe: `${HOME}\\AppData\\Roaming\\npm\\node_modules\\@openai\\codex\\node_modules\\@openai\\codex-win32-x64\\vendor\\x86_64-pc-windows-msvc\\bin\\codex.exe`,
  token_path: `${HOME}\\AppData\\Local\\ClaudeIpGate\\plugins\\gpt-bridge\\token.txt`,
  model: "codex-default",
  effort: "medium",
  persist_sessions: false,
  detail: "未运行 · 下次会用槽位「个人账户」",
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

/**
 * 应用自己的更新（0.25.3）。
 *
 * **启动那一次（`manual: false`）永远「没有新版」**：更新弹窗是全局的，演示里一打开就弹，
 * `test:ui` 那 151 个组合就全被它盖住了。只有设置页点「检查更新」（`manual: true`）
 * 才「查到」一个编出来的 0.99.0，好让弹窗能被看见、被 `test:ui` 走一遍。
 * 「一键更新」在演示里只会报错 —— 演示不下载、不安装、不退出。
 */
let update: UpdateStatus = {
  current_version: packageInfo.version,
  check_on_start: true,
  latest: null,
  update_available: false,
  skipped: false,
  checked_at: null,
  error: null,
};
const demoRelease = {
  version: "0.99.0",
  tag: "v0.99.0",
  published_at: "2026-10-01T08:00:00Z",
  notes: [
    "## 更新内容",
    "",
    "- 演示数据：这一版并不存在，只用来展示更新弹窗长什么样。",
    "- 点「一键更新」会从 GitHub 下载安装包，核对 SHA256SUMS.txt 之后再安装。",
    "",
    "## 修复",
    "",
    "- 演示数据：一条修复说明。",
  ].join("\n"),
  page_url: "https://github.com/smithtaylor7748-ops/qb-gate/releases",
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
/** 官方 Codex turn-state 上游接进路由了没。开启识别后翻真。 */
let stationTurnstateArmed = false;
/** 识别接管了哪个 Codex 槽位（真后端是落盘 marker 里的槽位标签）。 */
let stationTurnstateTakeover: string | null = null;
/** turn-state 注入开关与账号规则 —— 跟真后端一样由 configure 落定、由 status 回读。 */
let stationTurnstateEnabled = false;
let stationTurnstateTeam = false;
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
      problems: [],
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
    turnstate_official_armed: stationTurnstateArmed,
    turnstate_enabled: stationTurnstateEnabled,
    turnstate_team: stationTurnstateTeam,
    turnstate_takeover: stationTurnstateTakeover,
  }),
  station_router_start: (args) => {
    stationRouterUp = true;
    return FIXTURES.station_router_status(args);
  },
  // 官方 Codex turn-state（实验）。演示数据给一张采到的 292，好看清界面。
  station_turnstate_configure: (args) => {
    stationTurnstateEnabled = Boolean(args?.enabled);
    stationTurnstateTeam = Boolean(args?.team);
    return null;
  },
  // 开启识别：演示里挂上官方上游、把「接管的槽位」记成当前激活的那个 —— 跟真后端
  // 一样只改槽位配置、不起进程；起 Codex 仍是账户页那颗按钮的事。
  station_turnstate_enable: () => {
    if (egressRunning)
      throw new Error(
        "出站插件（ccodex）正在运行。它和识别都要改同一个槽位的 Codex 配置，一次只能开一个：先到扩展中心停掉插件（会恢复 Codex 配置），再开启识别。",
      );
    const active = codexSlots.find((s) => s.active);
    if (!active) throw new Error("没有激活的 Codex 账户槽位。");
    if (!active.logged_in)
      throw new Error(
        `槽位「${active.label}」还没用 ChatGPT 登录。先在账户页打开它登录，再开启识别。`,
      );
    stationRouterUp = true;
    stationTurnstateArmed = true;
    stationTurnstateTakeover = active.label;
    return FIXTURES.station_router_status({ client: "codex" });
  },
  station_turnstate_disable: () => {
    stationTurnstateArmed = false;
    stationTurnstateTakeover = null;
    return FIXTURES.station_router_status({ client: "codex" });
  },
  station_turnstate_status: () => [
    {
      model: "gpt-5.6-sol",
      status: {
        usable: true,
        version: 3n,
        blocks: 10,
        length: 292,
        remaining_seconds: 3180n,
        ready: true,
        strikes: 0,
        observations: 14n,
      },
    },
  ],
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
  station_run_audit: (args) => ({
    ...stationAudits[0],
    route_id: args?.routeId,
    round: {
      ...stationAudits[0].round,
      at_ms: Date.now(),
      model: args?.model,
      mult: null,
      batch: {
        planned: args?.cold ? 7 : 6,
        samples: Array.from({ length: args?.cold ? 7 : 6 }, (_, i) => ({
          stage: i === 0 ? "预热" : i === 6 ? "冷前缀对照" : "缓存验证 " + i,
          request_id: "demo-request-" + (i + 1),
          api_tokens: [i === 0 ? 5200 : 500, i === 0 ? 0 : 4700, 0, 82],
          ledger_tokens: [i === 0 ? 5200 : 500, i === 0 ? 0 : 4700, 0, 82],
          billed: 0.004,
          official: 0.005,
          first_token_ms: 720,
          match_kind: "request-id",
          problem: null,
        })),
        balance_before: 12.5,
        balance_after: 12.476,
        balance_delta: 0.024,
        currency: "USD",
        total_billed: 0.024,
        official_cost: 0.03,
        prefix_reuse: 0.904,
      },
    },
  }),
  station_billing_connect: (args) => ({
    backend: args?.backend === "auto" ? "newapi" : args?.backend,
    user_id: "12",
    configured: true,
    account: args?.account,
    balance: 12.5,
    currency: "USD",
    verified_at: Date.now(),
  }),
  station_billing_browser: () => ({
    backend: "newapi",
    user_id: "12",
    configured: true,
    account: "demo-user",
    balance: 12.5,
    currency: "USD",
    verified_at: Date.now(),
  }),
  station_billing_refresh: () => ({
    backend: "newapi",
    user_id: "12",
    configured: true,
    account: "demo-user",
    balance: 12.5,
    currency: "USD",
    verified_at: Date.now(),
  }),
  station_billing_settings: () => ({
    backend: "none",
    user_id: "",
    configured: false,
  }),
  station_billing_save: (args) => ({
    backend: args?.backend ?? "none",
    user_id: args?.userId ?? "",
    configured: args?.backend !== "none",
  }),
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
  // 真实浏览器采集（2026-09-24）：演示里不开任何端口，直接给一份编好的报告。
  browser_probe_start: () => ({
    url: "http://127.0.0.1:53123/0123456789abcdef0123456789abcdef/",
    token: "demo-probe",
    opened: true,
  }),
  browser_probe_wait: () => {
    demoProbeReport = demoBrowserReport();
    return demoProbeReport;
  },
  browser_probe_last: () => demoProbeReport,
  accounts_list: () => accounts,
  accounts_tokens: () => tokenUsage,
  accounts_token_summary: (args) => summaryFor(Number(args?.days ?? 1)),
  accounts_usage_overview: (args) =>
    demoUsageOverview(String(args?.label ?? ""), Number(args?.days ?? 7)),
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
  update_check: (args) => {
    if (args?.manual)
      update = {
        ...update,
        latest: demoRelease,
        update_available: true,
        skipped: update.skipped,
        checked_at: new Date().toISOString(),
        error: null,
      };
    return update;
  },
  update_skip: (args) => {
    update = {
      ...update,
      skipped: !!args?.version && args.version === update.latest?.version,
    };
    return update;
  },
  update_install: () => {
    throw new Error(
      "演示模式不会真的下载或安装：装好的面板里，这一步会从 GitHub 下载安装包、核对 SHA-256，然后退出并安装。",
    );
  },
  killswitch_preview: () => kill,
  plugin_list: () => plugins,
  plugin_catalog_status: () => catalog,
  tavern_config: () => tavern,
  tavern_gpt_status: () => gptBridge,
  tavern_gpt_token: () => "demo-token-not-a-real-secret",
  tavern_gpt_quota: (args) =>
    askedOnly("gpt:active", args?.refresh, () =>
      gptQuotaFixture("demo@example.test"),
    ),
  tavern_gemini_status: () => geminiBridge,
  tavern_gemini_token: () => "demo-token-not-a-real-secret",
  tavern_gemini_quota: (args) =>
    askedOnly("gemini:active", args?.refresh, () => ({
      provider: "gemini",
      account: "demo@example.test",
      project: "demo-project",
      tier: "Google AI Pro",
      models: [
        {
          model_id: "gemini-2.5-pro",
          label: "gemini-2.5-pro",
          token_type: "REQUESTS",
          remaining_percent: 62,
          reset_at: "2026-09-22 00:00",
          reset_epoch: Math.floor(Date.now() / 1000) + 36000,
        },
        {
          model_id: "gemini-2.5-flash",
          label: "gemini-2.5-flash",
          token_type: "REQUESTS",
          remaining_percent: 84,
          reset_at: "2026-09-22 00:00",
          reset_epoch: Math.floor(Date.now() / 1000) + 36000,
        },
      ],
      fetched_at: new Date().toISOString(),
      source:
        "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota",
    })),
  tavern_bridge_roleplay_test: (args) => ({
    provider: String(args?.provider ?? "gpt"),
    ok: true,
    model:
      String(args?.provider ?? "gpt") === "gemini"
        ? "gemini-2.5-pro"
        : "codex-default",
    reply: "（演示）林澈微笑着指向星海区：我想起一本很适合你的书。",
    latency_ms: 842,
    input_tokens: 42,
    output_tokens: 18,
    detail: "演示模式：桥接角色扮演测试通过",
    tested_at: new Date().toISOString(),
  }),
  antigravity_ui_status: () => antigravityUiStatus(),
  antigravity_ui_start: () => {
    antigravityUiRunning = true;
    return antigravityUiStatus();
  },
  antigravity_ui_stop: () => {
    antigravityUiRunning = false;
    return null;
  },
  antigravity_ui_config_save: (args) => {
    Object.assign(antigravityUiConfig, (args?.cfg as UiConfig) ?? {});
    return null;
  },
  antigravity_ui_rules: () => structuredClone(antigravityRules),
  antigravity_ui_rules_save: (args) => {
    const next = args?.rules as DangerRules | undefined;
    if (next) antigravityRules.rules = structuredClone(next.rules);
    return null;
  },
  antigravity_ui_rules_reset: () => structuredClone(antigravityRules),
  antigravity_status: () => antigravityStatus(),
  antigravity_launch: (args) => {
    antigravityLive = { ...antigravityLive, [String(args?.product)]: true };
    if (args?.product === "hub") antigravityUiRunning = true;
    return null;
  },
  antigravity_close: (args) => {
    const was = antigravityLive[String(args?.product)] ? 1 : 0;
    antigravityLive = { ...antigravityLive, [String(args?.product)]: false };
    if (args?.product === "hub") antigravityUiRunning = false;
    return was;
  },
  // 启动前那次「有没有在跑」的只读查询（0.27.0）。真机上按安装目录数进程，
  // 演示里就是这个状态位。
  antigravity_running: (args) =>
    antigravityLive[String(args?.product)] ? 1 : 0,
  antigravity_auto_update_set: () => null,
  // 一键安装（0.29.0）：演示里不联网、不装包，只回一句结论。
  antigravity_install: (args) =>
    args?.product === "ide"
      ? "演示：反重力 IDE 2.5.5 已装好（winget）。"
      : "演示：反重力 2.15.1 已装好（winget）。",
  antigravity_latest: (args) => (args?.product === "ide" ? "2.5.5" : "2.15.1"),
  // 反重力账户槽位与本机用量（0.32.0：一条 = 一个账户，两半）。
  antigravity_ide_create: (args) => {
    const id = `ag-demo-${agAccounts.length}`;
    agAccounts.push({
      id,
      label: String(args?.label),
      active: agAccounts.length === 0,
      ide_dir: `${HOME}\\AppData\\Local\\ClaudeIpGate\\antigravity-accounts\\${id}\\ide-user-data`,
      ide_logged_in: false,
      ide_auth_state: "未登录 · 还没在这个账户起过 IDE",
      email: null,
      tier: null,
      identity_error: null,
      quota: [],
      written_at: null,
      cli_dir: `${HOME}\\AppData\\Local\\ClaudeIpGate\\antigravity-accounts\\${id}\\cli-home`,
      cli_logged_in: false,
      cli_auth_state: "未登录",
    });
    return id;
  },
  antigravity_ide_select: (args) => {
    for (const s of agAccounts) s.active = s.id === args?.id;
    return null;
  },
  antigravity_ide_archive: (args) => {
    const i = agAccounts.findIndex((s) => s.id === args?.id);
    if (i >= 0) agAccounts.splice(i, 1);
    return null;
  },
  antigravity_account_rename: (args) => {
    const s = agAccounts.find((x) => x.id === args?.id);
    if (s) s.label = String(args?.label);
    return null;
  },
  // 只改索引：把 `from` 那一半挂到 `into` 上，再把 `from` 从清单里去掉。
  antigravity_account_attach: (args) => {
    const into = agAccounts.find((x) => x.id === args?.into);
    const i = agAccounts.findIndex((x) => x.id === args?.from);
    if (into && i >= 0) {
      const from = agAccounts[i];
      if (!into.ide_dir && from.ide_dir) {
        into.ide_dir = from.ide_dir;
        into.ide_logged_in = from.ide_logged_in;
        into.ide_auth_state = from.ide_auth_state;
        into.email = from.email;
        into.tier = from.tier;
      }
      if (!into.cli_dir && from.cli_dir) {
        into.cli_dir = from.cli_dir;
        into.cli_logged_in = from.cli_logged_in;
        into.cli_auth_state = from.cli_auth_state;
      }
      agAccounts.splice(i, 1);
    }
    return null;
  },
  antigravity_usage: (args) => antigravityUsage(Number(args?.days ?? 1)),
  // 演示里不联网。给的是「问到了」那一档 —— 真实机器上它可能报「令牌换新失败」之类，
  // 那些话在 `crates/qb-app/src/usecase/antigravity_quota.rs` 里。
  // 跟真的一样只手动刷新：`refresh` 为假时只回问过的那份，没问过就是 null。
  antigravity_hub_quota: (args) =>
    askedOnly("ag:hub", args?.refresh, () => agOnlineQuota(true)),
  antigravity_account_quota: (args) =>
    askedOnly(`ag:${String(args?.id)}`, args?.refresh, () =>
      agOnlineQuota(args?.id === "ag-demo-0"),
    ),
  gemini_login: (args) => {
    const s = agAccounts.find((x) => x.id === args?.id);
    if (s) {
      s.cli_dir ??= `${HOME}\\AppData\\Local\\ClaudeIpGate\\antigravity-accounts\\${s.id}\\cli-home`;
      s.cli_logged_in = true;
      s.cli_auth_state = "已登录 · 本地凭据（由 Gemini CLI 自己保管）";
    }
    return null;
  },
  // 0.29.0 起它是个长任务、回一句结论（原来只是「弹个窗口」回 null）。
  gemini_cli_install: () => "演示：Gemini CLI 0.9.1 已装好（npm 全局）。",
  // Claude 桥的设置与监控（0.32.0）。演示里给的是「桥在跑」那一档 ——
  // 真机上它常常是「没在跑」，那时界面显示的是后端那句能照着做的话。
  tavern_bridge_health: () => ({
    bridge_version: "2.4.0",
    invocation_mode: "cli",
    model: "claude-opus-5",
    effort: "max",
    cache_ttl: "1h",
    sdk_sessions: 1,
  }),
  tavern_bridge_settings: () => ({
    settings: {
      priority_prompt: "你是一个演示用的角色。",
      tail_prompt: "",
      model: "claude-opus-5",
      effort: "max",
      cache_ttl: "1h",
      system_prompt_mode: "append",
      invocation_mode: "cli",
    },
    models: ["claude-opus-5", "claude-sonnet-5", "claude-haiku-4-5"],
    efforts: ["low", "medium", "high", "xhigh", "max"],
    cache_ttls: ["5m", "1h"],
    prompt_modes: ["append", "replace"],
    invocation_modes: ["cli", "agent_sdk"],
  }),
  tavern_bridge_settings_save: () => null,
  tavern_bridge_telemetry: () => ({
    records: [
      {
        id: "demo-1",
        at: "09-21 08:12:03",
        status: "ok",
        mode: "cli",
        model: "claude-opus-5",
        effort: "max",
        input: 1820,
        output: 640,
        cache_read: 41200,
        cache_write: 0,
        prefix: "append_only",
        elapsed_ms: 8400,
      },
      {
        id: "demo-2",
        at: "09-21 08:09:51",
        status: "ok",
        mode: "cli",
        model: "claude-opus-5",
        effort: "max",
        input: 1760,
        output: 512,
        cache_read: 39800,
        cache_write: 2100,
        prefix: "first",
        elapsed_ms: 11200,
      },
    ],
    total: 2,
    offset: 0,
    has_more: false,
    hit_rate: 0.94,
    cache_read: 81000,
    cache_write: 2100,
    countable: 2,
    prefix_reusable: 0.5,
  }),
  tavern_bridge_telemetry_clear: () => 2,
  tavern_locate: () => survey,
  tavern_assets: () => assets,
  tavern_backups: () => backups,
};

/** Only a missing fixture should fall through to the other demo dispatcher. */
export class MissingDemoCommand extends Error {}

// ------------------------------------------------ 联网额度的演示（2026-09-23）
//
// 真实面板里联网额度只手动刷新：`refresh: false` 只回「最近一次」问到的，没问过就是 null。
// 演示照同一条规矩走，`test:ui` 才测得出「打开页面不显示在线、点了刷新才有」。
const demoAsked = new Map<string, unknown>();
function askedOnly<T>(key: string, refresh: unknown, make: () => T): T | null {
  if (refresh === true) {
    const value = make();
    demoAsked.set(key, value);
    return value;
  }
  return (demoAsked.get(key) as T | undefined) ?? null;
}

/** 本地 `YYYY-MM-DD HH:MM`，跟后端给的格式一样。 */
function demoMinute(epochSec: number): string {
  const d = new Date(epochSec * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

function gptQuotaFixture(account: string) {
  const now = Math.floor(Date.now() / 1000);
  return {
    provider: "gpt",
    account,
    plan_type: "plus",
    windows: [
      {
        label: "5 小时",
        remaining_percent: 71,
        reset_at: demoMinute(now + 7200),
        reset_epoch: now + 7200,
        window_minutes: 300,
        present: true,
      },
      {
        label: "7 天",
        remaining_percent: 23,
        reset_at: demoMinute(now + 360000),
        reset_epoch: now + 360000,
        window_minutes: 10080,
        present: true,
      },
    ],
    fetched_at: demoMinute(now) + ":00",
    source: "https://chatgpt.com/backend-api/wham/usage",
  };
}

/**
 * 反重力的联网额度。付费档是四格 + AI 积分（Gemini 周窗口用掉 5%，跟使用者截图里那台一样）；
 * 免费档 Google 不给四格，只有按模型的。
 */
function agOnlineQuota(paid: boolean) {
  const now = Math.floor(Date.now() / 1000);
  const win = (
    group: "claude" | "gemini",
    span: "five-hour" | "weekly",
    remaining: number,
    resetIn: number,
    note: string | null = null,
  ) => ({
    group,
    span,
    remaining,
    remaining_implied: false,
    reset_epoch: now + resetIn,
    reset_at: demoMinute(now + resetIn),
    note,
  });
  if (!paid)
    return {
      tier_id: "free-tier",
      tier_name: "Antigravity Starter Quota",
      gcp_tos: false,
      credits: null,
      windows: [],
      windows_note:
        "免费档：Google 不给免费档 5 小时 / 每周的汇总（HTTP 403），下面按模型显示。",
      models: [
        {
          label: "Gemini 3.8 Flash (High)",
          model_id: 0,
          remaining: 0.8,
          reset_at: demoMinute(now + 5 * 86400),
          reset_epoch: now + 5 * 86400,
          tags: ["Fast"],
        },
        {
          label: "Claude Sonnet 4.6",
          model_id: 0,
          remaining: 1,
          reset_at: demoMinute(now + 5 * 86400),
          reset_epoch: now + 5 * 86400,
          tags: [],
        },
      ],
      fetched_at: demoMinute(now),
    };
  return {
    tier_id: "g1-pro-tier",
    tier_name: "Google AI Pro",
    gcp_tos: false,
    credits: 1000,
    windows: [
      win("claude", "five-hour", 1, 5 * 3600 - 300),
      win("claude", "weekly", 1, 7 * 86400 - 300),
      win("gemini", "five-hour", 1, 5 * 3600 - 300),
      win("gemini", "weekly", 0.95, 3 * 86400 + 660, "部分已用"),
    ],
    windows_note: null,
    models: [],
    fetched_at: demoMinute(now),
  };
}

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
// 一份**不是面板起的** Codex（开始菜单 / `codex://` 链接 / 别的多开工具，默认资料）。
// 起槽位、切槽位都不动它（2026-09-23）；只有使用者点「一键关闭」才一起关。
let codexForeign = true;
function codexDemo(cmd: string, args: Record<string, unknown>) {
  if (cmd === "codex_accounts")
    return {
      slots: structuredClone(codexSlots),
      launched_id: codexLaunched,
      launched_pid: codexLaunched ? 100 : null,
      launched_at: codexLaunched ? "demo-start" : null,
    };
  if (cmd === "codex_desktop_status") {
    const processes = [
      ...(codexLaunched
        ? [
            {
              pid: 100,
              started: "demo-start",
              profile: `C:/Users/demo/AppData/Local/ClaudeIpGate/codex-accounts/${codexLaunched}/desktop`,
              ours: true,
            },
          ]
        : []),
      ...(codexForeign
        ? [{ pid: 200, started: "demo-other", profile: null, ours: false }]
        : []),
    ];
    return {
      executable: "C:/Demo/Codex/ChatGPT.exe",
      version: "26.9.0",
      running: processes.length > 0,
      processes,
    };
  }
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
      buckets: [],
      long_context: [],
    };
  // 用量明细页的 GPT 一侧（2026-09-24）：按天、按模型、美元。
  if (cmd === "codex_usage_summary")
    return demoCodexSummary(Number(args?.days ?? 7));
  // 额度窗口（0.32.0）：Codex 自己写在会话记录里的，零网络。
  // 演示里故意让**七天那个更紧**（剩 23% vs 剩 71%）—— 那正是「只看宽裕的
  // 那半做判断」会出错的形状，界面得把「卡这儿」标在七天那根上。
  if (cmd === "codex_rate_limits")
    return {
      files_examined: 2,
      files_failed: 0,
      first_error: null,
      found: {
        source_file: "C:/Demo/.codex/sessions/2026/09/21/rollout-demo.jsonl",
        measured_at: new Date(Date.now() - 42 * 60_000).toISOString(),
        age_minutes: 42,
        primary: {
          name: "5 小时",
          window_minutes: 300,
          window: {
            used: 29,
            resets_at: new Date(Date.now() + 2.4 * 3600_000).toISOString(),
            estimated: false,
          },
        },
        secondary: {
          name: "7 天",
          window_minutes: 10080,
          window: {
            used: 77,
            resets_at: new Date(Date.now() + 3.2 * 86400_000).toISOString(),
            estimated: false,
          },
        },
        plan_type: null,
        credits: { has_credits: false, unlimited: false, balance: null },
      },
    };
  // 官方额度接口（`wham/usage`）的演示：只在点了那一行的刷新图标（`refresh: true`）时才有。
  if (cmd === "codex_quota") {
    const slot = codexSlots.find((s) => s.id === args.id);
    return askedOnly(`codex:${String(args.id)}`, args.refresh, () =>
      gptQuotaFixture(slot?.email ?? String(args.id)),
    );
  }
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
    // 「一键关闭」是全关：别处起的那份也一起。
    codexLaunched = null;
    codexForeign = false;
    return null;
  }
  if (cmd === "codex_repair_registration") {
    // 演示里没有真实的打包应用注册，直接当作修好了。
    return null;
  }
  // 直装（0.28.0）：演示里不联网、不装包，只回一句结果；版本查询回一个比本机新的。
  if (cmd === "codex_desktop_install")
    return "演示：Codex 桌面端 26.9.1 已装好（winget · Store 源）。";
  if (cmd === "codex_desktop_latest") return "26.9.1";
  // Codex 出站与换出口插件（外部程序）。演示里只翻状态位，不起任何进程。
  if (cmd === "codex_egress_status") {
    const base = plugins.find((p) => p.id === "codex-egress")!;
    return {
      ...base,
      state: egressRunning ? "running" : "ready",
      detail: egressRunning
        ? "运行中，面板：http://127.0.0.1:17841/admin/"
        : "已安装，没在跑。",
      checks: base.checks.map((c) =>
        c.label === "端口 17841"
          ? {
              ...c,
              detail: egressRunning ? "本面板启动的服务正在监听" : "空闲",
            }
          : c,
      ),
    };
  }
  if (cmd === "codex_egress_config") return { exe: egressExe };
  if (cmd === "codex_egress_config_save") {
    egressExe = (args.cfg as { exe: string | null } | undefined)?.exe ?? null;
    return null;
  }
  if (cmd === "codex_egress_start") {
    if (stationTurnstateArmed || stationTurnstateTakeover)
      throw new Error(
        "账户页的「识别（turn-state）」还开着。它和这个插件都要改同一个 Codex 槽位的配置，一次只能开一个：到账户页「识别」弹窗点「关闭识别」（会恢复槽位配置），再启动插件。",
      );
    egressRunning = true;
    return "http://127.0.0.1:17841/admin/";
  }
  if (cmd === "codex_egress_stop") {
    egressRunning = false;
    return ["已结束插件进程", "已用它自己的 restore 恢复 Codex 配置"];
  }
  if (cmd === "codex_egress_install") {
    // 演示里不真下载；给一个像样的结果，让界面能演到「已登记」。
    egressExe = `${HOME}\\AppData\\Local\\Programs\\ccodex-sleep-state\\ccodex-sleep-state-windows-amd64\\ccodex-sleep-state.exe`;
    return {
      tag: "v0.1.0",
      exe: egressExe,
      sha256:
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    };
  }
  throw new MissingDemoCommand(`Unknown Codex fixture: ${cmd}`);
}
