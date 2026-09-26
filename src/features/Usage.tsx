/**
 * 用量明细页 `/usage`（0.32.0 起；2026-09-24 按 cc-switch 的信息量重排）。
 *
 * 使用者 09-24 的原话：「ccswitch 里显示的数据本面板也要有，这个趋势图不好，
 * 要看见每天、7 天、30 天花了多少刀的额度」。所以这一页现在是：
 *
 * 1. **花了多少**：今天 / 近 7 天 / 近 30 天三格，不跟着下面选的时间档走；
 * 2. 概览（cc-switch 的 UsageHero）：等价费用、真实消耗 Tokens、回复数，再一行四类 token 与命中率；
 * 3. 趋势：柱状图（`ui/BarChart`），按天，「今天」按小时，美元 / Token 可切；
 * 4. 按天明细表（图的表格版）、费用构成、按模型、按账户（Claude）、最近请求（cc-switch 的请求日志）；
 * 5. 配额与统计说明。
 *
 * # 为什么是一个**路由**，不是又一个弹窗
 *
 * 三个账户页是**固定高度、不滚动**的（`test:ui` 钉着 680×640 起四档不许裁切），
 * 右下那张用量卡只有四格的位置。这一页**可以滚动**，从三张用量卡的页脚按钮进来 ——
 * 它不进 `NAV`，跟 `/onboarding` 同一种「有路由没导航项」。
 *
 * # ⛔ 这一页一个联网请求都没有
 *
 * Claude 的 `accounts_usage_overview`、GPT 的 `codex_usage_summary` + `codex_rate_limits`、
 * 反重力的 `antigravity_usage`，全是读本机文件。美元用的价是面板启动时抓回来存在库里的官方价
 * （没有就用内置快照），这一页不去抓。
 *
 * # ⛔ 四条显示口径，跟别处一字不差
 *
 * 1. **美元是按官方 API 价折算的，订阅不按这个收费。** 这句话跟数字放在一起，不许藏进悬停。
 * 2. **「没测到」不是 0。** 没有记录的那天不画柱、表里写「—」；读不到的比例显示「—」，不显示 0%。
 * 3. **认不出价的模型不进美元，单独列出来** —— 美元旁边看得到有多少没算进去。
 * 4. **覆盖率必须写出来。** 读了几个文件、几个没读成、归不到槽位的有多少、价是抓回来的还是快照。
 */
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { ArrowLeft, Gauge as GaugeIcon, RefreshCw } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";

import { api } from "../lib/api";
import { antigravityApi } from "../lib/antigravity";
import { codexApi } from "../lib/codexAccounts";
import { AG_R } from "../lib/antigravity";
import { R } from "../lib/resources";
import { CODEX_R } from "../lib/codexAccounts";
import { setSession, useResource } from "../lib/store";
import { resetIn } from "../lib/antigravityQuota";
import { daysFrom, lastDays, shortDay, today as todayYmd } from "../lib/dates";
import { slotName } from "../lib/slotName";
import type { AccountSpend } from "../lib/generated/AccountSpend";
import type { AntigravityIdentity } from "../lib/generated/AntigravityIdentity";
import type { AntigravityUsage } from "../lib/generated/AntigravityUsage";
import type { CodexRateLimitScan } from "../lib/generated/CodexRateLimitScan";
import type { CodexUsageSummary } from "../lib/generated/CodexUsageSummary";
import type { SpendWindow } from "../lib/generated/SpendWindow";
import type { TokenSummary } from "../lib/generated/TokenSummary";
import { SIDES, SIDE_KEY, type Side } from "../lib/side";
import SlotUsageBars from "./overview/SlotUsage";
import {
  BarChart,
  Button,
  Card,
  Gauge,
  PageHeader,
  Pill,
  type BarDatum,
} from "../ui";

const NUM = new Intl.NumberFormat("zh-CN");
const RANGES: [number, string][] = [
  [1, "今天"],
  [7, "7 天"],
  [30, "30 天"],
  [0, "全部"],
];
/** 自己重读的间隔。跟账户卡一样：一个叫「今天」的数字不会自己往前走。 */
const REFRESH_MS = 60_000;
/** 最近请求每页几条。 */
const PAGE = 50;

function short(n: number): string {
  if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(2) + "B";
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
  return NUM.format(n);
}
/** ⛔ `null` → 「—」，**绝不显示 0%** —— 那是断言，没数据不是。 */
function pct(v: number | null | undefined): string {
  return v == null ? "—" : `${Math.round(v * 100)}%`;
}
const USD = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});
/** 美元。`null` → 「—」；不到一分钱的写「<$0.01」，不写成 $0.00。 */
function usd(v: number | null | undefined): string {
  if (v == null) return "—";
  if (v > 0 && v < 0.01) return "<$0.01";
  return USD.format(v);
}
/** 坐标轴上的美元：去掉多余的零。 */
function usdTick(v: number): string {
  if (v >= 1000) return `$${Number((v / 1000).toFixed(1))}K`;
  return `$${Number(v.toFixed(2))}`;
}
function tokenTotal(x: {
  input: number;
  output: number;
  cache_read: number;
  cache_write: number;
}): number {
  return x.input + x.output + x.cache_read + x.cache_write;
}

function Stat({
  name,
  value,
  sub,
  title,
}: {
  name: string;
  value: string;
  sub?: ReactNode;
  title?: string;
}) {
  return (
    <div className="ustat" title={title}>
      <span className="ustat-name">{name}</span>
      <span className="ustat-value">{value}</span>
      {sub != null && sub !== "" && <span className="ustat-sub">{sub}</span>}
    </div>
  );
}

/** 「花了多少」里的一格。没有记录说「没有记录」，不说 $0。 */
function SpendTile({
  name,
  w,
  unit,
}: {
  name: string;
  w: SpendWindow;
  unit: string;
}) {
  const value =
    w.messages === 0 ? "没有记录" : w.usd == null ? "没有官方价" : usd(w.usd);
  const sub =
    w.messages === 0
      ? "本机这段时间没有记录"
      : `${NUM.format(w.messages)} 条${unit}` +
        (w.unpriced_messages > 0
          ? ` · ${NUM.format(w.unpriced_messages)} 条没有官方价、未计入`
          : "");
  return <Stat name={name} value={value} sub={sub} />;
}

type Loaded =
  | {
      side: "claude";
      summary: TokenSummary;
      byAccount: AccountSpend[];
    }
  | {
      side: "gpt";
      summary: TokenSummary;
      codex: CodexUsageSummary;
      limits: CodexRateLimitScan | null;
    }
  | { side: "antigravity"; summary: TokenSummary; ag: AntigravityUsage };

/** 趋势图的数据：7 / 30 天列满每一天（没记录的是 `null`）；「今天」按小时；「全部」从第一天到今天。 */
function trendData(
  s: TokenSummary,
  days: number,
  metric: "usd" | "tokens",
  unit: string,
): { data: BarDatum[]; highlight?: string; hourly: boolean } {
  const today = todayYmd();
  const value = (x: {
    cost_usd: number | null;
    input: number;
    output: number;
    cache_read: number;
    cache_write: number;
  }) => (metric === "usd" ? x.cost_usd : tokenTotal(x));
  const rows = (x: {
    messages: number;
    input: number;
    output: number;
    cache_read: number;
    cache_write: number;
    cost_usd: number | null;
  }): [string, string][] => [
    [unit, NUM.format(x.messages)],
    [
      metric === "usd" ? "Token" : "美元",
      metric === "usd" ? short(tokenTotal(x)) : usd(x.cost_usd),
    ],
    ["输入 / 输出", `${short(x.input)} / ${short(x.output)}`],
    ["缓存写 / 读", `${short(x.cache_write)} / ${short(x.cache_read)}`],
  ];

  if (days === 1 && s.hourly.length > 0) {
    const byHour = new Map(s.hourly.map((h) => [h.hour, h]));
    const now = new Date().getHours();
    const data: BarDatum[] = [];
    for (let h = 0; h <= now; h++) {
      const x = byHour.get(h);
      const hh = String(h).padStart(2, "0");
      data.push({
        key: String(h),
        axis: hh,
        title: `${hh}:00–${hh}:59`,
        value: x ? value(x) : null,
        rows: x ? rows(x) : undefined,
      });
    }
    return { data, highlight: String(now), hourly: true };
  }

  const byDay = new Map(s.daily.map((d) => [d.day, d]));
  const span =
    days > 0
      ? lastDays(days, today)
      : s.daily.length > 0
        ? daysFrom(s.daily[0].day, today)
        : [];
  const data = span.map((day) => {
    const x = byDay.get(day);
    return {
      key: day,
      axis: shortDay(day),
      title: day === today ? `${day}（今天）` : day,
      value: x ? value(x) : null,
      rows: x ? rows(x) : undefined,
    };
  });
  return { data, highlight: today, hourly: false };
}

export default function Usage() {
  const [params, setParams] = useSearchParams();
  // 地址里的 `side` 不认识就当 Claude —— 原来原样强转，一个写错的值会一路传到
  // 总览，落进「不是 Claude 也不是 GPT」那个分支，显示成反重力页。
  const rawSide = params.get("side");
  const side: Side = SIDES.some((s) => s.id === rawSide)
    ? (rawSide as Side)
    : "claude";
  // 用量明细是「官方账户」底下的一页：它显示的是哪一边，侧栏高亮、
  // 「回账户页」回的就是哪一边。
  useEffect(() => {
    setSession<Side>(SIDE_KEY, side);
  }, [side]);
  const [days, setDays] = useState(7);
  const [metric, setMetric] = useState<"usd" | "tokens">("usd");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [data, setData] = useState<Loaded | null>(null);
  const [page, setPage] = useState(0);

  const accounts = useResource("accounts", R.accounts);
  const codexAccounts = useResource("codexAccounts", CODEX_R.accounts);
  const agStatus = useResource("antigravity", AG_R.status);

  const slots = accounts.data?.slots ?? [];
  const activeClaude = slots.find((s) => s.active);
  const activeGpt = codexAccounts.data?.slots.find((s) => s.active);
  const agIdentity = agStatus.data?.identity ?? null;
  const claudeLabel = activeClaude?.label ?? "";
  const gptId = activeGpt?.id ?? "";

  /**
   * 每一次读的序号：只认最后发出去的那一次（2026-09-25）。
   *
   * 原来没有：先点「30 天」再马上点「7 天」，30 天那一遍扫得慢、后回来，把 7 天的结果盖掉 ——
   * 标签写着「7 天」，数是 30 天的，要等下一分钟自动重读才对；切边时也一样，慢的那边回来把页面清空。
   * `busy` 也被先回来的那一次提前清掉。
   */
  const loadSeq = useRef(0);
  const load = useCallback(async () => {
    const seq = ++loadSeq.current;
    const latest = () => seq === loadSeq.current;
    setBusy(true);
    setErr("");
    try {
      if (side === "claude") {
        if (!claudeLabel) {
          if (latest()) setData(null);
          return;
        }
        const o = await api.accountsUsageOverview(claudeLabel, days);
        if (latest())
          setData({ side, summary: o.summary, byAccount: o.by_account });
      } else if (side === "gpt") {
        if (!gptId) {
          if (latest()) setData(null);
          return;
        }
        const [codex, limits] = await Promise.all([
          codexApi.usageSummary(gptId, days),
          codexApi.rateLimits(gptId),
        ]);
        if (latest()) setData({ side, summary: codex.summary, codex, limits });
      } else {
        const ag = await antigravityApi.usage(days);
        if (latest()) setData({ side, summary: ag.summary, ag });
      }
    } catch (e) {
      if (latest()) setErr(e instanceof Error ? e.message : String(e));
    } finally {
      if (latest()) setBusy(false);
    }
  }, [side, days, claudeLabel, gptId]);

  // 换了一边就把上一边的数清掉 —— 同一块屏幕不许短暂显示「另一边的数」。
  useEffect(() => {
    setData(null);
    setPage(0);
  }, [side]);
  useEffect(() => {
    setPage(0);
  }, [days]);
  useEffect(() => {
    void load();
    const timer = window.setInterval(() => void load(), REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [load]);

  const sideName = SIDES.find((s) => s.id === side)?.label ?? side;
  const unit = side === "gpt" ? "请求" : "回复";
  const s = data && data.side === side ? data.summary : null;
  const rangeName = RANGES.find(([d]) => d === days)?.[1] ?? "";
  const lowerBound =
    data?.side === "gpt" && data.codex.long_context_requests > 0;

  const trend = useMemo(
    () => (s ? trendData(s, days, metric, unit) : null),
    [s, days, metric, unit],
  );

  // 快照里已经过了重置时刻的那一格是重置前的数，不参加比紧（2026-09-25）。
  const tightGpt =
    data?.side === "gpt"
      ? [data.limits?.found?.primary, data.limits?.found?.secondary]
          .filter((w): w is NonNullable<typeof w> => !!w)
          .filter((w) => !w.window.reset_passed)
          .sort((a, b) => b.window.used - a.window.used)[0]
      : undefined;

  // 按天明细：7 / 30 天列满每一天（新的在前）；「全部」只列有记录的；「今天」就是今天一行。
  const dayRows = useMemo(() => {
    if (!s) return [];
    const byDay = new Map(s.daily.map((d) => [d.day, d]));
    const span =
      days > 0 ? lastDays(days, todayYmd()) : s.daily.map((d) => d.day);
    return span
      .slice()
      .reverse()
      .map((day) => ({ day, x: byDay.get(day) ?? null }));
  }, [s, days]);

  const noAccount =
    (side === "claude" && !claudeLabel && accounts.data) ||
    (side === "gpt" && !gptId && codexAccounts.data);

  return (
    <div className="qb-page qb-usage">
      <PageHeader
        title={`${sideName} · 用量明细`}
        sub="全部读自官方客户端写在本机的文件，零网络请求。美元按官方 API 价折算 —— 订阅不按这个收费。"
        actions={
          <div className="flex flex-wrap items-center gap-1.5">
            <Link to="/" className="btn btn--sm">
              <ArrowLeft size={12} /> 回账户页
            </Link>
            {SIDES.map((x) => (
              <Button
                key={x.id}
                size="sm"
                variant={side === x.id ? "primary" : "default"}
                onClick={() => setParams({ side: x.id })}
              >
                {x.label}
              </Button>
            ))}
          </div>
        }
      />

      {err && (
        <p role="alert" className="notice notice--danger mb-3">
          {err}
        </p>
      )}
      {noAccount && (
        <p className="notice mb-3">
          {side === "claude"
            ? "还没有激活的 Claude 槽位 —— 回账户页选一个槽位再来。"
            : "还没有激活的 GPT 槽位 —— 回账户页选一个槽位再来。"}
        </p>
      )}

      {/* ------------------------------------------------ 花了多少（三档固定） */}
      <Card
        title={
          <>
            <GaugeIcon size={14} />
            花了多少 · 按官方 API 价折算
          </>
        }
      >
        <div className={`qb-usage-spend${busy && s ? " qb-usage-stale" : ""}`}>
          <SpendTile
            name="今天"
            w={s?.spend.today ?? blankWindow}
            unit={unit}
          />
          <SpendTile
            name="近 7 天"
            w={s?.spend.last_7d ?? blankWindow}
            unit={unit}
          />
          <SpendTile
            name="近 30 天"
            w={s?.spend.last_30d ?? blankWindow}
            unit={unit}
          />
        </div>
        <p className="notice qb-usage-caveat">
          同样的 token 走官方 API 要付的钱 ——
          <strong>订阅账户付的是月费，不按这个收费</strong>。
          拿它看「这个月的订阅用出去了多少」，不是账单。
          {lowerBound && (
            <>
              {" "}
              GPT 这边有 {NUM.format(data.codex.long_context_requests)}{" "}
              次请求的输入超过 272K（长上下文那一档输入价更高），美元是下限。
            </>
          )}
        </p>
      </Card>

      {/* ------------------------------------------------ 时间档（下面全部跟着它） */}
      <div className="mt-3 flex flex-wrap items-center gap-1.5">
        <span className="usage-ranges" role="group" aria-label="时间范围">
          {RANGES.map(([d, name]) => (
            <Button
              key={d}
              size="sm"
              variant={days === d ? "primary" : "default"}
              aria-pressed={days === d}
              onClick={() => setDays(d)}
            >
              {name}
            </Button>
          ))}
        </span>
        <Button
          size="sm"
          icon={<RefreshCw size={12} />}
          loading={busy}
          aria-label="重新统计"
          title="立刻重读一遍。这一页每分钟也会自己重读。"
          onClick={() => void load()}
        />
        <span className="notice">
          下面全部按「{rangeName}」统计 · 每分钟自己重读
        </span>
      </div>

      <div className={busy && s ? "qb-usage-stale" : undefined}>
        {/* ---------------------------------------------- 概览 */}
        <Card title="概览" className="mt-3">
          <div className="ustats">
            <Stat
              name="等价费用"
              value={
                s
                  ? lowerBound
                    ? `≥ ${usd(s.cost_usd)}`
                    : usd(s.cost_usd)
                  : "—"
              }
              sub={
                s
                  ? s.cost_usd == null && s.messages > 0
                    ? "这一档的模型都没有官方价"
                    : s.unpriced_models > 0
                      ? `${s.unpriced_models} 个模型没有官方价，未计入`
                      : `${rangeName} · 按 API 价折算`
                  : undefined
              }
            />
            <Stat
              name="真实消耗 Tokens"
              value={s ? short(tokenTotal(s)) : "—"}
              sub="输入 + 输出 + 缓存写 + 缓存读"
              title={s ? NUM.format(tokenTotal(s)) : undefined}
            />
            <Stat
              name={side === "gpt" ? "请求数" : "回复数"}
              value={s ? NUM.format(s.messages) : "—"}
              sub={
                s
                  ? s.empty_replies > 0
                    ? `另有 ${NUM.format(s.empty_replies)} 条报错没计入`
                    : s.last_at
                      ? `最后一条 ${s.last_at.slice(5, 16)}`
                      : rangeName
                  : undefined
              }
            />
          </div>
          <div className="ustats mt-2">
            <Stat name="新增输入" value={s ? short(s.input) : "—"} />
            <Stat name="输出" value={s ? short(s.output) : "—"} />
            <Stat
              name="缓存写入"
              value={side === "gpt" ? "不上报" : s ? short(s.cache_write) : "—"}
              sub={
                side === "gpt"
                  ? "OpenAI 的记录不分缓存写"
                  : s && s.cache_write > 0
                    ? `其中 1 小时档 ${short(s.cache_write_1h)}`
                    : undefined
              }
            />
            <Stat
              name="缓存命中"
              value={s ? short(s.cache_read) : "—"}
              sub={
                s
                  ? `命中率 ${pct(s.hit_rate)}` +
                    (s.saved_usd != null ? ` · 省下 ≈${usd(s.saved_usd)}` : "")
                  : undefined
              }
              title="命中率 = 缓存读 ÷（输入 + 缓存读 + 缓存写），输出不进分母。省下 = 缓存读 ×（输入价 − 缓存读价）。"
            />
          </div>
        </Card>

        {/* ---------------------------------------------- 趋势 */}
        <Card
          title={trend?.hourly ? "趋势 · 今天按小时" : "趋势 · 按天"}
          className="mt-3"
          actions={
            <span
              className="qb-usage-switch"
              role="group"
              aria-label="柱子的高度"
            >
              <Button
                size="sm"
                variant={metric === "usd" ? "primary" : "default"}
                aria-pressed={metric === "usd"}
                onClick={() => setMetric("usd")}
              >
                美元
              </Button>
              <Button
                size="sm"
                variant={metric === "tokens" ? "primary" : "default"}
                aria-pressed={metric === "tokens"}
                onClick={() => setMetric("tokens")}
              >
                Token
              </Button>
            </span>
          }
        >
          {s && trend ? (
            <>
              <BarChart
                data={trend.data}
                format={metric === "usd" ? usdTick : short}
                valueFormat={metric === "usd" ? usd : short}
                label={`${sideName} ${rangeName}${trend.hourly ? "按小时" : "按天"}的${metric === "usd" ? "等价费用" : "Token 合计"}`}
                highlight={trend.highlight}
                empty="这一档时间范围里没有记录。"
              />
              <p className="notice mt-1">
                {trend.hourly
                  ? "柱子是每个小时的合计；没有柱子的小时本机没有记录。"
                  : "没有柱子的那天本机没有记录（不是 $0 —— 可能是那天没用，也可能是记录不在这台机器上）。"}
                {metric === "usd" &&
                  " 悬停或用方向键看每一根的明细；完整的数在下面的表里。"}
              </p>
            </>
          ) : (
            <p className="notice">
              {busy ? "正在读本机记录…" : "这一档还没有数据。"}
            </p>
          )}
        </Card>

        {/* ---------------------------------------------- 按天明细（图的表格版） */}
        <Card title={days === 1 ? "今天" : "按天明细"} className="mt-3">
          {s && dayRows.length > 0 ? (
            <div className="qb-usage-tw">
              <table>
                <thead>
                  <tr>
                    <th>日期</th>
                    <th className="num">{unit}</th>
                    <th className="num">输入</th>
                    <th className="num">输出</th>
                    <th className="num">缓存写</th>
                    <th className="num">缓存读</th>
                    <th className="num">合计</th>
                    <th className="num">美元</th>
                  </tr>
                </thead>
                <tbody>
                  {dayRows.map(({ day, x }) =>
                    x ? (
                      <tr key={day}>
                        <td>{day === todayYmd() ? `${day}（今天）` : day}</td>
                        <td className="num">{NUM.format(x.messages)}</td>
                        <td className="num">{short(x.input)}</td>
                        <td className="num">{short(x.output)}</td>
                        <td className="num">{short(x.cache_write)}</td>
                        <td className="num">{short(x.cache_read)}</td>
                        <td className="num">{short(tokenTotal(x))}</td>
                        <td className="num">{usd(x.cost_usd)}</td>
                      </tr>
                    ) : (
                      <tr key={day} className="is-empty">
                        <td>{day === todayYmd() ? `${day}（今天）` : day}</td>
                        <td colSpan={7}>没有记录</td>
                      </tr>
                    ),
                  )}
                </tbody>
                {dayRows.length > 1 && (
                  <tfoot>
                    <tr>
                      <td>合计</td>
                      <td className="num">{NUM.format(s.messages)}</td>
                      <td className="num">{short(s.input)}</td>
                      <td className="num">{short(s.output)}</td>
                      <td className="num">{short(s.cache_write)}</td>
                      <td className="num">{short(s.cache_read)}</td>
                      <td className="num">{short(tokenTotal(s))}</td>
                      <td className="num">
                        {lowerBound ? `≥ ${usd(s.cost_usd)}` : usd(s.cost_usd)}
                      </td>
                    </tr>
                  </tfoot>
                )}
              </table>
            </div>
          ) : (
            <p className="notice">这一档还没有数据。</p>
          )}
        </Card>

        {/* ---------------------------------------------- 费用构成 */}
        <Card title="费用构成" className="mt-3">
          {s?.cost_parts ? (
            <CostMix
              parts={s.cost_parts}
              tokens={{
                input: s.input,
                output: s.output,
                cache_write: s.cache_write,
                cache_read: s.cache_read,
              }}
            />
          ) : (
            <p className="notice">
              {s && s.messages > 0
                ? "这一档的模型都没有官方价，拆不出美元。"
                : "这一档还没有数据。"}
            </p>
          )}
        </Card>

        {/* ---------------------------------------------- 按模型 */}
        <Card title="消耗分布 · 按模型" className="mt-3">
          {s && s.models.length > 0 ? (
            <div className="qb-usage-tw">
              <table>
                <thead>
                  <tr>
                    <th>模型</th>
                    <th className="num">{unit}</th>
                    <th className="num">Tokens</th>
                    <th className="num">美元</th>
                    <th className="num">占美元</th>
                    <th className="num">平均每条</th>
                  </tr>
                </thead>
                <tbody>
                  {s.models.map((m) => (
                    <tr key={m.model}>
                      <td className="clip" title={m.model}>
                        {m.model}
                      </td>
                      <td className="num">{NUM.format(m.messages)}</td>
                      <td className="num">{short(tokenTotal(m))}</td>
                      <td className="num">
                        {m.cost_usd == null ? "没有官方价" : usd(m.cost_usd)}
                      </td>
                      <td className="num">
                        {m.cost_usd != null && s.cost_usd
                          ? `${((m.cost_usd / s.cost_usd) * 100).toFixed(1)}%`
                          : "—"}
                      </td>
                      <td className="num">
                        {m.cost_usd != null && m.messages > 0
                          ? usd(m.cost_usd / m.messages)
                          : "—"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <p className="notice">这一档还没有数据。</p>
          )}
        </Card>

        {/* ---------------------------------------------- 按账户（Claude） */}
        {data?.side === "claude" && data.byAccount.length > 0 && (
          <Card title="按账户" className="mt-3">
            <div className="qb-usage-tw">
              <table>
                <thead>
                  <tr>
                    <th>账户</th>
                    <th className="num">回复</th>
                    <th className="num">Tokens</th>
                    <th className="num">美元</th>
                  </tr>
                </thead>
                <tbody>
                  {data.byAccount.map((a) => {
                    const slot = slots.find((x) => x.label === a.label);
                    const name =
                      a.kind === "slot"
                        ? slotName(slot?.email, a.label)
                        : a.kind === "unattributed"
                          ? "归不到槽位"
                          : "面板以外的账户";
                    const current =
                      a.kind === "slot" && a.label === claudeLabel;
                    return (
                      <tr
                        key={`${a.kind}-${a.label}`}
                        className={
                          a.messages === 0
                            ? "is-empty"
                            : current
                              ? "is-current"
                              : undefined
                        }
                      >
                        <td className="clip" title={name}>
                          {name}
                          {current && "（当前）"}
                        </td>
                        <td className="num">{NUM.format(a.messages)}</td>
                        <td className="num">
                          {short(
                            a.input + a.output + a.cache_write + a.cache_read,
                          )}
                        </td>
                        <td className="num">
                          {a.messages === 0 ? "—" : usd(a.cost_usd)}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            <p className="notice mt-2">
              桌面端 Code
              页的会话按桌面端自己记下的账户归属；三级都归不出主的（多是终端里直接起的旧会话）
              单独一行，<strong>不摊给任何账户</strong>
              。切过账户的那一天，量会分在两个账户上。
            </p>
          </Card>
        )}

        {/* ---------------------------------------------- 最近请求 */}
        <Card title="最近请求" className="mt-3">
          {data?.side === "claude" ? (
            s && s.recent.length > 0 ? (
              <RecentTable rows={s.recent} page={page} onPage={setPage} />
            ) : (
              <p className="notice">这一档里没有回复。</p>
            )
          ) : (
            <p className="notice">
              {side === "gpt"
                ? "Codex 的会话记录只写累计数，逐条的明细这里拿不出来 —— 按天、按模型的数在上面。"
                : "反重力的记录库按对话存，这里只列按天、按模型的合计。"}
            </p>
          )}
        </Card>

        {/* ---------------------------------------------- 配额 */}
        <Card title="配额" className="mt-3">
          {side === "claude" ? (
            activeClaude ? (
              <>
                <SlotUsageBars usage={activeClaude.usage ?? null} />
                <p className="notice mt-2">
                  读自 Claude 桌面端写在本机的样本（约 15
                  分钟一条，桌面端没开时不写）或 Claude Code 留下的快照，
                  <strong>零网络</strong>。刷新在账户页的槽位行上。
                </p>
              </>
            ) : (
              <p className="notice">还没有激活的 Claude 槽位。</p>
            )
          ) : side === "gpt" ? (
            data?.side === "gpt" && data.limits?.found ? (
              <>
                <div className="slotusage">
                  {[data.limits.found.primary, data.limits.found.secondary]
                    .filter((w): w is NonNullable<typeof w> => !!w)
                    .map((w, i) =>
                      w.window.reset_passed ? (
                        <Gauge
                          key={`${w.name}-${i}`}
                          name={w.name}
                          used={null}
                          note="已重置，快照是重置前的数"
                        />
                      ) : (
                        <Gauge
                          // 两格都没带时长时 `window_minutes` 同是 0，拿它当 key 会撞（2026-09-25）。
                          key={`${w.name}-${i}`}
                          name={w.name}
                          used={w.window.used}
                          binding={tightGpt === w}
                          extra={
                            w.window.resets_at ? (
                              <span className="gauge-reset">
                                ↻{" "}
                                {new Date(w.window.resets_at).toLocaleString(
                                  "zh-CN",
                                  {
                                    month: "2-digit",
                                    day: "2-digit",
                                    hour: "2-digit",
                                    minute: "2-digit",
                                  },
                                )}
                              </span>
                            ) : undefined
                          }
                        />
                      ),
                    )}
                </div>
                <p className="notice mt-2">
                  读自 Codex 自己写在会话记录里的 <code>rate_limits</code>
                  ，零网络。
                  <strong>它是上一次请求时的快照</strong>（
                  {data.limits.found.age_minutes} 分钟前），不是此刻。
                </p>
              </>
            ) : (
              <p className="notice">
                {/* 读不出来 ≠ 没有（§7.17，2026-09-25）：会话目录是联结点、文件打不开时，
                    原来照样写「这很正常」。 */}
                {data?.side === "gpt" &&
                data.limits &&
                data.limits.files_failed > 0
                  ? `本机会话记录有 ${data.limits.files_failed} 份读不出来（${data.limits.first_error ?? "原因不明"}），判不了有没有额度记录。`
                  : "还没有带额度信息的会话记录 —— 走中转或 API Key 的会话不带额度，这很正常。"}
              </p>
            )
          ) : agIdentity ? (
            <AgQuota identity={agIdentity} />
          ) : (
            <p className="notice">
              反重力 IDE 登录后这里显示各模型的剩余额度。
            </p>
          )}
        </Card>

        {/* ---------------------------------------------- 统计说明 */}
        <Card title="统计说明" className="mt-3">
          <ul className="notice list-disc space-y-1 pl-5">
            <li>
              <strong>美元是按官方 API 价折算的等价费用</strong>：
              <code>输入×输入价 + 输出×输出价 + 缓存写×写价 + 缓存读×读价</code>
              。 缓存写分 5 分钟、1 小时两档（1 小时档 = 输入价 ×2），Claude
              Code 的缓存写基本都是 1 小时档。
              {s?.priced_from_snapshot != null && (
                <>
                  {" "}
                  这一档用的是
                  {s.priced_from_snapshot
                    ? "编译进面板的价目快照（可能过期）"
                    : "面板启动时抓回来的官方价"}
                  。
                </>
              )}
            </li>
            {s && s.unpriced.length > 0 && (
              <li>
                <strong>没有官方价、没算进美元的：</strong>
                {s.unpriced
                  .map(
                    (u) =>
                      `${u.model}（${NUM.format(u.messages)} 条、${short(u.tokens)} token）`,
                  )
                  .join("、")}
                。不拿别的模型的价去凑。
              </li>
            )}
            {s && s.empty_replies > 0 && (
              <li>
                另有 {NUM.format(s.empty_replies)} 条回复四类 token 全是
                0（报错、被打断的请求， 比如「请重新登录」），
                <strong>不算回复、不算钱</strong>。
              </li>
            )}
            <li>
              <strong>缓存命中率的分母里没有输出</strong>（
              <code>缓存读 ÷（输入 + 缓存读 + 缓存写）</code>）——
              输出是生成出来的，没有「命中」一说。
            </li>
            {!!s?.unattributed && (
              <li>
                有 {short(s.unattributed)} token（
                {NUM.format(s.unattributed_messages)} 条回复）
                <strong>归不到任何槽位</strong>，没有摊给这个账户 ——
                多半是终端里直接起、没经过面板的旧会话。
              </li>
            )}
            {s?.coverage && (
              <li>
                读了 {NUM.format(s.coverage.files_read)} 个会话转写
                {s.coverage.files_failed > 0 &&
                  `，${s.coverage.files_failed} 个没读成`}
                ，重放的 {NUM.format(s.coverage.duplicates)}{" "}
                条按「同一条消息只算一次」去掉了
                {s.coverage.undated > 0 &&
                  `，${s.coverage.undated} 条没有时刻、落不进任何一天`}
                。
              </li>
            )}
            {data?.side === "gpt" && (
              <li>
                扫了 {data.codex.files_read} 个会话记录
                {data.codex.files_failed > 0 &&
                  `，${data.codex.files_failed} 个没读成`}
                {data.codex.duplicates > 0 &&
                  `，去重 ${data.codex.duplicates} 条`}
                {data.codex.incomplete > 0 &&
                  `，${data.codex.incomplete} 条计数回退（没重复计费）`}
                。<strong>只统计该槽位的本机会话</strong> ——
                桌面端的云端任务不写本机文件。 模型取自会话里的{" "}
                <code>turn_context</code>。
              </li>
            )}
            {data?.side === "antigravity" && (
              <li>
                扫了 {data.ag.files_read} 个记录库
                {data.ag.files_failed > 0 &&
                  `，${data.ag.files_failed} 个没读成`}
                {data.ag.legacy_skipped > 0 &&
                  `，跳过 ${data.ag.legacy_skipped} 个旧归档`}
                {data.ag.duplicates > 0 && `，去重 ${data.ag.duplicates} 条`}
                {data.ag.incomplete > 0 &&
                  `，${data.ag.incomplete} 条解不出用量或时间`}
                。 字段含义<strong>按实测推断</strong>（没有官方 schema）。
                <br />
                扫描目录：<code>{data.ag.scanned_dirs.join(" · ")}</code>
              </li>
            )}
            <li>
              这不是「这个账户一共用了多少」：别的机器上跑的不在内，归不出属的那部分单独列。
            </li>
          </ul>
          {s && s.prices_used.length > 0 && (
            <PriceTable prices={s.prices_used} />
          )}
        </Card>
      </div>

      <p className="notice mt-3">
        <Pill tone="default">零网络</Pill>{" "}
        这一页的每一个数字都来自官方客户端自己写在本机的文件。
      </p>
    </div>
  );
}

const blankWindow: SpendWindow = {
  usd: null,
  messages: 0,
  unpriced_messages: 0,
};

/** 四类的美元与 token 各占多少。一行一类，同一个颜色（类别不是系列，不上四种颜色）。 */
function CostMix({
  parts,
  tokens,
}: {
  parts: NonNullable<TokenSummary["cost_parts"]>;
  tokens: {
    input: number;
    output: number;
    cache_write: number;
    cache_read: number;
  };
}) {
  const total =
    parts.input + parts.output + parts.cache_write + parts.cache_read;
  const tokenAll =
    tokens.input + tokens.output + tokens.cache_write + tokens.cache_read;
  const rows: [string, number, number][] = [
    ["缓存读", parts.cache_read, tokens.cache_read],
    ["缓存写", parts.cache_write, tokens.cache_write],
    ["输出", parts.output, tokens.output],
    ["新增输入", parts.input, tokens.input],
  ];
  return (
    <div className="qb-usage-mix">
      <div className="qb-usage-mixrow qb-usage-mixhead" aria-hidden="true">
        <span className="qb-usage-mixname">类别</span>
        <span className="gauge-track qb-usage-mixspacer" />
        <span className="gauge-pct">美元</span>
        <span className="qb-usage-mixpct">占美元</span>
        <span className="qb-usage-mixpct">Token</span>
      </div>
      {rows.map(([name, dollars, tok]) => (
        <div key={name} className="qb-usage-mixrow">
          <span className="qb-usage-mixname">{name}</span>
          <span className="gauge-track">
            <span
              className="gauge-used"
              style={{
                width: `${total > 0 ? (dollars / total) * 100 : 0}%`,
                background: "var(--chart-1)",
              }}
            />
          </span>
          <strong className="gauge-pct">{usd(dollars)}</strong>
          <span className="qb-usage-mixpct">
            {total > 0 ? `${((dollars / total) * 100).toFixed(1)}%` : "—"}
          </span>
          <span
            className="qb-usage-mixpct"
            title={`${NUM.format(tok)} token，占全部 token 的 ${
              tokenAll > 0 ? ((tok / tokenAll) * 100).toFixed(1) : "0"
            }%`}
          >
            {short(tok)}
          </span>
        </div>
      ))}
    </div>
  );
}

function RecentTable({
  rows,
  page,
  onPage,
}: {
  rows: TokenSummary["recent"];
  page: number;
  onPage: (p: number) => void;
}) {
  const pages = Math.max(1, Math.ceil(rows.length / PAGE));
  const p = Math.min(page, pages - 1);
  const shown = rows.slice(p * PAGE, p * PAGE + PAGE);
  return (
    <>
      <div className="qb-usage-tw">
        <table>
          <thead>
            <tr>
              <th>时间</th>
              <th>模型</th>
              <th className="num">输入</th>
              <th className="num">输出</th>
              <th className="num">缓存写</th>
              <th className="num">缓存读</th>
              <th className="num">美元</th>
              <th>项目</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((r, i) => (
              <tr key={`${r.day}-${r.time}-${r.session}-${p * PAGE + i}`}>
                <td>
                  {shortDay(r.day)} {r.time}
                </td>
                <td className="clip" title={r.model}>
                  {r.model}
                </td>
                <td className="num">{short(r.input)}</td>
                <td className="num">{short(r.output)}</td>
                <td className="num">{short(r.cache_write)}</td>
                <td className="num">{short(r.cache_read)}</td>
                <td className="num">{usd(r.cost_usd)}</td>
                <td className="clip" title={`${r.project} · 会话 ${r.session}`}>
                  {r.project}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="qb-usage-pager">
        <span className="notice">
          共 {NUM.format(rows.length)} 条（只列这一档里最近的 200 条）
        </span>
        <Button size="sm" disabled={p === 0} onClick={() => onPage(p - 1)}>
          上一页
        </Button>
        <span className="notice">
          {p + 1} / {pages}
        </span>
        <Button
          size="sm"
          disabled={p >= pages - 1}
          onClick={() => onPage(p + 1)}
        >
          下一页
        </Button>
      </div>
    </>
  );
}

/** 「按什么价算」：用到的每个模型的五个价，推出来的标「推」。 */
function PriceTable({ prices }: { prices: TokenSummary["prices_used"] }) {
  const cell = (v: number, derived: boolean) => (
    <td
      className="num"
      title={derived ? "官方表里没单独标，按官方倍数推的" : undefined}
    >
      ${Number(v.toFixed(4))}
      {derived && "（推）"}
    </td>
  );
  return (
    <div className="qb-usage-tw mt-3">
      <table>
        <thead>
          <tr>
            <th>按什么价算（每百万 token）</th>
            <th className="num">输入</th>
            <th className="num">输出</th>
            <th className="num">缓存读</th>
            <th className="num">5 分钟写</th>
            <th className="num">1 小时写</th>
            <th>来源</th>
          </tr>
        </thead>
        <tbody>
          {prices.map((p) => (
            <tr key={p.model}>
              <td className="clip" title={p.model}>
                {p.model}
              </td>
              <td className="num">${Number(p.input.toFixed(4))}</td>
              <td className="num">${Number(p.output.toFixed(4))}</td>
              {cell(p.cache_read, p.cache_read_derived)}
              {cell(p.cache_write_5m, p.cache_write_5m_derived)}
              {cell(p.cache_write_1h, p.cache_write_1h_derived)}
              <td>
                {p.live ? "官方定价页" : "内置快照"} · {p.verified_at}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function AgQuota({ identity }: { identity: AntigravityIdentity }) {
  return (
    <>
      <div className="slotusage">
        {identity.models.map((m) => (
          <Gauge
            key={`${m.label}-${m.model_id}`}
            name={m.label}
            used={m.remaining == null ? null : 100 - m.remaining * 100}
            value={
              m.remaining == null
                ? undefined
                : `剩 ${Math.round(m.remaining * 100)}%`
            }
            extra={
              m.reset_epoch ? (
                <span className="gauge-reset">
                  ↻ {resetIn(m.reset_epoch, Date.now())}
                </span>
              ) : undefined
            }
          />
        ))}
      </div>
      <p className="notice mt-2">
        读自反重力 IDE 写在 <code>state.vscdb</code> 的 <code>userStatus</code>
        ，零网络。
        <strong>只有 IDE 上次同步时那么新</strong>（IDE 写入{" "}
        {identity.written_at}）。Hub 不写这份状态 —— 它那一格在账户页上。
      </p>
    </>
  );
}
