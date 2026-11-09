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
