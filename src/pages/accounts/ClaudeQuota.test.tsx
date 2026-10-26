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
import { queryClient } from "../../lib/store";
import type { ClaudeQuota, Slot } from "../../lib/api";
import SlotRow from "../../features/overview/SlotRow";
import { OfficialQuota } from "./AccountDetail";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));

const base: Slot = {
  label: "main",
  active: true,
  logged_in: true,
  cli_days_left: 20,
  account_uuid: null,
  email: "demo@example.com",
  org_name: null,
  expires_at: null,
  dir: "C:\\Users\\demo\\slot",
  desktop_dir: null,
  plan: "Claude Pro",
  billing: null,
  plan_fetched_at: null,
  desktop_profile: false,
  usage: null,
  login_problem: null,
};

const later = (ms: number) => new Date(Date.now() + ms).toISOString();
const quota: ClaudeQuota = {
  label: "main",
  windows: [
    {
      key: "five_hour",
      label: "5 小时",
      used_percent: 41,
      resets_at: later(3_600_000),
    },
    {
      key: "seven_day",
      label: "7 天",
      used_percent: 58,
      resets_at: later(86_400_000),
    },
    {
      key: "seven_day_opus",
      label: "7 天 · Opus",
      used_percent: 73,
      resets_at: null,
    },
  ],
  extra: {
    enabled: true,
    monthly_limit: 50,
    used_credits: 6.4,
    utilization: 12.8,
    currency: "USD",
  },
  fetched_at: new Date().toISOString(),
  source: "https://api.anthropic.com/api/oauth/usage",
};

beforeEach(() => {
  mock.call.mockReset();
});
afterEach(() => {
  cleanup();
  queryClient.clear();
});

function wrap(node: React.ReactNode) {
  return render(
    <QueryClientProvider client={queryClient}>{node}</QueryClientProvider>,
  );
}

it("says the server rejected the login apart from never logged in, and labels the official reading", () => {
  const refresh = vi.fn();
  wrap(
    <SlotRow
      slot={{
        ...base,
        logged_in: false,
        login_problem: "登录已失效：服务端拒绝了这份登录（HTTP 401）",
        usage: {
          source: "official",
          measured_at: new Date().toISOString(),
          age_minutes: 0,
          five_hour: {
            used: 41,
            resets_at: null,
            estimated: false,
            reset_passed: false,
          },
          seven_day: null,
        },
      }}
      onRefresh={refresh}
    />,
  );
  const pill = screen.getByText("登录已失效");
  expect(pill.closest("[title]")?.getAttribute("title")).toContain("HTTP 401");
  expect(screen.queryByText("未登录")).toBeNull();
  expect(screen.getByText(/官方接口 · 刚刚/)).toBeTruthy();
  fireEvent.click(
    screen.getByRole("button", { name: "联网刷新这个账户的额度" }),
  );
  expect(refresh).toHaveBeenCalledTimes(1);
});

it("opening the detail only reads the last answer; asking goes out once and shows every window", async () => {
  mock.call.mockImplementation(
    async (command: string, args?: { refresh?: boolean }) => {
      if (command === "claude_quota") return args?.refresh ? quota : null;
      if (command === "accounts_list") return { slots: [base] };
    },
  );
  wrap(<OfficialQuota slot={base} />);
  await waitFor(() =>
    expect(mock.call).toHaveBeenCalledWith("claude_quota", {
      label: "main",
      refresh: false,
    }),
  );
  // 打开不联网：没问过就说没问过，不画假数字。
  expect(await screen.findByText(/还没问过/)).toBeTruthy();
  expect(mock.call).not.toHaveBeenCalledWith("claude_quota", {
    label: "main",
    refresh: true,
  });

  const listed = () =>
    mock.call.mock.calls.filter(([c]) => c === "accounts_list").length;
  await waitFor(() => expect(listed()).toBeGreaterThan(0));
  const before = listed();

  fireEvent.click(screen.getByRole("button", { name: "向官方问一次" }));
  expect(await screen.findByText("7 天 · Opus")).toBeTruthy();
  expect(mock.call).toHaveBeenCalledWith("claude_quota", {
    label: "main",
    refresh: true,
  });
  expect(screen.getByText("超额用量：已用 $6.40 / 月上限 $50.00")).toBeTruthy();
  // 问完重读账户列表：横条要换成「官方接口」那份。
  await waitFor(() => expect(listed()).toBeGreaterThan(before));
});

it("shows why it did not ask, and cannot ask for a slot that never logged in", async () => {
  mock.call.mockImplementation(
    async (command: string, args?: { refresh?: boolean }) => {
      if (command === "claude_quota") {
        if (!args?.refresh) return null;
        throw new Error(
          "Claude 槽位「main」本机这份访问令牌已经到点了，没有发请求",
        );
      }
      if (command === "accounts_list") return { slots: [base] };
    },
  );
  wrap(<OfficialQuota slot={base} />);
  fireEvent.click(await screen.findByRole("button", { name: "向官方问一次" }));
  expect(await screen.findByText(/已经到点了，没有发请求/)).toBeTruthy();

  cleanup();
  wrap(<OfficialQuota slot={{ ...base, label: "empty", logged_in: false }} />);
  const button = (await screen.findByRole("button", {
    name: "向官方问一次",
  })) as HTMLButtonElement;
  expect(button.disabled).toBe(true);
});
