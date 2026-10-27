/**
 * 环境变量清理（2026-10-27，中转站重构阶段 7 · X5，照 cc-switch 的环境变量冲突检查）。
 *
 * 本机体检里「环境变量残留」那一项只报不删；这里是使用者点了之后才删的那一半。
 * 只删当前用户（HKCU）那一处，删之前存一份备份（值在本机加密存），之后能按备份还原；
 * 整机的（要管理员）和只在面板进程里的只列出来、说清楚去哪改。
 *
 * **什么都不预先勾** —— 这些变量可能正是使用者别的工作要用的（体检那一项原来的那句话照旧成立）。
 */
import { useState } from "react";
import { Eraser, RotateCcw, Trash2 } from "lucide-react";
import {
  api,
  type EnvEntry,
  type EnvRemoval,
  type EnvRestoreReport,
  type EnvScope,
} from "../../lib/api";
import { AFTER, R } from "../../lib/resources";
import { useResource } from "../../lib/store";
import { useAction } from "../../lib/workspace";
import {
  Button,
  Checkbox,
  ConfirmDialog,
  EmptyState,
  Modal,
  Pill,
  Row,
} from "../../ui";

const SCOPE_LABEL: Record<EnvScope, string> = {
  user: "当前用户",
  machine: "整机",
  process: "只在面板进程里",
};

/** 体检卡与修复中心都放这一颗按钮，点开同一个弹窗。 */
export default function EnvCleanup() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button
        size="sm"
        icon={<Eraser size={12} />}
        onClick={() => setOpen(true)}
      >
        清理环境变量…
      </Button>
      {open && <EnvCleanupDialog onClose={() => setOpen(false)} />}
    </>
  );
}

function removalLines(r: EnvRemoval): string[] {
  const out: string[] = [];
  if (r.removed.length)
    out.push(
      `已删除 ${r.removed.join("、")}（备份 ${r.backup_id ?? "—"}）。新开的终端和程序读不到它们了；已经开着的（包括这个面板）要重开才干净。`,
    );
  if (r.missing.length)
    out.push(`当前用户这一处本来就没有：${r.missing.join("、")}。`);
  for (const f of r.failed) out.push(`没删掉 ${f}`);
  return out;
}

function restoreLines(r: EnvRestoreReport): string[] {
  const out: string[] = [];
  if (r.restored.length) out.push(`已写回 ${r.restored.join("、")}。`);
  if (r.already.length) out.push(`本来就是这个值：${r.already.join("、")}。`);
  if (r.conflicts.length)
    out.push(
      `现在有一个不一样的值，没覆盖：${r.conflicts.join("、")}。要换回来先把现在的删掉再还原。`,
    );
  for (const f of r.failed) out.push(`没写回 ${f}`);
  return out;
}

function EnvCleanupDialog({ onClose }: { onClose: () => void }) {
  const vars = useResource("envVars", R.envVars);
  const backups = useResource("envBackups", R.envBackups);
  const action = useAction();
  const [picked, setPicked] = useState<string[]>([]);
  const [confirm, setConfirm] = useState(false);
  const [dropping, setDropping] = useState<string | null>(null);
  const [lines, setLines] = useState<string[]>([]);
  const busy = !!action.pending;

  const list = vars.data ?? [];
  const removable = list.filter((e) => e.removable);
  const elsewhere = list.filter((e) => !e.removable);

  const toggle = (name: string, on: boolean) =>
    setPicked((p) => (on ? [...p, name] : p.filter((n) => n !== name)));

  async function remove() {
    const r = await action.run(
      "remove",
      () => api.envVarsRemove(picked),
      undefined,
      AFTER.env,
    );
    setConfirm(false);
    if (r) {
      setLines(removalLines(r));
      setPicked([]);
    }
  }

  async function restore(id: string) {
    const r = await action.run(
      `restore:${id}`,
      () => api.envBackupRestore(id),
      undefined,
      AFTER.env,
    );
    if (r) setLines(restoreLines(r));
  }

  async function drop(id: string) {
    const ok = await action.run(
      `drop:${id}`,
      () => api.envBackupDelete(id).then(() => true),
      "备份已删除",
    );
    if (ok) setDropping(null);
  }

  const row = (e: EnvEntry) => (
    <Row
      key={`${e.scope}-${e.name}`}
      side={
        <>
          <Pill>{ownerLabel(e.owner)}</Pill>
          <Pill tone={e.removable ? "default" : "warn"}>
            {SCOPE_LABEL[e.scope]}
          </Pill>
        </>
      }
    >
      {e.removable ? (
        <Checkbox
          checked={picked.includes(e.name)}
          disabled={busy}
          onChange={(on) => toggle(e.name, on)}
        >
          <span className="font-mono">{e.name}</span>
        </Checkbox>
      ) : (
        <span className="font-mono">{e.name}</span>
      )}
      <span className="notice block break-all font-mono">{e.shown}</span>
    </Row>
  );

  return (
    <Modal
      open
      onClose={() => !busy && onClose()}
      dismissible={!busy}
      size="wide"
      title={
        <>
          <Eraser size={18} />
          清理环境变量
        </>
      }
      footer={
        <>
          <Button disabled={busy} onClick={() => void vars.refresh()}>
            重新读取
          </Button>
          <Button
            variant="danger"
            disabled={busy || picked.length === 0}
            onClick={() => setConfirm(true)}
          >
            先备份，再删除勾选的 {picked.length} 个
          </Button>
        </>
      }
    >
      <p className="notice mb-3">
        列的是会改变 Claude Code、Codex、Gemini CLI、Grok Build
        去向的变量，和四个代理变量。值是掩码过的：Key
        类只说「已设置」，地址类只留主机。面板只删当前用户这一处，删之前存一份备份（值在本机加密存），之后能在下面还原。
      </p>
      {vars.error && <p className="notice notice--danger">{vars.error}</p>}
      {vars.data && list.length === 0 && (
        <EmptyState title="没有相关的环境变量">
          当前用户、整机和面板进程里都没有这几类变量。
        </EmptyState>
      )}
      {removable.map(row)}
      {elsewhere.length > 0 && (
        <div className="mt-3">
          <p className="notice">
            下面这些面板不删：整机的要管理员（开始菜单搜「编辑系统环境变量」→
            环境变量 →
            下半部分「系统变量」里改）；只在面板进程里的，是起面板的那个程序传进来的，重开面板就没了。
          </p>
          {elsewhere.map(row)}
        </div>
      )}
      {lines.length > 0 && (
        <div role="status" className="mt-3">
          {lines.map((l) => (
            <p key={l} className="notice mt-1 break-words">
              {l}
            </p>
          ))}
        </div>
      )}

      <h3 className="mt-4">备份</h3>
      {backups.error && (
        <p className="notice notice--danger">{backups.error}</p>
      )}
      {backups.data && backups.data.length === 0 && (
        <p className="notice">还没有备份 —— 删过一次之后，这里会多一份。</p>
      )}
      {(backups.data ?? []).map((b) => (
        <Row
          key={b.id}
          side={
            <>
              <Button
                size="sm"
                icon={<RotateCcw size={12} />}
                loading={action.pending === `restore:${b.id}`}
                disabled={busy}
                onClick={() => void restore(b.id)}
              >
                还原
              </Button>
              <Button
                size="sm"
                variant="ghost"
                aria-label="删除这份备份"
                icon={<Trash2 size={12} />}
                disabled={busy}
                onClick={() => setDropping(b.id)}
              />
            </>
          }
        >
          <span>{new Date(b.created).toLocaleString("zh-CN")}</span>
          <span className="notice block break-all font-mono">
            {b.names.join("、")}
          </span>
          {b.restored_at && (
            <span className="notice block">
              {new Date(b.restored_at).toLocaleString("zh-CN")} 还原过
            </span>
          )}
        </Row>
      ))}

      <ConfirmDialog
        open={confirm}
        danger
        loading={action.pending === "remove"}
        title={`删除 ${picked.length} 个环境变量？`}
        confirmLabel="备份并删除"
        onCancel={() => setConfirm(false)}
        onConfirm={() => void remove()}
      >
        <p>
          会删掉当前用户的 {picked.join("、")}
          。删之前先存一份备份，删完能在「备份」里还原。
        </p>
        <p className="notice mt-2">
          已经开着的终端和程序还带着旧值，新开的才读不到；面板自己也要重开才干净。
        </p>
      </ConfirmDialog>
      <ConfirmDialog
        open={!!dropping}
        danger
        loading={!!dropping && action.pending === `drop:${dropping}`}
        title="删除这份备份？"
        confirmLabel="删除"
        onCancel={() => setDropping(null)}
        onConfirm={() => dropping && void drop(dropping)}
      >
        <p>删掉之后，这一次删掉的变量就不能从面板里还原了。</p>
      </ConfirmDialog>
    </Modal>
  );
}

function ownerLabel(owner: EnvEntry["owner"]): string {
  switch (owner) {
    case "claude-code":
      return "Claude Code";
    case "codex":
      return "Codex";
    case "gemini-cli":
      return "Gemini CLI";
    case "grok-build":
      return "Grok Build";
    case "proxy":
      return "代理";
  }
}
