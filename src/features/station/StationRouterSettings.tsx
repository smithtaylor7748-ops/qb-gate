/**
 * 「选线与分流」弹窗（每个软件一份，2026-11-15 第三版样机）。
 *
 * # 第一屏只有三样
 *
 * 1. **备用线路**：当前线路出故障（熔断、上游限流、额度用光）时依次改用谁。勾选 + 往前挪；
 *    每条标出此刻会不会被跳过、为什么（`selection.plan`），比当前线路贵的也标出来（`pricier_backups`）。
 *    一条都不勾 = 只用当前线路，失败直接报错。
 * 2. **自动选线偏好**：省钱 / 速度 / 稳定三选一（默认省钱，跟后端 `Prefs::saving()` 一样）。
 *    开关只有主页右列那一个（D1）。
 * 3. **下一次请求实际会按什么顺序试**（F3）：照后端算好的 `selection.plan` 写，不自己再算。
 *
 * 其余全部收进「高级设置」折叠区，带默认值，可以永远不打开：一次最多试几条、超时、熔断、整流器、
 * 限流与容量分流、换首选的防抖与底线、聚合模型、Codex 会话修复、检测与排名。
 *
 * # ⛔ 只有一个「保存」（F2）
 *
 * 原来备用名单要点保存、自动选线偏好改一项存一项、防抖又是另一套自动保存 —— 来回切会把刚改的冲掉。
 * 现在整个弹窗改的都是草稿，底部一个「保存」一起落盘。
 *
 * ⛔ **旧草稿不能盖掉新状态**：保存之前把库里现在那份再读一遍，跟打开弹窗时读到的不一样
 * （别的窗口存过、自动选线开启时把当前线路放进了名单……）就不存，读回最新的、让人再改一次。
 *
 * # ⛔ 读的是库里那一份
 *
 * 本机路由用的是存进库、又装进路由器的那份。只改本地 state 的话，界面上的备用名单跟路由真用的漂开，而且看不出来。
 *
 * # 删掉的
 *
 * - 「仅所选线路 / 允许所勾选线路」单选框：名单空就是只用当前线路，不需要第二个开关（存储字段 `routing_mode`
 *   由 {@link withBackups} 跟着名单设对）；
 * - 前端「当前线路必须在名单里」那句校验：后端早就不要求（名单是「当前线路出故障时依次改用谁」）；
 * - 弹窗里重复的范围卡与「最近请求去向」（D4）、「用这一条」（B2）、页脚那个重复的自动选线开关（D1）；
 * - 网络、出站代理、测活频率与预算挪到「设置 → 中转站」（D8，五个软件共用）。
 */

import { useEffect, useMemo, useRef, useState } from "react";

import type { Client } from "../../lib/generated/Client";
import type { DispatchParams } from "../../lib/generated/DispatchParams";
import type { RouterStatus } from "../../lib/generated/RouterStatus";
import type { Schedule } from "../../lib/generated/Schedule";
import {
  aggregateHowTo,
  aggregateSupported,
  convertedFormat,
  parseModelList,
  stationApi,
  withBackups,
  type AggregateModel,
  type Prefs,
  type Route,
  type RouterSettings,
} from "../../lib/station";
import { Button, useToast } from "../../ui";
import {
  admissionSettingsProblem,
  StationAdmissionSettings,
} from "./StationAdmissionSettings";
import { StationSessionRecoverySettings } from "./StationSessionRecoverySettings";
import StationDispatch from "./StationDispatch";
import {
  FailureRules,
  PRESETS,
  PrefsEditor,
  presetOf,
  RankingPreview,
} from "./StationSchedule";
import {
  planText,
  retryable,
  SKIP_LABEL,
  SKIP_SHORT,
  untilText,
} from "./selection";

// 明确点过保存的操作即使弹窗关闭也会完成；新弹窗先等旧写入，再读落盘值。
const routerWriteQueue = { current: Promise.resolve() as Promise<void> };

/** 比较两份设置是否一样（`Prefs` 里有 bigint，`JSON.stringify` 直接会抛）。 */
function same(a: unknown, b: unknown): boolean {
  const s = (v: unknown) =>
    JSON.stringify(v, (_k, x) => (typeof x === "bigint" ? `${x}n` : x));
  return s(a) === s(b);
}

type Loaded = {
  settings: RouterSettings;
  prefs: Prefs;
  dispatch: DispatchParams;
};

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
  /** 存好之后回调（主页的状态要读回来）。 */
  onSaved?: () => void;
}) {
  const toast = useToast();
  const pageRevision = useRef(0);
  const clientRef = useRef(client);
  clientRef.current = client;
  /** 打开弹窗时（或上一次保存后）库里的那一份 —— 保存前拿它跟库里现在的比，防止旧草稿盖掉新状态。 */
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [settings, setSettings] = useState<RouterSettings | null>(null);
  const [prefs, setPrefs] = useState<Prefs | null>(null);
  const [dispatch, setDispatch] = useState<DispatchParams | null>(null);
  const [error, setError] = useState<string | null>(null);
  /**
   * 上一次保存因为「打开之后被别处改过」被拒了。⛔ 单独一份 state：拒了之后要重读最新的，
   * 而重读开头会清 `error` —— 放在 `error` 里的话这句话刚出现就被清掉。下一次存成功才清。
   */
  const [stale, setStale] = useState(false);
  const [busy, setBusy] = useState(false);
  const [reload, setReload] = useState(0);
  // 聚合模型每条线「发布哪些模型」的输入框原文：边打边理会把刚打的逗号吃掉，存的时候才拆。
  const [modelText, setModelText] = useState<Record<string, string>>({});
  const [published, setPublished] = useState<AggregateModel[]>([]);
  /** 聚合模型写进这个软件配置时出的问题（阶段 4）。 */
  const [configProblem, setConfigProblem] = useState<string | null>(null);
  /** 「高级设置」里「检测与排名」那一块点开了才挂（排名要现拉账单，好几秒）。 */
  const [inspect, setInspect] = useState(false);

  useEffect(() => {
    let dropped = false;
    setLoaded(null);
    setSettings(null);
    setError(null);
    setBusy(false);
    void routerWriteQueue.current
      .then(() =>
        Promise.all([
          stationApi.routerSettings(client),
          stationApi.schedules(),
          stationApi.aggregateModels(client),
          aggregateSupported(client)
            ? stationApi.aggregateConfigProblem(client)
            : Promise.resolve(null),
        ]),
      )
      .then(([s, all, models, problem]) => {
        if (dropped) return;
        const mine = all.find((x) => x.client === client);
        if (!mine) throw new Error("没读到这个软件的自动选线设置");
        setConfigProblem(problem);
        setLoaded({ settings: s, prefs: mine.prefs, dispatch: mine.dispatch });
        setSettings(s);
        setPrefs(mine.prefs);
        setDispatch(mine.dispatch);
        setModelText(textOf(s.aggregate.models));
        setPublished(models);
      })
      .catch((e: unknown) => {
        if (!dropped) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      dropped = true;
      pageRevision.current++;
    };
  }, [client, reload]);

  const selection = status?.selection ?? null;
  const current = selection?.current ?? null;
  const autoSelect = selection?.auto_select ?? false;
  const order = useMemo(() => settings?.allowed_routes ?? [], [settings]);
  /** 名单里除了当前线路本身之外的那几条，按排的顺序。 */
  const backupIds = order.filter((id) => id !== current);
  const stepOf = useMemo(
    () => new Map((selection?.plan.steps ?? []).map((s) => [s.route_id, s])),
    [selection],
  );
  const pricier = new Set(selection?.pricier_backups ?? []);
  // 当前线路排最前，勾上的照名单顺序，没勾的跟在后面（照线路原来的顺序）。
  const rows = useMemo(() => {
    const byId = new Map(routes.map((r) => [r.id, r]));
    const head = current ? routes.filter((r) => r.id === current) : [];
    const queued = order
      .filter((id) => id !== current)
      .map((id) => byId.get(id))
      .filter((r): r is Route => r != null);
    const rest = routes.filter(
      (r) => r.id !== current && !order.includes(r.id),
    );
    return [...head, ...queued, ...rest];
  }, [routes, order, current]);
  const labelOf = (id: string) => {
    const r = routes.find((x) => x.id === id);
    return r ? label(r) : id;
  };

  if (error && !settings)
    return (
      <div className="qb-st-nudge">
        读不到选线设置：{error}
        <Button variant="ghost" onClick={() => setReload((v) => v + 1)}>
          重试
        </Button>
      </div>
    );
  if (!settings || !prefs || !dispatch || !loaded)
    return <p className="qb-st-note">读取中…</p>;

  const dirty =
    !same(settings, loaded.settings) ||
    !same(prefs, loaded.prefs) ||
    !same(dispatch, loaded.dispatch) ||
    !same(textOf(settings.aggregate.models), modelText);

  const set = (patch: Partial<RouterSettings>) =>
    setSettings({ ...settings, ...patch });
  const setBreaker = (patch: Partial<RouterSettings["breaker"]>) =>
    setSettings({ ...settings, breaker: { ...settings.breaker, ...patch } });
  const setRect = (patch: Partial<RouterSettings["rectifier"]>) =>
    setSettings({
      ...settings,
      rectifier: { ...settings.rectifier, ...patch },
    });
  /** 改名单一律经 withBackups：存储字段 routing_mode 跟着名单设对（非空 = pool，空 = selected）。 */
  const setOrder = (next: string[]) => setSettings(withBackups(settings, next));

  const move = (id: string, by: -1 | 1) => {
    // 只在「备用线路」之间挪；当前线路在不在名单里、在哪，原样留着。
    const i = backupIds.indexOf(id);
    const j = i + by;
    if (i < 0 || j < 0 || j >= backupIds.length) return;
    const nextBackups = [...backupIds];
    [nextBackups[i], nextBackups[j]] = [nextBackups[j], nextBackups[i]];
    let k = 0;
    setOrder(order.map((x) => (x === current ? x : nextBackups[k++])));
  };
  const toggle = (id: string, on: boolean) =>
    setOrder(
      on
        ? [...order.filter((x) => x !== id), id]
        : order.filter((x) => x !== id),
    );

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
    if (busy) return;
    const revision = pageRevision.current;
    const currentPage = () =>
      revision === pageRevision.current && clientRef.current === client;
    const capacityProblem = admissionSettingsProblem(settings);
    if (capacityProblem) {
      setError(capacityProblem);
      return;
    }
    setBusy(true);
    setError(null);
    const write = routerWriteQueue.current.then(async () => {
      // ⛔ 先看库里那份在这期间有没有被别处改过：改过就不存，免得拿旧草稿盖掉新状态。
      const [fresh, all] = await Promise.all([
        stationApi.routerSettings(client),
        stationApi.schedules(),
      ]);
      const freshSchedule: Schedule | undefined = all.find(
        (x) => x.client === client,
      );
      if (
        !same(fresh, loaded.settings) ||
        !freshSchedule ||
        !same(freshSchedule.prefs, loaded.prefs) ||
        !same(freshSchedule.dispatch, loaded.dispatch)
      )
        return { stale: true as const };
      const models: Record<string, string[]> = {};
      for (const id of agg.members) {
        const list = parseModelList(modelText[id] ?? "");
        if (list.length > 0) models[id] = list;
      }
      const saved = await stationApi.saveRouterSettings(
        withBackups({ ...settings, aggregate: { ...agg, models } }, order),
      );
      let schedule = freshSchedule;
      if (!same(prefs, loaded.prefs) || !same(dispatch, loaded.dispatch))
        schedule = await stationApi.scheduleSet(client, null, prefs, dispatch);
      return { stale: false as const, saved, schedule };
    });
    routerWriteQueue.current = write.then(
      () => {},
      () => {},
    );
    try {
      const done = await write;
      if (!currentPage()) return;
      if (done.stale) {
        setStale(true);
        setReload((v) => v + 1);
        return;
      }
      setStale(false);
      const { saved, schedule } = done;
      setLoaded({
        settings: saved,
        prefs: schedule.prefs,
        dispatch: schedule.dispatch,
      });
      setSettings(saved);
      setPrefs(schedule.prefs);
      setDispatch(schedule.dispatch);
      setModelText(textOf(saved.aggregate.models));
      toast.ok("选线设置已保存，下一次请求就按新的来");
      onSaved?.();
      try {
        const models = await stationApi.aggregateModels(client);
        if (!currentPage()) return;
        setPublished(models);
        if (aggregateSupported(client)) {
          const problem = await stationApi.aggregateConfigProblem(client);
          if (currentPage()) setConfigProblem(problem);
        }
      } catch (e) {
        if (!currentPage()) return;
        setError(
          `选线设置已保存，但聚合模型列表刷新失败：${e instanceof Error ? e.message : String(e)}`,
        );
      }
    } catch (e) {
      if (currentPage()) setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (currentPage()) setBusy(false);
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
      toast.error(`重试失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const preset = presetOf(prefs);

  return (
    <div className="qb-st-form">
      <fieldset disabled={busy} className="qb-st-settings-fields">
        {/* ------------------------------------------------ 第一屏 · 备用线路 */}
        <h4 className="qb-st-sect">备用线路</h4>
        <p className="qb-st-note">
          当前线路出故障（熔断、上游限流、额度用光）时，依次改用下面勾上的线路；当前线路恢复后，下一次请求自动回到它。
          一条都不勾 =
          只用当前线路，失败直接报错、不换站。新加的线路不会自动加进来。
          {current ? "" : " 还没有当前线路：先在主页的线路行上点「设为当前」。"}
        </p>
        {autoSelect && (
          <p className="qb-st-note">
            自动选线开着：它在当前线路和这些备用线路里挑一条当当前线路，排出来的顺序不改这份名单；要手排顺序先在主页把自动选线关掉。
          </p>
        )}
        {rows.length === 0 ? (
          <p className="qb-st-note">这个软件底下还没有线路。</p>
        ) : (
          <div className="qb-st-tw">
            <table>
              <thead>
                <tr>
                  <th>备用</th>
                  <th>线路</th>
                  <th>此刻</th>
                  <th className="num">顺序</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r) => {
                  const isCurrent = r.id === current;
                  const pos = backupIds.indexOf(r.id);
                  const step = stepOf.get(r.id);
                  const savedIn = loaded.settings.allowed_routes.includes(r.id);
                  return (
                    <tr key={r.id} className={isCurrent ? "qb-st-cur" : ""}>
                      <td>
                        {isCurrent ? (
                          <span className="qb-st-dim">—</span>
                        ) : (
                          <input
                            type="checkbox"
                            aria-label={`把「${label(r)}」加为备用线路`}
                            checked={pos >= 0}
                            disabled={!r.credential_id}
                            onChange={(e) => toggle(r.id, e.target.checked)}
                          />
                        )}
                      </td>
                      <td>
                        {label(r)}
                        {isCurrent && (
                          <span className="qb-st-pill qb-st-pill--accent">
                            当前线路 · 不用勾，永远可用
                          </span>
                        )}
                        {!isCurrent && pos >= 0 && (
                          <span className="qb-st-pill qb-st-pill--unknown">
                            第 {pos + 1} 备用
                          </span>
                        )}
                        {pricier.has(r.id) && pos >= 0 && (
                          <span
                            className="qb-st-pill qb-st-pill--warn"
                            title="按折过充值比例的真实倍率比，这条比当前线路贵：当前线路出故障改用它时花得更多"
                          >
                            比当前线路贵
                          </span>
                        )}
                        {!r.credential_id && (
                          <span className="qb-st-dim"> · 未配置 Key</span>
                        )}
                      </td>
                      <td>
                        {step?.skip ? (
                          <>
                            <span
                              className={`qb-st-pill ${
                                step.skip === "over-limit"
                                  ? "qb-st-pill--unknown"
                                  : "qb-st-pill--danger"
                              }`}
                              title={SKIP_LABEL[step.skip]}
                            >
                              {SKIP_SHORT[step.skip]}，跳过
                              {untilText(step.until_ms)
                                ? ` · ${untilText(step.until_ms)}`
                                : ""}
                            </span>
                            {retryable(step.skip) && (
                              <button
                                type="button"
                                className="qb-st-iconbtn qb-st-iconbtn--text"
                                title="充值了、换了 Key 之后用：马上转半开，下一发真实请求就是回探（不额外花钱）"
                                onClick={() => void retest(r)}
                              >
                                重试这条
                              </button>
                            )}
                          </>
                        ) : step ? (
                          <span className="qb-st-dim">会试</span>
                        ) : (
                          <span className="qb-st-dim">
                            {savedIn || isCurrent ? "—" : "不在名单，不会试"}
                          </span>
                        )}
                        {step && step.missing_models.length > 0 && (
                          <span className="qb-st-dim">
                            {" "}
                            · 缺 {step.missing_models.join("、")}
                          </span>
                        )}
                      </td>
                      <td className="num">
                        {!isCurrent && pos >= 0 ? (
                          <span className="qb-st-inline">
                            <button
                              type="button"
                              className="qb-st-iconbtn"
                              aria-label={`把「${label(r)}」往前挪`}
                              disabled={autoSelect || pos === 0}
                              onClick={() => move(r.id, -1)}
                            >
                              ↑
                            </button>
                            <button
                              type="button"
                              className="qb-st-iconbtn"
                              aria-label={`把「${label(r)}」往后挪`}
                              disabled={
                                autoSelect || pos === backupIds.length - 1
                              }
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

        {/* ------------------------------------------------ 第一屏 · 自动选线偏好 */}
        <h4 className="qb-st-sect">自动选线偏好</h4>
        <p className="qb-st-note">
          只在自动选线开着时用（开关在主页右列）：它在当前线路和备用线路里按这一档挑一条当当前线路，只换当前线路，备用名单不动。
          面板关掉它就停。
        </p>
        <div
          className="qb-st-routing-options qb-st-presets3"
          role="radiogroup"
          aria-label="自动选线偏好"
        >
          {PRESETS.map((p) => (
            <label
              key={p.id}
              className={`qb-st-routing-option${preset === p.id ? " active" : ""}`}
            >
              <input
                type="radio"
                name={`auto-pref-${client}`}
                checked={preset === p.id}
                onChange={() => setPrefs(p.prefs)}
              />
              <span>
                <strong>{p.name}</strong>
                <span>{p.desc}</span>
              </span>
            </label>
          ))}
        </div>
        {preset == null && (
          <p className="qb-st-note">
            现在是自定义的一套（在「高级设置 →
            换当前线路的防抖与底线」里改过）；点上面任意一档会换成那一档。
          </p>
        )}

        {/* ------------------------------------------------ 第一屏 · 下一次请求 */}
        <h4 className="qb-st-sect">下一次请求</h4>
        <p className="qb-st-plan" aria-label="下一次请求实际会按什么顺序试">
          {planText(selection?.plan, labelOf)}
        </p>
        {dirty && (
          <p className="qb-st-note">
            这一行按已保存的设置算；改了上面的名单，保存之后才按新的来。
          </p>
        )}

        {/* ------------------------------------------------ 高级设置 */}
        <details className="qb-st-adv qb-st-advanced">
          <summary>高级设置 · 一般不用改</summary>
          <p className="qb-st-note">
            下面每一项都有默认值。改了跟第一屏一样，点底部「保存」才生效。
          </p>

          <h4 className="qb-st-sect">一次请求最多试几条</h4>
          <NumField
            label="含当前线路那一条；每条都可能已经开始计费（默认 2）"
            value={settings.max_attempts}
            onChange={(v) => set({ max_attempts: v })}
          />

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
            首包超时在交出去之前，有备用线路时可改用备用；静默超时在交出去之后，超了只能断开。
            推理很久才出下一段的长回合会安静好几分钟 ——
            静默超时别设得比客户端自己的还短。
          </p>

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
            失效、域名退役这类一次就熔断、关一个小时（充了值点「重试这条」）。
            只有当前线路、没有备用线路时，熔断不拦发送。
          </p>
          <FailureRules />

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
                !settings.rectifier.enabled ||
                !settings.rectifier.media_fallback
              }
              onChange={(v) => setRect({ media_heuristic: v })}
            />
          </div>

          <StationAdmissionSettings
            settings={settings}
            routes={rows}
            label={label}
            onChange={set}
          />

          <h4 className="qb-st-sect">换当前线路的防抖与底线</h4>
          <p className="qb-st-note">
            只对自动选线有用。挑战者要同时满足这三条才把当前线路换下来；当前线路熔断了、判死了、没过底线，有合格的就立刻换，不谈防抖。
            换线会让上游那边的提示缓存作废，所以默认要领先 8%、连续 2 轮。
          </p>
          <div className="qb-st-floors">
            <div className="qb-st-floor">
              <span />
              <label>
                领先当前线路至少{" "}
                <input
                  className="qb-st-num"
                  type="number"
                  step={1}
                  min={0}
                  max={100}
                  aria-label="领先当前线路至少多少（%）"
                  value={Math.round(dispatch.margin * 100)}
                  onChange={(e) =>
                    setDispatch({
                      ...dispatch,
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
                  value={dispatch.min_samples}
                  onChange={(e) =>
                    setDispatch({
                      ...dispatch,
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
                  value={dispatch.confirm}
                  onChange={(e) =>
                    setDispatch({
                      ...dispatch,
                      confirm: Math.max(1, Number(e.target.value)),
                    })
                  }
                />{" "}
                轮有新证据都领先
              </label>
            </div>
          </div>
          <PrefsEditor prefs={prefs} onChange={setPrefs} />

          {/* ------------------------------------------------ 聚合模型（阶段 3）：只有 Claude Code、桌面端、Codex 有 */}
          {aggregateSupported(client) && (
            <>
              <h4 className="qb-st-sect">聚合模型</h4>
              <label className="qb-st-check">
                <input
                  type="checkbox"
                  checked={agg.enabled}
                  onChange={(e) => setAgg({ enabled: e.target.checked })}
                />
                把勾上的几条线的模型一起列给这个软件：客户端选哪个模型，就请求对应线路；只能是当前线路或备用线路，别的线路会拒绝访问
              </label>
              <p className="qb-st-note">
                模型 id 带 <code>qbg</code>{" "}
                前缀，跟普通模型名分得开；不带前缀的照旧走当前线路。
                选中聚合模型的那一发只打那一条线：不看当前线路、不换线、不记熔断，钱花在那条线上（主页「最近请求实际去向」会提示）。
                那条线移出备用线路或删掉之后，带着旧 id
                的请求直接报错，不会悄悄改用别的线。
                {aggregateHowTo(client)}
              </p>
              {routes.length > 0 && (
                <div className="qb-st-tw">
                  <table>
                    <thead>
                      <tr>
                        <th>聚合</th>
                        <th>线路</th>
                        <th>
                          发布哪些模型（逗号分隔；空 =
                          用这条线「线路模型映射」的目标模型）
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
                                onChange={(e) =>
                                  toggleMember(r.id, e.target.checked)
                                }
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
                                  setModelText({
                                    ...modelText,
                                    [r.id]: e.target.value,
                                  })
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
                  {configProblem && (
                    <p className="qb-st-note" role="alert">
                      没写进配置：{configProblem}
                    </p>
                  )}
                  {published.length === 0 ? (
                    <p className="qb-st-note">
                      还没有：勾上线、保存。没配 Key
                      的线、没写模型又没配「线路模型映射」的线发布不了模型。
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
                                  type="button"
                                  className="qb-st-iconbtn qb-st-iconbtn--text"
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
            </>
          )}

          {client === "codex" && (
            <StationSessionRecoverySettings
              settings={settings}
              onChange={set}
            />
          )}

          <details
            className="qb-st-adv"
            onToggle={(e) =>
              setInspect((e.target as HTMLDetailsElement).open || inspect)
            }
          >
            <summary>检测与排名 · 手动重新检测、每条线的读数</summary>
            {inspect && (
              <>
                <StationDispatch client={client} />
                <RankingPreview client={client} prefs={prefs} />
              </>
            )}
          </details>
        </details>
      </fieldset>

      {/* ------------------------------------------------ 唯一的保存 */}
      <div className="qb-st-savebar">
        {stale && (
          <div className="qb-st-nudge" role="alert">
            <span aria-hidden="true">⚠</span>
            <span>
              这份设置在你打开弹窗之后被别处改过（比如另一个窗口存过、开了自动选线），刚才那次没有保存。上面已经是读回的最新设置，请再改一次。
            </span>
          </div>
        )}
        {error && (
          <div className="qb-st-nudge" role="alert">
            <span aria-hidden="true">⚠</span>
            <span>{error}</span>
          </div>
        )}
        <span className="qb-st-note">
          {dirty
            ? "有没保存的改动。"
            : "已保存的就是正在用的。面板关掉自动选线就停。"}
        </span>
        <span className="qb-st-inline">
          <Button
            variant="ghost"
            disabled={busy || !dirty}
            onClick={() => setReload((v) => v + 1)}
          >
            撤销改动
          </Button>
          <Button
            variant="primary"
            disabled={busy || !dirty}
            onClick={() => void save()}
          >
            保存
          </Button>
        </span>
      </div>
    </div>
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

/** 这条线「线路模型映射」里的目标模型（没写发布列表时发布的就是这些），拿来当输入框的提示。 */
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
