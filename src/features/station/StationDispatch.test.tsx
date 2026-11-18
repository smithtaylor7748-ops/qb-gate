// @vitest-environment jsdom
// 「检测与排名」（选线与分流 · 高级设置里）和挪到「设置 → 中转站」的测活 / 网络设置（2026-11-15）。
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import StationDispatch from "./StationDispatch";
import {
  ProbeSettingsPanel,
  RouterNetworkSettings,
} from "./StationNetworkSettings";
import { ToastProvider } from "../../ui";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
const probe = {
  paid: false,
  daily_budget_cny: 1,
  free_interval_min: 5,
  paid_min_interval_min: 5,
  paid_max_interval_min: 60,
};
const line = (route_id: string) => ({
  route_id,
  label: route_id,
  health: { state: "up", reason: "", missing_models: [] },
  speed_ms: null,
  speed_samples: 0,
  fail_rate: null,
  real_rate: null,
  last_probe_ms: null,
  next_probe_ms: null,
  requests_24h: 0,
});
const view = {
  client: "codex",
  enabled: false,
  queue: { current: "main", rows: [], alerts: [] },
  // 「outside」不是当前线路也不是备用线路：不列、不测。
  lines: [line("main"), line("cheap"), line("outside")],
  spent_today: [0, 0],
  settings: probe,
  actions: [],
};
const status = {
  current_route: "main",
  pending_route: null,
  admission: [],
  selection: { current: "main", backups: ["cheap"] },
};

function response(command: string, args?: Record<string, unknown>) {
  if (command === "station_dispatch_view") return view;
  if (command === "station_router_status") return status;
  if (command === "station_router_settings")
    return {
      routing_mode: "pool",
      allowed_routes: ["cheap"],
      admission: {
        overflow_to_pool: false,
        route_limits: {},
        max_wait_ms: 0,
        max_waiters: 0,
      },
    };
  if (command === "station_probe_now")
    return {
      attempts: [],
      paid_pending: args?.paid
        ? []
        : [{ route_id: "main", model: "gpt", cost_cny: 0.01 }],
      paid_estimate_cny: args?.paid ? 0 : 0.01,
      over_budget: 0,
    };
  if (command === "station_probe_settings_save") return args?.settings;
  if (command === "station_router_network")
    return { scope: "loopback", key: null, outbound_proxy: null, port: 15721 };
  if (command === "station_router_listen_save")
    return {
      scope: args?.scope,
      key: args?.scope === "all-interfaces" ? "k" : null,
      outbound_proxy: null,
      port: 15721,
    };
  throw new Error(`Unexpected command: ${command}`);
}
beforeEach(() => {
  mock.call.mockReset();
  mock.call.mockImplementation(async (command, args) =>
    response(command, args),
  );
});
afterEach(cleanup);
const wrap = (node: React.ReactNode) =>
  render(<ToastProvider>{node}</ToastProvider>);
const calls = (command: string) =>
  mock.call.mock.calls.filter(([c]) => c === command);

it("lists only the current line and backups, and re-checks free first, paid only after a second click", async () => {
  wrap(<StationDispatch client="codex" />);
  await screen.findByText("当前线路与备用线路的检测");
  expect(screen.getAllByText("main").length).toBeGreaterThan(0);
  expect(screen.getAllByText("cheap").length).toBeGreaterThan(0);
  expect(screen.queryByText("outside")).toBeNull();
  // D7：只有一个「重新检测」，原来的「立刻测」和每行的「测」都没了。
  expect(screen.queryByRole("button", { name: "立刻测" })).toBeNull();
  expect(screen.queryByRole("button", { name: "测" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "重新检测" }));
  const paid = await screen.findByRole("button", { name: "花钱测这几条" });
  expect(calls("station_probe_now")).toHaveLength(1);
  expect(calls("station_probe_now")[0][1]).toMatchObject({ paid: false });
  fireEvent.click(paid);
  await waitFor(() => expect(calls("station_probe_now")).toHaveLength(2));
  expect(calls("station_probe_now")[1][1]).toMatchObject({ paid: true });
});

it("the moved range card, recent request and debounce editor are gone from here", async () => {
  wrap(<StationDispatch client="codex" />);
  await screen.findByText("当前线路与备用线路的检测");
  expect(screen.queryByText("最近请求实际去向")).toBeNull();
  expect(screen.queryByText(/仅所选线路|已允许/)).toBeNull();
  expect(screen.queryByLabelText(/领先.*至少多少/)).toBeNull();
  expect(screen.queryByLabelText("开启花钱测活")).toBeNull();
});

it("probe settings are a draft until saved, and a failed save keeps the draft", async () => {
  let fail = true;
  mock.call.mockImplementation(async (command, args) => {
    if (command === "station_probe_settings_save" && fail) {
      fail = false;
      throw new Error("write failed");
    }
    return response(command, args);
  });
  wrap(<ProbeSettingsPanel />);
  const paid = (await screen.findByRole("checkbox", {
    name: "开启花钱测活",
  })) as HTMLInputElement;
  fireEvent.click(paid);
  expect(calls("station_probe_settings_save")).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", { name: "保存测活设置" }));
  await screen.findByText("测活设置未保存：write failed");
  expect(paid.checked).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "保存测活设置" }));
  await waitFor(() =>
    expect(calls("station_probe_settings_save")).toHaveLength(2),
  );
  expect(calls("station_probe_settings_save")[1][1]).toMatchObject({
    settings: { paid: true },
  });
});

it("changing the listen scope asks first and only saves after confirming", async () => {
  wrap(<RouterNetworkSettings />);
  fireEvent.click(await screen.findByRole("radio", { name: /所有网卡/ }));
  // A4：原来一点就保存并重启路由。
  expect(calls("station_router_listen_save")).toHaveLength(0);
  expect(screen.getByText(/拿到口令的人能用你的线路额度/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "取消" }));
  expect(calls("station_router_listen_save")).toHaveLength(0);
  fireEvent.click(screen.getByRole("radio", { name: /所有网卡/ }));
  fireEvent.click(screen.getByRole("button", { name: "改成所有网卡" }));
  await waitFor(() =>
    expect(calls("station_router_listen_save")).toHaveLength(1),
  );
  expect(calls("station_router_listen_save")[0][1]).toEqual({
    scope: "all-interfaces",
  });
  await screen.findByLabelText("路由口令");
});
