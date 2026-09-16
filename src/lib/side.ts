/**
 * 总览分成 Claude / GPT 两边，现在**由侧栏切**。
 *
 * 0.20.0 之前是页面顶上一条两格的页签条，占 61px；使用者要把它并进主菜单栏。
 * 于是这里只剩一个会话态的键：侧栏写它，`Home` 读它。
 *
 * 放在 `lib/` 而不是 `features/overview/`：侧栏（`Shell`）也要用，
 * 而 `Shell` 往 `overview` 里 import 一个状态键会把两边绑死 ——
 * 下一个人挪 `Home` 的时候就会带出一个莫名其妙的 import 环。
 */

export type Side = "claude" | "gpt";

/** 会话态的键。`Home` 与侧栏读写的是同一个。 */
export const SIDE_KEY = "home.side";

export const SIDES: Array<{ id: Side; label: string; hint: string }> = [
  {
    id: "claude",
    label: "Claude",
    hint: "槽位 · Claude Code / 桌面端 / 酒馆",
  },
  { id: "gpt", label: "Codex", hint: "Codex 桌面端 · 账户登录与用量" },
];
