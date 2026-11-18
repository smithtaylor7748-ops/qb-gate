import type { RouterSettings } from "../../lib/generated/RouterSettings";
import type { Route } from "../../lib/generated/Route";

/** 编辑草稿；跟备用线路一起提交。设了限额不等于把这条线加为备用线路。 */
export function StationAdmissionSettings({
  settings,
  routes,
  label,
  onChange,
}: {
  settings: RouterSettings;
  routes: Route[];
  label: (route: Route) => string;
  onChange: (patch: Partial<RouterSettings>) => void;
}) {
  const { admission } = settings;
  const overflow =
    settings.routing_mode === "pool" && admission.overflow_to_pool;
  const useLoad = overflow && settings.pool_strategy === "least-loaded";
  const setAdmission = (patch: Partial<RouterSettings["admission"]>) =>
    onChange({ admission: { ...admission, ...patch } });

  return (
    <section className="qb-st-admission-settings" aria-label="请求容量设置">
      <h4 className="qb-st-sect">限流与容量分流</h4>
      <label className="qb-st-admission-check">
        <input
          type="checkbox"
          aria-label="本机限额满了时分流到备用线路"
          checked={overflow}
          disabled={settings.routing_mode !== "pool"}
          onChange={(event) =>
            // 关分流时把「怎么分」一起复位成优先当前线路（C3）：不然再开分流就立刻按负载分到别的站。
            onChange({
              admission: {
                ...admission,
                overflow_to_pool: event.target.checked,
              },
              ...(event.target.checked
                ? {}
                : { pool_strategy: "priority" as const }),
            })
          }
        />
        本机限额满了时分流到备用线路
      </label>
      <p className="qb-st-note">
        默认关闭，只管下面这几条你自己设的本机限额（并发、每分钟）；上游说限流（429）、额度用光是线路故障，有备用线路就直接改用，跟这个开关无关。
        例：当前线路并发上限设为 2，两个请求还在进行时，第 3
        个会去备用线路；当前线路空出来后新请求回到它。
        关着时达到上限会等待或返回繁忙。分流按独立请求进行，不拆分一条回复；需要服务端会话状态的对话请清空备用线路。
      </p>
      {settings.routing_mode === "pool" && (
        <>
          <h4 className="qb-st-sect">分流时怎么分</h4>
          <div className="qb-st-routing-options">
            <label
              className={`qb-st-routing-option${!useLoad ? " active" : ""}`}
            >
              <input
                type="radio"
                name={`pool-strategy-${settings.client}`}
                checked={!useLoad}
                onChange={() => onChange({ pool_strategy: "priority" })}
              />
              <span>
                <strong>先用当前线路</strong>
                <span>优先当前线路，满了才去备用线路（默认）。</span>
              </span>
            </label>
            <label
              className={`qb-st-routing-option${useLoad ? " active" : ""}`}
            >
              <input
                type="radio"
                name={`pool-strategy-${settings.client}`}
                disabled={!admission.overflow_to_pool}
                checked={useLoad}
                onChange={() => onChange({ pool_strategy: "least-loaded" })}
              />
              <span>
                <strong>优先空闲线路</strong>
                <span>
                  开了分流才可选：每次请求在当前线路和备用线路里比较负载，当前线路没满也可能用备用线路、花它的额度。
                </span>
              </span>
            </label>
          </div>
        </>
      )}
      <h4 className="qb-st-sect">每条线路的请求容量</h4>
      <p className="qb-st-note">
        默认不限制。这里只约束经过本机路由的 API
        中转请求，不限制官方账户；设了限额也不会把线路加为备用线路。
        同一线路的所有对话共用上限，流式回复结束或取消后释放名额。
        没有备用线路时，达到限制也不会改用其他站。
        每分钟按实际发送次数计，重试与测活也计入；等待和本机拒绝不计入。
      </p>
      <div className="qb-st-admission-routes">
        {routes.length === 0 && (
          <p className="qb-st-note">添加线路后可分别设置。</p>
        )}
        {routes.map((route) => {
          const limit = admission.route_limits[route.id] ?? {
            max_concurrent: 0,
            rpm: 0,
            check_quota: false,
          };
          const setLimit = (patch: Partial<typeof limit>) =>
            setAdmission({
              route_limits: {
                ...admission.route_limits,
                [route.id]: { ...limit, ...patch },
              },
            });
          return (
            <div key={route.id} className="qb-st-admission-route">
              <strong>{label(route)}</strong>
              <div className="qb-st-admission-fields">
                <AdmissionNumber
                  label="同时进行的请求上限"
                  accessibleLabel={`${label(route)} · 同时进行的请求上限`}
                  value={limit.max_concurrent}
                  max={1024}
                  onChange={(max_concurrent) => setLimit({ max_concurrent })}
                />
                <AdmissionNumber
                  label="每分钟请求上限"
                  accessibleLabel={`${label(route)} · 每分钟请求上限`}
                  value={limit.rpm}
                  max={60000}
                  onChange={(rpm) => setLimit({ rpm })}
                />
              </div>
              <label className="qb-st-admission-check">
                <input
                  type="checkbox"
                  checked={limit.check_quota}
                  onChange={(event) =>
                    setLimit({ check_quota: event.target.checked })
                  }
                  aria-label={`${label(route)} · 使用已查询的额度状态`}
                />
                使用已查询的额度状态
              </label>
            </div>
          );
        })}
      </div>
      <p className="qb-st-note">
        额度检查只采用近期查询中上游明确的不可用状态（明确用光了就当这条线故障，有备用线路就改用）。仅余额为
        0、未查询或缓存过期时不据此拦截，也不会自动联网刷新额度。这不是消费预算上限；余额本身显示在站点卡表头。
      </p>
      <h4 className="qb-st-sect">容量不足时等待</h4>
      <div className="qb-st-admission-fields">
        <AdmissionNumber
          label="最多等待（毫秒）"
          value={admission.max_wait_ms}
          max={60000}
          onChange={(max_wait_ms) => setAdmission({ max_wait_ms })}
          zeroHint="0 = 不等待"
        />
        <AdmissionNumber
          label="每条线路最多排队请求数"
          value={admission.max_waiters}
          max={1024}
          onChange={(max_waiters) => setAdmission({ max_waiters })}
          zeroHint="0 = 不排队"
        />
      </div>
      <p className="qb-st-note">
        开着分流时先试当前线路和备用线路里空着的，都忙才等待；关着分流时只等当前那一条。
        两项都大于 0 才会排队；任一为 0 时，容量不足就返回本机繁忙。
        队列满或超时也会结束等待，不会多用名单外的线路。
      </p>
    </section>
  );
}

function AdmissionNumber({
  label,
  accessibleLabel,
  value,
  max,
  onChange,
  zeroHint = "0 = 不限制",
}: {
  label: string;
  accessibleLabel?: string;
  value: number;
  max: number;
  onChange: (value: number) => void;
  zeroHint?: string;
}) {
  return (
    <label className="qb-st-fieldrow">
      <span>{label}</span>
      <input
        type="number"
        min={0}
        max={max}
        step={1}
        value={value}
        aria-label={accessibleLabel ?? label}
        onChange={(event) => onChange(Number(event.target.value))}
      />
      <span className="qb-st-dim">{zeroHint}</span>
    </label>
  );
}

export function admissionSettingsProblem(
  settings: RouterSettings,
): string | null {
  const { max_wait_ms, max_waiters, route_limits } = settings.admission;
  const limits = Object.values(route_limits).filter((limit) => limit != null);
  const values = [
    max_wait_ms,
    max_waiters,
    ...limits.flatMap((limit) => [limit.max_concurrent, limit.rpm]),
  ];
  if (!values.every((value) => Number.isSafeInteger(value) && value >= 0))
    return "请求容量与等待设置必须是大于或等于 0 的整数。";
  if (max_wait_ms > 60000 || max_waiters > 1024)
    return "最多等待 60000 毫秒，每条线路最多排队 1024 个请求。";
  if (limits.some((limit) => limit.max_concurrent > 1024 || limit.rpm > 60000))
    return "每条线路最多同时进行 1024 个请求，每分钟最多 60000 个请求；0 表示不限制。";
  return null;
}
