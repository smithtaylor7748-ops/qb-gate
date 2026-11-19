// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { RouterRequest, SelectionView } from "../../lib/station";
import { RecentRoute, RouteAttemptTrail } from "./RecentRoute";

afterEach(cleanup);
const label = (id: string) => ({ free: "免费站", paid: "付费站" })[id] ?? id;

it("does not describe a permission denial as a paid upstream request", () => {
  const request = {
    route_id: "paid",
    fault: "route-permission-denied",
    attempts: [],
  } as unknown as RouterRequest;
  render(<RouteAttemptTrail request={request} routeLabel={label} />);
  expect(screen.getByLabelText("最近请求尝试路径").textContent).toBe(
    "未发往上游 · 不是当前线路或备用线路",
  );
  expect(screen.queryByText(/付费站/)).toBeNull();
});

it("retains the last real attempt when permission blocks the next route", () => {
  const request = {
    route_id: "free",
    status: 403,
    fault: "route-permission-denied",
    attempts: [{ route_id: "free", status: 429, fault: "rate-limit" }],
  } as RouterRequest;
  render(<RouteAttemptTrail request={request} routeLabel={label} />);
  const text = screen.getByLabelText("最近请求尝试路径").textContent;
  expect(text).toBe("免费站 · 被限流 · 已阻止名单外的后续尝试");
  expect(text).not.toContain("403");
  expect(text).not.toContain("付费站");
});

it("shows recorded route attempts and their failure reason instead of inferring from the current line", () => {
  const request = {
    route_id: "paid",
    fault: null,
    attempts: [
      { route_id: "free", status: null, fault: "timeout" },
      { route_id: "paid", status: 200, fault: null },
    ],
  } as RouterRequest;
  render(<RouteAttemptTrail request={request} routeLabel={label} />);
  expect(screen.getByLabelText("最近请求尝试路径").textContent).toBe(
    "免费站 · 超时 → 付费站 · 成功",
  );
});

it("does not label a successful response header as success after the stream fails", () => {
  const request = {
    route_id: "free",
    fault: "timeout",
    attempts: [{ route_id: "free", status: 200, fault: null }],
  } as RouterRequest;
  render(<RouteAttemptTrail request={request} routeLabel={label} />);
  expect(screen.getByLabelText("最近请求尝试路径").textContent).toBe(
    "免费站 · 超时",
  );
});

it("does not attribute local admission refusal to an upstream route", () => {
  render(
    <RouteAttemptTrail
      request={
        {
          route_id: "paid",
          fault: "route-admission-denied",
          attempts: [],
        } as unknown as RouterRequest
      }
      routeLabel={label}
    />,
  );
  expect(screen.getByText("未发往上游 · 本机请求容量限制")).toBeTruthy();
  expect(screen.queryByText(/付费站/)).toBeNull();
});

it("retains actual failure before a later local capacity refusal", () => {
  render(
    <RouteAttemptTrail
      request={
        {
          route_id: "free",
          fault: "route-admission-denied",
          attempts: [{ route_id: "free", status: 429, fault: "rate-limit" }],
        } as RouterRequest
      }
      routeLabel={label}
    />,
  );
  expect(screen.getByLabelText("最近请求尝试路径").textContent).toBe(
    "免费站 · 被限流 · 后续尝试受本机请求容量限制",
  );
});

// ------------------------------------------------------------------ 「最近请求实际去向」兼做提示行

const base: SelectionView = {
  current: "paid",
  backups: ["free"],
  auto_select: false,
  plan: {
    current: "paid",
    steps: [],
    first: "paid",
    max_attempts: 2,
    strict_single: false,
  },
  paused: null,
  aggregate: null,
  pricier_backups: [],
  problem: null,
};
const latest = {
  route_id: "paid",
  at_ms: 1_757_820_000_000n,
  fault: null,
  attempts: [{ route_id: "paid", status: 200, fault: null }],
} as unknown as RouterRequest;

it("normally shows where the latest request went", () => {
  render(
    <RecentRoute
      selection={base}
      latest={latest}
      error={null}
      routeLabel={label}
      onRetry={() => {}}
    />,
  );
  expect(screen.getByRole("status").textContent).toContain("最近请求实际去向");
  expect(screen.getByLabelText("最近请求尝试路径").textContent).toBe(
    "付费站 · 成功",
  );
});

it("turns into a warning with a retry for a tripped breaker on the current line", () => {
  const onRetry = vi.fn();
  render(
    <RecentRoute
      selection={{
        ...base,
        paused: {
          route_id: "paid",
          reason: "breaker-rejected",
          until_ms: null,
          using: "free",
        },
      }}
      latest={latest}
      error={null}
      routeLabel={label}
      onRetry={onRetry}
    />,
  );
  const line = screen.getByRole("alert");
  expect(line.textContent).toContain("当前线路「付费站」暂停中");
  expect(line.textContent).toContain("这期间改用备用线路「免费站」");
  fireEvent.click(screen.getByRole("button", { name: "重试主线" }));
  expect(onRetry).toHaveBeenCalledWith("paid");
});

it("offers no retry for rate limits and says requests fail without a backup", () => {
  render(
    <RecentRoute
      selection={{
        ...base,
        backups: [],
        paused: {
          route_id: "paid",
          reason: "rate-limited",
          until_ms: null,
          using: null,
        },
      }}
      latest={latest}
      error={null}
      routeLabel={label}
      onRetry={() => {}}
    />,
  );
  expect(screen.getByRole("alert").textContent).toContain(
    "没有能用的备用线路，这期间请求会直接报错",
  );
  expect(screen.queryByRole("button", { name: "重试主线" })).toBeNull();
});

it("warns that an aggregated model went straight to its own line", () => {
  render(
    <RecentRoute
      selection={{
        ...base,
        aggregate: {
          route_id: "free",
          model: "qbg-free/gpt-x",
          at_ms: 1_757_820_000_000n,
          latest: true,
        },
      }}
      latest={latest}
      error={null}
      routeLabel={label}
      onRetry={() => {}}
    />,
  );
  const line = screen.getByRole("alert").textContent ?? "";
  expect(line).toContain("客户端选了聚合模型「qbg-free/gpt-x」");
  expect(line).toContain("最近一发直达「免费站」");
});
