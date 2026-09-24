import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

import { DEMO_ENABLED, demoCall, MissingDemoCommand } from "./demo";
import { toIpcError } from "./ipcError";
import { demoWorkspaceCall } from "./workspaceDemo";

/**
 * 还没回来的命令：命令名 → 发出去的时刻。
 *
 * # ⛔ 为什么不是「加个超时把它取消掉」
 *
 * 后端的 `operations::exclusive()` 是一把**无界**的 tokio Mutex。排在一个长安装
 * 后面的命令会等到那个安装结束为止 —— 可能是几分钟，也可能是永远
 * （0.22.6–0.27.0 托管安装那次死锁就是永远，坑 7.51）。
 *
 * 前端加一个「N 秒就超时」看着很对，其实**更糟**：`invoke` 没有取消机制，
 * 超时只是把 Promise 丢掉，后端那条命令照跑。于是 `busy` 提前清空、按钮亮回来，
 * 使用者可以再点一次 —— 我们要修的「两个互斥操作同时在跑」反而被制造出来了。
 *
 * 真正的病是**沉默**：界面既不动，也不说它在等什么。所以这里只做记账，
 * 由界面把「还在等」显示出来（[`usePending`]），一个命令都不取消。
 * 不让它们排上队是另一半，那归页面共用同一个 busy。
 */
const pending = new Map<string, number>();
const pendingListeners = new Set<() => void>();

function notePending(cmd: string, started: number | null) {
  const key = `${cmd}#${started ?? ""}`;
  if (started === null) {
    for (const k of [...pending.keys()])
      if (k.startsWith(`${cmd}#`)) pending.delete(k);
  } else {
    pending.set(key, started);
  }
  for (const fn of pendingListeners) fn();
}

/**
 * 有没有命令已经等了超过 `afterMs`，等了多久。
 *
 * 界面拿它把「我在等后端」说出口。返回 `null` 表示没有慢调用。
 */
export function usePending(
  afterMs = 12_000,
): { cmd: string; ms: number } | null {
  const [, tick] = useState(0);
  useEffect(() => {
    const onChange = () => tick((n) => n + 1);
    pendingListeners.add(onChange);
    // 没有新事件时也要走针 —— 「已经等了多久」是随时间变的。
    const timer = setInterval(onChange, 1000);
    return () => {
      pendingListeners.delete(onChange);
      clearInterval(timer);
    };
  }, []);
  const now = Date.now();
  let worst: { cmd: string; ms: number } | null = null;
  for (const [key, started] of pending) {
    const ms = now - started;
    if (ms < afterMs) continue;
    if (!worst || ms > worst.ms) worst = { cmd: key.split("#")[0], ms };
  }
  return worst;
}

/**
 * 跟后端说话的**唯一**入口。
 *
 * # 为什么只该有一个
 *
 * 拆之前有两个：`api.ts` 一个、`workspace.ts` 一个，各自写了一遍
 * 「demo 模式怎么绕开」「错误怎么转」。两份就会分叉 —— E1 给错误加分类时
 * 就得往两个地方各补一次，而漏掉一个不会有任何报错，只是那半边的页面
 * 拿不到 `kind`。
 *
 * # 演示模式
 *
 * `VITE_DEMO` 不等于 `1` 时 `DEMO_ENABLED` 是编译期常量 `false`，
 * 整个分支连同两个 fixture 模块一起被摇掉 —— **安装包里的数据来源只有
 * Rust 一处**，没有第二条路径。
 *
 * 两份 fixture 暂时还是两个文件（一份给面板页、一份给工作区页），
 * 但**派发只有这一处**：找不到就报同一句话，不会出现「一边说没有数据、
 * 另一边静默返回 undefined」。
 */
export async function call<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (DEMO_ENABLED) return demoDispatch<T>(cmd, args);
  notePending(cmd, Date.now());
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    // 抛的是 `IpcError`，带着 `kind`。页面做条件处理看那个，
    // 不要去匹配 `message` 里的中文（文案会改，分支会静默失效）。
    throw toIpcError(e);
  } finally {
    notePending(cmd, null);
  }
}

/** 演示数据：先问面板那份，没有再问工作区那份。 */
async function demoDispatch<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await demoCall<T>(cmd, args);
  } catch (error) {
    if (!(error instanceof MissingDemoCommand)) throw error;
    return (await demoWorkspaceCall(cmd, args ?? {})) as T;
  }
}
