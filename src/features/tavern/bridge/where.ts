/**
 * 三条桥的设置在哪（2026-09-29 第三轮起：软件页各自那张 CLI 卡的最下面，`BridgeSection.tsx`）。
 *
 * 单独一个文件：快速跳转、「酒馆」弹窗、三个账户页只要这几个名字和地址，
 * 从 `BridgeSection.tsx` 拿的话会把整套桥接界面（连接、七项、会话、消息监控、额度）拖进首页那一包 ——
 * 它们只在软件页上画，软件页是按需加载的。
 */

export type BridgeProvider = "claude" | "gpt" | "gemini";

export const BRIDGE_TITLE: Record<BridgeProvider, string> = {
  claude: "Claude 桥接设置",
  gpt: "GPT 桥接设置",
  gemini: "Gemini 桥接设置",
};

/** 挂在软件页哪张卡上 —— 指路的话里写的就是这个名字。 */
export const BRIDGE_CARD: Record<BridgeProvider, string> = {
  claude: "Claude Code",
  gpt: "Codex CLI",
  gemini: "Gemini CLI",
};

/** 到这一段的地址（软件页，自动展开）。 */
export function bridgeHref(provider: BridgeProvider): string {
  return `/software?focus=bridge-${provider}`;
}
