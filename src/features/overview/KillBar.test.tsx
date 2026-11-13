// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

import KillBar from "./KillBar";
import { ToastProvider } from "../../ui";
import type { KillReport } from "../../lib/api";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));

const report = {
  targets: [
    {
      process_created: "1",
      pid: 123,
      name: "claude.exe",
      path: "C:/fixture/claude.exe",
      evidence: "AnthropicSigned",
      role: "code",
    },
  ],
  killed: [],
  failed: [],
  relocked: 0,
  spared: [],
} as KillReport;

beforeEach(() => {
  const proto = HTMLDialogElement.prototype as HTMLDialogElement & {
    showModal?: () => void;
  };
  if (typeof proto.showModal !== "function")
    proto.showModal = function (this: HTMLDialogElement) {
      this.setAttribute("open", "");
    };
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    if (command === "killswitch_preview" || command === "killswitch_execute")
      return report;
    throw new Error(`unexpected command: ${command}`);
  });
});

afterEach(cleanup);

it("closes only Claude clients and keeps the shared tavern and other providers running", async () => {
  render(
    <ToastProvider>
      <KillBar />
    </ToastProvider>,
  );

  fireEvent.click(screen.getByRole("button", { name: "一键关闭所有 Claude" }));
  await screen.findByText(/GPT、Gemini、反重力以及共用的酒馆页面继续运行/);
  expect(mock.call).toHaveBeenCalledWith("killswitch_preview", {
    claudeOnly: true,
  });

  fireEvent.click(screen.getByRole("button", { name: "确认关闭 1 个进程" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("killswitch_execute", {
      claudeOnly: true,
    }),
  );
});

it("keeps a partial Claude shutdown failure visible instead of reporting success", async () => {
  mock.call.mockImplementation(async (command: string) => {
    if (command === "killswitch_preview") return report;
    if (command === "killswitch_execute")
      return { ...report, killed: [321], failed: [[123, "access denied"]] };
    throw new Error(`unexpected command: ${command}`);
  });
  render(
    <ToastProvider>
      <KillBar />
    </ToastProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "一键关闭所有 Claude" }));
  fireEvent.click(
    await screen.findByRole("button", { name: "确认关闭 1 个进程" }),
  );
  await waitFor(() =>
    expect(
      screen.getAllByText(/PID 123: access denied/).length,
    ).toBeGreaterThan(0),
  );
  expect(
    screen.getByRole("button", { name: "确认关闭 1 个进程" }),
  ).toBeTruthy();
  expect(screen.queryByText("关闭 1 个 Claude 进程")).toBeNull();
});

it("shows steps that are not tied to one process without a fake PID 0", async () => {
  // 停桥接、交回租约这类步骤后端记成 PID 0（2026-11-13）：界面只写原因，不写「PID 0」。
  mock.call.mockImplementation(async (command: string) => {
    if (command === "killswitch_preview") return report;
    if (command === "killswitch_execute")
      return {
        ...report,
        killed: [123],
        failed: [[0, "停 Claude 酒馆桥接：端口被占"]],
      };
    throw new Error(`unexpected command: ${command}`);
  });
  render(
    <ToastProvider>
      <KillBar />
    </ToastProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "一键关闭所有 Claude" }));
  fireEvent.click(
    await screen.findByRole("button", { name: "确认关闭 1 个进程" }),
  );
  await waitFor(() =>
    expect(
      screen.getAllByText(/停 Claude 酒馆桥接：端口被占/).length,
    ).toBeGreaterThan(0),
  );
  expect(screen.queryAllByText(/PID 0/)).toHaveLength(0);
});
