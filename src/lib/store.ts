/**
 * 一个轻量数据层。零依赖，基于 `useSyncExternalStore`。
 *
 * 「切页丢状态」的根因不是没有路由，是**每个页面各自持有自己拉来的数据** ——
 * 旧代码里 `probeIp` 被三个页面各打一遍，Purity 勾的人工复核、Environment 的
 * 升级计划切页就没。加路由解决不了这个，得让数据活在页面外面。
 *
 * 四件事：
 *   1. 模块级缓存，页面卸载不清空 —— 切回来数据还在，不重新探测。
 *   2. **单一轮询器**，`document.hidden` 时自动暂停。旧代码三处 `setInterval`
 *      都没做这个判断，窗口最小化照跑。
 *   3. stale-while-revalidate：先给旧值再后台刷新，不闪回「—」。
 *   4. 三态显式分离：`loading` / `error` / `data === undefined`。
 *      旧 Dashboard 六个请求全 `.catch(() => undefined)`，加载中、无数据、
 *      请求失败三种情况在界面上长得一模一样，都是一个「—」。
 *
 * 进度持久化仍然在 Rust 那边（`progress.json`），这里不做本地乐观更新，
 * 也不用 localStorage —— 全项目保持零 localStorage。
 */

import { useCallback, useSyncExternalStore } from 'react';

export interface ResourceOptions {
  /** 轮询间隔（毫秒）。不填就不轮询。 */
  pollMs?: number;
  /** 超过这个时间算陈旧：重新挂载或窗口重新可见时后台刷新一次。 */
  staleMs?: number;
  /** 首次被订阅时是否自动拉一次。默认 true；DNS 那种慢的填 false。 */
  auto?: boolean;
}

export interface ResourceDef<T> {
  fetcher: () => Promise<T>;
  options: ResourceOptions;
}

export function res<T>(fetcher: () => Promise<T>, options: ResourceOptions = {}): ResourceDef<T> {
  return { fetcher, options };
}

export interface ResourceState<T> {
  data: T | undefined;
  /** Rust 侧抛回来的错误信息，已经是可以直接显示的中文。 */
  error: string | undefined;
  /** 正在请求中。**有旧值时也可能为真**（后台刷新）。 */
  loading: boolean;
  /** 有值但已过 staleMs。界面可以据此加个淡色提示。 */
  stale: boolean;
  /** 从没成功拉到过数据（用于区分「空」和「还没开始」）。 */
  neverLoaded: boolean;
}

interface Entry<T> {
  def: ResourceDef<T>;
  snapshot: ResourceState<T>;
  /** 最近一次**成功**的时间。失败不更新它。 */
  fetchedAt: number;
  /** 最近一次发起请求的时间，成功失败都算。用来给失败重试做节流。 */
  attemptedAt: number;
  inflight: Promise<void> | null;
  listeners: Set<() => void>;
}

const EMPTY: ResourceState<never> = {
  data: undefined,
  error: undefined,
  loading: false,
  stale: false,
  neverLoaded: true,
};

const entries = new Map<string, Entry<unknown>>();

function entryOf<T>(key: string, def: ResourceDef<T>): Entry<T> {
  let e = entries.get(key) as Entry<T> | undefined;
  if (!e) {
    e = {
      def,
      snapshot: EMPTY as ResourceState<T>,
      fetchedAt: 0,
      attemptedAt: 0,
      inflight: null,
      listeners: new Set(),
    };
    entries.set(key, e as Entry<unknown>);
  }
  return e;
}

/** 快照必须是稳定引用，所以每次变更都换一个新对象，不变更就一直是同一个。 */
function patch<T>(e: Entry<T>, next: Partial<ResourceState<T>>): void {
  e.snapshot = { ...e.snapshot, ...next };
  e.listeners.forEach((l) => l());
}

function load<T>(e: Entry<T>): Promise<void> {
  // 同一个 key 同时只有一个请求在飞 —— 三个页面同时挂载也只打一次。
  if (e.inflight) return e.inflight;

  e.attemptedAt = Date.now();
  patch(e, { loading: true });
  const p = e.def
    .fetcher()
    .then((data) => {
      e.fetchedAt = Date.now();
      patch(e, { data, error: undefined, loading: false, stale: false, neverLoaded: false });
    })
    .catch((err: unknown) => {
      // 保留旧值 —— 一次网络抖动不该把界面上已有的数字抹成空。
      patch(e, {
        error: err instanceof Error ? err.message : String(err),
        loading: false,
      });
    })
    .finally(() => {
      e.inflight = null;
    });

  e.inflight = p;
  return p;
}

/** 立刻重新拉取某个资源。返回的 Promise 在这一轮结束时 resolve。 */
export function refresh(key: string): Promise<void> {
  const e = entries.get(key);
  return e ? load(e) : Promise.resolve();
}

/**
 * 让某个资源作废。
 *
 * 有人在看就立刻重拉；没人在看就只清时间戳，下次挂载时自然会拉。
 * 破坏性操作之后调它，比各页面自己 `await refresh()` 更不容易漏。
 */
export function invalidate(...keys: string[]): void {
  for (const key of keys) {
    const e = entries.get(key);
    if (!e) continue;
    e.fetchedAt = 0;
    if (e.listeners.size > 0) void load(e);
  }
}

export function invalidateAll(): void {
  invalidate(...entries.keys());
}

// ---------------------------------------------------------------- 轮询器

/**
 * 全局唯一的心跳。每秒看一遍哪些资源到点了，**窗口不可见时整个跳过**。
 *
 * 旧代码是三个页面各自 `setInterval`，既不共享也不暂停；最小化之后
 * 后台仍然每 15 秒打一次 IP 查询接口，白白消耗第三方接口的额度。
 */
const TICK_MS = 1000;

/**
 * 从没成功过的资源，隔这么久重试一次。
 *
 * 没有这条的话，**App 这种永不卸载的订阅者会把启动那一刻的失败一直挂到关面板**：
 * `progress` 在开机断网时读失败，之后网络恢复了也永远不会再读一次，
 * 侧栏就一直显示「检查进度 0 / 5」，而用户根本不知道那是读失败不是真的 0。
 */
const ERROR_RETRY_MS = 15_000;

let timer: ReturnType<typeof setInterval> | null = null;

function tick(): void {
  if (typeof document !== 'undefined' && document.hidden) return;
  const now = Date.now();

  for (const e of entries.values()) {
    if (e.listeners.size === 0) continue;
    const { pollMs, staleMs, auto } = e.def.options;

    // 失败过且一直没拿到数据 —— 隔一阵重试。
    // 手动资源（auto: false）不在此列：DNS 探测要跑六秒，
    // 自动重试它既慢又不是用户要的。
    if (auto !== false && e.snapshot.neverLoaded && e.snapshot.error && !e.inflight) {
      if (now - e.attemptedAt >= ERROR_RETRY_MS) void load(e);
      continue;
    }

    if (pollMs && e.fetchedAt > 0 && now - e.fetchedAt >= pollMs) {
      void load(e);
      continue;
    }
    // stale 标记只在这里更新，快照才能保持稳定引用。
    if (staleMs && e.fetchedAt > 0) {
      const stale = now - e.fetchedAt >= staleMs;
      if (stale !== e.snapshot.stale) patch(e, { stale });
    }
  }
}

function onVisible(): void {
  if (document.hidden) return;
  // 回到前台先把陈旧的补一遍，不然要等下一个轮询周期。
  const now = Date.now();
  for (const e of entries.values()) {
    if (e.listeners.size === 0) continue;
    const { pollMs, staleMs } = e.def.options;
    const age = now - e.fetchedAt;
    if (e.fetchedAt > 0 && ((pollMs && age >= pollMs) || (staleMs && age >= staleMs))) {
      void load(e);
    }
  }
}

function ensureTimer(): void {
  if (timer !== null) return;
  timer = setInterval(tick, TICK_MS);
  if (typeof document !== 'undefined') {
    document.addEventListener('visibilitychange', onVisible);
  }
}

// ---------------------------------------------------------------- Hook

/**
 * 订阅一个资源。
 *
 * `def` 用 `resources.ts` 里集中声明的那份，不要在页面里现写 —— 集中声明才能
 * 保证同一个 key 在所有页面用的是同一个 fetcher 和同一套轮询策略。
 */
export function useResource<T>(key: string, def: ResourceDef<T>): ResourceState<T> & {
  refresh: () => Promise<void>;
} {
  const subscribe = useCallback(
    (cb: () => void) => {
      const e = entryOf(key, def);
      e.listeners.add(cb);
      ensureTimer();

      // 首次订阅且从没拉过 —— 开一炮。已经有值就交给轮询与 stale 逻辑。
      const auto = e.def.options.auto !== false;
      if (auto && e.fetchedAt === 0 && !e.inflight) void load(e);

      return () => {
        e.listeners.delete(cb);
      };
    },
    // def 每次渲染都是新对象引用，但内容恒定；只按 key 订阅。
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [key]
  );

  const getSnapshot = useCallback(
    () => entryOf(key, def).snapshot,
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [key]
  );

  const state = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  const doRefresh = useCallback(() => refresh(key), [key]);

  return { ...state, refresh: doRefresh };
}

/** 不订阅，直接读当前缓存值。用在事件处理里（比如按钮点下去要看一眼当前 IP）。 */
export function peek<T>(key: string): T | undefined {
  return entries.get(key)?.snapshot.data as T | undefined;
}

/**
 * 把命令返回的全量新状态直接写进缓存，省掉一次回查。
 *
 * `progress_set` 就是这种：它返回的是整个 `Progress`，前端直接替换即可 ——
 * 这也是档案里定的规矩，进度的真相在 Rust，前端不做本地乐观更新。
 */
export function put<T>(key: string, data: T): void {
  const e = entries.get(key) as Entry<T> | undefined;
  if (!e) return;
  e.fetchedAt = Date.now();
  patch(e, { data, error: undefined, loading: false, stale: false, neverLoaded: false });
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
  const subscribe = useCallback((cb: () => void) => {
    let set = sessionListeners.get(key);
    if (!set) {
      set = new Set();
      sessionListeners.set(key, set);
    }
    set.add(cb);
    return () => {
      set.delete(cb);
    };
  }, [key]);

  const getSnapshot = useCallback(
    () => (session.has(key) ? (session.get(key) as T) : initial),
    [key, initial]
  );

  const value = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  const setValue = useCallback(
    (v: T) => {
      session.set(key, v);
      sessionListeners.get(key)?.forEach((l) => l());
    },
    [key]
  );

  return [value, setValue];
}
