import { useEffect, useState } from "react";

/** 只推进本地显示时钟；不会刷新额度或续写缓存。 */
export function useQuotaClock(): number {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 15_000);
    return () => window.clearInterval(timer);
  }, []);
  return now;
}

export function resetEpoch(iso: string | null | undefined): number | null {
  if (!iso) return null;
  const ms = Date.parse(iso);
  return Number.isFinite(ms) ? ms / 1000 : null;
}

export function resetPassed(
  epoch: number | null | undefined,
  now: number,
): boolean {
  return epoch != null && Number.isFinite(epoch) && epoch * 1000 <= now;
}

/** 到点不代表已经确认恢复，更不能据此把旧额度改成 100%。 */
export function quotaCountdown(
  epoch: number | null | undefined,
  now: number,
): string {
  if (epoch == null || !Number.isFinite(epoch)) return "";
  const diff = epoch * 1000 - now;
  if (diff <= 0) return "到点 · 待刷新";
  const minutes = Math.ceil(diff / 60_000);
  const days = Math.floor(minutes / 1440);
  const hours = Math.floor((minutes % 1440) / 60);
  const mins = minutes % 60;
  const time = days
    ? `${days}d${hours}h`
    : hours
      ? `${hours}h${mins}m`
      : `${mins}m`;
  return `${time} 后重置`;
}
