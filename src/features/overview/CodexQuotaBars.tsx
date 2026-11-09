import { Gauge } from "../../ui";
import type { CodexRateLimitScan } from "../../lib/generated/CodexRateLimitScan";
import type { CodexWindow } from "../../lib/generated/CodexWindow";
import type { TavernGptQuota } from "../../lib/generated/TavernGptQuota";
import type { TavernQuotaWindow } from "../../lib/generated/TavernQuotaWindow";
import { gptExtras } from "../../lib/gptExtras";
import { resetEpoch, resetPassed, useQuotaClock } from "../../lib/quotaClock";
import QuotaCountdown from "./QuotaCountdown";

export interface CodexQuotaState {
  local: CodexRateLimitScan | null;
  live: TavernGptQuota | null;
  /** 联网那一次（点刷新图标问 `wham/usage`）的错。 */
  liveError: string;
  /**
   * 读本机快照的错。**跟联网的错分开说**（2026-09-25）：原来两种错塞在一个字段里，
   * 联网刚问成功、本机那份读不出来，那一行照样写「刷新没成」。
   */
  localError: string;
  loading: boolean;
}

function stamp(value: string | null | undefined): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function age(ageMinutes: number): string {
  if (ageMinutes < 1) return "刚刚";
  if (ageMinutes < 60) return `${ageMinutes} 分钟前`;
  if (ageMinutes < 24 * 60) return `${Math.round(ageMinutes / 60)} 小时前`;
  return `${Math.round(ageMinutes / (24 * 60))} 天前`;
}

function localWindows(scan: CodexRateLimitScan | null) {
  if (!scan?.found) return [];
  return [scan.found.primary, scan.found.secondary].filter(
    (window): window is CodexWindow => !!window,
  );
}

/** 一格还剩多少（百分比）。`null` = 不知道 —— 联网那一格没给比例，或者本机那一格已经重置过。 */
function epochOf(window: TavernQuotaWindow | CodexWindow): number | null {
  return "remaining_percent" in window
    ? window.reset_epoch
    : resetEpoch(window.window.resets_at);
}

function leftOf(
  window: TavernQuotaWindow | CodexWindow,
  now: number,
): number | null {
  if (resetPassed(epochOf(window), now)) return null;
  if ("remaining_percent" in window) return window.remaining_percent;
  // 本机快照里这一格的重置时刻已经过去：记的是重置之前的数，现在剩多少不知道（2026-09-25）。
  if (window.window.reset_passed) return null;
  return 100 - window.window.used;
}

/**
 * GPT 额度的统一槽位版展示：每条账户卡下面显示自己的窗口。
 * 在线读数优先，在线失败时保留可识别的本地快照，但绝不把失败折算成 0。
 *
 * # 刷新图标（2026-09-23；2026-11-06 挪进账户表的「操作」列）
 *
 * 那颗图标是**这一个槽位**唯一的联网入口：点了才问一次 `wham/usage`。
 * 打开页面、切页回来都只显示本机快照与「最近一次」问到的，并标明时刻（来源那一行）。
 *
 * # 「审查」「模型」两个开关去掉了（2026-11-06）
 *
 * 原来标题栏上两个开关决定多画不画「代码审查」「模型专属（含 Spark）」两类限额，使用者嫌挤。
 * 现在条子只画套餐那两格，两类限额照旧在来源那一行的悬停提示里（`gptExtras`），读不到就不写。
 *
 * # 三件 2026-09-25 改的
 *
 * - 本机记录**读不出来**（会话目录是联结点、文件打不开）原来写「还没有额度记录 · 点右边刷新联网查」——
 *   那是「没有」，不是「读不出来」。现在说读不出来，悬停给第一条原因。
 * - 本机快照里**已经过了重置时刻**的那一格原来照样画旧数（「剩 5%」标红、还标「卡这儿」）。
 *   现在那一格写「已重置」，不参加比紧。
 * - 联网那一格没给比例时原来按 0 参加比紧，结果谁都对不上、「卡这儿」标记整个丢了。
 */
export default function CodexQuotaBars({ state }: { state?: CodexQuotaState }) {
  const now = useQuotaClock();
  const liveWindows: TavernQuotaWindow[] =
    state?.live?.windows.filter((window) => window.present) ?? [];
  const scan = state?.local ?? null;
  const local = localWindows(scan);
  const online = liveWindows.length > 0;
  const windows = online ? liveWindows : local;
  /** 本机那份没找到额度、而且有读不出来的文件 —— 说不清是「没有」还是「读不到」。 */
  const localUnreadable = !!scan && !scan.found && scan.files_failed > 0;
  const localReason = state?.localError
    ? state.localError
    : localUnreadable
      ? `本机会话记录有 ${scan.files_failed} 份读不出来：${scan.first_error ?? "原因不明"}`
      : "";

  if (windows.length === 0) {
    const message = state?.loading
      ? "额度读取中…"
      : state?.liveError
        ? "联网没问到 · 悬停看原因"
        : localReason
          ? "本机记录读不出来 · 悬停看原因"
          : state?.local
            ? "还没有额度记录 · 点刷新联网查"
            : "未读取额度";
    return (
      <div className="slotusage codex-slotusage">
        <div className="slotusage-bars">
          <span
            className={`gauge-src${state?.liveError || localReason ? " qb-tone-warn" : ""}`}
            title={state?.liveError || localReason || message}
          >
            {message}
          </span>
        </div>
      </div>
    );
  }

  // 只在「知道剩多少」的那几格里比紧。
  const known = windows
    .map((window) => leftOf(window, now))
    .filter((left): left is number => left != null);
  const tight = known.length ? Math.min(...known) : null;

  return (
    <div className="slotusage codex-slotusage">
      <div className="slotusage-bars">
        {windows.map((window, index) => {
          const isLive = "remaining_percent" in window;
          const name = isLive ? window.label : window.name;
          // 过了重置时刻就不画旧百分比（联网那份按 `reset_epoch` 判，本机快照按 `reset_passed`）。
          const passed =
            resetPassed(epochOf(window), now) ||
            (!isLive && window.window.reset_passed);
          if (passed) {
            return (
              <Gauge
                key={`${name}-${index}`}
                name={name}
                used={null}
                note="已到重置时间 · 待刷新确认额度"
              />
            );
          }
          const left = leftOf(window, now);
          const used = left == null ? null : 100 - left;
          const reset = isLive
            ? window.reset_at
            : window.window.resets_at
              ? stamp(window.window.resets_at)
              : null;
          return (
            <Gauge
              className="gauge--countdown"
              key={`${name}-${index}`}
              name={name}
              used={used}
              binding={left != null && left === tight && known.length > 1}
              hint={
                reset
                  ? `${reset} 重置 · ${online ? "官方额度接口返回的" : "Codex 写在本机会话记录里的"}`
                  : undefined
              }
              extra={<QuotaCountdown epoch={epochOf(window)} now={now} />}
            />
          );
        })}
      </div>
      <span
        className={`gauge-src${state?.liveError || localReason ? " qb-tone-warn" : ""}`}
        title={
          online
            ? // 存下的重置、Credits 余额（阶段 6 · U2）只放悬停：这一行是固定高度的。
              [
                `在线额度 · ${state?.live?.fetched_at ?? ""}`,
                ...gptExtras(state?.live),
                ...(state?.liveError
                  ? [`这次刷新没成：${state.liveError}`]
                  : []),
              ].join(" · ")
            : `Codex 本地快照 · ${scan?.found ? age(scan.found.age_minutes) : ""}${state?.liveError ? ` · 联网没问到：${state.liveError}` : ""}${localReason ? ` · ${localReason}` : ""}`
        }
      >
        {state?.liveError
          ? "刷新没成 · 保留上次读数"
          : online && state?.live?.last_success
            ? `上次成功：${stamp(state.live.last_success)}`
            : online
              ? `在线 · ${stamp(state?.live?.fetched_at)}`
              : `本地快照 · ${scan?.found ? age(scan.found.age_minutes) : "—"}`}
      </span>
    </div>
  );
}
