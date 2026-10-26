/**
 * MCP 面板（中转站重构阶段 5，cc-switch 的 M1）：一条服务器勾上哪些目标，就写进哪些配置。
 *
 * 照 cc-switch 的「统一 MCP 面板」：一行一条服务器、每行一排开关。不一样的是开关按**目标**来 ——
 * Claude Code / Codex / Gemini CLI 各有中转环境、官方默认目录、官方槽位好几份配置。
 * 官方那一侧标着「官方」，要使用者自己勾；新建的默认只勾 Claude Code 的中转环境。
 * 只增删面板登记的那几条，使用者手写的别的 MCP 不动；连接定义里的 Key 库里加密存。
 */

import { useEffect, useMemo, useState } from "react";
import { Pencil, Plus, Trash2, Upload } from "lucide-react";

import type { McpServer } from "../lib/generated/McpServer";
import type { McpTarget } from "../lib/generated/McpTarget";
import type { McpView } from "../lib/generated/McpView";
import { workspaceApi } from "../lib/workspace";
import { Button, Modal, useToast } from "../ui";

/** 常用的几个（cc-switch 的 `mcpPresets` 五个 + 原来扩展目录里的两个）。Windows 上的 `cmd /c` 由后端写 Claude Code 时补。 */
const PRESETS: Array<Pick<McpServer, "id" | "name" | "server" | "homepage">> = [
  {
    id: "fetch",
    name: "mcp-server-fetch",
    server: { type: "stdio", command: "uvx", args: ["mcp-server-fetch"] },
    homepage: "https://github.com/modelcontextprotocol/servers",
  },
  {
    id: "time",
    name: "mcp-server-time",
    server: { type: "stdio", command: "uvx", args: ["mcp-server-time"] },
    homepage: "https://github.com/modelcontextprotocol/servers",
  },
  {
    id: "memory",
    name: "@modelcontextprotocol/server-memory",
    server: {
      type: "stdio",
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-memory"],
    },
    homepage: "https://github.com/modelcontextprotocol/servers",
  },
  {
    id: "sequential-thinking",
    name: "@modelcontextprotocol/server-sequential-thinking",
    server: {
      type: "stdio",
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-sequential-thinking"],
    },
    homepage: "https://github.com/modelcontextprotocol/servers",
  },
  {
    id: "context7",
    name: "@upstash/context7-mcp",
    server: {
      type: "stdio",
      command: "npx",
      args: ["-y", "@upstash/context7-mcp"],
    },
    homepage: "https://context7.com",
  },
  {
    id: "filesystem",
    name: "Filesystem MCP",
    server: {
      type: "stdio",
      command: "npx",
      args: [
        "-y",
        "@modelcontextprotocol/server-filesystem@2026.8.31",
        "D:\\你要开放的目录",
      ],
    },
    homepage:
      "https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem",
  },
  {
    id: "git",
    name: "Git MCP",
    server: {
      type: "stdio",
      command: "uvx",
      args: ["mcp-server-git==2026.8.18", "--repository", "D:\\你的仓库"],
    },
    homepage:
      "https://github.com/modelcontextprotocol/servers/tree/main/src/git",
  },
];

const EMPTY: McpServer = {
  id: "",
  name: "",
  server: { type: "stdio", command: "", args: [] },
  targets: ["claude-code:relay"],
  description: null,
  homepage: null,
  tags: [],
};

function summary(s: McpServer): string {
  const spec = s.server as Record<string, unknown>;
  if (typeof spec.url === "string")
    return `${spec.type ?? "http"} · ${spec.url}`;
  const args = Array.isArray(spec.args) ? spec.args.join(" ") : "";
  return `${spec.command ?? ""} ${args}`.trim();
}

export default function McpPanel() {
  const toast = useToast();
  const [view, setView] = useState<McpView | null>(null);
  const [editing, setEditing] = useState<{
    server: McpServer;
    previous: string | null;
  } | null>(null);
  const [importFrom, setImportFrom] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void workspaceApi
      .mcpList()
      .then(setView)
      .catch((e: unknown) =>
        setError(e instanceof Error ? e.message : String(e)),
      );
  }, []);

  const run = async (what: () => Promise<McpView>, ok?: string) => {
    setBusy(true);
    setError(null);
    try {
      setView(await what());
      if (ok) toast.ok(ok);
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const targets = view?.targets ?? [];
  const byId = useMemo(
    () => new Map(targets.map((t) => [t.id, t] as const)),
    [targets],
  );

  if (!view)
    return error ? (
      <p className="notice notice--danger">{error}</p>
    ) : (
      <p className="qb-muted" role="status">
        正在读取 MCP…
      </p>
    );

  const shown = (s: McpServer) =>
    targets.filter((t) => t.available || s.targets.includes(t.id));

  const doImport = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await workspaceApi.mcpImport(importFrom);
      setView(r.view);
      toast.ok(
        `导入了 ${r.added.length} 条，${r.linked.length} 条勾上了这个目标` +
          (r.skipped.length ? `，${r.skipped.length} 条没导入` : ""),
      );
      if (r.skipped.length) setError(r.skipped.join("；"));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="qb-mcp">
      <p className="qb-muted">
        一条服务器写一次，勾上哪些地方就写进哪些配置。面板只增删它自己登记的那几条，你手写的别的
        MCP 不动；官方目录（标着「官方」）要你自己勾。连接里的 Key
        在面板的库里加密存。 改完要重开对应的客户端才生效。
      </p>
      <div className="qb-catalog-toolbar">
        <Button
          variant="primary"
          icon={<Plus size={16} />}
          onClick={() => setEditing({ server: { ...EMPTY }, previous: null })}
        >
          添加 MCP
        </Button>
        <span style={{ flex: 1 }} />
        <select
          aria-label="从哪里导入"
          value={importFrom}
          onChange={(e) => setImportFrom(e.target.value)}
        >
          <option value="">从已有配置导入…</option>
          {targets
            .filter((t) => t.available && t.entries.length > 0)
            .map((t) => (
              <option key={t.id} value={t.id}>
                {t.label}（{t.entries.length} 条）
              </option>
            ))}
        </select>
        <Button
          icon={<Upload size={16} />}
          disabled={!importFrom || busy}
          onClick={() => void doImport()}
        >
          导入
        </Button>
      </div>
      {error && <p className="notice notice--danger">{error}</p>}
      {view.servers.length === 0 ? (
        <p className="qb-muted">
          还没有 MCP。点「添加 MCP」挑一个常用的，或者从已有配置导入。
        </p>
      ) : (
        <div className="qb-mcp-list">
          {view.servers.map((s) => (
            <article key={s.id} className="qb-mcp-row">
              <div className="qb-mcp-head">
                <b>{s.name}</b> <code className="qb-muted">{s.id}</code>
                <span style={{ flex: 1 }} />
                <button
                  className="qb-icon-button"
                  aria-label={`编辑 ${s.id}`}
                  onClick={() => setEditing({ server: s, previous: s.id })}
                >
                  <Pencil size={15} />
                </button>
                <button
                  className="qb-icon-button"
                  aria-label={`删除 ${s.id}`}
                  disabled={busy}
                  onClick={() =>
                    void run(
                      () => workspaceApi.mcpDelete(s.id),
                      `删掉了 ${s.id}（勾着的配置里一起删了）`,
                    )
                  }
                >
                  <Trash2 size={15} />
                </button>
              </div>
              <p className="qb-muted qb-mcp-summary">{summary(s)}</p>
              <div
                className="qb-filter-pills"
                role="group"
                aria-label={`${s.id} 写进哪里`}
              >
                {shown(s).map((t) => {
                  const on = s.targets.includes(t.id);
                  const missing = on && !t.entries.includes(s.id);
                  return (
                    <button
                      key={t.id}
                      aria-pressed={on}
                      disabled={busy || (!on && !t.available)}
                      title={
                        missing
                          ? `${t.path} 里没有这一条（被手动删了？）—— 取消再勾上会重写`
                          : t.path
                      }
                      onClick={() =>
                        void run(() =>
                          workspaceApi.mcpSetTarget(s.id, t.id, !on),
                        )
                      }
                    >
                      {t.label}
                      {t.official && " · 官方"}
                      {missing && " ⚠"}
                    </button>
                  );
                })}
              </div>
            </article>
          ))}
        </div>
      )}
      <Modal
        open={editing != null}
        onClose={() => setEditing(null)}
        title={
          editing?.previous ? `编辑 MCP · ${editing.previous}` : "添加 MCP"
        }
        size="form"
      >
        {editing && (
          <McpEditor
            initial={editing.server}
            previous={editing.previous}
            targets={targets}
            busy={busy}
            byId={byId}
            onSave={async (server) => {
              if (
                await run(
                  () => workspaceApi.mcpSave(server, editing.previous),
                  `存好了：${server.id}`,
                )
              )
                setEditing(null);
            }}
          />
        )}
        {error && editing && <p className="notice notice--danger">{error}</p>}
      </Modal>
    </section>
  );
}

function McpEditor({
  initial,
  previous,
  targets,
  busy,
  byId,
  onSave,
}: {
  initial: McpServer;
  previous: string | null;
  targets: McpTarget[];
  busy: boolean;
  byId: Map<string, McpTarget>;
  onSave: (server: McpServer) => Promise<void>;
}) {
  const [id, setId] = useState(initial.id);
  const [name, setName] = useState(initial.name);
  const [text, setText] = useState(JSON.stringify(initial.server, null, 2));
  const [picked, setPicked] = useState<string[]>(initial.targets);
  const [problem, setProblem] = useState<string | null>(null);

  const usePreset = (presetId: string) => {
    const p = PRESETS.find((x) => x.id === presetId);
    if (!p) return;
    setId(p.id);
    setName(p.name);
    setText(JSON.stringify(p.server, null, 2));
  };

  const save = () => {
    let server: Record<string, unknown>;
    try {
      server = JSON.parse(text) as Record<string, unknown>;
    } catch (e) {
      setProblem(
        `连接定义不是合法的 JSON：${e instanceof Error ? e.message : String(e)}`,
      );
      return;
    }
    setProblem(null);
    void onSave({
      ...initial,
      id: id.trim(),
      name: name.trim() || id.trim(),
      server,
      targets: picked,
    });
  };

  return (
    <div className="qb-form">
      {!previous && (
        <label className="qb-field">
          <span>常用的</span>
          <select defaultValue="" onChange={(e) => usePreset(e.target.value)}>
            <option value="">自己填…</option>
            {PRESETS.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
      )}
      <label className="qb-field">
        <span>名字（写进配置的键）</span>
        <input
          value={id}
          onChange={(e) => setId(e.target.value)}
          placeholder="my-mcp"
        />
      </label>
      <label className="qb-field">
        <span>显示名</span>
        <input value={name} onChange={(e) => setName(e.target.value)} />
      </label>
      <label className="qb-field">
        <span>连接定义（type：stdio / http / sse）</span>
        <textarea
          className="qb-code-input"
          rows={9}
          aria-label="连接定义"
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      </label>
      <fieldset className="qb-field">
        <legend>写进哪里</legend>
        {targets
          .filter((t) => t.available || picked.includes(t.id))
          .map((t) => (
            <label key={t.id} className="qb-check">
              <input
                type="checkbox"
                checked={picked.includes(t.id)}
                onChange={(e) =>
                  setPicked(
                    e.target.checked
                      ? [...picked, t.id]
                      : picked.filter((x) => x !== t.id),
                  )
                }
              />
              {byId.get(t.id)?.label ?? t.id}
              {t.official && <span className="qb-muted"> · 官方</span>}
            </label>
          ))}
      </fieldset>
      {problem && <p className="notice notice--danger">{problem}</p>}
      <Button
        variant="primary"
        loading={busy}
        disabled={!id.trim()}
        onClick={save}
      >
        保存并写进勾选的配置
      </Button>
    </div>
  );
}
