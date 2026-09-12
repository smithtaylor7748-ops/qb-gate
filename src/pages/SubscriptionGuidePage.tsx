import { openUrl } from '@tauri-apps/plugin-opener';

import { useNav } from '../lib/nav';
import SubscriptionGuide from '../subscription-guide';

/**
 * 订阅引导。
 *
 * 这一页只是个**适配层**：真正的内容在 `src/subscription-guide/`，那份是外来模块
 * （MIT，见同目录 `LICENSE` 与 `ATTRIBUTION.md`），按原样拿进来的，**不要在里面
 * 改宿主相关的东西** —— 上游还会出新版本，改动越少越容易整目录换掉。
 * 宿主这边要适配什么，写在这个文件或 `styles/components.css` 里。
 *
 * 两个 prop 就是全部接口：
 *
 * - `openExternal` 传 opener 插件的 `openUrl`。WebView 里直接导航会把面板自己顶掉，
 *   跟 `ui/ExternalLink.tsx` 是同一个理由。模块自己还有一层来源域名白名单
 *   （`content.ts` 的 `isAllowedExternalUrl`），不在名单里的链接它自己就拦了，
 *   所以这里不需要额外再包一层。
 * - `onNavigate` 只会传 `accounts` / `environment` 两个值，用来跳回面板里对应的页。
 *
 * **这一页没有安全检查结果，所以没有进度步骤**：不加 `StepId`、不调 `mark()`、
 * 不动 Rust 那边的 `progress.json`（那五个 key 一个都不能多，见 `lib/steps.ts`）。
 * 模块内部那六步勾选只活在组件的内存里，切走或刷新就没了 —— 这是它自己的设计，
 * 不是缺陷：它记的是「这次读到哪儿了」，不是「这台机器检查通过了」。
 */
export default function SubscriptionGuidePage() {
  const { go } = useNav();
  return <SubscriptionGuide openExternal={openUrl} onNavigate={go} />;
}
