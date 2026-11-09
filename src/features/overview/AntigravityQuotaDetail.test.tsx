// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import AntigravityQuotaDetail from "./AntigravityQuotaDetail";
import type { AntigravityOnlineQuota } from "../../lib/generated/AntigravityOnlineQuota";
import type { AntigravityIdentity } from "../../lib/generated/AntigravityIdentity";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));
const now = Date.parse("2026-11-09T12:00:00Z");
const online: AntigravityOnlineQuota = {
  tier_id: null,
  tier_name: null,
  gcp_tos: false,
  credits: null,
  fetched_at: "2026-11-09 11:50",
  models: [],
  windows_note: null,
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
    {
      group: "gemini",
      span: "weekly",
      pool_id: "gemini-separate",
      pool_label: "Separate pool",
      remaining: 0.7,
      remaining_implied: false,
      reset_epoch: null,
      reset_at: null,
      note: null,
    },
  ],
};
const local: AntigravityIdentity = {
  name: null,
  email: "demo@example.com",
  tier_id: null,
  tier_name: null,
  written_at: "2026-11-09 11:00",
  models: [
    {
      label: "Gemini 4 Argon",
      model_id: 417,
      remaining: 0.6,
      remaining_implied: false,
      reset_epoch: now / 1000 + 60,
      reset_at: "12:01",
      tags: [],
    },
  ],
};
beforeEach(() => {
  mock.call.mockReset();
  vi.useFakeTimers();
  vi.setSystemTime(now);
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

it("只读激活账户缓存并分池显示，不把同为 Gemini 的两个周窗口合并", async () => {
  mock.call.mockResolvedValue(online);
  render(<AntigravityQuotaDetail accountId="a" identity={local} />);
  await act(async () => {});
  expect(mock.call).toHaveBeenCalledExactlyOnceWith(
    "antigravity_account_quota",
    { id: "a", refresh: false },
  );
  expect(
    within(screen.getByLabelText("配额池 gemini")).getByText("剩 20%"),
  ).toBeTruthy();
  expect(
    within(screen.getByLabelText("配额池 gemini-argon-wtu")).getByText(
      "剩 90%",
    ),
  ).toBeTruthy();
  expect(screen.getByText("Separate pool · gemini-separate")).toBeTruthy();
  expect(screen.getByText(/上次成功的在线额度缓存/)).toBeTruthy();
  act(() => vi.advanceTimersByTime(120_000));
  expect(screen.queryByText("剩 20%")).toBeNull();
  expect(screen.getByText("到点 · 待刷新")).toBeTruthy();
  expect(screen.getByText("剩 90%")).toBeTruthy();
  expect(mock.call).toHaveBeenCalledTimes(1);
});

it("没有在线缓存时明确展示本地记录，到点撤下旧比例", async () => {
  mock.call.mockResolvedValue(null);
  render(<AntigravityQuotaDetail accountId="a" identity={local} />);
  await act(async () => {});
  expect(screen.getByText(/没有已缓存的在线配额池/)).toBeTruthy();
  expect(screen.getByText(/本地模型记录/)).toBeTruthy();
  expect(screen.getByText("剩 60%")).toBeTruthy();
  act(() => vi.advanceTimersByTime(60_000));
  expect(screen.queryByText("剩 60%")).toBeNull();
  expect(screen.getByText("到点 · 待刷新")).toBeTruthy();
  expect(mock.call).toHaveBeenCalledTimes(1);
});

it("换账户后晚到的旧缓存不能覆盖当前账户", async () => {
  let finishOld!: (value: AntigravityOnlineQuota) => void;
  mock.call.mockImplementation((_command: string, args: { id: string }) =>
    args.id === "a"
      ? new Promise((resolve) => {
          finishOld = resolve;
        })
      : Promise.resolve(null),
  );
  const view = render(
    <AntigravityQuotaDetail accountId="a" identity={local} />,
  );
  view.rerender(<AntigravityQuotaDetail accountId="b" identity={null} />);
  await act(async () => {});
  await act(async () => {
    finishOld(online);
  });
  expect(screen.queryByLabelText("配额池 gemini")).toBeNull();
  expect(screen.getByText(/本地 IDE 也还没有额度记录/)).toBeTruthy();
  expect(mock.call.mock.calls.every(([, args]) => args.refresh === false)).toBe(
    true,
  );
});

it("缓存读取失败不伪装成没有数据，保留本地来源说明", async () => {
  mock.call.mockRejectedValue(new Error("缓存文件读不出来"));
  render(<AntigravityQuotaDetail accountId="a" identity={local} />);
  await act(async () => {});
  expect(
    screen.getByText(/在线额度缓存读不出来：缓存文件读不出来/),
  ).toBeTruthy();
  expect(screen.getByText("剩 60%")).toBeTruthy();
});
