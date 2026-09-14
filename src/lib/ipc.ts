import { invoke } from "@tauri-apps/api/core";

import { DEMO_ENABLED, demoCall } from "./demo";
import { toIpcError } from "./ipcError";
import { demoWorkspaceCall } from "./workspaceDemo";

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
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    // 抛的是 `IpcError`，带着 `kind`。页面做条件处理看那个，
    // 不要去匹配 `message` 里的中文（文案会改，分支会静默失效）。
    throw toIpcError(e);
  }
}

/** 演示数据：先问面板那份，没有再问工作区那份。 */
async function demoDispatch<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await demoCall<T>(cmd);
  } catch {
    return (await demoWorkspaceCall(cmd, args ?? {})) as T;
  }
}
