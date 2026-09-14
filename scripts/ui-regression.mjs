import { chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import assert from "node:assert/strict";
// 六个入口。`""` 是根路径（官方账户，同时是仪表盘）。
// 这张表必须跟 src/lib/routes.ts 的 NAV 对得上，否则 CI 直接红。
const ROUTES = ["", "relays", "software", "extensions", "settings"];
// 截图沿用旧文件名 —— README.md 引着 workspace-light.png / extensions-light.png。
const SHOT = { "": "workspace", extensions: "extensions", relays: "relays" };
/**
 * 文字有没有从盒子里长出去。
 *
 * ⚠ 原来这里只有一条 `.qb-main` 的 `scrollWidth > clientWidth` ——
 * 那是「整页会不会横向滚」。小窗是 `dialog.modal`，它 `overflow: hidden`：
 * 里面的字溢出去既不滚也不报，**直接被裁掉**，整页宽度一点没变。
 * 于是 v0.12.1 的实机截图里「执行锁」那一格的字整段横着压在隔壁格上，
 * 而这条测试全绿。
 *
 * 两档一起查：
 *   live   —— 按演示数据现在的样子，谁已经顶出了自己的盒子；
 *   stress —— 把每个叶子文本节点轮流换成 48 个不可断开的字符
 *             （就是 `config_io::id()` 产出的那种会话 id），谁先被撑破。
 *             实机上撑破界面的从来不是演示数据，是真实的路径、
 *             环境变量名和一串会话 id。
 */
const OVERFLOW_PROBE = `
(() => {
  const where = (el) => {
    const parts = [];
    for (let e = el; e && e.tagName && parts.length < 4; e = e.parentElement) {
      let s = e.tagName.toLowerCase();
      const cn = typeof e.className === "string" ? e.className.trim() : "";
      if (cn) s += "." + cn.split(/\\s+/).slice(0, 3).join(".");
      parts.unshift(s);
    }
    return parts.join(" > ");
  };
  const root =
    document.querySelector("dialog[open]") ||
    document.querySelector(".qb-main") ||
    document.body;

  const live = [];
  for (const el of root.querySelectorAll("*")) {
    const cs = getComputedStyle(el);
    if (cs.display === "none" || cs.visibility === "hidden") continue;
    if (el.closest(".sr-only")) continue;
    const r = el.getBoundingClientRect();
    if (!r.width && !r.height) continue;
    const p = el.parentElement;
    if (!p) continue;
    // 负外边距是故意让盒子探出去的（.metachip 的点击热区），不算。
    if ((parseFloat(cs.marginRight) || 0) < 0) continue;
    if (cs.position === "absolute" || cs.position === "fixed") continue;
    const pcs = getComputedStyle(p);
    if (pcs.overflowX !== "visible") continue;
    const pr = p.getBoundingClientRect();
    const innerR =
      pr.right -
      (parseFloat(pcs.borderRightWidth) || 0) -
      (parseFloat(pcs.paddingRight) || 0);
    const innerL =
      pr.left +
      (parseFloat(pcs.borderLeftWidth) || 0) +
      (parseFloat(pcs.paddingLeft) || 0);
    const outBy = Math.max(r.right - innerR, innerL - r.left);
    if (outBy > 2)
      live.push(where(el) + " 溢出 " + Math.round(outBy) + "px：" + (el.textContent || "").trim().slice(0, 40));
  }

  const TOKEN = "20260913-045632-ef2e6a0958ef48deb76f0b4d12ca77b1";
  const stress = [];
  const w = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const nodes = [];
  let n;
  while ((n = w.nextNode())) {
    if (!n.data.trim()) continue;
    const host = n.parentElement;
    if (!host || host.closest("script,style,svg,.sr-only")) continue;
    nodes.push(n);
  }
  for (const node of nodes) {
    const old = node.data;
    const host = node.parentElement;
    // 只看装机器给的值的那几种盒子。按钮、标签、分数格里放的都是写死的
    // 文案（「已装」「去检测」「权重」），长不了 —— 拿长 token 去撑它们
    // 只会撑出一堆假警报，把真问题埋掉。
    if (host.closest(".pill,.btn,.scorecell-head,.scorecell-value")) continue;
    const box = host.closest(".metric,.row,.bullet");
    if (!box) continue;
    node.data = TOKEN;
    const over = box.scrollWidth - box.clientWidth;
    node.data = old;
    if (over > 2)
      stress.push(where(host) + " 撑破 " + box.className.split(/\\s+/)[0] + " " + Math.round(over) + "px（原文：" + old.trim().slice(0, 24) + "）");
  }
  return { live: [...new Set(live)], stress: [...new Set(stress)] };
})()
`;

/** 查一处，有问题就把具体是哪个元素、溢出多少 px 写进断言消息。 */
async function assertNoOverflow(page, scope) {
  const { live, stress } = await page.evaluate(OVERFLOW_PROBE);
  assert.deepEqual(live, [], `${scope} 有文字顶出了盒子`);
  assert.deepEqual(stress, [], `${scope} 装不下长字符串`);
}
const port = 20000 + Math.floor(Math.random() * 20000);
const origin = `http://localhost:${port}`;
const server = spawn(
  process.execPath,
  [
    "node_modules/vite/bin/vite.js",
    "--mode",
    "demo",
    "--port",
    String(port),
    "--strictPort",
  ],
  { cwd: resolve("."), windowsHide: true, stdio: "ignore" },
);
let browser;
try {
  for (let i = 0; i < 60; i++) {
    if (server.exitCode !== null)
      throw new Error("Demo server failed to start on its own port");
    try {
      if ((await fetch(origin)).ok) break;
    } catch {}
    await new Promise((r) => setTimeout(r, 250));
  }
  const executablePath =
    process.env.QB_SHOT_BROWSER ||
    [
      "C:/Program Files/Google/Chrome/Application/chrome.exe",
      "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
    ].find(existsSync);
  browser = await chromium.launch({
    headless: true,
    ...(executablePath ? { executablePath } : {}),
  });
  const context = await browser.newContext({
    viewport: { width: 1180, height: 820 },
    colorScheme: "light",
  });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await context.route("**/*", (route) =>
    route.request().url().startsWith(origin) ||
    route.request().url().startsWith("data:")
      ? route.continue()
      : route.abort(),
  );
  const report = [];
  mkdirSync("docs/screenshots", { recursive: true });
  for (const theme of ["light", "dark"]) {
    await page.goto(origin, { waitUntil: "domcontentloaded" });
    await page.evaluate(
      (theme) => localStorage.setItem("qb-theme", theme),
      theme,
    );
    for (const width of [1180, 944, 786, 680]) {
      await page.setViewportSize({ width, height: 820 });
      for (const route of ROUTES) {
        await page.goto(origin + "/#/" + route, {
          waitUntil: "domcontentloaded",
        });
        await page.getByText("演示数据", { exact: true }).waitFor();
        await page.locator(".qb-page h1").waitFor();
        await page.waitForTimeout(200);
        const overflow = await page
          .locator(".qb-main")
          .evaluate((e) => e.scrollWidth > e.clientWidth + 2);
        assert.equal(
          overflow,
          false,
          `${route} overflows at ${width} / ${theme}`,
        );
        await assertNoOverflow(page, `/${route} ${width}px ${theme}`);
        assert.equal(await page.getByText("此页面暂时无法显示").count(), 0);
        report.push({ route, width, theme, passed: true });
        if (width === 1180 && SHOT[route])
          await page.screenshot({
            path: `docs/screenshots/${SHOT[route]}-${theme}.png`,
          });
      }
    }
  }
  for (const scale of [1.25, 1.5]) {
    for (const theme of ["light", "dark"]) {
      const scaled = await browser.newContext({
        viewport: {
          width: Math.floor(1180 / scale),
          height: Math.floor(1020 / scale),
        },
        deviceScaleFactor: scale,
        colorScheme: theme,
      });
      await scaled.route("**/*", (route) =>
        route.request().url().startsWith(origin)
          ? route.continue()
          : route.abort(),
      );
      const view = await scaled.newPage();
      view.on("pageerror", (e) => errors.push(e.message));
      for (const route of ROUTES) {
        await view.goto(origin + "/#/" + route);
        await view.getByText("演示数据", { exact: true }).waitFor();
        await view.locator(".qb-page h1").waitFor();
        assert.equal(
          await view
            .locator(".qb-main")
            .evaluate((e) => e.scrollWidth > e.clientWidth + 2),
          false,
          `DPI ${scale}: ${route}`,
        );
        report.push({
          route,
          width: Math.floor(1180 / scale),
          theme,
          scale,
          passed: true,
        });
      }
      await scaled.close();
    }
  }
  await page.setViewportSize({ width: 1180, height: 820 });
  await page.goto(origin + "/#/extensions");
  await page.getByRole("heading", { name: "扩展中心", exact: true }).waitFor();
  assert.equal(
    await page.getByText("启动酒馆", { exact: true }).count(),
    0,
    "catalog must not mount the Tavern manager",
  );
  await page.keyboard.press("Control+k");
  await page
    .getByRole("textbox", { name: "搜索功能、账户、服务商或扩展" })
    .fill("Filesystem");
  await page.getByRole("button", { name: /Filesystem MCP/ }).click();
  await page
    .getByRole("heading", { name: "Filesystem MCP", exact: true })
    .waitFor();
  await page.goBack();
  assert.match(page.url(), /#\/extensions$/);
  await page.goto(origin + "/#/relays/new");
  await page
    .getByLabel("名称", { exact: true })
    .fill("Draft survives navigation");
  await page.getByRole("link", { name: /官方账户/ }).click();
  await page.goBack();
  await page.getByLabel("名称", { exact: true }).waitFor();
  assert.equal(
    await page.getByLabel("名称", { exact: true }).inputValue(),
    "Draft survives navigation",
  );
  // 总览上的安全方块：点开是小窗，**不跳页**。使用者点名要的就是这个形态，
  // 所以这条要断言地址一个字都没变 —— 退化成跳页的话这里会红。
  await page.goto(origin + "/#/");
  await page.getByRole("button", { name: /DNS 泄露/ }).click();
  const sheet = page.locator("dialog[open]");
  await sheet.getByRole("heading", { name: "简易通过", exact: true }).waitFor();
  // 先在弹窗里点一下再按 Esc。**这一步不能省**：焦点落在会重渲染的按钮上时，
  // 挂在 <dialog> 上的 keydown 收不到 Esc（节点被换掉、焦点掉回 body），
  // 弹窗就关不掉了。只按 Esc 不点东西的话这个 bug 测不出来。
  await sheet.getByRole("button", { name: /开始检测（约 6 秒）/ }).click();
  await sheet.getByRole("button", { name: "重新检测" }).waitFor();
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("dialog[open]").count(), 0);
  assert.match(page.url(), /#\/$/);

  // 六个安全小窗，每个都查一遍文字有没有长出盒子。
  //
  // ⚠ 这几个窗必须单独查。`.modal` 是 `overflow: hidden`，里面溢出去的字
  // 既不滚也不改变整页宽度，上面那条 `.qb-main` 的断言一个都拦不住 ——
  // 实机上「执行锁」那一格的字压在隔壁格上的时候，这套测试全绿。
  const OPENERS = "button.scorecell, button.metachip";
  for (const width of [1180, 786]) {
    await page.setViewportSize({ width, height: 820 });
    await page.goto(origin + "/#/", { waitUntil: "domcontentloaded" });
    await page.locator(".qb-page h1").waitFor();
    await page.waitForTimeout(200);
    const count = await page.evaluate(
      (sel) => document.querySelectorAll(sel).length,
      OPENERS,
    );
    assert.ok(count >= 6, `总览上应该有六个能点开小窗的入口，实际 ${count}`);
    for (let i = 0; i < count; i++) {
      const name = await page.evaluate(
        ([sel, i]) => {
          const b = document.querySelectorAll(sel)[i];
          b.click();
          return b.textContent.trim().slice(0, 12);
        },
        [OPENERS, i],
      );
      await page.locator("dialog[open]").waitFor();
      // 折叠块里也是正文，展开了才查得到。
      await page.evaluate(() =>
        document.querySelectorAll("dialog[open] details").forEach((d) => {
          d.open = true;
        }),
      );
      await page.waitForTimeout(200);
      await assertNoOverflow(page, `小窗「${name}」 ${width}px`);
      report.push({
        route: `modal:${name}`,
        width,
        theme: "light",
        passed: true,
      });
      // 关窗走 Esc，**不要**直接调 `dialog.close()`。
      // 那样关掉的只是 DOM 节点，React 那边 `open` 还是 true，
      // 下一个入口点下去什么都不会发生，报出来的是「等不到 dialog」。
      await page.keyboard.press("Escape");
      await page.locator("dialog[open]").waitFor({ state: "detached" });
    }
  }
  await page.setViewportSize({ width: 1180, height: 820 });

  await page.goto(origin + "/#/relays");
  await page.getByRole("tab", { name: /API 凭证/ }).click();
  await page.getByRole("button", { name: "添加凭证" }).click();
  await page
    .getByLabel("API Key", { exact: true })
    .fill("synthetic-unsubmitted-key");
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("dialog[open]").count(), 0);
  assert.deepEqual(errors, []);
  writeFileSync(
    "docs/ui-regression.json",
    JSON.stringify(
      {
        cases: report,
        flows: [
          "catalog isolation",
          "global search and browser back",
          "draft navigation persistence",
          "keyboard modal close",
          "overview security modal",
        ],
        pageErrors: errors,
      },
      null,
      2,
    ),
  );
  console.log(
    `${report.length} responsive/theme routes and 5 interaction flows passed.`,
  );
} finally {
  if (browser) await browser.close();
  server.kill();
}
