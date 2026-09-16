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
import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient, setSession } from "../../lib/store";
import type { Ipv6Status } from "../../lib/generated/Ipv6Status";
import Ipv6Panel from "./Ipv6Panel";

const mocks = vi.hoisted(() => ({ set: vi.fn(), read: vi.fn() }));
vi.mock("../../lib/api", () => ({
  api: { ipv6Set: (disable: boolean) => mocks.set(disable) },
}));
vi.mock("../../lib/resources", async () => {
  const { res } = await import("../../lib/store");
  return { R: { ipv6: res(() => mocks.read()) } };
});
const base: Ipv6Status = {
  disable: true,
  busy: false,
  restore_pending: 2,
  error: null,
  bindings: [
    { id: "a", name: "本地连接* 1", enabled: false },
    { id: "b", name: "WLAN", enabled: false },
  ],
};
beforeEach(() => {
  mocks.set.mockReset();
  mocks.read.mockResolvedValue(structuredClone(base));
  setSession("purity.ipv6.busy", false);
  setSession("purity.ipv6.error", "");
});
afterEach(() => {
  cleanup();
  queryClient.clear();
});
const mount = () =>
  render(
    <QueryClientProvider client={queryClient}>
      <Ipv6Panel />
    </QueryClientProvider>,
  );

it("restores the original mix of enabled and disabled adapters when switched off", async () => {
  mocks.set.mockResolvedValue({
    ...base,
    disable: false,
    restore_pending: 0,
    bindings: [{ ...base.bindings[0], enabled: true }, base.bindings[1]],
  });
  mount();
  await screen.findByText("网卡 IPv6 已禁用");
  fireEvent.click(screen.getByRole("switch"));
  await waitFor(() =>
    expect((screen.getByRole("switch") as HTMLInputElement).checked).toBe(
      false,
    ),
  );
  expect(mocks.set).toHaveBeenCalledWith(false);
  expect(screen.getByText("本地连接* 1：IPv6 启用")).toBeTruthy();
  expect(screen.getByText("WLAN：IPv6 禁用")).toBeTruthy();
});

it("keeps intent distinct from a failed disable and offers retry", async () => {
  mocks.read.mockResolvedValue({
    ...base,
    bindings: [{ ...base.bindings[0], enabled: true }],
    error: "管理员授权被取消",
  });
  mount();
  expect(await screen.findByText("1 张网卡仍启用 IPv6")).toBeTruthy();
  expect(screen.queryByText("网卡 IPv6 已禁用")).toBeNull();
  expect((screen.getByRole("switch") as HTMLInputElement).checked).toBe(true);
  fireEvent.click(screen.getByText("状态与恢复"));
  expect(screen.getByRole("alert").textContent).toContain("管理员授权被取消");
  expect(screen.getByRole("button", { name: "重新应用" })).toBeTruthy();
});

it("a long change stays busy across closing and reopening the modal", async () => {
  let finish!: (value: Ipv6Status) => void;
  mocks.set.mockImplementation(
    () =>
      new Promise<Ipv6Status>((resolve) => {
        finish = resolve;
      }),
  );
  const first = mount();
  await screen.findByText("网卡 IPv6 已禁用");
  fireEvent.click(screen.getByRole("switch"));
  first.unmount();
  mount();
  expect((screen.getByRole("switch") as HTMLInputElement).disabled).toBe(true);
  await act(async () =>
    finish({
      ...base,
      disable: false,
      restore_pending: 1,
      error: "网卡未连接，原值已保留",
    }),
  );
  fireEvent.click(screen.getByText("状态与恢复"));
  expect(screen.getByRole("alert").textContent).toContain("原值已保留");
  expect(screen.getByText("1 张网卡的原设置待恢复")).toBeTruthy();
  expect(mocks.set).toHaveBeenCalledTimes(1);
});
