/**
 * 中转站。**没有子菜单栏。**
 *
 * 原来的四个分页（中转站账户 / 智能调度 / 站点检验 / 请求日志）全部取消：
 * 调度进了右列按钮 + 设置弹窗，站点检验并进每条账户行，请求日志一行一个 ▤。
 * 分页把「这条线现在怎么样」拆成了四页，而使用者要决定的是同一件事 ——
 * 换哪条上游。四页之间来回跳等于让人自己在脑子里做联表查询。
 *
 * 页面自上而下只有四块：软件分段器 → 总览卡 + 右列 →「最近请求实际去向」一行 → 站点卡与线路行。
 *
 * # 选线只有三个概念（2026-11-14 R1，2026-11-15 界面跟上）
 *
 * 当前线路（行上「设为当前」，手动换线只有这一个入口）、备用线路（「选线与分流」里勾）、自动选线（右列那一个开关）。
 * 全页只用这几个词加「备用地址」（站点的备用 URL），不再有「智能调度 / 派单 / 允许名单 / 钉住」。
 * 线路行**整行不可点**：原来点一下整行就切线、关自动选线、把多条线收回成一条（A1）。
 *
 * # 一条线路 = 软件 + 站点 + 分组
 *
 * **线路按软件独立，站点按域名共享。** 同一个站点的同一个分组在 Claude Code
 * 和 Codex 底下是两行，各绑各的 key；而余额是站点级的，整站共享，
 * 所以画在站点卡的表头上而不是每一行各写一遍。
 *
 * ⛔ 软件这一维**不能用 `protocols` 推**：Claude Code 与桌面端走的是同一套
 * Anthropic 协议，协议上分不开。分得开它们的是 `Route.client`。
 *
 * # 颜色
 *
 * ⛔ 只引 `tokens.css` 的变量，不写 hex。草图（V24）用的是合并前的旧配色。
 */

import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  Check,
  ChevronRight,
  Download,
  Fingerprint,
  History,
  Plug,
  Play,
  Shuffle,
  Square,
  Wrench,
  X,
  Zap,
} from "lucide-react";

import { Link } from "react-router-dom";

import type { Axis } from "../../lib/generated/Axis";
import type { Client } from "../../lib/generated/Client";
import type { Floors } from "../../lib/generated/Floors";
import type { Provider } from "../../lib/generated/Provider";
import type { DispatchParams } from "../../lib/generated/DispatchParams";
import type { Schedule } from "../../lib/generated/Schedule";
import { StationUsageKpiValues } from "./StationUsageKpis";
import {
  StationUsageChart,
  useStationUsageOverview,
  type ChartMetric,
} from "./StationUsageChart";
import { RecentRoute } from "./RecentRoute";
import { InlineConfirm } from "./InlineConfirm";
import { backupSummary, backupsOf } from "./selection";
import {
  convertedFormat,
  OUTPUT_MARKUP_TOLERANCE,
  rateCell,
  stationApi,
  topupOf,
  UPSTREAM_FORMAT_LABEL,
  type Prefs,
  type RouterRequest,
  type Route,
  type RouteHealthView,
  type RouterStatus,
} from "../../lib/station";
import { useSession } from "../../lib/store";
import { useWorkspace, workspaceApi } from "../../lib/workspace";
import { Button, Modal, useToast } from "../../ui";
import { DeepLinkPasteButton } from "../DeepLinkImport";
import {
  dash,
  pct,
  RANGES,
  rangeOf,
  stamp,
  STATION_CLIENTS,
  type GranId,
  type RangeId,
} from "./shared";
import StationRouterSettings from "./StationRouterSettings";
import StationAudit from "./StationAudit";
import StationLogs from "./StationLogs";
import StationQualityCheck from "./StationQualityCheck";
import StationClaudeCodeExtras from "./StationClaudeCodeExtras";
import StationCodexHistory from "./StationCodexHistory";
import StationDesktopDirect from "./StationDesktopDirect";
import StationSiteQuota from "./StationSiteQuota";
import RouteDialog from "./RouteDialog";

/** 三个维度在界面上叫什么。跟调度设置弹窗里那份是同一套写法。 */
const AXIS_LABEL: Record<Axis, string> = {
  cheap: "便宜",
  fast: "快",
  stable: "稳",
};

/** 口径在卡片上的顺序。固定成弹窗里复选框的顺序，不跟着勾选先后变。 */
const AXIS_ORDER: Axis[] = ["cheap", "fast", "stable"];

/**
 * 当前口径，一句话。
 *
 * ⛔ 一项都没勾是合法的（「随便，别乱换」），不写成「未设置」——
 * 那会让人以为调度没配好，而实际上现任会一直留着。
 */
function axesText(axes: Axis[]): string {
  const on = AXIS_ORDER.filter((a) => axes.includes(a));
  return on.length
    ? `按 ${on.map((a) => AXIS_LABEL[a]).join(" + ")} 挑`
    : "一项都没勾 · 现任一直留着";
}

/** 设了的底线，一句话。一条都没设就回空串，不另占一行写「无底线」。 */
function floorsText(floors: Floors): string {
  const out: string[] = [];
  if (floors.min_success_rate != null)
    out.push(`成功率≥${num(floors.min_success_rate * 100)}%`);
  // 生成的类型是 bigint，演示夹具给的是 number —— Number() 两种都接得住。
  if (floors.max_ttft_p95_ms != null)
    out.push(`首字≤${num(Number(floors.max_ttft_p95_ms) / 1000)}s`);
  if (floors.max_rate != null) out.push(`倍率≤×${num(floors.max_rate)}`);
  return out.join(" · ");
}

/** 3 → "3"，2.5 → "2.5"。0.95 × 100 的浮点尾巴（95.00000000000001）不上界面。 */
function num(v: number): string {
  return String(Number(v.toFixed(2)));
}

const CLIENTS = STATION_CLIENTS;

export default function StationCenter() {
  const [routes, setRoutes] = useState<Route[]>([]);
  const toast = useToast();

  const reload = useCallback(async () => {
    try {
      setRoutes(await stationApi.routes());
    } catch (e) {
      toast.error(`读不到线路：${e instanceof Error ? e.message : String(e)}`);
    }
  }, []);

  // 别处加了线路（导入链接，阶段 5）会把这个数加一：停在这一页时也要重读线路池。
  const [routesBump] = useSession("station.routes.bump", 0);
  useEffect(() => {
    void reload();
  }, [reload, routesBump]);

  return (
    <section style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <StationBoard routes={routes} onChanged={reload} />
    </section>
  );
}

// ------------------------------------------------------------------ 主体

function StationBoard({
  routes,
  onChanged,
}: {
  routes: Route[];
  onChanged: () => void;
}) {
  // 停在哪个软件分页记在会话态里：导入链接导进一条线路之后，跳过来就停在那个软件上（阶段 5）。
  const [client, setClient] = useSession<Client>(
    "station.client",
    "claude-code",
  );
  const [range, setRange] = useState<RangeId>("24h");
  const [gran, setGran] = useState<GranId>("hour");
  /** 柱状图画哪一个量：请求数 / Token / 价格（2026-11-15）。记在会话态里，切软件不丢。 */
  const [metric, setMetric] = useSession<ChartMetric>(
    "station.chart.metric",
    "requests",
  );
  const usageGran =
    range === "all"
      ? "day"
      : range === "7d" && gran === "minute"
        ? "hour"
        : gran;
  const usage = useStationUsageOverview(client, range, usageGran);

  const [status, setStatus] = useState<RouterStatus | null>(null);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [latestRequest, setLatestRequest] = useState<RouterRequest | null>(
    null,
  );
  const [health, setHealth] = useState<Record<string, RouteHealthView>>({});
  const [logs, setLogs] = useState<RouterRequest[]>([]);
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const [stopping, setStopping] = useState<Client | null>(null);
  const [launchError, setLaunchError] = useState<{
    client: Client;
    text: string;
  } | null>(null);

  // 调度**是后端驻留的**，不是这里的一个 useState：开着就真的有一个循环
  // 每分钟排一次序、该换就换，面板重启也会接着跑（`station_schedules`）。
  //
  // ⛔ 这里只反映它的状态，不自己维护一份。自己维护的话，刷新一次页面
  // 界面显示「没开」而循环还在换上游 —— 使用者会以为上游是自己变的。
  const [sched, setSched] = useState(false);
  /** 驻留循环上一轮的结论，一句话。空 = 还没跑过。 */
  const [schedNote, setSchedNote] = useState<string>("");
  /**
   * 调度开着时每条线的名次。key 是线路 id。
   *
   * ⛔ 拿的是**驻留循环用的那套偏好**排出来的名次，不是这里另起一套。
   * 另起一套的话，行上写着「最弱项 .47」而循环按另一个数在换 ——
   * 两份判定必然漂移，而漂了之后没有任何地方看得出来。
   */
  const [schedRows, setSchedRows] = useState<
    Record<string, { weakest: Axis | null; score: number | null }>
  >({});
  /** 存着的自动选线偏好。右列按钮照着它写「按什么挑」。`null` = 还没读到。 */
  const [schedPrefs, setSchedPrefs] = useState<Prefs | null>(null);
  /** 存着的换首选防抖（G5：右列写真实的数，不再写死「领先 8% 连续 2 轮」）。 */
  const [schedDispatch, setSchedDispatch] = useState<DispatchParams | null>(
    null,
  );
  /**
   * 正在启动哪个软件。**按软件记，不是一个布尔**：启动要好几秒，
   * 中途切到别的分页的话，那一页的启动块不该跟着显示「正在启动」。
   */
  const [launching, setLaunching] = useState<Client | null>(null);
  /** 启动刚结束的两秒多：成功打勾、失败抖一下。按软件记，理由同上。 */
  const [launchFx, setLaunchFx] = useState<{
    client: Client;
    fx: "done" | "failed";
  } | null>(null);
  /** 调度开关正在落盘。 */
  const [toggling, setToggling] = useState(false);
  /** 调度刚被点亮的那一下，外圈泛一道光。 */
  const [ignite, setIgnite] = useState(false);
  const fxTimer = useRef<number | undefined>(undefined);
  const igniteTimer = useRef<number | undefined>(undefined);
  useEffect(
    () => () => {
      window.clearTimeout(fxTimer.current);
      window.clearTimeout(igniteTimer.current);
    },
    [],
  );
  const schedDescId = useId();
  const launchRipple = useRipple();
  const schedRipple = useRipple();
  /** 异步回来时判断「使用者还停在这个分页吗」。闭包里的 client 是点下去那一刻的。 */
  const clientRef = useRef(client);
  clientRef.current = client;
  const runtimeRead = useRef(0);

  const [routeDialog, setRouteDialog] = useState<{
    editing: Route | null;
  } | null>(null);
  const [routerOpen, setRouterOpen] = useState(false);
  const [turnOpen, setTurnOpen] = useState(false);
  /** 阶段 4 · C5：Claude Code 的两个小件、Codex 会话归桶。 */
  const [extrasOpen, setExtrasOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [importing, setImporting] = useState(false);
  /** 桌面端直连档（阶段 4，写真 Key）：直连的那条线 id；`null` = 经本机路由（或还没读到）。 */
  const [directOpen, setDirectOpen] = useState(false);
  const [directRoute, setDirectRoute] = useState<string | null>(null);
  useEffect(() => {
    if (client !== "claude-desktop") return;
    let dropped = false;
    void stationApi
      .desktopDirect()
      .then((v) => !dropped && setDirectRoute(v.route_id))
      .catch(() => undefined);
    return () => {
      dropped = true;
    };
  }, [client, routes]);
  const [logOpen, setLogOpen] = useState<Route | null>(null);
  const [auditOpen, setAuditOpen] = useState<Route | null>(null);
  const [auditBusy, setAuditBusy] = useState(false);

  const workspace = useWorkspace();
  const runningSessions = (workspace.data?.sessions ?? []).filter(
    (session) =>
      session.state === "running" &&
      session.context.client === client &&
      session.context.identity_kind === "relay" &&
      session.context.identity_id === `qb-router-${client}`,
  );
  const isRunning = runningSessions.length > 0;
  const setupNotice = runningSessions.find(
    (session) =>
      session.detail.includes("Windows") && session.detail.includes("Codex"),
  )?.detail;
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible")
        void workspace.refresh().catch(() => undefined);
    }, 2000);
    return () => window.clearInterval(timer);
  }, [workspace.refresh]);
  const closeNow = async () => {
    if (launching || stopping) return;
    setStopping(client);
    setLaunchError(null);
    try {
      for (const session of runningSessions)
        await workspaceApi.stop(session.id);
    } catch (e) {
      toast.error(`关闭失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      await workspace.refresh().catch(() => undefined);
      setStopping(null);
    }
  };
  const providers: Provider[] = useMemo(
    () => workspace.data?.providers ?? [],
    [workspace.data],
  );
  const siteName = useCallback(
    (id: string) => providers.find((p) => p.id === id)?.name ?? id,
    [providers],
  );
  const siteUrl = useCallback(
    (id: string) => providers.find((p) => p.id === id)?.base_url ?? "",
    [providers],
  );

  const refreshStatus = useCallback(async () => {
    const read = ++runtimeRead.current;
    try {
      // 备用线路、自动选线开没开、下一发顺序都在 `status.selection` 里（R1），不用另读路由设置。
      const [nextStatus, requests] = await Promise.all([
        stationApi.routerStatus(client),
        stationApi.usageRequests({
          client,
          routeId: null,
          model: null,
          failedOnly: false,
          offset: 0,
          limit: 1,
        }),
      ]);
      if (clientRef.current !== client || read !== runtimeRead.current) return;
      setStatus(nextStatus);
      setLatestRequest(requests.items[0] ?? null);
      setRuntimeError(null);
    } catch (e) {
      if (clientRef.current === client && read === runtimeRead.current)
        setRuntimeError(
          `无法核对当前路由状态：${e instanceof Error ? e.message : String(e)}`,
        );
    }
  }, [client]);

  // 换软件要重新问一次 —— 三个软件各有各的当前上游。不重问的话
  // 「正在路由」那一行会一直显示上一个软件的线，而那条线在这一页上根本不存在。
  useEffect(() => {
    setStatus(null);
    setLatestRequest(null);
    setRuntimeError(null);
    void refreshStatus();
  }, [refreshStatus]);

  /** 把后端回来的一份调度状态落到界面上。只动 state，立刻返回。 */
  const showSchedule = useCallback((s: Schedule | undefined) => {
    setSched(Boolean(s?.enabled));
    setSchedNote(s?.last_note ?? "");
    setSchedPrefs(s?.prefs ?? null);
    setSchedDispatch(s?.dispatch ?? null);
    if (!s?.enabled) setSchedRows({});
  }, []);

  /**
   * 调度开着时，把每条线的名次排出来。
   *
   * ⛔ `decide` 会给每条线现拉一轮站点账单，要好几秒 —— 所以跟开关分开：
   * 开关先翻过去，名次随后到；也**不放进定时器**。
   */
  const loadRows = useCallback(
    async (s: Schedule | undefined) => {
      if (!s?.enabled) return;
      try {
        const d = await stationApi.decide(s.prefs, client);
        if (clientRef.current !== client) return;
        setSchedRows(
          Object.fromEntries(
            d.ranking.rows.map((r) => [
              r.route_id,
              { weakest: r.weakest, score: r.score },
            ]),
          ),
        );
      } catch {
        // 排不出名次不等于调度没开 —— 行上不写名次，开关照实显示开着。
        if (clientRef.current === client) setSchedRows({});
      }
    },
    [client],
  );

  const refreshSched = useCallback(async () => {
    let mine: Schedule | undefined;
    try {
      mine = (await stationApi.schedules()).find((s) => s.client === client);
    } catch (e) {
      if (clientRef.current === client)
        setSchedNote(
          `自动选线状态读取失败：${e instanceof Error ? e.message : String(e)}`,
        );
      return;
    }
    if (clientRef.current !== client) return;
    showSchedule(mine);
    await loadRows(mine);
  }, [client, loadRows, showSchedule]);

  useEffect(() => {
    void refreshSched();
  }, [refreshSched]);

  // 每 5 秒核对本机调度、选线范围和真实请求记录。这里只读本机状态，
  // 不调用会访问上游的 decide，也不改变驻留调度的周期。
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (document.visibilityState !== "visible") return;
      void stationApi
        .schedules()
        .then((all) => {
          if (clientRef.current !== client) return;
          const s = all.find((x) => x.client === client);
          if (!s) return;
          setSched(s.enabled);
          setSchedNote(s.last_note);
          setSchedPrefs(s.prefs);
          setSchedDispatch(s.dispatch);
          if (!s.enabled) setSchedRows({});
        })
        .catch(() => undefined);
      void refreshStatus();
    }, 5_000);
    return () => window.clearInterval(timer);
  }, [client, refreshStatus]);

  // 健康度和日志都是**免费**的（读站点自己的账单 / 本机路由自己的记录，
  // 零额外请求），所以进页面就拉。要花钱的只有站点检验，那个得点。
  useEffect(() => {
    let dropped = false;
    void stationApi
      .refreshHealth()
      .then((rows) => {
        if (!dropped)
          setHealth(Object.fromEntries(rows.map((r) => [r.route_id, r])));
      })
      .catch(() => undefined);
    void stationApi
      .logs(2000)
      .then((rows) => {
        if (!dropped) setLogs(rows);
      })
      .catch(() => undefined);
    return () => {
      dropped = true;
    };
  }, []);

  /** 这个软件底下的线路。⛔ 按 `client` 分，不按协议推。 */
  const mine = useMemo(
    () => routes.filter((r) => r.client === client),
    [routes, client],
  );

  const since =
    range === "all" ? -Infinity : Date.now() - rangeOf(range).hours * 3600_000;
  /** 本段之内、属于这个软件的日志。 */
  const mineLogs = useMemo(() => {
    const ids = new Set(mine.map((r) => r.id));
    return logs.filter((l) => ids.has(l.route_id) && Number(l.at_ms) >= since);
  }, [logs, mine, since]);

  const routeCounts = useMemo(
    () =>
      new Map(
        usage.data?.by_route.map((r) => [r.key, r.totals.requests]) ?? [],
      ),
    [usage.data],
  );

  // ⛔ 「选中的上游」**只有这一个定义**：排队中的那条优先，没有才看已落定的。
  //
  // 点行、调度换线走的都是 `request_switch` —— 只排队，下一发请求才落定
  // （`qb-station::router`）。只认 `current_route` 的话：面板刚启动时它是空的，
  // 点哪一行都不高亮、右列「启动」一直灰着；有值时界面还指着旧的那条，
  // 看起来像点行没反应。演示夹具以前直接改 current，所以截图里一直看不出来。
  const selectedId = status?.pending_route ?? status?.current_route ?? null;
  /** 选中的那条还在排队 —— 下一发请求才生效。 */
  const queued =
    status?.pending_route != null &&
    status.pending_route !== status.current_route;
  const selected = mine.find((r) => r.id === selectedId) ?? null;
  const selection = status?.selection ?? null;
  /** 备用线路（不含当前线路本身），按排的顺序。 */
  const backups = backupsOf(selection);
  /** 桌面端直连一条线时，本机路由不经手：选线、自动选线、启动说明都不生效（B7）。 */
  const direct = client === "claude-desktop" && directRoute != null;
  /** 开自动选线之前要先有什么。`null` = 可以开。 */
  const schedBlocked = runtimeError
    ? "读不到当前路由状态"
    : direct
      ? "桌面端直连一条线：不经本机路由，自动选线不生效"
      : !selection
        ? "正在读取"
        : !selection.current
          ? "先在线路行上点「设为当前」"
          : backups.length === 0
            ? "先在选线与分流里勾备用线路"
            : null;
  const canSchedule = schedBlocked == null;
  const routeLabel = (id: string) => {
    const r = routes.find((route) => route.id === id);
    return r ? `${siteName(r.station_id)} · ${r.group || "默认分组"}` : id;
  };
  const up = Boolean(status?.running && isRunning);

  const act = async (fn: () => Promise<RouterStatus>, what: string) => {
    setBusy(true);
    setLaunchError(null);
    try {
      const next = await fn();
      if (clientRef.current !== client) return;
      setStatus(next);
      await Promise.all([refreshStatus(), refreshSched()]);
    } catch (e) {
      toast.error(`${what}失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  /**
   * 开/关这个软件的智能调度。**落到后端**,不是翻一个本地布尔。
   *
   * 关掉时不动当前上游 —— 停的意思是「以后不替你换了」,
   * 不是「把你换回去」。
   */
  const toggleSched = async () => {
    // 连点只认第一下。⛔ 不用 disabled 挡：按钮一 disabled 键盘焦点就掉回 body，
    // 按空格启停之后 Tab 得从页首重新走一遍。
    if (toggling) return;
    if (!sched && !canSchedule) {
      toast.info(`${schedBlocked}，再开自动选线`);
      return;
    }
    setBusy(true);
    setToggling(true);
    setLaunchError(null);
    try {
      const next = await stationApi.scheduleSet(client, !sched);
      if (clientRef.current !== client) return;
      showSchedule(next);
      if (next.enabled) {
        setIgnite(true);
        window.clearTimeout(igniteTimer.current);
        igniteTimer.current = window.setTimeout(() => setIgnite(false), 1200);
      }
      // 名次随后到，不让按钮陪着转圈等账单。
      void loadRows(next);
      // 开自动选线会把当前线路放进备用名单最前面、`selection.auto_select` 也跟着变：读回来。
      void refreshStatus();
    } catch (e) {
      toast.error(
        `${sched ? "关" : "开"}自动选线失败：${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setBusy(false);
      setToggling(false);
    }
  };

  /** 启动动效收尾：打勾或抖一下，两秒多以后回到平时的样子。 */
  const finishLaunch = (who: Client, fx: "done" | "failed") => {
    setLaunching(null);
    setLaunchFx({ client: who, fx });
    window.clearTimeout(fxTimer.current);
    fxTimer.current = window.setTimeout(() => setLaunchFx(null), 2600);
  };

  /**
   * 右列「启动」：**只是把这个软件打开**，走它的当前线路。不停调度、也不改上游 ——
   * 换线只有行上的「设为当前」（2026-11-14 起，原来行上那个「仅用这条并启动」删了）。
   *
   * ⛔ **本机路由只有「启动」会拉起来**（后端 `station_launch`）。拉路由、起客户端进程是后端里的一个动作，
   * 不在这里拆成两次调用 —— 拆开的话中间失败会留下「路由起了、客户端
   * 没起」的半成品状态，而界面显示「运行中」。
   */
  const launchNow = async () => {
    // 同 toggleSched：连点只认第一下，不靠 disabled 挡（焦点会掉）。
    if (!selected || launching || stopping) return;
    const who = client;
    setBusy(true);
    setLaunching(who);
    setLaunchFx(null);
    setLaunchError(null);
    try {
      await stationApi.launch(who);
      await workspace.refresh();
      if (clientRef.current === who)
        setStatus(await stationApi.routerStatus(who));
      finishLaunch(who, "done");
    } catch (e) {
      // 失败还是要说 —— 不接住的话按钮看起来像没反应。
      setLaunchError({
        client: who,
        text: e instanceof Error ? e.message : String(e),
      });
      finishLaunch(who, "failed");
    } finally {
      setBusy(false);
    }
  };

  // 启动的真实进度。后端每走一步就更新那条操作记录（关闭运行中的桌面端、
  // 准备中转用的可执行文件……；中转不过 IP 锁），工作区数据跟着刷新 —— 启动块上显示的就是这一句，
  // 不是前端自己编的转圈文案。
  //
  // 只按「目标是这个软件的中转环境、还在跑」来找，⛔ 不匹配操作名里的中文：
  // 文案会改，而改文案的人不知道这里靠它做判断。演示模式没有这条记录，退回通用的一句。
  const launchPhase = useMemo(() => {
    if (launching !== client) return null;
    const target = `qb-router-${client}`;
    const op = (workspace.data?.operations ?? [])
      .filter((o) => o.target_id === target && o.status === "running")
      .sort((a, b) => b.started_at.localeCompare(a.started_at))[0];
    // progress 0 是刚建出来那一刻的「准备」，没有信息量，不显示。
    return op && op.progress > 0 ? op.phase : null;
  }, [launching, client, workspace.data]);

  // 按站点归并：一个站点一份余额，底下挂它在这个软件下的各个分组。
  const sites = useMemo(() => {
    const out: { id: string; lines: Route[] }[] = [];
    for (const r of mine) {
      let site = out.find((s) => s.id === r.station_id);
      if (!site) {
        site = { id: r.station_id, lines: [] };
        out.push(site);
      }
      site.lines.push(r);
    }
    return out;
  }, [mine]);

  // ---------------------------------------------------------- 四个总量
  //
  // ⛔ 每一格只报它**真拿得到**的那个口径，而且把口径写在副标题上。
  //
  // 总量、线路计数和图表共享完整 SQL 聚合；平均耗时仍是最近日志样本，明确写出范围。
  const totals = mineLogs
    .map((l) => (l.total_ms == null ? null : Number(l.total_ms)))
    .filter((v): v is number => v != null);
  const avgMs = totals.length
    ? totals.reduce((a, b) => a + b, 0) / totals.length
    : null;

  const clientName = CLIENTS.find((c) => c.id === client)!;

  // ---------------------------------------------------------- 右列按钮上写什么
  /** 启动块现在处在哪一段。只看这个软件自己的启动。 */
  const launchState: "launching" | "done" | "failed" | undefined =
    launching === client
      ? "launching"
      : launchFx?.client === client
        ? launchFx.fx
        : undefined;
  const launchTitle =
    stopping === client
      ? `正在关闭 ${clientName.short}`
      : launching === client
        ? `正在启动 ${clientName.short}`
        : isRunning
          ? `一键关闭 ${clientName.short}`
          : `启动 ${clientName.short}`;
  const selectedGroup = selected ? selected.group || "默认分组" : "";
  const launchLines: string[] =
    stopping === client
      ? ["正在关闭本页启动的会话…"]
      : launching === client
        ? [launchPhase ?? "正在检查配置、打开客户端…"]
        : launchError?.client === client
          ? [launchError.text]
          : isRunning
            ? ["运行中 · 关闭本页启动的会话"]
            : direct
              ? [
                  `直连「${routeLabel(directRoute!)}」：不经本机路由`,
                  "这一页的当前线路不生效",
                ]
              : !selected
                ? ["先在下面线路行上点「设为当前」"]
                : [
                    `走当前线路「${selectedGroup}」`,
                    ...(client === "claude-desktop"
                      ? ["开着的桌面端会先被关掉"]
                      : []),
                  ];
  const schedWhat = schedPrefs
    ? [axesText(schedPrefs.axes), floorsText(schedPrefs.floors)]
        .filter(Boolean)
        .join(" · ")
    : "按存着的偏好在当前线路和备用线路里挑当前线路";
  // G5：写真实的防抖数（原来写死「领先 8% 连续 2 轮才换」，而这几个数在高级设置里改得了）。
  const cadence = schedDispatch
    ? `领先 ${num(schedDispatch.margin * 100)}% 连续 ${schedDispatch.confirm} 轮才换`
    : "";

  return (
    <>
      {/* ------------------------------------------------ 软件分段器 */}
      <div className="qb-st-ctl">
        <div className="qb-st-ctabs" role="tablist" aria-label="软件">
          {CLIENTS.map((c) => (
            <button
              key={c.id}
              role="tab"
              aria-selected={c.id === client}
              onClick={() => setClient(c.id)}
            >
              {c.short}
              <span className="n">
                {routes.filter((r) => r.client === c.id).length}
              </span>
              {up && c.id === client && (
                <span className="live" title="运行中" />
              )}
            </button>
          ))}
        </div>
        <span style={{ flex: 1 }} />
        {/* 「项目」按钮 2026-11-15 挪到了「扩展」页（B5 / G1）：它只管 MCP / Skills / 提示词，不碰线路。 */}
        {/* 导入链接（阶段 5 · X4）：粘贴中转站给的 qbgate:// / ccswitch:// 链接。 */}
        <DeepLinkPasteButton />
        <Button
          variant="primary"
          onClick={() => setRouteDialog({ editing: null })}
        >
          ＋ 添加线路
        </Button>
      </div>

      {/* ------------------------------------------------ 总览卡 + 右列 */}
      <div className="qb-st-ov">
        <div className="qb-st-ovmain">
          <div className="qb-st-now">
            <span className="eyebrow">正在路由</span>
            {selected ? (
              <>
                <span className="name">
                  {siteName(selected.station_id)} ·{" "}
                  {selected.group || "默认分组"}
                </span>
                <span className="qb-st-pill qb-st-pill--accent">
                  {queued
                    ? "已排队 · 下一次请求生效"
                    : up
                      ? "运行中"
                      : "已选中 · 路由没启动"}
                </span>
                <span style={{ flex: 1 }} />
                <span className="side">
                  倍率{" "}
                  <RateCell
                    route={selected}
                    topup={topupOf(
                      providers.find((p) => p.id === selected.station_id),
                    )}
                  />
                </span>
              </>
            ) : (
              <span className="name dim">
                还没有当前线路 —— 在下面线路行上点「设为当前」
              </span>
            )}
          </div>

          <div className="qb-st-kpis">
            <StationUsageKpiValues
              totals={usage.data?.totals ?? null}
              error={usage.error != null}
              range={range}
            />
            <Kpi
              k="平均耗时（样本）"
              v={avgMs == null ? "—" : `${(avgMs / 1000).toFixed(2)} s`}
              s={
                totals.length
                  ? `最近日志中取到耗时的 ${totals.length} 条`
                  : "最近日志样本未记到耗时"
              }
            />
          </div>

          {/* 柱状图上方只有一行（第三版样机）：左边「柱状图」下拉框当标题 + 一句总量，
              右边时间范围、粒度两个小下拉框 +「用量明细」。不再单写「时间范围」「粒度」字样（在 aria-label / title 里）。 */}
          <StationUsageChart
            data={usage.data}
            error={usage.error}
            metric={metric}
            onMetric={setMetric}
            controls={
              <>
                <select
                  aria-label="时间范围"
                  title="时间范围"
                  value={range}
                  onChange={(e) => setRange(e.target.value as RangeId)}
                >
                  {RANGES.map((r) => (
                    <option key={r.id} value={r.id}>
                      {r.name.replace(/^近 /, "")}
                    </option>
                  ))}
                </select>
                <select
                  aria-label="粒度"
                  title={
                    range === "all"
                      ? "全部历史只有按天的归档"
                      : "粒度：每根柱子多长"
                  }
                  value={usageGran}
                  disabled={range === "all"}
                  onChange={(e) => setGran(e.target.value as GranId)}
                >
                  {range !== "7d" && range !== "all" && (
                    <option value="minute">每 5 分钟</option>
                  )}
                  <option value="hour">按小时</option>
                  <option value="day">按天</option>
                </select>
                {/* 中转站重构阶段 6 · U1：能滚动的用量明细页（趋势、全年热力图、按站点 / 模型、逐条日志）。 */}
                <Link
                  to={`/relays/usage?client=${client}`}
                  className="btn btn--sm btn--ghost"
                >
                  用量明细
                </Link>
              </>
            }
          />
        </div>

        {/* 右列只有三个按钮。⛔ 不要往里塞别的（排序看板被删过）。
            三个都是整块点的大按钮 —— 使用者点名不要小开关。按高度撑满、
            底边对齐总览卡，多出来的高度拿来写真实状态，不留空地。 */}
        <div className="qb-st-ovside">
          {/* 启动的是**这个软件**，不是本机路由的开关 —— 路由是它顺带拉起来的。
              没选上游时置灰，⛔ 原因直接写在按钮上：只放进 title 等于没说。
              启动过程就在按钮本身上演：光带一直扫、底边进度条、后端报来的真实阶段；
              结束时打勾泛光或者抖一下 —— 不用看别处就知道成没成。 */}
          <button
            type="button"
            className="qb-st-launchbtn"
            data-fx={launchState}
            // 只在「没有选中的上游」时灰。⛔ 别把 busy 加回来：点行、启停调度都会
            // 置 busy，那样每点一下行这块主色就闪一下灰。
            disabled={!selected && !isRunning}
            aria-busy={
              launchState === "launching" || stopping === client || undefined
            }
            onPointerDown={launchRipple.onPointerDown}
            onClick={() => void (isRunning ? closeNow() : launchNow())}
            title={
              isRunning
                ? `关闭本页启动的 ${clientName.name}`
                : direct
                  ? `桌面端直连「${routeLabel(directRoute!)}」，不经本机路由：这一页的当前线路、备用线路、自动选线都不生效（在「接入方式」里改回经本机路由）`
                  : selected
                    ? `启动 ${clientName.name}，走当前线路「${selectedGroup}」；顺带把本机路由拉起来，并把它的 base_url 指过去${
                        client === "claude-desktop"
                          ? "。桌面端开着的话先替你关掉它（单实例，不关换不了配置目录）"
                          : ""
                      }${sched ? "。自动选线不受影响，照常挑当前线路" : ""}`
                    : "先在下面线路行上点「设为当前」"
            }
          >
            <span className="qb-st-sheen" aria-hidden="true" />
            {launchRipple.layer}
            <span className="hd">
              <span className="qb-st-ico" aria-hidden="true">
                {launchState === "launching" || stopping === client ? (
                  <span className="qb-st-orbit" />
                ) : isRunning ? (
                  <Square size={15} />
                ) : launchState === "done" ? (
                  <Check size={16} strokeWidth={2.6} />
                ) : launchState === "failed" ? (
                  <X size={16} strokeWidth={2.6} />
                ) : (
                  <Play size={15} />
                )}
              </span>
              {launchTitle}
            </span>
            {/* 每句单独一行写死 —— 拼成一句的话会在中间折断。直连时置灰（B7）。 */}
            {launchLines.map((line) => (
              <span key={line} className={`sub${direct ? " qb-st-dim" : ""}`}>
                {line}
              </span>
            ))}
            {launchState === "launching" && (
              <span className="qb-st-progress" aria-hidden="true">
                <i />
              </span>
            )}
          </button>

          {setupNotice && (
            <p role="status" className="qb-st-nudge">
              {setupNotice}
            </p>
          )}
          {/* 自动选线：点一下开，再点一下关。全页只有这一个开关（D1：弹窗页脚那个删了）。⛔ 不许改成「打开设置」——
              设置是下面单独那个按钮。开着时整块转绿，一道光沿着边一直绕、
              状态点一圈圈往外扩，告诉人它是真的在跑。 */}
          <button
            type="button"
            className="qb-st-schedbtn"
            data-fx={toggling ? "busy" : ignite ? "ignite" : undefined}
            aria-label="自动选线"
            aria-pressed={sched}
            aria-describedby={schedDescId}
            aria-busy={toggling || undefined}
            aria-disabled={!sched && !canSchedule}
            onPointerDown={schedRipple.onPointerDown}
            onClick={() => void toggleSched()}
          >
            <span className="qb-st-edge" aria-hidden="true" />
            <span className="qb-st-sheen" aria-hidden="true" />
            {schedRipple.layer}
            <span className="hd">
              <span className="qb-st-ico" aria-hidden="true">
                {toggling ? (
                  <span className="qb-st-orbit" />
                ) : sched ? (
                  <span className="qb-st-sonar" />
                ) : (
                  <Zap size={15} />
                )}
              </span>
              {sched ? "自动选线运行中" : "开启自动选线"}
            </span>
            <span id={schedDescId} className="desc">
              <span className="sub">
                {!sched && !canSchedule ? schedBlocked : schedWhat}
              </span>
              {sched && (
                <span className="sub last" title={schedNote || undefined}>
                  {schedNote ? `上一轮：${schedNote}` : "每 30 秒排一次"}
                </span>
              )}
              {/* ⛔ 「面板关掉它就停」必须写在这里。不写的话使用者会以为
                  关了面板还有人替他挑线路 —— 而实际上客户端会一直用
                  最后选中的那条，可能是几天前那条已经涨价的。 */}
              <span className="foot">
                {sched
                  ? [cadence, "面板关掉即停"].filter(Boolean).join(" · ")
                  : "面板开着才生效"}
              </span>
            </span>
          </button>

          {/* 本机路由每个软件一份的设置：第一屏是备用线路与自动选线偏好，其余在「高级设置」里。
              第二行写备用线路是谁（第三版样机：原来那张全宽的范围卡删了）。
              熔断小标只在后端说「会影响发送」时才有（E6：只有一条线时熔断不拦发送，不显示）。 */}
          <button
            type="button"
            className={`qb-st-schedcfg qb-st-selcard${direct ? " is-direct" : ""}`}
            aria-label={`选线与分流 · ${backupSummary(selection, routeLabel)}`}
            title={
              direct
                ? "桌面端直连一条线：不经本机路由，这里的备用线路、自动选线都不生效"
                : undefined
            }
            onClick={() => setRouterOpen(true)}
          >
            <span className="qb-st-sheen" aria-hidden="true" />
            <span className="qb-st-selcard-hd">
              <Shuffle size={14} aria-hidden="true" />
              <span className="qb-st-selcard-title">选线与分流</span>
              <ChevronRight size={14} className="chev" aria-hidden="true" />
            </span>
            <span className="qb-st-selcard-sub">
              {(status?.breakers.length ?? 0) > 0 && (
                <span className="qb-st-pill qb-st-pill--danger">
                  {status?.breakers.length} 条熔断
                </span>
              )}{" "}
              {direct
                ? "直连中：备用线路不生效"
                : backupSummary(selection, routeLabel)}
            </span>
          </button>

          {/* 阶段 4 · C5：每个软件各自那一件。 */}
          {client === "claude-code" && (
            <button
              type="button"
              className="qb-st-schedcfg"
              onClick={() => setExtrasOpen(true)}
            >
              <span className="qb-st-sheen" aria-hidden="true" />
              <Wrench size={14} aria-hidden="true" />
              首次确认 · VS Code 插件
              <ChevronRight size={14} className="chev" aria-hidden="true" />
            </button>
          )}
          {client === "claude-desktop" && (
            <button
              type="button"
              className="qb-st-schedcfg"
              onClick={() => setDirectOpen(true)}
            >
              <span className="qb-st-sheen" aria-hidden="true" />
              <Plug size={14} aria-hidden="true" />
              接入方式 · {directRoute ? "直连一条线" : "经本机路由"}
              {directRoute && (
                <span className="qb-st-pill qb-st-pill--warn">
                  直连中 · 真 Key 在桌面端配置里
                </span>
              )}
              <ChevronRight size={14} className="chev" aria-hidden="true" />
            </button>
          )}
          {client === "claude-desktop" && (
            <button
              type="button"
              className="qb-st-schedcfg"
              disabled={importing}
              title="同一个站点、同一个分组、同一把 Key 复制一份给桌面端；已经有的跳过"
              onClick={() => {
                setImporting(true);
                void stationApi
                  .importDesktopRoutes()
                  .then((added) => {
                    if (added.length === 0)
                      toast.info(
                        "没有要导入的：Claude Code 的线路桌面端都有了（或者还没绑 Key）",
                      );
                    else
                      toast.ok(`从 Claude Code 导入了 ${added.length} 条线路`);
                    onChanged();
                  })
                  .catch((e: unknown) =>
                    toast.error(
                      `导入失败：${e instanceof Error ? e.message : String(e)}`,
                    ),
                  )
                  .finally(() => setImporting(false));
              }}
            >
              <span className="qb-st-sheen" aria-hidden="true" />
              <Download size={14} aria-hidden="true" />
              从 Claude Code 导入线路
              <ChevronRight size={14} className="chev" aria-hidden="true" />
            </button>
          )}
          {client === "codex" && (
            <button
              type="button"
              className="qb-st-schedcfg"
              onClick={() => setHistoryOpen(true)}
            >
              <span className="qb-st-sheen" aria-hidden="true" />
              <History size={14} aria-hidden="true" />
              会话归桶 · 找回老会话
              <ChevronRight size={14} className="chev" aria-hidden="true" />
            </button>
          )}

          {/* turn-state 只对官方 Codex 有意义,别的软件不显示这个入口。 */}
          {client === "codex" && (
            <button
              type="button"
              className="qb-st-schedcfg"
              onClick={() => setTurnOpen(true)}
            >
              <span className="qb-st-sheen" aria-hidden="true" />
              <Fingerprint size={14} aria-hidden="true" />
              GPT 质量检查
              <ChevronRight size={14} className="chev" aria-hidden="true" />
            </button>
          )}
        </div>
      </div>

      {/* 「最近请求实际去向」那一行（第三版样机）：平时照旧写最近一发去了哪；当前线路被暂停（C5）、
          客户端选了聚合模型（B6）时这一行变黄，兼做提示。只在出问题时才变，不额外占地方。 */}
      <RecentRoute
        selection={selection}
        latest={latestRequest}
        error={runtimeError}
        routeLabel={routeLabel}
        onRetry={(id) =>
          void stationApi
            .retest(id)
            .then((was) => {
              toast.ok(
                was
                  ? "当前线路转成半开了：下一发真实请求就是回探（不额外花钱）"
                  : "当前线路已经不在熔断了",
              );
              void refreshStatus();
            })
            .catch((e: unknown) =>
              toast.error(
                `重试失败：${e instanceof Error ? e.message : String(e)}`,
              ),
            )
        }
      />

      {/* ------------------------------------------------ 站点 → 线路行 */}
      {sites.length === 0 ? (
        <div className="qb-st-empty">
          <p>
            <b>{clientName.name}</b> 底下还没有线路。
            <br />
            线路按软件分开 —— 在别的软件下加过的，这里看不到。
            站点（域名）是共用的，加线路时直接选。
          </p>
          <Button
            variant="primary"
            onClick={() => setRouteDialog({ editing: null })}
          >
            ＋ 添加第一条线路
          </Button>
        </div>
      ) : (
        <div className={`qb-st-sites${sched ? " sched" : ""}`}>
          {sites.map((s) => (
            <article key={s.id} className="qb-st-site">
              <div className="qb-st-sitehd">
                <b>{siteName(s.id)}</b>
                <small>{siteUrl(s.id) || "（站点已删除，线路成了孤儿）"}</small>
                {topupOf(providers.find((p) => p.id === s.id)) !== 1 && (
                  <span
                    className="qb-st-pill qb-st-pill--unknown"
                    title="充值比例：这家站买 1 美元站内额度要付几元。跨站比便宜时倍率先乘上它"
                  >
                    1 美元额度 = {topupOf(providers.find((p) => p.id === s.id))}{" "}
                    元
                  </span>
                )}
                {sched &&
                  s.lines.some(
                    (l) => l.id === selectedId || backups.includes(l.id),
                  ) && (
                    <span className="qb-st-pill qb-st-pill--ok">
                      自动选线中
                    </span>
                  )}
                <span style={{ flex: 1 }} />
                {/* 余额是站点级的 —— 整站共享，画在这一层。阶段 6 · U3 起认得出的那几家
                    （DeepSeek、Kimi 编程套餐……）用第一条线路的 Key 问一次，别的照旧「站点未提供」。 */}
                <StationSiteQuota
                  key={s.lines[0].id}
                  routeId={s.lines[0].id}
                  group={s.lines[0].group}
                  stationId={s.id}
                  siteName={siteName(s.id)}
                />
              </div>
              {s.lines.map((r) => (
                <RouteRow
                  key={r.id}
                  route={r}
                  health={health[r.id]}
                  requests={usage.data ? (routeCounts.get(r.id) ?? 0) : null}
                  range={range}
                  current={selectedId === r.id}
                  backupNo={
                    backups.includes(r.id) ? backups.indexOf(r.id) + 1 : null
                  }
                  running={isRunning}
                  onClose={() => void closeNow()}
                  queued={queued && selectedId === r.id}
                  sched={sched}
                  // 两份都看：刚点开自动选线时 `sched` 已经翻过来，而 `status` 要等下一次读回。
                  // 任一份说开着就先问 —— 宁可多问一句，也不能悄悄把它关掉。
                  autoSelect={sched || selection?.auto_select === true}
                  lockReason={
                    direct
                      ? "桌面端直连一条线：不经本机路由，设为当前不生效"
                      : null
                  }
                  busy={busy}
                  lane={schedRows[r.id]}
                  topup={topupOf(providers.find((p) => p.id === s.id))}
                  onSetCurrent={() => {
                    // 设为当前：只换当前线路，备用名单不动；自动选线开着时后端会关掉它，并如实说。
                    void act(async () => {
                      const done = await stationApi.setCurrent(r.id);
                      toast.ok(
                        done.auto_select_stopped
                          ? "已设为当前，下一次请求生效。备用名单没动；自动选线已关"
                          : "已设为当前，下一次请求生效。备用名单没动",
                      );
                      return done.status;
                    }, "设为当前");
                  }}
                  onAudit={() => setAuditOpen(r)}
                  onLogs={() => setLogOpen(r)}
                  onEdit={() => setRouteDialog({ editing: r })}
                  onRemove={() => {
                    // 行里已经确认过一次（A2）。失败要落到提示条上：不接住的话是一个未捕获的 rejection ——
                    // 界面一动不动，只有控制台里有话说，看起来像「按钮没反应」。
                    void stationApi
                      .removeRoute(r.id)
                      .then(() => {
                        toast.ok(`已移除「${r.group || "默认分组"}」`);
                        onChanged();
                        void refreshStatus();
                      })
                      .catch((e: unknown) => {
                        toast.error(
                          `移除失败：${e instanceof Error ? e.message : String(e)}`,
                        );
                      });
                  }}
                />
              ))}
            </article>
          ))}
        </div>
      )}

      {/* ------------------------------------------------ 弹窗 */}
      <RouteDialog
        open={routeDialog != null}
        onClose={() => setRouteDialog(null)}
        editing={routeDialog?.editing ?? null}
        client={client}
        providers={providers}
        credentials={workspace.data?.credentials ?? []}
        onSave={async (route) => {
          await stationApi.putRoute(route, routeDialog?.editing?.id);
          await workspace.refresh();
          onChanged();
        }}
      />

      <Modal
        open={routerOpen}
        onClose={() => {
          setRouterOpen(false);
          // 关掉时读回来一次，不然主页的备用摘要、行上的「第 N 备用」还停在打开弹窗之前。
          void refreshStatus();
          void refreshSched();
        }}
        title={`选线与分流 · ${clientName.name}`}
        size="form"
      >
        {direct && (
          <p className="qb-st-nudge">
            <span aria-hidden="true">⚠</span>
            <span>
              桌面端现在直连「{routeLabel(directRoute!)}
              」，不经本机路由：这里的备用线路、自动选线都不生效。
              要让它们生效，先在右列「接入方式」里改回经本机路由。
            </span>
          </p>
        )}
        {routerOpen && (
          <StationRouterSettings
            client={client}
            routes={mine}
            status={status}
            label={(r) =>
              `${siteName(r.station_id)} · ${r.group || "默认分组"}`
            }
            onSaved={() => {
              void refreshStatus();
              void refreshSched();
            }}
          />
        )}
      </Modal>

      <Modal
        open={extrasOpen}
        onClose={() => setExtrasOpen(false)}
        title="Claude Code · 首次确认与 VS Code 插件"
        size="form"
      >
        {extrasOpen && <StationClaudeCodeExtras />}
      </Modal>

      <Modal
        open={directOpen}
        onClose={() => setDirectOpen(false)}
        title="Claude 桌面端 · 接入方式"
        size="form"
      >
        {directOpen && (
          <StationDesktopDirect
            routes={routes.filter((r) => r.client === "claude-desktop")}
            label={(r) =>
              `${siteName(r.station_id)} · ${r.group || "默认分组"}`
            }
            onChanged={(v) => setDirectRoute(v.route_id)}
          />
        )}
      </Modal>

      <Modal
        open={historyOpen}
        onClose={() => setHistoryOpen(false)}
        title="Codex · 会话归桶"
        size="huge"
      >
        {historyOpen && <StationCodexHistory />}
      </Modal>

      <Modal
        open={turnOpen}
        onClose={() => setTurnOpen(false)}
        title="GPT 质量检查"
        size="huge"
      >
        {turnOpen && (
          <StationQualityCheck
            routeId={selectedId ?? undefined}
            route={selectedId ? routeLabel(selectedId) : ""}
          />
        )}
      </Modal>

      <Modal
        open={logOpen != null}
        onClose={() => setLogOpen(null)}
        title={`请求日志 · ${logOpen ? logOpen.group || "默认分组" : ""}`}
        size="huge"
      >
        {logOpen && (
          <StationLogs
            route={logOpen}
            health={health[logOpen.id]}
            siteUrl={siteUrl(logOpen.station_id)}
            credentials={workspace.data?.credentials ?? []}
          />
        )}
      </Modal>

      <Modal
        open={auditOpen != null}
        onClose={() => setAuditOpen(null)}
        title="查套路 · 账单检验"
        size="huge"
        dismissible={!auditBusy}
      >
        {auditOpen && (
          // focus 一定要给：行上已经选过这条线了，弹窗里不该再问一遍。
          <StationAudit
            route={auditOpen}
            stationName={siteName(auditOpen.station_id)}
            onBusyChange={setAuditBusy}
          />
        )}
      </Modal>
    </>
  );
}

// ------------------------------------------------------------------ 线路行

/**
 * 一条线路行。**整行不可点**（A1，2026-11-15）：原来整行是 `div[role=radio]`，点一下就切线、关自动选线，
 * 还会把多条线收回成一条。现在只有行上的按钮能动：
 *
 * - 当前线路那一行：「当前线路」标签，不显示「设为当前」；
 * - 备用线路：「第 N 备用」标签；
 * - 「设为当前」：只换当前线路，备用名单不动。自动选线开着时先在这一行里问一句（它会被关掉）；
 *   悬停说明写清正在进行的对话从下一句起改走新线路、提示缓存会失效（G7）；
 * - 「移除」：在行里再点一次确认，写明不能恢复；是当前线路时再加一句（A2）。
 */
function RouteRow({
  route,
  health,
  requests,
  range,
  current,
  backupNo,
  running,
  onClose,
  queued,
  sched,
  autoSelect,
  lockReason,
  busy,
  lane,
  topup,
  onSetCurrent,
  onAudit,
  onLogs,
  onEdit,
  onRemove,
}: {
  route: Route;
  health?: RouteHealthView;
  requests: number | null;
  range: RangeId;
  /** 这一条是当前线路（排队中的也算）。 */
  current: boolean;
  /** 第几条备用线路；不是备用线路是 `null`。 */
  backupNo: number | null;
  running: boolean;
  onClose: () => void;
  /** 设为当前了但还在排队，下一发请求才生效。 */
  queued: boolean;
  sched: boolean;
  /** 自动选线开着 —— 「设为当前」会把它关掉，先问一句。 */
  autoSelect: boolean;
  /** 「设为当前」点了也不生效的原因（桌面端直连）；`null` = 能点。 */
  lockReason: string | null;
  busy: boolean;
  /** 自动选线开着时这条线的名次。`undefined` = 没开，或者它没参与排序。 */
  lane?: { weakest: Axis | null; score: number | null };
  /** 这家站 1 美元额度付几元（充值比例），见 `topupOf`。 */
  topup: number;
  onSetCurrent: () => void;
  onAudit: () => void;
  onLogs: () => void;
  onEdit: () => void;
  onRemove: () => void;
}) {
  /** 行里展开的那一句确认：设为当前（自动选线开着时）或移除。 */
  const [asking, setAsking] = useState<"current" | "remove" | null>(null);
  // ⛔ 不看 completion_ratio 大不大于 1：那是「输出价 ÷ 输入价」，Claude 官方本来就是 5。
  // 要看的是它比官方的那个比例多出来多少，由检验时拿官方价算好（`output_markup`）。
  const markup = route.output_markup;
  const name = route.group || "默认分组";
  return (
    <div
      className={`qb-st-row${current ? " on" : ""}`}
      role="group"
      aria-label={`线路「${name}」${current ? "，当前线路" : backupNo ? `，第 ${backupNo} 备用` : ""}`}
    >
      <div className="qb-st-rowtop">
        <span className="gname">{name}</span>
        {current && (
          <span className="qb-st-pill qb-st-pill--accent">
            {queued ? "当前线路 · 下一次请求生效" : "当前线路"}
          </span>
        )}
        {!current && backupNo != null && (
          <span
            className="qb-st-pill qb-st-pill--unknown"
            title="当前线路出故障时按这个顺序改用；在「选线与分流」里改"
          >
            第 {backupNo} 备用
          </span>
        )}
        {markup != null &&
          Number.isFinite(markup) &&
          markup > 1 + OUTPUT_MARKUP_TOLERANCE && (
            <span
              className="qb-st-pill qb-st-pill--warn"
              title={`按官方的「输出价 ÷ 输入价」算，这家的输出另外多收了 ${Math.round((markup - 1) * 100)}%（官方的输出本来就比输入贵，这里已经扣掉了）`}
            >
              输出加价 ×{markup.toFixed(2)}
            </span>
          )}
        {!route.credential_id && (
          <span className="qb-st-pill qb-st-pill--unknown">没绑令牌</span>
        )}
        {/* 阶段 3：上游说的协议跟软件不一样，本机路由两头转换 —— 标出来，别让人以为是原样转发。 */}
        {convertedFormat(route) && (
          <span
            className="qb-st-pill qb-st-pill--accent"
            title="这条线的上游说的协议跟这个软件不一样：本机路由把请求换过去、回复换回来"
          >
            转换 → {UPSTREAM_FORMAT_LABEL[convertedFormat(route)!]}
          </span>
        )}
        <AuditPill route={route} />
        {/* 自动选线开着时，当前那行写「自动选线选中 · 最弱项 .47」，其余行各写各的最弱项。
            ⛔ 算不出名次的**不写 0 分** —— 那是断言「它很差」，而实际上是勾中的维度里有一项没有证据。 */}
        {sched && (current || lane) && (
          <span
            className={`qb-st-pill ${current ? "qb-st-pill--ok" : "qb-st-pill--unknown"}`}
            title={
              lane?.weakest
                ? `这条线的总分由「${AXIS_LABEL[lane.weakest]}」这一维决定`
                : "勾中的维度里有一项这条线没有证据，排不出名次"
            }
          >
            {current ? "自动选线选中" : ""}
            {current && lane ? " · " : ""}
            {lane
              ? lane.score == null
                ? "没有证据"
                : `最弱项 ${AXIS_LABEL[lane.weakest ?? "cheap"]} ${lane.score
                    .toFixed(2)
                    .replace(/^0/, "")}`
              : ""}
          </span>
        )}
        <span className="acts">
          {running && current ? (
            <Button variant="primary" onClick={onClose}>
              一键关闭
            </Button>
          ) : (
            !current && (
              <Button
                variant="primary"
                disabled={busy || lockReason != null || asking != null}
                title={
                  lockReason ??
                  `只换当前线路，备用名单不动；下一次请求生效。正在进行的对话从下一句起改走这条线，上游那边的提示缓存会失效（多花一次缓存写）${
                    autoSelect ? "。自动选线开着，会被关掉" : ""
                  }`
                }
                onClick={() =>
                  autoSelect ? setAsking("current") : onSetCurrent()
                }
              >
                设为当前
              </Button>
            )
          )}
          <Button variant="ghost" onClick={onAudit}>
            查套路
          </Button>
          <Button
            variant="ghost"
            onClick={onLogs}
            aria-label={`「${name}」的请求日志`}
            title={requests === 0 ? "本段没有最终归属此线路的请求" : "请求日志"}
          >
            ▤
          </Button>
          <Button
            variant="ghost"
            onClick={onEdit}
            aria-label={`编辑「${name}」`}
            title="编辑"
          >
            ✎
          </Button>
          <Button
            variant="ghost"
            onClick={() => setAsking("remove")}
            disabled={asking != null}
            aria-label={`移除「${name}」`}
            title="移除"
          >
            🗑
          </Button>
        </span>
      </div>

      {asking === "current" && (
        <div className="qb-st-rowask">
          <InlineConfirm
            message="自动选线开着，设为当前会把它关掉（备用名单不动）。正在进行的对话从下一句起改走这条线，提示缓存会失效。"
            confirmLabel="继续"
            busy={busy}
            onCancel={() => setAsking(null)}
            onConfirm={() => {
              setAsking(null);
              onSetCurrent();
            }}
          />
        </div>
      )}
      {asking === "remove" && (
        <div className="qb-st-rowask">
          <InlineConfirm
            danger
            message={`移除「${name}」：这条线路连同它的设置一起删掉，不能恢复（站点和 Key 留着）${
              current
                ? "。而且它是当前线路：删掉之后下一次请求会直接报错，要先另设一条当前线路"
                : backupNo != null
                  ? "；它也会从备用线路里拿掉"
                  : ""
            }。`}
            confirmLabel="确认移除"
            onCancel={() => setAsking(null)}
            onConfirm={() => {
              setAsking(null);
              onRemove();
            }}
          />
        </div>
      )}

      {/* ⛔ 用间距分组，竖线只是装饰 —— 靠竖线分格撑布局的话，一换行就断成两截。 */}
      <div className="qb-st-quick">
        <Cell k="倍率">
          <RateCell route={route} topup={topup} />
        </Cell>
        <Cell k="成功率">{pct(health?.success_rate)}</Cell>
        <Cell k="缓存命中">{pct(health?.cache_hit_rate)}</Cell>
        <Cell k="首字 P95">
          {health?.ttft_p95_ms == null
            ? dash
            : `${(health.ttft_p95_ms / 1000).toFixed(2)} s`}
        </Cell>
        <Cell k="本段请求">
          <span
            title={`${rangeOf(range).name} · 本机完整记录，按请求最终归属线路统计`}
          >
            {requests == null ? dash : requests.toLocaleString()}
          </span>
        </Cell>
        <Cell k="本段实扣">
          {/* 账单只有 24 小时窗 —— 别的档没有数，写「—」而不是拿 24h 顶上。 */}
          {range === "24h" && health?.cost_24h != null
            ? health.cost_24h.toFixed(4)
            : dash}
        </Cell>
        <Cell k="上次检验">
          {route.last_audit_ms == null ? (
            <span className="qb-st-dim">从没检验过</span>
          ) : (
            stamp(Number(route.last_audit_ms))
          )}
        </Cell>
      </div>

      {health?.error && (
        <div className="qb-st-rowerr">
          {/* 拉取失败和「这段时间没跑东西」必须分开 —— 混成一个，
              界面会把挂掉的站显示成健康的。 */}
          {health.error}（上面几项是没有数据，不是零）
        </div>
      )}
      {health && !health.error && (
        <p className="qb-st-note">
          账单统计为最近一页（最多 100 条）的时间窗样本，不代表完整流量。
        </p>
      )}
    </div>
  );
}

/**
 * 点击涟漪：从按下的位置泛开一圈。纯装饰（aria-hidden），动画放完自己摘掉。
 *
 * 用真元素、不用伪元素：伪元素一个按钮只有一个，连点两下第二圈会把第一圈顶掉。
 * 最多同时留三圈，免得狂点时堆出一串节点。键盘触发（空格 / 回车）没有按下的
 * 位置，不出涟漪 —— 那时焦点环就是反馈。
 */
function useRipple() {
  const [ripples, setRipples] = useState<
    { id: number; x: number; y: number; size: number }[]
  >([]);
  const seq = useRef(0);
  const onPointerDown = (e: React.PointerEvent<HTMLElement>) => {
    if (e.button !== 0) return;
    const box = e.currentTarget.getBoundingClientRect();
    const id = ++seq.current;
    setRipples((rs) => [
      ...rs.slice(-2),
      {
        id,
        x: e.clientX - box.left,
        y: e.clientY - box.top,
        size: Math.max(box.width, box.height) * 2.2,
      },
    ]);
  };
  const layer = ripples.map((r) => (
    <span
      key={r.id}
      className="qb-st-ripple"
      aria-hidden="true"
      style={{ left: r.x, top: r.y, width: r.size, height: r.size }}
      onAnimationEnd={() => setRipples((rs) => rs.filter((x) => x.id !== r.id))}
    />
  ));
  return { onPointerDown, layer };
}

function Cell({ k, children }: { k: string; children: React.ReactNode }) {
  return (
    <div>
      <span className="qb-st-k">{k}</span>
      <span className="qb-st-v">{children}</span>
    </div>
  );
}

function Kpi({ k, v, s }: { k: string; v: string; s: string }) {
  return (
    <div className="qb-st-kpi">
      <span className="k">{k}</span>
      <span className="v">{v}</span>
      <span className="s">{s}</span>
    </div>
  );
}

/**
 * 检验状态。**「从没检验过」是独立行态**（虚线框），不写成 0 分 ——
 * 0 是断言，没检验过是还没有断言。
 */
function AuditPill({ route }: { route: Route }) {
  const m = route.latest_mult;
  if (m == null || !Number.isFinite(m) || m <= 0)
    return <span className="qb-st-pill qb-st-pill--unknown">从没检验过</span>;
  return Math.abs(m - 1) <= 0.05 ? (
    <span className="qb-st-pill qb-st-pill--ok">六项都对得上</span>
  ) : (
    <span className="qb-st-pill qb-st-pill--danger">
      倍率对不上 ×{m.toFixed(2)}
    </span>
  );
}

/**
 * 倍率三态。
 *
 * ⛔ 「没检验过」不写成「已核实」，也不写成 0 —— 那是还没有断言。
 *
 * 倍率是站内口径（站内标价 ÷ 官方牌价）。这家站的充值比例不是 1 元 = 1 美元额度时，
 * 后面多写一句折合多少元 —— 调度比便宜用的也是折过的那个数。
 */
function RateCell({ route, topup }: { route: Route; topup: number }) {
  const cell = rateCell(route);
  const shown =
    cell.kind === "unverified"
      ? cell.nominal
      : cell.kind === "verified"
        ? cell.rate
        : cell.real;
  const folded =
    topup !== 1 && shown != null ? (
      <span
        className="qb-st-dim"
        title={`这家站 1 美元额度要付 ${topup} 元，倍率要乘上它才能跟别家比`}
      >
        {" "}
        · 折合每 $1 牌价 {(shown * topup).toFixed(2)} 元
      </span>
    ) : null;
  if (cell.kind === "unverified")
    return (
      <>
        {cell.nominal == null ? dash : `×${cell.nominal.toFixed(2)}`}{" "}
        <span className="qb-st-dim">标称 · 未核实</span>
        {folded}
      </>
    );
  if (cell.kind === "verified")
    return (
      <>
        ×{cell.rate.toFixed(2)}{" "}
        <span className="qb-st-pill qb-st-pill--ok">已核实</span>
        {folded}
      </>
    );
  return (
    <>
      <span className="qb-st-strike">×{cell.nominal.toFixed(2)}</span>{" "}
      <b style={{ color: "var(--danger)" }}>×{cell.real.toFixed(2)}</b>{" "}
      <span className="qb-st-pill qb-st-pill--danger">实测</span>
      {folded}
    </>
  );
}
