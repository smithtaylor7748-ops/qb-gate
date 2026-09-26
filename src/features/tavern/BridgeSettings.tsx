/**
 * 酒馆桥接设置：三条桥的状态 / 端口 / 模型 / 地址，全在一个弹窗里（0.31.0 起，0.32.0 抽成组件）。
 *
 * # 为什么要抽出来
 *
 * 0.31.0 它是 `AntigravityBand.tsx` 里一个**内联的 `<Modal>` 字面量**，靠那个文件里的
 * 局部 state `bridgeOpen` 驱动。后果是：
 *
 * - **只有反重力页开得了它。** Claude 页和 GPT 页的「酒馆」磁贴上没有入口，
 *   而那两条桥的端口和模型恰恰也在这里改；
 * - 同一份配置在酒馆插件详情页（`src/plugins/TavernPanel.tsx`）还有**第二套**界面，
 *   两处各写各的 —— 这正是坑 7.58 那种「同一块屏幕上两句互相矛盾的话」的形状。
 *
 * 现在三个账户页的酒馆磁贴 + 插件详情页共用这一个组件。
 *
 * # ⛔ 两条别改回去
 *
 * 1. **「还缺什么」走 `lib/tavernReady.ts` 的纯函数**，不在这里另写一套判定 ——
 *    各写一份迟早一处说能起、另一处说缺东西。
 * 2. **Claude 那条桥拿不到真状态**（它活在面板外，是使用者自己那个 `bridge.py` 进程）。
 *    写「状态见插件页」，**不许谎报成「没在跑」** —— §7.17：查不到不等于没有。
 */
import { useCallback, useEffect, useState } from "react";
import { Copy, RefreshCw, Settings2, Wine } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { api } from "../../lib/api";
import { R } from "../../lib/resources";
import { invalidate, useResource } from "../../lib/store";
import { bridgeBlockers } from "../../lib/tavernReady";
import type { GeminiBridgeStatus } from "../../lib/generated/GeminiBridgeStatus";
import type { GptBridgeStatus } from "../../lib/generated/GptBridgeStatus";
import type { TavernGptQuota } from "../../lib/generated/TavernGptQuota";
import type { TavernGeminiQuota } from "../../lib/generated/TavernGeminiQuota";
import type { TavernRoleplayTest } from "../../lib/generated/TavernRoleplayTest";
import type { TavernConfig } from "../../lib/generated/TavernConfig";
import {
  Button,
  Gauge as QuotaBar,
  Modal,
  Pill,
  PortField,
  TextField,
  useToast,
} from "../../ui";

export default function BridgeSettings({
  open,
  onClose,
  provider = "all",
  /** 从插件详情页打开时不必再给「路径与备份…」那颗按钮 —— 人已经在那儿了。 */
  showPathsLink = true,
}: {
  open: boolean;
  onClose: () => void;
  provider?: "all" | "claude" | "gpt" | "gemini";
  showPathsLink?: boolean;
}) {
  const toast = useToast();
  const navigate = useNavigate();
  const tavernCfg = useResource("tavernConfig", R.tavernConfig);
  const [cfgDraft, setCfgDraft] = useState<TavernConfig | null>(null);
  const [cfgBusy, setCfgBusy] = useState(false);
  const [gptBridge, setGptBridge] = useState<GptBridgeStatus | null>(null);
  const [geminiBridge, setGeminiBridge] = useState<GeminiBridgeStatus | null>(
    null,
  );
  const [gptQuota, setGptQuota] = useState<TavernGptQuota | null>(null);
  const [geminiQuota, setGeminiQuota] = useState<TavernGeminiQuota | null>(
    null,
  );
  const [gptBusy, setGptBusy] = useState(false);
  const [geminiBusy, setGeminiBusy] = useState(false);
  const [gptError, setGptError] = useState<string | null>(null);
  const [geminiError, setGeminiError] = useState<string | null>(null);
  const [fixture, setFixture] = useState(
    "你叫林澈，是一名温和但机敏的图书管理员。请始终保持角色身份，用中文回应。",
  );
  const [testing, setTesting] = useState<"gpt" | "gemini" | null>(null);
  const [testResult, setTestResult] = useState<TavernRoleplayTest | null>(null);
  const [clock, setClock] = useState(() => Date.now());
  const refreshBridges = useCallback(async () => {
    try {
      const [g, m] = await Promise.all([
        api.tavernGptStatus(),
        api.tavernGeminiStatus(),
      ]);
      setGptBridge(g);
      setGeminiBridge(m);
    } catch {
      // 演示模式没有这两条命令；保持上一次的值。
    }
  }, []);
  /**
   * 两块额度面板。⛔ 只手动刷新（2026-09-23 使用者定的）：`refresh: false` 只读「最近一次」
   * 问到的、后端绝不联网 —— 打开弹窗走这条；面板上的「刷新」才传 `true`，而且只问它自己那一侧。
   * 原来一打开弹窗就同时问 GPT 与 Gemini，连 Claude 页打开设置也会问。
   */
  const loadGpt = useCallback(async (refresh: boolean) => {
    setGptBusy(true);
    setGptError(null);
    try {
      const q = await api.tavernGptQuota(refresh);
      if (q || refresh) setGptQuota(q);
    } catch (e) {
      setGptError(e instanceof Error ? e.message : String(e));
    } finally {
      setGptBusy(false);
    }
  }, []);
  const loadGemini = useCallback(async (refresh: boolean) => {
    setGeminiBusy(true);
    setGeminiError(null);
    try {
      const q = await api.tavernGeminiQuota(refresh);
      if (q || refresh) setGeminiQuota(q);
    } catch (e) {
      setGeminiError(e instanceof Error ? e.message : String(e));
    } finally {
      setGeminiBusy(false);
    }
  }, []);
  // 弹窗只画 `provider` 那一行（插件页是全部）；看不见的那一侧不问，连内存都不读。
  const showsGpt = provider === "all" || provider === "gpt";
  const showsGemini = provider === "all" || provider === "gemini";
  useEffect(() => {
    if (!open) return;
    void refreshBridges();
    if (showsGpt) void loadGpt(false);
    if (showsGemini) void loadGemini(false);
  }, [open, refreshBridges, loadGpt, loadGemini, showsGpt, showsGemini]);
  useEffect(() => {
    if (!open) return;
    setClock(Date.now());
    const timer = window.setInterval(() => setClock(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, [open]);

  const cfg = cfgDraft ?? tavernCfg.data ?? null;
  const cfgDirty = !!cfgDraft;
  /** 酒馆自己那两个路径还空着 —— 三条桥都过不了这一关。 */
  const tavernPathsUnset =
    !!tavernCfg.data &&
    (!tavernCfg.data.sillytavern_root || !tavernCfg.data.st_launcher);

  const bridgeRows = [
    {
      key: "claude" as const,
      name: "Claude 桥接",
      port: cfg?.bridge_port ?? 5001,
      portKey: "bridge_port" as const,
      modelKey: null,
      modelLabel: "",
      modelHint: "",
      model: null as string | null,
      promptKey: null,
      running: undefined as boolean | undefined,
      slot: null as string | null,
      url: `http://127.0.0.1:${cfg?.bridge_port ?? 5001}/v1`,
      eyebrow: "Claude · bridge.py",
      summary:
        "保留现有 Claude bridge.py；这里只统一端口、地址与入口，提示词和监控仍在插件页。",
      drives:
        "你自己那份 bridge.py（驱动官方 claude）。面板不分发它，提示词与监控在酒馆插件页。",
      blockers: [] as string[],
    },
    {
      key: "gpt" as const,
      name: "GPT 桥接",
      port: cfg?.gpt_bridge_port ?? 5002,
      portKey: "gpt_bridge_port" as const,
      modelKey: "gpt_model" as const,
      modelLabel: "模型（留空用 Codex 默认）",
      modelHint: "例如 gpt-5.6-sol",
      model: cfg?.gpt_model ?? "",
      promptKey: "gpt_system_prompt" as const,
      running: gptBridge?.running,
      slot: gptBridge?.slot ?? null,
      url:
        gptBridge?.url || `http://127.0.0.1:${cfg?.gpt_bridge_port ?? 5002}/v1`,
      eyebrow: "OpenAI · ChatGPT / Codex",
      summary:
        "每次请求由官方 Codex CLI 执行；在这里管理模型、推理偏好、独立系统提示词与 5 小时 / 7 天额度。",
      drives: "面板内置，每个请求驱动一次官方 codex exec。",
      blockers: bridgeBlockers({
        tavernPathsUnset,
        cli: gptBridge?.codex_exe ?? null,
        cliLabel: "官方 Codex CLI",
        cliWhere: "到「软件」页装 Codex",
        slot: gptBridge?.slot ?? null,
        slotLoggedIn: !!gptBridge?.slot_logged_in,
        slotWhere: "「官方账户 · GPT」",
      }),
    },
    {
      key: "gemini" as const,
      name: "Gemini 桥接",
      port: cfg?.gemini_bridge_port ?? 5003,
      portKey: "gemini_bridge_port" as const,
      modelKey: "gemini_model" as const,
      modelLabel: "模型（留空用 CLI 默认）",
      modelHint: "例如 gemini-2.5-pro",
      model: cfg?.gemini_model ?? "",
      promptKey: "gemini_system_prompt" as const,
      running: geminiBridge?.running,
      slot: geminiBridge?.slot ?? null,
      url:
        geminiBridge?.url ||
        `http://127.0.0.1:${cfg?.gemini_bridge_port ?? 5003}/v1`,
      eyebrow: "Google · Gemini CLI",
      summary:
        "通过官方 Gemini CLI 的无交互模式转发；模型桶、登录状态、项目与重置时间在本页集中管理。",
      drives: "面板内置，每个请求驱动一次官方 Gemini CLI 的无交互模式。",
      blockers: bridgeBlockers({
        tavernPathsUnset,
        cli: geminiBridge?.gemini_cli ?? null,
        cliLabel: "官方 Gemini CLI",
        cliWhere: "到「软件」页点「用 npm 安装」",
        slot: geminiBridge?.slot ?? null,
        slotLoggedIn: !!geminiBridge?.slot_logged_in,
        // 0.32.0：两半同属一条账户槽位了，指路也跟着改。
        slotWhere: "「官方账户 · 反重力」里那条账户的 CLI 那一半",
      }),
    },
  ];
  const visibleRows =
    provider === "all"
      ? bridgeRows
      : bridgeRows.filter((bridge) => bridge.key === provider);

  async function copyBridgeUrl(url: string) {
    try {
      await navigator.clipboard.writeText(url);
      toast.ok("地址已复制，到酒馆的 API 连接里粘贴");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
  }
  async function saveBridgeCfg() {
    if (!cfg) return;
    setCfgBusy(true);
    try {
      await api.tavernConfigSave(cfg);
      setCfgDraft(null);
      invalidate("tavernConfig", "plugins");
      toast.ok("已保存；下次起桥接时生效");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setCfgBusy(false);
    }
  }

  return (
    <Modal
      open={open}
      onClose={() => !cfgBusy && onClose()}
      icon={<Wine size={16} />}
      title={
        provider === "all"
          ? "酒馆桥接设置"
          : `${visibleRows[0]?.name ?? "酒馆桥接"}设置`
      }
      headerActions={
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            icon={<RefreshCw size={12} />}
            onClick={() => {
              void refreshBridges();
              // 使用者点了才联网，而且只问这个弹窗里看得见的那一侧。
              if (showsGpt) void loadGpt(true);
              if (showsGemini) void loadGemini(true);
            }}
          >
            {showsGpt || showsGemini ? "刷新状态与额度" : "刷新状态"}
          </Button>
        </div>
      }
    >
      <p className="notice">
        一份酒馆、三条桥，可以同时开着，在酒馆里各配一个 Custom 连接档来回切。
        端口与模型改完<strong>下次起桥接时生效</strong>
        ，正在跑的那条不受影响。
      </p>
      {/* 每条桥一块：状态 · 缺什么 · 端口 / 模型 / 地址 全在同一块里。
          原来端口挤在下面一个统一网格里，改哪个都得先抬头数一遍是第几条。 */}
      <div className="bridgelist mt-3">
        {visibleRows.map((b) => (
          <section
            key={b.key}
            className={`bridge-provider bridge-provider--${b.key}${b.running ? " is-up" : ""}`}
            data-provider={b.key}
          >
            <span className="bridge-provider-eyebrow">{b.eyebrow}</span>
            <div className="bridgelist-head">
              <strong>{b.name}</strong>
              {/* Claude 那条活在面板外（使用者自己那个 bridge.py 进程），
                  这里拿不到真状态 —— 不谎报，写明去哪儿看。 */}
              <Pill tone={b.running ? "ok" : "default"}>
                {b.running === undefined
                  ? "状态见插件页"
                  : b.running
                    ? "在跑"
                    : "没在跑"}
              </Pill>
              {b.slot && <span className="bridgelist-slot">槽位 {b.slot}</span>}
            </div>
            <p className="bridge-provider-summary">{b.summary}</p>
            <p className="bridgelist-sub">{b.drives}</p>
            {b.blockers.length > 0 && (
              <ul className="bridgelist-blockers">
                {b.blockers.map((t) => (
                  <li key={t}>{t}</li>
                ))}
              </ul>
            )}
            {cfg && (
              <div className="bridgelist-fields">
                <PortField
                  label="端口"
                  value={b.port}
                  onChange={(v) => setCfgDraft({ ...cfg, [b.portKey]: v })}
                />
                {/* Claude 那条没有「模型」可调（bridge.py 自己决定）。
                    放一个写着「—」的只读框只是占位，点不动又要人读一遍。 */}
                {b.modelKey && (
                  <TextField
                    label={b.modelLabel}
                    value={b.model ?? ""}
                    placeholder={b.modelHint}
                    onChange={(v) => setCfgDraft({ ...cfg, [b.modelKey]: v })}
                  />
                )}
                {b.promptKey && (
                  <label className="field">
                    <span className="field-label">独立角色提示词（可选）</span>
                    <textarea
                      className="tavern-provider-prompt"
                      rows={3}
                      value={cfg[b.promptKey]}
                      placeholder="只作用于这一条桥；酒馆角色卡会继续保留。"
                      onChange={(event) =>
                        setCfgDraft({
                          ...cfg,
                          [b.promptKey]: event.target.value,
                        })
                      }
                    />
                  </label>
                )}
                <div className="bridgelist-url">
                  <span className="mono">{b.url}</span>
                  <Button
                    size="sm"
                    icon={<Copy size={12} />}
                    onClick={() => void copyBridgeUrl(b.url)}
                  >
                    复制地址
                  </Button>
                </div>
              </div>
            )}
            {b.key === "gpt" && (
              <QuotaPanel
                title="ChatGPT / Codex 剩余额度"
                windows={gptQuota?.windows ?? []}
                account={gptQuota?.account}
                fetchedAt={gptQuota?.fetched_at}
                busy={gptBusy}
                error={gptError}
                onRefresh={() => void loadGpt(true)}
                now={clock}
              />
            )}
            {b.key === "gemini" && (
              <GeminiQuotaPanel
                quota={geminiQuota}
                busy={geminiBusy}
                error={geminiError}
                onRefresh={() => void loadGemini(true)}
                now={clock}
              />
            )}
            {b.key !== "claude" && (
              <div className="tavern-roleplay-test">
                <div className="tavern-roleplay-head">
                  <strong>
                    {b.key === "gpt"
                      ? "ChatGPT / Codex 角色扮演链路"
                      : "Gemini CLI 角色扮演链路"}
                  </strong>
                  <Pill
                    tone={
                      testResult?.provider === b.key && testResult.ok
                        ? "ok"
                        : "default"
                    }
                  >
                    {testResult?.provider === b.key
                      ? testResult.ok
                        ? "通过"
                        : "失败"
                      : "未测试"}
                  </Pill>
                </div>
                <textarea
                  className="tavern-roleplay-fixture"
                  rows={2}
                  value={fixture}
                  onChange={(event) => setFixture(event.target.value)}
                  aria-label={`${b.name}角色扮演测试提示词`}
                  data-testid={`${b.key}-roleplay-fixture`}
                />
                <Button
                  size="sm"
                  loading={testing === b.key}
                  onClick={async () => {
                    setTesting(b.key);
                    try {
                      const result = await api.tavernBridgeRoleplayTest(
                        b.key,
                        fixture,
                      );
                      setTestResult(result);
                      result.ok
                        ? toast.ok(`${b.name}角色扮演测试通过`)
                        : toast.error(result.detail);
                    } catch (e) {
                      toast.error(e instanceof Error ? e.message : String(e));
                    } finally {
                      setTesting(null);
                    }
                  }}
                  data-testid={`${b.key}-roleplay-test`}
                >
                  测试真实桥接
                </Button>
                {testResult?.provider === b.key && testResult.reply && (
                  <p className="tavern-roleplay-reply">{testResult.reply}</p>
                )}
              </div>
            )}
          </section>
        ))}
      </div>
      {cfg && (
        <div className="mt-3">
          <PortField
            label="酒馆端口（三条桥共用这一个酒馆）"
            value={cfg.st_port}
            onChange={(v) => setCfgDraft({ ...cfg, st_port: v })}
          />
        </div>
      )}
      <div className="mt-3 flex flex-wrap items-center gap-2">
        <Button
          variant="primary"
          loading={cfgBusy}
          disabled={!cfgDirty || cfgBusy}
          onClick={() => void saveBridgeCfg()}
        >
          保存
        </Button>
        <Button
          disabled={!cfgDirty || cfgBusy}
          onClick={() => setCfgDraft(null)}
        >
          放弃改动
        </Button>
        {/* 路径、资产盘点、备份恢复、Claude 桥的提示词与监控仍在插件详情页 ——
            那些不是「起之前要调的旋钮」。 */}
        {showPathsLink && (
          <Button
            onClick={() => {
              onClose();
              navigate("/extensions/sillytavern");
            }}
          >
            路径 · 提示词 · 监控…
          </Button>
        )}
        {cfgDirty && <span className="bridgelist-dirty">改动还没保存</span>}
      </div>
    </Modal>
  );
}

function resetLabel(epoch: number | null, now = Date.now()) {
  if (!epoch) return "重置时间未知";
  const seconds = Math.max(0, epoch - Math.floor(now / 1000));
  if (seconds < 60) return "即将重置";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 24) return `${Math.floor(hours / 24)} 天后重置`;
  if (hours > 0) return `${hours} 小时 ${minutes} 分后重置`;
  return `${minutes} 分后重置`;
}

function QuotaPanel({
  title,
  windows,
  account,
  fetchedAt,
  busy,
  error,
  onRefresh,
  now,
}: {
  title: string;
  windows: TavernGptQuota["windows"];
  account?: string | null;
  fetchedAt?: string;
  busy: boolean;
  error: string | null;
  onRefresh: () => void;
  now: number;
}) {
  return (
    <div
      className="tavern-quota-card tavern-quota-card--gpt"
      data-testid="tavern-gpt-quota"
    >
      <div className="tavern-quota-head">
        <strong>{title}</strong>
        <Button size="sm" loading={busy} onClick={onRefresh}>
          刷新
        </Button>
      </div>
      {account && <span className="tavern-quota-meta">{account}</span>}
      {error && <p className="tavern-quota-error">{error}</p>}
      <div className="tavern-quota-grid">
        {windows.map((window) => (
          <div key={window.label} className="tavern-quota-window">
            <QuotaBar
              name={window.label}
              used={
                window.remaining_percent == null
                  ? null
                  : 100 - window.remaining_percent
              }
              value={
                window.remaining_percent == null
                  ? "剩余未知"
                  : `剩 ${window.remaining_percent}%`
              }
              extra={
                <span className="gauge-reset">
                  ↻ {window.reset_at ?? "重置时间未知"} ·{" "}
                  {resetLabel(window.reset_epoch, now)}
                </span>
              }
              className="tavern-quota-gauge"
            />
          </div>
        ))}
      </div>
      {windows.length === 0 && (
        <p className="tavern-quota-empty">
          还没联网问过；点「刷新」问一次（只在你点的时候问）。
        </p>
      )}
      <small className="tavern-quota-meta">
        {fetchedAt ? `读取于 ${fetchedAt}` : "尚未读取额度"}
      </small>
    </div>
  );
}

function GeminiQuotaPanel({
  quota,
  busy,
  error,
  onRefresh,
  now,
}: {
  quota: TavernGeminiQuota | null;
  busy: boolean;
  error: string | null;
  onRefresh: () => void;
  now: number;
}) {
  return (
    <div
      className="tavern-quota-card tavern-quota-card--gemini"
      data-testid="tavern-gemini-quota"
    >
      <div className="tavern-quota-head">
        <strong>Google Gemini CLI 模型额度</strong>
        <Button size="sm" loading={busy} onClick={onRefresh}>
          刷新
        </Button>
      </div>
      {quota?.tier && (
        <span className="tavern-quota-meta">
          {quota.tier}
          {quota.project ? ` · ${quota.project}` : ""}
        </span>
      )}
      {error && <p className="tavern-quota-error">{error}</p>}
      <div className="tavern-quota-models">
        {(quota?.models ?? []).map((model) => (
          <div
            // 汇总接口的桶没有 model_id / token_type（2026-09-25 起不再被合成一个），键要带上名字。
            key={`${model.model_id}-${model.token_type}-${model.label}`}
            className="tavern-quota-window"
          >
            <QuotaBar
              name={model.label}
              used={
                model.remaining_percent == null
                  ? null
                  : 100 - model.remaining_percent
              }
              value={
                model.remaining_percent == null
                  ? "剩余未知"
                  : `剩 ${model.remaining_percent}%`
              }
              extra={
                <span
                  className="gauge-reset"
                  title={
                    model.remaining_implied
                      ? "Google 没给这一格的比例 —— 它的 JSON 会把 0 省掉，按用光算"
                      : undefined
                  }
                >
                  ↻ {model.reset_at ?? "重置时间未知"} ·{" "}
                  {resetLabel(model.reset_epoch, now)}
                  {model.remaining_implied && <em>推算</em>}
                </span>
              }
              className="tavern-quota-gauge"
            />
          </div>
        ))}
      </div>
      {(quota?.models.length ?? 0) === 0 && (
        <p className="tavern-quota-empty">
          还没联网问过；登录 CLI 后点「刷新」问一次（只在你点的时候问）。
        </p>
      )}
      <small className="tavern-quota-meta">
        {quota ? `读取于 ${quota.fetched_at}` : "尚未读取额度"}
      </small>
    </div>
  );
}

/**
 * 三个账户页的「酒馆」磁贴共用的那颗角标。
 *
 * ⛔ 磁贴本身是一个 `<button>`，`button` 套 `button` 是非法 HTML ——
 * 所以它是 `Tile` 的 `corner`（兄弟节点），不是塞进磁贴里的。见 `Tile.tsx`。
 */
export function BridgeSettingsCorner({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      title="桥接设置：三条桥的端口与模型，全在面板里改"
      aria-label="桥接设置"
      data-testid="tavern-settings-corner"
      onClick={onClick}
    >
      <Settings2 size={13} />
    </button>
  );
}
