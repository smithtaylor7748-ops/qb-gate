/**
 * 官方账户 · 反重力（0.26.0；0.30.0 重排）。
 *
 * 跟 Claude / GPT 两页同一套版式（左：槽位卡；右：启动卡 + 用量卡），内容按反重力的事实来：
 *
 *   - 左边一张槽位卡装**两种**槽位，脚注上一对页签切换：
 *       · **反重力 IDE 槽位**（0.30.0）—— 一个槽位一个 `--user-data-dir`。IDE 是 VS Code 分支，
 *         登录令牌在那个目录的 `state.vscdb` 里，换目录就是换登录；面板只问令牌那一行的长度。
 *         **Hub 没有槽位**：它的令牌在 Windows 凭据管理器（按用户全局），换号在它自己界面里做。
 *       · **Gemini CLI 槽位** —— 酒馆的 Gemini 桥接用（`GEMINI_CLI_HOME`）。
 *   - 右上是启动卡：跟 Claude 页同一套 `Tile` + `.launchcol` 2×2 —— Hub / IDE / 酒馆 / 一键关闭。
 *     起之前验 IP、先关正在跑的、挂看门狗（桌面档，零宽限）。IDE 起的是**激活槽位**那份资料。
 *     汉化 / 自动审批 / 高危拦截引擎收进标题栏一颗按钮打开的弹窗（照 GPT 页「识别」的先例）——
 *     0.29.0 之前它占着右下整张卡。
 *   - 右下是用量卡（0.30.0）：token 用量读语言服务器的对话记录库、账户读 IDE 自己写下的
 *     `state.vscdb`。剩余配额只显示，面板不据此做任何决定。
 *   - **联网额度**（2026-09-23）：每条账户行右侧、用量卡的 Hub / Gemini CLI 两格各有一个刷新，
 *     点了才问 Google 一次、一次只问那一个；打开页面只显示「最近一次」问到的与本机那份。
 *
 * # 确认框只在真的有东西在跑的时候弹（0.27.0）
 *
 * 起反重力会先关掉正在跑的那份（Electron 单实例），所以确认一次是对的；可什么都没跑的时候
 * 那个框就是纯噪音。现在点启动先问一次 `antigravity_running`（跟启动链同一套证据），
 * 有才弹。**代价没有因此藏起来** —— 「开着的会先关掉」常驻在每格的副标题里。
 *
 * ⛔ 这一页是固定高度（test:ui 钉着 680×640 起四档不许裁切），每一行都要省着用。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Code2,
  Gauge,
  Languages,
  MonitorSmartphone,
  Plus,
  RefreshCw,
  Square,
  Users,
  Wine,
} from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Link, useNavigate } from "react-router-dom";
import { api } from "../../lib/api";
import { AG_R, antigravityApi } from "../../lib/antigravity";
import {
  countdown,
  groupLabel,
  lowestQuota,
  modelGroups,
  percent,
  quotaErrorLabel,
  resetIn,
  spanLabel,
  tightestWindow,
} from "../../lib/antigravityQuota";
import type { AntigravityAccount } from "../../lib/generated/AntigravityAccount";
import type { AntigravityIdentity } from "../../lib/generated/AntigravityIdentity";
import type { AntigravityHubIdentity } from "../../lib/generated/AntigravityHubIdentity";
import type { AntigravityOnlineQuota } from "../../lib/generated/AntigravityOnlineQuota";
import type { AntigravityProduct } from "../../lib/generated/AntigravityProduct";
import type { AntigravityUsage } from "../../lib/generated/AntigravityUsage";
import type { TavernGeminiQuota } from "../../lib/generated/TavernGeminiQuota";
import type { UiStatus } from "../../lib/generated/UiStatus";
import { AFTER, R } from "../../lib/resources";
import AntigravitySlotRow, { canAttach } from "./AntigravitySlotRow";
import BridgeSettings, { BridgeSettingsCorner } from "../tavern/BridgeSettings";
import { invalidate, useResource, useSession } from "../../lib/store";
import { endTask, resetTask, useTask, type TaskState } from "../../lib/tasks";
import {
  Button,
  Card,
  Checkbox,
  ConfirmDialog,
  Gauge as QuotaBar,
  Modal,
  Pill,
  useToast,
} from "../../ui";
import Tile from "./Tile";

const PER_PAGE = 4;
const NUM = new Intl.NumberFormat("zh-CN");
/** 大数字缩写：1.2M / 34.5K。表格里要精确值，卡片上要一眼看得懂。 */
function short(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
  return NUM.format(n);
}
/** 一个账户联网额度的界面状态：最近一次问到的、这次的错、在不在问。 */
interface OnlineState {
  quota: AntigravityOnlineQuota | null;
  error: string;
  loading: boolean;
}

/** 磁贴副标题里放槽位名：太长就截，`.tile-note` 裁切会被 test:ui 抓到。 */
function shortLabel(label: string): string {
  const chars = Array.from(label);
  return chars.length > 8 ? chars.slice(0, 8).join("") + "…" : label;
}

export default function AntigravityBand() {
  const status = useResource("antigravity", AG_R.status);
  const plugins = useResource("plugins", R.plugins);
  const toast = useToast();
  const navigate = useNavigate();
  const tavern = plugins.data?.find((p) => p.id === "sillytavern");
  const tavernUnready = tavern?.state === "missing";
  const tavernTask = useTask("tavern-start");

  // ------------------------------------------------------------ 汉化与审批引擎
  const [ui, setUi] = useState<UiStatus | null>(null);
  const refreshUi = useCallback(async () => {
    try {
      setUi(await api.antigravityUiStatus());
    } catch {
      // 演示模式或后端没起时读不到；保持上一次的值。
    }
  }, []);
  useEffect(() => {
    void refreshUi();
    const timer = window.setInterval(() => void refreshUi(), 5000);
    return () => window.clearInterval(timer);
  }, [refreshUi]);
  const [uiBusy, setUiBusy] = useState(false);
  const [engineOpen, setEngineOpen] = useState(false);
  async function toggleEngine() {
    setUiBusy(true);
    try {
      if (ui?.running) {
        await api.antigravityUiStop();
        toast.ok("汉化与审批引擎已停止（页面里的脚本随下次刷新消失）");
      } else {
        const s = await api.antigravityUiStart();
        toast.ok(
          s.verified > 0
            ? `已附加：${s.verified} 个页面核实生效，汉化立即可见`
            : "已附加到反重力：正在核实脚本是否真的跑起来（看引擎弹窗里的状态）",
        );
      }
      await refreshUi();
      invalidate("plugins");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setUiBusy(false);
    }
  }
  async function saveFlag(patch: Partial<UiStatus["config"]>) {
    if (!ui) return;
    const next = { ...ui.config, ...patch };
    setUi({ ...ui, config: next });
    try {
      await api.antigravityUiConfigSave(next);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
      void refreshUi();
    }
  }

  // ------------------------------------------------------------ 起 / 关反重力
  const [ask, setAsk] = useState<
    | {
        action: "launch";
        product: AntigravityProduct;
        /** 点的时候实测到的进程数。`null` = 查不出来（枚举失败）—— 照弹，但不许编一个数。 */
        running: number | null;
        /** 这次要起的是哪一条账户（提示里要写它，不是切换之前那一条）。 */
        slot?: AntigravityAccount;
      }
    | { action: "close-all"; running: number }
    | { action: "ide-switch" | "ide-archive"; id: string; label: string }
    | {
        action: "attach";
        id: string;
        label: string;
        into: string;
        intoLabel: string;
      }
    | null
  >(null);
  const [busy, setBusy] = useState(false);
  /** 「启动」正在查进程 / 正在起：挡住第二次点击（见 `requestLaunch`）。 */
  const launching = useRef(false);
  const [error, setError] = useState("");
  const hubTask = useTask("launch-antigravity");
  const ideTask = useTask("launch-antigravity-ide");
  const [closing, setClosing] = useState(false);
  const [closeErr, setCloseErr] = useState("");
  const hub = status.data?.hub;
  const ide = status.data?.ide;
  const underGate = status.data?.under_gate ?? true;
  const slots: AntigravityAccount[] = status.data?.accounts.slots ?? [];
  const activeIde = slots.find((s) => s.active);

  // ------------------------------------------------------------ 每个账户的联网额度（2026-09-23）
  //
  // IDE 写在本机的 `userStatus` 每个模型只有一个比例、只有 IDE 开着时才更新 —— 使用者看到的
  // 「剩 100% · 5 天后重置」是两天前的数。联网那一份（5 小时 / 每周四格 + AI 积分）
  // ⛔ **只手动刷新**：挂载、账户增减时只读「最近一次」问到的（`refresh: false`，后端绝不联网），
  // 联网只从某一行的刷新图标发出，一次只问那一个账户。
  const [online, setOnline] = useState<Record<string, OnlineState>>({});
  const slotIds = slots.map((s) => s.id).join("\u0000");
  useEffect(() => {
    const ids = slotIds ? slotIds.split("\u0000") : [];
    let cancelled = false;
    void Promise.all(
      ids.map(async (id) => {
        try {
          return [id, await antigravityApi.accountQuota(id, false)] as const;
        } catch {
          return [id, null] as const;
        }
      }),
    ).then((rows) => {
      if (cancelled) return;
      setOnline((prev) => {
        const next: Record<string, OnlineState> = {};
        for (const [id, quota] of rows) {
          next[id] = {
            quota: quota ?? prev[id]?.quota ?? null,
            error: prev[id]?.error ?? "",
            loading: prev[id]?.loading ?? false,
          };
        }
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
  }, [slotIds]);
  /** 点某一行的刷新图标：**只联网问这一个账户**。没问到就留着上一次的，原因写在那一行。 */
  const refreshOnline = useCallback(
    async (id: string) => {
      setOnline((prev) => ({
        ...prev,
        [id]: { quota: prev[id]?.quota ?? null, error: "", loading: true },
      }));
      try {
        const quota = await antigravityApi.accountQuota(id, true);
        setOnline((prev) => ({
          ...prev,
          [id]: { quota, error: "", loading: false },
        }));
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        toast.error(msg);
        setOnline((prev) => ({
          ...prev,
          [id]: { quota: prev[id]?.quota ?? null, error: msg, loading: false },
        }));
      }
    },
    [toast],
  );
  /**
   * 点「启动」。**先查这个产品有没有在跑，真的有才弹确认框**（0.27.0）。
   *
   * 起反重力会先把正在跑的那份关掉（Electron 单实例），所以确认一次是对的；
   * 可什么都没在跑的时候那个框就是纯噪音 —— 使用者的原话是「每次点启动都会跳这个」。
   *
   * ⛔ 查不出来（枚举失败）一律当作**可能在跑**照弹。把「不知道」降级成「没有」
   * 正是 §7.17 那条坑的形状（`-AsArray` 让一键关闭永远数出 0 个）。
   */
  async function requestLaunch(
    product: AntigravityProduct,
    slot?: AntigravityAccount,
  ) {
    // 查进程要跑一遍 PowerShell 加签名核对，要几秒。这几秒里再点一次，原来会起两次 ——
    // 第二次起之前把第一次刚起的那个关掉（2026-09-25）。用 ref 挡，不等下一次渲染。
    if (launching.current) return;
    launching.current = true;
    setBusy(true);
    setError("");
    try {
      let running: number | null;
      try {
        running = await antigravityApi.running(product);
      } catch {
        // 查不出来一律当作**可能在跑**照弹（§7.17），但框里不许写一个编出来的数。
        running = null;
      }
      if (running === null || running > 0) {
        setAsk({ action: "launch", product, running, slot });
        return;
      }
      await runLaunch(product, slot);
    } finally {
      launching.current = false;
      setBusy(false);
    }
  }

  /**
   * 真正去起。确认框走这里，没东西在跑时也走这里（跳过确认框）。
   *
   * `slot` 是这次要起的那条账户。⛔ 别读 `activeIde`：从账户行上起的时候是「先切过去、再起」，
   * 而这里拿到的是切换之前那次渲染的闭包 —— 原来提示写的是切换前那条账户、
   * 「要不要在窗口里登录」也是按那一条判的（2026-09-25）。
   */
  async function runLaunch(
    product: AntigravityProduct,
    slot?: AntigravityAccount,
  ) {
    setBusy(true);
    setError("");
    try {
      const task =
        product === "hub" ? "launch-antigravity" : "launch-antigravity-ide";
      resetTask(task);
      try {
        await antigravityApi.launch(product);
        endTask(task);
      } catch (e) {
        endTask(task, e instanceof Error ? e.message : String(e));
        throw e;
      }
      const who = product === "ide" ? (slot ?? activeIde) : undefined;
      toast.ok(
        product === "hub"
          ? "反重力已启动；汉化引擎会在它的调试端口起来后自动附加"
          : who
            ? `反重力 IDE 已用账户「${who.label}」启动；${who.ide_logged_in ? "" : "在它的窗口里用 Google 登录，"}汉化引擎会在调试端口起来后自动附加`
            : "反重力 IDE 已启动；汉化引擎会在它的调试端口起来后自动附加",
      );
      await status.refresh();
      invalidate(...AFTER.lease);
      void refreshUi();
      setAsk(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  /** 账户行上的「登录 / 打开 IDE」：不是激活槽位就先切过去，再走同一条启动路径。 */
  async function requestIdeLaunch(slot: AntigravityAccount) {
    setError("");
    if (!slot.active) {
      setBusy(true);
      try {
        await antigravityApi.ideSelect(slot.id);
        await status.refresh();
      } catch (e) {
        // ⛔ 这里没有弹窗开着，`error` 只在两个弹窗里显示 —— 原来切不过去（比如酒馆的 Gemini
        // 桥接正在跑）就什么都看不见，点了跟没点一样（2026-09-25）。
        const msg = e instanceof Error ? e.message : String(e);
        setError(msg);
        toast.error(msg);
        setBusy(false);
        return;
      }
      setBusy(false);
    }
    await requestLaunch("ide", slot);
  }

  /** 点「一键关闭反重力」：Hub 与 IDE 一起收。同样先查有没有东西在跑。 */
  async function requestCloseAll() {
    setError("");
    setCloseErr("");
    let running = 0;
    let known = true;
    for (const p of ["hub", "ide"] as const) {
      try {
        running += await antigravityApi.running(p);
      } catch {
        known = false;
      }
    }
    if (!known || running > 0) {
      setAsk({ action: "close-all", running });
      return;
    }
    toast.info("没有在跑的反重力，什么都不用关");
  }

  async function confirm() {
    if (!ask) return;
    setBusy(true);
    setError("");
    try {
      if (ask.action === "close-all") {
        setClosing(true);
        setCloseErr("");
        try {
          let n = 0;
          for (const p of ["hub", "ide"] as const) {
            n += await antigravityApi.close(p);
          }
          toast.ok(n ? `已关闭 ${n} 个进程` : "没有在跑的实例");
        } catch (e) {
          setCloseErr(e instanceof Error ? e.message : String(e));
          throw e;
        } finally {
          setClosing(false);
        }
      } else if (ask.action === "launch") {
        // 确认过了，交给同一条启动路径 —— 别在这里再写一份。
        setBusy(false);
        await runLaunch(ask.product, ask.slot);
        return;
      } else if (ask.action === "ide-switch") {
        await antigravityApi.ideSelect(ask.id);
        toast.ok(
          "已切换账户：IDE 与 Gemini CLI 两半一起换。点右侧「反重力 IDE」用它启动",
        );
      } else if (ask.action === "ide-archive") {
        await antigravityApi.ideArchive(ask.id);
        toast.ok("账户已移除，资料目录保留在本机归档中");
      } else if (ask.action === "attach") {
        await antigravityApi.accountAttach(ask.into, ask.id);
        toast.ok("已并成一条；两边的目录都还在原地，只是索引合了");
      }
      await status.refresh();
      invalidate(...AFTER.lease);
      void refreshUi();
      setAsk(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  // ------------------------------------------------------------ 账户卡（0.32.0：一条 = 一个账户）
  //
  // 0.30.0–0.31.0 这里是两个页签：「IDE 槽位」与「Gemini CLI 槽位」。同一个 Google
  // 账户要建两次、登两次，而只用 IDE 的人永远看着一句「Gemini CLI · 0 个槽位」——
  // 它读起来像个故障，其实只是「你还没建」。现在一条槽位两半，页签没了。
  const count = slots.length;
  const activeIndex = slots.findIndex((s) => s.active);
  const [pageRaw, setPage] = useSession("home.antigravity.page", -1);
  const pages = Math.max(1, Math.ceil(count / PER_PAGE));
  const autoPage = Math.floor(Math.max(0, activeIndex) / PER_PAGE);
  const page = Math.min(pageRaw < 0 ? autoPage : pageRaw, pages - 1);
  const [adding, setAdding] = useState(false);
  const [label, setLabel] = useState("");
  async function create() {
    if (!adding) return;
    setBusy(true);
    setError("");
    try {
      const id = await antigravityApi.ideCreate(label);
      await status.refresh();
      setAdding(false);
      setLabel("");
      setPage(Math.floor(slots.length / PER_PAGE));
      // 新建完直接去起 IDE：新槽位不是激活的就先切过去（里面自己判断要不要弹框）。
      setBusy(false);
      await requestIdeLaunch({
        id,
        label: label.trim(),
        active: slots.length === 0,
        ide_dir: "",
        ide_logged_in: false,
        ide_auth_state: "",
        ide_unreadable: false,
        email: null,
        tier: null,
        identity_error: null,
        quota: [],
        written_at: null,
        cli_dir: "",
        cli_logged_in: false,
        cli_auth_state: "",
        cli_unreadable: false,
      });
      return;
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  }
  async function login(id: string) {
    try {
      await antigravityApi.geminiLogin(id);
      toast.ok(
        "已打开 Gemini CLI 登录窗口：在里面选「Login with Google」完成登录，关掉窗口后这里会显示已登录",
      );
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
  }
  /**
   * `npm install -g @google/gemini-cli`。**等它装完**（0.29.0）。
   *
   * 原来这里 toast 的是「已打开 npm 安装窗口，装完关掉窗口再刷新」—— 而那个窗口里
   * npm 一次都没跑起来过（`/k` 被加了引号，cmd 认不出是开关）。现在返回的是
   * 后端回读检测之后的结论，装没装上由那句话说了算。进度条在软件页那张卡上。
   */
  async function installCli() {
    setBusy(true);
    try {
      toast.ok(await antigravityApi.geminiCliInstall());
      // 资源名是 "antigravity"（见本文件开头的 useResource）—— 原来这里写成
      // "antigravityStatus"，装完之后状态从来没刷新过，要等下一次 15 秒轮询。
      invalidate("antigravity", "software");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  // ------------------------------------------------------------ 酒馆（Gemini 后端）
  const [tavernBusy, setTavernBusy] = useState(false);
  // Gemini 桥接在不在跑（本机状态，不联网）。原来这块贴不看它：在跑时照样写「起 Gemini 桥接与酒馆」，
  // 而 GPT 页那块会写「GPT 桥接运行中 · 再点只打开页面」（2026-09-25）。
  const [geminiRunning, setGeminiRunning] = useState(false);
  const refreshGeminiBridge = useCallback(async () => {
    try {
      setGeminiRunning((await api.tavernGeminiStatus()).running);
    } catch {
      // 演示模式或后端没起时读不到；保持上一次的值。
    }
  }, []);
  useEffect(() => {
    void refreshGeminiBridge();
    const timer = window.setInterval(() => void refreshGeminiBridge(), 20_000);
    return () => window.clearInterval(timer);
  }, [refreshGeminiBridge]);
  async function launchTavern() {
    setTavernBusy(true);
    resetTask("tavern-start");
    try {
      const url = await api.pluginStart("gemini");
      endTask("tavern-start");
      invalidate(...AFTER.tavern);
      void refreshGeminiBridge();
      try {
        if (url.startsWith("http")) await openUrl(url);
        toast.ok("酒馆已就绪（Gemini 桥接），已打开页面");
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

  // ---------------------------------------------------- 桥接设置（0.31.0，0.32.0 抽成组件）
  //
  // 使用者定的：**三条桥的设置全由面板管**，入口做成酒馆那格右上角的小按钮。
  // 0.32.0 弹窗本体搬去了 `features/tavern/BridgeSettings.tsx` —— 原来它是这个
  // 文件里一个内联的 `<Modal>` 字面量，于是**只有反重力页开得了它**，
  // 而 Claude / GPT 两页的酒馆磁贴上没有入口 —— 可那两条桥的端口与模型
  // 恰恰也在这里改。
  const [bridgeOpen, setBridgeOpen] = useState(false);

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

  // ------------------------------------------------------------ 启动磁贴（0.27.0）
  //
  // 跟 Claude 页同一套 `Tile` + `.launchcol`（2×2）：Hub / IDE / 酒馆 / 一键关闭。
  // 状态不再是一个单独的 Pill，而是写进每格的常驻副标题 —— 这一页是固定高度，
  // 省下来的那两行正好给磁贴。
  const productTile = (
    p: typeof hub,
    product: AntigravityProduct,
    task: TaskState,
  ) => {
    const running = !!p?.session_id;
    const state = running ? "运行中" : p?.installed ? "未运行" : "未安装";
    // IDE 起的是激活槽位那份资料：写进副标题，让人知道点下去登的是谁。
    const who =
      product === "ide"
        ? activeIde
          ? ` · 槽位「${shortLabel(activeIde.label)}」`
          : " · 默认资料"
        : " · 单实例";
    return (
      <Tile
        icon={
          product === "hub" ? (
            <MonitorSmartphone size={18} />
          ) : (
            <Code2 size={18} />
          )
        }
        name={p?.label ?? (product === "hub" ? "反重力" : "反重力 IDE")}
        // ⛔ 代价常驻：省掉确认框之后，「开着的会先关掉」这句必须一直看得见，
        // 不许缩进悬停提示里（档案 §7 / CLAUDE.md 桌面端那一条同源）。
        // 状态还没回来 / 读失败时不许说「未安装」—— 那是「不知道」，不是「没有」（§7.17，2026-09-25）。
        note={
          !status.data
            ? status.error
              ? "状态读不出来 · 见左边的报错"
              : "读取中…"
            : p?.installed
              ? `${state}${who} · 开着的会先关掉`
              : "未安装 · 到「软件」页一键装"
        }
        tone={product === "hub" ? "accent" : "warn"}
        task={task}
        disabled={busy || !p?.installed}
        testId={`ag-tile-${product}`}
        onClick={() => void requestLaunch(product)}
      />
    );
  };

  // ------------------------------------------------------------ 账户行
  //
  // 两个页签合成一份列表（0.32.0）。行本体在 `AntigravitySlotRow.tsx` ——
  // 这个文件已经一千五百行，再往里塞一段就没人读得动了。
  // 「几小时后重置」的字得自己往前走。一分钟一跳就够了 —— 那个字的粒度是小时。
  const [nowMs, setNowMs] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNowMs(Date.now()), 60_000);
    return () => clearInterval(t);
  }, []);
  const [attachFrom, setAttachFrom] = useState<string | null>(null);
  const attachSource = slots.find((x) => x.id === attachFrom) ?? null;
  const slotRows = slots
    .slice(page * PER_PAGE, (page + 1) * PER_PAGE)
    .map((s) => (
      <AntigravitySlotRow
        key={s.id}
        slot={s}
        busy={busy}
        ideInstalled={!!ide?.installed}
        cliInstalled={!!status.data?.gemini_cli_installed}
        now={nowMs}
        // 源行自己也要拿到 `attachFrom`：它那一行上的「取消」靠的就是它（2026-09-25）。
        // 原来只传给「能并进去」的行，而 `canAttach` 对同一条回 false —— 取消键永远不出现，
        // 挑不到能并的那一条时只能离开这一页。
        attachFrom={
          attachSource &&
          (s.id === attachSource.id || canAttach(s, attachSource))
            ? attachFrom
            : null
        }
        onSwitch={() => {
          setError("");
          setAsk({ action: "ide-switch", id: s.id, label: s.label });
        }}
        onOpenIde={() => void requestIdeLaunch(s)}
        online={online[s.id]?.quota ?? null}
        onlineError={online[s.id]?.error ?? ""}
        onlineLoading={online[s.id]?.loading ?? false}
        onRefresh={() => void refreshOnline(s.id)}
        onLoginCli={() => void login(s.id)}
        onArchive={() => {
          setError("");
          setAsk({ action: "ide-archive", id: s.id, label: s.label });
        }}
        onAttachStart={setAttachFrom}
        onAttachHere={() => {
          if (!attachSource) return;
          setError("");
          setAttachFrom(null);
          setAsk({
            action: "attach",
            id: attachSource.id,
            label: attachSource.label,
            into: s.id,
            intoLabel: s.label,
          });
        }}
      />
    ));

  return (
    <>
      <div className="account-workspace" data-testid="antigravity-accounts">
        <Card
          className="account-slots"
          title={
            <>
              <Users size={14} />
              反重力账户
            </>
          }
          actions={
            <div className="flex gap-1.5">
              <Button
                size="sm"
                icon={<RefreshCw size={12} />}
                aria-label="刷新反重力状态"
                loading={status.loading}
                onClick={() =>
                  void status.refresh().catch((e) => toast.error(String(e)))
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
          {status.error && (
            <p role="alert" className="notice notice--danger">
              {status.error}
            </p>
          )}
          {status.data && !slots.length && (
            <div className="py-3">
              <h3>建一个反重力账户</h3>
              <p className="notice mt-2">
                一条 = 一个 Google 账户，底下两半：<strong>IDE</strong>{" "}
                一个独立的资料目录（在 IDE 自己的窗口里登录），
                <strong>Gemini CLI</strong> 一个 <code>GEMINI_CLI_HOME</code>
                （酒馆的 Gemini
                桥接用）。两半各登各的，登录都在官方客户端里完成；面板不经手登录，
                只在你点额度刷新时读一次它们存在本机的令牌、在内存里用（过期了在内存里换新），
                不写回、不落盘。现有的默认资料保持原样。
                <strong>Hub 没有槽位</strong>
                ：它的令牌在 Windows 凭据管理器里，换号在它自己界面里做。
              </p>
              {status.data && !status.data.gemini_cli_installed && (
                <Button
                  size="sm"
                  className="mt-2"
                  onClick={() => void installCli()}
                >
                  用 npm 安装 Gemini CLI
                </Button>
              )}
            </div>
          )}
          <div className="account-slot-list">{slotRows}</div>
          {/* 脚注一行装下计数 + 翻页：这一页是固定高度，不另起一行。 */}
          <div className="account-pagination">
            {attachSource ? (
              <span className="notice qb-tone-warn">
                挑一条把「{attachSource.label}」并进去
              </span>
            ) : (
              <span className="notice">
                {status.data
                  ? `${count} 个账户`
                  : status.error
                    ? "账户清单读不出来"
                    : "读取中…"}
              </span>
            )}
            {pages > 1 && (
              <span className="pager ml-auto">
                {Array.from({ length: pages }, (_, i) => (
                  <button
                    key={i}
                    className="pager-btn"
                    type="button"
                    aria-label={`第 ${i + 1} 页`}
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
            className="account-launch ag-launch"
            title="启动"
            actions={
              <div className="flex items-center gap-1.5">
                {/* 汉化 / 自动审批 / 高危拦截的入口：放在标题栏，不占纵向空间 ——
                    开关、计数、附加 / 停止全在弹窗里（照 GPT 页「识别」的先例）。
                    按钮的文字如实显示引擎在不在跑。 */}
                <Button
                  size="sm"
                  variant={ui?.running ? "primary" : "default"}
                  className="turnstate-entry"
                  data-testid="ag-engine-entry"
                  aria-label={`汉化与审批引擎：${ui?.running ? "运行中" : "未附加"}`}
                  onClick={() => setEngineOpen(true)}
                >
                  {ui?.running ? "汉化：开" : "汉化"}
                </Button>
                {status.data && (
                  <Pill tone={underGate ? "ok" : "warn"}>
                    {underGate ? "归 IP 锁" : "已移出门禁"}
                  </Pill>
                )}
              </div>
            }
          >
            {/* ⛔ 磁贴必须包在 Card 的一个**直接子 div** 里：
                `.account-launch > div:has(.launchcol)` 才吃得到 flex:1 与两行等高。 */}
            <div className="flex min-w-0 flex-col gap-2">
              <div className="launchcol">
                {productTile(hub, "hub", hubTask)}
                {productTile(ide, "ide", ideTask)}
                <Tile
                  icon={<Wine size={18} />}
                  name="酒馆"
                  note={
                    tavernUnready
                      ? "还没配好 · 点开去填路径"
                      : geminiRunning
                        ? "Gemini 桥接运行中 · 再点只打开页面"
                        : // 就绪窗口是 180 秒（`sillytavern.rs` 的 ST_READY_ATTEMPTS），原来写「最长 60 秒」。
                          "起 Gemini 桥接与酒馆 · 最长约 3 分钟"
                  }
                  tone={tavernUnready ? "warn" : undefined}
                  task={tavernTask}
                  disabled={busy || tavernBusy}
                  testId="ag-tile-tavern"
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
                  name="一键关闭反重力"
                  note="Hub 与 IDE 一起收 · 按安装目录认"
                  task={closeTask}
                  disabled={busy || closing}
                  testId="ag-tile-close"
                  onClick={() => void requestCloseAll()}
                />
              </div>
              {/* 原来这里常驻一行「Hub 日志里见过登录成功 · 启动前先验出口 IP…」，
                  2026-09-23 使用者删了（归不归门禁看标题栏那枚徽标，Hub 登没登录看用量卡的 Hub 那一格）。
                  只在两个都没装时留一条路。 */}
              {status.data && !hub?.installed && !ide?.installed && (
                <p className="notice">
                  <Link to="/software">去软件页安装反重力</Link>
                </p>
              )}
            </div>
          </Card>
          <AntigravityUsageCard
            identity={status.data?.identity ?? null}
            source={status.data?.identity_source ?? ""}
            identityError={status.data?.identity_error ?? null}
            quotaKey={activeIde?.id ?? null}
            activeOnline={
              activeIde ? (online[activeIde.id]?.quota ?? null) : null
            }
            hubLoggedIn={status.data?.hub_logged_in ?? false}
            hubIdentity={status.data?.hub_identity ?? null}
            hubIdentityError={status.data?.hub_identity_error ?? null}
          />
        </div>
      </div>
      {/* 桥接设置（0.31.0）：一份酒馆、三条桥，端口与模型全在这里改。
          入口是酒馆那格右上角的小按钮 —— 使用者要的是「都由面板操控设置」。 */}
      <BridgeSettings
        open={bridgeOpen}
        provider="gemini"
        onClose={() => setBridgeOpen(false)}
      />
      {/* 汉化 / 自动审批 / 高危拦截：附加 / 停止、三个开关、计数、规则入口。 */}
      <Modal
        open={engineOpen}
        onClose={() => setEngineOpen(false)}
        icon={<Languages size={16} />}
        title="汉化 · 自动审批 · 高危拦截"
        headerActions={
          <Button
            size="sm"
            variant={ui?.running ? "danger" : "primary"}
            loading={uiBusy}
            disabled={!ui || (!ui.running && !ui.port_file_present)}
            data-testid="ag-engine-toggle"
            onClick={() => void toggleEngine()}
          >
            {ui?.running ? "停止" : "附加"}
          </Button>
        }
      >
        {/* 状态一行。运行中时按源报数 —— 「Hub 生效了、IDE 没端口」是两件事，
            合成一个数字之后没人说得清该去修哪边。 */}
        <p className="notice" title={ui?.detail}>
          {ui?.detail ?? "读取中…"}
          {ui?.running && ui.sources.length > 0
            ? ` · ${ui.sources
                .map(
                  (s) =>
                    `${s.label} ${
                      s.port === 0
                        ? "无端口"
                        : s.error
                          ? "连不上"
                          : `${s.verified}/${s.sockets}`
                    }`,
                )
                .join(" · ")}`
            : ""}
        </p>
        <div className="ag-flags mt-2">
          <Checkbox
            checked={ui?.config.enable_i18n ?? true}
            disabled={!ui}
            onChange={(v) => void saveFlag({ enable_i18n: v })}
          >
            界面汉化
          </Checkbox>
          <Checkbox
            checked={ui?.config.auto_accept ?? true}
            disabled={!ui}
            onChange={(v) => void saveFlag({ auto_accept: v })}
          >
            自动审批
          </Checkbox>
          <Checkbox
            checked={ui?.config.block_dangerous ?? true}
            disabled={!ui || !ui.config.auto_accept}
            onChange={(v) => void saveFlag({ block_dangerous: v })}
          >
            高危拦截
          </Checkbox>
        </div>
        <div className="ustats mt-2">
          <div className="ustat">
            <span className="ustat-name">已放行</span>
            <span className="ustat-value">{ui?.approve_count ?? 0}</span>
          </div>
          <div className="ustat">
            <span className="ustat-name">已拦截</span>
            <span className="ustat-value">{ui?.block_count ?? 0}</span>
          </div>
        </div>
        <div className="usage-footer">
          <span className="notice">
            字典 {ui?.dict_entries ?? 0} 条 · 规则 {ui?.rules_on ?? 0}/
            {ui?.rules_total ?? 0} 条 · 取自 EasyAntigravity（MIT）·
            只对面板起的那份生效
          </span>
          <Button
            size="sm"
            onClick={() => {
              setEngineOpen(false);
              navigate("/extensions/antigravity-ui");
            }}
          >
            规则与日志
          </Button>
        </div>
      </Modal>
      <Modal
        open={adding}
        onClose={() => !busy && setAdding(false)}
        title="新建反重力账户"
      >
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void create();
          }}
          className="flex flex-col gap-3"
        >
          <label htmlFor="ag-slot-label">账户名称</label>
          <input
            id="ag-slot-label"
            className="input"
            value={label}
            maxLength={40}
            autoFocus
            onChange={(e) => setLabel(e.target.value)}
            placeholder="例如：工作账户"
          />
          <p className="notice">
            {
              "两半一起建：IDE 的资料目录与 Gemini CLI 的 home。下一步用它启动反重力 IDE，在 IDE 自己的窗口里用 Google 登录；CLI 那一半在列表里单独点「登录」。两处都不需要填写密码或粘贴 Token；面板只在你点额度刷新时读一次它们存在本机的令牌（在内存里用，不写回）。新账户只复制你的设置与快捷键，不复制登录。"
            }
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
          ask?.action === "launch"
            ? `启动${ask.product === "hub" ? "反重力" : "反重力 IDE"}？`
            : ask?.action === "close-all"
              ? "关闭反重力？"
              : ask?.action === "ide-switch"
                ? `切换到 ${ask.label}？`
                : ask?.action === "ide-archive"
                  ? `移除 ${ask.label}？`
                  : ask?.action === "attach"
                    ? `把「${ask.label}」并进「${ask.intoLabel}」？`
                    : ""
        }
        confirmLabel={
          ask?.action === "launch"
            ? "关闭旧实例并启动"
            : ask?.action === "close-all"
              ? "确认关闭"
              : ask?.action === "ide-switch"
                ? "切换"
                : ask?.action === "attach"
                  ? "并成一条"
                  : "移除并保留归档"
        }
        danger
      >
        {/* 这个框只在**真的有东西在跑**的时候才弹（见 requestLaunch），
            所以正文可以直接报数，不用再写「如果有的话」这种模棱两可的话。 */}
        <p>
          {ask?.action === "launch"
            ? `${ask.running === null ? "查不出现在有没有在跑（照样会先关）。" : ask.running > 0 ? `现在有 ${ask.running} 个进程在跑，` : ""}会先关掉正在跑的同一个程序（它是单实例，不退干净新起的只会把旧窗口拉到前面），正在运行的任务会中断，请先保存。归门禁时会先验出口 IP。`
            : ask?.action === "close-all"
              ? `${ask.running > 0 ? `现在有 ${ask.running} 个进程在跑。` : "查不出现在有没有在跑（照关不误）。"}Hub 与 IDE 的所有窗口及正在运行的任务都会关掉，请先保存工作。登录资料保留在它们自己那里。`
              : ask?.action === "ide-switch"
                ? "只换激活账户，不关不起任何东西：正在跑的 IDE 继续用它起来时那份资料，下次从面板启动 IDE 才用这一条。IDE 与 Gemini CLI 两半一起换；酒馆的 Gemini 桥接在跑时不能切，停酒馆后再来。"
                : ask?.action === "ide-archive"
                  ? "账户从列表移除，它的资料目录（含登录状态、设置）保留在本机归档中。从旧清单升上来的那些目录留在原地不动。"
                  : ask?.action === "attach"
                    ? "只把两条索引合成一条，两边的目录都留在原地、一个字节都不动。并错了再建一条指回去就行。面板不替你认哪两条是同一个 Google 账户 —— 这一步由你确认。"
                    : ""}
        </p>
        {error && (
          <p role="alert" className="notice notice--danger mt-2">
            {error}
          </p>
        )}
      </ConfirmDialog>
    </>
  );
}

/** 时间档。`0` = 全部。比 Claude 那张多一档 30 天 —— 反重力的额度是按周期重置的，看一个月更有用。 */
const RANGES = [
  [1, "今天"],
  [7, "7 天"],
  [30, "30 天"],
  [0, "全部"],
] as const;

/**
 * 反重力的本机用量 + IDE 写下的账户状态与配额（0.30.0）。
 *
 * 四格是「输入 / 输出 / 缓存命中率 / 剩余配额」。头条不放「总 token」——
 * 实测这台机器上缓存读占 95%（21 亿里 20 亿），把几类加起来当「用了多少」，
 * 读到的是一个被缓存读淹掉的数字。「缓存省下」的美元与按模型明细在弹窗里。
 *
 * 配额那一格：当前账户点过刷新（联网）就显示四格里最紧的那一格；没点过就是 IDE 写在本机的
 * **剩余最低的那一族模型**（每族三个推理档实测配额一样，合并显示）—— 那份只有 IDE 上次同步时
 * 那么新，IDE 那一格的悬停写「IDE 写入 hh:mm」，不说成此刻。
 */
function AntigravityUsageCard({
  identity,
  source,
  identityError,
  quotaKey,
  activeOnline,
  hubLoggedIn,
  hubIdentity,
  hubIdentityError,
}: {
  identity: AntigravityIdentity | null;
  source: string;
  identityError: string | null;
  quotaKey: string | null;
  /** 当前账户最近一次联网问到的额度（在账户行上点刷新图标问的）。没问过就是 `null`。 */
  activeOnline: AntigravityOnlineQuota | null;
  /** Hub 登没登录（凭据管理器里那条在不在）。**不读内容。** */
  hubLoggedIn: boolean;
  /** Hub 登的是谁。邮箱是本机从 `id_token` 解出来的，零网络请求。 */
  hubIdentity: AntigravityHubIdentity | null;
  hubIdentityError: string | null;
}) {
  const [days, setDays] = useState<number>(1);
  const [data, setData] = useState<AntigravityUsage | null>(null);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const [geminiQuota, setGeminiQuota] = useState<TavernGeminiQuota | null>(
    null,
  );
  const [geminiQuotaError, setGeminiQuotaError] = useState("");
  const [geminiBusy, setGeminiBusy] = useState(false);
  const load = useCallback(
    async (signal?: { cancelled: boolean }) => {
      setBusy(true);
      setErr("");
      try {
        const r = await antigravityApi.usage(days);
        if (!signal?.cancelled) {
          setData(r);
          setNow(Date.now());
        }
      } catch (e) {
        if (!signal?.cancelled)
          setErr(e instanceof Error ? e.message : String(e));
      } finally {
        if (!signal?.cancelled) setBusy(false);
      }
    },
    [days],
  );
  useEffect(() => {
    setData(null);
    const signal = { cancelled: false };
    void load(signal);
    // ⛔ 这个定时器是这张卡的正确性的一部分：一个叫「今天」的数字不会自己往前走。
    const timer = window.setInterval(() => void load(signal), 60_000);
    return () => {
      signal.cancelled = true;
      window.clearInterval(timer);
    };
  }, [load]);

  // Gemini CLI 的联网额度：⛔ 只手动刷新（2026-09-23 使用者定的）。
  // 挂载、切账户时只读「最近一次」问到的（`false`，后端绝不联网）；那一格的「刷新」才联网。
  useEffect(() => {
    let cancelled = false;
    // 账户切换后旧槽位的额度不能继续占位，否则会把上一账户的剩余额度
    // 误显示在当前账户上；未知要保持未知，不能折算成 0。
    setGeminiQuota(null);
    setGeminiQuotaError("");
    void api
      .tavernGeminiQuota(false)
      .then((quota) => {
        if (!cancelled) setGeminiQuota(quota);
      })
      .catch(() => {
        // 只读内存那一条不会因为网络失败；读不到就当没问过。
      });
    return () => {
      cancelled = true;
    };
  }, [quotaKey]);
  const refreshGemini = useCallback(async () => {
    setGeminiBusy(true);
    setGeminiQuotaError("");
    try {
      setGeminiQuota(await api.tavernGeminiQuota(true));
    } catch (e) {
      setGeminiQuotaError(e instanceof Error ? e.message : String(e));
    } finally {
      setGeminiBusy(false);
    }
  }, []);

  const summary = data?.summary;
  const hit =
    summary?.hit_rate == null ? null : `${Math.round(summary.hit_rate * 100)}%`;
  const low = useMemo(
    () => (identity ? lowestQuota(identity.models) : null),
    [identity],
  );
  // 当前账户联网问到的：四格里最紧的那一格；免费档没有四格，就看按模型最低的那一族。
  const onlineTight = useMemo(
    () => (activeOnline ? tightestWindow(activeOnline.windows) : null),
    [activeOnline],
  );
  const onlineLowModel = useMemo(
    () => (activeOnline ? lowestQuota(activeOnline.models) : null),
    [activeOnline],
  );
  const cliLow = useMemo(
    () =>
      (geminiQuota?.models ?? [])
        .filter((model) => model.remaining_percent != null)
        .sort(
          (a, b) => (a.remaining_percent ?? 101) - (b.remaining_percent ?? 101),
        )[0],
    [geminiQuota],
  );
  const identityLine = identityError
    ? `账户状态读不出来：${identityError}`
    : identity
      ? `${identity.email ?? "未读到邮箱"}${identity.tier_name ? ` · ${identity.tier_name}` : ""} · 读自${source} · IDE 写入 ${identity.written_at}`
      : `${source}还没登录过反重力 IDE：登录后这里显示邮箱与配额`;

  // ---------------------------------------------------- Hub 那一行（0.32.0）
  //
  // ⛔ **Hub 与 IDE 是两个账户、两套额度，绝不合并成一个数。** 合并就是编一个
  // 不存在的数字：使用者完全可以在 Hub 里登 A、在 IDE 槽位里登 B。
  //
  // 邮箱来自凭据里 `id_token` 的载荷（本机 base64 解码，零网络）；
  // 档位与配额是 Hub 唯一没写在本机的东西 —— 那一格单独去问，见 `hubQuota`。
  const [hubQuota, setHubQuota] = useState<AntigravityOnlineQuota | null>(null);
  const [hubErr, setHubErr] = useState("");
  const [hubBusy, setHubBusy] = useState(false);
  /** `refresh: false` 只读「最近一次」（不联网）；`true` 才问 Google —— 只从那一格的「刷新」来。 */
  const loadHub = useCallback(async (refresh: boolean) => {
    setHubBusy(true);
    setHubErr("");
    try {
      const q = await antigravityApi.hubQuota(refresh);
      // 只读内存没问到，就别把上一次问到的冲掉。
      if (q || refresh) setHubQuota(q);
    } catch (e) {
      setHubErr(e instanceof Error ? e.message : String(e));
    } finally {
      setHubBusy(false);
    }
  }, []);
  // 凭据读不出来不是「没登录」（§7.17）：那时 `hubLoggedIn` 是 false，但刷新照样给 ——
  // 原来这一格写「未登录」、把刷新藏起来，悬停里却说「读不出来」（2026-09-25）。
  const hubUnreadable = !hubLoggedIn && !!hubIdentityError;
  useEffect(() => {
    // ⛔ 挂载时只读「最近一次」，**不联网、没有定时器** —— 会对外发请求的动作不许自己发生。
    if (hubLoggedIn || hubUnreadable) void loadHub(false);
  }, [hubLoggedIn, hubUnreadable, loadHub]);
  /**
   * Hub 那一格问到了什么。按 0 推出来的那一格要说出来（Google 的 JSON 把 0 省掉了）；
   * 免费档没有四格，就按模型分两组给（原来免费档问到了按模型的，一个数都不显示，2026-09-25）。
   */
  const hubQuotaText = (q: AntigravityOnlineQuota): string => {
    if (q.windows.length)
      return q.windows
        .map(
          (w) =>
            `${groupLabel(w.group)} ${spanLabel(w.span)} ${percent(w.remaining)}${w.remaining_implied ? "（没给比例，按用光算）" : ""}`,
        )
        .join(" · ");
    return [
      ...modelGroups(q.models).map(
        ({ group, model }) =>
          `${groupLabel(group)} ${percent(model.remaining)}${model.remaining_implied ? "（没给比例，按用光算）" : ""}`,
      ),
      q.windows_note ?? "",
    ]
      .filter(Boolean)
      .join(" · ");
  };
  const hubLine = hubIdentityError
    ? `Hub 的登录读不出来：${hubIdentityError}`
    : !hubLoggedIn
      ? "Hub 还没登录过（凭据管理器里没有它那一条）"
      : `${hubIdentity?.email ?? "未读到邮箱"}${
          hubQuota?.tier_name ? ` · ${hubQuota.tier_name}` : ""
        } · Hub 的登录（本机解码）${
          hubQuota
            ? ` · 额度问于 ${hubQuota.fetched_at}${
                hubQuotaText(hubQuota) ? ` · ${hubQuotaText(hubQuota)}` : ""
              }${hubQuota.credits != null ? ` · AI 积分 ${NUM.format(hubQuota.credits)}` : ""}`
            : " · 点「刷新」联网查额度"
        }`;

  return (
    <Card className="account-usage ag-usage">
      <div className="usage-heading">
        <h2 className="card-title" title="反重力用量">
          <Gauge size={14} aria-hidden="true" />
          反重力用量
        </h2>
        <span className="usage-ranges">
          {RANGES.map(([d, name]) => (
            <Button
              key={d}
              size="sm"
              variant={days === d ? "primary" : "default"}
              disabled={busy}
              onClick={() => setDays(d)}
            >
              {name}
            </Button>
          ))}
          <Button
            size="sm"
            icon={<RefreshCw size={12} />}
            loading={busy}
            aria-label="立刻重算反重力用量"
            title="立刻重算本机用量（不联网）。后台每分钟也会自己读一次。联网额度在下面各格与账户行上的刷新里。"
            onClick={() => void load()}
          />
        </span>
      </div>
      {err && <p className="notice notice--danger">{err}</p>}
      <div className="quota-sources" aria-label="额度来源">
        <span
          className={`quota-source${identityError ? " qb-tone-warn" : ""}`}
          title={identityLine}
        >
          <strong>IDE</strong>
          <span>
            {identityError
              ? quotaErrorLabel(identityError)
              : identity
                ? "本地"
                : "未登录"}
          </span>
        </span>
        <span
          className={`quota-source${hubErr ? " qb-tone-warn" : ""}`}
          title={hubErr || hubLine}
        >
          <strong>Hub</strong>
          <span>
            {hubErr
              ? quotaErrorLabel(hubErr)
              : hubQuota
                ? "在线"
                : hubUnreadable
                  ? "读不出来"
                  : !hubLoggedIn
                    ? "未登录"
                    : hubBusy
                      ? "读取中…"
                      : "未读取"}
          </span>
          {(hubLoggedIn || hubUnreadable) && !hubBusy && (
            <button
              type="button"
              className="btn btn--ghost btn--sm !px-1 !py-0"
              aria-label="刷新 Hub 额度"
              title={hubErr || "联网问一次 Hub 的额度（只问这一次）"}
              onClick={() => void loadHub(true)}
            >
              刷新
            </button>
          )}
        </span>
        <span
          className={`quota-source${geminiQuotaError ? " qb-tone-warn" : ""}`}
          title={
            geminiQuotaError ||
            (geminiQuota
              ? `Gemini CLI 联网额度 · 问于 ${geminiQuota.fetched_at}`
              : "Gemini CLI 的联网额度还没问过 · 点「刷新」问一次")
          }
        >
          <strong>Gemini CLI</strong>
          <span>
            {geminiQuotaError
              ? quotaErrorLabel(geminiQuotaError)
              : geminiQuota
                ? "在线"
                : geminiBusy
                  ? "读取中…"
                  : "未读取"}
          </span>
          {!geminiBusy && (
            <button
              type="button"
              className="btn btn--ghost btn--sm !px-1 !py-0"
              aria-label="刷新 Gemini CLI 额度"
              title="联网问一次 Gemini CLI 的额度（只问这一次）"
              onClick={() => void refreshGemini()}
            >
              刷新
            </button>
          )}
        </span>
      </div>
      {/* ⛔ 四格要说**四件事**（0.32.0 改的）。
          原来是「输入 / 输出 / 缓存命中率 / 剩余配额」—— 前两格是同一个量的两半，
          各占一格却看不出量级；而命中率那一格的副行写着「读 0」，跟它自己重复。
          现在：总量 / 回复数 / 命中率（副行说省下多少）/ 剩余配额。 */}
      <div className="ustats">
        <div className="ustat">
          <span className="ustat-name">Token 总量</span>
          <span className="ustat-value">
            {summary
              ? short(summary.input + summary.output + summary.cache_read)
              : "—"}
          </span>
          {summary && (
            <span className="ustat-sub">
              入 {short(summary.input)} · 出 {short(summary.output)} · 缓存{" "}
              {short(summary.cache_read)}
            </span>
          )}
        </div>
        <div className="ustat">
          <span className="ustat-name">回复数</span>
          <span className="ustat-value">
            {summary ? NUM.format(summary.messages) : "—"}
          </span>
          {summary && (
            <span className="ustat-sub">
              {RANGES.find(([d]) => d === days)?.[1] ?? ""} · Hub 与 IDE 合计
            </span>
          )}
        </div>
        <div className="ustat">
          <span className="ustat-name">缓存命中率</span>
          <span className="ustat-value">{hit ?? "—"}</span>
          {summary && (
            <span className="ustat-sub">
              {/* 「缓存省下的」不是「你花了多少」—— 算式与口径在明细里写着。 */}
              {summary.saved_usd == null
                ? "省下多少算不出来"
                : `省下 ≈$${summary.saved_usd.toFixed(2)}`}
            </span>
          )}
        </div>
        {/* 先后顺序：当前账户联网问到的四格（最紧那一格）→ 免费档按模型 → Gemini CLI →
            IDE 写在本机的。联网那几份都只在点过刷新之后才有，没点过就照旧是本机那份。 */}
        <div className="ustat" data-testid="ag-quota">
          <span className="ustat-name">剩余配额</span>
          <span className="ustat-value">
            {onlineTight
              ? percent(onlineTight.remaining)
              : onlineLowModel
                ? percent(onlineLowModel.remaining)
                : cliLow
                  ? `${cliLow.remaining_percent}%`
                  : low
                    ? percent(low.remaining)
                    : "—"}
          </span>
          {/* 按 0 推出来的（Google 只给了重置时刻）要说出来，不许装成读到的 0（2026-09-25）。 */}
          <span className="ustat-sub">
            {onlineTight
              ? `${groupLabel(onlineTight.group)} ${spanLabel(onlineTight.span)} · ${countdown(onlineTight.reset_epoch, now)}${onlineTight.remaining_implied ? " · 没给比例，按用光算" : ""}`
              : onlineLowModel
                ? `${onlineLowModel.family} · ${resetIn(onlineLowModel.reset_epoch, now)}${onlineLowModel.implied ? " · 没给比例，按用光算" : ""}`
                : cliLow
                  ? `${cliLow.label} · ${cliLow.reset_at ?? "重置时间未知"}${cliLow.remaining_implied ? " · 没给比例，按用光算" : ""}`
                  : low
                    ? `${low.family} · ${resetIn(low.reset_epoch, now)}`
                    : geminiQuotaError
                      ? geminiQuotaError
                      : identity
                        ? "IDE 没写下额度信息"
                        : "IDE 登录后可见"}
          </span>
        </div>
      </div>
      {geminiQuota?.models.length ? (
        <div className="slotusage">
          {geminiQuota.models.map((model) => (
            <QuotaBar
              // 汇总接口的桶没有 model_id / token_type（2026-09-25 起不再被合成一个），
              // 键要带上名字，不然几格全是 `null-null`。
              key={`${model.model_id}-${model.token_type}-${model.label}`}
              name={model.label}
              used={
                model.remaining_percent == null
                  ? null
                  : 100 - model.remaining_percent
              }
              // 按对象认是不是最紧的那一格：按 model_id 比，没有 id 的几格会全被标成「卡这儿」。
              binding={cliLow === model}
              extra={
                model.reset_at ? (
                  <span
                    className="gauge-reset"
                    title={
                      model.remaining_implied
                        ? "Google 没给这一格的比例 —— 它的 JSON 会把 0 省掉，按用光算"
                        : undefined
                    }
                  >
                    ↻ {model.reset_at}
                    {model.remaining_implied && <em>推算</em>}
                  </span>
                ) : undefined
              }
            />
          ))}
        </div>
      ) : null}
      {/* 全零有两种：真没跑过，和读坏了。分开说 —— 「0」在界面上读起来是一句
          斩钉截铁的事实，使用者只会以为统计坏了。 */}
      <div className="usage-footer">
        <span className="notice">
          {data && data.files_failed > 0
            ? `${data.files_failed} 个记录库没读成 · 统计不完整`
            : data && data.files_read === 0
              ? "本机还没有反重力的对话记录"
              : "本机记录 · Hub 与 IDE 合计 · 不分账户 · 每分钟刷新"}
        </span>
        {/* 0.32.0：明细搬到可滚动的 `/usage` 上去了 ——
            这一页是固定高度的，趋势图与按模型分布摆不下。 */}
        <Link to="/usage?side=antigravity" className="btn btn--sm btn--ghost">
          用量明细
        </Link>
      </div>
    </Card>
  );
}
