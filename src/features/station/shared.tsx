/**
 * 中转站几个界面共用的那点东西：时间范围、请求量曲线、读数格式化。
 *
 * # 为什么单独一个文件
 *
 * 总览卡和请求日志弹窗用的是**同一套时间档**；完整统计柱状图在 StationUsageChart。
 * 各写一份的话，两处会慢慢漂开 —— 而漂开之后最难受的不是样式不一致，
 * 是同一段时间在两个地方算出不同的请求数，而没有任何地方说得清哪个对。
 *
 * ⛔ 颜色只引 `tokens.css` 的变量，不写 hex。
 */

import type { Client } from "../../lib/generated/Client";

/**
 * 有中转那一档的五个软件，分页的顺序就是这个顺序（中转站页、中转用量页共用；2026-10-26 从 `StationCenter` 挪过来）。
 * 阶段 4 的两档中转命令行排在后面，前三个分页的位置不变。
 */
export const STATION_CLIENTS: { id: Client; name: string; short: string }[] = [
  { id: "claude-code", name: "Claude Code", short: "Claude Code" },
  { id: "claude-desktop", name: "Claude 桌面端", short: "桌面端" },
  { id: "codex", name: "Codex 桌面端", short: "Codex 桌面端" },
  { id: "gemini-cli", name: "Gemini CLI", short: "Gemini CLI" },
  { id: "grok-build", name: "Grok Build", short: "Grok Build" },
];

/** 时间范围；全部必须作为无下界查询处理，不能把下列占位小时数当截断点。 */
export const RANGES = [
  { id: "1h", name: "近 1 小时", hours: 1 },
  { id: "24h", name: "近 24 小时", hours: 24 },
  { id: "7d", name: "近 7 天", hours: 24 * 7 },
  { id: "all", name: "全部", hours: 24 * 365 },
] as const;
export type RangeId = (typeof RANGES)[number]["id"];
export const rangeOf = (id: RangeId) => RANGES.find((r) => r.id === id)!;

export type GranId = "minute" | "hour" | "day";

export const dash = <span className="qb-st-dim">—</span>;

// ------------------------------------------------------------------ 小工具

/**
 * Token 数写成 `1.23M` / `45.6K`。
 *
 * 原样印一串七位数没人读得出量级，而这一格的用处就是看量级。
 */
export function mtok(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(Math.round(n));
}

/**
 * 四类 token 的副标题。
 *
 * ⛔ **一类都没取证到就如实说「没有分类」**，不要把缺的那几类按 0 显示 ——
 * 少一类会让占比整个偏掉，而偏的方向恰好是「这家看起来更便宜」。
 */
export function tokenMix(
  input: number | null,
  cacheRead: number | null,
  cacheWrite: number | null,
  output: number | null,
): string {
  const parts: string[] = [];
  if (input != null) parts.push(`输入 ${mtok(input)}`);
  if (cacheRead != null) parts.push(`缓存读 ${mtok(cacheRead)}`);
  if (cacheWrite != null) parts.push(`缓存写 ${mtok(cacheWrite)}`);
  if (output != null) parts.push(`输出 ${mtok(output)}`);
  return parts.length ? parts.join(" · ") : "站点账单没有分类明细";
}

/** 百分比。`null` 显示「—」，**绝不显示 0%** —— 那是断言，没数据不是。 */
export function pct(v: number | null | undefined) {
  return v == null ? dash : `${(v * 100).toFixed(1)}%`;
}

/** `09-13 22:10`。 */
export function stamp(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
