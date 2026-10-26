import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import {
  ArrowLeft,
  ArrowRight,
  Blocks,
  Download,
  FolderInput,
  Plug,
  Search,
  Sparkles,
} from "lucide-react";
import { Button, ExternalLink, Modal } from "../ui";
import { useResource, useSession } from "../lib/store";
import { R } from "../lib/resources";
import {
  CLIENT_NAMES,
  KIND_NAMES,
  STATE_NAMES,
  useAction,
  useCatalog,
  useWorkspace,
  workspaceApi,
  type Client,
  type ExtensionManifest,
  type IdentityKind,
  type InstallPreview,
  type InstallRequest,
} from "../lib/workspace";
import CodexEgressPanel from "../plugins/CodexEgressPanel";
import AntigravityPanel from "../plugins/AntigravityPanel";
import McpPanel from "./McpPanel";

const KIND_ICON = {
  application: Blocks,
  mcp: Plug,
  skill: Sparkles,
};
/**
 * 目录里默认露出来的。其余的装过才显示，见下面 `list` 的注释。
 *
 * 原来第一个是酒馆（`sillytavern`）。2026-09-29 使用者：「扩展页的酒馆删掉好了，酒馆变成本项目的
 * 功能之一」—— 现在酒馆本体（下载 / 更新 / 指定版本 / 卸载、位置）在软件页的 SillyTavern 卡，
 * 三条桥的设置在软件页三张 CLI 卡的最下面，用酒馆（控制台、角色卡、世界书、扩展、备份与回收站）在
 * 三个账户页酒馆磁贴右上角的「酒馆」弹窗里（同一天第三轮对调过）。目录里那一条也删了（`extensions::RETIRED`）。
 */
const SURFACED = new Set(["codex-egress", "antigravity-ui"]);

export default function ExtensionCenter() {
  const { id } = useParams();
  const catalog = useCatalog();
  const workspace = useWorkspace();
  const action = useAction();
  const [tab, setTab] = useSession("extensions.tab", "discover");
  const [category, setCategory] = useSession("extensions.category", "all");
  const [search, setSearch] = useSession("extensions.search", "");
  const [importing, setImporting] = useSession("extensions.import.open", false);
  // 检查来源更新的结果原来直接丢掉了：点完什么都不显示。
  const [updateNotes, setUpdateNotes] = useSession<string[]>(
    "extensions.updates.notes",
    [],
  );
  const selected = catalog.data?.find((m) => m.id === id);
  // 精选目录里默认只露出三个应用。两个 MCP、两个 Skill 是「装了也没人开」的陈列品，
  // 装过的照常显示（下面的 `installed` 放行）—— 谁要是已经装过某一条，
  // 升级不该让它从界面上凭空消失，所以这几条留在 `extension-catalog.json` 里、只在前端收起。
  //
  // 原来还有两个「中转环境模板」：建的是直连站点的旧式环境、建完跳进已经退休的
  // 「服务商与环境」旧页，跟中转站页的添加线路完全重复。2026-09-27 连同 `template`
  // 这一类一起删了 —— 模板不产生安装记录，删掉不会让谁已有的东西消失。
  const installations = workspace.data?.installations ?? [];
  // 2026-10-26 起 MCP 不在目录里了，在「MCP」分页（阶段 5，M1）：原来装过的第一次打开那一页时迁过去。
  const list = (catalog.data ?? []).filter(
    (m) =>
      m.kind !== "mcp" &&
      (SURFACED.has(m.id) ||
        installations.some((i) => i.extension_id === m.id)) &&
      (category === "all" || m.kind === category) &&
      `${m.name} ${m.description}`
        .toLowerCase()
        .includes(search.toLowerCase()) &&
      (tab === "discover" ||
        installations.some(
          (i) =>
            i.extension_id === m.id &&
            (tab !== "updates" || i.version !== m.version),
        )),
  );
  if (selected)
    return (
      <>
        <Link className="qb-text-link qb-back-link" to="/extensions">
          <ArrowLeft size={15} />
          返回扩展中心
        </Link>
        <ExtensionDetail key={selected.id} manifest={selected} />
      </>
    );
  return (
    <>
      <header className="qb-page-heading">
        <div>
          <h1>扩展中心</h1>
          <p>发现适合你的能力，再装进需要它的环境。</p>
        </div>
        <Button
          icon={<FolderInput size={16} />}
          onClick={() => setImporting(true)}
        >
          手动导入
        </Button>
      </header>
      <div className="qb-tabs" role="tablist">
        {[
          ["discover", "发现"],
          ["installed", "已安装"],
          ["updates", "更新"],
          ["mcp", "MCP"],
        ].map(([key, name]) => (
          <button
            key={key}
            role="tab"
            aria-selected={tab === key}
            onClick={() => setTab(key)}
          >
            {name}
            {key === "installed" && <span>{installations.length}</span>}
          </button>
        ))}
      </div>
      {tab === "mcp" ? (
        <McpPanel />
      ) : (
        <>
          <div className="qb-catalog-toolbar">
            <div className="qb-filter-pills">
              {[
                ["all", "全部"],
                ...Object.entries(KIND_NAMES).filter(([k]) => k !== "mcp"),
              ].map(([key, name]) => (
                <button
                  key={key}
                  aria-pressed={category === key}
                  onClick={() => setCategory(key)}
                >
                  {name}
                </button>
              ))}
            </div>
            <label className="qb-search-box">
              <Search size={16} />
              <input
                aria-label="搜索扩展"
                placeholder="搜索扩展…"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </label>
          </div>
          {catalog.error && (
            <p className="notice notice--danger">{catalog.error}</p>
          )}
          {catalog.loading && !catalog.data && (
            <p className="qb-muted" role="status">
              正在读取扩展目录…
            </p>
          )}
          <div className="qb-catalog-grid">
            {list.map((m) => {
              const Icon = KIND_ICON[m.kind];
              const installed = installations.filter(
                (i) => i.extension_id === m.id,
              );
              return (
                <Link
                  className="qb-extension-card"
                  key={m.id}
                  to={"/extensions/" + m.id}
                >
                  <div
                    className={"qb-extension-icon qb-extension-icon-" + m.kind}
                  >
                    <Icon size={25} />
                  </div>
                  <span className="qb-extension-category">
                    {KIND_NAMES[m.kind]}
                  </span>
                  <h3>{m.name}</h3>
                  <p>{m.description}</p>
                  <div className="qb-extension-foot">
                    <span>
                      {installed.length
                        ? `已安装到 ${installed.length} 个环境`
                        : m.clients.map((c) => CLIENT_NAMES[c]).join(" / ")}
                    </span>
                    <ArrowRight size={17} />
                  </div>
                </Link>
              );
            })}
          </div>
          {!list.length && !catalog.loading && (
            <div className="qb-empty">
              <Blocks size={30} />
              <h3>
                {tab === "updates"
                  ? "精选目录中的已安装扩展已是当前版本"
                  : tab === "installed"
                    ? "还没有安装扩展"
                    : "没有匹配的扩展"}
              </h3>
              <p>
                {tab === "updates"
                  ? "手动来源可重新导入同一地址，解析新的固定版本后再预览更新。"
                  : "切换分类或到发现页选择需要的能力。"}
              </p>
            </div>
          )}
          {tab === "updates" && (
            <>
              <Button
                loading={action.pending === "refresh"}
                disabled={!!action.pending}
                onClick={() =>
                  void action
                    .run("refresh", workspaceApi.checkUpdates)
                    .then((r) => r && setUpdateNotes(r))
                }
              >
                检查来源更新
              </Button>
              {updateNotes.length > 0 && (
                <ul className="qb-help-note" role="status">
                  {updateNotes.map((n) => (
                    <li key={n}>{n}</li>
                  ))}
                </ul>
              )}
            </>
          )}
          <p className="qb-help-note">
            精选目录随程序发布。导入只读取元数据；安装和连接测试由你手动发起。扩展不会自动同步到其他账户。
          </p>
        </>
      )}
      <Modal
        open={importing}
        onClose={() => setImporting(false)}
        title="导入扩展"
      >
        <ImportForm onDone={() => setImporting(false)} />
      </Modal>
    </>
  );
}
function ImportForm({ onDone }: { onDone: () => void }) {
  const [source, setSource] = useSession("import.source", "");
  const action = useAction();
  return (
    <div className="qb-form">
      <label className="qb-field">
        <span>Skills · GitHub 仓库地址或本地目录</span>
        <input
          value={source}
          onChange={(e) => setSource(e.target.value)}
          placeholder="https://github.com/owner/repo/tree/main/skills"
        />
      </label>
      <p className="qb-muted">
        先扫描并加入目录，再选择目标环境安装。此步骤不会运行仓库脚本。 MCP
        在「MCP」分页里添加或从已有配置导入。
      </p>
      <Button
        variant="primary"
        loading={!!action.pending}
        onClick={() =>
          void action
            .run(
              "import",
              async () => workspaceApi.importSkills(source),
              "扩展已加入目录",
            )
            .then((r) => {
              if (r) onDone();
            })
        }
      >
        扫描并导入
      </Button>
      {action.error && <p className="notice notice--danger">{action.error}</p>}
    </div>
  );
}
function ExtensionDetail({ manifest: m }: { manifest: ExtensionManifest }) {
  const workspace = useWorkspace();
  const accounts = useResource("accounts", R.accounts);
  const action = useAction();
  const Icon = KIND_ICON[m.kind];
  const [target, setTarget] = useSession(
    "extension.target." + m.id,
    "official|claude-code|",
  );
  const [directory, setDirectory] = useSession(
    "extension.directory." + m.id,
    "",
  );
  const [config, setConfig] = useSession("extension.config." + m.id, "");
  const [preview, setPreview] = useState<InstallPreview | null>(null);
  const [check, setCheck] = useState("");
  const [removeId, setRemoveId] = useState<string | null>(null);
  const targets = [
    { value: "official|claude-code|", label: "官方 · Claude Code 默认环境" },
    { value: "official|codex|", label: "官方 · Codex" },
    ...(accounts.data?.slots ?? []).map((a) => ({
      value: "official|claude-code|" + a.label,
      label: "官方 · " + a.label,
    })),
    ...(workspace.data?.environments ?? []).map((e) => ({
      value: `relay|${e.client}|${e.id}`,
      label: "中转 · " + e.name,
    })),
  ].filter((t) => m.clients.includes(t.value.split("|")[1] as Client));
  const chosen = targets.find((t) => t.value === target) ?? targets[0];
  const [identity, client, identityId] = (
    chosen?.value ?? "official|claude-code|"
  ).split("|");
  const request: InstallRequest = {
    extension_id: m.id,
    identity_kind: identity as IdentityKind,
    client: client as Client,
    environment_id: identityId,
    directory: directory || null,
    configuration: config || null,
    preview_fingerprint: preview?.fingerprint ?? null,
  };
  const installed =
    workspace.data?.installations.filter((i) => i.extension_id === m.id) ?? [];
  return (
    <>
      <header className="qb-extension-detail-hero">
        <div className={"qb-extension-icon qb-extension-icon-" + m.kind}>
          <Icon size={34} />
        </div>
        <div>
          <span className="qb-eyebrow">{KIND_NAMES[m.kind]}</span>
          <h1>{m.name}</h1>
          <p>{m.description}</p>
        </div>
      </header>
      <div className="qb-extension-layout">
        <section className="qb-detail">
          <h2>{m.kind === "application" ? "接入与运行" : "安装到你的环境"}</h2>
          {m.kind === "application" ? (
            m.id === "codex-egress" ? (
              <CodexEgressPanel />
            ) : m.id === "antigravity-ui" ? (
              <AntigravityPanel />
            ) : (
              // 目录里只剩这两个应用；认不出的应用（手改过的目录、将来新加的）不许
              // 落到别人的面板上 —— 原来这里的兜底是酒馆页，点进去看到的是另一个东西。
              <p className="notice">这个应用在面板里没有详情页。</p>
            )
          ) : (
            <div className="qb-form">
              <label className="qb-field">
                <span>目标账户或中转环境</span>
                <select
                  value={chosen?.value}
                  onChange={(e) => {
                    setTarget(e.target.value);
                    setPreview(null);
                  }}
                >
                  {targets.map((t) => (
                    <option key={t.value} value={t.value}>
                      {t.label}
                    </option>
                  ))}
                </select>
              </label>
              {m.kind === "mcp" && (
                <>
                  <label className="qb-field">
                    <span>允许访问的目录 · 按扩展需要填写</span>
                    <input
                      placeholder="本地目录路径"
                      value={directory}
                      onChange={(e) => {
                        setDirectory(e.target.value);
                        setPreview(null);
                      }}
                    />
                  </label>
                  <details>
                    <summary>编辑 MCP 连接配置</summary>
                    <p className="qb-muted">
                      留空使用目录中的配置。可填写 command、args、env，或 HTTP
                      url 与 headers。
                    </p>
                    <textarea
                      aria-label="MCP 配置"
                      rows={8}
                      className="qb-input qb-code-input"
                      value={config}
                      placeholder={m.configuration}
                      onChange={(e) => {
                        setConfig(e.target.value);
                        setPreview(null);
                      }}
                    />
                  </details>
                </>
              )}
              <div className="qb-inline-actions">
                <Button
                  loading={action.pending === "preview"}
                  disabled={!!action.pending}
                  onClick={() =>
                    void action
                      .run("preview", () =>
                        workspaceApi.previewExtension(request),
                      )
                      .then((p) => {
                        if (p) setPreview(p);
                      })
                  }
                >
                  预览安装变更
                </Button>
                {m.kind === "mcp" && (
                  <Button
                    icon={<Plug size={15} />}
                    disabled={!!action.pending}
                    loading={action.pending === "check"}
                    onClick={() =>
                      void action
                        .run("check", () => workspaceApi.checkMcp(request))
                        .then((c) => {
                          if (c)
                            setCheck(
                              `${c.detail}；发现 ${c.tools.length} 个工具：${c.tools.join("、")}`,
                            );
                        })
                    }
                  >
                    连接测试
                  </Button>
                )}
              </div>
              {check && <p className="notice">{check}</p>}
              {preview && (
                <div className="qb-install-preview">
                  <h3>即将应用的变更</h3>
                  <p className="qb-break">{preview.destination}</p>
                  {preview.conflict && (
                    <p className="notice notice--danger">
                      发现已有内容或用户修改，安装已阻断。请先另存修改并处理冲突。
                    </p>
                  )}
                  <details>
                    <summary>查看配置与文件差异</summary>
                    <pre className="qb-code-preview">{preview.diff}</pre>
                  </details>
                  <ul>
                    {preview.changes.map((c) => (
                      <li key={c}>{c}</li>
                    ))}
                  </ul>
                  <details>
                    <summary>文件清单 · {preview.files.length}</summary>
                    {preview.files.map((f) => (
                      <p key={f} className="qb-muted">
                        {f}
                      </p>
                    ))}
                  </details>
                  <Button
                    variant="primary"
                    icon={<Download size={15} />}
                    disabled={preview.conflict || !!action.pending}
                    loading={action.pending === "install"}
                    onClick={() =>
                      void action
                        .run(
                          "install",
                          () => workspaceApi.install(request),
                          m.kind === "mcp"
                            ? "MCP 已写入所选环境"
                            : "Skill 已安装",
                        )
                        .then((i) => {
                          if (i) setPreview(null);
                        })
                    }
                  >
                    {m.kind === "mcp" ? "启用到此环境" : "安装到此环境"}
                  </Button>
                </div>
              )}
              <p className="qb-help-note">
                已有会话可能需要重启才能读取新扩展。连接测试会启动所配置的 MCP
                程序或连接 HTTP 服务。
              </p>
            </div>
          )}
          {action.error && (
            <p role="alert" className="notice notice--danger">
              {action.error}
            </p>
          )}
          {installed.length > 0 && (
            <section className="qb-section">
              <h3>已安装的位置</h3>
              {installed.map((i) => (
                <div className="qb-check-row" key={i.id}>
                  <div className="qb-grow">
                    <strong>
                      {i.identity_kind === "relay" ? "中转" : "官方"} ·{" "}
                      {i.environment_id || CLIENT_NAMES[i.client]}
                    </strong>
                    <p className="qb-break">{i.path}</p>
                    <small>
                      {STATE_NAMES[i.state]} · {i.version.slice(0, 16)}
                    </small>
                  </div>
                  <Button size="sm" onClick={() => setRemoveId(i.id)}>
                    {m.kind === "application"
                      ? "解除接入"
                      : m.kind === "mcp"
                        ? "停用并移除"
                        : "卸载"}
                  </Button>
                </div>
              ))}
            </section>
          )}
        </section>
        <aside className="qb-extension-meta">
          <h3>扩展信息</h3>
          <dl>
            <dt>类型</dt>
            <dd>{KIND_NAMES[m.kind]}</dd>
            <dt>适用客户端</dt>
            <dd>{m.clients.map((c) => CLIENT_NAMES[c]).join("、")}</dd>
            <dt>固定版本</dt>
            <dd className="qb-break">{m.version}</dd>
            <dt>许可证</dt>
            <dd>{m.license}</dd>
            <dt>来源</dt>
            {/* 箭头由 ExternalLink 自己补一个 ↗，这里不要再加 →。
                本页的约定是：站内链接用 →，外链用 ↗，两个一起出现
                读起来像两个不同的动作。 */}
            <dd className="qb-break">
              {m.source.startsWith("https://") ? (
                <ExternalLink href={m.source}>查看项目来源</ExternalLink>
              ) : (
                m.source
              )}
            </dd>
          </dl>
          {m.dependencies.length > 0 && (
            <>
              <h3>依赖与要求</h3>
              <ul>
                {m.dependencies.map((d) => (
                  <li key={d}>{d}</li>
                ))}
              </ul>
              <Link className="qb-text-link" to="/software">
                管理软件依赖
                <ArrowRight size={14} />
              </Link>
            </>
          )}
        </aside>
      </div>
      <Modal
        open={!!removeId}
        onClose={() => setRemoveId(null)}
        title="卸载扩展"
        footer={
          <Button
            variant="danger"
            loading={!!action.pending}
            onClick={() => {
              if (removeId)
                void action
                  .run(
                    "uninstall",
                    () => workspaceApi.uninstall(removeId).then(() => true),
                    "扩展已卸载，用户数据保留",
                  )
                  .then((ok) => {
                    if (ok) setRemoveId(null);
                  });
            }}
          >
            确认卸载
          </Button>
        }
      >
        <p>
          从所选环境中移除扩展。Skill 文件移入保留数据目录；MCP
          只移除由本程序管理且没有外部修改的条目。
        </p>
      </Modal>
    </>
  );
}
