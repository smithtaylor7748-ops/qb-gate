// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import ToolFilesPanel from "./ToolFilesPanel";
import { ToastProvider } from "../ui";
import { setSession } from "../lib/store";
const mock = vi.hoisted(() => ({ call: vi.fn(), open: vi.fn() }));
vi.mock("../lib/ipc", () => ({ call: mock.call }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: mock.open }));
const file = (name: string, content: string | null, label: string | null) => ({
  name,
  label,
  exists: content != null,
  content: content ?? "",
  revision: content == null ? "missing" : `rev-${name}`,
  size: content?.length ?? 0,
  modified_ms: 0,
});
const openclaw = (installed: boolean) => ({
  tool: "open-claw",
  dir: "C:\\fixture\\.openclaw",
  installed,
  files: [file("workspace/SOUL.md", "温柔", "Agent 人格和沟通风格")],
  entries: [file("workspace/memory/2026-10-25.md", "聊了 Rust", null)],
  hermes: null,
});
let installed = true;
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
  installed = true;
  setSession("toolfiles.tool", "open-claw");
  mock.open.mockReset();
  mock.call.mockReset();
  mock.call.mockImplementation(
    async (command: string, args?: Record<string, unknown>) => {
      if (command === "tool_files_view")
        return args?.tool === "hermes"
          ? {
              tool: "hermes",
              dir: "C:\\fixture\\.hermes",
              installed: true,
              files: [file("memories/USER.md", "喜欢短的", "关于你的档案")],
              entries: [],
              hermes: {
                memory_enabled: true,
                user_enabled: true,
                memory_limit: 2200,
                user_limit: 1375,
              },
            }
          : openclaw(installed);
      if (command === "tool_files_save")
        return file(String(args?.name), "x", null);
      if (command === "tool_files_delete") return openclaw(true);
      if (command === "tool_files_hermes_save") return args?.settings;
      if (command === "tool_files_hermes_web") return "http://127.0.0.1:9119/";
      return null;
    },
  );
});
afterEach(cleanup);

it("edits a file with the revision it was read at, and deletes only after confirming", async () => {
  render(
    <ToastProvider>
      <ToolFilesPanel />
    </ToastProvider>,
  );
  expect(await screen.findByText("workspace/SOUL.md")).toBeTruthy();
  fireEvent.click(screen.getAllByRole("button", { name: "编辑" })[0]);
  fireEvent.change(screen.getByLabelText("内容"), {
    target: { value: "更温柔" },
  });
  fireEvent.click(screen.getByRole("button", { name: "存" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("tool_files_save", {
      tool: "open-claw",
      name: "workspace/SOUL.md",
      content: "更温柔",
      revision: "rev-workspace/SOUL.md",
    }),
  );

  fireEvent.click(
    screen.getByRole("button", { name: "删除 workspace/memory/2026-10-25.md" }),
  );
  expect(screen.getByText(/原内容先挪进面板的备份目录/)).toBeTruthy();
  expect(mock.call).not.toHaveBeenCalledWith(
    "tool_files_delete",
    expect.anything(),
  );
  fireEvent.click(screen.getByRole("button", { name: "删除" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("tool_files_delete", {
      tool: "open-claw",
      name: "workspace/memory/2026-10-25.md",
      revision: "rev-workspace/memory/2026-10-25.md",
    }),
  );
});

it("a tool that is not installed is read-only", async () => {
  installed = false;
  render(
    <ToastProvider>
      <ToolFilesPanel />
    </ToastProvider>,
  );
  expect(await screen.findByText(/多半没装/)).toBeTruthy();
  for (const b of screen.getAllByRole("button", { name: "编辑" }))
    expect((b as HTMLButtonElement).disabled).toBe(true);
});

it("hermes memory switches are saved, and the web panel opens only when it answers", async () => {
  render(
    <ToastProvider>
      <ToolFilesPanel />
    </ToastProvider>,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Hermes" }));
  const user = await screen.findByLabelText("USER.md 字数上限");
  fireEvent.change(user, { target: { value: "900" } });
  fireEvent.click(screen.getByRole("button", { name: "存设置" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("tool_files_hermes_save", {
      settings: {
        memory_enabled: true,
        user_enabled: true,
        memory_limit: 2200,
        user_limit: 900,
      },
    }),
  );
  fireEvent.click(screen.getByRole("button", { name: "打开 Hermes 网页面板" }));
  await waitFor(() =>
    expect(mock.open).toHaveBeenCalledWith("http://127.0.0.1:9119/"),
  );
});
