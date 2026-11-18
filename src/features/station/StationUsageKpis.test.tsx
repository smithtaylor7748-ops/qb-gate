// @vitest-environment jsdom
// 首页四格只照 `station_usage_overview` 那一次查询的总数显示（2026-11-15 起没有单独的总数命令了）；
// 换软件时丢掉旧回复那一条在 StationUsageChart.test.tsx 的 Harness 里。
import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { StationUsageKpiValues } from "./StationUsageKpis";
afterEach(cleanup);

const totals = {
  requests: 2201,
  failed: 1,
  input_tokens: 100,
  output_tokens: 20,
  cache_read_tokens: 30,
  cache_write_tokens: 5,
  station_cost: 0.001,
  official_cost: 1,
};

it("shows rolling usage with all four token categories", () => {
  render(<StationUsageKpiValues totals={totals} error={false} range="1h" />);
  expect(screen.getByText("155")).toBeTruthy();
  expect(screen.getByText("2,201")).toBeTruthy();
  expect(screen.getByText("<$0.01")).toBeTruthy();
  expect(screen.getByText(/USD 站内额度估算/)).toBeTruthy();
});

it("keeps unknown cost unknown instead of showing $0.00", () => {
  render(
    <StationUsageKpiValues
      totals={{ ...totals, station_cost: null }}
      error={false}
      range="24h"
    />,
  );
  expect(screen.getByText(/缺少用量或站内价格，无法估算/)).toBeTruthy();
  expect(screen.queryByText("$0.00")).toBeNull();
});

it("says the read failed rather than showing zeros", () => {
  render(<StationUsageKpiValues totals={null} error range="7d" />);
  expect(screen.getAllByText(/读取本机用量失败/).length).toBe(3);
  expect(screen.queryByText("0")).toBeNull();
});
