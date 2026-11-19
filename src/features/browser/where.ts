/**
 * 浏览器助手页的地址与叫法。快速跳转、扩展页的卡片、三个旧入口（中文环境 / 执行锁小窗、查套路）
 * 只引这里，不引整页 —— 免得把整页拖进首页那一包。
 *
 * ⛔ 插件里写死了这几个叫法（`browser-extension/core.js`、`welcome.html`、`README.md`）：
 * 「扩展 → 浏览器助手」「连接浏览器」「一键重连」「打开插件文件夹」。改这里要连插件一起改。
 */
export const BROWSER_COMPANION_ID = "browser-companion";
export const BROWSER_COMPANION_PATH = "/extensions/" + BROWSER_COMPANION_ID;
export const BROWSER_COMPANION_NAME = "浏览器助手";
/** 卡片与快速跳转里那一句。 */
export const BROWSER_COMPANION_BLURB =
  "Chrome / Edge 插件：面板没确认出口 IP 时拦下受保护的网站（默认 Claude 官方网页），其他网站照常。";
/** 插件默认保护的网站（各连同子域名）。名单只在插件里，面板看不到改过之后的样子。 */
export const DEFAULT_PROTECTED_SITES = [
  "claude.ai",
  "claude.com",
  "anthropic.com",
  "claudeusercontent.com",
];
