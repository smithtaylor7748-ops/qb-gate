/**
 * 提示词库（中转站重构阶段 5，cc-switch 的 M3）：一库提示词，每个目标挑一条写进它的提示词文件
 * （Claude Code 的 `CLAUDE.md`、Codex / 其他工具的 `AGENTS.md`、Gemini CLI 的 `GEMINI.md`、Hermes 的 `SOUL.md`）。
 *
 * 写进哪一份由使用者选：中转环境、官方目录（标着「官方」）、各个工具各挑各的。换之前文件里原来的内容会先存回库里，
 * 换来换去不会丢东西。
 */

import { useEffect, useState } from "react";
import { Pencil, Plus, Trash2 } from "lucide-react";

import type { Prompt } from "../lib/generated/Prompt";
import type { PromptsView } from "../lib/generated/PromptsView";
import { workspaceApi } from "../lib/workspace";
import { Button, Modal, useToast } from "../ui";

const EMPTY: Prompt = {
  id: "",
  name: "",
  content: "",
  description: null,
  created_ms: 0,
  updated_ms: 0,
};

export default function PromptsPanel() {
  const toast = useToast();
  const [view, setView] = useState<PromptsView | null>(null);
  const [editing, setEditing] = useState<Prompt | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void workspaceApi
      .promptsList()
      .then(setView)
      .catch((e: unknown) =>
        setError(e instanceof Error ? e.message : String(e)),
      );
  }, []);

  const run = async (what: () => Promise<PromptsView>, ok?: string) => {
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

  if (!view)
    return error ? (
      <p className="notice notice--danger">{error}</p>
    ) : (
      <p className="qb-muted" role="status">
        正在读取提示词…
      </p>
    );

  const nameOf = (id: string | null) =>
    view.prompts.find((p) => p.id === id)?.name ?? null;

  return (
    <section className="qb-mcp">
      <p className="qb-muted">
        每个地方挑一条提示词，整份写进它的提示词文件。换之前那份文件原来的内容会先存回库里（第一次是一条「原始提示词」），
        换来换去不会丢。官方目录（标着「官方」）要你自己挑。
      </p>
      {error && <p className="notice notice--danger">{error}</p>}
      <div className="qb-mcp-list">
        {view.targets
          .filter((t) => t.available)
          .map((t) => (
            <article key={t.id} className="qb-mcp-row">
              <div className="qb-mcp-head">
                <b>{t.label}</b>
                {t.official && <span className="qb-muted">官方</span>}
                <span style={{ flex: 1 }} />
                <select
                  aria-label={`${t.label} 用哪一条`}
                  value={t.active ?? ""}
                  disabled={busy || view.prompts.length === 0}
                  onChange={(e) =>
                    e.target.value &&
                    void run(
                      () => workspaceApi.promptsEnable(t.id, e.target.value),
                      `${t.label} 换成了「${nameOf(e.target.value)}」`,
                    )
                  }
                >
                  <option value="">
                    {t.active ? "（已删除）" : "还没用面板的"}
                  </option>
                  {view.prompts.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
                {t.chars != null && t.chars > 0 && (
                  <Button
                    disabled={busy}
                    onClick={() =>
                      void run(
                        () => workspaceApi.promptsImport(t.id),
                        "导进了库里",
                      )
                    }
                  >
                    导入当前文件
                  </Button>
                )}
              </div>
              <p className="qb-muted qb-mcp-summary">
                {t.path}
                {t.chars == null ? " · 还没有这个文件" : ` · ${t.chars} 字`}
                {t.drift && " · ⚠ 文件被手动改过，下次换的时候会先存回那一条"}
              </p>
            </article>
          ))}
      </div>
      <div className="qb-catalog-toolbar">
        <b>提示词库</b>
        <span style={{ flex: 1 }} />
        <Button
          variant="primary"
          icon={<Plus size={16} />}
          onClick={() => setEditing({ ...EMPTY })}
        >
          新建提示词
        </Button>
      </div>
      {view.prompts.length === 0 ? (
        <p className="qb-muted">
          库里还没有提示词。新建一条，或者从上面某个地方「导入当前文件」。
        </p>
      ) : (
        <div className="qb-mcp-list">
          {view.prompts.map((p) => {
            const using = view.targets.filter((t) => t.active === p.id);
            return (
              <article key={p.id} className="qb-mcp-row">
                <div className="qb-mcp-head">
                  <b>{p.name}</b>
                  {using.length > 0 && (
                    <span className="qb-muted">
                      用在 {using.map((t) => t.label).join("、")}
                    </span>
                  )}
                  <span style={{ flex: 1 }} />
                  <button
                    className="qb-icon-button"
                    aria-label={`编辑 ${p.name}`}
                    onClick={() => setEditing(p)}
                  >
                    <Pencil size={15} />
                  </button>
                  <button
                    className="qb-icon-button"
                    aria-label={`删除 ${p.name}`}
                    disabled={busy || using.length > 0}
                    title={
                      using.length > 0 ? "正在用着：先换成别的" : undefined
                    }
                    onClick={() =>
                      void run(() => workspaceApi.promptsDelete(p.id), "删掉了")
                    }
                  >
                    <Trash2 size={15} />
                  </button>
                </div>
                {p.description && (
                  <p className="qb-muted qb-mcp-summary">{p.description}</p>
                )}
              </article>
            );
          })}
        </div>
      )}
      <Modal
        open={editing != null}
        onClose={() => setEditing(null)}
        title={editing?.id ? `编辑 · ${editing.name}` : "新建提示词"}
        size="form"
      >
        {editing && (
          <PromptEditor
            initial={editing}
            busy={busy}
            onSave={async (p) => {
              if (await run(() => workspaceApi.promptsSave(p), "存好了"))
                setEditing(null);
            }}
          />
        )}
        {error && editing && <p className="notice notice--danger">{error}</p>}
      </Modal>
    </section>
  );
}

function PromptEditor({
  initial,
  busy,
  onSave,
}: {
  initial: Prompt;
  busy: boolean;
  onSave: (p: Prompt) => Promise<void>;
}) {
  const [name, setName] = useState(initial.name);
  const [description, setDescription] = useState(initial.description ?? "");
  const [content, setContent] = useState(initial.content);
  return (
    <div className="qb-form">
      <label className="qb-field">
        <span>名字</span>
        <input value={name} onChange={(e) => setName(e.target.value)} />
      </label>
      <label className="qb-field">
        <span>说明（可空）</span>
        <input
          value={description}
          onChange={(e) => setDescription(e.target.value)}
        />
      </label>
      <label className="qb-field">
        <span>内容（Markdown，整份写进提示词文件）</span>
        <textarea
          className="qb-code-input"
          rows={14}
          aria-label="提示词内容"
          value={content}
          onChange={(e) => setContent(e.target.value)}
        />
      </label>
      {initial.id && (
        <p className="qb-muted">
          正在用这一条的地方会跟着改；那份文件被手动改过的话，改动先另存一条。
        </p>
      )}
      <Button
        variant="primary"
        loading={busy}
        disabled={!name.trim()}
        onClick={() =>
          void onSave({
            ...initial,
            name: name.trim(),
            description: description.trim() || null,
            content,
          })
        }
      >
        保存
      </Button>
    </div>
  );
}
