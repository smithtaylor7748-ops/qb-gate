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
import type { DispatchQueue } from "./generated/DispatchQueue";
import type { DispatchParams } from "./generated/DispatchParams";
import type { DispatchView } from "./generated/DispatchView";
import type { ProbeAttempt } from "./generated/ProbeAttempt";
import type { ProbeSettings } from "./generated/ProbeSettings";
import type { ListenScope } from "./generated/ListenScope";
import type { RouterNetworkView } from "./generated/RouterNetworkView";
import type { RouterRequest } from "./generated/RouterRequest";
import type { RouterSettings } from "./generated/RouterSettings";
import type { RouteHealthView } from "./generated/RouteHealthView";
import type { Client } from "./generated/Client";
import type { RouterStatus } from "./generated/RouterStatus";
import type { ClientConfig } from "./generated/ClientConfig";
import type { ProbeResult } from "./generated/ProbeResult";
import type { SiteQuota } from "./generated/SiteQuota";
import type { RelayUsageReport } from "./generated/RelayUsageReport";
import type { RelayRequestPage } from "./generated/RelayRequestPage";
import type { TurnStateModelStatus } from "./generated/TurnStateModelStatus";
import type { Schedule } from "./generated/Schedule";
import type { Session } from "./generated/Session";
import type { Route } from "./generated/Route";
import type { StationModel } from "./generated/StationModel";
import type { StationModelsView } from "./generated/StationModelsView";
import type { UpstreamFormat } from "./generated/UpstreamFormat";
import type { Protocols } from "./generated/Protocols";
import type { AggregateModel } from "./generated/AggregateModel";
import type { AggregateSettings } from "./generated/AggregateSettings";
import type { EndpointTestView } from "./generated/EndpointTestView";
import type { StationEndpoints } from "./generated/StationEndpoints";
import type { CoexistStatus } from "./generated/CoexistStatus";
import type { CoexistTool } from "./generated/CoexistTool";
import type { OnboardingTarget } from "./generated/OnboardingTarget";
import type { EditorLink } from "./generated/EditorLink";
import type { CodexHistoryHome } from "./generated/CodexHistoryHome";
import type { DesktopDirectView } from "./generated/DesktopDirectView";
import type { ProjectChange } from "./generated/ProjectChange";
import type { ProjectsView } from "./generated/ProjectsView";

export type {
  ProjectChange,
  ProjectsView,
  AggregateModel,
  CoexistStatus,
  CoexistTool,
  OnboardingTarget,
  EditorLink,
  CodexHistoryHome,
  DesktopDirectView,
  AggregateSettings,
  EndpointTestView,
  StationEndpoints,
  AuditRound,
  DecisionView,
  DispatchParams,
  DispatchQueue,
  DispatchView,
  ProbeAttempt,
  ProbeSettings,
  ListenScope,
  Prefs,
  RouterNetworkView,
  RouterRequest,
  RouterSettings,
  Route,
  RouteHealthView,
  RouterStatus,
  StationModel,
  StationModelsView,
  UpstreamFormat,
};

export const stationApi = {
  /**
   * 本机路由现在的状态。
   *
   * 路由没有单独的启停按钮：`launch` 起客户端时顺带拉起它，面板退出时它跟着停。
   * 原来的 `station_router_start` / `station_router_stop` 两条命令一直没有界面，2026-09-27 删了。
   */
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
   * 站点卡表头的余额 / 编程套餐额度（中转站重构阶段 6 · U3），用这条线路那把 Key 问。
   *
   * 认不出是哪一家（大部分中转站）就是 `null`，后端一个请求都不发。`force: false`（打开页面）
   * 这把 Key 5 分钟内问过的直接给那一份；`force: true`（点刷新图标）问一次。只显示，调度不读它。
   */
  siteQuota: (routeId: string, force: boolean) =>
    call<SiteQuota | null>("station_site_quota", { routeId, force }),

  /**
   * 中转用量页（中转站重构阶段 6 · U1）：本机路由的请求按软件 / 时间档汇总，加全年热力图。只读本机库，不联网。
   * `client: null` = 五个软件全部；`days` 1 = 今天、0 = 全部。
   */
  usageReport: (client: Client | null, days: number) =>
    call<RelayUsageReport>("station_usage_report", { client, days }),

  /** 中转用量页的逐条日志（新的在前；明细只留 30 天，更早的只剩按天汇总）。 */
  usageRequests: (q: {
    client: Client | null;
    routeId: string | null;
    model: string | null;
    failedOnly: boolean;
    offset: number;
    limit: number;
  }) => call<RelayRequestPage>("station_usage_requests", q),

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
    call<RouterRequest[]>("station_logs", { limit: limit ?? null }),

  /** 这个软件的本机路由设置（故障转移、三种超时、熔断参数、整流器开关）。 */
  routerSettings: (client: Client) =>
    call<RouterSettings>("station_router_settings", { client }),
  /** 存了立刻生效（下一发就按新的来；熔断器的当前状态不动）。不合格的整份不存、报原因。 */
  saveRouterSettings: (settings: RouterSettings) =>
    call<RouterSettings>("station_router_settings_save", { settings }),
  /**
   * 聚合模型此刻发布哪些（阶段 3）：本机路由答模型发现时发的就是这一份。没开聚合是空的。
   * id 由后端编（key 只增不减、桌面端是散列出来的数字），前端别自己拼。
   */
  aggregateModels: (client: Client) =>
    call<AggregateModel[]>("station_aggregate_models", { client }),
  /** 聚合模型写进这个软件的配置时出的问题；没问题是 `null`。 */
  aggregateConfigProblem: (client: Client) =>
    call<string | null>("station_aggregate_config_problem", { client }),
  /**
   * 站点的备用地址（阶段 4，多端点）。主地址连不上时，同一发请求改打这些（同一把 Key），按上次测速从快到慢。
   * 主地址在站点自己身上（`Provider.base_url`），不在这份列表里。
   */
  endpoints: (providerId: string) =>
    call<StationEndpoints>("station_endpoints", { providerId }),
  addEndpoint: (providerId: string, url: string) =>
    call<StationEndpoints>("station_endpoint_add", { providerId, url }),
  removeEndpoint: (providerId: string, url: string) =>
    call<StationEndpoints>("station_endpoint_remove", { providerId, url }),
  /** 换成主地址：站点的地址改成它，原来的主地址挪进备用。账单、检验从此问新地址。 */
  promoteEndpoint: (providerId: string, url: string) =>
    call<StationEndpoints>("station_endpoint_promote", { providerId, url }),
  /** 测速：不带 Key、不花钱，只在点的时候跑；设了出站代理就经它测。 */
  testEndpoints: (providerId: string) =>
    call<EndpointTestView>("station_endpoints_test", { providerId }),
  /**
   * 共存型工具（阶段 4）：OpenCode、MiniMax Code、OpenClaw、Hermes、Pi 里各有没有这条线。只读。
   * ⛔ 这五个不经本机路由，写入写的是**真 Key**（明文进它们自己的配置）。
   */
  coexistStatus: (routeId: string) =>
    call<CoexistStatus[]>("station_coexist_status", { routeId }),
  coexistWrite: (routeId: string, tool: CoexistTool) =>
    call<CoexistStatus[]>("station_coexist_write", { routeId, tool }),
  /** 只删 QB Gate 写的那一条。 */
  coexistRemove: (routeId: string, tool: CoexistTool) =>
    call<CoexistStatus[]>("station_coexist_remove", { routeId, tool }),

  // ---- 阶段 4 · C5 四个小件
  /** 把 Claude Code 的线路复制一份给桌面端（同站点、同分组、同一把 Key）。回新加的那几条。 */
  importDesktopRoutes: () => call<Route[]>("station_import_desktop_routes"),
  /** 中转环境和各个官方目录的 Claude Code 跳没跳首次确认。 */
  onboardingStatus: () => call<OnboardingTarget[]>("claude_onboarding_status"),
  /** 官方那几份只有点名到才改；撤销只撤面板自己设的。 */
  setOnboarding: (target: string, skip: boolean) =>
    call<OnboardingTarget[]>("claude_onboarding_set", { target, skip }),
  /** 各个编辑器里的 Claude Code 插件联动了没有。 */
  vscodeStatus: () => call<EditorLink[]>("claude_vscode_status"),
  /** 让插件走中转环境：编辑器设置里加两条变量（只有占位 Key）、关掉登录提示。 */
  vscodeLink: (editor: string) =>
    call<EditorLink[]>("claude_vscode_link", { editor }),
  vscodeUnlink: (editor: string) =>
    call<EditorLink[]>("claude_vscode_unlink", { editor }),
  /** 各个 Codex 目录里的会话分桶。 */
  codexHistory: () => call<CodexHistoryHome[]>("codex_history_status"),
  /** ⛔ 先备份再改；`from` 是使用者逐个勾的桶。 */
  codexHistoryMerge: (home: string, from: string[]) =>
    call<CodexHistoryHome[]>("codex_history_merge", { home, from }),
  codexHistoryRestore: (home: string, backup: string) =>
    call<CodexHistoryHome[]>("codex_history_restore", { home, backup }),
  /** 桌面端直连档：现在经本机路由还是直连哪条线，每条桌面端线路能不能直连。 */
  desktopDirect: () => call<DesktopDirectView>("station_desktop_direct"),
  /** ⛔ 给线路 = 直连（桌面端配置里写那条线的真 Key）；`null` = 改回经本机路由。重开桌面端才生效。 */
  setDesktopDirect: (routeId: string | null) =>
    call<DesktopDirectView>("station_desktop_direct_set", { routeId }),
  /** 派单队列：上次选中的那条 + 手动排的顺序（当前那条失败时依次改用谁）。 */
  dispatchQueue: (client: Client) =>
    call<DispatchQueue>("station_dispatch_queue", { client }),
  /** 只收这个软件自己的线；混进别的软件的线整份拒绝。 */
  saveDispatchQueue: (client: Client, order: string[]) =>
    call<DispatchQueue>("station_dispatch_queue_save", { client, order }),

  // 项目（阶段 5，cc-switch 的 M4）：只换中转那一侧（线路、派单顺序、中转环境里的 MCP / Skills / 提示词），
  // ⛔ 不碰官方账户和槽位。项目列表各软件共用，「当前项目」每个软件各记各的。
  /** 项目列表和每个软件的当前项目。 */
  projects: () => call<ProjectsView>("projects_list"),
  /** 把这个软件现在的样子存成一个项目，并记成它的当前项目。 */
  projectCreate: (name: string, client: Client) =>
    call<ProjectsView>("projects_create", { name, client }),
  projectRename: (id: string, name: string) =>
    call<ProjectsView>("projects_rename", { id, name }),
  /** 只删记录，不动任何配置。 */
  projectDelete: (id: string) => call<ProjectsView>("projects_delete", { id }),
  /** 换到这个项目（只这个软件那一组）：现在的样子先存回原来那个项目。尽力而为，没落成的回在 `warnings` 里。 */
  projectApply: (id: string, client: Client) =>
    call<ProjectChange>("projects_apply", { id, client }),
  /** 不使用项目：先存回当前项目，再清掉这个软件的当前项目，什么配置都不动。 */
  projectClear: (client: Client) =>
    call<ProjectChange>("projects_clear", { client }),
  /**
   * 「我充值了，立刻重测」：熔断中的线马上转半开，下一发真实请求就是回探 —— 不额外花钱。
   * 回它原来是不是熔断着。
   */
  retest: (routeId: string) =>
    call<boolean>("station_route_retest", { routeId }),

  /** 本机路由的网络设置（三个软件共用）：监听范围、路由口令、出站代理（不带用户名密码）。 */
  routerNetwork: () => call<RouterNetworkView>("station_router_network"),
  /**
   * 改监听范围。开到所有网卡上时没有口令就先生成一个；路由起着就按新的重启一次（在飞的请求照样跑完）。
   * Windows 防火墙会弹窗问一次 —— 界面上提前写着。
   */
  saveRouterListen: (scope: ListenScope) =>
    call<RouterNetworkView>("station_router_listen_save", { scope }),
  /** 换一个新的路由口令。旧的立刻失效。 */
  rotateRouterKey: () => call<RouterNetworkView>("station_router_key_rotate"),
  /** 设 / 清中转线路的出站代理（`null` = 直连）。官方 Codex 那条线永远直连。 */
  saveOutboundProxy: (url: string | null) =>
    call<RouterNetworkView>("station_outbound_proxy_save", { url }),
  /** 经出站代理试连这个软件的第一条线（只在点「测试」时）。回耗时毫秒。 */
  testOutboundProxy: (client: Client, url: string | null) =>
    call<number>("station_outbound_proxy_test", { client, url }),
  /** 扫本机常见代理端口（只连 127.0.0.1、不发数据）。 */
  scanOutboundProxies: () => call<string[]>("station_outbound_proxy_scan"),

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
    dispatch?: DispatchParams | null,
  ) =>
    call<Schedule>("station_schedule_set", {
      client,
      enabled,
      prefs: prefs ?? null,
      dispatch: dispatch ?? null,
    }),

  /**
   * 派单队列与测活（调度设置弹窗那张表）：每一行的档位、死活、速度、失败率、折合价、
   * 上次 / 下次测活、24 小时流量，告警、动作日志、今天测活花了多少。**全是读库**，不联网。
   */
  dispatchView: (client: Client) =>
    call<DispatchView>("station_dispatch_view", { client }),
  /**
   * 「立刻测」：不看到没到点。免费的一律测；花钱的只在打开了花钱测活、今天预算还够时测。
   * `routeId` 不给 = 这个软件的每一条。
   */
  probeNow: (client: Client, routeId?: string | null) =>
    call<ProbeAttempt[]>("station_probe_now", {
      client,
      routeId: routeId ?? null,
    }),
  /**
   * 钉住一条线：下一发请求换过去，4 小时内调度不换走它（它挂了就不算）。
   * `routeId` 传 `null` = 取消钉住、交回调度。
   */
  pin: (client: Client, routeId: string | null) =>
    call<DispatchQueue>("station_pin", { client, routeId }),
  /** 测活设置（三个软件共用）：花钱测活开关（默认关）、每日预算、间隔。不合格整份不存。 */
  saveProbeSettings: (settings: ProbeSettings) =>
    call<ProbeSettings>("station_probe_settings_save", { settings }),

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
 * New API 的倍率为 1 时每百万 token 多少美元（站内额度）。
 *
 * 跟 Rust 侧 `pricing::NEW_API_USD_PER_MTOK` 是同一个数，出处是 New API 源码
 * `1 === $0.002 / 1K tokens`，它自己的定价页也按 `model_ratio * 2 * 分组倍率` 算单价。
 * ⛔ New API 的 `model_ratio` 是单价，不是「官方的几倍」：照官方价抄的 Opus 5 是 2.5。
 */
export const NEW_API_USD_PER_MTOK = 2;

/**
 * 这个模型那一格怎么写：`每百万 输入 $1 · 输出 $5`。
 *
 * ⛔ **两套口径都换成单价写。** sub2api 系公布的就是单价（使用者原话：「sub2api
 * 的单价就是官方单价」）；New API 系公布的倍率也是单价，单位是 $2 / 百万
 * （使用者原话：「New API 的倍率算在了单价内，所以单价便宜」）。0.25.3 及以前
 * 把 New API 的倍率写成 `×2.5 · 计费翻倍 ×5` —— 照官方价收费的 Opus 5 看起来
 * 贵两倍半、输出还翻五倍，而 5 只是官方自己的「输出 ÷ 输入」。
 *
 * 已乘站点公布的分组倍率（知道的话），跟站点自己定价页上显示的是同一个数。
 * 哪一套有效看谁填了 —— 跟 Rust 侧 `StationRates::basis` 同一个判定顺序。
 */
export function modelRateLabel(m: StationModel): string {
  const r = m.rates;
  if (r.per_request_price != null && r.per_request_price > 0) {
    return `按次 $${r.per_request_price}`;
  }
  const g = r.group_ratio != null && r.group_ratio > 0 ? r.group_ratio : 1;
  // ⛔ 缺的一边写 `—`，不拿另一边顶。
  const money = (v: number | null) =>
    v == null ? "—" : `$${Number((v * g).toFixed(4))}`;
  const inp = r.input_price;
  const out = r.output_price;
  if ((inp != null && inp > 0) || (out != null && out > 0)) {
    return `每百万 输入 ${money(inp)} · 输出 ${money(out)}`;
  }
  if (r.model_ratio == null || !(r.model_ratio > 0)) return "价格未公布";
  const input = r.model_ratio * NEW_API_USD_PER_MTOK;
  const c = r.completion_ratio;
  return `每百万 输入 ${money(input)} · 输出 ${money(c == null ? null : input * c)}`;
}

/**
 * 这家站 1 美元站内额度付几元（充值比例）。没填、填坏了都按 1 —— 跟 Rust 侧
 * `Provider::topup_per_usd()` 同一个规则。
 *
 * 中转站的倍率按站内额度算；不同站放在一起比，要先乘上各自的充值比例，
 * 折成「每 $1 官方牌价付几元」（linux.do「中转站百科」帖说的「倍率陷阱」）。
 */
export function topupOf(p?: { topup_per_usd: number | null } | null): number {
  const v = p?.topup_per_usd;
  return v != null && Number.isFinite(v) && v > 0 ? v : 1;
}

/**
 * 输出另外加价到什么程度才在线路上挂一枚「输出加价」。
 *
 * `route.output_markup` 是「站点输出 ÷ 输入」比官方的「输出 ÷ 输入」多出来的倍数，
 * 检验时由后端拿官方价算好（`StationRates::output_markup`）。5% 以内当舍入，
 * 跟 `route::RATE_TOLERANCE` 同一个宽度。
 */
export const OUTPUT_MARKUP_TOLERANCE = 0.05;

/**
 * 线路「上游协议」那一格的写法（中转站重构阶段 3：跟软件自己说的不一样时，本机路由两头转换）。
 */
export const UPSTREAM_FORMAT_LABEL: Record<UpstreamFormat, string> = {
  anthropic: "Anthropic Messages",
  openai_chat: "OpenAI Chat",
  openai_responses: "OpenAI Responses",
  gemini_native: "Gemini 原生",
};

/** 说 Responses 的那几个软件（Codex，阶段 4 起还有 Grok Build）：转换的几对跟 Codex 一样。 */
function speaksResponses(client: Client): boolean {
  return client === "codex" || client === "grok-build";
}

/** 这个软件自己说的协议 —— 线路没写协议时就是它，原样转发。跟 Rust 侧 `local_router::native_format` 一致。 */
export function nativeFormat(client: Client): UpstreamFormat {
  if (speaksResponses(client)) return "openai_responses";
  if (client === "gemini-cli") return "gemini_native";
  return "anthropic";
}

/**
 * 这个软件能转换到哪几种上游协议（cc-switch 搬来的那几对）。Codex / Grok Build 没有转 Gemini 的那一对 ——
 * 后端选到这一对会直接跳过这条线（`convert::plan::select` 的 `Unsupported`）。
 * Gemini CLI（阶段 4）一对都没有：只能原样转发给 Gemini 原生上游（cc-switch 的 Gemini 路由同样只透传）。
 */
export function convertibleFormats(client: Client): UpstreamFormat[] {
  if (speaksResponses(client)) return ["openai_chat", "anthropic"];
  if (client === "gemini-cli") return [];
  return ["openai_responses", "openai_chat", "gemini_native"];
}

/** 这个软件能不能用聚合模型（聚合 id 的编法只有 Claude Code、桌面端、Codex 三种，见 `qb_proxy::aggregate`）。 */
export function aggregateSupported(client: Client): boolean {
  return (
    client === "claude-code" ||
    client === "claude-desktop" ||
    client === "codex"
  );
}

/** 这条线要不要本机路由转换协议：写了协议、而且跟软件自己说的不一样。不转回 `null`。 */
export function convertedFormat(route: Route): UpstreamFormat | null {
  const f = route.upstream_format;
  return f != null && f !== nativeFormat(route.client) ? f : null;
}

/**
 * 探完协议之后「上游协议」建议填什么。能直通（或者没探出来）就不建议（`null`，原样转发）；
 * 直通**明确**不行、别的协议探通了，建议转换成那个 —— Claude 先 Responses 再 Chat，Codex 先 Chat 再 Anthropic。
 * 探不出来的一律不猜。跟 Rust 侧 `Protocols::native` 同一套判定（Codex 直通只认 Responses）。
 */
export function suggestUpstreamFormat(
  client: Client,
  p: Protocols,
): UpstreamFormat | null {
  // Gemini 原生不在探测的三种里，也没有可以转换的那几对：不建议。
  if (client === "gemini-cli") return null;
  const native = speaksResponses(client) ? p.openai_responses : p.anthropic;
  if (native !== false) return null;
  const probed: Partial<Record<UpstreamFormat, boolean | null>> = {
    anthropic: p.anthropic,
    openai_chat: p.openai_chat,
    openai_responses: p.openai_responses,
  };
  return convertibleFormats(client).find((f) => probed[f] === true) ?? null;
}

/**
 * 聚合模型「这条线发布哪些模型」那一格：逗号、顿号、换行都算分隔，去空白、去重。空 = 用这条线模型映射里的目标模型。
 * 后端存之前还会再理一遍（`station_ops::settle_aggregate`），这里只是让输入框里的样子跟存下的一致。
 */
export function parseModelList(text: string): string[] {
  const out: string[] = [];
  for (const raw of text.split(/[,，、\n]/)) {
    const m = raw.trim();
    if (m && !out.includes(m)) out.push(m);
  }
  return out;
}

/** 聚合模型怎么在客户端里选（阶段 3 还不写客户端的配置，所以要人手选或手填 id）。 */
export function aggregateHowTo(client: Client): string {
  switch (client) {
    case "codex":
      return "Codex：存了之后写进中转环境的模型目录（Codex 自带的模型原样留着，聚合的接在后面），重开 Codex 后 /model 里就有；要本机装着 Codex 命令行。也能 -m <id> 手填。";
    case "claude-desktop":
      return "Claude 桌面端：存了之后写进它的模型条目（inferenceModels，你自己写的条目不动），重开桌面端后选择器里就有。";
    default:
      return "Claude Code：存了之后写进 settings.json 的 modelPicker（你自己写的行不动），/model 里就列出来，要 2.1.243 以上；也能 /model <id> 手填。";
  }
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
