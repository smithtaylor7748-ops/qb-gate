/**
 * 「换算成中转站倍率」的算式，纯函数，不碰界面。
 *
 * 中转站的倍率 = 你付的钱 ÷ 同样用量按官方 API 牌价要付的钱。
 * 把官方订阅也按这个口径算：`价 ÷ 一个月能用到的 API 等值`。
 * 使用者给的算式就是这一条：「开会员用了多少钱，除以订阅一个月能总共用多少刀」。
 *
 * 所有入口对非法输入（空、非数字、≤ 0）返回 `null`，界面上显示占位，
 * 绝不显示 `NaN` / `Infinity`。
 */

import type { Tone } from "../../ui";

/** 把输入框里的字符串读成正数；读不出来就是 `null`。允许千分位逗号与美元号。 */
export function parseAmount(raw: string): number | null {
  const cleaned = raw.replace(/[$,，\s]/g, "");
  if (cleaned === "") return null;
  const n = Number(cleaned);
  if (!Number.isFinite(n) || n <= 0) return null;
  return n;
}

/** 本地货币 → 美元。汇率是「1 美元 = 多少本地货币」。 */
export function toUsd(amount: number, rate: number): number | null {
  if (!Number.isFinite(amount) || !Number.isFinite(rate)) return null;
  if (amount <= 0 || rate <= 0) return null;
  return amount / rate;
}

/** 等效倍率 = 花的钱 ÷ 一个月能用到的 API 等值。 */
export function effectiveMultiplier(
  spendUsd: number,
  monthlyValueUsd: number,
): number | null {
  if (!Number.isFinite(spendUsd) || !Number.isFinite(monthlyValueUsd))
    return null;
  if (spendUsd <= 0 || monthlyValueUsd <= 0) return null;
  return spendUsd / monthlyValueUsd;
}

/** 中转比官方贵几倍：中转倍率 ÷ 官方倍率。 */
export function timesMoreExpensive(
  relayMultiplier: number,
  officialMultiplier: number,
): number | null {
  if (!Number.isFinite(relayMultiplier) || !Number.isFinite(officialMultiplier))
    return null;
  if (relayMultiplier <= 0 || officialMultiplier <= 0) return null;
  return relayMultiplier / officialMultiplier;
}

/**
 * 倍率的语气：官方各档都落在 0.05 以下；逆向流量中转常见 0.05～0.3；
 * 官转 0.8 以上。三档颜色对应这三段。
 */
export function multiplierTone(m: number): Tone {
  if (m <= 0.05) return "ok";
  if (m <= 0.3) return "warn";
  return "danger";
}

/** `0.025×`。小于 1 保留三位，大于等于 1 保留两位，去掉尾随的零。 */
export function fmtMultiplier(m: number): string {
  const digits = m >= 1 ? 2 : 3;
  const s = m.toFixed(digits).replace(/\.?0+$/, "");
  return `${s}×`;
}

/** `$8,000`；小数只在不是整数时显示两位。 */
export function fmtUsd(n: number): string {
  const rounded = Math.round(n * 100) / 100;
  const isInt = Number.isInteger(rounded);
  return (
    "$" +
    rounded.toLocaleString("en-US", {
      minimumFractionDigits: isInt ? 0 : 2,
      maximumFractionDigits: 2,
    })
  );
}

/** `12.5 倍`；大于等于 10 取整。 */
export function fmtTimes(n: number): string {
  if (n >= 10) return `${Math.round(n)} 倍`;
  return `${(Math.round(n * 10) / 10).toString()} 倍`;
}
