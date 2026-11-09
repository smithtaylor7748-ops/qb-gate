/**
 * 一键关闭所有 Claude。
 *
 * Claude 页的操作按客户端收窄；全局停用由 IP 锁入口处理。
 *
 * 判定逻辑一个字没动，仍然全在 Rust 侧（`killswitch.rs`）：
 * 只收满足双重证据的进程，绝不按进程名杀，面板自己祖先链上的一律放过。
 */

import { useState } from "react";
import { Square } from "lucide-react";

import { api, type KillReport } from "../../lib/api";
import { describeTargets } from "../../pages/accounts/AccountDialogs";
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
 * 桌面端、Claude Code（含中转）与 Claude 桥接都在内。
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
    // 新的一轮开始了，上一轮执行的失败原文不再代表现状。原来它会一直挂在贴上，
    // 挡住后面每一次扫描的进度与结果，直到再执行一次（2026-09-25）。
    resetTask("killswitch-execute");
    try {
      setKill(await api.killswitchPreview(true));
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
      const r = await api.killswitchExecute(true);
      endTask("killswitch-execute");
      toast.ok(
        `关闭 ${r.killed.length} 个 Claude 进程` +
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
          note="Claude 桌面端、CLI 与桥接 · 含 Claude 中转会话"
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
            收回已关闭 Claude 会话的租约。
            {shown.error && ` ${shown.error}`}
          </p>
        </>
      )}

      <ConfirmDialog
        open={kill !== null}
        onCancel={() => setKill(null)}
        onConfirm={doKill}
        title="一键关闭所有 Claude"
        confirmLabel={
          kill?.targets.length
            ? `确认关闭 ${kill.targets.length} 个进程`
            : "停止 Claude 托管会话与桥接"
        }
        loading={killing}
        danger
      >
        {kill?.targets.length ? (
          <>
            <p>
              下面这些进程会被收掉（{describeTargets(kill)}
              ）：
            </p>
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
          <p>没有扫到要按证据收的进程。</p>
        )}

        <p className="notice notice--danger mt-3">
          会停止面板起的 Claude 桌面端、Claude Code（包括 Claude 中转会话）和
          Claude
          酒馆桥接，并收回这些会话的租约。GPT、Gemini、反重力以及共用的酒馆页面继续运行。
        </p>

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
