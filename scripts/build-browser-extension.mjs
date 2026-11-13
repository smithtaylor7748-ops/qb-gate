// Build the unpacked, dependency-free Chrome extension with its legal notices.
import { cpSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import assert from "node:assert/strict";

const root = resolve(import.meta.dirname, "..");
const source = resolve(root, "browser-extension");
const destination = resolve(root, "dist/browser-extension");
const manifest = JSON.parse(
  readFileSync(resolve(source, "manifest.json"), "utf8"),
);
assert.equal(manifest.manifest_version, 3);
assert.ok(Number(manifest.minimum_chrome_version) >= 116);
assert.deepEqual(manifest.host_permissions, ["http://127.0.0.1/*"]);
mkdirSync(destination, { recursive: true });
for (const file of [
  "manifest.json",
  "background.js",
  "core.js",
  "capture.js",
  "popup.html",
  "popup.js",
  "popup.css",
  "README.md",
]) {
  cpSync(resolve(source, file), resolve(destination, file));
}
for (const file of ["LICENSE", "LICENSE-ADDITIONAL-TERMS.md"])
  cpSync(resolve(root, file), resolve(destination, file));
writeFileSync(
  resolve(destination, "BUILD.txt"),
  "QB Gate Chrome companion\nLoad this folder as an unpacked extension.\nSource: https://github.com/smithtaylor7748-ops/qb-gate\n",
);
console.log(`Chrome extension ready: ${destination}`);
