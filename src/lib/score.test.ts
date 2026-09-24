import { describe, expect, it } from "vitest";
import { computeScore, isLegacyDnsReport } from "./score";
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
      score: null,
      adapters_safe: true,
      adapters_note: "看了 WLAN 上配的 DNS：都走隧道。",
      findings: [],
      resolvers: [{ from_adapter: true }],
    } as unknown as DnsReport;
    const score = computeScore({
      progress: { steps: {}, completed_once: false },
      dns,
    });
    expect(score.items.find((i) => i.id === "dns")?.earned).toBeNull();
  });
  it("scores a Wi-Fi-only DNS report without asking for Ethernet", () => {
    // 2026-09-24：只连 Wi-Fi、没有有线网卡的机器也能拿满 —— 原来按名字找「以太网」。
    const dns = {
      score: 100,
      adapters_safe: true,
      adapters_note: "看了 WLAN 上配的 DNS：都走隧道。",
      findings: [],
      resolvers: [{ from_adapter: false }, { from_adapter: true }],
    } as unknown as DnsReport;
    const item = computeScore({
      progress: { steps: {}, completed_once: false },
      dns,
    }).items.find((i) => i.id === "dns");
    expect(item?.earned).toBe(20);
    expect(item?.detail).toBe("100 / 100 · 没有发现泄露");
  });
  it("keeps a stored DNS report from the old rules out of the total", () => {
    // 存盘的旧报告（按名字找「以太网」那一版评分）：分数不进总分，请使用者重测。
    const dns = {
      score: 50,
      ethernet_safe: null,
      findings: ["以太网 的 DNS 指向内网地址 172.18.0.2"],
      resolvers: [{ from_adapter: false }],
    } as unknown as DnsReport;
    expect(isLegacyDnsReport(dns)).toBe(true);
    const item = computeScore({
      progress: { steps: {}, completed_once: false },
      dns,
    }).items.find((i) => i.id === "dns");
    expect(item?.earned).toBeNull();
    expect(item?.detail).toContain("请重测一次");
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
  // 关键项一票否决（2026-09-24）：API 回 403 只占出口那 10 分里的一行，
  // 加权下来总分照样很高 —— 档位必须封顶，而且点名是哪一项。
  it("caps the band when a critical check fails, whatever the total", () => {
    const item = (id: string, state: "pass" | "fail") => ({
      id,
      label: id,
      state,
      detail: "",
      fixable: false,
      manual: null,
    });
    const egress = {
      items: [
        item("egress_consistency", "pass"),
        item("anthropic_reach", "fail"),
        item("claude_dns", "pass"),
      ],
      checked_at: "",
      undoable: [],
    };
    const r = computeScore({ progress, ip, egress });
    expect(r.critical).toEqual(["Anthropic 服务可达"]);
    expect(r.band).toBe("poor");
    expect(r.total).toBeGreaterThan(85);
    const ok = computeScore({
      progress,
      ip,
      egress: {
        ...egress,
        items: egress.items.map((i) => ({ ...i, state: "pass" as const })),
      },
    });
    expect(ok.critical).toEqual([]);
    expect(ok.band).toBe("good");
  });
});
