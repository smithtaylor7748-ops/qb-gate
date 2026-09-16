/**
 * 后端枚举 → 中文。
 *
 * 旧界面直接把 serde 的名字甩给用户看：受管文件列表里一列 `Cli` / `CliVersioned`
 * / `DesktopStub` / `StaleCopy`，升级渠道写 `latest` / `stable`，一键关闭的证据
 * 写 `AnthropicSigned`。这些是给程序看的标识，不是给人看的词。
 *
 * **枚举值本身一个字都不能改** —— 它们是 Rust serde 的输出，改了就断线。
 * 要改的只是显示层，就在这个文件里。
 */

import type {
  Channel,
  Check,
  Evidence,
  ExternalMethod,
  InstallKind,
  KillRole,
  ManagedApp,
  PluginState,
  PolicyScope,
  PurgeAction,
  PurgeCategory,
  PurgeTarget,
  Risk,
  StepState,
  UpgradeAction,
  InstallTarget,
} from "../lib/api";

export const TARGET_KIND_LABEL: Record<string, string> = {
  Managed: "面板托管",
  ManagedVersion: "版本库留存",
  Cli: "CLI 主副本",
  CliVersioned: "桌面端带的副本",
  NativeVersion: "安装器版本库",
  EditorExtension: "编辑器扩展自带",
  DesktopStub: "桌面端存根",
  StaleCopy: "升级残留",
  CodexCli: "Codex CLI",
};

/** 每类副本一句话说明，鼠标停上去显示。 */
export const TARGET_KIND_HINT: Record<string, string> = {
  Managed: "面板自己从官方源装进托管目录的那份。启动时优先用它",
  ManagedVersion:
    "版本库里留着给回滚用的旧版本。只锁不启动 —— 它照样是完整可执行的",
  Cli: "官方安装器、winget、Scoop 或 PATH 上的 Claude Code",
  CliVersioned:
    "%APPDATA%\\Claude*\\claude-code\\<版本>\\ 下的副本，每个桌面端资料目录、每个版本各一份，全都要锁。⚠ Claude 桌面端的 Code 页每开一个新会话都要拉起它 —— 上锁之后那句 Claude Code couldn’t start 就是这么来的",
  NativeVersion:
    "~\\.local\\share\\claude\\versions\\<版本> —— 官方安装器存的每个版本都是一份完整的二进制，漏锁一份就是一个绕过入口",
  EditorExtension: "VS Code / Cursor 等编辑器里 Claude Code 扩展自带的那份",
  DesktopStub: "桌面端的 Squirrel 存根，开始菜单的快捷方式指向它",
  StaleCopy: "升级留下的旧副本，没有执行锁，是能绕过门禁的入口",
  CodexCli: "Codex CLI。只有在设置里打开「Codex 也归门禁管」之后才会出现在这里",
};

/** Claude Code 副本的来源。与 Rust `install::inventory::Kind` 一一对应。 */
export const INSTALL_KIND_LABEL: Record<InstallKind, string> = {
  managed: "面板托管",
  managed_version: "面板版本库",
  native: "官方安装器",
  native_version: "安装器版本库",
  winget: "winget",
  scoop: "Scoop",
  programs: "Programs\\Claude",
  path: "PATH 上的",
  desktop_managed: "桌面端带的",
  msix_managed: "MSIX 桌面端带的",
  npm: "npm",
  npm_native: "npm 包内",
  editor: "编辑器扩展",
  desktop_stub: "桌面端存根",
};

/** 外部副本的清法。 */
export const EXTERNAL_METHOD_LABEL: Record<ExternalMethod, string> = {
  npm: "npm 卸载",
  winget: "winget 卸载",
  scoop: "scoop 卸载",
  delete: "删除文件",
  registry: "删登记项",
};

export const MANAGED_APP_LABEL: Record<ManagedApp, string> = {
  "claude-code": "Claude Code",
  codex: "Codex CLI",
};

export const KILL_ROLE_LABEL: Record<KillRole, string> = {
  desktop: "桌面端",
  code: "Claude Code",
  bridge: "酒馆桥接",
};

export const CHECK_LABEL: Record<Check, string> = {
  Pass: "通过",
  Fail: "不通过",
  Unknown: "未知",
};

export const CHANNEL_LABEL: Record<Channel, string> = {
  latest: "最新版",
  stable: "稳定版",
};

export const EVIDENCE_LABEL: Record<Evidence, string> = {
  AnthropicSigned: "Anthropic 签名",
  BridgeAndDataDir: "桥接 + 数据目录",
  NpmPackage: "node + claude-code 包",
};

export const UPGRADE_ACTION_LABEL: Record<UpgradeAction, string> = {
  fresh_install: "尚未安装",
  upgrade: "可升级",
  up_to_date: "已是最新",
  would_downgrade: "渠道版本更旧",
  unknown: "判断不了",
  version_unreadable: "读不出版本",
};

export const PLUGIN_STATE_LABEL: Record<PluginState, string> = {
  missing: "依赖不齐",
  ready: "就绪",
  running: "运行中",
  broken: "状态异常",
};

/**
 * 浏览器策略读自哪一处。
 *
 * **必须显示出来**：面板写的是当前用户那一处，而整机策略优先级更高。
 * 不分开显示的话，会出现「面板说已经设好了、Chrome 却用着另一条」，
 * 而使用者看不出为什么。
 */
export const POLICY_SCOPE_LABEL: Record<PolicyScope, string> = {
  current_user: "当前用户",
  machine: "整机策略",
};

/**
 * 卸载目标的显示名。
 *
 * `Record<PurgeTarget, string>` 而不是随手写个数组：后端加一个目标时，
 * 这里漏掉就编译不过。软件页原来自己在文件里存了一份四元组来查这个名字，
 * 那一份还带着一个从来没人读的 `tag` 字段。
 */
export const PURGE_TARGET_LABEL: Record<PurgeTarget, string> = {
  "claude-code": "Claude Code",
  codex: "Codex CLI",
  "claude-desktop": "Claude 桌面端",
  chrome: "Google Chrome",
};

/** 完全卸载的分类。确认框按这个分段列，一段一段给使用者看。 */
export const PURGE_CATEGORY_LABEL: Record<PurgeCategory, string> = {
  program: "程序",
  version_store: "版本库",
  cache: "缓存与登记",
  config: "配置与会话",
  auth: "认证",
  env_path: "环境变量与 PATH",
  shell_line: "Shell 配置行",
  startup: "启动项",
  account_slot: "账户槽位",
  browser_data: "浏览器数据",
};

/**
 * 每一项**怎么处理**。界面必须显示出来。
 *
 * 「删文件」和「走 winget 卸载」代价完全不同：后者会连启动器与登记一起走，
 * 前者只动那一个文件。合并成一句「清除」，使用者就看不出这台机器会发生什么。
 */
export const PURGE_ACTION_LABEL: Record<PurgeAction, string> = {
  delete_file: "删除文件",
  delete_dir: "删除目录",
  npm_uninstall: "npm 卸载",
  winget_uninstall: "winget 卸载",
  scoop_uninstall: "scoop 卸载",
  registry_delete: "删注册表登记项",
  env_unset: "清除环境变量",
  path_entry_remove: "从 PATH 移除",
  shell_line_remove: "删 Shell 配置行",
  credential_delete: "删凭据管理器条目",
  startup_remove: "删启动项",
};

export const LAUNCH_TARGET_LABEL: Record<string, string> = {
  "claude-code": "Claude Code",
  "claude-desktop": "Claude 桌面端",
  codex: "Codex",
};

export const INSTALL_TARGET_LABEL: Record<InstallTarget, string> = {
  "claude-code": "Claude Code",
  "claude-desktop": "Claude 桌面端",
  codex: "Codex CLI",
};

export const RISK_LABEL: Record<Risk, string> = {
  unknown: "未检测",
  low: "通过",
  medium: "注意",
  high: "高危",
};

export const STEP_STATE_LABEL: Record<StepState, string> = {
  pending: "未检测",
  passed: "已通过",
  failed: "未通过",
  skipped: "已跳过",
};

// ---------------------------------------------------------------- 语气

export type Tone = "default" | "ok" | "warn" | "danger" | "accent";

export const RISK_TONE: Record<Risk, Tone> = {
  unknown: "default",
  low: "ok",
  medium: "warn",
  high: "danger",
};

export const CHECK_TONE: Record<Check, Tone> = {
  Pass: "ok",
  Fail: "danger",
  Unknown: "default",
};

export const PLUGIN_STATE_TONE: Record<PluginState, Tone> = {
  missing: "default",
  ready: "ok",
  running: "ok",
  broken: "danger",
};

export const UPGRADE_ACTION_TONE: Record<UpgradeAction, Tone> = {
  fresh_install: "default",
  upgrade: "warn",
  up_to_date: "ok",
  would_downgrade: "danger",
  // 装是装着的，但面板现在瞎着 —— 不能显示成中性状态。
  version_unreadable: "warn",
  unknown: "default",
};

// ---------------------------------------------------------------- 格式化

export function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/** 剩余天数。负数是已过期 —— 过期账户**仍然可以切换**，见档案 §4.8。 */
export function fmtDaysLeft(days: number | null | undefined): string {
  if (days === null || days === undefined) return "未知";
  if (days < 0) return "凭证已过期";
  if (days === 0) return "今天到期";
  return `剩 ${days} 天`;
}

/**
 * 「这份读数是什么时候测的」。
 *
 * DNS 与中文环境的结果会留到下次开面板（`ResourceOptions.persist`），
 * 所以界面上**必须**带时刻 —— 否则一份昨天的读数看起来跟刚测的一模一样，
 * 而中间你可能已经换过网。`null` 表示没有记录，调用方自己决定显示什么。
 */
export function fmtMeasured(at: number | null): string | null {
  if (at === null) return null;
  const min = Math.floor((Date.now() - at) / 60_000);
  if (min < 2) return "刚测的";
  if (min < 60) return `${min} 分钟前测的`;
  const d = new Date(at);
  const hhmm = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  if (d.toDateString() === new Date().toDateString()) return `测于 ${hhmm}`;
  return `测于 ${d.getMonth() + 1}-${String(d.getDate()).padStart(2, "0")} ${hhmm}`;
}
