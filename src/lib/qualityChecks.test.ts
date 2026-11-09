import { describe, expect, it } from "vitest";
import {
  animationPreviewDocument,
  inspectAnimationHtml,
  inspectCandyAnswer,
  loadQualityReports,
  plannedQualityRequests,
  sameQualityConditions,
  type QualityConditions,
} from "./qualityChecks";

describe("independent candy grading", () => {
  it("scores only the complete contracted final answer", () => {
    expect(inspectCandyAnswer(' { "answer": 21 } ').verdict).toBe("passed");
    expect(inspectCandyAnswer('{"answer":29}').verdict).toBe("failed");
    for (const text of [
      "不是 21，而是 29",
      "21",
      '{"answer":21.5}',
      '{"answer":"21"}',
      '{"answer":21,"answer":29}',
      '{"answer":29,"answer":21}',
      '{"answer":21,"reason":"not 21"}',
      '```json\n{"answer":21}\n```',
      '{"answer":21}\n{"answer":29}',
      '{"answer":NaN}',
    ])
      expect(inspectCandyAnswer(text).verdict).toBe("inconclusive");
    expect(inspectCandyAnswer('{"answer":-21}').verdict).toBe("failed");
  });
  it("independently checks the shape-choice bound without assuming the expected answer", () => {
    // Opposite fruit/shape pairs: round apple ↔ star peach; round peach ↔ star apple.
    const failures = new Set<string>();
    for (let ra = 0; ra <= 7; ra++)
      for (let rp = 0; rp <= 9; rp++)
        for (let rw = 0; rw <= 8; rw++)
          for (let sa = 0; sa <= 7; sa++)
            for (let sp = 0; sp <= 6; sp++)
              for (let sw = 0; sw <= 4; sw++)
                if (!(ra && sp) && !(rp && sa))
                  failures.add(`${ra + rp + rw},${sa + sp + sw}`);
    const guaranteed: number[] = [];
    for (let round = 0; round <= 24; round++)
      for (let star = 0; star <= 17; star++)
        if (!failures.has(`${round},${star}`)) guaranteed.push(round + star);
    expect(Math.min(...guaranteed)).toBe(21);
  });
});

const animation =
  '<!doctype html><html><head><style>@keyframes wave { to { transform: translateX(2px) } }</style></head><body><svg xmlns="http://www.w3.org/2000/svg"><defs><linearGradient id="beam"/></defs><path fill="url(#beam)" d="M0 0L1 1"/></svg></body></html>';
describe("untrusted animation preview", () => {
  it("accepts local SVG references and places restrictive CSP ahead of generated markup", () => {
    expect(inspectAnimationHtml(animation).ok).toBe(true);
    expect(
      inspectAnimationHtml(
        animation.replace(
          "<head>",
          '<head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">',
        ),
      ).ok,
    ).toBe(true);
    const preview = animationPreviewDocument(animation);
    expect(preview.indexOf("Content-Security-Policy")).toBeLessThan(
      preview.indexOf("<svg"),
    );
    expect(preview).toContain("default-src 'none'");
    expect(preview).toContain("form-action 'none'");
  });
  it("blocks scripts, external references, event handlers and navigation before preview", () => {
    for (const addition of [
      "<script>alert(1)</script>",
      '<img src="https://example.test/x">',
      '<meta http-equiv="refresh" content="0;https://example.test">',
      '<svg onload="alert(1)"></svg>',
      '<iframe srcdoc="hello"></iframe>',
      '<a href="https://example.test">visit</a>',
      '<style>@import "https://example.test/x.css";</style>',
      "<svg><foreignObject>foreign HTML</foreignObject></svg>",
    ]) {
      const text = animation.replace("</body>", `${addition}</body>`);
      expect(inspectAnimationHtml(text).ok).toBe(false);
      expect(() => animationPreviewDocument(text)).toThrow();
    }
    expect(inspectAnimationHtml("x".repeat(250_001)).html).toBeNull();
  });
});

describe("batch evidence", () => {
  const condition: QualityConditions = {
    targetId: "route:stable-id",
    targetLabel: "relay",
    model: "model",
    effort: "high",
    transport: "relay",
    protocol: "responses",
    sourceStamp: "process-salted-source-a",
    suite: "qb-quality-v2",
    fingerprintBank: "bank-v1",
    fingerprintReference: "model",
    methods: ["candy", "fingerprint"],
    rounds: 1,
  };
  it("compares stable identities, test settings and reference version rather than labels", () => {
    expect(
      sameQualityConditions(condition, {
        ...condition,
        sourceStamp: "process-salted-source-b",
      }),
    ).toBe(false);
    expect(
      sameQualityConditions(
        { ...condition, sourceStamp: null },
        { ...condition, sourceStamp: null },
      ),
    ).toBe(false);
    expect(
      sameQualityConditions(condition, {
        ...condition,
        protocol: "chat-completions",
      }),
    ).toBe(false);
    expect(
      sameQualityConditions(condition, {
        ...condition,
        targetLabel: "renamed",
      }),
    ).toBe(true);
    expect(
      sameQualityConditions(condition, {
        ...condition,
        targetId: "route:another",
      }),
    ).toBe(false);
    expect(
      sameQualityConditions(condition, {
        ...condition,
        fingerprintBank: "bank-v2",
      }),
    ).toBe(false);
    expect(sameQualityConditions(condition, { ...condition, rounds: 2 })).toBe(
      false,
    );
    expect(
      sameQualityConditions(condition, {
        ...condition,
        methods: ["fingerprint", "candy"],
      }),
    ).toBe(true);
  });
  it("estimates bounded requests without hidden retries", () => {
    expect(plannedQualityRequests(["candy"], 1)).toBe(1);
    expect(
      plannedQualityRequests(["candy", "fingerprint", "animation"], 2),
    ).toBe(10);
    expect(() => plannedQualityRequests(["fingerprint"], 6)).toThrow();
  });
  it("ignores corrupt or unsupported history without throwing", () => {
    for (const raw of ["not JSON", "{}", '[null,{}, {"version":1}]'])
      expect(loadQualityReports({ getItem: () => raw })).toEqual([]);
  });
  it("keeps older reports readable without treating their unknown source as comparable", () => {
    const { sourceStamp: _sourceStamp, ...oldConditions } = condition;
    const raw = JSON.stringify([
      {
        version: 2,
        id: "old",
        at: "2026-11-09",
        conditions: oldConditions,
        plannedRequests: 4,
        attemptedRequests: 0,
        state: "cancelled",
        samples: [],
        results: [],
      },
    ]);
    const reports = loadQualityReports({ getItem: () => raw });
    expect(reports).toHaveLength(1);
    expect(reports[0].conditions.sourceStamp).toBeNull();
    expect(sameQualityConditions(reports[0].conditions, condition)).toBe(false);
  });
});
