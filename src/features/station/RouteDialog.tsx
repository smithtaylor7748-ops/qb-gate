/**
 * 添加 / 编辑一条线路（§2.5，880px）。
 *
 * **一条线路 = 软件 + 站点 + 分组。一个分组一把 key，没有第三层。**
 *
 * 形态参考 cc-switch（MIT，见 ATTRIBUTION.md）的 `ProviderForm`：预设胶囊 +
 * 大图标 + 两列基本信息 +「接入」分节 + 收起的高级选项。数据模型是我们自己的。
 *
 * # ⛔ 预设里一个第三方中转站都没有
 *
 * 只收厂商公开文档里的**官方直连地址**。第三方中转站各家地址自己在变、
 * 很多要登录后台才看得到 —— 预置一个记错的地址，使用者会当成官方推荐，
 * 排查时先怀疑自己的 Key 而不是怀疑地址。这句话也写在界面上。
 *
 * # ⛔ 协议和倍率都自动来，手填只是兜底
 *
 * ⚡ 那个按钮探三种协议 + 拉 `/api/pricing`，两步都不花钱。手填的两项
 * （倍率兜底、协议手动改）降级在「高级选项」里，各带一句说明写清楚
 * 「这是你自己填的，不是站点公布的」。
 *
 * # ⛔ 三态不许塌成两态
 *
 * 协议是「不知道 / 支持 / 不支持」。探不出来（Key 错、网络断）时留「不知道」——
 * 猜成「不支持」会让这条线路在界面上整个消失，而使用者只是少粘了一位。
 */

import { useEffect, useMemo, useState } from "react";

import type { Client } from "../../lib/generated/Client";
import type { Credential } from "../../lib/generated/Credential";
import type { Preset } from "../../lib/generated/Preset";
import type { ProbeResult } from "../../lib/generated/ProbeResult";
import type { Provider } from "../../lib/generated/Provider";
import { api } from "../../lib/api";
import { saveStationConnection } from "../../lib/stationConnection";
import { modelRateLabel, stationApi, type Route } from "../../lib/station";
import { Button, Modal } from "../../ui";
import ConfigPane from "./ConfigPane";

/** 三态下拉的值。`""` = 还没探过。 */
type Tri = "" | "yes" | "no";

function toTri(v: boolean | null): Tri {
  return v == null ? "" : v ? "yes" : "no";
}
function fromTri(v: Tri): boolean | null {
  return v === "" ? null : v === "yes";
}

const SEP = ""; // Route::make_id 的分隔符

/**
 * 大图标按钮的几种配色。点一下换下一个。
 *
 * ⛔ 只引 `tokens.css` 的变量。这里是「哪几个变量」的列表，不是色值表。
 */
const TINTS = ["accent", "ok", "warn", "danger"] as const;

const CLIENT_NAME: Record<Client, string> = {
  "claude-code": "Claude Code",
  "claude-desktop": "Claude 桌面端",
  codex: "Codex",
  // 反重力没有中转路径（后端入口拒绝），线路对话框永远不会拿到它；键只是让 Record 完整。
  antigravity: "反重力",
  "antigravity-ide": "反重力 IDE",
};

/** 这个软件该看哪一侧的预设。 */
function presetTarget(client: Client): "codex" | "claude-code" {
  return client === "codex" ? "codex" : "claude-code";
}

export default function RouteDialog({
  open,
  onClose,
  onSave,
  providers,
  credentials,
  editing,
  client,
}: {
  open: boolean;
  onClose: () => void;
  onSave: (route: Route) => Promise<void>;
  providers: Provider[];
  credentials: Credential[];
  editing?: Route | null;
  /**
   * 这条线属于哪个软件。**新建时取当前分页的那个** —— 线路按软件独立，
   * 少了这一维，在 Codex 底下加的那条会顶掉 Claude Code 同名的那条
   * （库里是 id 做主键）。改已有的那条时沿用它自己的，不跟着分页跑。
   */
  client: Client;
}) {
  const owner = editing?.client ?? client;

  // ---- 基本信息
  const [name, setName] = useState("");
  const [note, setNote] = useState("");
  const [website, setWebsite] = useState("");
  /** 充值比例：1 美元站内额度付几元。站点级，空 = 按 1 算。 */
  const [topup, setTopup] = useState("");
  const [tint, setTint] = useState(0);
  // ---- 接入
  const [group, setGroup] = useState("");
  const [key, setKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [baseUrl, setBaseUrl] = useState("");
  const [fullUrl, setFullUrl] = useState(false);
  // ---- 高级
  const [advanced, setAdvanced] = useState(false);
  const [rate, setRate] = useState("");
  const [anthropic, setAnthropic] = useState<Tri>("");
  const [chat, setChat] = useState<Tri>("");
  const [responses, setResponses] = useState<Tri>("");
  const [credentialId, setCredentialId] = useState("");
  // ---- 探测
  const [probe, setProbe] = useState<ProbeResult | null>(null);
  const [probing, setProbing] = useState(false);
  const [probeError, setProbeError] = useState<string | null>(null);

  const [presets, setPresets] = useState<Preset[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const existing = useMemo(
    () => providers.find((p) => p.name.trim() === name.trim() && name.trim()),
    [providers, name],
  );

  useEffect(() => {
    if (!open) return;
    let dropped = false;
    void api
      .relayPresets(presetTarget(owner))
      .then((p) => !dropped && setPresets(p))
      .catch(() => {});
    return () => {
      dropped = true;
    };
  }, [open, owner]);

  useEffect(() => {
    if (!open) return;
    setError(null);
    setProbe(null);
    setProbeError(null);
    setShowKey(false);
    setAdvanced(false);
    setKey("");
    setFullUrl(false);
    if (editing) {
      const p = providers.find((x) => x.id === editing.station_id);
      setName(p?.name ?? editing.station_id);
      setNote(p?.note ?? "");
      setWebsite(p?.website ?? "");
      setTopup(p?.topup_per_usd == null ? "" : String(p.topup_per_usd));
      setBaseUrl(p?.base_url ?? "");
      setGroup(editing.group);
      setRate(editing.nominal_rate == null ? "" : String(editing.nominal_rate));
      setCredentialId(editing.credential_id ?? "");
      setAnthropic(toTri(editing.protocols.anthropic));
      setChat(toTri(editing.protocols.openai_chat));
      setResponses(toTri(editing.protocols.openai_responses));
    } else {
      setName("");
      setNote("");
      setWebsite("");
      setTopup("");
      setBaseUrl("");
      setGroup("");
      setRate("");
      setCredentialId("");
      setAnthropic("");
      setChat("");
      setResponses("");
    }
    // Reset only when opening another route, never on a workspace refresh.
  }, [open, editing]);

  // 新建时填了一个已有的站点名 = 复用那个站点：把它的充值比例带出来 ——
  // 否则保存时这一格的空白会把站点上已经填好的比例冲掉（它是整站共用的）。
  const existingId = existing?.id;
  const existingTopup = existing?.topup_per_usd ?? null;
  useEffect(() => {
    if (!open || editing || !existingId) return;
    setTopup(existingTopup == null ? "" : String(existingTopup));
  }, [open, editing, existingId, existingTopup]);

  const applyPreset = (p: Preset) => {
    setName(p.name);
    setBaseUrl(p.base_url);
    if (p.website) setWebsite(p.website);
    // ⛔ 预设只填地址和名字。它**不填协议** —— 预设知道的是「官方端点长什么样」，
    // 不是「你这条线通不通」。协议由 ⚡ 探出来，探不到就留「不知道」。
    setProbe(null);
  };

  const runProbe = async () => {
    setProbing(true);
    setProbeError(null);
    try {
      const r = await stationApi.probe(baseUrl, key || null, group || null);
      setProbe(r);
      // 探出来的写进三态。⛔ 探不出来的那几项**留原样** ——
      // 一次网络抖动不该把上一次探到的结论抹成「不知道」。
      if (r.protocols.anthropic != null)
        setAnthropic(toTri(r.protocols.anthropic));
      if (r.protocols.openai_chat != null)
        setChat(toTri(r.protocols.openai_chat));
      if (r.protocols.openai_responses != null)
        setResponses(toTri(r.protocols.openai_responses));
    } catch (e) {
      setProbeError(e instanceof Error ? e.message : String(e));
    } finally {
      setProbing(false);
    }
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const parsed = rate.trim() === "" ? null : Number(rate);
      if (parsed != null && (!Number.isFinite(parsed) || parsed <= 0)) {
        // 倍率会参与排序和底线筛选 —— 一个坏数字比没有数字危险。
        throw new Error("倍率兜底要是个大于 0 的数，留空表示交给检验去量");
      }
      const topupPerUsd = topup.trim() === "" ? null : Number(topup);
      if (
        topupPerUsd != null &&
        (!Number.isFinite(topupPerUsd) || topupPerUsd <= 0)
      ) {
        // 跨站比便宜要乘它 —— 同样是坏数字比没有数字危险。
        throw new Error(
          "充值比例要是个大于 0 的数（1 美元额度付几元），留空按 1 元 = 1 美元额度算",
        );
      }
      if (!name.trim()) throw new Error("站点名不能留空");
      if (!baseUrl.trim()) throw new Error("API 端点不能留空");

      const { stationId, credentialId: savedCredentialId } =
        await saveStationConnection(
          {
            id: existing?.id ?? "",
            name: name.trim(),
            base_url: baseUrl.trim(),
            website: website.trim(),
            note: note.trim(),
            tags: existing?.tags ?? [],
            favorite: existing?.favorite ?? false,
            revision: existing?.revision ?? 0,
            topup_per_usd: topupPerUsd,
          },
          key,
          credentials.find((c) => c.id === credentialId),
          group,
        );

      await onSave({
        id: owner + SEP + stationId + SEP + group,
        client: owner,
        station_id: stationId,
        group,
        credential_id: savedCredentialId,
        nominal_rate: parsed,
        // 改分组不该把已经量到的 mult 和检验时间抹掉。
        latest_mult: editing?.latest_mult ?? null,
        last_audit_ms: editing?.last_audit_ms ?? null,
        protocols: {
          anthropic: fromTri(anthropic),
          openai_chat: fromTri(chat),
          openai_responses: fromTri(responses),
        },
        // 计费价目由检验时从站点的 /api/pricing 拉，不在这里手填 ——
        // 手填一个 completion_ratio 会让「输出加价」这个显示变成猜的。
        output_markup: editing?.output_markup ?? null,
        rates: editing?.rates ?? {
          model_ratio: null,
          completion_ratio: null,
          cache_ratio: null,
          create_cache_ratio: null,
          group_ratio: null,
          peak_rate: null,
          per_request_price: null,
          input_price: null,
          cache_read_price: null,
          cache_write_price: null,
          output_price: null,
        },
        // 这组价是哪个模型的，也由检验那一步写回 —— 手填一个模型名会让
        // 官方价按错的那一行去查，算出的倍率错得看不出来。
        rates_model: editing?.rates_model ?? null,
      });
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const mine = credentials.filter((c) => c.provider_id === existing?.id);
  const custom = !presets.some((p) => p.base_url === baseUrl);

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={
        editing
          ? `编辑 ${CLIENT_NAME[owner]} 中转站`
          : `添加 ${CLIENT_NAME[owner]} 中转站`
      }
      size="form"
      footer={
        <>
          <span className="qb-st-note" style={{ flex: 1 }}>
            这条线只属于 <b>{CLIENT_NAME[owner]}</b> —— 同一个站点的同一个分组，
            在另一个软件底下是另一行，各绑各的 Key。
          </span>
          <Button variant="ghost" onClick={onClose}>
            取消
          </Button>
          <Button variant="primary" disabled={busy} onClick={() => void save()}>
            {editing ? "保存" : "＋ 添加"}
          </Button>
        </>
      }
    >
      <div className="qb-st-form">
        {/* ------------------------------------------------ 预设胶囊 */}
        <div className="qb-st-presets">
          <button
            className="chip"
            aria-pressed={custom}
            onClick={() => {
              setBaseUrl("");
              setName("");
            }}
          >
            自定义配置
          </button>
          {presets.map((p) => (
            <button
              key={p.id}
              className="chip"
              aria-pressed={p.base_url === baseUrl}
              title={p.note}
              onClick={() => applyPreset(p)}
            >
              ★ {p.name}
            </button>
          ))}
        </div>
        <p className="qb-st-hint">
          💡 第三方中转站不在预设里 ——
          各家地址自己在变、很多要登录后台才看得到。 预置一个记错的地址，
          你会当成官方推荐，排查时先怀疑自己的 Key。
        </p>

        {/* ------------------------------------------------ 大图标 */}
        <div className="qb-st-icon">
          <button
            className={`tile tile--${TINTS[tint]}`}
            title="换个颜色 —— 只影响这张卡怎么认，不影响任何行为"
            onClick={() => setTint((t) => (t + 1) % TINTS.length)}
          >
            {(name.trim()[0] ?? "＋").toUpperCase()}
          </button>
        </div>

        {/* ------------------------------------------------ 站点名 / 备注 */}
        <div className="qb-st-two">
          <Field
            label="站点名"
            hint={
              existing
                ? "已有同名站点 —— 会复用它，不重复建（余额整站共享）"
                : "填一个已有的名字就复用那个站点，不重复建"
            }
          >
            <input
              list="qb-station-names"
              value={name}
              placeholder="例如：我的 API 服务"
              onChange={(e) => setName(e.target.value)}
            />
            <datalist id="qb-station-names">
              {providers.map((p) => (
                <option key={p.id} value={p.name} />
              ))}
            </datalist>
          </Field>
          <Field label="备注">
            <input
              value={note}
              placeholder="给自己看的一句话"
              onChange={(e) => setNote(e.target.value)}
            />
          </Field>
        </div>

        <Field label="官网 / 控制台链接">
          <input
            value={website}
            placeholder="https://…（充值、查余额的那个页面）"
            onChange={(e) => setWebsite(e.target.value)}
          />
        </Field>

        <Field
          label="充值比例（1 美元额度 = ? 元）"
          hint="整个站点共用。大多数站是 1 元买 1 美元额度，留空就按 1 算；按真实汇率卖的站填 7 左右。跨站比便宜时倍率要先乘上它 —— 7 元一美元、标 ×0.1 的站，比 1 元一美元、标 ×0.5 的还贵。查套路的账单核对不受影响。"
        >
          <div className="qb-st-inline">
            <input
              value={topup}
              inputMode="decimal"
              placeholder="1"
              onChange={(e) => setTopup(e.target.value)}
            />
            {probe?.topup_hint != null && (
              <button
                className="qb-st-iconbtn"
                title="站点在 /api/status 公布的在线充值价（New API 后台的「充值价格」）。默认值是 7.3，很多站没改过、实际靠兑换码按 1 元卖 —— 只作参考"
                onClick={() => setTopup(String(probe.topup_hint))}
              >
                站点写 {probe.topup_hint}
              </button>
            )}
          </div>
        </Field>

        {/* ------------------------------------------------ 接入 */}
        <h4 className="qb-st-sect">接入</h4>

        <Field
          label="分组名"
          hint="一把 key 只绑一个分组 —— 要用这个站点的另一个分组，就再加一把 key"
        >
          <input
            value={group}
            disabled={Boolean(editing)}
            placeholder="照站点后台里的分组名抄，留空 = 默认分组"
            onChange={(e) => setGroup(e.target.value)}
          />
        </Field>

        <Field
          label="API Key"
          hint={
            editing
              ? "留空 = 不改动已经绑着的那把。这里填的只用来探测。"
              : "只用来探测。真正绑给这条线的那把在「高级选项」里挑。"
          }
        >
          <div className="qb-st-inline">
            <input
              type={showKey ? "text" : "password"}
              value={key}
              placeholder="sk-…"
              onChange={(e) => setKey(e.target.value)}
            />
            {/* ⛔ 眼睛是输入框**右侧的独立按钮**，不是框内图标 ——
                框内图标会盖住最后几个字符，而那几个字符正是最常粘错的。 */}
            <button
              className="qb-st-iconbtn"
              aria-pressed={showKey}
              title={showKey ? "藏起来" : "看一眼"}
              onClick={() => setShowKey((v) => !v)}
            >
              {showKey ? "🙈" : "👁"}
            </button>
          </div>
        </Field>

        <Field label="API 端点">
          <div className="qb-st-inline">
            <input
              value={baseUrl}
              placeholder="https://…/v1"
              onChange={(e) => setBaseUrl(e.target.value)}
            />
            <button
              className="qb-st-iconbtn qb-st-iconbtn--go"
              disabled={probing || !baseUrl.trim()}
              title="探三种协议 + 拉价目表。两步都不花钱。"
              onClick={() => void runProbe()}
            >
              {probing ? "…" : "⚡"}
            </button>
          </div>
        </Field>

        <label className="qb-st-check">
          <input
            type="checkbox"
            checked={fullUrl}
            onChange={(e) => setFullUrl(e.target.checked)}
          />
          完整 URL 端点模式
        </label>
        <p className="qb-st-note">
          开了就原样用这个地址，不追加 <code>/v1/messages</code>。
        </p>

        {/* ------------------------------------------------ 探测结果 */}
        <ProbeBox
          probe={probe}
          probing={probing}
          error={probeError}
          anthropic={anthropic}
          chat={chat}
          responses={responses}
        />

        {/* ------------------------------------------------ 配置 JSON */}
        <details className="qb-st-adv">
          {/* ⛔ 不写「配置 JSON」—— Codex 那份是 TOML，标成 JSON 会让人
              以为自己打开错了文件。 */}
          <summary>配置文件 · {CLIENT_NAME[owner]} 共用</summary>
          <ConfigPane client={owner} />
        </details>

        {/* ------------------------------------------------ 高级选项 */}
        <details
          className="qb-st-adv"
          open={advanced}
          onToggle={(e) => setAdvanced((e.target as HTMLDetailsElement).open)}
        >
          <summary>高级选项</summary>
          <p className="qb-st-note">
            正常不用动这里 —— 下面两项都是<b>探不出来时的兜底</b>， 填进去的是
            <b>你自己说的</b>，不是站点公布的。
          </p>

          <Field
            label="这个分组用哪把 Key"
            hint="站点建好之后才挑得了 —— 新站点先保存一次，再回来绑"
          >
            <select
              value={credentialId}
              onChange={(e) => setCredentialId(e.target.value)}
            >
              <option value="">（还没绑）</option>
              {mine.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.label} · {c.masked}
                </option>
              ))}
            </select>
          </Field>

          <Field
            label="倍率兜底"
            hint="站点没开 /api/pricing 时才用。检验量到实测值之后，排序按实测的那个比。"
          >
            <input
              value={rate}
              placeholder="站点标的那个数，例如 0.2；留空 = 交给检验去量"
              onChange={(e) => setRate(e.target.value)}
            />
          </Field>

          <div>
            <span className="qb-st-k">协议 · 手动改</span>
            <p className="qb-st-note">
              正常别动，只在探不出来而你确知答案时才改。
              <b>「不知道」和「不支持」是两回事</b> ——
              不知道的会照常列出来让你自己试，标成不支持就彻底看不见了。
            </p>
            <div className="qb-st-tris">
              <TriSelect
                label="Anthropic Messages"
                value={anthropic}
                onChange={setAnthropic}
              />
              <TriSelect
                label="OpenAI Responses"
                value={responses}
                onChange={setResponses}
              />
              <TriSelect label="OpenAI Chat" value={chat} onChange={setChat} />
            </div>
          </div>
        </details>

        {error && (
          <div className="qb-st-nudge">
            <span aria-hidden="true">⚠</span>
            <span>{error}</span>
          </div>
        )}
      </div>
    </Modal>
  );
}

// ------------------------------------------------------------------ 探测结果区

/**
 * 三行：协议 / 计费 / 模型。
 *
 * ⛔ **没探过时如实说「还没探过」**，不要把三条协议画成灰色的「不支持」——
 * 那是两件事，而画成一样的话使用者会去改配置。
 */
function ProbeBox({
  probe,
  probing,
  error,
  anthropic,
  chat,
  responses,
}: {
  probe: ProbeResult | null;
  probing: boolean;
  error: string | null;
  anthropic: Tri;
  chat: Tri;
  responses: Tri;
}) {
  const rows: [string, Tri][] = [
    ["Anthropic Messages", anthropic],
    ["OpenAI Responses", responses],
    ["OpenAI Chat", chat],
  ];
  return (
    <div className="qb-st-probe">
      <div className="row">
        <span className="k">协议</span>
        <span className="v">
          {probing ? (
            "正在探…"
          ) : rows.every(([, v]) => v === "") ? (
            <span className="qb-st-dim">还没探过 —— 点 ⚡ 让它自己认</span>
          ) : (
            rows.map(([label, v]) => (
              <span
                key={label}
                className={`qb-st-pill ${
                  v === "yes"
                    ? "qb-st-pill--ok"
                    : v === "no"
                      ? "qb-st-pill--danger"
                      : "qb-st-pill--unknown"
                }`}
                title={
                  v === "yes"
                    ? "探通了"
                    : v === "no"
                      ? "上游明确拒绝"
                      : "探不出来 —— 不是「不支持」"
                }
              >
                {label} {v === "yes" ? "✓" : v === "no" ? "✕" : "?"}
              </span>
            ))
          )}
        </span>
      </div>
      <div className="row">
        <span className="k">计费</span>
        <span className="v">
          {probing ? (
            "正在拉…"
          ) : probe == null ? (
            <span className="qb-st-dim">还没探过</span>
          ) : probe.pricing_problem ? (
            <span className="qb-st-dim">{probe.pricing_problem}</span>
          ) : (
            `站点公布了 ${probe.models.length} 个模型的价`
          )}
        </span>
      </div>
      <div className="row">
        <span className="k">模型</span>
        <span className="v">
          {probe == null || probe.models.length === 0 ? (
            <span className="qb-st-dim">
              {probing ? "正在拉…" : "还没探到 —— 检验时可以自己填模型名"}
            </span>
          ) : (
            <span className="qb-st-models">
              {probe.models.slice(0, 6).map((m) => (
                <span key={m.model} title={modelRateLabel(m)}>
                  {m.model}
                </span>
              ))}
              {probe.models.length > 6 && (
                <span className="qb-st-dim">
                  还有 {probe.models.length - 6} 个
                </span>
              )}
            </span>
          )}
        </span>
      </div>
      {error && <p className="qb-st-note">探测失败：{error}</p>}
    </div>
  );
}

// ------------------------------------------------------------------ 小件

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="qb-st-fieldrow">
      <span className="qb-st-k">{label}</span>
      {children}
      {hint && <span className="qb-st-note">{hint}</span>}
    </label>
  );
}

function TriSelect({
  label,
  value,
  onChange,
}: {
  label: string;
  value: Tri;
  onChange: (v: Tri) => void;
}) {
  return (
    <label className="qb-st-fieldrow">
      <span className="qb-st-k">{label}</span>
      <select value={value} onChange={(e) => onChange(e.target.value as Tri)}>
        <option value="">不知道</option>
        <option value="yes">支持</option>
        <option value="no">不支持</option>
      </select>
    </label>
  );
}
