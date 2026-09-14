/**
 * 把演示模式的面板拍成 docs/screenshots/*.png（README 里用的那几张）。
 *
 * ```bash
 * node scripts/screenshots.mjs
 * ```
 *
 * # 为什么不直接拍真面板
 *
 * 面板上每个数字都是本机的真实状态：出口 IP、账户槽位、套餐、安装路径、日志。
 * 直接拍等于把这些一起发出去。所以这里跑的是 `npm run demo`（`VITE_DEMO=1`），
 * 数据全部来自 `src/lib/demo.ts` 里编造的那一份，见那个文件的文件头。
 *
 * # 它做了什么
 *
 * 1. 1420 端口没人应答就自己起一个 `npm run demo`，拍完再收掉（本来就在跑就复用，
 *    也不会替你关掉）；
 * 2. 用无头 Chromium（Chrome 或 Edge，谁在就用谁）按 `?page=` 逐页拍，
 *    `?page=` 这个参数**只有演示模式认**，见 `src/App.tsx::initialPage`；
 * 3. 窗口固定 1180×820 —— 跟 `tauri.conf.json` 里的主窗口一样大，
 *    倍率 2 出的是 2360×1640，GitHub 上缩着看才够清楚。
 *
 * 拍出来的是**浅色**：无头浏览器默认就是浅色，不用再去模拟 `prefers-color-scheme`，
 * 谁在什么机器上跑都拍出同一套图。
 */

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(ROOT, "docs", "screenshots");
const ORIGIN = "http://localhost:1420";
const SIZE = { w: 1180, h: 820 };

/**
 * 拍哪几页。路径要对得上 `src/lib/routes.ts` 的 `NAV`，改路由要同步改这里。
 *
 * 三项体检与门禁**没有自己的路径**了：它们是总览上点开的小窗
 * （`SecuritySheet`），拍不到，也不该为了拍图给它们造一个入口回来。
 * 总览那张里四格评分明细和门禁读数都在，该看的都看得见。
 */
const SHOTS = [
  ["", "overview.png"],
  ["accounts", "accounts.png"],
  ["relays", "relay.png"],
  ["software", "software.png"],
  ["extensions", "extensions.png"],
  ["settings", "settings.png"],
];

const BROWSERS = [
  process.env.QB_SHOT_BROWSER,
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
].filter(Boolean);

function findBrowser() {
  const hit = BROWSERS.find((p) => existsSync(p));
  if (!hit) {
    throw new Error(
      "找不到 Chrome 或 Edge。装一个，或用 QB_SHOT_BROWSER=<浏览器完整路径> 指过去。",
    );
  }
  return hit;
}

async function alive() {
  try {
    const r = await fetch(ORIGIN, { signal: AbortSignal.timeout(1000) });
    return r.ok;
  } catch {
    return false;
  }
}

/** 等 vite 起来。60 次 × 500ms = 30 秒，够冷启动了。 */
async function waitUp() {
  for (let i = 0; i < 60; i++) {
    if (await alive()) return true;
    await new Promise((r) => setTimeout(r, 500));
  }
  return false;
}

function shoot(browser, page, file) {
  const args = [
    "--headless=new",
    "--disable-gpu",
    "--hide-scrollbars",
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-extensions",
    `--user-data-dir=${join(tmpdir(), "qb-gate-shots")}`,
    `--window-size=${SIZE.w},${SIZE.h}`,
    "--force-device-scale-factor=2",
    // SPA 要等 React 挂载 + 演示层那 120ms，不等就会拍到空壳。
    "--virtual-time-budget=8000",
    `--screenshot=${join(OUT, file)}`,
    `${ORIGIN}/#/${page}`,
  ];
  return new Promise((ok, bad) => {
    const p = spawn(browser, args, { stdio: "ignore" });
    p.on("error", bad);
    p.on("exit", (code) =>
      code === 0 ? ok() : bad(new Error(`浏览器退出码 ${code}`)),
    );
  });
}

const browser = findBrowser();
mkdirSync(OUT, { recursive: true });

let server = null;
if (!(await alive())) {
  console.log("1420 没人应答，起一个 npm run demo …");
  server = spawn("npm", ["run", "demo"], {
    cwd: ROOT,
    stdio: "ignore",
    shell: true,
  });
  if (!(await waitUp())) {
    server.kill();
    throw new Error(
      "演示服务器 30 秒内没起来。先手动 npm run demo 看看报什么错。",
    );
  }
} else {
  console.log("1420 已经在跑，直接用它。");
}

try {
  for (const [page, file] of SHOTS) {
    await shoot(browser, page, file);
    const bytes = statSync(join(OUT, file)).size;
    console.log(`  ${file}  ${(bytes / 1024).toFixed(0)} KB`);
  }
} finally {
  // 自己起的才收；本来就在跑的别动人家。
  if (server) server.kill();
}

console.log(`\n拍完 ${readdirSync(OUT).length} 张，在 docs/screenshots/`);
console.log("图里全是 src/lib/demo.ts 的演示数据，没有任何真实机器的状态。");
