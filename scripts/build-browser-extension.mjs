// Build the unpacked, dependency-free Chrome extension with its legal notices.
// 产物位置（dist/browser-extension）与安装包里的去处由 P3 定；这里只保证文件齐、权限没多给。
import {
  cpSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { resolve } from "node:path";
import assert from "node:assert/strict";
import { DEFAULT_SITES, sitePatterns } from "../browser-extension/core.js";

const root = resolve(import.meta.dirname, "..");
const source = resolve(root, "browser-extension");
const destination = resolve(root, "dist/browser-extension");
const manifest = JSON.parse(
  readFileSync(resolve(source, "manifest.json"), "utf8"),
);
assert.equal(manifest.manifest_version, 3);
assert.ok(Number(manifest.minimum_chrome_version) >= 116);
assert.ok(
  typeof manifest.key === "string" && manifest.key.length > 100,
  "manifest 缺固定扩展 ID 的公钥",
);
// 主机权限只有本机回环（连 QB Gate）与默认受保护网站（拦截时把已打开的页面切到说明页）。
assert.deepEqual(manifest.host_permissions, [
  "http://127.0.0.1/*",
  ...sitePatterns(DEFAULT_SITES),
]);
mkdirSync(destination, { recursive: true });
// 单测（*.node.mjs）与开发者手动走查（连独立测试桥用的）不进分发目录；别的都要（页面、脚本、样式、图标）。
const files = readdirSync(source, { recursive: true, withFileTypes: true })
  .filter(
    (entry) =>
      entry.isFile() &&
      !entry.name.endsWith(".node.mjs") &&
      entry.name !== "MANUAL-CHECK.zh-CN.md",
  )
  .map((entry) =>
    resolve(entry.parentPath ?? entry.path, entry.name).slice(
      source.length + 1,
    ),
  );
for (const file of files) {
  mkdirSync(resolve(destination, file, ".."), { recursive: true });
  cpSync(resolve(source, file), resolve(destination, file));
}
for (const file of ["LICENSE", "LICENSE-ADDITIONAL-TERMS.md"])
  cpSync(resolve(root, file), resolve(destination, file));
writeFileSync(
  resolve(destination, "BUILD.txt"),
  "QB Gate Chrome companion\nLoad this folder as an unpacked extension.\nSource: https://github.com/smithtaylor7748-ops/qb-gate\n",
);
console.log(`Chrome extension ready: ${destination}（${files.length} 个文件）`);
