// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import RelayUsage, { routeParts, tokenSum } from "./RelayUsage";
import type { RelayUsageReport } from "../../lib/generated/RelayUsageReport";
import type { RelayUsageTotals } from "../../lib/generated/RelayUsageTotals";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));

const US = String.fromCharCode(31);
const route = ["codex", "moxi", "GPT 标准组"].join(US);
const totals = (over: Partial<RelayUsageTotals> = {}): RelayUsageTotals => ({
  requests: 12,
  failed: 3,
  input_tokens: 1000,
  output_tokens: 200,
  cache_read_tokens: null,
  cache_write_tokens: null,
  station_cost: 0.5,
  official_cost: null,
  ...over,
});
const report: RelayUsageReport = {
  client: null,
  days: 7,
  totals: totals(),
  series: [],
  hours: [],
  by_client: [{ key: "codex", label: "codex", totals: totals() }],
  by_site: [{ key: "moxi", label: "备用中转", totals: totals() }],
  by_model: [{ key: "gpt-5.6-sol", label: "gpt-5.6-sol", totals: totals() }],
  year: [],
  first_day: "2026-10-01",
};
const log = {
  id: "l1",
  at_ms: 1_793_000_000_000,
  client: "codex",
  route_id: route,
  model: "gpt-5.6-sol",
  upstream_model: "",
  status: 429,
  first_token_ms: null,
  total_ms: 1200,
  retry_after_ms: null,
  streaming: true,
  input_tokens: null,
  output_tokens: null,
  cache_read_tokens: null,
  cache_write_tokens: null,
  cache_write_1h_tokens: null,
  station_cost: null,
  official_cost: null,
  session_id: null,
  fault: "rate-limit",
  error: "rate-limit（HTTP 429）",
  attempts: [{ route_id: route, status: 429, fault: "rate-limit", ms: 1200 }],
};

beforeEach(() => {
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    if (command === "station_usage_report") return report;
    if (command === "station_usage_requests") return { items: [log], total: 1 };
  });
});
afterEach(cleanup);

it("reads the local report, says what it cannot see, and never turns a missing number into zero", async () => {
  render(
    <MemoryRouter initialEntries={["/relays/usage"]}>
      <RelayUsage />
    </MemoryRouter>,
  );
  expect(await screen.findByText("成功率 75%", { exact: false })).toBeTruthy();
  expect(mock.call).toHaveBeenCalledWith("station_usage_report", {
    client: null,
    days: 7,
  });
  expect(screen.getByText(/共存型工具.*不经本机路由，这里看不到/)).toBeTruthy();
  // 官方价折算没记到：写「—」，不写 $0.00。
  const official = screen.getByText("官方价折算", { selector: ".ustat-name" });
  expect(official.parentElement?.textContent).toContain("—");
  expect(official.parentElement?.textContent).not.toContain("$0.00");
  // 逐条日志：站点按线路 id 拆出来、用概览里的站点名；失败的那条标出来。
  expect(await screen.findByText("备用中转 · GPT 标准组")).toBeTruthy();
});

it("scopes everything to one client and filters the log to failures", async () => {
  render(
    <MemoryRouter initialEntries={["/relays/usage?client=codex"]}>
      <RelayUsage />
    </MemoryRouter>,
  );
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("station_usage_report", {
      client: "codex",
      days: 7,
    }),
  );
  fireEvent.click(await screen.findByLabelText(/只看失败的/));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("station_usage_requests", {
      client: "codex",
      routeId: null,
      model: null,
      failedOnly: true,
      offset: 0,
      limit: 50,
    }),
  );
  // 「全部」有两个（软件、时间范围），点软件那一组里的。
  fireEvent.click(
    within(screen.getByRole("group", { name: "软件" })).getByRole("button", {
      name: "全部",
    }),
  );
  await waitFor(() =>
    expect(mock.call).toHaveBeenLastCalledWith(
      expect.stringMatching(/^station_usage_/),
      expect.objectContaining({ client: null }),
    ),
  );
});

it("splits route ids and adds tokens only when something was recorded", () => {
  expect(routeParts(route)).toEqual({ station: "moxi", group: "GPT 标准组" });
  expect(routeParts("official")).toEqual({ station: "official", group: "" });
  expect(tokenSum(totals())).toBe(1200);
  expect(
    tokenSum(
      totals({
        input_tokens: null,
        output_tokens: null,
        cache_read_tokens: null,
        cache_write_tokens: null,
      }),
    ),
  ).toBeNull();
});
