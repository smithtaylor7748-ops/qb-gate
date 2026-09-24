import { RefreshCw } from "lucide-react";
import { Button, Gauge } from "../../ui";
import type { CodexRateLimitScan } from "../../lib/generated/CodexRateLimitScan";
import type { CodexWindow } from "../../lib/generated/CodexWindow";
import type { TavernGptQuota } from "../../lib/generated/TavernGptQuota";

export interface CodexQuotaState {
  local: CodexRateLimitScan | null;
  live: TavernGptQuota | null;
  error: string;
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
  const local = localWindows(state?.local ?? null);
  const online = liveWindows.length > 0;
  const windows = online ? liveWindows : local;

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
      : state?.error
        ? "额度没读到"
        : state?.local
          ? "还没有额度记录 · 点右边刷新联网查"
          : "未读取额度";
    return (
      <div
        className={`slotusage codex-slotusage${refresh ? " slotusage--refreshable" : ""}`}
      >
        <div className="slotusage-bars">
          <span className="gauge-src" title={state?.error || message}>
            {message}
          </span>
        </div>
        {refresh}
      </div>
    );
  }

  const remaining = windows.map((window) =>
    "remaining_percent" in window
      ? (window.remaining_percent ?? 0)
      : 100 - window.window.used,
  );
  const tight = Math.min(...remaining);

  return (
    <div
      className={`slotusage codex-slotusage${refresh ? " slotusage--refreshable" : ""}`}
    >
      <div className="slotusage-bars">
        {windows.map((window, index) => {
          const left =
            "remaining_percent" in window
              ? window.remaining_percent
              : 100 - window.window.used;
          const used = left == null ? null : 100 - left;
          const name =
            "remaining_percent" in window ? window.label : window.name;
          const reset =
            "remaining_percent" in window
              ? window.reset_at
              : window.window.resets_at
                ? stamp(window.window.resets_at)
                : null;
          return (
            <Gauge
              key={`${name}-${index}`}
              name={name}
              used={used}
              binding={left != null && left === tight && windows.length > 1}
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
        className={`gauge-src${state?.error ? " qb-tone-warn" : ""}`}
        title={
          online
            ? `在线额度 · ${state?.live?.fetched_at ?? ""}${state?.error ? ` · 这次刷新没成：${state.error}` : ""}`
            : `Codex 本地快照 · ${state?.local?.found ? age(state.local.found.age_minutes) : ""}${state?.error ? ` · ${state.error}` : ""}`
        }
      >
        {state?.error
          ? "刷新没成 · 悬停看原因"
          : online
            ? `在线 · ${stamp(state?.live?.fetched_at)}`
            : `本地快照 · ${state?.local?.found ? age(state.local.found.age_minutes) : "—"}`}
      </span>
    </div>
  );
}
