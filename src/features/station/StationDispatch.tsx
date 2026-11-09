/**
 * 派单队列与测活（中转站重构阶段 2）：调度这一轮把线排成什么样、每条线是死是活、测活花了多少。
 *
 * 判定全在 Rust —— 分档与防抖在 `qb-station::schedule::dispatch`，死活在 `schedule::liveness`，
 * 这里**只显示**，跟赛道那边同一条规矩：两份判定必然漂移。
 *
 * ⛔ 花钱测活默认关（使用者选的）。开关、预算都摆在这一块里，「今天测活约 ¥a～¥b」照实写 ——
 * 自动花钱的事必须让人一眼看得见花了多少。
 */

import { useCallback, useEffect, useRef, useState } from "react";

import type { Client } from "../../lib/generated/Client";
import type { DispatchParams } from "../../lib/generated/DispatchParams";
import type { DispatchTier } from "../../lib/generated/DispatchTier";
import type { LineLiveness } from "../../lib/generated/LineLiveness";
import type { ProbeState } from "../../lib/generated/ProbeState";
import type { RouterRequest } from "../../lib/generated/RouterRequest";
import {
  routeLabelFromId,
  stationApi,
  type DispatchView,
  type ProbeSettings,
  type RouterSettings,
} from "../../lib/station";
import { Button, useToast } from "../../ui";
import { StationAdmissionStatus } from "./StationAdmissionStatus";
import {
  RouteAttemptTrail,
  StationRouteBoundary,
} from "./StationRouteBoundary";

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

// 自动保存必须跨弹窗挂载按序完成；测活设置为全局，防抖参数由请求中的 client 区分。
const probeWriteQueue = { current: Promise.resolve() as Promise<void> };
const dispatchWriteQueue = { current: Promise.resolve() as Promise<void> };

export default function StationDispatch({ client }: { client: Client }) {
  const toast = useToast();
  const [view, setView] = useState<DispatchView | null>(null);
  const [router, setRouter] = useState<Awaited<
    ReturnType<typeof stationApi.routerStatus>
  > | null>(null);
  const [params, setParams] = useState<DispatchParams>(DEFAULT_PARAMS);
  const [error, setError] = useState<string | null>(null);
  const [probing, setProbing] = useState(false);
  const [policy, setPolicy] = useState<RouterSettings | null>(null);
  const clientRef = useRef(client);
  clientRef.current = client;
  const readSequence = useRef(0);
  const probeWrites = probeWriteQueue;
  const paramsWrites = dispatchWriteQueue;
  const probeRevision = useRef(0);
  const paramsRevision = useRef(0);
  const probeDirty = useRef(false);
  const probeDraft = useRef<ProbeSettings | null>(null);
  const mounted = useRef(true);
  const [probeError, setProbeError] = useState<string | null>(null);
  const [paramsError, setParamsError] = useState<string | null>(null);
  const [probeSaving, setProbeSaving] = useState(false);
  const [paramsSaving, setParamsSaving] = useState(false);
  const [paramsLoadedFor, setParamsLoadedFor] = useState<Client | null>(null);
  const [paramsReload, setParamsReload] = useState(0);
  const [latestRequest, setLatestRequest] = useState<RouterRequest | null>(
    null,
  );

  const load = useCallback(async () => {
    const read = ++readSequence.current;
    const probeRead = probeRevision.current;
    const probeWasDirty = probeDirty.current;
    try {
      if (!probeDirty.current) await probeWrites.current;
      const [nextView, nextRouter, nextPolicy, requests] = await Promise.all([
        stationApi.dispatchView(client),
        stationApi.routerStatus(client),
        stationApi.routerSettings(client),
        stationApi.usageRequests({
          client,
          routeId: null,
          model: null,
          failedOnly: false,
          offset: 0,
          limit: 1,
        }),
      ]);
      if (clientRef.current !== client || read !== readSequence.current) return;
      setView(
        probeDraft.current &&
          (probeDirty.current ||
            probeWasDirty ||
            probeRead !== probeRevision.current)
          ? { ...nextView, settings: probeDraft.current }
          : nextView,
      );
      setRouter(nextRouter);
      setPolicy(nextPolicy);
      setLatestRequest(requests.items[0] ?? null);
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
    setLatestRequest(null);
    setError(null);
    void load();
    const t = setInterval(() => void load(), REFRESH_MS);
    return () => {
      clearInterval(t);
      readSequence.current++;
    };
  }, [load]);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      probeRevision.current++;
      paramsRevision.current++;
    };
  }, []);

  // 防抖参数存在这个软件的调度那一份里。
  useEffect(() => {
    let dropped = false;
    setParamsLoadedFor(null);
    setParamsError(null);
    setParamsSaving(false);
    void paramsWrites.current
      .catch(() => {})
      .then(() => stationApi.schedules())
      .then((all) => {
        const mine = all.find((s) => s.client === client);
        if (dropped) return;
        if (!mine) throw new Error("未返回这个软件的防抖设置");
        setParams(mine.dispatch);
        setParamsLoadedFor(client);
      })
      .catch((e: unknown) => {
        if (!dropped)
          setParamsError(
            `防抖设置读取失败：${e instanceof Error ? e.message : String(e)}`,
          );
      });
    return () => {
      dropped = true;
      paramsRevision.current++;
    };
  }, [client, paramsReload]);

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
          ? policy?.admission.overflow_to_pool &&
            policy?.pool_strategy === "least-loaded"
            ? "已固定池内首选 4 小时；优先空闲线路模式下仅影响相同负载时的顺序"
            : "已固定池内首选 4 小时；首选不可用时仍允许名单内备用线路"
          : "已取消钉住，交回调度",
      );
      await load();
    } catch (e) {
      toast.error(`钉住失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  // 按编辑顺序写入；旧结果不覆盖新草稿，定时刷新也不盖掉未保存的设置。
  const saveSettings = (next: ProbeSettings) => {
    const revision = ++probeRevision.current;
    // 测活设置是全局的，切换客户端后仍需展示这次编辑，不能回到旧落盘值。
    const current = () => mounted.current && revision === probeRevision.current;
    probeDirty.current = true;
    probeDraft.current = next;
    setProbeSaving(true);
    setProbeError(null);
    setView((v) => (v ? { ...v, settings: next } : v));
    probeWrites.current = probeWrites.current
      .catch(() => {})
      .then(async () => {
        const saved = await stationApi.saveProbeSettings(next);
        if (current()) {
          probeDirty.current = false;
          probeDraft.current = saved;
          setView((v) => (v ? { ...v, settings: saved } : v));
        }
      })
      .catch((e: unknown) => {
        if (current())
          setProbeError(
            `测活设置未保存：${e instanceof Error ? e.message : String(e)}`,
          );
      })
      .finally(() => {
        if (current()) setProbeSaving(false);
      });
  };

  const saveParams = (next: DispatchParams) => {
    if (paramsLoadedFor !== client) return;
    const revision = ++paramsRevision.current;
    const current = () =>
      mounted.current &&
      revision === paramsRevision.current &&
      clientRef.current === client;
    setParams(next);
    setParamsSaving(true);
    setParamsError(null);
    paramsWrites.current = paramsWrites.current
      .catch(() => {})
      .then(async () => {
        const saved = await stationApi.scheduleSet(client, null, null, next);
        if (current()) setParams(saved.dispatch);
      })
      .catch((e: unknown) => {
        if (current())
          setParamsError(
            `防抖设置未保存：${e instanceof Error ? e.message : String(e)}`,
          );
      })
      .finally(() => {
        if (current()) setParamsSaving(false);
      });
  };

  if (error && !view)
    return <div className="qb-st-nudge">读不到派单队列：{error}</div>;
  if (!view) return <p className="qb-st-note">读取中…</p>;

  const { queue, settings } = view;
  const now = Date.now();
  const runtimeCurrent = router?.current_route ?? null;
  const runtimePending = router?.pending_route ?? null;
  const effectiveCurrent = runtimePending ?? runtimeCurrent;
  const persistedDiffers =
    queue.current != null &&
    effectiveCurrent != null &&
    queue.current !== effectiveCurrent;
  const pinned =
    queue.pin && Number(queue.pin.until_ms) > now ? queue.pin : null;
  const rowOf = new Map(queue.rows.map((r) => [r.route_id, r]));

  return (
    <>
      <StationRouteBoundary
        settings={policy}
        selected={effectiveCurrent}
        routeLabel={routeLabelFromId}
        error={error}
      />
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
          {policy?.routing_mode === "pool" &&
          policy.admission.overflow_to_pool &&
          policy.pool_strategy === "least-loaded"
            ? "优先空闲线路已开启：每次请求按实时负载在允许名单内分配，所选线路正常时也可能使用其他站。下方队列提供候选顺序。"
            : view.enabled
              ? "调度只在允许名单内排序，首选失败时可依次使用名单内备用线路。切换只影响尚未发出的请求。"
              : policy?.routing_mode === "pool"
                ? "调度关着：仅允许使用已勾选线路，首选失败时按名单顺序尝试备用。"
                : "仅用所选线路：失败直接报错。其他线路仅供查看，调度不会把请求转给它们。"}
        </p>

        <div className="qb-st-dispatch-runtime" aria-live="polite">
          <span className="qb-st-runtime-chip">
            <span className="dot" />
            当前选线：
            {runtimeCurrent ? routeLabelFromId(runtimeCurrent) : "尚未选线"}
          </span>
          {runtimePending && runtimePending !== runtimeCurrent && (
            <span className="qb-st-runtime-chip pending">
              下一发：{routeLabelFromId(runtimePending)}
            </span>
          )}
          {router?.running === false && (
            <span className="qb-st-runtime-chip warn">本机路由未启动</span>
          )}
          {persistedDiffers && (
            <span className="qb-st-runtime-note">
              落盘记录是「{routeLabelFromId(queue.current!)}
              」；已以正在运行的选择为准，避免旧快照覆盖新操作。
            </span>
          )}
          <span className="qb-st-runtime-note">
            {latestRequest ? (
              <>
                最近请求 · {ago(Number(latestRequest.at_ms), now)}
                {latestRequest.attempts.length > 0 &&
                  ` · 实际尝试 ${latestRequest.attempts.length} 条线路，路径见下方`}
                {latestRequest.fault === "route-permission-denied"
                  ? latestRequest.attempts.length === 0
                    ? " · 未发往上游：线路超出允许范围"
                    : " · 后续线路超出允许范围，已阻止"
                  : latestRequest.fault === "route-admission-denied"
                    ? latestRequest.attempts.length === 0
                      ? " · 未发往上游：本机请求容量限制"
                      : " · 后续尝试受本机请求容量限制"
                    : latestRequest.fault && " · 这次请求有错误"}
                {latestRequest.model.startsWith("qbg") ||
                latestRequest.model.startsWith("claude-fable-qbg")
                  ? " · 客户端指定了聚合模型；同样受允许范围限制"
                  : ""}
              </>
            ) : (
              "尚无请求记录；开始一次新请求后可在这里核对实际去向。"
            )}
          </span>
        </div>

        {latestRequest && (
          <RouteAttemptTrail
            request={latestRequest}
            routeLabel={routeLabelFromId}
          />
        )}

        {queue.alerts.map((a) => (
          <div key={a} className="qb-st-nudge">
            {a}
          </div>
        ))}

        {pinned && (
          <div className="qb-st-nudge qb-st-dq">
            已钉住「{routeLabelFromId(pinned.route_id)}」到{" "}
            {clock(Number(pinned.until_ms))}
            {policy?.admission.overflow_to_pool &&
            policy?.pool_strategy === "least-loaded"
              ? "。优先空闲线路模式下，固定首选只影响相同负载时的顺序；要固定每次请求，请改用仅所选线路。"
              : "，首选不可用时仍可使用名单内备用线路"}
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
                    current={runtimeCurrent === line.route_id}
                    pinned={pinned?.route_id === line.route_id}
                    scheduled={
                      view.enabled &&
                      policy?.routing_mode === "pool" &&
                      policy.allowed_routes.includes(line.route_id)
                    }
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
          {probeSaving && (
            <p className="qb-st-note" role="status">
              正在保存测活设置…
            </p>
          )}
          {probeError && (
            <p className="qb-st-nudge" role="alert">
              {probeError}
              <Button variant="ghost" onClick={() => saveSettings(settings)}>
                重试保存
              </Button>
            </p>
          )}
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
          {paramsSaving && (
            <p className="qb-st-note" role="status">
              正在保存防抖设置…
            </p>
          )}
          {paramsError && (
            <p className="qb-st-nudge" role="alert">
              {paramsError}
              <Button
                variant="ghost"
                onClick={() =>
                  paramsLoadedFor === client
                    ? saveParams(params)
                    : setParamsReload((v) => v + 1)
                }
              >
                重试
              </Button>
            </p>
          )}
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
                  disabled={paramsLoadedFor !== client}
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
                  disabled={paramsLoadedFor !== client}
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
                  disabled={paramsLoadedFor !== client}
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
                title="固定名单内首选 4 小时；仍允许使用名单内备用线路"
                onClick={() => onPin(line.route_id)}
              >
                固定池内首选
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
