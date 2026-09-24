/** Backend state is owned by TanStack Query. Drafts remain in the separate session store. */
import { useCallback, useSyncExternalStore } from "react";
import { QueryClient, useQuery } from "@tanstack/react-query";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: false,
      refetchOnWindowFocus: false,
      refetchOnReconnect: false,
      gcTime: Infinity,
    },
    mutations: { retry: false },
  },
});
export interface ResourceOptions {
  pollMs?: number;
  staleMs?: number;
  auto?: boolean;
  /**
   * 把结果留到下次开面板。**只给慢的手动检测用**（dns / signals / checkup）。
   *
   * 不留的后果是实打实的：那几项是 `auto:false`，不点不跑，而结果只活在
   * 内存缓存里 —— 面板一重启就没了，综合评分里 DNS 25 分加中文环境 20 分
   * 每次开面板都从「未检测」重新开始，昨天测过也白测。
   *
   * ⚠ **不许给 `ip` / `purity` / `gate` 用。** 出口 IP 是门禁判定的依据，
   * 把上一次的 IP 端出来当当前值，人会照着一个已经不成立的前提做决定。
   * 留的这三项都是「环境长什么样」，慢、且不随网络秒变。
   */
  persist?: boolean;
}
export interface ResourceDef<T> {
  fetcher: () => Promise<T>;
  options: ResourceOptions;
}
export interface ResourceState<T> {
  data: T | undefined;
  error: string | undefined;
  loading: boolean;
  stale: boolean;
  neverLoaded: boolean;
}
const definitions = new Map<string, ResourceDef<unknown>>();
// IPC cannot be aborted. Invalidated measurements must not return to disk later.
const generations = new Map<string, number>();

// ------------------------------------------------------- 跨重启的结果缓存

const CACHE_PREFIX = "qb.cache.";
/** key → 这份数据是什么时候测出来的（毫秒）。界面必须把它显示出来。 */
const measured = new Map<string, number>();
/** 已经试过从 localStorage 取的 key，避免每次渲染都读一遍。 */
const hydrated = new Set<string>();

/** 这份数据测于何时。`null` = 没有记录（本次会话里也还没跑过）。 */
export function measuredAt(key: string): number | null {
  return measured.get(key) ?? null;
}

/**
 * 第一次用到某个持久化资源时，把上次的结果塞回缓存。
 *
 * 存取一律 try/catch：无痕窗口、清过站点数据、或者 WebView 禁了存储时
 * `localStorage` 本身就会抛，**读不到要当没有，不能让整页崩掉**。
 */
function hydrate(key: string, def: ResourceDef<unknown>): void {
  if (!def.options.persist || hydrated.has(key)) return;
  hydrated.add(key);
  try {
    const raw = localStorage.getItem(CACHE_PREFIX + key);
    if (!raw) return;
    const saved = JSON.parse(raw) as { at: number; data: unknown };
    if (typeof saved?.at !== "number" || saved.data === undefined) return;
    measured.set(key, saved.at);
    // 只在缓存还空着时塞 —— 本次会话已经跑过的话，那份才是新的。
    if (queryClient.getQueryData([key]) === undefined) {
      queryClient.setQueryData([key], saved.data);
    }
  } catch {
    // 读不到就当没有。
  }
}

/** 包一层，成功的结果顺手落盘并记下时刻。 */
function fetcherFor<T>(key: string, def: ResourceDef<T>): () => Promise<T> {
  if (!def.options.persist) return def.fetcher;
  return async () => {
    const generation = generations.get(key) ?? 0;
    const data = await def.fetcher();
    if ((generations.get(key) ?? 0) !== generation) return data;
    const at = Date.now();
    measured.set(key, at);
    try {
      localStorage.setItem(CACHE_PREFIX + key, JSON.stringify({ at, data }));
    } catch {
      // 写不进去不影响本次显示，下次重启退回「未检测」而已。
    }
    return data;
  };
}
export function res<T>(
  fetcher: () => Promise<T>,
  options: ResourceOptions = {},
): ResourceDef<T> {
  return { fetcher, options };
}
export function useResource<T>(
  key: string,
  def: ResourceDef<T>,
): ResourceState<T> & { refresh: () => Promise<void> } {
  definitions.set(key, def);
  hydrate(key, def as ResourceDef<unknown>);
  const query = useQuery({
    queryKey: [key],
    queryFn: fetcherFor(key, def),
    enabled: def.options.auto !== false,
    staleTime: def.options.staleMs ?? Infinity,
    refetchInterval: def.options.auto !== false ? def.options.pollMs : false,
    refetchIntervalInBackground: false,
  });
  return {
    data: query.data,
    error: query.error ? String(query.error) : undefined,
    loading: query.isFetching,
    stale: query.isStale,
    neverLoaded: query.data === undefined,
    refresh: useCallback(() => refresh(key), [key]),
  };
}
/**
 * 重新取一份。
 *
 * # ⛔ 定义依赖组件 state 时，必须把新定义显式传进来
 *
 * `definitions.set(key, def)` 发生在 `useResource` **渲染时**。而 `setState`
 * 排的是下一次渲染 —— 所以
 *
 * ```ts
 * setChannel("stable");
 * void upgrade.refresh();   // ❌ 注册表里还是 latest 那份定义
 * ```
 *
 * 取回来的是**旧渠道**的结果，然后当成新渠道的显示出去。软件页的升级渠道
 * 下拉就是这么错了一整版：选 stable、查的是 latest、界面写着 stable 的读数。
 * 这跟 `resources.ts` 里 `upgradeOf` 那段注释警告的是同一件事，只是低了一层。
 *
 * 所以要在同一个事件里换定义又重取，走 `refresh(key, R.xxxOf(新值))`。
 * 传进来的定义会顺手写回注册表，后续的 `invalidate` / 自动重取才跟得上。
 */
export async function refresh<T>(
  key: string,
  override?: ResourceDef<T>,
): Promise<void> {
  if (override) definitions.set(key, override as ResourceDef<unknown>);
  const def = definitions.get(key);
  if (!def) return;
  // fetchQuery joins an in-flight request. Cancelling here makes two callers
  // refreshing the same resource reject each other with CancelledError.
  await queryClient.fetchQuery({
    queryKey: [key],
    queryFn: fetcherFor(key, def),
    staleTime: 0,
  });
}
/**
 * 破坏性操作之后作废这些资源。
 *
 * # ⛔ 手动资源是**扔掉**，不是「标记过期」
 *
 * `auto:false` 的资源不会自动重取。老代码对它们只调 `invalidateQueries`
 * 加 `refetchType:'none'` —— 结果是**什么都没发生**：那份旧数据原样留在
 * 界面上继续冒充当前值。
 *
 * 实际长出来的样子（都在软件页）：升级装完了，版本栏还写着「可升级 →
 * 2.1.x」；Chrome 清空重装完了，隐私审计还列着刚被删掉的那几个扩展的
 * 高危权限。两处都不报错，只是**在说一件已经不成立的事**。
 *
 * 手动资源的答案是一次**测量**。破坏性操作之后，上一次测量既不是
 * 「当前值」也不是「过期的当前值」，它就是没了 —— 界面该退回
 * 「还没跑过」，跟「没查」不显示成「没问题」是同一条规矩。
 *
 * 持久化的那几份（dns / signals / checkup）连磁盘上那份一起扔：
 * 留着的话下次开面板又会把它水化回来，等于作废没作废。
 *
 * ⚠ 想要的是「立刻重测一遍」而不是「退回没测过」时，**别用这个** ——
 * 直接调那个资源的 `refresh()`。软件页的出站锁规则表与 Chrome 隐私审计
 * 就是这种：使用者正盯着那一块，扔掉会让它当场空一下。
 */
export function invalidate(...keys: string[]): void {
  for (const key of keys) {
    generations.set(key, (generations.get(key) ?? 0) + 1);
    const manual = definitions.get(key)?.options.auto === false;
    // Query cancellation takes effect synchronously. Finish invalidating in this
    // turn too: a deferred removeQueries would cancel a subsequent refresh().
    void queryClient.cancelQueries({ queryKey: [key], exact: true });
    if (manual) {
      queryClient.removeQueries({ queryKey: [key], exact: true });
      measured.delete(key);
      hydrated.delete(key);
      try {
        localStorage.removeItem(CACHE_PREFIX + key);
      } catch {
        // 存储读写不了就算了，内存里那份已经扔掉了。
      }
      continue;
    }
    void queryClient.invalidateQueries({
      queryKey: [key],
      exact: true,
      refetchType: "active",
    });
  }
}
export function invalidateAll(): void {
  invalidate(...definitions.keys());
}
/** Background workspace notifications refresh live state, not manual diagnostics.
 * They must neither erase measurements nor interrupt a check already in flight.
 * Actual mutations still use invalidate(keys) to discard affected measurements.
 */
export function invalidateAutomatic(): Promise<void> {
  return queryClient.invalidateQueries(
    {
      predicate: (query) => {
        const def = definitions.get(String(query.queryKey[0]));
        return !!def && def.options.auto !== false;
      },
      refetchType: "active",
    },
    { cancelRefetch: false },
  );
}
export function peek<T>(key: string): T | undefined {
  return queryClient.getQueryData<T>([key]);
}
export function put<T>(key: string, data: T): void {
  generations.set(key, (generations.get(key) ?? 0) + 1);
  void queryClient.cancelQueries({ queryKey: [key], exact: true });
  queryClient.setQueryData([key], data);
}

// ---------------------------------------------------------------- 会话态

/**
 * 页面内的临时判定，也得活在页面外面。
 *
 * Purity 的「两家均通过」勾选、Environment 的「以前登录过吗」——
 * 旧代码是纯 `useState`，切页就丢，用户勾完走开再回来又得重勾一遍。
 * 这些不该写进 `progress.json`（那是 Rust 管的正式进度），
 * 但也不该活不过一次导航，所以放在这里：**活到面板关闭为止**。
 */
const session = new Map<string, unknown>();
const sessionListeners = new Map<string, Set<() => void>>();

export function useSession<T>(key: string, initial: T): [T, (v: T) => void] {
  if (!session.has(key)) session.set(key, initial);
  const subscribe = useCallback(
    (cb: () => void) => {
      let set = sessionListeners.get(key);
      if (!set) {
        set = new Set();
        sessionListeners.set(key, set);
      }
      set.add(cb);
      return () => {
        set.delete(cb);
      };
    },
    [key],
  );

  const getSnapshot = useCallback(
    () => (session.has(key) ? (session.get(key) as T) : initial),
    [key, initial],
  );

  const value = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  const setValue = useCallback((v: T) => setSession(key, v), [key]);

  return [value, setValue];
}

/**
 * 不在组件里也能写会话态。
 *
 * 给「请求打开某个全局对话框」这类场合用：总览、账户页、托盘事件都能喊一声
 * `requestSwitch(label)`，对话框本身只挂一份（`pages/accounts/SwitchFlow.tsx`）。
 * 原来两个页面各挂一份切换对话框，文案和逻辑已经各走各的了。
 */
export function setSession<T>(key: string, v: T): void {
  session.set(key, v);
  sessionListeners.get(key)?.forEach((l) => l());
}

/**
 * 读会话态的当前值，**绕开 render 快照**。
 *
 * `useSession` 给出的是本次渲染那一刻的值。要拿它当互斥锁用（「已经有一项
 * 检测在跑就别再起一项」）就必须读实时值 —— 串行跑第二项时闭包里那份还是
 * 上一次渲染的旧值，用它判定等于没判。
 */
export function getSession<T>(key: string, fallback: T): T {
  return session.has(key) ? (session.get(key) as T) : fallback;
}
