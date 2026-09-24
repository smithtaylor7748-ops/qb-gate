/**
 * 酒馆两条内置桥（GPT / Gemini）的就绪判定：现在起不起得来，起不来缺哪一样。
 *
 * # 为什么单独放一个模块
 *
 * 挡路的东西有四种，分布在三个不同的页面上：酒馆路径在酒馆插件页、CLI 在软件页、
 * 槽位与登录在官方账户页。界面上原来只有一个会灰掉的按钮加一句 `detail`，
 * 于是「为什么点不动」这个问题在界面上没有答案 —— 2026-09-21 使用者报的
 * 「GPT 和 Gemini 启动失败」里，Gemini 那条每次都停在「没有激活的 Gemini 槽位」，
 * 而按钮看上去是可以点的。
 *
 * 判定是纯函数，才能被单测钉住。放在组件里就只能靠肉眼。
 */

/** 一条桥的就绪输入。字段与 `GptBridgeStatus` / `GeminiBridgeStatus` 一一对应。 */
export interface BridgeReadiness {
  /** 酒馆自己的两个路径（SillyTavern 目录、启动脚本）还空着。 */
  tavernPathsUnset: boolean;
  /** 找到的 CLI（`codex.exe`，或 `node + 入口`）。没找到就是 null。 */
  cli: string | null;
  /** CLI 的名字，写进提示里。 */
  cliLabel: string;
  /** 没有 CLI 时该去哪儿装。 */
  cliWhere: string;
  /** 激活的账户槽位标签。没有激活槽位就是 null。 */
  slot: string | null;
  slotLoggedIn: boolean;
  /** 槽位在哪个页面里建、在哪儿登录。 */
  slotWhere: string;
}

/**
 * 还差什么。空数组 = 可以起了。
 *
 * ⚠ 顺序 = **该先做哪一件**：路径 → CLI → 槽位 → 登录。
 * 倒过来做要返工（没有 CLI 就没法在槽位里完成登录）。
 */
export function bridgeBlockers(o: BridgeReadiness): string[] {
  const out: string[] = [];
  if (o.tavernPathsUnset) {
    out.push(
      "酒馆路径还没配 —— 在上面「设置与定位」里点「自动定位」，或自己填。",
    );
  }
  if (!o.cli) out.push(`找不到${o.cliLabel} —— ${o.cliWhere}。`);
  if (!o.slot) {
    out.push(`没有激活的账户槽位 —— 到${o.slotWhere}新建一个并登录。`);
  } else if (!o.slotLoggedIn) {
    out.push(`槽位「${o.slot}」还没登录 —— 到${o.slotWhere}完成登录。`);
  }
  return out;
}
