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
 *
 * # 聚合模型（阶段 3，cc-switch 的 Stack）
 *
 * 跟上面几节一起存。模型 id 由后端编（key 只增不减、桌面端是散列出来的数字），这里只显示后端读回来的那份，
 * 不在前端自己拼 —— 拼出来的跟路由真认的对不上，使用者照着填就是一发 400。
 */

import { useEffect, useMemo, useState } from "react";

import type { Client } from "../../lib/generated/Client";
import type { RouterStatus } from "../../lib/generated/RouterStatus";
import {
  aggregateHowTo,
  convertedFormat,
  parseModelList,
  stationApi,
  type AggregateModel,
  type Route,
  type RouterNetworkView,
  type RouterSettings,
} from "../../lib/station";
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
  // 调度开着时替补顺序归调度排（阶段 2）：这里只读，保存时也不碰它 —— 后端会拒收手排的顺序。
  const [scheduled, setScheduled] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // 聚合模型每条线「发布哪些模型」的输入框原文：边打边理会把刚打的逗号吃掉，存的时候才拆。
  const [modelText, setModelText] = useState<Record<string, string>>({});
  const [published, setPublished] = useState<AggregateModel[]>([]);

  useEffect(() => {
    let dropped = false;
    setSettings(null);
    setError(null);
    void Promise.all([
      stationApi.routerSettings(client),
      stationApi.dispatchQueue(client),
      stationApi.schedules(),
      stationApi.aggregateModels(client),
    ])
      .then(([s, q, all, models]) => {
        if (dropped) return;
        setSettings(s);
        setOrder(q.order);
        setScheduled(all.some((x) => x.client === client && x.enabled));
        setModelText(textOf(s.aggregate.models));
        setPublished(models);
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

  const agg = settings.aggregate;
  const setAgg = (patch: Partial<RouterSettings["aggregate"]>) =>
    setSettings({ ...settings, aggregate: { ...agg, ...patch } });
  const toggleMember = (id: string, on: boolean) =>
    setAgg({
      members: on
        ? [...agg.members.filter((x) => x !== id), id]
        : agg.members.filter((x) => x !== id),
    });

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.ok("模型 id 已复制");
    } catch {
      toast.error("复制不了，请手动选中复制");
    }
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const models: Record<string, string[]> = {};
      for (const id of agg.members) {
        const list = parseModelList(modelText[id] ?? "");
        if (list.length > 0) models[id] = list;
      }
      const saved = await stationApi.saveRouterSettings({
        ...settings,
        aggregate: { ...agg, models },
      });
      setSettings(saved);
      setModelText({ ...modelText, ...textOf(saved.aggregate.models) });
      setPublished(await stationApi.aggregateModels(client));
      if (!scheduled) {
        const q = await stationApi.saveDispatchQueue(client, order);
        setOrder(q.order);
      }
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
        {scheduled && (
          <p className="qb-st-note">
            智能调度开着：替补顺序由调度按档位排（在「智能调度」里看），这里只读。
            要自己排先停调度，或者在调度里钉住一条。
          </p>
        )}
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
                          disabled={scheduled}
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
                              disabled={scheduled || pos === 0}
                              onClick={() => move(r.id, -1)}
                            >
                              ↑
                            </button>
                            <button
                              className="qb-st-iconbtn"
                              aria-label="往后挪"
                              disabled={scheduled || pos === order.length - 1}
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

      {/* ------------------------------------------------ 聚合模型（阶段 3） */}
      <h4 className="qb-st-sect">聚合模型</h4>
      <label className="qb-st-check">
        <input
          type="checkbox"
          checked={agg.enabled}
          onChange={(e) => setAgg({ enabled: e.target.checked })}
        />
        把勾上的几条线的模型一起列给这个软件：选哪个模型，那一发就直达那条线
      </label>
      <p className="qb-st-note">
        模型 id 带 <code>qbg</code>{" "}
        前缀，跟普通模型名分得开；不带前缀的照旧按派单走。
        <b>选中的那一发只打那一条线：不换线、不记熔断。</b>
        那条线移出名单或删掉之后，带着旧 id 的请求直接报错，不会悄悄改用别的线
        （那等于拿别家的钱、别家的模型回答）。{aggregateHowTo(client)}
      </p>
      {routes.length > 0 && (
        <div className="qb-st-tw">
          <table>
            <thead>
              <tr>
                <th>聚合</th>
                <th>线路</th>
                <th>
                  发布哪些模型（逗号分隔；空 = 用这条线模型映射的目标模型）
                </th>
              </tr>
            </thead>
            <tbody>
              {routes.map((r) => {
                const on = agg.members.includes(r.id);
                return (
                  <tr key={r.id}>
                    <td>
                      <input
                        type="checkbox"
                        aria-label={`把「${label(r)}」放进聚合`}
                        checked={on}
                        onChange={(e) => toggleMember(r.id, e.target.checked)}
                      />
                    </td>
                    <td>
                      {label(r)}
                      {convertedFormat(r) && (
                        <span className="qb-st-pill qb-st-pill--accent">
                          转换
                        </span>
                      )}
                    </td>
                    <td>
                      <input
                        value={modelText[r.id] ?? ""}
                        disabled={!on}
                        aria-label={`「${label(r)}」发布的模型`}
                        placeholder={
                          mappedTargets(r) || "例如 glm-5[1M], glm-4.7"
                        }
                        onChange={(e) =>
                          setModelText({ ...modelText, [r.id]: e.target.value })
                        }
                      />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {agg.enabled && (
        <div>
          <span className="qb-st-k">现在发布的（保存后刷新）</span>
          {published.length === 0 ? (
            <p className="qb-st-note">
              还没有：勾上线、保存。没配 Key
              的线、没写模型又没配模型映射的线发布不了模型。
            </p>
          ) : (
            <div className="qb-st-tw">
              <table>
                <thead>
                  <tr>
                    <th>模型 id</th>
                    <th>显示名</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {published.map((m) => (
                    <tr key={m.id}>
                      <td>
                        <code>{m.id}</code>
                      </td>
                      <td className="qb-st-dim">{m.display_name}</td>
                      <td>
                        <button
                          className="qb-st-iconbtn"
                          onClick={() => void copy(m.id)}
                        >
                          复制
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

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

      <RouterNetworkPanel client={client} />
    </div>
  );
}

// ------------------------------------------------------------------ 网络（三个软件共用）

/**
 * 本机路由监听在哪、中转线路走不走出站代理（R6）。这一节**三个软件共用**，各自即时保存 ——
 * 不跟上面那个「保存」绑在一起：改监听会重启路由，不该因为顺手改了一个超时就重启一次。
 *
 * ⛔ 代理地址框**不回填**存着的那个：后端给的是去掉了用户名密码的写法，回填之后一保存，
 * 密码就被冲掉了。框里只放新填的；存着的那个写在框上面。
 */
function RouterNetworkPanel({ client }: { client: Client }) {
  const toast = useToast();
  const [view, setView] = useState<RouterNetworkView | null>(null);
  const [proxy, setProxy] = useState("");
  const [found, setFound] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let dropped = false;
    void stationApi
      .routerNetwork()
      .then((v) => !dropped && setView(v))
      .catch((e: unknown) => {
        if (!dropped) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      dropped = true;
    };
  }, []);

  const run = async (what: () => Promise<RouterNetworkView>, ok: string) => {
    setBusy(true);
    setError(null);
    try {
      setView(await what());
      toast.ok(ok);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const test = async () => {
    setBusy(true);
    setError(null);
    try {
      const ms = await stationApi.testOutboundProxy(
        client,
        proxy.trim() === "" ? null : proxy.trim(),
      );
      toast.ok(`代理通了：经它连到这个软件的第一条线用了 ${ms} 毫秒`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const scan = async () => {
    setBusy(true);
    try {
      setFound(await stationApi.scanOutboundProxies());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const copyKey = async (key: string) => {
    try {
      await navigator.clipboard.writeText(key);
      toast.ok("路由口令已复制");
    } catch {
      toast.error("复制不了，请手动选中复制");
    }
  };

  // 点了就先翻过去（存要重启路由，等它回来再翻的话，点下去那一下看起来像没点上）；
  // 存失败就翻回原来那一档，并把原因写出来。
  const switchScope = async (scope: RouterNetworkView["scope"], ok: string) => {
    if (!view || view.scope === scope) return;
    const before = view;
    setView({ ...view, scope });
    setBusy(true);
    setError(null);
    try {
      setView(await stationApi.saveRouterListen(scope));
      toast.ok(ok);
    } catch (e) {
      setView(before);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!view)
    return error ? (
      <div className="qb-st-nudge">读不到本机路由的网络设置：{error}</div>
    ) : null;

  const open = view.scope === "all-interfaces";
  return (
    <>
      <h4 className="qb-st-sect">网络 · 三个软件共用</h4>
      <div role="radiogroup" aria-label="本机路由监听在哪">
        <label className="qb-st-check">
          <input
            type="radio"
            name="qb-router-scope"
            checked={!open}
            disabled={busy}
            onChange={() =>
              void switchScope("loopback", "本机路由改成只本机能连")
            }
          />
          只本机（127.0.0.1）—— 默认
        </label>
        <label className="qb-st-check">
          <input
            type="radio"
            name="qb-router-scope"
            checked={open}
            disabled={busy}
            onChange={() =>
              void switchScope(
                "all-interfaces",
                "本机路由开到了所有网卡上：别的机器要带路由口令",
              )
            }
          />
          所有网卡：WSL、局域网里别的机器也能连
        </label>
      </div>
      {open && (
        <>
          <p className="qb-st-nudge">
            <span aria-hidden="true">⚠</span>
            <span>
              别的机器连 <code>这台电脑的 IP:{view.port}</code>，API Key
              填下面这个路由口令；这台电脑上的客户端不用改。
              <b>拿到口令的人就能用你的线路额度</b> ——
              只在你信得过的网络里开。Windows
              防火墙会弹窗问一次，要点「允许」。官方 Codex
              那条线不给别的机器用。
            </span>
          </p>
          <label className="qb-st-fieldrow">
            <span className="qb-st-k">路由口令</span>
            <span className="qb-st-inline">
              <input readOnly value={view.key ?? ""} />
              <button
                className="qb-st-iconbtn"
                onClick={() => void copyKey(view.key ?? "")}
              >
                复制
              </button>
              <button
                className="qb-st-iconbtn"
                title="旧口令立刻失效：别的机器上的客户端要改成新的"
                disabled={busy}
                onClick={() =>
                  void run(
                    () => stationApi.rotateRouterKey(),
                    "换了一个新口令，旧的已经失效",
                  )
                }
              >
                换一个
              </button>
            </span>
          </label>
        </>
      )}

      <div>
        <span className="qb-st-k">出站代理（只给中转线路）</span>
        <p className="qb-st-note">
          中转线路的请求经这个代理出去（HTTP / SOCKS5）。官方 Codex
          那条线永远直连 —— 走代理的话，门禁核过的出口就不是它真走的出口。
          现在：<b>{view.outbound_proxy ?? "直连"}</b>
        </p>
        <span className="qb-st-inline">
          <input
            value={proxy}
            aria-label="出站代理地址"
            placeholder="http://127.0.0.1:7890 或 socks5://127.0.0.1:1080"
            onChange={(e) => setProxy(e.target.value)}
          />
          <button
            className="qb-st-iconbtn"
            disabled={busy}
            title="只连本机 127.0.0.1 的几个常见端口，不发任何数据"
            onClick={() => void scan()}
          >
            扫描本机
          </button>
        </span>
        {found != null && (
          <div className="qb-st-presets">
            {found.length === 0 ? (
              <span className="qb-st-note">本机常见端口上没有代理在听。</span>
            ) : (
              found.map((u) => (
                <button key={u} className="chip" onClick={() => setProxy(u)}>
                  {u}
                </button>
              ))
            )}
          </div>
        )}
        <div className="qb-st-inline" style={{ justifyContent: "flex-end" }}>
          <Button
            variant="ghost"
            disabled={busy || (proxy.trim() === "" && !view.outbound_proxy)}
            onClick={() => void test()}
          >
            测试
          </Button>
          <Button
            variant="ghost"
            disabled={busy || !view.outbound_proxy}
            onClick={() =>
              void run(
                () => stationApi.saveOutboundProxy(null),
                "出站代理清掉了：中转线路直连",
              )
            }
          >
            清掉，直连
          </Button>
          <Button
            variant="primary"
            disabled={busy || proxy.trim() === ""}
            onClick={() =>
              void run(async () => {
                const v = await stationApi.saveOutboundProxy(proxy.trim());
                setProxy("");
                return v;
              }, "出站代理已保存：下一发中转请求就经它走")
            }
          >
            保存代理
          </Button>
        </div>
      </div>
      {error && (
        <div className="qb-st-nudge">
          <span aria-hidden="true">⚠</span>
          <span>{error}</span>
        </div>
      )}
    </>
  );
}

/** 存着的「每条线发布哪些模型」换成输入框里的写法。 */
function textOf(
  models: RouterSettings["aggregate"]["models"],
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(models).map(([id, list]) => [id, (list ?? []).join(", ")]),
  );
}

/** 这条线模型映射里的目标模型（没写发布列表时发布的就是这些），拿来当输入框的提示。 */
function mappedTargets(route: Route): string {
  // 老数据（和演示夹具）里可能没有这一格 —— 跟 `modelMap.ts` 一样当它没写。
  const m = route.model_map as Route["model_map"] | undefined;
  if (!m) return "";
  const out: string[] = [];
  for (const t of [
    ...m.exact.map((p) => p.to),
    m.default,
    m.opus,
    m.sonnet,
    m.haiku,
    m.fable,
    m.subagent,
  ]) {
    const v = t?.trim();
    if (v && !out.includes(v)) out.push(v);
  }
  return out.join(", ");
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
