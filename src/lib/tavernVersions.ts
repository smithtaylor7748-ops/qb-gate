/**
 * SillyTavern 的版本号比较（软件页 SillyTavern 卡的「版本」，2026-09-29）。
 *
 * 只用来决定按钮怎么写、亮不亮（「更新到」「换到这一版（降级）」）；真正判能不能换的是后端
 * （`sillytavern_setup::check_switch`）。
 */

/** 数字逐段比。认不出的段按 0。 */
export function compareVersions(a: string, b: string): number {
  const parse = (v: string) =>
    v
      .trim()
      .replace(/^v/, "")
      .split(".")
      .map((x) => parseInt(x, 10) || 0);
  const pa = parse(a);
  const pb = parse(b);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d) return d;
  }
  return 0;
}

export type Relation = "newer" | "older" | "same" | "unknown";

/** `candidate` 相对现在装着的 `current` 是新是旧。现在没装 / 读不出版本是 `unknown`。 */
export function relation(
  candidate: string,
  current: string | null | undefined,
): Relation {
  if (!current || current === "?") return "unknown";
  const d = compareVersions(candidate, current);
  return d > 0 ? "newer" : d < 0 ? "older" : "same";
}

export const RELATION_TEXT: Record<Relation, string> = {
  newer: "比现在新",
  older: "比现在旧",
  same: "就是现在这版",
  unknown: "",
};
