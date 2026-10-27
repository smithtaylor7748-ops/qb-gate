/**
 * 中转用量页 `/relays/usage`（中转站重构阶段 6 · U1，照 cc-switch 的用量页：趋势、全年热力图、按供应商 / 模型、请求日志）。
 *
 * 从中转站页总览那一格的「用量明细」进来（不进侧栏；侧栏里「中转站」那一项跟着亮）。可以滚动 ——
 * 中转站页本身是一屏，趋势、热力图、几张表、逐条日志摆不下。
 *
 * # 数从哪来（页面上写明，不藏进悬停）
 *
 * - 只统计**经本机路由**的请求：五个软件（Claude Code、Claude 桌面端、Codex、Gemini CLI、Grok Build）。
 *   共存型工具和桌面端直连档写的是真 Key、不经路由，这里看不到。
 * - 近 30 天是逐条明细；更早的只剩按天汇总（`usage_daily`，迁移 10），所以逐条日志只翻得到 30 天。
 * - 美元两种：**站内价**（按线路公布的价算）与**官方价折算**（同样的 token 走官方 API 的价 —— 订阅不按这个收费）。
 * - 一个联网请求都没有（`usecase::relay_usage_ops`，读本机库）。
 *
 * # ⛔ 显示口径（跟 `/usage` 一样）
 *
 * 没记到的 token / 花费写「—」不写 0；没有请求的那天不画柱；热力图里最早那条记录之前的格子不画。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, RefreshCw } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";

import type { Client } from "../../lib/generated/Client";
import type { RelayRequestPage } from "../../lib/generated/RelayRequestPage";
import type { RelayUsageGroup } from "../../lib/generated/RelayUsageGroup";
import type { RelayUsageReport } from "../../lib/generated/RelayUsageReport";
import type { RelayUsageTotals } from "../../lib/generated/RelayUsageTotals";
import { stationApi } from "../../lib/station";
import {
  lastDays,
  daysFrom,
  shortDay,
  today as todayYmd,
} from "../../lib/dates";
import { NUM, pct, short, usd, usdTick } from "../../lib/usageFormat";
import {
  BarChart,
  Button,
  Card,
  PageHeader,
  YearHeatmap,
  YearHeatmapLegend,
  type BarDatum,
  type HeatDay,
} from "../../ui";
import { STATION_CLIENTS, stamp } from "./shared";

const RANGES: [number, string][] = [
  [1, "今天"],
  [7, "7 天"],
  [30, "30 天"],
  [0, "全部"],
];
/** 自己重读的间隔：一个叫「今天」的数字不会自己往前走。 */
const REFRESH_MS = 60_000;
const PAGE = 50;

type Metric = "station" | "official" | "tokens" | "requests";
const METRICS: [Metric, string][] = [
  ["station", "站内花费"],
  ["official", "官方价折算"],
  ["tokens", "Token"],
  ["requests", "请求数"],
];

/** 四类 token 加起来；一类都没记到就是 `null`。 */
export function tokenSum(t: RelayUsageTotals): number | null {
  const parts = [
    t.input_tokens,
    t.output_tokens,
    t.cache_read_tokens,
    t.cache_write_tokens,
  ];
  if (parts.every((x) => x == null)) return null;
  return parts.reduce<number>((a, x) => a + (x ?? 0), 0);
}

function metricOf(t: RelayUsageTotals, m: Metric): number | null {
  if (m === "station") return t.station_cost;
  if (m === "official") return t.official_cost;
  if (m === "tokens") return tokenSum(t);
  return t.requests;
}

const fmtMetric = (m: Metric) =>
  m === "station" || m === "official"
    ? usd
    : m === "tokens"
      ? short
      : (v: number) => NUM.format(v);
const tickMetric = (m: Metric) =>
  m === "station" || m === "official" ? usdTick : short;

function tipRows(t: RelayUsageTotals): [string, string][] {
  const tok = tokenSum(t);
  return [
    [
      "请求",
      `${NUM.format(t.requests)}${t.failed ? `（失败 ${NUM.format(t.failed)}）` : ""}`,
    ],
    ["Token", tok == null ? "—" : short(tok)],
    ["站内花费", usd(t.station_cost)],
    ["官方价折算", usd(t.official_cost)],
  ];
}

/** 线路 id（`软件␟站点␟分组`）里的站点与分组；`official` 是 turn-state 那条官方线。 */
export function routeParts(routeId: string): {
  station: string;
  group: string;
} {
  if (routeId === "official") return { station: "official", group: "" };
  const parts = routeId.split("\u001f");
  return parts.length >= 3
    ? { station: parts[1], group: parts.slice(2).join("\u001f") }
    : { station: routeId, group: "" };
}

function clientName(id: string): string {
  return STATION_CLIENTS.find((c) => c.id === id)?.name ?? id;
}

function GroupTable({
  title,
  groups,
  nameOf,
}: {
  title: string;
  groups: RelayUsageGroup[];
  nameOf?: (g: RelayUsageGroup) => string;
}) {
  return (
    <div className="qb-usage-tw mt-3">
      <table>
        <thead>
          <tr>
            <th>{title}</th>
            <th className="num">请求</th>
            <th className="num">失败率</th>
            <th className="num">Token</th>
            <th className="num">站内花费</th>
            <th className="num">官方价折算</th>
          </tr>
        </thead>
        <tbody>
          {groups.map((g) => {
            const tok = tokenSum(g.totals);
            return (
              <tr key={g.key}>
                <td className="clip" title={nameOf ? nameOf(g) : g.label}>
                  {nameOf ? nameOf(g) : g.label}
                </td>
                <td className="num">{NUM.format(g.totals.requests)}</td>
                <td className="num">
                  {pct(
                    g.totals.requests
                      ? g.totals.failed / g.totals.requests
                      : null,
                  )}
                </td>
                <td className="num">{tok == null ? "—" : short(tok)}</td>
                <td className="num">{usd(g.totals.station_cost)}</td>
                <td className="num">{usd(g.totals.official_cost)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

export default function RelayUsage() {
  const [params, setParams] = useSearchParams();
  const rawClient = params.get("client");
  const client: Client | null = STATION_CLIENTS.some((c) => c.id === rawClient)
    ? (rawClient as Client)
    : null;
  const [days, setDays] = useState(7);
  const [metric, setMetric] = useState<Metric>("station");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [data, setData] = useState<RelayUsageReport | null>(null);
  const [model, setModel] = useState("");
  const [failedOnly, setFailedOnly] = useState(false);
  const [page, setPage] = useState(0);
  const [logs, setLogs] = useState<RelayRequestPage | null>(null);
  const [logErr, setLogErr] = useState("");

  // 只认最后发出去的那一次（同 `/usage`）：先点「全部」再点「7 天」，慢的那次后回来不许盖掉快的。
  const seq = useRef(0);
  const load = useCallback(async () => {
    const mine = ++seq.current;
    setBusy(true);
    setErr("");
    try {
      const r = await stationApi.usageReport(client, days);
      if (mine === seq.current) setData(r);
    } catch (e) {
      if (mine === seq.current)
        setErr(e instanceof Error ? e.message : String(e));
    } finally {
      if (mine === seq.current) setBusy(false);
    }
  }, [client, days]);
  useEffect(() => {
    void load();
    const timer = window.setInterval(() => void load(), REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [load]);

  const logSeq = useRef(0);
  const loadLogs = useCallback(async () => {
    const mine = ++logSeq.current;
    setLogErr("");
    try {
      const r = await stationApi.usageRequests({
        client,
        routeId: null,
        model: model || null,
        failedOnly,
        offset: page * PAGE,
        limit: PAGE,
      });
      if (mine === logSeq.current) setLogs(r);
    } catch (e) {
      if (mine === logSeq.current)
        setLogErr(e instanceof Error ? e.message : String(e));
    }
  }, [client, model, failedOnly, page]);
  useEffect(() => {
    void loadLogs();
  }, [loadLogs]);
  // 换了软件 / 筛选就回第一页；换了软件上一边的模型名不一定还在。
  useEffect(() => {
    setPage(0);
  }, [client, model, failedOnly]);
  useEffect(() => {
    setModel("");
  }, [client]);

  const rangeName = RANGES.find(([d]) => d === days)?.[1] ?? "";
  const scope = client ? clientName(client) : "五个软件";
  const today = todayYmd();
  const t = data?.totals ?? null;
  const tok = t ? tokenSum(t) : null;

  const trend = useMemo((): { data: BarDatum[]; hourly: boolean } | null => {
    if (!data) return null;
    if (days === 1) {
      const byHour = new Map(data.hours.map((h) => [h.hour, h.totals]));
      const now = new Date().getHours();
      const out: BarDatum[] = [];
      for (let h = 0; h <= now; h++) {
        const x = byHour.get(h);
        const hh = String(h).padStart(2, "0");
        out.push({
          key: String(h),
          axis: hh,
          title: `${hh}:00–${hh}:59`,
          value: x ? metricOf(x, metric) : null,
          rows: x ? tipRows(x) : undefined,
        });
      }
      return { data: out, hourly: true };
    }
    const byDay = new Map(data.series.map((d) => [d.day, d.totals]));
    const span =
      days > 0
        ? lastDays(days, today)
        : data.first_day
          ? daysFrom(data.first_day, today)
          : [];
    return {
      data: span.map((day) => {
        const x = byDay.get(day);
        return {
          key: day,
          axis: shortDay(day),
          title: day === today ? `${day}（今天）` : day,
          value: x ? metricOf(x, metric) : null,
          rows: x ? tipRows(x) : undefined,
        };
      }),
      hourly: false,
    };
  }, [data, days, metric, today]);

  const heat: HeatDay[] = useMemo(
    () =>
      (data?.year ?? []).map((d) => ({
        day: d.day,
        value: d.totals.requests,
        rows: tipRows(d.totals),
      })),
    [data],
  );
  // 热力图的表格版：按月合计（一年 12 行，展开看）。
  const months = useMemo(() => {
    const m = new Map<
      string,
      { requests: number; station: number | null; official: number | null }
    >();
    for (const d of data?.year ?? []) {
      const key = d.day.slice(0, 7);
      const cur = m.get(key) ?? { requests: 0, station: null, official: null };
      cur.requests += d.totals.requests;
      if (d.totals.station_cost != null)
        cur.station = (cur.station ?? 0) + d.totals.station_cost;
      if (d.totals.official_cost != null)
        cur.official = (cur.official ?? 0) + d.totals.official_cost;
      m.set(key, cur);
    }
    return [...m.entries()].sort((a, b) => b[0].localeCompare(a[0]));
  }, [data]);

  const siteLabel = useMemo(() => {
    const m = new Map((data?.by_site ?? []).map((g) => [g.key, g.label]));
    return (routeId: string) => {
      const { station, group } = routeParts(routeId);
      const site = m.get(station) ?? station;
      return group ? `${site} · ${group}` : site;
    };
  }, [data]);

  const pages = logs ? Math.max(1, Math.ceil(logs.total / PAGE)) : 1;

  return (
    <div className="qb-page qb-usage" data-testid="relay-usage">
      <PageHeader
        title="中转站 · 用量明细"
        sub="只统计经本机路由的请求，全部读本机库，零网络请求。站内花费按线路公布的价算；官方价折算是同样的 token 走官方 API 的价，订阅不按这个收费。"
        actions={
          <Link to="/relays" className="btn btn--sm">
            <ArrowLeft size={12} /> 回中转站
          </Link>
        }
      />

      {err && (
        <p role="alert" className="notice notice--danger mb-3">
          {err}
        </p>
      )}

      {/* ------------------------------------------------ 筛选（下面全部跟着它） */}
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="usage-ranges" role="group" aria-label="软件">
          <Button
            size="sm"
            variant={client == null ? "primary" : "default"}
            aria-pressed={client == null}
            onClick={() => setParams({})}
          >
            全部
          </Button>
          {STATION_CLIENTS.map((c) => (
            <Button
              key={c.id}
              size="sm"
              variant={client === c.id ? "primary" : "default"}
              aria-pressed={client === c.id}
              onClick={() => setParams({ client: c.id })}
            >
              {c.short}
            </Button>
          ))}
        </span>
        <span className="usage-ranges" role="group" aria-label="时间范围">
          {RANGES.map(([d, name]) => (
            <Button
              key={d}
              size="sm"
              variant={days === d ? "primary" : "default"}
              aria-pressed={days === d}
              onClick={() => setDays(d)}
            >
              {name}
            </Button>
          ))}
        </span>
        <Button
          size="sm"
          icon={<RefreshCw size={12} />}
          loading={busy}
          aria-label="重新统计"
          title="立刻重读一遍。这一页每分钟也会自己重读。"
          onClick={() => {
            void load();
            void loadLogs();
          }}
        />
        <span className="notice">
          {scope} · {rangeName} · 每分钟自己重读
        </span>
      </div>
      <p className="notice mt-2">
        共存型工具（OpenCode、OpenClaw、Hermes、Pi、MiniMax
        Code）和桌面端直连档写的是真 Key、不经本机路由，这里看不到。
        {data?.first_day
          ? ` 最早的记录是 ${data.first_day}；30 天以前的只剩按天汇总，逐条日志只翻得到近 30 天。`
          : " 还没有任何经本机路由的请求。"}
      </p>

      <div className={busy && data ? "qb-usage-stale" : undefined}>
        {/* ---------------------------------------------- 概览 */}
        <Card title="概览" className="mt-3">
          <div className="ustats">
            <div className="ustat">
              <span className="ustat-name">请求数</span>
              <span className="ustat-value">
                {t ? NUM.format(t.requests) : "—"}
              </span>
              <span className="ustat-sub">
                {t && t.requests > 0
                  ? `失败 ${NUM.format(t.failed)} · 成功率 ${pct((t.requests - t.failed) / t.requests)}`
                  : rangeName}
              </span>
            </div>
            <div
              className="ustat"
              title={tok != null ? NUM.format(tok) : undefined}
            >
              <span className="ustat-name">Token 合计</span>
              <span className="ustat-value">
                {tok == null ? "—" : short(tok)}
              </span>
              <span className="ustat-sub">
                {t
                  ? `入 ${t.input_tokens == null ? "—" : short(t.input_tokens)} · 出 ${t.output_tokens == null ? "—" : short(t.output_tokens)} · 缓存写 ${t.cache_write_tokens == null ? "—" : short(t.cache_write_tokens)} · 缓存读 ${t.cache_read_tokens == null ? "—" : short(t.cache_read_tokens)}`
                  : "输入 + 输出 + 缓存写 + 缓存读"}
              </span>
            </div>
            <div className="ustat">
              <span className="ustat-name">站内花费</span>
              <span className="ustat-value">
                {t ? usd(t.station_cost) : "—"}
              </span>
              <span className="ustat-sub">按线路公布的价算</span>
            </div>
            <div className="ustat">
              <span className="ustat-name">官方价折算</span>
              <span className="ustat-value">
                {t ? usd(t.official_cost) : "—"}
              </span>
              <span className="ustat-sub">
                同样的 token 走官方 API · 不是账单
              </span>
            </div>
          </div>
        </Card>

        {/* ---------------------------------------------- 趋势 */}
        <Card
          title={trend?.hourly ? "趋势 · 今天按小时" : "趋势 · 按天"}
          className="mt-3"
          actions={
            <span
              className="qb-usage-switch"
              role="group"
              aria-label="柱子的高度"
            >
              {METRICS.map(([m, name]) => (
                <Button
                  key={m}
                  size="sm"
                  variant={metric === m ? "primary" : "default"}
                  aria-pressed={metric === m}
                  onClick={() => setMetric(m)}
                >
                  {name}
                </Button>
              ))}
            </span>
          }
        >
          {trend ? (
            <>
              <BarChart
                data={trend.data}
                format={tickMetric(metric)}
                valueFormat={fmtMetric(metric)}
                label={`${scope} ${rangeName}${trend.hourly ? "按小时" : "按天"}的${METRICS.find(([m]) => m === metric)?.[1]}`}
                highlight={trend.hourly ? String(new Date().getHours()) : today}
                empty="这一档时间范围里没有经本机路由的请求。"
              />
              <p className="notice mt-1">
                {trend.hourly
                  ? "柱子是每个小时的合计；没有柱子的小时没有请求。"
                  : "没有柱子的那天没有经本机路由的请求（或者那天的 token / 花费一条都没记到）。"}{" "}
                悬停或用方向键看每一根的明细。
              </p>
            </>
          ) : (
            <p className="notice">
              {busy ? "正在读本机库…" : "这一档还没有数据。"}
            </p>
          )}
        </Card>

        {/* ---------------------------------------------- 全年热力图 */}
        <Card title="全年 · 每天的请求数" className="mt-3">
          <YearHeatmap
            data={heat}
            today={today}
            firstDay={data?.first_day ?? null}
            format={(v) => `${NUM.format(v)} 次请求`}
            label={`${scope}近一年每天经本机路由的请求数`}
          />
          <YearHeatmapLegend hasGap={!!data?.first_day} />
          {months.length > 0 && (
            <details className="mt-2">
              <summary className="notice">按月看（表格版）</summary>
              <div className="qb-usage-tw mt-2">
                <table>
                  <thead>
                    <tr>
                      <th>月份</th>
                      <th className="num">请求</th>
                      <th className="num">站内花费</th>
                      <th className="num">官方价折算</th>
                    </tr>
                  </thead>
                  <tbody>
                    {months.map(([m, x]) => (
                      <tr key={m}>
                        <td>{m}</td>
                        <td className="num">{NUM.format(x.requests)}</td>
                        <td className="num">{usd(x.station)}</td>
                        <td className="num">{usd(x.official)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </details>
          )}
        </Card>

        {/* ---------------------------------------------- 按软件 / 站点 / 模型 */}
        {data && data.totals.requests > 0 && (
          <Card
            title={`按${client ? "" : "软件、"}站点、模型 · ${rangeName}`}
            className="mt-3"
          >
            {!client && data.by_client.length > 0 && (
              <GroupTable
                title="软件"
                groups={data.by_client}
                nameOf={(g) => clientName(g.key)}
              />
            )}
            <GroupTable title="站点" groups={data.by_site} />
            <GroupTable
              title="模型（客户端请求里写的）"
              groups={data.by_model}
            />
          </Card>
        )}

        {/* ---------------------------------------------- 逐条日志 */}
        <Card title="逐条日志 · 近 30 天" className="mt-3">
          <div className="flex flex-wrap items-center gap-2">
            <label className="notice">
              模型{" "}
              <select
                value={model}
                onChange={(e) => setModel(e.target.value)}
                aria-label="只看这个模型"
              >
                <option value="">全部</option>
                {(data?.by_model ?? [])
                  .filter((g) => g.key !== "（没写模型）")
                  .map((g) => (
                    <option key={g.key} value={g.key}>
                      {g.key}
                    </option>
                  ))}
              </select>
            </label>
            <label className="notice">
              <input
                type="checkbox"
                checked={failedOnly}
                onChange={(e) => setFailedOnly(e.target.checked)}
              />{" "}
              只看失败的
            </label>
          </div>
          {logErr && <p className="notice notice--danger mt-2">{logErr}</p>}
          {logs && logs.items.length > 0 ? (
            <>
              <div className="qb-usage-tw mt-2">
                <table>
                  <thead>
                    <tr>
                      <th>时间</th>
                      {!client && <th>软件</th>}
                      <th>站点 · 分组</th>
                      <th>模型</th>
                      <th className="num">状态</th>
                      <th className="num">首字 / 总耗时</th>
                      <th className="num">Token</th>
                      <th className="num">站内花费</th>
                    </tr>
                  </thead>
                  <tbody>
                    {logs.items.map((r) => {
                      // 生成的类型把 u64 写成 bigint，传过来的是 JSON 数字 —— 一律 Number() 一下。
                      const tk = [
                        r.input_tokens,
                        r.output_tokens,
                        r.cache_read_tokens,
                        r.cache_write_tokens,
                      ].map((x) => (x == null ? null : Number(x)));
                      const sum = tk.every((x) => x == null)
                        ? null
                        : tk.reduce<number>((a, x) => a + (x ?? 0), 0);
                      const ok =
                        r.fault == null &&
                        r.status != null &&
                        r.status >= 200 &&
                        r.status < 300;
                      return (
                        <tr key={r.id}>
                          <td>{stamp(Number(r.at_ms))}</td>
                          {!client && (
                            <td className="clip">{clientName(r.client)}</td>
                          )}
                          <td className="clip" title={siteLabel(r.route_id)}>
                            {siteLabel(r.route_id)}
                          </td>
                          <td
                            className="clip"
                            title={
                              r.upstream_model
                                ? `${r.model} → ${r.upstream_model}`
                                : r.model
                            }
                          >
                            {r.model || "—"}
                          </td>
                          <td
                            className={`num${ok ? "" : " qb-tone-danger"}`}
                            title={r.error ?? r.fault ?? undefined}
                          >
                            {r.status ?? "—"}
                            {r.attempts.length > 1
                              ? ` · 试了 ${r.attempts.length} 条`
                              : ""}
                          </td>
                          <td className="num">
                            {r.first_token_ms == null
                              ? "—"
                              : `${(Number(r.first_token_ms) / 1000).toFixed(1)}s`}{" "}
                            /{" "}
                            {r.total_ms == null
                              ? "—"
                              : `${(Number(r.total_ms) / 1000).toFixed(1)}s`}
                          </td>
                          <td className="num">
                            {sum == null ? "—" : short(sum)}
                          </td>
                          <td className="num">{usd(r.station_cost)}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
              <div className="qb-usage-pager">
                <span className="notice">共 {NUM.format(logs.total)} 条</span>
                <Button
                  size="sm"
                  disabled={page === 0}
                  onClick={() => setPage(page - 1)}
                >
                  上一页
                </Button>
                <span className="notice">
                  {page + 1} / {pages}
                </span>
                <Button
                  size="sm"
                  disabled={page >= pages - 1}
                  onClick={() => setPage(page + 1)}
                >
                  下一页
                </Button>
              </div>
            </>
          ) : (
            <p className="notice mt-2">
              {logs
                ? failedOnly || model
                  ? "没有符合条件的请求。"
                  : "近 30 天没有经本机路由的请求。"
                : "正在读本机库…"}
            </p>
          )}
        </Card>
      </div>
    </div>
  );
}
