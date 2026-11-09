// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type { Route, RouterSettings, RouterStatus } from "../../lib/station";
import StationRouterSettings from "./StationRouterSettings";

const mock = vi.hoisted(() => ({ call: vi.fn(), ok: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
vi.mock("../../ui", async (original) => ({
  ...(await original<typeof import("../../ui")>()),
  useToast: () => ({ ok: mock.ok, error: vi.fn(), info: vi.fn() }),
}));

const settings: RouterSettings = {
  client: "codex",
  routing_mode: "selected",
  allowed_routes: [],
  pool_strategy: "priority",
  admission: { route_limits: {}, max_wait_ms: 0, max_waiters: 0 },
  max_attempts: 3,
  first_byte_timeout_secs: 60,
  idle_timeout_secs: 300,
  non_streaming_timeout_secs: 600,
  breaker: {
    trip_after_consecutive: 5,
    error_rate: 0.6,
    min_requests: 10,
    half_open_successes: 1,
  },
  rectifier: {
    enabled: true,
    thinking_signature: true,
    thinking_budget: true,
    media_fallback: true,
    media_heuristic: true,
  },
  aggregate: { enabled: false, members: [], models: {}, keys: {} },
};
const routes = ["free", "paid"].map<Route>((id) => ({
  id,
  client: "codex",
  station_id: id,
  group: id,
  credential_id: `${id}-key`,
  nominal_rate: null,
  latest_mult: null,
  last_audit_ms: null,
  output_markup: null,
  protocols: { anthropic: null, openai_chat: null, openai_responses: true },
  rates: {
    model_ratio: null,
    completion_ratio: null,
    cache_ratio: null,
    create_cache_ratio: null,
    group_ratio: null,
    peak_rate: null,
    per_request_price: null,
    input_price: null,
    cache_read_price: null,
    cache_write_price: null,
    output_price: null,
  },
  rates_model: null,
  model_map: {
    exact: [],
    haiku: null,
    sonnet: null,
    opus: null,
    fable: null,
    subagent: null,
    default: null,
  },
  upstream_format: null,
}));
const status = {
  current_route: "paid",
  pending_route: "free",
  breakers: [],
} as unknown as RouterStatus;
const writes = () =>
  mock.call.mock.calls.filter(
    ([command]) => command === "station_router_settings_save",
  );
const props = {
  client: "codex" as const,
  routes,
  status,
  label: (route: Route) => route.group,
};

beforeEach(() => {
  mock.call.mockReset();
  mock.ok.mockReset();
  mock.call.mockImplementation(async (command, args) => {
    if (command === "station_router_settings") return structuredClone(settings);
    if (
      command === "station_schedules" ||
      command === "station_aggregate_models"
    )
      return [];
    if (command === "station_router_settings_save") return args.settings;
    if (command === "station_router_network")
      return { scope: "loopback", key: null, outbound_proxy: null, port: 3157 };
    if (command === "station_dispatch_queue")
      return { order: ["paid", "free"] };
    return null;
  });
});
afterEach(cleanup);

it("does not turn an old fallback queue into approval when opened or switched to pool", async () => {
  render(<StationRouterSettings {...props} />);
  await screen.findByRole("radio", { name: /仅所选线路/ });
  expect(writes()).toHaveLength(0);
  fireEvent.click(screen.getByRole("radio", { name: /允许所勾选线路/ }));
  const checks = screen.getAllByRole("checkbox", {
    name: /允许使用/,
  }) as HTMLInputElement[];
  expect(checks.map((input) => input.checked)).toEqual([false, false]);
  expect(
    mock.call.mock.calls.some(
      ([command]) => command === "station_dispatch_queue",
    ),
  ).toBe(false);
});

it("requires the pending selection and saves policy with the allowlist in one transaction", async () => {
  const onSaved = vi.fn();
  render(<StationRouterSettings {...props} onSaved={onSaved} />);
  fireEvent.click(await screen.findByRole("radio", { name: /允许所勾选线路/ }));
  fireEvent.click(screen.getByRole("checkbox", { name: "允许使用「paid」" }));
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await screen.findByText("请把当前所选线路勾入允许名单，再保存。");
  expect(writes()).toHaveLength(0);
  fireEvent.click(screen.getByRole("checkbox", { name: "允许使用「free」" }));
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() => expect(onSaved).toHaveBeenCalledOnce());
  expect(writes()).toHaveLength(1);
  expect(writes()[0][1].settings).toMatchObject({
    routing_mode: "pool",
    allowed_routes: ["paid", "free"],
  });
  expect(
    mock.call.mock.calls.some(
      ([command]) => command === "station_dispatch_queue_save",
    ),
  ).toBe(false);
});

it("requires an explicit current choice before authorizing pool use", async () => {
  render(<StationRouterSettings {...props} status={null} />);
  fireEvent.click(await screen.findByRole("radio", { name: /允许所勾选线路/ }));
  fireEvent.click(screen.getByRole("checkbox", { name: "允许使用「free」" }));
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await screen.findByText("请先在中转站页选择一条线路，再设置允许名单。");
  expect(writes()).toHaveLength(0);
});

it("keeps failed saves out of the effective state and prevents edits during an outstanding save", async () => {
  const original = mock.call.getMockImplementation()!;
  let rejectSave: (error: Error) => void = () => {};
  mock.call.mockImplementation((command, args) =>
    command === "station_router_settings_save"
      ? new Promise((_resolve, reject) => {
          rejectSave = reject;
        })
      : original(command, args),
  );
  const onSaved = vi.fn();
  render(<StationRouterSettings {...props} onSaved={onSaved} />);
  const radio = await screen.findByRole("radio", { name: /仅所选线路/ });
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() => expect(writes()).toHaveLength(1));
  expect(radio.closest("fieldset")?.disabled).toBe(true);
  rejectSave(new Error("transaction failed"));
  await screen.findByText("transaction failed");
  expect(onSaved).not.toHaveBeenCalled();
  expect(mock.ok).not.toHaveBeenCalled();
  expect(radio.closest("fieldset")?.disabled).toBe(false);
});

it("starts without admission limits and saves load distribution only with an explicit pool", async () => {
  render(<StationRouterSettings {...props} />);
  await screen.findByRole("radio", { name: /仅所选线路/ });
  expect(screen.queryByRole("radio", { name: /优先空闲线路/ })).toBeNull();
  expect(
    (screen.getByLabelText("free · 同时进行的请求上限") as HTMLInputElement)
      .value,
  ).toBe("0");
  expect(
    (screen.getByLabelText("free · 使用已查询的额度状态") as HTMLInputElement)
      .checked,
  ).toBe(false);
  fireEvent.change(screen.getByLabelText("free · 同时进行的请求上限"), {
    target: { value: "2" },
  });
  fireEvent.change(screen.getByLabelText("free · 每分钟请求上限"), {
    target: { value: "30" },
  });
  fireEvent.click(screen.getByLabelText("free · 使用已查询的额度状态"));
  fireEvent.change(screen.getByLabelText("最多等待（毫秒）"), {
    target: { value: "3000" },
  });
  fireEvent.change(screen.getByLabelText("每条线路最多排队请求数"), {
    target: { value: "4" },
  });
  fireEvent.click(screen.getByRole("radio", { name: /允许所勾选线路/ }));
  fireEvent.click(screen.getByRole("radio", { name: /优先空闲线路/ }));
  expect(writes()).toHaveLength(0);
  expect(
    (
      screen.getByRole("checkbox", {
        name: "允许使用「free」",
      }) as HTMLInputElement
    ).checked,
  ).toBe(false);
  fireEvent.click(screen.getByRole("checkbox", { name: "允许使用「free」" }));
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() => expect(mock.ok).toHaveBeenCalledOnce());
  expect(writes()).toHaveLength(1);
  expect(writes()[0][1].settings).toMatchObject({
    routing_mode: "pool",
    allowed_routes: ["free"],
    pool_strategy: "least-loaded",
    admission: {
      route_limits: { free: { max_concurrent: 2, rpm: 30, check_quota: true } },
      max_wait_ms: 3000,
      max_waiters: 4,
    },
  });
});

it.each(["-1", "1.5"])(
  "refuses invalid admission input %s without sending a save",
  async (value) => {
    render(<StationRouterSettings {...props} />);
    fireEvent.change(await screen.findByLabelText("free · 每分钟请求上限"), {
      target: { value },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await screen.findByText("请求容量与等待设置必须是大于或等于 0 的整数。");
    expect(writes()).toHaveLength(0);
  },
);

it("rejects waiting above the bounded maximum before save", async () => {
  render(<StationRouterSettings {...props} />);
  fireEvent.change(await screen.findByLabelText("最多等待（毫秒）"), {
    target: { value: "60001" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await screen.findByText(
    "最多等待 60000 毫秒，每条线路最多排队 1024 个请求。",
  );
  expect(writes()).toHaveLength(0);
});
