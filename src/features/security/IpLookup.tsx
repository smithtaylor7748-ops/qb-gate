import { api } from "../../lib/api";
import type { IpLookupReport } from "../../lib/generated/IpLookupReport";
import { getSession, useSession } from "../../lib/store";
import { Metric, Pill } from "../../ui";

const BUSY = "purity.lookup.busy";

/** 指定地址的结果只用于显示，不能成为当前出口或门禁输入。 */
export function useIpLookup() {
  const [input, saveInput] = useSession("purity.lookup.input", "");
  const [busy, setBusy] = useSession(BUSY, false);
  const [error, setError] = useSession("purity.lookup.error", "");
  const [result, setResult] = useSession<IpLookupReport | null>(
    "purity.lookup.result",
    null,
  );

  function setInput(value: string) {
    saveInput(value);
    setResult(null);
    setError("");
  }

  async function run() {
    if (getSession(BUSY, false) || !input.trim()) return;
    setBusy(true);
    setError("");
    setResult(null);
    try {
      setResult(await api.lookupIp(input.trim()));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }
  return { input, setInput, busy, error, result, run };
}

export function IpLookupResult({
  lookup,
}: {
  lookup: ReturnType<typeof useIpLookup>;
}) {
  const { result, busy, error } = lookup;
  return (
    <div
      data-testid={result ? "ip-lookup-result" : undefined}
      aria-live="polite"
    >
      {error && (
        <p role="alert" className="notice notice--danger mb-2">
          {error}
        </p>
      )}
      <div className="grid gap-2 sm:grid-cols-2">
        <Metric label="风险分（IPQuery）" loading={busy} emptyText="未知">
          {result?.risk_score == null
            ? undefined
            : `${result.risk_score} / 100`}
        </Metric>
        <Metric label="指定 IP" loading={busy} mono>
          {result?.ip}
        </Metric>
        <Metric label="ASN / 运营商" loading={busy}>
          {result
            ? [result.asn, result.organization].filter(Boolean).join(" · ")
            : undefined}
        </Metric>
        <Metric label="位置 / 时区" loading={busy}>
          {result
            ? [
                result.country_code ?? result.country,
                result.city,
                result.timezone,
              ]
                .filter(Boolean)
                .join(" · ")
            : undefined}
        </Metric>
      </div>
      {result && (
        <>
          <div className="mt-2 flex flex-wrap gap-2">
            {(
              [
                ["代理", result.is_proxy],
                ["VPN", result.is_vpn],
                ["Tor", result.is_tor],
                ["机房", result.is_datacenter],
              ] as const
            ).map(([label, flagged]) => (
              <Pill key={label} tone={flagged === true ? "warn" : "default"}>
                {label}：
                {flagged == null ? "未知" : flagged ? "已标记" : "未标记"}
              </Pill>
            ))}
          </div>
          <p className="notice mt-2">
            来源：{result.source} ·{" "}
            {new Date(result.checked_at).toLocaleString("zh-CN")} ·
            分数越低，来源评估的风险越低。
          </p>
        </>
      )}
    </div>
  );
}
