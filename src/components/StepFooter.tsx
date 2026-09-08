import type { StepApi, PageId } from '../App';
import { nextStep, prevStep, stepLabel, type StepId } from '../lib/steps';
import type { Risk } from '../lib/api';

interface Props extends StepApi {
  step: StepId;
  /** 本步当前的判定，决定「下一步」按钮记什么状态。 */
  risk: Risk;
  detail: string;
  /** 未检测完时禁用「下一步」，但**永远不禁用「强制跳过」**。 */
  canAdvance: boolean;
}

/**
 * 每个步骤页底部的导航条。
 *
 * 强制跳过必须始终可用 —— 这是用户明确要的：任何一步都能跳过去。
 * 但跳过会记成 `skipped`（区别于「还没做」），侧栏亮黄灯，
 * 总览上显示「跳过 N 项」，不让它变成静默的坑。
 */
export default function StepFooter({
  step,
  risk,
  detail,
  canAdvance,
  mark,
  go,
}: Props) {
  const prev = prevStep(step);
  const next = nextStep(step);

  async function advance() {
    await mark(step, risk === 'high' ? 'failed' : 'passed', risk, detail);
    if (next) go(next as PageId);
    else go('dashboard');
  }

  async function skip() {
    await mark(step, 'skipped', risk, detail || '用户强制跳过，未完成检测');
    if (next) go(next as PageId);
    else go('dashboard');
  }

  return (
    <div className="footer">
      <div>
        {prev ? (
          <button className="btn" onClick={() => go(prev as PageId)}>
            ← {stepLabel(prev)}
          </button>
        ) : (
          <span className="notice">这是第一步</span>
        )}
      </div>
      <div>
        <button className="btn" onClick={skip}>
          强制跳过
        </button>
        <button className="btn primary" onClick={advance} disabled={!canAdvance}>
          {next ? `下一步：${stepLabel(next)} →` : '完成，回到总览'}
        </button>
      </div>
    </div>
  );
}
