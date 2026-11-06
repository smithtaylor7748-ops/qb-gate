/**
 * 扩展中心「会话」分页（中转站重构阶段 6 · S1，照 cc-switch 的会话管理器）。
 *
 * Claude Code 和 Codex 留在本机的会话记录：每个软件的中转环境、官方默认目录、每个官方槽位。
 * 列表、搜索、阅读、导出 Markdown、复制恢复命令、删除（先挪进面板的回收站，能原样放回去）。
 * 后端 `usecase::session_ops`：只读本机文件；会话里有对话内容，不进日志、不外传。
 *
 * ⛔ 恢复命令只**复制**，不替使用者开终端（CLAUDE.md「别把 shell 当启动目标」）。
 */
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import {
  Copy,
  FolderOpen,
  MessagesSquare,
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react";

import {
  sessionsApi,
  type SessionDetail,
  type SessionList,
  type SessionMessage,
  type TrashEntry,
} from "../lib/sessions";
import type { SessionSourceCount } from "../lib/generated/SessionSourceCount";
import {
  Button,
  ConfirmDialog,
  EmptyState,
  Modal,
  Pill,
  useToast,
} from "../ui";
import { ago, compact, groupSessions, shortId } from "./accounts/sessionGroups";

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

/** 集合里有就去掉、没有就加上，返回新集合（勾选、折叠用）。 */
function toggleIn(set: Set<string>, id: string): Set<string> {
  const next = new Set(set);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}

const KINDS: [string | null, string][] = [
  ["chat", "对话"],
  ["subagent", "子代理"],
  [null, "全部"],
];

export default function SessionsPanel({
  lockedSource,
  onSources,
  toolbarStart,
  hint,
}: {
  /** 给了就只看这一处，不显示来源筛选（账户页的会话弹窗用）。 */
  lockedSource?: string;
  /** 每次读完列表把各处的数量报给外面（弹窗拿它做槽位选择）。 */
  onSources?: (sources: SessionSourceCount[]) => void;
  /** 放在工具栏最前面的东西（弹窗的槽位选择）。 */
  toolbarStart?: ReactNode;
  /** 工具栏下面一行小字说明。 */
  hint?: ReactNode;
} = {}) {
  const toast = useToast();
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");
  const [contentText, setContentText] = useState("");
  const [content, setContent] = useState("");
  const [kind, setKind] = useState<string | null>("chat");
  const [picked, setSource] = useState<string | null>(null);
  const source = lockedSource ?? picked;
  const [page, setPage] = useState(0);
  const [list, setList] = useState<SessionList | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [reading, setReading] = useState<string | null>(null);
  const [trash, setTrash] = useState<TrashEntry[]>([]);
  const [purging, setPurging] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [trashOpen, setTrashOpen] = useState(false);
  const [trashSel, setTrashSel] = useState<Set<string>>(new Set());
  /** 要彻底删除的那几项（回收站里一条或选中的几条）：真删之前先问一句，放不回去了。 */
  const [purgeAsk, setPurgeAsk] = useState<string[] | null>(null);
  const sizeOf = (ids: Iterable<string>) => {
    const set = new Set(ids);
    return bytes(
      trash.filter((t) => set.has(t.id)).reduce((n, t) => n + t.size, 0),
    );
  };

  // 打字停 300ms 再搜：内容搜索要读文件，每敲一个字都读一遍不行。
  useEffect(() => {
    const t = window.setTimeout(() => setQuery(text), 300);
    return () => window.clearTimeout(t);
  }, [text]);
  useEffect(() => {
    const t = window.setTimeout(() => setContent(contentText), 500);
    return () => window.clearTimeout(t);
  }, [contentText]);
  useEffect(() => setPage(0), [query, content, kind, source]);
  useEffect(() => setSelected(new Set()), [list]);

  const load = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      const got = await sessionsApi.list({
        text: query,
        content,
        kind,
        source,
        offset: page * PAGE,
        limit: PAGE,
      });
      setList(got);
      onSources?.(got.sources);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  }, [query, content, kind, source, page, onSources]);
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

  const copyId = async (id: string) => {
    try {
      await navigator.clipboard.writeText(id);
      toast.ok("会话 id 已复制");
    } catch (e) {
      toast.error(`复制没成：${errText(e)}`);
    }
  };
  const reveal = async (id: string) => {
    try {
      await sessionsApi.reveal(id);
    } catch (e) {
      toast.error(errText(e));
    }
  };

  /** 放回去（一条或多条）；放完关掉回收站弹窗，回到列表。 */
  const restoreMany = async (ids: string[]) => {
    let ok = 0;
    for (const id of ids) {
      try {
        await sessionsApi.restore(id);
        ok += 1;
      } catch (e) {
        toast.error(errText(e));
      }
    }
    if (ok) toast.ok(ok === 1 ? "已放回原处" : `已放回 ${ok} 条`);
    setTrashSel(new Set());
    setTrashOpen(false);
    await Promise.all([load(), loadTrash()]);
  };
  const purgeMany = async (ids: string[]) => {
    let n = 0;
    for (const id of ids) {
      try {
        n += await sessionsApi.purge(id);
      } catch (e) {
        toast.error(errText(e));
      }
    }
    toast.ok(`彻底删除了 ${n} 项`);
    setTrashSel(new Set());
    setPurgeAsk(null);
    await loadTrash();
  };
  /** 勾选的会话一起挪进回收站。 */
  const removeSelected = async () => {
    let ok = 0;
    for (const id of selected) {
      try {
        await sessionsApi.remove(id);
        ok += 1;
      } catch (e) {
        toast.error(errText(e));
      }
    }
    setConfirmRemove(false);
    setSelected(new Set());
    if (ok) toast.ok(`已把 ${ok} 条会话挪进回收站`);
    await Promise.all([load(), loadTrash()]);
  };
  const applyNow = () => {
    setQuery(text);
    setContent(contentText);
  };
  const clearSearch = () => {
    setText("");
    setContentText("");
    setQuery("");
    setContent("");
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
  const pageIds = list?.items.map((x) => x.id) ?? [];
  const allChecked =
    pageIds.length > 0 && pageIds.every((id) => selected.has(id));
  const claude = list?.sources.filter((x) => x.client === "claude-code") ?? [];
  const codex = list?.sources.filter((x) => x.client === "codex") ?? [];
  return (
    <section className="qb-session-panel" data-testid="sessions-panel">
      <p className="qb-session-hint">
        {lockedSource === undefined
          ? "Claude Code 和 Codex 留在本机的会话记录（中转环境、官方默认目录、每个官方槽位）。只读本机文件；删除先挪进面板的回收站，能原样放回去。Gemini CLI、OpenCode 等的会话这里还看不到。"
          : "按项目展开查看会话；删除先挪进回收站，能原样放回去。"}
        {hint ? <> {hint}</> : null}
      </p>

      {/* 操作栏（2026-11-06 照 Cockpit Tools 的会话管理重排，使用者：「Cockpit 什么样子，你就做成什么样子」）。 */}
      <div className="qb-session-bar">
        {toolbarStart}
        <label className="qb-session-field">
          <span>标题</span>
          <input
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && applyNow()}
            placeholder="按标题搜索（也搜项目目录、会话 ID）"
            aria-label="搜标题"
          />
        </label>
        <label className="qb-session-field">
          <span>对话内容</span>
          <input
            value={contentText}
            onChange={(e) => setContentText(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && applyNow()}
            placeholder="按对话内容搜索（读文件，有上限）"
            aria-label="搜内容"
          />
        </label>
        <Button
          size="sm"
          variant="primary"
          icon={<Search size={13} />}
          onClick={applyNow}
        >
          搜索
        </Button>
        <Button
          size="sm"
          disabled={!text && !contentText}
          onClick={clearSearch}
        >
          清空
        </Button>
        <span className="qb-session-bar-gap" />
        <Button
          size="sm"
          icon={<Trash2 size={13} />}
          onClick={() => setTrashOpen(true)}
        >
          <span>会话回收站</span>
          {trash.length > 0 && (
            <span className="qb-session-badge">{trash.length}</span>
          )}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          icon={<RefreshCw size={13} />}
          aria-label="刷新会话列表"
          title="刷新"
          loading={busy}
          onClick={() => void load()}
        />
      </div>

      {lockedSource === undefined && (
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
      )}
      {error && <p className="notice notice--danger">{error}</p>}
      {list?.search_truncated && (
        <p className="notice">
          内容搜索读到上限停了（一次最多现读 2000 个文件、512
          MB），没读到的会话当没搜到；再搜一次会接着读。
        </p>
      )}

      <div className="qb-session-selbar">
        <label className="qb-session-check">
          <input
            type="checkbox"
            checked={allChecked}
            disabled={!pageIds.length}
            onChange={() =>
              setSelected(allChecked ? new Set() : new Set(pageIds))
            }
            aria-label="全选这一页的会话"
          />
          全选
        </label>
        {selected.size > 0 && (
          <span className="qb-session-selcount">已选 {selected.size} 条</span>
        )}
        <div className="qb-segmented" role="group" aria-label="会话类型">
          {KINDS.map(([k, label]) => (
            <button
              key={label}
              type="button"
              aria-pressed={kind === k}
              onClick={() => setKind(k)}
            >
              {label}
            </button>
          ))}
        </div>
        <span className="qb-session-bar-gap" />
        <Button
          size="sm"
          variant="danger"
          icon={<Trash2 size={13} />}
          disabled={!selected.size}
          onClick={() => setConfirmRemove(true)}
        >
          移到回收站
        </Button>
      </div>

      <div className="qb-session-list">
        {!list ? (
          !error && <p className="qb-session-loading">正在读会话记录…</p>
        ) : list.items.length === 0 ? (
          <div className="qb-session-empty">
            <EmptyState
              icon={<MessagesSquare size={20} />}
              title={query || content ? "未找到匹配会话" : "还没有可管理的会话"}
            >
              {query || content
                ? "换个关键词再试，或者把类型切到「全部」。"
                : "用这个账户跑过 Claude Code / Codex 之后，会话会出现在这里。"}
            </EmptyState>
          </div>
        ) : (
          groupSessions(list.items).map((g) => {
            const open = !collapsed.has(g.key);
            const flip = () => setCollapsed(toggleIn(collapsed, g.key));
            return (
              <section key={g.key} className="qb-session-group">
                <div className="qb-session-group-head">
                  <button
                    type="button"
                    className="qb-session-group-toggle"
                    aria-expanded={open}
                    onClick={flip}
                  >
                    <FolderOpen size={13} aria-hidden="true" />
                    <b title={g.items[0]?.cwd ?? undefined}>{g.label}</b>
                    <span>{g.items.length} 条会话</span>
                  </button>
                  {g.unowned && (
                    <span className="qb-session-group-note">
                      官方默认目录里找不到归属依据，不算给任何账户
                    </span>
                  )}
                  <button
                    type="button"
                    className="qb-session-link"
                    onClick={flip}
                  >
                    {open ? "收起" : "展开"}
                  </button>
                </div>
                {open &&
                  g.items.map((s) => (
                    <div key={s.id} className="qb-session-item">
                      <input
                        type="checkbox"
                        checked={selected.has(s.id)}
                        onChange={() => setSelected(toggleIn(selected, s.id))}
                        aria-label={`选中 ${s.title || "未命名会话"}`}
                      />
                      <div className="qb-session-main">
                        <div className="qb-session-titleline">
                          <button
                            type="button"
                            className="qb-session-open"
                            title={s.title}
                            onClick={() => setReading(s.id)}
                          >
                            {s.title || "未命名会话"}
                          </button>
                          {s.running && (
                            <Pill tone="ok" title="两分钟内还在写，多半正开着">
                              运行中
                            </Pill>
                          )}
                          {s.kind === "subagent" && <Pill>子代理</Pill>}
                        </div>
                        <div className="qb-session-meta2">
                          {s.session_id && (
                            <span>
                              会话 ID{" "}
                              <button
                                type="button"
                                className="qb-session-id"
                                title={`复制会话 ID：${s.session_id}`}
                                onClick={() => void copyId(s.session_id)}
                              >
                                <code>{shortId(s.session_id)}</code>
                                <Copy size={11} aria-hidden="true" />
                              </button>
                            </span>
                          )}
                          {lockedSource === undefined && (
                            <span>{s.source_label}</span>
                          )}
                          {s.tokens && (
                            <span title="输入含缓存读写 / 输出 / 合计">
                              Token 使用 {compact(s.tokens.total)}（入{" "}
                              {compact(s.tokens.input)} · 出{" "}
                              {compact(s.tokens.output)}）
                            </span>
                          )}
                          <span>{bytes(s.size)}</span>
                        </div>
                      </div>
                      <div className="qb-session-side">
                        <span
                          className="qb-session-when"
                          title={clock(s.updated_at)}
                        >
                          最近更新 {ago(s.updated_at)}
                        </span>
                        <Button
                          size="sm"
                          variant="ghost"
                          icon={<FolderOpen size={13} />}
                          aria-label="打开所在位置"
                          title="打开所在位置"
                          onClick={() => void reveal(s.id)}
                        />
                      </div>
                    </div>
                  ))}
              </section>
            );
          })
        )}
      </div>

      {list && list.total > 0 && (
        <div className="qb-session-pager">
          <span>共 {list.total} 条</span>
          {list.total > PAGE && (
            <>
              <Button
                size="sm"
                variant="ghost"
                disabled={page === 0}
                onClick={() => setPage(page - 1)}
              >
                上一页
              </Button>
              <span>
                {page + 1} / {pages}
              </span>
              <Button
                size="sm"
                variant="ghost"
                disabled={page >= pages - 1}
                onClick={() => setPage(page + 1)}
              >
                下一页
              </Button>
            </>
          )}
        </div>
      )}

      <Modal
        open={trashOpen}
        onClose={() => setTrashOpen(false)}
        size="wide"
        title="会话回收站"
        footer={
          <>
            <Button
              variant="danger"
              disabled={!trash.length}
              onClick={() => setPurging(true)}
            >
              清空会话回收站
            </Button>
            <Button onClick={() => setTrashOpen(false)}>关闭</Button>
          </>
        }
      >
        <div className="qb-session-panel">
          <p className="qb-session-hint">
            回收站里的会话可以放回原处，也可以彻底删除、腾出磁盘。
          </p>
          {trash.length === 0 ? (
            <div className="qb-session-list">
              <div className="qb-session-empty">
                <EmptyState
                  icon={<Trash2 size={20} />}
                  title="回收站里还没有会话"
                >
                  挪进回收站的会话会显示在这里。
                </EmptyState>
              </div>
            </div>
          ) : (
            <>
              <div className="qb-session-selbar">
                <label className="qb-session-check">
                  <input
                    type="checkbox"
                    checked={trashSel.size === trash.length}
                    onChange={() =>
                      setTrashSel(
                        trashSel.size === trash.length
                          ? new Set()
                          : new Set(trash.map((t) => t.id)),
                      )
                    }
                    aria-label="全选回收站"
                  />
                  全选
                </label>
                <span className="qb-session-selcount">
                  {trashSel.size
                    ? `已选 ${trashSel.size} 条，${sizeOf(trashSel)}`
                    : `共 ${trash.length} 条，${bytes(trash.reduce((n, t) => n + t.size, 0))}`}
                </span>
                <span className="qb-session-bar-gap" />
                <Button
                  size="sm"
                  disabled={!trashSel.size}
                  onClick={() => void restoreMany([...trashSel])}
                >
                  放回选中
                </Button>
                <Button
                  size="sm"
                  variant="danger"
                  disabled={!trashSel.size}
                  onClick={() => setPurgeAsk([...trashSel])}
                >
                  彻底删除选中
                </Button>
              </div>
              <div className="qb-session-list qb-session-list--trash">
                {trash.map((t) => (
                  <div key={t.id} className="qb-session-item">
                    <input
                      type="checkbox"
                      checked={trashSel.has(t.id)}
                      onChange={() => setTrashSel(toggleIn(trashSel, t.id))}
                      aria-label={`选中 ${t.name}`}
                    />
                    <div className="qb-session-main">
                      <span
                        className="qb-session-trash-name"
                        title={t.originals.join("\n")}
                      >
                        {t.name}
                      </span>
                      <span className="qb-session-meta2">
                        <span>挪进来于 {t.deleted_at}</span>
                        <span>大小 {bytes(t.size)}</span>
                      </span>
                    </div>
                    <div className="qb-session-side">
                      <Button
                        size="sm"
                        onClick={() => void restoreMany([t.id])}
                      >
                        放回去
                      </Button>
                      <Button
                        size="sm"
                        variant="danger"
                        onClick={() => setPurgeAsk([t.id])}
                      >
                        彻底删除
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
      </Modal>
      <ConfirmDialog
        open={confirmRemove}
        onCancel={() => setConfirmRemove(false)}
        onConfirm={() => void removeSelected()}
        title="移到回收站"
        confirmLabel="移到回收站"
        danger
      >
        <p>
          把选中的 {selected.size}
          条会话挪进面板的回收站，之后可以原样放回去。正开着的客户端可能要重开才看得到变化。
        </p>
      </ConfirmDialog>
      <ConfirmDialog
        open={purging}
        onCancel={() => setPurging(false)}
        onConfirm={() => void purge(null)}
        title="清空会话回收站？"
        confirmLabel="彻底删除"
        danger
      >
        <p>
          回收站里的 {trash.length} 项会真的删掉，放不回去了，腾出{" "}
          {bytes(trash.reduce((n, t) => n + t.size, 0))}。
        </p>
      </ConfirmDialog>
      <ConfirmDialog
        open={!!purgeAsk}
        onCancel={() => setPurgeAsk(null)}
        onConfirm={() => purgeAsk && void purgeMany(purgeAsk)}
        title={
          purgeAsk && purgeAsk.length > 1
            ? `彻底删除选中的 ${purgeAsk.length} 条？`
            : "彻底删除这一条？"
        }
        confirmLabel="彻底删除"
        danger
      >
        <p>删掉之后放不回去了，腾出 {purgeAsk ? sizeOf(purgeAsk) : ""}。</p>
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
