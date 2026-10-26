// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import McpPanel from "./McpPanel";
import { ToastProvider } from "../ui";
const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../lib/ipc", () => ({ call: mock.call }));
const target = (id: string, label: string, over: object = {}) => ({
  id,
  label,
  path: `C:\\fixture\\${id}`,
  official: false,
  available: true,
  entries: [] as string[],
  problem: null,
  ...over,
});
const view = (targets: string[]) => ({
  servers: [
    {
      id: "fetch",
      name: "mcp-server-fetch",
      server: { type: "stdio", command: "uvx", args: ["mcp-server-fetch"] },
      targets,
      description: null,
      homepage: null,
      tags: [],
    },
  ],
  targets: [
    target("claude-code:relay", "Claude Code · 中转环境", {
      entries: targets.includes("claude-code:relay") ? ["fetch"] : [],
    }),
    target("claude-code:default", "Claude Code · 官方默认目录", {
      official: true,
    }),
    target("opencode", "OpenCode", { available: false }),
  ],
});
beforeEach(() => {
  // jsdom 没有实现 <dialog> 的 showModal / close，弹窗组件靠它们开关（同 Subscription.test.tsx）。
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
  mock.call.mockImplementation(
    async (command: string, args?: Record<string, unknown>) => {
      if (command === "mcp_list") return view(["claude-code:relay"]);
      if (command === "mcp_set_target")
        return view(
          args?.enabled
            ? ["claude-code:relay", String(args.target)]
            : ["claude-code:relay"],
        );
      if (command === "mcp_save") return view(["claude-code:relay"]);
    },
  );
});
afterEach(cleanup);
it("ticks an official target only when asked and hides tools that are not installed", async () => {
  render(
    <ToastProvider>
      <McpPanel />
    </ToastProvider>,
  );
  const official = await screen.findByRole("button", {
    name: "Claude Code · 官方默认目录 · 官方",
  });
  expect(official.getAttribute("aria-pressed")).toBe("false");
  expect(screen.queryByRole("button", { name: "OpenCode" })).toBeNull();
  fireEvent.click(official);
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("mcp_set_target", {
      id: "fetch",
      target: "claude-code:default",
      enabled: true,
    }),
  );
});
it("fills the editor from a preset and saves to the relay environment by default", async () => {
  render(
    <ToastProvider>
      <McpPanel />
    </ToastProvider>,
  );
  fireEvent.click(await screen.findByRole("button", { name: "添加 MCP" }));
  fireEvent.change(screen.getByRole("combobox", { name: "常用的" }), {
    target: { value: "time" },
  });
  expect(
    (screen.getByLabelText("连接定义") as HTMLTextAreaElement).value,
  ).toContain("mcp-server-time");
  fireEvent.click(screen.getByRole("button", { name: "保存并写进勾选的配置" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith(
      "mcp_save",
      expect.objectContaining({
        previousId: null,
        server: expect.objectContaining({
          id: "time",
          targets: ["claude-code:relay"],
        }),
      }),
    ),
  );
});
