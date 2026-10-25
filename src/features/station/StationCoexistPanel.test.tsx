// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import StationCoexistPanel from "./StationCoexistPanel";
import { ToastProvider } from "../../ui";
const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
const row = (tool: string, label: string, over: object = {}) => ({
  tool,
  label,
  path: `C:\\fixture\\${tool}.json`,
  installed: true,
  written: false,
  problem: null,
  ...over,
});
beforeEach(() => {
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    if (command === "station_coexist_status")
      return [
        row("open-code", "OpenCode"),
        row("hermes", "Hermes", { installed: false }),
      ];
    if (command === "station_coexist_write")
      return [
        row("open-code", "OpenCode", { written: true }),
        row("hermes", "Hermes", { installed: false }),
      ];
  });
});
afterEach(cleanup);
it("asks once more, naming the file, before writing the real key", async () => {
  render(
    <ToastProvider>
      <StationCoexistPanel routeId="r" />
    </ToastProvider>,
  );
  const buttons = await screen.findAllByRole("button", { name: "写入" });
  // 没装的那个不能写（不替它建目录）。
  expect((buttons[1] as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(buttons[0]);
  expect(mock.call).not.toHaveBeenCalledWith(
    "station_coexist_write",
    expect.anything(),
  );
  expect((await screen.findByRole("alert")).textContent).toContain(
    "C:\\fixture\\open-code.json",
  );
  fireEvent.click(screen.getByRole("button", { name: "确定写入" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("station_coexist_write", {
      routeId: "r",
      tool: "open-code",
    }),
  );
  expect(await screen.findByText("已写入")).toBeTruthy();
  expect(screen.getByRole("button", { name: "更新" })).toBeTruthy();
});
