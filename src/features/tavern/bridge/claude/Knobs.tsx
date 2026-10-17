/**
 * Claude 桥接设置的「桥接设置」一段（七项）：`bridge.py` 自己 `settings.json` 里的七项（跟它自己那个本机网页改的是同一份）。
 *
 * ⛔ 三条（0.32.0 / 2026-09-26 定的，没变）：
 * 1. 改了任何一项，`bridge.py` 都会退掉当前那条 SDK 会话 —— 代价常驻在保存按钮旁边，不放悬停；
 * 2. 「系统提示词模式」只写两边各发什么，不写推荐（选哪个是使用者的事）；
 * 3. 保存被拒就把桥回的原话照搬 —— 它自己的白名单可能比面板这份快照新。
 */
import { useCallback, useEffect, useState, type ReactNode } from "react";

import { api } from "../../../../lib/api";
import type { BridgeSettings } from "../../../../lib/generated/BridgeSettings";
import type { BridgeSettingsView } from "../../../../lib/generated/BridgeSettingsView";
import { Button, Field, useToast } from "../../../../ui";
import { errorText } from "../../card/common";
import { count } from "../format";

/** 多行提示词。`TextField` 只有单行 input，而这两项常常是几百字。 */
function Prompt({
  label,
  hint,
  value,
  rows,
  onChange,
}: {
  label: string;
  hint?: ReactNode;
  value: string;
  rows: number;
  onChange: (v: string) => void;
}) {
  return (
    <Field
      label={label}
      hint={
        <>
          {hint}
          {hint ? " · " : ""}
          {count(Array.from(value).length)} 字
        </>
      }
    >
      {(p) => (
        <textarea
          {...p}
          className="input"
          rows={rows}
          maxLength={262144}
          spellCheck={false}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </Field>
  );
}

function Select({
  label,
  hint,
  value,
  options,
  onChange,
}: {
  label: string;
  hint?: string;
  value: string;
  options: string[];
  onChange: (v: string) => void;
}) {
  return (
    <Field label={label} hint={hint}>
      {(p) => (
        <select
          {...p}
          className="input"
          value={value}
          onChange={(e) => onChange(e.target.value)}
        >
          {options.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      )}
    </Field>
  );
}

export default function Knobs({ id }: { id: string }) {
  const toast = useToast();
  const [view, setView] = useState<BridgeSettingsView | null>(null);
  const [draft, setDraft] = useState<BridgeSettings | null>(null);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    setBusy(true);
    setErr("");
    try {
      setView(await api.tavernBridgeSettings());
      setDraft(null);
    } catch (e) {
      setView(null);
      setErr(errorText(e));
    } finally {
      setBusy(false);
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load]);

  const cur = draft ?? view?.settings ?? null;
  const dirty = !!draft;
  const patch = (p: Partial<BridgeSettings>) =>
    cur && setDraft({ ...cur, ...p });

  async function save() {
    if (!cur) return;
    setBusy(true);
    try {
      await api.tavernBridgeSettingsSave(cur);
      toast.ok(
        "已保存到 bridge.py 自己的 settings.json；当前那条 SDK 会话已退掉",
      );
      await load();
    } catch (e) {
      // ⛔ 照搬桥的原话：它自己的 allowlist 可能比面板这份快照新。
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section
      className="bridge-section"
      id={id}
      data-testid="claude-bridge-knobs"
    >
      <h3>桥接设置（bridge.py）</h3>
      <p className="notice">
        这些存在 <code>bridge.py</code> 自己的 <code>settings.json</code>{" "}
        里，面板只是替你改 ——
        跟它自己那个本机网页改的是同一份。桥没在跑时读不到，先起一次酒馆。
      </p>
      {err && (
        <p role="alert" className="notice notice--danger mt-2">
          {err}
        </p>
      )}
      {cur && view && (
        <>
          <div className="mt-3 grid gap-3">
            <Prompt
              label="附加系统提示词"
              hint="排在酒馆全部系统内容之前（紧接 Claude Code 自己的提示词，角色卡、世界书排在它后面）"
              rows={6}
              value={cur.priority_prompt}
              onChange={(v) => patch({ priority_prompt: v })}
            />
            <Prompt
              label="贴尾提示词"
              hint="每一轮钉在对话最后、紧挨着模型开始写字的位置：放最容易被写飘的几条（字数、格式）。留空不贴"
              rows={3}
              value={cur.tail_prompt}
              onChange={(v) => patch({ tail_prompt: v })}
            />
          </div>
          <div className="bridge-knob-grid mt-3">
            <Select
              label="调用模式"
              hint="cli = 驱动官方 claude；agent_sdk = 官方 Agent SDK"
              value={cur.invocation_mode}
              options={view.invocation_modes}
              onChange={(v) => patch({ invocation_mode: v })}
            />
            <Select
              label="模型"
              value={cur.model}
              options={view.models}
              onChange={(v) => patch({ model: v })}
            />
            <Select
              label="思考深度"
              value={cur.effort}
              options={view.efforts}
              onChange={(v) => patch({ effort: v })}
            />
            <Select
              label="提示缓存"
              hint="缓存有效期"
              value={cur.cache_ttl}
              options={view.cache_ttls}
              onChange={(v) => patch({ cache_ttl: v })}
            />
            {/* 2026-09-26 抓过 claude -p 真正发出去的请求：append 时你的规则排在 Claude Code
                自带的编程助手提示词后面，那段里有「回答要简短」「结尾一两句总结」这类要求。
                所以只写事实、不替人选。 */}
            <Select
              label="系统提示词模式"
              hint="append：Claude Code 自带的编程助手提示词在前（里面有「回答要简短」这类要求），你的规则接在后面；replace：只用你的规则"
              value={cur.system_prompt_mode}
              options={view.prompt_modes}
              onChange={(v) => patch({ system_prompt_mode: v })}
            />
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Button
              variant="primary"
              loading={busy}
              disabled={!dirty || busy}
              onClick={() => void save()}
            >
              保存到 bridge.py
            </Button>
            <Button disabled={!dirty || busy} onClick={() => setDraft(null)}>
              放弃改动
            </Button>
            {dirty && <span className="bridgelist-dirty">改动还没保存</span>}
            {/* 代价常驻，不放悬停提示。 */}
            <span className="notice">
              这七项任何一项改了，<strong>当前那条 SDK 会话会被退掉</strong>
              （下一轮重新开）。固定 200K 上下文，两种模式都不能覆盖 Anthropic
              服务端规则。
            </span>
          </div>
        </>
      )}
    </section>
  );
}
