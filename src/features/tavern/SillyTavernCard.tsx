/**
 * 软件页「SillyTavern（酒馆）」卡：酒馆本体的全部（2026-09-29 第二轮）。
 *
 * 2026-09-29 建（使用者：「酒馆变成本项目的功能之一，软件页也增加下载酒馆的功能，然后被本项目接管」）；
 * 同一天第二轮「前端完全重构」（「原位置啊……直接做这个基础上改」）分成了七页：
 * 概览 · 版本 · 控制台 · 角色卡 · 世界书 · 扩展 · 备份与回收站（`card/` 下各一个文件）。
 * 三条桥的设置在三个账户页「酒馆」磁贴右上角，各装各的（`bridge/BridgeModal.tsx`）。
 *
 * # 这张卡长什么样
 *
 * - 标题行：名称 + 版本 / 状态 Pill + 主按钮（下载安装 / 检查并更新），下面一行说明加「当前：<目录>」
 * - 一排常驻摘要：版本 · 状态 · Node.js · 目录
 * - 装 / 换版本的进度
 * - 分页条（方向键可切；上次停的页记在 localStorage `qb-tavern-tab`；`?tab=console` 直达）
 *
 * # ⛔ 五条
 *
 * 1. **代价常驻在确认框里**（`card/SetupDialogs.tsx`，主按钮和「版本」分页共用）：下载、几分钟 npm、会先停酒馆、卸载前备份到哪。
 * 2. **Pill 与版本行说同一件事**（`test:ui` 的「every pill agrees with its version row」查这张卡）：
 *    摘要里那个 `.metric-v` 是这张卡上第一个版本行；读取中时两边都不说「未安装」。
 *    这张卡上别的地方不许出现写着「未安装」「未装」的 Pill。
 * 3. **联网只在点了之后**：最新版、版本清单都是按钮，没有定时器。
 * 4. **分页不在前台不读**：只挂当前那一页（控制台不轮询、角色卡不列）。
 * 5. **起酒馆、停酒馆不在这张卡上**：起 = 账户页磁贴，停 = 磁贴右上角那一家的桥接设置底部。
 */
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Download } from "lucide-react";
import { useSearchParams } from "react-router-dom";

import type { TavernSetupStatus } from "../../lib/api";
import { R } from "../../lib/resources";
import { useResource } from "../../lib/store";
import { useTask } from "../../lib/tasks";
import { Button, Card, Pill } from "../../ui";
import { TaskBlock } from "../../pages/softwareParts";
import RelocateControl from "../../pages/managed/RelocateControl";
import Characters from "./card/Characters";
import {
  TAVERN_TABS,
  isTavernTab,
  rememberTab,
  rememberedTab,
  type TavernTab,
} from "./card/common";
import Console from "./card/Console";
import DataTab from "./card/DataTab";
import Extensions from "./card/Extensions";
import Overview from "./card/Overview";
import { useSetupDialogs } from "./card/SetupDialogs";
import Versions from "./card/Versions";
import Worlds from "./card/Worlds";

export default function SillyTavernCard() {
  const [params] = useSearchParams();
  const anchor = useRef<HTMLDivElement>(null);
  const tabs = useRef<HTMLDivElement>(null);
  const setupRes = useResource("tavernSetup", R.tavernSetup);
  const task = useTask("install-sillytavern");
  const s = setupRes.data;
  const setup = useSetupDialogs(s);
  const linked = params.get("tab");
  const [tab, setTabState] = useState<TavernTab>(() =>
    isTavernTab(linked) ? linked : (rememberedTab() ?? "overview"),
  );
  const setTab = (t: TavernTab) => {
    setTabState(t);
    rememberTab(t);
  };

  // 从账户页（磁贴「酒馆还没装好」、桥接设置底部「看酒馆控制台」「到软件页」）、快速跳转进来：滚到这张卡；
  // 带 `?tab=` 就直接落在那一页。
  useEffect(() => {
    if (isTavernTab(linked)) {
      setTabState(linked);
      rememberTab(linked);
    }
    if (params.get("focus") === "sillytavern")
      anchor.current?.scrollIntoView({ block: "start" });
  }, [params, linked]);

  /** 分页条的键盘操作（左右键、Home / End），照 WAI-ARIA 的 tabs 写法。 */
  function onTabKey(e: KeyboardEvent<HTMLDivElement>) {
    const i = TAVERN_TABS.findIndex((t) => t.id === tab);
    const next =
      e.key === "ArrowRight"
        ? (i + 1) % TAVERN_TABS.length
        : e.key === "ArrowLeft"
          ? (i - 1 + TAVERN_TABS.length) % TAVERN_TABS.length
          : e.key === "Home"
            ? 0
            : e.key === "End"
              ? TAVERN_TABS.length - 1
              : -1;
    if (next < 0) return;
    e.preventDefault();
    const id = TAVERN_TABS[next].id;
    setTab(id);
    tabs.current?.querySelector<HTMLButtonElement>(`#tv-tab-${id}`)?.focus();
  }

  const managed = s?.managed ?? null;
  const owner = managed ? "面板装的" : "你自己的";
  const pill = !s
    ? { tone: "default" as const, text: "读取中" }
    : !s.present
      ? { tone: "default" as const, text: "未安装" }
      : {
          tone: managed ? ("ok" as const) : ("accent" as const),
          text: `v${s.version ?? "?"} · ${owner}`,
        };

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
        <p className="notice mb-2 break-words">
          从 SillyTavern 官方 GitHub
          取正式版，原样解压、装依赖，三个账户页的酒馆共用这一份；
          三条桥的设置在各自账户页「酒馆」磁贴右上角。
          {s?.root && (
            <>
              {" "}
              当前：<code>{s.root}</code>
            </>
          )}
        </p>
        <RelocateControl
          app="sillytavern"
          onMoved={() => void setupRes.refresh()}
        />
        <Summary s={s} />
        <TaskBlock task={task} />
        <div
          ref={tabs}
          className="tv-tabs"
          role="tablist"
          aria-label="酒馆的分页"
          onKeyDown={onTabKey}
        >
          {TAVERN_TABS.map((t) => (
            <button
              key={t.id}
              id={`tv-tab-${t.id}`}
              type="button"
              role="tab"
              aria-selected={tab === t.id}
              aria-controls="tv-tabpanel"
              tabIndex={tab === t.id ? 0 : -1}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>
        <div
          id="tv-tabpanel"
          role="tabpanel"
          aria-labelledby={`tv-tab-${tab}`}
          className="tv-panel"
        >
          {tab === "overview" && <Overview s={s} onTab={setTab} />}
          {tab === "versions" && (
            <Versions s={s} setup={setup} taskRunning={task.running} />
          )}
          {tab === "console" && <Console />}
          {tab === "characters" && <Characters />}
          {tab === "worlds" && <Worlds />}
          {tab === "extensions" && <Extensions />}
          {tab === "data" && <DataTab />}
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
