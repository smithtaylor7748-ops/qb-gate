/**
 * GPT 额度附带的两样（中转站重构阶段 6 · U2，照 cc-switch `subscription.rs` / `quotaRules.ts`）：
 * 存下的限额重置、Codex Credits 余额。只在点刷新问到的那一份里有，平时的本机快照没有。
 *
 * ⛔ Credits 只写个数，**不折算美元**：接口不给比例，cc-switch 的「1 Credit = $0.04」是它按价目表推的，
 * 这里不显示猜出来的数（CLAUDE.md「联网额度」第 6 条）。
 */
import type { TavernGptQuota } from "./generated/TavernGptQuota";

const COUNT = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 });

/** `MM-DD HH:mm`，跟账户行上别的时刻一个写法。 */
function md(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** 还能用的那几次重置：问完之后才过期的也去掉（「最近一次」没有 TTL，可能是几天前问的）。 */
export function usableResets(
  q: TavernGptQuota | null | undefined,
  now = Date.now(),
): (string | null)[] {
  return (q?.banked_resets ?? []).filter((at) => {
    if (!at) return true;
    const ms = Date.parse(at);
    // 认不出的到期时刻当作不过期 —— 宁可多显示一次也不吞掉（同后端）。
    return !Number.isFinite(ms) || ms > now;
  });
}

/** 一句一样，没有就是空数组。 */
export function gptExtras(
  q: TavernGptQuota | null | undefined,
  now = Date.now(),
): string[] {
  if (!q) return [];
  const out: string[] = [];
  const resets = usableResets(q, now);
  if (resets.length) {
    const first = resets[0];
    out.push(
      first
        ? `存下的限额重置 ${resets.length} 次 · 最早 ${md(first)} 到期`
        : `存下的限额重置 ${resets.length} 次 · 不会过期`,
    );
  }
  if (q.credits_balance != null && q.credits_balance > 0)
    out.push(
      `Codex Credits 余额 ${COUNT.format(q.credits_balance)} · 额度用完后才扣`,
    );
  return out;
}
