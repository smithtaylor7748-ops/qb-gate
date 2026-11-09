import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = new URL("../", import.meta.url);
const sha256 = (value) => createHash("sha256").update(value).digest("hex");
const reference = new URL(
  "public/quality-reference/lmfpd-0.2.0/shared_detector.bin",
  root,
);
const [binary, metadata, sdkLicense, dataLicense, adapter] = await Promise.all([
  readFile(reference),
  readFile(new URL("src/vendor/lmfpd/bundle.json", root)),
  readFile(new URL("src/vendor/lmfpd/LICENSE", root), "utf8"),
  readFile(
    new URL("public/quality-reference/lmfpd-0.2.0/LICENSE", root),
    "utf8",
  ),
  readFile(new URL("src/lib/qualityFingerprint.ts", root), "utf8"),
]);
const expected =
  "59dffad8128dca8bcf0cfb024f0fcebe7f0b7d7fbae3583b4b5d1418c18e601f";
if (binary.length !== 4_808_724 || sha256(binary) !== expected) {
  throw new Error(`Invalid fingerprint reference: ${fileURLToPath(reference)}`);
}
if (
  sha256(metadata) !==
  "2e19810b40a9b2ffe84b312c20874e2932c9f151816d6348cb485c60871bd9a4"
) {
  throw new Error(
    "Fingerprint metadata differs from the pinned npm data package.",
  );
}
if (
  sdkLicense !== dataLicense ||
  sha256(sdkLicense) !==
    "238afb57e498990742f84b9ce6167b4e256ac06104a9938ce953bc41c14bea51" ||
  !sdkLicense.startsWith("MIT License") ||
  !sdkLicense.includes("Copyright (c) 2026 xqy2006") ||
  !sdkLicense.includes('THE SOFTWARE IS PROVIDED "AS IS"')
) {
  throw new Error(
    "Original lmfpd SDK/data MIT licenses are missing or differ.",
  );
}
if (!adapter.includes(expected)) {
  throw new Error("The app must bind its own compiled reference hash.");
}
const bundle = JSON.parse(metadata.toString("utf8"));
if (
  bundle.schema !== "lmfpd-data-v1" ||
  bundle.detector_schema !== "shared-detector-v2" ||
  bundle.files["shared_detector.bin"].sha256 !== expected ||
  bundle.models.length !== 66
) {
  throw new Error("Unexpected fingerprint reference schema or model catalog.");
}
console.log(
  "lmfpd: pinned binary, metadata, compiled hash and MIT licenses verified.",
);
