import { describe, expect, it } from "vitest";
import { computeScore } from "./score";
import type { DnsReport, GateStatus, IpInfo, Progress } from "./api";

const progress: Progress = {
  completed_once: true,
  steps: {
    purity: {
      state: "passed",
      risk: "low",
      detail: "已人工确认两家均通过（203.0.113.7）",
      updated_at: "2026-09-16",
    },
  },
};
const ip = { ip: "203.0.113.7" } as IpInfo;
describe("evidence behind the score", () => {
  it("counts a successful IP self-test separately from a manual score", () => {
    const score = computeScore({
      progress: { completed_once: false, steps: {} },
      ip: { ...ip, fraudScore: 3, isResidential: true },
    });
    expect(score.measured).toBe(1);
    expect(score.assessed).toBe(0);
    expect(score.items[0].earned).toBeNull();
    expect(score.items[0].detail).toBe("自测完成，待人工复核");
  });
  it("requires legacy confirmations without an IP to be reviewed without calling the self-test broken", () => {
    const legacy: Progress = {
      ...progress,
      steps: {
        purity: {
          ...progress.steps.purity!,
          detail: "已在 IPQS 与 ippure 复核通过",
        },
      },
    };
    const score = computeScore({ progress: legacy, ip });
    expect(score.measured).toBe(1);
    expect(score.items[0].earned).toBeNull();
    expect(score.items[0].detail).toBe("自测完成，请复核当前 IP");
  });
  it("does not score stale IP data when a new request has failed", () => {
    const score = computeScore({ progress, ip, ipError: "HTTP 503" });
    expect(score.measured).toBe(0);
    expect(score.items[0].earned).toBeNull();
    expect(score.items[0].detail).toContain("自测失败");
  });
  it("does not reuse an old IP verdict for an unknown or changed exit", () => {
    expect(computeScore({ progress }).items[0].earned).toBeNull();
    expect(
      computeScore({ progress, ip: { ...ip, ip: "203.0.113.8" } }).items[0]
        .earned,
    ).toBeNull();
    expect(computeScore({ progress, ip }).items[0].earned).toBe(30);
  });
  it("does not treat DNS adapter settings as a real resolver echo", () => {
    const dns = {
      score: 100,
      ethernet_safe: true,
      findings: [],
      resolvers: [{ from_adapter: true }],
    } as unknown as DnsReport;
    const score = computeScore({
      progress: { steps: {}, completed_once: false },
      dns,
    });
    expect(score.items.find((i) => i.id === "dns")?.earned).toBeNull();
  });
  it("does not hide unprotected copies behind a valid lease", () => {
    const gate = {
      targets: [{ locked: false }],
      lease: { holders: { session: "cli" } },
      stale_copies: ["stray.exe"],
      allowlist: ["203.0.113.7"],
    } as unknown as GateStatus;
    expect(
      computeScore({ progress, gate }).items.find((i) => i.id === "iplock")
        ?.earned,
    ).toBeLessThan(20);
  });
  it("keeps all-unknown reports unscored", () => {
    const r = computeScore({
      progress: { steps: {}, completed_once: false },
      egress: {
        items: [
          {
            id: "test",
            label: "test",
            state: "unknown",
            detail: "unavailable",
            fixable: false,
            manual: null,
          },
        ],
        checked_at: "",
        undoable: [],
      },
    });
    expect(r.total).toBeNull();
    expect(r.missing).toBe(5);
  });
});
