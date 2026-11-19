/**
 * 自动选线的偏好与排名预览（「选线与分流」弹窗用的几块）。
 *
 * 2026-11-15 起这里不再是一个自己存盘的弹窗页：偏好挑三档之一（省钱 / 速度 / 稳定），
 * 底线与换首选的防抖收进「高级设置」，跟备用线路一起由弹窗底部那一个「保存」落盘（F2：原来两种保存方式
 * 来回切会把刚改的冲掉）。原来那颗「用这一条」删了（B2：它点了反而关掉自动选线，又没有提示），
 * 手动换线只有线路行上的「设为当前」。
 *
 * # 勾的几项里，最弱的那一项最好的那条赢
 *
 * ⛔ 三个复选框，不是三根推子。推子显示 50% 的精度，而那个精度不存在。
 * 排序本身在 Rust（`qb-station::schedule::rank`），这里**只显示**：每一维单独一根条 = 跟池里最好那条的比值。
 */

import { useEffect, useRef, useState } from "react";

import type { Axis } from "../../lib/generated/Axis";
import type { Client } from "../../lib/generated/Client";
import type { AxisScore } from "../../lib/generated/AxisScore";
import type { Row } from "../../lib/generated/Row";
import {
  routeLabelFromId,
  stationApi,
  type DecisionView,
  type Prefs,
} from "../../lib/station";

export const AX: Record<Axis, { label: string; desc: string }> = {
  cheap: { label: "便宜", desc: "单位配额消耗最低" },
  fast: { label: "快", desc: "首字 P95 最短" },
  stable: { label: "稳", desc: "成功率最高" },
};
export const AXES: Axis[] = ["cheap", "fast", "stable"];

export type PresetId = "saving" | "speed" | "stable";

/**
 * 三档偏好，对到原来的三个预设（跑批量 / 对话 / 写代码）。
 *
 * ⛔ 「省钱」必须跟后端 `Prefs::saving()`（新建自动选线的默认）一样：便宜 + 成功率 ≥ 95%（C1 / G6）。
 */
export const PRESETS: {
  id: PresetId;
  name: string;
  desc: string;
  prefs: Prefs;
}[] = [
  {
    id: "saving",
    name: "省钱",
    desc: "挑最便宜的，成功率不低于 95%（默认）",
    prefs: {
      axes: ["cheap"],
      floors: { min_success_rate: 0.95, max_ttft_p95_ms: null, max_rate: null },
    },
  },
  {
    id: "speed",
    name: "速度",
    desc: "挑首字最快的，成功率不低于 99%",
    prefs: {
      axes: ["fast"],
      floors: { min_success_rate: 0.99, max_ttft_p95_ms: null, max_rate: null },
    },
  },
  {
    id: "stable",
    name: "稳定",
    desc: "又稳又快，首字 P95 不超过 3 秒（不看价格）",
    prefs: {
      axes: ["stable", "fast"],
      floors: {
        min_success_rate: null,
        max_ttft_p95_ms: BigInt(3000),
        max_rate: null,
      },
    },
  },
];

/** 存着的偏好对得上哪一档（只比「按什么挑」；底线可以在高级设置里另调）。对不上是 `null` = 自定义。 */
export function presetOf(prefs: Prefs): PresetId | null {
  const key = (axes: Axis[]) => [...axes].sort().join("+");
  return PRESETS.find((p) => key(p.prefs.axes) === key(prefs.axes))?.id ?? null;
}

/** 底线三格。勾上才算，没勾就是不设。 */
export function Floor({
  label,
  suffix,
  note,
  value,
  fallback,
  step,
  onChange,
}: {
  label: string;
  suffix: string;
  note?: string;
  value: number | null;
  fallback: number;
  step: number;
  onChange: (v: number | null) => void;
}) {
  const on = value != null;
  return (
    <div className={`qb-st-floor${on ? "" : " off"}`}>
      <input
        type="checkbox"
        checked={on}
        aria-label={`启用${label}`}
        onChange={(e) => onChange(e.target.checked ? fallback : null)}
      />
      <label>
        {label}{" "}
        <input
          className="qb-st-num"
          type="number"
          step={step}
          disabled={!on}
          value={on ? value : fallback}
          onChange={(e) => onChange(Number(e.target.value))}
        />{" "}
        {suffix}
        {note && (
          <span style={{ fontSize: 11, color: "var(--text-3)" }}> {note}</span>
        )}
      </label>
    </div>
  );
}

/** 按什么挑（三个复选框）+ 底线，给「高级设置」里想细调的人。 */
export function PrefsEditor({
  prefs,
  onChange,
}: {
  prefs: Prefs;
  onChange: (p: Prefs) => void;
}) {
  const toggle = (a: Axis) => {
    const has = prefs.axes.includes(a);
    // 一项都不勾是合法的 —— 那表示「随便，别乱换」，当前线路会一直留着。
    onChange({
      ...prefs,
      axes: has ? prefs.axes.filter((x) => x !== a) : [...prefs.axes, a],
    });
  };
  return (
    <>
      <div className="qb-st-goals">
        {AXES.map((a) => {
          const on = prefs.axes.includes(a);
          return (
            <label key={a} className={`qb-st-goal${on ? " on" : ""}`}>
              <input type="checkbox" checked={on} onChange={() => toggle(a)} />
              <span style={{ flex: 1, minWidth: 0 }}>
                <span className="gn">{AX[a].label}</span>
                <span className="gd">{AX[a].desc}</span>
              </span>
            </label>
          );
        })}
      </div>
      <p className="qb-st-note">
        勾的几项里，最弱的那一项最好的那条赢；一项都不勾就是「随便，别乱换」。底线先筛掉，不参与排序；
        熔断中、测活判死、没过底线的线永远不会被换上来当当前线路。
      </p>
      <div className="qb-st-floors">
        <Floor
          label="成功率不低于"
          suffix="%"
          value={
            prefs.floors.min_success_rate == null
              ? null
              : prefs.floors.min_success_rate * 100
          }
          fallback={99}
          step={0.1}
          onChange={(v) =>
            onChange({
              ...prefs,
              floors: {
                ...prefs.floors,
                min_success_rate: v == null ? null : v / 100,
              },
            })
          }
        />
        <Floor
          label="首字 P95 不超过"
          suffix="秒"
          value={
            prefs.floors.max_ttft_p95_ms == null
              ? null
              : Number(prefs.floors.max_ttft_p95_ms) / 1000
          }
          fallback={3}
          step={0.1}
          onChange={(v) =>
            onChange({
              ...prefs,
              floors: {
                ...prefs.floors,
                max_ttft_p95_ms:
                  v == null ? null : BigInt(Math.round(v * 1000)),
              },
            })
          }
        />
        <Floor
          label="倍率不超过 ×"
          suffix=""
          note="按实测倍率比，没检验过的按标称；充值比例不是 1 元 = 1 美元额度的站，先乘上它（每 $1 牌价付几元）"
          value={prefs.floors.max_rate}
          fallback={0.5}
          step={0.01}
          onChange={(v) =>
            onChange({ ...prefs, floors: { ...prefs.floors, max_rate: v } })
          }
        />
      </div>
    </>
  );
}

/**
 * 按某套偏好排一次名（只排当前线路 + 备用线路，自动选线真会挑的那几条）。**只排不换**。
 *
 * `decide` 会给每条线现拉一轮站点账单，要好几秒 —— 所以这一块只在「高级设置」里点开时才挂。
 */
export function RankingPreview({
  client,
  prefs,
}: {
  client: Client;
  prefs: Prefs;
}) {
  const [view, setView] = useState<DecisionView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const revision = useRef(0);
  useEffect(() => {
    const mine = ++revision.current;
    setBusy(true);
    setError(null);
    stationApi
      .decide(prefs, client)
      .then((v) => mine === revision.current && setView(v))
      .catch(
        (e: unknown) =>
          mine === revision.current &&
          setError(e instanceof Error ? e.message : String(e)),
      )
      .finally(() => mine === revision.current && setBusy(false));
    return () => {
      revision.current++;
    };
  }, [client, prefs]);
  const ranking = view?.ranking;
  const winner = ranking?.winner ?? null;
  return (
    <div className="qb-card">
      <h3>按这套偏好的排名（预览）</h3>
      {error && <div className="qb-st-nudge">排序失败：{error}</div>}
      <p className="qb-st-note">
        {busy && !view ? "正在排…" : ""}
        {ranking?.rows.length
          ? `排第一：${winner ? routeLabelFromId(winner) : "没有能用的线路"}。每根条 = 跟这几条里最好那条的比值，1.00 最好${
              view?.basis === "cost-per-token"
                ? "。「便宜」用的是 24h 实扣 ÷ 实际 token（算进了缓存命中）"
                : view?.basis === "blended-ratio"
                  ? "。「便宜」按这条线实际的输入输出比，把四类的「站点单价 ÷ 官方单价」加权"
                  : view?.basis === "real-rate"
                    ? "。「便宜」用的是真实倍率 —— 连 token 结构都拿不到，退回这个粗口径"
                    : ""
            }${
              view?.basis && view.basis !== "unavailable"
                ? "；不同站点先按各自的充值比例折成同一种钱再比"
                : ""
            }。`
          : view
            ? "还没有可排的线路：先设当前线路、勾备用线路。"
            : ""}
      </p>
      <div className="qb-st-lanes">
        {ranking?.rows.map((row, i) => (
          <Lane key={row.route_id} row={row} index={i} />
        ))}
      </div>
      <p className="qb-st-note" style={{ marginTop: 10 }}>
        {ranking?.floors_relaxed &&
          "⚠ 你设的底线现在一条线路都满足不了 —— 仍然照常走，但下面这些都是没过线的，别当成合格。 "}
        {ranking?.breakers_relaxed &&
          "⚠ 所有线路都在熔断中 —— 仍然留一条给你用，如实报错，不制造死局。 "}
        {ranking && ranking.indistinguishable.length > 0 && (
          <>
            「{ranking.indistinguishable.map((a) => AX[a].label).join("」「")}
            」在这几条线路上差距太小，分不出高下，没有参与排序。{" "}
          </>
        )}
        这里只是这一轮的排名；真要换当前线路还得过「换当前线路的防抖」。
      </p>
    </div>
  );
}

/** 三种故障分开处理（静态说明，跟熔断设置放在一起）。 */
export function FailureRules() {
  return (
    <details className="qb-st-adv">
      <summary>故障怎么处理</summary>
      <div className="qb-st-brks">
        <div className="qb-st-brk warn">
          <span className="trig">429 限流</span>
          <span className="act">有备用线路就改用下一条</span>
          <span className="set">
            听 Retry-After；没有备用线路时等它给的时间，或回「繁忙」。
          </span>
        </div>
        <div className="qb-st-brk bad">
          <span className="trig">401 / 403 / 402</span>
          <span className="act">立即熔断</span>
          <span className="set">
            令牌失效 / 没权限 / 余额不足，重试没有意义；充了值点「重试这条」
          </span>
        </div>
        <div className="qb-st-brk bad">
          <span className="trig">5xx / 超时</span>
          <span className="act">连续几次才熔断</span>
          <span className="set">单发 500 是噪声，次数在上面「熔断」里设</span>
        </div>
        <div className="qb-st-brk">
          <span className="trig">只有当前线路</span>
          <span className="act">熔断不拦发送</span>
          <span className="set">没有备用线路可换，照打、如实报错</span>
        </div>
        <div className="qb-st-brk">
          <span className="trig">切换时机</span>
          <span className="act">两次请求之间</span>
          <span className="set">不打断进行中的那一条</span>
        </div>
      </div>
    </details>
  );
}

function Lane({ row, index }: { row: Row; index: number }) {
  const out = !row.eligible || row.score == null;
  return (
    <>
      <div
        className={`qb-st-lane${index === 0 && !out ? " win" : ""}${
          out ? " out" : ""
        }`}
      >
        <span className="no">{out ? "—" : index + 1}</span>
        <span className="nm" title={row.route_id}>
          {routeLabelFromId(row.route_id)}
        </span>
        {row.per_axis.length > 0 ? (
          <span className="qb-st-mets">
            {row.per_axis.map(([axis, score]) => (
              <Met
                key={axis}
                axis={axis}
                score={score}
                weak={row.weakest === axis}
              />
            ))}
          </span>
        ) : (
          <span className="why">没有可比的项</span>
        )}
        <span className="tot">
          {row.score == null ? "—" : row.score.toFixed(2)}
        </span>
      </div>
      {/* ⛔ 熔断中写「熔断中」，没检验过写缺了哪一维 —— 都不写成 0 分 */}
      {row.tripped && (
        <div className="qb-st-lane out">
          <span className="no" />
          <span className="nm" />
          <span className="why">熔断中，不参与竞争</span>
        </div>
      )}
      {row.failed_floors.length > 0 && (
        <div className="qb-st-lane out">
          <span className="no" />
          <span className="nm" />
          <span className="why">
            没过线：
            {row.failed_floors
              .map(
                (f) =>
                  `${AX[f.axis].label} ${fmt(f.axis, f.actual)}（要求 ${fmt(
                    f.axis,
                    f.limit,
                  )}）`,
              )
              .join(" · ")}
          </span>
        </div>
      )}
      {row.missing.length > 0 && (
        <div className="qb-st-lane out">
          <span className="no" />
          <span className="nm" />
          <span className="why">
            还没有证据：{row.missing.map((a) => AX[a].label).join(" · ")}
          </span>
        </div>
      )}
    </>
  );
}

function Met({
  axis,
  score,
  weak,
}: {
  axis: Axis;
  score: AxisScore;
  weak: boolean;
}) {
  const ratio = score.kind === "ratio" ? score.value : null;
  return (
    <span className={`qb-st-met ${axis}${weak ? " weak" : ""}`}>
      <i>
        <span>{AX[axis].label}</span>
        <span>{ratio == null ? "—" : ratio.toFixed(2)}</span>
      </i>
      <span className="mb">
        <b style={{ width: `${(ratio ?? 0) * 100}%` }} />
      </span>
    </span>
  );
}

function fmt(axis: Axis, v: number): string {
  if (axis === "stable") return `${(v * 100).toFixed(1)}%`;
  if (axis === "fast") return `${(v / 1000).toFixed(2)}s`;
  return `×${v.toFixed(2)}`;
}
