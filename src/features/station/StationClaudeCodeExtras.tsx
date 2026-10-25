/**
 * Claude Code 的两个小件（中转站重构阶段 4，cc-switch 的 C5）：跳过首次确认、VS Code 插件联动。
 *
 * 跳过首次确认：默认只给中转环境；官方默认目录和官方槽位**要点名**（每一行自己一个按钮）。
 * 撤销只撤面板自己设的 —— 使用者真走完过一次、Claude Code 自己记下的那个不动。
 *
 * VS Code 插件联动：编辑器的用户设置里加两条变量（中转环境目录 + 本机路由的占位 Key）、关掉登录提示。
 * ⛔ 真 Key 不进编辑器设置。使用者自己设了同名变量时后端拒绝、不覆盖。
 */

import { useEffect, useState } from "react";

import {
  stationApi,
  type EditorLink,
  type OnboardingTarget,
} from "../../lib/station";
import { useToast } from "../../ui";

export default function StationClaudeCodeExtras() {
  const toast = useToast();
  const [targets, setTargets] = useState<OnboardingTarget[] | null>(null);
  const [editors, setEditors] = useState<EditorLink[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let dropped = false;
    void Promise.all([stationApi.onboardingStatus(), stationApi.vscodeStatus()])
      .then(([t, e]) => {
        if (dropped) return;
        setTargets(t);
        setEditors(e);
      })
      .catch((e: unknown) => {
        if (!dropped) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      dropped = true;
    };
  }, []);

  const run = async <T,>(
    what: () => Promise<T>,
    put: (v: T) => void,
    ok: string,
  ) => {
    setBusy(true);
    setError(null);
    try {
      put(await what());
      toast.ok(ok);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const installed = (editors ?? []).filter((e) => e.installed || e.linked);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
      <section>
        <span className="qb-st-k">跳过首次确认</span>
        <p className="qb-st-note">
          在 <code>.claude.json</code> 里记上「首次确认已经走完」，Claude Code
          起来就不再走欢迎页那几步。中转环境建议打开；官方目录要你在那一行自己点。
          撤销只撤面板设的。
        </p>
        {targets ? (
          <div className="qb-st-tw">
            <table>
              <tbody>
                {targets.map((t) => (
                  <tr key={t.id}>
                    <td>
                      {t.label}{" "}
                      {t.skipped && (
                        <span className="qb-st-pill qb-st-pill--ok">
                          {t.ours ? "已跳过（面板设的）" : "已跳过"}
                        </span>
                      )}
                      <br />
                      <code className="qb-st-dim">{t.path}</code>
                      {t.problem && (
                        <>
                          <br />
                          <span className="qb-st-dim">{t.problem}</span>
                        </>
                      )}
                    </td>
                    <td>
                      {!t.skipped ? (
                        <button
                          className="qb-st-iconbtn"
                          disabled={busy || Boolean(t.problem)}
                          title={
                            t.official
                              ? "只改这一个官方目录"
                              : "只改中转环境那一份"
                          }
                          onClick={() =>
                            void run(
                              () => stationApi.setOnboarding(t.id, true),
                              setTargets,
                              `${t.label}：下次起 Claude Code 不再走首次确认`,
                            )
                          }
                        >
                          跳过
                        </button>
                      ) : t.ours ? (
                        <button
                          className="qb-st-iconbtn"
                          disabled={busy}
                          onClick={() =>
                            void run(
                              () => stationApi.setOnboarding(t.id, false),
                              setTargets,
                              `${t.label}：恢复了首次确认`,
                            )
                          }
                        >
                          恢复
                        </button>
                      ) : null}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : null}
      </section>

      <section>
        <span className="qb-st-k">VS Code 插件联动</span>
        <p className="qb-st-note">
          让编辑器里的 Claude Code
          插件也走本机路由：在编辑器的用户设置里加两条变量（
          <code>CLAUDE_CONFIG_DIR</code> 指到中转环境、
          <code>ANTHROPIC_AUTH_TOKEN</code> 是本机路由的占位串，
          <b>不是你的 Key</b>
          ），并关掉插件的登录提示。改完重开一次插件面板。撤销只删这两条，登录提示还原。
        </p>
        {editors ? (
          installed.length === 0 ? (
            <p className="qb-st-note">
              没找到装着的编辑器（VS Code、Cursor、Windsurf、VSCodium）。
            </p>
          ) : (
            <div className="qb-st-tw">
              <table>
                <tbody>
                  {installed.map((e) => (
                    <tr key={e.editor}>
                      <td>
                        {e.label}{" "}
                        {e.linked && (
                          <span className="qb-st-pill qb-st-pill--ok">
                            已联动
                          </span>
                        )}
                        <br />
                        <code className="qb-st-dim">{e.path}</code>
                        {e.problem && (
                          <>
                            <br />
                            <span className="qb-st-dim">{e.problem}</span>
                          </>
                        )}
                      </td>
                      <td>
                        <button
                          className="qb-st-iconbtn"
                          disabled={busy || Boolean(e.problem)}
                          onClick={() =>
                            void (e.linked
                              ? run(
                                  () => stationApi.vscodeUnlink(e.editor),
                                  setEditors,
                                  `${e.label}：撤销了联动`,
                                )
                              : run(
                                  () => stationApi.vscodeLink(e.editor),
                                  setEditors,
                                  `${e.label}：插件改走本机路由了，重开插件面板生效`,
                                ))
                          }
                        >
                          {e.linked ? "撤销" : "联动"}
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )
        ) : null}
      </section>
      {error && (
        <p className="qb-st-note" role="alert">
          没成：{error}
        </p>
      )}
    </div>
  );
}
