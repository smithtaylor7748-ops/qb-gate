// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
//
// 浏览器助手插件的真实浏览器验收（npm run test:browser-extension:e2e）。
//
// 在一个**临时用户资料目录**里起本机已安装的 Edge / Chrome，加载解压的插件，连独立测试桥
// （cargo run -p qb-app --example browser-link-dev，不开面板、不开数据库、配对记录放临时目录），
// 用测试桥的控制端点切换「放行 / 出口变了 / 查不到 / 面板退出 / 崩溃 / 端口被占……」，看插件怎么拦。
//
//   npm run test:browser-extension:e2e                       （本机有的 Edge、Chrome 都跑一遍）
//   node scripts/browser-extension-e2e.mjs --browser edge     （只跑一个：edge / chrome）
//   node scripts/browser-extension-e2e.mjs --headed --keep    （看得见窗口；跑完留下临时目录与截图）
//
// 不碰的东西：使用者真实的浏览器资料、%LOCALAPPDATA%\ClaudeIpGate、任何数据库、真实的 claude.ai。
// 受保护网站用一个本机 HTTP 服务假扮：--host-resolver-rules 把 qb-protected.test 指到它，
// 再把这个域名加进插件的受保护名单。为此只在**临时复制的那一份插件**的 manifest 里给这两个测试域名
// 加上主机权限（真插件里使用者加网站时由浏览器弹窗申请）；扩展 ID 不变。
//
// 怎么加载插件（Chrome 137 起品牌版不认 --load-extension）：
//   - Edge：--load-extension（实测 157 仍认）。浏览器重启时插件跟真装的一样随浏览器启动，规则照原样留着；
//   - Chrome：--enable-unsafe-extension-debugging + 调试协议 Extensions.loadUnpacked。这样加载的插件
//     重启后不会自己回来，脚本重启后再加载一次（插件存的东西还在，动态规则由插件醒来时按存的设置重建）。
// 都不会下载任何浏览器；本机两个都没有就直接报错退出。
import { chromium } from "@playwright/test";
import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:http";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { inspect } from "node:util";

const root = resolve(import.meta.dirname, "..");
const EXTENSION_ID = "dffmajnemniojfblgfhenndeeelbjgoh";
const SITE = "qb-protected.test";
const OTHER = "qb-other.test";
const IP_A = "203.0.113.10";
const IP_B = "203.0.113.11";

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const option = (name) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : undefined;
};
const headed = flag("--headed");
const keep = flag("--keep");

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const stamp = () => new Date().toLocaleTimeString("zh-CN", { hour12: false });
const log = (text) => console.log(`[${stamp()}] ${text}`);

async function waitFor(check, what, timeout = 20_000) {
  const started = Date.now();
  let last;
  while (Date.now() - started < timeout) {
    try {
      last = await check();
      if (last) return last;
    } catch (error) {
      last = error;
    }
    await sleep(200);
  }
  throw new Error(
    `等不到：${what}（${timeout} 毫秒）。最后一次：${inspect(last, { depth: 4 })}`,
  );
}

function expect(condition, what, detail) {
  if (!condition)
    throw new Error(
      `${what}${detail === undefined ? "" : `：${inspect(detail, { depth: 4 })}`}`,
    );
}

// ------------------------------------------------------------------ 浏览器

function browsers() {
  const env = process.env;
  const candidates = {
    edge: [
      env.QB_E2E_EDGE,
      env["ProgramFiles(x86)"] &&
        join(env["ProgramFiles(x86)"], "Microsoft/Edge/Application/msedge.exe"),
      env.ProgramFiles &&
        join(env.ProgramFiles, "Microsoft/Edge/Application/msedge.exe"),
      env.LOCALAPPDATA &&
        join(env.LOCALAPPDATA, "Microsoft/Edge/Application/msedge.exe"),
      "/usr/bin/microsoft-edge",
    ],
    chrome: [
      env.QB_E2E_CHROME,
      env.ProgramFiles &&
        join(env.ProgramFiles, "Google/Chrome/Application/chrome.exe"),
      env["ProgramFiles(x86)"] &&
        join(env["ProgramFiles(x86)"], "Google/Chrome/Application/chrome.exe"),
      env.LOCALAPPDATA &&
        join(env.LOCALAPPDATA, "Google/Chrome/Application/chrome.exe"),
      "/usr/bin/google-chrome",
    ],
  };
  const found = {};
  for (const [name, list] of Object.entries(candidates)) {
    const exe = list.find((path) => path && existsSync(path));
    if (exe) found[name] = exe;
  }
  return found;
}

// ------------------------------------------------------------------ 测试桥

function bridgeExe() {
  const given = option("--bridge");
  if (given) return given;
  const target = process.env.CARGO_TARGET_DIR || join(root, "target");
  const exe = join(
    target,
    "debug/examples",
    process.platform === "win32" ? "browser-link-dev.exe" : "browser-link-dev",
  );
  if (!existsSync(exe)) {
    log(
      "没有找到独立测试桥，先编译：cargo build -p qb-app --example browser-link-dev",
    );
    const built = spawnSync(
      "cargo",
      ["build", "-p", "qb-app", "--example", "browser-link-dev"],
      {
        cwd: root,
        stdio: "inherit",
        shell: false,
      },
    );
    if (built.status !== 0) throw new Error("独立测试桥编译失败");
  }
  return exe;
}

// 只给 Bridge.freeze 用：Windows 没有 SIGSTOP，借 ntdll 挂起 / 恢复整个进程（只对脚本自己起的测试桥）。
const FREEZE_PS1 = `param([int]$Id, [string]$Op)
Add-Type -Namespace QbE2E -Name Nt -MemberDefinition @'
[DllImport("ntdll.dll")] public static extern int NtSuspendProcess(IntPtr handle);
[DllImport("ntdll.dll")] public static extern int NtResumeProcess(IntPtr handle);
'@
$process = [System.Diagnostics.Process]::GetProcessById($Id)
if ($Op -eq "suspend") { [void][QbE2E.Nt]::NtSuspendProcess($process.Handle) }
else { [void][QbE2E.Nt]::NtResumeProcess($process.Handle) }
`;

class Bridge {
  constructor(exe, dir, logFile, stations) {
    this.exe = exe;
    this.dir = dir;
    this.logFile = logFile;
    this.stations = stations;
    this.proc = null;
  }
  async start({ fresh = false } = {}) {
    rmSync(join(this.dir, "dev-control.json"), { force: true });
    const out = openSync(this.logFile, "a");
    const extra = this.stations.flatMap((spec) => ["--station", spec]);
    this.proc = spawn(
      this.exe,
      ["--dir", this.dir, ...(fresh ? ["--fresh"] : []), ...extra],
      {
        stdio: ["ignore", out, out],
        windowsHide: true,
      },
    );
    this.exited = new Promise((done) => this.proc.once("exit", done));
    const control = await waitFor(
      () => {
        const file = join(this.dir, "dev-control.json");
        return existsSync(file) && JSON.parse(readFileSync(file, "utf8"));
      },
      "测试桥写出 dev-control.json",
      30_000,
    );
    this.control = control.control;
    this.token = control.token;
    await waitFor(() => this.state(), "测试桥的控制端点应答", 10_000);
  }
  async call(command, body) {
    const response = await fetch(`${this.control}/${command}`, {
      method: command === "state" ? "GET" : "POST",
      headers: {
        "X-QB-Dev-Token": this.token,
        "Content-Type": "application/json",
      },
      body: command === "state" ? undefined : JSON.stringify(body ?? {}),
    });
    const value = await response.json();
    if (!response.ok) throw new Error(`测试桥 ${command} 失败：${value.error}`);
    return value;
  }
  state() {
    return this.call("state");
  }
  /** 让测试桥整个停住 / 继续：像电脑睡眠时那样，不出声，但连接没断。 */
  freeze(on) {
    const pid = this.proc.pid;
    if (process.platform !== "win32")
      return void process.kill(pid, on ? "SIGSTOP" : "SIGCONT");
    const script = join(this.dir, "freeze.ps1");
    writeFileSync(script, FREEZE_PS1);
    const result = spawnSync(
      "powershell",
      [
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        script,
        "-Id",
        String(pid),
        "-Op",
        on ? "suspend" : "resume",
      ],
      { windowsHide: true, encoding: "utf8" },
    );
    if (result.status !== 0)
      throw new Error(`${on ? "冻结" : "解冻"}测试桥失败：${result.stderr}`);
  }
  verdict(mode, ip) {
    return this.call("verdict", ip ? { mode, ip } : { mode });
  }
  async crash() {
    await this.call("crash").catch(() => {});
    await this.exited;
    this.proc = null;
  }
  async stop() {
    if (!this.proc) return;
    await this.call("exit").catch(() => {});
    await Promise.race([this.exited, sleep(5_000)]);
    this.proc?.kill();
    this.proc = null;
  }
}

// ------------------------------------------------------------------ 假扮的网站

function siteServer() {
  const hits = [];
  const server = createServer((request, response) => {
    const host = (request.headers.host || "").split(":")[0];
    hits.push({ host, url: request.url, at: Date.now() });
    if (request.url.startsWith("/api/pricing")) {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(
        JSON.stringify({
          data: [
            {
              model_name: "claude-opus-5",
              model_ratio: 2.5,
              completion_ratio: 5,
              enable_groups: ["team"],
            },
          ],
          group_ratio: { team: 0.2 },
        }),
      );
      return;
    }
    if (request.url.startsWith("/api")) {
      response.writeHead(200, {
        "content-type": "application/json",
        "access-control-allow-origin": "*",
      });
      response.end('{"ok":true}');
      return;
    }
    response.writeHead(200, {
      "content-type": "text/html; charset=utf-8",
      "cache-control": "no-store",
    });
    response.end(
      `<!doctype html><meta charset="utf-8"><title>${host}</title><h1 id="marker">QB-E2E ${host}</h1>`,
    );
  });
  return new Promise((done) =>
    server.listen(0, "127.0.0.1", () =>
      done({ server, port: server.address().port, hits }),
    ),
  );
}

// ------------------------------------------------------------------ 一次完整的验收（一个浏览器）

async function run(name, exe) {
  const work = mkdtempSync(join(tmpdir(), `qb-ext-e2e-${name}-`));
  const extDir = join(work, "extension");
  const shots = join(work, "screens");
  mkdirSync(shots, { recursive: true });
  // 临时复制一份插件，只给两个测试域名加主机权限（见文件头）。
  cpSync(join(root, "browser-extension"), extDir, {
    recursive: true,
    filter: (path) => !path.endsWith(".node.mjs"),
  });
  const manifest = JSON.parse(
    readFileSync(join(extDir, "manifest.json"), "utf8"),
  );
  manifest.host_permissions.push(`*://*.${SITE}/*`, `*://*.${OTHER}/*`);
  writeFileSync(
    join(extDir, "manifest.json"),
    JSON.stringify(manifest, null, 2),
  );

  const site = await siteServer();
  const bridge = new Bridge(
    bridgeExe(),
    join(work, "bridge"),
    join(work, "bridge.log"),
    [`E2E=http://${OTHER}`],
  );
  mkdirSync(bridge.dir, { recursive: true });
  const profile = join(work, "profile");
  const results = [];
  let context = null;
  let ctl = null;
  let loadedBy = null;
  const protectedUrl = (path = "") => `http://${SITE}/${path}`;
  const otherUrl = (path = "") => `http://${OTHER}/${path}`;
  const extUrl = (path) => `chrome-extension://${EXTENSION_ID}/${path}`;
  const protectedHits = () =>
    site.hits.filter((hit) => hit.host.endsWith(SITE)).length;

  async function launch() {
    const hostRules = [
      `MAP ${SITE} 127.0.0.1:${site.port}`,
      `MAP *.${SITE} 127.0.0.1:${site.port}`,
      `MAP ${OTHER} 127.0.0.1:${site.port}`,
    ].join(",");
    context = await chromium.launchPersistentContext(profile, {
      executablePath: exe,
      headless: !headed,
      viewport: { width: 1100, height: 820 },
      ignoreDefaultArgs: ["--disable-extensions"],
      args: [
        `--load-extension=${extDir}`,
        "--enable-unsafe-extension-debugging",
        `--host-resolver-rules=${hostRules}`,
        "--disable-features=HttpsUpgrades,HttpsFirstBalancedModeAutoEnable",
        "--no-first-run",
        "--no-default-browser-check",
      ],
    });
    const worker = () =>
      context
        .serviceWorkers()
        .find((w) => w.url().startsWith(`chrome-extension://${EXTENSION_ID}/`));
    let sw = await waitFor(worker, "插件后台随浏览器启动", 4_000).catch(
      () => null,
    );
    if (sw) loadedBy = "命令行 --load-extension";
    else {
      const cdp = await context.browser().newBrowserCDPSession();
      const { id } = await cdp.send("Extensions.loadUnpacked", {
        path: extDir,
      });
      expect(
        id === EXTENSION_ID,
        "调试协议加载出来的扩展 ID 跟 manifest 的公钥不符",
        id,
      );
      await cdp.detach();
      sw = await waitFor(worker, "插件后台启动（调试协议加载）", 10_000);
      loadedBy = "调试协议 Extensions.loadUnpacked";
    }
    ctl = await context.newPage();
    await ctl.goto(extUrl("options.html"));
    // 控制页只用来跟后台说话：停掉它自己的轮询，免得在「后台被回收」那一步把后台叫醒。
    await ctl.evaluate(() => {
      for (let i = 1; i < 10_000; i++) clearInterval(i);
    });
  }

  async function send(type, data = {}) {
    const reply = await ctl.evaluate(
      ([t, d]) => chrome.runtime.sendMessage({ type: t, ...d }),
      [type, data],
    );
    if (!reply?.ok) throw new Error(`插件 ${type}：${reply?.error}`);
    return reply.value;
  }
  const state = () => send("state");
  const sw = () =>
    context
      .serviceWorkers()
      .find((w) => w.url().startsWith(`chrome-extension://${EXTENSION_ID}/`));
  async function rules() {
    // 在插件后台里读规则；后台没醒的话先叫醒它。
    await state();
    const worker = await waitFor(sw, "插件后台在跑");
    return worker.evaluate(async () => ({
      dynamic: (await chrome.declarativeNetRequest.getDynamicRules()).map(
        (r) => [r.id, r.action.type],
      ),
      session: (await chrome.declarativeNetRequest.getSessionRules()).map(
        (r) => [r.id, r.action.type],
      ),
    }));
  }

  async function where(page) {
    const url = page.url();
    if (url.startsWith(extUrl("blocked.html"))) return "blocked";
    let host = null;
    try {
      host = new URL(url).hostname;
    } catch {
      return url;
    }
    if (host.endsWith(SITE) || host === OTHER) {
      const marker = await page
        .locator("#marker")
        .textContent({ timeout: 500 })
        .catch(() => null);
      return marker ? (host === OTHER ? "other" : "site") : `error:${url}`;
    }
    return url;
  }
  async function open(url) {
    const page = await context.newPage();
    await page.goto(url, { timeout: 15_000 }).catch(() => {});
    return page;
  }
  const until = (page, expected, what, timeout) =>
    waitFor(
      async () => (await where(page)) === expected,
      `${what}（期望 ${expected}）`,
      timeout,
    );
  const allowed = (what, timeout) =>
    waitFor(async () => (await state()).allowed, what, timeout);
  const shot = async (page, file) => {
    await page
      .screenshot({ path: join(shots, `${name}-${file}.png`), fullPage: true })
      .catch(() => {});
  };
  async function popupShot(file) {
    const page = await context.newPage();
    await page.setViewportSize({ width: 410, height: 640 });
    await page.goto(extUrl("popup.html"));
    await sleep(600);
    await shot(page, `popup-${file}`);
    const text = await page.locator("#status h1").textContent();
    await page.close();
    return text;
  }
  async function claimIn(url) {
    const page = await open(url);
    await waitFor(
      () => page.locator('[data-view="done"]').isVisible(),
      "认领页显示「已连接」",
      20_000,
    );
    expect(
      !page.url().includes("#c="),
      "认领之后认领码还留在地址栏",
      page.url(),
    );
    return page;
  }

  const scenarios = [
    [
      "装好后自动打开引导页，保护默认关、不拦任何网站",
      async () => {
        const welcome = await waitFor(
          () =>
            context
              .pages()
              .find((p) => p.url().startsWith(extUrl("welcome.html"))),
          "引导页自动打开",
          10_000,
        );
        await waitFor(
          () => welcome.locator("#status h2").textContent(),
          "引导页显示状态",
        );
        await shot(welcome, "welcome-unpaired");
        await welcome.close();
        const s = await state();
        expect(!s.paired && !s.protect, "刚装好不该有配对、不该开保护", s);
        // 把测试域名加进受保护名单（使用者在设置页里做的同一件事）。
        const sites = await send("sites", { sites: [...s.sites, SITE] });
        expect(sites.includes(SITE), "受保护名单没加上测试域名", sites);
        const page = await open(protectedUrl());
        await until(page, "site", "保护没开时受保护网站照常打开");
        const r = await rules();
        expect(
          r.dynamic.length === 0 && r.session.length === 0,
          "保护没开却有规则",
          r,
        );
        await page.close();
        expect(
          (await popupShot("unpaired")).includes("还没连上"),
          "弹窗首屏应说还没连上",
        );
      },
    ],
    [
      "首次配对：面板「连接浏览器」→ 认领页，插件自动认领（不粘贴）",
      async () => {
        const start = await bridge.call("connect");
        expect(
          /^http:\/\/127\.0\.0\.1:\d+\/link#c=[0-9a-f]{64}$/.test(start.url),
          "认领页地址形状不对",
          start,
        );
        const page = await claimIn(start.url);
        await shot(page, "claim-done");
        await page.close();
        const s = await waitFor(async () => {
          const v = await state();
          return v.link === "connected" && v;
        }, "插件连上保护通道");
        expect(s.paired && s.port === start.port, "插件存的端口不对", s);
        const b = await bridge.state();
        expect(
          b.link.pairings.length === 1 && b.link.pairings[0].connected,
          "测试桥没看到这条连接",
          b.link,
        );
        expect(
          b.link.pairings[0].extension_id === EXTENSION_ID,
          "配对记的扩展 ID 不对",
          b.link.pairings[0],
        );
        expect(
          (await popupShot("connected-off")).includes("保护未开启"),
          "弹窗应说保护未开启",
        );
      },
    ],
    [
      "开启保护：出口核实后受保护网站能打开，其他网站正常",
      async () => {
        await send("enable");
        await allowed("开启保护后放行");
        const r = await rules();
        expect(
          JSON.stringify(r) ===
            JSON.stringify({
              dynamic: [[1, "block"]],
              session: [[2, "allow"]],
            }),
          "放行时应有一条拦截（动态）+ 一条放行（会话）规则",
          r,
        );
        const s = await state();
        expect(s.baseline === IP_A, "放行时要记下出口", s);
        const page = await open(protectedUrl("chat"));
        await until(page, "site", "放行时打开受保护网站");
        const other = await open(otherUrl());
        await until(other, "other", "其他网站正常");
        expect(
          (await popupShot("allowed")).includes("保护中"),
          "弹窗应说保护中",
        );
        await page.close();
        await other.close();
      },
    ],
    [
      "出口变了（白名单内换 IP）：已打开的受保护页被切到拦截页、新开被拦、其他网站不动；重新放行后自动回去",
      async () => {
        const kept = await open(protectedUrl("kept"));
        await until(kept, "site", "先打开一个受保护页");
        const other = await open(otherUrl("stay"));
        await until(other, "other", "再打开一个其他网站");
        await other.evaluate(() => (window.__notReloaded = true));
        await bridge.verdict("changed");
        await until(
          kept,
          "blocked",
          "出口变了之后已打开的受保护页被切到拦截页",
          15_000,
        );
        const s = await state();
        expect(
          s.status?.code === "ip_changed" && !s.allowed,
          "应是「出口变了」",
          s,
        );
        const before = protectedHits();
        const fresh = await open(protectedUrl("new"));
        await until(fresh, "blocked", "拦着时新打开的受保护页直接到拦截页");
        expect(
          protectedHits() === before,
          "拦着时受保护网站的请求不该到达网站",
          site.hits.slice(-3),
        );
        // 其他网站：页面没被动过，从它那里发往受保护网站的请求也被拦。
        expect(
          await other.evaluate(() => window.__notReloaded === true),
          "其他网站的标签页被重新加载了",
        );
        expect((await where(other)) === "other", "其他网站的标签页被动了");
        const sub = await other.evaluate(async (url) => {
          try {
            await fetch(url, { mode: "no-cors", cache: "no-store" });
            return "loaded";
          } catch {
            return "blocked";
          }
        }, protectedUrl("api/x"));
        expect(sub === "blocked", "其他网站向受保护网站发的请求应被拦", sub);
        const free = await other.evaluate(
          async (url) => (await fetch(url, { cache: "no-store" })).ok,
          otherUrl("api/y"),
        );
        expect(free, "其他网站自己的请求不该受影响");
        await shot(kept, "blocked-ip-changed");
        expect(
          (await kept.locator("#status h2").textContent()).includes(
            "出口 IP 变了",
          ),
          "拦截页要说出口变了",
        );
        expect(
          (await popupShot("ip-changed")).includes("出口 IP 变了"),
          "弹窗应说出口变了",
        );
        await send("reallow");
        await allowed("重新放行");
        expect((await state()).baseline === IP_B, "重新放行后基准换成新出口");
        await until(kept, "site", "放行后拦截页自动回到原页面");
        await until(fresh, "site", "放行后新开的那一页也回去了");
        expect(
          kept.url().endsWith("/kept"),
          "回到的不是原来的地址",
          kept.url(),
        );
        // 换回 IP_A（测试桥重启后默认就是它），后面的场景都按 IP_A 走。
        await bridge.verdict("allowed", IP_A);
        await waitFor(
          async () => (await state()).status?.code === "ip_changed",
          "换回 IP_A 也算出口变了",
          15_000,
        );
        await send("reallow");
        await allowed("再次重新放行");
        expect((await state()).baseline === IP_A, "基准换回 IP_A");
        for (const p of [kept, fresh, other]) await p.close();
      },
    ],
    [
      "查不到出口：立即拦；出口恢复成放行过的那一个后插件自己再核放行（≥15 秒退避）",
      async () => {
        const page = await open(protectedUrl("unknown"));
        await until(page, "site", "先打开受保护页");
        await bridge.verdict("unknown");
        await until(page, "blocked", "查不到出口时切到拦截页", 15_000);
        expect(
          (await state()).status?.code === "ip_unknown",
          "原因应是查不到出口",
        );
        await bridge.verdict("allowed", IP_A);
        const started = Date.now();
        await allowed("插件按上次放行的出口自动再核", 40_000);
        log(`  自动再核用了 ${Math.round((Date.now() - started) / 1000)} 秒`);
        await until(page, "site", "自动回到原页面");
        await page.close();
      },
    ],
    [
      "面板手动关门 / 代次变化：拦；「立即重新检查」后恢复",
      async () => {
        const page = await open(protectedUrl("paused"));
        await until(page, "site", "先打开受保护页");
        await bridge.call("pause", { paused: true });
        await until(page, "blocked", "IP 锁手动关闭时拦", 15_000);
        expect(
          /手动关闭/.test((await state()).status?.reason ?? ""),
          "原因应是手动关闭",
        );
        await bridge.call("pause", { paused: false });
        await send("retry");
        await allowed("点「立即重新检查」后恢复", 20_000);
        await until(page, "site", "回到原页面");
        await bridge.call("generation");
        await allowed("代次变化后按上次放行的出口自动再核", 20_000);
        await page.close();
      },
    ],
    [
      "面板正常退出（offline）：拦并写明面板已退出；面板重新开始监听后自动连回",
      async () => {
        const page = await open(protectedUrl("offline"));
        await until(page, "site", "先打开受保护页");
        await bridge.call("offline");
        await until(page, "blocked", "面板退出后切到拦截页", 10_000);
        await sleep(3_500); // 让插件重连失败两次：原因要一直说「面板已退出」
        const s = await state();
        expect(
          s.link !== "connected" && /面板已退出/.test(s.linkReason),
          "原因应是面板已退出",
          s,
        );
        await shot(page, "blocked-panel-exit");
        expect(
          (await popupShot("panel-exit")).includes("连不上"),
          "弹窗应说连不上",
        );
        await bridge.call("resume");
        await allowed("面板回来后自动连回并放行", 30_000);
        await until(page, "site", "回到原页面");
        await page.close();
      },
    ],
    [
      "面板卡住不出声（像电脑睡眠时那样，连接没断）：放行到期就拦；面板恢复后自动放行",
      async () => {
        const page = await open(protectedUrl("frozen"));
        await until(page, "site", "先打开受保护页");
        bridge.freeze(true);
        try {
          await until(
            page,
            "blocked",
            "桥不出声、放行到期后切到拦截页",
            20_000,
          );
          const s = await state();
          expect(
            !s.allowed && (s.hold === "expired" || s.link !== "connected"),
            "应是放行到期（或已断开重连）",
            s,
          );
          const before = protectedHits();
          const fresh = await open(protectedUrl("frozen-new"));
          await until(fresh, "blocked", "卡住期间新开的受保护页也拦");
          expect(protectedHits() === before, "卡住期间请求到达了受保护网站");
          await shot(page, "blocked-silent");
          await fresh.close();
        } finally {
          bridge.freeze(false);
        }
        await allowed("面板恢复后自动放行", 40_000);
        await until(page, "site", "回到原页面");
        await page.close();
      },
    ],
    [
      "面板崩溃（不发下线消息）：拦；桥重启（不带 --fresh）后自动连回",
      async () => {
        const page = await open(protectedUrl("crash"));
        await until(page, "site", "先打开受保护页");
        const port = (await state()).port;
        await bridge.crash();
        await until(page, "blocked", "面板崩溃后切到拦截页", 10_000);
        await bridge.start();
        const b = await bridge.state();
        expect(
          b.link.listening && b.link.port === port,
          "桥重启后应沿用原端口",
          b.link,
        );
        await allowed("桥重启后插件自己连回并放行", 40_000);
        await until(page, "site", "回到原页面");
        await page.close();
      },
    ],
    [
      "端口被占：拦；一键重连经认领页学到新端口；放开后一键重连拿回原端口，插件自己连上",
      async () => {
        const page = await open(protectedUrl("occupied"));
        await until(page, "site", "先打开受保护页");
        const original = (await state()).port;
        const moved = await bridge.call("occupy");
        expect(
          moved.relink_needed && moved.port !== original,
          "占住端口后桥应换端口并提示重连",
          moved,
        );
        await until(page, "blocked", "端口被占后拦", 15_000);
        // 旧端口上的程序不回话：插件握手超时，照样拦着。
        await waitFor(
          async () => /没有回应|连不上/.test((await state()).linkReason),
          "插件说明连不上",
          30_000,
        );
        const start = await bridge.call("reconnect");
        expect(
          !start.rebound && start.url,
          "端口还被占着，一键重连应发认领页",
          start,
        );
        const claim = await claimIn(start.url);
        await claim.close();
        await allowed("经认领页重连后放行", 20_000);
        expect((await state()).port === moved.port, "插件应改记新端口");
        expect(
          !(await bridge.state()).link.relink_needed,
          "重连后桥不该再提示重连",
        );
        await until(page, "site", "回到原页面");
        // 第二轮：再被占、放开之后一键重连拿回原端口（不用认领页）。
        const second = await bridge.call("occupy");
        await until(page, "blocked", "又被占时拦", 15_000);
        await bridge.call("release");
        const again = await bridge.call("reconnect");
        expect(
          again.rebound && again.port === moved.port,
          "放开后一键重连应拿回原端口",
          again,
        );
        await allowed("拿回原端口后插件自己连上", 40_000);
        expect(second.port !== moved.port, "第二轮应换过端口", second);
        await until(page, "site", "回到原页面");
        await page.close();
      },
    ],
    [
      "插件后台被回收：醒来先撤放行、按存的配对重连、按上次放行的出口核 —— 期间出口变了就拦",
      async () => {
        const page = await open(protectedUrl("worker"));
        await until(page, "site", "先打开受保护页");
        const boot = (await state()).boot;
        const cdp = await context.newCDPSession(ctl);
        await cdp.send("ServiceWorker.enable");
        await cdp.send("ServiceWorker.stopAllWorkers");
        await bridge.verdict("allowed", IP_B); // 后台不在时出口变了（相对上次放行的 IP_A）
        await waitFor(
          async () =>
            (await bridge.state()).link.pairings.every((p) => !p.connected),
          "桥看到插件断开",
          10_000,
        );
        await cdp.detach();
        const s = await waitFor(
          async () => {
            const v = await state(); // 发消息就把后台叫醒
            return (
              v.boot !== boot &&
              v.link === "connected" &&
              v.status?.code === "ip_changed" &&
              v
            );
          },
          "后台重启后按存的出口核出「出口变了」",
          30_000,
        );
        expect(!s.allowed, "出口变了不该放行", s);
        await until(
          page,
          "blocked",
          "后台重启后已打开的受保护页被切到拦截页",
          10_000,
        );
        await send("reallow");
        await allowed("重新放行");
        await until(page, "site", "回到原页面");
        await page.close();
      },
    ],
    [
      "插件不依赖后台活着：后台停着时受保护网站的请求照样被拦（服务器收不到）",
      async () => {
        await bridge.verdict("unknown");
        await waitFor(
          async () => !(await state()).allowed,
          "查不到出口时不放行",
          15_000,
        );
        const cdp = await context.newCDPSession(ctl);
        await cdp.send("ServiceWorker.enable");
        await cdp.send("ServiceWorker.stopAllWorkers");
        await cdp.detach();
        const before = protectedHits();
        const page = await context.newPage();
        const failed = await page
          .goto(protectedUrl("no-worker"), { timeout: 15_000 })
          .then(() => false)
          .catch((error) =>
            String(error.message).includes("ERR_BLOCKED_BY_CLIENT"),
          );
        expect(
          failed || (await where(page)) === "blocked",
          "后台停着时也应被拦",
        );
        expect(protectedHits() === before, "请求到达了受保护网站");
        await until(page, "blocked", "后台被这次导航叫醒后切到拦截页", 15_000);
        await bridge.verdict("allowed", IP_B);
        await sleep(3_100); // 「立即重新检查」三秒内只认一次
        await send("retry");
        await allowed("恢复放行", 30_000);
        await page.close();
      },
    ],
    [
      "浏览器重启：重启那一刻起就拦（拦截规则还在、放行规则没了），核实之后才放行",
      async () => {
        await bridge.call("offline");
        await waitFor(
          async () => !(await state()).allowed,
          "面板退出后不放行",
          10_000,
        );
        await context.close();
        context = null;
        await launch();
        const s = await state();
        expect(s.protect && !s.allowed, "重启后应开着保护、还没放行", s);
        const r = await rules();
        expect(
          JSON.stringify(r) ===
            JSON.stringify({ dynamic: [[1, "block"]], session: [] }),
          "重启后应只剩拦截规则",
          r,
        );
        const before = protectedHits();
        const page = await open(protectedUrl("restart"));
        await until(page, "blocked", "重启后受保护网站先拦着", 15_000);
        expect(protectedHits() === before, "核实之前请求到达了受保护网站");
        await bridge.call("resume");
        await allowed("面板回来后核实放行", 40_000);
        await until(page, "site", "放行后回到原页面");
        await page.close();
      },
    ],
    [
      "面板里解除配对：插件删掉本地配对、继续拦；重新认领后恢复",
      async () => {
        const page = await open(protectedUrl("revoked"));
        await until(page, "site", "先打开受保护页");
        const pairing = (await state()).pairingId;
        await bridge.call("unpair", { pairing });
        const s = await waitFor(
          async () => {
            const v = await state();
            return !v.paired && v;
          },
          "插件收到解除后删掉本地配对",
          10_000,
        );
        expect(
          s.protect && /解除/.test(s.linkReason),
          "应说明已在面板里解除，并继续开着保护",
          s,
        );
        await until(page, "blocked", "解除后受保护网站拦着", 10_000);
        const start = await bridge.call("connect");
        const claim = await claimIn(start.url);
        await claim.close();
        await allowed("重新认领后放行", 20_000);
        expect((await state()).pairingId !== pairing, "重新认领应是一条新配对");
        await until(page, "site", "回到原页面");
        await page.close();
      },
    ],
    [
      "暂停保护：撤掉全部规则，查不到出口时受保护网站也照常打开；再开启时重新核实",
      async () => {
        const page = await open(protectedUrl("pause"));
        await until(page, "site", "先打开受保护页");
        await bridge.verdict("unknown");
        await until(page, "blocked", "查不到出口时拦", 15_000);
        await send("pause");
        await until(page, "site", "暂停后拦截页回到原页面", 10_000);
        const r = await rules();
        expect(
          r.dynamic.length === 0 && r.session.length === 0,
          "暂停后不该有任何规则",
          r,
        );
        const fresh = await open(protectedUrl("pause-new"));
        await until(fresh, "site", "暂停时受保护网站照常打开");
        await bridge.verdict("allowed", IP_B);
        await send("enable");
        await allowed("再开启后放行");
        for (const p of [page, fresh]) await p.close();
      },
    ],
    [
      "附带功能改走 /v2：站点列表、公开价格、已登录账户导入、浏览器检查",
      async () => {
        const stations = await send("stations");
        expect(
          Array.isArray(stations) && stations[0]?.origin === `http://${OTHER}`,
          "站点列表应来自测试桥的假站点",
          stations,
        );
        const station = await open(otherUrl("console"));
        await until(station, "other", "打开假站点");
        await station.evaluate(() =>
          localStorage.setItem("auth_token", "e2e-fixture-token"),
        );
        await station.bringToFront();
        const target = { stationId: stations[0].id, origin: `http://${OTHER}` };
        const pricing = await send("pricing", { ...target, backend: "newapi" });
        expect(
          pricing.models?.[0]?.model === "claude-opus-5",
          "公开价格应被桥解析出来",
          pricing,
        );
        const imported = await send("import", { ...target, backend: "sub2" });
        expect(
          /开发桥/.test(imported.account ?? ""),
          "账户导入应交到桥（开发桥只核对、不保存）",
          imported,
        );
        if (existsSync(join(root, "dist/probe.html"))) {
          const pagesBefore = context.pages().length;
          await send("probe");
          const probe = await waitFor(
            () =>
              context.pages().length > pagesBefore &&
              context
                .pages()
                .find((p) =>
                  /^http:\/\/127\.0\.0\.1:\d+\/[0-9a-f]{32}\/$/.test(p.url()),
                ),
            "浏览器检查页在这个浏览器里打开",
          );
          await probe.close();
        } else
          log(
            "  没有 dist/probe.html（先 npm run build），跳过浏览器检查页这一项",
          );
        await station.close();
      },
    ],
  ];

  log(`${name}：${exe}`);
  try {
    await bridge.start({ fresh: true });
    await launch();
    log(`  插件加载方式：${loadedBy}；扩展 ID ${EXTENSION_ID}`);
    for (const [title, body] of scenarios) {
      const started = Date.now();
      try {
        await body();
        results.push({ title, ok: true, ms: Date.now() - started });
        log(
          `  ✔ ${title}（${Math.round((Date.now() - started) / 100) / 10} 秒）`,
        );
      } catch (error) {
        results.push({ title, ok: false, error });
        log(
          `  ✘ ${title}\n      ${String(error.message).split("\n").join("\n      ")}`,
        );
        try {
          log(
            `      插件状态：${inspect(await state(), { depth: 3, breakLength: 160 })}`,
          );
        } catch {
          /* 后台可能不在 */
        }
        for (const page of context?.pages() ?? [])
          await shot(
            page,
            `fail-${results.length}-${page.url().slice(0, 30).replace(/\W+/g, "_")}`,
          );
        break;
      }
    }
  } finally {
    await context?.close().catch(() => {});
    await bridge.stop().catch(() => {});
    site.server.close();
  }
  const failed =
    results.some((r) => !r.ok) || results.length < scenarios.length;
  if (failed || keep)
    log(`  临时目录（截图在 screens，测试桥日志 bridge.log）：${work}`);
  else rmSync(work, { recursive: true, force: true });
  return { name, exe, loadedBy, results, total: scenarios.length, failed };
}

const found = browsers();
const wanted = option("--browser");
const names = wanted ? [wanted] : Object.keys(found);
if (!names.length) {
  console.error(
    "本机没有找到 Edge 或 Chrome。可以用环境变量 QB_E2E_EDGE / QB_E2E_CHROME 指到 msedge.exe / chrome.exe。",
  );
  process.exit(2);
}
let anyFailed = false;
const summaries = [];
for (const name of names) {
  if (!found[name]) {
    console.error(`本机没有找到 ${name}`);
    anyFailed = true;
    continue;
  }
  const summary = await run(name, found[name]);
  summaries.push(summary);
  anyFailed ||= summary.failed;
}
console.log("\n结果：");
for (const s of summaries)
  console.log(
    `  ${s.failed ? "✘" : "✔"} ${s.name}（${s.loadedBy ?? "没加载上"}）：${s.results.filter((r) => r.ok).length}/${s.total} 项通过`,
  );
process.exit(anyFailed ? 1 : 0);
