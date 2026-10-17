/**
 * GPT / Gemini 两条面板内置桥的设置 —— 软件页 Codex CLI / Gemini CLI 卡最下面「酒馆 · X 桥接设置」
 * 展开之后的正文（外壳是 `BridgeSection.tsx`）。
 *
 * 2026-09-29 使用者「三种不同的厂商按账户页分开」：原来一半在弹窗、一半在扩展中心酒馆页的设置合到这里，
 * 放在账户页酒馆磁贴右上角的弹窗里。同一天第二轮「前端完全重构」：差异收进 `cli/meta.ts`，两份额度卡并成一份
 * （`cli/Quota.tsx`），顶上加一排「跳到」，地址与密钥能一次复制成酒馆那边要的两行。第三轮使用者说
 * 「这个应该在软件页面」，挪到了软件页各自那张 CLI 卡的最下面，组件内部没变。
 *
 * # ⛔ 三条别改回去
 *
 * 1. **「还缺什么」走 `lib/tavernReady.ts` 的纯函数**，不在这里另写一套判定。
 * 2. **额度只手动刷新**：展开时只读「最近一次」，点「刷新」才联网，而且只问这一家。
 * 3. **只存这一家的字段**（`saveTavernFields`）：同一份酒馆配置 SillyTavern 卡与另外两条桥的设置也在改。
 */
import { useCallback, useEffect, useState } from "react";
import { ClipboardList, Copy } from "lucide-react";

import { api } from "../../../lib/api";
import type { GeminiBridgeStatus } from "../../../lib/generated/GeminiBridgeStatus";
import type { GptBridgeStatus } from "../../../lib/generated/GptBridgeStatus";
import { R } from "../../../lib/resources";
import { useResource } from "../../../lib/store";
import { bridgeBlockers } from "../../../lib/tavernReady";
import {
  Button,
  Checkbox,
  Collapsible,
  Field,
  Pill,
  PortField,
  Row,
  TextField,
  useToast,
} from "../../../ui";
import { errorText } from "../card/common";
import { pick, saveTavernFields, type TavernPatch } from "../saveTavernConfig";
import { META, type CliProvider } from "./cli/meta";
import Quota from "./cli/Quota";
import RoleplayTest from "./cli/RoleplayTest";
import SectionNav from "./SectionNav";

type Status = GptBridgeStatus | GeminiBridgeStatus;

export default function CliBridge({ provider }: { provider: CliProvider }) {
  const meta = META[provider];
  const toast = useToast();
  const cfgRes = useResource("tavernConfig", R.tavernConfig);
  const cfg = cfgRes.data ?? null;
  const [draft, setDraft] = useState<TavernPatch>({});
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<Status | null>(null);
  const [busy, setBusy] = useState("");
  const ids = {
    setup: `bridge-${provider}-setup`,
    quota: `bridge-${provider}-quota`,
    test: `bridge-${provider}-test`,
    howto: `bridge-${provider}-howto`,
  };

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(
        provider === "gpt"
          ? await api.tavernGptStatus()
          : await api.tavernGeminiStatus(),
      );
    } catch {
      // 演示模式或后端没起时读不到；保持上一次的值。
    }
  }, [provider]);
  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  const dirty = Object.keys(draft).length > 0;
  const port = pick(draft, cfg, meta.portKey) ?? meta.defaultPort;
  const model = pick(draft, cfg, meta.modelKey) ?? "";
  const prompt = pick(draft, cfg, meta.promptKey) ?? "";
  const cli =
    status === null
      ? null
      : "codex_exe" in status
        ? status.codex_exe
        : status.gemini_cli;
  const url = status?.url || `http://127.0.0.1:${port}/v1`;
  const tavernPathsUnset = !!cfg && (!cfg.sillytavern_root || !cfg.st_launcher);
  const blockers = status
    ? bridgeBlockers({
        tavernPathsUnset,
        cli,
        cliLabel: meta.cliLabel,
        cliWhere: meta.cliWhere,
        slot: status.slot,
        slotLoggedIn: status.slot_logged_in,
        slotWhere: meta.slotWhere,
      })
    : [];

  async function save() {
    setSaving(true);
    try {
      await saveTavernFields(draft);
      setDraft({});
      toast.ok("已保存；下次起桥接时生效");
      void refreshStatus();
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setSaving(false);
    }
  }

  const token = () =>
    provider === "gpt" ? api.tavernGptToken() : api.tavernGeminiToken();

  async function copy(what: "url" | "token" | "both") {
    setBusy(what);
    try {
      const text =
        what === "url"
          ? url
          : what === "token"
            ? await token()
            : `API 地址：${url}\nAPI 密钥：${await token()}`;
      await navigator.clipboard.writeText(text);
      toast.ok(
        what === "url"
          ? "地址已复制，到酒馆的 API 连接里粘贴"
          : what === "token"
            ? "密钥已复制，到酒馆的 API 连接里粘贴"
            : "地址与密钥已一起复制（两行），照着填进酒馆的 API 连接",
      );
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy("");
    }
  }

  return (
    <div className="bridgelist" data-provider={provider}>
      <section
        className={`bridge-provider bridge-provider--${provider}${status?.running ? " is-up" : ""}`}
      >
        <span className="bridge-provider-eyebrow">{meta.eyebrow}</span>
        <div className="bridgelist-head">
          <strong>{meta.name}</strong>
          <Pill tone={status?.running ? "ok" : "default"}>
            {status === null ? "读取中" : status.running ? "在跑" : "没在跑"}
          </Pill>
          {status?.slot && (
            <span className="bridgelist-slot">槽位 {status.slot}</span>
          )}
          {status?.slot && !status.slot_logged_in && (
            <Pill tone="warn">未登录</Pill>
          )}
        </div>
        <p className="bridge-provider-summary">{meta.drives}</p>
        {status?.detail && <p className="bridgelist-sub">{status.detail}</p>}
        {!status?.running && blockers.length > 0 && (
          <ul className="bridgelist-blockers">
            {blockers.map((t) => (
              <li key={t}>{t}</li>
            ))}
          </ul>
        )}
        <SectionNav
          sections={[
            { id: ids.setup, label: "连接与设置" },
            { id: ids.quota, label: "额度" },
            { id: ids.test, label: "链路测试" },
            { id: ids.howto, label: "酒馆那边怎么配" },
          ]}
        />

        <div id={ids.setup}>
          {/* 长路径要能折行：`Row` 的右半边不收缩，这里用会折行的等宽小字。 */}
          <div className="mt-2">
            <Row>
              <span>{meta.cliLabel}</span>
              <span className="break-all font-mono text-xs">
                {cli ?? "没找到"}
              </span>
            </Row>
            <Row>
              <span>密钥文件</span>
              <span className="break-all font-mono text-xs">
                {status?.token_path || "—"}
              </span>
            </Row>
          </div>

          {cfg && (
            <div className="bridgelist-fields">
              <PortField
                label="端口"
                value={port}
                onChange={(v) => setDraft({ ...draft, [meta.portKey]: v })}
              />
              <TextField
                label={meta.modelLabel}
                value={model}
                placeholder={meta.modelHint}
                onChange={(v) => setDraft({ ...draft, [meta.modelKey]: v })}
              />
              {provider === "gpt" && (
                <>
                  <Field
                    label="推理强度"
                    hint="交给 codex exec 的 model_reasoning_effort"
                  >
                    {(p) => (
                      <select
                        {...p}
                        className="input"
                        value={pick(draft, cfg, "gpt_effort") ?? ""}
                        onChange={(e) =>
                          setDraft({ ...draft, gpt_effort: e.target.value })
                        }
                      >
                        <option value="">默认</option>
                        <option value="low">low</option>
                        <option value="medium">medium</option>
                        <option value="high">high</option>
                      </select>
                    )}
                  </Field>
                  <div className="flex items-end pb-1">
                    <Checkbox
                      checked={!!pick(draft, cfg, "gpt_persist_sessions")}
                      onChange={(v) =>
                        setDraft({ ...draft, gpt_persist_sessions: v })
                      }
                    >
                      记入槽位用量（对话会落进槽位的会话目录）
                    </Checkbox>
                  </div>
                </>
              )}
              <Field
                label="独立角色提示词（可选）"
                className="bridgelist-wide"
                hint={`只作用于这一条桥；酒馆角色卡会继续保留 · ${Array.from(prompt).length} 字`}
              >
                {(p) => (
                  <textarea
                    {...p}
                    className="input"
                    rows={3}
                    value={prompt}
                    onChange={(e) =>
                      setDraft({ ...draft, [meta.promptKey]: e.target.value })
                    }
                  />
                )}
              </Field>
              <div className="bridgelist-url">
                <span className="mono">{url}</span>
                <Button
                  size="sm"
                  icon={<Copy size={12} />}
                  loading={busy === "url"}
                  onClick={() => void copy("url")}
                >
                  复制地址
                </Button>
                <Button
                  size="sm"
                  icon={<Copy size={12} />}
                  loading={busy === "token"}
                  onClick={() => void copy("token")}
                  data-testid={`${provider}-copy-token`}
                >
                  复制密钥
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<ClipboardList size={12} />}
                  loading={busy === "both"}
                  onClick={() => void copy("both")}
                >
                  两样一起复制
                </Button>
              </div>
            </div>
          )}
          <div className="mt-3 flex flex-wrap items-center gap-2">
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
            <span className="notice">
              端口、模型、提示词改完<strong>下次起桥接时生效</strong>
              ，正在跑的那条不受影响。
            </span>
            {dirty && <span className="bridgelist-dirty">改动还没保存</span>}
          </div>
        </div>

        <Quota provider={provider} id={ids.quota} />
        <RoleplayTest provider={provider} id={ids.test} />

        <div id={ids.howto}>
          <Collapsible className="mt-3" summary={meta.howTo}>
            <p className="notice">
              SillyTavern → API 连接 → Chat Completion → 来源选 Custom
              (OpenAI-compatible)：地址填上面那条（带
              /v1），密钥点「复制密钥」粘贴，模型选列表里唯一那个。三条桥各存一个连接档，切换在酒馆里切。
            </p>
            {provider === "gpt" ? (
              <p className="notice mt-2">
                它只驱动未修改的官方 Codex CLI 的无交互模式（codex
                exec），用你当前激活的 GPT
                槽位里官方客户端自己完成的登录；不读、不复制、不转发
                auth.json，只监听 127.0.0.1，没有 API Key
                回退。没有增量流式（整段一起出现），不传图、不给工具。是否符合
                OpenAI 的条款由你自行判断，见免责声明第 8 节。
              </p>
            ) : (
              <p className="notice mt-2">
                它驱动的是<strong>官方 Gemini CLI</strong>
                ，不是反重力本体：反重力 Hub 没有公开的无交互模式，令牌在
                Windows 凭据管理器里。用同一个 Google 账户在 Gemini CLI
                里再登一次；额度是否与反重力订阅共享，面板没有核实过。
                只驱动未修改的 CLI 的公开无交互模式，不读、不复制、不转发
                oauth_creds.json，只监听 127.0.0.1，没有 API Key
                回退，没有增量流式，不传图、不给工具。是否符合 Google
                的条款由你自行判断，见免责声明第 8 节。
              </p>
            )}
          </Collapsible>
        </div>
      </section>
    </div>
  );
}
