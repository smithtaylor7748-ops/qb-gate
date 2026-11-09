import bundle from "../vendor/lmfpd/bundle.json";
import { analyzeOutputs } from "../vendor/lmfpd/analysis";
import {
  generateChallenges,
  type ChallengeRandom,
} from "../vendor/lmfpd/challenge-browser.js";
import { decodeDetector } from "../vendor/lmfpd/detector-codec";
import { parseNumbers } from "../vendor/lmfpd/fingerprint-core.js";
import {
  supportsSharedDetector,
  type SharedDetector,
} from "../vendor/lmfpd/shared-detector";
import type { Analysis, Challenge, Output } from "../vendor/lmfpd/sdk-types";
import type { Bank } from "../vendor/lmfpd/types";

/** The hash is compiled into the application, never taken from a fetched manifest. */
export const FINGERPRINT_REFERENCE_SHA256 =
  "59dffad8128dca8bcf0cfb024f0fcebe7f0b7d7fbae3583b4b5d1418c18e601f";
const REFERENCE_BYTES = 4_808_724;
const REFERENCE_BUILT_AT = "2026-11-03T12:06:39.864352+00:00";
const REFERENCE_BASE_SHA256 =
  "ae48f7c1832e118382b03da9b150fa333cabc5f91a74e722e0719caa48ff87c4";
const REFERENCE_SOURCE_SHA256 =
  "ce430f0de948c3f14da1143c693b4e4d69754169bb2856e766eb223cf5a4bacb";
const SDK_VERSION = "0.2.0";
const DATA_VERSION = "0.0.1793708131735";
const MAX_REPLY_CHARACTERS = 262_144;

export type FingerprintChallenge = Challenge;
export type FingerprintOutput = Output;
export type FingerprintAnalysis = Analysis;

export interface FingerprintReferenceModel {
  id: string;
  label: string;
  family: string;
  familyLabel: string;
  responseCount: number;
  aliases: readonly string[];
  merged: boolean;
}

export interface FingerprintCatalog {
  sdkVersion: string;
  dataVersion: string;
  builtAt: string;
  referenceSha256: string;
  models: readonly FingerprintReferenceModel[];
}

export interface FingerprintExpectation {
  requested: string;
  referenceId: string | null;
  label: string | null;
  supported: boolean;
  merged: boolean;
}

export interface FingerprintScore {
  analysis: Analysis;
  expected: FingerprintExpectation;
  /** Comparison of reference-class ranks only; never a pass/fail or identity verdict. */
  verdict:
    | "insufficient"
    | "unknown_reference"
    | "same_reference"
    | "different_reference";
  summary: string;
  sdkVersion: string;
  dataVersion: string;
  builtAt: string;
}

/** An explicit merge in this pinned bank, not a general model-name alias heuristic. */
const MERGED_ALIASES: Readonly<Record<string, readonly string[]>> = {
  "gpt-6-astra": ["gpt-6-astra", "gpt-6.1-sol"],
};

function checkedBank(): Bank {
  if (
    !bundle ||
    typeof bundle !== "object" ||
    bundle.schema !== "lmfpd-data-v1" ||
    bundle.detector_schema !== "shared-detector-v2" ||
    bundle.built_at !== REFERENCE_BUILT_AT ||
    bundle.files?.["shared_detector.bin"]?.sha256 !==
      FINGERPRINT_REFERENCE_SHA256 ||
    !Array.isArray(bundle.models) ||
    bundle.models.length !== 66 ||
    bundle.models.some(
      (model) =>
        !model ||
        typeof model.id !== "string" ||
        !model.id ||
        typeof model.display_name !== "string" ||
        !model.display_name ||
        typeof model.family !== "string" ||
        !model.family ||
        typeof model.family_name !== "string" ||
        !model.family_name ||
        !Number.isSafeInteger(model.response_count) ||
        model.response_count < 1,
    ) ||
    new Set(bundle.models.map((model) => model.id)).size !==
      bundle.models.length
  ) {
    throw new Error("指纹参考库的版本或模型元数据无效，请修复安装后重试。");
  }
  // As in lmfpd-sdk, the bundle contains the names/counts needed for scoring;
  // its large training-only Bank fields are not needed by the shared detector.
  return bundle as unknown as Bank;
}

/** Metadata is bundled with the app; this function performs no fetch or model call. */
export function getFingerprintCatalog(): FingerprintCatalog {
  const bank = checkedBank();
  return Object.freeze({
    sdkVersion: SDK_VERSION,
    dataVersion: DATA_VERSION,
    builtAt: bank.built_at,
    referenceSha256: FINGERPRINT_REFERENCE_SHA256,
    models: Object.freeze(
      bank.models.map((model) =>
        Object.freeze({
          id: model.id,
          label: model.display_name,
          family: model.family,
          familyLabel: model.family_name,
          responseCount: model.response_count,
          aliases: Object.freeze([...(MERGED_ALIASES[model.id] ?? [model.id])]),
          merged: Boolean(MERGED_ALIASES[model.id]),
        }),
      ),
    ),
  });
}

function seededRandom(seed: string | number): ChallengeRandom {
  const text = String(seed);
  if (
    !text.trim() ||
    text.length > 256 ||
    (typeof seed === "number" && !Number.isFinite(seed))
  ) {
    throw new Error("指纹测试种子必须是非空短文本或有限数值。");
  }
  // Only prompt selection is seeded. The SDK's features, ranking and calibration
  // remain unchanged. This local PRNG never mutates global crypto or Math.random.
  let state = 2166136261;
  for (let index = 0; index < text.length; index++) {
    state = Math.imul(state ^ text.charCodeAt(index), 16777619) >>> 0;
  }
  const seedHash = state.toString(16).padStart(8, "0");
  let id = 0;
  return {
    getRandomValues(array) {
      for (let index = 0; index < array.length; index++) {
        state = (state + 0x6d2b79f5) >>> 0;
        let value = state;
        value = Math.imul(value ^ (value >>> 15), value | 1);
        value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
        array[index] = (value ^ (value >>> 14)) >>> 0;
      }
      return array;
    },
    randomUUID() {
      // The ID is local bookkeeping, not part of the model prompt.
      id += 1;
      return `qb-${seedHash}-${id}`;
    },
  };
}

/** Exactly three upstream SDK prompts, with distinct requested lengths 292–332. */
export function createFingerprintPrompts(
  seed: string | number,
): FingerprintChallenge[] {
  return generateChallenges(3, [], seededRandom(seed));
}

/** Preserve the SDK's longest-number-run parser instead of inventing a second one. */
export const parseFingerprintNumbers = parseNumbers;

let referencePromise: Promise<SharedDetector> | undefined;

async function readLocalReference(): Promise<SharedDetector> {
  const bank = checkedBank();
  // BASE_URL supports both the Vite root and the packaged Tauri asset origin.
  // There is no external fallback, remote manifest or automatic reference update.
  const path = `${import.meta.env.BASE_URL}quality-reference/lmfpd-0.2.0/shared_detector.bin`;
  let response: Response;
  try {
    response = await fetch(path, { redirect: "error" });
  } catch {
    throw new Error("无法读取应用内置指纹参考库，请修复安装后重试。");
  }
  if (!response.ok)
    throw new Error("无法读取应用内置指纹参考库，请修复安装后重试。");
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.byteLength !== REFERENCE_BYTES) {
    throw new Error("指纹参考库大小不符，已停止评分；请修复安装后重试。");
  }
  if (!globalThis.crypto?.subtle) {
    throw new Error("当前 WebView 无法校验指纹参考库，请更新 WebView 后重试。");
  }
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  const hash = Array.from(new Uint8Array(digest), (value) =>
    value.toString(16).padStart(2, "0"),
  ).join("");
  if (hash !== FINGERPRINT_REFERENCE_SHA256) {
    throw new Error(
      "指纹参考库 SHA-256 校验失败，已停止评分；请修复安装后重试。",
    );
  }
  try {
    const detector = decodeDetector(bytes);
    if (
      !supportsSharedDetector(bank, detector) ||
      detector.base_sha256 !== REFERENCE_BASE_SHA256 ||
      detector.source_reference_sha256 !== REFERENCE_SOURCE_SHA256
    ) {
      throw new Error("mismatched detector");
    }
    return detector;
  } catch {
    throw new Error("指纹参考库的结构或模型元数据不匹配，已停止评分。");
  }
}

function reference(): Promise<SharedDetector> {
  if (!referencePromise) {
    referencePromise = readLocalReference().catch((error: unknown) => {
      referencePromise = undefined;
      throw error;
    });
  }
  return referencePromise;
}

/** Optional preflight; reads only one app-local binary and verifies its fixed hash. */
export async function loadFingerprintReference(): Promise<FingerprintCatalog> {
  await reference();
  return getFingerprintCatalog();
}

function expectation(
  requested: string,
  catalog: FingerprintCatalog,
): FingerprintExpectation {
  const name = requested.trim().toLowerCase();
  const model = catalog.models.find((entry) => entry.aliases.includes(name));
  return {
    requested,
    referenceId: model?.id ?? null,
    label: model?.label ?? null,
    supported: Boolean(model),
    merged: model?.merged ?? false,
  };
}

/**
 * Offline lmfpd-sdk 0.2.0 analysis. The UI comparison is deliberately narrower
 * than the SDK's 55%-length acceptance rule; the raw SDK result is kept intact.
 */
export async function scoreFingerprint(
  outputs: FingerprintOutput[],
  expectedReferenceId: string,
): Promise<FingerprintScore> {
  if (
    !Array.isArray(outputs) ||
    outputs.length < 1 ||
    outputs.length > 3 ||
    outputs.some(
      (output) =>
        !output ||
        typeof output.text !== "string" ||
        output.text.length > MAX_REPLY_CHARACTERS ||
        !Number.isSafeInteger(output.expected_count) ||
        output.expected_count < 292 ||
        output.expected_count > 332,
    ) ||
    new Set(outputs.map((output) => output.expected_count)).size !==
      outputs.length ||
    typeof expectedReferenceId !== "string"
  ) {
    throw new Error(
      "指纹评分需要 1–3 份原始回复及各自 292–332 的请求数量，单份回复不可超过 256 Ki 字符。",
    );
  }
  const catalog = getFingerprintCatalog();
  const detector = await reference();
  const analysis = analyzeOutputs(outputs, checkedBank(), detector, false);
  const expected = expectation(expectedReferenceId, catalog);
  let verdict: FingerprintScore["verdict"];
  let summary: string;
  if (!expected.supported) {
    verdict = "unknown_reference";
    summary = `固定参考库没有“${expected.requested || "未指定模型"}”的独立或明确合并参考，无法判定。即使有库内排名，也不能据此把未知模型认作已有模型。`;
  } else if (
    analysis.decision !== "scored" ||
    outputs.length !== 3 ||
    analysis.probability_status !== "reference_calibrated" ||
    outputs.some(
      (output) => parseNumbers(output.text).length < output.expected_count,
    ) ||
    outputs.some((output) =>
      // Same raw range check as the SDK diagnostic, before its optional
      // overlong-answer truncation can remove evidence of an invalid number.
      Array.from(output.text.matchAll(/-?\d+/g), (match) =>
        Number(match[0]),
      ).some((number) => number < 1 || number > 355),
    )
  ) {
    verdict = "insufficient";
    summary =
      analysis.decision === "not_model"
        ? "数字序列的变化方式不符合参考回答分布，无法判定；这不代表已经证明回复来自程序或假模型。"
        : "缺少三份达到请求数量的有效回答，或参考校准不可用，无法判定；部分排名仅供查看。";
  } else {
    verdict =
      analysis.prediction === expected.referenceId
        ? "same_reference"
        : "different_reference";
    summary = `在固定参考库中排名最接近“${analysis.prediction_name}”，${verdict === "same_reference" ? "与所选参考类相同" : `与所选参考类“${expected.label}”不同`}。概率仅为库内相对结果，不认证模型身份，也不直接判断质量下降。`;
  }
  const predictedModel = catalog.models.find(
    (model) => model.id === analysis.prediction,
  );
  const merged = expected.merged
    ? catalog.models.find((model) => model.id === expected.referenceId)
    : predictedModel?.merged
      ? predictedModel
      : undefined;
  if (merged) {
    summary += ` 此参考类合并了 ${merged.aliases.join(" / ")}，无法区分这些成员。`;
  }
  return {
    analysis,
    expected,
    verdict,
    summary,
    sdkVersion: catalog.sdkVersion,
    dataVersion: catalog.dataVersion,
    builtAt: catalog.builtAt,
  };
}
