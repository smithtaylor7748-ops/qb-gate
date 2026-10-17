/**
 * 酒馆那几个分页共用的小东西（2026-09-29 重构时抽出来的）。
 *
 * 分页原来在软件页的 SillyTavern 卡上；同一天第三轮使用者说「软件页面的应该在这个官方账户弹窗页面」，
 * 搬进了账户页「酒馆」磁贴右上角那个弹窗（`TavernPanel.tsx`）。「版本」留在软件页，不是分页了。
 */
import { open as openDialog } from "@tauri-apps/plugin-dialog";

/** 错误对象 → 能直接显示的一句话（后端的错误文案本来就是写给人看的，原样给）。 */
export function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/**
 * 「酒馆」弹窗的分页。顺序 = 使用者最常来的在前。
 *
 * 没有「版本」：下载安装 / 更新 / 指定版本 / 卸载是「软件」的事，留在软件页的 SillyTavern 卡（使用者选的）。
 * 以前记住的若是 `versions`，`isTavernTab` 不认，自然回到「概览」。
 */
export const TAVERN_TABS = [
  { id: "overview", label: "概览" },
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
