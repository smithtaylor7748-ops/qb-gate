import type { Risk, StepState } from './api';

/**
 * 侧栏的五个菜单**本身就是引导步骤**，没有单独的「新手引导」页。
 *
 * 导航是自由的 —— 随便点，只标状态；每行右侧标风险度。
 * id 必须与 Rust 侧 `progress.rs` 的 STEP_IDS 一一对应，改名要两边一起改。
 */
export const STEPS = [
  { id: 'purity', label: 'IP 纯净度', blurb: '三项硬指标，缺一不可' },
  { id: 'environment', label: '环境与安装', blurb: '检测、清理、安装 Claude' },
  { id: 'dns', label: 'DNS 泄露', blurb: '简易通过与高级通过' },
  { id: 'iplock', label: 'IP 锁', blurb: '白名单、执行锁与看门狗' },
  { id: 'accounts', label: '账户与启动', blurb: '登录、切换、验证后启动' },
] as const;

export type StepId = (typeof STEPS)[number]['id'];

export const ALWAYS_AVAILABLE = [
  { id: 'relay', label: '中转站' },
  { id: 'settings', label: '设置' },
] as const;

export const RISK_LABEL: Record<Risk, string> = {
  unknown: '未检测',
  low: '通过',
  medium: '注意',
  high: '高危',
};

export const STATE_MARK: Record<StepState, string> = {
  pending: '',
  passed: '✓',
  failed: '!',
  skipped: '–',
};

export function nextStep(id: StepId): StepId | null {
  const i = STEPS.findIndex((s) => s.id === id);
  return i >= 0 && i < STEPS.length - 1 ? STEPS[i + 1].id : null;
}

export function prevStep(id: StepId): StepId | null {
  const i = STEPS.findIndex((s) => s.id === id);
  return i > 0 ? STEPS[i - 1].id : null;
}

export function stepLabel(id: string): string {
  return STEPS.find((s) => s.id === id)?.label ?? id;
}

export function stepIndex(id: string): number {
  return STEPS.findIndex((s) => s.id === id) + 1;
}
