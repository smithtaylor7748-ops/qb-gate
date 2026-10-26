/**
 * 项目（中转站重构阶段 5，cc-switch 的 M4 Profile）：把中转那一侧的一整套设置存成一个名字，一键换过去。
 *
 * 照 cc-switch 的项目切换器：头部一个按钮写着当前项目，点开是项目列表 —— 换到哪个、按现在的样子新建、
 * 不使用项目、改名、删除。cc-switch 是弹出菜单 + 管理对话框两层；QB Gate 没有弹出菜单组件，并成一个弹窗。
 *
 * 项目存的是**这个软件**的：选的线路、派单顺序、中转环境里勾着的 MCP / Skills、用的提示词（桌面端只有前两样）。
 * ⛔ 官方账户和槽位一概不动。换项目时现在的样子先存回原来那个项目（cc-switch 同样），所以不另给「重拍」。
 */

import { useCallback, useEffect, useState } from "react";
import { Check, FolderOpen, Pencil, Plus, Trash2, X } from "lucide-react";

import type { Client } from "../../lib/generated/Client";
import type { Project } from "../../lib/generated/Project";
import {
  stationApi,
  type ProjectChange,
  type ProjectsView,
} from "../../lib/station";
import { Button, ConfirmDialog, Modal, useToast } from "../../ui";

export default function StationProjects({
  client,
  clientName,
  onApplied,
}: {
  client: Client;
  clientName: string;
  /** 换了项目（线路、派单顺序可能变了），中转站页要重读状态。 */
  onApplied?: () => void;
}) {
  const toast = useToast();
  const [view, setView] = useState<ProjectsView | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** 上一次换项目没落成的那几样（尽力而为，单项失败不整体回滚）。 */
  const [warnings, setWarnings] = useState<string[]>([]);
  const [newName, setNewName] = useState("");
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(
    null,
  );
  const [deleting, setDeleting] = useState<Project | null>(null);

  const load = useCallback(async () => {
    try {
      setView(await stationApi.projects());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const currentId = view?.current[client] ?? null;
  const current = view?.projects.find((p) => p.id === currentId) ?? null;
  const desktop = client === "claude-desktop";

  /** 跑一个改动；回来的是列表就落列表，是换项目的结果就连提醒一起落。 */
  const run = async (
    fn: () => Promise<ProjectsView | ProjectChange>,
    done?: string,
  ): Promise<boolean> => {
    setBusy(true);
    setError(null);
    try {
      const r = await fn();
      if ("view" in r) {
        setView(r.view);
        setWarnings(r.warnings);
        onApplied?.();
        if (r.warnings.length)
          toast.info(`${done ?? "完成"}，有 ${r.warnings.length} 样没落成`);
        else if (done) toast.ok(done);
      } else {
        setView(r);
        if (done) toast.ok(done);
      }
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const create = async () => {
    const name = newName.trim();
    if (!name) return;
    if (
      await run(
        () => stationApi.projectCreate(name, client),
        `存成项目「${name}」了`,
      )
    )
      setNewName("");
  };

  const rename = async () => {
    if (!renaming || !renaming.name.trim()) return;
    if (await run(() => stationApi.projectRename(renaming.id, renaming.name)))
      setRenaming(null);
  };

  return (
    <>
      <Button
        icon={<FolderOpen size={14} aria-hidden="true" />}
        onClick={() => {
          setOpen(true);
          setWarnings([]);
          void load();
        }}
        title={`项目：一键换 ${clientName} 中转那一侧的线路、派单顺序${desktop ? "" : "、MCP / Skills / 提示词"}`}
        aria-haspopup="dialog"
      >
        {current ? `项目：${current.name}` : "不使用项目"}
      </Button>

      <Modal
        open={open}
        onClose={() => {
          if (busy) return;
          setOpen(false);
          setRenaming(null);
        }}
        title={`项目 · ${clientName}`}
        icon={<FolderOpen size={16} aria-hidden="true" />}
        footer={
          <>
            {current && (
              <Button
                icon={<X size={14} aria-hidden="true" />}
                disabled={busy}
                onClick={() =>
                  void run(
                    () => stationApi.projectClear(client),
                    "不使用项目了（刚才的样子存回了原来那个项目）",
                  )
                }
              >
                不使用项目
              </Button>
            )}
            <Button disabled={busy} onClick={() => setOpen(false)}>
              关闭
            </Button>
          </>
        }
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
          <p className="qb-st-note">
            项目是中转那一侧的一套设置：{clientName}选的线路、派单顺序
            {desktop
              ? "（桌面端没有 MCP / Skills / 提示词那几样）"
              : "，中转环境里勾着的 MCP / Skills 和用的提示词"}
            。换项目时，现在的样子先存回原来那个项目；
            <b>官方账户和槽位一概不动</b>
            。项目列表各软件共用，当前项目每个软件各记各的。
          </p>

          {warnings.length > 0 && (
            <div className="qb-st-note" role="alert">
              换过去了，下面几样没落成：
              <ul style={{ margin: "4px 0 0", paddingLeft: 18 }}>
                {warnings.map((w) => (
                  <li key={w}>{w}</li>
                ))}
              </ul>
            </div>
          )}

          {view && view.projects.length === 0 && (
            <p className="qb-st-dim">
              还没有项目 —— 在下面按现在的样子存一个。
            </p>
          )}

          {view && view.projects.length > 0 && (
            <div className="qb-st-tw">
              <table>
                <tbody>
                  {view.projects.map((p) => {
                    const isCurrent = p.id === currentId;
                    const saved = p.slots[client] != null;
                    return (
                      <tr key={p.id}>
                        <td>
                          {renaming?.id === p.id ? (
                            <span
                              style={{
                                display: "inline-flex",
                                gap: 6,
                                alignItems: "center",
                              }}
                            >
                              <input
                                className="input"
                                aria-label="新名字"
                                value={renaming.name}
                                autoFocus
                                onChange={(e) =>
                                  setRenaming({
                                    id: p.id,
                                    name: e.target.value,
                                  })
                                }
                                onKeyDown={(e) => {
                                  if (e.key === "Enter") void rename();
                                  if (e.key === "Escape") setRenaming(null);
                                }}
                              />
                              <Button
                                size="sm"
                                icon={<Check size={13} aria-hidden="true" />}
                                disabled={busy || !renaming.name.trim()}
                                onClick={() => void rename()}
                              >
                                改好了
                              </Button>
                              <Button
                                size="sm"
                                variant="ghost"
                                disabled={busy}
                                onClick={() => setRenaming(null)}
                              >
                                算了
                              </Button>
                            </span>
                          ) : (
                            <>
                              <span>{p.name}</span>
                              {isCurrent && (
                                <span className="qb-st-pill qb-st-pill--accent">
                                  当前
                                </span>
                              )}
                              {!saved && (
                                <span className="qb-st-dim">
                                  {" "}
                                  · {clientName}还没存过
                                </span>
                              )}
                            </>
                          )}
                        </td>
                        <td
                          style={{ textAlign: "right", whiteSpace: "nowrap" }}
                        >
                          {renaming?.id !== p.id && (
                            <>
                              <Button
                                size="sm"
                                variant={isCurrent ? "ghost" : "primary"}
                                disabled={busy || isCurrent}
                                onClick={() =>
                                  void run(
                                    () => stationApi.projectApply(p.id, client),
                                    `换到项目「${p.name}」了`,
                                  )
                                }
                              >
                                {isCurrent ? "正在用" : "换到这个"}
                              </Button>{" "}
                              <Button
                                size="sm"
                                variant="ghost"
                                icon={<Pencil size={13} aria-hidden="true" />}
                                disabled={busy}
                                aria-label={`给「${p.name}」改名`}
                                onClick={() =>
                                  setRenaming({ id: p.id, name: p.name })
                                }
                              />{" "}
                              <Button
                                size="sm"
                                variant="ghost"
                                icon={<Trash2 size={13} aria-hidden="true" />}
                                disabled={busy}
                                aria-label={`删除「${p.name}」`}
                                onClick={() => setDeleting(p)}
                              />
                            </>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}

          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <input
              className="input"
              style={{ flex: 1, minWidth: 0 }}
              aria-label="新项目的名字"
              placeholder="例如：开发、绘图"
              value={newName}
              disabled={busy}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void create();
              }}
            />
            <Button
              icon={<Plus size={14} aria-hidden="true" />}
              disabled={busy || !newName.trim()}
              onClick={() => void create()}
            >
              按现在的样子新建
            </Button>
          </div>

          {error && (
            <p className="qb-st-note" role="alert">
              没成：{error}
            </p>
          )}
        </div>
      </Modal>

      <ConfirmDialog
        open={deleting != null}
        title="删除项目"
        danger
        confirmLabel="删除"
        loading={busy}
        onCancel={() => setDeleting(null)}
        onConfirm={() => {
          const p = deleting;
          if (!p) return;
          void run(
            () => stationApi.projectDelete(p.id),
            `删掉项目「${p.name}」了`,
          ).then(() => setDeleting(null));
        }}
      >
        确定删除项目「{deleting?.name}」？只删这个项目，不会改动任何现有配置。
      </ConfirmDialog>
    </>
  );
}
