/**
 * Codex 出站与换出口插件 —— 接入本机已安装的 ccodex-sleep-state（外部程序，GPL-3.0）。
 *
 * 本面板只做接入：定位 exe、启动（新开控制台窗口，独立于面板生命周期）、
 * 停止（结束进程 + 跑它自己的 restore 恢复 Codex 配置）、打开它的网页面板。
 * 代理 / 订阅 / 机场出站 / 换出口凑 292 都在它自己的仓库里实现；
 * 核心不内置代理、不换出口。跟核心的官方 turn-state 识别互斥（后端守卫）。
 */

import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Download, ExternalLink as LinkIcon, Play, Square } from "lucide-react";

import { api } from "../lib/api";
import type { EgressConfig } from "../lib/generated/EgressConfig";
import type { PluginStatus } from "../lib/generated/PluginStatus";
import { invalidate } from "../lib/store";
import { endTask, resetTask, useTask } from "../lib/tasks";
import { Button, PathField, Pill, ProgressBar, useToast } from "../ui";

const ADMIN_URL = "http://127.0.0.1:17841/admin/";

export default function CodexEgressPanel() {
  const toast = useToast();
  const [status, setStatus] = useState<PluginStatus | null>(null);
  const [cfg, setCfg] = useState<EgressConfig>({ exe: null });
  const [exe, setExe] = useState("");
  const [busy, setBusy] = useState<
    "start" | "stop" | "save" | "install" | null
  >(null);
  const task = useTask("egress-install");
  const [error, setError] = useState("");

  async function refresh() {
    try {
      const [s, c] = await Promise.all([
        api.codexEgressStatus(),
        api.codexEgressConfig(),
      ]);
      setStatus(s);
      setCfg(c);
      setExe(c.exe ?? "");
      setError("");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }
  useEffect(() => {
    void refresh();
    const t = window.setInterval(() => void refresh(), 10_000);
    return () => window.clearInterval(t);
  }, []);

  async function save() {
    setBusy("save");
    try {
      await api.codexEgressConfigSave({ exe: exe.trim() || null });
      toast.ok("已保存 exe 位置");
      await refresh();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }
  async function start() {
    setBusy("start");
    try {
      const url = await api.codexEgressStart();
      invalidate("plugins");
      // 它的 setup 通常会自己打开面板；这里再开一次也无妨，少开一次却会让
      // 「成功启动」和「卡住」看起来一样。
      try {
        await openUrl(url);
      } catch {
        /* 浏览器没起来就让使用者照下面的地址手动开 */
      }
      toast.ok(
        "插件已启动，已打开它的面板；退出请在它的窗口按 Ctrl+C 或点这里的停止",
      );
      await refresh();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }
  /**
   * 一键从它的 Releases 下载 Windows 包、按发布自带的 SHA256SUMS 校验、解压到默认位置并登记。
   * 校验防的是下载坏包、下错包；仓库本身是使用者自己的，那一层由他自己守。
   */
  async function install() {
    setBusy("install");
    resetTask("egress-install");
    try {
      const done = await api.codexEgressInstall();
      endTask("egress-install");
      invalidate("plugins");
      toast.ok(
        `已安装 ${done.tag}，SHA-256 已核对：${done.sha256.slice(0, 12)}…`,
      );
      await refresh();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask("egress-install", msg);
      toast.error(msg);
    } finally {
      setBusy(null);
    }
  }
  async function stop() {
    setBusy("stop");
    try {
      const done = await api.codexEgressStop();
      invalidate("plugins");
      toast.ok(done.join("；"));
      await refresh();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  const running = status?.state === "running";
  const missing = !status || status.state === "missing";
  const dirty = (exe.trim() || null) !== (cfg.exe ?? null);

  return (
    <div className="mt-3 flex flex-col gap-3" data-testid="codex-egress-panel">
      <div className="flex flex-wrap items-center gap-2">
        <Pill
          tone={
            running ? "ok" : status?.state === "ready" ? "accent" : "default"
          }
        >
          {running
            ? "运行中"
            : status?.state === "ready"
              ? "已安装"
              : status?.state === "broken"
                ? "有问题"
                : "未安装"}
        </Pill>
        <span className="notice">{status?.detail ?? "读取中…"}</span>
      </div>
      {error && (
        <p role="alert" className="notice notice--danger">
          {error}
        </p>
      )}
      {status?.checks.map((c) => (
        <div key={c.label} className="flex flex-wrap gap-2">
          <Pill tone={c.ok ? "ok" : "default"}>{c.label}</Pill>
          <span className="notice" style={{ overflowWrap: "anywhere" }}>
            {c.detail}
          </span>
        </div>
      ))}

      <PathField
        label="ccodex-sleep-state.exe 的位置（留空 = 按默认解压位置找）"
        value={exe}
        onChange={setExe}
        kind="file"
        disabled={busy !== null}
        hint="点下面的「下载并安装」就不用填；自己解压到 %LOCALAPPDATA%\Programs\ccodex-sleep-state 也不用填。"
      />
      <div className="flex flex-wrap gap-2">
        <Button
          variant={missing ? "primary" : "default"}
          icon={<Download size={14} />}
          disabled={busy !== null || running}
          loading={busy === "install"}
          onClick={() => void install()}
        >
          {missing ? "下载并安装（校验 SHA256）" : "下载并更新（校验 SHA256）"}
        </Button>
        <Button
          size="sm"
          disabled={busy !== null || !dirty}
          loading={busy === "save"}
          onClick={() => void save()}
        >
          保存位置
        </Button>
        <Button
          variant="primary"
          icon={<Play size={14} />}
          disabled={busy !== null || missing || running}
          loading={busy === "start"}
          onClick={() => void start()}
        >
          启动插件
        </Button>
        <Button
          variant="danger"
          icon={<Square size={14} />}
          disabled={busy !== null || !running}
          loading={busy === "stop"}
          onClick={() => void stop()}
        >
          停止并恢复 Codex 配置
        </Button>
        <Button
          size="sm"
          icon={<LinkIcon size={14} />}
          disabled={!running}
          onClick={() => void openUrl(ADMIN_URL)}
        >
          打开它的面板
        </Button>
      </div>

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
            label="插件安装进度"
          />
          {task.error && (
            <p className="notice notice--danger mt-1.5">{task.error}</p>
          )}
        </div>
      )}

      <p className="notice">
        它是一个<strong>独立的第三方程序</strong>
        （GPL-3.0），本面板只负责下载它发布的二进制包 （按发布自带的 SHA256SUMS
        校验）、启停与接入，不复制它的源码、不把它编进本程序。代理 / 订阅 /
        机场出站，以及实验性的「换出口凑 292」都在它自己的仓库里实现；QB Gate
        核心仍然不内置代理、不换出口。
      </p>
      <p className="notice">
        <strong>它接管的是当前激活的 Codex 账户槽位</strong>
        （没有槽位就是默认 ~/.codex；上面「接管的 Codex 目录」写着是哪份），
        退出时恢复。启动后按槽位打开的 Codex 就会经过它；接管期间别切换槽位。
        跟账户页的「识别（turn-state）」一次只能开一个——两边都要改同一个槽位的
        Codex 配置；识别开着时后端会拒绝，到账户页「识别」弹窗点「关闭识别」
        （会恢复槽位配置）即可。它自己的说明也讲清了：符合 292/332
        不证明模型质量，也不增加额度；探测会消耗你自己的额度。
      </p>
      <p className="notice">
        停止用这里的按钮（会结束进程并跑它自己的 restore
        恢复配置），或到它的窗口按
        Ctrl+C。不是本面板启动的那份，本面板不替它停。
      </p>
    </div>
  );
}
