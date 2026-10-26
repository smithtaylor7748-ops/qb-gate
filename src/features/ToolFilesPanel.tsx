/**
 * 工具文件（中转站重构阶段 5，C3 / C4 的附带件，照 cc-switch）：OpenClaw 的工作区文件与每日记忆、
 * Hermes 的两份记忆（加开关和字数上限）、Pi 的系统提示词文件与斜杠命令模板。
 *
 * 都是那个工具自己目录里的 Markdown，这里只给一个编辑器：没装不写（不替它建目录）、
 * 写回之前核对这期间没被别处改过、删掉的先挪进面板的备份目录。不碰中转，也不碰官方账户。
 */

import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Pencil, Plus, Search, Trash2 } from "lucide-react";

import type { CoexistTool } from "../lib/generated/CoexistTool";
import type { HermesMemorySettings } from "../lib/generated/HermesMemorySettings";
import type { ToolFile } from "../lib/generated/ToolFile";
import type { ToolFileHit } from "../lib/generated/ToolFileHit";
import type { ToolFilesView } from "../lib/generated/ToolFilesView";
import { useSession } from "../lib/store";
import { workspaceApi } from "../lib/workspace";
import { Button, ConfirmDialog, Modal, useToast } from "../ui";

const MISSING = "missing";

const TOOLS: { id: CoexistTool; name: string; intro: string }[] = [
  {
    id: "open-claw",
    name: "OpenClaw",
    intro:
      "OpenClaw 工作区里的九份说明（人格、用户档案、工具文档、长期记忆……）和每天一份的记忆（workspace/memory/日期.md）。",
  },
  {
    id: "hermes",
    name: "Hermes",
    intro:
      "Hermes 的两份记忆：MEMORY.md 是它自己的笔记、USER.md 是关于你的档案，都会放进它的系统提示词；超过字数上限的部分 Hermes 读的时候截掉。",
  },
  {
    id: "pi",
    name: "Pi",
    intro:
      "Pi 的系统提示词文件（SYSTEM.md 整份替换、APPEND_SYSTEM.md 接在后面）和斜杠命令模板（prompts/名字.md，在 Pi 里打 /名字 用）。",
  },
];

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

function today(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** 列表里那一行的第二行：多少字、什么时候改的。 */
function facts(f: ToolFile): string {
  if (!f.exists) return "还没有这个文件";
  const when = f.modified_ms
    ? new Date(f.modified_ms).toLocaleString("zh-CN", { hour12: false })
    : "";
  return `${f.content.length} 字${when ? ` · 改于 ${when}` : ""}`;
}

export default function ToolFilesPanel() {
  const toast = useToast();
  const [tool, setTool] = useSession<CoexistTool>(
    "toolfiles.tool",
    "open-claw",
  );
  const [view, setView] = useState<ToolFilesView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<ToolFile | null>(null);
  const [draft, setDraft] = useState("");
  const [deleting, setDeleting] = useState<ToolFile | null>(null);
  const [newName, setNewName] = useState("");
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<ToolFileHit[] | null>(null);
  const [hermes, setHermes] = useState<HermesMemorySettings | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const v = await workspaceApi.toolFiles(tool);
      setView(v);
      setHermes(v.hermes);
    } catch (e) {
      setError(message(e));
    }
  }, [tool]);

  useEffect(() => {
    setView(null);
    setHits(null);
    setQuery("");
    setNewName("");
    void load();
  }, [load]);

  const meta = TOOLS.find((t) => t.id === tool) ?? TOOLS[0];

  const open = (f: ToolFile) => {
    setEditing(f);
    setDraft(f.content);
    setError(null);
  };

  const save = async () => {
    if (!editing) return;
    setBusy(true);
    setError(null);
    try {
      await workspaceApi.toolFileSave(
        tool,
        editing.name,
        draft,
        editing.revision,
      );
      toast.ok(`存好了：${editing.name}`);
      setEditing(null);
      await load();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (f: ToolFile) => {
    setBusy(true);
    setError(null);
    try {
      setView(await workspaceApi.toolFileDelete(tool, f.name, f.revision));
      toast.ok(`删掉了 ${f.name}（原内容在面板的备份目录里）`);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
      setDeleting(null);
    }
  };

  /** 新建一份：每日记忆按日期、Pi 模板按名字；已经有了就打开那一份。 */
  const create = () => {
    const name =
      tool === "open-claw"
        ? `workspace/memory/${newName.trim() || today()}.md`
        : `prompts/${newName.trim()}.md`;
    const have = view?.entries.find((e) => e.name === name);
    open(
      have ?? {
        name,
        label: null,
        exists: false,
        content: "",
        revision: MISSING,
        size: 0,
        modified_ms: 0,
      },
    );
  };

  const search = async () => {
    if (!query.trim()) {
      setHits(null);
      return;
    }
    try {
      setHits(await workspaceApi.toolFilesSearch(query));
    } catch (e) {
      setError(message(e));
    }
  };

  const saveHermes = async () => {
    if (!hermes) return;
    setBusy(true);
    setError(null);
    try {
      setHermes(await workspaceApi.hermesMemorySave(hermes));
      toast.ok("Hermes 的记忆设置存好了：重开 Hermes 生效");
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };

  const openHermesWeb = async () => {
    try {
      const url = await workspaceApi.hermesWeb();
      if (url) await openUrl(url);
      else
        toast.info(
          "Hermes 的网页面板没开着：在终端里跑 hermes dashboard 起它，再点一次",
        );
    } catch (e) {
      setError(message(e));
    }
  };

  const row = (f: ToolFile, deletable: boolean) => (
    <article key={f.name} className="qb-mcp-row">
      <div className="qb-mcp-head">
        <b>{f.name}</b>
        {f.label && <span className="qb-muted">{f.label}</span>}
        <span style={{ flex: 1 }} />
        <Button
          size="sm"
          icon={<Pencil size={13} aria-hidden="true" />}
          disabled={busy || !view?.installed}
          onClick={() => open(f)}
        >
          {f.exists ? "编辑" : "新建"}
        </Button>
        {deletable && f.exists && (
          <Button
            size="sm"
            variant="ghost"
            icon={<Trash2 size={13} aria-hidden="true" />}
            disabled={busy}
            aria-label={`删除 ${f.name}`}
            onClick={() => setDeleting(f)}
          />
        )}
      </div>
      <p className="qb-muted qb-mcp-summary">{facts(f)}</p>
    </article>
  );

  return (
    <section className="qb-mcp">
      <div className="qb-filter-pills" role="group" aria-label="哪个工具">
        {TOOLS.map((t) => (
          <button
            key={t.id}
            aria-pressed={tool === t.id}
            onClick={() => setTool(t.id)}
          >
            {t.name}
          </button>
        ))}
      </div>
      <p className="qb-muted">
        {meta.intro}
        面板只是编辑器：没装不写，写回之前核对这期间没被别处改过，删掉的先挪进面板的备份目录。
      </p>
      {error && <p className="notice notice--danger">{error}</p>}
      {!view ? (
        !error && (
          <p className="qb-muted" role="status">
            正在读取…
          </p>
        )
      ) : (
        <>
          {!view.installed && (
            <p className="notice">
              没找到 {meta.name}（{view.dir}）：多半没装，这里只能看、不写。
            </p>
          )}
          <div className="qb-mcp-list">
            {view.files.map((f) => row(f, tool === "pi"))}
          </div>

          {tool === "hermes" && hermes && (
            <div className="qb-mcp-row">
              <div className="qb-mcp-head">
                <b>记忆开关与字数上限</b>
                <span className="qb-muted">
                  config.yaml 的 memory 小节，只改这四个
                </span>
              </div>
              <div
                style={{
                  display: "flex",
                  gap: 12,
                  flexWrap: "wrap",
                  alignItems: "center",
                }}
              >
                <label className="qb-check-row">
                  <input
                    type="checkbox"
                    checked={hermes.memory_enabled}
                    disabled={busy || !view.installed}
                    onChange={(e) =>
                      setHermes({ ...hermes, memory_enabled: e.target.checked })
                    }
                  />
                  用 MEMORY.md
                </label>
                <label>
                  上限{" "}
                  <input
                    className="input w-24"
                    type="number"
                    min={0}
                    aria-label="MEMORY.md 字数上限"
                    value={hermes.memory_limit}
                    disabled={busy || !view.installed}
                    onChange={(e) =>
                      setHermes({
                        ...hermes,
                        memory_limit: Math.max(0, Number(e.target.value) || 0),
                      })
                    }
                  />{" "}
                  字
                </label>
                <label className="qb-check-row">
                  <input
                    type="checkbox"
                    checked={hermes.user_enabled}
                    disabled={busy || !view.installed}
                    onChange={(e) =>
                      setHermes({ ...hermes, user_enabled: e.target.checked })
                    }
                  />
                  用 USER.md
                </label>
                <label>
                  上限{" "}
                  <input
                    className="input w-24"
                    type="number"
                    min={0}
                    aria-label="USER.md 字数上限"
                    value={hermes.user_limit}
                    disabled={busy || !view.installed}
                    onChange={(e) =>
                      setHermes({
                        ...hermes,
                        user_limit: Math.max(0, Number(e.target.value) || 0),
                      })
                    }
                  />{" "}
                  字
                </label>
                <Button
                  disabled={busy || !view.installed}
                  onClick={() => void saveHermes()}
                >
                  存设置
                </Button>
                <span style={{ flex: 1 }} />
                <Button onClick={() => void openHermesWeb()}>
                  打开 Hermes 网页面板
                </Button>
              </div>
            </div>
          )}

          {tool !== "hermes" && (
            <>
              <div className="qb-catalog-toolbar">
                <b>{tool === "open-claw" ? "每日记忆" : "斜杠命令模板"}</b>
                <span style={{ flex: 1 }} />
                {tool === "open-claw" && (
                  <label className="qb-search-box">
                    <Search size={16} />
                    <input
                      aria-label="搜索每日记忆"
                      placeholder="搜全文…"
                      value={query}
                      onChange={(e) => setQuery(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") void search();
                      }}
                    />
                  </label>
                )}
                <input
                  className="input"
                  style={{ width: 160 }}
                  aria-label={tool === "open-claw" ? "日期" : "模板名"}
                  placeholder={
                    tool === "open-claw" ? `日期，默认 ${today()}` : "模板名"
                  }
                  value={newName}
                  disabled={busy || !view.installed}
                  onChange={(e) => setNewName(e.target.value)}
                />
                <Button
                  icon={<Plus size={14} aria-hidden="true" />}
                  disabled={
                    busy ||
                    !view.installed ||
                    (tool === "pi" && !newName.trim())
                  }
                  onClick={create}
                >
                  {tool === "open-claw" ? "写一天" : "新建模板"}
                </Button>
              </div>
              {hits && (
                <div className="qb-mcp-list" aria-label="搜索结果">
                  {hits.length === 0 ? (
                    <p className="qb-muted">没搜到。</p>
                  ) : (
                    hits.map((h) => (
                      <article key={h.name} className="qb-mcp-row">
                        <div className="qb-mcp-head">
                          <b>{h.name}</b>
                          <span className="qb-muted">{h.matches} 处</span>
                        </div>
                        <p className="qb-muted qb-mcp-summary">{h.snippet}</p>
                      </article>
                    ))
                  )}
                </div>
              )}
              {view.entries.length === 0 ? (
                <p className="qb-muted">
                  {tool === "open-claw" ? "还没有每日记忆。" : "还没有模板。"}
                </p>
              ) : (
                <div className="qb-mcp-list">
                  {view.entries.map((f) => row(f, true))}
                </div>
              )}
            </>
          )}
        </>
      )}

      <Modal
        open={editing != null}
        onClose={() => !busy && setEditing(null)}
        title={editing ? `${meta.name} · ${editing.name}` : ""}
        size="wide"
        footer={
          <>
            <Button disabled={busy} onClick={() => setEditing(null)}>
              取消
            </Button>
            <Button
              variant="primary"
              loading={busy}
              onClick={() => void save()}
            >
              存
            </Button>
          </>
        }
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {editing?.label && <p className="qb-muted">{editing.label}</p>}
          <textarea
            className="input qb-code-input"
            aria-label="内容"
            rows={18}
            value={draft}
            disabled={busy}
            onChange={(e) => setDraft(e.target.value)}
          />
          {error && <p className="notice notice--danger">{error}</p>}
        </div>
      </Modal>

      <ConfirmDialog
        open={deleting != null}
        title="删除这份文件"
        danger
        confirmLabel="删除"
        loading={busy}
        onCancel={() => setDeleting(null)}
        onConfirm={() => deleting && void remove(deleting)}
      >
        从 {meta.name} 的目录里拿掉「{deleting?.name}
        」？原内容先挪进面板的备份目录（tool-file-backups），要的话能从那里找回来。
      </ConfirmDialog>
    </section>
  );
}
