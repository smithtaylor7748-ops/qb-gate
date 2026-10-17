/**
 * GPT / Gemini 两条面板内置桥的差异，集中在这一张表里（2026-09-29 从 `CliBridgeSettings.tsx` 抽出来）。
 * 组件里别再写 `provider === "gpt" ? … : …` 的文案。
 */
export type CliProvider = "gpt" | "gemini";

export const META = {
  gpt: {
    name: "GPT 桥接",
    eyebrow: "OpenAI · ChatGPT / Codex",
    drives:
      "面板内置，每个请求驱动一次官方 codex exec，用当前激活的 GPT 槽位里官方客户端自己完成的登录。",
    cliLabel: "官方 Codex CLI",
    cliWhere: "到「软件」页装 Codex，或 npm i -g @openai/codex",
    slotWhere: "「官方账户 · GPT」",
    portKey: "gpt_bridge_port",
    modelKey: "gpt_model",
    promptKey: "gpt_system_prompt",
    modelLabel: "模型（留空用 Codex 默认）",
    modelHint: "例如 gpt-5.6-sol",
    defaultPort: 5002,
    howTo: "酒馆那边怎么配",
    testTitle: "ChatGPT / Codex 角色扮演链路",
  },
  gemini: {
    name: "Gemini 桥接",
    eyebrow: "Google · Gemini CLI",
    drives:
      "面板内置，每个请求驱动一次官方 Gemini CLI 的无交互模式，用当前激活账户 CLI 那一半的登录。",
    cliLabel: "官方 Gemini CLI",
    cliWhere: "到「软件」页点「用 npm 安装」",
    // 0.32.0：两半同属一条账户槽位了，指路也跟着改。
    slotWhere: "「官方账户 · 反重力」里那条账户的 CLI 那一半",
    portKey: "gemini_bridge_port",
    modelKey: "gemini_model",
    promptKey: "gemini_system_prompt",
    modelLabel: "模型（留空用 CLI 默认）",
    modelHint: "例如 gemini-2.5-pro",
    defaultPort: 5003,
    howTo: "酒馆那边怎么配 · 它是什么、不是什么",
    testTitle: "Gemini CLI 角色扮演链路",
  },
} as const;

/** 「几分后重置」。 */
export function resetLabel(epoch: number | null, now = Date.now()) {
  if (!epoch) return "重置时间未知";
  const seconds = Math.max(0, epoch - Math.floor(now / 1000));
  if (seconds < 60) return "即将重置";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 24) return `${Math.floor(hours / 24)} 天后重置`;
  if (hours > 0) return `${hours} 小时 ${minutes} 分后重置`;
  return `${minutes} 分后重置`;
}
