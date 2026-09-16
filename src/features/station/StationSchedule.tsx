/**
 * 智能调度：**勾的几项里，最弱的那一项最好的那条赢。**
 *
 * ⛔ 三个复选框，不是三根推子。推子显示 50% 的精度，而那个精度不存在 ——
 * 三条线稳定度都在 91~92 时，把「稳定」从 0 拉到 100 排名一个字不变，
 * 界面却写着「稳定占 50%」。复选框问的是「这一项算不算」，
 * 那是使用者答得出的问题。
 *
 * 排序本身在 Rust（`qb-station::schedule::rank`），这里**只显示**：
 * 每一维单独一根条 = 跟池里最好那条的比值。合成一根总分条会把
 * 「哪一项拖后腿」抹掉，而那正是要看的东西。
 */

import { useCallback, useEffect, useRef, useState } from "react";

import type { Axis } from "../../lib/generated/Axis";
import type { Client } from "../../lib/generated/Client";
import type { AxisScore } from "../../lib/generated/AxisScore";
import type { Row } from "../../lib/generated/Row";
import type { Schedule } from "../../lib/generated/Schedule";
import {
  routeLabelFromId,
  stationApi,
  type DecisionView,
  type Prefs,
} from "../../lib/station";
import { Button } from "../../ui";

const AX: Record<Axis, { label: string; desc: string }> = {
  cheap: { label: "便宜", desc: "单位配额消耗最低" },
  fast: { label: "快", desc: "首字 P95 最短" },
  stable: { label: "稳", desc: "成功率最高" },
};
const AXES: Axis[] = ["cheap", "fast", "stable"];

const PRESETS: { name: string; prefs: Prefs }[] = [
  {
    name: "写代码",
    prefs: {
      axes: ["stable", "fast"],
      floors: {
        min_success_rate: null,
        max_ttft_p95_ms: BigInt(3000),
        max_rate: null,
      },
    },
  },
  {
    name: "跑批量",
    prefs: {
      axes: ["cheap"],
      floors: {
        min_success_rate: 0.95,
        max_ttft_p95_ms: null,
        max_rate: null,
      },
    },
  },
  {
    name: "对话",
    prefs: {
      axes: ["fast"],
      floors: {
        min_success_rate: 0.99,
        max_ttft_p95_ms: null,
        max_rate: null,
      },
    },
  },
];

const DEFAULT_PREFS: Prefs = {
  axes: ["cheap", "fast", "stable"],
  floors: { min_success_rate: null, max_ttft_p95_ms: null, max_rate: null },
};

// 线路不从上面传进来：`decide` 每次都从库里现读一遍，
// 两处各持一份列表必然漂移（界面显示旧的、排序按新的）。
//
// ⛔ **偏好要存回后端。** 驻留循环读的是库里那一份；只改本地 state 的话，
// 界面上勾的是「便宜」而循环还按上一次那套在换上游 —— 两份判定必然漂移，
// 而且漂了之后没有任何地方看得出来。
export default function StationSchedule({
  client,
  onSaved,
}: {
  client: Client;
  /** 偏好存进库之后回调，带回存进去的那一份。右列那张调度卡靠它跟着改口径。 */
  onSaved?: (saved: Schedule) => void;
}) {
  const [prefs, setPrefs] = useState<Prefs>(DEFAULT_PREFS);
  const [view, setView] = useState<DecisionView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // 从库里读回来那一份之前不要往回存 —— 否则默认偏好会把使用者存过的
  // 那套盖掉，而他只是打开弹窗看了一眼。
  const [loaded, setLoaded] = useState(false);

  const run = useCallback(
    async (p: Prefs) => {
      setBusy(true);
      setError(null);
      try {
        setView(await stationApi.decide(p, client));
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [client],
  );

  // 存着的那一份偏好读回来 —— 这个弹窗显示的必须是驻留循环正在用的那套。
  useEffect(() => {
    let dropped = false;
    setLoaded(false);
    void stationApi
      .schedules()
      .then((all) => {
        if (dropped) return;
        const mine = all.find((s) => s.client === client);
        if (mine) setPrefs(mine.prefs);
        setLoaded(true);
      })
      .catch(() => {
        if (!dropped) setLoaded(true);
      });
    return () => {
      dropped = true;
    };
  }, [client]);

  useEffect(() => {
    void run(prefs);
  }, [run, prefs]);

  // 回调用 ref 接，不进下面那个 effect 的依赖 —— 进了的话，
  // 父组件每重渲染一次（传进来的箭头函数都是新的）就会多存一遍。
  const onSavedRef = useRef(onSaved);
  useEffect(() => {
    onSavedRef.current = onSaved;
  });

  // 改一项存一项。`enabled` 传 null = 只存偏好，别顺手把调度打开。
  // 存失败不弹错：赛道照样看得见，下次开调度时会再存一遍。
  useEffect(() => {
    if (!loaded) return;
    void stationApi
      .scheduleSet(client, null, prefs)
      .then((saved) => onSavedRef.current?.(saved))
      .catch(() => {});
  }, [loaded, prefs, client]);

  const toggle = (a: Axis) => {
    const has = prefs.axes.includes(a);
    // 一项都不勾是合法的 —— 那表示「随便，别乱换」，现任会一直留着。
    setPrefs({
      ...prefs,
      axes: has ? prefs.axes.filter((x) => x !== a) : [...prefs.axes, a],
    });
  };

  const ranking = view?.ranking;
  const winner = ranking?.winner ?? null;

  return (
    <>
      <div className="qb-st-verdict">
        <span className="lead">现在会走</span>
        <span className="big">
          {winner ? routeLabelFromId(winner) : "没有可用线路"}
          {ranking?.held_by_hysteresis && (
            <small>现任留任 · 挑战者没领先够 8%</small>
          )}
        </span>
        <span style={{ flex: 1 }} />
        {winner && (
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => void stationApi.selectRoute(winner, false)}
          >
            用这一条（下次请求生效）
          </Button>
        )}
      </div>

      {error && <div className="qb-st-nudge">排序失败：{error}</div>}

      <div className="qb-st-deck">
        <div className="qb-card">
          <h3>按什么挑</h3>
          <div className="qb-st-goals">
            {AXES.map((a) => {
              const on = prefs.axes.includes(a);
              const flat = ranking?.indistinguishable.includes(a);
              return (
                <label key={a} className={`qb-st-goal${on ? " on" : ""}`}>
                  <input
                    type="checkbox"
                    checked={on}
                    onChange={() => toggle(a)}
                  />
                  <span style={{ flex: 1, minWidth: 0 }}>
                    <span className="gn">{AX[a].label}</span>
                    <span className="gd">
                      {AX[a].desc}
                      {on && flat && " · 本页线路在这一项上分不出高下，已忽略"}
                    </span>
                  </span>
                </label>
              );
            })}
          </div>

          <p className="qb-st-note">
            勾的几项里，<b>最弱的那一项最好</b>的那条赢。勾得越多，越不会拿到
            「某一项特别烂」的线路；一项都不勾就是「随便，别乱换」。
          </p>

          <h3 style={{ marginTop: 12 }}>底线（可以不设）</h3>
          <p className="qb-st-note">先筛掉，不参与排序。</p>
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
                setPrefs({
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
                setPrefs({
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
              note="按实测倍率比，没检验过的按标称"
              value={prefs.floors.max_rate}
              fallback={0.5}
              step={0.01}
              onChange={(v) =>
                setPrefs({ ...prefs, floors: { ...prefs.floors, max_rate: v } })
              }
            />
          </div>

          <div
            style={{ display: "flex", gap: 6, flexWrap: "wrap", marginTop: 12 }}
          >
            {PRESETS.map((p) => (
              <Button
                key={p.name}
                variant="ghost"
                onClick={() => setPrefs(p.prefs)}
              >
                {p.name}
              </Button>
            ))}
            <Button variant="ghost" onClick={() => setPrefs(DEFAULT_PREFS)}>
              默认
            </Button>
          </div>
        </div>

        <div className="qb-card">
          <h3>赛道</h3>
          <p className="qb-st-note">
            {ranking?.rows.length
              ? `每根条 = 跟本页最好那条的比值，1.00 最好${
                  view?.basis === "cost-per-token"
                    ? "。「便宜」用的是 24h 实扣 ÷ 实际 token（算进了缓存命中）"
                    : view?.basis === "blended-ratio"
                      ? "。「便宜」按这条线实际的输入输出比加权 —— 开了计费翻倍的站，输出多就会被算贵"
                      : view?.basis === "real-rate"
                        ? "。「便宜」用的是真实倍率 —— 连 token 结构都拿不到，整池退回这个粗口径"
                        : ""
                }`
              : "还没有可排的线路"}
          </p>
          <div className="qb-st-lanes">
            {ranking?.rows.map((row, i) => (
              <Lane key={row.route_id} row={row} index={i} />
            ))}
          </div>
          <p className="qb-st-note" style={{ marginTop: 10 }}>
            {ranking?.floors_relaxed && (
              <>
                ⚠ 你设的底线现在一条线路都满足不了 ——
                仍然照常走，但下面这些都是没过线的，别当成合格。
                <br />
              </>
            )}
            {ranking?.breakers_relaxed && (
              <>
                ⚠ 所有线路都在熔断中 —— 仍然留一条给你用，如实报错，不制造死局。
                <br />
              </>
            )}
            {ranking && ranking.indistinguishable.length > 0 && (
              <>
                「
                {ranking.indistinguishable.map((a) => AX[a].label).join("」「")}
                」在本页线路上差距太小，分不出高下，没有参与排序。
                <br />
              </>
            )}
            换上游会让上游那边的 prompt 缓存作废，所以挑战者要领先现任 8% 才换。
          </p>
        </div>
      </div>

      <div className="qb-card">
        <h3>熔断</h3>
        <p className="qb-st-note">三档分开，合并成一档必然误伤。</p>
        <div className="qb-st-brks">
          <div className="qb-st-brk warn">
            <span className="trig">429 限流</span>
            <span className="act">只降权，留在池子里</span>
            <span className="set">
              听 Retry-After；读不到就默认 60 秒。被限流说明这条线是通的。
            </span>
          </div>
          <div className="qb-st-brk bad">
            <span className="trig">401 / 403 / 402</span>
            <span className="act">立即熔断</span>
            <span className="set">
              令牌失效 / 没权限 / 余额不足，重试没有意义
            </span>
          </div>
          <div className="qb-st-brk bad">
            <span className="trig">5xx / 超时</span>
            <span className="act">连续 5 次才熔断</span>
            <span className="set">
              单发 500 是噪声，一次抖动不该被放大成故障
            </span>
          </div>
          <div className="qb-st-brk">
            <span className="trig">熔断之后</span>
            <span className="act">半开回探</span>
            <span className="set">
              30 秒起退避、封顶 10 分钟；到期后带重降权重新参与竞争
            </span>
          </div>
          <div className="qb-st-brk">
            <span className="trig">全熔断了</span>
            <span className="act">仍留一条线路</span>
            <span className="set">如实报错，不制造死局</span>
          </div>
          <div className="qb-st-brk">
            <span className="trig">切换时机</span>
            <span className="act">两次请求之间</span>
            <span className="set">不打断进行中的那一条</span>
          </div>
        </div>
      </div>
    </>
  );
}

function Floor({
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
