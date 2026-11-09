import { useEffect, useState } from "react";
import type { Client } from "../../lib/generated/Client";
import type { RelayUsageOverview } from "../../lib/generated/RelayUsageOverview";
import { stationApi } from "../../lib/station";
import BarChart from "../../ui/BarChart";
import { rangeOf, type GranId, type RangeId } from "./shared";

export function useStationUsageOverview(
  client: Client,
  range: RangeId,
  granularity: GranId,
  routeId: string | null = null,
) {
  const key = `${client}/${range}/${granularity}/${routeId ?? ""}`;
  const [state, setState] = useState<{
    key: string;
    data: RelayUsageOverview | null;
    error: string | null;
  }>({ key, data: null, error: null });
  useEffect(() => {
    let dropped = false;
    let pending = false;
    setState({ key, data: null, error: null });
    const refresh = async () => {
      if (pending) return;
      pending = true;
      try {
        const data = await stationApi.usageOverview(
          client,
          range === "all" ? null : rangeOf(range).hours,
          granularity,
          routeId,
        );
        if (!dropped) setState({ key, data, error: null });
      } catch (e) {
        if (!dropped)
          setState({
            key,
            data: null,
            error: e instanceof Error ? e.message : String(e),
          });
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
  }, [client, range, granularity, routeId, key]);
  return state.key === key ? state : { key, data: null, error: null };
}

const pad = (n: number) => String(n).padStart(2, "0");
function localStamp(ms: number) {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** 只合并相邻桶，不抽样、不裁掉历史；每根柱子的提示保留实际起止。 */
export function requestBars(data: RelayUsageOverview) {
  const size = Math.max(1, Math.ceil(data.buckets.length / 60));
  const bars = [];
  for (let i = 0; i < data.buckets.length; i += size) {
    const part = data.buckets.slice(i, i + size);
    const start = part[0].start_ms;
    const end = part[part.length - 1].end_ms;
    const at = new Date(start);
    const requests = part.reduce((sum, b) => sum + b.totals.requests, 0);
    const failed = part.reduce((sum, b) => sum + b.totals.failed, 0);
    const last = i + size >= data.buckets.length;
    bars.push({
      key: String(start),
      axis:
        data.granularity === "day"
          ? `${pad(at.getMonth() + 1)}-${pad(at.getDate())}`
          : data.through_ms - data.since_ms > 86_400_000
            ? `${pad(at.getMonth() + 1)}-${pad(at.getDate())} ${pad(at.getHours())}时`
            : `${pad(at.getHours())}:${pad(at.getMinutes())}`,
      title: `${localStamp(start)} 至 ${localStamp(end)}（${last ? "含结束时刻" : "不含结束时刻"}）`,
      value: requests,
      rows: [
        ["成功", (requests - failed).toLocaleString()],
        ["失败", failed.toLocaleString()],
      ] as [string, string][],
    });
  }
  return { bars, size };
}

export function StationUsageChart({
  data,
  error,
}: {
  data: RelayUsageOverview | null;
  error: string | null;
}) {
  if (!data)
    return (
      <p className="qb-st-chart-empty">
        {error ? `读取完整用量失败：${error}` : "正在读取完整用量…"}
      </p>
    );
  const { bars, size } = requestBars(data);
  const unit =
    data.granularity === "day"
      ? "天"
      : data.granularity === "minute"
        ? "5 分钟"
        : "小时";
  return (
    <div className="qb-st-usage-chart">
      <div className="qb-st-legend">
        <b>请求数</b>
        <span>本机完整记录 · {data.totals.requests.toLocaleString()} 次</span>
        <span>
          {size === 1
            ? `每${unit}一柱`
            : `每 ${data.granularity === "minute" ? size * 5 + " 分钟" : size + " " + unit}一柱`}
        </span>
      </div>
      <BarChart
        data={bars}
        label="本机路由请求数，完整时间范围"
        format={(v) => v.toLocaleString()}
        valueFormat={(v) => `${v.toLocaleString()} 次`}
        integerTicks
        height={100}
        empty="这段时间没有经过本机路由的请求"
      />
      <p className="qb-st-note">
        本地时间 · 悬停、点击或用左右方向键查看时间与数量。
        {data.granularity === "day" && "日汇总包含已归档记录。"}
      </p>
    </div>
  );
}
