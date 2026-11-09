// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import StationDispatch from "./StationDispatch";
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
const dispatch = { margin: 0.08, min_samples: 3, confirm: 2 };
const schedule = { client: "codex", enabled: false, dispatch };
const view = {
  client: "codex",
  enabled: false,
  queue: { current: null, pin: null, rows: [], alerts: [] },
  lines: [],
  spent_today: [0, 0],
  settings: probe,
  actions: [],
};

function response(command: string) {
  if (command === "station_dispatch_view") return view;
  if (command === "station_router_status")
    return { current_route: null, pending_route: null, admission: [] };
  if (command === "station_router_settings")
    return {
      routing_mode: "selected",
      allowed_routes: [],
      admission: {
        overflow_to_pool: false,
        route_limits: {},
        max_wait_ms: 0,
        max_waiters: 0,
      },
    };
  if (command === "station_usage_requests") return { items: [] };
  if (command === "station_schedules") return [schedule];
  throw new Error(`Unexpected command: ${command}`);
}
beforeEach(() => {
  mock.call.mockReset();
  mock.call.mockImplementation(async (command) => response(command));
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});
function mount() {
  return render(
    <ToastProvider>
      <StationDispatch client="codex" />
    </ToastProvider>,
  );
}

it("serializes paid-probe edits and keeps the latest off choice while an earlier save finishes", async () => {
  let finishFirst: ((value: typeof probe) => void) | undefined;
  const writes: (typeof probe)[] = [];
  mock.call.mockImplementation(async (command, args) => {
    if (command === "station_probe_settings_save") {
      writes.push(args.settings);
      if (writes.length === 1)
        return new Promise((resolve) => {
          finishFirst = resolve;
        });
      return args.settings;
    }
    return response(command);
  });
  mount();
  const paid = await screen.findByRole("checkbox", { name: "开启花钱测活" });
  fireEvent.click(paid);
  await waitFor(() => expect(writes).toHaveLength(1));
  fireEvent.click(paid);
  expect(writes).toHaveLength(1);
  await act(async () => finishFirst?.({ ...probe, paid: true }));
  await waitFor(() => expect(writes).toHaveLength(2));
  expect(writes[1].paid).toBe(false);
  expect((paid as HTMLInputElement).checked).toBe(false);
});

it("serializes dispatch parameter edits and reports a failed latest save", async () => {
  let finishFirst: ((value: typeof schedule) => void) | undefined;
  const writes: (typeof dispatch)[] = [];
  mock.call.mockImplementation(async (command, args) => {
    if (command === "station_schedule_set") {
      writes.push(args.dispatch);
      if (writes.length === 1)
        return new Promise((resolve) => {
          finishFirst = resolve;
        });
      throw new Error("write failed");
    }
    return response(command);
  });
  mount();
  const margin = await screen.findByRole("spinbutton", {
    name: "领先现任至少多少（%）",
  });
  fireEvent.change(margin, { target: { value: "12" } });
  await waitFor(() => expect(writes).toHaveLength(1));
  fireEvent.change(margin, { target: { value: "20" } });
  expect(writes).toHaveLength(1);
  await act(async () =>
    finishFirst?.({ ...schedule, dispatch: { ...dispatch, margin: 0.12 } }),
  );
  await waitFor(() => expect(writes).toHaveLength(2));
  expect(writes[1].margin).toBe(0.2);
  expect(await screen.findByRole("alert")).toHaveProperty(
    "textContent",
    expect.stringContaining("防抖设置未保存"),
  );
});

it("does not expose default dispatch values as saved settings after a failed read", async () => {
  mock.call.mockImplementation(async (command) => {
    if (command === "station_schedules")
      throw new Error("database unavailable");
    return response(command);
  });
  mount();
  await screen.findByText(/防抖设置读取失败/);
  expect(
    (
      screen.getByRole("spinbutton", {
        name: "领先现任至少多少（%）",
      }) as HTMLInputElement
    ).disabled,
  ).toBe(true);
});

it("preserves an unsaved probe draft during refresh and allows retry after a write failure", async () => {
  let failFirst: ((reason: Error) => void) | undefined;
  let writes = 0;
  mock.call.mockImplementation(async (command, args) => {
    if (command === "station_probe_settings_save") {
      writes++;
      if (writes === 1)
        return new Promise((_, reject) => {
          failFirst = reject;
        });
      return args.settings;
    }
    return response(command);
  });
  mount();
  const paid = await screen.findByRole("checkbox", { name: "开启花钱测活" });
  fireEvent.click(paid);
  await waitFor(() => expect(failFirst).toBeDefined());
  fireEvent.click(screen.getByRole("button", { name: "刷新状态" }));
  await waitFor(() =>
    expect(
      mock.call.mock.calls.filter(([c]) => c === "station_dispatch_view"),
    ).toHaveLength(2),
  );
  expect((paid as HTMLInputElement).checked).toBe(true);
  await act(async () => failFirst?.(new Error("write failed")));
  await screen.findByText(/测活设置未保存：write failed/);
  fireEvent.click(screen.getByRole("button", { name: "刷新状态" }));
  await waitFor(() =>
    expect(
      mock.call.mock.calls.filter(([c]) => c === "station_dispatch_view"),
    ).toHaveLength(3),
  );
  expect((paid as HTMLInputElement).checked).toBe(true);
  expect(screen.getByText(/测活设置未保存/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "重试保存" }));
  await waitFor(() => expect(writes).toBe(2));
  await waitFor(() => expect(screen.queryByText(/测活设置未保存/)).toBeNull());
});

it("keeps global probe edits when the client tab changes during a save", async () => {
  let finish: ((value: typeof probe) => void) | undefined;
  let finishRead: ((value: typeof view) => void) | undefined;
  let reads = 0;
  mock.call.mockImplementation(async (command) => {
    if (command === "station_dispatch_view" && ++reads === 2)
      return new Promise((resolve) => {
        finishRead = resolve;
      });
    if (command === "station_probe_settings_save")
      return new Promise((resolve) => {
        finish = resolve;
      });
    if (command === "station_schedules")
      return [
        schedule,
        {
          ...schedule,
          client: "claude-code",
          dispatch: { ...dispatch, margin: 0.3 },
        },
      ];
    return response(command);
  });
  const mounted = mount();
  fireEvent.click(
    await screen.findByRole("checkbox", { name: "开启花钱测活" }),
  );
  await waitFor(() => expect(finish).toBeDefined());
  mounted.rerender(
    <ToastProvider>
      <StationDispatch client="claude-code" />
    </ToastProvider>,
  );
  await waitFor(() => expect(finishRead).toBeDefined());
  await act(async () => finish?.({ ...probe, paid: true }));
  await act(async () => finishRead?.(view));
  const paid = await screen.findByRole("checkbox", { name: "开启花钱测活" });
  expect((paid as HTMLInputElement).checked).toBe(true);
  expect(
    (
      screen.getByRole("spinbutton", {
        name: "领先现任至少多少（%）",
      }) as HTMLInputElement
    ).value,
  ).toBe("30");
});

it("waits for queued probe saves before reopening the editor", async () => {
  let finish: ((value: typeof probe) => void) | undefined;
  let stored = probe;
  let writes = 0;
  mock.call.mockImplementation(async (command, args) => {
    if (command === "station_dispatch_view")
      return { ...view, settings: stored };
    if (command === "station_probe_settings_save") {
      writes++;
      if (writes === 1) {
        const value = await new Promise<typeof probe>((resolve) => {
          finish = resolve;
        });
        stored = value;
      } else stored = args.settings;
      return stored;
    }
    return response(command);
  });
  const first = mount();
  fireEvent.click(
    await screen.findByRole("checkbox", { name: "开启花钱测活" }),
  );
  await waitFor(() => expect(finish).toBeDefined());
  first.unmount();
  mount();
  expect(screen.queryByRole("checkbox", { name: "开启花钱测活" })).toBeNull();
  await act(async () => finish?.({ ...probe, paid: true }));
  const paid = await screen.findByRole("checkbox", { name: "开启花钱测活" });
  expect((paid as HTMLInputElement).checked).toBe(true);
  fireEvent.click(paid);
  await waitFor(() => expect(writes).toBe(2));
  expect(stored.paid).toBe(false);
});
