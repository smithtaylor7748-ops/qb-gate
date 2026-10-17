/**
 * Claude 桥接（使用者自己的 `bridge.py`）的全部设置 —— 软件页 Claude Code 卡最下面「酒馆 · Claude 桥接设置」
 * 展开之后的正文（外壳是 `BridgeSection.tsx`）。
 *
 * # 由来
 *
 * 这些旋钮和那张调用日志表**不是面板的东西**：它们是 `bridge.py` 起的 `GET /settings` 与 `GET /monitor`
 * 两个本机网页。0.32.0 搬进扩展中心的酒馆页；2026-09-29 使用者「将酒馆插件的设置页写入本项目的控制面板内，
 * 而不是用打开本地链接的形式，三种不同的厂商按账户页分开」，搬进了「官方账户 · Claude」酒馆磁贴右上角的弹窗，
 * 并补齐了 SDK 会话与逐条全文。同一天第二轮使用者要「前端完全重构」：拆成四段（`claude/` 下各一个文件），
 * 顶上加一排「跳到」，提示词数字数、监控能只看没成功的。第三轮使用者说「这个应该在软件页面」，
 * 整块挪到软件页（那个弹窗换成了酒馆本身的控制面板），组件内部没变。
 *
 * # ⛔ 四条（没变）
 *
 * 1. **桥没在跑 / 没有这个接口时，照实说后端那句话**（`tavern_bridge_api::explain` 已经写好了能照着做的话），
 *    不把空表显示成 0 条、不把读不到显示成没有（§7.17）。
 * 2. **改设置会退掉当前那条 SDK 会话** —— 代价常驻在保存按钮旁边。
 * 3. **清空日志不可逆**，走确认框。
 * 4. **全文只在点「展开」时读**，不随列表一起拉。
 */
import { useCallback, useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";

import { api } from "../../../lib/api";
import type { BridgeHealth } from "../../../lib/generated/BridgeHealth";
import { Button, Pill } from "../../../ui";
import { errorText } from "../card/common";
import Connection from "./claude/Connection";
import Knobs from "./claude/Knobs";
import Monitor from "./claude/Monitor";
import Sessions from "./claude/Sessions";
import SectionNav from "./SectionNav";

const IDS = {
  connection: "bridge-claude-connection",
  knobs: "bridge-claude-knobs",
  sessions: "bridge-claude-sessions",
  monitor: "bridge-claude-monitor",
};

export default function ClaudeBridge() {
  const [health, setHealth] = useState<BridgeHealth | null>(null);
  const [healthErr, setHealthErr] = useState("");
  const [probing, setProbing] = useState(false);
  const [round, setRound] = useState(0);

  const probe = useCallback(async () => {
    setProbing(true);
    try {
      setHealth(await api.tavernBridgeHealth());
      setHealthErr("");
    } catch (e) {
      setHealth(null);
      setHealthErr(errorText(e));
    } finally {
      setProbing(false);
    }
  }, []);
  useEffect(() => {
    void probe();
  }, [probe]);

  return (
    <div className="bridge-claude" data-provider="claude">
      <div className="bridge-head">
        <div className="bridge-claude-status">
          <Pill tone={health ? "ok" : "default"}>
            {probing && !health && !healthErr
              ? "探测中"
              : health
                ? "在跑"
                : "没连上"}
          </Pill>
          <span className="bridgelist-sub">
            {health
              ? `bridge.py v${health.bridge_version || "?"} · ${
                  health.invocation_mode === "agent_sdk" ? "Agent SDK" : "CLI"
                } 模式 · ${health.model || "?"} · 思考 ${health.effort || "?"}`
              : "你自己那份 bridge.py（驱动官方 claude），面板不分发它。"}
          </span>
          <Button
            size="sm"
            icon={<RefreshCw size={12} />}
            loading={probing}
            className="ml-auto"
            onClick={() => {
              void probe();
              setRound((r) => r + 1);
            }}
          >
            重新读取
          </Button>
        </div>
        <SectionNav
          sections={[
            { id: IDS.connection, label: "连接" },
            { id: IDS.knobs, label: "桥接设置" },
            ...(health ? [{ id: IDS.sessions, label: "会话" }] : []),
            { id: IDS.monitor, label: "消息监控" },
          ]}
        />
      </div>
      {healthErr && (
        <p role="alert" className="notice notice--danger mt-2">
          {healthErr}
        </p>
      )}

      <Connection id={IDS.connection} />
      {/* `key` 换一次 = 整块重读一次（「重新读取」）。 */}
      <Knobs key={`knobs-${round}`} id={IDS.knobs} />
      <Sessions id={IDS.sessions} health={health} />
      <Monitor key={`monitor-${round}`} id={IDS.monitor} />
    </div>
  );
}
