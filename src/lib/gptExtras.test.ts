import { describe, expect, it } from "vitest";
import { gptExtras, usableResets } from "./gptExtras";
import type { TavernGptQuota } from "./generated/TavernGptQuota";

const base: TavernGptQuota = {
  provider: "gpt",
  account: "a@example.com",
  plan_type: "plus",
  windows: [],
  fetched_at: "2026-10-26 10:00:00",
  source: "https://chatgpt.com/backend-api/wham/usage",
  credits_balance: null,
  banked_resets: null,
};
const now = Date.parse("2026-10-26T00:00:00Z");

describe("GPT 额度附带的两样", () => {
  it("drops resets that expired after the last ask and keeps unknown dates", () => {
    const q = {
      ...base,
      banked_resets: [
        "2026-10-25T00:00:00Z",
        "2026-11-02T08:00:00Z",
        null,
        "下周",
      ],
    };
    expect(usableResets(q, now)).toEqual([
      "2026-11-02T08:00:00Z",
      null,
      "下周",
    ]);
    expect(gptExtras(q, now)[0]).toMatch(
      /^存下的限额重置 3 次 · 最早 \d\d-\d\d \d\d:\d\d 到期$/,
    );
  });

  it("says a reset never expires and shows the credits count without a dollar guess", () => {
    const q = { ...base, banked_resets: [null], credits_balance: 62500 };
    expect(gptExtras(q, now)).toEqual([
      "存下的限额重置 1 次 · 不会过期",
      "Codex Credits 余额 62,500 · 额度用完后才扣",
    ]);
    expect(gptExtras(q, now).join("")).not.toMatch(/\$/);
  });

  it("shows nothing when the answer had neither", () => {
    expect(gptExtras(base, now)).toEqual([]);
    expect(gptExtras({ ...base, banked_resets: [] }, now)).toEqual([]);
    expect(gptExtras(null, now)).toEqual([]);
  });
});
