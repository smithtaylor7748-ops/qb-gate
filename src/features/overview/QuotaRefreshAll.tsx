import { useEffect, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { api } from "../../lib/api";
import type { Settings } from "../../lib/generated/Settings";
import { useToast } from "../../ui";
import Button from "../../ui/Button";

/** 自动刷新的档位（分钟）。0 = 关。 */
const STEPS = [0, 10, 30, 60] as const;

/**
 * 「全部刷新」按钮 +（可选）自动刷新的小开关。
 *
 * - 全部刷新：按顺序问每个账户，`run` 自己负责去重、跳过停用 / 无权限的，返回一句结果。
 * - 自动刷新：只在窗口可见时跑（`document.visibilityState`），默认 10 分钟，存进 settings。
 *   Claude 子页不给这个开关（`autoKey` 不传）。
 */
export default function QuotaRefreshAll({
  run,
  autoKey,
}: {
  run: (auto: boolean) => Promise<string>;
  autoKey?: "gpt_quota_auto_minutes" | "antigravity_quota_auto_minutes";
}) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const [minutes, setMinutes] = useState<number>(10);
  const settingsRef = useRef<Settings | null>(null);
  const busyRef = useRef(false);
  const runRef = useRef(run);
  runRef.current = run;

  useEffect(() => {
    if (!autoKey) return;
    api
      .settingsLoad()
      .then((s) => {
        settingsRef.current = s;
        setMinutes(s[autoKey] ?? 10);
      })
      .catch(() => undefined);
  }, [autoKey]);

  async function go(auto: boolean) {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    try {
      const msg = await runRef.current(auto);
      if (!auto && msg) toast.info(msg);
    } catch (e) {
      if (!auto) toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }

  useEffect(() => {
    if (!autoKey || !minutes) return;
    const t = setInterval(() => {
      if (document.visibilityState === "visible") void go(true);
    }, minutes * 60_000);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoKey, minutes]);

  async function pick(next: number) {
    setMinutes(next);
    const base = settingsRef.current ?? (await api.settingsLoad());
    if (!autoKey) return;
    try {
      settingsRef.current = await api.settingsSave({
        ...base,
        [autoKey]: next,
      });
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <span className="inline-flex items-center gap-1">
      {/* 只画图标：同 SessionsEntry，标题栏窄时放不下带字的按钮。 */}
      <Button
        size="sm"
        icon={<RefreshCw size={12} />}
        aria-label="全部刷新"
        title="全部刷新：按顺序向官方问一遍每个账户的额度"
        loading={busy}
        onClick={() => void go(false)}
      />
      {autoKey && (
        <select
          className="input input--sm qb-wide-only"
          aria-label="自动刷新额度"
          title="自动刷新额度（只在窗口可见时；跳过停用和无权限的）"
          value={minutes}
          onChange={(e) => void pick(Number(e.target.value))}
        >
          {STEPS.map((m) => (
            <option key={m} value={m}>
              {m ? `每 ${m} 分` : "自动关"}
            </option>
          ))}
        </select>
      )}
    </span>
  );
}
