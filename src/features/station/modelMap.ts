/**
 * 线路自己的模型映射在表单里的那几格（`RouteModelMap`，本机路由转发到这条线时换模型名）。
 *
 * 读写都在这里，表单（`RouteDialog.tsx`）只管画。
 */

import type { PresetModels } from "../../lib/generated/PresetModels";
import type { RouteModelMap } from "../../lib/generated/RouteModelMap";

/**
 * 预设里各档的模型名换成表单那几格（中转站重构阶段 4：cc-switch 的预设带着各档模型名）。
 * 预设没写的那一档留空。
 */
export function tiersFromPreset(m: PresetModels): Tiers {
  return {
    opus: m.opus ?? "",
    sonnet: m.sonnet ?? "",
    haiku: m.haiku ?? "",
    fable: m.fable ?? "",
    subagent: m.subagent ?? "",
    default: m.default ?? "",
  };
}

/** 档位那几格（`RouteModelMap` 里除 `exact` 以外的）。空串 = 没填。 */
export type Tiers = Record<
  "opus" | "sonnet" | "haiku" | "fable" | "subagent" | "default",
  string
>;

export const NO_TIERS: Tiers = {
  opus: "",
  sonnet: "",
  haiku: "",
  fable: "",
  subagent: "",
  default: "",
};

/** 「逐个指定」那一格认的分隔符。照着写最常见的几种，不让人去猜。 */
const PAIR_SEP = /\s*(?:→|=>|->|=)\s*/;

/** 一行一条「从 → 到」读成数组。读不懂的那一行**报出来**，不悄悄跳过。 */
export function parsePairs(text: string): RouteModelMap["exact"] {
  const pairs: RouteModelMap["exact"] = [];
  text.split(/\r?\n/).forEach((raw, i) => {
    const line = raw.trim();
    if (!line) return;
    const [from, to, ...rest] = line.split(PAIR_SEP);
    if (!from || !to || rest.length) {
      throw new Error(
        `模型映射「逐个指定」第 ${i + 1} 行看不懂：「${line}」—— 写成「客户端写的名字 → 这条线上的名字」`,
      );
    }
    pairs.push({ from, to });
  });
  return pairs;
}

/** 存下来的那份变回表单里的文字（一行一条）。 */
export function pairsText(m: RouteModelMap | undefined): string {
  return (m?.exact ?? []).map((p) => `${p.from} → ${p.to}`).join("\n");
}

export function tiersOf(m: RouteModelMap | undefined): Tiers {
  return {
    opus: m?.opus ?? "",
    sonnet: m?.sonnet ?? "",
    haiku: m?.haiku ?? "",
    fable: m?.fable ?? "",
    subagent: m?.subagent ?? "",
    default: m?.default ?? "",
  };
}

const orNull = (s: string) => (s.trim() === "" ? null : s.trim());

/** 表单里的几格拼回 `RouteModelMap`。逐个指定那一格读不懂会抛错（见 [`parsePairs`]）。 */
export function modelMapOf(tiers: Tiers, pairs: string): RouteModelMap {
  return {
    exact: parsePairs(pairs),
    opus: orNull(tiers.opus),
    sonnet: orNull(tiers.sonnet),
    haiku: orNull(tiers.haiku),
    fable: orNull(tiers.fable),
    subagent: orNull(tiers.subagent),
    default: orNull(tiers.default),
  };
}

/** 填了几项映射（标在折叠标题上：收起来也看得出这条线换了名字）。 */
export function mappedCount(tiers: Tiers, pairs: string): number {
  const filled = Object.values(tiers).filter((v) => v.trim() !== "").length;
  const lines = pairs.split(/\r?\n/).filter((l) => l.trim() !== "").length;
  return filled + lines;
}
