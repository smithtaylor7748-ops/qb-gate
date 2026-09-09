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
  PluginState,
  Risk,
  StepState,
  UpgradeAction,
  InstallTarget,
} from '../lib/api';

export const TARGET_KIND_LABEL: Record<string, string> = {
  Cli: 'CLI 主副本',
  CliVersioned: '版本化副本',
  DesktopStub: '桌面端存根',
  StaleCopy: '升级残留',
  CodexCli: 'Codex CLI',
};

/** 每类副本一句话说明，鼠标停上去显示。 */
export const TARGET_KIND_HINT: Record<string, string> = {
  Cli: '官方安装脚本或 winget 落下的主副本',
  CliVersioned: '%APPDATA%\\Claude\\claude-code\\<版本>\\ 下的副本，每个版本一份，全都要锁',
  DesktopStub: '桌面端的 Squirrel 存根，开始菜单的快捷方式指向它',
  StaleCopy: '升级留下的旧副本，没有执行锁，是能绕过门禁的入口',
  CodexCli: 'Codex CLI。只有在设置里打开「Codex 也归门禁管」之后才会出现在这里',
};

export const CHECK_LABEL: Record<Check, string> = {
  Pass: '通过',
  Fail: '不通过',
  Unknown: '未知',
};

export const CHANNEL_LABEL: Record<Channel, string> = {
  latest: '最新版',
  stable: '稳定版',
};

export const EVIDENCE_LABEL: Record<Evidence, string> = {
  AnthropicSigned: 'Anthropic 签名',
  BridgeAndDataDir: '桥接 + 数据目录',
};

export const UPGRADE_ACTION_LABEL: Record<UpgradeAction, string> = {
  fresh_install: '尚未安装',
  upgrade: '可升级',
  up_to_date: '已是最新',
  would_downgrade: '渠道版本更旧',
  unknown: '判断不了',
};

export const PLUGIN_STATE_LABEL: Record<PluginState, string> = {
  missing: '依赖不齐',
  ready: '就绪',
  running: '运行中',
  broken: '状态异常',
};

export const LAUNCH_TARGET_LABEL: Record<string, string> = {
  'claude-code': 'Claude Code',
  'claude-desktop': 'Claude 桌面端',
  codex: 'Codex',
};

export const INSTALL_TARGET_LABEL: Record<InstallTarget, string> = {
  'claude-code': 'Claude Code',
  'claude-desktop': 'Claude 桌面端',
  codex: 'Codex CLI',
};

export const RISK_LABEL: Record<Risk, string> = {
  unknown: '未检测',
  low: '通过',
  medium: '注意',
  high: '高危',
};

export const STEP_STATE_LABEL: Record<StepState, string> = {
  pending: '未检测',
  passed: '已通过',
  failed: '未通过',
  skipped: '已跳过',
};

// ---------------------------------------------------------------- 语气

export type Tone = 'default' | 'ok' | 'warn' | 'danger' | 'accent';

export const RISK_TONE: Record<Risk, Tone> = {
  unknown: 'default',
  low: 'ok',
  medium: 'warn',
  high: 'danger',
};

export const CHECK_TONE: Record<Check, Tone> = {
  Pass: 'ok',
  Fail: 'danger',
  Unknown: 'default',
};

export const PLUGIN_STATE_TONE: Record<PluginState, Tone> = {
  missing: 'default',
  ready: 'ok',
  running: 'ok',
  broken: 'danger',
};

export const UPGRADE_ACTION_TONE: Record<UpgradeAction, Tone> = {
  fresh_install: 'default',
  upgrade: 'warn',
  up_to_date: 'ok',
  would_downgrade: 'danger',
  unknown: 'default',
};

// ---------------------------------------------------------------- 格式化

export function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/** 剩余天数。负数是已过期 —— 过期账户**仍然可以切换**，见档案 §4.8。 */
export function fmtDaysLeft(days: number | null | undefined): string {
  if (days === null || days === undefined) return '未知';
  if (days < 0) return '凭证已过期';
  if (days === 0) return '今天到期';
  return `剩 ${days} 天`;
}
