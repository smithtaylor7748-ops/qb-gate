/**
 * 长任务的实时进度。
 *
 * 后端原来**一条 event 都不发** —— 两个接收 `AppHandle` 的命令拿到之后写的是
 * `let _ = app;`。所以「启动酒馆」这种最长 80 秒的操作，界面上只有一个
 * 「启动中…」，成功和卡死看起来一模一样。
 *
 * Rust 侧现在往 `gate://task` 发 `TaskProgress`，这里接住并按任务名分发。
 * 字段名与 `src-tauri/src/events.rs` 的 `TaskProgress` 一一对应。
 */

import { CHANNELS } from "./channels";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useSyncExternalStore } from "react";

/** 与 Rust `events::TaskProgress` 对应。任务名也在那边定义。 */
export interface TaskProgress {
  task: string;
  /** 中文短句，直接显示，不需要前端再翻译一遍。 */
  phase: string;
  step: number;
  total: number;
  log?: string | null;
  done: boolean;
  error?: string | null;
}

export type TaskName =
  | "install"
  | "upgrade"
  | "tavern-start"
  | "egress-install"
  | "dns-probe"
  | "launch-claude-code"
  | "launch-claude-desktop"
  | "launch-codex"
  | "launch-antigravity"
  | "launch-antigravity-ide"
  | "killswitch-preview"
  | "killswitch-execute"
  | "chrome-reinstall"
  | "install-codex-desktop"
  | "install-antigravity"
  | "install-gemini-cli"
  | "self-update";

export interface TaskState {
  running: boolean;
  phase: string;
  step: number;
  total: number;
  log: string[];
  error: string | undefined;
  /** 结束过至少一次。用来区分「没跑过」和「跑完了」。 */
  finished: boolean;
}

const IDLE: TaskState = {
  running: false,
  phase: "",
  step: 0,
  total: 0,
  log: [],
  error: undefined,
  finished: false,
};

const states = new Map<string, TaskState>();
const listeners = new Map<string, Set<() => void>>();

let snapshot: Array<[string, TaskState]> = [];
const allListeners = new Set<() => void>();
function notify(task: string): void {
  snapshot = Array.from(states);
  allListeners.forEach((fn) => fn());
  listeners.get(task)?.forEach((l) => l());
}

export function stateOf(task: string): TaskState {
  return states.get(task) ?? IDLE;
}

/** 日志无上限会把内存吃光；装一个大包能刷出几千行。 */
const MAX_LOG = 500;

export function applyTaskProgress(p: TaskProgress): void {
  const prev = stateOf(p.task);
  const log = p.log ? [...prev.log, p.log].slice(-MAX_LOG) : prev.log;
  states.set(p.task, {
    running: !p.done,
    // 只带日志的那种事件 `phase` 是空串（见 `events::Reporter::log`）。
    // 照抄过去就会把当前这一段的标题抹掉，界面退回「启动中…」——
    // 于是**日志刷得越勤，界面上能看见的信息越少**。空串 = 没有新标题，保留旧的。
    phase: p.phase || prev.phase,
    step: p.step,
    total: p.total,
    log,
    error: p.error ?? undefined,
    finished: p.done ? true : prev.finished,
  });
  notify(p.task);
}

/**
 * 开跑之前先清干净。
 *
 * 不清的话第二次点会看到上一次的日志和「已完成」，让人以为什么都没发生。
 */
export function resetTask(task: TaskName): void {
  states.set(task, { ...IDLE, running: true, phase: "准备中…" });
  notify(task);
}

/** 命令返回了（不管成没成），把 running 落下来 —— 万一最后一条 done 事件丢了。 */
export function endTask(task: TaskName, error?: string): void {
  const prev = stateOf(task);
  states.set(task, {
    ...prev,
    running: false,
    finished: true,
    error: error ?? prev.error,
  });
  notify(task);
}

// 全局只订阅一次。Tauri 的 listen 是异步的，先记下 promise 防止并发重复订阅。
let unlisten: Promise<UnlistenFn> | null = null;

function ensureListening(): void {
  if (unlisten) return;
  // 订阅失败不能变成未捕获的 promise 拒绝：拿不到进度事件只是进度条不动，
  // 面板其它部分照常能用，不该在控制台留一条吓人的红字。
  unlisten = listen<TaskProgress>(CHANNELS.task, (e) =>
    applyTaskProgress(e.payload),
  ).catch(() => {
    unlisten = null; // 允许下次挂载时重试
    return () => undefined;
  });
}

export function useTask(task: TaskName): TaskState {
  const subscribe = useCallback(
    (cb: () => void) => {
      ensureListening();
      let set = listeners.get(task);
      if (!set) {
        set = new Set();
        listeners.set(task, set);
      }
      set.add(cb);
      return () => {
        set.delete(cb);
      };
    },
    [task],
  );

  const getSnapshot = useCallback(() => stateOf(task), [task]);
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

export function useAllTasks() {
  return useSyncExternalStore(
    useCallback((fn: () => void) => {
      ensureListening();
      allListeners.add(fn);
      return () => {
        allListeners.delete(fn);
      };
    }, []),
    () => snapshot,
    () => snapshot,
  );
}
