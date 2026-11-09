import type { RouterSettings } from "../../lib/generated/RouterSettings";
import type { RouterRequest } from "../../lib/generated/RouterRequest";

/** 这里只显示后端读回的有效设置，编辑中的草稿留在设置弹窗。 */
export function StationRouteBoundary({
  settings,
  selected,
  routeLabel,
  error,
  onConfigure,
}: {
  settings: RouterSettings | null;
  selected: string | null;
  routeLabel: (id: string) => string;
  error?: string | null;
  onConfigure?: () => void;
}) {
  const pool = settings?.routing_mode === "pool";
  return (
    <div className={`qb-st-route-boundary${pool ? " pool" : ""}`} role="status">
      <div className="qb-st-route-boundary-heading">
        <strong>
          {error
            ? "选线范围读取失败"
            : !settings
              ? "正在读取选线范围"
              : pool
                ? `已允许 ${settings.allowed_routes.length} 条线路`
                : "仅所选线路"}
        </strong>
        {onConfigure && (
          <button type="button" className="qb-st-iconbtn" onClick={onConfigure}>
            设置允许范围
          </button>
        )}
      </div>
      <p>
        {error ??
          (!settings
            ? "读取成功后才可开启智能调度。"
            : pool
              ? settings.pool_strategy === "least-loaded"
                ? "优先空闲线路：每次请求都可能分配给下列任意站，各站分别计费；首选正常也可能使用其他站。"
                : "首选失败或容量不足时可使用下列备用，各站分别计费；智能调度只能在此名单内选线。"
              : selected
                ? `仅用「${routeLabel(selected)}」。失败直接报错，不改用其他站。`
                : "先选择一条线路；失败时不会改用其他站。")}
      </p>
      {pool && settings && (
        <div className="qb-st-allowed-lines" aria-label="当前允许的线路">
          {settings.allowed_routes.length ? (
            settings.allowed_routes.map((id) => (
              <span key={id}>{routeLabel(id)}</span>
            ))
          ) : (
            <span>名单为空，尚无可用线路</span>
          )}
        </div>
      )}
    </div>
  );
}

const FAULT_LABEL: Record<string, string> = {
  timeout: "超时",
  "upstream-down": "上游不可用",
  "rate-limit": "被限流",
  "model-missing": "缺少模型",
  auth: "鉴权失败",
  "client-abort": "客户端取消",
  "route-permission-denied": "线路超出允许范围",
  "route-admission-denied": "本机请求容量限制",
};

/** 呈现日志记录的尝试，不根据当前首选反推之前的请求。 */
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
        <span>未发往上游 · 线路超出允许范围</span>
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
        <span> · 已阻止范围外的后续尝试</span>
      )}
      {attempts.length > 0 && request.fault === "route-admission-denied" && (
        <span> · 后续尝试受本机请求容量限制</span>
      )}
    </div>
  );
}
