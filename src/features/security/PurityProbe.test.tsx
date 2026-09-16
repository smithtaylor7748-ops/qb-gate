// @vitest-environment jsdom
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { peek, put, queryClient, setSession } from "../../lib/store";
import { PurityProbe } from "./PurityPanel";

const mock = vi.hoisted(() => ({ lookup: vi.fn(), current: vi.fn() }));
vi.mock("./useChecks", () => ({
  useChecks: () => ({
    purity: {
      running: false,
      done: false,
      error: undefined,
      run: mock.current,
    },
  }),
  judgePurity: vi.fn(),
}));
vi.mock("../../lib/api", () => ({
  api: { lookupIp: (ip: string) => mock.lookup(ip) },
}));
vi.mock("../../lib/resources", async () => {
  const { res } = await import("../../lib/store");
  return {
    R: {
      ip: res(async () => null, { auto: false }),
      purity: res(async () => null, { auto: false }),
      criteria: res(async () => ({
        iproyal: "https://iproyal.cn/?r=sulianyan",
        optional: [],
      })),
    },
  };
});
beforeEach(() => {
  mock.lookup.mockReset();
  mock.current.mockReset();
  setSession("purity.lookup.input", "");
  setSession("purity.lookup.busy", false);
  setSession("purity.lookup.error", "");
  setSession("purity.lookup.result", null);
});

it("leaving the inline input empty runs the existing current-exit check", async () => {
  mount();
  fireEvent.click(screen.getByRole("button", { name: "检测本机 IP" }));
  expect(mock.current).toHaveBeenCalledOnce();
  expect(mock.lookup).not.toHaveBeenCalled();
  expect(
    (screen.getByRole("button", { name: "查询指定 IP" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
});
afterEach(() => {
  cleanup();
  queryClient.clear();
});
const mount = () =>
  render(
    <QueryClientProvider client={queryClient}>
      <PurityProbe />
    </QueryClientProvider>,
  );

it("queries the supplied address while preserving current exit and purity results", async () => {
  mock.lookup.mockResolvedValue({
    ip: "8.8.8.8",
    source: "IPQuery",
    checked_at: "2026-09-16T12:00:00Z",
    risk_score: null,
    is_vpn: null,
    is_tor: null,
    is_proxy: null,
    is_datacenter: null,
  });
  put("ip", { ip: "203.0.113.7", isResidential: true });
  put("purity", { passed: false });
  mount();
  const link = await screen.findByRole("link", { name: /IPRoyal 选购/ });
  expect(link.getAttribute("href")).toBe("https://iproyal.cn/?r=sulianyan");
  fireEvent.change(screen.getByLabelText("新 IP 地址"), {
    target: { value: " 8.8.8.8 " },
  });
  fireEvent.click(screen.getByRole("button", { name: "查询指定 IP" }));
  expect(await screen.findByTestId("ip-lookup-result")).toBeTruthy();
  expect(mock.lookup).toHaveBeenCalledWith("8.8.8.8");
  expect(screen.getByTestId("current-ip-result").textContent).toContain(
    "203.0.113.7",
  );
  expect(screen.getByTestId("current-ip-result").textContent).toContain(
    "住宅 IP",
  );
  fireEvent.click(screen.getByRole("button", { name: "检测本机 IP" }));
  expect(mock.current).toHaveBeenCalledOnce();
  expect(mock.lookup).toHaveBeenCalledOnce();
  expect(screen.getByText("代理：未知")).toBeTruthy();
  expect(screen.getByText("机房：未知")).toBeTruthy();
  expect(peek("ip")).toEqual({ ip: "203.0.113.7", isResidential: true });
  expect(peek("purity")).toEqual({ passed: false });
});

it("keeps an in-flight lookup across reopening and shows failures without old results", async () => {
  let fail!: (reason: Error) => void;
  mock.lookup.mockImplementation(
    () =>
      new Promise((_, reject) => {
        fail = reject;
      }),
  );
  const first = mount();
  fireEvent.change(screen.getByLabelText("新 IP 地址"), {
    target: { value: "8.8.8.8" },
  });
  fireEvent.click(screen.getByRole("button", { name: "查询指定 IP" }));
  first.unmount();
  mount();
  expect(
    (
      screen.getByRole("button", {
        name: "查询指定 IP",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  await act(async () => {
    fail(new Error("查询服务暂时不可用"));
  });
  await waitFor(() =>
    expect(screen.getByRole("alert").textContent).toContain(
      "查询服务暂时不可用",
    ),
  );
  expect(screen.queryByTestId("ip-lookup-result")).toBeNull();
  expect(mock.lookup).toHaveBeenCalledOnce();
  expect(
    (
      screen.getByRole("button", {
        name: "查询指定 IP",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(false);
});
