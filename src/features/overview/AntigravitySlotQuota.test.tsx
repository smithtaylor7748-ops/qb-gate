// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import AntigravitySlotRow, { type SlotRowProps } from "./AntigravitySlotRow";
import type { AntigravityOnlineQuota } from "../../lib/generated/AntigravityOnlineQuota";

afterEach(cleanup);
const now = Date.parse("2026-11-09T12:00:00Z");
const noop = vi.fn();
const props: SlotRowProps = {
  slot: {
    id: "demo",
    label: "demo",
    active: false,
    ide_dir: "fixture",
    ide_logged_in: true,
    ide_auth_state: "已登录",
    ide_unreadable: false,
    email: null,
    tier: null,
    identity_error: null,
    quota: [],
    written_at: null,
    cli_dir: null,
    cli_logged_in: false,
    cli_auth_state: "未登录",
    cli_unreadable: false,
  },
  busy: false,
  ideInstalled: true,
  cliInstalled: false,
  now,
  attachFrom: null,
  online: null,
  onlineError: "",
  onlineLoading: false,
  onRefresh: noop,
  onSwitch: noop,
  onOpenIde: noop,
  onLoginCli: noop,
  onArchive: noop,
  onAttachStart: noop,
  onAttachHere: noop,
};
const quota: AntigravityOnlineQuota = {
  tier_id: null,
  tier_name: null,
  gcp_tos: false,
  credits: null,
  windows_note: null,
  models: [],
  fetched_at: "2026-11-09 12:00",
  windows: [
    {
      group: "gemini",
      span: "weekly",
      pool_id: "gemini",
      remaining: 0.2,
      remaining_implied: false,
      reset_epoch: now / 1000 + 120,
      reset_at: "12:02",
      note: null,
    },
    {
      group: "argon",
      span: "weekly",
      pool_id: "gemini-argon-wtu",
      remaining: 0.9,
      remaining_implied: false,
      reset_epoch: now / 1000 + 3600,
      reset_at: "13:00",
      note: null,
    },
  ],
};

it("Gemini 4 实际返回的池单独显示，网络失败不停止缓存倒计时", () => {
  const view = render(
    <AntigravitySlotRow {...props} online={quota} onlineError="DNS 查询失败" />,
  );
  expect(screen.getByText("Gemini 4")).toBeTruthy();
  expect(screen.getByText("20%")).toBeTruthy();
  expect(screen.getByText("90%")).toBeTruthy();
  expect(screen.getByText("2m 后重置")).toBeTruthy();
  expect(screen.getByText("刷新没成 · 保留上次读数")).toBeTruthy();
  view.rerender(
    <AntigravitySlotRow
      {...props}
      now={now + 120_000}
      online={quota}
      onlineError="DNS 查询失败"
    />,
  );
  expect(screen.getByText("到点 · 待刷新")).toBeTruthy();
  expect(screen.queryByText("20%")).toBeNull();
  expect(screen.getByText("90%")).toBeTruthy();
});

it("模型清单兜底逐项保留 Gemini 4 和其它 Gemini，缺数据不补零", () => {
  const models = [
    { label: "Gemini 4 Argon", model_key: "gemini-4-argon", remaining: 0.85 },
    { label: "Gemini Flash", model_key: "gemini-flash", remaining: 0.3 },
    { label: "Gemini 未知额度", model_key: "gemini-unknown", remaining: null },
  ].map((m) => ({
    ...m,
    model_id: 0,
    reset_epoch: null,
    reset_at: null,
    tags: [],
    remaining_implied: false,
  }));
  render(
    <AntigravitySlotRow
      {...props}
      online={{ ...quota, windows: [], models }}
    />,
  );
  expect(screen.getByText("Gemini 4 Argon")).toBeTruthy();
  expect(screen.getByText("Gemini Flash")).toBeTruthy();
  expect(screen.getByText("剩 85%")).toBeTruthy();
  expect(screen.getByText("剩 30%")).toBeTruthy();
  expect(screen.queryByText("剩 0%")).toBeNull();
  expect(screen.getByText("—")).toBeTruthy();
});
