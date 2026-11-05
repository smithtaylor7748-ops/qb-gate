/**
 * 软件页 SillyTavern 卡那几个分页共用的小东西（2026-09-29 第二轮重构时抽出来的）。
 */
import { open as openDialog } from "@tauri-apps/plugin-dialog";

/** 错误对象 → 能直接显示的一句话（后端的错误文案本来就是写给人看的，原样给）。 */
export function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/**
 * 软件页 SillyTavern 卡的七个分页。顺序 = 使用者最常来的在前。
 * 记住的值不在这张表里（改过名的、删掉的），`isTavernTab` 不认，自然回到「概览」。
 */
export const TAVERN_TABS = [
  { id: "overview", label: "概览" },
  { id: "versions", label: "版本" },
  { id: "console", label: "控制台" },
  { id: "characters", label: "角色卡" },
  { id: "worlds", label: "世界书" },
  { id: "extensions", label: "扩展" },
  { id: "data", label: "备份与回收站" },
] as const;

export type TavernTab = (typeof TAVERN_TABS)[number]["id"];

export function isTavernTab(v: string | null | undefined): v is TavernTab {
  return TAVERN_TABS.some((t) => t.id === v);
}

/** 记住上次停在哪一页 —— 只是这台机器上的顺手，读写失败就当没有（无痕窗口、禁了存储）。 */
const TAB_KEY = "qb-tavern-tab";
export function rememberedTab(): TavernTab | null {
  try {
    const v = localStorage.getItem(TAB_KEY);
    return isTavernTab(v) ? v : null;
  } catch {
    return null;
  }
}
export function rememberTab(tab: TavernTab): void {
  try {
    localStorage.setItem(TAB_KEY, tab);
  } catch {
    /* 存不了就算了 */
  }
}

/**
 * 选几个文件（多选）。取消了是空数组。
 *
 * 只收路径、不读内容 —— 读、校验、复制全在后端（`tavern_library`），这里拿不到文件的字节。
 */
export async function pickFiles(
  name: string,
  extensions: string[],
): Promise<string[]> {
  try {
    const picked = await openDialog({
      multiple: true,
      directory: false,
      filters: [{ name, extensions }],
    });
    if (!picked) return [];
    return Array.isArray(picked) ? picked : [picked];
  } catch {
    return [];
  }
}

/** 名字 / 作者 / 标签里有没有这几个字（不分大小写）。 */
export function matches(
  query: string,
  ...fields: (string | string[])[]
): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return fields.flat().some((f) => f.toLowerCase().includes(q));
}
