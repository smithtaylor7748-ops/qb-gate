/**
 * 发版时生成两样东西（`release.yml` 收集完安装包之后调）：
 *
 * 1. `dist-installers/update.json` —— 面板「发现新版本」弹窗读的就是它
 *    （`crates/qb-install/src/install/self_update.rs`）。只放版本号、标签、发布时刻和说明，
 *    **不放任何地址或哈希**：下载地址面板按常量自己拼，哈希看同一个 Release 的 `SHA256SUMS.txt`。
 * 2. `target/release-body.md` —— GitHub Release 的正文：中英两段说明 + 固定的下载与免责说明。
 *
 * 说明的唯一来源是 `.github/release-notes.md`（格式写在那个文件头上）。
 *
 * ```bash
 * node scripts/update-manifest.mjs           # 写上面两个文件
 * node scripts/update-manifest.mjs --check   # 只核对说明跟 package.json 的版本号对得上
 * ```
 *
 * `npm run release:check` 也会核同一件事 —— 提了版本号却没换说明，CI 当场红，
 * 不会发出一版弹窗里写着上一版内容的更新。
 */
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

export const NOTES_FILE = ".github/release-notes.md";

/** 拆出版本号、中文段、英文段。格式不对就抛出能照着改的话。 */
export function parseReleaseNotes(text) {
  const src = text.replace(/\r\n/g, "\n");
  const marker = src.match(/<!--\s*release-notes:\s*(\d+\.\d+\.\d+)\s*-->/);
  if (!marker)
    throw new Error(
      `${NOTES_FILE} 开头要有 <!-- release-notes: <版本号> --> 这一行`,
    );
  const zhAt = src.indexOf("<!-- zh -->");
  const enAt = src.indexOf("<!-- en -->");
  if (zhAt < 0 || enAt < 0 || enAt < zhAt)
    throw new Error(
      `${NOTES_FILE} 要有 <!-- zh --> 与 <!-- en --> 两段，中文在前`,
    );
  const zh = src.slice(zhAt + "<!-- zh -->".length, enAt).trim();
  const en = src.slice(enAt + "<!-- en -->".length).trim();
  if (!zh || !en) throw new Error(`${NOTES_FILE} 的中文或英文那一段是空的`);
  return { version: marker[1], zh, en };
}

/** 读说明并核对版本号。`release-check.mjs` 也调它。 */
export function loadReleaseNotes() {
  const pkg = JSON.parse(readFileSync("package.json", "utf8"));
  const notes = parseReleaseNotes(readFileSync(NOTES_FILE, "utf8"));
  if (notes.version !== pkg.version)
    throw new Error(
      `${NOTES_FILE} 写的是 ${notes.version}，package.json 是 ${pkg.version} —— 提版本号时要把更新说明整份换成这一版的`,
    );
  return notes;
}

const FOOTER = `---

Windows 10 / 11 x64 安装包，由上方标签对应的源码在 GitHub Actions 上构建。安装包尚未做代码签名，下载后请对照 SHA256SUMS.txt 核对哈希。已经装了 0.25.3 及以后版本的，打开面板会收到这一版的更新提醒。

使用前请务必阅读 DISCLAIMER.md（免责声明）：本项目是非官方项目，与任何服务商无隶属或合作关系，不对账户状态作任何承诺。中转站功能（含智能调度）仍在内测，界面里已经出现，但暂时不能正常使用。

问题反馈：GitHub Issues，或 QQ 群「门禁值班室」1109462206。

Windows 10 / 11 x64. This installer was built on GitHub Actions from the source tag shown above. It is not code-signed yet; verify the download against SHA256SUMS.txt. Installs of 0.25.3 or later are notified about this release inside the panel.
Read README.md and DISCLAIMER.md before use. Third-party notices and the source license are included with this release and inside the installer.
`;

function main() {
  const notes = loadReleaseNotes();
  if (process.argv.includes("--check")) {
    console.log(`Release notes match version ${notes.version}.`);
    return;
  }
  const manifest = {
    version: notes.version,
    tag: `v${notes.version}`,
    published_at: new Date().toISOString(),
    notes: notes.zh,
    notes_en: notes.en,
  };
  mkdirSync("dist-installers", { recursive: true });
  writeFileSync(
    "dist-installers/update.json",
    JSON.stringify(manifest, null, 2) + "\n",
  );
  mkdirSync("target", { recursive: true });
  writeFileSync(
    "target/release-body.md",
    [notes.zh, "", notes.en, "", FOOTER].join("\n"),
  );
  console.log(
    `Wrote dist-installers/update.json and target/release-body.md for ${manifest.tag}.`,
  );
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) main();
