/**
 * 站点的备用地址（中转站重构阶段 4，cc-switch 的 P3 多端点 + 测速）。
 *
 * 照 cc-switch 的「端点测速」那一块的交互：一个地址一行、测速看耗时、挑一个当主地址。
 * 跟它不一样的地方：备用地址**参与故障转移** —— 主地址根本连不上时，本机路由同一发改打备用的（同一把 Key），
 * 按上次测速从快到慢。拿到了响应（哪怕是报错）不换地址，那是站点自己的答复。
 *
 * 测速不带 Key、不调用模型、不花钱，只在点「测速」时跑；设了出站代理就经它测（跟中转请求走同一条路）。
 */

import { useEffect, useState } from "react";

import type { EndpointLatency } from "../../lib/generated/EndpointLatency";
import { stationApi, type StationEndpoints } from "../../lib/station";
import { Button, useToast } from "../../ui";

export default function StationEndpointsPanel({
  providerId,
  mainUrl,
  onMainChanged,
}: {
  providerId: string;
  mainUrl: string;
  /** 换了主地址之后回调（表单里的地址栏跟着换）。 */
  onMainChanged?: (url: string) => void;
}) {
  const toast = useToast();
  const [data, setData] = useState<StationEndpoints | null>(null);
  const [main, setMain] = useState<EndpointLatency | null>(null);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let dropped = false;
    setMain(null);
    void stationApi
      .endpoints(providerId)
      .then((d) => !dropped && setData(d))
      .catch((e: unknown) => {
        if (!dropped) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      dropped = true;
    };
  }, [providerId]);

  const run = async (what: () => Promise<StationEndpoints>, ok?: string) => {
    setBusy(true);
    setError(null);
    try {
      setData(await what());
      if (ok) toast.ok(ok);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const test = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await stationApi.testEndpoints(providerId);
      setMain(r.main);
      setData(r.endpoints);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const promote = (url: string) =>
    run(async () => {
      const d = await stationApi.promoteEndpoint(providerId, url);
      setMain(null);
      onMainChanged?.(url);
      return d;
    }, "换好了：这个站点的主地址改成了它，原来的挪进备用");

  if (!data)
    return error ? <p className="qb-st-note">读不到备用地址：{error}</p> : null;

  return (
    <div>
      <span className="qb-st-k">备用地址（同一个站点的其他地址）</span>
      <p className="qb-st-note">
        主地址根本连不上时，这一发改打下面的地址（同一把
        Key），按上次测速从快到慢； 站点答了报错不换地址。测速不带 Key、不花钱。
      </p>
      <div className="qb-st-tw">
        <table>
          <thead>
            <tr>
              <th>地址</th>
              <th className="num">耗时</th>
              <th />
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>
                <code>{mainUrl}</code>{" "}
                <span className="qb-st-pill qb-st-pill--ok">主地址</span>
              </td>
              <td className="num">{latencyText(main)}</td>
              <td />
            </tr>
            {data.endpoints.map((e) => (
              <tr key={e.url}>
                <td>
                  <code>{e.url}</code>
                </td>
                <td className="num">
                  {e.error ? (
                    <span className="qb-st-dim">{e.error}</span>
                  ) : e.latency_ms != null ? (
                    `${e.latency_ms} ms`
                  ) : (
                    <span className="qb-st-dim">没测过</span>
                  )}
                </td>
                <td>
                  <span className="qb-st-inline">
                    <button
                      className="qb-st-iconbtn"
                      disabled={busy}
                      title="把它换成这个站点的主地址；原来的主地址挪进备用"
                      onClick={() => void promote(e.url)}
                    >
                      设为主地址
                    </button>
                    <button
                      className="qb-st-iconbtn"
                      disabled={busy}
                      aria-label={`删掉备用地址 ${e.url}`}
                      onClick={() =>
                        void run(() =>
                          stationApi.removeEndpoint(providerId, e.url),
                        )
                      }
                    >
                      删除
                    </button>
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <span className="qb-st-inline">
        <input
          value={draft}
          aria-label="新的备用地址"
          placeholder="https://另一个地址"
          onChange={(e) => setDraft(e.target.value)}
        />
        <button
          className="qb-st-iconbtn"
          disabled={busy || draft.trim() === ""}
          onClick={() =>
            void run(async () => {
              const d = await stationApi.addEndpoint(providerId, draft);
              setDraft("");
              return d;
            })
          }
        >
          添加
        </button>
      </span>
      {error && (
        <div className="qb-st-nudge">
          <span aria-hidden="true">⚠</span>
          <span>{error}</span>
        </div>
      )}
      <div className="qb-st-inline" style={{ justifyContent: "flex-end" }}>
        <Button variant="ghost" disabled={busy} onClick={() => void test()}>
          {busy ? "测速中…" : "测速"}
        </Button>
      </div>
    </div>
  );
}

function latencyText(r: EndpointLatency | null) {
  if (!r) return <span className="qb-st-dim">—</span>;
  if (r.error) return <span className="qb-st-dim">{r.error}</span>;
  return r.latency_ms != null ? `${r.latency_ms} ms` : "—";
}
