/** 会话列表的分组与几个显示小工具（纯函数，单测在旁边）。 */
import type { SessionSummary } from "../../lib/sessions";

export interface SessionGroup {
  key: string;
  label: string;
  /** 默认目录里找不到归属依据的那一组。 */
  unowned: boolean;
  items: SessionSummary[];
}

export const UNOWNED_LABEL = "默认目录（归属不明）";

/**
 * 按项目目录分组：组内保持传进来的顺序（后端已按更新时间排好），组按最近一条的先后排。
 * 归属不明的单列一组，放最后。
 */
export function groupSessions(items: SessionSummary[]): SessionGroup[] {
  const groups = new Map<string, SessionGroup>();
  let unowned: SessionGroup | null = null;
  for (const s of items) {
    if (s.unowned) {
      unowned ??= {
        key: "unowned",
        label: UNOWNED_LABEL,
        unowned: true,
        items: [],
      };
      unowned.items.push(s);
      continue;
    }
    const key = (s.cwd ?? s.project ?? "").toLowerCase() || "(none)";
    let g = groups.get(key);
    if (!g) {
      g = {
        key,
        label: s.project || s.cwd || "（项目目录读不出来）",
        unowned: false,
        items: [],
      };
      groups.set(key, g);
    }
    g.items.push(s);
  }
  const latest = (g: SessionGroup) =>
    g.items.reduce((m, s) => (s.updated_at > m ? s.updated_at : m), "");
  const out = [...groups.values()].sort((a, b) =>
    latest(b).localeCompare(latest(a)),
  );
  if (unowned) out.push(unowned);
  return out;
}

/** 「3 分钟前」「2 天前」。 */
export function ago(iso: string, now = Date.now()): string {
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return iso;
  const s = Math.max(0, Math.round((now - t) / 1000));
  if (s < 60) return "刚刚";
  if (s < 3600) return `${Math.floor(s / 60)} 分钟前`;
  if (s < 86400) return `${Math.floor(s / 3600)} 小时前`;
  if (s < 86400 * 30) return `${Math.floor(s / 86400)} 天前`;
  if (s < 86400 * 365) return `${Math.floor(s / (86400 * 30))} 个月前`;
  return `${Math.floor(s / (86400 * 365))} 年前`;
}

/** 1234 → 1.2K，2_500_000 → 2.5M。 */
export function compact(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)}B`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}K`;
  return String(n);
}

/** 会话 id 截短显示：前 8 位。 */
export function shortId(id: string): string {
  return id.length > 10 ? `${id.slice(0, 8)}…` : id;
}
