/**
 * 「酒馆」弹窗的「概览」分页：酒馆在不在跑、三条桥的设置在哪、依赖齐不齐。
 *
 * ⛔ 起酒馆不在这里：起 = 账户页的「酒馆」磁贴（按厂商起那条桥）；停 = 这个弹窗底部那一行。
 * 「位置与端口」2026-09-29 第三轮搬回了软件页的 SillyTavern 卡 —— 酒馆本体装在哪、用哪一份是「软件」的事，
 * 「酒馆还没装好 → 去软件页」的指路才落得到能改的地方。
 */
import { ExternalLink as LinkIcon, TerminalSquare } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useNavigate } from "react-router-dom";

import type { TavernSetupStatus } from "../../../lib/api";
import { R } from "../../../lib/resources";
import { useResource } from "../../../lib/store";
import { tavernBodyMissing } from "../../../lib/tavernReady";
import { Button, Collapsible, Pill, Row } from "../../../ui";
import {
  BRIDGE_CARD,
  BRIDGE_TITLE,
  bridgeHref,
  type BridgeProvider,
} from "../bridge/where";
import type { TavernTab } from "./common";

const BRIDGES: BridgeProvider[] = ["claude", "gpt", "gemini"];

export default function Overview({
  s,
  onTab,
  onLeave,
}: {
  s: TavernSetupStatus | undefined;
  onTab: (t: TavernTab) => void;
  /** 要离开这个弹窗去别的页面之前调（先关弹窗再跳）。 */
  onLeave: () => void;
}) {
  const navigate = useNavigate();
  const plugins = useResource("plugins", R.plugins);
  const tavern = plugins.data?.find((p) => p.id === "sillytavern");
  const missing = tavern?.checks.filter((c) => !c.ok) ?? [];

  return (
    <div className="tv-overview">
      <section className="tv-block" aria-labelledby="tv-run">
        <h3 id="tv-run" className="tv-block-title">
          运行
        </h3>
        {!s ? (
          <p className="notice">读取中…</p>
        ) : s.running ? (
          <>
            <p className="flex flex-wrap items-center gap-2">
              <Pill tone="ok">在跑</Pill>
              <span className="mono text-xs">127.0.0.1:{s.st_port}</span>
            </p>
            <p className="notice mt-1">
              面板起的这一份。要停它：这个窗口最下面「停止酒馆」（三条桥一起停）。
            </p>
          </>
        ) : s.foreign_running ? (
          <>
            <p>
              <Pill tone="warn">别人起的在跑</Pill>
            </p>
            <p className="notice notice--warn mt-1">
              酒馆端口 {s.st_port}{" "}
              上有一份不是面板起的酒馆（多半是你自己双击开的）。它的输出在它自己的窗口里；
              装、换版本、卸载、导入之前先关掉那个窗口。
            </p>
          </>
        ) : (
          <>
            <p>
              <Pill>没在跑</Pill>
            </p>
            <p className="notice mt-1">
              起酒馆点账户页的「酒馆」磁贴 ——
              一份酒馆、三条桥，按你在哪个账户页点的起那一家的桥。
            </p>
            {tavernBodyMissing(tavern?.checks) && (
              <p className="notice notice--warn mt-1">
                酒馆还没装好：到「软件」页的 SillyTavern
                卡下载安装一份，或在那里指到你自己那份（最下面「下载与版本」）。
              </p>
            )}
          </>
        )}
        <div className="mt-2 flex flex-wrap gap-2">
          {s?.running && (
            <Button
              size="sm"
              icon={<LinkIcon size={12} />}
              onClick={() => void openUrl(`http://127.0.0.1:${s.st_port}`)}
            >
              打开酒馆页面
            </Button>
          )}
          <Button
            size="sm"
            icon={<TerminalSquare size={12} />}
            onClick={() => onTab("console")}
          >
            看控制台
          </Button>
        </div>
      </section>

      <section className="tv-block" aria-labelledby="tv-bridges">
        <h3 id="tv-bridges" className="tv-block-title">
          三条桥
        </h3>
        <p className="notice">
          Claude 走你自己的 bridge.py，GPT / Gemini
          是面板内置的。每条桥的设置在「软件」页它驱动的那个命令行的卡片最下面：
          Claude → {BRIDGE_CARD.claude}，GPT → {BRIDGE_CARD.gpt}，Gemini →{" "}
          {BRIDGE_CARD.gemini}。
        </p>
        {tavern && <p className="notice mt-1 break-words">{tavern.detail}</p>}
        <div className="mt-2 flex flex-wrap gap-2">
          {BRIDGES.map((p) => (
            <Button
              key={p}
              size="sm"
              onClick={() => {
                onLeave();
                navigate(bridgeHref(p));
              }}
            >
              {BRIDGE_TITLE[p]}
            </Button>
          ))}
        </div>
      </section>

      {!!tavern?.checks.length && (
        <Collapsible
          className="tv-wide"
          // 缺东西时默认展开：那正是要看的时候。key 跟着变，状态变了就按新的默认值重来一次。
          key={missing.length ? "bad" : "ok"}
          defaultOpen={missing.length > 0}
          summary={
            <span className="flex items-center gap-1.5">
              依赖
              <Pill tone={missing.length ? "warn" : "ok"}>
                {missing.length ? `${missing.length} 项缺` : "全部就绪"}
              </Pill>
            </span>
          }
        >
          {tavern.checks.map((c) => (
            <Row
              key={c.label}
              side={
                <Pill tone={c.ok ? "ok" : "warn"}>{c.ok ? "有" : "缺"}</Pill>
              }
            >
              <span className="min-w-0">
                <strong>{c.label}</strong>
                {/* 路径完整显示，不截断 —— 这一行的用处就是让人看出自己指的到底是哪个目录。 */}
                <p className="notice break-all">{c.detail}</p>
              </span>
            </Row>
          ))}
        </Collapsible>
      )}
    </div>
  );
}
