// @vitest-environment jsdom
// 「选线与分流」弹窗（2026-11-15 第三版样机）：第一屏备用线路 + 自动选线偏好 + 下一次请求，
// 其余在「高级设置」里；整个弹窗只有一个「保存」，旧草稿不能盖掉新状态。
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type { PlanSkip } from "../../lib/generated/PlanSkip";
import type { PlanStep } from "../../lib/generated/PlanStep";
import type { Route, RouterSettings, RouterStatus } from "../../lib/station";
import StationRouterSettings from "./StationRouterSettings";

const mock = vi.hoisted(() => ({ call: vi.fn(), ok: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
vi.mock("../../ui", async (original) => ({
  ...(await original<typeof import("../../ui")>()),
  useToast: () => ({ ok: mock.ok, error: vi.fn(), info: vi.fn() }),
}));
// 「检测与排名」那一块点开才挂，这里不测它。
vi.mock("./StationDispatch", () => ({ default: () => <div>检测表</div> }));

const settings: RouterSettings = {
  client: "codex",
  routing_mode: "selected",
  allowed_routes: [],
  pool_strategy: "priority",
  session_recovery: { strip_item_ids: false, filter_encrypted_content: false },
  admission: {
    overflow_to_pool: false,
    route_limits: {},
    max_wait_ms: 0,
    max_waiters: 0,
  },
  max_attempts: 2,
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
const schedule = (client = "codex") => ({
  client,
  enabled: false,
  prefs: {
    axes: ["cheap"],
    floors: { min_success_rate: 0.95, max_ttft_p95_ms: null, max_rate: null },
  },
  dispatch: { margin: 0.08, min_samples: 3, confirm: 2 },
  last_switch_ms: null,
  last_note: "",
});
const routes = ["main", "cheap", "pricey"].map<Route>((id) => ({
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
const step = (
  route_id: string,
  role: PlanStep["role"],
  skip: PlanSkip | null = null,
): PlanStep => ({
  route_id,
  role,
  skip,
  until_ms: null,
  missing_models: [],
});
const statusWith = (
  selection: Partial<RouterStatus["selection"]> = {},
): RouterStatus =>
  ({
    current_route: "main",
    pending_route: null,
    breakers: [],
    selection: {
      current: "main",
      backups: [],
      auto_select: false,
      plan: {
        current: "main",
        steps: [step("main", "current")],
        first: "main",
        max_attempts: 2,
        strict_single: true,
      },
      paused: null,
      aggregate: null,
      pricier_backups: [],
      problem: null,
      ...selection,
    },
  }) as unknown as RouterStatus;
const writes = () =>
  mock.call.mock.calls.filter(
    ([command]) => command === "station_router_settings_save",
  );
const scheduleWrites = () =>
  mock.call.mock.calls.filter(
    ([command]) => command === "station_schedule_set",
  );
const props = {
  client: "codex" as const,
  routes,
  status: statusWith(),
  label: (route: Route) => route.group,
};

beforeEach(() => {
  mock.call.mockReset();
  mock.ok.mockReset();
  mock.call.mockImplementation(async (command, args) => {
    if (command === "station_router_settings")
      return { ...structuredClone(settings), client: args.client };
    if (command === "station_schedules")
      return [schedule("codex"), schedule("claude-code")];
    if (command === "station_aggregate_models") return [];
    if (command === "station_router_settings_save") return args.settings;
    if (command === "station_schedule_set")
      return {
        ...schedule(args.client),
        prefs: args.prefs,
        dispatch: args.dispatch,
      };
    return null;
  });
});
afterEach(cleanup);

it("opens without saving, approves nothing on its own and has no single-line radio", async () => {
  render(<StationRouterSettings {...props} />);
  await screen.findByText("备用线路");
  expect(writes()).toHaveLength(0);
  expect(scheduleWrites()).toHaveLength(0);
  // 「仅所选线路 / 允许所勾选线路」单选框删了：名单空就是只用当前线路。
  expect(screen.queryByRole("radio", { name: /仅所选线路/ })).toBeNull();
  expect(screen.queryByRole("radio", { name: /允许所勾选线路/ })).toBeNull();
  // 当前线路不用勾；另外两条都没勾 —— 旧数据不构成授权。
  const checks = screen.getAllByRole("checkbox", {
    name: /加为备用线路/,
  }) as HTMLInputElement[];
  expect(checks.map((c) => c.checked)).toEqual([false, false]);
  expect(screen.getByText("当前线路 · 不用勾，永远可用")).toBeTruthy();
  expect(
    screen.getByText("只用「main」，失败直接报错（没有备用线路）"),
  ).toBeTruthy();
});

it("saves backups without requiring the current line in the list, in one save", async () => {
  const onSaved = vi.fn();
  render(<StationRouterSettings {...props} onSaved={onSaved} />);
  fireEvent.click(
    await screen.findByRole("checkbox", { name: "把「pricey」加为备用线路" }),
  );
  fireEvent.click(
    screen.getByRole("checkbox", { name: "把「cheap」加为备用线路" }),
  );
  // 往前挪：cheap 排到 pricey 前面。
  fireEvent.click(screen.getByRole("button", { name: "把「cheap」往前挪" }));
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() => expect(onSaved).toHaveBeenCalledOnce());
  expect(writes()).toHaveLength(1);
  expect(writes()[0][1].settings).toMatchObject({
    routing_mode: "pool",
    allowed_routes: ["cheap", "pricey"],
  });
  // 偏好没改就不写偏好。
  expect(scheduleWrites()).toHaveLength(0);
});

it("clearing every backup saves strict single-line mode", async () => {
  const original = mock.call.getMockImplementation()!;
  mock.call.mockImplementation((command, args) =>
    command === "station_router_settings"
      ? Promise.resolve({
          ...settings,
          routing_mode: "pool",
          allowed_routes: ["cheap"],
        })
      : original(command, args),
  );
  render(<StationRouterSettings {...props} />);
  fireEvent.click(
    await screen.findByRole("checkbox", { name: "把「cheap」加为备用线路" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() => expect(writes()).toHaveLength(1));
  expect(writes()[0][1].settings).toMatchObject({
    routing_mode: "selected",
    allowed_routes: [],
  });
});

it("a preference preset is a draft until the one save, then goes out with the backups", async () => {
  render(<StationRouterSettings {...props} />);
  fireEvent.click(await screen.findByRole("radio", { name: /速度/ }));
  expect(scheduleWrites()).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() => expect(scheduleWrites()).toHaveLength(1));
  expect(scheduleWrites()[0][1]).toMatchObject({
    client: "codex",
    enabled: null,
    prefs: { axes: ["fast"] },
  });
});

it("refuses to overwrite settings that changed elsewhere after the dialog opened", async () => {
  const original = mock.call.getMockImplementation()!;
  let reads = 0;
  mock.call.mockImplementation((command, args) => {
    if (command === "station_router_settings") {
      reads += 1;
      // 第二次读（保存前的核对）时库里已经是别人存的那份。
      return Promise.resolve(
        reads === 1
          ? structuredClone(settings)
          : { ...settings, routing_mode: "pool", allowed_routes: ["pricey"] },
      );
    }
    return original(command, args);
  });
  render(<StationRouterSettings {...props} />);
  fireEvent.click(
    await screen.findByRole("checkbox", { name: "把「cheap」加为备用线路" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await screen.findByText(/被别处改过/);
  // 拒了之后重读最新的：读完那句话还在（原来放在 error 里，重读一开头就被清掉了）。
  await waitFor(() => expect(reads).toBe(3));
  await screen.findByText("备用线路");
  expect(screen.getByText(/被别处改过/)).toBeTruthy();
  expect(writes()).toHaveLength(0);
  expect(mock.ok).not.toHaveBeenCalled();
});

it("keeps failed saves out of the effective state and locks edits while saving", async () => {
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
  const box = await screen.findByRole("checkbox", {
    name: "把「cheap」加为备用线路",
  });
  fireEvent.click(box);
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() => expect(writes()).toHaveLength(1));
  expect(box.closest("fieldset")?.disabled).toBe(true);
  rejectSave(new Error("transaction failed"));
  await screen.findByText("transaction failed");
  expect(onSaved).not.toHaveBeenCalled();
  expect(mock.ok).not.toHaveBeenCalled();
  expect(box.closest("fieldset")?.disabled).toBe(false);
});

it("shows why a backup is skipped, offers retry only for breakers, and marks pricier backups", async () => {
  const original = mock.call.getMockImplementation()!;
  mock.call.mockImplementation((command, args) =>
    command === "station_router_settings"
      ? Promise.resolve({
          ...settings,
          routing_mode: "pool",
          allowed_routes: ["cheap", "pricey"],
        })
      : original(command, args),
  );
  render(
    <StationRouterSettings
      {...props}
      status={statusWith({
        backups: ["cheap", "pricey"],
        pricier_backups: ["pricey"],
        plan: {
          current: "main",
          steps: [
            step("main", "current"),
            step("cheap", "backup", "rate-limited"),
            step("pricey", "backup", "breaker-rejected"),
          ],
          first: "main",
          max_attempts: 2,
          strict_single: false,
        },
      })}
    />,
  );
  await screen.findByText("第 1 备用");
  expect(screen.getByText("比当前线路贵")).toBeTruthy();
  expect(
    screen.getByText(/限流中，跳过/, { selector: ".qb-st-pill" }),
  ).toBeTruthy();
  expect(
    screen.getByText(/熔断中，跳过/, { selector: ".qb-st-pill" }),
  ).toBeTruthy();
  // 只有熔断那一条给「重试这条」。
  expect(screen.getAllByRole("button", { name: "重试这条" })).toHaveLength(1);
  expect(
    screen.getByText(
      "main → cheap（限流中，跳过） → pricey（熔断中，跳过） · 一次最多试 2 条",
    ),
  ).toBeTruthy();
});

it("turning capacity split off resets how it splits, and limits save with the backups", async () => {
  const original = mock.call.getMockImplementation()!;
  mock.call.mockImplementation((command, args) =>
    command === "station_router_settings"
      ? Promise.resolve({
          ...settings,
          routing_mode: "pool",
          allowed_routes: ["cheap"],
          pool_strategy: "least-loaded",
          admission: { ...settings.admission, overflow_to_pool: true },
        })
      : original(command, args),
  );
  render(<StationRouterSettings {...props} />);
  const load = (await screen.findByRole("radio", {
    name: /优先空闲线路/,
  })) as HTMLInputElement;
  expect(load.checked).toBe(true);
  fireEvent.change(screen.getByLabelText("cheap · 同时进行的请求上限"), {
    target: { value: "2" },
  });
  fireEvent.click(
    screen.getByRole("checkbox", { name: "本机限额满了时分流到备用线路" }),
  );
  expect(load.checked).toBe(false);
  expect(load.disabled).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() => expect(mock.ok).toHaveBeenCalledOnce());
  expect(writes()[0][1].settings).toMatchObject({
    pool_strategy: "priority",
    admission: {
      overflow_to_pool: false,
      route_limits: { cheap: { max_concurrent: 2 } },
    },
  });
});

it("ignores the old client's save response after changing client", async () => {
  const original = mock.call.getMockImplementation()!;
  let finish: ((value: RouterSettings) => void) | undefined;
  mock.call.mockImplementation((command, args) => {
    if (command === "station_router_settings")
      return Promise.resolve({
        ...settings,
        client: args.client,
        admission: {
          ...settings.admission,
          max_wait_ms: args.client === "claude-code" ? 1234 : 0,
        },
      });
    if (command === "station_router_settings_save")
      return new Promise((resolve) => {
        finish = resolve;
      });
    return original(command, args);
  });
  const onSaved = vi.fn();
  const mounted = render(
    <StationRouterSettings {...props} onSaved={onSaved} />,
  );
  fireEvent.click(
    await screen.findByRole("checkbox", { name: "把「cheap」加为备用线路" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() => expect(finish).toBeDefined());
  mounted.rerender(
    <StationRouterSettings {...props} client="claude-code" onSaved={onSaved} />,
  );
  expect(screen.getByText("读取中…")).toBeTruthy();
  await act(async () => finish?.(settings));
  const wait = (await screen.findByLabelText(
    "最多等待（毫秒）",
  )) as HTMLInputElement;
  expect(wait.value).toBe("1234");
  expect(mock.ok).not.toHaveBeenCalled();
  expect(onSaved).not.toHaveBeenCalled();
});

it.each(["-1", "1.5"])(
  "refuses invalid admission input %s without sending a save",
  async (value) => {
    render(<StationRouterSettings {...props} />);
    fireEvent.change(await screen.findByLabelText("main · 每分钟请求上限"), {
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
