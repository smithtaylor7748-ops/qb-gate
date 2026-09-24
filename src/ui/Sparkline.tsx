/**
 * 一条极简折线。中转站的请求量曲线与用量明细页的趋势图共用（0.32.0 抽出来）。
 *
 * # ⛔ 没数据的那一格**断开**，不画成 0
 *
 * 这是从 `features/station/shared.tsx` 那条曲线原样带过来的规矩：连到 0 会让人以为
 * 那段时间的值真的是零，而真相通常是「那段时间没有记录」。两件事在界面上必须分得开。
 *
 * 所以 `points` 里的 `null` 表示「这一格没有数据」，画的时候分段：
 * 连续的非空点连成一段，遇到 `null` 就断。
 *
 * # 只引 `tokens.css` 的变量，不写 hex
 *
 * 跟全项目一条规矩（`CLAUDE.md`「颜色只在 tokens.css 里定义」）。
 */

export interface SparklineProps {
  /** 从旧到新。`null` = 这一格没有数据（断开，不画 0）。 */
  points: (number | null)[];
  /** 每个点的说明，长度跟 `points` 一致时会进 `<title>`。 */
  labels?: string[];
  /** 无障碍名字。 */
  label: string;
  height?: number;
  className?: string;
}

const W = 440;

export default function Sparkline({
  points,
  labels,
  label,
  height = 46,
  className = "",
}: SparklineProps) {
  const known = points.filter((p): p is number => p != null);
  if (known.length < 2) {
    return (
      <div className={`qb-st-chart-empty ${className}`}>
        这段时间还凑不出一条曲线（至少要两天有记录）
      </div>
    );
  }
  const max = Math.max(1, ...known);
  const n = points.length;
  const x = (i: number) => (i * W) / (n - 1);
  const y = (v: number) => height - 4 - (v / max) * (height - 10);

  // 分段：`null` 处断开。
  const segs: string[][] = [];
  let cur: string[] = [];
  points.forEach((v, i) => {
    if (v == null) {
      if (cur.length > 1) segs.push(cur);
      cur = [];
    } else {
      cur.push(`${x(i).toFixed(1)},${y(v).toFixed(1)}`);
    }
  });
  if (cur.length > 1) segs.push(cur);

  // 面积只画最长的那一段 —— 跨过断口去填会把「没数据」那段也涂上颜色。
  const longest = segs.reduce(
    (a, b) => (b.length > a.length ? b : a),
    segs[0] ?? [],
  );

  const summary =
    labels && labels.length === points.length
      ? points
          .map((v, i) => `${labels[i]}：${v == null ? "没有记录" : v}`)
          .join("\n")
      : label;

  return (
    <svg
      viewBox={`0 0 ${W} ${height}`}
      width="100%"
      height={height}
      preserveAspectRatio="none"
      role="img"
      aria-label={label}
      className={className}
      style={{ display: "block" }}
    >
      <title>{summary}</title>
      {longest.length > 1 && (
        <path
          d={`M${longest[0].split(",")[0]},${height - 4} L${longest.join(" L")} L${
            longest[longest.length - 1].split(",")[0]
          },${height - 4} Z`}
          fill="var(--accent-bg)"
        />
      )}
      {segs.map((s, i) => (
        <polyline
          key={i}
          points={s.join(" ")}
          fill="none"
          stroke="var(--accent)"
          strokeWidth={1.4}
          strokeLinejoin="round"
          vectorEffect="non-scaling-stroke"
        />
      ))}
    </svg>
  );
}
