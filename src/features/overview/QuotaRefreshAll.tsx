import { useEffect, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { R } from "../../lib/resources";
import { useResource } from "../../lib/store";
import { useToast } from "../../ui";
import Button from "../../ui/Button";

/** 自动刷新的档位（分钟）。0 = 关。设置页「账户额度」那两行用的也是它。 */
export const QUOTA_AUTO_STEPS = [0, 10, 30, 60] as const;

/** 自动刷新存在设置里的那两个键。Claude 子页没有自动刷新。 */
export type QuotaAutoKey =
  "gpt_quota_auto_minutes" | "antigravity_quota_auto_minutes";

/**
 * 账户列表标题栏上**唯一**那颗刷新（2026-11-06 重排，使用者：两个长得一样的刷新图标合成一个）。
 *
 * - 点一下：调用方的 `run(false)` —— 先重读本机账户，再按顺序向官方问一遍每个账户的额度
 *   （同一个账户只问一次，停用 / 无权限 / 没登录的跳过），完了弹一句结果。
 * - 自动刷新：档位在设置页「账户额度」（默认每 10 分钟），只在窗口可见时跑，
 *   走同一条 `run(true)`、不弹提示。原来标题栏上那个「每 10 分」下拉框挤掉了一行，挪走了。
 *   Claude 子页不给（`autoKey` 不传）。
 */
export default function QuotaRefreshAll({
  run,
  autoKey,
}: {
  run: (auto: boolean) => Promise<string>;
  autoKey?: QuotaAutoKey;
}) {
  const toast = useToast();
  const settings = useResource("settings", R.settings);
  const minutes = autoKey ? (settings.data?.[autoKey] ?? 10) : 0;
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const runRef = useRef(run);
  runRef.current = run;

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
    if (!minutes) return;
    const t = setInterval(() => {
      if (document.visibilityState === "visible") void go(true);
    }, minutes * 60_000);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [minutes]);

  return (
    <Button
      size="sm"
      icon={<RefreshCw size={13} />}
      aria-label="刷新"
      title={
        minutes
          ? `刷新：重读本机账户，再向官方问一遍每个账户的额度。窗口开着时每 ${minutes} 分钟自己也问一遍（设置页「账户额度」里改）`
          : "刷新：重读本机账户，再向官方问一遍每个账户的额度"
      }
      loading={busy}
      onClick={() => void go(false)}
    />
  );
}
