/**
 * 中转站（P1–P4）的 IPC 包装。
 *
 * 类型**全部**来自 `generated/` —— Rust 侧改一个字段，`npm run types:check`
 * 当场红。这里一个手写的 interface 都没有，理由见 `api.ts` 开头那段。
 *
 * # 错误按 kind 分支，不要匹配中文
 *
 * `call()` 抛的是带 `kind` 的 `IpcError`。文案会改，`kind` 不会。
 */

import { call } from "./ipc";

import type { AuditRound } from "./generated/AuditRound";
import type { DecisionView } from "./generated/DecisionView";
import type { Prefs } from "./generated/Prefs";
import type { RequestLog } from "./generated/RequestLog";
import type { RouteHealthView } from "./generated/RouteHealthView";
import type { Client } from "./generated/Client";
import type { RouterStatus } from "./generated/RouterStatus";
import type { ClientConfig } from "./generated/ClientConfig";
import type { ProbeResult } from "./generated/ProbeResult";
import type { TurnStateModelStatus } from "./generated/TurnStateModelStatus";
import type { Schedule } from "./generated/Schedule";
import type { Session } from "./generated/Session";
import type { Route } from "./generated/Route";
import type { StationModel } from "./generated/StationModel";
import type { StationModelsView } from "./generated/StationModelsView";

export type {
  AuditRound,
  DecisionView,
  Prefs,
  RequestLog,
  Route,
  RouteHealthView,
  RouterStatus,
  StationModel,
  StationModelsView,
};

export const stationApi = {
  /**
   * 启动本机路由。重复调不会换端口。
   *
   * `client` 只影响回来的 `current_route` 显示哪一个软件的上游 ——
   * 三个软件共用同一个监听端口，不是一个软件一个路由。
   */
  routerStart: (client: Client, port?: number) =>
    call<RouterStatus>("station_router_start", { client, port: port ?? null }),
  /** 停掉本机路由。已经在飞的请求会跑完。 */
  routerStop: (client: Client) =>
    call<RouterStatus>("station_router_stop", { client }),
  routerStatus: (client: Client) =>
    call<RouterStatus>("station_router_status", { client }),

  /**
   * 选一条上游。
   *
   * `immediate` 为假时**下一发请求才生效** —— 换上游绝不影响正在飞的那一条。
   *
   * 切的是**哪个软件**的上游，由后端按这条线自己的 `client` 决定，
   * 前端不传 —— 拿界面停在哪个分页当依据的话，只要两者对不上
   * （切完分页还没重渲染、深链接直接打进来），就会把 A 软件的上游
   * 换成一条属于 B 软件的线，账记到 B 头上而且看不出来。
   */
  selectRoute: (routeId: string, immediate: boolean) =>
    call<RouterStatus>("station_select_route", { routeId, immediate }),

  /**
   * 官方 Codex 的 turn-state 注入开关 + 账号规则（实验功能，默认关闭，只对 Codex）。
   * `team` 为真 = Team 规则（12 块 / 332），否则个人（10 块 / 292）。
   */
  turnstateConfigure: (enabled: boolean, team: boolean) =>
    call<void>("station_turnstate_configure", { enabled, team }),
  /** 每个 model 当前的 turn-state 外形（不含值本身）。 */
  turnstateStatus: () =>
    call<TurnStateModelStatus[]>("station_turnstate_status", {}),

  /**
   * 开启识别（实验功能，只对 Codex）：作用于**当前激活的 Codex 账户槽位**。
   *
   * 后端一口气做完：在路由里挂上官方上游（`OAuthPassthrough`，唯一受控例外，仍只绑
   * 127.0.0.1）→ 拉起路由 → 备份并增量改那个槽位的 `config.toml`（`model_provider` 指到
   * 本机路由，`auth.json` 一个字不碰）→ 落盘 marker。不另起 Codex、不复制 OAuth ——
   * 开完到账户页照常启动 Codex（已开着的要重启，新会话才走路由）。
   *
   * ⛔ 别在前端把这几步拆开：中间失败会留下「路由起了、配置没改」的半成品；后端失败时
   * 会把上游摘回去。返回的路由状态里 `turnstate_takeover` 就是被接管的槽位。
   */
  turnstateEnable: () => call<RouterStatus>("station_turnstate_enable", {}),
  /**
   * 关闭识别：把官方上游从本机路由摘掉，并按落盘 marker 把那个槽位的 `config.toml`
   * 反向恢复。不动正在跑的那个 Codex（它接下来会收到 503，界面上提示重启）。
   * 出站插件的互斥守卫指的就是这一步。
   */
  turnstateDisable: () => call<RouterStatus>("station_turnstate_disable", {}),

  /**
   * 启动这个软件，走某一条上游。
   *
   * 后端一口气做三件事：切上游 → 拉起本机路由 → 起客户端进程（用那个
   * 走本机路由的环境，没有就现建）。
   *
   * ⛔ **不要在前端把这三步拆开做。** 拆开的话中间任何一步失败都会留下
   * 一个半成品状态：上游切了、路由起了、客户端没起来 —— 而界面显示
   * 「运行中」，使用者回终端敲 `claude`，那个 claude 走的还是官方。
   */
  launch: (client: Client, routeId?: string, workingDir?: string) =>
    call<Session>("station_launch", {
      client,
      routeId: routeId ?? null,
      workingDir: workingDir ?? null,
    }),

  routes: () => call<Route[]>("station_routes"),
  putRoute: (route: Route, previousId?: string) =>
    call<Route[]>("station_put_route", {
      route,
      previousId: previousId ?? null,
    }),
  removeRoute: (id: string) => call<Route[]>("station_remove_route", { id }),

  /** 落库并回最近若干条。 */
  logs: (limit?: number) =>
    call<RequestLog[]>("station_logs", { limit: limit ?? null }),

  /** 拉一轮账单并聚合。**零成本** —— 读的是站点自己的日志，不打模型。 */
  refreshHealth: () => call<RouteHealthView[]>("station_refresh_health"),

  /**
   * 排一次序。**只排不换** —— 换上游是 `selectRoute` 的事。
   *
   * ⛔ `client` 不能省：整池一起排会排出一条属于别的软件的线，
   * 那条线的 Key 是给别人配的，账记到另一头而每一发请求都成功。
   */
  decide: (prefs: Prefs, client: Client) =>
    call<DecisionView>("station_decide", { prefs, client }),

  /**
   * ⚡ 探测：这家站点支持哪几种协议、公布了哪些模型的价。**免费。**
   *
   * 协议那一步发的是故意不合法的空 body（上游连模型都不碰），
   * 价目那一步读的是站点自己的 /api/pricing。两步都不产生 token。
   *
   * ⛔ 探不出来的那几项回 null，**不是 false** —— Key 打错时三条会
   * 一起 401，猜成「都不支持」会让这条线路在界面上整个消失。
   */
  probe: (baseUrl: string, key?: string | null, group?: string | null) =>
    call<ProbeResult>("station_probe", {
      baseUrl,
      key: key ?? null,
      group: group ?? null,
    }),

  /**
   * 这个软件走本机路由时那份配置文件。
   *
   * ⛔ **一个软件一份，不是一条线路一份。** 换上游不重启客户端 ——
   * 客户端只在启动时读一次配置，六条线轮着走用的是同一份。
   */
  clientConfig: (client: Client) =>
    call<ClientConfig>("station_client_config", { client }),
  /** 存这份配置，存完后端会重新应用一次（把指纹对上、把 base_url 合并回去）。 */
  clientConfigSave: (client: Client, text: string) =>
    call<ClientConfig>("station_client_config_save", { client, text }),

  /** 三个软件各自的调度状态。一次全给。 */
  schedules: () => call<Schedule[]>("station_schedules"),

  /**
   * 开关这个软件的智能调度，顺带存下偏好。
   *
   * ⛔ **这是落盘的**，不是界面上的一个勾。开着就真的有一个循环在按
   * 偏好换上游；面板重启会接着跑。相应地，**面板关掉它就停** ——
   * 界面上必须把这一句说出来。
   *
   * `enabled` 传 `null` = 只存偏好，别动开关。
   */
  scheduleSet: (
    client: Client,
    enabled: boolean | null,
    prefs?: Prefs | null,
  ) =>
    call<Schedule>("station_schedule_set", {
      client,
      enabled,
      prefs: prefs ?? null,
    }),

  /**
   * 这条线路(站点 + 分组)上能用哪些模型。**零成本** —— 读的是站点的价目表。
   *
   * 拉不到时 `models` 是空的、`problem` 写一句话 —— 不抛错。
   * 界面照样得让使用者自己填模型名，不然这种站一个模型都验不了。
   */
  models: (routeId: string) =>
    call<StationModelsView>("station_models", { routeId }),
};

/**
 * 一条线路上有哪些分组可选 —— 「站点 → 分组」那一级。
 *
 * 一条线路就是「站点 + 分组」，所以分组列表是从线路池里推出来的，
 * 不是另拉一个接口。
 */
export function groupsOf(routes: Route[], stationId: string): Route[] {
  return routes
    .filter((r) => r.station_id === stationId)
    .sort((a, b) => a.group.localeCompare(b.group));
}

/** 线路池里出现过的站点，去重后按名字排。 */
export function stationsOf(routes: Route[]): string[] {
  return [...new Set(routes.map((r) => r.station_id))].sort((a, b) =>
    a.localeCompare(b),
  );
}

/**
 * 这个模型那一格怎么写：`×0.15 · 计费翻倍 ×5`。
 *
 * ⛔ **翻倍必须显示出来。** 两家站一个翻倍一个不翻倍时，光看
 * 「×0.15 对 ×0.4」会得出完全相反的结论 —— 输出占七成的活儿上，
 * ×0.15 翻 5 倍那家其实更贵。
 *
 * ⛔ **两套口径分开写。** New API 系公布相对倍率，sub2api 系公布绝对单价
 * （使用者原话：「sub2api 的单价就是官方单价」）。把 `$2.5` 写成 `×2.5`
 * 会让一家五折的站点看起来贵两倍半。哪一套有效看谁填了 ——
 * 跟 Rust 侧 `StationRates::basis` 同一个判定顺序。
 */
export function modelRateLabel(m: StationModel): string {
  const r = m.rates;
  if (r.per_request_price != null && r.per_request_price > 0) {
    return `按次 $${r.per_request_price}`;
  }
  const inp = r.input_price;
  const out = r.output_price;
  if ((inp != null && inp > 0) || (out != null && out > 0)) {
    // 绝对单价那一套。⛔ 缺的一边写 `—`，不拿另一边顶。
    const g = r.group_ratio != null && r.group_ratio > 0 ? r.group_ratio : 1;
    const money = (v: number | null) =>
      v == null ? "—" : `$${Number((v * g).toFixed(4))}`;
    return `每百万 输入 ${money(inp)} · 输出 ${money(out)}`;
  }
  if (r.model_ratio == null) return "倍率未公布";
  const base = `×${r.model_ratio}`;
  const c = r.completion_ratio;
  if (c == null) return `${base} · 输出倍率未公布`;
  if (c > 1) return `${base} · 计费翻倍 ×${c}`;
  return `${base} · 输出不翻倍`;
}

/** 一条线路在界面上叫什么：站点 · 分组。 */
export function routeLabel(r: Route): string {
  return r.group ? `${r.station_id} · ${r.group}` : r.station_id;
}

/** `Route::make_id` 用的单元分隔符。写成 fromCharCode 是因为字面量控制字符
 *  会被编辑器和管线吃掉，而 `""` 写法在几处工具链里各被转义一次。 */
const SEP = String.fromCharCode(31);

/**
 * 只拿得到 id（日志、排序结果）时，这条线在界面上叫什么。
 *
 * ⛔ **全项目只有这一处拆 id。** id 从两段变三段（0.16.0 前面多了软件那一段）
 * 时，各写一份的那两处**都**悄悄变成了「软件 · 站点」—— 不报错、不空白，
 * 只是每一行的名字都不对。分组名恰好是使用者用来认线路的那一段。
 */
export function routeLabelFromId(id: string): string {
  const parts = id.split(SEP);
  // 三段是现在的格式；两段是 0.16.0 之前的老 id（迁移没跑到的场合）。
  const [station, group] =
    parts.length >= 3 ? [parts[1], parts.slice(2).join(SEP)] : parts;
  if (!station) return id;
  return group ? `${station} · ${group}` : station;
}

/**
 * 倍率那一格怎么写。**三态。**
 *
 * 「没检验过」必须独立于「检验过属实」—— 前者是还没有断言，后者是有证据的
 * 断言。合并成一个「看起来没问题」，使用者就分不出「这站我查过」和
 * 「这站我还没查」。判定在 Rust 的 `RateTrust`，这里只负责显示。
 */
export type RateCell =
  | { kind: "unverified"; nominal: number | null }
  | { kind: "verified"; rate: number }
  | { kind: "differs"; nominal: number; real: number; overcharging: boolean };

export function rateCell(r: Route): RateCell {
  const nominal = r.nominal_rate;
  const mult = r.latest_mult;
  if (nominal != null && mult != null && mult > 0 && Number.isFinite(mult)) {
    // 容差跟 Rust 的 RATE_TOLERANCE 对齐（5%）。**只有这一处** ——
    // 两边各写一个数就会漂，界面说「已核实」而排序按「对不上」算。
    if (Math.abs(mult - 1) <= 0.05) return { kind: "verified", rate: nominal };
    return {
      kind: "differs",
      nominal,
      real: nominal * mult,
      overcharging: mult > 1,
    };
  }
  return { kind: "unverified", nominal };
}
