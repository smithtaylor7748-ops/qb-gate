// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import SlotUsageBars from "./SlotUsage";
import CodexQuotaBars, { type CodexQuotaState } from "./CodexQuotaBars";
import { quotaCountdown, resetEpoch } from "../../lib/quotaClock";

const now = Date.parse("2026-11-09T12:00:00Z");
beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(now);
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

it("缓存中的 Claude 时间自行倒计时，到点后不伪造恢复额度", () => {
  render(
    <SlotUsageBars
      usage={{
        source: "official",
        measured_at: new Date(now).toISOString(),
        age_minutes: 0,
        five_hour: {
          used: 81,
          resets_at: new Date(now + 120_000).toISOString(),
          reset_passed: false,
          estimated: false,
        },
        seven_day: {
          used: 25,
          resets_at: new Date(now + 2 * 86_400_000).toISOString(),
          reset_passed: false,
          estimated: false,
        },
      }}
    />,
  );
  expect(screen.getByText("2m 后重置")).toBeTruthy();
  expect(screen.getByText("2d0h 后重置")).toBeTruthy();
  act(() => vi.advanceTimersByTime(60_000));
  expect(screen.getByText("1m 后重置")).toBeTruthy();
  act(() => vi.advanceTimersByTime(60_000));
  expect(screen.getByText("已到重置时间 · 待刷新确认额度")).toBeTruthy();
  expect(screen.queryByText("剩 19%")).toBeNull();
  expect(screen.queryByText("剩 100%")).toBeNull();
});

it("GPT 访问令牌过期仍按上次成功的时间倒计时，并保留缓存标记", () => {
  const state: CodexQuotaState = {
    loading: false,
    local: null,
    localError: "",
    liveError: "访问令牌已经到点了，没有发请求",
    live: {
      provider: "gpt",
      account: "demo",
      plan_type: null,
      fetched_at: "2026-11-09 12:00",
      source: "fixture",
      windows: [
        {
          label: "5 小时",
          present: true,
          remaining_percent: 40,
          reset_epoch: now / 1000 + 120,
          reset_at: "12:02",
          window_minutes: 300,
        },
      ],
      credits_balance: null,
      banked_resets: null,
    },
  };
  render(<CodexQuotaBars state={state} />);
  expect(screen.getByText("2m 后重置")).toBeTruthy();
  expect(screen.getByText("刷新没成 · 保留上次读数")).toBeTruthy();
  act(() => vi.advanceTimersByTime(120_000));
  expect(screen.getByText("已到重置时间 · 待刷新确认额度")).toBeTruthy();
  expect(screen.queryByText("剩 40%")).toBeNull();
});

it("没有有效重置时间时不编造下一次刷新周期", () => {
  expect(resetEpoch("bad-date")).toBeNull();
  expect(quotaCountdown(null, now)).toBe("");
  expect(quotaCountdown(Number.NaN, now)).toBe("");
  expect(quotaCountdown(now / 1000 - 1, now)).toBe("到点 · 待刷新");
});
