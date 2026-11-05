/**
 * 软件卡版本栏底部的「迁移…」（2026-11-05，每个官方软件都能单独搬到别的目录）。
 *
 * 用法：`<SoftwareCard relocate={<RelocateControl app="claude-code" />} …>`
 *
 * Props：
 * - `app`（必填）：软件键 —— `claude-code` | `codex` | `sillytavern` | `gemini-cli` |
 *   `antigravity` | `antigravity-ide` | `claude-desktop` | `codex-desktop`。Chrome 不迁，不要挂。
 * - `label`（可选）：一张卡上挂两个时（反重力 Hub / IDE）区分用的前缀，比如 "Hub"。
 * - `onMoved`（可选）：迁完 / 迁回之后回调，卡片拿它刷新自己的检测。
 *
 * 显示「现在在哪」+「迁移…」按钮；点开弹窗选目录（`PathField`）、实时探测、说清会做什么、确认后执行，
 * 进度与结果走 toast。Claude 桌面端搬过之后多一个「迁回」；Codex 桌面端（MSIX）不搬，给
 * 「打开 Windows 的移动设置」+「重新核对位置」。没装、面板也没法收编的软件不渲染任何东西。
 */
import { useEffect, useRef, useState } from "react";
import { Button, ConfirmDialog, Modal, PathField, useToast } from "../../ui";
import { api, type RelocateApp, type RelocateWhere } from "../../lib/api";
import type { ManagedProbe } from "../../lib/generated/ManagedProbe";

export interface RelocateControlProps {
  app: RelocateApp;
  label?: string;
  onMoved?: () => void;
}

export function RelocateControl({ app, label, onMoved }: RelocateControlProps) {
  const toast = useToast();
  const [where, setWhere] = useState<RelocateWhere | null>(null);
  const [open, setOpen] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [busy, setBusy] = useState(false);

  const load = () =>
    api
      .relocateWhere(app)
      .then(setWhere)
      .catch(() => setWhere(null));
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [app]);

  if (!where) return null;
  // 没装：只有 Claude Code / Codex CLI 能「收编」（托管安装进新目录），其余不渲染。
  const canAdopt = app === "claude-code" || app === "codex";
  if (!where.installed && !canAdopt && !where.dangling) return null;

  const prefix = label ? `${label}：` : "";
  const shown =
    where.real && where.real !== where.path ? where.real : where.path;

  async function restore() {
    setBusy(true);
    toast.info("正在把 Claude 桌面端搬回原处…");
    try {
      const r = await api.relocateDesktopRestore();
      toast.ok(r.notes.join("；"));
      await load();
      onMoved?.();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
      setRestoring(false);
    }
  }

  async function openMoveSettings() {
    await toast.run(() =>
      api.codexDesktopMoveSettings().then((t) => toast.info(t)),
    );
  }

  async function recheck() {
    const r = await toast.run(() => api.codexDesktopLocation());
    if (r) {
      (r.watchdog_ok ? toast.ok : toast.info)(
        `${r.location ?? "没找到"} —— ${r.note}`,
      );
      await load();
    }
  }

  return (
    <div className="flex flex-wrap items-center gap-2 text-sm">
      <span className="notice min-w-0 break-all">
        {prefix}
        {where.installed ? "在 " : "将装到 "}
        <span className="mono">{shown ?? "（读不出）"}</span>
      </span>
      {app === "codex-desktop" ? (
        <>
          <Button size="sm" onClick={openMoveSettings}>
            打开 Windows 的移动设置
          </Button>
          <Button size="sm" variant="ghost" onClick={recheck}>
            重新核对位置
          </Button>
        </>
      ) : (
        <>
          {where.can_move && (
            <Button size="sm" onClick={() => setOpen(true)} disabled={busy}>
              迁移…
            </Button>
          )}
          {app === "claude-desktop" && (where.relocated || where.dangling) && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setRestoring(true)}
              loading={busy}
            >
              {where.dangling ? "收掉空联结点" : "迁回"}
            </Button>
          )}
        </>
      )}
      {open && (
        <RelocateDialog
          app={app}
          where={where}
          title={`迁移 ${prefix}${titleOf(app)}`}
          onClose={() => setOpen(false)}
          onDone={async () => {
            setOpen(false);
            await load();
            onMoved?.();
          }}
        />
      )}
      <ConfirmDialog
        open={restoring}
        onCancel={() => setRestoring(false)}
        onConfirm={restore}
        loading={busy}
        title="把 Claude 桌面端迁回原处"
        confirmLabel="迁回"
      >
        拆掉 %LOCALAPPDATA%\AnthropicClaude 这个联结点，把 {where.real}{" "}
        里的文件搬回去 （跨盘是复制核对后删源）。先把桌面端完全退出。
      </ConfirmDialog>
    </div>
  );
}

function titleOf(app: RelocateApp): string {
  const names: Record<RelocateApp, string> = {
    "claude-code": "Claude Code",
    codex: "Codex CLI",
    sillytavern: "SillyTavern",
    "gemini-cli": "Gemini CLI",
    antigravity: "反重力",
    "antigravity-ide": "反重力 IDE",
    "claude-desktop": "Claude 桌面端",
    "codex-desktop": "Codex 桌面端",
  };
  return names[app];
}

function RelocateDialog({
  app,
  where,
  title,
  onClose,
  onDone,
}: {
  app: RelocateApp;
  where: RelocateWhere;
  title: string;
  onClose: () => void;
  onDone: () => Promise<void>;
}) {
  const toast = useToast();
  const [path, setPath] = useState("");
  const [probe, setProbe] = useState<ManagedProbe | null>(null);
  const [probing, setProbing] = useState(false);
  const [running, setRunning] = useState(false);
  const seq = useRef(0);

  // 实时探测：停手 400ms 后探一次；晚回来的旧结果不覆盖新的。
  useEffect(() => {
    const p = path.trim();
    setProbe(null);
    if (!p) return;
    const mine = ++seq.current;
    const t = setTimeout(() => {
      setProbing(true);
      api
        .relocateProbe(app, p)
        .then((r) => mine === seq.current && setProbe(r))
        .catch(
          (e) =>
            mine === seq.current &&
            setProbe({
              ok: false,
              path: p,
              filesystem: null,
              reason: String(e),
            }),
        )
        .finally(() => mine === seq.current && setProbing(false));
    }, 400);
    return () => clearTimeout(t);
  }, [app, path]);

  async function run() {
    setRunning(true);
    toast.info(`${title}：开始，完成前别关面板…`);
    try {
      const r = await api.relocateRun(app, path.trim());
      toast.ok(`已迁到 ${r.to}。${r.notes.filter(Boolean).join("；")}`);
      await onDone();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setRunning(false);
    }
  }

  const target =
    app === "claude-desktop" && path.trim()
      ? `${path.trim().replace(/[\\/]+$/, "")}\\AnthropicClaude`
      : path.trim();

  return (
    <Modal
      open
      onClose={onClose}
      dismissible={!running}
      title={title}
      footer={
        <>
          <Button onClick={onClose} disabled={running}>
            取消
          </Button>
          <Button
            variant="primary"
            onClick={run}
            loading={running}
            disabled={!probe?.ok || probing}
          >
            {where.installed ? "确认迁移" : "确认安装到这里"}
          </Button>
        </>
      }
    >
      <p className="notice mb-2 break-all">
        现在：
        <span className="mono">{where.real ?? where.path ?? "（读不出）"}</span>
      </p>
      <PathField
        label={app === "claude-desktop" ? "搬到哪个目录下" : "新目录"}
        value={path}
        onChange={setPath}
        disabled={running}
        hint="只能是本机固定盘（NTFS）上的空目录；网络盘、U 盘会被拒绝。"
      />
      <p className="mt-2 text-sm" role="status">
        {!path.trim()
          ? "先选一个目录。"
          : probing
            ? "正在实测这个目录…"
            : probe?.ok
              ? `可以用（${probe.filesystem ?? "文件系统未知"}）。会装到：${target}`
              : (probe?.reason ?? "")}
      </p>
      <p className="notice mt-2">会做什么：{where.note}</p>
    </Modal>
  );
}

export default RelocateControl;
