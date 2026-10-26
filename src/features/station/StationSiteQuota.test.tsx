// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import StationSiteQuota, { money, quotaLine } from "./StationSiteQuota";
import type { SiteQuota } from "../../lib/generated/SiteQuota";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));

const plan: SiteQuota = {
  provider: "Kimi For Coding",
  kind: "plan",
  balances: [],
  windows: [
    { key: "five_hour", label: "5 小时", used_percent: 35, resets_at: null },
    {
      key: "weekly",
      label: "每周",
      used_percent: 82.4,
      resets_at: "2026-10-30T00:00:00Z",
    },
  ],
  plan: null,
  warning: null,
  fetched_at: "2026-10-26T08:00:00Z",
  route_id: "r1",
};

beforeEach(() => mock.call.mockReset());
afterEach(cleanup);

it("says the site gives nothing when it is not a provider it knows, and asks without forcing", async () => {
  mock.call.mockResolvedValue(null);
  render(<StationSiteQuota routeId="r1" group="默认" />);
  expect(await screen.findByText("余额（整站共享）站点未提供")).toBeTruthy();
  expect(mock.call).toHaveBeenCalledTimes(1);
  expect(mock.call).toHaveBeenCalledWith("station_site_quota", {
    routeId: "r1",
    force: false,
  });
});

it("shows what is left per window, forces a new ask on the icon, and keeps the last answer when it fails", async () => {
  mock.call.mockResolvedValueOnce(plan);
  render(<StationSiteQuota routeId="r1" group="编程组" />);
  const line = await screen.findByText("5 小时 剩 65% · 每周 剩 18%");
  expect(line.className).toBe("qb-tone-danger");
  const box = screen.getByTestId("site-quota");
  expect(box.getAttribute("title")).toContain("按「编程组」那把 Key 问的");

  mock.call.mockRejectedValueOnce(
    new Error("Kimi For Coding 拒绝了这把 Key（HTTP 401）"),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "再问一次这家站的余额或套餐额度" }),
  );
  await waitFor(() =>
    expect(mock.call).toHaveBeenLastCalledWith("station_site_quota", {
      routeId: "r1",
      force: true,
    }),
  );
  expect(await screen.findByText(/刷新没成/)).toBeTruthy();
  // 上一次问到的还在，不被一次失败抹成 0。
  expect(screen.getByText("5 小时 剩 65% · 每周 剩 18%")).toBeTruthy();
  expect(screen.getByTestId("site-quota").getAttribute("title")).toContain(
    "HTTP 401",
  );
});

it("formats balances without hiding an overdraft", () => {
  expect(money(-5, "USD")).toBe("-$5.00");
  expect(money(12.3, "CNY")).toBe("12.30 元");
  const q: SiteQuota = {
    ...plan,
    provider: "DeepSeek",
    kind: "balance",
    windows: [],
    balances: [{ remaining: 0, total: null, used: null, unit: "CNY" }],
  };
  expect(quotaLine(q)).toEqual({ text: "余额 0.00 元", tone: "danger" });
});
