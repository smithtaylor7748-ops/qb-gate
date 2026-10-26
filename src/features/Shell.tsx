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
import { NAV, RELAY_BASE } from "../lib/routes";
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
import { slotName } from "../lib/slotName";
import { codexApi } from "../lib/codexAccounts";
import { antigravityApi } from "../lib/antigravity";
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
import UpdateDialog from "./UpdateDialog";
import DeepLinkHost from "./DeepLinkImport";
import {
  BRIDGE_CARD,
  bridgeHref,
  type BridgeProvider,
} from "./tavern/bridge/where";

/** 快速跳转里三条桥的名字（「酒馆桥接 · Claude」）。 */
const BRIDGE_SEARCH: Record<BridgeProvider, string> = {
  claude: "Claude",
  gpt: "GPT",
  gemini: "Gemini",
};

// 首屏只需要仪表盘（「/」）和两个全局小窗，其余六页各拆一个 chunk。
// 不是点进去才下：模块一加载就排一个空闲回调把它们全部预取进来 ——
// 首屏不用解析它们，点进去时也不用再等。
//
// ⛔ 这张表就是全部页面（2026-09-27）。新手引导（`/onboarding`）和中转站底下那套
// 「服务商 / 凭证 / 环境」旧页（`/relays/<id>`）都删了：前者的五步首页和软件页早就覆盖，
// 后者被中转站页的添加线路弹窗取代之后只剩深链接进得去，而快速跳转和扩展页正好在发深链接。
// 加页面就加在这里、给它一个侧栏入口或一个看得见的按钮；别留「有路由没入口」的页。
const PAGES = {
  // 0.32.0：用量明细。**不进 NAV** —— 只从三张用量卡的页脚按钮进来。
  usage: () => import("./Usage"),
  relays: () => import("./station/StationCenter"),
  extensions: () => import("./ExtensionCenter"),
  software: () => import("./Software"),
  subscription: () => import("./subscription/Subscription"),
  settings: () => import("./SettingsCenter"),
};
const Usage = lazy(PAGES.usage);
const StationCenter = lazy(PAGES.relays);
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
  // 快速跳转里的 GPT / 反重力账户：**只在搜索框打开时读一次**，不挂全局轮询 ——
  // 侧栏常驻，给它挂两个 15 秒的轮询只为了一个偶尔才开的搜索框不划算。
  const [otherSlots, setOtherSlots] = useState<
    { key: string; label: string; side: Side }[]
  >([]);
  useEffect(() => {
    if (!searchOpen) return;
    let cancelled = false;
    void Promise.allSettled([
      codexApi.accounts(),
      antigravityApi.status(),
    ]).then(([gpt, ag]) => {
      if (cancelled) return;
      setOtherSlots([
        ...(gpt.status === "fulfilled" ? gpt.value.slots : []).map((s) => ({
          key: "gpt:" + s.id,
          label: slotName(s.email, s.label),
          side: "gpt" as const,
        })),
        ...(ag.status === "fulfilled" ? ag.value.accounts.slots : []).map(
          (s) => ({
            key: "antigravity:" + s.id,
            label: slotName(s.email, s.label),
            side: "antigravity" as const,
          }),
        ),
      ]);
    });
    return () => {
      cancelled = true;
    };
  }, [searchOpen]);
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
  // 「官方账户」这一块包括它的用量明细页 `/usage`（只从三张用量卡进、不进 NAV）。
  // 原来只认 `/`，一进用量明细侧栏就一项都不亮，人不知道自己在哪。
  const inAccounts =
    location.pathname === "/" || location.pathname === "/usage";
  // ⛔ 每一条要有自己的 `key`。原来用 `path` 当 key，而「官方账户」那一项和每个 Claude 槽位
  // 的 path 都是 `/` —— React 拿重复的 key 对不上号，过滤之后列表里会留着不匹配的旧项、
  // 还会把同一个账户画两遍（2026-09-23 实测：搜「demo-alt」列出了五个别的槽位）。
  // 账户那几条带 `side`：点了要先切到它那一边，不然在 GPT 页搜一个 Claude 槽位点进去，
  // 落地的还是 GPT 页。
  const results: {
    key: string;
    label: string;
    detail: string;
    path: string;
    side?: Side;
  }[] = [
    ...(catalog.data ?? []).map((m) => ({
      key: "ext:" + m.id,
      label: m.name,
      detail: "扩展 · " + m.description,
      path: "/extensions/" + m.id,
    })),
    ...NAV.map((n) => ({
      key: "nav:" + n.path,
      label: n.name,
      detail: n.hint,
      path: n.path,
    })),
    // 酒馆 2026-09-29 起是面板的内置功能、不在扩展目录里了 —— 不补这几条，搜「酒馆」什么都搜不到。
    // 第三轮对调之后：酒馆本体（下载与版本）在软件页的 SillyTavern 卡；三条桥的设置在软件页各自那张
    // CLI 卡的最下面；用酒馆（控制台、角色卡……）在账户页「酒馆」磁贴右上角的弹窗里（弹窗没有地址，只写在说明里）。
    {
      key: "feature:sillytavern",
      label: "酒馆 SillyTavern",
      detail:
        "软件 · 下载安装、版本、位置与端口（控制台、角色卡、世界书、扩展在账户页「酒馆」磁贴右上角）",
      path: "/software?focus=sillytavern",
    },
    ...(["claude", "gpt", "gemini"] as const).map((p) => ({
      key: "feature:bridge-" + p,
      label: `酒馆桥接 · ${BRIDGE_SEARCH[p]}`,
      detail: `软件 · ${BRIDGE_CARD[p]} 卡最下面`,
      path: bridgeHref(p),
    })),
    ...(accounts.data?.slots ?? []).map((a) => ({
      key: "claude:" + a.label,
      label: slotName(a.email, a.label),
      detail: "官方账户 · Claude",
      path: "/",
      side: "claude" as const,
    })),
    ...otherSlots.map((s) => ({
      key: s.key,
      label: s.label,
      detail: s.side === "gpt" ? "官方账户 · GPT" : "官方账户 · 反重力",
      path: "/",
      side: s.side,
    })),
    // 站点只落到中转站页本身。原来带着 id 进 `/relays/<id>`，落地的是已经退休的
    // 「服务商与环境」旧页；使用环境那一类也删了 —— 新页里没有「环境」这一层，
    // 本机路由那几个环境是面板自己建的，不是给人挑的。
    ...(workspace.data?.providers ?? []).map((p) => ({
      key: "provider:" + p.id,
      label: p.name,
      detail: "中转站点",
      path: RELAY_BASE,
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
                        "qb-account-nav" + (inAccounts ? " active" : "")
                      }
                    >
                      <NavLink to="/" end title={name}>
                        <Icon size={19} />
                        <span>
                          {name}
                          {/* Claude 那一边的读数是激活槽位的凭证剩余天数，要带语气色
                              （「未登录」「凭证已过期」是红的）—— 原来这里漏了色，
                              别的菜单项都有。 */}
                          <small
                            className={
                              "qb-nav-readout" +
                              (side === "claude" && readout?.[1]
                                ? " " + readout[1]
                                : "")
                            }
                          >
                            {side === "gpt"
                              ? "桌面端账户"
                              : side === "antigravity"
                                ? "Hub / IDE"
                                : (readout?.[0] ?? hint)}
                          </small>
                        </span>
                      </NavLink>
                      <div
                        className="qb-account-switch"
                        role="group"
                        aria-label="官方账户平台"
                      >
                        {SIDES.map((s) => {
                          // 「按下」与高亮说同一件事：不在账户页时三个都不亮，
                          // 读屏也别报「已按下」。
                          const on = side === s.id && inAccounts;
                          return (
                            <button
                              key={s.id}
                              type="button"
                              className={"qb-nav-sub" + (on ? " on" : "")}
                              aria-pressed={on}
                              title={s.hint}
                              onClick={() => {
                                setSession<Side>(SIDE_KEY, s.id);
                                if (location.pathname !== "/") navigate("/");
                              }}
                            >
                              {s.label}
                            </button>
                          );
                        })}
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
                      <Route path="/usage" element={<Usage />} />
                      <Route path="/relays" element={<StationCenter />} />
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
                      {/* 其余一律回首页，包括已经删掉的旧页面地址（`/onboarding`、
                          `/relays/<id>`、`/environment/*` 这一类）。 */}
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
      {/* 「发现新版本」同理只挂一份：启动检查与设置页的「检查更新」都喊它（0.25.3）。
          挂在 RecoveryGuard 外面 —— 卡在「需要完成数据恢复」时也许正需要新版。 */}
      <UpdateDialog />
      {/* 导入链接的确认框同样只挂一份（阶段 5）：点开的 qbgate://、粘贴的 ccswitch:// 都落到它。 */}
      <DeepLinkHost />
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
              key={r.key}
              onClick={() => {
                if (r.side) setSession<Side>(SIDE_KEY, r.side);
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
