/**
 * Claude 桥接设置的「消息监控」一段：`bridge.py` 的调用日志（`/api/telemetry`），原来只在它自己的 `/monitor` 页上。
 *
 * ⛔ 字段照 2.4.0 录下来的真实回复读（坑 7.98）；读不到的数是「—」不是 0。
 * 「只看没成功的」只按录到过的那一个值判：状态不是 `ok`（空着的不算，那是桥没给）。
 * 清空不可逆，走确认框。
 */
import { Fragment, useCallback, useEffect, useState } from "react";
import { ChevronDown, ChevronRight, RefreshCw, Trash2 } from "lucide-react";

import { api } from "../../../../lib/api";
import type { BridgeTelemetry } from "../../../../lib/generated/BridgeTelemetry";
import { Button, Checkbox, ConfirmDialog, useToast } from "../../../../ui";
import { errorText } from "../../card/common";
import { PREFIX_TEXT, count, exact, pct, short } from "../format";
import CallDetail from "./CallDetail";

const PAGE = 20;

export default function Monitor({ id }: { id: string }) {
  const toast = useToast();
  const [data, setData] = useState<BridgeTelemetry | null>(null);
  const [offset, setOffset] = useState(0);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const [asking, setAsking] = useState(false);
  const [open, setOpen] = useState<string | null>(null);
  const [onlyBad, setOnlyBad] = useState(false);

  const load = useCallback(async (at: number) => {
    setBusy(true);
    setErr("");
    try {
      setData(await api.tavernBridgeTelemetry(at, PAGE));
    } catch (e) {
      setData(null);
      setErr(errorText(e));
    } finally {
      setBusy(false);
    }
  }, []);
  useEffect(() => {
    void load(offset);
  }, [load, offset]);

  const bad = (status: string) => !!status && status !== "ok";
  const rows = (data?.records ?? []).filter((c) => !onlyBad || bad(c.status));

  return (
    <section
      className="bridge-section"
      id={id}
      data-testid="claude-bridge-monitor"
    >
      <div className="flex flex-wrap items-center gap-2">
        <h3>消息监控</h3>
        <span className="ml-auto flex flex-wrap items-center gap-1.5">
          <Checkbox checked={onlyBad} onChange={setOnlyBad}>
            只看没成功的（这一页）
          </Checkbox>
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
        </span>
      </div>
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
                {exact(data.countable)} 次可统计
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
              <span className="ustat-sub">共 {count(data.total)} 条</span>
            </div>
          </div>
          <div className="qb-acct-tw mt-3">
            {/* 一格一行：窄了就在 `.qb-acct-tw` 里横向滚，不挤成竖条。 */}
            <table className="ag-table whitespace-nowrap">
              <thead>
                <tr>
                  <th aria-label="展开" />
                  <th>时间</th>
                  <th>状态 · 模式</th>
                  <th>模型 · 深度</th>
                  <th>输入 · 回复（字符）</th>
                  <th>输出 token（含思考）</th>
                  <th>提示缓存</th>
                  <th>前缀</th>
                  <th>耗时</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((c, i) => {
                  const key = `${c.id || c.at}#${i}`;
                  const expanded = open === key;
                  return (
                    <Fragment key={key}>
                      <tr
                        className={
                          bad(c.status) ? "bridge-call--bad" : undefined
                        }
                      >
                        <td>
                          <button
                            type="button"
                            className="bridge-call-toggle"
                            aria-expanded={expanded}
                            aria-label={
                              expanded ? "收起全文" : "展开输入 / 输出"
                            }
                            disabled={!c.id}
                            onClick={() => setOpen(expanded ? null : key)}
                          >
                            {expanded ? (
                              <ChevronDown size={13} />
                            ) : (
                              <ChevronRight size={13} />
                            )}
                          </button>
                        </td>
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
                          {short(c.input_chars)} · {exact(c.output_chars)}
                        </td>
                        <td className="mono">{short(c.output_tokens)}</td>
                        <td className="mono">
                          读 {short(c.cache_read)} · 写 {short(c.cache_write)}
                        </td>
                        <td>{PREFIX_TEXT[c.prefix] ?? (c.prefix || "—")}</td>
                        <td className="mono">
                          {c.elapsed_ms != null
                            ? `${(c.elapsed_ms / 1000).toFixed(1)}s`
                            : "—"}
                        </td>
                      </tr>
                      {expanded && (
                        <tr className="bridge-call-detail-row">
                          <td colSpan={9}>
                            <CallDetail id={c.id} />
                          </td>
                        </tr>
                      )}
                    </Fragment>
                  );
                })}
              </tbody>
            </table>
          </div>
          {!data.records.length && (
            <p className="notice mt-2">
              还没有调用记录 —— 在酒馆里发一句话之后这里就有了。
            </p>
          )}
          {data.records.length > 0 && onlyBad && rows.length === 0 && (
            <p className="notice mt-2">这一页的调用都成功了。</p>
          )}
          {data.records.length > 0 && (
            <p className="notice mt-2">
              回复字符是桥记下的正文长度，标点和换行都算在内；输出 token
              含思考，比回复本身多得多是正常的。点行首的箭头看这一条送进去和回来的全文。
            </p>
          )}
          <div className="account-pagination mt-2">
            <span className="notice">
              第 {data.records.length ? data.offset + 1 : 0}–
              {data.offset + data.records.length} 条，共 {count(data.total)} 条
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
            toast.error(errorText(e));
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
    </section>
  );
}
