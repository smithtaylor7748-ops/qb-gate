/**
 * 同一份 `TavernConfig` 现在有三处在改（2026-09-29）：软件页的 SillyTavern 卡（路径与酒馆端口）、
 * Claude 弹窗（bridge.py 目录与端口）、GPT / Gemini 弹窗（各自的端口、模型、提示词……）。
 *
 * ⛔ 每处只存自己那几个字段：先读最新的一份，只覆盖这次改了的键再存。拿整份旧草稿去存，
 * 会把别处刚改好的值盖回去 —— 而且没有任何报错，只是「刚才明明保存了」。
 */
import { api } from "../../lib/api";
import type { TavernConfig } from "../../lib/generated/TavernConfig";
import { invalidate } from "../../lib/store";

export type TavernPatch = Partial<TavernConfig>;

export async function saveTavernFields(patch: TavernPatch): Promise<void> {
  const latest = await api.tavernConfig();
  await api.tavernConfigSave({ ...latest, ...patch });
  invalidate("tavernConfig", "plugins", "tavernSetup");
}

/** 这处草稿里改过的值；没改过就用读回来的那份。 */
export function pick<K extends keyof TavernConfig>(
  draft: TavernPatch,
  saved: TavernConfig | null | undefined,
  key: K,
): TavernConfig[K] | undefined {
  return key in draft ? (draft[key] as TavernConfig[K]) : saved?.[key];
}
