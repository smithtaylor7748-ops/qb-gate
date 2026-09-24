import { Pill } from "../../ui";
import {
  PLANS,
  QUOTA_CAVEATS,
  RELAY_BANDS,
  REVIEWED_ON,
  VENDOR_LABEL,
  WEEKS_PER_MONTH,
} from "./data";
import {
  YUAN_PER_USD,
  YUAN_PER_USD_TYPICAL,
  fmtMultiplier,
  fmtTimes,
  fmtUsd,
  planMultiplier,
} from "./multiplier";

/**
 * 官方价目表 + 「换算成中转站倍率」的对照条。
 *
 * 周额度取 linux.do 那个帖子里网友估算区间的中间值（2026-09-24 使用者定的口径）；
 * 等效倍率 = 网页月价 × 7 ÷（周额度 × 4）—— 美元按 1:7 折成元，跟中转站
 * 「每 $1 牌价付几元」同一把尺子（同日使用者定的，见 `multiplier.ts` 文件头）。
 * 表格在 780px 以下由 CSS 堆成卡片（每格前面带列名），不横向滚。
 */

/** 列名与说明里的「1:7」。 */
const RATE_LABEL = `1:${YUAN_PER_USD}`;
/** 按实际汇率算，倍率还要再低多少（百分比，取整）。 */
const TYPICAL_LOWER_PCT = Math.round(
  (1 - YUAN_PER_USD_TYPICAL / YUAN_PER_USD) * 100,
);

/** `$2,000～3,000`；两头相同就只写一个数。 */
function fmtRange([lo, hi]: [number, number]): string {
  return lo === hi ? fmtUsd(lo) : `${fmtUsd(lo)}～${fmtUsd(hi).slice(1)}`;
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
            <th scope="col">周额度（网友估算中间值）</th>
            <th scope="col">等效倍率（{RATE_LABEL}）</th>
          </tr>
        </thead>
        <tbody>
          {PLANS.map((p) => {
            const m = planMultiplier(p);
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
                <td data-col="周额度（中间值）" className="qb-sub-num">
                  {p.weeklyValue === null || p.weeklyRange === null ? (
                    <span className="qb-sub-muted">没有数据</span>
                  ) : (
                    <>
                      ≈ {fmtUsd(p.weeklyValue)} / 周
                      <span className="qb-sub-plan-note">
                        帖子里 {fmtRange(p.weeklyRange)}
                        {p.fiveHourRange &&
                          ` · 5 小时 ${fmtRange(p.fiveHourRange)}`}
                      </span>
                    </>
                  )}
                </td>
                <td
                  data-col={`等效倍率（${RATE_LABEL}）`}
                  className="qb-sub-num"
                >
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
        「周额度」是 linux.do「国内外 AI 订阅性价比」帖里网友估算的每周能用到的
        API 牌价等值，本页取区间的<strong>中间值</strong>。等效倍率 = 网页月价按{" "}
        <strong>
          1 美元 = {YUAN_PER_USD} 元（{RATE_LABEL}）
        </strong>
        折成元 ÷ 一个月的 API 牌价等值，跟中转站「每 $1
        牌价的用量付几元」是同一个口径；一个月按 {WEEKS_PER_MONTH}{" "}
        个周限算（帖子说官方常重置，实际能用得更多）。
        <strong>{YUAN_PER_USD} 是往贵了取的</strong>，实际汇率一般在{" "}
        {YUAN_PER_USD_TYPICAL} 左右，按实际汇率算倍率还要再低约{" "}
        {TYPICAL_LOWER_PCT}%。日常用不满额度时倍率会比这高。价格均不含税。复核于{" "}
        {REVIEWED_ON}。
      </p>
      <ul className="notice qb-sub-caveats">
        {QUOTA_CAVEATS.map((c) => (
          <li key={c}>{c}</li>
        ))}
      </ul>
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
  const plans = PLANS.filter((p) => p.weeklyValue !== null)
    .map((p) => ({ plan: p, m: planMultiplier(p)! }))
    .sort((a, b) => a.m - b.m);
  const lo = plans[0].m;
  const hi = plans[plans.length - 1].m;
  const relay = RELAY_BANDS.find((b) => b.id === "official-relay")!;

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
        横轴是对数刻度，越靠左越便宜。倍率是中转站的口径：每 $1
        官方牌价的用量付几元（按常见的 1 元 = 1
        美元额度充值；充值比例不是这个的站要先换算），直接按 API
        牌价付美元折过来是 {YUAN_PER_USD}×。官方订阅的美元按 {RATE_LABEL}{" "}
        折成元，落在 {fmtMultiplier(lo)}～{fmtMultiplier(hi)}
        ：跟逆向中转一个价位（逆向中转卖的本来就是别人的订阅），官转是它的{" "}
        {fmtTimes(relay.min / hi).replace(" 倍", "")}～
        {fmtTimes(relay.max / lo)}；而且账号是你自己的，没有前面那六条问题。
      </p>
    </div>
  );
}
