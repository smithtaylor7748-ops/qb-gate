import { call } from "./ipc";
import type { DeepLinkChoice } from "./generated/DeepLinkChoice";
import type { DeepLinkEntry } from "./generated/DeepLinkEntry";
import type { DeepLinkOutcome } from "./generated/DeepLinkOutcome";

export type { DeepLinkChoice, DeepLinkEntry, DeepLinkOutcome };

/**
 * 导入链接（中转站重构阶段 5，cc-switch 的 X4）。链接放在后端：冷启动时它比界面先到。
 * ⛔ 回来的形状里没有明文 Key（站点链接只给打码的）。
 */
export const deeplinkApi = {
  /** 现在待处理的那一条（读不清除）。 */
  peek: () => call<DeepLinkEntry | null>("deeplink_peek"),
  /** 「不导入」/ 显示过了：按编号清掉。 */
  ack: (id: number) => call<void>("deeplink_ack", { id }),
  /** 粘贴进来的 `qbgate://` / `ccswitch://` 链接。 */
  paste: (url: string) => call<DeepLinkEntry | null>("deeplink_paste", { url }),
  /** 确认导入。什么都不自动启用。 */
  import: (id: number, choice: DeepLinkChoice) =>
    call<DeepLinkOutcome>("deeplink_import", { id, choice }),
};
