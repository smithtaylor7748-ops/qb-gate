// @vitest-environment jsdom
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import {
  invalidate,
  invalidateAutomatic,
  measuredAt,
  peek,
  put,
  queryClient,
  refresh,
  res,
  useResource,
} from "./store";
import { R } from "./resources";
const wrapper = ({ children }: { children: React.ReactNode }) => (
  <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
);
function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
afterEach(() => {
  cleanup();
  queryClient.clear();
  localStorage.clear();
});
describe("backend query lifecycle", () => {
  it("expires only old signals and egress measurements without starting replacement probes", () => {
    const fetcher = vi.fn(async () => "fresh measurement");
    for (const kind of ["signals", "egress"] as const) {
      for (const version of [undefined, 0]) {
        const key = `migration-${kind}-${String(version)}`;
        localStorage.setItem(
          `qb.cache.${key}`,
          JSON.stringify({
            at: 1234,
            data: "legacy pass",
            ...(version === undefined ? {} : { version }),
          }),
        );
        const hook = renderHook(
          () => useResource(key, res(fetcher, R[kind].options)),
          { wrapper },
        );
        expect(hook.result.current.data).toBeUndefined();
        expect(hook.result.current.neverLoaded).toBe(true);
        expect(measuredAt(key)).toBeNull();
      }
    }
    for (const kind of ["dns", "checkup"] as const) {
      const key = `migration-preserve-${kind}`;
      localStorage.setItem(
        `qb.cache.${key}`,
        JSON.stringify({ at: 5678, data: "retained measurement" }),
      );
      const hook = renderHook(
        () => useResource(key, res(fetcher, R[kind].options)),
        { wrapper },
      );
      expect(hook.result.current.data).toBe("retained measurement");
      expect(measuredAt(key)).toBe(5678);
      expect(JSON.parse(localStorage.getItem(`qb.cache.${key}`)!).data).toBe(
        "retained measurement",
      );
    }
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("restores matching versions and saves a manual replacement with its current version", async () => {
    const fetcher = vi.fn(async () => "new evidence");
    const def = res(fetcher, { auto: false, persist: true, persistVersion: 1 });
    const restoredKey = "version-matching";
    localStorage.setItem(
      `qb.cache.${restoredKey}`,
      JSON.stringify({ version: 1, at: 6789, data: "current evidence" }),
    );
    const restored = renderHook(() => useResource(restoredKey, def), {
      wrapper,
    });
    expect(restored.result.current.data).toBe("current evidence");
    expect(measuredAt(restoredKey)).toBe(6789);
    expect(fetcher).not.toHaveBeenCalled();

    const replacedKey = "version-replacement";
    localStorage.setItem(
      `qb.cache.${replacedKey}`,
      JSON.stringify({ at: 1234, data: "obsolete evidence" }),
    );
    const replaced = renderHook(() => useResource(replacedKey, def), {
      wrapper,
    });
    expect(replaced.result.current.neverLoaded).toBe(true);
    await act(async () => {
      await replaced.result.current.refresh();
    });
    expect(fetcher).toHaveBeenCalledOnce();
    await waitFor(() =>
      expect(replaced.result.current.data).toBe("new evidence"),
    );
    expect(
      JSON.parse(localStorage.getItem(`qb.cache.${replacedKey}`)!),
    ).toEqual({
      version: 1,
      data: "new evidence",
      at: measuredAt(replacedKey),
    });
  });

  it("does not silently accept a future or malformed cache version", () => {
    const fetcher = vi.fn(async () => "fresh");
    for (const version of [2, "1", null]) {
      const key = `version-invalid-${String(version)}-${typeof version}`;
      localStorage.setItem(
        `qb.cache.${key}`,
        JSON.stringify({ version, at: 1234, data: "unverified pass" }),
      );
      const hook = renderHook(
        () =>
          useResource(
            key,
            res(fetcher, { auto: false, persist: true, persistVersion: 1 }),
          ),
        { wrapper },
      );
      expect(hook.result.current.data).toBeUndefined();
      expect(measuredAt(key)).toBeNull();
    }
    expect(fetcher).not.toHaveBeenCalled();
  });
  it.each([false, true])(
    "refresh immediately after a mutation waits for invalidation (auto=%s)",
    async (auto) => {
      const response = deferred<string>();
      const def = res(() => response.promise, { auto });
      renderHook(() => useResource(`after-mutation-${auto}`, def), { wrapper });
      put(`after-mutation-${auto}`, "before");
      await act(async () => {
        invalidate(`after-mutation-${auto}`);
        const outcome = refresh(`after-mutation-${auto}`).then(
          () => peek(`after-mutation-${auto}`),
          (error: unknown) => String(error),
        );
        for (let i = 0; i < 10; i++) await Promise.resolve();
        response.resolve("after");
        expect(await outcome).toBe("after");
      });
    },
  );
  it("shares concurrent manual refreshes instead of cancelling the first check", async () => {
    const response = deferred<string>();
    let calls = 0;
    const def = res(
      () => {
        calls++;
        return response.promise;
      },
      { auto: false },
    );
    renderHook(() => useResource("shared-check", def), { wrapper });
    await act(async () => {
      const first = refresh("shared-check");
      await Promise.resolve();
      const second = refresh("shared-check");
      response.resolve("done");
      await Promise.all([first, second]);
    });
    expect(calls).toBe(1);
    expect(peek("shared-check")).toBe("done");
  });
  it("background updates preserve running and saved manual diagnostics while refreshing live state", async () => {
    const response = deferred<string>();
    const diagnostic = res(() => response.promise, {
      auto: false,
      persist: true,
    });
    let liveCalls = 0;
    renderHook(() => useResource("background-check", diagnostic), { wrapper });
    const live = renderHook(
      () =>
        useResource(
          "background-live",
          res(async () => ++liveCalls),
        ),
      { wrapper },
    );
    await waitFor(() => expect(live.result.current.data).toBe(1));
    await act(async () => {
      const checking = refresh("background-check");
      await Promise.resolve();
      await invalidateAutomatic();
      response.resolve("measurement");
      await checking;
      await invalidateAutomatic();
    });
    expect(peek("background-check")).toBe("measurement");
    expect(
      JSON.parse(localStorage.getItem("qb.cache.background-check")!).data,
    ).toBe("measurement");
    expect(peek("background-live")).toBe(3);
  });
  it("background updates do not cancel a live-state check with cached data", async () => {
    const response = deferred<string>();
    let calls = 0;
    const def = res(() =>
      ++calls === 1 ? Promise.resolve("old") : response.promise,
    );
    const live = renderHook(() => useResource("live-check", def), { wrapper });
    await waitFor(() => expect(live.result.current.data).toBe("old"));
    await act(async () => {
      const checking = refresh("live-check");
      await Promise.resolve();
      const notified = invalidateAutomatic();
      response.resolve("new");
      await Promise.all([checking, notified]);
    });
    expect(calls).toBe(2);
    expect(peek("live-check")).toBe("new");
  });
  it("a discarded non-cancellable measurement cannot reappear in persistent storage", async () => {
    const response = deferred<string>();
    renderHook(
      () =>
        useResource(
          "discarded-check",
          res(() => response.promise, { auto: false, persist: true }),
        ),
      { wrapper },
    );
    await act(async () => {
      const checking = refresh("discarded-check").catch(() => undefined);
      await Promise.resolve();
      invalidate("discarded-check");
      await checking;
      response.resolve("obsolete");
      await response.promise;
    });
    expect(peek("discarded-check")).toBeUndefined();
    expect(measuredAt("discarded-check")).toBeNull();
    expect(localStorage.getItem("qb.cache.discarded-check")).toBeNull();
  });
  it("refetches expired data after leaving and returning to a page", async () => {
    let calls = 0;
    const def = res(async () => ++calls, { staleMs: 0 });
    const first = renderHook(() => useResource("expiry", def), { wrapper });
    await waitFor(() => expect(first.result.current.data).toBe(1));
    first.unmount();
    const second = renderHook(() => useResource("expiry", def), { wrapper });
    await waitFor(() => expect(second.result.current.data).toBe(2));
  });
  it("keeps invalidation while the object has no mounted page", async () => {
    let calls = 0;
    const def = res(async () => ++calls);
    const first = renderHook(() => useResource("inactive", def), { wrapper });
    await waitFor(() => expect(first.result.current.data).toBe(1));
    first.unmount();
    await act(async () => {
      invalidate("inactive");
      await Promise.resolve();
    });
    const second = renderHook(() => useResource("inactive", def), { wrapper });
    await waitFor(() => expect(second.result.current.data).toBe(2));
  });
  it("an old non-cancellable IPC response cannot overwrite a successful mutation", async () => {
    const old = deferred<string>();
    const hook = renderHook(
      () =>
        useResource(
          "mutation",
          res(() => old.promise),
        ),
      { wrapper },
    );
    await waitFor(() => expect(hook.result.current.loading).toBe(true));
    await act(async () => {
      put("mutation", "new");
      old.resolve("old");
      await old.promise;
    });
    expect(peek("mutation")).toBe("new");
  });
  it("invalidation starts a new request and ignores the superseded response", async () => {
    const old = deferred<string>();
    let calls = 0;
    const def = res(() =>
      ++calls === 1 ? old.promise : Promise.resolve("new"),
    );
    const hook = renderHook(() => useResource("race", def), { wrapper });
    await waitFor(() => expect(calls).toBe(1));
    await act(async () => {
      invalidate("race");
    });
    await waitFor(() => expect(hook.result.current.data).toBe("new"));
    await act(async () => {
      old.resolve("old");
      await old.promise;
    });
    expect(peek("race")).toBe("new");
  });
  it("throws away a manual measurement instead of leaving it on screen", async () => {
    // 软件页真出过的两个：升完级版本栏还写着「可升级 →」，Chrome 清空重装完
    // 隐私审计还列着刚被删掉的扩展。标记过期对 `auto:false` 等于没做。
    let calls = 0;
    const def = res(async () => ++calls, { auto: false });
    const hook = renderHook(() => useResource("voided", def), { wrapper });
    await act(async () => {
      await hook.result.current.refresh();
    });
    expect(peek("voided")).toBe(1);
    await act(async () => {
      invalidate("voided");
      await Promise.resolve();
    });
    // 扔掉了，而且**没有**顺手再跑一次 —— 手动的就该等使用者点。
    expect(peek("voided")).toBeUndefined();
    expect(calls).toBe(1);
  });
  it("paid diagnostics stay manual through mount, invalidation and remount", async () => {
    let calls = 0;
    const def = res(async () => ++calls, { auto: false });
    const first = renderHook(() => useResource("paid", def), { wrapper });
    await act(async () => {
      invalidate("paid");
      await Promise.resolve();
    });
    expect(calls).toBe(0);
    await act(async () => {
      await first.result.current.refresh();
    });
    expect(calls).toBe(1);
    first.unmount();
    renderHook(() => useResource("paid", def), { wrapper });
    await act(async () => {
      invalidate("paid");
      await Promise.resolve();
    });
    expect(calls).toBe(1);
  });
});
