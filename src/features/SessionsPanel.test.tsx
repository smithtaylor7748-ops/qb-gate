// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import SessionsPanel, { bytes, who } from "./SessionsPanel";
import { ToastProvider } from "../ui";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../lib/ipc", () => ({ call: mock.call }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: vi.fn(async () => "C:/Users/demo/Desktop/会话.md"),
}));

const summary = {
  id: "claude-code:relay|projects/x/s1.jsonl",
  client: "claude-code",
  source: "claude-code:relay",
  source_label: "Claude Code · 中转环境",
  official: false,
  session_id: "s1",
  title: "把登录页改成深色",
  cwd: "D:/work/demo",
  started_at: "2026-10-20T01:00:00Z",
  updated_at: "2026-10-20T02:00:00Z",
  size: 2048,
  path: "C:/Users/demo/env/projects/x/s1.jsonl",
};
const list = {
  items: [summary],
  total: 1,
  sources: [
    {
      id: "claude-code:relay",
      label: "Claude Code · 中转环境",
      client: "claude-code",
      count: 1,
    },
  ],
  search_truncated: false,
};
const detail = {
  summary,
  messages: [
    {
      role: "user",
      kind: "text",
      name: null,
      text: "把登录页改成深色",
      truncated: false,
      at: null,
    },
    {
      role: "assistant",
      kind: "tool_use",
      name: "Edit",
      text: '{"file":"login.css"}',
      truncated: false,
      at: null,
    },
  ],
  total: 2,
  offset: 0,
  resume: "Set-Location -LiteralPath 'D:/work/demo'; claude --resume s1",
  resume_note: "这是中转环境的会话：要本机路由开着。",
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
  Object.assign(navigator, {
    clipboard: { writeText: vi.fn(async () => undefined) },
  });
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    if (command === "sessions_list") return list;
    if (command === "sessions_trash") return [];
    if (command === "sessions_read") return detail;
    if (command === "sessions_export") return null;
    if (command === "sessions_delete")
      return {
        id: "t1",
        kind: "session",
        name: "x",
        originals: [summary.path],
        deleted_at: "2026-10-26 12:00",
        size: 2048,
      };
  });
});
afterEach(cleanup);

function renderPanel() {
  return render(
    <ToastProvider>
      <SessionsPanel />
    </ToastProvider>,
  );
}

it("lists sessions, reads one, copies the resume command and exports to the chosen file", async () => {
  renderPanel();
  fireEvent.click(
    await screen.findByRole("button", { name: "把登录页改成深色" }),
  );
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("sessions_read", {
      id: summary.id,
      offset: 0,
      limit: 200,
    }),
  );
  expect(await screen.findByText("助手 · 调用工具 Edit")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "复制恢复命令" }));
  await waitFor(() =>
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(detail.resume),
  );
  fireEvent.click(screen.getByRole("button", { name: "导出 Markdown…" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("sessions_export", {
      id: summary.id,
      path: "C:/Users/demo/Desktop/会话.md",
    }),
  );
});

it("only deletes after confirming, and searches after typing stops", async () => {
  renderPanel();
  fireEvent.click(
    await screen.findByRole("button", { name: "把登录页改成深色" }),
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "删除（挪进回收站）" }),
  );
  expect(mock.call).not.toHaveBeenCalledWith(
    "sessions_delete",
    expect.anything(),
  );
  fireEvent.click(await screen.findByRole("button", { name: "挪进回收站" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("sessions_delete", {
      id: summary.id,
    }),
  );
  fireEvent.change(screen.getByLabelText("搜会话"), {
    target: { value: "深色" },
  });
  await waitFor(
    () =>
      expect(mock.call).toHaveBeenCalledWith("sessions_list", {
        text: "深色",
        source: null,
        deep: false,
        offset: 0,
        limit: 50,
      }),
    { timeout: 2000 },
  );
});

it("names each kind of message and sizes files", () => {
  expect(
    who({
      role: "tool",
      kind: "tool_result",
      name: null,
      text: "",
      truncated: false,
      at: null,
    }),
  ).toBe("工具输出");
  expect(
    who({
      role: "assistant",
      kind: "thinking",
      name: null,
      text: "",
      truncated: false,
      at: null,
    }),
  ).toBe("助手 · 思考");
  expect(bytes(2048)).toBe("2 KB");
  expect(bytes(3 * 1024 * 1024)).toBe("3.0 MB");
});
