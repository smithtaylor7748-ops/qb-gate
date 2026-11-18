import { useEffect, useState, type ReactNode } from "react";
import type { Client } from "../../lib/generated/Client";
import type { RelayUsageOverview } from "../../lib/generated/RelayUsageOverview";
import type { RelayUsageTotals } from "../../lib/generated/RelayUsageTotals";
import { stationApi } from "../../lib/station";
import { usd, usdTick } from "../../lib/usageFormat";
import BarChart, { type BarDatum } from "../../ui/BarChart";
import { mtok, rangeOf, type GranId, type RangeId } from "./shared";

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

/**
 * 柱状图画哪一个量（2026-11-15，使用者：「柱状图的数据可以有下拉框改为请求数和 token 和价格」）。
 *
 * ⛔ 口径跟上面那几格一样：Token = 四类加总（跟「总 Token」同一个算法），价格 = 站内价（跟「总消费」同一个数）。
 * 一个桶里有请求、却一类 token / 一分钱价格都没记到时，那根柱子**不画**、提示框写「没记到」——
 * 画成 0 会被读成「那一小时一个 token 都没用、一分钱没花」。
 */
export type ChartMetric = "requests" | "tokens" | "cost";
export const CHART_METRICS: { id: ChartMetric; name: string }[] = [
  { id: "requests", name: "请求数" },
  { id: "tokens", name: "Token" },
  { id: "cost", name: "价格" },
];

type Totals = RelayUsageTotals;
type NullableKey =
  | "input_tokens"
  | "output_tokens"
  | "cache_read_tokens"
  | "cache_write_tokens"
  | "station_cost"
  | "official_cost";
const TOKEN_PARTS: [NullableKey, string][] = [
  ["input_tokens", "输入"],
  ["cache_read_tokens", "缓存读"],
  ["cache_write_tokens", "缓存写"],
  ["output_tokens", "输出"],
];
const NULLABLE: NullableKey[] = [
  "input_tokens",
  "output_tokens",
  "cache_read_tokens",
  "cache_write_tokens",
  "station_cost",
  "official_cost",
];

/** 几个桶合在一起：没记到 + 没记到 还是没记到，不变成 0。 */
function mergeTotals(part: Totals[]): Totals {
  const out: Totals = {
    requests: part.reduce((sum, t) => sum + t.requests, 0),
    failed: part.reduce((sum, t) => sum + t.failed, 0),
    input_tokens: null,
    output_tokens: null,
    cache_read_tokens: null,
    cache_write_tokens: null,
    station_cost: null,
    official_cost: null,
  };
  for (const key of NULLABLE)
    for (const t of part) {
      const v = t[key];
      if (v != null) out[key] = (out[key] ?? 0) + v;
    }
  return out;
}

/** 四类 token 的加总；一类都没记到是 `null`（跟「总 Token」那一格同一条规矩）。 */
export function tokenSum(t: Totals): number | null {
  const known = TOKEN_PARTS.map(([k]) => t[k]).filter(
    (v): v is number => v != null,
  );
  return known.length ? known.reduce((a, b) => a + b, 0) : null;
}

/** 只合并相邻桶，不抽样、不裁掉历史；每根柱子的提示保留实际起止。 */
export function requestBars(
  data: RelayUsageOverview,
  metric: ChartMetric = "requests",
) {
  const size = Math.max(1, Math.ceil(data.buckets.length / 60));
  const bars: BarDatum[] = [];
  for (let i = 0; i < data.buckets.length; i += size) {
    const part = data.buckets.slice(i, i + size);
    const start = part[0].start_ms;
    const end = part[part.length - 1].end_ms;
    const at = new Date(start);
    const merged = mergeTotals(part.map((b) => b.totals));
    const { requests, failed } = merged;
    const last = i + size >= data.buckets.length;
    const value =
      metric === "requests"
        ? requests
        : metric === "tokens"
          ? tokenSum(merged)
          : merged.station_cost;
    const rows: [string, string][] =
      metric === "requests"
        ? [
            ["成功", (requests - failed).toLocaleString()],
            ["失败", failed.toLocaleString()],
          ]
        : metric === "tokens"
          ? [
              ["请求", `${requests.toLocaleString()} 次`],
              ...TOKEN_PARTS.map(([k, name]): [string, string] => [
                name,
                merged[k] == null ? "没记到" : merged[k]!.toLocaleString(),
              ]),
            ]
          : [
              ["请求", `${requests.toLocaleString()} 次`],
              ["官方价折算", usd(merged.official_cost)],
            ];
    bars.push({
      key: String(start),
      axis:
        data.granularity === "day"
          ? `${pad(at.getMonth() + 1)}-${pad(at.getDate())}`
          : data.through_ms - data.since_ms > 86_400_000
            ? `${pad(at.getMonth() + 1)}-${pad(at.getDate())} ${pad(at.getHours())}时`
            : `${pad(at.getHours())}:${pad(at.getMinutes())}`,
      title: `${localStamp(start)} 至 ${localStamp(end)}（${last ? "含结束时刻" : "不含结束时刻"}）`,
      value,
      // 有请求却没记到：提示框写「没记到」；没请求就是「没有请求」。两种都不画柱。
      missing: requests > 0 ? "没记到" : "没有请求",
      rows,
    });
  }
  return { bars, size };
}

/** 一柱代表多长时间。 */
function barSpan(data: RelayUsageOverview, size: number): string {
  const unit =
    data.granularity === "day"
      ? "天"
      : data.granularity === "minute"
        ? "5 分钟"
        : "小时";
  return size === 1
    ? `每${unit}一柱`
    : `每 ${data.granularity === "minute" ? size * 5 + " 分钟" : size + " " + unit}一柱`;
}

/** 图表标题旁那一句的总量：跟上面那几格同一个口径。 */
function totalText(data: RelayUsageOverview, metric: ChartMetric): string {
  if (metric === "requests")
    return `${data.totals.requests.toLocaleString()} 次`;
  if (metric === "tokens") {
    const t = tokenSum(data.totals);
    return t == null ? "Token 没记到" : `${mtok(t)} Token`;
  }
  return data.totals.station_cost == null
    ? "站内价没记到"
    : `站内价 ${usd(data.totals.station_cost)}`;
}

export function StationUsageChart({
  data,
  error,
  metric = "requests",
  onMetric,
  controls,
}: {
  data: RelayUsageOverview | null;
  error: string | null;
  metric?: ChartMetric;
  /** 给了才画「柱状图」那个下拉框（当图表标题用）。 */
  onMetric?: (m: ChartMetric) => void;
  /** 标题那一行右边的东西：时间范围、粒度、「用量明细」。 */
  controls?: ReactNode;
}) {
  const built = data ? requestBars(data, metric) : null;
  const name = CHART_METRICS.find((m) => m.id === metric)!.name;
  return (
    <div className="qb-st-usage-chart">
      {/* 一行：左边「柱状图」下拉框 + 一句总量，右边时间范围、粒度、用量明细（第三版样机）。 */}
      <div className="qb-st-chart-head">
        {onMetric ? (
          <select
            className="qb-st-chart-metric"
            aria-label="柱状图"
            title="柱状图画哪一个量"
            value={metric}
            onChange={(e) => onMetric(e.target.value as ChartMetric)}
          >
            {CHART_METRICS.map((m) => (
              <option key={m.id} value={m.id}>
                {m.name}
              </option>
            ))}
          </select>
        ) : (
          <b>{name}</b>
        )}
        {data && built && (
          <span className="qb-st-chart-sum">
            本机完整记录 · {totalText(data, metric)} ·{" "}
            {barSpan(data, built.size)}
          </span>
        )}
        {controls && <span className="qb-st-chart-ctl">{controls}</span>}
      </div>
      {!data || !built ? (
        <p className="qb-st-chart-empty">
          {error ? `读取完整用量失败：${error}` : "正在读取完整用量…"}
        </p>
      ) : (
        <>
          <BarChart
            data={built.bars}
            label={`本机路由${name}，完整时间范围`}
            format={
              metric === "cost"
                ? usdTick
                : metric === "tokens"
                  ? mtok
                  : (v) => v.toLocaleString()
            }
            valueFormat={
              metric === "cost"
                ? usd
                : metric === "tokens"
                  ? (v) => `${v.toLocaleString()} Token`
                  : (v) => `${v.toLocaleString()} 次`
            }
            integerTicks={metric !== "cost"}
            height={100}
            empty={
              metric === "requests" || data.totals.requests === 0
                ? "这段时间没有经过本机路由的请求"
                : metric === "tokens"
                  ? "这段时间的请求上游都没回报 Token 用量（没记到，不是 0）"
                  : "这段时间的请求都没记到站内价（没记到，不是 0）"
            }
          />
          <p className="qb-st-note">
            本地时间 · 悬停、点击或用左右方向键查看
            {metric === "requests"
              ? "时间与数量"
              : metric === "tokens"
                ? "四类 Token 的拆分"
                : "站内价与官方价折算"}
            。
            {metric === "cost" &&
              "价格是站内额度估算（美元），只算已知价格的部分。"}
            {data.granularity === "day" && "日汇总包含已归档记录。"}
          </p>
        </>
      )}
    </div>
  );
}
