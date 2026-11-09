// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import type { Route, RouterRequest } from "../../lib/station";
import StationLogs from "./StationLogs";
import RelayUsage from "./RelayUsage";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
afterEach(cleanup);

function request(id: string, attempted: boolean): RouterRequest {
  return {
    id,
    at_ms: BigInt(Date.now()),
    client: "codex",
    route_id: "codex\u001ffree\u001fdefault",
    model: attempted ? "attempted-model" : "blocked-model",
    upstream_model: "",
    status: 403,
    first_token_ms: null,
    total_ms: null,
    retry_after_ms: null,
    streaming: false,
    input_tokens: null,
    output_tokens: null,
    cache_read_tokens: null,
    cache_write_tokens: null,
    cache_write_1h_tokens: null,
    station_cost: null,
    official_cost: null,
    session_id: null,
    fault: "route-permission-denied",
    error: "线路不在允许范围",
    attempts: attempted
      ? [
          {
            route_id: "codex\u001ffree\u001fdefault",
            status: 429,
            fault: "rate-limit",
            ms: 1n,
          },
        ]
      : [],
  };
}
const rows = [request("blocked", false), request("attempted", true)];
const capacityRows = rows.map((row) => ({
  ...row,
  status: 429,
  fault: "route-admission-denied",
  error: "本机等待队列已满",
}));

beforeEach(() => {
  mock.call.mockReset();
  mock.call.mockImplementation(async (command) => {
    if (command === "station_logs") return rows;
    if (command === "station_usage_requests") return { items: rows, total: 2 };
    if (command === "station_usage_report")
      return {
        client: "codex",
        days: 7,
        totals: {
          requests: 2,
          failed: 2,
          input_tokens: null,
          output_tokens: null,
          cache_read_tokens: null,
          cache_write_tokens: null,
          station_cost: null,
          official_cost: null,
        },
        series: [],
        hours: [],
        by_client: [],
        by_site: [],
        by_model: [],
        year: [],
        first_day: null,
      };
    return null;
  });
});

it("never describes a local permission refusal as an upstream circuit breaker", async () => {
  const route = {
    id: rows[0].route_id,
    group: "default",
    rates: {},
    model_map: { exact: [] },
    protocols: {},
    credential_id: null,
    nominal_rate: null,
    latest_mult: null,
  } as unknown as Route;
  render(
    <StationLogs
      route={route}
      siteUrl="https://example.invalid"
      credentials={[]}
    />,
  );
  await screen.findAllByText("本机拒绝 · 超出允许范围");
  expect(screen.queryByText("403 · 熔断")).toBeNull();
  expect(screen.getAllByText("未发往上游")).toHaveLength(1);
});

it("preserves an attempted route in usage history and marks only empty attempts unsent", async () => {
  render(
    <MemoryRouter initialEntries={["/relays/usage?client=codex"]}>
      <RelayUsage />
    </MemoryRouter>,
  );
  const blocked = (await screen.findByText("blocked-model")).closest("tr")!;
  const attempted = screen.getByText("attempted-model").closest("tr")!;
  expect(within(blocked).getByText("未发往上游")).toBeTruthy();
  expect(within(attempted).queryByText("未发往上游")).toBeNull();
  expect(within(attempted).getByText(/free/)).toBeTruthy();
  expect(screen.getAllByText("本机拒绝 · 超出允许范围")).toHaveLength(2);
});

it("does not describe a local admission refusal as upstream rate limiting", async () => {
  const original = mock.call.getMockImplementation()!;
  mock.call.mockImplementation((command, args) =>
    command === "station_logs"
      ? Promise.resolve(capacityRows)
      : original(command, args),
  );
  render(
    <StationLogs
      route={
        {
          id: rows[0].route_id,
          group: "default",
          rates: {},
          model_map: { exact: [] },
          protocols: {},
          credential_id: null,
          nominal_rate: null,
          latest_mult: null,
        } as unknown as Route
      }
      siteUrl="https://example.invalid"
      credentials={[]}
    />,
  );
  await screen.findAllByText("本机繁忙 · 请求容量限制");
  expect(screen.queryByText("429 · 降权")).toBeNull();
  expect(screen.getAllByText("未发往上游")).toHaveLength(1);
});

it("marks only unsent capacity refusals as not sent in usage history", async () => {
  const original = mock.call.getMockImplementation()!;
  mock.call.mockImplementation((command, args) =>
    command === "station_usage_requests"
      ? Promise.resolve({ items: capacityRows, total: 2 })
      : original(command, args),
  );
  render(
    <MemoryRouter initialEntries={["/relays/usage?client=codex"]}>
      <RelayUsage />
    </MemoryRouter>,
  );
  const blocked = (await screen.findByText("blocked-model")).closest("tr")!;
  const attempted = screen.getByText("attempted-model").closest("tr")!;
  expect(within(blocked).getByText("未发往上游")).toBeTruthy();
  expect(within(attempted).getByText(/free/)).toBeTruthy();
  expect(screen.getAllByText("本机繁忙 · 请求容量限制")).toHaveLength(2);
});
