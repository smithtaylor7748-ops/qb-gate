import { useState } from "react";
import {
  Link,
  useNavigate,
  useParams,
  useSearchParams,
} from "react-router-dom";
import {
  Copy,
  FlaskConical,
  KeyRound,
  Pencil,
  Play,
  Plus,
  Save,
  Star,
  Trash2,
  Waypoints,
} from "lucide-react";
import { Button, Modal } from "../ui";
import type { Preset } from "../lib/api";
import { R as RES } from "../lib/resources";
import { getSession, setSession, useResource, useSession } from "../lib/store";
import {
  CLIENT_NAMES,
  STATE_NAMES,
  useAction,
  useDiagnostics,
  useWorkspace,
  workspaceApi,
  type Credential,
  type Environment,
  type Provider,
  type ProbeReport,
  type ConfigPreview,
} from "../lib/workspace";

const blankProvider: Provider = {
  id: "",
  name: "",
  base_url: "",
  website: "",
  note: "",
  tags: [],
  favorite: false,
  revision: 0,
};
/**
 * 预设里那些**不属于服务商、属于使用环境**的字段。
 *
 * 服务商这一层只有名字和地址，装不下协议与模型；但预设是一整套配好的东西，
 * 拆开填一半、另一半让用户自己猜，等于没做。所以先记在会话里，
 * 创建环境时当默认值用。
 */
type PresetHint = Pick<Preset, "target" | "wire_api" | "auth_style"> & {
  model: string;
};

const hintKey = (providerId: string) => "relay.preset." + providerId;

const newEnvironment = (
  provider: Provider,
  hint?: PresetHint | null,
): Environment => ({
  id: "",
  name: "",
  client: hint?.target === "codex" ? "codex" : "claude-code",
  provider_id: provider.id,
  credential_id: null,
  model: hint?.model ?? "",
  small_model: "",
  wire_api: hint?.wire_api ?? "responses",
  auth_style: hint?.auth_style ?? "env_key",
  revision: 0,
  applied_revision: null,
  config_dir: "",
  config_state: "saved",
});

export default function RelayCenter() {
  const { id } = useParams();
  const navigate = useNavigate();
  const workspace = useWorkspace();
  const [filter, setFilter] = useSession("relay.filter", "");
  const [favorites, setFavorites] = useSession("relay.favorites", false);
  const [editing, setEditing] = useState(false);
  const providers = workspace.data?.providers ?? [];
  const selected =
    providers.find((p) => p.id === id) ??
    (id !== "new" ? providers[0] : undefined);
  const transfer = useAction();
  async function exportData() {
    const data = await transfer.run("export", workspaceApi.exportRelays);
    if (!data) return;
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }),
    );
    const a = document.createElement("a");
    a.href = url;
    a.download = "qb-gate-relays.redacted.json";
    a.click();
    URL.revokeObjectURL(url);
  }
  return (
    <>
      <div className="qb-inline-actions">
        <Button size="sm" onClick={() => void exportData()}>
          导出脱敏配置
        </Button>
        <label className="btn btn--sm">
          导入配置
          <input
            type="file"
            accept=".json"
            hidden
            onChange={(event) => {
              const file = event.target.files?.[0];
              if (file)
                void transfer.run(
                  "import",
                  async () => {
                    if (file.size > 2 * 1024 * 1024)
                      throw new Error("配置文件超过 2 MB");
                    return workspaceApi.importRelays(
                      JSON.parse(await file.text()),
                    );
                  },
                  "已导入为新环境，请重新填写 API 凭证",
                );
              event.target.value = "";
            }}
          />
        </label>
      </div>
      <header className="qb-page-heading">
        <div>
          <h1>中转站</h1>
          <p>服务商、API 凭证和独立使用环境，在一处管理。</p>
        </div>
        <Button
          variant="primary"
          icon={<Plus size={15} />}
          onClick={() => {
            navigate("/relays/new");
            setEditing(false);
          }}
        >
          添加服务商
        </Button>
      </header>
      {workspace.error && (
        <p className="notice notice--danger">{workspace.error}</p>
      )}
      <div className="qb-master-detail">
        <aside className="qb-object-list">
          <div className="qb-list-search">
            <input
              className="qb-input"
              aria-label="搜索中转站"
              placeholder="搜索名称、标签或地址…"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
            <button
              title="只看收藏"
              aria-pressed={favorites}
              onClick={() => setFavorites(!favorites)}
            >
              <Star size={17} fill={favorites ? "currentColor" : "none"} />
            </button>
          </div>
          {providers
            .filter(
              (p) =>
                (!favorites || p.favorite) &&
                `${p.name} ${p.base_url} ${p.tags.join(" ")}`
                  .toLowerCase()
                  .includes(filter.toLowerCase()),
            )
            .map((p) => (
              <Link
                className={selected?.id === p.id ? "selected" : ""}
                key={p.id}
                to={"/relays/" + p.id}
                onClick={() => setEditing(false)}
              >
                <span className="qb-avatar qb-avatar-orange">
                  <Waypoints size={19} />
                </span>
                <span>
                  <strong>{p.name}</strong>
                  <small>{p.base_url}</small>
                </span>
                {p.favorite && <Star size={12} />}
              </Link>
            ))}
          {!providers.length && (
            <p className="qb-list-note">
              添加第一家 API 服务商，然后配置凭证和使用环境。
            </p>
          )}
        </aside>
        <section className="qb-detail">
          {id === "new" || editing ? (
            <ProviderEditor
              key={editing ? selected?.id : "new"}
              provider={editing && selected ? selected : blankProvider}
              onSaved={(p) => {
                setEditing(false);
                navigate("/relays/" + p.id);
              }}
            />
          ) : selected ? (
            <ProviderDetails
              key={selected.id}
              provider={selected}
              onEdit={() => setEditing(true)}
            />
          ) : (
            <div className="qb-empty">
              <Waypoints size={32} />
              <h2>让每个 API 服务各就各位</h2>
              <p>支持多把凭证、多个模型环境和独立启动。</p>
              <Button variant="primary" onClick={() => navigate("/relays/new")}>
                添加第一个服务商
              </Button>
            </div>
          )}
        </section>
      </div>
    </>
  );
}

function ProviderEditor({
  provider,
  onSaved,
}: {
  provider: Provider;
  onSaved: (p: Provider) => void;
}) {
  const [draft, setDraft] = useSession<Provider>(
    "relay.provider.draft." + (provider.id || "new"),
    provider,
  );
  const action = useAction();
  const [tags, setTags] = useSession(
    "provider.tags." + (provider.id || "new"),
    provider.tags.join(", "),
  );
  const [hint, setHint] = useSession<PresetHint | null>(
    "relay.preset.draft." + (provider.id || "new"),
    null,
  );
  return (
    <>
      <h2>{provider.id ? "编辑服务商" : "添加服务商"}</h2>
      <p className="qb-muted">
        保存服务信息后，可以添加多把 API 凭证和使用环境。
      </p>
      <PresetPicker
        onPick={(p) => {
          setDraft({
            ...draft,
            name: p.name,
            base_url: p.base_url,
            website: p.website ?? "",
            note: p.note,
          });
          setHint({
            target: p.target,
            wire_api: p.wire_api,
            auth_style: p.auth_style,
            model: p.model ?? "",
          });
        }}
      />
      <form
        className="qb-form"
        onSubmit={(e) => {
          e.preventDefault();
          void action
            .run(
              "save",
              () =>
                workspaceApi.saveProvider({
                  ...draft,
                  tags: tags
                    .split(/[,，]/)
                    .map((t) => t.trim())
                    .filter(Boolean),
                }),
              "服务商已保存",
            )
            .then((p) => {
              if (p) {
                // 服务商这会儿才有 id，把预设里那半套挪到它名下 ——
                // 挑预设的时候还没有 id，没法直接存。
                if (hint) setSession(hintKey(p.id), hint);
                setDraft(provider.id ? p : blankProvider);
                if (!provider.id) setTags("");
                setHint(null);
                onSaved(p);
              }
            });
        }}
      >
        <label className="qb-field">
          <span>名称</span>
          <input
            required
            value={draft.name}
            placeholder="例如：工作 API"
            onChange={(e) => setDraft({ ...draft, name: e.target.value })}
          />
        </label>
        <label className="qb-field">
          <span>API 基础地址</span>
          <input
            required
            type="url"
            value={draft.base_url}
            placeholder="https://api.example.com/v1"
            onChange={(e) => setDraft({ ...draft, base_url: e.target.value })}
          />
        </label>
        <div className="qb-form-grid">
          <label className="qb-field">
            <span>服务网站</span>
            <input
              type="url"
              value={draft.website}
              onChange={(e) => setDraft({ ...draft, website: e.target.value })}
            />
          </label>
          <label className="qb-field">
            <span>标签 · 用逗号分隔</span>
            <input value={tags} onChange={(e) => setTags(e.target.value)} />
          </label>
        </div>
        <label className="qb-field">
          <span>备注</span>
          <textarea
            rows={3}
            value={draft.note}
            onChange={(e) => setDraft({ ...draft, note: e.target.value })}
          />
        </label>
        <label className="qb-check">
          <input
            type="checkbox"
            checked={draft.favorite}
            onChange={(e) => setDraft({ ...draft, favorite: e.target.checked })}
          />
          收藏此服务商
        </label>
        <Button
          type="submit"
          variant="primary"
          icon={<Save size={15} />}
          loading={!!action.pending}
        >
          保存服务商
        </Button>
        {action.error && (
          <p className="notice notice--danger">{action.error}</p>
        )}
      </form>
      <details className="qb-help-details">
        <summary>保存前测试草稿地址与凭证</summary>
        <DiagnosticsForm provider={draft} />
      </details>
    </>
  );
}

/**
 * 官方端点预设。
 *
 * v0.8.0 起 `relay_presets` 这个命令一直活着，却**没有任何入口** ——
 * 11 家厂商的公开直连地址躺在 `presets.rs` 里没人用得上，添加服务商只能手打
 * base_url，打错一个字符的症状是「连不上」，而不是「地址写错了」。
 */
function PresetPicker({ onPick }: { onPick: (p: Preset) => void }) {
  const presets = useResource("presets", RES.presets);
  if (!presets.data?.length) return null;
  return (
    <details className="qb-help-details">
      <summary>从官方端点预设填充 · {presets.data.length} 家</summary>
      <p className="qb-muted">
        只收厂商公开文档里的直连地址，这里不会出现任何第三方中转站 ——
        那些得你自己填地址。
      </p>
      <div className="qb-model-list">
        {presets.data.map((p) => (
          <button
            key={p.id}
            type="button"
            title={p.note}
            onClick={() => onPick(p)}
          >
            {p.name} · {p.target === "codex" ? "Codex" : "Claude"}
          </button>
        ))}
      </div>
    </details>
  );
}

function ProviderDetails({
  provider: p,
  onEdit,
}: {
  provider: Provider;
  onEdit: () => void;
}) {
  const workspace = useWorkspace();
  const action = useAction();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const tab = params.get("tab") ?? "environments";
  const [credential, setCredential] = useSession<Credential | null>(
    "relay.open.credential." + p.id,
    null,
  );
  const [environment, setEnvironment] = useSession<Environment | null>(
    "relay.open.environment." + p.id,
    null,
  );
  const [rollback, setRollback] = useState<string | null>(null);
  const [preview, setPreview] = useSession<{
    id: string;
    files: ConfigPreview[];
  } | null>("relay.open.preview." + p.id, null);
  const [deletion, setDeletion] = useState<{
    kind: string;
    id: string;
    name: string;
    refs: string[];
  } | null>(null);
  const credentials =
    workspace.data?.credentials.filter((c) => c.provider_id === p.id) ?? [];
  const environments =
    workspace.data?.environments.filter((e) => e.provider_id === p.id) ?? [];
  const selected =
    params.get("environment") === ""
      ? undefined
      : (environments.find((e) => e.id === params.get("environment")) ??
        environments[0]);
  async function askDelete(kind: string, id: string, name: string) {
    const refs = await action.run("references", () =>
      workspaceApi.references(kind, id),
    );
    if (refs) setDeletion({ kind, id, name, refs });
  }
  return (
    <>
      <div className="qb-detail-heading">
        <span className="qb-icon-tile">
          <Waypoints size={24} />
        </span>
        <div className="qb-grow">
          <h2>{p.name}</h2>
          <p className="qb-break">{p.base_url}</p>
        </div>
        <Button size="sm" icon={<Pencil size={14} />} onClick={onEdit}>
          编辑
        </Button>
        <Button
          size="sm"
          variant="ghost"
          aria-label="删除服务商"
          icon={<Trash2 size={14} />}
          onClick={() => void askDelete("providers", p.id, p.name)}
        />
      </div>
      <div className="qb-tags">
        {p.tags.map((t) => (
          <span className="qb-badge" key={t}>
            {t}
          </span>
        ))}
      </div>
      {p.note && <p className="qb-muted">{p.note}</p>}
      <div className="qb-tabs" role="tablist">
        {[
          ["environments", "使用环境", environments.length],
          ["credentials", "API 凭证", credentials.length],
          ["diagnostics", "连接诊断", ""],
        ].map(([id, label, count]) => (
          <button
            role="tab"
            aria-selected={tab === id}
            key={id}
            onClick={() =>
              setParams((prev) => {
                prev.set("tab", String(id));
                return prev;
              })
            }
          >
            {label}
            <span>{count}</span>
          </button>
        ))}
      </div>
      {tab === "credentials" && (
        <>
          <div className="qb-section-heading">
            <p className="qb-muted">
              凭证加密保存，可供此服务商的多个环境使用。
            </p>
            <Button
              icon={<Plus size={14} />}
              onClick={() =>
                setCredential({
                  id: "",
                  provider_id: p.id,
                  label: "",
                  masked: "",
                  available: false,
                  revision: 0,
                })
              }
            >
              添加凭证
            </Button>
          </div>
          {credentials.map((c) => (
            <article className="qb-environment-card" key={c.id}>
              <div className="qb-object-title">
                <KeyRound size={18} />
                <h3>{c.label}</h3>
                <span className="qb-badge">
                  {c.available ? "可用" : "需要填写"}
                </span>
              </div>
              <p className="qb-muted">{c.masked || "尚未配置"}</p>
              <div className="qb-inline-actions">
                <Button size="sm" onClick={() => setCredential(c)}>
                  管理凭证
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => void askDelete("credentials", c.id, c.label)}
                >
                  删除
                </Button>
              </div>
            </article>
          ))}
          {!credentials.length && (
            <div className="qb-empty">
              <KeyRound size={25} />
              <h3>还没有 API 凭证</h3>
              <p>添加后，再选择需要使用它的环境。</p>
            </div>
          )}
        </>
      )}
      {tab === "environments" && (
        <>
          <div className="qb-section-heading">
            <p className="qb-muted">每个环境使用独立的配置目录。</p>
            <Button
              icon={<Plus size={14} />}
              onClick={() =>
                setEnvironment(
                  newEnvironment(
                    p,
                    getSession<PresetHint | null>(hintKey(p.id), null),
                  ),
                )
              }
            >
              创建环境
            </Button>
          </div>
          {environments.map((e) => (
            <article key={e.id} className="qb-environment-card">
              <div className="qb-object-title">
                <span className="qb-avatar">
                  {e.client === "codex" ? "C" : "✳"}
                </span>
                <div className="qb-grow">
                  <h3>{e.name}</h3>
                  <p>
                    {CLIENT_NAMES[e.client]} · {e.model || "默认模型"}
                  </p>
                </div>
                <span
                  className={
                    "qb-badge " +
                    (e.config_state === "external" ? "qb-badge-warn" : "")
                  }
                >
                  {STATE_NAMES[e.config_state]}
                </span>
              </div>
              <p className="qb-muted">
                凭证：
                {credentials.find((c) => c.id === e.credential_id)?.label ??
                  "未选择"}
              </p>
              <div className="qb-inline-actions">
                <Button
                  variant="primary"
                  size="sm"
                  icon={<Play size={13} />}
                  disabled={!!action.pending}
                  onClick={() =>
                    void action.run(
                      e.id,
                      () => workspaceApi.launch(e.client, "relay", e.id),
                      "中转会话已启动",
                    )
                  }
                >
                  启动
                </Button>
                <Button size="sm" onClick={() => setEnvironment(e)}>
                  配置
                </Button>
                <Button
                  size="sm"
                  onClick={() =>
                    void action
                      .run("preview", () =>
                        workspaceApi.previewEnvironment(e.id),
                      )
                      .then((files) => {
                        if (files) setPreview({ id: e.id, files });
                      })
                  }
                >
                  预览与应用
                </Button>
                <Button
                  size="sm"
                  disabled={!!action.pending}
                  onClick={() => setRollback(e.id)}
                >
                  回滚
                </Button>
                <Button
                  size="sm"
                  icon={<FlaskConical size={13} />}
                  onClick={() =>
                    setParams({ tab: "diagnostics", environment: e.id })
                  }
                >
                  诊断
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  aria-label={"复制 " + e.name}
                  icon={<Copy size={13} />}
                  onClick={() =>
                    setEnvironment({
                      ...e,
                      id: "",
                      name: e.name + " · 副本",
                      revision: 0,
                      applied_revision: null,
                    })
                  }
                />
                <Button
                  variant="ghost"
                  size="sm"
                  aria-label={"删除 " + e.name}
                  icon={<Trash2 size={13} />}
                  onClick={() => void askDelete("environments", e.id, e.name)}
                />
              </div>
              <details>
                <summary>配置目录</summary>
                <code className="qb-break">{e.config_dir}</code>
              </details>
            </article>
          ))}
          {!environments.length && (
            <div className="qb-empty">
              <Waypoints size={25} />
              <h3>创建第一个使用环境</h3>
              <p>选择客户端、凭证和模型，即可独立启动。</p>
            </div>
          )}
        </>
      )}
      {tab === "diagnostics" && (
        <>
          <label className="qb-field">
            <span>测试哪个使用环境</span>
            <select
              value={selected?.id ?? ""}
              onChange={(e) =>
                setParams({ tab: "diagnostics", environment: e.target.value })
              }
            >
              <option value="">草稿连接</option>
              {environments.map((e) => (
                <option value={e.id} key={e.id}>
                  {e.name}
                </option>
              ))}
            </select>
          </label>
          <DiagnosticsForm
            key={selected?.id ?? "draft"}
            provider={p}
            environment={selected}
          />
        </>
      )}
      {action.error && (
        <p role="alert" className="notice notice--danger">
          {action.error}
        </p>
      )}
      <Modal
        open={!!credential}
        onClose={() => setCredential(null)}
        title={credential?.id ? "管理 API 凭证" : "添加 API 凭证"}
      >
        {credential && (
          <CredentialEditor
            key={credential.id}
            credential={credential}
            onSaved={() => setCredential(null)}
          />
        )}
      </Modal>
      <Modal
        open={!!environment}
        onClose={() => setEnvironment(null)}
        title={environment?.id ? "配置使用环境" : "创建使用环境"}
      >
        {environment && (
          <EnvironmentEditor
            key={environment.id + environment.name}
            environment={environment}
            credentials={credentials}
            onSaved={() => setEnvironment(null)}
          />
        )}
      </Modal>
      <Modal
        open={!!rollback}
        onClose={() => setRollback(null)}
        title="回滚环境配置"
        footer={
          <Button
            variant="primary"
            loading={!!action.pending}
            onClick={() => {
              if (rollback)
                void action
                  .run(
                    "rollback",
                    () => workspaceApi.rollbackEnvironment(rollback),
                    "环境配置已回滚",
                  )
                  .then((e) => {
                    if (e) setRollback(null);
                  });
            }}
          >
            恢复上一个版本
          </Button>
        }
      >
        <p>
          恢复上一次应用前的配置，并为当前版本保留恢复记录。现有会话继续使用启动时的配置，新会话使用恢复后的配置。
        </p>
      </Modal>
      <Modal
        open={!!preview}
        onClose={() => setPreview(null)}
        title="配置差异预览"
        footer={
          <Button
            variant="primary"
            loading={action.pending === "apply"}
            onClick={() => {
              if (preview)
                void action
                  .run(
                    "apply",
                    () =>
                      workspaceApi.applyEnvironment(
                        preview.id,
                        preview.files[0]?.fingerprint ?? "",
                      ),
                    "配置已应用",
                  )
                  .then((e) => {
                    if (e) setPreview(null);
                  });
            }}
          >
            应用到此环境
          </Button>
        }
      >
        {preview?.files.map((f) => (
          <section key={f.path}>
            <p className="qb-break">{f.path}</p>
            <div className="qb-diff">
              <div>
                <strong>当前</strong>
                <pre>{f.before || "文件尚不存在"}</pre>
              </div>
              <div>
                <strong>应用后</strong>
                <pre>{f.after}</pre>
              </div>
            </div>
          </section>
        ))}
      </Modal>
      <Modal
        open={!!deletion}
        onClose={() => setDeletion(null)}
        title={"删除 " + (deletion?.name ?? "")}
        footer={
          <Button
            variant="danger"
            disabled={!!deletion?.refs.length}
            loading={action.pending === "delete"}
            onClick={() => {
              if (deletion)
                void action
                  .run(
                    "delete",
                    () =>
                      workspaceApi
                        .remove(deletion.kind, deletion.id)
                        .then(() => true),
                    "记录已删除",
                  )
                  .then((ok) => {
                    if (ok) {
                      const provider = deletion.kind === "providers";
                      setDeletion(null);
                      if (provider) navigate("/relays");
                    }
                  });
            }}
          >
            删除记录
          </Button>
        }
      >
        {deletion?.refs.length ? (
          <>
            <p>先解除以下引用，再删除此记录：</p>
            <ul>
              {deletion.refs.map((r) => (
                <li key={r}>{r}</li>
              ))}
            </ul>
          </>
        ) : (
          <p>删除管理记录。已生成的环境目录和用户数据会保留。</p>
        )}
      </Modal>
    </>
  );
}

function CredentialEditor({
  credential,
  onSaved,
}: {
  credential: Credential;
  onSaved: () => void;
}) {
  const [draft, setDraft] = useSession(
    "credential.draft." + (credential.id || credential.provider_id),
    credential,
  );
  const [key, setKey] = useSession(
    "credential.secret." + (credential.id || credential.provider_id),
    "",
  );
  const [mode, setMode] = useSession(
    "credential.mode." + (credential.id || credential.provider_id),
    credential.id ? "keep" : "replace",
  );
  const action = useAction();
  return (
    <form
      className="qb-form"
      onSubmit={(e) => {
        e.preventDefault();
        void action
          .run(
            "save",
            () =>
              workspaceApi.saveCredential(
                draft,
                mode,
                mode === "replace" ? key : undefined,
              ),
            "凭证已保存",
          )
          .then((c) => {
            if (c) {
              setKey("");
              setDraft(credential.id ? c : credential);
              onSaved();
            }
          });
      }}
    >
      <label className="qb-field">
        <span>凭证名称</span>
        <input
          required
          value={draft.label}
          onChange={(e) => setDraft({ ...draft, label: e.target.value })}
        />
      </label>
      {credential.id && (
        <label className="qb-field">
          <span>凭证操作</span>
          <select value={mode} onChange={(e) => setMode(e.target.value)}>
            <option value="keep">保留现有 Key</option>
            <option value="replace">替换 Key</option>
            <option value="clear">清除 Key</option>
          </select>
        </label>
      )}
      {mode === "replace" && (
        <label className="qb-field">
          <span>API Key</span>
          <input
            autoComplete="off"
            type="password"
            required
            value={key}
            onChange={(e) => setKey(e.target.value)}
          />
        </label>
      )}
      {mode === "clear" && (
        <p className="notice">使用此凭证的环境需要重新配置后才能启动。</p>
      )}
      <Button type="submit" variant="primary" loading={!!action.pending}>
        保存凭证
      </Button>
      {action.error && <p className="notice notice--danger">{action.error}</p>}
    </form>
  );
}
function EnvironmentEditor({
  environment,
  credentials,
  onSaved,
}: {
  environment: Environment;
  credentials: Credential[];
  onSaved: () => void;
}) {
  const [draft, setDraft] = useSession<Environment>(
    "environment.draft." +
      (environment.id || environment.provider_id + environment.name),
    environment,
  );
  const action = useAction();
  return (
    <form
      className="qb-form"
      onSubmit={(e) => {
        e.preventDefault();
        void action
          .run(
            "save",
            () => workspaceApi.saveEnvironment(draft),
            "使用环境已保存",
          )
          .then((e) => {
            if (e) {
              setDraft(environment.id ? e : environment);
              onSaved();
            }
          });
      }}
    >
      <label className="qb-field">
        <span>环境名称</span>
        <input
          required
          placeholder="例如：Claude · 工作"
          value={draft.name}
          onChange={(e) => setDraft({ ...draft, name: e.target.value })}
        />
      </label>
      <div className="qb-form-grid">
        <label className="qb-field">
          <span>客户端</span>
          <select
            value={draft.client}
            onChange={(e) =>
              setDraft({
                ...draft,
                client: e.target.value as Environment["client"],
              })
            }
          >
            <option value="claude-code">Claude Code</option>
            <option value="codex">Codex</option>
          </select>
        </label>
        <label className="qb-field">
          <span>API 凭证</span>
          <select
            value={draft.credential_id ?? ""}
            onChange={(e) =>
              setDraft({ ...draft, credential_id: e.target.value || null })
            }
          >
            <option value="">选择凭证</option>
            {credentials.map((c) => (
              <option value={c.id} key={c.id}>
                {c.label}
                {c.available ? "" : "（需要填写）"}
              </option>
            ))}
          </select>
        </label>
      </div>
      <label className="qb-field">
        <span>默认模型</span>
        <input
          value={draft.model}
          onChange={(e) => setDraft({ ...draft, model: e.target.value })}
          placeholder="模型 ID；可从连接诊断获取"
        />
      </label>
      {draft.client === "claude-code" && (
        <label className="qb-field">
          <span>快速模型 · 可选</span>
          <input
            value={draft.small_model}
            onChange={(e) =>
              setDraft({ ...draft, small_model: e.target.value })
            }
          />
        </label>
      )}
      <div className="qb-form-grid">
        <label className="qb-field">
          <span>认证方式</span>
          <select
            value={draft.auth_style}
            onChange={(e) => setDraft({ ...draft, auth_style: e.target.value })}
          >
            <option value="env_key">API Key</option>
            <option value="bearer_token">Bearer Token</option>
            <option value="none">无认证</option>
          </select>
        </label>
        <label className="qb-field">
          <span>协议</span>
          <select
            value={draft.wire_api}
            onChange={(e) => setDraft({ ...draft, wire_api: e.target.value })}
          >
            <option value="responses">
              {draft.client === "claude-code"
                ? "Anthropic Messages"
                : "OpenAI Responses"}
            </option>
            {draft.client === "codex" && (
              <option value="chat">Chat Completions · 仅诊断</option>
            )}
          </select>
        </label>
      </div>
      <Button type="submit" variant="primary" loading={!!action.pending}>
        保存使用环境
      </Button>
      {action.error && <p className="notice notice--danger">{action.error}</p>}
    </form>
  );
}

function DiagnosticsForm({
  provider,
  environment,
}: {
  provider: Provider;
  environment?: Environment;
}) {
  const action = useAction();
  const history = useDiagnostics();
  const [draft, setDraft] = useSession(
    "diagnostic.draft." + ((environment?.id ?? provider.id) || "new"),
    {
      base: provider.base_url,
      key: "",
      model: environment?.model ?? "",
      client: environment?.client ?? "claude-code",
      wire: environment?.wire_api ?? "responses",
      call: false,
      stream: false,
      tools: false,
    },
  );
  const [testedDraft, setTestedDraft] = useSession(
    "diagnostic.tested." + ((environment?.id ?? provider.id) || "new"),
    "",
  );
  const [report, setReport] = useSession<ProbeReport | null>(
    "diagnostic.result." + ((environment?.id ?? provider.id) || "new"),
    null,
  );
  // 历史记录按环境查。草稿诊断（没有环境）在库里 environment_id 是 null，
  // 分不出属于哪个服务商，所以只认本次运行的结果 —— 它活在会话态里，切页还在。
  const shown =
    report ??
    (environment
      ? history.data?.find((r) => r.environment_id === environment.id)
      : undefined);
  function download() {
    if (!shown) return;
    const a = document.createElement("a");
    const url = URL.createObjectURL(
      new Blob([shown.request_export], { type: "text/plain;charset=utf-8" }),
    );
    a.href = url;
    a.download = "qb-gate-diagnostic.bru";
    a.click();
    URL.revokeObjectURL(url);
  }
  return (
    <div className="qb-form">
      {!environment && (
        <div className="qb-form-grid">
          <label className="qb-field">
            <span>客户端协议</span>
            <select
              value={draft.client}
              onChange={(e) =>
                setDraft({
                  ...draft,
                  client: e.target.value as Environment["client"],
                  wire: "responses",
                })
              }
            >
              <option value="claude-code">Anthropic Messages</option>
              <option value="codex">OpenAI</option>
            </select>
          </label>
          {draft.client === "codex" && (
            <label className="qb-field">
              <span>API 协议</span>
              <select
                value={draft.wire}
                onChange={(e) => setDraft({ ...draft, wire: e.target.value })}
              >
                <option value="responses">Responses</option>
                <option value="chat">Chat Completions</option>
              </select>
            </label>
          )}
        </div>
      )}
      <div className="qb-form-grid">
        <label className="qb-field">
          <span>测试地址</span>
          <input
            value={draft.base}
            onChange={(e) => setDraft({ ...draft, base: e.target.value })}
          />
        </label>
        <label className="qb-field">
          <span>
            {environment?.credential_id
              ? "草稿 Key · 留空使用所选凭证"
              : "草稿 API Key"}
          </span>
          <input
            type="password"
            autoComplete="off"
            value={draft.key}
            onChange={(e) => setDraft({ ...draft, key: e.target.value })}
          />
        </label>
      </div>
      <label className="qb-field">
        <span>测试模型</span>
        <input
          list="diagnostic-models"
          value={draft.model}
          onChange={(e) => setDraft({ ...draft, model: e.target.value })}
          placeholder="先获取模型列表，或填写模型 ID"
        />
        <datalist id="diagnostic-models">
          {shown?.models.map((m) => (
            <option key={m} value={m} />
          ))}
        </datalist>
      </label>
      <div className="qb-checkbox-row">
        <label>
          <input
            type="checkbox"
            checked={draft.call}
            onChange={(e) => setDraft({ ...draft, call: e.target.checked })}
          />
          实际模型调用
        </label>
        <label>
          <input
            type="checkbox"
            checked={draft.stream}
            onChange={(e) => setDraft({ ...draft, stream: e.target.checked })}
          />
          流式输出
        </label>
        <label>
          <input
            type="checkbox"
            checked={draft.tools}
            onChange={(e) => setDraft({ ...draft, tools: e.target.checked })}
          />
          工具调用
        </label>
      </div>
      <p className="qb-muted">
        默认检查连接与模型目录。勾选模型测试会发送实际请求，可能产生费用。
      </p>
      <div className="qb-inline-actions">
        <Button
          variant="primary"
          icon={<FlaskConical size={15} />}
          loading={!!action.pending}
          onClick={() =>
            void action
              .run("diagnose", () =>
                workspaceApi.diagnose({
                  auth_style: environment?.auth_style ?? "env_key",
                  base_url: draft.base,
                  client: draft.client,
                  credential_id: environment?.credential_id ?? null,
                  draft_key: draft.key || null,
                  model: draft.model,
                  wire_api: draft.wire,
                  environment_id: environment?.id ?? null,
                  revision: environment?.revision ?? 0,
                  test_call: draft.call,
                  test_stream: draft.stream,
                  test_tools: draft.tools,
                }),
              )
              .then((r) => {
                if (r) {
                  setReport(r);
                  setTestedDraft(JSON.stringify({ ...draft, key: "" }));
                  setDraft({ ...draft, key: "" });
                }
              })
          }
        >
          运行诊断
        </Button>
        {shown?.request_export && (
          <Button onClick={download}>导出脱敏请求</Button>
        )}
      </div>
      {action.error && <p className="notice notice--danger">{action.error}</p>}
      {shown && (
        <section className="qb-diagnostic-report">
          <div className="qb-section-heading">
            <h3>诊断结果</h3>
            <small>
              {new Date(shown.checked_at).toLocaleString()}
              {(environment && shown.revision !== environment.revision) ||
              (report && testedDraft !== JSON.stringify(draft))
                ? " · 配置或草稿已更新，结果过期"
                : ""}
            </small>
          </div>
          {shown.checks.map((c, i) => (
            <div className="qb-check-row" key={i}>
              <span
                className={
                  "qb-badge " + (c.status === "failed" ? "qb-badge-warn" : "")
                }
              >
                {
                  (
                    {
                      passed: "通过",
                      failed: "失败",
                      unknown: "待验证",
                      skipped: "未运行",
                    } as Record<string, string>
                  )[c.status]
                }
              </span>
              <div>
                <strong>{c.name}</strong>
                <p>{c.detail}</p>
              </div>
              <small>{c.elapsed_ms} ms</small>
            </div>
          ))}
          <details>
            <summary>后端来源线索</summary>
            {shown.evidence.map((e, i) => (
              <p className="qb-muted" key={i}>
                {e}
              </p>
            ))}
          </details>
          {shown.models.length > 0 && (
            <details>
              <summary>模型目录 · {shown.models.length}</summary>
              <div className="qb-model-list">
                {shown.models.map((m) => (
                  <button
                    key={m}
                    onClick={() => setDraft({ ...draft, model: m })}
                  >
                    {m}
                  </button>
                ))}
              </div>
            </details>
          )}
        </section>
      )}
    </div>
  );
}
