import { quotaCountdown } from "../../lib/quotaClock";

export default function QuotaCountdown({
  epoch,
  now,
  estimated = false,
}: {
  epoch: number | null | undefined;
  now: number;
  estimated?: boolean;
}) {
  const text = quotaCountdown(epoch, now);
  return text ? (
    <span
      className="quota-countdown"
      title="沿用上次读数的重置时间；倒计时结束后仍须刷新确认额度。"
    >
      {estimated ? "推算 " : ""}
      {text}
    </span>
  ) : null;
}
