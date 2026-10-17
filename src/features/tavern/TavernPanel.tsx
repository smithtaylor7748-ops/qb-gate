/**
 * 账户页「酒馆」磁贴右上角那个弹窗：酒馆本身的控制面板（2026-09-29 第三轮）。
 *
 * # 怎么变成这样的
 *
 * - 2026-09-29：使用者要「将酒馆插件的设置页写入本项目的控制面板内……三种不同的厂商按账户页分开」，
 *   这个角标开的是这一家那条桥的设置（原 `BridgeSettings.tsx`），酒馆本体在软件页的 SillyTavern 卡。
 * - 同一天第二轮「前端完全重构」：控制台、角色卡、世界书、扩展、回收站做成了软件页那张卡的分页。
 *   他当时说「原位置啊……把控制面板接入了本项目」，指的其实是**这个弹窗**。
 * - 同一天第三轮，他：「错了错了，1弹窗太小，2这个应该在软件页面，软件页面的应该在这个官方账户弹窗页面。」于是对调：
 *   - 这里：概览 · 控制台 · 角色卡 · 世界书 · 扩展 · 备份与回收站，几乎占满窗口（`Modal size="full"`，高度固定）；
 *   - 软件页 SillyTavern 卡：下载安装 / 更新 / 指定版本 / 卸载、位置与端口（问他时选的「留在软件页」）；
 *   - 三条桥的设置：软件页各自那张 CLI 卡的最下面（`bridge/BridgeSection.tsx`，他选的「分别挂到各自的软件卡下」）。
 *
 * # ⛔ 别改回去
 *
 * 1. **三个账户页开的是同一个弹窗**：酒馆只有一份，三家共用。`provider` 只决定底部那颗「这一家桥接设置」去哪。
 * 2. **「停止酒馆」停的是整个酒馆和三条桥** —— 按钮旁边写明，不放悬停。起酒馆仍是磁贴本身。
 * 3. **这里没有桥接的字段**：一家的设置只在一处（软件页那张卡），`test:ui` 查着。
 * 4. **只在开着时挂正文**（`Modal` 关着时什么都不渲染）：关着的弹窗不读控制台、不列角色卡。
 */
import { useRef, useState, type KeyboardEvent } from "react";
import { ArrowRight, Download, Settings2, Square, Wine } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { api } from "../../lib/api";
import { AFTER, R } from "../../lib/resources";
import { invalidate, useResource } from "../../lib/store";
import { Button, Modal, useToast } from "../../ui";
import { BRIDGE_TITLE, bridgeHref, type BridgeProvider } from "./bridge/where";
import Characters from "./card/Characters";
import {
  TAVERN_TABS,
  errorText,
  rememberTab,
  rememberedTab,
  type TavernTab,
} from "./card/common";
import Console from "./card/Console";
import DataTab from "./card/DataTab";
import Extensions from "./card/Extensions";
import Overview from "./card/Overview";
import Worlds from "./card/Worlds";

export default function TavernPanel({
  open,
  onClose,
  provider,
}: {
  open: boolean;
  onClose: () => void;
  /** 从哪一家的账户页打开的 —— 只用在底部「这一家桥接设置」那颗按钮上。 */
  provider: BridgeProvider;
}) {
  return (
    <Modal
      open={open}
      onClose={onClose}
      icon={<Wine size={16} />}
      title="酒馆"
      size="full"
      footer={<PanelFooter provider={provider} onClose={onClose} />}
    >
      <PanelBody onClose={onClose} />
    </Modal>
  );
}

function PanelBody({ onClose }: { onClose: () => void }) {
  const setup = useResource("tavernSetup", R.tavernSetup);
  const tabs = useRef<HTMLDivElement>(null);
  const [tab, setTabState] = useState<TavernTab>(
    () => rememberedTab() ?? "overview",
  );
  const setTab = (t: TavernTab) => {
    setTabState(t);
    rememberTab(t);
  };

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

  return (
    <>
      {/* 钉在弹窗正文顶上：控制台、角色卡一长，往下滚时分页条不走。 */}
      <div
        ref={tabs}
        className="tv-tabs tv-panel-tabs"
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
        {tab === "overview" && (
          <Overview s={setup.data} onTab={setTab} onLeave={onClose} />
        )}
        {tab === "console" && <Console />}
        {tab === "characters" && <Characters />}
        {tab === "worlds" && <Worlds />}
        {tab === "extensions" && <Extensions />}
        {tab === "data" && <DataTab />}
      </div>
    </>
  );
}

/** 钉在弹窗底部的那一行：停止整个酒馆；下载与版本、这一家的桥接设置在软件页。 */
function PanelFooter({
  provider,
  onClose,
}: {
  provider: BridgeProvider;
  onClose: () => void;
}) {
  const toast = useToast();
  const navigate = useNavigate();
  const plugins = useResource("plugins", R.plugins);
  const cfg = useResource("tavernConfig", R.tavernConfig).data;
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
          停止酒馆
        </Button>
        {/* 代价常驻：停的是整个酒馆，不只是这一家那条桥。 */}
        <span className="notice">
          停止会关掉酒馆和三条桥，正在生成的回复会断。
        </span>
      </div>
      <div className="tv-panel-foot-group">
        <span className="notice">
          酒馆本体 127.0.0.1:{cfg?.st_port ?? 8000}，三家共用。在「软件」页：
        </span>
        <Button
          size="sm"
          icon={<Download size={12} />}
          onClick={() => go("/software?focus=sillytavern")}
        >
          下载与版本
        </Button>
        <Button
          size="sm"
          icon={<ArrowRight size={12} />}
          onClick={() => go(bridgeHref(provider))}
          data-testid="tavern-bridge-link"
        >
          {BRIDGE_TITLE[provider]}
        </Button>
      </div>
    </div>
  );
}

/**
 * 三个账户页的「酒馆」磁贴共用的那颗角标：打开上面这个弹窗。
 *
 * ⛔ 磁贴本身是一个 `<button>`，`button` 套 `button` 是非法 HTML ——
 * 所以它是 `Tile` 的 `corner`（兄弟节点），不是塞进磁贴里的。见 `Tile.tsx`。
 * 可访问名是「酒馆面板」，不叫「酒馆」：旁边那格磁贴自己就叫「酒馆」，两颗同名的按钮读屏分不开。
 */
export function TavernPanelCorner({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      title="酒馆面板：概览、控制台、角色卡、世界书、扩展、备份与回收站"
      aria-label="酒馆面板"
      data-testid="tavern-panel-corner"
      onClick={onClick}
    >
      <Settings2 size={13} />
    </button>
  );
}
