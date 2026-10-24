/**
 * 本机路由设置（每个软件一份）：自动故障转移、派单顺序、三种超时、熔断参数、整流器开关。
 *
 * 照 cc-switch v4.0.14 的「代理设置」那一页（每个 app 一行 `proxy_config`）的布局和项目，
 * 用 QB Gate 自己的表单样式重做。
 *
 * # ⛔ 读的是库里那一份
 *
 * 跟调度设置一样：本机路由用的是存进库、又装进路由器的那份。只改本地 state 的话，
 * 界面上关了故障转移而路由还在换线 —— 两份判定漂开，而且看不出来。
 *
 * # 派单顺序只排「替补」
 *
 * 当前那条（行上高亮的那条）永远第一个试；这里排的是它失败之后依次改用谁。
 * 只列这个软件自己的线 —— 后端也只收这个软件的线，混进别的软件的线整份拒绝。
 */

import { useEffect, useMemo, useState } from "react";

import type { Client } from "../../lib/generated/Client";
import type { RouterStatus } from "../../lib/generated/RouterStatus";
import { stationApi, type Route, type RouterSettings } from "../../lib/station";
import { Button, useToast } from "../../ui";

export default function StationRouterSettings({
  client,
  routes,
  status,
  label,
  onSaved,
}: {
  client: Client;
  /** 这个软件的线（别的软件的不传进来）。 */
  routes: Route[];
  status: RouterStatus | null;
  label: (route: Route) => string;
  /** 存好之后回调（右列的状态要读回来）。 */
  onSaved?: () => void;
}) {
  const toast = useToast();
  const [settings, setSettings] = useState<RouterSettings | null>(null);
  const [order, setOrder] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let dropped = false;
    setSettings(null);
    setError(null);
    void Promise.all([
      stationApi.routerSettings(client),
      stationApi.dispatchQueue(client),
    ])
      .then(([s, q]) => {
        if (dropped) return;
        setSettings(s);
        setOrder(q.order);
      })
      .catch((e: unknown) => {
        if (!dropped) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      dropped = true;
    };
  }, [client]);

  const current = status?.current_route ?? null;
  const breakerOf = useMemo(
    () => new Map((status?.breakers ?? []).map((b) => [b.route_id, b])),
    [status],
  );
  // 排在顺序里的在前（照顺序），没排进去的跟在后面（照线路原来的顺序）。
  const rows = useMemo(() => {
    const byId = new Map(routes.map((r) => [r.id, r]));
    const queued = order
      .map((id) => byId.get(id))
      .filter((r): r is Route => r != null);
    const rest = routes.filter((r) => !order.includes(r.id));
    return [...queued, ...rest];
  }, [routes, order]);

  if (error && !settings)
    return <div className="qb-st-nudge">读不到路由设置：{error}</div>;
  if (!settings) return <p className="qb-st-note">读取中…</p>;

  const set = (patch: Partial<RouterSettings>) =>
    setSettings({ ...settings, ...patch });
  const setBreaker = (patch: Partial<RouterSettings["breaker"]>) =>
    setSettings({ ...settings, breaker: { ...settings.breaker, ...patch } });
  const setRect = (patch: Partial<RouterSettings["rectifier"]>) =>
    setSettings({
      ...settings,
      rectifier: { ...settings.rectifier, ...patch },
    });

  const move = (id: string, by: -1 | 1) => {
    const i = order.indexOf(id);
    const j = i + by;
    if (i < 0 || j < 0 || j >= order.length) return;
    const next = [...order];
    [next[i], next[j]] = [next[j], next[i]];
    setOrder(next);
  };
  const toggle = (id: string, on: boolean) =>
    setOrder(on ? [...order, id] : order.filter((x) => x !== id));

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const saved = await stationApi.saveRouterSettings(settings);
      const q = await stationApi.saveDispatchQueue(client, order);
      setSettings(saved);
      setOrder(q.order);
      toast.ok("路由设置已保存，下一发请求就按新的来");
      onSaved?.();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const retest = async (route: Route) => {
    try {
      const was = await stationApi.retest(route.id);
      toast.ok(
        was
          ? `「${label(route)}」转成半开了：下一发真实请求就是回探`
          : `「${label(route)}」没在熔断`,
      );
      onSaved?.();
    } catch (e) {
      toast.error(`重测失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  return (
    <div className="qb-st-form">
      {/* ------------------------------------------------ 故障转移 */}
      <h4 className="qb-st-sect">自动故障转移</h4>
      <label className="qb-st-check">
        <input
          type="checkbox"
          checked={settings.auto_failover}
          onChange={(e) => set({ auto_failover: e.target.checked })}
        />
        当前那条线在交出回复之前失败时，这一发改用下面排的下一条
      </label>
      <p className="qb-st-note">
        连不上、超时、5xx、藏在 200 里的错误、被限流、没有这个模型 ——
        这些会换线；请求本身的问题（400 之类）原样回给客户端，不换线。
        <b>交出去之后再断就只能断开</b>，不会把两条线的回复拼在一起。
        关掉就只打当前那一条。
      </p>
      <NumField
        label="一发最多试几条线（含当前那条）"
        value={settings.max_attempts}
        onChange={(v) => set({ max_attempts: v })}
      />

      <div>
        <span className="qb-st-k">派单顺序（替补）</span>
        <p className="qb-st-note">
          当前那条永远第一个试；勾上的按从上到下的顺序当替补。熔断中的会跳过
          （所有线都熔断着时仍打当前那条）。
        </p>
        {rows.length === 0 ? (
          <p className="qb-st-note">这个软件底下还没有线路。</p>
        ) : (
          <div className="qb-st-tw">
            <table>
              <thead>
                <tr>
                  <th>替补</th>
                  <th>线路</th>
                  <th>熔断</th>
                  <th className="num">顺序</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r) => {
                  const pos = order.indexOf(r.id);
                  const b = breakerOf.get(r.id);
                  return (
                    <tr key={r.id}>
                      <td>
                        <input
                          type="checkbox"
                          aria-label={`把「${label(r)}」排进替补`}
                          checked={pos >= 0}
                          onChange={(e) => toggle(r.id, e.target.checked)}
                        />
                      </td>
                      <td>
                        {label(r)}
                        {r.id === current && (
                          <span className="qb-st-pill qb-st-pill--ok">
                            当前
                          </span>
                        )}
                      </td>
                      <td>
                        {b == null ? (
                          <span className="qb-st-dim">正常</span>
                        ) : (
                          <>
                            <span
                              className={`qb-st-pill ${
                                b.state === "open"
                                  ? "qb-st-pill--danger"
                                  : "qb-st-pill--unknown"
                              }`}
                            >
                              {b.state === "open"
                                ? `熔断中${minutesLeft(b.open_until_ms)}`
                                : "半开 · 下一发回探"}
                            </span>
                            {b.state === "open" && (
                              <button
                                className="qb-st-iconbtn"
                                title="充值了、换了 Key 之后用：马上转半开，下一发真实请求就是回探（不额外花钱）"
                                onClick={() => void retest(r)}
                              >
                                立刻重测
                              </button>
                            )}
                          </>
                        )}
                      </td>
                      <td className="num">
                        {pos >= 0 ? (
                          <span className="qb-st-inline">
                            <button
                              className="qb-st-iconbtn"
                              aria-label="往前挪"
                              disabled={pos === 0}
                              onClick={() => move(r.id, -1)}
                            >
                              ↑
                            </button>
                            <button
                              className="qb-st-iconbtn"
                              aria-label="往后挪"
                              disabled={pos === order.length - 1}
                              onClick={() => move(r.id, 1)}
                            >
                              ↓
                            </button>
                          </span>
                        ) : (
                          <span className="qb-st-dim">—</span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* ------------------------------------------------ 超时 */}
      <h4 className="qb-st-sect">超时（秒，0 = 不限）</h4>
      <div className="qb-st-two">
        <NumField
          label="首包：响应头回来后多久没等到第一块"
          value={settings.first_byte_timeout_secs}
          onChange={(v) => set({ first_byte_timeout_secs: v })}
        />
        <NumField
          label="静默：两块数据之间最长等多久"
          value={settings.idle_timeout_secs}
          onChange={(v) => set({ idle_timeout_secs: v })}
        />
      </div>
      <NumField
        label="非流式：整份响应要在多久之内读完"
        value={settings.non_streaming_timeout_secs}
        onChange={(v) => set({ non_streaming_timeout_secs: v })}
      />
      <p className="qb-st-note">
        首包超时在交出去之前，超了还能换线；静默超时在交出去之后，超了只能断开。
        推理很久才出下一段的长回合会安静好几分钟 ——
        静默超时别设得比客户端自己的还短。
      </p>

      {/* ------------------------------------------------ 熔断 */}
      <h4 className="qb-st-sect">熔断</h4>
      <div className="qb-st-two">
        <NumField
          label="连续几次 5xx / 超时才熔断"
          value={settings.breaker.trip_after_consecutive}
          onChange={(v) => setBreaker({ trip_after_consecutive: v })}
        />
        <NumField
          label="最近 20 次里失败到几成也熔断（%）"
          value={Math.round(settings.breaker.error_rate * 100)}
          onChange={(v) => setBreaker({ error_rate: v / 100 })}
        />
      </div>
      <div className="qb-st-two">
        <NumField
          label="至少几次结果才看失败比例"
          value={settings.breaker.min_requests}
          onChange={(v) => setBreaker({ min_requests: v })}
        />
        <NumField
          label="半开时要成功几次才算修好"
          value={settings.breaker.half_open_successes}
          onChange={(v) => setBreaker({ half_open_successes: v })}
        />
      </div>
      <p className="qb-st-note">
        余额不足、Key
        失效、域名退役这类一次就熔断、关一个小时（充了值点「立刻重测」）；
        被限流只降权、不熔断。
      </p>

      {/* ------------------------------------------------ 整流器 */}
      <h4 className="qb-st-sect">整流器</h4>
      <label className="qb-st-check">
        <input
          type="checkbox"
          checked={settings.rectifier.enabled}
          onChange={(e) => setRect({ enabled: e.target.checked })}
        />
        上游因为请求形状拒绝时，改一下再打同一条线一次（总开关）
      </label>
      <div className="qb-st-tris">
        <Switch
          label="thinking 签名不对（跨站换线后最常见）"
          checked={settings.rectifier.thinking_signature}
          disabled={!settings.rectifier.enabled}
          onChange={(v) => setRect({ thinking_signature: v })}
        />
        <Switch
          label="thinking 预算超限"
          checked={settings.rectifier.thinking_budget}
          disabled={!settings.rectifier.enabled}
          onChange={(v) => setRect({ thinking_budget: v })}
        />
        <Switch
          label="上游不收图片：换成「[Unsupported Image]」标记"
          checked={settings.rectifier.media_fallback}
          disabled={!settings.rectifier.enabled}
          onChange={(v) => setRect({ media_fallback: v })}
        />
        <Switch
          label="确认只认文字的模型：发之前就换掉图片"
          checked={settings.rectifier.media_heuristic}
          disabled={
            !settings.rectifier.enabled || !settings.rectifier.media_fallback
          }
          onChange={(v) => setRect({ media_heuristic: v })}
        />
      </div>

      {error && (
        <div className="qb-st-nudge">
          <span aria-hidden="true">⚠</span>
          <span>{error}</span>
        </div>
      )}
      <div className="qb-st-inline" style={{ justifyContent: "flex-end" }}>
        <Button variant="primary" disabled={busy} onClick={() => void save()}>
          保存
        </Button>
      </div>
    </div>
  );
}

/** 还要关多久，写成「 · 还要 37 分钟」。取不到就不写。 */
function minutesLeft(until: bigint | null): string {
  if (until == null) return "";
  const left = Math.ceil((Number(until) - Date.now()) / 60_000);
  return left > 0 ? ` · 还要 ${left} 分钟` : "";
}

function NumField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number;
  onChange: (v: number) => void;
}) {
  return (
    <label className="qb-st-fieldrow">
      <span className="qb-st-k">{label}</span>
      <input
        value={String(value)}
        inputMode="numeric"
        onChange={(e) => {
          // 只认非负整数；别的字符不进框 —— 存的时候后端再按范围查一遍。
          const digits = e.target.value.replace(/[^0-9]/g, "");
          onChange(digits === "" ? 0 : Number(digits));
        }}
      />
    </label>
  );
}

function Switch({
  label,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="qb-st-check">
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      {label}
    </label>
  );
}
