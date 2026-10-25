/**
 * 把这条线写进共存型工具（中转站重构阶段 4，cc-switch 的 C3 / C4）：OpenCode、MiniMax Code、OpenClaw、Hermes、Pi。
 *
 * # ⛔ 写的是真 Key
 *
 * 这五个工具**不经本机路由**：站点地址和这条线绑着的那把 Key 明文写进它们自己的配置文件（使用者 2026-10-25 定的，
 * 照 cc-switch 的做法）。所以每次写入前都要在这里再点一次确认，确认那一行写着要改哪个文件。
 * 「Key 不出后端」那条规矩只管 Claude Code / Codex / 桌面端 / Gemini CLI / Grok Build 五个经本机路由的软件。
 *
 * 只动 QB Gate 自己写的那一条（`qb-gate-…`），使用者自己的供应商不碰；配置目录不在（多半没装）就不替它建。
 */

import { useEffect, useState } from "react";

import type { CoexistStatus } from "../../lib/generated/CoexistStatus";
import type { CoexistTool } from "../../lib/generated/CoexistTool";
import { stationApi } from "../../lib/station";
import { useToast } from "../../ui";

export default function StationCoexistPanel({ routeId }: { routeId: string }) {
  const toast = useToast();
  const [rows, setRows] = useState<CoexistStatus[] | null>(null);
  const [asking, setAsking] = useState<CoexistTool | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let dropped = false;
    setAsking(null);
    void stationApi
      .coexistStatus(routeId)
      .then((r) => !dropped && setRows(r))
      .catch((e: unknown) => {
        if (!dropped) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      dropped = true;
    };
  }, [routeId]);

  const run = async (what: () => Promise<CoexistStatus[]>, ok: string) => {
    setBusy(true);
    setError(null);
    try {
      setRows(await what());
      setAsking(null);
      toast.ok(ok);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!rows)
    return error ? (
      <p className="qb-st-note">读不到其他工具的配置：{error}</p>
    ) : null;

  const pending = rows.find((r) => r.tool === asking) ?? null;

  return (
    <div>
      <span className="qb-st-k">写进其他工具（不经本机路由）</span>
      <p className="qb-st-note">
        这几个工具直接连站点：
        <b>站点地址和这条线的 Key 会明文写进它们自己的配置文件</b>
        ，谁能读那个文件谁就拿得到 Key。只改 QB Gate
        写的那一条，你自己配的供应商不动。写进去的模型清单取自这条线的「模型映射」（没写就写不进去）；
        之后换 Key、改映射要回来再点一次「更新」。
      </p>
      <div className="qb-st-tw">
        <table>
          <thead>
            <tr>
              <th>工具</th>
              <th>配置文件</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.tool}>
                <td>
                  {r.label}{" "}
                  {r.written ? (
                    <span className="qb-st-pill qb-st-pill--ok">已写入</span>
                  ) : !r.installed ? (
                    <span className="qb-st-pill qb-st-pill--unknown">
                      没找到
                    </span>
                  ) : null}
                </td>
                <td>
                  <code>{r.path}</code>
                  {r.problem && (
                    <>
                      <br />
                      <span className="qb-st-dim">{r.problem}</span>
                    </>
                  )}
                </td>
                <td>
                  <span className="qb-st-inline">
                    <button
                      className="qb-st-iconbtn"
                      disabled={busy || !r.installed || Boolean(r.problem)}
                      title={
                        r.installed
                          ? "把站点地址和真 Key 写进这个工具的配置"
                          : "配置目录不在（多半没装），不替它建"
                      }
                      onClick={() => setAsking(r.tool)}
                    >
                      {r.written ? "更新" : "写入"}
                    </button>
                    {r.written && (
                      <button
                        className="qb-st-iconbtn"
                        disabled={busy}
                        aria-label={`从 ${r.label} 删掉这条线`}
                        onClick={() =>
                          void run(
                            () => stationApi.coexistRemove(routeId, r.tool),
                            `从 ${r.label} 删掉了 QB Gate 写的那一条`,
                          )
                        }
                      >
                        删除
                      </button>
                    )}
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {pending && (
        <p className="qb-st-note" role="alert">
          要把这条线的 <b>真 Key</b> 明文写进 <code>{pending.path}</code>？{" "}
          <button
            className="qb-st-iconbtn qb-st-iconbtn--go"
            disabled={busy}
            onClick={() =>
              void run(
                () => stationApi.coexistWrite(routeId, pending.tool),
                `写进了 ${pending.label}：重开它就能在供应商里选到`,
              )
            }
          >
            确定写入
          </button>{" "}
          <button
            className="qb-st-iconbtn"
            disabled={busy}
            onClick={() => setAsking(null)}
          >
            算了
          </button>
        </p>
      )}
      {error && <p className="qb-st-note">没成：{error}</p>}
    </div>
  );
}
