/**
 * 检测与排名（「选线与分流」弹窗 · 高级设置里）：当前线路和备用线路每条是死是活、快慢、失败率、折合价，
 * 自动选线这一轮把它们分进哪一档，手动「重新检测」，以及自动选线做过的动作。
 *
 * 判定全在 Rust —— 分档与防抖在 `qb-station::schedule::dispatch`，死活在 `schedule::liveness`，
 * 这里**只显示**：两份判定必然漂移。
 *
 * 2026-11-15 起：
 * - 弹窗里重复的那份范围卡和「最近请求去向」删了（D4）—— 主页上就有；
 * - 测活频率与花钱测活的预算挪到「设置 → 中转站」（D8：五个软件共用，原来挂在每个软件的弹窗里）；
 * - 换首选的防抖挪进弹窗「高级设置」那一节，跟别的设置一起由底部的「保存」落盘（F2）；
 * - 「立刻测」「测」合成一个「重新检测」（D7）：只测当前线路 + 备用线路，先只发免费的，
 *   开着花钱测活时只给预估、再点一下才发（C6）。熔断那一种叫「重试这条」，两件事不同名。
 */

import { useCallback, useEffect, useRef, useState } from "react";

import type { Client } from "../../lib/generated/Client";
import type { DispatchTier } from "../../lib/generated/DispatchTier";
import type { LineLiveness } from "../../lib/generated/LineLiveness";
import type { ProbeState } from "../../lib/generated/ProbeState";
import {
  routeLabelFromId,
  stationApi,
  type DispatchView,
  type RouterSettings,
} from "../../lib/station";
import { Button, useToast } from "../../ui";
import { StationAdmissionStatus } from "./StationAdmissionStatus";

const TIER: Record<DispatchTier, { label: string; pill: string }> = {
  qualified: { label: "合格", pill: "qb-st-pill--ok" },
  fallback: { label: "兜底", pill: "qb-st-pill--warn" },
  faulty: { label: "故障", pill: "qb-st-pill--danger" },
};

const STATE: Record<ProbeState, { label: string; pill: string }> = {
  up: { label: "活着", pill: "qb-st-pill--ok" },
  down: { label: "判死", pill: "qb-st-pill--danger" },
  untested: { label: "没测过", pill: "qb-st-pill--unknown" },
};

const KIND: Record<string, string> = {
  switch: "换了当前线路",
  reorder: "改了备用顺序",
  // 钉住 2026-11-14 删了；动作日志里的旧记录照样认得出来。
  pin: "旧版固定线路（已停用的旧记录）",
  unpin: "旧版取消固定（已停用的旧记录）",
  "probe-burst": "紧急检测",
};

/** 界面每隔多久重读一次（自动选线一跳 30 秒）。 */
const REFRESH_MS = 15_000;

export default function StationDispatch({ client }: { client: Client }) {
  const toast = useToast();
  const [view, setView] = useState<DispatchView | null>(null);
  const [router, setRouter] = useState<Awaited<
    ReturnType<typeof stationApi.routerStatus>
  > | null>(null);
  const [policy, setPolicy] = useState<RouterSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [probing, setProbing] = useState(false);
  /** 免费测完之后，还能花钱测几条、最多多少钱（等使用者点头）。 */
  const [paidOffer, setPaidOffer] = useState<{
    count: number;
    estimate: number;
    overBudget: number;
  } | null>(null);
  const clientRef = useRef(client);
  clientRef.current = client;
  const readSequence = useRef(0);

  const load = useCallback(async () => {
    const read = ++readSequence.current;
    try {
      const [nextView, nextRouter, nextPolicy] = await Promise.all([
        stationApi.dispatchView(client),
        stationApi.routerStatus(client),
        stationApi.routerSettings(client),
      ]);
      if (clientRef.current !== client || read !== readSequence.current) return;
      setView(nextView);
      setRouter(nextRouter);
      setPolicy(nextPolicy);
      setError(null);
    } catch (e) {
      if (clientRef.current === client && read === readSequence.current)
        setError(e instanceof Error ? e.message : String(e));
    }
  }, [client]);

  useEffect(() => {
    setView(null);
    setRouter(null);
    setPolicy(null);
    setError(null);
    void load();
    const t = setInterval(() => void load(), REFRESH_MS);
    return () => {
      clearInterval(t);
      readSequence.current++;
    };
  }, [load]);

  /**
   * 「重新检测」：只测当前线路和备用线路。先只发免费的；开着花钱测活时后端顺便算出能花钱测哪几条、最多多少钱，
   * 由使用者再点一下才发（`paid: true`）。
   */
  const probeNow = async (paid = false) => {
    if (probing) return;
    setProbing(true);
    setPaidOffer(null);
    try {
      const got = await stationApi.probeNow(client, null, paid);
      const n = got.attempts.length;
      const bad = got.attempts.filter((a) => !a.ok).length;
      toast.ok(
        bad === 0
          ? `测了 ${n} 次，都通`
          : `测了 ${n} 次，${bad} 次没通（原因在表里）`,
      );
      if (!paid && got.paid_pending.length > 0)
        setPaidOffer({
          count: got.paid_pending.length,
          estimate: got.paid_estimate_cny,
          overBudget: got.over_budget,
        });
      await load();
    } catch (e) {
      toast.error(`检测失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setProbing(false);
    }
  };

  if (error && !view)
    return <div className="qb-st-nudge">读不到检测记录：{error}</div>;
  if (!view) return <p className="qb-st-note">读取中…</p>;

  const { queue } = view;
  const now = Date.now();
  const current =
    router?.selection.current ??
    router?.pending_route ??
    router?.current_route ??
    null;
  const rowOf = new Map(queue.rows.map((r) => [r.route_id, r]));
  // 只列当前线路 + 备用线路：名单外的线不会被试，也不测（C6）。
  const inPlay = new Set([
    ...(current ? [current] : []),
    ...(router?.selection.backups ?? []),
  ]);
  const lines = view.lines.filter((l) => inPlay.has(l.route_id));

  return (
    <>
      <StationAdmissionStatus
        settings={policy}
        status={router}
        routeLabel={routeLabelFromId}
        error={error}
        onRefresh={() => void load()}
      />
      <div className="qb-card">
        <div
          className="qb-st-inline"
          style={{ justifyContent: "space-between" }}
        >
          <h3>当前线路与备用线路的检测</h3>
          <Button
            variant="ghost"
            disabled={probing || lines.length === 0}
            title="只测当前线路和备用线路。先发免费的空请求（只证明线通、Key 认）；开着花钱测活时，花钱的那几条先给预估、你点了才发"
            onClick={() => void probeNow()}
          >
            {probing ? "检测中…" : "重新检测"}
          </Button>
        </div>
        <p className="qb-st-note">
          {view.enabled
            ? "自动选线开着：每一跳在当前线路和备用线路里重排、挑一条当当前线路，下表「档位」就是这一轮的结果。熔断中、判死、没过底线的线不会被换上来当当前线路。"
            : "自动选线关着：当前线路出故障时按备用线路的顺序依次改用，下表只是读数。"}
        </p>

        {queue.alerts.map((a) => (
          <div key={a} className="qb-st-nudge">
            {a}
          </div>
        ))}

        {paidOffer && (
          <div className="qb-st-nudge qb-st-dq" role="status">
            免费的测完了。花钱测活开着：还能花钱测 {paidOffer.count}{" "}
            条，这次最多约 ¥{paidOffer.estimate.toFixed(2)}
            （计入今天的测活预算）
            {paidOffer.overBudget > 0 &&
              `；另有 ${paidOffer.overBudget} 条今天的预算不够，不测`}
            。
            <button
              className="qb-st-iconbtn"
              disabled={probing}
              onClick={() => void probeNow(true)}
            >
              花钱测这几条
            </button>
          </div>
        )}

        {lines.length === 0 ? (
          <p className="qb-st-note">
            还没有当前线路：先在主页的线路行上点「设为当前」。
          </p>
        ) : (
          <div className="qb-st-tw qb-st-dq">
            <table>
              <thead>
                <tr>
                  <th>线路</th>
                  <th>档位</th>
                  <th>状态</th>
                  <th className="num">速度</th>
                  <th className="num">失败率</th>
                  <th className="num">折合价</th>
                  <th>上次 / 下次测活</th>
                  <th className="num">24h</th>
                  <th>原因</th>
                </tr>
              </thead>
              <tbody>
                {lines.map((line) => (
                  <Line
                    key={line.route_id}
                    line={line}
                    row={rowOf.get(line.route_id)}
                    current={current === line.route_id}
                    now={now}
                  />
                ))}
              </tbody>
            </table>
          </div>
        )}
        <p className="qb-st-note">
          今天测活约 ¥{view.spent_today[0].toFixed(4)}～¥
          {view.spent_today[1].toFixed(4)}
          （按这家站的价和充值比例估的；预算按上界扣，不知道价的线按偏贵的价记）。
          花钱测活的开关、每日预算、免费探测多久一次在「设置 →
          中转站」，五个软件共用。
        </p>
      </div>

      <div className="qb-card">
        <h3>自动选线做过的动作</h3>
        {view.actions.length === 0 ? (
          <p className="qb-st-note">自动选线还没做过什么。</p>
        ) : (
          <div className="qb-st-tw">
            <table>
              <tbody>
                {view.actions.map((a) => (
                  <tr key={a.id}>
                    <td className="qb-st-dim">{clock(Number(a.at_ms))}</td>
                    <td>{KIND[a.kind] ?? a.kind}</td>
                    <td>{a.route_id ? routeLabelFromId(a.route_id) : ""}</td>
                    <td>{a.note}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </>
  );
}

function Line({
  line,
  row,
  current,
  now,
}: {
  line: LineLiveness;
  row: DispatchView["queue"]["rows"][number] | undefined;
  current: boolean;
  now: number;
}) {
  const st = STATE[line.health.state];
  const reason = [
    row?.reason,
    line.health.state === "down" ? line.health.reason : null,
    line.health.missing_models.length > 0
      ? `上游没有：${line.health.missing_models.join("、")}`
      : null,
  ]
    .filter(Boolean)
    .join("；");
  return (
    <tr>
      <td>
        {line.label || routeLabelFromId(line.route_id)}
        {current && <span className="qb-st-pill qb-st-pill--ok">当前线路</span>}
      </td>
      <td>
        {row ? (
          <span className={`qb-st-pill ${TIER[row.tier].pill}`}>
            {TIER[row.tier].label}
          </span>
        ) : (
          <span className="qb-st-dim">—</span>
        )}
      </td>
      <td>
        <span className={`qb-st-pill ${st.pill}`} title={line.health.reason}>
          {st.label}
        </span>
      </td>
      <td className="num">
        {line.speed_ms == null
          ? "—"
          : `${(Number(line.speed_ms) / 1000).toFixed(2)}s · ${line.speed_samples} 次`}
      </td>
      <td className="num">
        {line.fail_rate == null ? "—" : `${Math.round(line.fail_rate * 100)}%`}
      </td>
      <td className="num" title="每 $1 官方牌价付几元（真实倍率 × 充值比例）">
        {line.real_rate == null ? "—" : `¥${line.real_rate.toFixed(2)}`}
      </td>
      <td>
        {line.last_probe_ms == null
          ? "没测过"
          : ago(Number(line.last_probe_ms), now)}
        {" / "}
        {line.next_probe_ms == null
          ? "有真实流量，不用测"
          : soon(Number(line.next_probe_ms), now)}
      </td>
      <td className="num">{line.requests_24h}</td>
      <td>{reason || <span className="qb-st-dim">—</span>}</td>
    </tr>
  );
}

function clock(ms: number): string {
  const d = new Date(ms);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  return `${hh}:${mm}`;
}

function ago(ms: number, now: number): string {
  const min = Math.round((now - ms) / 60_000);
  return min <= 0 ? "刚刚" : `${min} 分钟前`;
}

function soon(ms: number, now: number): string {
  const min = Math.round((ms - now) / 60_000);
  return min <= 0 ? "下一跳" : `${min} 分钟后`;
}
