/**
 * 总览第三段：运行日志。
 *
 * 数据不需要新命令：`gate` 这个资源本来就 15 秒轮询一次（`resources.ts`），
 * `GateStatus.recent_log` 带着 `log::tail(20)`（`lib.rs`）。
 * 所以总览上这块是**自动实时刷新**的。
 *
 * 跟 IP 锁页那个 `LogView` 的区别：那边是原始文本框（复制、排查用），
 * 这边按 `logline.ts` 解析成事件行 —— 时间、图标、正文、行内快捷动作。
 * 两边并存是故意的，排查的时候还是要能看见一模一样的原文。
 */

import { useState } from "react";
import { Activity, Check, Copy, Lock, ShieldAlert, Users } from "lucide-react";

import { api } from "../../lib/api";
import { AFTER, R } from "../../lib/resources";
import { invalidate, useResource, useSession } from "../../lib/store";
import {
  LOG_FILTER_LABEL,
  matchesFilter,
  parseLog,
  type LogEntry,
  type LogFilter,
} from "../../lib/logline";
import { Button, Card, ConfirmDialog, useToast } from "../../ui";

const FILTERS: LogFilter[] = ["all", "gate", "account", "process", "error"];

function iconOf(e: LogEntry) {
  if (e.tone === "danger") return <ShieldAlert size={13} aria-hidden="true" />;
  if (e.category === "account") return <Users size={13} aria-hidden="true" />;
  if (e.category === "gate") return <Lock size={13} aria-hidden="true" />;
  return <Activity size={13} aria-hidden="true" />;
}

export default function LogBand() {
  const toast = useToast();
  const gate = useResource("gate", R.gate);

  const [filter, setFilter] = useSession<LogFilter>("home.log.filter", "all");
  const [copied, setCopied] = useState(false);
  /** 待加进白名单的 IP。确认之后才写 —— 这是给门禁开一个新口子。 */
  const [addIp, setAddIp] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);

  const lines = gate.data?.recent_log ?? [];
  const entries = parseLog(lines);
  const shown = entries.filter((e) => matchesFilter(e, filter));

  async function copy() {
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      /* 剪贴板不给用就算了 */
    }
  }

  async function doAdd(ip: string) {
    setAdding(true);
    try {
      const current = gate.data?.allowlist ?? [];
      // 幂等：重复点不会写出两条一样的。
      if (!current.includes(ip)) await api.allowlistWrite([...current, ip]);
      toast.ok(`${ip} 已加入白名单。下次启动才会用上它 —— 现在的锁没有变化。`);
      invalidate(...AFTER.gate);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setAdding(false);
      setAddIp(null);
    }
  }

  return (
    <>
      <Card
        title="运行日志"
        icon={<Activity size={14} aria-hidden="true" />}
        actions={
          <>
            <span className="pager">
              {FILTERS.map((f) => (
                <button
                  key={f}
                  type="button"
                  className="pager-btn"
                  aria-current={f === filter}
                  onClick={() => setFilter(f)}
                >
                  {LOG_FILTER_LABEL[f]}
                </button>
              ))}
            </span>
            <Button
              size="sm"
              variant="ghost"
              onClick={copy}
              disabled={lines.length === 0}
              icon={
                copied ? (
                  <Check size={12} aria-hidden="true" />
                ) : (
                  <Copy size={12} aria-hidden="true" />
                )
              }
            >
              {copied ? "已复制" : "复制原文"}
            </Button>
          </>
        }
      >
        {shown.length === 0 ? (
          <p className="notice">
            {lines.length === 0
              ? "还没有记录。上锁、放行、切换账户这些动作都会写进来。"
              : `最近 ${lines.length} 行里没有「${LOG_FILTER_LABEL[filter]}」类的记录。`}
          </p>
        ) : (
          <div className="timeline" role="log" aria-live="polite">
            {shown.map((e, i) => (
              <div
                key={i}
                className={`tl-row${e.tone !== "default" ? ` tl-row--${e.tone}` : ""}`}
              >
                {e.stamp && <span className="tl-stamp">{e.stamp}</span>}
                <span className="tl-icon">{iconOf(e)}</span>
                <span className="tl-text">{e.text}</span>
                {e.action?.kind === "allowlist" && (
                  <span className="tl-side">
                    <Button size="sm" onClick={() => setAddIp(e.action!.ip)}>
                      加入白名单
                    </Button>
                  </span>
                )}
              </div>
            ))}
          </div>
        )}

        <p className="notice mt-2">
          每 15 秒自动刷新，最近 20 行。只记时间、公网 IP 与上锁解锁动作，
          <strong>不含任何对话内容</strong>。
        </p>
      </Card>

      <ConfirmDialog
        open={!!addIp}
        onCancel={() => setAddIp(null)}
        onConfirm={() => addIp && void doAdd(addIp)}
        title={`把 ${addIp ?? ""} 加进白名单？`}
        confirmLabel="确认加入"
        loading={adding}
      >
        <p>
          白名单决定门禁放行哪些出口 IP。加进去之后，从这条 IP
          出网就能拿到租约、 解开 <code>claude.exe</code> 上的执行锁。
        </p>
        <p className="notice mt-2">
          <strong>先确认这条 IP 是你自己的、并且已经复核过纯净度。</strong>
          日志里出现它只说明有过一次被拒的启动尝试，不代表它合格。
        </p>
        <p className="notice mt-1">
          加完不会自动放行 —— 当前的锁不变，要到启动时才用上。
        </p>
      </ConfirmDialog>
    </>
  );
}
