import { useId, useState } from "react";
import { Calculator } from "lucide-react";
import { Card, Field, Pill } from "../../ui";
import { PLANS, WEEKS_PER_MONTH, monthlyValue } from "./data";
import {
  YUAN_PER_USD,
  effectiveMultiplier,
  fmtMultiplier,
  fmtTimes,
  fmtUsd,
  fmtYuan,
  multiplierTone,
  parseAmount,
  planMultiplier,
  timesMoreExpensive,
} from "./multiplier";

const CUSTOM = "custom";
const DEFAULT_PLAN = "claude-max-20";

function planValue(id: string): string {
  const p = PLANS.find((x) => x.id === id);
  const monthly = p ? monthlyValue(p) : null;
  return monthly === null ? "" : String(monthly);
}

/** 结果旁边那枚标签：只按价钱说话（`multiplierTone` 的三段）。 */
function toneLabel(m: number): string {
  const tone = multiplierTone(m);
  if (tone === "ok") return "和自己订阅官方一个价位";
  if (tone === "warn") return "比自己订阅官方贵";
  return m > YUAN_PER_USD
    ? "比官方 API 牌价还贵"
    : "官转的价位，比自己订阅贵好几倍";
}

/**
 * 「我这笔钱，相当于中转站什么倍率」。
 *
 * 使用者给的算式：开会员花的钱 ÷ 订阅一个月能总共用多少刀。
 * 花的钱按元填 —— 中转站的倍率本来就是「每 $1 牌价的用量付几元」（2026-09-24 起，
 * 原来那格「汇率」随之去掉）。分母默认取所选官方套餐的周额度中间值 × 4（`monthlyValue`），
 * 可以手改；官方那一档按 1:7 折成元（`planMultiplier`），跟价目表同一个数。
 * 再多给一个可选项：中转站实际给了多少额度 —— 有了它能算出「比官方贵几倍」。
 */
export function MultiplierCalculator() {
  const [amount, setAmount] = useState("");
  const [planId, setPlanId] = useState(DEFAULT_PLAN);
  const [value, setValue] = useState(planValue(DEFAULT_PLAN));
  const [quota, setQuota] = useState("");
  const selectId = useId();

  const plan = PLANS.find((p) => p.id === planId) ?? null;
  const spend = parseAmount(amount);
  const monthly = parseAmount(value);
  const m =
    spend !== null && monthly !== null
      ? effectiveMultiplier(spend, monthly)
      : null;
  const planMonthly = plan ? monthlyValue(plan) : null;
  const officialM = plan ? planMultiplier(plan) : null;
  const quotaUsd = parseAmount(quota);
  const relayM =
    spend !== null && quotaUsd !== null
      ? effectiveMultiplier(spend, quotaUsd)
      : null;
  const times =
    relayM !== null && officialM !== null
      ? timesMoreExpensive(relayM, officialM)
      : null;

  const amountError =
    amount.trim() !== "" && spend === null ? "请填一个正数" : undefined;
  const valueError =
    value.trim() !== "" && monthly === null ? "请填一个正数" : undefined;
  const quotaError =
    quota.trim() !== "" && quotaUsd === null ? "请填一个正数" : undefined;

  return (
    <Card
      title="换算成中转站倍率"
      icon={<Calculator size={16} aria-hidden="true" />}
      className="qb-sub-calc-card"
    >
      <p className="sub qb-sub-calc-intro">
        填上你为「会员」付的钱（元），再除以一个月能用到多少美元的用量，得到的就是中转站口径的倍率
        —— 每 $1
        牌价的用量付几元。分母默认取所选官方套餐的周额度（网友估算中间值）×{" "}
        {WEEKS_PER_MONTH}，可以改；官方那一档的美元按 1:{YUAN_PER_USD} 折成元。
      </p>
      <div className="qb-sub-calc">
        <div className="qb-sub-calc-inputs">
          <Field
            label="你为会员付了多少钱（元 / 月）"
            error={amountError}
            hint={`付的是美元就先乘 ${YUAN_PER_USD}`}
          >
            {(p) => (
              <input
                {...p}
                className="input"
                inputMode="decimal"
                placeholder="例如 40"
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
              />
            )}
          </Field>

          <div>
            <label className="field-label" htmlFor={selectId}>
              按哪档官方套餐折算
            </label>
            <select
              id={selectId}
              className="input"
              value={planId}
              onChange={(e) => {
                setPlanId(e.target.value);
                setValue(planValue(e.target.value));
              }}
            >
              {PLANS.filter((p) => p.weeklyValue !== null).map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} —— {fmtUsd(p.webMonthly)}/月，周额度 ≈{" "}
                  {fmtUsd(p.weeklyValue!)}
                </option>
              ))}
              <option value={CUSTOM}>自己填一个月能用多少</option>
            </select>
          </div>

          <Field
            label="一个月总共能用多少刀（API 等值，美元）"
            error={valueError}
            hint={
              plan
                ? `默认是 ${plan.name} 的周额度中间值 × ${WEEKS_PER_MONTH}，可以按你的实际用量改`
                : "按你自己的用量估"
            }
          >
            {(p) => (
              <input
                {...p}
                className="input"
                inputMode="decimal"
                placeholder="例如 16000"
                value={value}
                onChange={(e) => setValue(e.target.value)}
              />
            )}
          </Field>

          <Field
            label="中转站给你的额度（美元，选填）"
            error={quotaError}
            hint="填了就能看出中转站比官方贵几倍"
          >
            {(p) => (
              <input
                {...p}
                className="input"
                inputMode="decimal"
                placeholder="例如 100"
                value={quota}
                onChange={(e) => setQuota(e.target.value)}
              />
            )}
          </Field>
        </div>

        <div className="qb-sub-calc-result" aria-live="polite">
          {m === null ? (
            <div className="qb-sub-calc-empty">
              <span className="qb-sub-calc-big qb-sub-muted">—</span>
              <p className="notice">填上金额，这里马上算出倍率。</p>
            </div>
          ) : (
            <>
              <span className="qb-sub-calc-k">等效倍率</span>
              <span
                className={`qb-sub-calc-big qb-sub-calc-big--${multiplierTone(m)}`}
              >
                {fmtMultiplier(m)}
              </span>
              <Pill tone={multiplierTone(m)}>{toneLabel(m)}</Pill>
              <p className="qb-sub-calc-line">
                你每月付 {fmtYuan(spend!)}，按「一个月能用 {fmtUsd(monthly!)}
                」折算，相当于中转站倍率 {fmtMultiplier(m)}。
              </p>
              {plan && officialM !== null && (
                <p className="qb-sub-calc-line">
                  官方 {plan.name} 自己是 {fmtUsd(plan.webMonthly)} ×{" "}
                  {YUAN_PER_USD} ÷ {fmtUsd(planMonthly!)} ={" "}
                  {fmtMultiplier(officialM)}（美元按 1:{YUAN_PER_USD} 折成元）。
                </p>
              )}
              {relayM !== null && (
                <p className="qb-sub-calc-line">
                  中转站给你 {fmtUsd(quotaUsd!)} 额度 → 实际倍率{" "}
                  <strong>{fmtMultiplier(relayM)}</strong>
                  {times !== null && officialM !== null && (
                    <>
                      ，是官方的 <strong>{fmtTimes(times)}</strong>
                      ；同样一笔钱按官方倍率能换到 ≈{" "}
                      {fmtUsd(spend! / officialM)} 的用量
                    </>
                  )}
                  。
                </p>
              )}
            </>
          )}
        </div>
      </div>
    </Card>
  );
}
