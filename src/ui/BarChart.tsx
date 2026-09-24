/**
 * 柱状图。用量明细页「趋势」那一格（2026-09-24，替掉原来那条 `Sparkline`）。
 *
 * 使用者原话：「这个趋势图不好，要看见每天、7 天、30 天花了多少刀」。原来那条折线
 * 没有坐标轴、没有数值、只填最长的一段、还被 `preserveAspectRatio="none"` 拉伸 ——
 * 看得出「起伏」，读不出「那天花了多少」。
 *
 * # 形状（按 dataviz 的规矩，用本项目的色板落地）
 *
 * - **单系列、一个颜色**（`--chart-1`），不要图例 —— 卡片标题说明画的是什么。
 * - 柱宽封顶 24px，不塞满槽位；顶端 4px 圆角、底端方角；柱与柱之间天然留空。
 * - 网格线 1px 实线、很淡（`--chart-grid`）；y 轴取整刻度，数字等宽。
 * - **只直接标两根**：最高的那根和「今天」那根。其余的值在提示框和下面的按天明细表里 ——
 *   每根柱子顶上都写数，等于一根都没写。
 * - 悬停和键盘焦点给同一份提示：数值在前、日期其次、再是明细。
 *
 * # ⛔ 没有记录的那一格不画柱，也不写 $0
 *
 * `value: null` = 那一天（那一小时）本机没有记录。画成 0 高的柱子会被读成「花了 $0」——
 * 跟 `Sparkline` 那条「断开，不连到 0」是同一个理由。提示框里写「没有记录」。
 *
 * # 键盘与读屏
 *
 * 整张图只占**一个** Tab 停靠点（30 根柱子 30 个停靠点没人受得了）；←/→ 逐根、Home/End 到两头。
 * 读屏从 `aria-live` 那一行读到当前这根；完整的数在下面的表格里 —— 那张表就是这张图的表格版，
 * 提示框只是锦上添花，不是唯一的出口。
 *
 * # 只引 `tokens.css` 的变量，不写 hex
 */
import {
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
} from "react";

export interface BarDatum {
  /** 稳定的键（日期、小时）。 */
  key: string;
  /** x 轴上的短标签（`09-24`、`14`）。 */
  axis: string;
  /** 提示框的标题（`2026-09-24`、`14:00–15:00`）。 */
  title: string;
  /** `null` = 没有记录：不画柱。 */
  value: number | null;
  /** 提示框里数值下面的几行 `[名字, 值]`。 */
  rows?: [string, string][];
}

export interface BarChartProps {
  data: BarDatum[];
  /** 坐标轴刻度怎么显示（`$20`、`1.2M`）—— 去掉多余的零，刻度要短。 */
  format: (v: number) => string;
  /** 提示框、直接标注、读屏怎么显示（`$12.70`）。不给就跟刻度一样。 */
  valueFormat?: (v: number) => string;
  /** 无障碍名字。 */
  label: string;
  /** 要直接标注的那一根（通常是今天）。 */
  highlight?: string;
  /** 画图区高度（不含 x 轴那一带）。 */
  height?: number;
  /** 一根有值的柱子都没有时显示的话。 */
  empty?: string;
  className?: string;
}

/** y 轴刻度那一列的宽度。 */
const LEFT = 56;
const RIGHT = 8;
/** 直接标注要的顶部空间。 */
const TOP = 18;
/** x 轴那一带。**算进容器高度** —— 不然 x 轴标签被裁、卡片里冒出一根内嵌滚动条。 */
const AXIS = 22;
const MAX_BAR = 24;

/** 取整刻度：步长取 1 / 2 / 2.5 / 5 × 10^k，大约四格，最后一格盖过最大值。 */
export function niceTicks(max: number, count = 4): number[] {
  if (!(max > 0) || !Number.isFinite(max)) return [0, 1];
  const raw = max / count;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step =
    [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? raw;
  const ticks: number[] = [0];
  while (ticks[ticks.length - 1] < max - step * 1e-9) {
    ticks.push(Number((ticks[ticks.length - 1] + step).toPrecision(12)));
  }
  return ticks;
}

/** 顶端圆角、底端方角的一根柱子。 */
function barPath(x: number, y: number, w: number, h: number): string {
  const r = Math.min(4, w / 2, h);
  const b = y + h;
  return (
    `M${x},${b} L${x},${y + r} Q${x},${y} ${x + r},${y} ` +
    `L${x + w - r},${y} Q${x + w},${y} ${x + w},${y + r} L${x + w},${b} Z`
  );
}

export default function BarChart({
  data,
  format,
  valueFormat = format,
  label,
  highlight,
  height = 150,
  empty = "这一档没有记录。",
  className = "",
}: BarChartProps) {
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(640);
  const [active, setActive] = useState<number | null>(null);

  // 按容器的真实宽度画，不拉伸 —— 拉伸会把 4px 的圆角和 1px 的线一起拉变形。
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => {
      const w = el.clientWidth;
      if (w > 0) setWidth(w);
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const values = data.map((d) => d.value);
  const known = values.filter((v): v is number => v != null);
  const n = data.length;
  const max = known.length ? Math.max(...known) : 0;
  const ticks = niceTicks(max);
  const top = ticks[ticks.length - 1] || 1;
  const plotW = Math.max(40, width - LEFT - RIGHT);
  const band = n > 0 ? plotW / n : plotW;
  const barW = Math.max(2, Math.min(MAX_BAR, band * 0.66));
  const y = (v: number) => TOP + height - (v / top) * height;
  const barX = (i: number) => LEFT + i * band + (band - barW) / 2;
  const svgH = TOP + height + AXIS;

  // x 轴标签：少的时候全标；多的时候从最后一根（通常是今天）往前每隔几根标一个。
  const every = n <= 12 ? 1 : Math.ceil(n / 8);
  const showAxis = (i: number) => (n - 1 - i) % every === 0;

  const maxIdx = known.length ? values.indexOf(max) : -1;
  const hiIdx = highlight ? data.findIndex((d) => d.key === highlight) : -1;
  const labelled = new Set(
    [maxIdx, hiIdx].filter(
      (i) => i >= 0 && values[i] != null && values[i]! > 0,
    ),
  );

  const move = (i: number) => setActive(Math.max(0, Math.min(n - 1, i)));
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!n) return;
    const cur = active ?? n - 1;
    if (e.key === "ArrowLeft") move(cur - 1);
    else if (e.key === "ArrowRight") move(cur + 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(n - 1);
    else return;
    e.preventDefault();
  };
  const onPointer = (e: PointerEvent<SVGRectElement>) => {
    const rect = (
      e.currentTarget.ownerSVGElement ?? e.currentTarget
    ).getBoundingClientRect();
    const i = Math.floor((e.clientX - rect.left - LEFT) / band);
    if (i >= 0 && i < n) setActive(i);
  };

  const cur = active != null ? data[active] : null;
  const say = cur
    ? `${cur.title}：${cur.value == null ? "没有记录" : valueFormat(cur.value)}`
    : "";

  if (known.length === 0) {
    return (
      <div ref={box} className={`qb-bars ${className}`}>
        <p className="notice">{empty}</p>
      </div>
    );
  }

  // 提示框贴在那根柱子**旁边**，不盖住它：柱子在右半边就放左侧，否则放右侧。
  const onRight = active != null && barX(active) + barW / 2 > width / 2;
  const tipLeft =
    active != null ? (onRight ? barX(active) - 8 : barX(active) + barW + 8) : 0;
  const tipAlign = onRight ? "end" : "start";

  return (
    <div
      ref={box}
      className={`qb-bars ${className}`}
      tabIndex={0}
      role="group"
      aria-label={`${label}。用左右方向键逐根查看。`}
      onKeyDown={onKey}
      onFocus={() => active == null && setActive(hiIdx >= 0 ? hiIdx : n - 1)}
      onBlur={() => setActive(null)}
    >
      <svg
        width={width}
        height={svgH}
        viewBox={`0 0 ${width} ${svgH}`}
        role="img"
        aria-label={label}
        style={{ display: "block" }}
      >
        {ticks.map((t) => (
          <g key={t}>
            <line
              x1={LEFT}
              x2={width - RIGHT}
              y1={y(t)}
              y2={y(t)}
              stroke={t === 0 ? "var(--border-strong)" : "var(--chart-grid)"}
              strokeWidth={1}
              shapeRendering="crispEdges"
            />
            <text
              x={LEFT - 8}
              y={y(t)}
              dy="0.32em"
              textAnchor="end"
              className="qb-bars-tick"
            >
              {format(t)}
            </text>
          </g>
        ))}
        {data.map((d, i) =>
          d.value != null && d.value > 0 ? (
            <path
              key={d.key}
              d={barPath(
                barX(i),
                y(d.value),
                barW,
                Math.max(1, y(0) - y(d.value)),
              )}
              fill={active === i ? "var(--chart-1-hover)" : "var(--chart-1)"}
            />
          ) : null,
        )}
        {[...labelled].map((i) => (
          <text
            key={`v-${data[i].key}`}
            x={Math.max(
              LEFT + 12,
              Math.min(width - RIGHT - 12, barX(i) + barW / 2),
            )}
            y={y(values[i]!) - 5}
            textAnchor="middle"
            className="qb-bars-value"
          >
            {valueFormat(values[i]!)}
          </text>
        ))}
        {data.map((d, i) =>
          showAxis(i) ? (
            <text
              key={`x-${d.key}`}
              x={barX(i) + barW / 2}
              y={TOP + height + 15}
              textAnchor="middle"
              className={`qb-bars-axis${i === hiIdx ? " qb-bars-axis--hi" : ""}`}
            >
              {d.axis}
            </text>
          ) : null,
        )}
        {/* 命中区：整片画图区，按指针的 x 找那一根 —— 读者瞄的是日期，不是那根细柱子。 */}
        <rect
          x={LEFT}
          y={TOP}
          width={plotW}
          height={height}
          fill="transparent"
          onPointerMove={onPointer}
          onPointerLeave={() => setActive(null)}
        />
      </svg>
      {cur && (
        <div
          className={`qb-bars-tip qb-bars-tip--${tipAlign}`}
          style={{ left: tipLeft }}
          aria-hidden="true"
        >
          <strong>
            {cur.value == null ? "没有记录" : valueFormat(cur.value)}
          </strong>
          <span className="qb-bars-tip-title">{cur.title}</span>
          {cur.value != null &&
            cur.rows?.map(([k, v]) => (
              <span key={k} className="qb-bars-tip-row">
                <span>{k}</span>
                <span>{v}</span>
              </span>
            ))}
        </div>
      )}
      <span className="sr-only" aria-live="polite">
        {say}
      </span>
    </div>
  );
}
