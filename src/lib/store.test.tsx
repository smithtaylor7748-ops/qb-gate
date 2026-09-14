// @vitest-environment jsdom
import React from "react";
import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { invalidate, peek, put, queryClient, res, useResource } from "./store";
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
});
describe("backend query lifecycle", () => {
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
