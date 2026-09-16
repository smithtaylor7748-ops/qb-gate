// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useChecks } from "./useChecks";

const mock = vi.hoisted(() => ({
  state: new Map<string, unknown>(),
  refresh: vi.fn(async (_key: string) => {}),
  toast: { ok: vi.fn(), info: vi.fn(), error: vi.fn() },
}));
vi.mock("../../ui", () => ({ useToast: () => mock.toast }));
vi.mock("../../lib/resources", () => ({ R: {} }));
vi.mock("../../lib/progress", () => ({ markStep: vi.fn() }));
vi.mock("../../lib/store", () => ({
  getSession: (key: string, fallback: unknown) =>
    mock.state.get(key) ?? fallback,
  setSession: (key: string, value: unknown) => mock.state.set(key, value),
  useSession: (key: string, fallback: unknown) => [
    mock.state.get(key) ?? fallback,
  ],
  useResource: () => ({}),
  peek: () => undefined,
  refresh: (key: string) => mock.refresh(key),
}));
beforeEach(() => {
  mock.state.clear();
  mock.refresh.mockReset();
  mock.refresh.mockResolvedValue(undefined);
});
describe("complete five-item checkup", () => {
  it("runs egress and local details as part of the complete checkup", async () => {
    const { result } = renderHook(useChecks);
    await act(async () => {
      const report = await result.current.runAll();
      expect(report.completed).toHaveLength(5);
      expect(report.failed).toEqual({});
    });
    expect(mock.refresh.mock.calls.map(([id]) => id)).toEqual([
      "ip",
      "dns",
      "signals",
      "checkup",
      "gate",
      "egress",
    ]);
  });
  it("continues after a failed check and reports that failure", async () => {
    mock.refresh.mockImplementation(async (key) => {
      if (key === "dns") throw new Error("DNS endpoint unavailable");
    });
    const { result } = renderHook(useChecks);
    await act(async () => {
      const report = await result.current.runAll();
      expect(report.failed.dns).toContain("unavailable");
      expect(report.completed).toEqual([
        "purity",
        "signals",
        "iplock",
        "egress",
      ]);
    });
  });
  it("does not start another batch during an in-flight check", async () => {
    let release!: () => void;
    mock.refresh.mockImplementation(async (key) => {
      if (key === "ip")
        await new Promise<void>((r) => {
          release = r;
        });
    });
    const { result } = renderHook(useChecks);
    await act(async () => {
      const first = result.current.runAll();
      expect(await result.current.runAll()).toEqual({
        completed: [],
        failed: {},
      });
      release();
      await first;
    });
    expect(
      mock.refresh.mock.calls.filter(([id]) => id === "egress"),
    ).toHaveLength(1);
  });
});
