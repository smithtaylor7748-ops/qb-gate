/**
 * 侧栏每一行右侧的实时读数。
 *
 * 这是 v0.11.0 侧栏最强的一块，重构时丢了。它比一个静态的风险度 pill
 * 信息量大得多 —— 那个只有四种词（未检测／通过／注意／高危），
 * 而这里直接告诉你「10/10 已锁」「剩 18 天」「23/100」。
 *
 * ⚠ 新加的 `.qb-nav-readout` **必须**一起进 `workspace.css` 里
 * `@media (max-width:1020px)` 那条 `display:none` 名单：那个断点把
 * `.qb-sidebar nav a > span` 整个藏掉、侧栏收成 72px 图标条，读数留在外面
 * 会把 72px 撑爆，`test:ui` 的三档窄屏溢出断言当场全红。
 */
import { useMemo } from "react";
import { R } from "../lib/resources";
import { useResource } from "../lib/store";
import { fmtDaysLeft } from "../ui";

export type Tone = "" | "ok" | "warn" | "danger";
export type Readout = [string, Tone];

/** 键是 `NAV` 里的 path。没有读数的项（设置）不给，避免六行全是数字变成噪音。 */
export function useSummaries(): Record<string, Readout> {
  const accounts = useResource("accounts", R.accounts);
  const software = useResource("software", R.software);
  const plugins = useResource("plugins", R.plugins);

  return useMemo(() => {
    const out: Record<string, Readout> = {};

    // ---- 官方账户：当前槽位的凭证剩余天数
    const active = accounts.data?.slots.find((s) => s.active);
    if (active) {
      if (!active.logged_in) out["/"] = ["未登录", "danger"];
      else {
        const d = active.cli_days_left;
        out["/"] = [
          fmtDaysLeft(d),
          d === null || d === undefined
            ? ""
            : d < 0
              ? "danger"
              : d < 5
                ? "warn"
                : "ok",
        ];
      }
    } else if (accounts.data) out["/"] = ["无槽位", "warn"];

    // ---- 软件：Claude 两样 + 反重力（0.26.0）。读数只说 Claude 那两样装没装，
    // 反重力装了就在后面加一个字，没装不算「缺」—— 它不是每个人都要的。
    if (software.data) {
      const cc = software.data.claudeCode.installed;
      const cd = software.data.claudeDesktop.installed;
      const ag = software.data.antigravity.installed;
      out["/software"] =
        cc && cd
          ? [ag ? "都已装 +AG" : "都已装", "ok"]
          : cc || cd
            ? ["缺一个", "warn"]
            : ["未安装", "warn"];
    }

    // ---- 扩展：酒馆是这台机器上唯一在用的那个
    const tavern = plugins.data?.[0];
    if (tavern) {
      out["/extensions"] = [
        tavern.state === "running"
          ? "运行中"
          : tavern.state === "ready"
            ? "就绪"
            : "依赖不齐",
        tavern.state === "running" || tavern.state === "ready" ? "ok" : "",
      ];
    }

    return out;
  }, [accounts.data, software.data, plugins.data]);
}
