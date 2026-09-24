import { describe, expect, it } from "vitest";
import {
  YUAN_PER_USD,
  YUAN_PER_USD_TYPICAL,
  effectiveMultiplier,
  fmtMultiplier,
  fmtTimes,
  fmtUsd,
  fmtYuan,
  multiplierTone,
  officialMultiplier,
  parseAmount,
  planMultiplier,
  timesMoreExpensive,
} from "./multiplier";
import { PLANS, WEEKS_PER_MONTH, monthlyValue } from "./data";

describe("plan quotas", () => {
  // 2026-09-24 使用者定的口径：「额度取这个帖子里的中间值」（linux.do 2831355）。
  // 钉住的是规矩本身：以后改区间，周额度必须跟着取中间值，不许各改各的。
  it("uses the midpoint of the forum thread's range for every plan", () => {
    for (const plan of PLANS) {
      if (plan.weeklyRange === null) {
        expect(plan.weeklyValue, plan.id).toBeNull();
        continue;
      }
      const [lo, hi] = plan.weeklyRange;
      expect(plan.weeklyValue, plan.id).toBe((lo + hi) / 2);
    }
  });

  it("matches the thread's numbers (weekly, USD at API list prices)", () => {
    const weekly = Object.fromEntries(PLANS.map((p) => [p.id, p.weeklyValue]));
    expect(weekly).toEqual({
      "claude-pro": 350,
      "claude-max-5": 2500,
      "claude-max-20": 4000,
      "chatgpt-go": null,
      "chatgpt-plus": 130,
      "chatgpt-pro-5": 600,
      "chatgpt-pro-20": 2500,
    });
  });

  it("turns a week into a month with a fixed four weeks", () => {
    const max20 = PLANS.find((p) => p.id === "claude-max-20")!;
    expect(WEEKS_PER_MONTH).toBe(4);
    expect(monthlyValue(max20)).toBe(16000);
    expect(monthlyValue(PLANS.find((p) => p.id === "chatgpt-go")!)).toBeNull();
  });
});

describe("parseAmount", () => {
  it("reads plain numbers, currency signs and thousands separators", () => {
    expect(parseAmount("40")).toBe(40);
    expect(parseAmount(" $1,234.5 ")).toBe(1234.5);
    expect(parseAmount("1，000")).toBe(1000);
    expect(parseAmount("40元")).toBe(40);
    expect(parseAmount("¥1,400")).toBe(1400);
    expect(parseAmount("￥ 280")).toBe(280);
  });

  it("refuses empty, non-numeric, zero and negative input", () => {
    expect(parseAmount("")).toBeNull();
    expect(parseAmount("abc")).toBeNull();
    expect(parseAmount("0")).toBeNull();
    expect(parseAmount("-5")).toBeNull();
    expect(parseAmount("1e999")).toBeNull();
  });
});

describe("effectiveMultiplier", () => {
  it("is spend divided by monthly API value — the formula the user asked for", () => {
    expect(effectiveMultiplier(40, 8000)).toBeCloseTo(0.005, 6);
    expect(effectiveMultiplier(200, 8000)).toBeCloseTo(0.025, 6);
    expect(effectiveMultiplier(200, 14000)).toBeCloseTo(0.0142857, 6);
  });

  it("never yields NaN or Infinity", () => {
    expect(effectiveMultiplier(40, 0)).toBeNull();
    expect(effectiveMultiplier(0, 8000)).toBeNull();
    expect(effectiveMultiplier(Number.POSITIVE_INFINITY, 1)).toBeNull();
  });
});

describe("official plans at 1:7", () => {
  // 2026-09-24 使用者定的：官方月价按 1 美元 = 7 元折成元（往贵了取，实际一般 6.8 左右），
  // 跟中转站「每 $1 牌价付几元」放在一把尺子上。钉住的是这两个数本身。
  it("converts the dollar price at the rate the user picked", () => {
    expect(YUAN_PER_USD).toBe(7);
    expect(YUAN_PER_USD_TYPICAL).toBe(6.8);
    expect(YUAN_PER_USD).toBeGreaterThan(YUAN_PER_USD_TYPICAL);
    expect(officialMultiplier(200, 16000)).toBeCloseTo(0.0875, 9);
    expect(officialMultiplier(200, 0)).toBeNull();
  });

  it("gives every plan one number, the one the whole page shows", () => {
    const shown = Object.fromEntries(
      PLANS.map((p) => {
        const m = planMultiplier(p);
        return [p.id, m === null ? null : fmtMultiplier(m)];
      }),
    );
    expect(shown).toEqual({
      "claude-pro": "0.1×",
      "claude-max-5": "0.07×",
      "claude-max-20": "0.088×",
      "chatgpt-go": null,
      "chatgpt-plus": "0.269×",
      "chatgpt-pro-5": "0.292×",
      "chatgpt-pro-20": "0.14×",
    });
  });

  it("lands every plan with quota data in the reverse-relay price band, coloured ok", () => {
    for (const plan of PLANS) {
      if (monthlyValue(plan) === null) continue;
      const m = planMultiplier(plan);
      expect(m, plan.id).not.toBeNull();
      expect(m!, plan.id).toBeLessThanOrEqual(0.3);
      expect(multiplierTone(m!), plan.id).toBe("ok");
    }
  });
});

describe("timesMoreExpensive", () => {
  it("compares a relay multiplier against the official one", () => {
    expect(timesMoreExpensive(0.3, 0.025)).toBeCloseTo(12, 6);
    expect(timesMoreExpensive(0.3, 0)).toBeNull();
  });
});

describe("tone and formatting", () => {
  it("colours by price only: subscription level, dearer, official-relay and up", () => {
    expect(multiplierTone(0.014)).toBe("ok");
    expect(multiplierTone(0.0875)).toBe("ok");
    expect(multiplierTone(0.3)).toBe("ok");
    expect(multiplierTone(0.31)).toBe("warn");
    expect(multiplierTone(0.79)).toBe("warn");
    expect(multiplierTone(0.8)).toBe("danger");
    expect(multiplierTone(8)).toBe("danger");
  });

  it("formats multipliers without trailing zeros, rounding half up", () => {
    expect(fmtMultiplier(0.025)).toBe("0.025×");
    expect(fmtMultiplier(0.05)).toBe("0.05×");
    expect(fmtMultiplier(1.5)).toBe("1.5×");
    expect(fmtMultiplier(0.0142857)).toBe("0.014×");
    // $200 × 7 ÷ $16,000：二进制里略小于 0.0875，直接 toFixed(3) 会是 0.087。
    expect(fmtMultiplier((200 * 7) / 16000)).toBe("0.088×");
    expect(fmtMultiplier(7)).toBe("7×");
  });

  it("formats dollars, yuan and multiples for humans", () => {
    expect(fmtUsd(8000)).toBe("$8,000");
    expect(fmtUsd(124.99)).toBe("$124.99");
    expect(fmtUsd(5.7142857)).toBe("$5.71");
    expect(fmtYuan(1400)).toBe("1,400 元");
    expect(fmtYuan(12.5)).toBe("12.50 元");
    expect(fmtTimes(12)).toBe("12 倍");
    expect(fmtTimes(2.56)).toBe("2.6 倍");
  });
});
