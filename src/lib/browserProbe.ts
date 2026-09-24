/**
 * 真实浏览器采集（2026-09-24）的前端一半：解析交回来的报告、认浏览器、两条补充判定。
 *
 * 后端那一半在 `qb-app::browser_probe`（一次性的 127.0.0.1 小服务）；采集页在 `src/probe/main.ts`。
 * 「中文环境」那十项的分数在这里用 `signals.ts` 的 `scanFrom` 算 —— **分数只在一个地方算**，
 * 采集页只管采。
 *
 * 交回来的 JSON 是**不可信数据**（任何能打开那个链接的页面都可以往回交）：
 * 逐项检查形状，认不出的丢掉，字符串截短，分数夹在 0–1（`scanFrom` 里再夹一次）。
 */
import type { BrowserReport } from "./generated/BrowserReport";
import { SIGNALS, scanFrom, type RawSignal, type ScanResult } from "./signals";

export interface ProbePage {
  signals: RawSignal[];
  webrtc: string[];
  languages: string[];
  timezone: string | null;
  locale: string | null;
  ua: string | null;
  ua_platform: string | null;
  brands: string | null;
}

const str = (x: unknown, max = 200): string | null =>
  typeof x === "string" ? x.slice(0, max) : null;
const strs = (x: unknown, count: number, max = 80): string[] =>
  Array.isArray(x)
    ? x
        .filter((s): s is string => typeof s === "string")
        .slice(0, count)
        .map((s) => s.slice(0, max))
    : [];

/** 解析采集页交回来的那段 JSON。形状不对就是 `null`。 */
export function parsePage(json: string): ProbePage | null {
  let v: unknown;
  try {
    v = JSON.parse(json);
  } catch {
    return null;
  }
  if (!v || typeof v !== "object") return null;
  const o = v as Record<string, unknown>;
  const ids = new Set<string>(SIGNALS.map((s) => s.id));
  const signals: RawSignal[] = Array.isArray(o.signals)
    ? o.signals.flatMap((s): RawSignal[] => {
        if (!s || typeof s !== "object") return [];
        const r = s as Record<string, unknown>;
        if (typeof r.id !== "string" || !ids.has(r.id)) return [];
        return [
          {
            id: r.id as RawSignal["id"],
            raw: str(r.raw) ?? "",
            score: typeof r.score === "number" ? r.score : 0,
          },
        ];
      })
    : [];
  return {
    signals,
    webrtc: strs(o.webrtc, 16, 64),
    languages: strs(o.languages, 16, 32),
    timezone: str(o.timezone, 64),
    locale: str(o.locale, 32),
    ua: str(o.ua, 300),
    ua_platform: str(o.ua_platform, 32),
    brands: str(o.brands, 200),
  };
}

/** 从 UA 认个浏览器名。跟 Rust 侧 `egress_checks::browser_name` 同一套规则。 */
export function browserName(ua: string | null | undefined): string {
  const s = ua ?? "";
  const ver = (key: string) => {
    const m = s.split(key)[1]?.match(/^\d+/);
    return m ? ` ${m[0]}` : "";
  };
  if (s.includes("Edg/")) return `Edge${ver("Edg/")}`;
  if (s.includes("Firefox/")) return `Firefox${ver("Firefox/")}`;
  if (s.includes("OPR/")) return `Opera${ver("OPR/")}`;
  if (s.includes("Chrome/")) return `Chrome${ver("Chrome/")}`;
  return "默认浏览器";
}

/** 「中文环境」这份分数是在哪量的。 */
export type ScanSource =
  { kind: "panel" } | { kind: "browser"; browser: string; at: string };

/** 报告 → 「中文环境」的得分，带上来源。十项一项都没交回来就是 `null`。 */
export function scanFromReport(
  r: BrowserReport,
): (ScanResult & { source: ScanSource }) | null {
  const page = parsePage(r.page_json);
  if (!page || page.signals.length === 0) return null;
  return {
    ...scanFrom(page.signals),
    source: {
      kind: "browser",
      browser: browserName(r.user_agent ?? page.ua),
      at: r.received_at,
    },
  };
}

export interface ExtraCheck {
  id: string;
  label: string;
  state: "pass" | "warn" | "unknown";
  detail: string;
}

/** `zh-CN,zh;q=0.9` → `zh`（首选那一个的主语言）。 */
export function primaryLang(list: string | null | undefined): string | null {
  const first = (list ?? "").split(",")[0]?.split(";")[0]?.trim();
  return first ? first.split("-")[0].toLowerCase() : null;
}

/**
 * 两条不进 100 分的补充（参照 CheckClaude 的「HTTP 语言首标」「Client Hints」）：
 *
 * - 请求头里的 `Accept-Language` 首选，跟页面里 `navigator.languages` 首选是不是同一个语言 ——
 *   不一样通常是装了改请求头的扩展，网站两边一对就对出矛盾；
 * - UA-CH 报的平台是不是 Windows —— 不是的话多半是改 UA 的扩展。
 *
 * 只报告：它们说明的是「浏览器里装了什么」，跟出口无关，也不该动 FuckClaude 那张权重表。
 */
export function extraChecks(r: BrowserReport): ExtraCheck[] {
  const page = parsePage(r.page_json);
  const out: ExtraCheck[] = [];
  const header = primaryLang(r.accept_language);
  const js = primaryLang(page?.languages.join(","));
  out.push(
    !header || !js
      ? {
          id: "accept_language",
          label: "请求头语言",
          state: "unknown",
          detail: "这个浏览器没交回请求头语言或页面语言，比不了。",
        }
      : header === js
        ? {
            id: "accept_language",
            label: "请求头语言",
            state: "pass",
            detail: `请求头 Accept-Language（${r.accept_language}）跟页面里的首选语言一致。`,
          }
        : {
            id: "accept_language",
            label: "请求头语言",
            state: "warn",
            detail: `请求头里首选的是 ${header}，页面里首选的是 ${js}：两边对不上，多半是装了改请求头的扩展。`,
          },
  );
  const platform = r.ch_platform ?? page?.ua_platform ?? null;
  out.push(
    !platform
      ? {
          id: "client_hints",
          label: "Client Hints",
          state: "unknown",
          detail:
            "这个浏览器不发 UA-CH（Firefox、Safari 就不发），这一项不适用。",
        }
      : platform.toLowerCase().includes("windows")
        ? {
            id: "client_hints",
            label: "Client Hints",
            state: "pass",
            detail: `UA-CH 报的平台是 ${platform}，跟这台机器一致。`,
          }
        : {
            id: "client_hints",
            label: "Client Hints",
            state: "warn",
            detail: `UA-CH 报的平台是 ${platform}，不是 Windows：多半是改 UA 的扩展，网站两边一对就对出矛盾。`,
          },
  );
  return out;
}
