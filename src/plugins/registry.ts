import type { ComponentType } from "react";
import TavernPanel from "./TavernPanel";
import AntigravityPanel from "./AntigravityPanel";
import ClaudeZhPanel from "./ClaudeZhPanel";

/**
 * 插件清单。
 *
 * 现在是内置数组；将来远程清单只能来自配置的 GitHub 官方仓库并通过签名校验，
 * 调用方（Plugins.tsx）只认下面这个形状，不接受用户任意输入的下载地址。
 *
 * 后端 `plugin_list` 负责报状态与依赖检查，前端这份只补充「怎么展示」。
 * 状态的中文名在 `src/ui/labels.ts`，与其它后端枚举放在一起。
 */
export interface PluginMeta {
  id: string;
  name: string;
  /** 一句话说明，列表里显示。 */
  blurb: string;
  /** 详情面板，负责启停与该插件自己的设置。 */
  panel: ComponentType;
  /** 来源与许可，开源发布要能追溯。 */
  upstream?: { label: string; url: string; license: string };
}

export const PLUGINS: PluginMeta[] = [
  {
    id: "sillytavern",
    name: "酒馆 SillyTavern",
    blurb:
      "启动真正的 SillyTavern，世界书、角色卡、群聊、扩展全部原样具备。两条桥：Claude 走你自己的 bridge.py，GPT 走面板内置的桥接（驱动官方 codex exec）。面板负责启停、IP 门禁与资产备份。",
    panel: TavernPanel,
    upstream: {
      label: "SillyTavern",
      url: "https://github.com/SillyTavern/SillyTavern",
      license: "AGPL-3.0",
    },
  },
  {
    id: "antigravity-ui",
    name: "反重力 · 汉化与审批",
    blurb:
      "通过反重力 Hub 自己开着的调试协议往页面里注入脚本：界面汉化、审批卡自动点选、高危命令拦截。不起 Hub、不改它任何文件、不碰凭据。脚本、字典、规则取自 EasyAntigravity（MIT）。",
    panel: AntigravityPanel,
    upstream: {
      label: "EasyAntigravity",
      url: "https://github.com/DSDS-CMHL/EasyAntigravity",
      license: "MIT",
    },
  },
  {
    id: "claude-desktop-zh-cn",
    name: "Claude 桌面端 · 中文界面",
    blurb:
      "运行时取上游 claude-desktop-zh-cn 的 Release（打开时查新版，点了才下载），只调用它 Windows 脚本的安全模式：放翻译文件、改前端界面文字、设界面语言。不碰 app.asar / Claude.exe，改前改后核哈希与签名，动了就自动还原。",
    panel: ClaudeZhPanel,
    upstream: {
      label: "claude-desktop-zh-cn",
      url: "https://github.com/javaht/claude-desktop-zh-cn",
      license: "MIT",
    },
  },
];
