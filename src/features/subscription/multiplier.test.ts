import { describe, expect, it } from "vitest";
import {
  effectiveMultiplier,
  fmtMultiplier,
  fmtTimes,
  fmtUsd,
  multiplierTone,
  parseAmount,
  timesMoreExpensive,
  toUsd,
} from "./multiplier";
import { PLANS } from "./data";

describe("parseAmount", () => {
  it("reads plain numbers, currency signs and thousands separators", () => {
    expect(parseAmount("40")).toBe(40);
    expect(parseAmount(" $1,234.5 ")).toBe(1234.5);
    expect(parseAmount("1，000")).toBe(1000);
  });

  it("refuses empty, non-numeric, zero and negative input", () => {
    expect(parseAmount("")).toBeNull();
    expect(parseAmount("abc")).toBeNull();
    expect(parseAmount("0")).toBeNull();
    expect(parseAmount("-5")).toBeNull();
    expect(parseAmount("1e999")).toBeNull();
  });
});

describe("toUsd", () => {
  it("divides by the rate and refuses non-positive values", () => {
    expect(toUsd(70, 7)).toBe(10);
    expect(toUsd(40, 1)).toBe(40);
    expect(toUsd(40, 0)).toBeNull();
    expect(toUsd(-1, 7)).toBeNull();
    expect(toUsd(Number.NaN, 7)).toBeNull();
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

  it("puts every measured official plan at or below 0.05×", () => {
    for (const plan of PLANS) {
      if (plan.apiValue === null) continue;
      const m = effectiveMultiplier(plan.webMonthly, plan.apiValue);
      expect(m).not.toBeNull();
      expect(m!).toBeLessThanOrEqual(0.05);
      expect(multiplierTone(m!)).toBe("ok");
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
  it("colours the three bands the page talks about", () => {
    expect(multiplierTone(0.014)).toBe("ok");
    expect(multiplierTone(0.05)).toBe("ok");
    expect(multiplierTone(0.2)).toBe("warn");
    expect(multiplierTone(0.3)).toBe("warn");
    expect(multiplierTone(0.8)).toBe("danger");
  });

  it("formats multipliers without trailing zeros", () => {
    expect(fmtMultiplier(0.025)).toBe("0.025×");
    expect(fmtMultiplier(0.05)).toBe("0.05×");
    expect(fmtMultiplier(1.5)).toBe("1.5×");
    expect(fmtMultiplier(0.0142857)).toBe("0.014×");
  });

  it("formats dollars and multiples for humans", () => {
    expect(fmtUsd(8000)).toBe("$8,000");
    expect(fmtUsd(124.99)).toBe("$124.99");
    expect(fmtUsd(5.7142857)).toBe("$5.71");
    expect(fmtTimes(12)).toBe("12 倍");
    expect(fmtTimes(2.56)).toBe("2.6 倍");
  });
});
