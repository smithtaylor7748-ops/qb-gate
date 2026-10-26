// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import StationProjects from "./StationProjects";
import { ToastProvider } from "../../ui";
const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
const slot = { route: "r", queue: [], mcp: [], skills: [], prompt: null };
const view = {
  projects: [
    { id: "a", name: "开发", slots: { codex: slot }, updated_ms: 0 },
    { id: "b", name: "绘图", slots: {}, updated_ms: 0 },
  ],
  current: { codex: "a" },
};
beforeEach(() => {
  // jsdom 没有实现 <dialog> 的 showModal / close，弹窗组件靠它们开关（同 McpPanel.test.tsx）。
  const proto = HTMLDialogElement.prototype as HTMLDialogElement & {
    showModal?: () => void;
    close?: () => void;
  };
  if (typeof proto.showModal !== "function")
    proto.showModal = function (this: HTMLDialogElement) {
      this.setAttribute("open", "");
    };
  if (typeof proto.close !== "function")
    proto.close = function (this: HTMLDialogElement) {
      this.removeAttribute("open");
    };
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    if (command === "projects_apply")
      return {
        view: { ...view, current: { codex: "b" } },
        warnings: ["派单顺序里有 1 条线已经不在了，跳过"],
      };
    if (command === "projects_clear")
      return { view: { ...view, current: {} }, warnings: [] };
    if (command.startsWith("projects_")) return view;
  });
});
afterEach(cleanup);
it("switches per software, shows what did not land, and only deletes after confirming", async () => {
  const applied = vi.fn();
  render(
    <ToastProvider>
      <StationProjects
        client="codex"
        clientName="Codex 桌面端"
        onApplied={applied}
      />
    </ToastProvider>,
  );
  fireEvent.click(await screen.findByRole("button", { name: /项目：开发/ }));
  // 说清楚只换中转那一侧。
  expect(screen.getByText(/官方账户和槽位一概不动/)).toBeTruthy();
  // 当前那个不能再「换到」；没存过 Codex 那一组的标出来。
  expect(
    (screen.getByRole("button", { name: "正在用" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  expect(screen.getByText(/Codex 桌面端还没存过/)).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "换到这个" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("projects_apply", {
      id: "b",
      client: "codex",
    }),
  );
  // 没落成的那几样留在弹窗里，中转站页要重读状态。
  expect(
    await screen.findByText("派单顺序里有 1 条线已经不在了，跳过"),
  ).toBeTruthy();
  expect(applied).toHaveBeenCalled();

  // 删除要再确认一次，确认之前什么都不发。
  fireEvent.click(screen.getByRole("button", { name: "删除「开发」" }));
  expect(screen.getByText(/不会改动任何现有配置/)).toBeTruthy();
  expect(mock.call).not.toHaveBeenCalledWith("projects_delete", { id: "a" });
  fireEvent.click(screen.getByRole("button", { name: "删除" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("projects_delete", { id: "a" }),
  );
});

it("creates from the current state of this software only", async () => {
  render(
    <ToastProvider>
      <StationProjects client="claude-code" clientName="Claude Code" />
    </ToastProvider>,
  );
  fireEvent.click(await screen.findByRole("button", { name: "不使用项目" }));
  const name = screen.getByRole("textbox", { name: "新项目的名字" });
  fireEvent.change(name, { target: { value: "  写作  " } });
  fireEvent.click(screen.getByRole("button", { name: /按现在的样子新建/ }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("projects_create", {
      name: "写作",
      client: "claude-code",
    }),
  );
});
