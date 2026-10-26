// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import StationDesktopDirect from "./StationDesktopDirect";
import { ToastProvider } from "../../ui";
import type { Route } from "../../lib/station";
const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
const ok = { id: "ok", client: "claude-desktop", station_id: "s", group: "a" };
const mapped = { ...ok, id: "mapped", group: "b" };
beforeEach(() => {
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    const candidates = [
      { route_id: "ok", problem: null },
      { route_id: "mapped", problem: "这条线设了模型映射" },
    ];
    if (command === "station_desktop_direct")
      return { route_id: null, base_url: null, problem: null, candidates };
    if (command === "station_desktop_direct_set")
      return {
        route_id: "ok",
        base_url: "https://fixture.invalid",
        problem: null,
        candidates,
      };
  });
});
afterEach(cleanup);
it("asks once more, naming the real key, before going direct", async () => {
  render(
    <ToastProvider>
      <StationDesktopDirect
        routes={[ok, mapped] as Route[]}
        label={(r) => r.group}
      />
    </ToastProvider>,
  );
  const radios = await screen.findAllByRole("radio");
  expect((radios[2] as HTMLInputElement).disabled).toBe(true);
  expect(screen.getByText("这条线设了模型映射")).toBeTruthy();
  fireEvent.click(radios[1]);
  expect(mock.call).not.toHaveBeenCalledWith(
    "station_desktop_direct_set",
    expect.anything(),
  );
  expect((await screen.findByRole("alert")).textContent).toContain("真 Key");
  fireEvent.click(screen.getByRole("button", { name: "确定" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("station_desktop_direct_set", {
      routeId: "ok",
    }),
  );
  expect(await screen.findByText("直连中")).toBeTruthy();
});
