/**
 * 「这个软件装没装、是哪一版」在界面上怎么说 —— **全项目一份**。
 *
 * # ⛔ 为什么要抽出来
 *
 * 0.28.0 之前软件页七张卡各写各的：Pill 读 `installed`、版本行读 `version`，
 * 两个字面量分散在七处。于是同一张卡上可以同时出现
 *
 * * Pill「已装」 + 版本行「未安装」（Claude 桌面端：目录在、没有 `app-*` 子目录）；
 * * Pill「未安装」 + 版本行一个真版本号（Codex 桌面端：Store 包在册、找不到 exe）；
 * * Pill「已装」 + 版本行「未安装」（Gemini CLI：入口在、`package.json` 读不出）。
 *
 * 三处都不报错，只是**在同一块屏幕上说两句互相矛盾的话**。0.28.0 只修了 Codex CLI
 * 那一张（CHANGELOG 有记），剩下几张是同一个形状 —— 一张一张修必然再漏。
 *
 * # 三档，不是两档
 *
 * 「拿不到版本」和「没装」**必须分开**（§7.20 那条教训：`decide()` 把
 * `None` 一律判成 `FreshInstall`，于是一个 218 MB 的 `claude.exe` 好端端躺在盘上，
 * 界面写着「未安装」，把排查方向从 ACL 带到了安装路径）。
 *
 * | `installed` | `version` | 显示 |
 * |---|---|---|
 * | false | — | 未安装 |
 * | true | 有 | 那个版本号 |
 * | true | 无 | **已装 · 版本读不出** |
 */

import type { Software } from "./generated/Software";

/** 没装。 */
export const NOT_INSTALLED = "未安装";
/** 装了，但版本号读不出来 —— 不是「没装」，也不是空白。 */
export const VERSION_UNREADABLE = "已装 · 版本读不出";
/** 连「装没装」都还没读到（资源还没回来）。 */
export const NOT_YET_READ = "还没读到";

/**
 * 「版本」那一行显示什么。
 *
 * `sw` 是 `undefined` 表示后端那份报告还没回来 —— 那是第四档，
 * **不许折成「未安装」**（同一条规矩：没查不显示成没问题）。
 */
export function versionLine(sw: Software | undefined | null): string {
  if (!sw) return NOT_YET_READ;
  if (!sw.installed) return NOT_INSTALLED;
  return sw.version ?? VERSION_UNREADABLE;
}

/**
 * Pill 上那一小段。跟 [`versionLine`] **同源**，所以两者不可能互相矛盾。
 *
 * 装了且知道版本时只显示版本号（Pill 很窄）；装了但读不出版本时显示「已装」。
 */
export function pillLabel(sw: Software | undefined | null): string {
  if (!sw) return NOT_YET_READ;
  if (!sw.installed) return NOT_INSTALLED;
  return sw.version ?? "已装";
}

/** Pill 的色调。装了就是 ok，没装是中性 —— 「没装」不是错误。 */
export function pillTone(
  sw: Software | undefined | null,
): "ok" | "warn" | "default" {
  if (!sw) return "warn";
  return sw.installed ? "ok" : "default";
}
