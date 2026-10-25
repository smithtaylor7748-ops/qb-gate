import { describe, expect, it } from "vitest";

import {
  mappedCount,
  modelMapOf,
  NO_TIERS,
  pairsText,
  parsePairs,
  tiersFromPreset,
  tiersOf,
} from "./modelMap";

describe("预设预填模型映射", () => {
  it("预设写了的档位填进去，没写的留空", () => {
    expect(
      tiersFromPreset({
        default: "deepseek-v4-pro",
        haiku: "deepseek-flash",
        sonnet: "deepseek-v4-pro",
        opus: "deepseek-v4-pro",
        fable: null,
        subagent: null,
      }),
    ).toEqual({
      ...NO_TIERS,
      default: "deepseek-v4-pro",
      haiku: "deepseek-flash",
      sonnet: "deepseek-v4-pro",
      opus: "deepseek-v4-pro",
    });
  });
});

describe("线路的模型映射表单", () => {
  it("逐个指定认几种常见的箭头，空行跳过、两边去空白", () => {
    expect(parsePairs("gpt-5.6-sol → gpt-5.6\n\n  a=>b \nc->d\ne = f")).toEqual(
      [
        { from: "gpt-5.6-sol", to: "gpt-5.6" },
        { from: "a", to: "b" },
        { from: "c", to: "d" },
        { from: "e", to: "f" },
      ],
    );
  });

  it("读不懂的那一行报出来（带行号），不悄悄跳过", () => {
    // 悄悄跳过的话，存下来少了一条，而界面上看起来全都保存了。
    expect(() => parsePairs("a → b\n只有一边")).toThrow(/第 2 行/);
    expect(() => parsePairs("a → b → c")).toThrow(/第 1 行/);
    expect(() => parsePairs("→ b")).toThrow(/第 1 行/);
  });

  it("存下来的读回表单再存，一字不变", () => {
    const saved = modelMapOf(
      { ...NO_TIERS, opus: " relay-opus ", default: "" },
      "claude-opus-5 → x",
    );
    expect(saved).toEqual({
      exact: [{ from: "claude-opus-5", to: "x" }],
      opus: "relay-opus",
      sonnet: null,
      haiku: null,
      fable: null,
      subagent: null,
      // 空的那几格是 null（不换），不是空串 —— 空串会被当成「换成空名字」。
      default: null,
    });
    expect(modelMapOf(tiersOf(saved), pairsText(saved))).toEqual(saved);
  });

  it("折叠标题上的数只数填了的", () => {
    expect(mappedCount(NO_TIERS, "")).toBe(0);
    expect(mappedCount({ ...NO_TIERS, sonnet: "s" }, "a → b\n\nc → d")).toBe(3);
  });
});
