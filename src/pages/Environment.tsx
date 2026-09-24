/**
 * 软件页（0.19.0 重写）。
 *
 * # 一张卡回答三件事
 *
 * 四张软件卡，每张三栏，位置固定：**版本**（升级、渠道、版本库回滚）、
 * **门禁**（开关）、**卸载**（完全卸载 + 卸载提示词）。Chrome 那张把「门禁」
 * 换成「隐私审计」。这是使用者选定的方案 A 第二版。
 *
 * 改版之前这一页的毛病是**同一件事散在三处**：多余副本在「安装」卡底部、
 * 卸载重装提示词在「版本库与回滚」卡里、Chrome 重装在「以前装过吗」卡里。
 * 现在按「哪个软件」分，不按「哪种操作」分。
 *
 * # ⛔ 不编造数字
 *
 * 卡上不显示「本机 N 处」，直到真跑过一次只读盘点。盘点要起子进程、读注册表，
 * 四张卡进页面各跑一次是不能接受的开销；而编一个数字比不显示糟得多。
 * 点了「完全卸载」→ 先只读盘点 → 把每一处连同归属依据摆出来 → 才谈删。
 *
 * # ⛔ 「没查」不显示成「没问题」
 *
 * 隐私审计里 `unchecked` 那一串必须显示出来。Chrome 正跑着时痕迹扫不了、
 * 扩展启用与否判不了、注册表里有值不等于策略生效 —— 这三件事都要说出口，
 * 跟 `chrome_scanned` 那条是同一条规矩。
 *
 * # 时区与语言不在这一页
 *
 * 移到了「设置 › 启动时对齐」，面板启动时自动执行。这一页不再有时区卡。
 */

import { useState, type ReactNode } from "react";
import {
  Download,
  Lock,
  RotateCw,
  ShieldAlert,
  Terminal,
  Trash2,
} from "lucide-react";

import {
  api,
  type Channel,
  type InstallTarget,
  type PurgeItem,
  type PurgeReport,
  type PurgeTarget,
  type Risk,
  type SoftwareReport,
} from "../lib/api";
import type { Software } from "../lib/generated/Software";
import {
  NOT_YET_READ,
  pillLabel,
  pillTone,
  versionLine,
} from "../lib/software";
import type { ScanResult } from "../lib/signals";
import { markStep } from "../lib/progress";
import { usePending } from "../lib/ipc";
import { AFTER, R } from "../lib/resources";
import {
  invalidate,
  peek,
  refresh,
  setSession,
  useResource,
  useSession,
} from "../lib/store";
import { SIDE_KEY, type Side } from "../lib/side";
import { useNavigate } from "react-router-dom";
import { endTask, resetTask, useTask, type TaskState } from "../lib/tasks";
import { antigravityApi, PRODUCT_LABEL } from "../lib/antigravity";
import type { AntigravityProduct } from "../lib/generated/AntigravityProduct";

/** winget 上的官方包 id。跟 Rust 侧 `antigravity_setup::winget_id` 是同一对，
    只出现在确认框的说明文字里 —— 真正拿它去装的是后端那一份。 */
const AG_WINGET_ID: Record<AntigravityProduct, string> = {
  hub: "Google.Antigravity",
  ide: "Google.AntigravityIDE",
};
import { codexApi } from "../lib/codexAccounts";
import {
  CLEAN_REINSTALL_PROMPT,
  CODEX_REINSTALL_PROMPT,
  BROWSER_REINSTALL_PROMPT,
  type PromptDef,
} from "../prompts";
import {
  Button,
  Card,
  Checkbox,
  CodeBlock,
  Collapsible,
  ConfirmDialog,
  ExternalLink,
  LogView,
  Modal,
  PathField,
  Pill,
  ProgressBar,
  Row,
  TextField,
  useToast,
  CHANNEL_LABEL,
  INSTALL_TARGET_LABEL,
  POLICY_SCOPE_LABEL,
  PURGE_ACTION_LABEL,
  PURGE_CATEGORY_LABEL,
  PURGE_TARGET_LABEL,
  UPGRADE_ACTION_LABEL,
  UPGRADE_ACTION_TONE,
} from "../ui";
import {
  ExternalsBlock,
  ManagedDirControl,
  VersionHistoryBlock,
} from "./managed/ManagedPanel";

const CHROME_PAGE = "https://www.google.com/chrome/";
const CODEX_STORE_PAGE = "https://apps.microsoft.com/detail/9plm9xgg6vks";

/**
 * 「卸载提示词」按软件选。0.28.0 之前只分 chrome / 其它，其它一律给 Claude 那份 ——
 * 于是 Codex 卡点出来的是一份通篇 Claude 路径、跟 Codex 无关的提示词。
 * 反重力 / Gemini 没有对应的官方重装流程，这两张卡不给提示词按钮，所以这里不会收到它们。
 */
function promptFor(target: PurgeTarget): PromptDef {
  switch (target) {
    case "chrome":
      return BROWSER_REINSTALL_PROMPT;
    case "codex":
    case "codex-desktop":
      return CODEX_REINSTALL_PROMPT;
    default:
      return CLEAN_REINSTALL_PROMPT;
  }
}

/**
 * 卡片右上角那颗「装没装」的 Pill。
 *
 * ⛔ **每一张卡都用它**，别再各写各的三元表达式。文案与色调都来自
 * `lib/software.ts`，跟版本行同源 —— 这是「Pill 和版本行互相矛盾」那一类 bug
 * （0.28.0 之前四张卡都有）唯一治得住的办法。
 */
function SwPill({ sw, prefix }: { sw: Software | undefined; prefix?: string }) {
  return (
    <Pill tone={pillTone(sw)}>
      {prefix ? `${prefix} ` : ""}
      {pillLabel(sw)}
    </Pill>
  );
}

/**
 * 长任务的进度条 + 日志。
 *
 * ⛔ **要画在按下去的那张卡里。** 0.28.0 之前 `install` 这个任务的进度块只渲染在
 * 第一张卡（Claude Code），而 Codex CLI 与 Claude 桌面端的「安装」按钮走的是同一个
 * 任务名 —— 点的是第三张卡，进度条出现在页面最上面那张卡里，按钮那一带一动不动。
 * 所以共用任务名的几张卡要用 `show` 指明「这一次是谁在跑」。
 */
function TaskBlock({
  task,
  show = true,
  label = "安装进度",
}: {
  task: TaskState;
  show?: boolean;
  label?: string;
}) {
  if (!show || (!task.running && task.log.length === 0)) return null;
  return (
    <div className="mt-3">
      <ProgressBar
        value={task.total > 0 ? (task.step / task.total) * 100 : undefined}
        tone={task.error ? "danger" : "accent"}
        label={label}
      />
      <p className="notice mt-1">{task.phase}</p>
      <LogView lines={task.log} follow={task.running} />
    </div>
  );
}

/**
 * 「卸载提示词」按钮 + 它下面那句说明。
 *
 * ⛔ 一份就够。0.28.0 之前 Chrome 那一列自己抄了一份 `UninstallCol`，
 * 于是同一个按钮有两套说明：短版「兜面板够不到的地方」、长版还点名了
 * WSL 与其它 Windows 用户账户。两句话说的是同一件事，而短的那句**没说清**
 * 到底够不到哪里 —— 卡上那句话本来就是为了回答这个。
 */
function PromptButton({
  onPrompt,
  disabled,
}: {
  onPrompt: () => void;
  disabled?: boolean;
}) {
  return (
    <div className="mt-3 border-t border-line pt-3">
      <Button
        size="sm"
        icon={<Terminal size={13} />}
        disabled={disabled}
        onClick={onPrompt}
      >
        卸载提示词
      </Button>
      <p className="notice mt-1">
        给别的 AI 用，兜面板够不到的地方：WSL 发行版、其它 Windows 用户账户。
      </p>
    </div>
  );
}

/** 卡内一栏。三栏之间用左边框分隔，窄屏时自然堆叠。 */
function Col({ label, children }: { label: string; children: ReactNode }) {
  return (
    <section className="min-w-0 border-line px-1 py-2 lg:border-l lg:px-4 lg:first:border-l-0 lg:first:pl-0">
      <span className="notice mb-2 block font-mono uppercase tracking-wide">
        {label}
      </span>
      {children}
    </section>
  );
}

export default function Environment() {
  const toast = useToast();
  const navigate = useNavigate();

  const sw = useResource("software", R.software);
  const managed = useResource("managed", R.managed);
  const install = useResource("install", R.install);
  // ⛔ 渠道要在 `useResource` 之前拿到：升级计划是**按渠道**查的。
  // 写死 latest 的那一版，下拉选 stable 之后「检查」查的仍是 latest，
  // 而「升级」按 stable 走 —— 界面报的版本和实际要装的版本是两回事。
  const [channel, setChannel] = useSession<Channel>("env.channel", "latest");
  const upgrade = useResource("upgrade", R.upgradeOf(channel));
  const hook = useResource("hook", R.hook);
  const settings = useResource("settings", R.settings);
  const traces = useResource("traces", R.traces);
  const audit = useResource("browserAudit", R.browserAudit);
  const rules = useResource("firewallRules", R.firewallRules);
  const adapters = useResource("adapters", R.adapters);
  const proxy = useResource("proxy", R.proxy);
  const proxyBackup = useResource("proxyBackup", R.proxyBackup);

  const installTask = useTask("install");
  const upgradeTask = useTask("upgrade");
  const chromeTask = useTask("chrome-reinstall");
  const codexDesktopTask = useTask("install-codex-desktop");
  const antigravityTask = useTask("install-antigravity");
  const geminiTask = useTask("install-gemini-cli");

  /**
   * `install` 这一个任务名被三张卡共用（Claude Code / Codex CLI / Claude 桌面端）。
   * 记下这一次是谁按的，进度条才画得回按钮旁边 —— 见 [`TaskBlock`] 的说明。
   */
  const [installOwner, setInstallOwner] = useState<InstallTarget | null>(null);

  const [busy, setBusy] = useState("");
  const [prompt, setPrompt] = useState<PurgeTarget | null>(null);
  /** 有命令等了太久 —— 用来把「我在等后端」说出口，见下面那条横幅。 */
  const slow = usePending();

  // Codex 桌面端直装（0.28.0）：确认框里的两个选项 + 「检查」查到的 Store 版本。
  const [askCodexDesktop, setAskCodexDesktop] = useState(false);
  const [codexForce, setCodexForce] = useState(false);
  const [codexLocal, setCodexLocal] = useState("");
  const [codexLatest, setCodexLatest] = useState<string | null>(null);

  // 反重力一键安装（0.29.0）。一次只装一个产品，所以确认框记着是哪一个。
  const [askAntigravity, setAskAntigravity] = useState(false);
  const [agProduct, setAgProduct] = useState<AntigravityProduct>("hub");
  const [agForce, setAgForce] = useState(false);
  const [agLocal, setAgLocal] = useState("");
  const [agLatest, setAgLatest] = useState<{
    hub?: string;
    ide?: string;
  } | null>(null);

  // 卸载三部曲：盘点 → 确认 → 报告。任一步都可能停下来，所以分三个状态。
  const [plan, setPlan] = useState<{
    target: PurgeTarget;
    items: PurgeItem[];
  } | null>(null);
  const [report, setReport] = useState<{
    target: PurgeTarget;
    r: PurgeReport;
  } | null>(null);

  const [askChrome, setAskChrome] = useState(false);
  const [askFirewall, setAskFirewall] = useState(false);
  const [picked, setPicked] = useState<string[]>([]);
  const [askProxy, setAskProxy] = useState(false);
  const [proxyDraft, setProxyDraft] = useState({ enabled: false, server: "" });

  const tr = traces.data;
  const au = audit.data;

  /**
   * Chrome 装没装、装在哪。
   *
   * ⛔ **不要只看 `tr` / `au`。** 那两份是 `auto: false` 的（要起子进程，
   * 进页面不跑），所以在使用者按「扫描」之前它们都是 `undefined` ——
   * 卡片于是在一台**装着 Chrome** 的机器上大写着「未装」、位置写「还没扫过」。
   * 使用者原话：「Google Chrome 扫描不行，我的电脑都没扫出来」。
   *
   * 而面板其实早就知道答案：`detect::browsers()` 里的 chrome 条目走的是
   * 同一个 `chrome::chrome_exe()`，跟着 `software` 自动取回来了 ——
   * 另外三张卡读的就是它。这里补上同一个来源，三者取第一个有值的：
   * 扫过之后用扫的结果，没扫过就用 `software`。
   */
  const chromeSw = sw.data?.browsers.find((b) => b.id === "chrome");
  const chromeInstalled =
    au?.chrome_installed ?? tr?.chrome_installed ?? chromeSw?.installed;
  const chromePath =
    au?.chrome_path ?? tr?.chrome_path ?? chromeSw?.path ?? null;

  const traceHit = !!tr?.traces.some((t) => t.kind === "browser");
  const locked = tr?.chrome_files_locked ?? 0;
  const [traceText, traceTone]: [string, "ok" | "warn" | "danger" | "default"] =
    !tr
      ? ["还没扫", "warn"]
      : traceHit
        ? ["Cookie / 历史里有", "danger"]
        : chromeInstalled === false
          ? ["没装 Chrome", "default"]
          : !tr.chrome_scanned
            ? ["一个文件都没读开", "warn"]
            : locked > 0
              ? [`没扫到（${locked} 个没读开）`, "warn"]
              : ["没扫到", "ok"];

  async function recordRisk() {
    const scan = peek<ScanResult>("signals");
    const s = peek<SoftwareReport>("software");
    const risk: Risk =
      scan?.band === "high"
        ? "high"
        : scan?.band === "medium" || !s?.claudeCode.installed
          ? "medium"
          : "low";
    await markStep(
      "environment",
      risk === "high" ? "failed" : "passed",
      risk,
      [
        s?.claudeCode.installed ? "Claude Code 已装" : "Claude Code 未装",
        s?.codex.installed ? "Codex 已装" : "Codex 未装",
        scan ? `中文环境 ${scan.total}/100` : null,
      ]
        .filter(Boolean)
        .join("；"),
    );
  }

  /** 重新扫描。**顺带记录这一步的结果** —— 原来那个页底按钮去掉了，
      但新手引导的 `environment` 这一步仍然要有人记，不然引导永远卡着。 */
  async function rescan() {
    await Promise.all([sw.refresh(), install.refresh(), managed.refresh()]);
    await recordRisk();
  }

  /**
   * 换升级渠道。
   *
   * # ⛔ 不能 `setChannel(x)` 之后直接 `upgrade.refresh()`
   *
   * `setChannel` 排的是下一次渲染，而资源定义是在渲染里注册的 ——
   * 同一个事件里调 `refresh()` 用的还是**旧渠道**那份定义。0.28.0 之前就是这么写的：
   * 选 stable、查回来的是 latest 的计划、界面当成 stable 显示。不报错，只是在说谎。
   *
   * 所以这里现造一份新渠道的定义传给 `refresh`。另外**先把旧渠道那份结论扔掉** ——
   * 在新结果回来之前，下拉写着 stable 而读数是 latest 的，那一瞬间同样是谎话；
   * `upgrade` 是 `auto:false` 的手动资源，`invalidate` 对它是「扔掉」，界面退回
   * 「还没查过版本」（同 `store.ts` 里那条「没查不显示成没问题」）。
   */
  function changeChannel(next: Channel) {
    const checkedBefore = upgrade.data !== undefined;
    setChannel(next);
    invalidate("upgrade");
    // 之前查过才自动再查一遍；没查过就保持「还没查过」，别替使用者起子进程。
    if (checkedBefore) void refresh("upgrade", R.upgradeOf(next));
  }

  async function runInstall(target: InstallTarget) {
    setBusy(target);
    setInstallOwner(target);
    resetTask("install");
    try {
      const r = await api.installRun(target);
      endTask("install", r.ok ? undefined : r.detail);
      if (r.ok) {
        toast.ok(
          `${INSTALL_TARGET_LABEL[target]} 安装完成，重新上锁 ${r.relocked} 个副本`,
        );
        if (r.signature_ok === false) {
          toast.error("注意：新文件的签名主体里没有 Anthropic，请自行核实来源");
        }
      } else {
        toast.error(r.detail);
      }
      invalidate(...AFTER.install);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask("install", msg);
      toast.error(msg);
    } finally {
      setBusy("");
    }
  }

  async function runUpgrade() {
    setBusy("upgrade");
    resetTask("upgrade");
    try {
      toast.ok(await api.upgradeExecute(channel, false));
      endTask("upgrade");
      invalidate(...AFTER.install);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask("upgrade", msg);
      toast.error(msg);
    } finally {
      setBusy("");
    }
  }

  /** 第一步：只读盘点。**什么都不删。** */
  async function startPurge(target: PurgeTarget) {
    setBusy(`plan-${target}`);
    try {
      const items = await api.purgePlan(target);
      // 盘点是空的就别开确认框。四张卡的「完全卸载」在没装那个软件时
      // 一样点得动，开出来是一个写着「下面 0 处会被清掉」、还要求你
      // 打「卸载」两个字的对话框 —— 打完什么也不会发生。
      if (items.length === 0) {
        toast.ok(
          `没扫到任何属于${PURGE_TARGET_LABEL[target]}的东西，没有可清的。`,
        );
        return;
      }
      setPlan({ target, items });
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  }

  /** 第二步：执行。返回里的 `left` 是复扫之后还剩下的。 */
  async function runPurge() {
    if (!plan) return;
    const target = plan.target;
    setBusy("purge");
    try {
      const r = await api.purgeExecute(target);
      setReport({ target, r });
      // `traces` / `browserAudit` 是手动扫的：卸载完那两份答案描述的是
      // 一台已经不存在的机器，`invalidate` 会把它们整个扔掉（store.ts）。
      invalidate(
        ...AFTER.install,
        ...AFTER.gate,
        "accounts",
        "traces",
        "browserAudit",
      );
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      // ⛔ 成功失败都关。这一页原来三个弹窗三套生命周期：卸载只在成功时关、
      // Chrome 在 finally 里关、Codex 桌面端只在成功时关。失败时那个框留在
      // 屏幕上，正好压着刚弹出来的错误提示 —— 使用者看不到失败原因，
      // 只看到一个还开着的确认框，多半会再点一次。
      setPlan(null);
      setBusy("");
    }
  }

  /**
   * 装（或更新）Codex 桌面端。后端会**先关掉正在跑的桌面端**（Store 包在跑时更新会失败），
   * 所以走确认框，代价在框里说清。装完把账户页那份 `codexDesktop` 也作废 —— 那边是另一个
   * 资源键，`AFTER.install` 管不到它。
   */
  async function runCodexDesktopInstall() {
    setBusy("codex-desktop");
    resetTask("install-codex-desktop");
    try {
      const msg = await codexApi.install(codexLocal.trim() || null, codexForce);
      endTask("install-codex-desktop");
      toast.ok(msg);
      invalidate(...AFTER.install, "codexDesktop");
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask("install-codex-desktop", msg);
      toast.error(msg);
    } finally {
      // 成功失败都关 —— 跟 `runPurge` / `runChromeReinstall` 一套规矩。
      setAskCodexDesktop(false);
      setBusy("");
    }
  }

  /**
   * 一键装反重力（0.29.0）。后端会**先关掉正在跑的那一份**（Electron 单实例 +
   * 托盘后台运行，开着装不上），所以走确认框，代价在框里说清。
   *
   * 装完 `AFTER.install` 之外还要作废 `antigravity`（账户页的资源键；原来写成了不存在的
   * `antigravityStatus`，这一步从来没生效过）—— 账户页那一侧读的是
   * 另一个资源键，不作废的话那边会继续显示「未安装 · 到软件页装」。
   */
  async function runAntigravityInstall() {
    setBusy("antigravity");
    resetTask("install-antigravity");
    try {
      const msg = await antigravityApi.install(
        agProduct,
        agLocal.trim() || null,
        agForce,
      );
      endTask("install-antigravity");
      toast.ok(msg);
      invalidate(...AFTER.install, "antigravity");
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask("install-antigravity", msg);
      toast.error(msg);
    } finally {
      // ⛔ 成功失败都关。0.28.0 之前这一页三个弹窗三套生命周期
      //（有的只在成功时关、有的在 finally 里关），失败之后那个框留在屏幕上
      // 挡着下面的错误提示。
      setAskAntigravity(false);
      setBusy("");
    }
  }

  /** 官网上现在是哪一版。只读下载页，不下载不安装。 */
  async function checkAntigravityLatest() {
    setBusy("antigravity-latest");
    try {
      const [hub, ide] = await Promise.all([
        antigravityApi.latest("hub").catch(() => undefined),
        antigravityApi.latest("ide").catch(() => undefined),
      ]);
      setAgLatest({ hub, ide });
      if (!hub && !ide) toast.error("读不到官网的版本号，请稍后再试。");
    } finally {
      setBusy("");
    }
  }

  /**
   * `npm install -g @google/gemini-cli`。0.29.0 起**等它装完**。
   *
   * 原来这里只是让后端弹一个 cmd 窗口，然后立刻 toast「已打开安装窗口」并
   * `invalidate(["software"])` —— 而那个窗口里 npm 从来没跑起来过（`/k` 被加了引号），
   * 那句提示和那次重新检测都是在 npm 还没动的时候发生的。两个都去掉了。
   */
  async function runGeminiCliInstall() {
    setBusy("gemini-cli");
    resetTask("install-gemini-cli");
    try {
      const msg = await antigravityApi.geminiCliInstall();
      endTask("install-gemini-cli");
      toast.ok(msg);
      invalidate(...AFTER.install, "antigravity");
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask("install-gemini-cli", msg);
      toast.error(msg);
    } finally {
      setBusy("");
    }
  }

  /** Store 上现在是哪一版。只查元数据（FE3），不下载不安装。 */
  async function checkCodexDesktopLatest() {
    setBusy("codex-desktop-latest");
    try {
      setCodexLatest(await codexApi.latest());
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  }

  async function runChromeReinstall() {
    setBusy("chrome");
    resetTask("chrome-reinstall");
    try {
      toast.ok(await api.chromeReinstall());
      endTask("chrome-reinstall");
      invalidate(...AFTER.browser);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask("chrome-reinstall", msg);
      toast.error(msg);
    } finally {
      setBusy("");
      setAskChrome(false);
    }
  }

  /**
   * 一次「点一下 → 调后端 → 把界面对齐」。
   *
   * # ⛔ 失败也要作废
   *
   * 这里的操作**不都是原子的**：加出站锁是点名的每块网卡各加一条，
   * 第三条失败时前两条已经在系统里拦着流量了。只在成功时作废的话，
   * 规则表上一条都不出现 —— 使用者以为什么都没发生，而浏览器已经
   * 被拦了两块网卡，且这规则不随面板消失。
   *
   * # 返回成不成功
   *
   * 给调用方决定要不要关窗。失败了还把窗关掉，等于把错误信息和使用者
   * 刚勾好的那一排网卡一起扔了，只能从头再来一遍。
   */
  async function act(
    key: string,
    fn: () => Promise<string>,
    after: readonly string[],
  ): Promise<boolean> {
    setBusy(key);
    try {
      const msg = await fn();
      invalidate(...after);
      toast.ok(msg);
      return true;
    } catch (e) {
      invalidate(...after);
      toast.error(e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      setBusy("");
    }
  }

  const up = upgrade.data;
  const managedOf = (t: string) => managed.data?.apps.find((a) => a.app === t);
  /** 确认框里那个产品此刻装没装 —— 标题与「危险」样式都看它。 */
  const agInstalled = !!(agProduct === "hub"
    ? sw.data?.antigravity.installed
    : sw.data?.antigravityIde.installed);

  /**
   * 「纳入 IP 门禁」那颗复选框。**三张卡共用这一份。**
   *
   * ⛔ Codex CLI 与 Codex 桌面端写的是**同一个设置键** `codex_outside_gate`
   * （同一个 OpenAI 账户，只管一个就是给另一个留门）。0.28.0 之前两张卡各抄了一份
   * 27 行的 JSX，连 `act` 的 key 都不一样（`codex-gate` / `codex-desktop-gate`）——
   * 改一处漏一处只是时间问题，而症状会是「在这张卡上关掉，那张卡还写着开着」。
   *
   * 反义存法（`*_outside_gate` 默认 false = 归门禁）是升级路径，别改成正着存：
   * 旧配置文件里没有这个键，读出来就是 `false`，于是所有人升级后自动落到「归门禁」。
   */
  function gateToggle(
    field: "codex_outside_gate" | "antigravity_outside_gate",
    label: string,
  ) {
    return (
      <Checkbox
        checked={!settings.data?.[field]}
        disabled={!!busy || !settings.data}
        onChange={(v) =>
          void act(
            `gate:${field}`,
            () =>
              api
                .settingsSave({ ...settings.data!, [field]: !v })
                .then(() => `${label} ${v ? "已纳入" : "已移出"} IP 门禁`),
            ["settings", "gate"],
          )
        }
      >
        纳入 IP 门禁
      </Checkbox>
    );
  }

  return (
    <>
      <header className="qb-page-heading">
        <div>
          <h1>软件</h1>
          <p>装进托管目录、升级、管门禁、彻底卸载。</p>
        </div>
        <Button
          icon={<RotateCw size={13} />}
          loading={sw.loading}
          disabled={!!busy}
          onClick={() => void rescan()}
        >
          重新扫描
        </Button>
      </header>

      {/* ⛔ 后端可以慢，界面不许既不动也不说话。
          `operations::exclusive()` 是一把无界的锁：排在长安装后面的命令会一直等，
          而前端这边的表现就是「按钮转圈、什么都不发生、没有报错」——
          0.22.6–0.27.0 那次托管安装死锁（坑 7.51）之所以几个版本没人发现，
          正是因为它长得跟「正常但很慢」一模一样。这条横幅把「我在等」说出口。
          ⚠ 这里**不加超时取消**：`invoke` 取消不了，超时只会让 busy 提前清空、
          按钮亮回来，使用者再点一次 —— 反而制造出两个互斥操作同时在跑。 */}
      {slow && (
        <p className="notice notice--warn mb-3">
          还在等后端回应（<code>{slow.cmd}</code>，已{" "}
          {Math.round(slow.ms / 1000)}{" "}
          秒）。安装、下载、卸载这类操作本来就慢；如果同时还有别的操作在跑，
          这一条会排在它后面。
        </p>
      )}

      <ManagedDirControl disabled={!!busy} />

      {/* ------------------------------------------------ Claude Code */}
      <Card
        title="Claude Code"
        className="mb-3"
        actions={
          <>
            <SwPill sw={sw.data?.claudeCode} />
            <Button
              size="sm"
              icon={<Download size={13} />}
              loading={busy === "claude-code"}
              disabled={!!busy}
              onClick={() => void runInstall("claude-code")}
            >
              {managedOf("claude-code")?.installed ? "重新下载安装" : "安装"}
            </Button>
          </>
        }
      >
        <p className="notice mb-2 break-words">
          从官方源下载，核对 SHA-256 与 Anthropic
          签名后放进托管目录，装完自动重新上锁。
          {managedOf("claude-code")?.path && (
            <>
              {" "}
              当前：<code>{managedOf("claude-code")?.path}</code>
            </>
          )}
        </p>

        <div className="grid gap-1 lg:grid-cols-3">
          <Col label="版本">
            {up ? (
              <>
                <div className="flex flex-wrap items-baseline gap-2">
                  <span className="metric-v mono">
                    {up.installed || "未安装"}
                  </span>
                  {up.available && (
                    <span className="mono text-accent">→ {up.available}</span>
                  )}
                  <Pill tone={UPGRADE_ACTION_TONE[up.action]}>
                    {UPGRADE_ACTION_LABEL[up.action]}
                  </Pill>
                </div>
                <p className="notice mt-1">{up.detail}</p>
              </>
            ) : (
              <p className="notice">还没查过版本。</p>
            )}
            <div className="mt-2 flex flex-wrap items-center gap-2">
              <select
                className="input w-auto"
                aria-label="升级渠道"
                value={channel}
                disabled={!!busy}
                onChange={(e) => changeChannel(e.target.value as Channel)}
              >
                <option value="latest">{CHANNEL_LABEL.latest}</option>
                <option value="stable">{CHANNEL_LABEL.stable}</option>
              </select>
              <Button
                size="sm"
                loading={upgrade.loading}
                disabled={!!busy}
                onClick={() => void upgrade.refresh()}
              >
                检查
              </Button>
              <Button
                size="sm"
                variant="primary"
                loading={busy === "upgrade"}
                disabled={
                  !!busy ||
                  !up ||
                  up.action === "up_to_date" ||
                  up.action === "would_downgrade"
                }
                onClick={() => void runUpgrade()}
              >
                升级
              </Button>
            </div>
            <div className="mt-3">
              <VersionHistoryBlock disabled={!!busy} />
            </div>
          </Col>

          <Col label="门禁">
            <div className="flex items-center gap-2">
              <Lock size={13} aria-hidden="true" />
              <strong className="text-sm">执行锁</strong>
              <Pill tone="ok">始终开</Pill>
            </div>
            <p className="notice mt-1">
              每一份 claude.exe
              都必须锁上，漏掉一份就是一个绕过门禁的入口，所以没有开关。
            </p>
            <div className="mt-3 border-t border-line pt-3">
              <Checkbox
                checked={!!hook.data?.installed}
                disabled={!!busy}
                onChange={(v) =>
                  void act(
                    "hook",
                    () =>
                      (v ? api.hookInstall() : api.hookUninstall()).then(
                        (s) => `会话内门禁已${s.installed ? "开启" : "关闭"}`,
                      ),
                    ["hook"],
                  )
                }
              >
                会话内门禁
              </Checkbox>
              <p className="notice mt-1">
                每次请求发出前再验一次出口 IP。
                <strong>白名单为空时装不上</strong> —— 装上等于每次请求都被拦。
              </p>
            </div>
            {!!sw.data?.claudeCodeInstalls.length && (
              <Collapsible
                className="mt-3"
                summary={`本机共 ${sw.data.claudeCodeInstalls.length} 份副本`}
              >
                {sw.data.claudeCodeInstalls.map((i) => (
                  <Row
                    key={i.path}
                    side={
                      i.lockable ? (
                        <Pill tone="ok">锁得到</Pill>
                      ) : (
                        <Pill tone="warn">锁不到</Pill>
                      )
                    }
                  >
                    <span className="notice block w-full break-all font-mono">
                      {i.path}
                    </span>
                  </Row>
                ))}
              </Collapsible>
            )}
          </Col>

          <UninstallCol
            target="claude-code"
            busy={busy}
            onPlan={startPurge}
            onPrompt={setPrompt}
            note="会先关闭全部 Claude（未保存的对话会丢）。账户槽位一起清 —— 清完所有账户都要重新登录。"
          />
        </div>

        <TaskBlock task={installTask} show={installOwner === "claude-code"} />
        <TaskBlock task={upgradeTask} label="升级进度" />
        <ExternalsBlock disabled={!!busy} />
      </Card>

      {/* ------------------------------------------------------ Codex */}
      <Card
        title="Codex CLI"
        className="mb-3"
        actions={
          <>
            <SwPill sw={sw.data?.codex} />
            <Button
              size="sm"
              icon={<Download size={13} />}
              loading={busy === "codex"}
              disabled={!!busy}
              onClick={() => void runInstall("codex")}
            >
              {managedOf("codex")?.installed ? "重新下载安装" : "安装"}
            </Button>
          </>
        }
      >
        <div className="grid gap-1 lg:grid-cols-3">
          <Col label="版本">
            {/* ⛔ 跟右上角的 Pill 同源（`lib/software.ts`）。0.28.0 之前这里只读托管那份
                的记录：npm / winget 装的 Codex，Pill 写着「已装 0.x」、这一栏却写着
                「未安装」，一张卡两个答案。托管那份的版本更准（读的是安装记录），
                有就优先用它，没有就退回同一个渲染函数。 */}
            <span className="metric-v mono">
              {managedOf("codex")?.version ?? versionLine(sw.data?.codex)}
            </span>
            {sw.data?.codex.installed && !managedOf("codex")?.installed && (
              <p className="notice mt-1 break-words">
                这一份不是面板装的：<code>{sw.data.codex.path}</code>
                。点「安装」会另装一份托管的，之后启动用托管那份。
              </p>
            )}
            <p className="notice mt-1 break-words">
              从 <code>github.com/openai/codex</code> 取最新发布，核对 OpenAI
              签名后装进托管目录。 升级就是再点一次「重新下载安装」，旧版留底。
            </p>
          </Col>

          <Col label="门禁">
            {gateToggle("codex_outside_gate", "Codex")}
            <p className="notice mt-1">
              打开后 codex.exe 和 claude.exe 一样上锁：出口 IP
              不在白名单时直接跑不起来。
              <strong>默认开。</strong>
            </p>
            <p className="notice mt-2">
              打开之后，npm 装的 <code>codex.cmd</code> 仍然管不到 —— 批处理由
              cmd.exe 读进去执行，给它加 Deny ExecuteFile 挡不住。
            </p>
          </Col>

          <UninstallCol
            target="codex"
            busy={busy}
            onPlan={startPurge}
            onPrompt={setPrompt}
            note="正在跑的 Codex 要自己先关掉 —— 面板不按进程名杀进程，正在运行的那份删不掉，会如实出现在复扫结果里。账户槽位不在内：Codex 桌面端也在用它们的登录身份。"
          />
        </div>
        <TaskBlock task={installTask} show={installOwner === "codex"} />
      </Card>

      {/* ------------------------------------------- Codex 桌面端（0.28.0） */}
      <Card
        title="Codex 桌面端（Microsoft Store）"
        className="mb-3"
        actions={
          <>
            {/* 「查不到」是第五档：PowerShell 起不来时后端给 advisory 且 installed=false。
                那既不是「装了」也不是「没装」，不能折进去（§7.17）。 */}
            {!sw.data?.codexDesktop.installed &&
            sw.data?.codexDesktop.advisory &&
            !sw.data.codexDesktop.version ? (
              <Pill tone="warn">查不到</Pill>
            ) : (
              <SwPill sw={sw.data?.codexDesktop} />
            )}
            <Button
              size="sm"
              icon={<Download size={13} />}
              loading={busy === "codex-desktop"}
              disabled={!!busy}
              onClick={() => {
                setCodexForce(false);
                setCodexLocal("");
                setAskCodexDesktop(true);
              }}
            >
              {sw.data?.codexDesktop.installed ? "更新 / 重装" : "安装"}
            </Button>
          </>
        }
      >
        <p className="notice mb-2 break-words">
          账户页起的就是它（跟上面的 Codex CLI 是两个东西）。面板不打开 Store
          也能装：先走 winget 的 Store 源，没成就直连微软的分发接口取官方 MSIX，
          核对清单 SHA-256 与 OpenAI 签名后 <code>Add-AppxPackage</code>{" "}
          正规注册 —— 装出来的就是 Store 那个包，自动更新、<code>codex://</code>{" "}
          都照旧。
          {sw.data?.codexDesktop.advisory &&
            ` ${sw.data.codexDesktop.advisory}`}
        </p>

        <div className="grid gap-1 lg:grid-cols-3">
          <Col label="版本">
            <div className="flex flex-wrap items-baseline gap-2">
              <span className="metric-v mono">
                {versionLine(sw.data?.codexDesktop)}
              </span>
              {codexLatest && (
                <span className="mono text-accent">Store 上 {codexLatest}</span>
              )}
            </div>
            {/* ⛔ 没有路径 ≠ 包不在册。0.28.0 之前这里无条件断言
                「Get-AppxPackage 里没有 OpenAI.Codex」—— 而包在册、只是找不到 exe 的
                那一档（注册坏了）里，这句话是**假的**，跟旁边显示的真版本号直接打架。 */}
            <p className="notice mt-1 break-words">
              {sw.data?.codexDesktop.path ? (
                <span className="mono">{sw.data.codexDesktop.path}</span>
              ) : sw.data?.codexDesktop.installed ? (
                "包在册，但找不到它的可执行文件。"
              ) : (
                "未检测到 Store 包（Get-AppxPackage 里没有 OpenAI.Codex）。"
              )}
            </p>
            <div className="mt-2 flex flex-wrap items-center gap-2">
              <Button
                size="sm"
                loading={busy === "codex-desktop-latest"}
                disabled={!!busy}
                onClick={() => void checkCodexDesktopLatest()}
              >
                检查 Store 版本
              </Button>
              <ExternalLink href={CODEX_STORE_PAGE}>Store 页面</ExternalLink>
            </div>
            <p className="notice mt-2">
              「检查」只问微软的目录接口，不下载、不安装。
              更新后新包的打包服务要管理员注册，装的时候可能弹一次 UAC。
            </p>
          </Col>

          <Col label="门禁">
            {gateToggle("codex_outside_gate", "Codex")}
            <p className="notice mt-1">
              跟 Codex CLI 是<strong>同一个开关</strong>
              （同一个 OpenAI
              账户，只管一个就是给另一个留门）。打开后账户页起它之前先验出口
              IP，看门狗按桌面档盯，查不到 IP 立即关闭。
              <strong>默认开。</strong>
            </p>
            <p className="notice mt-2">
              Store 包在 <code>WindowsApps</code> 下，加不了执行锁 —— 这一档跟
              Claude 桌面端的 app-&lt;版本&gt; 一样只能靠看门狗兜。
            </p>
          </Col>

          <UninstallCol
            target="codex-desktop"
            busy={busy}
            onPlan={startPurge}
            onPrompt={setPrompt}
            note="Store 包走 Remove-AppxPackage，开着的桌面端会先被关掉。清的是包和它的数据（Packages 目录、每个 GPT 槽位的桌面端资料）；槽位里的登录身份（home）不动 —— Codex CLI 也在用它。"
          />
        </div>

        <TaskBlock task={codexDesktopTask} />
      </Card>

      {/* ------------------------------------------------- 桌面端 */}
      <Card
        title="Claude 桌面端"
        className="mb-3"
        actions={
          <>
            <SwPill sw={sw.data?.claudeDesktop} />
            <Button
              size="sm"
              icon={<Download size={13} />}
              loading={busy === "claude-desktop"}
              disabled={!!busy || install.data?.winget_available === false}
              onClick={() => void runInstall("claude-desktop")}
            >
              {sw.data?.claudeDesktop.installed ? "重新安装" : "安装"}
            </Button>
          </>
        }
      >
        <div className="grid gap-1 lg:grid-cols-3">
          <Col label="版本">
            <span className="metric-v mono">
              {versionLine(sw.data?.claudeDesktop)}
            </span>
            <p className="notice mt-1 break-words">
              位置被官方安装器写死（<code>%LOCALAPPDATA%\AnthropicClaude</code>
              ），它还会自己在 那里更新 ——
              面板接管不了它的目录，所以没有升级按钮，也没有版本库。
              {/* ⛔ `=== false` 不是 `!`。`install` 还没回来时那个字段是 undefined，
                  用 `!` 会在**还不知道**的时候就断言「本机没有 winget」并把按钮灰掉。 */}
              {install.data?.winget_available === false &&
                " 本机没有 winget，只能到官方页面手动装。"}
            </p>
          </Col>

          <Col label="门禁">
            <div className="flex items-center gap-2">
              <Lock size={13} aria-hidden="true" />
              <strong className="text-sm">执行锁</strong>
              <Pill tone="ok">始终开</Pill>
            </div>
            <p className="notice mt-1">
              存根会上锁。<strong>本体 app-&lt;版本&gt; 加不了锁</strong>
              （加了开新窗口就崩），
              那一档靠看门狗兜：查不到合格出口就关掉它。这是设计使然的已知缺口。
            </p>
          </Col>

          <UninstallCol
            target="claude-desktop"
            busy={busy}
            onPlan={startPurge}
            onPrompt={setPrompt}
            note="程序走 winget 卸载，不是逐个删文件 —— 官方卸载器会连启动器与登记一起走。开着的桌面端会先被关掉。"
          />
        </div>
        <TaskBlock
          task={installTask}
          show={installOwner === "claude-desktop"}
        />
      </Card>

      {/* ------------------------------------------------- 反重力（0.26.0） */}
      <Card
        title="反重力（Google Antigravity）"
        className="mb-3"
        actions={
          <>
            <SwPill sw={sw.data?.antigravity} prefix="Hub" />
            <SwPill sw={sw.data?.antigravityIde} prefix="IDE" />
            <Button
              size="sm"
              icon={<Download size={13} />}
              loading={busy === "antigravity"}
              disabled={!!busy}
              onClick={() => {
                setAgProduct("hub");
                setAgForce(false);
                setAgLocal("");
                setAskAntigravity(true);
              }}
            >
              {sw.data?.antigravity.installed ? "更新 / 重装" : "安装"}
            </Button>
          </>
        }
      >
        <div className="grid gap-1 lg:grid-cols-3">
          <Col label="安装">
            <p className="notice break-words">
              面板<strong>不分发 Google 的安装包</strong>：先走 winget
              的官方包（
              <code>Google.Antigravity</code> /{" "}
              <code>Google.AntigravityIDE</code>
              ），没有 winget 就从 Google 自己的下载域取官方安装器，核过
              Authenticode 签名主体含 Google 之后静默安装 —— 装出来的跟你自己去
              <ExternalLink href="https://antigravity.google/download">
                官网点下载
              </ExternalLink>
              是同一个文件。
            </p>
            <p className="notice mt-2 break-words">
              {sw.data?.antigravity.path ? (
                <span className="mono">{sw.data.antigravity.path}</span>
              ) : (
                "Hub：未检测到"
              )}
              {agLatest?.hub && (
                <span className="text-accent"> · 官网 {agLatest.hub}</span>
              )}
            </p>
            <p className="notice mt-1 break-words">
              {sw.data?.antigravityIde.path ? (
                <span className="mono">{sw.data.antigravityIde.path}</span>
              ) : (
                "IDE：未检测到"
              )}
              {agLatest?.ide && (
                <span className="text-accent"> · 官网 {agLatest.ide}</span>
              )}
            </p>
            <div className="mt-2 flex flex-wrap items-center gap-2">
              <Button
                size="sm"
                loading={busy === "antigravity-latest"}
                disabled={!!busy}
                onClick={() => void checkAntigravityLatest()}
              >
                检查官网版本
              </Button>
              <Button
                size="sm"
                icon={<Download size={13} />}
                loading={busy === "antigravity"}
                disabled={!!busy}
                onClick={() => {
                  setAgProduct("ide");
                  setAgForce(false);
                  setAgLocal("");
                  setAskAntigravity(true);
                }}
              >
                {sw.data?.antigravityIde.installed
                  ? "更新 / 重装 IDE"
                  : "安装 IDE"}
              </Button>
            </div>
            <UninstallInline
              target="antigravity"
              busy={busy}
              onPlan={startPurge}
              note="Hub 与 IDE 一起卸（同一个 Google 账户）：先关掉正在跑的，删两个安装目录、~\.gemini\antigravity*、%APPDATA%\Antigravity*，以及凭据管理器里名字含 antigravity 的条目 —— 名字不含它的条目面板认不出，卸完自己去凭据管理器核一眼。"
            />
          </Col>

          <Col label="门禁">
            {gateToggle("antigravity_outside_gate", "反重力")}
            <p className="notice mt-1">
              Hub 与 IDE 共用这一个开关（同一个 Google
              账户，只锁一个就是给另一个留门）：
              主程序、语言服务器、第三方汉化壳留下的 <code>*.original.exe</code>
              一起加执行锁，出口 IP
              不在白名单时跑不起来；起来之后看门狗按桌面档盯， 查不到 IP
              立即关闭。<strong>默认开。</strong>
            </p>
            <p className="notice mt-2">
              它自动更新装完的新程序在下一次上锁之前没有执行锁 ——
              在账户页「反重力」 栏可以关掉 Hub
              的自动检查更新（只并入它自己的一个设置键，重启 Hub 生效）。
            </p>
          </Col>

          <Col label="账户与汉化">
            <p className="notice">
              启动、关闭、汉化 / 自动审批 / 高危拦截、酒馆的 Gemini 桥接都在
              「官方账户 · 反重力」那一侧。反重力<strong>没有中转路径</strong>
              （API 端点写死在客户端里、只认 Google 登录），中转站页不会列它。
            </p>
            <Button
              size="sm"
              className="mt-2"
              onClick={() => {
                setSession<Side>(SIDE_KEY, "antigravity");
                navigate("/");
              }}
            >
              去「官方账户 · 反重力」
            </Button>
          </Col>
        </div>
        <TaskBlock task={antigravityTask} />
      </Card>

      {/* ------------------------------------------------- Gemini CLI（0.26.0） */}
      <Card
        title="Gemini CLI"
        className="mb-3"
        actions={
          <>
            <SwPill sw={sw.data?.geminiCli} />
            <Button
              size="sm"
              icon={<Download size={13} />}
              loading={busy === "gemini-cli"}
              disabled={!!busy}
              onClick={() => void runGeminiCliInstall()}
            >
              {sw.data?.geminiCli.installed ? "用 npm 重装" : "用 npm 安装"}
            </Button>
          </>
        }
      >
        <div className="grid gap-1 lg:grid-cols-3">
          <Col label="版本">
            <span className="metric-v mono">
              {versionLine(sw.data?.geminiCli)}
            </span>
            <p className="notice mt-1 break-words">
              Google 官方的 <code>@google/gemini-cli</code>（Node 包）。 酒馆的
              Gemini 桥接每个请求起一次它的无交互模式 ——
              反重力本体没有公开的无交互 CLI，接酒馆的是它，不是反重力本体。
              需要本机有 Node.js；没有的话安装会当场说出来，不会让你对着一个
              空窗口猜。
              {sw.data?.geminiCli.advisory
                ? ` ${sw.data.geminiCli.advisory}`
                : ""}
            </p>
            <UninstallInline
              target="gemini-cli"
              busy={busy}
              onPlan={startPurge}
              note="npm 卸掉 @google/gemini-cli、删托管目录里那份，连同所有 Gemini 账户槽位（酒馆的 Gemini 桥接用的就是它们，清完都要重新登录）。不动 ~\.gemini 本身 —— 反重力的数据也在那底下。"
            />
          </Col>
          <Col label="门禁">
            <p className="notice">
              它是 <code>node</code> 跑的脚本，跟 npm 装的{" "}
              <code>codex.cmd</code>
              一样加不了执行锁；桥接起它之前按反重力那一档验
              IP，出口不合格时看门狗 连桥接一起收。
            </p>
          </Col>
          <Col label="登录">
            <p className="notice">
              槽位与登录在「官方账户 · 反重力」：每个槽位是一个{" "}
              <code>GEMINI_CLI_HOME</code>，CLI 自己把 Google
              登录写在里面，面板只看凭据文件在不在。
            </p>
          </Col>
        </div>
        <TaskBlock task={geminiTask} />
      </Card>

      {/* ------------------------------------------------------ Chrome */}
      <Card
        title="Google Chrome"
        className="mb-3"
        actions={
          <>
            {/* Chrome 这一张不走 `SwPill`：它的「装没装」是三个来源兜出来的
                （审计 / 痕迹 / software），不是一条 `Software`。三档还是同一套：
                装了 / 没装 / 还没读到。 */}
            {chromeInstalled ? (
              <Pill tone="ok">已装</Pill>
            ) : (
              <Pill tone={chromeInstalled === undefined ? "warn" : "default"}>
                {chromeInstalled === undefined ? NOT_YET_READ : "未装"}
              </Pill>
            )}
            {/* ⛔ 扫描也要让 busy 挡住。在 Chrome 清空重装 / 完全卸载跑着的时候扫，
                扫的是一台正在被改的机器 —— 读出来的那份审计描述的是哪一刻，
                没人说得清。 */}
            <Button
              size="sm"
              loading={traces.loading || audit.loading}
              disabled={!!busy}
              onClick={() => {
                void traces.refresh();
                void audit.refresh();
              }}
            >
              扫描
            </Button>
          </>
        }
      >
        <div className="grid gap-1 lg:grid-cols-3">
          <Col label="位置">
            <span className="notice block break-all font-mono">
              {chromePath ??
                (chromeInstalled === false ? "本机没装 Chrome" : NOT_YET_READ)}
            </span>
            <p className="notice mt-2">
              Chrome 自己更新，面板不管它的版本。
              <strong>只碰 Google Chrome</strong>—— Edge、Firefox
              及其它浏览器一概不动。面板自己跑在 WebView2 上， 卸掉 Chrome
              不影响这个面板。
            </p>
          </Col>

          <Col label="隐私审计">
            {!au && !tr ? (
              <p className="notice">
                还没扫过。点右上角「扫描」—— 只读，不改任何东西。
              </p>
            ) : (
              /* ⛔ 这里不许用 `grid`。栏目是三分之一宽（1280px 下内容区 271px），
                  而 `display:grid` 不写 `grid-template-columns` 时只有一条隐式
                  `auto` 轨道 —— `auto` 的上限是 **max-content**，轨道会长到
                  「行里所有东西一个字都不折」那么宽（实测 325px）再溢出容器，
                  而不是反过来逼行里的东西收缩。于是每一行都往右吐出 57px，
                  盖到隔壁「卸载」栏的正文上，右边那颗按钮也跟着跑过去；
                  值那一格的 `truncate` 因为轨道够宽而永远不触发。
                  普通块级容器没有这个毛病：宽度就是栏宽，行内自己收缩。 */
              <div>
                {/* 这一行有**五种**结局，不许塌成「有 / 没有」两种：
                    还没扫 · 扫出来了 · 没装 Chrome · 一个文件都没读开 ·
                    读了一部分但没找到。后三种都不是「没问题」。 */}
                <AuditRow k="claude.ai 痕迹" tone={traceTone} v={traceText} />
                <AuditRow
                  k="WebRTC"
                  tone={
                    au?.webrtc?.value === "disable_non_proxied_udp"
                      ? "ok"
                      : "warn"
                  }
                  v={
                    au?.webrtc
                      ? `${au.webrtc.value} · ${POLICY_SCOPE_LABEL[au.webrtc.scope]}`
                      : "未设策略"
                  }
                  action={
                    au?.webrtc?.value === "disable_non_proxied_udp" ? (
                      <Button
                        size="sm"
                        variant="ghost"
                        loading={busy === "webrtc"}
                        disabled={!!busy}
                        onClick={() =>
                          void act(
                            "webrtc",
                            () => api.browserWebrtcClear(),
                            [],
                          ).then((ok) => {
                            if (ok) void audit.refresh();
                          })
                        }
                      >
                        撤销
                      </Button>
                    ) : (
                      <Button
                        size="sm"
                        loading={busy === "webrtc"}
                        disabled={!!busy}
                        onClick={() =>
                          // ⚠ 重扫，不是 `invalidate` —— 审计是手动资源，作废等于
                          // 整栏退回「还没扫过」，而使用者点的就是这一行，
                          // 他要看的正是这一行变没变。
                          void act(
                            "webrtc",
                            () => api.browserWebrtcHarden(),
                            [],
                          ).then((ok) => {
                            if (ok) void audit.refresh();
                          })
                        }
                      >
                        收紧
                      </Button>
                    )
                  }
                />
                <AuditRow
                  k="安全 DNS"
                  tone={au?.doh ? "ok" : "default"}
                  v={
                    au?.doh
                      ? `${au.doh.value} · ${POLICY_SCOPE_LABEL[au.doh.scope]}`
                      : "未设策略"
                  }
                />
                <AuditRow
                  k="系统代理"
                  tone={proxy.data?.enabled || proxy.data?.pac ? "warn" : "ok"}
                  v={
                    proxy.data
                      ? proxy.data.enabled
                        ? (proxy.data.server ?? "开着")
                        : proxy.data.pac
                          ? "挂着 PAC"
                          : "没开"
                      : "读不出"
                  }
                  action={
                    <Button
                      size="sm"
                      disabled={!!busy}
                      onClick={() => {
                        setProxyDraft({
                          enabled: !!proxy.data?.enabled,
                          server: proxy.data?.server ?? "",
                        });
                        setAskProxy(true);
                      }}
                    >
                      修改
                    </Button>
                  }
                />
                <AuditRow
                  k="扩展权限"
                  tone={
                    au?.extensions.some((e) => e.risky.length) ? "danger" : "ok"
                  }
                  v={
                    au
                      ? `${au.extensions.filter((e) => e.risky.length).length} / ${au.extensions.length} 有高危权限`
                      : "还没扫"
                  }
                />
                <AuditRow
                  k="出站锁"
                  tone={rules.data?.length ? "accent" : "default"}
                  v={rules.data ? `${rules.data.length} 条规则` : "没查"}
                  action={
                    <Button
                      size="sm"
                      disabled={!!busy}
                      onClick={() => {
                        void rules.refresh();
                        void adapters.refresh();
                        setPicked([]);
                        setAskFirewall(true);
                      }}
                    >
                      管理
                    </Button>
                  }
                />
              </div>
            )}

            {!!au?.extensions.some((e) => e.risky.length) && (
              <Collapsible className="mt-2" summary="看看是哪几个扩展">
                {au.extensions
                  .filter((e) => e.risky.length)
                  .map((e) => (
                    <Row
                      key={e.id}
                      side={<Pill tone="warn">{e.risky.length} 项</Pill>}
                    >
                      <span>{e.name ?? e.id}</span>
                      <span className="notice block w-full">
                        {e.risky.join("、")}
                      </span>
                      <span className="notice block font-mono">
                        {e.profile} · {e.version}
                      </span>
                    </Row>
                  ))}
                <p className="notice mt-2">
                  <strong>面板只报告，不替你禁用或删除。</strong>
                  看完自己去 <code>chrome://extensions</code> 处理。
                </p>
              </Collapsible>
            )}

            {/* ⛔「没查」不能显示成「没问题」。 */}
            {!!au?.unchecked.length && (
              <div className="mt-2">
                {au.unchecked.map((u) => (
                  <p key={u} className="notice notice--warn">
                    {u}
                  </p>
                ))}
              </div>
            )}
            {/* ⛔「没查」不能显示成「没问题」。
                Chrome 开着也照扫了（见 `chrome.rs` 的「Chrome 开着照扫」），
                所以这里不再按「开没开」说话，按**这一轮读开了几个**说话。 */}
            {tr && chromeInstalled && !tr.chrome_scanned && (
              <p className="notice notice--warn mt-2">
                <strong>一个 Chrome 资料文件都没读开，这一项没扫成。</strong>
                这不等于「没有痕迹」。常见原因：资料目录不在默认位置（自定义
                <code>--user-data-dir</code>）、或者被别的程序锁着。
              </p>
            )}
            {tr && tr.chrome_scanned && locked > 0 && (
              <p className="notice notice--warn mt-2">
                <strong>
                  读开 {tr.chrome_files_read} 个，还有 {locked} 个没读开
                  {tr.chrome_running ? "（Chrome 正开着）" : ""}。
                </strong>
                上面那句结论只覆盖读开的那部分。要扫全，关掉 Chrome 再扫一次。
              </p>
            )}
          </Col>

          <Col label="卸载">
            <p className="notice">
              完全卸载会清掉程序和整个 <code>User Data</code>
              ：书签、保存的密码、扩展、
              <strong>全部网站</strong>的登录态都会没，<strong>不可恢复</strong>
              。
            </p>
            <div className="mt-2 flex flex-wrap gap-2">
              <Button
                size="sm"
                variant="danger"
                icon={<Trash2 size={13} />}
                loading={busy === "plan-chrome"}
                disabled={!!busy}
                onClick={() => void startPurge("chrome")}
              >
                完全卸载
              </Button>
              {/* winget 在不在：`install` 是进页面就自动取的，`traces` 要按「扫描」才有。
                  0.28.0 之前只看 `tr` —— 扫描之前这里永远只给一条手动链接，
                  哪怕 winget 明明在。装没装 Chrome 也走顶上那条三源兜底链，不单看 `tr`。 */}
              {(install.data?.winget_available ?? tr?.winget_available) ? (
                <Button
                  size="sm"
                  variant="danger"
                  loading={busy === "chrome"}
                  disabled={!!busy}
                  onClick={() => setAskChrome(true)}
                >
                  {chromeInstalled === false ? "安装 Chrome" : "清空并重装"}
                </Button>
              ) : (
                <ExternalLink href={CHROME_PAGE}>
                  到官方页面手动处理
                </ExternalLink>
              )}
            </div>
            <PromptButton
              onPrompt={() => setPrompt("chrome")}
              disabled={!!busy}
            />
            <TaskBlock task={chromeTask} label="Chrome 重装进度" />
          </Col>
        </div>
      </Card>

      {/* ---------------------------------------------------- 提示词 */}
      {/* 弹窗，不是页底的一张卡（0.28.0）。原来内联在整页最下面：点顶上 Claude Code 卡的
          「卸载提示词」，内容出现在六张卡之后，多半看不见它弹出来了。
          `Modal` 只在 open 时挂载，所以 `prompt` 为 null 时下面什么都不渲染。 */}
      <Modal
        open={!!prompt}
        onClose={() => setPrompt(null)}
        title={prompt ? promptFor(prompt).title : ""}
        size="wide"
        footer={<Button onClick={() => setPrompt(null)}>关闭</Button>}
      >
        {prompt && (
          <>
            <p className="notice mb-2">{promptFor(prompt).modelHint}</p>
            <p className="notice mb-2">
              这份提示词兜的是面板够不到的地方：WSL 发行版、本机其它 Windows
              用户账户。复制给别的 AI，让它先盘点、等你确认再动手。
            </p>
            <CodeBlock
              text={promptFor(prompt).body}
              caption="复制给别的 AI"
              maxHeight={420}
            />
          </>
        )}
      </Modal>

      {/* ------------------------------------ Codex 桌面端安装确认（0.28.0） */}
      <ConfirmDialog
        open={askCodexDesktop}
        onCancel={() => setAskCodexDesktop(false)}
        onConfirm={() => void runCodexDesktopInstall()}
        title={
          sw.data?.codexDesktop.installed
            ? "更新 / 重装 Codex 桌面端？"
            : "安装 Codex 桌面端？"
        }
        confirmLabel={codexLocal.trim() ? "装这个 .msix" : "开始"}
        loading={busy === "codex-desktop"}
        danger={!!sw.data?.codexDesktop.installed}
      >
        <p>
          <strong>开着的 Codex 桌面端会先被关掉</strong>
          （Store
          包在跑的时候装不上），正在跑的任务会中断，请先保存。账户槽位、登录资料、历史会话都保留。
        </p>
        <ol className="mt-2 ml-4 list-decimal">
          <li>先用 winget 从 Store 源装（系统自己管注册，不用提权）</li>
          <li>
            没成就直连微软的分发接口取官方 MSIX（约 800 MB），核对清单 SHA-256
            与 OpenAI 签名后 <code>Add-AppxPackage</code>
          </li>
          <li>装完回读 Get-AppxPackage 核对，不看命令退出码</li>
        </ol>
        <p className="notice mt-2">
          新版包里带打包服务，注册要管理员 —— 走到直连那条时可能弹一次
          UAC；拒绝就不装，已下好的包留在托管目录里可以手动装。
        </p>
        <div className="mt-3">
          <Checkbox
            checked={codexForce}
            disabled={busy === "codex-desktop"}
            onChange={setCodexForce}
          >
            强制重装（同版本也重装，用来修复注册失效 / 拒绝访问 os error 5）
          </Checkbox>
        </div>
        <div className="mt-2">
          <PathField
            label="已有 .msix 就填这里（可选）"
            value={codexLocal}
            onChange={setCodexLocal}
            kind="file"
            disabled={busy === "codex-desktop"}
            hint="地区拦截拿不到时的退路：自己从可信来源下的官方包。面板仍会核对 OpenAI 签名才注册。"
          />
        </div>
      </ConfirmDialog>

      {/* ------------------------------------ 反重力安装确认（0.29.0） */}
      <ConfirmDialog
        open={askAntigravity}
        onCancel={() => setAskAntigravity(false)}
        onConfirm={() => void runAntigravityInstall()}
        title={`${agInstalled ? "更新 / 重装" : "安装"} ${PRODUCT_LABEL[agProduct]}？`}
        confirmLabel={agLocal.trim() ? "装这个安装包" : "开始"}
        loading={busy === "antigravity"}
        danger={agInstalled}
      >
        <p>
          <strong>正在跑的{PRODUCT_LABEL[agProduct]}会先被关掉</strong>
          （它是 Electron 单实例 +
          托盘后台运行，开着的时候安装器换不动文件，而且多半不报错），
          未保存的对话会丢，请先保存。登录身份在 Windows
          凭据管理器里，不受影响。
        </p>
        <ol className="mt-2 ml-4 list-decimal">
          <li>
            先用 winget 装官方包（<code>{AG_WINGET_ID[agProduct]}</code>）
          </li>
          <li>
            没成就从 Google 自己的下载域取官方安装器，核 Authenticode 签名主体含
            Google 之后静默安装
          </li>
          <li>装完回读检测核对，不看安装器的退出码</li>
          <li>
            <strong>装完自动重新上锁</strong> —— 新程序继承的是干净权限，
            门禁那条执行锁跟着旧文件没了
          </li>
        </ol>
        <p className="notice mt-2">
          面板<strong>不分发也不托管 Google 的安装包</strong>
          ：装的就是你自己去官网点下载得到的那一个文件。官网不公布安装包哈希，
          所以完整性靠「只从 Google 的域下」加「必须验出 Google 签名」两道 ——
          读不出签名一律不装。
        </p>
        <div className="mt-3">
          <Checkbox
            checked={agForce}
            disabled={busy === "antigravity"}
            onChange={setAgForce}
          >
            强制重装（同版本也重装）
          </Checkbox>
        </div>
        <div className="mt-2">
          <PathField
            label="已有官方安装包就填这里（可选）"
            value={agLocal}
            onChange={setAgLocal}
            kind="file"
            disabled={busy === "antigravity"}
            hint="拿不到网络时的退路：你自己从官网下的那个 .exe。面板仍会核对 Google 签名才跑它。"
          />
        </div>
      </ConfirmDialog>

      {/* ------------------------------------------------ 卸载确认框 */}
      <ConfirmDialog
        open={!!plan}
        onCancel={() => setPlan(null)}
        onConfirm={() => void runPurge()}
        title={`完全卸载 ${plan ? PURGE_TARGET_LABEL[plan.target] : ""}？`}
        confirmLabel="完全卸载"
        confirmWord="卸载"
        loading={busy === "purge"}
        danger
      >
        <p>
          下面 <strong>{plan?.items.length ?? 0}</strong> 处会被清掉，
          <strong>不可恢复</strong>。每一处都带着归属依据 ——
          凭什么认定它属于这个软件：
        </p>
        <div className="mt-2 max-h-72 overflow-auto">
          {plan &&
            groupByCategory(plan.items).map(([cat, items]) => (
              <div key={cat} className="mb-2">
                <h4 className="notice font-mono uppercase">
                  {PURGE_CATEGORY_LABEL[cat]}（{items.length}）
                </h4>
                {items.map((it) => (
                  <Row
                    key={`${it.category}:${it.subject}`}
                    side={
                      <Pill tone="default">
                        {PURGE_ACTION_LABEL[it.action]}
                      </Pill>
                    }
                  >
                    <span className="notice block w-full break-all font-mono">
                      {it.subject}
                    </span>
                    <span className="notice block">{it.why}</span>
                  </Row>
                ))}
              </div>
            ))}
        </div>
        <p className="notice notice--warn mt-2">
          <strong>认证与环境变量只列名字与位置，不显示内容</strong> ——
          结构上就装不下。
          清完会自动复扫一遍：复扫走的是同一套扫描器，它证明的是「这些位置现在是空的」，
          <strong>不是</strong>「这台机器再无痕迹」。WSL
          与其它用户账户面板够不到， 那一块交给卡里那份卸载提示词。
        </p>
      </ConfirmDialog>

      {/* ------------------------------------------------ 卸载结果 */}
      <Modal
        open={!!report}
        onClose={() => setReport(null)}
        title="卸载结果"
        footer={<Button onClick={() => setReport(null)}>知道了</Button>}
      >
        {report && (
          <>
            <p>
              做完 {report.r.done.length} 项
              {report.r.failed.length > 0 &&
                `，失败 ${report.r.failed.length} 项`}
              。
            </p>
            {report.r.failed.map((f) => (
              <p key={f} className="notice notice--danger mt-1 break-all">
                {f}
              </p>
            ))}
            {report.r.notes.map((n) => (
              <p key={n} className="notice mt-1">
                {n}
              </p>
            ))}
            <div className="mt-3 border-t border-line pt-3">
              {report.r.left.length === 0 ? (
                <p className="notice">
                  <strong>复扫没有再扫到东西。</strong>
                  这证明的是「面板扫得到的那些位置现在是空的」，不是「这台机器再无痕迹」。
                </p>
              ) : (
                <>
                  <p className="notice notice--warn">
                    <strong>复扫还剩 {report.r.left.length} 处。</strong>
                    正在运行的可执行文件删不掉 —— Windows
                    允许改名，但不允许删除。
                  </p>
                  {report.r.left.map((l) => (
                    <Row
                      key={`${l.category}:${l.subject}`}
                      side={
                        <Pill tone="warn">
                          {PURGE_CATEGORY_LABEL[l.category]}
                        </Pill>
                      }
                    >
                      <span className="notice block w-full break-all font-mono">
                        {l.subject}
                      </span>
                    </Row>
                  ))}
                </>
              )}
            </div>
            <p className="notice mt-2">
              建议重启一次，再点「完全卸载」复核一遍。
            </p>
          </>
        )}
      </Modal>

      {/* --------------------------------------------- Chrome 清空重装 */}
      {/* ⛔ 装没装按 `chromeInstalled` 那条三源兜底链判，而且**没读到时按「装了」说** ——
          后端 `chrome::reinstall` 自己会去看 exe 在不在、在就清空，前端拿不准时把
          最重的那份后果说出口，比说成「只装不碰数据」安全。 */}
      <ConfirmDialog
        open={askChrome}
        onCancel={() => setAskChrome(false)}
        onConfirm={() => void runChromeReinstall()}
        title={
          chromeInstalled !== false
            ? "清空并重装 Google Chrome？"
            : "安装 Google Chrome？"
        }
        confirmLabel={
          chromeInstalled !== false ? "我知道会全部清空，开始" : "开始安装"
        }
        loading={busy === "chrome"}
        danger={chromeInstalled !== false}
      >
        {chromeInstalled !== false ? (
          <>
            <p>
              <strong>这一步会毁掉数据，而且不可恢复。</strong>
              点下去之后全程自动，中途不再问。
            </p>
            <ol className="mt-2 ml-4 list-decimal">
              <li>强制关闭所有 Chrome 窗口（没保存的网页会丢）</li>
              <li>用 winget 卸载 Chrome</li>
              <li>
                删掉整个 <code>%LOCALAPPDATA%\Google\Chrome\User Data</code>
              </li>
              <li>用 winget 重新装一个干净的 Chrome</li>
            </ol>
            <p className="notice notice--danger mt-2">
              <strong>会一起没掉的：</strong>
              书签、保存的密码、自动填充、扩展及其数据、 全部网站的 Cookie
              与登录态 —— 不只是 claude.ai，是<strong>所有网站</strong>。
            </p>
            <p className="notice mt-2">
              第 3 步不是可选的：官方卸载程序<strong>默认不删这个目录</strong>，
              不删的话重装完旧的登录态原样还在，整件事白做。
            </p>
          </>
        ) : (
          <p>
            本机没有 Chrome，这一步只做安装：用 winget 装{" "}
            <code>Google.Chrome</code>， 不碰任何现有数据。
          </p>
        )}
      </ConfirmDialog>

      {/* ------------------------------------------------ 出站锁 */}
      <Modal
        open={askFirewall}
        onClose={() => setAskFirewall(false)}
        title="浏览器出站锁"
        footer={
          <>
            <Button
              variant="ghost"
              loading={busy === "fw-revoke"}
              disabled={!!busy || !rules.data?.length}
              onClick={() =>
                void act(
                  "fw-revoke",
                  () =>
                    api.firewallRevokeAll().then((n) => `已撤销 ${n} 条规则`),
                  [],
                ).then(() => rules.refresh())
              }
            >
              全部撤销
            </Button>
            <Button
              variant="danger"
              loading={busy === "fw-add"}
              // ⛔ 用 `chromePath` 那条兜底链，不是 `au?.chrome_path` ——
              // 审计没跑成而痕迹扫描跑成了（或者只有 `software` 认出了
              // Chrome）时，这颗按钮会永远灰着，而且不给任何理由。
              // 卡片顶上那段注释记的就是同一个坑，这里别再踩一遍。
              disabled={!!busy || !picked.length || !chromePath}
              onClick={() =>
                void act(
                  "fw-add",
                  () =>
                    api
                      .firewallBlock(chromePath!, picked)
                      .then((r) => `已加 ${r.length} 条出站阻止规则`),
                  [],
                ).then((ok) => {
                  void rules.refresh();
                  // ⛔ 只有成功才关窗。失败还关的话，错误信息和刚勾好的
                  // 那一排网卡一起没了 —— 而这时候系统里可能已经加进去
                  // 几条了（是逐块网卡加的）。
                  if (ok) setAskFirewall(false);
                })
              }
            >
              加规则
            </Button>
          </>
        }
      >
        <p>
          给 Chrome 加 Windows 防火墙<strong>出站</strong>
          规则：勾中的网卡一律拦掉， 没勾的（比如你的 VPN / TUN
          虚拟网卡）不受影响，流量照走那一条。
        </p>
        <p className="notice notice--warn mt-2">
          <strong>面板不替你判断哪块是物理网卡、哪块是 TUN。</strong>
          勾错的两种后果都很实在：勾到 TUN 上会当场断网，漏掉物理网卡等于没拦 ——
          而面板不会知道你勾错了。
        </p>
        <p className="notice mt-2">
          <strong>加和撤都要管理员权限</strong>
          —— Windows 不让普通权限改防火墙。面板不是以管理员身份启动的话，
          这两颗按钮会被系统拒绝（下面会如实报出拒绝的原因）。
        </p>
        <div className="mt-2">
          {(adapters.data ?? []).map((a) => (
            <Checkbox
              key={a.name}
              checked={picked.includes(a.name)}
              disabled={!!busy}
              onChange={(v) =>
                setPicked((p) =>
                  v ? [...p, a.name] : p.filter((x) => x !== a.name),
                )
              }
            >
              {a.name}
              <span className="notice ml-1">
                {a.description}
                {a.up ? "" : " · 未连接"}
              </span>
            </Checkbox>
          ))}
          {!adapters.data?.length && (
            <p className="notice">还没读到网卡清单。</p>
          )}
        </div>
        {!!rules.data?.length && (
          <div className="mt-3 border-t border-line pt-3">
            <p className="notice mb-1">面板当前加着这几条：</p>
            {rules.data.map((r) => (
              <Row
                key={r.name}
                side={r.enabled ? <Pill tone="accent">生效中</Pill> : null}
              >
                <span className="notice block w-full break-all">{r.name}</span>
              </Row>
            ))}
          </div>
        )}
        <p className="notice notice--danger mt-2">
          <strong>规则不随面板退出而消失。</strong>
          面板关掉、甚至卸载之后它还在， 浏览器会继续上不了网。
          <strong>卸载面板之前请先回到这里撤销。</strong>
          <br />
          同理，<strong>卸载 Chrome 也不会带走这些规则</strong>：规则认的是那个
          exe 路径，重新装回同一个位置就又被拦上了。
        </p>
      </Modal>

      {/* ------------------------------------------------ 系统代理 */}
      <Modal
        open={askProxy}
        onClose={() => setAskProxy(false)}
        title="修改系统代理"
        footer={
          <>
            {proxyBackup.data && (
              <Button
                variant="ghost"
                loading={busy === "proxy-back"}
                disabled={!!busy}
                onClick={() =>
                  void act(
                    "proxy-back",
                    () => api.proxyRollback(),
                    AFTER.proxy,
                  ).then((ok) => ok && setAskProxy(false))
                }
              >
                还原到原值
              </Button>
            )}
            <Button
              variant="danger"
              loading={busy === "proxy"}
              disabled={!!busy}
              onClick={() =>
                void act(
                  "proxy",
                  () =>
                    api
                      .proxyApply({
                        enabled: proxyDraft.enabled,
                        server: proxyDraft.server || null,
                        bypass: proxy.data?.bypass ?? null,
                        pac: proxy.data?.pac ?? null,
                      })
                      .then(() => "系统代理已修改"),
                  AFTER.proxy,
                ).then((ok) => ok && setAskProxy(false))
              }
            >
              修改
            </Button>
          </>
        }
      >
        <p className="notice notice--danger">
          <strong>这是整机设置。</strong>
          所有跟随系统代理的程序都会跟着变，不只是浏览器 ——
          <strong>改错会当场断网</strong>
          。面板会拦下「开着代理却没填地址」这种当场自毁的组合，
          但拦不住地址填错。
        </p>
        <div className="mt-2">
          <Checkbox
            checked={proxyDraft.enabled}
            disabled={!!busy}
            onChange={(v) => setProxyDraft((d) => ({ ...d, enabled: v }))}
          >
            启用手动代理
          </Checkbox>
        </div>
        <TextField
          className="mt-2"
          label="代理地址"
          value={proxyDraft.server}
          onChange={(v) => setProxyDraft((d) => ({ ...d, server: v }))}
          placeholder="127.0.0.1:7890"
          disabled={!!busy}
          hint="改之前面板会把原值存到磁盘上，随时能还原 —— 面板关掉也还在。"
        />
        {proxy.data?.pac && (
          <p className="notice notice--warn mt-2">
            这台机器还挂着 PAC（<code>{proxy.data.pac}</code>）。
            <strong>只关手动代理不等于没有代理</strong> —— PAC 照样接管流量。
          </p>
        )}
        <p className="notice mt-2">
          改完面板会读回注册表核对。但那核对的是<strong>设置本身</strong>，
          不是「所有程序都已经用上新设置」——
          已经在跑的程序多半要重启才会重新读。
        </p>
      </Modal>
    </>
  );
}

/** 卸载栏。四张卡长得一样，只有文案不同。 */
function UninstallCol({
  target,
  busy,
  onPlan,
  onPrompt,
  note,
}: {
  target: PurgeTarget;
  busy: string;
  onPlan: (t: PurgeTarget) => void;
  onPrompt: (t: PurgeTarget) => void;
  note: string;
}) {
  return (
    <Col label="卸载">
      <p className="notice">
        按类清掉：程序、版本库、缓存与登记、配置与会话、认证、环境变量与 PATH、
        Shell 配置行、启动项、账户槽位。<strong>没有选项，一次清完。</strong>
      </p>
      <div className="mt-2">
        <Button
          size="sm"
          variant="danger"
          icon={<Trash2 size={13} />}
          loading={busy === `plan-${target}`}
          disabled={!!busy}
          onClick={() => onPlan(target)}
        >
          完全卸载
        </Button>
      </div>
      <p className="notice mt-1">
        先<strong>只读盘点</strong>：每一处带着绝对路径与归属依据摆给你看，
        输入「卸载」两个字才动手。{note}
      </p>
      <PromptButton onPrompt={() => onPrompt(target)} disabled={!!busy} />
    </Col>
  );
}

/**
 * 塞在某一栏底部的精简版卸载入口（反重力 / Gemini CLI，0.28.0）。
 *
 * 这两张卡的三栏各有各的用途，硬加第四栏会把 `lg:grid-cols-3` 挤成四栏窄条；
 * 而它们也没有对应的官方重装流程，不给「卸载提示词」按钮 —— 只有「完全卸载」，
 * 走跟别的卡同一套「只读盘点 → 输入确认词 → 执行 → 复扫」。
 */
function UninstallInline({
  target,
  busy,
  onPlan,
  note,
}: {
  target: PurgeTarget;
  busy: string;
  onPlan: (t: PurgeTarget) => void;
  note: string;
}) {
  return (
    <div className="mt-3 border-t border-line pt-3">
      <Button
        size="sm"
        variant="danger"
        icon={<Trash2 size={13} />}
        loading={busy === `plan-${target}`}
        disabled={!!busy}
        onClick={() => onPlan(target)}
      >
        完全卸载
      </Button>
      <p className="notice mt-1">
        先<strong>只读盘点</strong>，输入「卸载」两个字才动手。{note}
      </p>
    </div>
  );
}

/**
 * 审计里的一行。
 *
 * 语气只有这三档有对应的类（`workspace.css`）。别写成 `qb-tone-${tone}` ——
 * 那会拼出 `qb-tone-accent` 这类根本不存在的类名，样式静默失效而没人发现。
 */
const AUDIT_TONE: Partial<Record<string, string>> = {
  ok: "qb-tone-ok",
  warn: "qb-tone-warn",
  danger: "qb-tone-danger",
};

function AuditRow({
  k,
  v,
  tone,
  action,
}: {
  k: string;
  v: string;
  tone: "ok" | "warn" | "danger" | "accent" | "default";
  action?: ReactNode;
}) {
  return (
    // ⚠ `flex-shrink-0` 而不是 `shrink-0`：这个仓库的 Tailwind 产物里
    // **只有前者**（源码别处用的是前者，JIT 只生成源码里出现过的类名）。
    // 写成 `shrink-0` 会静默失效，跟上面 `AUDIT_TONE` 那条注释是同一个坑。
    //
    // 能让的只有「值」那一格：键让了会折成两行（「安全 DNS」「系统代理」
    // 实测都会），按钮让了会被挤扁。值有 `truncate` + `title`，截断了
    // 鼠标悬停还能看全。
    <div className="flex items-center gap-2 border-b border-line py-1.5 last:border-b-0">
      <ShieldAlert
        size={12}
        aria-hidden="true"
        className={AUDIT_TONE[tone] ?? ""}
      />
      <span className="flex-shrink-0 text-sm">{k}</span>
      <span className="notice ml-auto min-w-0 truncate text-right" title={v}>
        {v}
      </span>
      {action && <span className="flex-shrink-0">{action}</span>}
    </div>
  );
}

/** 按分类分组，保持 `PURGE_CATEGORY_LABEL` 的声明顺序。 */
function groupByCategory(
  items: PurgeItem[],
): Array<[PurgeItem["category"], PurgeItem[]]> {
  const order = Object.keys(PURGE_CATEGORY_LABEL) as Array<
    PurgeItem["category"]
  >;
  return order
    .map(
      (c) =>
        [c, items.filter((i) => i.category === c)] as [
          PurgeItem["category"],
          PurgeItem[],
        ],
    )
    .filter(([, v]) => v.length > 0);
}
