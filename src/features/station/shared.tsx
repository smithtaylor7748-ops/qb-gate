/**
 * 中转站几个界面共用的那点东西：时间范围、请求量曲线、读数格式化。
 *
 * # 为什么单独一个文件
 *
 * 总览卡和请求日志弹窗画的是**同一条曲线**、用的是**同一套时间档**。
 * 各写一份的话，两处会慢慢漂开 —— 而漂开之后最难受的不是样式不一致，
 * 是同一段时间在两个地方算出不同的请求数，而没有任何地方说得清哪个对。
 *
 * ⛔ 颜色只引 `tokens.css` 的变量，不写 hex。
 */

import type { RequestLog } from "../../lib/station";

/** 时间范围。`hours` 只用来算起点，「全部」给一个够大的数。 */
export const RANGES = [
  { id: "1h", name: "近 1 小时", hours: 1 },
  { id: "24h", name: "近 24 小时", hours: 24 },
  { id: "7d", name: "近 7 天", hours: 24 * 7 },
  { id: "all", name: "全部", hours: 24 * 365 },
] as const;
export type RangeId = (typeof RANGES)[number]["id"];
export const rangeOf = (id: RangeId) => RANGES.find((r) => r.id === id)!;

export type GranId = "hour" | "day";

// ------------------------------------------------------------------ 曲线

/**
 * 请求量 + 首字 P95，**各按自己的量程**。
 *
 * ⛔ 那一格没请求就**断开**，不画成 0 —— 连到 0 会让人以为那段时间延迟为零。
 */
export function Chart({
  rows,
  range,
  gran,
}: {
  rows: RequestLog[];
  range: RangeId;
  gran: GranId;
}) {
  const W = 440;
  const H = 46;
  if (!rows.length)
    return (
      <div className="qb-st-chart-empty">这段时间没有经过本机路由的请求</div>
    );

  const now = Date.now();
  const step = gran === "day" ? 24 : 1;
  const span = Math.min(rangeOf(range).hours, 24 * 30);
  const n = Math.max(2, Math.ceil(span / step));
  const buckets: RequestLog[][] = Array.from({ length: n }, () => []);
  for (const r of rows) {
    const age = (now - Number(r.at_ms)) / 3600_000;
    if (age >= 0 && age < span) buckets[n - 1 - Math.floor(age / step)].push(r);
  }

  const counts = buckets.map((b) => b.length);
  const maxC = Math.max(1, ...counts);
  const p95 = buckets.map((b) => {
    const v = b
      .map((x) => (x.first_token_ms == null ? null : Number(x.first_token_ms)))
      .filter((x): x is number => x != null)
      .sort((a, b2) => a - b2);
    return v.length
      ? v[Math.min(v.length - 1, Math.floor(v.length * 0.95))]
      : null;
  });
  const maxT = Math.max(1, ...p95.filter((v): v is number => v != null));

  const x = (i: number) => (i * W) / (n - 1);
  const y = (v: number, m: number) => H - 4 - (v / m) * (H - 10);
  const pts = counts.map(
    (c, i) => `${x(i).toFixed(1)},${y(c, maxC).toFixed(1)}`,
  );

  // 首字那条线分段画：断开的格子不连过去。
  const segs: string[][] = [];
  let cur: string[] = [];
  p95.forEach((v, i) => {
    if (v == null) {
      if (cur.length > 1) segs.push(cur);
      cur = [];
    } else cur.push(`${x(i).toFixed(1)},${y(v, maxT).toFixed(1)}`);
  });
  if (cur.length > 1) segs.push(cur);

  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      width="100%"
      height={H}
      preserveAspectRatio="none"
      role="img"
      style={{ display: "block" }}
    >
      <title>所选时段的请求量与首字延迟</title>
      <path
        d={`M0,${H - 4} L${pts.join(" L")} L${W},${H - 4} Z`}
        fill="var(--accent-bg)"
      />
      <polyline
        points={pts.join(" ")}
        fill="none"
        stroke="var(--accent)"
        strokeWidth={1.4}
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
      {segs.map((s, i) => (
        <polyline
          key={i}
          points={s.join(" ")}
          fill="none"
          stroke="var(--warn)"
          strokeWidth={1.1}
          vectorEffect="non-scaling-stroke"
        />
      ))}
    </svg>
  );
}

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
