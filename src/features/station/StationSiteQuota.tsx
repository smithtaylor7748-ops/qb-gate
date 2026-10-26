/**
 * 站点卡表头那一格：余额 / 编程套餐额度（中转站重构阶段 6 · U3，cc-switch 的 `balance.rs` + `coding_plan.rs`）。
 *
 * 打开页面问一次（后端：这把 Key 5 分钟内问过的直接给那一份），右边的图标再问一次；没有定时器。
 * 认不出是哪一家（绝大多数中转站）后端一个请求都不发，这里照旧写「站点未提供」。
 * 只显示 —— 调度、排序一概不读它。
 *
 * ⛔ 问不到不画 0：有上一次就留着上一次（悬停里时刻照实），没有就说没问到、悬停看原因。
 */
import { useCallback, useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import type { SiteQuota } from "../../lib/generated/SiteQuota";
import { stationApi } from "../../lib/station";
import { Button } from "../../ui";

/** `12.30 元` / `-$5.00`。 */
export function money(n: number, unit: string): string {
  const v = Math.abs(n).toLocaleString("zh-CN", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
  const sign = n < 0 ? "-" : "";
  if (unit === "CNY") return `${sign}${v} 元`;
  if (unit === "USD") return `${sign}$${v}`;
  return `${sign}${v} ${unit}`;
}

function clock(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** 表头上那一行和它的语气。 */
export function quotaLine(q: SiteQuota): {
  text: string;
  tone: "ok" | "warn" | "danger" | null;
} {
  if (q.kind === "balance") {
    const low = q.balances.some((b) => b.remaining <= 0);
    return {
      text: `余额 ${q.balances.map((b) => money(b.remaining, b.unit)).join(" · ")}`,
      tone: q.warning || low ? "danger" : null,
    };
  }
  const left = q.windows.map((w) =>
    Math.max(0, Math.round(100 - w.used_percent)),
  );
  const tight = Math.min(...left);
  return {
    text: q.windows.map((w, i) => `${w.label} 剩 ${left[i]}%`).join(" · "),
    tone: tight < 20 ? "danger" : tight < 50 ? "warn" : "ok",
  };
}

/** 悬停里的全文：哪一家、各格什么时候重置、按哪一组的 Key、几点问的。 */
function detail(q: SiteQuota, group: string): string {
  const lines = [`${q.provider}${q.plan ? ` · ${q.plan}` : ""}`];
  for (const b of q.balances)
    if (b.total != null && b.used != null)
      lines.push(
        `充了 ${money(b.total, b.unit)}，用了 ${money(b.used, b.unit)}`,
      );
  for (const w of q.windows)
    lines.push(
      `${w.label}：已用 ${Math.round(w.used_percent)}%${w.resets_at ? ` · ${clock(w.resets_at)} 重置` : ""}`,
    );
  if (q.warning) lines.push(q.warning);
  lines.push(
    `按「${group || "默认分组"}」那把 Key 问的 · ${clock(q.fetched_at)} 问的`,
  );
  lines.push(
    "打开中转站页时 5 分钟内问过就不再问；点右边的图标再问一次。只显示，调度不看它。",
  );
  return lines.join("\n");
}

export default function StationSiteQuota({
  routeId,
  group,
}: {
  /** 站点在这个软件下的第一条线路：用它那把 Key 问。 */
  routeId: string;
  group: string;
}) {
  /** `undefined` = 还没回话；`null` = 认不出是哪一家（不联网）。 */
  const [q, setQ] = useState<SiteQuota | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const load = useCallback(
    async (force: boolean) => {
      setBusy(true);
      setError("");
      try {
        setQ(await stationApi.siteQuota(routeId, force));
      } catch (e) {
        // 留着上一次问到的（悬停里时刻照实），只把这次的错挂出来。
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [routeId],
  );
  useEffect(() => {
    void load(false);
  }, [load]);

  if (q === null && !error)
    return <span className="bal">余额（整站共享）站点未提供</span>;
  if (q === undefined && !error)
    return <span className="bal">余额 · 读取中…</span>;

  const shown = q ? quotaLine(q) : null;
  const title = [q ? detail(q, group) : "", error ? `这次没问到：${error}` : ""]
    .filter(Boolean)
    .join("\n\n");
  return (
    <span className="bal qb-st-quota" title={title} data-testid="site-quota">
      {shown ? (
        <span className={shown.tone ? `qb-tone-${shown.tone}` : undefined}>
          {shown.text}
        </span>
      ) : (
        <span className="qb-tone-warn">余额 · 没问到（悬停看原因）</span>
      )}
      {shown && error && <span className="qb-tone-warn"> · 刷新没成</span>}
      <Button
        size="sm"
        variant="ghost"
        className="qb-st-quota-refresh"
        icon={<RefreshCw size={11} />}
        aria-label="再问一次这家站的余额或套餐额度"
        title="用这条线路的 Key 再问一次（只读）"
        loading={busy}
        onClick={() => void load(true)}
      />
    </span>
  );
}
