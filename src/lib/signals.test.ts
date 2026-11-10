// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  iceCandidateAddress,
  normalizeIceAddress,
  parseWebRtcReport,
  scanFrom,
  SIGNALS,
  WEBRTC_TIMEOUT_MS,
  webrtcCandidates,
  webrtcOutcome,
} from "./signals";

class Peer {
  static last: Peer;
  onicecandidate:
    | ((event: {
        candidate: { candidate: string; address?: string } | null;
      }) => void)
    | null = null;
  onicecandidateerror: (() => void) | null = null;
  onicegatheringstatechange: (() => void) | null = null;
  iceGatheringState = "new";
  close = vi.fn();
  createDataChannel = vi.fn();
  createOffer = vi.fn(async () => ({}));
  setLocalDescription = vi.fn(async () => undefined);
  constructor() {
    Peer.last = this;
  }
  candidate(address: string, kind = "srflx") {
    this.onicecandidate?.({
      candidate: {
        candidate: `candidate:7 1 udp 123 ${address} 54321 typ ${kind} raddr 10.0.0.1 rport 9`,
      },
    });
  }
  complete() {
    this.onicecandidate?.({ candidate: null });
  }
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("RTCPeerConnection", Peer);
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("WebRTC collection outcomes", () => {
  it("distinguishes unsupported, exception, and timeout from a completed empty result", async () => {
    vi.stubGlobal("RTCPeerConnection", undefined);
    expect(await webrtcCandidates()).toEqual({
      status: "unsupported",
      addresses: [],
    });
    vi.stubGlobal(
      "RTCPeerConnection",
      class {
        constructor() {
          throw Error("unavailable");
        }
      },
    );
    expect(await webrtcCandidates()).toEqual({
      status: "error",
      addresses: [],
    });
    vi.stubGlobal("RTCPeerConnection", Peer);
    const pending = webrtcCandidates();
    await vi.advanceTimersByTimeAsync(WEBRTC_TIMEOUT_MS);
    expect(await pending).toEqual({ status: "timeout", addresses: [] });
    expect(Peer.last.close).toHaveBeenCalledOnce();
    const completed = webrtcCandidates();
    Peer.last.complete();
    expect(await completed).toEqual({ status: "completed", addresses: [] });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("retains partial observations without passing a timed out probe or accepting late candidates", async () => {
    const pending = webrtcCandidates();
    Peer.last.candidate("2001:db8::7");
    await vi.advanceTimersByTimeAsync(WEBRTC_TIMEOUT_MS);
    const result = await pending;
    Peer.last.candidate("198.51.100.8");
    Peer.last.complete();
    expect(result).toEqual({ status: "timeout", addresses: ["2001:db8::7"] });
    expect(webrtcOutcome(result).status).toBe("timeout");
  });

  it("reports a failed STUN exchange instead of treating its host candidates as safe", async () => {
    const pending = webrtcCandidates();
    Peer.last.candidate("192.168.1.2", "host");
    Peer.last.onicecandidateerror?.();
    Peer.last.complete();
    expect((await pending).status).toBe("error");
  });

  it("accepts a successful STUN observation despite an error from another interface", async () => {
    const pending = webrtcCandidates();
    Peer.last.onicecandidateerror?.();
    Peer.last.candidate("2001:db8::7");
    Peer.last.candidate("2001:0db8:0000:0000:0000:0000:0000:0007");
    Peer.last.iceGatheringState = "complete";
    Peer.last.onicegatheringstatechange?.();
    expect(await pending).toEqual({
      status: "completed",
      addresses: ["2001:db8::7"],
    });
  });

  it("reports offer failures and closes the peer", async () => {
    vi.stubGlobal(
      "RTCPeerConnection",
      class extends Peer {
        createOffer = vi.fn(async () => {
          throw Error("offer failed");
        });
      },
    );
    expect((await webrtcCandidates()).status).toBe("error");
    expect(Peer.last.close).toHaveBeenCalledOnce();
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe("ICE addresses and report validity", () => {
  it("reads the actual candidate address, including compressed IPv6 and IPv4 mapped IPv6", () => {
    expect(
      iceCandidateAddress({
        candidate:
          "candidate:203.0.113.9 1 UDP 123 2001:db8::7 456 typ srflx raddr 10.0.0.1 rport 9",
      }),
    ).toBe("2001:db8::7");
    expect(normalizeIceAddress("::ffff:203.0.113.7")).toBe("::ffff:cb00:7107");
    expect(
      iceCandidateAddress({
        address: "203.0.113.8",
        candidate: "candidate:7 1 udp 123 203.0.113.9 456 typ srflx",
      }),
    ).toBe("203.0.113.8");
    expect(
      iceCandidateAddress({
        candidate:
          "candidate:7 1 udp 123 device.local 456 typ host raddr 10.0.0.1 rport 9",
      }),
    ).toBeNull();
    for (const bad of [
      "999.1.1.1",
      "1.2.3",
      "1.2.3.04",
      "2001:db8:::7",
      "fe80::1%eth0",
      "example.com",
    ])
      expect(normalizeIceAddress(bad)).toBeNull();
  });

  it("rejects legacy, malformed, and oversized reports instead of passing empty arrays", () => {
    for (const value of [
      [],
      ["203.0.113.7"],
      {},
      { status: "completed" },
      { status: "completed", addresses: [5] },
      { status: "completed", addresses: ["bad"] },
      { status: "completed", addresses: Array(65).fill("203.0.113.7") },
    ]) {
      expect(parseWebRtcReport(value).status).toBe("unknown");
    }
    expect(parseWebRtcReport({ status: "completed", addresses: [] })).toEqual({
      status: "completed",
      addresses: [],
    });
  });

  it("removes unfinished WebRTC from the score denominator and marks its row unknown", () => {
    const raw = SIGNALS.map((s) => ({
      id: s.id,
      raw: "fixture",
      score: s.id === "timezone" ? 1 : 0,
    }));
    const scan = scanFrom(raw);
    expect(scan.total).toBe(27); // 24 / (100 - 10), not a false zero-risk WebRTC contribution.
    expect(scan.signals.find((s) => s.id === "webrtcLeak")?.verdict).toBe(
      "unknown",
    );
    const completed = scanFrom(
      raw.map((r) =>
        r.id === "webrtcLeak" ? { ...r, status: "completed" as const } : r,
      ),
    );
    expect(completed.total).toBe(24);
  });
});
