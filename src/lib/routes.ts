/**
 * 六个入口的唯一定义处。
 *
 * 路径原来散在三处手写：`Shell.tsx` 的 `NAVIGATION`、它旁边的旧路径映射表，
 * 以及中转站页里六个 `/relays` 字面量。改一条路由要对着改三处，
 * 漏一处就是一个点不动的链接。
 *
 * ⛔ 旧路径不再有映射表（2026-09-27）。原来的 `LEGACY_REDIRECTS` 把十来个旧地址
 * （`/environment/*`、`/security`、`/accounts`、`/guide`……）接到新页面上，理由是书签和
 * 旧截图脚本还指着它们 —— 可面板是 HashRouter 的桌面应用，没有书签；托盘只发 `/relays`。
 * 认不出的路径一律由 `Shell` 末尾那条 `*` 送回首页，别再为某个旧地址单独开一条路。
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
    hint: "MCP、Skills、会话",
  },
  { path: "/settings", name: "设置", icon: Settings, hint: "偏好、备份与帮助" },
];
