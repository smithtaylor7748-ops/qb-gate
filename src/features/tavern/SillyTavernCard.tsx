/**
 * 软件页「SillyTavern（酒馆）」卡：酒馆**作为一个软件**的那一半。
 *
 * 2026-09-29 建（使用者：「酒馆变成本项目的功能之一，软件页也增加下载酒馆的功能，然后被本项目接管」）；
 * 同一天第二轮「前端完全重构」时它分成了七页（概览 · 版本 · 控制台 · 角色卡 · 世界书 · 扩展 · 备份与回收站）。
 * 第三轮使用者：「错了错了，1弹窗太小，2这个应该在软件页面，软件页面的应该在这个官方账户弹窗页面」——
 * 用酒馆的那几页搬进了账户页「酒馆」磁贴右上角的弹窗（`TavernPanel.tsx`），这张卡只留问他时选的
 * 「下载安装 / 检查更新 / 指定版本 / 卸载留在软件页」，外加「位置与端口」（酒馆本体装在哪、用哪一份）。
 * 三条桥的设置在软件页各自那张 CLI 卡的最下面（`bridge/BridgeSection.tsx`）。
 *
 * # 这张卡长什么样
 *
 * - 右上角：面板装的 / 你自己的 / 未安装 + 下载安装（检查并更新）
 * - 一排摘要：版本 · 状态 · Node.js · 目录
 * - 装 / 换版本的进度
 * - 版本（现在 · GitHub 上 · 指定版本）→ 位置与端口 → 卸载
 *
 * # ⛔ 四条
 *
 * 1. **代价常驻在确认框里**（`card/SetupDialogs.tsx`）：下载、几分钟 npm、会先停酒馆、卸载前备份到哪。
 * 2. **Pill 与版本行说同一件事**（`test:ui` 的「every pill agrees with its version row」查这张卡）：
 *    摘要里那个 `.metric-v` 是这张卡上第一个版本行；读取中时两边都不说「未安装」。
 *    这张卡上别的地方不许出现写着「未安装」「未装」的 Pill。
 * 3. **联网只在点了之后**：最新版、版本清单都是按钮，没有定时器。
 * 4. **起酒馆、停酒馆不在这张卡上**：起 = 账户页磁贴，停 = 账户页「酒馆」弹窗底部。
 */
import { useEffect, useRef } from "react";
import { Download } from "lucide-react";
import { useSearchParams } from "react-router-dom";

import type { TavernSetupStatus } from "../../lib/api";
import { R } from "../../lib/resources";
import { useResource } from "../../lib/store";
import { useTask } from "../../lib/tasks";
import { Button, Card, Pill } from "../../ui";
import { TaskBlock } from "../../pages/softwareParts";
import Location from "./card/Location";
import { useSetupDialogs } from "./card/SetupDialogs";
import Versions from "./card/Versions";

export default function SillyTavernCard() {
  const [params] = useSearchParams();
  const anchor = useRef<HTMLDivElement>(null);
  const setupRes = useResource("tavernSetup", R.tavernSetup);
  const task = useTask("install-sillytavern");
  const s = setupRes.data;
  const setup = useSetupDialogs(s);

  // 从账户页（磁贴「酒馆还没装好」、酒馆弹窗底部「下载与版本」）、快速跳转进来：滚到这张卡。
  useEffect(() => {
    if (params.get("focus") === "sillytavern")
      anchor.current?.scrollIntoView({ block: "start" });
  }, [params]);

  const managed = s?.managed ?? null;
  const pill = !s
    ? { tone: "default" as const, text: "读取中" }
    : !s.present
      ? { tone: "warn" as const, text: "未安装" }
      : managed
        ? { tone: "ok" as const, text: "面板装的" }
        : { tone: "accent" as const, text: "你自己的" };

  return (
    <div ref={anchor} id="sillytavern" className="scroll-mt-4">
      <Card
        title="SillyTavern（酒馆）"
        className="mb-3 tv-card"
        actions={
          <>
            <Pill tone={pill.tone}>{pill.text}</Pill>
            <Button
              size="sm"
              icon={<Download size={13} />}
              loading={setup.busy === "install" || setup.busy === "update"}
              disabled={!s || !!setup.busy || task.running}
              onClick={() => setup.open(managed ? "update" : "install")}
            >
              {managed ? "检查并更新" : "下载安装"}
            </Button>
          </>
        }
      >
        <Summary s={s} />
        <TaskBlock task={task} />
        <p className="notice tv-card-where">
          用酒馆（控制台、角色卡、世界书、扩展、备份与回收站）在账户页「酒馆」磁贴右上角的设置里；
          三条桥的设置在 Claude Code / Codex CLI / Gemini CLI 三张卡的最下面。
        </p>
        <div className="tv-card-body">
          <Versions s={s} setup={setup} taskRunning={task.running}>
            <Location
              root={s?.root ?? ""}
              launcher={s?.launcher ?? ""}
              configPort={s?.config_port ?? null}
            />
          </Versions>
        </div>
      </Card>
      {setup.dialogs}
    </div>
  );
}

/** 一直看得见的那一排：版本 · 状态 · Node.js · 目录。 */
function Summary({ s }: { s: TavernSetupStatus | undefined }) {
  const status = !s
    ? "…"
    : s.running
      ? "在跑"
      : s.foreign_running
        ? "别人起的在跑"
        : "没在跑";
  return (
    <div className="tv-summary">
      <div className="tv-stat">
        <span className="tv-stat-label">版本</span>
        {/* ⛔ 这是这张卡上第一个 `.metric-v`：跟右上角的 Pill 说同一件事（读取中时两边都不说「未安装」）。 */}
        <span className="metric-v mono">
          {!s ? "…" : s.present ? `v${s.version ?? "?"}` : "未安装"}
        </span>
        <span className="tv-stat-sub">
          {!s
            ? ""
            : s.managed
              ? `面板装的 · 标签 ${s.managed.tag}${s.previous_version ? ` · 留着 ${s.previous_version}` : ""}`
              : s.present
                ? "你自己的那份（面板只起停）"
                : "右上角「下载安装」装一份"}
        </span>
      </div>
      <div className="tv-stat">
        <span className="tv-stat-label">状态</span>
        <span
          className={`tv-stat-v${s?.running ? " tv-stat-v--ok" : s?.foreign_running ? " tv-stat-v--warn" : ""}`}
        >
          {status}
        </span>
        <span className="tv-stat-sub mono">
          {s ? `127.0.0.1:${s.st_port}` : ""}
        </span>
      </div>
      <div className="tv-stat">
        <span className="tv-stat-label">Node.js</span>
        <span
          className={`tv-stat-v mono${s && s.node_ok === false ? " tv-stat-v--warn" : ""}`}
        >
          {!s ? "…" : s.node.path ? (s.node.version ?? "版本读不出") : "没找到"}
        </span>
        <span className="tv-stat-sub">
          {s?.node_engine
            ? `它要求 ${s.node_engine}${s.node_ok === false ? " · 不够新" : s.node_ok ? " · 够" : ""}`
            : ""}
        </span>
      </div>
      <div className="tv-stat tv-stat--wide">
        <span className="tv-stat-label">目录</span>
        <span className="tv-stat-path mono">{s ? s.root || "没配" : "…"}</span>
      </div>
    </div>
  );
}
