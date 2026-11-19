// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type { RelayUsageOverview } from "../../lib/generated/RelayUsageOverview";
import type { RelayUsageTotals } from "../../lib/generated/RelayUsageTotals";
import {
  requestBars,
  StationUsageChart,
  useStationUsageOverview,
} from "./StationUsageChart";
import { StationUsageKpiValues } from "./StationUsageKpis";
import StationLogs from "./StationLogs";
import type { Route } from "../../lib/station";
import type { Client } from "../../lib/generated/Client";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
afterEach(() => {
  cleanup();
  mock.call.mockReset();
});

const totals = (requests: number): RelayUsageTotals => ({
  requests,
  failed: 1,
  input_tokens: null,
  output_tokens: null,
  cache_read_tokens: null,
  cache_write_tokens: null,
  station_cost: null,
  official_cost: null,
});
const start = new Date(2026, 10, 9, 9, 30).getTime();
const overview: RelayUsageOverview = {
  since_ms: start,
  through_ms: start + 3_600_000,
  granularity: "hour",
  totals: totals(2304),
  by_route: [{ key: "a", label: "a", totals: totals(2304) }],
  buckets: [
    { start_ms: start, end_ms: start + 1_800_000, totals: totals(2302) },
    {
      start_ms: start + 1_800_000,
      end_ms: start + 3_600_000,
      totals: totals(2),
    },
  ],
};

it("uses time ticks and a keyboard-readable exact interval and count", () => {
  render(<StationUsageChart data={overview} error={null} />);
  expect(screen.getByText("09:30")).toBeTruthy();
  expect(screen.getByText("10:00")).toBeTruthy();
  const chart = screen.getByRole("group", { name: /本机路由请求数/ });
  fireEvent.focus(chart);
  fireEvent.keyDown(chart, { key: "Home" });
  expect(
    screen.getByText(
      /2026-11-09 09:30:00 至 2026-11-09 10:00:00（不含结束时刻）$/,
    ),
  ).toBeTruthy();
  expect(screen.getAllByText("2,302 次").length).toBeGreaterThan(0);
  fireEvent.keyDown(chart, { key: "End" });
  expect(
    screen.getByText(
      /2026-11-09 10:00:00 至 2026-11-09 10:30:00（含结束时刻）$/,
    ),
  ).toBeTruthy();
  expect(screen.getByText(/本机完整记录 · 2,304 次/)).toBeTruthy();
});

it("combines a long history without dropping counts or compressing missing dates", () => {
  const buckets = Array.from({ length: 401 }, (_, i) => ({
    start_ms: start + i * 86_400_000,
    end_ms: start + (i + 1) * 86_400_000,
    totals: { ...totals(i === 0 ? 2201 : i === 400 ? 3 : 0), failed: 0 },
  }));
  const { bars, size } = requestBars({
    ...overview,
    granularity: "day",
    buckets,
  });
  expect(size).toBe(7);
  expect(bars).toHaveLength(58);
  expect(bars.reduce((sum, b) => sum + (b.value ?? 0), 0)).toBe(2204);
  expect(bars[0].key).toBe(String(start));
  expect(bars[1].value).toBe(0);
  expect(bars[bars.length - 1].value).toBe(3);
});

it("token and price bars use the KPI totals and never draw a missing bucket as zero", () => {
  const known: RelayUsageTotals = {
    requests: 3,
    failed: 0,
    input_tokens: 100,
    output_tokens: 20,
    cache_read_tokens: null,
    cache_write_tokens: 5,
    station_cost: 0.5,
    official_cost: 2,
  };
  const data: RelayUsageOverview = {
    ...overview,
    totals: known,
    buckets: [
      { ...overview.buckets[0], totals: known },
      // 有请求、一类 token、一分钱都没记到：不画柱，提示框写「没记到」。
      { ...overview.buckets[1], totals: totals(2) },
    ],
  };
  const tokens = requestBars(data, "tokens").bars;
  expect(tokens[0].value).toBe(125);
  expect(tokens[0].rows).toContainEqual(["缓存读", "没记到"]);
  expect(tokens[1].value).toBeNull();
  expect(tokens[1].missing).toBe("没记到");
  const cost = requestBars(data, "cost").bars;
  expect(cost[0].value).toBe(0.5);
  expect(cost[1].value).toBeNull();
  render(
    <StationUsageChart
      data={data}
      error={null}
      metric="tokens"
      onMetric={() => {}}
    />,
  );
  expect(screen.getByRole("combobox", { name: "柱状图" })).toBeTruthy();
  expect(
    screen.getByText(/本机完整记录 · 125 Token · 每小时一柱/),
  ).toBeTruthy();
});

function Harness({ client }: { client: Client }) {
  const usage = useStationUsageOverview(client, "24h", "hour");
  return (
    <>
      <StationUsageKpiValues
        range="24h"
        totals={usage.data?.totals ?? null}
        error={usage.error != null}
      />
      <StationUsageChart data={usage.data} error={usage.error} />
    </>
  );
}

it("shares the complete query for KPI and chart and discards old client results", async () => {
  let resolveOld: ((value: RelayUsageOverview) => void) | undefined;
  mock.call.mockImplementation((_command, args) =>
    args.client === "codex"
      ? new Promise((resolve) => {
          resolveOld = resolve;
        })
      : Promise.resolve({
          ...overview,
          totals: totals(8),
          buckets: [{ ...overview.buckets[0], totals: totals(8) }],
        }),
  );
  const view = render(<Harness client="codex" />);
  view.rerender(<Harness client="claude-code" />);
  expect(
    await screen.findByText("8", { selector: ".qb-st-kpi .v" }),
  ).toBeTruthy();
  resolveOld?.(overview);
  await waitFor(() => expect(screen.queryByText("2,304")).toBeNull());
  expect(mock.call).toHaveBeenCalledTimes(2);
  expect(mock.call).toHaveBeenLastCalledWith("station_usage_overview", {
    client: "claude-code",
    hours: 24,
    granularity: "hour",
    routeId: null,
  });
});

it("route logs show complete totals even when their latency sample has no loaded rows", async () => {
  mock.call.mockImplementation((command) =>
    Promise.resolve(command === "station_logs" ? [] : overview),
  );
  const route = {
    id: "a",
    client: "codex",
    group: "default",
    credential_id: null,
    upstream_format: null,
    protocols: {
      anthropic: null,
      openai_chat: null,
      openai_responses: null,
      gemini: null,
    },
  } as unknown as Route;
  render(
    <StationLogs
      route={route}
      credentials={[]}
      siteUrl="https://example.test"
    />,
  );
  expect(await screen.findByText("2,304")).toBeTruthy();
  expect(screen.getByText("首字 P95（样本）")).toBeTruthy();
  expect(
    screen.getByText(/最近日志样本中没有此线路的记录；完整请求数见上方统计/),
  ).toBeTruthy();
  expect(
    screen.getByText(/最近最多 2,000 条日志中涉及此线路的 0 条样本/),
  ).toBeTruthy();
  expect(mock.call).toHaveBeenCalledWith("station_usage_overview", {
    client: "codex",
    hours: 24,
    granularity: "hour",
    routeId: "a",
  });
});
