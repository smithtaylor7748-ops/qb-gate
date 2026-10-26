// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import SkillsPanel from "./SkillsPanel";
import { ToastProvider } from "../ui";
const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../lib/ipc", () => ({ call: mock.call }));
const target = (id: string, label: string, over: object = {}) => ({
  id,
  label,
  path: `C:\\fixture\\${id}\\skills`,
  official: false,
  available: true,
  entries: [] as string[],
  ...over,
});
const view = {
  skills: [] as unknown[],
  targets: [
    target("claude-code:relay", "Claude Code · 中转环境"),
    target("claude-code:default", "Claude Code · 官方默认目录", {
      official: true,
    }),
    target("opencode", "OpenCode", { available: false }),
  ],
  unmanaged: [{ target: "claude-code:default", directory: "mine" }],
  backups: [],
};
const candidate = {
  name: "pdf",
  description: "PDF 工具",
  license: "MIT",
  source: "https://github.com/o/r/tree/HEAD/pdf",
  revision: "abc",
  fingerprint: "f",
};
beforeEach(() => {
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
  sessionStorage.clear();
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    if (command === "skills_list") return view;
    if (command === "skills_discover") return [candidate];
    if (command === "skills_install") return view;
  });
});
afterEach(cleanup);
it("scans a source, then installs the picked skill into the relay environment by default", async () => {
  render(
    <ToastProvider>
      <SkillsPanel />
    </ToastProvider>,
  );
  fireEvent.click(await screen.findByRole("button", { name: "添加 Skill" }));
  fireEvent.change(
    screen.getByLabelText("GitHub 仓库 / 子目录地址，或本地目录"),
    { target: { value: "https://github.com/o/r" } },
  );
  fireEvent.click(screen.getByRole("button", { name: "扫描" }));
  await screen.findByText("pdf");
  expect(mock.call).not.toHaveBeenCalledWith(
    "skills_install",
    expect.anything(),
  );
  expect(screen.queryByRole("checkbox", { name: /OpenCode/ })).toBeNull();
  fireEvent.click(
    screen.getByRole("button", { name: "装上并复制到勾选的地方" }),
  );
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("skills_install", {
      picked: [candidate],
      targets: ["claude-code:relay"],
    }),
  );
});
it("lists skills the panel does not manage so they can be imported", async () => {
  render(
    <ToastProvider>
      <SkillsPanel />
    </ToastProvider>,
  );
  expect(await screen.findByText("没管着的 · 1 个")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "导入" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("skills_import", {
      target: "claude-code:default",
      directory: "mine",
    }),
  );
});
