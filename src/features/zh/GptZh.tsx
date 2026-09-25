/**
 * GPT（Codex 桌面端）界面语言：一键设成中文（2026-09-25，使用者定的）。
 *
 * 写的是 GPT **自己的设置项** —— `CODEX_HOME\config.toml` 里 `[desktop]` 表的
 * `localeOverride = "zh-CN"`，跟在它「设置 → General → Language」里选中文写下的是同一行。
 * 改哪几份是使用者选的：全部 GPT 槽位 + 默认那份 + 以后新建的槽位。弹窗里逐个列出来。
 *
 * 面板起的 GPT 开着时要先关（开着改会被它写回去），确认框里写明任务会停；
 * 改完按当前槽位重开。别处起的那份面板不关（2026-09-23 那条），它开着时默认那一份这次不改。
 */
import { useCallback, useEffect, useState } from "react";
import { Languages, RotateCcw } from "lucide-react";

import { codexApi } from "../../lib/codexAccounts";
import type { CodexLocaleOutcome } from "../../lib/generated/CodexLocaleOutcome";
import type { CodexLocaleStatus } from "../../lib/generated/CodexLocaleStatus";
import { invalidate } from "../../lib/store";
import {
  Button,
  ConfirmDialog,
  ExternalLink,
  LogView,
  Pill,
  useToast,
} from "../../ui";

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** 所有份都设成了中文。一份都没有时不算。 */
export function gptZhOn(s: CodexLocaleStatus | null | undefined): boolean {
  return (
    !!s && s.homes.length > 0 && s.homes.every((h) => h.current === "zh-CN")
  );
}

export default function GptZh({ onDone }: { onDone?: () => void }) {
  const toast = useToast();
  const [status, setStatus] = useState<CodexLocaleStatus | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState<"on" | "off" | null>(null);
  const [ask, setAsk] = useState<"on" | "off" | null>(null);
  const [outcome, setOutcome] = useState<CodexLocaleOutcome | null>(null);

  const load = useCallback(async () => {
    try {
      setStatus(await codexApi.localeStatus());
      setError("");
    } catch (e) {
      setError(errorText(e));
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load]);

  async function run(kind: "on" | "off") {
    setAsk(null);
    setBusy(kind);
    setOutcome(null);
    try {
      const o = await codexApi.localeSet(kind === "on");
      setOutcome(o);
      toast.ok(
        kind === "on"
          ? `已设为中文（改了 ${o.changed} 份）`
          : `已恢复默认（改了 ${o.changed} 份）`,
      );
      onDone?.();
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(null);
      invalidate("codexLocale", "codexDesktop", "codexAccounts");
      await load();
    }
  }

  const on = gptZhOn(status);
  const ours = status?.ours_running ?? 0;
  const blocked = !!status?.egress_running || !!status?.processes_error;
  /** 开着的 GPT 要先关的时候才弹确认框；没开着直接改。 */
  const request = (kind: "on" | "off") =>
    ours > 0 ? setAsk(kind) : void run(kind);

  return (
    <div className="flex flex-col gap-3" data-testid="gpt-zh-panel">
      <div className="flex flex-wrap items-center gap-2">
        <Pill tone={on ? "ok" : "default"}>
          {status ? (on ? "已是中文" : "未全设为中文") : "读取中…"}
        </Pill>
        {status && ours > 0 && (
          <Pill tone="warn">面板起的 GPT 开着 {ours} 个</Pill>
        )}
      </div>
      {error && (
        <p role="alert" className="notice notice--danger">
          {error}
        </p>
      )}
      {status?.egress_running && (
        <p role="alert" className="notice notice--danger">
          Codex 出站插件正接管着当前槽位的
          config.toml（停下时会整份恢复），这时改会被它冲掉。先在扩展中心停掉它。
        </p>
      )}
      {status?.processes_error && (
        <p role="alert" className="notice notice--danger">
          查不到 GPT 桌面端有没有在跑（{status.processes_error}），这时不改。
        </p>
      )}

      {status && (
        <div className="flex flex-col gap-1 text-sm">
          {status.homes.length === 0 && (
            <span className="notice">
              还没有 GPT 槽位，也没有默认的 ~\.codex。先在左边新建一个槽位。
            </span>
          )}
          {status.homes.map((h) => (
            <span key={h.config} style={{ overflowWrap: "anywhere" }}>
              {h.label}：
              {h.error
                ? `读不出来（${h.error}），不改`
                : h.current
                  ? `界面语言 ${h.current}`
                  : "没设（跟随 GPT 自己的默认）"}
              {h.adopted ? ` · GPT 上次运行时用的是 ${h.adopted}` : ""}
              <span className="notice"> · {h.config}</span>
            </span>
          ))}
        </div>
      )}

      <div className="flex flex-wrap gap-2">
        <Button
          variant="primary"
          icon={<Languages size={14} />}
          disabled={!status || busy !== null || blocked || on}
          loading={busy === "on"}
          data-testid="gpt-zh-apply"
          onClick={() => request("on")}
        >
          {on ? "已是中文" : "设为中文"}
        </Button>
        <Button
          variant="danger"
          icon={<RotateCcw size={14} />}
          disabled={!status || busy !== null || blocked}
          loading={busy === "off"}
          onClick={() => request("off")}
        >
          恢复默认
        </Button>
      </div>
      <p className="notice">
        {ours > 0
          ? "面板起的 GPT 桌面端正开着：点了会先关掉它（正在跑的任务会停），改完按当前槽位重新打开。"
          : "GPT 桌面端只在启动时读这一项：改完下次打开就是中文。"}
        {(status?.foreign_running ?? 0) > 0
          ? " 别处（开始菜单等）起的 GPT 开着，默认那一份这次不改。"
          : ""}
      </p>

      {outcome && (
        <div data-testid="gpt-zh-outcome">
          <LogView
            lines={[
              ...outcome.lines,
              ...(outcome.reopened ? [outcome.reopened] : []),
            ]}
          />
        </div>
      )}

      <p className="notice">
        写的是 GPT <strong>自己的设置项</strong>：config.toml 里 [desktop] 表的
        localeOverride = "zh-CN"，跟在它「设置 → General →
        Language」里选中文写下的是同一行。不碰程序文件、不碰登录文件（auth.json）、也不动
        config.toml 里别的任何一项。「恢复默认」只撤面板设的中文，你在 GPT
        里自己选的别的语言不动。 中转环境的 GPT 不跟着改，在它自己的设置里选。
      </p>
      <p className="notice">
        重启之后界面还是英文？那是 OpenAI 按账户 /
        机器放开中文界面的远端开关（enable_i18n）没给你开——
        设置项已经写好了（上面「GPT 上次运行时用的是
        zh-CN」就是它读到了的证据），界面翻不翻译由 OpenAI
        决定，面板不碰那个开关（
        <ExternalLink href="https://github.com/openai/codex/issues/19239">
          openai/codex#19239
        </ExternalLink>
        ）。界面语言很可能会随请求带给
        OpenAI（面板没核实）；体检里「浏览器语言与出口地区对不上」那一条跟它是同一类信号。
      </p>

      <ConfirmDialog
        open={ask !== null}
        onCancel={() => setAsk(null)}
        onConfirm={() => ask && void run(ask)}
        title={ask === "on" ? "把 GPT 设为中文？" : "把 GPT 恢复成默认语言？"}
        confirmLabel="关掉 GPT 并修改"
        loading={busy !== null}
        danger
      >
        <p>
          面板起的 GPT 桌面端正开着（{ours} 个）：会先<strong>关掉它</strong>
          ——它正在跑的任务会停，先保存手头的工作。别处起的那份不动。
        </p>
        <p className="mt-2">
          改完按当前槽位重新打开（跟账户页「打开」同一条门禁链）。
        </p>
      </ConfirmDialog>
    </div>
  );
}
