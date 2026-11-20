// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { queryClient } from "../../lib/store";
import { ToastProvider } from "../../ui";
import BrowserCompanionPage from "./BrowserCompanionPage";
import { DEFAULT_PROTECTED_SITES } from "./where";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));

const protection = (allow: boolean, armed: boolean, reason: string) => ({
  allow,
  code: allow ? "allowed" : "disconnected",
  reason,
  armed,
  checking: false,
  ip: null,
  baseline: null,
  checked_at: 0,
});
const pairing = (
  id: string,
  label: string,
  connected: boolean,
  needs_relink = false,
) => ({
  pairing_id: id,
  label,
  extension_id: "dffmajnemniojfblgfhenndeeelbjgoh",
  extension_version: "0.2.0",
  paired_at: 0,
  connected,
  needs_relink,
  protection: protection(connected, true, `${label} 的原因`),
  enhancement_json: null,
});
let pairings = [
  pairing("a", "Chrome 158", true),
  pairing("b", "Edge 157", false),
  pairing("c", "Chrome 工作资料", false, true),
];
const status = () => ({
  listening: pairings.length > 0,
  port: 53124,
  relink_needed: pairings.some((p) => p.needs_relink),
  pairings,
  last_check: null,
  claim: null,
  problem: null,
});

beforeEach(() => {
  queryClient.clear();
  pairings = [
    pairing("a", "Chrome 158", true),
    pairing("b", "Edge 157", false),
    pairing("c", "Chrome 工作资料", false, true),
  ];
  mock.call.mockReset();
  mock.call.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "browser_companion_status") return status();
      if (cmd === "browser_companion_folder")
        return {
          path: "C:\\QB Gate\\browser-extension",
          source: "installed",
          version: "0.2.0",
          problem: null,
          searched: [],
        };
      if (cmd === "browser_companion_open_folder")
        throw "没找到插件文件夹。重新运行一次 QB Gate 安装包";
      if (cmd === "browser_companion_reconnect")
        return {
          url: "http://127.0.0.1:53125/link#c=abc",
          opened: false,
          port: 53125,
          expires_at: 0,
          rebound: false,
        };
      if (cmd === "browser_companion_unpair") {
        pairings = pairings.filter((p) => p.pairing_id !== args?.pairingId);
        return status();
      }
      throw new Error("unexpected " + cmd);
    },
  );
});
afterEach(cleanup);

function show() {
  render(
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
        <MemoryRouter>
          <BrowserCompanionPage />
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

it("uses the exact words the extension points people to", async () => {
  show();
  // 插件里写死了这几个叫法（core.js、welcome.html、README）：页面上的按钮必须一字不差。
  await screen.findByRole("button", { name: "打开插件文件夹" });
  screen.getByRole("button", { name: "复制路径" });
  screen.getByRole("button", { name: "连接浏览器" });
  await screen.findByRole("button", { name: "一键重连" });
  screen.getByRole("heading", { name: "浏览器助手" });
  const plugin = ["core.js", "welcome.html", "README.md"]
    .map((f) =>
      readFileSync(resolve(__dirname, "../../../browser-extension", f), "utf8"),
    )
    .join("\n");
  for (const word of [
    "扩展 → 浏览器助手",
    "连接浏览器",
    "一键重连",
    "打开插件文件夹",
  ])
    expect(plugin).toContain(word);
});

it("lists every paired browser with its link and protection state", async () => {
  show();
  await screen.findByText("Chrome 158");
  expect(screen.getAllByTestId("companion-pairing")).toHaveLength(3);
  screen.getByText("已连接");
  screen.getByText("没连上");
  screen.getByText("要一键重连");
  screen.getByText("保护开着 · 放行中");
  expect(screen.getAllByText("保护开着 · 拦着")).toHaveLength(2);
  screen.getByText("C:\\QB Gate\\browser-extension", { exact: false });
});

it("unpairs only after confirming in place", async () => {
  show();
  await screen.findByText("Edge 157");
  fireEvent.click(screen.getByRole("button", { name: "解除 Edge 157 的配对" }));
  expect(
    mock.call.mock.calls.some((c) => c[0] === "browser_companion_unpair"),
  ).toBe(false);
  screen.getByText(/解除之后 Edge 157 里的插件立刻断开/);
  fireEvent.click(screen.getByRole("button", { name: "解除配对" }));
  await waitFor(() => expect(screen.queryByText("Edge 157")).toBeNull());
  expect(mock.call).toHaveBeenCalledWith("browser_companion_unpair", {
    pairingId: "b",
  });
});

it("shows the claim address when the browser could not be opened", async () => {
  show();
  fireEvent.click(await screen.findByRole("button", { name: "一键重连" }));
  await screen.findByText("http://127.0.0.1:53125/link#c=abc");
  screen.getByText(/没能替你打开浏览器/);
});

it("says what to do when the extension folder is missing", async () => {
  show();
  fireEvent.click(
    await screen.findByRole("button", { name: "打开插件文件夹" }),
  );
  await screen.findByText(/没找到插件文件夹/);
});

it("names the same default protected sites as the extension", () => {
  // 插件是不经打包的纯 JS，这里直接读它的源码：两边的默认名单必须一样。
  const core = readFileSync(
    resolve(__dirname, "../../../browser-extension/core.js"),
    "utf8",
  );
  const block = core.match(/DEFAULT_SITES = Object\.freeze\(\[([^\]]*)\]/);
  const sites = [...(block?.[1] ?? "").matchAll(/"([^"]+)"/g)].map((m) => m[1]);
  expect(sites.length).toBeGreaterThan(0);
  expect(DEFAULT_PROTECTED_SITES).toEqual(sites);
});
