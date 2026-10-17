/**
 * 软件页上「酒馆 · X 桥接设置」那一段：挂在这条桥驱动的那个 CLI 的卡片最下面（2026-09-29 第三轮，使用者选的）。
 *
 * | 桥 | 挂在哪张卡 | 正文 |
 * |---|---|---|
 * | Claude（你自己的 bridge.py，默认 5001） | Claude Code | [`ClaudeBridge`]：连接 · 七项 · 会话 · 消息监控 |
 * | GPT（面板内置，默认 5002） | Codex CLI | [`CliBridge`] `provider="gpt"` |
 * | Gemini（面板内置，默认 5003） | Gemini CLI | [`CliBridge`] `provider="gemini"` |
 *
 * 同一天第一、二轮它们在三个账户页「酒馆」磁贴右上角的弹窗里。第三轮使用者：「错了错了，1弹窗太小，
 * 2这个应该在软件页面，软件页面的应该在这个官方账户弹窗页面」—— 那个弹窗换成了酒馆本身的控制面板
 * （`TavernPanel.tsx`），桥接设置搬到这里；放哪张卡是问他时选的（「分别挂到各自的软件卡下」）。
 *
 * # ⛔ 三条
 *
 * 1. **默认收起，展开了才挂正文。** 没用 `ui/Collapsible`：它把正文渲染成 `hidden`，等于每次进软件页都去探
 *    bridge.py、读消息监控、读额度 —— 而人多半是来装软件的。从 `?focus=bridge-<家>` 进来（账户页磁贴「还缺东西」、
 *    酒馆弹窗底部、快速跳转）时自动展开并滚到这里。
 * 2. **标题行里不放 `.pill`**：`test:ui`「Pill 与版本行说同一件事」读整张卡的全部 `.pill`。在跑 / 没在跑在展开后的正文里。
 * 3. **一家的字段只在一处**：这条桥的设置只在这里，酒馆弹窗里一个都没有（`test:ui` 查着）。
 */
import { useEffect, useId, useRef, useState } from "react";
import { ChevronRight, Wine } from "lucide-react";
import { useSearchParams } from "react-router-dom";

import ClaudeBridge from "./ClaudeBridge";
import CliBridge from "./CliBridge";
import { BRIDGE_TITLE, type BridgeProvider } from "./where";

/** 收起时标题下面那一行：它是谁的、驱动什么、展开能改什么。 */
const SUMMARY: Record<BridgeProvider, string> = {
  claude:
    "你自己的 bridge.py，驱动官方 claude · 默认端口 5001 · 连接、七项设置、会话、消息监控",
  gpt: "面板内置，驱动官方 codex exec · 默认端口 5002 · 端口、模型、提示词、额度、链路测试",
  gemini:
    "面板内置，驱动官方 Gemini CLI · 默认端口 5003 · 端口、模型、提示词、额度、链路测试",
};

export default function BridgeSection({
  provider,
}: {
  provider: BridgeProvider;
}) {
  const [params] = useSearchParams();
  const anchor = useRef<HTMLElement>(null);
  const bodyId = useId();
  const focused = params.get("focus") === `bridge-${provider}`;
  const [open, setOpen] = useState(focused);

  // 从账户页、酒馆弹窗、快速跳转进来：展开并滚到这一段。
  useEffect(() => {
    if (!focused) return;
    setOpen(true);
    anchor.current?.scrollIntoView({ block: "start" });
  }, [focused, params]);

  return (
    // 类名叫 `bridge-dock`：`.bridge-section` 已经是 Claude 那份正文里各段（连接、七项、会话、监控）的样式。
    <section
      ref={anchor}
      id={`bridge-${provider}`}
      className="bridge-dock scroll-mt-4"
      data-testid={`bridge-section-${provider}`}
    >
      <button
        type="button"
        className="bridge-dock-toggle"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={() => setOpen((v) => !v)}
        data-testid={`bridge-toggle-${provider}`}
      >
        <ChevronRight
          size={13}
          className="collapsible-chevron"
          aria-hidden="true"
        />
        <Wine size={14} aria-hidden="true" className="bridge-dock-icon" />
        <span className="bridge-dock-title">
          酒馆 · {BRIDGE_TITLE[provider]}
        </span>
        <span className="bridge-dock-sub">{SUMMARY[provider]}</span>
      </button>
      <div id={bodyId} className="bridge-dock-body" hidden={!open}>
        {open &&
          (provider === "claude" ? (
            <ClaudeBridge />
          ) : (
            <CliBridge provider={provider} />
          ))}
      </div>
    </section>
  );
}
