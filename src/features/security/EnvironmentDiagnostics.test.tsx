// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { getSession, setSession } from "../../lib/store";
import EnvironmentDiagnostics from "./EnvironmentDiagnostics";

const mock = vi.hoisted(() => ({
  call: vi.fn(),
  sessionKeys: new Set<string>(),
}));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
// Record session keys without replacing the real store or its unmount/reopen behaviour.
vi.mock("../../lib/store", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/store")>();
  return {
    ...actual,
    useSession: <T,>(key: string, fallback: T) => {
      mock.sessionKeys.add(key);
      return actual.useSession(key, fallback);
    },
  };
});

const PREFIX = "environment.diagnostics.";
const TITLES = ["代理运行规则", "ASN 网络类型", "Claude 客户端语言"];
const ACTIONS = [
  "检测 UDP 出口",
  "核验运行规则",
  "查询 ASN 类型",
  "读取客户端语言",
];

function report(detail = "未收到探针响应，本次不能确认") {
  return {
    checked_at: "2026-11-10 12:30",
    scope: "一次手动采集",
    items: [
      {
        id: "result",
        label: "探针结果",
        state: "unknown",
        detail,
        fixable: false,
        manual: null,
      },
    ],
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function expand(title: string) {
  const trigger = screen.getByRole("button", { name: title });
  if (trigger.getAttribute("aria-expanded") !== "true")
    fireEvent.click(trigger);
}

function input(label: string, value: string) {
  fireEvent.change(screen.getByLabelText(label), { target: { value } });
}

function action(label: string) {
  return screen.getByRole("button", { name: label }) as HTMLButtonElement;
}

beforeEach(() => {
  mock.call.mockReset();
  mock.call.mockResolvedValue(report());
  mock.sessionKeys.clear();
  for (const key of ["busy", "error", "endpoint", "ip", "expected"])
    setSession(`${PREFIX}${key}`, "");
  setSession(`${PREFIX}results`, {});
  localStorage.clear();
  sessionStorage.clear();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("manual environment diagnostics", () => {
  it("opening, expanding, collapsing and reopening never sends a request", () => {
    const mounted = render(<EnvironmentDiagnostics />);
    for (const title of ["UDP 出口对照", ...TITLES]) {
      fireEvent.click(screen.getByRole("button", { name: title }));
      fireEvent.click(screen.getByRole("button", { name: title }));
    }
    mounted.unmount();
    render(<EnvironmentDiagnostics />);
    TITLES.forEach(expand);
    expect(mock.call).not.toHaveBeenCalled();
    expect(action("核验运行规则").disabled).toBe(true);
    expect(action("查询 ASN 类型").disabled).toBe(true);
  });

  it("only manual actions send their exact trimmed inputs and optional nulls", async () => {
    render(<EnvironmentDiagnostics />);
    TITLES.forEach(expand);
    await act(async () => {
      fireEvent.click(action("检测 UDP 出口"));
    });
    expect(mock.call).toHaveBeenLastCalledWith("environment_stun");
    input("本机控制接口", "  http://127.0.0.1:19090  ");
    await act(async () => {
      fireEvent.click(action("核验运行规则"));
    });
    expect(mock.call).toHaveBeenLastCalledWith("environment_proxy_runtime", {
      endpoint: "http://127.0.0.1:19090",
      secret: null,
    });
    input("查询公网 IP", "  2001:db8::7  ");
    await act(async () => {
      fireEvent.click(action("查询 ASN 类型"));
    });
    expect(mock.call).toHaveBeenLastCalledWith("environment_asn", {
      ip: "2001:db8::7",
    });
    await act(async () => {
      fireEvent.click(action("读取客户端语言"));
    });
    expect(mock.call).toHaveBeenLastCalledWith("environment_client_locale", {
      expected: null,
    });
    input("期望语言（可选）", "  zh-TW  ");
    await act(async () => {
      fireEvent.click(action("读取客户端语言"));
    });
    expect(mock.call).toHaveBeenLastCalledWith("environment_client_locale", {
      expected: "zh-TW",
    });
    expect(mock.call).toHaveBeenCalledTimes(5);
  });

  it("clears the control secret immediately, keeps it out of session/storage and does not restore it on reopen", async () => {
    const pending = deferred<ReturnType<typeof report>>();
    mock.call.mockReturnValue(pending.promise);
    const writes = vi.spyOn(Storage.prototype, "setItem");
    const secret = "fixture-controller-secret-42";
    const mounted = render(<EnvironmentDiagnostics />);
    expand("代理运行规则");
    input("本机控制接口", "http://[::1]:19090");
    input("控制接口密钥（可选）", secret);
    expect(
      (screen.getByLabelText("控制接口密钥（可选）") as HTMLInputElement).type,
    ).toBe("password");
    fireEvent.click(action("核验运行规则"));
    expect(mock.call).toHaveBeenCalledWith("environment_proxy_runtime", {
      endpoint: "http://[::1]:19090",
      secret,
    });
    expect(
      (screen.getByLabelText("控制接口密钥（可选）") as HTMLInputElement).value,
    ).toBe("");
    mounted.unmount();
    render(<EnvironmentDiagnostics />);
    expand("代理运行规则");
    expect(
      (screen.getByLabelText("控制接口密钥（可选）") as HTMLInputElement).value,
    ).toBe("");
    expect(
      JSON.stringify(
        [...mock.sessionKeys].map((key) => [key, getSession(key, null)]),
      ),
    ).not.toContain(secret);
    expect(writes).not.toHaveBeenCalled();
    expect(localStorage.length).toBe(0);
    expect(sessionStorage.length).toBe(0);
    await act(async () => {
      pending.resolve(report());
    });
    expect(
      JSON.stringify([...mock.sessionKeys].map((key) => getSession(key, null))),
    ).not.toContain(secret);
  });

  it("renders unknown findings as unconfirmed, never as verified or failed", async () => {
    render(<EnvironmentDiagnostics />);
    fireEvent.click(action("检测 UDP 出口"));
    expect(await screen.findByText("未确认")).toBeTruthy();
    expect(screen.getByText("未收到探针响应，本次不能确认")).toBeTruthy();
    expect(screen.queryByText("已核实")).toBeNull();
    expect(screen.queryByText("失败")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("keeps all actions and targets disabled across unmount/reopen and prevents duplicate requests", async () => {
    const pending = deferred<ReturnType<typeof report>>();
    mock.call.mockReturnValue(pending.promise);
    const mounted = render(<EnvironmentDiagnostics />);
    TITLES.forEach(expand);
    input("本机控制接口", "http://127.0.0.1:19090");
    input("查询公网 IP", "203.0.113.7");
    act(() => {
      fireEvent.click(action("检测 UDP 出口"));
      fireEvent.click(action("检测 UDP 出口"));
    });
    mounted.unmount();
    render(<EnvironmentDiagnostics />);
    TITLES.forEach(expand);
    expect(screen.getByRole("status").textContent).toContain("UDP 出口对照");
    for (const label of ACTIONS) {
      expect(action(label).disabled).toBe(true);
      fireEvent.click(action(label));
    }
    for (const label of [
      "本机控制接口",
      "控制接口密钥（可选）",
      "查询公网 IP",
      "期望语言（可选）",
    ]) {
      expect((screen.getByLabelText(label) as HTMLInputElement).disabled).toBe(
        true,
      );
    }
    expect(mock.call).toHaveBeenCalledTimes(1);
    await act(async () => {
      pending.resolve(report("回到详情后仍能收到本轮结果"));
    });
    expect(await screen.findByText("回到详情后仍能收到本轮结果")).toBeTruthy();
    for (const label of ACTIONS) expect(action(label).disabled).toBe(false);
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("changing a target or control secret clears only that target's previous result", async () => {
    render(<EnvironmentDiagnostics />);
    TITLES.forEach(expand);
    mock.call.mockImplementation(async (command: string) =>
      report(`saved:${command}`),
    );
    input("本机控制接口", "http://127.0.0.1:19090");
    input("查询公网 IP", "203.0.113.7");
    for (const label of ACTIONS)
      await act(async () => {
        fireEvent.click(action(label));
      });
    input("本机控制接口", "http://127.0.0.1:19091");
    expect(screen.queryByText("saved:environment_proxy_runtime")).toBeNull();
    expect(screen.getByText("saved:environment_asn")).toBeTruthy();
    await act(async () => {
      fireEvent.click(action("核验运行规则"));
    });
    input("控制接口密钥（可选）", "changed-fixture-secret");
    expect(screen.queryByText("saved:environment_proxy_runtime")).toBeNull();
    input("查询公网 IP", "203.0.113.8");
    expect(screen.queryByText("saved:environment_asn")).toBeNull();
    input("期望语言（可选）", "ja-JP");
    expect(screen.queryByText("saved:environment_client_locale")).toBeNull();
    expect(screen.getByText("saved:environment_stun")).toBeTruthy();
    expect(mock.call).toHaveBeenCalledTimes(5);
  });

  it("a failed request releases busy and reports the error without automatic retry", async () => {
    const pending = deferred<ReturnType<typeof report>>();
    mock.call.mockReturnValueOnce(pending.promise);
    render(<EnvironmentDiagnostics />);
    fireEvent.click(action("检测 UDP 出口"));
    await act(async () => {
      pending.reject(new Error("fixture unavailable"));
    });
    expect(screen.getByRole("alert").textContent).toContain(
      "fixture unavailable",
    );
    expect(action("检测 UDP 出口").disabled).toBe(false);
    await waitFor(() => expect(mock.call).toHaveBeenCalledTimes(1));
    expect(screen.queryByText("已核实")).toBeNull();
  });
});
