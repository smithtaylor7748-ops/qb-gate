/**
 * 三条桥的设置在哪（2026-09-29 第二轮）：账户页「酒馆」磁贴右上角那颗按钮，只装这一家的弹窗（`BridgeModal.tsx`）。
 *
 * 单独一个文件：快速跳转、软件页 SillyTavern 卡的「概览」只要这几个名字和地址，
 * 从 `BridgeModal.tsx` 拿的话会把整套桥接界面（连接、七项、会话、消息监控、额度）拖进别的包。
 */
import type { Side } from "../../../lib/side";

export type BridgeProvider = "claude" | "gpt" | "gemini";

export const BRIDGE_TITLE: Record<BridgeProvider, string> = {
  claude: "Claude 桥接设置",
  gpt: "GPT 桥接设置",
  gemini: "Gemini 桥接设置",
};

/** 这一家在哪个账户页 —— 指路的话里写的就是这个名字。 */
export const BRIDGE_PAGE: Record<BridgeProvider, string> = {
  claude: "官方账户 · Claude",
  gpt: "官方账户 · GPT",
  gemini: "官方账户 · 反重力",
};

/** 账户页的哪一边（跳过去之前先切到它，见 `lib/side.ts`）。 */
export const BRIDGE_SIDE: Record<BridgeProvider, Side> = {
  claude: "claude",
  gpt: "gpt",
  gemini: "antigravity",
};

/** 到这一家弹窗的地址：账户页，带 `?bridge=<家>` 进来就自动打开（`useBridgeLink`）。 */
export function bridgeHref(provider: BridgeProvider): string {
  return `/?bridge=${provider}`;
}
