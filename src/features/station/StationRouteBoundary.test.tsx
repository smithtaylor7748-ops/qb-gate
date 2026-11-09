// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { RouterRequest, RouterSettings } from "../../lib/station";
import {
  RouteAttemptTrail,
  StationRouteBoundary,
} from "./StationRouteBoundary";

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
    "未发往上游 · 线路超出允许范围",
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
  expect(text).toBe("免费站 · 被限流 · 已阻止范围外的后续尝试");
  expect(text).not.toContain("403");
  expect(text).not.toContain("付费站");
});

it("shows the saved selection boundary without implying retained pool approval is active", () => {
  const settings = {
    routing_mode: "selected",
    allowed_routes: ["free", "paid"],
  } as RouterSettings;
  render(
    <StationRouteBoundary
      settings={settings}
      selected="free"
      routeLabel={label}
    />,
  );
  expect(screen.getByText("仅所选线路")).toBeTruthy();
  expect(screen.getByText(/仅用「免费站」/)).toBeTruthy();
  expect(screen.queryByText("付费站")).toBeNull();
});

it("renders every approved route when pool mode is effective", () => {
  const settings = {
    routing_mode: "pool",
    allowed_routes: ["free", "paid"],
  } as RouterSettings;
  render(
    <StationRouteBoundary
      settings={settings}
      selected="free"
      routeLabel={label}
    />,
  );
  expect(screen.getByText("已允许 2 条线路")).toBeTruthy();
  expect(screen.getByText("免费站")).toBeTruthy();
  expect(screen.getByText("付费站")).toBeTruthy();
});

it("shows recorded route attempts and their failure reason instead of inferring from selected route", () => {
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

it("makes load distribution explicit even while the preferred route is healthy", () => {
  render(
    <StationRouteBoundary
      settings={
        {
          routing_mode: "pool",
          pool_strategy: "least-loaded",
          admission: { overflow_to_pool: true },
          allowed_routes: ["free", "paid"],
        } as RouterSettings
      }
      selected="free"
      routeLabel={label}
    />,
  );
  expect(screen.getByText(/首选正常也可能使用其他站/)).toBeTruthy();
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

it("reports overflow as off even if a stored load strategy is retained", () => {
  render(
    <StationRouteBoundary
      settings={
        {
          routing_mode: "pool",
          pool_strategy: "least-loaded",
          allowed_routes: ["free", "paid"],
          admission: { overflow_to_pool: false },
        } as RouterSettings
      }
      selected="free"
      routeLabel={label}
    />,
  );
  expect(screen.getByText(/容量溢出分流已关闭/)).toBeTruthy();
  expect(screen.queryByText(/首选正常也可能使用其他站/)).toBeNull();
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
