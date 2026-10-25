/**
 * Codex 会话归桶（中转站重构阶段 4，cc-switch 的 C5「Codex 历史统一」）。
 *
 * Codex 只列当前 provider 那个桶里的会话；用过别的切换工具、换过 provider 名，老会话就「不见了」。
 * 这里列出每个 Codex 目录里别的桶，使用者**逐个勾**要并进来的，再确认一次（写明改哪个目录、备份放哪）才动。
 * ⛔ 官方目录也一样要逐个勾；并之前先备份，还原按账本把每一处改回去（并之后 Codex 接着写的内容留着）。
 */

import { useEffect, useState } from "react";

import { stationApi, type CodexHistoryHome } from "../../lib/station";
import { useToast } from "../../ui";

export default function StationCodexHistory() {
  const toast = useToast();
  const [homes, setHomes] = useState<CodexHistoryHome[] | null>(null);
  /** 每个目录勾了哪些桶。 */
  const [picked, setPicked] = useState<Record<string, string[]>>({});
  const [asking, setAsking] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let dropped = false;
    void stationApi
      .codexHistory()
      .then((h) => !dropped && setHomes(h))
      .catch((e: unknown) => {
        if (!dropped) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      dropped = true;
    };
  }, []);

  const run = async (what: () => Promise<CodexHistoryHome[]>, ok: string) => {
    setBusy(true);
    setError(null);
    try {
      setHomes(await what());
      setPicked({});
      setAsking(null);
      toast.ok(ok);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const toggle = (home: string, bucket: string) =>
    setPicked((p) => {
      const now = p[home] ?? [];
      return {
        ...p,
        [home]: now.includes(bucket)
          ? now.filter((b) => b !== bucket)
          : [...now, bucket],
      };
    });

  if (!homes)
    return error ? (
      <p className="qb-st-note">读不到 Codex 的会话：{error}</p>
    ) : (
      <p className="qb-st-note">正在数会话……</p>
    );

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
      <p className="qb-st-note">
        Codex 只列当前那一家（<code>model_provider</code>
        ）的会话。用过别的切换工具、换过 provider
        名的话，老会话还在，只是挂在别的「桶」里。
        勾上要并进来的桶，确认后先备份、再改；随时能按备份还原。
        <b>先关掉 Codex</b>
        ，它开着时会话文件可能正在写。
      </p>
      {homes
        .filter((h) => h.exists)
        .map((h) => {
          const mine = picked[h.id] ?? [];
          return (
            <section key={h.id}>
              <span className="qb-st-k">
                {h.label}
                {h.official && (
                  <span className="qb-st-pill qb-st-pill--warn">官方</span>
                )}
              </span>
              <p className="qb-st-note">
                <code>{h.path}</code> · 现在列的是 <b>{h.current}</b>
                {h.compressed > 0 &&
                  ` · ${h.compressed} 个压缩过的会话这次不动（在 Codex 里打开一次就解压回来了）`}
              </p>
              {h.problem && <p className="qb-st-note">{h.problem}</p>}
              {h.buckets.length === 0 ? (
                <p className="qb-st-note">没有别的桶，会话都在当前这个里。</p>
              ) : (
                <div className="qb-st-tw">
                  <table>
                    <thead>
                      <tr>
                        <th />
                        <th>桶（provider）</th>
                        <th className="num">会话文件</th>
                        <th className="num">状态库</th>
                      </tr>
                    </thead>
                    <tbody>
                      {h.buckets.map((b) => (
                        <tr key={b.provider}>
                          <td>
                            <input
                              type="checkbox"
                              aria-label={`把 ${b.provider} 并进 ${h.current}`}
                              checked={mine.includes(b.provider)}
                              disabled={busy}
                              onChange={() => toggle(h.id, b.provider)}
                            />
                          </td>
                          <td>
                            <code>{b.provider}</code>
                          </td>
                          <td className="num">{b.sessions}</td>
                          <td className="num">{b.threads}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
              {mine.length > 0 &&
                (asking === h.id ? (
                  <p className="qb-st-note" role="alert">
                    把 <b>{mine.join("、")}</b> 并进 <b>{h.current}</b>（改的是{" "}
                    <code>{h.path}</code> 里的会话文件和状态库，先备份）？{" "}
                    <button
                      className="qb-st-iconbtn qb-st-iconbtn--go"
                      disabled={busy}
                      onClick={() =>
                        void run(
                          () => stationApi.codexHistoryMerge(h.id, mine),
                          `并好了：${mine.join("、")} 的会话现在列在 ${h.current} 下`,
                        )
                      }
                    >
                      确定并入
                    </button>{" "}
                    <button
                      className="qb-st-iconbtn"
                      disabled={busy}
                      onClick={() => setAsking(null)}
                    >
                      算了
                    </button>
                  </p>
                ) : (
                  <button
                    className="qb-st-iconbtn"
                    disabled={busy}
                    onClick={() => setAsking(h.id)}
                  >
                    并入勾选的 {mine.length} 个桶
                  </button>
                ))}
              {h.backups.length > 0 && (
                <details className="qb-st-adv">
                  <summary>备份 · {h.backups.length} 份</summary>
                  <ul className="qb-st-note">
                    {h.backups.map((b) => (
                      <li key={b.id}>
                        {new Date(b.at_ms).toLocaleString()}：
                        {b.from.join("、")} → {b.to}（{b.sessions} 个会话文件、
                        {b.threads} 行）{" "}
                        {b.restored ? (
                          <span className="qb-st-dim">已还原</span>
                        ) : (
                          <button
                            className="qb-st-iconbtn"
                            disabled={busy}
                            onClick={() =>
                              void run(
                                () =>
                                  stationApi.codexHistoryRestore(h.id, b.id),
                                "还原好了：那几个会话回到原来的桶",
                              )
                            }
                          >
                            还原
                          </button>
                        )}
                      </li>
                    ))}
                  </ul>
                </details>
              )}
            </section>
          );
        })}
      {error && (
        <p className="qb-st-note" role="alert">
          没成：{error}
        </p>
      )}
    </div>
  );
}
