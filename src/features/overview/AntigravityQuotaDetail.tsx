import { useEffect, useState } from "react";
import { antigravityApi } from "../../lib/antigravity";
import {
  modelGroups,
  percent,
  quotaPools,
  spanLabel,
} from "../../lib/antigravityQuota";
import { resetPassed, useQuotaClock } from "../../lib/quotaClock";
import type { AntigravityIdentity } from "../../lib/generated/AntigravityIdentity";
import type { AntigravityOnlineQuota } from "../../lib/generated/AntigravityOnlineQuota";
import { Gauge } from "../../ui";
import QuotaCountdown from "./QuotaCountdown";

/** 详情页只读上次成功的缓存；所有联网刷新留在账户页已有入口。 */
export default function AntigravityQuotaDetail({
  accountId,
  identity,
  identityError,
}: {
  accountId: string | null;
  identity: AntigravityIdentity | null;
  identityError?: string | null;
}) {
  const now = useQuotaClock();
  const cacheKey = JSON.stringify([accountId, identity?.email ?? null]);
  const [cached, setCached] = useState<{
    key: string;
    quota: AntigravityOnlineQuota | null;
    error: string;
  } | null>(null);
  useEffect(() => {
    let cancelled = false;
    if (!accountId) return;
    antigravityApi.accountQuota(accountId, false).then(
      (quota) => {
        if (!cancelled) setCached({ key: cacheKey, quota, error: "" });
      },
      (error: unknown) => {
        if (!cancelled)
          setCached({
            key: cacheKey,
            quota: null,
            error: error instanceof Error ? error.message : String(error),
          });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [accountId, cacheKey]);
  // 切换账户的同一次 render 即隐藏旧结果，不等 effect 清空。
  const result = cached?.key === cacheKey ? cached : null;
  const online = result?.quota;
  const hasOnline =
    !!online && (online.windows.length > 0 || online.models.length > 0);
  const models = hasOnline ? online.models : (identity?.models ?? []);
  return (
    <div data-testid="ag-quota-detail">
      {hasOnline && online.windows.length > 0 ? (
        <div className="slotusage-bars">
          {quotaPools(online.windows).map((pool) => (
            <div
              key={pool.id}
              className="slotusage"
              aria-label={`配额池 ${pool.id}`}
            >
              <strong title={pool.id}>{pool.label}</strong>
              {pool.windows.map((window) => (
                <Gauge
                  key={window.span}
                  name={spanLabel(window.span)}
                  className="gauge--countdown"
                  used={
                    resetPassed(window.reset_epoch, now)
                      ? null
                      : 100 - window.remaining * 100
                  }
                  value={
                    <>
                      剩 {percent(window.remaining)}
                      {window.remaining_implied && (
                        <em className="ag-implied">推算</em>
                      )}
                    </>
                  }
                  hint={[
                    window.reset_at,
                    window.note,
                    window.remaining_implied
                      ? "Google 未给比例，按 proto3 缺省值推为 0"
                      : "",
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                  extra={
                    <QuotaCountdown epoch={window.reset_epoch} now={now} />
                  }
                />
              ))}
            </div>
          ))}
        </div>
      ) : models.length ? (
        <div className="slotusage">
          {modelGroups(models).map(({ key, label, model }) => (
            <Gauge
              key={key}
              name={label}
              className="gauge--countdown"
              used={
                model.remaining == null || resetPassed(model.reset_epoch, now)
                  ? null
                  : 100 - model.remaining * 100
              }
              value={
                <>
                  剩 {percent(model.remaining)}
                  {model.remaining_implied && (
                    <em className="ag-implied">推算</em>
                  )}
                </>
              }
              hint={[
                model.model_key,
                model.reset_at,
                model.remaining_implied
                  ? "Google 未给比例，按 proto3 缺省值推为 0"
                  : "",
              ]
                .filter(Boolean)
                .join(" · ")}
              extra={<QuotaCountdown epoch={model.reset_epoch} now={now} />}
            />
          ))}
        </div>
      ) : (
        <p className="notice">
          还没有可显示的额度记录；到官方账户页刷新后可在此查看缓存。
        </p>
      )}
      {hasOnline ? (
        <p className="notice mt-2">
          上次成功的在线额度缓存 · {online.last_success ?? online.fetched_at}
          。本页只读缓存，未发起联网查询；到点后需在账户页刷新确认额度。
        </p>
      ) : (
        <p className="notice mt-2">
          {result?.error
            ? `在线额度缓存读不出来：${result.error}。`
            : accountId && !result
              ? "正在读取在线额度缓存。"
              : "没有已缓存的在线配额池。"}
          {identity ? (
            <>
              当前显示反重力 IDE 的本地模型记录，只有上次同步时那么新（IDE 写入{" "}
              {identity.written_at}），不代表在线配额池；本页零网络查询。
            </>
          ) : identityError ? (
            `本地账户状态读不出来：${identityError}`
          ) : (
            "本地 IDE 也还没有额度记录。"
          )}
        </p>
      )}
    </div>
  );
}
