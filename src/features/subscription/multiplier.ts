/**
 * 「换算成中转站倍率」的算式，纯函数，不碰界面。
 *
 * 中转站的倍率按它**站内的额度**算：常见的充值口径是 1 元 = 1 美元额度，
 * 所以 1× 就是「每 $1 官方牌价的用量付 1 元」。把官方订阅也按这个口径算：
 * `月价折成元 ÷ 一个月能用到的 API 等值（美元）`。
 * 使用者给的算式就是这一条：「开会员用了多少钱，除以订阅一个月能总共用多少刀」。
 *
 * 官方订阅付的是真美元，要先折成元才能跟中转站放在一把尺子上 ——
 * 2026-09-24 使用者定的：按 1 美元 = 7 元，界面上标明「1:7」、写明是往贵了取的
 * （实际汇率一般在 6.8 左右）。在这之前是美元直接除美元，官方各档因此显得便宜了 7 倍。
 *
 * 所有入口对非法输入（空、非数字、≤ 0）返回 `null`，界面上显示占位，
 * 绝不显示 `NaN` / `Infinity`。
 */

import type { Tone } from "../../ui";
import { monthlyValue, type Plan } from "./data";

/** 官方订阅的美元折成元用的汇率（使用者定的 1:7，往贵了取）。 */
export const YUAN_PER_USD = 7;
/** 实际汇率大概在哪。只进说明文字，不参与计算。 */
export const YUAN_PER_USD_TYPICAL = 6.8;

/** 把输入框里的字符串读成正数；读不出来就是 `null`。允许千分位逗号与货币符号。 */
export function parseAmount(raw: string): number | null {
  const cleaned = raw.replace(/[$¥￥元,，\s]/g, "");
  if (cleaned === "") return null;
  const n = Number(cleaned);
  if (!Number.isFinite(n) || n <= 0) return null;
  return n;
}

/** 等效倍率 = 花的钱（元）÷ 一个月能用到的 API 等值（美元）。 */
export function effectiveMultiplier(
  spendYuan: number,
  monthlyValueUsd: number,
): number | null {
  if (!Number.isFinite(spendYuan) || !Number.isFinite(monthlyValueUsd))
    return null;
  if (spendYuan <= 0 || monthlyValueUsd <= 0) return null;
  return spendYuan / monthlyValueUsd;
}

/** 官方订阅的等效倍率：美元月价按 1:7 折成元，再除以一个月能用到的 API 等值。 */
export function officialMultiplier(
  priceUsd: number,
  monthlyValueUsd: number,
): number | null {
  return effectiveMultiplier(priceUsd * YUAN_PER_USD, monthlyValueUsd);
}

/**
 * 官方某一档的等效倍率（网页月价，按 1:7）；帖子里没有这一档的额度就是 `null`。
 * 价目表、刻度条、首页那两格、计算器都用它 —— 同一档在页面上只许有一个数。
 */
export function planMultiplier(plan: Plan): number | null {
  const monthly = monthlyValue(plan);
  return monthly === null ? null : officialMultiplier(plan.webMonthly, monthly);
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
 * 倍率的语气。按 1:7 折算后官方各档落在 0.07～0.29，逆向流量中转常见 0.05～0.3 ——
 * **同一个价位**（逆向中转卖的本来就是别人的订阅），所以价钱分不开这两种，
 * 颜色只按价钱说话：≤ 0.3 跟自己订阅一个价位；0.3～0.8 比订阅贵；官转（0.8 起）往上是红的。
 */
export function multiplierTone(m: number): Tone {
  if (m <= 0.3) return "ok";
  if (m < 0.8) return "warn";
  return "danger";
}

/**
 * `0.025×`。小于 1 保留三位，大于等于 1 保留两位，去掉尾随的零。
 * 按四舍五入进位：0.0875（$200 × 7 ÷ $16,000）在二进制里略小于 0.0875，
 * 直接 `toFixed(3)` 会给「0.087」，跟读者心算的 0.088 对不上。
 */
export function fmtMultiplier(m: number): string {
  const digits = m >= 1 ? 2 : 3;
  const f = 10 ** digits;
  const rounded = Math.round((m + Number.EPSILON) * f) / f;
  const s = rounded.toFixed(digits).replace(/\.?0+$/, "");
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

/** `1,400 元`；小数只在不是整数时显示两位。 */
export function fmtYuan(n: number): string {
  const rounded = Math.round(n * 100) / 100;
  const isInt = Number.isInteger(rounded);
  return `${rounded.toLocaleString("en-US", {
    minimumFractionDigits: isInt ? 0 : 2,
    maximumFractionDigits: 2,
  })} 元`;
}

/** `12.5 倍`；大于等于 10 取整。 */
export function fmtTimes(n: number): string {
  if (n >= 10) return `${Math.round(n)} 倍`;
  return `${(Math.round(n * 10) / 10).toString()} 倍`;
}
