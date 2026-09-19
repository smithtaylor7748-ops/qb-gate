import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowRight,
  Check,
  FileSearch,
  FlaskConical,
  LockKeyhole,
  ReceiptText,
  ShieldCheck,
} from "lucide-react";
import { call } from "../../lib/ipc";
import {
  stationApi,
  type Route,
  type StationModelsView,
} from "../../lib/station";
import type { StoredAudit } from "../../lib/generated/StoredAudit";
import type { StationBillingSettings } from "../../lib/generated/StationBillingSettings";
import { Button } from "../../ui";
import StationBilling from "./StationBilling";
import StationBillImport from "./StationBillImport";
import "./station-audit.css";

const money = (n: number | null | undefined, currency = "USD") =>
  n == null ? "—" : (currency === "USD" ? "$" : currency + " ") + n.toFixed(5);
const pct = (n: number | null | undefined) =>
  n == null ? "—" : (n * 100).toFixed(1) + "%";
const count = (n: number | null | undefined) =>
  n == null ? "—" : n.toLocaleString();
const category = {
  input: "未缓存输入",
  "cache-read": "缓存读取",
  "cache-write": "缓存写入",
  output: "输出",
};

export default function StationAudit({
  routes,
  focus = null,
  stationName,
  onBusyChange,
}: {
  routes: Route[];
  focus?: string | null;
  stationName?: string;
  onBusyChange?: (busy: boolean) => void;
}) {
  const [routeId, setRouteId] = useState(focus || routes[0]?.id || "");
  const route = routes.find((r) => r.id === routeId);
  const [settings, setSettings] = useState<StationBillingSettings | null>(null);
  const billingChanged = useCallback(
    (value: StationBillingSettings | null) => setSettings(value),
    [],
  );
  const [models, setModels] = useState<StationModelsView | null>(null);
  const [model, setModel] = useState("");
  const [key, setKey] = useState("");
  const [cold, setCold] = useState(false);
  const [coldConsent, setColdConsent] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [history, setHistory] = useState<StoredAudit[]>([]);
  const [selected, setSelected] = useState(0);
  const [view, setView] = useState("setup");
  const [tab, setTab] = useState("basic");
  const [progress, setProgress] = useState({
    completed: 0,
    total: 6,
    stage: "检查账单连接",
  });
  const refresh = useCallback(async () => {
    if (!routeId) return;
    try {
      setHistory(await call<StoredAudit[]>("station_audits", { routeId }));
    } catch (e) {
      setError("历史读取失败：" + String(e));
    }
  }, [routeId]);
  useEffect(() => {
    let live = true;
    setModel("");
    setKey("");
    setModels(null);
    setSettings(null);
    setHistory([]);
    setSelected(0);
    setCold(false);
    setColdConsent(false);
    setError("");
    setView("setup");
    void refresh();
    if (routeId)
      stationApi
        .models(routeId)
        .then((v) => {
          if (live) {
            setModels(v);
            setModel((m) => m || v.recommended || "");
          }
        })
        .catch((e) => {
          if (live)
            setModels({ models: [], recommended: null, problem: String(e) });
        });
    return () => {
      live = false;
    };
  }, [routeId, refresh]);
  useEffect(() => {
    if (!busy) return;
    let stopped = false;
    let off: (() => void) | undefined;
    void listen<{
      routeId: string;
      completed: number;
      total: number;
      stage: string;
    }>("station-audit-progress", (e) => {
      if (e.payload.routeId === routeId) setProgress(e.payload);
    })
      .then((fn) => {
        if (stopped) fn();
        else off = fn;
      })
      .catch(() => undefined);
    return () => {
      stopped = true;
      off?.();
    };
  }, [busy, routeId]);
  useEffect(() => {
    if (!settings?.configured || !routeId) return;
    let live = true;
    stationApi
      .models(routeId)
      .then((v) => {
        if (live) {
          setModels(v);
          setModel((m) => m || v.recommended || "");
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [settings?.configured, settings?.verified_at, routeId]);
  const run = async () => {
    setBusy(true);
    onBusyChange?.(true);
    setError("");
    setProgress({ completed: 0, total: cold ? 7 : 6, stage: "检查账单连接" });
    const testKey = key.trim();
    setKey("");
    try {
      const report = await call<StoredAudit>("station_run_audit", {
        routeId,
        model: model.trim(),
        testKey,
        cold,
      });
      setHistory((current) => [report, ...current]);
      setSelected(0);
      setTab("basic");
      setView("report");
    } catch (e) {
      setError("检验失败：" + (e instanceof Error ? e.message : String(e)));
    } finally {
      setBusy(false);
      onBusyChange?.(false);
    }
  };
  const round = history[selected]?.round;
  const batch = round?.batch;
  const issues = [
    ...(round?.problems || []),
    ...(batch?.samples.flatMap((s, i) =>
      s.problem ? ["第 " + (i + 1) + " 次 · " + s.problem] : [],
    ) || []),
  ];
  const matches =
    batch?.samples.filter((s) => s.match_kind === "request-id").length || 0;
  const ready =
    settings?.configured &&
    model.trim() &&
    key.trim() &&
    (!cold || coldConsent);
  return (
    <div className="audit-workbench">
      <header className="audit-hero">
        <div className="audit-emblem">
          <ShieldCheck size={29} />
        </div>
        <div>
          <span className="audit-eyebrow">BILLING INSPECTOR</span>
          <h2>查清每一次扣费</h2>
          <p>
            {stationName || route?.station_id || "选择线路"}
            <span> / </span>
            {route?.group || "默认分组"} · 核对缓存、Token 与实际账单
          </p>
        </div>
        <span className="audit-local">
          <LockKeyhole size={13} />
          本机保存
        </span>
      </header>
      <div className="audit-navigation">
        <button
          aria-pressed={view === "setup"}
          disabled={busy}
          onClick={() => setView("setup")}
        >
          <FlaskConical size={16} />
          配置与检验
        </button>
        <button
          aria-pressed={view === "report"}
          disabled={busy}
          onClick={() => setView("report")}
        >
          <ReceiptText size={16} />
          检验报告{history.length > 0 && <span>{history.length}</span>}
        </button>
      </div>
      {error && (
        <p role="alert" className="audit-error">
          {error}
        </p>
      )}
      {view === "setup" ? (
        <div className="audit-layout">
          <main className="audit-main">
            {routes.length > 1 && (
              <label className="audit-route">
                待检验线路
                <select
                  value={routeId}
                  disabled={busy}
                  onChange={(e) => setRouteId(e.target.value)}
                >
                  {routes.map((r) => (
                    <option key={r.id} value={r.id}>
                      {r.station_id} · {r.group || "默认分组"}
                    </option>
                  ))}
                </select>
              </label>
            )}
            {route && (
              <StationBilling
                key={route.station_id}
                stationId={route.station_id}
                disabled={busy}
                onChange={billingChanged}
              />
            )}
            <section className="audit-test">
              <div className="audit-section-heading">
                <span className="audit-step">02</span>
                <div>
                  <h3>设置本轮检验</h3>
                  <p>1 次预热 + 5 次缓存复用验证</p>
                </div>
                <FlaskConical size={19} />
              </div>
              <div className="audit-fields">
                <label>
                  检验模型
                  <input
                    aria-label="检验模型"
                    list="audit-models"
                    value={model}
                    disabled={busy}
                    onChange={(e) => setModel(e.target.value)}
                    placeholder="选择或手动输入模型"
                  />
                  <datalist id="audit-models">
                    {models?.models.map((m) => (
                      <option key={m.model} value={m.model} />
                    ))}
                  </datalist>
                </label>
                <label>
                  本次临时 API Key
                  <input
                    type="password"
                    autoComplete="off"
                    value={key}
                    disabled={busy}
                    onChange={(e) => setKey(e.target.value)}
                    placeholder="仅用于这一轮检验"
                  />
                </label>
              </div>
              {models?.problem && (
                <p className="audit-caption">{models.problem}</p>
              )}
              <p className="audit-caption">
                <LockKeyhole size={13} />
                测试 Key 仅在本轮内存中使用，提交后清空，不写入历史或线路。
              </p>
              <label className="audit-option">
                <input
                  type="checkbox"
                  checked={cold}
                  disabled={busy}
                  onChange={(e) => {
                    setCold(e.target.checked);
                    setColdConsent(false);
                  }}
                />
                <span>
                  <strong>增加冷前缀对照</strong>
                  <small>
                    第 7 次更换前缀顺序，帮助区分背景缓存与本轮复用。
                  </small>
                </span>
                <span className="audit-chip">可选</span>
              </label>
              {cold && (
                <label className="audit-consent">
                  <input
                    type="checkbox"
                    checked={coldConsent}
                    disabled={busy}
                    onChange={(e) => setColdConsent(e.target.checked)}
                  />
                  我确认增加第 7 次真实请求，并承担这次额外费用
                </label>
              )}
              <div className="audit-run">
                <div>
                  <strong>本轮最多 {cold ? 7 : 6} 次计费请求</strong>
                  <p>每次输出上限 112 Token。实际费用以站点为准。</p>
                </div>
                <Button
                  variant="primary"
                  disabled={busy || !ready}
                  onClick={() => void run()}
                >
                  {busy ? "检验进行中…" : "开始检验"}
                  {!busy && <ArrowRight size={16} />}
                </Button>
              </div>
              {!settings?.configured && (
                <p className="audit-caption">
                  先完成上方后台登录，再开始计费检验。
                </p>
              )}
              {busy && (
                <div className="audit-progress" role="status">
                  <div>
                    <strong>{progress.stage}</strong>
                    <span>
                      {progress.completed} / {progress.total}
                    </span>
                  </div>
                  <progress max={progress.total} value={progress.completed} />
                  <p>
                    失败会停止后续请求；账单可能延迟，核对最多等待约 77 秒。
                  </p>
                </div>
              )}
            </section>
          </main>
          <aside className="audit-guide">
            <span className="audit-eyebrow">HOW IT WORKS</span>
            <h3>从登录，到证据</h3>
            <ol>
              <li className={settings?.configured ? "complete" : ""}>
                <i>{settings?.configured ? <Check size={14} /> : "1"}</i>
                <div>
                  <strong>连接后台</strong>
                  <p>自动读取登录会话、账户余额和消费记录。</p>
                </div>
              </li>
              <li>
                <i>2</i>
                <div>
                  <strong>运行受控请求</strong>
                  <p>同一批材料重复验证，记录真实用量与首字延迟。</p>
                </div>
              </li>
              <li>
                <i>3</i>
                <div>
                  <strong>逐条对账</strong>
                  <p>比对请求 ID、Token、实扣和前后余额，保留缺失项。</p>
                </div>
              </li>
            </ol>
            <div className="audit-guide-note">
              <ReceiptText size={20} />
              <strong>两种凭证，各有用途</strong>
              <p>
                后台账号用于读账单；临时 API Key
                用于发测试请求。二者都应属于当前站点和本人账户。
              </p>
            </div>
            <p className="audit-caption">
              检验由你点击后开始。登录、读取账单和查看历史不调用模型。
            </p>
          </aside>
        </div>
      ) : (
        <section className="audit-report">
          <div className="audit-report-top">
            <div>
              <span className="audit-eyebrow">INSPECTION REPORT</span>
              <h3>{round?.model || "还没有检验报告"}</h3>
            </div>
            {history.length > 0 && (
              <label>
                检验历史
                <select
                  value={selected}
                  onChange={(e) => setSelected(Number(e.target.value))}
                >
                  {history.map((h, i) => (
                    <option value={i} key={String(h.round.at_ms)}>
                      {new Date(Number(h.round.at_ms)).toLocaleString()} ·{" "}
                      {h.round.model}
                    </option>
                  ))}
                </select>
              </label>
            )}
          </div>
          {!round ? (
            <div className="audit-empty">
              <FileSearch size={40} />
              <h3>准备好后，开始第一轮检验</h3>
              <p>每一轮的缓存、账单和余额证据都会保存在这里。</p>
              <Button onClick={() => setView("setup")}>
                去配置检验
                <ArrowRight size={15} />
              </Button>
            </div>
          ) : (
            <>
              <div
                className="audit-report-tabs"
                role="tablist"
                aria-label="报告内容"
              >
                {[
                  ["basic", "基础报告"],
                  ["attention", "注意报告"],
                  ["evidence", "完整证据"],
                ].map(([id, label]) => (
                  <button
                    role="tab"
                    aria-selected={tab === id}
                    key={id}
                    onClick={() => setTab(id)}
                  >
                    {label}
                  </button>
                ))}
              </div>
              {tab === "basic" && (
                <>
                  <div className="audit-verdict">
                    <ShieldCheck size={25} />
                    <div>
                      <strong>
                        {batch
                          ? matches === batch.planned
                            ? "本轮请求已逐条匹配账单"
                            : "本轮仍有证据需要核实"
                          : "历史报告 · 旧版检验"}
                      </strong>
                      <p>
                        {batch
                          ? matches +
                            " / " +
                            batch.planned +
                            " 条有相同请求 ID。结论仅适用于本轮模型与分组。"
                          : "旧版没有逐次证据，可重新运行完整检验。"}
                      </p>
                    </div>
                  </div>
                  <div className="audit-metrics">
                    <Metric
                      label="本轮账单合计"
                      value={money(batch?.total_billed, batch?.currency)}
                      detail="完整匹配后才汇总"
                    />
                    <Metric
                      label="官方参考成本"
                      value={money(batch?.official_cost)}
                      detail="同一批 API Token · USD"
                    />
                    <Metric
                      label="前缀平均复用"
                      value={pct(batch?.prefix_reuse)}
                      detail="5 次验证 · 扣除预热背景"
                    />
                    <Metric
                      label="实扣 / 标称成本"
                      value={
                        round.mult == null ? "—" : round.mult.toFixed(3) + "×"
                      }
                      detail="按账单数值比较 · 不换算汇率"
                    />
                  </div>
                  <div className="audit-balance-flow">
                    <div>
                      <span>检验前余额</span>
                      <strong>
                        {money(batch?.balance_before, batch?.currency)}
                      </strong>
                    </div>
                    <ArrowRight size={18} />
                    <div>
                      <span>检验后余额</span>
                      <strong>
                        {money(batch?.balance_after, batch?.currency)}
                      </strong>
                    </div>
                    <div>
                      <span>账户余额变化</span>
                      <strong>
                        {money(batch?.balance_delta, batch?.currency)}
                      </strong>
                    </div>
                  </div>
                  <p className="audit-caption">
                    余额会受同账户的其它消费、充值和入账延迟影响。API、账单和余额均由站点报告，不作为模型身份鉴定。
                  </p>
                </>
              )}
              {tab === "attention" && (
                <div className="audit-attention">
                  <h4>需要留意的地方</h4>
                  {issues.length ? (
                    issues.map((text, i) => (
                      <div key={i}>
                        <span>{String(i + 1).padStart(2, "0")}</span>
                        <p>{text}</p>
                      </div>
                    ))
                  ) : (
                    <p>本轮没有额外提示；仍请结合完整证据判断。</p>
                  )}
                </div>
              )}
              {tab === "evidence" && (
                <>
                  <div className="audit-table-scroll">
                    <table>
                      <thead>
                        <tr>
                          <th>请求 / 阶段</th>
                          <th>未缓存输入</th>
                          <th>缓存读</th>
                          <th>缓存写</th>
                          <th>输出</th>
                          <th>实扣</th>
                          <th>匹配依据</th>
                        </tr>
                      </thead>
                      <tbody>
                        {batch?.samples.map((s, i) => (
                          <tr key={i}>
                            <td>
                              <strong>
                                {String(i + 1).padStart(2, "0")} · {s.stage}
                              </strong>
                              <small>
                                {s.request_id || "未完成"}
                                {s.problem && (
                                  <span className="audit-error-inline">
                                    {s.problem}
                                  </span>
                                )}
                              </small>
                            </td>
                            {s.api_tokens.map((n, j) => (
                              <td key={j}>
                                <strong>{count(n)}</strong>
                                <small>账单 {count(s.ledger_tokens[j])}</small>
                              </td>
                            ))}
                            <td>{money(s.billed, batch.currency)}</td>
                            <td>
                              <span
                                className={
                                  "audit-chip " +
                                  (s.match_kind === "request-id"
                                    ? "audit-chip-ok"
                                    : "")
                                }
                              >
                                {s.match_kind === "request-id"
                                  ? "请求 ID"
                                  : s.match_kind === "tokens-only"
                                    ? "仅 Token"
                                    : "未匹配"}
                              </span>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                  <h4>站点公布的计费结构</h4>
                  <p className="audit-caption">
                    以下是价目说明；实际收取的倍率以同一请求的账单和 API
                    用量核对。
                  </p>
                  <div className="audit-price-grid">
                    {round.rates.map((r) => (
                      <div key={r.category}>
                        <span>{category[r.category]}</span>
                        <strong>
                          {r.station_price != null
                            ? money(r.station_price) + " / M"
                            : r.station_ratio != null
                              ? r.station_ratio.toFixed(3) + "×"
                              : "未公布"}
                        </strong>
                        <small>官方 {money(r.official)} / M</small>
                      </div>
                    ))}
                  </div>
                  <p className="audit-caption">
                    历史仅保存加密后的结构化证据；不保存临时
                    Key、提示词和模型输出。
                  </p>
                </>
              )}
            </>
          )}
          <StationBillImport />
        </section>
      )}
    </div>
  );
}

function Metric({
  label,
  value,
  detail,
}: {
  label: string;
  value: string;
  detail: string;
}) {
  return (
    <div>
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{detail}</small>
    </div>
  );
}
