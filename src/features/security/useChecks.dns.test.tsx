// @vitest-environment jsdom
import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { DnsReport } from "../../lib/api";
import { useChecks } from "./useChecks";

const mock = vi.hoisted(() => ({
  state: new Map<string, unknown>(),
  report: undefined as DnsReport | undefined,
  mark: vi.fn(),
  toast: { ok: vi.fn(), info: vi.fn(), error: vi.fn() },
}));
vi.mock("../../ui", () => ({ useToast: () => mock.toast }));
vi.mock("../../lib/api", () => ({ api: { gateCheck: vi.fn() } }));
vi.mock("../../lib/resources", () => ({ R: {} }));
vi.mock("../../lib/progress", () => ({
  markStep: (...args: unknown[]) => mock.mark(...args),
}));
vi.mock("../../lib/store", () => ({
  getSession: (key: string, fallback: unknown) =>
    mock.state.get(key) ?? fallback,
  setSession: (key: string, value: unknown) => mock.state.set(key, value),
  useSession: (key: string, fallback: unknown) => [
    mock.state.get(key) ?? fallback,
  ],
  useResource: () => ({}),
  refresh: vi.fn(async () => {}),
  peek: (key: string) => (key === "dns" ? mock.report : undefined),
}));

beforeEach(() => {
  vi.clearAllMocks();
  mock.state.clear();
  mock.report = {
    egress_asn: null,
    resolvers: [],
    passed: false,
    findings: ["部分 DNS 路由无法确认"],
    upstream_conclusion: null,
    note: "只代表已采集的路径",
    score: null,
    adapters_safe: null,
    adapters_note: "查不出路由",
  };
});

it("keeps unknown DNS evidence pending without a success or leak claim", async () => {
  const { result } = renderHook(useChecks);
  await act(() => result.current.dns.run());
  expect(mock.mark).toHaveBeenCalledWith(
    "dns",
    "pending",
    "medium",
    expect.stringContaining("尚不能判定"),
  );
  expect(mock.toast.ok).not.toHaveBeenCalled();
  expect(mock.toast.error).not.toHaveBeenCalled();
  expect(mock.toast.info).toHaveBeenCalledWith(
    expect.stringContaining("尚不能判定"),
  );
});

it("still fails a known bypass even when another route is unknown", async () => {
  mock.report!.adapters_safe = false;
  const { result } = renderHook(useChecks);
  await act(() => result.current.dns.run());
  expect(mock.mark).toHaveBeenCalledWith(
    "dns",
    "failed",
    "medium",
    expect.any(String),
  );
  expect(mock.toast.error).toHaveBeenCalled();
  expect(mock.toast.ok).not.toHaveBeenCalled();
});
