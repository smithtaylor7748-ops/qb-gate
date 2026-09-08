import type { ComponentType } from 'react';
import type { PluginState } from '../lib/api';
import TavernPanel from './TavernPanel';

/**
 * 插件清单。
 *
 * 现在是内置数组；将来要做远程插件市场时，把这里换成 fetch 一个 index.json
 * 即可，调用方（Plugins.tsx）只认下面这个形状，不认具体插件。
 *
 * 后端 `plugin_list` 负责报状态与依赖检查，前端这份只补充「怎么展示」。
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
    id: 'sillytavern',
    name: '酒馆 SillyTavern',
    blurb:
      '启动真正的 SillyTavern 与 Claude 桥接，世界书、角色卡、群聊、扩展全部原样具备。面板负责启停、IP 门禁与资产备份。',
    panel: TavernPanel,
    upstream: {
      label: 'SillyTavern',
      url: 'https://github.com/SillyTavern/SillyTavern',
      license: 'AGPL-3.0',
    },
  },
];

export const STATE_LABEL: Record<PluginState, string> = {
  missing: '依赖不齐',
  ready: '就绪',
  running: '运行中',
  broken: '状态异常',
};

export const STATE_CLASS: Record<PluginState, string> = {
  missing: 'neutral',
  ready: 'ok',
  running: 'ok',
  broken: 'bad',
};
