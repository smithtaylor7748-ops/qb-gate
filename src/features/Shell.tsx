import { CHANNELS } from "../lib/channels";
import { listen } from "@tauri-apps/api/event";
import { useAllTasks } from "../lib/tasks";
import {
  Component,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  NavLink,
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from "react-router-dom";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  Command,
  History,
  Search,
} from "lucide-react";
import { LEGACY_REDIRECTS, NAV, RELAY_BASE } from "../lib/routes";
import { api } from "../lib/api";
import { R } from "../lib/resources";
import { invalidateAll, res, useResource, useSession } from "../lib/store";
import { useSummaries } from "./sidebar";
import {
  useAction,
  workspaceApi,
  useCatalog,
  useWorkspace,
  useWorkspaceEvents,
  STATE_NAMES,
} from "../lib/workspace";
import { DEMO_ENABLED } from "../lib/demo";
import { Button, Modal, ToastProvider } from "../ui";
import AccountDialogs from "../pages/accounts/AccountDialogs";
import Accounts from "./Accounts";
import Official from "./Official";
import Onboarding from "./Onboarding";
import SecuritySheet from "./security/SecuritySheet";
import RelayCenter from "./RelayCenter";
import ExtensionCenter from "./ExtensionCenter";
import Software from "./Software";
import SettingsCenter from "./SettingsCenter";

// 不在侧栏里、但需要顶栏标题的路由。
const EXTRA_TITLES: Record<string, string> = {
  "/onboarding": "新手引导",
  "/accounts": "账户槽位管理",
};
const positions = new Map<string, number>();
class Boundary extends Component<{ children: ReactNode }, { error: string }> {
  state = { error: "" };
  static getDerivedStateFromError(error: Error) {
    return { error: error.message };
  }
  render() {
    return this.state.error ? (
      <div className="qb-empty">
        <h2>此页面暂时无法显示</h2>
        <p>{this.state.error}</p>
        <Button
          onClick={() => {
            invalidateAll();
            this.setState({ error: "" });
          }}
        >
          重新加载页面
        </Button>
      </div>
    ) : (
      this.props.children
    );
  }
}

const startupQuery = res(workspaceApi.startup);
function RecoveryGuard({ children }: { children: ReactNode }) {
  const startup = useResource("startup", startupQuery);
  const action = useAction();
  if (DEMO_ENABLED) return <>{children}</>;
  if (!startup.data && !startup.error)
    return (
      <div className="qb-empty" role="status">
        正在检查配置与恢复记录…
      </div>
    );
  if (startup.error || !startup.data?.ready) {
    const blocked = startup.data?.blocked ?? [];
    return (
      <section className="qb-panel qb-form">
        <h1>需要完成数据恢复</h1>
        <p>
          启动检查发现未完成的恢复或不可读取的配置。先根据下面的原因处理原文件，再重试；恢复记录仍保留在本机。
        </p>
        <pre className="qb-break" role="alert">
          {startup.data?.error ?? startup.error}
        </pre>
        {blocked.length > 0 && (
          <details open>
            <summary>结不清的恢复记录（{blocked.length} 条）</summary>
            <p className="qb-muted">
              放弃之后这几条记录会移到 quarantine 目录保留，
              <strong>不会删除</strong>
              ；但它们牵涉的配置文件将保持当前状态，不再能按记录回滚。确认过下面的路径再操作。
            </p>
            <pre className="qb-break">{blocked.join("\n")}</pre>
          </details>
        )}
        <div className="qb-inline-actions">
          <Button
            variant="primary"
            loading={!!action.pending}
            onClick={() => void action.run("retry", workspaceApi.retryStartup)}
          >
            重新检查与恢复
          </Button>
          {blocked.length > 0 && (
            <Button
              disabled={!!action.pending}
              onClick={() =>
                void action.run(
                  "discard",
                  workspaceApi.discardBlockedRecovery,
                  "已放弃结不清的恢复记录",
                )
              }
            >
              放弃这 {blocked.length} 条记录并继续
            </Button>
          )}
          <Button
            disabled={!!action.pending}
            onClick={() =>
              void action.run("unlock", api.gateUnlockAll, "已移除执行锁")
            }
          >
            应急移除软件执行锁
          </Button>
        </div>
        {action.error && <p role="alert">{action.error}</p>}
      </section>
    );
  }
  return <>{children}</>;
}
function Layout() {
  const taskAction = useAction();
  const navigate = useNavigate();
  const location = useLocation();
  const main = useRef<HTMLElement>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [tasksOpen, setTasksOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [theme, setTheme] = useSession(
    "theme",
    localStorage.getItem("qb-theme") ?? "system",
  );
  const workspace = useWorkspace();
  const catalog = useCatalog();
  const legacyTasks = useAllTasks();
  const accounts = useResource("accounts", R.accounts);
  useWorkspaceEvents();
  useEffect(() => {
    if (DEMO_ENABLED) return;
    let disposed = false;
    const p = listen<string>(CHANNELS.navigate, (event) => {
      if (event.payload.startsWith("/")) navigate(event.payload);
    });
    void p.then((stop) => {
      if (disposed) stop();
    });
    return () => {
      disposed = true;
      void p.then((stop) => stop());
    };
  }, [navigate]);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("qb-theme", theme);
  }, [theme]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSearchOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  useLayoutEffect(() => {
    const element = main.current;
    if (element) element.scrollTop = positions.get(location.pathname) ?? 0;
    return () => {
      if (element) positions.set(location.pathname, element.scrollTop);
    };
  }, [location.pathname]);
  // 「/」要精确匹配，否则它会命中所有路径。
  const active = NAV.find((n) =>
    n.path === "/"
      ? location.pathname === "/"
      : location.pathname.startsWith(n.path),
  );
  const summaries = useSummaries();
  const operations = workspace.data?.operations ?? [];
  const running = [
    ...operations.filter((o) => o.status === "running"),
    ...legacyTasks.filter(([, t]) => t.running),
  ];
  const results = [
    ...(catalog.data ?? []).map((m) => ({
      label: m.name,
      detail: "扩展 · " + m.description,
      path: "/extensions/" + m.id,
    })),
    ...NAV.map((n) => ({ label: n.name, detail: n.hint, path: n.path })),
    ...(accounts.data?.slots ?? []).map((a) => ({
      label: a.label,
      detail: "官方账户",
      path: "/",
    })),
    ...(workspace.data?.providers ?? []).map((p) => ({
      label: p.name,
      detail: "中转服务商",
      path: RELAY_BASE + "/" + p.id,
    })),
    ...(workspace.data?.environments ?? []).map((e) => ({
      label: e.name,
      detail: "中转使用环境",
      path: RELAY_BASE + "/" + e.provider_id + "?environment=" + e.id,
    })),
  ].filter((r) =>
    (r.label + r.detail).toLowerCase().includes(search.toLowerCase()),
  );
  return (
    <>
      <a className="qb-skip" href="#main-content">
        跳到主要内容
      </a>
      <div className="qb-shell">
        <aside className="qb-sidebar">
          <NavLink to="/" className="qb-brand">
            <span className="qb-mark">
              Q<span>↗</span>
            </span>
            <span>
              QB Gate<small>你的 AI 工作空间</small>
            </span>
          </NavLink>
          <button
            className="qb-search-trigger"
            onClick={() => setSearchOpen(true)}
          >
            <Search size={16} />
            <span>搜索与快速跳转</span>
            <kbd>⌃ K</kbd>
          </button>
          <nav aria-label="主导航">
            {NAV.map(({ path, name, icon: Icon, hint }) => {
              const readout = summaries[path];
              return (
                <NavLink key={path} to={path} end={path === "/"} title={name}>
                  <Icon size={19} />
                  <span>
                    {name}
                    <small
                      className={readout ? "qb-nav-readout " + readout[1] : ""}
                    >
                      {readout ? readout[0] : hint}
                    </small>
                  </span>
                </NavLink>
              );
            })}
          </nav>
          <div className="qb-sidebar-foot">
            <div className="qb-local-dot" />
            <span>本地工作空间</span>
            <select
              aria-label="主题"
              value={theme}
              onChange={(e) => setTheme(e.target.value)}
            >
              <option value="system">跟随系统</option>
              <option value="light">浅色</option>
              <option value="dark">深色</option>
            </select>
          </div>
        </aside>
        <div className="qb-content">
          <header className="qb-topbar">
            <div className="qb-history">
              <button aria-label="返回" onClick={() => navigate(-1)}>
                <ArrowLeft size={17} />
              </button>
              <button aria-label="前进" onClick={() => navigate(1)}>
                <ArrowRight size={17} />
              </button>
              <span>
                {active?.name ?? EXTRA_TITLES[location.pathname] ?? "QB Gate"}
              </span>
            </div>
            <div className="qb-top-actions">
              {DEMO_ENABLED && <span className="qb-badge">演示数据</span>}
              <button
                onClick={() => setTasksOpen(true)}
                className="qb-task-button"
              >
                <History size={16} />
                任务中心{running.length > 0 && <b>{running.length}</b>}
              </button>
            </div>
          </header>
          <main id="main-content" className="qb-main" ref={main} tabIndex={-1}>
            <div className="qb-page">
              <Boundary key={location.pathname.split("/")[1]}>
                <RecoveryGuard>
                  <Routes>
                    <Route path="/" element={<Accounts />} />
                    <Route path="/onboarding" element={<Onboarding />} />
                    {/* 槽位的逐项管理（桌面端独立资料、官方目录残留迁移）。
                        不进侧栏 —— 低频，而且从总览「账户槽位 · 管理」进来就够。 */}
                    <Route path="/accounts" element={<Official />} />
                    <Route path="/relays/:id?" element={<RelayCenter />} />
                    <Route
                      path="/extensions/:id?"
                      element={<ExtensionCenter />}
                    />
                    <Route path="/software" element={<Software />} />
                    <Route
                      path="/settings/:section?"
                      element={<SettingsCenter />}
                    />
                    {/* 旧路径。托盘写死了 emit("workspace://navigate", "/relays")，
                        书签和旧截图脚本也还指着这些地址。 */}
                    {LEGACY_REDIRECTS.map(([from, to]) => (
                      <Route
                        key={from}
                        path={from + "/*"}
                        element={<Navigate to={to} replace />}
                      />
                    ))}
                    <Route path="*" element={<Navigate to="/" replace />} />
                  </Routes>
                </RecoveryGuard>
              </Boundary>
            </div>
          </main>
        </div>
      </div>
      <AccountDialogs />
      {/* 安全小窗全局只挂一份 —— 总览四格、门禁读数、GPT 那侧都喊同一个。 */}
      <SecuritySheet />
      <Modal
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        title={
          <>
            <Command size={18} />
            快速跳转
          </>
        }
      >
        <input
          autoFocus
          className="qb-input"
          aria-label="搜索功能、账户、服务商或扩展"
          placeholder="搜索功能、账户、服务商、扩展…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <div className="qb-search-results">
          {results.map((r) => (
            <button
              key={r.path}
              onClick={() => {
                navigate(r.path);
                setSearchOpen(false);
              }}
            >
              <span>
                {r.label}
                <small>{r.detail}</small>
              </span>
              <ArrowRight size={16} />
            </button>
          ))}
          {!results.length && <p className="qb-muted">没有找到匹配项目。</p>}
        </div>
      </Modal>
      <Modal
        open={tasksOpen}
        onClose={() => setTasksOpen(false)}
        title="任务中心"
      >
        <p className="qb-muted">
          任务持续运行，切换页面不会取消。配置提交阶段不支持中断。
        </p>
        <div className="qb-task-list">
          {legacyTasks.map(([id, t]) => (
            <article key={id}>
              <div>
                <strong>{t.phase || id}</strong>
                <span className="qb-badge">
                  {t.running ? "运行中" : t.error ? "失败" : "完成"}
                </span>
              </div>
              {t.running && <progress value={t.step} max={t.total || 1} />}
              <p>{t.error}</p>
              <details>
                <summary>执行详情</summary>
                <pre className="qb-code-preview">{t.log.join("\n")}</pre>
              </details>
            </article>
          ))}
          {operations.slice(0, 40).map((o) => (
            <article key={o.id}>
              <div>
                <strong>{o.kind}</strong>
                <span className="qb-badge">
                  {STATE_NAMES[o.status] ?? o.status}
                </span>
              </div>
              <p>{o.phase}</p>
              {o.status === "running" && (
                <progress value={o.progress} max={100} />
              )}
              <small>{new Date(o.started_at).toLocaleString()}</small>
              {o.can_cancel && o.status === "running" && (
                <Button
                  size="sm"
                  disabled={!!taskAction.pending}
                  onClick={() =>
                    void taskAction.run("cancel", () =>
                      workspaceApi.cancelOperation(o.id),
                    )
                  }
                >
                  取消此任务
                </Button>
              )}
              {o.detail && <p className="qb-error-text">{o.detail}</p>}
            </article>
          ))}
          {operations.length === 0 && legacyTasks.length === 0 && (
            <div className="qb-empty">
              <Check size={24} />
              <h3>暂时没有任务</h3>
              <p>启动、诊断和安装的进度会出现在这里。</p>
            </div>
          )}
        </div>
      </Modal>
    </>
  );
}
export default function Shell() {
  return (
    <ToastProvider>
      <Layout />
    </ToastProvider>
  );
}
