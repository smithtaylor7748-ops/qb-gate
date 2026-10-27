// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import SiteUsageScript, { SCRIPT_TEMPLATES } from "./SiteUsageScript";
import { ToastProvider } from "../../ui";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));

const quota = {
  provider: "用量脚本",
  kind: "balance",
  balances: [{ remaining: 23.4, total: null, used: null, unit: "USD" }],
  windows: [],
  plan: "这把 Key 的额度",
  warning: null,
  fetched_at: "2026-10-26T00:00:00Z",
  route_id: "r1",
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
});
afterEach(cleanup);

function open(onSaved = vi.fn(), onClose = vi.fn()) {
  render(
    <ToastProvider>
      <SiteUsageScript
        stationId="moxi"
        routeId="r1"
        siteName="备用中转"
        onClose={onClose}
        onSaved={onSaved}
      />
    </ToastProvider>,
  );
  return { onSaved, onClose };
}

it("starts from the New API template, test-runs with this line's key and saves only what was asked", async () => {
  mock.call.mockImplementation(async (command: string) => {
    if (command === "station_usage_script_get")
      return { enabled: false, code: "", updated_at: null };
    if (command === "station_usage_script_test") return quota;
    if (command === "station_usage_script_save")
      return { enabled: true, code: SCRIPT_TEMPLATES[0].code, updated_at: "x" };
  });
  const { onSaved, onClose } = open();
  const box = (await screen.findByLabelText("脚本")) as HTMLTextAreaElement;
  await waitFor(() => expect(box.value).toBe(SCRIPT_TEMPLATES[0].code));
  // 同源、https 的那句话就在编辑器里，不藏进悬停。
  expect(screen.getByText(/必须跟这个站点同源、而且是 https/)).toBeTruthy();

  fireEvent.click(
    screen.getByRole("button", { name: "用这条线路的 Key 试跑一次" }),
  );
  expect(await screen.findByTestId("usage-script-result")).toBeTruthy();
  expect(screen.getByTestId("usage-script-result").textContent).toContain(
    "余额 $23.40",
  );
  expect(mock.call).toHaveBeenCalledWith("station_usage_script_test", {
    routeId: "r1",
    code: SCRIPT_TEMPLATES[0].code,
  });
  expect(mock.call).not.toHaveBeenCalledWith(
    "station_usage_script_save",
    expect.anything(),
  );

  fireEvent.click(screen.getByLabelText(/开着/));
  fireEvent.click(screen.getByRole("button", { name: "存" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("station_usage_script_save", {
      stationId: "moxi",
      enabled: true,
      code: SCRIPT_TEMPLATES[0].code,
    }),
  );
  await waitFor(() => expect(onSaved).toHaveBeenCalled());
  expect(onClose).toHaveBeenCalled();
});

it("shows why a test run was refused and keeps what was typed", async () => {
  mock.call.mockImplementation(async (command: string) => {
    if (command === "station_usage_script_get")
      return {
        enabled: true,
        code: "({ request: { url: 'x' } })",
        updated_at: "x",
      };
    if (command === "station_usage_script_test")
      throw new Error("请求地址必须跟站点同源 —— Key 不发给别的地方");
  });
  open();
  const box = (await screen.findByLabelText("脚本")) as HTMLTextAreaElement;
  await waitFor(() => expect(box.value).toBe("({ request: { url: 'x' } })"));
  fireEvent.click(
    screen.getByRole("button", { name: "用这条线路的 Key 试跑一次" }),
  );
  expect(await screen.findByText(/Key 不发给别的地方/)).toBeTruthy();
  expect(box.value).toBe("({ request: { url: 'x' } })");
});
