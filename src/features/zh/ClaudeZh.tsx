/**
 * Claude 桌面端 · 中文界面（插件 claude-desktop-zh-cn，2026-09-25，使用者定的）。
 *
 * **一个组件两处用**（0.32.0 BridgeSettings 那条教训：同一块功能写两份，迟早一份没保存、
 * 另一份显示旧值）：账户页启动卡标题栏的「汉化」按钮开的弹窗，与扩展中心的插件详情页。
 *
 * 打开时问一次上游最新版 —— 使用者选的节奏（「打开汉化弹窗或插件页时查」），没有定时器；
 * 只查 github.com 的 `releases/latest` 跳转，不走 api.github.com。
 *
 * 代价与边界常驻在按钮旁边，不放悬停（CLAUDE.md 的老规矩）：
 * - 会关掉 Claude 桌面端，**包括它 Code 页里正在跑的会话**；
 * - 只用上游的安全模式；上游另外两种模式（重写 Claude.exe 完整性哈希、Frida 绕过调试开关闸门）
 *   越过了 Anthropic 条款「不得绕过保护措施」，面板永远不调用；
 * - 安全模式也是非官方修改：Anthropic 不支持，Claude 一更新就没了。
 */
import { useCallback, useEffect, useState } from "react";
import { Languages, RefreshCw, RotateCcw } from "lucide-react";

import { api } from "../../lib/api";
import type { ClaudeZhOutcome } from "../../lib/generated/ClaudeZhOutcome";
import type { ClaudeZhStatus } from "../../lib/generated/ClaudeZhStatus";
import { invalidate } from "../../lib/store";
import { endTask, resetTask, useTask } from "../../lib/tasks";
import {
  Button,
  ConfirmDialog,
  ExternalLink,
  LogView,
  Pill,
  ProgressBar,
  useToast,
} from "../../ui";
import type { Tone } from "../../ui";

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** 状态 → 药丸上的字与颜色。按钮文字也从这里取，两处说法不许分叉。 */
export function claudeZhLabel(s: ClaudeZhStatus | null | undefined): {
  text: string;
  tone: Tone;
  entry: string;
} {
  switch (s?.state) {
    case "on":
      return { text: "已汉化", tone: "ok", entry: "汉化：开" };
    case "partial":
      return { text: "只汉化了一半", tone: "warn", entry: "汉化：半" };
    case "needs-reapply":
      return {
        text: "Claude 更新了，需要重新应用",
        tone: "warn",
        entry: "汉化：需重新应用",
      };
    case "unsupported":
      return { text: "做不了", tone: "default", entry: "汉化" };
    case "off":
      return { text: "未汉化", tone: "default", entry: "汉化" };
    default:
      return { text: "读取中…", tone: "default", entry: "汉化" };
  }
}

type Busy = "check" | "apply" | "upgrade" | "restore" | null;

export default function ClaudeZh() {
  const toast = useToast();
  const task = useTask("claude-zh");
  const [status, setStatus] = useState<ClaudeZhStatus | null>(null);
  const [checkError, setCheckError] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState<Busy>(null);
  const [ask, setAsk] = useState<"apply" | "upgrade" | "restore" | null>(null);
  const [outcome, setOutcome] = useState<
    (ClaudeZhOutcome & { kind: "apply" | "upgrade" | "restore" }) | null
  >(null);

  const load = useCallback(async () => {
    try {
      setStatus(await api.claudeZhStatus());
      setError("");
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  /**
   * 问上游最新版（会联网）。失败只把原因摆出来，本机状态照样显示。
   * 不依赖 toast —— 这个函数的身份一变，下面那个 effect 就会再联网问一次。
   */
  const check = useCallback(async (): Promise<boolean> => {
    setBusy((b) => b ?? "check");
    try {
      setStatus(await api.claudeZhCheck());
      setCheckError("");
      return true;
    } catch (e) {
      setCheckError(errorText(e));
      return false;
    } finally {
      setBusy((b) => (b === "check" ? null : b));
    }
  }, []);

  // 打开时问一次（使用者选的节奏）。先读本机，再联网 —— 网慢的时候本机那些先显示出来。
  useEffect(() => {
    void load().then(() => check());
  }, [load, check]);

  async function run(kind: "apply" | "upgrade" | "restore") {
    setAsk(null);
    setBusy(kind);
    setOutcome(null);
    resetTask("claude-zh");
    try {
      const o =
        kind === "restore"
          ? await api.claudeZhRestore()
          : await api.claudeZhApply(kind === "upgrade");
      setOutcome({ ...o, kind });
      endTask("claude-zh", o.ok ? undefined : "核验没全过，见下面逐条");
      if (o.restored) toast.error("上游这一版越线了，已自动还原并停用");
      else if (o.ok)
        toast.ok(
          kind === "restore" ? "已恢复英文，核验通过" : "已汉化，核验通过",
        );
      else toast.error("做完了，但核验没全过，见下面逐条");
    } catch (e) {
      const msg = errorText(e);
      endTask("claude-zh", msg);
      toast.error(msg);
    } finally {
      setBusy(null);
      invalidate("claudeZh", "plugins");
      await load();
    }
  }

  const label = claudeZhLabel(status);
  const unsupported = status?.state === "unsupported";
  const blockedLatest =
    !!status?.latest_tag && status.blocked.includes(status.latest_tag);
  const primary: { kind: "apply" | "upgrade"; text: string } | null =
    !status || unsupported
      ? null
      : status.update_available && status.package_tag
        ? { kind: "upgrade", text: `更新到 ${status.latest_tag} 并重新应用` }
        : status.state === "needs-reapply"
          ? { kind: "apply", text: "重新应用" }
          : status.state === "on"
            ? null
            : { kind: "apply", text: "一键汉化" };
  const running = status?.desktop_running ?? null;

  return (
    <div className="flex flex-col gap-3" data-testid="claude-zh-panel">
      <div className="flex flex-wrap items-center gap-2">
        <Pill tone={label.tone}>{label.text}</Pill>
        {status?.claude_version && (
          <Pill>
            Claude {status.claude_version}
            {status.install === "msix" ? "（MSIX）" : ""}
          </Pill>
        )}
        {status?.package_tag && <Pill>上游 {status.package_tag}</Pill>}
      </div>
      <p className="notice">{status?.detail ?? "读取中…"}</p>
      {error && (
        <p role="alert" className="notice notice--danger">
          {error}
        </p>
      )}

      {status && (
        <div className="flex flex-col gap-1.5 text-sm">
          <span style={{ overflowWrap: "anywhere" }}>
            上游：
            <ExternalLink href={status.upstream}>
              javaht/claude-desktop-zh-cn
            </ExternalLink>
            （MIT，每一版下载后重核许可证）
            {status.package_tag
              ? ` · 本机用的是 ${status.package_tag}${status.package_commit ? `（提交 ${status.package_commit.slice(0, 8)}）` : ""}`
              : " · 还没下载"}
          </span>
          <span>
            {checkError
              ? `查不到上游最新版：${checkError}`
              : status.latest_tag
                ? `上游最新 ${status.latest_tag}${status.latest_checked_at ? `（${status.latest_checked_at} 查的）` : ""}${status.update_available ? " · 有新版" : ""}${blockedLatest ? " · 这一版被停用了" : ""}`
                : busy === "check"
                  ? "正在问上游最新版…"
                  : "还没查过上游最新版"}
          </span>
          {status.applied_tag && (
            <span>
              上次汉化：上游 {status.applied_tag} → Claude{" "}
              {status.applied_claude ?? "?"}（{status.applied_at ?? "?"}）
            </span>
          )}
          {status.profiles.length > 0 && (
            <span style={{ overflowWrap: "anywhere" }}>
              各账户资料的界面语言：
              {status.profiles
                .map(
                  (p) =>
                    `${p.dir.split(/[\\/]/).pop()} ${p.locale ?? "读不出来"}`,
                )
                .join(" · ")}
            </span>
          )}
          {status.blocked.length > 0 && (
            <span className="notice notice--danger">
              已停用的上游版本：{status.blocked.join("、")}
              （核验发现它们改动了 app.asar 或 Claude.exe）
            </span>
          )}
        </div>
      )}

      <div className="flex flex-wrap gap-2">
        {primary && (
          <Button
            variant="primary"
            icon={<Languages size={14} />}
            disabled={busy !== null || blockedLatest}
            loading={busy === primary.kind}
            data-testid="claude-zh-apply"
            onClick={() => setAsk(primary.kind)}
          >
            {primary.text}
          </Button>
        )}
        <Button
          size="sm"
          icon={<RefreshCw size={14} />}
          disabled={busy !== null}
          loading={busy === "check"}
          onClick={() =>
            void check().then((ok) => ok && toast.ok("已查过上游最新版"))
          }
        >
          检查更新
        </Button>
        {(status?.files_present || status?.applied_tag) && (
          <Button
            variant="danger"
            icon={<RotateCcw size={14} />}
            disabled={busy !== null || unsupported}
            loading={busy === "restore"}
            data-testid="claude-zh-restore"
            onClick={() => setAsk("restore")}
          >
            恢复英文
          </Button>
        )}
      </div>
      {!unsupported && (
        <p className="notice">
          点了会先<strong>关掉 Claude 桌面端</strong>
          ——包括它 Code 页里正在跑的会话，先保存手头的工作。
          {running !== null && running > 0
            ? ` 它现在开着（${running} 个进程）。`
            : ""}
          {running === null ? " 查不到它现在开没开。" : ""}
        </p>
      )}

      {(task.running || task.error) && (
        <div>
          <div className="mb-1.5 flex items-center gap-2">
            <span className="text-sm">{task.phase || "准备中…"}</span>
            {task.total > 0 && (
              <span className="notice ml-auto">
                {task.step} / {task.total}
              </span>
            )}
          </div>
          <ProgressBar
            value={task.total > 0 ? (task.step / task.total) * 100 : undefined}
            tone={task.error ? "danger" : "accent"}
            label="Claude 汉化进度"
          />
          {task.error && (
            <p className="notice notice--danger mt-1.5">{task.error}</p>
          )}
        </div>
      )}
      {outcome && (
        <div data-testid="claude-zh-outcome">
          <p
            className={`notice ${outcome.ok ? "" : "notice--danger"}`}
            role={outcome.ok ? undefined : "alert"}
          >
            {outcome.restored
              ? "越线了：已用上游自己的卸载还原，并停用了这一版上游。"
              : outcome.ok
                ? outcome.kind === "restore"
                  ? "核验通过：已恢复英文。下次启动 Claude 桌面端就是原来的界面。"
                  : "核验通过。关掉这个窗口，从「Claude 桌面端」那块磁贴启动就是中文界面。"
                : "做完了，但核验没全过："}
          </p>
          <LogView lines={outcome.lines} />
        </div>
      )}
      {task.log.length > 0 && (
        <LogView lines={task.log} follow={task.running} />
      )}

      <p className="notice">
        <strong>面板做什么：</strong>从上游 GitHub Release
        取它这一版的源码归档，只解出 Windows
        安全模式用得到的脚本与翻译（它仓库里的 Frida
        实验脚本一个字节都不落盘）， 跑它的{" "}
        <code>install zh-CN -PatchMode safe</code>
        ：往 Claude 安装目录放三份翻译文件、改前端 JS
        里的语言列表与硬编码英文、把界面语言设成
        zh-CN。面板再把其他账户资料的界面语言也设成
        zh-CN（改之前的值记下来，恢复英文时写回）。
      </p>
      <p className="notice">
        <strong>永远不做：</strong>上游另外两种模式——改 app.asar 并重写
        Claude.exe 的完整性哈希、用 Frida 在内存里绕过「带调试开关就拒绝启动」——
        都是绕过 Claude 的保护措施，Anthropic
        消费者条款明文不许，面板永远不调用。每次应用前后面板自己比对 app.asar 与
        Claude.exe 的哈希和签名状态，变了就立刻还原、停用那一版上游。
      </p>
      <p className="notice">
        安全模式也是<strong>非官方修改</strong>：Anthropic
        不提供中文界面、也不支持它，Claude
        一自动更新就会换一个新目录、汉化随之没了（这里会变成「需要重新应用」）。在线的
        claude.ai
        页面（对话等）仍是英文——安全模式不改它们。只改本机的界面文字，不碰账户、网络与请求头；
        Claude
        桌面端本来就会上报系统的首选语言，汉化与否它都看得到。只支持官网安装包（Squirrel）装的
        Claude 桌面端。
      </p>

      <ConfirmDialog
        open={ask !== null}
        onCancel={() => setAsk(null)}
        onConfirm={() => ask && void run(ask)}
        title={
          ask === "restore"
            ? "恢复英文？"
            : ask === "upgrade"
              ? `更新到上游 ${status?.latest_tag ?? ""} 并重新应用？`
              : "一键汉化 Claude 桌面端？"
        }
        confirmLabel={
          ask === "restore" ? "关掉 Claude 并恢复" : "关掉 Claude 并汉化"
        }
        loading={busy !== null && busy !== "check"}
        danger
      >
        <p>
          会先<strong>关掉 Claude 桌面端</strong>——包括它 Code
          页里正在跑的会话。先保存手头的工作。
        </p>
        <p className="mt-2">
          {ask === "restore"
            ? "然后跑上游自己的卸载（从它的备份还原前端文件、删掉翻译），再把各账户资料的界面语言写回汉化之前的值。"
            : "然后验出口 IP（上游脚本最后会自己重启一次 Claude，面板随即把它关掉）、跑上游的安全模式、核验 app.asar 与 Claude.exe 没被动过。"}
        </p>
      </ConfirmDialog>
    </div>
  );
}
