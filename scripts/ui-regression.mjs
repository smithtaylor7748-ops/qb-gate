import { chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import assert from "node:assert/strict";
// 五个入口加一条详情页。`""` 是根路径（官方账户，同时是仪表盘）。
// 前五条必须跟 src/lib/routes.ts 的 NAV 对得上，否则 CI 直接红。
// 多带一条**详情页**：`extensions` 只覆盖列表，酒馆那一整块面板
// （路径设置、依赖检查、资产、备份）挂在 `extensions/sillytavern` 下，
// 一直没被这套断言碰过。它恰恰是最容易横向撑爆的一页 ——
// 依赖检查那几行要完整显示绝对路径，680 宽下不许溢出。
const ROUTES = [
  "",
  "relays",
  "subscription",
  "software",
  "extensions",
  "extensions/sillytavern",
  "settings",
];
// 截图沿用旧文件名 —— README.md 引着 workspace-light.png / extensions-light.png。
// 详情页不入册：README 不引它，多拍一张只会多一个要维护的二进制。
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
  // A rotating loading ring has a changing bounding box; inspect the settled layout.
  await page.waitForFunction(
    () =>
      !Array.from(document.querySelectorAll(".btn-spinner")).some(
        (el) => el.getBoundingClientRect().width > 0,
      ),
  );
  const { live, stress } = await page.evaluate(OVERFLOW_PROBE);
  assert.deepEqual(live, [], `${scope} 有文字顶出了盒子`);
  assert.deepEqual(stress, [], `${scope} 装不下长字符串`);
}
async function assertFixedAccounts(page, scope) {
  const geometry = await page.evaluate(() => {
    const left = document
      .querySelector(".account-slots")
      .getBoundingClientRect();
    const right = document
      .querySelector(".account-workspace-right")
      .getBoundingClientRect();
    const overflow = [];
    for (const el of document.querySelectorAll(
      ".official-page, .account-workspace, .account-workspace section, .account-slot-list, .account-usage .ustats",
    )) {
      if (el.closest("dialog")) continue;
      if (
        el.scrollHeight > el.clientHeight + 2 ||
        el.scrollWidth > el.clientWidth + 2
      )
        overflow.push(el.className);
    }
    for (const el of document.querySelectorAll(".account-workspace *")) {
      if (el.closest("dialog, .sr-only") || !el.checkVisibility()) continue;
      const style = getComputedStyle(el);
      if (["absolute", "fixed"].includes(style.position)) continue;
      const rect = el.getBoundingClientRect();
      const parent = el.parentElement.getBoundingClientRect();
      if (
        rect.height &&
        (rect.bottom > innerHeight + 2 || rect.bottom > parent.bottom + 2)
      )
        overflow.push(el.className);
      if (
        el.matches(".notice, .tile-note") &&
        el.clientHeight > 0 &&
        el.scrollHeight > el.clientHeight + 2
      )
        overflow.push(`clipped text: ${el.className}`);
    }
    return {
      widthDifference: Math.abs(left.width - right.width),
      heightDifference: Math.abs(left.height - right.height),
      aligned: Math.abs(left.y - right.y),
      overflow,
    };
  });
  assert.ok(
    geometry.widthDifference < 2 &&
      geometry.heightDifference < 2 &&
      geometry.aligned < 2,
    `${scope}: columns must match ${JSON.stringify(geometry)}`,
  );
  assert.deepEqual(
    geometry.overflow,
    [],
    `${scope}: fixed page must fit content without clipping`,
  );
  await page.locator(".account-workspace").hover();
  // Edge's elastic overscroll can move the painted viewport while every DOM
  // scrollTop remains zero. Compare a static part of the sidebar as well.
  const screenshotOptions = {
    clip: { x: 0, y: 0, width: 72, height: 120 },
    animations: "disabled",
  };
  const beforeWheel = await page.screenshot(screenshotOptions);
  await page.mouse.wheel(0, 700);
  await page.waitForTimeout(80);
  assert.ok(
    beforeWheel.equals(await page.screenshot(screenshotOptions)),
    `${scope}: the painted page must not bounce under the wheel`,
  );
  assert.equal(
    await page.locator(".qb-main").evaluate((el) => el.scrollTop),
    0,
    `${scope}: wheel must not move the page`,
  );
  const shifted = await page.evaluate(() =>
    [
      ...document.querySelectorAll(
        "html, body, #root, .qb-shell, .qb-content, .qb-main",
      ),
    ]
      .filter((el) => el.scrollTop !== 0 || el.getBoundingClientRect().top < -1)
      .map((el) => ({
        name: el.className || el.tagName,
        scroll: el.scrollTop,
        y: el.getBoundingClientRect().top,
      })),
  );
  assert.deepEqual(
    shifted,
    [],
    `${scope}: focus and wheel must not shift any outer container`,
  );
  await assertNoOverflow(page, scope);
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
  const waitFor = async (path, tries, what) => {
    for (let i = 0; i < tries; i++) {
      if (server.exitCode !== null)
        throw new Error("Demo server failed to start on its own port");
      try {
        if ((await fetch(origin + path)).ok) return;
      } catch {}
      await new Promise((r) => setTimeout(r, 250));
    }
    throw new Error(
      `等不到${what}（${path}）——demo 服务器起来了但没能开始服务`,
    );
  };
  await waitFor("", 60, "首页");

  // ⚠ 首页回 200 **不代表可以开始测**。
  //
  // vite 会按住所有模块请求,直到依赖预打包跑完;冷缓存下那一步在开发机上要
  // 40–60 秒。只等首页的话,第一发 `page.goto` 会卡在 DOMContentLoaded 上 ——
  // 文档已经 commit、readyState 停在 interactive,而 deferred 的 module script
  // 永远没回来,30 秒后报成「page.goto: Timeout」。
  //
  // 那个报错**看起来像浏览器坏了**:服务器 curl 得通、单独开个浏览器也打得开
  // (因为那时缓存已经热了)。实际只是还没热身完。**新克隆的仓库第一次跑
  // `npm run test:ui` 必然撞上这个**,所以这里等的是「模块真的服务得出来」。
  await waitFor("/src/main.tsx", 600, "依赖预打包");
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
    // ⚠ 这里必须真的重载一次。下面那圈 `goto(origin + "/#/" + route)` 是从 origin
    // 出发只改 fragment —— 同文档导航，不重新加载。主题是在模块加载时从
    // localStorage 读一次定下来的，不 reload 的话整轮拍到的都是上一轮的主题：
    // light 轮拍成「跟随系统」，dark 轮拍成「浅色」，`docs/screenshots/*-dark.png`
    // 因此一直是浅色的，而 72 条断言全绿 —— 深色从来没被真正渲染过。
    await page.reload({ waitUntil: "domcontentloaded" });
    for (const width of [1180, 944, 786, 680]) {
      await page.setViewportSize({ width, height: 820 });
      for (const route of ROUTES) {
        await page.goto(origin + "/#/" + route, {
          waitUntil: "domcontentloaded",
        });
        await page.getByText("演示数据", { exact: true }).waitFor();
        await page.locator(".qb-page h1, .qb-page .qb-st-ctl").waitFor();
        await page.waitForTimeout(200);
        const overflow = await page
          .locator(".qb-main")
          .evaluate((e) => e.scrollWidth > e.clientWidth + 2);
        assert.equal(
          overflow,
          false,
          `${route} overflows at ${width} / ${theme}`,
        );
        // ⛔ 文档本身一格都不许滚。外壳是 `height: 100dvh; overflow: hidden`，
        // 会滚的只该是 `.qb-main` 一个 —— 文档也能滚就意味着窗口右边出现了
        // **第二根滚动条**，而且多出来的那一段全是空白。
        //
        // 这一条是补窟窿：上面那句只查 `.qb-main` 横向。0.19.0 实测漏掉了
        // 软件页 +1089px、设置页 +257px，原因是 `ExternalLink` 里那个
        // `.sr-only` 绝对定位后认到了 `<body>`，整个逃出滚动容器
        // （修法见 `workspace.css` 的 `.qb-main { position: relative }`）。
        assert.equal(
          await page.evaluate(
            () =>
              document.documentElement.scrollHeight -
              document.documentElement.clientHeight,
          ),
          0,
          `${route} 在 ${width} / ${theme} 下文档自己也能滚 —— 窗口会多出第二根滚动条`,
        );
        await assertNoOverflow(page, `/${route} ${width}px ${theme}`);
        assert.equal(await page.getByText("此页面暂时无法显示").count(), 0);
        report.push({ route, width, theme, passed: true });
        if (width === 1180 && SHOT[route]) {
          // 截图落盘前先确认主题真的是这一轮的那个，否则文件名会撒谎。
          assert.equal(
            await page.evaluate(() => document.documentElement.dataset.theme),
            theme,
            `${SHOT[route]}-${theme}.png 落盘时页面主题其实不是 ${theme}`,
          );
          await page.screenshot({
            path: `docs/screenshots/${SHOT[route]}-${theme}.png`,
          });
        }
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
        await view.locator(".qb-page h1, .qb-page .qb-st-ctl").waitFor();
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
    await page.locator(".qb-page h1, .qb-page .qb-st-ctl").waitFor();
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

  // 中转站页**没有外壳分页栏**了（0.16.0）：它就是一页。
  // 「供应商 / 凭证 / 环境」那一套只有深链接进得去 —— 走 RELAY_LEGACY
  // Relay launch/close follows the actual workspace session, not router uptime.
  await page.goto(origin + "/#/relays");
  await page.locator(".qb-st-ctl").waitFor();
  assert.equal(await page.locator(".qb-page-heading h1").count(), 0);
  assert.equal(await page.locator(".qb-st-nudge").count(), 0);
  const relayButton = page.locator(".qb-st-launchbtn");
  await relayButton.click();
  await page.getByRole("button", { name: /一键关闭 Claude Code/ }).waitFor();
  await page.getByRole("tab", { name: /Codex 桌面端/ }).click();
  assert.equal(
    await page.getByRole("button", { name: /一键关闭 Codex/ }).count(),
    0,
  );
  await page.getByRole("tab", { name: /Claude Code/ }).click();
  await page.getByRole("button", { name: /一键关闭 Claude Code/ }).click();
  await page.getByRole("button", { name: /^启动 Claude Code/ }).waitFor();

  // 那个不指定服务商的入口（/relays/new 落到的是「添加服务商」编辑器，
  // 那上面没有「API 凭证」分页）。
  await page.goto(origin + "/#/relays/providers");
  await page.getByRole("tab", { name: /API 凭证/ }).click();
  await page.getByRole("button", { name: "添加凭证" }).click();
  await page
    .getByLabel("API Key", { exact: true })
    .fill("synthetic-unsubmitted-key");
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("dialog[open]").count(), 0);

  // 软件页「Google Chrome」那张卡的隐私审计。
  //
  // ⚠ 那六行**只有扫过之后才渲染** —— `traces` / `browserAudit` 都是
  // `auto: false`，而上面那 72 轮从头到尾没点过任何按钮。于是 0.19.0 里
  // 「每一行往右吐 57px、压在隔壁『卸载』栏正文上」这种一眼可见的回归，
  // 整套测试照样全绿（成因见 `Environment.tsx` 里「这里不许用 grid」那段）。
  // 演示数据里的 `browser_audit` 就是为这条补的，值特意写得够长。
  for (const width of [1180, 786]) {
    await page.setViewportSize({ width, height: 820 });
    await page.goto(origin + "/#/software", { waitUntil: "domcontentloaded" });
    await page.locator(".qb-page h1, .qb-page .qb-st-ctl").waitFor();
    const chromeCard = page.locator(".card").filter({
      has: page.getByRole("heading", { name: "Google Chrome", exact: true }),
    });
    await chromeCard.getByRole("button", { name: "扫描", exact: true }).click();
    await chromeCard.getByText("扩展权限", { exact: true }).waitFor();
    // 折叠起来的「看看是哪几个扩展」里也是正文，展开了才查得到。
    await chromeCard.locator("details").evaluateAll((list) =>
      list.forEach((d) => {
        d.open = true;
      }),
    );
    await page.waitForTimeout(200);
    await assertNoOverflow(page, `软件页 Chrome 隐私审计 ${width}px`);
    report.push({
      route: "software:chrome-audit",
      width,
      theme: "light",
      passed: true,
    });
  }
  await page.setViewportSize({ width: 1180, height: 820 });

  // 「跟随系统」必须整套落到浅色或深色之一，手选主题也不许受系统影响。
  //
  // ⚠ 上面那一大轮先写了 `qb-theme` 才截图，拍的全是手选主题；DPI 那轮倒是
  // 跟随系统，却只查宽度。v0.13.1 之前 tokens.css 与 workspace.css 各有一套色值，
  // 默认的「跟随系统」两边都不完整命中 —— 系统浅色时边框、状态色来自一套，
  // 背景、正文来自另一套 —— 而这套测试全绿。所以这里逐个令牌比，不靠截图。
  const THEME_TOKENS = [
    "--bg",
    "--surface",
    "--surface-2",
    "--border",
    "--border-strong",
    "--text",
    "--text-2",
    "--text-3",
    "--accent",
    "--accent-bg",
    "--accent-border",
    "--ok",
    "--ok-bg",
    "--ok-border",
    "--warn",
    "--warn-bg",
    "--warn-border",
    "--danger",
    "--danger-bg",
    "--danger-border",
  ];
  const palettes = {};
  for (const scheme of ["light", "dark"]) {
    const themed = await browser.newContext({
      viewport: { width: 1180, height: 820 },
      colorScheme: scheme,
    });
    await themed.route("**/*", (route) =>
      route.request().url().startsWith(origin)
        ? route.continue()
        : route.abort(),
    );
    const view = await themed.newPage();
    view.on("pageerror", (e) => errors.push(e.message));
    for (const theme of ["system", "light", "dark"]) {
      await view.goto(origin, { waitUntil: "domcontentloaded" });
      await view.evaluate((t) => localStorage.setItem("qb-theme", t), theme);
      await view.reload({ waitUntil: "domcontentloaded" });
      await view.locator(".qb-page h1, .qb-page .qb-st-ctl").waitFor();
      palettes[`${scheme}/${theme}`] = await view.evaluate((names) => {
        const style = getComputedStyle(document.documentElement);
        return Object.fromEntries([
          ["color-scheme", style.colorScheme],
          ...names.map((n) => [n, style.getPropertyValue(n).trim()]),
        ]);
      }, THEME_TOKENS);
    }
    await themed.close();
  }
  assert.deepEqual(
    palettes["light/system"],
    palettes["light/light"],
    "跟随系统 + 系统浅色，必须与手选浅色逐个令牌一致",
  );
  assert.deepEqual(
    palettes["dark/system"],
    palettes["dark/dark"],
    "跟随系统 + 系统深色，必须与手选深色逐个令牌一致",
  );
  assert.deepEqual(
    palettes["dark/light"],
    palettes["light/light"],
    "手选浅色不许受系统深色影响",
  );
  assert.deepEqual(
    palettes["light/dark"],
    palettes["dark/dark"],
    "手选深色不许受系统浅色影响",
  );
  assert.notDeepEqual(
    palettes["light/light"],
    palettes["dark/dark"],
    "浅色和深色读出来是同一套值",
  );

  // Desktop accounts and the new two-column workspace.
  await page.goto(origin + "/#/");
  await page.setViewportSize({ width: 1320, height: 940 });
  await page.evaluate(() => localStorage.setItem("qb-theme", "light"));
  await page.reload();
  await page.locator(".official-page").waitFor();
  assert.equal(await page.locator("html").getAttribute("data-theme"), "light");
  await page.getByRole("button", { name: "Codex", exact: true }).click();
  await page.getByTestId("codex-slot").first().waitFor();
  assert.equal(await page.getByTestId("codex-slot").count(), 4);
  await page
    .getByRole("button", { name: "Codex 第 2 页", exact: true })
    .click();
  assert.equal(await page.getByTestId("codex-slot").count(), 2);
  await page
    .getByRole("button", { name: "Codex 第 1 页", exact: true })
    .click();
  await assertNoOverflow(page, "Codex four-account desktop layout");
  await page.screenshot({ path: "docs/screenshots/codex-accounts-light.png" });
  // Closing requires confirmation and preserves the selected account and login.
  const closeCodex = page.getByRole("button", {
    name: "一键关闭",
    exact: true,
  });
  await closeCodex.click();
  assert.match(await page.getByRole("dialog").innerText(), /正在运行的任务/);
  await page.getByRole("button", { name: "取消", exact: true }).click();
  assert.equal(await closeCodex.isEnabled(), true);
  await closeCodex.click();
  await page.getByRole("button", { name: "确认关闭", exact: true }).click();
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  assert.equal(await closeCodex.isDisabled(), true);
  assert.match(await page.locator(".account-launch").innerText(), /未运行/);
  assert.match(
    await page.getByTestId("codex-slot").first().innerText(),
    /当前槽位/,
  );
  assert.match(
    await page.getByTestId("codex-slot").first().innerText(),
    /已登录/,
  );
  await page
    .getByTestId("codex-slot")
    .nth(1)
    .getByRole("button", { name: "切换", exact: true })
    .click();
  await page.getByRole("dialog").waitFor();
  assert.match(await page.getByRole("dialog").innerText(), /正在运行的任务/);
  await page.getByRole("button", { name: "取消", exact: true }).click();
  assert.match(
    await page.getByTestId("codex-slot").first().innerText(),
    /当前槽位/,
  );
  await page
    .getByTestId("codex-accounts")
    .getByRole("button", { name: "新建", exact: true })
    .click();
  await page.getByLabel("账户名称", { exact: true }).fill("测试登录账户");
  await page.getByRole("button", { name: "新建并登录", exact: true }).click();
  await page
    .getByRole("button", { name: "关闭旧窗口并打开", exact: true })
    .waitFor();
  await page.getByRole("button", { name: "取消", exact: true }).click();
  await page
    .getByRole("button", { name: "一键修复 / 明细", exact: true })
    .click();
  const repairDialog = page.getByRole("dialog");
  assert.match(await repairDialog.innerText(), /五项检测与修复/);
  assert.equal(
    await repairDialog
      .getByRole("button", { name: "详细处理", exact: true })
      .count(),
    5,
  );
  await repairDialog
    .getByRole("button", { name: "重新检测五项", exact: true })
    .click();
  await page.waitForFunction(
    () => !document.querySelector("dialog[open] .btn-spinner"),
    null,
    { timeout: 60000 },
  );
  await assertNoOverflow(page, "five-check repair review");
  await repairDialog
    .getByRole("button", { name: /一键修复 2 项并复检/ })
    .click();
  await page.waitForFunction(
    () => !document.querySelector("dialog[open] .btn-spinner"),
    null,
    { timeout: 60000 },
  );
  assert.match(await repairDialog.innerText(), /本次修复结果/);
  assert.match(await repairDialog.innerText(), /已重新检测 5\/5 项/);
  await page.screenshot({ path: "docs/screenshots/checkup-repair-light.png" });
  await page.keyboard.press("Escape");
  // A completed self-test is visible even if a legacy manual record has no IP.
  assert.match(
    await page.locator(".official-score").innerText(),
    /已测 5\/5 项/,
  );
  assert.match(await page.locator(".scorecell").first().innerText(), /已自测/);
  assert.match(
    await page.locator(".scorecell").first().innerText(),
    /IPPure.*住宅/,
  );
  assert.doesNotMatch(
    await page.locator(".official-page").innerText(),
    /还有 \d+ 项待检查/,
  );
  // The bought IP lookup must not replace the current exit used by the score/gate.
  const currentScore = await page.locator(".scorecell").first().innerText();
  await page.locator(".scorecell").first().click();
  const purityDialog = page.getByRole("dialog");
  const currentReadout = await purityDialog
    .getByTestId("current-ip-result")
    .innerText();
  assert.match(currentReadout, /住宅 IP/);
  await page.screenshot({ path: "docs/screenshots/ip-current-light.png" });
  assert.equal(
    await purityDialog.locator(".modal-head .ipv6-switch").count(),
    1,
  );
  assert.equal(
    await purityDialog.locator(".purity-probe .card-head input").count(),
    1,
  );
  assert.equal(
    await purityDialog
      .getByText("为什么面板不直接给结论？", { exact: true })
      .count(),
    0,
  );
  assert.equal(
    await purityDialog
      .getByRole("link", { name: /IPRoyal 选购/ })
      .getAttribute("href"),
    "https://iproyal.cn/?r=sulianyan",
  );
  await purityDialog.getByLabel("新 IP 地址", { exact: true }).fill("8.8.8.8");
  await purityDialog
    .getByRole("button", { name: "查询指定 IP", exact: true })
    .click();
  await purityDialog.getByTestId("ip-lookup-result").waitFor();
  assert.match(
    await purityDialog.getByTestId("ip-lookup-result").innerText(),
    /8\.8\.8\.8/,
  );
  assert.match(
    await purityDialog.getByTestId("ip-lookup-result").innerText(),
    /IPQuery/,
  );
  assert.equal(
    await page.locator(".scorecell").first().innerText(),
    currentScore,
  );
  await assertNoOverflow(page, "candidate IP lookup");
  assert.equal(
    await purityDialog.getByTestId("current-ip-result").innerText(),
    currentReadout,
  );
  await purityDialog
    .getByRole("button", { name: "检测本机 IP", exact: true })
    .click();
  await page.waitForFunction(
    () =>
      !document.querySelector('.purity-probe button[type="button"]')?.disabled,
  );
  assert.equal(
    await purityDialog.getByLabel("新 IP 地址", { exact: true }).inputValue(),
    "8.8.8.8",
  );
  assert.match(
    await purityDialog.getByTestId("current-ip-result").innerText(),
    /住宅 IP/,
  );
  await page.screenshot({ path: "docs/screenshots/ip-lookup-light.png" });
  await purityDialog
    .getByLabel("新 IP 地址", { exact: true })
    .fill("not-an-ip");
  await purityDialog
    .getByRole("button", { name: "查询指定 IP", exact: true })
    .click();
  await purityDialog.getByRole("alert").waitFor();
  assert.match(
    await purityDialog.getByRole("alert").innerText(),
    /有效的 IPv4 或 IPv6/,
  );
  assert.equal(await purityDialog.getByTestId("ip-lookup-result").count(), 0);
  await purityDialog.getByLabel("新 IP 地址", { exact: true }).fill("8.8.8.8");
  await purityDialog
    .getByRole("button", { name: "查询指定 IP", exact: true })
    .click();
  await purityDialog.getByTestId("ip-lookup-result").waitFor();
  const ipv6Switch = purityDialog.getByRole("switch", {
    name: "禁用本机 IPv6",
  });
  assert.equal(await ipv6Switch.isChecked(), true);
  assert.equal(
    await ipv6Switch.evaluate((el) => getComputedStyle(el).appearance),
    "none",
  );
  // 开关显示后端保存的意图，等待异步操作完成后再断言，不能要求点击瞬间翻转。
  await ipv6Switch.click();
  await purityDialog
    .getByText("1 张网卡仍启用 IPv6", { exact: true })
    .waitFor();
  assert.equal(await ipv6Switch.isChecked(), false);
  await purityDialog.getByText("状态与恢复", { exact: true }).click();
  await purityDialog.getByText("查看 2 张网卡的实际状态").click();
  assert.match(await purityDialog.innerText(), /WLAN（原本已禁用）：IPv6 禁用/);
  await ipv6Switch.click();
  await purityDialog.getByText("网卡 IPv6 已禁用", { exact: true }).waitFor();
  await purityDialog.getByText("状态与恢复", { exact: true }).click();
  await assertNoOverflow(page, "IPv6 switch and adapter restore");
  await page.screenshot({ path: "docs/screenshots/ipv6-light.png" });
  for (const width of [680, 900]) {
    await page.setViewportSize({ width, height: 740 });
    await assertNoOverflow(page, `inline IP controls ${width}px`);
    assert.ok(await ipv6Switch.isVisible());
  }
  await page.screenshot({ path: "docs/screenshots/ip-inline-compact.png" });
  await page.setViewportSize({ width: 1320, height: 940 });
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Claude", exact: true }).click();
  const right = await page.locator(".account-workspace-right").boundingBox();
  const left = await page.locator(".account-slots").boundingBox();
  assert.ok(right.x > left.x && Math.abs(right.y - left.y) < 3);
  for (const [width, height] of [
    [1320, 940],
    [1180, 820],
    [900, 740],
    [680, 640],
  ]) {
    await page.setViewportSize({ width, height });
    await assertFixedAccounts(page, `Claude ${width}x${height}`);
    await page.getByRole("button", { name: "Codex", exact: true }).click();
    await assertFixedAccounts(page, `Codex ${width}x${height}`);
    await page.getByRole("button", { name: "Claude", exact: true }).click();
  }
  await page.getByRole("button", { name: "统计说明", exact: true }).click();
  await page.getByRole("dialog").waitFor();
  await assertNoOverflow(page, "usage details in fixed account page");
  await page.keyboard.press("Escape");
  await page
    .getByRole("button", { name: "检查官方目录的 API 配置", exact: true })
    .click();
  await page.getByRole("dialog").waitFor();
  await page.keyboard.press("Escape");
  await page.setViewportSize({ width: 1320, height: 940 });
  await page.waitForFunction(() => !document.querySelector(".toast"));
  await assertFixedAccounts(page, "final account workspace");
  await page.screenshot({ path: "docs/screenshots/accounts-layout-light.png" });
  await page.goto(origin + "/#/relays");
  await page
    .getByRole("button", { name: "查套路", exact: true })
    .first()
    .click();
  await page.getByRole("dialog").waitFor();
  await page.getByLabel("检验模型").fill("fixture-model");
  await page.getByLabel("站点账号 / 邮箱").fill("demo-user");
  await page.getByLabel("站点密码").fill("fixture-password");
  await page
    .getByRole("button", { name: "登录并读取账单", exact: true })
    .click();
  await page.getByText("后台已连接，账单读取已验证。").waitFor();
  await page.getByLabel("本次临时 API Key").fill("fixture-only-key");
  mkdirSync("target/ui", { recursive: true });
  for (const width of [1320, 680]) {
    await page.setViewportSize({ width, height: 940 });
    await assertNoOverflow(page, `station audit ${width}`);
    await page.screenshot({ path: `target/ui/station-audit-${width}.png` });
  }
  await page.getByRole("button", { name: "开始检验", exact: true }).click();
  await page.getByRole("tab", { name: "基础报告", exact: true }).waitFor();
  await page.setViewportSize({ width: 1320, height: 940 });
  await page.screenshot({ path: "target/ui/station-audit-report.png" });
  await page.getByRole("tab", { name: "完整证据" }).click();
  await assertNoOverflow(page, "station audit evidence");
  await page.screenshot({ path: "target/ui/station-audit-evidence.png" });
  await page.getByText("本地账单分析", { exact: false }).click();
  await page.getByLabel("选择账单文件").setInputFiles({
    name: "fixture.json",
    mimeType: "application/json",
    buffer: Buffer.from(
      JSON.stringify([
        {
          model: "fixture",
          group: "demo",
          input_tokens: 1000,
          cached: 800,
          output: 5,
          cost: 0.01,
          currency: "USD",
        },
      ]),
    ),
  });
  await page.getByText("fixture.json · 1 条").waitFor();
  await page.keyboard.press("Escape");
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
          "system theme matches explicit theme",
          "desktop account pagination and safe login confirmation",
          "five-item diagnostics and repair review",
          "IPv6 default switch and per-adapter restore",
          "equal fixed account columns and wheel immobility",
          "station audit model entry and independent billing credentials",
        ],
        pageErrors: errors,
      },
      null,
      2,
    ),
  );
  console.log(
    `${report.length} responsive/theme routes and 11 interaction flows passed.`,
  );
} finally {
  if (browser) await browser.close();
  server.kill();
}
