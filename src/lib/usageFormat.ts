/**
 * 用量页的数字写法：官方账户的 `/usage` 和中转用量页 `/relays/usage` 共用（2026-10-26 从 `Usage.tsx` 抽出来）。
 * 两页的同一种数各写一份，迟早一边写「<$0.01」一边写「$0.00」。
 */

export const NUM = new Intl.NumberFormat("zh-CN");

/** `12.3K` / `4.5M`。 */
export function short(n: number): string {
  if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(2) + "B";
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
  return NUM.format(n);
}

/** ⛔ `null` → 「—」，**绝不显示 0%** —— 那是断言，没数据不是。 */
export function pct(v: number | null | undefined): string {
  return v == null ? "—" : `${Math.round(v * 100)}%`;
}

const USD = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

/** 美元。`null` → 「—」；不到一分钱的写「<$0.01」，不写成 $0.00。 */
export function usd(v: number | null | undefined): string {
  if (v == null) return "—";
  if (v > 0 && v < 0.01) return "<$0.01";
  return USD.format(v);
}

/** 坐标轴上的美元：去掉多余的零。 */
export function usdTick(v: number): string {
  if (v >= 1000) return `$${Number((v / 1000).toFixed(1))}K`;
  return `$${Number(v.toFixed(2))}`;
}
