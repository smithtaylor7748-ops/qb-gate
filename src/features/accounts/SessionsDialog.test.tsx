// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { SessionsEntry } from "./SessionsDialog";
import { ToastProvider } from "../../ui";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));

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
    if (command === "sessions_trash") return [];
    if (command === "sessions_list")
      return {
        items: [],
        total: 0,
        search_truncated: false,
        sources: [
          {
            id: "claude-code:default",
            label: "默认",
            client: "claude-code",
            count: 9,
          },
          {
            id: "claude-code:slot:a",
            label: "Claude Code · 官方槽位 a",
            client: "claude-code",
            count: 1,
          },
          {
            id: "claude-code:slot:b",
            label: "Claude Code · 官方槽位 b",
            client: "claude-code",
            count: 2,
          },
        ],
      };
    if (command === "antigravity_conversations")
      return [
        {
          id: "c1",
          dir: "antigravity",
          title: null,
          updated_at: "2026-11-01T00:00:00Z",
          size: 2048,
          legacy: false,
        },
      ];
  });
});
afterEach(cleanup);

it("opens on the current slot and offers only that client's slots", async () => {
  render(
    <ToastProvider>
      <SessionsEntry kind="claude" current="b" />
    </ToastProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "会话" }));
  expect(mock.call).toHaveBeenCalledWith(
    "sessions_list",
    expect.objectContaining({ source: "claude-code:slot:b", kind: "chat" }),
  );
  const select = (await screen.findByLabelText("槽位")) as HTMLSelectElement;
  await screen.findByRole("option", { name: "a · 1 个" });
  expect([...select.options].map((o) => o.value)).toEqual([
    "claude-code:slot:a",
    "claude-code:slot:b",
  ]);
  expect(select.value).toBe("claude-code:slot:b");
});

it("antigravity is a read-only, machine-wide list", async () => {
  render(
    <ToastProvider>
      <SessionsEntry kind="antigravity" />
    </ToastProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "会话" }));
  expect(await screen.findByText("读不出标题")).toBeTruthy();
  expect(
    screen.getAllByText("全机共用，不分账户", { exact: false }).length,
  ).toBeGreaterThan(0);
  expect(screen.queryByRole("button", { name: /删除/ })).toBeNull();
});
