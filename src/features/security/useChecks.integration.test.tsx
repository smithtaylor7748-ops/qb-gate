// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import {
  invalidateAutomatic,
  peek,
  queryClient,
  setSession,
} from "../../lib/store";
import { useChecks } from "./useChecks";

const mock = vi.hoisted(() => ({
  ip: vi.fn(),
  dns: vi.fn(),
  signals: vi.fn(),
  toast: { ok: vi.fn(), info: vi.fn(), error: vi.fn() },
}));
vi.mock("../../ui", () => ({ useToast: () => mock.toast }));
vi.mock("../../lib/progress", () => ({ markStep: vi.fn() }));
vi.mock("../../lib/resources", async () => {
  const { res } = await import("../../lib/store");
  const manual = { auto: false, persist: true };
  return {
    R: {
      ip: res(() => mock.ip(), { auto: false }),
      dns: res(() => mock.dns(), manual),
      signals: res(() => mock.signals(), manual),
      checkup: res(async () => ({ checks: [] }), manual),
      gate: res(async () => ({
        allowlist: ["203.0.113.7"],
        ip_allowed: true,
        stale_copies: [],
      })),
      egress: res(async () => ({ items: [] }), manual),
    },
  };
});
beforeEach(() => {
  vi.clearAllMocks();
  mock.ip.mockResolvedValue({
    ip: "203.0.113.7",
    fraudScore: 3,
    isResidential: true,
  });
  setSession("security.failed", {});
});
afterEach(() => {
  cleanup();
  queryClient.clear();
  localStorage.clear();
});

it("finishes all five checks when workspace notifications arrive during DNS and environment scans", async () => {
  let finishDns!: (value: unknown) => void;
  let finishSignals!: (value: unknown) => void;
  mock.dns.mockImplementation(
    () =>
      new Promise((resolve) => {
        finishDns = resolve;
      }),
  );
  mock.signals.mockImplementation(
    () =>
      new Promise((resolve) => {
        finishSignals = resolve;
      }),
  );
  const { result } = renderHook(useChecks, {
    wrapper: ({ children }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    ),
  });
  let checking!: ReturnType<typeof result.current.runAll>;
  await act(async () => {
    checking = result.current.runAll();
  });
  await waitFor(() => expect(mock.dns).toHaveBeenCalledOnce());
  await act(async () => {
    await invalidateAutomatic();
    finishDns({ passed: true, findings: [] });
  });
  await waitFor(() => expect(mock.signals).toHaveBeenCalledOnce());
  await act(async () => {
    await invalidateAutomatic();
    finishSignals({ total: 0, hits: [] });
    expect(await checking).toEqual({
      completed: ["purity", "dns", "signals", "iplock", "egress"],
      failed: {},
    });
  });
  expect(mock.toast.error).not.toHaveBeenCalled();
  expect(peek("dns")).toEqual({ passed: true, findings: [] });
  expect(peek("signals")).toEqual({ total: 0, hits: [] });
  expect(result.current.busy).toBe(false);
  expect(mock.ip).toHaveBeenCalledOnce();
});

it("uses one snapshot for the local IP self-test and recovers from a failed refresh", async () => {
  const { result } = renderHook(useChecks, {
    wrapper: ({ children }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    ),
  });
  await act(async () => {
    await result.current.purity.run();
  });
  await waitFor(() => expect(result.current.purity.done).toBe(true));
  expect(mock.ip).toHaveBeenCalledOnce();
  expect(peek("ip")).toMatchObject({ fraudScore: 3, isResidential: true });

  mock.ip.mockRejectedValueOnce(new Error("IPPure HTTP 503"));
  await act(async () => {
    await result.current.purity.run();
  });
  await waitFor(() => expect(result.current.purity.error).toContain("503"));
  expect(result.current.purity.done).toBe(false);

  await act(async () => {
    await result.current.purity.run();
  });
  await waitFor(() => expect(result.current.purity.error).toBeUndefined());
  expect(result.current.purity.done).toBe(true);
  expect(mock.ip).toHaveBeenCalledTimes(3);
});
