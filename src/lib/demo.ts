import { demoWorkspaceCall } from "./workspaceDemo";
import { makeTavernFixtures } from "./demoTavern";
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
import type { ClaudeZhLeftovers } from "./generated/ClaudeZhLeftovers";
import type { ClaudeZhOutcome } from "./generated/ClaudeZhOutcome";
import type { CodexLocaleOutcome } from "./generated/CodexLocaleOutcome";
import type { CodexLocaleStatus } from "./generated/CodexLocaleStatus";
import type { BreakerView } from "./generated/BreakerView";
import type { DispatchPin } from "./generated/DispatchPin";
import type { DispatchRow } from "./generated/DispatchRow";
import type { DispatchTier } from "./generated/DispatchTier";
import type { LineLiveness } from "./generated/LineLiveness";
import type { ProbeAttempt } from "./generated/ProbeAttempt";
import type { ProbeSettings } from "./generated/ProbeSettings";
import type { SchedulerAction } from "./generated/SchedulerAction";
import type { RouterRequest } from "./generated/RouterRequest";
import type { ListenScope } from "./generated/ListenScope";
import type { RouterNetworkView } from "./generated/RouterNetworkView";
import type { RouterSettings } from "./generated/RouterSettings";
import type { StationEndpoints } from "./generated/StationEndpoints";
import type { CoexistStatus } from "./generated/CoexistStatus";
import type { CoexistTool } from "./generated/CoexistTool";
import type { McpServer } from "./generated/McpServer";
import type { InstalledSkill } from "./generated/InstalledSkill";
import type { Prompt } from "./generated/Prompt";
import type { SkillCandidate } from "./generated/SkillCandidate";
import type { OnboardingTarget } from "./generated/OnboardingTarget";
import type { EditorLink } from "./generated/EditorLink";
import type { CodexHistoryHome } from "./generated/CodexHistoryHome";
import type { Project } from "./generated/Project";
import type { ProjectsView } from "./generated/ProjectsView";
import type { DeepLinkEntry } from "./generated/DeepLinkEntry";
import type { DeepLinkView } from "./generated/DeepLinkView";
import type { ClaudeQuota } from "./generated/ClaudeQuota";
import type { SiteQuota } from "./generated/SiteQuota";
import type { UsageScript } from "./generated/UsageScript";
import type { RelayUsageReport } from "./generated/RelayUsageReport";
import type { RelayUsageTotals } from "./generated/RelayUsageTotals";
import type { RelayUsageGroup } from "./generated/RelayUsageGroup";
import type { RelayRequestPage } from "./generated/RelayRequestPage";
import type { SessionSummary } from "./generated/SessionSummary";
import type { SessionDetail } from "./generated/SessionDetail";
import type { TrashEntry } from "./generated/TrashEntry";
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
  PluginStatus,
  Preset,
  Progress,
  ProxyState,
  PurityCriteria,
  Settings,
  SnapshotEntry,
  SoftwareReport,
  TavernConfig,
  TavernRelease,
  TavernSetupStatus,
  TavernSurvey,
  TavernUninstalled,
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
      login_problem: null,
      // 当前登录的那个：桌面端有实时样本，五小时窗口的恢复时刻是推算的。
      usage: {
        source: "desktop",
        measured_at: new Date(Date.now() - 6 * 60_000).toISOString(),
        age_minutes: 6,
        five_hour: {
          used: 62,
          resets_at: new Date(Date.now() + 97 * 60_000).toISOString(),
          estimated: true,
          reset_passed: false,
        },
        seven_day: {
          used: 79,
          resets_at: new Date(Date.now() + 3 * 86_400_000).toISOString(),
          estimated: false,
          reset_passed: false,
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
      login_problem: null,
      // 没登录的槽位不可能有实时数据 —— 物理限制。只有 Claude Code 留下的快照，
      // 而且是十天前的：`resets_at` 早过去了，所以两个窗口都没有恢复时刻。
      usage: {
        source: "cache",
        measured_at: "2026-09-02T18:07:05.310Z",
        age_minutes: 14_400,
        five_hour: {
          used: 25,
          resets_at: null,
          estimated: false,
          reset_passed: false,
        },
        seven_day: {
          used: 13,
          resets_at: null,
          estimated: false,
          reset_passed: false,
        },
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
      login_problem: null,
      // 两源都没有 —— 什么都不显示，不给假读数。
      usage: null,
    },
  ],
  caveat:
    "剩余天数只读本地时间戳，查不出「被风控下线」。唯一能确认的办法是实际发一次认证请求。",
  planCaveat:
    "套餐读自槽位目录里的 .claude.json（官方客户端写下的档案缓存），不发任何网络请求，可能过期。",
  desktop: { managed: true, active: "demo-main" },
  bridgePresent: true,
  slotsError: null,
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
  // 2026-09-25：GPT 界面语言的选择，照 Rust 的默认值关着（只经「汉化」弹窗改）。
  gpt_ui_zh: false,
  // 2026-10-27（阶段 7 · X7）：两个后台开关照 Rust 的默认值关着。
  lightweight_on_close: false,
  start_router_on_launch: false,
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

// 阶段 4 起预设多了 cc-switch 那份带的几项（分类、拿 Key 的地址、各档模型名……）。演示里第一条带着各档模型名，
// 点了它模型映射会被预填 —— 跟真后端那几十条一个形状。
const PRESET_EXTRAS = {
  api_key_url: null,
  api_format: null,
  models: {
    default: null,
    haiku: null,
    sonnet: null,
    opus: null,
    fable: null,
    subagent: null,
  },
  supports_1m: false,
  catalog: [],
  endpoint_candidates: [],
  models_url: null,
  needs_edit: false,
} satisfies Partial<Preset>;
const presets: Preset[] = [
  {
    id: "claude-code/demo-preset",
    name: "示例预设",
    target: "claude-code",
    base_url: "https://api.example.com/anthropic",
    wire_api: "responses",
    auth_style: "bearer_token",
    model: "example-pro",
    website: "https://example.com",
    note: "示例数据，不是任何真实服务。",
    ...PRESET_EXTRAS,
    category: "cn_official",
    api_key_url: "https://example.com/keys",
    api_format: "anthropic",
    models: {
      ...PRESET_EXTRAS.models,
      default: "example-pro",
      sonnet: "example-pro",
      opus: "example-pro",
      haiku: "example-flash",
    },
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
    ...PRESET_EXTRAS,
    category: "own",
  },
];

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
    // 标签照 `plugins::sillytavern::status_with` 一字不差 —— 账户页按这几个标签分
    // 「酒馆本体缺东西（去软件页）」与「桥缺东西（开弹窗）」（`lib/tavernReady.ts`）。
    checks: [
      {
        label: "桥接项目",
        ok: true,
        detail: `${HOME}\\SillyTavern\\bridge`,
      },
      { label: "SillyTavern", ok: true, detail: `${HOME}\\SillyTavern` },
      {
        label: "酒馆启动脚本",
        ok: true,
        detail: `${HOME}\\SillyTavern\\start-sillytavern.cmd`,
      },
      {
        label: "Node.js",
        ok: true,
        detail: "C:\\Program Files\\nodejs\\node.exe",
      },
      {
        label: "Python 3",
        ok: true,
        detail: `${HOME}\\AppData\\Local\\Programs\\Python\\Python312\\python.exe`,
      },
      {
        label: "官方 Claude Code",
        ok: true,
        detail: `${HOME}\\.local\\bin\\claude.exe`,
      },
      {
        label: "官方 Codex CLI（GPT 桥接用）",
        ok: true,
        detail: `${HOME}\\AppData\\Roaming\\npm\\node_modules\\@openai\\codex\\node_modules\\@openai\\codex-win32-x64\\vendor\\x86_64-pc-windows-msvc\\bin\\codex.exe`,
      },
      { label: "Claude 桥接端口 5001", ok: true, detail: "空闲" },
      { label: "GPT 桥接端口 5002", ok: true, detail: "空闲" },
      { label: "Gemini 桥接端口 5003", ok: true, detail: "空闲" },
      { label: "酒馆端口 8000", ok: true, detail: "空闲" },
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

/**
 * Claude 汉化残留的演示（2026-09-27：Claude 汉化撤掉了，只剩清理）。
 *
 * **默认没有残留** —— 账户页那颗「清理汉化残留」只在有残留时出现，而 README 的截图就是演示模式拍的，
 * 不该让每张截图都挂着它。`test:ui` 走清理流程之前把 `qb-demo-claude-zh-leftovers` 设成 `1`
 * 再刷新（演示专用的开关，真实面板不读它）；点「清理」之后回到「没有残留」。
 */
let claudeZhCleaned = false;
function demoLeftoversOn(): boolean {
  try {
    return localStorage.getItem("qb-demo-claude-zh-leftovers") === "1";
  } catch {
    return false;
  }
}
const claudeZhLeftovers = (): ClaudeZhLeftovers => {
  const none: ClaudeZhLeftovers = {
    empty: true,
    apps: [],
    zh_profiles: [],
    stray_3p: null,
    panel_copy: null,
    state_file: null,
    needs_close: false,
    desktop_running: 1,
  };
  if (claudeZhCleaned || !demoLeftoversOn()) return none;
  return {
    ...none,
    empty: false,
    apps: [
      {
        version: "2.9939.2",
        dir: `${HOME}\\AppData\\Local\\AnthropicClaude\\app-2.9939.2`,
        current: true,
        zh_files: 0,
        backup_mb: 107,
        changed: 3,
        restorable: true,
      },
    ],
    stray_3p: `${HOME}\\AppData\\Roaming\\Claude-3p`,
    panel_copy: `${HOME}\\AppData\\Local\\ClaudeIpGate\\plugins\\claude-desktop-zh-cn`,
    state_file: `${HOME}\\AppData\\Local\\ClaudeIpGate\\claude-zh\\state.json`,
    needs_close: true,
  };
};
const claudeZhCleanup = (): ClaudeZhOutcome => {
  const had = claudeZhLeftovers();
  claudeZhCleaned = true;
  if (had.empty)
    return { ok: true, closed_claude: false, lines: ["没有残留，不用清。"] };
  return {
    ok: true,
    closed_claude: true,
    lines: [
      "先关掉了 Claude 桌面端（1 个进程）。",
      "Claude 2.9939.2：3 个前端文件换回了原版（上游改过的，也包括它卸载时删多了的）。",
      "Claude 2.9939.2：删掉了上游留下的备份（107 MB）。",
      `删掉了上游建的 ${HOME}\\AppData\\Roaming\\Claude-3p（里面只有一个写了界面语言的 config.json）。`,
      `删掉了 ${HOME}\\AppData\\Local\\ClaudeIpGate\\plugins\\claude-desktop-zh-cn。`,
      `删掉了 ${HOME}\\AppData\\Local\\ClaudeIpGate\\claude-zh\\state.json。`,
    ],
  };
};
/**
 * 软件页 SillyTavern 卡的演示（2026-09-29）。
 *
 * 默认「你自己的那份 1.18.0、Node 够用」—— README 的截图拍的就是这一档。演示专用的两个开关
 * （真实面板不读它们）：`qb-demo-tavern-no-node` = `1` → 本机没有 Node（安装框里露出 winget 勾选）；
 * `qb-demo-tavern-managed` = `1` → 一开始就是面板装的 1.18.0（GitHub 上最新 1.18.1，能更新）。
 * 点「下载安装」/「更新」之后是面板装的 1.18.1，「卸载」回到你自己的那份。
 */
let demoTavern: {
  managed: boolean;
  version: string;
  previous: string | null;
} | null = null;
function demoFlag(key: string): boolean {
  try {
    return localStorage.getItem(key) === "1";
  } catch {
    return false;
  }
}
function demoTavernState() {
  return (
    demoTavern ?? {
      managed: demoFlag("qb-demo-tavern-managed"),
      version: "1.18.0",
      previous: null,
    }
  );
}
const DEMO_ST_MANAGED = `${HOME}\\AppData\\Local\\ClaudeIpGate\\apps\\sillytavern`;
const DEMO_ST_LATEST = "1.18.1";
const tavernSetupStatus = (): TavernSetupStatus => {
  const st = demoTavernState();
  const noNode = demoFlag("qb-demo-tavern-no-node");
  return {
    root: st.managed ? DEMO_ST_MANAGED : tavern.sillytavern_root,
    launcher: st.managed ? `${DEMO_ST_MANAGED}\\Start.bat` : tavern.st_launcher,
    present: true,
    version: st.version,
    node_engine: ">= 20",
    managed: st.managed
      ? {
          version: st.version,
          tag: st.version,
          installed_at: "2026-09-29T08:00:00Z",
          archive_sha256: "0".repeat(64),
          source: `https://github.com/SillyTavern/SillyTavern/archive/refs/tags/${st.version}.zip`,
        }
      : null,
    managed_dir: DEMO_ST_MANAGED,
    previous_version: st.previous,
    node: noNode
      ? { path: null, version: null }
      : { path: "C:\\Program Files\\nodejs\\node.exe", version: "v24.14.1" },
    node_ok: !noNode,
    winget: true,
    config_port: 8000,
    st_port: tavern.st_port,
    running: false,
    foreign_running: false,
  };
};

/** 演示里的出站插件运行态：启动翻真、停止翻假，状态随之变。 */
let egressRunning = false;
let egressExe: string | null = null;

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
    ide_unreadable: false,
    email: "someone@example.com",
    tier: "Google AI Pro",
    identity_error: null,
    quota: [],
    written_at: "2026-09-21 03:35",
    cli_dir: `${HOME}\\AppData\\Local\\ClaudeIpGate\\antigravity-accounts\\ag-demo-0\\cli-home`,
    cli_logged_in: true,
    cli_auth_state: "已登录 · 本地凭据（由 Gemini CLI 自己保管）",
    cli_unreadable: false,
  },
  {
    id: "ag-demo-1",
    label: "工作账户",
    active: false,
    ide_dir: `${HOME}\\AppData\\Local\\ClaudeIpGate\\antigravity-ide-accounts\\ag-ide-demo-1\\user-data`,
    ide_logged_in: false,
    ide_auth_state: "未登录 · 还没在这个账户起过 IDE",
    ide_unreadable: false,
    email: null,
    tier: null,
    identity_error: null,
    quota: [],
    written_at: null,
    cli_dir: null,
    cli_logged_in: false,
    cli_auth_state: "还没有 CLI 那一半",
    cli_unreadable: false,
  },
  {
    id: "ag-demo-2",
    label: "个人 Google",
    active: false,
    ide_dir: null,
    ide_logged_in: false,
    ide_auth_state: "还没有 IDE 那一半",
    ide_unreadable: false,
    email: null,
    tier: null,
    identity_error: null,
    quota: [],
    written_at: null,
    cli_dir: `${HOME}\\AppData\\Local\\ClaudeIpGate\\gemini-accounts\\gemini-demo-1\\home`,
    cli_logged_in: false,
    cli_auth_state: "未登录",
    cli_unreadable: false,
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
    remaining_implied: false,
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
    // example 是 New API 系：倍率是单价（1 = $2 / 百万），照官方价抄的 Opus 5 是 2.5。
    // 折扣在分组上（×0.2）。⛔ 补全倍率官方本来就是 5 —— 这家填了 6，输出另外多收两成，
    // 线路上挂「输出加价 ×1.20」；隔壁桌面端那条照官方填 5，什么都不挂 —— 有对照才看得出意义。
    rates: {
      model_ratio: 2.5,
      completion_ratio: 6,
      cache_ratio: 0.1,
      create_cache_ratio: 1.25,
      group_ratio: 0.2,
      peak_rate: null,
      per_request_price: null,
      input_price: null,
      cache_read_price: null,
      cache_write_price: null,
      output_price: null,
    },
    output_markup: 1.2,
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
    // 高倍组：单价照官方（2.5 / 补全 5），分组 ×2.5。
    rates: {
      model_ratio: 2.5,
      completion_ratio: 5,
      cache_ratio: 0.1,
      create_cache_ratio: 1.25,
      group_ratio: 2.5,
      peak_rate: null,
      per_request_price: null,
      input_price: null,
      cache_read_price: null,
      cache_write_price: null,
      output_price: null,
    },
    // 从没检验过 —— 这组价是手上一次留下的，模型名也就无从谈起，输出加没加价也就不知道。
    output_markup: null,
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
    // 输出 ÷ 输入跟官方一样是 5 —— 没另外加价。
    output_markup: 1,
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
      model_ratio: 2.5,
      completion_ratio: 5,
      cache_ratio: 0.1,
      create_cache_ratio: 1.25,
      group_ratio: 0.35,
      peak_rate: null,
      per_request_price: null,
      input_price: null,
      cache_read_price: null,
      cache_write_price: null,
      output_price: null,
    },
    // 补全倍率照官方填 5 —— 不挂「输出加价」。0.25.3 及以前这里会挂「翻倍 ×5」。
    output_markup: 1,
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
    // 协议转换（阶段 3）的演示：这个分组只说 OpenAI Chat，桌面端那头说 Anthropic ——
    // 线路标成 Chat，本机路由两头转换，线路行上挂「转换 → OpenAI Chat」。
    protocols: { anthropic: false, openai_chat: true, openai_responses: null },
    upstream_format: "openai_chat",
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
    output_markup: null,
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
    // gpt-5 不在官方价目快照里 —— 输出加没加价无从比，不是「没加价」。
    output_markup: null,
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
 * 本机路由每个软件一份的设置（`RouterSettings::for_client` 的默认值，两边写同一组数）。
 * 存了就真的存进这里 —— 只回固定值的话，改完点保存弹回旧值，看起来像没存上。
 */
const routerSettingsOf = (client: string): RouterSettings => {
  // Grok Build 跟 Codex 同一套（阶段 4，见 `RouterSettings::for_client`）。
  const codex = client === "codex" || client === "grok-build";
  return {
    client: client as RouterSettings["client"],
    auto_failover: true,
    max_attempts: codex ? 3 : 6,
    first_byte_timeout_secs: codex ? 60 : 90,
    idle_timeout_secs: codex ? 300 : 180,
    non_streaming_timeout_secs: 600,
    breaker: {
      trip_after_consecutive: 5,
      error_rate: codex ? 0.6 : 0.7,
      min_requests: codex ? 10 : 15,
      half_open_successes: 1,
    },
    rectifier: {
      enabled: true,
      thinking_signature: true,
      thinking_budget: true,
      media_fallback: true,
      media_heuristic: true,
    },
    aggregate: { enabled: false, members: [], models: {}, keys: {} },
  };
};
const stationRouterSettings: Record<string, RouterSettings> = {};
/** 站点的备用地址（阶段 4），按站点各记一份。演示里 example 站自带一个备用地址。 */
const stationEndpoints: Record<string, StationEndpoints> = {
  example: {
    provider_id: "example",
    endpoints: [
      {
        url: "https://backup.example.com",
        added_ms: 1757820000000,
        latency_ms: null,
        status: null,
        error: null,
        tested_ms: null,
      },
    ],
  },
};
const demoEndpointsOf = (providerId: string): StationEndpoints =>
  (stationEndpoints[providerId] ??= { provider_id: providerId, endpoints: [] });
/** 提示词库（阶段 5）的演示数据：Claude Code 中转环境用着一条。 */
let demoPrompts: Prompt[] = [
  {
    id: "prompt-demo",
    name: "中文回答、先看再改",
    content: "- 用中文回答\n- 改代码之前先读相关文件",
    description: null,
    created_ms: 1761400000000,
    updated_ms: 1761400000000,
  },
];
const demoPromptActive: Record<string, string> = {
  "claude-code:relay": "prompt-demo",
};
const demoPromptsView = () => ({
  prompts: demoPrompts.map((p) => ({ ...p })),
  targets: DEMO_MCP_TARGETS.map((t) => ({
    ...t,
    path: `C:\\Users\\demo\\${t.id}\\CLAUDE.md`,
    active: demoPromptActive[t.id] ?? null,
    chars: demoPromptActive[t.id] ? 24 : null,
    drift: false,
  })),
});
/** 项目（阶段 5 · M4）的演示数据：一个存过 Claude Code 那一组的项目，Claude Code 正用着它。 */
let demoProjects: Project[] = [
  {
    id: "project-demo",
    name: "日常开发",
    slots: {
      "claude-code": {
        route: R_MAIN,
        queue: [R_CHEAP],
        mcp: [],
        skills: [],
        prompt: "prompt-demo",
      },
    },
    updated_ms: 1761400000000,
  },
];
const demoProjectCurrent: Record<string, string> = {
  "claude-code": "project-demo",
};
const demoProjectsView = (): ProjectsView => ({
  projects: [...demoProjects]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((p) => ({ ...p, slots: { ...p.slots } })),
  current: { ...demoProjectCurrent },
});
/** 这个软件现在的样子（演示里只有线路和派单顺序是真有状态的）。 */
const demoProjectSnapshot = (who: string) => ({
  route: stationCurrent[who] ?? null,
  queue: [...(stationDispatch[who] ?? [])],
  mcp: who === "claude-desktop" ? null : [],
  skills: who === "claude-desktop" ? null : [],
  prompt:
    who === "claude-desktop"
      ? null
      : (demoPromptActive[`${who}:relay`] ?? null),
});
/** 导入链接（阶段 5 · X4）的演示数据：粘贴什么都当成这一条站点链接（Key 只有打码的，跟真后端一样）。 */
const DEMO_DEEPLINK_PROVIDER: DeepLinkView = {
  kind: "provider",
  client: "claude-code",
  name: "示例中转",
  endpoints: ["https://relay.example.com", "https://backup.relay.example.com"],
  key_masked: "sk-d…7a1f",
  homepage: "https://relay.example.com",
  notes: null,
  model_map: {
    exact: [],
    haiku: null,
    sonnet: null,
    opus: "claude-opus-demo",
    fable: null,
    subagent: null,
    default: "claude-sonnet-demo",
  },
  upstream_format: null,
  existing_station: null,
  ignored: [
    "用量查询脚本：QB Gate 的自定义用量脚本要到阶段 6 才有，这一部分没导入",
  ],
};
let demoDeepLink: DeepLinkEntry | null = null;
/** 工具文件（阶段 5，C3 / C4 的附带件）的演示数据：三个工具都装着，OpenClaw 写过一天的记忆。 */
const DEMO_TOOL_FIXED: Record<string, [string, string][]> = {
  "open-claw": [
    ["workspace/AGENTS.md", "Agent 操作指令和规则"],
    ["workspace/SOUL.md", "Agent 人格和沟通风格"],
    ["workspace/USER.md", "用户档案和偏好"],
    ["workspace/IDENTITY.md", "Agent 名称和头像"],
    ["workspace/TOOLS.md", "本地工具文档"],
    ["workspace/MEMORY.md", "长期记忆和决策记录"],
    ["workspace/HEARTBEAT.md", "心跳运行清单"],
    ["workspace/BOOTSTRAP.md", "首次运行仪式"],
    ["workspace/BOOT.md", "网关重启清单"],
  ],
  hermes: [
    ["memories/MEMORY.md", "Agent 自己的笔记（会放进它的系统提示词）"],
    ["memories/USER.md", "关于你的档案（同上）"],
  ],
  pi: [
    ["SYSTEM.md", "整份替换 Pi 的系统提示词"],
    ["APPEND_SYSTEM.md", "接在 Pi 的系统提示词后面"],
  ],
};
const demoToolText: Record<string, Record<string, string>> = {
  "open-claw": {
    "workspace/SOUL.md": "说话温和，先听再答。",
    "workspace/memory/2026-10-25.md": "- 帮使用者把中转站的线路理了一遍",
  },
  hermes: { "memories/USER.md": "喜欢简短的回答。" },
  pi: { "prompts/review.md": "审一下这段代码：先说风险，再说改法。" },
};
let demoHermes = {
  memory_enabled: true,
  user_enabled: true,
  memory_limit: 2200,
  user_limit: 1375,
};
const demoToolFile = (tool: string, name: string, label: string | null) => {
  const text = demoToolText[tool]?.[name];
  return {
    name,
    label,
    exists: text != null,
    content: text ?? "",
    revision: text == null ? "missing" : `demo-${text.length}`,
    size: text?.length ?? 0,
    modified_ms: text == null ? 0 : 1761400000000,
  };
};
const demoToolFilesView = (tool: string) => ({
  tool,
  dir: `C:\\Users\\demo\\.${tool === "pi" ? "pi\\agent" : tool.replace("-", "")}`,
  installed: true,
  files: (DEMO_TOOL_FIXED[tool] ?? []).map(([n, l]) =>
    demoToolFile(tool, n, l),
  ),
  entries: Object.keys(demoToolText[tool] ?? {})
    .filter(
      (n) => n.startsWith("workspace/memory/") || n.startsWith("prompts/"),
    )
    .sort((a, b) =>
      tool === "open-claw" ? b.localeCompare(a) : a.localeCompare(b),
    )
    .map((n) => demoToolFile(tool, n, null)),
  hermes: tool === "hermes" ? { ...demoHermes } : null,
});
/** Skills 面板（阶段 5）的演示数据：一个装在 Claude Code 中转环境的。 */
const DEMO_SKILL_CANDIDATE: SkillCandidate = {
  name: "webapp-testing",
  description: "用 Playwright 测本地网页应用",
  license: "Apache-2.0",
  source: "https://github.com/anthropics/skills/tree/main/webapp-testing",
  revision: "0000000000000000000000000000000000000000",
  fingerprint: "demo",
};
let demoSkills: InstalledSkill[] = [
  {
    id: "skill-creator",
    description: "帮你写新的 Skill",
    license: "Apache-2.0",
    source: "https://github.com/anthropics/skills/tree/main/skill-creator",
    revision: "0000000000000000000000000000000000000000",
    fingerprint: "demo",
    hash: "demo",
    targets: ["claude-code:relay"],
    installed_ms: 1761400000000,
    updated_ms: 1761400000000,
  },
];
const demoSkillsView = () => ({
  skills: demoSkills.map((s) => ({ ...s, targets: [...s.targets] })),
  targets: DEMO_MCP_TARGETS.map((t) => ({
    ...t,
    path: `C:\\Users\\demo\\${t.id}\\skills`,
    entries: demoSkills
      .filter((s) => s.targets.includes(t.id))
      .map((s) => s.id),
  })),
  unmanaged: [],
  backups: [],
});
/** MCP 面板（阶段 5）的演示数据：一条写进了 Claude Code 中转环境的，目标里 OpenCode 当没装。 */
let demoMcp: McpServer[] = [
  {
    id: "fetch",
    name: "mcp-server-fetch",
    server: { type: "stdio", command: "uvx", args: ["mcp-server-fetch"] },
    targets: ["claude-code:relay"],
    description: null,
    homepage: "https://github.com/modelcontextprotocol/servers",
    tags: [],
  },
];
const DEMO_MCP_TARGETS = [
  ["claude-code:relay", "Claude Code · 中转环境", false, true],
  ["claude-code:default", "Claude Code · 官方默认目录", true, true],
  ["codex:relay", "Codex · 中转环境", false, true],
  ["codex:default", "Codex · 官方默认目录", true, true],
  ["gemini-cli:relay", "Gemini CLI · 中转环境", false, true],
  ["opencode", "OpenCode", false, false],
].map(([id, label, official, available]) => ({
  id: id as string,
  label: label as string,
  official: official as boolean,
  available: available as boolean,
}));
const demoMcpView = () => ({
  servers: demoMcp.map((m) => ({ ...m })),
  targets: DEMO_MCP_TARGETS.map((t) => ({
    ...t,
    path: `C:\\Users\\demo\\${t.id}`,
    entries: demoMcp.filter((m) => m.targets.includes(t.id)).map((m) => m.id),
    problem: null,
  })),
});
/** 共存型工具（阶段 4）：哪条线写进了哪个工具（`线路|工具`）。演示里 Hermes 当没装，好让「没找到」也看得见。 */
const demoCoexistWritten = new Set<string>();
const DEMO_COEXIST_TOOLS: Array<[CoexistTool, string, string]> = [
  ["open-code", "OpenCode", ".config\\opencode\\opencode.json"],
  ["minimax-code", "MiniMax Code", ".minimax\\config.yaml"],
  ["open-claw", "OpenClaw", ".openclaw\\openclaw.json"],
  ["hermes", "Hermes", ".hermes\\config.yaml"],
  ["pi", "Pi", ".pi\\agent\\models.json"],
];
const demoCoexistOf = (routeId: string): CoexistStatus[] =>
  DEMO_COEXIST_TOOLS.map(([tool, label, rel]) => ({
    tool,
    label,
    path: `C:\\Users\\demo\\${rel}`,
    installed: tool !== "hermes",
    written: demoCoexistWritten.has(`${routeId}|${tool}`),
    problem: null,
  }));
/** 桌面端直连的那条线（演示里只在内存里）。 */
let demoDirectRoute: string | null = null;
const demoDesktopDirect = () => ({
  route_id: demoDirectRoute,
  base_url: demoDirectRoute ? "https://relay.example.com" : null,
  problem: null,
  candidates: stationRoutes
    .filter((r) => r.client === "claude-desktop")
    .map((r) => ({
      route_id: r.id,
      problem:
        r.upstream_format && r.upstream_format !== "anthropic"
          ? "这条线的上游不是 Anthropic 协议：直连时没有本机路由替它转换"
          : null,
    })),
});
/** 阶段 4 · C5 的演示数据：中转环境还没跳过首次确认，官方默认目录是使用者自己走完的。 */
const demoOnboarding: OnboardingTarget[] = [
  {
    id: "relay",
    label: "中转环境（Claude Code · 走本机路由）",
    path: "C:\\Users\\demo\\AppData\\Local\\qb-gate\\environments\\qb-router-claude-code\\.claude.json",
    official: false,
    skipped: false,
    ours: false,
    problem: null,
  },
  {
    id: "default",
    label: "官方 · 默认目录",
    path: "C:\\Users\\demo\\.claude.json",
    official: true,
    skipped: true,
    ours: false,
    problem: null,
  },
];
const demoEditors: EditorLink[] = [
  {
    editor: "Code",
    label: "VS Code",
    path: "C:\\Users\\demo\\AppData\\Roaming\\Code\\User\\settings.json",
    installed: true,
    linked: false,
    problem: null,
  },
  {
    editor: "Cursor",
    label: "Cursor",
    path: "C:\\Users\\demo\\AppData\\Roaming\\Cursor\\User\\settings.json",
    installed: false,
    linked: false,
    problem: null,
  },
];
const demoCodexHomes: CodexHistoryHome[] = [
  {
    id: "relay",
    label: "中转环境（Codex · 走本机路由）",
    path: "C:\\Users\\demo\\AppData\\Local\\qb-gate\\environments\\qb-router-codex",
    official: false,
    exists: true,
    current: "qb-gate",
    buckets: [{ provider: "ccswitch", sessions: 12, threads: 12 }],
    compressed: 3,
    backups: [],
    problem: null,
  },
  {
    id: "default",
    label: "官方 · 默认目录",
    path: "C:\\Users\\demo\\.codex",
    official: true,
    exists: true,
    current: "openai",
    buckets: [],
    compressed: 0,
    backups: [],
    problem: null,
  },
];
/** 演示里的路由口令。一眼看得出是演示用的，别像真的。 */
const DEMO_ROUTER_KEY = "qbr-demo-not-a-real-key-0000";
/** 本机路由的网络设置（R6）。默认只本机、直连 —— 跟真后端没存过时一样。 */
const stationNetwork: RouterNetworkView = {
  scope: "loopback",
  key: null,
  outbound_proxy: null,
  port: 15721,
};
/** 派单顺序。Claude Code 排了一条替补，好让弹窗里那张表两种状态都看得见。 */
const stationDispatch: Record<string, string[]> = {
  "claude-code": [R_CHEAP],
};
/**
 * 熔断器不在「正常」的那几条。演示里让 Claude Code 的高倍组熔断着（余额不足关一小时），
 * 好让「熔断中 · 还要几分钟」和「立刻重测」在截图里看得见。
 */
const stationBreakers: Record<string, BreakerView[]> = {
  "claude-code": [
    {
      route_id: R_CLAUDE,
      state: "open",
      // 不用 ANCHOR：它在下面才定义（const 不能提前用）。
      open_until_ms: BigInt(Date.now() + 37 * 60_000),
    },
  ],
};
/**
 * 排队中的那条，也按软件各记一份 —— 跟真后端的 `request_switch` 一样。
 *
 * ⛔ 点行只排队，下一发请求才落定。演示以前点行直接改 current，于是界面
 * 「只认 current_route」的毛病在截图里永远看不出来：真机上面板刚启动时
 * 点哪一行都不高亮，右列「启动」一直灰着。
 */
const stationPending: Record<string, string> = {};

/** 测活设置（三个软件共用）。跟真后端没存过时一样：花钱测活关着。 */
const stationProbeSettings: ProbeSettings = {
  paid: false,
  daily_budget_cny: 3,
  free_interval_min: 10,
  paid_min_interval_min: 10,
  paid_max_interval_min: 120,
};
/** 钉住的那条（按软件）。调度开着时手动选的线钉 4 小时 —— 跟真后端 `PIN_MS` 一样。 */
const stationPins: Record<string, DispatchPin> = {};
const DEMO_PIN_MS = 4 * 3600_000;
/** 每条线上次测活是什么时候（「立刻测」会刷新它）。没有 = 演示里按「十分钟前」算。 */
const stationProbedAt: Record<string, number> = {};
/** 今天花钱测活花了多少元（演示里只有开了花钱测活再点「立刻测」才会涨）。 */
let stationProbeSpent: [number, number] = [0, 0];
/** 动作日志，新的在前。 */
const stationActions: SchedulerAction[] = [];

function demoAction(
  client: string,
  kind: string,
  routeId: string | null,
  note: string,
) {
  stationActions.unshift({
    id: `demo-action-${stationActions.length + 1}`,
    at_ms: BigInt(Date.now()),
    client,
    kind,
    route_id: routeId,
    note,
  });
}

/**
 * 演示里的派单：跟真后端同一套分档（钉住 → 合格 → 兜底 → 故障），熔断着的那条排最后、
 * 照样留在队列里（永不全关）。调度关着时队列是手动排的那份，档位那几列是空的。
 *
 * 低价组演示成「没过底线、兜底」：那一档在截图里要看得见，而它恰好是成功率最差的那条。
 */
function demoDispatchOf(who: string) {
  const routes = stationRoutes.filter((r) => r.client === who);
  const open = new Set(
    (stationBreakers[who] ?? [])
      .filter((b) => b.state === "open")
      .map((b) => b.route_id),
  );
  const sched = stationSchedules.find((x) => x.client === who);
  const held = stationPins[who];
  const pin = held && Number(held.until_ms) > Date.now() ? held : null;
  const tierOf = (id: string): DispatchTier =>
    open.has(id)
      ? "faulty"
      : pin?.route_id === id
        ? "pinned"
        : id === R_CHEAP
          ? "fallback"
          : "qualified";
  const rank: Record<DispatchTier, number> = {
    pinned: 0,
    qualified: 1,
    fallback: 2,
    faulty: 3,
  };
  const reason: Record<DispatchTier, string> = {
    pinned: "你手动选的，钉住",
    qualified: "合格",
    fallback: "稳没过线，兜底",
    faulty: "熔断中",
  };
  const sorted = [...routes].sort(
    (a, b) =>
      rank[tierOf(a.id)] - rank[tierOf(b.id)] || a.id.localeCompare(b.id),
  );
  if (!sched?.enabled) {
    return {
      order: [...(stationDispatch[who] ?? [])],
      rows: [] as DispatchRow[],
      alerts: [] as string[],
      pin,
    };
  }
  const rows: DispatchRow[] = sorted.map((r, i) => ({
    route_id: r.id,
    tier: tierOf(r.id),
    position: i + 1,
    score:
      tierOf(r.id) === "faulty" ? null : Number((0.92 - i * 0.18).toFixed(2)),
    samples: Math.max(0, 14 - i * 5),
    reason: reason[tierOf(r.id)],
  }));
  const alerts =
    routes.length === 1
      ? ["只有 1 条线，挂了没有可以换的 —— 再加一条线才谈得上调度"]
      : [];
  return { order: sorted.map((r) => r.id), rows, alerts, pin };
}

function demoQueueOf(who: string) {
  const d = demoDispatchOf(who);
  return {
    client: who,
    current: stationCurrent[who] ?? null,
    order: d.order,
    updated_ms: BigInt(Date.now()),
    pin: d.pin,
    memory: { challenger: null, streak: 0, signature: null },
    rows: d.rows,
    alerts: d.alerts,
  };
}

/** 演示里一条线的测活状况。熔断着的那条测活也判死（余额不足），对得上熔断那一格。 */
function demoLivenessOf(who: string, id: string): LineLiveness {
  const route = stationRoutes.find((r) => r.id === id);
  const dead = (stationBreakers[who] ?? []).some(
    (b) => b.route_id === id && b.state === "open",
  );
  const at = stationProbedAt[id] ?? Date.now() - 10 * 60_000;
  const busy = id === stationCurrent[who];
  return {
    route_id: id,
    label: routeLabelOfDemo(id),
    health: {
      state: dead ? "down" : "up",
      reason: dead ? "测活失败：上游余额不足，要充值" : "测活通过",
      fault: dead ? "balance" : null,
      faults: dead ? 1 : 0,
      last_ok_ms: dead ? null : BigInt(at),
      busy: false,
      missing_models: [],
    },
    last_probe_ms: BigInt(at),
    next_probe_ms:
      busy && !dead ? null : BigInt(at + (dead ? 60 : 10) * 60_000),
    speed_ms:
      stationProbeSettings.paid && !dead ? 1180n + BigInt(id.length * 7) : null,
    speed_samples: stationProbeSettings.paid && !dead ? 3 : 0,
    fail_rate: dead ? 1 : id === R_CHEAP ? 0.2 : 0,
    requests_24h: busy ? 186 : id === R_CHEAP ? 9 : 0,
    real_rate:
      route?.nominal_rate == null
        ? null
        : route.nominal_rate * (route.latest_mult ?? 1),
  };
}

function routeLabelOfDemo(id: string): string {
  const r = stationRoutes.find((x) => x.id === id);
  if (!r) return id;
  const station = r.station_id === "example" ? "Example 中转" : "Moxi";
  return r.group ? `${station} · ${r.group}` : station;
}

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
  const rows: RouterRequest[] = [];
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
      const model = s.models[Math.floor(rand() * s.models.length)];
      const firstToken =
        status === 200
          ? BigInt(Math.round(s.baseTtft * 0.5 + rand() * s.baseTtft * 1.6))
          : null;
      const total =
        status === 500 ? null : BigInt(Math.round(1600 + rand() * 24000));
      const retryAfter =
        status === 429 ? BigInt(Math.round(10 + rand() * 25) * 1000) : null;
      // token 按序号算（不再多抽随机数 —— 那会让上面几列的序列整个变掉）。
      const ok = status === 200;
      const input = ok ? 1200 + ((i * 7919) % 30000) : null;
      const output = ok ? 80 + ((i * 104729) % 2400) : null;
      const fault = ok ? null : status === 429 ? "rate-limit" : "upstream-down";
      rows.push({
        id: `log-${++n}`,
        at_ms: BigInt(t),
        route_id: s.route,
        client: s.client,
        model,
        upstream_model: "",
        status,
        // 上游没回时首字取不到 —— `null`，⛔ 不拿 0 顶上。
        first_token_ms: firstToken,
        total_ms: total,
        retry_after_ms: retryAfter,
        streaming: true,
        input_tokens: input == null ? null : BigInt(input),
        output_tokens: output == null ? null : BigInt(output),
        cache_read_tokens: input == null ? null : BigInt(input * 6),
        cache_write_tokens:
          input == null ? null : BigInt(Math.round(input / 4)),
        cache_write_1h_tokens:
          input == null ? null : BigInt(Math.round(input / 4)),
        station_cost:
          input == null ? null : (input * 0.6 + (output ?? 0) * 3) / 1e6,
        official_cost:
          input == null ? null : (input * 5 + (output ?? 0) * 25) / 1e6,
        session_id: null,
        fault,
        error: fault == null ? null : `${fault}（HTTP ${status}）`,
        attempts: [
          {
            route_id: s.route,
            status,
            fault,
            ms: total ?? 0n,
          },
        ],
      });
    }
  }
  // 新的在前，跟 Rust 侧 `station_logs` 的顺序一致。
  return rows.sort((a, b) => Number(b.at_ms - a.at_ms));
})();

// ------------------------------------------------ 中转用量页（阶段 6 · U1）
//
// 近 7 天按上面那份 `stationLogs` 逐条算；更早的一年里断断续续地编几天（按天汇总的那种），
// 也挂在同一批线路和模型上 —— 这样按软件、按站点、按模型三张表加起来都等于概览那个总数。
type DemoUsageItem = {
  day: string;
  hour: number | null;
  client: string;
  route: string;
  model: string;
  totals: RelayUsageTotals;
};
const blankTotals = (): RelayUsageTotals => ({
  requests: 0,
  failed: 0,
  input_tokens: null,
  output_tokens: null,
  cache_read_tokens: null,
  cache_write_tokens: null,
  station_cost: null,
  official_cost: null,
});
const plusN = (a: number | null, b: number | null) =>
  a == null ? b : b == null ? a : a + b;
function addTotals(t: RelayUsageTotals, x: RelayUsageTotals) {
  t.requests += x.requests;
  t.failed += x.failed;
  t.input_tokens = plusN(t.input_tokens, x.input_tokens);
  t.output_tokens = plusN(t.output_tokens, x.output_tokens);
  t.cache_read_tokens = plusN(t.cache_read_tokens, x.cache_read_tokens);
  t.cache_write_tokens = plusN(t.cache_write_tokens, x.cache_write_tokens);
  t.station_cost = plusN(t.station_cost, x.station_cost);
  t.official_cost = plusN(t.official_cost, x.official_cost);
}
const demoUsageItems: DemoUsageItem[] = (() => {
  const items: DemoUsageItem[] = stationLogs.map((r) => {
    const at = new Date(Number(r.at_ms));
    const ok = r.fault == null && r.status != null && r.status < 300;
    const n = (x: bigint | null) => (x == null ? null : Number(x));
    return {
      day: ymd(at),
      hour: at.getHours(),
      client: r.client,
      route: r.route_id,
      model: r.model,
      totals: {
        requests: 1,
        failed: ok ? 0 : 1,
        input_tokens: n(r.input_tokens),
        output_tokens: n(r.output_tokens),
        cache_read_tokens: n(r.cache_read_tokens),
        cache_write_tokens: n(r.cache_write_tokens),
        station_cost: r.station_cost,
        official_cost: r.official_cost,
      },
    };
  });
  const rand = seeded(20261026);
  const today = todayYmd();
  for (let i = 8; i < 230; i++) {
    if (rand() < 0.35) continue;
    const s = LOG_SHAPES[Math.floor(rand() * LOG_SHAPES.length)];
    const k = Math.round(4 + rand() * s.perDay * 1.4);
    items.push({
      day: addDays(today, -i),
      hour: null,
      client: s.client,
      route: s.route,
      model: s.models[0],
      totals: {
        requests: k,
        failed: Math.round(k * (1 - s.succ)),
        input_tokens: k * 9_000,
        output_tokens: k * 700,
        cache_read_tokens: k * 54_000,
        cache_write_tokens: k * 2_200,
        station_cost: k * 0.0075,
        official_cost: k * 0.062,
      },
    });
  }
  return items;
})();
const demoSiteName: Record<string, string> = {
  example: "我的 API 服务",
  moxi: "备用中转",
};
function demoGroups(
  items: DemoUsageItem[],
  keyOf: (x: DemoUsageItem) => [string, string],
): RelayUsageGroup[] {
  const m = new Map<string, RelayUsageGroup>();
  for (const x of items) {
    const [key, label] = keyOf(x);
    const g = m.get(key) ?? { key, label, totals: blankTotals() };
    addTotals(g.totals, x.totals);
    m.set(key, g);
  }
  return [...m.values()].sort(
    (a, b) =>
      b.totals.requests - a.totals.requests || a.key.localeCompare(b.key),
  );
}
function demoByDay(items: DemoUsageItem[]) {
  const m = new Map<string, RelayUsageTotals>();
  for (const x of items) {
    const t = m.get(x.day) ?? blankTotals();
    addTotals(t, x.totals);
    m.set(x.day, t);
  }
  return [...m.entries()]
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([day, totals]) => ({ day, totals }));
}
function demoRelayUsage(client: string | null, days: number): RelayUsageReport {
  const today = todayYmd();
  const mine = demoUsageItems.filter((x) => !client || x.client === client);
  const first = mine.reduce<string | null>(
    (a, x) => (a == null || x.day < a ? x.day : a),
    null,
  );
  const start = days === 0 ? (first ?? today) : addDays(today, -(days - 1));
  const inRange = mine.filter((x) => x.day >= start);
  const totals = blankTotals();
  for (const x of inRange) addTotals(totals, x.totals);
  const hours = new Map<number, RelayUsageTotals>();
  if (days === 1)
    for (const x of inRange)
      if (x.hour != null) {
        const t = hours.get(x.hour) ?? blankTotals();
        addTotals(t, x.totals);
        hours.set(x.hour, t);
      }
  const yearStart = addDays(today, -370);
  return {
    client,
    days,
    totals,
    series: demoByDay(inRange),
    hours: [...hours.entries()]
      .sort((a, b) => a[0] - b[0])
      .map(([hour, t]) => ({ hour, totals: t })),
    by_client: demoGroups(inRange, (x) => [x.client, x.client]),
    by_site: demoGroups(inRange, (x) => {
      const station = x.route.split(US)[1] ?? x.route;
      return [station, demoSiteName[station] ?? station];
    }),
    by_model: demoGroups(inRange, (x) => [x.model, x.model]),
    year: demoByDay(mine.filter((x) => x.day >= yearStart)),
    first_day: first,
  };
}
function demoRelayRequests(
  args: Record<string, unknown> | undefined,
): RelayRequestPage {
  const rows = stationLogs.filter(
    (r) =>
      (!args?.client || r.client === args.client) &&
      (!args?.model || r.model === args.model) &&
      (!args?.failedOnly ||
        !(r.fault == null && r.status != null && r.status < 300)),
  );
  const offset = Number(args?.offset ?? 0);
  const limit = Number(args?.limit ?? 50);
  return { items: rows.slice(offset, offset + limit), total: rows.length };
}

// ------------------------------------------------ 用量脚本（阶段 6 · U4）
const demoUsageScripts = new Map<string, UsageScript>();
function demoScriptQuota(routeId: string): SiteQuota {
  return {
    provider: "用量脚本",
    kind: "balance",
    balances: [{ remaining: 23.4, total: null, used: null, unit: "USD" }],
    windows: [],
    plan: "这把 Key 的额度",
    warning: null,
    fetched_at: new Date().toISOString(),
    route_id: routeId,
  };
}

// ------------------------------------------------ 会话管理器（阶段 6 · S1）
//
// 三个编出来的会话：中转环境里的 Claude Code、官方槽位 demo-main、Codex 默认目录。路径都在 C:\Users\demo 下面。
const demoSessionSources = [
  {
    id: "claude-code:relay",
    label: "Claude Code · 中转环境",
    client: "claude-code",
  },
  {
    id: "claude-code:slot:demo-main",
    label: "Claude Code · 官方槽位 demo-main",
    client: "claude-code",
  },
  { id: "codex:default", label: "Codex · 官方默认目录", client: "codex" },
];
const demoSessions: SessionSummary[] = [
  {
    id: "claude-code:relay|projects/D--work-shop/1f0c9a2e.jsonl",
    client: "claude-code",
    source: "claude-code:relay",
    source_label: "Claude Code · 中转环境",
    official: false,
    session_id: "1f0c9a2e-5b7d-4c11-9e2a-000000000001",
    title: "把商品页的价格改成按地区显示，顺便补上单测",
    cwd: "D:\\work\\shop",
    started_at: new Date(Date.now() - 3 * 3_600_000).toISOString(),
    updated_at: new Date(Date.now() - 40 * 60_000).toISOString(),
    size: 482_311,
    path: `${HOME}\\AppData\\Local\\ClaudeIpGate\\environments\\qb-router-claude-code\\projects\\D--work-shop\\1f0c9a2e.jsonl`,
  },
  {
    id: "claude-code:slot:demo-main|projects/D--notes/7a1b2c3d.jsonl",
    client: "claude-code",
    source: "claude-code:slot:demo-main",
    source_label: "Claude Code · 官方槽位 demo-main",
    official: true,
    session_id: "7a1b2c3d-0000-4000-8000-000000000002",
    title: "整理一下这周的会议记录，按项目分组",
    cwd: "D:\\notes",
    started_at: new Date(Date.now() - 2 * 86_400_000).toISOString(),
    updated_at: new Date(Date.now() - 2 * 86_400_000 + 1_800_000).toISOString(),
    size: 96_402,
    path: `${HOME}\\AppData\\Local\\ClaudeIpGate\\claude-profile-demo-main\\projects\\D--notes\\7a1b2c3d.jsonl`,
  },
  {
    id: "codex:default|sessions/2026/10/24/rollout-2026-10-24T09-12-00-abc.jsonl",
    client: "codex",
    source: "codex:default",
    source_label: "Codex · 官方默认目录",
    official: true,
    session_id: "abc00000-0000-4000-8000-000000000003",
    title: "CI 里的 lint 挂了，看一下是哪条规则",
    cwd: "D:\\work\\api",
    started_at: new Date(Date.now() - 2.5 * 86_400_000).toISOString(),
    updated_at: new Date(Date.now() - 2.4 * 86_400_000).toISOString(),
    size: 210_877,
    path: `${HOME}\\.codex\\sessions\\2026\\10\\24\\rollout-2026-10-24T09-12-00-abc.jsonl`,
  },
];
const demoSessionTrash: TrashEntry[] = [];
/** 回收站里那几项原来是哪个会话（放回去时插回列表）。 */
const demoSessionRemoved = new Map<string, SessionSummary>();
function demoSessionList(args: Record<string, unknown> | undefined) {
  const text = String(args?.text ?? "")
    .trim()
    .toLowerCase();
  const source = (args?.source as string | null | undefined) ?? null;
  const items = demoSessions.filter(
    (x) =>
      (!source || x.source === source) &&
      (!text ||
        `${x.title} ${x.cwd ?? ""} ${x.session_id}`
          .toLowerCase()
          .includes(text)),
  );
  return {
    items,
    total: items.length,
    sources: demoSessionSources.map((src) => ({
      ...src,
      count: demoSessions.filter((x) => x.source === src.id).length,
    })),
    search_truncated: false,
  };
}
function demoSessionRead(id: string): SessionDetail {
  const s = demoSessions.find((x) => x.id === id);
  if (!s) throw new Error("这个会话 id 不对（可能已经删了或挪走了）");
  const codex = s.client === "codex";
  const at = s.started_at;
  return {
    summary: s,
    messages: [
      {
        role: "user",
        kind: "text",
        name: null,
        text: s.title,
        truncated: false,
        at,
      },
      {
        role: "assistant",
        kind: "thinking",
        name: null,
        text: "先看一下相关的文件和测试。",
        truncated: false,
        at,
      },
      {
        role: "assistant",
        kind: "tool_use",
        name: codex ? "shell" : "Read",
        text: codex
          ? '{"command":["npm","run","lint"]}'
          : '{"file_path":"src/price.ts"}',
        truncated: false,
        at,
      },
      {
        role: "tool",
        kind: "tool_result",
        name: null,
        text: "（演示）工具输出的前几行……",
        truncated: false,
        at,
      },
      {
        role: "assistant",
        kind: "text",
        name: null,
        text: "改好了，测试也补上了。",
        truncated: false,
        at: s.updated_at,
      },
    ],
    total: 5,
    offset: 0,
    resume: codex
      ? `Set-Location -LiteralPath '${s.cwd}'; codex resume ${s.session_id}`
      : `Set-Location -LiteralPath '${s.cwd}'; $env:CLAUDE_CONFIG_DIR = '${s.path.slice(0, s.path.indexOf("\\projects\\"))}'; claude --resume ${s.session_id}`,
    resume_note: s.official
      ? "这是官方账户的会话：门禁开着时 Windows 会拒绝直接启动，先在面板里放行；还要装了对应的命令行。"
      : "这是中转环境的会话：要本机路由开着（从中转站页起一次这个软件就会拉起它），还要装了对应的命令行。",
  };
}

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
    dispatch: { margin: 0.08, min_samples: 3, confirm: 2 },
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
    dispatch: { margin: 0.08, min_samples: 3, confirm: 2 },
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
    dispatch: { margin: 0.08, min_samples: 3, confirm: 2 },
  },
  // 阶段 4 的两档中转命令行（跟真后端的 `ALL_CLIENTS` 一样排在后面）。
  ...(["gemini-cli", "grok-build"] as const).map((client) => ({
    client,
    enabled: false,
    prefs: {
      axes: ["stable"],
      floors: {
        min_success_rate: null,
        max_ttft_p95_ms: null,
        max_rate: null,
      },
    },
    last_switch_ms: null,
    last_note: "",
    dispatch: { margin: 0.08, min_samples: 3, confirm: 2 },
  })),
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
  // 阶段 4 的两档中转命令行：只给编辑器（跟真后端同一组格式标签）。
  if (client === "gemini-cli")
    return {
      path: dir + "\\qb-router-gemini-cli\\.gemini\\settings.json",
      text: '{\n  "security": {\n    "auth": {\n      "selectedType": "gemini-api-key"\n    }\n  }\n}',
      format: "plain-json",
    };
  if (client === "grok-build")
    return {
      path: "C:\\Users\\demo\\.grok\\config.toml",
      text: [
        "[models]",
        'default = "qb-gate"',
        "",
        '[model."qb-gate"]',
        'name = "QB Gate 本机路由"',
        'model = "grok-4.5"',
        'base_url = "http://127.0.0.1:15721/grok/v1"',
        'api_key = "qb-gate-local-router"',
        'api_backend = "responses"',
      ].join("\n"),
      format: "plain-toml",
    };
  return {
    path: dir + "\\qb-router-claude-code\\settings.json",
    text: DEMO_SETTINGS,
    format: "json",
  };
}

const STATION_NO_TABLE = "这个站点没公布价目表（/api/pricing），模型名要自己填";

/**
 * 一个站点公布的模型表（New API 系，分组 ×0.2 从 `/api/pricing` 顶层读到的）。
 *
 * ⛔ New API 的倍率是单价（1 = $2 / 百万）：照官方价抄的 Opus 5 是 2.5、补全 5，
 * Sonnet 5 是 1、补全 5 —— 界面上写成每百万的单价（乘过分组），不写「×2.5 · 翻倍 ×5」。
 */
const stationModels = [
  {
    model: "claude-opus-5",
    rates: {
      model_ratio: 2.5,
      completion_ratio: 5,
      cache_ratio: 0.1,
      create_cache_ratio: 1.25,
      group_ratio: 0.2,
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
      model_ratio: 1,
      completion_ratio: 5,
      cache_ratio: 0.1,
      create_cache_ratio: 1.25,
      group_ratio: 0.2,
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
  station_usage_script_get: (args): UsageScript =>
    demoUsageScripts.get(String(args?.stationId)) ?? {
      enabled: false,
      code: "",
      updated_at: null,
    },
  station_usage_script_save: (args): UsageScript => {
    const code = String(args?.code ?? "");
    if (args?.enabled && !code.trim()) throw new Error("脚本是空的，不能开");
    const saved = {
      enabled: Boolean(args?.enabled),
      code,
      updated_at: new Date().toISOString(),
    };
    demoUsageScripts.set(String(args?.stationId), saved);
    return saved;
  },
  station_usage_script_test: (args): SiteQuota => {
    // 跟后端一样：请求地址不在这个站点上就拒绝（演示里只看有没有写别的域名）。
    if (/https?:\/\/(?!\{\{origin\}\})/.test(String(args?.code ?? "")))
      throw new Error("请求地址必须跟站点同源 —— Key 不发给别的地方（演示）");
    return demoScriptQuota(String(args?.routeId));
  },
  // 站点卡表头的余额 / 套餐额度（阶段 6 · U3）：example 是普通中转站，认不出是哪一家 → null（后端不联网）；
  // moxi 演示成一家认得出的编程套餐站（名字是编的）。
  station_site_quota: (args): SiteQuota | null => {
    const route = stationRoutes.find((r) => r.id === args?.routeId);
    // 阶段 6 · U4：开着用量脚本的站点用脚本那一份（演示里不真跑，给一份编好的）。
    if (route && demoUsageScripts.get(route.station_id)?.enabled)
      return demoScriptQuota(String(args?.routeId));
    if (route?.station_id !== "moxi") return null;
    const now = Date.now();
    return {
      provider: "演示编程套餐",
      kind: "plan",
      balances: [],
      windows: [
        {
          key: "five_hour",
          label: "5 小时",
          used_percent: 35,
          resets_at: new Date(now + 2 * 3_600_000).toISOString(),
        },
        {
          key: "weekly",
          label: "每周",
          used_percent: 82,
          resets_at: new Date(now + 3 * 86_400_000).toISOString(),
        },
      ],
      plan: "pro",
      warning: null,
      fetched_at: new Date(now).toISOString(),
      route_id: String(args?.routeId),
    };
  },
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
    breakers: [
      ...(stationBreakers[String(args?.client ?? "claude-code")] ?? []),
    ],
  }),
  station_router_settings: (args) => {
    const who = String(args?.client ?? "claude-code");
    return { ...(stationRouterSettings[who] ?? routerSettingsOf(who)) };
  },
  station_router_settings_save: (args) => {
    const s = args?.settings as RouterSettings;
    // 跟真后端一样：聚合 key 的登记簿只增不减、界面传来的不认；名单里没 key 的按站点 id 分一个。
    const before =
      stationRouterSettings[s.client] ?? routerSettingsOf(s.client);
    const keys = { ...before.aggregate.keys };
    for (const id of s.aggregate.members) {
      if (Object.values(keys).includes(id)) continue;
      const base = id.split(US)[1] || "line";
      let key = base;
      for (let n = 2; key in keys; n++) key = `${base}-${n}`;
      keys[key] = id;
    }
    const saved = { ...s, aggregate: { ...s.aggregate, keys } };
    stationRouterSettings[s.client] = saved;
    return { ...saved };
  },
  // 站点的备用地址（阶段 4，多端点）。演示里按站点各记一份，测速给固定的假耗时（真后端才联网）。
  station_endpoints: (args) => demoEndpointsOf(String(args?.providerId ?? "")),
  station_endpoint_add: (args) => {
    const e = demoEndpointsOf(String(args?.providerId ?? ""));
    const url = String(args?.url ?? "")
      .trim()
      .replace(/\/+$/, "");
    if (!/^https?:\/\//i.test(url))
      throw new Error(`地址要以 https:// 或 http:// 开头：${url}`);
    if (!e.endpoints.some((x) => x.url.toLowerCase() === url.toLowerCase()))
      e.endpoints.push({
        url,
        added_ms: Date.now(),
        latency_ms: null,
        status: null,
        error: null,
        tested_ms: null,
      });
    return { ...e, endpoints: [...e.endpoints] };
  },
  station_endpoint_remove: (args) => {
    const e = demoEndpointsOf(String(args?.providerId ?? ""));
    e.endpoints = e.endpoints.filter((x) => x.url !== args?.url);
    return { ...e, endpoints: [...e.endpoints] };
  },
  station_endpoint_promote: (args) => {
    const e = demoEndpointsOf(String(args?.providerId ?? ""));
    e.endpoints = e.endpoints.filter((x) => x.url !== args?.url);
    return { ...e, endpoints: [...e.endpoints] };
  },
  station_endpoints_test: (args) => {
    const e = demoEndpointsOf(String(args?.providerId ?? ""));
    e.endpoints = e.endpoints.map((x, i) => ({
      ...x,
      latency_ms: 180 + i * 140,
      status: 404,
      error: null,
      tested_ms: Date.now(),
    }));
    return {
      main: { url: "", latency_ms: 120, status: 404, error: null },
      endpoints: { ...e, endpoints: [...e.endpoints] },
    };
  },
  // 提示词库（阶段 5）。演示里只在内存里记。
  prompts_list: () => demoPromptsView(),
  prompts_save: (args) => {
    const p = args?.prompt as Prompt;
    const id = p.id || `prompt-${Date.now()}`;
    demoPrompts = demoPrompts.filter((x) => x.id !== id);
    demoPrompts.unshift({ ...p, id, updated_ms: Date.now() });
    return demoPromptsView();
  },
  prompts_delete: (args) => {
    if (Object.values(demoPromptActive).includes(String(args?.id)))
      throw new Error("正在用这一条：先在那里换成别的再删");
    demoPrompts = demoPrompts.filter((x) => x.id !== args?.id);
    return demoPromptsView();
  },
  prompts_enable: (args) => {
    demoPromptActive[String(args?.target)] = String(args?.id);
    return demoPromptsView();
  },
  prompts_import: () => demoPromptsView(),
  // Skills 面板（阶段 5）。演示里只在内存里记；扫描回一个假的 Skill。
  skills_list: () => demoSkillsView(),
  skills_discover: () => [DEMO_SKILL_CANDIDATE],
  skills_discover_zip: () => [DEMO_SKILL_CANDIDATE],
  skills_install: (args) => {
    const targets = (args?.targets as string[] | undefined) ?? [];
    for (const c of (args?.picked as SkillCandidate[] | undefined) ?? [])
      if (!demoSkills.some((s) => s.id === c.name))
        demoSkills.push({
          id: c.name,
          description: c.description,
          license: c.license,
          source: c.source,
          revision: c.revision,
          fingerprint: c.fingerprint,
          hash: c.fingerprint,
          targets: [...targets],
          installed_ms: Date.now(),
          updated_ms: Date.now(),
        });
    return demoSkillsView();
  },
  skills_install_zip: () => demoSkillsView(),
  skills_set_target: (args) => {
    const s = demoSkills.find((x) => x.id === args?.id);
    if (!s) throw new Error("没有这个 Skill");
    s.targets = s.targets.filter((t) => t !== args?.target);
    if (args?.enabled) s.targets.push(String(args?.target));
    return demoSkillsView();
  },
  skills_uninstall: (args) => {
    demoSkills = demoSkills.filter((s) => s.id !== args?.id);
    return demoSkillsView();
  },
  skills_restore: () => demoSkillsView(),
  skills_import: () => demoSkillsView(),
  skills_check_updates: () => [],
  skills_update: () => demoSkillsView(),
  skills_search: () => [],
  // MCP 面板（阶段 5）。演示里只在内存里记；目标的「现有条目」跟着勾选变。
  mcp_list: () => demoMcpView(),
  mcp_save: (args) => {
    const server = args?.server as McpServer;
    const previous = (args?.previousId as string | null) ?? null;
    if (!/^[A-Za-z0-9_.-]{1,64}$/.test(server.id))
      throw new Error(`MCP 服务器名只能用字母、数字、_、- 和 .：${server.id}`);
    if (previous == null && demoMcp.some((m) => m.id === server.id))
      throw new Error(`已经有一条叫 ${server.id} 的 MCP 服务器`);
    demoMcp = demoMcp.filter((m) => m.id !== (previous ?? server.id));
    demoMcp.push({ ...server, targets: [...server.targets].sort() });
    return demoMcpView();
  },
  mcp_delete: (args) => {
    demoMcp = demoMcp.filter((m) => m.id !== args?.id);
    return demoMcpView();
  },
  mcp_set_target: (args) => {
    const m = demoMcp.find((x) => x.id === args?.id);
    const t = DEMO_MCP_TARGETS.find((x) => x.id === args?.target);
    if (!m || !t) throw new Error("没有这条");
    if (args?.enabled && !t.available)
      throw new Error(`${t.label} 没找到：多半还没装，不替它建`);
    m.targets = m.targets.filter((x) => x !== t.id);
    if (args?.enabled) m.targets.push(t.id);
    return demoMcpView();
  },
  mcp_import: () => ({
    added: [],
    linked: [],
    skipped: [],
    view: demoMcpView(),
  }),
  // 共存型工具（阶段 4）：演示里只在内存里记一下写没写，路径照真后端在 Windows 上的样子编。
  station_coexist_status: (args) => demoCoexistOf(String(args?.routeId ?? "")),
  station_coexist_write: (args) => {
    const id = String(args?.routeId ?? "");
    demoCoexistWritten.add(`${id}|${String(args?.tool)}`);
    return demoCoexistOf(id);
  },
  station_coexist_remove: (args) => {
    const id = String(args?.routeId ?? "");
    demoCoexistWritten.delete(`${id}|${String(args?.tool)}`);
    return demoCoexistOf(id);
  },
  // 聚合模型写进配置时的问题（阶段 4）。演示里没有问题。
  station_aggregate_config_problem: () => null,
  // 阶段 4 · C5 四个小件。演示里都只在内存里记。
  station_import_desktop_routes: () => {
    const added: Array<(typeof stationRoutes)[number]> = [];
    for (const r of stationRoutes.filter(
      (x) => x.client === "claude-code" && x.credential_id,
    )) {
      const id = `claude-desktop\u001f${r.station_id}\u001f${r.group}`;
      if (stationRoutes.some((x) => x.id === id)) continue;
      const copy = { ...r, id, client: "claude-desktop" as const };
      stationRoutes.push(copy);
      added.push(copy);
    }
    return added;
  },
  // 桌面端直连档（阶段 4）。演示里只记选了哪条；没设映射、说 Anthropic 的才能选。
  station_desktop_direct: () => demoDesktopDirect(),
  station_desktop_direct_set: (args) => {
    const id = (args?.routeId as string | null | undefined) ?? null;
    if (id) {
      const c = demoDesktopDirect().candidates.find((x) => x.route_id === id);
      if (!c) throw new Error("这条线不在了");
      if (c.problem) throw new Error(c.problem);
    }
    demoDirectRoute = id;
    return demoDesktopDirect();
  },
  claude_onboarding_status: () => demoOnboarding.map((t) => ({ ...t })),
  claude_onboarding_set: (args) => {
    const t = demoOnboarding.find((x) => x.id === args?.target);
    if (!t) throw new Error("不认得这个目标");
    if (args?.skip) {
      if (!t.skipped) Object.assign(t, { skipped: true, ours: true });
    } else {
      if (t.skipped && !t.ours)
        throw new Error(
          `${t.label} 的首次确认是 Claude Code 自己记下的，面板不动它`,
        );
      Object.assign(t, { skipped: false, ours: false });
    }
    return demoOnboarding.map((x) => ({ ...x }));
  },
  claude_vscode_status: () => demoEditors.map((e) => ({ ...e })),
  claude_vscode_link: (args) => {
    const e = demoEditors.find((x) => x.editor === args?.editor);
    if (!e?.installed)
      throw new Error("没找到这个编辑器的用户设置目录：多半还没装");
    e.linked = true;
    return demoEditors.map((x) => ({ ...x }));
  },
  claude_vscode_unlink: (args) => {
    const e = demoEditors.find((x) => x.editor === args?.editor);
    if (e) e.linked = false;
    return demoEditors.map((x) => ({ ...x }));
  },
  codex_history_status: () => structuredClone(demoCodexHomes),
  codex_history_merge: (args) => {
    const h = demoCodexHomes.find((x) => x.id === args?.home);
    const from = (args?.from as string[] | undefined) ?? [];
    if (!h || from.length === 0) throw new Error("没勾要并进来的桶");
    const moved = h.buckets.filter((b) => from.includes(b.provider));
    h.buckets = h.buckets.filter((b) => !from.includes(b.provider));
    h.backups.unshift({
      id: `demo-${Date.now()}`,
      at_ms: Date.now(),
      from,
      to: h.current,
      sessions: moved.reduce((n, b) => n + b.sessions, 0),
      threads: moved.reduce((n, b) => n + b.threads, 0),
      restored: false,
    });
    return structuredClone(demoCodexHomes);
  },
  codex_history_restore: (args) => {
    const h = demoCodexHomes.find((x) => x.id === args?.home);
    const b = h?.backups.find((x) => x.id === args?.backup);
    if (!h || !b || b.restored) throw new Error("找不到这份备份");
    b.restored = true;
    for (const p of b.from)
      h.buckets.push({ provider: p, sessions: b.sessions, threads: b.threads });
    return structuredClone(demoCodexHomes);
  },
  // 聚合模型的预览（阶段 3）。id 照后端的格式拼；桌面端的数字在演示里是假的（真后端按 key 和模型名散列）。
  station_aggregate_models: (args) => {
    const who = String(args?.client ?? "claude-code");
    const a = (stationRouterSettings[who] ?? routerSettingsOf(who)).aggregate;
    if (!a.enabled) return [];
    const out: Array<Record<string, unknown>> = [];
    for (const id of a.members) {
      const key = Object.entries(a.keys).find(([, r]) => r === id)?.[0];
      const route = stationRoutes.find((r) => r.id === id);
      if (!key || !route) continue;
      (a.models[id] ?? []).forEach((upstream, i) => {
        const model = upstream.replace(/\[1m\]$/i, "");
        const one_m = model !== upstream;
        out.push({
          route_id: id,
          id:
            who === "codex"
              ? `qbg-${key}/${model}`
              : who === "claude-desktop"
                ? `claude-fable-qbg-${String(100000 + ((key.length * 7919 + i * 104729) % 900000))}`
                : `qbg-claude-${key}--${model}${one_m ? "[1M]" : ""}`,
          upstream,
          display_name: `${model}（${route.station_id} · ${route.group}）`,
          one_m,
        });
      });
    }
    return out;
  },
  // 导入链接（阶段 5 · X4）。粘贴什么都回那一条假的站点链接；导入只回一句话，不真的加线路。
  deeplink_peek: () => demoDeepLink,
  deeplink_ack: (args) => {
    if (demoDeepLink?.id === Number(args?.id)) demoDeepLink = null;
    return null;
  },
  deeplink_paste: (args) => {
    const url = String(args?.url ?? "").trim();
    if (!/^(qbgate|ccswitch):\/\//i.test(url))
      throw new Error("只认 qbgate:// 或 ccswitch:// 开头的导入链接");
    demoDeepLink = {
      id: Date.now(),
      scheme: url.split("://")[0].toLowerCase(),
      view: DEMO_DEEPLINK_PROVIDER,
    };
    return demoDeepLink;
  },
  deeplink_import: () => {
    demoDeepLink = null;
    return {
      message: "导入了「示例中转 · 默认分组」：要用它，在中转站页点这一行",
      client: "claude-code",
      route_id: null,
      skipped: [],
    };
  },
  // 工具文件（阶段 5，C3 / C4 的附带件）。演示里只在内存里记；网页面板永远「没开着」。
  tool_files_view: (args) =>
    demoToolFilesView(String(args?.tool ?? "open-claw")),
  tool_files_save: (args) => {
    const tool = String(args?.tool ?? "open-claw");
    const name = String(args?.name ?? "");
    (demoToolText[tool] ??= {})[name] = String(args?.content ?? "");
    const label =
      (DEMO_TOOL_FIXED[tool] ?? []).find(([n]) => n === name)?.[1] ?? null;
    return demoToolFile(tool, name, label);
  },
  tool_files_delete: (args) => {
    const tool = String(args?.tool ?? "open-claw");
    delete demoToolText[tool]?.[String(args?.name ?? "")];
    return demoToolFilesView(tool);
  },
  tool_files_search: (args) => {
    const q = String(args?.query ?? "").toLowerCase();
    return Object.entries(demoToolText["open-claw"] ?? {})
      .filter(
        ([n, t]) =>
          n.startsWith("workspace/memory/") && t.toLowerCase().includes(q),
      )
      .map(([n, t]) => ({ name: n, snippet: t.slice(0, 120), matches: 1 }));
  },
  tool_files_hermes_save: (args) => {
    demoHermes = { ...(args?.settings as typeof demoHermes) };
    return { ...demoHermes };
  },
  tool_files_hermes_web: () => null,
  // 项目（阶段 5 · M4）。演示里只在内存里记；换项目照真后端：先存回原来那个，再落线路和派单顺序。
  projects_list: () => demoProjectsView(),
  projects_create: (args) => {
    const who = String(args?.client ?? "claude-code");
    const name = String(args?.name ?? "").trim();
    if (!name) throw new Error("给项目起个名字");
    const id = `project-${Date.now()}`;
    demoProjects.push({
      id,
      name,
      slots: { [who]: demoProjectSnapshot(who) },
      updated_ms: Date.now(),
    });
    demoProjectCurrent[who] = id;
    return demoProjectsView();
  },
  projects_rename: (args) => {
    const p = demoProjects.find((x) => x.id === args?.id);
    if (!p) throw new Error("没有这个项目");
    p.name = String(args?.name ?? "").trim() || p.name;
    return demoProjectsView();
  },
  projects_delete: (args) => {
    demoProjects = demoProjects.filter((x) => x.id !== args?.id);
    for (const [k, v] of Object.entries(demoProjectCurrent))
      if (v === args?.id) delete demoProjectCurrent[k];
    return demoProjectsView();
  },
  projects_apply: (args) => {
    const who = String(args?.client ?? "claude-code");
    const cur = demoProjectCurrent[who];
    const p = demoProjects.find((x) => x.id === args?.id);
    if (!p) throw new Error("没有这个项目");
    const before = demoProjects.find((x) => x.id === cur);
    if (before && before.id !== p.id)
      before.slots = { ...before.slots, [who]: demoProjectSnapshot(who) };
    const warnings: string[] = [];
    const slot = p.slots[who];
    if (!slot)
      warnings.push(
        "这个项目还没存过这个软件的设置：记成当前项目，什么都没动（换走时会自动存下）",
      );
    else {
      if (slot.route && slot.route !== stationCurrent[who]) {
        if (stationRoutes.some((r) => r.id === slot.route && r.client === who))
          FIXTURES.station_select_route({
            routeId: slot.route,
            immediate: false,
          });
        else
          warnings.push(`项目里存的线路「${slot.route}」已经不在了，上游没换`);
      }
      if (slot.queue)
        if (stationSchedules.find((x) => x.client === who)?.enabled)
          warnings.push(
            "智能调度开着：派单顺序归调度排，项目里存的顺序没落（停了调度再换一次项目就会落）",
          );
        else stationDispatch[who] = [...slot.queue];
    }
    demoProjectCurrent[who] = p.id;
    return { view: demoProjectsView(), warnings };
  },
  projects_clear: (args) => {
    const who = String(args?.client ?? "claude-code");
    const cur = demoProjects.find((x) => x.id === demoProjectCurrent[who]);
    if (cur) cur.slots = { ...cur.slots, [who]: demoProjectSnapshot(who) };
    delete demoProjectCurrent[who];
    return { view: demoProjectsView(), warnings: [] };
  },
  station_dispatch_queue: (args) =>
    demoQueueOf(String(args?.client ?? "claude-code")),
  station_dispatch_queue_save: (args) => {
    const who = String(args?.client ?? "claude-code");
    // 跟真后端一样：调度开着时顺序归调度排。
    if (stationSchedules.find((x) => x.client === who)?.enabled)
      throw new Error(
        "智能调度开着时热备顺序由调度排（下一跳就会盖掉手排的）；要自己排先停调度，或者钉住一条",
      );
    stationDispatch[who] = [...((args?.order as string[]) ?? [])];
    return demoQueueOf(who);
  },
  // 派单队列与测活（阶段 2）。全是演示夹具里的状态，跟真后端一样有状态。
  station_dispatch_view: (args) => {
    const who = String(args?.client ?? "claude-code");
    const queue = demoQueueOf(who);
    const ids = [
      ...queue.order,
      ...stationRoutes
        .filter((r) => r.client === who && !queue.order.includes(r.id))
        .map((r) => r.id),
    ];
    return {
      client: who,
      enabled: Boolean(stationSchedules.find((x) => x.client === who)?.enabled),
      queue,
      lines: ids.map((id) => demoLivenessOf(who, id)),
      spent_today: [...stationProbeSpent] as [number, number],
      settings: { ...stationProbeSettings },
      actions: stationActions.filter((a) => a.client === who).slice(0, 30),
    };
  },
  station_probe_now: (args) => {
    const who = String(args?.client ?? "claude-code");
    const only = args?.routeId as string | null | undefined;
    const now = Date.now();
    const out: ProbeAttempt[] = [];
    for (const r of stationRoutes.filter(
      (x) => x.client === who && (only == null || x.id === only),
    )) {
      stationProbedAt[r.id] = now;
      const dead = (stationBreakers[who] ?? []).some(
        (b) => b.route_id === r.id && b.state === "open",
      );
      out.push({
        id: `demo-probe-${r.id}-${now}`,
        route_id: r.id,
        at_ms: BigInt(now),
        model: "",
        ok: !dead,
        fault: dead ? "balance" : null,
        total_ms: dead ? 140n : 320n,
        first_ms: null,
        paid: false,
        cost_lo: null,
        cost_hi: null,
        error: dead ? "HTTP 402：上游余额不足，要充值" : null,
      });
      if (stationProbeSettings.paid && !dead) {
        stationProbeSpent = [
          stationProbeSpent[0] + 0.0004,
          stationProbeSpent[1] + 0.0011,
        ];
        out.push({
          ...out[out.length - 1],
          id: `demo-paid-${r.id}-${now}`,
          model: who === "codex" ? "gpt-6-luna" : "claude-sonnet-5",
          total_ms: 1180n,
          first_ms: 860n,
          paid: true,
          cost_lo: 0.0004,
          cost_hi: 0.0011,
        });
      }
    }
    return new Promise((done) => setTimeout(done, 600)).then(() => out);
  },
  station_pin: (args) => {
    const who = String(args?.client ?? "claude-code");
    const id = (args?.routeId as string | null | undefined) ?? null;
    if (id) {
      stationPins[who] = {
        route_id: id,
        until_ms: BigInt(Date.now() + DEMO_PIN_MS),
      };
      if (stationCurrent[who] !== id) stationPending[who] = id;
      demoAction(who, "pin", id, "你手动选的，钉住 4 小时（它挂了就不算）");
    } else {
      delete stationPins[who];
      demoAction(who, "unpin", null, "取消钉住，交回调度");
    }
    return demoQueueOf(who);
  },
  station_probe_settings_save: (args) => {
    const next = args?.settings as ProbeSettings;
    if (!(next.daily_budget_cny >= 0))
      throw new Error("每日预算要是个不小于 0 的数");
    if (next.paid_min_interval_min > next.paid_max_interval_min)
      throw new Error("花钱测活的最短间隔不能比最长的还长");
    Object.assign(stationProbeSettings, next);
    return { ...stationProbeSettings };
  },
  // 本机路由的网络设置（R6）。演示里不真的改监听、不真的连代理；状态照样记着。
  station_router_network: () => ({ ...stationNetwork }),
  station_router_listen_save: (args) => {
    stationNetwork.scope = (args?.scope as ListenScope) ?? "loopback";
    stationNetwork.key =
      stationNetwork.scope === "all-interfaces"
        ? (stationNetwork.key ?? DEMO_ROUTER_KEY)
        : null;
    return { ...stationNetwork };
  },
  station_router_key_rotate: () => {
    if (stationNetwork.scope === "all-interfaces")
      stationNetwork.key = `${DEMO_ROUTER_KEY.slice(0, 12)}${Date.now()}`;
    return { ...stationNetwork };
  },
  station_outbound_proxy_save: (args) => {
    const url = (args?.url as string | null) ?? null;
    // 跟真后端一样只给界面看去掉用户名密码的写法。
    stationNetwork.outbound_proxy = url
      ? url.replace(/\/\/[^@/]*@/, "//").replace(/(:\/\/[^/]*)\/.*$/, "$1")
      : null;
    return { ...stationNetwork };
  },
  station_outbound_proxy_test: () => 182,
  station_outbound_proxy_scan: () => [
    "http://127.0.0.1:7890",
    "socks5://127.0.0.1:7890",
  ],
  // 「立刻重测」：熔断中的那条转半开（真后端一样：下一发真实请求就是回探）。
  station_route_retest: (args) => {
    const id = String(args?.routeId ?? "");
    for (const list of Object.values(stationBreakers)) {
      const b = list.find((x) => x.route_id === id);
      if (b && b.state === "open") {
        b.state = "half-open";
        b.open_until_ms = BigInt(Date.now());
        return true;
      }
    }
    return false;
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
  // 切哪个软件的上游，由**这条线自己的 client** 决定 —— 跟真后端同一条规矩。
  // 拿参数里的 client 当依据的话，演示就验不出那个 bug。
  station_select_route: (args) => {
    const route = stationRoutes.find(
      (r) => r.id === args?.id || r.id === args?.routeId,
    );
    if (route) {
      const who = route.client;
      if (stationSchedules.find((x) => x.client === who)?.enabled) {
        stationPins[who] = {
          route_id: route.id,
          until_ms: BigInt(Date.now() + DEMO_PIN_MS),
        };
        demoAction(
          who,
          "pin",
          route.id,
          "你手动选的，钉住 4 小时（它挂了就不算）",
        );
      }
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
    // New API 后台「充值价格」的默认值 —— 很多站没改过，所以只当参考，不自动填。
    topup_hint: 7.3,
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
    if (args?.dispatch != null)
      s.dispatch = args.dispatch as (typeof s)["dispatch"];
    return { ...s };
  },
  station_logs: () => stationLogs,
  station_usage_report: (args) =>
    demoRelayUsage(
      (args?.client as string | null | undefined) ?? null,
      Number(args?.days ?? 7),
    ),
  station_usage_requests: (args) => demoRelayRequests(args),
  sessions_list: (args) => demoSessionList(args),
  sessions_read: (args) => demoSessionRead(String(args?.id)),
  sessions_export: () => null,
  sessions_delete: (args) => {
    const at = demoSessions.findIndex((x) => x.id === args?.id);
    if (at < 0) throw new Error("这个会话 id 不对（可能已经删了或挪走了）");
    const [s] = demoSessions.splice(at, 1);
    const entry: TrashEntry = {
      id: `demo-${Date.now()}-session-1`,
      kind: "session",
      name: `${s.source_label} · ${s.title}`,
      originals: [s.path],
      deleted_at: "2026-10-26 12:00",
      size: s.size,
    };
    demoSessionTrash.unshift(entry);
    demoSessionRemoved.set(entry.id, s);
    return entry;
  },
  sessions_trash: () => [...demoSessionTrash],
  sessions_trash_restore: (args) => {
    const at = demoSessionTrash.findIndex((t) => t.id === args?.id);
    if (at < 0) throw new Error("回收站里没有这一项");
    const [entry] = demoSessionTrash.splice(at, 1);
    const back = demoSessionRemoved.get(entry.id);
    if (back) {
      demoSessions.push(back);
      demoSessions.sort((a, b) => b.updated_at.localeCompare(a.updated_at));
      demoSessionRemoved.delete(entry.id);
    }
    return entry;
  },
  sessions_trash_purge: (args) => {
    const n = args?.id ? 1 : demoSessionTrash.length;
    if (args?.id) {
      const at = demoSessionTrash.findIndex((t) => t.id === args.id);
      if (at >= 0) demoSessionTrash.splice(at, 1);
    } else demoSessionTrash.length = 0;
    return n;
  },
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
  // 克隆一份：额度演示会就地改槽位（盖上「官方接口」那份），同一个引用 React Query 看不出变了。
  accounts_list: () => structuredClone(accounts),
  accounts_tokens: () => tokenUsage,
  accounts_token_summary: (args) => summaryFor(Number(args?.days ?? 1)),
  accounts_usage_overview: (args) =>
    demoUsageOverview(String(args?.label ?? ""), Number(args?.days ?? 7)),
  // 演示里给「本地令牌已过期」那一档 —— 实机上它就是最常见的那个，
  // 而且它是唯一一个**不发请求**的分支，最该让人在截图里先看见。
  account_probe: () => accountProbe,
  // 官方额度接口（阶段 6 · U2）：只在点了刷新图标 / 「向官方问一次」时才有，问到了就盖到那一行上。
  claude_quota: (args) =>
    askedOnly(`claude:${String(args?.label)}`, args?.refresh, () =>
      claudeQuotaFixture(String(args?.label)),
    ),
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
  snapshot_list: () => snapshots,
  settings_load: () => settings,
  // 环境变量清理（阶段 7 · X5）：两个当前用户的（能删）、一个只在面板进程里的（不能删），值照真后端那样掩码过。
  // 删 / 还原没有演示：那两条只在装好的面板里可用。
  env_vars_scan: () => [
    {
      name: "ANTHROPIC_BASE_URL",
      scope: "user",
      owner: "claude-code",
      shown: "https://relay.example.test",
      expand: false,
      removable: true,
    },
    {
      name: "OPENAI_API_KEY",
      scope: "user",
      owner: "codex",
      shown: "（已设置，值不显示）",
      expand: false,
      removable: true,
    },
    {
      name: "HTTPS_PROXY",
      scope: "process",
      owner: "proxy",
      shown: "http://127.0.0.1:7890",
      expand: false,
      removable: false,
    },
  ],
  env_backups: () => [
    {
      id: "env-20261027-101500",
      created: "2026-10-27T10:15:00+08:00",
      names: ["GEMINI_API_KEY"],
      restored_at: null,
    },
  ],
  // 开机自启（阶段 7 · X7）：演示成「没登记」—— 演示页面不该假装改了谁的注册表。
  autostart_status: () => ({
    registered: false,
    disabled_by_user: false,
    command: null,
    points_here: false,
    effective: false,
    dev_build: false,
  }),
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
  // 切 Claude 账户的确认框：只报 Claude 那几类（中转与反重力不碰），跟真去关的是同一个范围。
  official_switch_preview: () => ({
    ...kill,
    targets: kill.targets.filter((t) => t.role !== "antigravity"),
  }),
  plugin_list: () => plugins,
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
          remaining_implied: false,
        },
        {
          model_id: "gemini-2.5-flash",
          label: "gemini-2.5-flash",
          token_type: "REQUESTS",
          remaining_percent: 84,
          reset_at: "2026-09-22 00:00",
          reset_epoch: Math.floor(Date.now() / 1000) + 36000,
          remaining_implied: false,
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
  claude_zh_leftovers: () => claudeZhLeftovers(),
  claude_zh_cleanup: () => claudeZhCleanup(),
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
      ide_unreadable: false,
      email: null,
      tier: null,
      identity_error: null,
      quota: [],
      written_at: null,
      cli_dir: `${HOME}\\AppData\\Local\\ClaudeIpGate\\antigravity-accounts\\${id}\\cli-home`,
      cli_logged_in: false,
      cli_auth_state: "未登录",
      cli_unreadable: false,
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
    sdk_sessions: 0,
    // CLI 模式：没有常驻会话（弹窗里写明「每条消息起一次 claude -p」）。
    sessions: [],
  }),
  // 一条调用的全文（2026-09-29，原来只在 bridge.py 自己的监控页上）。全是编的。
  tavern_bridge_call_detail: (args) => ({
    system_prompt:
      "（演示）你是林澈，一名温和但机敏的图书管理员。始终保持角色身份，用中文回应；每轮 340–520 字。",
    messages: [
      { role: "user", content: "（演示）你好，今天有什么推荐的书吗？" },
      {
        role: "system",
        content: "（演示）[作者注释] 这一段放慢节奏，多写环境。",
      },
    ],
    output:
      "（演示）林澈从一排排书架间抬起头，推了推眼镜：「你来得正好，昨天刚到了一批旧版的游记。」" +
      String(args?.id ?? ""),
    error: null,
    raw: null,
  }),
  // 软件页 SillyTavern 卡（2026-09-29）。安装 / 更新 / 卸载只改演示里的状态，什么都不下载。
  tavern_setup_status: () => tavernSetupStatus(),
  tavern_latest: (): TavernRelease => ({
    tag: DEMO_ST_LATEST,
    version: DEMO_ST_LATEST,
    node_engine: ">= 20",
    node_ok: !demoFlag("qb-demo-tavern-no-node"),
    page_url: `https://github.com/SillyTavern/SillyTavern/releases/tag/${DEMO_ST_LATEST}`,
    archive_url: `https://github.com/SillyTavern/SillyTavern/archive/refs/tags/${DEMO_ST_LATEST}.zip`,
  }),
  tavern_install: (args) => {
    const version = args?.tag ? String(args.tag) : DEMO_ST_LATEST;
    demoTavern = { managed: true, version, previous: null };
    return (
      `演示：SillyTavern ${version} 已装好：${DEMO_ST_MANAGED}` +
      (args?.importFrom
        ? `；从 ${String(args.importFrom)} 复制了 1234 个文件（data、config.yaml），原来那份没动`
        : "")
    );
  },
  tavern_update: (args) => {
    const was = demoTavernState().version;
    const version = args?.tag ? String(args.tag) : DEMO_ST_LATEST;
    if (version === was) throw new Error(`现在就是 ${was}，没有换。`);
    demoTavern = { managed: true, version, previous: was };
    return `演示：从 ${was} 换到了 ${version}；原来那份（${was}）整份留在 ${DEMO_ST_MANAGED}.previous`;
  },
  // 2026-09-29：软件页 SillyTavern 卡的版本清单；账户页「酒馆」弹窗的控制台、角色卡、世界书、扩展、回收站。
  ...makeTavernFixtures({
    home: HOME,
    get: demoTavernState,
    set: (s) => {
      demoTavern = s;
    },
    noNode: () => demoFlag("qb-demo-tavern-no-node"),
    flag: demoFlag,
  }),
  tavern_uninstall: (): TavernUninstalled => {
    demoTavern = { managed: false, version: "1.18.0", previous: null };
    return {
      backup: `${HOME}\\AppData\\Local\\ClaudeIpGate\\tavern-uninstall\\20260929-080000`,
      backed_up: 1234,
      removed: [DEMO_ST_MANAGED],
      left: [],
    };
  },
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
  // 数字照 bridge.py 2.4.0 真实会给的量级：输入是系统提示词 + 酒馆消息的字符数，
  // 回复几百字，输出 token 含思考所以比回复多得多；缓存读的是整段前缀。
  tavern_bridge_telemetry: () => ({
    records: [
      {
        id: "demo-1",
        at: "09-21 08:12:03",
        status: "ok",
        mode: "cli",
        model: "claude-opus-5",
        effort: "max",
        input_chars: 15626,
        output_chars: 462,
        output_tokens: 1720,
        cache_read: 19387,
        cache_write: 1902,
        prefix: "append_only",
        elapsed_ms: 41500,
      },
      {
        id: "demo-2",
        at: "09-21 08:09:51",
        status: "ok",
        mode: "cli",
        model: "claude-opus-5",
        effort: "max",
        input_chars: 14996,
        output_chars: 513,
        output_tokens: 3014,
        cache_read: 0,
        cache_write: 20480,
        prefix: "first",
        elapsed_ms: 56400,
      },
    ],
    total: 2,
    offset: 0,
    has_more: false,
    hit_rate: 0.5,
    cache_read: 19387,
    cache_write: 22382,
    countable: 2,
    prefix_reusable: 1,
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

/** Claude 官方额度接口的演示回复；同时像后端的 `overlay` 一样把 5 小时 / 7 天盖到那一行上。 */
function claudeQuotaFixture(label: string): ClaudeQuota {
  const now = Date.now();
  const at = (ms: number) => new Date(now + ms).toISOString();
  const week = at(3.1 * 86_400_000);
  const q: ClaudeQuota = {
    label,
    windows: [
      {
        key: "five_hour",
        label: "5 小时",
        used_percent: 41,
        resets_at: at(2.4 * 3_600_000),
      },
      { key: "seven_day", label: "7 天", used_percent: 58, resets_at: week },
      {
        key: "seven_day_opus",
        label: "7 天 · Opus",
        used_percent: 73,
        resets_at: week,
      },
      {
        key: "seven_day_sonnet",
        label: "7 天 · Sonnet",
        used_percent: 22,
        resets_at: week,
      },
    ],
    extra: {
      enabled: true,
      monthly_limit: 50,
      used_credits: 6.4,
      utilization: 12.8,
      currency: "USD",
    },
    fetched_at: new Date(now).toISOString(),
    source: "https://api.anthropic.com/api/oauth/usage",
  };
  const slot = accounts.slots.find((s) => s.label === label);
  if (slot) {
    const win = (key: string) => {
      const w = q.windows.find((x) => x.key === key)!;
      return {
        used: w.used_percent,
        resets_at: w.resets_at,
        estimated: false,
        reset_passed: false,
      };
    };
    slot.usage = {
      source: "official",
      measured_at: q.fetched_at,
      age_minutes: 0,
      five_hour: win("five_hour"),
      seven_day: win("seven_day"),
    };
  }
  return q;
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
    // 阶段 6 · U2：存下的限额重置两次（一次十天后到期、一次不过期），Credits 余额没有。
    credits_balance: null,
    banked_resets: [new Date((now + 10 * 86400) * 1000).toISOString(), null],
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
          remaining_implied: false,
        },
        {
          label: "Claude Sonnet 4.6",
          model_id: 0,
          remaining: 1,
          reset_at: demoMinute(now + 5 * 86400),
          reset_epoch: now + 5 * 86400,
          tags: [],
          remaining_implied: false,
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
/**
 * GPT 界面语言的演示：`codex_locale_set` 翻它；默认那份只在别处起的那份没开着时才跟着变。
 * 第二个槽位一开始是 GPT 自己写坏了的那种（`[desktop]` 写了两遍，它读不到里面任何桌面设置），
 * 设中文时顺手修掉 —— 演示那一行怎么显示。
 */
let codexZh = false;
let codexZhDefault = false;
let codexZhBroken = true;
function codexLocaleStatus(): CodexLocaleStatus {
  return {
    enabled: codexZh,
    homes: [
      ...codexSlots.map((s, i) => {
        const broken = i === 1 && codexZhBroken;
        return {
          label: `槽位「${s.label}」`,
          config: `${HOME}\\AppData\\Local\\ClaudeIpGate\\codex-accounts\\${s.id}\\home\\config.toml`,
          current: codexZh && !broken ? "zh-CN" : null,
          error: null,
          repair: broken ? "[desktop]" : null,
          adopted: codexZh ? "zh-CN" : "en-US",
          is_default: false,
        };
      }),
      {
        label: `默认（${HOME}\\.codex，开始菜单起的 GPT 用它）`,
        config: `${HOME}\\.codex\\config.toml`,
        current: codexZhDefault ? "zh-CN" : null,
        error: null,
        repair: null,
        adopted: "en-US",
        is_default: true,
      },
    ],
    ours_running: codexLaunched ? 1 : 0,
    foreign_running: codexForeign ? 1 : 0,
    processes_error: null,
    egress_running: egressRunning,
  };
}
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
            reset_passed: false,
          },
        },
        secondary: {
          name: "7 天",
          window_minutes: 10080,
          window: {
            used: 77,
            resets_at: new Date(Date.now() + 3.2 * 86400_000).toISOString(),
            estimated: false,
            reset_passed: false,
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
  // 一键中文（2026-09-25）：只在演示里翻状态，不碰任何文件。别处起的那份开着时默认那份跳过。
  if (cmd === "codex_locale_status") return codexLocaleStatus();
  if (cmd === "codex_locale_set") {
    const on = !!args.enabled;
    const wasOurs = !!codexLaunched;
    const repairedNow = on && codexZhBroken;
    codexZh = on;
    if (on) codexZhBroken = false;
    if (!codexForeign) codexZhDefault = on;
    const lines = [
      ...codexSlots.map(
        (s, i) =>
          `槽位「${s.label}」：${on ? "已设为中文。" : "已撤掉中文，回到 GPT 自己的默认。"}${
            repairedNow && i === 1
              ? "顺手并掉了 GPT 自己写重复的 [desktop]（它原来因此读不到这份里的任何桌面设置），没改任何值。"
              : ""
          }`,
      ),
      codexForeign
        ? `默认（${HOME}\\.codex，开始菜单起的 GPT 用它）：别处起的 GPT 正开着，这一份这次没改（它只在启动时读这一项，开着改了也不生效）。完全退出它（托盘里也退）再点一次。`
        : `默认（${HOME}\\.codex，开始菜单起的 GPT 用它）：${on ? "已设为中文。" : "已撤掉中文，回到 GPT 自己的默认。"}`,
    ];
    const outcome: CodexLocaleOutcome = {
      changed: codexSlots.length + (codexForeign ? 0 : 1),
      already: 0,
      repaired: repairedNow ? 1 : 0,
      skipped: codexForeign ? 1 : 0,
      default_busy: codexForeign,
      lines,
      reopened: wasOurs ? "已按当前槽位重新打开 GPT 桌面端。" : null,
    };
    return outcome;
  }
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
