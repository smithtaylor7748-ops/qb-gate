/**
 * 本地日历上的日期串 `YYYY-MM-DD`。
 *
 * # ⛔ 不经过 UTC
 *
 * 用量明细页原来这样一天一天往后数：
 *
 * ```ts
 * new Date(`${day}T00:00:00`).toISOString().slice(0, 10)
 * ```
 *
 * 本地零点换成 UTC 再截前十位 —— 在 UTC 以东（东八区就是）零点对应的是**前一天**的
 * 16:00Z，于是整张趋势图错一天，最后「今天」那一格掉出去。作者的机器在 UTC−4，
 * 看不出来（CLAUDE.md「这是个开源项目：别人的机器跟你的不一样」）。
 *
 * 后端给的日期也是本地日历（`tokens.rs` 按本机时区切天），两边都按本地算才对得上。
 */

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

/** 一个 `Date` 在本地日历上是哪一天。 */
export function ymd(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** 今天（本地日历）。 */
export function today(): string {
  return ymd(new Date());
}

/**
 * `YYYY-MM-DD` 往后挪 `n` 天（负数往前）。按本地日历的年月日构造，
 * 跨月、跨年、夏令时那一天都不会多一天或少一天。
 */
export function addDays(day: string, n: number): string {
  const [y, m, d] = day.split("-").map(Number);
  return ymd(new Date(y, m - 1, d + n));
}

/**
 * 从 `first` 到 `last`（含两头）的每一天。最多 `cap` 天，防一个坏日期把循环拖成死循环。
 *
 * 超过上限时留的是**最近**的 `cap` 天（2026-09-25）：原来从 `first` 往后数满就停，
 * 历史超过 400 天时，「全部」那张图正好把今天截掉。
 */
export function daysFrom(first: string, last: string, cap = 400): string[] {
  const earliest = addDays(last, -(cap - 1));
  const start = first < earliest ? earliest : first;
  const out: string[] = [];
  for (let d = start; d <= last && out.length < cap; d = addDays(d, 1)) {
    out.push(d);
  }
  return out;
}

/** 含今天的最近 `days` 天，旧到新。 */
export function lastDays(days: number, end: string = today()): string[] {
  return daysFrom(addDays(end, -(days - 1)), end);
}

/** `2026-09-24` → `09-24`。 */
export function shortDay(day: string): string {
  return day.slice(5);
}
