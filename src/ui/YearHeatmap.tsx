/**
 * 全年热力图（中转站重构阶段 6 · U1，照 cc-switch 用量页的全年格子；按 dataviz 的规矩，用本项目的色板落地）。
 *
 * # 形状
 *
 * - 53 列（一周一列，周一在上）× 7 行；最后一列是今天所在的那一周，今天之后的格子不画。
 * - **一个色相、由浅到深**（顺序色：数量的大小）。四档都是 `--chart-1` 往卡片底色里掺
 *   （`color-mix`，样式在 `workspace.css` 的 `.qb-heat-*`）：掺得越少越深，tokens.css 之外不写一个色值；
 *   暗色主题下同一个公式自然翻成「越亮越多」。
 * - 四档按「有请求的那些天」的四分位切（GitHub 同一个做法）：一两个几百发的大日子不会把别的天全压成最浅一档。
 * - ⛔ **「没有请求」和「没有记录」分开画**：有记录以来、那天一发都没有的，画中性灰的空格子；最早那条记录之前的格子
 *   **不画** —— 那时候还没有记录，说不出那天用没用。画成空格子就是替使用者断言「那天没用」。
 * - 格子之间 2px（底色当分隔，不描边），2px 圆角。悬停和键盘焦点给同一份提示：数在前、日期其次、再是明细。
 *
 * # 键盘与读屏
 *
 * 整张图一个 Tab 停靠点；←/→ 一周、↑/↓ 一天、Home/End 到两头；`aria-live` 读出当前格子。
 * 表格版由调用方放在图下面 —— 提示框只是锦上添花，不是唯一的出口。
 */
import { useMemo, useState, type KeyboardEvent } from "react";
import { addDays } from "../lib/dates";

export interface HeatDay {
  /** 本地日期 `YYYY-MM-DD`。 */
  day: string;
  /** 这一天的量（请求数之类）。没列出来的天就是 0。 */
  value: number;
  /** 提示框里数值下面的几行 `[名字, 值]`。 */
  rows?: [string, string][];
}

export interface YearHeatmapProps {
  data: HeatDay[];
  /** 今天（本地日历）。最后一列是它所在的那一周。 */
  today: string;
  /** 最早有记录的那一天。它之前的格子不画；不给就全都画。 */
  firstDay?: string | null;
  /** 提示框和读屏怎么写这个量（`12 次请求`）。 */
  format: (v: number) => string;
  /** 无障碍名字。 */
  label: string;
  weeks?: number;
}

const CELL = 10;
const GAP = 2;
const STEP = CELL + GAP;
const LEFT = 20;
const TOP = 14;
const WEEKDAY = ["一", "", "三", "", "五", "", "日"];

/** 四档的分界：有量的那些天的 25 / 50 / 75 分位。返回 1–4；0 = 那天没有量。 */
export function heatLevels(values: number[]): (v: number) => 0 | 1 | 2 | 3 | 4 {
  const sorted = values.filter((v) => v > 0).sort((a, b) => a - b);
  const at = (q: number) =>
    sorted.length
      ? sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))]
      : 0;
  const cuts = [at(0.25), at(0.5), at(0.75)];
  return (v) => {
    if (!(v > 0)) return 0;
    if (v <= cuts[0]) return 1;
    if (v <= cuts[1]) return 2;
    if (v <= cuts[2]) return 3;
    return 4;
  };
}

/** 网格：从 `today` 所在那一周往前数 `weeks` 周的每一天（今天之后的不要）。 */
export function heatGrid(
  today: string,
  weeks: number,
): { day: string; col: number; row: number }[] {
  const [y, m, d] = today.split("-").map(Number);
  const monday = (new Date(y, m - 1, d).getDay() + 6) % 7;
  const start = addDays(today, -monday - 7 * (weeks - 1));
  const out: { day: string; col: number; row: number }[] = [];
  for (let i = 0; i < weeks * 7; i++) {
    const day = addDays(start, i);
    if (day > today) break;
    out.push({ day, col: Math.floor(i / 7), row: i % 7 });
  }
  return out;
}

export default function YearHeatmap({
  data,
  today,
  firstDay,
  format,
  label,
  weeks = 53,
}: YearHeatmapProps) {
  const [active, setActive] = useState<number | null>(null);
  const byDay = useMemo(() => new Map(data.map((x) => [x.day, x])), [data]);
  const level = useMemo(() => heatLevels(data.map((x) => x.value)), [data]);
  const grid = useMemo(() => heatGrid(today, weeks), [today, weeks]);
  const width = LEFT + weeks * STEP;
  const height = TOP + 7 * STEP;

  // 月份标在那个月第一个整周开始的那一列上方；挤在一起的（不到三列）跳过。
  const months: { col: number; text: string }[] = [];
  for (const g of grid) {
    if (g.row !== 0) continue;
    const [, m, d] = g.day.split("-").map(Number);
    if (
      d <= 7 &&
      (!months.length || g.col - months[months.length - 1].col >= 3)
    )
      months.push({ col: g.col, text: `${m}月` });
  }

  const shown = (day: string) => !firstDay || day >= firstDay;
  const move = (i: number) => {
    let j = Math.max(0, Math.min(grid.length - 1, i));
    // 没画的格子（最早记录之前）跳过，停在最早画出来的那一格。
    while (j < grid.length - 1 && !shown(grid[j].day)) j++;
    setActive(j);
  };
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const cur = active ?? grid.length - 1;
    if (e.key === "ArrowLeft") move(cur - 7);
    else if (e.key === "ArrowRight") move(cur + 7);
    else if (e.key === "ArrowUp") move(cur - 1);
    else if (e.key === "ArrowDown") move(cur + 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(grid.length - 1);
    else return;
    e.preventDefault();
  };

  const cur = active != null ? grid[active] : null;
  const curData = cur ? byDay.get(cur.day) : undefined;
  const curText = cur
    ? !shown(cur.day)
      ? "那时还没有记录"
      : curData && curData.value > 0
        ? format(curData.value)
        : "没有请求"
    : "";
  const weekday = cur
    ? ["周一", "周二", "周三", "周四", "周五", "周六", "周日"][cur.row]
    : "";
  // 提示框贴在格子旁边：格子在右半边就放左侧。
  const onRight = cur ? LEFT + cur.col * STEP > width / 2 : false;

  return (
    <div
      className="qb-heat"
      tabIndex={0}
      role="group"
      aria-label={`${label}。左右方向键一周、上下方向键一天。`}
      onKeyDown={onKey}
      onFocus={() => active == null && setActive(grid.length - 1)}
      onBlur={() => setActive(null)}
      // 图按容器宽度缩放，最多放大到 1.4 倍；容器和图一样宽，提示框按百分比摆才对得上格子。
      style={{ maxWidth: width * 1.4 }}
    >
      <svg
        viewBox={`0 0 ${width} ${height}`}
        width="100%"
        role="img"
        aria-label={label}
        style={{ display: "block" }}
        onPointerLeave={() => setActive(null)}
      >
        {months.map((x) => (
          <text
            key={`m-${x.col}`}
            x={LEFT + x.col * STEP}
            y={TOP - 4}
            className="qb-heat-axis"
          >
            {x.text}
          </text>
        ))}
        {WEEKDAY.map((t, row) =>
          t ? (
            <text
              key={`w-${row}`}
              x={LEFT - 6}
              y={TOP + row * STEP + CELL / 2}
              dy="0.35em"
              textAnchor="end"
              className="qb-heat-axis"
            >
              {t}
            </text>
          ) : null,
        )}
        {grid.map((g, i) =>
          shown(g.day) ? (
            <rect
              key={g.day}
              x={LEFT + g.col * STEP}
              y={TOP + g.row * STEP}
              width={CELL}
              height={CELL}
              rx={2}
              className={`qb-heat-${level(byDay.get(g.day)?.value ?? 0)}${active === i ? " qb-heat-on" : ""}`}
              onPointerEnter={() => setActive(i)}
            />
          ) : null,
        )}
      </svg>
      {cur && (
        <div
          className={`qb-bars-tip qb-bars-tip--${onRight ? "end" : "start"}`}
          style={{
            left: `${((LEFT + cur.col * STEP + (onRight ? -6 : CELL + 6)) / width) * 100}%`,
            top: 0,
          }}
          aria-hidden="true"
        >
          <strong>{curText}</strong>
          <span className="qb-bars-tip-title">
            {cur.day} {weekday}
            {cur.day === today ? "（今天）" : ""}
          </span>
          {curData &&
            curData.value > 0 &&
            curData.rows?.map(([k, v]) => (
              <span key={k} className="qb-bars-tip-row">
                <span>{k}</span>
                <span>{v}</span>
              </span>
            ))}
        </div>
      )}
      <span className="sr-only" aria-live="polite">
        {cur ? `${cur.day} ${weekday}：${curText}` : ""}
      </span>
    </div>
  );
}

/** 图例：少 → 多四档、「没有请求」、「没有记录」（不画）。 */
export function YearHeatmapLegend({ hasGap }: { hasGap: boolean }) {
  return (
    <div className="qb-heat-legend" aria-hidden="true">
      <span>少</span>
      {[1, 2, 3, 4].map((l) => (
        <svg key={l} width={CELL} height={CELL} viewBox={`0 0 ${CELL} ${CELL}`}>
          <rect width={CELL} height={CELL} rx={2} className={`qb-heat-${l}`} />
        </svg>
      ))}
      <span>多</span>
      <svg width={CELL} height={CELL} viewBox={`0 0 ${CELL} ${CELL}`}>
        <rect width={CELL} height={CELL} rx={2} className="qb-heat-0" />
      </svg>
      <span>没有请求</span>
      {hasGap && <span>· 空白 = 那时还没有记录</span>}
    </div>
  );
}
