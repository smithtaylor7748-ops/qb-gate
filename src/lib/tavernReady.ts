/**
 * 酒馆两条内置桥（GPT / Gemini）的就绪判定：现在起不起得来，起不来缺哪一样。
 *
 * # 为什么单独放一个模块
 *
 * 挡路的东西有四种，分布在两个页面上：酒馆本体与 CLI 在软件页（2026-09-29 起酒馆不在扩展中心了，
 * 软件页的 SillyTavern 卡能直接下载安装一份），槽位与登录在官方账户页。
 * 界面上原来只有一个会灰掉的按钮加一句 `detail`，
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
 * 酒馆**本体**的那几项依赖（`PluginStatus.checks` 的标签，照 `plugins::sillytavern::status_with`）。
 * 缺的是它们，就该去软件页的 SillyTavern 卡（下载安装、指到自己那份、装 Node.js）；
 * 缺的是别的（bridge.py、CLI、槽位），就去软件页这一家那张 CLI 卡最下面的「酒馆 · X 桥接设置」
 * （2026-09-29 第三轮起桥的设置在那里，那里列着缺什么）。
 */
export const TAVERN_BODY_CHECKS = ["SillyTavern", "酒馆启动脚本", "Node.js"];

/** 酒馆本体缺东西吗。读不出检查结果（还在加载）不算缺 —— 不猜。 */
export function tavernBodyMissing(
  checks: readonly { label: string; ok: boolean }[] | undefined,
): boolean {
  return !!checks?.some((c) => TAVERN_BODY_CHECKS.includes(c.label) && !c.ok);
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
    // 这句话出现在软件页 Codex CLI / Gemini CLI 卡最下面的桥接设置里 —— 别写「在上面」「在下面」
    // （卡片的先后顺序会变）。2026-09-29 起酒馆本体在软件页（扩展中心的酒馆页删了）。
    out.push(
      "酒馆还没装好 —— 到「软件」页的 SillyTavern 卡下载安装一份，或指到你自己那份。",
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
