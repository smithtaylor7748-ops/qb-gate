// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import type { ManagedExternal } from "../../lib/api";
import { ExternalsBlock, ManagedDirControl } from "./ManagedPanel";

const mock = vi.hoisted(() => ({
  externals: [] as ManagedExternal[],
  cleanup: vi.fn(),
  refresh: vi.fn(),
  ok: vi.fn(),
  info: vi.fn(),
  error: vi.fn(),
}));
vi.mock("../../lib/platform", () => ({
  usePlatform: () => ({ os: "macos", system_execution_lock: false }),
}));
vi.mock("../../lib/api", async (original) => {
  const actual = await original<typeof import("../../lib/api")>();
  return { ...actual, api: { ...actual.api, managedCleanup: mock.cleanup } };
});
vi.mock("../../lib/store", async (original) => ({
  ...(await original<typeof import("../../lib/store")>()),
  invalidate: vi.fn(),
  useSession: () => [false, vi.fn()],
  useResource: (key: string) => ({
    data:
      key === "managed"
        ? {
            root: "/Users/test/QB Gate/apps",
            apps: [{ app: "codex", installed: true }],
          }
        : mock.externals,
    refresh: mock.refresh,
    loading: false,
  }),
}));
vi.mock("../../ui", async (original) => ({
  ...(await original<typeof import("../../ui")>()),
  useToast: () => ({ ok: mock.ok, info: mock.info, error: mock.error }),
}));

const manual: ManagedExternal = {
  app: "codex",
  method: "manual",
  target: "/opt/homebrew/bin/codex",
  action: "请使用 Homebrew 卸载：brew uninstall codex",
};
beforeEach(() => {
  // jsdom does not implement the native dialog methods.
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
  vi.clearAllMocks();
  mock.externals = [manual];
  mock.refresh.mockResolvedValue(undefined);
  mock.cleanup.mockResolvedValue({
    done: [],
    failed: [],
    notes: [manual.action],
  });
});
afterEach(cleanup);

it("shows package-manager instructions without offering deletion of manual-only copies", () => {
  render(<ExternalsBlock />);
  expect(screen.getByText(manual.action)).toBeTruthy();
  expect(screen.queryByRole("button", { name: /可自动清理副本/ })).toBeNull();
  expect(screen.getByRole("button", { name: "保留外部副本" })).toBeTruthy();
  expect(screen.queryByText(/照样上锁/)).toBeNull();
});

it("excludes manual copies from deletion confirmation and never reports zero removals as success", async () => {
  mock.externals.push({
    app: "codex",
    method: "delete",
    target: "/Users/test/bin/codex",
    action: "删除独立 Codex 副本",
  });
  render(<ExternalsBlock />);
  fireEvent.click(screen.getByRole("button", { name: /可自动清理副本/ }));
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByText("删除独立 Codex 副本")).toBeTruthy();
  expect(within(dialog).queryByText(manual.action)).toBeNull();
  fireEvent.change(within(dialog).getByRole("textbox"), {
    target: { value: "清除" },
  });
  fireEvent.click(within(dialog).getByRole("button", { name: "彻底清除" }));
  await waitFor(() => expect(mock.info).toHaveBeenCalledWith(manual.action));
  expect(mock.cleanup).toHaveBeenCalledWith("codex");
  expect(mock.ok).not.toHaveBeenCalled();
  expect(mock.error).not.toHaveBeenCalled();
  expect(mock.info).toHaveBeenCalledWith(
    "未清除任何副本，请按扫描结果中的卸载说明操作。",
  );
});

it("describes directory permissions without promising a macOS execution lock", () => {
  render(<ManagedDirControl />);
  fireEvent.click(screen.getByRole("button", { name: "更改目录" }));
  expect(screen.getByText(/面板会检查读写权限/)).toBeTruthy();
  expect(screen.queryByText(/锁得上|试着上锁|重新上锁/)).toBeNull();
});
