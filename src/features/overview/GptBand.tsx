/** Codex desktop: official login slots, explicit switching, local rollout usage. */
import { useCallback, useEffect, useRef, useState } from "react";
import {
  Gauge,
  MonitorSmartphone,
  Plus,
  RefreshCw,
  Square,
  Trash2,
  Users,
  Wine,
} from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Link, useNavigate } from "react-router-dom";
import { api } from "../../lib/api";
import { CODEX_R, codexApi } from "../../lib/codexAccounts";
import type { CodexSlot } from "../../lib/generated/CodexSlot";
import type { CodexUsage } from "../../lib/generated/CodexUsage";
import { AFTER, R } from "../../lib/resources";
import { stationApi } from "../../lib/station";
import { invalidate, useResource, useSession } from "../../lib/store";
import { slotName } from "../../lib/slotName";
import { endTask, resetTask, useTask, type TaskState } from "../../lib/tasks";
import StationTurnState from "../station/StationTurnState";
import BridgeSettings, { BridgeSettingsCorner } from "../tavern/BridgeSettings";
import CodexQuotaBars, { type CodexQuotaState } from "./CodexQuotaBars";
import Tile from "./Tile";
import { Button, Card, ConfirmDialog, Modal, Pill, useToast } from "../../ui";

const PER_PAGE = 4;
const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));
const short = (n: number) =>
  new Intl.NumberFormat("zh-CN", {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(n);

export default function GptBand() {
  const accounts = useResource("codexAccounts", CODEX_R.accounts);
  const desktop = useResource("codexDesktop", CODEX_R.desktop);
  const toast = useToast();
  const navigate = useNavigate();
  // 酒馆（GPT 后端）：跟 Claude 页那块贴同一套状态。插件是同一个，后端不同。
  const plugins = useResource("plugins", R.plugins);
  const tavern = plugins.data?.find((p) => p.id === "sillytavern");
  const tavernUnready = tavern?.state === "missing";
  const tavernTask = useTask("tavern-start");
  const [gptBridge, setGptBridge] = useState<{
    running: boolean;
    detail: string;
  } | null>(null);
  const refreshGptBridge = useCallback(async () => {
    try {
      const st = await api.tavernGptStatus();
      setGptBridge({ running: st.running, detail: st.detail });
    } catch {
      // 演示模式或后端没起时读不到；保持上一次的值。
    }
  }, []);
  useEffect(() => {
    void refreshGptBridge();
    const timer = window.setInterval(() => void refreshGptBridge(), 20_000);
    return () => window.clearInterval(timer);
  }, [refreshGptBridge]);
  const [tavernBusy, setTavernBusy] = useState(false);
  const [bridgeOpen, setBridgeOpen] = useState(false);
  async function launchTavern() {
    setTavernBusy(true);
    resetTask("tavern-start");
    try {
      const url = await api.pluginStart("gpt");
      endTask("tavern-start");
      invalidate(...AFTER.tavern);
      void refreshGptBridge();
      // 无论是新起的还是复用现有服务都要打开页面 —— 少了这一步，
      // 成功的启动和崩溃看起来一模一样（跟 Claude 页那块贴同一条理由）。
      try {
        if (url.startsWith("http")) await openUrl(url);
        toast.ok("酒馆已就绪（GPT 桥接），已打开页面");
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        toast.error(
          `酒馆已就绪，但页面没打开：${msg}。可以在浏览器里手动打开 ${url}`,
        );
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask("tavern-start", msg);
      toast.error(msg);
    } finally {
      setTavernBusy(false);
    }
  }
  const [pageRaw, setPage] = useSession("home.codex.page", -1);
  const [adding, setAdding] = useState(false);
  const [label, setLabel] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [ask, setAsk] = useState<
    | {
        id: string;
        label: string;
        action: "switch" | "launch" | "archive";
        /** 点的时候实测到的桌面端进程数。0 = 没在跑（那时候 launch 压根不弹框）。 */
        running?: number;
      }
    | { action: "close" }
    | null
  >(null);
  const launchTask = useTask("launch-codex");
  const [closing, setClosing] = useState(false);
  const [closeErr, setCloseErr] = useState("");
  const slots = accounts.data?.slots ?? [];
  const active = slots.find((s) => s.active);
  const ownedProcess = desktop.data?.processes.some(
    (p) =>
      p.pid === accounts.data?.launched_pid &&
      p.started === accounts.data?.launched_at,
  );
  const runningSlot = ownedProcess
    ? slots.find((s) => s.id === accounts.data?.launched_id)
    : undefined;
  // 2026-09-23：起槽位 / 切槽位之前**只关面板起的**那份（资料目录在面板目录下的）。
  // 开始菜单、`codex://` 链接、别的多开工具起的默认实例不动 —— 原来一律全关，
  // 起它的那个程序再把它拉起来，看到的就是「突然又弹出一个 Codex 窗口」。
  const oursRunning = desktop.data?.processes.some((p) => p.ours) ?? false;
  const othersRunning =
    desktop.data?.processes.filter((p) => !p.ours).length ?? 0;
  const quotaRun = useRef(0);
  const [slotQuotas, setSlotQuotas] = useState<Record<string, CodexQuotaState>>(
    {},
  );
  const slotKey = slots.map((slot) => slot.id).join("\u0000");
  /**
   * 每个槽位的本机快照 + 「最近一次」联网结果。**不联网。**
   *
   * ⛔ 2026-09-23 使用者定的：联网额度只手动刷新。原来这里挂载时对所有槽位各问一次
   * `wham/usage`、之后每 5 分钟再问一轮（窗口缩到托盘也照问）。现在挂载、槽位增减、
   * 账户卡 / 用量卡刷新都只走这条；联网只在某一行的刷新图标（[`refreshSlotQuota`]）。
   */
  const loadSlotQuotas = useCallback(async () => {
    const ids = slotKey ? slotKey.split("\u0000") : [];
    const run = ++quotaRun.current;
    if (!ids.length) {
      setSlotQuotas({});
      return;
    }
    const results = await Promise.all(
      ids.map(async (id) => {
        const [local, live] = await Promise.allSettled([
          codexApi.rateLimits(id),
          codexApi.quota(id, false),
        ]);
        return [
          id,
          {
            local: local.status === "fulfilled" ? local.value : null,
            live: live.status === "fulfilled" ? live.value : null,
            error:
              local.status === "rejected"
                ? errorText(local.reason)
                : live.status === "rejected"
                  ? errorText(live.reason)
                  : "",
            loading: false,
          },
        ] as const;
      }),
    );
    if (run !== quotaRun.current) return;
    setSlotQuotas(Object.fromEntries(results));
  }, [slotKey]);
  useEffect(() => {
    void loadSlotQuotas();
  }, [loadSlotQuotas]);
  /** 点某一行的刷新图标：**只联网问这一个槽位**，顺带重读它的本机快照。 */
  const refreshSlotQuota = useCallback(
    async (id: string) => {
      setSlotQuotas((prev) => ({
        ...prev,
        [id]: {
          local: prev[id]?.local ?? null,
          live: prev[id]?.live ?? null,
          error: "",
          loading: true,
        },
      }));
      const [local, live] = await Promise.allSettled([
        codexApi.rateLimits(id),
        codexApi.quota(id, true),
      ]);
      if (live.status === "rejected") toast.error(errorText(live.reason));
      setSlotQuotas((prev) => ({
        ...prev,
        [id]: {
          local:
            local.status === "fulfilled"
              ? local.value
              : (prev[id]?.local ?? null),
          // 这次没问到就留着上一次问到的 —— 界面上时刻照实，不会被当成此刻。
          live:
            live.status === "fulfilled" ? live.value : (prev[id]?.live ?? null),
          error:
            live.status === "rejected"
              ? errorText(live.reason)
              : local.status === "rejected"
                ? errorText(local.reason)
                : "",
          loading: false,
        },
      }));
    },
    [toast],
  );
  const pages = Math.max(1, Math.ceil(slots.length / PER_PAGE));
  const autoPage = Math.floor(
    Math.max(
      0,
      slots.findIndex((s) => s.active),
    ) / PER_PAGE,
  );
  const page = Math.min(pageRaw < 0 ? autoPage : pageRaw, pages - 1);
  // 确认框里那个槽位怎么称呼：「邮箱 - 命名」。槽位可能已经不在列表里（刚新建），
  // 那就退回 `ask.label`。
  const askShown =
    ask && ask.action !== "close"
      ? slotName(slots.find((s) => s.id === ask.id)?.email, ask.label)
      : "";
  // 「一键关闭」那格没有后端进度事件，用本地状态凑一个 TaskState：
  // 失败原文要留在贴上（toast 会自己消失）。
  const closeTask: TaskState = {
    running: closing,
    phase: "正在关闭…",
    step: 0,
    total: 0,
    log: [],
    error: closeErr || undefined,
    finished: false,
  };

  async function reload() {
    await accounts.refresh();
    await desktop.refresh();
    await loadSlotQuotas();
  }
  async function create() {
    setBusy(true);
    setError("");
    try {
      const id = await codexApi.create(label);
      await accounts.refresh();
      setAdding(false);
      setPage(Math.floor(slots.length / PER_PAGE));
      // 新建完直接去起（里面自己判断要不要弹框）。
      setLabel("");
      void requestLaunch({ id, label: label.trim() } as CodexSlot);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  }
  async function confirm() {
    if (!ask) return;
    if (ask.action === "close") {
      setClosing(true);
      setCloseErr("");
    }
    setBusy(true);
    setError("");
    try {
      if (ask.action === "close") await codexApi.close();
      else if (ask.action === "launch") {
        // 确认过了，交给同一条启动路径 —— 别在这里再写一份。
        setBusy(false);
        await runLaunch(ask.id);
        return;
      } else await codexApi[ask.action](ask.id);
      await reload();
      toast.ok(
        ask.action === "close"
          ? "Codex 桌面端已关闭，账户与登录资料已保留"
          : ask.action === "switch"
            ? "账户已切换，点击右侧启动"
            : "槽位已移除，登录资料保留在本机归档中",
      );
      setAsk(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      if (ask.action === "close")
        setCloseErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      setClosing(false);
    }
  }

  /**
   * 点「打开 / 启动」。**先查桌面端有没有在跑，真的有才弹确认框**（0.27.0）。
   *
   * 起 Codex 桌面端会先把开着的关掉（`usecase::codex_accounts::launch` 里就是这么做的），
   * 所以确认一次是对的；可什么都没在跑的时候那个框就是纯噪音 ——
   * 使用者的原话是「每次点启动都会跳这个」。
   *
   * ⛔ 查不出来（探测失败）一律当作**可能在跑**照弹。把「不知道」降级成「没有」
   * 正是 §7.17 那条坑的形状。这里用的是 `codex_desktop_status`：它按官方 Store 包的
   * exe 路径认进程，不按名字 —— 跟启动链自己那套证据同源。
   *
   * 只数**面板起的**（`ours`）：别处起的那份不会被关，就不该为它弹框。
   */
  async function requestLaunch(slot: CodexSlot) {
    setError("");
    let running = 1;
    try {
      const d = await codexApi.desktop();
      running = d.processes.filter((p) => p.ours).length;
    } catch {
      running = 1;
    }
    if (running > 0) {
      setAsk({ id: slot.id, label: slot.label, action: "launch", running });
      return;
    }
    await runLaunch(slot.id);
  }

  /** 真正去起。确认框走这里，没东西在跑时也走这里（跳过确认框）。 */
  async function runLaunch(id: string) {
    setBusy(true);
    setError("");
    resetTask("launch-codex");
    try {
      await codexApi.launch(id);
      endTask("launch-codex");
      await reload();
      toast.ok("Codex 桌面端已打开，请在官方窗口完成登录");
      setAsk(null);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask("launch-codex", msg);
      setError(msg);
    } finally {
      setBusy(false);
    }
  }

  const open = (slot: CodexSlot, action: "switch" | "archive") => {
    setError("");
    setAsk({ id: slot.id, label: slot.label, action });
  };
  // 官方 Codex 的 turn-state「识别」（实验，默认关，只对 Codex）。
  // 开关 / 规则 / 接管了哪个槽位都从后端路由状态回读，不本地臆测 —— 见 StationTurnState。
  // 「识别开着」看的是 `turnstate_takeover`（落盘 marker），不是内存里的 armed 位。
  const [ts, setTs] = useState<{
    enabled: boolean;
    team: boolean;
    takeover: string | null;
  }>({ enabled: false, team: false, takeover: null });
  const [tsOpen, setTsOpen] = useState(false);
  const refreshTs = useCallback(async () => {
    try {
      const rs = await stationApi.routerStatus("codex");
      setTs({
        enabled: rs.turnstate_enabled,
        team: rs.turnstate_team,
        takeover: rs.turnstate_takeover,
      });
    } catch {
      // 路由没起时读不到；保持上一次的值，不让整块报错。
    }
  }, []);
  useEffect(() => {
    void refreshTs();
    const timer = window.setInterval(() => void refreshTs(), 5000);
    return () => window.clearInterval(timer);
  }, [refreshTs]);
  // Codex 打包应用更新后需管理员重新注册，否则启动一律「拒绝访问 (os error 5)」。
  // 命中这类报错时给一个一键修复（会弹 UAC）。
  const needsReg = /os error 5|拒绝访问|RegisterByFamilyName/i.test(error);
  const [repairing, setRepairing] = useState(false);
  async function repairRegistration() {
    setRepairing(true);
    setError("");
    try {
      await codexApi.repairRegistration();
      await reload();
      toast.ok("已重新注册 Codex，请重试启动");
      setAsk(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setRepairing(false);
    }
  }

  return (
    <>
      <div className="account-workspace" data-testid="codex-accounts">
        <Card
          className="account-slots"
          title={
            <>
              <Users size={14} />
              账户槽位
            </>
          }
          actions={
            <div className="flex gap-1.5">
              <Button
                size="sm"
                icon={<RefreshCw size={12} />}
                aria-label="刷新 Codex 账户"
                loading={accounts.loading}
                onClick={() =>
                  void reload().catch((e) => toast.error(String(e)))
                }
              />
              <Button
                size="sm"
                icon={<Plus size={12} />}
                onClick={() => {
                  setError("");
                  setAdding(true);
                }}
              >
                新建
              </Button>
            </div>
          }
        >
          {accounts.error && (
            <p role="alert" className="notice notice--danger">
              {accounts.error}
            </p>
          )}
          {!slots.length && (
            <div className="py-4">
              <h3>登录你的 Codex 账户</h3>
              <p className="notice mt-2">
                新建槽位后，在 Codex 桌面端选择「使用 ChatGPT
                登录」。每个槽位单独保存登录状态。
              </p>
              <p className="notice mt-2">
                现有 Codex 的默认账户和会话保留原处。
              </p>
            </div>
          )}
          <div className="account-slot-list">
            {slots.slice(page * PER_PAGE, (page + 1) * PER_PAGE).map((s) => (
              <div
                key={s.id}
                className={"slotrow" + (s.active ? " slotrow--active" : "")}
                data-testid="codex-slot"
              >
                <div className="slotrow-main">
                  {/* 「邮箱 - 命名」（0.27.0）。这一格 nowrap + 省略号，
                      所以 title 上挂完整的那一串。 */}
                  <strong
                    className="slotrow-label"
                    title={slotName(s.email, s.label)}
                  >
                    {slotName(s.email, s.label)}
                  </strong>
                  <span className="slotrow-side">
                    {s.active ? (
                      <Pill tone="accent">当前槽位</Pill>
                    ) : (
                      <Button
                        size="sm"
                        disabled={busy}
                        onClick={() => open(s, "switch")}
                      >
                        切换
                      </Button>
                    )}
                    {/* 2026-09-23 使用者要求去掉「打开」：起桌面端走右边那块磁贴。
                        没登录的槽位留着「登录」—— 不然新建之后没登完，就只能先切过去再去磁贴登。 */}
                    {!s.logged_in && (
                      <Button
                        size="sm"
                        disabled={busy || !desktop.data?.executable}
                        onClick={() => void requestLaunch(s)}
                      >
                        登录
                      </Button>
                    )}
                    <Button
                      size="sm"
                      aria-label={`移除 Codex ${slotName(s.email, s.label)}`}
                      icon={<Trash2 size={12} />}
                      disabled={busy || s.active}
                      onClick={() => open(s, "archive")}
                    />
                  </span>
                </div>
                <p
                  className="notice codex-slot-identity"
                  title={`${s.email ?? "尚未读取到账户邮箱"}${s.plan ? ` · ${s.plan}` : ""}`}
                >
                  {s.email ?? "尚未读取到账户邮箱"}
                  {s.plan ? ` · ${s.plan}` : ""}
                </p>
                {/* 登录正常时不画那句「已登录 · 本地凭据」（2026-09-23 使用者删的）；
                    没登录 / 登录文件损坏 / API 模式这几种要照旧说出来，那是下一步该做什么。 */}
                {/* 「登录已失效」（点刷新时服务端不认了，`usecase::login_health`）标红：
                    令牌还躺在本机，可登录已经没了 —— 跟「从没登过」是两件事。 */}
                {!s.logged_in && (
                  <span
                    className={
                      "notice" +
                      (s.auth_state.startsWith("登录已失效")
                        ? " notice--danger"
                        : "")
                    }
                  >
                    {s.auth_state}
                  </span>
                )}
                <CodexQuotaBars
                  state={slotQuotas[s.id]}
                  canRefresh={s.logged_in}
                  onRefresh={() => void refreshSlotQuota(s.id)}
                />
              </div>
            ))}
          </div>
          <div className="account-pagination">
            <span className="notice">{slots.length} 个槽位 · 每页 4 个</span>
            {pages > 1 && (
              <span className="pager ml-auto">
                {Array.from({ length: pages }, (_, i) => (
                  <button
                    key={i}
                    className="pager-btn"
                    type="button"
                    aria-label={`Codex 第 ${i + 1} 页`}
                    aria-current={page === i}
                    onClick={() => setPage(i)}
                  >
                    {i + 1}
                  </button>
                ))}
              </span>
            )}
          </div>
        </Card>
        <div className="account-workspace-right">
          <Card
            className="account-launch"
            title="Codex 桌面端"
            actions={
              <div className="flex items-center gap-1.5">
                {/* 官方 Codex 识别（turn-state）的入口：放在卡片标题栏，不占纵向空间 ——
                    这一页是固定高度（test:ui 钉着 900×740 不许裁切），卡片里再加一行就裁。
                    开关、个人/Team、识别模式启动、状态表和完整说明都在弹窗里；
                    这个按钮的文字如实显示后端状态（注入开没开、接没接进路由）。 */}
                <Button
                  size="sm"
                  variant={ts.takeover !== null ? "primary" : "default"}
                  className="turnstate-entry"
                  data-testid="codex-turnstate"
                  aria-label={`Codex 识别（turn-state）：${ts.takeover !== null ? `已开启，作用于槽位 ${ts.takeover}` : "未开启"}；注入${ts.enabled ? "开" : "关"}`}
                  onClick={() => setTsOpen(true)}
                >
                  {ts.takeover !== null ? "识别：开" : "识别"}
                </Button>
                <Pill tone={desktop.data?.executable ? "ok" : "default"}>
                  {oursRunning
                    ? "运行中"
                    : desktop.data?.executable
                      ? "未运行"
                      : "未检测到"}
                </Pill>
              </div>
            }
          >
            {/* ⛔ 磁贴必须包在 Card 的一个**直接子 div** 里：
                `.account-launch > div:has(.launchcol)` 才吃得到 flex:1 与两行等高。 */}
            <div className="flex min-w-0 flex-col gap-2">
              {desktop.error && (
                <p role="alert" className="notice notice--danger">
                  {desktop.error}
                </p>
              )}
              {/* GPT 只有三个动作，第三格横跨两列 —— 看起来是刻意排的，不像缺了一块。 */}
              <div className="launchcol">
                <Tile
                  icon={<MonitorSmartphone size={18} />}
                  name={active?.logged_in ? "Codex 桌面端" : "打开并登录"}
                  // 代价常驻：确认框省掉之后这句必须一直看得见。
                  note={
                    !desktop.data?.executable
                      ? "未检测到 Codex 桌面端"
                      : oursRunning
                        ? "运行中 · 面板起的这个会先关掉"
                        : othersRunning
                          ? `未运行 · 别处开着的 ${othersRunning} 个不会被关`
                          : "未运行"
                  }
                  tone="accent"
                  task={launchTask}
                  disabled={busy || !active || !desktop.data?.executable}
                  testId="gpt-tile-launch"
                  onClick={() => active && void requestLaunch(active)}
                />
                {/* 酒馆（GPT 后端）：面板内置桥接驱动官方 codex exec，酒馆与 Claude 那条共用。
                    路径没配就带去扩展中心填；在跑就只开页面。 */}
                <Tile
                  icon={<Wine size={18} />}
                  name="酒馆"
                  note={
                    tavernUnready
                      ? "还没配好 · 点开去填路径"
                      : gptBridge?.running
                        ? "GPT 桥接运行中 · 再点只打开页面"
                        : "起 GPT 桥接与酒馆 · 最长 60 秒"
                  }
                  tone={tavernUnready ? "warn" : undefined}
                  task={tavernTask}
                  disabled={busy || tavernBusy}
                  testId="gpt-tile-tavern"
                  onClick={
                    tavernUnready
                      ? () => navigate("/extensions/sillytavern")
                      : () => void launchTavern()
                  }
                  corner={
                    <BridgeSettingsCorner onClick={() => setBridgeOpen(true)} />
                  }
                />
                <Tile
                  icon={<Square size={18} />}
                  name="一键关闭"
                  note="关掉所有 Codex 桌面端窗口 · 按官方包路径认"
                  task={closeTask}
                  disabled={busy || !desktop.data?.running}
                  testId="gpt-tile-close"
                  className="launchcol-wide"
                  onClick={() => {
                    setError("");
                    setAsk({ action: "close" });
                  }}
                />
              </div>
              {/* 原来这里常驻一行「当前槽位 · 版本号 · 启动前先验出口 IP…」，
                  2026-09-23 使用者删了（门禁归不归管在「IP 锁」弹窗里看）。
                  只在没检测到桌面端时留一条路（0.28.0 起面板自己能装）。 */}
              {desktop.data && !desktop.data.executable && (
                <p className="notice">
                  <Link to="/software">去软件页安装 Codex 桌面端</Link>
                </p>
              )}
            </div>
          </Card>
          <CodexUsageCard
            slot={runningSlot ?? active}
            quota={slotQuotas[(runningSlot ?? active)?.id ?? ""]}
            onReloadQuota={() => void loadSlotQuotas()}
          />
        </div>
      </div>
      {/* 开关、个人/Team、「开启识别（作用于当前槽位）」、状态表与完整说明都在这里。
          关掉弹窗立刻回读一次，标题栏那个按钮的文字跟着变，不等下一轮轮询。 */}
      <Modal
        open={tsOpen}
        onClose={() => {
          setTsOpen(false);
          void refreshTs();
        }}
        title="Codex 识别（turn-state）· 开关、配置与状态"
      >
        <StationTurnState />
      </Modal>
      <Modal
        open={adding}
        onClose={() => !busy && setAdding(false)}
        title="新建 Codex 账户槽位"
      >
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void create();
          }}
          className="flex flex-col gap-3"
        >
          <label htmlFor="codex-label">账户名称</label>
          <input
            id="codex-label"
            className="input"
            value={label}
            maxLength={40}
            autoFocus
            onChange={(e) => setLabel(e.target.value)}
            placeholder="例如：个人账户"
          />
          <p className="notice">
            下一步打开官方桌面端登录，不需要填写密码或粘贴 Token。
          </p>
          {error && (
            <p role="alert" className="notice notice--danger">
              {error}
            </p>
          )}
          <Button
            type="submit"
            variant="primary"
            loading={busy}
            disabled={!label.trim()}
          >
            新建并登录
          </Button>
        </form>
      </Modal>
      <ConfirmDialog
        open={!!ask}
        onCancel={() => !busy && setAsk(null)}
        onConfirm={() => void confirm()}
        loading={busy}
        title={
          ask?.action === "close"
            ? "关闭 Codex 桌面端？"
            : ask?.action === "archive"
              ? `移除 ${askShown}？`
              : `${ask?.action === "switch" ? "切换到" : "打开"} ${askShown}？`
        }
        confirmLabel={
          ask?.action === "close"
            ? "确认关闭"
            : ask?.action === "archive"
              ? "移除并保留归档"
              : ask?.action === "switch"
                ? "关闭桌面端并切换"
                : "关闭旧窗口并打开"
        }
        danger
      >
        {/* 「打开」这个框只在**真的有窗口开着**的时候才弹（见 requestLaunch），
            所以正文可以直接报数。「切换」照旧每次都确认 —— 换的是激活账户，
            那是个决定，不是顺手一点；只是措辞按有没有在跑改一下。 */}
        <p>
          {ask?.action === "close"
            ? "将关闭所有 Codex 桌面端窗口及其正在运行的任务，请先保存工作。账户槽位、登录资料和历史会话会保留。"
            : ask?.action === "archive"
              ? "槽位从列表移除，登录资料和会话保留在本机归档中。"
              : ask?.action === "switch"
                ? oursRunning
                  ? "会先关闭面板起的 Codex 桌面端及其正在运行的任务（开始菜单或别的程序起的不动）。请确认当前工作已经保存，再继续。"
                  : "面板起的 Codex 桌面端没在跑，直接切。"
                : `面板起的 Codex 桌面端还开着（${ask?.running ?? 0} 个窗口），会先关掉它及其正在运行的任务；开始菜单或别的程序起的不动。请确认当前工作已经保存，再继续。`}
        </p>
        {error && (
          <p role="alert" className="notice notice--danger mt-2">
            {error}
          </p>
        )}
        {needsReg && (
          <Button
            variant="primary"
            className="mt-2"
            loading={repairing}
            onClick={() => void repairRegistration()}
          >
            修复 Codex 注册（需要管理员）
          </Button>
        )}
      </ConfirmDialog>
      {/* 0.32.0：三条桥的端口与模型 —— 三个账户页共用同一个组件。 */}
      <BridgeSettings
        open={bridgeOpen}
        provider="gpt"
        onClose={() => setBridgeOpen(false)}
      />
    </>
  );
}

function CodexUsageCard({
  slot,
  quota,
  onReloadQuota,
}: {
  slot?: CodexSlot;
  quota?: CodexQuotaState;
  /** 重读本机快照与「最近一次」联网结果。**不联网** —— 联网只在槽位行的刷新图标。 */
  onReloadQuota: () => void;
}) {
  const [days, setDays] = useState(1);
  const [revision, setRevision] = useState(0);
  const [data, setData] = useState<CodexUsage | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let cancelled = false;
    let pending = false;
    setData(null);
    setError("");
    if (!slot) return;
    const load = async () => {
      if (pending) return;
      pending = true;
      setBusy(true);
      try {
        const next = await codexApi.usage(slot.id, days);
        if (!cancelled) {
          setData(next);
          setError("");
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      } finally {
        pending = false;
        if (!cancelled) setBusy(false);
      }
    };
    void load();
    const timer = window.setInterval(() => void load(), 60_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [slot?.id, days, revision]);

  const found = quota?.local?.found ?? null;
  const tight = [found?.primary, found?.secondary]
    .filter((window): window is NonNullable<typeof window> => !!window)
    .sort((a, b) => b.window.used - a.window.used)[0];
  const liveTight = quota?.live?.windows
    .filter((window) => window.remaining_percent != null)
    .sort(
      (a, b) => (a.remaining_percent ?? 101) - (b.remaining_percent ?? 101),
    )[0];
  const stamp = (iso?: string | null) =>
    iso
      ? new Date(iso).toLocaleString("zh-CN", {
          month: "2-digit",
          day: "2-digit",
          hour: "2-digit",
          minute: "2-digit",
        })
      : "";
  const quotaValue = liveTight
    ? `${Math.round(liveTight.remaining_percent ?? 0)}%`
    : tight
      ? `${Math.round(100 - tight.window.used)}%`
      : "—";
  const quotaSource = liveTight
    ? `在线 · ${stamp(quota?.live?.fetched_at)}`
    : tight
      ? "本地快照"
      : quota?.loading
        ? "读取中…"
        : quota?.error
          ? "在线读取失败"
          : "未读取额度";

  return (
    <Card
      className="account-usage"
      title={
        <>
          <Gauge size={14} />
          {slot
            ? `${slotName(slot.email, slot.label)} 的用量`
            : "当前账户的用量"}
        </>
      }
    >
      <div className="usage-ranges">
        {(
          [
            [1, "今天"],
            [7, "7 天"],
            [0, "全部"],
          ] as const
        ).map(([d, n]) => (
          <Button
            key={d}
            size="sm"
            variant={days === d ? "primary" : "default"}
            onClick={() => setDays(d)}
          >
            {n}
          </Button>
        ))}
        <Button
          size="sm"
          aria-label="刷新 Codex 用量"
          icon={<RefreshCw size={12} />}
          loading={busy}
          onClick={() => {
            setRevision((n) => n + 1);
            onReloadQuota();
          }}
        />
      </div>
      {error && (
        <p role="alert" className="notice notice--danger">
          {error}
        </p>
      )}
      {/* ⛔ 四格要说**四件事**（0.32.0 改的）。
          原来是「输入 / 输出 / 缓存读取 / 合计」—— 合计就是前三格加起来，
          四个数字只携带三个信息，而真正想知道的「还剩多少额度」一格都没有。 */}
      <div className="ustats">
        <div className="ustat">
          <span className="ustat-name">Token 总量</span>
          <span className="ustat-value">{data ? short(data.total) : "—"}</span>
          {data && (
            <span className="ustat-sub">
              入 {short(data.input)} · 出 {short(data.output)} · 缓存{" "}
              {short(data.cached)}
            </span>
          )}
        </div>
        <div className="ustat">
          <span className="ustat-name">会话数</span>
          <span className="ustat-value">
            {data ? String(data.sessions) : "—"}
          </span>
          {data && <span className="ustat-sub">本机记录 · 不含云端任务</span>}
        </div>
        <div className="ustat">
          <span className="ustat-name">推理 token</span>
          <span className="ustat-value">
            {data ? short(data.reasoning) : "—"}
          </span>
          {data && (
            <span className="ustat-sub">
              占输出{" "}
              {data.output > 0
                ? `${Math.round((data.reasoning / data.output) * 100)}%`
                : "—"}
            </span>
          )}
        </div>
        {/* 剩余额度：两个窗口里**紧的那个**。只看宽裕的那半做判断是这套显示最大的风险。 */}
        <div className="ustat" data-testid="codex-quota">
          <span className="ustat-name">剩余额度</span>
          <span className="ustat-value">{quotaValue}</span>
          <span className="ustat-sub">{quotaSource}</span>
        </div>
      </div>
      {/* 全零有两种：真没跑过，和读坏了。分开说 —— 「0」在界面上读起来是一句
          斩钉截铁的事实，使用者只会以为统计坏了。桌面端的云端任务不写本机文件，
          这个槽位只用过云端任务时，这里就是 0 份文件、0 个会话。 */}
      <div className="usage-footer">
        <span className="notice">
          {data && (data.files_failed > 0 || data.incomplete > 0)
            ? "统计不完整"
            : data && data.files_read === 0 && data.sessions === 0
              ? "该槽位还没有本机会话记录 · 云端任务不写本机文件"
              : "本机记录 · 每分钟刷新"}
        </span>
        {/* 0.32.0：明细搬到可滚动的 `/usage` 上去了 ——
            这一页是固定高度的，趋势图与按模型分布摆不下。 */}
        <Link to="/usage?side=gpt" className="btn btn--sm btn--ghost">
          用量明细
        </Link>
      </div>
    </Card>
  );
}
