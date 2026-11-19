/**
 * 选线那几句话，主页和「选线与分流」弹窗共用（2026-11-15）。
 *
 * ⛔ 全页只用四个词：**当前线路、备用线路、备用地址、自动选线**。原来同一件事有「智能调度 / 调度 / 派单」
 * 「允许名单 / 授权池 / 线路池」「钉住 / 固定池内首选」好几个名字，「备用」还同时指备用线路和站点的备用地址（G4）。
 *
 * 数据全来自后端的 `RouterStatus.selection`（R1）：这里只把它写成人话，**不自己再算一遍顺序** ——
 * 再算一遍就是两份判定。
 */

import type { PlanSkip } from "../../lib/generated/PlanSkip";
import type { RoutePlan } from "../../lib/generated/RoutePlan";
import type { SelectionView } from "../../lib/generated/SelectionView";

/** 这一步为什么会被跳过。 */
export const SKIP_LABEL: Record<PlanSkip, string> = {
  "breaker-rejected": "熔断中：上游明确拒绝过（余额不足 / Key 失效 / 没权限）",
  "breaker-failing": "熔断中：连着出错（5xx / 超时）",
  "rate-limited": "上游限流中",
  "quota-exhausted": "额度用光了",
  "over-limit": "超出一次最多试几条，轮不到",
};

/** 短的那种，放在线路名后面。 */
export const SKIP_SHORT: Record<PlanSkip, string> = {
  "breaker-rejected": "熔断中",
  "breaker-failing": "熔断中",
  "rate-limited": "限流中",
  "quota-exhausted": "额度用光",
  "over-limit": "轮不到",
};

/**
 * 只有熔断那两种「重试」有用：`station_route_retest` 让熔断中的线马上转半开，下一发真实请求就是回探。
 * 限流（等它给的时间）、额度用光（充值后等下一次查额度）点了也没用，不给按钮（R1 交接）。
 */
export function retryable(reason: PlanSkip | null | undefined): boolean {
  return reason === "breaker-rejected" || reason === "breaker-failing";
}

/** `bigint | number | null` 的毫秒 → 「预计 14:05 恢复」；不知道就空串。 */
export function untilText(until: bigint | number | null | undefined): string {
  if (until == null) return "";
  const ms = Number(until);
  if (!Number.isFinite(ms) || ms <= Date.now()) return "";
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  const sameDay = d.toDateString() === new Date().toDateString();
  return `预计 ${sameDay ? "" : `${p(d.getMonth() + 1)}-${p(d.getDate())} `}${p(d.getHours())}:${p(d.getMinutes())} 恢复`;
}

/** 备用线路（不含当前线路本身），按排的顺序。 */
export function backupsOf(
  selection: SelectionView | null | undefined,
): string[] {
  if (!selection) return [];
  return selection.backups.filter((id) => id !== selection.current);
}

/**
 * 右列「选线与分流」卡片第二行：「备用：经济线 → 标准线」或「备用：无 · 失败直接报错」。
 * 读不出来时如实说读不出来，不写成「无」。
 */
export function backupSummary(
  selection: SelectionView | null | undefined,
  label: (id: string) => string,
): string {
  if (!selection) return "备用：正在读取…";
  if (selection.problem) return "备用：读不出来";
  const ids = backupsOf(selection);
  return ids.length
    ? `备用：${ids.map(label).join(" → ")}`
    : "备用：无 · 失败直接报错";
}

/** 「下一次请求：A → B（熔断中，跳过）→ C」。没设当前线路时说清楚。 */
export function planText(
  plan: RoutePlan | null | undefined,
  label: (id: string) => string,
): string {
  if (!plan) return "正在读取…";
  if (!plan.current) return "还没设当前线路：先在线路行上点「设为当前」";
  if (plan.strict_single)
    return `只用「${label(plan.current)}」，失败直接报错（没有备用线路）`;
  const steps = plan.steps.map((s) =>
    s.skip
      ? `${label(s.route_id)}（${SKIP_SHORT[s.skip]}，跳过）`
      : label(s.route_id),
  );
  const tail =
    plan.first == null ? " · 现在没有能发的线，这一发会回「繁忙」" : "";
  return `${steps.join(" → ")}${tail} · 一次最多试 ${plan.max_attempts} 条`;
}
