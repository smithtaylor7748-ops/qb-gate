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
import StationSchedule from "./StationSchedule";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
vi.mock("./StationDispatch", () => ({ default: () => <div>派单状态</div> }));

const prefs = {
  axes: ["stable", "fast"],
  floors: { min_success_rate: null, max_ttft_p95_ms: 3000n, max_rate: null },
};
const schedule = {
  client: "codex",
  enabled: true,
  prefs,
  dispatch: { margin: 0.08, min_samples: 3, confirm: 2 },
  last_switch_ms: null,
  last_note: "",
};
const decision = {
  basis: "unavailable",
  ranking: {
    winner: null as string | null,
    rows: [],
    indistinguishable: [],
    floors_relaxed: false,
    breakers_relaxed: false,
  },
};

beforeEach(() => {
  mock.call.mockReset();
  mock.call.mockImplementation(async (command, args) => {
    if (command === "station_schedules") return [schedule];
    if (command === "station_decide") return decision;
    if (command === "station_schedule_set")
      return { ...schedule, prefs: args.prefs };
  });
});
afterEach(cleanup);

it("ignores an older ranking that finishes after the latest preference preview", async () => {
  let finishOld: ((value: typeof decision) => void) | undefined;
  mock.call.mockImplementation(async (command, args) => {
    if (command === "station_schedules") return [schedule];
    if (command === "station_schedule_set")
      return { ...schedule, prefs: args.prefs };
    if (command === "station_decide") {
      if (args.prefs.axes.length === 1 && args.prefs.axes[0] === "cheap")
        return new Promise((resolve) => {
          finishOld = resolve;
        });
      return {
        ...decision,
        ranking: { ...decision.ranking, winner: "latest-route" },
      };
    }
  });
  render(<StationSchedule client="codex" />);
  await screen.findByText("派单状态");
  fireEvent.click(screen.getByRole("button", { name: "选择偏好" }));
  fireEvent.click(screen.getByRole("button", { name: "跑批量" }));
  await waitFor(() => expect(finishOld).toBeDefined());
  fireEvent.click(screen.getByRole("button", { name: "对话" }));
  await waitFor(() => expect(screen.getByText("latest-route")).toBeTruthy());
  await act(async () =>
    finishOld?.({
      ...decision,
      ranking: { ...decision.ranking, winner: "stale-route" },
    }),
  );
  expect(screen.queryByText("stale-route")).toBeNull();
  expect(screen.getByText("latest-route")).toBeTruthy();
});

it("never replaces stored preferences with defaults after a failed read", async () => {
  mock.call.mockImplementation(async (command) => {
    if (command === "station_schedules")
      throw new Error("database unavailable");
    return decision;
  });
  render(<StationSchedule client="codex" />);
  await screen.findByText(/调度设置读取失败：database unavailable/);
  expect(screen.queryByRole("checkbox")).toBeNull();
  expect(
    mock.call.mock.calls.some(
      ([command]) => command === "station_schedule_set",
    ),
  ).toBe(false);
});

it("opening the settings reads the saved policy without saving it again", async () => {
  render(<StationSchedule client="codex" />);
  await screen.findByText("派单状态");
  expect(
    mock.call.mock.calls.some(
      ([command]) => command === "station_schedule_set",
    ),
  ).toBe(false);
});

it("serializes consecutive preference edits so an old write cannot arrive last", async () => {
  let finishFirst: ((value: typeof schedule) => void) | undefined;
  const writes: unknown[] = [];
  mock.call.mockImplementation(async (command, args) => {
    if (command === "station_schedules") return [schedule];
    if (command === "station_decide") return decision;
    if (command === "station_schedule_set") {
      writes.push(args);
      if (writes.length === 1)
        return new Promise((resolve) => {
          finishFirst = resolve;
        });
      return { ...schedule, prefs: args.prefs };
    }
  });
  render(<StationSchedule client="codex" />);
  await screen.findByText("派单状态");
  fireEvent.click(screen.getByRole("button", { name: "选择偏好" }));
  fireEvent.click(screen.getByRole("button", { name: "跑批量" }));
  await waitFor(() => expect(writes).toHaveLength(1));
  fireEvent.click(screen.getByRole("button", { name: "对话" }));
  expect(writes).toHaveLength(1);
  finishFirst?.(schedule);
  await waitFor(() => expect(writes).toHaveLength(2));
  expect(writes[1]).toMatchObject({
    client: "codex",
    enabled: null,
    prefs: { axes: ["fast"] },
  });
});

it("shows failed saves instead of claiming that the new policy is active", async () => {
  mock.call.mockImplementation(async (command) => {
    if (command === "station_schedules") return [schedule];
    if (command === "station_schedule_set") throw new Error("write failed");
    return decision;
  });
  render(<StationSchedule client="codex" />);
  await screen.findByText("派单状态");
  fireEvent.click(screen.getByRole("button", { name: "选择偏好" }));
  fireEvent.click(screen.getByRole("button", { name: "跑批量" }));
  await screen.findByText("调度偏好未保存：write failed");
});

it("does not publish an old client's save or read preferences before its pending writes finish", async () => {
  let finish: ((value: typeof schedule) => void) | undefined;
  const saved = vi.fn();
  mock.call.mockImplementation(async (command) => {
    if (command === "station_schedules")
      return [schedule, { ...schedule, client: "claude-code" }];
    if (command === "station_decide") return decision;
    if (command === "station_schedule_set")
      return new Promise((resolve) => {
        finish = resolve;
      });
  });
  const mounted = render(<StationSchedule client="codex" onSaved={saved} />);
  await screen.findByText("派单状态");
  fireEvent.click(screen.getByRole("button", { name: "选择偏好" }));
  fireEvent.click(screen.getByRole("button", { name: "跑批量" }));
  await waitFor(() => expect(finish).toBeDefined());
  mounted.rerender(<StationSchedule client="claude-code" onSaved={saved} />);
  expect(screen.getByText("读取调度设置…")).toBeTruthy();
  await act(async () => finish?.(schedule));
  await screen.findByText("选择偏好");
  expect(saved).not.toHaveBeenCalled();
  expect(
    mock.call.mock.calls.filter(([c]) => c === "station_schedule_set"),
  ).toHaveLength(1);
});
