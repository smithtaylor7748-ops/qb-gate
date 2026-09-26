import { describe, expect, it } from "vitest";

import type { AntigravityModelQuota } from "./generated/AntigravityModelQuota";
import type { AntigravityQuotaWindow } from "./generated/AntigravityQuotaWindow";
import {
  countdown,
  familyOf,
  groupQuota,
  groupWindows,
  lowestQuota,
  modelGroups,
  percent,
  quotaErrorLabel,
  resetIn,
  tierShort,
  tightestWindow,
} from "./antigravityQuota";

function w(
  group: AntigravityQuotaWindow["group"],
  span: AntigravityQuotaWindow["span"],
  remaining: number,
): AntigravityQuotaWindow {
  return {
    group,
    span,
    remaining,
    remaining_implied: false,
    reset_epoch: 1_790_000_000,
    reset_at: "2026-09-21 07:38",
    note: null,
  };
}

function q(
  label: string,
  remaining: number | null,
  reset_epoch: number | null = 1_790_000_000,
  tags: string[] = [],
): AntigravityModelQuota {
  return {
    label,
    model_id: 1,
    remaining,
    reset_at: reset_epoch == null ? null : "2026-09-21 07:38",
    reset_epoch,
    tags,
    remaining_implied: false,
  };
}

describe("反重力配额的显示", () => {
  it("括号里的推理档位不算族名", () => {
    expect(familyOf("Gemini 3.7 Flash (High)")).toBe("Gemini 3.7 Flash");
    expect(familyOf("Claude Opus 4.6 (Thinking)")).toBe("Claude Opus 4.6");
    expect(familyOf("GPT-OSS 120B")).toBe("GPT-OSS 120B");
    expect(familyOf("  ")).toBe("");
  });

  it("同族三档合成一行，取最低的剩余比例和它的重置时间，标签并集", () => {
    const g = groupQuota([
      q("Gemini 3.7 Flash (High)", 1.0, 100, ["Fast"]),
      q("Gemini 3.7 Flash (Medium)", 0.4, 200, ["Fast", "Limited time"]),
      q("Gemini 3.7 Flash (Low)", 0.9, 300, ["Limited time"]),
      q("Claude Opus 4.6 (Thinking)", 0.7),
    ]);
    expect(g.map((f) => f.family)).toEqual([
      "Gemini 3.7 Flash",
      "Claude Opus 4.6",
    ]);
    expect(g[0].remaining).toBe(0.4);
    expect(g[0].reset_epoch).toBe(200);
    expect(g[0].variants).toBe(3);
    expect(g[0].tags).toEqual(["Fast", "Limited time"]);
    expect(g[1].variants).toBe(1);
  });

  // ⛔ 没有额度信息的模型不参与「最低」，也不显示成 0% —— 0% 读起来是「用光了」。
  it("没带额度的模型既不拉低最低值也不显示成 0", () => {
    const models = [
      q("GPT-OSS 120B (Medium)", null, null),
      q("Gemini 3.7 Flash (High)", 0.55),
    ];
    expect(lowestQuota(models)?.family).toBe("Gemini 3.7 Flash");
    expect(groupQuota(models)[0].remaining).toBeNull();
    expect(percent(null)).toBe("—");
    expect(lowestQuota([q("X", null, null)])).toBeNull();
    expect(lowestQuota([])).toBeNull();
  });

  it("百分比四舍五入并夹在 0–100", () => {
    expect(percent(0.554)).toBe("55%");
    expect(percent(1)).toBe("100%");
    expect(percent(-0.2)).toBe("0%");
    expect(percent(Number.NaN)).toBe("—");
  });

  it("重置倒计时按分钟 / 小时 / 天说，过了就说已到", () => {
    const now = 1_790_000_000_000;
    expect(resetIn(null, now)).toBe("");
    expect(resetIn(1_790_000_000, now)).toBe("已到重置时间");
    expect(resetIn(1_790_000_000 + 90, now)).toBe("2 分钟后重置");
    expect(resetIn(1_790_000_000 + 10, now)).toBe("1 分钟后重置");
    expect(resetIn(1_790_000_000 + 3 * 3600 + 120, now)).toBe("3 小时后重置");
    expect(resetIn(1_790_000_000 + 3 * 86_400, now)).toBe("3 天后重置");
  });
});

describe("反重力联网额度（四格）的显示", () => {
  it("紧凑倒计时照 4h55m / 3d11m 的写法，零段省掉", () => {
    const now = 1_790_000_000_000;
    expect(countdown(null, now)).toBe("");
    expect(countdown(1_790_000_000, now)).toBe("已重置");
    expect(countdown(1_790_000_000 + 30, now)).toBe("<1m");
    expect(countdown(1_790_000_000 + 4 * 3600 + 55 * 60 + 20, now)).toBe(
      "4h55m",
    );
    expect(countdown(1_790_000_000 + 3 * 86_400 + 11 * 60, now)).toBe("3d11m");
    expect(countdown(1_790_000_000 + 2 * 86_400, now)).toBe("2d");
  });

  it("最紧的那一格是剩得最少的那一格", () => {
    const all = [
      w("claude", "five-hour", 1),
      w("claude", "weekly", 1),
      w("gemini", "five-hour", 1),
      w("gemini", "weekly", 0.95),
    ];
    expect(tightestWindow(all)).toEqual(all[3]);
    expect(tightestWindow([])).toBeNull();
  });

  it("按组取两格，缺哪格就是 null", () => {
    const all = [w("gemini", "weekly", 0.5), w("claude", "five-hour", 0.2)];
    expect(groupWindows(all, "gemini")).toEqual([null, all[0]]);
    expect(groupWindows(all, "claude")).toEqual([all[1], null]);
  });

  // ⛔ 免费档没有四格：按模型合成两组，没读到比例的不参与（没读到 ≠ 0）。
  it("免费档按模型合成 Claude / Gemini 两组，各取最低", () => {
    const g = modelGroups([
      q("Gemini 3.8 Flash (High)", 0.9),
      q("Gemini 3.1 Pro (High)", 0.4),
      q("Claude Sonnet 4.6", 0.7),
      q("GPT-OSS 120B", null, null),
    ]);
    expect(g.map((x) => [x.group, x.model.label])).toEqual([
      ["claude", "Claude Sonnet 4.6"],
      ["gemini", "Gemini 3.1 Pro (High)"],
    ]);
    expect(modelGroups([q("GPT-OSS 120B", null, null)])).toEqual([]);
  });

  it("档位写短：Ultra / Pro / 免费档，认不出就原样", () => {
    expect(tierShort("g1-ultra-tier", "Google AI Ultra")).toBe("Ultra");
    expect(tierShort("g1-pro-tier", "Google AI Pro")).toBe("Pro");
    expect(tierShort("free-tier", "Antigravity Starter Quota")).toBe("免费档");
    expect(tierShort("standard-tier", "Standard")).toBe("Standard");
    expect(tierShort(null, null)).toBe("");
  });
});

describe("额度来源那一格的短标签", () => {
  // 2026-09-25：原来含「令牌 / token」的错误一律叫「401 未授权」，连网络超时、
  // 找不到客户端标识都算 —— 把到点说成被拒。
  it("只有真的 401 才叫 401", () => {
    expect(quotaErrorLabel("Google 回了 HTTP 401：unauthenticated")).toBe(
      "401 未授权",
    );
    expect(quotaErrorLabel("换新访问令牌没成功（operation timed out）")).toBe(
      "读取失败",
    );
    expect(quotaErrorLabel("本机没有能用的客户端标识，令牌换不下来")).toBe(
      "读取失败",
    );
  });

  it("登录已失效、到点、被拒、限流、没登录各说各的", () => {
    expect(quotaErrorLabel("登录已失效：Google 说刷新令牌作废了")).toBe(
      "登录已失效",
    );
    expect(quotaErrorLabel("访问令牌已经到点了（登录本身没问题）")).toBe(
      "令牌到点",
    );
    expect(quotaErrorLabel("HTTP 403 forbidden")).toBe("403 被拒绝");
    expect(quotaErrorLabel("HTTP 429 too many requests")).toBe("限流");
    expect(quotaErrorLabel("没有激活的 Gemini CLI 槽位")).toBe("未登录");
  });
});
