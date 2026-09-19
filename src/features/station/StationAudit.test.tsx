// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import StationAudit from "./StationAudit";
import type { Route } from "../../lib/station";
const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => undefined,
}));
const route = {
  id: "r",
  station_id: "s",
  client: "codex",
  group: "team",
  nominal_rate: null,
} as Route;
beforeEach(() => {
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    if (command === "station_billing_settings")
      return {
        backend: "newapi",
        configured: true,
        user_id: "12",
        account: "fixture",
      };
    if (command === "station_models")
      return { models: [], recommended: null, problem: "价目表不可用" };
    if (command === "station_audits") return [];
    if (command === "station_run_audit") throw new Error("后台登录已过期");
  });
});
afterEach(cleanup);
it("requires an explicit temporary key and keeps failures visible without replay", async () => {
  render(<StationAudit routes={[route]} focus="r" />);
  await screen.findByText("价目表不可用");
  const run = screen.getByRole("button", {
    name: "开始检验",
  }) as HTMLButtonElement;
  expect(run.disabled).toBe(true);
  fireEvent.change(screen.getByLabelText("检验模型"), {
    target: { value: "fixture-model" },
  });
  expect(run.disabled).toBe(true);
  fireEvent.change(screen.getByLabelText("本次临时 API Key"), {
    target: { value: "fixture-test-key" },
  });
  fireEvent.click(run);
  expect(await screen.findByRole("alert")).toHaveProperty(
    "textContent",
    "检验失败：后台登录已过期",
  );
  expect(mock.call).toHaveBeenCalledWith("station_run_audit", {
    routeId: "r",
    model: "fixture-model",
    testKey: "fixture-test-key",
    cold: false,
  });
  expect(
    (screen.getByLabelText("本次临时 API Key") as HTMLInputElement).value,
  ).toBe("");
  expect(
    mock.call.mock.calls.filter(([c]) => c === "station_run_audit"),
  ).toHaveLength(1);
});
it("obtains management credentials from the site login without using a model key", async () => {
  mock.call.mockImplementation(async (command: string) => {
    if (command === "station_billing_settings")
      return { backend: "none", configured: false, user_id: "" };
    if (command === "station_models")
      return { models: [], recommended: null, problem: null };
    if (command === "station_billing_connect")
      return {
        backend: "newapi",
        configured: true,
        user_id: "12",
        account: "fixture",
        balance: 10,
        currency: "USD",
      };
    return [];
  });
  render(<StationAudit routes={[route]} focus="r" />);
  await waitFor(() =>
    expect(
      (screen.getByLabelText("站点账号 / 邮箱") as HTMLInputElement).disabled,
    ).toBe(false),
  );
  fireEvent.change(screen.getByLabelText("站点账号 / 邮箱"), {
    target: { value: "fixture" },
  });
  fireEvent.change(screen.getByLabelText("站点密码"), {
    target: { value: "fixture-password" },
  });
  fireEvent.click(screen.getByRole("button", { name: "登录并读取账单" }));
  await screen.findByText("后台已连接，账单读取已验证。");
  expect(mock.call).toHaveBeenCalledWith("station_billing_connect", {
    stationId: "s",
    backend: "auto",
    account: "fixture",
    password: "fixture-password",
  });
  expect(mock.call.mock.calls.some(([c]) => c === "station_run_audit")).toBe(
    false,
  );
  fireEvent.click(screen.getByRole("button", { name: "重新登录" }));
  expect((screen.getByLabelText("站点密码") as HTMLInputElement).value).toBe(
    "",
  );
});
it("requires separate extra-cost consent for the seventh call", async () => {
  render(<StationAudit routes={[route]} />);
  await screen.findByText("价目表不可用");
  fireEvent.change(screen.getByLabelText("检验模型"), {
    target: { value: "fixture-model" },
  });
  fireEvent.change(screen.getByLabelText("本次临时 API Key"), {
    target: { value: "fixture-key" },
  });
  const run = screen.getByRole("button", {
    name: "开始检验",
  }) as HTMLButtonElement;
  expect(run.disabled).toBe(false);
  fireEvent.click(screen.getByRole("checkbox", { name: /增加冷前缀/ }));
  expect(run.disabled).toBe(true);
  fireEvent.click(screen.getByRole("checkbox", { name: /我确认增加第 7 次/ }));
  fireEvent.click(run);
  await screen.findByRole("alert");
  expect(mock.call).toHaveBeenCalledWith(
    "station_run_audit",
    expect.objectContaining({ cold: true }),
  );
});
