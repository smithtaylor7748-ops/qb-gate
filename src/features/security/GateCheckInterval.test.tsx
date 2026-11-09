// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "../../lib/store";
import GateCheckInterval from "./GateCheckInterval";

const mocks = vi.hoisted(() => ({ save: vi.fn(), read: vi.fn() }));
vi.mock("../../lib/api", () => ({
  api: { gateCheckIntervalSave: (seconds: number) => mocks.save(seconds) },
}));
vi.mock("../../lib/resources", async () => {
  const { res } = await import("../../lib/store");
  return { R: { settings: res(() => mocks.read()) } };
});

beforeEach(() => {
  mocks.save.mockReset();
  mocks.read.mockResolvedValue({
    gate_check_interval_secs: 5,
    codex_outside_gate: true,
  });
});
afterEach(() => {
  cleanup();
  queryClient.clear();
});
const mount = () =>
  render(
    <QueryClientProvider client={queryClient}>
      <GateCheckInterval />
    </QueryClientProvider>,
  );

it("saves through the dedicated command and reloads the persisted interval after reopening", async () => {
  mocks.save.mockImplementation(async (seconds: number) => {
    const saved = {
      gate_check_interval_secs: seconds,
      codex_outside_gate: true,
    };
    mocks.read.mockResolvedValue(saved);
    return saved;
  });
  const first = mount();
  const input = await screen.findByRole("spinbutton", {
    name: "IP 锁检测间隔（秒）",
  });
  await waitFor(() => expect((input as HTMLInputElement).disabled).toBe(false));
  fireEvent.change(input, { target: { value: "15" } });
  fireEvent.click(screen.getByRole("button", { name: "保存间隔" }));
  await screen.findByText("当前已保存：15 秒");
  expect(mocks.save).toHaveBeenCalledWith(15);
  expect(queryClient.getQueryData(["settings"])).toEqual({
    gate_check_interval_secs: 15,
    codex_outside_gate: true,
  });
  first.unmount();
  queryClient.clear();
  mount();
  await screen.findByText("当前已保存：15 秒");
  expect((screen.getByRole("spinbutton") as HTMLInputElement).value).toBe("15");
});

it("rejects empty, fractional and out of range values before calling the backend", async () => {
  mount();
  const input = await screen.findByRole("spinbutton");
  await waitFor(() => expect((input as HTMLInputElement).disabled).toBe(false));
  for (const value of ["", "0", "-1", "1.5", "31"]) {
    fireEvent.change(input, { target: { value } });
    expect(
      (screen.getByRole("button", { name: "保存间隔" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(screen.getByText("请输入 1–30 秒的整数。")).toBeTruthy();
  }
  expect(mocks.save).not.toHaveBeenCalled();
  fireEvent.change(input, { target: { value: "1" } });
  expect(
    (screen.getByRole("button", { name: "保存间隔" }) as HTMLButtonElement)
      .disabled,
  ).toBe(false);
});

it("retains the saved value and editable draft after a rejected save", async () => {
  mocks.save.mockRejectedValue(new Error("配置写入失败"));
  mount();
  const input = await screen.findByRole("spinbutton");
  await waitFor(() => expect((input as HTMLInputElement).disabled).toBe(false));
  fireEvent.change(input, { target: { value: "30" } });
  fireEvent.click(screen.getByRole("button", { name: "保存间隔" }));
  expect((await screen.findByRole("alert")).textContent).toBe("配置写入失败");
  expect(screen.getByText("当前已保存：5 秒")).toBeTruthy();
  expect((input as HTMLInputElement).value).toBe("30");
  expect(screen.queryByText("检测间隔已保存。")).toBeNull();
});
