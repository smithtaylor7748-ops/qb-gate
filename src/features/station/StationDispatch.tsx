/**
 * 派单队列与测活（中转站重构阶段 2）：调度这一轮把线排成什么样、每条线是死是活、测活花了多少。
 *
 * 判定全在 Rust —— 分档与防抖在 `qb-station::schedule::dispatch`，死活在 `schedule::liveness`，
 * 这里**只显示**，跟赛道那边同一条规矩：两份判定必然漂移。
 *
 * ⛔ 花钱测活默认关（使用者选的）。开关、预算都摆在这一块里，「今天测活约 ¥a～¥b」照实写 ——
 * 自动花钱的事必须让人一眼看得见花了多少。
 */

import { useCallback, useEffect, useState } from "react";

import type { Client } from "../../lib/generated/Client";
import type { DispatchParams } from "../../lib/generated/DispatchParams";
import type { DispatchTier } from "../../lib/generated/DispatchTier";
import type { LineLiveness } from "../../lib/generated/LineLiveness";
import type { ProbeState } from "../../lib/generated/ProbeState";
import {
  routeLabelFromId,
  stationApi,
  type DispatchView,
  type ProbeSettings,
} from "../../lib/station";
import { Button, useToast } from "../../ui";

const TIER: Record<DispatchTier, { label: string; pill: string }> = {
  pinned: { label: "钉住", pill: "qb-st-pill--accent" },
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
  switch: "换了第一条",
  reorder: "热备改了顺序",
  pin: "钉住",
  unpin: "取消钉住",
  "probe-burst": "紧急测活",
};

/** 界面每隔多久重读一次（调度一跳 30 秒）。 */
const REFRESH_MS = 15_000;

const DEFAULT_PARAMS: DispatchParams = {
  margin: 0.08,
  min_samples: 3,
  confirm: 2,
};

export default function StationDispatch({ client }: { client: Client }) {
  const toast = useToast();
  const [view, setView] = useState<DispatchView | null>(null);
  const [params, setParams] = useState<DispatchParams>(DEFAULT_PARAMS);
  const [error, setError] = useState<string | null>(null);
  const [probing, setProbing] = useState(false);

  const load = useCallback(async () => {
    try {
      setView(await stationApi.dispatchView(client));
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [client]);

  useEffect(() => {
    void load();
    const t = setInterval(() => void load(), REFRESH_MS);
    return () => clearInterval(t);
  }, [load]);

  // 防抖参数存在这个软件的调度那一份里。
  useEffect(() => {
    let dropped = false;
    void stationApi
      .schedules()
      .then((all) => {
        const mine = all.find((s) => s.client === client);
        if (!dropped && mine) setParams(mine.dispatch);
      })
      .catch(() => {});
    return () => {
      dropped = true;
    };
  }, [client]);

  const probeNow = async (routeId?: string) => {
    if (probing) return;
    setProbing(true);
    try {
      const got = await stationApi.probeNow(client, routeId ?? null);
      const bad = got.filter((a) => !a.ok).length;
      toast.ok(
        bad === 0
          ? `测了 ${got.length} 次，都通`
          : `测了 ${got.length} 次，${bad} 次没通（原因在表里）`,
      );
      await load();
    } catch (e) {
      toast.error(`测活失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setProbing(false);
    }
  };

  const pin = async (routeId: string | null) => {
    try {
      await stationApi.pin(client, routeId);
      toast.ok(
        routeId
          ? "已钉住 4 小时：下一发请求就走它，这期间调度不换走它（它挂了就不算）"
          : "已取消钉住，交回调度",
      );
      await load();
    } catch (e) {
      toast.error(`钉住失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  // 先改界面再存（勾选框、数字框受控于这一份，等存完才变的话点了像没反应）；存失败就退回原来那份。
  const saveSettings = async (next: ProbeSettings) => {
    const before = view?.settings;
    setView((v) => (v ? { ...v, settings: next } : v));
    try {
      const saved = await stationApi.saveProbeSettings(next);
      setView((v) => (v ? { ...v, settings: saved } : v));
    } catch (e) {
      if (before) setView((v) => (v ? { ...v, settings: before } : v));
      toast.error(
        `测活设置没存上：${e instanceof Error ? e.message : String(e)}`,
      );
    }
  };

  const saveParams = async (next: DispatchParams) => {
    setParams(next);
    try {
      await stationApi.scheduleSet(client, null, null, next);
    } catch (e) {
      toast.error(
        `防抖设置没存上：${e instanceof Error ? e.message : String(e)}`,
      );
    }
  };

  if (error && !view)
    return <div className="qb-st-nudge">读不到派单队列：{error}</div>;
  if (!view) return <p className="qb-st-note">读取中…</p>;

  const { queue, settings } = view;
  const now = Date.now();
  const pinned =
    queue.pin && Number(queue.pin.until_ms) > now ? queue.pin : null;
  const rowOf = new Map(queue.rows.map((r) => [r.route_id, r]));

  return (
    <>
      <div className="qb-card">
        <div
          className="qb-st-inline"
          style={{ justifyContent: "space-between" }}
        >
          <h3>派单队列</h3>
          <Button
            variant="ghost"
            disabled={probing}
            onClick={() => void probeNow()}
          >
            {probing ? "测活中…" : "立刻测"}
          </Button>
        </div>
        <p className="qb-st-note">
          {view.enabled
            ? "第一条是当前上游，后面依次是它失败时改用的热备。调度每 30 秒排一次：熔断中或测活判死的排最后，没过底线的兜底 —— 这两档永远不会被换上来当第一。"
            : "调度关着：这是你在「路由设置」里排的替补顺序，档位那一列空着。测活只在调度开着时自动跑，「立刻测」随时可以点。"}
        </p>

        {queue.alerts.map((a) => (
          <div key={a} className="qb-st-nudge">
            {a}
          </div>
        ))}

        {pinned && (
          <div className="qb-st-nudge qb-st-dq">
            已钉住「{routeLabelFromId(pinned.route_id)}」到{" "}
            {clock(Number(pinned.until_ms))}（它挂了就不算）
            <button className="qb-st-iconbtn" onClick={() => void pin(null)}>
              取消钉住
            </button>
          </div>
        )}

        {view.lines.length === 0 ? (
          <p className="qb-st-note">这个软件底下还没有线路。</p>
        ) : (
          <div className="qb-st-tw qb-st-dq">
            <table>
              <thead>
                <tr>
                  <th className="num">#</th>
                  <th>线路</th>
                  <th>档位</th>
                  <th>状态</th>
                  <th className="num">速度</th>
                  <th className="num">失败率</th>
                  <th className="num">折合价</th>
                  <th>上次 / 下次测活</th>
                  <th className="num">24h</th>
                  <th>原因</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {view.lines.map((line) => (
                  <Line
                    key={line.route_id}
                    line={line}
                    row={rowOf.get(line.route_id)}
                    current={queue.current === line.route_id}
                    pinned={pinned?.route_id === line.route_id}
                    scheduled={view.enabled}
                    now={now}
                    onPin={(id) => void pin(id)}
                    onProbe={(id) => void probeNow(id)}
                    probing={probing}
                  />
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      <div className="qb-st-deck">
        <div className="qb-card">
          <h3>测活</h3>
          <p className="qb-st-note">
            免费的那种（调度开着时一直跑）发一个空请求，只证明「线通、Key
            认」，测不出快慢；最近走过真实流量的线不测 —— 真实流量本身就是证据。
          </p>
          <div className="qb-st-floors">
            <div className="qb-st-floor">
              <input
                type="checkbox"
                checked={settings.paid}
                aria-label="开启花钱测活"
                onChange={(e) =>
                  void saveSettings({ ...settings, paid: e.target.checked })
                }
              />
              <label>
                花钱测活（默认关）
                <span style={{ fontSize: 11, color: "var(--text-3)" }}>
                  {" "}
                  真打一发最小的请求（输入 hi、输出 1 个
                  token），量出谁快；开了「快」整池按它比
                </span>
              </label>
            </div>
            <div className={`qb-st-floor${settings.paid ? "" : " off"}`}>
              <span />
              <label>
                每天最多花 ¥{" "}
                <input
                  className="qb-st-num"
                  type="number"
                  step={0.5}
                  min={0}
                  aria-label="每天最多花多少元"
                  value={settings.daily_budget_cny}
                  onChange={(e) =>
                    void saveSettings({
                      ...settings,
                      daily_budget_cny: Number(e.target.value),
                    })
                  }
                />
              </label>
            </div>
            <div className="qb-st-floor">
              <span />
              <label>
                免费探测每{" "}
                <input
                  className="qb-st-num"
                  type="number"
                  step={1}
                  min={1}
                  aria-label="免费探测间隔（分钟）"
                  value={settings.free_interval_min}
                  onChange={(e) =>
                    void saveSettings({
                      ...settings,
                      free_interval_min: Math.max(1, Number(e.target.value)),
                    })
                  }
                />{" "}
                分钟一次（判死了的按 5 → 60 分钟退避）
              </label>
            </div>
          </div>
          <p className="qb-st-note">
            今天测活约 ¥{view.spent_today[0].toFixed(4)}～¥
            {view.spent_today[1].toFixed(4)}
            （按这家站的价和充值比例估的；预算按上界扣，不知道价的线按偏贵的价记）
          </p>
        </div>

        <div className="qb-card">
          <h3>换第一条的防抖</h3>
          <p className="qb-st-note">
            挑战者要同时满足这三条才把现任换下来；现任熔断了、判死了、没过底线，有合格的就立刻换，不谈防抖。
          </p>
          <div className="qb-st-floors">
            <div className="qb-st-floor">
              <span />
              <label>
                领先现任至少{" "}
                <input
                  className="qb-st-num"
                  type="number"
                  step={1}
                  min={0}
                  max={100}
                  aria-label="领先现任至少多少（%）"
                  value={Math.round(params.margin * 100)}
                  onChange={(e) =>
                    void saveParams({
                      ...params,
                      margin: Number(e.target.value) / 100,
                    })
                  }
                />{" "}
                %
              </label>
            </div>
            <div className="qb-st-floor">
              <span />
              <label>
                自己至少{" "}
                <input
                  className="qb-st-num"
                  type="number"
                  step={1}
                  min={1}
                  aria-label="挑战者至少几个样本"
                  value={params.min_samples}
                  onChange={(e) =>
                    void saveParams({
                      ...params,
                      min_samples: Math.max(1, Number(e.target.value)),
                    })
                  }
                />{" "}
                个样本
              </label>
            </div>
            <div className="qb-st-floor">
              <span />
              <label>
                连续{" "}
                <input
                  className="qb-st-num"
                  type="number"
                  step={1}
                  min={1}
                  aria-label="连续几轮有新证据都领先"
                  value={params.confirm}
                  onChange={(e) =>
                    void saveParams({
                      ...params,
                      confirm: Math.max(1, Number(e.target.value)),
                    })
                  }
                />{" "}
                轮有新证据都领先
              </label>
            </div>
          </div>
          <p className="qb-st-note">
            换上游会让上游那边的 prompt 缓存作废，所以默认要领先 8%、连续 2 轮。
          </p>
        </div>
      </div>

      <div className="qb-card">
        <h3>动作日志</h3>
        {view.actions.length === 0 ? (
          <p className="qb-st-note">调度还没做过什么。</p>
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
  pinned,
  scheduled,
  now,
  onPin,
  onProbe,
  probing,
}: {
  line: LineLiveness;
  row: DispatchView["queue"]["rows"][number] | undefined;
  current: boolean;
  pinned: boolean;
  scheduled: boolean;
  now: number;
  onPin: (id: string | null) => void;
  onProbe: (id: string) => void;
  probing: boolean;
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
      <td className="num">{row ? row.position : "—"}</td>
      <td>
        {line.label || routeLabelFromId(line.route_id)}
        {current && <span className="qb-st-pill qb-st-pill--ok">当前</span>}
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
      <td>
        <span className="qb-st-inline">
          <button
            className="qb-st-iconbtn"
            disabled={probing}
            onClick={() => onProbe(line.route_id)}
          >
            测
          </button>
          {scheduled &&
            (pinned ? (
              <button className="qb-st-iconbtn" onClick={() => onPin(null)}>
                取消钉住
              </button>
            ) : (
              <button
                className="qb-st-iconbtn"
                title="下一发请求就走它，4 小时内调度不换走它（它挂了就不算）"
                onClick={() => onPin(line.route_id)}
              >
                钉住
              </button>
            ))}
        </span>
      </td>
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
