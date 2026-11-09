// @vitest-environment node
import { webcrypto } from "node:crypto";
import { readFile } from "node:fs/promises";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import oracle from "./__fixtures__/lmfpd-sdk-0.2.0-oracle.json";
import bundle from "../vendor/lmfpd/bundle.json";
import { analyzeOutputs } from "../vendor/lmfpd/analysis";
import { decodeDetector } from "../vendor/lmfpd/detector-codec";
import type { Bank } from "../vendor/lmfpd/types";

const binary = new Uint8Array(
  await readFile(
    new URL(
      "../../public/quality-reference/lmfpd-0.2.0/shared_detector.bin",
      import.meta.url,
    ),
  ),
);
const full = oracle.cases.find((scenario) => scenario.name === "calibrated")!;

function localFetch(data = binary) {
  return vi.fn(async () => new Response(Uint8Array.from(data).buffer));
}

beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal("crypto", webcrypto);
  vi.stubGlobal("fetch", localFetch());
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
  vi.doUnmock("../vendor/lmfpd/bundle.json");
});

describe("pinned lmfpd SDK conformance", () => {
  it("matches original npm SDK scores, probabilities, predictions and diagnostics", () => {
    const detector = decodeDetector(binary);
    for (const scenario of oracle.cases) {
      // These expected results were produced by the untouched npm SDK using
      // its local-directory loader, with fetch forbidden. All inputs are synthetic.
      const actual = analyzeOutputs(
        scenario.outputs,
        bundle as unknown as Bank,
        detector,
        false,
      );
      expect(actual, scenario.name).toEqual(scenario.analysis);
    }
  });

  it("keeps the original SDK's multilingual prompts and selection rules with a local seed", async () => {
    const api = await import("./qualityFingerprint");
    const prompts = api.createFingerprintPrompts(oracle.seed);
    expect(prompts).toEqual(oracle.challenges);
    expect(api.createFingerprintPrompts(oracle.seed)).toEqual(prompts);
    expect(api.createFingerprintPrompts("a different run")).not.toEqual(
      prompts,
    );
    expect(new Set(prompts.map((prompt) => prompt.expected_count)).size).toBe(
      3,
    );
    expect(
      prompts.every(
        (prompt) =>
          prompt.expected_count >= 292 && prompt.expected_count <= 332,
      ),
    ).toBe(true);
    expect(globalThis.crypto).toBe(webcrypto);
    expect(fetch).not.toHaveBeenCalled();
  });
});

describe("browser-local fingerprint reference", () => {
  it("ships the fixed catalog, including the explicitly merged GPT reference", async () => {
    const api = await import("./qualityFingerprint");
    const catalog = api.getFingerprintCatalog();
    expect(catalog.referenceSha256).toBe(oracle.referenceSha256);
    expect(catalog.sdkVersion).toBe("0.2.0");
    expect(catalog.dataVersion).toBe("0.0.1793708131735");
    expect(catalog.models).toHaveLength(66);
    expect(
      catalog.models.find((model) => model.id === "gpt-6-astra"),
    ).toMatchObject({
      label: "gpt-6-astra/6.1-sol",
      aliases: ["gpt-6-astra", "gpt-6.1-sol"],
      merged: true,
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it("loads only the app-local asset, verifies it and shares concurrent reads", async () => {
    vi.stubEnv("BASE_URL", "/nested/");
    const api = await import("./qualityFingerprint");
    const [first, second] = await Promise.all([
      api.loadFingerprintReference(),
      api.loadFingerprintReference(),
    ]);
    expect(first).toEqual(second);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch).toHaveBeenCalledWith(
      "/nested/quality-reference/lmfpd-0.2.0/shared_detector.bin",
      { redirect: "error" },
    );
  });

  it("rejects a same-sized tampered binary instead of trusting its metadata", async () => {
    const corrupt = binary.slice();
    corrupt[corrupt.length - 1] ^= 1;
    vi.stubGlobal("fetch", localFetch(corrupt));
    const api = await import("./qualityFingerprint");
    await expect(api.loadFingerprintReference()).rejects.toThrow(
      "SHA-256 校验失败",
    );
  });

  it("rejects truncated data and allows an explicit retry after a failed load", async () => {
    const fakeFetch = localFetch(binary.subarray(0, 12));
    vi.stubGlobal("fetch", fakeFetch);
    const api = await import("./qualityFingerprint");
    await expect(api.loadFingerprintReference()).rejects.toThrow("大小不符");
    fakeFetch.mockImplementation(
      async () => new Response(Uint8Array.from(binary).buffer),
    );
    await expect(api.loadFingerprintReference()).resolves.toMatchObject({
      dataVersion: "0.0.1793708131735",
    });
    expect(fakeFetch).toHaveBeenCalledTimes(2);
  });

  it.each([
    null,
    { ...bundle, schema: "unknown" },
    { ...bundle, files: {} },
    { ...bundle, models: [null, ...bundle.models.slice(1)] },
    { ...bundle, models: [bundle.models[1], ...bundle.models.slice(1)] },
  ])(
    "rejects invalid metadata before any asset or external request",
    async (invalid) => {
      vi.doMock("../vendor/lmfpd/bundle.json", () => ({ default: invalid }));
      const api = await import("./qualityFingerprint");
      await expect(api.loadFingerprintReference()).rejects.toThrow(
        "元数据无效",
      );
      expect(fetch).not.toHaveBeenCalled();
    },
  );
});

describe("reference-class comparisons do not claim identity", () => {
  it("preserves original SDK analysis and describes even a same-class rank as reference only", async () => {
    const api = await import("./qualityFingerprint");
    const result = await api.scoreFingerprint(
      full.outputs,
      full.analysis.prediction,
    );
    expect(result.analysis).toEqual(full.analysis);
    expect(result.verdict).toBe("same_reference");
    expect(result.summary).toContain("不认证模型身份");
    // No arbitrary confidence cutoff is introduced; this is a rank comparison,
    // not a pass/fail judgment or a calibrated open-set identity claim.
    expect(result.analysis.probability).toBeLessThan(0.8);
  });

  it("does not turn an unknown claimed model into a supported one", async () => {
    const api = await import("./qualityFingerprint");
    const result = await api.scoreFingerprint(
      full.outputs,
      "gpt-unknown-private-model",
    );
    expect(result.analysis.prediction).toBe(full.analysis.prediction);
    expect(result.expected).toMatchObject({
      supported: false,
      referenceId: null,
    });
    expect(result.verdict).toBe("unknown_reference");
    expect(result.summary).toContain("无法判定");
  });

  it("maps only the bank's explicit merged alias and never distinguishes its members", async () => {
    const api = await import("./qualityFingerprint");
    const result = await api.scoreFingerprint(full.outputs, "gpt-6.1-sol");
    expect(result.expected).toMatchObject({
      referenceId: "gpt-6-astra",
      supported: true,
      merged: true,
    });
    expect(result.verdict).toBe("different_reference");
    expect(result.summary).toContain("无法区分这些成员");
    const unknownAlias = await api.scoreFingerprint(
      full.outputs,
      "gpt-6.1-sol-custom",
    );
    expect(unknownAlias.verdict).toBe("unknown_reference");
  });

  it("leaves partial and non-model-shaped sequences inconclusive", async () => {
    const api = await import("./qualityFingerprint");
    for (const name of ["partial", "tooShort", "ordered"]) {
      const scenario = oracle.cases.find((entry) => entry.name === name)!;
      const result = await api.scoreFingerprint(
        scenario.outputs,
        "gpt-6.1-sol",
      );
      expect(result.analysis).toEqual(scenario.analysis);
      expect(result.verdict).toBe("insufficient");
      expect(result.summary).toContain("无法判定");
    }
  });

  it("requires complete counts for the UI comparison without modifying SDK scores", async () => {
    const api = await import("./qualityFingerprint");
    const short = full.outputs.map((output) => ({
      ...output,
      text: output.text.split(", ").slice(0, 200).join(", "),
    }));
    const result = await api.scoreFingerprint(short, "gpt-6-astra");
    expect(result.analysis.decision).toBe("scored");
    expect(result.verdict).toBe("insufficient");
  });

  it("keeps invalid-number evidence even when the SDK trims an overlong answer", async () => {
    const api = await import("./qualityFingerprint");
    const overlong = full.outputs.map((output) => ({
      ...output,
      text: `${output.text}, 12, -400`,
    }));
    const result = await api.scoreFingerprint(
      overlong,
      full.analysis.prediction,
    );
    expect(result.analysis).toEqual(full.analysis);
    expect(result.verdict).toBe("insufficient");
  });

  it("rejects malformed or excessive inputs before loading the binary", async () => {
    const api = await import("./qualityFingerprint");
    await expect(api.scoreFingerprint([], "gpt-6-astra")).rejects.toThrow(
      "1–3",
    );
    await expect(
      api.scoreFingerprint([{ text: "1", expected_count: 999 }], "gpt-6-astra"),
    ).rejects.toThrow("292–332");
    await expect(
      api.scoreFingerprint(
        [{ text: "1".repeat(262_145), expected_count: 300 }],
        "gpt-6-astra",
      ),
    ).rejects.toThrow("256 Ki");
    expect(fetch).not.toHaveBeenCalled();
  });
});
