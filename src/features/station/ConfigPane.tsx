/**
 * 配置一节（§2.5 第 14 项）：快捷开关 + 模型映射 + 一个可手改的编辑器。
 *
 * 改的是**这个软件走本机路由时那份配置**。纯逻辑全在 `lib/clientConfig.ts`
 * （那边有单测），这里只管界面。
 *
 * # ⛔ 一个软件一份，不是一条线路一份
 *
 * 换上游不重启客户端 —— 那正是本机路由存在的理由。客户端只在启动时读一次配置，
 * 所以「这条线路的配置」这个概念根本不成立：六条线轮着走，那份 settings.json
 * 自始至终是同一份。做成一条线一份的话，改另外五条完全没有反应，
 * 而没有任何地方说得清为什么。
 *
 * # ⛔ 「跟随表单」是活的，但手改优先
 *
 * 开关一动，JSON 实时跟着变。可一旦有人手改过 JSON，就停在「已手改」，
 * 表单不再覆盖它 —— 覆盖掉的是别人刚敲进去的东西，而且没有撤销。
 * 要回到跟随，得自己点一下。
 */

import { useCallback, useEffect, useState } from "react";

import type { Client } from "../../lib/generated/Client";
import {
  CODEX_COMPACT_LIMIT,
  CODEX_EFFORTS,
  CODEX_ONE_M,
  compose,
  hasOneM,
  jsonProblem,
  MODEL_ROLES,
  QUICK_SWITCHES,
  readModels,
  removeTomlInt,
  setOneM,
  setTomlInt,
  setTomlStr,
  tomlInt,
  tomlStr,
  type ConfigObject,
} from "../../lib/clientConfig";
import { stationApi } from "../../lib/station";
import { Button } from "../../ui";

export default function ConfigPane({ client }: { client: Client }) {
  const [path, setPath] = useState("");
  const [format, setFormat] = useState("json");
  const [text, setText] = useState("");
  /** 库里那一份，用来判断「改过没有」。 */
  const [saved, setSaved] = useState("");
  const [on, setOn] = useState<Record<string, boolean>>({});
  const [models, setModels] = useState<Record<string, string>>({});
  /** `true` = JSON 跟着表单走；`false` = 手改过，表单不再覆盖它。 */
  const [follow, setFollow] = useState(true);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      const c = await stationApi.clientConfig(client);
      setPath(c.path);
      setFormat(c.format);
      setText(c.text);
      setSaved(c.text);
      setFollow(true);
      if (c.format !== "json") return;
      try {
        const o = JSON.parse(c.text) as ConfigObject;
        setOn(Object.fromEntries(QUICK_SWITCHES.map((s) => [s.id, s.read(o)])));
        setModels(readModels(o));
      } catch {
        // 读回来的就不是合法 JSON —— 开关状态判断不了，一律按「没开」显示，
        // 而且停在「已手改」不去覆盖它。
        setOn({});
        setModels({});
        setFollow(false);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, [client]);

  useEffect(() => {
    void load();
  }, [load]);

  /** 改完表单顺带重算 JSON —— 手改过就不动编辑器。 */
  const sync = (
    nextOn: Record<string, boolean>,
    nextModels: Record<string, string>,
  ) => {
    setOn(nextOn);
    setModels(nextModels);
    if (follow) setText(compose(text, nextOn, nextModels));
  };

  const toggle = (id: string) => sync({ ...on, [id]: !on[id] }, models);
  const setModel = (key: string, value: string) =>
    sync(on, { ...models, [key]: value });
  const toggleOneM = (key: string) => {
    const cur = models[key] ?? "";
    sync(on, { ...models, [key]: setOneM(cur, !hasOneM(cur)) });
  };

  /** 一键设置：把主模型那一格套到所有角色上。 */
  const spreadMain = () => {
    const main = (models.ANTHROPIC_MODEL ?? "").trim();
    const next = { ...models };
    for (const r of MODEL_ROLES) next[r.key] = main;
    sync(on, next);
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      const c = await stationApi.clientConfigSave(client, text);
      setText(c.text);
      setSaved(c.text);
      setNote("已保存，并重新应用了一次配置");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  // 三个软件三份文件，三种界面。⛔ 桌面端不能跟 Claude Code 共用那一套 ——
  // 六个快捷开关和模型映射写的全是 Claude Code 的环境变量，桌面端不读，
  // 勾了会往它的配置里塞一堆没人认的键，而界面上看起来一切正常。
  const problem = format !== "toml" ? jsonProblem(text) : null;
  const dirty = text !== saved;
  const json = format === "json";
  const desktop = format === "desktop";

  return (
    <div className="qb-st-cfg">
      <p className="qb-st-note">
        这份配置<b>整个软件共用</b>，不是这一条线路的 ——
        换上游不重启客户端，所以配置也换不了。
      </p>

      {json ? (
        <>
          <div className="qb-st-quicks">
            {QUICK_SWITCHES.map((s) => (
              <label key={s.id} className="qb-st-check" title={s.hint}>
                <input
                  type="checkbox"
                  checked={Boolean(on[s.id])}
                  onChange={() => toggle(s.id)}
                />
                {s.label}
              </label>
            ))}
          </div>

          <div>
            <div className="qb-st-cfghd">
              <span className="qb-st-k">模型映射</span>
              <span className="qb-st-note">
                空着 = 原样透传。<b>1M</b> 那一格是在模型名末尾加
                <code>[1M]</code> 后缀 —— 没有对应的环境变量，就是这么写的。
              </span>
              <span style={{ flex: 1 }} />
              <Button
                variant="ghost"
                disabled={!(models.ANTHROPIC_MODEL ?? "").trim()}
                onClick={spreadMain}
              >
                一键套到所有角色
              </Button>
            </div>
            <div className="qb-st-tw">
              <table>
                <thead>
                  <tr>
                    <th>模型角色</th>
                    <th>实际请求模型</th>
                    <th>菜单显示名</th>
                    <th className="num">1M</th>
                  </tr>
                </thead>
                <tbody>
                  {MODEL_ROLES.map((r) => (
                    <tr key={r.key} title={r.hint}>
                      <td>{r.label}</td>
                      <td>
                        <input
                          value={models[r.key] ?? ""}
                          placeholder="原样透传"
                          onChange={(e) => setModel(r.key, e.target.value)}
                        />
                      </td>
                      <td>
                        {r.nameKey ? (
                          <input
                            value={models[r.nameKey] ?? ""}
                            placeholder="跟实际模型一样"
                            onChange={(e) =>
                              setModel(String(r.nameKey), e.target.value)
                            }
                          />
                        ) : (
                          <span className="qb-st-dim">—</span>
                        )}
                      </td>
                      <td className="num">
                        {r.oneM ? (
                          <input
                            type="checkbox"
                            aria-label={`${r.label} 声明支持 1M 上下文`}
                            checked={hasOneM(models[r.key] ?? "")}
                            disabled={!(models[r.key] ?? "").trim()}
                            onChange={() => toggleOneM(r.key)}
                          />
                        ) : (
                          <span className="qb-st-dim">—</span>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <p className="qb-st-note">
              ⛔ 键名照 cc-switch（MIT）抄的，不是自己编的 ——
              编一个写进去客户端根本不认，而界面上看起来一切正常。 旧键
              <code>ANTHROPIC_SMALL_FAST_MODEL</code> 每次保存都会删掉，
              子代理走的是 <code>CLAUDE_CODE_SUBAGENT_MODEL</code>。
            </p>
          </div>
        </>
      ) : desktop ? (
        <DesktopNote />
      ) : (
        <CodexSwitches
          text={text}
          onChange={(t) => {
            setText(t);
            setFollow(false);
          }}
        />
      )}

      <div className="qb-st-cfghd">
        <code className="qb-st-mono">{path || "…"}</code>
        <span style={{ flex: 1 }} />
        {json &&
          (follow ? (
            <span className="qb-st-pill qb-st-pill--ok">✓ 跟随表单</span>
          ) : (
            <button
              className="qb-linkish"
              onClick={() => {
                setFollow(true);
                setText(compose(text, on, models));
              }}
            >
              已手改 · 点此跟随表单
            </button>
          ))}
      </div>

      <textarea
        className="qb-st-editor"
        value={text}
        spellCheck={false}
        rows={10}
        aria-label="配置文件内容"
        onChange={(e) => {
          setText(e.target.value);
          setFollow(false);
        }}
      />

      {problem && (
        <div className="qb-st-nudge">
          <span aria-hidden="true">⚠</span>
          <span>{problem}</span>
        </div>
      )}
      {error && (
        <div className="qb-st-nudge">
          <span aria-hidden="true">⚠</span>
          <span>{error}</span>
        </div>
      )}
      {note && <p className="qb-st-note">{note}</p>}

      <div style={{ display: "flex", gap: 8 }}>
        <Button
          variant="primary"
          disabled={busy || Boolean(problem) || !dirty}
          onClick={() => void save()}
        >
          保存这份配置
        </Button>
        <Button
          variant="ghost"
          disabled={busy || !dirty}
          onClick={() => void load()}
        >
          丢弃改动
        </Button>
      </div>
    </div>
  );
}

/**
 * 桌面端那一档。
 *
 * # 为什么这里没有开关
 *
 * 六个快捷开关和模型映射写的全是 **Claude Code 的环境变量**，桌面端不读。
 * 把它们摆在这里，勾了会往桌面端的配置里塞一堆没人认的键 ——
 * 而界面上看起来一切正常，这是最糟的一类失败。
 *
 * 桌面端自己那套（`inferenceModels` 里的 `supports1m` 之类）先不做：
 * 模型清单由本机路由转发 `GET /v1/models` 从站点取，不填也能跑；
 * 填错了反而会盖掉站点真正提供的那一份。要改的人直接改下面的 JSON。
 */
function DesktopNote() {
  return (
    <p className="qb-st-note">
      桌面端走的是它自己的<b>第三方网关模式</b>（<code>deploymentMode: 3p</code>
      ）—— 这份配置由「启动」那一步自动写好，指向本机路由。
      <br />⛔ 这里没有快捷开关和模型映射：那六个键是{" "}
      <b>Claude Code 的环境变量</b>， 桌面端不读它们。要手动调什么，直接改下面的
      JSON。
      <br />⚠ 这个文件跟 <code>mcpServers</code> 共用，面板只并入不覆盖 ——
      你配的 MCP 不会被动。
    </p>
  );
}

/**
 * Codex 那份是 TOML，六个 Claude 开关不适用。这里给它自己那两项。
 *
 * ⛔ 两项都只认**第一个 `[section]` 之前**的那一段。
 * `[model_providers.qb_relay]` 底下也可能有同名键，改错了地方的症状是
 * 「我明明设了，它没生效」。
 */
function CodexSwitches({
  text,
  onChange,
}: {
  text: string;
  onChange: (t: string) => void;
}) {
  const oneM = tomlInt(text, "model_context_window") === CODEX_ONE_M;
  const effort = tomlStr(text, "model_reasoning_effort") ?? "";

  const toggleOneM = (checked: boolean) => {
    let t = text;
    if (checked) {
      t = setTomlInt(t, "model_context_window", CODEX_ONE_M);
      // ⛔ 压缩上限要一起设。只设上下文窗口的话，客户端会按默认阈值
      // （远小于 1M）提前压缩 —— 1M 那一档等于白开，而开关是亮的。
      if (tomlInt(t, "model_auto_compact_token_limit") === undefined) {
        t = setTomlInt(
          t,
          "model_auto_compact_token_limit",
          CODEX_COMPACT_LIMIT,
        );
      }
    } else {
      t = removeTomlInt(t, "model_context_window");
      t = removeTomlInt(t, "model_auto_compact_token_limit");
    }
    onChange(t);
  };

  return (
    <div className="qb-st-quicks">
      <label
        className="qb-st-check"
        title={`model_context_window = ${CODEX_ONE_M}，并把自动压缩上限设成 ${CODEX_COMPACT_LIMIT}`}
      >
        <input
          type="checkbox"
          checked={oneM}
          onChange={(e) => toggleOneM(e.target.checked)}
        />
        1M 上下文
      </label>
      <label className="qb-st-field" title="model_reasoning_effort">
        思考等级
        <select
          value={effort}
          onChange={(e) =>
            onChange(
              setTomlStr(
                text,
                "model_reasoning_effort",
                e.target.value || null,
              ),
            )
          }
        >
          <option value="">不设（用 Codex 的默认）</option>
          {CODEX_EFFORTS.map((v) => (
            <option key={v} value={v}>
              {v}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}
