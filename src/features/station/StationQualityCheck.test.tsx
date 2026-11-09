// @vitest-environment jsdom
import {
  cleanup,
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import StationQualityCheck from "./StationQualityCheck";
const status = vi.hoisted(() =>
  vi.fn(async () => ({
    turnstate_enabled: false,
    turnstate_takeover: null,
    turnstate_official_armed: false,
  })),
);
vi.mock("../../lib/station", () => ({ stationApi: { routerStatus: status } }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
const quality = vi.hoisted(() => ({
  run: vi.fn(),
  cancel: vi.fn(async () => true),
  load: vi.fn(async () => ({})),
}));
vi.mock("../../lib/qualityApi", () => ({ qualityApi: quality }));
vi.mock("../../lib/qualityFingerprint", () => ({
  getFingerprintCatalog: () => ({
    sdkVersion: "test",
    dataVersion: "bank-1",
    referenceSha256: "sha",
    models: [{ id: "model-a", label: "model-a", merged: false }],
  }),
  loadFingerprintReference: quality.load,
  createFingerprintPrompts: () =>
    [1, 2, 3].map((n) => ({
      id: String(n),
      prompt: `sample ${n}`,
      expected_count: 300,
    })),
  scoreFingerprint: async () => ({
    verdict: "same_reference",
    summary: "库内相对参考结果",
    analysis: {
      prediction: "model-a",
      prediction_name: "model-a",
      probability: 0.7,
    },
  }),
}));
function response(request: {
  request_id: string;
  target: { id: string; kind: string };
  model: string;
}) {
  return {
    request_id: request.request_id,
    target_id: request.target.id,
    target_kind: request.target.kind,
    target_label: "relay-a",
    source_stamp: "process-salted-source-a",
    requested_model: request.model,
    actual_model: "declared-alias",
    protocol: "responses",
    status: "completed",
    text: '{"answer":21}',
    input_tokens: 50,
    output_tokens: 6,
    cache_read_tokens: null,
    reasoning_tokens: null,
    elapsed_ms: 1200,
    first_token_ms: null,
    error: null,
    output_limit_enforced: true,
  };
}
beforeEach(() => {
  quality.run.mockReset();
  quality.cancel.mockClear();
  quality.load.mockReset();
  quality.load.mockResolvedValue({});
  quality.run.mockImplementation(async (request) => response(request));
});
afterEach(() => {
  cleanup();
  localStorage.clear();
});
it("compares only matching conditions and stores scores without response content", async () => {
  render(<StationQualityCheck route="relay-a" />);
  await waitFor(() => expect(status).toHaveBeenCalled());
  fireEvent.click(screen.getByText("高级：原五题手动导入与历史记录"));
  fireEvent.change(screen.getByLabelText("测试模型"), {
    target: { value: "chosen-model" },
  });
  fireEvent.change(screen.getByLabelText("测试结果"), {
    target: {
      value: JSON.stringify({
        q1: 25,
        q2: "B",
        q3: 64,
        q4: -10,
        q5: 4,
        privateNote: "DO_NOT_SAVE_BODY",
      }),
    },
  });
  fireEvent.click(screen.getByRole("button", { name: "检查并记录" }));
  expect(screen.getByText(/相同条件已记录 1 次/)).toBeTruthy();
  expect(localStorage.getItem("qb-quality-check-v1")).not.toContain(
    "DO_NOT_SAVE_BODY",
  );
  fireEvent.change(screen.getByLabelText("测试模型"), {
    target: { value: "another-model" },
  });
  expect(screen.getByText(/相同条件已记录 0 次/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "清除测试记录" }));
  expect(localStorage.getItem("qb-quality-check-v1")).toBeNull();
});

it("shows actionable backend errors without persisting their text", async () => {
  quality.run.mockImplementation(async (request) => ({
    ...response(request),
    status: "failed",
    text: "",
    error: "当前 Codex CLI 不支持 --ignore-user-config，请先升级官方 CLI",
  }));
  render(<StationQualityCheck route="official" accountId="account-a" />);
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（1 次请求）" }),
  );
  await waitFor(() =>
    expect(screen.getByText(/当前 Codex CLI 不支持/)).toBeTruthy(),
  );
  expect(localStorage.getItem("qb-quality-check-v2")).not.toContain(
    "请先升级官方 CLI",
  );
});

it("starts only after explicit action, binds the target, and preserves legacy history", async () => {
  localStorage.setItem("qb-quality-check-v1", "[]");
  render(<StationQualityCheck route="relay-a" routeId="route-a" />);
  expect(quality.run).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（1 次请求）" }),
  );
  await waitFor(() => expect(screen.getByText("批次完成")).toBeTruthy());
  expect(quality.run).toHaveBeenCalledTimes(1);
  expect(quality.run.mock.calls[0][0]).toMatchObject({
    target: { kind: "relay", id: "route-a" },
    model: "model-a",
    effort: "high",
    timeout_secs: 180,
  });
  const saved = JSON.parse(localStorage.getItem("qb-quality-check-v2")!)[0];
  expect(saved.results[0]).toMatchObject({ verdict: "passed", answer: 21 });
  expect(saved.samples[0]).toMatchObject({
    reasoningTokens: null,
    declaredModel: "declared-alias",
  });
  expect(saved).not.toHaveProperty("text");
  expect(localStorage.getItem("qb-quality-check-v2")).not.toContain('"text"');
  expect(localStorage.getItem("qb-quality-check-v1")).toBe("[]");
  expect(screen.getByText(/同条件已记录 1 批/)).toBeTruthy();
});

it("requires an actual target ID instead of trusting a display label", () => {
  render(<StationQualityCheck route="paid-looking-label" />);
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  const button = screen.getByRole("button", {
    name: "开始自动检测（1 次请求）",
  }) as HTMLButtonElement;
  expect(button.disabled).toBe(true);
  expect(quality.run).not.toHaveBeenCalled();
});

it("never borrows a historical source stamp and separates the same route ID after its source changes", async () => {
  const first = render(
    <StationQualityCheck route="relay-a" routeId="route-a" />,
  );
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（1 次请求）" }),
  );
  await waitFor(() =>
    expect(screen.getByText(/同条件已记录 1 批/)).toBeTruthy(),
  );
  first.unmount();
  render(<StationQualityCheck route="relay-a" routeId="route-a" />);
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  expect(screen.getByText(/同条件已记录 0 批/)).toBeTruthy();
  expect(screen.getByText(/来源尚未核验，运行一批检测后/)).toBeTruthy();
  quality.run.mockImplementation(async (request) => ({
    ...response(request),
    source_stamp: "process-salted-source-b",
  }));
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（1 次请求）" }),
  );
  await waitFor(() =>
    expect(
      JSON.parse(localStorage.getItem("qb-quality-check-v2")!).length,
    ).toBe(2),
  );
  expect(screen.getByText(/同条件已记录 1 批/)).toBeTruthy();
  expect(screen.getByText(/不同来源或条件/)).toBeTruthy();
  expect(
    JSON.parse(localStorage.getItem("qb-quality-check-v2")!).map(
      (report: { conditions: { sourceStamp: string } }) =>
        report.conditions.sourceStamp,
    ),
  ).toEqual(["process-salted-source-b", "process-salted-source-a"]);
});

it("rejects a source change within the same batch", async () => {
  quality.run.mockImplementationOnce(async (request) => response(request));
  quality.run.mockImplementationOnce(async (request) => ({
    ...response(request),
    source_stamp: "changed-source",
  }));
  render(<StationQualityCheck route="relay-a" routeId="route-a" />);
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.change(screen.getByLabelText("检测轮数"), {
    target: { value: "3" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（3 次请求）" }),
  );
  await waitFor(() =>
    expect(localStorage.getItem("qb-quality-check-v2")).not.toBeNull(),
  );
  expect(quality.run).toHaveBeenCalledTimes(2);
  const saved = JSON.parse(localStorage.getItem("qb-quality-check-v2")!)[0];
  expect(saved.state).toBe("error");
  expect(saved.conditions.sourceStamp).toBe("process-salted-source-a");
  expect(saved.results[1].verdict).toBe("error");
});

it("retains the friendly account label when the backend only returns the opaque ID", async () => {
  quality.run.mockImplementation(async (request) => ({
    ...response(request),
    target_label: request.target.id,
  }));
  render(
    <StationQualityCheck route="工作账户" accountId="opaque-account-id" />,
  );
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（1 次请求）" }),
  );
  await waitFor(() =>
    expect(localStorage.getItem("qb-quality-check-v2")).not.toBeNull(),
  );
  expect(
    JSON.parse(localStorage.getItem("qb-quality-check-v2")!)[0].conditions
      .targetLabel,
  ).toBe("工作账户");
});

it("cancels the active request and rejects a late completed answer without starting the next round", async () => {
  let resolve!: (value: ReturnType<typeof response>) => void;
  quality.run.mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  render(<StationQualityCheck route="relay-a" routeId="route-a" />);
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.change(screen.getByLabelText("检测轮数"), {
    target: { value: "3" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（3 次请求）" }),
  );
  await waitFor(() => expect(quality.run).toHaveBeenCalledTimes(1));
  const request = quality.run.mock.calls[0][0];
  fireEvent.click(screen.getByRole("button", { name: "取消本批次" }));
  expect(quality.cancel).toHaveBeenCalledWith(request.request_id);
  await act(async () => resolve(response(request)));
  const saved = JSON.parse(localStorage.getItem("qb-quality-check-v2")!)[0];
  expect(saved.state).toBe("cancelled");
  expect(saved.results[0].verdict).toBe("cancelled");
  expect(saved.samples[0].status).toBe("cancelled");
  expect(quality.run).toHaveBeenCalledTimes(1);
});

it("cancels on target change and attributes the partial report to its original target", async () => {
  let resolve!: (value: ReturnType<typeof response>) => void;
  quality.run.mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  const view = render(
    <StationQualityCheck route="relay-a" routeId="route-a" />,
  );
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（1 次请求）" }),
  );
  const request = quality.run.mock.calls[0][0];
  view.rerender(<StationQualityCheck route="relay-b" routeId="route-b" />);
  expect(quality.cancel).toHaveBeenCalledWith(request.request_id);
  await act(async () => resolve(response(request)));
  expect(
    JSON.parse(localStorage.getItem("qb-quality-check-v2")!)[0].conditions
      .targetId,
  ).toBe("relay:route-a");
  expect(screen.getByText(/同条件已记录 0 批/)).toBeTruthy();
});

it("stops the batch when the backend cancels before capturing the next source", async () => {
  quality.run.mockImplementationOnce(async (request) => response(request));
  quality.run.mockImplementationOnce(async (request) => ({
    ...response(request),
    status: "cancelled",
    source_stamp: null,
    text: "",
  }));
  render(<StationQualityCheck route="relay-a" routeId="route-a" />);
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.change(screen.getByLabelText("检测轮数"), {
    target: { value: "3" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（3 次请求）" }),
  );
  await waitFor(() => expect(screen.getByText("批次已取消")).toBeTruthy());
  expect(quality.run).toHaveBeenCalledTimes(2);
  const saved = JSON.parse(localStorage.getItem("qb-quality-check-v2")!)[0];
  expect(saved.state).toBe("cancelled");
  expect(saved.results[1].verdict).toBe("cancelled");
  expect(saved.conditions.sourceStamp).toBe("process-salted-source-a");
});

it("cancels on unmount and does not continue queued samples", async () => {
  let resolve!: (value: ReturnType<typeof response>) => void;
  quality.run.mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  const view = render(
    <StationQualityCheck route="relay-a" routeId="route-a" />,
  );
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.change(screen.getByLabelText("检测轮数"), {
    target: { value: "2" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（2 次请求）" }),
  );
  const request = quality.run.mock.calls[0][0];
  view.unmount();
  expect(quality.cancel).toHaveBeenCalledWith(request.request_id);
  await act(async () => resolve(response(request)));
  expect(quality.run).toHaveBeenCalledTimes(1);
  expect(
    JSON.parse(localStorage.getItem("qb-quality-check-v2")!)[0].state,
  ).toBe("cancelled");
});

it("keeps upstream failure separate from an incorrect answer and does not persist error text", async () => {
  quality.run.mockImplementation(async (request) => ({
    ...response(request),
    status: "failed",
    error: "PRIVATE_ERROR_BODY",
  }));
  render(<StationQualityCheck route="relay-a" routeId="route-a" />);
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（1 次请求）" }),
  );
  await waitFor(() =>
    expect(localStorage.getItem("qb-quality-check-v2")).not.toBeNull(),
  );
  const raw = localStorage.getItem("qb-quality-check-v2")!;
  expect(JSON.parse(raw)[0].results[0].verdict).toBe("error");
  expect(raw).not.toContain("PRIVATE_ERROR_BODY");
});

it("uses three sequential fingerprint samples and reports a reference result without a pass verdict", async () => {
  render(<StationQualityCheck route="relay-a" routeId="route-a" />);
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.click(screen.getByLabelText(/快速糖果题/));
  fireEvent.click(screen.getByLabelText(/模型指纹（实验性）/));
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（3 次请求）" }),
  );
  await waitFor(() =>
    expect(localStorage.getItem("qb-quality-check-v2")).not.toBeNull(),
  );
  expect(quality.run).toHaveBeenCalledTimes(3);
  const saved = JSON.parse(localStorage.getItem("qb-quality-check-v2")!)[0];
  expect(
    new Set(quality.run.mock.calls.map(([request]) => request.run_id)).size,
  ).toBe(1);
  expect(
    new Set(quality.run.mock.calls.map(([request]) => request.request_id)).size,
  ).toBe(3);
  expect(saved.results[0]).toMatchObject({
    method: "fingerprint",
    verdict: "reference",
    probability: 0.7,
  });
});

it("validates the fingerprint asset before sending any paid request", async () => {
  quality.load.mockRejectedValue(new Error("bad asset"));
  render(<StationQualityCheck route="relay-a" routeId="route-a" />);
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.click(screen.getByLabelText(/模型指纹（实验性）/));
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（4 次请求）" }),
  );
  await waitFor(() =>
    expect(localStorage.getItem("qb-quality-check-v2")).not.toBeNull(),
  );
  expect(quality.run).not.toHaveBeenCalled();
  expect(screen.getByText(/内置指纹参考数据校验失败/)).toBeTruthy();
});

it("rejects a completed response belonging to another target and stops the batch", async () => {
  quality.run.mockImplementation(async (request) => ({
    ...response(request),
    target_id: "unexpected-target",
  }));
  render(<StationQualityCheck route="relay-a" routeId="route-a" />);
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.change(screen.getByLabelText("检测轮数"), {
    target: { value: "2" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（2 次请求）" }),
  );
  await waitFor(() =>
    expect(localStorage.getItem("qb-quality-check-v2")).not.toBeNull(),
  );
  const saved = JSON.parse(localStorage.getItem("qb-quality-check-v2")!)[0];
  expect(saved.state).toBe("error");
  expect(saved.results[0].verdict).toBe("error");
  expect(quality.run).toHaveBeenCalledTimes(1);
});

it("requires manual animation review and renders it without script or same-origin sandbox grants", async () => {
  quality.run.mockImplementation(async (request) => ({
    ...response(request),
    text: '<!doctype html><html><head><style>@keyframes wave {to {opacity:0.5}}</style></head><body><svg><circle r="4" /></svg></body></html>',
  }));
  render(<StationQualityCheck route="relay-a" routeId="route-a" />);
  fireEvent.change(screen.getByLabelText("自动检测模型"), {
    target: { value: "model-a" },
  });
  fireEvent.click(screen.getByLabelText(/快速糖果题/));
  fireEvent.click(screen.getByText("进阶：CSS / SVG 动画与人工复核"));
  fireEvent.click(screen.getByLabelText(/灯塔与小船动画/));
  fireEvent.click(
    screen.getByRole("button", { name: "开始自动检测（1 次请求）" }),
  );
  await waitFor(() => expect(screen.getByTitle("动画隔离预览")).toBeTruthy());
  expect(
    JSON.parse(localStorage.getItem("qb-quality-check-v2")!)[0].results[0]
      .verdict,
  ).toBe("needs_review");
  fireEvent.click(screen.getByText("第 1 轮动画预览与人工复核"));
  const frame = screen.getByTitle("动画隔离预览");
  expect(frame.getAttribute("sandbox")).toBe("");
  expect(frame.getAttribute("srcdoc")).toContain("default-src 'none'");
  const approve = screen.getByRole("button", {
    name: "记录人工复核通过",
  }) as HTMLButtonElement;
  expect(approve.disabled).toBe(true);
  for (const label of [
    /能辨认灯塔/,
    /光束、海浪/,
    /观察至少两个循环/,
    /在宽屏和窄屏查看/,
  ])
    fireEvent.click(screen.getByLabelText(label));
  expect(approve.disabled).toBe(false);
  fireEvent.click(approve);
  expect(
    JSON.parse(localStorage.getItem("qb-quality-check-v2")!)[0].results[0]
      .verdict,
  ).toBe("passed");
  expect(localStorage.getItem("qb-quality-check-v2")).not.toContain(
    "<!doctype",
  );
});
