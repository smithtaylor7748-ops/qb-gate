/**
 * 清理 Claude 汉化残留（2026-09-27，使用者定的：Claude 汉化撤掉，只留这一次性的清理）。
 *
 * 为什么撤：官方账户下 Claude 的窗口是在线的 claude.ai，它没有中文，每次打开都会把桌面端的
 * 界面语言改回英文 —— 09-25 那两次「核验通过」、界面却还是英文，就是这个原因。能把语言锁住的
 * 办法都要改 Claude 的程序文件、绕过它的防篡改，面板不做。
 *
 * 只在账户页检测到残留时出现（`ClaudeZhLeftovers.empty` 为假），清完就不再出现。
 * 代价常驻在按钮旁边、不放悬停（CLAUDE.md 的老规矩）：要往 Claude 现在用的那一版里写时，
 * 会先关掉 Claude 桌面端（包括它 Code 页里的会话）；不用写就不关。
 */
import { useCallback, useEffect, useState } from "react";
import { Eraser } from "lucide-react";

import { api } from "../../lib/api";
import type { ClaudeZhLeftovers } from "../../lib/generated/ClaudeZhLeftovers";
import type { ClaudeZhOutcome } from "../../lib/generated/ClaudeZhOutcome";
import { invalidate } from "../../lib/store";
import { endTask, resetTask, useTask } from "../../lib/tasks";
import {
  Button,
  ConfirmDialog,
  LogView,
  ProgressBar,
  useToast,
} from "../../ui";

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** 还剩什么、清的时候对每一样做什么 —— 弹窗与确认框用同一份说法。 */
export function claudeZhItems(l: ClaudeZhLeftovers): string[] {
  const out: string[] = [];
  for (const a of l.apps) {
    const where = `Claude ${a.version}${a.current ? "（现在用的这一版）" : "（旧版目录）"}`;
    if (a.backup_mb === null) {
      out.push(
        `${where}：还有 ${a.zh_files} 个上游放的中文文件，但上游的备份不在了，面板换不回原版 —— 到 Claude 官网重新下载安装包覆盖安装。`,
      );
      continue;
    }
    if (!a.restorable) {
      out.push(
        `${where}：上游的备份里有程序文件（app.asar / claude.exe 之类），面板不碰 —— 到 Claude 官网重新下载安装包覆盖安装。`,
      );
      continue;
    }
    const parts: string[] = [];
    if (a.changed > 0)
      parts.push(
        `${a.changed} 个前端文件跟原版不一样（上游改过的，也包括它卸载时删多了的），换回备份里的原版`,
      );
    if (a.zh_files > 0) parts.push(`删掉 ${a.zh_files} 个上游放的中文文件`);
    parts.push(`删掉上游留下的备份（${a.backup_mb} MB）`);
    out.push(`${where}：${parts.join("；")}。`);
  }
  for (const p of l.zh_profiles)
    out.push(
      `资料 ${p.dir.split(/[\\/]/).pop()}：界面语言还是 ${p.locale ?? "中文"}，写回汉化之前的值（没记下就写 en-US）。`,
    );
  if (l.stray_3p)
    out.push(
      `删掉上游建的 ${l.stray_3p}（里面只有一个写了界面语言的 config.json）。`,
    );
  if (l.panel_copy) out.push(`删掉面板下载过的上游副本：${l.panel_copy}。`);
  if (l.state_file) out.push(`删掉那版汉化插件的记录：${l.state_file}。`);
  return out;
}

export default function ClaudeZhCleanup({ onDone }: { onDone?: () => void }) {
  const toast = useToast();
  const task = useTask("claude-zh");
  const [left, setLeft] = useState<ClaudeZhLeftovers | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [ask, setAsk] = useState(false);
  const [outcome, setOutcome] = useState<ClaudeZhOutcome | null>(null);

  const load = useCallback(async () => {
    try {
      setLeft(await api.claudeZhLeftovers());
      setError("");
    } catch (e) {
      setError(errorText(e));
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load]);

  async function run() {
    setAsk(false);
    setBusy(true);
    setOutcome(null);
    resetTask("claude-zh");
    try {
      const o = await api.claudeZhCleanup();
      setOutcome(o);
      endTask("claude-zh", o.ok ? undefined : "没全清掉，见下面逐条");
      if (o.ok) toast.ok("Claude 汉化残留清掉了");
      else toast.error("没全清掉，见下面逐条");
      onDone?.();
    } catch (e) {
      const msg = errorText(e);
      endTask("claude-zh", msg);
      toast.error(msg);
    } finally {
      setBusy(false);
      invalidate("claudeZhLeftovers");
      await load();
    }
  }

  const items = left ? claudeZhItems(left) : [];
  const running = left?.desktop_running ?? null;
  const closeNote = left?.needs_close ? (
    <>
      要往 Claude 现在用的这一版里换回原版文件，会先
      <strong>关掉 Claude 桌面端</strong>
      ——包括它 Code 页里正在跑的会话，先保存手头的工作。
      {running !== null && running > 0
        ? ` 它现在开着（${running} 个进程）。`
        : ""}
      {running === null ? " 查不到它现在开没开。" : ""}
    </>
  ) : (
    "不用关 Claude。"
  );

  return (
    <div className="flex flex-col gap-3" data-testid="claude-zh-cleanup">
      <p className="notice">
        Claude 汉化已经撤掉：官方账户下 Claude 的窗口是在线的
        claude.ai，它没有中文， 每次打开都会把桌面端的界面语言改回英文——09-25
        那次「核验通过」、界面却还是英文，就是这个原因。能把语言锁住的办法都要改
        Claude 的程序文件、绕过它的防篡改，面板不做。
      </p>
      {error && (
        <p role="alert" className="notice notice--danger">
          {error}
        </p>
      )}
      {!left && !error && <p className="notice">读取中…</p>}
      {left && left.empty && <p className="notice">没有残留了，不用清。</p>}
      {left && !left.empty && (
        <>
          <div className="text-sm">
            还剩这些，清的时候逐样处理：
            <ul
              className="mt-1 list-disc pl-5"
              style={{ overflowWrap: "anywhere" }}
            >
              {items.map((t) => (
                <li key={t}>{t}</li>
              ))}
            </ul>
          </div>
          <p className="notice">
            {closeNote}
            不联网、不跑上游的脚本：换回的原版取自上游自己留下的备份，只写备份里列着的前端文件，不碰
            app.asar 与 claude.exe。
          </p>
          <div className="flex flex-wrap gap-2">
            <Button
              variant="danger"
              icon={<Eraser size={14} />}
              disabled={busy}
              loading={busy}
              data-testid="claude-zh-cleanup-run"
              onClick={() => setAsk(true)}
            >
              清理
            </Button>
          </div>
        </>
      )}

      {(task.running || task.error) && (
        <div>
          <div className="mb-1.5 flex items-center gap-2">
            <span className="text-sm">{task.phase || "准备中…"}</span>
            {task.total > 0 && (
              <span className="notice ml-auto">
                {task.step} / {task.total}
              </span>
            )}
          </div>
          <ProgressBar
            value={task.total > 0 ? (task.step / task.total) * 100 : undefined}
            tone={task.error ? "danger" : "accent"}
            label="清理 Claude 汉化残留的进度"
          />
          {task.error && (
            <p className="notice notice--danger mt-1.5">{task.error}</p>
          )}
        </div>
      )}
      {outcome && (
        <div data-testid="claude-zh-cleanup-outcome">
          <p
            className={`notice ${outcome.ok ? "" : "notice--danger"}`}
            role={outcome.ok ? undefined : "alert"}
          >
            {outcome.ok
              ? `清完了。${outcome.closed_claude ? "Claude 桌面端是清理时关掉的，从「Claude 桌面端」那块磁贴再打开。" : ""}`
              : "没全清掉："}
          </p>
          <LogView lines={outcome.lines} />
        </div>
      )}

      <ConfirmDialog
        open={ask}
        onCancel={() => setAsk(false)}
        onConfirm={() => void run()}
        title="清理 Claude 汉化残留？"
        confirmLabel={left?.needs_close ? "关掉 Claude 并清理" : "清理"}
        loading={busy}
        danger
      >
        <p>{closeNote}</p>
        <p className="mt-2">
          然后按上面那张单子逐样处理；哪一样对不上就停在那一样，上游的备份留着。
        </p>
      </ConfirmDialog>
    </div>
  );
}
