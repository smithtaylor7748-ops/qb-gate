/**
 * 槽位在界面上怎么称呼：**「邮箱 - 命名」**（0.27.0，使用者定的）。
 *
 * # 为什么只是显示层
 *
 * `label`（使用者给槽位起的名字）仍然是**标识**，一个字不许动：
 *
 * - 槽位目录名是 `claude-profile-<label>`；
 * - 托盘菜单项的 id、切换/删除走的参数都是它；
 * - 启动日志里那句「…账户槽位 main」是 `src/lib/logline.ts` 按格式**解析**出来的 ——
 *   改日志格式当场把日志行解析弄坏。
 *
 * 所以这里只做一件事：把要显示给人看的那串字拼出来。
 *
 * # 邮箱可能没有
 *
 * 没登录、官方客户端还没写档案、或者换了格式，`email` 都会是 `null`；空字符串同样算没有。
 * 那时候退回只显示命名 —— 显示成「 - NEW1」比不改还糟。
 *
 * Rust 侧有一份同语义的 `qb_accounts::accounts::display_name`（托盘菜单用），两边改要一起改。
 */
export function slotName(
  email: string | null | undefined,
  label: string,
): string {
  const e = (email ?? "").trim();
  return e ? `${e} - ${label}` : label;
}
