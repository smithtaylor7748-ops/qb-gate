/**
 * 五项检查的进度记录。接替 `NavCtx` 的 `progress` / `mark` 两个字段。
 *
 * 它们原来挂在 context 上，但根本不需要 —— `mark` 只是调一个模块级函数再
 * `put('progress', …)`，`progress` 直接 `useResource('progress', R.progress)`
 * 就能读。挂在 context 上的唯一后果是每个用到它的组件都得活在 Provider 底下。
 *
 * ⚠ `steps.ts` 里那五个 id 与 `src-tauri/src/progress.rs::STEP_IDS` 和使用者机器上
 * 已有的 `progress.json` 双向绑定，**不许改名** —— 改了老用户的进度全丢。
 * 这次去掉的只是把它们呈现成 ①②③④⑤ 向导的那层 UI。
 */
import { api, type Progress, type Risk, type StepState } from "./api";
import { R } from "./resources";
import { put, useResource } from "./store";
import type { StepId } from "./steps";

const EMPTY: Progress = { steps: {}, completed_once: false };

/** 当前进度。没读到就给一份空的，调用方不用处理 undefined。 */
export function useProgress(): Progress {
  return useResource("progress", R.progress).data ?? EMPTY;
}

/**
 * 记录某一步的结果，写进 Rust 侧的 progress.json。
 *
 * 返回后整份 `Progress` 被替换进缓存 —— `score.ts::purityItem` 的 35 分权重
 * 完全建立在这条路径上（纯净度的结论只能人工给，面板不替使用者判定）。
 */
export async function markStep(
  id: StepId,
  state: StepState,
  risk: Risk,
  detail: string,
): Promise<void> {
  put("progress", await api.progressSet(id, state, risk, detail));
}
