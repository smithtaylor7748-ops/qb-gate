// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import Console from "./Console";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../../lib/ipc", () => ({ call: mock.call }));

afterEach(() => {
  cleanup();
  mock.call.mockReset();
});

const chunk = (over: Record<string, unknown>) => ({
  exists: true,
  run: "run-now",
  started_at: "2026-09-29 20:12:44",
  offset: 1,
  lines: [] as string[],
  truncated: false,
  more: false,
  path: "C:/Users/demo/AppData/Local/ClaudeIpGate/plugins/sillytavern-console.log",
  running: false,
  hints: [] as unknown[],
  ...over,
});

// 2026-10-28：发 0.25.8 时 CI 上的 test:ui 红在这 —— 切到「上一次」时「这一次」那一读还在路上，
// 回来晚了把视图和游标改回「这一次」，「上一次」又不再读，于是一直挂着「这一次」的报错提示。
it("切到「上一次」之后才回来的「这一次」那一读被丢掉", async () => {
  let release: (c: unknown) => void = () => {};
  mock.call.mockImplementation((cmd: string, args: { previous: boolean }) => {
    expect(cmd).toBe("tavern_console");
    if (!args.previous)
      return new Promise((resolve) => {
        release = resolve;
      });
    return Promise.resolve(
      chunk({
        run: "run-prev",
        started_at: "2026-09-28 21:10:03",
        lines: ["SillyTavern 1.18.0"],
      }),
    );
  });
  render(<Console />);
  await waitFor(() => expect(mock.call).toHaveBeenCalledTimes(1));

  fireEvent.click(screen.getByRole("button", { name: "上一次" }));
  await screen.findByText("起于 2026-09-28 21:10:03");

  release(
    chunk({
      lines: ["Address 127.0.0.1:8000 is already in use."],
      hints: [
        {
          id: "port-in-use",
          title: "酒馆端口被占了",
          detail: "关掉另一份酒馆，或者换一个端口。",
          line: "Address 127.0.0.1:8000 is already in use.",
        },
      ],
    }),
  );
  await new Promise((r) => setTimeout(r, 30));

  expect(screen.queryByTestId("tavern-console-hints")).toBeNull();
  expect(screen.queryByText(/already in use/)).toBeNull();
  expect(screen.getByText("SillyTavern 1.18.0")).toBeTruthy();
  expect(screen.getByText("起于 2026-09-28 21:10:03")).toBeTruthy();
  // 「上一次」只读一遍，不轮询。
  expect(mock.call).toHaveBeenCalledTimes(2);
});
