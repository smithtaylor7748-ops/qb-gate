// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import StationCodexHistory from "./StationCodexHistory";
import StationClaudeCodeExtras from "./StationClaudeCodeExtras";
import { ToastProvider } from "../../ui";
const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
const home = {
  id: "default",
  label: "官方 · 默认目录",
  path: "C:\\fixture\\.codex",
  official: true,
  exists: true,
  current: "openai",
  buckets: [
    { provider: "ccswitch", sessions: 3, threads: 3 },
    { provider: "other", sessions: 1, threads: 0 },
  ],
  compressed: 0,
  backups: [],
  problem: null,
};
beforeEach(() => {
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    if (command === "codex_history_status") return [home];
    if (command === "codex_history_merge")
      return [{ ...home, buckets: [home.buckets[1]] }];
    if (command === "claude_onboarding_status")
      return [
        {
          id: "relay",
          label: "中转环境",
          path: "C:\\fixture\\env\\.claude.json",
          official: false,
          skipped: false,
          ours: false,
          problem: null,
        },
        {
          id: "default",
          label: "官方 · 默认目录",
          path: "C:\\fixture\\.claude.json",
          official: true,
          skipped: true,
          ours: false,
          problem: null,
        },
      ];
    if (command === "claude_vscode_status") return [];
  });
});
afterEach(cleanup);
it("merges only the ticked buckets, after a confirmation naming the directory", async () => {
  render(
    <ToastProvider>
      <StationCodexHistory />
    </ToastProvider>,
  );
  fireEvent.click(await screen.findByLabelText("把 ccswitch 并进 openai"));
  fireEvent.click(screen.getByRole("button", { name: "并入勾选的 1 个桶" }));
  expect(mock.call).not.toHaveBeenCalledWith(
    "codex_history_merge",
    expect.anything(),
  );
  expect((await screen.findByRole("alert")).textContent).toContain(
    "C:\\fixture\\.codex",
  );
  fireEvent.click(screen.getByRole("button", { name: "确定并入" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("codex_history_merge", {
      home: "default",
      from: ["ccswitch"],
    }),
  );
});
it("offers no undo for an onboarding the user finished themselves", async () => {
  render(
    <ToastProvider>
      <StationClaudeCodeExtras />
    </ToastProvider>,
  );
  expect(await screen.findByRole("button", { name: "跳过" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "恢复" })).toBeNull();
  expect(screen.getByText("已跳过")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "跳过" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("claude_onboarding_set", {
      target: "relay",
      skip: true,
    }),
  );
});
