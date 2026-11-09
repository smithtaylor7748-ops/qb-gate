// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { StationUsageKpis } from "./StationUsageKpis";
const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
afterEach(() => {
  cleanup();
  mock.call.mockReset();
});

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

it("shows rolling usage without a site bill or loaded log page, with all four token categories", async () => {
  mock.call.mockResolvedValue(totals);
  render(<StationUsageKpis client="codex" range="1h" />);
  expect(await screen.findByText("155")).toBeTruthy();
  expect(screen.getByText("2,201")).toBeTruthy();
  expect(screen.getByText("<$0.01")).toBeTruthy();
  expect(mock.call).toHaveBeenCalledWith("station_usage_totals", {
    client: "codex",
    hours: 1,
  });
  expect(screen.getByText(/USD 站内额度估算/)).toBeTruthy();
});

it("keeps unknown cost unknown and refetches for the client and all-time range", async () => {
  mock.call.mockResolvedValue({ ...totals, station_cost: null });
  const view = render(<StationUsageKpis client="codex" range="24h" />);
  expect(await screen.findByText(/缺少用量或站内价格，无法估算/)).toBeTruthy();
  expect(screen.queryByText("$0.00")).toBeNull();
  view.rerender(<StationUsageKpis client="claude-code" range="all" />);
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("station_usage_totals", {
      client: "claude-code",
      hours: null,
    }),
  );
});

it("ignores a stale response after switching to another client", async () => {
  let resolveOld: ((value: typeof totals) => void) | undefined;
  mock.call.mockImplementation((_cmd, args) =>
    args.client === "codex"
      ? new Promise((resolve) => {
          resolveOld = resolve;
        })
      : Promise.resolve({ ...totals, requests: 8 }),
  );
  const view = render(<StationUsageKpis client="codex" range="7d" />);
  view.rerender(<StationUsageKpis client="claude-code" range="7d" />);
  expect(await screen.findByText("8")).toBeTruthy();
  resolveOld?.(totals);
  await waitFor(() => expect(screen.queryByText("2,201")).toBeNull());
});
