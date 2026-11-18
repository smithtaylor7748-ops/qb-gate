/**
 * 主页「最近请求实际去向」那一行（兼做提示行）和它用的尝试路径。
 *
 * 2026-11-15 起原来那张全宽的「仅所选线路 / 已允许 N 条线路」范围卡删了（第三版样机）：
 * 备用线路写进右列「选线与分流」卡片的第二行，这一行在出问题时变黄兼做提示。
 */

import type { RouterRequest } from "../../lib/generated/RouterRequest";
import type { SelectionView } from "../../lib/generated/SelectionView";
import { retryable, SKIP_LABEL, untilText } from "./selection";
import { stamp } from "./shared";

const FAULT_LABEL: Record<string, string> = {
  timeout: "超时",
  "upstream-down": "上游不可用",
  "rate-limit": "被限流",
  "model-missing": "缺少模型",
  auth: "鉴权失败",
  "client-abort": "客户端取消",
  "route-permission-denied": "不是当前线路或备用线路",
  "route-admission-denied": "本机请求容量限制",
};

/** 呈现日志记录的尝试，不根据当前线路反推之前的请求。 */
export function RouteAttemptTrail({
  request,
  routeLabel,
}: {
  request: RouterRequest;
  routeLabel: (id: string) => string;
}) {
  const attempts = request.attempts;
  return (
    <div className="qb-st-attempt-trail" aria-label="最近请求尝试路径">
      {attempts.length ? (
        attempts.map((attempt, index) => {
          const fault =
            attempt.fault ||
            (index === attempts.length - 1 &&
            request.fault !== "route-permission-denied" &&
            request.fault !== "route-admission-denied"
              ? request.fault
              : null);
          const result = fault
            ? (FAULT_LABEL[fault] ?? fault)
            : attempt.status != null &&
                attempt.status >= 200 &&
                attempt.status < 300
              ? "成功"
              : attempt.status == null
                ? "未收到状态"
                : `HTTP ${attempt.status}`;
          return (
            <span key={`${index}-${attempt.route_id}`}>
              {index > 0 && <span aria-hidden="true"> → </span>}
              {routeLabel(attempt.route_id)} · {result}
            </span>
          );
        })
      ) : request.fault === "route-permission-denied" ? (
        <span>未发往上游 · 不是当前线路或备用线路</span>
      ) : request.fault === "route-admission-denied" ? (
        <span>未发往上游 · 本机请求容量限制</span>
      ) : (
        <span>
          {routeLabel(request.route_id)} ·{" "}
          {request.fault
            ? (FAULT_LABEL[request.fault] ?? request.fault)
            : "已记录"}
        </span>
      )}
      {attempts.length > 0 && request.fault === "route-permission-denied" && (
        <span> · 已阻止名单外的后续尝试</span>
      )}
      {attempts.length > 0 && request.fault === "route-admission-denied" && (
        <span> · 后续尝试受本机请求容量限制</span>
      )}
    </div>
  );
}

/**
 * 「最近请求实际去向」那一行，兼做提示行（第三版样机）。
 *
 * - 当前线路被暂停（`selection.paused`，C5）：变黄，写清为什么、这期间改用哪条备用线路（没有就说请求会直接报错）、
 *   预计什么时候恢复。只有熔断那两种给「重试主线」—— 限流、额度用光点了也没用（R1 交接）；
 * - 客户端选了聚合模型（`selection.aggregate`，B6）：那一发不看当前线路、不换线，钱花在它直达的那条线上 —— 也变黄说清楚；
 * - 平时照旧写最近一发去了哪。
 *
 * ⛔ 只照后端给的写，不拿当前线路反推之前的请求去了哪。
 */
export function RecentRoute({
  selection,
  latest,
  error,
  routeLabel,
  onRetry,
}: {
  selection: SelectionView | null;
  latest: RouterRequest | null;
  error: string | null;
  routeLabel: (id: string) => string;
  onRetry: (routeId: string) => void;
}) {
  const paused = selection?.paused ?? null;
  const agg = selection?.aggregate?.latest ? selection.aggregate : null;
  const warn = paused != null || agg != null;
  return (
    <div
      className={`qb-st-recent-route${warn ? " warn" : ""}`}
      role={warn ? "alert" : "status"}
    >
      {paused ? (
        <>
          <strong>
            当前线路「{routeLabel(paused.route_id)}」暂停中（
            {SKIP_LABEL[paused.reason]}
            {untilText(paused.until_ms)
              ? `，${untilText(paused.until_ms)}`
              : ""}
            ）
          </strong>
          <span>
            {paused.using
              ? `这期间改用备用线路「${routeLabel(paused.using)}」`
              : "没有能用的备用线路，这期间请求会直接报错（或回「繁忙」）"}
          </span>
          {retryable(paused.reason) && (
            <button
              type="button"
              className="qb-st-iconbtn qb-st-iconbtn--text"
              title="充值了、换了 Key 之后用：当前线路马上转半开，下一发真实请求就是回探（不额外花钱）"
              onClick={() => onRetry(paused.route_id)}
            >
              重试主线
            </button>
          )}
        </>
      ) : agg ? (
        <>
          <strong>客户端选了聚合模型「{agg.model}」</strong>
          <span>
            最近一发直达「{routeLabel(agg.route_id)}
            」：不看当前线路、不换线，钱花在这条线上（
            {stamp(Number(agg.at_ms))}）
          </span>
        </>
      ) : (
        <>
          <strong>最近请求实际去向</strong>
          {latest && (
            <span className="qb-st-note">{stamp(Number(latest.at_ms))}</span>
          )}
          {error ? (
            <span className="qb-st-note">{error}</span>
          ) : latest ? (
            <RouteAttemptTrail request={latest} routeLabel={routeLabel} />
          ) : (
            <span className="qb-st-note">
              尚无请求记录；开始新请求后可在此核对。
            </span>
          )}
        </>
      )}
    </div>
  );
}
