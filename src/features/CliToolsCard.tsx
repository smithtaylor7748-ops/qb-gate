/**
 * 软件页「其他命令行工具」（2026-10-27，中转站重构阶段 7 · X3，照 cc-switch 的工具管理）：
 * OpenCode、Grok Build、OpenClaw、Hermes、Pi、MiniMax Code。
 *
 * - 打开页面只读文件（`cli_tools_status`）：装了几份、谁装的、版本从包的 `package.json` 读 —— 不运行它们、不联网。
 * - 「检查新版本」才问 npm / GitHub / PyPI，顺带看本机 Node 够不够新版用（不够就不给升级按钮）。
 * - 升级交给装它的那个包管理器（写回同一处）；认不出就只说要你自己做什么。
 * - ⛔ 官方 PowerShell 安装脚本：先下载、在这里给你看全文和 SHA-256，你勾了「看过了」才跑那一份。
 *
 * 这几个不归 IP 门禁管，也不进托管目录（Claude Code / Codex / Gemini CLI 在上面各自的卡里）。
 */
import { useState } from "react";
import { Download, FileCode, RefreshCw, Trash2 } from "lucide-react";
import {
  api,
  type CliInstall,
  type CliScriptPurpose,
  type CliTool,
  type CliToolLatest,
  type CliToolScript,
  type CliToolSource,
  type CliToolStatus,
} from "../lib/api";
import { R } from "../lib/resources";
import { invalidate, useResource } from "../lib/store";
import { endTask, resetTask, useTask } from "../lib/tasks";
import { TaskBlock } from "../pages/softwareParts";
import {
  Button,
  Card,
  Checkbox,
  CodeBlock,
  Collapsible,
  ConfirmDialog,
  Modal,
  Pill,
  Row,
  useToast,
} from "../ui";

const SOURCE_LABEL: Record<CliToolSource, string> = {
  npm: "npm",
  nvm: "nvm-windows",
  pnpm: "pnpm",
  volta: "Volta",
  yarn: "Yarn",
  bun: "Bun",
  scoop: "Scoop",
  mise: "mise",
  installer: "官方安装器",
  python: "Python",
  unknown: "来源不明",
};

/** 面板替你卸得了的那几类（包管理器装的）。别的装法点了也只会得到一句「怎么做」。 */
const REMOVABLE: CliToolSource[] = ["npm", "nvm", "pnpm", "volta"];

function errMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export default function CliToolsCard() {
  const status = useResource("cliTools", R.cliTools);
  const task = useTask("cli-tool");
  const toast = useToast();
  const [busy, setBusy] = useState("");
  const [latest, setLatest] = useState<Partial<Record<CliTool, CliToolLatest>>>(
    {},
  );
  const [script, setScript] = useState<CliToolScript | null>(null);
  const [agreed, setAgreed] = useState(false);
  const [removing, setRemoving] = useState<{
    tool: CliToolStatus;
    install: CliInstall;
  } | null>(null);

  /** 一次会动盘的操作：进度条、提示、刷新。一次只做一件（后端也只让一件跑）。 */
  async function run(key: string, fn: () => Promise<string>) {
    setBusy(key);
    resetTask("cli-tool");
    try {
      const msg = await fn();
      endTask("cli-tool");
      toast.ok(msg);
    } catch (e) {
      const msg = errMsg(e);
      endTask("cli-tool", msg);
      toast.error(msg);
    } finally {
      setBusy("");
      invalidate("cliTools");
    }
  }

  async function check(tool: CliTool) {
    setBusy(`check:${tool}`);
    try {
      const l = await api.cliToolsLatest(tool);
      setLatest((m) => ({ ...m, [tool]: l }));
    } catch (e) {
      toast.error(errMsg(e));
    } finally {
      setBusy("");
    }
  }

  async function fetchScript(tool: CliTool, purpose: CliScriptPurpose) {
    setBusy(`fetch:${tool}`);
    try {
      setAgreed(false);
      setScript(await api.cliToolsScriptFetch(tool, purpose));
    } catch (e) {
      toast.error(errMsg(e));
    } finally {
      setBusy("");
    }
  }

  const report = status.data;
  const label = (tool: CliTool) =>
    report?.tools.find((t) => t.tool === tool)?.label ?? tool;

  return (
    <Card
      title="其他命令行工具"
      className="mb-3"
      actions={
        <Button
          size="sm"
          icon={<RefreshCw size={13} />}
          loading={status.loading}
          disabled={!!busy}
          onClick={() => void status.refresh()}
        >
          重新看一遍
        </Button>
      }
    >
      <p className="notice mb-2">
        OpenCode、Grok Build、OpenClaw、Hermes、Pi、MiniMax Code。这里只看文件：
        装了几份、谁装的、版本从包里的 package.json
        读，不运行它们、不联网；「检查新版本」才问 npm / GitHub /
        PyPI。升级交给装它的那个包管理器，写回同一处。这几个不归 IP 门禁管。
      </p>
      {status.error && <p className="notice notice--danger">{status.error}</p>}
      {report && !report.npm && (
        <p className="notice notice--warn mb-2">
          没找到 npm（Node.js 没装，或者不在 PATH 上）：除了 Hermes，这几个都是
          npm 包。先装 Node.js LTS（https://nodejs.org/），Grok Build 和 MiniMax
          Code 也可以用官方脚本装。
        </p>
      )}
      {report?.tools.map((t) => (
        <ToolRow
          key={t.tool}
          t={t}
          latest={latest[t.tool]}
          busy={busy}
          onCheck={() => void check(t.tool)}
          onInstall={() =>
            void run(`install:${t.tool}`, () => api.cliToolsInstall(t.tool))
          }
          onUpdate={() =>
            void run(`update:${t.tool}`, () => api.cliToolsUpdate(t.tool))
          }
          onScript={(purpose) => void fetchScript(t.tool, purpose)}
          onRemove={(install) => setRemoving({ tool: t, install })}
        />
      ))}
      <TaskBlock task={task} label="进度" />

      <Modal
        open={!!script}
        size="huge"
        onClose={() => setScript(null)}
        title={
          <>
            <FileCode size={18} />
            先看一眼：{script ? label(script.tool) : ""} 的官方安装脚本
          </>
        }
        footer={
          <>
            <Button onClick={() => setScript(null)}>不跑了</Button>
            <Button
              variant="primary"
              disabled={!agreed || !!busy}
              onClick={() => {
                const s = script;
                setScript(null);
                if (s)
                  void run(`script:${s.tool}`, () =>
                    api.cliToolsScriptRun(s.id, s.sha256),
                  );
              }}
            >
              我看过了，运行
            </Button>
          </>
        }
      >
        {script && (
          <>
            <p className="notice break-all">
              地址 <code>{script.url}</code>
              {script.final_url !== script.url && (
                <>
                  （跳转到 <code>{script.final_url}</code>）
                </>
              )}{" "}
              · {script.size} 字节 · SHA-256 <code>{script.sha256}</code>
            </p>
            {script.env.length > 0 && (
              <p className="notice mt-1 break-all">
                会带上环境变量：
                {script.env.map(([k, v]) => (
                  <code key={k} className="ml-1">
                    {k}={v}
                  </code>
                ))}
              </p>
            )}
            <p className="notice notice--warn mt-2">
              这是官方给的 PowerShell
              脚本，面板不审查它的内容：它会以你的身份在这台电脑上运行（-ExecutionPolicy
              Bypass 只对这一次生效，不改系统设置）。跑的就是下面这一份 ——
              哈希对不上（下载之后被改过）面板会拒绝。看不懂就别跑。
            </p>
            <div className="mt-2">
              <CodeBlock
                text={script.text}
                caption="install.ps1"
                maxHeight={360}
              />
            </div>
            {script.truncated && (
              <p className="notice mt-1">
                太长了，只显示前 256 KiB；跑的是整份。
              </p>
            )}
            <div className="mt-2">
              <Checkbox checked={agreed} onChange={setAgreed}>
                我看过了，确认这是从上面那个地址下来的官方脚本
              </Checkbox>
            </div>
          </>
        )}
      </Modal>

      <ConfirmDialog
        open={!!removing}
        danger
        title={`卸掉这一份 ${removing?.tool.label ?? ""}？`}
        confirmLabel="卸载"
        loading={busy.startsWith("remove:")}
        onCancel={() => setRemoving(null)}
        onConfirm={() => {
          const r = removing;
          setRemoving(null);
          if (r)
            void run(`remove:${r.tool.tool}`, () =>
              api.cliToolsUninstall(r.tool.tool, r.install.path),
            );
        }}
      >
        <p className="break-all">
          用装它的那个包管理器（
          {SOURCE_LABEL[removing?.install.source ?? "npm"]}
          ）卸掉 <code>{removing?.install.path}</code>。它自己的配置和数据（比如
          <code>~\.openclaw</code>）不动。
        </p>
      </ConfirmDialog>
    </Card>
  );
}

function ToolRow({
  t,
  latest,
  busy,
  onCheck,
  onInstall,
  onUpdate,
  onScript,
  onRemove,
}: {
  t: CliToolStatus;
  latest: CliToolLatest | undefined;
  busy: string;
  onCheck: () => void;
  onInstall: () => void;
  onUpdate: () => void;
  onScript: (purpose: CliScriptPurpose) => void;
  onRemove: (install: CliInstall) => void;
}) {
  const def = t.installs[0];
  const installed = t.installs.length > 0;
  const nodeBlocked = latest?.node_ok === false;
  const actions = (
    <span className="flex flex-wrap justify-end gap-1">
      {!installed && (
        <Button
          size="sm"
          icon={<Download size={12} />}
          disabled={!!busy || t.install.kind !== "run"}
          loading={busy === `install:${t.tool}`}
          onClick={onInstall}
        >
          用 npm 装
        </Button>
      )}
      {!installed && t.script_url && (
        <Button
          size="sm"
          icon={<FileCode size={12} />}
          disabled={!!busy}
          loading={busy === `fetch:${t.tool}`}
          onClick={() => onScript("install")}
        >
          用官方脚本装…
        </Button>
      )}
      {installed && (
        <Button
          size="sm"
          disabled={!!busy}
          loading={busy === `check:${t.tool}`}
          onClick={onCheck}
        >
          检查新版本
        </Button>
      )}
      {installed && t.update?.kind === "run" && (
        <Button
          size="sm"
          variant={latest?.outdated ? "primary" : "default"}
          disabled={!!busy || nodeBlocked}
          loading={busy === `update:${t.tool}`}
          onClick={onUpdate}
        >
          升级
        </Button>
      )}
      {installed && t.update?.kind === "script" && (
        <Button
          size="sm"
          icon={<FileCode size={12} />}
          disabled={!!busy}
          loading={busy === `fetch:${t.tool}`}
          onClick={() => onScript("update")}
        >
          用官方脚本升级…
        </Button>
      )}
    </span>
  );
  return (
    <Row side={actions}>
      <span className="flex flex-wrap items-baseline gap-2">
        <strong>{t.label}</strong>
        <code className="notice">{t.command}</code>
        {def ? (
          <Pill tone="ok">{def.version ?? "版本读不出"}</Pill>
        ) : (
          <Pill>未安装</Pill>
        )}
        {def && <Pill>{SOURCE_LABEL[def.source]}</Pill>}
        {latest?.latest && (
          <Pill tone={latest.outdated ? "accent" : "default"}>
            {latest.outdated
              ? `可升级 → ${latest.latest}`
              : `最新 ${latest.latest}（${latest.from ?? ""}）`}
          </Pill>
        )}
      </span>
      {!installed && t.install.kind === "manual" && !t.script_url && (
        <span className="notice block">{t.install.shown}</span>
      )}
      {nodeBlocked && (
        <span className="notice notice--warn block">
          新版要求 Node {latest?.node_required}，本机是 {latest?.node_current}
          ：先升级 Node.js，否则装上也跑不起来。
        </span>
      )}
      {!!latest?.problems.length && !latest.latest && (
        <span className="notice notice--warn block break-words">
          没问到最新版本：{latest.problems.join("；")}
        </span>
      )}
      {t.conflict && (
        <span className="notice notice--warn block">
          装了 {t.installs.length}{" "}
          份，版本对不上或者读不出。升级只动命令行用的那一份（排在最前面的），别的留着。
        </span>
      )}
      {installed && t.update?.kind === "manual" && (
        <span className="notice block break-words">{t.update.shown}</span>
      )}
      {installed && (
        <Collapsible className="mt-1" summary={`本机 ${t.installs.length} 份`}>
          {t.installs.map((i) => (
            <Row
              key={i.path}
              side={
                REMOVABLE.includes(i.source) ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label={`卸掉 ${i.path}`}
                    icon={<Trash2 size={12} />}
                    disabled={!!busy}
                    onClick={() => onRemove(i)}
                  />
                ) : undefined
              }
            >
              <span className="notice block break-all font-mono">{i.path}</span>
              <span className="notice block">
                {SOURCE_LABEL[i.source]} · {i.version ?? "版本读不出"}
                {i.default ? " · 命令行用的是这一份" : ""}
              </span>
            </Row>
          ))}
        </Collapsible>
      )}
    </Row>
  );
}
