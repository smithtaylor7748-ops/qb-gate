/**
 * Claude 桥接设置的「连接」一段：面板这边管的两项 —— bridge.py 所在的目录（面板不分发它）与起它时交给它的端口。
 *
 * ⛔ 只存这两个字段（`saveTavernFields`）：同一份酒馆配置 SillyTavern 卡与 GPT / Gemini 两条桥的设置也在改。
 */
import { useState } from "react";
import { Copy, Radar } from "lucide-react";

import { api, type TavernCandidate } from "../../../../lib/api";
import { R } from "../../../../lib/resources";
import { useResource } from "../../../../lib/store";
import { Button, PathField, PortField, useToast } from "../../../../ui";
import { errorText } from "../../card/common";
import {
  pick,
  saveTavernFields,
  type TavernPatch,
} from "../../saveTavernConfig";

/** Claude 那条桥的依赖（`PluginStatus.checks` 里这几项）。 */
const CLAUDE_CHECKS = ["桥接项目", "Python 3", "官方 Claude Code"];

export default function Connection({ id }: { id: string }) {
  const toast = useToast();
  const cfgRes = useResource("tavernConfig", R.tavernConfig);
  const plugins = useResource("plugins", R.plugins);
  const cfg = cfgRes.data ?? null;
  const [draft, setDraft] = useState<TavernPatch>({});
  const [saving, setSaving] = useState(false);
  const [locating, setLocating] = useState(false);
  const [found, setFound] = useState<TavernCandidate[] | null>(null);
  const dirty = Object.keys(draft).length > 0;
  const root = pick(draft, cfg, "bridge_root") ?? "";
  const port = pick(draft, cfg, "bridge_port") ?? 5001;
  const url = `http://127.0.0.1:${port}/v1`;
  const tavern = plugins.data?.find((p) => p.id === "sillytavern");
  const missing =
    tavern?.checks.filter((c) => CLAUDE_CHECKS.includes(c.label) && !c.ok) ??
    [];

  async function save() {
    setSaving(true);
    try {
      await saveTavernFields(draft);
      setDraft({});
      toast.ok("已保存；下次起酒馆时生效");
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setSaving(false);
    }
  }

  async function locate() {
    setLocating(true);
    try {
      const s = await api.tavernLocate(false);
      setFound(s.bridge.slice(0, 3));
      if (!s.bridge.length)
        toast.error("本机没找到 bridge.py。装在别处的话请自己填目录。");
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setLocating(false);
    }
  }

  async function copyUrl() {
    try {
      await navigator.clipboard.writeText(url);
      toast.ok("地址已复制，到酒馆的 API 连接里粘贴");
    } catch (e) {
      toast.error(errorText(e));
    }
  }

  return (
    <section className="bridge-section" id={id}>
      <h3>连接</h3>
      <p className="notice">
        这两项存在面板这边：桥接项目是 bridge.py
        所在的目录（面板不分发它），端口是面板起它时交给它的。
      </p>
      {missing.length > 0 && (
        <ul className="bridgelist-blockers">
          {missing.map((c) => (
            <li key={c.label}>
              {c.label}：{c.detail}
            </li>
          ))}
        </ul>
      )}
      {cfg && (
        <div className="bridgelist-fields mt-2">
          <div className="bridgelist-wide">
            <PathField
              label="桥接项目目录（bridge.py 所在）"
              value={root}
              onChange={(v) => setDraft({ ...draft, bridge_root: v })}
            />
          </div>
          <PortField
            label="端口"
            value={port}
            onChange={(v) => setDraft({ ...draft, bridge_port: v })}
          />
          <div className="flex items-end pb-1">
            <Button
              size="sm"
              icon={<Radar size={12} />}
              loading={locating}
              onClick={() => void locate()}
            >
              自动定位
            </Button>
          </div>
          {found && found.length > 0 && (
            <div className="bridgelist-wide grid gap-1">
              {found.map((c) => (
                <div
                  key={c.path}
                  className={`tv-cand${root === c.path ? " tv-cand--picked" : ""}`}
                >
                  <span className="tv-cand-main">
                    <span className="tv-cand-path">{c.path}</span>
                    <span className="tv-cand-note">{c.note}</span>
                  </span>
                  <Button
                    size="sm"
                    disabled={root === c.path}
                    onClick={() => setDraft({ ...draft, bridge_root: c.path })}
                  >
                    {root === c.path ? "已选" : "采用"}
                  </Button>
                </div>
              ))}
            </div>
          )}
          <div className="bridgelist-url">
            <span className="mono">{url}</span>
            <Button
              size="sm"
              icon={<Copy size={12} />}
              onClick={() => void copyUrl()}
            >
              复制地址
            </Button>
          </div>
        </div>
      )}
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <Button
          variant="primary"
          loading={saving}
          disabled={!dirty || saving}
          onClick={() => void save()}
        >
          保存
        </Button>
        <Button disabled={!dirty || saving} onClick={() => setDraft({})}>
          放弃改动
        </Button>
        {dirty && <span className="bridgelist-dirty">改动还没保存</span>}
      </div>
    </section>
  );
}
