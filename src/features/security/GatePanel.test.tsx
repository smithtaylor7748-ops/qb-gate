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
import { queryClient, setSession } from "../../lib/store";
import { ToastProvider } from "../../ui";
import type { GateStatus } from "../../lib/api";
import { AllowlistPanel, LockPanel } from "./GatePanel";

const mock = vi.hoisted(() => ({ call: vi.fn(), gate: {} as GateStatus }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));

beforeEach(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
  setSession("security.running", "");
  setSession("security.batch", false);
  setSession("security.failed", {});
  mock.gate = {
    current_ip: null,
    allowlist: [],
    ip_allowed: false,
    targets: [],
    all_locked: false,
    lease: { holders: {}, holder: null, granted: [], mode: null },
    watchdog_running: true,
    stale_copies: [],
    recent_log: ["2026-11-10 12:00:00 出口 IP 不合格，受管会话已停止"],
    needs_reopen: null,
  };
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    if (command === "platform_capabilities")
      return { os: "macos", system_execution_lock: false };
    if (command === "gate_status") return structuredClone(mock.gate);
    if (command === "settings_load")
      return {
        gate_check_interval_secs: 5,
        country_allowlist: [],
        codex_outside_gate: false,
        antigravity_outside_gate: false,
      };
    if (command === "country_presets") return [];
    if (command === "progress_set") return { steps: {} };
    if (command === "gate_check") {
      mock.gate.current_ip = "203.0.113.9";
      return;
    }
    if (command === "allowlist_add_current") {
      mock.gate.current_ip = "203.0.113.9";
      mock.gate.allowlist = ["203.0.113.9"];
      mock.gate.ip_allowed = true;
      return mock.gate.allowlist;
    }
    if (command === "gate_lock_all") {
      mock.gate.needs_reopen = "受管门禁已暂停";
      return 0;
    }
    if (command === "gate_reopen") return "已放行";
    throw new Error(`Unexpected command: ${command}`);
  });
});
afterEach(() => {
  cleanup();
  queryClient.clear();
  localStorage.clear();
});
const mount = (element: React.ReactNode) =>
  render(
    <QueryClientProvider client={queryClient}>
      <ToastProvider>{element}</ToastProvider>
    </QueryClientProvider>,
  );

it("can add the current IP on first use without a cached IP or starting a session", async () => {
  mount(<AllowlistPanel />);
  const add = await screen.findByRole("button", {
    name: "把当前 IP 加进白名单",
  });
  expect((add as HTMLButtonElement).disabled).toBe(false);
  fireEvent.click(add);
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("allowlist_add_current"),
  );
  await waitFor(() =>
    expect(queryClient.getQueryData(["gate"])).toMatchObject({
      current_ip: "203.0.113.9",
      allowlist: ["203.0.113.9"],
    }),
  );
  expect(
    mock.call.mock.calls.some(([command]) => /launch|reopen/.test(command)),
  ).toBe(false);
});

it("refreshes an empty gate reading through the explicit inspection command", async () => {
  mount(<AllowlistPanel />);
  fireEvent.click(await screen.findByRole("button", { name: "检测当前出口" }));
  await waitFor(() => expect(mock.call).toHaveBeenCalledWith("gate_check"));
  await waitFor(() =>
    expect(queryClient.getQueryData(["gate"])).toMatchObject({
      current_ip: "203.0.113.9",
      ip_allowed: false,
    }),
  );
  expect(
    mock.call.mock.calls.some(([command]) => /launch|reopen/.test(command)),
  ).toBe(false);
  expect(screen.queryByText(/codex.exe 上执行锁/)).toBeNull();
});

it("keeps Mac supervision and logs available and confirms closing the managed gate", async () => {
  mount(<LockPanel />);
  expect(await screen.findByText("ip-gate.log")).toBeTruthy();
  expect(await screen.findByText("运行中")).toBeTruthy();
  expect(screen.getByRole("button", { name: "验证并重新放行" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "应急解锁" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "关闭受管门禁" }));
  expect(mock.call).not.toHaveBeenCalledWith("gate_lock_all");
  fireEvent.click(await screen.findByRole("button", { name: "确认关闭" }));
  await waitFor(() => expect(mock.call).toHaveBeenCalledWith("gate_lock_all"));
  await screen.findByText("受管门禁已暂停");
  expect(mock.call).not.toHaveBeenCalledWith("gate_unlock_all");
});
