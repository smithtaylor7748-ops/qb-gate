/**
 * 总览第一段：综合评分。
 *
 * 这一段吃掉了三处旧内容：原来的「综合评分」半栏卡、独占一张通栏卡的
 * 「出口」、以及页面底部那三个门禁 `Metric`。它们讲的是同一件事
 * ——「现在这台机器安不安全」—— 分散在三处的结果是评分被挤在半栏里，
 * 四项明细压成一列窄行。
 *
 * 三层：
 *   1. 标题行右侧的 meta：出口 IP、锁了几个、看门狗、残留副本；
 *   2. 权重堆叠条：段宽 = 权重，段内填充 = 得分比例，未检测的段留斜纹；
 *   3. 四列明细：得分、权重、关键事实两行。
 *
 * 第 2 层是这一段真正新增的东西。旧的单色进度条只能表达总分，
 * 看不出**分数被哪一项拖着**，也看不出哪一块还是空的。
 */

import { AlertTriangle, Lock, Radar, ShieldCheck } from "lucide-react";

import { describeLease } from "../../lib/lease";
import { useProgress } from "../../lib/progress";
import { R } from "../../lib/resources";
import { measuredAt, useResource } from "../../lib/store";
import {
  BAND_LABEL,
  BAND_TONE,
  computeScore,
  type ScoreItem,
} from "../../lib/score";
import { Button, Card, Pill, fmtMeasured } from "../../ui";
import { openSecurity } from "../security/SecuritySheet";
import type { ObjectId } from "../security/objects";
import type { Tone } from "../../ui/labels";

/** 一项拿到了自己权重的多少 → 什么语气。 */
function toneOf(ratio: number): Exclude<Tone, "accent" | "default"> {
  if (ratio >= 0.85) return "ok";
  if (ratio >= 0.5) return "warn";
  return "danger";
}

/**
 * 四项明细里点一格要打开哪个安全对象。
 *
 * `score.ts` 的 id 与 `objects.tsx` 的 id 大部分同名，只有 `iplock` 例外 ——
 * 评分把「白名单 + 执行锁」算成一项，而安全那边是两个对象。评分扣分最常见的
 * 原因是白名单为空或当前 IP 不在名单里，所以落在白名单那一格。
 */
const OPENS: Record<ScoreItem["id"], ObjectId> = {
  purity: "purity",
  dns: "dns",
  signals: "signals",
  iplock: "allowlist",
};

export default function ScoreBand() {
  const progress = useProgress();

  const ip = useResource("ip", R.ip);
  const gate = useResource("gate", R.gate);
  // 这两个是 auto:false —— 只把缓存拿出来，不会自己去跑。
  // 没跑过就是 undefined，评分那边会如实记成「未检测」。
  const dns = useResource("dns", R.dns);
  const signals = useResource("signals", R.signals);
  const hook = useResource("hook", R.hook);

  const score = computeScore({
    progress,
    dns: dns.data,
    signals: signals.data,
    gate: gate.data,
  });

  const info = ip.data;
  const locked = gate.data?.targets.filter((t) => t.locked).length ?? 0;
  const total = gate.data?.targets.length ?? 0;
  const stale = gate.data?.stale_copies.length ?? 0;

  /**
   * 每一项的两行关键事实。
   *
   * 第一行是那一项自己的原始读数，第二行是结论或补充。
   * `score.ts` 的 `detail` 只有一句，这里能给的比它多 —— 但**读数取不到时
   * 一律回落到 `detail`，不自己编**。
   */
  function factsOf(it: ScoreItem): [string, string] {
    switch (it.id) {
      case "purity": {
        const place =
          info?.isResidential === true
            ? "住宅"
            : info?.isResidential === false
              ? "非住宅"
              : "住宅未知";
        return [
          `纯净度 ${info?.fraudScore ?? "未知"} · ${place}`,
          `${it.detail} · 原生未知`,
        ];
      }
      case "dns": {
        const d = dns.data;
        if (!d) return [it.detail, ""];
        // 这一项的结果会留到下次开面板，所以**必须**带上测的时刻 ——
        // 不然一份昨天的读数看起来跟刚测的一样，而你中间可能换过网。
        const when = fmtMeasured(measuredAt("dns"));
        const head =
          d.ethernet_safe === true
            ? "以太网无泄露"
            : `${d.findings.length} 项问题`;
        return [`${d.score} / 100`, when ? `${head} · ${when}` : head];
      }
      case "signals": {
        const s = signals.data;
        if (!s) return [it.detail, ""];
        // 扣分最重的那一项 —— 比「命中 3 项」有用得多，它直接指向该去修什么。
        const worst = [...s.hits].sort((a, b) => b.points - a.points)[0];
        const when = fmtMeasured(measuredAt("signals"));
        const head = worst
          ? `最重：${worst.label} −${worst.points}`
          : "没有命中项";
        return [
          `识别度 ${s.total} / 100 · 命中 ${s.hits.length} 项`,
          when ? `${head} · ${when}` : head,
        ];
      }
      case "iplock": {
        const g = gate.data;
        if (!g) return [it.detail, ""];
        const head =
          describeLease(g.lease, "已放行给")?.text ??
          `${locked} / ${total} 已锁`;
        const bits: string[] = [];
        if (stale > 0) bits.push(`${stale} 个残留副本可绕过`);
        if (g.allowlist.length === 0) bits.push("白名单为空");
        else bits.push(`白名单 ${g.allowlist.length} 条`);
        return [head, bits.join(" · ")];
      }
    }
  }

  return (
    <Card
      title="综合评分"
      className="mb-3"
      actions={
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px]">
          {ip.error ? (
            <span className="flex items-center gap-1.5 text-[var(--danger)]">
              <AlertTriangle size={12} aria-hidden="true" />
              查不到出口 IP
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void ip.refresh()}
              >
                重试
              </Button>
            </span>
          ) : (
            <>
              <span className="font-mono text-[var(--text)]">
                {ip.loading && !info ? (
                  <span
                    className="skeleton inline-block h-3 w-28 align-middle"
                    aria-hidden="true"
                  />
                ) : (
                  (info?.ip ?? "—")
                )}
              </span>
              <span className="notice">
                {[
                  info?.isResidential === true
                    ? "住宅"
                    : info?.isResidential === false
                      ? "非住宅"
                      : null,
                  info?.city || info?.region || info?.country || null,
                  info?.timezone || null,
                ]
                  .filter(Boolean)
                  .join(" · ") || "归属未知"}
              </span>
            </>
          )}

          {/* 这三样点下去开「执行锁」小窗 —— 上锁、应急解锁、清残留、看门狗启停
              全在那一窗里。删掉安全页之后它们只剩这一个入口，所以必须能点。 */}
          <button
            type="button"
            className="metachip"
            title="执行锁 —— 上锁、应急解锁、清残留、看门狗"
            onClick={() => openSecurity("lock")}
          >
            <span
              className={
                total > 0 && locked === total
                  ? "flex items-center gap-1 text-[var(--ok)]"
                  : "flex items-center gap-1 text-[var(--warn)]"
              }
            >
              <Lock size={12} aria-hidden="true" />
              {gate.data ? `${locked}/${total} 已锁` : "—"}
            </span>

            <span
              className={
                gate.data?.watchdog_running
                  ? "flex items-center gap-1 text-[var(--ok)]"
                  : "flex items-center gap-1 text-[var(--text-3)]"
              }
            >
              <Radar size={12} aria-hidden="true" />
              {gate.data
                ? gate.data.watchdog_running
                  ? "看门狗"
                  : "看门狗未跑"
                : "—"}
            </span>

            {/* 残留副本没有执行锁，是能绕过门禁的入口。这里变红，
                同时评分那一列的得分已经被 score.ts 砍半 —— 两处一起变色。 */}
            <span className={stale > 0 ? "text-[var(--danger)]" : "notice"}>
              残留 {stale}
            </span>
          </button>

          <button
            type="button"
            className="metachip"
            title="会话内门禁 —— 执行锁只管启动，这一档在每次请求发出前再验一遍"
            onClick={() => openSecurity("session")}
          >
            <span
              className={
                hook.data?.installed
                  ? "flex items-center gap-1 text-[var(--ok)]"
                  : "flex items-center gap-1 text-[var(--text-3)]"
              }
            >
              <ShieldCheck size={12} aria-hidden="true" />
              {hook.data
                ? hook.data.installed
                  ? "会话门禁"
                  : "会话门禁未开"
                : "—"}
            </span>
          </button>
        </div>
      }
    >
      <div className="flex items-center gap-3">
        <div className="flex items-baseline gap-1.5">
          <span className="text-[30px] leading-none font-medium">
            {score.total ?? "—"}
          </span>
          <span className="notice">/ 100</span>
        </div>
        {score.total !== null && (
          <Pill tone={BAND_TONE[score.band]}>{BAND_LABEL[score.band]}</Pill>
        )}

        <div
          className="wbar flex-1"
          role="img"
          aria-label={`综合评分 ${score.total ?? "未知"} / 100，四项权重分别为 ${score.items
            .map((i) => `${i.label} ${i.earned ?? "未检测"} / ${i.weight}`)
            .join("，")}`}
        >
          {score.items.map((it) => {
            const ratio = it.earned === null ? 0 : it.earned / it.weight;
            return (
              <div
                key={it.id}
                className={`wseg${it.earned === null ? " wseg--none" : ""}`}
                style={{ flexGrow: it.weight, flexBasis: 0 }}
                title={`${it.label} ${it.earned ?? "未检测"} / ${it.weight}`}
              >
                {it.earned !== null && (
                  <span
                    className={`wseg-fill wseg-fill--${toneOf(ratio)}`}
                    style={{ width: `${Math.round(ratio * 100)}%` }}
                  />
                )}
              </div>
            );
          })}
        </div>
      </div>

      {/* auto-fit 网格而不是 flex-wrap：flex 换行时落单的最后一格会被
          `flex: 1` 拉满整行，四格变成 3 + 1 很难看。网格换行后仍然等宽。 */}
      <div
        className="mt-3 grid gap-2"
        style={{ gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))" }}
      >
        {score.items.map((it) => {
          // 拆成局部常量 TS 才会收窄 —— 用 `none` 这个布尔量做条件的话，
          // 分支里的 `it.earned` 仍然是 `number | null`。
          const earned = it.earned;
          const [a, b] = factsOf(it);
          return (
            <button
              type="button"
              key={it.id}
              className={`scorecell${earned === null ? " scorecell--none" : ""}`}
              title={`${it.label} —— 点开就地处理`}
              onClick={() => openSecurity(OPENS[it.id])}
            >
              <div className="scorecell-head">
                <span className="scorecell-name">{it.label}</span>
                <span className="scorecell-weight">权重 {it.weight}</span>
              </div>
              <div
                className={`scorecell-value${earned === null ? " scorecell-value--none" : ""}`}
              >
                {earned ?? "—"}
              </div>
              {earned === null ? (
                <>
                  <p className="notice">{it.detail}</p>
                  {/* 整格本身就是按钮，这里不能再嵌一个真按钮（嵌套 button 是
                      非法 HTML，浏览器会把它拆出去）。只借 .btn 的样子。 */}
                  <span className="btn btn--sm mt-1.5 w-full justify-center">
                    去检测
                  </span>
                </>
              ) : (
                <>
                  <div className="bar mt-1 mb-1.5" style={{ height: 3 }}>
                    <span
                      className={`bar-fill bar-fill--${toneOf(earned / it.weight)}`}
                      style={{
                        width: `${Math.round((earned / it.weight) * 100)}%`,
                      }}
                    />
                  </div>
                  <p className="notice">{a}</p>
                  {b && <p className="notice">{b}</p>}
                </>
              )}
            </button>
          );
        })}
      </div>

      <p className="notice mt-2">
        <strong>分母只算已检测项。</strong>
        {score.missing > 0 && `${score.missing} 项未检测 —— `}
        没跑过的不按 0 分算（那会在你什么都没做时就报「环境很差」），
        也不按满分算（那是替一个没做过的检测打包票）。
      </p>
    </Card>
  );
}
