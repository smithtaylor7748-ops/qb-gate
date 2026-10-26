/**
 * Skills 面板（中转站重构阶段 5，cc-switch 的 M2）：装一次，勾上哪些目标就复制到哪些 `skills` 目录。
 *
 * 照 cc-switch 的 Skills 管理：已装的一行一个、每行一排开关；「添加」可以填 GitHub 地址 / 本地目录、选 ZIP、搜 skills.sh。
 * 来源只读元数据、只下文件，**不执行仓库里的任何脚本**。取消勾选、卸载、更新换下来的都挪进备份，卸载的能还原；
 * 目标里已经有同名、不是面板装的 Skill 不覆盖 —— 用「导入」让面板接管。
 */

import { useEffect, useState } from "react";
import { RefreshCw, Search, Trash2, Upload } from "lucide-react";

import type { SkillCandidate } from "../lib/generated/SkillCandidate";
import type { SkillsShHit } from "../lib/generated/SkillsShHit";
import type { SkillsView } from "../lib/generated/SkillsView";
import type { SkillUpdate } from "../lib/generated/SkillUpdate";
import { useSession } from "../lib/store";
import { workspaceApi } from "../lib/workspace";
import { Button, Modal, PathField, useToast } from "../ui";

export default function SkillsPanel() {
  const toast = useToast();
  const [view, setView] = useState<SkillsView | null>(null);
  const [adding, setAdding] = useState(false);
  const [updates, setUpdates] = useState<SkillUpdate[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void workspaceApi
      .skillsList()
      .then(setView)
      .catch((e: unknown) =>
        setError(e instanceof Error ? e.message : String(e)),
      );
  }, []);

  const run = async (what: () => Promise<SkillsView>, ok?: string) => {
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
        正在读取 Skills…
      </p>
    );

  const label = (id: string) =>
    view.targets.find((t) => t.id === id)?.label ?? id;

  return (
    <section className="qb-mcp">
      <p className="qb-muted">
        Skill
        本体放在面板自己的目录里一份，勾上哪些地方就复制过去；官方目录（标着「官方」）要你自己勾。
        取消勾选、卸载换下来的都挪进备份，不直接删。只读取元数据、下载文件，不执行仓库里的脚本。
      </p>
      <div className="qb-catalog-toolbar">
        <Button variant="primary" onClick={() => setAdding(true)}>
          添加 Skill
        </Button>
        <span style={{ flex: 1 }} />
        <Button
          icon={<RefreshCw size={16} />}
          disabled={busy || view.skills.length === 0}
          onClick={() => {
            setBusy(true);
            setError(null);
            void workspaceApi
              .skillsCheckUpdates()
              .then((u) => {
                setUpdates(u);
                toast.ok(u.length ? `${u.length} 个有新版` : "都是最新的");
              })
              .catch((e: unknown) =>
                setError(e instanceof Error ? e.message : String(e)),
              )
              .finally(() => setBusy(false));
          }}
        >
          检查更新
        </Button>
      </div>
      {error && <p className="notice notice--danger">{error}</p>}
      {view.skills.length === 0 ? (
        <p className="qb-muted">
          还没有装 Skill。点「添加 Skill」，或者从下面「没管着的」导入。
        </p>
      ) : (
        <div className="qb-mcp-list">
          {view.skills.map((s) => {
            const update = updates?.find((u) => u.id === s.id);
            return (
              <article key={s.id} className="qb-mcp-row">
                <div className="qb-mcp-head">
                  <b>{s.id}</b>
                  <span className="qb-muted">{s.description}</span>
                  <span style={{ flex: 1 }} />
                  {update && (
                    <Button
                      disabled={busy}
                      onClick={() =>
                        void run(
                          () => workspaceApi.skillsUpdate(s.id),
                          `${s.id} 换成了新的一版`,
                        ).then(
                          (ok) =>
                            ok &&
                            setUpdates(
                              (u) => u?.filter((x) => x.id !== s.id) ?? null,
                            ),
                        )
                      }
                    >
                      更新
                    </Button>
                  )}
                  <button
                    className="qb-icon-button"
                    aria-label={`卸载 ${s.id}`}
                    disabled={busy}
                    onClick={() =>
                      void run(
                        () => workspaceApi.skillsUninstall(s.id),
                        `卸载了 ${s.id}（文件挪进了备份，能还原）`,
                      )
                    }
                  >
                    <Trash2 size={15} />
                  </button>
                </div>
                <p className="qb-muted qb-mcp-summary">{s.source}</p>
                <div
                  className="qb-filter-pills"
                  role="group"
                  aria-label={`${s.id} 放到哪里`}
                >
                  {view.targets
                    .filter((t) => t.available || s.targets.includes(t.id))
                    .map((t) => {
                      const on = s.targets.includes(t.id);
                      return (
                        <button
                          key={t.id}
                          aria-pressed={on}
                          disabled={busy || (!on && !t.available)}
                          title={t.path}
                          onClick={() =>
                            void run(() =>
                              workspaceApi.skillsSetTarget(s.id, t.id, !on),
                            )
                          }
                        >
                          {t.label}
                          {t.official && " · 官方"}
                        </button>
                      );
                    })}
                </div>
              </article>
            );
          })}
        </div>
      )}
      {view.unmanaged.length > 0 && (
        <details className="qb-st-adv">
          <summary>没管着的 · {view.unmanaged.length} 个</summary>
          <p className="qb-muted">
            这些 Skill
            在各个目录里，但不是面板装的。导入后面板复制一份管起来，原来那份不动。
          </p>
          <ul className="qb-help-note">
            {view.unmanaged.map((u) => (
              <li key={`${u.target}/${u.directory}`}>
                <code>{u.directory}</code> · {label(u.target)}{" "}
                <Button
                  disabled={busy}
                  onClick={() =>
                    void run(
                      () => workspaceApi.skillsImport(u.target, u.directory),
                      `导入了 ${u.directory}`,
                    )
                  }
                >
                  导入
                </Button>
              </li>
            ))}
          </ul>
        </details>
      )}
      {view.backups.length > 0 && (
        <details className="qb-st-adv">
          <summary>卸载的备份 · {view.backups.length} 份</summary>
          <ul className="qb-help-note">
            {view.backups.map((b) => (
              <li key={b.id}>
                <code>{b.skill}</code> · {new Date(b.at_ms).toLocaleString()}{" "}
                <Button
                  disabled={busy}
                  onClick={() =>
                    void run(
                      () => workspaceApi.skillsRestore(b.id),
                      `还原了 ${b.skill}（目标要你再勾）`,
                    )
                  }
                >
                  还原
                </Button>
              </li>
            ))}
          </ul>
        </details>
      )}
      <Modal
        open={adding}
        onClose={() => setAdding(false)}
        title="添加 Skill"
        size="form"
      >
        {adding && (
          <SkillAdder
            view={view}
            onDone={(v) => {
              setView(v);
              setAdding(false);
            }}
          />
        )}
      </Modal>
    </section>
  );
}

function SkillAdder({
  view,
  onDone,
}: {
  view: SkillsView;
  onDone: (view: SkillsView) => void;
}) {
  const toast = useToast();
  const [mode, setMode] = useState<"source" | "zip" | "search">("source");
  // 草稿存在会话态里：关掉小窗、切到别的页再回来都还在（跟原来「手动导入」一样）。
  const [source, setSource] = useSession("skills.add.source", "");
  const [zip, setZip] = useState("");
  const [query, setQuery] = useSession("skills.add.query", "");
  const [hits, setHits] = useState<SkillsShHit[] | null>(null);
  const [found, setFound] = useState<SkillCandidate[] | null>(null);
  const [picked, setPicked] = useState<string[]>([]);
  const [targets, setTargets] = useState<string[]>(["claude-code:relay"]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const go = async <T,>(what: () => Promise<T>, then: (v: T) => void) => {
    setBusy(true);
    setError(null);
    try {
      then(await what());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  const scan = (src: string, only?: string) =>
    void go(
      () => workspaceApi.skillsDiscover(src),
      (c) => {
        const list = only ? c.filter((x) => x.name === only) : c;
        setFound(list.length ? list : c);
        setPicked((list.length ? list : c).map((x) => x.name));
      },
    );
  const installed = new Set(view.skills.map((s) => s.id));

  return (
    <div className="qb-form">
      <div className="qb-filter-pills" role="tablist">
        {(
          [
            ["source", "GitHub / 本地目录"],
            ["zip", "ZIP"],
            ["search", "搜 skills.sh"],
          ] as const
        ).map(([k, n]) => (
          <button
            key={k}
            role="tab"
            aria-pressed={mode === k}
            onClick={() => {
              setMode(k);
              setFound(null);
            }}
          >
            {n}
          </button>
        ))}
      </div>
      {mode === "source" && (
        <label className="qb-field">
          <span>GitHub 仓库 / 子目录地址，或本地目录</span>
          <div className="qb-st-inline">
            <input
              value={source}
              onChange={(e) => setSource(e.target.value)}
              placeholder="https://github.com/owner/repo/tree/main/skills"
            />
            <Button
              disabled={busy || !source.trim()}
              onClick={() => scan(source)}
            >
              扫描
            </Button>
          </div>
        </label>
      )}
      {mode === "zip" && (
        <>
          <PathField
            label="ZIP 文件"
            kind="file"
            value={zip}
            onChange={setZip}
          />
          <Button
            icon={<Upload size={16} />}
            disabled={busy || !zip}
            onClick={() =>
              void go(
                () => workspaceApi.skillsDiscoverZip(zip),
                (c) => {
                  setFound(c);
                  setPicked(c.map((x) => x.name));
                },
              )
            }
          >
            读这个 ZIP
          </Button>
        </>
      )}
      {mode === "search" && (
        <>
          <label className="qb-field">
            <span>在 skills.sh 上搜（会联网）</span>
            <div className="qb-st-inline">
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="比如 pdf、testing"
              />
              <Button
                icon={<Search size={16} />}
                disabled={busy || !query.trim()}
                onClick={() =>
                  void go(() => workspaceApi.skillsSearch(query), setHits)
                }
              >
                搜
              </Button>
            </div>
          </label>
          {hits && (
            <ul className="qb-help-note">
              {hits.length === 0 && <li>没搜到。</li>}
              {hits.map((h) => (
                <li key={`${h.owner}/${h.repo}/${h.skill}`}>
                  <b>{h.name}</b> · {h.owner}/{h.repo} · {h.installs} 次安装{" "}
                  <Button disabled={busy} onClick={() => scan(h.url, h.skill)}>
                    看看
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
      {found && (
        <fieldset className="qb-field">
          <legend>找到的 Skill</legend>
          {found.map((c) => (
            <label key={c.name} className="qb-check">
              <input
                type="checkbox"
                disabled={installed.has(c.name)}
                checked={picked.includes(c.name) && !installed.has(c.name)}
                onChange={(e) =>
                  setPicked(
                    e.target.checked
                      ? [...picked, c.name]
                      : picked.filter((x) => x !== c.name),
                  )
                }
              />
              <b>{c.name}</b>
              <span className="qb-muted">
                {" "}
                · {c.description} · {c.license}
                {installed.has(c.name) && " · 已经装了"}
              </span>
            </label>
          ))}
        </fieldset>
      )}
      {found && (
        <fieldset className="qb-field">
          <legend>放到哪里</legend>
          {view.targets
            .filter((t) => t.available)
            .map((t) => (
              <label key={t.id} className="qb-check">
                <input
                  type="checkbox"
                  checked={targets.includes(t.id)}
                  onChange={(e) =>
                    setTargets(
                      e.target.checked
                        ? [...targets, t.id]
                        : targets.filter((x) => x !== t.id),
                    )
                  }
                />
                {t.label}
                {t.official && <span className="qb-muted"> · 官方</span>}
              </label>
            ))}
        </fieldset>
      )}
      {error && <p className="notice notice--danger">{error}</p>}
      {found && (
        <Button
          variant="primary"
          loading={busy}
          disabled={picked.filter((n) => !installed.has(n)).length === 0}
          onClick={() => {
            const names = picked.filter((n) => !installed.has(n));
            void go(
              () =>
                mode === "zip"
                  ? workspaceApi.skillsInstallZip(zip, names, targets)
                  : workspaceApi.skillsInstall(
                      found.filter((c) => names.includes(c.name)),
                      targets,
                    ),
              (v) => {
                toast.ok(`装好了 ${names.length} 个 Skill`);
                onDone(v);
              },
            );
          }}
        >
          装上并复制到勾选的地方
        </Button>
      )}
    </div>
  );
}
