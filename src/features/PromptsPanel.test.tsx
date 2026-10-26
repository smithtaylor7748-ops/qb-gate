// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import PromptsPanel from "./PromptsPanel";
import { ToastProvider } from "../ui";
const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../lib/ipc", () => ({ call: mock.call }));
const prompt = (id: string, name: string) => ({
  id,
  name,
  content: `${name} 的内容`,
  description: null,
  created_ms: 0,
  updated_ms: 0,
});
const view = {
  prompts: [prompt("a", "规矩 A"), prompt("b", "规矩 B")],
  targets: [
    {
      id: "claude-code:relay",
      label: "Claude Code · 中转环境",
      path: "C:\\fixture\\CLAUDE.md",
      official: false,
      available: true,
      active: "a",
      chars: 10,
      drift: false,
    },
    {
      id: "opencode",
      label: "OpenCode",
      path: "C:\\fixture\\AGENTS.md",
      official: false,
      available: false,
      active: null,
      chars: null,
      drift: false,
    },
  ],
};
beforeEach(() => {
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    if (command.startsWith("prompts_")) return view;
  });
});
afterEach(cleanup);
it("switches a target's prompt and refuses to delete one that is in use", async () => {
  render(
    <ToastProvider>
      <PromptsPanel />
    </ToastProvider>,
  );
  const picker = await screen.findByRole("combobox", {
    name: "Claude Code · 中转环境 用哪一条",
  });
  expect(
    screen.queryByRole("combobox", { name: "OpenCode 用哪一条" }),
  ).toBeNull();
  fireEvent.change(picker, { target: { value: "b" } });
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("prompts_enable", {
      target: "claude-code:relay",
      id: "b",
    }),
  );
  expect(
    (screen.getByRole("button", { name: "删除 规矩 A" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  expect(
    (screen.getByRole("button", { name: "删除 规矩 B" }) as HTMLButtonElement)
      .disabled,
  ).toBe(false);
});
