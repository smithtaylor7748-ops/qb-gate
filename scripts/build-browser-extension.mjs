// Build the unpacked, dependency-free Chrome extension with its legal notices.
//
// 产物放在仓库根的 build/browser-extension（2026-11-19，P3）：
// - 不放 dist/ —— `vite build` 每次都清空 dist/，插件跟着没了；
// - 安装包从这里取：tauri.conf.json 的 bundle.resources 把它装进安装目录的 browser-extension 文件夹，
//   面板「扩展 → 浏览器助手」的「打开插件文件夹」在安装版里指的就是那一份；
// - 开发版的「打开插件文件夹」直接指这里；src-tauri/build.rs 在 cargo 编译时也会跑这个脚本，
//   所以干净克隆不用先手动构建插件，cargo test 也不会因为资源目录不在而失败；
// - 发版时 release.yml 把这个文件夹打成 QB-Gate-Browser-Extension.zip 作为发布附件。
//
// ⚠ 只在内容变了时才写文件，并保留源文件的修改时间：tauri-build 对资源里每个文件都登记了
// rerun-if-changed，build.rs 跑这个脚本时要是每次都重写，cargo 就会认为资源变了、每次都重编。
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  rmdirSync,
  statSync,
  utimesSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import assert from "node:assert/strict";
import { DEFAULT_SITES, sitePatterns } from "../browser-extension/core.js";

const root = resolve(import.meta.dirname, "..");
const source = resolve(root, "browser-extension");
export const destination = resolve(root, "build", "browser-extension");
const manifest = JSON.parse(
  readFileSync(resolve(source, "manifest.json"), "utf8"),
);
assert.equal(manifest.manifest_version, 3);
assert.equal(
  manifest.name,
  "QB Gate 浏览器助手",
  "面板按这个名字认插件文件夹（crates/qb-app/src/browser_link/folder.rs）",
);
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

/** 分发目录里该有的文件：相对路径 → 源文件（或要写的内容）。 */
const wanted = new Map();
// 单测（*.node.mjs）与开发者手动走查（连独立测试桥用的）不进分发目录；别的都要（页面、脚本、样式、图标）。
for (const entry of readdirSync(source, {
  recursive: true,
  withFileTypes: true,
})) {
  if (
    !entry.isFile() ||
    entry.name.endsWith(".node.mjs") ||
    entry.name === "MANUAL-CHECK.zh-CN.md"
  )
    continue;
  const from = resolve(entry.parentPath ?? entry.path, entry.name);
  wanted.set(relative(source, from), { from });
}
for (const file of ["LICENSE", "LICENSE-ADDITIONAL-TERMS.md"])
  wanted.set(file, { from: resolve(root, file) });
wanted.set("BUILD.txt", {
  text: `QB Gate Chrome companion ${manifest.version}\nLoad this folder as an unpacked extension (chrome://extensions → Developer mode → Load unpacked).\nSource: https://github.com/smithtaylor7748-ops/qb-gate\n`,
});

let written = 0;
mkdirSync(destination, { recursive: true });
for (const [file, item] of wanted) {
  const to = resolve(destination, file);
  const bytes = item.text !== undefined ? Buffer.from(item.text) : null;
  const same =
    existsSync(to) &&
    (bytes
      ? readFileSync(to).equals(bytes)
      : readFileSync(to).equals(readFileSync(item.from)));
  if (same) continue;
  mkdirSync(dirname(to), { recursive: true });
  if (bytes) writeFileSync(to, bytes);
  else {
    copyFileSync(item.from, to);
    const { atime, mtime } = statSync(item.from);
    utimesSync(to, atime, mtime);
  }
  written += 1;
}
// 源里删掉的文件，分发目录里也删掉 —— 不然旧版的页面会一直跟着安装包走。
let removed = 0;
const sweep = (dir) => {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      sweep(full);
      if (readdirSync(full).length === 0) rmdirSync(full);
    } else if (!wanted.has(relative(destination, full))) {
      rmSync(full);
      removed += 1;
    }
  }
};
sweep(destination);
assert.ok(
  existsSync(resolve(destination, "manifest.json")),
  "分发目录里没有 manifest.json",
);
console.log(
  `Chrome extension ${manifest.version} ready: ${destination}${sep}（${wanted.size} 个文件，更新 ${written} 个，删掉 ${removed} 个）`,
);
