/**
 * 反重力 IDE 写下的配额怎么显示（0.30.0）。**纯函数**，界面与 `test:ui` 演示数据共用。
 *
 * IDE 的状态库里每个「模型 × 推理档」一条（`Gemini 3.7 Flash (High)` / `(Medium)` / `(Low)`），
 * 实测同一族三档的剩余比例与重置时间完全一样 —— 卡片上按**族**合并（去掉括号里的档位），
 * 同族取最低的那个剩余比例，别让 14 行把弹窗撑成一根面条。
 *
 * # ⛔ 「没有额度信息」不是 0
 *
 * `remaining` 为 `null` 的模型（IDE 没给它写额度）不参与「最低」的计算，也不显示成 0% ——
 * 0% 在界面上读起来是「用光了」这句斩钉截铁的话。
 */

import type { AntigravityModelQuota } from "./generated/AntigravityModelQuota";
import type { AntigravityQuotaGroup } from "./generated/AntigravityQuotaGroup";
import type { AntigravityQuotaSpan } from "./generated/AntigravityQuotaSpan";
import type { AntigravityQuotaWindow } from "./generated/AntigravityQuotaWindow";

export interface QuotaFamily {
  /** 去掉括号档位之后的名字，如 `Gemini 3.7 Flash`。 */
  family: string;
  /** 同族里最低的剩余比例 0–1；一档都没给就是 `null`。 */
  remaining: number | null;
  reset_at: string | null;
  reset_epoch: number | null;
  /** 同族所有档位的标签并集，去重、保序。 */
  tags: string[];
  /** 合并进来的档位数。 */
  variants: number;
  /** 取到的那个最低值是按 0 推出来的（Google 只给了重置时刻）。界面要说出来。 */
  implied: boolean;
}

/** `Gemini 3.7 Flash (High)` → `Gemini 3.7 Flash`。没有括号就原样。 */
export function familyOf(label: string): string {
  return label.replace(/\s*\([^()]*\)\s*$/, "").trim() || label.trim();
}

/** 按族合并；顺序按第一次出现。 */
export function groupQuota(models: AntigravityModelQuota[]): QuotaFamily[] {
  const out: QuotaFamily[] = [];
  const index = new Map<string, QuotaFamily>();
  for (const m of models) {
    const family = familyOf(m.label);
    let f = index.get(family);
    if (!f) {
      f = {
        family,
        remaining: null,
        reset_at: null,
        reset_epoch: null,
        tags: [],
        variants: 0,
        implied: false,
      };
      index.set(family, f);
      out.push(f);
    }
    f.variants += 1;
    if (
      m.remaining != null &&
      (f.remaining == null || m.remaining < f.remaining)
    ) {
      f.remaining = m.remaining;
      f.reset_at = m.reset_at;
      f.reset_epoch = m.reset_epoch;
      f.implied = m.remaining_implied;
    }
    for (const t of m.tags) if (!f.tags.includes(t)) f.tags.push(t);
  }
  return out;
}

/** 所有族里剩余最低的那一个；一个都没有额度信息就是 `null`。 */
export function lowestQuota(
  models: AntigravityModelQuota[],
): QuotaFamily | null {
  let low: QuotaFamily | null = null;
  for (const f of groupQuota(models)) {
    if (f.remaining == null) continue;
    if (low == null || f.remaining < (low.remaining ?? 1)) low = f;
  }
  return low;
}

/** `0.6` → `60%`；`null` → `—`。 */
export function percent(remaining: number | null): string {
  if (remaining == null || !Number.isFinite(remaining)) return "—";
  return `${Math.round(Math.min(1, Math.max(0, remaining)) * 100)}%`;
}

// ------------------------------------------------------------ 联网额度（2026-09-23）
//
// 联网问到的是「Claude / Gemini 两组 × 5 小时 / 每周」四格（`retrieveUserQuotaSummary`）。
// Google 的分组叫「Claude and GPT models」—— Claude 与 GPT-OSS 共用那一份，界面上叫 Claude，
// 悬停里说清楚。

export function groupLabel(group: AntigravityQuotaGroup): string {
  return group === "claude" ? "Claude" : "Gemini";
}

export function spanLabel(span: AntigravityQuotaSpan): string {
  return span === "five-hour" ? "5h" : "周";
}

/**
 * 紧凑的倒计时：`4h55m` / `3d11m` / `<1m`，过了就是「已重置」。给窄窄的额度条用 ——
 * 完整的本地时刻放在悬停里。
 */
export function countdown(resetEpoch: number | null, nowMs: number): string {
  if (resetEpoch == null) return "";
  const diff = resetEpoch * 1000 - nowMs;
  if (diff <= 0) return "已重置";
  const minutes = Math.floor(diff / 60_000);
  if (minutes < 1) return "<1m";
  const d = Math.floor(minutes / 1440);
  const h = Math.floor((minutes % 1440) / 60);
  const m = minutes % 60;
  return [d ? `${d}d` : "", h ? `${h}h` : "", m ? `${m}m` : ""].join("");
}

/** 四格里剩得最少的那一格（平局取先出现的）。一格都没有就是 `null`。 */
export function tightestWindow(
  windows: AntigravityQuotaWindow[],
): AntigravityQuotaWindow | null {
  let tight: AntigravityQuotaWindow | null = null;
  for (const w of windows)
    if (tight == null || w.remaining < tight.remaining) tight = w;
  return tight;
}

/** 某一组的两格：`[5h, 周]`，缺哪格就是 `null`。 */
export function groupWindows(
  windows: AntigravityQuotaWindow[],
  group: AntigravityQuotaGroup,
): [AntigravityQuotaWindow | null, AntigravityQuotaWindow | null] {
  const pick = (span: AntigravityQuotaSpan) =>
    windows.find((w) => w.group === group && w.span === span) ?? null;
  return [pick("five-hour"), pick("weekly")];
}

/**
 * 免费档没有四格（Google 回 403），只有按模型的：照汇总的分法合成两组，各取剩得最少的那个。
 * `remaining` 为 `null` 的不参与（没读到 ≠ 0）。
 */
export function modelGroups(
  models: AntigravityModelQuota[],
): { group: AntigravityQuotaGroup; model: AntigravityModelQuota }[] {
  const out: { group: AntigravityQuotaGroup; model: AntigravityModelQuota }[] =
    [];
  for (const group of ["claude", "gemini"] as const) {
    let low: AntigravityModelQuota | null = null;
    for (const m of models) {
      if (m.remaining == null) continue;
      const isGemini = /gemini/i.test(m.label);
      if ((group === "gemini") !== isGemini) continue;
      if (low == null || m.remaining < (low.remaining ?? 1)) low = m;
    }
    if (low) out.push({ group, model: low });
  }
  return out;
}

/** 档位的短写法：Ultra / Pro / 免费档；认不出就原样（太长截断）。 */
export function tierShort(
  tierId: string | null,
  tierName: string | null,
): string {
  const id = (tierId ?? "").toLowerCase();
  if (id.includes("ultra")) return "Ultra";
  if (id.includes("pro")) return "Pro";
  if (id.includes("free")) return "免费档";
  const name = tierName ?? tierId ?? "";
  return Array.from(name).length > 12
    ? Array.from(name).slice(0, 12).join("") + "…"
    : name;
}

/**
 * 额度来源那一格的短标签（反重力用量卡的 IDE / Hub / Gemini CLI 三格）。完整原因在悬停里。
 *
 * ⛔ 顺序要紧（2026-09-25）：先认「登录已失效」「到点」，**只有真的 401 才叫「401 未授权」**。
 * 原来含「令牌 / token」的全叫 401 —— 连「换新访问令牌没成功（网络超时）」「找不到客户端标识」
 * 都算，把到点说成被拒，正是 CLAUDE.md「联网额度」第 8 条要防的那件事。
 */
export function quotaErrorLabel(message: string): string {
  if (message.startsWith("登录已失效")) return "登录已失效";
  if (/到点/.test(message)) return "令牌到点";
  if (/\b401\b|unauthori[sz]ed/i.test(message)) return "401 未授权";
  if (/\b403\b|forbidden|拒绝/i.test(message)) return "403 被拒绝";
  if (/\b429\b|限流|too many/i.test(message)) return "限流";
  if (/没有激活|未登录|还没登录|没有.*槽位/.test(message)) return "未登录";
  if (/设置里关掉/.test(message)) return "已关闭";
  return "读取失败";
}

/**
 * 「几小时后重置」。`nowMs` 显式传入 —— 一个叫「N 分钟后」的字不会自己往前走，
 * 调用方要在重算用量时一起重算它。
 */
export function resetIn(resetEpoch: number | null, nowMs: number): string {
  if (resetEpoch == null) return "";
  const diff = resetEpoch * 1000 - nowMs;
  if (diff <= 0) return "已到重置时间";
  const minutes = Math.round(diff / 60_000);
  if (minutes < 60) return `${Math.max(1, minutes)} 分钟后重置`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours} 小时后重置`;
  return `${Math.floor(hours / 24)} 天后重置`;
}
