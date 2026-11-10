import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "../lib/store";
// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import DeepLinkHost, {
  DeepLinkPasteButton,
  showDeepLink,
} from "./DeepLinkImport";
import { ToastProvider } from "../ui";
import type { DeepLinkEntry } from "../lib/deeplink";
const mock = vi.hoisted(() => ({ call: vi.fn(), event: () => {} }));
vi.mock("../lib/ipc", () => ({ call: mock.call }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_channel: string, callback: () => void) => {
    mock.event = callback;
    return () => undefined;
  },
}));
const provider = {
  id: 7,
  scheme: "ccswitch",
  view: {
    kind: "provider",
    client: "claude-code",
    name: "Relay",
    endpoints: ["https://api.relay.example", "https://b.relay.example"],
    key_masked: null,
    homepage: null,
    notes: null,
    model_map: {
      exact: [],
      haiku: null,
      sonnet: null,
      opus: "big",
      fable: null,
      subagent: null,
      default: "main",
    },
    upstream_format: "openai_chat",
    existing_station: null,
    ignored: [
      "链接要求导入后直接用这条线：QB Gate 不替你切，导入后在中转站页点它",
    ],
  },
};
let pending: unknown = null;
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
  pending = null;
  showDeepLink(null);
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    if (command === "platform_capabilities")
      return { os: "windows", system_execution_lock: true };
    if (command === "deeplink_peek") return pending;
    if (command === "deeplink_paste") return provider;
    if (command === "deeplink_handler_status")
      return {
        enabled: false,
        registered: false,
        can_restore: false,
        dev_build: false,
        handler: "CC Switch",
        user_choice: false,
      };
    if (command === "deeplink_handler_set")
      return {
        enabled: true,
        registered: true,
        can_restore: true,
        dev_build: false,
        handler: "QB Gate",
        user_choice: false,
      };
    if (command === "deeplink_import")
      return {
        message: "导入了「Relay · 默认分组」",
        client: "claude-code",
        route_id: "r",
        skipped: [],
      };
    return null;
  });
});
afterEach(() => {
  cleanup();
  queryClient.clear();
});

it("only takes over CC Switch web links after the explicit button and explains the previous handler", async () => {
  mount(<DeepLinkPasteButton />);
  fireEvent.click(screen.getByRole("button", { name: "从链接导入" }));
  expect(await screen.findByText("当前接收程序：CC Switch")).toBeTruthy();
  expect(mock.call).not.toHaveBeenCalledWith(
    "deeplink_handler_set",
    expect.anything(),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "由 QB Gate 接收 CC Switch 导入" }),
  );
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("deeplink_handler_set", {
      enabled: true,
    }),
  );
  expect(
    await screen.findByRole("button", { name: "还原原来的接收方式" }),
  ).toBeTruthy();
});

function mount(extra?: React.ReactNode) {
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>
        <ToastProvider>
          <DeepLinkHost />
          {extra}
        </ToastProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

it("shows a pending provider link first and imports only after confirming, with the typed key", async () => {
  pending = provider;
  mount();
  expect(await screen.findByText("导入站点 · Claude Code")).toBeTruthy();
  // 说清楚不切过去；链接里写的「直接用」列成没照做的；cc-switch 的链接照样认。
  expect(screen.getByText(/不会切过去/)).toBeTruthy();
  expect(screen.getByText(/不替你切/)).toBeTruthy();
  expect(screen.getByText(/给 CC Switch 的链接/)).toBeTruthy();
  expect(screen.getByText(/兜底 → main；Opus 档 → big/)).toBeTruthy();
  expect(mock.call).not.toHaveBeenCalledWith(
    "deeplink_import",
    expect.anything(),
  );

  fireEvent.change(screen.getByLabelText("API Key（链接里没带）"), {
    target: { value: " typed-key " },
  });
  fireEvent.change(screen.getByLabelText("分组"), { target: { value: "vip" } });
  fireEvent.click(screen.getByRole("button", { name: "导入这条线路" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("deeplink_import", {
      id: 7,
      choice: { group: "vip", key: "typed-key" },
    }),
  );
  await waitFor(() =>
    expect(screen.queryByText("导入站点 · Claude Code")).toBeNull(),
  );
});

it("dismissing clears it by id, and a broken link says why", async () => {
  pending = {
    id: 3,
    scheme: "qbgate",
    view: { kind: "error", message: "链接里没有站点地址（endpoint）" },
  };
  mount();
  expect(
    await screen.findByText("链接里没有站点地址（endpoint）"),
  ).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "关闭" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("deeplink_ack", { id: 3 }),
  );
});

it("keeps a newer received link visible when an earlier import completes", async () => {
  pending = provider;
  let complete: ((value: unknown) => void) | undefined;
  mock.call.mockImplementation(async (command: string) => {
    if (command === "deeplink_peek") return pending;
    if (command === "deeplink_import")
      return new Promise((resolve) => {
        complete = resolve;
      });
    return null;
  });
  mount();
  await screen.findByText("导入站点 · Claude Code");
  fireEvent.change(screen.getByLabelText("API Key（链接里没带）"), {
    target: { value: "typed-key" },
  });
  fireEvent.click(screen.getByRole("button", { name: "导入这条线路" }));
  await waitFor(() => expect(complete).toBeDefined());
  await act(async () =>
    showDeepLink({
      ...provider,
      id: 8,
      view: { ...provider.view, name: "New Relay" },
    } as DeepLinkEntry),
  );
  await act(async () =>
    complete?.({ message: "imported", client: null, skipped: [] }),
  );
  expect(screen.getByText("New Relay")).toBeTruthy();
  expect(screen.getByRole("button", { name: "导入这条线路" })).toBeTruthy();
});

it("ignores stale peeks after a newer link or dismissal", async () => {
  pending = provider;
  mount();
  await screen.findByText("导入站点 · Claude Code");
  const reads: ((value: unknown) => void)[] = [];
  mock.call.mockImplementation(async (command: string) => {
    if (command === "deeplink_peek")
      return new Promise((resolve) => {
        reads.push(resolve);
      });
    return null;
  });
  await act(async () => {
    mock.event();
    mock.event();
  });
  await act(async () =>
    reads[1]({
      ...provider,
      id: 8,
      view: { ...provider.view, name: "New Relay" },
    }),
  );
  await act(async () => reads[0](provider));
  expect(screen.getByText("New Relay")).toBeTruthy();
  await act(async () => mock.event());
  fireEvent.click(screen.getByRole("button", { name: "不导入" }));
  await act(async () =>
    reads[2]({
      ...provider,
      id: 8,
      view: { ...provider.view, name: "New Relay" },
    }),
  );
  expect(screen.queryByText("导入站点 · Claude Code")).toBeNull();
});

it("a pasted link goes through the same confirmation, and MCP that already exists is not offered again", async () => {
  mount(<DeepLinkPasteButton />);
  fireEvent.click(screen.getByRole("button", { name: "从链接导入" }));
  fireEvent.change(screen.getByLabelText("导入链接"), {
    target: { value: "ccswitch://v1/import?resource=provider" },
  });
  fireEvent.click(screen.getByRole("button", { name: "看看里面是什么" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("deeplink_paste", {
      url: "ccswitch://v1/import?resource=provider",
    }),
  );
  expect(await screen.findByText("导入站点 · Claude Code")).toBeTruthy();

  cleanup();
  pending = {
    id: 9,
    scheme: "qbgate",
    view: {
      kind: "mcp",
      apps: ["claude"],
      servers: [
        {
          id: "fetch",
          summary: "stdio · uvx mcp-server-fetch",
          spec: "{}",
          exists: true,
        },
      ],
      ignored: [],
    },
  };
  showDeepLink(null);
  mount();
  expect(await screen.findByText(/已经有同名的，会跳过/)).toBeTruthy();
  expect(screen.queryByRole("button", { name: "加进 MCP 列表" })).toBeNull();
});
