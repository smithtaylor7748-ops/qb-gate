/**
 * 扩展中心「会话」分页（中转站重构阶段 6 · S1，照 cc-switch 的会话管理器）。
 *
 * Claude Code 和 Codex 留在本机的会话记录：每个软件的中转环境、官方默认目录、每个官方槽位。
 * 列表、搜索、阅读、导出 Markdown、复制恢复命令、删除（先挪进面板的回收站，能原样放回去）。
 * 后端 `usecase::session_ops`：只读本机文件；会话里有对话内容，不进日志、不外传。
 *
 * ⛔ 恢复命令只**复制**，不替使用者开终端（CLAUDE.md「别把 shell 当启动目标」）。
 */
import { useCallback, useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";

import {
  sessionsApi,
  type SessionDetail,
  type SessionList,
  type SessionMessage,
  type TrashEntry,
} from "../lib/sessions";
import { Button, ConfirmDialog, Modal, useToast } from "../ui";

const PAGE = 50;
const MSG_PAGE = 200;

function clock(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function bytes(n: number): string {
  if (n >= 1 << 20) return `${(n / (1 << 20)).toFixed(1)} MB`;
  if (n >= 1 << 10) return `${Math.round(n / (1 << 10))} KB`;
  return `${n} B`;
}

/** 一段消息是谁说的、是什么。 */
export function who(m: SessionMessage): string {
  if (m.kind === "summary") return "摘要";
  if (m.kind === "thinking") return "助手 · 思考";
  if (m.kind === "tool_use") return `助手 · 调用工具 ${m.name ?? ""}`.trim();
  if (m.kind === "tool_result") return "工具输出";
  return m.role === "user" ? "用户" : m.role === "assistant" ? "助手" : m.role;
}

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** 一个会话：头部信息、恢复命令、导出、删除，下面是逐段的内容（分页）。 */
function Reader({
  id,
  onClose,
  onDeleted,
}: {
  id: string;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const toast = useToast();
  const [offset, setOffset] = useState(0);
  const [detail, setDetail] = useState<SessionDetail | null>(null);
  const [error, setError] = useState("");
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    setError("");
    sessionsApi
      .read(id, offset, MSG_PAGE)
      .then((d) => live && setDetail(d))
      .catch((e: unknown) => live && setError(errText(e)));
    return () => {
      live = false;
    };
  }, [id, offset]);

  const s = detail?.summary;
  const copyResume = async () => {
    if (!detail) return;
    try {
      await navigator.clipboard.writeText(detail.resume);
      toast.ok("恢复命令已复制：在 PowerShell 里粘贴运行");
    } catch (e) {
      toast.error(`复制没成：${errText(e)}`);
    }
  };
  const exportMd = async () => {
    if (!s) return;
    try {
      const path = await save({
        defaultPath: `${s.title.replace(/[\\/:*?"<>|]/g, " ").slice(0, 60) || s.session_id}.md`,
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (!path) return;
      await sessionsApi.exportTo(id, path);
      toast.ok(`已导出到 ${path}`);
    } catch (e) {
      toast.error(`导出没成：${errText(e)}`);
    }
  };
  const remove = async () => {
    setBusy(true);
    try {
      await sessionsApi.remove(id);
      toast.ok("已挪进会话回收站，可以放回去");
      setAsking(false);
      onDeleted();
    } catch (e) {
      toast.error(errText(e));
      setAsking(false);
    } finally {
      setBusy(false);
    }
  };

  const total = detail?.total ?? 0;
  return (
    <Modal
      open
      onClose={onClose}
      size="huge"
      title={s ? s.title : "会话"}
      footer={<Button onClick={onClose}>关闭</Button>}
    >
      {error && <p className="notice notice--danger">{error}</p>}
      {!detail ? (
        !error && <p className="qb-muted">正在读这个会话…</p>
      ) : (
        <div className="qb-session" data-testid="session-reader">
          <div className="qb-facts qb-facts--wide">
            <div>
              <span>来源</span>
              <strong>{s?.source_label}</strong>
            </div>
            <div>
              <span>项目目录</span>
              <strong>{s?.cwd ?? "读不出来"}</strong>
            </div>
            <div>
              <span>会话 id</span>
              <strong>
                <code>{s?.session_id || "读不出来"}</code>
              </strong>
            </div>
            <div>
              <span>开始 / 最后写入</span>
              <strong>
                {clock(s?.started_at) || "—"} / {clock(s?.updated_at)}
              </strong>
            </div>
            <div>
              <span>文件</span>
              <strong>
                <code>{s?.path}</code>
              </strong>
              <span>{bytes(s?.size ?? 0)}</span>
            </div>
          </div>
          <div className="qb-session-actions">
            <Button
              size="sm"
              variant="primary"
              onClick={() => void copyResume()}
            >
              复制恢复命令
            </Button>
            <Button size="sm" onClick={() => void exportMd()}>
              导出 Markdown…
            </Button>
            <Button size="sm" variant="danger" onClick={() => setAsking(true)}>
              删除（挪进回收站）
            </Button>
          </div>
          <pre className="qb-session-resume" aria-label="恢复命令">
            {detail.resume}
          </pre>
          <p className="qb-muted">{detail.resume_note} 面板不替你开终端。</p>

          <div className="qb-session-msgs">
            {detail.messages.map((m, i) => (
              <div
                key={`${offset + i}`}
                className={`qb-session-msg qb-session-msg--${m.role}`}
              >
                <span className="qb-session-who">
                  {who(m)}
                  {m.at ? ` · ${clock(m.at)}` : ""}
                </span>
                {m.kind === "text" || m.kind === "summary" ? (
                  <p className="qb-session-text">{m.text}</p>
                ) : (
                  <details>
                    <summary className="qb-muted">
                      {m.text.split("\n")[0].slice(0, 120) || "（空）"}
                    </summary>
                    <pre className="qb-session-pre">{m.text}</pre>
                  </details>
                )}
                {m.truncated && (
                  <span className="qb-muted">（太长，后面截掉了）</span>
                )}
              </div>
            ))}
            {detail.messages.length === 0 && (
              <p className="qb-muted">这个会话里没有能显示的文字。</p>
            )}
          </div>
          {total > MSG_PAGE && (
            <div className="qb-usage-pager">
              <span className="notice">
                第 {offset + 1}–{Math.min(total, offset + MSG_PAGE)} 段，共{" "}
                {total} 段
              </span>
              <Button
                size="sm"
                disabled={offset === 0}
                onClick={() => setOffset(Math.max(0, offset - MSG_PAGE))}
              >
                上一页
              </Button>
              <Button
                size="sm"
                disabled={offset + MSG_PAGE >= total}
                onClick={() => setOffset(offset + MSG_PAGE)}
              >
                下一页
              </Button>
            </div>
          )}
        </div>
      )}
      <ConfirmDialog
        open={asking}
        onCancel={() => setAsking(false)}
        onConfirm={() => void remove()}
        title="把这个会话挪进回收站？"
        confirmLabel="挪进回收站"
        danger
        loading={busy}
      >
        <p>
          会话文件挪进面板的回收站（<code>session-trash</code>
          ），能原样放回去；只有在回收站里「彻底删除」才真删。
          两分钟内还在写的会话不让删 —— 多半正开着。
        </p>
      </ConfirmDialog>
    </Modal>
  );
}

export default function SessionsPanel() {
  const toast = useToast();
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");
  const [deep, setDeep] = useState(false);
  const [source, setSource] = useState<string | null>(null);
  const [page, setPage] = useState(0);
  const [list, setList] = useState<SessionList | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [reading, setReading] = useState<string | null>(null);
  const [trash, setTrash] = useState<TrashEntry[]>([]);
  const [purging, setPurging] = useState(false);

  // 打字停 300ms 再搜：内容搜索要读文件，每敲一个字都读一遍不行。
  useEffect(() => {
    const t = window.setTimeout(() => setQuery(text), 300);
    return () => window.clearTimeout(t);
  }, [text]);
  useEffect(() => setPage(0), [query, deep, source]);

  const load = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      setList(
        await sessionsApi.list({
          text: query,
          source,
          deep: deep && query.trim() !== "",
          offset: page * PAGE,
          limit: PAGE,
        }),
      );
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  }, [query, source, deep, page]);
  const loadTrash = useCallback(async () => {
    try {
      setTrash(await sessionsApi.trash());
    } catch {
      // 回收站读不出来不该挡住会话列表；下面那块就显示空。
      setTrash([]);
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load]);
  useEffect(() => {
    void loadTrash();
  }, [loadTrash]);

  const restore = async (id: string) => {
    try {
      await sessionsApi.restore(id);
      toast.ok("已放回原处");
      await Promise.all([load(), loadTrash()]);
    } catch (e) {
      toast.error(errText(e));
    }
  };
  const purge = async (id: string | null) => {
    try {
      const n = await sessionsApi.purge(id);
      toast.ok(`彻底删除了 ${n} 项`);
      setPurging(false);
      await loadTrash();
    } catch (e) {
      toast.error(errText(e));
    }
  };

  const pages = list ? Math.max(1, Math.ceil(list.total / PAGE)) : 1;
  const claude = list?.sources.filter((x) => x.client === "claude-code") ?? [];
  const codex = list?.sources.filter((x) => x.client === "codex") ?? [];
  return (
    <section className="qb-mcp" data-testid="sessions-panel">
      <p className="qb-muted">
        Claude Code 和 Codex
        留在本机的会话记录（中转环境、官方默认目录、每个官方槽位）。只读本机文件；
        删除先挪进面板的回收站。Gemini CLI、OpenCode 等的会话这里还看不到。
      </p>
      <div className="qb-catalog-toolbar">
        <label className="qb-search-box">
          <input
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="搜标题、项目目录、会话 id"
            aria-label="搜会话"
          />
        </label>
        <label className="qb-check-row">
          <input
            type="checkbox"
            checked={deep}
            onChange={(e) => setDeep(e.target.checked)}
          />
          连内容一起搜（慢）
        </label>
        <Button size="sm" loading={busy} onClick={() => void load()}>
          刷新
        </Button>
      </div>
      <div className="qb-filter-pills" role="group" aria-label="哪一处的会话">
        <button aria-pressed={source == null} onClick={() => setSource(null)}>
          全部
        </button>
        {[...claude, ...codex].map((x) => (
          <button
            key={x.id}
            aria-pressed={source === x.id}
            onClick={() => setSource(x.id)}
          >
            {x.label} · {x.count}
          </button>
        ))}
      </div>
      {error && <p className="notice notice--danger">{error}</p>}
      {list?.search_truncated && (
        <p className="notice">
          内容搜索读到上限停了，后面的会话没搜内容（标题和目录照样搜了）。
        </p>
      )}
      {!list ? (
        !error && <p className="qb-muted">正在读会话记录…</p>
      ) : list.items.length === 0 ? (
        <p className="qb-muted">
          {query ? "没有符合的会话。" : "这里还没有会话记录。"}
        </p>
      ) : (
        <>
          <div className="qb-mcp-list">
            {list.items.map((s) => (
              <div key={s.id} className="qb-mcp-row qb-session-row">
                <div className="qb-mcp-head">
                  <button
                    type="button"
                    className="qb-session-open"
                    title={s.title}
                    onClick={() => setReading(s.id)}
                  >
                    {s.title}
                  </button>
                  <span className="qb-muted qb-session-when">
                    {clock(s.updated_at)}
                  </span>
                </div>
                <span className="qb-muted qb-mcp-summary">
                  {s.source_label} · {s.cwd ?? "项目目录读不出来"} ·{" "}
                  {bytes(s.size)}
                </span>
              </div>
            ))}
          </div>
          <div className="qb-usage-pager">
            <span className="notice">共 {list.total} 个</span>
            <Button
              size="sm"
              disabled={page === 0}
              onClick={() => setPage(page - 1)}
            >
              上一页
            </Button>
            <span className="notice">
              {page + 1} / {pages}
            </span>
            <Button
              size="sm"
              disabled={page >= pages - 1}
              onClick={() => setPage(page + 1)}
            >
              下一页
            </Button>
          </div>
        </>
      )}

      <details className="qb-mcp-row">
        <summary>
          <b>会话回收站</b> <span className="qb-muted">{trash.length} 项</span>
        </summary>
        {trash.length === 0 ? (
          <p className="qb-muted">空的。</p>
        ) : (
          <>
            <div className="qb-mcp-list mt-2">
              {trash.map((t) => (
                <div key={t.id} className="qb-mcp-head">
                  <span
                    className="qb-session-trash-name"
                    title={t.originals.join("\n")}
                  >
                    {t.name}
                  </span>
                  <span className="qb-muted">
                    {t.deleted_at} · {bytes(t.size)}
                  </span>
                  <Button size="sm" onClick={() => void restore(t.id)}>
                    放回去
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    onClick={() => void purge(t.id)}
                  >
                    彻底删除
                  </Button>
                </div>
              ))}
            </div>
            <Button
              size="sm"
              variant="danger"
              className="mt-2"
              onClick={() => setPurging(true)}
            >
              清空会话回收站
            </Button>
          </>
        )}
      </details>
      <ConfirmDialog
        open={purging}
        onCancel={() => setPurging(false)}
        onConfirm={() => void purge(null)}
        title="清空会话回收站？"
        confirmLabel="彻底删除"
        danger
      >
        <p>回收站里的 {trash.length} 项会真的删掉，放不回去了。</p>
      </ConfirmDialog>

      {reading && (
        <Reader
          id={reading}
          onClose={() => setReading(null)}
          onDeleted={() => {
            setReading(null);
            void load();
            void loadTrash();
          }}
        />
      )}
    </section>
  );
}
