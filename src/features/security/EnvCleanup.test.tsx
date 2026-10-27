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
import { ToastProvider } from "../../ui";
import EnvCleanup from "./EnvCleanup";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));

const list = [
  {
    name: "ANTHROPIC_BASE_URL",
    scope: "user",
    owner: "claude-code",
    shown: "https://relay.example.test",
    expand: false,
    removable: true,
  },
  {
    name: "OPENAI_API_KEY",
    scope: "user",
    owner: "codex",
    shown: "（已设置，值不显示）",
    expand: false,
    removable: true,
  },
  {
    name: "HTTPS_PROXY",
    scope: "process",
    owner: "proxy",
    shown: "http://127.0.0.1:7890",
    expand: false,
    removable: false,
  },
];
const backup = {
  id: "env-20261027-101500",
  created: "2026-10-27T10:15:00+08:00",
  names: ["GEMINI_API_KEY"],
  restored_at: null,
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
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    if (command === "env_vars_scan") return list;
    if (command === "env_backups") return [backup];
    if (command === "env_vars_remove")
      return {
        backup_id: "env-20261027-110000",
        removed: ["ANTHROPIC_BASE_URL"],
        missing: [],
        failed: [],
      };
    if (command === "env_backup_restore")
      return {
        restored: [],
        already: [],
        conflicts: ["GEMINI_API_KEY"],
        failed: [],
      };
    return null;
  });
});
afterEach(() => {
  cleanup();
  queryClient.clear();
});

function open() {
  render(
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
        <EnvCleanup />
      </ToastProvider>
    </QueryClientProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "清理环境变量…" }));
}

it("ticks nothing up front, only offers the current-user ones, and deletes exactly the ticked names after confirming", async () => {
  open();
  const base = await screen.findByRole("checkbox", {
    name: "ANTHROPIC_BASE_URL",
  });
  expect(screen.getAllByRole("checkbox")).toHaveLength(2);
  expect(screen.queryByRole("checkbox", { name: "HTTPS_PROXY" })).toBeNull();
  expect((base as HTMLInputElement).checked).toBe(false);
  const go = screen.getByRole("button", { name: /先备份，再删除勾选的 0 个/ });
  expect((go as HTMLButtonElement).disabled).toBe(true);

  fireEvent.click(base);
  fireEvent.click(
    screen.getByRole("button", { name: /先备份，再删除勾选的 1 个/ }),
  );
  expect(mock.call).not.toHaveBeenCalledWith(
    "env_vars_remove",
    expect.anything(),
  );
  fireEvent.click(await screen.findByRole("button", { name: "备份并删除" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("env_vars_remove", {
      names: ["ANTHROPIC_BASE_URL"],
    }),
  );
  expect(
    await screen.findByText(
      /已删除 ANTHROPIC_BASE_URL（备份 env-20261027-110000）/,
    ),
  ).toBeTruthy();
});

it("restoring reports a value that changed since instead of overwriting it", async () => {
  open();
  fireEvent.click(await screen.findByRole("button", { name: "还原" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("env_backup_restore", {
      id: backup.id,
    }),
  );
  expect(await screen.findByText(/没覆盖：GEMINI_API_KEY/)).toBeTruthy();
});
