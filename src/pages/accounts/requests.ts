/**
 * 账户相关对话框的「喊一声」入口与共用判定。
 *
 * # 为什么单独一个文件
 *
 * 对话框本体（`AccountDialogs.tsx`）要挂详情页（`AccountDetail.tsx`），
 * 而详情页里的按钮又要喊切换、喊删除。两边互相 import 就成了循环 ——
 * ES 模块能跑，但求值顺序一变就是一个「函数在初始化前被调用」的运行期错误，
 * 而且报错位置离真正的原因很远。
 *
 * 把会话态的键和纯函数放这里，两边都只往下依赖这一个文件。
 */

import { setSession } from "../../lib/store";
import type { Slot } from "../../lib/api";

export const SWITCH_KEY = "accounts.switchTo";
/** 切完之后要不要顺手起一个 Claude Code（登录界面由它自己弹）。 */
export const SWITCH_THEN_LOGIN_KEY = "accounts.switchThenLogin";
export const NEW_KEY = "accounts.newSlot";
export const DELETE_KEY = "accounts.deleteSlot";
export const DETAIL_KEY = "accounts.detail";

/** 打开切换对话框。总览与详情页都走这一个。 */
export function requestSwitch(label: string) {
  setSession(SWITCH_THEN_LOGIN_KEY, false);
  setSession<string | null>(SWITCH_KEY, label);
}

/**
 * 切过去，切完立刻起 Claude Code 让官方登录界面弹出来。
 *
 * 「新建即登录」和详情页的「重新登录」都走它。**面板全程不经手账号密码** ——
 * 登录界面是官方客户端自己弹的，这里只负责把那个槽位变成当前的、再起进程。
 *
 * 为什么必须先切：`workspace::launch` 里那道拦截（「请先在官方账户页切换到此账户」）
 * 守着硬约束「任意时刻只有一个账户激活」。绕过它就是同时跑两个账户。
 *
 * # ⛔ 目标已经是当前账户时也照切一遍，这是**故意的**
 *
 * 那一下会关掉本机全部 Claude、再把三处指向换到它已经在的地方 ——
 * 看起来完全多余，`preflight_switch` 也确实没有「已经是当前」的短路。
 * 但重登要处理的场景正是「令牌被回收 / 状态不对」，那种时候别的进程
 * 内存里还握着旧凭证；不清场就是带着半截旧状态再登一次。
 * 使用者 0.20.0 明确选了这一档：「就要这种非常保守的」。
 *
 * 只启动、不清场的那条路仍然在 —— 在槽位横条上（`AccountBand`），
 * 给的是「激活槽位凭证过期、但我只想再起一个会话」那一档。
 * **两条路故意不一样，别合并。**
 */
export function requestSwitchAndLogin(label: string) {
  setSession(SWITCH_THEN_LOGIN_KEY, true);
  setSession<string | null>(SWITCH_KEY, label);
}

/** 打开「新建槽位」对话框。 */
export function requestNewSlot() {
  setSession(NEW_KEY, true);
}

/** 打开「删除槽位」确认框。 */
export function requestDelete(label: string) {
  setSession<string | null>(DELETE_KEY, label);
}

/** 打开某个槽位的详情。 */
export function requestAccountDetail(label: string) {
  setSession<string | null>(DETAIL_KEY, label);
}

/**
 * 凭证过期或从没登过 —— 这两种都得在那个槽位里登录一次。
 *
 * ⚠ 反过来**不成立**：这里回 `false` 不等于「这个账户还能用」。
 * 剩余天数只读本地那个时间戳，**查不出「被风控下线」** ——
 * 令牌被回收时 `.credentials.json` 还在、到期日还是将来某一天，
 * 而实际已经发不出请求了。所以「重新登录」这个入口必须常驻，
 * 不能只在这个函数回 `true` 时才给。
 */
export function needsLogin(s: Slot | null | undefined): boolean {
  if (!s) return true;
  return !s.logged_in || (s.cli_days_left ?? 0) < 0;
}

/** 槽位名规矩，与 Rust `accounts::validate_label` 一致。后端仍会再验一次。 */
export function labelError(name: string): string | undefined {
  const n = name.trim();
  if (!n) return undefined;
  if ([...n].length > 32) return "最长 32 个字符";
  if (!/^[\p{L}\p{N}_.-]+$/u.test(n)) return "只能用字母、数字、中文和 - _ .";
  if (n.startsWith(".") || n.endsWith(".")) return "不能以点开头或结尾";
  return undefined;
}
