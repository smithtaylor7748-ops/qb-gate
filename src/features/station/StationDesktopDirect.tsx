/**
 * Claude 桌面端直连档（中转站重构阶段 4，cc-switch 的 C5；使用者 2026-10-26 定的：写真 Key）。
 *
 * 平时桌面端经本机路由（配置里是占位串）。选一条线直连之后，桌面端那份配置里的网关地址换成那个站点、
 * Key 换成**那条线的真 Key**（明文）—— 换线、故障转移、熔断、请求日志、模型映射都不再管桌面端。
 * 所以每次切换都要再确认一次，确认那一行把这几件事说清楚；重开桌面端才生效。
 */

import { useEffect, useState } from "react";

import {
  stationApi,
  type DesktopDirectView,
  type Route,
} from "../../lib/station";
import { useToast } from "../../ui";

export default function StationDesktopDirect({
  routes,
  label,
  onChanged,
}: {
  /** 桌面端自己的线。 */
  routes: Route[];
  label: (r: Route) => string;
  onChanged?: (view: DesktopDirectView) => void;
}) {
  const toast = useToast();
  const [view, setView] = useState<DesktopDirectView | null>(null);
  /** 选中但还没确认的：线路 id；`""` = 改回本机路由。`null` = 没在选。 */
  const [pick, setPick] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let dropped = false;
    void stationApi
      .desktopDirect()
      .then((v) => !dropped && setView(v))
      .catch((e: unknown) => {
        if (!dropped) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      dropped = true;
    };
  }, []);

  if (!view)
    return error ? (
      <p className="qb-st-note">读不到桌面端的接入方式：{error}</p>
    ) : null;

  const current = view.route_id ?? "";
  const problemOf = (id: string) =>
    view.candidates.find((c) => c.route_id === id)?.problem ?? null;
  const pickedRoute = routes.find((r) => r.id === pick) ?? null;

  const confirm = async () => {
    if (pick == null) return;
    setBusy(true);
    setError(null);
    try {
      const v = await stationApi.setDesktopDirect(pick || null);
      setView(v);
      setPick(null);
      onChanged?.(v);
      toast.ok(
        pick
          ? "改成直连了：重开 Claude 桌面端生效"
          : "改回经本机路由了：重开 Claude 桌面端生效",
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <p className="qb-st-note">
        平时桌面端经本机路由，配置里只有占位串。直连一条线时，
        <b>那个站点的地址和这条线的 API Key 会明文写进桌面端的配置文件</b>（
        <code>claude_desktop_config.json</code>，在面板的中转环境目录里）；
        换线、故障转移、熔断、请求日志、模型映射都不再管桌面端。
        只有没设模型映射、说 Anthropic 协议的线能直连 —— 站点要认桌面端发的
        claude-* 模型名。
      </p>
      {view.problem && (
        <p className="qb-st-note" role="alert">
          {view.problem}
        </p>
      )}
      <div className="qb-st-tw">
        <table>
          <tbody>
            <tr>
              <td>
                <label className="qb-st-check">
                  <input
                    type="radio"
                    name="desktop-direct"
                    checked={(pick ?? current) === ""}
                    disabled={busy}
                    onChange={() => setPick(current === "" ? null : "")}
                  />
                  经本机路由（默认）
                </label>
              </td>
              <td />
            </tr>
            {routes.map((r) => {
              const problem = problemOf(r.id);
              return (
                <tr key={r.id}>
                  <td>
                    <label className="qb-st-check">
                      <input
                        type="radio"
                        name="desktop-direct"
                        checked={(pick ?? current) === r.id}
                        disabled={busy || Boolean(problem)}
                        onChange={() => setPick(current === r.id ? null : r.id)}
                      />
                      直连 {label(r)}
                    </label>
                    {current === r.id && (
                      <span className="qb-st-pill qb-st-pill--warn">
                        直连中
                      </span>
                    )}
                  </td>
                  <td>
                    {problem && <span className="qb-st-dim">{problem}</span>}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {pick != null && (
        <p className="qb-st-note" role="alert">
          {pickedRoute ? (
            <>
              要把 <b>{label(pickedRoute)}</b> 的站点地址和<b>真 Key</b>
              明文写进桌面端的配置？之后桌面端不经过本机路由。
            </>
          ) : (
            <>改回经本机路由？桌面端配置里的真 Key 换回占位串。</>
          )}{" "}
          <button
            className="qb-st-iconbtn qb-st-iconbtn--go"
            disabled={busy}
            onClick={() => void confirm()}
          >
            确定
          </button>{" "}
          <button
            className="qb-st-iconbtn"
            disabled={busy}
            onClick={() => setPick(null)}
          >
            算了
          </button>
        </p>
      )}
      {error && <p className="qb-st-note">没成：{error}</p>}
    </div>
  );
}
