/**
 * 中转站。**没有子菜单栏。**
 *
 * 原来的四个分页（中转站账户 / 智能调度 / 站点检验 / 请求日志）全部取消：
 * 调度进了右列按钮 + 设置弹窗，站点检验并进每条账户行，请求日志一行一个 ▤。
 * 分页把「这条线现在怎么样」拆成了四页，而使用者要决定的是同一件事 ——
 * 换哪条上游。四页之间来回跳等于让人自己在脑子里做联表查询。
 *
 * 页面自上而下只有四块：软件分段器 → 总览卡 + 右列 → 站点卡 → 账户行。
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
  Play,
  SlidersHorizontal,
  Square,
  X,
  Zap,
} from "lucide-react";

import type { Axis } from "../../lib/generated/Axis";
import type { Client } from "../../lib/generated/Client";
import type { Floors } from "../../lib/generated/Floors";
import type { Provider } from "../../lib/generated/Provider";
import type { Schedule } from "../../lib/generated/Schedule";
import {
  rateCell,
  stationApi,
  type Prefs,
  type RequestLog,
  type Route,
  type RouteHealthView,
  type RouterStatus,
} from "../../lib/station";
import { useWorkspace, workspaceApi } from "../../lib/workspace";
import { Button, Modal, useToast } from "../../ui";
import {
  Chart,
  dash,
  mtok,
  pct,
  RANGES,
  rangeOf,
  stamp,
  tokenMix,
  type GranId,
  type RangeId,
} from "./shared";
import StationSchedule from "./StationSchedule";
import StationAudit from "./StationAudit";
import StationLogs from "./StationLogs";
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

const CLIENTS: { id: Client; name: string; short: string }[] = [
  { id: "claude-code", name: "Claude Code", short: "Claude Code" },
  { id: "claude-desktop", name: "Claude 桌面端", short: "桌面端" },
  { id: "codex", name: "Codex 桌面端", short: "Codex 桌面端" },
];

export default function StationCenter() {
  const [routes, setRoutes] = useState<Route[]>([]);
  const toast = useToast();

  const reload = useCallback(async () => {
    try {
      setRoutes(await stationApi.routes());
    } catch (e) {
      toast.error(
        `读不到线路池：${e instanceof Error ? e.message : String(e)}`,
      );
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

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
  const [client, setClient] = useState<Client>("claude-code");
  const [range, setRange] = useState<RangeId>("24h");
  const [gran, setGran] = useState<GranId>("hour");

  const [status, setStatus] = useState<RouterStatus | null>(null);
  const [health, setHealth] = useState<Record<string, RouteHealthView>>({});
  const [logs, setLogs] = useState<RequestLog[]>([]);
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
  /** 存着的调度偏好。右列调度按钮照着它写「按什么挑」。`null` = 还没读到。 */
  const [schedPrefs, setSchedPrefs] = useState<Prefs | null>(null);
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
  const cfgRipple = useRipple();
  /** 异步回来时判断「使用者还停在这个分页吗」。闭包里的 client 是点下去那一刻的。 */
  const clientRef = useRef(client);
  clientRef.current = client;

  const [routeDialog, setRouteDialog] = useState<{
    editing: Route | null;
  } | null>(null);
  const [schedOpen, setSchedOpen] = useState(false);
  const [logOpen, setLogOpen] = useState<Route | null>(null);
  const [auditOpen, setAuditOpen] = useState<Route | null>(null);

  const workspace = useWorkspace();
  const runningSessions = (workspace.data?.sessions ?? []).filter(
    (session) =>
      session.state === "running" &&
      session.context.client === client &&
      session.context.identity_kind === "relay" &&
      session.context.identity_id === `qb-router-${client}`,
  );
  const isRunning = runningSessions.length > 0;
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
    try {
      setStatus(await stationApi.routerStatus(client));
    } catch {
      // 状态读不到不该让整页报错 —— 右列会显示成「未启动」。
      setStatus(null);
    }
  }, [client]);

  // 换软件要重新问一次 —— 三个软件各有各的当前上游。不重问的话
  // 「正在路由」那一行会一直显示上一个软件的线，而那条线在这一页上根本不存在。
  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  /** 把后端回来的一份调度状态落到界面上。只动 state，立刻返回。 */
  const showSchedule = useCallback((s: Schedule | undefined) => {
    setSched(Boolean(s?.enabled));
    setSchedNote(s?.last_note ?? "");
    setSchedPrefs(s?.prefs ?? null);
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
        setSchedRows({});
      }
    },
    [client],
  );

  const refreshSched = useCallback(async () => {
    let mine: Schedule | undefined;
    try {
      mine = (await stationApi.schedules()).find((s) => s.client === client);
    } catch {
      // 读不到就当没开 —— ⛔ 但不要顺手去关它。读不到和关着是两回事，
      // 而「顺手关掉」会在一次 IPC 抖动后把使用者的调度悄悄停掉。
      mine = undefined;
    }
    showSchedule(mine);
    await loadRows(mine);
  }, [client, loadRows, showSchedule]);

  useEffect(() => {
    void refreshSched();
  }, [refreshSched]);

  // 调度开着时每 30 秒把「上一轮」和排队中的那条读回来一次。不读的话，
  // 卡上那句「上一轮」停在打开页面那一刻，而循环每分钟都在往下跑。
  //
  // ⛔ 只读本机的两样东西：库里的调度状态、路由器内存里的选择。**不调 decide** ——
  // 它会给每条线现拉一轮站点账单，放进定时器等于替驻留循环加跳，
  // 而 CLAUDE.md 写着一跳 60 秒、别调小。
  useEffect(() => {
    if (!sched) return;
    const timer = window.setInterval(() => {
      if (document.visibilityState !== "visible") return;
      void stationApi
        .schedules()
        .then((all) => {
          const s = all.find((x) => x.client === client);
          if (!s) return;
          setSched(s.enabled);
          setSchedNote(s.last_note);
          setSchedPrefs(s.prefs);
          if (!s.enabled) setSchedRows({});
        })
        .catch(() => undefined);
      void refreshStatus();
    }, 30_000);
    return () => window.clearInterval(timer);
  }, [sched, client, refreshStatus]);

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

  const since = Date.now() - rangeOf(range).hours * 3600_000;
  /** 本段之内、属于这个软件的日志。 */
  const mineLogs = useMemo(() => {
    const ids = new Set(mine.map((r) => r.id));
    return logs.filter((l) => ids.has(l.route_id) && Number(l.at_ms) >= since);
  }, [logs, mine, since]);

  const logsOf = useCallback(
    (id: string) => mineLogs.filter((l) => l.route_id === id),
    [mineLogs],
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
  const up = Boolean(status?.running && isRunning);

  const act = async (fn: () => Promise<RouterStatus>, what: string) => {
    setBusy(true);
    setLaunchError(null);
    try {
      setStatus(await fn());
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
    setBusy(true);
    setToggling(true);
    setLaunchError(null);
    try {
      const next = await stationApi.scheduleSet(client, !sched);
      showSchedule(next);
      if (next.enabled) {
        setIgnite(true);
        window.clearTimeout(igniteTimer.current);
        igniteTimer.current = window.setTimeout(() => setIgnite(false), 1200);
      }
      // 名次随后到，不让按钮陪着转圈等账单。
      void loadRows(next);
    } catch (e) {
      toast.error(
        `${sched ? "停" : "起"}调度失败：${e instanceof Error ? e.message : String(e)}`,
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
   * 右列「启动」：**只是把这个软件打开**，走它现在选中的那条上游。
   *
   * ⛔ 不停调度、也不改上游 —— 所以**不传 routeId**。传了的话后端会 `set_now`，
   * 而 `set_now` 会把排队中的切换清掉：刚点的那一行、调度刚排上的那条，
   * 都会被悄悄换回旧的。「指定这一条再启动」是行上那个按钮，见 `launchFrom`。
   *
   * ⛔ **本机路由只有「启动」会拉起来**（右列和行上两个入口，后端都是
   * `station_launch`）。切上游、拉路由、起客户端进程是后端里的一个动作，
   * 不在这里拆成三次调用 —— 拆开的话中间失败会留下「路由起了、客户端
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

  /**
   * 行上的「启动」：**指定这一条再启动**。
   *
   * 调度开着时先把调度停掉 —— 不停的话下一跳会立刻把它换走，而使用者以为
   * 自己选的那条生效了（定稿 §2.2 #4）。停了就在提示里照实写出三步：
   * 只说「已启动」的话，调度等于是被悄悄关掉的。⛔ 停在后端，不是只改这里的 state。
   */
  const launchFrom = async (r: Route) => {
    if (launching || stopping) return;
    const who = client;
    const stoppedSchedule = sched;
    if (stoppedSchedule) {
      try {
        showSchedule(await stationApi.scheduleSet(who, false));
      } catch {
        // 停不掉就别硬起 —— 起了也会被下一跳换走。
        toast.error("调度停不下来，先别启动：下一跳会把你选的这条换走");
        return;
      }
    }
    // 从行上点的也让右列那块跟着动 —— 启动在哪儿点的，进度都看同一个地方。
    setBusy(true);
    setLaunching(who);
    setLaunchFx(null);
    setLaunchError(null);
    try {
      await stationApi.launch(who, r.id);
      await workspace.refresh();
      if (clientRef.current === who)
        setStatus(await stationApi.routerStatus(who));
      if (stoppedSchedule) toast.info("已停止智能调度，当前使用手动选择的线路");
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
  // 检查身份与门禁……），工作区数据跟着刷新 —— 启动块上显示的就是这一句，
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
  // 请求数与平均耗时来自本机路由自己的日志，跟着时间范围走；
  // Token 与消费来自站点账单，而账单只有一个固定 24 小时窗 ——
  // 选别的档时这两格写「—」。拿 24h 的数去冒充「近 1 小时」，
  // 或者缺数据时填个 0，都是在编。
  const req = mineLogs.length;
  const totals = mineLogs
    .map((l) => (l.total_ms == null ? null : Number(l.total_ms)))
    .filter((v): v is number => v != null);
  const avgMs = totals.length
    ? totals.reduce((a, b) => a + b, 0) / totals.length
    : null;

  /**
   * 把这个软件底下各条线的账单数加起来。
   *
   * 一条都没取证到就返回 `null`（显示「—」），**不是 0** ——
   * 0 是「量过了，就是零」这个断言。
   */
  const sumHealth = (
    pick: (h: RouteHealthView) => number | null | undefined,
  ) =>
    range === "24h"
      ? mine.reduce<number | null>((sum, r) => {
          const h = health[r.id];
          const v = h == null ? null : pick(h);
          return v == null ? sum : (sum ?? 0) + v;
        }, null)
      : null;

  const cost = sumHealth((h) => h.cost_24h);
  const tokens = sumHealth((h) => h.tokens_24h);
  // 四类分开给 —— 合成一个总数会把「输出翻五倍」这种结构藏掉。
  const tokIn = sumHealth((h) => h.input_tokens);
  const tokCacheRead = sumHealth((h) => h.cache_read_tokens);
  const tokCacheWrite = sumHealth((h) => h.cache_write_tokens);
  const tokOut = sumHealth((h) => h.output_tokens);

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
            : !selected
              ? [sched ? "调度还没选出线路" : "先在下面点一行选上游"]
              : [
                  `走「${selectedGroup}」`,
                  ...(client === "claude-desktop"
                    ? ["开着的桌面端会先被关掉"]
                    : []),
                ];
  const schedWhat = schedPrefs
    ? [axesText(schedPrefs.axes), floorsText(schedPrefs.floors)]
        .filter(Boolean)
        .join(" · ")
    : "按当前口径自动挑上游";

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
                  倍率 <RateCell route={selected} />
                </span>
              </>
            ) : (
              <span className="name dim">还没选上游 —— 点下面任意一行即可</span>
            )}
          </div>

          <div className="qb-st-kpis">
            <Kpi
              k="总请求数"
              v={req ? req.toLocaleString() : "—"}
              s={`${rangeOf(range).name}内`}
            />
            <Kpi
              k="总 Token"
              v={tokens == null ? "—" : mtok(tokens)}
              s={
                range !== "24h"
                  ? "账单只有 24 小时窗，这一档没有数"
                  : tokens == null
                    ? "站点账单里没有 token 字段"
                    : tokenMix(tokIn, tokCacheRead, tokCacheWrite, tokOut)
              }
            />
            <Kpi
              k="总消费"
              v={cost == null ? "—" : cost.toFixed(4)}
              s={
                range === "24h"
                  ? "站点账单报的实扣，逐条相加"
                  : "账单只有 24 小时窗，这一档没有数"
              }
            />
            <Kpi
              k="平均耗时"
              v={avgMs == null ? "—" : `${(avgMs / 1000).toFixed(2)} s`}
              s={
                totals.length
                  ? `取到耗时的 ${totals.length} 条`
                  : "本段没有请求"
              }
            />
          </div>

          <div className="qb-st-rangebar">
            <label htmlFor="qb-st-range">时间范围</label>
            <select
              id="qb-st-range"
              value={range}
              onChange={(e) => setRange(e.target.value as RangeId)}
            >
              {RANGES.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.name}
                </option>
              ))}
            </select>
            <span style={{ flex: 1 }} />
            <label htmlFor="qb-st-gran">粒度</label>
            <select
              id="qb-st-gran"
              value={gran}
              onChange={(e) => setGran(e.target.value as GranId)}
            >
              <option value="hour">按小时</option>
              <option value="day">按天</option>
            </select>
          </div>

          <Chart rows={mineLogs} range={range} gran={gran} />
          <div className="qb-st-legend">
            <span>
              {rangeOf(range).name} ·{" "}
              {gran === "hour" ? "每小时一格" : "每天一格"}
            </span>
            <span style={{ flex: 1 }} />
            <span>
              <i style={{ background: "var(--accent)" }} />
              请求量
            </span>
            <span>
              <i style={{ background: "var(--warn)" }} />
              首字 P95
            </span>
            <span>各按自己的量程</span>
          </div>
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
                : selected
                  ? `启动 ${clientName.name}，走「${selectedGroup}」；顺带把本机路由拉起来，并把它的 base_url 指过去${
                      client === "claude-desktop"
                        ? "。桌面端开着的话先替你关掉它（单实例，不关换不了配置目录）"
                        : ""
                    }${sched ? "。智能调度不受影响，照常挑线" : ""}`
                  : sched
                    ? "智能调度还没排出可用的线路 —— 打开调度设置看赛道"
                    : "先点下面任意一行选上游"
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
            {/* 每句单独一行写死 —— 拼成一句的话会在「调度接 / 着挑」中间折断。 */}
            {launchLines.map((line) => (
              <span key={line} className="sub">
                {line}
              </span>
            ))}
            {launchState === "launching" && (
              <span className="qb-st-progress" aria-hidden="true">
                <i />
              </span>
            )}
          </button>

          {/* 智能调度：点一下启动，再点一下停止。⛔ 不许改成「打开设置」——
              设置是下面单独那个按钮。开着时整块转绿，一道光沿着边一直绕、
              状态点一圈圈往外扩，告诉人它是真的在跑。 */}
          <button
            type="button"
            className="qb-st-schedbtn"
            data-fx={toggling ? "busy" : ignite ? "ignite" : undefined}
            aria-label="智能调度"
            aria-pressed={sched}
            aria-describedby={schedDescId}
            aria-busy={toggling || undefined}
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
              {sched ? "智能调度运行中" : "启动智能调度"}
            </span>
            <span id={schedDescId} className="desc">
              <span className="sub">{schedWhat}</span>
              {sched && (
                <span className="sub last" title={schedNote || undefined}>
                  {schedNote
                    ? `上一轮：${schedNote}`
                    : "每分钟排一次 · 迟滞 8%"}
                </span>
              )}
              {/* ⛔ 「面板关掉它就停」必须写在这里。不写的话使用者会以为
                  关了面板还有人替他挑线路 —— 而实际上客户端会一直用
                  最后选中的那条，可能是几天前那条已经涨价的。 */}
              <span className="foot">
                {sched ? "面板关掉即停 · 点击停止" : "面板开着才生效"}
              </span>
            </span>
          </button>

          <button
            type="button"
            className="qb-st-schedcfg"
            onPointerDown={cfgRipple.onPointerDown}
            onClick={() => setSchedOpen(true)}
          >
            <span className="qb-st-sheen" aria-hidden="true" />
            {cfgRipple.layer}
            <SlidersHorizontal size={14} aria-hidden="true" />
            调度设置
            <ChevronRight size={14} className="chev" aria-hidden="true" />
          </button>
        </div>
      </div>

      {/* ------------------------------------------------ 站点 → 账户行 */}
      {sites.length === 0 ? (
        <div className="qb-st-empty">
          <p>
            <b>{clientName.name}</b> 底下还没有中转站账户。
            <br />
            账户按软件分开 —— 在别的软件下加过的，这里看不到。
            站点（域名）是共用的，加账户时直接选。
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
                {sched && (
                  <span className="qb-st-pill qb-st-pill--ok">调度中</span>
                )}
                <span style={{ flex: 1 }} />
                {/* 余额是站点级的 —— 整站共享，画在这一层。 */}
                <span className="bal">余额（整站共享）站点未提供</span>
              </div>
              {s.lines.map((r) => (
                <RouteRow
                  key={r.id}
                  route={r}
                  health={health[r.id]}
                  logs={logsOf(r.id)}
                  range={range}
                  current={selectedId === r.id}
                  running={isRunning}
                  onClose={() => void closeNow()}
                  queued={queued && selectedId === r.id}
                  sched={sched}
                  lane={schedRows[r.id]}
                  siteName={siteName}
                  onPick={() => {
                    if (sched) {
                      toast.error(
                        "要自己选就先停调度 —— 不停的话下一跳会把它换走。",
                      );
                      return;
                    }
                    void act(
                      () => stationApi.selectRoute(r.id, false),
                      "切换上游",
                    );
                  }}
                  onLaunch={() => void launchFrom(r)}
                  onAudit={() => setAuditOpen(r)}
                  onLogs={() => setLogOpen(r)}
                  onEdit={() => setRouteDialog({ editing: r })}
                  onRemove={() => {
                    // 失败要落到提示条上。不接住的话是一个未捕获的 rejection ——
                    // 界面一动不动，只有控制台里有话说，看起来像「按钮没反应」。
                    void stationApi
                      .removeRoute(r.id)
                      .then(() => {
                        toast.ok(`已移除「${r.group || "默认分组"}」`);
                        onChanged();
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
        open={schedOpen}
        onClose={() => {
          setSchedOpen(false);
          // 弹窗里「用这一条」会排队换线、改口径会改名次 —— 关掉时读回来一次，
          // 不然「正在路由」、行上的高亮和「最弱项」还停在打开弹窗之前。
          void refreshStatus();
          void refreshSched();
        }}
        title={`调度设置 · ${clientName.name}`}
        size="form"
        footer={
          <>
            <span className="qb-st-note" style={{ flex: 1 }}>
              换上游会让上游那边的 prompt 缓存作废，所以挑战者要领先现任 8%
              才换。调度<b>只在面板开着时</b>生效。
            </span>
            <Button variant="ghost" onClick={() => setSchedOpen(false)}>
              关闭
            </Button>
            {/* ⛔ 启停也放一份在这里：弹窗开着的时候右列那个按钮被遮住了，
                改完口径要关掉弹窗才能启动，那一步没有任何理由。 */}
            <Button
              variant={sched ? "ghost" : "primary"}
              disabled={busy}
              onClick={() => void toggleSched()}
            >
              {sched ? "停止调度" : "按这套启动调度"}
            </Button>
          </>
        }
      >
        {/* 每存一次偏好就把卡上的口径换掉 —— 不等弹窗关掉。 */}
        <StationSchedule
          client={client}
          onSaved={(s) => setSchedPrefs(s.prefs)}
        />
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
        title={`站点检验 · ${auditOpen ? auditOpen.group || "默认分组" : ""}`}
        size="wide"
      >
        {auditOpen && (
          // focus 一定要给：行上已经选过这条线了，弹窗里不该再问一遍。
          <StationAudit routes={[auditOpen]} focus={auditOpen.id} />
        )}
      </Modal>
    </>
  );
}

// ------------------------------------------------------------------ 账户行

/**
 * 一条账户行。**整行可点（切上游），没有小圆点。**
 *
 * ⛔ 行必须是 `<div role="radio">`，**不能是 `<button>`** —— `<button>` 里套
 * `<button>` 会被 HTML 解析器提前闭合外层，名字、动作、读数格会散成三段。
 * 行内的按钮各自 `stopPropagation`，否则点「移除」会顺带把上游也切过去。
 */
function RouteRow({
  route,
  health,
  logs,
  range,
  current,
  running,
  onClose,
  queued,
  sched,
  lane,
  siteName: _siteName,
  onPick,
  onLaunch,
  onAudit,
  onLogs,
  onEdit,
  onRemove,
}: {
  route: Route;
  health?: RouteHealthView;
  logs: RequestLog[];
  range: RangeId;
  /** 这一条是选中的上游（排队中的也算）。 */
  current: boolean;
  running: boolean;
  onClose: () => void;
  /** 选中了但还在排队，下一发请求才生效。 */
  queued: boolean;
  sched: boolean;
  /** 调度开着时这条线的名次。`undefined` = 没开，或者它没参与排序。 */
  lane?: { weakest: Axis | null; score: number | null };
  siteName: (id: string) => string;
  onPick: () => void;
  onLaunch: () => void;
  onAudit: () => void;
  onLogs: () => void;
  onEdit: () => void;
  onRemove: () => void;
}) {
  const stop = (fn: () => void) => (e: React.MouseEvent) => {
    e.stopPropagation();
    fn();
  };
  const fold = route.rates?.completion_ratio;
  return (
    <div
      className={`qb-st-row${current ? " on" : ""}`}
      role="radio"
      tabIndex={0}
      aria-checked={current}
      title={
        current
          ? queued
            ? "已排队，下一次请求生效"
            : "已是当前上游"
          : "切到这条上游（下一次请求生效）"
      }
      onClick={onPick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onPick();
        }
      }}
    >
      <div className="qb-st-rowtop">
        <span className="gname">{route.group || "默认分组"}</span>
        {fold != null && Number.isFinite(fold) && fold > 1 && (
          <span
            className="qb-st-pill qb-st-pill--warn"
            title="输出按输入的若干倍计费"
          >
            翻倍 ×{fold}
          </span>
        )}
        {!route.credential_id && (
          <span className="qb-st-pill qb-st-pill--unknown">没绑令牌</span>
        )}
        <AuditPill route={route} />
        {current && !sched && (
          <span className="qb-st-pill qb-st-pill--accent">
            {queued ? "已排队 · 下一次请求生效" : "当前上游"}
          </span>
        )}
        {/* §2.6：选中那行写「调度选中 · 最弱项 .47」，其余行各写各的最弱项。
            ⛔ 算不出名次的**不写 0 分** —— 那是断言「它很差」，
            而实际上是勾中的维度里有一项没有证据。 */}
        {sched && (current || lane) && (
          <span
            className={`qb-st-pill ${current ? "qb-st-pill--ok" : "qb-st-pill--unknown"}`}
            title={
              lane?.weakest
                ? `这条线的总分由「${AXIS_LABEL[lane.weakest]}」这一维决定`
                : "勾中的维度里有一项这条线没有证据，排不出名次"
            }
          >
            {current ? "调度选中" : ""}
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
          <Button
            variant="primary"
            onClick={stop(running && current ? onClose : onLaunch)}
          >
            {running ? (current ? "一键关闭" : "切换并使用") : "启动"}
          </Button>
          <Button variant="ghost" onClick={stop(onAudit)}>
            检验
          </Button>
          <Button
            variant="ghost"
            onClick={stop(onLogs)}
            title={logs.length ? "请求日志" : "本段没有请求记录"}
          >
            ▤
          </Button>
          <Button variant="ghost" onClick={stop(onEdit)} title="编辑">
            ✎
          </Button>
          <Button variant="ghost" onClick={stop(onRemove)} title="移除">
            🗑
          </Button>
        </span>
      </div>

      {/* ⛔ 用间距分组，竖线只是装饰 —— 靠竖线分格撑布局的话，一换行就断成两截。 */}
      <div className="qb-st-quick">
        <Cell k="倍率">
          <RateCell route={route} />
        </Cell>
        <Cell k="成功率">{pct(health?.success_rate)}</Cell>
        <Cell k="缓存命中">{pct(health?.cache_hit_rate)}</Cell>
        <Cell k="首字 P95">
          {health?.ttft_p95_ms == null
            ? dash
            : `${(health.ttft_p95_ms / 1000).toFixed(2)} s`}
        </Cell>
        <Cell k="本段请求">
          {logs.length ? logs.length.toLocaleString() : dash}
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
 */
function RateCell({ route }: { route: Route }) {
  const cell = rateCell(route);
  if (cell.kind === "unverified")
    return (
      <>
        {cell.nominal == null ? dash : `×${cell.nominal.toFixed(2)}`}{" "}
        <span className="qb-st-dim">标称 · 未核实</span>
      </>
    );
  if (cell.kind === "verified")
    return (
      <>
        ×{cell.rate.toFixed(2)}{" "}
        <span className="qb-st-pill qb-st-pill--ok">已核实</span>
      </>
    );
  return (
    <>
      <span className="qb-st-strike">×{cell.nominal.toFixed(2)}</span>{" "}
      <b style={{ color: "var(--danger)" }}>×{cell.real.toFixed(2)}</b>{" "}
      <span className="qb-st-pill qb-st-pill--danger">实测</span>
    </>
  );
}
