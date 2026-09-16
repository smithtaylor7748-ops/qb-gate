/**
 * 一键关闭所有 Claude。
 *
 * v0.7.0 从 `AccountBand.tsx` 里抽出来，理由是位置变了：
 * 总览分成 Claude / GPT 两个页签之后，这个按钮**不能待在任一页签里** ——
 * 它按双重证据收进程，Claude 侧和 GPT 侧的都收。
 * 摆在 Claude 页签里会让人以为它只关 Claude，摆两份则会有人点两次。
 * 所以它通栏摆在页签面板下面，跟页签选了哪一边无关。
 *
 * 判定逻辑一个字没动，仍然全在 Rust 侧（`killswitch.rs`）：
 * 只收满足双重证据的进程，绝不按进程名杀，面板自己祖先链上的一律放过。
 */

import { useState } from "react";
import { Square } from "lucide-react";

import { api, type KillReport } from "../../lib/api";
import Tile from "./Tile";
import { AFTER } from "../../lib/resources";
import { invalidate } from "../../lib/store";
import { endTask, resetTask, useTask } from "../../lib/tasks";
import {
  Bullet,
  ConfirmDialog,
  Pill,
  Row,
  useToast,
  EVIDENCE_LABEL,
} from "../../ui";

/**
 * 0.20.0 加的第二种长相：**跟三个启动磁贴排在同一格网格里**。
 *
 * 使用者要「原来的这种按钮，四个，把一键关闭所有 claude 也放进去」——
 * 起和收本来就是同一件事的两头，四块一格 2×2 也正好把右栏填满。
 * 判定、确认框、收完重新上锁那一套一个字没动，换的只是这一个入口的外观。
 *
 * ⚠ 名字必须一直是「一键关闭所有 Claude」。它收的是**两边**的进程
 * （判据是双重证据，桌面端、Claude Code 会话、酒馆桥接都在内），
 * 不许因为这块贴现在长在 Claude 那一侧就改成「关闭本页」之类。
 */
export default function KillBar({
  variant = "bar",
}: {
  variant?: "bar" | "tile";
} = {}) {
  const toast = useToast();
  const previewTask = useTask("killswitch-preview");
  /**
   * ⛔ 执行那一步的任务态也要拿出来渲染。
   *
   * 0.20.0 之前只渲染 `previewTask`：扫描失败看得见，**真正的关闭失败
   * 只进一个会自己消失的 toast**。而 `Tile.tsx` 的文件头写着
   * 「失败则整贴变红并留住错误原文（toast 会自己消失，错误不能只靠它）」——
   * 一键关闭恰恰是最不能只靠 toast 的那一个：没关掉的进程还连着旧账户，
   * 而界面上看不出来。
   */
  const executeTask = useTask("killswitch-execute");

  const [busy, setBusy] = useState(false);
  const [kill, setKill] = useState<KillReport | null>(null);
  const [killing, setKilling] = useState(false);

  // 两步合成一个对外的「忙 / 出错」：扫描和执行对使用者是同一件事。
  const shown = executeTask.error ? executeTask : previewTask;
  const working = busy || killing;

  async function previewKill() {
    setBusy(true);
    resetTask("killswitch-preview");
    try {
      setKill(await api.killswitchPreview());
      endTask("killswitch-preview");
    } catch (e) {
      // 枚举失败要报出来，**不能显示成「0 个进程」** —— 档案 §7.17：
      // 一个说谎的空结果比一个报错难查得多。
      const msg = e instanceof Error ? e.message : String(e);
      endTask("killswitch-preview", msg);
      toast.error(msg);
    } finally {
      setBusy(false);
    }
  }

  async function doKill() {
    setKilling(true);
    resetTask("killswitch-execute");
    try {
      const r = await api.killswitchExecute();
      endTask("killswitch-execute");
      toast.ok(
        `收掉 ${r.killed.length} 个进程，重新上锁 ${r.relocked} 个可执行文件` +
          (r.failed.length ? `，${r.failed.length} 个没收掉` : ""),
      );
      setKill(null);
      invalidate(...AFTER.gate, "plugins");
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask("killswitch-execute", msg);
      toast.error(msg);
    } finally {
      setKilling(false);
    }
  }

  return (
    <>
      {variant === "tile" ? (
        <Tile
          icon={<Square size={18} />}
          name="一键关闭所有 Claude"
          note="只收双重证据的进程 · 连本会话一起收"
          task={killing ? { ...executeTask, running: true } : shown}
          disabled={working}
          onClick={previewKill}
        />
      ) : (
        <>
          <button
            type="button"
            className="killbar mt-1"
            disabled={working}
            onClick={previewKill}
          >
            <Square size={13} aria-hidden="true" />
            {busy
              ? "扫描进程中…"
              : killing
                ? "正在关闭…"
                : "一键关闭所有 Claude"}
          </button>
          <p className="notice notice--danger text-center">
            只收满足双重证据的进程，绝不按进程名杀。
            <strong>会连这个面板正在服务的 Claude Code 会话一起收掉。</strong>
            收完会重新上锁，<strong>但你原来的租约会还给你</strong>（出口 IP
            仍合格的话）。
            {shown.error && ` ${shown.error}`}
          </p>
        </>
      )}

      <ConfirmDialog
        open={kill !== null}
        onCancel={() => setKill(null)}
        onConfirm={doKill}
        title="一键关闭所有 Claude"
        confirmLabel={`确认关闭 ${kill?.targets.length ?? 0} 个进程`}
        loading={killing}
        danger
      >
        {kill?.targets.length ? (
          <>
            <p>下面这些进程会被收掉，收完自动重新上锁：</p>
            <div className="mt-2">
              {kill.targets.map((t) => (
                <Row
                  key={t.pid}
                  side={
                    <Pill tone="danger">
                      {EVIDENCE_LABEL[t.evidence] ?? t.evidence}
                    </Pill>
                  }
                >
                  <span className="font-mono">
                    PID {t.pid} · {t.name}
                  </span>
                  <span className="notice block w-full break-all font-mono">
                    {t.path ?? "路径未知"}
                  </span>
                </Row>
              ))}
            </div>
            <p className="notice notice--danger mt-3">
              其中很可能包含
              <strong>正在为你运行这个面板的 Claude Code 会话</strong>。
            </p>
          </>
        ) : (
          <p>没有满足双重证据的进程，什么都不会动。</p>
        )}

        {!!kill?.spared.length && (
          <div className="mt-3">
            <p className="notice">放过了 {kill.spared.length} 个：</p>
            {kill.spared.slice(0, 6).map((s) => (
              <Bullet key={s}>{s}</Bullet>
            ))}
            {kill.spared.length > 6 && (
              <Bullet>…另有 {kill.spared.length - 6} 个，理由同上</Bullet>
            )}
          </div>
        )}
      </ConfirmDialog>
    </>
  );
}
