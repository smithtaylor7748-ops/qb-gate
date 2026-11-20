/**
 * 安全项小窗开着哪一项。单独一个文件：`SecuritySheet` 要引 `objects.tsx`，
 * 而 `objects.tsx` 里那几块（浏览器助手的跳转）要能把小窗关掉 —— 放在 `SecuritySheet` 里就成环了。
 */
import { setSession } from "../../lib/store";
import type { ObjectId } from "./objects";

export const SHEET_KEY = "security.sheet";

/** 打开某一项安全对象的小窗。不在组件里也能调。 */
export function openSecurity(id: ObjectId): void {
  setSession<ObjectId | null>(SHEET_KEY, id);
}

/** 关掉小窗（跳去别的页面之前）。 */
export function closeSecurity(): void {
  setSession<ObjectId | null>(SHEET_KEY, null);
}
