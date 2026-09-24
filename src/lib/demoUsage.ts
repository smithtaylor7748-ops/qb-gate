/**
 * 演示用的用量数据（`npm run demo` 与 `test:ui` 吃这一份，2026-09-24）。
 *
 * 小结按 Rust 侧 `usecase::token_summary::summarize_with` 的同一个算式**现算** ——
 * 夹具自己另编一套数的话，界面上那几个格子就永远验不出算错了没有。
 * 数字全是编的（见 `demo.ts` 文件头）；只被 `demo.ts` 引用，正式包里整个摇掉。
 *
 * 故意放进去的几种形状，每一种界面都得有人看过：
 * - 缓存写大部分是 1 小时档（实机就是这样）；
 * - 今天有一条四类全 0 的报错（截图那一天的形状）；
 * - 一个没有官方价的模型（`claude-opus-4-6-thinking`）；
 * - 中间缺一天（趋势图上没有那根柱子，表里写「没有记录」）；
 * - 有未归属、有别的槽位的量（「按账户」表）。
 */
import type { AccountSpend } from "./generated/AccountSpend";
import type { CodexUsageSummary } from "./generated/CodexUsageSummary";
import type { TokenBucket } from "./generated/TokenBucket";
import type { TokenSummary } from "./generated/TokenSummary";
import type { TokenUsage } from "./generated/TokenUsage";
import type { UsageOverview } from "./generated/UsageOverview";
import { addDays, today as todayYmd, ymd } from "./dates";

interface Price {
  input: number;
  output: number;
  read: number;
  w5: number;
  w1: number;
  live: boolean;
}

/** 演示价（每百万 token，美元）。`claude-opus-4-6-thinking` 故意不给。 */
const PRICES: Record<string, Price> = {
  "claude-opus-5": {
    input: 5,
    output: 25,
    read: 0.5,
    w5: 6.25,
    w1: 10,
    live: true,
  },
  "claude-sonnet-5": {
    input: 2,
    output: 10,
    read: 0.2,
    w5: 2.5,
    w1: 4,
    live: true,
  },
  "gemini-3.7-flash": {
    input: 0.75,
    output: 3,
    read: 0.075,
    w5: 0.9375,
    w1: 1.5,
    live: false,
  },
  "gpt-5.6-sol": { input: 4, output: 20, read: 0.4, w5: 5, w1: 8, live: true },
};

type Tok = Pick<
  TokenBucket,
  "input" | "output" | "cache_write" | "cache_write_1h" | "cache_read"
>;

function parts(t: Tok, p: Price) {
  const w1 = Math.min(t.cache_write_1h, t.cache_write);
  const w5 = t.cache_write - w1;
  return {
    input: (t.input * p.input) / 1e6,
    output: (t.output * p.output) / 1e6,
    cache_write: (w5 * p.w5 + w1 * p.w1) / 1e6,
    cache_read: (t.cache_read * p.read) / 1e6,
  };
}
const total = (c: ReturnType<typeof parts>) =>
  c.input + c.output + c.cache_write + c.cache_read;
const costOf = (model: string, t: Tok): number | null => {
  const p = PRICES[model];
  return p ? total(parts(t, p)) : null;
};
const add = (a: number | null, b: number | null) =>
  b == null ? a : (a ?? 0) + b;

function window(buckets: TokenBucket[], from: string | null) {
  let usd: number | null = null;
  let messages = 0;
  let unpriced = 0;
  for (const b of buckets) {
    if (from && b.day < from) continue;
    messages += b.messages;
    const c = costOf(b.model, b);
    if (c == null) unpriced += b.messages;
    usd = add(usd, c);
  }
  return { usd, messages, unpriced_messages: unpriced };
}

const sinceDay = (days: number, today: string) =>
  days > 0 ? addDays(today, -(days - 1)) : null;

/** 跟 Rust 的 `summarize_with` 同一个算式。 */
export function demoSummarize(
  buckets: TokenBucket[],
  days: number,
  unattributed: TokenBucket[] = [],
): TokenSummary {
  const today = todayYmd();
  const from = sinceDay(days, today);
  const picked = buckets.filter((b) => !from || b.day >= from);
  const s: TokenSummary = {
    days,
    input: 0,
    output: 0,
    cache_write: 0,
    cache_write_1h: 0,
    cache_read: 0,
    messages: 0,
    hit_rate: null,
    saved_usd: null,
    unpriced_models: 0,
    unattributed: 0,
    unattributed_messages: 0,
    priced_from_snapshot: null,
    daily: [],
    cost_usd: null,
    cost_parts: null,
    spend: {
      today: window(buckets, today),
      last_7d: window(buckets, sinceDay(7, today)),
      last_30d: window(buckets, sinceDay(30, today)),
    },
    models: [],
    hourly: [],
    recent: [],
    empty_replies: 0,
    last_at: null,
    unpriced: [],
    prices_used: [],
    coverage: null,
  };
  for (const b of unattributed.filter((b) => !from || b.day >= from)) {
    s.unattributed += b.input + b.output + b.cache_write + b.cache_read;
    s.unattributed_messages += b.messages;
  }
  const days_ = new Map<string, TokenSummary["daily"][number]>();
  const models = new Map<string, TokenSummary["models"][number]>();
  const unpriced = new Map<string, TokenSummary["unpriced"][number]>();
  const cp = { input: 0, output: 0, cache_write: 0, cache_read: 0 };
  let priced = false;
  let live = false;
  let saved: number | null = null;
  for (const b of picked) {
    s.input += b.input;
    s.output += b.output;
    s.cache_write += b.cache_write;
    s.cache_write_1h += b.cache_write_1h;
    s.cache_read += b.cache_read;
    s.messages += b.messages;
    const p = PRICES[b.model];
    const c = p ? parts(b, p) : null;
    const d = days_.get(b.day) ?? {
      day: b.day,
      input: 0,
      output: 0,
      cache_read: 0,
      cache_write: 0,
      cache_write_1h: 0,
      messages: 0,
      cost_usd: null,
    };
    d.input += b.input;
    d.output += b.output;
    d.cache_read += b.cache_read;
    d.cache_write += b.cache_write;
    d.cache_write_1h += b.cache_write_1h;
    d.messages += b.messages;
    days_.set(b.day, d);
    const m = models.get(b.model) ?? {
      model: b.model,
      messages: 0,
      input: 0,
      output: 0,
      cache_read: 0,
      cache_write: 0,
      cache_write_1h: 0,
      cost_usd: null,
    };
    m.messages += b.messages;
    m.input += b.input;
    m.output += b.output;
    m.cache_read += b.cache_read;
    m.cache_write += b.cache_write;
    m.cache_write_1h += b.cache_write_1h;
    models.set(b.model, m);
    if (p && c) {
      d.cost_usd = add(d.cost_usd, total(c));
      m.cost_usd = add(m.cost_usd, total(c));
      cp.input += c.input;
      cp.output += c.output;
      cp.cache_write += c.cache_write;
      cp.cache_read += c.cache_read;
      priced = true;
      live ||= p.live;
      if (b.cache_read > 0)
        saved = (saved ?? 0) + (b.cache_read * (p.input - p.read)) / 1e6;
    } else {
      const u = unpriced.get(b.model) ?? {
        model: b.model,
        messages: 0,
        tokens: 0,
      };
      u.messages += b.messages;
      u.tokens += b.input + b.output + b.cache_write + b.cache_read;
      unpriced.set(b.model, u);
    }
  }
  s.daily = [...days_.values()].sort((a, b) => a.day.localeCompare(b.day));
  s.models = [...models.values()].sort(
    (a, b) => (b.cost_usd ?? -1) - (a.cost_usd ?? -1),
  );
  s.prices_used = s.models
    .filter((m) => PRICES[m.model])
    .map((m) => {
      const p = PRICES[m.model];
      return {
        model: m.model,
        input: p.input,
        output: p.output,
        cache_read: p.read,
        cache_write_5m: p.w5,
        cache_write_1h: p.w1,
        cache_read_derived: true,
        cache_write_5m_derived: true,
        cache_write_1h_derived: true,
        live: p.live,
        verified_at: "2026-09-24",
      };
    });
  const readTotal = s.input + s.cache_read + s.cache_write;
  s.hit_rate = readTotal > 0 ? s.cache_read / readTotal : null;
  s.unpriced = [...unpriced.values()];
  s.unpriced_models = s.unpriced.length;
  if (priced) {
    s.cost_usd = cp.input + cp.output + cp.cache_write + cp.cache_read;
    s.cost_parts = cp;
    s.priced_from_snapshot = !live;
  }
  s.saved_usd = saved;
  return s;
}

/** 从今天往前第 `i` 天。 */
const dayAgo = (i: number) => addDays(todayYmd(), -i);

/**
 * 详情页的 token 统计（`accounts_tokens`）与用量小结的底料。
 *
 * 数字照实机的形状编：缓存读远大于输入（长会话靠缓存命中），缓存写几乎全是 1 小时档，
 * `duplicates` 和留下的条数一个量级 —— 那是续接会话重放出来的。
 */
export const demoTokenUsage: TokenUsage = {
  buckets: (() => {
    const out: TokenBucket[] = [];
    for (let i = 0; i < 12; i++) {
      if (i === 4) continue; // 故意缺一天：趋势图上没有那根柱子，表里写「没有记录」
      const day = dayAgo(i);
      const cw = 420_000 - i * 9_000;
      out.push({
        day,
        model: "claude-opus-5",
        input: 300 + i * 17,
        output: 90_000 - i * 3_100,
        cache_write: cw,
        cache_write_1h: Math.round(cw * 0.9),
        cache_read: 14_000_000 - i * 310_000,
        messages: 140 - i * 4,
      });
      if (i % 3 === 0) {
        out.push({
          day,
          model: "claude-sonnet-5",
          input: 120,
          output: 6_400,
          cache_write: 31_000,
          cache_write_1h: 31_000,
          cache_read: 880_000,
          messages: 11,
        });
      }
    }
    // 后端给的是按 (day, model) 升序，演示夹具不该比它宽松。
    return out.sort(
      (a, b) => a.day.localeCompare(b.day) || a.model.localeCompare(b.model),
    );
  })(),
  sessions: 37,
  files_read: 41,
  files_failed: 0,
  duplicates: 2_860,
  undated: 0,
  // 默认目录里没有账户标记的那部分。夹具不出现的形态，界面就没人验过。
  unattributed: [
    {
      day: dayAgo(1),
      model: "claude-opus-5",
      input: 1_680,
      output: 635_633,
      cache_write: 1_835_505,
      cache_write_1h: 1_835_505,
      cache_read: 289_051_108,
      messages: 780,
    },
  ],
  empty_replies: 1,
  empty_days: [{ day: todayYmd(), replies: 1 }],
  // 按小时：从现在往前几个小时（中间空一个小时）—— 写死成「9 点到 15 点」的话，
  // 凌晨打开演示就是一张空图，而「今天」那一行却有 151 条，自相矛盾。
  hourly: [0, 1, 3, 4, 6]
    .map((back) => new Date().getHours() - back)
    .filter((hour) => hour >= 0)
    .reverse()
    .map((hour, k) => ({
      day: todayYmd(),
      hour,
      model: "claude-opus-5",
      input: 60,
      output: 18_000 - k * 1_500,
      cache_write: 84_000,
      cache_write_1h: 75_600,
      cache_read: 2_800_000 - k * 90_000,
      messages: 28,
    })),
  // 最近请求：从现在往前每 7 分钟一条，跨过零点的就落在昨天。
  recent: Array.from({ length: 64 }, (_, k) => {
    const at = new Date(Date.now() - (k + 1) * 7 * 60_000);
    const hh = String(at.getHours()).padStart(2, "0");
    const mm = String(at.getMinutes()).padStart(2, "0");
    const ss = String(at.getSeconds()).padStart(2, "0");
    return {
      day: ymd(at),
      time: `${hh}:${mm}:${ss}`,
      model: k % 9 === 0 ? "claude-sonnet-5" : "claude-opus-5",
      input: 2 + (k % 3),
      output: 300 + ((k * 137) % 900),
      cache_write: 1_500 + ((k * 211) % 4_000),
      cache_write_1h: 1_500 + ((k * 211) % 4_000),
      cache_read: 120_000 + ((k * 7_919) % 380_000),
      project: k % 2 ? "C--Users-demo-projects-site" : "D--demo-app",
      session: k % 2 ? "3f2a9c1b" : "8d41e0a7",
    };
  }),
};

/** 账户卡那一条（不带明细）。 */
export function demoSlotSummary(days: number): TokenSummary {
  return withTranscriptExtras(
    demoSummarize(demoTokenUsage.buckets, days, demoTokenUsage.unattributed),
    days,
    false,
  );
}

function withTranscriptExtras(
  s: TokenSummary,
  days: number,
  detail: boolean,
): TokenSummary {
  const today = todayYmd();
  const from = sinceDay(days, today);
  const inRange = (d: string) => !from || d >= from;
  s.empty_replies = demoTokenUsage.empty_days
    .filter((d) => inRange(d.day))
    .reduce((a, d) => a + d.replies, 0);
  s.coverage = {
    sessions: demoTokenUsage.sessions,
    files_read: demoTokenUsage.files_read,
    files_failed: demoTokenUsage.files_failed,
    duplicates: demoTokenUsage.duplicates,
    undated: demoTokenUsage.undated,
  };
  const recent = demoTokenUsage.recent.filter((r) => inRange(r.day));
  s.last_at = recent[0] ? `${recent[0].day} ${recent[0].time}` : null;
  if (!detail) {
    s.prices_used = [];
    return s;
  }
  s.recent = recent
    .slice(0, 200)
    .map((r) => ({ ...r, cost_usd: costOf(r.model, r) }));
  if (days === 1) {
    const byHour = new Map<number, TokenSummary["hourly"][number]>();
    for (const b of demoTokenUsage.hourly.filter((h) => h.day === today)) {
      const h = byHour.get(b.hour) ?? {
        hour: b.hour,
        input: 0,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        cache_write_1h: 0,
        messages: 0,
        cost_usd: null,
      };
      h.input += b.input;
      h.output += b.output;
      h.cache_read += b.cache_read;
      h.cache_write += b.cache_write;
      h.cache_write_1h += b.cache_write_1h;
      h.messages += b.messages;
      h.cost_usd = add(h.cost_usd, costOf(b.model, b));
      byHour.set(b.hour, h);
    }
    s.hourly = [...byHour.values()].sort((a, b) => a.hour - b.hour);
  }
  return s;
}

/** 另一个槽位的量（「按账户」表里第二行）。 */
const altBuckets: TokenBucket[] = [0, 1, 2].map((i) => ({
  day: dayAgo(i),
  model: "claude-opus-5",
  input: 90,
  output: 21_000,
  cache_write: 96_000,
  cache_write_1h: 96_000,
  cache_read: 3_400_000,
  messages: 36,
}));

/** 用量明细页（`accounts_usage_overview`）。 */
export function demoUsageOverview(label: string, days: number): UsageOverview {
  const summary = withTranscriptExtras(
    demoSummarize(demoTokenUsage.buckets, days, demoTokenUsage.unattributed),
    days,
    true,
  );
  const row = (
    kind: AccountSpend["kind"],
    l: string,
    s: TokenSummary,
  ): AccountSpend => ({
    kind,
    label: l,
    messages: s.messages,
    input: s.input,
    output: s.output,
    cache_write: s.cache_write,
    cache_read: s.cache_read,
    cost_usd: s.cost_usd,
    unpriced_messages: s.unpriced.reduce((a, u) => a + u.messages, 0),
  });
  const unattr = demoSummarize(demoTokenUsage.unattributed, days);
  const by_account: AccountSpend[] = [
    row("slot", label || "demo-main", summary),
    row("slot", "demo-alt", demoSummarize(altBuckets, days)),
    row("slot", "demo-empty", demoSummarize([], days)),
  ];
  if (unattr.messages > 0) by_account.push(row("unattributed", "", unattr));
  return { summary, by_account };
}

/** GPT 一侧（`codex_usage_summary`）。模型取自 `turn_context`，缓存写恒为 0。 */
export function demoCodexSummary(days: number): CodexUsageSummary {
  const buckets: TokenBucket[] = [0, 1, 3, 5].map((i) => ({
    day: dayAgo(i),
    model: "gpt-5.6-sol",
    input: 22_000 - i * 1_000,
    output: 3_100 - i * 90,
    cache_write: 0,
    cache_write_1h: 0,
    cache_read: 15_800 - i * 600,
    messages: 6,
  }));
  return {
    summary: demoSummarize(buckets, days),
    long_context_requests: 0,
    sessions: 8,
    files_read: 10,
    files_failed: 0,
    duplicates: 2,
    incomplete: 0,
    checked_at: `${todayYmd()} 10:35`,
  };
}
