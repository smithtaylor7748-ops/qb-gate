import { describe, expect, it } from "vitest";

import {
  browserName,
  extraChecks,
  parsePage,
  primaryLang,
  scanFromReport,
} from "./browserProbe";
import type { BrowserReport } from "./generated/BrowserReport";

const CHROME =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";

function report(
  page: unknown,
  extra: Partial<BrowserReport> = {},
): BrowserReport {
  return {
    page_json: typeof page === "string" ? page : JSON.stringify(page),
    accept_language: "en-US,en;q=0.9",
    ch_platform: "Windows",
    user_agent: CHROME,
    received_at: "2026-09-24 03:59:00",
    received_unix: 1_790_000_000,
    ...extra,
  };
}

describe("真实浏览器交回来的报告", () => {
  // 任何能打开那个链接的页面都能往回交 —— 当不可信数据处理。
  it("认不出的项丢掉，形状不对的字段不进来", () => {
    const p = parsePage(
      JSON.stringify({
        signals: [
          { id: "timezone", raw: "America/New_York", score: 0 },
          { id: "not-a-signal", raw: "x", score: 1 },
          { id: "language", raw: 5, score: "high" },
          "garbage",
        ],
        webrtc: ["203.0.113.7", 42],
        languages: "en-US",
      }),
    );
    expect(p?.signals.map((s) => s.id)).toEqual(["timezone", "language"]);
    expect(p?.signals[1]).toEqual({ id: "language", raw: "", score: 0 });
    expect(p?.webrtc).toEqual({ status: "unknown", addresses: [] });
    expect(p?.languages).toEqual([]);
    expect(parsePage("not json")).toBeNull();
    expect(parsePage("null")).toBeNull();
  });

  it("分数按面板的权重表重算，交回来的分数夹在 0–1", () => {
    const scan = scanFromReport(
      report({
        signals: [
          { id: "fonts", raw: "6 款", score: 1 },
          { id: "timezone", raw: "Asia/Shanghai", score: 99 },
        ],
      }),
    );
    expect(scan?.source).toEqual({
      kind: "browser",
      browser: "Chrome 128",
      at: "2026-09-24 03:59:00",
    });
    // 时区 24 + 字体 14；旧报告的 WebRTC 未知，去掉其 10 分权重后折算。
    expect(scan?.total).toBe(42);
    expect(scan?.signals.find((s) => s.id === "language")?.raw).toBe(
      "没有采到",
    );
    expect(scanFromReport(report({ signals: [] }))).toBeNull();
  });

  it("旧报告和失败的 WebRTC 不会被旧的零风险分数伪装成成功", () => {
    for (const webrtc of [
      [],
      { status: "timeout", addresses: [] },
      { status: "error", addresses: ["203.0.113.7"] },
    ]) {
      const scan = scanFromReport(
        report({
          signals: [{ id: "webrtcLeak", raw: "未发现泄露", score: 0 }],
          webrtc,
        }),
      );
      expect(scan?.signals.find((s) => s.id === "webrtcLeak")?.verdict).toBe(
        "unknown",
      );
    }
    const scan = scanFromReport(
      report({
        signals: [{ id: "webrtcLeak", raw: "legacy", score: 1 }],
        webrtc: { status: "completed", addresses: [] },
      }),
    );
    expect(scan?.signals.find((s) => s.id === "webrtcLeak")?.verdict).toBe(
      "low",
    );
  });

  it("认浏览器", () => {
    expect(browserName(CHROME)).toBe("Chrome 128");
    expect(browserName(`${CHROME} Edg/128.0.1`)).toBe("Edge 128");
    expect(
      browserName(
        "Mozilla/5.0 (Windows NT 10.0; rv:130.0) Gecko/20100101 Firefox/130.0",
      ),
    ).toBe("Firefox 130");
    expect(browserName(null)).toBe("默认浏览器");
  });

  it("macOS and other matching UA platforms are accepted, and unknown platforms stay unknown", () => {
    for (const [platform, userAgent] of [
      [
        "macOS",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/128.0.0.0 Safari/537.36",
      ],
      ["Linux", "Mozilla/5.0 (X11; Linux x86_64) Chrome/128.0.0.0"],
      ["Android", "Mozilla/5.0 (Linux; Android 15) Chrome/128.0.0.0"],
      [
        "iOS",
        "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15",
      ],
    ]) {
      expect(
        extraChecks(
          report(
            { languages: ["en-US"] },
            { ch_platform: platform, user_agent: userAgent },
          ),
        )[1].state,
      ).toBe("pass");
    }
    expect(
      extraChecks(
        report(
          { languages: ["en-US"] },
          { ch_platform: "macOS", user_agent: "UnrecognizedBrowser/1" },
        ),
      )[1].state,
    ).toBe("unknown");
    expect(
      extraChecks(
        report({ languages: ["en-US"] }, { ch_platform: "future-os" }),
      )[1].state,
    ).toBe("unknown");
  });

  it("两条补充判定：请求头语言对页面语言、UA-CH 平台", () => {
    expect(primaryLang("zh-CN,zh;q=0.9")).toBe("zh");
    const ok = extraChecks(report({ languages: ["en-US", "en"] }));
    expect(ok.map((x) => x.state)).toEqual(["pass", "pass"]);
    const odd = extraChecks(
      report(
        { languages: ["zh-CN"] },
        { accept_language: "en-US,en;q=0.9", ch_platform: "macOS" },
      ),
    );
    expect(odd.map((x) => x.state)).toEqual(["warn", "warn"]);
    const firefox = extraChecks(
      report({ languages: ["en-US"] }, { ch_platform: null }),
    );
    expect(firefox[1].state).toBe("unknown");
  });
});
