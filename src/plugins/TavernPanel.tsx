/**
 * 酒馆详情页。
 *
 * # 这一页要回答的只有一个问题：现在能不能起，不能的话缺什么
 *
 * 改版之前它回答不了。三个路径默认留空（对的，见
 * `TavernConfig::default`），而空路径撞上的是 `路径不存在: bridge.py` ——
 * 空目录拼出来的相对文件名。页面上同时还有一句「依赖不齐，展开看缺哪一项」，
 * 指着一个**根本没有渲染出来的** `checks` 列表。于是使用者能看到的全部信息是
 * 一句看不懂的话和一个只会失败的按钮。
 *
 * 现在：状态条一句话说清在哪一档 → 没配就直接给「自动定位」→
 * 候选按证据排好让人点「采用」→ 保存 → 启动。
 *
 * # 文案克制
 *
 * 这一页不写功能介绍。原来每张卡顶上都有一段解释「面板为什么只做盘点不做编辑」
 * 「备份为什么是目录复制不是打包」—— 那些是设计说明，属于 DESIGN-NOTES，
 * 不属于一个要用来干活的界面。留下的只有**当下能改变使用者下一步动作**的句子。
 */

import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Archive,
  Bot,
  Copy,
  ExternalLink as LinkIcon,
  FolderTree,
  History,
  Play,
  Radar,
  RotateCw,
  Settings2,
  Square,
} from "lucide-react";

import {
  api,
  type GptBridgeStatus,
  type GeminiBridgeStatus,
  type TavernCandidate,
  type TavernConfig,
  type TavernEvidence,
  type TavernSurvey,
} from "../lib/api";
import { AFTER, R } from "../lib/resources";
import { invalidate, useResource } from "../lib/store";
import { bridgeBlockers } from "../lib/tavernReady";
import BridgeSettings from "../features/tavern/BridgeSettings";
import ClaudeBridgePanel, { ClaudeBridgeMonitor } from "./ClaudeBridgePanel";
import { endTask, resetTask, useTask } from "../lib/tasks";
import {
  Button,
  Card,
  Checkbox,
  Collapsible,
  ConfirmDialog,
  EmptyState,
  Field,
  PathField,
  Pill,
  ProgressBar,
  Row,
  useToast,
  fmtSize,
} from "../ui";

/** 证据档位 → 一个词。顺序即可信度，界面上不许把它们合成「找到了」。 */
const EVIDENCE: Record<
  TavernEvidence,
  { label: string; tone: "ok" | "accent" | "default" }
> = {
  running: { label: "正在运行", tone: "ok" },
  pidfile: { label: "上次运行", tone: "accent" },
  configured: { label: "当前配置", tone: "accent" },
  scan: { label: "扫描命中", tone: "default" },
};

/** 缺什么就列什么。一样都不缺时什么都不画，不留一块空提示。 */
function Blockers({ items }: { items: string[] }) {
  if (items.length === 0) return null;
  return (
    <ul className="notice notice--warn mt-2 list-disc space-y-1 pl-5">
      {items.map((t) => (
        <li key={t}>{t}</li>
      ))}
    </ul>
  );
}

type Slot = "bridge_root" | "sillytavern_root" | "st_launcher";

const GROUPS: {
  slot: Slot;
  key: keyof TavernSurvey & string;
  label: string;
}[] = [
  { slot: "bridge_root", key: "bridge", label: "桥接项目目录" },
  { slot: "sillytavern_root", key: "sillytavern", label: "SillyTavern 目录" },
  { slot: "st_launcher", key: "launcher", label: "酒馆启动脚本" },
];

export default function TavernPanel() {
  const toast = useToast();
  const [bridgeOpen, setBridgeOpen] = useState(false);
  const cfgRes = useResource("tavernConfig", R.tavernConfig);
  const assets = useResource("tavernAssets", R.tavernAssets);
  const backups = useResource("tavernBackups", R.tavernBackups);
  const plugins = useResource("plugins", R.plugins);
  const task = useTask("tavern-start");

  const [busy, setBusy] = useState("");
  const [draft, setDraft] = useState<TavernConfig | null>(null);
  const [openCat, setOpenCat] = useState<string | null>(null);
  const [restoreId, setRestoreId] = useState<string | null>(null);
  const [survey, setSurvey] = useState<TavernSurvey | null>(null);

  const cfg = draft ?? cfgRes.data ?? null;
  const status = plugins.data?.[0];
  // 内置 GPT 桥接（0.25.0）：跟 Claude 那条桥并排，酒馆共用。
  // 状态单独从后端读：桥接活在面板进程里，插件的 checks 只知道端口占没占。
  const [gpt, setGpt] = useState<GptBridgeStatus | null>(null);
  const refreshGpt = useCallback(async () => {
    try {
      setGpt(await api.tavernGptStatus());
    } catch {
      // 演示模式没有这条命令；保持上一次的值。
    }
  }, []);
  useEffect(() => {
    void refreshGpt();
    const timer = window.setInterval(() => void refreshGpt(), 20_000);
    return () => window.clearInterval(timer);
  }, [refreshGpt]);
  // 内置 Gemini 桥接（0.26.0）：驱动官方 Gemini CLI，不是反重力本体。
  const [gemini, setGemini] = useState<GeminiBridgeStatus | null>(null);
  const refreshGemini = useCallback(async () => {
    try {
      setGemini(await api.tavernGeminiStatus());
    } catch {
      // 演示模式没有这条命令；保持上一次的值。
    }
  }, []);
  useEffect(() => {
    void refreshGemini();
    const timer = window.setInterval(() => void refreshGemini(), 20_000);
    return () => window.clearInterval(timer);
  }, [refreshGemini]);
  const running = status?.state === "running";
  const broken = status?.state === "broken";
  /**
   * 三个路径还空着。判据取**已落盘的那份**（`cfgRes.data`），不看 `draft` ——
   * 用草稿的话，人刚在框里敲下第一个字符，「还没配」的提示就没了。
   */
  const saved = cfgRes.data;
  const unconfigured =
    !!saved &&
    [saved.bridge_root, saved.sillytavern_root, saved.st_launcher].some(
      (p) => !p,
    );
  /** GPT 那条桥用不着 bridge.py：只看酒馆自己的两个路径。 */
  const gptUnconfigured =
    !!saved && (!saved.sillytavern_root || !saved.st_launcher);
  /** 两条内置桥各自还缺什么（顺序 = 该先做哪一件）。 */
  const gptBlockers = bridgeBlockers({
    tavernPathsUnset: gptUnconfigured,
    cli: gpt?.codex_exe ?? null,
    cliLabel: "官方 Codex CLI",
    cliWhere: "到「软件」页装 Codex，或 npm i -g @openai/codex",
    slot: gpt?.slot ?? null,
    slotLoggedIn: !!gpt?.slot_logged_in,
    slotWhere: "「官方账户 · GPT」",
  });
  const geminiBlockers = bridgeBlockers({
    tavernPathsUnset: gptUnconfigured,
    cli: gemini?.gemini_cli ?? null,
    cliLabel: "官方 Gemini CLI",
    cliWhere: "到「软件」页点「用 npm 安装」",
    slot: gemini?.slot ?? null,
    slotLoggedIn: !!gemini?.slot_logged_in,
    slotWhere: "「官方账户 · 反重力」的 Gemini CLI 槽位",
  });
  /** 填了但依赖检查没过 = 路径指错了或东西被挪走了，跟「还没配」是两回事。 */
  const misconfigured = !!saved && !unconfigured && status?.state === "missing";
  const needsSetup = unconfigured || misconfigured;
  const dirty = !!draft;

  // ------------------------------------------------------------ 动作

  async function start() {
    setBusy("start");
    resetTask("tavern-start");
    try {
      const url = await api.pluginStart();
      endTask("tavern-start");
      invalidate(...AFTER.tavern);
      // 无论新起还是复用现有服务都要打开页面 —— 少了这步，
      // 成功的启动和崩溃看起来一模一样。反过来也一样：走到这里酒馆已经在跑、
      // 租约也拿着，页面没打开不许报成启动失败。
      try {
        if (url.startsWith("http")) await openUrl(url);
        toast.ok("酒馆已就绪，已打开页面");
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
      setBusy("");
    }
  }

  /** 起 GPT 那条：内置桥接 + 酒馆（酒馆已经在跑就复用）。 */
  async function startGpt() {
    setBusy("start-gpt");
    resetTask("tavern-start");
    try {
      const url = await api.pluginStart("gpt");
      endTask("tavern-start");
      invalidate(...AFTER.tavern);
      void refreshGpt();
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
      setBusy("");
    }
  }

  /** 起 Gemini 那条：内置桥接（官方 Gemini CLI）+ 酒馆。 */
  async function startGemini() {
    setBusy("start-gemini");
    resetTask("tavern-start");
    try {
      const url = await api.pluginStart("gemini");
      endTask("tavern-start");
      invalidate(...AFTER.tavern);
      void refreshGemini();
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
      setBusy("");
    }
  }

  async function copyGeminiToken() {
    setBusy("token-gemini");
    try {
      const token = await api.tavernGeminiToken();
      await navigator.clipboard.writeText(token);
      toast.ok("密钥已复制，到酒馆的 API 连接里粘贴");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  }

  /** 密钥只在点的时候读，读完直接进剪贴板，不在页面上显示。 */
  async function copyGptToken() {
    setBusy("token");
    try {
      const token = await api.tavernGptToken();
      await navigator.clipboard.writeText(token);
      toast.ok("密钥已复制，到酒馆的 API 连接里粘贴");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  }

  async function locate(deep: boolean) {
    setBusy(deep ? "deep" : "locate");
    try {
      const s = await api.tavernLocate(deep);
      setSurvey(s);
      const hits = s.bridge.length + s.sillytavern.length + s.launcher.length;
      if (hits === 0) {
        toast.error(
          s.truncated
            ? `扫了 ${s.scanned_dirs} 个目录还没扫完，也没找到。可以试深扫，或自己填路径。`
            : "本机没找到酒馆和桥接。装在别处的话请自己填路径。",
        );
      } else {
        toast.ok(`找到 ${hits} 条候选`);
      }
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  }

  /** 采用一条候选：只写进草稿，**保存仍然要人自己点**。 */
  function adopt(slot: Slot, path: string) {
    if (!cfg) return;
    setDraft({ ...cfg, [slot]: path });
  }

  function adoptBest() {
    if (!cfg || !survey) return;
    const next = { ...cfg };
    for (const g of GROUPS) {
      const top = (survey[g.key] as TavernCandidate[])[0];
      if (top) next[g.slot] = top.path;
    }
    setDraft(next);
  }

  async function act(name: string, fn: () => Promise<unknown>, ok: string) {
    setBusy(name);
    try {
      await fn();
      toast.ok(ok);
      invalidate(
        ...AFTER.tavern,
        "tavernAssets",
        "tavernBackups",
        "tavernConfig",
      );
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
      setRestoreId(null);
    }
  }

  // ------------------------------------------------------------ 状态条

  const tone = running ? "ok" : needsSetup || broken ? "warn" : "accent";
  const headline = running
    ? "运行中"
    : unconfigured
      ? "还没配路径"
      : misconfigured
        ? "路径或依赖有问题"
        : broken
          ? "只起来了一半"
          : "就绪";

  return (
    <>
      <div className={`tv-status tv-status--${tone}`}>
        <span
          className={`tv-dot tv-dot--${running ? "ok" : needsSetup || broken ? "warn" : "ok"}`}
        />
        <span className="tv-status-text">
          <strong>{headline}</strong>
          <span>{status?.detail ?? "读取中…"}</span>
        </span>
        <span className="tv-status-actions">
          {needsSetup ? (
            <Button
              variant="primary"
              icon={<Radar size={13} />}
              loading={busy === "locate"}
              disabled={!!busy}
              onClick={() => locate(false)}
            >
              自动定位
            </Button>
          ) : (
            <Button
              variant="primary"
              icon={<Play size={13} />}
              loading={busy === "start"}
              disabled={!!busy}
              onClick={start}
            >
              {running ? "打开页面" : "启动酒馆"}
            </Button>
          )}
          {/* 0.32.0：端口与模型全在这一个弹窗里改，
              跟三个账户页酒馆磁贴右上角那颗是**同一个组件**。 */}
          <Button
            icon={<Settings2 size={13} />}
            disabled={!!busy}
            onClick={() => setBridgeOpen(true)}
          >
            桥接设置
          </Button>
          <Button
            variant="danger"
            icon={<Square size={13} />}
            loading={busy === "stop"}
            disabled={!!busy || !running}
            onClick={() =>
              act("stop", api.pluginStop, "酒馆与桥接已停止，租约已收回")
            }
          >
            停止
          </Button>
        </span>
      </div>

      {(task.running || task.error) && (
        <div className="mt-2">
          <div className="mb-1.5 flex items-center gap-2">
            <span className="text-sm">{task.phase || "启动中…"}</span>
            {task.total > 0 && (
              <span className="notice ml-auto">
                {task.step} / {task.total}
              </span>
            )}
          </div>
          <ProgressBar
            value={task.total > 0 ? (task.step / task.total) * 100 : undefined}
            tone={task.error ? "danger" : "accent"}
            label="酒馆启动进度"
          />
          {task.error && (
            <p className="notice notice--danger mt-1.5">{task.error}</p>
          )}
        </div>
      )}

      {/* ------------------------------------------------------ 定位 */}
      {(needsSetup || survey) && (
        <Card
          title="定位"
          icon={<Radar size={14} />}
          as="h3"
          className="mt-3"
          actions={
            <div className="flex gap-2">
              <Button
                size="sm"
                loading={busy === "locate"}
                disabled={!!busy}
                onClick={() => locate(false)}
              >
                自动定位
              </Button>
              <Button
                size="sm"
                variant="ghost"
                loading={busy === "deep"}
                disabled={!!busy}
                onClick={() => locate(true)}
              >
                深扫
              </Button>
            </div>
          }
        >
          {survey ? (
            <>
              {GROUPS.map((g) => {
                const list = survey[g.key] as TavernCandidate[];
                return (
                  <div className="tv-group" key={g.slot}>
                    <div className="tv-group-head">{g.label}</div>
                    {list.length ? (
                      list.map((c) => (
                        <div
                          key={c.path}
                          className={`tv-cand${cfg?.[g.slot] === c.path ? " tv-cand--picked" : ""}`}
                        >
                          <Pill tone={EVIDENCE[c.evidence].tone}>
                            {EVIDENCE[c.evidence].label}
                          </Pill>
                          <span className="tv-cand-main">
                            <span className="tv-cand-path">{c.path}</span>
                            <span className="tv-cand-note">{c.note}</span>
                          </span>
                          <Button
                            size="sm"
                            disabled={cfg?.[g.slot] === c.path}
                            onClick={() => adopt(g.slot, c.path)}
                          >
                            {cfg?.[g.slot] === c.path ? "已选" : "采用"}
                          </Button>
                        </div>
                      ))
                    ) : (
                      <p className="notice">没找到。</p>
                    )}
                  </div>
                );
              })}
              <div className="mt-3 flex flex-wrap items-center gap-2">
                <Button
                  variant="primary"
                  disabled={!!busy || !survey.bridge.length}
                  onClick={adoptBest}
                >
                  采用每项第一条
                </Button>
                {dirty && (
                  <Button
                    variant="primary"
                    loading={busy === "cfg"}
                    disabled={!!busy}
                    onClick={() =>
                      act(
                        "cfg",
                        () => api.tavernConfigSave(cfg!),
                        "已保存",
                      ).then(() => setDraft(null))
                    }
                  >
                    保存
                  </Button>
                )}
                <span className="notice ml-auto">
                  扫了 {survey.scanned_dirs} 个目录
                  {survey.truncated ? " · 没扫完，结果可能不全" : ""}
                </span>
              </div>
            </>
          ) : (
            <p className="notice">
              面板不分发 SillyTavern 和 bridge.py，用的是你机器上那一份。
              点「自动定位」让它找，或在下面自己填。
            </p>
          )}
        </Card>
      )}

      {/* ------------------------------------------------------ 路径 */}
      <Collapsible
        className="mt-3"
        key={needsSetup ? "setup" : "configured"}
        defaultOpen={needsSetup}
        summary={
          <span className="flex items-center gap-1.5">
            <Settings2 size={13} />
            路径与端口
            {dirty && <Pill tone="warn">未保存</Pill>}
          </span>
        }
      >
        {cfg ? (
          <>
            <div className="flex flex-col gap-3">
              <PathField
                label="桥接项目目录"
                value={cfg.bridge_root}
                onChange={(v) => setDraft({ ...cfg, bridge_root: v })}
              />
              <PathField
                label="SillyTavern 目录"
                value={cfg.sillytavern_root}
                onChange={(v) => setDraft({ ...cfg, sillytavern_root: v })}
              />
              <PathField
                label="酒馆启动脚本"
                kind="file"
                value={cfg.st_launcher}
                onChange={(v) => setDraft({ ...cfg, st_launcher: v })}
              />
              {/* ⛔ 端口全在「酒馆桥接设置」那个弹窗里改（0.32.0）——
                  三条桥并排着看才看得出谁跟谁撞号。这一页只管路径。 */}
              <p className="notice">端口与模型在顶上那颗「桥接设置」里改。</p>
            </div>
            <div className="mt-3 flex gap-2">
              <Button
                variant="primary"
                loading={busy === "cfg"}
                disabled={!dirty || !!busy}
                onClick={() =>
                  act("cfg", () => api.tavernConfigSave(cfg), "已保存").then(
                    () => setDraft(null),
                  )
                }
              >
                保存
              </Button>
              <Button disabled={!dirty} onClick={() => setDraft(null)}>
                放弃改动
              </Button>
              {!needsSetup && (
                <Button
                  variant="ghost"
                  icon={<Radar size={12} />}
                  loading={busy === "locate"}
                  disabled={!!busy}
                  onClick={() => locate(false)}
                >
                  重新定位
                </Button>
              )}
            </div>
          </>
        ) : (
          <p className="notice">读取配置中…</p>
        )}
      </Collapsible>

      {/* ------------------------------------- Claude 桥的设置与监控（0.32.0） */}
      {/* 这两张原来是 `bridge.py` 自己那两个网页（`/settings` 与 `/monitor`），
          要从酒馆的扩展抽屉里用 iframe 嵌进去看。使用者定的：植入面板。
          桥没在跑 / 这份 bridge.py 没有那个接口时，卡里照实说后端那句话。 */}
      <div className="mt-3 grid gap-3">
        <ClaudeBridgePanel />
        <ClaudeBridgeMonitor />
      </div>

      {/* ------------------------------------------------------ GPT 桥接 */}
      {/* 面板自带的 GPT 后端：每个请求驱动一次官方 `codex exec`，用当前激活的
          GPT 账户槽位，auth.json 一个字不碰、只绑 127.0.0.1。酒馆那边自己配一个
          Custom 连接指过来 —— 面板不改酒馆的 settings.json / secrets.json，
          跟「候选一律由使用者点采用」同一口径。 */}
      <Card
        title="GPT 桥接（内置 · 驱动官方 Codex CLI）"
        icon={<Bot size={14} />}
        as="h3"
        className="mt-3"
        actions={
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="primary"
              icon={<Play size={12} />}
              loading={busy === "start-gpt"}
              // 已经在跑时按钮是「打开页面」，那一步跟依赖无关，不该拦。
              disabled={!!busy || (!gpt?.running && gptBlockers.length > 0)}
              onClick={startGpt}
            >
              {gpt?.running ? "打开页面" : "起 GPT 酒馆"}
            </Button>
            <Button
              size="sm"
              icon={<Copy size={12} />}
              loading={busy === "token"}
              disabled={!!busy}
              onClick={copyGptToken}
            >
              复制密钥
            </Button>
          </div>
        }
      >
        <p className="notice">
          {gpt ? gpt.detail : "读取中…"}
          {gpt?.running
            ? ""
            : " · 酒馆路径与 Claude 那条共用，桥接本身不用装。"}
        </p>
        {!gpt?.running && <Blockers items={gptBlockers} />}
        {/* 长路径要能折行：`.row-side` 是 flex-shrink: 0，装 Windows 路径会横着长出去，
            所以这里不用 Row，用会折行的段落。 */}
        <div className="mt-2">
          <Row>
            <span>用的 GPT 槽位</span>
            <strong>{gpt?.slot ?? "—"}</strong>
            {gpt && gpt.slot && !gpt.slot_logged_in && (
              <Pill tone="warn">未登录</Pill>
            )}
          </Row>
          <Row>
            <span>Codex CLI</span>
            <span className="break-all font-mono text-xs">
              {gpt?.codex_exe ?? "没找到"}
            </span>
          </Row>
          <Row>
            <span>酒馆里填的地址</span>
            <span className="break-all font-mono text-xs">
              {gpt?.url ?? "—"}
            </span>
          </Row>
          <Row>
            <span>密钥文件</span>
            <span className="break-all font-mono text-xs">
              {gpt?.token_path ?? "—"}
            </span>
          </Row>
        </div>
        {cfg && (
          <div className="mt-3 grid gap-3 sm:grid-cols-2">
            {/* ⛔ 端口与模型**不在这里改**（0.32.0）。
                它们在「酒馆桥接设置」那个弹窗里（三条桥并排着看）。
                两处各放一份同一个字段的输入框，迟早会出现一个没保存、
                另一个显示旧值的局面 —— 坑 7.58 那种「同一块屏幕上两句矛盾的话」。
                留在这里的只有弹窗里没有的两项。 */}
            <Field
              label="推理强度"
              hint="交给 codex exec 的 model_reasoning_effort"
            >
              {(p) => (
                <select
                  {...p}
                  className="input"
                  value={cfg.gpt_effort}
                  onChange={(e) =>
                    setDraft({ ...cfg, gpt_effort: e.target.value })
                  }
                >
                  <option value="">默认</option>
                  <option value="low">low</option>
                  <option value="medium">medium</option>
                  <option value="high">high</option>
                </select>
              )}
            </Field>
            <div className="flex items-end pb-1">
              <Checkbox
                checked={cfg.gpt_persist_sessions}
                onChange={(v) => setDraft({ ...cfg, gpt_persist_sessions: v })}
              >
                记入槽位用量（对话会落进槽位的会话目录）
              </Checkbox>
            </div>
          </div>
        )}
        {dirty && (
          <div className="mt-3 flex gap-2">
            <Button
              variant="primary"
              loading={busy === "cfg"}
              disabled={!!busy}
              onClick={() =>
                act("cfg", () => api.tavernConfigSave(cfg!), "已保存").then(
                  () => setDraft(null),
                )
              }
            >
              保存
            </Button>
            <Button disabled={!!busy} onClick={() => setDraft(null)}>
              放弃改动
            </Button>
          </div>
        )}
        <Collapsible className="mt-3" summary="酒馆那边怎么配">
          <p className="notice">
            SillyTavern → API 连接 → Chat Completion → 来源选 Custom
            (OpenAI-compatible)：地址填上面那条（带
            /v1），密钥点「复制密钥」粘贴，模型选列表里唯一那个。
            连接档单独存一份，跟 Claude 那条（5001）并排，切换在酒馆里切。
          </p>
          <p className="notice mt-2">
            它只驱动未修改的官方 Codex CLI 的无交互模式（codex
            exec），用你当前激活的 GPT
            槽位里官方客户端自己完成的登录；不读、不复制、不转发 auth.json，
            只监听 127.0.0.1，没有 API Key 回退。没有增量流式（整段一起出现），
            不传图、不给工具。是否符合 OpenAI 的条款由你自行判断，见免责声明第 8
            节。
          </p>
        </Collapsible>
      </Card>

      {/* ------------------------------------------------------ Gemini 桥接 */}
      {/* 面板自带的第三条桥（0.26.0）：每个请求起一次官方 Gemini CLI 的无交互模式
          （stdin 喂提示词、--output-format json），用当前激活的 Gemini CLI 槽位
          （GEMINI_CLI_HOME），oauth_creds.json 一个字不碰、只绑 127.0.0.1。
          ⚠ 接的是 Gemini CLI，不是反重力本体 —— 反重力没有公开的无交互 CLI。 */}
      <Card
        title="Gemini 桥接（内置 · 驱动官方 Gemini CLI）"
        icon={<Bot size={14} />}
        as="h3"
        className="mt-3"
        actions={
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="primary"
              icon={<Play size={12} />}
              loading={busy === "start-gemini"}
              disabled={
                !!busy || (!gemini?.running && geminiBlockers.length > 0)
              }
              onClick={startGemini}
            >
              {gemini?.running ? "打开页面" : "起 Gemini 酒馆"}
            </Button>
            <Button
              size="sm"
              icon={<Copy size={12} />}
              loading={busy === "token-gemini"}
              disabled={!!busy}
              onClick={copyGeminiToken}
            >
              复制密钥
            </Button>
          </div>
        }
      >
        <p className="notice">
          {gemini ? gemini.detail : "读取中…"}
          {gemini?.running
            ? ""
            : " · 槽位在「官方账户 · 反重力」新建并登录；酒馆路径与另外两条共用。"}
        </p>
        {!gemini?.running && <Blockers items={geminiBlockers} />}
        <div className="mt-2">
          <Row>
            <span>用的 Gemini 槽位</span>
            <strong>{gemini?.slot ?? "—"}</strong>
            {gemini && gemini.slot && !gemini.slot_logged_in && (
              <Pill tone="warn">未登录</Pill>
            )}
          </Row>
          <Row>
            <span>Gemini CLI</span>
            <span className="break-all font-mono text-xs">
              {gemini?.gemini_cli ?? "没找到（软件页用 npm 装）"}
            </span>
          </Row>
          <Row>
            <span>酒馆里填的地址</span>
            <span className="break-all font-mono text-xs">
              {gemini?.url ?? "—"}
            </span>
          </Row>
          <Row>
            <span>密钥文件</span>
            <span className="break-all font-mono text-xs">
              {gemini?.token_path ?? "—"}
            </span>
          </Row>
        </div>
        {cfg && (
          <div className="mt-3 grid gap-3 sm:grid-cols-2">
            {/* 同上：端口与模型在「酒馆桥接设置」里改。 */}
          </div>
        )}
        {dirty && (
          <div className="mt-3 flex gap-2">
            <Button
              variant="primary"
              loading={busy === "cfg"}
              disabled={!!busy}
              onClick={() =>
                act("cfg", () => api.tavernConfigSave(cfg!), "已保存").then(
                  () => setDraft(null),
                )
              }
            >
              保存
            </Button>
            <Button disabled={!!busy} onClick={() => setDraft(null)}>
              放弃改动
            </Button>
          </div>
        )}
        <Collapsible
          className="mt-3"
          summary="酒馆那边怎么配 · 它是什么、不是什么"
        >
          <p className="notice">
            SillyTavern → API 连接 → Chat Completion → 来源选 Custom
            (OpenAI-compatible)：地址填上面那条（带
            /v1），密钥点「复制密钥」粘贴，
            模型选列表里唯一那个。连接档单独存一份，跟
            Claude（5001）、GPT（5002）并排。
          </p>
          <p className="notice mt-2">
            它驱动的是<strong>官方 Gemini CLI</strong>，不是反重力本体：反重力
            Hub 没有公开的无交互模式，令牌在 Windows 凭据管理器里。用同一个
            Google 账户在 Gemini CLI
            里再登一次；额度是否与反重力订阅共享，面板没有核实过。
            只驱动未修改的 CLI 的公开无交互模式，不读、不复制、不转发
            oauth_creds.json， 只监听 127.0.0.1，没有 API Key
            回退，没有增量流式，不传图、不给工具。 是否符合 Google
            的条款由你自行判断，见免责声明第 8 节。
          </p>
        </Collapsible>
      </Card>

      {/* ------------------------------------------------------ 依赖 */}
      {/* Rust 侧 `status()` 逐项算好了 `checks`，改版之前**没有任何一处
          把它渲染出来** —— detail 里那句「展开看缺哪一项」指着一个不存在的
          地方。算了不显示等于没算。 */}
      {!!status?.checks.length && (
        <Collapsible
          className="mt-3"
          key={status.checks.some((c) => !c.ok) ? "bad" : "ok"}
          defaultOpen={status.checks.some((c) => !c.ok)}
          summary={
            <span className="flex items-center gap-1.5">
              依赖
              <Pill tone={status.checks.some((c) => !c.ok) ? "warn" : "ok"}>
                {status.checks.filter((c) => !c.ok).length || "全部就绪"}
                {status.checks.some((c) => !c.ok) ? " 项缺失" : ""}
              </Pill>
            </span>
          }
        >
          {status.checks.map((c) => (
            <Row
              key={c.label}
              side={
                <Pill tone={c.ok ? "ok" : "warn"}>{c.ok ? "有" : "缺"}</Pill>
              }
            >
              <span className="min-w-0">
                <strong>{c.label}</strong>
                {/* 路径完整显示，不截断 —— 这一行的用处就是让人看出
                    自己填的到底是哪个目录。 */}
                <p className="notice break-all">{c.detail}</p>
              </span>
            </Row>
          ))}
        </Collapsible>
      )}

      {/* ------------------------------------------------------ 资产 */}
      <Card
        title="资产"
        icon={<FolderTree size={14} />}
        as="h3"
        className="mt-3"
        actions={
          <Button
            size="sm"
            icon={<RotateCw size={12} />}
            onClick={() => void assets.refresh()}
          >
            刷新
          </Button>
        }
      >
        {assets.data?.length ? (
          <>
            <div className="tv-cats">
              {assets.data.map((c) => (
                <button
                  type="button"
                  key={c.id}
                  className={`tv-cat${c.items.length ? "" : " tv-cat--empty"}`}
                  aria-expanded={openCat === c.id}
                  onClick={() => setOpenCat(openCat === c.id ? null : c.id)}
                >
                  <span>{c.label}</span>
                  <span className="tv-cat-n">
                    {c.exists ? c.items.length : "—"}
                  </span>
                </button>
              ))}
            </div>
            {assets.data
              .filter((c) => c.id === openCat)
              .map((c) =>
                c.items.length ? (
                  <div className="tv-files" key={c.id}>
                    {c.items.map((it) => (
                      <Row
                        key={it.path}
                        side={
                          <span className="notice">{fmtSize(it.size)}</span>
                        }
                      >
                        <span className="break-all">
                          {it.is_dir ? "📁 " : ""}
                          {it.name}
                        </span>
                        <span className="notice">{it.modified ?? ""}</span>
                      </Row>
                    ))}
                  </div>
                ) : (
                  <p className="notice mt-2" key={c.id}>
                    {c.exists ? "这一类还是空的。" : `目录不存在：${c.dir}`}
                  </p>
                ),
              )}
          </>
        ) : (
          <EmptyState title="还没盘点到资产">
            SillyTavern 目录填对了再点刷新。
          </EmptyState>
        )}
      </Card>

      {/* ------------------------------------------------------ 备份 */}
      <Card
        title="备份"
        icon={<Archive size={14} />}
        as="h3"
        className="mt-3"
        actions={
          <Button
            size="sm"
            variant="primary"
            loading={busy === "backup"}
            disabled={!!busy}
            onClick={() => act("backup", api.tavernBackup, "已备份全部资产")}
          >
            立即备份
          </Button>
        }
      >
        {backups.data?.length ? (
          backups.data.map((b) => (
            <Row
              key={b.id}
              side={
                <Button
                  size="sm"
                  icon={<History size={12} />}
                  disabled={!!busy}
                  onClick={() => setRestoreId(b.id)}
                >
                  恢复
                </Button>
              }
            >
              <span className="font-mono">{b.id}</span>
              <span className="notice">
                {b.created} · {fmtSize(b.size)}
              </span>
            </Row>
          ))
        ) : (
          <EmptyState icon={<Archive size={22} />} title="还没有备份">
            备份是目录复制，不打包。
          </EmptyState>
        )}
      </Card>

      <div className="mt-3 flex items-center gap-3">
        <Button
          size="sm"
          variant="ghost"
          icon={<LinkIcon size={12} />}
          onClick={() => openUrl(`http://127.0.0.1:${cfg?.st_port ?? 8000}`)}
        >
          直接打开酒馆页面
        </Button>
        <span className="tv-ports">
          桥接 {cfg?.bridge_port ?? 5001} · 酒馆 {cfg?.st_port ?? 8000}
        </span>
      </div>

      <ConfirmDialog
        open={restoreId !== null}
        onCancel={() => setRestoreId(null)}
        onConfirm={() =>
          restoreId &&
          act(
            "restore",
            () => api.tavernRestore(restoreId),
            `已恢复到 ${restoreId}`,
          )
        }
        title="恢复这份备份？"
        confirmLabel="确认恢复"
        confirmWord="恢复"
        loading={busy === "restore"}
        danger
      >
        <p>
          恢复 <code>{restoreId}</code> 会
          <strong>覆盖现有的世界书、角色卡、预设与扩展</strong>。
        </p>
        <p className="notice mt-2">
          恢复前会自动把现状再存一份。正在跑的酒馆不会自动重载，建议先停。
        </p>
      </ConfirmDialog>
      {/* 跟三个账户页磁贴右上角那颗是同一个组件。
          人已经在这一页了，所以不用再给「路径…」那颗按钮。 */}
      <BridgeSettings
        open={bridgeOpen}
        onClose={() => setBridgeOpen(false)}
        showPathsLink={false}
      />
    </>
  );
}
