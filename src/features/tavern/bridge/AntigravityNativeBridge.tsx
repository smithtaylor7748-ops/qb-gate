import { useCallback, useEffect, useState } from "react";
import { Copy, Play, RefreshCw, Square } from "lucide-react";
import { api } from "../../../lib/api";
import type { AntigravityBridgeConfig } from "../../../lib/generated/AntigravityBridgeConfig";
import type { AntigravityBridgeModel } from "../../../lib/generated/AntigravityBridgeModel";
import type { AntigravityBridgeStatus } from "../../../lib/generated/AntigravityBridgeStatus";
import { AFTER } from "../../../lib/resources";
import { invalidate } from "../../../lib/store";
import { Button, Field, Pill, PortField, useToast } from "../../../ui";
import { errorText } from "../card/common";

export default function AntigravityNativeBridge() {
  const toast = useToast();
  const [status, setStatus] = useState<AntigravityBridgeStatus | null>(null);
  const [draft, setDraft] = useState<AntigravityBridgeConfig | null>(null);
  const [models, setModels] = useState<AntigravityBridgeModel[]>([]);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const refresh = useCallback(async () => {
    try {
      const value = await api.tavernAntigravityStatus();
      setStatus(value);
      setDraft(value.config);
      setModels(value.models);
      setError("");
    } catch (e) {
      setError(errorText(e));
    }
  }, []);
  useEffect(() => {
    void refresh();
  }, [refresh]);

  const running = status?.running ?? false;
  const dirty =
    !!draft &&
    !!status &&
    JSON.stringify(draft) !== JSON.stringify(status.config);
  const selected = status?.accounts.find((a) => a.id === draft?.account_id);
  const validModel = models.some((m) => m.id === draft?.model);
  const url = running
    ? status!.url
    : `http://127.0.0.1:${draft?.port ?? 5004}/v1`;

  async function act(action: string, task: () => Promise<void>) {
    setBusy(action);
    setError("");
    try {
      await task();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy("");
    }
  }

  async function readModels() {
    if (!draft?.account_id) return;
    await act("models", async () => {
      setModels([]);
      const next = await api.tavernAntigravityModels(draft.account_id);
      setModels(next);
      if (!next.some((m) => m.id === draft.model))
        setDraft({ ...draft, model: "" });
      toast.ok("已读取这个 IDE 账户在官方生产端点返回的文本模型");
    });
  }

  async function save() {
    if (!draft) return;
    await act("save", async () => {
      await api.tavernAntigravitySave(draft);
      await refresh();
      toast.ok("原生桥配置已保存，尚未启动");
    });
  }

  async function start() {
    await act("start", async () => {
      const next = await api.tavernAntigravityStart();
      setStatus(next);
      setDraft(next.config);
      setModels(next.models);
      invalidate(...AFTER.tavern);
      toast.ok("原生桥与酒馆已启动；请在酒馆使用下方原生连接");
    });
  }

  async function stop() {
    await act("stop", async () => {
      await api.tavernAntigravityStop();
      await refresh();
      invalidate(...AFTER.tavern);
      toast.ok("原生桥已停止");
    });
  }

  async function copy() {
    await act("copy", async () => {
      const token = await api.tavernAntigravityToken();
      await navigator.clipboard.writeText(
        `SillyTavern · Chat Completion · Custom (OpenAI-compatible)\nAPI 地址：${url}\nAPI 密钥：${token}\n模型：${draft?.model ?? ""}`,
      );
      toast.ok("原生桥连接已复制；请在酒馆另存一个连接档");
    });
  }

  return (
    <section
      className={`bridge-provider bridge-provider--gemini mt-4${running ? " is-up" : ""}`}
      data-testid="antigravity-native-bridge"
    >
      <span className="bridge-provider-eyebrow">Google · Antigravity IDE</span>
      <div className="bridgelist-head">
        <strong>反重力原生桥接</strong>
        <Pill tone={running ? "ok" : "default"}>
          {running ? "在跑" : "未启动"}
        </Pill>
      </div>
      <p className="bridge-provider-summary">
        用选定的反重力 IDE 登录直接连接官方模型，支持逐段流式回复。
        模型清单只在点击读取或启动时联网查询当前所选账户。
      </p>
      <p className="notice">
        独立保存一个酒馆连接档。只支持文本角色卡和对话，不支持图片或工具调用。
        Hub 与 Gemini CLI 登录暂不接入；不会改用其他账户或中转站。
      </p>
      {status?.detail && <p className="bridgelist-sub">{status.detail}</p>}
      {error && (
        <p className="notice mt-2" role="alert">
          {error}
        </p>
      )}
      {draft && status && (
        <>
          <div className="bridgelist-fields">
            <Field
              label="原生桥使用的 IDE 账户"
              hint="始终锁定此账户；账户页切换激活项不会改动这条桥。"
            >
              {(p) => (
                <select
                  {...p}
                  className="input"
                  value={draft.account_id}
                  disabled={running || !!busy}
                  onChange={(e) => {
                    setDraft({
                      ...draft,
                      account_id: e.target.value,
                      model: "",
                    });
                    setModels([]);
                  }}
                >
                  <option value="">请选择 IDE 账户</option>
                  {status.accounts.map((a) => (
                    <option key={a.id} value={a.id} disabled={!a.logged_in}>
                      {a.label}
                      {a.logged_in ? "" : " · IDE 未登录"}
                    </option>
                  ))}
                </select>
              )}
            </Field>
            <Field
              label="官方返回的文本模型"
              hint="清单为空时先点「读取此账户模型」；不填猜测的模型名。"
            >
              {(p) => (
                <select
                  {...p}
                  className="input"
                  value={draft.model}
                  disabled={running || !!busy || !models.length}
                  onChange={(e) =>
                    setDraft({ ...draft, model: e.target.value })
                  }
                >
                  <option value="">请选择模型</option>
                  {draft.model && !validModel && (
                    <option value={draft.model}>
                      {draft.model} · 等待读取验证
                    </option>
                  )}
                  {models.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.name} · {m.id}
                    </option>
                  ))}
                </select>
              )}
            </Field>
            <PortField
              label="原生桥端口"
              value={draft.port}
              onChange={(port) => {
                if (!running && !busy) setDraft({ ...draft, port });
              }}
              disabled={running || !!busy}
            />
            <div className="flex items-end pb-1">
              <Button
                icon={<RefreshCw size={13} />}
                loading={busy === "models"}
                disabled={running || !!busy || !selected?.logged_in}
                onClick={() => void readModels()}
              >
                读取此账户模型
              </Button>
            </div>
            <Field
              label="原生桥独立角色提示词（可选）"
              className="bridgelist-wide"
              hint="与酒馆角色卡共同生效，只用于这条原生桥。"
            >
              {(p) => (
                <textarea
                  {...p}
                  className="input"
                  rows={3}
                  value={draft.system_prompt}
                  disabled={running || !!busy}
                  onChange={(e) =>
                    setDraft({ ...draft, system_prompt: e.target.value })
                  }
                />
              )}
            </Field>
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Button
              disabled={running || !!busy || !dirty}
              loading={busy === "save"}
              onClick={() => void save()}
            >
              保存原生桥配置
            </Button>
            <Button
              variant="primary"
              icon={<Play size={13} />}
              loading={busy === "start"}
              disabled={
                running ||
                !!busy ||
                dirty ||
                !selected?.logged_in ||
                !draft.model
              }
              onClick={() => void start()}
            >
              启动原生桥与酒馆
            </Button>
            <Button
              icon={<Square size={13} />}
              loading={busy === "stop"}
              disabled={!running || !!busy}
              onClick={() => void stop()}
            >
              停止原生桥
            </Button>
            <Button
              variant="ghost"
              disabled={!!busy}
              onClick={() => void refresh()}
            >
              刷新本机状态
            </Button>
            {dirty && <span className="bridgelist-dirty">配置尚未保存</span>}
          </div>
          <div className="bridgelist-url mt-3">
            <span className="mono">{url}</span>
            <Button
              size="sm"
              icon={<Copy size={12} />}
              loading={busy === "copy"}
              disabled={!!busy || dirty || !draft.model}
              onClick={() => void copy()}
            >
              复制原生连接
            </Button>
          </div>
          <p className="notice mt-2">
            酒馆：Chat Completion → Custom
            (OpenAI-compatible)，填入此地址、本机密钥和模型，开启流式。
            更换账户或模型前先停止原生桥；额度不足会返回错误，不会自动换号。
          </p>
        </>
      )}
    </section>
  );
}
