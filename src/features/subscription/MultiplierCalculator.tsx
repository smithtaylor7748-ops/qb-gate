import { useId, useState } from "react";
import { Calculator } from "lucide-react";
import { Card, Field, Pill } from "../../ui";
import { PLANS } from "./data";
import {
  effectiveMultiplier,
  fmtMultiplier,
  fmtTimes,
  fmtUsd,
  multiplierTone,
  parseAmount,
  timesMoreExpensive,
  toUsd,
} from "./multiplier";

const CUSTOM = "custom";
const DEFAULT_PLAN = "claude-max-20";

function planValue(id: string): string {
  const p = PLANS.find((x) => x.id === id);
  return p && p.apiValue !== null ? String(p.apiValue) : "";
}

/**
 * 「我这笔钱，相当于中转站什么倍率」。
 *
 * 使用者给的算式：开会员花的钱 ÷ 订阅一个月能总共用多少刀。
 * 分母默认取所选官方套餐的实测 API 等值，可以手改；分子支持按汇率折成美元。
 * 再多给一个可选项：中转站实际给了多少额度 —— 有了它能算出「比官方贵几倍」。
 */
export function MultiplierCalculator() {
  const [amount, setAmount] = useState("");
  const [rate, setRate] = useState("1");
  const [planId, setPlanId] = useState(DEFAULT_PLAN);
  const [value, setValue] = useState(planValue(DEFAULT_PLAN));
  const [quota, setQuota] = useState("");
  const selectId = useId();

  const plan = PLANS.find((p) => p.id === planId) ?? null;
  const parsedAmount = parseAmount(amount);
  const parsedRate = parseAmount(rate);
  const spendUsd =
    parsedAmount !== null && parsedRate !== null
      ? toUsd(parsedAmount, parsedRate)
      : null;
  const monthly = parseAmount(value);
  const m =
    spendUsd !== null && monthly !== null
      ? effectiveMultiplier(spendUsd, monthly)
      : null;
  const officialM =
    plan && plan.apiValue !== null
      ? effectiveMultiplier(plan.webMonthly, plan.apiValue)
      : null;
  const quotaUsd = parseAmount(quota);
  const relayM =
    spendUsd !== null && quotaUsd !== null
      ? effectiveMultiplier(spendUsd, quotaUsd)
      : null;
  const times =
    relayM !== null && officialM !== null
      ? timesMoreExpensive(relayM, officialM)
      : null;

  const amountError =
    amount.trim() !== "" && parsedAmount === null ? "请填一个正数" : undefined;
  const rateError =
    rate.trim() !== "" && parsedRate === null ? "汇率要是正数" : undefined;
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
        填上你为「会员」付的钱，再除以一个月能用到多少美元的用量，得到的就是中转站口径的倍率。分母默认取所选官方套餐的实测上限，可以改。
      </p>
      <div className="qb-sub-calc">
        <div className="qb-sub-calc-inputs">
          <div className="qb-sub-calc-row">
            <Field
              label="你为会员付了多少钱（每月）"
              error={amountError}
              hint="随便什么货币都行，下一格填汇率"
              className="qb-sub-calc-grow"
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
            <Field
              label="汇率（1 美元 = ？）"
              error={rateError}
              hint="付的是美元就填 1"
              className="qb-sub-calc-rate"
            >
              {(p) => (
                <input
                  {...p}
                  className="input"
                  inputMode="decimal"
                  value={rate}
                  onChange={(e) => setRate(e.target.value)}
                />
              )}
            </Field>
          </div>

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
              {PLANS.filter((p) => p.apiValue !== null).map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} —— {fmtUsd(p.webMonthly)}/月，实测上限 ≈{" "}
                  {fmtUsd(p.apiValue!)}
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
                ? `默认是 ${plan.name} 的实测上限，可以按你的实际用量改小`
                : "按你自己的用量估"
            }
          >
            {(p) => (
              <input
                {...p}
                className="input"
                inputMode="decimal"
                placeholder="例如 8000"
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
              <Pill tone={multiplierTone(m)}>
                {multiplierTone(m) === "ok"
                  ? "和官方订阅一个水平"
                  : multiplierTone(m) === "warn"
                    ? "逆向流量中转的价位"
                    : "比官方 API 牌价还贵"}
              </Pill>
              <p className="qb-sub-calc-line">
                你每月付 {fmtUsd(spendUsd!)}，按「一个月能用 {fmtUsd(monthly!)}
                」折算，相当于中转站倍率 {fmtMultiplier(m)}。
              </p>
              {plan && officialM !== null && (
                <p className="qb-sub-calc-line">
                  官方 {plan.name} 自己是 {fmtUsd(plan.webMonthly)} ÷{" "}
                  {fmtUsd(plan.apiValue!)} = {fmtMultiplier(officialM)}。
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
                      {fmtUsd(spendUsd! / officialM)} 的用量
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
