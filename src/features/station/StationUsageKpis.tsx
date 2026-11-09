import { useEffect, useState } from "react";
import type { Client } from "../../lib/generated/Client";
import type { RelayUsageTotals } from "../../lib/generated/RelayUsageTotals";
import { stationApi } from "../../lib/station";
import { usd } from "../../lib/usageFormat";
import { mtok, rangeOf, tokenMix, type RangeId } from "./shared";

/** 首页用量读完整汇总；不依赖站点账单、现有线路或已加载的日志页。 */
export function StationUsageKpis({
  client,
  range,
}: {
  client: Client;
  range: RangeId;
}) {
  const [totals, setTotals] = useState<RelayUsageTotals | null>(null);
  const [error, setError] = useState(false);
  useEffect(() => {
    let dropped = false;
    let pending = false;
    setTotals(null);
    setError(false);
    const refresh = async () => {
      if (pending) return;
      pending = true;
      try {
        const next = await stationApi.usageTotals(
          client,
          range === "all" ? null : rangeOf(range).hours,
        );
        if (!dropped) {
          setTotals(next);
          setError(false);
        }
      } catch {
        if (!dropped) {
          setTotals(null);
          setError(true);
        }
      } finally {
        pending = false;
      }
    };
    void refresh();
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh();
    }, 5000);
    return () => {
      dropped = true;
      window.clearInterval(timer);
    };
  }, [client, range]);

  const categories = totals && [
    totals.input_tokens,
    totals.cache_read_tokens,
    totals.cache_write_tokens,
    totals.output_tokens,
  ];
  const known = categories?.filter((n): n is number => n != null) ?? [];
  const tokens = known.length ? known.reduce((sum, n) => sum + n, 0) : null;
  const windowName = rangeOf(range).name;
  const note = error
    ? `${windowName} · 读取本机用量失败`
    : `${windowName} · 正在读取本机用量`;
  const partial = categories?.some((n) => n == null) ?? false;
  const tokenDetail = categories
    ? tokenMix(categories[0], categories[1], categories[2], categories[3])
    : null;
  const tokenNote = !totals
    ? note
    : known.length && tokenDetail
      ? `${windowName} · ${tokenDetail}${partial ? " · 部分分类缺失" : ""}`
      : `${windowName} · 上游未返回 Token 用量`;
  return (
    <>
      <UsageKpi
        label="总请求数"
        value={totals ? totals.requests.toLocaleString() : "—"}
        note={totals ? `${rangeOf(range).name} · 本机路由完整记录` : note}
      />
      <UsageKpi
        label="总 Token"
        value={tokens == null ? "—" : mtok(tokens)}
        note={tokenNote}
      />
      <UsageKpi
        label="总消费"
        value={usd(totals?.station_cost ?? null)}
        note={
          !totals
            ? note
            : totals.station_cost == null
              ? `${windowName} · 缺少用量或站内价格，无法估算`
              : `${windowName} · USD 站内额度估算（已知价格部分）`
        }
      />
    </>
  );
}

function UsageKpi({
  label,
  value,
  note,
}: {
  label: string;
  value: string;
  note: string;
}) {
  return (
    <div className="qb-st-kpi">
      <span className="k">{label}</span>
      <span className="v">{value}</span>
      <span className="s">{note}</span>
    </div>
  );
}
