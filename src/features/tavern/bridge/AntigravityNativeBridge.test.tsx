// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { api } from "../../../lib/api";
import type { AntigravityBridgeStatus } from "../../../lib/generated/AntigravityBridgeStatus";
import { ToastProvider } from "../../../ui/Toast";
import AntigravityNativeBridge from "./AntigravityNativeBridge";

vi.mock("../../../lib/store", async (original) => ({
  ...(await original<typeof import("../../../lib/store")>()),
  invalidate: vi.fn(),
}));

let state: AntigravityBridgeStatus;
beforeEach(() => {
  state = {
    running: false,
    config: { account_id: "", port: 5004, model: "", system_prompt: "" },
    url: "http://127.0.0.1:5004/v1",
    account_label: null,
    accounts: [
      { id: "ide-a", label: "账户 A", logged_in: true },
      { id: "ide-b", label: "账户 B", logged_in: true },
    ],
    models: [],
    detail: "本机状态",
  };
  vi.spyOn(api, "tavernAntigravityStatus").mockImplementation(async () =>
    structuredClone(state),
  );
  vi.spyOn(api, "tavernAntigravityModels").mockResolvedValue([
    { id: "gemini-returned", name: "官方模型" },
  ]);
  vi.spyOn(api, "tavernAntigravitySave").mockImplementation(async (config) => {
    state.config = { ...config };
    state.models = [{ id: "gemini-returned", name: "官方模型" }];
  });
  vi.spyOn(api, "tavernAntigravityStart").mockImplementation(async () => ({
    ...structuredClone(state),
    running: true,
  }));
  vi.spyOn(api, "tavernAntigravityStop").mockResolvedValue();
  vi.spyOn(api, "tavernAntigravityToken").mockResolvedValue(
    "fixture-local-token",
  );
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const mount = () =>
  render(
    <ToastProvider>
      <AntigravityNativeBridge />
    </ToastProvider>,
  );

describe("Antigravity native bridge controls", () => {
  it("opening and choosing an account never trigger discovery, tokens or startup", async () => {
    mount();
    const account = await screen.findByLabelText("原生桥使用的 IDE 账户");
    fireEvent.change(account, { target: { value: "ide-b" } });
    expect(api.tavernAntigravityModels).not.toHaveBeenCalled();
    expect(api.tavernAntigravityStart).not.toHaveBeenCalled();
    expect(api.tavernAntigravityToken).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "读取此账户模型" }));
    await screen.findByRole("option", { name: "官方模型 · gemini-returned" });
    expect(api.tavernAntigravityModels).toHaveBeenCalledExactlyOnceWith(
      "ide-b",
    );
    fireEvent.change(account, { target: { value: "ide-a" } });
    expect(
      screen.queryByRole("option", { name: "官方模型 · gemini-returned" }),
    ).toBeNull();
    expect(api.tavernAntigravityModels).toHaveBeenCalledTimes(1);
  });

  it("requires saved choices before start and locks source controls while running", async () => {
    mount();
    const account = await screen.findByLabelText("原生桥使用的 IDE 账户");
    fireEvent.change(account, { target: { value: "ide-a" } });
    fireEvent.click(screen.getByRole("button", { name: "读取此账户模型" }));
    await screen.findByRole("option", { name: "官方模型 · gemini-returned" });
    fireEvent.change(screen.getByLabelText("官方返回的文本模型"), {
      target: { value: "gemini-returned" },
    });
    const start = screen.getByRole("button", {
      name: "启动原生桥与酒馆",
    }) as HTMLButtonElement;
    expect(start.disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "保存原生桥配置" }));
    await waitFor(() => expect(start.disabled).toBe(false));
    fireEvent.click(start);
    await waitFor(() =>
      expect((account as HTMLSelectElement).disabled).toBe(true),
    );
    expect(api.tavernAntigravityStart).toHaveBeenCalledTimes(1);
    expect(
      (
        screen.getByRole("button", {
          name: "读取此账户模型",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  });

  it("displays discovery errors without selecting a fallback model or starting", async () => {
    vi.mocked(api.tavernAntigravityModels).mockRejectedValue(
      new Error("官方 HTTP 403：没有权限"),
    );
    mount();
    fireEvent.change(await screen.findByLabelText("原生桥使用的 IDE 账户"), {
      target: { value: "ide-a" },
    });
    fireEvent.click(screen.getByRole("button", { name: "读取此账户模型" }));
    expect((await screen.findByRole("alert")).textContent).toContain(
      "HTTP 403",
    );
    expect(api.tavernAntigravityModels).toHaveBeenCalledExactlyOnceWith(
      "ide-a",
    );
    expect(api.tavernAntigravityStart).not.toHaveBeenCalled();
    expect(
      (screen.getByLabelText("官方返回的文本模型") as HTMLSelectElement).value,
    ).toBe("");
  });
});
