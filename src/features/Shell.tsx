import { CHANNELS } from "../lib/channels";
import { listen } from "@tauri-apps/api/event";
import {
  Component,
  Fragment,
  lazy,
  Suspense,
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
import { ArrowRight, Command, Search } from "lucide-react";
import { LEGACY_REDIRECTS, NAV, RELAY_BASE } from "../lib/routes";
import { api } from "../lib/api";
import { R } from "../lib/resources";
import {
  invalidateAll,
  res,
  setSession,
  useResource,
  useSession,
} from "../lib/store";
import { SIDES, SIDE_KEY, type Side } from "../lib/side";
import { useSummaries } from "./sidebar";
import {
  useAction,
  workspaceApi,
  useCatalog,
  useWorkspace,
  useWorkspaceEvents,
} from "../lib/workspace";
import { DEMO_ENABLED } from "../lib/demo";
import { Button, Modal, ToastProvider } from "../ui";
import AccountDialogs from "../pages/accounts/AccountDialogs";
import Accounts from "./Accounts";
import SecuritySheet from "./security/SecuritySheet";

// 首屏只需要仪表盘（「/」）和两个全局小窗，其余六页各拆一个 chunk。
// 不是点进去才下：模块一加载就排一个空闲回调把它们全部预取进来 ——
// 首屏不用解析它们，点进去时也不用再等。
const PAGES = {
  onboarding: () => import("./Onboarding"),
  relays: () => import("./RelayCenter"),
  extensions: () => import("./ExtensionCenter"),
  software: () => import("./Software"),
  subscription: () => import("./subscription/Subscription"),
  settings: () => import("./SettingsCenter"),
};
const Onboarding = lazy(PAGES.onboarding);
const RelayCenter = lazy(PAGES.relays);
const ExtensionCenter = lazy(PAGES.extensions);
const Software = lazy(PAGES.software);
const Subscription = lazy(PAGES.subscription);
const SettingsCenter = lazy(PAGES.settings);
requestIdleCallback(
  () => {
    // 预取失败不在这里报：真点进那一页时 lazy 会再 import 一次，
    // 失败会落到 Boundary 上，带着原因显示出来。
    for (const load of Object.values(PAGES)) load().catch(() => {});
  },
  { timeout: 3000 },
);

// 不在侧栏里、但需要顶栏标题的路由。
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
  const navigate = useNavigate();
  const location = useLocation();
  const main = useRef<HTMLElement>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [theme, setTheme] = useSession(
    "theme",
    localStorage.getItem("qb-theme") ?? "system",
  );
  const workspace = useWorkspace();
  const catalog = useCatalog();
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
  const summaries = useSummaries();
  // 侧栏里那两个子项的高亮跟着它走；`Home` 读的是同一个键。
  const [side] = useSession<Side>(SIDE_KEY, "claude");
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
          {/* 演示数据标记。**任何宽度都必须看得见** —— 它回答的是
              「屏幕上这些数字是不是你的真实状态」。放进会随窄屏收起的
              侧栏页脚里，窄屏截图就看不出这是演示数据了，而截图恰恰是
              最容易被当成真实状态拿去用的东西。 */}
          {DEMO_ENABLED && (
            <span
              className="qb-demo-badge"
              title="界面上的数字全部是编造的演示数据"
            >
              演示数据
            </span>
          )}
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
                <Fragment key={path}>
                  {path === "/" ? (
                    <div
                      className={
                        "qb-account-nav" +
                        (location.pathname === "/" ? " active" : "")
                      }
                    >
                      <NavLink to="/" end title={name}>
                        <Icon size={19} />
                        <span>
                          {name}
                          <small className="qb-nav-readout">
                            {side === "gpt"
                              ? "桌面端账户"
                              : (readout?.[0] ?? hint)}
                          </small>
                        </span>
                      </NavLink>
                      <div
                        className="qb-account-switch"
                        aria-label="官方账户平台"
                      >
                        {SIDES.map((s) => (
                          <button
                            key={s.id}
                            type="button"
                            className={
                              "qb-nav-sub" +
                              (side === s.id && location.pathname === "/"
                                ? " on"
                                : "")
                            }
                            aria-pressed={side === s.id}
                            title={s.hint}
                            onClick={() => {
                              setSession<Side>(SIDE_KEY, s.id);
                              if (location.pathname !== "/") navigate("/");
                            }}
                          >
                            {s.label}
                          </button>
                        ))}
                      </div>
                    </div>
                  ) : (
                    <NavLink to={path} title={name}>
                      <Icon size={19} />
                      <span>
                        {name}
                        <small
                          className={
                            readout ? "qb-nav-readout " + readout[1] : ""
                          }
                        >
                          {readout ? readout[0] : hint}
                        </small>
                      </span>
                    </NavLink>
                  )}
                </Fragment>
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
          <main id="main-content" className="qb-main" ref={main} tabIndex={-1}>
            <div className="qb-page">
              {/* Suspense 必须在带 key 的 Boundary 外面。Boundary 按一级路径换 key，
                  切页时整棵子树重建；Suspense 若在里面，每次都是「新挂上的」边界，
                  React 会直接亮出 fallback。放在外面它一直在，路由自带的
                  startTransition 才能让旧页面留到新 chunk 到齐。
                  fallback 里不许有 h1：test:ui 以 `.qb-page h1` 出现作为页面就绪。 */}
              <Suspense
                fallback={
                  <div className="qb-empty" role="status">
                    正在打开页面…
                  </div>
                }
              >
                <Boundary key={location.pathname.split("/")[1]}>
                  <RecoveryGuard>
                    <Routes>
                      <Route path="/" element={<Accounts />} />
                      <Route path="/onboarding" element={<Onboarding />} />
                      <Route path="/relays/:id?" element={<RelayCenter />} />
                      <Route
                        path="/extensions/:id?"
                        element={<ExtensionCenter />}
                      />
                      <Route path="/subscription" element={<Subscription />} />
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
              </Suspense>
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
