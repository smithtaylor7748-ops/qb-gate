import { createContext, useContext } from 'react';
import type { Progress, Risk, StepState } from './api';
import type { StepId } from './steps';

/**
 * 页面标识。
 *
 * 「中文环境识别」是新拆出来的独立页 —— 旧 `Environment.tsx` 391 行塞了五件事
 * （软件检测 / 安装 / 升级 / 时区 / 中文环境识别），而中文环境识别本身是一个
 * 完整独立的话题（10 项加权指纹，背后 475 行 `signals.ts`）。
 * 它有自己的页面，但**没有自己的进度步骤**，见 `steps.ts` 的说明。
 */
export type PageId =
  | 'home'
  | 'purity'
  | 'dns'
  | 'signals'
  | 'iplock'
  | 'accounts'
  | 'environment'
  | 'plugins'
  | 'relay'
  | 'settings';

export interface Nav {
  page: PageId;
  go: (p: PageId) => void;
  progress: Progress;
  /** 记录某一步的结果。写进 Rust 侧的 progress.json，返回后替换整个 Progress。 */
  mark: (id: StepId, state: StepState, risk: Risk, detail: string) => Promise<void>;
}

export const NavCtx = createContext<Nav | null>(null);

export function useNav(): Nav {
  const ctx = useContext(NavCtx);
  if (!ctx) throw new Error('useNav 必须在 <NavCtx.Provider> 里用');
  return ctx;
}
