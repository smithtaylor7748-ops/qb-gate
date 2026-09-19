import { Pill } from "../../ui";
import {
  PLANS,
  RELAY_BANDS,
  REVIEWED_ON,
  VENDOR_LABEL,
  type Plan,
} from "./data";
import { effectiveMultiplier, fmtMultiplier, fmtUsd } from "./multiplier";

/**
 * 官方价目表 + 「换算成中转站倍率」的对照条。
 *
 * 等效倍率 = 网页月价 ÷ SemiAnalysis 实测的 API 等值上限。表格在 780px 以下
 * 由 CSS 堆成卡片（每格前面带列名），不横向滚。
 */

function multiplierOf(plan: Plan): number | null {
  return plan.apiValue === null
    ? null
    : effectiveMultiplier(plan.webMonthly, plan.apiValue);
}

export function PlanTable() {
  return (
    <div className="qb-sub-table-wrap">
      <table className="qb-sub-table">
        <thead>
          <tr>
            <th scope="col">套餐</th>
            <th scope="col">网页价 / 月</th>
            <th scope="col">iOS 内购 / 月</th>
            <th scope="col">实测 API 等值 / 月（上限）</th>
            <th scope="col">等效倍率</th>
          </tr>
        </thead>
        <tbody>
          {PLANS.map((p) => {
            const m = multiplierOf(p);
            return (
              <tr key={p.id}>
                <td data-col="套餐">
                  <span className="qb-sub-plan-name">{p.name}</span>
                  <span className="qb-sub-plan-vendor">
                    {VENDOR_LABEL[p.vendor]}
                  </span>
                  {p.note && <span className="qb-sub-plan-note">{p.note}</span>}
                </td>
                <td data-col="网页价 / 月" className="qb-sub-num">
                  {fmtUsd(p.webMonthly)}
                </td>
                <td data-col="iOS 内购 / 月" className="qb-sub-num">
                  {fmtUsd(p.iosMonthly)}
                  {p.iosMonthly > p.webMonthly && (
                    <Pill tone="warn">贵 25%</Pill>
                  )}
                </td>
                <td data-col="实测 API 等值 / 月" className="qb-sub-num">
                  {p.apiValue === null ? (
                    <span className="qb-sub-muted">未实测</span>
                  ) : (
                    `≈ ${fmtUsd(p.apiValue)}`
                  )}
                </td>
                <td data-col="等效倍率" className="qb-sub-num">
                  {m === null ? (
                    <span className="qb-sub-muted">—</span>
                  ) : (
                    <strong className="text-ok">{fmtMultiplier(m)}</strong>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <p className="notice">
        「实测 API 等值」是 SemiAnalysis 2026 年 6
        月买下每档套餐、跑长任务跑到周限额、 再按 API 牌价折算出来的
        <strong>上限</strong>
        ；日常用不到上限时倍率会比这高，但仍远低于任何中转站。价格均不含税。复核于{" "}
        {REVIEWED_ON}。
      </p>
    </div>
  );
}

const SCALE_MIN = 0.01;
const SCALE_MAX = 2;

/** 对数刻度：0.01× 在最左，2× 在最右。 */
function pct(x: number): number {
  const lo = Math.log10(SCALE_MIN);
  const hi = Math.log10(SCALE_MAX);
  const v = Math.min(Math.max(x, SCALE_MIN), SCALE_MAX);
  return ((Math.log10(v) - lo) / (hi - lo)) * 100;
}

export function MultiplierMeter() {
  const plans = PLANS.filter((p) => p.apiValue !== null)
    .map((p) => ({ plan: p, m: multiplierOf(p)! }))
    .sort((a, b) => a.m - b.m);

  return (
    <div className="qb-sub-meter">
      {plans.map(({ plan, m }) => (
        <div className="qb-sub-meter-row" key={plan.id}>
          <span className="qb-sub-meter-label">{plan.name}</span>
          <span className="qb-sub-meter-track" aria-hidden="true">
            <span
              className="qb-sub-meter-fill qb-sub-meter-fill--ok"
              style={{ width: `${pct(m)}%` }}
            />
          </span>
          <span className="qb-sub-meter-value text-ok">{fmtMultiplier(m)}</span>
        </div>
      ))}
      {RELAY_BANDS.map((b) => (
        <div className="qb-sub-meter-row" key={b.id}>
          <span className="qb-sub-meter-label">{b.label}</span>
          <span className="qb-sub-meter-track" aria-hidden="true">
            <span
              className={`qb-sub-meter-fill qb-sub-meter-fill--${
                b.id === "reverse" ? "warn" : "danger"
              }`}
              style={{
                left: `${pct(b.min)}%`,
                width: `${pct(b.max) - pct(b.min)}%`,
              }}
            />
          </span>
          <span
            className={
              b.id === "reverse"
                ? "qb-sub-meter-value text-warn"
                : "qb-sub-meter-value text-danger"
            }
          >
            {fmtMultiplier(b.min)}～{fmtMultiplier(b.max)}
          </span>
        </div>
      ))}
      <p className="notice">
        横轴是对数刻度，越靠左越便宜。倍率 = 你付的钱 ÷ 同样用量按官方 API
        牌价要付的钱，0.1×
        就是官方牌价的一折。官方订阅按同一口径算出来，比最便宜的逆向中转还低，而且没有后面那六条问题。
      </p>
    </div>
  );
}
