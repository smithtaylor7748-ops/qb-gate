/**
 * 真实浏览器采集页（2026-09-24，参照 CheckClaude 的 BrowserBridge，MIT，自己写）。
 *
 * # 为什么要有这一页
 *
 * 面板「中文环境」那十项是在面板**内置的 WebView2** 里算的 —— 那不是使用者登 claude.ai
 * 用的那个浏览器：语言列表、WebRTC 策略、扩展、字体渲染都可能不一样。面板点「用默认浏览器测」
 * 时，后端在 127.0.0.1 上临时开一个一次性的小服务（`qb-app::browser_probe`），用系统默认浏览器
 * 打开这一页；这一页跑**同一份** `signals.ts` 的采集，再把原始结果交回去。分数在面板里按同一张
 * 权重表算（`scanFrom`），这一页只管采。
 *
 * # 交回去什么
 *
 * 十项的原始值与单项分、WebRTC 候选地址、`navigator.languages`、时区、Intl 区域、UA 与 UA-CH。
 * `Accept-Language` 这类**请求头**由那个小服务自己记（页面读不到自己发出的请求头）。
 * 不交任何 cookie、历史、页面内容 —— 这一页本来也拿不到。
 */
import "./probe.css";
import { collectSignals, webrtcCandidates } from "../lib/signals";

type UaData = {
  platform?: string;
  brands?: { brand: string; version: string }[];
};

function uaData(): { platform: string | null; brands: string | null } {
  const d = (navigator as Navigator & { userAgentData?: UaData }).userAgentData;
  if (!d) return { platform: null, brands: null };
  const brands = (d.brands ?? [])
    .map((b) => `${b.brand} ${b.version}`)
    .join(", ");
  return { platform: d.platform ?? null, brands: brands || null };
}

async function main() {
  const status = document.getElementById("status");
  const say = (t: string) => {
    if (status) status.textContent = t;
  };
  try {
    const [signals, webrtc] = await Promise.all([
      collectSignals(),
      webrtcCandidates(),
    ]);
    const opts = Intl.DateTimeFormat().resolvedOptions();
    const ua = uaData();
    const body = JSON.stringify({
      v: 1,
      signals,
      webrtc,
      languages: [...navigator.languages],
      timezone: opts.timeZone,
      locale: opts.locale,
      ua: navigator.userAgent,
      ua_platform: ua.platform,
      brands: ua.brands,
    });
    // `./report` 相对于 `/<令牌>/`。text/plain 是「简单请求」，不会先发预检。
    const res = await fetch("./report", {
      method: "POST",
      headers: { "Content-Type": "text/plain;charset=UTF-8" },
      body,
    });
    say(
      res.ok
        ? "已完成，结果已经交回面板。可以关掉这个页面了。"
        : `交回面板失败（HTTP ${res.status}）：这个链接可能已经用过或过期了，回面板重新点一次。`,
    );
  } catch (e) {
    say(`采集失败：${e instanceof Error ? e.message : String(e)}`);
  }
}

void main();
