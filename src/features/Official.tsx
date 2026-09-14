// 槽位选择走会话态，不走路由 ——
// 这一页现在就是首页（`/`），选中哪个槽位不该改变地址，
// 也不该在浏览器历史里留一堆记录。
import { KeyRound, Plus, Play, ArrowLeftRight } from "lucide-react";
import { useResource, useSession } from "../lib/store";
import { R } from "../lib/resources";
import {
  useAction,
  workspaceApi,
  CLIENT_NAMES,
  type Client,
  type ConfigPreview,
} from "../lib/workspace";
import { Button, Modal } from "../ui";
import {
  requestNewSlot,
  requestSwitch,
} from "../pages/accounts/AccountDialogs";
export default function Official() {
  const [id, setId] = useSession<string>("official.selected", "");
  const accounts = useResource("accounts", R.accounts);
  const action = useAction();
  const [migration, setMigration] = useSession<{
    client: Client;
    id: string;
    files: ConfigPreview[];
  } | null>("official.migration", null);
  const [filter, setFilter] = useSession("official.filter", "");
  const slots = accounts.data?.slots ?? [];
  const selected =
    slots.find((s) => s.label === id) ??
    slots.find((s) => s.active) ??
    slots[0];
  return (
    <>
      <header className="qb-page-heading">
        <div>
          <h1>官方账户</h1>
          <p>管理你自己的登录槽位，以及各自的官方客户端资料。</p>
        </div>
        <Button
          variant="primary"
          icon={<Plus size={15} />}
          onClick={requestNewSlot}
        >
          新建账户槽位
        </Button>
      </header>
      {accounts.error && (
        <p className="notice notice--danger">{accounts.error}</p>
      )}
      <div className="qb-master-detail">
        <aside className="qb-object-list">
          <input
            className="qb-input"
            aria-label="搜索官方账户"
            placeholder="搜索账户…"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          {slots
            .filter((s) => s.label.includes(filter))
            .map((s) => (
              <button
                type="button"
                className={
                  "qb-object-item" +
                  (selected?.label === s.label ? " selected" : "")
                }
                onClick={() => setId(s.label)}
                key={s.label}
              >
                <span className="qb-avatar">
                  {s.label.slice(0, 1).toUpperCase()}
                </span>
                <span>
                  <strong>{s.label}</strong>
                  <small>{s.plan ?? "官方账户"}</small>
                </span>
                {s.active && <span className="qb-dot" />}
              </button>
            ))}
          {!slots.length && (
            <p className="qb-list-note">
              尚未建立槽位，仍可从工作台启动默认官方环境。
            </p>
          )}
        </aside>
        <section className="qb-detail">
          {selected ? (
            <>
              <div className="qb-detail-heading">
                <span className="qb-icon-tile">
                  <KeyRound size={24} />
                </span>
                <div>
                  <h2>{selected.label}</h2>
                  <p>
                    {selected.active ? "当前官方账户" : "已保存的官方账户槽位"}
                  </p>
                </div>
                {!selected.active && (
                  <Button
                    icon={<ArrowLeftRight size={15} />}
                    onClick={() => requestSwitch(selected.label)}
                  >
                    切换至此账户
                  </Button>
                )}
              </div>
              <div className="qb-facts">
                <div>
                  <span>本地登录记录</span>
                  <strong>
                    {selected.logged_in ? "已检测到" : "需要登录"}
                  </strong>
                </div>
                <div>
                  <span>账户方案</span>
                  <strong>{selected.plan ?? "未读取到"}</strong>
                </div>
                <div>
                  <span>桌面端独立资料</span>
                  <strong>
                    {selected.desktop_profile ? "已建立" : "尚未建立"}
                  </strong>
                </div>
              </div>
              <h3 className="qb-subheading">启动客户端</h3>
              <div className="qb-tool-list">
                {Object.entries(CLIENT_NAMES).map(([client, name]) => (
                  <div key={client}>
                    <div>
                      <strong>{name}</strong>
                      <p>
                        {client === "codex"
                          ? "使用 Codex 原生官方登录目录"
                          : "使用当前选定的官方身份"}
                      </p>
                    </div>
                    <Button
                      icon={<Play size={14} />}
                      disabled={
                        !!action.pending ||
                        (!selected.active && client !== "codex")
                      }
                      loading={action.pending === client}
                      onClick={() =>
                        void action.run(
                          client,
                          () =>
                            workspaceApi.launch(
                              client as Client,
                              "official",
                              client === "codex" ? "" : selected.label,
                            ),
                          "官方会话已启动",
                        )
                      }
                    >
                      启动
                    </Button>
                  </div>
                ))}
              </div>
              <p className="qb-help-note">
                切换官方账户会先关闭相关官方会话。已登记的中转会话使用独立目录，继续运行。登录信息来自本地客户端记录。
              </p>
            </>
          ) : (
            <div className="qb-empty">
              <KeyRound size={30} />
              <h2>给不同的官方账户各留一个位置</h2>
              <p>槽位分别保存客户端资料；登录由官方客户端完成。</p>
              <Button variant="primary" onClick={requestNewSlot}>
                建立第一个槽位
              </Button>
            </div>
          )}
          {action.error && (
            <p role="alert" className="notice notice--danger">
              {action.error}
            </p>
          )}
        </section>
      </div>
      <details className="qb-help-details">
        <summary>检查与迁移官方目录中的 API 配置</summary>
        <p className="qb-muted">
          先停止相关官方会话，查看差异后，将 API
          配置迁入独立中转环境。未知云平台配置需要手动处理。
        </p>
        <div className="qb-inline-actions">
          {(["claude-code", "codex"] as Client[]).map((client) => (
            <Button
              key={client}
              onClick={() =>
                void action
                  .run("preview", () =>
                    workspaceApi.officialPreview(
                      client,
                      client === "codex" ? "" : (selected?.label ?? ""),
                    ),
                  )
                  .then((files) => {
                    if (files)
                      setMigration({
                        client,
                        id: client === "codex" ? "" : (selected?.label ?? ""),
                        files,
                      });
                  })
              }
            >
              检查 {CLIENT_NAMES[client]}
            </Button>
          ))}
        </div>
      </details>
      <Modal
        open={!!migration}
        onClose={() => setMigration(null)}
        title="迁移 API 配置"
        footer={
          <Button
            variant="primary"
            loading={!!action.pending}
            onClick={() => {
              if (migration)
                void action
                  .run(
                    "migrate",
                    () =>
                      workspaceApi.officialMigrate(
                        migration.client,
                        migration.id,
                        migration.files[0]?.fingerprint ?? "",
                      ),
                    "已迁移到独立中转环境",
                  )
                  .then((done) => {
                    if (done) setMigration(null);
                  });
            }}
          >
            创建中转环境并移除上述残留
          </Button>
        }
      >
        {migration?.files.map((file) => (
          <section key={file.path}>
            <p className="qb-break">{file.path}</p>
            <div className="qb-diff">
              <pre>{file.before}</pre>
              <pre>{file.after || "移除 API 认证文件"}</pre>
            </div>
          </section>
        ))}
      </Modal>
    </>
  );
}
