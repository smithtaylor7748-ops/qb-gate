/**
 * 浏览器助手的三个旧入口（2026-11-19）：「中文环境」「执行锁」两个小窗、查套路的「使用 Chrome 已登录账户」。
 * 原来各挂一整张配对卡；现在只有一句说明 + 跳到「扩展 → 浏览器助手」，外加一行连接结果（查套路要看它）。
 *
 * 从小窗跳走之前先把小窗关掉：小窗是全局只挂一份的，不关的话它会盖在新页面上。
 */
import { useNavigate } from "react-router-dom";
import { ArrowRight } from "lucide-react";
import { api } from "../../lib/api";
import { res, useResource } from "../../lib/store";
import { Button } from "../../ui";
import { closeSecurity } from "../security/sheetState";
import { BROWSER_COMPANION_PATH } from "./where";

// 跟浏览器助手页同一个缓存键、同一个读法（那一页挂着时两边看到的是同一份）。
const linkResource = res(api.browserCompanionStatus, { pollMs: 2000 });

const WHAT: Record<"signals" | "lock" | "billing", string> = {
  signals:
    "装了浏览器助手插件的话，在插件弹窗里点「检查当前浏览器」，就是在你真正登 Claude 的那个浏览器里测；结果回到这里。",
  lock: "IP 锁管的是这台电脑上的 Claude 程序。浏览器里的 Claude 网页由浏览器助手插件管：开启保护后，面板没确认出口 IP 时插件拦下受保护的网站。",
  billing:
    "要先在 Chrome 里装好浏览器助手插件、连上面板。装法与连接都在「扩展 → 浏览器助手」。",
};

export default function BrowserCompanionLink({
  where,
}: {
  where: "signals" | "lock" | "billing";
}) {
  const navigate = useNavigate();
  const status = useResource("browserCompanion", linkResource);
  const s = status.data;
  const connected = s?.pairings.filter((p) => p.connected) ?? [];
  const summary = !s
    ? status.error
      ? "读不到浏览器助手的连接状态。"
      : "正在读浏览器助手的连接状态…"
    : s.pairings.length === 0
      ? "还没有配对的浏览器。"
      : connected.length === 0
        ? `已配对 ${s.pairings.length} 个浏览器，现在一个都没连着${s.relink_needed ? "（要点「一键重连」）" : ""}。`
        : `已连接：${connected.map((p) => p.label).join("、")}${
            s.pairings.length > connected.length
              ? `；另有 ${s.pairings.length - connected.length} 个没连着`
              : ""
          }。`;
  return (
    <div className="qb-companion-pointer" data-testid="companion-pointer">
      <div>
        <strong>浏览器助手</strong>
        <p>{WHAT[where]}</p>
        <p role="status">{summary}</p>
      </div>
      <Button
        size="sm"
        icon={<ArrowRight size={13} />}
        onClick={() => {
          closeSecurity();
          navigate(BROWSER_COMPANION_PATH);
        }}
      >
        打开浏览器助手页
      </Button>
    </div>
  );
}
