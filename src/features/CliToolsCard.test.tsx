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
import { queryClient } from "../lib/store";
import { ToastProvider } from "../ui";
import CliToolsCard from "./CliToolsCard";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../lib/ipc", () => ({ call: mock.call }));

const npm = "C:\\Program Files\\nodejs\\npm.cmd";
const report = {
  npm,
  tools: [
    {
      tool: "openclaw",
      label: "OpenClaw",
      command: "openclaw",
      installs: [
        {
          path: "C:\\Users\\demo\\AppData\\Roaming\\npm\\openclaw.cmd",
          source: "npm",
          version: "2026.9.1",
          package_dir: null,
          default: true,
        },
      ],
      conflict: false,
      npm_package: "openclaw",
      script_url: null,
      install: { kind: "run", shown: `& "${npm}" i -g openclaw@latest` },
      update: { kind: "run", shown: `& "${npm}" i -g openclaw@latest` },
    },
    {
      tool: "hermes",
      label: "Hermes",
      command: "hermes",
      installs: [],
      conflict: false,
      npm_package: null,
      script_url:
        "https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.ps1",
      install: {
        kind: "manual",
        shown: "Hermes 没有 npm 包，用官方安装脚本装。",
      },
      update: null,
    },
  ],
};
const script = {
  id: "hermes-20261027-120000-000",
  tool: "hermes",
  purpose: "install",
  url: "https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.ps1",
  final_url:
    "https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.ps1",
  sha256: "ab".repeat(32),
  size: 42,
  env: [],
  text: "Write-Host 'install hermes'",
  truncated: false,
  bom_added: true,
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
  Object.assign(navigator, {
    clipboard: { writeText: vi.fn(async () => undefined) },
  });
  mock.call.mockReset();
  mock.call.mockImplementation(async (command: string) => {
    if (command === "cli_tools_status") return report;
    if (command === "cli_tools_script_fetch") return script;
    if (command === "cli_tools_script_run") return "Hermes 的官方脚本跑完了";
    if (command === "cli_tools_latest")
      return {
        tool: "openclaw",
        latest: "2026.10.2",
        from: "npm",
        outdated: true,
        node_required: ">=24.16.0 <25 || >=26.1.0",
        node_current: "22.11.0",
        node_ok: false,
        problems: [],
      };
    return null;
  });
});
afterEach(() => {
  cleanup();
  queryClient.clear();
});

function mount() {
  render(
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
        <CliToolsCard />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

it("only runs the official script after it was shown and ticked, and runs exactly that copy", async () => {
  mount();
  fireEvent.click(await screen.findByRole("button", { name: "用官方脚本装…" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("cli_tools_script_fetch", {
      tool: "hermes",
      purpose: "install",
    }),
  );
  expect(await screen.findByText("Write-Host 'install hermes'")).toBeTruthy();
  const go = screen.getByRole("button", { name: "我看过了，运行" });
  expect((go as HTMLButtonElement).disabled).toBe(true);
  expect(mock.call).not.toHaveBeenCalledWith(
    "cli_tools_script_run",
    expect.anything(),
  );
  fireEvent.click(
    screen.getByRole("checkbox", {
      name: "我看过了，确认这是从上面那个地址下来的官方脚本",
    }),
  );
  fireEvent.click(screen.getByRole("button", { name: "我看过了，运行" }));
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("cli_tools_script_run", {
      id: script.id,
      sha256: script.sha256,
    }),
  );
});

it("checking for a new version that needs a newer Node keeps the update button off", async () => {
  mount();
  fireEvent.click(await screen.findByRole("button", { name: "检查新版本" }));
  expect(await screen.findByText("可升级 → 2026.10.2")).toBeTruthy();
  expect(screen.getByText(/新版要求 Node/)).toBeTruthy();
  const up = screen.getByRole("button", { name: "升级" });
  expect((up as HTMLButtonElement).disabled).toBe(true);
});
