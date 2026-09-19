/**
 * 六个入口的唯一定义处。
 *
 * 路径原来散在三处手写：`Shell.tsx` 的 `NAVIGATION`、它旁边的 `LEGACY` 映射表，
 * 以及 `RelayCenter.tsx` 里六个 `/relays` 字面量。改一条路由要对着改三处，
 * 漏一处就是一个点不动的链接。
 */
import {
  Blocks,
  CreditCard,
  KeyRound,
  Package,
  Settings,
  Waypoints,
  type LucideIcon,
} from "lucide-react";

export const RELAY_BASE = "/relays";

/**
 * 旧那套「供应商 / 凭证 / 环境」的入口。
 *
 * 中转站页去掉外壳分页栏之后，旧页面只剩深链接进得去。这个路径是
 * **不指定服务商**的那一个 —— 没有它，旧页面只有知道某个服务商 id
 * 的人才进得去，而站点目前还只能从那儿添。
 */
export const RELAY_LEGACY = RELAY_BASE + "/providers";

export interface NavEntry {
  path: string;
  name: string;
  icon: LucideIcon;
  /** 侧栏窄屏收起时用作无障碍名，也用于 Ctrl+K 搜索结果的副标题。 */
  hint: string;
}

/**
 * 第一项**就是**仪表盘，没有单独的「总览」。
 *
 * 旧版总览和账户页都列槽位，两处重复；新版把槽位、切换、启动、评分、门禁状态、
 * 会话、日志全放在这一页，低频操作（新建槽位 / 合并桥接 / 官方目录残留迁移）
 * 在槽位块里就地展开。
 */
export const NAV: NavEntry[] = [
  { path: "/", name: "官方账户", icon: KeyRound, hint: "槽位、额度与启动" },
  { path: RELAY_BASE, name: "中转站", icon: Waypoints, hint: "API 服务与诊断" },
  {
    path: "/subscription",
    name: "订阅",
    icon: CreditCard,
    hint: "双 AI 订阅与避坑实战",
  },
  { path: "/software", name: "软件", icon: Package, hint: "安装、升级与版本" },
  {
    path: "/extensions",
    name: "扩展",
    icon: Blocks,
    hint: "酒馆、MCP、Skills",
  },
  { path: "/settings", name: "设置", icon: Settings, hint: "偏好、备份与帮助" },
];

/**
 * 旧路径 → 新路径。**这不是善意补充，是必需品。**
 *
 * `src-tauri/src/tray.rs` 里写死了 `emit("workspace://navigate", "/relays")`，
 * 后端这次不动，所以前端必须接得住。这张表同时接住书签、旧截图脚本、
 * 以及文档里引用过的 URL。
 *
 * 按前缀匹配，长的排前面。
 */
export const LEGACY_REDIRECTS: [string, string][] = [
  ["/subscription-guide", "/subscription"],
  ["/guide", "/subscription"],
  ["/environment/software", "/software"],
  // 六项安全对象全在总览上：四格是评分明细，执行锁与会话内门禁挂在
  // 评分卡右上角那排门禁读数上，点一下开小窗。**没有单独的安全页** ——
  // 那会让同一份内容有两个入口，正是这次重排要消灭的。
  ["/environment/gate", "/"],
  ["/environment/ip", "/"],
  ["/environment/dns", "/"],
  ["/environment/signals", "/"],
  ["/environment", "/"],
  ["/security", "/"],
  ["/workspace", "/"],
  // 0.20.0 删掉了「管理账户」那一页：使用者要的「管理」就是删除，而删除
  // 现在直接长在总览的槽位横条上。那一页原来独有的「官方目录里的 API 配置」
  // 搬进了总览（`OfficialConfigResidue`），所以这里只需要把地址接住。
  ["/accounts", "/"],
  ["/official", "/"],
];

/** 给一个可能是旧路径的地址，返回它现在该去哪；不需要重定向就返回 null。 */
export function redirectFor(pathname: string): string | null {
  for (const [from, to] of LEGACY_REDIRECTS) {
    if (pathname === from || pathname.startsWith(from + "/")) return to;
  }
  return null;
}
