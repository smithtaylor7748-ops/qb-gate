/**
 * 反重力 · 汉化与审批（插件 antigravity-ui）的详情面板。
 *
 * 引擎（Rust 侧 `plugins::antigravity_ui`）通过反重力 Hub 自己开着的 Chrome DevTools
 * 协议往它的页面里注入 EasyAntigravity 的脚本：汉化字典替换文本、审批卡自动点选、
 * 高危命令拦截。不起 Hub、不改它任何文件、不碰凭据。
 *
 * 这里放：附加 / 停止、四个开关 + 选项档、高危规则表（启停单条 / 整组、改正则、恢复出厂）、
 * 引擎日志。账户页那一栏只放最常用的三个开关和计数。
 */
import { useCallback, useEffect, useState } from "react";
import { Play, RotateCcw, Save, Square } from "lucide-react";
import {
  api,
  type DangerRules,
  type UiConfig,
  type UiStatus,
} from "../lib/api";
import { invalidate } from "../lib/store";
import {
  Button,
  Card,
  Checkbox,
  Collapsible,
  Field,
  Pill,
  Row,
  useToast,
} from "../ui";

const OPTION_LABEL: Record<number, string> = {
  1: "1 · 仅允许本次",
  2: "2 · 本次对话中始终允许",
  3: "3 · 本项目中始终允许",
  4: "4 · 全局始终允许（EasyAG 默认）",
};

export default function AntigravityPanel() {
  const toast = useToast();
  const [status, setStatus] = useState<UiStatus | null>(null);
  const [rules, setRules] = useState<DangerRules | null>(null);
  const [rulesDirty, setRulesDirty] = useState(false);
  const [busy, setBusy] = useState("");

  const refresh = useCallback(async () => {
    try {
      setStatus(await api.antigravityUiStatus());
    } catch {
      // 演示 / 后端没起：保持上一次的值。
    }
  }, []);
  const loadRules = useCallback(async () => {
    try {
      setRules(await api.antigravityUiRules());
      setRulesDirty(false);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
  }, [toast]);
  useEffect(() => {
    void refresh();
    void loadRules();
    const timer = window.setInterval(() => void refresh(), 4000);
    return () => window.clearInterval(timer);
  }, [refresh, loadRules]);

  async function act(key: string, fn: () => Promise<unknown>, ok: string) {
    setBusy(key);
    try {
      await fn();
      toast.ok(ok);
      await refresh();
      invalidate("plugins");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  }
  async function saveConfig(patch: Partial<UiConfig>) {
    if (!status) return;
    const next = { ...status.config, ...patch };
    setStatus({ ...status, config: next });
    try {
      await api.antigravityUiConfigSave(next);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
      void refresh();
    }
  }
  const cfg = status?.config;

  return (
    <>
      <Card
        title="引擎"
        as="h3"
        className="mt-3"
        actions={
          <div className="flex gap-2">
            {status?.running ? (
              <Button
                size="sm"
                variant="danger"
                icon={<Square size={12} />}
                loading={busy === "stop"}
                disabled={!!busy}
                onClick={() =>
                  void act("stop", api.antigravityUiStop, "引擎已停止")
                }
              >
                停止
              </Button>
            ) : (
              <Button
                size="sm"
                variant="primary"
                icon={<Play size={12} />}
                loading={busy === "start"}
                disabled={!!busy || !status || !status.port_file_present}
                onClick={() =>
                  void act(
                    "start",
                    api.antigravityUiStart,
                    "已附加到反重力 Hub",
                  )
                }
              >
                附加到正在跑的 Hub
              </Button>
            )}
          </div>
        }
      >
        <p className="notice">{status?.detail ?? "读取中…"}</p>
        <div className="mt-2">
          <Row>
            <span>调试端口 / 页面 / 已连</span>
            <span className="font-mono text-xs">
              {status?.running
                ? `${status.port} / ${status.targets} / ${status.sockets}`
                : "—"}
            </span>
          </Row>
          <Row>
            <span>注入 / 放行 / 拦截</span>
            <span className="font-mono text-xs">
              {status
                ? `${status.inject_count} / ${status.approve_count} / ${status.block_count}`
                : "—"}
            </span>
          </Row>
          {status?.cdp_error && (
            <Row>
              <span>CDP</span>
              <span className="break-all text-xs text-[var(--warn)]">
                {status.cdp_error}
              </span>
            </Row>
          )}
          <Row>
            <span>字典</span>
            <span className="text-xs">
              {status?.dict_entries ?? 0} 条（内置 ui_v2 +
              common；可在状态目录放 dict.override.json 追加）
            </span>
          </Row>
        </div>
        {cfg && (
          <div className="mt-3 grid gap-2 sm:grid-cols-2">
            <Checkbox
              checked={cfg.enable_i18n}
              onChange={(v) => void saveConfig({ enable_i18n: v })}
            >
              界面汉化（字典替换文本、placeholder、title）
            </Checkbox>
            <Checkbox
              checked={cfg.auto_accept}
              onChange={(v) => void saveConfig({ auto_accept: v })}
            >
              自动审批（审批卡自动点选项与提交）
            </Checkbox>
            <Checkbox
              checked={cfg.block_dangerous}
              disabled={!cfg.auto_accept}
              onChange={(v) => void saveConfig({ block_dangerous: v })}
            >
              高危拦截（命中规则的命令不放行，记入日志）
            </Checkbox>
            <Checkbox
              checked={cfg.attach_on_launch}
              onChange={(v) => void saveConfig({ attach_on_launch: v })}
            >
              从面板起 Hub 后自动附加
            </Checkbox>
            <Field
              label="审批卡上选哪一项"
              hint="EasyAG 的 preferOption；没有选项组的卡直接点提交"
            >
              {(p) => (
                <select
                  {...p}
                  className="input"
                  value={cfg.prefer_option}
                  onChange={(e) =>
                    void saveConfig({ prefer_option: Number(e.target.value) })
                  }
                >
                  {[1, 2, 3, 4].map((n) => (
                    <option key={n} value={n}>
                      {OPTION_LABEL[n]}
                    </option>
                  ))}
                </select>
              )}
            </Field>
          </div>
        )}
        <p className="notice mt-3">
          改开关立即生效（引擎每 2 秒把最新设置带进页面）。脚本、字典、规则取自
          EasyAntigravity（MIT，Astwarp）；本面板不做它的「免 TUN 代理」——
          那要往 Google 的安装目录丢一个来源不明的 DLL，见 ATTRIBUTION.md。
        </p>
      </Card>

      <Card
        title="高危规则"
        as="h3"
        className="mt-3"
        actions={
          <div className="flex gap-2">
            <Button
              size="sm"
              icon={<RotateCcw size={12} />}
              disabled={!!busy}
              onClick={() =>
                void act(
                  "reset",
                  async () => {
                    setRules(await api.antigravityUiRulesReset());
                    setRulesDirty(false);
                  },
                  "已恢复出厂规则",
                )
              }
            >
              恢复出厂
            </Button>
            <Button
              size="sm"
              variant="primary"
              icon={<Save size={12} />}
              loading={busy === "rules"}
              disabled={!!busy || !rules || !rulesDirty}
              onClick={() =>
                void act(
                  "rules",
                  async () => {
                    await api.antigravityUiRulesSave(rules!);
                    setRulesDirty(false);
                  },
                  "规则已保存，引擎下一次心跳生效",
                )
              }
            >
              保存
            </Button>
          </div>
        }
      >
        {rules && (
          <>
            <Checkbox
              checked={rules.enabled}
              onChange={(v) => {
                setRules({ ...rules, enabled: v });
                setRulesDirty(true);
              }}
            >
              整组启用（关掉 = 一条都不拦）
            </Checkbox>
            <div className="mt-2 flex flex-col gap-2">
              {rules.rules.map((r, i) => (
                <div key={r.id || i} className="ag-rule" data-testid="ag-rule">
                  <Checkbox
                    checked={r.enabled}
                    onChange={(v) => {
                      const next = rules.rules.slice();
                      next[i] = { ...r, enabled: v };
                      setRules({ ...rules, rules: next });
                      setRulesDirty(true);
                    }}
                  >
                    <strong>{r.name || r.id}</strong>
                    {r.description ? (
                      <span className="notice"> · {r.description}</span>
                    ) : null}
                  </Checkbox>
                  <input
                    className="input font-mono text-xs"
                    aria-label={`规则 ${r.name || r.id} 的正则`}
                    value={r.pattern}
                    onChange={(e) => {
                      const next = rules.rules.slice();
                      next[i] = { ...r, pattern: e.target.value };
                      setRules({ ...rules, rules: next });
                      setRulesDirty(true);
                    }}
                  />
                </div>
              ))}
            </div>
            <p className="notice mt-2 break-all">
              文件：{status?.rules_path ?? "—"}（JSON，正则按 JavaScript
              语法，默认不区分大小写）
            </p>
          </>
        )}
      </Card>

      <Collapsible
        className="mt-3"
        summary={
          <span className="flex items-center gap-1.5">
            引擎日志
            <Pill tone={status?.log.length ? "accent" : "default"}>
              {status?.log.length ?? 0}
            </Pill>
          </span>
        }
      >
        {status?.log.length ? (
          <div className="ag-log">
            {status.log
              .slice()
              .reverse()
              .map((l, i) => (
                <div key={i} className="ag-log-line">
                  <span className="font-mono text-xs">{l.at}</span>
                  <Pill
                    tone={
                      l.category === "SECURITY ALERT"
                        ? "danger"
                        : l.category === "AUTO-ACCEPT"
                          ? "ok"
                          : "default"
                    }
                  >
                    {l.category}
                  </Pill>
                  <span className="break-all text-xs">{l.message}</span>
                </div>
              ))}
          </div>
        ) : (
          <p className="notice">
            还没有日志。附加到 Hub 之后，放行与拦截都会记在这里。
          </p>
        )}
      </Collapsible>
    </>
  );
}
