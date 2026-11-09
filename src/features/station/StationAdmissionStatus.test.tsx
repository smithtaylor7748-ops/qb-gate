// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { RouterSettings } from "../../lib/generated/RouterSettings";
import type { RouterStatus } from "../../lib/generated/RouterStatus";
import { StationAdmissionStatus } from "./StationAdmissionStatus";

afterEach(cleanup);
const settings = {
  routing_mode: "selected",
  allowed_routes: ["paid"],
  pool_strategy: "least-loaded",
  admission: { route_limits: {}, max_wait_ms: 0, max_waiters: 0 },
} as unknown as RouterSettings;
const status = {
  current_route: "free",
  pending_route: null,
  admission: [],
} as unknown as RouterStatus;
const label = (id: string) => (id === "free" ? "免费站" : "付费站");

it("does not turn absent metrics into zero usage or retained pool settings into active balancing", () => {
  render(
    <StationAdmissionStatus
      settings={settings}
      status={status}
      routeLabel={label}
    />,
  );
  expect(screen.getByText("优先所选线路")).toBeTruthy();
  expect(screen.getByText("不排队 · 容量不足时返回本机繁忙")).toBeTruthy();
  expect(screen.getAllByText("未知")).toHaveLength(3);
  expect(screen.getAllByText("上限 未设置")).toHaveLength(2);
  expect(screen.queryByText("付费站")).toBeNull();
  expect(screen.queryByText("0")).toBeNull();
});

it("uses backend quota state without deriving freshness from the browser clock", () => {
  render(
    <StationAdmissionStatus
      settings={{
        ...settings,
        admission: {
          ...settings.admission,
          route_limits: {
            free: { max_concurrent: 2, rpm: 30, check_quota: true },
          },
        },
      }}
      status={{
        ...status,
        admission: [
          {
            route_id: "free",
            active: 2,
            waiting: 1,
            rpm_used: 8,
            reason: "concurrency",
            retry_at_ms: null,
            quota_checked_at_ms: 1n,
            quota_state: "available",
          },
        ],
      }}
      routeLabel={label}
    />,
  );
  expect(screen.getByText("本机暂缓发送：同时进行的请求已达上限")).toBeTruthy();
  expect(screen.getByText(/已查询的额度状态可用/)).toBeTruthy();
  expect(screen.queryByText(/额度缓存已过期/)).toBeNull();
  expect(screen.getByText("上限 2")).toBeTruthy();
});

it.each(["unknown", "stale"] as const)(
  "does not claim exhausted quota for %s state",
  (quota_state) => {
    render(
      <StationAdmissionStatus
        settings={settings}
        status={{
          ...status,
          admission: [
            {
              route_id: "free",
              active: 0,
              waiting: 0,
              rpm_used: 0,
              reason: null,
              retry_at_ms: null,
              quota_checked_at_ms: null,
              quota_state,
            },
          ],
        }}
        routeLabel={label}
      />,
    );
    expect(screen.getByText(/不据此拦截/)).toBeTruthy();
    expect(screen.queryByText("上游明确额度不可用")).toBeNull();
    expect(screen.queryByText("暂不可接收请求")).toBeNull();
  },
);

it("labels refresh errors as stale snapshots and allows an explicit status refresh", () => {
  const refresh = vi.fn();
  render(
    <StationAdmissionStatus
      settings={settings}
      status={status}
      routeLabel={label}
      error="暂时断开"
      onRefresh={refresh}
    />,
  );
  expect(screen.getByRole("alert").textContent).toContain("下方保留上次快照");
  fireEvent.click(screen.getByRole("button", { name: "刷新状态" }));
  expect(refresh).toHaveBeenCalledOnce();
});
