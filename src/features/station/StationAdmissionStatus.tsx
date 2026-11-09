import type { RouterSettings } from "../../lib/generated/RouterSettings";
import type { RouterStatus } from "../../lib/generated/RouterStatus";

const REASON: Record<string, string> = {
  concurrency: "同时进行的请求已达上限",
  rpm: "每分钟请求数已达上限",
  "retry-after": "上游要求稍后再试",
  "quota-exhausted": "已查询的上游额度状态不可用",
  "queue-full": "本机等待队列已满",
  "wait-timeout": "本机等待已超时",
};

const QUOTA: Record<string, string> = {
  disabled: "未启用额度检查",
  unknown: "额度状态未知，不据此拦截",
  stale: "额度缓存已过期，不据此拦截",
  available: "已查询的额度状态可用",
  exhausted: "上游明确额度不可用",
};

/** 仅展示后端快照；不在浏览器重算额度有效性、负载或是否准入。 */
export function StationAdmissionStatus({
  settings,
  status,
  routeLabel,
  error,
  onRefresh,
}: {
  settings: RouterSettings | null;
  status: RouterStatus | null;
  routeLabel: (id: string) => string;
  error?: string | null;
  onRefresh?: () => void;
}) {
  const ids = new Set([
    ...(settings?.routing_mode === "pool"
      ? settings.allowed_routes
      : [status?.pending_route ?? status?.current_route].filter(
          (id): id is string => !!id,
        )),
    ...Object.keys(settings?.admission.route_limits ?? {}),
    ...(status?.admission.map((row) => row.route_id) ?? []),
  ]);
  const byId = new Map(
    status?.admission.map((row) => [row.route_id, row]) ?? [],
  );
  return (
    <section
      className="qb-card qb-st-admission-status"
      aria-label="本机请求容量"
    >
      <div className="qb-st-route-boundary-heading">
        <h3>本机请求容量</h3>
        {onRefresh && (
          <button type="button" className="qb-st-iconbtn" onClick={onRefresh}>
            刷新状态
          </button>
        )}
      </div>
      <p className="qb-st-note">
        约每 15
        秒读取一次正在进行与排队的请求。容量限制在本机拦截尚未发出的请求，不会因此记为站点故障。
      </p>
      {error ? (
        <p className="qb-st-nudge" role="alert">
          容量状态读取失败：{error}；下方保留上次快照。
        </p>
      ) : null}
      {!settings || !status ? (
        <p className="qb-st-note">正在读取容量设置与状态…</p>
      ) : (
        <>
          <div className="qb-st-admission-summary">
            <span>
              {settings.routing_mode === "pool" &&
              settings.pool_strategy === "least-loaded"
                ? "名单内优先空闲线路"
                : "优先所选线路"}
            </span>
            <span>
              {settings.admission.max_wait_ms > 0 &&
              settings.admission.max_waiters > 0
                ? `最多等待 ${settings.admission.max_wait_ms} 毫秒 · 每条线路最多排队 ${settings.admission.max_waiters} 个`
                : "不排队 · 容量不足时返回本机繁忙"}
            </span>
          </div>
          {ids.size === 0 ? (
            <p className="qb-st-note">尚未选线，也未设置线路容量。</p>
          ) : (
            <div className="qb-st-admission-routes">
              {[...ids].map((id) => {
                const row = byId.get(id);
                const limit = settings.admission.route_limits[id];
                const allowed =
                  settings.routing_mode === "pool"
                    ? settings.allowed_routes.includes(id)
                    : id === (status.pending_route ?? status.current_route);
                return (
                  <article
                    className="qb-st-admission-route"
                    key={id}
                    aria-label={`${routeLabel(id)} · 容量状态`}
                  >
                    <div className="qb-st-route-boundary-heading">
                      <strong>{routeLabel(id)}</strong>
                      {!allowed && (
                        <span className="qb-st-pill qb-st-pill--unknown">
                          不在当前允许范围
                        </span>
                      )}
                      {row?.reason && (
                        <span className="qb-st-pill qb-st-pill--warn">
                          暂不可接收请求
                        </span>
                      )}
                    </div>
                    <dl className="qb-st-admission-metrics">
                      <div>
                        <dt>进行中</dt>
                        <dd>
                          {row ? row.active : "未知"}
                          <small>
                            上限{" "}
                            {limit?.max_concurrent
                              ? limit.max_concurrent
                              : "未设置"}
                          </small>
                        </dd>
                      </div>
                      <div>
                        <dt>等待中</dt>
                        <dd>{row ? row.waiting : "未知"}</dd>
                      </div>
                      <div>
                        <dt>最近一分钟</dt>
                        <dd>
                          {row ? row.rpm_used : "未知"}
                          <small>
                            上限 {limit?.rpm ? limit.rpm : "未设置"}
                          </small>
                        </dd>
                      </div>
                    </dl>
                    {row?.reason && (
                      <p className="qb-st-admission-reason">
                        本机暂缓发送：{REASON[row.reason] ?? row.reason}
                      </p>
                    )}
                    {row?.retry_at_ms != null && (
                      <p className="qb-st-note">
                        最早可重试时间：
                        {new Date(Number(row.retry_at_ms)).toLocaleTimeString()}
                        ；届时仍需检查容量。
                      </p>
                    )}
                    <p className="qb-st-note">
                      {row
                        ? (QUOTA[row.quota_state] ?? "额度状态未知，不据此拦截")
                        : limit?.check_quota
                          ? "额度状态未知，不据此拦截"
                          : "未启用额度检查"}
                      {row?.quota_checked_at_ms != null &&
                        ` · 缓存查询于 ${new Date(Number(row.quota_checked_at_ms)).toLocaleString()}`}
                    </p>
                  </article>
                );
              })}
            </div>
          )}
        </>
      )}
    </section>
  );
}
