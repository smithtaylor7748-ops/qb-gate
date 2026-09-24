/**
 * Claude 桥接的设置与监控 —— 原来是 `bridge.py` 自己那两个网页（0.32.0，使用者定的）。
 *
 * # 原来长什么样
 *
 * 那七个旋钮和那张调用日志表**不是面板的东西**：它们是 `bridge.py` 起的
 * `GET /settings` 与 `GET /monitor` 两个 HTML 页，由使用者本机那份 SillyTavern 扩展
 * （`claude-tavern-bridge`，**面板不分发它**）用 `<iframe>` 嵌进酒馆的扩展抽屉。
 * 改一句提示词要先起酒馆、进抽屉、等 iframe 加载；而用 GPT / Gemini 那两条桥聊天时，
 * 那个 iframe 指着没人听的 5001，弹 `ERR_CONNECTION_REFUSED`（坑 7.66）。
 *
 * 现在面板直接调它的 HTTP 接口。
 *
 * # ⛔ 三条
 *
 * 1. **桥没在跑 / 没有这个接口时，照实说后端那句话。** 那几句是能照着做的
 *    （「到状态条点启动酒馆」「升级 bridge.py」「密钥对不上，重起一次」），
 *    而「加载失败」不是。`tavern_bridge_api::explain` 已经把话写好了，这里原样显示。
 * 2. **改设置会退掉当前那条 SDK 会话。** `bridge.py` 在这七项里任何一项变了时都会
 *    `cancel_active(retire=True)` —— 代价常驻在保存按钮旁边，不放悬停提示。
 * 3. **清空日志不可逆**，走 `ConfirmDialog`。
 */
import { useCallback, useEffect, useState } from "react";
import { Gauge, RefreshCw, Trash2 } from "lucide-react";

import { api } from "../lib/api";
import type { BridgeSettings } from "../lib/generated/BridgeSettings";
import type { BridgeSettingsView } from "../lib/generated/BridgeSettingsView";
import type { BridgeTelemetry } from "../lib/generated/BridgeTelemetry";
import { Button, Card, ConfirmDialog, Field, Pill, useToast } from "../ui";

const PAGE = 20;
const NUM = new Intl.NumberFormat("zh-CN");
const short = (n: number) =>
  n >= 1_000_000
    ? (n / 1_000_000).toFixed(1) + "M"
    : n >= 1_000
      ? (n / 1_000).toFixed(1) + "K"
      : NUM.format(n);

/** 前缀稳定性那一列的人话。桥给的是英文枚举，原样显示没人读得懂。 */
const PREFIX_TEXT: Record<string, string> = {
  identical: "完全一致",
  append_only: "只追加",
  rewound: "回退过",
  first: "首轮",
  system_changed: "系统提示词变了",
  settings_changed: "设置变了",
  history_rewritten: "历史被改写",
};

/** 多行提示词。`TextField` 只有单行 input，而这两项常常是几百字。 */
function Prompt({
  label,
  hint,
  value,
  onChange,
}: {
  label: string;
  hint?: string;
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <Field label={label} hint={hint}>
      {(p) => (
        <textarea
          {...p}
          className="input"
          rows={3}
          maxLength={262144}
          spellCheck={false}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </Field>
  );
}

function Select({
  label,
  hint,
  value,
  options,
  onChange,
}: {
  label: string;
  hint?: string;
  value: string;
  options: string[];
  onChange: (v: string) => void;
}) {
  return (
    <Field label={label} hint={hint}>
      {(p) => (
        <select
          {...p}
          className="input"
          value={value}
          onChange={(e) => onChange(e.target.value)}
        >
          {options.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      )}
    </Field>
  );
}

export default function ClaudeBridgePanel() {
  const toast = useToast();
  const [view, setView] = useState<BridgeSettingsView | null>(null);
  const [draft, setDraft] = useState<BridgeSettings | null>(null);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const [version, setVersion] = useState("");

  const load = useCallback(async () => {
    setBusy(true);
    setErr("");
    try {
      // 先探一次活：桥没在跑时这一条给的话最具体。
      const h = await api.tavernBridgeHealth();
      setVersion(h.bridge_version);
      const v = await api.tavernBridgeSettings();
      setView(v);
      setDraft(null);
    } catch (e) {
      setView(null);
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load]);

  const cur = draft ?? view?.settings ?? null;
  const dirty = !!draft;
  const patch = (p: Partial<BridgeSettings>) =>
    cur && setDraft({ ...cur, ...p });

  async function save() {
    if (!cur) return;
    setBusy(true);
    try {
      await api.tavernBridgeSettingsSave(cur);
      toast.ok(
        "已保存到 bridge.py 自己的 settings.json；当前那条 SDK 会话已退掉",
      );
      await load();
    } catch (e) {
      // ⛔ 照搬桥的原话：它自己的 allowlist 可能比面板这份快照新。
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card
      title={
        <>
          <Gauge size={14} />
          Claude 桥接设置
          {version && <Pill tone="default">v{version}</Pill>}
        </>
      }
      actions={
        <Button
          size="sm"
          icon={<RefreshCw size={12} />}
          loading={busy}
          aria-label="重新读取 Claude 桥接的设置"
          onClick={() => void load()}
        />
      }
    >
      <p className="notice">
        这些存在 <code>bridge.py</code> 自己的 <code>settings.json</code> 里，
        面板只是替你改。原来要到酒馆的扩展抽屉里开那个嵌入页才看得见。
      </p>
      {err && (
        <p role="alert" className="notice notice--danger mt-2">
          {err}
        </p>
      )}
      {cur && view && (
        <>
          <div className="mt-3 grid gap-3">
            <Prompt
              label="附加系统提示词"
              hint="排在酒馆全部系统内容之前"
              value={cur.priority_prompt}
              onChange={(v) => patch({ priority_prompt: v })}
            />
            <Prompt
              label="贴尾提示词"
              hint="每一轮钉在对话最后"
              value={cur.tail_prompt}
              onChange={(v) => patch({ tail_prompt: v })}
            />
          </div>
          <div className="mt-3 grid gap-3 sm:grid-cols-2">
            <Select
              label="调用模式"
              hint="cli = 驱动官方 claude；agent_sdk = 官方 Agent SDK"
              value={cur.invocation_mode}
              options={view.invocation_modes}
              onChange={(v) => patch({ invocation_mode: v })}
            />
            <Select
              label="模型"
              value={cur.model}
              options={view.models}
              onChange={(v) => patch({ model: v })}
            />
            <Select
              label="思考深度"
              value={cur.effort}
              options={view.efforts}
              onChange={(v) => patch({ effort: v })}
            />
            <Select
              label="提示缓存"
              hint="缓存有效期"
              value={cur.cache_ttl}
              options={view.cache_ttls}
              onChange={(v) => patch({ cache_ttl: v })}
            />
            <Select
              label="系统提示词模式"
              hint="append = 追加到 Claude 默认提示词（推荐）；replace = 替换"
              value={cur.system_prompt_mode}
              options={view.prompt_modes}
              onChange={(v) => patch({ system_prompt_mode: v })}
            />
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Button
              variant="primary"
              loading={busy}
              disabled={!dirty || busy}
              onClick={() => void save()}
            >
              保存
            </Button>
            <Button disabled={!dirty || busy} onClick={() => setDraft(null)}>
              放弃改动
            </Button>
            {/* 代价常驻，不放悬停提示。 */}
            <span className="notice">
              这七项任何一项改了，<strong>当前那条 SDK 会话会被退掉</strong>
              （下一轮重新开）。
            </span>
          </div>
        </>
      )}
    </Card>
  );
}

/** 调用日志（原来是 `bridge.py` 的 `/monitor` 页）。 */
export function ClaudeBridgeMonitor() {
  const toast = useToast();
  const [data, setData] = useState<BridgeTelemetry | null>(null);
  const [offset, setOffset] = useState(0);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const [asking, setAsking] = useState(false);

  const load = useCallback(async (at: number) => {
    setBusy(true);
    setErr("");
    try {
      setData(await api.tavernBridgeTelemetry(at, PAGE));
    } catch (e) {
      setData(null);
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, []);
  useEffect(() => {
    void load(offset);
  }, [load, offset]);

  const pct = (v: number | null) =>
    v == null ? "—" : `${Math.round(v * 100)}%`;

  return (
    <Card
      title={
        <>
          <Gauge size={14} />
          消息监控
        </>
      }
      actions={
        <div className="flex gap-1.5">
          <Button
            size="sm"
            icon={<RefreshCw size={12} />}
            loading={busy}
            aria-label="重新读取调用日志"
            onClick={() => void load(offset)}
          />
          <Button
            size="sm"
            variant="danger"
            icon={<Trash2 size={12} />}
            disabled={busy || !data?.total}
            onClick={() => setAsking(true)}
          >
            清空
          </Button>
        </div>
      }
    >
      {err && (
        <p role="alert" className="notice notice--danger">
          {err}
        </p>
      )}
      {data && (
        <>
          {/* ⛔ 「没给」显示成「—」，不是 0% —— 0% 是一句断言。 */}
          <div className="ustats mt-1">
            <div className="ustat">
              <span className="ustat-name">缓存命中率</span>
              <span className="ustat-value">{pct(data.hit_rate)}</span>
              <span className="ustat-sub">
                {NUM.format(data.countable)} 次可统计
              </span>
            </div>
            <div className="ustat">
              <span className="ustat-name">缓存读取</span>
              <span className="ustat-value">{short(data.cache_read)}</span>
              <span className="ustat-sub">token</span>
            </div>
            <div className="ustat">
              <span className="ustat-name">缓存写入</span>
              <span className="ustat-value">{short(data.cache_write)}</span>
              <span className="ustat-sub">token</span>
            </div>
            <div className="ustat">
              <span className="ustat-name">前缀可复用</span>
              <span className="ustat-value">{pct(data.prefix_reusable)}</span>
              <span className="ustat-sub">共 {NUM.format(data.total)} 条</span>
            </div>
          </div>
          <div className="qb-acct-tw mt-3">
            <table className="ag-table">
              <thead>
                <tr>
                  <th>时间</th>
                  <th>状态 · 模式</th>
                  <th>模型 · 深度</th>
                  <th>输入 · 输出</th>
                  <th>提示缓存</th>
                  <th>前缀</th>
                  <th>耗时</th>
                </tr>
              </thead>
              <tbody>
                {data.records.map((c) => (
                  <tr key={c.id || c.at}>
                    <td className="mono">{c.at || "—"}</td>
                    <td>
                      {c.status || "—"}
                      {c.mode ? ` · ${c.mode}` : ""}
                    </td>
                    <td>
                      {c.model || "—"}
                      {c.effort ? ` · ${c.effort}` : ""}
                    </td>
                    <td className="mono">
                      {short(c.input)} · {short(c.output)}
                    </td>
                    <td className="mono">
                      读 {short(c.cache_read)} · 写 {short(c.cache_write)}
                    </td>
                    <td>{PREFIX_TEXT[c.prefix] ?? c.prefix ?? "—"}</td>
                    <td className="mono">
                      {c.elapsed_ms
                        ? `${(c.elapsed_ms / 1000).toFixed(1)}s`
                        : "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {!data.records.length && (
            <p className="notice mt-2">
              还没有调用记录 —— 在酒馆里发一句话之后这里就有了。
            </p>
          )}
          <div className="account-pagination mt-2">
            <span className="notice">
              第 {data.offset + 1}–{data.offset + data.records.length} 条，共{" "}
              {NUM.format(data.total)} 条
            </span>
            <span className="pager ml-auto">
              <Button
                size="sm"
                disabled={busy || offset <= 0}
                onClick={() => setOffset(Math.max(0, offset - PAGE))}
              >
                上一页
              </Button>
              <Button
                size="sm"
                disabled={busy || !data.has_more}
                onClick={() => setOffset(offset + PAGE)}
              >
                下一页
              </Button>
            </span>
          </div>
        </>
      )}
      <ConfirmDialog
        open={asking}
        onCancel={() => setAsking(false)}
        onConfirm={async () => {
          try {
            const n = await api.tavernBridgeTelemetryClear();
            toast.ok(`已清空 ${n} 条调用日志`);
            setOffset(0);
            await load(0);
            setAsking(false);
          } catch (e) {
            toast.error(e instanceof Error ? e.message : String(e));
          }
        }}
        title="清空全部调用日志？"
        confirmLabel="清空"
        danger
      >
        <p>
          <strong>不可逆。</strong>
          这会删掉 <code>bridge.py</code> 自己那份 <code>call-log.jsonl</code>{" "}
          里的全部记录，缓存命中率与前缀统计也跟着从头算。对话本身不受影响。
        </p>
      </ConfirmDialog>
    </Card>
  );
}
