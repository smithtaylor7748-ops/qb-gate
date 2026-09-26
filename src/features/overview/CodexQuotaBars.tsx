import { RefreshCw } from "lucide-react";
import { Button, Gauge } from "../../ui";
import type { CodexRateLimitScan } from "../../lib/generated/CodexRateLimitScan";
import type { CodexWindow } from "../../lib/generated/CodexWindow";
import type { TavernGptQuota } from "../../lib/generated/TavernGptQuota";
import type { TavernQuotaWindow } from "../../lib/generated/TavernQuotaWindow";

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
function leftOf(window: TavernQuotaWindow | CodexWindow): number | null {
  if ("remaining_percent" in window) return window.remaining_percent;
  // 本机快照里这一格的重置时刻已经过去：记的是重置之前的数，现在剩多少不知道（2026-09-25）。
  if (window.window.reset_passed) return null;
  return 100 - window.window.used;
}

/**
 * GPT 额度的统一槽位版展示：每条账户卡下面显示自己的窗口。
 * 在线读数优先，在线失败时保留可识别的本地快照，但绝不把失败折算成 0。
 *
 * # 刷新图标（2026-09-23）
 *
 * 右侧那颗图标是**这一个槽位**唯一的联网入口：点了才问一次 `wham/usage`。
 * 打开页面、切页回来都只显示本机快照与「最近一次」问到的，并标明时刻。
 * 使用者要的位置就是额度条右边那块空白，所以额度区是两列：左边条子、右边图标，
 * 来源那一行横跨在最下面。
 *
 * # 三件 2026-09-25 改的
 *
 * - 本机记录**读不出来**（会话目录是联结点、文件打不开）原来写「还没有额度记录 · 点右边刷新联网查」——
 *   那是「没有」，不是「读不出来」。现在说读不出来，悬停给第一条原因。
 * - 本机快照里**已经过了重置时刻**的那一格原来照样画旧数（「剩 5%」标红、还标「卡这儿」）。
 *   现在那一格写「已重置」，不参加比紧。
 * - 联网那一格没给比例时原来按 0 参加比紧，结果谁都对不上、「卡这儿」标记整个丢了。
 */
export default function CodexQuotaBars({
  state,
  canRefresh = true,
  onRefresh,
}: {
  state?: CodexQuotaState;
  /** 没登录的槽位没有令牌可问，图标灰掉。 */
  canRefresh?: boolean;
  onRefresh?: () => void;
}) {
  const liveWindows =
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

  const refresh = onRefresh && (
    <Button
      size="sm"
      variant="ghost"
      className="slotusage-refresh"
      icon={<RefreshCw size={12} />}
      aria-label="联网刷新这个槽位的额度"
      title={
        canRefresh
          ? "联网问一次这个槽位的额度（ChatGPT 官方接口，只问这一个）"
          : "还没登录，没有令牌可问"
      }
      loading={state?.loading}
      disabled={!canRefresh}
      onClick={onRefresh}
    />
  );

  if (windows.length === 0) {
    const message = state?.loading
      ? "额度读取中…"
      : state?.liveError
        ? "联网没问到 · 悬停看原因"
        : localReason
          ? "本机记录读不出来 · 悬停看原因"
          : state?.local
            ? canRefresh
              ? "还没有额度记录 · 点右边刷新联网查"
              : "还没有额度记录"
            : "未读取额度";
    return (
      <div
        className={`slotusage codex-slotusage${refresh ? " slotusage--refreshable" : ""}`}
      >
        <div className="slotusage-bars">
          <span
            className="gauge-src"
            title={state?.liveError || localReason || message}
          >
            {message}
          </span>
        </div>
        {refresh}
      </div>
    );
  }

  // 只在「知道剩多少」的那几格里比紧。
  const known = windows
    .map(leftOf)
    .filter((left): left is number => left != null);
  const tight = known.length ? Math.min(...known) : null;

  return (
    <div
      className={`slotusage codex-slotusage${refresh ? " slotusage--refreshable" : ""}`}
    >
      <div className="slotusage-bars">
        {windows.map((window, index) => {
          const isLive = "remaining_percent" in window;
          const name = isLive ? window.label : window.name;
          if (!isLive && window.window.reset_passed) {
            return (
              <Gauge
                key={`${name}-${index}`}
                name={name}
                used={null}
                note="已重置，快照是重置前的数 · 点右边刷新"
              />
            );
          }
          const left = leftOf(window);
          const used = left == null ? null : 100 - left;
          const reset = isLive
            ? window.reset_at
            : window.window.resets_at
              ? stamp(window.window.resets_at)
              : null;
          return (
            <Gauge
              key={`${name}-${index}`}
              name={name}
              used={used}
              binding={left != null && left === tight && known.length > 1}
              extra={
                reset ? (
                  <span
                    className="gauge-reset"
                    title={
                      online
                        ? "官方额度接口返回的重置时间"
                        : "Codex 写在本机会话记录里的重置时间"
                    }
                  >
                    ↻ {reset}
                  </span>
                ) : undefined
              }
            />
          );
        })}
      </div>
      {refresh}
      <span
        className={`gauge-src${state?.liveError || localReason ? " qb-tone-warn" : ""}`}
        title={
          online
            ? `在线额度 · ${state?.live?.fetched_at ?? ""}${state?.liveError ? ` · 这次刷新没成：${state.liveError}` : ""}`
            : `Codex 本地快照 · ${scan?.found ? age(scan.found.age_minutes) : ""}${state?.liveError ? ` · 联网没问到：${state.liveError}` : ""}${localReason ? ` · ${localReason}` : ""}`
        }
      >
        {state?.liveError
          ? "刷新没成 · 悬停看原因"
          : online
            ? `在线 · ${stamp(state?.live?.fetched_at)}`
            : `本地快照 · ${scan?.found ? age(scan.found.age_minutes) : "—"}`}
      </span>
    </div>
  );
}
