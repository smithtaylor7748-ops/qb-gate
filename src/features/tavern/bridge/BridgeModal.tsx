/**
 * 账户页「酒馆」磁贴右上角那颗按钮打开的弹窗：**只装这一家**那条桥的设置（2026-09-29 第二轮）。
 *
 * | 账户页 | 正文 |
 * |---|---|
 * | Claude | [`ClaudeBridge`]：连接（bridge.py 目录可自动定位、端口）· 七项 · SDK 会话 · 消息监控（逐条展开输入 / 输出） |
 * | GPT | [`CliBridge`] `provider="gpt"` |
 * | 反重力 | [`CliBridge`] `provider="gemini"` |
 *
 * 由来：使用者「将酒馆插件的设置页写入本项目的控制面板内……三种不同的厂商按账户页分开」（第一轮）；
 * 第二轮「前端完全重构」时正文拆进 `bridge/`，顶上钉一排「跳到」。酒馆本体（控制台、角色卡……）在软件页
 * SillyTavern 卡的分页里。
 *
 * # ⛔ 四条
 *
 * 1. **不分页**：顶上是一排「跳到」（`SectionNav`）——分页会把「别家的字段一项都不许出现」那条检查（test:ui）
 *    变成只查当前那一页。
 * 2. **一家只装一家**：别家的字段一项都不许出现。
 * 3. **「停止酒馆」停的是整个酒馆和三条桥** —— 按钮上写明，不放悬停。起酒馆仍是磁贴本身。
 * 4. **只在开着时挂正文**（`Modal` 关着时什么都不渲染）：关着的弹窗不探 bridge.py、不读额度。
 */
import { useEffect, useState } from "react";
import {
  ArrowRight,
  Settings2,
  Square,
  TerminalSquare,
  Wine,
} from "lucide-react";
import { useNavigate, useSearchParams } from "react-router-dom";

import { api } from "../../../lib/api";
import { AFTER, R } from "../../../lib/resources";
import { invalidate, useResource } from "../../../lib/store";
import { Button, Modal, useToast } from "../../../ui";
import { errorText } from "../card/common";
import ClaudeBridge from "./ClaudeBridge";
import CliBridge from "./CliBridge";
import { BRIDGE_TITLE, type BridgeProvider } from "./where";

export default function BridgeModal({
  open,
  onClose,
  provider,
}: {
  open: boolean;
  onClose: () => void;
  provider: BridgeProvider;
}) {
  return (
    <Modal
      open={open}
      onClose={onClose}
      icon={<Wine size={16} />}
      title={BRIDGE_TITLE[provider]}
      size="huge"
      footer={<Footer onClose={onClose} />}
    >
      {provider === "claude" ? (
        <ClaudeBridge />
      ) : (
        <CliBridge provider={provider} />
      )}
    </Modal>
  );
}

/** 底部：停止酒馆（三条桥一起停）· 看酒馆控制台 · 到软件页。 */
function Footer({ onClose }: { onClose: () => void }) {
  const toast = useToast();
  const navigate = useNavigate();
  const plugins = useResource("plugins", R.plugins);
  const tavern = plugins.data?.find((p) => p.id === "sillytavern");
  // 只起来一半（broken）也要能停 —— 那正是最需要收拾的时候。
  const up = tavern?.state === "running" || tavern?.state === "broken";
  const [stopping, setStopping] = useState(false);

  async function stop() {
    setStopping(true);
    try {
      const done = await api.pluginStop();
      invalidate(...AFTER.tavern);
      toast.ok(done.length ? done.join("；") : "酒馆没在跑");
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setStopping(false);
    }
  }

  function go(to: string) {
    onClose();
    navigate(to);
  }

  return (
    <div className="tv-panel-foot">
      <div className="tv-panel-foot-group">
        <Button
          size="sm"
          variant="danger"
          icon={<Square size={12} />}
          loading={stopping}
          disabled={!up || stopping}
          onClick={() => void stop()}
          data-testid="tavern-stop"
        >
          停止酒馆（所有桥接一起停）
        </Button>
      </div>
      <div className="tv-panel-foot-group">
        <Button
          size="sm"
          icon={<TerminalSquare size={12} />}
          onClick={() => go("/software?focus=sillytavern&tab=console")}
          data-testid="tavern-console-link"
        >
          看酒馆控制台
        </Button>
        <Button
          size="sm"
          icon={<ArrowRight size={12} />}
          onClick={() => go("/software?focus=sillytavern")}
        >
          到软件页
        </Button>
      </div>
    </div>
  );
}

/**
 * 三个账户页「酒馆」磁贴共用的那颗角标：打开这一家的桥接设置。
 *
 * ⛔ 磁贴本身是一个 `<button>`，`button` 套 `button` 是非法 HTML ——
 * 所以它是 `Tile` 的 `corner`（兄弟节点），不是塞进磁贴里的。见 `Tile.tsx`。
 */
export function BridgeCorner({
  provider,
  onClick,
}: {
  provider: BridgeProvider;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      title={BRIDGE_TITLE[provider]}
      aria-label={BRIDGE_TITLE[provider]}
      data-testid="tavern-bridge-corner"
      onClick={onClick}
    >
      <Settings2 size={13} />
    </button>
  );
}

/** 带 `?bridge=<这一家>` 进账户页（快速跳转、SillyTavern 卡的「概览」）：打开弹窗，再把参数去掉。 */
export function useBridgeLink(
  provider: BridgeProvider,
  setOpen: (v: boolean) => void,
) {
  const [params, setParams] = useSearchParams();
  useEffect(() => {
    if (params.get("bridge") !== provider) return;
    setOpen(true);
    const next = new URLSearchParams(params);
    next.delete("bridge");
    setParams(next, { replace: true });
  }, [params, provider, setOpen, setParams]);
}
