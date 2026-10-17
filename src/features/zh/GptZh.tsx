/**
 * GPT（Codex 桌面端）界面语言：一键设成中文（2026-09-25，使用者定的）。
 *
 * 写的是 GPT **自己的设置项** —— `CODEX_HOME\config.toml` 里 `[desktop]` 表的
 * `localeOverride = "zh-CN"`，跟在它「设置 → General → Language」里选中文写下的是同一行。
 * 改哪几份是使用者选的：全部 GPT 槽位 + 默认那份 + 以后新建的槽位。弹窗里逐个列出来。
 *
 * GPT 只在启动时读这一项：要写的那几份里、面板起的 GPT 开着就要先关（确认框里写明任务会停），
 * 改完按当前槽位重开；不想关的话，在 GPT 自己的设置里选中文是立刻生效的（写的是同一行）。
 * 别处起的那份面板不关（2026-09-23 那条），它开着时默认那一份这次不改。
 *
 * GPT 自己把某一份写坏了（同一张表写了两遍，它因此一项桌面设置都读不到）时，「设为中文」
 * 先把重复段并掉再写（2026-09-27，使用者选的）。成功还是失败按后端给的几个数说（`gptZhResult`），
 * **不去猜那几句话** —— 09-26 那版对中文日志行跑正则，措辞一改就判错。
 */
import { useCallback, useEffect, useState } from "react";
import { Languages, RotateCcw } from "lucide-react";

import { codexApi } from "../../lib/codexAccounts";
import type { CodexLocaleHome } from "../../lib/generated/CodexLocaleHome";
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

/** 所有份都设成了中文（GPT 读得到的那个值）。一份都没有时不算。 */
export function gptZhOn(s: CodexLocaleStatus | null | undefined): boolean {
  return (
    !!s && s.homes.length > 0 && s.homes.every((h) => h.current === "zh-CN")
  );
}

/** 一份的现状，一句话。 */
export function gptZhHomeText(h: CodexLocaleHome): string {
  if (h.error) return `GPT 读不了这份，也没法自动修（${h.error}），不改`;
  if (h.repair)
    return `GPT 现在读不了这份：${h.repair} 重复定义了，它因此读不到里面任何一项桌面设置（含语言）。点「设为中文」会先把重复段并掉，不改任何值`;
  return h.current ? `界面语言 ${h.current}` : "没设（跟随 GPT 自己的默认）";
}

/** 点完之后那一句提示：成功还是失败，全看后端给的数。 */
export function gptZhResult(
  o: CodexLocaleOutcome,
  kind: "on" | "off",
): { ok: boolean; text: string } {
  if (o.skipped > 0) {
    const done = o.changed > 0 ? `改了 ${o.changed} 份；` : "";
    if (o.default_busy && o.skipped === 1)
      return {
        ok: false,
        text: `${done}默认那一份没改：别处起的 GPT 正开着。完全退出它（托盘里也退），再点一次。`,
      };
    return {
      ok: false,
      text: `${done}有 ${o.skipped} 份没改成，原因见下面逐条。`,
    };
  }
  if (o.changed === 0)
    return {
      ok: true,
      text:
        kind === "on"
          ? "本来就是中文，没有要改的。"
          : "没有面板设的中文，不用改。",
    };
  if (kind === "off")
    return { ok: true, text: `已恢复默认（改了 ${o.changed} 份）。` };
  return {
    ok: true,
    text: `已设为中文（改了 ${o.changed} 份${
      o.repaired > 0 ? `，其中 ${o.repaired} 份顺手修了 GPT 自己写重复的表` : ""
    }）。下次打开 GPT 生效。`,
  };
}

/** 不想关掉正在用的 GPT 时的另一条路 —— GPT 自己的设置，立刻生效。 */
const IN_APP =
  "不想关掉正在用的 GPT？在它自己的「设置 → General → Language」里选「中文（中国）」，立刻生效 —— 写的就是同一行。";

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
      const r = gptZhResult(o, kind);
      if (r.ok) toast.ok(r.text);
      else toast.error(r.text);
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
  /** 开着的 GPT 可能要先关的时候才弹确认框；没开着直接改。 */
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
              {h.label}：{gptZhHomeText(h)}
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
          ? `面板起的 GPT 桌面端正开着：要改的那几份里它正在用的会先关掉（正在跑的任务会停），改完按当前槽位重新打开。${IN_APP}`
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
        Language」里选中文写下的是同一行。不碰程序文件、不碰登录文件（auth.json）、也不改
        config.toml 里别的任何一项的值；GPT
        自己把某一份写坏了（同一张表写了两遍，它因此读不到里面任何桌面设置）时，
        先把重复段并掉再写。「恢复默认」只撤面板设的中文，你在 GPT
        里自己选的别的语言不动。中转环境的 GPT 不跟着改，在它自己的设置里选。
      </p>
      <p className="notice">
        重启之后界面还是英文？先看上面那一份写的「GPT 上次运行时用的是」：是
        zh-CN，说明 GPT 已经读到了这项设置，界面翻不翻译是 OpenAI
        按账户放开的远端开关（enable_i18n）决定的，面板不碰那个开关（
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
          面板起的 GPT 桌面端正开着（{ours} 个）：要改的那几份里它正在用的会先
          <strong>关掉</strong>
          ——它正在跑的任务会停，先保存手头的工作。别处起的那份不动；用不着改的那几份也不关。
        </p>
        <p className="mt-2">
          改完按当前槽位重新打开（跟账户页「打开」同一条门禁链）。
        </p>
        <p className="mt-2">{IN_APP}</p>
      </ConfirmDialog>
    </div>
  );
}
