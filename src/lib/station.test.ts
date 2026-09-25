import { describe, expect, it } from "vitest";
import type { StationRates } from "./generated/StationRates";
import {
  NEW_API_USD_PER_MTOK,
  modelRateLabel,
  topupOf,
  type StationModel,
} from "./station";

const empty: StationRates = {
  model_ratio: null,
  completion_ratio: null,
  cache_ratio: null,
  create_cache_ratio: null,
  group_ratio: null,
  peak_rate: null,
  per_request_price: null,
  input_price: null,
  cache_read_price: null,
  cache_write_price: null,
  output_price: null,
};

const model = (rates: Partial<StationRates>): StationModel => ({
  model: "claude-opus-5",
  rates: { ...empty, ...rates },
  groups: [],
});

describe("modelRateLabel", () => {
  // New API 的倍率是单价，单位 $2 / 百万（它源码：1 === $0.002 / 1K tokens）。
  // 0.25.3 及以前这里写「×2.5 · 计费翻倍 ×5」—— 照官方价收费的 Opus 5 看起来贵两倍半、
  // 输出还翻五倍，而 5 只是官方自己的「输出 ÷ 输入」。
  it("turns New API ratios into unit prices, group discount included", () => {
    expect(NEW_API_USD_PER_MTOK).toBe(2);
    const label = modelRateLabel(
      model({ model_ratio: 2.5, completion_ratio: 5, group_ratio: 0.2 }),
    );
    expect(label).toBe("每百万 输入 $1 · 输出 $5");
    expect(label).not.toContain("翻倍");
    // 不知道分组 = 按不打折的价写，跟站点自己定价页一样。
    expect(
      modelRateLabel(model({ model_ratio: 2.5, completion_ratio: 5 })),
    ).toBe("每百万 输入 $5 · 输出 $25");
  });

  it("writes an unknown side as a dash rather than borrowing the other", () => {
    expect(modelRateLabel(model({ model_ratio: 0.5 }))).toBe(
      "每百万 输入 $1 · 输出 —",
    );
  });

  it("reads sub2api's absolute prices the same way", () => {
    expect(
      modelRateLabel(
        model({ input_price: 5, output_price: 25, group_ratio: 0.12 }),
      ),
    ).toBe("每百万 输入 $0.6 · 输出 $3");
  });

  it("says per-request and unknown plainly", () => {
    expect(modelRateLabel(model({ per_request_price: 0.02 }))).toBe(
      "按次 $0.02",
    );
    expect(modelRateLabel(model({}))).toBe("价格未公布");
  });
});

describe("topupOf", () => {
  // 帖子里说的「1 美金 = 多少平台币」。没填按最常见的 1 元 = 1 美元额度。
  it("defaults to one yuan per dollar and ignores nonsense", () => {
    expect(topupOf(undefined)).toBe(1);
    expect(topupOf({ topup_per_usd: null })).toBe(1);
    expect(topupOf({ topup_per_usd: 0 })).toBe(1);
    expect(topupOf({ topup_per_usd: Number.NaN })).toBe(1);
    expect(topupOf({ topup_per_usd: 7 })).toBe(7);
  });
});
