/**
 * 桥接设置里的数字格式（2026-09-29 从 `ClaudeBridgeSettings.tsx` 抽出来）。
 *
 * ⛔ 桥没给的数显示「—」：0 是一句断言，「不知道」不能说成 0（§7.17 / 坑 7.98）。
 */
const NUM = new Intl.NumberFormat("zh-CN");

/** token 之类的大数：超过一千缩成 K / M。 */
export const short = (n: number | null) =>
  n == null
    ? "—"
    : n >= 1_000_000
      ? (n / 1_000_000).toFixed(1) + "M"
      : n >= 1_000
        ? (n / 1_000).toFixed(1) + "K"
        : NUM.format(n);

/** 回复字数要拿去跟角色卡里的字数规则对照，不缩写成 K。 */
export const exact = (n: number | null) => (n == null ? "—" : NUM.format(n));

/** 0–1 的比例 → 百分数。 */
export const pct = (v: number | null) =>
  v == null ? "—" : `${Math.round(v * 100)}%`;

export const count = (n: number) => NUM.format(n);

/** 前缀稳定性那一列的人话。桥给的是英文枚举，原样显示没人读得懂。 */
export const PREFIX_TEXT: Record<string, string> = {
  identical: "完全一致",
  append_only: "只追加",
  rewound: "回退过",
  first: "首轮",
  system_changed: "系统提示词变了",
  settings_changed: "设置变了",
  history_rewritten: "历史被改写",
};
